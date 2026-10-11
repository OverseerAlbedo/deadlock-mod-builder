use deadlock_cloth_authoring::physics::{self, AnalysisOptions};
use deadlock_core::CancellationToken;
use deadlock_dmx::Dmx;
use deadlock_skeleton::{
    source_identity::{source_contract, SourceContract},
    source_skeleton::SourceAuditReport,
};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fs, path::PathBuf};
#[derive(Deserialize)]
struct Fixture {
    audit: SourceAuditReport,
    selected: BTreeSet<String>,
    options: AnalysisOptions,
    snapshot: bool,
    expected: Value,
}
fn fixture(name: &str) -> (PathBuf, Fixture, Dmx, Option<SourceContract>) {
    let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/physics")
        .join(name);
    let fixture: Fixture =
        serde_json::from_slice(&fs::read(folder.join("oracle.json")).unwrap()).unwrap();
    let dmx = Dmx::load(&folder.join("source.dmx")).unwrap();
    let sha = format!("{:x}", Sha256::digest(dmx.bytes()));
    let contract = source_contract(&dmx, &fixture.audit, &fixture.selected, &sha).unwrap();
    (folder, fixture, dmx, contract)
}
fn compare(a: &Value, b: &Value, path: &str) {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => {
            let a = a.as_f64().unwrap();
            let b = b.as_f64().unwrap();
            assert!(
                (a - b).abs() <= 2e-12 * a.abs().max(b.abs()).max(1.),
                "{path}: {a} != {b}"
            );
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                compare(a, b, &format!("{path}[{i}]"));
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(
                a.keys().collect::<BTreeSet<_>>(),
                b.keys().collect(),
                "{path}"
            );
            for (k, a) in a {
                compare(a, &b[k], &format!("{path}.{k}"));
            }
        }
        _ => assert_eq!(a, b, "{path}"),
    }
}
#[test]
fn proposals_match_independent_python_for_all_analysis_paths() {
    for name in [
        "ragdoll",
        "scale",
        "rounded",
        "noextras",
        "missing_snapshot",
        "ambiguous",
        "explicit_role",
        "branching",
        "upward",
        "zero_radius",
        "negative_radius",
        "fallback",
        "no_phys",
        "ambiguous_phys",
        "explicit_empty_roots",
        "collision_subset",
    ] {
        let (folder, f, dmx, c) = fixture(name);
        let before = dmx.bytes().to_vec();
        let original = fs::read(folder.join("original.vmdl_c")).unwrap();
        let snapshot = folder.join("original.vmdl_c");
        let result = physics::analyze(
            &dmx,
            &f.audit,
            f.snapshot.then_some(snapshot.as_path()),
            c.as_ref(),
            &f.options,
            &CancellationToken::new(),
        )
        .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(
            !result.runtime_ready
                && !result.ready_for_authoring
                && !result.solver_written
                && !result.source_modified
                && !result.weights_modified
        );
        let mut actual = serde_json::to_value(result).unwrap();
        let mut expected = f.expected;
        // The Rust report uses its own canonical JSON numeric encoding. Both identities bind
        // their exact report bytes; geometry is compared independently below.
        let digest = actual.as_object_mut().unwrap().remove("analysis_sha256");
        expected.as_object_mut().unwrap().remove("analysis_sha256");
        if let Some(digest) = digest {
            actual.sort_all_objects();
            assert_eq!(
                digest.as_str().unwrap(),
                format!("{:x}", Sha256::digest(serde_json::to_vec(&actual).unwrap()))
            );
        }
        compare(&actual, &expected, name);
        assert_eq!(before, dmx.bytes());
        assert_eq!(original, fs::read(snapshot).unwrap());
    }
}
#[test]
fn stale_contract_snapshot_audit_and_flags_fail_closed() {
    let (folder, mut f, dmx, contract) = fixture("ragdoll");
    let mut c = contract.unwrap();
    let cancel = CancellationToken::new();
    let original = folder.join("original.vmdl_c");
    c.source_sha256 = "f".repeat(64);
    assert!(physics::analyze(
        &dmx,
        &f.audit,
        Some(&original),
        Some(&c),
        &f.options,
        &cancel
    )
    .unwrap_err()
    .to_string()
    .contains("stale"));
    let temp = tempfile::tempdir().unwrap();
    let changed = temp.path().join("changed.vmdl_c");
    fs::write(&changed, b"changed").unwrap();
    assert!(physics::original(
        &changed,
        f.audit.compiled_sha256.as_ref().unwrap(),
        &f.audit,
        &cancel
    )
    .unwrap_err()
    .to_string()
    .contains("snapshot changed"));
    f.audit.runtime_motion.resource_sha256 = Some("f".repeat(64));
    assert!(physics::original(
        &original,
        f.audit.compiled_sha256.as_ref().unwrap(),
        &f.audit,
        &cancel
    )
    .unwrap_err()
    .to_string()
    .contains("digest"));
    f.audit.runtime_motion.resource_sha256 = f.audit.compiled_sha256.clone();
    f.audit.runtime_motion.skeleton.as_mut().unwrap().flags[0] = 0;
    assert!(physics::original(
        &original,
        f.audit.compiled_sha256.as_ref().unwrap(),
        &f.audit,
        &cancel
    )
    .unwrap_err()
    .to_string()
    .contains("flags"));
}
#[test]
fn cancellation_and_unknown_or_unvalidated_selections_are_rejected() {
    let (folder, f, dmx, c) = fixture("ragdoll");
    let before = dmx.bytes().to_vec();
    let snapshot = folder.join("original.vmdl_c");
    let cancel = CancellationToken::new();
    cancel.cancel();
    assert!(physics::analyze(
        &dmx,
        &f.audit,
        Some(&snapshot),
        c.as_ref(),
        &f.options,
        &cancel
    )
    .is_err());
    assert_eq!(before, dmx.bytes());
    assert!(serde_json::from_str::<AnalysisOptions>(r#"{"bypass":true}"#).is_err());
    for options in [
        r#"{"roots":["garment_end"]}"#,
        r#"{"roots":["garment_top","garment_top"]}"#,
        r#"{"roles":{"thigh_L":"garment_top"}}"#,
        r#"{"roles":{"arm":"pelvis"}}"#,
        r#"{"collision_meshes":["missing"]}"#,
    ] {
        let options = serde_json::from_str(options).unwrap();
        assert!(
            physics::analyze(
                &dmx,
                &f.audit,
                Some(&snapshot),
                c.as_ref(),
                &options,
                &CancellationToken::new()
            )
            .is_err(),
            "{options:?}"
        );
    }
    assert!(physics::distance([0.; 3], [0.; 3], [0.; 3]).is_err());
    assert_eq!(
        physics::distance([3., 2., 0.], [0., 0., 0.], [1., 0., 0.]).unwrap(),
        8f64.sqrt()
    );
}

#[test]
fn original_without_cloth_supports_verified_source_geometry_without_solver_authority() {
    let (folder, mut f, dmx, _) = fixture("fallback");
    f.audit.runtime_motion.cloth_nodes = 0;
    let sha = format!("{:x}", Sha256::digest(dmx.bytes()));
    let contract = source_contract(&dmx, &f.audit, &f.selected, &sha)
        .unwrap()
        .expect("No-cloth original must permit a verified extra-bone source contract");
    let snapshot = folder.join("original.vmdl_c");
    let result = physics::analyze(
        &dmx,
        &f.audit,
        Some(&snapshot),
        Some(&contract),
        &f.options,
        &CancellationToken::new(),
    )
    .unwrap();
    assert_eq!(result.summary.chain_count, 1);
    assert_eq!(result.summary.leg_collider_count, 4);
    assert!(result.setup_ready_for_review);
    assert!(!result.runtime_ready && !result.ready_for_authoring && !result.solver_written);
    assert!(result
        .limitations
        .iter()
        .any(|s| s.contains("does not establish a new native solver")));
    assert!(!result.pending_gates.is_empty());
    let absent = physics::analyze(
        &dmx,
        &f.audit,
        Some(&snapshot),
        None,
        &f.options,
        &CancellationToken::new(),
    )
    .unwrap();
    assert!(!absent.setup_ready_for_review);
    assert!(absent.blockers.iter().any(|s| s.contains("No validated")));
    assert!(!absent
        .blockers
        .iter()
        .any(|s| s.contains("not implemented")));
}
