//! Compiled AnimGraph 2 clips (`.vnmclip_c`) as joint transforms, in the form
//! `stance::frames` returns (model-space matrices by joint name), so
//! `stance::skin_matrices` and the movement preview packet take them as they
//! are. Bone names and parents come from the clip's compiled skeleton
//! (`.vnmskel_c`). Read-only; it never changes a clip.
//!
//! The pose layout follows the public ValveResourceFormat decoder
//! (`ModelAnimation2/AnimationClip.cs`, MIT licence): per frame a run of
//! 16-bit words starting at `m_compressedPoseOffsets[frame]`; per track, in
//! track order, three words of rotation unless `m_bIsRotationStatic`, three of
//! translation unless `m_bIsTranslationStatic`, one of scale unless
//! `m_bIsScaleStatic`. A rotation is the three smaller quaternion components
//! (the first two words masked to 15 bits, the third whole) mapped from
//! 0..32767 to ±1/√2, the largest component rebuilt as √(1 − sum of squares)
//! and placed at index ((w0 >> 14) & 2) | (w1 >> 15). A translation or scale
//! word is start + word / 65535 × length of its range. Static values are the
//! constant rotation and the range starts.
use deadlock_resource::{container, Kind, Value};
use deadlock_skeleton::transforms::{multiply, Matrix4};
use std::collections::BTreeMap;

pub type Result<T> = std::result::Result<T, String>;

/// A bone's parent-space transform: position, rotation (x, y, z, w), scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Local {
    pub position: [f64; 3],
    pub rotation: [f64; 4],
    pub scale: f64,
}
impl Local {
    pub const IDENTITY: Local = Local { position: [0.; 3], rotation: [0., 0., 0., 1.], scale: 1. };
    /// `[tx, ty, tz, scale, qx, qy, qz, qw]`, the layout of the skeleton's
    /// reference poses and of root motion transforms.
    fn from_row(v: &[f64]) -> Self {
        Local { position: [v[0], v[1], v[2]], scale: v[3], rotation: normalize([v[4], v[5], v[6], v[7]]) }
    }
    pub fn matrix(&self) -> Matrix4 {
        matrix(self.position, self.rotation, self.scale)
    }
}

/// The compiled skeleton a clip names.
#[derive(Debug, Clone)]
pub struct Skeleton {
    pub id: String,
    pub names: Vec<String>,
    pub parents: Vec<i32>,
    pub reference: Vec<Local>,
}

/// One compiled clip, decoded into parent-space transforms per frame.
#[derive(Debug, Clone)]
pub struct Clip {
    pub skeleton: String,
    pub duration: f64,
    pub additive: bool,
    /// Per frame, per track (= skeleton bone), the parent-space transform.
    pub frames: Vec<Vec<Local>>,
    /// Per frame, the root motion transform (empty when the clip has none).
    pub root_motion: Vec<Local>,
    /// Bones the clip asks to be sampled in model space (not applied here).
    pub model_space_bones: usize,
}

fn field<'a>(v: &'a Value, key: &str) -> Result<&'a Value> {
    v.get(key).ok_or_else(|| format!("compiled clip lacks {key}"))
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
fn nums(v: &Value) -> Result<Vec<f64>> {
    v.array().map_err(|e| e.to_string())?.iter().map(num).collect()
}
fn flag(v: &Value) -> Result<bool> {
    match v.kind {
        Kind::Bool(b) => Ok(b),
        _ => Err("expected a boolean".into()),
    }
}
fn data(raw: &[u8]) -> Result<Value> {
    let mut blocks = container::read_blocks(raw, "DATA").map_err(|e| e.to_string())?;
    if blocks.len() != 1 {
        return Err("expected exactly one DATA block".into());
    }
    Ok(blocks.remove(0).root)
}

pub fn normalize(q: [f64; 4]) -> [f64; 4] {
    let n = q.iter().map(|v| v * v).sum::<f64>().sqrt().max(1e-12);
    q.map(|v| v / n)
}

pub fn matrix(position: [f64; 3], rotation: [f64; 4], scale: f64) -> Matrix4 {
    let [x, y, z, w] = normalize(rotation);
    let r = [
        [1. - 2. * (y * y + z * z), 2. * (x * y - z * w), 2. * (x * z + y * w)],
        [2. * (x * y + z * w), 1. - 2. * (x * x + z * z), 2. * (y * z - x * w)],
        [2. * (x * z - y * w), 2. * (y * z + x * w), 1. - 2. * (x * x + y * y)],
    ];
    let mut m = [[0.; 4]; 4];
    for i in 0..3 {
        for j in 0..3 {
            m[i][j] = r[i][j] * scale;
        }
        m[i][3] = position[i];
    }
    m[3][3] = 1.;
    m
}

pub fn product(a: [f64; 4], b: [f64; 4]) -> [f64; 4] {
    let ([ax, ay, az, aw], [bx, by, bz, bw]) = (a, b);
    [
        aw * bx + ax * bw + ay * bz - az * by,
        aw * by - ax * bz + ay * bw + az * bx,
        aw * bz + ax * by - ay * bx + az * bw,
        aw * bw - ax * bx - ay * by - az * bz,
    ]
}

impl Skeleton {
    pub fn decode(raw: &[u8]) -> Result<Self> {
        let v = data(raw)?;
        let id = field(&v, "m_ID")?.string().map_err(|e| e.to_string())?.to_string();
        let names: Vec<String> = field(&v, "m_boneIDs")?
            .array()
            .map_err(|e| e.to_string())?
            .iter()
            .map(|n| n.string().map(str::to_string).map_err(|e| e.to_string()))
            .collect::<Result<_>>()?;
        let parents: Vec<i32> = field(&v, "m_parentIndices")?
            .array()
            .map_err(|e| e.to_string())?
            .iter()
            .map(|p| p.integer().map(|p| p as i32).map_err(|e| e.to_string()))
            .collect::<Result<_>>()?;
        let reference: Vec<Local> = field(&v, "m_parentSpaceReferencePose")?
            .array()
            .map_err(|e| e.to_string())?
            .iter()
            .map(|row| nums(row).and_then(|r| if r.len() == 8 { Ok(Local::from_row(&r)) } else { Err("reference pose rows need 8 values".into()) }))
            .collect::<Result<_>>()?;
        if parents.len() != names.len() || reference.len() != names.len() {
            return Err("skeleton arrays differ in length".into());
        }
        if parents.iter().enumerate().any(|(i, &p)| p >= i as i32) {
            return Err("skeleton parents must come before their children".into());
        }
        Ok(Skeleton { id, names, parents, reference })
    }
    /// Model-space matrices of a full set of parent-space transforms.
    pub fn model_space(&self, locals: &[Local]) -> Vec<Matrix4> {
        let mut out: Vec<Matrix4> = Vec::with_capacity(locals.len());
        for (i, l) in locals.iter().enumerate() {
            let m = l.matrix();
            out.push(if self.parents[i] < 0 { m } else { multiply(&out[self.parents[i] as usize], &m) });
        }
        out
    }
}

struct Track {
    rotation: Option<[f64; 4]>,
    translation: [(f64, f64); 3],
    translation_static: bool,
    scale: (f64, f64),
    scale_static: bool,
}

fn range(v: &Value) -> Result<(f64, f64)> {
    Ok((num(field(v, "m_flRangeStart")?)?, num(field(v, "m_flRangeLength")?)?))
}

/// One quaternion from its three stored words.
pub fn rotation(words: [u16; 3]) -> [f64; 4] {
    let step = 2f64.sqrt() / 32767.;
    let min = -1. / 2f64.sqrt();
    let c = [(words[0] & 0x7fff) as f64 * step + min, (words[1] & 0x7fff) as f64 * step + min, words[2] as f64 * step + min];
    let largest = (((words[0] >> 14) & 2) | (words[1] >> 15)) as usize;
    let omitted = (1. - c.iter().map(|v| v * v).sum::<f64>()).max(0.).sqrt();
    let mut q = [0.; 4];
    let mut at = 0;
    for (k, slot) in q.iter_mut().enumerate() {
        if k == largest {
            *slot = omitted;
        } else {
            *slot = c[at];
            at += 1;
        }
    }
    q
}

fn quantized(word: u16, (start, length): (f64, f64)) -> f64 {
    start + word as f64 / 65535. * length
}

impl Clip {
    pub fn decode(raw: &[u8]) -> Result<Self> {
        Self::from_value(&data(raw)?)
    }
    pub fn from_value(v: &Value) -> Result<Self> {
        let skeleton = field(v, "m_skeleton")?.string().map_err(|e| e.to_string())?.to_string();
        let count = field(v, "m_nNumFrames")?.integer().map_err(|e| e.to_string())? as usize;
        let duration = num(field(v, "m_flDuration")?)?;
        let additive = flag(field(v, "m_bIsAdditive")?)?;
        let Kind::Blob(blob) = &field(v, "m_compressedPoseData")?.kind else {
            return Err("compressed pose data is not a blob".into());
        };
        let words: Vec<u16> = blob.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        let offsets: Vec<usize> = field(v, "m_compressedPoseOffsets")?
            .array()
            .map_err(|e| e.to_string())?
            .iter()
            .map(|o| o.integer().map(|o| o as usize).map_err(|e| e.to_string()))
            .collect::<Result<_>>()?;
        let mut tracks = vec![];
        for t in field(v, "m_trackCompressionSettings")?.array().map_err(|e| e.to_string())? {
            let constant = nums(field(t, "m_constantRotation")?)?;
            if constant.len() != 4 {
                return Err("constant rotation needs 4 values".into());
            }
            tracks.push(Track {
                rotation: flag(field(t, "m_bIsRotationStatic")?)?.then(|| normalize([constant[0], constant[1], constant[2], constant[3]])),
                translation: [range(field(t, "m_translationRangeX")?)?, range(field(t, "m_translationRangeY")?)?, range(field(t, "m_translationRangeZ")?)?],
                translation_static: flag(field(t, "m_bIsTranslationStatic")?)?,
                scale: range(field(t, "m_scaleRange")?)?,
                scale_static: flag(field(t, "m_bIsScaleStatic")?)?,
            });
        }
        if offsets.len() != count {
            return Err("pose offsets do not match the frame count".into());
        }
        let mut frames = Vec::with_capacity(count);
        for &start in &offsets {
            let mut at = start;
            let mut take = |n: usize| -> Result<&[u16]> {
                let s = words.get(at..at + n).ok_or("pose data ends early")?;
                at += n;
                Ok(s)
            };
            let mut pose = Vec::with_capacity(tracks.len());
            for t in &tracks {
                let r = match t.rotation {
                    Some(q) => q,
                    None => {
                        let w = take(3)?;
                        normalize(rotation([w[0], w[1], w[2]]))
                    }
                };
                let p = if t.translation_static {
                    t.translation.map(|r| r.0)
                } else {
                    let w = take(3)?;
                    [quantized(w[0], t.translation[0]), quantized(w[1], t.translation[1]), quantized(w[2], t.translation[2])]
                };
                let s = if t.scale_static { t.scale.0 } else { quantized(take(1)?[0], t.scale) };
                pose.push(Local { position: p, rotation: r, scale: s });
            }
            frames.push(pose);
        }
        let root_motion = match v.get("m_rootMotion").and_then(|r| r.get("m_transforms")) {
            Some(list) => list.array().map_err(|e| e.to_string())?.iter().map(|row| nums(row).map(|r| Local::from_row(&r))).collect::<Result<_>>()?,
            None => vec![],
        };
        let model_space_bones = v.get("m_modelSpaceBoneSamplingIndices").and_then(|a| a.array().ok()).map_or(0, |a| a.len());
        Ok(Clip { skeleton, duration, additive, frames, root_motion, model_space_bones })
    }

    /// Seconds between stored frames.
    pub fn frame_time(&self) -> f64 {
        if self.frames.len() > 1 { self.duration / (self.frames.len() - 1) as f64 } else { 0. }
    }

    /// Parent-space transforms at `t` seconds (linear between stored frames,
    /// rotations the short way round, held at the ends).
    pub fn locals_at(&self, t: f64) -> Vec<Local> {
        let n = self.frames.len();
        if n == 1 || self.duration <= 0. {
            return self.frames[0].clone();
        }
        let p = (t / self.frame_time()).clamp(0., (n - 1) as f64);
        let lo = (p.floor() as usize).min(n - 1);
        let hi = (lo + 1).min(n - 1);
        let f = p - lo as f64;
        self.frames[lo].iter().zip(&self.frames[hi]).map(|(a, b)| lerp(a, b, f)).collect()
    }
}

fn lerp(a: &Local, b: &Local, f: f64) -> Local {
    let mut q = b.rotation;
    if (0..4).map(|k| a.rotation[k] * q[k]).sum::<f64>() < 0. {
        q = q.map(|v| -v);
    }
    Local {
        position: std::array::from_fn(|k| a.position[k] + (b.position[k] - a.position[k]) * f),
        rotation: normalize(std::array::from_fn(|k| a.rotation[k] + (q[k] - a.rotation[k]) * f)),
        scale: a.scale + (b.scale - a.scale) * f,
    }
}

/// Adds an additive layer's parent-space values to a base pose bone by bone
/// (positions added, rotations composed after the base, scales added: an
/// additive clip stores a scale change of 0 where nothing changes).
pub fn layer(base: &[Local], delta: &[Local]) -> Vec<Local> {
    base.iter()
        .zip(delta)
        .map(|(b, d)| Local {
            position: std::array::from_fn(|k| b.position[k] + d.position[k]),
            rotation: normalize(product(b.rotation, d.rotation)),
            scale: b.scale + d.scale,
        })
        .collect()
}

/// Model-space joint transforms sampled `fps` times a second (at most `limit`
/// frames) and the clip's length in seconds, like `stance::frames`. An
/// additive clip is played over `base` (looping under it); without a base it
/// is refused, never shown as a pose.
pub fn frames(skeleton: &Skeleton, clip: &Clip, base: Option<&Clip>, fps: f64, limit: usize) -> Result<(f64, Vec<BTreeMap<String, Matrix4>>)> {
    if clip.frames.first().map(Vec::len) != Some(skeleton.names.len()) {
        return Err("clip tracks do not match its skeleton".into());
    }
    if clip.additive && base.is_none() {
        return Err("an additive clip needs a base clip to play over".into());
    }
    let count = ((clip.duration * fps).floor() as usize + 1).min(limit.max(1));
    let out = (0..count).map(|frame| pose_at(skeleton, clip, base, frame as f64 / fps)).collect();
    Ok((clip.duration, out))
}

/// Model-space joint transforms at `t` seconds (an additive clip over `base`,
/// which loops under it).
pub fn pose_at(skeleton: &Skeleton, clip: &Clip, base: Option<&Clip>, t: f64) -> BTreeMap<String, Matrix4> {
    let mut locals = clip.locals_at(t);
    if let Some(base) = base.filter(|_| clip.additive) {
        let bt = if base.duration > 0. { t % base.duration } else { 0. };
        locals = layer(&base.locals_at(bt), &locals);
    }
    let world = skeleton.model_space(&locals);
    skeleton.names.iter().cloned().zip(world).collect()
}

/// The compiled clip keeps its skeleton's root (`root_motion`) without a turn
/// and holds the pelvis, IK targets and attachments in the game's model space
/// directly. The hero's compiled model skeleton (and the decompiled takes)
/// give that root bone a rest rotation (an axis turn) instead; to play a
/// compiled clip on such a skeleton the root's own matrix takes that turn on
/// the right, every other joint keeps the clip's model-space value. Without it
/// anything weighted to the root (or following it) would turn by 120°.
pub fn turn_root(pose: &mut BTreeMap<String, Matrix4>, root: &str, rest_rotation: [f64; 4]) {
    if let Some(m) = pose.get_mut(root) {
        *m = multiply(m, &matrix([0.; 3], rest_rotation, 1.));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_stored_rotation_puts_its_largest_component_where_the_two_flag_bits_say() {
        // Components 0.1, 0.2, 0.3 stored; the largest one goes to index 2.
        let word = |x: f64| ((x + 1. / 2f64.sqrt()) / (2f64.sqrt() / 32767.)).round() as u16;
        let (a, b, c) = (word(0.1), word(0.2), word(0.3));
        let q = rotation([a | 0x8000, b, c]);
        let w = (1f64 - 0.01 - 0.04 - 0.09).sqrt();
        for (got, want) in q.iter().zip([0.1, 0.2, w, 0.3]) {
            assert!((got - want).abs() < 1e-4, "{q:?}");
        }
        let q = rotation([a, b | 0x8000, c]);
        assert!((q[1] - w).abs() < 1e-4 && (q[3] - 0.3).abs() < 1e-4, "{q:?}");
    }
}
