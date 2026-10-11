//! Bounded, read-only glTF intake. The browser receives a fully embedded GLB,
//! never filesystem URLs. Preview acceptance is not build or game acceptance.
use crate::{fail, measure, MAX_NODES, MAX_PACKET, MAX_TRIANGLES, MAX_VERTICES};
use base64::{engine::general_purpose::STANDARD, Engine};
use deadlock_core::process::{CommandSpec, ToolExecutor};
use deadlock_core::{CancellationToken, CoreError, Result, TimingRecorder};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{Cursor, Read},
    path::{Path, PathBuf},
    sync::Arc,
};
const MAX_INPUT: usize = 512 * 1024 * 1024;
const MAX_TEXTURE: usize = 32 * 1024 * 1024;
const MAX_JSON: usize = 32 * 1024 * 1024;
const EXPORTER: &str = include_str!("preview_export.py");
const SOURCE_HELPERS: &str =
    include_str!("../../deadlock-preparation/resources/blender/source_helpers.py");
fn err(e: impl std::fmt::Display) -> CoreError {
    fail(e.to_string())
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn int(v: &Value) -> Result<usize> {
    v.as_u64()
        .and_then(|v| usize::try_from(v).ok())
        .ok_or_else(|| fail("Expected a nonnegative glTF integer"))
}
fn field(v: &Value, k: &str) -> Result<usize> {
    int(v.get(k).ok_or_else(|| fail(format!("Missing glTF {k}")))?)
}
fn defint(v: &Value, k: &str, default: usize) -> Result<usize> {
    v.get(k).map(int).unwrap_or(Ok(default))
}
fn array<'a>(v: &'a Value, k: &str) -> Result<&'a [Value]> {
    match v.get(k) {
        None => Ok(&[]),
        Some(Value::Array(a)) => Ok(a),
        _ => Err(fail(format!("Invalid glTF {k} array"))),
    }
}
fn at<'a>(v: &'a Value, k: &str, i: usize) -> Result<&'a Value> {
    array(v, k)?
        .get(i)
        .ok_or_else(|| fail(format!("Invalid glTF {k} reference")))
}
fn obj(v: &Value) -> Result<&serde_json::Map<String, Value>> {
    v.as_object().ok_or_else(|| fail("Expected a glTF object"))
}
fn nums(v: &Value, n: Option<usize>) -> Result<Vec<f64>> {
    let a = v
        .as_array()
        .ok_or_else(|| fail("Expected numeric glTF array"))?;
    if n.is_some_and(|n| n != a.len()) {
        return Err(fail("Wrong glTF numeric array shape"));
    }
    a.iter()
        .map(|v| {
            v.as_f64()
                .filter(|x| x.is_finite())
                .ok_or_else(|| fail("Non-finite or nonnumeric glTF value"))
        })
        .collect()
}
fn default_nums(v: &Value, k: &str, d: &[f64]) -> Result<Vec<f64>> {
    v.get(k)
        .map(|v| nums(v, Some(d.len())))
        .unwrap_or_else(|| Ok(d.to_vec()))
}
fn bounded_file(
    path: &Path,
    limit: usize,
    cancel: &CancellationToken,
) -> Result<(PathBuf, Vec<u8>)> {
    cancel.check()?;
    deadlock_vpk::check_unlinked_path(path, false).map_err(err)?;
    let path = path
        .canonicalize()
        .map_err(|e| CoreError::io("Resolve preview file", e))?;
    let mut file = File::open(&path).map_err(|e| CoreError::io("Open preview file", e))?;
    let before = file
        .metadata()
        .map_err(|e| CoreError::io("Read preview metadata", e))?;
    if !before.is_file() || before.len() > limit as u64 {
        return Err(fail("Preview input is not an ordinary bounded file"));
    }
    let identity = same_file::Handle::from_file(
        file.try_clone()
            .map_err(|e| CoreError::io("Clone preview input", e))?,
    )
    .map_err(err)?;
    let mut data = Vec::with_capacity(before.len() as usize);
    let mut chunk = [0; 1024 * 1024];
    loop {
        cancel.check()?;
        let n = file
            .read(&mut chunk)
            .map_err(|e| CoreError::io("Read preview input", e))?;
        if n == 0 {
            break;
        }
        if data.len() + n > limit {
            return Err(fail("Preview input grew beyond its size limit"));
        }
        data.extend_from_slice(&chunk[..n]);
    }
    let after = file
        .metadata()
        .map_err(|e| CoreError::io("Read preview metadata", e))?;
    deadlock_vpk::check_unlinked_path(&path, false).map_err(err)?;
    let current_file = File::open(&path).map_err(err)?;
    let current_metadata = current_file.metadata().map_err(err)?;
    let current = same_file::Handle::from_file(current_file).map_err(err)?;
    if identity != current
        || !same_stamp(&before, &after)?
        || !same_stamp(&before, &current_metadata)?
        || data.len() != after.len() as usize
    {
        return Err(fail("Preview input changed while reading"));
    }
    Ok((path, data))
}
fn same_stamp(a: &fs::Metadata, b: &fs::Metadata) -> Result<bool> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if a.dev() != b.dev()
            || a.ino() != b.ino()
            || (a.ctime(), a.ctime_nsec()) != (b.ctime(), b.ctime_nsec())
        {
            return Ok(false);
        }
    }
    Ok(a.len() == b.len()
        && a.modified().map_err(err)? == b.modified().map_err(err)?
        && a.created().ok() == b.created().ok())
}
#[derive(Clone)]
struct Accessor {
    data: Vec<f64>,
    count: usize,
    width: usize,
    integer: bool,
}
struct Document<'a> {
    path: PathBuf,
    doc: Value,
    buffers: Vec<Vec<u8>>,
    dependencies: BTreeMap<String, String>,
    total: usize,
    decoded: usize,
    cache: BTreeMap<usize, Arc<Accessor>>,
    cancel: &'a CancellationToken,
}
impl<'a> Document<'a> {
    fn load(path: &Path, cancel: &'a CancellationToken) -> Result<Self> {
        let (path, raw) = bounded_file(path, MAX_INPUT, cancel)?;
        let suffix = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if suffix != "gltf" && suffix != "glb" {
            return Err(fail("Select a GLB or glTF preview source"));
        }
        let mut binary = None;
        let source_json;
        if suffix == "glb" {
            if raw.len() < 20
                || &raw[..4] != b"glTF"
                || u32at(&raw, 4)? != 2
                || u32at(&raw, 8)? as usize != raw.len()
            {
                return Err(fail("Invalid complete GLB version 2 header"));
            }
            let mut pos = 12;
            let mut found_json = None;
            while pos < raw.len() {
                cancel.check()?;
                let size = u32at(&raw, pos)? as usize;
                let kind = u32at(&raw, pos + 4)?;
                pos += 8;
                let end = pos
                    .checked_add(size)
                    .filter(|&x| x <= raw.len())
                    .ok_or_else(|| fail("Truncated GLB chunk"))?;
                if size % 4 != 0 {
                    return Err(fail("GLB chunk must be aligned"));
                }
                if pos == 20 && kind != 0x4e4f534a {
                    return Err(fail("GLB must start with JSON"));
                }
                match kind {
                    0x4e4f534a => {
                        if found_json.is_some() {
                            return Err(fail("Duplicate GLB JSON"));
                        }
                        found_json = Some(&raw[pos..end]);
                    }
                    0x004e4942 => {
                        if binary.is_some() {
                            return Err(fail("Duplicate GLB binary"));
                        }
                        binary = Some(raw[pos..end].to_vec());
                    }
                    _ => {}
                }
                pos = end;
            }
            source_json = found_json.ok_or_else(|| fail("Missing GLB JSON"))?;
        } else {
            source_json = &raw;
        }
        if source_json.len() > MAX_JSON {
            return Err(fail("Preview JSON exceeds 32 MiB"));
        }
        let doc: Value = serde_json::from_slice(source_json)?;
        obj(&doc)?;
        if doc
            .get("asset")
            .and_then(|x| x.get("version"))
            .and_then(Value::as_str)
            != Some("2.0")
        {
            return Err(fail("Preview requires glTF 2.0"));
        }
        for extension in array(&doc, "extensionsRequired")? {
            if extension.as_str() != Some("KHR_mesh_quantization") {
                return Err(fail("Unsupported required glTF extension"));
            }
        }
        let dependencies = BTreeMap::from([(path.to_string_lossy().into_owned(), sha(&raw))]);
        let mut d = Self {
            path,
            doc,
            buffers: vec![],
            dependencies,
            total: raw.len(),
            decoded: 0,
            cache: BTreeMap::new(),
            cancel,
        };
        for (i, entry) in array(&d.doc, "buffers")?.to_vec().iter().enumerate() {
            obj(entry)?;
            let value = if let Some(uri) = entry.get("uri") {
                d.uri(uri, MAX_INPUT)?
            } else if i == 0 {
                binary
                    .take()
                    .ok_or_else(|| fail("Missing glTF binary buffer"))?
            } else {
                return Err(fail("Missing glTF buffer URI"));
            };
            let length = field(entry, "byteLength")?;
            if length > value.len() {
                return Err(fail("Truncated glTF buffer"));
            }
            d.buffers.push(value[..length].to_vec());
        }
        // All streams are validated, including currently inactive streams which
        // a loader may prefetch. Cached accessors avoid repeated large copies.
        for i in 0..array(&d.doc, "bufferViews")?.len() {
            d.view(i)?;
        }
        for i in 0..array(&d.doc, "accessors")?.len() {
            d.accessor(i)?;
        }
        Ok(d)
    }
    fn uri(&mut self, uri: &Value, limit: usize) -> Result<Vec<u8>> {
        self.cancel.check()?;
        let uri = uri.as_str().ok_or_else(|| fail("glTF URI must be text"))?;
        let data = if uri.starts_with("data:") {
            let (head, body) = uri
                .split_once(',')
                .ok_or_else(|| fail("Malformed embedded glTF URI"))?;
            if !head.ends_with(";base64") || body.len() > limit * 4 / 3 + 8 {
                return Err(fail("Unsupported embedded glTF encoding or size"));
            }
            STANDARD.decode(body).map_err(err)?
        } else {
            let relative = percent_encoding::percent_decode_str(uri)
                .decode_utf8()
                .map_err(err)?;
            if relative.is_empty()
                || relative.starts_with('/')
                || relative.contains(['\\', ':', '\0', '?', '#'])
                || relative.split('/').any(|s| s == "..")
            {
                return Err(fail("Remote or escaping glTF URI refused"));
            }
            let path = self.path.parent().unwrap().join(relative.as_ref());
            let (path, data) = bounded_file(&path, limit, self.cancel)?;
            if !path.starts_with(self.path.parent().unwrap()) {
                return Err(fail("glTF resource escapes source directory"));
            }
            let key = path.to_string_lossy().into_owned();
            let hash = sha(&data);
            if self.dependencies.get(&key).is_some_and(|old| old != &hash) {
                return Err(fail("Preview dependency changed between resource reads"));
            }
            self.dependencies.insert(key, hash);
            data
        };
        self.total = self
            .total
            .checked_add(data.len())
            .ok_or_else(|| fail("glTF input overflow"))?;
        if data.len() > limit || self.total > 2 * MAX_INPUT {
            return Err(fail("glTF resources exceed input budget"));
        }
        Ok(data)
    }
    fn view(&self, index: usize) -> Result<(&[u8], &Value)> {
        let v = at(&self.doc, "bufferViews", index)?;
        let b = self
            .buffers
            .get(field(v, "buffer")?)
            .ok_or_else(|| fail("Invalid glTF buffer reference"))?;
        let offset = defint(v, "byteOffset", 0)?;
        let size = field(v, "byteLength")?;
        let end = offset
            .checked_add(size)
            .ok_or_else(|| fail("glTF view overflow"))?;
        Ok((
            b.get(offset..end)
                .ok_or_else(|| fail("Truncated glTF buffer view"))?,
            v,
        ))
    }
    fn accessor(&mut self, index: usize) -> Result<Arc<Accessor>> {
        self.cancel.check()?;
        if let Some(a) = self.cache.get(&index) {
            return Ok(a.clone());
        }
        let a = at(&self.doc, "accessors", index)?.clone();
        let ct = field(&a, "componentType")?;
        let size = component_size(ct)?;
        let kind = a
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| fail("Missing glTF accessor type"))?;
        let width = match kind {
            "SCALAR" => 1,
            "VEC2" => 2,
            "VEC3" => 3,
            "VEC4" | "MAT2" => 4,
            "MAT3" => 9,
            "MAT4" => 16,
            _ => return Err(fail("Unsupported glTF accessor type")),
        };
        if kind.starts_with("MAT") && size < 4 {
            return Err(fail("Preview requires 32-bit matrix accessors"));
        }
        let count = field(&a, "count")?;
        let offset = defint(&a, "byteOffset", 0)?;
        let values = count
            .checked_mul(width)
            .ok_or_else(|| fail("glTF accessor overflow"))?;
        let bytes = values
            .checked_mul(size)
            .ok_or_else(|| fail("glTF accessor overflow"))?;
        self.decoded = self
            .decoded
            .checked_add(
                values
                    .checked_mul(8)
                    .ok_or_else(|| fail("glTF allocation overflow"))?,
            )
            .ok_or_else(|| fail("glTF allocation overflow"))?;
        if count > MAX_VERTICES * 4 || bytes > MAX_INPUT || self.decoded > MAX_INPUT * 2 {
            return Err(fail("Decoded glTF streams exceed memory budget"));
        }
        let normalized = match a.get("normalized") {
            None => false,
            Some(Value::Bool(b)) => *b,
            _ => return Err(fail("Invalid normalized flag")),
        };
        if normalized && ct == 5126 {
            return Err(fail("Floating glTF accessor cannot be normalized"));
        }
        let mut data = vec![0.; values];
        if let Some(view) = a.get("bufferView") {
            let (raw, v) = self.view(int(view)?)?;
            let stride = defint(v, "byteStride", width * size)?;
            if stride < width * size || stride % size != 0 || offset % size != 0 {
                return Err(fail("Invalid glTF accessor stride/alignment"));
            }
            let end = if count == 0 {
                offset
            } else {
                offset
                    .checked_add(
                        (count - 1)
                            .checked_mul(stride)
                            .ok_or_else(|| fail("glTF stride overflow"))?,
                    )
                    .and_then(|n| n.checked_add(width * size))
                    .ok_or_else(|| fail("glTF accessor overflow"))?
            };
            if end > raw.len() {
                return Err(fail("Truncated glTF accessor"));
            }
            for row in 0..count {
                if row % 65536 == 0 {
                    self.cancel.check()?;
                }
                for col in 0..width {
                    data[row * width + col] = number(raw, offset + row * stride + col * size, ct)?;
                }
            }
        }
        if let Some(s) = a.get("sparse") {
            let n = field(s, "count")?;
            if n > count {
                return Err(fail("Oversized sparse glTF accessor"));
            }
            let indices = s
                .get("indices")
                .ok_or_else(|| fail("Missing sparse indices"))?;
            let ict = field(indices, "componentType")?;
            if ![5121, 5123, 5125].contains(&ict) {
                return Err(fail("Invalid sparse index type"));
            }
            let isize = component_size(ict)?;
            let (iraw, _) = self.view(field(indices, "bufferView")?)?;
            let io = defint(indices, "byteOffset", 0)?;
            let values = s
                .get("values")
                .ok_or_else(|| fail("Missing sparse values"))?;
            let (vraw, _) = self.view(field(values, "bufferView")?)?;
            let vo = defint(values, "byteOffset", 0)?;
            if io.checked_add(n * isize).is_none_or(|e| e > iraw.len())
                || vo
                    .checked_add(n * width * size)
                    .is_none_or(|e| e > vraw.len())
            {
                return Err(fail("Truncated sparse glTF accessor"));
            }
            let mut previous = None;
            for row in 0..n {
                if row % 65536 == 0 {
                    self.cancel.check()?;
                }
                let index = number(iraw, io + row * isize, ict)? as usize;
                if index >= count || previous.is_some_and(|p| index <= p) {
                    return Err(fail("Sparse indices must be unique and increasing"));
                }
                previous = Some(index);
                for col in 0..width {
                    data[index * width + col] = number(vraw, vo + (row * width + col) * size, ct)?;
                }
            }
        }
        if normalized {
            let max = match ct {
                5120 => 127.,
                5121 => 255.,
                5122 => 32767.,
                5123 => 65535.,
                5125 => 4294967295.,
                _ => unreachable!(),
            };
            for v in &mut data {
                *v = (*v / max).max(-1.);
            }
        }
        if data.iter().any(|v| !v.is_finite()) {
            return Err(fail("Non-finite glTF accessor value"));
        }
        // GLTFLoader uses declared min/max without checking the stream. Derive
        // those bounds from verified values so forged metadata cannot poison
        // camera framing or morph bounds. glTF min/max are pre-normalization.
        let record = self.doc["accessors"][index].as_object_mut().unwrap();
        record.remove("min");
        record.remove("max");
        if count > 0 && !kind.starts_with("MAT") {
            let scale = if normalized {
                match ct {
                    5120 => 127.,
                    5121 => 255.,
                    5122 => 32767.,
                    5123 => 65535.,
                    5125 => 4294967295.,
                    _ => 1.,
                }
            } else {
                1.
            };
            let mut lo = vec![f64::INFINITY; width];
            let mut hi = vec![f64::NEG_INFINITY; width];
            for row in data.chunks_exact(width) {
                for col in 0..width {
                    let v = row[col] * scale;
                    lo[col] = lo[col].min(v);
                    hi[col] = hi[col].max(v);
                }
            }
            record.insert("min".into(), json!(lo));
            record.insert("max".into(), json!(hi));
        }
        let a = Arc::new(Accessor {
            data,
            count,
            width,
            integer: ct != 5126 && !normalized,
        });
        self.cache.insert(index, a.clone());
        Ok(a)
    }
    fn attr(
        &mut self,
        attrs: &Value,
        name: &str,
        count: usize,
        width: usize,
    ) -> Result<Option<Arc<Accessor>>> {
        attrs
            .get(name)
            .map(|v| {
                let a = self.accessor(int(v)?)?;
                if a.count != count || a.width != width {
                    return Err(fail(format!("glTF {name} stream shape mismatch")));
                }
                Ok(a)
            })
            .transpose()
    }
}
fn u32at(raw: &[u8], at: usize) -> Result<u32> {
    let b = raw
        .get(at..at + 4)
        .ok_or_else(|| fail("Truncated GLB header"))?;
    Ok(u32::from_le_bytes(b.try_into().unwrap()))
}
fn component_size(ct: usize) -> Result<usize> {
    match ct {
        5120 | 5121 => Ok(1),
        5122 | 5123 => Ok(2),
        5125 | 5126 => Ok(4),
        _ => Err(fail("Unsupported glTF component type")),
    }
}
fn number(raw: &[u8], at: usize, ct: usize) -> Result<f64> {
    let size = component_size(ct)?;
    let b = raw
        .get(at..at + size)
        .ok_or_else(|| fail("Truncated glTF number"))?;
    Ok(match ct {
        5120 => b[0] as i8 as f64,
        5121 => b[0] as f64,
        5122 => i16::from_le_bytes(b.try_into().unwrap()) as f64,
        5123 => u16::from_le_bytes(b.try_into().unwrap()) as f64,
        5125 => u32::from_le_bytes(b.try_into().unwrap()) as f64,
        5126 => f32::from_le_bytes(b.try_into().unwrap()) as f64,
        _ => unreachable!(),
    })
}
#[derive(Debug, Serialize)]
pub struct AnimationInfo {
    pub name: String,
    pub duration: f64,
}
#[derive(Debug, Serialize)]
pub struct Metadata {
    pub schema_version: u32,
    pub source: String,
    pub source_sha256: String,
    pub dependencies: BTreeMap<String, String>,
    pub warnings: Vec<String>,
    pub up_axis: String,
    pub mesh_count: usize,
    pub vertex_count: usize,
    pub triangle_count: usize,
    pub bone_count: usize,
    pub animations: Vec<AnimationInfo>,
}
struct Prepared {
    metadata: Metadata,
    glb: Vec<u8>,
}
impl Document<'_> {
    fn validate(&mut self) -> Result<Metadata> {
        let nodes = array(&self.doc, "nodes")?.to_vec();
        if nodes.len() > MAX_NODES {
            return Err(fail("Preview has too many nodes"));
        }
        let mut parents = vec![None; nodes.len()];
        let mut locals = Vec::new();
        for (i, n) in nodes.iter().enumerate() {
            self.cancel.check()?;
            obj(n)?;
            for child in array(n, "children")? {
                let child = int(child)?;
                if child >= nodes.len() || parents[child].replace(i).is_some() {
                    return Err(fail("glTF node has invalid or multiple parents"));
                }
            }
            if let Some(w) = n.get("weights") {
                nums(w, None)?;
            }
            let t = default_nums(n, "translation", &[0., 0., 0.])?;
            let mut q = default_nums(n, "rotation", &[0., 0., 0., 1.])?;
            let s = default_nums(n, "scale", &[1., 1., 1.])?;
            let norm = q.iter().map(|x| x * x).sum::<f64>().sqrt();
            if !norm.is_finite() || norm < 1e-12 {
                return Err(fail("glTF rotation quaternion has zero or invalid length"));
            }
            for v in &mut q {
                *v /= norm;
            }
            if n.get("rotation").is_some() {
                self.doc["nodes"][i]["rotation"] = json!(q);
            }
            let m = if let Some(m) = n.get("matrix") {
                if ["translation", "rotation", "scale"]
                    .iter()
                    .any(|k| n.get(k).is_some())
                {
                    return Err(fail("glTF matrix node mixes TRS fields"));
                }
                let v = nums(m, Some(16))?;
                if !affine(&v, 1e-9) {
                    return Err(fail("glTF node matrix is not affine"));
                }
                let mut m = crate::math::identity();
                for r in 0..4 {
                    for c in 0..4 {
                        m[r][c] = v[c * 4 + r];
                    }
                }
                m
            } else {
                let [x, y, z, w]: [f64; 4] = q.try_into().unwrap();
                let mut m = crate::math::identity();
                m[0][0] = (1. - 2. * (y * y + z * z)) * s[0];
                m[0][1] = 2. * (x * y - z * w) * s[1];
                m[0][2] = 2. * (x * z + y * w) * s[2];
                m[1][0] = 2. * (x * y + z * w) * s[0];
                m[1][1] = (1. - 2. * (x * x + z * z)) * s[1];
                m[1][2] = 2. * (y * z - x * w) * s[2];
                m[2][0] = 2. * (x * z - y * w) * s[0];
                m[2][1] = 2. * (y * z + x * w) * s[1];
                m[2][2] = (1. - 2. * (x * x + y * y)) * s[2];
                for r in 0..3 {
                    m[r][3] = t[r];
                }
                m
            };
            locals.push(m);
        }
        let mut world: Vec<Option<crate::math::Matrix>> = vec![None; nodes.len()];
        for start in 0..nodes.len() {
            let mut chain = Vec::new();
            let mut seen = BTreeSet::new();
            let mut current = Some(start);
            while let Some(i) = current {
                if world[i].is_some() {
                    break;
                }
                if !seen.insert(i) {
                    return Err(fail("glTF node hierarchy cycle"));
                }
                chain.push(i);
                current = parents[i];
            }
            for i in chain.into_iter().rev() {
                let m = parents[i]
                    .map(|p| crate::math::mul(&world[p].unwrap(), &locals[i]))
                    .unwrap_or(locals[i]);
                if m.iter().flatten().any(|v| !v.is_finite()) {
                    return Err(fail("glTF world transform overflow"));
                }
                world[i] = Some(m);
            }
        }
        let scenes = array(&self.doc, "scenes")?;
        let roots = if scenes.is_empty() {
            parents
                .iter()
                .enumerate()
                .filter_map(|(i, p)| p.is_none().then_some(json!(i)))
                .collect::<Vec<_>>()
        } else {
            array(
                scenes
                    .get(defint(&self.doc, "scene", 0)?)
                    .ok_or_else(|| fail("Invalid glTF default scene"))?,
                "nodes",
            )?
            .to_vec()
        };
        let mut active = BTreeSet::new();
        let mut stack = roots.clone();
        while let Some(v) = stack.pop() {
            let i = int(&v)?;
            if i >= nodes.len() {
                return Err(fail("Invalid glTF scene root"));
            }
            if active.insert(i) {
                stack.extend(array(&nodes[i], "children")?.iter().cloned());
            }
        }
        self.doc["scenes"] = json!([{"nodes":roots}]);
        self.doc["scene"] = json!(0);
        let mut skins = Vec::new();
        let mut skin_matrices = Vec::new();
        let mut bones = BTreeSet::new();
        for skin in array(&self.doc, "skins")?.to_vec() {
            let joints = array(&skin, "joints")?;
            let mut unique = BTreeSet::new();
            if joints.is_empty() {
                return Err(fail("glTF skin has no joints"));
            }
            for j in joints {
                let j = int(j)?;
                if j >= nodes.len() || !unique.insert(j) {
                    return Err(fail("Invalid or repeated glTF skin joint"));
                }
                bones.insert(j);
            }
            if let Some(skeleton) = skin.get("skeleton") {
                if int(skeleton)? >= nodes.len() {
                    return Err(fail("Invalid glTF skeleton reference"));
                }
            }
            let inverse = skin
                .get("inverseBindMatrices")
                .map(|v| self.accessor(int(v)?))
                .transpose()?;
            if let Some(a) = &inverse {
                if a.width != 16
                    || a.count != joints.len()
                    || a.data.chunks_exact(16).any(|v| !affine(v, 1e-6))
                {
                    return Err(fail("Invalid glTF inverse bind matrices"));
                }
            }
            let mut matrices = Vec::new();
            for (i, j) in joints.iter().enumerate() {
                let mut ibm = crate::math::identity();
                if let Some(a) = &inverse {
                    for r in 0..4 {
                        for c in 0..4 {
                            ibm[r][c] = a.data[i * 16 + c * 4 + r];
                        }
                    }
                }
                let m = crate::math::mul(&world[int(j)?].unwrap(), &ibm);
                if m.iter().flatten().any(|v| !v.is_finite()) {
                    return Err(fail("Preview skin transform overflow"));
                }
                matrices.push(m);
            }
            skin_matrices.push(matrices);
            skins.push(joints.len());
        }
        let materials = array(&self.doc, "materials")?.to_vec();
        let mut textured = Vec::new();
        for (i, material) in materials.iter().enumerate() {
            obj(material)?;
            let p = material
                .get("pbrMetallicRoughness")
                .cloned()
                .unwrap_or(json!({}));
            obj(&p)?;
            let color = default_nums(&p, "baseColorFactor", &[1., 1., 1., 1.])?;
            let metallic = p
                .get("metallicFactor")
                .map(|v| v.as_f64().ok_or_else(|| fail("Invalid metallic factor")))
                .transpose()?
                .unwrap_or(1.);
            let rough = p
                .get("roughnessFactor")
                .map(|v| v.as_f64().ok_or_else(|| fail("Invalid roughness factor")))
                .transpose()?
                .unwrap_or(1.);
            let cutoff = material
                .get("alphaCutoff")
                .map(|v| v.as_f64().ok_or_else(|| fail("Invalid alpha cutoff")))
                .transpose()?
                .unwrap_or(0.5);
            let mode = material
                .get("alphaMode")
                .and_then(Value::as_str)
                .unwrap_or("OPAQUE");
            if color.iter().any(|v| !(0.0..=1.).contains(v))
                || !(0.0..=1.).contains(&metallic)
                || !(0.0..=1.).contains(&rough)
                || !cutoff.is_finite()
                || cutoff < 0.
                || !["OPAQUE", "MASK", "BLEND"].contains(&mode)
            {
                return Err(fail("glTF material factors are outside supported range"));
            }
            let is_textured = if let Some(info) = p.get("baseColorTexture") {
                if defint(info, "texCoord", 0)? != 0
                    || info
                        .get("extensions")
                        .is_some_and(|v| v.as_object().is_none_or(|o| !o.is_empty()))
                {
                    return Err(fail(
                        "glTF base color texture requires UV0 without transforms",
                    ));
                }
                let texture = at(&self.doc, "textures", field(info, "index")?)?;
                at(&self.doc, "images", field(texture, "source")?)?;
                true
            } else {
                false
            };
            textured.push(is_textured);
            // Match the legacy preview material subset. Unsupported optional
            // texture/shader extensions must not cause browser network requests.
            let m = self.doc["materials"][i].as_object_mut().unwrap();
            m.retain(|k, _| {
                [
                    "name",
                    "pbrMetallicRoughness",
                    "alphaMode",
                    "alphaCutoff",
                    "doubleSided",
                    "extras",
                ]
                .contains(&k.as_str())
            });
            if let Some(p) = m
                .get_mut("pbrMetallicRoughness")
                .and_then(Value::as_object_mut)
            {
                p.retain(|k, _| {
                    [
                        "baseColorFactor",
                        "baseColorTexture",
                        "metallicFactor",
                        "roughnessFactor",
                    ]
                    .contains(&k.as_str())
                });
            }
        }
        for texture in array(&self.doc, "textures")? {
            at(&self.doc, "images", field(texture, "source")?)?;
            if let Some(v) = texture.get("sampler") {
                at(&self.doc, "samplers", int(v)?)?;
            }
        }
        for sampler in array(&self.doc, "samplers")? {
            for (key, allowed) in [
                ("magFilter", &[9728usize, 9729][..]),
                ("minFilter", &[9728, 9729, 9984, 9985, 9986, 9987][..]),
                ("wrapS", &[33071, 33648, 10497][..]),
                ("wrapT", &[33071, 33648, 10497][..]),
            ] {
                if let Some(v) = sampler.get(key) {
                    if !allowed.contains(&int(v)?) {
                        return Err(fail("Unsupported glTF texture sampler"));
                    }
                }
            }
        }
        let mut meshes = 0;
        let mut vertices = 0usize;
        let mut triangles = 0usize;
        let mut morph_width = BTreeMap::new();
        let mut render_bytes = 0usize;
        // Validate every mesh instance including inactive meshes; only selected
        // instances contribute to the displayed catalog and counts.
        let all_meshes = array(&self.doc, "meshes")?.to_vec();
        for (ni, node) in nodes.iter().enumerate() {
            self.cancel.check()?;
            let Some(mi) = node.get("mesh") else { continue };
            let mesh = all_meshes
                .get(int(mi)?)
                .ok_or_else(|| fail("Invalid glTF mesh reference"))?;
            let skin = node.get("skin").map(int).transpose()?;
            let joints = skin
                .map(|i| {
                    skins
                        .get(i)
                        .copied()
                        .ok_or_else(|| fail("Invalid glTF skin reference"))
                })
                .transpose()?;
            for primitive in array(mesh, "primitives")? {
                let attrs = primitive
                    .get("attributes")
                    .ok_or_else(|| fail("Missing glTF attributes"))?;
                obj(attrs)?;
                let position = self.accessor(field(attrs, "POSITION")?)?;
                let count = position.count;
                if position.width != 3 || count == 0 {
                    return Err(fail("glTF POSITION must be nonempty VEC3"));
                }
                if position.data.iter().any(|v| v.abs() > f32::MAX as f64) {
                    return Err(fail("Preview positions exceed float32 range"));
                }
                let icount = if let Some(i) = primitive.get("indices") {
                    let a = self.accessor(int(i)?)?;
                    if a.width != 1
                        || !a.integer
                        || a.data.iter().any(|&v| v < 0. || v >= count as f64)
                    {
                        return Err(fail("Invalid glTF triangle indices"));
                    }
                    a.count
                } else {
                    count
                };
                if icount < 3 {
                    return Err(fail("glTF primitive contains no triangle"));
                }
                let mode = defint(primitive, "mode", 4)?;
                let tcount = match mode {
                    4 if icount % 3 == 0 => icount / 3,
                    5 | 6 => icount - 2,
                    _ => {
                        return Err(fail(
                            "Preview supports triangle, strip and fan primitives only",
                        ))
                    }
                };
                self.attr(attrs, "NORMAL", count, 3)?;
                let uv = self.attr(attrs, "TEXCOORD_0", count, 2)?;
                if let Some(v) = attrs.get("COLOR_0") {
                    let a = self.accessor(int(v)?)?;
                    if a.count != count || ![3, 4].contains(&a.width) {
                        return Err(fail("Invalid glTF vertex colors"));
                    }
                }
                let mut influences = Vec::new();
                if let Some(joint_count) = joints {
                    let map = obj(attrs)?;
                    let sets = map
                        .keys()
                        .filter(|k| k.starts_with("JOINTS_"))
                        .map(|k| k[7..].parse::<usize>().map_err(err))
                        .collect::<Result<BTreeSet<_>>>()?;
                    let weights = map
                        .keys()
                        .filter(|k| k.starts_with("WEIGHTS_"))
                        .cloned()
                        .collect::<BTreeSet<_>>();
                    let joint_keys = map
                        .keys()
                        .filter(|k| k.starts_with("JOINTS_"))
                        .cloned()
                        .collect::<BTreeSet<_>>();
                    if sets.is_empty()
                        || sets.len() > 8
                        || sets.iter().copied().ne(0..sets.len())
                        || weights != (0..sets.len()).map(|i| format!("WEIGHTS_{i}")).collect()
                        || joint_keys != (0..sets.len()).map(|i| format!("JOINTS_{i}")).collect()
                    {
                        return Err(fail("Skin requires consecutive matching joint/weight sets, up to 32 influences"));
                    }
                    let mut sums = vec![0.; count];
                    for set in sets {
                        let ja = self
                            .attr(attrs, &format!("JOINTS_{set}"), count, 4)?
                            .unwrap();
                        let wa = self
                            .attr(attrs, &format!("WEIGHTS_{set}"), count, 4)?
                            .unwrap();
                        if !ja.integer
                            || ja.data.iter().any(|&v| v < 0. || v >= joint_count as f64)
                            || wa.data.iter().any(|&v| v < 0.)
                        {
                            return Err(fail("Invalid glTF skin joints or weights"));
                        }
                        for (sum, w) in sums.iter_mut().zip(wa.data.chunks_exact(4)) {
                            *sum += w.iter().sum::<f64>();
                        }
                        influences.push((ja, wa));
                    }
                    if sums.iter().any(|v| (*v - 1.).abs() > 0.00201) {
                        return Err(fail("glTF skin weights must sum to one across all sets"));
                    }
                }
                let targets = array(primitive, "targets")?;
                if targets.len() > 256 {
                    return Err(fail("Too many glTF morph targets"));
                }
                if let Some(names) = mesh.get("extras").and_then(|v| v.get("targetNames")) {
                    let names = names
                        .as_array()
                        .ok_or_else(|| fail("Invalid morph names"))?;
                    let names = names
                        .iter()
                        .map(|v| v.as_str().ok_or_else(|| fail("Morph name must be text")))
                        .collect::<Result<Vec<_>>>()?;
                    if names.len() != targets.len()
                        || names.iter().collect::<BTreeSet<_>>().len() != names.len()
                    {
                        return Err(fail("Morph names are duplicated or mismatched"));
                    }
                }
                for target in targets {
                    obj(target)?;
                    self.attr(target, "POSITION", count, 3)?;
                    self.attr(target, "NORMAL", count, 3)?;
                }
                for weight in [mesh.get("weights"), node.get("weights")]
                    .into_iter()
                    .flatten()
                {
                    nums(weight, Some(targets.len()))?;
                }
                if let Some(m) = primitive.get("material") {
                    let m = int(m)?;
                    if *textured
                        .get(m)
                        .ok_or_else(|| fail("Invalid glTF material reference"))?
                        && uv.is_none()
                    {
                        return Err(fail("Textured glTF mesh has no UV0"));
                    }
                }
                if active.contains(&ni) {
                    // A renderer expands morph textures even when many targets
                    // share one cached accessor. Bound the expanded allocation.
                    let morph_streams =
                        1 + usize::from(targets.iter().any(|t| t.get("NORMAL").is_some()));
                    let floats = 12usize
                        .checked_add(targets.len() * 3 * morph_streams)
                        .and_then(|n| n.checked_add(influences.len() * 8))
                        .ok_or_else(|| fail("Preview render allocation overflow"))?;
                    render_bytes = render_bytes
                        .checked_add(
                            count
                                .checked_mul(floats)
                                .and_then(|n| n.checked_mul(4))
                                .ok_or_else(|| fail("Preview render allocation overflow"))?,
                        )
                        .ok_or_else(|| fail("Preview render allocation overflow"))?;
                    if render_bytes > MAX_INPUT * 2 {
                        return Err(fail("Expanded preview geometry and morphs exceed 1 GiB"));
                    }
                    let default_weights = node
                        .get("weights")
                        .or_else(|| mesh.get("weights"))
                        .map(|v| nums(v, Some(targets.len())))
                        .transpose()?
                        .unwrap_or_else(|| vec![0.; targets.len()]);
                    let morphs = targets
                        .iter()
                        .map(|t| {
                            t.get("POSITION")
                                .map(|v| self.accessor(int(v)?))
                                .transpose()
                        })
                        .collect::<Result<Vec<_>>>()?;
                    for vertex in 0..count {
                        if vertex % 65536 == 0 {
                            self.cancel.check()?;
                        }
                        let mut p = [
                            position.data[vertex * 3],
                            position.data[vertex * 3 + 1],
                            position.data[vertex * 3 + 2],
                        ];
                        for (m, w) in morphs.iter().zip(&default_weights) {
                            if let Some(m) = m {
                                for r in 0..3 {
                                    p[r] += m.data[vertex * 3 + r] * w;
                                }
                            }
                        }
                        let posed = if let Some(si) = skin {
                            let mut result = [0.; 3];
                            for (ja, wa) in &influences {
                                for k in 0..4 {
                                    let j = ja.data[vertex * 4 + k] as usize;
                                    let weight = wa.data[vertex * 4 + k];
                                    let q = crate::math::transform(&skin_matrices[si][j], p);
                                    for r in 0..3 {
                                        result[r] += q[r] * weight;
                                    }
                                }
                            }
                            result
                        } else {
                            crate::math::transform(&world[ni].unwrap(), p)
                        };
                        if posed
                            .iter()
                            .any(|v| !v.is_finite() || v.abs() > f32::MAX as f64)
                        {
                            return Err(fail("Preview rest pose exceeds finite float32 range"));
                        }
                    }
                    if let Some(previous) = morph_width.insert(ni, targets.len()) {
                        if previous != targets.len() {
                            return Err(fail("Mesh primitive morph counts differ"));
                        }
                    }
                    meshes += 1;
                    vertices = vertices
                        .checked_add(count)
                        .ok_or_else(|| fail("Vertex budget overflow"))?;
                    triangles = triangles
                        .checked_add(tcount)
                        .ok_or_else(|| fail("Triangle budget overflow"))?;
                    if vertices > MAX_VERTICES || triangles > MAX_TRIANGLES {
                        return Err(fail(
                            "Preview exceeds six million vertices or four million triangles",
                        ));
                    }
                }
            }
        }
        if meshes == 0 {
            return Err(fail("Selected glTF scene contains no triangle meshes"));
        }
        let mut animations = Vec::new();
        let mut names = BTreeSet::new();
        for (i, animation) in array(&self.doc, "animations")?.to_vec().iter().enumerate() {
            let channels = array(animation, "channels")?;
            if channels.len() > MAX_NODES * 4 {
                return Err(fail("Too many glTF animation channels"));
            }
            let mut used = BTreeSet::new();
            let mut duration = 0f64;
            for channel in channels {
                let target = channel
                    .get("target")
                    .ok_or_else(|| fail("Missing animation target"))?;
                let node = field(target, "node")?;
                let n = nodes
                    .get(node)
                    .ok_or_else(|| fail("Invalid animation node"))?;
                let path = target
                    .get("path")
                    .and_then(Value::as_str)
                    .ok_or_else(|| fail("Invalid animation path"))?;
                if !["translation", "rotation", "scale", "weights"].contains(&path)
                    || !used.insert((node, path))
                    || (n.get("matrix").is_some() && path != "weights")
                {
                    return Err(fail(
                        "Unsupported, repeated or matrix glTF animation target",
                    ));
                }
                let sampler = at(animation, "samplers", field(channel, "sampler")?)?;
                let times = self.accessor(field(sampler, "input")?)?;
                if times.width != 1
                    || times.count == 0
                    || times.data[0] < 0.
                    || times.data.windows(2).any(|w| w[0] >= w[1])
                {
                    return Err(fail(
                        "glTF animation times must increase from a nonnegative time",
                    ));
                }
                duration = duration.max(*times.data.last().unwrap());
                let values = self.accessor(field(sampler, "output")?)?;
                let interpolation = sampler
                    .get("interpolation")
                    .and_then(Value::as_str)
                    .unwrap_or("LINEAR");
                let multiplier = match interpolation {
                    "LINEAR" | "STEP" => 1,
                    "CUBICSPLINE" => 3,
                    _ => return Err(fail("Unsupported animation interpolation")),
                };
                let width = match path {
                    "rotation" => 4,
                    "weights" => *morph_width
                        .get(&node)
                        .filter(|&&w| w > 0)
                        .ok_or_else(|| fail("Animation targets unloaded morph weights"))?,
                    _ => 3,
                };
                if values.data.len() != times.count * width * multiplier {
                    return Err(fail("glTF animation output count mismatch"));
                }
                if path == "rotation" {
                    for key in 0..times.count {
                        let offset = (key * multiplier + if multiplier == 3 { 1 } else { 0 }) * 4;
                        let norm = values.data[offset..offset + 4]
                            .iter()
                            .map(|v| v * v)
                            .sum::<f64>()
                            .sqrt();
                        if norm < 1e-12 || !norm.is_finite() {
                            return Err(fail(
                                "Animation contains a zero-length rotation quaternion",
                            ));
                        }
                    }
                }
            }
            let mut name = animation
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .unwrap_or_else(|| format!("Animation {}", i + 1));
            if !names.insert(name.clone()) {
                name = format!("{name} [{}]", i + 1);
                names.insert(name.clone());
            }
            self.doc["animations"][i]["name"] = json!(name);
            animations.push(AnimationInfo { name, duration });
        }
        // Preserve all ancestor and joint transforms, while removing meshes outside
        // the selected scene. This keeps animation on an unselected ancestor
        // consistent with the legacy scene's world-matrix evaluation.
        let scene_roots = parents
            .iter()
            .enumerate()
            .filter_map(|(i, p)| p.is_none().then_some(i))
            .collect::<Vec<_>>();
        self.doc["scenes"] = json!([{"nodes":scene_roots}]);
        for i in 0..nodes.len() {
            if !active.contains(&i) {
                let node = self.doc["nodes"][i].as_object_mut().unwrap();
                node.remove("mesh");
                node.remove("skin");
            }
        }
        let source = self.path.to_string_lossy().into_owned();
        Ok(Metadata{schema_version:1,source_sha256:self.dependencies[&source].clone(),source,dependencies:self.dependencies.clone(),warnings:vec!["Preview uses approximate lighting and materials. Deadlock shaders, procedural motion and cloth simulation are not reproduced.".into()],up_axis:"Y".into(),mesh_count:meshes,vertex_count:vertices,triangle_count:triangles,bone_count:bones.len(),animations})
    }
}
fn affine(v: &[f64], eps: f64) -> bool {
    v.len() == 16
        && v[3].abs() <= eps
        && v[7].abs() <= eps
        && v[11].abs() <= eps
        && (v[15] - 1.).abs() <= eps + 1e-5
}
impl Document<'_> {
    fn generated_accessor(&mut self, data: Vec<f64>, count: usize, width: usize) -> Result<usize> {
        self.decoded = self
            .decoded
            .checked_add(data.len() * 8)
            .ok_or_else(|| fail("Preview accessor budget overflow"))?;
        if self.decoded > MAX_INPUT * 2 {
            return Err(fail("Preview decoded streams exceed 1 GiB"));
        }
        let index = array(&self.doc, "accessors")?.len();
        let mut lo = vec![f64::INFINITY; width];
        let mut hi = vec![f64::NEG_INFINITY; width];
        for row in data.chunks_exact(width) {
            for c in 0..width {
                lo[c] = lo[c].min(row[c]);
                hi[c] = hi[c].max(row[c]);
            }
        }
        self.doc["accessors"].as_array_mut().unwrap().push(json!({"count":count,"type":match width{3=>"VEC3",4=>"VEC4",_=>"SCALAR"},"componentType":5126,"min":lo,"max":hi}));
        self.cache.insert(
            index,
            Arc::new(Accessor {
                data,
                count,
                width,
                integer: false,
            }),
        );
        Ok(index)
    }
    fn complete_streams(&mut self) -> Result<()> {
        let used = array(&self.doc, "nodes")?
            .iter()
            .filter_map(|n| n.get("mesh"))
            .map(int)
            .collect::<Result<BTreeSet<_>>>()?;
        let mut zeros = BTreeMap::new();
        for mesh_index in used {
            let mesh = at(&self.doc, "meshes", mesh_index)?.clone();
            for (part_index, part) in array(&mesh, "primitives")?.iter().enumerate() {
                self.cancel.check()?;
                let attrs = part
                    .get("attributes")
                    .ok_or_else(|| fail("Missing mesh attributes"))?;
                let positions = self.accessor(field(attrs, "POSITION")?)?;
                let count = positions.count;
                if attrs.get("NORMAL").is_none() {
                    let indices = part
                        .get("indices")
                        .map(|i| self.accessor(int(i)?))
                        .transpose()?;
                    let n = indices.as_ref().map(|a| a.count).unwrap_or(count);
                    let index =
                        |i: usize| indices.as_ref().map(|a| a.data[i] as usize).unwrap_or(i);
                    let mode = defint(part, "mode", 4)?;
                    let triangles = if mode == 4 { n / 3 } else { n - 2 };
                    let mut normals = vec![[0f64; 3]; count];
                    // Same accumulation order as Python's 65,536-triangle chunks.
                    for start in (0..triangles).step_by(65536) {
                        self.cancel.check()?;
                        let end = (start + 65536).min(triangles);
                        let mut faces = Vec::with_capacity(end - start);
                        for t in start..end {
                            let ids = match mode {
                                4 => [index(t * 3), index(t * 3 + 1), index(t * 3 + 2)],
                                5 => {
                                    if t % 2 == 0 {
                                        [index(t), index(t + 1), index(t + 2)]
                                    } else {
                                        [index(t + 1), index(t), index(t + 2)]
                                    }
                                }
                                6 => [index(0), index(t + 1), index(t + 2)],
                                _ => return Err(fail("Unsupported primitive mode")),
                            };
                            let point = |i: usize| {
                                [
                                    positions.data[i * 3],
                                    positions.data[i * 3 + 1],
                                    positions.data[i * 3 + 2],
                                ]
                            };
                            let face = crate::math::cross(
                                crate::math::sub(point(ids[1]), point(ids[0])),
                                crate::math::sub(point(ids[2]), point(ids[0])),
                            );
                            faces.push((ids, face));
                        }
                        for c in 0..3 {
                            for (ids, face) in &faces {
                                for r in 0..3 {
                                    normals[ids[c]][r] += face[r];
                                }
                            }
                        }
                    }
                    let values = normals
                        .into_iter()
                        .flat_map(|v| crate::math::normalize(v).map(|v| v as f32 as f64))
                        .collect();
                    let index = self.generated_accessor(values, count, 3)?;
                    self.doc["meshes"][mesh_index]["primitives"][part_index]["attributes"]
                        ["NORMAL"] = json!(index);
                }
                let targets = array(part, "targets")?;
                let has_normals = targets.iter().any(|t| t.get("NORMAL").is_some());
                for (target_index, target) in targets.iter().enumerate() {
                    for key in ["POSITION", "NORMAL"] {
                        if (key == "POSITION" || has_normals) && target.get(key).is_none() {
                            let zero = if let Some(&i) = zeros.get(&count) {
                                i
                            } else {
                                let i = self.generated_accessor(vec![0.; count * 3], count, 3)?;
                                zeros.insert(count, i);
                                i
                            };
                            self.doc["meshes"][mesh_index]["primitives"][part_index]["targets"]
                                [target_index][key] = json!(zero);
                        }
                    }
                }
            }
        }
        Ok(())
    }
    fn bundle(&mut self) -> Result<Vec<u8>> {
        self.complete_streams()?;
        let mut images = Vec::new();
        for image in array(&self.doc, "images")?.to_vec() {
            self.cancel.check()?;
            let bytes = if let Some(uri) = image.get("uri") {
                self.uri(uri, MAX_TEXTURE)?
            } else {
                self.view(field(&image, "bufferView")?)?.0.to_vec()
            };
            let (mime, pixels) = validate_image(&bytes, self.cancel)?;
            self.decoded = self
                .decoded
                .checked_add(pixels)
                .ok_or_else(|| fail("Preview texture allocation overflow"))?;
            if self.decoded > MAX_INPUT * 2 {
                return Err(fail("Preview decoded geometry and textures exceed 1 GiB"));
            }
            images.push((bytes, mime));
        }
        // Canonical, tightly packed accessors avoid loader-specific ambiguity in
        // sparse matrices, sparse+interleaved arrays and partial final strides.
        // The decoded numeric values and original component types are retained.
        let mut index_accessors = BTreeSet::new();
        for mesh in array(&self.doc, "meshes")? {
            for part in array(mesh, "primitives")? {
                if let Some(i) = part.get("indices") {
                    index_accessors.insert(int(i)?);
                }
            }
        }
        self.buffers.clear();
        let mut bin = Vec::new();
        let mut views = Vec::new();
        for (index, accessor) in std::mem::take(&mut self.cache) {
            self.cancel.check()?;
            let record = &mut self.doc["accessors"][index];
            let original_type = field(record, "componentType")?;
            let signed_index =
                index_accessors.contains(&index) && [5120, 5122].contains(&original_type);
            let ct = if signed_index { 5125 } else { original_type };
            if signed_index && accessor.data.iter().any(|v| *v < 0.) {
                return Err(fail("Negative triangle index"));
            }
            let normalized = record
                .get("normalized")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let scale = if normalized {
                match ct {
                    5120 => 127.,
                    5121 => 255.,
                    5122 => 32767.,
                    5123 => 65535.,
                    5125 => 4294967295.,
                    _ => return Err(fail("Invalid normalized float")),
                }
            } else {
                1.
            };
            align(&mut bin, 0);
            let offset = bin.len();
            let bytes = accessor
                .data
                .len()
                .checked_mul(component_size(ct)?)
                .ok_or_else(|| fail("Preview accessor size overflow"))?;
            if bin.len().checked_add(bytes).is_none_or(|n| n > MAX_PACKET) {
                return Err(fail("Preview binary exceeds 512 MiB"));
            }
            for (i, &value) in accessor.data.iter().enumerate() {
                if i % 65536 == 0 {
                    self.cancel.check()?;
                }
                let value = if normalized {
                    (value * scale).round()
                } else {
                    value
                };
                match ct {
                    5120 => bin.push(value as i8 as u8),
                    5121 => bin.push(value as u8),
                    5122 => bin.extend_from_slice(&(value as i16).to_le_bytes()),
                    5123 => bin.extend_from_slice(&(value as u16).to_le_bytes()),
                    5125 => bin.extend_from_slice(&(value as u32).to_le_bytes()),
                    5126 => bin.extend_from_slice(&(value as f32).to_le_bytes()),
                    _ => unreachable!(),
                }
            }
            let view = views.len();
            views.push(json!({"buffer":0,"byteOffset":offset,"byteLength":bytes}));
            let record = record.as_object_mut().unwrap();
            record.remove("sparse");
            record.insert("bufferView".into(), json!(view));
            record.insert("byteOffset".into(), json!(0));
            record.insert("componentType".into(), json!(ct));
        }
        for (i, (bytes, mime)) in images.into_iter().enumerate() {
            align(&mut bin, 0);
            let offset = bin.len();
            append(&mut bin, &bytes, self.cancel)?;
            let view = views.len();
            views.push(json!({"buffer":0,"byteOffset":offset,"byteLength":bytes.len()}));
            let image = self.doc["images"][i]
                .as_object_mut()
                .ok_or_else(|| fail("Invalid glTF image object"))?;
            image.remove("uri");
            image.insert("bufferView".into(), json!(view));
            image.insert("mimeType".into(), json!(mime));
        }
        self.doc["bufferViews"] = json!(views);
        self.doc["buffers"] = json!([{"byteLength":bin.len()}]);
        // Only the legacy material/attribute subset is exposed to the renderer.
        if let Some(meshes) = self.doc.get_mut("meshes").and_then(Value::as_array_mut) {
            for mesh in meshes {
                if let Some(parts) = mesh.get_mut("primitives").and_then(Value::as_array_mut) {
                    for part in parts {
                        if let Some(attrs) =
                            part.get_mut("attributes").and_then(Value::as_object_mut)
                        {
                            attrs.retain(|k, _| {
                                ["POSITION", "NORMAL", "TEXCOORD_0", "COLOR_0"]
                                    .contains(&k.as_str())
                                    || (0..8).any(|i| {
                                        k == &format!("JOINTS_{i}") || k == &format!("WEIGHTS_{i}")
                                    })
                            });
                        }
                        if let Some(targets) = part.get_mut("targets").and_then(Value::as_array_mut)
                        {
                            for target in targets {
                                if let Some(t) = target.as_object_mut() {
                                    t.retain(|k, _| k == "POSITION" || k == "NORMAL");
                                }
                            }
                        }
                    }
                }
            }
        }
        strip_extensions(&mut self.doc);
        for key in ["extensionsUsed", "extensionsRequired"] {
            if let Some(v) = self.doc.get_mut(key).and_then(Value::as_array_mut) {
                v.retain(|v| v.as_str() == Some("KHR_mesh_quantization"));
            }
        }
        if let Some(nodes) = self.doc.get_mut("nodes").and_then(Value::as_array_mut) {
            for n in nodes {
                obj(n)?;
                n.as_object_mut().unwrap().remove("camera");
            }
        }
        self.doc.as_object_mut().unwrap().remove("cameras");
        let mut json = serde_json::to_vec(&self.doc)?;
        if json.len() > MAX_JSON {
            return Err(fail("Bundled preview JSON exceeds 32 MiB"));
        }
        align(&mut json, b' ');
        align(&mut bin, 0);
        let length = 12usize
            .checked_add(8 + json.len())
            .and_then(|v| v.checked_add(8 + bin.len()))
            .filter(|&v| v < MAX_PACKET)
            .ok_or_else(|| fail("Bundled preview exceeds 512 MiB"))?;
        let mut output = Vec::with_capacity(length);
        output.extend_from_slice(b"glTF");
        output.extend_from_slice(&2u32.to_le_bytes());
        output.extend_from_slice(&(length as u32).to_le_bytes());
        output.extend_from_slice(&(json.len() as u32).to_le_bytes());
        output.extend_from_slice(&0x4e4f534au32.to_le_bytes());
        output.extend(json);
        output.extend_from_slice(&(bin.len() as u32).to_le_bytes());
        output.extend_from_slice(&0x004e4942u32.to_le_bytes());
        append(&mut output, &bin, self.cancel)?;
        Ok(output)
    }
    fn verify_dependencies(&self) -> Result<()> {
        let mut total = 0usize;
        for (path, hash) in &self.dependencies {
            let (_, bytes) = bounded_file(Path::new(path), MAX_INPUT, self.cancel)?;
            total = total
                .checked_add(bytes.len())
                .ok_or_else(|| fail("Dependency size overflow"))?;
            if total > 2 * MAX_INPUT || sha(&bytes) != *hash {
                return Err(fail("Preview dependency changed during validation"));
            }
        }
        Ok(())
    }
}
fn strip_extensions(value: &mut Value) {
    match value {
        Value::Object(map) => {
            map.remove("extensions");
            for (key, v) in map {
                if key != "extras" {
                    strip_extensions(v);
                }
            }
        }
        Value::Array(a) => {
            for v in a {
                strip_extensions(v);
            }
        }
        _ => {}
    }
}
fn align(v: &mut Vec<u8>, fill: u8) {
    while v.len() % 4 != 0 {
        v.push(fill);
    }
}
fn append(dest: &mut Vec<u8>, data: &[u8], cancel: &CancellationToken) -> Result<()> {
    if dest
        .len()
        .checked_add(data.len())
        .is_none_or(|n| n > MAX_PACKET)
    {
        return Err(fail("Preview binary exceeds 512 MiB"));
    }
    for chunk in data.chunks(1024 * 1024) {
        cancel.check()?;
        dest.extend_from_slice(chunk);
    }
    Ok(())
}
fn validate_image(bytes: &[u8], cancel: &CancellationToken) -> Result<(&'static str, usize)> {
    cancel.check()?;
    if bytes.len() > MAX_TEXTURE {
        return Err(fail("Preview texture exceeds 32 MiB"));
    }
    let format = image::guess_format(bytes).map_err(err)?;
    let mime = match format {
        image::ImageFormat::Png => "image/png",
        image::ImageFormat::Jpeg => "image/jpeg",
        _ => return Err(fail("glTF preview texture must be PNG or JPEG")),
    };
    let reader = image::ImageReader::with_format(Cursor::new(bytes), format);
    let (w, h) = reader.into_dimensions().map_err(err)?;
    if w == 0 || h == 0 || w > 16384 || h > 16384 || w as u64 * h as u64 > 64_000_000 {
        return Err(fail("Preview texture exceeds 64 megapixels"));
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16384);
    limits.max_image_height = Some(16384);
    limits.max_alloc = Some(512 * 1024 * 1024);
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), format);
    reader.limits(limits);
    reader.decode().map_err(err)?;
    cancel.check()?;
    Ok((mime, w as usize * h as usize * 4))
}
impl Prepared {
    fn packet(self, cancel: &CancellationToken) -> Result<Vec<u8>> {
        let json = serde_json::to_vec(&self.metadata)?;
        if json.len() > 16 * 1024 * 1024 {
            return Err(fail("Preview metadata exceeds 16 MiB"));
        }
        let mut output = Vec::new();
        output.extend_from_slice(b"DMBG");
        output.extend_from_slice(&1u32.to_le_bytes());
        output.extend_from_slice(&(json.len() as u32).to_le_bytes());
        output.extend_from_slice(&0u32.to_le_bytes());
        output.extend(json);
        align(&mut output, 0);
        append(&mut output, &self.glb, cancel)?;
        Ok(output)
    }
}
fn prepare(path: &Path, cancel: &CancellationToken, clock: &TimingRecorder) -> Result<Prepared> {
    let mut d = measure(clock, "Read and decode glTF preview", || {
        Document::load(path, cancel)
    })?;
    let mut metadata = measure(
        clock,
        "Validate preview geometry, skins and animations",
        || d.validate(),
    )?;
    let glb = measure(clock, "Embed preview textures and buffers", || d.bundle())?;
    measure(clock, "Verify preview source dependencies", || {
        d.verify_dependencies()
    })?;
    metadata.dependencies = d.dependencies;
    Ok(Prepared { metadata, glb })
}
/// Return DMBG v1 metadata and a self-contained, fully validated GLB.
pub fn load_packet(
    path: &Path,
    cancel: &CancellationToken,
    clock: &TimingRecorder,
) -> Result<Vec<u8>> {
    prepare(path, cancel, clock)?.packet(cancel)
}
/// Blender is an explicitly configured external authoring tool. The only Python
/// here executes inside Blender; the desktop process and format parser are Rust.
pub fn load_source_packet(
    path: &Path,
    blender: &Path,
    workspace: &Path,
    runner: &dyn ToolExecutor,
    cancel: &CancellationToken,
    clock: &TimingRecorder,
) -> Result<Vec<u8>> {
    let suffix = path
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if suffix == "gltf" || suffix == "glb" {
        return load_packet(path, cancel, clock);
    }
    if suffix != "blend" && suffix != "fbx" {
        return Err(fail(
            "Choose a GLB, glTF, Blender or FBX model for this preview",
        ));
    }
    let (source, raw) = measure(clock, "Snapshot authoring preview input", || {
        bounded_file(path, MAX_INPUT, cancel)
    })?;
    let before = sha(&raw);
    drop(raw);
    deadlock_vpk::check_unlinked_path(blender, false).map_err(err)?;
    if !blender.is_file() {
        return Err(fail(
            "Configure Blender in Setup to preview Blender or FBX sources",
        ));
    }
    if workspace.as_os_str().is_empty() {
        return Err(fail(
            "Configure a workspace for the Blender preview snapshot",
        ));
    }
    let root = workspace.join("preview");
    deadlock_vpk::check_unlinked_path(&root, true).map_err(err)?;
    fs::create_dir_all(&root).map_err(|e| CoreError::io("Create preview workspace", e))?;
    let root = root.canonicalize().map_err(err)?;
    if root.starts_with(&source) {
        return Err(fail("Preview workspace cannot be the source model"));
    }
    let destination = tempfile::Builder::new()
        .prefix("view-")
        .tempdir_in(root)
        .map_err(err)?;
    let output = destination.path().join("scene.glb");
    let receipt = destination.path().join("result.json");
    let job = destination.path().join("job.json");
    let script = destination.path().join("preview_export.py");
    fs::write(&script, EXPORTER).map_err(err)?;
    let helpers_script = destination.path().join("source_helpers.py");
    fs::write(&helpers_script, SOURCE_HELPERS).map_err(err)?;
    fs::write(
        &job,
        serde_json::to_vec(&json!({"source":source,"output":output,"receipt":receipt}))?,
    )
    .map_err(err)?;
    measure(clock, "Export Blender preview snapshot", || {
        let result = runner.run(
            &CommandSpec::new(blender)
                .args([
                    "--background",
                    "--factory-startup",
                    "--disable-autoexec",
                    "--python-exit-code",
                    "23",
                    "--python",
                ])
                .arg(&script)
                .arg("--")
                .arg(&job)
                .current_dir(destination.path()),
        )?;
        cancel.check()?;
        if !result.success {
            return Err(fail("Blender preview export failed; review the tool log"));
        }
        Ok(())
    })?;
    let mut prepared = measure(clock, "Verify Blender preview receipt", || {
        let (_, raw) = bounded_file(&receipt, MAX_JSON, cancel)?;
        let result: Value = serde_json::from_slice(&raw)?;
        let (_, snapshot) = bounded_file(&output, MAX_INPUT, cancel)?;
        if result.get("status").and_then(Value::as_str) != Some("ok")
            || result.get("output_sha256").and_then(Value::as_str) != Some(sha(&snapshot).as_str())
        {
            return Err(fail("Blender preview snapshot receipt/hash mismatch"));
        }
        let (_, after) = bounded_file(&source, MAX_INPUT, cancel)?;
        if sha(&after) != before
            || result.get("source_sha256").and_then(Value::as_str) != Some(before.as_str())
        {
            return Err(fail("Source changed during Blender preview export"));
        }
        if bounded_file(&script, MAX_JSON, cancel)?.1 != EXPORTER.as_bytes() {
            return Err(fail("Blender preview exporter script changed"));
        }
        if bounded_file(&helpers_script, MAX_JSON, cancel)?.1 != SOURCE_HELPERS.as_bytes() {
            return Err(fail("Blender source helper script changed"));
        }
        let dependencies = result
            .get("dependencies")
            .and_then(Value::as_object)
            .ok_or_else(|| fail("Blender preview receipt has no dependencies"))?;
        let mut verified = BTreeMap::new();
        let mut total = 0usize;
        for (path, hash) in dependencies {
            let hash = hash
                .as_str()
                .ok_or_else(|| fail("Invalid Blender dependency digest"))?;
            let (path, raw) = bounded_file(Path::new(path), MAX_INPUT, cancel)?;
            total = total
                .checked_add(raw.len())
                .ok_or_else(|| fail("Dependency size overflow"))?;
            if total > 2 * MAX_INPUT || sha(&raw) != hash {
                return Err(fail("Blender linked dependency changed during export"));
            }
            verified.insert(path.to_string_lossy().into_owned(), hash.to_owned());
        }
        let mut p = prepare(&output, cancel, clock)?;
        if p.metadata.source_sha256 != sha(&snapshot) {
            return Err(fail("Blender output changed during preview validation"));
        }
        for (path, hash) in &verified {
            if sha(&bounded_file(Path::new(path), MAX_INPUT, cancel)?.1) != *hash {
                return Err(fail("Blender source dependency changed during validation"));
            }
        }
        if sha(&bounded_file(&source, MAX_INPUT, cancel)?.1) != before {
            return Err(fail("Preview source changed during validation"));
        }
        p.metadata.source = source.to_string_lossy().into_owned();
        p.metadata.source_sha256 = before.clone();
        p.metadata.dependencies = verified;
        p.metadata
            .dependencies
            .insert(p.metadata.source.clone(), before.clone());
        for warning in array(&result, "warnings")? {
            p.metadata.warnings.push(
                warning
                    .as_str()
                    .ok_or_else(|| fail("Invalid Blender preview warning"))?
                    .to_owned(),
            );
        }
        Ok(p)
    })?;
    // Hold no handles into the temp directory when TempDir cleans up on Windows.
    prepared
        .metadata
        .dependencies
        .insert(source.to_string_lossy().into_owned(), before);
    prepared.packet(cancel)
}
