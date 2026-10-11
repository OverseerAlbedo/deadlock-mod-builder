use deadlock_project::{presets::OutfitPreset, Project};
use serde_json::{json, Value};
#[test]
fn python_outfit_authority_and_choice_parity() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/presets.json")).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let project: Project = serde_json::from_value(case["project"].clone()).unwrap();
        let preset: OutfitPreset = serde_json::from_value(case["preset"].clone()).unwrap();
        let result = preset.apply(&project).unwrap();
        let expected: Project = serde_json::from_value(case["expected"].clone()).unwrap();
        assert_eq!(result, expected, "{}", case["name"]);
    }
    for case in fixture["invalid"].as_array().unwrap() {
        let project: Project = serde_json::from_value(case["project"].clone()).unwrap();
        let result = serde_json::from_value::<OutfitPreset>(case["preset"].clone())
            .map_err(|e| e.to_string())
            .and_then(|p| p.apply(&project).map_err(|e| e.to_string()));
        assert!(result.is_err(), "{}", case["name"]);
    }
}
#[test]
fn portable_preset_preserves_targets_and_protects_sources() {
    let root = tempfile::tempdir().unwrap();
    let original = root.path().join("original");
    let moved = root.path().join("moved");
    std::fs::create_dir_all(&original).unwrap();
    let source = original.join("model.dmx");
    std::fs::write(&source, b"source").unwrap();
    let project:Project=serde_json::from_value(json!({"original_model":"models/hero.vmdl_c","dmx":source.to_str().unwrap(),"customization":{"icons":[{"target":"icons/hero.vtex_c","source":original.join("icon.png").to_str().unwrap()}]}})).unwrap();
    assert!(OutfitPreset::save(&project, &source, "outfit").is_err());
    assert_eq!(std::fs::read(&source).unwrap(), b"source");
    OutfitPreset::save(&project, &original.join("outfit.json"), "outfit").unwrap();
    std::fs::rename(&original, &moved).unwrap();
    let preset = OutfitPreset::load(&moved.join("outfit.json")).unwrap();
    assert_eq!(
        preset.choices.dmx,
        moved.join("model.dmx").to_str().unwrap()
    );
    assert_eq!(
        preset.choices.customization.icons.as_ref().unwrap()[0]
            .target
            .as_deref(),
        Some("icons/hero.vtex_c")
    );
}
