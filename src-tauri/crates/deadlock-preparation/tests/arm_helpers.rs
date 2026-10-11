use deadlock_preparation::{algorithms::Bone,arm_helpers,automap};
use serde_json::{json,Value};
use std::collections::BTreeMap;

fn fixture()->(Vec<Bone>,Vec<Bone>,BTreeMap<String,String>,BTreeMap<String,String>) {
    let v:Value=serde_json::from_str(include_str!("fixtures/arm-deformation.json")).unwrap();
    (serde_json::from_value(v["source"].clone()).unwrap(),serde_json::from_value(v["target"].clone()).unwrap(),
     serde_json::from_value(v["source_roles"].clone()).unwrap(),serde_json::from_value(v["target_roles"].clone()).unwrap())
}
#[test]
fn actual_shoulders_and_sleeve_helpers_use_existing_hero_joints() {
    let (s,t,sr,tr)=fixture();let found=arm_helpers::detect(&s,&t,&sr,&tr);
    for side in ["L","R"] {
        assert_eq!(found[&format!("肩.{side}")].target,format!("clavicle_{side}"));
        for (a,b) in [("ShoulderPart1","arm_upper"),("ElbowPart1","arm_lower")] {
            assert_eq!(found[&format!("{a}_{side}")].target,format!("{b}_{side}_TWIST"));
            assert_eq!(found[&format!("{}2_{side}",a.trim_end_matches('1'))].target,format!("{b}_{side}_TWIST1"));
        }
    }
    assert!(!found.keys().any(|s|s.starts_with("_dummy_")||s.starts_with("_shadow_")));
}
#[test]
fn scales_names_spaces_and_rotated_rigs_do_not_change_matches() {
    let (s,t,sr,tr)=fixture();let expected=arm_helpers::detect(&s,&t,&sr,&tr);
    let mut rotated=s.clone();
    for b in &mut rotated {let [x,y,z]=b.position;b.position=[-y*3.+11.,z*3.-7.,-x*3.+5.];}
    assert_eq!(arm_helpers::detect(&rotated,&t,&sr,&tr),expected);
    let mut bad=s.clone();
    bad.iter_mut().find(|b|b.name=="ShoulderPart1_L").unwrap().position[2]+=1.;
    bad.iter_mut().find(|b|b.name=="ElbowPart1_L").unwrap().parent=Some(sr["forearm_R"].clone());
    let found=arm_helpers::detect(&bad,&t,&sr,&tr);
    assert!(!found.contains_key("ShoulderPart1_L")&&!found.contains_key("ElbowPart1_L"));
    let mut ambiguous=t.clone();
    let mut extra=t.iter().find(|b|b.name=="arm_upper_L_TWIST").unwrap().clone();extra.name="upperarm_twist_duplicate_L".into();ambiguous.push(extra);
    assert!(!arm_helpers::detect(&s,&ambiguous,&sr,&tr).contains_key("ShoulderPart1_L"));
    let mut renamed=s.clone();
    for (old,new) in [("肩.L","vendor:LeftShoulder"),("ShoulderPart1_L","vendor:upperarm_twist_L"),("ElbowPart1_L","vendor:forearm_twist_L")] {
        for b in &mut renamed {if b.name==old {b.name=new.into();}if b.parent.as_deref()==Some(old){b.parent=Some(new.into());}}
    }
    let found=arm_helpers::detect(&renamed,&t,&sr,&tr);
    assert_eq!(found["vendor:LeftShoulder"].target,"clavicle_L");
    assert_eq!(found["vendor:upperarm_twist_L"].target,"arm_upper_L_TWIST");
    assert_eq!(found["vendor:forearm_twist_L"].target,"arm_lower_L_TWIST");
    for name in ["arm_controller_L","tail_twist_L","hair_twist_L","arm_roll_R"] {
        let mut trial=s.clone();let mut extra=trial.iter().find(|b|b.name=="ShoulderPart1_L").unwrap().clone();
        extra.name=name.into();trial.push(extra);
        assert!(!arm_helpers::detect(&trial,&t,&sr,&tr).contains_key(name),"{name}");
    }
}
#[test]
fn stale_follows_upgrade_but_explicit_direct_choices_remain() {
    let (s,t,sr,tr)=fixture();
    let weighted=s.iter().filter(|b|b.details.contains_key("skin")).map(|b|b.name.clone()).collect::<Vec<_>>();
    let options=serde_json::from_value(json!({"source_roles":sr,"target_roles":tr,
        "mapping":{"肩.L":"spine_3","ShoulderPart1_L":"arm_upper_L","ShoulderPart2_L":"arm_upper_L"},
        "mapping_anchors":{"肩.L":"上半身2","ShoulderPart1_L":"腕.L"}})).unwrap();
    let found=automap::suggest(&s,&weighted,&t,&options).unwrap();
    assert!(found.source.as_ref().unwrap().errors.is_empty());assert!(found.target.errors.is_empty());
    assert_eq!(found.mapping["肩.L"],"clavicle_L");
    assert_eq!(found.mapping["ShoulderPart1_L"],"arm_upper_L_TWIST");
    assert_eq!(found.mapping["ShoulderPart2_L"],"arm_upper_L");
    let a=found.assistance.unwrap();assert!(!a.anchors.contains_key("肩.L")&&!a.anchors.contains_key("ShoulderPart1_L"));
}

#[test]
#[ignore="requires DMB_ARM_INPUT and DMB_ARM_OUTPUT for a captured project"]
fn captured_arm_mapping() {
    use std::{fs,path::PathBuf};
    let root=PathBuf::from(std::env::var("DMB_ARM_INPUT").unwrap());
    let out=PathBuf::from(std::env::var("DMB_ARM_OUTPUT").unwrap());
    let read=|name:&str|serde_json::from_slice::<Value>(&fs::read(root.join(name)).unwrap()).unwrap();
    let f=read("fixture.json");let s:Vec<Bone>=serde_json::from_value(f["source"].clone()).unwrap();
    let t:Vec<Bone>=serde_json::from_value(f["target"].clone()).unwrap();let w:Vec<String>=serde_json::from_value(f["weighted"].clone()).unwrap();
    let mut p=read("project-before.json");let options=serde_json::from_value(p["preparation_options"].clone()).unwrap();
    let suggestion=automap::suggest(&s,&w,&t,&options).unwrap();
    assert!(suggestion.source.as_ref().unwrap().errors.is_empty());assert!(suggestion.target.errors.is_empty());
    p["preparation_options"]["mapping"]=serde_json::to_value(&suggestion.mapping).unwrap();
    p["preparation_options"]["mapping_anchors"]=serde_json::to_value(&suggestion.assistance.as_ref().unwrap().anchors).unwrap();
    p["preparation_options"]["source_roles"]=serde_json::to_value(&suggestion.source.as_ref().unwrap().roles).unwrap();
    p["preparation_options"]["target_roles"]=serde_json::to_value(&suggestion.target.roles).unwrap();
    fs::create_dir_all(&out).unwrap();
    fs::write(out.join("suggestions.json"),serde_json::to_vec_pretty(&suggestion).unwrap()).unwrap();
    fs::write(out.join("project-after.json"),serde_json::to_vec_pretty(&p).unwrap()).unwrap();
    println!("{} qualified arm mappings",suggestion.assistance.unwrap().arm_deformation.len());
}

#[test]
#[ignore="requires DMB_ARM_INPUT with completed Blender conversion"]
fn captured_arm_preparation_export() {
    use deadlock_preparation::secondary_bones;
    use deadlock_skeleton::{Skeleton,export::{self,PreparedReport},transforms::PreparedBone,extensions::SourceBinding};
    use sha2::{Digest,Sha256};
    use std::{fs,path::PathBuf};
    let root=PathBuf::from(std::env::var("DMB_ARM_INPUT").unwrap());let checks=root.parent().unwrap();
    let f:Value=serde_json::from_slice(&fs::read(root.join("fixture.json")).unwrap()).unwrap();
    let source:Vec<Bone>=serde_json::from_value(f["source"].clone()).unwrap();
    let weighted:Vec<String>=serde_json::from_value(f["weighted"].clone()).unwrap();
    let target:Vec<PreparedBone>=serde_json::from_value(f["target"].clone()).unwrap();
    let original:Skeleton=serde_json::from_value(f["target_skeleton"].clone()).unwrap();
    let project:deadlock_project::Project=serde_json::from_slice(&fs::read(root.join("project-after.json")).unwrap()).unwrap();
    let report:Value=serde_json::from_slice(&fs::read(checks.join("blender/tangtang/result.json")).unwrap()).unwrap();
    let target_bones:Vec<_>=target.iter().map(Bone::from).collect();
    let issues=deadlock_preparation::algorithms::review_issues(&target_bones,&source,&weighted,"rigged_humanoid","rigged_humanoid",&project.preparation_options);
    assert!(issues.is_empty(),"{issues:?}");
    let plan=secondary_bones::selection(&source,&weighted,&target_bones,&project.preparation_options).unwrap().unwrap();
    assert_eq!(serde_json::to_value(&plan).unwrap(),report["preserved_physics_bones"]);
    let prepared:Vec<PreparedBone>=serde_json::from_value(report["target"].clone()).unwrap();
    let combined=secondary_bones::export_skeleton(&original,&target,&prepared,&source,Some(&plan)).unwrap();
    let report:PreparedReport=serde_json::from_value(report).unwrap();
    let (bytes,audit)=export::export_bytes(&report,&combined).unwrap();
    let extra=plan.exported_bones.values().cloned().collect::<Vec<_>>();
    let identity=SourceBinding{source_sha256:format!("{:x}",Sha256::digest(&bytes)),original_sha256:f["original_sha256"].as_str().unwrap().into(),skeleton:combined,extra_names:extra.clone(),required_extra_names:extra};
    let dest=checks.join("converted-capture");fs::create_dir_all(dest.join("input")).unwrap();fs::create_dir_all(dest.join("workspace")).unwrap();
    fs::write(dest.join("project.json"),serde_json::to_vec_pretty(&project).unwrap()).unwrap();
    fs::write(dest.join("input/prepared.dmx"),bytes).unwrap();
    fs::write(dest.join("workspace/source-skeleton-identity.json"),serde_json::to_vec_pretty(&json!({"extension":identity})).unwrap()).unwrap();
    fs::write(checks.join("converted-export.json"),serde_json::to_vec_pretty(&audit).unwrap()).unwrap();
    println!("{} original + {} retained bones exported",original.names.len(),plan.exported_bones.len());
}
