//! Development aid: per chain (control-name stem) of a compiled cloth, the
//! dynamic nodes' damping, attraction, gravity, mass and travel, root to tip.
//!
//!   fe_stats MODEL.vmdl_c [STEM_SUBSTRING ...]
use deadlock_resource::{container, Kind, Value};
use std::collections::BTreeMap;
fn num(v: &Value) -> f64 { v.number().unwrap_or(f64::NAN) }
fn int(v: &Value) -> i64 { v.integer().map(|n| n as i64).unwrap_or(-1) }
fn stem(name: &str) -> String {
    let trimmed = name.trim_end_matches(|c: char| c.is_ascii_digit() || c == '_');
    trimmed.to_string()
}
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let raw = std::fs::read(&args[0]).unwrap();
    let fe = container::read_blocks(&raw, "PHYS").unwrap()[0].root.get("m_pFeModel").unwrap().clone();
    let get = |k: &str| fe.get(k).and_then(|v| v.array().ok()).map(|a| a.to_vec()).unwrap_or_default();
    let names: Vec<String> = get("m_CtrlName").iter().map(|n| n.string().unwrap_or("?").to_string()).collect();
    let integ = get("m_NodeIntegrator");
    let inv = get("m_NodeInvMasses");
    let statics = fe.get("m_nStaticNodes").map(int).unwrap_or(0) as usize;
    let driven = fe.get("m_nFirstPositionDrivenNode").map(int).unwrap_or(i64::MAX) as usize;
    let mut stray = BTreeMap::new();
    for s in get("m_AnimStrayRadii") {
        let ns: Vec<usize> = match s.get("nNode").map(|v| &v.kind) { Some(Kind::Array(a)) => a.iter().map(|x| int(x) as usize).collect(), _ => vec![s.get("nNode").map(int).unwrap_or(-1) as usize] };
        for n in ns { stray.insert(n, s.get("flMaxDist").map(num).unwrap_or(f64::NAN)); }
    }
    let mut chains: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for n in statics..driven.min(names.len()) { chains.entry(stem(&names[n])).or_default().push(n); }
    for (stem, nodes) in &chains {
        if args.len() > 1 && !args[1..].iter().any(|f| stem.contains(f.as_str())) { continue; }
        let row = |n: usize| { let g = |k: &str| integ[n].get(k).map(num).unwrap_or(f64::NAN); format!("d{:.2} a{:.3} g{:.2} m{:.0}{}", g("flPointDamping"), g("flAnimationForceAttraction"), g("flGravity"), 1.0 / num(&inv[n]).max(1e-6), stray.get(&n).map_or(String::new(), |s| format!(" s{s:.1}"))) };
        let first = nodes[0]; let mid = nodes[nodes.len() / 2]; let last = *nodes.last().unwrap();
        println!("{stem:<36} {:>3} nodes | root {} | mid {} | tip {}", nodes.len(), row(first), row(mid), row(last));
    }
}
