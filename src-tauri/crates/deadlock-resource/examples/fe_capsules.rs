//! Development aid: the tapered capsule colliders of a compiled model's cloth,
//! with the control name of the node each one follows.
//!
//!   fe_capsules MODEL.vmdl_c
use deadlock_resource::{container, Kind, Value};
fn int(v: &Value) -> i64 { v.integer().map(|n| n as i64).unwrap_or(-1) }
fn show(v: &Value) -> String {
    match &v.kind {
        Kind::Array(a) => format!("[{}]", a.iter().map(show).collect::<Vec<_>>().join(", ")),
        Kind::F32(n) => format!("{n:.2}"), Kind::F64(n) => format!("{n:.2}"),
        Kind::I64(n) => n.to_string(), Kind::U64(n) => n.to_string(),
        other => format!("{other:?}").chars().take(20).collect(),
    }
}
fn main() {
    let path = std::env::args().nth(1).unwrap();
    let raw = std::fs::read(&path).unwrap();
    let fe = container::read_blocks(&raw, "PHYS").unwrap()[0].root.get("m_pFeModel").unwrap().clone();
    let names: Vec<String> = fe.get("m_CtrlName").unwrap().array().unwrap().iter().map(|n| n.string().unwrap_or("?").to_string()).collect();
    let statics = fe.get("m_nStaticNodes").map(int).unwrap_or(0);
    println!("static nodes {statics}: {}", names.iter().take(statics as usize).cloned().collect::<Vec<_>>().join(" "));
    for key in ["m_TaperedCapsuleRigids", "m_SphereRigids", "m_TaperedCapsuleStretches"] {
        for c in fe.get(key).and_then(|v| v.array().ok()).unwrap_or(&[]) {
            let node = c.get("nNode").map(int).unwrap_or(-1);
            let fields: Vec<String> = c.object().unwrap().iter().filter(|(k, _)| k != "nNode").map(|(k, v)| format!("{k}={}", show(v))).collect();
            println!("{key} node {node} ({}) {}", names.get(node as usize).cloned().unwrap_or_default(), fields.join(" "));
        }
    }
    for key in ["m_NodeCollisionRadii", "m_DynNodeFriction", "m_TreeCollisionMasks"] {
        let a = fe.get(key).and_then(|v| v.array().ok()).unwrap_or(&[]);
        let mut uniq: Vec<String> = a.iter().map(show).collect(); uniq.sort(); uniq.dedup();
        println!("{key}: {} entries, values {}", a.len(), uniq.iter().take(8).cloned().collect::<Vec<_>>().join(" "));
    }
}
