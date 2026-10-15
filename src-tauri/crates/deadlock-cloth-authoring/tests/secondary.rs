use deadlock_cloth_authoring::{
    secondary::{self, Group},
    surface,
};
use deadlock_core::CancellationToken;
use deadlock_dmx::Dmx;
use deadlock_project::{customization::SecondaryMotion, Project};
use deadlock_skeleton::extensions::SourceBinding;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::Path};
mod common;
fn raw(name: &str) -> Vec<u8> {
    fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/secondary")
            .join(name),
    )
    .unwrap()
}
fn binding(d: &Dmx) -> SourceBinding {
    let mut s = surface::binds(d).unwrap().skeleton;
    s.flags = vec![64; s.names.len()];
    let extra = s.names[1..].to_vec();
    SourceBinding {
        source_sha256: format!("{:x}", Sha256::digest(d.bytes())),
        original_sha256: "a".repeat(64),
        skeleton: s,
        extra_names: extra.clone(),
        required_extra_names: extra,
    }
}
fn stream(d: &Dmx, name: &str) -> Vec<f64> {
    let e = d
        .elements
        .iter()
        .find(|e| e.kind == "DmeVertexData" && e.get(name).is_some())
        .unwrap();
    let a = e.attributes.iter().find(|a| a.name == name).unwrap();
    d.raw_attribute(a)[4..]
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes(b.try_into().unwrap()) as f64)
        .collect()
}
#[test]
fn automatic_frames_match_independent_python_and_preserve_all_binds() {
    for name in ["chain", "independent"] {
        let d = Dmx::parse(raw(&format!("{name}.dmx"))).unwrap();
        let j: serde_json::Value = serde_json::from_slice(&raw(&format!("{name}.json"))).unwrap();
        let expected: Vec<Group> = serde_json::from_value(j["groups"].clone()).unwrap();
        let roots = expected
            .iter()
            .map(|g| g.source_root.clone())
            .collect::<Vec<_>>();
        let source = binding(&d);
        let groups = secondary::reviewed_groups(&d, &roots, &source).unwrap();
        assert_eq!(groups, expected);
        let (bytes, _, _) =
            secondary::generate(&d, &groups, &BTreeMap::new(), &CancellationToken::new()).unwrap();
        let actual = Dmx::parse(bytes).unwrap();
        let oracle = Dmx::parse(raw(&format!("{name}.proxy.dmx"))).unwrap();
        for key in [
            "position$0",
            "blendweights$0",
            "blendindices$0",
            "cloth_enable$0",
            "cloth_goal_strength$0",
            "cloth_drag$0",
            "cloth_antishrink$0",
        ] {
            let a = stream(&actual, key);
            let b = stream(&oracle, key);
            assert_eq!(a.len(), b.len(), "{key}");
            for (a, b) in a.iter().zip(&b) {
                assert!((a - b).abs() < 1e-6, "{name} {key}: {a} vs {b}");
            }
        }
        assert_eq!(
            surface::binds(&actual).unwrap().skeleton.positions,
            source.skeleton.positions
        );
        assert_eq!(d.bytes(), raw(&format!("{name}.dmx")));
    }
}
#[test]
fn selection_rejects_body_children_duplicates_unweighted_and_collapsed() {
    let d = Dmx::parse(raw("chain.dmx")).unwrap();
    let source = binding(&d);
    for roots in [
        vec![],
        vec!["pelvis".into()],
        vec!["garment_mid".into()],
        vec!["garment_root".into(), "garment_root".into()],
    ] {
        assert!(secondary::reviewed_groups(&d, &roots, &source).is_err());
    }
    let d = Dmx::parse(raw("unweighted.dmx")).unwrap();
    assert!(secondary::reviewed_groups(&d, &["garment_root".into()], &binding(&d)).is_err());
    let d = Dmx::parse(raw("collapsed.dmx")).unwrap();
    let groups = secondary::reviewed_groups(&d, &["garment_root".into()], &binding(&d)).unwrap();
    assert!(secondary::generate(&d, &groups, &BTreeMap::new(), &CancellationToken::new()).is_err());
}
#[test]
fn setup_requires_reviewed_metadata_and_tang_rejects_other_models() {
    let d = Dmx::parse(raw("chain.dmx")).unwrap();
    let source = binding(&d);
    let c = CancellationToken::new();
    let mut p = Project::default();
    p.original_model = "models/heroes_staging/haze/haze.vmdl".into();
    p.customization.enabled = Some(true);
    p.customization.secondary_motion = Some(SecondaryMotion {
        enabled: Some(true),
        preset: Some("automatic".into()),
        ..Default::default()
    });
    assert!(secondary::setup(&p, &d, Some(&source), &c).is_err());
    p.preparation_options
        .values
        .insert("preserve_physics_bones".into(), true.into());
    p.preparation_options
        .values
        .insert("physics_roots".into(), serde_json::json!(["garment_root"]));
    assert!(secondary::setup(&p, &d, Some(&source), &c)
        .unwrap()
        .is_some());
    p.customization.secondary_motion.as_mut().unwrap().preset = Some("tangtang_r2".into());
    assert!(secondary::setup(&p, &d, Some(&source), &c).is_err());
    p.customization.secondary_motion.as_mut().unwrap().preset = Some("tangtang_reference".into());
    assert!(secondary::setup(&p, &d, Some(&source), &c).is_err());
    p.customization.enabled = Some(false);
    assert!(secondary::setup(&p, &d, None, &c).unwrap().is_none());
}
#[test]
fn stronger_and_softer_groups_change_only_motion_streams() {
    let d = Dmx::parse(raw("independent.dmx")).unwrap();
    let roots = vec![
        "garment_root".into(),
        "garment_mid".into(),
        "garment_tip".into(),
    ];
    let groups = secondary::reviewed_groups(&d, &roots, &binding(&d)).unwrap();
    let c = CancellationToken::new();
    let (before, _, _) = secondary::generate(&d, &groups, &BTreeMap::new(), &c).unwrap();
    let (after, _, _) = secondary::generate(
        &d,
        &groups,
        &BTreeMap::from([
            ("garment_root".into(), "stiff".into()),
            ("garment_mid".into(), "soft".into()),
        ]),
        &c,
    )
    .unwrap();
    let a = Dmx::parse(before).unwrap();
    let b = Dmx::parse(after).unwrap();
    for k in [
        "position$0",
        "blendweights$0",
        "blendindices$0",
        "cloth_enable$0",
        "cloth_antishrink$0",
    ] {
        assert_eq!(stream(&a, k), stream(&b, k));
    }
    let a = stream(&a, "cloth_goal_strength$0");
    let b = stream(&b, "cloth_goal_strength$0");
    assert!(b[3] > a[3]);
    assert!(b[9] < a[9]);
    assert_eq!(&a[12..], &b[12..]);
}

#[test]
#[ignore = "requires the separately supplied TangTang prepared DMX and project"]
fn supplied_tangtang_reuses_r2_and_automatic_preserves_its_geometry() {
    let path = std::env::var("DMB_TANG_DMX").unwrap();
    let project = std::env::var("DMB_TANG_PROJECT").unwrap();
    let d = Dmx::parse(fs::read(path).unwrap()).unwrap();
    let mut p: Project = serde_json::from_slice(&fs::read(project).unwrap()).unwrap();
    let mut s = surface::binds(&d).unwrap().skeleton;
    s.flags = vec![64; s.names.len()];
    let extras = s
        .names
        .iter()
        .filter(|n| n.starts_with("dmb_secondary_"))
        .cloned()
        .collect::<Vec<_>>();
    // Source authoring comparison only. This is not a compiled-original authority.
    let source = SourceBinding {
        original_sha256: "0".repeat(64),
        source_sha256: format!("{:x}", Sha256::digest(d.bytes())),
        skeleton: s,
        extra_names: extras.clone(),
        required_extra_names: extras,
    };
    p.customization.enabled = Some(true);
    if let Some(old) = &mut p.customization.physics {
        old.enabled = Some(false);
    }
    p.customization.secondary_motion = Some(SecondaryMotion {
        enabled: Some(true),
        preset: Some("tangtang_r2".into()),
        ..Default::default()
    });
    let cancel = CancellationToken::new();
    let tuned = secondary::setup(&p, &d, Some(&source), &cancel)
        .unwrap()
        .unwrap();
    assert_eq!(tuned.proxy_sha256, secondary::TANG_PROXY);
    assert_eq!(tuned.groups.len(), 38);
    assert_eq!(tuned.dynamic_positions, 786);
    p.customization.secondary_motion.as_mut().unwrap().preset = Some("tangtang_reference".into());
    let reference = secondary::setup(&p, &d, Some(&source), &cancel)
        .unwrap()
        .unwrap();
    assert_eq!(reference.proxy_sha256, secondary::TANG_REFERENCE_PROXY);
    assert_eq!(reference.groups, tuned.groups);
    assert_eq!(
        (reference.proxy_positions, reference.dynamic_positions),
        (900, 786)
    );
    let new = Dmx::parse(reference.proxy).unwrap();
    let old = Dmx::parse(tuned.proxy.clone()).unwrap();
    for key in [
        "blendweights$0",
        "blendindices$0",
        "cloth_enable$0",
        "cloth_antishrink$0",
    ] {
        assert_eq!(stream(&new, key), stream(&old, key), "{key}");
    }
    let gravity = stream(&new, "cloth_gravity$0");
    assert_eq!(gravity.len(), 900);
    assert!(gravity.contains(&0.) && gravity.contains(&1.));
    assert!(gravity.iter().any(|v| (*v - 0.35).abs() < 1e-7));
    // Alpha.23 changes only selected proxy frames. The rendered source and
    // every serialized joint bind stay exact; accepted groups keep geometry.
    assert_eq!(surface::binds(&new).unwrap().skeleton, surface::binds(&old).unwrap().skeleton);
    let receipt: serde_json::Value = serde_json::from_str(
        &common::private_text("cloth-authoring/tangtang-stabilized.json").expect("private assets"),
    ).unwrap();
    let positions = stream(&new, "position$0");
    let prior = stream(&old, "position$0");
    let mut target = vec![false; 900];
    for group in receipt["changes"].as_array().unwrap() {
        let range = group["dynamic_point_range"].as_array().unwrap();
        let start = range[0].as_u64().unwrap() as usize;
        let end = range[1].as_u64().unwrap() as usize;
        target[start-3..end].fill(true);
    }
    for point in 0..900 {
        if !target[point] {
            assert_eq!(&positions[point*3..point*3+3], &prior[point*3..point*3+3]);
        }
    }
    let radii = stream(&new, "cloth_stray_radius$0");
    assert_eq!(radii.iter().filter(|v| **v > 0.).count(), 90);
    assert!(radii.iter().all(|v| v.is_finite() && (0.0..=17.16).contains(v)));
    // Alpha.25 changes only streamer proxy geometry/topology and free goals.
    // The independent Python checks additionally compare every corner normal
    // and face outside the two groups, and prove finite hinge rotations.
    let approved = Dmx::parse(common::private_asset("cloth-authoring/tangtang-streamers-proxy.dmx").expect("private assets")).unwrap();
    for key in ["cloth_gravity$0", "cloth_drag$0", "cloth_stray_radius$0",
                "cloth_stray_radius_stretchiness$0", "cloth_antishrink$0"] {
        assert_eq!(stream(&new, key), stream(&approved, key), "{key}");
    }
    let previous_positions = stream(&approved, "position$0");
    let goal = stream(&new, "cloth_goal_strength$0");
    let previous_goal = stream(&approved, "cloth_goal_strength$0");
    for point in 0..900 {
        let streamer = (660..702).contains(&point) || (705..747).contains(&point);
        if !streamer {
            assert_eq!(&positions[point*3..point*3+3], &previous_positions[point*3..point*3+3]);
        }
        let free = (666..702).contains(&point) || (711..747).contains(&point);
        assert_eq!(goal[point] != previous_goal[point], free, "goal {point}");
        if free {
            assert!((0.0199..=0.0401).contains(&goal[point]));
        }
    }
    assert!(!reference.render_source_modified);
    // Exact presets cannot silently accept an incomplete preparation review.
    let roots = p.preparation_options.values["physics_roots"].clone();
    p.preparation_options
        .values
        .get_mut("physics_roots")
        .unwrap()
        .as_array_mut()
        .unwrap()
        .pop();
    assert!(secondary::setup(&p, &d, Some(&source), &cancel).is_err());
    p.preparation_options
        .values
        .insert("physics_roots".into(), roots);
    p.customization.secondary_motion.as_mut().unwrap().preset = Some("automatic".into());
    let auto = secondary::setup(&p, &d, Some(&source), &cancel)
        .unwrap()
        .unwrap();
    assert_eq!(auto.groups, tuned.groups);
    assert_eq!(auto.proxy_positions, 900);
    assert!(auto.reused_accepted_reference);
    assert_eq!(auto.proxy_sha256, secondary::TANG_REFERENCE_PROXY);
    assert_eq!(auto.roll_lock_chains.len(), 2);
    let automatic = Dmx::parse(auto.proxy).unwrap();
    assert_eq!(surface::binds(&automatic).unwrap().skeleton, surface::binds(&d).unwrap().skeleton);
    println!("TangTang: exact R2 and accepted reference digests, 38 groups, 262 source bones, 900 points; automatic selects the accepted reference and two streamer roll locks. Render geometry and binds remain unchanged.");
}
