//! Color refits of compressed texture blocks for recolored hero textures.
//!
//! A recolored BC7 block keeps its mode, partition, rotation, index selection,
//! p-bits, alpha endpoints and every index bit that feeds alpha; only its color
//! endpoints, and color indices that feed no alpha, are fitted again by least
//! squares. The decoded alpha (Deadlock keeps metalness there) therefore stays
//! byte-exact by construction. Blocks without alpha data (modes 0-3, alpha 255)
//! are written as mode 5 with a constant alpha. A BC3 block keeps its alpha half
//! and refits its color half. `bcdec_rs` is the reference decoder: every refit
//! block is decoded with it, and a block whose alpha differs is an error.

const W2: [i32; 4] = [0, 21, 43, 64];
const W3: [i32; 8] = [0, 9, 18, 27, 37, 46, 55, 64];
const W4: [i32; 16] = [0, 4, 9, 13, 17, 21, 26, 30, 34, 38, 43, 47, 51, 55, 60, 64];
/// BC7 two-subset partitions: bit `t` is the subset of texel `t` (row-major).
const PARTITIONS2: [u16; 64] = [
    0xcccc, 0x8888, 0xeeee, 0xecc8, 0xc880, 0xfeec, 0xfec8, 0xec80, 0xc800, 0xffec, 0xfe80, 0xe800,
    0xffe8, 0xff00, 0xfff0, 0xf000, 0xf710, 0x008e, 0x7100, 0x08ce, 0x008c, 0x7310, 0x3100, 0x8cce,
    0x088c, 0x3110, 0x6666, 0x366c, 0x17e8, 0x0ff0, 0x718e, 0x399c, 0xaaaa, 0xf0f0, 0x5a5a, 0x33cc,
    0x3c3c, 0x55aa, 0x9696, 0xa55a, 0x73ce, 0x13c8, 0x324c, 0x3bdc, 0x6996, 0xc33c, 0x9966, 0x0660,
    0x0272, 0x04e4, 0x4e40, 0x2720, 0xc936, 0x936c, 0x39c6, 0x639c, 0x9336, 0x9cc6, 0x817e, 0xe718,
    0xccf0, 0x0fcc, 0x7744, 0xee22,
];
/// The anchor texel of subset 1 for each two-subset partition.
const ANCHORS2: [usize; 64] = [
    15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 2, 8, 2, 2, 8, 8, 15, 2, 8, 2,
    2, 8, 8, 2, 2, 15, 15, 6, 8, 2, 8, 15, 15, 2, 8, 2, 2, 2, 15, 15, 6, 6, 2, 6, 8, 15, 15, 2, 2, 15,
    15, 15, 15, 15, 2, 2, 15,
];

pub type Texels = [[u8; 4]; 16];

/// Decode one BC7 block with the reference decoder.
pub fn decode_bc7(block: &[u8; 16]) -> Texels {
    let mut rgba = [0u8; 64];
    bcdec_rs::bc7(block, &mut rgba, 16);
    texels(&rgba)
}
/// Decode one BC3 block with the reference decoder.
pub fn decode_bc3(block: &[u8; 16]) -> Texels {
    let mut rgba = [0u8; 64];
    bcdec_rs::bc3(block, &mut rgba, 16);
    texels(&rgba)
}
fn texels(rgba: &[u8; 64]) -> Texels {
    let mut out = [[0; 4]; 16];
    for (texel, value) in out.iter_mut().zip(rgba.chunks_exact(4)) {
        texel.copy_from_slice(value);
    }
    out
}

#[derive(Clone, Copy)]
struct Bits(u128);
impl Bits {
    fn get(self, at: u32, len: u32) -> u32 {
        ((self.0 >> at) & ((1u128 << len) - 1)) as u32
    }
    fn set(&mut self, at: u32, len: u32, value: u32) {
        let mask = ((1u128 << len) - 1) << at;
        self.0 = (self.0 & !mask) | (((value as u128) << at) & mask);
    }
}
fn interpolate(e0: i32, e1: i32, weight: i32) -> i32 {
    (e0 * (64 - weight) + e1 * weight + 32) >> 6
}
/// An endpoint code (with its p-bit, if any) widened to 8 bits.
fn expand(code: u32, bits: u32, p: Option<u32>) -> i32 {
    let (value, width) = match p {
        Some(p) => ((code << 1) | p, bits + 1),
        None => (code, bits),
    };
    if width >= 8 { value as i32 } else { ((value << (8 - width)) | (value >> (2 * width - 8))) as i32 }
}

/// Where one stored channel's two endpoints of one subset live.
#[derive(Clone, Copy)]
struct Field {
    at: [u32; 2],
    bits: u32,
    pbit: [Option<u32>; 2],
}
/// One index stream: texels in order, anchors one bit shorter.
#[derive(Clone, Copy)]
struct IndexSet {
    at: u32,
    bits: u32,
    anchors: [usize; 2],
}
impl IndexSet {
    fn width(&self, texel: usize) -> u32 {
        if self.anchors.contains(&texel) { self.bits - 1 } else { self.bits }
    }
    fn read(&self, block: Bits) -> [u32; 16] {
        let mut out = [0; 16];
        let mut at = self.at;
        for (texel, value) in out.iter_mut().enumerate() {
            let width = self.width(texel);
            *value = block.get(at, width);
            at += width;
        }
        out
    }
    fn write(&self, block: &mut Bits, indices: &[u32; 16]) {
        let mut at = self.at;
        for (texel, value) in indices.iter().enumerate() {
            let width = self.width(texel);
            block.set(at, width, *value);
            at += width;
        }
    }
    fn weights(&self) -> &'static [i32] {
        match self.bits {
            2 => &W2,
            3 => &W3,
            _ => &W4,
        }
    }
}

/// The fields of a BC7 block in a mode that stores alpha (4-7).
struct Layout {
    rotation: u32,
    subset: [usize; 16],
    subsets: usize,
    /// `fields[subset][stored channel]`, stored channels R, G, B, A.
    fields: [[Field; 4]; 2],
    /// The index set that feeds each stored channel.
    feeds: [usize; 4],
    sets: Vec<IndexSet>,
}
impl Layout {
    fn read(block: Bits) -> Option<Self> {
        let mode = (block.0.trailing_zeros()).min(8);
        let single = |at: u32| IndexSet { at, bits: 0, anchors: [0, 0] };
        let field = |at: [u32; 2], bits, pbit| Field { at, bits, pbit };
        let unused = field([0, 0], 0, [None, None]);
        Some(match mode {
            4 => {
                let selection = block.get(7, 1) as usize;
                let mut fields = [[unused; 4]; 2];
                for c in 0..3 {
                    fields[0][c] = field([8 + c as u32 * 10, 13 + c as u32 * 10], 5, [None, None]);
                }
                fields[0][3] = field([38, 44], 6, [None, None]);
                Layout {
                    rotation: block.get(5, 2),
                    subset: [0; 16],
                    subsets: 1,
                    fields,
                    // Index selection 1 swaps the 2-bit and 3-bit streams.
                    feeds: [selection, selection, selection, 1 - selection],
                    sets: vec![IndexSet { bits: 2, ..single(50) }, IndexSet { bits: 3, ..single(81) }],
                }
            }
            5 => {
                let mut fields = [[unused; 4]; 2];
                for c in 0..3 {
                    fields[0][c] = field([8 + c as u32 * 14, 15 + c as u32 * 14], 7, [None, None]);
                }
                fields[0][3] = field([50, 58], 8, [None, None]);
                Layout {
                    rotation: block.get(6, 2),
                    subset: [0; 16],
                    subsets: 1,
                    fields,
                    feeds: [0, 0, 0, 1],
                    sets: vec![IndexSet { bits: 2, ..single(66) }, IndexSet { bits: 2, ..single(97) }],
                }
            }
            6 => {
                let mut fields = [[unused; 4]; 2];
                for c in 0..4 {
                    fields[0][c] = field([7 + c as u32 * 14, 14 + c as u32 * 14], 7, [Some(63), Some(64)]);
                }
                Layout {
                    rotation: 0,
                    subset: [0; 16],
                    subsets: 1,
                    fields,
                    feeds: [0; 4],
                    sets: vec![IndexSet { bits: 4, ..single(65) }],
                }
            }
            7 => {
                let partition = block.get(8, 6) as usize;
                let mut subset = [0; 16];
                for (texel, s) in subset.iter_mut().enumerate() {
                    *s = (PARTITIONS2[partition] >> texel) as usize & 1;
                }
                let mut fields = [[unused; 4]; 2];
                for (s, row) in fields.iter_mut().enumerate() {
                    for (c, slot) in row.iter_mut().enumerate() {
                        let first = 14 + c as u32 * 20 + s as u32 * 10;
                        let p = 94 + s as u32 * 2;
                        *slot = field([first, first + 5], 5, [Some(p), Some(p + 1)]);
                    }
                }
                Layout {
                    rotation: 0,
                    subset,
                    subsets: 2,
                    fields,
                    feeds: [0; 4],
                    sets: vec![IndexSet { at: 98, bits: 2, anchors: [0, ANCHORS2[partition]] }],
                }
            }
            _ => return None,
        })
    }
    /// The stored channel that becomes final channel `channel` after rotation.
    fn stored(&self, channel: usize) -> usize {
        match self.rotation {
            0 => channel,
            r if channel == 3 => r as usize - 1,
            r if channel == r as usize - 1 => 3,
            _ => channel,
        }
    }
    fn endpoint(&self, block: Bits, subset: usize, stored: usize, which: usize) -> i32 {
        let f = self.fields[subset][stored];
        expand(block.get(f.at[which], f.bits), f.bits, f.pbit[which].map(|p| block.get(p, 1)))
    }
    /// This layout's own decode (checked against the reference decoder).
    fn decode(&self, block: Bits) -> Texels {
        let indices: Vec<_> = self.sets.iter().map(|s| s.read(block)).collect();
        let mut out = [[0u8; 4]; 16];
        for (texel, pixel) in out.iter_mut().enumerate() {
            for (channel, value) in pixel.iter_mut().enumerate() {
                let stored = self.stored(channel);
                let set = self.feeds[stored];
                let weight = self.sets[set].weights()[indices[set][texel] as usize];
                let s = self.subset[texel];
                *value = interpolate(
                    self.endpoint(block, s, stored, 0),
                    self.endpoint(block, s, stored, 1),
                    weight,
                ) as u8;
            }
        }
        out
    }
}

/// Least-squares endpoints for `targets` at `weights`, searched over nearby codes.
/// Returns the two codes (p-bits fixed by the caller).
fn fit_channel(
    weights: &[i32],
    targets: &[i32],
    bits: u32,
    pbit: [Option<u32>; 2],
    current: [u32; 2],
) -> [u32; 2] {
    if weights.is_empty() {
        return current;
    }
    let (mut aa, mut ab, mut bb, mut ay, mut by, mut sum) = (0., 0., 0., 0., 0., 0.);
    for (&w, &y) in weights.iter().zip(targets) {
        let (a, b, y) = ((64 - w) as f64 / 64., w as f64 / 64., y as f64);
        aa += a * a;
        ab += a * b;
        bb += b * b;
        ay += a * y;
        by += b * y;
        sum += y;
    }
    let mean = sum / targets.len() as f64;
    let det = aa * bb - ab * ab;
    let ideal = if det.abs() > 1e-9 {
        [(ay * bb - by * ab) / det, (by * aa - ay * ab) / det]
    } else if weights.iter().all(|w| *w == 0) {
        [mean, expand(current[1], bits, pbit[1]) as f64]
    } else if weights.iter().all(|w| *w == 64) {
        [expand(current[0], bits, pbit[0]) as f64, mean]
    } else {
        [mean, mean]
    };
    let top = (1u32 << bits) - 1;
    let nearest = |value: f64, p: Option<u32>| -> i64 {
        let value = value.clamp(0., 255.);
        match p {
            Some(p) => (((value / 255. * ((top << 1 | 1) as f64)) - p as f64) / 2.).round() as i64,
            None => (value / 255. * top as f64).round() as i64,
        }
    };
    let centre = [nearest(ideal[0], pbit[0]), nearest(ideal[1], pbit[1])];
    let mut best = (i64::MAX, current);
    for d0 in [0i64, -1, 1, -2, 2] {
        for d1 in [0i64, -1, 1, -2, 2] {
            let codes = [centre[0] + d0, centre[1] + d1];
            if codes.iter().any(|c| *c < 0 || *c > top as i64) {
                continue;
            }
            let codes = [codes[0] as u32, codes[1] as u32];
            let (e0, e1) = (expand(codes[0], bits, pbit[0]), expand(codes[1], bits, pbit[1]));
            let error: i64 = weights
                .iter()
                .zip(targets)
                .map(|(&w, &y)| {
                    let d = (interpolate(e0, e1, w) - y) as i64;
                    d * d
                })
                .sum();
            if error < best.0 {
                best = (error, codes);
            }
        }
    }
    best.1
}

#[derive(Debug, PartialEq)]
pub enum RefitError {
    /// The refit block decoded with a different alpha.
    Alpha,
    /// This layout's decode disagrees with the reference decoder.
    Decoder,
}

/// `block` with its colors refitted to `target` (RGB used; alpha is kept).
pub fn refit_bc7(block: &[u8; 16], target: &Texels) -> Result<[u8; 16], RefitError> {
    let original = decode_bc7(block);
    let bits = Bits(u128::from_le_bytes(*block));
    let output = match Layout::read(bits) {
        Some(layout) => {
            if layout.decode(bits) != original {
                return Err(RefitError::Decoder);
            }
            refit_layout(&layout, bits, target).0.to_le_bytes()
        }
        // Modes 0-3 have no alpha data (255); a reserved mode decodes to 0.
        None => constant_alpha_mode5(target, original[0][3]),
    };
    let decoded = decode_bc7(&output);
    if decoded.iter().zip(&original).any(|(a, b)| a[3] != b[3]) {
        return Err(RefitError::Alpha);
    }
    Ok(output)
}

/// The refit with the lower color error of two starts: the block's own indices,
/// and (for color indices that feed no alpha) the target colors' bounding box.
fn refit_layout(layout: &Layout, block: Bits, target: &Texels) -> Bits {
    let alpha_set = layout.feeds[layout.stored(3)];
    let free: Vec<usize> = (0..layout.sets.len()).filter(|s| *s != alpha_set).collect();
    let own = iterate(layout, block, target, &free);
    if free.is_empty() {
        return own;
    }
    let spread = iterate(layout, seeded(layout, block, target, &free), target, &free);
    let error = |bits: Bits| -> i64 {
        layout.decode(bits).iter().zip(target).map(|(p, q)| (0..3).map(|c| (p[c] as i64 - q[c] as i64).pow(2)).sum::<i64>()).sum()
    };
    if error(spread) < error(own) { spread } else { own }
}

/// `block` with each free index set's endpoints at the target colors' bounding
/// box (each channel ordered by its covariance with the widest one) and its
/// indices chosen for them. Free sets exist only in single-subset modes.
fn seeded(layout: &Layout, mut block: Bits, target: &Texels, free: &[usize]) -> Bits {
    for &set in free {
        let channels: Vec<usize> = (0..3).filter(|c| layout.feeds[layout.stored(*c)] == set).collect();
        let values = |c: usize| target.iter().map(move |t| t[c] as f64);
        let mean = |c: usize| values(c).sum::<f64>() / 16.;
        let widest = *channels
            .iter()
            .max_by(|a, b| {
                let range = |c: usize| values(c).fold(f64::MIN, f64::max) - values(c).fold(f64::MAX, f64::min);
                range(**a).total_cmp(&range(**b))
            })
            .expect("a free set feeds a color");
        for &c in &channels {
            let covariance: f64 = values(c).zip(values(widest)).map(|(x, w)| (x - mean(c)) * (w - mean(widest))).sum();
            let (lo, hi) = (values(c).fold(f64::MAX, f64::min), values(c).fold(f64::MIN, f64::max));
            let ends = if covariance < 0. { [hi, lo] } else { [lo, hi] };
            let f = layout.fields[0][layout.stored(c)];
            let top = ((1u32 << f.bits) - 1) as f64;
            for (at, value) in f.at.iter().zip(ends) {
                block.set(*at, f.bits, (value / 255. * top).round() as u32);
            }
        }
        let weights = layout.sets[set].weights();
        let mut indices = layout.sets[set].read(block);
        for (texel, index) in indices.iter_mut().enumerate() {
            let mut best = (i64::MAX, 0);
            for (candidate, &w) in weights.iter().enumerate() {
                let error: i64 = channels
                    .iter()
                    .map(|&c| {
                        let stored = layout.stored(c);
                        let v = interpolate(layout.endpoint(block, 0, stored, 0), layout.endpoint(block, 0, stored, 1), w);
                        ((v - target[texel][c] as i32) as i64).pow(2)
                    })
                    .sum();
                if error < best.0 {
                    best = (error, candidate as u32);
                }
            }
            *index = best.1;
        }
        anchored(layout, &mut block, set, &mut indices);
        layout.sets[set].write(&mut block, &indices);
    }
    block
}

/// The anchor texel's index has no top bit: when it needs one, the set's
/// endpoints swap and its indices count from the other end.
fn anchored(layout: &Layout, block: &mut Bits, set: usize, indices: &mut [u32; 16]) {
    let bits = layout.sets[set].bits;
    if indices[0] < 1 << (bits - 1) {
        return;
    }
    for value in indices.iter_mut() {
        *value = (1 << bits) - 1 - *value;
    }
    for stored in (0..4).filter(|c| layout.feeds[*c] == set) {
        let f = layout.fields[0][stored];
        let (a, b) = (block.get(f.at[0], f.bits), block.get(f.at[1], f.bits));
        block.set(f.at[0], f.bits, b);
        block.set(f.at[1], f.bits, a);
    }
}

fn iterate(layout: &Layout, mut block: Bits, target: &Texels, free: &[usize]) -> Bits {
    let mut indices: Vec<_> = layout.sets.iter().map(|s| s.read(block)).collect();
    let fit = |block: &mut Bits, indices: &[[u32; 16]]| {
        for s in 0..layout.subsets {
            let texels: Vec<usize> = (0..16).filter(|t| layout.subset[*t] == s).collect();
            for channel in 0..3 {
                let stored = layout.stored(channel);
                let set = &layout.sets[layout.feeds[stored]];
                let weights: Vec<i32> = texels
                    .iter()
                    .map(|t| set.weights()[indices[layout.feeds[stored]][*t] as usize])
                    .collect();
                let targets: Vec<i32> = texels.iter().map(|t| target[*t][channel] as i32).collect();
                let f = layout.fields[s][stored];
                let pbit = [f.pbit[0].map(|p| block.get(p, 1)), f.pbit[1].map(|p| block.get(p, 1))];
                let current = [block.get(f.at[0], f.bits), block.get(f.at[1], f.bits)];
                let codes = fit_channel(&weights, &targets, f.bits, pbit, current);
                block.set(f.at[0], f.bits, codes[0]);
                block.set(f.at[1], f.bits, codes[1]);
            }
        }
    };
    for _ in 0..3 {
        fit(&mut block, &indices);
        if free.is_empty() {
            break;
        }
        // Color indices that feed no alpha are chosen again for the new endpoints
        // (modes 4 and 5 only, which have one subset).
        for &set in free {
            let channels: Vec<usize> =
                (0..3).filter(|c| layout.feeds[layout.stored(*c)] == set).collect();
            let weights = layout.sets[set].weights();
            for texel in 0..16 {
                let mut best = (i64::MAX, 0);
                for (index, &w) in weights.iter().enumerate() {
                    let error: i64 = channels
                        .iter()
                        .map(|&c| {
                            let stored = layout.stored(c);
                            let value = interpolate(
                                layout.endpoint(block, 0, stored, 0),
                                layout.endpoint(block, 0, stored, 1),
                                w,
                            );
                            let d = (value - target[texel][c] as i32) as i64;
                            d * d
                        })
                        .sum();
                    if error < best.0 {
                        best = (error, index as u32);
                    }
                }
                indices[set][texel] = best.1;
            }
        }
    }
    fit(&mut block, &indices);
    for &set in free {
        anchored(layout, &mut block, set, &mut indices[set]);
    }
    for (set, values) in layout.sets.iter().zip(&indices) {
        set.write(&mut block, values);
    }
    block
}

/// A mode 5 block with the given colors and one alpha value everywhere.
fn constant_alpha_mode5(target: &Texels, alpha: u8) -> [u8; 16] {
    let mut block = Bits(1 << 5);
    block.set(50, 8, alpha as u32);
    block.set(58, 8, alpha as u32);
    let layout = Layout::read(block).expect("mode 5 layout");
    refit_layout(&layout, block, target).0.to_le_bytes()
}

// BC3: the color half decodes like BC1 in four-color mode, whatever the endpoint order.
fn bc1_value(c0: u32, c1: u32, index: u32, bits: u32) -> i32 {
    let (scale, bias, mix, mix_bias, shift) =
        if bits == 5 { (527, 23, 351, 61, 7) } else { (259, 33, 2763, 1039, 11) };
    (match index {
        0 => (c0 * scale + bias) >> 6,
        1 => (c1 * scale + bias) >> 6,
        2 => ((2 * c0 + c1) * mix + mix_bias) >> shift,
        _ => ((c0 + 2 * c1) * mix + mix_bias) >> shift,
    }) as i32
}
const BC1_FIELDS: [(u32, u32); 3] = [(11, 5), (5, 6), (0, 5)];

/// A BC3 block with its colors refitted to `target`; the alpha half is kept.
pub fn refit_bc3(block: &[u8; 16], target: &Texels) -> Result<[u8; 16], RefitError> {
    let original = decode_bc3(block);
    let mut c = [
        u16::from_le_bytes([block[8], block[9]]) as u32,
        u16::from_le_bytes([block[10], block[11]]) as u32,
    ];
    let packed = u32::from_le_bytes(block[12..16].try_into().unwrap());
    let mut indices: [u32; 16] = std::array::from_fn(|t| (packed >> (2 * t)) & 3);
    // Weight of the second endpoint per index, in 64ths, for the least squares.
    const TOWARD_C1: [i32; 4] = [0, 64, 21, 43];
    for round in 0..4 {
        for (channel, &(shift, bits)) in BC1_FIELDS.iter().enumerate() {
            let top = (1u32 << bits) - 1;
            let codes = [(c[0] >> shift) & top, (c[1] >> shift) & top];
            let weights: Vec<i32> = indices.iter().map(|i| TOWARD_C1[*i as usize]).collect();
            let targets: Vec<i32> = target.iter().map(|t| t[channel] as i32).collect();
            // Least squares in 8-bit units, then the exact decode over nearby codes.
            let guess = fit_channel(&weights, &targets, 8, [None, None], [codes[0] << (8 - bits), codes[1] << (8 - bits)]);
            let centre = [(guess[0] * top + 127) / 255, (guess[1] * top + 127) / 255];
            let mut best = (i64::MAX, codes);
            for d0 in [0i64, -1, 1, -2, 2] {
                for d1 in [0i64, -1, 1, -2, 2] {
                    let pair = [centre[0] as i64 + d0, centre[1] as i64 + d1];
                    if pair.iter().any(|v| *v < 0 || *v > top as i64) {
                        continue;
                    }
                    let pair = [pair[0] as u32, pair[1] as u32];
                    let error: i64 = indices
                        .iter()
                        .zip(&targets)
                        .map(|(i, y)| {
                            let d = (bc1_value(pair[0], pair[1], *i, bits) - y) as i64;
                            d * d
                        })
                        .sum();
                    if error < best.0 {
                        best = (error, pair);
                    }
                }
            }
            for (slot, code) in c.iter_mut().zip(best.1) {
                *slot = (*slot & !(top << shift)) | (code << shift);
            }
        }
        if round == 3 {
            break;
        }
        for (texel, index) in indices.iter_mut().enumerate() {
            let mut best = (i64::MAX, 0);
            for candidate in 0..4 {
                let error: i64 = BC1_FIELDS
                    .iter()
                    .enumerate()
                    .map(|(channel, &(shift, bits))| {
                        let top = (1u32 << bits) - 1;
                        let v = bc1_value((c[0] >> shift) & top, (c[1] >> shift) & top, candidate, bits);
                        let d = (v - target[texel][channel] as i32) as i64;
                        d * d
                    })
                    .sum();
                if error < best.0 {
                    best = (error, candidate);
                }
            }
            *index = best.1;
        }
    }
    let mut output = *block;
    output[8..10].copy_from_slice(&(c[0] as u16).to_le_bytes());
    output[10..12].copy_from_slice(&(c[1] as u16).to_le_bytes());
    let packed = indices.iter().enumerate().fold(0u32, |acc, (t, i)| acc | (i << (2 * t)));
    output[12..16].copy_from_slice(&packed.to_le_bytes());
    let decoded = decode_bc3(&output);
    if decoded.iter().zip(&original).any(|(a, b)| a[3] != b[3]) {
        return Err(RefitError::Alpha);
    }
    Ok(output)
}
