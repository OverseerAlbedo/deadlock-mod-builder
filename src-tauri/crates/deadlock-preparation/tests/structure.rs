//! Structural humanoid recognition: rigs without usable anatomical names.
use deadlock_preparation::{algorithms::{self, Bone, ROLES}, automap, structure};
use serde_json::json;
use std::collections::BTreeMap;

/// key, parent, position (Z up, facing -Y, left = +X).
fn layout(toes: bool, a_pose: bool, skirt: bool) -> Vec<(String, Option<String>, [f64; 3])> {
    let mut rows: Vec<(String, Option<String>, [f64; 3])> = vec![
        ("root".into(), None, [0., 0., 0.]),
        ("pelvis".into(), Some("root".into()), [0., 0., 0.95]),
        ("spine".into(), Some("pelvis".into()), [0., 0., 1.05]),
        ("chest".into(), Some("spine".into()), [0., 0., 1.25]),
        ("neck".into(), Some("chest".into()), [0., 0., 1.48]),
        ("head".into(), Some("neck".into()), [0., 0., 1.58]),
        ("hair_0".into(), Some("head".into()), [0., 0.09, 1.74]),
        ("hair_1".into(), Some("hair_0".into()), [0., 0.13, 1.55]),
    ];
    for (side, sign) in [("L", 1.), ("R", -1.)] {
        let arm = |x: f64, z: f64| if a_pose { [sign * (0.17 + (x - 0.17) * 0.7), 0., 1.45 - (x - 0.17) * 0.7] } else { [sign * x, 0., z] };
        rows.push((format!("clavicle_{side}"), Some("chest".into()), [sign * 0.03, 0., 1.44]));
        rows.push((format!("upperarm_{side}"), Some(format!("clavicle_{side}")), arm(0.17, 1.45)));
        rows.push((format!("upperarm_twist_{side}"), Some(format!("upperarm_{side}")), arm(0.31, 1.45)));
        rows.push((format!("forearm_{side}"), Some(format!("upperarm_{side}")), arm(0.45, 1.45)));
        rows.push((format!("hand_{side}"), Some(format!("forearm_{side}")), arm(0.70, 1.45)));
        for (finger, offset) in [("thumb", -0.04), ("index", -0.02), ("middle", 0.), ("pinky", 0.03)] {
            let mut p = arm(0.78, 1.45);
            p[1] = offset;
            rows.push((format!("{finger}_{side}"), Some(format!("hand_{side}")), p));
            let mut q = arm(0.84, 1.45);
            q[1] = offset;
            rows.push((format!("{finger}2_{side}"), Some(format!("{finger}_{side}")), q));
        }
        rows.push((format!("thigh_{side}"), Some("pelvis".into()), [sign * 0.10, 0., 0.93]));
        rows.push((format!("shin_{side}"), Some(format!("thigh_{side}")), [sign * 0.10, 0., 0.50]));
        rows.push((format!("foot_{side}"), Some(format!("shin_{side}")), [sign * 0.10, 0., 0.09]));
        if toes {
            rows.push((format!("toe_{side}"), Some(format!("foot_{side}")), [sign * 0.10, -0.12, 0.03]));
        }
    }
    if skirt {
        for (i, (x, y)) in [(0.12, -0.1), (-0.12, -0.1), (0.12, 0.1), (-0.12, 0.1)].into_iter().enumerate() {
            rows.push((format!("skirt_{i}_0"), Some("pelvis".into()), [x, y, 0.9]));
            rows.push((format!("skirt_{i}_1"), Some(format!("skirt_{i}_0")), [x * 1.4, y * 1.4, 0.45]));
            rows.push((format!("skirt_{i}_2"), Some(format!("skirt_{i}_1")), [x * 1.6, y * 1.6, 0.12]));
        }
    }
    rows
}

fn rig(names: impl Fn(&str, usize) -> String, toes: bool, a_pose: bool, skirt: bool, turn: fn([f64; 3]) -> [f64; 3]) -> (Vec<Bone>, Vec<String>, BTreeMap<String, String>) {
    let rows = layout(toes, a_pose, skirt);
    let renamed: BTreeMap<String, String> = rows.iter().enumerate().map(|(i, r)| (r.0.clone(), names(&r.0, i))).collect();
    let bones: Vec<Bone> = rows
        .iter()
        .map(|(key, parent, p)| {
            serde_json::from_value(json!({
                "name": renamed[key], "parent": parent.as_ref().map(|p| renamed[p].clone()),
                "position": turn(*p), "deform": key != "root",
                "skin": {"vertices": if key == "head" { 900 } else { 120 }},
            }))
            .unwrap()
        })
        .collect();
    let weighted = rows.iter().filter(|r| r.0 != "root").map(|r| renamed[&r.0].clone()).collect();
    let expected = ROLES.iter().map(|role| (role.to_string(), renamed[*role].clone())).collect();
    (bones, weighted, expected)
}
fn same(p: [f64; 3]) -> [f64; 3] {
    p
}
fn y_up(p: [f64; 3]) -> [f64; 3] {
    // A Y-up export opened without axis conversion: (x, y, z) -> (x, z, -y)
    [p[0], p[2], -p[1]]
}
fn generic(_: &str, i: usize) -> String {
    if i == 0 { "Bone".into() } else { format!("Bone.{i:03}") }
}

#[test]
fn generic_names_are_recognized_from_structure() {
    for (toes, a_pose, skirt) in [(true, false, false), (true, true, false), (true, false, true), (true, true, true)] {
        let (bones, weighted, expected) = rig(generic, toes, a_pose, skirt, same);
        let found = automap::roles(&bones, &weighted, &Default::default()).unwrap();
        assert!(found.errors.is_empty(), "{toes} {a_pose} {skirt}: {:?} / {}", found.errors, structure::explain(&bones, &weighted));
        assert_eq!(found.roles, expected, "{toes} {a_pose} {skirt}");
        assert!(found.evidence.contains("toes"), "{}", found.evidence);
    }
}

#[test]
fn side_tokens_override_toe_direction_and_user_choices_still_win() {
    // Names label the +X limbs as right. Names outrank the toe direction.
    let names = |key: &str, i: usize| {
        let side = if key.ends_with("_L") { ".R" } else if key.ends_with("_R") { ".L" } else { "" };
        format!("J{i}{side}")
    };
    let (bones, weighted, expected) = rig(names, true, false, false, same);
    let found = automap::roles(&bones, &weighted, &Default::default()).unwrap();
    assert!(found.errors.is_empty(), "{:?}", found.errors);
    // Sides follow the names (mirrored relative to the layout).
    assert_eq!(found.roles["hand_L"], expected["hand_R"]);
    assert!(found.evidence.contains("names"), "{}", found.evidence);
    let overrides = BTreeMap::from([("head".to_string(), expected["neck"].clone())]);
    let forced = automap::roles(&bones, &weighted, &overrides).unwrap();
    assert_eq!(forced.roles["head"], expected["neck"]);
}

#[test]
fn missing_toes_fall_back_to_default_facing_with_a_note() {
    let (bones, weighted, expected) = rig(generic, false, false, false, same);
    let found = automap::roles(&bones, &weighted, &Default::default()).unwrap();
    assert!(found.errors.is_empty(), "{:?}", found.errors);
    assert_eq!(found.roles, expected);
    assert!(found.evidence.contains("default facing"), "{}", found.evidence);
}

#[test]
fn lying_sources_get_an_upright_rotation() {
    let (bones, weighted, _) = rig(generic, true, false, false, y_up);
    let named = automap::roles(&bones, &weighted, &Default::default()).unwrap();
    assert!(!named.errors.is_empty());
    let turn = structure::orientation(&bones, &weighted, &named.roles).expect("rotation");
    assert_eq!(turn.rotation, [90., 0., 0.]);
    // Named rigs use the same rotation search.
    let (bones, weighted, expected) = rig(|key, _| key.replace("_L", ".L").replace("_R", ".R"), true, false, false, y_up);
    let turn = structure::orientation(&bones, &weighted, &expected).expect("rotation");
    assert_eq!(turn.rotation, [90., 0., 0.]);
    // Already upright: nothing to do.
    let (bones, weighted, expected) = rig(generic, true, false, false, same);
    assert!(structure::orientation(&bones, &weighted, &expected).is_none());
}

#[test]
fn real_mmd_rigs_renamed_to_generic_names_keep_their_body_roles() {
    for fixture in [include_str!("fixtures/daki-rig.json"), include_str!("fixtures/tangtang-rig.json"), include_str!("fixtures/parallel-leg-rig.json")] {
        let value: serde_json::Value = serde_json::from_str(fixture).unwrap();
        let bones: Vec<Bone> = serde_json::from_value(value["source"].clone()).unwrap();
        let weighted: Vec<String> = serde_json::from_value(value["weighted"].clone()).unwrap();
        let named = automap::roles(&bones, &weighted, &Default::default()).unwrap();
        assert!(named.errors.is_empty());
        let rename: BTreeMap<String, String> = bones.iter().enumerate().map(|(i, b)| (b.name.clone(), format!("node{i}"))).collect();
        let generic: Vec<Bone> = bones
            .iter()
            .map(|b| {
                let mut copy = b.clone();
                copy.name = rename[&b.name].clone();
                copy.parent = b.parent.as_ref().map(|p| rename[p].clone());
                copy
            })
            .collect();
        let weighted: Vec<String> = weighted.iter().map(|n| rename[n].clone()).collect();
        let found = automap::roles(&generic, &weighted, &Default::default()).unwrap();
        assert!(found.errors.is_empty(), "{:?} / {}", found.errors, structure::explain(&generic, &weighted));
        // Limbs and sides must agree with the named recognition. Torso joints
        // may land on an equivalent neighbouring spine joint.
        for role in ROLES.iter().filter(|r| r.contains('_')) {
            let expected = &rename[&named.roles[*role]];
            let actual = &found.roles[*role];
            let distance = |a: &str, b: &str| {
                let p = generic.iter().find(|x| x.name == a).unwrap().position;
                let q = generic.iter().find(|x| x.name == b).unwrap().position;
                (0..3).map(|k| (p[k] - q[k]).powi(2)).sum::<f64>().sqrt()
            };
            assert!(actual == expected || distance(actual, expected) < 0.6, "{role}: {actual} vs {expected}");
        }
        assert!(algorithms::identify(&generic, &found.roles).unwrap().errors.is_empty());
    }
}
