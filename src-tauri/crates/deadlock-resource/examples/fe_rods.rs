//! Development aid: the rods touching the nodes of control names that match a
//! filter, with their rest length (from the initial pose), min and max distance.
//!
//!   fe_rods MODEL.vmdl_c NAME_SUBSTRING [LIMIT]
use deadlock_resource::{container, Kind, Value};
use std::collections::BTreeSet;
fn num(v: &Value) -> f64 { v.number().unwrap_or(f64::NAN) }
fn int(v: &Value) -> i64 { v.integer().map(|n| n as i64).unwrap_or(-1) }
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let raw = std::fs::read(&args[0]).unwrap();
    let limit: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(12);
    let fe = container::read_blocks(&raw, "PHYS").unwrap()[0].root.get("m_pFeModel").unwrap().clone();
    let get = |k: &str| fe.get(k).and_then(|v| v.array().ok()).map(|a| a.to_vec()).unwrap_or_default();
    let names: Vec<String> = get("m_CtrlName").iter().map(|n| n.string().unwrap_or("?").to_string()).collect();
    let pose: Vec<[f64; 3]> = get("m_InitPose").iter().map(|t| {
        let p = t.get("m_vPosition").or_else(|| t.get("position")).cloned().unwrap_or_else(|| t.clone());
        match &p.kind { Kind::Array(a) if a.len() >= 3 => [num(&a[0]), num(&a[1]), num(&a[2])], _ => [f64::NAN; 3] }
    }).collect();
    // Target nodes of the matching bones (reverse offsets), and their tree neighbors.
    let mut targets = BTreeSet::new();
    for o in get("m_ReverseOffsets") {
        let ctrl = o.get("nBoneCtrl").map(int).unwrap_or(-1) as usize;
        if names.get(ctrl).is_some_and(|n| n.contains(args[1].as_str())) { targets.insert(o.get("nTargetNode").map(int).unwrap_or(-1) as usize); }
    }
    // Valve's own chains: match the node names directly.
    for (i, n) in names.iter().enumerate() { if n.contains(args[1].as_str()) && targets.len() < 64 { targets.insert(i); } }
    let mut shown = 0;
    let (mut ratios_min, mut ratios_max) = (vec![], vec![]);
    for key in ["m_Rods"] {
        for r in get(key) {
            let nodes: Vec<usize> = match r.get("nNode").map(|v| &v.kind) { Some(Kind::Array(a)) => a.iter().map(|x| int(x) as usize).collect(), _ => vec![] };
            if nodes.len() != 2 || !(targets.contains(&nodes[0]) || targets.contains(&nodes[1])) { continue; }
            let (a, b) = (pose.get(nodes[0]).copied().unwrap_or([f64::NAN; 3]), pose.get(nodes[1]).copied().unwrap_or([f64::NAN; 3]));
            let rest = ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
            let (mn, mx) = (r.get("flMinDist").map(num).unwrap_or(f64::NAN), r.get("flMaxDist").map(num).unwrap_or(f64::NAN));
            if rest > 1e-6 { ratios_min.push(mn / rest); ratios_max.push(mx / rest); }
            if std::env::var("LONG").is_ok() && rest < 6. { continue; }
            if shown < limit {
                println!("{:<34} - {:<34} rest {rest:6.2} min {mn:6.2} max {mx:6.2} w0 {:.2} relax {:.2}", names.get(nodes[0]).cloned().unwrap_or_default(), names.get(nodes[1]).cloned().unwrap_or_default(), r.get("flWeight0").map(num).unwrap_or(f64::NAN), r.get("flRelaxationFactor").map(num).unwrap_or(f64::NAN));
                shown += 1;
            }
        }
    }
    if std::env::var("SIMD").is_ok() { simd(&fe, &names, &pose, &targets, limit); }
    let avg = |v: &Vec<f64>| v.iter().filter(|x| x.is_finite()).sum::<f64>() / v.len().max(1) as f64;
    println!("rods {} | min/rest avg {:.3} | max/rest avg {:.3}", ratios_min.len(), avg(&ratios_min), avg(&ratios_max));
}
/// SIMD rods: four rods per entry (nNode holds eight node indices: pairs).
#[allow(dead_code)]
pub fn simd(fe: &Value, names: &[String], pose: &[[f64; 3]], targets: &BTreeSet<usize>, limit: usize) {
    let rows = fe.get("m_SimdRods").and_then(|v| v.array().ok()).map(|a| a.to_vec()).unwrap_or_default();
    if let Some(first) = rows.first() { println!("simd rod fields: {}", first.object().unwrap().iter().map(|(k, v)| format!("{k}:{}", match &v.kind { Kind::Array(a) => format!("[{}]", a.len()), _ => "scalar".into() })).collect::<Vec<_>>().join(" ")); }
    let flat = |v: &Value| -> Vec<f64> { match &v.kind { Kind::Array(a) => a.iter().flat_map(|x| match &x.kind { Kind::Array(b) => b.iter().map(num).collect::<Vec<_>>(), _ => vec![num(x)] }).collect(), _ => vec![num(v)] } };
    let (mut shown, mut long, mut total) = (0, 0, 0);
    for r in rows {
        let nodes: Vec<usize> = r.get("nNode").map(|v| flat(v)).unwrap_or_default().iter().map(|x| *x as usize).collect();
        let (mins, maxs) = (r.get("f4MinDist").map(|v| flat(v)).unwrap_or_default(), r.get("f4MaxDist").map(|v| flat(v)).unwrap_or_default());
        for k in 0..mins.len().min(maxs.len()) {
            // Layout: nNode = [a0..a3, b0..b3] or interleaved pairs; try both.
            let (a, b) = if nodes.len() == 8 { (nodes[k], nodes[4 + k]) } else { continue };
            if !(targets.contains(&a) || targets.contains(&b)) { continue; }
            total += 1;
            let (pa, pb) = (pose.get(a).copied().unwrap_or([f64::NAN; 3]), pose.get(b).copied().unwrap_or([f64::NAN; 3]));
            let rest = ((pa[0] - pb[0]).powi(2) + (pa[1] - pb[1]).powi(2) + (pa[2] - pb[2]).powi(2)).sqrt();
            if rest >= 6. { long += 1; }
            if rest >= 6. && shown < limit {
                println!("SIMD {:<32} - {:<32} rest {rest:6.2} min {:6.2} max {:6.2}", names.get(a).cloned().unwrap_or_default(), names.get(b).cloned().unwrap_or_default(), mins[k], maxs[k]);
                shown += 1;
            }
        }
    }
    println!("simd rods touching: {total}, longer than 6 in: {long}");
}
