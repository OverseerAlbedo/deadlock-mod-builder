use deadlock_cloth::*;
use deadlock_resource::{binary, cloth, container, Kind, Value};
use deadlock_skeleton::extensions::SourceBinding;
use sha2::{Digest, Sha256};
mod common;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
fn fixture(name: &str) -> Vec<u8> {
    fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name),
    )
    .unwrap()
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn manifest() -> serde_json::Value {
    serde_json::from_slice(&fixture("manifest.json")).unwrap()
}
fn source(name: &str) -> SourceBinding {
    serde_json::from_slice(&fixture(name)).unwrap()
}
fn tested() -> (Vec<u8>, Vec<u8>, Vec<u8>, SourceBinding) {
    (
        fixture("tested-original.vmdl_c"),
        fixture("tested-reference.vmdl_c"),
        fixture("tested-candidate.vmdl_c"),
        source("tested-source.json"),
    )
}
#[test]
fn original_motion_restoration_matches_python_bytes() {
    for row in manifest()["restorations"].as_array().unwrap() {
        let name = row["name"].as_str().unwrap();
        let original = fixture(&format!("{name}-original.vmdl_c"));
        let candidate = fixture(&format!("{name}-candidate.vmdl_c"));
        let source = row.get("source").map(|s| self::source(s.as_str().unwrap()));
        let (output, receipt) = restore(&original, &candidate, source.as_ref())
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(receipt.status, row["status"].as_str().unwrap(), "{name}");
        assert_eq!(
            hash(&output),
            row["output_sha256"].as_str().unwrap(),
            "{name}"
        );
        assert_eq!(output, fixture(&format!("{name}-output.vmdl_c")), "{name}");
        assert!(!receipt.in_game_motion_tested);
        assert!(receipt.independent_reader_validation_required);
        assert!(
            validate_motion(&original, &output, source.as_ref())
                .unwrap()
                .passed
        );
        let (again, receipt) = restore(&original, &output, source.as_ref()).unwrap();
        assert_eq!(again, output);
        assert!(receipt.status == "already_preserved" || receipt.status == "not_required");
    }
}
#[test]
fn tested_complete_solver_import_matches_python_bytes_and_holds_body_authority() {
    let (original, reference, candidate, source) = tested();
    let expected = inspect_tested(&original, &reference, Some(&source)).unwrap();
    let (output, receipt) =
        apply_tested(&original, &reference, &candidate, &expected, Some(&source)).unwrap();
    assert_eq!(
        hash(&output),
        manifest()["tested"]["output_sha256"].as_str().unwrap()
    );
    assert_eq!(output, fixture("tested-output.vmdl_c"));
    assert!(receipt.audit.motion.passed);
    assert!(receipt.audit.all_tested_solver_fields_preserved);
    assert!(receipt.original_rigid_body_physics_preserved);
    assert!(!receipt.original_cloth_solver_preserved);
    assert!(!receipt.in_game_motion_tested);
    assert!(receipt.independent_reader_validation_required);
    assert_eq!(
        expected.target.extra_names,
        vec!["custom_skirt_root", "custom_skirt_tip"]
    );
    assert!(
        validate_tested(&original, &reference, &output, &expected, Some(&source))
            .unwrap()
            .motion
            .passed
    );
    assert!(restore(&original, &output, Some(&source)).is_err());
    assert!(inspect_tested(&original, &reference, None).is_err());
    let mut changed = expected.clone();
    changed.qualification.reference_sha256 = "0".repeat(64);
    assert!(apply_tested(&original, &reference, &candidate, &changed, Some(&source)).is_err());
    changed = expected.clone();
    changed.original_sha256 = "0".repeat(64);
    assert!(validate_tested(&original, &reference, &output, &changed, Some(&source)).is_err());
}
#[test]
fn python_rejection_gates_remain_closed() {
    let (original, reference, _, source) = tested();
    let expected = inspect_tested(&original, &reference, Some(&source)).unwrap();
    for row in manifest()["rejects"].as_array().unwrap() {
        let name = row["name"].as_str().unwrap();
        let raw = fixture(&format!("reject-{name}.vmdl_c"));
        let error = match row["stage"].as_str().unwrap() {
            "apply" => apply_tested(&original, &reference, &raw, &expected, Some(&source))
                .map(|_| ())
                .err(),
            "qualify" => qualify_tested(&raw).map(|_| ()).err(),
            "validate" => validate_tested(&original, &reference, &raw, &expected, Some(&source))
                .map(|_| ())
                .err(),
            _ => panic!("stage"),
        };
        assert!(error.is_some(), "accepted Python rejection: {name}");
    }
}
/// The accepted Vindicta reference (private assets), or `None` with a note when it is not checked out.
fn path() -> Option<PathBuf> {
    common::private_path("physics-references/Vindicta-R12-Balanced/cloth_supported_hair_balanced.vmdl_c")
}
#[test]
fn accepted_native_baseline_qualifies_without_modification() {
    let Some(path) = path() else { return };
    let raw = fs::read(&path).unwrap();
    assert_eq!(hash(&raw), cloth::BALANCED_REFERENCE_SHA256);
    let proof = qualify_tested(&raw).unwrap();
    assert_eq!(proof.structure.node_count, 8381);
    assert_eq!(proof.structure.static_nodes, 380);
    assert_eq!(proof.reference_summary.skeleton.names.len(), 95);
    assert_eq!(proof.capacity.status, cloth::Status::Passed);
    assert_eq!(proof.constraint_feasibility.status, cloth::Status::Passed);
    assert!(!proof.in_game_motion_tested);
    assert_eq!(
        hash(&fs::read(&path).unwrap()),
        cloth::BALANCED_REFERENCE_SHA256
    );
}
#[test]
fn original_mode_refuses_unknown_solver_annotations_and_body_dependencies() {
    let original = fixture("simple-original.vmdl_c");
    let candidate = fixture("simple-candidate.vmdl_c");
    let layout = container::layout(&original).unwrap();
    let block = container::unique(&layout.blocks, "PHYS").unwrap();
    let mut doc = binary::decode(container::payload(&original, block).unwrap()).unwrap();
    if let Kind::Object(rows) = &mut doc.root.get_mut("m_pFeModel").unwrap().kind {
        rows.push(("unknown_retained".into(), Value::new(Kind::F64(2.))));
    }
    let changed = container::rebuild(
        &original,
        &BTreeMap::from([(
            "PHYS".into(),
            binary::encode(&doc.root, doc.format_guid).unwrap(),
        )]),
    )
    .unwrap();
    assert!(restore(&original, &changed, None).is_err());
    let mut trailing = candidate.clone();
    trailing.push(0);
    assert!(restore(&original, &trailing, None).is_err());
}
#[test]
fn no_physics_is_supported_but_external_or_multiple_physics_is_blocked() {
    let raw = fixture("no-phys.vmdl_c");
    let audit = audit_resource(&raw).unwrap();
    assert_eq!(audit.cloth_nodes, 0);
    assert_eq!(audit.solver_sha256, None);
    let (output, receipt) = restore(&raw, &raw, None).unwrap();
    assert_eq!(output, raw);
    assert_eq!(receipt.status, "not_required");
    assert!(validate_motion(&raw, &raw, None).unwrap().passed);
    for name in ["external-phys.vmdl_c", "multiple-phys.vmdl_c"] {
        let invalid = fixture(name);
        assert!(audit_resource(&invalid).is_err());
        assert!(restore(&invalid, &raw, None).is_err());
        assert!(restore(&raw, &invalid, None).is_err());
        assert!(validate_motion(&raw, &invalid, None).is_err());
    }
}

fn edit_block(raw: &[u8], name: &str, ordinal: usize, edit: impl FnOnce(&mut Value)) -> Vec<u8> {
    let layout = container::layout(raw).unwrap();
    let (at, block) = layout
        .blocks
        .iter()
        .enumerate()
        .filter(|(_, b)| b.name == name)
        .nth(ordinal)
        .unwrap();
    let mut doc = binary::decode(container::payload(raw, block).unwrap()).unwrap();
    edit(&mut doc.root);
    container::rebuild_ordinals(
        raw,
        &BTreeMap::from([(at, binary::encode(&doc.root, doc.format_guid).unwrap())]),
    )
    .unwrap()
}
fn no_solver(raw: &[u8], no_phys: bool) -> Vec<u8> {
    let mut output = edit_block(raw, "PHYS", 0, |v| {
        v.get_mut("m_pFeModel").unwrap().kind = Kind::Null
    });
    if no_phys {
        output = edit_block(&output, "CTRL", 0, |v| {
            let Kind::Object(rows) = &mut v.kind else {
                panic!()
            };
            rows.retain(|(key, _)| key != "embedded_physics");
        });
        let layout = container::layout(&output).unwrap();
        let ordinal = layout.blocks.iter().position(|b| b.name == "PHYS").unwrap();
        // Preserve all other block ordinals, including opaque vertex/index data.
        // An unknown payload has no PHYS meaning and no physical fields are used.
        output[16 + 12 * ordinal..20 + 12 * ordinal].copy_from_slice(b"TEST");
    }
    output
}
#[test]
fn authorized_no_cloth_extensions_require_complete_bindings_and_source_authority() {
    for absent in [false, true] {
        let original = no_solver(&fixture("extended-original.vmdl_c"), absent);
        let candidate = no_solver(&fixture("extended-candidate.vmdl_c"), absent);
        let mut source = source("extended-source.json");
        source.original_sha256 = hash(&original);
        let audit = validate_motion(&original, &candidate, Some(&source)).unwrap();
        assert!(audit.passed);
        assert!(!audit.required);
        assert_eq!(audit.original.cloth_nodes, 0);
        assert_eq!(audit.original.solver_sha256, None);
        let proof = audit.skeleton_extension.unwrap();
        assert!(proof.source_binding_checked && proof.compiled_bindings.passed);
        assert_eq!(proof.identity.extra_count, 2);
        assert_eq!(
            restore(&original, &candidate, Some(&source)).unwrap().0,
            candidate
        );
        assert!(validate_motion(&original, &candidate, None).is_err());
        assert!(audit_skeleton_extension(&original, &candidate, None).is_err());
        assert!(validate_motion(&original, &original, Some(&source)).is_err());
        let mut unauthorized = source.clone();
        unauthorized.extra_names.pop();
        assert!(validate_motion(&original, &candidate, Some(&unauthorized)).is_err());
        let mut stale = source.clone();
        stale.original_sha256 = "0".repeat(64);
        assert!(validate_motion(&original, &candidate, Some(&stale)).is_err());
        let inverse = edit_block(&candidate, "MDAT", 0, |v| {
            let Kind::Array(bones) = &mut v
                .get_mut("m_skeleton")
                .unwrap()
                .get_mut("m_bones")
                .unwrap()
                .kind
            else {
                panic!()
            };
            let Kind::Array(inverse) = &mut bones
                .last_mut()
                .unwrap()
                .get_mut("m_invBindPose")
                .unwrap()
                .kind
            else {
                panic!()
            };
            inverse[0].kind = Kind::F64(99.);
        });
        assert!(validate_motion(&original, &inverse, Some(&source)).is_err());
        let remap = edit_block(&candidate, "DATA", 0, |v| {
            let Kind::Array(values) = &mut v.get_mut("m_remappingTableStarts").unwrap().kind else {
                panic!()
            };
            values[0].kind = Kind::I64(65536);
        });
        assert!(validate_motion(&original, &remap, Some(&source)).is_err());
        let controls = edit_block(&candidate, "CTRL", 0, |v| {
            let Kind::Array(values) = &mut v.get_mut("embedded_meshes").unwrap().kind else {
                panic!()
            };
            values[1] = values[0].clone();
        });
        assert!(validate_motion(&original, &controls, Some(&source)).is_err());
        let flags = edit_block(&candidate, "DATA", 0, |v| {
            let Kind::Array(values) = &mut v
                .get_mut("m_modelSkeleton")
                .unwrap()
                .get_mut("m_nFlag")
                .unwrap()
                .kind
            else {
                panic!()
            };
            values.last_mut().unwrap().kind = Kind::I64(0x400008);
        });
        assert!(validate_motion(&original, &flags, Some(&source)).is_err());
    }
    let mut source = source("extended-source.json");
    let original = edit_block(
        &no_solver(&fixture("extended-original.vmdl_c"), false),
        "PHYS",
        0,
        |v| {
            let Kind::Object(rows) = &mut v.kind else {
                panic!()
            };
            rows.push((
                "m_boneNames".into(),
                Value::new(Kind::Array(vec![Value::new(Kind::String(
                    "custom_skirt_root".into(),
                ))])),
            ));
        },
    );
    source.original_sha256 = hash(&original);
    assert!(validate_motion(
        &original,
        &no_solver(&fixture("extended-candidate.vmdl_c"), false),
        Some(&source)
    )
    .is_err());
}
