use deadlock_playback_test::{animation::*, native::*};
use deadlock_resource::{binary, container, Kind};
use std::collections::BTreeMap;
fn motion() -> Motion {
    serde_json::from_str(include_str!("../resources/authored-motion.json")).unwrap()
}
fn model() -> Vec<u8> {
    envelope(
        1,
        &[
            (
                "DATA".into(),
                include_bytes!("../../deadlock-cloth/tests/fixtures/compact-ribbon/data.bin")
                    .to_vec(),
            ),
            (
                "PHYS".into(),
                include_bytes!("../../deadlock-cloth/tests/fixtures/compact-ribbon/phys.bin")
                    .to_vec(),
            ),
            ("MDAT".into(), b"opaque mesh bytes must survive".to_vec()),
        ],
    )
    .unwrap()
}
fn skeleton() -> Vec<u8> {
    let original = data(&model(), "DATA").unwrap();
    let root = obj(vec![
        ("m_ID", s("models/heroes_staging/haze/haze.vnmskel")),
        ("m_boneIDs", list(vec![s(ANCHOR)])),
        ("m_parentIndices", list(vec![i(-1)])),
        (
            "m_parentSpaceReferencePose",
            list(vec![vector(&[0., 0., 0., 1., 0., 0., 0., 1.])]),
        ),
        (
            "m_modelSpaceReferencePose",
            list(vec![vector(&[0., 0., 0., 1., 0., 0., 0., 1.])]),
        ),
        ("m_numBonesToSampleAtLowLOD", i(1)),
        ("m_maskDefinitions", list(vec![])),
        ("m_secondarySkeletons", list(vec![])),
        ("m_bIsPropSkeleton", b(false)),
    ]);
    envelope(
        1,
        &[(
            "DATA".into(),
            binary::encode(&root, original.format_guid).unwrap(),
        )],
    )
    .unwrap()
}
#[test]
fn resource_ids_match_two_independent_native_rerl_records() {
    assert_eq!(
        resource_id("animation/skeletons/characters/viewmodel.vnmskel"),
        0x9174afe96ca9ada4
    );
    assert_eq!(
        resource_id("animation/skeletons/weapons/ak47.vnmskel"),
        0xb437c30c3ebcbc99
    );
}
#[test]
fn authored_animation_is_nontrivial_and_closes() {
    let m = motion();
    m.validate().unwrap();
    assert!(qdistance(m.rotations[0][0], m.rotations[33][0]) > 0.3);
}
#[test]
fn quaternion_codec_covers_every_largest_component_and_sign() {
    let mut seed = 983521u64;
    for _ in 0..10000 {
        let q = normalize(std::array::from_fn(|_| {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            (seed >> 32) as f64 / u32::MAX as f64 * 2. - 1.
        }));
        assert!(qdistance(q, unpack(pack(q))) < 0.0003);
    }
}
#[test]
fn only_selected_physics_outputs_and_flags_change() {
    let raw = model();
    let (new, skel, receipt) = prepare_model(&raw).unwrap();
    let old = data(&raw, "PHYS").unwrap();
    let out = data(&new, "PHYS").unwrap();
    let a = field(&old.root, "m_pFeModel").unwrap();
    let b = field(&out.root, "m_pFeModel").unwrap();
    assert_eq!(
        arr(a, "m_NodeBases").unwrap().len() - arr(b, "m_NodeBases").unwrap().len(),
        14
    );
    unchanged_except(a, b, &["m_NodeBases", "m_ReverseOffsets"]).unwrap();
    let after = deadlock_skeleton::Skeleton::from_value(
        field(&data(&new, "DATA").unwrap().root, "m_modelSkeleton").unwrap(),
    )
    .unwrap();
    for j in 0..skel.names.len() {
        if skel.names[j].starts_with("dmb_secondary_spring_base_L_a_") {
            assert_eq!(after.flags[j] & 0x400008, 0);
        } else {
            assert_eq!(after.flags[j], skel.flags[j]);
        }
    }
    let l = container::layout(&new).unwrap();
    assert_eq!(
        container::payload(&new, container::unique(&l.blocks, "MDAT").unwrap()).unwrap(),
        b"opaque mesh bytes must survive"
    );
    assert_eq!(receipt["removed_output_records"]["m_NodeBases"], 14);
    assert!(prepare_model(&new).is_err());
}
#[test]
fn unsupported_physics_output_route_stops_test() {
    let raw = model();
    let mut doc = data(&raw, "PHYS").unwrap();
    let fe = doc.root.get_mut("m_pFeModel").unwrap();
    set(fe, "m_FitMatrices", list(vec![i(1)])).unwrap();
    let changed = rebuild(
        &raw,
        BTreeMap::from([("PHYS".into(), encode(&doc).unwrap())]),
    )
    .unwrap();
    assert!(prepare_model(&changed).is_err());
}
#[test]
fn secondary_skeleton_preserves_primary_bones_and_attaches_at_spine() {
    let (_, skel, _) = prepare_model(&model()).unwrap();
    let raw = skeleton();
    let (primary, secondary) = prepare_skeleton(&raw, &skel).unwrap();
    unchanged_except(
        &data(&raw, "DATA").unwrap().root,
        &data(&primary, "DATA").unwrap().root,
        &["m_secondarySkeletons"],
    )
    .unwrap();
    let s = data(&secondary, "DATA").unwrap();
    assert_eq!(arr(&s.root, "m_boneIDs").unwrap().len(), 7);
    assert_eq!(
        field(&s.root, "m_bIsPropSkeleton").unwrap().kind,
        Kind::Bool(true)
    );
    assert_eq!(refs(&primary).unwrap().last().unwrap().1, SECONDARY);
    let local = arr(&s.root, "m_parentSpaceReferencePose").unwrap();
    for (j, &index) in selected(&skel).unwrap().iter().enumerate() {
        assert_eq!(
            number(&local[j].array().unwrap()[3]).unwrap(),
            skel.scales[index]
        );
    }
}
#[test]
fn injection_preserves_primary_timing_events_and_pose_samples() {
    let (_, skel, _) = prepare_model(&model()).unwrap();
    let raw = include_bytes!("../../deadlock-components/tests/fixtures/clip.bin");
    let before = data(raw, "DATA").unwrap();
    for animated in [false, true] {
        let new = inject_clip(raw, &skel, &motion(), animated).unwrap();
        let after = data(&new, "DATA").unwrap();
        unchanged_except(&before.root, &after.root, &["m_secondaryAnimations"]).unwrap();
        assert_eq!(arr(&after.root, "m_secondaryAnimations").unwrap().len(), 1);
    }
}
#[test]
fn original_pose_bytes_are_not_reencoded_when_sample_count_changes() {
    let (_, skel, _) = prepare_model(&model()).unwrap();
    let raw = include_bytes!("../../deadlock-components/tests/fixtures/clip.bin");
    let mut d = data(raw, "DATA").unwrap();
    set(&mut d.root, "m_nNumFrames", i(91)).unwrap();
    set(&mut d.root, "m_flDuration", f(3.)).unwrap();
    let track = arr(&d.root, "m_trackCompressionSettings").unwrap();
    let mut t = track[0].clone();
    set(&mut t, "m_bIsTranslationStatic", b(true)).unwrap();
    set(&mut d.root, "m_trackCompressionSettings", list(vec![t])).unwrap();
    set(
        &mut d.root,
        "m_compressedPoseData",
        deadlock_resource::Value::new(Kind::Blob(vec![])),
    )
    .unwrap();
    set(&mut d.root, "m_compressedPoseOffsets", list(vec![i(0); 91])).unwrap();
    let raw = rebuild(raw, BTreeMap::from([("DATA".into(), encode(&d).unwrap())])).unwrap();
    let new = inject_clip(&raw, &skel, &motion(), true).unwrap();
    let out = data(&new, "DATA").unwrap();
    unchanged_except(&d.root, &out.root, &["m_secondaryAnimations"]).unwrap();
    let sec = &arr(&out.root, "m_secondaryAnimations").unwrap()[0];
    assert_eq!(field(sec, "m_nNumFrames").unwrap().integer().unwrap(), 91);
    assert_eq!(arr(sec, "m_compressedPoseOffsets").unwrap().len(), 91);
}
#[test]
fn additive_and_nonclosing_authored_inputs_are_rejected() {
    let (_, skel, _) = prepare_model(&model()).unwrap();
    let raw = include_bytes!("../../deadlock-components/tests/fixtures/clip.bin");
    let mut doc = data(raw, "DATA").unwrap();
    set(&mut doc.root, "m_bIsAdditive", b(true)).unwrap();
    let raw = rebuild(
        raw,
        BTreeMap::from([("DATA".into(), encode(&doc).unwrap())]),
    )
    .unwrap();
    assert!(inject_clip(&raw, &skel, &motion(), true).is_err());
    let mut m = motion();
    m.rotations[120][0] = [0.5, 0., 0., 3f64.sqrt() / 2.];
    assert!(m.validate().is_err());
}
#[test]
#[ignore = "Set DMB_PLAYBACK_NATIVE_DIR to independently downloaded upstream fixture folder"]
fn real_native_secondary_clip_preserves_existing_weapon_animation() {
    let dir = std::env::var("DMB_PLAYBACK_NATIVE_DIR").unwrap();
    let raw = std::fs::read(std::path::Path::new(&dir).join("shoot1_nova.vnmclip_c")).unwrap();
    let (_, skel, _) = prepare_model(&model()).unwrap();
    let before = data(&raw, "DATA").unwrap();
    let new = inject_clip(&raw, &skel, &motion(), true).unwrap();
    let after = data(&new, "DATA").unwrap();
    unchanged_except(&before.root, &after.root, &["m_secondaryAnimations"]).unwrap();
    let old_secondary = arr(&before.root, "m_secondaryAnimations").unwrap();
    let new_secondary = arr(&after.root, "m_secondaryAnimations").unwrap();
    assert_eq!(new_secondary.len(), old_secondary.len() + 1);
    for (a, b) in old_secondary.iter().zip(&new_secondary) {
        assert!(a.canonical_eq(b));
    }
    std::fs::write(
        std::path::Path::new(&dir).join("test-injected.vnmclip_c"),
        new,
    )
    .unwrap();
}
