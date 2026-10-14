use crate::{invalid, Annotation, Kind, Result, Value, MAX_BLOCK, MAX_COUNT, MAX_DEPTH};
use std::collections::{HashMap, HashSet, VecDeque};

const FOOTER: u32 = 0xffeedd00;
#[derive(Clone, Debug)]
pub struct Document {
    pub version: u8,
    pub format_guid: [u8; 16],
    pub root: Value,
}

#[derive(Default)]
struct Cursor {
    bytes: Vec<u8>,
    at: usize,
}
impl Cursor {
    fn new(bytes: Vec<u8>) -> Self {
        Self { bytes, at: 0 }
    }
    fn remaining(&self) -> usize {
        self.bytes.len() - self.at
    }
    fn take(&mut self, n: usize) -> Result<Vec<u8>> {
        if n > self.remaining() {
            return Err(invalid("read outside bounded buffer"));
        }
        let out = self.bytes[self.at..self.at + n].to_vec();
        self.at += n;
        Ok(out)
    }
    fn fixed<const N: usize>(&mut self) -> Result<[u8; N]> {
        if N > self.remaining() {
            return Err(invalid("truncated scalar"));
        }
        let mut result = [0; N];
        result.copy_from_slice(&self.bytes[self.at..self.at + N]);
        self.at += N;
        Ok(result)
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.fixed::<1>()?[0])
    }
    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.fixed()?))
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.fixed()?))
    }
    fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_le_bytes(self.fixed()?))
    }
    fn align(&mut self, n: usize) -> Result<()> {
        self.take((n - self.at % n) % n)?;
        Ok(())
    }
    fn string(&mut self) -> Result<String> {
        let n = self.bytes[self.at..]
            .iter()
            .position(|b| *b == 0)
            .ok_or_else(|| invalid("unterminated string"))?;
        let raw = self.take(n)?;
        self.u8()?;
        String::from_utf8(raw).map_err(|_| invalid("invalid UTF-8 string"))
    }
    fn finished(&self) -> Result<()> {
        if self.remaining() == 0 {
            Ok(())
        } else {
            Err(invalid("unconsumed KV3 values; refusing lossy conversion"))
        }
    }
}
#[derive(Default)]
struct Streams {
    bytes: Cursor,
    shorts: Cursor,
    ints: Cursor,
    doubles: Cursor,
}
impl Streams {
    fn split(c: &mut Cursor, b: usize, s: usize, i: usize, d: usize, force: bool) -> Result<Self> {
        let bytes = Cursor::new(c.take(b)?);
        if s > 0 || force {
            c.align(2)?;
        }
        let shorts =
            Cursor::new(c.take(s.checked_mul(2).ok_or_else(|| invalid("size overflow"))?)?);
        if i > 0 || force {
            c.align(4)?;
        }
        let ints = Cursor::new(c.take(i.checked_mul(4).ok_or_else(|| invalid("size overflow"))?)?);
        if d > 0 || force {
            c.align(8)?;
        }
        let doubles =
            Cursor::new(c.take(d.checked_mul(8).ok_or_else(|| invalid("size overflow"))?)?);
        Ok(Self {
            bytes,
            shorts,
            ints,
            doubles,
        })
    }
    fn finished(&self) -> Result<()> {
        self.bytes.finished()?;
        self.shorts.finished()?;
        self.ints.finished()?;
        self.doubles.finished()
    }
}
struct Context {
    version: u8,
    strings: Vec<String>,
    streams: Vec<Streams>,
    active: usize,
    types: Cursor,
    members: Option<Cursor>,
    blob_sizes: Option<VecDeque<usize>>,
    blobs: Cursor,
    values: usize,
}
impl Context {
    fn type_tag(&mut self) -> Result<(u8, Annotation)> {
        let mut tag = self.types.u8()?;
        let annotation = if tag & 128 != 0 {
            let flag = self.types.u8()?;
            if self.version >= 4 {
                Annotation::try_from(flag)?
            } else {
                Annotation::try_from(match flag {
                    1 => 1,
                    2 => 2,
                    8 => 3,
                    16 => 4,
                    32 => 5,
                    _ => return Err(invalid("unsupported legacy annotation flags")),
                })?
            }
        } else {
            Annotation::Unspecified
        };
        if self.version >= 4 {
            if tag & 64 != 0 {
                return Err(invalid("unsupported KV3 type flags"));
            }
            tag &= 63;
        } else {
            tag &= 127;
        }
        // 26 and 27 (UINT32_ZERO, UINT32_ONE) appear in the October 2026 game
        // files (Baba's model).
        if !(1..=27).contains(&tag) {
            return Err(invalid(format!("unknown KV3 type {tag}")));
        }
        Ok((tag, annotation))
    }
    fn value(&mut self, depth: usize) -> Result<Value> {
        let (tag, annotation) = self.type_tag()?;
        self.typed(tag, annotation, depth)
    }
    fn count(n: i64) -> Result<usize> {
        if !(0..=MAX_COUNT as i64).contains(&n) {
            Err(invalid("array/object count exceeds supported bound"))
        } else {
            Ok(n as usize)
        }
    }
    fn string(&self, id: i32, empty: String) -> Result<String> {
        if id == -1 {
            Ok(empty)
        } else if id >= 0 {
            self.strings
                .get(id as usize)
                .cloned()
                .ok_or_else(|| invalid("string index outside table"))
        } else {
            Err(invalid("string index outside table"))
        }
    }
    fn typed(&mut self, tag: u8, annotation: Annotation, depth: usize) -> Result<Value> {
        if depth > MAX_DEPTH {
            return Err(invalid("nesting exceeds supported bound"));
        }
        self.values += 1;
        if self.values > 8 * MAX_COUNT {
            return Err(invalid("total values exceed supported bound"));
        }
        let active = self.active;
        let kind = match tag {
            1 => {
                if annotation != Annotation::Unspecified {
                    return Err(invalid("unsupported annotation on null"));
                }
                Kind::Null
            }
            2 => Kind::Bool(self.streams[active].bytes.u8()? == 1),
            3 => Kind::I64(i64::from_le_bytes(self.streams[active].doubles.fixed()?)),
            4 => Kind::U64(u64::from_le_bytes(self.streams[active].doubles.fixed()?)),
            5 => Kind::F64(f64::from_le_bytes(self.streams[active].doubles.fixed()?)),
            6 => {
                let id = self.streams[active].ints.i32()?;
                Kind::String(self.string(id, String::new())?)
            }
            7 => {
                let bytes = if let Some(sizes) = &mut self.blob_sizes {
                    let size = sizes
                        .pop_front()
                        .ok_or_else(|| invalid("missing binary blob size"))?;
                    self.blobs.take(size)?
                } else {
                    let size = self.streams[active].ints.i32()?;
                    if size < 0 {
                        return Err(invalid("negative blob size"));
                    }
                    self.streams[active].bytes.take(size as usize)?
                };
                Kind::Blob(bytes)
            }
            8 => {
                let n = Self::count(self.streams[active].ints.i32()? as i64)?;
                let mut a = Vec::with_capacity(n.min(4096));
                for _ in 0..n {
                    a.push(self.value(depth + 1)?);
                }
                Kind::Array(a)
            }
            9 => {
                let n = if let Some(m) = &mut self.members {
                    m.u32()?
                } else {
                    self.streams[active].ints.u32()?
                };
                let n = Self::count(n as i64)?;
                let mut rows = Vec::with_capacity(n.min(4096));
                let mut names = HashSet::new();
                for i in 0..n {
                    let id = self.streams[self.active].ints.i32()?;
                    let name = self.string(id, i.to_string())?;
                    if !names.insert(name.clone()) {
                        return Err(invalid("duplicate property"));
                    }
                    rows.push((name, self.value(depth + 1)?));
                }
                Kind::Object(rows)
            }
            10 | 24 | 25 => {
                let n = if tag == 10 {
                    Self::count(self.streams[active].ints.u32()? as i64)?
                } else {
                    self.streams[active].bytes.u8()? as usize
                };
                if tag == 25 {
                    if self.version != 5 || annotation != Annotation::Unspecified {
                        return Err(invalid("unsupported alternate-buffer typed array"));
                    }
                    self.active = 0;
                }
                let (element, element_annotation) = self.type_tag()?;
                if [3, 4, 5, 11, 12, 15, 16, 17, 18, 26, 27].contains(&element)
                    && (annotation != Annotation::Unspecified
                        || element_annotation != Annotation::Unspecified)
                {
                    return Err(invalid("unsupported annotated numeric typed array"));
                }
                let mut values = Vec::with_capacity(n.min(4096));
                for _ in 0..n {
                    values.push(self.typed(element, element_annotation, depth + 1)?);
                }
                if tag == 25 {
                    self.active = 1;
                }
                Kind::Array(values)
            }
            11 => Kind::I64(self.streams[active].ints.i32()? as i64),
            12 => Kind::I64(self.streams[active].ints.u32()? as i64),
            13 => Kind::Bool(true),
            14 => Kind::Bool(false),
            15 => Kind::I64(0),
            16 => Kind::I64(1),
            17 => Kind::F64(0.0),
            18 => Kind::F64(1.0),
            19 => Kind::F32(f32::from_le_bytes(self.streams[active].ints.fixed()?)),
            20 => Kind::I64(i16::from_le_bytes(self.streams[active].shorts.fixed()?) as i64),
            21 => Kind::I64(self.streams[active].shorts.u16()? as i64),
            22 => Kind::I64(self.streams[active].bytes.u8()? as i8 as i64),
            23 => Kind::I64(self.streams[active].bytes.u8()? as i64),
            26 => Kind::I64(0),
            27 => Kind::I64(1),
            _ => return Err(invalid("unknown scalar type")),
        };
        match kind {
            Kind::F32(v) if !v.is_finite() => return Err(invalid("non-finite number")),
            Kind::F64(v) if !v.is_finite() => return Err(invalid("non-finite number")),
            _ => (),
        }
        Ok(Value { annotation, kind })
    }
    fn finished(&mut self) -> Result<()> {
        if self.version == 1 && self.types.u32()? != FOOTER {
            return Err(invalid("invalid v1 footer"));
        }
        self.types.finished()?;
        for s in &self.streams {
            s.finished()?;
        }
        if let Some(m) = &self.members {
            m.finished()?;
        }
        self.blobs.finished()?;
        if self.blob_sizes.as_ref().is_some_and(|s| !s.is_empty()) {
            return Err(invalid("unconsumed binary blob sizes"));
        }
        Ok(())
    }
}

fn decompress(data: &[u8], expected: usize, method: u32) -> Result<Vec<u8>> {
    if expected > MAX_BLOCK {
        return Err(invalid("decompressed block exceeds 128 MiB"));
    }
    let output = match method {
        0 => data.to_vec(),
        1 => {
            lz4_flex::block::decompress(data, expected).map_err(|_| invalid("invalid LZ4 block"))?
        }
        2 => {
            if zstd::zstd_safe::find_frame_compressed_size(data)
                .map_err(|_| invalid("invalid Zstandard frame"))?
                != data.len()
            {
                return Err(invalid("extra Zstandard frame/trailing bytes"));
            }
            let mut decoder = zstd::bulk::Decompressor::new()
                .map_err(|_| invalid("Zstandard decoder initialization failed"))?;
            decoder
                .set_parameter(zstd::zstd_safe::DParameter::WindowLogMax(27))
                .map_err(|_| invalid("cannot bound Zstandard window"))?;
            decoder
                .decompress(data, expected)
                .map_err(|_| invalid("invalid/truncated Zstandard frame or size bound"))?
        }
        _ => return Err(invalid("unknown compression method")),
    };
    if output.len() != expected {
        return Err(invalid("decompressed size mismatch"));
    }
    Ok(output)
}

pub fn decode(payload: &[u8]) -> Result<Document> {
    if payload.len() > MAX_BLOCK {
        return Err(invalid("compiled KV3 block exceeds 128 MiB"));
    }
    let mut input = Cursor::new(payload.to_vec());
    let signature = input.fixed::<4>()?;
    let version = match signature {
        [1, b'3', b'V', b'K'] => 1,
        [4, b'3', b'V', b'K'] => 4,
        [5, b'3', b'V', b'K'] => 5,
        _ => return Err(invalid("supported KV3 layouts are v1, v4 and v5")),
    };
    let format_guid = input.fixed()?;
    let mut context = match version {
        1 => read_v1(&mut input)?,
        4 => read_v4(&mut input)?,
        _ => read_v5(&mut input)?,
    };
    input.finished()?;
    let root = context.value(0)?;
    context.finished()?;
    root.validate()?;
    Ok(Document {
        version,
        format_guid,
        root,
    })
}
fn read_v1(input: &mut Cursor) -> Result<Context> {
    let method = input.u32()?;
    if method > 1 {
        return Err(invalid("unsupported v1 compression"));
    }
    let b = input.u32()? as usize;
    let i = input.u32()? as usize;
    let d = input.u32()? as usize;
    let size = input.u32()? as usize;
    let packed = input.take(if method == 0 { size } else { input.remaining() })?;
    let mut raw = Cursor::new(decompress(&packed, size, method)?);
    let mut streams = Streams::split(&mut raw, b, 0, i, d, true)?;
    let count = Context::count(streams.ints.u32()? as i64)?;
    let mut strings = Vec::with_capacity(count.min(4096));
    for _ in 0..count {
        strings.push(raw.string()?);
    }
    Ok(Context {
        version: 1,
        strings,
        streams: vec![streams],
        active: 0,
        types: Cursor::new(raw.take(raw.remaining())?),
        members: None,
        blob_sizes: None,
        blobs: Cursor::default(),
        values: 0,
    })
}
/// The single-buffer v4 layout used by shipped sound CTRL blocks. Layout
/// reference: ValveResourceFormat BinaryKV3.ReadBuffer (see audio fix evidence).
/// Unlike v5, strings follow the scalar lanes and Zstd may cover a second,
/// consecutive blob frame. Keep all length, footer and consumption checks.
fn read_v4(input: &mut Cursor) -> Result<Context> {
    let method = input.u32()?;
    let dictionary = input.u16()?;
    let frame = input.u16()? as usize;
    if method > 2 || dictionary != 0 || frame != if method == 1 { 16384 } else { 0 } {
        return Err(invalid("unsupported v4 compression configuration"));
    }
    let b = input.u32()? as usize;
    let i = input.u32()? as usize;
    let d = input.u32()? as usize;
    let strings_types_size = input.u32()? as usize;
    input.u16()?; // Object/array allocation hints; actual counts are in the lanes.
    input.u16()?;
    let size = input.u32()? as usize;
    let packed_size = input.u32()? as usize;
    let block_count = Context::count(input.u32()? as i64)?;
    let block_size = input.u32()? as usize;
    let s = input.u32()? as usize;
    let frame_bytes = input.u32()? as usize;
    let total_size = size.checked_add(block_size).ok_or_else(|| invalid("v4 size overflow"))?;
    if total_size > MAX_BLOCK || frame_bytes % 2 != 0 || frame_bytes / 2 > MAX_COUNT
        || (block_count == 0 && (block_size != 0 || frame_bytes != 0))
        || (method != 1 && frame_bytes != 0)
        || (method == 0 && packed_size != size)
    {
        return Err(invalid("invalid v4 buffer/blob sizes"));
    }
    let packed = input.take(packed_size)?;
    let (decoded, zstd_blobs) = if method == 2 {
        let first_size = zstd::zstd_safe::find_frame_compressed_size(&packed)
            .map_err(|_| invalid("invalid v4 Zstandard frame"))?;
        if first_size == packed.len() {
            let mut joined = decompress(&packed, total_size, 2)?;
            let blobs = joined.split_off(size);
            (joined, blobs)
        } else {
            if block_count == 0 || first_size > packed.len() {
                return Err(invalid("unexpected v4 Zstandard frames"));
            }
            (decompress(&packed[..first_size], size, 2)?,
             decompress(&packed[first_size..], block_size, 2)?)
        }
    } else {
        (decompress(&packed, size, method)?, Vec::new())
    };
    let mut raw = Cursor::new(decoded);
    let mut streams = Streams::split(&mut raw, b, s, i, d, true)?;
    let count = Context::count(streams.ints.u32()? as i64)?;
    let mut types = Cursor::new(raw.take(strings_types_size)?);
    let mut strings = Vec::with_capacity(count.min(4096));
    for _ in 0..count { strings.push(types.string()?); }
    let mut sizes = VecDeque::with_capacity(block_count.min(4096));
    for _ in 0..block_count { sizes.push_back(raw.u32()? as usize); }
    let actual = sizes.iter().try_fold(0usize, |sum, n| sum.checked_add(*n))
        .ok_or_else(|| invalid("v4 blob size overflow"))?;
    if actual != block_size || raw.u32()? != FOOTER {
        return Err(invalid("invalid v4 blob total or footer"));
    }
    let mut frames = Cursor::new(raw.take(frame_bytes)?);
    raw.finished()?;
    let blob_data = match method {
        0 => input.take(block_size)?,
        1 => {
            let mut output = Vec::with_capacity(block_size);
            // A frame cannot cross a blob boundary, but its dictionary can
            // reference bytes from preceding blobs. Empty blobs need no frame.
            for length in &sizes {
                let end = output.len() + length;
                while output.len() < end {
                    let compressed_size = frames.u16()? as usize;
                    if compressed_size == 0 { return Err(invalid("empty v4 LZ4 frame")); }
                    let compressed = input.take(compressed_size)?;
                    let mut decoded = vec![0; (end - output.len()).min(frame)];
                    let dictionary = &output[output.len().saturating_sub(65536)..];
                    let n = lz4_flex::block::decompress_into_with_dict(&compressed, &mut decoded, dictionary)
                        .map_err(|_| invalid("invalid v4 chained LZ4 frame"))?;
                    if n == 0 { return Err(invalid("empty decoded v4 LZ4 frame")); }
                    output.extend_from_slice(&decoded[..n]);
                }
            }
            output
        }
        2 => zstd_blobs,
        _ => unreachable!(),
    };
    frames.finished()?;
    if block_count > 0 && input.u32()? != FOOTER {
        return Err(invalid("invalid v4 binary blob footer"));
    }
    Ok(Context {
        version: 4, strings, streams: vec![streams], active: 0, types,
        members: None, blob_sizes: Some(sizes), blobs: Cursor::new(blob_data), values: 0,
    })
}
fn read_v5(input: &mut Cursor) -> Result<Context> {
    let method = input.u32()?;
    let dictionary = input.u16()?;
    let frame = input.u16()?;
    if (method == 0 && (dictionary != 0 || frame != 0))
        || (method == 1 && (dictionary != 0 || frame != 16384))
        || method > 2
    {
        return Err(invalid("unsupported compression configuration"));
    }
    let b = input.u32()? as usize;
    let i = input.u32()? as usize;
    let d = input.u32()? as usize;
    let types_size = input.u32()? as usize;
    input.u16()?;
    input.u16()?;
    let _uncompressed_total = input.u32()?;
    let compressed_total = input.u32()? as usize;
    let block_count = Context::count(input.u32()? as i64)?;
    let block_size = input.u32()? as usize;
    let s = input.u32()? as usize;
    let frame_bytes = input.u32()? as usize;
    if block_size > MAX_BLOCK || frame_bytes % 2 != 0 || frame_bytes / 2 > MAX_COUNT {
        return Err(invalid("invalid blob/frame size table"));
    }
    let out0 = input.u32()? as usize;
    let packed0 = input.u32()? as usize;
    let out1 = input.u32()? as usize;
    let packed1 = input.u32()? as usize;
    let b1 = input.u32()? as usize;
    let s1 = input.u32()? as usize;
    let i1 = input.u32()? as usize;
    let d1 = input.u32()? as usize;
    input.u32()?;
    let objects = Context::count(input.u32()? as i64)?;
    input.u32()?;
    input.u32()?;
    let raw0 = input.take(if method == 0 { out0 } else { packed0 })?;
    let raw1 = input.take(if method == 0 { out1 } else { packed1 })?;
    let mut raw0 = Cursor::new(decompress(&raw0, out0, method)?);
    let mut raw1 = Cursor::new(decompress(&raw1, out1, method)?);
    let mut streams0 = Streams::split(&mut raw0, b, s, i, d, false)?;
    let string_count = Context::count(streams0.ints.u32()? as i64)?;
    let mut strings = Vec::with_capacity(string_count.min(4096));
    for _ in 0..string_count {
        strings.push(streams0.bytes.string()?);
    }
    let members = Cursor::new(raw1.take(objects * 4)?);
    let streams1 = Streams::split(&mut raw1, b1, s1, i1, d1, false)?;
    let types = Cursor::new(raw1.take(types_size)?);
    let mut sizes = VecDeque::with_capacity(block_count.min(4096));
    for _ in 0..block_count {
        sizes.push_back(raw1.u32()? as usize);
    }
    if raw1.u32()? != FOOTER {
        return Err(invalid("invalid v5 footer"));
    }
    let mut blob_data = Vec::new();
    if block_count > 0 {
        let actual = sizes
            .iter()
            .try_fold(0usize, |sum, n| sum.checked_add(*n))
            .ok_or_else(|| invalid("blob size overflow"))?;
        if actual != block_size {
            return Err(invalid("binary blob total mismatch"));
        }
        let mut frames = Vec::with_capacity((frame_bytes / 2).min(4096));
        for _ in 0..frame_bytes / 2 {
            frames.push(raw1.u16()? as usize);
        }
        // A declared blob stream has its own footer even when every blob is
        // empty (for example Haze's one-frame item_crouch_idle clip). Process
        // its compression envelope and validate the footer in that case too.
        blob_data = match method {
            0 => input.take(block_size)?,
            1 => {
                let mut output = Vec::with_capacity(block_size);
                for length in frames {
                    if length == 0 || output.len() >= block_size {
                        return Err(invalid("invalid compressed frame table"));
                    }
                    let bound = (block_size - output.len()).min(frame as usize);
                    let mut decoded = vec![0; bound];
                    let compressed = input.take(length)?;
                    let dictionary = &output[output.len().saturating_sub(65536)..];
                    let n = lz4_flex::block::decompress_into_with_dict(
                        &compressed,
                        &mut decoded,
                        dictionary,
                    )
                    .map_err(|_| invalid("invalid chained LZ4 frame"))?;
                    if n == 0 || n > bound {
                        return Err(invalid("invalid decoded LZ4 frame size"));
                    }
                    output.extend_from_slice(&decoded[..n]);
                }
                if output.len() != block_size {
                    return Err(invalid("chained LZ4 total size mismatch"));
                }
                output
            }
            2 => {
                let length = compressed_total
                    .checked_sub(packed0)
                    .and_then(|v| v.checked_sub(packed1))
                    .ok_or_else(|| invalid("compressed size underflow"))?;
                decompress(&input.take(length)?, block_size, 2)?
            }
            _ => return Err(invalid("unknown compression")),
        };
        if input.u32()? != FOOTER {
            return Err(invalid("invalid binary blob footer"));
        }
    }
    raw0.finished()?;
    raw1.finished()?;
    Ok(Context {
        version: 5,
        strings,
        streams: vec![streams0, streams1],
        active: 1,
        types,
        members: Some(members),
        blob_sizes: if block_count > 0 { Some(sizes) } else { None },
        blobs: Cursor::new(blob_data),
        values: 0,
    })
}

/// Canonical uncompressed v1 encoding, byte-for-byte matching compiled_kv3.encode.
pub fn encode(root: &Value, format_guid: [u8; 16]) -> Result<Vec<u8>> {
    root.validate()?;
    struct Writer {
        strings: Vec<String>,
        lookup: HashMap<String, i32>,
        types: Vec<u8>,
        bytes: Vec<u8>,
        ints: Vec<u8>,
        doubles: Vec<u8>,
    }
    impl Writer {
        fn sid(&mut self, s: &str) -> Result<i32> {
            if let Some(n) = self.lookup.get(s) {
                return Ok(*n);
            }
            let n =
                i32::try_from(self.strings.len()).map_err(|_| invalid("string table overflow"))?;
            self.strings.push(s.into());
            self.lookup.insert(s.into(), n);
            Ok(n)
        }
        fn i32(&mut self, n: usize) -> Result<()> {
            self.ints.extend_from_slice(
                &i32::try_from(n)
                    .map_err(|_| invalid("length overflow"))?
                    .to_le_bytes(),
            );
            Ok(())
        }
        fn visit(&mut self, v: &Value) -> Result<()> {
            let tag = match v.kind {
                Kind::Null => 1,
                Kind::Bool(_) => 2,
                Kind::I64(_) => 3,
                Kind::U64(_) => 4,
                Kind::F32(_) | Kind::F64(_) => 5,
                Kind::String(_) => 6,
                Kind::Blob(_) => 7,
                Kind::Array(_) => 8,
                Kind::Object(_) => 9,
            };
            let flag = match v.annotation {
                Annotation::Resource => 1,
                Annotation::ResourceName => 2,
                Annotation::Panorama => 8,
                Annotation::SoundEvent => 16,
                Annotation::Subclass => 32,
                Annotation::Unspecified => 0,
            };
            self.types.push(tag | if flag > 0 { 128 } else { 0 });
            if flag > 0 {
                self.types.push(flag);
            }
            match &v.kind {
                Kind::Null => (),
                Kind::Bool(b) => self.bytes.push(u8::from(*b)),
                Kind::I64(n) => self.doubles.extend_from_slice(&n.to_le_bytes()),
                Kind::U64(n) => self.doubles.extend_from_slice(&n.to_le_bytes()),
                Kind::F32(n) => self.doubles.extend_from_slice(&(*n as f64).to_le_bytes()),
                Kind::F64(n) => self.doubles.extend_from_slice(&n.to_le_bytes()),
                Kind::String(s) => {
                    let n = self.sid(s)?;
                    self.ints.extend_from_slice(&n.to_le_bytes());
                }
                Kind::Blob(b) => {
                    self.i32(b.len())?;
                    self.bytes.extend_from_slice(b);
                }
                Kind::Array(a) => {
                    self.i32(a.len())?;
                    for x in a {
                        self.visit(x)?;
                    }
                }
                Kind::Object(o) => {
                    self.i32(o.len())?;
                    for (k, v) in o {
                        let n = self.sid(k)?;
                        self.ints.extend_from_slice(&n.to_le_bytes());
                        self.visit(v)?;
                    }
                }
            }
            if self.types.len() + self.bytes.len() + self.ints.len() + self.doubles.len()
                > MAX_BLOCK
            {
                return Err(invalid("encoded block exceeds 128 MiB"));
            }
            Ok(())
        }
    }
    let mut writer = Writer {
        strings: Vec::new(),
        lookup: HashMap::new(),
        types: Vec::new(),
        bytes: Vec::new(),
        ints: vec![0; 4],
        doubles: Vec::new(),
    };
    writer.visit(root)?;
    writer.ints[..4].copy_from_slice(&(writer.strings.len() as u32).to_le_bytes());
    let counts = [
        writer.bytes.len() as u32,
        (writer.ints.len() / 4) as u32,
        (writer.doubles.len() / 8) as u32,
    ];
    let mut raw = writer.bytes;
    raw.resize((raw.len() + 3) & !3, 0);
    raw.extend(writer.ints);
    raw.resize((raw.len() + 7) & !7, 0);
    raw.extend(writer.doubles);
    for s in writer.strings {
        raw.extend_from_slice(s.as_bytes());
        raw.push(0);
    }
    raw.extend(writer.types);
    raw.extend_from_slice(&FOOTER.to_le_bytes());
    if raw.len() + 40 > MAX_BLOCK {
        return Err(invalid("encoded block exceeds 128 MiB"));
    }
    let mut out = vec![1, b'3', b'V', b'K'];
    out.extend(format_guid);
    for n in [0, counts[0], counts[1], counts[2], raw.len() as u32] {
        out.extend_from_slice(&n.to_le_bytes());
    }
    out.extend(raw);
    if !decode(&out)?.root.canonical_eq(root) {
        return Err(invalid("typed round-trip verification failed"));
    }
    Ok(out)
}
