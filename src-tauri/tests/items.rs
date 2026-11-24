//! Extras → Items commands: nothing about items runs until the Items tab asks
//! for the catalog; the catalog is kept per game archive and its pictures are
//! previewed through `preview_extra` with kind `items`. One test, because the
//! kept catalog is shared by the whole process.
use deadlock_core::safety;
use deadlock_desktop::{app::Application, catalogs::ExtraPreviewRequest, items};
use deadlock_project::{Project, Settings};
use std::path::Path;

const HERO: &str = "models/heroes_staging/hornet_v3/hornet.vmdl_c";
const ICON: &str = "panorama/images/heroes/vindicta_png.vtex_c";
const ITEM: &str = "panorama/images/items/weapon/basic_magazine_psd.vtex_c";

/// A made-up English names file.
const NAMES: &str = "\u{feff}\"lang\"\r\n{\r\n\t\"Tokens\"\r\n\t{\r\n\t\t\"upgrade_sample_clip\"\t\"Sample Clip\"\r\n\t\t\"upgrade_sample_clip_search\"\t\"Sample Clip SC\"\r\n\t}\r\n}\r\n";

fn request(target: &str) -> ExtraPreviewRequest {
    ExtraPreviewRequest { hero_path: String::new(), target: target.into(), kind: "items".into(), source: None, fit: None, audio_fit: None, max_dimension: Some(96) }
}

#[test]
fn the_item_catalog_is_read_only_when_the_items_tab_asks() {
    let temp = tempfile::tempdir().unwrap();
    let app = Application::new(temp.path().join("state")).unwrap();
    // No game archive yet: the page says what to set up.
    let error = app.item_catalog(false).unwrap_err();
    assert!(error.to_string().contains(items::ITEMS_NO_GAME), "{error}");
    assert!(error.help.is_some(), "every Items message a user can act on has help");

    let picture = std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("crates/deadlock-components/tests/fixtures/icons/embedded.vtex_c")).unwrap();
    let tree = temp.path().join("tree");
    let defs = br#"{ upgrade_clip_size = { m_eAbilityType = "EAbilityType_Item" m_eItemSlotType = "EItemSlotType_WeaponMod" m_iItemTier = "EModTier_1" m_strShopIconLarge = "file://{images}/items/weapon/basic_magazine.psd" } }"#;
    let files: [(&str, &[u8]); 4] = [(HERO, b"not a model"), (ICON, &picture), (ITEM, &picture), ("scripts/abilities.vdata", defs)];
    for (path, raw) in files {
        safety::atomic_write(&tree.join(path), raw).unwrap();
    }
    let pak = temp.path().join("pak18_dir.vpk");
    deadlock_vpk::write_vpk(&tree, &files.map(|f| f.0.to_string()), &pak, None, 2, &mut || Ok(())).unwrap();
    let mut settings = Settings::default();
    settings.pak = pak.display().to_string();
    app.save_settings(&settings).unwrap();
    let mut project = Project::default();
    project.original_model = HERO.into();

    // Loading the character's assets (and opening Extras) reads nothing about items.
    let catalog = app.discover_extras(project).unwrap();
    assert!(catalog.icons.iter().all(|r| r.target != ITEM));
    assert!(!items::loaded(), "the item catalog is built only by the Items tab");
    assert_eq!(deadlock_components::item_names::file_reads(), 0, "nor are the game's names files read");
    let error = app.preview_extra(request(ITEM)).unwrap_err();
    assert!(error.to_string().contains(items::ITEMS_NOT_LOADED), "{error}");
    assert!(error.help.is_some());

    // The Items tab asks: the catalog is read and kept. It is not an operation
    // (no lease), so it never waits for or blocks hero asset loading or a build.
    let before = app.get_timings().unwrap().run_id;
    let items_catalog = app.item_catalog(false).unwrap();
    assert!(items::loaded());
    assert_eq!(items_catalog.items.len(), 1);
    assert_eq!(items_catalog.items[0].category, "weapon");
    assert_eq!(app.item_catalog(false).unwrap(), items_catalog, "kept, not read again");
    assert_eq!(app.get_timings().unwrap().run_id, before, "no operation ran");
    let reading = std::thread::scope(|s| {
        let a = s.spawn(|| app.item_catalog(true));
        let b = s.spawn(|| app.item_catalog(false));
        (a.join().unwrap(), b.join().unwrap())
    });
    assert!(reading.0.is_ok() && reading.1.is_ok(), "two reads at once both succeed");

    // Pictures of the list only, from the kept archive.
    let preview = app.preview_extra(request(ITEM)).unwrap();
    let header = u32::from_le_bytes(preview[..4].try_into().unwrap()) as usize;
    assert!(preview[4 + header..].starts_with(b"\x89PNG"));
    assert!(app.preview_extra(request(ICON)).is_err(), "a hero picture is not an item picture");

    // Another game archive in Settings asks for Reload assets, which reads it.
    let other = temp.path().join("pak19_dir.vpk");
    deadlock_vpk::write_vpk(&tree, &files.map(|f| f.0.to_string()), &other, None, 2, &mut || Ok(())).unwrap();
    settings.pak = other.display().to_string();
    app.save_settings(&settings).unwrap();
    let error = app.preview_extra(request(ITEM)).unwrap_err();
    assert!(error.to_string().contains(items::ITEMS_GAME_CHANGED), "{error}");
    assert!(error.help.is_some());
    app.item_catalog(true).unwrap();
    assert!(app.preview_extra(request(ITEM)).is_ok());

    // Renamed items: the page asks for the game's names and the saved rows' state (no operation).
    use deadlock_project::item_names::ItemNameRow;
    let rows = [ItemNameRow { item: "Upgrade_Sample_Clip".into(), enabled: true, name: "Longer Clip".into() }, ItemNameRow { item: "upgrade_gone".into(), enabled: false, name: "Gone".into() }];
    let before = app.get_timings().unwrap().run_id;
    let none = app.item_names_state(&rows).unwrap();
    assert!(!none.game.found && none.unknown.len() == 2, "{none:?}");
    let folder = deadlock_components::item_names::folder(&other);
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("citadel_gc_mod_names_english.txt"), NAMES).unwrap();
    let state = app.item_names_state(&rows).unwrap();
    assert!(state.game.found, "a changed names folder is read again");
    assert_eq!(state.game.names["upgrade_sample_clip"], "Sample Clip");
    assert_eq!(state.game.languages, ["english"]);
    assert!(state.rows[0].known && !state.rows[1].known);
    assert_eq!(state.unknown, ["upgrade_gone"]);
    assert_eq!(app.get_timings().unwrap().run_id, before, "no operation ran");
}
