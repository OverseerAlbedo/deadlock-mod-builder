//! Replay the user's native compiler output without requiring Windows compilation.
use deadlock_cloth::generated;
use deadlock_resource::{binary, container, independent, Kind, Value};
use deadlock_skeleton::extensions::SourceBinding;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::PathBuf,
};

fn edit(raw: &[u8], name: &str, mut f: impl FnMut(&mut Value)) -> Vec<u8> {
    let layout = container::layout(raw).unwrap();
    let mut replacements = BTreeMap::new();
    for (i, block) in layout
        .blocks
        .iter()
        .enumerate()
        .filter(|(_, b)| b.name == name)
    {
        let mut d = binary::decode(container::payload(raw, block).unwrap()).unwrap();
        f(&mut d.root);
        replacements.insert(i, binary::encode(&d.root, d.format_guid).unwrap());
    }
    container::rebuild_ordinals(raw, &replacements).unwrap()
}
fn array(v: &mut Value) -> &mut Vec<Value> {
    let Kind::Array(a) = &mut v.kind else {
        panic!("array required")
    };
    a
}
fn inputs() -> (Vec<u8>, Vec<u8>, SourceBinding, BTreeSet<String>) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/secondary");
    let source: SourceBinding =
        serde_json::from_slice(&fs::read(root.join("source.json")).unwrap()).unwrap();
    let selected = source.required_extra_names.iter().cloned().collect();
    (
        fs::read(root.join("original.vmdl_c")).unwrap(),
        fs::read(root.join("native.vmdl_c")).unwrap(),
        source,
        selected,
    )
}
fn drift(native: &[u8]) -> Vec<u8> {
    edit(native, "DATA", |d| {
        let sk = d.get_mut("m_modelSkeleton").unwrap();
        array(sk.get_mut("m_boneScaleParent").unwrap())[4].kind = Kind::F64(0.5004);
        array(&mut array(sk.get_mut("m_bonePosParent").unwrap())[4])[1].kind = Kind::F64(1.998);
    })
}
#[test]
fn reconciles_arbitrary_secondary_names_and_preserves_authored_nonunit_scale() {
    let (original, native, source, selected) = inputs();
    let (baseline, valid) =
        generated::apply(&original, &native, &source, &selected, &selected).unwrap();
    assert!(valid.bind_repair.corrected_bones.is_empty());
    let drifted = drift(&native);
    assert!(generated::qualify(&original, &drifted, &source, &selected, &selected).is_err());
    let (output, audit) =
        generated::apply(&original, &drifted, &source, &selected, &selected).unwrap();
    assert_eq!(audit.bind_repair.corrected_bones, ["custom_skirt_tip"]);
    assert!(audit.bind_repair.source_and_animation_agree);
    assert!(audit.bind_repair.unchanged_mesh_bindings_checked);
    let a = container::layout(&baseline).unwrap();
    let b = container::layout(&output).unwrap();
    for (a, b) in a.blocks.iter().zip(&b.blocks) {
        if a.name == "DATA" {
            assert!(binary::decode(container::payload(&baseline, a).unwrap())
                .unwrap()
                .root
                .canonical_eq(
                    &binary::decode(container::payload(&output, b).unwrap())
                        .unwrap()
                        .root
                ));
        } else {
            assert_eq!(
                container::payload(&baseline, a).unwrap(),
                container::payload(&output, b).unwrap()
            );
        }
    }
    generated::validate(&original, &drifted, &output, &source, &selected, &selected).unwrap();
    // Installation must reconstruct the exact edit; even a tiny further DATA
    // change that would pass the numerical budgets is not permitted.
    let changed = edit(&output, "DATA", |d| {
        array(
            d.get_mut("m_modelSkeleton")
                .unwrap()
                .get_mut("m_boneScaleParent")
                .unwrap(),
        )[4]
        .kind = Kind::F64(0.500001);
    });
    assert!(
        generated::validate(&original, &drifted, &changed, &source, &selected, &selected).is_err()
    );
}
#[test]
fn refuses_conflicting_source_animation_mesh_hierarchy_and_body_evidence() {
    let (original, native, source, selected) = inputs();
    let drifted = drift(&native);
    for case in 0..7 {
        let bad = match case {
            0 => edit(&drifted, "AGRP", |d| {
                array(
                    d.get_mut("m_decodeKey")
                        .unwrap()
                        .get_mut("m_boneArray")
                        .unwrap(),
                )[4]
                .get_mut("m_scale")
                .unwrap()
                .kind = Kind::F64(0.5004);
            }),
            1 => edit(&drifted, "MDAT", |d| {
                for bone in array(d.get_mut("m_skeleton").unwrap().get_mut("m_bones").unwrap()) {
                    if bone.get("m_boneName").unwrap().string().unwrap() == "custom_skirt_tip" {
                        array(bone.get_mut("m_invBindPose").unwrap())[3].kind = Kind::F64(5.);
                    }
                }
            }),
            2 => edit(&drifted, "DATA", |d| {
                array(
                    d.get_mut("m_modelSkeleton")
                        .unwrap()
                        .get_mut("m_nParent")
                        .unwrap(),
                )[4]
                .kind = Kind::I64(0);
            }),
            3 => edit(&drifted, "DATA", |d| {
                array(
                    d.get_mut("m_modelSkeleton")
                        .unwrap()
                        .get_mut("m_boneScaleParent")
                        .unwrap(),
                )[0]
                .kind = Kind::F64(1.001);
            }),
            4 => edit(&drifted, "AGRP", |d| {
                array(
                    d.get_mut("m_decodeKey")
                        .unwrap()
                        .get_mut("m_boneArray")
                        .unwrap(),
                )
                .pop();
            }),
            5 => edit(&drifted, "CTRL", |d| {
                d.get_mut("embedded_animation")
                    .unwrap()
                    .get_mut("group_data_block")
                    .unwrap()
                    .kind = Kind::I64(0);
            }),
            _ => edit(&drifted, "PHYS", |d| {
                let fe = d.get_mut("m_pFeModel").unwrap();
                let i = fe
                    .get("m_CtrlName")
                    .unwrap()
                    .array()
                    .unwrap()
                    .iter()
                    .position(|n| n.string().unwrap() == "custom_skirt_tip")
                    .unwrap();
                array(&mut array(fe.get_mut("m_InitPose").unwrap())[i])[0].kind = Kind::F64(100.);
            }),
        };
        assert!(
            generated::apply(&original, &bad, &source, &selected, &selected).is_err(),
            "{case}"
        );
    }
    for case in 0..2 {
        let mut stale = source.clone();
        if case == 0 {
            stale.original_sha256 = "0".repeat(64);
        } else {
            stale.skeleton.scales[4] = 0.6;
        }
        assert!(generated::apply(&original, &drifted, &stale, &selected, &selected).is_err());
    }
}

#[test]
#[ignore = "requires DMB_BIND_INPUT collected on the failing Windows build"]
fn collected_native_bind_replays_complete_secondary_validation() {
    let root = PathBuf::from(std::env::var("DMB_BIND_INPUT").unwrap());
    let original = fs::read(root.join("input/original.vmdl_c")).unwrap();
    let native_path = fs::read_dir(root.join("native"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let native = fs::read(native_path).unwrap();
    let identity: serde_json::Value = serde_json::from_slice(
        &fs::read(root.join("workspace/source-skeleton-identity.json")).unwrap(),
    )
    .unwrap();
    let source: SourceBinding = serde_json::from_value(identity["extension"].clone()).unwrap();
    let plan: serde_json::Value = serde_json::from_slice(
        &fs::read(root.join("workspace/secondary-motion-source.json")).unwrap(),
    )
    .unwrap();
    let names = |key: &str| -> BTreeSet<String> {
        plan["groups"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|g| {
                g[key]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|n| n.as_str().unwrap().to_owned())
            })
            .collect()
    };
    let (selected, weighted) = (names("bones"), names("weighted_bones"));
    let chains: Vec<generated::roll_lock::Chain> =
        serde_json::from_value(plan["roll_lock_chains"].clone()).unwrap();
    let before = generated::qualify(&original, &native, &source, &selected, &weighted).unwrap_err();
    println!("Raw compiler output reproduces: {before}");
    assert!(before
        .to_string()
        .contains("Compiled skeleton bind transform changed"));
    let (output, audit) =
        generated::apply_with_roll_lock(&original, &native, &source, &selected, &weighted, &chains)
            .unwrap();
    generated::validate_with_roll_lock(
        &original, &native, &output, &source, &selected, &weighted, &chains,
    )
    .unwrap();
    assert!(audit.original_rigid_body_physics_preserved);
    assert!(audit.compiled_bindings.passed);
    assert_eq!(audit.fitted_bones.len(), selected.len());
    assert!(!audit.bind_repair.corrected_bones.is_empty());
    if let Ok(folder) = std::env::var("DMB_BIND_OUTPUT") {
        let folder = PathBuf::from(folder);
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("repaired.vmdl_c"), &output).unwrap();
        fs::write(
            folder.join("audit.json"),
            serde_json::to_vec_pretty(&audit).unwrap(),
        )
        .unwrap();
        if let Ok(reader) = std::env::var("DMB_BIND_READER") {
            for block in ["DATA", "PHYS"] {
                let result = std::process::Command::new(&reader)
                    .arg("-i")
                    .arg(folder.join("repaired.vmdl_c"))
                    .args(["-b", block])
                    .output()
                    .unwrap();
                assert!(
                    result.status.success(),
                    "{}",
                    String::from_utf8_lossy(&result.stderr)
                );
                let docs =
                    independent::parse_cli_blocks(&String::from_utf8(result.stdout).unwrap())
                        .unwrap();
                let expected = container::read_blocks(&output, block).unwrap();
                assert_eq!(docs.len(), 1);
                let proof = independent::verify(&docs[0].root, &expected[0].root).unwrap();
                println!(
                    "Independent native reader {block}: {} values verified.",
                    proof.values_checked
                );
            }
        }
    }
    println!(
        "All {} weighted secondary bones passed source, mesh, animation and solver checks.",
        weighted.len()
    );
}
