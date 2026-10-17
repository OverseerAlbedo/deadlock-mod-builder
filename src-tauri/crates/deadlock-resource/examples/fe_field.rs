//! Development aid: print one top-level field of a compiled model's cloth
//! solver (first LIMIT entries of an array).
//!
//!   fe_field MODEL.vmdl_c FIELD [LIMIT]
use deadlock_resource::{container, Value};
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let raw = std::fs::read(&args[0]).unwrap();
    let fe = container::read_blocks(&raw, "PHYS").unwrap()[0].root.get("m_pFeModel").unwrap().clone();
    let limit: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(10);
    let v: &Value = fe.get(&args[1]).expect("no such field");
    match v.array() {
        Ok(a) => { println!("{} entries", a.len()); for x in a.iter().take(limit) { println!("{x:?}"); } }
        Err(_) => println!("{v:?}"),
    }
}
