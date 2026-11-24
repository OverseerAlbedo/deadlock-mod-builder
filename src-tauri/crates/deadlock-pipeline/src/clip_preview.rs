//! The Motion page's player: one animation clip of the hero played on the
//! prepared character, in the binary form of the movement preview
//! (`motion_preview::build`: a little-endian u32 header length, the header as
//! JSON, padding to 4 bytes, then a row-major 3x4 rest-to-pose matrix per
//! frame and bone as little-endian f32).
//!
//! Unlike the movement preview the clip keeps its own frames and frame rate:
//! no resampling and no 5 s cut, only a generous cap (`MAX_FRAMES`, stated by
//! `cut`). The game's clip and a compiled replacement (`.vnmclip_c`) are
//! decoded straight from their compiled data (`clip_decode`), so nothing here
//! needs the hero's decompiled sources; an authored `.vnmclip` plays the
//! animation DMX it names (`stance::frames`). Additive clips are layered over
//! the hero's standing clip. Preview only; it changes nothing in a build.
use crate::invalid;
use deadlock_core::{CancellationToken, Result, text::casefold};
use deadlock_dmx::Dmx;
use deadlock_preparation::{clip_decode, stance};
use deadlock_skeleton::transforms::{Matrix4, PreparedBone};
use deadlock_vpk::Vpk;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;

/// The most frames one clip packet holds (the longest game clips have about
/// 160; a looping idle authored at 60 fps for a minute would be cut).
pub const MAX_FRAMES: usize = 1200;
/// Frames per second of an authored DMX without a frame rate.
const DEFAULT_FPS: f64 = 30.;

#[derive(Debug, Clone, Serialize)]
pub struct Bone {
    pub name: String,
    pub parent: i32,
    pub position: [f64; 3],
}

/// The prepared character's skeleton, read once per prepared DMX.
#[derive(Debug, Clone)]
pub struct Character {
    pub target: Vec<PreparedBone>,
    pub bones: Vec<Bone>,
}
impl Character {
    pub fn load(dmx: &Path, cancel: &CancellationToken) -> Result<Self> {
        if !dmx.is_file() {
            return Err(invalid("Prepare the model first: the clip plays on the prepared character."));
        }
        let dmx = Dmx::load_with_cancel(dmx, cancel).map_err(deadlock_dmx::DmxError::into_core)?;
        let skeleton = deadlock_cloth_authoring::surface::binds(&dmx).map_err(|e| invalid(e.to_string()))?.skeleton;
        Ok(Self::from_skeleton(&skeleton)?)
    }
    pub fn from_skeleton(skeleton: &deadlock_skeleton::Skeleton) -> Result<Self> {
        let (target, _) = deadlock_skeleton::transforms::prepare_target(skeleton).map_err(|e| invalid(e.to_string()))?;
        let index: BTreeMap<&str, i32> = target.iter().enumerate().map(|(i, b)| (b.name.as_str(), i as i32)).collect();
        let bones = target
            .iter()
            .map(|b| Bone { name: b.name.clone(), parent: b.parent.as_deref().and_then(|p| index.get(p)).copied().unwrap_or(-1), position: b.position })
            .collect();
        Ok(Self { target, bones })
    }
    /// The rest rotation of the character's bone that is the clip skeleton's
    /// root (`root_motion`): the axis turn a compiled clip leaves out
    /// (`clip_decode::turn_root`). None when the character has no such root.
    pub fn root_turn(&self, root: &str) -> Option<[f64; 4]> {
        self.target.iter().find(|b| b.parent.is_none() && b.name.eq_ignore_ascii_case(root)).map(|b| b.local_rotation)
    }
}

/// Which clip to play.
pub enum Source<'a> {
    /// The game's clip, from the game archive.
    Game,
    /// A compiled replacement (`.vnmclip_c`).
    Compiled(&'a Path),
    /// An authored `.vnmclip` and the animation DMX it names.
    Authored(&'a Path),
    /// A clip made from an animation (`made_clips::author_row`'s bytes).
    Made(&'a [u8]),
    /// A clip taken from another hero (`hero_clips::author_row`'s bytes),
    /// played exactly as a made clip.
    Taken(&'a [u8]),
}

/// The packet header: the character's bones and the one clip.
#[derive(Debug, Serialize)]
pub struct Header {
    pub bones: Vec<Bone>,
    pub clips: Vec<Played>,
}

/// The played clip, in the movement preview's clip form plus what the page
/// states about it.
#[derive(Debug, Serialize)]
pub struct Played {
    pub name: String,
    pub sources: Vec<String>,
    /// Frames per second of the clip's own frames.
    pub fps: f64,
    /// Frames in the packet (`clip_frames`, or `MAX_FRAMES` when cut).
    pub frames: usize,
    pub segments: Vec<usize>,
    pub offset: usize,
    /// The clip's own length in seconds and frame count.
    pub duration: f64,
    pub clip_frames: usize,
    pub additive: bool,
    /// The standing clip an additive clip is shown over (its file stem).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub over: Option<String>,
    /// An additive clip: the game blends it with its own rule, which this
    /// layering may not match.
    pub blend_may_differ: bool,
    /// More frames than `MAX_FRAMES`: only the first ones are in the packet.
    pub cut: bool,
    /// "game", "compiled", "authored", "made" (from an animation) or "taken"
    /// (from another hero).
    pub which: String,
    pub marks: Vec<deadlock_components::motion::EventMark>,
}

fn file_stem(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or(path).trim_end_matches("_c").trim_end_matches(".vnmclip").to_string()
}

fn skeleton(archive: &Vpk, id: &str) -> Result<clip_decode::Skeleton> {
    let path = if id.ends_with("_c") { id.to_string() } else { format!("{id}_c") };
    let raw = archive.read(&path).map_err(|e| invalid(format!("The clip's skeleton is missing from the game files: {id} ({e})")))?;
    clip_decode::Skeleton::decode(&raw).map_err(|e| invalid(format!("The clip's skeleton could not be read: {e}")))
}

/// The hero's standing clip (the movement preview's choice of stand, from
/// the compiled clips beside the clip) for additive clips to play over.
pub fn standing_clip(archive: &Vpk, target: &str, skeleton: &str) -> Option<(String, clip_decode::Clip)> {
    let folder = target.rsplit_once('/').map(|(f, _)| casefold(f) + "/").unwrap_or_default();
    let mut ranked: Vec<((u8, usize), String)> = archive
        .entries
        .values()
        .filter(|e| casefold(&e.path).starts_with(&folder) && e.path.ends_with(".vnmclip_c"))
        .filter_map(|e| {
            let stem = file_stem(&e.path);
            let rank = stance::CLIPS.iter().position(|c| stem.eq_ignore_ascii_case(c)).map(|i| (0u8, i)).or_else(|| stance::rank(&stem).map(|(g, l)| (g + 1, l)))?;
            Some((rank, e.path.clone()))
        })
        .collect();
    ranked.sort();
    ranked.into_iter().find_map(|(_, path)| {
        let clip = clip_decode::Clip::decode(&archive.read(&path).ok()?).ok()?;
        (!clip.additive && clip.skeleton == skeleton).then(|| (file_stem(&path), clip))
    })
}

/// Model-space joint transforms of each stored frame of a compiled clip (an
/// additive one over `base`, which loops under it), with the root's axis turn
/// put back for the character (`turn`).
pub fn decoded_poses(skeleton: &clip_decode::Skeleton, clip: &clip_decode::Clip, base: Option<&clip_decode::Clip>, turn: Option<[f64; 4]>, limit: usize) -> Result<Vec<BTreeMap<String, Matrix4>>> {
    if clip.frames.first().map(Vec::len) != Some(skeleton.names.len()) {
        return Err(invalid("The clip's bone tracks do not match its skeleton."));
    }
    if base.is_some_and(|b| b.frames.first().map(Vec::len) != Some(skeleton.names.len())) {
        return Err(invalid("The standing clip's bone tracks do not match the clip's skeleton."));
    }
    let step = clip.frame_time();
    Ok((0..clip.frames.len().min(limit))
        .map(|k| {
            let mut pose = clip_decode::pose_at(skeleton, clip, base, k as f64 * step);
            if let Some(turn) = turn {
                clip_decode::turn_root(&mut pose, &skeleton.names[0], turn);
            }
            pose
        })
        .collect())
}

/// The frame rate of an authored animation DMX (its `DmeChannelsClip`).
fn dmx_rate(path: &Path) -> f64 {
    let Ok(dmx) = Dmx::load(path) else { return DEFAULT_FPS };
    dmx.elements
        .iter()
        .find(|e| e.kind == "DmeChannelsClip")
        .and_then(|c| match c.get("frameRate") {
            Some(deadlock_dmx::Value::Integer(r)) if *r > 0 => Some(f64::from(*r)),
            Some(deadlock_dmx::Value::Float(r)) if *r > 0. => Some(f64::from(*r)),
            _ => None,
        })
        .unwrap_or(DEFAULT_FPS)
}

/// The packet for one clip of the hero (`target`, a compiled clip path the
/// hero's catalog lists) on `character`.
pub fn packet(archive: &Vpk, character: &Character, target: &str, source: Source, cancel: &CancellationToken) -> Result<Vec<u8>> {
    let target = deadlock_core::safety::relative_asset(target)?;
    if !casefold(&target).ends_with(".vnmclip_c") {
        return Err(invalid("Choose one of the hero's animation clips."));
    }
    let original = archive.read(&target).map_err(|e| invalid(format!("The game's clip could not be read: {target} ({e})")))?;
    let game_data = deadlock_resource::container::read_blocks(&original, "DATA").map_err(|e| invalid(e.to_string()))?;
    let game_value = &game_data.first().ok_or_else(|| invalid("The game's clip has no data."))?.root;
    let marks = deadlock_components::motion::event_marks(game_value);
    let game = clip_decode::Clip::from_value(game_value).map_err(|e| invalid(format!("The game's clip could not be decoded: {e}")))?;
    cancel.check()?;
    let label = file_stem(&target);
    let (which, poses, fps, duration, clip_frames, additive, over, cut) = match source {
        Source::Game | Source::Compiled(_) | Source::Made(_) | Source::Taken(_) => {
            let (which, clip) = match source {
                Source::Compiled(path) => {
                    let raw = read_file(path)?;
                    let clip = clip_decode::Clip::decode(&raw).map_err(|e| invalid(format!("Your clip could not be decoded: {e}")))?;
                    ("compiled", clip)
                }
                Source::Made(raw) => {
                    let clip = clip_decode::Clip::decode(raw).map_err(|e| invalid(format!("Your clip could not be decoded: {e}")))?;
                    ("made", clip)
                }
                Source::Taken(raw) => {
                    let clip = clip_decode::Clip::decode(raw).map_err(|e| invalid(format!("The taken clip could not be decoded: {e}")))?;
                    ("taken", clip)
                }
                _ => ("game", game.clone()),
            };
            let skeleton = skeleton(archive, &clip.skeleton)?;
            let base = if clip.additive { standing_clip(archive, &target, &clip.skeleton) } else { None };
            if clip.additive && base.is_none() {
                return Err(invalid("This additive clip needs the hero's standing clip to play over, and none was found."));
            }
            let turn = character.root_turn(&skeleton.names[0]);
            let poses = decoded_poses(&skeleton, &clip, base.as_ref().map(|(_, b)| b), turn, MAX_FRAMES)?;
            let n = clip.frames.len();
            let fps = if n > 1 && clip.duration > 0. { (n - 1) as f64 / clip.duration } else { DEFAULT_FPS };
            (which, poses, fps, clip.duration, n, clip.additive, base.map(|(name, _)| name), n > MAX_FRAMES)
        }
        Source::Authored(path) => {
            let dmx = deadlock_components::motion::source_dependency(path)?.ok_or_else(|| invalid("This .vnmclip names no animation DMX."))?;
            if matches!(stance::additive_authoring(path), Some(stance::Additive::Frame(_) | stance::Additive::Other(_))) {
                return Err(invalid("This authored clip is additive; it can be seen here once it is compiled, as a .vnmclip_c."));
            }
            let fps = dmx_rate(&dmx);
            let (length, poses) = stance::frames(&dmx, fps, MAX_FRAMES + 1).ok_or_else(|| invalid("The animation DMX this .vnmclip names could not be read as an animation."))?;
            let n = ((length * fps).round() as usize + 1).max(poses.len());
            let cut = poses.len() > MAX_FRAMES;
            ("authored", poses.into_iter().take(MAX_FRAMES).collect(), fps, length, n, false, None, cut)
        }
    };
    cancel.check()?;
    let mut data: Vec<f32> = Vec::with_capacity(poses.len() * character.target.len() * 12);
    for pose in &poses {
        let skin = stance::skin_matrices(&character.target, pose);
        for bone in &character.target {
            let m = skin.get(&bone.name).copied().unwrap_or_else(deadlock_skeleton::transforms::identity);
            data.extend((0..3).flat_map(|r| (0..4).map(move |c| m[r][c] as f32)));
        }
    }
    let frames = poses.len();
    let played = Played {
        name: label.clone(),
        sources: vec![label],
        fps,
        frames,
        segments: vec![frames],
        offset: 0,
        duration,
        clip_frames,
        additive,
        over,
        blend_may_differ: additive,
        cut,
        which: which.into(),
        marks,
    };
    let header = serde_json::to_vec(&Header { bones: character.bones.clone(), clips: vec![played] })?;
    let mut out = (header.len() as u32).to_le_bytes().to_vec();
    out.extend(header);
    out.resize(out.len().div_ceil(4) * 4, b' ');
    out.reserve(data.len() * 4);
    for value in data {
        out.extend(value.to_le_bytes());
    }
    Ok(out)
}

/// A replacement file, read as the build reads it (no links, at most 128 MiB).
fn read_file(path: &Path) -> Result<Vec<u8>> {
    deadlock_vpk::check_unlinked_path(path, false).map_err(|e| invalid(e.to_string()))?;
    let size = std::fs::metadata(path).map_err(|e| invalid(format!("Your clip could not be read: {e}")))?.len();
    if !path.is_file() || size > deadlock_components::MAX_RESOURCE_BYTES as u64 {
        return Err(invalid("Choose an existing replacement file smaller than 128 MiB."));
    }
    std::fs::read(path).map_err(|e| invalid(format!("Your clip could not be read: {e}")))
}
