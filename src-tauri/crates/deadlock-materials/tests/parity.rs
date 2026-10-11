use deadlock_core::CancellationToken;
use deadlock_materials::*;
use deadlock_project::customization::{AppearanceMaterial, Customization};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}
fn expected() -> Value {
    serde_json::from_slice(&fs::read(fixtures().join("expected.json")).unwrap()).unwrap()
}
#[test]
fn generator_all_presets_and_texture_channels_matches_python_exactly() {
    for case in expected()["material_text"].as_array().unwrap() {
        let config: MaterialChoices = serde_json::from_value(case["config"].clone()).unwrap();
        let paths = serde_json::from_value(case["paths"].clone()).unwrap();
        assert_eq!(
            material_text(&config, &paths).unwrap(),
            case["expected"].as_str().unwrap()
        );
    }
}
#[test]
fn lossless_appearance_edits_match_python_exactly() {
    for case in expected()["appearance"].as_array().unwrap() {
        let row: AppearanceMaterial = serde_json::from_value(case["row"].clone()).unwrap();
        assert_eq!(
            patch_text(case["text"].as_str().unwrap(), &row).unwrap(),
            case["expected"].as_str().unwrap()
        );
    }
}
fn normalize(value: Value, root: &str) -> Value {
    match value {
        // Planned paths are native; the Python reference uses `/` after the root.
        Value::String(s) if s.contains(root) => {
            Value::String(s.replace(root, "$TEXTURES").replace('\\', "/"))
        }
        Value::Array(v) => Value::Array(v.into_iter().map(|v| normalize(v, root)).collect()),
        Value::Object(v) => Value::Object(
            v.into_iter()
                .map(|(k, v)| (k, normalize(v, root)))
                .collect(),
        ),
        other => other,
    }
}
#[test]
fn planner_existing_ambiguity_and_explicit_no_texture_match_python() {
    let fixtures = fixtures();
    let configured: Plans = serde_json::from_value(
        serde_json::json!({"models/test/body.vmat":{"textures":{"color":""},"confirmed":true}}),
    )
    .unwrap();
    let plan = plan(
        &[
            "models/test/body.vmat".into(),
            "models/test/existing.vmat".into(),
        ],
        &fixtures.join("source"),
        &fixtures.join("textures"),
        &configured,
        &CancellationToken::new(),
    )
    .unwrap();
    let value = normalize(
        serde_json::to_value(plan).unwrap(),
        deadlock_project::asset_paths::resolve(&fixtures.join("textures"))
            .unwrap()
            .to_str()
            .unwrap(),
    );
    assert_eq!(value, expected()["plans"]);
}
#[test]
fn texture_references_and_cache_key_match_python() {
    let all = expected();
    assert_eq!(
        texture_references(all["references"]["text"].as_str().unwrap()),
        serde_json::from_value::<Vec<String>>(all["references"]["expected"].clone()).unwrap()
    );
    let source = serde_json::from_value(all["cache_key"]["source"].clone()).unwrap();
    let engine = serde_json::from_value(all["cache_key"]["engine"].clone()).unwrap();
    assert_eq!(
        cache_key_from_inputs(&source, &engine).unwrap(),
        all["cache_key"]["expected"].as_str().unwrap()
    );
}
#[test]
fn all_texture_channel_aliases_have_boundary_matching() {
    let temp = tempfile::tempdir().unwrap();
    for (channel, aliases) in CHANNELS {
        for alias in *aliases {
            let p = temp.path().join(format!("body_{alias}.png"));
            fs::write(&p, "x").unwrap();
            let values = texture_candidates(
                "models/test/body.vmat",
                temp.path(),
                &CancellationToken::new(),
            )
            .unwrap();
            assert_eq!(values[*channel].len(), 1, "{alias}");
            fs::remove_file(p).unwrap();
        }
    }
    for name in [
        "nobody_color.png",
        "body_coloring.png",
        "body_normal_roughness.png",
    ] {
        fs::write(temp.path().join(name), "x").unwrap();
    }
    let candidates = texture_candidates(
        "models/test/body.vmat",
        temp.path(),
        &CancellationToken::new(),
    )
    .unwrap();
    assert_eq!(candidates["color"].len(), 1);
    assert!(candidates["normal"].is_empty());
    assert!(candidates["roughness"].is_empty());
}
#[test]
fn invalid_material_choices_and_unreviewed_plans_fail() {
    for config in [
        serde_json::json!({"preset":"invented"}),
        serde_json::json!({"color":[2,0,0]}),
        serde_json::json!({"roughness":-1}),
        serde_json::json!({"metalness":2}),
    ] {
        let config = serde_json::from_value(config).unwrap();
        assert!(material_text(&config, &BTreeMap::new()).is_err());
    }
    let config = MaterialChoices::default();
    assert!(material_text(
        &config,
        &BTreeMap::from([("wrong".into(), "tex.png".into())])
    )
    .is_err());
    assert!(material_text(
        &config,
        &BTreeMap::from([("color".into(), "../escape.png".into())])
    )
    .is_err());
    let temp = tempfile::tempdir().unwrap();
    for config in [
        serde_json::json!({"confirmed":false}),
        serde_json::json!({"confirmed":true,"ambiguous":["color"]}),
        serde_json::json!({"mode":"keep"}),
    ] {
        let plans = Plans::from([(
            "models/test/body.vmat".into(),
            serde_json::from_value(config).unwrap(),
        )]);
        assert!(generate(
            &plans,
            temp.path(),
            "models/test",
            &CancellationToken::new()
        )
        .is_err());
        assert!(!temp.path().join("models").exists());
    }
}
#[test]
fn retained_shader_unknown_parameters_disabled_state_and_plan_choices_preserved() {
    let text = "// retain\n\"Layer0\" { \"shader\" \"pbr.vfx\" \"future\" \"unchanged\" }\n";
    let row: AppearanceMaterial =
        serde_json::from_value(serde_json::json!({"color":[0.2,0.3,0.4]})).unwrap();
    let changed = patch_text(text, &row).unwrap();
    assert!(changed.contains("\"future\" \"unchanged\""));
    assert!(changed.contains("// retain"));
    let plans: Plans = serde_json::from_value(
        serde_json::json!({"models/test/body.vmat":{"color":[0.1,0.2,0.3],"confirmed":false}}),
    )
    .unwrap();
    let state:Customization=serde_json::from_value(serde_json::json!({"enabled":true,"appearance":{"enabled":true,"materials":[{"target":"models/test/body.vmat","color":[1,0,0]}]}})).unwrap();
    assert_eq!(apply_materials(&plans, &state).unwrap(), plans);
    let disabled: Customization =
        serde_json::from_value(serde_json::json!({"enabled":false})).unwrap();
    assert_eq!(apply_materials(&plans, &disabled).unwrap(), plans);
    assert!(apply_materials(&Plans::new(), &state).is_err());
}
#[test]
fn malformed_layer_unknown_shader_duplicates_and_invalid_overrides_fail() {
    let opacity: AppearanceMaterial =
        serde_json::from_value(serde_json::json!({"opacity":0.5})).unwrap();
    for text in [
        "\"Layer0\" { \"shader\" \"unknown.vfx\" }",
        "Layer0 { shader pbr.vfx Layer1 { x 1 } }",
        "Layer0 { shader pbr.vfx shader pbr.vfx }",
        "Layer0 { shader }",
        "Layer0 { shader \"unterminated }",
        "Layer0 { shader pbr.vfx /* incomplete",
    ] {
        assert!(patch_text(text, &opacity).is_err(), "{text}");
        assert_eq!(
            patch_text(text, &AppearanceMaterial::default()).unwrap(),
            text
        );
    }
    for row in [
        serde_json::json!({"opacity":-0.1}),
        serde_json::json!({"emission":65}),
        serde_json::json!({"color":[1.1,0,0]}),
    ] {
        assert!(patch_text(
            "Layer0 { shader pbr.vfx }",
            &serde_json::from_value(row).unwrap()
        )
        .is_err());
    }
    let duplicate:Customization=serde_json::from_value(serde_json::json!({"enabled":true,"appearance":{"enabled":true,"materials":[{"target":"a.vmat"},{"target":"A.vmat"}]}})).unwrap();
    assert!(validate_appearance(&duplicate).is_err());
}
#[test]
fn generation_copies_reviewed_texture_and_keeps_original_unchanged() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("téxture.png");
    fs::write(&source, b"opaque texture bytes").unwrap();
    let content = temp.path().join("content");
    let plans:Plans=serde_json::from_value(serde_json::json!({"models/test/body.vmat":{"confirmed":true,"preset":"cloth","textures":{"color":source}}})).unwrap();
    assert_eq!(
        generate(&plans, &content, "models/test", &CancellationToken::new()).unwrap(),
        ["models/test/body.vmat"]
    );
    let text = fs::read_to_string(content.join("models/test/body.vmat")).unwrap();
    let references = texture_references(&text);
    assert_eq!(references.len(), 1);
    assert!(references[0].ends_with("_t_xture.png"));
    assert_eq!(
        fs::read(content.join(&references[0])).unwrap(),
        b"opaque texture bytes"
    );
    assert_eq!(fs::read(source).unwrap(), b"opaque texture bytes");
}
#[test]
fn appearance_validates_every_shader_before_writing_any() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("a.vmat"), "Layer0 { shader pbr.vfx }").unwrap();
    fs::write(temp.path().join("b.vmat"), "Layer0 { shader unknown.vfx }").unwrap();
    let plans = Plans::from([
        ("a.vmat".into(), MaterialChoices::default()),
        ("b.vmat".into(), MaterialChoices::default()),
    ]);
    let state:Customization=serde_json::from_value(serde_json::json!({"enabled":true,"appearance":{"enabled":true,"materials":[{"target":"a.vmat","color":[1,0,0]},{"target":"b.vmat","color":[1,0,0]}]}})).unwrap();
    assert!(patch_materials(temp.path(), &plans, &state, &CancellationToken::new()).is_err());
    assert_eq!(
        fs::read_to_string(temp.path().join("a.vmat")).unwrap(),
        "Layer0 { shader pbr.vfx }"
    );
}

#[test]
fn quoted_braces_are_values_not_layer_structure() {
    let row: AppearanceMaterial =
        serde_json::from_value(serde_json::json!({"opacity":0.5})).unwrap();
    let text = "Layer0 { shader pbr.vfx future \"{\" }";
    assert!(patch_text(text, &row).unwrap().contains("future \"{\""));
    assert!(patch_text("Layer0 \"{\" shader pbr.vfx }", &row).is_err());
}
