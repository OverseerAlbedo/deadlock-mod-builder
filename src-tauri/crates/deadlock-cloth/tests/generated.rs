//! Synthetic native-structure tests, not native compiler or game acceptance.
use deadlock_cloth::generated;
use deadlock_resource::{binary, container, Kind, Value};
use deadlock_skeleton::extensions::SourceBinding;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};
fn raw(n: &str) -> Vec<u8> {
    fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/secondary")
            .join(n),
    )
    .unwrap()
}
fn inputs() -> (Vec<u8>, Vec<u8>, SourceBinding, BTreeSet<String>) {
    (
        raw("original.vmdl_c"),
        raw("native.vmdl_c"),
        serde_json::from_slice(&raw("source.json")).unwrap(),
        serde_json::from_slice(&raw("groups.json")).unwrap(),
    )
}
fn edit(raw: &[u8], name: &str, f: impl FnOnce(&mut Value)) -> Vec<u8> {
    let layout = container::layout(raw).unwrap();
    let b = container::unique(&layout.blocks, name).unwrap();
    let mut d = binary::decode(container::payload(raw, b).unwrap()).unwrap();
    f(&mut d.root);
    container::rebuild(
        raw,
        &BTreeMap::from([(name.into(), binary::encode(&d.root, d.format_guid).unwrap())]),
    )
    .unwrap()
}
#[test]
fn generated_solver_keeps_native_fe_meshes_and_original_body() {
    let (original, native, source, selected) = inputs();
    let (output, audit) =
        generated::apply(&original, &native, &source, &selected, &selected).unwrap();
    assert!(audit.original_rigid_body_physics_preserved);
    assert!(!audit.original_cloth_solver_preserved);
    assert!(!audit.in_game_motion_tested);
    assert_eq!(audit.fitted_bones.len(), 2);
    generated::validate(&original, &native, &output, &source, &selected, &selected).unwrap();
    assert!(deadlock_cloth::restore(&original, &output, Some(&source)).is_err());
}
#[test]
fn missing_fits_body_drivers_degenerate_supports_and_unknown_bones_fail() {
    let (original, native, source, selected) = inputs();
    for case in 0..4 {
        let bad = edit(&native, "PHYS", |v| {
            let fe = v.get_mut("m_pFeModel").unwrap();
            match case {
                0 => {
                    fe.get_mut("m_FitMatrices").unwrap().kind = Kind::Array(vec![]);
                }
                1 => {
                    if let Kind::Array(a) = &mut fe.get_mut("m_FitMatrices").unwrap().kind {
                        a[0].get_mut("nNode").unwrap().kind = Kind::I64(0);
                    }
                }
                2 => {
                    if let Kind::Array(a) = &mut fe.get_mut("m_InitPose").unwrap().kind {
                        for i in 5..8 {
                            a[i] = a[5].clone();
                        }
                    }
                }
                _ => {
                    if let Kind::Array(a) = &mut fe.get_mut("m_CtrlName").unwrap().kind {
                        a[3].kind = Kind::String("missing".into());
                    }
                }
            }
        });
        assert!(
            generated::qualify(&original, &bad, &source, &selected, &selected).is_err(),
            "{case}"
        );
    }
}
#[test]
fn stale_authority_body_changes_and_native_solver_changes_fail() {
    let (original, native, source, selected) = inputs();
    let (output, _) = generated::apply(&original, &native, &source, &selected, &selected).unwrap();
    let mut stale = source.clone();
    stale.original_sha256 = "0".repeat(64);
    assert!(generated::qualify(&original, &native, &stale, &selected, &selected).is_err());
    for key in ["m_bodySentinel", "m_pFeModel"] {
        let bad = edit(&output, "PHYS", |v| {
            v.get_mut(key).unwrap().kind = Kind::Null;
        });
        assert!(
            generated::validate(&original, &native, &bad, &source, &selected, &selected).is_err()
        );
    }
    let mut unselected = selected.clone();
    unselected.pop_last();
    assert!(generated::qualify(&original, &native, &source, &unselected, &unselected).is_err());
}

#[test]
fn bounded_preset_requires_native_limit_records_scalar_or_simd() {
    let native = raw("native.vmdl_c");
    assert!(generated::require_animation_limit_records(&native).is_err());
    for name in ["m_AnimStrayRadii", "m_SimdAnimStrayRadii"] {
        for (kind, accepted) in [
            (Kind::Array(vec![]), false),
            (Kind::Null, false),
            (Kind::Array(vec![Value::new(Kind::F64(1.))]), false),
            // Synthetic presence only; complete solver qualification is separate.
            (Kind::Array(vec![Value::new(Kind::Object(vec![]))]), true),
        ] {
            let changed = edit(&native, "PHYS", |v| {
                let fe = v.get_mut("m_pFeModel").unwrap();
                if let Some(value) = fe.get_mut(name) {
                    value.kind = kind;
                } else if let Kind::Object(fields) = &mut fe.kind {
                    fields.push((name.into(), Value::new(kind)));
                }
            });
            assert_eq!(generated::require_animation_limit_records(&changed).is_ok(), accepted, "{name}");
        }
    }
}

#[test]
#[ignore = "requires the supplied compiled TangTang, accepted source binding and an explicit original authority"]
fn supplied_tangtang_compact_solver_passes_complete_generated_checks() {
    let original = fs::read(std::env::var("DMB_COMPACT_ORIGINAL").unwrap()).unwrap();
    let native = fs::read(std::env::var("DMB_COMPACT_NATIVE").unwrap()).unwrap();
    let source: SourceBinding =
        serde_json::from_slice(&fs::read(std::env::var("DMB_COMPACT_SOURCE").unwrap()).unwrap())
            .unwrap();
    let selected = source.extra_names.iter().cloned().collect();
    let weighted = source.required_extra_names.iter().cloned().collect();
    let (output, audit) =
        generated::apply(&original, &native, &source, &selected, &weighted).unwrap();
    assert_eq!(audit.fitting.compact_fitted_bones.len(), 262);
    assert_eq!(audit.fitted_bones.len(), 262);
    assert!(audit.original_rigid_body_physics_preserved);
    generated::validate(&original, &native, &output, &source, &selected, &weighted).unwrap();
    if let Ok(path) = std::env::var("DMB_COMPACT_AUDIT") {
        fs::write(path, serde_json::to_vec_pretty(&audit).unwrap()).unwrap();
    }
    if let Ok(path) = std::env::var("DMB_COMPACT_OUTPUT") {
        fs::write(&path, &output).unwrap();
        if let Ok(reader) = std::env::var("DMB_COMPACT_READER") {
            let layout = container::layout(&output).unwrap();
            for block in ["DATA", "PHYS"] {
                let result = std::process::Command::new(&reader)
                    .args(["-i", &path, "-b", block])
                    .output()
                    .unwrap();
                assert!(
                    result.status.success(),
                    "{}",
                    String::from_utf8_lossy(&result.stderr)
                );
                let expected = binary::decode(
                    container::payload(&output, container::unique(&layout.blocks, block).unwrap())
                        .unwrap(),
                )
                .unwrap();
                let docs = deadlock_resource::independent::parse_cli_blocks(
                    &String::from_utf8(result.stdout).unwrap(),
                )
                .unwrap();
                assert_eq!(docs.len(), 1);
                let numeric =
                    deadlock_resource::independent::verify(&docs[0].root, &expected.root).unwrap();
                println!(
                    "Independent native reader {block}: {} values verified.",
                    numeric.values_checked
                );
            }
        }
    }
    println!("Supplied native compact model: 262 fits; full generated apply/validate passed. Original authority is the explicitly supplied test input; this is not game acceptance.");
}
