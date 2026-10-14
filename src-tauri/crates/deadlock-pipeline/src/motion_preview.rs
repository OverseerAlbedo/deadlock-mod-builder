//! The movement preview of the Physics page: the prepared character's
//! skeleton and the hero's own stand, run and jump clips as rest-to-pose bone
//! matrices per frame. The page plays them and simulates the moving parts
//! (secondary motion) itself with the values Analyze recorded and the current
//! Fine-tune sliders. Preview only; it changes nothing in the build.
use crate::{invalid, sources};
use deadlock_core::{CancellationToken, Result};
use deadlock_dmx::Dmx;
use deadlock_preparation::stance;
use deadlock_project::{Project, Settings};
use deadlock_skeleton::transforms::{Matrix4, PreparedBone};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Frames per second of the preview and the longest animation it keeps.
pub const FPS: f64 = 60.;
const MAX_SECONDS: f64 = 5.;
/// Frames blended from one part of a jump into the next (0.1 s).
const BLEND_FRAMES: usize = 6;

#[derive(Debug, Serialize)]
pub struct Bone {
    pub name: String,
    /// Index of the parent bone, -1 for a root.
    pub parent: i32,
    /// Rest position in model space (inches).
    pub position: [f64; 3],
}

#[derive(Debug, Serialize)]
pub struct Clip {
    /// "idle", "run" or "jump".
    pub name: String,
    /// The hero's clip files it plays, in order.
    pub sources: Vec<String>,
    pub fps: f64,
    pub frames: usize,
    /// Frames of each source clip, in order (a jump: take-off, air, landing, settle).
    pub segments: Vec<usize>,
    /// First float of this clip in the data: frame f, bone b starts at
    /// offset + (f * bones + b) * 12 (a row-major 3x4 rest-to-pose matrix).
    pub offset: usize,
}

#[derive(Debug, Serialize)]
pub struct Header {
    pub bones: Vec<Bone>,
    pub clips: Vec<Clip>,
}

const RUN: [&str; 3] = ["weapon_run_n", "primary_run_n", "run_n"];
const JUMP_START: [&str; 3] = ["jump_ground", "jump_start", "jump"];
const JUMP_AIR: [&str; 4] = ["jump_air", "jump_in_air_loop", "in_air_loop", "jump_in_air"];
const LANDING: [&str; 5] = ["landing_impact", "jump_landing", "jump_impact", "landing", "landing_impact_idle"];
const AVOID: [&str; 11] = ["crouch", "zipline", "hideout", "invis", "catapult", "forced", "wall", "stun", "rope", "slide", "ability"];

/// One part of a preview animation: candidate clip files (best first), the
/// most seconds of it to keep, and the clip an additive candidate is played
/// over (a landing impact over the stand).
pub struct Part {
    pub candidates: Vec<PathBuf>,
    pub seconds: f64,
    pub base: Option<PathBuf>,
}

/// The parts of each preview animation, by the hero's own clip names: the
/// gun-ready stand, running forward, and a jump (take-off, in the air, landing,
/// then settling into the stand).
pub fn clips_for(base: &Path, model_folder: &Path) -> Vec<(&'static str, Vec<Part>)> {
    let part = |candidates: Vec<PathBuf>, seconds: f64| Part { candidates, seconds, base: None };
    let mut out = vec![];
    let idle = stance::find_ranked(base, model_folder, &stance::CLIPS, stance::rank);
    if !idle.is_empty() {
        out.push(("idle", vec![part(idle.clone(), MAX_SECONDS)]));
    }
    let run = stance::find_ranked(base, model_folder, &RUN, |s| if s.ends_with("run_n") { stance::rank_words(s, &["run"], &AVOID) } else { None });
    if !run.is_empty() {
        out.push(("run", vec![part(run, MAX_SECONDS)]));
    }
    let start_avoid = [&AVOID[..], &["air", "land", "impact"]].concat();
    let start = stance::find_ranked(base, model_folder, &JUMP_START, |s| stance::rank_words(s, &["jump"], &start_avoid));
    let air = stance::find_ranked(base, model_folder, &JUMP_AIR, |s| stance::rank_words(s, &["jump", "air"], &AVOID));
    let landing = stance::find_ranked(base, model_folder, &LANDING, |s| stance::rank_words(s, &["land"], &AVOID).or_else(|| stance::rank_words(s, &["jump", "impact"], &AVOID)));
    if !start.is_empty() || !air.is_empty() {
        let landing = Part { candidates: landing, seconds: 1.2, base: idle.first().cloned() };
        out.push(("jump", vec![part(start, 1.), part(air, 0.6), landing, part(idle, 0.7)]));
    }
    out
}

/// A clip authored on top of another pose (an additive layer such as Haze's
/// `landing_impact_idle`, whose pelvis sits at the floor) rather than a pose
/// of its own: its joints lie far below the rest skeleton.
fn additive(pose: &BTreeMap<String, Matrix4>, target: &[PreparedBone]) -> bool {
    let rest_top = target.iter().map(|b| b.position[2]).fold(f64::MIN, f64::max);
    let top = target.iter().filter_map(|b| pose.get(&b.name)).map(|m| m[2][3]).fold(f64::MIN, f64::max);
    top < rest_top * 0.45
}

fn quaternion(m: &Matrix4) -> [f64; 4] {
    let t = m[0][0] + m[1][1] + m[2][2];
    let q = if t > 0. {
        let s = (t + 1.).sqrt() * 2.;
        [(m[2][1] - m[1][2]) / s, (m[0][2] - m[2][0]) / s, (m[1][0] - m[0][1]) / s, 0.25 * s]
    } else if m[0][0] > m[1][1] && m[0][0] > m[2][2] {
        let s = (1. + m[0][0] - m[1][1] - m[2][2]).sqrt() * 2.;
        [0.25 * s, (m[0][1] + m[1][0]) / s, (m[0][2] + m[2][0]) / s, (m[2][1] - m[1][2]) / s]
    } else if m[1][1] > m[2][2] {
        let s = (1. + m[1][1] - m[0][0] - m[2][2]).sqrt() * 2.;
        [(m[0][1] + m[1][0]) / s, 0.25 * s, (m[1][2] + m[2][1]) / s, (m[0][2] - m[2][0]) / s]
    } else {
        let s = (1. + m[2][2] - m[0][0] - m[1][1]).sqrt() * 2.;
        [(m[0][2] + m[2][0]) / s, (m[1][2] + m[2][1]) / s, 0.25 * s, (m[1][0] - m[0][1]) / s]
    };
    let n = q.iter().map(|v| v * v).sum::<f64>().sqrt().max(1e-12);
    q.map(|v| v / n)
}

fn matrix(q: [f64; 4], p: [f64; 3]) -> Matrix4 {
    let [x, y, z, w] = q;
    [
        [1. - 2. * (y * y + z * z), 2. * (x * y - z * w), 2. * (x * z + y * w), p[0]],
        [2. * (x * y + z * w), 1. - 2. * (x * x + z * z), 2. * (y * z - x * w), p[1]],
        [2. * (x * z - y * w), 2. * (y * z + x * w), 1. - 2. * (x * x + y * y), p[2]],
        [0., 0., 0., 1.],
    ]
}

/// Joint transforms between two poses: positions straight, rotations the short
/// way round (a rotation-only pose; scale stays 1).
fn blend(a: &BTreeMap<String, Matrix4>, b: &BTreeMap<String, Matrix4>, t: f64) -> BTreeMap<String, Matrix4> {
    b.iter()
        .map(|(name, mb)| {
            let Some(ma) = a.get(name) else { return (name.clone(), *mb) };
            let (qa, mut qb) = (quaternion(ma), quaternion(mb));
            if (0..4).map(|k| qa[k] * qb[k]).sum::<f64>() < 0. {
                qb = qb.map(|v| -v);
            }
            let q: [f64; 4] = std::array::from_fn(|k| qa[k] + (qb[k] - qa[k]) * t);
            let n = q.iter().map(|v| v * v).sum::<f64>().sqrt().max(1e-12);
            let p: [f64; 3] = std::array::from_fn(|k| ma[k][3] + (mb[k][3] - ma[k][3]) * t);
            (name.clone(), matrix(q.map(|v| v / n), p))
        })
        .collect()
}

/// The preview packet: a little-endian u32 header length, the header as JSON,
/// padding to 4 bytes and the frames as little-endian f32.
pub fn build(settings: &Settings, project: &Project, cancel: &CancellationToken) -> Result<Vec<u8>> {
    if project.dmx.is_empty() || !Path::new(&project.dmx).is_file() {
        return Err(invalid("Prepare the model first: the movement preview plays the prepared character."));
    }
    let dmx = Dmx::load_with_cancel(Path::new(&project.dmx), cancel).map_err(deadlock_dmx::DmxError::into_core)?;
    let skeleton = deadlock_cloth_authoring::surface::binds(&dmx).map_err(|e| invalid(e.to_string()))?.skeleton;
    let (target, _) = deadlock_skeleton::transforms::prepare_target(&skeleton).map_err(|e| invalid(e.to_string()))?;
    let base = sources::base_dir(settings, project)?;
    let model = base.join(project.source_model());
    let folder = model.parent().ok_or_else(|| invalid("The hero's sources have no model folder."))?;
    let index: BTreeMap<&str, i32> = target.iter().enumerate().map(|(i, b)| (b.name.as_str(), i as i32)).collect();
    let bones = target
        .iter()
        .map(|b| Bone { name: b.name.clone(), parent: b.parent.as_deref().and_then(|p| index.get(p)).copied().unwrap_or(-1), position: b.position })
        .collect();
    let max_frames = (MAX_SECONDS * FPS) as usize;
    let mut data: Vec<f32> = vec![];
    let mut clips = vec![];
    for (name, parts) in clips_for(&base, folder) {
        cancel.check()?;
        let offset = data.len();
        let mut frames = 0;
        let mut segments = vec![];
        let mut sources = vec![];
        let mut previous: Option<BTreeMap<String, Matrix4>> = None;
        for part in parts {
            if frames >= max_frames {
                break;
            }
            let cap = ((part.seconds * FPS) as usize).min(max_frames - frames);
            // The first candidate that is a pose of its own; an additive layer
            // (a landing impact) is played over the part's base clip instead.
            let Some((file, poses)) = part.candidates.iter().find_map(|file| {
                let (_, poses) = stance::frames(file, FPS, cap)?;
                if poses.is_empty() {
                    return None;
                }
                if !additive(&poses[0], &target) {
                    return Some((file, poses));
                }
                let base = part.base.as_ref()?;
                let (_, layered) = stance::frames_layered(base, Some(file), FPS, cap)?;
                (!layered.is_empty() && !additive(&layered[0], &target)).then_some((file, layered))
            }) else {
                continue;
            };
            sources.push(file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default());
            segments.push(poses.len());
            for (k, pose) in poses.iter().enumerate() {
                let pose = match &previous {
                    Some(last) if k < BLEND_FRAMES => blend(last, pose, (k + 1) as f64 / (BLEND_FRAMES + 1) as f64),
                    _ => pose.clone(),
                };
                let skin = stance::skin_matrices(&target, &pose);
                for bone in &target {
                    let m = skin.get(&bone.name).copied().unwrap_or_else(deadlock_skeleton::transforms::identity);
                    data.extend((0..3).flat_map(|r| (0..4).map(move |c| m[r][c] as f32)));
                }
                frames += 1;
            }
            previous = poses.last().cloned();
        }
        if frames > 0 {
            clips.push(Clip { name: name.into(), sources, fps: FPS, frames, segments, offset });
        }
    }
    let header = serde_json::to_vec(&Header { bones, clips })?;
    let mut out = (header.len() as u32).to_le_bytes().to_vec();
    out.extend(header);
    out.resize(out.len().div_ceil(4) * 4, b' ');
    for value in data {
        out.extend(value.to_le_bytes());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn poses_blend_the_short_way_round() {
        let turn = |angle: f64| matrix([0., 0., (angle / 2.).sin(), (angle / 2.).cos()], [0., 0., 0.]);
        let a = BTreeMap::from([("j".to_string(), turn(0.))]);
        let b = BTreeMap::from([("j".to_string(), turn(3.0))]);
        let half = blend(&a, &b, 0.5)["j"];
        // Halfway between 0 and 3 rad about Z, not the long way round.
        let angle = half[1][0].atan2(half[0][0]);
        assert!((angle - 1.5).abs() < 1e-9, "{angle}");
        // Joints only the newer pose has are kept as they are.
        let c = BTreeMap::from([("j".to_string(), turn(0.)), ("k".to_string(), turn(1.))]);
        assert_eq!(blend(&a, &c, 0.5)["k"], turn(1.));
    }
}
