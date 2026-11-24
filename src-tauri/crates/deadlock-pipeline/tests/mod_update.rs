//! Installed-mods repair end to end on synthetic archives: a "game" VPK whose
//! hero model uses the current attachment convention and an installed mod VPK
//! compiled with the previous one. No game data is involved.
use deadlock_core::CancellationToken;
use deadlock_pipeline::mod_update::{self, Status};
use deadlock_project::Settings;
use deadlock_resource::attachments::{self, Convention};
use deadlock_resource::{binary, container, Kind, Value};
use deadlock_vpk::Vpk;
use std::collections::BTreeMap;
use std::path::Path;

const FIXTURE: &[u8] = include_bytes!("../../deadlock-cloth/tests/fixtures/tested-original.vmdl_c");
const MODEL: &str = "models/heroes_staging/haze/haze.vmdl_c";
const ROOT: [f64; 4] = [0.5, 0.5, 0.5, 0.5];
const OLD_ROT: [f64; 4] = [-0.5, -0.5, -0.5, 0.5];

fn f32s(values: &[f64]) -> Value {
    Value::new(Kind::Array(values.iter().map(|v| Value::new(Kind::F32(*v as f32))).collect()))
}
fn attachment(name: &str, bone: &str, offset: [f64; 3], rotation: [f64; 4]) -> Value {
    let value = Value::new(Kind::Object(vec![
        ("m_name".into(), Value::new(Kind::String(name.into()))),
        ("m_influenceNames".into(), Value::new(Kind::Array([bone, "", ""].iter().map(|b| Value::new(Kind::String(b.to_string()))).collect()))),
        ("m_vInfluenceRotations".into(), Value::new(Kind::Array(vec![f32s(&rotation), f32s(&[0., 0., 0., 1.]), f32s(&[0., 0., 0., 1.])]))),
        ("m_vInfluenceOffsets".into(), Value::new(Kind::Array(vec![f32s(&offset), f32s(&[0.; 3]), f32s(&[0.; 3])]))),
        ("m_influenceWeights".into(), f32s(&[1., 0., 0.])),
        ("m_nInfluences".into(), Value::new(Kind::I64(1))),
        ("m_bIgnoreRotation".into(), Value::new(Kind::Bool(false))),
    ]));
    Value::new(Kind::Object(vec![("key".into(), Value::new(Kind::String(name.into()))), ("value".into(), value)]))
}
/// The fixture with a root_motion skeleton and the given attachments in every MDAT block.
fn model(attachments: Vec<Value>) -> Vec<u8> {
    let layout = container::layout(FIXTURE).unwrap();
    let mut replacements = BTreeMap::new();
    for (ordinal, block) in layout.blocks.iter().enumerate() {
        let mut doc = binary::decode(container::payload(FIXTURE, block).unwrap()).unwrap();
        match block.name.as_str() {
            "DATA" => {
                let skeleton = Value::new(Kind::Object(vec![
                    ("m_boneName".into(), Value::new(Kind::Array(["root_motion", "spine_0"].iter().map(|n| Value::new(Kind::String(n.to_string()))).collect()))),
                    ("m_nParent".into(), Value::new(Kind::Array(vec![Value::new(Kind::I64(-1)), Value::new(Kind::I64(0))]))),
                    ("m_bonePosParent".into(), Value::new(Kind::Array(vec![f32s(&[0.; 3]), f32s(&[0., 40., 0.])]))),
                    ("m_boneRotParent".into(), Value::new(Kind::Array(vec![f32s(&ROOT), f32s(&[0., 0., 0., 1.])]))),
                ]));
                match doc.root.get_mut("m_modelSkeleton") {
                    Some(existing) => *existing = skeleton,
                    None => {
                        let Kind::Object(fields) = &mut doc.root.kind else { panic!() };
                        fields.push(("m_modelSkeleton".into(), skeleton));
                    }
                }
            }
            "MDAT" => {
                let Kind::Object(fields) = &mut doc.root.kind else { panic!() };
                fields.retain(|(k, _)| k != "m_attachments");
                fields.push(("m_attachments".into(), Value::new(Kind::Array(attachments.clone()))));
            }
            _ => continue,
        }
        replacements.insert(ordinal, binary::encode(&doc.root, doc.format_guid).unwrap());
    }
    container::rebuild_ordinals(FIXTURE, &replacements).unwrap()
}
fn current() -> Vec<u8> {
    model(vec![
        attachment("standing_pivot", "root_motion", [0., 0., 86.25], [0., 0., 0., 1.]),
        attachment("crouching_pivot", "root_motion", [0., 0., 57.5], [0., 0., 0., 1.]),
        attachment("mask", "spine_0", [3.5, 8., 0.], [0., 0., 0., 1.]),
    ])
}
fn previous() -> Vec<u8> {
    model(vec![
        attachment("standing_pivot", "root_motion", [0., 86.25, 0.], OLD_ROT),
        attachment("crouching_pivot", "root_motion", [0., 57.5, 0.], OLD_ROT),
        attachment("mask", "spine_0", [3.5, 8., 0.], [0., 0., 0., 1.]),
        attachment("my_camera", "root_motion", [10., 120., -5.], OLD_ROT),
    ])
}
/// A single-file VPK with these entries.
fn vpk(folder: &Path, name: &str, entries: &[(&str, &[u8])]) -> std::path::PathBuf {
    let stage = folder.join(format!("{name}-stage"));
    let mut files = vec![];
    for (relative, bytes) in entries {
        let target = stage.join(relative);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&target, bytes).unwrap();
        files.push(relative.to_string());
    }
    let out = folder.join(name);
    let mut check = || Ok(());
    deadlock_vpk::write_vpk(&stage, &files, &out, None, 2, &mut check).unwrap();
    std::fs::remove_dir_all(&stage).unwrap();
    out
}

#[test]
fn installed_mods_are_scanned_and_repaired_in_place_with_a_backup() {
    let temp = tempfile::tempdir().unwrap();
    let game_root = temp.path().join("Deadlock");
    let addons = game_root.join("game/citadel/addons");
    std::fs::create_dir_all(&addons).unwrap();
    let game_pak = vpk(temp.path(), "original_dir.vpk", &[(MODEL, &current()), ("materials/dev/x.vmat_c", b"material")]);
    let installed = vpk(&addons, "pak05_dir.vpk", &[(MODEL, &previous()), ("materials/dev/x.vmat_c", b"material"), ("panorama/images/heroes/haze_card_psd.vtex_c", b"icon")]);
    let skin_only = vpk(&addons, "pak06_dir.vpk", &[("materials/dev/y.vmat_c", b"material")]);
    let settings = Settings { deadlock: game_root.display().to_string(), pak: game_pak.display().to_string(), ..Settings::default() };
    let cancel = CancellationToken::new();

    let scan = mod_update::scan(&settings, &cancel).unwrap();
    let names: Vec<(&str, Status)> = scan.iter().map(|m| (m.file.as_str(), m.status)).collect();
    assert_eq!(names, [("pak05_dir.vpk", Status::NeedsUpdate), ("pak06_dir.vpk", Status::NoHeroModel)]);
    assert_eq!(scan[0].models[0].convention, Convention::Previous);
    assert_eq!(scan[0].models[0].hero.as_deref(), Some("Haze"));

    let backups = temp.path().join("backups");
    let before = std::fs::read(&installed).unwrap();
    let logged = std::cell::RefCell::new(Vec::<String>::new());
    let report = mod_update::update(&settings, &installed, &backups, 2, &cancel, &|m| logged.borrow_mut().push(m.to_string()));
    let report = report.unwrap();
    assert_eq!(report.updated, [MODEL]);
    assert_eq!(report.converted, [format!("{MODEL}: my_camera")]);
    assert!(Path::new(&report.backup).is_file() && std::fs::read(&report.backup).unwrap() == before, "backup keeps the previous file");
    assert!(logged.borrow().iter().any(|m| m.contains("3 attachments taken from the game's model, 1 custom converted")), "{:?}", logged.borrow());

    // The rewritten VPK keeps every other entry and now matches the game.
    let after = Vpk::open(&installed).unwrap();
    assert_eq!(after.entries.len(), 3);
    assert_eq!(after.read("materials/dev/x.vmat_c").unwrap(), b"material");
    assert_eq!(after.read("panorama/images/heroes/haze_card_psd.vtex_c").unwrap(), b"icon");
    let repaired = after.read(MODEL).unwrap();
    assert_eq!(attachments::convention(&repaired, &current()).unwrap(), Convention::Current);
    let camera = attachments::list(&repaired).unwrap().into_iter().find(|a| a.name == "my_camera").unwrap();
    assert!((camera.offset[2] - 120.).abs() < 1e-3 && camera.offset[0].abs() - 5. < 1e-3, "{:?}", camera.offset);
    assert_eq!(mod_update::scan(&settings, &cancel).unwrap()[0].status, Status::UpToDate);
    // Nothing is left behind in the addons folder, and an up-to-date VPK is refused.
    let leftovers: Vec<String> = std::fs::read_dir(&addons).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().to_string()).collect();
    assert_eq!(leftovers, ["pak05_dir.vpk", "pak06_dir.vpk"]);
    assert!(mod_update::update(&settings, &installed, &backups, 2, &cancel, &|_| {}).unwrap_err().to_string().contains("needs no update"));
    assert!(mod_update::update(&settings, &skin_only, &backups, 2, &cancel, &|_| {}).is_err());
    // The game's own archive and files outside the addons folder are never touched.
    assert!(mod_update::update(&settings, &game_pak, &backups, 2, &cancel, &|_| {}).is_err());
}

#[test]
fn the_scan_names_every_vpk_by_its_contents_and_never_drops_one() {
    use deadlock_components::vpk_contents::Kind;
    let temp = tempfile::tempdir().unwrap();
    let game_root = temp.path().join("Deadlock");
    let addons = game_root.join("game/citadel/addons");
    std::fs::create_dir_all(&addons).unwrap();
    let game_pak = vpk(temp.path(), "original_dir.vpk", &[(MODEL, &current())]);
    let model = current();
    // A character model with its materials; a sounds-and-icons file for two heroes; item
    // pictures; menu files only; a copy of the first under a name a loader gave it; a broken file.
    vpk(&addons, "pak02_dir.vpk", &[(MODEL, &model), ("materials/models/heroes/haze/body.vmat_c", b"m1"), ("materials/models/heroes/haze/body_color_png.vtex_c", b"t1")]);
    vpk(&addons, "pak03_dir.vpk", &[
        ("sounds/abilities/haze/haze_dagger_cast.vsnd_c", b"s1"),
        ("sounds/vo/ghost/ghost_kill_01.vsnd_c", b"s2"),
        ("panorama/images/heroes/haze_card_psd.vtex_c", b"i1"),
        ("materials/models/heroes/haze/body.vmat_c", b"m2"),
        ("materials/default/default_ao_tga_0123abcd.vtex_c", b"d2"),
    ]);
    vpk(&addons, "pak04_dir.vpk", &[("panorama/images/items/weapon/basic_magazine_psd.vtex_c", b"x"), ("panorama/images/items/spirit/mystic_burst_psd.vtex_c", b"y"), ("readme.txt", b"one"), ("materials/default/default_ao_tga_0123abcd.vtex_c", b"d1")]);
    vpk(&addons, "pak05_dir.vpk", &[("panorama/layout/main_menu.vxml_c", b"l"), ("panorama/styles/main_menu.vcss_c", b"c"), ("readme.txt", b"two")]);
    let renamed = vpk(&addons, "renamed_by_loader.vpk", &[(MODEL, &model), ("materials/models/heroes/haze/body.vmat_c", b"m1"), ("materials/models/heroes/haze/body_color_png.vtex_c", b"t1")]);
    std::fs::write(addons.join("pak06_dir.vpk"), b"not a vpk at all").unwrap();
    // A chunk beside its directory file is read through it; a "_001" name alone is a VPK of its own.
    vpk(&addons, "pak08_dir.vpk", &[("sounds/abilities/haze/haze_other.vsnd_c", b"z")]);
    std::fs::write(addons.join("pak08_000.vpk"), b"a chunk of a split vpk").unwrap();
    std::fs::write(addons.join("standalone_001.vpk"), b"not a chunk").unwrap();
    std::fs::create_dir_all(addons.join("folder.vpk")).unwrap();
    let settings = Settings { deadlock: game_root.display().to_string(), pak: game_pak.display().to_string(), ..Settings::default() };
    let cancel = CancellationToken::new();
    let before: Vec<_> = std::fs::read_dir(&addons).unwrap().map(|e| e.unwrap().file_name()).collect();

    let scan = mod_update::scan(&settings, &cancel).unwrap();
    let files: Vec<&str> = scan.iter().map(|m| m.file.as_str()).collect();
    assert_eq!(files, ["pak02_dir.vpk", "pak03_dir.vpk", "pak04_dir.vpk", "pak05_dir.vpk", "pak06_dir.vpk", "pak08_dir.vpk", "renamed_by_loader.vpk", "standalone_001.vpk"], "chunks and folders are not files of their own");
    let by = |name: &str| scan.iter().find(|m| m.file == name).unwrap();
    let heroes = |name: &str| by(name).contents.heroes.iter().map(|h| h.name.clone()).collect::<Vec<_>>();

    let skin = by("pak02_dir.vpk");
    assert_eq!(heroes("pak02_dir.vpk"), ["Haze"]);
    assert_eq!(skin.status, Status::UpToDate);
    assert_eq!((skin.contents.kinds[&Kind::CharacterModel], skin.contents.kinds[&Kind::Material], skin.contents.kinds[&Kind::Texture]), (1, 1, 1));
    // Extras for two heroes, without a character model: not checked.
    let extras = by("pak03_dir.vpk");
    assert_eq!(heroes("pak03_dir.vpk"), ["Haze", "Lady Geist"]);
    assert_eq!(extras.status, Status::NoHeroModel);
    assert_eq!((extras.contents.kinds[&Kind::Sound], extras.contents.kinds[&Kind::Icon]), (2, 1));
    // Item pictures belong to every hero; menu files to none.
    assert_eq!((by("pak04_dir.vpk").contents.every_hero, by("pak04_dir.vpk").contents.heroes.len()), (2, 0));
    assert_eq!((by("pak05_dir.vpk").contents.unattributed, by("pak05_dir.vpk").contents.kinds[&Kind::Menu]), (3, 2));
    // The broken file is listed with its reason.
    let broken = by("pak06_dir.vpk");
    assert!(broken.unreadable.is_some() && broken.identity.is_empty() && broken.note.starts_with("Not readable as a VPK"), "{broken:?}");
    // The renamed copy keeps the identity; the other files differ.
    assert_eq!(by("renamed_by_loader.vpk").identity, skin.identity);
    assert!(skin.identity.starts_with("vpk-tree-v1:"));
    assert_ne!(extras.identity, skin.identity);
    assert_eq!(mod_update::identity_of(&renamed).unwrap(), skin.identity);
    // Game files two VPKs both hold with other contents are listed on both sides, without
    // saying which one the game uses. The same bytes in both (the renamed copy) and files at
    // the archive's root (a readme) and the compiler's default textures do not count.
    let shared: Vec<(&str, usize)> = skin.shared.iter().map(|s| (s.file.as_str(), s.count)).collect();
    assert_eq!(shared, [("pak03_dir.vpk", 1)]);
    let shared: Vec<(&str, usize)> = extras.shared.iter().map(|s| (s.file.as_str(), s.count)).collect();
    assert_eq!(shared, [("pak02_dir.vpk", 1), ("renamed_by_loader.vpk", 1)]);
    assert_eq!(extras.shared[0].paths, ["materials/models/heroes/haze/body.vmat_c"]);
    assert!(by("pak04_dir.vpk").shared.is_empty() && by("pak05_dir.vpk").shared.is_empty());
    // Nothing in the addons folder changed.
    let after: Vec<_> = std::fs::read_dir(&addons).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(before, after);
}

#[test]
fn own_outputs_match_by_identity_and_name_their_project() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().join("workspaces");
    let id = "0123456789abcdef0123456789abcdef";
    let output = workspace.join(id).join("output");
    std::fs::create_dir_all(&output).unwrap();
    let built = vpk(&output, "pak09_dir.vpk", &[("sounds/abilities/haze/a.vsnd_c", b"a")]);
    // Not a project folder: ignored.
    std::fs::create_dir_all(workspace.join("scratch/output")).unwrap();
    vpk(&workspace.join("scratch/output"), "pak10_dir.vpk", &[("sounds/abilities/haze/a.vsnd_c", b"a")]);
    std::fs::write(workspace.join(id).join("build-manifest.json"), format!(r#"{{"schema":1,"project":{{"id":"{id}","name":"Sample","hero_name":"Haze"}}}}"#)).unwrap();
    let outputs = mod_update::own_outputs(&workspace, &CancellationToken::new()).unwrap();
    assert_eq!(outputs.len(), 1);
    assert_eq!(outputs[0].project_id, id);
    assert_eq!(outputs[0].identity, mod_update::identity_of(&built).unwrap());
    assert_eq!(mod_update::project_summary(&workspace, id), Some(("Sample".to_owned(), "Haze".to_owned())));
    assert_eq!(mod_update::project_summary(&workspace, "fedcba9876543210fedcba9876543210"), None);
    assert_eq!(mod_update::project_summary(&workspace, "../outside"), None);
}

/// The compiled model of a build with an extra bone (as generated movement bones are).
fn compiled_with_extra_bone(attachments_list: Vec<Value>) -> Vec<u8> {
    let base = model(attachments_list);
    let layout = container::layout(&base).unwrap();
    let mut replacements = BTreeMap::new();
    for (ordinal, block) in layout.blocks.iter().enumerate() {
        if block.name != "DATA" {
            continue;
        }
        let mut doc = binary::decode(container::payload(&base, block).unwrap()).unwrap();
        let skeleton = doc.root.get_mut("m_modelSkeleton").unwrap();
        let push = |value: &mut Value, item: Value| {
            let Kind::Array(items) = &mut value.kind else { panic!() };
            items.push(item);
        };
        push(skeleton.get_mut("m_boneName").unwrap(), Value::new(Kind::String("dmb_secondary_L_bowB_01_jnt".into())));
        push(skeleton.get_mut("m_nParent").unwrap(), Value::new(Kind::I64(1)));
        push(skeleton.get_mut("m_bonePosParent").unwrap(), f32s(&[0., 5., 0.]));
        push(skeleton.get_mut("m_boneRotParent").unwrap(), f32s(&[0., 0., 0., 1.]));
        replacements.insert(ordinal, binary::encode(&doc.root, doc.format_guid).unwrap());
    }
    container::rebuild_ordinals(&base, &replacements).unwrap()
}

#[test]
fn a_build_keeps_its_compiled_model_and_takes_only_the_game_attachments() {
    // An earlier build passed the two models the wrong way round and
    // wrote the game's original model over the compiled one ("Missing or
    // ambiguous generated bone: dmb_secondary_L_bowB_01_jnt").
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("artist.vmdl_c");
    std::fs::write(&output, compiled_with_extra_bone(vec![
        attachment("standing_pivot", "root_motion", [0., 86.25, 0.], OLD_ROT),
        attachment("crouching_pivot", "root_motion", [0., 57.5, 0.], OLD_ROT),
        attachment("mask", "spine_0", [3.5, 8., 0.], [0., 0., 0., 1.]),
    ])).unwrap();
    let game = current();
    assert_eq!(mod_update::take_game_attachments(&output, &game).unwrap(), 3);
    let after = std::fs::read(&output).unwrap();
    let skeleton = container::read_blocks(&after, "DATA").unwrap()[0].root.get("m_modelSkeleton").unwrap().clone();
    let names: Vec<String> = skeleton.get("m_boneName").unwrap().array().unwrap().iter().map(|n| n.string().unwrap().to_string()).collect();
    assert_eq!(names, ["root_motion", "spine_0", "dmb_secondary_L_bowB_01_jnt"], "the compiled skeleton stays");
    assert_eq!(attachments::convention(&after, &game).unwrap(), Convention::Current);
    // Already current: nothing to write.
    assert_eq!(mod_update::take_game_attachments(&output, &game).unwrap(), 0);
}

/// A made-up names file in the game's format with these item tokens.
fn names_file(items: &[(&str, &str)]) -> Vec<u8> {
    let mut text = String::from("\u{feff}\"lang\"\r\n{\r\n\t\"Tokens\"\r\n\t{\r\n");
    for (key, value) in items {
        text.push_str(&format!("\t\t\"{key}\"\t\"{value}\"\r\n"));
    }
    text.push_str("\t}\r\n}\r\n");
    text.into_bytes()
}

#[test]
fn item_names_files_are_their_own_kind_and_say_when_the_game_has_more_names() {
    use deadlock_components::vpk_contents::Kind;
    let temp = tempfile::tempdir().unwrap();
    let game_root = temp.path().join("Deadlock");
    let addons = game_root.join("game/citadel/addons");
    std::fs::create_dir_all(&addons).unwrap();
    let game_pak = vpk(temp.path(), "original_dir.vpk", &[(MODEL, &current())]);
    let folder = deadlock_components::item_names::folder(&game_pak);
    std::fs::create_dir_all(&folder).unwrap();
    let game_names = [("upgrade_one", "One"), ("upgrade_two", "Two"), ("upgrade_three", "Three")];
    std::fs::write(folder.join("citadel_gc_mod_names_english.txt"), names_file(&game_names)).unwrap();
    std::fs::write(folder.join("citadel_gc_mod_names_german.txt"), names_file(&game_names)).unwrap();
    let path = |language: &str| format!("resource/localization/citadel_gc_mod_names/citadel_gc_mod_names_{language}.txt");
    // Made before the game added an item: one name missing in English.
    vpk(&addons, "pak10_dir.vpk", &[(&path("english"), &names_file(&[("upgrade_one", "New One"), ("upgrade_two", "Two")]))]);
    // Current in English, two missing in German.
    vpk(&addons, "pak11_dir.vpk", &[(&path("english"), &names_file(&game_names)), (&path("german"), &names_file(&[("upgrade_one", "Eins")]))]);
    // Current everywhere.
    vpk(&addons, "pak12_dir.vpk", &[(&path("english"), &names_file(&[("upgrade_one", "New One"), ("upgrade_two", "Two"), ("upgrade_three", "Three")]))]);
    let settings = Settings { deadlock: game_root.display().to_string(), pak: game_pak.display().to_string(), ..Settings::default() };
    let cancel = CancellationToken::new();
    let scan = mod_update::scan(&settings, &cancel).unwrap();
    let by = |name: &str| scan.iter().find(|m| m.file == name).unwrap();
    for file in ["pak10_dir.vpk", "pak11_dir.vpk", "pak12_dir.vpk"] {
        let row = by(file);
        assert_eq!(row.status, Status::NoHeroModel);
        assert!(row.contents.kinds.contains_key(&Kind::ItemNames) && !row.contents.kinds.contains_key(&Kind::Text));
        assert_eq!((row.contents.every_hero, row.contents.unattributed), (row.contents.files, 0), "every hero's, like item pictures");
    }
    assert_eq!(by("pak10_dir.vpk").note, "1 of the game's item names is missing from this file (english): those items show without a name until the mod is built again.");
    assert_eq!(by("pak11_dir.vpk").note, "2 of the game's item names are missing from this file (german): those items show without a name until the mod is built again.");
    assert_eq!(by("pak12_dir.vpk").note, "");
}
