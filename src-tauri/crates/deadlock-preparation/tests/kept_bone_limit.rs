//! The limit on kept moving-part bones: one number in Rust, Python and
//! TypeScript, and the two messages of the check on the kept groups as a whole.
use deadlock_preparation::{algorithms::{self, Bone}, page_names, secondary_bones::{self as secondary, PhysicsGroup, MAX_KEPT_BONES}};
use deadlock_project::PreparationOptions;
use serde_json::json;

/// Whether the file has exactly this line (either line ending).
fn has_line(file: &str, line: &str) -> bool { file.lines().any(|l| l.trim_end_matches('\r') == line) }

/// Changing the limit is three constants; this test says when one was forgotten.
#[test]
fn the_limit_is_the_same_number_in_rust_python_and_typescript() {
    let script = include_str!("../resources/blender/secondary_bones.py");
    assert!(has_line(script, &format!("MAX_KEPT_BONES = {MAX_KEPT_BONES}")), "secondary_bones.py has another MAX_KEPT_BONES than {MAX_KEPT_BONES}");
    let page = include_str!("../../../../src/preparation/secondary-bones.ts");
    assert!(has_line(page, &format!("export const MAX_KEPT_BONES={MAX_KEPT_BONES};")), "secondary-bones.ts has another MAX_KEPT_BONES than {MAX_KEPT_BONES}");
}

/// The Blender copy of the check says the same words (secondary_bones.py).
#[test]
fn the_messages_are_the_same_in_python() {
    let script = include_str!("../resources/blender/secondary_bones.py");
    for (name, text) in [("SHARED_KEPT_BONES", secondary::shared_bones_template()), ("TOO_MANY_KEPT_BONES", secondary::too_many_bones_template()),
        ("KEPT_GROUP_SIZE", secondary::KEPT_GROUP_SIZE.to_string())] {
        assert!(!text.contains('\''), "{text}");
        assert!(has_line(script, &format!("{name} = '{text}'")), "secondary_bones.py has another {name}: {text}");
    }
    assert!(has_line(script, &format!("LARGEST_NAMED = {}", secondary::LARGEST_NAMED)));
    for text in [secondary::shared_bones_template(), secondary::too_many_bones_template()] {
        assert!(text.contains(page_names::MOVING_PARTS_TAB), "{text}");
        // No other preparation phrase of the review catches them.
        assert!(!text.to_lowercase().contains("stale"), "{text}");
    }
}

fn group(root: &str, bones: &[&str]) -> PhysicsGroup {
    PhysicsGroup { root: root.into(), source_anchor: "head".into(), target_anchor: "head".into(),
        source_bones: bones.iter().map(|b| (*b).into()).collect(), weighted_bones: bones.iter().map(|b| (*b).into()).collect() }
}

#[test]
fn groups_that_share_bones_are_named() {
    let groups = [group("hair_a_01", &["hair_a_01", "hair_a_02"]), group("hair_b_01", &["hair_b_01"]), group("hair_c_01", &["hair_c_01", "hair_a_02"])];
    let message = secondary::kept_bones_problem(&groups).unwrap();
    assert_eq!(message, "The moving parts hair_a_01 and hair_c_01 share bones, so they cannot both be kept. On the Moving parts tab, keep only one of them.");
    assert!(secondary::kept_bones_problem(&groups[..2]).is_none());
    // A bone name that holds braces is written as it is.
    assert_eq!(secondary::shared_bones_message("a{second}", "b"), "The moving parts a{second} and b share bones, so they cannot both be kept. On the Moving parts tab, keep only one of them.");
}

/// Strands of hair on a head, (letter, bones), one bone over the limit in all:
/// the largest first by count, three of equal size (named by root), and a
/// sixth that is too small to be named. Each strand stays under the 512-step
/// walk along one chain.
fn strands() -> [(char, usize); 6] {
    let (f, a, d) = (MAX_KEPT_BONES * 15 / 64, MAX_KEPT_BONES * 25 / 128, MAX_KEPT_BONES * 5 / 32);
    let e = MAX_KEPT_BONES + 1 - f - 3 * a - d;
    assert!(f < 512 && f > a && a > d && d > e && e > 1);
    [('f', f), ('a', a), ('b', a), ('c', a), ('d', d), ('e', e)]
}

/// A made-up rig with one hair strand per entry under the head, and its options.
fn rig(strands: &[(char, usize)]) -> (Vec<Bone>, Vec<String>, Vec<Bone>, PreparationOptions, Vec<String>) {
    let mut bones = vec![json!({"name":"pelvis","parent":null,"position":[0,0,0]}), json!({"name":"head","parent":"pelvis","position":[0,0,5]})];
    let (mut weighted, mut roots) = (vec![], vec![]);
    for (side, &(letter, count)) in strands.iter().enumerate() {
        let mut parent = "head".to_string();
        for at in 1..=count {
            let name = format!("hair_{letter}_{at:03}");
            bones.push(json!({"name":name,"parent":parent,"position":[side as f64 * 0.1, 0.0, 4.9 - at as f64 * 0.01]}));
            weighted.push(name.clone());
            parent = name;
        }
        roots.push(format!("hair_{letter}_001"));
    }
    let source: Vec<Bone> = serde_json::from_value(json!(bones)).unwrap();
    let target: Vec<Bone> = serde_json::from_value(json!([{"name":"pelvis","parent":null,"position":[0,0,0]}, {"name":"head","parent":"pelvis","position":[0,0,5]}])).unwrap();
    let roles = json!({"pelvis":"pelvis","head":"head"});
    let options = serde_json::from_value(json!({"preserve_physics_bones":true,"physics_roots":roots,"source_roles":roles,"target_roles":roles})).unwrap();
    (source, weighted, target, options, roots)
}

#[test]
fn one_bone_over_the_limit_is_refused_with_the_total_and_the_largest_groups() {
    let strands = strands();
    let total: usize = strands.iter().map(|s| s.1).sum();
    assert_eq!(total, MAX_KEPT_BONES + 1);
    let (source, weighted, target, options, roots) = rig(&strands);
    let detected = secondary::candidates(&source, &weighted, &serde_json::from_value(options.values["source_roles"].clone()).unwrap(), &serde_json::from_value(options.values["target_roles"].clone()).unwrap()).unwrap();
    let mut found: Vec<_> = detected.iter().map(|g| g.root.clone()).collect();
    found.sort();
    let mut expected = roots.clone();
    expected.sort();
    assert_eq!(found, expected, "every strand is one detected group");
    let [(_, f), (_, a), _, _, (_, d), _] = strands;
    let (over, limit) = (MAX_KEPT_BONES + 1, MAX_KEPT_BONES);
    let line = format!("The kept moving parts have {over} bones, more than the limit of {limit}. The largest, with their bone counts: hair_f_001 ({f}), hair_a_001 ({a}), hair_b_001 ({a}), hair_c_001 ({a}), hair_d_001 ({d}). On the Moving parts tab, leave some of them out until the total is {limit} or less; a part that is left out follows the body and stays still.");
    let error = secondary::selection(&source, &weighted, &target, &options).unwrap_err().to_string();
    assert!(error.contains(&line), "{error}");
    // The review shows it on the Moving parts tab, as the page's "things to fix" list does.
    let issues = algorithms::review_issues(&target, &source, &weighted, "rigged_humanoid", "rigged_humanoid", &options);
    assert!(issues.iter().any(|i| i.tab == "physics bones" && i.message == line && i.kind.is_none()), "{issues:?}");
}

#[test]
fn exactly_the_limit_is_kept() {
    let mut strands = strands();
    strands[5].1 -= 1;
    let (source, weighted, target, options, _) = rig(&strands);
    let plan = secondary::selection(&source, &weighted, &target, &options).unwrap().unwrap();
    assert_eq!(plan.exported_bones.len(), MAX_KEPT_BONES);
    assert_eq!(plan.groups.len(), strands.len());
}
