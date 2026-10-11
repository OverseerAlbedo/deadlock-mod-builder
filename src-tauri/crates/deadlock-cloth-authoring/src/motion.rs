//! Explicit animation support maps on immutable standalone source proxies.
use crate::{
    invalid,
    surface::{self, Editable, MeshData},
    surface_geometry::{cross, norm, sub},
    Result,
};
use deadlock_core::CancellationToken;
use deadlock_dmx::Dmx;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    cmp::Ordering,
    collections::{BTreeSet, BinaryHeap},
};
const MAPS: [&str; 3] = [
    "cloth_goal_strength$0",
    "cloth_drag$0",
    "cloth_antishrink$0",
];
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MotionProfile {
    pub goal_at_anchor: f64,
    pub goal_at_hem: f64,
    pub drag: f64,
    pub antishrink: f64,
}
impl Default for MotionProfile {
    fn default() -> Self {
        Self {
            goal_at_anchor: 0.4,
            goal_at_hem: 0.15,
            drag: 0.15,
            antishrink: 0.98,
        }
    }
}
impl MotionProfile {
    pub fn validate(&self) -> Result<()> {
        for value in [
            self.goal_at_anchor,
            self.goal_at_hem,
            self.drag,
            self.antishrink,
        ] {
            if !value.is_finite() || value <= 0. || value > 1. {
                return Err(invalid(
                    "Cloth motion profile needs finite controls in(0,1]",
                ));
            }
        }
        if self.goal_at_anchor < self.goal_at_hem {
            return Err(invalid(
                "Cloth shape support must not weaken toward its anchors",
            ));
        }
        Ok(())
    }
}
struct Row {
    ordinal: usize,
    name: String,
    mesh: MeshData,
    enabled: Vec<bool>,
    edges: Vec<(usize, usize)>,
}
fn rows(d: &Dmx, cancel: &CancellationToken) -> Result<Vec<Row>> {
    if d.version != 9 || d.format != "model" || d.format_version != 22 {
        return Err(invalid("Cloth motion maps require binary9/model22"));
    }
    let mut rows = vec![];
    let mut states = BTreeSet::new();
    for (ordinal, (index, e)) in d
        .elements
        .iter()
        .enumerate()
        .filter(|(_, e)| e.kind == "DmeMesh")
        .enumerate()
    {
        cancel.check()?;
        let state = surface::reference(d, e, "bindState", "DmeVertexData")?;
        if !states.insert(state)
            || e.get("currentState")
                .is_some_and(|v| v.as_ref_index() != Some(state as i32))
            || e.get("deltaStates").is_some_and(|v| !v.is_empty())
        {
            return Err(invalid(
                "Cloth motion requires independent non-morphing proxy states",
            ));
        }
        let vertex = &d.elements[state];
        let positions = surface::floats(d, vertex, "position$0", 42, 3)?
            .chunks_exact(3)
            .map(|v| v.try_into().unwrap())
            .collect::<Vec<[f64; 3]>>();
        let corners = surface::ints(d, vertex, "position$0Indices", 34)?;
        let enabled = surface::floats(d, vertex, "cloth_enable$0", 35, 1)?;
        let indices = surface::ints(d, vertex, "cloth_enable$0Indices", 34)?;
        let formats = surface::strings(vertex, "vertexFormat")?;
        if formats.len() != formats.iter().collect::<BTreeSet<_>>().len()
            || !formats.iter().any(|v| v == "cloth_enable$0")
            || positions.len() < 3
            || positions.iter().flatten().any(|v| !v.is_finite())
            || corners.is_empty()
            || corners
                .iter()
                .any(|&i| i < 0 || i as usize >= positions.len())
            || enabled.len() != positions.len()
            || enabled.iter().any(|v| *v != 0. && *v != 1.)
            || indices != corners
        {
            return Err(invalid(
                "Cloth motion needs finite binary position-indexed mask",
            ));
        }
        let mesh = MeshData {
            mesh: index,
            state,
            drawn: vec![],
            positions,
            corners: corners.into_iter().map(|v| v as usize).collect(),
            weights: vec![],
            joints: vec![],
        };
        let triangles = surface::triangles(d, &mesh, cancel, None)?;
        let used = triangles.iter().flatten().copied().collect::<BTreeSet<_>>();
        if used.len() != mesh.positions.len()
            || triangles.iter().any(|t| {
                t.iter().collect::<BTreeSet<_>>().len() != 3
                    || norm(cross(
                        sub(mesh.positions[t[1]], mesh.positions[t[0]]),
                        sub(mesh.positions[t[2]], mesh.positions[t[0]]),
                    )) == 0.
            })
        {
            return Err(invalid(
                "Cloth motion needs drawn non-degenerate proxy positions",
            ));
        }
        let edges = triangles
            .iter()
            .flat_map(|t| {
                (0..3).map(|i| {
                    let (a, b) = (t[i], t[(i + 1) % 3]);
                    (a.min(b), a.max(b))
                })
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        rows.push(Row {
            ordinal,
            name: e.name.clone(),
            mesh,
            enabled: enabled.iter().map(|v| *v != 0.).collect(),
            edges,
        });
    }
    if rows.is_empty() {
        return Err(invalid("Cloth motion requires mesh proxy"));
    }
    Ok(rows)
}
#[derive(PartialEq)]
struct Pending {
    distance: f64,
    index: usize,
}
impl Eq for Pending {}
impl Ord for Pending {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .distance
            .total_cmp(&self.distance)
            .then_with(|| other.index.cmp(&self.index))
    }
}
impl PartialOrd for Pending {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
fn anchor_distances(row: &Row, cancel: &CancellationToken) -> Result<(Vec<f64>, usize)> {
    let positions = &row.mesh.positions;
    let mut adjacent = vec![vec![]; positions.len()];
    for &(a, b) in &row.edges {
        let length = norm(sub(positions[b], positions[a]));
        if !length.is_finite() || length <= 0. {
            return Err(invalid("Collapsed cloth proxy edge"));
        }
        adjacent[a].push((b, length));
        adjacent[b].push((a, length));
    }
    let mut labels = vec![usize::MAX; positions.len()];
    let mut component = 0;
    for start in 0..positions.len() {
        if labels[start] != usize::MAX {
            continue;
        }
        let mut todo = vec![start];
        labels[start] = component;
        while let Some(i) = todo.pop() {
            for &(j, _) in &adjacent[i] {
                if labels[j] == usize::MAX {
                    labels[j] = component;
                    todo.push(j);
                }
            }
        }
        component += 1;
    }
    let mut counts = vec![(0, 0); component];
    for (i, &label) in labels.iter().enumerate() {
        if row.enabled[i] {
            counts[label].0 += 1;
        } else {
            counts[label].1 += 1;
        }
    }
    if counts.iter().any(|v| v.0 == 0 || v.1 == 0) {
        return Err(invalid(
            "Every physical cloth panel needs existing anchors and dynamic positions",
        ));
    }
    let mut distance = vec![f64::INFINITY; positions.len()];
    let mut heap = BinaryHeap::new();
    for (i, &active) in row.enabled.iter().enumerate() {
        if !active {
            distance[i] = 0.;
            heap.push(Pending {
                distance: 0.,
                index: i,
            });
        }
    }
    let mut steps = 0;
    while let Some(Pending {
        distance: current,
        index,
    }) = heap.pop()
    {
        steps += 1;
        if steps % 1024 == 0 {
            cancel.check()?;
        }
        if current != distance[index] {
            continue;
        }
        for &(neighbor, length) in &adjacent[index] {
            let proposed = current + length;
            if proposed < distance[neighbor] {
                distance[neighbor] = proposed;
                heap.push(Pending {
                    distance: proposed,
                    index: neighbor,
                });
            }
        }
    }
    let mut maximum = vec![0f64; component];
    for (i, &label) in labels.iter().enumerate() {
        maximum[label] = maximum[label].max(distance[i]);
    }
    if maximum.iter().any(|v| !v.is_finite() || *v <= 0.) {
        return Err(invalid("Cloth anchor distances are invalid"));
    }
    Ok((
        distance
            .iter()
            .enumerate()
            .map(|(i, v)| v / maximum[labels[i]])
            .collect(),
        component,
    ))
}
pub fn author_motion_maps(
    source: &[u8],
    profile: &MotionProfile,
    cancel: &CancellationToken,
) -> Result<(Vec<u8>, Value)> {
    author_motion_maps_by_group(source, profile, &[], cancel)
}

pub(crate) fn author_motion_maps_by_group(
    source: &[u8], profile: &MotionProfile,
    groups: &[(std::ops::Range<usize>, MotionProfile)], cancel: &CancellationToken,
) -> Result<(Vec<u8>, Value)> {
    profile.validate()?;
    for (_, profile) in groups { profile.validate()?; }
    cancel.check()?;
    let original = Dmx::parse(source.to_vec())?;
    let mut output = Editable::new(&original);
    let mut reports = vec![];
    let mut changed = BTreeSet::new();
    for row in rows(&original, cancel)? {
        let state = &original.elements[row.mesh.state];
        if state.attributes.iter().any(|a| {
            MAPS.iter()
                .any(|n| a.name == *n || a.name == format!("{n}Indices"))
        }) {
            return Err(invalid(
                "Existing cloth motion maps require explicit review",
            ));
        }
        changed.insert(row.mesh.state);
        let (normalized, panels) = anchor_distances(&row, cancel)?;
        let mut profiles = vec![*profile; row.enabled.len()];
        let mut assigned = BTreeSet::new();
        for (range, profile) in groups {
            if range.is_empty() || range.end > profiles.len() { return Err(invalid("Invalid secondary group point range")); }
            for i in range.clone() {
                if !assigned.insert(i) { return Err(invalid("Overlapping secondary group point ranges")); }
                profiles[i] = *profile;
            }
        }
        let goal = normalized
            .iter()
            .zip(&row.enabled)
            .zip(&profiles)
            .map(|((&n, &active), profile)| {
                if active {
                    profile.goal_at_hem
                        + (profile.goal_at_anchor - profile.goal_at_hem) * (1. - n).powi(2)
                } else {
                    0.
                }
            })
            .collect::<Vec<_>>();
        let drag = row
            .enabled
            .iter()
            .zip(&profiles)
            .map(|(&v, profile)| if v { profile.drag } else { 0. })
            .collect::<Vec<_>>();
        let contraction = vec![profile.antishrink; row.enabled.len()];
        for (name, values) in MAPS.iter().zip([goal.clone(), drag, contraction]) {
            output.set(row.mesh.state, name, 35, surface::numeric_f32(values))?;
            output.set(
                row.mesh.state,
                &format!("{name}Indices"),
                34,
                surface::numeric_i32(row.mesh.corners.iter().copied())?,
            )?;
        }
        let mut formats = surface::strings(state, "vertexFormat")?;
        formats.extend(MAPS.iter().map(|s| s.to_string()));
        output.set(
            row.mesh.state,
            "vertexFormat",
            37,
            surface::string_array(&formats)?,
        )?;
        let dynamic = row.enabled.iter().filter(|v| **v).count();
        let range = goal
            .iter()
            .zip(&row.enabled)
            .filter(|(_, v)| **v)
            .map(|(&v, _)| v)
            .collect::<Vec<_>>();
        reports.push(json!({"mesh":row.name,"mesh_index":row.ordinal,"positions":row.enabled.len(),"dynamic":dynamic,"pins":row.enabled.len()-dynamic,"physical_panels":panels,"goal_dynamic_range":[range.iter().copied().fold(f64::INFINITY,f64::min),range.iter().copied().fold(f64::NEG_INFINITY,f64::max)],"distance_method":"Shortest rest-surface edge path from existing pins, normalized per connected panel.","seams_added":0,"panel_connections_unchanged":true}));
    }
    let encoded = output.bytes()?;
    let parsed = Dmx::parse(encoded.clone())?;
    if original.elements.len() != parsed.elements.len() {
        return Err(invalid("Cloth motion changed source elements"));
    }
    for (i, (before, after)) in original.elements.iter().zip(&parsed.elements).enumerate() {
        if before.kind != after.kind || before.name != after.name || before.uuid != after.uuid {
            return Err(invalid("Cloth motion changed source identities"));
        }
        for a in &before.attributes {
            let actual = surface::attribute(after, &a.name, a.type_code)?;
            if changed.contains(&i) && a.name == "vertexFormat" {
                let mut wanted = surface::strings(before, "vertexFormat")?;
                wanted.extend(MAPS.iter().map(|v| v.to_string()));
                if surface::strings(after, "vertexFormat")? != wanted {
                    return Err(invalid("Cloth motion changed unrelated formats"));
                }
            } else if original.raw_attribute(a) != parsed.raw_attribute(actual) {
                return Err(invalid(
                    "Cloth motion changed geometry, pins, weights or binds",
                ));
            }
        }
    }
    for row in rows(&parsed, cancel)? {
        for name in MAPS {
            let state = &parsed.elements[row.mesh.state];
            let values = surface::floats(&parsed, state, name, 35, 1)?;
            let indices = surface::ints(&parsed, state, &format!("{name}Indices"), 34)?;
            if values.len() != row.mesh.positions.len()
                || values.iter().any(|v| !v.is_finite())
                || indices.iter().map(|&i| i as usize).collect::<Vec<_>>() != row.mesh.corners
            {
                return Err(invalid("Cloth motion float32/index roundtrip failed"));
            }
        }
    }
    cancel.check()?;
    let receipt = json!({"policy":"animation-supported-cloth-maps-v1","parameters":profile,"source_sha256":crate::hash(source),"proxy_sha256":crate::hash(&encoded),"meshes":reports,"maps":MAPS,"render_source_modified":false,"geometry_skinning_binds_preserved":true,"gravity_modified":false,"seams_inferred":false,"native_map_interpretation_verified":false,"native_compiled":false,"runtime_ready":false,"build_authorized":false});
    Ok((encoded, receipt))
}
