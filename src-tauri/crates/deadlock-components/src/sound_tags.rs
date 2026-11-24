//! Tags, readable names, groups and variation families for a hero's sounds
//! (Extras → Sounds). Everything here is derived from where the game keeps a
//! sound, what the file is called and, where the hero's sound events list it,
//! which event plays it. The rules:
//!
//! * `voice`: the file is under `sounds/vo/`, or an event that plays it derives
//!   from `Base.VO…` / `Base.Hero.VO`.
//! * `ability`: the file is under `sounds/abilities/`, or an event that plays it
//!   derives from `Base.Ability…` (Haze's sleep sound lives in
//!   `sounds/gameplay/status_effects/` and is played by an ability event), or it
//!   is a voice line said about an ability: `use_power1`…`4`,
//!   `upgrade_power…`, `desperation_power…`, `bespoke_ability_line` and pings
//!   `ping_use_ability…`. Item lines (`use_metal_skin`, `use_silencer`, …) are
//!   not ability lines.
//! * `weapon`: the file is under `sounds/weapons/`, or an event that plays it
//!   derives from `Base.Weapon…` or is named `<Hero>.Wpn.…` (whizbys borrowed
//!   from another hero's folder).
//! * `hero`: the file is in one of the hero's own folders (association
//!   `hero_directory`). Linked sounds that other heroes also use, and other
//!   heroes' lines about this hero, never carry it.
//!
//! Voice sub-kinds (only on voice lines): `ping` (the `ping/` folder or a
//! `ping` word), `emote` (the `emote/` folder: efforts, pain, death), `ability`
//! (the ability lines above), `about` (another hero's line that names this
//! hero), `match_start`, `kill` (`kill`, `killstreak…` words; `killed` is an ally
//! dying, not a kill) and `select` (`select` / `unselect`, the hero pick lines).
//!
//! Variation families: the numbered copies of one sound (`_01`, `-001`, …) in
//! one folder. Where the hero's sound events list the files, a family is also
//! limited to one file array of one event (`vsnd_files`, `vsnd_files_tail`, …
//! are separate arrays): events can split a numbered set but never join files
//! with different names, because some arrays mix layers (Haze's fire array
//! holds the shots, their tails and the last shot). Voice lines never use
//! events: the generated voice events hold one file each, and the progression
//! page events list unrelated lines.
use deadlock_core::text::casefold;
use deadlock_resource::{Kind, Value};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// How a sound belongs to the hero (see `resources::catalog_paths`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Owner {
    /// In one of the hero's own folders.
    Own,
    /// Reached through the hero's model, particles, events or graphs; may be shared.
    Linked,
    /// Another hero's voice line that names this hero.
    OtherHeroVoice,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SoundInfo {
    /// Readable name, for example "Assassinate · fire".
    pub name: String,
    /// Group key, for example `ability:assassinate`.
    pub group: String,
    pub group_name: String,
    /// Sort order of the group's category (abilities first).
    pub group_order: u32,
    /// Main tags: `hero`, `ability`, `weapon`, `voice` (any combination, maybe none).
    pub tags: Vec<String>,
    /// Voice sub-kinds (empty for other sounds).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub voice: Vec<String>,
    /// Variation family key; files with the same key are variations of one sound.
    pub family: String,
    /// `event` when a sound event lists the family's files, else `name`.
    pub family_source: String,
    /// The file's own variation number (`_03` → 3), when it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variation: Option<u32>,
    /// Names of the hero's sound events that play this file.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<String>,
}

struct List {
    event: String,
    root: String,
}

/// File arrays of decoded sound event documents (`.vsndevts_c` DATA roots).
#[derive(Default)]
pub struct Events {
    /// Casefolded `.vsnd_c` path → indexes into `lists` in document order.
    uses: BTreeMap<String, Vec<usize>>,
    lists: Vec<List>,
    /// Casefolded path → index of the first array with more than one file.
    family: BTreeMap<String, usize>,
    pub documents: usize,
    pub events: usize,
}

fn sound_path(value: &str) -> String {
    let mut path = casefold(value.trim_start_matches(['/', '\\'])).replace('\\', "/");
    if path.ends_with(".vsnd") {
        path.push_str("_c");
    }
    path
}

fn strings(value: &Value) -> Vec<String> {
    match &value.kind {
        Kind::String(s) => vec![s.clone()],
        Kind::Array(rows) => rows
            .iter()
            .filter_map(|v| match &v.kind {
                Kind::String(s) => Some(s.clone()),
                _ => None,
            })
            .collect(),
        _ => vec![],
    }
}

impl Events {
    /// Event documents in a fixed order (sorted by path), each the DATA root.
    pub fn from_documents(documents: &[Value]) -> Self {
        let mut bases: BTreeMap<String, String> = BTreeMap::new();
        let mut raw: Vec<(String, Vec<Vec<String>>)> = vec![];
        for doc in documents {
            let Kind::Object(rows) = &doc.kind else { continue };
            for (name, event) in rows {
                let Kind::Object(fields) = &event.kind else { continue };
                if let Some(base) = event.get("base").and_then(|b| b.string().ok()) {
                    bases.entry(name.clone()).or_insert_with(|| base.to_string());
                }
                let arrays: Vec<Vec<String>> = fields
                    .iter()
                    // vsnd_files, vsnd_files_tail, vsnd_files_last_shot, track_2.track_vsnd_files
                    .filter(|(key, _)| key.contains("vsnd_files"))
                    .map(|(_, v)| strings(v).iter().map(|s| sound_path(s)).collect::<Vec<_>>())
                    .filter(|files| !files.is_empty())
                    .collect();
                raw.push((name.clone(), arrays));
            }
        }
        let root = |name: &str| {
            let mut current = name.to_string();
            for _ in 0..16 {
                match bases.get(&current) {
                    Some(base) if base != &current => current = base.clone(),
                    _ => break,
                }
            }
            current
        };
        let mut out = Self { documents: documents.len(), events: raw.len(), ..Self::default() };
        let mut identical: BTreeMap<Vec<String>, usize> = BTreeMap::new();
        for (name, arrays) in raw {
            let base = root(&name);
            for files in arrays {
                let index = out.lists.len();
                out.lists.push(List { event: name.clone(), root: base.clone() });
                let distinct: BTreeSet<String> = files.iter().cloned().collect();
                // Identical arrays (Footstep and JumpLand) are one family.
                let family = *identical.entry(distinct.iter().cloned().collect()).or_insert(index);
                for file in &distinct {
                    out.uses.entry(file.clone()).or_default().push(index);
                    if distinct.len() > 1 {
                        out.family.entry(file.clone()).or_insert(family);
                    }
                }
            }
        }
        out
    }
    fn lists_of(&self, path: &str) -> impl Iterator<Item = &List> {
        self.uses
            .get(path)
            .into_iter()
            .flatten()
            .map(|i| &self.lists[*i])
    }
}

pub(crate) fn words(stem: &str) -> Vec<String> {
    casefold(stem)
        .split(['_', '-', ' ', '.'])
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

/// Removes trailing numbers (`_01`, `-001`, `_02_02`) and returns the last one.
fn split_variation(mut list: Vec<String>) -> (Vec<String>, Option<u32>) {
    let mut variation = None;
    while list.len() > 1 && list.last().is_some_and(|w| w.chars().all(|c| c.is_ascii_digit())) {
        let number = list.pop().unwrap();
        if variation.is_none() {
            variation = number.parse().ok();
        }
    }
    (list, variation)
}

pub(crate) fn token_words(tokens: &BTreeSet<String>) -> Vec<Vec<String>> {
    let mut out: Vec<Vec<String>> = tokens.iter().map(|t| words(t)).filter(|w| !w.is_empty()).collect();
    out.sort_by_key(|w| std::cmp::Reverse(w.len()));
    out
}

/// Drops leading hero names and ability slots (`a1`…`a4`).
pub(crate) fn strip_leading(mut list: Vec<String>, names: &[Vec<String>], extra: &[&str]) -> Vec<String> {
    loop {
        let before = list.len();
        if let Some(name) = names.iter().find(|n| list.len() > n.len() && list.starts_with(n)) {
            list.drain(..name.len());
        } else if list.len() > 1 && names.iter().any(|n| n.len() == 1 && misspelt(&list[0], &n[0])) {
            // The game's own typos of a hero prefix (Violet's `arist_landscape_…`).
            list.remove(0);
        }
        while list.len() > 1 && (slot(&list[0]) || extra.contains(&list[0].as_str())) {
            list.remove(0);
        }
        if list.len() == before {
            return list;
        }
    }
}

/// `word` is `name` with one letter left out (names of five letters or more).
fn misspelt(word: &str, name: &str) -> bool {
    name.len() >= 5
        && word.len() + 1 == name.len()
        && (0..name.len()).any(|i| name.is_char_boundary(i) && name.is_char_boundary(i + 1) && format!("{}{}", &name[..i], &name[i + 1..]) == word)
}

fn slot(word: &str) -> bool {
    word.len() == 2 && word.starts_with('a') && word.as_bytes()[1].is_ascii_digit()
}

pub(crate) fn readable(list: &[String]) -> String {
    let mut out: Vec<String> = vec![];
    for word in list {
        let w = match word.as_str() {
            "lp" => "loop".to_string(),
            "proj" => "projectile".to_string(),
            "wpn" => "weapon".to_string(),
            "explo" => "explosion".to_string(),
            "aoe" => "area".to_string(),
            w if w.len() == 6 && w.starts_with("group") => format!("group {}", w[5..].to_uppercase()),
            w if w.starts_with("convo") && w.len() > 5 && w[5..].chars().all(|c| c.is_ascii_digit()) => {
                format!("conversation {}", w[5..].trim_start_matches('0'))
            }
            w => w.to_string(),
        };
        out.push(w);
    }
    capital(&out.join(" "))
}

pub(crate) fn capital(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn joined(group: &str, rest: &[String]) -> String {
    if rest.is_empty() {
        group.to_string()
    } else {
        format!("{group} · {}", readable(rest).to_lowercase_first())
    }
}

trait LowerFirst {
    fn to_lowercase_first(&self) -> String;
}
impl LowerFirst for String {
    fn to_lowercase_first(&self) -> String {
        let mut chars = self.chars();
        match chars.next() {
            Some(c) => c.to_lowercase().collect::<String>() + chars.as_str(),
            None => String::new(),
        }
    }
}

/// Readable hero names for the game's internal names (`ghost` → Lady Geist).
pub fn hero_names() -> BTreeMap<String, String> {
    let aliases: BTreeMap<String, Vec<String>> =
        serde_json::from_str(include_str!("../../../../resources/catalog/hero_aliases.json")).unwrap_or_default();
    aliases
        .into_iter()
        .filter_map(|(k, v)| v.into_iter().next().map(|name| (k, name)))
        .collect()
}

/// Is this voice line about an ability (not an item)?
fn ability_line(list: &[String]) -> bool {
    list.windows(2).any(|pair| {
        let power = pair[1].strip_prefix("power").is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
        let ability = pair[1].strip_prefix("ability").is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
        (["use", "upgrade", "desperation"].contains(&pair[0].as_str()) && power)
            || (pair[0] == "use" && ability)
            || (pair[0] == "bespoke" && pair[1] == "ability")
    })
}

fn ability_line_name(list: &[String]) -> Option<String> {
    for pair in list.windows(2) {
        let number = pair[1].strip_prefix("power").or_else(|| pair[1].strip_prefix("ability")).filter(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
        if let Some(n) = number {
            match pair[0].as_str() {
                "use" => return Some(format!("Uses ability {n}")),
                "upgrade" => return Some(format!("Upgrades ability {n}")),
                "desperation" => return Some(format!("Ability {n} · desperate")),
                _ => {}
            }
        }
    }
    None
}

struct Parsed {
    folders: Vec<String>,
    /// Words of the file name, hero names and slots removed, no variation number.
    words: Vec<String>,
    stem: Vec<String>,
    variation: Option<u32>,
    dir: String,
}

fn parse(path: &str, names: &[Vec<String>]) -> Parsed {
    let folded = casefold(path);
    let mut parts: Vec<&str> = folded.split('/').collect();
    let file = parts.pop().unwrap_or_default();
    let stem = file.trim_end_matches(".vsnd_c").trim_end_matches(".vsnd");
    let (stem_words, variation) = split_variation(words(stem));
    Parsed {
        folders: parts.iter().map(|s| s.to_string()).collect(),
        words: strip_leading(stem_words.clone(), names, &[]),
        stem: stem_words,
        variation,
        dir: parts.join("/"),
    }
}

/// The hero being browsed.
pub struct Hero<'a> {
    /// `heroes::tokens` of the hero's model.
    pub tokens: &'a BTreeSet<String>,
    /// Readable hero name used in other heroes' lines ("Kill Vindicta").
    pub name: &'a str,
}

fn subfolder(parsed: &Parsed, under: &str) -> Option<Vec<String>> {
    let i = parsed.folders.iter().position(|f| f == under)?;
    // sounds/<under>/<hero>/<subfolder>/…
    let sub = parsed.folders.get(i + 2..)?;
    if sub.is_empty() {
        return None;
    }
    let list = strip_leading(words(&sub.join("_")), &[], &[]);
    (!list.is_empty()).then_some(list)
}

/// Group words of each top-level ability file: files sharing a first word form
/// a cluster whose common start (at most two words) names the ability; a file
/// alone keeps all but its last word.
fn ability_clusters(files: &[&Parsed]) -> BTreeMap<(String, Vec<String>), Vec<String>> {
    let mut clusters: BTreeMap<(String, String), BTreeSet<Vec<String>>> = BTreeMap::new();
    for p in files {
        if let Some(first) = p.words.first() {
            clusters.entry((p.dir.clone(), first.clone())).or_default().insert(p.words.clone());
        }
    }
    let mut out = BTreeMap::new();
    for ((dir, _), stems) in clusters {
        let list: Vec<&Vec<String>> = stems.iter().collect();
        let group: Vec<String> = if list.len() > 1 {
            let mut common = list[0].clone();
            for other in &list[1..] {
                let n = common.iter().zip(other.iter()).take_while(|(a, b)| a == b).count();
                common.truncate(n);
            }
            common.truncate(2);
            common
        } else {
            let w = list[0];
            let keep = (w.len().saturating_sub(1)).clamp(1, 2);
            w[..keep.min(w.len())].to_vec()
        };
        for stem in stems {
            out.insert((dir.clone(), stem), group.clone());
        }
    }
    out
}

fn slug(list: &[String]) -> String {
    list.join("_")
}

/// Tags, names, groups and families for one hero's sounds, in input order.
pub fn classify_all(rows: &[(String, Owner)], hero: &Hero, events: &Events) -> Vec<SoundInfo> {
    let names = token_words(hero.tokens);
    let display = hero_names();
    let parsed: Vec<Parsed> = rows.iter().map(|(path, _)| parse(path, &names)).collect();
    let top_abilities: Vec<&Parsed> = rows
        .iter()
        .zip(&parsed)
        .filter(|((path, owner), p)| {
            *owner == Owner::Own && casefold(path).starts_with("sounds/abilities/") && subfolder(p, "abilities").is_none()
        })
        .map(|(_, p)| p)
        .collect();
    let clusters = ability_clusters(&top_abilities);
    rows.iter()
        .zip(&parsed)
        .map(|((path, owner), p)| classify(path, *owner, p, hero, events, &names, &display, &clusters))
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn classify(
    path: &str,
    owner: Owner,
    p: &Parsed,
    hero: &Hero,
    events: &Events,
    names: &[Vec<String>],
    display: &BTreeMap<String, String>,
    clusters: &BTreeMap<(String, Vec<String>), Vec<String>>,
) -> SoundInfo {
    let folded = sound_path(path);
    let in_folder = |name: &str| folded.starts_with(&format!("sounds/{name}/"));
    let lists: Vec<&List> = events.lists_of(&folded).collect();
    let root_is = |prefix: &str| lists.iter().any(|l| l.root.starts_with(prefix));
    let wpn_event = lists.iter().any(|l| l.event.split('.').nth(1).is_some_and(|s| s.eq_ignore_ascii_case("wpn")));
    let voice = in_folder("vo") || root_is("Base.VO") || root_is("Base.Hero.VO");
    let mut kinds: Vec<String> = vec![];
    if voice {
        if p.folders.iter().any(|f| f == "ping") || p.stem.iter().any(|w| w == "ping") {
            kinds.push("ping".into());
        }
        if p.folders.iter().any(|f| f == "emote") {
            kinds.push("emote".into());
        }
        if ability_line(&p.stem) {
            kinds.push("ability".into());
        }
        if owner == Owner::OtherHeroVoice {
            kinds.push("about".into());
        }
        if p.stem.windows(2).any(|w| w[0] == "match" && w[1] == "start") {
            kinds.push("match_start".into());
        }
        if p.stem.iter().any(|w| w == "kill" || w.starts_with("killstreak")) {
            kinds.push("kill".into());
        }
        if matches!(p.words.as_slice(), [w] if w == "select" || w == "unselect") {
            kinds.push("select".into());
        }
    }
    let ability = in_folder("abilities") || root_is("Base.Ability") || kinds.iter().any(|k| k == "ability");
    let weapon = in_folder("weapons") || root_is("Base.Weapon") || wpn_event;
    let mut tags = vec![];
    if owner == Owner::Own {
        tags.push("hero".to_string());
    }
    for (on, tag) in [(ability, "ability"), (weapon, "weapon"), (voice, "voice")] {
        if on {
            tags.push(tag.to_string());
        }
    }
    // Families: events split, names join (voice lines by name only).
    let event_family = (!in_folder("vo")).then(|| events.family.get(&folded)).flatten();
    let family = match event_family {
        Some(list) => format!("{}/{}#{list}", p.dir, slug(&p.stem)),
        None => format!("{}/{}", p.dir, slug(&p.stem)),
    };
    let (group, group_name, group_order, name) = name_and_group(owner, p, hero, names, display, clusters, &kinds, voice, ability, weapon, &folded);
    let mut event_names: Vec<String> = lists.iter().map(|l| l.event.clone()).collect();
    event_names.dedup();
    SoundInfo {
        name,
        group,
        group_name,
        group_order,
        tags,
        voice: kinds,
        family,
        family_source: if event_family.is_some() { "event" } else { "name" }.into(),
        variation: p.variation,
        events: event_names,
    }
}

fn replace_heroes(list: &[String], hero: &Hero, names: &[Vec<String>], display: &BTreeMap<String, String>) -> Vec<String> {
    let mut out = vec![];
    let mut i = 0;
    while i < list.len() {
        if let Some(name) = names.iter().find(|n| list[i..].starts_with(n)) {
            out.push(hero.name.to_string());
            i += name.len();
            continue;
        }
        out.push(display.get(&list[i]).cloned().unwrap_or_else(|| list[i].clone()));
        i += 1;
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn name_and_group(
    owner: Owner,
    p: &Parsed,
    hero: &Hero,
    names: &[Vec<String>],
    display: &BTreeMap<String, String>,
    clusters: &BTreeMap<(String, Vec<String>), Vec<String>>,
    kinds: &[String],
    voice: bool,
    ability: bool,
    weapon: bool,
    folded: &str,
) -> (String, String, u32, String) {
    let plain = || readable(&p.words);
    if owner == Owner::Linked {
        return ("shared".into(), "Shared with other heroes".into(), 900, plain());
    }
    if voice {
        let has = |k: &str| kinds.iter().any(|x| x == k);
        let mut list = p.words.clone();
        if owner == Owner::OtherHeroVoice {
            // sounds/vo/<speaker>/…: the speaker's own name starts the file.
            let speaker = p.folders.get(2).cloned().unwrap_or_default();
            let speaker_words = words(&speaker);
            if list.starts_with(&speaker_words) && list.len() > speaker_words.len() {
                list.drain(..speaker_words.len());
            }
            if has("ping") && list.first().is_some_and(|w| w == "ping") && list.len() > 1 {
                list.remove(0);
            }
            let who = display.get(&speaker).cloned().unwrap_or_else(|| readable(&speaker_words));
            let text = readable(&replace_heroes(&list, hero, names, display));
            return (format!("about:{speaker}"), format!("Lines from {who}"), 380, text);
        }
        if has("ping") {
            if list.first().is_some_and(|w| w == "ping") && list.len() > 1 {
                list.remove(0);
            }
        }
        let text = ability_line_name(&list).unwrap_or_else(|| readable(&replace_heroes(&list, hero, names, display)));
        let (key, label, order) = if has("ping") {
            ("pings", "Pings", 310)
        } else if has("ability") {
            ("ability_lines", "Ability voice lines", 300)
        } else if has("emote") {
            ("emotes", "Emotes and efforts", 320)
        } else if has("match_start") {
            ("match_start", "Match start", 330)
        } else if has("kill") {
            ("kills", "Kills", 340)
        } else if has("select") {
            ("select", "Hero select", 350)
        } else {
            ("other", "Other voice lines", 360)
        };
        return (format!("voice:{key}"), label.into(), order, text);
    }
    if folded.starts_with("sounds/abilities/") || (ability && !weapon) {
        let group = subfolder(p, "abilities")
            .or_else(|| clusters.get(&(p.dir.clone(), p.words.clone())).cloned())
            .unwrap_or_else(|| p.words.iter().take(1).cloned().collect());
        let mut rest = p.words.clone();
        if rest.starts_with(&group) {
            rest.drain(..group.len());
        }
        let label = readable(&group);
        return (format!("ability:{}", slug(&group)), label.clone(), 100, joined(&label, &rest));
    }
    if weapon {
        let sub = subfolder(p, "weapons");
        let mut rest = strip_leading(p.words.clone(), names, &["wpn", "weapon"]);
        return match sub {
            Some(group) => {
                if rest.starts_with(&group) && rest.len() > group.len() {
                    rest.drain(..group.len());
                }
                let label = readable(&group);
                (format!("weapon:{}", slug(&group)), label.clone(), 210, joined(&label, &rest))
            }
            None => ("weapon:".into(), "Weapon".into(), 200, readable(&rest)),
        };
    }
    // Other sounds: named after their last folder that is not the hero's.
    let folder = p
        .folders
        .iter()
        .rev()
        .find(|f| f.as_str() != "sounds" && !names.iter().any(|n| words(f) == *n || words(&crate::heroes::strip_version(f)) == *n))
        .cloned()
        .unwrap_or_else(|| "other".into());
    let label = readable(&words(&folder));
    (format!("other:{folder}"), label, 400, plain())
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadlock_resource::{Kind, Value};

    fn s(v: &str) -> Value {
        Value::new(Kind::String(v.into()))
    }
    fn event(base: &str, arrays: &[(&str, &[&str])]) -> Value {
        let mut fields = vec![("base".to_string(), s(base))];
        for (key, files) in arrays {
            fields.push((key.to_string(), Value::new(Kind::Array(files.iter().map(|f| s(f)).collect()))));
        }
        Value::new(Kind::Object(fields))
    }
    fn doc(rows: Vec<(&str, Value)>) -> Value {
        Value::new(Kind::Object(rows.into_iter().map(|(k, v)| (k.to_string(), v)).collect()))
    }
    fn tokens(list: &[&str]) -> BTreeSet<String> {
        list.iter().map(|s| s.to_string()).collect()
    }
    fn run(rows: &[(&str, Owner)], toks: &[&str], name: &str, events: &Events) -> Vec<SoundInfo> {
        let toks = tokens(toks);
        let hero = Hero { tokens: &toks, name };
        classify_all(&rows.iter().map(|(p, o)| (p.to_string(), *o)).collect::<Vec<_>>(), &hero, events)
    }
    const VINDICTA: &[&str] = &["hornet", "hornet_v3", "vindicta"];

    #[test]
    fn main_tags_follow_the_folders_for_several_heroes() {
        let rows = [
            ("sounds/abilities/hornet/vindicta_stake_cast_01.vsnd_c", Owner::Own),
            ("sounds/weapons/hornet/vindicta_wpn_fire_main_01.vsnd_c", Owner::Own),
            ("sounds/vo/hornet/hornet_pick_up_gold_01.vsnd_c", Owner::Own),
            ("sounds/vo/hornet/hornet_use_power4_01.vsnd_c", Owner::Own),
            ("sounds/vo/hornet/hornet_use_metal_skin_01.vsnd_c", Owner::Own),
            ("sounds/player/footsteps/hornet/vindicta_footstep_01.vsnd_c", Owner::Own),
            ("sounds/abilities/priest/priest_witching_hour_cycle_stop_04.vsnd_c", Owner::Linked),
        ];
        let out = run(&rows, VINDICTA, "Vindicta", &Events::default());
        let tags: Vec<Vec<&str>> = out.iter().map(|i| i.tags.iter().map(String::as_str).collect()).collect();
        assert_eq!(tags[0], ["hero", "ability"]);
        assert_eq!(tags[1], ["hero", "weapon"]);
        assert_eq!(tags[2], ["hero", "voice"]);
        assert_eq!(tags[3], ["hero", "ability", "voice"], "a line said when using an ability");
        assert_eq!(tags[4], ["hero", "voice"], "item lines are not ability lines");
        assert_eq!(tags[5], ["hero"], "footsteps carry no kind tag (Other)");
        assert_eq!(tags[6], ["ability"], "a shared sound is never the hero's own");
        // Haze, Lady Geist (folder ghost / geist), Holliday (astro), Calico (nano, calico_ prefix).
        let haze = run(&[("sounds/abilities/haze/haze_smoke_bomb_cast.vsnd_c", Owner::Own), ("sounds/vo/haze/emote/haze_pain_death_01.vsnd_c", Owner::Own)], &["haze"], "Haze", &Events::default());
        assert_eq!(haze[0].tags, ["hero", "ability"]);
        assert_eq!(haze[1].voice, ["emote"]);
        let geist = run(&[("sounds/abilities/geist/geist_malice_cast_01.vsnd_c", Owner::Own), ("sounds/vo/ghost/ping/ghost_ping_use_ability4_01.vsnd_c", Owner::Own)], &["geist", "ghost", "lady_geist", "spectre"], "Lady Geist", &Events::default());
        assert_eq!(geist[0].name, "Malice · cast");
        assert_eq!(geist[1].tags, ["hero", "ability", "voice"]);
        assert_eq!(geist[1].voice, ["ping", "ability"]);
        assert_eq!(geist[1].name, "Uses ability 4");
        let calico = run(&[("sounds/abilities/nano/calico_ava_melee_01.vsnd_c", Owner::Own), ("sounds/abilities/nano/nano_ava_meow_01.vsnd_c", Owner::Own), ("sounds/abilities/nano/a3_rollermine/cat_mine_meow_01.vsnd_c", Owner::Own)], &["nano", "nano_v2", "calico"], "Calico", &Events::default());
        assert_eq!(calico[0].group_name, "Ava");
        assert_eq!(calico[1].group, calico[0].group);
        assert_eq!(calico[2].group_name, "Rollermine");
        assert_eq!(calico[2].name, "Rollermine · cat mine meow");
    }

    #[test]
    fn voice_sub_kinds_and_lines_other_heroes_say() {
        let rows = [
            ("sounds/vo/hornet/ping/hornet_ping_careful_yamato_01.vsnd_c", Owner::Own),
            ("sounds/vo/hornet/hornet_match_start_haze_hornet_convo01_01.vsnd_c", Owner::Own),
            ("sounds/vo/hornet/hornet_kill_anyhero_03.vsnd_c", Owner::Own),
            ("sounds/vo/hornet/hornet_ally_haze_killed_in_lane_01.vsnd_c", Owner::Own),
            ("sounds/vo/hornet/hornet_select_09.vsnd_c", Owner::Own),
            ("sounds/vo/hornet/hornet_desperation_power1_02_02.vsnd_c", Owner::Own),
            ("sounds/vo/hornet/hornet_bespoke_ability_line_07.vsnd_c", Owner::Own),
            ("sounds/vo/ghost/ghost_kill_hornet_02.vsnd_c", Owner::OtherHeroVoice),
            ("sounds/vo/haze/haze_enemy_hornet_damaged_by_snipe_01.vsnd_c", Owner::OtherHeroVoice),
        ];
        let out = run(&rows, VINDICTA, "Vindicta", &Events::default());
        assert_eq!(out[0].voice, ["ping"]);
        assert_eq!(out[0].name, "Careful Yamato");
        assert_eq!(out[1].voice, ["match_start"]);
        assert_eq!(out[2].voice, ["kill"]);
        assert!(out[3].voice.is_empty(), "an ally dying is not a kill: {:?}", out[3].voice);
        assert_eq!(out[3].name, "Ally Haze killed in lane");
        assert_eq!(out[4].voice, ["select"]);
        assert_eq!(out[5].voice, ["ability"]);
        assert_eq!(out[5].name, "Ability 1 · desperate");
        assert_eq!(out[5].family, "sounds/vo/hornet/hornet_desperation_power1");
        assert_eq!(out[6].group_name, "Ability voice lines");
        assert_eq!(out[7].tags, ["voice"], "another hero's line is never the hero's own");
        assert_eq!(out[7].voice, ["about", "kill"]);
        assert_eq!(out[7].name, "Kill Vindicta");
        assert_eq!(out[7].group_name, "Lines from Lady Geist");
        assert_eq!(out[8].name, "Enemy Vindicta damaged by snipe");
        assert_eq!(out[8].group, "about:haze");
        assert_eq!(out[1].name, "Match start Haze Vindicta conversation 1");
        let ping = run(&[("sounds/vo/astro/ping/astro_ping_hornet_almost_respawn.vsnd_c", Owner::OtherHeroVoice)], VINDICTA, "Vindicta", &Events::default());
        assert_eq!((ping[0].name.as_str(), ping[0].group_name.as_str()), ("Vindicta almost respawn", "Lines from Holliday"));
        assert_eq!(ping[0].voice, ["ping", "about"]);
    }

    #[test]
    fn a_misspelt_hero_prefix_joins_the_ability() {
        let out = run(&[
            ("sounds/abilities/artist/arist_landscape_expire_01.vsnd_c", Owner::Own),
            ("sounds/abilities/artist/artist_landscape_cast_01.vsnd_c", Owner::Own),
            ("sounds/abilities/artist/artist_landscape_zone_lp_01.vsnd_c", Owner::Own),
        ], &["artist", "violet"], "Violet", &Events::default());
        assert_eq!(out[0].group, out[1].group);
        assert_eq!(out[0].name, "Landscape · expire");
        assert_eq!(out[2].name, "Landscape · zone loop");
    }

    #[test]
    fn readable_names_and_groups() {
        let rows = [
            ("sounds/abilities/hornet/vindicta_assassinate_fire_01.vsnd_c", Owner::Own),
            ("sounds/abilities/hornet/vindicta_assassinate_scope_lp.vsnd_c", Owner::Own),
            ("sounds/abilities/hornet/vindicta_crow_familiar_cast.vsnd_c", Owner::Own),
            ("sounds/abilities/hornet/vindicta_flight_cast.vsnd_c", Owner::Own),
            ("sounds/abilities/hornet/vindicta_flight_loop.vsnd_c", Owner::Own),
            ("sounds/abilities/hornet/a4_assassinate/target_impact_pop_01.vsnd_c", Owner::Own),
            ("sounds/weapons/hornet/reload/vindicta_wpn_reload_start_01.vsnd_c", Owner::Own),
            ("sounds/weapons/hornet/vindicta_weapon_whizby_groupa-001.vsnd_c", Owner::Own),
            ("sounds/player/footsteps/hornet/vindicta_footstep_01.vsnd_c", Owner::Own),
            ("sounds/abilities/astro/a2_bounce_pad/holliday_a2_bounce_pad_launch_01.vsnd_c", Owner::Own),
        ];
        let mut toks = VINDICTA.to_vec();
        toks.push("holliday");
        let out = run(&rows, &toks, "Vindicta", &Events::default());
        let names: Vec<(&str, &str)> = out.iter().map(|i| (i.group_name.as_str(), i.name.as_str())).collect();
        assert_eq!(names, [
            ("Assassinate", "Assassinate · fire"),
            ("Assassinate", "Assassinate · scope loop"),
            ("Crow familiar", "Crow familiar · cast"),
            ("Flight", "Flight · cast"),
            ("Flight", "Flight · loop"),
            ("Assassinate", "Assassinate · target impact pop"),
            ("Reload", "Reload · start"),
            ("Weapon", "Whizby group A"),
            ("Footsteps", "Footstep"),
            ("Bounce pad", "Bounce pad · launch"),
        ]);
        assert_eq!(out[0].group, out[5].group, "the a4_ folder joins the ability's other files");
        assert!(out[0].group_order < out[6].group_order);
    }

    #[test]
    fn variation_families_from_names_and_from_event_arrays() {
        // Numbered names in one folder are one family; the same name in another folder is not.
        let rows = [
            ("sounds/abilities/hornet/vindicta_assassinate_fire_01.vsnd_c", Owner::Own),
            ("sounds/abilities/hornet/vindicta_assassinate_fire_02.vsnd_c", Owner::Own),
            ("sounds/weapons/hornet/vindicta_assassinate_fire_01.vsnd_c", Owner::Own),
            ("sounds/vo/hornet/hornet_use_power4_01.vsnd_c", Owner::Own),
            ("sounds/vo/hornet/hornet_use_power4_16_02.vsnd_c", Owner::Own),
            ("sounds/vo/hornet/hornet_ally_bookworm_killed_in_lane_01_alt.vsnd_c", Owner::Own),
        ];
        let out = run(&rows, VINDICTA, "Vindicta", &Events::default());
        assert_eq!(out[0].family, out[1].family);
        assert_ne!(out[0].family, out[2].family);
        assert_eq!((out[0].variation, out[1].variation), (Some(1), Some(2)));
        assert_eq!(out[3].family, out[4].family, "_16_02 is still a use_power4 line");
        assert_eq!(out[4].variation, Some(2));
        assert_eq!(out[5].variation, None);
        assert_eq!(out[0].family_source, "name");

        // Events: separate arrays of one event are separate sets, files repeat across events,
        // identical arrays are one family, and event paths are matched case-blind as .vsnd_c.
        let events = Events::from_documents(&[doc(vec![
            ("Vindicta.Wpn.Fire", event("Base.Weapon.Rifle", &[
                ("vsnd_files", &["sounds/weapons/hornet/vindicta_wpn_fire_main_01.vsnd", "sounds/weapons/hornet/vindicta_wpn_fire_main_02.vsnd"]),
                ("vsnd_files_tail", &["sounds/weapons/hornet/vindicta_wpn_fire_tail_ext_01.vsnd", "sounds/weapons/hornet/vindicta_wpn_fire_tail_ext_02.vsnd"]),
            ])),
            ("Vindicta.Wpn.Whizby", event("Base.Bullet.Whizby", &[("vsnd_files", &["sounds/weapons/hornet/vindicta_weapon_whizby_groupA-001.vsnd", "sounds/weapons/hornet/vindicta_weapon_whizby_groupA-002.vsnd", "sounds/weapons/hornet/vindicta_weapon_whizby_groupB-001.vsnd"])])),
            ("Vindicta.Crow.Proj.Expire", event("Base.Ability.Detached", &[("vsnd_files", &["sounds/abilities/hornet/vindicta_familiar_proj_expire_01.vsnd", "sounds/abilities/hornet/vindicta_familiar_proj_expire_02.vsnd"])])),
            ("Vindicta.Crow.Debuff.End", event("Base.Ability.StatusEffect", &[("vsnd_files", &["sounds/abilities/hornet/vindicta_familiar_proj_expire_02.vsnd", "sounds/abilities/hornet/vindicta_familiar_proj_expire_01.vsnd"])])),
            ("Haze.Reload.End_01", event("Base.Weapon.Foley", &[("vsnd_files", &["sounds/weapons/haze/reload/end_01.vsnd"])])),
            ("Haze.Reload.End_02", event("Base.Weapon.Foley", &[("vsnd_files", &["sounds/weapons/haze/reload/end_02.vsnd"])])),
            ("Hornet.Assassinate.Charge.Complete", event("Base.Ability.Ult", &[("vsnd_files", &["sounds/abilities/priest/priest_witching_hour_cycle_stop_04.vsnd"])])),
            ("Artist.Wpn.Whizby", event("Base.Bullet.Whizby", &[("vsnd_files", &["sounds/weapons/bookworm/whizbys/bookworm_weapon_whizby_01.vsnd", "sounds/weapons/bookworm/whizbys/bookworm_weapon_whizby_02.vsnd"])])),
            ("Haze.Finesse.Dagger.Sleep", event("Base.Ability.StatusEffect", &[("vsnd_files", &["sounds/gameplay/status_effects/sleep.vsnd"])])),
        ])]);
        let rows = [
            ("sounds/weapons/hornet/vindicta_wpn_fire_main_01.vsnd_c", Owner::Own),
            ("sounds/weapons/hornet/vindicta_wpn_fire_main_02.vsnd_c", Owner::Own),
            ("sounds/weapons/hornet/vindicta_wpn_fire_tail_ext_01.vsnd_c", Owner::Own),
            ("sounds/weapons/hornet/vindicta_weapon_whizby_groupa-001.vsnd_c", Owner::Own),
            ("sounds/weapons/hornet/vindicta_weapon_whizby_groupa-002.vsnd_c", Owner::Own),
            ("sounds/weapons/hornet/vindicta_weapon_whizby_groupb-001.vsnd_c", Owner::Own),
            ("sounds/abilities/hornet/vindicta_familiar_proj_expire_01.vsnd_c", Owner::Own),
            ("sounds/abilities/hornet/vindicta_familiar_proj_expire_02.vsnd_c", Owner::Own),
            ("sounds/weapons/haze/reload/end_01.vsnd_c", Owner::Own),
            ("sounds/weapons/haze/reload/end_02.vsnd_c", Owner::Own),
            ("sounds/abilities/priest/priest_witching_hour_cycle_stop_04.vsnd_c", Owner::Linked),
            ("sounds/weapons/bookworm/whizbys/bookworm_weapon_whizby_01.vsnd_c", Owner::Linked),
            ("sounds/gameplay/status_effects/sleep.vsnd_c", Owner::Linked),
        ];
        let out = run(&rows, VINDICTA, "Vindicta", &events);
        assert_eq!(out[0].family, out[1].family);
        assert_eq!(out[0].family_source, "event");
        assert_ne!(out[0].family, out[2].family, "the tail array is its own set");
        assert_eq!(out[3].family, out[4].family);
        assert_ne!(out[3].family, out[5].family, "events never join different names");
        assert_eq!(out[6].family, out[7].family, "a file in two events gets one family");
        assert_eq!(out[6].events, ["Vindicta.Crow.Proj.Expire", "Vindicta.Crow.Debuff.End"]);
        assert_eq!(out[8].family, out[9].family, "one-file events do not split a numbered set");
        assert_eq!(out[8].family_source, "name");
        assert_eq!(out[10].tags, ["ability"]);
        assert_eq!(out[11].tags, ["weapon"], "a Wpn event borrowing another hero's whizbys");
        assert_eq!(out[12].tags, ["ability"], "played by an ability event outside the abilities folder");
        assert_eq!(out[12].group_name, "Shared with other heroes");
        // Base chains resolve through the hero's own events.
        let chained = Events::from_documents(&[doc(vec![
            ("Haze.Wpn.Fire.Main", event("Base.Weapon.Pistol", &[])),
            ("Haze.Wpn.Fire.First", event("Haze.Wpn.Fire.Main", &[("vsnd_files", &["sounds/x/first_01.vsnd", "sounds/x/first_02.vsnd"])])),
        ])]);
        let out = run(&[("sounds/x/first_01.vsnd_c", Owner::Linked)], &["haze"], "Haze", &chained);
        assert_eq!(out[0].tags, ["weapon"]);
    }
}
