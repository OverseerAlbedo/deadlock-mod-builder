use deadlock_preparation::{
    algorithms::{self, Bone},
    automap, following, mapping_presets,
};
use deadlock_project::PreparationOptions;
use serde_json::{Value, json};
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
fn tangtang_groups_resolve_weights_only_after_explicit_secondary_choices() {
    let Some((source, weighted, target)) = fixture() else { return };
    let result =
        automap::suggest(&source, &weighted, &target, &PreparationOptions::default()).unwrap();
    let mut mapping = result.mapping.clone();
    let assist = result.assistance.unwrap();
    let mut anchors = assist.anchors.clone();
    let first = weighted.iter().filter(|n| mapping.contains_key(*n)).count();
    let mut groups = std::collections::BTreeSet::new();
    for entry in assist.entries {
        if mapping.contains_key(&entry.source) {
            continue;
        }
        if let Some(p) = entry.follow {
            assert!(!p.automatic);
            groups.insert(p.group);
            mapping.insert(entry.source.clone(), p.target);
            anchors.insert(entry.source, p.anchor);
        }
    }
    let missing: Vec<_> = weighted
        .iter()
        .filter(|n| !mapping.contains_key(*n))
        .collect();
    assert!(missing.is_empty(), "{missing:?}");
    let mut options:PreparationOptions=serde_json::from_value(json!({"mapping":mapping,"mapping_anchors":anchors,"source_roles":result.source.unwrap().roles,"target_roles":result.target.roles,"reviewed":true})).unwrap();
    assert!(!following::errors(&source, &options).is_empty());
    options
        .values
        .insert("follow_motion_reviewed".into(), true.into());
    assert!(
        algorithms::review_issues(
            &target,
            &source,
            &weighted,
            "rigged_humanoid",
            "rigged_humanoid",
            &options
        )
        .is_empty()
    );
    eprintln!(
        "TangTang rig: {first}/{} weighted bones assigned initially; {} review groups resolve the rest. This is a rig test, not a converted-model or game test.",
        weighted.len(),
        groups.len()
    );
    options.values.get_mut("mapping").unwrap()["ひじ.L"] = json!("head");
    assert!(!following::errors(&source, &options).is_empty());
}
#[test]
fn unexpected_geometry_and_side_disable_automatic_helper_adaptation() {
    let Some((mut source, weighted, target)) = fixture() else { return };
    let helper = source
        .iter_mut()
        .find(|b| b.name == "Bip001_L_Forearm_ty_minus")
        .unwrap();
    helper
        .details
        .insert("skin".into(), json!({"centroid":[100.,0.,0.]}));
    let result =
        automap::suggest(&source, &weighted, &target, &PreparationOptions::default()).unwrap();
    assert!(!result.mapping.contains_key("Bip001_L_Forearm_ty_minus"));
    let p = result
        .assistance
        .unwrap()
        .entries
        .into_iter()
        .find(|e| e.source == "Bip001_L_Forearm_ty_minus")
        .unwrap()
        .follow
        .unwrap();
    assert!(!p.automatic);
    let helper = source
        .iter_mut()
        .find(|b| b.name == "Bip001_L_Forearm_ty_minus")
        .unwrap();
    helper.details.remove("skin");
    helper.parent = Some("ひじ.R".into());
    assert!(
        !automap::suggest(&source, &weighted, &target, &PreparationOptions::default())
            .unwrap()
            .mapping
            .contains_key("Bip001_L_Forearm_ty_minus")
    );
}
#[test]
fn existing_direct_choices_are_never_overwritten_by_helper_suggestions() {
    let Some((source, weighted, target)) = fixture() else { return };
    let options: PreparationOptions =
        serde_json::from_value(json!({"mapping":{"手捩.L":"hand_L","HB_1_0":"pelvis"}})).unwrap();
    let result = automap::suggest(&source, &weighted, &target, &options).unwrap();
    assert_eq!(result.mapping["手捩.L"], "hand_L");
    assert!(!result.assistance.unwrap().anchors.contains_key("HB_1_0"));
}
#[test]
fn presets_bind_to_rig_geometry_and_never_restore_approval() {
    let Some((source, weighted, mut target)) = fixture() else { return };
    let key = mapping_presets::fingerprint(&source, &weighted, &target, [0.; 3]).unwrap();
    let mut reordered = source.clone();
    reordered.reverse();
    let mut weights = weighted.clone();
    weights.reverse();
    assert_eq!(
        key,
        mapping_presets::fingerprint(&reordered, &weights, &target, [0.; 3]).unwrap()
    );
    target[0].position[0] += 0.01;
    assert_ne!(
        key,
        mapping_presets::fingerprint(&source, &weighted, &target, [0.; 3]).unwrap()
    );
    let preset=mapping_presets::Preset{policy:mapping_presets::POLICY.into(),fingerprint:key.clone(),options:serde_json::from_value(json!({"mapping":{"a":"b"},"reviewed":true,"follow_motion_reviewed":true,"install":true})).unwrap()};
    let restored = mapping_presets::restore(preset, &key).unwrap();
    assert_eq!(restored.values["reviewed"], false);
    assert_eq!(restored.values["follow_motion_reviewed"], false);
    assert!(!restored.values.contains_key("install"));
}
/// The fit policies the Blender retarget reports are the ones Rust accepts
/// (`fitting::check_policy`). Match hero proportions with body-following
/// mappings reported "-v1" against Rust's "-v2" and never converted (found
/// preparing a character on Nurse Harrow, October 20, 2026).
#[test]
fn retarget_reports_the_fit_policies_rust_accepts() {
    let script = include_str!("../resources/blender/retarget.py");
    for (constant, policy) in [
        ("POLICY", deadlock_preparation::service::REST_POLICY),
        ("PROPORTIONS_POLICY", deadlock_preparation::fitting::PROPORTIONS_POLICY),
        ("FOLLOWING_POLICY", following::POLICY),
    ] {
        assert!(script.contains(&format!("\n{constant} = '{policy}'")), "retarget.py {constant} is not {policy}");
    }
    assert!(!script.contains("ancestor-offset-preservation-v1"));
}
