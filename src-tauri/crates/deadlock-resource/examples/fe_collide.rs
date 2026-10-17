//! Development aid: per control name, the collision radius and tree collision
//! mask of the compiled cloth's nodes (first N of each kind).
use deadlock_resource::{container, Value};
fn num(v: &Value) -> f64 { v.number().unwrap_or(f64::NAN) }
fn main() {
    let path = std::env::args().nth(1).unwrap();
    let raw = std::fs::read(&path).unwrap();
    let fe = container::read_blocks(&raw, "PHYS").unwrap()[0].root.get("m_pFeModel").unwrap().clone();
    let get = |k: &str| fe.get(k).and_then(|v| v.array().ok()).map(|a| a.to_vec()).unwrap_or_default();
    let names: Vec<String> = get("m_CtrlName").iter().map(|n| n.string().unwrap_or("?").to_string()).collect();
    let radii = get("m_NodeCollisionRadii");
    let masks = get("m_TreeCollisionMasks");
    let statics = fe.get("m_nStaticNodes").and_then(|v| v.integer().ok()).unwrap_or(0) as usize;
    println!("nodes {} statics {statics} radii {} masks {}", names.len(), radii.len(), masks.len());
    let mut shown = std::collections::BTreeMap::<String, usize>::new();
    for (i, n) in names.iter().enumerate() {
        let r = radii.get(i).map(num); let m = masks.get(i).map(num);
        let key = format!("{:?}/{:?}", r, m);
        let c = shown.entry(key.clone()).or_default(); *c += 1;
        if *c <= 3 { println!("{i:>5} {n:<40} radius {r:?} mask {m:?}"); }
    }
    println!("{:?}", shown);
}
