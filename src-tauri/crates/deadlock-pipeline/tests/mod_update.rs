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
