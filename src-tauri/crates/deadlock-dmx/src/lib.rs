//! Bounded binary DMX 4/5/9 parsing and lossless source rewriting.
//!
//! Scalar f32 values are never promoted and reserialized implicitly. Numeric
//! arrays and opaque vector/matrix payloads refer to the immutable input buffer.
//! Compiled Source 2 resources are a different format and are not accepted here.
#![forbid(unsafe_code)]

use deadlock_core::CancellationToken;
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt, fs,
    io::Read,
    ops::Range,
    path::Path,
    sync::Arc,
};

pub const MAX_INPUT_BYTES: usize = 512 * 1024 * 1024;
/// Bound decoded metadata allocation independently of the compressed/repeated
/// string-table representation. Large vertex streams remain borrowed ranges.
pub const MAX_METADATA_BYTES: usize = 256 * 1024 * 1024;
const MAX_COUNT: usize = 20_000_000;
const MAX_STRING_BYTES: usize = 1_000_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DmxError {
    pub offset: usize,
    pub message: String,
    pub cancelled: bool,
}
impl DmxError {
    pub fn into_core(self) -> deadlock_core::CoreError {
        if self.cancelled {
            deadlock_core::CoreError::Cancelled
        } else {
            deadlock_core::CoreError::Integrity(self.to_string())
        }
    }
    fn from_core(error: deadlock_core::CoreError) -> Self {
        let cancelled = matches!(error, deadlock_core::CoreError::Cancelled);
        Self {
            offset: 0,
            message: error.to_string(),
            cancelled,
        }
    }
    fn at(offset: usize, message: impl Into<String>) -> Self {
        Self {
            offset,
            message: message.into(),
            cancelled: false,
        }
    }
}
impl fmt::Display for DmxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (DMX byte {})", self.message, self.offset)
    }
}
impl std::error::Error for DmxError {}
pub type Result<T> = std::result::Result<T, DmxError>;

#[derive(Debug, Clone)]
pub enum Value {
    ElementRef(i32),
    Integer(i32),
    Float(f32),
    Bool(bool),
    String(String),
    Blob(Range<usize>),
    Opaque(Range<usize>),
    Packed {
        base: u8,
        range: Range<usize>,
        count: usize,
        stride: usize,
    },
    Array(Vec<Value>),
}
impl Value {
    pub fn len(&self) -> usize {
        match self {
            Self::Packed { count, .. } => *count,
            Self::Array(v) => v.len(),
            _ => 0,
        }
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn as_ref_index(&self) -> Option<i32> {
        if let Self::ElementRef(v) = self {
            Some(*v)
        } else {
            None
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        if let Self::String(v) = self {
            Some(v)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone)]
pub struct Attribute {
    pub name: String,
    pub type_code: u8,
    pub array: bool,
    pub value: Value,
    pub raw: Range<usize>,
}
#[derive(Debug, Clone)]
pub struct Element {
    pub kind: String,
    pub name: String,
    pub uuid: [u8; 16],
    pub attributes: Vec<Attribute>,
}
impl Element {
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.attributes
            .iter()
            .find(|a| a.name == name)
            .map(|a| &a.value)
    }
}

#[derive(Debug, Clone)]
pub struct Dmx {
    data: Arc<[u8]>,
    header_end: usize,
    pool_end: usize,
    pub version: u8,
    pub format: String,
    pub format_version: u32,
    pub strings: Vec<String>,
    pub elements: Vec<Element>,
}

pub mod limits;
pub mod split;

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
    metadata: usize,
}
impl<'a> Reader<'a> {
    fn reserve_metadata(&mut self, size: usize) -> Result<()> {
        self.metadata = self
            .metadata
            .checked_add(size)
            .filter(|&n| n <= MAX_METADATA_BYTES)
            .ok_or_else(|| {
                DmxError::at(
                    self.pos,
                    "DMX decoded metadata exceeds its 256 MiB allocation limit",
                )
            })?;
        Ok(())
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .pos
            .checked_add(n)
            .filter(|&end| end <= self.data.len())
            .ok_or_else(|| DmxError::at(self.pos, "Truncated or invalid DMX"))?;
        let data = &self.data[self.pos..end];
        self.pos = end;
        Ok(data)
    }
    fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn count(&mut self) -> Result<usize> {
        let n = self.i32()?;
        if n < 0 || n as usize > MAX_COUNT || n as usize > self.data.len() {
            return Err(DmxError::at(self.pos, "Invalid DMX array count"));
        }
        Ok(n as usize)
    }
    fn string(&mut self) -> Result<String> {
        let size = self.data[self.pos..]
            .iter()
            .take(MAX_STRING_BYTES + 1)
            .position(|&b| b == 0)
            .ok_or_else(|| DmxError::at(self.pos, "Unterminated DMX string"))?;
        let start = self.pos;
        self.reserve_metadata(size)?;
        let bytes = self.take(size + 1)?;
        String::from_utf8(bytes[..size].to_vec())
            .map_err(|_| DmxError::at(start, "Invalid UTF-8 in DMX string"))
    }
    fn pool(&mut self, version: u8, strings: &[String]) -> Result<String> {
        let index = if version == 4 {
            u16::from_le_bytes(self.take(2)?.try_into().unwrap()) as i32
        } else {
            self.i32()?
        };
        if index == -1 || (version == 4 && index == 65535) {
            return Ok(String::new());
        }
        let string = strings
            .get(index as usize)
            .ok_or_else(|| DmxError::at(self.pos, "Invalid DMX string index"))?;
        self.reserve_metadata(string.len())?;
        Ok(string.clone())
    }
    fn one(&mut self, base: u8, array: bool, version: u8, strings: &[String]) -> Result<Value> {
        match base {
            1 => {
                let i = self.i32()?;
                if i == -2 {
                    Err(DmxError::at(
                        self.pos,
                        "External element references are unsupported",
                    ))
                } else {
                    Ok(Value::ElementRef(i))
                }
            }
            2 | 7 => Ok(Value::Integer(self.i32()?)),
            3 => Ok(Value::Float(f32::from_le_bytes(
                self.take(4)?.try_into().unwrap(),
            ))),
            4 => Ok(Value::Bool(self.take(1)?[0] != 0)),
            5 => Ok(Value::String(if array {
                self.string()?
            } else {
                self.pool(version, strings)?
            })),
            6 => {
                let n = self.count()?;
                let start = self.pos;
                self.take(n)?;
                Ok(Value::Blob(start..self.pos))
            }
            _ => {
                let n =
                    stride(base).ok_or_else(|| DmxError::at(self.pos, "Unsupported DMX type"))?;
                let start = self.pos;
                self.take(n)?;
                Ok(Value::Opaque(start..self.pos))
            }
        }
    }
}
fn stride(base: u8) -> Option<usize> {
    match base {
        1 | 2 | 3 | 7 | 8 => Some(4),
        4 | 16 => Some(1),
        9 | 15 => Some(8),
        10 | 12 => Some(12),
        11 | 13 => Some(16),
        14 => Some(64),
        _ => None,
    }
}

impl Dmx {
    pub fn load(path: &Path) -> Result<Self> {
        Self::load_with_cancel(path, &CancellationToken::new())
    }
    pub fn load_with_cancel(path: &Path, cancel: &CancellationToken) -> Result<Self> {
        let metadata = fs::metadata(path)
            .map_err(|e| DmxError::at(0, format!("Cannot read {}: {e}", path.display())))?;
        if !metadata.is_file() || metadata.len() > MAX_INPUT_BYTES as u64 {
            return Err(DmxError::at(0, "DMX must be a file no larger than 512 MiB"));
        }
        let mut input = fs::File::open(path)
            .map_err(|e| DmxError::at(0, e.to_string()))?
            .take(MAX_INPUT_BYTES as u64 + 1);
        let mut data = Vec::new();
        let mut buffer = vec![0; 1024 * 1024];
        loop {
            cancel.check().map_err(DmxError::from_core)?;
            let n = input
                .read(&mut buffer)
                .map_err(|e| DmxError::at(0, e.to_string()))?;
            if n == 0 {
                break;
            }
            data.extend_from_slice(&buffer[..n]);
            if data.len() > MAX_INPUT_BYTES {
                return Err(DmxError::at(0, "DMX exceeds 512 MiB input limit"));
            }
        }
        Self::parse_with_cancel(data, cancel)
    }
    pub fn parse(data: impl Into<Arc<[u8]>>) -> Result<Self> {
        Self::parse_with_cancel(data, &CancellationToken::new())
    }
    pub fn parse_with_cancel(
        data: impl Into<Arc<[u8]>>,
        cancel: &CancellationToken,
    ) -> Result<Self> {
        let check = || cancel.check().map_err(DmxError::from_core);
        check()?;
        let data = data.into();
        if data.len() > MAX_INPUT_BYTES {
            return Err(DmxError::at(0, "DMX exceeds 512 MiB input limit"));
        }
        let header_end = data
            .iter()
            .take(512)
            .position(|&b| b == 0)
            .map(|n| n + 1)
            .ok_or_else(|| DmxError::at(0, "DMX must be a Blender binary 4, 5 or 9 export"))?;
        let header = std::str::from_utf8(&data[..header_end - 1])
            .map_err(|_| DmxError::at(0, "Invalid DMX header"))?;
        let fields: Vec<_> = header.split_whitespace().collect();
        let start = fields
            .windows(3)
            .position(|v| v == ["dmx", "encoding", "binary"])
            .ok_or_else(|| DmxError::at(0, "Unsupported DMX encoding"))?;
        let fail_header = || DmxError::at(0, "Unsupported DMX encoding or format header");
        let version: u8 = fields
            .get(start + 3)
            .ok_or_else(fail_header)?
            .parse()
            .map_err(|_| fail_header())?;
        if ![4, 5, 9].contains(&version) || fields.get(start + 4) != Some(&"format") {
            return Err(fail_header());
        }
        let format = fields.get(start + 5).ok_or_else(fail_header)?.to_string();
        if format.is_empty()
            || !format
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            return Err(fail_header());
        }
        let format_version = fields
            .get(start + 6)
            .ok_or_else(fail_header)?
            .parse()
            .map_err(|_| fail_header())?;
        let mut reader = Reader {
            data: &data,
            pos: header_end,
            metadata: 0,
        };
        if version == 9 && reader.i32()? != 0 {
            return Err(DmxError::at(
                reader.pos,
                "DMX prefix attributes are unsupported",
            ));
        }
        let count = reader.count()?;
        // Do not allocate from untrusted counts before the required bytes exist.
        if count > data.len() - reader.pos {
            return Err(DmxError::at(reader.pos, "Truncated DMX string table"));
        }
        reader.reserve_metadata(count * std::mem::size_of::<String>())?;
        let mut strings = Vec::new();
        for i in 0..count {
            if i % 256 == 0 {
                check()?;
            }
            strings.push(reader.string()?);
        }
        let pool_end = reader.pos;
        let count = reader.count()?;
        if count > (data.len() - reader.pos) / (if version == 4 { 20 } else { 24 }) {
            return Err(DmxError::at(reader.pos, "Truncated DMX element table"));
        }
        reader.reserve_metadata(count * std::mem::size_of::<Element>())?;
        let mut elements = Vec::new();
        for _ in 0..count {
            check()?;
            elements.push(Element {
                kind: reader.pool(version, &strings)?,
                name: reader.pool(version, &strings)?,
                uuid: reader.take(16)?.try_into().unwrap(),
                attributes: Vec::new(),
            });
        }
        for element in &mut elements {
            check()?;
            let attributes = reader.count()?;
            if attributes > (data.len() - reader.pos) / (if version == 4 { 4 } else { 6 }) {
                return Err(DmxError::at(reader.pos, "Truncated DMX attribute table"));
            }
            reader.reserve_metadata(attributes * std::mem::size_of::<Attribute>())?;
            for _ in 0..attributes {
                check()?;
                let name = reader.pool(version, &strings)?;
                let type_code = reader.take(1)?[0];
                let array = type_code >= if version == 9 { 32 } else { 15 };
                let base = if array {
                    type_code - if version == 9 { 32 } else { 14 }
                } else {
                    type_code
                };
                if (stride(base).is_none() && base != 5 && base != 6) || (version != 9 && base > 14)
                {
                    return Err(DmxError::at(
                        reader.pos,
                        format!("Unknown DMX attribute type: {type_code}"),
                    ));
                }
                let start = reader.pos;
                let n = if array { reader.count()? } else { 1 };
                let value = if array {
                    if let Some(stride) = stride(base) {
                        let size = n
                            .checked_mul(stride)
                            .ok_or_else(|| DmxError::at(reader.pos, "DMX array size overflow"))?;
                        let at = reader.pos;
                        reader.take(size)?;
                        Value::Packed {
                            base,
                            range: at..reader.pos,
                            count: n,
                            stride,
                        }
                    } else {
                        if n > data.len() - reader.pos {
                            return Err(DmxError::at(reader.pos, "Truncated DMX variable array"));
                        }
                        reader.reserve_metadata(n * std::mem::size_of::<Value>())?;
                        let mut values = Vec::new();
                        for i in 0..n {
                            if i % 256 == 0 {
                                check()?;
                            }
                            values.push(reader.one(base, true, version, &strings)?);
                        }
                        Value::Array(values)
                    }
                } else {
                    reader.one(base, false, version, &strings)?
                };
                element.attributes.push(Attribute {
                    name,
                    type_code,
                    array,
                    value,
                    raw: start..reader.pos,
                });
            }
        }
        if data[reader.pos..].iter().any(|b| !b"\0\r\n\t ".contains(b)) {
            return Err(DmxError::at(
                reader.pos,
                "Unexpected data after DMX element table",
            ));
        }
        let dmx = Self {
            data,
            header_end,
            pool_end,
            version,
            format,
            format_version,
            strings,
            elements,
        };
        dmx.validate_refs(cancel)?;
        check()?;
        Ok(dmx)
    }
    pub fn bytes(&self) -> &[u8] {
        &self.data
    }
    pub fn raw_attribute(&self, attribute: &Attribute) -> &[u8] {
        &self.data[attribute.raw.clone()]
    }
    pub fn value_bytes(&self, value: &Value) -> Option<&[u8]> {
        match value {
            Value::Opaque(r) | Value::Blob(r) | Value::Packed { range: r, .. } => {
                self.data.get(r.clone())
            }
            _ => None,
        }
    }
    pub fn integer_values(&self, value: &Value) -> Result<Vec<i32>> {
        match value {
            Value::Packed {
                base: 1 | 2 | 7,
                range,
                ..
            } => Ok(self.data[range.clone()]
                .chunks_exact(4)
                .map(|b| i32::from_le_bytes(b.try_into().unwrap()))
                .collect()),
            _ => Err(DmxError::at(0, "Expected a DMX integer array")),
        }
    }
    fn validate_refs(&self, cancel: &CancellationToken) -> Result<()> {
        for e in &self.elements {
            for a in &e.attributes {
                cancel.check().map_err(DmxError::from_core)?;
                if a.type_code == 1 || a.type_code == if self.version == 9 { 33 } else { 15 } {
                    let valid = |i: i32| i >= -1 && (i == -1 || (i as usize) < self.elements.len());
                    match &a.value {
                        Value::ElementRef(i) if valid(*i) => (),
                        Value::Packed { range, .. }
                            if self.data[range.clone()]
                                .chunks_exact(4)
                                .all(|b| valid(i32::from_le_bytes(b.try_into().unwrap()))) =>
                        {
                            ()
                        }
                        _ => {
                            return Err(DmxError::at(
                                a.raw.start,
                                "Out-of-range DMX element reference",
                            ))
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub fn summary(&self) -> Result<DmxSummary> {
        self.summary_with_cancel(&CancellationToken::new())
    }
    pub fn summary_with_cancel(&self, cancel: &CancellationToken) -> Result<DmxSummary> {
        let check = || cancel.check().map_err(DmxError::from_core);
        check()?;
        let mut meshes = Vec::new();
        let mut names = BTreeSet::new();
        for (index, e) in self
            .elements
            .iter()
            .enumerate()
            .filter(|(_, e)| e.kind == "DmeMesh")
        {
            check()?;
            if !names.insert(&e.name) {
                return Err(DmxError::at(
                    0,
                    format!("Duplicate source mesh name {:?}", e.name),
                ));
            }
            let state = e
                .get("bindState")
                .or_else(|| e.get("currentState"))
                .and_then(Value::as_ref_index)
                .filter(|&i| i >= 0)
                .and_then(|i| self.elements.get(i as usize));
            let length = |options: &[&str]| {
                state
                    .and_then(|e| {
                        e.attributes
                            .iter()
                            .find(|a| options.contains(&a.name.as_str()))
                    })
                    .map(|a| a.value.len())
                    .unwrap_or(0)
            };
            let weights = length(&["jointWeights", "blendweights$0"]);
            let mut attrs: Vec<_> = state
                .into_iter()
                .flat_map(|s| s.attributes.iter().map(|a| a.name.clone()))
                .collect();
            attrs.sort();
            attrs.dedup();
            meshes.push(MeshSummary {
                name: e.name.clone(),
                element: index,
                vertices: length(&["positions", "position$0"]),
                indexed_vertices: length(&["positionsIndices", "position$0Indices"]),
                weight_values: weights,
                joint_index_values: length(&["jointIndices", "blendindices$0"]),
                has_skin_weights: weights > 0,
                vertex_attributes: attrs,
            });
        }
        if meshes.is_empty() {
            return Err(DmxError::at(
                0,
                "Replacement DMX contains no DmeMesh elements",
            ));
        }
        let mut identities = Vec::new();
        for e in self.elements.iter().filter(|e| e.kind == "DmeMaterial") {
            let path = match e.get("mtlName") {
                None => e.name.clone(),
                Some(Value::String(s)) => s.clone(),
                _ => return Err(DmxError::at(0, "Material path must be a string")),
            };
            identities.push(MaterialIdentity {
                name: e.name.clone(),
                uuid: e.uuid.iter().map(|b| format!("{b:02x}")).collect(),
                path: path.replace('\\', "/"),
            });
        }
        let materials = identities
            .iter()
            .map(|m| m.path.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        Ok(DmxSummary {
            encoding: format!("binary {}", self.version),
            format: self.format.clone(),
            format_version: self.format_version,
            meshes,
            materials,
            material_identities: identities,
            bones: self
                .elements
                .iter()
                .filter(|e| e.kind == "DmeJoint")
                .map(|e| e.name.clone())
                .collect(),
            elements: self.elements.len(),
        })
    }
    pub fn geometry_counts(&self) -> Result<GeometryCounts> {
        self.geometry_counts_with_cancel(&CancellationToken::new())
    }
    pub fn geometry_counts_with_cancel(
        &self,
        cancel: &CancellationToken,
    ) -> Result<GeometryCounts> {
        let check = || cancel.check().map_err(DmxError::from_core);
        check()?;
        let mut meshes = Vec::new();
        for mesh in self.summary_with_cancel(cancel)?.meshes {
            check()?;
            let mut polygons = 0;
            let mut triangles = 0;
            let mut corners = 0;
            if let Some(face_refs) = self.elements[mesh.element].get("faceSets") {
                for index in self.integer_values(face_refs)? {
                    if index < 0 {
                        continue;
                    }
                    let face = self.elements.get(index as usize).ok_or_else(|| {
                        DmxError::at(0, "Face set references an out-of-range element")
                    })?;
                    if let Some(faces) = face.get("faces") {
                        let mut n = 0;
                        for (at, index) in self.integer_values(faces)?.into_iter().enumerate() {
                            if at % 65536 == 0 {
                                check()?;
                            }
                            if index == -1 {
                                if n < 3 {
                                    return Err(DmxError::at(
                                        0,
                                        "DMX contains a face with fewer than three corners",
                                    ));
                                }
                                polygons += 1;
                                triangles += n - 2;
                                corners += n;
                                n = 0;
                            } else if index < 0 {
                                return Err(DmxError::at(0, "Invalid DMX face index"));
                            } else {
                                n += 1;
                            }
                        }
                        if n != 0 {
                            return Err(DmxError::at(0, "Unterminated DMX face set"));
                        }
                    }
                }
            }
            meshes.push(MeshGeometry {
                name: mesh.name,
                vertices: mesh.vertices,
                polygons,
                triangles,
                corners,
            });
        }
        Ok(GeometryCounts {
            source_triangles: meshes.iter().map(|m| m.triangles).sum(),
            meshes,
        })
    }
    pub fn single_mesh(&self, name: &str) -> Result<Vec<u8>> {
        if !self.summary()?.meshes.iter().any(|m| m.name == name) {
            return Err(DmxError::at(
                0,
                format!("Replacement DMX does not contain mapped mesh: {name}"),
            ));
        }
        let keep: Vec<_> = self
            .elements
            .iter()
            .enumerate()
            .filter(|(_, e)| e.kind != "DmeMesh" || e.name == name)
            .map(|(i, _)| i)
            .collect();
        let mapping: BTreeMap<_, _> = keep
            .iter()
            .enumerate()
            .map(|(new, &old)| (old as i32, new as i32))
            .collect();
        let pool: BTreeMap<_, _> = self
            .strings
            .iter()
            .enumerate()
            .map(|(i, s)| (s.as_str(), i))
            .collect();
        let mut output = self.data[..self.pool_end].to_vec();
        put_count(&mut output, keep.len())?;
        for &i in &keep {
            let e = &self.elements[i];
            put_pool(&mut output, &pool, &e.kind, self.version)?;
            put_pool(&mut output, &pool, &e.name, self.version)?;
            output.extend(e.uuid);
        }
        for i in keep {
            let e = &self.elements[i];
            put_count(&mut output, e.attributes.len())?;
            for a in &e.attributes {
                put_pool(&mut output, &pool, &a.name, self.version)?;
                output.push(a.type_code);
                if a.type_code == 1 {
                    let at = a
                        .value
                        .as_ref_index()
                        .ok_or_else(|| DmxError::at(a.raw.start, "Expected element reference"))?;
                    output.extend(mapping.get(&at).copied().unwrap_or(-1).to_le_bytes());
                } else if a.type_code == if self.version == 9 { 33 } else { 15 } {
                    let refs = self.integer_values(&a.value)?;
                    put_count(&mut output, refs.len())?;
                    for at in refs {
                        output.extend(mapping.get(&at).copied().unwrap_or(-1).to_le_bytes());
                    }
                } else {
                    output.extend(self.raw_attribute(a));
                }
            }
        }
        // Required separate decode verifies all rebuilt indexes and output shape.
        let verified = Self::parse(output.clone())?;
        if verified
            .summary()?
            .meshes
            .iter()
            .map(|m| m.name.as_str())
            .collect::<Vec<_>>()
            != [name]
        {
            return Err(DmxError::at(0, "Isolated DMX validation failed"));
        }
        Ok(output)
    }
    /// Overrides contain complete serialized attribute payloads, including array
    /// length where applicable. Unknown keys fail; result is reparsed before return.
    pub fn rewrite_binary9(&self, edits: &Rewrite) -> Result<Vec<u8>> {
        if self.version != 9 {
            return Err(DmxError::at(0, "Generated sources require binary DMX 9"));
        }
        if edits
            .names
            .keys()
            .chain(edits.identities.keys())
            .any(|&i| i >= self.elements.len())
        {
            return Err(DmxError::at(0, "Unknown rewritten element"));
        }
        let mut strings = self.strings.clone();
        for name in edits.names.values() {
            if name.is_empty() || name.contains('\0') {
                return Err(DmxError::at(0, "Invalid generated DMX name"));
            }
            if !strings.contains(name) {
                strings.push(name.clone());
            }
        }
        let pool: BTreeMap<_, _> = strings
            .iter()
            .enumerate()
            .map(|(i, s)| (s.as_str(), i))
            .collect();
        let mut output = self.data[..self.header_end].to_vec();
        output.extend(0_i32.to_le_bytes());
        put_count(&mut output, strings.len())?;
        for s in &strings {
            output.extend(s.as_bytes());
            output.push(0);
        }
        put_count(&mut output, self.elements.len())?;
        for (i, e) in self.elements.iter().enumerate() {
            put_pool(&mut output, &pool, &e.kind, 9)?;
            put_pool(
                &mut output,
                &pool,
                edits.names.get(&i).unwrap_or(&e.name),
                9,
            )?;
            output.extend(edits.identities.get(&i).unwrap_or(&e.uuid));
        }
        let mut used = BTreeSet::new();
        for (i, e) in self.elements.iter().enumerate() {
            put_count(&mut output, e.attributes.len())?;
            for a in &e.attributes {
                put_pool(&mut output, &pool, &a.name, 9)?;
                output.push(a.type_code);
                let key = (i, a.name.clone());
                if let Some(raw) = edits.attributes.get(&key) {
                    used.insert(key);
                    output.extend(raw);
                } else {
                    output.extend(self.raw_attribute(a));
                }
            }
        }
        if used.len() != edits.attributes.len() {
            return Err(DmxError::at(
                0,
                "Rewrite attempted to replace an unknown attribute",
            ));
        }
        Self::parse(output.clone())?;
        Ok(output)
    }
}

#[derive(Debug, Default)]
pub struct Rewrite {
    pub attributes: BTreeMap<(usize, String), Vec<u8>>,
    pub names: BTreeMap<usize, String>,
    pub identities: BTreeMap<usize, [u8; 16]>,
}
fn put_count(output: &mut Vec<u8>, count: usize) -> Result<()> {
    let n = i32::try_from(count).map_err(|_| DmxError::at(0, "DMX output count overflow"))?;
    output.extend(n.to_le_bytes());
    Ok(())
}
fn put_pool(
    output: &mut Vec<u8>,
    pool: &BTreeMap<&str, usize>,
    text: &str,
    version: u8,
) -> Result<()> {
    if version == 4 {
        let v = match pool.get(text) {
            Some(&i) => u16::try_from(i).map_err(|_| DmxError::at(0, "DMX v4 pool overflow"))?,
            None => 65535,
        };
        output.extend(v.to_le_bytes());
    } else {
        output.extend(
            match pool.get(text) {
                Some(&i) => i32::try_from(i).map_err(|_| DmxError::at(0, "DMX pool overflow"))?,
                None => -1,
            }
            .to_le_bytes(),
        );
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MeshSummary {
    pub name: String,
    pub element: usize,
    pub vertices: usize,
    pub indexed_vertices: usize,
    pub weight_values: usize,
    pub joint_index_values: usize,
    pub has_skin_weights: bool,
    pub vertex_attributes: Vec<String>,
}
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MaterialIdentity {
    pub name: String,
    pub uuid: String,
    pub path: String,
}
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DmxSummary {
    pub encoding: String,
    pub format: String,
    pub format_version: u32,
    pub meshes: Vec<MeshSummary>,
    pub materials: Vec<String>,
    pub material_identities: Vec<MaterialIdentity>,
    pub bones: Vec<String>,
    pub elements: usize,
}
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MeshGeometry {
    pub name: String,
    pub vertices: usize,
    pub polygons: u64,
    pub triangles: u64,
    pub corners: u64,
}
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GeometryCounts {
    pub meshes: Vec<MeshGeometry>,
    pub source_triangles: u64,
}

#[cfg(test)]
mod allocation_tests {
    use super::*;
    #[test]
    fn pool_references_charge_before_cloning_and_cannot_amplify_without_bound() {
        let data = 0_i32.to_le_bytes();
        let strings = vec!["pooled string".to_string()];
        let mut reader = Reader {
            data: &data,
            pos: 0,
            metadata: MAX_METADATA_BYTES - 1,
        };
        assert!(reader.pool(9, &strings).is_err());
    }
}
