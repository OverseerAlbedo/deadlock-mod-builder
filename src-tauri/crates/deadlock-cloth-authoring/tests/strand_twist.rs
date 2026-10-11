//! Actual imported-model replay. Never substitutes for an in-game motion test.
use deadlock_cloth::generated;
use deadlock_cloth_authoring::{parts::Kind, secondary};
use deadlock_core::CancellationToken;
use deadlock_dmx::Dmx;
use deadlock_project::Project;
use deadlock_resource::{container, independent};
use deadlock_skeleton::extensions::SourceBinding;
use std::{collections::BTreeSet, fs, path::PathBuf};

#[test]
#[ignore = "requires DMB_STRAND_INPUT collected on Windows"]
fn captured_native_roll_protection_preserves_all_other_compiled_motion() {
    let root = PathBuf::from(std::env::var("DMB_STRAND_INPUT").unwrap());
    let p: Project = serde_json::from_slice(&fs::read(root.join("project.json")).unwrap()).unwrap();
    let d = Dmx::parse(fs::read(root.join("input/prepared.dmx")).unwrap()).unwrap();
    let identity: serde_json::Value = serde_json::from_slice(
        &fs::read(root.join("workspace/source-skeleton-identity.json")).unwrap(),
    )
    .unwrap();
    let source: SourceBinding = serde_json::from_value(identity["extension"].clone()).unwrap();
    let prior: serde_json::Value = serde_json::from_slice(
        &fs::read(root.join("workspace/secondary-motion-source.json")).unwrap(),
    )
    .unwrap();
    let setup = secondary::setup(&p, &d, Some(&source), &CancellationToken::new())
        .unwrap()
        .unwrap();
    // This replays orientation fitting on the captured compiler output. The
    // shared hair-flow update changes source goals and requires a new native
    // compile; this test must never claim to validate that new compiled tuning.
    assert_eq!(setup.source_sha256, prior["source_sha256"].as_str().unwrap());
    let old_parts = prior["parts"].as_array().unwrap();
    assert_eq!(setup.parts.len(), old_parts.len());
    for (part, old) in setup.parts.iter().zip(old_parts) {
        assert_eq!(serde_json::to_value(part.kind).unwrap(), old["kind"]);
        if !matches!(part.kind, Kind::Hair | Kind::Tail) {
            assert_eq!(serde_json::to_value(part).unwrap(), *old);
        }
    }
    assert_eq!(setup.roll_lock_chains.len(), 14);
    assert!(setup.roll_lock_chains.iter().all(|c| c.per_joint_reference));
    let original = fs::read(root.join("input/original.vmdl_c")).unwrap();
    let native_path = fs::read_dir(root.join("native"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let native = fs::read(native_path).unwrap();
    let selected = setup
        .groups
        .iter()
        .flat_map(|g| g.bones.iter().cloned())
        .collect();
    let weighted = setup
        .groups
        .iter()
        .flat_map(|g| g.weighted_bones.iter().cloned())
        .collect();
    let (baseline, _) =
        generated::apply(&original, &native, &source, &selected, &weighted).unwrap();
    let (output, audit) = generated::apply_with_roll_lock(
        &original,
        &native,
        &source,
        &selected,
        &weighted,
        &setup.roll_lock_chains,
    )
    .unwrap();
    generated::validate_with_roll_lock(
        &original,
        &native,
        &output,
        &source,
        &selected,
        &weighted,
        &setup.roll_lock_chains,
    )
    .unwrap();
    assert!(audit.original_rigid_body_physics_preserved);
    assert!(audit.compiled_bindings.passed);
    assert_eq!(audit.roll_lock.chains.len(), 14);
    assert!(audit
        .roll_lock
        .chains
        .iter()
        .all(|p| p.minimum_pole_angle_degrees >= 60.));
    let before = container::read_blocks(&baseline, "PHYS")
        .unwrap()
        .remove(0)
        .root;
    let after = container::read_blocks(&output, "PHYS")
        .unwrap()
        .remove(0)
        .root;
    for (key, value) in before.object().unwrap() {
        if key != "m_pFeModel" {
            assert!(value.canonical_eq(after.get(key).unwrap()), "{key}");
        }
    }
    let a = before.get("m_pFeModel").unwrap();
    let b = after.get("m_pFeModel").unwrap();
    for (key, value) in a.object().unwrap() {
        if key != "m_NodeBases" {
            assert!(value.canonical_eq(b.get(key).unwrap()), "{key}");
        }
    }
    let allowed = setup
        .roll_lock_chains
        .iter()
        .flat_map(|c| c.bones.iter().cloned())
        .collect::<BTreeSet<_>>();
    let names = a.get("m_CtrlName").unwrap().array().unwrap();
    let mut changed = 0;
    for (a, b) in a
        .get("m_NodeBases")
        .unwrap()
        .array()
        .unwrap()
        .iter()
        .zip(b.get("m_NodeBases").unwrap().array().unwrap())
    {
        let node = a.get("nNode").unwrap().integer().unwrap() as usize;
        if allowed.contains(names[node].string().unwrap()) {
            assert!(!a.canonical_eq(b));
            changed += 1;
        } else {
            assert!(a.canonical_eq(b));
        }
    }
    assert_eq!(changed, 44);
    for (a, b) in container::layout(&baseline)
        .unwrap()
        .blocks
        .iter()
        .zip(container::layout(&output).unwrap().blocks)
    {
        assert_eq!(a.name, b.name);
        if a.name != "PHYS" {
            assert_eq!(
                container::payload(&baseline, a).unwrap(),
                container::payload(&output, &b).unwrap()
            );
        }
    }
    let folder = PathBuf::from(std::env::var("DMB_STRAND_OUTPUT").unwrap());
    fs::create_dir_all(&folder).unwrap();
    let path = folder.join("strand-protected.vmdl_c");
    fs::write(&path, &output).unwrap();
    fs::write(
        folder.join("setup.json"),
        serde_json::to_vec_pretty(&setup).unwrap(),
    )
    .unwrap();
    fs::write(
        folder.join("audit.json"),
        serde_json::to_vec_pretty(&audit).unwrap(),
    )
    .unwrap();
    fs::write(
        folder.join("preservation.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "hair_tail_groups":14,"changed_orientation_rows":changed,"captured_native_orientation_replay_only":true,
            "all_other_fe_and_body_values_unchanged":true,"all_nonphys_blocks_match_alpha30":true,
            "game_tested":false,"native_compiler_reexecuted":false
        }))
        .unwrap(),
    )
    .unwrap();
    if let Ok(reader) = std::env::var("DMB_STRAND_READER") {
        for block in ["DATA", "PHYS"] {
            let result = std::process::Command::new(&reader)
                .arg("-i")
                .arg(&path)
                .args(["-b", block])
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            let docs =
                independent::parse_cli_blocks(&String::from_utf8(result.stdout).unwrap()).unwrap();
            let expected = container::read_blocks(&output, block).unwrap();
            let proof = independent::verify(&docs[0].root, &expected[0].root).unwrap();
            println!(
                "Independent reader {block}: {} values verified.",
                proof.values_checked
            );
        }
    }
    println!("44 hair/tail orientations corrected on captured native output; all other compiled data unchanged. New source hair goals require recompilation.");
}
