//! Replay the supplied Graves compiler capture; no new native compile or game run.
use deadlock_cloth::generated;
use deadlock_resource::{binary, container, independent, Kind, Value};
use deadlock_skeleton::extensions::SourceBinding;
use std::{collections::{BTreeMap, BTreeSet}, fs, path::PathBuf};

fn plain(value: &Value) -> serde_json::Value {
    use serde_json::{json, Value as Json};
    match &value.kind {
        Kind::Null => Json::Null,
        Kind::Bool(v) => json!(v),
        Kind::I64(v) => json!(v),
        Kind::U64(v) => json!(v),
        Kind::F32(v) => json!(*v as f64),
        Kind::F64(v) => json!(v),
        Kind::String(v) => json!(v),
        Kind::Blob(v) => json!({"bytes":v.len()}),
        Kind::Array(v) => v.iter().map(plain).collect(),
        Kind::Object(v) => v.iter().map(|(k,v)|(k.clone(),plain(v))).collect(),
    }
}

#[test]
#[ignore = "requires DMB_GRAVES_INPUT from the supplied failed Windows build"]
fn captured_graves_rest_transforms_pass_complete_secondary_validation() {
    let root = PathBuf::from(std::env::var("DMB_GRAVES_INPUT").unwrap());
    let original = fs::read(root.join("input/original.vmdl_c")).unwrap();
    let native = fs::read(root.join("compiler/model.vmdl_c")).unwrap();
    let identity: serde_json::Value = serde_json::from_slice(
        &fs::read(root.join("workspace/source-skeleton-identity.json")).unwrap(),
    ).unwrap();
    let source: SourceBinding = serde_json::from_value(identity["extension"].clone()).unwrap();
    let plan: serde_json::Value = serde_json::from_slice(
        &fs::read(root.join("workspace/secondary-motion-source.json")).unwrap(),
    ).unwrap();
    let names = |key: &str| -> BTreeSet<String> {
        plan["groups"].as_array().unwrap().iter().flat_map(|g| {
            g[key].as_array().unwrap().iter().map(|n| n.as_str().unwrap().to_owned())
        }).collect()
    };
    let (selected, weighted) = (names("bones"), names("weighted_bones"));
    let chains: Vec<generated::roll_lock::Chain> = serde_json::from_value(plan["roll_lock_chains"].clone()).unwrap();
    if let Ok(folder) = std::env::var("DMB_GRAVES_OUTPUT") {
        let folder = PathBuf::from(folder);
        fs::create_dir_all(&folder).unwrap();
        for block in ["DATA", "AGRP", "PHYS", "MDAT"] {
            let blocks = container::read_blocks(&native, block).unwrap();
            let values: Vec<_> = blocks.iter().map(|b| plain(&b.root)).collect();
            fs::write(folder.join(format!("native-{block}.json")), serde_json::to_vec(&values).unwrap()).unwrap();
        }
    }
    let qualified = generated::qualify(&original, &native, &source, &selected, &weighted).unwrap();
    let data = container::read_blocks(&native, "DATA").unwrap();
    let actual = deadlock_skeleton::Skeleton::from_value(data[0].root.get("m_modelSkeleton").unwrap()).unwrap();
    let original_data = container::read_blocks(&original, "DATA").unwrap();
    let original_sk = deadlock_skeleton::Skeleton::from_value(original_data[0].root.get("m_modelSkeleton").unwrap()).unwrap();
    let mut raw_source = source.clone();
    for name in raw_source.skeleton.names.iter_mut().chain(&mut raw_source.extra_names).chain(&mut raw_source.required_extra_names) {
        if let Some(alias) = qualified.compiler_aliases.get(name) { *name = alias.clone(); }
    }
    let before = deadlock_skeleton::extensions::source_binding(&actual, &original_sk, &raw_source, &source.original_sha256).unwrap_err();
    assert_eq!(before.to_string(), "Bind transform outside original-physics transfer budget: dmb_secondary_maozi_a1_M");
    println!("Unconverted source comparison reproduces: {before}");
    let (output, audit) = generated::apply_with_roll_lock(
        &original, &native, &source, &selected, &weighted, &chains,
    ).unwrap();
    generated::validate_with_roll_lock(
        &original, &native, &output, &source, &selected, &weighted, &chains,
    ).unwrap();
    assert!(audit.original_rigid_body_physics_preserved);
    assert!(audit.compiled_bindings.passed);
    assert_eq!(audit.fitted_bones.len(), 220);
    for name in &weighted {
        assert!(audit.fitted_bones.contains(audit.compiler_aliases.get(name).unwrap_or(name)));
    }
    assert_eq!(audit.source_rotation_import.bones.len(), 1);
    assert_eq!(audit.source_rotation_import.bones[0].name, "dmb_secondary_maozi_a1_M");
    assert!(audit.source_rotation_import.bones[0].native_component_residual < 4e-7);
    assert_eq!(audit.source_rotation_import.solver_rest_bones_checked, 1);
    assert!(!audit.source_rotation_import.native_values_modified);
    assert!(audit.bind_repair.corrected_bones.is_empty());
    let before_blocks = container::layout(&native).unwrap();
    let after_blocks = container::layout(&output).unwrap();
    for (a,b) in before_blocks.blocks.iter().zip(&after_blocks.blocks) {
        assert_eq!(a.name, b.name);
        if a.name != "PHYS" {
            assert_eq!(container::payload(&native,a).unwrap(), container::payload(&output,b).unwrap());
        }
    }
    if let Ok(folder) = std::env::var("DMB_GRAVES_OUTPUT") {
        let folder = PathBuf::from(folder);
        fs::write(folder.join("repaired.vmdl_c"), &output).unwrap();
        fs::write(folder.join("audit.json"), serde_json::to_vec_pretty(&audit).unwrap()).unwrap();
        if let Ok(reader) = std::env::var("DMB_GRAVES_READER") {
            for block in ["DATA", "PHYS"] {
                let result = std::process::Command::new(&reader)
                    .arg("-i").arg(folder.join("repaired.vmdl_c")).args(["-b",block])
                    .output().unwrap();
                assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
                let docs = independent::parse_cli_blocks(&String::from_utf8(result.stdout).unwrap()).unwrap();
                let expected = container::read_blocks(&output,block).unwrap();
                assert_eq!(docs.len(),expected.len());
                let count: usize = docs.iter().zip(&expected).map(|(a,b)| independent::verify(&a.root,&b.root).unwrap().values_checked).sum();
                println!("Independent native reader {block}: {count} values verified.");
            }
        }
    }
    println!("Graves passed: {} secondary fits, {} selected bones ({} weighted).", audit.fitted_bones.len(), selected.len(), weighted.len());
}

fn edit(raw: &[u8], name: &str, mut change: impl FnMut(&mut Value)) -> Vec<u8> {
    let layout = container::layout(raw).unwrap();
    let mut replacements = BTreeMap::new();
    for (i,block) in layout.blocks.iter().enumerate().filter(|(_,b)| b.name == name) {
        let mut decoded = binary::decode(container::payload(raw,block).unwrap()).unwrap();
        change(&mut decoded.root);
        replacements.insert(i,binary::encode(&decoded.root,decoded.format_guid).unwrap());
    }
    container::rebuild_ordinals(raw,&replacements).unwrap()
}
fn array(v: &mut Value) -> &mut Vec<Value> {
    let Kind::Array(a) = &mut v.kind else { panic!("array required"); }; a
}

#[test]
#[ignore = "requires DMB_GRAVES_INPUT from the supplied failed Windows build"]
fn captured_graves_rejects_conflicting_rotation_authorities() {
    let root = PathBuf::from(std::env::var("DMB_GRAVES_INPUT").unwrap());
    let original = fs::read(root.join("input/original.vmdl_c")).unwrap();
    let native = fs::read(root.join("compiler/model.vmdl_c")).unwrap();
    let identity: serde_json::Value = serde_json::from_slice(&fs::read(root.join("workspace/source-skeleton-identity.json")).unwrap()).unwrap();
    let source: SourceBinding = serde_json::from_value(identity["extension"].clone()).unwrap();
    let plan: serde_json::Value = serde_json::from_slice(&fs::read(root.join("workspace/secondary-motion-source.json")).unwrap()).unwrap();
    let names = |key: &str| -> BTreeSet<String> {
        plan["groups"].as_array().unwrap().iter().flat_map(|g| g[key].as_array().unwrap().iter().map(|n|n.as_str().unwrap().to_owned())).collect()
    };
    let (selected,weighted) = (names("bones"),names("weighted_bones"));
    let bone = "dmb_secondary_maozi_a1_M";
    for case in 0..5 {
        let mut bad = match case {
            0 | 4 => edit(&native,"DATA",|d| {
                let sk = d.get_mut("m_modelSkeleton").unwrap();
                let at = if case == 4 { 0 } else {
                    sk.get("m_boneName").unwrap().array().unwrap().iter().position(|n|n.string().unwrap()==bone).unwrap()
                };
                array(&mut array(sk.get_mut("m_boneRotParent").unwrap())[at])[0].kind = Kind::F64(if case == 4 {0.52} else {0.5180450345993042});
            }),
            1 => edit(&native,"AGRP",|d| {
                let rows = array(d.get_mut("m_decodeKey").unwrap().get_mut("m_boneArray").unwrap());
                let row = rows.iter_mut().find(|r|r.get("m_name").unwrap().string().unwrap()==bone).unwrap();
                array(row.get_mut("m_quat").unwrap())[0].kind = Kind::F64(0.5180449749946594);
            }),
            2 => edit(&native,"MDAT",|d| {
                for row in array(d.get_mut("m_skeleton").unwrap().get_mut("m_bones").unwrap()) {
                    if row.get("m_boneName").unwrap().string().unwrap() == bone {
                        array(row.get_mut("m_invBindPose").unwrap())[0].kind = Kind::F64(0.01);
                    }
                }
            }),
            _ => edit(&native,"PHYS",|d| {
                let fe = d.get_mut("m_pFeModel").unwrap();
                let at = fe.get("m_CtrlName").unwrap().array().unwrap().iter().position(|n|n.string().unwrap()==bone).unwrap();
                array(&mut array(fe.get_mut("m_InitPose").unwrap())[at])[0].kind = Kind::F64(100.);
            }),
        };
        if case == 0 {
            bad = edit(&bad,"AGRP",|d| {
                let rows = array(d.get_mut("m_decodeKey").unwrap().get_mut("m_boneArray").unwrap());
                let row = rows.iter_mut().find(|r|r.get("m_name").unwrap().string().unwrap()==bone).unwrap();
                array(row.get_mut("m_quat").unwrap())[0].kind = Kind::F64(0.5180450345993042);
            });
        }
        assert!(generated::apply(&original,&bad,&source,&selected,&weighted).is_err(), "Corruption case {case}");
    }
    let mut unrelated = source.clone();
    let at = unrelated.skeleton.names.iter().position(|n|n==bone).unwrap();
    unrelated.skeleton.rotations[at] = [0.,0.,0.,1.];
    assert!(generated::apply(&original,&native,&unrelated,&selected,&weighted).is_err());
    // Both supported compiler effects may occur in one build: the rotation
    // import plus DATA drift. AGRP and unchanged mesh/solver records must still
    // prove the exact reconciliation, without accepting the drift directly.
    let drifted = edit(&native,"DATA",|d| {
        let sk = d.get_mut("m_modelSkeleton").unwrap();
        let at = sk.get("m_boneName").unwrap().array().unwrap().iter().position(|n|n.string().unwrap()==bone).unwrap();
        array(&mut array(sk.get_mut("m_bonePosParent").unwrap())[at])[0].kind = Kind::F64(3.34);
    });
    assert!(generated::qualify(&original,&drifted,&source,&selected,&weighted).is_err());
    let (reconciled,proof) = generated::apply(&original,&drifted,&source,&selected,&weighted).unwrap();
    assert!(proof.bind_repair.corrected_bones.iter().any(|n|n==bone));
    generated::validate(&original,&drifted,&reconciled,&source,&selected,&weighted).unwrap();
    println!("Six conflicting DATA, animation, mesh, solver, original-bone and source cases rejected.");
    println!("Combined rotation import and source-qualified DATA reconciliation passed.");
}
