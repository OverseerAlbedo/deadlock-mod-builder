//! Short animation-relative travel for source-evidenced upper garment shells.
//! Preserves fit bases, rods and every unselected motion record. Build and
//! Install recompute this transform from the immutable native snapshot.
use super::{fitting::{cross,dot,sub,vector},number,usize_field};
use crate::{field,field_mut,invalid,strings,Result};
use deadlock_resource::{Kind,Value};
use serde::{Deserialize,Serialize};
use std::collections::{BTreeMap,BTreeSet};

pub const POLICY:&str="attached-garment-travel-v1";
#[derive(Clone,Debug,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Restraint {
    pub bone:String,
    pub anchor:String,
    pub radius_inches:f64,
    pub region:String,
}
#[derive(Clone,Debug,Default,Serialize,Deserialize)]
pub struct Plan { pub bones:Vec<Restraint> }
#[derive(Clone,Debug,Default,Serialize,Deserialize)]
pub struct Proof { pub policy:String, pub bones:Vec<BoneProof> }
#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct BoneProof {
    pub bone:String, pub points:Vec<usize>, pub radius_inches:f64,
    pub maximum_normal_rotation_degrees:f64,
}
fn arr(v:impl IntoIterator<Item=Value>)->Value {Value::new(Kind::Array(v.into_iter().collect()))}
fn n(v:f64)->Value {Value::new(Kind::F32(v as f32))}
fn i(v:usize)->Value {Value::new(Kind::I64(v as i64))}
fn pair(row:&Value)->Result<[usize;2]> {
    let values=field(row,"nNode")?.array()?;
    if values.len()!=2 {return Err(invalid("Malformed animation travel nodes."));}
    let get=|v:&Value|usize::try_from(v.integer()?).map_err(|_|invalid("Invalid animation travel node."));
    Ok([get(&values[0])?,get(&values[1])?])
}
fn lanes(pack:&Value)->Result<Vec<[usize;2]>> {
    let nodes=field(pack,"nNode")?.array()?;
    if nodes.len()!=2 || nodes.iter().any(|v|v.array().map_or(true,|v|v.len()!=4)) {
        return Err(invalid("Malformed SIMD animation travel nodes."));
    }
    (0..4).map(|lane|pair(&Value::new(Kind::Object(vec![("nNode".into(),arr(nodes.iter().map(|v|v.array().unwrap()[lane].clone())))])))).collect()
}
pub fn verify_coverage(fe:&Value)->Result<()> {
    let count=usize_field(fe,"m_nNodeCount")?;
    let mut scalar=BTreeMap::new();
    for row in field(fe,"m_AnimStrayRadii")?.array()? {
        let nodes=pair(row)?;let radius=number(field(row,"flMaxDist")?)?;
        let relaxation=number(field(row,"flRelaxationFactor")?)?;
        if nodes.iter().any(|&p|p>=count) || radius<0. || !(0. ..=1.).contains(&relaxation)
            || scalar.insert(nodes,[radius,relaxation]).is_some() {
            return Err(invalid("Invalid or duplicate animation travel limit."));
        }
    }
    let mut seen=BTreeSet::new();
    for pack in field(fe,"m_SimdAnimStrayRadii")?.array()? {
        for key in ["flMaxDist","flRelaxationFactor"] {
            if field(pack,key)?.array()?.len()!=4 {return Err(invalid("Malformed SIMD animation travel values."));}
        }
        for (lane,nodes) in lanes(pack)?.into_iter().enumerate() {
            let expected=scalar.get(&nodes).ok_or_else(||invalid("SIMD animation limit has no scalar record."))?;
            for (key,value) in ["flMaxDist","flRelaxationFactor"].into_iter().zip(expected) {
                if number(&field(pack,key)?.array()?[lane])?!=*value {return Err(invalid("Scalar/SIMD animation travel limits disagree."));}
            }
            seen.insert(nodes);
        }
    }
    if seen!=scalar.keys().copied().collect() {return Err(invalid("Animation limits have incomplete SIMD coverage."));}
    Ok(())
}
pub fn apply(fe:&Value,plan:&Plan)->Result<(Value,Proof)> {
    let mut proof=Proof{policy:POLICY.into(),..Default::default()};
    if plan.bones.is_empty() {return Ok((fe.clone(),proof));}
    verify_coverage(fe)?;
    let names=strings(field(fe,"m_CtrlName")?)?;
    let lookup=names.iter().enumerate().map(|(i,n)|(n.as_str(),i)).collect::<BTreeMap<_,_>>();
    let parents=field(fe,"m_SkelParents")?.array()?;
    let poses=field(fe,"m_InitPose")?.array()?;
    let masses=field(fe,"m_NodeInvMasses")?.array()?;
    let static_=usize_field(fe,"m_nStaticNodes")?;
    let driven=usize_field(fe,"m_nFirstPositionDrivenNode")?;
    if names.len()!=parents.len() || names.len()!=poses.len() || names.len()!=masses.len() || static_>driven || driven>names.len() {
        return Err(invalid("Invalid attachment solver partitions."));
    }
    let pos=|p:usize|->Result<[f64;3]>{let p=vector::<8>(poses.get(p).ok_or_else(||invalid("Invalid attachment point."))?)?;Ok([p[0],p[1],p[2]])};
    let mut selected=BTreeSet::new();let mut targets=BTreeMap::<usize,f64>::new();
    for bone in &plan.bones {
        if !bone.radius_inches.is_finite() || bone.radius_inches<=0. {return Err(invalid("Invalid attachment travel radius."));}
        let &ctrl=lookup.get(bone.bone.as_str()).ok_or_else(||invalid("Missing attachment control."))?;
        let &anchor=lookup.get(bone.anchor.as_str()).ok_or_else(||invalid("Missing attachment anchor."))?;
        if ctrl<driven || anchor>=static_ || !selected.insert(ctrl) {return Err(invalid("Attachment selection overlaps or includes animated body controls."));}
        let mut at=ctrl;let mut visited=BTreeSet::new();
        while at!=anchor {
            if !visited.insert(at) {return Err(invalid("Cyclic attachment ancestry."));}
            at=parents.get(at).and_then(|v|v.integer().ok()).and_then(|v|usize::try_from(v).ok())
                .filter(|&v|v<parents.len()).ok_or_else(||invalid("Attachment does not descend from its source anchor."))?;
        }
        let supports=field(fe,"m_CtrlOffsets")?.array()?.iter()
            .filter(|r|usize_field(r,"nCtrlParent").ok()==Some(ctrl))
            .map(|r|usize_field(r,"nCtrlChild")).collect::<Result<Vec<_>>>()?;
        if supports.len()!=3 || supports.iter().any(|&p|p<static_ || p>=driven || parents[p].integer().ok()!=Some(ctrl as i128) || number(&masses[p]).map_or(true,|v|v<=0.)) {
            return Err(invalid("Attachment requires a verified three-point dynamic support."));
        }
        let bases=field(fe,"m_NodeBases")?.array()?.iter().filter(|r|usize_field(r,"nNode").ok()==Some(ctrl)).collect::<Vec<_>>();
        if bases.len()!=1 {return Err(invalid("Attachment needs one compact orientation fit."));}
        let ids=["nNodeX0","nNodeX1","nNodeY0","nNodeY1"].map(|k|usize_field(bases[0],k)).into_iter().collect::<Result<Vec<_>>>()?;
        if ids.iter().any(|p|!supports.contains(p)) {return Err(invalid("Attachment fit uses another part's moving supports."));}
        let x=sub(pos(ids[1])?,pos(ids[0])?);let y=sub(pos(ids[3])?,pos(ids[2])?);
        let length=|v|dot(v,v).sqrt();let area=length(cross(x,y));
        if area<1e-12 {return Err(invalid("Collapsed attachment support."));}
        // Bound |delta(x cross y)| <= 2r(|x|+|y|)+4r². Keep it below
        // 45% of rest area, excluding collapse/inversion once limits converge.
        let b=2.*(length(x)+length(y));
        let safe=0.9*area/(b+(b*b+7.2*area).sqrt());
        let radius=bone.radius_inches.min(safe);
        for &point in &supports {
            if targets.insert(point,radius).is_some() {return Err(invalid("Attachment supports have shared ownership."));}
        }
        let error=2.*radius*(length(x)+length(y))+4.*radius*radius;
        proof.bones.push(BoneProof{bone:bone.bone.clone(),points:supports,radius_inches:radius,
            maximum_normal_rotation_degrees:(error/area).clamp(0.,1.).asin().to_degrees()});
    }
    let mut out=fe.clone();let mut integrators=field(fe,"m_NodeIntegrator")?.array()?.to_vec();
    if integrators.len()!=names.len() {return Err(invalid("Incomplete attachment integrators."));}
    for &point in targets.keys() {
        for (key,value) in [("flPointDamping",11.4),("flAnimationForceAttraction",0.5),("flAnimationVertexAttraction",0.5)] {
            *field_mut(&mut integrators[point],key)?=n(number(field(&integrators[point],key)?)?.max(value));
        }
    }
    *field_mut(&mut out,"m_NodeIntegrator")?=arr(integrators);
    let mut scalar=field(fe,"m_AnimStrayRadii")?.array()?.to_vec();let mut existing=BTreeSet::new();
    for row in &mut scalar {
        let nodes=pair(row)?;
        if let Some(radius)=targets.get_mut(&nodes[0]) {
            if nodes[0]!=nodes[1] {return Err(invalid("Attachment has a non-animation travel reference."));}
            *radius=radius.min(number(field(row,"flMaxDist")?)?);
            *field_mut(row,"flMaxDist")?=n(*radius);*field_mut(row,"flRelaxationFactor")?=n(1.);existing.insert(nodes[0]);
        }
    }
    let mut packs=field(fe,"m_SimdAnimStrayRadii")?.array()?.to_vec();
    for pack in &mut packs {
        let pairs=lanes(pack)?;
        for key in ["flMaxDist","flRelaxationFactor"] {
            let mut values=field(pack,key)?.array()?.to_vec();
            for (lane,nodes) in pairs.iter().enumerate() {if let Some(&radius)=targets.get(&nodes[0]) {
                values[lane]=n(if key=="flMaxDist" {radius} else {1.});
            }}
            *field_mut(pack,key)?=arr(values);
        }
    }
    let additions=targets.iter().filter(|(p,_)|!existing.contains(p)).map(|(&point,&radius)|Value::new(Kind::Object(vec![
        ("nNode".into(),arr([i(point),i(point)])),("flMaxDist".into(),n(radius)),("flRelaxationFactor".into(),n(1.))
    ]))).collect::<Vec<_>>();
    for chunk in additions.chunks(4) {
        let lanes=(0..4).map(|j|&chunk[j.min(chunk.len()-1)]).collect::<Vec<_>>();
        let mut fields=vec![("nNode".into(),arr((0..2).map(|axis|arr(lanes.iter().map(|r|field(r,"nNode").unwrap().array().unwrap()[axis].clone())))))];
        for key in ["flMaxDist","flRelaxationFactor"] {fields.push((key.into(),arr(lanes.iter().map(|r|field(r,key).unwrap().clone()))));}
        packs.push(Value::new(Kind::Object(fields)));
    }
    scalar.extend(additions);*field_mut(&mut out,"m_AnimStrayRadii")?=arr(scalar);
    *field_mut(&mut out,"m_SimdAnimStrayRadii")?=arr(packs);
    verify_coverage(&out)?;
    Ok((out,proof))
}
