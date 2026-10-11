//! Bounded exact welding, overlay eligibility and conforming edge subdivision.
use crate::{
    invalid,
    surface::{triangles, MeshData, Selection},
    Result,
};
use deadlock_core::CancellationToken;
use deadlock_dmx::Dmx;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
pub(crate) fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| a[i] - b[i])
}
pub(crate) fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| a[i] + b[i])
}
pub(crate) fn mul(a: [f64; 3], n: f64) -> [f64; 3] {
    a.map(|v| v * n)
}
pub(crate) fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
pub(crate) fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
pub(crate) fn norm(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}
fn edge(a: usize, b: usize) -> (usize, usize) {
    if a < b {
        (a, b)
    } else {
        (b, a)
    }
}
fn bits(v: f64) -> u64 {
    if v == 0. {
        0
    } else {
        v.to_bits()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn thin_face_projection_retains_interior_barycentric_skin_weights() {
        let (distance, bary) = super::closest(
            [0.75, 0.25e-9, 0.],
            [[0., 0., 0.], [1., 0., 0.], [1., 1e-9, 0.]],
        );
        assert!(distance <= f64::EPSILON);
        for (a, b) in bary.into_iter().zip([0.25, 0.5, 0.25]) {
            assert!((a - b).abs() <= 1e-12);
        }
    }
}

pub(crate) fn closest(point: [f64; 3], t: [[f64; 3]; 3]) -> (f64, [f64; 3]) {
    let ab = sub(t[1], t[0]);
    let ac = sub(t[2], t[0]);
    let ap = sub(point, t[0]);
    let n = cross(ab, ac);
    let denominator = dot(n, n);
    let u = dot(cross(ap, ac), n) / denominator;
    let v = dot(cross(ab, ap), n) / denominator;
    let mut bary = [1. - u - v, u, v];
    let projected = add(
        add(mul(t[0], bary[0]), mul(t[1], bary[1])),
        mul(t[2], bary[2]),
    );
    let mut distance = if bary.iter().any(|v| *v < 0.) {
        f64::INFINITY
    } else {
        norm(sub(projected, point))
    };
    for (x, y) in [(0, 1), (1, 2), (2, 0)] {
        let e = sub(t[y], t[x]);
        let n = (dot(sub(point, t[x]), e) / dot(e, e)).clamp(0., 1.);
        let d = norm(sub(add(t[x], mul(e, n)), point));
        if d < distance {
            distance = d;
            bary = [0.; 3];
            bary[x] = 1. - n;
            bary[y] = n;
        }
    }
    (distance, bary)
}
struct Node {
    lo: [f64; 3],
    hi: [f64; 3],
    children: Option<(usize, usize)>,
    indices: Vec<usize>,
}
struct Tree {
    nodes: Vec<Node>,
}
impl Tree {
    fn new(triangles: &[[[f64; 3]; 3]], cancel: &CancellationToken) -> Result<Self> {
        let bounds = triangles
            .iter()
            .map(|t| {
                let lo =
                    std::array::from_fn(|k| t.iter().map(|p| p[k]).fold(f64::INFINITY, f64::min));
                let hi = std::array::from_fn(|k| {
                    t.iter().map(|p| p[k]).fold(f64::NEG_INFINITY, f64::max)
                });
                (lo, hi, mul(add(lo, hi), 0.5))
            })
            .collect::<Vec<_>>();
        let mut tree = Self { nodes: vec![] };
        tree.build((0..triangles.len()).collect(), &bounds, cancel)?;
        Ok(tree)
    }
    fn build(
        &mut self,
        mut ids: Vec<usize>,
        bounds: &[([f64; 3], [f64; 3], [f64; 3])],
        cancel: &CancellationToken,
    ) -> Result<usize> {
        cancel.check()?;
        let index = self.nodes.len();
        let lo = std::array::from_fn(|k| {
            ids.iter()
                .map(|&i| bounds[i].0[k])
                .fold(f64::INFINITY, f64::min)
        });
        let hi = std::array::from_fn(|k| {
            ids.iter()
                .map(|&i| bounds[i].1[k])
                .fold(f64::NEG_INFINITY, f64::max)
        });
        self.nodes.push(Node {
            lo,
            hi,
            children: None,
            indices: vec![],
        });
        if ids.len() <= 32 {
            self.nodes[index].indices = ids;
        } else {
            let mut axis = 0;
            let mut width = f64::NEG_INFINITY;
            for k in 0..3 {
                let max = ids
                    .iter()
                    .map(|&i| bounds[i].2[k])
                    .fold(f64::NEG_INFINITY, f64::max);
                let min = ids
                    .iter()
                    .map(|&i| bounds[i].2[k])
                    .fold(f64::INFINITY, f64::min);
                if max - min > width {
                    width = max - min;
                    axis = k;
                }
            }
            ids.sort_by(|a, b| bounds[*a].2[axis].total_cmp(&bounds[*b].2[axis]));
            let right = ids.split_off(ids.len() / 2);
            let a = self.build(ids, bounds, cancel)?;
            let b = self.build(right, bounds, cancel)?;
            self.nodes[index].children = Some((a, b));
        }
        Ok(index)
    }
    fn candidates(&self, point: [f64; 3], max: f64) -> Vec<usize> {
        let mut pending = vec![0];
        let mut ids = vec![];
        while let Some(i) = pending.pop() {
            let node = &self.nodes[i];
            let delta =
                std::array::from_fn(|k| (node.lo[k] - point[k]).max(point[k] - node.hi[k]).max(0.));
            if dot(delta, delta) > max * max {
                continue;
            }
            if let Some((a, b)) = node.children {
                pending.extend([a, b]);
            } else {
                ids.extend(&node.indices);
            }
        }
        ids
    }
}

#[derive(Debug)]
pub(crate) struct Surface {
    pub points: Vec<[f64; 3]>,
    pub faces: Vec<[usize; 3]>,
    pub influences: Vec<BTreeMap<usize, f64>>,
    pub enabled: Vec<bool>,
    pub initial_proxy_positions: usize,
    pub fixed_positions_not_on_dynamic_faces: usize,
    pub components: usize,
    pub degenerate_triangles_excluded: usize,
    pub overlay_followers: Value,
}

fn overlay(
    points: &[[f64; 3]],
    faces: &[[usize; 3]],
    source_faces: &[[usize; 3]],
    weights: &[BTreeMap<usize, f64>],
    flags: &[bool],
    groups: &[BTreeSet<usize>],
    source_ids: &[Vec<usize>],
    cancel: &CancellationToken,
) -> Result<(Vec<usize>, Vec<usize>, Value)> {
    let anchored = groups
        .iter()
        .map(|g| g.iter().any(|&i| !flags[i]))
        .collect::<Vec<_>>();
    if !anchored.iter().any(|v| *v) {
        return Err(invalid("Each cloth surface component needs a connected fixed boundary; no anchored surface found"));
    }
    let mut report = json!({"policy":"bounded-direct-cloth-overlay-v1","native_binding_verified":false,"eligibility_only":true,"distance_limit_mm":2.,"per_influence_delta_limit":0.05,"weight_l1_limit":0.10,"ambiguity_margin_mm":0.1,"minimum_support_margin_mm":null,"components":[],"component_count":0,"source_position_count":0,"welded_position_count":0,"max_distance_mm":0.,"max_weight_delta":0.,"max_weight_l1":0.});
    if anchored.iter().all(|v| *v) {
        return Ok((
            (0..points.len()).collect(),
            (0..faces.len()).collect(),
            report,
        ));
    }
    let mut group_ids = vec![0; points.len()];
    for (i, g) in groups.iter().enumerate() {
        for &v in g {
            group_ids[v] = i;
        }
    }
    let kept_faces = (0..faces.len())
        .filter(|&i| anchored[group_ids[faces[i][0]]])
        .collect::<Vec<_>>();
    let geometry = kept_faces
        .iter()
        .map(|&i| faces[i].map(|j| points[j]))
        .collect::<Vec<_>>();
    let tree = Tree::new(&geometry, cancel)?;
    let bones = weights
        .iter()
        .flat_map(|v| v.keys().copied())
        .collect::<BTreeSet<_>>();
    let mut components = vec![];
    let (mut source_count, mut welded_count) = (0, 0);
    let (mut maximum_distance, mut maximum_delta, mut maximum_l1) = (0f64, 0f64, 0f64);
    let mut minimum_margin: Option<f64> = None;
    for (component, group) in groups.iter().enumerate() {
        if anchored[component] {
            continue;
        }
        let mut chosen = None;
        let mut matches = vec![];
        for &vertex in group {
            cancel.check()?;
            let mut candidates = vec![];
            for index in tree.candidates(points[vertex], 2. / 25.4) {
                let (distance, bary) = closest(points[vertex], geometry[index]);
                if distance > 2. / 25.4 {
                    continue;
                }
                let triangle = faces[kept_faces[index]];
                let (mut max_delta, mut l1) = (0f64, 0f64);
                for bone in &bones {
                    let target = (0..3)
                        .map(|k| weights[triangle[k]].get(bone).copied().unwrap_or(0.) * bary[k])
                        .sum::<f64>();
                    let delta = (target - weights[vertex].get(bone).copied().unwrap_or(0.)).abs();
                    max_delta = max_delta.max(delta);
                    l1 += delta;
                }
                candidates.push((index, distance, bary, max_delta, l1, group_ids[triangle[0]]));
            }
            if candidates.is_empty() {
                return Err(invalid("Unpinned cloth component is not a bounded overlay: no anchored surface within 2 mm"));
            }
            let mut nearest = 0;
            for i in 1..candidates.len() {
                if candidates[i].1 < candidates[nearest].1 {
                    nearest = i;
                }
            }
            let &(face, distance, bary, max_delta, l1, target) = &candidates[nearest];
            if max_delta > 0.05 || l1 > 0.10 {
                return Err(invalid("Unpinned cloth overlay has incompatible skin binding at its nearest anchored surface"));
            }
            let competitor = candidates
                .iter()
                .filter(|c| c.3 <= 0.05 && c.4 <= 0.10 && c.5 != target)
                .map(|c| c.1 * 25.4)
                .min_by(f64::total_cmp);
            let margin = competitor.map(|d| d - distance * 25.4);
            if margin.is_some_and(|m| m <= 0.1) {
                return Err(invalid("Unpinned cloth overlay has ambiguous support from different anchored components"));
            }
            if chosen.is_some_and(|c| c != target) {
                return Err(invalid(
                    "Unpinned cloth overlay crosses different anchored components",
                ));
            }
            chosen = Some(target);
            if let Some(margin) = margin {
                minimum_margin = Some(minimum_margin.map(|old| old.min(margin)).unwrap_or(margin));
            }
            maximum_distance = maximum_distance.max(distance * 25.4);
            maximum_delta = maximum_delta.max(max_delta);
            maximum_l1 = maximum_l1.max(l1);
            matches.push(json!({"source_position_ids":source_ids[vertex],"triangle_source_positions":source_faces[kept_faces[face]],"barycentric":bary,"distance_mm":distance*25.4,"max_weight_delta":max_delta,"weight_l1":l1,"runner_up_component_distance_mm":competitor,"support_margin_mm":margin}));
        }
        let mut sources = group
            .iter()
            .flat_map(|&i| source_ids[i].clone())
            .collect::<Vec<_>>();
        sources.sort_unstable();
        source_count += sources.len();
        welded_count += group.len();
        components.push(json!({"component":component,"anchor_component":chosen,"excluded_proxy_source_positions":sources,"correspondences":matches}));
    }
    report["component_count"] = json!(components.len());
    report["components"] = json!(components);
    report["source_position_count"] = json!(source_count);
    report["welded_position_count"] = json!(welded_count);
    report["max_distance_mm"] = json!(maximum_distance);
    report["max_weight_delta"] = json!(maximum_delta);
    report["max_weight_l1"] = json!(maximum_l1);
    report["minimum_support_margin_mm"] = json!(minimum_margin);
    Ok((
        (0..points.len())
            .filter(|&i| anchored[group_ids[i]])
            .collect(),
        kept_faces,
        report,
    ))
}

pub(crate) fn surface(
    d: &Dmx,
    row: &MeshData,
    selected: &Selection,
    max_edge: f64,
    budget: usize,
    cancel: &CancellationToken,
) -> Result<Surface> {
    let count = row.positions.len();
    for ids in [&selected.enabled, &selected.pinned] {
        if ids.is_empty()
            || ids.len() != ids.iter().collect::<BTreeSet<_>>().len()
            || ids.iter().any(|&v| v >= count)
        {
            return Err(invalid(
                "Cloth selection needs unique in-range enabled and pinned position IDs",
            ));
        }
    }
    let enabled = selected.enabled.iter().copied().collect::<BTreeSet<_>>();
    let pinned = selected.pinned.iter().copied().collect::<BTreeSet<_>>();
    if !enabled.is_disjoint(&pinned) {
        return Err(invalid(
            "A cloth position cannot be both enabled and pinned",
        ));
    }
    let all = enabled.union(&pinned).copied().collect::<BTreeSet<_>>();
    let active = (0..count).map(|i| enabled.contains(&i)).collect::<Vec<_>>();
    let mut selected_faces = vec![];
    for face in triangles(d, row, cancel, Some(&active))? {
        if face.iter().any(|i| enabled.contains(i)) {
            if face.iter().any(|i| !all.contains(i)) {
                return Err(invalid("Cloth selection cuts through a surface face; include fixed boundary vertices as pinned"));
            }
            selected_faces.push(face);
        }
    }
    let used = selected_faces
        .iter()
        .flatten()
        .copied()
        .collect::<BTreeSet<_>>();
    if selected_faces.is_empty() || !enabled.is_subset(&used) {
        return Err(invalid(
            "Cloth selection contains enabled positions without complete surface faces",
        ));
    }
    let (mut points, mut influences, mut flags, mut remap, mut lookup) =
        (vec![], vec![], vec![], BTreeMap::new(), BTreeMap::new());
    for old in used {
        let mut weights = BTreeMap::new();
        for (&joint, &weight) in row.joints[old].iter().zip(&row.weights[old]) {
            if weight > 0. {
                *weights.entry(joint).or_insert(0.) += weight;
            }
        }
        let position = row.positions[old];
        if position.iter().any(|v| !v.is_finite()) {
            return Err(invalid("Non-finite cloth source position"));
        }
        let key = (
            position.map(bits),
            weights
                .iter()
                .map(|(&i, &v)| (i, bits(v)))
                .collect::<Vec<_>>(),
            enabled.contains(&old),
        );
        let next = points.len();
        let i = *lookup.entry(key).or_insert_with(|| {
            points.push(position);
            influences.push(weights);
            flags.push(enabled.contains(&old));
            next
        });
        remap.insert(old, i);
    }
    let (mut faces, mut source_faces, mut degenerate) = (vec![], vec![], 0);
    for face in selected_faces {
        let t = face.map(|i| remap[&i]);
        if t.iter().collect::<BTreeSet<_>>().len() < 3
            || cross(
                sub(points[t[1]], points[t[0]]),
                sub(points[t[2]], points[t[0]]),
            )
            .iter()
            .all(|v| *v == 0.)
        {
            degenerate += 1;
        } else {
            faces.push(t);
            source_faces.push(face);
        }
    }
    if faces.is_empty() || points.len() > budget {
        return Err(invalid(
            "Selected garment exceeds bounded proxy budget or has no non-degenerate faces",
        ));
    }
    let mut adjacency = vec![BTreeSet::new(); points.len()];
    for &[a, b, c] in &faces {
        adjacency[a].extend([b, c]);
        adjacency[b].extend([a, c]);
        adjacency[c].extend([a, b]);
    }
    let (mut visited, mut groups) = (BTreeSet::<usize>::new(), vec![]);
    for start in 0..points.len() {
        if visited.contains(&start) {
            continue;
        }
        if adjacency[start].is_empty() {
            return Err(invalid(
                "Cloth surface component has no non-degenerate faces",
            ));
        }
        let (mut pending, mut group) = (vec![start], BTreeSet::new());
        while let Some(i) = pending.pop() {
            if group.insert(i) {
                pending.extend(adjacency[i].iter().filter(|v| !group.contains(v)).copied());
            }
        }
        visited.extend(&group);
        groups.push(group);
    }
    let mut source_ids = vec![vec![]; points.len()];
    for (&old, &i) in &remap {
        source_ids[i].push(old);
    }
    let (kept, kept_faces, overlays) = overlay(
        &points,
        &faces,
        &source_faces,
        &influences,
        &flags,
        &groups,
        &source_ids,
        cancel,
    )?;
    let removed = overlays["component_count"].as_u64().unwrap() as usize;
    if removed > 0 {
        let mapping = kept
            .iter()
            .enumerate()
            .map(|(i, &old)| (old, i))
            .collect::<BTreeMap<_, _>>();
        points = kept.iter().map(|&i| points[i]).collect();
        influences = kept.iter().map(|&i| influences[i].clone()).collect();
        flags = kept.iter().map(|&i| flags[i]).collect();
        faces = kept_faces
            .iter()
            .map(|&i| faces[i].map(|v| mapping[&v]))
            .collect();
    }
    let initial = points.len();
    let mut converged = false;
    for _ in 0..32 {
        cancel.check()?;
        let edges = faces
            .iter()
            .flat_map(|t| (0..3).map(|i| edge(t[i], t[(i + 1) % 3])))
            .collect::<BTreeSet<_>>();
        let long = edges
            .into_iter()
            .filter(|&(a, b)| norm(sub(points[a], points[b])) > max_edge * (1. + 1e-7))
            .collect::<Vec<_>>();
        if long.is_empty() {
            converged = true;
            break;
        }
        if points.len() + long.len() > budget {
            return Err(invalid(
                "Requested cloth spacing exceeds bounded proxy budget",
            ));
        }
        let mut mid = BTreeMap::new();
        for (a, b) in long {
            mid.insert((a, b), points.len());
            points.push(mul(add(points[a], points[b]), 0.5));
            let mut values = BTreeMap::new();
            for parent in [a, b] {
                for (&bone, &weight) in &influences[parent] {
                    *values.entry(bone).or_insert(0.) += weight * 0.5;
                }
            }
            influences.push(values);
            flags.push(flags[a] || flags[b]);
        }
        let mut refined = vec![];
        for t in faces {
            let mids =
                std::array::from_fn::<_, 3, _>(|i| mid.get(&edge(t[i], t[(i + 1) % 3])).copied());
            let number = mids.iter().filter(|m| m.is_some()).count();
            match number {
                0 => refined.push(t),
                3 => {
                    let [a, b, c] = t;
                    let [x, y, z] = mids.map(Option::unwrap);
                    refined.extend([[a, x, z], [x, b, y], [z, y, c], [x, y, z]]);
                }
                _ => {
                    let rotation = if number == 2 {
                        (mids.iter().position(Option::is_none).unwrap() + 1) % 3
                    } else {
                        mids.iter().position(Option::is_some).unwrap()
                    };
                    let [a, b, c] = std::array::from_fn(|i| t[(i + rotation) % 3]);
                    let x = mids[rotation].unwrap();
                    if number == 1 {
                        refined.extend([[a, x, c], [x, b, c]]);
                    } else {
                        let y = mids[(rotation + 1) % 3].unwrap();
                        refined.extend([[x, b, y], [a, x, c], [x, y, c]]);
                    }
                }
            }
        }
        faces = refined;
    }
    if !converged {
        return Err(invalid(
            "Cloth surface refinement did not converge within 32 iterations",
        ));
    }
    Ok(Surface {
        points,
        faces,
        influences,
        enabled: flags,
        initial_proxy_positions: initial,
        fixed_positions_not_on_dynamic_faces: pinned
            .iter()
            .filter(|i| !remap.contains_key(i))
            .count(),
        components: groups.len() - removed,
        degenerate_triangles_excluded: degenerate,
        overlay_followers: overlays,
    })
}
