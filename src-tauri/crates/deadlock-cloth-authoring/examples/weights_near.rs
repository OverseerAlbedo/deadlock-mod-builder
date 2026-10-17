//! Development aid: which bones weight the drawn vertices near a bone (within
//! RADIUS inches of its bind position), summed per bone.
//!
//!   weights_near PREPARED.dmx BONE [RADIUS]
use deadlock_cloth_authoring::surface;
use deadlock_dmx::Dmx;
use std::collections::BTreeMap;
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let d = Dmx::parse(std::fs::read(&args[0]).unwrap()).unwrap();
    let radius: f64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(2.0);
    let bind = surface::binds(&d).unwrap();
    let state = deadlock_skeleton::transforms::bind_state(&bind.skeleton).unwrap();
    let lookup = bind.skeleton.indices();
    let &bone = lookup.get(args[1].as_str()).expect("bone");
    let at: [f64; 3] = std::array::from_fn(|r| state.world[bone][r][3]);
    let slot_name: Vec<&str> = bind.palette.iter().map(|&e| d.elements[e].name.as_str()).collect();
    let mut totals: BTreeMap<&str, (f64, usize)> = BTreeMap::new();
    let mut count = 0;
    for mesh in surface::mesh_data(&d).unwrap().values() {
        for i in 0..mesh.positions.len() {
            if !mesh.drawn[i] { continue; }
            let p = mesh.positions[i];
            if (0..3).map(|k| (p[k] - at[k]).powi(2)).sum::<f64>().sqrt() > radius { continue; }
            count += 1;
            for (j, w) in mesh.joints[i].iter().zip(&mesh.weights[i]) {
                if *w <= 0. { continue; }
                let e = totals.entry(slot_name.get(*j).copied().unwrap_or("?")).or_default();
                e.0 += w; e.1 += 1;
            }
        }
    }
    println!("{} at [{:.2}, {:.2}, {:.2}]: {count} vertices within {radius} in", args[1], at[0], at[1], at[2]);
    let mut rows: Vec<_> = totals.into_iter().collect();
    rows.sort_by(|a, b| b.1 .0.total_cmp(&a.1 .0));
    for (name, (w, n)) in rows.iter().take(10) { println!("  {name:<40} weight {w:8.2} on {n} vertices"); }
}
