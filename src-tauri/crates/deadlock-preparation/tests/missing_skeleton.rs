//! The inspection's optional missing-skeleton record (worker.py `missing_skeleton`).
use deadlock_preparation::service::{Inspection, MISSING_SKELETON_POLICY, MissingSkeleton};
use serde_json::{Value, json};

fn inspection(extra: Value) -> Value {
    let mut value = json!({
        "armatures": 0, "bones": [], "vertices": 3, "bounds": [[0.0, 0.0, 0.0], [1.0, 1.0, 2.0]],
        "meshes": [{"name": "Body", "vertices": 3, "polygons": 1, "materials": ["Skin"], "disconnected_components": 1}],
        "morphs": [], "weighted_bones": [], "suggested_mode": "unrigged_humanoid",
        "humanoid": {"status": "Unsupported", "errors": ["Unsupported: two separated legs were not identified."]},
        "source_helpers": [], "warnings": [], "status": "ok", "weapons": [],
    });
    value.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
    value
}

#[test]
fn an_inspection_without_the_record_serialises_as_before() {
    let input = inspection(json!({}));
    let parsed: Inspection = serde_json::from_value(input.clone()).unwrap();
    assert_eq!(parsed.missing_skeleton, None);
    let written = serde_json::to_value(&parsed).unwrap();
    assert!(written.get("missing_skeleton").is_none(), "{written}");
    assert_eq!(written, input);
}

#[test]
fn the_missing_skeleton_record_round_trips() {
    let record = json!({"policy": MISSING_SKELETON_POLICY, "groups": 42, "meshes": ["Body", "Hair"], "modifier": true});
    let input = inspection(json!({"missing_skeleton": record}));
    let parsed: Inspection = serde_json::from_value(input.clone()).unwrap();
    assert_eq!(
        parsed.missing_skeleton,
        Some(MissingSkeleton { policy: MISSING_SKELETON_POLICY.into(), groups: 42, meshes: vec!["Body".into(), "Hair".into()], modifier: true })
    );
    assert_eq!(serde_json::to_value(&parsed).unwrap(), input);
}

#[test]
fn the_policy_name_is_the_one_blender_writes() {
    let worker = include_str!("../resources/blender/worker.py");
    assert!(worker.contains(&format!("MISSING_SKELETON_POLICY='{MISSING_SKELETON_POLICY}'")));
}
