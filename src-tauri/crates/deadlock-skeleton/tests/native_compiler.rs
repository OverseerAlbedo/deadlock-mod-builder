use deadlock_resource::{Kind, Value};
use deadlock_skeleton::{bindings, Skeleton};
use std::collections::BTreeSet;
mod common;
fn typed(v: &serde_json::Value) -> Value {
    Value::new(match v {
        serde_json::Value::Null => Kind::Null,
        serde_json::Value::Bool(v) => Kind::Bool(*v),
        serde_json::Value::String(v) => Kind::String(v.clone()),
        serde_json::Value::Number(v) if v.is_i64() => Kind::I64(v.as_i64().unwrap()),
        serde_json::Value::Number(v) => Kind::F64(v.as_f64().unwrap()),
        serde_json::Value::Array(v) => Kind::Array(v.iter().map(typed).collect()),
        serde_json::Value::Object(v) => {
            Kind::Object(v.iter().map(|(k, v)| (k.clone(), typed(v))).collect())
        }
    })
}
#[test]
fn actual_tangtang_native_drift_is_bounded_and_original_budget_is_unchanged() {
    // Captured from the TangTang conversion: private assets, skipped with a note when absent.
    let Some(text) = common::private_text("fixtures/tangtang-native-binds.json") else { return };
    let fixture: serde_json::Value = serde_json::from_str(&text).unwrap();
    let skeleton: Skeleton = serde_json::from_value(fixture["skeleton"].clone()).unwrap();
    let meshes: Vec<_> = fixture["mdats"]
        .as_array()
        .unwrap()
        .iter()
        .map(typed)
        .collect();
    let old = bindings::inverse_binds(&meshes, &skeleton)
        .unwrap_err()
        .to_string();
    assert!(old.contains("dmb_secondary_HB_6_0"));
    // Numeric regression only. Separate source-compiler tests prove which
    // names may enter this set; the prefix is not production authority.
    let custom: BTreeSet<String> = skeleton
        .names
        .iter()
        .filter(|n| n.starts_with("dmb_secondary_"))
        .cloned()
        .collect();
    assert_eq!(custom.len(), 262);
    let audit = bindings::inverse_binds_with_custom_bones(&meshes, &skeleton, &custom).unwrap();
    assert_eq!(audit.meshes.len(), 12);
    assert_eq!(audit.linear_component_limit, 1e-4);
    assert_eq!(audit.translation_component_limit_inches, 0.001);
    assert!((audit.max_translation_residual_inches - 0.00291850832298).abs() < 1e-9);
    assert!((audit.max_linear_residual - 0.000127082529).abs() < 1e-10);
    assert_eq!(
        audit.source_qualified_custom_bones.unwrap().names.len(),
        262
    );
}
