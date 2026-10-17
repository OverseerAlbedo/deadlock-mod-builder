//! Development aid: for each bone whose name contains FILTER, the centre and
//! size of the drawn vertices it weights most (inches), and its bind position
//! and parent, to tell which bone drives which visible piece.
//!
//!   group_mesh PREPARED.dmx FILTER
use deadlock_cloth_authoring::surface;
use deadlock_dmx::Dmx;
use std::collections::BTreeMap;
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let d = Dmx::parse(std::fs::read(&args[0]).unwrap()).unwrap();
    let bind = surface::binds(&d).unwrap();
    let sk = &bind.skeleton;
    let state = deadlock_skeleton::transforms::bind_state(sk).unwrap();
    let slot_name: Vec<&str> = bind.palette.iter().map(|&e| d.elements[e].name.as_str()).collect();
    let mut points: BTreeMap<&str, Vec<[f64; 3]>> = BTreeMap::new();
    for mesh in surface::mesh_data(&d).unwrap().values() {
        for i in 0..mesh.positions.len() {
            if !mesh.drawn[i] { continue; }
            if let Some((j, w)) = mesh.joints[i].iter().zip(&mesh.weights[i]).max_by(|a, b| a.1.total_cmp(b.1)) {
                if *w >= 0.5 { points.entry(slot_name.get(*j).copied().unwrap_or("?")).or_default().push(mesh.positions[i]); }
            }
        }
    }
    for (i, name) in sk.names.iter().enumerate() {
        if !name.contains(args[1].as_str()) { continue; }
        let at: [f64; 3] = std::array::from_fn(|r| state.world[i][r][3]);
        let parent = if sk.parents[i] >= 0 { sk.names[sk.parents[i] as usize].as_str() } else { "-" };
        let rotation = sk.rotations[i];
        match points.get(name.as_str()) {
            Some(p) if !p.is_empty() => {
                let c: [f64; 3] = std::array::from_fn(|k| p.iter().map(|q| q[k]).sum::<f64>() / p.len() as f64);
                let lo: [f64; 3] = std::array::from_fn(|k| p.iter().map(|q| q[k]).fold(f64::INFINITY, f64::min));
                let hi: [f64; 3] = std::array::from_fn(|k| p.iter().map(|q| q[k]).fold(f64::NEG_INFINITY, f64::max));
                println!("{name:<34} bone [{:6.2},{:6.2},{:6.2}] parent {parent:<26} rot [{:.3},{:.3},{:.3},{:.3}] | {} verts centre [{:6.2},{:6.2},{:6.2}] size [{:.1},{:.1},{:.1}]",
                    at[0], at[1], at[2], rotation[0], rotation[1], rotation[2], rotation[3], p.len(), c[0], c[1], c[2], hi[0]-lo[0], hi[1]-lo[1], hi[2]-lo[2]);
            }
            _ => println!("{name:<34} bone [{:6.2},{:6.2},{:6.2}] parent {parent:<26} rot [{:.3},{:.3},{:.3},{:.3}] | no vertices of its own", at[0], at[1], at[2], rotation[0], rotation[1], rotation[2], rotation[3]),
        }
    }
}
