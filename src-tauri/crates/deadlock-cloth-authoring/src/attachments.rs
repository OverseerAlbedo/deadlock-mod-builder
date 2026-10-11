//! Body-relative upper garment attachments. Free strands and explicit part
//! choices are excluded before inspecting mesh weights. Bone names never
//! identify a character or authorize a restraint.
use crate::{parts::{Kind,Part},secondary::Group,surface,
    surface_geometry::{cross,dot,norm,sub},Result};
use deadlock_cloth::generated::attachment_limits::{Plan,Restraint};
use deadlock_core::CancellationToken;
use deadlock_dmx::Dmx;
use deadlock_skeleton::transforms;
use serde::Serialize;
use std::collections::{BTreeMap,BTreeSet};

pub const POLICY:&str="weighted-upper-attachments-v1";
#[derive(Debug,Serialize)]
pub struct Evidence {
    pub bone:String, pub region:String, pub supporting_positions:usize,
    pub weighted_height_fraction:[f64;2], pub width_fraction:f64,
    pub posterior_center_fraction:f64,
}
#[derive(Debug,Serialize)]
pub struct Report {pub policy:String,pub plan:Plan,pub evidence:Vec<Evidence>}

pub fn analyze(d:&Dmx,groups:&[Group],parts:&[Part],overrides:&BTreeMap<String,String>,
    profiles:&BTreeMap<String,String>,skirt:Option<&crate::long_skirt::Plan>,cancel:&CancellationToken)->Result<Option<Report>> {
    let bind=surface::binds(d)?;let sk=&bind.skeleton;let lookup=sk.indices();
    let named=|n:&str|sk.names.iter().position(|v|v.eq_ignore_ascii_case(n));
    let (Some(head),Some(pelvis),Some(left),Some(right))=(named("head"),named("pelvis"),named("arm_upper_L"),named("arm_upper_R")) else {return Ok(None);};
    let state=transforms::bind_state(sk)?;
    let pos=|i:usize|std::array::from_fn::<_,3,_>(|r|state.world[i][r][3]);
    let torso=norm(sub(pos(head),pos(pelvis)));
    if torso<=1e-4 {return Ok(None);}
    let up=sub(pos(head),pos(pelvis)).map(|v|v/torso);
    let lateral=sub(pos(left),pos(right));let side=sub(lateral,up.map(|v|v*dot(lateral,up)));
    if norm(side)<torso*0.1 {return Ok(None);}
    let side=side.map(|v|v/norm(side));let forward=cross(side,up);
    let height=|p|dot(sub(p,pos(pelvis)),up)/torso;
    let mut body=BTreeSet::new();let mut at=head as i32;
    while at>=0 {body.insert(at as usize);if at as usize==pelvis {break;}at=sk.parents[at as usize];}
    if !body.contains(&pelvis) {return Ok(None);}
    let connected=skirt.map(|s|s.stability.seams.iter().flatten().map(String::as_str).collect::<BTreeSet<_>>()).unwrap_or_default();
    let eligible=groups.iter().zip(parts).filter(|(g,p)|
        matches!(p.kind,Kind::Cloth|Kind::Accessory) && (p.kind!=Kind::Accessory || p.review_recommended)
        && !overrides.contains_key(&g.source_root) && !profiles.contains_key(&g.source_root)
        && body.contains(&lookup[g.anchor.as_str()]) && height(pos(lookup[g.anchor.as_str()]))>=0.3)
        .flat_map(|(g,_)|g.weighted_bones.iter().map(|n|lookup[n.as_str()])).collect::<BTreeSet<_>>();
    if eligible.is_empty() {return Ok(None);}
    let palette=bind.palette.iter().map(|&e|lookup.get(d.elements[e].name.as_str()).copied()).collect::<Vec<_>>();
    let mut samples=BTreeMap::<usize,Vec<([f64;3],f64)>>::new();
    for mesh in surface::mesh_data(d)?.values() {
        for (i,&p) in mesh.positions.iter().enumerate() {
            if i%4096==0 {cancel.check()?;}if !mesh.drawn[i] {continue;}
            for (&slot,&weight) in mesh.joints[i].iter().zip(&mesh.weights[i]) {
                if weight<0.01 {continue;}
                if let Some(bone)=palette.get(slot).copied().flatten().filter(|b|eligible.contains(b)) {
                    samples.entry(bone).or_default().push((p,weight));
                }
            }
        }
    }
    let mut report=Report{policy:POLICY.into(),plan:Plan::default(),evidence:vec![]};
    for (group,part) in groups.iter().zip(parts) {
        let connected_panel=part.kind==Kind::Cloth && group.weighted_bones.iter().any(|n|connected.contains(n.as_str()));
        let mut accepted=BTreeMap::<usize,usize>::new();
        for name in &group.bones {
            let bone=lookup[name.as_str()];let Some(points)=samples.get(&bone) else {continue;};
            if points.len()<12 {continue;}
            let mut heights=points.iter().map(|(p,_)|height(*p)).collect::<Vec<_>>();heights.sort_by(f64::total_cmp);
            let low=heights[(heights.len()-1)*5/100];let high=heights[(heights.len()-1)*95/100];
            let middle=heights[heights.len()/2];
            let width=points.iter().map(|(p,_)|dot(sub(*p,pos(pelvis)),side)/torso).collect::<Vec<_>>();
            let lo=width.iter().copied().fold(f64::INFINITY,f64::min);
            let hi=width.iter().copied().fold(f64::NEG_INFINITY,f64::max);
            let posterior=points.iter().map(|(p,w)|dot(sub(*p,pos(pelvis)),forward)/torso*w).sum::<f64>()/points.iter().map(|(_,w)|w).sum::<f64>();
            // A broad folded shell straddles the back of the upper torso.
            // Narrow strings on the same rig are evaluated separately.
            let shell=points.len()>=32 && low>=0.32 && high<=1.1 && middle>=0.40
                && high-low<=0.65 && hi-lo>=0.35 && lo< -0.1 && hi>0.1 && posterior< -0.02;
            // Only a short rear bodice with an evidenced seam into the skirt
            // qualifies. Long cape surfaces, front lapels and sparse weights
            // on low decorations cannot turn a whole rig into rigid cloth.
            let panel=connected_panel && low>=0.04 && high<=0.65 && high-low<=0.45
                && hi-lo>=0.12 && posterior< -0.02;
            if !shell && !panel {continue;}
            let mut parent=sk.parents[bone];let mut depth=0;
            while parent>=0 && parent!=lookup[group.anchor.as_str()] as i32 {
                if let Some(&rank)=accepted.get(&(parent as usize)) {depth=rank+1;break;}
                parent=sk.parents[parent as usize];
            }
            if (shell && depth>=2) || (!shell && depth>=3) {continue;}
            let region=if shell {"upper_shell"}else{"upper_back_panel"};
            let fraction=if shell {[0.001,0.0014][depth]}else{[0.001,0.0016,0.0026][depth]};
            report.plan.bones.push(Restraint{bone:name.clone(),anchor:group.anchor.clone(),radius_inches:torso*fraction,region:region.into()});
            report.evidence.push(Evidence{bone:name.clone(),region:region.into(),supporting_positions:points.len(),
                weighted_height_fraction:[low,high],width_fraction:hi-lo,posterior_center_fraction:posterior});
            accepted.insert(bone,depth);
        }
    }
    Ok((!report.plan.bones.is_empty()).then_some(report))
}
