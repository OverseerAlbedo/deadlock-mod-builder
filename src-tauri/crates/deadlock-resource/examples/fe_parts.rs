//! Development aid: per moving part (dmb_secondary_<root>…), how its bones are
//! attached in the compiled cloth (ctrl offsets and node bases) and the
//! simulation values of the proxy nodes they hang from.
//!
//!   fe_parts MODEL.vmdl_c [NAME_SUBSTRING ...]
use deadlock_resource::{container, Kind, Value};
use std::collections::{BTreeMap, BTreeSet};
fn num(v: &Value) -> f64 { v.number().unwrap_or(f64::NAN) }
fn int(v: &Value) -> i64 { v.integer().map(|n| n as i64).unwrap_or(-1) }
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let raw = std::fs::read(&args[0]).unwrap();
    let phys = container::read_blocks(&raw, "PHYS").unwrap();
    let fe = phys[0].root.get("m_pFeModel").unwrap();
    let get = |k: &str| fe.get(k).and_then(|v| v.array().ok()).map(|a| a.to_vec()).unwrap_or_default();
    if args.len() == 1 {
        for (k, v) in fe.object().unwrap() {
            let shape = match &v.kind { Kind::Array(a) => format!("[{}]", a.len()), Kind::Object(o) => format!("{{{}}}", o.len()), other => format!("{:?}", other).chars().take(24).collect() };
            println!("{k} {shape}{}", v.array().ok().and_then(|a| a.first()).and_then(|f| f.object().ok()).map(|o| format!("  fields: {}", o.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>().join(" "))).unwrap_or_default());
        }
        return;
    }
    let names: Vec<String> = get("m_CtrlName").iter().map(|n| n.string().unwrap_or("?").to_string()).collect();
    let integ = get("m_NodeIntegrator");
    let inv = get("m_NodeInvMasses");
    let statics = fe.get("m_nStaticNodes").map(int).unwrap_or(0) as usize;
    let first_driven = fe.get("m_nFirstPositionDrivenNode").map(int).unwrap_or(i64::MAX) as usize;
    let mut parent_of = BTreeMap::new();
    for o in get("m_ReverseOffsets") {
        parent_of.insert(o.get("nBoneCtrl").map(int).unwrap_or(-1) as usize, o.get("nTargetNode").map(int).unwrap_or(-1) as usize);
    }
    let mut stray = BTreeMap::new();
    for s in get("m_AnimStrayRadii") {
        let ns: Vec<usize> = match s.get("nNode").map(|v| &v.kind) { Some(Kind::Array(a)) => a.iter().map(|x| int(x) as usize).collect(), Some(_) => vec![s.get("nNode").map(int).unwrap_or(-1) as usize], None => vec![] };
        for n in ns { stray.insert(n, s.get("flMaxDist").map(num).unwrap_or(f64::NAN)); }
    }
    let free: BTreeSet<usize> = get("m_FreeNodes").iter().map(|v| int(v) as usize).collect();
    let bases = get("m_NodeBases");
    let mut base_nodes: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for b in &bases {
        let node = b.get("nNode").map(int).unwrap_or(-1) as usize;
        let refs: Vec<usize> = ["nDummy", "nNodeX0", "nNodeX1", "nNodeY0", "nNodeY1"].iter().filter_map(|k| b.get(k)).flat_map(|v| match &v.kind { Kind::Array(a) => a.iter().map(|x| int(x) as usize).collect::<Vec<_>>(), _ => vec![int(v) as usize] }).collect();
        base_nodes.insert(node, refs);
    }
    let describe = |n: usize| -> String {
        if n >= integ.len() { return format!("{n}?"); }
        let it = &integ[n];
        let g = |k: &str| it.get(k).map(num).unwrap_or(f64::NAN);
        format!("{n}{}{}{} inv {:.2} stray {} att {:.3} vtx {:.3} damp {:.2} grav {:.2}", if n < statics { " STATIC" } else { "" }, if n >= first_driven { " DRIVEN" } else { "" }, if free.contains(&n) { " free" } else { "" }, num(&inv[n]), stray.get(&n).map_or("-".to_string(), |d| format!("{d:.2}")), g("flAnimationForceAttraction"), g("flAnimationVertexAttraction"), g("flPointDamping"), g("flGravity"))
    };
    for filter in &args[1..] {
        let mut seen = BTreeSet::new();
        for (i, name) in names.iter().enumerate() {
            if !name.contains(filter.as_str()) || !seen.insert(i) { continue; }
            let parent = parent_of.get(&i).copied();
            println!("{name:<32} -> {}", parent.map(describe).unwrap_or_else(|| "no target node".into()));
        }
    }
}
