//! Development aid: how often one SIMD rod pack uses the same node in more
//! than one lane, split into static (never written) and dynamic nodes, and
//! whether a padded lane repeats a whole rod.
//!
//!   fe_simd_share MODEL.vmdl_c
use deadlock_resource::{container, Kind, Value};
use std::collections::BTreeMap;
fn int(v: &Value) -> i64 { v.integer().map(|n| n as i64).unwrap_or(-1) }
fn main() {
    let raw = std::fs::read(std::env::args().nth(1).unwrap()).unwrap();
    let fe = container::read_blocks(&raw, "PHYS").unwrap()[0].root.get("m_pFeModel").unwrap().clone();
    let statics = fe.get("m_nStaticNodes").map(int).unwrap_or(0);
    let packs = fe.get("m_SimdRods").and_then(|v| v.array().ok()).map(|a| a.to_vec()).unwrap_or_default();
    let (mut static_shared, mut dynamic_shared, mut repeated) = (0, 0, 0);
    for pack in &packs {
        let nodes = pack.get("nNode").unwrap();
        let Kind::Array(axes) = &nodes.kind else { continue };
        let lane = |axis: usize, i: usize| int(&axes[axis].array().unwrap()[i]);
        let pairs: Vec<(i64, i64)> = (0..4).map(|i| (lane(0, i), lane(1, i))).collect();
        let mut distinct = pairs.clone(); distinct.sort(); distinct.dedup();
        if distinct.len() < 4 { repeated += 1; }
        let mut count: BTreeMap<i64, usize> = BTreeMap::new();
        for (a, b) in &distinct { *count.entry(*a).or_default() += 1; *count.entry(*b).or_default() += 1; }
        if count.iter().any(|(n, c)| *c > 1 && *n < statics) { static_shared += 1; }
        if count.iter().any(|(n, c)| *c > 1 && *n >= statics) { dynamic_shared += 1; }
    }
    println!("packs {} | packs with a static node in 2+ lanes {static_shared} | with a dynamic node in 2+ lanes {dynamic_shared} | with a repeated lane {repeated}", packs.len());
}
