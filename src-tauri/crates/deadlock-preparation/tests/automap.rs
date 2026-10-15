use deadlock_preparation::{
    algorithms::{self, Bone},
    automap,
};
use deadlock_project::PreparationOptions;
use serde_json::{Value, json};
use std::collections::BTreeMap;
mod common;
/// The captured TangTang rig (private assets), or `None` when it is not checked out.
fn fixture() -> Option<(Vec<Bone>, Vec<String>, Vec<Bone>)> {
    let v: Value = serde_json::from_str(&common::private_text("fixtures/tangtang-rig.json")?).unwrap();
    Some((
        serde_json::from_value(v["source"].clone()).unwrap(),
        serde_json::from_value(v["weighted"].clone()).unwrap(),
        serde_json::from_value(v["target"].clone()).unwrap(),
    ))
}
#[test]
fn tangtang_body_uses_weighted_d_chain_and_common_pelvis_control() {
    let Some((source, weighted, target)) = fixture() else { return };
    let result =
        automap::suggest(&source, &weighted, &target, &PreparationOptions::default()).unwrap();
    let sr = result.source.as_ref().unwrap();
    assert!(sr.errors.is_empty(), "{:?}", sr.errors);
    assert!(
        result.target.errors.is_empty(),
        "{:?}",
        result.target.errors
    );
    assert_eq!(sr.roles.len(), 17);
    assert_eq!(sr.roles["pelvis"], "腰");
    assert!(!weighted.contains(&"腰".into()));
    assert_eq!(result.mapping["腰"], "pelvis");
    for (role, name) in [
        ("head", "頭"),
        ("thigh_L", "足D.L"),
        ("shin_R", "ひざD.R"),
        ("foot_L", "足首D.L"),
    ] {
        assert_eq!(sr.roles[role], name);
    }
    assert_eq!(result.target.roles["chest"], "spine_3");
    assert_eq!(result.mapping["中指１.L"], "finger_middle_0_L");
    assert!(
        !result.mapping.contains_key("親指０.L"),
        "Unequal thumb chains cannot be collapsed automatically"
    );
    let assistance = result.assistance.unwrap();
    assert!(
        assistance
            .entries
            .iter()
            .any(|e| e.source == "下半身" && e.target.as_deref() == Some("pelvis") && e.automatic)
    );
    for entry in &assistance.entries {
        if entry.source.starts_with("HB_") || entry.source.starts_with("QB_") {
            assert!(
                !entry.automatic && entry.target.is_none(),
                "{}",
                entry.source
            );
        }
    }
    assert_eq!(assistance.anchors["下半身"], "腰");
    assert_eq!(assistance.anchors["手捩.L"], "ひじ.L");
    let options: PreparationOptions=serde_json::from_value(json!({"source_roles":sr.roles,"target_roles":result.target.roles,"mapping":result.mapping,"reviewed":true})).unwrap();
    assert!(
        !algorithms::review_issues(
            &target,
            &source,
            &weighted,
            "rigged_humanoid",
            "rigged_humanoid",
            &options
        )
        .is_empty(),
        "Partial auto-map must still block conversion"
    );
    eprintln!(
        "TangTang: 17 roles, {} mapped weighted bones of {}",
        weighted
            .iter()
            .filter(|n| result.mapping.contains_key(*n))
            .count(),
        weighted.len()
    );
}
#[test]
fn wrong_head_or_pelvis_role_is_not_qualified() {
    let Some((source, weighted, target)) = fixture() else { return };
    for (role, name) in [("head", "腰"), ("pelvis", "頭")] {
        let options: PreparationOptions =
            serde_json::from_value(json!({"source_roles":{role:name}})).unwrap();
        let result = automap::suggest(&source, &weighted, &target, &options).unwrap();
        assert!(!result.source.unwrap().errors.is_empty());
        assert!(!result.assistance.unwrap().warnings.is_empty());
    }
}
#[test]
fn existing_extra_mapping_is_retained_and_invalid_target_removed() {
    let Some((source, weighted, target)) = fixture() else { return };
    let options: PreparationOptions =
        serde_json::from_value(json!({"mapping":{"下半身":"pelvis","手捩.L":"missing-target"}}))
            .unwrap();
    let result = automap::suggest(&source, &weighted, &target, &options).unwrap();
    assert_eq!(result.mapping["下半身"], "pelvis");
    assert_eq!(result.mapping["手捩.L"], "arm_lower_L");
    assert_eq!(result.assistance.unwrap().anchors["手捩.L"], "ひじ.L");
}
#[test]
fn ambiguous_unweighted_leg_and_broken_finger_hierarchy_need_review() {
    let Some((mut source, _, target)) = fixture() else { return };
    let roles = automap::roles(&source, &[], &BTreeMap::new()).unwrap();
    assert!(!roles.roles.contains_key("thigh_L"));
    source
        .iter_mut()
        .find(|b| b.name == "中指２.L")
        .unwrap()
        .parent = Some("腰".into());
    let result = automap::suggest(&source, &[], &target, &PreparationOptions::default()).unwrap();
    assert!(!result.mapping.contains_key("中指１.L"));
}

#[test]
fn biped_and_mixamo_fingers_require_matching_complete_chains() {
    let Some((original,weighted,target))=fixture() else { return };
    for names in [["Bip001_L_Finger2","Bip001_L_Finger21","Bip001_L_Finger22"],
                  ["mixamorig:LeftHandMiddle1","mixamorig:LeftHandMiddle2","mixamorig:LeftHandMiddle3"]] {
        let replacements: BTreeMap<_,_>=["中指１.L","中指２.L","中指３.L"].into_iter().zip(names).collect();
        let rename=|n:&str|replacements.get(n).copied().unwrap_or(n).to_string();
        let mut source=original.clone();
        for b in &mut source { b.name=rename(&b.name);b.parent=b.parent.as_deref().map(rename); }
        let weighted=weighted.iter().map(|n|rename(n)).collect::<Vec<_>>();
        let result=automap::suggest(&source,&weighted,&target,&Default::default()).unwrap();
        assert_eq!(result.mapping[names[0]],"finger_middle_0_L");
        assert_eq!(result.mapping[names[2]],"finger_middle_2_L");
        source.iter_mut().find(|b|b.name==names[1]).unwrap().parent=Some("腰".into());
        let result=automap::suggest(&source,&weighted,&target,&Default::default()).unwrap();
        assert!(!result.mapping.contains_key(names[0]));
    }
}
