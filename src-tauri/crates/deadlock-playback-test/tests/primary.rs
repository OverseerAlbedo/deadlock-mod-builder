use deadlock_playback_test::{animation::*, native::*, primary};
use deadlock_resource::Kind;
use deadlock_skeleton::Skeleton;
use std::collections::{BTreeMap, BTreeSet};

fn model() -> (Vec<u8>, Skeleton) {
    let raw = envelope(
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
        ],
    )
    .unwrap();
    let (_, skel, _) = prepare_primary_model(&raw).unwrap();
    (raw, skel)
}
fn skel() -> &'static [u8] {
    include_bytes!("fixtures/primary-r2/haze.vnmskel_c")
}
fn idle() -> &'static [u8] {
    include_bytes!("fixtures/primary-r2/idle_hideout.vnmclip_c")
}
fn motion() -> Motion {
    serde_json::from_str(include_str!("../resources/authored-motion.json")).unwrap()
}
fn blob(v: &deadlock_resource::Value) -> &[u8] {
    let Kind::Blob(b) = &field(v, "m_compressedPoseData").unwrap().kind else {
        panic!()
    };
    b
}

#[test]
fn primary_binding_keeps_original_indices_and_inherits_anchor_masks() {
    let (raw, model) = model();
    let (patched_model, _, _) = prepare_primary_model(&raw).unwrap();
    assert!(
        field(&data(&raw, "DATA").unwrap().root, "m_vecNmSkeletonRefs")
            .unwrap()
            .canonical_eq(
                field(
                    &data(&patched_model, "DATA").unwrap().root,
                    "m_vecNmSkeletonRefs"
                )
                .unwrap()
            )
    );
    let before = data(skel(), "DATA").unwrap();
    let (new, binding) = primary::skeleton(skel(), &model).unwrap();
    assert_eq!(binding.original_bones, 98);
    let after = data(&new, "DATA").unwrap();
    for key in [
        "m_boneIDs",
        "m_parentIndices",
        "m_parentSpaceReferencePose",
        "m_modelSpaceReferencePose",
    ] {
        let a = arr(&before.root, key).unwrap();
        let b = arr(&after.root, key).unwrap();
        assert_eq!(b.len(), 105);
        assert!(a.iter().zip(&b).all(|(a, b)| a.canonical_eq(b)));
    }
    assert_eq!(
        arr(&after.root, "m_parentIndices").unwrap()[98]
            .integer()
            .unwrap(),
        13
    );
    assert_eq!(
        field(&after.root, "m_numBonesToSampleAtLowLOD")
            .unwrap()
            .integer()
            .unwrap(),
        105
    );
    for (a, b) in arr(&before.root, "m_maskDefinitions")
        .unwrap()
        .iter()
        .zip(arr(&after.root, "m_maskDefinitions").unwrap())
    {
        let a = field(a, "m_primaryWeightList").unwrap();
        let b = field(&b, "m_primaryWeightList").unwrap();
        let ids = arr(a, "m_boneIDs").unwrap();
        let weights = arr(a, "m_weights").unwrap();
        let expected = ids
            .iter()
            .position(|n| n.string().ok() == Some(ANCHOR))
            .map(|j| number(&weights[j]).unwrap())
            .unwrap_or(0.);
        let newids = arr(b, "m_boneIDs").unwrap();
        let newweights = arr(b, "m_weights").unwrap();
        assert!(ids.iter().zip(&newids).all(|(a, b)| a.canonical_eq(b)));
        assert!(weights
            .iter()
            .zip(&newweights)
            .all(|(a, b)| a.canonical_eq(b)));
        assert!(newweights[weights.len()..]
            .iter()
            .all(|v| number(v).unwrap() == expected));
    }
}

#[test]
fn animated_primary_keeps_every_body_sample_and_has_per_track_offsets() {
    let (_, model) = model();
    let (_, binding) = primary::skeleton(skel(), &model).unwrap();
    let before = data(idle(), "DATA").unwrap();
    let new = primary::clip(idle(), &binding, &model, &motion(), true).unwrap();
    let after = data(&new, "DATA").unwrap();
    unchanged_except(
        &before.root,
        &after.root,
        &[
            "m_trackCompressionSettings",
            "m_compressedPoseData",
            "m_compressedPoseOffsets",
        ],
    )
    .unwrap();
    let frames = field(&before.root, "m_nNumFrames")
        .unwrap()
        .integer()
        .unwrap() as usize;
    let width = blob(&before.root).len() / frames;
    let tracks = arr(&after.root, "m_trackCompressionSettings").unwrap();
    for j in 0..7 {
        assert_eq!(
            field(&tracks[98 + j], "m_nTrackReadOffset")
                .unwrap()
                .integer()
                .unwrap(),
            (width / 2 + j * 3) as i128
        );
    }
    let mut min = 1.;
    let mut max = 0.;
    for frame in 0..frames {
        let old = &blob(&before.root)[frame * width..(frame + 1) * width];
        let start = frame * (width + 42);
        assert_eq!(old, &blob(&after.root)[start..start + width]);
        let p = start + width;
        let bytes = &blob(&after.root)[p..p + 6];
        let q = unpack(std::array::from_fn(|j| {
            u16::from_le_bytes([bytes[2 * j], bytes[2 * j + 1]])
        }));
        let distance = qdistance(q, normalize(model.rotations[binding.indices[0]]));
        min = f64::min(min, distance);
        max = f64::max(max, distance);
    }
    assert!(min < 0.0003 && max > 0.3);
}

#[test]
fn fixed_and_additive_tracks_preserve_original_blob_and_additive_identity() {
    let (_, model) = model();
    let (_, binding) = primary::skeleton(skel(), &model).unwrap();
    for (raw, additive) in [
        (idle(), false),
        (
            include_bytes!("fixtures/primary-r2/item_crouch_idle.vnmclip_c").as_slice(),
            false,
        ),
        (
            include_bytes!("fixtures/primary-r2/aim_hideout_idle_mid.vnmclip_c").as_slice(),
            true,
        ),
    ] {
        let before = data(raw, "DATA").unwrap();
        let new = primary::clip(raw, &binding, &model, &motion(), false).unwrap();
        let after = data(&new, "DATA").unwrap();
        unchanged_except(&before.root, &after.root, &["m_trackCompressionSettings"]).unwrap();
        let tracks = arr(&after.root, "m_trackCompressionSettings").unwrap();
        for (j, t) in tracks[98..].iter().enumerate() {
            assert!(matches!(
                field(t, "m_bIsRotationStatic").unwrap().kind,
                Kind::Bool(true)
            ));
            let scale = number(field(field(t, "m_scaleRange").unwrap(), "m_flRangeStart").unwrap())
                .unwrap();
            assert_eq!(
                scale,
                if additive {
                    0.
                } else {
                    model.scales[binding.indices[j]]
                }
            );
            if additive {
                for k in [
                    "m_translationRangeX",
                    "m_translationRangeY",
                    "m_translationRangeZ",
                ] {
                    assert_eq!(
                        number(field(field(t, k).unwrap(), "m_flRangeStart").unwrap()).unwrap(),
                        0.
                    );
                }
                let q = arr(t, "m_constantRotation").unwrap();
                assert_eq!(number(&q[3]).unwrap(), 1.);
            }
        }
        if additive {
            assert!(primary::clip(raw, &binding, &model, &motion(), true).is_err());
        }
    }
}

#[test]
fn bad_offsets_incomplete_clip_coverage_and_existing_binding_are_rejected() {
    let (_, model) = model();
    let (patched, binding) = primary::skeleton(skel(), &model).unwrap();
    assert!(primary::skeleton(&patched, &model).is_err());
    assert!(primary::verify_coverage(
        &BTreeSet::from(["missing.vnmclip_c".into()]),
        &BTreeSet::new(),
        &BTreeSet::new()
    )
    .is_err());
    let mut doc = data(idle(), "DATA").unwrap();
    let mut tracks = arr(&doc.root, "m_trackCompressionSettings").unwrap();
    set(&mut tracks[5], "m_nTrackReadOffset", i(44)).unwrap();
    set(&mut doc.root, "m_trackCompressionSettings", list(tracks)).unwrap();
    let broken = rebuild(
        idle(),
        BTreeMap::from([("DATA".into(), encode(&doc).unwrap())]),
    )
    .unwrap();
    assert!(primary::clip(&broken, &binding, &model, &motion(), true).is_err());
}

#[test]
fn missing_ui_only_clips_are_reported_but_shared_gameplay_clips_are_required() {
    let missing = "models/heroes_staging/haze/clips/ui_hero_reveal.vnmclip_c".to_owned();
    let ui = BTreeSet::from([missing.clone(), "present.vnmclip_c".into()]);
    let available = BTreeSet::from(["present.vnmclip_c".into()]);
    assert_eq!(
        primary::verify_coverage(&BTreeSet::new(), &ui, &available).unwrap(),
        vec![missing.clone()]
    );
    // Its UI-looking NAME must not exempt it when gameplay also references it.
    assert!(primary::verify_coverage(&BTreeSet::from([missing]), &ui, &available).is_err());
}
