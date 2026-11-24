//! Body-role guess on a rig whose body names carry a shared prefix, whose root
//! carries the hips, whose face is weighted to the neck-named joint, and which
//! has two long mirrored decoration chains hanging from the root to the floor.
use deadlock_preparation::{
    algorithms::{self, Bone, ROLES},
    automap,
};
use deadlock_project::PreparationOptions;
use serde_json::json;
use std::collections::BTreeMap;

struct Row {
    name: String,
    parent: Option<String>,
    position: [f64; 3],
    skin: f64,
}
fn row(name: &str, parent: Option<&str>, position: [f64; 3], skin: f64) -> Row {
    Row { name: name.into(), parent: parent.map(str::to_string), position, skin }
}

/// Z up, facing -Y, left = +X. `skin` is each bone's weight sum (0 = unweighted).
fn rows(rope_skin: f64) -> Vec<Row> {
    let mut rows = vec![
        row("Root_Body", None, [0., 0., 0.95], 1500.),
        row("Abc_Spine", Some("Root_Body"), [0., 0., 1.03], 900.),
        row("Abc_Spine_1", Some("Abc_Spine"), [0., 0., 1.18], 1200.),
        row("Abc_Spine_2", Some("Abc_Spine_1"), [0., 0.01, 1.32], 300.),
        // The face is weighted to the neck-named joint, the hair cap to the head-named one.
        row("Abc_Neck", Some("Abc_Spine_2"), [0., 0.01, 1.38], 1800.),
        row("Abc_Head", Some("Abc_Neck"), [0., 0.01, 1.50], 1500.),
        row("Ear_L", Some("Abc_Head"), [0.05, 0., 1.58], 200.),
        row("Ear_1_L", Some("Ear_L"), [0.06, 0., 1.62], 150.),
        row("Ear_R", Some("Abc_Head"), [-0.05, 0., 1.58], 200.),
        row("Ear_1_R", Some("Ear_R"), [-0.06, 0., 1.62], 150.),
    ];
    for (k, x) in [-0.06, 0., 0.06].into_iter().enumerate() {
        rows.push(row(&format!("Strand_{k}"), Some("Abc_Head"), [x, 0.05, 1.50], 30.));
        rows.push(row(&format!("Strand_{k}_1"), Some(&format!("Strand_{k}")), [x, 0.07, 1.38], 30.));
    }
    for (side, sign) in [("L", 1.), ("R", -1.)] {
        let n = |base: &str| format!("Abc_{base}_{side}");
        rows.push(row(&n("Shoulder"), Some("Abc_Spine_1"), [sign * 0.05, 0., 1.30], 200.));
        rows.push(row(&n("Arm"), Some(&n("Shoulder")), [sign * 0.12, 0., 1.30], 700.));
        rows.push(row(&n("ForeArm"), Some(&n("Arm")), [sign * 0.38, 0., 1.30], 600.));
        rows.push(row(&n("Hand"), Some(&n("ForeArm")), [sign * 0.62, 0., 1.30], 300.));
        rows.push(row(&n("Finger"), Some(&n("Hand")), [sign * 0.70, 0., 1.30], 80.));
        rows.push(row(&n("UpLeg"), Some("Root_Body"), [sign * 0.09, 0., 0.85], 900.));
        // One shin is misspelled, as real rigs sometimes are.
        let shin = if side == "L" { n("Leg") } else { n("Lag") };
        rows.push(row(&shin, Some(&n("UpLeg")), [sign * 0.09, 0.01, 0.58], 850.));
        rows.push(row(&n("Foot"), Some(&shin), [sign * 0.09, 0.05, 0.09], 500.));
        rows.push(row(&n("ToeBase"), Some(&n("Foot")), [sign * 0.09, -0.10, 0.02], 0.));
        // A floor-length decoration rope hanging from the root behind the hips.
        let mut parent = "Root_Body".to_string();
        for k in 0..11 {
            let t = k as f64 / 10.;
            let name = format!("Rope_{k}_{side}");
            let skin = if k >= 8 { rope_skin * 6. } else { rope_skin };
            rows.push(row(&name, Some(&parent), [sign * (0.05 + 0.25 * t), 0.06 + 0.2 * t, 0.97 - 0.90 * t], skin));
            parent = name;
        }
    }
    // A centred tail going backwards.
    let mut parent = "Root_Body".to_string();
    for k in 0..8 {
        let name = format!("Tail_{k}");
        rows.push(row(&name, Some(&parent), [0., 0.06 + 0.08 * k as f64, 1.0 - 0.05 * k as f64], 50.));
        parent = name;
    }
    rows
}

fn rig(rope_skin: f64, rename: impl Fn(&str) -> String) -> (Vec<Bone>, Vec<String>) {
    let rows = rows(rope_skin);
    let bones = rows
        .iter()
        .map(|r| {
            serde_json::from_value(json!({
                "name": rename(&r.name), "parent": r.parent.as_deref().map(&rename), "position": r.position,
                "deform": true, "skin": {"vertices": (r.skin * 1.3).round(), "weight_sum": r.skin},
            }))
            .unwrap()
        })
        .collect();
    let weighted = rows.iter().filter(|r| r.skin > 0.).map(|r| rename(&r.name)).collect();
    (bones, weighted)
}

fn expected() -> BTreeMap<String, String> {
    [
        ("pelvis", "Root_Body"), ("spine", "Abc_Spine"), ("chest", "Abc_Spine_1"), ("neck", "Abc_Spine_2"),
        ("head", "Abc_Neck"), ("upperarm_L", "Abc_Arm_L"), ("forearm_L", "Abc_ForeArm_L"), ("hand_L", "Abc_Hand_L"),
        ("thigh_L", "Abc_UpLeg_L"), ("shin_L", "Abc_Leg_L"), ("foot_L", "Abc_Foot_L"), ("upperarm_R", "Abc_Arm_R"),
        ("forearm_R", "Abc_ForeArm_R"), ("hand_R", "Abc_Hand_R"), ("thigh_R", "Abc_UpLeg_R"), ("shin_R", "Abc_Lag_R"),
        ("foot_R", "Abc_Foot_R"),
    ]
    .into_iter()
    .map(|(a, b)| (a.to_string(), b.to_string()))
    .collect()
}

fn target() -> Vec<Bone> {
    let rows: Vec<(&str, Option<&str>, [f64; 3])> = vec![
        ("pelvis", None, [0., 0., 0.95]),
        ("spine_1", Some("pelvis"), [0., 0., 1.05]),
        ("spine_3", Some("spine_1"), [0., 0., 1.25]),
        ("neck_0", Some("spine_3"), [0., 0., 1.45]),
        ("head", Some("neck_0"), [0., 0., 1.55]),
        ("arm_upper_L", Some("spine_3"), [0.17, 0., 1.42]),
        ("arm_lower_L", Some("arm_upper_L"), [0.42, 0., 1.42]),
        ("hand_L", Some("arm_lower_L"), [0.66, 0., 1.42]),
        ("leg_upper_L", Some("pelvis"), [0.1, 0., 0.9]),
        ("leg_lower_L", Some("leg_upper_L"), [0.1, 0., 0.5]),
        ("ankle_L", Some("leg_lower_L"), [0.1, 0., 0.08]),
        ("arm_upper_R", Some("spine_3"), [-0.17, 0., 1.42]),
        ("arm_lower_R", Some("arm_upper_R"), [-0.42, 0., 1.42]),
        ("hand_R", Some("arm_lower_R"), [-0.66, 0., 1.42]),
        ("leg_upper_R", Some("pelvis"), [-0.1, 0., 0.9]),
        ("leg_lower_R", Some("leg_upper_R"), [-0.1, 0., 0.5]),
        ("ankle_R", Some("leg_lower_R"), [-0.1, 0., 0.08]),
    ];
    rows.into_iter()
        .map(|(n, p, pos)| serde_json::from_value(json!({"name": n, "parent": p, "position": pos})).unwrap())
        .collect()
}

#[test]
fn prefixed_names_skin_carrying_legs_and_the_face_bearing_head_are_found() {
    let (bones, weighted) = rig(40., str::to_string);
    let result = automap::suggest(&bones, &weighted, &target(), &PreparationOptions::default()).unwrap();
    let source = result.source.as_ref().unwrap();
    assert!(source.errors.is_empty(), "{:?}", source.errors);
    assert_eq!(source.roles, expected(), "{}", source.evidence);
    // The hair-cap joint above the face-carrying head follows the hero's head too.
    assert_eq!(result.mapping["Abc_Head"], "head");
    assert_eq!(result.mapping["Abc_Neck"], "head");
    assert_eq!(result.mapping["Abc_Spine_2"], "neck_0");
    let warnings = result.assistance.unwrap().warnings;
    for phrase in ["prefix \"Abc_\"", "shin R is Abc_Lag_R", "The head is Abc_Neck"] {
        assert!(warnings.iter().any(|w| w.starts_with("Body guess: ") && w.contains(phrase)), "{phrase}: {warnings:?}");
    }
}

#[test]
fn legs_are_chosen_by_the_skin_they_carry_without_names() {
    let (named, _) = rig(40., str::to_string);
    let index: BTreeMap<String, usize> = named.iter().enumerate().map(|(i, b)| (b.name.clone(), i)).collect();
    let rename = |n: &str| format!("node{}", index[n]);
    let (bones, weighted) = rig(40., rename);
    let found = automap::roles(&bones, &weighted, &Default::default()).unwrap();
    assert!(found.errors.is_empty(), "{:?}", found.errors);
    assert!(found.evidence.starts_with("Skeleton structure"), "{}", found.evidence);
    for (role, name) in expected() {
        assert_eq!(found.roles[&role], rename(&name), "{role}: {}", found.evidence);
    }
}

#[test]
fn ropes_carrying_as_much_skin_as_the_legs_are_reported_as_doubtful_or_lose() {
    // With equal skin the ropes are no longer told apart by their load; the
    // guess must still be a valid body, and a close second pair is reported.
    let (named, _) = rig(800., str::to_string);
    let index: BTreeMap<String, usize> = named.iter().enumerate().map(|(i, b)| (b.name.clone(), i)).collect();
    let rename = |n: &str| format!("node{}", index[n]);
    let (bones, weighted) = rig(800., rename);
    let found = automap::roles(&bones, &weighted, &Default::default()).unwrap();
    assert!(found.errors.is_empty(), "{:?}", found.errors);
    let legs_right = found.roles["thigh_L"] == rename("Abc_UpLeg_L");
    assert!(legs_right || found.notes.iter().any(|n| n.starts_with("Two pairs of chains could be the legs")), "{:?}", found.notes);
}

#[test]
fn user_choices_for_the_head_and_neck_win_over_the_skin_rule() {
    let (bones, weighted) = rig(40., str::to_string);
    let options: PreparationOptions =
        serde_json::from_value(json!({"source_roles": {"head": "Abc_Head", "neck": "Abc_Neck"}})).unwrap();
    let result = automap::suggest(&bones, &weighted, &target(), &options).unwrap();
    let source = result.source.unwrap();
    assert_eq!(source.roles["head"], "Abc_Head");
    assert_eq!(source.roles["neck"], "Abc_Neck");
    assert!(source.also.is_empty());
}

#[test]
fn ordinary_heads_keep_their_roles() {
    // The same rig with the face on the head-named joint: nothing moves.
    let (mut bones, weighted) = rig(40., str::to_string);
    for b in &mut bones {
        if b.name == "Abc_Neck" {
            b.details.insert("skin".into(), json!({"vertices": 300, "weight_sum": 250.}));
        }
    }
    let found = automap::roles(&bones, &weighted, &Default::default()).unwrap();
    assert_eq!(found.roles["head"], "Abc_Head");
    assert!(found.also.is_empty());
}

#[test]
fn a_prefix_needs_many_proven_roles_and_never_matches_helpers() {
    let bone = |name: &str| -> Bone { serde_json::from_value(json!({"name": name, "parent": null, "position": [0., 0., 0.]})).unwrap() };
    let few: Vec<Bone> = ["Abc_Spine", "Abc_Neck", "Abc_Head", "Abc_Hand_L"].map(bone).to_vec();
    assert!(algorithms::prefixed_roles(&few, &[]).is_none());
    let mut many: Vec<Bone> = ROLES.iter().map(|r| bone(&format!("Abc_{r}"))).collect();
    many.extend(["Abc_ArmTwist_L", "Abc_IK_Foot_L", "Abc_UpLeg_Helper_R"].map(bone));
    let (prefix, roles) = algorithms::prefixed_roles(&many, &[]).unwrap();
    assert_eq!(prefix, "abc_");
    assert!(roles.values().all(|n| !n.contains("Twist") && !n.contains("IK") && !n.contains("Helper")), "{roles:?}");
    assert_eq!(roles["foot_L"], "Abc_foot_L");
}

#[test]
fn an_unweighted_control_family_never_wins_the_prefix() {
    // Generic skin bones next to unweighted controls named for every body part.
    let bone = |name: &str| -> Bone { serde_json::from_value(json!({"name": name, "parent": null, "position": [0., 0., 0.]})).unwrap() };
    let mut bones: Vec<Bone> = ROLES.iter().map(|r| bone(&format!("Ctrl_{r}"))).collect();
    bones.extend(ROLES.iter().map(|r| bone(&format!("IK_{r}"))));
    bones.extend((0..17).map(|i| bone(&format!("Bone.{i:03}"))));
    let weighted: Vec<String> = (0..17).map(|i| format!("Bone.{i:03}")).collect();
    assert!(algorithms::prefixed_roles(&bones, &weighted).is_none());
}

#[test]
fn named_legs_win_over_ropes_that_carry_as_much_skin() {
    // The skin no longer tells ropes from legs; the names behind the prefix do.
    let (bones, weighted) = rig(800., str::to_string);
    let found = automap::roles(&bones, &weighted, &Default::default()).unwrap();
    assert!(found.errors.is_empty(), "{:?}", found.errors);
    for role in ["thigh_L", "shin_L", "foot_L", "thigh_R", "shin_R", "foot_R"] {
        assert_eq!(found.roles[role], expected()[role], "{role}: {}", found.evidence);
    }
}

#[test]
fn a_light_head_bone_with_facial_bones_keeps_the_head() {
    // The head bone carries only the scalp; jaw and eye bones below it carry
    // the face. The neck carries more than the head bone itself, but far less
    // than the head with its facial bones.
    let (mut bones, mut weighted) = rig(40., str::to_string);
    for b in &mut bones {
        let skin = match b.name.as_str() {
            "Abc_Head" => Some(300.),
            "Abc_Neck" => Some(400.),
            _ => None,
        };
        if let Some(skin) = skin {
            b.details.insert("skin".into(), json!({"vertices": skin, "weight_sum": skin}));
        }
    }
    for (name, position) in [("Jaw", [0., -0.04, 1.47]), ("Eye_L", [0.03, -0.08, 1.53]), ("Eye_R", [-0.03, -0.08, 1.53])] {
        bones.push(
            serde_json::from_value(json!({"name": name, "parent": "Abc_Head", "position": position, "deform": true,
                "skin": {"vertices": 1500, "weight_sum": 1200.}}))
            .unwrap(),
        );
        weighted.push(name.into());
    }
    let found = automap::roles(&bones, &weighted, &Default::default()).unwrap();
    assert_eq!(found.roles["head"], "Abc_Head", "{}", found.evidence);
    assert!(found.also.is_empty());
}
