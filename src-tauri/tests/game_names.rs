//! `game_names`: hero and item names in an interface language, read from the
//! names files beside the game archive. Lease-free, read-only, never an error,
//! and independent of the Items catalog. One test, because the kept answers
//! are shared by the whole process.
use deadlock_core::safety;
use deadlock_desktop::app::Application;
use deadlock_project::Settings;
use std::path::Path;

const ITEM: &str = "panorama/images/items/weapon/basic_magazine_psd.vtex_c";

fn names(language: &str, rows: &[(&str, &str)]) -> String {
    let mut text = format!("\u{feff}\"lang\"\r\n{{\r\n\t\"Language\"\t\t\"{language}\"\r\n\t\"Tokens\"\r\n\t{{\r\n");
    for (key, value) in rows {
        text.push_str(&format!("\t\t\"{key}\" \"{value}\"\r\n"));
    }
    text.push_str("\t}\r\n}\r\n");
    text
}

#[test]
fn game_names_reads_the_names_files_and_leaves_the_item_catalog_alone() {
    let temp = tempfile::tempdir().unwrap();
    let app = Application::new(temp.path().join("state")).unwrap();
    // No game archive: empty, not an error.
    let none = app.game_names("zh-CN").unwrap();
    assert!(none.heroes.is_empty() && none.items.is_empty() && none.language == "zh-CN");

    let picture = std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("crates/deadlock-components/tests/fixtures/icons/embedded.vtex_c")).unwrap();
    let tree = temp.path().join("tree");
    let abilities = br#"{ upgrade_sample_clip = { m_eAbilityType = "EAbilityType_Item" m_eItemSlotType = "EItemSlotType_WeaponMod" m_iItemTier = "EModTier_1" m_strShopIconLarge = "file://{images}/items/weapon/basic_magazine.psd" } }"#;
    // A made-up definition key loading a roster model.
    let heroes = br#"{ hero_sample_key = { m_strModelName = "models/heroes_wip/abrams/abrams.vmdl" } }"#;
    let files: [(&str, &[u8]); 3] = [(ITEM, &picture), ("scripts/abilities.vdata", abilities), ("scripts/heroes.vdata", heroes)];
    for (path, raw) in files {
        safety::atomic_write(&tree.join(path), raw).unwrap();
    }
    let game = temp.path().join("game");
    let pak = game.join("pak18_dir.vpk");
    std::fs::create_dir_all(&game).unwrap();
    deadlock_vpk::write_vpk(&tree, &files.map(|f| f.0.to_string()), &pak, None, 2, &mut || Ok(())).unwrap();
    let mut settings = Settings::default();
    settings.pak = pak.display().to_string();
    app.save_settings(&settings).unwrap();

    let localization = game.join("resource").join("localization");
    let write = |folder: &str, file: &str, text: String| {
        std::fs::create_dir_all(localization.join(folder)).unwrap();
        std::fs::write(localization.join(folder).join(file), text).unwrap();
    };
    write("citadel_gc_mod_names", "citadel_gc_mod_names_english.txt", names("english", &[("upgrade_sample_clip", "Sample Clip")]));
    let catalog = app.item_catalog(false).unwrap();
    write("citadel_gc_mod_names", "citadel_gc_mod_names_schinese.txt", names("schinese", &[("upgrade_sample_clip", "样本弹匣")]));
    write("citadel_gc_hero_names", "citadel_gc_hero_names_english.txt", names("english", &[("hero_sample_key:n", "Sample Hero")]));
    write("citadel_gc_hero_names", "citadel_gc_hero_names_schinese.txt", names("schinese", &[("hero_sample_key:n", "样本英雄")]));

    let run = app.get_timings().map(|t| t.run_id);
    let answer = app.game_names("zh-CN").unwrap();
    assert_eq!(answer.language, "zh-CN");
    // The roster's English name is the key, joined through the definition key.
    assert_eq!(answer.heroes.get("Abrams").map(String::as_str), Some("样本英雄"));
    assert_eq!(answer.heroes.len(), 1);
    assert_eq!(answer.items.get("upgrade_sample_clip").map(String::as_str), Some("样本弹匣"));
    assert!(!answer.missing.contains(&"Abrams".to_string()) && !answer.missing.is_empty());
    assert_eq!(app.game_names("zh-CN").unwrap(), answer, "kept, the same answer");
    assert_eq!(app.game_names("zh-cn").unwrap().heroes, answer.heroes);

    // English, an empty id and an unknown id: empty maps.
    for id in ["en", "", "xx"] {
        let empty = app.game_names(id).unwrap();
        assert!(empty.heroes.is_empty() && empty.items.is_empty() && empty.missing.is_empty(), "{id}: {empty:?}");
        assert_eq!(empty.language, id);
    }

    // A changed language file is read again.
    write("citadel_gc_hero_names", "citadel_gc_hero_names_schinese.txt", names("schinese", &[("hero_sample_key:n", "另一个名字")]));
    assert_eq!(app.game_names("zh-CN").unwrap().heroes.get("Abrams").map(String::as_str), Some("另一个名字"));

    // No operation ran, the names files only were read, the item catalog is as before.
    assert_eq!(app.get_timings().map(|t| t.run_id), run, "no operation ran");
    assert_eq!(app.item_catalog(false).unwrap(), catalog);
    assert_eq!(app.item_catalog(true).unwrap(), catalog, "read again, the same catalog");
    assert_eq!(deadlock_components::item_names::file_reads(), 0, "the Items names reader was not used");
    // Nothing was written beside the archive.
    let mut listed: Vec<String> = ["citadel_gc_mod_names", "citadel_gc_hero_names"].iter().flat_map(|f| std::fs::read_dir(localization.join(f)).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned())).collect();
    listed.sort();
    assert_eq!(listed, ["citadel_gc_hero_names_english.txt", "citadel_gc_hero_names_schinese.txt", "citadel_gc_mod_names_english.txt", "citadel_gc_mod_names_schinese.txt"]);
    assert_eq!(std::fs::read_dir(&game).unwrap().count(), 2, "the archive and the resource folder only");
}
