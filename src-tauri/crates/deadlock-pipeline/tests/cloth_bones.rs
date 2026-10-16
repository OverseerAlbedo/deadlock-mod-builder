//! A hero whose October 2026 decompile no longer lists the compiler's cloth
//! bones (Yamato: 387 `$cloth_…` bones, a Softbody and a cloth proxy mesh) gets
//! them listed again and its cloth definitions taken out, so the compile
//! template check passes as it did with older decompiles. Run with --ignored;
//! DMB_PAK is the game archive, DMB_HERO_SOURCE the decompiled .vmdl.
#[test]
#[ignore = "needs the Deadlock game archive and a decompiled hero source"]
fn a_new_decompile_lists_the_compiled_cloth_bones_for_the_template_check() {
    let (Ok(pak), Ok(source)) = (std::env::var("DMB_PAK"), std::env::var("DMB_HERO_SOURCE")) else { return };
    let source = std::path::PathBuf::from(source);
    let model = source.strip_prefix(source.ancestors().find(|a| a.join("models").is_dir()).unwrap()).unwrap().to_string_lossy().replace('\\', "/");
    let vpk = deadlock_vpk::Vpk::open(&pak).unwrap();
    let raw = vpk.read(&format!("{model}_c")).unwrap();
    let data = deadlock_resource::container::read_blocks(&raw, "DATA").unwrap();
    let compiled = deadlock_skeleton::Skeleton::from_value(data[0].root.get("m_modelSkeleton").unwrap()).unwrap();
    let text = std::fs::read_to_string(&source).unwrap();
    let (listed, added) = deadlock_model::modeldoc::with_compiled_cloth_bones(&text, &compiled).unwrap();
    let (listed, aligned) = deadlock_model::modeldoc::with_compiled_bone_transforms(&listed, &compiled).unwrap();
    let (stripped, removed) = deadlock_model::modeldoc::without_decompiled_cloth(&listed).unwrap();
    println!("{model}: {added} cloth bones listed, {aligned} bone transforms taken from the game's model, {removed} cloth definitions left out");
    // Heroes whose compiled skeleton carries no $cloth bones (Violet) have nothing to list.
    let compiler_cloth = compiled.names.iter().any(|n| n.starts_with("$cloth"));
    assert!(added > 0 || !compiler_cloth, "{added} cloth bones listed for a skeleton with $cloth bones");
    assert!(removed > 0, "no decompiled cloth definition was left out");
    // Some new decompiles place bones off the compiled model (Lady Geist's fur;
    // Yamato's match); the source now agrees with it within the physics
    // transfer budget either way.
    let doc = deadlock_resource::text::Document::parse(&stripped).unwrap();
    for node in doc.objects(Some("Bone")) {
        let name = node.get("name").and_then(|n| n.scalar()).unwrap();
        let alias = name.strip_prefix('_').map(|s| format!("${s}"));
        let Some(i) = compiled.names.iter().position(|n| n == name || alias.as_deref() == Some(n.as_str())) else { continue };
        let read = |key: &str| -> Vec<f64> { node.get(key).unwrap().array().unwrap().iter().map(|n| n.scalar().unwrap().trim().parse().unwrap()).collect() };
        let (origin, angles) = (read("origin"), read("angles"));
        for k in 0..3 {
            assert!((origin[k] - compiled.positions[i][k]).abs() < 1e-4, "{name} origin {origin:?} vs {:?}", compiled.positions[i]);
        }
        let q = deadlock_model::modeldoc::quaternion_of([angles[0], angles[1], angles[2]]);
        let c = compiled.rotations[i];
        let delta = [1., -1.].into_iter().map(|s| (0..4).map(|k| (q[k] * s - c[k]).abs()).fold(0., f64::max)).fold(f64::MAX, f64::min);
        assert!(delta < 1e-5, "{name} angles {angles:?} vs {c:?}");
    }
    // The compile template requires every bone of the compiled skeleton.
    let (target, _) = deadlock_skeleton::transforms::prepare_target(&compiled).unwrap();
    let doc = deadlock_resource::text::Document::parse(&stripped).unwrap();
    let sha = format!("{:x}", <sha2::Sha256 as sha2::Digest>::digest(&raw));
    let report = deadlock_skeleton::source_skeleton::SourceAuditReport {
        compiled_sha256: Some(sha.clone()),
        runtime_motion: deadlock_skeleton::source_skeleton::RuntimeMotionSourceAudit { status: "audited".into(), resource_sha256: Some(sha), cloth_nodes: 1, controls: vec![], skeleton: Some(compiled.clone()) },
    };
    let bones = deadlock_model::source_skeleton::canonical_source_hierarchy(deadlock_model::bone_hierarchy(&doc).unwrap(), Some(&report)).unwrap();
    let missing: Vec<&str> = target.iter().filter(|b| !bones.contains_key(&b.name)).map(|b| b.name.as_str()).collect();
    assert!(missing.is_empty(), "missing from the source skeleton: {:?}…", &missing[..missing.len().min(5)]);
    for b in &target {
        assert_eq!(bones[&b.name].parent, b.parent, "{}", b.name);
    }
    // The listed bones carry the compiled transforms (Source 2 Viewer's spelling).
    let Some(first) = compiled.names.iter().position(|n| n.starts_with("$cloth")) else { return };
    let origin = format!("origin = [ {}, {}, {} ]", compiled.positions[first][0] as f32, compiled.positions[first][1] as f32, compiled.positions[first][2] as f32);
    assert!(stripped.contains(&origin), "{origin}");
    assert!(!stripped.contains("ClothProxyMeshFile") && !stripped.contains("\"Softbody\""));
}
