//! Exact source geometry and bind primitives for isolated cloth authoring.
use crate::{invalid, Result};
use deadlock_core::CancellationToken;
use deadlock_dmx::{Attribute, Dmx, Element, Rewrite, Value};
use deadlock_skeleton::{
    dmx_bind::{self, BindAudit},
    Skeleton,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone)]
pub struct MeshData {
    pub mesh: usize,
    pub state: usize,
    pub positions: Vec<[f64; 3]>,
    pub corners: Vec<usize>,
    pub weights: Vec<Vec<f64>>,
    pub joints: Vec<Vec<usize>>,
    pub drawn: Vec<bool>,
}
#[derive(Debug, Clone)]
pub struct BindInfo {
    pub palette: Vec<usize>,
    pub skeleton: Skeleton,
    pub audit: BindAudit,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    pub enabled: Vec<usize>,
    pub pinned: Vec<usize>,
}
pub type Selections = BTreeMap<String, Selection>;

pub fn selection_from_analysis(
    d: &Dmx,
    analysis: &crate::physics::PhysicsAnalysis,
    extension: &deadlock_skeleton::source_identity::SourceContract,
) -> Result<Selections> {
    let sha = crate::hash(d.bytes());
    if analysis.source_sha256 != sha
        || extension.source_sha256 != sha
        || analysis.original_sha256.as_deref() != Some(extension.original_sha256.as_str())
        || !extension.original_names_and_parents_preserved
    {
        return Err(invalid(
            "Cloth source analysis binding is stale; analyze current source again",
        ));
    }
    let d = Dmx::parse(d.bytes().to_vec())?;
    let palette = binds(&d)?.palette;
    let names = palette
        .iter()
        .map(|&i| {
            extension
                .source_aliases
                .get(&d.elements[i].name)
                .unwrap_or(&d.elements[i].name)
                .clone()
        })
        .collect::<Vec<_>>();
    let simulated = analysis
        .chains
        .iter()
        .flat_map(|c| c.simulated_bones.iter().cloned())
        .collect::<BTreeSet<_>>();
    let roots = analysis
        .chains
        .iter()
        .flat_map(|c| c.pinned_bones.iter().cloned())
        .collect::<BTreeSet<_>>();
    let anchors = analysis
        .chains
        .iter()
        .map(|c| c.anchor.clone())
        .collect::<BTreeSet<_>>();
    let required = extension
        .required_extra_names
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    if simulated.is_empty()
        || roots.is_empty()
        || !simulated.is_disjoint(&roots)
        || !simulated.union(&roots).all(|v| required.contains(v))
    {
        return Err(invalid(
            "Cloth selection needs valid custom chains and fixed roots",
        ));
    }
    let dynamic = names
        .iter()
        .enumerate()
        .filter(|(_, n)| simulated.contains(*n))
        .map(|(i, _)| i)
        .collect::<BTreeSet<_>>();
    let root = names
        .iter()
        .enumerate()
        .filter(|(_, n)| roots.contains(*n))
        .map(|(i, _)| i)
        .collect::<BTreeSet<_>>();
    let fixed = names
        .iter()
        .enumerate()
        .filter(|(_, n)| roots.contains(*n) || anchors.contains(*n))
        .map(|(i, _)| i)
        .collect::<BTreeSet<_>>();
    let mut result = BTreeMap::new();
    for (name, row) in mesh_data(&d)? {
        if !extension.selected_meshes.contains(&name) {
            continue;
        }
        let mut enabled = vec![false; row.positions.len()];
        let mut pinned = enabled.clone();
        let mut permitted = enabled.clone();
        let mut mixed_root = false;
        for i in 0..row.positions.len() {
            let influence = row.joints[i].iter().zip(&row.weights[i]);
            enabled[i] = row.drawn[i]
                && influence
                    .clone()
                    .any(|(j, w)| dynamic.contains(j) && *w > 0.);
            let has_root = influence.clone().any(|(j, w)| root.contains(j) && *w > 0.);
            permitted[i] = influence
                .clone()
                .all(|(j, w)| fixed.contains(j) || *w == 0.);
            pinned[i] = has_root && !enabled[i] && permitted[i] && row.drawn[i];
            mixed_root |= has_root && !enabled[i] && !permitted[i] && row.drawn[i];
        }
        if enabled.iter().any(|v| *v) {
            if mixed_root {
                return Err(invalid(format!(
                    "Cloth root region mixes unrelated influences: {name}"
                )));
            }
            for t in triangles(&d, &row, &CancellationToken::new(), Some(&enabled))? {
                if t.iter().any(|&i| enabled[i]) {
                    for i in t {
                        if !enabled[i] {
                            if !permitted[i] {
                                return Err(invalid(format!(
                                    "Dynamic cloth meets unrelated fixed boundary weights: {name}"
                                )));
                            }
                            pinned[i] = true;
                        }
                    }
                }
            }
            result.insert(
                name,
                Selection {
                    enabled: enabled
                        .iter()
                        .enumerate()
                        .filter(|(_, v)| **v)
                        .map(|(i, _)| i)
                        .collect(),
                    pinned: pinned
                        .iter()
                        .enumerate()
                        .filter(|(_, v)| **v)
                        .map(|(i, _)| i)
                        .collect(),
                },
            );
        }
    }
    if result.is_empty() {
        return Err(invalid(
            "No drawn garment vertices use selected simulated bones",
        ));
    }
    Ok(result)
}

#[derive(Debug, Clone)]
pub struct SurfaceOptions {
    pub max_edge_inches: f64,
    pub max_proxy_vertices: usize,
    pub reserved_controls: usize,
}
impl Default for SurfaceOptions {
    fn default() -> Self {
        Self {
            max_edge_inches: 0.02 / 0.0254,
            max_proxy_vertices: 60000,
            reserved_controls: 0,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SurfaceReceipt {
    pub render_path: std::path::PathBuf,
    pub proxy_path: std::path::PathBuf,
    pub render_sha256: String,
    pub proxy_sha256: String,
    #[serde(flatten)]
    pub evidence: BTreeMap<String, serde_json::Value>,
}

fn vectors(values: &[[f64; 3]]) -> Vec<u8> {
    let mut raw = numeric_f32(values.iter().flatten().copied());
    raw[..4].copy_from_slice(&(values.len() as i32).to_le_bytes());
    raw
}
pub(crate) fn write_surface(
    output: &mut Editable,
    row: &MeshData,
    surface: &crate::surface_geometry::Surface,
) -> Result<()> {
    use crate::surface_geometry::{cross, dot, norm, sub};
    let points = surface
        .points
        .iter()
        .map(|p| p.map(|v| v as f32 as f64))
        .collect::<Vec<_>>();
    if points.iter().flatten().any(|v| !v.is_finite()) {
        return Err(invalid("Cloth positions exceed native float32 range"));
    }
    let mut normals = vec![];
    for &[a, b, c] in &surface.faces {
        let original = cross(
            sub(surface.points[b], surface.points[a]),
            sub(surface.points[c], surface.points[a]),
        );
        let native = cross(sub(points[b], points[a]), sub(points[c], points[a]));
        let length = norm(native);
        if !length.is_finite() || length == 0. || dot(original, native) <= 0. {
            return Err(invalid(
                "Native float32 precision collapses or reverses a cloth proxy face",
            ));
        }
        normals.push(native.map(|v| v / length));
    }
    let slots = surface
        .influences
        .iter()
        .map(BTreeMap::len)
        .max()
        .unwrap_or(0);
    if slots == 0 || slots > 64 {
        return Err(invalid(
            "Proxy interpolation exceeds supported influence count; no weights discarded",
        ));
    }
    let (mut weights, mut joints) = (vec![], vec![]);
    for influence in &surface.influences {
        if (influence.values().sum::<f64>() - 1.).abs() > 2e-6 {
            return Err(invalid("Interpolated proxy weights do not sum to one"));
        }
        let mut row_weights = vec![0.; slots];
        let mut row_joints = vec![0; slots];
        for (i, (&j, &w)) in influence.iter().enumerate() {
            row_weights[i] = w;
            row_joints[i] = j;
        }
        weights.extend(row_weights);
        joints.extend(row_joints);
    }
    let corners = surface.faces.iter().flatten().copied().collect::<Vec<_>>();
    let mut faces = ((surface.faces.len() * 4) as i32).to_le_bytes().to_vec();
    for i in 0..surface.faces.len() {
        for n in [i as i32 * 3, i as i32 * 3 + 1, i as i32 * 3 + 2, -1] {
            faces.extend(n.to_le_bytes());
        }
    }
    output.d.elements[row.state].attributes.clear();
    output.edits.attributes.retain(|(i, _), _| *i != row.state);
    let formats = [
        "position$0",
        "normal$0",
        "texcoord$0",
        "blendweights$0",
        "blendindices$0",
        "cloth_enable$0",
    ]
    .map(str::to_owned)
    .to_vec();
    for (name, kind, raw) in [
        ("vertexFormat", 37, string_array(&formats)?),
        ("jointCount", 2, (slots as i32).to_le_bytes().to_vec()),
        ("flipVCoordinates", 4, vec![1]),
        ("position$0", 42, vectors(&points)),
        (
            "position$0Indices",
            34,
            numeric_i32(corners.iter().copied())?,
        ),
        ("normal$0", 42, vectors(&normals)),
        (
            "normal$0Indices",
            34,
            numeric_i32((0..surface.faces.len()).flat_map(|i| [i; 3]))?,
        ),
        (
            "texcoord$0",
            41,
            [1i32.to_le_bytes().as_slice(), &[0; 8]].concat(),
        ),
        (
            "texcoord$0Indices",
            34,
            numeric_i32(std::iter::repeat_n(0, corners.len()))?,
        ),
        ("blendweights$0", 35, numeric_f32(weights)),
        ("blendindices$0", 34, numeric_i32(joints)?),
        (
            "cloth_enable$0",
            35,
            numeric_f32(surface.enabled.iter().map(|v| if *v { 1. } else { 0. })),
        ),
        (
            "cloth_enable$0Indices",
            34,
            numeric_i32(corners.iter().copied())?,
        ),
    ] {
        output.set(row.state, name, kind, raw)?;
    }
    let first = references(&output.d, &output.d.elements[row.mesh], "faceSets")?
        .first()
        .copied()
        .ok_or_else(|| invalid("Cloth mesh has no source face set"))?;
    output.set(row.mesh, "faceSets", 33, numeric_i32([first])?)?;
    output.set(first, "faces", 34, faces)?;
    Ok(())
}
pub(crate) fn prune_proxy(mut edited: Editable, selected: &BTreeSet<usize>) -> Result<Vec<u8>> {
    for i in 0..edited.d.elements.len() {
        let e = &edited.d.elements[i];
        if e.kind == "DmeDag"
            && e.get("shape").is_some_and(|v| {
                v.as_ref_index()
                    .is_none_or(|s| !selected.contains(&(s as usize)))
            })
        {
            edited.set(i, "shape", 1, (-1i32).to_le_bytes().to_vec())?;
        }
    }
    edited.d.elements[0]
        .attributes
        .retain(|a| ["model", "skeleton"].contains(&a.name.as_str()));
    let parsed = Dmx::parse(edited.bytes()?)?;
    let (mut keep, mut pending) = (BTreeSet::new(), vec![0]);
    while let Some(i) = pending.pop() {
        if !keep.insert(i) {
            continue;
        }
        for a in &parsed.elements[i].attributes {
            if a.type_code == 1 || a.type_code == 33 {
                for v in if a.array {
                    parsed.integer_values(&a.value)?
                } else {
                    vec![a
                        .value
                        .as_ref_index()
                        .ok_or_else(|| invalid("Expected cloth element reference"))?]
                } {
                    if v >= 0 {
                        pending.push(v as usize);
                    }
                }
            }
        }
    }
    let order = keep.into_iter().collect::<Vec<_>>();
    let mapping = order
        .iter()
        .enumerate()
        .map(|(new, &old)| (old, new))
        .collect::<BTreeMap<_, _>>();
    let mut reduced = Editable::new(&parsed);
    reduced.d.elements = order.iter().map(|&i| parsed.elements[i].clone()).collect();
    for i in 0..reduced.d.elements.len() {
        let attrs = reduced.d.elements[i].attributes.clone();
        for a in attrs {
            if a.type_code == 1 || a.type_code == 33 {
                let values = if a.array {
                    parsed.integer_values(&a.value)?
                } else {
                    vec![a
                        .value
                        .as_ref_index()
                        .ok_or_else(|| invalid("Expected cloth element reference"))?]
                };
                let mapped = values
                    .into_iter()
                    .map(|old| {
                        mapping
                            .get(&(old as usize))
                            .map(|&i| i as i32)
                            .unwrap_or(-1)
                    })
                    .collect::<Vec<_>>();
                let mut raw = if a.array {
                    (mapped.len() as i32).to_le_bytes().to_vec()
                } else {
                    vec![]
                };
                for v in mapped {
                    raw.extend(v.to_le_bytes());
                }
                reduced.set(i, &a.name, a.type_code, raw)?;
            }
        }
    }
    reduced.bytes()
}
pub fn author_direct_cloth(
    d: &Dmx,
    selection: &Selections,
    destination: &std::path::Path,
    source_sha: &str,
    reserved_controls: usize,
    cancel: &CancellationToken,
    motion: Option<&crate::motion::MotionProfile>,
) -> Result<SurfaceReceipt> {
    author_direct_cloth_with_options(
        d,
        selection,
        destination,
        source_sha,
        &SurfaceOptions {
            reserved_controls,
            ..Default::default()
        },
        cancel,
        motion,
    )
}
pub fn author_direct_cloth_with_options(
    d: &Dmx,
    selection: &Selections,
    destination: &std::path::Path,
    source_sha: &str,
    options: &SurfaceOptions,
    cancel: &CancellationToken,
    motion: Option<&crate::motion::MotionProfile>,
) -> Result<SurfaceReceipt> {
    use deadlock_core::safety;
    use serde_json::json;
    cancel.check()?;
    if crate::hash(d.bytes()) != source_sha {
        return Err(invalid("Cloth source SHA256 changed; re-analyze source"));
    }
    let d = Dmx::parse(d.bytes().to_vec())?;
    if selection.is_empty()
        || options.max_proxy_vertices == 0
        || !options.max_edge_inches.is_finite()
        || options.max_edge_inches <= 0.
    {
        return Err(invalid(
            "Invalid cloth selection, spacing or control budget",
        ));
    }
    if destination.exists()
        && (!destination.is_dir() || std::fs::read_dir(destination)?.next().is_some())
    {
        return Err(invalid("Cloth authoring destination must be new or empty"));
    }
    let bind = binds(&d)?;
    let mut budget = 65535usize
        .checked_sub(bind.skeleton.names.len())
        .and_then(|v| v.checked_sub(options.reserved_controls))
        .ok_or_else(|| {
            invalid("No cloth controls remain after reserving original skeleton/solver")
        })?
        .min(options.max_proxy_vertices);
    if budget == 0 {
        return Err(invalid("No cloth controls remain"));
    }
    let rows = mesh_data(&d)?;
    if selection.keys().any(|n| !rows.contains_key(n)) {
        return Err(invalid("Selected cloth mesh is missing from source"));
    }
    for row in rows.values() {
        for (joints, weights) in row.joints.iter().zip(&row.weights) {
            for (&j, &w) in joints.iter().zip(weights) {
                if j >= bind.palette.len()
                    || w > 0. && d.elements[bind.palette[j]].kind != "DmeJoint"
                {
                    return Err(invalid(
                        "Cloth skin indices reference missing or non-bone palette entry",
                    ));
                }
            }
        }
    }
    let mut surfaces = BTreeMap::new();
    for (name, selected) in selection {
        cancel.check()?;
        let surface = crate::surface_geometry::surface(
            &d,
            &rows[name],
            selected,
            options.max_edge_inches,
            budget,
            cancel,
        )?;
        budget -= surface.points.len();
        surfaces.insert(name.clone(), surface);
    }
    let count = surfaces.values().map(|s| s.points.len()).sum::<usize>();
    let dynamic = surfaces
        .values()
        .map(|s| s.enabled.iter().filter(|v| **v).count())
        .sum::<usize>();
    if dynamic > 32768 {
        return Err(invalid(
            "Native single collision tree supports at most32768 dynamic positions",
        ));
    }
    let allocation = count + bind.skeleton.names.len() + options.reserved_controls;
    if allocation > 65535 {
        return Err(invalid(
            "Cloth control allocation exceeds native uint16 count",
        ));
    }
    let tree = if dynamic > 0 { dynamic * 2 - 1 } else { 0 };
    let capacity = json!({"policy":"native-cloth-capacity-v1","evidence":"docs/evidence/skirt-physics/real-garment/native-capacity.json","proxy_positions":count,"dynamic_positions":dynamic,"source_bones":bind.skeleton.names.len(),"reserved_controls":options.reserved_controls,"pinned_positions":count-dynamic,"reserved_model_solver_controls":allocation,"max_reserved_model_solver_controls":65535,"max_dynamic_positions":32768,"collision_tree_entries":tree,"collision_tree_root_index":tree.checked_sub(1),"collision_tree_index_type":"uint16","collision_tree_absent_parent":65535,"tree_scope":"New proxy dynamic positions only; original solver is not composed.","reserved_controls_in_tree":false,"preflight_bounds_passed":true,"native_compiled":false,"native_output_capacity_verified":false,"original_solver_preserved":false,"runtime_ready":false});
    let (mut marked, mut proxy) = (Editable::new(&d), Editable::new(&d));
    for (name, row) in &rows {
        let mut mask = vec![0.; row.positions.len()];
        if let Some(selected) = selection.get(name) {
            for &i in &selected.enabled {
                mask[i] = 1.;
            }
        }
        let mut formats = strings(&d.elements[row.state], "vertexFormat")?;
        formats.push("cloth_enable$0".into());
        marked.set(row.state, "vertexFormat", 37, string_array(&formats)?)?;
        marked.set(row.state, "cloth_enable$0", 35, numeric_f32(mask))?;
        marked.set(
            row.state,
            "cloth_enable$0Indices",
            34,
            numeric_i32(row.corners.iter().copied())?,
        )?;
        if let Some(surface) = surfaces.get(name) {
            write_surface(&mut proxy, row, surface)?;
        }
    }
    let render = marked.bytes()?;
    let proxy = prune_proxy(proxy, &surfaces.keys().map(|n| rows[n].mesh).collect())?;
    let (proxy, motion_receipt) = if let Some(profile) = motion {
        let (bytes, report) = crate::motion::author_motion_maps(&proxy, profile, cancel)?;
        (bytes, Some(report))
    } else {
        (proxy, None)
    };
    let render_check = Dmx::parse(render.clone())?;
    let proxy_check = Dmx::parse(proxy.clone())?;
    dmx_bind::audit_dmx_bind(&render_check, &bind.skeleton, false)?;
    dmx_bind::audit_dmx_bind(&proxy_check, &bind.skeleton, false)?;
    if proxy_check
        .summary()?
        .meshes
        .iter()
        .map(|m| m.name.clone())
        .collect::<BTreeSet<_>>()
        != selection.keys().cloned().collect()
    {
        return Err(invalid(
            "Proxy includes unexpected or missing garment meshes",
        ));
    }
    if d.elements.len() != render_check.elements.len() {
        return Err(invalid("Cloth staging changed render element count"));
    }
    for (before, after) in d.elements.iter().zip(&render_check.elements) {
        if before.kind != after.kind || before.name != after.name || before.uuid != after.uuid {
            return Err(invalid("Cloth staging changed render identities"));
        }
        for a in &before.attributes {
            if a.name != "vertexFormat" || before.kind != "DmeVertexData" {
                let actual = attribute(after, &a.name, a.type_code)?;
                if d.raw_attribute(a) != render_check.raw_attribute(actual) {
                    return Err(invalid(
                        "Cloth staging changed source geometry, weights, materials or binds",
                    ));
                }
            }
        }
    }
    let meshes=surfaces.iter().map(|(name,s)|(name.clone(),json!({"enabled_render_positions":selection[name].enabled.len(),"pinned_render_positions":selection[name].pinned.len(),"proxy_positions":s.points.len(),"proxy_triangles":s.faces.len(),"surface_components":s.components,"initial_proxy_positions":s.initial_proxy_positions,"fixed_positions_not_on_dynamic_faces":s.fixed_positions_not_on_dynamic_faces,"zero_area_triangles_excluded":s.degenerate_triangles_excluded,"overlay_followers":s.overlay_followers}))).collect::<BTreeMap<_,_>>();
    let mut value = json!({"policy":"direct-cloth-surface-authoring-v1","source_sha256":source_sha,"source_modified":false,"render_geometry_weights_preserved":true,"serialized_binds_and_scales_preserved":true,"bind_audit":bind.audit,"max_edge_inches":options.max_edge_inches,"source_bones_reserved":bind.skeleton.names.len(),"additional_controls_reserved":options.reserved_controls,"native_capacity":capacity,"proxy_positions":count,"meshes":meshes,"simplified":false,"runtime_ready":false,"native_compiled":false,"original_solver_preserved":false,"published":false,"installed":false,"pending":["Native direct-binding, overlay-follower and excluded-region audit.","Proxy and rendered-surface collision and runtime performance.","Preserve original solver while adding new cloth.","Native VPK and in-game validation."]});
    if let Some(m) = motion_receipt {
        value["motion_maps"] = serde_json::to_value(m)?;
    }
    let receipt = SurfaceReceipt {
        render_path: destination.join("marked-render.dmx"),
        proxy_path: destination.join("surface-proxy.dmx"),
        render_sha256: crate::hash(&render),
        proxy_sha256: crate::hash(&proxy),
        evidence: serde_json::from_value(value)?,
    };
    cancel.check()?;
    safety::atomic_write(&receipt.render_path, &render)?;
    safety::atomic_write(&receipt.proxy_path, &proxy)?;
    safety::write_json(&destination.join("surface-authoring.json"), &receipt)?;
    Ok(receipt)
}

pub(crate) fn attribute<'a>(e: &'a Element, name: &str, kind: u8) -> Result<&'a Attribute> {
    let found = e
        .attributes
        .iter()
        .filter(|a| a.name == name)
        .collect::<Vec<_>>();
    if found.len() != 1 || found[0].type_code != kind {
        return Err(invalid(format!(
            "Cloth requires one correctly typed stream: {name}"
        )));
    }
    Ok(found[0])
}
pub(crate) fn floats(d: &Dmx, e: &Element, name: &str, kind: u8, width: usize) -> Result<Vec<f64>> {
    let a = attribute(e, name, kind)?;
    let raw = d.raw_attribute(a);
    let prefix = if a.array { 4 } else { 0 };
    if raw.len() < prefix || (raw.len() - prefix) % (4 * width) != 0 {
        return Err(invalid("Cloth stream has invalid numeric dimensions"));
    }
    Ok(raw[prefix..]
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes(b.try_into().unwrap()) as f64)
        .collect())
}
pub(crate) fn ints(d: &Dmx, e: &Element, name: &str, kind: u8) -> Result<Vec<i32>> {
    let a = attribute(e, name, kind)?;
    Ok(d.integer_values(&a.value)?)
}
pub(crate) fn strings(e: &Element, name: &str) -> Result<Vec<String>> {
    let a = attribute(e, name, 37)?;
    let Value::Array(values) = &a.value else {
        return Err(invalid("Cloth stream needs a string array"));
    };
    values
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .ok_or_else(|| invalid("Cloth stream needs strings"))
        })
        .collect()
}
pub(crate) fn reference(d: &Dmx, e: &Element, name: &str, kind: &str) -> Result<usize> {
    let i = e
        .get(name)
        .and_then(Value::as_ref_index)
        .and_then(|v| usize::try_from(v).ok())
        .filter(|&v| d.elements.get(v).is_some_and(|e| e.kind == kind))
        .ok_or_else(|| invalid(format!("Invalid cloth {name} element reference")))?;
    Ok(i)
}
pub(crate) fn references(d: &Dmx, e: &Element, name: &str) -> Result<Vec<usize>> {
    if e.get(name).is_none() {
        return Ok(vec![]);
    }
    ints(d, e, name, 33)?
        .into_iter()
        .map(|i| {
            usize::try_from(i)
                .ok()
                .filter(|&i| i < d.elements.len())
                .ok_or_else(|| invalid("Invalid cloth element reference"))
        })
        .collect()
}
fn scalar(e: &Element, name: &str) -> Option<i32> {
    match e.get(name) {
        Some(Value::Integer(n)) => Some(*n),
        _ => None,
    }
}

pub fn mesh_data(d: &Dmx) -> Result<BTreeMap<String, MeshData>> {
    d.summary()?;
    let mut states = BTreeSet::new();
    let mut rows = BTreeMap::new();
    for (mesh, e) in d
        .elements
        .iter()
        .enumerate()
        .filter(|(_, e)| e.kind == "DmeMesh")
    {
        let state = reference(d, e, "bindState", "DmeVertexData")?;
        if !states.insert(state)
            || e.get("currentState")
                .is_some_and(|v| v.as_ref_index() != Some(state as i32))
            || e.get("deltaStates").is_some_and(|v| !v.is_empty())
        {
            return Err(invalid(
                "Cloth requires independent non-morphing meshes with matching current/bind states",
            ));
        }
        let vertex = &d.elements[state];
        if strings(vertex, "vertexFormat")?
            .iter()
            .any(|v| v.starts_with("cloth_"))
        {
            return Err(invalid("Existing cloth streams require separate review; automatic authoring will not overwrite them"));
        }
        let values = floats(d, vertex, "position$0", 42, 3)?;
        let positions = values
            .chunks_exact(3)
            .map(|v| v.try_into().unwrap())
            .collect::<Vec<[f64; 3]>>();
        let slots = scalar(vertex, "jointCount")
            .filter(|n| (1..=64).contains(n))
            .ok_or_else(|| invalid("Cloth requires position-aligned skin weights"))?
            as usize;
        let weight = floats(d, vertex, "blendweights$0", 35, 1)?;
        let joints = ints(d, vertex, "blendindices$0", 34)?;
        let corner = ints(d, vertex, "position$0Indices", 34)?;
        if positions.is_empty()
            || weight.len() != positions.len() * slots
            || joints.len() != weight.len()
            || corner.is_empty()
            || corner
                .iter()
                .any(|&i| i < 0 || i as usize >= positions.len())
            || weight.iter().any(|v| !v.is_finite() || *v < 0.)
        {
            return Err(invalid(
                "Cloth source has invalid weights or corner indices",
            ));
        }
        // numpy sums float32 skin streams in float32 before its absolute2e-6 gate.
        if weight
            .chunks_exact(slots)
            .any(|w| (w.iter().map(|&v| v as f32).sum::<f32>() as f64 - 1.).abs() > 2e-6)
        {
            return Err(invalid("Cloth source weights do not sum to one"));
        }
        if joints.iter().any(|i| *i < 0) {
            return Err(invalid("Negative cloth skin palette index"));
        }
        let corners = corner.into_iter().map(|v| v as usize).collect::<Vec<_>>();
        let mut drawn = vec![false; positions.len()];
        for face in references(d, e, "faceSets")? {
            let face = &d.elements[face];
            if face.kind != "DmeFaceSet" {
                return Err(invalid("Invalid cloth face-set element"));
            }
            for c in ints(d, face, "faces", 34)? {
                if c == -1 {
                    continue;
                }
                let c = usize::try_from(c)
                    .ok()
                    .filter(|&i| i < corners.len())
                    .ok_or_else(|| invalid("Invalid drawn cloth face index"))?;
                drawn[corners[c]] = true;
            }
        }
        rows.insert(
            e.name.clone(),
            MeshData {
                mesh,
                state,
                positions,
                corners,
                weights: weight.chunks_exact(slots).map(|v| v.to_vec()).collect(),
                joints: joints
                    .chunks_exact(slots)
                    .map(|v| v.iter().map(|&i| i as usize).collect())
                    .collect(),
                drawn,
            },
        );
    }
    Ok(rows)
}

pub fn binds(d: &Dmx) -> Result<BindInfo> {
    let models = d
        .elements
        .iter()
        .filter(|e| e.kind == "DmeModel" && e.get("jointList").is_some_and(|v| !v.is_empty()))
        .collect::<Vec<_>>();
    if d.version != 9 || d.format != "model" || d.format_version != 22 || models.len() != 1 {
        return Err(invalid(
            "Cloth requires binary9/model22 and one canonical joint palette",
        ));
    }
    let model = models[0];
    if model.get("axisSystem").is_some() {
        let axis = &d.elements[reference(d, model, "axisSystem", "DmeAxisSystem")?];
        if [("upAxis", 3), ("forwardParity", 1), ("coordSys", 0)]
            .iter()
            .any(|(k, v)| scalar(axis, k) != Some(*v))
        {
            return Err(invalid(
                "Cloth source requires canonical Z-up model coordinates",
            ));
        }
    }
    let palette = references(d, model, "jointList")?;
    let joints = palette
        .iter()
        .copied()
        .filter(|&i| d.elements[i].kind == "DmeJoint")
        .collect::<Vec<_>>();
    let positions = joints
        .iter()
        .enumerate()
        .map(|(i, &j)| (j, i))
        .collect::<BTreeMap<_, _>>();
    let mut parents = BTreeMap::new();
    for &joint in &joints {
        for child in references(d, &d.elements[joint], "children")? {
            if positions.contains_key(&child) && parents.insert(child, joint).is_some() {
                return Err(invalid("Cloth source has multiply parented bones"));
            }
        }
    }
    let mut skeleton = Skeleton {
        names: vec![],
        parents: vec![],
        positions: vec![],
        rotations: vec![],
        scales: vec![],
        flags: vec![],
    };
    for &joint in &joints {
        let e = &d.elements[joint];
        let transform = &d.elements[reference(d, e, "transform", "DmeTransform")?];
        skeleton.names.push(e.name.clone());
        skeleton.parents.push(
            parents
                .get(&joint)
                .map(|p| positions[p] as i32)
                .unwrap_or(-1),
        );
        skeleton.positions.push(
            floats(d, transform, "position", 10, 3)?
                .try_into()
                .map_err(|_| invalid("Invalid cloth bind position"))?,
        );
        skeleton.rotations.push(
            floats(d, transform, "orientation", 13, 4)?
                .try_into()
                .map_err(|_| invalid("Invalid cloth bind rotation"))?,
        );
        skeleton.scales.push(match transform.get("scale") {
            None => 1.,
            Some(Value::Float(v)) => *v as f64,
            _ => return Err(invalid("Invalid cloth local scale")),
        });
    }
    let audit = dmx_bind::audit_dmx_bind(d, &skeleton, false)?;
    Ok(BindInfo {
        palette,
        skeleton,
        audit,
    })
}

pub fn triangles(
    d: &Dmx,
    row: &MeshData,
    cancel: &CancellationToken,
    active: Option<&[bool]>,
) -> Result<Vec<[usize; 3]>> {
    if active.is_some_and(|v| v.len() != row.positions.len()) {
        return Err(invalid("Cloth active position mask has wrong length"));
    }
    let positions = row
        .positions
        .iter()
        .map(|v| v.map(|v| v as f32))
        .collect::<Vec<_>>();
    let mut output = vec![];
    for index in references(d, &d.elements[row.mesh], "faceSets")? {
        cancel.check()?;
        let e = &d.elements[index];
        if e.kind != "DmeFaceSet" {
            return Err(invalid("Invalid cloth face set"));
        }
        let values = ints(d, e, "faces", 34)?;
        if values.is_empty()
            || values.last() != Some(&-1)
            || values
                .iter()
                .any(|&v| v < -1 || v >= row.corners.len() as i32)
        {
            return Err(invalid("Cloth face indices are invalid or unterminated"));
        }
        let mut face = vec![];
        for v in values {
            if v >= 0 {
                face.push(v as u32);
                continue;
            }
            if face.len() < 3 {
                return Err(invalid("Cloth polygon has fewer than three corners"));
            }
            if active.is_none_or(|m| face.iter().any(|&c| m[row.corners[c as usize]])) {
                for t in
                    deadlock_preview::triangulate_polygon(&face, &positions, &row.corners, cancel)?
                {
                    output.push(t.map(|c| row.corners[c as usize]));
                }
            }
            face.clear();
        }
    }
    if output.is_empty() {
        return Err(invalid("Cloth selection has no surface faces"));
    }
    Ok(output)
}

// Ordered raw edits preserve all untouched attribute payloads and pool indices.
pub(crate) struct Editable {
    pub d: Dmx,
    pub edits: Rewrite,
}
impl Editable {
    pub fn new(d: &Dmx) -> Self {
        Self {
            d: d.clone(),
            edits: Rewrite::default(),
        }
    }
    pub fn set(&mut self, index: usize, name: &str, kind: u8, raw: Vec<u8>) -> Result<()> {
        let e = self
            .d
            .elements
            .get_mut(index)
            .ok_or_else(|| invalid("Unknown cloth element"))?;
        if let Some(a) = e.attributes.iter().find(|a| a.name == name) {
            if a.type_code != kind {
                return Err(invalid(format!(
                    "Cloth attribute has unexpected type: {name}"
                )));
            }
        } else {
            e.attributes.push(Attribute {
                name: name.into(),
                type_code: kind,
                array: kind >= 32,
                value: Value::Integer(0),
                raw: 0..0,
            });
        }
        if !self.d.strings.iter().any(|s| s == name) {
            self.d.strings.push(name.into());
        }
        self.edits.attributes.insert((index, name.into()), raw);
        Ok(())
    }
    pub fn bytes(&self) -> Result<Vec<u8>> {
        Ok(self.d.rewrite_binary9(&self.edits)?)
    }
}
pub(crate) fn numeric_f32(values: impl IntoIterator<Item = f64>) -> Vec<u8> {
    let data = values.into_iter().map(|v| v as f32).collect::<Vec<_>>();
    let mut raw = (data.len() as i32).to_le_bytes().to_vec();
    for v in data {
        raw.extend(v.to_le_bytes());
    }
    raw
}
pub(crate) fn numeric_i32(values: impl IntoIterator<Item = usize>) -> Result<Vec<u8>> {
    let data = values
        .into_iter()
        .map(|v| i32::try_from(v).map_err(|_| invalid("Native cloth index overflow")))
        .collect::<Result<Vec<_>>>()?;
    let count = i32::try_from(data.len()).map_err(|_| invalid("Native cloth array overflow"))?;
    let mut raw = count.to_le_bytes().to_vec();
    for v in data {
        raw.extend(v.to_le_bytes());
    }
    Ok(raw)
}
pub(crate) fn string_array(values: &[String]) -> Result<Vec<u8>> {
    let count = i32::try_from(values.len()).map_err(|_| invalid("Cloth string array overflow"))?;
    let mut raw = count.to_le_bytes().to_vec();
    for s in values {
        if s.contains('\0') {
            return Err(invalid("Embedded NUL in cloth stream name"));
        }
        raw.extend(s.as_bytes());
        raw.push(0);
    }
    Ok(raw)
}
