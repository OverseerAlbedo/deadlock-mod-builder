//! Local fabric supports must remain rigid enough to fit a bone. Cloth may
//! bend freely between supports, up to a bounded extra fold at each joint.
//! Every change is recomputed from the immutable native solver on installation.
use super::{fitting::{dot, sub, vector}, number, usize_field};
use crate::{field, field_mut, invalid, strings, Result};
use deadlock_resource::{Kind, Value};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const POLICY: &str = "surface-fitted-fabric-v4";
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Plan {
    pub bones: Vec<String>,
    /// Source-authorized consecutive fabric rows, never proximity-only seams.
    pub hinges: Vec<[String; 3]>,
    /// Source-rendered seams, including connections to shorter coat panels.
    #[serde(default)]
    pub seams: Vec<[String; 2]>,
    /// Fit broad fabric from its neighboring rows, not independent tiny cages.
    #[serde(default)]
    pub frames: Vec<super::sheet_frames::Frame>,
    #[serde(default)]
    pub attachments: super::attachment_limits::Plan,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Proof {
    pub policy: String,
    pub rigid_support_rods: usize,
    pub bend_limit_rods: usize,
    #[serde(default)]
    pub seam_limit_rods: usize,
    pub added_rods: Vec<[usize; 2]>,
    pub tightened_native_rods: Vec<[usize; 2]>,
    pub original_records_preserved: bool,
    #[serde(default)]
    pub surface_frames: Vec<super::sheet_frames::FrameProof>,
    #[serde(default)]
    pub attachments: super::attachment_limits::Proof,
}
fn n(v: f64) -> Value { Value::new(Kind::F32(v as f32)) }
fn i(v: usize) -> Value { Value::new(Kind::I64(v as i64)) }
fn arr(v: impl IntoIterator<Item=Value>) -> Value { Value::new(Kind::Array(v.into_iter().collect())) }
fn pair(a: usize,b: usize) -> [usize;2] { if a<b {[a,b]} else {[b,a]} }
fn length(v: [f64;3]) -> f64 { dot(v,v).sqrt() }

pub fn apply(fe: &Value, plan: &Plan) -> Result<(Value,Proof)> {
    let mut out=fe.clone();
    let mut proof=Proof {policy:POLICY.into(),original_records_preserved:true,..Default::default()};
    if plan.bones.is_empty() {
        if !plan.hinges.is_empty() || !plan.seams.is_empty() || !plan.frames.is_empty() {return Err(invalid("Fabric constraints have no selected fabric."));}
        let (limited,attachments)=super::attachment_limits::apply(&out,&plan.attachments)?;
        out=limited;proof.original_records_preserved=attachments.bones.is_empty();proof.attachments=attachments;
        return Ok((out,proof));
    }
    let names=strings(field(fe,"m_CtrlName")?)?;
    let poses=field(fe,"m_InitPose")?.array()?;
    let parents=field(fe,"m_SkelParents")?.array()?;
    let masses=field(fe,"m_NodeInvMasses")?.array()?;
    let static_=usize_field(fe,"m_nStaticNodes")?;
    let driven=usize_field(fe,"m_nFirstPositionDrivenNode")?;
    if names.len()!=poses.len() || poses.len()!=parents.len() || poses.len()!=masses.len() || static_>driven || driven>poses.len() {
        return Err(invalid("Invalid fabric solver partitions."));
    }
    let lookup:BTreeMap<_,_>=names.iter().enumerate().map(|(i,n)|(n.as_str(),i)).collect();
    let mut controls=BTreeMap::new();
    let mut points=BTreeMap::new();
    let mut reverse=BTreeMap::new();
    for row in field(fe,"m_ReverseOffsets")?.array()? {
        if reverse.insert(usize_field(row,"nBoneCtrl")?,usize_field(row,"nTargetNode")?).is_some() {return Err(invalid("Duplicate fabric position fit."));}
    }
    let mut matrix_supports=BTreeMap::new();
    if let Some(matrices)=fe.get("m_FitMatrices") {
        let weights=field(fe,"m_FitWeights")?.array()?;let mut begin=0;
        for row in matrices.array()? {
            let end=usize_field(row,"nEnd")?;let dynamic=usize_field(row,"nBeginDynamic")?;
            if begin>dynamic || dynamic>end || end>weights.len(){return Err(invalid("Invalid fabric matrix fit range."));}
            let mut nodes=BTreeSet::new();
            for weight in &weights[dynamic..end] {if number(field(weight,"flWeight")?)?>0. {nodes.insert(usize_field(weight,"nNode")?);}}
            if matrix_supports.insert(usize_field(row,"nNode")?,nodes).is_some(){return Err(invalid("Duplicate fabric matrix fit."));}
            begin=end;
        }
        if begin!=weights.len(){return Err(invalid("Unbound fabric matrix weights."));}
    }
    for name in &plan.bones {
        let &ctrl=lookup.get(name.as_str()).ok_or_else(||invalid(format!("Fabric native control is missing: {name}")))?;
        if ctrl<driven || controls.insert(name.as_str(),ctrl).is_some() {return Err(invalid("Fabric controls overlap or include an animated body control."));}
        let owned=(static_..driven).filter(|&p|parents[p].integer().ok()==Some(ctrl as i128)).collect::<Vec<_>>();
        if !(3..=8).contains(&owned.len()) || owned.iter().any(|&p|number(&masses[p]).map_or(true,|m|m<=0.)) {
            return Err(invalid(format!("Fabric bone needs a verified local fitting support: {name}")));
        }
        let point=if let Some(&point)=reverse.get(&ctrl) {point} else {
            let fitted=matrix_supports.get(&ctrl).ok_or_else(||invalid("Fabric control has no verified fitting support."))?;
            if fitted!=&owned.iter().copied().collect(){return Err(invalid("Fabric matrix fit shares another bone's support."));}
            let rest=vector::<8>(&poses[ctrl])?;
            let mut closest=None;
            for &point in &owned {
                let p=vector::<8>(&poses[point])?;let distance=(0..3).map(|i|(p[i]-rest[i]).powi(2)).sum::<f64>();
                if closest.is_none_or(|(best,_)|distance<best){closest=Some((distance,point));}
            }
            let point=closest.unwrap().1;reverse.insert(ctrl,point);point
        };
        if !owned.contains(&point) {return Err(invalid("Fabric position fit belongs to another control."));}
        points.insert(name.as_str(),owned);
    }
    let pos=|node:usize|->Result<[f64;3]>{let p=vector::<8>(&poses[node])?;Ok([p[0],p[1],p[2]])};
    let mut rods=field(fe,"m_Rods")?.array()?.to_vec();
    let mut existing=BTreeMap::new();
    let mut adjacency=vec![BTreeSet::new();driven];
    for rod in &rods {
        let v=field(rod,"nNode")?.array()?;
        if v.len()!=2 {return Err(invalid("Malformed fabric support rod."));}
        let a=usize::try_from(v[0].integer()?).map_err(|_|invalid("Invalid fabric rod node."))?;
        let b=usize::try_from(v[1].integer()?).map_err(|_|invalid("Invalid fabric rod node."))?;
        if a>=driven || b>=driven {return Err(invalid("Fabric rods include fitted outputs."));}
        adjacency[a].insert(b);adjacency[b].insert(a);
        let min=number(field(rod,"flMinDist")?)?;
        let max=number(field(rod,"flMaxDist")?)?;
        if !min.is_finite() || !max.is_finite() || min<0. || max<min {return Err(invalid("Invalid native fabric rod interval."));}
        let bounds=existing.entry(pair(a,b)).or_insert([min,max]);
        bounds[0]=bounds[0].max(min);bounds[1]=bounds[1].min(max);
        if bounds[0]>bounds[1] {return Err(invalid("Existing native fabric rods have incompatible limits."));}
    }
    // A separate position centre must not turn an unrelated component into a
    // fabric support. Each owned frame must already be connected internally.
    for owned in points.values() {
        let mut reached=BTreeSet::new();let mut todo=vec![owned[0]];
        while let Some(p)=todo.pop(){if reached.insert(p){todo.extend(adjacency[p].iter().copied().filter(|q|owned.contains(q)));}}
        if reached.len()!=owned.len(){return Err(invalid("Fabric fitting support is disconnected."));}
    }
    let mut additions=Vec::new();
    let native_pairs=existing.keys().copied().collect::<BTreeSet<_>>();
    let mut tightened=BTreeMap::new();
    let mut add=|a:usize,b:usize,min:f64,max:f64|->Result<bool>{
        let [a,b]=pair(a,b);
        if a==b || min<=0. || max<min || !max.is_finite(){return Err(invalid("Collapsed fabric fold constraint."));}
        if let Some(old)=existing.get_mut(&[a,b]) {
            // Broad native intervals are compatible with a narrower fold
            // limit. Intersect them in place instead of adding duplicate rods.
            // Existing constraints are never loosened, and rounding at a rest
            // endpoint uses the native endpoint rather than changing its bound.
            let (mut lo,mut hi)=(old[0].max(min),old[1].min(max));
            if lo>hi {
                let tolerance=1e-5*lo.abs().max(hi.abs()).max(1.);
                if lo-hi>tolerance {
                    return Err(invalid(format!("Fabric fold constraint has disjoint native limits for {} / {}: native [{}, {}], requested [{min}, {max}].",names[a],names[b],old[0],old[1])));
                }
                let endpoint=if old[1]<min {old[1]} else {old[0]};lo=endpoint;hi=endpoint;
            }
            if lo>old[0] || hi<old[1] {*old=[lo,hi];tightened.insert([a,b],[lo,hi]);}
            return Ok(false);
        }
        let ma=number(&masses[a])?;let mb=number(&masses[b])?;
        let rod=Value::new(Kind::Object(vec![
            ("nNode".into(),arr([i(a),i(b)])),("flMaxDist".into(),n(max)),("flMinDist".into(),n(min)),
            ("flWeight0".into(),n(ma/(ma+mb))),("flRelaxationFactor".into(),n(1.))]));
        existing.insert([a,b],[min,max]);additions.push(rod);proof.added_rods.push([a,b]);Ok(true)
    };
    // Complete a local fitting frame's diagonals. This prevents folding INSIDE
    // one bone's four-point patch, without bracing one bone rigidly to another.
    for owned in points.values() {for (at,&a) in owned.iter().enumerate(){for &b in &owned[at+1..]{
        let distance=length(sub(pos(a)?,pos(b)?));
        if distance>1e-6 && add(a,b,distance,distance)? {proof.rigid_support_rods+=1;}
    }}}
    // Bound separation at each source-evidenced seam. The first rod connects
    // position supports; the second bounds the nearest remaining patch edges,
    // preventing independent frame rotation from opening the visible fabric.
    // Only extension is constrained: these rods allow compression and bending,
    // and never weld unrelated layers or a deliberately open slit.
    let mut used_seams=BTreeSet::new();
    for seam in &plan.seams {
        if seam[0]==seam[1] || seam.iter().any(|n|!controls.contains_key(n.as_str())) {
            return Err(invalid("Fabric seam escapes its selected cloth."));
        }
        let centers=seam.each_ref().map(|n|reverse[&controls[n.as_str()]]);
        if !used_seams.insert(pair(centers[0],centers[1])) {continue;}
        let mut candidates=Vec::new();
        for &a in &points[seam[0].as_str()] {for &b in &points[seam[1].as_str()] {
            if a==centers[0] || b==centers[1] {continue;}
            let distance=length(sub(pos(a)?,pos(b)?));
            if distance>1e-6 {candidates.push((distance,a,b));}
        }}
        let (_,a,b)=candidates.into_iter().min_by(|a,b|a.0.total_cmp(&b.0).then_with(||(a.1,a.2).cmp(&(b.1,b.2))))
            .ok_or_else(||invalid("Fabric seam has no distinct edge supports."))?;
        for [a,b] in [centers,[a,b]] {
            let distance=length(sub(pos(a)?,pos(b)?));
            if distance<=1e-6 {return Err(invalid("Collapsed fabric seam."));}
            if add(a,b,distance*0.01,distance*1.02)? {proof.seam_limit_rods+=1;}
        }
    }
    let mut used_hinges=BTreeSet::new();
    for hinge in &plan.hinges {
        if hinge.iter().collect::<BTreeSet<_>>().len()!=3 || hinge.iter().any(|n|!controls.contains_key(n.as_str())) {
            return Err(invalid("Fabric hinge escapes its selected cloth."));
        }
        let nodes=hinge.each_ref().map(|n|reverse[&controls[n.as_str()]]);
        if !used_hinges.insert(pair(nodes[0],nodes[2])) {continue;}
        for band in hinge.windows(2) {
            if !points[band[0].as_str()].iter().any(|a|points[band[1].as_str()].iter().any(|b|adjacency[*a].contains(b))) {
                return Err(invalid("Fabric hinge crosses a disconnected seam."));
            }
        }
        let first=sub(pos(nodes[1])?,pos(nodes[0])?);let second=sub(pos(nodes[2])?,pos(nodes[1])?);
        let (a,b)=(length(first),length(second));
        if a<1e-6 || b<1e-6 {return Err(invalid("Collapsed fabric row."));}
        let rest=(dot(first,second)/(a*b)).clamp(-1.,1.).acos();
        // Free within 60 degrees of its own rest bend. Strongly curved rest
        // rows are not straightened. The upper distance allows full unfolding.
        let limit=(rest+std::f64::consts::FRAC_PI_3).min(std::f64::consts::PI-0.05);
        let min=(a*a+b*b+2.*a*b*limit.cos()).max(0.).sqrt().min(length(sub(pos(nodes[2])?,pos(nodes[0])?)));
        if add(nodes[0],nodes[2],min,a+b)? {proof.bend_limit_rods+=1;}
    }
    // Keep the compiler's scalar and SIMD schedules intact; append separate
    // packs, duplicating only the last new rod to fill its final SIMD lane.
    let mut packs=field(fe,"m_SimdRods")?.array()?.to_vec();
    // Preserve topology, ordering, masses, relaxation and every unrelated
    // field. Update all copies of a selected pair, including padded SIMD lanes.
    for rod in rods.iter_mut().chain(additions.iter_mut()) {
        let nodes=field(rod,"nNode")?.array()?;
        let key=pair(nodes[0].integer()? as usize,nodes[1].integer()? as usize);
        if let Some([min,max])=tightened.get(&key) {
            *field_mut(rod,"flMinDist")?=n(*min);*field_mut(rod,"flMaxDist")?=n(*max);
        }
    }
    let mut updated_simd=BTreeSet::new();
    for pack in &mut packs {
        let nodes=field(pack,"nNode")?.array()?;
        if nodes.len()!=2 || nodes.iter().any(|v|v.array().map_or(true,|v|v.len()!=4)) {return Err(invalid("Malformed fabric SIMD rod nodes."));}
        let keys=(0..4).map(|lane|Ok(pair(nodes[0].array()?[lane].integer()? as usize,nodes[1].array()?[lane].integer()? as usize))).collect::<Result<Vec<_>>>()?;
        for (key,index) in [("f4MinDist",0),("f4MaxDist",1)] {
            let mut values=field(pack,key)?.array()?.to_vec();
            if values.len()!=4 {return Err(invalid("Malformed fabric SIMD rod bounds."));}
            for (lane,pair) in keys.iter().enumerate() {if let Some(bounds)=tightened.get(pair) {values[lane]=n(bounds[index]);updated_simd.insert(*pair);}}
            *field_mut(pack,key)?=arr(values);
        }
    }
    proof.tightened_native_rods=tightened.keys().filter(|p|native_pairs.contains(*p)).copied().collect();
    if proof.tightened_native_rods.iter().any(|p|!updated_simd.contains(p)) {return Err(invalid("Native fabric rod has no matching SIMD constraint."));}
    proof.original_records_preserved=proof.tightened_native_rods.is_empty();
    let mut pending=(0..additions.len()).collect::<Vec<_>>();
    while !pending.is_empty() {
        let mut used=BTreeSet::new();let mut chosen=Vec::new();
        pending.retain(|&index|{
            let endpoints=field(&additions[index],"nNode").unwrap().array().unwrap();
            let a=endpoints[0].integer().unwrap();let b=endpoints[1].integer().unwrap();
            if chosen.len()==4 || used.contains(&a) || used.contains(&b){return true;}
            used.extend([a,b]);chosen.push(index);false
        });
        let lanes=(0..4).map(|at|&additions[chosen[at.min(chosen.len()-1)]]).collect::<Vec<_>>();
        let mut fields=vec![("nNode".into(),arr((0..2).map(|axis|arr(lanes.iter().map(|r|field(r,"nNode").unwrap().array().unwrap()[axis].clone())))))];
        for key in ["MaxDist","MinDist","Weight0","RelaxationFactor"] {
            fields.push((format!("f4{key}"),arr(lanes.iter().map(|r|field(r,&format!("fl{key}")).unwrap().clone()))));
        }
        packs.push(Value::new(Kind::Object(fields)));
    }
    rods.extend(additions);
    *field_mut(&mut out,"m_Rods")?=arr(rods);
    *field_mut(&mut out,"m_SimdRods")?=arr(packs);
    // Validate surface adjacency against the native input, before newly added
    // rods could appear to authorize an otherwise disconnected reference.
    let (bases,frames)=super::sheet_frames::apply(fe,&plan.bones,&plan.seams,&plan.frames)?;
    if let Some(bases)=bases {*field_mut(&mut out,"m_NodeBases")?=bases;}
    proof.surface_frames=frames;
    proof.original_records_preserved &= proof.surface_frames.is_empty();
    let (limited,attachments)=super::attachment_limits::apply(&out,&plan.attachments)?;
    out=limited;proof.original_records_preserved &= attachments.bones.is_empty();proof.attachments=attachments;
    Ok((out,proof))
}
