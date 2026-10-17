//! Leg colliders for automatic motion: one capsule along each thigh and shin of
//! the hero skeleton, so long skirts, tails and coat tails stop at the legs
//! instead of passing through the body (SiViolet's skirt when walking
//! backwards). Valve's heroes protect their own cloth the same way
//! (`ClothShapeCapsule` on the legs), but those capsules are sized for the
//! hero's body; these are sized to the character's own leg: the 95th
//! percentile distance of the leg-dominant vertices from the bone, as the cloth
//! authoring flow measured them for the accepted Vindicta reference.
//!
//! Two hip capsules across the back of the hips keep a skirt's back panel out
//! of the butt when the hero walks backwards (Valve's heroes have one on the
//! pelvis as well): one at hip joint height and one lower, toward the bottom
//! of the butt. Each runs from hip to hip, as far back and as thick as the butt
//! allows (at most half its depth each) while every moving node stays clear,
//! so a back panel resting against the butt is stopped right where it rests.
//!
//! A collider never starts with moving cloth inside it. The thigh capsule
//! begins a little below the hip joint (a skirt's waistband sits at the hip),
//! and every radius is reduced until each moving proxy node clears it at rest;
//! a collider that would become thinner than [`MIN_RADIUS`] is left out.
use crate::{proxy_source::ColliderProposal, surface, Result};
use deadlock_skeleton::transforms::BindState;
use deadlock_core::CancellationToken;
use deadlock_dmx::Dmx;
use deadlock_skeleton::transforms;

/// Thigh and shin of each leg: (bone, tip bone, where along the bone the
/// capsule starts).
pub const LEGS: [(&str, &str, f64); 4] = [
    ("leg_upper_L", "leg_lower_L", 0.2),
    ("leg_lower_L", "ankle_L", 0.0),
    ("leg_upper_R", "leg_lower_R", 0.2),
    ("leg_lower_R", "ankle_R", 0.0),
];
/// Inches between a moving proxy node at rest and a collider's surface.
pub const CLEARANCE: f64 = 0.25;
/// Thinner colliders than this (inches) are left out.
pub const MIN_RADIUS: f64 = 0.75;

type V = [f64; 3];
fn sub(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: V, b: V) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn lerp(a: V, b: V, t: f64) -> V {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}
/// Distance from `p` to the segment a-b, and where along it (0..1, clamped).
pub fn segment_distance(p: V, a: V, b: V) -> (f64, f64) {
    let ab = sub(b, a);
    let len2 = dot(ab, ab);
    let t = if len2 <= 1e-12 { 0. } else { (dot(sub(p, a), ab) / len2).clamp(0., 1.) };
    let q = lerp(a, b, t);
    let d = sub(p, q);
    (dot(d, d).sqrt(), t)
}
fn apply(m: &transforms::Matrix4, p: V) -> V {
    std::array::from_fn(|row| m[row][3] + (0..3).map(|k| m[row][k] * p[k]).sum::<f64>())
}
fn cross(a: V, b: V) -> V {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn norm(a: V) -> f64 {
    dot(a, a).sqrt()
}
fn quantile95(mut samples: Vec<f64>) -> f64 {
    samples.sort_by(f64::total_cmp);
    samples[((samples.len() - 1) as f64 * 0.95).round() as usize]
}

/// Leg colliders for the character in `d` (the prepared model, on the hero
/// skeleton), clear of every point in `moving` (the moving proxy nodes at rest,
/// in model space). Legs the skeleton or the mesh does not have are skipped.
pub fn leg_colliders(d: &Dmx, moving: &[V], cancel: &CancellationToken) -> Result<Vec<ColliderProposal>> {
    let bind = surface::binds(d)?;
    let skeleton = &bind.skeleton;
    let state = transforms::bind_state(skeleton)?;
    let lookup = skeleton.indices();
    let palette: Vec<Option<usize>> = bind.palette.iter().map(|&e| lookup.get(d.elements[e].name.as_str()).copied()).collect();
    let meshes = surface::mesh_data(d)?;
    let mut out = vec![];
    for (bone, tip, start) in LEGS {
        let (Some(&index), Some(&tip_index)) = (lookup.get(bone), lookup.get(tip)) else { continue };
        let joint = |i: usize| -> V { std::array::from_fn(|row| state.world[i][row][3]) };
        let (top, bottom) = (joint(index), joint(tip_index));
        let a = lerp(top, bottom, start);
        // The leg's own surface: vertices mostly weighted to this bone, along its
        // middle (not the knee or hip, where neighboring bones share them).
        let mut samples = vec![];
        for mesh in meshes.values() {
            for (i, p) in mesh.positions.iter().enumerate() {
                if i % 4096 == 0 {
                    cancel.check()?;
                }
                if !mesh.drawn[i] {
                    continue;
                }
                let weight: f64 = mesh.joints[i]
                    .iter()
                    .zip(&mesh.weights[i])
                    .filter(|(slot, _)| palette.get(**slot).copied().flatten() == Some(index))
                    .map(|(_, w)| *w)
                    .sum();
                if weight < 0.5 {
                    continue;
                }
                let (distance, t) = segment_distance(*p, top, bottom);
                if (0.1..=0.9).contains(&t) {
                    samples.push(distance);
                }
            }
        }
        if samples.len() < 12 {
            continue;
        }
        let mut radius = quantile95(samples);
        for point in moving {
            let (distance, _) = segment_distance(*point, a, bottom);
            radius = radius.min(distance - CLEARANCE);
        }
        if !radius.is_finite() || radius < MIN_RADIUS {
            continue;
        }
        out.push(proposal(&state, index, bone, tip, [a, bottom], radius));
    }
    out.extend(hip_colliders(d, &state, &palette, &meshes, moving, cancel)?);
    Ok(out)
}

/// A capsule on `bone` between two model-space points, stored in the bone's
/// frame (the world points are recomputed from it exactly as the authoring
/// check does).
fn proposal(state: &BindState, index: usize, bone: &str, tip: &str, world: [V; 2], radius: f64) -> ColliderProposal {
    let local = world.map(|p| apply(&state.inverse[index], p));
    let radius_local = radius / state.accumulated[index];
    ColliderProposal {
        bone: bone.into(),
        tip_bone: tip.into(),
        endpoints_world: local.map(|p| apply(&state.world[index], p)),
        radius_world: radius_local * state.accumulated[index],
        endpoints_local: local,
        radius_local,
    }
}

/// Heights of the hip capsules below the hip joints, in thigh lengths.
pub const HIP_LEVELS: [f64; 2] = [0., 0.15];

/// The hip capsules on the pelvis, from hip joint to hip joint at each of
/// [`HIP_LEVELS`]. The butt's depth is the 95th percentile of how far the hip
/// and thigh vertices around hip height reach behind the hip joints. Each
/// capsule is moved back and thickened (up to half that depth each) as far as
/// the moving nodes allow, keeping its back surface as far back as possible.
fn hip_colliders(
    d: &Dmx,
    state: &BindState,
    palette: &[Option<usize>],
    meshes: &std::collections::BTreeMap<String, surface::MeshData>,
    moving: &[V],
    cancel: &CancellationToken,
) -> Result<Vec<ColliderProposal>> {
    let skeleton = &surface::binds(d)?.skeleton;
    let lookup = skeleton.indices();
    let (Some(&pelvis), Some(&left), Some(&right), Some(&knee_l), Some(&knee_r)) = (
        lookup.get("pelvis"),
        lookup.get("leg_upper_L"),
        lookup.get("leg_upper_R"),
        lookup.get("leg_lower_L"),
        lookup.get("leg_lower_R"),
    ) else {
        return Ok(vec![]);
    };
    let joint = |i: usize| -> V { std::array::from_fn(|row| state.world[i][row][3]) };
    let (hip_l, hip_r) = (joint(left), joint(right));
    let thigh = (norm(sub(joint(knee_l), hip_l)) + norm(sub(joint(knee_r), hip_r))) / 2.;
    let up = match lookup.get("head") {
        Some(&head) if norm(sub(joint(head), joint(pelvis))) > 1e-3 => {
            let v = sub(joint(head), joint(pelvis));
            v.map(|x| x / norm(v))
        }
        _ => [0., 0., 1.],
    };
    let lateral = sub(hip_l, hip_r);
    let side = sub(lateral, up.map(|x| x * dot(lateral, up)));
    if norm(side) < 1e-3 || thigh < 1e-3 {
        return Ok(vec![]);
    }
    let side = side.map(|x| x / norm(side));
    // The hero faces forward along side x up (Source: +Y left, +Z up, +X forward).
    let back = cross(side, up).map(|x| -x);
    let middle = lerp(hip_l, hip_r, 0.5);
    let half_width = norm(sub(hip_l, hip_r)) / 2.;
    let hips = [pelvis, left, right];
    let mut samples = vec![];
    for mesh in meshes.values() {
        for (i, p) in mesh.positions.iter().enumerate() {
            if i % 4096 == 0 {
                cancel.check()?;
            }
            if !mesh.drawn[i] {
                continue;
            }
            let weight: f64 = mesh.joints[i]
                .iter()
                .zip(&mesh.weights[i])
                .filter(|(slot, _)| palette.get(**slot).copied().flatten().is_some_and(|b| hips.contains(&b)))
                .map(|(_, w)| *w)
                .sum();
            let offset = sub(*p, middle);
            let behind = dot(offset, back);
            let height = dot(offset, up);
            if weight >= 0.5 && behind > 0. && (-thigh * 0.3..=thigh * 0.15).contains(&height) && dot(offset, side).abs() <= half_width {
                samples.push(behind);
            }
        }
    }
    if samples.len() < 12 {
        return Ok(vec![]);
    }
    let half_depth = quantile95(samples) / 2.;
    let mut out = vec![];
    for level in HIP_LEVELS {
        // The best of 17 set-backs from the hip line to half the butt's depth.
        // Keep the one whose back surface (set-back plus radius) reaches furthest.
        let mut best: Option<(f64, [V; 2], f64)> = None;
        for step in 0..=16 {
            let set_back = half_depth * step as f64 / 16.;
            let ends = [hip_l, hip_r].map(|p| -> V { std::array::from_fn(|k| p[k] - up[k] * thigh * level + back[k] * set_back) });
            let mut radius = half_depth;
            for point in moving {
                radius = radius.min(segment_distance(*point, ends[0], ends[1]).0 - CLEARANCE);
            }
            let reach = set_back + radius;
            if radius.is_finite() && radius >= MIN_RADIUS && best.as_ref().is_none_or(|(r, _, _)| reach > r + 1e-9) {
                best = Some((reach, ends, radius));
            }
        }
        if let Some((_, ends, radius)) = best {
            out.push(proposal(state, pelvis, "pelvis", "pelvis", ends, radius));
        }
    }
    Ok(out)
}

/// The moving nodes of an automatic proxy at rest: points whose goal strength
/// is below 1 (fixed points follow their bone and never collide).
pub fn moving_points(proxy: &Dmx) -> Result<Vec<V>> {
    let vertex = proxy
        .elements
        .iter()
        .find(|e| e.kind == "DmeVertexData" && e.get("cloth_enable$0").is_some())
        .ok_or_else(|| crate::invalid("Automatic proxy needs one cloth state."))?;
    let positions = surface::floats(proxy, vertex, "position$0", 42, 3)?;
    let goals = surface::floats(proxy, vertex, "cloth_goal_strength$0", 35, 1)?;
    let enabled = surface::floats(proxy, vertex, "cloth_enable$0", 35, 1)?;
    Ok(positions
        .chunks_exact(3)
        .enumerate()
        .filter(|(i, _)| goals.get(*i).is_some_and(|g| *g < 0.999) && enabled.get(*i).is_none_or(|e| *e > 0.))
        .map(|(_, p)| [p[0], p[1], p[2]])
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn segment_distance_clamps_to_the_ends() {
        let (a, b) = ([0., 0., 0.], [0., 0., -10.]);
        assert_eq!(segment_distance([3., 0., -5.], a, b), (3., 0.5));
        assert_eq!(segment_distance([0., 4., 3.], a, b), (5., 0.));
        assert_eq!(segment_distance([0., 0., -13.], a, b), (3., 1.));
    }
    #[test]
    fn skirt_travel_stops_short_of_the_body() {
        // Fabric 1.45 in from the body may travel 1.2 in, not its free 2.5 in.
        assert!((body_capped_skirt_travel(2.5, Some(1.45)) - 1.2).abs() < 1e-12);
        // Far from the body, or with no body nearby, the free travel stays.
        assert_eq!(body_capped_skirt_travel(2.5, Some(6.)), 2.5);
        assert_eq!(body_capped_skirt_travel(2.5, None), 2.5);
        // Fabric touching the body keeps a small travel; 0 would mean unlimited.
        assert_eq!(body_capped_skirt_travel(2.5, Some(0.1)), BODY_TRAVEL_FLOOR);
    }
    #[test]
    fn the_body_grid_finds_the_nearest_point_within_its_limit() {
        let grid = BodyGrid::new(&[[0., 0., 0.], [5., 0., 0.], [0., 9., 0.]]);
        assert_eq!(grid.nearest([4., 0., 0.], 3.), Some(1.));
        assert_eq!(grid.nearest([0., 4.5, 0.], 3.), None);
        assert!((grid.nearest([0., 4.5, 0.], 5.).unwrap() - 4.5).abs() < 1e-12);
    }
    #[test]
    fn the_95th_percentile_ignores_a_few_outliers() {
        let mut samples: Vec<f64> = (0..100).map(|_| 4.).collect();
        samples.extend([30., 40.]);
        assert_eq!(quantile95(samples), 4.);
    }
}

/// The lowest travel limit (inches) a body-capped skirt joint keeps; 0 would
/// mean unlimited.
pub const BODY_TRAVEL_FLOOR: f64 = 0.3;

/// Body vertices in a coarse grid, for "how close does this fabric come to
/// the body" queries.
pub(crate) struct BodyGrid {
    cell: f64,
    cells: std::collections::HashMap<[i64; 3], Vec<V>>,
}
impl BodyGrid {
    pub(crate) fn new(points: &[V]) -> Self {
        let cell = 1.5;
        let mut cells: std::collections::HashMap<[i64; 3], Vec<V>> = Default::default();
        for p in points {
            cells.entry(p.map(|v| (v / cell).floor() as i64)).or_default().push(*p);
        }
        Self { cell, cells }
    }
    /// Distance from `p` to the nearest body point, if one is within `limit`.
    pub(crate) fn nearest(&self, p: V, limit: f64) -> Option<f64> {
        let reach = (limit / self.cell).ceil() as i64 + 1;
        let home = p.map(|v| (v / self.cell).floor() as i64);
        let mut best = f64::INFINITY;
        for x in -reach..=reach {
            for y in -reach..=reach {
                for z in -reach..=reach {
                    for q in self.cells.get(&[home[0] + x, home[1] + y, home[2] + z]).into_iter().flatten() {
                        best = best.min(dot(sub(*q, p), sub(*q, p)).sqrt());
                    }
                }
            }
        }
        (best <= limit).then_some(best)
    }
}

/// How close each moving joint's own fabric comes to the body, for the joints
/// in `joints` (skeleton indices). A drawn vertex is fabric of the moving bone
/// that weights it most, when moving bones weight it at least half; it is body
/// when bones outside `moving` weight it at least half. Gaps beyond `limit`
/// are left out, and so are joints with no fabric of their own.
pub(crate) fn fabric_gaps(
    d: &Dmx,
    moving: &std::collections::BTreeSet<usize>,
    joints: &std::collections::BTreeSet<usize>,
    limit: f64,
    cancel: &CancellationToken,
) -> Result<std::collections::BTreeMap<usize, f64>> {
    let bind = surface::binds(d)?;
    let lookup = bind.skeleton.indices();
    let palette: Vec<Option<usize>> = bind.palette.iter().map(|&e| lookup.get(d.elements[e].name.as_str()).copied()).collect();
    let mut body = vec![];
    let mut fabric: std::collections::BTreeMap<usize, Vec<V>> = Default::default();
    for mesh in surface::mesh_data(d)?.values() {
        for i in 0..mesh.positions.len() {
            if i % 4096 == 0 {
                cancel.check()?;
            }
            if !mesh.drawn[i] {
                continue;
            }
            let (mut still, mut moved) = (0., 0.);
            let mut weights: std::collections::BTreeMap<usize, f64> = Default::default();
            for (slot, w) in mesh.joints[i].iter().zip(&mesh.weights[i]) {
                match palette.get(*slot).copied().flatten() {
                    Some(bone) if moving.contains(&bone) => {
                        moved += w;
                        *weights.entry(bone).or_default() += w;
                    }
                    _ => still += w,
                }
            }
            if still >= 0.5 {
                body.push(mesh.positions[i]);
            } else if moved >= 0.5 {
                if let Some((&bone, _)) = weights.iter().max_by(|a, b| a.1.total_cmp(b.1)) {
                    if joints.contains(&bone) {
                        fabric.entry(bone).or_default().push(mesh.positions[i]);
                    }
                }
            }
        }
    }
    let grid = BodyGrid::new(&body);
    let mut gaps = std::collections::BTreeMap::new();
    for (bone, points) in fabric {
        cancel.check()?;
        let step = points.len().div_ceil(256).max(1);
        let gap = points.iter().step_by(step).filter_map(|p| grid.nearest(*p, limit)).fold(f64::INFINITY, f64::min);
        if gap.is_finite() {
            gaps.insert(bone, gap);
        }
    }
    Ok(gaps)
}

/// A skirt joint's travel limit, capped so its fabric cannot reach the body:
/// at most the fabric's gap to the body less [`CLEARANCE`], never below
/// [`BODY_TRAVEL_FLOOR`].
pub fn body_capped_skirt_travel(travel: f64, gap: Option<f64>) -> f64 {
    match gap {
        Some(gap) => travel.min((gap - CLEARANCE).max(BODY_TRAVEL_FLOOR)),
        None => travel,
    }
}
