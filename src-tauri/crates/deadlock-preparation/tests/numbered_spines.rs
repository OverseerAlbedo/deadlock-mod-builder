//! Rigs whose spine bones are numbered (`Spine`, `Spine1`, often `Spine2`): the
//! name aliases read both `Spine` and `Spine1` as the spine, so the plain name
//! pass proved no spine. One ancestor chain of such bones maps lowest to
//! highest: the lowest is the spine, the highest the chest (as the rest-rig
//! recovery in Blender reads them), and no two bones claim one role.
use deadlock_preparation::{algorithms::Bone, automap};
use deadlock_project::PreparationOptions;
use serde_json::json;
use std::collections::BTreeMap;

/// Z up, facing -Y, left = +X. `spines` are the numbered spine bones from the
/// hips up; the neck hangs from the last one, the shoulders from the one below
/// the neck. `extra` bones are added as (name, parent, position).
fn rig(spines: &[&str], extra: &[(&str, &str, [f64; 3])]) -> (Vec<Bone>, Vec<String>) {
    let mut rows: Vec<(String, Option<String>, [f64; 3])> = vec![("Hips".into(), None, [0., 0., 0.95])];
    let mut parent = "Hips".to_string();
    for (k, name) in spines.iter().enumerate() {
        rows.push((name.to_string(), Some(parent.clone()), [0., 0., 1.05 + 0.3 * (k as f64 + 1.) / spines.len() as f64]));
        parent = name.to_string();
    }
    let top = parent.clone();
    rows.push(("Neck".into(), Some(top.clone()), [0., 0.01, 1.45]));
    rows.push(("Head".into(), Some("Neck".into()), [0., 0.01, 1.58]));
    for (side, sign) in [("Left", 1.), ("Right", -1.)] {
        let n = |base: &str| format!("{side}{base}");
        rows.push((n("Shoulder"), Some(top.clone()), [sign * 0.05, 0., 1.40]));
        rows.push((n("Arm"), Some(n("Shoulder")), [sign * 0.14, 0., 1.40]));
        rows.push((n("ForeArm"), Some(n("Arm")), [sign * 0.40, 0., 1.40]));
        rows.push((n("Hand"), Some(n("ForeArm")), [sign * 0.64, 0., 1.40]));
        rows.push((n("UpLeg"), Some("Hips".into()), [sign * 0.09, 0., 0.90]));
        rows.push((n("Leg"), Some(n("UpLeg")), [sign * 0.09, 0.01, 0.50]));
        rows.push((n("Foot"), Some(n("Leg")), [sign * 0.09, 0.03, 0.08]));
        rows.push((n("ToeBase"), Some(n("Foot")), [sign * 0.09, -0.10, 0.02]));
    }
    for (name, parent, position) in extra {
        rows.push((name.to_string(), Some(parent.to_string()), *position));
    }
    let bones = rows
        .iter()
        .map(|(name, parent, position)| {
            serde_json::from_value(json!({"name": name, "parent": parent, "position": position, "deform": true,
                "skin": {"vertices": 100, "weight_sum": 80.}}))
            .unwrap()
        })
        .collect();
    (bones, rows.iter().map(|r| r.0.clone()).collect())
}

fn target() -> Vec<Bone> {
    let rows: Vec<(&str, Option<&str>, [f64; 3])> = vec![
        ("pelvis", None, [0., 0., 0.95]),
        ("spine_1", Some("pelvis"), [0., 0., 1.05]),
        ("spine_2", Some("spine_1"), [0., 0., 1.18]),
        ("spine_3", Some("spine_2"), [0., 0., 1.30]),
        ("neck_0", Some("spine_3"), [0., 0., 1.45]),
        ("head", Some("neck_0"), [0., 0., 1.58]),
        ("arm_upper_L", Some("spine_3"), [0.14, 0., 1.40]),
        ("arm_lower_L", Some("arm_upper_L"), [0.40, 0., 1.40]),
        ("hand_L", Some("arm_lower_L"), [0.64, 0., 1.40]),
        ("leg_upper_L", Some("pelvis"), [0.09, 0., 0.9]),
        ("leg_lower_L", Some("leg_upper_L"), [0.09, 0., 0.5]),
        ("ankle_L", Some("leg_lower_L"), [0.09, 0., 0.08]),
        ("arm_upper_R", Some("spine_3"), [-0.14, 0., 1.40]),
        ("arm_lower_R", Some("arm_upper_R"), [-0.40, 0., 1.40]),
        ("hand_R", Some("arm_lower_R"), [-0.64, 0., 1.40]),
        ("leg_upper_R", Some("pelvis"), [-0.09, 0., 0.9]),
        ("leg_lower_R", Some("leg_upper_R"), [-0.09, 0., 0.5]),
        ("ankle_R", Some("leg_lower_R"), [-0.09, 0., 0.08]),
    ];
    rows.into_iter()
        .map(|(n, p, pos)| serde_json::from_value(json!({"name": n, "parent": p, "position": pos})).unwrap())
        .collect()
}

fn roles_of(spines: &[&str], extra: &[(&str, &str, [f64; 3])]) -> deadlock_preparation::algorithms::Identification {
    let (bones, weighted) = rig(spines, extra);
    automap::roles(&bones, &weighted, &BTreeMap::new()).unwrap()
}

#[test]
fn a_numbered_spine_chain_maps_lowest_to_spine_and_highest_to_chest_by_name() {
    for (spines, chest) in [
        (&["Spine", "Spine1", "Spine2"][..], "Spine2"),
        (&["Spine", "Spine1"][..], "Spine1"),
        (&["Spine", "Spine1", "Spine2", "Spine3"][..], "Spine3"),
        (&["Spine01", "Spine02", "Spine03"][..], "Spine03"),
    ] {
        let found = roles_of(spines, &[]);
        assert!(found.errors.is_empty(), "{spines:?}: {:?}", found.errors);
        assert!(found.evidence.starts_with("Names"), "{spines:?}: {}", found.evidence);
        assert_eq!(found.roles["spine"], spines[0], "{spines:?}");
        assert_eq!(found.roles["chest"], chest, "{spines:?}");
        assert_eq!(found.roles["pelvis"], "Hips");
        assert_eq!(found.roles["neck"], "Neck");
        // No bone holds two roles.
        let mut held: Vec<&String> = found.roles.values().collect();
        held.sort();
        held.dedup();
        assert_eq!(held.len(), found.roles.len(), "{spines:?}");
    }
}

/// Two floor-length decoration ropes hanging from the hips beside the legs.
fn ropes() -> Vec<(String, String, [f64; 3])> {
    let mut out = vec![];
    for (side, sign) in [("L", 1.), ("R", -1.)] {
        let mut parent = "Hips".to_string();
        for k in 0..11 {
            let t = k as f64 / 10.;
            let name = format!("Rope_{k}_{side}");
            out.push((name.clone(), parent.clone(), [sign * (0.05 + 0.25 * t), 0.06 + 0.2 * t, 0.97 - 0.90 * t]));
            parent = name;
        }
    }
    out
}

#[test]
fn names_decide_the_body_when_ropes_look_like_legs() {
    let ropes = ropes();
    let extra: Vec<(&str, &str, [f64; 3])> = ropes.iter().map(|(n, p, x)| (n.as_str(), p.as_str(), *x)).collect();
    let found = roles_of(&["Spine", "Spine1", "Spine2"], &extra);
    assert!(found.errors.is_empty(), "{:?}", found.errors);
    assert!(found.evidence.starts_with("Names"), "{}", found.evidence);
    assert_eq!(found.roles["thigh_L"], "LeftUpLeg");
    assert_eq!(found.roles["foot_R"], "RightFoot");
}

#[test]
fn without_skin_weights_the_names_alone_find_the_spine() {
    // The skeleton-structure guess needs skin weights; before the chain rule
    // such a rig was left asking for the spine.
    let (bones, _) = rig(&["Spine", "Spine1", "Spine2"], &[]);
    let found = automap::roles(&bones, &[], &BTreeMap::new()).unwrap();
    assert!(found.errors.is_empty(), "{:?}", found.errors);
    assert_eq!((found.roles["spine"].as_str(), found.roles["chest"].as_str()), ("Spine", "Spine2"));
}

#[test]
fn the_middle_spine_bones_map_as_they_did_before() {
    // The spine and chest go to the hero's spine and chest; the bones between
    // them keep the mapping the structure guess gave them before the chain rule
    // (they move with the hero bone of the spine below them).
    let mut options = PreparationOptions::default();
    options.values.insert("complete_mapping".into(), true.into());
    for (spines, expected) in [
        (&["Spine", "Spine1", "Spine2"][..], &["spine_1", "spine_1", "spine_3"][..]),
        (&["Spine", "Spine1", "Spine2", "Spine3"][..], &["spine_1", "spine_1", "spine_1", "spine_3"][..]),
    ] {
        let (bones, weighted) = rig(spines, &[]);
        let result = automap::suggest(&bones, &weighted, &target(), &options).unwrap();
        assert!(result.source.as_ref().unwrap().errors.is_empty());
        let mapped: Vec<&str> = spines.iter().map(|s| result.mapping.get(*s).map_or("", String::as_str)).collect();
        assert_eq!(mapped, expected, "{spines:?}");
    }
}

#[test]
fn spine_bones_off_one_chain_are_not_ordered_by_name() {
    // A second spine-named bone on a branch beside the chain (not above or
    // below it): the names prove nothing about which one is the spine. Without
    // skin weights the structure guess cannot run either, so the spine stays
    // for the user to choose.
    let (bones, _) = rig(&["Spine", "Spine2"], &[("Spine1", "Hips", [0.05, 0.08, 1.1])]);
    let found = automap::roles(&bones, &[], &BTreeMap::new()).unwrap();
    assert!(!found.roles.contains_key("spine"), "{:?}", found.roles);
    assert!(found.missing.contains(&"spine".to_string()), "{:?}", found.missing);
}

#[test]
fn user_choices_and_unambiguous_names_are_untouched() {
    // One plain spine name: the name pass already decided, nothing changes.
    let found = roles_of(&["Spine", "Chest"], &[]);
    assert_eq!(found.roles["spine"], "Spine");
    assert_eq!(found.roles["chest"], "Chest");
    // A user's spine choice wins over the chain rule.
    let (bones, weighted) = rig(&["Spine", "Spine1", "Spine2"], &[]);
    let chosen: BTreeMap<String, String> = [("spine".to_string(), "Spine1".to_string())].into();
    let found = automap::roles(&bones, &weighted, &chosen).unwrap();
    assert_eq!(found.roles["spine"], "Spine1");
}
