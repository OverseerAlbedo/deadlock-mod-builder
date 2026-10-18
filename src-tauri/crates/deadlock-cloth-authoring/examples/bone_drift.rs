//! Development aid: how far each bone of a prepared DMX moved in a compiled
//! model (local transforms, inches and degrees), worst first, and how far the
//! compiled animation rest table (AGRP) is from the same source values.
//!
//!   bone_drift PREPARED.dmx COMPILED.vmdl_c
//!   DMB_DRIFT_FILTER=hairE bone_drift ...   (also print matching bones in full)
use deadlock_cloth_authoring::surface;
use deadlock_dmx::Dmx;
use deadlock_resource::{container, Kind, Value};

fn angle(a: &[f64; 4], b: &[f64; 4]) -> f64 {
    let n = |q: &[f64; 4]| q.iter().map(|x| x * x).sum::<f64>().sqrt();
    let dot = a.iter().zip(b).map(|(x, y)| x * y).sum::<f64>().abs() / (n(a) * n(b));
    (2. * dot.min(1.).acos()).to_degrees()
}
fn numbers(value: &Value) -> Vec<f64> {
    match &value.kind {
        Kind::Array(items) => items.iter().flat_map(numbers).collect(),
        Kind::F32(v) => vec![*v as f64],
        Kind::F64(v) => vec![*v],
        Kind::I64(v) => vec![*v as f64],
        _ => vec![],
    }
}
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let d = Dmx::parse(std::fs::read(&args[0]).unwrap()).unwrap();
    let source = surface::binds(&d).unwrap().skeleton;
    let raw = std::fs::read(&args[1]).unwrap();
    let compiled = deadlock_cloth::audit_resource(&raw).unwrap().skeleton;
    let index = compiled.indices();
    // Compiled animation rest values by bone name: (position, rotation, scale).
    let mut rest = std::collections::BTreeMap::new();
    for group in container::read_blocks(&raw, "AGRP").unwrap() {
        let Some(bones) = group.root.get("m_decodeKey").and_then(|k| k.get("m_boneArray")) else { continue };
        let Kind::Array(bones) = &bones.kind else { continue };
        for bone in bones {
            let (Some(name), Some(p), Some(q), Some(s)) = (bone.get("m_name"), bone.get("m_pos"), bone.get("m_quat"), bone.get("m_scale")) else { continue };
            let Kind::String(name) = &name.kind else { continue };
            let (p, q, s) = (numbers(p), numbers(q), numbers(s));
            rest.insert(name.clone(), ([p[0], p[1], p[2]], [q[0], q[1], q[2], q[3]], s[0]));
        }
    }
    let filter = std::env::var("DMB_DRIFT_FILTER").ok();
    let (mut worst_data, mut worst_rest) = ((0_f64, 0_f64, 0_f64), (0_f64, 0_f64, 0_f64));
    let mut extras = 0;
    for (i, name) in source.names.iter().enumerate().filter(|(_, n)| n.starts_with("dmb_secondary_")) {
        let Some(&j) = index.get(name.as_str()) else { continue };
        extras += 1;
        let distance = |p: &[f64; 3]| (0..3).map(|k| (source.positions[i][k] - p[k]).powi(2)).sum::<f64>().sqrt();
        let data = (distance(&compiled.positions[j]), angle(&source.rotations[i], &compiled.rotations[j]), (source.scales[i] - compiled.scales[j]).abs());
        worst_data = (worst_data.0.max(data.0), worst_data.1.max(data.1), worst_data.2.max(data.2));
        let animation = rest.get(name).map(|(p, q, s)| (distance(p), angle(&source.rotations[i], q), (source.scales[i] - s).abs()));
        if let Some(a) = animation {
            worst_rest = (worst_rest.0.max(a.0), worst_rest.1.max(a.1), worst_rest.2.max(a.2));
        }
        if filter.as_deref().is_some_and(|f| name.contains(f)) {
            println!("{name}: model data moved {:.6} in, {:.5} deg, scale {:.6}; animation rest {:?}", data.0, data.1, data.2, animation);
        }
    }
    println!("{extras} moving-part bones (worst: inches, degrees, scale difference)");
    println!("  model skeleton (DATA) vs prepared: {worst_data:.6?}");
    println!("  animation rest (AGRP) vs prepared: {worst_rest:.6?}  ({} rest bones read)", rest.len());
}
