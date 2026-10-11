//! Actual Windows compiler capture; no new compilation or in-game claim.
use deadlock_cloth::generated;
use deadlock_resource::{container, independent};
use deadlock_skeleton::extensions::SourceBinding;
use std::{collections::BTreeSet, fs, path::PathBuf};

#[test]
#[ignore = "requires DMB_WRAITH_INPUT from the supplied failed Windows build"]
fn captured_wraith_restores_all_original_bones_and_separates_proxy_names() {
    let root = PathBuf::from(std::env::var("DMB_WRAITH_INPUT").unwrap());
    let original = fs::read(root.join("input/original.vmdl_c")).unwrap();
    // The collector also contains an earlier Haze build. Replay the current
    // compiler output, whose bytes match Wraith's newest immutable snapshot.
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
    let before = generated::qualify(&original, &native, &source, &selected, &weighted).unwrap_err();
    assert!(before.to_string().contains("Extended skeleton missing original identities"));
    println!("Raw Wraith compiler error: {before}");
    let (output, audit) = generated::apply_with_roll_lock(
        &original, &native, &source, &selected, &weighted, &chains,
    ).unwrap();
    generated::validate_with_roll_lock(
        &original, &native, &output, &source, &selected, &weighted, &chains,
    ).unwrap();
    assert_eq!(audit.original_identity.original_aliases.len(), 258);
    assert_eq!(audit.original_identity.control_renames.len(), 185);
    assert_eq!(audit.original_identity.original_bones_checked, 422);
    assert!(audit.original_identity.control_renames.iter().all(|r| r.derived_proxy));
    assert!(audit.original_identity.solver_values_unchanged_except_control_names_and_hashes);
    assert!(audit.original_identity.geometry_and_animation_samples_byte_preserved);
    assert!(audit.original_rigid_body_physics_preserved);
    assert!(audit.compiled_bindings.passed);
    assert_eq!(audit.fitted_bones.len(), selected.len());
    for name in &weighted {
        assert!(audit.fitted_bones.contains(audit.compiler_aliases.get(name).unwrap_or(name)));
    }
    let data = container::read_blocks(&output, "DATA").unwrap();
    let skeleton = deadlock_skeleton::Skeleton::from_value(data[0].root.get("m_modelSkeleton").unwrap()).unwrap();
    for name in ["cards", "wraith_crimelord_card_01_jnt", "wraith_crimelord_card_06_jnt", "magazine"] {
        assert!(skeleton.names.contains(&name.to_owned()), "{name}");
    }
    assert_eq!(container::read_blocks(&output, "MDAT").unwrap().len(), 9);
    if let Ok(folder) = std::env::var("DMB_WRAITH_OUTPUT") {
        let folder = PathBuf::from(folder);
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("repaired.vmdl_c"), &output).unwrap();
        fs::write(folder.join("audit.json"), serde_json::to_vec_pretty(&audit).unwrap()).unwrap();
        if let Ok(reader) = std::env::var("DMB_WRAITH_READER") {
            for block in ["DATA", "PHYS"] {
                let result = std::process::Command::new(&reader)
                    .arg("-i").arg(folder.join("repaired.vmdl_c")).args(["-b", block])
                    .output().unwrap();
                assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
                let docs = independent::parse_cli_blocks(&String::from_utf8(result.stdout).unwrap()).unwrap();
                let expected = container::read_blocks(&output, block).unwrap();
                assert_eq!(docs.len(), expected.len());
                let mut count = 0;
                for (actual, expected) in docs.iter().zip(&expected) {
                    count += independent::verify(&actual.root, &expected.root).unwrap().values_checked;
                }
                println!("Independent native reader {block}: {count} values verified.");
            }
        }
    }
    println!("Wraith passed: {} original identities restored, {} proxy collisions resolved, {} secondary fits checked ({} weighted).",
        audit.original_identity.original_aliases.len(), audit.original_identity.control_renames.len(), selected.len(), weighted.len());
}
