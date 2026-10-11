use deadlock_cloth_authoring::{parts::Kind, secondary::{self, Setup}, surface};
use deadlock_core::CancellationToken;
use deadlock_dmx::{Dmx, Rewrite};
use deadlock_project::{customization::SecondaryMotion, Project};
use deadlock_skeleton::extensions::SourceBinding;
use sha2::{Digest,Sha256};
use std::collections::{BTreeMap,BTreeSet};

fn binding(d: &Dmx) -> SourceBinding {
    let mut skeleton = surface::binds(d).unwrap().skeleton;
    skeleton.flags.fill(64);
    let extra = skeleton.names.iter().filter(|n| n.starts_with("dmb_secondary_")).cloned().collect::<Vec<_>>();
    SourceBinding { source_sha256: format!("{:x}",Sha256::digest(d.bytes())), original_sha256:"0".repeat(64),
        skeleton, extra_names:extra.clone(), required_extra_names:extra }
}
fn fixture() -> (Dmx,Project) {
    let d = Dmx::parse(include_bytes!("fixtures/long_skirt/panels.dmx").to_vec()).unwrap();
    let oracle: serde_json::Value = serde_json::from_str(include_str!("fixtures/long_skirt/oracle.json")).unwrap();
    let mut p = Project::default();
    p.original_model = "models/heroes/independent_fixture/model.vmdl".into();
    p.customization.enabled = Some(true);
    p.customization.secondary_motion = Some(SecondaryMotion { enabled:Some(true), preset:Some("automatic".into()), ..Default::default() });
    p.preparation_options.values.insert("preserve_physics_bones".into(),true.into());
    p.preparation_options.values.insert("physics_roots".into(),oracle["roots"].clone());
    (d,p)
}
fn setup(d: &Dmx,p: &Project) -> Setup {
    secondary::setup(p,d,Some(&binding(d)),&CancellationToken::new()).unwrap().unwrap()
}
fn values(d: &Dmx, name: &str) -> Option<Vec<f32>> {
    d.elements.iter().find_map(|e| e.attributes.iter().find(|a| a.name==name))
        .map(|a|d.raw_attribute(a)[4..].chunks_exact(4).map(|v|f32::from_le_bytes(v.try_into().unwrap())).collect())
}
fn maps(d: &Dmx) -> BTreeMap<String,Vec<[f32;8]>> {
    let bind = surface::binds(d).unwrap();
    let vertex = d.elements.iter().find(|e| e.get("cloth_enable$0").is_some()).unwrap();
    let attr = |key| d.raw_attribute(vertex.attributes.iter().find(|a| a.name==key).unwrap());
    let slots = i32::from_le_bytes(attr("jointCount").try_into().unwrap()) as usize;
    let joints:Vec<_> = attr("blendindices$0")[4..].chunks_exact(4).map(|v|i32::from_le_bytes(v.try_into().unwrap()) as usize).collect();
    let enabled = values(d,"cloth_enable$0").unwrap();
    let streams = ["cloth_goal_strength$0","cloth_drag$0","cloth_antishrink$0","cloth_gravity$0",
        "cloth_stray_radius$0","cloth_stray_radius_stretchiness$0","cloth_suspenders$0","cloth_stretch$0"]
        .map(|k|values(d,k).unwrap_or_else(||vec![0.;enabled.len()]));
    let mut result = BTreeMap::<String,Vec<[f32;8]>>::new();
    for (i,js) in joints.chunks_exact(slots).enumerate() {
        if enabled[i] > 0. {
            result.entry(d.elements[bind.palette[js[0]]].name.clone()).or_default()
                .push(std::array::from_fn(|k|streams[k][i]));
        }
    }
    result
}

#[test]
fn detects_unnamed_branched_long_panels_without_closing_slits_or_changing_other_parts() {
    let (d,mut p) = fixture();
    let after = setup(&d,&p);
    let plan = after.long_skirt.as_ref().unwrap();
    assert_eq!(plan.groups.iter().map(|g|g.root.as_str()).collect::<Vec<_>>(),["spring0","spring1","spring2","spring3"]);
    assert!(after.parts.iter().find(|p|p.root=="spring0").unwrap().weighted_chain.is_empty());
    assert!(after.requires_animation_limits());
    assert!(!plan.links.is_empty());
    assert!(plan.shear_cells.is_empty(),"Separate horizontal strips have no rendered vertical edges; skeleton proximity cannot fill their gaps.");
    let bind=surface::binds(&d).unwrap();let indices=bind.skeleton.indices();
    let links:BTreeSet<_>=plan.links.iter().map(|l|l.bones.iter().cloned().collect::<BTreeSet<_>>()).collect();
    for [pa,pb,b,a] in &plan.shear_cells {
        assert!(links.contains(&BTreeSet::from([pa.clone(),pb.clone()])));
        assert!(links.contains(&BTreeSet::from([a.clone(),b.clone()])));
        assert_eq!(bind.skeleton.parents[indices[a.as_str()]],indices[pa.as_str()] as i32);
        assert_eq!(bind.skeleton.parents[indices[b.as_str()]],indices[pb.as_str()] as i32);
        assert!([pa,pb,b,a].iter().all(|n|n.contains("spring0")||n.contains("spring1")) ||
                [pa,pb,b,a].iter().all(|n|n.contains("spring2")||n.contains("spring3")),"Cell closes a slit");
    }
    for link in &plan.links {
        let [a,b] = &link.bones;
        assert!(link.supporting_render_triangles>=2);
        assert!((a.contains("spring0")&&b.contains("spring1")) || (a.contains("spring2")&&b.contains("spring3")),"Unexpected slit/other-part connection: {a} to {b}");
    }
    let changed:BTreeSet<_> = after.groups.iter().filter(|g|g.source_root.starts_with("spring")).flat_map(|g|g.bones.iter().cloned()).collect();
    p.customization.secondary_motion.as_mut().unwrap().part_kinds = Some(plan.groups.iter().map(|g|(g.root.clone(),"cloth".into())).collect());
    let before = setup(&d,&p);
    assert!(before.long_skirt.is_none());
    let a = Dmx::parse(before.proxy).unwrap();
    let b = Dmx::parse(after.proxy).unwrap();
    assert_eq!(surface::binds(&a).unwrap().skeleton,surface::binds(&b).unwrap().skeleton);
    let old = maps(&a);let new = maps(&b);
    for (bone,rows) in &new {
        if changed.contains(bone) {
            assert_eq!(rows.len(),4,"Open patch: {bone}");
            for row in rows { assert!(row[4]>0.);assert_eq!(row[6],1.);assert_eq!(row[7],0.);
                assert!(row[0]==1. || row[0]<=0.12,"Free garment rows must not inherit the rigid cloth goal"); }
        } else { assert_eq!(rows,&old[bone],"Untouched group changed: {bone}"); }
    }
    assert_eq!(d.bytes(),include_bytes!("fixtures/long_skirt/panels.dmx"));
    assert_eq!(plan.policy,"surface-fitted-long-skirt-v7");
    assert!(!plan.stability.frames.is_empty());
    for frame in &plan.stability.frames {
        assert!(frame.along.contains(&frame.bone));
        for neighbor in frame.across.iter().filter(|n|*n!=&frame.bone) {
            assert!(links.contains(&BTreeSet::from([frame.bone.clone(),neighbor.clone()])),"Surface fit closes an unevidenced gap");
        }
    }
    assert!(!plan.stability.seams.is_empty());
    for [a,b] in &plan.stability.seams {
        assert!((a.contains("spring0")&&b.contains("spring1")) || (a.contains("spring2")&&b.contains("spring3")),"Cohesion closes a slit: {a} / {b}");
    }
    assert!(!plan.stability.hinges.is_empty());
    for h in &plan.stability.hinges {assert!(h.iter().all(|n|plan.stability.bones.contains(n)));}
}

#[test]
fn detection_and_limits_follow_body_rotation_translation_and_scale() {
    let (d,p) = fixture();
    let before = setup(&d,&p);
    let mut rewrite = Rewrite::default();
    let pelvis = d.elements.iter().position(|e|e.kind=="DmeTransform"&&e.name=="pelvis").unwrap();
    rewrite.attributes.insert((pelvis,"scale".into()),3_f32.to_le_bytes().to_vec());
    rewrite.attributes.insert((pelvis,"position".into()),[17_f32,-12.,7.].into_iter().flat_map(f32::to_le_bytes).collect());
    rewrite.attributes.insert((pelvis,"orientation".into()),[0_f32,0.,std::f32::consts::FRAC_1_SQRT_2,std::f32::consts::FRAC_1_SQRT_2].into_iter().flat_map(f32::to_le_bytes).collect());
    for (i,e) in d.elements.iter().enumerate().filter(|(_,e)|e.kind=="DmeVertexData") {
        if let Some(attr) = e.attributes.iter().find(|a|a.name=="position$0") {
            let raw = d.raw_attribute(attr);let mut next = raw[..4].to_vec();
            for v in raw[4..].chunks_exact(12) {
                let xyz:[f32;3]=std::array::from_fn(|k|f32::from_le_bytes(v[k*4..k*4+4].try_into().unwrap()));
                for n in [-xyz[1]*3.+17.,xyz[0]*3.-12.,xyz[2]*3.+7.] { next.extend(n.to_le_bytes()); }
            }
            rewrite.attributes.insert((i,"position$0".into()),next);
        }
    }
    let rotated = Dmx::parse(d.rewrite_binary9(&rewrite).unwrap()).unwrap();
    let after = setup(&rotated,&p);
    let (a,b)=(before.long_skirt.unwrap(),after.long_skirt.unwrap());
    assert_eq!(a.shear_cells,b.shear_cells);
    assert_eq!(a.stability.frames,b.stability.frames);
    assert_eq!(a.links.iter().map(|l|&l.bones).collect::<Vec<_>>(),b.links.iter().map(|l|&l.bones).collect::<Vec<_>>());
    for (a,b) in a.groups.iter().zip(&b.groups) {
        assert_eq!(a.root,b.root);
        assert!((a.weighted_height_inches*3.-b.weighted_height_inches).abs()<1e-4);
        for (name,radius) in &a.travel_radius_by_bone {
            assert!((radius*3.-b.travel_radius_by_bone[name]).abs()<1e-4);
        }
    }
}

#[test]
fn explicit_types_and_isolated_columns_require_review_instead_of_invented_seams() {
    let (d,mut p) = fixture();
    let cfg = p.customization.secondary_motion.as_mut().unwrap();
    cfg.roots = Some(vec!["spring0".into()]);
    assert!(setup(&d,&p).long_skirt.is_none());
    p.customization.secondary_motion.as_mut().unwrap().part_kinds = Some(BTreeMap::from([("spring0".into(),"long_skirt".into())]));
    assert!(secondary::setup(&p,&d,Some(&binding(&d)),&CancellationToken::new()).is_err());
    let (_,mut p) = fixture();
    p.customization.secondary_motion.as_mut().unwrap().part_kinds = Some(BTreeMap::from([("hair_a".into(),"long_skirt".into())]));
    assert!(secondary::setup(&p,&d,Some(&binding(&d)),&CancellationToken::new()).is_err());
    let (_,mut p) = fixture();
    p.customization.secondary_motion.as_mut().unwrap().part_kinds = Some(BTreeMap::from([("spring0".into(),"long_skirt".into())]));
    assert_eq!(setup(&d,&p).parts.iter().find(|p|p.root=="spring0").unwrap().kind,Kind::LongSkirt);
}
