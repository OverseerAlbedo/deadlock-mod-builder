//! Conservative whole-polygon partitioning. No triangulation, welding, or float edits.
//! The validator compares exact protected bytes and source corner provenance.
use crate::{Attribute, Dmx, DmxError, Element, Result, Rewrite, Value};
use deadlock_core::{safety, CancellationToken};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};
use uuid::Uuid;

pub const VERSION: u32 = 3;
pub const PIECE_IDENTITY_VERSION: u32 = 2;
pub const HARD_LIMIT: usize = 524_288;
pub const DEFAULT_TARGET: usize = 400_000;
pub const MAGIC: &[u8; 8] = b"DMBSPL1\0";
fn error(message: impl Into<String>) -> DmxError {
    DmxError::at(0, message)
}
fn map_error(e: impl std::fmt::Display) -> DmxError {
    error(e.to_string())
}
fn check(cancel: &CancellationToken) -> Result<()> {
    cancel.check().map_err(DmxError::from_core)
}
pub fn validate_target(target: usize) -> Result<()> {
    if !(3..HARD_LIMIT).contains(&target) {
        Err(error(format!(
            "Split target must be 3 through {}; Valve hard limit is fixed at {HARD_LIMIT}",
            HARD_LIMIT - 1
        )))
    } else {
        Ok(())
    }
}
fn hash(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}
fn ints(values: impl IntoIterator<Item = i32>) -> Vec<u8> {
    let items: Vec<_> = values.into_iter().collect();
    let mut out = (items.len() as i32).to_le_bytes().to_vec();
    for v in items {
        out.extend(v.to_le_bytes());
    }
    out
}
fn elem(dmx: &Dmx, index: i32) -> Result<&Element> {
    if index < 0 {
        return Err(error("Invalid null element reference"));
    }
    dmx.elements
        .get(index as usize)
        .ok_or_else(|| error("Out-of-range element reference"))
}
fn reference(e: &Element, name: &str) -> Result<i32> {
    e.get(name)
        .and_then(Value::as_ref_index)
        .ok_or_else(|| error(format!("Missing or mistyped element reference {name}")))
}
fn list(dmx: &Dmx, e: &Element, name: &str) -> Result<Vec<i32>> {
    match e.get(name) {
        None => Ok(vec![]),
        Some(v) => dmx.integer_values(v),
    }
}
fn scalar_int(e: &Element, name: &str) -> Option<i32> {
    match e.get(name) {
        Some(Value::Integer(v)) => Some(*v),
        _ => None,
    }
}
fn attributes(e: &Element) -> Result<BTreeMap<&str, &Attribute>> {
    let out: BTreeMap<_, _> = e.attributes.iter().map(|a| (a.name.as_str(), a)).collect();
    if out.len() != e.attributes.len() {
        return Err(error("Duplicate DMX attribute names are unsupported"));
    }
    Ok(out)
}
fn names(e: &Element) -> BTreeSet<&str> {
    e.attributes.iter().map(|a| a.name.as_str()).collect()
}
fn allowed(e: &Element, fields: &[&str]) -> bool {
    e.attributes
        .iter()
        .all(|a| fields.contains(&a.name.as_str()))
}
fn finite(dmx: &Dmx, a: &Attribute, base: u8) -> Result<()> {
    if [3, 9, 10, 11, 12, 13, 14].contains(&base) {
        let raw = if a.array {
            dmx.value_bytes(&a.value)
                .ok_or_else(|| error("Missing packed numeric stream"))?
        } else {
            dmx.raw_attribute(a)
        };
        if raw
            .chunks_exact(4)
            .any(|b| !f32::from_le_bytes(b.try_into().unwrap()).is_finite())
        {
            return Err(error(format!(
                "Non-finite/NaN vertex attribute: {}",
                a.name
            )));
        }
    }
    Ok(())
}
fn truth(value: &Value) -> bool {
    match value {
        Value::Bool(x) => *x,
        Value::ElementRef(x) | Value::Integer(x) => *x != 0,
        Value::Float(x) => *x != 0.,
        Value::String(x) => !x.is_empty(),
        Value::Packed { count, .. } => *count > 0,
        Value::Array(x) => !x.is_empty(),
        Value::Blob(r) | Value::Opaque(r) => !r.is_empty(),
    }
}

pub struct Layout<'a> {
    pub dmx: &'a Dmx,
    pub index: usize,
    pub state_index: usize,
    pub dag_index: usize,
    pub transform_ids: BTreeSet<usize>,
    pub formats: Vec<String>,
    pub streams: Vec<String>,
    pub slots: usize,
    pub positions: usize,
    pub corners: usize,
    pub face_ids: Vec<usize>,
    attrs: BTreeMap<&'a str, &'a Attribute>,
    faces: BTreeMap<usize, Vec<i32>>,
    cancel: CancellationToken,
}
impl<'a> Layout<'a> {
    pub fn new(dmx: &'a Dmx, mesh_name: &str, cancel: &CancellationToken) -> Result<Self> {
        check(cancel)?;
        if (dmx.version, dmx.format.as_str(), dmx.format_version) != (9, "model", 22) {
            return Err(error(
                "Automatic splitting supports Binary 9 / Model 22 indexed bind-state DMX only",
            ));
        }
        let graph = [
            "DmElement",
            "DmeModel",
            "DmeAxisSystem",
            "DmeJoint",
            "DmeTransform",
            "DmeDag",
            "DmeMesh",
            "DmeVertexData",
            "DmeFaceSet",
            "DmeMaterial",
            "DmeTransformList",
        ];
        for e in &dmx.elements {
            let kind = e.kind.to_lowercase();
            if ["delta", "morph", "flex", "combination", "vertexanimation"]
                .iter()
                .any(|s| kind.contains(s))
            {
                return Err(error(format!(
                    "Morph/flex partitioning is unsupported: {}",
                    e.kind
                )));
            }
            if !graph.contains(&e.kind.as_str()) {
                return Err(error(format!(
                    "Cannot prove preservation of DMX graph class {}",
                    e.kind
                )));
            }
            attributes(e)?;
        }
        let found: Vec<_> = dmx
            .elements
            .iter()
            .enumerate()
            .filter(|(_, e)| e.kind == "DmeMesh" && e.name == mesh_name)
            .collect();
        if found.len() != 1 {
            return Err(error("Split source mesh is missing or ambiguous"));
        }
        let (index, mesh) = found[0];
        for a in &mesh.attributes {
            let name = a.name.to_lowercase();
            if ["delta", "morph", "flex", "wrinkle"]
                .iter()
                .any(|s| name.contains(s))
                && truth(&a.value)
            {
                return Err(error("Morph/flex partitioning is unsupported"));
            }
        }
        if !allowed(
            mesh,
            &[
                "bindState",
                "currentState",
                "baseStates",
                "faceSets",
                "visible",
                "deltaStates",
                "deltaStateWeights",
                "deltaStateWeightsLagged",
            ],
        ) {
            return Err(error("Unknown mesh metadata layout"));
        }
        let state_index = reference(mesh, "bindState")?;
        let state = elem(dmx, state_index)?;
        let mut states: BTreeSet<_> = list(dmx, mesh, "baseStates")?.into_iter().collect();
        states.insert(
            mesh.get("currentState")
                .and_then(Value::as_ref_index)
                .unwrap_or(state_index),
        );
        states.insert(state_index);
        if states != BTreeSet::from([state_index]) || state.kind != "DmeVertexData" {
            return Err(error(
                "Multiple/current non-bind states or unsupported bind-state type",
            ));
        }
        let attrs = attributes(state)?;
        let formats = match state.get("vertexFormat") {
            Some(Value::Array(rows)) => rows
                .iter()
                .map(|v| {
                    v.as_str()
                        .map(str::to_string)
                        .ok_or_else(|| error("Invalid vertexFormat string"))
                })
                .collect::<Result<Vec<_>>>()?,
            _ => return Err(error("Missing/ambiguous vertexFormat")),
        };
        let unique: BTreeSet<_> = formats.iter().map(String::as_str).collect();
        if formats.is_empty()
            || unique.len() != formats.len()
            || !["position$0", "blendweights$0", "blendindices$0"]
                .iter()
                .all(|s| unique.contains(s))
        {
            return Err(error(
                "Split mesh requires indexed position and position-bound skin streams",
            ));
        }
        let slots = scalar_int(state, "jointCount")
            .filter(|n| (1..=64).contains(n))
            .ok_or_else(|| error("Invalid skin influence count"))? as usize;
        let streams: Vec<_> = formats
            .iter()
            .filter(|n| n.as_str() != "blendweights$0" && n.as_str() != "blendindices$0")
            .cloned()
            .collect();
        let known: BTreeSet<String> = ["vertexFormat", "jointCount", "flipVCoordinates"]
            .into_iter()
            .map(str::to_string)
            .chain(formats.clone())
            .chain(streams.iter().map(|s| format!("{s}Indices")))
            .collect();
        if attrs.keys().any(|k| !known.contains(*k)) {
            return Err(error("Unclassified vertex metadata/stream"));
        }
        for name in &formats {
            check(cancel)?;
            let a = attrs
                .get(name.as_str())
                .ok_or_else(|| error(format!("Unsupported vertex stream {name}")))?;
            match a.value {
                Value::Packed { base, .. }
                    if [2, 3, 4, 8, 9, 10, 11, 12, 13, 14, 15, 16].contains(&base) =>
                {
                    finite(dmx, a, base)?
                }
                _ => return Err(error(format!("Unsupported vertex stream type {name}"))),
            }
        }
        if attrs["position$0"].type_code != 42
            || attrs["blendweights$0"].type_code != 35
            || attrs["blendindices$0"].type_code != 34
        {
            return Err(error("Unsupported position/skin stream types"));
        }
        let mut corners = None;
        for name in &streams {
            let a = attrs
                .get(format!("{name}Indices").as_str())
                .ok_or_else(|| error("Unindexed vertex stream"))?;
            if a.type_code != 34 {
                return Err(error("Unknown indexed stream layout"));
            }
            let indices = dmx.integer_values(&a.value)?;
            if corners.is_none() {
                corners = Some(indices.len());
            }
            if corners != Some(indices.len())
                || indices
                    .iter()
                    .any(|&i| i < 0 || i as usize >= attrs[name.as_str()].value.len())
            {
                return Err(error("Invalid/out-of-range vertex indices"));
            }
        }
        let positions = attrs["position$0"].value.len();
        if ["blendweights$0", "blendindices$0"]
            .iter()
            .any(|n| attrs[n].value.len() != positions * slots)
        {
            return Err(error(
                "Skin array lengths do not match positions × influence count",
            ));
        }
        let owners: Vec<_> = dmx
            .elements
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                e.kind == "DmeDag"
                    && e.get("shape").and_then(Value::as_ref_index) == Some(index as i32)
            })
            .collect();
        if owners.len() != 1 {
            return Err(error(
                "Split mesh requires one unambiguous owning DAG/transform",
            ));
        }
        let (dag_index, dag) = owners[0];
        if !allowed(dag, &["shape", "transform", "visible"]) {
            return Err(error("Unsupported mesh DAG metadata"));
        }
        let transform = reference(dag, "transform")?;
        if elem(dmx, transform)?.kind != "DmeTransform" {
            return Err(error("Missing mesh transform"));
        }
        let models: Vec<_> = dmx
            .elements
            .iter()
            .filter(|e| e.kind == "DmeModel")
            .collect();
        if models.len() != 1 {
            return Err(error(
                "Split mesh requires one explicit skeleton joint palette",
            ));
        }
        let model = models[0];
        let palette = list(dmx, model, "jointList")?;
        if palette.is_empty() || palette.iter().filter(|&&i| i == dag_index as i32).count() != 1 {
            return Err(error("Mesh DAG is not uniquely bound to joint palette"));
        }
        let palette_index = palette.iter().position(|&i| i == dag_index as i32).unwrap();
        let mut transform_ids = BTreeSet::from([transform as usize]);
        let base_states = list(dmx, model, "baseStates")?;
        if base_states.is_empty() {
            return Err(error("Missing model bind transforms"));
        }
        for at in base_states {
            let bind = elem(dmx, at)?;
            if bind.kind != "DmeTransformList" {
                return Err(error("Unsupported model bind-transform layout"));
            }
            let refs = list(dmx, bind, "transforms")?;
            if refs.len() != palette.len()
                || refs[palette_index] < 0
                || refs.iter().filter(|&&i| i == refs[palette_index]).count() != 1
            {
                return Err(error("Bind transforms do not uniquely match joint palette"));
            }
            transform_ids.insert(refs[palette_index] as usize);
        }
        for &at in &transform_ids {
            let transform = dmx
                .elements
                .get(at)
                .ok_or_else(|| error("Invalid bind transform reference"))?;
            if transform.kind != "DmeTransform" || transform.name != dag.name {
                return Err(error("Mesh transform name binding is ambiguous"));
            }
            for a in &transform.attributes {
                finite(dmx, a, a.type_code)?;
            }
        }
        for (at, e) in dmx.elements.iter().enumerate() {
            if at != dag_index
                && e.get("transform")
                    .and_then(Value::as_ref_index)
                    .is_some_and(|i| i >= 0 && transform_ids.contains(&(i as usize)))
            {
                return Err(error("Mesh transform has another owner"));
            }
        }
        let mut parent_count = 0;
        for e in &dmx.elements {
            if list(dmx, e, "children")?.contains(&(dag_index as i32)) {
                parent_count += 1;
            }
        }
        if parent_count != 1 {
            return Err(error(
                "Mesh DAG requires one parent; instanced topology unsupported",
            ));
        }
        let weights = dmx
            .value_bytes(&attrs["blendweights$0"].value)
            .ok_or_else(|| error("Missing packed weights"))?;
        let indices = dmx.integer_values(&attrs["blendindices$0"].value)?;
        for i in 0..positions {
            if i % 8192 == 0 {
                check(cancel)?;
            }
            let mut total = 0_f64;
            for j in i * slots..(i + 1) * slots {
                let w = f32::from_le_bytes(weights[j * 4..j * 4 + 4].try_into().unwrap()) as f64;
                let joint = indices[j];
                if !(0.0..=1.0).contains(&w) || joint < 0 || joint as usize >= palette.len() {
                    return Err(error("Invalid weight/joint"));
                }
                if w > 0. && elem(dmx, palette[joint as usize])?.kind != "DmeJoint" {
                    return Err(error(
                        "Positive skin weight does not reference bone identity",
                    ));
                }
                total += w;
            }
            if (total - 1.).abs() > 1e-5 {
                return Err(error(
                    "Unweighted or non-normalized vertex; weights will not be changed",
                ));
            }
        }
        let face_refs = list(dmx, mesh, "faceSets")?;
        if face_refs.is_empty()
            || face_refs.iter().collect::<BTreeSet<_>>().len() != face_refs.len()
        {
            return Err(error("Missing/duplicate face sets"));
        }
        let mut face_ids = vec![];
        let mut faces = BTreeMap::new();
        for at in face_refs {
            let face = elem(dmx, at)?;
            if face.kind != "DmeFaceSet" || names(face) != BTreeSet::from(["faces", "material"]) {
                return Err(error("Unsupported face-set data layout"));
            }
            let fa = attributes(face)?;
            if fa["faces"].type_code != 34
                || elem(dmx, reference(face, "material")?)?.kind != "DmeMaterial"
            {
                return Err(error("Invalid face-set topology/material reference"));
            }
            face_ids.push(at as usize);
            faces.insert(at as usize, dmx.integer_values(&fa["faces"].value)?);
        }
        let rewritten: BTreeSet<_> = face_ids
            .iter()
            .copied()
            .chain([state_index as usize])
            .collect();
        for (at, e) in dmx.elements.iter().enumerate() {
            if at == index {
                continue;
            }
            for a in &e.attributes {
                let refs = if a.type_code == 33 {
                    dmx.integer_values(&a.value)?
                } else if a.type_code == 1 {
                    vec![a.value.as_ref_index().unwrap()]
                } else {
                    vec![]
                };
                if refs
                    .iter()
                    .any(|&i| i >= 0 && rewritten.contains(&(i as usize)))
                {
                    return Err(error("Shared state/face data has another owner"));
                }
            }
        }
        Ok(Self {
            dmx,
            index,
            state_index: state_index as usize,
            dag_index,
            transform_ids,
            formats,
            streams,
            slots,
            positions,
            corners: corners.unwrap_or(0),
            face_ids,
            attrs,
            faces,
            cancel: cancel.clone(),
        })
    }
    pub fn mesh_name(&self) -> &str {
        &self.dmx.elements[self.index].name
    }
    fn polygons(&self) -> Result<Vec<Polygon>> {
        let mut out = vec![];
        for &face_set in &self.face_ids {
            let mut start = 0;
            let mut polygon = vec![];
            for (offset, &index) in self.faces[&face_set].iter().enumerate() {
                if offset % 65536 == 0 {
                    check(&self.cancel)?;
                }
                if index == -1 {
                    if polygon.len() < 3 {
                        return Err(error("Invalid polygon with fewer than three corners"));
                    }
                    out.push(Polygon {
                        face_set,
                        start,
                        end: offset + 1,
                        corners: std::mem::take(&mut polygon),
                    });
                    start = offset + 1;
                } else if index < 0 || index as usize >= self.corners {
                    return Err(error("Polygon references an invalid vertex index"));
                } else {
                    polygon.push(index);
                }
            }
            if !polygon.is_empty() {
                return Err(error("Unterminated polygon"));
            }
        }
        Ok(out)
    }
    fn raw_item(&self, name: &str, index: usize, width: usize) -> Result<&[u8]> {
        let a = self
            .attrs
            .get(name)
            .ok_or_else(|| error("Missing vertex stream"))?;
        let Value::Packed { stride, range, .. } = &a.value else {
            return Err(error("Vertex stream is not packed"));
        };
        let size = stride
            .checked_mul(width)
            .ok_or_else(|| error("Stream size overflow"))?;
        let start = index
            .checked_mul(size)
            .ok_or_else(|| error("Stream index overflow"))?;
        let data = self
            .dmx
            .bytes()
            .get(range.clone())
            .ok_or_else(|| error("Stream range is invalid"))?;
        data.get(
            start
                ..start
                    .checked_add(size)
                    .ok_or_else(|| error("Stream range overflow"))?,
        )
        .ok_or_else(|| error("Stream index out of range"))
    }
    fn indices(&self, name: &str) -> Result<Vec<i32>> {
        self.dmx.integer_values(
            &self
                .attrs
                .get(name)
                .ok_or_else(|| error("Missing index stream"))?
                .value,
        )
    }
}
struct Polygon {
    face_set: usize,
    start: usize,
    end: usize,
    corners: Vec<i32>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Segment {
    pub face_set: usize,
    pub start: usize,
    pub end: usize,
}
#[derive(Debug, Clone, Default)]
pub struct Chunk {
    pub corners: Vec<i32>,
    pub faces: BTreeMap<usize, Vec<i32>>,
    pub segments: Vec<Segment>,
    pub triangles: usize,
    pub polygons: usize,
    pub loose_positions: Vec<usize>,
}
fn polygon_chunks(
    layout: &Layout<'_>,
    target: usize,
    minimum: usize,
    mut remaining: usize,
) -> Result<Vec<Chunk>> {
    let mut chunks = vec![];
    let mut chunk = Chunk::default();
    let mut mapping = BTreeMap::new();
    for p in layout.polygons()? {
        let cost = 3 * (p.corners.len() - 2);
        if cost > target {
            return Err(error("A single polygon exceeds the split budget; it cannot be cut without changing triangulation"));
        }
        let reserve = minimum > chunks.len() + 1 && remaining == minimum - chunks.len() - 1;
        if chunk.polygons > 0 && (chunk.triangles * 3 + cost > target || reserve) {
            chunks.push(std::mem::take(&mut chunk));
            mapping.clear();
        }
        let face = chunk.faces.entry(p.face_set).or_default();
        for old in p.corners.iter().copied() {
            let next = chunk.corners.len() as i32;
            let local = *mapping.entry(old).or_insert_with(|| {
                chunk.corners.push(old);
                next
            });
            face.push(local);
        }
        face.push(-1);
        match chunk.segments.last_mut() {
            Some(s) if s.face_set == p.face_set => s.end = p.end,
            _ => chunk.segments.push(Segment {
                face_set: p.face_set,
                start: p.start,
                end: p.end,
            }),
        }
        chunk.triangles += p.corners.len() - 2;
        chunk.polygons += 1;
        remaining = remaining.saturating_sub(1);
    }
    if chunk.polygons > 0 {
        chunks.push(chunk);
    }
    if chunks.is_empty() {
        return Err(error("Mesh has no polygons"));
    }
    Ok(chunks)
}
pub fn partition(layout: &Layout<'_>, target: usize) -> Result<Vec<Chunk>> {
    validate_target(target)?;
    let mut chunks = polygon_chunks(layout, target, 0, 0)?;
    let mut used = vec![false; layout.positions];
    let indices = layout.indices("position$0Indices")?;
    for c in &chunks {
        check(&layout.cancel)?;
        for &at in &c.corners {
            used[indices[at as usize] as usize] = true;
        }
    }
    let loose: Vec<_> = used
        .iter()
        .enumerate()
        .filter(|(_, used)| !**used)
        .map(|(i, _)| i)
        .collect();
    let triangles: usize = chunks.iter().map(|c| c.triangles * 3).sum();
    if loose.len() > chunks.len() * target - triangles {
        let polygons = chunks.iter().map(|c| c.polygons).sum();
        let required = (triangles + loose.len()).div_ceil(target);
        if required > polygons {
            return Err(error(format!("Loose positions require {required} nonempty pieces but mesh has {polygons} polygons; no positions or skin data discarded")));
        }
        check(&layout.cancel)?;
        chunks = polygon_chunks(layout, target, required, polygons)?;
    }
    let mut cursor = 0;
    for c in chunks.iter_mut().rev() {
        check(&layout.cancel)?;
        let end = (cursor + target - c.triangles * 3).min(loose.len());
        c.loose_positions = loose[cursor..end].to_vec();
        cursor = end;
    }
    if cursor != loose.len() {
        return Err(error("Loose positions exceed spare split capacity"));
    }
    Ok(chunks)
}

pub fn make_piece(
    layout: &Layout<'_>,
    chunk: &Chunk,
    name: &str,
    source_sha: &str,
    ordinal: usize,
) -> Result<Vec<u8>> {
    let mut edits = Rewrite::default();
    let mut position_map = vec![];
    for stream in &layout.streams {
        let indices = layout.indices(&format!("{stream}Indices"))?;
        let mut old_to_new = BTreeMap::new();
        let mut old_values = vec![];
        let mut mapped = vec![];
        for &corner in &chunk.corners {
            let old = *indices
                .get(corner as usize)
                .ok_or_else(|| error("Invalid source corner"))?;
            let next = old_values.len() as i32;
            let at = *old_to_new.entry(old).or_insert_with(|| {
                old_values.push(old as usize);
                next
            });
            mapped.push(at);
        }
        if stream == "position$0" {
            old_values.extend(&chunk.loose_positions);
        }
        let mut payload = (old_values.len() as i32).to_le_bytes().to_vec();
        for &old in &old_values {
            payload.extend(layout.raw_item(stream, old, 1)?);
        }
        edits
            .attributes
            .insert((layout.state_index, stream.clone()), payload);
        edits.attributes.insert(
            (layout.state_index, format!("{stream}Indices")),
            ints(mapped),
        );
        if stream == "position$0" {
            position_map = old_values;
        }
    }
    for stream in ["blendweights$0", "blendindices$0"] {
        let mut payload = ((position_map.len() * layout.slots) as i32)
            .to_le_bytes()
            .to_vec();
        for &old in &position_map {
            payload.extend(layout.raw_item(stream, old, layout.slots)?);
        }
        edits
            .attributes
            .insert((layout.state_index, stream.into()), payload);
    }
    edits.attributes.insert(
        (layout.index, "faceSets".into()),
        ints(
            layout
                .face_ids
                .iter()
                .filter(|at| chunk.faces.contains_key(at))
                .map(|&at| at as i32),
        ),
    );
    for &at in &layout.face_ids {
        edits.attributes.insert(
            (at, "faces".into()),
            ints(chunk.faces.get(&at).cloned().unwrap_or_default()),
        );
    }
    for at in [layout.index, layout.dag_index]
        .into_iter()
        .chain(layout.transform_ids.iter().copied())
    {
        edits.names.insert(at, name.into());
    }
    for at in [layout.index, layout.dag_index, layout.state_index]
        .into_iter()
        .chain(layout.face_ids.iter().copied())
    {
        let key = format!(
            "dmb-split-v{PIECE_IDENTITY_VERSION}/{source_sha}/{}/{ordinal}/{at}",
            layout.mesh_name()
        );
        edits.identities.insert(
            at,
            *Uuid::new_v5(&Uuid::NAMESPACE_URL, key.as_bytes()).as_bytes(),
        );
    }
    layout.dmx.rewrite_binary9(&edits)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PieceValidation {
    pub name: String,
    pub source_positions: usize,
    pub indexed_corners: usize,
    pub polygons: usize,
    pub triangles: usize,
    pub render_vertex_upper_bound: usize,
    pub loose_positions_preserved: usize,
    pub streams_bit_exact: Vec<String>,
    pub normalized_weights_verified: bool,
    pub material_references_preserved: bool,
    pub protected_graph_preserved: bool,
    pub mesh_transform_name_binding_preserved: bool,
    pub transform_values_bit_exact: bool,
    pub morphs: String,
    pub passed: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Piece {
    pub name: String,
    pub file: String,
    pub sha256: String,
    pub corner_map: String,
    pub corner_map_sha256: String,
    pub segments: Vec<Segment>,
    #[serde(default)]
    pub loose_positions: Vec<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation: Option<PieceValidation>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    pub source_mesh: String,
    pub source_positions: usize,
    pub pieces: Vec<Piece>,
    pub triangles: usize,
    pub polygons: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleValidation {
    pub passed: bool,
    pub pieces: Vec<PieceValidation>,
    pub coverage: String,
    pub native_compile_passed: bool,
    pub compiled_vertex_count_measured: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SplitReport {
    pub algorithm: u32,
    pub source_sha256: String,
    pub source_file: PathBuf,
    pub target: usize,
    pub hard_limit: usize,
    pub budget: String,
    pub groups: Vec<Group>,
    pub native_compile_passed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation: Option<BundleValidation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub directory: Option<PathBuf>,
}

pub fn validate_piece(
    source: &Layout<'_>,
    generated: &Dmx,
    record: &Piece,
    corners: &[i32],
    target: usize,
) -> Result<PieceValidation> {
    validate_target(target)?;
    let layout = Layout::new(generated, &record.name, &source.cancel)?;
    if layout.index != source.index
        || layout.state_index != source.state_index
        || layout.dag_index != source.dag_index
        || layout.transform_ids != source.transform_ids
    {
        return Err(error("Split changed graph association/element order"));
    }
    if corners.len() != layout.corners
        || corners.iter().collect::<BTreeSet<_>>().len() != corners.len()
        || corners
            .iter()
            .any(|&i| i < 0 || i as usize >= source.corners)
    {
        return Err(error("Invalid split corner provenance/remapping"));
    }
    if layout.formats != source.formats || layout.slots != source.slots {
        return Err(error(
            "Vertex streams or influence count changed during splitting",
        ));
    }
    let mut changed: BTreeMap<usize, BTreeSet<String>> = BTreeMap::new();
    changed.insert(source.index, BTreeSet::from(["faceSets".into()]));
    changed.insert(
        source.state_index,
        source
            .formats
            .iter()
            .cloned()
            .chain(source.streams.iter().map(|s| format!("{s}Indices")))
            .collect(),
    );
    for &i in &source.face_ids {
        changed.insert(i, BTreeSet::from(["faces".into()]));
    }
    let identity_changes: BTreeSet<_> = changed.keys().copied().chain([source.dag_index]).collect();
    if generated.elements.len() != source.dmx.elements.len() {
        return Err(error("Split lost DMX graph elements"));
    }
    for (i, (a, b)) in source
        .dmx
        .elements
        .iter()
        .zip(&generated.elements)
        .enumerate()
    {
        let expected =
            if i == source.index || i == source.dag_index || source.transform_ids.contains(&i) {
                &record.name
            } else {
                &a.name
            };
        if a.kind != b.kind
            || &b.name != expected
            || (!identity_changes.contains(&i) && a.uuid != b.uuid)
        {
            return Err(error("Split changed an untouched element identity"));
        }
        if a.attributes.len() != b.attributes.len()
            || a.attributes
                .iter()
                .zip(&b.attributes)
                .any(|(a, b)| a.name != b.name || a.type_code != b.type_code)
        {
            return Err(error("Split dropped or changed an attribute type"));
        }
        for (old, new) in a.attributes.iter().zip(&b.attributes) {
            if !changed.get(&i).is_some_and(|s| s.contains(&old.name))
                && source.dmx.raw_attribute(old) != generated.raw_attribute(new)
            {
                return Err(error(format!(
                    "Split changed protected metadata {}/{}",
                    a.kind, old.name
                )));
            }
        }
    }
    if layout.face_ids
        != record
            .segments
            .iter()
            .map(|s| s.face_set)
            .collect::<Vec<_>>()
    {
        return Err(error("Split lost/reordered material slots"));
    }
    let mut polygons = 0;
    let mut triangles = 0;
    for segment in &record.segments {
        let original = source
            .faces
            .get(&segment.face_set)
            .ok_or_else(|| error("Unknown provenance face set"))?;
        let actual = layout
            .faces
            .get(&segment.face_set)
            .ok_or_else(|| error("Missing piece face set"))?;
        if segment.start >= segment.end
            || segment.end > original.len()
            || (segment.start > 0 && original[segment.start - 1] != -1)
            || original[segment.end - 1] != -1
        {
            return Err(error("Invalid source polygon range in provenance"));
        }
        if actual.len() != segment.end - segment.start {
            return Err(error("Lost or duplicated polygon corners"));
        }
        let mut n = 0;
        for (j, &index) in actual.iter().enumerate() {
            let want = original[segment.start + j];
            if index == -1 {
                if want != -1 || n < 3 {
                    return Err(error("Polygon topology changed"));
                }
                polygons += 1;
                triangles += n - 2;
                n = 0;
            } else {
                if index < 0 || corners.get(index as usize) != Some(&want) {
                    return Err(error("Polygon coverage/orientation changed"));
                }
                n += 1;
            }
        }
        if n != 0 {
            return Err(error("Unterminated piece polygon"));
        }
    }
    let bound = triangles * 3 + record.loose_positions.len();
    if bound > target || bound >= HARD_LIMIT || layout.positions > target {
        return Err(error("Generated piece exceeds conservative vertex budget"));
    }
    let pos_a = source.indices("position$0Indices")?;
    let pos_b = layout.indices("position$0Indices")?;
    for (local, &old) in corners.iter().enumerate() {
        if local % 8192 == 0 {
            check(&source.cancel)?;
        }
        for stream in &source.streams {
            let index_name = format!("{stream}Indices");
            let expected = source
                .dmx
                .value_bytes(&source.attrs[index_name.as_str()].value)
                .ok_or_else(|| error("Missing packed indices"))?;
            let actual = generated
                .value_bytes(&layout.attrs[index_name.as_str()].value)
                .ok_or_else(|| error("Missing packed indices"))?;
            let a = i32::from_le_bytes(
                expected[old as usize * 4..old as usize * 4 + 4]
                    .try_into()
                    .unwrap(),
            ) as usize;
            let b =
                i32::from_le_bytes(actual[local * 4..local * 4 + 4].try_into().unwrap()) as usize;
            if source.raw_item(stream, a, 1)? != layout.raw_item(stream, b, 1)? {
                return Err(error(format!(
                    "Vertex attribute changed in split stream: {stream}"
                )));
            }
        }
        for stream in ["blendweights$0", "blendindices$0"] {
            if source.raw_item(stream, pos_a[old as usize] as usize, source.slots)?
                != layout.raw_item(stream, pos_b[local] as usize, source.slots)?
            {
                return Err(error("Skin influences/weights changed during splitting"));
            }
        }
    }
    let referenced: BTreeSet<_> = pos_b.iter().map(|&i| i as usize).collect();
    let unused: Vec<_> = (0..layout.positions)
        .filter(|i| !referenced.contains(i))
        .collect();
    if unused.len() != record.loose_positions.len() {
        return Err(error("Loose-position provenance mismatch"));
    }
    for (&local, &old) in unused.iter().zip(&record.loose_positions) {
        if old >= source.positions {
            return Err(error("Invalid loose-position provenance"));
        }
        for (stream, width) in [
            ("position$0", 1),
            ("blendweights$0", source.slots),
            ("blendindices$0", source.slots),
        ] {
            if source.raw_item(stream, old, width)? != layout.raw_item(stream, local, width)? {
                return Err(error("Loose position/skinning changed"));
            }
        }
    }
    Ok(PieceValidation {
        name: record.name.clone(),
        source_positions: layout.positions,
        indexed_corners: layout.corners,
        polygons,
        triangles,
        render_vertex_upper_bound: bound,
        loose_positions_preserved: record.loose_positions.len(),
        streams_bit_exact: source.formats.clone(),
        normalized_weights_verified: true,
        material_references_preserved: true,
        protected_graph_preserved: true,
        mesh_transform_name_binding_preserved: true,
        transform_values_bit_exact: true,
        morphs: "absent; morph/flex layouts rejected".into(),
        passed: true,
    })
}

pub fn validate_bundle(
    dmx: &Dmx,
    report: &SplitReport,
    folder: &Path,
    cancel: &CancellationToken,
) -> Result<BundleValidation> {
    if report.algorithm != VERSION || report.source_sha256 != hash(dmx.bytes()) {
        return Err(error(
            "Split provenance does not match authoritative DMX/algorithm",
        ));
    }
    validate_target(report.target)?;
    let mut checked = vec![];
    let mut groups = BTreeSet::new();
    for group in &report.groups {
        check(cancel)?;
        if !groups.insert(&group.source_mesh) {
            return Err(error("Duplicated source mesh split group"));
        }
        let original = Layout::new(dmx, &group.source_mesh, cancel)?;
        let mut offsets: BTreeMap<_, _> = original.face_ids.iter().map(|&i| (i, 0)).collect();
        let mut triangles = 0;
        let mut polygons = 0;
        let mut used = vec![false; original.positions];
        let mut loose_seen = BTreeSet::new();
        let mut last_face_order = None;
        let pos = original.indices("position$0Indices")?;
        for piece in &group.pieces {
            check(cancel)?;
            let file = safety::inside(folder, &piece.file).map_err(map_error)?;
            let provenance = safety::inside(folder, &piece.corner_map).map_err(map_error)?;
            if safety::digest(&file).map_err(map_error)? != piece.sha256
                || safety::digest(&provenance).map_err(map_error)? != piece.corner_map_sha256
            {
                return Err(error("Generated split file/provenance hash mismatch"));
            }
            let size = fs::metadata(&provenance).map_err(map_error)?.len();
            if size > 8 + report.target as u64 * 4 {
                return Err(error("Split provenance exceeds piece budget"));
            }
            let raw = fs::read(&provenance).map_err(map_error)?;
            if raw.len() < 8 || &raw[..8] != MAGIC || (raw.len() - 8) % 4 != 0 {
                return Err(error("Invalid split provenance file"));
            }
            let corners: Vec<_> = raw[8..]
                .chunks_exact(4)
                .map(|b| i32::from_le_bytes(b.try_into().unwrap()))
                .collect();
            for segment in &piece.segments {
                let order = original
                    .face_ids
                    .iter()
                    .position(|&i| i == segment.face_set)
                    .ok_or_else(|| error("Unknown source face set in split provenance"))?;
                if last_face_order.is_some_and(|last| order < last) {
                    return Err(error("Material/polygon ordering changed across pieces"));
                }
                last_face_order = Some(order);
                let offset = offsets
                    .get_mut(&segment.face_set)
                    .ok_or_else(|| error("Unknown face set"))?;
                if segment.start != *offset {
                    return Err(error("Lost/duplicated source polygon ranges across pieces"));
                }
                *offset = segment.end;
            }
            for &corner in &corners {
                if corner < 0 || corner as usize >= original.corners {
                    return Err(error("Invalid split corner provenance index"));
                }
                used[pos[corner as usize] as usize] = true;
            }
            for &vertex in &piece.loose_positions {
                if !loose_seen.insert(vertex) {
                    return Err(error("Duplicated loose position"));
                }
            }
            let generated = Dmx::load(&file)?;
            let result = validate_piece(&original, &generated, piece, &corners, report.target)?;
            triangles += result.triangles;
            polygons += result.polygons;
            checked.push(result);
        }
        if loose_seen
            != used
                .iter()
                .enumerate()
                .filter(|(_, v)| !**v)
                .map(|(i, _)| i)
                .collect()
        {
            return Err(error("Lost or duplicated loose positions"));
        }
        if offsets
            .iter()
            .any(|(at, &end)| end != original.faces[at].len())
        {
            return Err(error("Split pieces do not cover every original polygon"));
        }
        if triangles != group.triangles
            || polygons != group.polygons
            || group.source_positions != original.positions
        {
            return Err(error("Split topology totals differ from source"));
        }
    }
    if report.groups.is_empty() {
        return Err(error("Split report contains no mesh groups"));
    }
    Ok(BundleValidation {
        passed: true,
        pieces: checked,
        coverage: "exact ordered polygon occurrences and per-corner attributes".into(),
        native_compile_passed: false,
        compiled_vertex_count_measured: false,
    })
}

// Python json.dumps' default ASCII string spelling participates in the existing
// cache identity, including supplementary Unicode represented as surrogate pairs.
fn python_string(value: &str) -> String {
    let mut out = String::from("\"");
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\x08' => out.push_str("\\b"),
            '\x0c' => out.push_str("\\f"),
            c if c < ' ' || c >= '\x7f' => {
                let mut units = [0; 2];
                for unit in c.encode_utf16(&mut units) {
                    out.push_str(&format!("\\u{unit:04x}"));
                }
            }
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}
pub fn cache_key(source_sha: &str, target: usize, names: &[String]) -> String {
    hash(
        format!(
            "[{VERSION}, {}, {target}, [{}]]",
            python_string(source_sha),
            names
                .iter()
                .map(|s| python_string(s))
                .collect::<Vec<_>>()
                .join(", ")
        )
        .as_bytes(),
    )
}
struct Pending(PathBuf);
impl Drop for Pending {
    fn drop(&mut self) {
        if self.0.exists() {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}
pub fn split_file(
    source: &Path,
    mesh_names: &[String],
    output_root: &Path,
    target: usize,
    cancel: &CancellationToken,
) -> Result<SplitReport> {
    validate_target(target)?;
    check(cancel)?;
    let dmx = Dmx::load(source)?;
    let source_sha = hash(dmx.bytes());
    let names: Vec<_> = mesh_names
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    if names.is_empty() {
        return Err(error("Choose a skinned mesh to split"));
    }
    let key = cache_key(&source_sha, target, &names);
    let destination = safety::inside(output_root, &key).map_err(map_error)?;
    let root = destination
        .parent()
        .ok_or_else(|| error("Invalid split directory"))?;
    let source_file = source.canonicalize().map_err(map_error)?;
    if source_file.starts_with(root) {
        return Err(error(
            "Original DMX must be outside generated split directory",
        ));
    }
    if destination.exists() {
        let path = destination.join("split-report.json");
        if fs::metadata(&path).map_err(map_error)?.len() > 32 * 1024 * 1024 {
            return Err(error("Split report exceeds 32 MiB metadata limit"));
        }
        let mut report: SplitReport =
            serde_json::from_slice(&fs::read(path).map_err(map_error)?).map_err(map_error)?;
        if report.target != target
            || report
                .groups
                .iter()
                .map(|g| &g.source_mesh)
                .ne(names.iter())
        {
            return Err(error("Split cache identity mismatch"));
        }
        report.validation = Some(validate_bundle(&dmx, &report, &destination, cancel)?);
        report.directory = Some(destination);
        report.source_file = source_file;
        return Ok(report);
    }
    let layouts = names
        .iter()
        .map(|name| Layout::new(&dmx, name, cancel))
        .collect::<Result<Vec<_>>>()?;
    fs::create_dir_all(root).map_err(map_error)?;
    let pending = Pending(root.join(format!("pending-{}", Uuid::new_v4().simple())));
    fs::create_dir(&pending.0).map_err(map_error)?;
    let mut report=SplitReport{algorithm:VERSION,source_sha256:source_sha.clone(),source_file,target,hard_limit:HARD_LIMIT,budget:"3 × sum(polygon_corner_count - 2) + preserved loose positions; no triangulation performed".into(),groups:vec![],native_compile_passed:false,validation:None,directory:None};
    let mut occupied: BTreeSet<_> = dmx
        .elements
        .iter()
        .filter(|e| ["DmeMesh", "DmeDag"].contains(&e.kind.as_str()))
        .map(|e| e.name.clone())
        .collect();
    for layout in layouts {
        let chunks = partition(&layout, target)?;
        let mut group = Group {
            source_mesh: layout.mesh_name().into(),
            source_positions: layout.positions,
            pieces: vec![],
            triangles: chunks.iter().map(|c| c.triangles).sum(),
            polygons: chunks.iter().map(|c| c.polygons).sum(),
        };
        for (offset, chunk) in chunks.iter().enumerate() {
            check(cancel)?;
            let ordinal = offset + 1;
            let mut name = format!("{}_{ordinal:02}", layout.mesh_name());
            if occupied.contains(&name) {
                name = format!("{}_split_{}_{ordinal:02}", layout.mesh_name(), &key[..12]);
            }
            if !occupied.insert(name.clone()) {
                return Err(error("Deterministic split mesh name collision"));
            }
            let slug: String = name
                .chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                        c
                    } else {
                        '_'
                    }
                })
                .take(70)
                .collect();
            let filename = format!("{slug}_{}.dmx", &hash(name.as_bytes())[..16]);
            let provenance = format!("{filename}.corners.bin");
            let data = make_piece(&layout, chunk, &name, &source_sha, ordinal)?;
            safety::atomic_write(&pending.0.join(&filename), &data).map_err(map_error)?;
            let mut corner_bytes = MAGIC.to_vec();
            for at in &chunk.corners {
                corner_bytes.extend(at.to_le_bytes());
            }
            safety::atomic_write(&pending.0.join(&provenance), &corner_bytes).map_err(map_error)?;
            let mut piece = Piece {
                name,
                file: filename,
                sha256: hash(&data),
                corner_map: provenance,
                corner_map_sha256: hash(&corner_bytes),
                segments: chunk.segments.clone(),
                loose_positions: chunk.loose_positions.clone(),
                validation: None,
            };
            piece.validation = Some(validate_piece(
                &layout,
                &Dmx::parse(data)?,
                &piece,
                &chunk.corners,
                target,
            )?);
            group.pieces.push(piece);
        }
        report.groups.push(group);
    }
    report.validation = Some(validate_bundle(&dmx, &report, &pending.0, cancel)?);
    if safety::digest(source).map_err(map_error)? != source_sha {
        return Err(error(
            "Original DMX changed while splitting; rebuild from stable export",
        ));
    }
    safety::write_json(&pending.0.join("split-report.json"), &report).map_err(map_error)?;
    check(cancel)?;
    fs::rename(&pending.0, &destination).map_err(map_error)?;
    report.directory = Some(destination);
    Ok(report)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum MeshMapping {
    One(String),
    Many(Vec<String>),
}
pub fn expand_mapping(
    mappings: &BTreeMap<String, MeshMapping>,
    report: &SplitReport,
) -> Result<BTreeMap<String, MeshMapping>> {
    let groups: BTreeMap<_, _> = report
        .groups
        .iter()
        .map(|g| {
            (
                g.source_mesh.as_str(),
                g.pieces.iter().map(|p| p.name.clone()).collect::<Vec<_>>(),
            )
        })
        .collect();
    let mut output = BTreeMap::new();
    for (name, value) in mappings {
        let selected = match value {
            MeshMapping::One(s) if s == "@keep" || s == "@disable" => vec![],
            MeshMapping::One(s) => {
                if s.is_empty() {
                    return Err(error("Invalid mesh mapping"));
                }
                vec![s.clone()]
            }
            MeshMapping::Many(v) => {
                if v.is_empty()
                    || v.iter().collect::<BTreeSet<_>>().len() != v.len()
                    || v.iter()
                        .any(|s| s.is_empty() || s == "@keep" || s == "@disable")
                {
                    return Err(error("Invalid mesh piece mapping"));
                }
                v.clone()
            }
        };
        let expanded: Vec<_> = selected
            .iter()
            .flat_map(|s| {
                groups
                    .get(s.as_str())
                    .cloned()
                    .unwrap_or_else(|| vec![s.clone()])
            })
            .collect();
        output.insert(
            name.clone(),
            match expanded.len() {
                0 => value.clone(),
                1 => MeshMapping::One(expanded[0].clone()),
                _ => MeshMapping::Many(expanded),
            },
        );
    }
    Ok(output)
}
