use deadlock_preparation::{algorithms::{self,Bone}, automap, secondary_bones};
use deadlock_project::PreparationOptions;
use serde_json::{Value,json};
mod common;

/// The captured Daki rig (private assets), or `None` when it is not checked out.
fn fixture() -> Option<(Vec<Bone>,Vec<String>,Vec<Bone>)> {
    let v:Value=serde_json::from_str(&common::private_text("fixtures/daki-rig.json")?).unwrap();
    Some((serde_json::from_value(v["source"].clone()).unwrap(),serde_json::from_value(v["weighted"].clone()).unwrap(),serde_json::from_value(v["target"].clone()).unwrap()))
}
#[test]
fn duplicate_fullwidth_alias_uses_neck_ancestry_and_all_weighted_bones_have_a_plan() {
    let Some((mut source,weighted,target))=fixture() else { return };
    for reversed in [false,true] {
        if reversed { source.reverse(); }
        let r=automap::suggest(&source,&weighted,&target,&PreparationOptions::default()).unwrap();
        let sr=r.source.unwrap();assert!(sr.errors.is_empty(),"{:?}",sr.errors);
        assert_eq!(sr.roles["chest"],"上半身2");assert_eq!(sr.roles.len(),17);
        let groups=secondary_bones::candidates(&source,&weighted,&sr.roles,&r.target.roles).unwrap();
        let options:PreparationOptions=serde_json::from_value(json!({"source_roles":sr.roles,"target_roles":r.target.roles,
            "mapping":r.mapping,"mapping_anchors":r.assistance.unwrap().anchors,"preserve_physics_bones":true,
            "physics_roots":groups.iter().map(|g|g.root.clone()).collect::<Vec<_>>(),"reviewed":true,"follow_motion_reviewed":true})).unwrap();
        let plan=secondary_bones::selection(&source,&weighted,&target,&options).unwrap().unwrap();
        let kept=plan.exported_bones.keys().cloned().collect::<Vec<_>>();
        let remaining:Vec<_>=weighted.iter().filter(|n|!kept.contains(n)).cloned().collect();
        assert_eq!(weighted.len(),263);
        assert_eq!(remaining.len(),140);
        let issues=algorithms::review_issues(&target,&source,&weighted,"rigged_humanoid","rigged_humanoid",&options);
        assert!(issues.is_empty(),"{issues:?}");
        for name in ["HAIR A_0_1","KIMONO A_0_1","上唇","顔上","頭２","舌4","足先EX.L","胸上.L"] {
            assert!(options.values["mapping"].get(name).is_some(),"{name}");
            assert!(options.values["mapping_anchors"].get(name).is_some(),"{name}");
        }
        assert!(!plan.new_physics);
    }
}
#[test]
fn static_follow_needs_the_correct_body_branch_bounded_skin_and_no_dynamic_binding() {
    let Some((source,weighted,target))=fixture() else { return };
    for variant in ["wrong-branch","distant-skin","dynamic"] {
        let mut changed=source.clone();
        let b=changed.iter_mut().find(|b|b.name=="顔上").unwrap();
        match variant {
            "wrong-branch"=>b.parent=Some("腰".into()),
            "distant-skin"=>{b.details.insert("skin".into(),json!({"centroid":[100.,0.,0.]}));},
            _=>{b.details.insert("physics".into(),json!({"policy":secondary_bones::BINDING_POLICY,"dynamic_bodies":["verified"],"kinematic_bodies":[]}));}
        }
        let r=automap::suggest(&changed,&weighted,&target,&PreparationOptions::default()).unwrap();
        assert!(!r.mapping.contains_key("顔上"),"{variant}");
    }
}
#[test]
fn explicit_facial_mapping_survives_and_disconnected_chest_alias_is_not_guessed() {
    let Some((mut source,weighted,target))=fixture() else { return };
    let options:PreparationOptions=serde_json::from_value(json!({"mapping":{"顎":"pelvis"}})).unwrap();
    let r=automap::suggest(&source,&weighted,&target,&options).unwrap();
    assert_eq!(r.mapping["顎"],"pelvis");assert!(!r.assistance.unwrap().anchors.contains_key("顎"));
    source.iter_mut().find(|b|b.name=="首").unwrap().parent=Some("上半身".into());
    let r=automap::suggest(&source,&weighted,&target,&PreparationOptions::default()).unwrap();
    assert!(r.source.unwrap().missing.contains(&"chest".into()));
}

#[test]
#[ignore="requires DMB_MMD_CHECKS with the captured Blender conversion"]
fn captured_mmd_geometry_exports_with_exact_original_binds_and_all_physics_branches() {
    use std::{fs,path::PathBuf};
    use deadlock_skeleton::{Skeleton,transforms::PreparedBone,export::{self,PreparedReport}};
    let root=PathBuf::from(std::env::var("DMB_MMD_CHECKS").unwrap());
    let read=|name:&str|serde_json::from_slice::<Value>(&fs::read(root.join(name)).unwrap()).unwrap();
    let f=read("daki-export-fixture.json");let raw=read("daki52/result.json");
    let source:Vec<Bone>=serde_json::from_value(f["source"].clone()).unwrap();
    let weighted:Vec<String>=serde_json::from_value(f["weighted"].clone()).unwrap();
    let target:Vec<PreparedBone>=serde_json::from_value(f["target"].clone()).unwrap();
    let original:Skeleton=serde_json::from_value(f["target_skeleton"].clone()).unwrap();
    let options:PreparationOptions=serde_json::from_value(read("daki-options.json")).unwrap();
    let target_bones:Vec<_>=target.iter().map(Bone::from).collect();
    let issues=algorithms::review_issues(&target_bones,&source,&weighted,"rigged_humanoid","rigged_humanoid",&options);
    assert!(issues.is_empty(),"{issues:?}");
    let plan=secondary_bones::selection(&source,&weighted,&target_bones,&options).unwrap().unwrap();
    assert_eq!(serde_json::to_value(&plan).unwrap(),raw["preserved_physics_bones"]);
    let prepared:Vec<PreparedBone>=serde_json::from_value(raw["target"].clone()).unwrap();
    let skeleton=secondary_bones::export_skeleton(&original,&target,&prepared,&source,Some(&plan)).unwrap();
    let report:PreparedReport=serde_json::from_value(raw).unwrap();
    let (bytes,audit)=export::export_bytes(&report,&skeleton).unwrap();
    fs::write(root.join("daki52/prepared.dmx"),bytes).unwrap();
    fs::write(root.join("daki52/export-audit.json"),serde_json::to_vec_pretty(&audit).unwrap()).unwrap();
    println!("{} original plus {} preserved bones; 263 weighted sources covered",original.names.len(),plan.exported_bones.len());
}
