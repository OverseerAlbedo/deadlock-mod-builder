use deadlock_preparation::{algorithms::{self, Bone}, automap};
use deadlock_project::PreparationOptions;
use serde_json::json;

fn fixture() -> Vec<Bone> {
    serde_json::from_str(include_str!("fixtures/recovered-skin-rig.json")).unwrap()
}

#[test]
fn recovered_roles_produce_follow_proposals_without_bypassing_review() {
    let source = fixture();
    let target: Vec<Bone> = source.iter().filter_map(|b| b.details.get("humanoid_role_hint").and_then(|v|v.as_str()).map(|r| {
        let mut bone = b.clone(); bone.name=r.into();
        bone.parent=b.parent.as_ref().and_then(|p|source.iter().find(|b| &b.name==p)).and_then(|b|b.details.get("humanoid_role_hint")).and_then(|v|v.as_str()).map(str::to_owned);
        bone.details.clear();bone
    })).collect();
    let weighted=source.iter().map(|b|b.name.clone()).collect::<Vec<_>>();
    let s=automap::suggest(&source,&weighted,&target,&PreparationOptions::default()).unwrap();
    assert!(s.source.as_ref().unwrap().errors.is_empty());
    assert_eq!(s.source.as_ref().unwrap().roles.len(),17);
    assert!(s.assistance.as_ref().unwrap().entries.iter().any(|e|e.follow.is_some()));
    let options:PreparationOptions=serde_json::from_value(json!({"source_roles":s.source.unwrap().roles,"target_roles":s.target.roles,"mapping":s.mapping})).unwrap();
    assert!(!algorithms::review_issues(&target,&source,&weighted,"rigged_humanoid","rigged_humanoid",&options).is_empty());
}

#[test]
fn hints_never_authorize_bad_positions_parents_or_manual_roles() {
    for broken in ["position","parent","override","duplicate"] {
        let mut source=fixture();
        let index=source.iter().position(|b|b.details.get("humanoid_role_hint")==Some(&json!("neck"))).unwrap();
        let mut overrides=std::collections::BTreeMap::new();
        match broken {
            "position" => source[index].position=[0.,0.,-20.],
            "parent" => source[index].parent=None,
            "override" => { overrides.insert("head".into(),"Bip001_pelvis".into()); },
            _ => {let mut duplicate=source[index].clone();duplicate.name="ambiguous neck".into();source.push(duplicate);}
        }
        let r=automap::roles(&source,&[],&overrides).unwrap();
        if broken=="duplicate" && r.errors.is_empty() {
            // Hints stay unusable, but structural recognition may resolve the
            // neck to the joint that actually carries the head - never the
            // detached duplicate.
            assert_eq!(r.roles["neck"],source[index].name,"{broken}");
            assert!(r.evidence.starts_with("Skeleton structure"),"{broken}");
            continue;
        }
        assert!(!r.errors.is_empty(),"{broken}");
    }
}

#[test]
fn common_namespaces_do_not_capture_control_or_twist_bones() {
    for (name,role) in [("ValveBiped.Bip01_L_UpperArm",Some("upperarm_L")),("DEF-forearm.L",Some("forearm_L")),
        ("J_Bip_C_Hips",Some("pelvis")),("J_Bip_L_UpperArm",Some("upperarm_L")),("J_Bip_R_Foot",Some("foot_R")),
        ("MCH-upperarm.L",None),("DEF-upperarm.L.001",None),("Bip001_B_NeckTwist",None)] {
        assert_eq!(algorithms::name_role(name),role,"{name}");
    }
}
