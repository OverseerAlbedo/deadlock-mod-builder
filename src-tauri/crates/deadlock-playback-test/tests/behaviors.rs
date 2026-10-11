use deadlock_playback_test::{
    animation::*,
    behaviors::{self, Behaviors},
    native::*,
    primary,
};
use deadlock_resource::{Kind, Value};
use deadlock_skeleton::Skeleton;
fn library() -> Behaviors {
    serde_json::from_str(include_str!("../resources/authored-behaviors.json")).unwrap()
}
fn select(path: &str, root: &Value) -> Result<Option<behaviors::Selection>> {
    let profiles = behaviors::RunProfiles::from([
        ("weapon_run_n.vnmclip_c".into(), "run_n".into()),
        ("hideout_crouch_walk_e.vnmclip_c".into(), "run_e".into()),
    ]);
    behaviors::select(path, root, &profiles)
}
fn model() -> Skeleton {
    Skeleton::from_value(
        field(
            &deadlock_resource::binary::decode(include_bytes!(
                "../../deadlock-cloth/tests/fixtures/compact-ribbon/data.bin"
            ))
            .unwrap()
            .root,
            "m_modelSkeleton",
        )
        .unwrap(),
    )
    .unwrap()
}
fn skeleton() -> &'static [u8] {
    include_bytes!("fixtures/primary-r2/haze.vnmskel_c")
}
fn stop() -> &'static [u8] {
    include_bytes!("fixtures/behaviors-r4/run_to_stop_stand.vnmclip_c")
}
fn blob(v: &Value) -> &[u8] {
    let Kind::Blob(ref b) = field(v, "m_compressedPoseData").unwrap().kind else {
        panic!()
    };
    b
}

#[test]
fn blender_profiles_are_restrained_and_seamless_and_bad_endpoints_rejected() {
    let mut m = library();
    m.validate().unwrap();
    m.profiles
        .get_mut("stop")
        .unwrap()
        .rotations
        .last_mut()
        .unwrap()[0] = [0.01, 0., 0., (1. - 0.0001f64).sqrt()];
    assert!(m.validate().unwrap_err().to_string().contains("endpoints"));
}
#[test]
fn state_selection_covers_run_turn_stop_without_animating_unrelated_overlays() {
    let run = data(
        include_bytes!("fixtures/behaviors-r4/weapon_run_n.vnmclip_c"),
        "DATA",
    )
    .unwrap();
    assert_eq!(
        select("weapon_run_n.vnmclip_c", &run.root)
            .unwrap()
            .unwrap()
            .profile,
        "run_n"
    );
    assert_eq!(
        select("hideout_crouch_walk_e.vnmclip_c", &run.root)
            .unwrap()
            .unwrap()
            .strength,
        0.45
    );
    let turn = data(
        include_bytes!("fixtures/behaviors-r4/turn_left.vnmclip_c"),
        "DATA",
    )
    .unwrap();
    assert_eq!(
        select("turn_left.vnmclip_c", &turn.root)
            .unwrap()
            .unwrap()
            .profile,
        "turn_l"
    );
    let additive = data(stop(), "DATA").unwrap();
    assert_eq!(
        select("run_to_stop_stand.vnmclip_c", &additive.root)
            .unwrap()
            .unwrap()
            .category,
        "stop"
    );
    assert!(select("aim_weapon_idle.vnmclip_c", &additive.root)
        .unwrap()
        .is_none());
    for name in [
        "death_idle_01",
        "reload_idle",
        "aim_weapon_idle.vnmclip+non_additive",
        "run_to_stop_stand.vnmclip+non_additive",
    ] {
        assert!(select(&format!("{name}.vnmclip_c"), &run.root)
            .unwrap()
            .is_none());
    }
    let still = data(
        include_bytes!("fixtures/primary-r2/item_crouch_idle.vnmclip_c"),
        "DATA",
    )
    .unwrap();
    assert!(select("weapon_stand_idle.vnmclip_c", &still.root)
        .unwrap()
        .is_none());
}
#[test]
fn native_stop_uses_rotation_deltas_with_zero_translation_and_scale_delta() {
    let model = model();
    let (_, binding) = primary::behavior_skeleton(skeleton(), &model).unwrap();
    let before = data(stop(), "DATA").unwrap();
    let m = library();
    let choice = select("run_to_stop_stand.vnmclip_c", &before.root)
        .unwrap()
        .unwrap();
    let frames = field(&before.root, "m_nNumFrames")
        .unwrap()
        .integer()
        .unwrap() as usize;
    let samples = m.samples(&choice, frames);
    let out = primary::clip_with_deltas(stop(), &binding, &model, Some(&samples)).unwrap();
    let after = data(&out, "DATA").unwrap();
    unchanged_except(
        &before.root,
        &after.root,
        &[
            "m_compressedPoseData",
            "m_compressedPoseOffsets",
            "m_trackCompressionSettings",
        ],
    )
    .unwrap();
    let tracks = arr(&after.root, "m_trackCompressionSettings").unwrap();
    let width = blob(&before.root).len() / frames;
    for t in &tracks[98..] {
        for k in [
            "m_translationRangeX",
            "m_translationRangeY",
            "m_translationRangeZ",
            "m_scaleRange",
        ] {
            assert_eq!(
                number(field(field(t, k).unwrap(), "m_flRangeStart").unwrap()).unwrap(),
                0.
            );
        }
    }
    let mut maximum = 0.0f64;
    for f in 0..frames {
        let at = f * (width + 42);
        assert_eq!(
            &blob(&after.root)[at..at + width],
            &blob(&before.root)[f * width..(f + 1) * width]
        );
        for j in 0..7 {
            let p = at + width + j * 6;
            let bytes = &blob(&after.root)[p..p + 6];
            let q = unpack(std::array::from_fn(|i| {
                u16::from_le_bytes([bytes[i * 2], bytes[i * 2 + 1]])
            }));
            assert!(qdistance(q, samples[f][j]) < 0.0003);
            maximum = maximum.max(qdistance(q, [0., 0., 0., 1.]));
        }
    }
    assert!(maximum > 0.05 && maximum < 0.08);
}
#[test]
fn turn_override_excludes_only_added_streamer_bones_and_preserves_body_masks() {
    let model = model();
    let (r3, _) = primary::skeleton(skeleton(), &model).unwrap();
    let (r4, _) = primary::behavior_skeleton(skeleton(), &model).unwrap();
    let before = data(&r3, "DATA").unwrap();
    let after = data(&r4, "DATA").unwrap();
    unchanged_except(&before.root, &after.root, &["m_maskDefinitions"]).unwrap();
    for (a, b) in arr(&before.root, "m_maskDefinitions")
        .unwrap()
        .iter()
        .zip(arr(&after.root, "m_maskDefinitions").unwrap())
    {
        if field(a, "m_ID").unwrap().string().unwrap() != "UpperBodyReplace_Turn" {
            assert!(a.canonical_eq(&b));
            continue;
        }
        let aa = field(a, "m_primaryWeightList").unwrap();
        let bb = field(&b, "m_primaryWeightList").unwrap();
        let ids = arr(aa, "m_boneIDs").unwrap();
        let a = arr(aa, "m_weights").unwrap();
        let b = arr(bb, "m_weights").unwrap();
        for (j, id) in ids.iter().enumerate() {
            if names().iter().any(|n| id.string().ok() == Some(n)) {
                assert_eq!(number(&b[j]).unwrap(), 0.);
            } else {
                assert!(a[j].canonical_eq(&b[j]));
            }
        }
    }
}
#[test]
fn malformed_authored_samples_cannot_overrun_native_frames() {
    let model = model();
    let (_, binding) = primary::behavior_skeleton(skeleton(), &model).unwrap();
    assert!(primary::clip_with_deltas(
        stop(),
        &binding,
        &model,
        Some(&[vec![[0., 0., 0., 1.]; 7]])
    )
    .is_err());
}

#[test]
fn run_profile_follows_graph_blend_slots_independent_of_clip_root_motion() {
    let graph = data(
        include_bytes!("fixtures/behaviors-r4/haze_abilityset_haspath.vnmgraph_c"),
        "DATA",
    )
    .unwrap();
    let mut profiles = behaviors::RunProfiles::new();
    behaviors::collect_graph_run_profiles(&graph.root, &mut profiles).unwrap();
    let run = data(
        include_bytes!("fixtures/behaviors-r4/weapon_run_n.vnmclip_c"),
        "DATA",
    )
    .unwrap();
    let selected = behaviors::select(
        "models/heroes_staging/haze/clips/out_of_combat_crouch_run_w.vnmclip_c",
        &run.root,
        &profiles,
    )
    .unwrap()
    .unwrap();
    assert_eq!(selected.profile, "run_w");
    assert_eq!(selected.strength, 0.6);
    assert_eq!(
        profiles["models/heroes_staging/haze/clips/out_of_combat_crouch_run_nw.vnmclip_c"],
        "run_nw"
    );
    assert!(behaviors::select(
        "models/heroes_staging/haze/clips/zoomed_crouch_run_w.vnmclip_c",
        &run.root,
        &profiles
    )
    .unwrap()
    .is_none());
    let mut inplace = run.root.clone();
    set(&mut inplace, "m_rootMotion", obj(vec![])).unwrap();
    assert_eq!(
        behaviors::select(
            "models/heroes_staging/haze/clips/weapon_run_n.vnmclip_c",
            &inplace,
            &profiles
        )
        .unwrap()
        .unwrap()
        .profile,
        "run_n"
    );
}

#[test]
fn movement_graph_axis_order_is_read_and_conflicting_clip_slots_are_rejected() {
    let mut graph = data(
        include_bytes!("fixtures/behaviors-r4/haze_abilityset_haspath.vnmgraph_c"),
        "DATA",
    )
    .unwrap();
    let mut expected = behaviors::RunProfiles::new();
    behaviors::collect_graph_run_profiles(&graph.root, &mut expected).unwrap();
    let paths = arr(&graph.root, "m_nodePaths").unwrap();
    let mut nodes = arr(&graph.root, "m_nodes").unwrap();
    for node in &mut nodes {
        if field(node, "_class").unwrap().string().unwrap() != "CNmBlend2DNode::CDefinition" {
            continue;
        }
        let a = field(node, "m_nInputParameterNodeIdx0").unwrap().clone();
        let b = field(node, "m_nInputParameterNodeIdx1").unwrap().clone();
        if paths[a.integer().unwrap() as usize].string().unwrap() != "strafe_speed"
            || paths[b.integer().unwrap() as usize].string().unwrap() != "forward_speed"
        {
            continue;
        }
        set(node, "m_nInputParameterNodeIdx0", b).unwrap();
        set(node, "m_nInputParameterNodeIdx1", a).unwrap();
        let values = arr(node, "m_values")
            .unwrap()
            .iter()
            .map(|v| {
                let pair = v.array().unwrap();
                list(vec![pair[1].clone(), pair[0].clone()])
            })
            .collect();
        set(node, "m_values", list(values)).unwrap();
    }
    set(&mut graph.root, "m_nodes", list(nodes)).unwrap();
    let mut actual = behaviors::RunProfiles::new();
    behaviors::collect_graph_run_profiles(&graph.root, &mut actual).unwrap();
    assert_eq!(expected, actual);
    actual.insert(
        "models/heroes_staging/haze/clips/weapon_run_n.vnmclip_c".into(),
        "run_s".into(),
    );
    assert!(
        behaviors::collect_graph_run_profiles(&graph.root, &mut actual)
            .unwrap_err()
            .to_string()
            .contains("Conflicting")
    );
}
