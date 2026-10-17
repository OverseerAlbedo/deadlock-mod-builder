//! Development aid: the cloth nodes a compiled model runs in game, per control
//! name: static or dynamic, the integrator values and which rods, springs and
//! ropes touch it.
//!
//!   fe_nodes MODEL.vmdl_c [NAME_SUBSTRING ...]
use deadlock_resource::{container, Kind, Value};

fn num(v: &Value) -> f64 { v.number().unwrap_or(f64::NAN) }
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let raw = std::fs::read(&args[0]).unwrap();
    let phys = container::read_blocks(&raw, "PHYS").unwrap();
    let fe = phys[0].root.get("m_pFeModel").expect("no m_pFeModel");
    let keys: Vec<&str> = fe.object().unwrap().iter().map(|(k, v)| { let _ = v; k.as_str() }).collect();
    println!("FeModel keys: {}", keys.join(" "));
    let names: Vec<String> = fe.get("m_CtrlName").and_then(|v| v.array().ok()).map(|a| a.iter().map(|n| n.string().unwrap_or("?").to_string()).collect()).unwrap_or_default();
    let count = |k: &str| fe.get(k).and_then(|v| v.array().ok()).map_or(0, |a| a.len());
    let scalar = |k: &str| fe.get(k).map(|v| match &v.kind { Kind::I64(n) => n.to_string(), Kind::U64(n) => n.to_string(), Kind::F32(n) => n.to_string(), Kind::F64(n) => n.to_string(), _ => "-".into() }).unwrap_or_default();
    println!("ctrls {} | static nodes {} | rot-lock static {} | node bases {} | integrator {} | inv masses {} | rods {} | simd rods {} | ropes {} | springs {} | free nodes {} | capsules {} | spheres {}",
        names.len(), scalar("m_nStaticNodes"), scalar("m_nRotLockStaticNodes"), count("m_NodeBases"), count("m_NodeIntegrator"), count("m_NodeInvMasses"), count("m_Rods"), count("m_SimdRods"), count("m_Ropes"), count("m_Springs"), count("m_FreeNodes"), count("m_TaperedCapsuleRigids"), count("m_SphereRigids"));
    let inv: Vec<f64> = fe.get("m_NodeInvMasses").and_then(|v| v.array().ok()).map(|a| a.iter().map(num).collect()).unwrap_or_default();
    let integ = fe.get("m_NodeIntegrator").and_then(|v| v.array().ok()).map(|a| a.to_vec()).unwrap_or_default();
    let statics: usize = scalar("m_nStaticNodes").parse().unwrap_or(0);
    let filters: Vec<&String> = args[1..].iter().collect();
    for (i, name) in names.iter().enumerate() {
        if !filters.is_empty() && !filters.iter().any(|f| name.contains(f.as_str())) { continue; }
        let it = integ.get(i).map(|v| {
            let g = |k: &str| v.get(k).map(num).unwrap_or(f64::NAN);
            format!("damp {:.2} animForce {:.3} animVertex {:.3} grav {:.2}", g("flPointDamping"), g("flAnimationForceAttraction"), g("flAnimationVertexAttraction"), g("flGravity"))
        }).unwrap_or_default();
        println!("{i:>4} {name:<34} {} invMass {:.3} {it}", if i < statics { "STATIC " } else { "dynamic" }, inv.get(i).copied().unwrap_or(f64::NAN));
    }
}
