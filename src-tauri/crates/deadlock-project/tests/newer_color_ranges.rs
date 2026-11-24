//! A project whose color ranges use the fields older apps would ignore
//! (`set_hue`, `set_saturation`, `lift`, `part`) is saved with customization
//! schema 2, which 0.13.0 refuses to open; every other project is saved exactly
//! as before.
use deadlock_project::{
    customization::{HeroColorRange, HeroMaterial, HeroMaterials, NEWER_RANGES_SCHEMA},
    Project,
};

fn project(range: HeroColorRange) -> Project {
    let mut project = Project::default();
    project.name = "Sample".into();
    project.original_model = "models/heroes_staging/haze/haze.vmdl".into();
    project.customization.hero_materials = Some(HeroMaterials {
        enabled: Some(true),
        materials: Some(vec![HeroMaterial {
            target: Some("models/heroes_staging/haze/materials/hazev2.vmat_c".into()),
            ranges: Some(vec![range]),
            ..Default::default()
        }]),
        ..Default::default()
    });
    project
}

#[test]
fn new_ranges_are_saved_for_older_apps_to_refuse_and_others_keep_their_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("sample.dmb.json");
    // An ordinary range: no schema key appears.
    let mut plain = project(HeroColorRange { hue_shift: Some(30.), ..Default::default() });
    plain.save(&path).unwrap();
    let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert!(saved["customization"].get("schema").is_none(), "{}", saved["customization"]);
    // A grey given a color: schema 2, which this app opens and keeps.
    let mut given = project(HeroColorRange { set_hue: Some(270.), set_saturation: Some(0.6), part: Some(deadlock_project::color_ranges::ColorPart { u: 0.25, v: 0.5 }), ..Default::default() });
    given.save(&path).unwrap();
    // DMB_SAVED_SAMPLE: keep a copy of this file to open with an older app's code.
    if let Ok(copy) = std::env::var("DMB_SAVED_SAMPLE") {
        std::fs::copy(&path, copy).unwrap();
    }
    let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(saved["customization"]["schema"], NEWER_RANGES_SCHEMA);
    let range = &saved["customization"]["hero_materials"]["materials"][0]["ranges"][0];
    assert_eq!(range["set_hue"], 270.0);
    assert_eq!(range["part"]["v"], 0.5);
    let loaded = Project::load(&path).unwrap();
    assert_eq!(loaded.customization.schema, Some(NEWER_RANGES_SCHEMA));
    // 0.13.0's own check on open (customization.rs at v0.13.0): any schema but 1 is refused with
    // "Unsupported character extras version." before anything is built.
    assert!(loaded.customization.schema.is_some_and(|v| v != 1));
    // Without the new fields the next save goes back to schema 1.
    let mut back = loaded;
    back.customization.hero_materials.as_mut().unwrap().materials.as_mut().unwrap()[0].ranges = Some(vec![HeroColorRange { hue_shift: Some(30.), ..Default::default() }]);
    back.save(&path).unwrap();
    assert_eq!(Project::load(&path).unwrap().customization.schema, Some(1));
}
