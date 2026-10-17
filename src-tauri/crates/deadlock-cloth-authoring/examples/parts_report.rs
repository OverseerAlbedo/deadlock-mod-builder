//! Development aid: the moving parts Analyze would author for a saved project,
//! from its prepared DMX, without the game or the compiler.
//!
//!   parts_report PROJECT.dmb.json
use deadlock_cloth_authoring::{secondary, surface};
use deadlock_core::CancellationToken;
use deadlock_dmx::Dmx;
use deadlock_project::Project;
use deadlock_skeleton::extensions::SourceBinding;
use sha2::{Digest, Sha256};
use std::path::Path;

fn main() {
    let path = std::env::args().nth(1).expect("PROJECT.dmb.json");
    let raw: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let project: Project = serde_json::from_value(raw.get("project").cloned().unwrap_or(raw)).unwrap();
    let dmx_path = Path::new(&path).parent().unwrap().join(&project.dmx);
    let d = Dmx::parse(std::fs::read(&dmx_path).unwrap()).unwrap();
    let mut skeleton = surface::binds(&d).unwrap().skeleton;
    skeleton.flags = vec![64; skeleton.names.len()];
    let extras: Vec<String> = skeleton.names.iter().filter(|n| n.starts_with("dmb_secondary_")).cloned().collect();
    let source = SourceBinding {
        source_sha256: format!("{:x}", Sha256::digest(d.bytes())),
        original_sha256: "0".repeat(64),
        skeleton,
        extra_names: extras.clone(),
        required_extra_names: extras,
    };
    let setup = secondary::setup(&project, &d, Some(&source), &CancellationToken::new()).unwrap().expect("automatic motion is on");
    for part in &setup.parts {
        println!(
            "{:<24} {:<10} {:>2} joints {:>5.1} in  {}",
            part.root,
            format!("{:?}", part.kind),
            part.weighted_chain.len(),
            part.chain_length_inches,
            part.reason
        );
    }
    for c in &setup.body_colliders {
        let length = (0..3).map(|k| (c.endpoints_world[1][k] - c.endpoints_world[0][k]).powi(2)).sum::<f64>().sqrt();
        println!("COLLIDER {} -> {}: radius {:.2} in, length {:.1} in", c.bone, c.tip_bone, c.radius_world, length);
    }
    // Which moving nodes shrink each leg collider: its radius with no cloth
    // nearby, and the closest moving nodes (named by their nearest bone).
    let cancel = CancellationToken::new();
    let free = deadlock_cloth_authoring::body_colliders::leg_colliders(&d, &[], &cancel).unwrap();
    let proxy = Dmx::parse(setup.proxy.clone()).unwrap();
    let moving = deadlock_cloth_authoring::body_colliders::moving_points(&proxy).unwrap();
    let skeleton = surface::binds(&d).unwrap().skeleton;
    let state = deadlock_skeleton::transforms::bind_state(&skeleton).unwrap();
    let nearest_bone = |p: [f64; 3]| {
        (0..skeleton.names.len())
            .min_by(|&a, &b| {
                let da: f64 = (0..3).map(|k| (state.world[a][k][3] - p[k]).powi(2)).sum();
                let db: f64 = (0..3).map(|k| (state.world[b][k][3] - p[k]).powi(2)).sum();
                da.total_cmp(&db)
            })
            .map(|i| skeleton.names[i].clone())
            .unwrap_or_default()
    };
    // Long-skirt evidence per group: weighted joints and their height range
    // relative to the pelvis, in torso lengths (head to pelvis).
    let lookup = skeleton.indices();
    let at = |n: &str| -> [f64; 3] { std::array::from_fn(|r| state.world[lookup[n]][r][3]) };
    if let (Some(_), Some(_)) = (lookup.get("head"), lookup.get("pelvis")) {
        let (head, pelvis) = (at("head"), at("pelvis"));
        let torso = (0..3).map(|k| (head[k] - pelvis[k]).powi(2)).sum::<f64>().sqrt();
        let up: Vec<f64> = (0..3).map(|k| (head[k] - pelvis[k]) / torso).collect();
        for (g, part) in setup.groups.iter().zip(&setup.parts) {
            if std::env::var("SKIRT").is_err() || !g.source_root.contains(std::env::var("SKIRT").unwrap().as_str()) { continue; }
            let h: Vec<f64> = g.weighted_bones.iter().map(|n| { let p = at(n); (0..3).map(|k| (p[k] - pelvis[k]) * up[k]).sum::<f64>() / torso }).collect();
            let top = h.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            let bottom = h.iter().copied().fold(f64::INFINITY, f64::min);
            println!("SKIRT {:<12} {:?} anchor {} weighted {} top {top:.2} bottom {bottom:.2} span {:.2}", g.source_root, part.kind, g.anchor, h.len(), top - bottom);
        }
        // The visible-fabric check of long skirts: heights (torso lengths) of the
        // vertices each matching group dominates, 5th and 95th percentile.
        if let Ok(filter) = std::env::var("SKIRT") {
            let bind = surface::binds(&d).unwrap();
            let owner: std::collections::BTreeMap<&str, usize> = setup.groups.iter().enumerate()
                .filter(|(_, g)| g.source_root.contains(filter.as_str()))
                .flat_map(|(i, g)| g.weighted_bones.iter().map(move |n| (n.as_str(), i))).collect();
            let slot_name: Vec<&str> = bind.palette.iter().map(|&e| d.elements[e].name.as_str()).collect();
            let mut samples: std::collections::BTreeMap<usize, Vec<f64>> = Default::default();
            for mesh in surface::mesh_data(&d).unwrap().values() {
                for i in 0..mesh.positions.len() {
                    if !mesh.drawn[i] { continue; }
                    let mut totals: std::collections::BTreeMap<usize, f64> = Default::default();
                    for (j, w) in mesh.joints[i].iter().zip(&mesh.weights[i]) {
                        if let Some(&g) = slot_name.get(*j).and_then(|n| owner.get(n)) { *totals.entry(g).or_default() += w; }
                    }
                    if let Some((&g, &w)) = totals.iter().max_by(|a, b| a.1.total_cmp(b.1)) {
                        if w >= 0.5 {
                            let p = mesh.positions[i];
                            samples.entry(g).or_default().push((0..3).map(|k| (p[k] - pelvis[k]) * up[k]).sum::<f64>() / torso);
                        }
                    }
                }
            }
            for (g, mut s) in samples {
                s.sort_by(f64::total_cmp);
                let (low, high) = (s[(s.len() - 1) * 5 / 100], s[(s.len() - 1) * 95 / 100]);
                println!("FABRIC {:<12} vertices {:>5} low {low:.2} high {high:.2} span {:.2} (long skirt needs low <= -0.50, high <= 0.35, span >= 0.65)", setup.groups[g].source_root, s.len(), high - low);
            }
        }
    }
    // Where the hip capsules sit relative to the hip joints, against the
    // direction the feet point (toes are in front).
    if let (Some(&hl), Some(&hr), Some(&ankle)) = (lookup.get("leg_upper_L"), lookup.get("leg_upper_R"), lookup.get("ankle_L")) {
        let p = |i: usize| -> [f64; 3] { std::array::from_fn(|r| state.world[i][r][3]) };
        let mid: Vec<f64> = (0..3).map(|k| (p(hl)[k] + p(hr)[k]) / 2.).collect();
        // The hero skeleton's toe bone (ankle_L also has a heel attachment).
        let toe = lookup.get("ball_L").copied().filter(|_| ankle < skeleton.names.len());
        if let Some(toe) = toe {
            let fwd: Vec<f64> = (0..3).map(|k| p(toe)[k] - p(ankle)[k]).collect();
            for c in setup.body_colliders.iter().filter(|c| c.bone == "pelvis") {
                let centre: Vec<f64> = (0..3).map(|k| (c.endpoints_world[0][k] + c.endpoints_world[1][k]) / 2. - mid[k]).collect();
                let along = (0..2).map(|k| centre[k] * fwd[k]).sum::<f64>() / (fwd[0].hypot(fwd[1]));
                println!("HIP capsule centre {:.2} in along the toe direction (negative = behind), {:.2} in above the hip joints", along, centre[2]);
            }
        }
    }
    // SKIRTGAP=<name part>: for each long-skirt joint matching it, its travel
    // limit and how close its own fabric comes to the body (vertices mostly
    // weighted to bones outside every moving part).
    if let Ok(filter) = std::env::var("SKIRTGAP") {
        let bind = surface::binds(&d).unwrap();
        let moving: std::collections::BTreeSet<&str> = setup.groups.iter().flat_map(|g| g.bones.iter().map(String::as_str)).collect();
        let slot_name: Vec<&str> = bind.palette.iter().map(|&e| d.elements[e].name.as_str()).collect();
        let meshes = surface::mesh_data(&d).unwrap();
        let mut body = vec![];
        let mut by_bone: std::collections::BTreeMap<&str, Vec<[f64; 3]>> = Default::default();
        for mesh in meshes.values() {
            for i in 0..mesh.positions.len() {
                if !mesh.drawn[i] { continue; }
                let mut share = 0.;
                for (j, w) in mesh.joints[i].iter().zip(&mesh.weights[i]) {
                    let name = slot_name.get(*j).copied().unwrap_or("");
                    if !moving.contains(name) { share += w; }
                    if *w >= 0.5 && moving.contains(name) { by_bone.entry(name).or_default().push(mesh.positions[i]); }
                }
                if share >= 0.5 { body.push(mesh.positions[i]); }
            }
        }
        let near = |p: [f64; 3]| body.iter().map(|q| (0..3).map(|k| (p[k] - q[k]).powi(2)).sum::<f64>()).fold(f64::INFINITY, f64::min).sqrt();
        for group in setup.long_skirt.iter().flat_map(|plan| &plan.groups) {
            for (bone, travel) in &group.travel_radius_by_bone {
                if !bone.contains(filter.as_str()) { continue; }
                let patch = by_bone.get(bone.as_str()).cloned().unwrap_or_default();
                let gap = patch.iter().step_by(patch.len().div_ceil(200).max(1)).map(|p| near(*p)).fold(f64::INFINITY, f64::min);
                let node = near(at(bone));
                println!("SKIRTGAP {bone:<28} travel {travel:5.2} in  fabric-to-body {gap:5.2} in  joint-to-body {node:5.2} in  ({} fabric vertices)", patch.len());
            }
        }
    }
    // HEAD=1: the head's own vertices (mostly weighted to the head bone) around
    // their centre, and the moving nodes closest to that centre.
    if std::env::var("HEAD").is_ok() {
        let bind = surface::binds(&d).unwrap();
        let slot_name: Vec<&str> = bind.palette.iter().map(|&e| d.elements[e].name.as_str()).collect();
        let mut head = vec![];
        for mesh in surface::mesh_data(&d).unwrap().values() {
            for i in 0..mesh.positions.len() {
                if !mesh.drawn[i] { continue; }
                let w: f64 = mesh.joints[i].iter().zip(&mesh.weights[i]).filter(|(j, _)| slot_name.get(**j) == Some(&"head")).map(|(_, w)| *w).sum();
                if w >= 0.9 { head.push(mesh.positions[i]); }
            }
        }
        let c: [f64; 3] = std::array::from_fn(|k| head.iter().map(|p| p[k]).sum::<f64>() / head.len() as f64);
        let dist = |p: &[f64; 3]| (0..3).map(|k| (p[k] - c[k]).powi(2)).sum::<f64>().sqrt();
        let mut ds: Vec<f64> = head.iter().map(dist).collect();
        ds.sort_by(f64::total_cmp);
        let q = |f: f64| ds[((ds.len() - 1) as f64 * f) as usize];
        let hj = lookup.get("head").map(|&i| at(&skeleton.names[i])).unwrap_or([0.; 3]);
        println!("HEAD {} vertices, centre [{:.2},{:.2},{:.2}] (head joint [{:.2},{:.2},{:.2}]), distance p10 {:.2} p50 {:.2} p95 {:.2}", head.len(), c[0], c[1], c[2], hj[0], hj[1], hj[2], q(0.1), q(0.5), q(0.95));
        let mut near: Vec<(f64, String)> = moving.iter().map(|p| (dist(p), nearest_bone(*p))).collect();
        near.sort_by(|x, y| x.0.total_cmp(&y.0));
        let mut seen = std::collections::BTreeSet::new();
        near.retain(|x| seen.insert(x.1.clone()));
        println!("HEAD closest moving nodes: {}", near.iter().take(12).map(|(d, n)| format!("{} {d:.2}", n.replace("dmb_secondary_", ""))).collect::<Vec<_>>().join(", "));
    }
    for c in &free {
        let (a, b) = (c.endpoints_world[0], c.endpoints_world[1]);
        let mut near: Vec<(f64, String)> = moving
            .iter()
            .map(|p| (deadlock_cloth_authoring::body_colliders::segment_distance(*p, a, b).0, nearest_bone(*p)))
            .collect();
        near.sort_by(|x, y| x.0.total_cmp(&y.0));
        let mut seen = std::collections::BTreeSet::new();
        near.retain(|x| seen.insert(x.1.clone()));
        let closest: Vec<String> = near.iter().take(6).map(|(d, n)| format!("{n} {d:.2}")).collect();
        println!("FREE {} radius {:.2} in; closest moving nodes: {}", c.bone, c.radius_world, closest.join(", "));
    }
}
