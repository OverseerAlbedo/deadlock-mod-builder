//! Read-only DMX bind/rest preview. Geometry is never used for compilation.
//! f32 serialized streams are decoded exactly; transform and skin arithmetic uses
//! f64, matching the reference preview before final GPU f32 conversion.
#![forbid(unsafe_code)]
pub mod gltf;
mod math;
mod triangulate;

/// Shared strict source-polygon triangulation used by preview and offline cloth
/// authoring. Returned indices address the input corner stream. Invalid inputs
/// fail before the internal triangulator can index the source arrays.
pub fn triangulate_polygon(
    corners: &[u32],
    positions: &[[f32; 3]],
    position_indices: &[usize],
    cancel: &deadlock_core::CancellationToken,
) -> deadlock_core::Result<Vec<[u32; 3]>> {
    cancel.check()?;
    if corners.iter().any(|&c| {
        position_indices.get(c as usize).is_none_or(|&i| {
            positions
                .get(i)
                .is_none_or(|p| p.iter().any(|v| !v.is_finite()))
        })
    }) {
        return Err(deadlock_core::CoreError::Integrity(
            "Polygon corners reference missing or non-finite positions".into(),
        ));
    }
    triangulate::polygon(corners, positions, position_indices, cancel)
}
use deadlock_core::{CancellationToken, CoreError, Result, TimingRecorder};
use deadlock_dmx::{Dmx, Element, Value};
use math::{cross, identity, inverse, mul, normalize, sub, transform, Matrix};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::Read,
    path::Path,
};
pub const MAX_VERTICES: usize = 6_000_000;
pub const MAX_TRIANGLES: usize = 4_000_000;
const MAX_NODES: usize = 16_384;
const MAX_PACKET: usize = 512 * 1024 * 1024;
fn fail(message: impl Into<String>) -> CoreError {
    CoreError::Integrity(message.into())
}
fn dmx_error(error: impl std::fmt::Display) -> CoreError {
    fail(error.to_string())
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BufferSlice {
    pub offset: usize,
    pub count: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkinInfo {
    pub influences: usize,
    pub joint_names: Vec<String>,
    pub width: usize,
    pub height: usize,
    pub data: BufferSlice,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshInfo {
    pub name: String,
    pub source_name: String,
    pub material: String,
    pub vertex_count: usize,
    pub index_count: usize,
    pub positions: BufferSlice,
    pub normals: BufferSlice,
    pub uvs: Option<BufferSlice>,
    pub colors: Option<BufferSlice>,
    pub indices: BufferSlice,
    pub skin: Option<SkinInfo>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoneInfo {
    pub name: String,
    pub parent: i32,
    pub position: [f64; 3],
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bounds {
    pub min: [f64; 3],
    pub max: [f64; 3],
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metadata {
    pub schema_version: u32,
    pub source: String,
    pub source_sha256: String,
    pub up_axis: String,
    pub warnings: Vec<String>,
    pub bounds: Bounds,
    pub meshes: Vec<MeshInfo>,
    pub bones: Vec<BoneInfo>,
}
pub struct Preview {
    pub metadata: Metadata,
    pub data: Vec<u8>,
}
fn measure<T>(
    clock: &TimingRecorder,
    name: &str,
    operation: impl FnOnce() -> Result<T>,
) -> Result<T> {
    let guard = clock.start_step(name, None)?;
    match operation() {
        Ok(value) => {
            guard.success()?;
            Ok(value)
        }
        Err(error) => {
            if matches!(error, CoreError::Cancelled) {
                guard.cancel()?;
            } else {
                guard.fail(error.to_string())?;
            }
            Err(error)
        }
    }
}
pub fn load_packet(
    path: &Path,
    cancel: &CancellationToken,
    clock: &TimingRecorder,
) -> Result<Vec<u8>> {
    let (source, bytes) = measure(clock, "Read preview DMX", || {
        cancel.check()?;
        let path = path
            .canonicalize()
            .map_err(|e| CoreError::io("Preview source could not be opened", e))?;
        if path
            .extension()
            .and_then(|x| x.to_str())
            .is_none_or(|s| !s.eq_ignore_ascii_case("dmx"))
        {
            return Err(fail("Select a binary DMX model for this preview"));
        }
        let mut file =
            File::open(&path).map_err(|e| CoreError::io("Preview source could not be read", e))?;
        let size = file
            .metadata()
            .map_err(|e| CoreError::io("Preview source metadata", e))?
            .len();
        if size > deadlock_dmx::MAX_INPUT_BYTES as u64 {
            return Err(fail("Preview DMX exceeds 512 MiB"));
        }
        let mut bytes = Vec::with_capacity(size as usize);
        let mut chunk = [0u8; 1024 * 1024];
        loop {
            cancel.check()?;
            let count = file
                .read(&mut chunk)
                .map_err(|e| CoreError::io("Preview source read", e))?;
            if count == 0 {
                break;
            }
            if bytes.len() + count > deadlock_dmx::MAX_INPUT_BYTES {
                return Err(fail("Preview DMX exceeds 512 MiB"));
            }
            bytes.extend_from_slice(&chunk[..count]);
        }
        let source = path
            .to_str()
            .ok_or_else(|| fail("Preview path is not valid Unicode"))?
            .to_owned();
        Ok((source, bytes))
    })?;
    let document = measure(clock, "Parse preview DMX", || {
        Dmx::parse_with_cancel(bytes, cancel).map_err(|error| {
            if cancel.is_cancelled() {
                CoreError::Cancelled
            } else {
                dmx_error(error)
            }
        })
    })?;
    let preview = measure(clock, "Assemble bind and rest preview", || {
        from_dmx(&document, source, cancel)
    })?;
    measure(clock, "Encode preview buffers", || preview.packet(cancel))
}
impl Preview {
    pub fn packet(&self, cancel: &CancellationToken) -> Result<Vec<u8>> {
        cancel.check()?;
        let json = serde_json::to_vec(&self.metadata)?;
        if json.len() > 16 * 1024 * 1024 {
            return Err(fail("Preview metadata exceeds 16 MiB"));
        }
        let padded = (json.len() + 3) & !3;
        let total = 16 + padded + self.data.len();
        if total > MAX_PACKET {
            return Err(fail("Preview packet exceeds 512 MiB"));
        }
        let mut output = Vec::with_capacity(total);
        output.extend_from_slice(b"DMBP");
        output.extend_from_slice(&1u32.to_le_bytes());
        output.extend_from_slice(&(json.len() as u32).to_le_bytes());
        output.extend_from_slice(&0u32.to_le_bytes());
        output.extend(json);
        output.resize(16 + padded, 0);
        for chunk in self.data.chunks(1024 * 1024) {
            cancel.check()?;
            output.extend_from_slice(chunk);
        }
        Ok(output)
    }
}
fn element(dmx: &Dmx, index: i32) -> Result<&Element> {
    if index < 0 {
        return Err(fail("Preview element reference is null"));
    }
    dmx.elements
        .get(index as usize)
        .ok_or_else(|| fail("Preview element reference is out of range"))
}
fn reference(value: Option<&Value>) -> Result<i32> {
    match value {
        Some(Value::ElementRef(i)) => Ok(*i),
        None => Ok(-1),
        _ => Err(fail("Preview expected an element reference")),
    }
}
fn ints(dmx: &Dmx, value: Option<&Value>) -> Result<Vec<i32>> {
    match value {
        None => Ok(vec![]),
        Some(v) => dmx.integer_values(v).map_err(dmx_error),
    }
}
fn floats(dmx: &Dmx, value: &Value) -> Result<Vec<f32>> {
    let bytes = dmx
        .value_bytes(value)
        .ok_or_else(|| fail("Preview expected a numeric stream"))?;
    if bytes.len() % 4 != 0 {
        return Err(fail("Preview numeric stream has invalid width"));
    }
    let values: Vec<f32> = bytes
        .chunks_exact(4)
        .map(|x| f32::from_le_bytes(x.try_into().unwrap()))
        .collect();
    if values.iter().any(|v| !v.is_finite()) {
        return Err(fail("Preview numeric stream contains non-finite values"));
    }
    Ok(values)
}
fn vector<const N: usize>(
    dmx: &Dmx,
    value: Option<&Value>,
    fallback: [f64; N],
) -> Result<[f64; N]> {
    match value {
        None => Ok(fallback),
        Some(value) => {
            let data = floats(dmx, value)?;
            if data.len() != N {
                return Err(fail("Preview transform has invalid vector width"));
            }
            Ok(std::array::from_fn(|i| f64::from(data[i])))
        }
    }
}
fn local(dmx: &Dmx, node: &Element) -> Result<Matrix> {
    let position = vector(dmx, node.get("position"), [0.; 3])?;
    let rotation = vector(dmx, node.get("orientation"), [0., 0., 0., 1.])?;
    let scale = match node.get("scale") {
        None => 1.,
        Some(Value::Float(x)) => f64::from(*x),
        Some(Value::Integer(x)) => f64::from(*x),
        _ => return Err(fail("Preview requires scalar local scale")),
    };
    math::trs(position, rotation, scale)
}
fn transform_ref(dmx: &Dmx, index: i32) -> Result<Matrix> {
    let tr = element(dmx, index)?;
    if tr.kind != "DmeTransform" {
        return Err(fail("Preview node has no supported transform"));
    }
    local(dmx, tr)
}
fn world_matrices(locals: &[Matrix], parents: &[i32]) -> Result<Vec<Matrix>> {
    let mut world = vec![identity(); locals.len()];
    let mut state = vec![0u8; locals.len()];
    for index in 0..locals.len() {
        let mut chain = Vec::new();
        let mut current = index as i32;
        while current >= 0 && state[current as usize] != 2 {
            let i = current as usize;
            if state[i] == 1 {
                return Err(fail("Preview node hierarchy contains a cycle"));
            }
            state[i] = 1;
            chain.push(i);
            current = parents[i];
        }
        for i in chain.into_iter().rev() {
            world[i] = if parents[i] >= 0 {
                mul(&world[parents[i] as usize], &locals[i])
            } else {
                locals[i]
            };
            state[i] = 2;
        }
    }
    if world.iter().flatten().flatten().any(|x| !x.is_finite()) {
        return Err(fail("Preview transforms overflowed"));
    }
    Ok(world)
}
struct Stream<const N: usize> {
    data: Vec<[f32; N]>,
    indices: Vec<usize>,
}
fn stream<const N: usize>(
    dmx: &Dmx,
    state: &Element,
    names: &[&str],
    required: bool,
) -> Result<Option<Stream<N>>> {
    let Some(name) = names.iter().find(|&&name| state.get(name).is_some()) else {
        if required {
            return Err(fail("Preview mesh is missing positions"));
        }
        return Ok(None);
    };
    let raw = floats(dmx, state.get(name).unwrap())?;
    if raw.len() % N != 0 {
        return Err(fail("Preview attribute has invalid vector width"));
    }
    let data: Vec<[f32; N]> = raw
        .chunks_exact(N)
        .map(|chunk| chunk.try_into().unwrap())
        .collect();
    let key = format!("{name}Indices");
    let indices = if let Some(value) = state.get(&key) {
        let indices = ints(dmx, Some(value))?;
        if indices.iter().any(|&i| i < 0 || i as usize >= data.len()) {
            return Err(fail("Preview attribute index is outside its stream"));
        }
        indices.into_iter().map(|x| x as usize).collect()
    } else {
        (0..data.len()).collect()
    };
    Ok(Some(Stream { data, indices }))
}
fn normal_transform(m: &Matrix, n: [f64; 3]) -> Result<[f64; 3]> {
    let a = [m[0][0], m[0][1], m[0][2]];
    let b = [m[1][0], m[1][1], m[1][2]];
    let c = [m[2][0], m[2][1], m[2][2]];
    let rows = [cross(b, c), cross(c, a), cross(a, b)];
    let determinant = math::dot(a, rows[0]);
    if !determinant.is_finite() || determinant == 0. {
        return Err(fail("Preview normal transform is singular"));
    }
    Ok(normalize(std::array::from_fn(|i| {
        math::dot(rows[i], n) / determinant
    })))
}
fn f32_checked(value: f64) -> Result<f32> {
    let value = value as f32;
    if !value.is_finite() {
        return Err(fail("Preview geometry overflows GPU float precision"));
    }
    Ok(value)
}
fn push_floats<const N: usize>(
    output: &mut Vec<u8>,
    values: &[[f32; N]],
    cancel: &CancellationToken,
) -> Result<BufferSlice> {
    let offset = output.len();
    let count = values
        .len()
        .checked_mul(N)
        .ok_or_else(|| fail("Preview buffer size overflow"))?;
    if offset + count * 4 > MAX_PACKET {
        return Err(fail("Preview buffer exceeds 512 MiB"));
    }
    for (index, row) in values.iter().enumerate() {
        if index % 32768 == 0 {
            cancel.check()?;
        }
        for value in row {
            output.extend_from_slice(&value.to_le_bytes());
        }
    }
    Ok(BufferSlice { offset, count })
}
struct Skin {
    matrices: Vec<Matrix>,
    names: Vec<String>,
}

pub fn from_dmx(dmx: &Dmx, source: String, cancel: &CancellationToken) -> Result<Preview> {
    cancel.check()?;
    let refs: Vec<usize> = dmx
        .elements
        .iter()
        .enumerate()
        .filter(|(_, e)| matches!(e.kind.as_str(), "DmeModel" | "DmeDag" | "DmeJoint"))
        .map(|(i, _)| i)
        .collect();
    if refs.len() > MAX_NODES {
        return Err(fail("Preview contains too many transform nodes"));
    }
    let lookup: BTreeMap<usize, usize> = refs
        .iter()
        .enumerate()
        .map(|(node, &reference)| (reference, node))
        .collect();
    let mut locals = Vec::new();
    let mut parents = vec![-1; refs.len()];
    for &index in &refs {
        locals.push(transform_ref(
            dmx,
            reference(dmx.elements[index].get("transform"))?,
        )?);
    }
    for (&reference, &node) in &lookup {
        for child in ints(dmx, dmx.elements[reference].get("children"))? {
            let child = *lookup
                .get(&(child as usize))
                .ok_or_else(|| fail("Preview graph has an unsupported child node"))?;
            if parents[child] != -1 {
                return Err(fail("Preview graph has multiple parents"));
            }
            parents[child] = node as i32;
        }
    }
    world_matrices(&locals, &parents)?;
    let mut palettes = Vec::new();
    for &index in &refs {
        let model = &dmx.elements[index];
        if model.kind != "DmeModel" {
            continue;
        }
        let palette = ints(dmx, model.get("jointList"))?;
        if palette.is_empty() {
            continue;
        }
        if palette.iter().collect::<BTreeSet<_>>().len() != palette.len()
            || palette
                .iter()
                .any(|&r| r < 0 || !lookup.contains_key(&(r as usize)))
        {
            return Err(fail("Preview joint palette is invalid"));
        }
        let bases = ints(dmx, model.get("baseStates"))?;
        let binds: Vec<_> = bases
            .into_iter()
            .filter(|&i| i >= 0 && dmx.elements[i as usize].name == "bind")
            .collect();
        if binds.len() > 1 {
            return Err(fail("Preview requires one explicit bind transform list"));
        }
        let Some(&bind) = binds.first() else {
            // Some decompiled meshes (Haze's guns) have no bind list, only an
            // unnamed partial base state: their joints' own transforms are the
            // rest pose, which `locals` already holds.
            palettes.push(palette);
            continue;
        };
        let transforms = ints(dmx, dmx.elements[bind as usize].get("transforms"))?;
        if transforms.len() != palette.len() {
            return Err(fail(format!(
                "Preview bind transforms do not match joint palette ({} transforms for {} joints)",
                transforms.len(),
                palette.len()
            )));
        }
        for (&joint, &tr) in palette.iter().zip(transforms.iter()) {
            locals[lookup[&(joint as usize)]] = transform_ref(dmx, tr)?;
        }
        palettes.push(palette);
    }
    let world = world_matrices(&locals, &parents)?;
    let mut skins = BTreeMap::<usize, Skin>::new();
    for palette in palettes {
        let palette_nodes: Vec<usize> = palette.iter().map(|r| lookup[&(*r as usize)]).collect();
        let inverse_world: Vec<Matrix> = palette_nodes
            .iter()
            .map(|&n| inverse(&world[n]))
            .collect::<Result<_>>()?;
        for &reference in &palette {
            if dmx.elements[reference as usize].kind != "DmeDag" {
                continue;
            }
            let node = lookup[&(reference as usize)];
            if skins.contains_key(&node) {
                return Err(fail("Preview mesh appears in multiple skeleton palettes"));
            }
            let matrices = palette_nodes
                .iter()
                .zip(&inverse_world)
                .map(|(&joint, inverse)| mul(&world[joint], &mul(inverse, &world[node])))
                .collect();
            skins.insert(
                node,
                Skin {
                    matrices,
                    names: palette
                        .iter()
                        .map(|&r| dmx.elements[r as usize].name.clone())
                        .collect(),
                },
            );
        }
    }
    let bone_nodes: Vec<_> = refs
        .iter()
        .enumerate()
        .filter(|(_, reference)| dmx.elements[**reference].kind == "DmeJoint")
        .map(|(node, _)| node)
        .collect();
    let bone_lookup: BTreeMap<_, _> = bone_nodes
        .iter()
        .enumerate()
        .map(|(i, &node)| (node, i))
        .collect();
    let bones: Vec<BoneInfo> = bone_nodes
        .iter()
        .map(|&node| {
            let mut parent = parents[node];
            while parent >= 0 && !bone_lookup.contains_key(&(parent as usize)) {
                parent = parents[parent as usize];
            }
            BoneInfo {
                name: dmx.elements[refs[node]].name.clone(),
                parent: if parent < 0 {
                    -1
                } else {
                    bone_lookup[&(parent as usize)] as i32
                },
                position: transform(&world[node], [0.; 3]).map(|v| f64::from(v as f32)),
            }
        })
        .collect();
    if bones
        .iter()
        .flat_map(|bone| bone.position)
        .any(|x| !x.is_finite())
    {
        return Err(fail("Preview bone positions overflow GPU precision"));
    }
    let mut metadata=Metadata{schema_version:1,source,source_sha256:format!("{:x}",Sha256::digest(dmx.bytes())),up_axis:"Z".into(),warnings:vec!["DMX preview shows explicit bind/rest transforms. Deadlock animations, shaders and cloth simulation are not reproduced.".into(),"Untextured material inspection; saved game shader appearance requires native validation.".into()],bounds:Bounds{min:[f64::INFINITY;3],max:[f64::NEG_INFINITY;3]},meshes:vec![],bones};
    let mut output = Vec::new();
    let mut vertices_total = 0;
    let mut triangles_total = 0;
    let mut mesh_names = BTreeSet::new();
    for (mesh_ref, mesh) in dmx.elements.iter().enumerate() {
        if mesh.kind != "DmeMesh" {
            continue;
        }
        cancel.check()?;
        let owners: Vec<_> = refs
            .iter()
            .filter(|&&r| reference(dmx.elements[r].get("shape")).ok() == Some(mesh_ref as i32))
            .collect();
        if owners.len() != 1 {
            return Err(fail("Preview requires one owning DAG per mesh"));
        }
        let owner = **owners.first().unwrap();
        let node = lookup[&owner];
        if matches!(dmx.elements[owner].get("visible"), Some(Value::Bool(false)))
            || matches!(mesh.get("visible"), Some(Value::Bool(false)))
        {
            continue;
        }
        // A null bindState (decompiled meshes such as Haze's guns) falls back
        // to currentState like a missing one.
        let bind = reference(mesh.get("bindState"))?;
        let state = element(
            dmx,
            if bind >= 0 { bind } else { reference(mesh.get("currentState"))? },
        )?;
        if state.kind != "DmeVertexData" {
            return Err(fail("Preview mesh has no supported vertex state"));
        }
        if mesh.get("deltaStates").is_some_and(|v| !v.is_empty()) {
            metadata.warnings.push(format!(
                "{}: DMX flex controller evaluation is unavailable; showing base mesh.",
                mesh.name
            ));
        }
        let positions = stream::<3>(dmx, state, &["position$0", "positions"], true)?.unwrap();
        let normals = stream::<3>(dmx, state, &["normal$0", "normals"], false)?;
        let uvs = stream::<2>(dmx, state, &["texcoord$0", "textureCoordinates"], false)?;
        let colors = stream::<4>(dmx, state, &["color$0"], false)?;
        if positions.indices.len() > MAX_VERTICES * 4 {
            return Err(fail("Preview contains too many polygon corners"));
        }
        for length in [
            normals.as_ref().map(|x| x.indices.len()),
            uvs.as_ref().map(|x| x.indices.len()),
            colors.as_ref().map(|x| x.indices.len()),
        ]
        .into_iter()
        .flatten()
        {
            if length != positions.indices.len() {
                return Err(fail("Preview corner streams have mismatched lengths"));
            }
        }
        let raw_weights = state
            .get("blendweights$0")
            .or_else(|| state.get("jointWeights"));
        let raw_joints = state
            .get("blendindices$0")
            .or_else(|| state.get("jointIndices"));
        let mut weights = Vec::new();
        let mut joints = Vec::new();
        let mut slots = 0usize;
        if raw_weights.is_some() || raw_joints.is_some() {
            slots = match state.get("jointCount") {
                Some(Value::Integer(value)) if (1..=64).contains(value) => *value as usize,
                _ => return Err(fail("Preview has invalid skin influence layout")),
            };
            weights = floats(
                dmx,
                raw_weights.ok_or_else(|| fail("Preview skin is missing weights"))?,
            )?;
            joints = ints(
                dmx,
                Some(raw_joints.ok_or_else(|| fail("Preview skin is missing joint indices"))?),
            )?;
            let skin = skins
                .get(&node)
                .ok_or_else(|| fail("Preview skin has no skeleton palette"))?;
            if weights.len() != positions.data.len() * slots || joints.len() != weights.len() {
                return Err(fail(
                    "Preview weights do not match positions and skeleton palette",
                ));
            }
            if joints
                .iter()
                .any(|&j| j < 0 || j as usize >= skin.matrices.len())
                || weights.iter().any(|&w| w < 0.)
                || weights.chunks_exact(slots).any(|row| {
                    (row.iter().map(|&x| f64::from(x)).sum::<f64>() - 1.).abs() > 0.00201
                })
            {
                return Err(fail(
                    "Preview has invalid joint indices or unnormalized skin weights",
                ));
            }
        }
        let face_refs = ints(dmx, mesh.get("faceSets"))?;
        for &face_ref in &face_refs {
            cancel.check()?;
            let face = element(dmx, face_ref)?;
            let triangles = triangulate::faces(
                &ints(dmx, face.get("faces"))?,
                &positions.data,
                &positions.indices,
                cancel,
                &mut metadata.warnings,
            )?;
            triangles_total += triangles.len();
            if triangles_total > MAX_TRIANGLES {
                return Err(fail("Preview exceeds four million triangles"));
            }
            let key = |corner: usize| {
                [
                    positions.indices[corner],
                    normals
                        .as_ref()
                        .map(|x| x.indices[corner])
                        .unwrap_or(usize::MAX),
                    uvs.as_ref()
                        .map(|x| x.indices[corner])
                        .unwrap_or(usize::MAX),
                    colors
                        .as_ref()
                        .map(|x| x.indices[corner])
                        .unwrap_or(usize::MAX),
                ]
            };
            let mut unique = BTreeMap::<[usize; 4], u32>::new();
            for (index, triangle) in triangles.iter().enumerate() {
                if index % 32768 == 0 {
                    cancel.check()?;
                }
                for &corner in triangle {
                    unique.entry(key(corner as usize)).or_default();
                }
            }
            vertices_total += unique.len();
            if vertices_total > MAX_VERTICES {
                return Err(fail("Preview exceeds six million vertices"));
            }
            for (id, value) in unique.values_mut().enumerate() {
                *value = id as u32;
            }
            let remapped: Vec<[u32; 3]> = triangles
                .iter()
                .map(|triangle| triangle.map(|c| unique[&key(c as usize)]))
                .collect();
            let mut source_positions = Vec::with_capacity(unique.len());
            let mut source_normals = Vec::with_capacity(unique.len());
            let mut output_uvs = Vec::new();
            let mut output_colors = Vec::new();
            let mut position_ids = Vec::new();
            for indices in unique.keys() {
                position_ids.push(indices[0]);
                source_positions.push(positions.data[indices[0]]);
                if let Some(normals) = &normals {
                    source_normals.push(normals.data[indices[1]]);
                }
                if let Some(uvs) = &uvs {
                    let mut uv = uvs.data[indices[2]];
                    if matches!(state.get("flipVCoordinates"), Some(Value::Bool(true))) {
                        uv[1] = 1. - uv[1];
                    }
                    output_uvs.push(uv);
                }
                if let Some(colors) = &colors {
                    output_colors.push(colors.data[indices[3]]);
                }
            }
            if normals.is_none() {
                let mut accumulated = vec![[0f64; 3]; source_positions.len()];
                for triangle in &remapped {
                    let [a, b, c] = triangle.map(|i| source_positions[i as usize].map(f64::from));
                    let normal = cross(sub(b, a), sub(c, a));
                    for &vertex in triangle {
                        for axis in 0..3 {
                            accumulated[vertex as usize][axis] += normal[axis];
                        }
                    }
                }
                source_normals = accumulated
                    .into_iter()
                    .map(|normal| normalize(normal).map(|v| v as f32))
                    .collect();
            }
            for i in 0..source_positions.len() {
                if i % 32768 == 0 {
                    cancel.check()?;
                }
                let matrix = if slots == 0 {
                    world[node]
                } else {
                    let skin = &skins[&node];
                    let mut blended = identity();
                    for row in blended.iter_mut().take(3) {
                        *row = [0.; 4];
                    }
                    let start = position_ids[i] * slots;
                    for slot in 0..slots {
                        let matrix = &skin.matrices[joints[start + slot] as usize];
                        let weight = f64::from(weights[start + slot]);
                        for r in 0..3 {
                            for c in 0..4 {
                                blended[r][c] += matrix[r][c] * weight;
                            }
                        }
                    }
                    blended
                };
                let point = transform(&matrix, source_positions[i].map(f64::from));
                let normal = normal_transform(&matrix, source_normals[i].map(f64::from))?;
                for axis in 0..3 {
                    source_positions[i][axis] = f32_checked(point[axis])?;
                    source_normals[i][axis] = f32_checked(normal[axis])?;
                    metadata.bounds.min[axis] =
                        metadata.bounds.min[axis].min(f64::from(source_positions[i][axis]));
                    metadata.bounds.max[axis] =
                        metadata.bounds.max[axis].max(f64::from(source_positions[i][axis]));
                }
            }
            let material_ref = reference(face.get("material"))?;
            let material = if material_ref < 0 {
                face.name.clone()
            } else {
                let material = element(dmx, material_ref)?;
                material
                    .get("mtlName")
                    .and_then(Value::as_str)
                    .unwrap_or(&material.name)
                    .to_owned()
            };
            let mut name = if face_refs.len() == 1 {
                mesh.name.clone()
            } else {
                format!(
                    "{} / {}",
                    mesh.name,
                    Path::new(&material)
                        .file_stem()
                        .and_then(|x| x.to_str())
                        .unwrap_or(&material)
                )
            };
            if !mesh_names.insert(name.clone()) {
                name.push_str(&format!(" [{mesh_ref}:{face_ref}]"));
                mesh_names.insert(name.clone());
            }
            let vertex_count = source_positions.len();
            let index_count = remapped.len() * 3;
            let positions_slice = push_floats(&mut output, &source_positions, cancel)?;
            let normals_slice = push_floats(&mut output, &source_normals, cancel)?;
            let uv_slice = if uvs.is_some() {
                Some(push_floats(&mut output, &output_uvs, cancel)?)
            } else {
                None
            };
            let color_slice = if colors.is_some() {
                Some(push_floats(&mut output, &output_colors, cancel)?)
            } else {
                None
            };
            let indices_slice = BufferSlice {
                offset: output.len(),
                count: index_count,
            };
            if output.len() + index_count * 4 > MAX_PACKET {
                return Err(fail("Preview packet exceeds 512 MiB"));
            }
            for (index, triangle) in remapped.iter().enumerate() {
                if index % 32768 == 0 {
                    cancel.check()?;
                }
                for value in triangle {
                    output.extend_from_slice(&value.to_le_bytes());
                }
            }
            let skin = if slots > 0 {
                let texels = (vertex_count * slots).div_ceil(2);
                let width = texels.clamp(1, 4096);
                let height = texels.div_ceil(width);
                let count = width * height * 4;
                if output.len() + count * 4 > MAX_PACKET {
                    return Err(fail("Preview skin buffers exceed 512 MiB packet limit"));
                }
                let offset = output.len();
                for (index, &position) in position_ids.iter().enumerate() {
                    if index % 32768 == 0 {
                        cancel.check()?;
                    }
                    for slot in 0..slots {
                        output.extend_from_slice(
                            &(joints[position * slots + slot] as f32).to_le_bytes(),
                        );
                        output.extend_from_slice(&weights[position * slots + slot].to_le_bytes());
                    }
                }
                output.resize(offset + count * 4, 0);
                Some(SkinInfo {
                    influences: slots,
                    joint_names: skins[&node].names.clone(),
                    width,
                    height,
                    data: BufferSlice { offset, count },
                })
            } else {
                None
            };
            metadata.meshes.push(MeshInfo {
                name,
                source_name: mesh.name.clone(),
                material,
                vertex_count,
                index_count,
                positions: positions_slice,
                normals: normals_slice,
                uvs: uv_slice,
                colors: color_slice,
                indices: indices_slice,
                skin,
            });
        }
    }
    if metadata.meshes.is_empty() {
        return Err(fail("Preview contains no visible triangle meshes"));
    }
    Ok(Preview {
        metadata,
        data: output,
    })
}
