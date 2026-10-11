//! Match existing shoulder/twist deformation joints without changing hero binds.
//! A helper name alone is insufficient: limb ancestry and normalized position
//! must agree. Explicit direct mappings remain authoritative.
use crate::{algorithms::Bone, following::ancestor};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const POLICY: &str = "anatomical-arm-deformation-v1";
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Match {
    pub target: String,
    pub role: String,
    pub kind: String,
}
fn usable(b: &Bone) -> bool {
    b.details.get("deform").and_then(|v| v.as_bool()) != Some(false)
}
fn compact(n: &str) -> String {
    n.rsplit(':').next().unwrap_or(n).chars().filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase).collect()
}
fn shoulder(n: &str, side: &str) -> bool {
    let n = compact(n);
    ["shoulder", "clavicle", "clav", "肩"].iter().any(|base| {
        [format!("{base}{}", side.to_lowercase()), format!("{}{base}", side.to_lowercase()),
         format!("{}{base}", if side == "L" {"left"} else {"right"}),
         format!("{}{base}", if side == "L" {"左"} else {"右"})].contains(&n)
    })
}
fn twist(n: &str) -> bool {
    let n = compact(n);
    let roll = n.starts_with("roll") || n.ends_with("roll")
        || ["armroll", "armupperroll", "armlowerroll"].iter().any(|s|n.contains(s));
    !["notwist", "weapon", "prop", "hair", "cloth", "skirt", "tail", "streamer", "controller"].iter().any(|s| n.contains(s))
        && !n.starts_with("ear") && !n.starts_with("leftear") && !n.starts_with("rightear")
        && (n.contains("twist") || n.contains('捩') || roll)
}
fn helper(n: &str) -> bool {
    let n = compact(n);
    twist(&n) || ["shoulderpart", "elbowpart", "upperarmhelper", "forearmhelper"]
        .iter().any(|s| n.contains(s))
}
fn side_agrees(n: &str, side: &str) -> bool {
    let n=n.rsplit(':').next().unwrap_or(n).to_lowercase();
    let wrong=if side=="L" {"r"} else {"l"};
    !n.ends_with(&format!(".{wrong}")) && !n.ends_with(&format!("_{wrong}"))
        && !n.starts_with(if side=="L" {"right"} else {"left"})
        && !n.starts_with(if side=="L" {"右"} else {"左"})
}
fn coordinate(p: [f64;3], a: [f64;3], b: [f64;3]) -> Option<f64> {
    let axis: [f64;3] = std::array::from_fn(|i| b[i]-a[i]);
    let length2: f64 = axis.iter().map(|v| v*v).sum();
    if !length2.is_finite() || length2 < 1e-12 { return None; }
    let t = (0..3).map(|i| (p[i]-a[i])*axis[i]).sum::<f64>() / length2;
    let off = (0..3).map(|i| (p[i]-a[i]-axis[i]*t).powi(2)).sum::<f64>() / length2;
    (t.is_finite() && (0.1..=0.9).contains(&t) && off <= 0.08_f64.powi(2)).then_some(t)
}
pub fn detect(source: &[Bone], target: &[Bone], sr: &BTreeMap<String,String>, tr: &BTreeMap<String,String>) -> BTreeMap<String,Match> {
    let by = |bones: &[Bone], n: &str| bones.iter().find(|b| b.name == n).cloned();
    let mut out = BTreeMap::new();
    for side in ["L","R"] {
        let upper = format!("upperarm_{side}");
        if let (Some(sa),Some(ta),Some(sc),Some(tc)) = (sr.get(&upper),tr.get(&upper),sr.get("chest"),tr.get("chest")) {
            let collect = |bones: &[Bone], chest: &str, arm: &str| bones.iter().filter(|b| usable(b)
                && shoulder(&b.name,side) && ancestor(bones,chest,&b.name) && ancestor(bones,&b.name,arm))
                .map(|b|b.name.clone()).collect::<Vec<_>>();
            let (a,b) = (collect(source,sc,sa),collect(target,tc,ta));
            if a.len()==1 && b.len()==1 { out.insert(a[0].clone(),Match {target:b[0].clone(),role:"chest".into(),kind:"shoulder".into()}); }
        }
        for (start,end) in [("upperarm","forearm"),("forearm","hand")] {
            let role = format!("{start}_{side}");let next = format!("{end}_{side}");
            let (Some(sa),Some(sb),Some(ta),Some(tb)) = (sr.get(&role),sr.get(&next),tr.get(&role),tr.get(&next)) else {continue};
            let (Some(a),Some(b),Some(c),Some(d)) = (by(source,sa),by(source,sb),by(target,ta),by(target,tb)) else {continue};
            let eligible = |bones: &[Bone], name: &str, first: &str, last: &str| name != first && name != last
                && ancestor(bones,first,name) && !ancestor(bones,last,name);
            let candidates:Vec<_> = target.iter().filter(|v| usable(v) && twist(&v.name) && eligible(target,&v.name,ta,tb))
                .filter_map(|v|coordinate(v.position,c.position,d.position).map(|t|(v,t))).collect();
            for v in source.iter().filter(|v| usable(v) && helper(&v.name) && side_agrees(&v.name,side) && eligible(source,&v.name,sa,sb)) {
                let Some(t) = coordinate(v.position,a.position,b.position) else {continue};
                let mut distances:Vec<_> = candidates.iter().map(|(b,u)|((t-u).abs(),*b)).filter(|(delta,_)|*delta<=0.20).collect();
                distances.sort_by(|a,b|a.0.total_cmp(&b.0).then(a.1.name.cmp(&b.1.name)));
                if distances.is_empty() || distances.get(1).is_some_and(|v|(v.0-distances[0].0).abs()<1e-5) {continue;}
                out.insert(v.name.clone(),Match {target:distances[0].1.name.clone(),role:role.clone(),kind:"twist".into()});
            }
        }
    }
    out
}
