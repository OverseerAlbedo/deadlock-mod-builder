//! Development aid: for control names matching a filter, the proxy nodes that
//! drive them (fit matrices) and those nodes' simulation values.
//!
//!   fe_fit MODEL.vmdl_c NAME_SUBSTRING
use deadlock_resource::{container, Kind, Value};
fn num(v: &Value) -> f64 { v.number().unwrap_or(f64::NAN) }
fn int(v: &Value) -> i64 { v.integer().map(|n| n as i64).unwrap_or(-1) }
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let raw = std::fs::read(&args[0]).unwrap();
    let phys = container::read_blocks(&raw, "PHYS").unwrap();
    let fe = phys[0].root.get("m_pFeModel").unwrap();
    for (k, v) in fe.object().unwrap() {
        let shape = match &v.kind { Kind::Array(a) => format!("array[{}]", a.len()), Kind::Object(o) => format!("object{{{}}}", o.len()), other => format!("{:?}", other).chars().take(30).collect() };
        print!("{k}={shape}  ");
    }
    println!();
    let get = |k: &str| fe.get(k).and_then(|v| v.array().ok()).map(|a| a.to_vec()).unwrap_or_default();
    let names: Vec<String> = get("m_CtrlName").iter().map(|n| n.string().unwrap_or("?").to_string()).collect();
    let fits = get("m_FitMatrices");
    if let Some(f) = fits.first() { println!("fit matrix fields: {}", f.object().unwrap().iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>().join(" ")); }
    let fit_weights = get("m_FitWeights");
    let integ = get("m_NodeIntegrator");
    let inv = get("m_NodeInvMasses");
    let statics = fe.get("m_nStaticNodes").map(int).unwrap_or(0) as usize;
    let ctrl_offsets = get("m_CtrlOffsets");
    if let Some(c) = ctrl_offsets.first() { println!("ctrl offset fields: {}", c.object().unwrap().iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>().join(" ")); }
    let filter = &args[1];
    let mut begin = 0usize;
    for fit in &fits {
        let ctrl = fit.get("nCtrl").map(int).unwrap_or(-1) as usize;
        let end = fit.get("nEnd").map(int).unwrap_or(0) as usize;
        let name = names.get(ctrl).cloned().unwrap_or_default();
        if name.contains(filter.as_str()) {
            let nodes: Vec<usize> = fit_weights[begin..end.min(fit_weights.len())].iter().map(|w| w.get("nNode").map(int).unwrap_or(-1) as usize).collect();
            let desc: Vec<String> = nodes.iter().map(|&n| {
                let it = &integ[n];
                let g = |k: &str| it.get(k).map(num).unwrap_or(f64::NAN);
                format!("{n}{}(inv {:.2} att {:.3} vtx {:.3} damp {:.2} grav {:.2})", if n < statics { "S" } else { "" }, num(&inv[n]), g("flAnimationForceAttraction"), g("flAnimationVertexAttraction"), g("flPointDamping"), g("flGravity"))
            }).collect();
            println!("{name}: {}", desc.join(" "));
        }
        begin = end;
    }
}
