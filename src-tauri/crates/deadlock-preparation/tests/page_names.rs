//! Messages that name the Prepare model page's tabs and buttons, the grouped
//! stale-match issue, and what a refresh of the suggestions keeps.
use deadlock_preparation::{algorithms::{self, Bone}, automap, following, page_names, secondary_bones};
use deadlock_project::PreparationOptions;
use serde_json::{json, Value};

#[test]
fn messages_name_the_labels_the_page_shows() {
    let page = include_str!("../../../../src/preparation/pageNames.ts");
    for label in [page_names::MOVING_PARTS_TAB, page_names::JOINT_MATCHES_TAB, page_names::GUN_TAB, page_names::REFRESH_MATCHES, page_names::REFRESH_THE_MATCHES, page_names::USE_DETECTED_GROUPS, page_names::LOAD_AND_CHECK] {
        assert!(page.contains(&format!("'{label}'")), "pageNames.ts has no label {label}");
    }
    // The issue kinds the page offers a one-click fix for.
    let models = include_str!("../../../../src/preparation/models.ts");
    let kinds = format!("export type IssueKind='{}'|'{}';", following::STALE_KIND, secondary_bones::STALE_GROUPS_KIND);
    assert!(models.contains(&kinds), "models.ts has no {kinds}");
    let groups = secondary_bones::stale_groups_message();
    for label in [page_names::MOVING_PARTS_TAB, page_names::USE_DETECTED_GROUPS, page_names::LOAD_AND_CHECK] { assert!(groups.contains(label), "{groups}"); }
    // The Blender copy of the moving-part selection raises the same text.
    let script = include_str!("../resources/blender/secondary_bones.py");
    assert!(script.contains(&format!("raise ValueError('{groups}')")), "secondary_bones.py does not raise: {groups}");
    let one = following::stale_message(&["hair_a".into()]);
    assert_eq!(one, "The body-following match for hair_a no longer fits your body roles. On the Joint matches tab, press Refresh suggestions using my roles, or choose a target for hair_a.");
    let many = following::stale_message(&(0..7).map(|i| format!("b{i}")).collect::<Vec<_>>());
    assert_eq!(many, "7 body-following matches no longer fit your body roles (b0, b1, b2, b3, b4 and 2 more). On the Joint matches tab, press Refresh suggestions using my roles, or choose targets for these bones.");
    // Neither message is caught by another preparation phrase of the review.
    for message in [&groups, &one, &many] { assert!(!message.to_lowercase().contains("stale"), "{message}"); }
}

fn rig() -> (Vec<Bone>, Vec<String>, Vec<Bone>) {
    let v: Value = serde_json::from_str(include_str!("fixtures/parallel-leg-rig.json")).unwrap();
    (serde_json::from_value(v["source"].clone()).unwrap(),
     serde_json::from_value(v["weighted"].clone()).unwrap(),
     serde_json::from_value(v["target"].clone()).unwrap())
}
fn options(value: Value) -> PreparationOptions { serde_json::from_value(value).unwrap() }

/// A role moved by hand leaves the body-following matches that hung from the old
/// joint stale: the review lists them as ONE issue with its bones and kind, and a
/// refresh of the suggestions with the same roles repairs them while keeping the
/// direct matches the user chose.
#[test]
fn stale_matches_are_one_issue_and_a_refresh_keeps_direct_choices() {
    let (source, weighted, target) = rig();
    let first = automap::suggest(&source, &weighted, &target, &options(json!({"complete_mapping": true}))).unwrap();
    let mut roles = first.source.clone().unwrap().roles;
    let target_roles = first.target.roles.clone();
    let mut mapping = first.mapping.clone();
    let anchors = first.assistance.clone().unwrap().anchors;
    let head = roles["head"].clone();
    // A direct match made by hand, different from what the suggestions would give.
    let direct = "hair_L_d_02_jnt";
    let followers: Vec<_> = anchors.iter().filter(|(n, a)| **a == head && n.as_str() != direct).map(|(n, _)| n.clone()).collect();
    assert!(followers.len() > 1, "the rig has bones following the head");
    mapping.insert(direct.into(), "spine_3".into());
    let mut anchors = anchors;
    anchors.remove(direct);
    // The head role moves to a child joint of the head (as changeRole does: the old
    // joint's match goes, the new joint takes the role's target).
    let moved = "Head_Local";
    assert!(source.iter().any(|b| b.name == moved && b.parent.as_deref() == Some(head.as_str())));
    mapping.remove(&head);
    anchors.remove(moved);
    roles.insert("head".into(), moved.into());
    mapping.insert(moved.into(), target_roles["head"].clone());
    let edited = options(json!({"mapping": mapping, "mapping_anchors": anchors, "source_roles": roles, "target_roles": target_roles,
        "follow_motion_reviewed": true, "complete_mapping": true}));
    let issues = algorithms::review_issues(&target, &source, &weighted, "rigged_humanoid", "rigged_humanoid", &edited);
    let grouped: Vec<_> = issues.iter().filter(|i| i.kind.as_deref() == Some(following::STALE_KIND)).collect();
    assert_eq!(grouped.len(), 1, "{issues:?}");
    assert_eq!(grouped[0].tab, "mapping");
    for name in &followers { assert!(grouped[0].bones.contains(name), "{name} {:?}", grouped[0].bones); }
    assert_eq!(grouped[0].message, following::stale_message(&grouped[0].bones));
    assert_eq!(issues.iter().filter(|i| i.message.contains("body-following match")).count(), 1, "{issues:?}");

    // Refresh suggestions using my roles: the same roles go in.
    let refreshed = automap::suggest(&source, &weighted, &target, &edited).unwrap();
    assert_eq!(refreshed.mapping[direct], "spine_3", "a direct match made by hand is kept");
    // The roles come back as they were sent (the page keeps the user's anyway).
    let identified = refreshed.source.clone().unwrap();
    assert_eq!(identified.roles, roles);
    assert!(identified.errors.is_empty(), "{:?}", identified.errors);
    let anchors = refreshed.assistance.clone().unwrap().anchors;
    assert!(!anchors.contains_key(direct));
    // The bones that followed the old head joint follow a mapped joint again.
    for name in &followers {
        let anchor = anchors.get(name).unwrap_or_else(|| panic!("{name} lost its body-following match"));
        assert_ne!(anchor, &head, "{name}");
        assert_eq!(refreshed.mapping.get(name), refreshed.mapping.get(anchor), "{name}");
    }
    let after = options(json!({"mapping": refreshed.mapping, "mapping_anchors": anchors, "source_roles": roles, "target_roles": target_roles, "follow_motion_reviewed": true}));
    assert!(following::stale(&source, &after).is_empty(), "{:?}", following::stale(&source, &after));
    let issues = algorithms::review_issues(&target, &source, &weighted, "rigged_humanoid", "rigged_humanoid", &after);
    assert!(!issues.iter().any(|i| i.kind.is_some()), "{issues:?}");
}

/// A saved moving-part root that is no longer a detected group is one issue with
/// its kind; other selection errors keep none.
#[test]
fn saved_roots_that_are_not_detected_carry_their_kind() {
    let (source, weighted, target) = rig();
    let first = automap::suggest(&source, &weighted, &target, &options(json!({"complete_mapping": true}))).unwrap();
    let roles = first.source.clone().unwrap().roles;
    let base = json!({"mapping": first.mapping, "mapping_anchors": first.assistance.unwrap().anchors, "source_roles": roles, "target_roles": first.target.roles,
        "follow_motion_reviewed": true, "preserve_physics_bones": true});
    let mut stale = base.clone();
    stale["physics_roots"] = json!([roles["thigh_L"]]);
    let issues = algorithms::review_issues(&target, &source, &weighted, "rigged_humanoid", "rigged_humanoid", &options(stale));
    let physics: Vec<_> = issues.iter().filter(|i| i.tab == "physics bones").collect();
    assert_eq!(physics.len(), 1, "{issues:?}");
    assert_eq!(physics[0].kind.as_deref(), Some(secondary_bones::STALE_GROUPS_KIND));
    assert_eq!(physics[0].message, secondary_bones::stale_groups_message());
    let mut empty = base;
    empty["physics_roots"] = json!([]);
    let issues = algorithms::review_issues(&target, &source, &weighted, "rigged_humanoid", "rigged_humanoid", &options(empty));
    assert!(issues.iter().any(|i| i.tab == "physics bones" && i.kind.is_none()), "{issues:?}");
}
