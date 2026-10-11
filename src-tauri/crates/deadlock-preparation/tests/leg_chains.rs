use deadlock_preparation::{algorithms::Bone, automap, leg_chains};
use deadlock_project::PreparationOptions;
use serde_json::{json, Value};
use std::collections::BTreeMap;
fn fixture() -> (Vec<Bone>, Vec<String>, Vec<Bone>) {
    let v: Value = serde_json::from_str(include_str!("fixtures/parallel-leg-rig.json")).unwrap();
    (serde_json::from_value(v["source"].clone()).unwrap(),
     serde_json::from_value(v["weighted"].clone()).unwrap(),
     serde_json::from_value(v["target"].clone()).unwrap())
}
#[test]
fn actual_mixed_skin_chains_share_all_three_limb_targets() {
    let (source, weighted, target) = fixture();
    let result = automap::suggest(&source, &weighted, &target, &Default::default()).unwrap();
    let roles = &result.source.as_ref().unwrap().roles;
    assert!(result.source.as_ref().unwrap().errors.is_empty());
    assert!(result.target.errors.is_empty());
    assert_eq!(leg_chains::equivalents(&source, roles).len(), 6);
    for side in ["L", "R"] { for (base, target) in [("足", "leg_upper"), ("ひざ", "leg_lower"), ("足首", "ankle")] {
        let ordinary = format!("{base}.{side}"); let deform = format!("{base}D.{side}");
        assert_eq!(result.mapping[&ordinary], format!("{target}_{side}"));
        assert_eq!(result.mapping[&ordinary], result.mapping[&deform]);
        assert!(!result.assistance.as_ref().unwrap().anchors.contains_key(&ordinary));
        assert!(!result.mapping.contains_key(&format!("_shadow_{base}D.{side}")));
    } }
}
#[test]
fn old_pelvis_follow_choices_are_repaired_without_changing_unrelated_choices() {
    let (source, weighted, target) = fixture();
    let options: PreparationOptions = serde_json::from_value(json!({
        "mapping":{"足.L":"pelvis","ひざ.R":"pelvis","hair_L_d_02_jnt":"head"},
        "mapping_anchors":{"足.L":"腰","ひざ.R":"腰"}
    })).unwrap();
    let result = automap::suggest(&source, &weighted, &target, &options).unwrap();
    assert_eq!(result.mapping["足.L"], "leg_upper_L");
    assert_eq!(result.mapping["ひざ.R"], "leg_lower_R");
    assert_eq!(result.mapping["hair_L_d_02_jnt"], "head");
    assert!(!result.assistance.unwrap().anchors.contains_key("足.L"));
}
#[test]
fn weights_choose_a_complete_chain_not_individual_joints_from_different_rigs() {
    let (source, _, _) = fixture();
    let weighted = ["足D.L", "ひざ.L", "足首.L", "足D.R", "ひざ.R", "足首.R"].map(String::from);
    let result = automap::roles(&source, &weighted, &BTreeMap::new()).unwrap();
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    for side in ["L", "R"] { assert_eq!(result.roles[&format!("thigh_{side}")], format!("足.{side}")); }
    let ambiguous = automap::roles(&source, &[], &BTreeMap::new()).unwrap();
    assert!(!ambiguous.roles.contains_key("thigh_L"));
}
#[test]
fn full_chain_evidence_rejects_wrong_positions_broken_ancestry_and_helper_names() {
    let (source, weighted, _) = fixture();
    let roles = automap::roles(&source, &weighted, &BTreeMap::new()).unwrap().roles;
    for mutation in 0..3 {
        let mut bones = source.clone();
        let knee = bones.iter_mut().find(|b| b.name == "ひざ.L").unwrap();
        match mutation {
            0 => knee.position[0] += 0.1,
            1 => knee.parent = Some("腰".into()),
            _ => { knee.name = "knee_helper_L".into();
                for b in &mut bones { if b.parent.as_deref() == Some("ひざ.L") { b.parent = Some("knee_helper_L".into()); } }
            }
        }
        let aliases = leg_chains::equivalents(&bones, &roles);
        assert!(!aliases.contains_key("足.L"));
        assert!(!aliases.contains_key("ひざ.L"));
        assert!(aliases.contains_key("ひざ.R"));
    }
}
#[test]
fn english_parallel_leg_names_work_and_target_knee_helpers_fail_review() {
    let (mut source, weighted, mut target) = fixture();
    let roles = automap::roles(&source, &weighted, &BTreeMap::new()).unwrap().roles;
    for (old,new) in [("足.L","mixamorig:LeftUpLeg"),("ひざ.L","mixamorig:LeftLeg"),("足首.L","mixamorig:LeftFoot")] {
        for b in &mut source { if b.name == old { b.name = new.into(); } if b.parent.as_deref() == Some(old) { b.parent = Some(new.into()); } }
    }
    let aliases = leg_chains::equivalents(&source, &roles);
    assert_eq!(aliases["mixamorig:LeftLeg"], "ひざD.L");
    let mut helper = target.iter().find(|b| b.name == "leg_lower_R").unwrap().clone();
    helper.name = "knee_helper_R".into(); target.retain(|b| b.name != helper.name); target.push(helper);
    let detected = automap::roles(&target, &[], &BTreeMap::new()).unwrap();
    assert_eq!(detected.roles["shin_R"], "leg_lower_R");
    let bad = automap::roles(&target, &[], &BTreeMap::from([("shin_R".into(),"knee_helper_R".into())])).unwrap();
    assert!(bad.errors.iter().any(|e| e.contains("Hierarchy mismatch")));
}

#[test]
#[ignore = "requires DMB_LEG_INPUT with the actual Blender conversion report"]
fn actual_converted_asset_exports_literal_mesh_filters_and_original_binds() {
    use deadlock_preparation::{secondary_bones, service};
    use deadlock_skeleton::{Skeleton, export::{self, PreparedReport}, transforms::PreparedBone};
    use std::{fs, path::PathBuf};
    let root = PathBuf::from(std::env::var("DMB_LEG_INPUT").unwrap());
    let load = |p: &str| -> Value { serde_json::from_slice(&fs::read(root.join(p)).unwrap()).unwrap() };
    let input = load("conversion-fixture.json");
    let report = load("real-conversion/tangtang/result.json");
    let source: Vec<Bone> = serde_json::from_value(input["source"].clone()).unwrap();
    let weighted: Vec<String> = serde_json::from_value(input["weighted"].clone()).unwrap();
    let target: Vec<PreparedBone> = serde_json::from_value(input["target"].clone()).unwrap();
    let target_bones: Vec<_> = target.iter().map(Bone::from).collect();
    let options: PreparationOptions = serde_json::from_value(load("corrected-test-project.json")["preparation_options"].clone()).unwrap();
    let issues = deadlock_preparation::algorithms::review_issues(&target_bones, &source, &weighted, "rigged_humanoid", "rigged_humanoid", &options);
    assert!(issues.is_empty(), "{issues:?}");
    let plan = secondary_bones::selection(&source, &weighted, &target_bones, &options).unwrap().unwrap();
    assert_eq!(serde_json::to_value(&plan).unwrap(), report["preserved_physics_bones"]);
    let original: Skeleton = serde_json::from_value(input["target_skeleton"].clone()).unwrap();
    let prepared: Vec<PreparedBone> = serde_json::from_value(report["target"].clone()).unwrap();
    let combined = secondary_bones::export_skeleton(&original, &target, &prepared, &source, Some(&plan)).unwrap();
    let report: PreparedReport = serde_json::from_value(report).unwrap();
    let (bytes, audit) = export::export_bytes(&report, &combined).unwrap();
    let template = service::mesh_template(&prepared, &report.meshes).unwrap();
    assert!(!template.contains("\\u"));
    assert!(template.contains("女管理员(技术测试版)_mesh"));
    let hierarchy = deadlock_model::template_bones(&template).unwrap();
    assert_eq!(hierarchy.len(), combined.names.len());
    let dmx = deadlock_dmx::Dmx::parse(bytes.clone()).unwrap();
    let isolated = deadlock_dmx::Dmx::parse(dmx.single_mesh(&report.meshes[0].name).unwrap()).unwrap();
    assert_eq!(isolated.summary().unwrap().meshes.len(), 1);
    fs::write(root.join("verified-prepared.dmx"), bytes).unwrap();
    fs::write(root.join("verified-mesh-template.vmdl"), template).unwrap();
    fs::write(root.join("native-export.json"), serde_json::to_vec_pretty(&audit).unwrap()).unwrap();
    println!("Actual asset: {} vertices, {} original + {} preserved bones; Unicode DMX name and ModelDoc filter agree.", report.meshes[0].positions.len(), original.names.len(), plan.exported_bones.len());
}
