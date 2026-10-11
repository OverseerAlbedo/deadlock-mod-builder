//! Read-only real-source check for geometric selection and body transforms.
use deadlock_cloth_authoring::{attachments,secondary,surface};
use deadlock_core::CancellationToken;
use deadlock_dmx::{Dmx,Rewrite};
use deadlock_project::Project;
use deadlock_skeleton::extensions::SourceBinding;
use std::{collections::BTreeMap,fs,path::PathBuf};

#[test]
#[ignore="requires DMB_SKIRT_INPUT containing the captured prepared source"]
fn renamed_scaled_rotated_source_retains_only_the_accepted_upper_regions() {
    let root=PathBuf::from(std::env::var("DMB_SKIRT_INPUT").unwrap());
    let p:Project=serde_json::from_slice(&fs::read(root.join("project.json")).unwrap()).unwrap();
    let d=Dmx::parse(fs::read(root.join("input/prepared.dmx")).unwrap()).unwrap();
    let identity:serde_json::Value=serde_json::from_slice(&fs::read(root.join("workspace/source-skeleton-identity.json")).unwrap()).unwrap();
    let source:SourceBinding=serde_json::from_value(identity["extension"].clone()).unwrap();
    let mut setup=secondary::setup(&p,&d,Some(&source),&CancellationToken::new()).unwrap().unwrap();
    let before=setup.attachments.as_ref().unwrap();assert_eq!(before.plan.bones.len(),11);
    assert!(before.plan.bones.iter().all(|b|!b.bone.contains("shengzi")));
    let expected=before.plan.bones.iter().map(|r|(r.bone.clone(),r.radius_inches)).collect::<BTreeMap<_,_>>();
    let mut rewrite=Rewrite::default();let mut renamed=BTreeMap::new();
    for (i,e) in d.elements.iter().enumerate().filter(|(_,e)|e.kind=="DmeJoint" && e.name.starts_with("dmb_secondary_")) {
        let name=format!("dmb_secondary_anonymous_{i}");rewrite.names.insert(i,name.clone());renamed.insert(e.name.clone(),name);
    }
    let bind=surface::binds(&d).unwrap();
    for (bone,&parent) in bind.skeleton.parents.iter().enumerate() {if parent<0 {
        let name=&bind.skeleton.names[bone];let element=d.elements.iter().position(|e|e.kind=="DmeTransform"&&&e.name==name).unwrap();
        let old=bind.skeleton.positions[bone];let q=bind.skeleton.rotations[bone];let c=std::f64::consts::FRAC_1_SQRT_2;
        let rotation=[c*q[0]-c*q[1],c*q[0]+c*q[1],c*q[2]+c*q[3],c*q[3]-c*q[2]];
        rewrite.attributes.insert((element,"scale".into()),((bind.skeleton.scales[bone]*3.) as f32).to_le_bytes().to_vec());
        rewrite.attributes.insert((element,"position".into()),[-old[1]*3.+17.,old[0]*3.-12.,old[2]*3.+7.].into_iter().flat_map(|v|(v as f32).to_le_bytes()).collect());
        rewrite.attributes.insert((element,"orientation".into()),rotation.into_iter().flat_map(|v|(v as f32).to_le_bytes()).collect());
    }}
    for (i,e) in d.elements.iter().enumerate().filter(|(_,e)|e.kind=="DmeVertexData") {
        if let Some(a)=e.attributes.iter().find(|a|a.name=="position$0") {
            let raw=d.raw_attribute(a);let mut next=raw[..4].to_vec();
            for v in raw[4..].chunks_exact(12) {
                let p:[f32;3]=std::array::from_fn(|k|f32::from_le_bytes(v[k*4..k*4+4].try_into().unwrap()));
                for v in [-p[1]*3.+17.,p[0]*3.-12.,p[2]*3.+7.] {next.extend(v.to_le_bytes());}
            }
            rewrite.attributes.insert((i,"position$0".into()),next);
        }
    }
    let rename=|n:&mut String|{if let Some(v)=renamed.get(n){*n=v.clone();}};
    for g in &mut setup.groups {rename(&mut g.root);for n in &mut g.bones {rename(n);}for n in &mut g.weighted_bones {rename(n);}g.source_root=g.root.clone();}
    for (p,g) in setup.parts.iter_mut().zip(&setup.groups) {p.root=g.source_root.clone();for n in &mut p.weighted_chain {rename(n);}}
    for seam in &mut setup.long_skirt.as_mut().unwrap().stability.seams {for n in seam {rename(n);}}
    let changed=Dmx::parse(d.rewrite_binary9(&rewrite).unwrap()).unwrap();
    let report=attachments::analyze(&changed,&setup.groups,&setup.parts,&BTreeMap::new(),&BTreeMap::new(),setup.long_skirt.as_ref(),&CancellationToken::new()).unwrap().unwrap();
    assert_eq!(report.plan.bones.len(),expected.len());
    for (name,radius) in expected {let b=report.plan.bones.iter().find(|b|b.bone==renamed[&name]).unwrap();assert!((b.radius_inches-radius*3.).abs()<1e-6);}
    let overrides=setup.groups.iter().map(|g|(g.source_root.clone(),"cloth".into())).collect();
    assert!(attachments::analyze(&changed,&setup.groups,&setup.parts,&overrides,&BTreeMap::new(),setup.long_skirt.as_ref(),&CancellationToken::new()).unwrap().is_none());
    let profiles=setup.groups.iter().map(|g|(g.source_root.clone(),"soft".into())).collect();
    assert!(attachments::analyze(&changed,&setup.groups,&setup.parts,&BTreeMap::new(),&profiles,setup.long_skirt.as_ref(),&CancellationToken::new()).unwrap().is_none());
    println!("11 accepted controls; arbitrary names, 3x scale, rotation and translation retain selection; explicit overrides preserved");
}
