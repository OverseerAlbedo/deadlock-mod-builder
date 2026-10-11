use deadlock_cloth_authoring::{parts::Kind, secondary, surface};
use deadlock_core::CancellationToken;
use deadlock_dmx::{Dmx, Rewrite};
use deadlock_project::{customization::SecondaryMotion, Project};
use deadlock_skeleton::extensions::SourceBinding;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

fn fixture() -> (Dmx, Project, SourceBinding, serde_json::Value) {
    let d = Dmx::parse(include_bytes!("fixtures/hair_motion/mixed.dmx").to_vec()).unwrap();
    let oracle: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/hair_motion/oracle.json")).unwrap();
    let mut skeleton = surface::binds(&d).unwrap().skeleton;
    skeleton.flags = vec![64; skeleton.names.len()];
    let extras = skeleton
        .names
        .iter()
        .filter(|n| n.starts_with("dmb_secondary_"))
        .cloned()
        .collect::<Vec<_>>();
    let binding = SourceBinding {
        source_sha256: format!("{:x}", Sha256::digest(d.bytes())),
        original_sha256: "0".repeat(64),
        skeleton,
        extra_names: extras.clone(),
        required_extra_names: extras,
    };
    let mut p = Project::default();
    p.original_model = "models/heroes_staging/hornet_v3/hornet.vmdl".into();
    p.customization.enabled = Some(true);
    p.customization.secondary_motion = Some(SecondaryMotion {
        enabled: Some(true),
        preset: Some("automatic".into()),
        ..Default::default()
    });
    p.preparation_options
        .values
        .insert("preserve_physics_bones".into(), true.into());
    p.preparation_options
        .values
        .insert("physics_roots".into(), oracle["roots"].clone());
    (d, p, binding, oracle)
}
fn stream(d: &Dmx, key: &str) -> Vec<f32> {
    let e = d
        .elements
        .iter()
        .find(|e| e.kind == "DmeVertexData" && e.get(key).is_some())
        .unwrap();
    let a = e.attributes.iter().find(|a| a.name == key).unwrap();
    d.raw_attribute(a)[4..]
        .chunks_exact(4)
        .map(|v| f32::from_le_bytes(v.try_into().unwrap()))
        .collect()
}
fn maps(d: &Dmx) -> BTreeMap<String, [f32; 6]> {
    let keys = [
        "cloth_goal_strength$0",
        "cloth_drag$0",
        "cloth_antishrink$0",
        "cloth_gravity$0",
        "cloth_stray_radius$0",
        "cloth_stray_radius_stretchiness$0",
    ];
    let values = keys.map(|k| stream(d, k));
    let binds = surface::binds(d).unwrap();
    let state = d
        .elements
        .iter()
        .find(|e| e.kind == "DmeVertexData" && e.get("cloth_enable$0").is_some())
        .unwrap();
    let raw = |key: &str| d.raw_attribute(state.attributes.iter().find(|a| a.name == key).unwrap());
    let slots = i32::from_le_bytes(raw("jointCount").try_into().unwrap()) as usize;
    let joints = raw("blendindices$0")[4..]
        .chunks_exact(4)
        .map(|v| i32::from_le_bytes(v.try_into().unwrap()) as usize)
        .collect::<Vec<_>>();
    let enabled = stream(d, "cloth_enable$0");
    assert_eq!(joints.len(), enabled.len() * slots);
    let mut result = BTreeMap::new();
    for (i, joints) in joints.chunks_exact(slots).enumerate() {
        if enabled[i] == 0. {
            continue;
        }
        let name = d.elements[binds.palette[joints[0]]].name.clone();
        let row = std::array::from_fn(|k| values[k][i]);
        if let Some(prior) = result.insert(name, row) {
            assert_eq!(prior, row);
        }
    }
    result
}
#[test]
fn automatic_long_hair_is_softer_without_changing_short_hair_or_helper_invariance() {
    let (d, p, binding, oracle) = fixture();
    let setup = secondary::setup(&p, &d, Some(&binding), &CancellationToken::new())
        .unwrap()
        .unwrap();
    assert!(setup.parts.iter().all(|p| p.kind == Kind::Hair));
    let report = setup.hair_motion.as_ref().unwrap();
    assert_eq!(report.shared_profile_groups.len(), 5);
    assert!(report.custom_profile_groups.is_empty());
    assert_eq!(report.review_groups, ["hair_single", "hair_branch"]);
    assert_eq!(report.length_adjusted_groups.len(), 3);
    assert_eq!(report.jump_protected_groups.len(), 3);
    assert!(setup.requires_animation_limits());
    for adjusted in &report.length_adjusted_groups {
        assert_eq!(adjusted.short_hair_limit_inches, 12.);
        assert_eq!(adjusted.goal_strength_multiplier, 0.6);
    }
    let proxy = Dmx::parse(setup.proxy).unwrap();
    assert_eq!(
        surface::binds(&d).unwrap().skeleton,
        surface::binds(&proxy).unwrap().skeleton
    );
    let settings = maps(&proxy);
    for (bone, t) in oracle["weighted_coordinates"].as_object().unwrap() {
        if bone.contains("single") || bone.contains("branch") {
            continue;
        }
        let t = t.as_f64().unwrap();
        let multiplier = if bone.contains("short") { 1. } else { 0.6 };
        let expected = [
            if t == 0. { 1. } else { (0.6 - 0.2 * t) * multiplier } as f32,
            0.,
            1.,
            0.,
            0.,
            0.,
        ];
        for k in [0, 1, 2, 3, 5] { assert_eq!(settings[bone][k], expected[k], "{bone} map {k}"); }
        if multiplier == 1. { assert_eq!(settings[bone][4], 0.); }
        else { assert!(settings[bone][4] > 0.); }
    }
    for helper in ["spring_imported", "helper_leading"] {
        assert_eq!(settings[&format!("dmb_secondary_{helper}")][0], settings["dmb_secondary_hair_short_0"][0]);
    }
    assert_eq!(
        settings["dmb_secondary_helper_end"][0],
        settings["dmb_secondary_ponytail_2"][0]
    );
    assert!(settings["dmb_secondary_ponytail_1"][0] < settings["dmb_secondary_bang_short_1"][0]);
    assert_eq!(settings["dmb_secondary_hair_short_1"][0], 0.4);
    assert_eq!(settings["dmb_secondary_ponytail_2"][0], 0.24);
    assert_eq!(
        settings["dmb_secondary_ponytail_1"][0],
        settings["dmb_secondary_hair_curved_1"][0]
    );
    assert_eq!(
        settings["dmb_secondary_hair_curved_1"][0],
        settings["dmb_secondary_hair_dense_2"][0]
    );
}
#[test]
fn explicit_hair_overrides_are_preserved_and_reported_instead_of_claimed_as_shared() {
    let (d, mut p, binding, _) = fixture();
    p.customization.secondary_motion.as_mut().unwrap().profiles =
        Some(BTreeMap::from([("spring_imported".into(), "soft".into())]));
    let setup = secondary::setup(&p, &d, Some(&binding), &CancellationToken::new())
        .unwrap()
        .unwrap();
    let report = setup.hair_motion.as_ref().unwrap();
    assert_eq!(report.shared_profile_groups.len(), 4);
    assert_eq!(report.custom_profile_groups, ["spring_imported"]);
    assert!(report.length_adjusted_groups.iter().all(|p| p.root != "spring_imported"));
    assert!(report.jump_protected_groups.iter().all(|p| p.root != "spring_imported"));
    let settings = maps(&Dmx::parse(setup.proxy).unwrap());
    assert_eq!(settings["dmb_secondary_hair_short_0"][0], 1.);
    assert_eq!(settings["dmb_secondary_ponytail_0"][0], 0.7);
    assert_eq!(settings["dmb_secondary_ponytail_2"][0], 0.28);
}
#[test]
fn long_hair_softens_even_when_no_short_hair_is_selected() {
    let (d, mut p, binding, _) = fixture();
    let all = secondary::setup(&p, &d, Some(&binding), &CancellationToken::new()).unwrap().unwrap();
    p.customization.secondary_motion.as_mut().unwrap().roots = Some(vec!["spring_imported".into()]);
    let single = secondary::setup(&p, &d, Some(&binding), &CancellationToken::new()).unwrap().unwrap();
    assert_eq!(single.hair_motion.as_ref().unwrap().length_adjusted_groups.len(), 1);
    let all_maps = maps(&Dmx::parse(all.proxy).unwrap());
    let single_maps = maps(&Dmx::parse(single.proxy).unwrap());
    assert_eq!(single_maps["dmb_secondary_ponytail_2"][0], 0.24);
    for (name, values) in single_maps {
        assert_eq!(all_maps[&name], values, "{name}");
    }
}
#[test]
fn model_rotation_and_uniform_scale_do_not_change_shared_hair_settings() {
    let (d, p, binding, _) = fixture();
    let original = secondary::setup(&p, &d, Some(&binding), &CancellationToken::new())
        .unwrap()
        .unwrap();
    let head = d
        .elements
        .iter()
        .position(|e| e.kind == "DmeTransform" && e.name == "head")
        .unwrap();
    let mut edits = Rewrite::default();
    edits
        .attributes
        .insert((head, "scale".into()), 3_f32.to_le_bytes().to_vec());
    edits.attributes.insert(
        (head, "orientation".into()),
        [
            std::f32::consts::FRAC_1_SQRT_2,
            0_f32,
            0.,
            std::f32::consts::FRAC_1_SQRT_2,
        ]
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect(),
    );
    let rotated = Dmx::parse(d.rewrite_binary9(&edits).unwrap()).unwrap();
    let mut binding = binding;
    binding.source_sha256 = format!("{:x}", Sha256::digest(rotated.bytes()));
    binding.skeleton = surface::binds(&rotated).unwrap().skeleton;
    let changed = secondary::setup(&p, &rotated, Some(&binding), &CancellationToken::new())
        .unwrap()
        .unwrap();
    let before = maps(&Dmx::parse(original.proxy).unwrap());
    let after = maps(&Dmx::parse(changed.proxy).unwrap());
    for (bone, values) in before {
        for (k, (a, b)) in values.into_iter().zip(after[&bone]).enumerate() {
            let expected = if k == 4 { a * 3. } else { a };
            assert!((expected - b).abs() < 1e-5, "{bone} map {k}: {expected} vs {b}");
        }
    }
}

#[test]
fn travel_envelopes_stay_below_ceiling_and_stretch_maps_only_target_long_hair() {
    let (d, p, binding, _) = fixture();
    let setup = secondary::setup(&p, &d, Some(&binding), &CancellationToken::new()).unwrap().unwrap();
    let protected = &setup.hair_motion.as_ref().unwrap().jump_protected_groups;
    let proxy = Dmx::parse(setup.proxy.clone()).unwrap();
    let state = proxy.elements.iter().find(|e| e.kind == "DmeVertexData" && e.get("cloth_enable$0").is_some()).unwrap();
    let raw = |key: &str| proxy.raw_attribute(state.attributes.iter().find(|a| a.name == key).unwrap());
    let count = i32::from_le_bytes(raw("jointCount").try_into().unwrap()) as usize;
    let joints: Vec<_> = raw("blendindices$0")[4..].chunks_exact(4).map(|v| i32::from_le_bytes(v.try_into().unwrap()) as usize).collect();
    let positions: Vec<_> = raw("position$0")[4..].chunks_exact(4).map(|v| f32::from_le_bytes(v.try_into().unwrap()) as f64).collect();
    let binds = surface::binds(&proxy).unwrap();
    let radii = stream(&proxy, "cloth_stray_radius$0");
    let suspenders = stream(&proxy, "cloth_suspenders$0");
    let stretch = stream(&proxy, "cloth_stretch$0");
    let enabled = stream(&proxy, "cloth_enable$0");
    let mut bounded = 0;
    for (i, js) in joints.chunks_exact(count).enumerate() {
        let name = &proxy.elements[binds.palette[js[0]]].name;
        if let Some(plan) = protected.iter().find(|plan| plan.travel_radius_by_bone.contains_key(name)) {
            assert_eq!(enabled[i], 1.);
            assert!(plan.stretch_tethers && radii[i] > 0.);
            assert!((radii[i] as f64 - plan.travel_radius_by_bone[name]).abs() < 1e-5);
            assert_eq!(suspenders[i], 1.);
            let height: f64 = (0..3).map(|k| (positions[i*3+k] - plan.reference_position_world[k])*plan.up_axis_world[k]).sum();
            assert!(height + radii[i] as f64 <= plan.ceiling_from_reference_inches + 1e-5);
            bounded += 1;
        } else {
            assert_eq!(suspenders[i], 0.);
            assert_eq!(radii[i], 0.);
        }
        assert_eq!(stretch[i], 0.);
    }
    assert!(bounded > 0);
}

#[test]
fn short_hair_only_needs_no_extra_maps_or_native_travel_gate() {
    let (d, mut p, binding, _) = fixture();
    p.customization.secondary_motion.as_mut().unwrap().roots = Some(vec!["hair_short_0".into()]);
    let setup = secondary::setup(&p, &d, Some(&binding), &CancellationToken::new()).unwrap().unwrap();
    assert!(!setup.requires_animation_limits());
    let proxy = Dmx::parse(setup.proxy).unwrap();
    assert!(proxy.elements.iter().all(|e| e.get("cloth_suspenders$0").is_none()));
}
