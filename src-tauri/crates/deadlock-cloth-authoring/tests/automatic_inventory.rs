use deadlock_cloth_authoring::{automatic, inventory, physics};
use deadlock_core::CancellationToken;
use deadlock_dmx::Dmx;
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
};
fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}
fn reference() -> Value {
    serde_json::from_str(include_str!("fixtures/automatic-reference.json")).unwrap()
}
fn same_numeric(a: &Value, b: &Value, path: &str) {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => {
            let (a, b) = (a.as_f64().unwrap(), b.as_f64().unwrap());
            assert!(
                (a - b).abs() <= 1e-12f64.max(b.abs() * 2e-15),
                "{path}: {a} != {b}"
            );
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                same_numeric(a, b, &format!("{path}[{i}]"));
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(
                a.keys().collect::<std::collections::BTreeSet<_>>(),
                b.keys().collect(),
                "{path}"
            );
            for (k, a) in a {
                same_numeric(a, &b[k], &format!("{path}.{k}"));
            }
        }
        _ => assert_eq!(a, b, "{path}"),
    }
}
#[test]
fn python_surface_selection_and_rejections() {
    for case in reference()["surfaces"].as_array().unwrap() {
        let raw = fs::read(fixtures().join(case["file"].as_str().unwrap())).unwrap();
        let dmx = Dmx::parse(raw.clone()).unwrap();
        let options = serde_json::from_value(case["options"].clone()).unwrap();
        let result = automatic::surface_selection(&dmx, &options, &CancellationToken::new());
        if case["expected"].get("error").is_some() {
            assert!(result.is_err(), "{}", case["file"]);
        } else {
            let (selection, surfaces, _) = result.unwrap();
            assert_eq!(
                serde_json::json!({"selection":selection,"surfaces":surfaces}),
                case["expected"],
                "{}",
                case["file"]
            );
        }
        assert_eq!(dmx.bytes(), raw);
    }
}
#[test]
fn python_full_automatic_capsules_none_body_fit_and_bind_rejection() {
    for case in reference()["analysis"].as_array().unwrap() {
        let raw = fs::read(fixtures().join(case["dmx"].as_str().unwrap())).unwrap();
        let dmx = Dmx::parse(raw.clone()).unwrap();
        let options = serde_json::from_value(case["options"].clone()).unwrap();
        let audit =
            serde_json::from_value::<deadlock_skeleton::source_skeleton::SourceAuditReport>(
                case["audit"].clone(),
            )
            .unwrap();
        let cancel = CancellationToken::new();
        let original = physics::original(
            &fixtures().join(case["original"].as_str().unwrap()),
            audit.compiled_sha256.as_deref().unwrap(),
            &audit,
            &cancel,
        )
        .unwrap();
        let result = automatic::analyze(&dmx, &options, &original, &audit, &cancel);
        if case["expected"].get("error").is_some() {
            assert!(result.is_err(), "{}", case["dmx"]);
            continue;
        }
        let actual = result.unwrap();
        assert!(
            !actual.report.runtime_ready
                && !actual.report.published
                && !actual.report.source_modified
                && !actual.report.weights_modified
        );
        let mut expected = case["expected"].clone();
        expected.as_object_mut().unwrap().remove("analysis_sha256");
        let mut observed = serde_json::to_value(actual.report).unwrap();
        observed.as_object_mut().unwrap().remove("analysis_sha256");
        assert_eq!(
            observed
                .as_object_mut()
                .unwrap()
                .remove("analysis_fingerprint_algorithm")
                .unwrap(),
            "rust-sorted-json-v1-sha256"
        );
        same_numeric(&observed, &expected, case["dmx"].as_str().unwrap());
        assert_eq!(dmx.bytes(), raw);
    }
}
#[test]
fn python_inventory_fields_counts_blobs_and_malformed_layouts() {
    for case in reference()["inventory"].as_array().unwrap() {
        let raw = fs::read(fixtures().join(case["file"].as_str().unwrap())).unwrap();
        let result = inventory::solver_inventory(&raw);
        if case["expected"].get("error").is_some() {
            assert!(result.is_err(), "{}", case["file"]);
            continue;
        }
        let mut actual = serde_json::to_value(result.unwrap()).unwrap();
        let mut expected = case["expected"].clone();
        if actual["status"] == "inventoried" {
            let fields = actual.as_object_mut().unwrap();
            assert_eq!(
                fields.remove("solver_fingerprint_algorithm").unwrap(),
                "canonical-kv3-v1-sha256"
            );
            assert_eq!(
                fields
                    .remove("solver_sha256")
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .len(),
                64
            );
            expected.as_object_mut().unwrap().remove("solver_sha256");
        }
        assert_eq!(actual, expected, "{}", case["file"]);
        assert!(!actual["authored_merge_supported"].as_bool().unwrap());
        assert!(!actual["compiled_resources_modified"].as_bool().unwrap());
    }
    let mut raw = fs::read(fixtures().join("inventory-basic.vmdl_c")).unwrap();
    let layout = deadlock_resource::container::layout(&raw).unwrap();
    let ordinal = layout.blocks.iter().position(|b| b.name == "PHYS").unwrap();
    raw[16 + 12 * ordinal..20 + 12 * ordinal].copy_from_slice(b"UNKN");
    let result = serde_json::to_value(inventory::solver_inventory(&raw).unwrap()).unwrap();
    assert_eq!(result["status"], "unavailable");
    assert_eq!(result["physics_block_count"], 0);
    assert!(inventory::solver_inventory(b"invalid").is_err());
}
#[test]
fn cancelled_automatic_selection_never_returns_a_preview() {
    let dmx = Dmx::parse(fs::read(fixtures().join("automatic-top.dmx")).unwrap()).unwrap();
    let cancel = CancellationToken::new();
    cancel.cancel();
    let options =
        serde_json::from_value(serde_json::json!({"meshes":["garment"],"collision":"none"}))
            .unwrap();
    let error = automatic::surface_selection(&dmx, &options, &cancel).unwrap_err();
    assert!(error.to_string().to_lowercase().contains("cancel"));
}
