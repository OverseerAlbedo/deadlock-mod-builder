//! Development aid: the simulated nodes behind the bone controls that match a
//! filter (nodes whose skeleton parent is the control), with static/dynamic,
//! integrator values, rest position, and the rods touching them.
//!
//!   fe_chain MODEL.vmdl_c NAME_SUBSTRING [MAX_CONTROLS]
use deadlock_resource::{container, Kind, Value};
use std::collections::{BTreeMap, BTreeSet};
fn num(v: &Value) -> f64 { v.number().unwrap_or(f64::NAN) }
fn int(v: &Value) -> i64 { v.integer().map(|n| n as i64).unwrap_or(-1) }
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let raw = std::fs::read(&args[0]).unwrap();
    let max: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(6);
    let fe = container::read_blocks(&raw, "PHYS").unwrap()[0].root.get("m_pFeModel").unwrap().clone();
    let get = |k: &str| fe.get(k).and_then(|v| v.array().ok()).map(|a| a.to_vec()).unwrap_or_default();
    let names: Vec<String> = get("m_CtrlName").iter().map(|n| n.string().unwrap_or("?").to_string()).collect();
    let parents: Vec<i64> = get("m_SkelParents").iter().map(int).collect();
    let statics = fe.get("m_nStaticNodes").map(int).unwrap_or(0) as usize;
    let driven = fe.get("m_nFirstPositionDrivenNode").map(int).unwrap_or(0) as usize;
    let integ = get("m_NodeIntegrator");
    let pose: Vec<[f64; 3]> = get("m_InitPose").iter().map(|t| match &t.kind { Kind::Array(a) if a.len() >= 3 => [num(&a[0]), num(&a[1]), num(&a[2])], _ => [f64::NAN; 3] }).collect();
    println!("nodes {} static {statics} driven {driven}", names.len());
    let reverse: BTreeMap<usize, usize> = get("m_ReverseOffsets").iter().map(|r| (r.get("nBoneCtrl").map(int).unwrap_or(-1) as usize, r.get("nTargetNode").map(int).unwrap_or(-1) as usize)).collect();
    let mut rods: BTreeMap<usize, Vec<(usize, f64, f64, f64)>> = BTreeMap::new();
    for r in get("m_Rods") {
        let v: Vec<usize> = match r.get("nNode").map(|v| &v.kind) { Some(Kind::Array(a)) => a.iter().map(|x| int(x) as usize).collect(), _ => vec![] };
        if v.len() != 2 { continue; }
        let (mn, mx, rl) = (r.get("flMinDist").map(num).unwrap_or(f64::NAN), r.get("flMaxDist").map(num).unwrap_or(f64::NAN), r.get("flRelaxationFactor").map(num).unwrap_or(f64::NAN));
        rods.entry(v[0]).or_default().push((v[1], mn, mx, rl));
        rods.entry(v[1]).or_default().push((v[0], mn, mx, rl));
    }
    let mut shown = 0;
    let mut seen = BTreeSet::new();
    for (ctrl, name) in names.iter().enumerate() {
        if !name.contains(args[1].as_str()) || shown >= max { continue; }
        shown += 1;
        let owned: Vec<usize> = (0..driven).filter(|&p| parents.get(p) == Some(&(ctrl as i64))).collect();
        println!("CTRL {ctrl} {name} parent {} target {:?} owned {owned:?}", parents.get(ctrl).map(|p| names.get(*p as usize).cloned().unwrap_or_default()).unwrap_or_default(), reverse.get(&ctrl));
        for &n in &owned {
            let g = |k: &str| integ.get(n).and_then(|v| v.get(k)).map(num).unwrap_or(f64::NAN);
            println!("  node {n} {} '{}' pos [{:.2},{:.2},{:.2}] d{:.2} a{:.4} g{:.1}", if n < statics { "STATIC" } else { "dyn" }, names[n], pose[n][0], pose[n][1], pose[n][2], g("flPointDamping"), g("flAnimationForceAttraction"), g("flGravity"));
            if seen.insert(n) {
                for (o, mn, mx, rl) in rods.get(&n).cloned().unwrap_or_default() {
                    let d = (0..3).map(|k| (pose[n][k] - pose[o][k]).powi(2)).sum::<f64>().sqrt();
                    println!("    rod -> {o} (parent ctrl {}) rest {d:.2} min {mn:.2} max {mx:.2} relax {rl:.2}", parents.get(o).and_then(|p| names.get(*p as usize)).cloned().unwrap_or_default());
                }
            }
        }
    }
}
