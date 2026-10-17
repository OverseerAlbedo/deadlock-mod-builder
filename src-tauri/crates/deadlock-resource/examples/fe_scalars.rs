//! Development aid: every top-level field of a compiled model's cloth solver
//! (scalars as values, arrays as lengths), for comparing models side by side.
//!
//!   fe_scalars MODEL.vmdl_c [MODEL.vmdl_c ...]
use deadlock_resource::{container, Kind, Value};
use std::collections::BTreeMap;
fn show(v: &Value) -> String {
    match &v.kind {
        Kind::Array(a) => format!("[{}]", a.len()),
        Kind::F32(n) => format!("{n}"),
        Kind::F64(n) => format!("{n}"),
        Kind::I64(n) => n.to_string(),
        Kind::U64(n) => n.to_string(),
        Kind::Bool(b) => b.to_string(),
        Kind::String(s) => format!("{s:?}"),
        Kind::Object(o) => format!("{{{} fields}}", o.len()),
        other => format!("{other:?}").chars().take(30).collect(),
    }
}
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut table: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (column, path) in args.iter().enumerate() {
        let raw = std::fs::read(path).unwrap();
        let fe = container::read_blocks(&raw, "PHYS").unwrap()[0].root.get("m_pFeModel").unwrap().clone();
        for (key, value) in fe.object().unwrap() {
            let row = table.entry(key.clone()).or_insert_with(|| vec!["-".into(); args.len()]);
            row[column] = show(value);
        }
    }
    for (key, row) in table {
        let differs = row.iter().any(|v| v != &row[0]);
        println!("{}{key:<40} {}", if differs { "* " } else { "  " }, row.iter().map(|v| format!("{v:<14}")).collect::<String>());
    }
}
