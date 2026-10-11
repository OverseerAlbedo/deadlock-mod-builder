//! Shared cloth orientation, as opposed to independently rotating bone patches.
//! Only source-evidenced neighbors may supply axes. Positions, offsets, masses,
//! rods and motion maps are unchanged; every rest rotation is reconstructed.
use super::{fitting::{cross,dot,rotation,sub,unit,vector},number,roll_lock,usize_field};
use crate::{field,field_mut,invalid,strings,Result};
use deadlock_resource::{Kind,Value};
use serde::{Deserialize,Serialize};
use std::collections::{BTreeMap,BTreeSet};

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
pub struct Frame {
    pub bone:String,
    /// Immediate parent/child pair; includes this bone's position support.
    pub along:[String;2],
    /// One-sided at a real edge, spanning both neighbors inside the sheet.
    pub across:[String;2],
}
#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct FrameProof {
    pub bone:String,
    pub along:[usize;2],
    pub across:[usize;2],
    pub rest_axis_angle_degrees:f64,
}
fn pair<T:Ord>(a:T,b:T)->[T;2] {if a<b {[a,b]}else{[b,a]}}

pub(super) fn apply(fe:&Value,selected:&[String],seams:&[[String;2]],frames:&[Frame])
    ->Result<(Option<Value>,Vec<FrameProof>)> {
    if frames.is_empty(){return Ok((None,vec![]));}
    let names=strings(field(fe,"m_CtrlName")?)?;
    let poses=field(fe,"m_InitPose")?.array()?;
    let parents=field(fe,"m_SkelParents")?.array()?;
    let masses=field(fe,"m_NodeInvMasses")?.array()?;
    let static_=usize_field(fe,"m_nStaticNodes")?;
    let driven=usize_field(fe,"m_nFirstPositionDrivenNode")?;
    if names.len()!=poses.len() || names.len()!=parents.len() || names.len()!=masses.len()
        || static_>driven || driven>names.len() {return Err(invalid("Invalid surface fitting partitions."));}
    if !field(fe,"m_SimdNodeBases")?.array()?.is_empty() {
        return Err(invalid("Surface fitting needs verified scalar native bone bases."));
    }
    let lookup=names.iter().enumerate().map(|(i,n)|(n.as_str(),i)).collect::<BTreeMap<_,_>>();
    let selected=selected.iter().map(String::as_str).collect::<BTreeSet<_>>();
    let seams=seams.iter().map(|s|pair(s[0].as_str(),s[1].as_str())).collect::<BTreeSet<_>>();
    let mut reverse=BTreeMap::new();
    for row in field(fe,"m_ReverseOffsets")?.array()? {
        if reverse.insert(usize_field(row,"nBoneCtrl")?,usize_field(row,"nTargetNode")?).is_some() {
            return Err(invalid("Duplicate surface position fit."));
        }
    }
    let mut connected=BTreeSet::new();
    for rod in field(fe,"m_Rods")?.array()? {
        let nodes=field(rod,"nNode")?.array()?;
        if nodes.len()!=2 {return Err(invalid("Invalid surface support rod."));}
        let a=usize::try_from(nodes[0].integer()?).map_err(|_|invalid("Invalid surface support."))?;
        let b=usize::try_from(nodes[1].integer()?).map_err(|_|invalid("Invalid surface support."))?;
        if a>=driven || b>=driven {return Err(invalid("Surface rod uses a fitted output."));}
        if number(field(rod,"flRelaxationFactor")?)?>0. {
            connected.insert(pair(parents[a].integer()?,parents[b].integer()?));
        }
    }
    let mut bases=field(fe,"m_NodeBases")?.array()?.to_vec();
    let mut rows=BTreeMap::new();
    for (i,row) in bases.iter().enumerate() {
        if rows.insert(usize_field(row,"nNode")?,i).is_some(){return Err(invalid("Duplicate surface bone basis."));}
    }
    let pos=|i:usize|->Result<[f64;3]>{let p=vector::<8>(&poses[i])?;Ok([p[0],p[1],p[2]])};
    let control=|name:&str|->Result<(usize,usize)>{
        let &ctrl=lookup.get(name).ok_or_else(||invalid(format!("Missing surface control: {name}")))?;
        let &point=reverse.get(&ctrl).ok_or_else(||invalid(format!("Missing compact surface position fit: {name}")))?;
        if !selected.contains(name) || ctrl<driven || point<static_ || point>=driven
            || parents[point].integer()?!=ctrl as i128 || number(&masses[point])?<=0. {
            return Err(invalid("Surface fit escapes its selected fabric supports."));
        }
        Ok((ctrl,point))
    };
    let mut seen=BTreeSet::new();let mut proofs=Vec::new();
    for f in frames {
        let (ctrl,point)=control(&f.bone)?;
        if !seen.insert(ctrl) || f.along[0]==f.along[1] || f.across[0]==f.across[1]
            || !f.along.contains(&f.bone) {return Err(invalid("Invalid or duplicate fabric surface frame."));}
        let mut along=[0;2];let mut across=[0;2];
        for (i,name) in f.along.iter().enumerate() {
            let (other,p)=control(name)?;along[i]=p;
            if other!=ctrl && (parents[ctrl].integer()?!=other as i128 && parents[other].integer()?!=ctrl as i128) {
                return Err(invalid("Surface tangent crosses an unrelated skeleton branch."));
            }
        }
        for (i,name) in f.across.iter().enumerate() {
            let (_,p)=control(name)?;across[i]=p;
            if name!=&f.bone && !seams.contains(&pair(f.bone.as_str(),name.as_str())) {
                return Err(invalid("Surface width crosses a seam without render evidence."));
            }
        }
        for name in f.along.iter().chain(&f.across).filter(|n|*n!=&f.bone) {
            let (other,_)=control(name)?;
            if !connected.contains(&pair(ctrl as i128,other as i128)) {
                return Err(invalid("Surface frame crosses disconnected native fabric."));
            }
        }
        if !along.contains(&point){return Err(invalid("Surface frame lost its own position support."));}
        let raw_x=sub(pos(across[1])?,pos(across[0])?);
        let y=unit(sub(pos(along[1])?,pos(along[0])?))?;
        let transverse=sub(raw_x,y.map(|v|v*dot(raw_x,y)));
        let ratio=dot(transverse,transverse)/dot(raw_x,raw_x);
        if !ratio.is_finite() || ratio<0.25 {
            return Err(invalid(format!("Fabric surface axes are too nearly parallel: {}",f.bone)));
        }
        let x=unit(transverse)?;let z=cross(x,y);
        let basis:[[f64;3];3]=std::array::from_fn(|r|[x[r],y[r],z[r]]);
        let rest=vector::<8>(&poses[ctrl])?;
        let r=rotation([rest[4],rest[5],rest[6],rest[7]])?;
        let adjust=roll_lock::quat(std::array::from_fn(|a|std::array::from_fn(|c|
            (0..3).map(|k|basis[k][a]*r[k][c]).sum())));
        let row=&mut bases[*rows.get(&ctrl).ok_or_else(||invalid("Fabric surface lacks a native compact bone basis."))?];
        for (key,node) in ["nNodeX0","nNodeX1","nNodeY0","nNodeY1"].into_iter().zip([across[0],across[1],along[0],along[1]]) {
            *field_mut(row,key)?=Value::new(Kind::I64(node as i64));
        }
        *field_mut(row,"qAdjust")?=Value::new(Kind::Array(adjust.into_iter().map(|q|Value::new(Kind::F32(q as f32))).collect()));
        proofs.push(FrameProof{bone:f.bone.clone(),along,across,rest_axis_angle_degrees:ratio.clamp(0.,1.).sqrt().asin().to_degrees()});
    }
    Ok((Some(Value::new(Kind::Array(bases))),proofs))
}
