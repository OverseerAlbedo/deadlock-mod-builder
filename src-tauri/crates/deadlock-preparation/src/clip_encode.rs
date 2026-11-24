//! Writes new poses into a compiled AnimGraph 2 clip (`.vnmclip_c`): the
//! inverse of `clip_decode` (same layout, derived from the public
//! ValveResourceFormat source). Only the pose data changes
//! (`m_compressedPoseData`, `m_compressedPoseOffsets` and the per-track
//! `m_trackCompressionSettings`); every other key, every other block, the
//! frame count, the length, events and root motion stay the game's, which is
//! what `motion::validate_clip` requires of a replacement.
//!
//! Layout (the one `clip_decode` reads and `motion::validate_pose` checks):
//! per frame, per track in track order, three 16-bit words of rotation unless
//! the rotation is static, three of translation unless static, one of scale
//! unless static; frame `i` starts at word `i × words per frame`; every
//! track's `m_nTrackReadOffset` is the word where its values start in a frame.
//! A rotation keeps the three smaller quaternion components, each mapped from
//! ±1/√2 to 0..32767, with the index of the largest one in the top bits of
//! the first two words (the largest is made positive). A translation axis or
//! the scale is `round((value − start) / length × 65535)` against the track's
//! range, quantized against the float values the clip stores. A static
//! translation is the start of its three ranges.
//!
//! Track settings policy, matching what the game's compiler writes (every
//! game clip checked: animated rotations carry an identity constant, moving
//! translation axes never have a zero-length range):
//! - a static rotation, translation or scale whose new values all equal the
//!   stored value (within `STATIC_ROTATION` / `STATIC_POSITION` /
//!   `STATIC_SCALE`) stays static and keeps its stored settings byte for byte;
//! - with `Statics::Rewrite`, a static rotation or translation whose new
//!   value is the same in every frame but differs from the stored one keeps
//!   the track static with the new value stored (same words per frame);
//! - a static track whose new values move is promoted to animated: ranges
//!   from the new values (zero spread gets `MIN_LENGTH`), identity constant;
//! - an animated track stays animated (never demoted). With `Ranges::Keep` its
//!   stored ranges are kept when every new value fits inside them, otherwise
//!   they are taken from the new values; `Ranges::Fresh` always takes them
//!   from the new values; `Ranges::Never` always keeps them (values outside
//!   are clamped to the range, listed in `ranges_clamped`).
//! Number kinds (f32/f64, integer kinds) of every rewritten value stay the
//! kinds the original stored.
use crate::clip_decode::{Clip, Local};
use deadlock_resource::{binary, container, Kind, Value};
use std::collections::BTreeMap;

pub type Result<T> = std::result::Result<T, String>;

/// A rotation within this angle (radians) of the stored constant keeps a
/// static rotation static (well below one quantization step, about 8.6e-5).
pub const STATIC_ROTATION: f64 = 1e-6;
/// A position within this distance (inches, per axis) keeps translation static.
pub const STATIC_POSITION: f64 = 1e-6;
/// A scale within this keeps scale static.
pub const STATIC_SCALE: f64 = 1e-7;
/// The largest angle (radians) one stored rotation can be off. Each stored
/// component is off by at most half a step δ = √2/32767/2; the rebuilt largest
/// component w ≥ 1/2 moves by Σ|cᵢ|δ/w ≤ 3δ, so |Δq| ≤ √12·δ and the angle
/// (2·|Δq|) ≤ √12·√2/32767 ≈ 1.495e-4 rad = 0.00857°.
pub const ROTATION_BOUND: f64 = 3.464_101_615_137_754_6 * 1.414_213_562_373_095 / 32767.;
/// Range length used for an axis whose new values do not spread.
pub const MIN_LENGTH: f64 = 1e-4;

/// How animated tracks get their ranges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ranges {
    /// Keep the stored ranges when every new value fits, else re-fit them.
    Keep,
    /// Always derive ranges from the new values.
    Fresh,
    /// Never change a stored range (values outside it are clamped).
    Never,
}

/// What happens to a static track whose new value differs from its stored one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Statics {
    /// Always promote it to animated.
    Promote,
    /// When the new value is the same in every frame, store it as the new
    /// fixed value and keep the track static; a track whose values move is
    /// still promoted.
    Rewrite,
}

/// What `encode` changed in the track settings (track indices).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Report {
    /// Tracks whose rotation was static and now moves.
    pub rotation_promoted: Vec<usize>,
    pub translation_promoted: Vec<usize>,
    pub scale_promoted: Vec<usize>,
    /// Static rotations and translations that stay static with a new value.
    pub rotation_constant_rewritten: Vec<usize>,
    pub translation_constant_rewritten: Vec<usize>,
    /// Animated tracks whose stored ranges changed.
    pub ranges_changed: Vec<usize>,
    /// `Ranges::Never`: tracks with a value outside its kept range (clamped).
    pub ranges_clamped: Vec<usize>,
    /// Whether `m_trackCompressionSettings` / `m_compressedPoseOffsets` differ
    /// from the original at all (false: only the pose blob changed).
    pub settings_changed: bool,
    pub words_per_frame: (usize, usize),
}
impl Report {
    /// Whether any static track was made to move.
    pub fn promoted_any(&self) -> bool {
        !(self.rotation_promoted.is_empty() && self.translation_promoted.is_empty() && self.scale_promoted.is_empty())
    }
}

fn field<'a>(v: &'a Value, key: &str) -> Result<&'a Value> {
    v.get(key).ok_or_else(|| format!("compiled clip lacks {key}"))
}
fn field_mut<'a>(v: &'a mut Value, key: &str) -> Result<&'a mut Value> {
    v.get_mut(key).ok_or_else(|| format!("compiled clip lacks {key}"))
}
fn num(v: &Value) -> Result<f64> {
    match v.kind {
        Kind::F32(n) => Ok(n as f64),
        Kind::F64(n) => Ok(n),
        Kind::I64(n) => Ok(n as f64),
        Kind::U64(n) => Ok(n as f64),
        _ => Err("expected a number".into()),
    }
}
/// The value as the clip will store it (rounded to f32 when it is an f32).
fn stored(kind: &Kind, x: f64) -> f64 {
    match kind {
        Kind::F32(_) => x as f32 as f64,
        _ => x,
    }
}
/// Writes a number keeping the value's kind.
fn put(v: &mut Value, x: f64) -> Result<()> {
    v.kind = match v.kind {
        Kind::F32(_) => Kind::F32(x as f32),
        Kind::F64(_) => Kind::F64(x),
        Kind::I64(_) => Kind::I64(x.round() as i64),
        Kind::U64(_) => Kind::U64(x.round().max(0.) as u64),
        _ => return Err("expected a number to overwrite".into()),
    };
    Ok(())
}
fn put_bool(v: &mut Value, b: bool) -> Result<()> {
    if !matches!(v.kind, Kind::Bool(_)) {
        return Err("expected a boolean to overwrite".into());
    }
    v.kind = Kind::Bool(b);
    Ok(())
}
fn flag(v: &Value, key: &str) -> Result<bool> {
    match field(v, key)?.kind {
        Kind::Bool(b) => Ok(b),
        _ => Err(format!("{key} is not a boolean")),
    }
}

/// The three stored words of a rotation (largest component made positive and
/// left out; its index in the top bits of the first two words).
pub fn pack(q: [f64; 4]) -> [u16; 3] {
    let mut q = crate::clip_decode::normalize(q);
    let largest = (0..4).max_by(|&a, &b| q[a].abs().total_cmp(&q[b].abs())).unwrap_or(3);
    if q[largest] < 0. {
        q = q.map(|x| -x);
    }
    let mut out = [0u16; 3];
    let mut at = 0;
    for (j, x) in q.iter().enumerate() {
        if j != largest {
            out[at] = ((x + 1. / 2f64.sqrt()) / 2f64.sqrt() * 32767.).round().clamp(0., 32767.) as u16;
            at += 1;
        }
    }
    out[0] |= ((largest as u16 >> 1) & 1) << 15;
    out[1] |= (largest as u16 & 1) << 15;
    out
}

/// Angle in radians between two rotations (either sign).
pub fn angle(a: [f64; 4], b: [f64; 4]) -> f64 {
    let d = (0..4).map(|k| a[k] * b[k]).sum::<f64>().abs().min(1.);
    2. * d.acos()
}

fn word(x: f64, start: f64, length: f64) -> u16 {
    if length <= 0. {
        return 0;
    }
    ((x - start) / length * 65535.).round().clamp(0., 65535.) as u16
}

struct RangeSlot {
    start: f64,
    length: f64,
}

/// A range covering `values` as the clip will store it: start rounded down to
/// the stored float, length rounded up so start + length still covers the max.
fn fresh_range(kind_start: &Kind, kind_length: &Kind, values: impl Iterator<Item = f64> + Clone) -> RangeSlot {
    let min = values.clone().fold(f64::INFINITY, f64::min);
    let max = values.fold(f64::NEG_INFINITY, f64::max);
    let mut start = stored(kind_start, min);
    if start > min {
        start = match kind_start {
            Kind::F32(_) => f32_down(min) as f64,
            _ => min,
        };
    }
    let mut length = stored(kind_length, (max - start).max(MIN_LENGTH));
    if start + length < max {
        length = match kind_length {
            Kind::F32(_) => f32_up(max - start) as f64,
            _ => max - start,
        };
    }
    RangeSlot { start, length }
}
fn f32_down(x: f64) -> f32 {
    let f = x as f32;
    if f as f64 > x { f32::from_bits(if f > 0. { f.to_bits() - 1 } else { f.to_bits() + 1 }) } else { f }
}
fn f32_up(x: f64) -> f32 {
    let f = x as f32;
    if (f as f64) < x { f32::from_bits(if f >= 0. { f.to_bits() + 1 } else { f.to_bits() - 1 }) } else { f }
}

fn read_range(v: &Value) -> Result<(f64, f64)> {
    Ok((num(field(v, "m_flRangeStart")?)?, num(field(v, "m_flRangeLength")?)?))
}
fn write_range(v: &mut Value, r: &RangeSlot) -> Result<()> {
    put(field_mut(v, "m_flRangeStart")?, r.start)?;
    put(field_mut(v, "m_flRangeLength")?, r.length)
}
fn range_kinds(v: &Value) -> Result<(Kind, Kind)> {
    Ok((field(v, "m_flRangeStart")?.kind.clone(), field(v, "m_flRangeLength")?.kind.clone()))
}

const AXES: [&str; 3] = ["m_translationRangeX", "m_translationRangeY", "m_translationRangeZ"];

/// Writes `frames` (per frame, per track, parent-space values as
/// `clip_decode` returns them) into the clip's DATA root in place.
pub fn encode(clip: &mut Value, frames: &[Vec<Local>], ranges: Ranges) -> Result<Report> {
    encode_with(clip, frames, ranges, Statics::Promote)
}

/// `encode` with a choice for static tracks whose value changes.
pub fn encode_with(clip: &mut Value, frames: &[Vec<Local>], ranges: Ranges, statics: Statics) -> Result<Report> {
    let count = field(clip, "m_nNumFrames")?.integer().map_err(|e| e.to_string())? as usize;
    if frames.len() != count {
        return Err(format!("{} frames given, the clip has {count}", frames.len()));
    }
    let before_settings = field(clip, "m_trackCompressionSettings")?.clone();
    let before_offsets = field(clip, "m_compressedPoseOffsets")?.clone();
    let tracks_len = before_settings.array().map_err(|e| e.to_string())?.len();
    if frames.iter().any(|f| f.len() != tracks_len) {
        return Err(format!("every frame needs {tracks_len} tracks"));
    }
    if frames.iter().flatten().any(|l| l.position.iter().chain(&l.rotation).chain([&l.scale]).any(|x| !x.is_finite())) {
        return Err("poses hold a non-finite value".into());
    }
    let old_words = {
        let mut w = 0;
        for t in before_settings.array().map_err(|e| e.to_string())? {
            w += if flag(t, "m_bIsRotationStatic")? { 0 } else { 3 } + if flag(t, "m_bIsTranslationStatic")? { 0 } else { 3 } + if flag(t, "m_bIsScaleStatic")? { 0 } else { 1 };
        }
        w
    };
    let mut report = Report::default();
    enum Slot {
        Rotation,
        Translation([RangeSlot; 3]),
        Scale(RangeSlot),
    }
    let mut plan: Vec<Vec<Slot>> = vec![];
    {
        let settings = field_mut(clip, "m_trackCompressionSettings")?;
        let Kind::Array(tracks) = &mut settings.kind else { return Err("track settings are not an array".into()) };
        let mut offset = 0usize;
        for (i, t) in tracks.iter_mut().enumerate() {
            let mut slots = vec![];
            let read_offset = offset;
            // Rotation.
            let constant: Vec<f64> = field(t, "m_constantRotation")?.array().map_err(|e| e.to_string())?.iter().map(num).collect::<Result<_>>()?;
            if constant.len() != 4 {
                return Err("constant rotation needs 4 values".into());
            }
            // The decoder plays the stored constant normalized; compare with that.
            let constant = crate::clip_decode::normalize([constant[0], constant[1], constant[2], constant[3]]);
            if flag(t, "m_bIsRotationStatic")? {
                let steady = frames.iter().all(|f| angle(f[i].rotation, frames[0][i].rotation) <= STATIC_ROTATION);
                if statics == Statics::Rewrite && steady && angle(frames[0][i].rotation, constant) > STATIC_ROTATION {
                    let q = crate::clip_decode::normalize(frames[0][i].rotation);
                    let Kind::Array(c) = &mut field_mut(t, "m_constantRotation")?.kind else { unreachable!() };
                    for (k, x) in q.into_iter().enumerate() {
                        put(&mut c[k], x)?;
                    }
                    report.rotation_constant_rewritten.push(i);
                } else if frames.iter().any(|f| angle(f[i].rotation, constant) > STATIC_ROTATION) {
                    put_bool(field_mut(t, "m_bIsRotationStatic")?, false)?;
                    let Kind::Array(c) = &mut field_mut(t, "m_constantRotation")?.kind else { unreachable!() };
                    for (k, x) in [0., 0., 0., 1.].into_iter().enumerate() {
                        put(&mut c[k], x)?;
                    }
                    report.rotation_promoted.push(i);
                    slots.push(Slot::Rotation);
                    offset += 3;
                }
            } else {
                slots.push(Slot::Rotation);
                offset += 3;
            }
            // Translation.
            let axes = [read_range(field(t, AXES[0])?)?, read_range(field(t, AXES[1])?)?, read_range(field(t, AXES[2])?)?];
            let translation_static = flag(t, "m_bIsTranslationStatic")?;
            let moves = |k: usize| frames.iter().any(|f| (f[i].position[k] - axes[k].0).abs() > STATIC_POSITION);
            let steady = (0..3).all(|k| frames.iter().all(|f| (f[i].position[k] - frames[0][i].position[k]).abs() <= STATIC_POSITION));
            let differs = translation_static && (0..3).any(moves);
            if differs && statics == Statics::Rewrite && steady {
                // The new fixed value is the start of each axis range (kinds kept).
                for k in 0..3 {
                    put(field_mut(field_mut(t, AXES[k])?, "m_flRangeStart")?, frames[0][i].position[k])?;
                }
                report.translation_constant_rewritten.push(i);
            } else if !translation_static || differs {
                let promote = differs;
                let fits = (0..3).all(|k| frames.iter().all(|f| {
                    let x = f[i].position[k];
                    x >= axes[k].0 && x <= axes[k].0 + axes[k].1
                }));
                let keep = !promote && (ranges == Ranges::Never || (ranges == Ranges::Keep && fits));
                if !promote && ranges == Ranges::Never && !fits {
                    report.ranges_clamped.push(i);
                }
                let mut new = vec![];
                for k in 0..3 {
                    let r = field_mut(t, AXES[k])?;
                    let slot = if keep {
                        RangeSlot { start: axes[k].0, length: axes[k].1 }
                    } else {
                        let (ks, kl) = range_kinds(r)?;
                        let s = fresh_range(&ks, &kl, frames.iter().map(|f| f[i].position[k]));
                        write_range(r, &s)?;
                        s
                    };
                    new.push(slot);
                }
                if promote {
                    put_bool(field_mut(t, "m_bIsTranslationStatic")?, false)?;
                    report.translation_promoted.push(i);
                } else if !keep {
                    report.ranges_changed.push(i);
                }
                let mut it = new.into_iter();
                slots.push(Slot::Translation([it.next().unwrap(), it.next().unwrap(), it.next().unwrap()]));
                offset += 3;
            }
            // Scale.
            let scale = read_range(field(t, "m_scaleRange")?)?;
            let scale_static = flag(t, "m_bIsScaleStatic")?;
            let promote = scale_static && frames.iter().any(|f| (f[i].scale - scale.0).abs() > STATIC_SCALE);
            if !scale_static || promote {
                let fits = frames.iter().all(|f| f[i].scale >= scale.0 && f[i].scale <= scale.0 + scale.1);
                let keep = !promote && (ranges == Ranges::Never || (ranges == Ranges::Keep && fits));
                if !promote && ranges == Ranges::Never && !fits && !report.ranges_clamped.contains(&i) {
                    report.ranges_clamped.push(i);
                }
                let r = field_mut(t, "m_scaleRange")?;
                let slot = if keep {
                    RangeSlot { start: scale.0, length: scale.1 }
                } else {
                    let (ks, kl) = range_kinds(r)?;
                    let s = fresh_range(&ks, &kl, frames.iter().map(|f| f[i].scale));
                    write_range(r, &s)?;
                    s
                };
                if promote {
                    put_bool(field_mut(t, "m_bIsScaleStatic")?, false)?;
                    report.scale_promoted.push(i);
                } else if !keep && !report.ranges_changed.contains(&i) {
                    report.ranges_changed.push(i);
                }
                slots.push(Slot::Scale(slot));
                offset += 1;
            }
            // The game writes it on every track; a clip without it keeps none.
            if let Some(v) = t.get_mut("m_nTrackReadOffset") {
                put(v, read_offset as f64)?;
            }
            plan.push(slots);
        }
        report.words_per_frame = (old_words, offset);
    }
    let words = report.words_per_frame.1;
    let mut blob = Vec::with_capacity(2 * words * count);
    for frame in frames {
        for (i, slots) in plan.iter().enumerate() {
            let l = &frame[i];
            for slot in slots {
                match slot {
                    Slot::Rotation => {
                        for w in pack(l.rotation) {
                            blob.extend_from_slice(&w.to_le_bytes());
                        }
                    }
                    Slot::Translation(r) => {
                        for k in 0..3 {
                            blob.extend_from_slice(&word(l.position[k], r[k].start, r[k].length).to_le_bytes());
                        }
                    }
                    Slot::Scale(r) => blob.extend_from_slice(&word(l.scale, r.start, r.length).to_le_bytes()),
                }
            }
        }
    }
    let Kind::Blob(data) = &mut field_mut(clip, "m_compressedPoseData")?.kind else {
        return Err("compressed pose data is not a blob".into());
    };
    *data = blob;
    {
        let Kind::Array(offsets) = &mut field_mut(clip, "m_compressedPoseOffsets")?.kind else {
            return Err("pose offsets are not an array".into());
        };
        if offsets.len() != count {
            return Err("pose offsets do not match the frame count".into());
        }
        for (i, o) in offsets.iter_mut().enumerate() {
            put(o, (i * words) as f64)?;
        }
    }
    report.settings_changed = !field(clip, "m_trackCompressionSettings")?.canonical_eq(&before_settings)
        || !field(clip, "m_compressedPoseOffsets")?.canonical_eq(&before_offsets);
    Ok(report)
}

/// The compiled clip with `frames` written into its pose data; every block
/// other than DATA byte for byte, DATA re-encoded (uncompressed KV3) and
/// checked to decode to exactly the edited tree.
pub fn reencode(raw: &[u8], frames: &[Vec<Local>], ranges: Ranges) -> Result<(Vec<u8>, Report)> {
    reencode_with(raw, frames, ranges, Statics::Promote)
}

/// `reencode` with a choice for static tracks whose value changes.
pub fn reencode_with(raw: &[u8], frames: &[Vec<Local>], ranges: Ranges, statics: Statics) -> Result<(Vec<u8>, Report)> {
    let layout = container::layout(raw).map_err(|e| e.to_string())?;
    let block = container::unique(&layout.blocks, "DATA").map_err(|e| e.to_string())?;
    let mut doc = binary::decode(container::payload(raw, block).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let report = encode_with(&mut doc.root, frames, ranges, statics)?;
    let bytes = binary::encode(&doc.root, doc.format_guid).map_err(|e| e.to_string())?;
    if !binary::decode(&bytes).map_err(|e| e.to_string())?.root.canonical_eq(&doc.root) {
        return Err("KV3 round trip changed the data".into());
    }
    let out = container::rebuild(raw, &BTreeMap::from([("DATA".to_string(), bytes)])).map_err(|e| e.to_string())?;
    Ok((out, report))
}

/// Per track, whether the clip stores its rotation and its translation as
/// static (fixed values).
pub fn static_tracks(clip: &Value) -> Result<Vec<(bool, bool)>> {
    field(clip, "m_trackCompressionSettings")?
        .array()
        .map_err(|e| e.to_string())?
        .iter()
        .map(|t| Ok((flag(t, "m_bIsRotationStatic")?, flag(t, "m_bIsTranslationStatic")?)))
        .collect()
}

/// The largest error the encoding itself may add: rotation in radians, and per
/// track the translation and scale half-steps of its stored ranges.
pub fn quantization(clip: &Value) -> Result<(f64, Vec<(f64, f64)>)> {
    let mut tracks = vec![];
    for t in field(clip, "m_trackCompressionSettings")?.array().map_err(|e| e.to_string())? {
        let tr = if flag(t, "m_bIsTranslationStatic")? {
            0.
        } else {
            let l: Vec<f64> = AXES.iter().map(|a| read_range(field(t, a)?).map(|r| r.1 / 65535. / 2.)).collect::<Result<_>>()?;
            (l[0] * l[0] + l[1] * l[1] + l[2] * l[2]).sqrt()
        };
        let sc = if flag(t, "m_bIsScaleStatic")? { 0. } else { read_range(field(t, "m_scaleRange")?)?.1 / 65535. / 2. };
        tracks.push((tr, sc));
    }
    Ok((ROTATION_BOUND, tracks))
}

/// Decodes the frames of an encoded DATA root (a convenience for checks).
pub fn decoded(clip: &Value) -> Result<Clip> {
    Clip::from_value(clip)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clip_decode::rotation;

    #[test]
    fn packed_rotations_decode_within_the_stated_bound() {
        for i in 0..2000 {
            let f = i as f64;
            let q = crate::clip_decode::normalize([(f * 0.37).sin(), (f * 0.91).cos(), (f * 1.3).sin() * 0.5, (f * 0.11).cos()]);
            let back = crate::clip_decode::normalize(rotation(pack(q)));
            assert!(angle(q, back) <= ROTATION_BOUND, "{q:?} {back:?}");
        }
    }

    #[test]
    fn ranges_cover_their_values_as_stored() {
        let vals = [1.000_000_1f64, 7.3, -2.123_456_789];
        let r = fresh_range(&Kind::F32(0.), &Kind::F32(0.), vals.iter().copied());
        assert!(r.start <= -2.123_456_789 && r.start + r.length >= 7.3);
        assert_eq!(r.start, r.start as f32 as f64);
        assert_eq!(r.length, r.length as f32 as f64);
    }

    /// A hand-made clip DATA root: track 0 static rotation and translation,
    /// track 1 animated rotation and translation.
    pub(crate) fn clip_value(frames: usize) -> Value {
        let n = |x: f64| Value::new(Kind::F32(x as f32));
        let b = |x: bool| Value::new(Kind::Bool(x));
        let obj = |rows: Vec<(&str, Value)>| Value::new(Kind::Object(rows.into_iter().map(|(k, v)| (k.to_string(), v)).collect()));
        let range = |s: f64, l: f64| obj(vec![("m_flRangeStart", n(s)), ("m_flRangeLength", n(l))]);
        let track = |rs: bool, ts: bool, t: [f64; 3]| {
            obj(vec![
                ("m_translationRangeX", range(t[0], if ts { 0. } else { 2. })),
                ("m_translationRangeY", range(t[1], if ts { 0. } else { 2. })),
                ("m_translationRangeZ", range(t[2], if ts { 0. } else { 2. })),
                ("m_scaleRange", range(1., 0.)),
                ("m_constantRotation", Value::new(Kind::Array([0., 0., 0., 1.].map(n).to_vec()))),
                ("m_bIsRotationStatic", b(rs)),
                ("m_bIsTranslationStatic", b(ts)),
                ("m_bIsScaleStatic", b(true)),
                ("m_nTrackReadOffset", Value::new(Kind::I64(0))),
            ])
        };
        obj(vec![
            ("m_skeleton", Value::new(Kind::String("t".into()))),
            ("m_nNumFrames", Value::new(Kind::I64(frames as i64))),
            ("m_flDuration", n(1.)),
            ("m_bIsAdditive", b(false)),
            ("m_trackCompressionSettings", Value::new(Kind::Array(vec![track(true, true, [0., 0., 5.]), track(false, false, [0., 0., 0.])]))),
            ("m_compressedPoseOffsets", Value::new(Kind::Array((0..frames).map(|_| Value::new(Kind::I64(0))).collect()))),
            ("m_compressedPoseData", Value::new(Kind::Blob(vec![]))),
        ])
    }
    fn turned(z: f64) -> [f64; 4] {
        [0., 0., (z / 2.).sin(), (z / 2.).cos()]
    }

    #[test]
    fn steady_new_values_are_rewritten_and_moving_ones_promoted() {
        let still = |p: [f64; 3], q: [f64; 4]| Local { position: p, rotation: q, scale: 1. };
        // Track 0 steady at a new value: rewritten with Rewrite, promoted with Promote.
        let frames: Vec<Vec<Local>> = (0..3).map(|_| vec![still([1., 0., 5.], turned(0.3)), still([0.5, 0.5, 0.5], turned(0.1))]).collect();
        let mut v = clip_value(3);
        let r = encode_with(&mut v, &frames, Ranges::Keep, Statics::Rewrite).unwrap();
        assert_eq!((r.rotation_constant_rewritten.clone(), r.translation_constant_rewritten.clone()), (vec![0], vec![0]));
        assert!(!r.promoted_any());
        assert_eq!(r.words_per_frame, (6, 6));
        let back = decoded(&v).unwrap();
        assert!(angle(back.frames[2][0].rotation, turned(0.3)) < 1e-6);
        assert!((back.frames[2][0].position[0] - 1.).abs() < 1e-6);
        let mut v = clip_value(3);
        let r = encode_with(&mut v, &frames, Ranges::Keep, Statics::Promote).unwrap();
        assert_eq!((r.rotation_promoted.clone(), r.translation_promoted.clone()), (vec![0], vec![0]));
        // Moving values are promoted even with Rewrite.
        let moving: Vec<Vec<Local>> = (0..3).map(|f| vec![still([f as f64, 0., 5.], turned(0.1 * f as f64)), still([0.5; 3], turned(0.1))]).collect();
        let mut v = clip_value(3);
        let r = encode_with(&mut v, &moving, Ranges::Keep, Statics::Rewrite).unwrap();
        assert_eq!((r.rotation_promoted.clone(), r.translation_promoted.clone()), (vec![0], vec![0]));
        assert_eq!(r.words_per_frame, (6, 12));
    }

    #[test]
    fn ranges_are_refitted_kept_or_never_changed() {
        let still = |p: [f64; 3]| Local { position: p, rotation: [0., 0., 0., 1.], scale: 1. };
        // Track 1's x leaves its stored 0..2 range.
        let frames: Vec<Vec<Local>> = (0..3).map(|f| vec![still([0., 0., 5.]), still([f as f64 * 2., 0.5, 0.5])]).collect();
        let mut v = clip_value(3);
        assert_eq!(encode_with(&mut v, &frames, Ranges::Keep, Statics::Rewrite).unwrap().ranges_changed, vec![1]);
        let mut v = clip_value(3);
        let r = encode_with(&mut v, &frames, Ranges::Never, Statics::Rewrite).unwrap();
        assert!(r.ranges_changed.is_empty());
        assert_eq!(r.ranges_clamped, vec![1]);
        let x = v.get("m_trackCompressionSettings").unwrap().array().unwrap()[1].get("m_translationRangeX").unwrap().get("m_flRangeLength").unwrap().number().unwrap();
        assert_eq!(x, 2., "the stored range is kept");
        // Values inside the range keep it with Keep.
        let inside: Vec<Vec<Local>> = (0..3).map(|f| vec![still([0., 0., 5.]), still([f as f64 * 0.5, 0.5, 0.5])]).collect();
        let mut v = clip_value(3);
        assert!(encode_with(&mut v, &inside, Ranges::Keep, Statics::Rewrite).unwrap().ranges_changed.is_empty());
    }
}
