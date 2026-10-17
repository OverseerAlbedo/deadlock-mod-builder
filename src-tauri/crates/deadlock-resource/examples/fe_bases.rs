//! Development aid: the node bases (orientation frames) of the nodes owned by
//! bone controls whose name contains FILTER, with the controls the frame's
//! nodes belong to.
//!
//!   fe_bases MODEL.vmdl_c FILTER
use deadlock_resource::{container, Kind, Value};
fn int(v: &Value) -> i64 { v.integer().map(|n| n as i64).unwrap_or(-1) }
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let raw = std::fs::read(&args[0]).unwrap();
    let fe = container::read_blocks(&raw, "PHYS").unwrap()[0].root.get("m_pFeModel").unwrap().clone();
    let get = |k: &str| fe.get(k).and_then(|v| v.array().ok()).map(|a| a.to_vec()).unwrap_or_default();
    let names: Vec<String> = get("m_CtrlName").iter().map(|n| n.string().unwrap_or("?").to_string()).collect();
    let parents: Vec<i64> = get("m_SkelParents").iter().map(int).collect();
    let driven = fe.get("m_nFirstPositionDrivenNode").map(int).unwrap_or(0) as usize;
    let owner = |n: i64| -> String { parents.get(n as usize).and_then(|p| names.get(*p as usize)).cloned().unwrap_or_default().replace("dmb_secondary_", "") };
    let bases = get("m_NodeBases");
    if let Some(first) = bases.first() { println!("fields: {:?}", first.object().map(|o| o.iter().map(|(k, _)| k.clone()).collect::<Vec<_>>())); }
    for (ctrl, name) in names.iter().enumerate() {
        if !name.contains(args[1].as_str()) || ctrl < driven { continue; }
        let owned: Vec<i64> = (0..driven as i64).filter(|&p| parents.get(p as usize) == Some(&(ctrl as i64))).collect();
        println!("CTRL {name} owns {owned:?}");
        for b in &bases {
            let n = b.get("nNode").map(int).unwrap_or(-1);
            if !owned.contains(&n) && n != ctrl as i64 { continue; }
            let f = |k: &str| b.get(k).map(int).unwrap_or(-1);
            let q = match b.get("qAdjust").map(|v| &v.kind) { Some(Kind::Array(a)) => a.iter().map(|x| format!("{:.3}", x.number().unwrap_or(f64::NAN))).collect::<Vec<_>>().join(","), _ => String::new() };
            println!("  node {n}: X {}({}) -> {}({})  Y {}({}) -> {}({})  qAdjust [{q}]", f("nNodeX0"), owner(f("nNodeX0")), f("nNodeX1"), owner(f("nNodeX1")), f("nNodeY0"), owner(f("nNodeY0")), f("nNodeY1"), owner(f("nNodeY1")));
        }
    }
}
