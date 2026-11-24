//! Extras → Items: the item catalog from the game's item definitions, and item
//! icon rows through staging, packaging, the frozen-build check and install.
use deadlock_components::{icons, item_icons, service::ComponentService};
use deadlock_core::{
    CancellationToken, TimingRecorder,
    process::{Runner, RunnerOptions},
    safety,
};
use deadlock_project::{Project, Settings, customization::ResourceReplacement};
use deadlock_vpk::{Vpk, write_vpk};
use std::{
    fs,
    path::{Path, PathBuf},
};

const HERO: &str = "models/heroes_staging/hornet_v3/hornet.vmdl";
const MAGAZINE: &str = "panorama/images/items/weapon/basic_magazine_psd.vtex_c";

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}

/// A Panorama RGBA texture (`icons` tests' synthetic layout), replaceable
/// without the resource compiler.
fn texture(size: u16) -> Vec<u8> {
    let mut data = vec![0u8; 40 + 24 + 1024 + 128];
    data[..2].copy_from_slice(&1u16.to_le_bytes());
    data[2..4].copy_from_slice(&395u16.to_le_bytes());
    data[20..22].copy_from_slice(&size.to_le_bytes());
    data[22..24].copy_from_slice(&size.to_le_bytes());
    data[24..26].copy_from_slice(&1u16.to_le_bytes());
    data[26] = 4;
    data[27] = 1;
    data[32..36].copy_from_slice(&8u32.to_le_bytes());
    data[36..40].copy_from_slice(&2u32.to_le_bytes());
    for (at, kind, offset, length) in [(40, 1u32, 20u32, 1024u32), (52, 3, 1032, 128)] {
        data[at..at + 4].copy_from_slice(&kind.to_le_bytes());
        data[at + 4..at + 8].copy_from_slice(&offset.to_le_bytes());
        data[at + 8..at + 12].copy_from_slice(&length.to_le_bytes());
    }
    data[1090..1092].copy_from_slice(&size.to_le_bytes());
    data[1092..1094].copy_from_slice(&size.to_le_bytes());
    let mut raw = vec![];
    raw.extend((28 + data.len() as u32).to_le_bytes());
    raw.extend(12u16.to_le_bytes());
    raw.extend(1u16.to_le_bytes());
    raw.extend(8u32.to_le_bytes());
    raw.extend(1u32.to_le_bytes());
    raw.extend(b"DATA");
    raw.extend(8u32.to_le_bytes());
    raw.extend((data.len() as u32).to_le_bytes());
    raw.extend(data);
    raw.extend(vec![200u8; usize::from(size) * usize::from(size) * 4]);
    raw
}

/// The game's item definitions as KV3 text (the compiled file holds the same
/// fields): sold items in each slot, a disabled one stored as the number 1, an
/// item in development, a Street Brawl item, a disabled Street Brawl item, a
/// base record, test items sharing a sold item's picture, and ability images.
const ABILITIES: &str = r#"{
    weapon_upgrade_t1 = { _not_pickable = 2 m_eAbilityType = "EAbilityType_Item" m_eItemSlotType = "EItemSlotType_WeaponMod" m_iItemTier = "EModTier_1" }
    upgrade_clip_size = { m_eAbilityType = "EAbilityType_Item" m_eItemSlotType = "EItemSlotType_WeaponMod" m_iItemTier = "EModTier_1" m_strShopIconLarge = "file://{images}/items/weapon/basic_magazine.psd" m_strAbilityImage = "file://{images}/upgrades/mods_weapon/clip_size.psd" }
    upgrade_close_range = { m_eAbilityType = "EAbilityType_Item" m_eItemSlotType = "EItemSlotType_WeaponMod" m_iItemTier = "EModTier_1" m_strShopIconLarge = "file://{images}/items/weapon/close_quarters.psd" m_strAbilityImage = "file://{images}/hud/abilities/lash/lash_death_slam.psd" }
    item_projectile_test_01 = { m_bDisabled = true m_eAbilityType = "EAbilityType_Item" m_eItemSlotType = "EItemSlotType_WeaponMod" m_iItemTier = "EModTier_1" m_strShopIconLarge = "file://{images}/items/weapon/basic_magazine.psd" }
    upgrade_health = { m_eAbilityType = "EAbilityType_Item" m_eItemSlotType = "EItemSlotType_Armor" m_iItemTier = "EModTier_4" m_strShopIconLarge = "file://{images}/items/vitality/extra_health.psd" m_strAbilityImage = "file://{images}/upgrades/mods_armor/bullet_shield.psd" }
    upgrade_grit = { m_eAbilityType = "EAbilityType_Item" m_eItemSlotType = "EItemSlotType_Armor" m_iItemTier = "EModTier_1" m_strShopIconLarge = "file://{images}/items/vitality/grit.psd" m_strAbilityImage = "file://{images}/upgrades/mods_armor/bullet_shield.psd" }
    upgrade_high_impact_armor = { m_bDisabled = 1 m_eAbilityType = "EAbilityType_Item" m_eItemSlotType = "EItemSlotType_Armor" m_iItemTier = "EModTier_2" m_strShopIconLarge = "file://{images}/items/vitality/high_impact.psd" m_strAbilityImage = "file://{images}/upgrades/mods_armor/bullet_armor.psd" }
    upgrade_improved_spirit = { m_eAbilityType = "EAbilityType_Item" m_eItemSlotType = "EItemSlotType_Tech" m_iItemTier = "EModTier_2" m_strShopIconLarge = "file://{images}/items/spirit/extra_spirit.psd" }
    upgrade_spellshield = { m_bInDevelopment = true m_eAbilityType = "EAbilityType_Item" m_eItemSlotType = "EItemSlotType_Tech" m_iItemTier = "EModTier_3" m_strShopIconLarge = "file://{images}/items/spirit/spell_shield.psd" }
    upgrade_icarus_wings = { m_eAbilityRequirements = "ERequirementStreetBrawl" m_eAbilityType = "EAbilityType_Item" m_eItemSlotType = "EItemSlotType_Armor" m_iItemTier = "EModTier_5" m_strShopIconLarge = "file://{images}/items/brawl/icarus_wings.psd" }
    upgrade_apex_combat = { m_bDisabled = true m_eAbilityRequirements = "ERequirementStreetBrawl" m_eAbilityType = "EAbilityType_Item" m_eItemSlotType = "EItemSlotType_WeaponMod" m_iItemTier = "EModTier_5" m_strShopIconLarge = "file://{images}/items/brawl/apex_combat.psd" }
    hornet_snipe = { m_eAbilityType = "EAbilityType_Signature" m_strAbilityImage = "file://{images}/hud/abilities/hornet/snipe.psd" }
}"#;
const GENERIC: &str = r#"{ m_nItemPricePerTier = [ 0, 800, 1600, 3200, 6400, 9999 ] }"#;
const NAMES: &str = "\u{feff}\"lang\"\n{\n\t\"Tokens\"\n\t{\n\t\t\"upgrade_clip_size\"\t\"Extended Magazine\"\n\t\t\"upgrade_close_range\"\t\"Close Quarters\"\n\t\t\"upgrade_health\"\t\"Extra Health\"\n\t\t\"upgrade_grit\"\t\"Grit\"\n\t\t\"upgrade_icarus_wings\"\t\"Seraphim Wings\"\n\t}\n}\n";

/// The writer refuses the game's own name, so the archive is written as
/// another number and renamed (the names file is looked up beside it).
fn write(tree: &Path, paths: &[String], pak: &Path) {
    let spare = pak.with_file_name("pak42_dir.vpk");
    write_vpk(tree, paths, &spare, None, 2, &mut || Ok(())).unwrap();
    fs::rename(&spare, pak).unwrap();
}
fn game(root: &Path, extra: &[(&str, Vec<u8>)]) -> PathBuf {
    let tree = root.join("game-source");
    // The hero model and its files from the shared component fixture.
    let base = Vpk::open(fixture("components_dir.vpk")).unwrap();
    let mut paths = vec![];
    for entry in base.entries.values() {
        safety::atomic_write(&tree.join(&entry.path), base.read(&entry.path).unwrap()).unwrap();
        paths.push(entry.path.clone());
    }
    let mut files: Vec<(&str, Vec<u8>)> = vec![
        ("scripts/abilities.vdata", ABILITIES.as_bytes().to_vec()),
        ("scripts/generic_data.vdata", GENERIC.as_bytes().to_vec()),
        (MAGAZINE, texture(64)),
        ("panorama/images/items/weapon/close_quarters_psd.vtex_c", texture(64)),
        ("panorama/images/items/vitality/extra_health_psd.vtex_c", texture(64)),
        ("panorama/images/items/vitality/grit_psd.vtex_c", texture(64)),
        ("panorama/images/items/vitality/high_impact_psd.vtex_c", texture(64)),
        ("panorama/images/items/spirit/extra_spirit_psd.vtex_c", texture(64)),
        ("panorama/images/items/spirit/spell_shield_psd.vtex_c", texture(64)),
        ("panorama/images/items/brawl/icarus_wings_psd.vtex_c", texture(64)),
        ("panorama/images/items/brawl/apex_combat_psd.vtex_c", texture(64)),
        ("panorama/images/items/seasonal/2025_snowball_psd.vtex_c", texture(64)),
        ("panorama/images/items/weapon/old_test_psd.vtex_c", texture(32)),
        ("panorama/images/upgrades/mods_weapon/clip_size_psd.vtex_c", texture(32)),
        ("panorama/images/upgrades/mods_armor/bullet_shield_psd.vtex_c", texture(32)),
        ("panorama/images/upgrades/mods_armor/bullet_armor_psd.vtex_c", texture(32)),
        ("panorama/images/upgrades/mods_tech/unused_glyph_psd.vtex_c", texture(32)),
        ("panorama/images/hud/abilities/lash/lash_death_slam_psd.vtex_c", texture(32)),
    ];
    files.retain(|(path, _)| extra.iter().all(|(other, _)| other != path));
    files.extend(extra.iter().cloned());
    for (path, raw) in &files {
        safety::atomic_write(&tree.join(path), raw).unwrap();
        paths.push((*path).to_string());
    }
    let pak = root.join("game/citadel/pak01_dir.vpk");
    fs::create_dir_all(pak.parent().unwrap()).unwrap();
    write(&tree, &paths, &pak);
    let names = root.join("game/citadel/resource/localization").join(item_icons::NAME_FILE);
    fs::create_dir_all(names.parent().unwrap()).unwrap();
    fs::write(names, NAMES).unwrap();
    pak
}

#[test]
fn the_catalog_follows_the_game_definitions() {
    let temp = tempfile::tempdir().unwrap();
    let pak = game(temp.path(), &[]);
    let archive = Vpk::open(&pak).unwrap();
    let cancel = CancellationToken::new();
    let catalog = item_icons::catalog(&archive, &item_icons::Names::beside(&pak), &cancel).unwrap();
    assert!(catalog.names_found);
    assert_eq!(catalog.prices, [0, 800, 1600, 3200, 6400, 9999]);
    let row = |path: &str| catalog.items.iter().find(|r| r.target == path).unwrap_or_else(|| panic!("{path} missing"));
    let place = |path: &str| {
        let r = row(path);
        (r.category.as_str(), r.group.as_str(), r.price, r.label.as_str())
    };
    // Sold items: slot → category, tier and the tier's price from the price table.
    assert_eq!(place(MAGAZINE), ("weapon", "1", Some(800), "Extended Magazine"));
    assert_eq!(place("panorama/images/items/vitality/extra_health_psd.vtex_c"), ("vitality", "4", Some(6400), "Extra Health"));
    assert_eq!(place("panorama/images/items/spirit/extra_spirit_psd.vtex_c"), ("spirit", "2", Some(1600), "Improved spirit"), "no English name: the internal name made readable");
    // A sold item owns a picture test items share; every record is listed, the owner first.
    assert_eq!(row(MAGAZINE).items, ["upgrade_clip_size", "item_projectile_test_01"]);
    // Street Brawl: Legendary, without a price (the table's 9,999 is a placeholder).
    assert_eq!(place("panorama/images/items/brawl/icarus_wings_psd.vtex_c"), ("legendary", "5", None, "Seraphim Wings"));
    // A Street Brawl item the game switches off: Legendary, its own group after the others.
    let apex = row("panorama/images/items/brawl/apex_combat_psd.vtex_c");
    assert_eq!((apex.category.as_str(), apex.group.as_str(), apex.price, apex.switched_off), ("legendary", "off", None, true));
    assert!(!row("panorama/images/items/brawl/icarus_wings_psd.vtex_c").switched_off);
    // Misc: other items switched off (disabled as true or 1, in development).
    for path in [
        "panorama/images/items/vitality/high_impact_psd.vtex_c",
        "panorama/images/items/spirit/spell_shield_psd.vtex_c",
    ] {
        assert_eq!((row(path).category.as_str(), row(path).group.as_str(), row(path).switched_off), ("misc", "unsold", true), "{path}");
    }
    let legendary: Vec<&str> = catalog.items.iter().filter(|r| r.category == "legendary").map(|r| r.group.as_str()).collect();
    assert_eq!(legendary, ["5", "off"], "switched-off Legendary items come last");
    assert_eq!(row("panorama/images/items/vitality/high_impact_psd.vtex_c").slot.as_deref(), Some("vitality"));
    assert_eq!(place("panorama/images/items/seasonal/2025_snowball_psd.vtex_c"), ("misc", "seasonal", None, "2025 snowball"));
    assert_eq!(place("panorama/images/items/weapon/old_test_psd.vtex_c"), ("misc", "unused", None, "Old test"));
    // The older item glyphs are not offered, even those current items still name, and
    // hero ability pictures are not item icons.
    for absent in [
        "panorama/images/upgrades/mods_armor/bullet_shield_psd.vtex_c",
        "panorama/images/upgrades/mods_weapon/clip_size_psd.vtex_c",
        "panorama/images/upgrades/mods_armor/bullet_armor_psd.vtex_c",
        "panorama/images/upgrades/mods_tech/unused_glyph_psd.vtex_c",
        "panorama/images/hud/abilities/lash/lash_death_slam_psd.vtex_c",
    ] {
        assert!(catalog.items.iter().all(|r| r.target != absent), "{absent}");
    }
    // Page order: category, tier or Misc group, then name.
    let order: Vec<(&str, &str, &str)> = catalog.items.iter().map(|r| (r.category.as_str(), r.group.as_str(), r.label.as_str())).collect();
    assert_eq!(&order[..2], [("weapon", "1", "Close Quarters"), ("weapon", "1", "Extended Magazine")]);
    assert_eq!(order.last().unwrap().1, "unused");
    for row in &catalog.items {
        assert!(row.supported, "{}: {}", row.target, row.reason);
        assert_eq!(row.method.as_deref(), Some("panorama_texture"));
        assert!(!row.shared, "item icons never ask for shared-resource consent");
        assert_eq!(row.association, "item");
    }
    // The build's allow-list is the same set of pictures, read without the textures.
    let allowed = item_icons::allowed(&archive, &cancel).unwrap();
    assert_eq!(allowed, catalog.items.iter().map(|r| r.target.to_lowercase()).collect());
    // Without English names the internal names are shown, with a note.
    let plain = item_icons::catalog(&archive, &item_icons::Names::default(), &cancel).unwrap();
    assert_eq!(plain.items.iter().find(|r| r.target == MAGAZINE).unwrap().label, "Clip size");
    assert!(!plain.warnings.is_empty());
}

fn service_parts(root: &Path, pak: &Path) -> (Settings, Runner, CancellationToken, TimingRecorder) {
    let settings = Settings {
        workspace: root.join("workspace").to_string_lossy().into(),
        pak: pak.to_string_lossy().into(),
        ..Default::default()
    };
    let cancel = CancellationToken::new();
    let runner = Runner::new(root.join("logs"), cancel.clone(), RunnerOptions::default()).unwrap();
    (settings, runner, cancel, TimingRecorder::new("item icons"))
}
fn item_row(target: &str) -> ResourceReplacement {
    ResourceReplacement {
        enabled: Some(true),
        source: Some(fixture("icons/profile.png").to_string_lossy().into()),
        target: Some(target.into()),
        fit: Some("cover".into()),
        allow_shared: Some(false),
        ..Default::default()
    }
}

#[test]
fn item_icon_rows_build_freeze_and_install_with_and_without_a_replacement_model() {
    for model_enabled in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let pak = game(temp.path(), &[]);
        let (settings, runner, cancel, clock) = service_parts(temp.path(), &pak);
        let log = |_: &str| {};
        let service = ComponentService { settings: &settings, runner: &runner, cancel: &cancel, clock: &clock, log: &log };
        let mut project = Project::default();
        project.original_model = HERO.into();
        project.customization.enabled = Some(true);
        project.customization.model_enabled = Some(model_enabled);
        project.customization.item_icons = Some(vec![item_row(MAGAZINE)]);
        let inspection = service.inspect(&project).unwrap();
        assert_eq!(inspection.item_icons.len(), 1);
        assert!(inspection.icons.is_empty(), "never an Icons entry");
        assert!(!inspection.native_compile_required, "item icons need no resource compiler");
        let output = temp.path().join("output");
        let receipt = service.compile(&project, &output).unwrap().unwrap();
        assert_eq!(receipt.files.keys().collect::<Vec<_>>(), [MAGAZINE]);
        assert_eq!(receipt.item_icons.entries.len(), 1);
        assert!(receipt.icons.entries.is_empty());
        assert!(receipt.item_icons.entries[0].metadata_verified);
        assert!(!receipt.item_icons.entries[0].allow_shared);
        service.verify_snapshot(&project, &receipt, &output).unwrap();
        // The packaged picture is the user's image with the game's texture settings.
        let packaged = fs::read(output.join(MAGAZINE)).unwrap();
        let original = Vpk::open(&pak).unwrap().read(MAGAZINE).unwrap();
        assert_ne!(packaged, original);
        let (a, b) = (icons::texture_metadata(&packaged).unwrap(), icons::texture_metadata(&original).unwrap());
        assert_eq!((a.width, a.height, a.display_width, a.display_height, a.mips), (b.width, b.height, b.display_width, b.display_height, b.mips));
        let package = temp.path().join("pak18_dir.vpk");
        write_vpk(&output, &receipt.files.keys().cloned().collect::<Vec<_>>(), &package, Some(&receipt.files), 2, &mut || Ok(())).unwrap();
        let candidate = Vpk::open(&package).unwrap();
        let current = Vpk::open(&pak).unwrap();
        service.verify_install(&project, &receipt, &candidate, &current).unwrap();
        // A receipt written before item icons existed still reads (no item_icons field).
        let mut old = serde_json::to_value(&receipt).unwrap();
        old.as_object_mut().unwrap().remove("item_icons");
        let old: deadlock_components::service::ComponentReceipt = serde_json::from_value(old).unwrap();
        assert!(old.item_icons.entries.is_empty());
        // A project without ability objects: no ability_objects key in the receipt or the
        // inspection (so they stay byte for byte), and the object catalog is never read.
        assert!(serde_json::to_value(&receipt).unwrap().get("ability_objects").is_none());
        assert!(serde_json::to_value(&inspection).unwrap().get("ability_objects").is_none());
        assert!(!serde_json::to_string(&project).unwrap().contains("ability_objects"));
        assert_eq!(deadlock_components::ability_objects::catalog_reads(), 0);
        // Nor renamed items: no item_names key, and the game's names files are never read.
        assert!(serde_json::to_value(&receipt).unwrap().get("item_names").is_none());
        assert!(serde_json::to_value(&inspection).unwrap().get("item_names").is_none());
        assert!(!serde_json::to_string(&project).unwrap().contains("item_names"));
        assert_eq!(deadlock_components::item_names::file_reads(), 0);
        // Hero model changes keep item icons: they are the same for every hero.
        assert!(clock.report().steps.iter().any(|s| s.name == "Stage item icons"));
        // A game update whose item no longer names the picture keeps it an item
        // picture (Misc), so the installed mod stays valid ...
        let tree = temp.path().join("updated/game-source");
        let changed = game(&temp.path().join("updated"), &[]);
        let defs = ABILITIES.replace("items/weapon/basic_magazine.psd", "items/weapon/close_quarters.psd");
        fs::write(tree.join("scripts/abilities.vdata"), defs).unwrap();
        let paths: Vec<String> = Vpk::open(&changed).unwrap().entries.values().map(|e| e.path.clone()).collect();
        write(&tree, &paths, &changed);
        service.verify_install(&project, &receipt, &candidate, &Vpk::open(&changed).unwrap()).unwrap();
        // ... while an update of the picture itself asks for a new build.
        let updated = game(&temp.path().join("redrawn"), &[(MAGAZINE, texture(128))]);
        assert!(service.verify_install(&project, &receipt, &candidate, &Vpk::open(&updated).unwrap()).is_err());
    }
}

#[test]
fn only_item_pictures_are_accepted_and_hero_icons_keep_their_checks() {
    let temp = tempfile::tempdir().unwrap();
    let pak = game(temp.path(), &[]);
    let (settings, runner, cancel, clock) = service_parts(temp.path(), &pak);
    let log = |_: &str| {};
    let service = ComponentService { settings: &settings, runner: &runner, cancel: &cancel, clock: &clock, log: &log };
    let mut project = Project::default();
    project.original_model = HERO.into();
    project.customization.enabled = Some(true);
    project.customization.model_enabled = Some(false);
    // A hero ability picture, older glyphs (named by a current item or by none) and a missing file.
    for target in [
        "panorama/images/hud/abilities/lash/lash_death_slam_psd.vtex_c",
        "panorama/images/upgrades/mods_tech/unused_glyph_psd.vtex_c",
        "panorama/images/upgrades/mods_armor/bullet_shield_psd.vtex_c",
        "panorama/images/items/weapon/not_there_psd.vtex_c",
    ] {
        project.customization.item_icons = Some(vec![item_row(target)]);
        let error = service.inspect(&project).unwrap_err().to_string();
        assert!(error.contains(item_icons::NOT_AN_ITEM_ICON), "{target}: {error}");
    }
    // A switched-off Legendary item is accepted (it may come back), with no shared-resource tick.
    project.customization.item_icons = Some(vec![item_row("panorama/images/items/brawl/apex_combat_psd.vtex_c")]);
    assert_eq!(service.inspect(&project).unwrap().item_icons.len(), 1);
    // The game's picture changed after it was chosen.
    let mut stale = item_row(MAGAZINE);
    stale.original_sha256 = Some("0".repeat(64));
    project.customization.item_icons = Some(vec![stale]);
    assert!(service.inspect(&project).unwrap_err().to_string().contains(item_icons::ITEM_ICON_CHANGED));
    // An empty row.
    project.customization.item_icons = Some(vec![ResourceReplacement { target: Some(MAGAZINE.into()), ..Default::default() }]);
    assert!(service.inspect(&project).unwrap_err().to_string().contains(item_icons::EMPTY_ROW));
    // An item picture saved as an Icons row still goes through the hero's checks:
    // the item list never widens what Icons accepts.
    project.customization.item_icons = None;
    project.customization.icons = Some(vec![item_row(MAGAZINE)]);
    let error = service.inspect(&project).unwrap_err().to_string();
    assert!(error.contains("Choose an image discovered for the selected hero"), "{error}");
}

#[test]
fn projects_without_item_icons_never_read_the_item_list() {
    // A game without item definitions at all: a hero-only build must not notice.
    let temp = tempfile::tempdir().unwrap();
    let (settings, runner, cancel, clock) = service_parts(temp.path(), &fixture("components_dir.vpk"));
    let archive = Vpk::open(fixture("components_dir.vpk")).unwrap();
    assert!(!archive.entries.contains_key("scripts/abilities.vdata_c") && !archive.entries.contains_key("scripts/abilities.vdata"));
    assert!(item_icons::allowed(&archive, &cancel).unwrap_err().to_string().contains(item_icons::ITEM_ICONS_UNREADABLE));
    let log = |_: &str| {};
    let service = ComponentService { settings: &settings, runner: &runner, cancel: &cancel, clock: &clock, log: &log };
    let mut project = Project::default();
    project.original_model = HERO.into();
    project.customization.enabled = Some(true);
    project.customization.model_enabled = Some(false);
    project.customization.audio = Some(vec![ResourceReplacement {
        source: Some(fixture("samples.wav").to_string_lossy().into()),
        target: Some("sounds/heroes/hornet/rifle.vsnd_c".into()),
        ..Default::default()
    }]);
    // A paused item row counts as none.
    project.customization.item_icons = Some(vec![ResourceReplacement { enabled: Some(false), ..item_row(MAGAZINE) }]);
    let output = temp.path().join("output");
    let receipt = service.compile(&project, &output).unwrap().unwrap();
    assert!(receipt.item_icons.entries.is_empty());
    service.verify_snapshot(&project, &receipt, &output).unwrap();
    assert!(clock.report().steps.iter().all(|s| s.name != "Stage item icons"));
    assert!(item_icons::stage(&archive, &[], &temp.path().join("content"), &cancel).unwrap().entries.is_empty());
}
