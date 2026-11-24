//! What the game's own data says about a hero's effects: which ability uses
//! which particle system, readable ability names, and which other particle
//! systems use a file as a child. Read once per game archive (`game_index`).
//!
//! * Definitions: every `scripts/*.vdata_c` record (abilities, heroes,
//!   modifiers, …) with the particle files it names anywhere inside it and the
//!   other records it names (links). Hero records bind abilities to slots
//!   (`m_mapBoundAbilities`); a hero owns what its bound abilities reach
//!   through links (weapon sets, triggers, sub-abilities), except what another
//!   enabled hero also reaches without passing through a link into one of the
//!   hero's bound abilities (an ability that copies or names the hero's
//!   ultimate does not take it away) and without loading the hero's own model
//!   (a template definition bound to the same weapon). Such a record still
//!   counts for "Also used by".
//! * Liveness ("Also used by" lists only these): a record reached from an
//!   enabled hero of the roster (not `m_bDisabled`, model in the roster), or
//!   from an item the game sells (`EAbilityType_Item`, an `m_iItemTier`, not
//!   `m_bDisabled`). Ability records reached from neither (test items,
//!   disabled or in-development items, abilities of heroes the game switched
//!   off, unbound cosmetics) are left out and
//!   counted with their reason. Records of other definition files (modifiers,
//!   NPC units, …) that nothing live reaches are listed unless their name has
//!   the word `test`.
//! * Names: the game's English localisation files beside the archive
//!   (`resource/localization/…_english.txt`), optional; without them the
//!   internal names are made readable.
use crate::{CancellationToken, Result, compiled, fail, heroes, icon_catalog, sound_tags};
use deadlock_core::text::casefold;
use deadlock_resource::{Kind, Value};
use deadlock_vpk::Vpk;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::Path;
use std::sync::{Arc, Mutex};

/// The most records one hero's bound abilities may reach.
const MAX_REACH: usize = 4096;
/// Localisation files read for names (under `resource/localization/`).
pub const NAME_FILES: [&str; 3] = [
    "citadel_heroes/citadel_heroes_english.txt",
    "citadel_gc_mod_names/citadel_gc_mod_names_english.txt",
    "citadel_mods/citadel_mods_english.txt",
];
/// A record bound by at least this share of the enabled heroes is "every hero".
const EVERY_HERO_SHARE: f64 = 0.8;

// ---------------------------------------------------------------- names

/// Readable names from the game's localisation (`"key" "value"` pairs).
#[derive(Debug, Clone, Default)]
pub struct Names(BTreeMap<String, String>);
impl Names {
    /// Adds the pairs of one localisation file (KeyValues text). Comments
    /// (`//` outside quotes) are skipped; a key followed by `{` is a block.
    pub fn add_text(&mut self, text: &str) {
        let mut tokens: Vec<Option<String>> = vec![];
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '"' => {
                    let mut s = String::new();
                    while let Some(c) = chars.next() {
                        match c {
                            '\\' => match chars.next() {
                                Some('n') => s.push('\n'),
                                Some('t') => s.push('\t'),
                                Some(other) => s.push(other),
                                None => break,
                            },
                            '"' => break,
                            c => s.push(c),
                        }
                    }
                    tokens.push(Some(s));
                }
                '/' if chars.peek() == Some(&'/') => {
                    for c in chars.by_ref() {
                        if c == '\n' {
                            break;
                        }
                    }
                }
                '{' | '}' => tokens.push(None),
                _ => {}
            }
        }
        let mut i = 0;
        while i + 1 < tokens.len() {
            if let (Some(key), Some(value)) = (&tokens[i], &tokens[i + 1]) {
                self.0.entry(casefold(key)).or_insert_with(|| value.clone());
                i += 2;
            } else {
                i += 1;
            }
        }
    }
    /// The names beside a game archive (`<folder of pak01_dir.vpk>/resource/localization`).
    /// Missing or unreadable files leave the names empty.
    pub fn beside(archive: &Path) -> Self {
        let mut names = Self::default();
        let Some(folder) = archive.parent() else { return names };
        for file in NAME_FILES {
            if let Ok(text) = std::fs::read_to_string(folder.join("resource").join("localization").join(file)) {
                names.add_text(&text);
            }
        }
        names
    }
    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.get(&casefold(key)).map(String::as_str).filter(|s| !s.is_empty() && !s.starts_with('#'))
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// An internal record name made readable (`citadel_ability_hornet_snipe` →
/// "Hornet snipe"); `drop` are hero words to leave out.
pub fn readable_internal(name: &str, drop: &[Vec<String>]) -> String {
    let words = sound_tags::words(name);
    let words = sound_tags::strip_leading(words, drop, &["citadel", "ability", "abilities", "upgrade", "item", "modifier", "hero"]);
    sound_tags::readable(&words)
}

// ---------------------------------------------------------------- definitions

#[derive(Debug, Clone, Default)]
pub struct Record {
    pub name: String,
    /// The definition file's stem (`abilities`, `heroes`, `modifiers`, …).
    pub file: String,
    /// `m_eAbilityType` (abilities only).
    pub kind: String,
    pub disabled: bool,
    pub item_tier: bool,
    /// Particle files it names (casefolded compiled paths).
    pub particles: BTreeSet<String>,
    /// Models it names (casefolded `.vmdl` paths, no `_c`): summons, turrets, props.
    pub models: BTreeSet<String>,
    /// Those of `models` it names only as a blocker (`blocker_keys`): every key naming
    /// the model is a blocker key held by a modifier, or an invisible vision blocker.
    pub blocker_models: BTreeSet<String>,
    /// Other records it names.
    pub links: BTreeSet<String>,
}
#[derive(Debug, Clone)]
pub struct HeroDef {
    pub key: String,
    /// Casefolded `.vmdl` path (no `_c`).
    pub model: String,
    pub disabled: bool,
    /// Bound slot → ability record, in the definition's order.
    pub bound: Vec<(String, String)>,
}
#[derive(Debug, Clone, Default)]
pub struct Definitions {
    pub records: BTreeMap<String, Record>,
    pub heroes: Vec<HeroDef>,
}
fn flag(value: Option<&Value>) -> bool {
    match value.map(|v| &v.kind) {
        Some(Kind::Bool(b)) => *b,
        Some(Kind::I64(n)) => *n != 0,
        Some(Kind::U64(n)) => *n != 0,
        Some(Kind::String(s)) => s == "1" || s.eq_ignore_ascii_case("true"),
        _ => false,
    }
}
fn particle_path(s: &str) -> Option<String> {
    let folded = casefold(s);
    (folded.ends_with(".vpcf") || folded.ends_with(".vpcf_c")).then(|| compiled(s).ok().map(|p| casefold(&p))).flatten()
}
fn strings<'a>(value: &'a Value, out: &mut Vec<&'a str>) {
    match &value.kind {
        Kind::String(s) => out.push(s),
        Kind::Object(rows) => rows.iter().for_each(|(_, v)| strings(v, out)),
        Kind::Array(rows) => rows.iter().for_each(|v| strings(v, out)),
        _ => {}
    }
}
fn model_key(path: &str) -> String {
    casefold(path).trim_end_matches("_c").to_owned()
}
/// The definition keys that name a model which only blocks: players and shots
/// (`m_BlockerModel`, `m_SolidBlockerModel`) or sight (`m_VisionBlockerModel`).
pub const BLOCKER_KEYS: [&str; 3] = ["m_BlockerModel", "m_SolidBlockerModel", "m_VisionBlockerModel"];
/// Every model string in `value` with whether that naming is a blocker: a
/// `BLOCKER_KEYS` key held by a modifier (an object whose `_class` starts with
/// `modifier_`), or an `m_VisionBlockerModel` whose object sets
/// `m_bVisionBlockerIsInvisible`, whatever holds it.
fn model_namings(value: &Value, key: Option<&str>, holder: Option<&Value>, out: &mut Vec<(String, bool)>) {
    match &value.kind {
        Kind::String(s) => {
            let folded = casefold(s);
            if folded.ends_with(".vmdl") || folded.ends_with(".vmdl_c") {
                let blocker = key.is_some_and(|k| BLOCKER_KEYS.contains(&k))
                    && holder.is_some_and(|h| {
                        let modifier = h.get("_class").and_then(|c| c.string().ok()).is_some_and(|c| c.starts_with("modifier_"));
                        modifier || (key == Some("m_VisionBlockerModel") && flag(h.get("m_bVisionBlockerIsInvisible")))
                    });
                out.push((model_key(&folded), blocker));
            }
        }
        Kind::Object(rows) => rows.iter().for_each(|(k, v)| model_namings(v, Some(k), Some(value), out)),
        Kind::Array(rows) => rows.iter().for_each(|v| model_namings(v, key, holder, out)),
        _ => {}
    }
}
/// The models of a record named only as blockers (`Record::blocker_models`).
fn blocker_models(record: &Value) -> BTreeSet<String> {
    let mut namings = vec![];
    model_namings(record, None, None, &mut namings);
    let other: BTreeSet<&String> = namings.iter().filter(|(_, b)| !b).map(|(m, _)| m).collect();
    namings.iter().filter(|(m, b)| *b && !other.contains(m)).map(|(m, _)| m.clone()).collect()
}
impl Definitions {
    /// From decoded definition documents: `(file stem, DATA root)` pairs; the
    /// `heroes` document gives the hero records.
    pub fn from_documents(documents: &[(String, Value)]) -> Self {
        let mut out = Self::default();
        for (file, doc) in documents {
            let Ok(rows) = doc.object() else { continue };
            for (name, record) in rows {
                if !matches!(record.kind, Kind::Object(_)) || out.records.contains_key(name) {
                    continue;
                }
                let mut found = vec![];
                strings(record, &mut found);
                let particles = found.iter().filter_map(|s| particle_path(s)).collect();
                let models = found.iter().map(|s| casefold(s)).filter(|s| s.ends_with(".vmdl") || s.ends_with(".vmdl_c")).map(|s| model_key(&s)).collect();
                let links = found.iter().filter(|s| s.as_bytes().first().is_some_and(u8::is_ascii_alphabetic) && !s.contains('/')).map(|s| s.to_string()).collect();
                out.records.insert(
                    name.clone(),
                    Record {
                        name: name.clone(),
                        file: file.clone(),
                        kind: record.get("m_eAbilityType").and_then(|v| v.string().ok()).unwrap_or("").to_owned(),
                        disabled: flag(record.get("m_bDisabled")),
                        item_tier: record.get("m_iItemTier").and_then(|v| v.string().ok()).is_some_and(|t| !t.is_empty()),
                        particles,
                        models,
                        blocker_models: blocker_models(record),
                        links,
                    },
                );
                if file == "heroes" {
                    let bound = record
                        .get("m_mapBoundAbilities")
                        .and_then(|v| v.object().ok())
                        .map(|rows| rows.iter().filter_map(|(slot, v)| Some((slot.clone(), v.string().ok()?.to_owned()))).collect())
                        .unwrap_or_default();
                    out.heroes.push(HeroDef {
                        key: name.clone(),
                        model: record.get("m_strModelName").and_then(|v| v.string().ok()).map(model_key).unwrap_or_default(),
                        disabled: flag(record.get("m_bDisabled")),
                        bound,
                    });
                }
            }
        }
        // Links only to records that exist.
        let names: BTreeSet<String> = out.records.keys().cloned().collect();
        for record in out.records.values_mut() {
            record.links.retain(|l| names.contains(l) && *l != record.name);
        }
        out
    }
    /// Every `scripts/*.vdata_c` of the archive; `None` without hero or
    /// ability definitions. Files that cannot be read are skipped.
    pub fn read(archive: &Vpk, cancel: &CancellationToken) -> Result<Option<Self>> {
        if !["scripts/heroes.vdata_c", "scripts/abilities.vdata_c"].iter().all(|p| archive.entries.contains_key(*p)) {
            return Ok(None);
        }
        let mut documents = vec![];
        let mut paths: Vec<&String> = archive.entries.keys().filter(|p| p.starts_with("scripts/") && p.ends_with(".vdata_c")).collect();
        // Heroes and abilities first, so their records win a name shared with another file.
        paths.sort_by_key(|p| (!p.ends_with("/heroes.vdata_c"), !p.ends_with("/abilities.vdata_c"), (*p).clone()));
        for path in paths {
            cancel.check()?;
            let stem = path.rsplit('/').next().unwrap_or_default().trim_end_matches(".vdata_c").to_owned();
            match crate::read(archive, path).and_then(|raw| icon_catalog::definition(&raw)) {
                Ok(doc) => documents.push((stem, doc)),
                Err(_) if stem != "heroes" && stem != "abilities" => continue,
                Err(e) => return Err(e),
            }
        }
        Ok(Some(Self::from_documents(&documents)))
    }
    fn reach(&self, starts: impl IntoIterator<Item = String>) -> BTreeSet<String> {
        self.reach_around(starts, &BTreeSet::new())
    }
    /// `reach`, never entering `blocked` through a link (a start is always reached).
    fn reach_around(&self, starts: impl IntoIterator<Item = String>, blocked: &BTreeSet<String>) -> BTreeSet<String> {
        let mut seen = BTreeSet::new();
        let mut queue: VecDeque<String> = starts.into_iter().collect();
        while let Some(name) = queue.pop_front() {
            if seen.len() >= MAX_REACH || !seen.insert(name.clone()) {
                continue;
            }
            if let Some(record) = self.records.get(&name) {
                queue.extend(record.links.iter().filter(|l| !blocked.contains(*l)).cloned());
            }
        }
        seen
    }
}

/// Who the game's definitions say uses a model or particle file: the enabled
/// roster heroes whose definition and bound abilities reach a record naming it,
/// and whether an item the game sells reaches it. For the installed-mods scan.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileUse {
    /// Hero names (roster display names).
    pub heroes: BTreeSet<String>,
    /// Reached by `EVERY_HERO_SHARE` of the enabled heroes (movement, melee, …).
    pub every_hero: bool,
    /// An item the game sells reaches it.
    pub item: bool,
}
impl Definitions {
    /// Casefolded compiled path (`….vmdl_c`, `….vpcf_c`) → its users.
    pub fn file_users(&self) -> Result<BTreeMap<String, FileUse>> {
        let roster = heroes::roster()?;
        let name_of = |model: &str| roster.iter().find(|h| model_key(&h.path) == model || h.extras.iter().any(|e| model_key(&e.path) == model)).map(|h| h.name.clone());
        let mut out: BTreeMap<String, FileUse> = BTreeMap::new();
        let files = |record: &Record| -> Vec<String> {
            record.particles.iter().cloned().chain(record.models.iter().map(|m| format!("{m}_c"))).collect()
        };
        let mut names = BTreeSet::new();
        for hero in self.heroes.iter().filter(|h| !h.disabled) {
            let Some(name) = name_of(&hero.model) else { continue };
            names.insert(name.clone());
            for record in self.reach(std::iter::once(hero.key.clone()).chain(hero.bound.iter().map(|(_, a)| a.clone()))) {
                if let Some(record) = self.records.get(&record) {
                    for file in files(record) {
                        out.entry(file).or_default().heroes.insert(name.clone());
                    }
                }
            }
        }
        for (name, record) in &self.records {
            if record.kind == "EAbilityType_Item" && record.item_tier && !record.disabled {
                for reached in self.reach([name.clone()]) {
                    if let Some(record) = self.records.get(&reached) {
                        for file in files(record) {
                            out.entry(file).or_default().item = true;
                        }
                    }
                }
            }
        }
        let every = (names.len() as f64 * EVERY_HERO_SHARE).max(3.);
        for usage in out.values_mut() {
            usage.every_hero = usage.heroes.len() as f64 >= every;
        }
        Ok(out)
    }
}

/// Order of a bound slot in the list (abilities 1–4 first).
pub fn slot_order(slot: &str) -> u32 {
    if let Some(n) = slot.strip_prefix("ESlot_Signature_").and_then(|n| n.parse::<u32>().ok()) {
        return 100 + n;
    }
    match slot {
        "ESlot_Weapon_Primary" => 200,
        "ESlot_Weapon_Secondary" => 201,
        "ESlot_Weapon_Melee" => 210,
        s if s.starts_with("ESlot_Ability_Innate") => 300,
        "hero" => 350,
        _ => 400,
    }
}

/// Who, besides the hero, uses a record.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UserKind {
    /// Abilities of other heroes (named).
    Hero,
    /// An ability every hero has (movement, melee, …).
    EveryHero,
    /// An item the game sells.
    Item,
    /// A record of another definition file (modifiers, NPC units, …).
    Other,
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
pub struct User {
    pub kind: UserKind,
    /// The record's internal name.
    pub record: String,
    /// Readable name (game localisation when it has one).
    pub name: String,
    /// Hero names (kind `hero`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub heroes: Vec<String>,
    /// The definition file (kind `other`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
}
impl User {
    pub fn text(&self) -> String {
        match self.kind {
            UserKind::Hero if !self.heroes.is_empty() => format!("{} ({})", self.name, self.heroes.join(", ")),
            UserKind::EveryHero => format!("{} (every hero)", self.name),
            UserKind::Item => format!("{} (item)", self.name),
            UserKind::Other => format!("{} (game {})", self.name, self.file.as_deref().unwrap_or("data").replace('_', " ")),
            _ => self.name.clone(),
        }
    }
}
/// A record that references the tree but is left out of "Also used by".
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
pub struct LeftOut {
    pub record: String,
    pub reason: &'static str,
}
pub const LEFT_DISABLED: &str = "disabled in the game's definitions";
pub const LEFT_HERO_OFF: &str = "bound only to heroes the game has switched off or that are not in the roster";
pub const LEFT_UNBOUND: &str = "not bound to a hero of the roster or sold as an item";
pub const LEFT_TEST: &str = "a test definition";

/// An own ability: the bound slot it hangs from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Own {
    pub slot: String,
    /// The bound ability (the slot's record).
    pub root: String,
    /// The record that names the file (the root or one it reaches).
    pub record: String,
}

/// The hero's own abilities and everyone else's, for one hero.
#[derive(Debug, Clone, Default)]
pub struct Usage {
    /// Own record → its bound slot and root (the first slot in list order).
    pub own: BTreeMap<String, (String, String)>,
    /// Particle file → own records naming it.
    pub own_files: BTreeMap<String, BTreeSet<String>>,
    /// Particle file → records of others naming it.
    pub users: BTreeMap<String, BTreeSet<String>>,
    /// Record → who uses it (live records only).
    pub user: BTreeMap<String, User>,
    /// Record → why it is left out.
    pub left: BTreeMap<String, &'static str>,
    /// Model (casefolded `.vmdl`, no `_c`) → own ability records naming it (not the hero record).
    pub own_models: BTreeMap<String, BTreeSet<String>>,
}

fn has_test_word(name: &str) -> bool {
    sound_tags::words(name).iter().any(|w| w == "test")
}

impl Usage {
    /// The usage for the hero model `model` (`.vmdl` path). Hero definitions
    /// loading the model own it; when several do, the ones whose key names the
    /// hero (`heroes::tokens`), as `icon_catalog` picks them.
    pub fn new(defs: &Definitions, model: &str, names: &Names) -> Result<Self> {
        let key = model_key(model);
        let tokens = heroes::tokens(model)?;
        let roster = heroes::roster()?;
        let in_roster = |m: &str| roster.iter().any(|h| model_key(&h.path) == m || h.extras.iter().any(|e| model_key(&e.path) == m));
        let exact: Vec<&HeroDef> = defs.heroes.iter().filter(|h| h.model == key).collect();
        let named: Vec<&HeroDef> = exact.iter().copied().filter(|h| tokens.contains(h.key.strip_prefix("hero_").unwrap_or(&h.key))).collect();
        // No definition loads the model (a roster hero's other model): the definitions named after the hero.
        let by_name = || defs.heroes.iter().filter(|h| tokens.contains(h.key.strip_prefix("hero_").unwrap_or(&h.key))).collect::<Vec<_>>();
        let own_defs: Vec<&HeroDef> = if !named.is_empty() { named } else if !exact.is_empty() { exact } else { by_name() };
        let own_keys: BTreeSet<&str> = own_defs.iter().map(|h| h.key.as_str()).collect();
        let enabled: Vec<&HeroDef> = defs.heroes.iter().filter(|h| !h.disabled && in_roster(&h.model) && !own_keys.contains(h.key.as_str())).collect();
        // Records each other enabled hero reaches (its definition included).
        let mut holders: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for hero in &enabled {
            let reach = defs.reach(std::iter::once(hero.key.clone()).chain(hero.bound.iter().map(|(_, a)| a.clone())));
            let name = heroes::display_name(&hero.model).unwrap_or_else(|| readable_internal(&hero.key, &[]));
            for record in reach {
                holders.entry(record).or_default().insert(name.clone());
            }
        }
        // Records only heroes the game switched off (or outside the roster) reach.
        let switched_off: BTreeSet<String> = defs
            .heroes
            .iter()
            .filter(|h| (h.disabled || !in_roster(&h.model)) && !own_keys.contains(h.key.as_str()))
            .flat_map(|h| defs.reach(h.bound.iter().map(|(_, a)| a.clone())))
            .collect();
        let hero_count = enabled.iter().filter_map(|h| heroes::display_name(&h.model)).collect::<BTreeSet<_>>().len().max(1);
        // Own records, by slot in list order; records another hero also reaches are not own.
        let mut slots: Vec<(String, String)> = own_defs.iter().flat_map(|h| h.bound.iter().cloned()).collect();
        slots.sort_by_key(|(slot, _)| slot_order(slot));
        // Who claims a record against the hero: other heroes' reach that does not pass through
        // a link into one of the hero's own bound abilities (another hero's ability naming the
        // hero's ultimate does not take it away; binding it does), and never a definition that
        // loads the hero's own model (a template bound to the hero's weapon). `holders` still
        // decides "Also used by".
        let bound: BTreeSet<String> = slots.iter().map(|(_, a)| a.clone()).collect();
        let mut claimed: BTreeSet<String> = BTreeSet::new();
        for hero in enabled.iter().filter(|h| h.model != key) {
            claimed.extend(defs.reach_around(std::iter::once(hero.key.clone()).chain(hero.bound.iter().map(|(_, a)| a.clone())), &bound));
        }
        let mut out = Self::default();
        let mut own_reach = BTreeSet::new();
        for def in &own_defs {
            own_reach.insert(def.key.clone());
            out.own.entry(def.key.clone()).or_insert_with(|| ("hero".into(), def.key.clone()));
        }
        for (slot, root) in &slots {
            for record in defs.reach([root.clone()]) {
                own_reach.insert(record.clone());
                if !claimed.contains(&record) {
                    out.own.entry(record).or_insert_with(|| (slot.clone(), root.clone()));
                }
            }
        }
        // Sold items and what they reach.
        let mut items: BTreeMap<String, String> = BTreeMap::new();
        for (name, record) in &defs.records {
            if record.kind == "EAbilityType_Item" && record.item_tier && !record.disabled {
                for reached in defs.reach([name.clone()]) {
                    items.entry(reached).or_insert_with(|| name.clone());
                }
            }
        }
        let display = |record: &str, fallback: &str| {
            names.get(record).or_else(|| names.get(fallback)).map(str::to_owned).unwrap_or_else(|| readable_internal(record, &[]))
        };
        for (name, record) in &defs.records {
            // An own record another hero still reaches is a user too, for the notes.
            let own = out.own.contains_key(name);
            let used = !own || holders.contains_key(name);
            if out.own.get(name).is_some_and(|(slot, _)| slot != "hero") {
                for m in &record.models {
                    out.own_models.entry(m.clone()).or_default().insert(name.clone());
                }
            }
            for file in &record.particles {
                if own {
                    out.own_files.entry(file.clone()).or_default().insert(name.clone());
                }
                if used {
                    out.users.entry(file.clone()).or_default().insert(name.clone());
                }
            }
            if !used || record.particles.is_empty() {
                continue;
            }
            if let Some(heroes) = holders.get(name) {
                let every = heroes.len() as f64 >= (hero_count as f64 * EVERY_HERO_SHARE).max(3.) || own_reach.contains(name) && heroes.len() + 1 >= hero_count;
                out.user.insert(
                    name.clone(),
                    User {
                        kind: if every { UserKind::EveryHero } else { UserKind::Hero },
                        record: name.clone(),
                        name: display(name, name),
                        heroes: if every { vec![] } else { heroes.iter().cloned().collect() },
                        file: None,
                    },
                );
            } else if let Some(item) = items.get(name) {
                out.user.insert(name.clone(), User { kind: UserKind::Item, record: name.clone(), name: display(name, item), heroes: vec![], file: None });
            } else if record.file != "abilities" && record.file != "heroes" && !has_test_word(name) {
                out.user.insert(
                    name.clone(),
                    User { kind: UserKind::Other, record: name.clone(), name: display(name, name), heroes: vec![], file: Some(record.file.clone()) },
                );
            } else {
                out.left.insert(
                    name.clone(),
                    if has_test_word(name) {
                        LEFT_TEST
                    } else if switched_off.contains(name) {
                        LEFT_HERO_OFF
                    } else if record.disabled {
                        LEFT_DISABLED
                    } else {
                        LEFT_UNBOUND
                    },
                );
            }
        }
        Ok(out)
    }
}

// ---------------------------------------------------------------- child index

/// Which particle systems name each file as a child (`m_Children[].m_ChildRef`).
#[derive(Debug, Clone, Default)]
pub struct ChildIndex {
    /// Casefolded child path → parent paths.
    parents: BTreeMap<String, Vec<String>>,
    pub files: usize,
    pub unreadable: usize,
}
impl ChildIndex {
    pub fn from_edges(edges: impl IntoIterator<Item = (String, String)>) -> Self {
        let mut out = Self::default();
        for (parent, child) in edges {
            out.parents.entry(casefold(&child)).or_default().push(parent);
        }
        out
    }
    pub fn parents(&self, child: &str) -> &[String] {
        self.parents.get(&casefold(child)).map(Vec::as_slice).unwrap_or(&[])
    }
    /// Every particle system of the archive, on several threads. Files whose
    /// resource references name no particle system are not decoded.
    pub fn scan(archive: &Vpk, cancel: &CancellationToken) -> Result<Self> {
        let paths: Vec<&String> = archive.entries.keys().filter(|p| p.starts_with("particles/") && p.ends_with(".vpcf_c")).collect();
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).clamp(1, 8);
        let chunk = paths.len().div_ceil(threads).max(1);
        let results: Vec<Result<(Vec<(String, String)>, usize)>> = std::thread::scope(|scope| {
            let workers: Vec<_> = paths
                .chunks(chunk)
                .map(|part| {
                    scope.spawn(move || -> Result<(Vec<(String, String)>, usize)> {
                        let (mut edges, mut unreadable) = (vec![], 0);
                        for (i, path) in part.iter().enumerate() {
                            if i % 64 == 0 {
                                cancel.check()?;
                            }
                            let Ok(raw) = archive.read(path) else {
                                unreadable += 1;
                                continue;
                            };
                            let refs = deadlock_vpk::resource_references(&raw).unwrap_or_default();
                            if !refs.iter().any(|r| casefold(r).contains(".vpcf")) {
                                continue;
                            }
                            match crate::effect_edit::document(&raw).and_then(|d| crate::effect_edit::children(&d.root)) {
                                Ok(children) => edges.extend(children.into_iter().map(|c| ((*path).clone(), c))),
                                Err(_) => unreadable += 1,
                            }
                        }
                        Ok((edges, unreadable))
                    })
                })
                .collect();
            workers.into_iter().map(|w| w.join().unwrap_or_else(|_| Err(fail("The particle scan stopped unexpectedly.")))).collect()
        });
        let mut edges = vec![];
        let mut unreadable = 0;
        for r in results {
            let (e, u) = r?;
            edges.extend(e);
            unreadable += u;
        }
        let mut out = Self::from_edges(edges);
        out.files = paths.len();
        out.unreadable = unreadable;
        for parents in out.parents.values_mut() {
            parents.sort();
            parents.dedup();
        }
        Ok(out)
    }
}

// ---------------------------------------------------------------- the game index

/// Everything read once per game archive.
#[derive(Debug)]
pub struct GameIndex {
    /// The archive and its directory file's size and modification time.
    pub key: String,
    pub definitions: Option<Definitions>,
    pub names: Names,
    pub children: ChildIndex,
    pub load_ms: f64,
}
/// The archive's identity: its path, size and modification time.
pub fn archive_key(archive: &Vpk) -> String {
    let stamp = std::fs::metadata(&archive.path)
        .ok()
        .map(|m| (m.len(), m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_millis())));
    format!("{}|{stamp:?}|{}", archive.path.display(), archive.entries.len())
}
static GAME: Mutex<Option<Arc<GameIndex>>> = Mutex::new(None);
/// The game index of `archive`, read once and kept until the archive changes
/// (another file, size or modification time).
pub fn game_index(archive: &Vpk, cancel: &CancellationToken) -> Result<Arc<GameIndex>> {
    let key = archive_key(archive);
    if let Some(index) = GAME.lock().unwrap_or_else(|e| e.into_inner()).as_ref().filter(|i| i.key == key) {
        return Ok(index.clone());
    }
    let started = std::time::Instant::now();
    let definitions = Definitions::read(archive, cancel)?;
    let children = ChildIndex::scan(archive, cancel)?;
    let names = Names::beside(&archive.path);
    let index = Arc::new(GameIndex { key, definitions, names, children, load_ms: started.elapsed().as_secs_f64() * 1000. });
    *GAME.lock().unwrap_or_else(|e| e.into_inner()) = Some(index.clone());
    Ok(index)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(t: &str) -> Value {
        Value::new(Kind::String(t.into()))
    }
    fn b(v: bool) -> Value {
        Value::new(Kind::Bool(v))
    }
    fn obj(rows: Vec<(&str, Value)>) -> Value {
        Value::new(Kind::Object(rows.into_iter().map(|(k, v)| (k.to_owned(), v)).collect()))
    }
    fn hero(model: &str, bound: &[(&str, &str)], disabled: Value) -> Value {
        obj(vec![
            ("m_strModelName", s(model)),
            ("m_bDisabled", disabled),
            ("m_mapBoundAbilities", obj(bound.iter().map(|(k, v)| (*k, s(v))).collect())),
        ])
    }
    fn ability(kind: &str, fields: Vec<(&str, Value)>) -> Value {
        let mut rows = vec![("m_eAbilityType", s(kind))];
        rows.extend(fields);
        obj(rows)
    }
    const BOMB: &str = "particles/abilities/ghost/ghost_blood_bomb_explode.vpcf";

    fn definitions() -> Definitions {
        let heroes = obj(vec![
            ("hero_ghost", hero("models/heroes_wip/geist/geist.vmdl", &[("ESlot_Signature_1", "ability_blood_bomb"), ("ESlot_Weapon_Primary", "citadel_weapon_ghost_set"), ("ESlot_Ability_Mantle", "citadel_ability_mantle")], b(false))),
            ("hero_bebop", hero("models/heroes_staging/bebop/bebop.vmdl", &[("ESlot_Signature_2", "ability_charged_bomb"), ("ESlot_Ability_Mantle", "citadel_ability_mantle")], b(false))),
            ("hero_haze", hero("models/heroes_staging/haze/haze.vmdl", &[("ESlot_Signature_1", "ability_sleep_dagger"), ("ESlot_Ability_Mantle", "citadel_ability_mantle")], b(false))),
            ("hero_astro", hero("models/heroes_staging/astro/astro.vmdl", &[("ESlot_Ability_Mantle", "citadel_ability_mantle")], b(false))),
            // Switched off with an integer flag: its abilities are not live.
            ("hero_kali", hero("models/heroes_wip/gen_man/gen_man.vmdl", &[("ESlot_Signature_1", "ability_kali_blade")], Value::new(Kind::I64(1)))),
        ]);
        let abilities = obj(vec![
            ("ability_blood_bomb", ability("EAbilityType_Signature", vec![("m_ExplodeParticle", s(BOMB)), ("m_SubAbility", s("ability_blood_bomb_trigger"))])),
            ("ability_blood_bomb_trigger", ability("EAbilityType_Signature", vec![("m_CastParticle", s("particles/abilities/ghost/trigger.vpcf"))])),
            ("citadel_weapon_ghost_set", ability("EAbilityType_Weapon", vec![("m_Tracer", s("particles/weapon_fx/ghost/ghost_tracer.vpcf"))])),
            ("citadel_ability_mantle", ability("EAbilityType_Innate", vec![("m_Dust", s("particles/generic/mantle.vpcf"))])),
            ("ability_charged_bomb", ability("EAbilityType_Signature", vec![("m_ExplodeParticle", s(BOMB))])),
            ("ability_kali_blade", ability("EAbilityType_Signature", vec![("m_ExplodeParticle", s(BOMB))])),
            ("upgrade_corpse_explosion", ability("EAbilityType_Item", vec![("m_iItemTier", s("EModTier_3")), ("m_bDisabled", b(true)), ("m_ExplodeParticle", s(BOMB))])),
            ("upgrade_soul_shredder", ability("EAbilityType_Item", vec![("m_iItemTier", s("EModTier_2")), ("m_ProcParticle", s(BOMB))])),
            ("item_projectile_test_01", ability("EAbilityType_Item", vec![("m_iItemTier", s("EModTier_1")), ("m_bDisabled", b(true)), ("m_particle", s(BOMB))])),
            ("cosmetic_ability_snowball", ability("EAbilityType_Cosmetic", vec![("m_particle", s(BOMB))])),
        ]);
        let modifiers = obj(vec![
            ("citadel_neutral_attack_lobfire", obj(vec![("m_ImpactParticle", s(BOMB))])),
            ("modifier_test_probe", obj(vec![("m_ImpactParticle", s(BOMB))])),
        ]);
        Definitions::from_documents(&[("heroes".into(), heroes), ("abilities".into(), abilities), ("modifiers".into(), modifiers)])
    }

    #[test]
    fn own_abilities_users_and_the_live_rule() {
        let defs = definitions();
        let mut names = Names::default();
        names.add_text("\"lang\"\n{\n \"Tokens\"\n {\n  \"ability_charged_bomb\"\t\"Kinetic Pulse\" // a comment \"x\" \"y\"\n  //\"ability_blood_bomb\" \"Hidden\"\n  \"ability_blood_bomb\" \"Essence \\\"Bomb\\\"\"\n }\n}\n");
        assert_eq!(names.get("ABILITY_CHARGED_BOMB"), Some("Kinetic Pulse"));
        assert_eq!(names.get("ability_blood_bomb"), Some("Essence \"Bomb\""), "commented lines are skipped");
        let usage = Usage::new(&defs, "models/heroes_wip/geist/geist.vmdl", &names).unwrap();
        // Own: the bound ability and the record it links to, the weapon; not the mantle every hero has.
        assert_eq!(usage.own.get("ability_blood_bomb_trigger"), Some(&("ESlot_Signature_1".to_owned(), "ability_blood_bomb".to_owned())));
        assert!(usage.own.contains_key("citadel_weapon_ghost_set"));
        assert!(!usage.own.contains_key("citadel_ability_mantle"));
        let bomb = casefold(&compiled(BOMB).unwrap());
        assert_eq!(usage.own_files[&bomb], BTreeSet::from(["ability_blood_bomb".to_owned()]));
        let users: Vec<String> = usage.users[&bomb].iter().filter_map(|r| usage.user.get(r)).map(User::text).collect();
        assert_eq!(users, ["Kinetic Pulse (Bebop)", "Neutral attack lobfire (game modifiers)", "Soul shredder (item)"]);
        let left: BTreeMap<&str, &str> = usage.left.iter().map(|(k, v)| (k.as_str(), *v)).collect();
        assert_eq!(
            left,
            BTreeMap::from([
                ("ability_kali_blade", LEFT_HERO_OFF),
                ("cosmetic_ability_snowball", LEFT_UNBOUND),
                ("item_projectile_test_01", LEFT_TEST),
                ("modifier_test_probe", LEFT_TEST),
                ("upgrade_corpse_explosion", LEFT_DISABLED),
            ])
        );
        // The mantle is every hero's, not a list of names.
        let mantle = casefold(&compiled("particles/generic/mantle.vpcf").unwrap());
        let user = &usage.user[usage.users[&mantle].iter().next().unwrap()];
        assert_eq!(user.kind, UserKind::EveryHero);
        assert_eq!(user.text(), "Mantle (every hero)");
        assert_eq!(slot_order("ESlot_Signature_4"), 104);
        assert!(slot_order("ESlot_Weapon_Primary") < slot_order("ESlot_Weapon_Melee"));
    }

    #[test]
    fn file_users_list_the_live_heroes_items_and_every_hero_files() {
        let users = definitions().file_users().unwrap();
        let bomb = casefold(&compiled(BOMB).unwrap());
        // Lady Geist and Bebop bind abilities naming it; the switched-off hero does not count.
        assert_eq!(users[&bomb].heroes, BTreeSet::from(["Bebop".to_owned(), "Lady Geist".to_owned()]));
        assert!(users[&bomb].item, "a sold item reaches it");
        assert!(!users[&bomb].every_hero);
        let mantle = casefold(&compiled("particles/generic/mantle.vpcf").unwrap());
        assert!(users[&mantle].every_hero && !users[&mantle].item);
        let tracer = casefold(&compiled("particles/weapon_fx/ghost/ghost_tracer.vpcf").unwrap());
        assert_eq!(users[&tracer].heroes, BTreeSet::from(["Lady Geist".to_owned()]));
    }

    #[test]
    fn a_bound_ability_another_hero_only_names_stays_own_and_keeps_its_note() {
        const SLASH: &str = "particles/abilities/blade/blade_power_slash.vpcf";
        const TRACER: &str = "particles/weapon_fx/blade/blade_tracer.vpcf";
        let heroes = obj(vec![
            ("hero_blade", hero("models/heroes_staging/haze/haze.vmdl", &[("ESlot_Signature_1", "ability_power_slash"), ("ESlot_Weapon_Primary", "weapon_blade_set"), ("ESlot_Signature_2", "ability_shared_dash")], b(false))),
            // Another hero's ability names the first one (a copy or a counter): that does not take it away.
            ("hero_bebop", hero("models/heroes_staging/bebop/bebop.vmdl", &[("ESlot_Signature_1", "ability_copy"), ("ESlot_Signature_2", "ability_shared_dash")], b(false))),
            // A template loading the same model and bound to the same weapon is not another hero.
            ("hero_base", hero("models/heroes_staging/haze/haze.vmdl", &[("ESlot_Weapon_Primary", "weapon_blade_set")], b(false))),
        ]);
        let abilities = obj(vec![
            ("ability_power_slash", ability("EAbilityType_Signature", vec![("m_Particle", s(SLASH)), ("m_Totem", s("models/abilities/blade_totem.vmdl"))])),
            ("ability_copy", ability("EAbilityType_Signature", vec![("m_Copies", s("ability_power_slash"))])),
            ("weapon_blade_set", ability("EAbilityType_Weapon", vec![("m_Tracer", s(TRACER))])),
            // Bound by both heroes: shared, not own.
            ("ability_shared_dash", ability("EAbilityType_Signature", vec![("m_Particle", s("particles/abilities/blade/blade_dash.vpcf"))])),
        ]);
        let defs = Definitions::from_documents(&[("heroes".into(), heroes), ("abilities".into(), abilities)]);
        let usage = Usage::new(&defs, "models/heroes_staging/haze/haze.vmdl", &Names::default()).unwrap();
        assert_eq!(usage.own.get("ability_power_slash"), Some(&("ESlot_Signature_1".to_owned(), "ability_power_slash".to_owned())));
        assert_eq!(usage.own.get("weapon_blade_set"), Some(&("ESlot_Weapon_Primary".to_owned(), "weapon_blade_set".to_owned())));
        assert!(!usage.own.contains_key("ability_shared_dash"));
        let slash = casefold(&compiled(SLASH).unwrap());
        assert_eq!(usage.own_files[&slash], BTreeSet::from(["ability_power_slash".to_owned()]));
        // The note is as it was: the other hero still reaches it.
        assert_eq!(usage.users[&slash], BTreeSet::from(["ability_power_slash".to_owned()]));
        assert!(usage.user.contains_key("ability_power_slash"));
        assert_eq!(usage.own_models["models/abilities/blade_totem.vmdl"], BTreeSet::from(["ability_power_slash".to_owned()]));
    }

    #[test]
    fn child_index_lists_parents_case_blind() {
        let index = ChildIndex::from_edges([("particles/a.vpcf_c".to_owned(), "particles/Child.vpcf_c".to_owned()), ("particles/b.vpcf_c".to_owned(), "particles/child.vpcf_c".to_owned())]);
        assert_eq!(index.parents("PARTICLES/child.vpcf_c"), ["particles/a.vpcf_c", "particles/b.vpcf_c"]);
        assert!(index.parents("particles/none.vpcf_c").is_empty());
        assert_eq!(readable_internal("citadel_ability_hornet_snipe", &[vec!["hornet".into()]]), "Snipe");
    }
}
