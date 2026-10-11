use deadlock_resource::{container, Kind, Value};
use deadlock_skeleton::{
    bindings, check_transfer, extensions, skeleton_identity, source_skeleton, transforms, Skeleton,
};
use serde_json::Value as Json;
use std::{fs, path::PathBuf};
fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}
fn json(name: &str) -> Json {
    serde_json::from_slice(&fs::read(fixtures().join(name)).unwrap()).unwrap()
}
fn close(actual: &Json, expected: &Json, path: &str) {
    match (actual, expected) {
        (Json::Number(a), Json::Number(b)) => {
            let a = a.as_f64().unwrap();
            let b = b.as_f64().unwrap();
            let limit = if path.contains("rotation_error_degrees") {
                2e-6
            } else {
                1e-11 * (1. + b.abs())
            };
            assert!((a - b).abs() <= limit, "{path}: {a} != {b}");
        }
        (Json::Array(a), Json::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                close(a, b, &format!("{path}[{i}]"));
            }
        }
        (Json::Object(a), Json::Object(b)) => {
            assert_eq!(
                a.keys().collect::<std::collections::BTreeSet<_>>(),
                b.keys().collect::<std::collections::BTreeSet<_>>(),
                "{path}"
            );
            for (key, a) in a {
                close(a, &b[key], &format!("{path}.{key}"));
            }
        }
        _ => assert_eq!(actual, expected, "{path}"),
    }
}
fn read(raw: &[u8], name: &str) -> Vec<Value> {
    container::read_blocks(raw, name)
        .unwrap()
        .into_iter()
        .map(|d| d.root)
        .collect()
}
fn skeleton(raw: &[u8]) -> Skeleton {
    Skeleton::from_value(read(raw, "DATA")[0].get("m_modelSkeleton").unwrap()).unwrap()
}
fn audit(raw: &[u8]) -> bindings::BindingAudit {
    let layout = container::layout(raw).unwrap();
    bindings::audit_bindings(
        &read(raw, "DATA")[0],
        &read(raw, "CTRL")[0],
        &read(raw, "MDAT"),
        &read(raw, "AGRP"),
        &layout.blocks,
    )
    .unwrap()
}
#[test]
fn local_inherited_scale_world_binds_and_inverse_match_python() {
    for case in json("transforms.json").as_array().unwrap() {
        let s: Skeleton = serde_json::from_value(case["skeleton"].clone()).unwrap();
        let before = s.clone();
        let (bones, audit) = transforms::prepare_target(&s).unwrap();
        close(
            &serde_json::to_value(bones).unwrap(),
            &case["bones"],
            "bones",
        );
        close(
            &serde_json::to_value(audit).unwrap(),
            &case["scale_audit"],
            "scale_audit",
        );
        assert_eq!(s, before);
    }
}
#[test]
fn native_and_synthetic_compiled_mesh_animation_bindings_match_python() {
    for name in ["canonical", "export-alias", "reordered"] {
        let raw = fs::read(fixtures().join(format!("{name}.vmdl_c"))).unwrap();
        close(
            &serde_json::to_value(audit(&raw)).unwrap(),
            &json(&format!("{name}-bindings.json")),
            name,
        );
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    for (name, path) in [
        (
            "r12-original",
            "physics-references/Vindicta-R12/cloth_supported_hair.vmdl_c",
        ),
        (
            "r12-balanced",
            "physics-references/Vindicta-R12-Balanced/cloth_supported_hair_balanced.vmdl_c",
        ),
    ] {
        let raw = fs::read(root.join(path)).expect("Bundled R12 compiled fixture is required");
        close(
            &serde_json::to_value(audit(&raw)).unwrap(),
            &json(&format!("{name}-bindings.json")),
            name,
        );
    }
}
#[test]
fn extension_and_source_authority_match_python() {
    let original = fs::read(fixtures().join("extension-original.vmdl_c")).unwrap();
    let actual = fs::read(fixtures().join("extension.vmdl_c")).unwrap();
    let expected = json("extension.json");
    let source: extensions::SourceBinding =
        serde_json::from_value(expected["source"].clone()).unwrap();
    close(
        &serde_json::to_value(
            extensions::extension_identity(&skeleton(&actual), &skeleton(&original)).unwrap(),
        )
        .unwrap(),
        &expected["identity"],
        "extension",
    );
    close(
        &serde_json::to_value(
            extensions::source_binding(
                &skeleton(&actual),
                &skeleton(&original),
                &source,
                &source.original_sha256,
            )
            .unwrap(),
        )
        .unwrap(),
        &expected["source_binding"],
        "source_binding",
    );
    close(
        &serde_json::to_value(audit(&actual)).unwrap(),
        &expected["bindings"],
        "bindings",
    );
    let mut changed = source.clone();
    changed.original_sha256 = "wrong".into();
    assert!(extensions::source_binding(
        &skeleton(&actual),
        &skeleton(&original),
        &changed,
        &source.original_sha256
    )
    .is_err());
}
#[test]
fn exact_f32_scale_and_cumulative_budgets_cannot_be_relaxed() {
    let mut s: Skeleton =
        serde_json::from_value(json("transforms.json")[1]["skeleton"].clone()).unwrap();
    let mut actual = s.clone();
    actual.scales[0] = s.scales[0] as f32 as f64;
    assert!(transforms::audit_scale_preservation(&actual, &s, true).is_ok());
    actual.scales[0] = f32::from_bits((s.scales[0] as f32).to_bits() + 1) as f64;
    assert!(transforms::audit_scale_preservation(&actual, &s, true).is_err());
    let count = s.names.len();
    s.parents = (0..count).map(|i| i as i32 - 1).collect();
    s.positions = vec![[0.; 3]; count];
    s.scales = vec![1.00009; count];
    let mut actual = s.clone();
    actual.scales = vec![1.; count];
    assert!(skeleton_identity(&actual, &s).is_ok());
    assert!(transforms::audit_scale_preservation(&actual, &s, false).is_err());
    for scale in [0., -1., f64::NAN, f64::INFINITY, 1e-50, 1e100] {
        let mut bad = s.clone();
        bad.scales[0] = scale;
        assert!(transforms::bind_state(&bad).is_err());
    }
}
#[test]
fn malformed_binds_parents_flags_and_extra_roots_are_rejected() {
    let raw = fs::read(fixtures().join("canonical.vmdl_c")).unwrap();
    let s = skeleton(&raw);
    let mut bad = s.clone();
    bad.parents[0] = 1;
    assert!(transforms::bind_state(&bad).is_err());
    let mut bad = s.clone();
    bad.positions[1][0] = 0.001;
    assert!(check_transfer(&bad, &s).is_err());
    let mut bad = s.clone();
    bad.rotations[1] = [0.; 4];
    assert!(skeleton_identity(&bad, &s).is_err());
    let raw = fs::read(fixtures().join("extension.vmdl_c")).unwrap();
    let original = skeleton(&fs::read(fixtures().join("extension-original.vmdl_c")).unwrap());
    let mut extra = skeleton(&raw);
    extra.flags[3] |= 8;
    assert!(extensions::extension_identity(&extra, &original).is_err());
    extra.flags[3] &= !8;
    extra.parents[3] = -1;
    assert!(extensions::extension_identity(&extra, &original).is_err());
}
#[test]
fn source_aliases_require_same_resource_and_support_cli_tokens() {
    let raw = fs::read(fixtures().join("canonical.vmdl_c")).unwrap();
    let s = skeleton(&raw);
    let sha = "a".repeat(64);
    let mut value = serde_json::json!({"compiled_sha256":sha,"runtime_motion":{"status":"audited","resource_sha256":sha,"cloth_nodes":3,"skeleton":s}});
    let report: source_skeleton::SourceAuditReport = serde_json::from_value(value.clone()).unwrap();
    let aliases = source_skeleton::canonical_aliases(&report).unwrap();
    assert_eq!(aliases["_cloth_a"], "$cloth_a");
    let object = value["runtime_motion"]["skeleton"].as_object_mut().unwrap();
    for key in ["m_nParent", "m_nFlag", "m_boneScaleParent"] {
        for row in object[key].as_array_mut().unwrap() {
            *row = Json::String(row.to_string());
        }
    }
    for key in ["m_bonePosParent", "m_boneRotParent"] {
        for row in object[key].as_array_mut().unwrap() {
            for n in row.as_array_mut().unwrap() {
                *n = Json::String(n.to_string());
            }
        }
    }
    let parsed: source_skeleton::SourceAuditReport = serde_json::from_value(value).unwrap();
    assert_eq!(
        source_skeleton::canonical_aliases(&parsed).unwrap(),
        aliases
    );
    let mut bad = report;
    bad.runtime_motion.resource_sha256 = Some("b".repeat(64));
    assert!(source_skeleton::canonical_aliases(&bad).is_err());
    bad.runtime_motion.status = "unavailable".into();
    assert!(source_skeleton::canonical_aliases(&bad).unwrap().is_empty());
}
#[test]
fn corrupted_compiled_binding_tables_are_rejected() {
    let raw = fs::read(fixtures().join("canonical.vmdl_c")).unwrap();
    let layout = container::layout(&raw).unwrap();
    let data = read(&raw, "DATA").remove(0);
    let ctrl = read(&raw, "CTRL").remove(0);
    let meshes = read(&raw, "MDAT");
    let anims = read(&raw, "AGRP");
    let mut bad = data.clone();
    bad.get_mut("m_remappingTable").unwrap().kind =
        Kind::Array(vec![Value::new(Kind::I64(999)); 5]);
    assert!(bindings::audit_bindings(&bad, &ctrl, &meshes, &anims, &layout.blocks).is_err());
    let mut bad = meshes.clone();
    let bone = &mut match &mut bad[0]
        .get_mut("m_skeleton")
        .unwrap()
        .get_mut("m_bones")
        .unwrap()
        .kind
    {
        Kind::Array(v) => v,
        _ => unreachable!(),
    }[0];
    bone.get_mut("m_invBindPose").unwrap().kind =
        Kind::Array((0..12).map(|_| Value::new(Kind::F64(0.))).collect());
    assert!(bindings::audit_bindings(&data, &ctrl, &bad, &anims, &layout.blocks).is_err());
    let mut bad = anims;
    bad[0]
        .get_mut("m_decodeKey")
        .unwrap()
        .get_mut("m_nChannelElements")
        .unwrap()
        .kind = Kind::I64(99);
    assert!(bindings::audit_bindings(&data, &ctrl, &meshes, &bad, &layout.blocks).is_err());
}
#[test]
fn alias_repairs_are_byte_identical_to_python_and_idempotent() {
    let original = fs::read(fixtures().join("canonical.vmdl_c")).unwrap();
    for label in ["alias", "alias-reordered"] {
        let input = fs::read(fixtures().join(format!("{label}-input.vmdl_c"))).unwrap();
        let (fixed, receipt) =
            deadlock_skeleton::aliases::repair_aliases(&original, &input, false).unwrap();
        assert_eq!(
            fixed,
            fs::read(fixtures().join(format!("{label}-expected.vmdl_c"))).unwrap()
        );
        close(
            &serde_json::to_value(receipt.unwrap()).unwrap(),
            &json(&format!("{label}-receipt.json")),
            label,
        );
        let (again, receipt) =
            deadlock_skeleton::aliases::repair_aliases(&original, &fixed, false).unwrap();
        assert_eq!(again, fixed);
        assert!(receipt.is_none());
    }
}
#[test]
fn prepared_export_exact_bytes_bind_scale_skin_and_unicode_match_python() {
    for label in ["prepared", "prepared-streams", "prepared-scale"] {
        let expected = json(&format!("{label}.json"));
        let s: Skeleton = serde_json::from_value(expected["skeleton"].clone()).unwrap();
        let input: deadlock_skeleton::export::PreparedReport =
            serde_json::from_value(expected["input"].clone()).unwrap();
        let (raw, audit) = deadlock_skeleton::export::export_bytes(&input, &s).unwrap();
        assert_eq!(
            raw,
            fs::read(fixtures().join(format!("{label}.dmx"))).unwrap(),
            "{label}"
        );
        close(
            &serde_json::to_value(audit).unwrap(),
            &expected["audit"],
            label,
        );
        let mut bad = input.clone();
        bad.meshes[0].weights[0][0].1 = f64::NAN;
        assert!(deadlock_skeleton::export::export_bytes(&bad, &s).is_err());
        let mut bad = input.clone();
        bad.meshes[0].faces[0].corners[0] = 1;
        assert!(deadlock_skeleton::export::export_bytes(&bad, &s).is_err());
    }
}
#[test]
fn byte_derived_source_authority_and_template_contract_match_python() {
    use deadlock_skeleton::{source_bones, source_identity as source};
    use sha2::{Digest, Sha256};
    for label in [
        "extension-source",
        "extension-loose",
        "extension-unselected",
        "extension-zero",
        "extension-implicit",
        "extension-alias",
    ] {
        let expected = json(&format!("{label}.json"));
        let original: source_skeleton::SourceAuditReport =
            serde_json::from_value(expected["original"].clone()).unwrap();
        let selected = serde_json::from_value(expected["selected"].clone()).unwrap();
        let d = deadlock_dmx::Dmx::load(&fixtures().join(format!("{label}.dmx"))).unwrap();
        let mut actual = serde_json::to_value(source::inspect_identity(&d, &original)).unwrap();
        let mut inspection = expected["inspection"].clone();
        assert_eq!(
            actual["errors"].as_array().unwrap().len(),
            inspection["errors"].as_array().unwrap().len()
        );
        actual.as_object_mut().unwrap().remove("errors");
        inspection.as_object_mut().unwrap().remove("errors");
        close(&actual, &inspection, label);
        let sha = format!("{:x}", Sha256::digest(d.bytes()));
        let contract = source::source_contract(&d, &original, &selected, &sha).unwrap();
        close(
            &serde_json::to_value(&contract).unwrap(),
            &expected["contract"],
            label,
        );
        close(
            &serde_json::to_value(deadlock_skeleton::skinning::skinning(&d).unwrap()).unwrap(),
            &expected["skinning"],
            label,
        );
        let preflight = source::preflight(&d, &original, &selected, None).unwrap();
        assert!(!preflight.blocked);
        if let Some(contract) = contract {
            let required = serde_json::from_value(expected["required"].clone()).unwrap();
            let audit =
                source_bones::template_contract(&d, &required, &original, &contract, None).unwrap();
            close(
                &serde_json::to_value(audit).unwrap(),
                &expected["template_audit"],
                label,
            );
            if label == "extension-source" {
                let reference = serde_json::from_value(json("extension-reference.json")).unwrap();
                let audit = source_bones::template_contract(
                    &d,
                    &required,
                    &original,
                    &contract,
                    Some(&reference),
                )
                .unwrap();
                close(
                    &serde_json::to_value(audit).unwrap(),
                    &expected["marked_template_audit"],
                    label,
                );
            }
            let mut stale = contract.clone();
            stale.skeleton.positions[2][0] += 1.;
            assert!(
                source_bones::template_contract(&d, &required, &original, &stale, None).is_err()
            );
            assert!(source::source_contract(&d, &original, &selected, &"f".repeat(64)).is_err());
            let mut collision = original.clone();
            collision.runtime_motion.controls.push("hem".into());
            assert!(source::source_contract(&d, &collision, &selected, &sha).is_err());
        }
    }
}
#[test]
fn unauthorized_alias_schema_and_corrupt_bindings_fail_without_mutation() {
    let original = fs::read(fixtures().join("canonical.vmdl_c")).unwrap();
    for label in json("alias-bad.json").as_array().unwrap() {
        let raw =
            fs::read(fixtures().join(format!("alias-bad-{}.vmdl_c", label.as_str().unwrap())))
                .unwrap();
        let before = raw.clone();
        assert!(
            deadlock_skeleton::aliases::repair_aliases(&original, &raw, false).is_err(),
            "{label}"
        );
        assert_eq!(raw, before);
    }
}
#[test]
fn malformed_current_bind_graph_palette_and_exact_scales_are_rejected() {
    let expected = json("prepared.json");
    let skeleton: Skeleton = serde_json::from_value(expected["skeleton"].clone()).unwrap();
    for label in json("bind-bad.json").as_array().unwrap() {
        let d = deadlock_dmx::Dmx::load(
            &fixtures().join(format!("bind-bad-{}.dmx", label.as_str().unwrap())),
        )
        .unwrap();
        assert!(
            deadlock_skeleton::dmx_bind::audit_dmx_bind(&d, &skeleton, true).is_err(),
            "{label}"
        );
    }
    let d = deadlock_dmx::Dmx::load(&fixtures().join("prepared.dmx")).unwrap();
    let reference = serde_json::from_value(json("prepared-reference.json")).unwrap();
    let expected = json("prepared-source-bones.json");
    let required = serde_json::from_value(expected["required"].clone()).unwrap();
    let actual = deadlock_skeleton::source_bones::source_bones(
        &d,
        &required,
        Some(&reference),
        Some(&"a".repeat(64)),
    )
    .unwrap();
    close(
        &serde_json::to_value(actual).unwrap(),
        &expected["audit"],
        "source_bones",
    );
}
#[test]
#[ignore = "requires recovered real V22 source and independently generated Python source-bone oracle"]
fn recovered_v22_source_bones_skin_palette_and_target_transforms_match_python() {
    let folder = PathBuf::from(std::env::var("DMB_V22_FIXTURE").expect("DMB_V22_FIXTURE required"));
    let expected: Json =
        serde_json::from_slice(&fs::read(folder.join("rust-skeleton-oracle.json")).unwrap())
            .unwrap();
    let d = deadlock_dmx::Dmx::load(&folder.join("Collection.dmx")).unwrap();
    let reference: deadlock_skeleton::source_bones::SkeletonReference =
        serde_json::from_value(expected["reference"].clone()).unwrap();
    let required = serde_json::from_value(expected["required"].clone()).unwrap();
    let audit = deadlock_skeleton::source_bones::source_bones(
        &d,
        &required,
        Some(&reference),
        Some(&reference.sha256),
    )
    .unwrap();
    close(
        &serde_json::to_value(audit).unwrap(),
        &expected["source_bones"],
        "nativeV22.source_bones",
    );
    let (bones, scale) = transforms::prepare_target(&reference.skeleton).unwrap();
    close(
        &serde_json::to_value(bones).unwrap(),
        &expected["prepared_bones"],
        "nativeV22.prepared",
    );
    close(
        &serde_json::to_value(scale).unwrap(),
        &expected["scale_audit"],
        "nativeV22.scale",
    );
}

#[test]
#[ignore = "requires recovered R11 authored mesh and independently generated Python oracle"]
fn recovered_r11_authored_mesh_matches_python_and_r12_bind_gate_decisions() {
    let folder = PathBuf::from(std::env::var("DMB_R11_FIXTURE").expect("DMB_R11_FIXTURE required"));
    let expected: Json =
        serde_json::from_slice(&fs::read(folder.join("rust-oracle.json")).unwrap()).unwrap();
    let path = folder.join("mesh_001.dmx");
    assert_eq!(
        deadlock_core::safety::digest(&path).unwrap(),
        "cfddc3ead3b641347ff724c5228cdb4675237a4baf7acc5f96e453bcac3d59d0"
    );
    let d = deadlock_dmx::Dmx::load(&path).unwrap();
    assert_eq!(
        serde_json::to_value(d.summary().unwrap()).unwrap(),
        expected["summary"]
    );
    assert_eq!(
        serde_json::to_value(d.geometry_counts().unwrap()).unwrap(),
        expected["geometry"]
    );
    for (name, hash) in expected["single_mesh_sha256"].as_object().unwrap() {
        use sha2::{Digest, Sha256};
        assert_eq!(
            format!("{:x}", Sha256::digest(d.single_mesh(name).unwrap())),
            hash.as_str().unwrap()
        );
    }
    for (label, proof) in expected["compiled_references"].as_object().unwrap() {
        let reference: Skeleton = serde_json::from_value(proof["skeleton"].clone()).unwrap();
        for (explicit, key) in [(true, "bind_explicit"), (false, "bind_legacy")] {
            let actual = deadlock_skeleton::dmx_bind::audit_dmx_bind(&d, &reference, explicit);
            assert_eq!(
                actual.is_ok(),
                proof[key]["passed"].as_bool().unwrap(),
                "{label}.{key}"
            );
            if let Ok(actual) = actual {
                close(
                    &serde_json::to_value(actual).unwrap(),
                    &proof[key]["audit"],
                    key,
                );
            }
        }
        let original: source_skeleton::SourceAuditReport =
            serde_json::from_value(proof["original_audit"].clone()).unwrap();
        close(
            &serde_json::to_value(deadlock_skeleton::source_identity::inspect_identity(
                &d, &original,
            ))
            .unwrap(),
            &proof["source_identity"],
            &format!("{label}.source_identity"),
        );
    }
}

#[test]
fn no_cloth_additions_require_fresh_source_binding_and_ordinary_models_keep_behavior() {
    use deadlock_skeleton::source_identity::{preflight, source_contract};
    use sha2::{Digest, Sha256};
    let fixture = json("extension-source.json");
    let mut original: source_skeleton::SourceAuditReport =
        serde_json::from_value(fixture["original"].clone()).unwrap();
    let selected = serde_json::from_value(fixture["selected"].clone()).unwrap();
    let d = deadlock_dmx::Dmx::load(&fixtures().join("extension-source.dmx")).unwrap();
    let expected = source_contract(
        &d,
        &original,
        &selected,
        &format!("{:x}", Sha256::digest(d.bytes())),
    )
    .unwrap()
    .unwrap();
    original.runtime_motion.cloth_nodes = 0;
    original.runtime_motion.controls.clear();
    let proof = preflight(&d, &original, &selected, None).unwrap();
    assert!(!proof.blocked && proof.identity.required && proof.identity.bind_transforms_checked);
    let contract = proof.extension.unwrap();
    assert_eq!(contract.skeleton, expected.skeleton);
    assert_eq!(contract.source_bind_audit, expected.source_bind_audit);
    assert_eq!(contract.required_extra_names, expected.required_extra_names);
    let mut stale = original.clone();
    stale.runtime_motion.resource_sha256 = Some("f".repeat(64));
    assert!(preflight(&d, &stale, &selected, None).unwrap().blocked);
    let mut changed = original.clone();
    changed.runtime_motion.skeleton.as_mut().unwrap().parents[1] = -1;
    assert!(preflight(&d, &changed, &selected, None).unwrap().blocked);
    let skeleton = original.runtime_motion.skeleton.as_ref().unwrap();
    let report:deadlock_skeleton::export::PreparedReport=serde_json::from_value(serde_json::json!({"meshes":[{"name":"plain","positions":[[0.,0.,0.],[1.,0.,0.],[0.,1.,0.]],"normals":[[0.,0.,1.],[0.,0.,1.],[0.,0.,1.]],"uvs":[[[0.,0.],[1.,0.],[0.,1.]]],"colors":[],"weights":[[[skeleton.names[0],1.]],[[skeleton.names[0],1.]],[[skeleton.names[0],1.]]],"faces":[{"vertices":[0,1,2],"corners":[0,1,2],"material":"materials/plain.vmat"}]}]})).unwrap();
    let plain = deadlock_dmx::Dmx::parse(
        deadlock_skeleton::export::export_bytes(&report, skeleton)
            .unwrap()
            .0,
    )
    .unwrap();
    let audit = preflight(
        &plain,
        &original,
        &std::collections::BTreeSet::from(["plain".into()]),
        None,
    )
    .unwrap();
    assert!(!audit.blocked && !audit.identity.required && audit.extension.is_none());
}
