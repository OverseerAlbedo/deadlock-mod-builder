//! The movement preview of the Physics page: the prepared character's
//! skeleton and the hero's own stand, run and jump clips as rest-to-pose bone
//! matrices per frame. The page plays them and simulates the moving parts
//! (secondary motion) itself with the values Analyze recorded and the current
//! Fine-tune sliders. Preview only; it changes nothing in the build. The
//! packet stores each distinct frame once and only the bones the view reads
//! (`Layout`); the page expands it to every frame of every clip
//! (`decodeMotion`, the same as `expand`).
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
pub const BLEND_FRAMES: usize = 6;

#[derive(Debug, Clone, Serialize)]
pub struct Bone {
    pub name: String,
    /// Index of the parent bone, -1 for a root.
    pub parent: i32,
    /// Rest position in model space (inches).
    pub position: [f64; 3],
}

#[derive(Debug, Clone, Serialize)]
pub struct Clip {
    /// "idle", "run" or "jump".
    pub name: String,
    /// The hero's clip files it plays, in order.
    pub sources: Vec<String>,
    pub fps: f64,
    pub frames: usize,
    /// Frames of each source clip, in order (a jump: take-off, air, landing, settle).
    pub segments: Vec<usize>,
    /// The full layout (no `layout` in the header): the first float of this
    /// clip in the data; frame f, bone b starts at offset + (f * bones + b) * 12
    /// (a row-major 3x4 rest-to-pose matrix).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<usize>,
    /// The stored layout (`Layout`): the stored frames the clip plays, in
    /// order, as runs of [first stored frame, count] (a stop's run and stand
    /// and a fall's repeated in-air loop name frames other clips store).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub runs: Vec<[usize; 2]>,
}

/// The stored layout of the frames (header `layout`): every distinct frame
/// once, and only for the bones the view needs (`needed_bones`). Stored frame
/// s, the k-th stored bone, starts at (s * stored.len() + k) * 12. A bone not
/// stored takes the matrix of its nearest stored ancestor (identity without
/// one): nothing is skinned to it and nothing reads it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Layout {
    /// `LAYOUT`.
    pub version: u32,
    /// Indices of the stored bones, ascending.
    pub stored: Vec<usize>,
    /// Number of stored frames.
    pub frames: usize,
}
pub const LAYOUT: u32 = 2;

/// The hero's movement speeds in inches per second, from the game's hero
/// definition: forward is the most move speed plus sprint, back and sideways
/// the most move speed alone (an assumption: the definitions name no backward
/// or sideways speed).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Speeds {
    pub forward: f64,
    pub back: f64,
    pub strafe: f64,
}
impl Speeds {
    pub fn from_game(speeds: deadlock_components::heroes::MoveSpeeds) -> Self {
        let inches = deadlock_components::heroes::INCHES_PER_METRE;
        Self { forward: (speeds.max_move + speeds.sprint) * inches, back: speeds.max_move * inches, strafe: speeds.max_move * inches }
    }
}

#[derive(Debug, Serialize)]
pub struct Header {
    pub bones: Vec<Bone>,
    pub clips: Vec<Clip>,
    /// None when the game's hero definitions could not be read (the page then
    /// uses its own constant).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speeds: Option<Speeds>,
    /// The stored layout; None for the full layout (every frame of every clip
    /// for every bone, in clip order).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<Layout>,
}

const JUMP_START: [&str; 3] = ["jump_ground", "jump_start", "jump"];
const JUMP_AIR: [&str; 4] = ["jump_air", "jump_in_air_loop", "in_air_loop", "jump_in_air"];
/// Heroes whose in-air loop is split in two (the top of the jump, then coming down). Tried only
/// when none of the clips above is found, so a hero that has one keeps its pick.
const JUMP_AIR_SPLIT: [&str; 2] = ["in_air_loop_down", "in_air_loop_apex"];
const LANDING: [&str; 5] = ["landing_impact", "jump_landing", "jump_impact", "landing", "landing_impact_idle"];
/// The gun-ready stand by exact name: `stance::CLIPS`, then `weapon_idle` (a
/// hero whose clips name the stand without "stand", next to its
/// `weapon_crouch_idle`). Tried after the names of `stance::CLIPS` in each
/// group of clips, so a hero with one of those keeps its pick.
const STAND: [&str; 5] = [stance::CLIPS[0], stance::CLIPS[1], stance::CLIPS[2], stance::CLIPS[3], "weapon_idle"];
const AVOID: [&str; 11] = ["crouch", "zipline", "hideout", "invis", "catapult", "forced", "wall", "stun", "rope", "slide", "ability"];

/// One part of a preview animation: candidate clip files (best first), the
/// most seconds of it to keep, and the clip an additive candidate is played
/// over (a landing impact over the stand).
pub struct Part {
    pub candidates: Vec<PathBuf>,
    pub seconds: f64,
    pub base: Option<PathBuf>,
    /// A "RelativeToFrame" lean clip held at its fullest frame on top of the
    /// part (candidates, best first; none found plays the part as it is).
    pub lean: Vec<PathBuf>,
    /// Loop the clip to fill `seconds` when it is shorter.
    pub fill: bool,
    /// Frames blended into this part from the one before.
    pub blend: usize,
}
impl Part {
    fn new(candidates: Vec<PathBuf>, seconds: f64) -> Self {
        Self { candidates, seconds, base: None, lean: vec![], fill: false, blend: BLEND_FRAMES }
    }
}

/// Seconds of the jump's in-air part: with the take-off's last 0.08 s the
/// character is off the ground for 0.65 s, the time a hero's jump lasts in
/// game; the landing clip starts as it lands.
pub const JUMP_AIR_SECONDS: f64 = 0.57;
/// Seconds of the long fall (the in-air loop, played again to fill it).
pub const FALL_SECONDS: f64 = 3.;
/// The stop: seconds of running, then of the stand, with the lean released
/// over this many frames (0.3 s, as the run's speed eases out).
pub const STOP_RUN_SECONDS: f64 = 1.;
pub const STOP_STAND_SECONDS: f64 = 2.;
pub const STOP_BLEND_FRAMES: usize = 18;

/// A hero's run clips for one direction (`n` forward, `s` back, `w` left,
/// `e` right), best first. The out-of-combat run is an ordinary run too: a hero
/// whose sources hold no weapon run gets it before a run of an ability.
fn runs(base: &Path, model_folder: &Path, direction: &str) -> Vec<PathBuf> {
    let names: Vec<String> = ["weapon_run_", "primary_run_", "run_", "out_of_combat_run_"].iter().map(|p| format!("{p}{direction}")).collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    let suffix = format!("run_{direction}");
    stance::find_ranked(base, model_folder, &names, |s| if s.ends_with(&suffix) { stance::rank_words(s, &["run"], &AVOID) } else { None })
}
/// A hero's movement lean clips (`loco_lean_forward`, `_back`, `_left`, `_right`), best first.
fn leans(base: &Path, model_folder: &Path, direction: &str) -> Vec<PathBuf> {
    let name = format!("loco_lean_{direction}");
    let avoid = [&AVOID[..], &["corner", "crouch", "stand", "run"]].concat();
    stance::find_ranked(base, model_folder, &[name.as_str()], |s| if s.ends_with(&format!("lean_{direction}")) { stance::rank_words(s, &["lean"], &avoid) } else { None })
}

/// The parts of each preview animation, by the hero's own clip names: the
/// gun-ready stand, running forward, a jump (take-off, in the air, landing,
/// then settling into the stand), running back, left and right, a long fall
/// and a stop (running, then the stand). Runs carry the hero's lean for their
/// direction.
pub fn clips_for(base: &Path, model_folder: &Path) -> Vec<(&'static str, Vec<Part>)> {
    let part = Part::new;
    let mut out = vec![];
    let idle = stance::find_ranked(base, model_folder, &STAND, stance::rank);
    if !idle.is_empty() {
        out.push(("idle", vec![part(idle.clone(), MAX_SECONDS)]));
    }
    let run = runs(base, model_folder, "n");
    if !run.is_empty() {
        out.push(("run", vec![Part { lean: leans(base, model_folder, "forward"), ..part(run.clone(), MAX_SECONDS) }]));
    }
    let start_avoid = [&AVOID[..], &["air", "land", "impact"]].concat();
    let start = stance::find_ranked(base, model_folder, &JUMP_START, |s| stance::rank_words(s, &["jump"], &start_avoid));
    let mut air = stance::find_ranked(base, model_folder, &JUMP_AIR, |s| stance::rank_words(s, &["jump", "air"], &AVOID));
    if air.is_empty() {
        air = stance::find_ranked(base, model_folder, &JUMP_AIR_SPLIT, |_| None);
    }
    let landing = stance::find_ranked(base, model_folder, &LANDING, |s| stance::rank_words(s, &["land"], &AVOID).or_else(|| stance::rank_words(s, &["jump", "impact"], &AVOID)));
    if !start.is_empty() || !air.is_empty() {
        let landing = Part { base: idle.first().cloned(), ..part(landing, 1.2) };
        // A short stretch in the air: the game's jump is quick, and a longer hold
        // of the in-air loop reads as floating.
        out.push(("jump", vec![part(start, 1.), Part { fill: true, ..part(air.clone(), JUMP_AIR_SECONDS) }, landing, part(idle.clone(), 0.7)]));
    }
    for (name, direction, lean) in [("back", "s", "back"), ("left", "w", "left"), ("right", "e", "right")] {
        let clips = runs(base, model_folder, direction);
        if !clips.is_empty() {
            out.push((name, vec![Part { lean: leans(base, model_folder, lean), ..part(clips, MAX_SECONDS) }]));
        }
    }
    if !air.is_empty() {
        out.push(("fall", vec![Part { fill: true, ..part(air, FALL_SECONDS) }]));
    }
    if !run.is_empty() && !idle.is_empty() {
        let lean = leans(base, model_folder, "forward");
        out.push(("stop", vec![Part { lean, fill: true, ..part(run, STOP_RUN_SECONDS) }, Part { blend: STOP_BLEND_FRAMES, ..part(idle, STOP_STAND_SECONDS) }]));
    }
    out
}

/// A pose that looks like an additive layer rather than a pose of its own:
/// its joints lie far below the rest skeleton (the change from a base frame
/// leaves every joint near the floor). Used for clips whose `.vnmclip` does
/// not say (`stance::additive_authoring`), and as a check on layered frames.
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

/// One joint between two transforms: position straight, rotation the short
/// way round (a rotation-only pose; scale stays 1).
fn mix(ma: &Matrix4, mb: &Matrix4, t: f64) -> Matrix4 {
    let (qa, mut qb) = (quaternion(ma), quaternion(mb));
    if (0..4).map(|k| qa[k] * qb[k]).sum::<f64>() < 0. {
        qb = qb.map(|v| -v);
    }
    let q: [f64; 4] = std::array::from_fn(|k| qa[k] + (qb[k] - qa[k]) * t);
    let n = q.iter().map(|v| v * v).sum::<f64>().sqrt().max(1e-12);
    let p: [f64; 3] = std::array::from_fn(|k| ma[k][3] + (mb[k][3] - ma[k][3]) * t);
    matrix(q.map(|v| v / n), p)
}

/// Joint transforms between two poses (`mix`). A joint of the character that
/// only one pose animates eases from or to where the other pose places it
/// (following its parent at its rest offset, `stance::posed_joints`), so it
/// never jumps at the change of part (an in-air clip's weapon target).
/// Joints the character does not have are kept from the newer pose only.
fn blend(a: &BTreeMap<String, Matrix4>, b: &BTreeMap<String, Matrix4>, t: f64, target: &[PreparedBone]) -> BTreeMap<String, Matrix4> {
    let names: std::collections::BTreeSet<&str> = target.iter().map(|bone| bone.name.as_str()).collect();
    let placed = |pose: &BTreeMap<String, Matrix4>, other: &BTreeMap<String, Matrix4>| {
        if other.keys().any(|name| !pose.contains_key(name) && names.contains(name.as_str())) { stance::posed_joints(target, pose) } else { BTreeMap::new() }
    };
    let (placed_a, placed_b) = (placed(a, b), placed(b, a));
    let mut out: BTreeMap<String, Matrix4> = b.iter().map(|(name, mb)| (name.clone(), a.get(name).or_else(|| placed_a.get(name)).map_or(*mb, |ma| mix(ma, mb, t)))).collect();
    for (name, ma) in a.iter().filter(|(name, _)| !b.contains_key(*name)) {
        if let Some(mb) = placed_b.get(name) {
            out.insert(name.clone(), mix(ma, mb, t));
        }
    }
    out
}

/// One preview animation as model-space joint transforms per frame (parts
/// joined, the first frames of each part blended from the last one).
pub struct Animation {
    pub name: &'static str,
    pub sources: Vec<String>,
    pub segments: Vec<usize>,
    /// How each source clip is played: as a pose, or as an additive layer and
    /// by which rule it was recognised.
    pub played: Vec<String>,
    pub poses: Vec<BTreeMap<String, Matrix4>>,
}

/// The poses of each preview animation of the hero's sources on `target`.
pub fn animations(base: &Path, folder: &Path, target: &[PreparedBone], cancel: &CancellationToken) -> Result<Vec<Animation>> {
    let mut out = vec![];
    for (name, parts) in clips_for(base, folder) {
        cancel.check()?;
        out.extend(animate(name, parts, target));
    }
    Ok(out)
}

/// One preview animation from its parts, None when no part could be played.
pub fn animate(name: &'static str, parts: Vec<Part>, target: &[PreparedBone]) -> Option<Animation> {
    let max_frames = (MAX_SECONDS * FPS) as usize;
    let mut poses_out: Vec<BTreeMap<String, Matrix4>> = vec![];
    let mut segments = vec![];
    let mut sources = vec![];
    let mut played = vec![];
    let mut previous: Option<BTreeMap<String, Matrix4>> = None;
    for part in parts {
        if poses_out.len() >= max_frames {
            break;
        }
        let cap = ((part.seconds * FPS) as usize).min(max_frames - poses_out.len());
        let Some((file, (how, poses))) = part.candidates.iter().find_map(|file| part_poses(file, part.base.as_deref(), target, cap).map(|p| (file, p))) else {
            continue;
        };
        // A lean held on top (a run in the game leans with its speed).
        let (how, poses) = match part.lean.iter().find_map(|lean| held_lean(file, lean, cap).map(|p| (lean, p))) {
            Some((lean, poses)) if part.base.is_none() => (format!("{how}, lean {} held at {:.2} s", lean.file_stem().map(|s| s.to_string_lossy()).unwrap_or_default(), poses.0), poses.1),
            _ => (how, poses),
        };
        // A part shorter than its time plays again from its start.
        let poses: Vec<_> = if part.fill && !poses.is_empty() && poses.len() < cap { poses.iter().cycle().take(cap).cloned().collect() } else { poses };
        sources.push(file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default());
        segments.push(poses.len());
        played.push(how);
        for (k, pose) in poses.iter().enumerate() {
            poses_out.push(match &previous {
                Some(last) if k < part.blend => blend(last, pose, (k + 1) as f64 / (part.blend + 1) as f64, target),
                _ => pose.clone(),
            });
        }
        previous = poses.last().cloned();
    }
    (!poses_out.is_empty()).then_some(Animation { name, sources, segments, played, poses: poses_out })
}

/// The frames one candidate clip of a part plays, or None when it cannot be
/// used: a pose of its own as it is; an additive layer (a landing impact)
/// over the part's base clip. Its `.vnmclip` says which it is; an additive
/// clip is never played as a pose, and one whose reference the preview cannot
/// rebuild (relative to the skeleton or to another animation) is not used.
/// Without that word in a `.vnmclip`, the pose itself decides (`additive`),
/// except for a clip document of the newer layout, which writes the word for
/// every additive clip: one without it is a pose even when its first frame is
/// a crouch deep enough to look like a layer (a take-off).
fn part_poses(file: &Path, base: Option<&Path>, target: &[PreparedBone], cap: usize) -> Option<(String, Vec<BTreeMap<String, Matrix4>>)> {
    match stance::additive_authoring(file) {
        Some(stance::Additive::Pose) => {
            let (_, poses) = stance::frames(file, FPS, cap)?;
            return (!poses.is_empty()).then(|| ("pose (clip)".into(), poses));
        }
        Some(stance::Additive::Frame(reference)) => {
            let (_, layered) = stance::frames_relative(base?, file, reference, FPS, cap)?;
            return (!layered.is_empty() && !additive(&layered[0], target)).then(|| (format!("additive (clip: relative to frame {reference})"), layered));
        }
        Some(stance::Additive::Other(_)) => return None,
        None => {}
    }
    let (_, poses) = stance::frames(file, FPS, cap)?;
    if poses.is_empty() {
        return None;
    }
    if let Some(how) = unstated_pose(additive(&poses[0], target), || stance::is_clip_document(file)) {
        return Some((how.into(), poses));
    }
    let (_, layered) = stance::frames_layered(base?, Some(file), FPS, cap)?;
    (!layered.is_empty() && !additive(&layered[0], target)).then_some(("additive (height)".into(), layered))
}

/// How a clip whose `.vnmclip` names no additive kind plays as a pose of its
/// own (its label), or None when it is to be layered: a pose when its first
/// frame does not look like a layer (`additive`), or when it is a clip
/// document of the newer layout (`stance::is_clip_document`), which names the
/// kind of every additive clip.
fn unstated_pose(looks_additive: bool, clip_document: impl FnOnce() -> bool) -> Option<&'static str> {
    if !looks_additive {
        Some("pose (height)")
    } else if clip_document() {
        Some("pose (clip document without an additive kind)")
    } else {
        None
    }
}

/// Weight of the held lean (1: the clip's full lean).
pub const LEAN_WEIGHT: f64 = 1.;

/// The frames of a pose clip with a "RelativeToFrame" lean clip held at its
/// fullest frame (`stance::layer_peak`) on top, and that time; None when the
/// lean clip is not such a layer or cannot be read.
fn held_lean(file: &Path, lean: &Path, cap: usize) -> Option<(f64, Vec<BTreeMap<String, Matrix4>>)> {
    let Some(stance::Additive::Frame(reference)) = stance::additive_authoring(lean) else { return None };
    let peak = stance::layer_peak(lean, reference, file)?;
    let (_, poses) = stance::frames_held(file, lean, reference, peak, LEAN_WEIGHT, FPS, cap)?;
    (!poses.is_empty()).then_some((peak, poses))
}

/// The hero's movement speeds from the game's definitions, None when they cannot be read.
fn game_speeds(settings: &Settings, project: &Project) -> Option<Speeds> {
    if settings.pak.is_empty() || !Path::new(&settings.pak).is_file() {
        return None;
    }
    let archive = deadlock_vpk::Vpk::open(&settings.pak).ok()?;
    deadlock_components::heroes::MoveSpeeds::read(&archive, project.source_model()).ok().flatten().map(Speeds::from_game)
}

/// The prepared character's movement: its bones, the bones the view needs,
/// and every frame of every clip for every bone (rest-to-pose 3x4, f32).
#[derive(Clone)]
pub struct Movement {
    pub bones: Vec<Bone>,
    /// Per bone: whether the view needs its pose (`needed_bones`).
    pub needed: Vec<bool>,
    /// Each clip (its `offset` and `runs` unset) and its frames.
    pub clips: Vec<(Clip, Vec<Vec<f32>>)>,
    pub speeds: Option<Speeds>,
}

/// The bones the movement view reads a pose of: every bone a vertex of the
/// prepared meshes is weighted to (the drawn skin, the swing points and
/// labels of the moving parts, the cloth model's nodes), the bones the
/// automatic-motion body colliders sit on, the app's own bones (`dmb_`), and
/// every ancestor of those. Every bone when the weights cannot be read.
pub fn needed_bones(dmx: &Dmx, target: &[PreparedBone]) -> Vec<bool> {
    let all = vec![true; target.len()];
    let Some(skinned) = skinned_bones(dmx) else { return all };
    use deadlock_cloth_authoring::body_colliders::{ARMS, LEGS, NECK};
    let colliders: Vec<&str> = LEGS.iter().chain(ARMS.iter()).chain(std::iter::once(&NECK)).flat_map(|(bone, tip, _)| [*bone, *tip]).chain(["head", "pelvis"]).collect();
    let index: BTreeMap<&str, usize> = target.iter().enumerate().map(|(i, b)| (b.name.as_str(), i)).collect();
    let mut needed = vec![false; target.len()];
    for (i, bone) in target.iter().enumerate() {
        if !(skinned.contains(&bone.name) || colliders.iter().any(|c| c.eq_ignore_ascii_case(&bone.name)) || bone.name.starts_with("dmb_")) {
            continue;
        }
        let mut at = Some(i);
        while let Some(k) = at.filter(|&k| !needed[k]) {
            needed[k] = true;
            at = target[k].parent.as_deref().and_then(|p| index.get(p)).copied();
        }
    }
    needed
}

/// Names of the joints any vertex is weighted to (weight above 0) through the
/// one joint palette of the prepared DMX; None when that cannot be read.
fn skinned_bones(dmx: &Dmx) -> Option<std::collections::BTreeSet<String>> {
    let palettes: Vec<Vec<i32>> = dmx
        .elements
        .iter()
        .filter(|e| e.kind == "DmeModel")
        .filter_map(|e| e.get("jointList").filter(|v| !v.is_empty()))
        .map(|v| dmx.integer_values(v).ok())
        .collect::<Option<_>>()?;
    let [palette] = palettes.as_slice() else { return None };
    let names: Vec<&str> = palette.iter().map(|&r| usize::try_from(r).ok().and_then(|r| dmx.elements.get(r)).map(|e| e.name.as_str())).collect::<Option<_>>()?;
    let floats = |v: &deadlock_dmx::Value| -> Option<Vec<f32>> {
        match v {
            deadlock_dmx::Value::Array(items) => items.iter().map(|x| if let deadlock_dmx::Value::Float(f) = x { Some(*f) } else { None }).collect(),
            _ => Some(dmx.value_bytes(v)?.chunks_exact(4).map(|b| f32::from_le_bytes(b.try_into().unwrap())).collect()),
        }
    };
    let mut out = std::collections::BTreeSet::new();
    for state in dmx.elements.iter().filter(|e| e.kind == "DmeVertexData") {
        let (Some(weights), Some(joints)) = (state.get("blendweights$0").or_else(|| state.get("jointWeights")), state.get("blendindices$0").or_else(|| state.get("jointIndices"))) else { continue };
        let (weights, joints) = (floats(weights)?, dmx.integer_values(joints).ok()?);
        if weights.len() != joints.len() {
            return None;
        }
        for (w, j) in weights.iter().zip(&joints) {
            if *w > 0. {
                out.insert((*names.get(usize::try_from(*j).ok()?)?).to_owned());
            }
        }
    }
    Some(out)
}

/// The character's movement for the preview (see `build`).
pub fn movement(settings: &Settings, project: &Project, cancel: &CancellationToken) -> Result<Movement> {
    if project.dmx.is_empty() || !Path::new(&project.dmx).is_file() {
        return Err(invalid("Prepare the model first: the movement preview plays the prepared character."));
    }
    let dmx = Dmx::load_with_cancel(Path::new(&project.dmx), cancel).map_err(deadlock_dmx::DmxError::into_core)?;
    let skeleton = deadlock_cloth_authoring::surface::binds(&dmx).map_err(|e| invalid(e.to_string()))?.skeleton;
    let (target, _) = deadlock_skeleton::transforms::prepare_target(&skeleton).map_err(|e| invalid(e.to_string()))?;
    let needed = needed_bones(&dmx, &target);
    let base = sources::base_dir(settings, project)?;
    let model = base.join(project.source_model());
    let folder = model.parent().ok_or_else(|| invalid("The hero's sources have no model folder."))?;
    let index: BTreeMap<&str, i32> = target.iter().enumerate().map(|(i, b)| (b.name.as_str(), i as i32)).collect();
    let bones = target
        .iter()
        .map(|b| Bone { name: b.name.clone(), parent: b.parent.as_deref().and_then(|p| index.get(p)).copied().unwrap_or(-1), position: b.position })
        .collect();
    let mut clips = vec![];
    for (name, parts) in clips_for(&base, folder) {
        cancel.check()?;
        let Some(animation) = animate(name, parts, &target) else { continue };
        let frames: Vec<Vec<f32>> = animation
            .poses
            .iter()
            .map(|pose| {
                let skin = stance::skin_matrices(&target, pose);
                target
                    .iter()
                    .flat_map(|bone| {
                        let m = skin.get(&bone.name).copied().unwrap_or_else(deadlock_skeleton::transforms::identity);
                        (0..3).flat_map(move |r| (0..4).map(move |c| m[r][c] as f32))
                    })
                    .collect()
            })
            .collect();
        let clip = Clip { name: animation.name.into(), sources: animation.sources, fps: FPS, frames: frames.len(), segments: animation.segments, offset: None, runs: vec![] };
        clips.push((clip, frames));
    }
    Ok(Movement { bones, needed, clips, speeds: game_speeds(settings, project) })
}

fn packet(header: &Header, data: impl Iterator<Item = f32>) -> Result<Vec<u8>> {
    let header = serde_json::to_vec(header)?;
    let mut out = (header.len() as u32).to_le_bytes().to_vec();
    out.extend(header);
    out.resize(out.len().div_ceil(4) * 4, b' ');
    for value in data {
        out.extend(value.to_le_bytes());
    }
    Ok(out)
}

/// The stored layout (`Layout`): each distinct frame once (compared bit for
/// bit over the stored bones), the stored bones only.
pub fn encode(movement: Movement) -> Result<Vec<u8>> {
    let stored: Vec<usize> = (0..movement.bones.len()).filter(|&b| movement.needed[b]).collect();
    let mut table: Vec<Vec<f32>> = vec![];
    let mut seen: std::collections::HashMap<Vec<u32>, usize> = std::collections::HashMap::new();
    let mut clips = vec![];
    for (mut clip, frames) in movement.clips {
        for frame in frames {
            let kept: Vec<f32> = stored.iter().flat_map(|&b| frame[b * 12..b * 12 + 12].iter().copied()).collect();
            let key: Vec<u32> = kept.iter().map(|v| v.to_bits()).collect();
            let at = *seen.entry(key).or_insert_with(|| {
                table.push(kept);
                table.len() - 1
            });
            match clip.runs.last_mut() {
                Some([first, count]) if *first + *count == at => *count += 1,
                _ => clip.runs.push([at, 1]),
            }
        }
        clips.push(clip);
    }
    let layout = Layout { version: LAYOUT, stored, frames: table.len() };
    packet(&Header { bones: movement.bones, clips, speeds: movement.speeds, layout: Some(layout) }, table.into_iter().flatten())
}

/// The full layout: every frame of every clip for every bone, in clip order
/// (the layout before `Layout`; kept to check that the stored layout plays the
/// same frames).
pub fn encode_full(movement: Movement) -> Result<Vec<u8>> {
    let mut clips = vec![];
    let mut data = vec![];
    for (mut clip, frames) in movement.clips {
        clip.offset = Some(data.len());
        data.extend(frames.into_iter().flatten());
        clips.push(clip);
    }
    packet(&Header { bones: movement.bones, clips, speeds: movement.speeds, layout: None }, data.into_iter())
}

/// A packet in the full layout: its header (with each clip's `offset`, no
/// `layout`, no `runs`) and every frame of every clip for every bone, the
/// unstored bones of a stored layout following their nearest stored ancestor.
/// The page's `decodeMotion` does the same; this one checks packets in Rust.
pub fn expand(packet: &[u8]) -> Result<(serde_json::Value, Vec<f32>)> {
    let length = u32::from_le_bytes(packet.get(..4).ok_or_else(|| invalid("Invalid movement preview packet."))?.try_into().unwrap()) as usize;
    let mut header: serde_json::Value = serde_json::from_slice(packet.get(4..4 + length).ok_or_else(|| invalid("Truncated movement preview header."))?)?;
    let start = (4 + length).div_ceil(4) * 4;
    let data: Vec<f32> = packet[start..].chunks_exact(4).map(|b| f32::from_le_bytes(b.try_into().unwrap())).collect();
    let Some(layout) = header.get("layout").cloned() else { return Ok((header, data)) };
    let parents: Vec<i64> = header["bones"].as_array().into_iter().flatten().map(|b| b["parent"].as_i64().unwrap_or(-1)).collect();
    let stored: Vec<usize> = layout["stored"].as_array().into_iter().flatten().filter_map(|v| v.as_u64()).map(|v| v as usize).collect();
    let mut slot = vec![None; parents.len()];
    for (k, &b) in stored.iter().enumerate() {
        *slot.get_mut(b).ok_or_else(|| invalid("Invalid movement preview layout."))? = Some(k);
    }
    // The stored bone each bone takes its matrix from (parents come first).
    let mut source: Vec<Option<usize>> = vec![None; parents.len()];
    for b in 0..parents.len() {
        source[b] = slot[b].or_else(|| usize::try_from(parents[b]).ok().filter(|&p| p < b).and_then(|p| source[p]));
    }
    let width = stored.len() * 12;
    let identity = [1f32, 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0.];
    let mut out = vec![];
    for clip in header["clips"].as_array_mut().into_iter().flatten() {
        let offset = out.len();
        for run in clip["runs"].as_array().into_iter().flatten() {
            let (first, count) = (run[0].as_u64().unwrap_or(0) as usize, run[1].as_u64().unwrap_or(0) as usize);
            for s in first..first + count {
                let frame = data.get(s * width..(s + 1) * width).ok_or_else(|| invalid("Truncated movement preview frames."))?;
                for b in 0..parents.len() {
                    out.extend_from_slice(source[b].map_or(&identity[..], |k| &frame[k * 12..k * 12 + 12]));
                }
            }
        }
        let object = clip.as_object_mut().unwrap();
        object.remove("runs");
        object.insert("offset".into(), offset.into());
    }
    header.as_object_mut().unwrap().remove("layout");
    Ok((header, out))
}

/// The preview packet: a little-endian u32 header length, the header as JSON,
/// padding to 4 bytes and the stored frames as little-endian f32 (`Layout`).
pub fn build(settings: &Settings, project: &Project, cancel: &CancellationToken) -> Result<Vec<u8>> {
    encode(movement(settings, project, cancel)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn the_run_is_an_ordinary_run_before_a_run_of_an_ability() {
        let temp = tempfile::tempdir().unwrap();
        let folder = temp.path().join("models/heroes_wip/sample");
        std::fs::create_dir_all(folder.join("dmx/animation")).unwrap();
        let add = |name: &str| std::fs::write(folder.join(format!("dmx/animation/{name}.dmx")), b"").unwrap();
        let first = |names: &[&str]| { for name in names { add(name); } runs(temp.path(), &folder, "n").first().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().into_owned()) };
        // Without a weapon run, the out-of-combat run comes before the ability's run.
        assert_eq!(first(&["barrage_run_n", "out_of_combat_run_n"]).as_deref(), Some("out_of_combat_run_n"));
        // A weapon run still comes first.
        assert_eq!(first(&["weapon_run_n"]).as_deref(), Some("weapon_run_n"));
    }
    #[test]
    fn a_stand_named_weapon_idle_comes_before_an_item_stand_but_after_the_stand_names() {
        let stand = |names: &[&str]| {
            let temp = tempfile::tempdir().unwrap();
            let folder = temp.path().join("models/heroes_wip/sample");
            std::fs::create_dir_all(folder.join("dmx/animation")).unwrap();
            for name in names {
                std::fs::write(folder.join(format!("dmx/animation/{name}.dmx")), b"").unwrap();
            }
            let idle = clips_for(temp.path(), &folder).into_iter().find(|(n, _)| *n == "idle").map(|(_, p)| p);
            idle.and_then(|p| p[0].candidates.first().and_then(|c| c.file_stem()).map(|s| s.to_string_lossy().into_owned()))
        };
        // Clips that name the gun-ready stand without "stand" (beside a weapon crouch idle).
        assert_eq!(stand(&["item_stand_idle", "reload_stand_idle", "weapon_idle", "weapon_crouch_idle"]).as_deref(), Some("weapon_idle"));
        // Every name the stand had before still wins.
        assert_eq!(stand(&["weapon_idle", "weapon_stand_idle"]).as_deref(), Some("weapon_stand_idle"));
        assert_eq!(stand(&["weapon_idle", "idle"]).as_deref(), Some("idle"));
        // Only the exact name: an aim layer is not a stand.
        assert_eq!(stand(&["aim_weapon_idle", "item_stand_idle"]).as_deref(), Some("item_stand_idle"));
    }

    #[test]
    fn a_clip_document_without_an_additive_kind_plays_as_a_pose() {
        // A first frame that does not look like a layer is a pose, whatever the file.
        assert_eq!(unstated_pose(false, || unreachable!()), Some("pose (height)"));
        // A deep crouch that looks like a layer: a pose when the newer clip
        // document says no additive kind, layered otherwise (older files).
        assert_eq!(unstated_pose(true, || true), Some("pose (clip document without an additive kind)"));
        assert_eq!(unstated_pose(true, || false), None);
    }

    fn bone(name: &str, parent: Option<&str>, at: [f64; 3]) -> PreparedBone {
        let id = deadlock_skeleton::transforms::identity();
        PreparedBone { name: name.into(), parent: parent.map(Into::into), local_position: at, local_rotation: [0., 0., 0., 1.], local_scale: 1., position: at, matrix: id, inverse_bind_matrix: id, length: 0. }
    }
    fn turn(angle: f64, at: [f64; 3]) -> Matrix4 {
        matrix([0., 0., (angle / 2.).sin(), (angle / 2.).cos()], at)
    }

    #[test]
    fn poses_blend_the_short_way_round() {
        let target = [bone("j", None, [0.; 3])];
        let a = BTreeMap::from([("j".to_string(), turn(0., [0.; 3]))]);
        let b = BTreeMap::from([("j".to_string(), turn(3.0, [0.; 3]))]);
        let half = blend(&a, &b, 0.5, &target)["j"];
        // Halfway between 0 and 3 rad about Z, not the long way round.
        let angle = half[1][0].atan2(half[0][0]);
        assert!((angle - 1.5).abs() < 1e-9, "{angle}");
        // A joint the character does not have is kept from the newer pose.
        let c = BTreeMap::from([("j".to_string(), turn(0., [0.; 3])), ("k".to_string(), turn(1., [0.; 3]))]);
        assert_eq!(blend(&a, &c, 0.5, &target)["k"], turn(1., [0.; 3]));
    }

    /// Three bones (root, arm, a loose root nothing needs) and two clips: an idle of two frames and a
    /// stop that plays the idle again, then a frame of its own.
    fn small() -> Movement {
        let bone = |name: &str, parent: i32| Bone { name: name.into(), parent, position: [0.; 3] };
        let frame = |x: f32, loose: f32| [[1., 0., 0., x, 0., 1., 0., 0., 0., 0., 1., 0.], [1., 0., 0., x + 0.5, 0., 1., 0., 0., 0., 0., 1., 0.], [1., 0., 0., loose, 0., 1., 0., 0., 0., 0., 1., 0.]].concat();
        let clip = |name: &str, frames: usize| Clip { name: name.into(), sources: vec![], fps: FPS, frames, segments: vec![frames], offset: None, runs: vec![] };
        Movement {
            bones: vec![bone("root", -1), bone("arm", 0), bone("loose", -1)],
            needed: vec![true, true, false],
            clips: vec![(clip("idle", 2), vec![frame(1., 7.), frame(2., 8.)]), (clip("stop", 3), vec![frame(1., 9.), frame(2., 7.), frame(3., 7.)])],
            speeds: None,
        }
    }
    fn header_of(packet: &[u8]) -> serde_json::Value {
        let length = u32::from_le_bytes(packet[..4].try_into().unwrap()) as usize;
        serde_json::from_slice(&packet[4..4 + length]).unwrap()
    }

    #[test]
    fn the_stored_layout_keeps_each_frame_once_for_the_needed_bones() {
        let packet = encode(small()).unwrap();
        let header = header_of(&packet);
        assert_eq!(header["layout"], serde_json::json!({"version": LAYOUT, "stored": [0, 1], "frames": 3}));
        assert_eq!(header["clips"][0]["runs"], serde_json::json!([[0, 2]]));
        // The stop's first two frames are the idle's (the loose bone is not compared): one run.
        assert_eq!(header["clips"][1]["runs"], serde_json::json!([[0, 3]]));
        assert!(header["clips"][0].get("offset").is_none());
        let floats = (packet.len() - (4 + u32::from_le_bytes(packet[..4].try_into().unwrap()) as usize).div_ceil(4) * 4) / 4;
        assert_eq!(floats, 3 * 2 * 12);
        // The full layout has no `layout` and every frame of every bone.
        let full = encode_full(small()).unwrap();
        let full_header = header_of(&full);
        assert!(full_header.get("layout").is_none());
        assert_eq!(full_header["clips"][1]["offset"], 2 * 3 * 12);
    }

    #[test]
    fn a_stored_packet_expands_to_the_full_layouts_frames() {
        let (header, data) = expand(&encode(small()).unwrap()).unwrap();
        let (full_header, full) = expand(&encode_full(small()).unwrap()).unwrap();
        assert_eq!(header, full_header);
        assert_eq!(data.len(), full.len());
        for (k, (a, b)) in data.iter().zip(&full).enumerate() {
            // Bone 2 (not needed) has no stored ancestor: identity in the stored layout.
            if (k / 12) % 3 != 2 {
                assert_eq!(a.to_bits(), b.to_bits(), "float {k}");
            }
        }
        assert_eq!(&data[24..36], &[1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0.]);
        // A bone that is not stored takes its stored parent's matrix.
        let mut movement = small();
        movement.bones[2].parent = 1;
        let (_, data) = expand(&encode(movement).unwrap()).unwrap();
        assert_eq!(data[24 + 3], 1.5);
    }

    #[test]
    fn a_joint_one_pose_lacks_eases_from_and_to_where_the_other_pose_places_it() {
        // "tip" hangs 10 in along x from "j"; only pose b animates it, 10 in higher.
        let target = [bone("j", None, [0.; 3]), bone("tip", Some("j"), [10., 0., 0.])];
        let a = BTreeMap::from([("j".to_string(), turn(0.5, [0.; 3]))]);
        let b = BTreeMap::from([("j".to_string(), turn(0.5, [0.; 3])), ("tip".to_string(), turn(0.5, [0., 0., 10.]))]);
        let followed = stance::posed_joints(&target, &a)["tip"];
        let position = |m: &Matrix4| [m[0][3], m[1][3], m[2][3]];
        let gap = |p: [f64; 3], q: [f64; 3]| (0..3).map(|k| (p[k] - q[k]).powi(2)).sum::<f64>().sqrt();
        // Into b: from where a's pose holds it (on its parent) toward b's, a seventh of the way per frame.
        let steps: Vec<[f64; 3]> = (1..=6).map(|k| position(&blend(&a, &b, k as f64 / 7., &target)["tip"])).collect();
        assert!(gap(steps[0], position(&followed)) < gap(position(&followed), position(&b["tip"])) / 6.);
        let whole = gap(position(&followed), position(&b["tip"]));
        for pair in steps.windows(2) {
            assert!((gap(pair[0], pair[1]) - whole / 7.).abs() < 1e-9, "{pair:?}");
        }
        // Out of b into a: it eases back down onto its parent.
        let back = blend(&b, &a, 6. / 7., &target)["tip"];
        assert!((gap(position(&back), position(&followed)) - whole / 7.).abs() < 1e-9);
        // Joints both poses lack are not added: they follow their parent as before.
        let other = [bone("j", None, [0.; 3]), bone("tip", Some("j"), [10., 0., 0.]), bone("loose", Some("j"), [0., 5., 0.])];
        assert!(!blend(&b, &a, 0.5, &other).contains_key("loose"));
    }
}
