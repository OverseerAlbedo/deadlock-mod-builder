//! What a VPK changes, read from the paths in its directory only (no file
//! contents): the kind of each file, the heroes it belongs to and a content
//! identity that survives renaming and renumbering. For Build & install →
//! Installed mods.
//!
//! Heroes come from the app's existing rules, never from new guesses:
//! * a folder named after the hero (`heroes::owned` with `heroes::tokens`, the
//!   rule the sound and effect catalogs start from);
//! * for Panorama images, the hero's name in the file name (`heroes::matches`,
//!   as `icon_catalog` picks hero images);
//! * item pictures (`item_icons::ITEM_FOLDER`, and the older
//!   `panorama/images/upgrades/` glyphs) and the shop's item names
//!   (`resource/localization/citadel_gc_mod_names/`) belong to every hero;
//! * models and particle systems no folder names: the game's definitions
//!   (`effect_catalog::Definitions::file_users`), when the caller has read them;
//! * ability objects (`models/abilities/`) the definitions still leave without
//!   a hero: the hero's name as a whole word of the file name (`heroes::matches`).
use crate::{effect_catalog::FileUse, heroes, item_icons, variants};
use deadlock_core::text::casefold;
use deadlock_vpk::Entry;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

/// The kind of change a file makes, from its path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// A hero body model of the roster (main or alternate).
    CharacterModel,
    /// A weapon model (`variants::weapon_like`, not an effect model).
    Gun,
    /// A model under `models/abilities/` (thrown objects, summons, totems).
    AbilityObject,
    /// Any other model.
    OtherModel,
    /// Meshes, physics, morphs and other parts of compiled models.
    ModelData,
    Material,
    /// Textures outside Panorama (used by materials).
    Texture,
    Sound,
    /// Sound event and mixing documents.
    SoundEvent,
    /// Panorama images other than item pictures (hero portraits, ability icons, HUD).
    Icon,
    /// Item pictures of the shop.
    ItemIcon,
    /// The shop's item names (`resource/localization/citadel_gc_mod_names/`).
    ItemNames,
    Effect,
    Animation,
    /// Game definitions (`scripts/`, `.vdata`).
    GameData,
    /// Maps and world files.
    Map,
    /// Panorama layouts, styles and scripts (menus and HUD).
    Menu,
    /// Localisation and other text resources.
    Text,
    Other,
}

const MODEL_DATA: [&str; 6] = [".vmesh_c", ".vphys_c", ".vmorf_c", ".vrman_c", ".vmdl_prefab_c", ".vmesh"];
const ANIMATION: [&str; 5] = [".vanim_c", ".vseq_c", ".vagrp_c", ".vnmskel_c", ".vanmgrph_c"];
const SOUND_EVENTS: [&str; 3] = [".vsndevts_c", ".vsndstck_c", ".vmix_c"];
const MAP: [&str; 5] = [".vwrld_c", ".vwnod_c", ".vents_c", ".vmap_c", ".vvis_c"];
const MENU: [&str; 5] = [".vxml_c", ".vcss_c", ".vjs_c", ".vts_c", ".vpdi_c"];
/// The older item glyphs (`m_strAbilityImage`); not offered by Extras → Items,
/// but item pictures all the same.
const UPGRADE_FOLDER: &str = "panorama/images/upgrades/";
/// Ability objects: the only models whose file names may name a hero.
const ABILITY_FOLDER: &str = "models/abilities/";

fn ends(path: &str, list: &[&str]) -> bool {
    list.iter().any(|s| path.ends_with(s))
}

/// The roster's heroes with their tokens, read once.
#[derive(Debug, Clone)]
pub struct HeroTable {
    heroes: Vec<(String, BTreeSet<String>)>,
    /// Casefolded body model paths (`….vmdl_c`) → hero name.
    bodies: BTreeMap<String, String>,
}
impl HeroTable {
    pub fn new() -> crate::Result<Self> {
        let mut heroes: Vec<(String, BTreeSet<String>)> = vec![];
        let mut bodies = BTreeMap::new();
        for hero in heroes::roster()? {
            let tokens = heroes::tokens(&hero.path)?;
            match heroes.iter_mut().find(|(name, _)| *name == hero.name) {
                Some((_, known)) => known.extend(tokens),
                None => heroes.push((hero.name.clone(), tokens)),
            }
            for path in std::iter::once(&hero.path).chain(hero.extras.iter().map(|m| &m.path)) {
                bodies.insert(format!("{}_c", casefold(path).trim_end_matches("_c")), hero.name.clone());
            }
        }
        Ok(Self { heroes, bodies })
    }
    /// The hero of a roster body model.
    pub fn body(&self, folded: &str) -> Option<&str> {
        self.bodies.get(folded).map(String::as_str)
    }
    /// Heroes a path belongs to by the app's path rules (folder; for Panorama
    /// images also the file name).
    pub fn heroes_of(&self, path: &str) -> BTreeSet<String> {
        let folded = casefold(path);
        let image = crate::icon_catalog::image_resource(&folded);
        self.heroes
            .iter()
            .filter(|(_, tokens)| heroes::owned(&folded, tokens) || image && heroes::matches(&folded, tokens))
            .map(|(name, _)| name.clone())
            .collect()
    }
    /// Heroes whose name is a whole word of the file name (`heroes::matches`),
    /// for files under `models/abilities/` only, after the game's definitions.
    pub fn named_in_file(&self, path: &str) -> BTreeSet<String> {
        let folded = casefold(path);
        if !folded.starts_with(ABILITY_FOLDER) {
            return BTreeSet::new();
        }
        let file = folded.rsplit('/').next().unwrap_or_default();
        let named: Vec<_> = self.heroes.iter().filter(|(_, tokens)| heroes::matches(file, tokens)).collect();
        // Several heroes named: the one the name STARTS with owns the file
        // (`warden_ghost_model` is Warden's, although "ghost" is another hero's word).
        let leads = |token: &String| file.strip_prefix(token.as_str()).is_some_and(|rest| !rest.starts_with(|c: char| c.is_ascii_alphanumeric()));
        let leading: BTreeSet<String> = named.iter().filter(|(_, tokens)| tokens.iter().any(leads)).map(|(name, _)| name.clone()).collect();
        if named.len() > 1 && !leading.is_empty() {
            return leading;
        }
        named.into_iter().map(|(name, _)| name.clone()).collect()
    }
    pub fn len(&self) -> usize {
        self.heroes.len()
    }
    pub fn is_empty(&self) -> bool {
        self.heroes.is_empty()
    }
}

/// The kind of one file (casefolded or not).
pub fn kind(table: &HeroTable, path: &str) -> Kind {
    let p = casefold(path);
    if p.ends_with(".vmdl_c") || p.ends_with(".vmdl") {
        let compiled = format!("{}_c", p.trim_end_matches("_c"));
        return if table.body(&compiled).is_some() {
            Kind::CharacterModel
        } else if p.starts_with("models/abilities/") {
            Kind::AbilityObject
        } else if variants::weapon_like(&p) && !variants::effect_model(&p) {
            Kind::Gun
        } else {
            Kind::OtherModel
        };
    }
    if p.starts_with(crate::item_names::PACKAGE_FOLDER) {
        return Kind::ItemNames;
    }
    if p.starts_with(item_icons::ITEM_FOLDER) || p.starts_with(UPGRADE_FOLDER) {
        return Kind::ItemIcon;
    }
    if p.starts_with("panorama/images/") {
        return Kind::Icon;
    }
    if p.starts_with("panorama/") || ends(&p, &MENU) {
        return Kind::Menu;
    }
    if ends(&p, &MODEL_DATA) {
        return Kind::ModelData;
    }
    if p.ends_with(".vmat_c") || p.ends_with(".vmat") {
        return Kind::Material;
    }
    if p.ends_with(".vtex_c") || p.ends_with(".vtex") {
        return Kind::Texture;
    }
    if p.ends_with(".vsnd_c") {
        return Kind::Sound;
    }
    if ends(&p, &SOUND_EVENTS) {
        return Kind::SoundEvent;
    }
    if p.ends_with(".vpcf_c") || p.ends_with(".vsnap_c") {
        return Kind::Effect;
    }
    if ends(&p, &ANIMATION) || p.ends_with(".vnmclip_c") || p.ends_with(".vnmgraph_c") || p.contains(".vnm") {
        return Kind::Animation;
    }
    if p.starts_with("scripts/") || p.ends_with(".vdata_c") || p.ends_with(".vdata") {
        return Kind::GameData;
    }
    if p.starts_with("maps/") || ends(&p, &MAP) {
        return Kind::Map;
    }
    if p.starts_with("resource/") || p.ends_with(".txt") {
        return Kind::Text;
    }
    Kind::Other
}

/// How many files of the VPK belong to a hero.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeroShare {
    pub name: String,
    pub files: usize,
}
/// A model under `models/abilities/` and who the game's definitions say uses it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AbilityObject {
    pub path: String,
    pub heroes: Vec<String>,
    pub every_hero: bool,
    /// False when the definitions were not read or do not name it.
    pub known: bool,
}
/// At most this many paths are listed per kind in the details.
pub const SAMPLE_PATHS: usize = 40;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contents {
    pub files: usize,
    /// Files per kind.
    pub kinds: BTreeMap<Kind, usize>,
    /// Heroes, most files first (then by name).
    pub heroes: Vec<HeroShare>,
    /// Files that belong to every hero (item pictures, files every hero's
    /// definitions reach).
    pub every_hero: usize,
    /// Files no rule gives to a hero.
    pub unattributed: usize,
    pub ability_objects: Vec<AbilityObject>,
    /// Up to `SAMPLE_PATHS` paths per kind, sorted.
    pub samples: BTreeMap<Kind, Vec<String>>,
}

/// Paths a classification would like the game's definitions for: models and
/// particle systems no folder gives to a hero.
pub fn wants_definitions(table: &HeroTable, paths: &[&str]) -> bool {
    paths.iter().any(|p| {
        let k = kind(table, p);
        matches!(k, Kind::AbilityObject | Kind::OtherModel | Kind::Gun | Kind::Effect) && table.heroes_of(p).is_empty()
    })
}

/// Classifies the paths of one VPK. `users` are the game definitions' file
/// users (`Definitions::file_users`), `None` when they were not read.
pub fn classify(table: &HeroTable, paths: &[&str], users: Option<&BTreeMap<String, FileUse>>) -> Contents {
    let mut out = Contents { files: paths.len(), ..Contents::default() };
    let mut shares: BTreeMap<String, usize> = BTreeMap::new();
    let mut sorted: Vec<String> = paths.iter().map(|p| casefold(p)).collect();
    sorted.sort();
    for path in &sorted {
        let k = kind(table, path);
        *out.kinds.entry(k).or_default() += 1;
        let samples = out.samples.entry(k).or_default();
        if samples.len() < SAMPLE_PATHS {
            samples.push(path.clone());
        }
        let mut heroes = match k {
            Kind::CharacterModel => table.body(&format!("{}_c", path.trim_end_matches("_c"))).map(|h| BTreeSet::from([h.to_owned()])).unwrap_or_default(),
            _ => table.heroes_of(path),
        };
        let mut every = matches!(k, Kind::ItemIcon | Kind::ItemNames);
        let compiled = format!("{}_c", path.trim_end_matches("_c"));
        let usage = (heroes.is_empty() && !every && matches!(k, Kind::AbilityObject | Kind::OtherModel | Kind::Gun | Kind::Effect))
            .then(|| users.and_then(|u| u.get(&compiled)))
            .flatten();
        if let Some(usage) = usage {
            if usage.every_hero {
                every = true;
            } else {
                heroes = usage.heroes.clone();
            }
        }
        // Ability objects the definitions leave without a hero: the hero's name in the file name.
        if k == Kind::AbilityObject && heroes.is_empty() && !every {
            heroes = table.named_in_file(path);
        }
        if k == Kind::AbilityObject {
            out.ability_objects.push(AbilityObject {
                path: path.clone(),
                heroes: if every { vec![] } else { heroes.iter().cloned().collect() },
                every_hero: every,
                known: every || !heroes.is_empty(),
            });
        }
        if every {
            out.every_hero += 1;
        } else if heroes.is_empty() {
            out.unattributed += 1;
        }
        for hero in heroes {
            *shares.entry(hero).or_default() += 1;
        }
    }
    let mut heroes: Vec<HeroShare> = shares.into_iter().map(|(name, files)| HeroShare { name, files }).collect();
    heroes.sort_by(|a, b| b.files.cmp(&a.files).then_with(|| a.name.cmp(&b.name)));
    out.heroes = heroes;
    out
}

/// Prefix of the content identity: bump it only when the hashed records change.
pub const IDENTITY_VERSION: &str = "vpk-tree-v1";

/// A stable identity of a VPK's contents from its directory alone: SHA-256 of
/// the sorted records (casefolded path, CRC32 of the file, its size). Archive
/// index, offsets, preload split and VPK version are left out, so renaming,
/// renumbering or repacking the same files keeps it; changing any file's bytes
/// changes its CRC or size and so the identity.
pub fn identity<'a>(entries: impl IntoIterator<Item = &'a Entry>) -> String {
    let mut rows: Vec<(String, u32, u64)> = entries
        .into_iter()
        .map(|e| (casefold(&e.path), e.crc, e.length as u64 + e.preload.len() as u64))
        .collect();
    rows.sort();
    let mut hash = Sha256::new();
    for (path, crc, size) in &rows {
        hash.update(path.as_bytes());
        hash.update([0]);
        hash.update(crc.to_le_bytes());
        hash.update(size.to_le_bytes());
    }
    let digest = hash.finalize();
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    format!("{IDENTITY_VERSION}:{hex}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> HeroTable {
        HeroTable::new().unwrap()
    }
    fn names(c: &Contents) -> Vec<&str> {
        c.heroes.iter().map(|h| h.name.as_str()).collect()
    }

    #[test]
    fn each_kind_comes_from_the_path() {
        let t = table();
        let cases = [
            ("models/heroes_staging/haze/haze.vmdl_c", Kind::CharacterModel),
            ("models/weapons/hornet/hornet_gun.vmdl_c", Kind::Gun),
            ("models/abilities/sleep_dagger.vmdl_c", Kind::AbilityObject),
            ("models/particle/haze_gun_magazine_model_fx.vmdl_c", Kind::OtherModel),
            ("models/heroes_staging/haze/haze.vmesh_c", Kind::ModelData),
            ("materials/models/heroes/haze/body.vmat_c", Kind::Material),
            ("materials/models/heroes/haze/body_color_png.vtex_c", Kind::Texture),
            ("sounds/abilities/haze/haze_dagger_cast.vsnd_c", Kind::Sound),
            ("soundevents/hero_haze.vsndevts_c", Kind::SoundEvent),
            ("panorama/images/heroes/haze_card_psd.vtex_c", Kind::Icon),
            ("panorama/images/items/weapon/basic_magazine_psd.vtex_c", Kind::ItemIcon),
            ("panorama/images/upgrades/basic_magazine_psd.vtex_c", Kind::ItemIcon),
            ("panorama/layout/hud/hud.vxml_c", Kind::Menu),
            ("particles/abilities/haze/haze_dagger.vpcf_c", Kind::Effect),
            ("animgraphs/haze/haze_clip.vnmclip_c", Kind::Animation),
            ("scripts/abilities.vdata_c", Kind::GameData),
            ("maps/street_test.vwrld_c", Kind::Map),
            ("resource/localization/citadel_gc_english.txt", Kind::Text),
            ("resource/localization/citadel_gc_mod_names/citadel_gc_mod_names_english.txt", Kind::ItemNames),
            ("resource/localization/citadel_mods/citadel_mods_english.txt", Kind::Text),
            ("shaders/vfx/pbr_pcgl_40_ps.vcs", Kind::Other),
        ];
        for (path, expected) in cases {
            assert_eq!(kind(&t, path), expected, "{path}");
        }
    }

    #[test]
    fn heroes_come_from_folders_and_hero_images_from_their_names() {
        let t = table();
        let paths = [
            "models/heroes_staging/haze/haze.vmdl_c",
            "materials/models/heroes/haze/body.vmat_c",
            "sounds/vo/haze/haze_kill_01.vsnd_c",
            "panorama/images/heroes/haze_card_psd.vtex_c",
            "particles/abilities/ghost/ghost_blood_bomb.vpcf_c",
        ];
        let c = classify(&t, &paths, None);
        assert_eq!(names(&c), ["Haze", "Lady Geist"]);
        assert_eq!(c.heroes[0].files, 4);
        assert_eq!(c.kinds[&Kind::CharacterModel], 1);
        assert_eq!(c.unattributed, 0);
        // A hero word in a file name outside Panorama images is not a folder: not attributed.
        let c = classify(&t, &["sounds/vo/ghost/ghost_kill_haze_01.vsnd_c", "maps/ghost_town.vwrld_c"], None);
        assert_eq!(names(&c), ["Lady Geist"], "the speaker's folder decides, not the words of the name");
        assert_eq!(c.unattributed, 1);
    }

    #[test]
    fn item_icons_belong_to_every_hero_and_files_without_a_hero_are_counted() {
        let t = table();
        let c = classify(&t, &["panorama/images/items/weapon/basic_magazine_psd.vtex_c", "panorama/images/items/spirit/mystic_burst_psd.vtex_c", "maps/start.vwrld_c", "panorama/layout/main_menu.vxml_c"], None);
        assert!(c.heroes.is_empty());
        assert_eq!((c.every_hero, c.unattributed), (2, 2));
        assert_eq!(c.kinds[&Kind::ItemIcon], 2);
        // The shop's item names too, as their own kind.
        let c = classify(&t, &["resource/localization/citadel_gc_mod_names/citadel_gc_mod_names_english.txt", "resource/localization/citadel_gc_mod_names/citadel_gc_mod_names_german.txt"], None);
        assert_eq!((c.every_hero, c.unattributed, c.kinds[&Kind::ItemNames]), (2, 0, 2));
        assert_eq!(serde_json::to_value(&c).unwrap()["kinds"]["item_names"], 2);
    }

    #[test]
    fn ability_objects_take_their_heroes_from_the_definitions() {
        let t = table();
        let mut users = BTreeMap::new();
        users.insert(
            "models/abilities/sleep_dagger.vmdl_c".to_owned(),
            FileUse { heroes: ["Haze".to_owned(), "Vyper".to_owned()].into(), every_hero: false, item: false },
        );
        users.insert("particles/generic/mantle_dust.vpcf_c".to_owned(), FileUse { heroes: BTreeSet::new(), every_hero: true, item: false });
        let paths = ["models/abilities/sleep_dagger.vmdl_c", "particles/generic/mantle_dust.vpcf_c", "models/abilities/unknown_totem.vmdl_c"];
        assert!(wants_definitions(&t, &paths));
        let c = classify(&t, &paths, Some(&users));
        assert_eq!(names(&c), ["Haze", "Vyper"]);
        assert_eq!(c.every_hero, 1);
        assert_eq!(c.ability_objects.len(), 2);
        assert!(c.ability_objects.iter().any(|a| a.path.ends_with("sleep_dagger.vmdl_c") && a.known && a.heroes == ["Haze", "Vyper"]));
        assert!(c.ability_objects.iter().any(|a| a.path.ends_with("unknown_totem.vmdl_c") && !a.known));
        // Without the definitions nothing is guessed.
        let c = classify(&t, &paths, None);
        assert!(c.heroes.is_empty() && c.unattributed == 3);
        assert!(!wants_definitions(&t, &["models/heroes_staging/haze/haze.vmdl_c", "sounds/vo/haze/a.vsnd_c"]));
    }

    #[test]
    fn ability_objects_the_definitions_leave_out_take_the_hero_named_in_the_file_name() {
        let t = table();
        let mut users = BTreeMap::new();
        users.insert("models/abilities/haze_shared_bomb.vmdl_c".to_owned(), FileUse { heroes: ["Mina".to_owned(), "Vyper".to_owned()].into(), every_hero: false, item: false });
        let paths = ["models/abilities/haze_smoke_bomb.vmdl_c", "models/abilities/haze_shared_bomb.vmdl_c", "models/abilities/hazelnut_tree.vmdl_c"];
        let c = classify(&t, &paths, Some(&users));
        let of = |p: &str| c.ability_objects.iter().find(|a| a.path.ends_with(p)).unwrap().heroes.clone();
        assert_eq!(of("haze_smoke_bomb.vmdl_c"), ["Haze"]);
        // The definitions decide first; the name never adds to or replaces them.
        assert_eq!(of("haze_shared_bomb.vmdl_c"), ["Mina", "Vyper"]);
        // A whole word only.
        assert!(of("hazelnut_tree.vmdl_c").is_empty());
        // Two heroes named: the one the name starts with owns the file; with neither leading, both stay.
        let named = |p: &str| t.named_in_file(p).into_iter().collect::<Vec<_>>();
        if !named("models/abilities/mina_orb.vmdl_c").is_empty() {
            assert_eq!(named("models/abilities/mina_haze_orb.vmdl_c"), ["Mina"]);
            assert_eq!(named("models/abilities/orb_haze_mina.vmdl_c"), ["Haze", "Mina"]);
        }
        // Nowhere else: other models and other kinds keep the folder rule.
        assert!(t.named_in_file("models/props/haze_poster.vmdl_c").is_empty());
        let c = classify(&t, &["models/props/haze_poster.vmdl_c", "sounds/ui/haze_menu.vsnd_c", "particles/generic/haze_mist.vpcf_c"], None);
        assert!(c.heroes.is_empty() && c.unattributed == 3);
    }

    fn entry(path: &str, crc: u32, length: u32, preload: usize, archive: u16, offset: u32) -> Entry {
        Entry { path: path.into(), crc, archive, offset, length, preload: vec![7; preload] }
    }

    #[test]
    fn the_identity_follows_the_contents_not_the_layout() {
        let a = [entry("models/a.vmdl_c", 1, 100, 0, 0x7fff, 0), entry("sounds/b.vsnd_c", 2, 50, 0, 0x7fff, 100)];
        // The same files in another order, another archive layout and preload split, other case.
        let b = [entry("Sounds/B.vsnd_c", 2, 40, 10, 0, 900), entry("models/a.vmdl_c", 1, 100, 0, 3, 0)];
        assert_eq!(identity(&a), identity(&b));
        assert!(identity(&a).starts_with("vpk-tree-v1:"));
        let changed = [entry("models/a.vmdl_c", 9, 100, 0, 0x7fff, 0), entry("sounds/b.vsnd_c", 2, 50, 0, 0x7fff, 100)];
        assert_ne!(identity(&a), identity(&changed));
        let fewer = [entry("models/a.vmdl_c", 1, 100, 0, 0x7fff, 0)];
        assert_ne!(identity(&a), identity(&fewer));
    }
}
