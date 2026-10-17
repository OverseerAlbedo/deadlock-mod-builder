//! Development aid: the travel limits (m_AnimStrayRadii) of the nodes owned by
//! the bone controls that match a filter.
//!
//!   fe_stray MODEL.vmdl_c NAME_SUBSTRING
use deadlock_resource::{container, Kind, Value};
use std::collections::BTreeMap;
fn num(v: &Value) -> f64 { v.number().unwrap_or(f64::NAN) }
fn int(v: &Value) -> i64 { v.integer().map(|n| n as i64).unwrap_or(-1) }
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let raw = std::fs::read(&args[0]).unwrap();
    let fe = container::read_blocks(&raw, "PHYS").unwrap()[0].root.get("m_pFeModel").unwrap().clone();
    let get = |k: &str| fe.get(k).and_then(|v| v.array().ok()).map(|a| a.to_vec()).unwrap_or_default();
    let names: Vec<String> = get("m_CtrlName").iter().map(|n| n.string().unwrap_or("?").to_string()).collect();
    let parents: Vec<i64> = get("m_SkelParents").iter().map(int).collect();
    let driven = fe.get("m_nFirstPositionDrivenNode").map(int).unwrap_or(0) as usize;
    let integ = get("m_NodeIntegrator");
    let mut stray: BTreeMap<usize, Vec<String>> = BTreeMap::new();
    for s in get("m_AnimStrayRadii") {
        let ns: Vec<i64> = match s.get("nNode").map(|v| &v.kind) { Some(Kind::Array(a)) => a.iter().map(int).collect(), _ => vec![] };
        let text = format!("{:?} max {:.2} relax {:.2}", ns, s.get("flMaxDist").map(num).unwrap_or(f64::NAN), s.get("flRelaxationFactor").map(num).unwrap_or(f64::NAN));
        for n in &ns { stray.entry(*n as usize).or_default().push(text.clone()); }
    }
    if let Some(first) = get("m_AnimStrayRadii").first() { println!("stray fields: {:?}", first.object().map(|o| o.iter().map(|(k, _)| k.clone()).collect::<Vec<_>>())); }
    for (ctrl, name) in names.iter().enumerate() {
        if !name.contains(args[1].as_str()) || ctrl < driven { continue; }
        for p in (0..driven).filter(|&p| parents.get(p) == Some(&(ctrl as i64))) {
            let g = |k: &str| integ.get(p).and_then(|v| v.get(k)).map(num).unwrap_or(f64::NAN);
            println!("{name:<32} node {p:>4} d{:.2} a{:.4} g{:.0} stray {:?}", g("flPointDamping"), g("flAnimationForceAttraction"), g("flGravity"), stray.get(&p));
        }
    }
}
