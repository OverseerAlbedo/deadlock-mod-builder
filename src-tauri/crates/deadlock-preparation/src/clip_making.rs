//! Making one of the hero's clips from an animation the user made in Blender
//! on their own character.
//!
//! 1. Convert saves the fit (`animation-fit.json` beside the prepared model,
//!    written by `animation_fit.py`): what the retarget needs, so making a
//!    clip never runs the preparation again. A preparation made before this
//!    has none (`NO_FIT`).
//! 2. `animation_export.py` (its own Blender entry) lists the actions of the
//!    user's file (a `.blend`, or an FBX or glTF file imported with the
//!    preparation's import settings) and samples one into a packet (`clip_author::Export`),
//!    kept under the project's workspace (`packet_folder`) and reused while
//!    the file's stamp and digest, the action, the mode, the fit's digest and
//!    the script's digest match (`current`).
//! 3. `author` turns the packet into the game's clip (timing, loop blend, in
//!    place, which bones, the `SWITCHES`) with `clip_author` and
//!    `clip_encode`, with no Blender: the build and the preview use it alike.
//!    The game's clip is the template: its length, frame count, events, root
//!    motion and every track the action does not drive stay the game's.
use crate::clip_author::{self, Bones, Export, Options, Timing};
use crate::clip_decode::{Clip, Skeleton};
use crate::clip_encode::{self, Ranges, Statics};
use crate::{blender::BlenderBridge, invalid, path_text, source, write_json, Result};
use deadlock_core::{safety, CancellationToken};
use deadlock_project::made_clip::{ClipBones, MadeClip};
use deadlock_project::Project;
use deadlock_resource::container;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

/// The fit file beside the prepared model; equals `animation_fit.py FILE`.
pub const FIT_FILE: &str = "animation-fit.json";
/// Equals `animation_fit.py POLICY` and `animation_export.py FIT_POLICY`.
pub const FIT_POLICY: &str = "animation-fit-v1";
pub const SCRIPT: &str = "animation_export.py";
/// The stamp's version (`made.json`).
pub const STAMP_VERSION: &str = "made-clip-packet-v1";
/// The packet's policy; equals `animation_export.py POLICY`.
pub const PACKET_POLICY: &str = clip_author::POLICY;

// Messages the page and troubleshooting compare (TS `makeTypes.ts`).
pub const NEEDS_PREPARED: &str = "Prepare the model first: a clip is made from an animation of the prepared character";
pub const NO_FIT: &str = "This character was prepared before clips could be made from an animation. Prepare the model again on Prepare model, then make the clip";
/// A project on a ready-made DMX model (Prepared mode): preparing again would
/// never give it a fit.
pub const READY_MADE_MODEL: &str = "This project uses a ready-made DMX model. A clip is made from an animation of a rigged character that the app prepared: prepare your rigged character file on Prepare model, then make the clip";
/// A model prepared as Unrigged humanoid: it has no skeleton of its own to animate.
pub const UNRIGGED_MODEL: &str = "This model was prepared without a skeleton of its own (Unrigged humanoid), so there is no animation of it to make a clip from. A clip is made from an animation of a rigged character that the app prepared: prepare a rigged character file on Prepare model, then make the clip";
pub const NOT_MADE: &str = "This clip is not made from its animation yet, or the animation file, the animation or the prepared model changed since. Make the clip again on the Motion page";
/// Followed by ": " and the file.
pub const ONLY_ANIMATION_FILES: &str = "A clip is made from an animation in a Blender, FBX or glTF file (.blend, .fbx, .glb or .gltf)";
/// A .gltf names its data and images beside it, so it is never copied to a
/// shorter path (a .blend, .fbx or .glb is). Followed by the length.
pub const GLTF_TOO_LONG: &str = "This glTF file's path is too long for Blender to open";
pub const CHANGED_WHILE_MAKING: &str = "The animation file or the prepared model changed while the clip was being made. Make the clip again.";
pub const ADDITIVE: &str = "This clip cannot be made from an animation. The game adds it on top of other clips (a lean, not a full pose), and the app can only make full poses. A compiled clip file can still replace it.";
pub const MODEL_SPACE: &str = "This clip cannot be made from an animation. The game samples some of its bones in the model's space, which the app does not write. A compiled clip file can still replace it.";
pub const SINGLE_POSE: &str = "This clip cannot be made from an animation. It holds a single pose, not a motion. A compiled clip file can still replace it.";
pub const SCALED: &str = "This clip cannot be made from an animation. Some of its bones change size, which the app does not write. A compiled clip file can still replace it.";
/// The start of the sentence `animation_export.py` gives for another skeleton
/// (then " (the bone X is missing). " or " (the bone X is not where it was
/// when you prepared). " and `SKELETON_ADVICE`).
pub const SKELETON_DIFFERS: &str = "This file's skeleton differs from your prepared character";
pub const SKELETON_ADVICE: &str = "Animate the file you prepared, or a copy of it.";
pub const NO_RIG: &str = "This file has no armature with the bones of your prepared character.";
/// Followed by the action's name and a full stop.
pub const NO_ACTION: &str = "The file has no animation named ";
pub const PART_TOO_LONG: &str = clip_author::PART_TOO_LONG;

/// What making a clip may change in the game's track settings. One place:
/// every made clip (build, preview, report) uses `SWITCHES`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Switches {
    /// A track the game stores as a fixed value, which the animation sets to
    /// another value that does not move, gets that value. Off: the game's.
    pub new_fixed_values: bool,
    /// An animated track whose new values leave its stored range gets a range
    /// fitted to them. Off: ranges are never changed (values are clamped).
    pub refit_ranges: bool,
    /// A track the game stores as a fixed value, which the animation moves,
    /// becomes a moving track. Off: it is held at its first frame's value (a
    /// held driven bone's children keep pointing where the action puts them).
    pub still_to_moving: bool,
    /// Feet on the floor: one vertical lift for the whole clip on the body
    /// (`clip_author::Options::floor_lift`). On in the app. Left out of saved
    /// JSON while off, so a set with it off keeps the bytes it had before the
    /// switch existed.
    #[serde(default, skip_serializing_if = "is_off")]
    pub floor_lift: bool,
    /// The off hand stays on the gun (`clip_author::Options::off_hand`). Off
    /// in the app; left out while off.
    #[serde(default, skip_serializing_if = "is_off")]
    pub off_hand: bool,
}
fn is_off(on: &bool) -> bool {
    !*on
}
/// The defaults: the first three on (a still track may get a new fixed value
/// or start moving, and a range may be re-fitted) and the floor lift on; the
/// off hand off. Change them only together with the test below.
pub const SWITCHES: Switches = Switches { new_fixed_values: true, refit_ranges: true, still_to_moving: true, floor_lift: true, off_hand: false };

/// One action of the user's file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ActionInfo {
    pub name: String,
    pub frame_start: f64,
    pub frame_end: f64,
    pub fps: f64,
    pub length_seconds: f64,
    /// Bones of the rig the action has keys for.
    pub keyed_bones: Vec<String>,
    /// How many of them drive a hero bone.
    pub driving_keyed_bones: usize,
}
/// The actions of a file and whether its skeleton is the prepared one.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ActionList {
    pub source: String,
    pub rig: String,
    /// Bones of the file's armature.
    pub bones: usize,
    /// How many of them drive a hero bone (what `driving_keyed_bones` is out of).
    pub driving_bones: usize,
    /// None when the skeleton is the prepared one; else the sentence to show.
    pub skeleton_error: Option<String>,
    pub actions: Vec<ActionInfo>,
}

/// The stamp of a packet (`made.json`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Made {
    pub version: String,
    pub source: String,
    pub source_size: u64,
    pub source_modified: u128,
    pub source_sha256: String,
    pub action: String,
    pub mode: String,
    pub fit_sha256: String,
    pub script_sha256: String,
    pub packet_sha256: String,
    pub samples: usize,
    pub length_seconds: f64,
    pub fps: f64,
}

fn script_digest() -> String {
    format!("{:x}", Sha256::digest(include_bytes!("../resources/blender/animation_export.py")))
}
fn stamp(path: &Path) -> Result<(u64, u128)> {
    let meta = fs::metadata(path)?;
    let modified = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_nanos());
    Ok((meta.len(), modified))
}

/// The project's saved fit: refused with `NEEDS_PREPARED` without an accepted
/// preparation and `NO_FIT` when it has no fit of this version; a project
/// that can never get one by preparing again is told what it needs instead
/// (`READY_MADE_MODEL`: a ready-made DMX, `UNRIGGED_MODEL`: Unrigged humanoid).
pub fn fit_path(project: &Project) -> Result<PathBuf> {
    use deadlock_project::ModelInputMode;
    let unprepared = project.preparation_report.trim().is_empty() || project.dmx.trim().is_empty();
    let never = match project.model_input_mode {
        ModelInputMode::UnriggedHumanoid => Some(UNRIGGED_MODEL),
        ModelInputMode::Prepared if project.preparation_report.trim().is_empty() && !project.dmx.trim().is_empty() => Some(READY_MADE_MODEL),
        _ => None,
    };
    if unprepared {
        return Err(invalid(never.unwrap_or(NEEDS_PREPARED)));
    }
    let report = Path::new(&project.preparation_report);
    let path = report.parent().ok_or_else(|| invalid(NEEDS_PREPARED))?.join(FIT_FILE);
    #[derive(Deserialize)]
    struct Head {
        policy: String,
    }
    let head: Option<Head> = fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok());
    match head {
        Some(h) if h.policy == FIT_POLICY => Ok(path),
        _ => Err(invalid(never.unwrap_or(NO_FIT))),
    }
}

/// The user's animation file (Blender, FBX or glTF), resolved; anything else
/// is refused (`ONLY_ANIMATION_FILES`).
pub fn check_source(source: &Path) -> Result<PathBuf> {
    if !deadlock_project::made_clip::is_animation_file(&path_text(source)?) {
        return Err(invalid(format!("{ONLY_ANIMATION_FILES}: {}", source.display())));
    }
    source::resolve_source(&path_text(source)?)
}

#[derive(Serialize)]
struct Job<'a> {
    operation: &'a str,
    source: String,
    fit: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    action: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mode: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    packet: Option<String>,
}
/// The file Blender opens: the user's own, or a copy in the job folder when
/// its path is too long for Blender (`source.<suffix>`; never for a .gltf,
/// whose data and images are found beside it: `GLTF_TOO_LONG`).
fn openable(source: &Path, folder: &Path) -> Result<PathBuf> {
    let Some(units) = source::too_long_for_blender(source) else { return Ok(source.to_path_buf()) };
    let suffix = source::suffix(source);
    if suffix == "gltf" {
        return Err(invalid(format!(
            "{GLTF_TOO_LONG} ({units} characters; Blender opens paths shorter than 260), and the files beside it cannot be copied with it. Move it and the files beside it to a shorter folder, then choose it again."
        )));
    }
    fs::create_dir_all(folder)?;
    let copy = folder.join(format!("source.{suffix}"));
    fs::copy(safety::long_path(source), &copy)?;
    Ok(copy)
}
/// Removes a copy `openable` made.
fn remove_copy(folder: &Path) {
    for suffix in deadlock_project::made_clip::SOURCE_SUFFIXES {
        let _ = fs::remove_file(folder.join(format!("source.{suffix}")));
    }
}

/// The actions of `source` (Blender, under the caller's operation).
pub fn list_actions(bridge: &BlenderBridge, project_root: &Path, source: &Path, fit: &Path) -> Result<ActionList> {
    let source = check_source(source)?;
    let folder = project_root.join("made-clips").join("list");
    #[derive(Deserialize)]
    struct Reply {
        rig: String,
        bones: usize,
        #[serde(default)]
        driving_bones: usize,
        skeleton_error: Option<String>,
        actions: Vec<ActionInfo>,
    }
    let open = openable(&source, &folder)?;
    let reply: Reply = bridge.execute_script(
        SCRIPT,
        &Job { operation: "list", source: path_text(&open)?, fit: path_text(fit)?, action: None, mode: None, packet: None },
        &folder.join("blender"),
    )?;
    Ok(ActionList { source: path_text(&source)?, rig: reply.rig, bones: reply.bones, driving_bones: reply.driving_bones, skeleton_error: reply.skeleton_error, actions: reply.actions })
}

/// Where the packet of one file, action and mode lives under a project's
/// workspace root (shared by every row that uses them).
pub fn packet_folder(project_root: &Path, source: &Path, made: &MadeClip) -> PathBuf {
    let key = format!(
        "{:x}",
        Sha256::digest(format!("{}\0{}\0{}", deadlock_core::text::casefold(&source.to_string_lossy()), made.action, made.mode.name()).as_bytes())
    );
    project_root.join("made-clips").join(&key[..16])
}

/// The kept packet's stamp when it is still current (same file stamp and
/// digest, action, mode, fit digest, script digest and packet digest).
pub fn current(folder: &Path, source: &Path, fit: &Path, made: &MadeClip, cancel: &CancellationToken) -> Option<Made> {
    let saved: Made = serde_json::from_slice(&fs::read(folder.join("made.json")).ok()?).ok()?;
    let (size, modified) = stamp(source).ok()?;
    (saved.version == STAMP_VERSION
        && saved.source == path_text(source).ok()?
        && saved.source_size == size
        && saved.source_modified == modified
        && saved.action == made.action
        && saved.mode == made.mode.name()
        && saved.script_sha256 == script_digest()
        && safety::digest_with_cancel(fit, cancel).ok()? == saved.fit_sha256
        && safety::digest_with_cancel(&folder.join("packet.json"), cancel).ok()? == saved.packet_sha256)
        .then_some(saved)
}

/// Samples the row's action into its packet (Blender), or returns the kept
/// packet when it is current.
pub fn make_packet(bridge: &BlenderBridge, project_root: &Path, source: &Path, fit: &Path, made: &MadeClip, cancel: &CancellationToken) -> Result<Made> {
    cancel.check()?;
    made.validate()?;
    let source = check_source(source)?;
    let folder = packet_folder(project_root, &source, made);
    if let Some(kept) = current(&folder, &source, fit, made, cancel) {
        return Ok(kept);
    }
    let before = safety::digest_with_cancel(&source, cancel)?;
    let fit_sha = safety::digest_with_cancel(fit, cancel)?;
    let (size, modified) = stamp(&source)?;
    if folder.exists() {
        fs::remove_dir_all(&folder)?;
    }
    fs::create_dir_all(&folder)?;
    #[derive(Deserialize)]
    struct Reply {
        samples: usize,
        length: f64,
        fps: f64,
    }
    let packet = folder.join("packet.json");
    let open = openable(&source, &folder)?;
    let reply: Reply = bridge.execute_script(
        SCRIPT,
        &Job { operation: "sample", source: path_text(&open)?, fit: path_text(fit)?, action: Some(&made.action), mode: Some(made.mode.name()), packet: Some(path_text(&packet)?) },
        &folder.join("blender"),
    )?;
    if safety::digest_with_cancel(&source, cancel)? != before || safety::digest_with_cancel(fit, cancel)? != fit_sha {
        return Err(invalid(CHANGED_WHILE_MAKING));
    }
    remove_copy(&folder);
    read_packet(&packet)?;
    let saved = Made {
        version: STAMP_VERSION.into(),
        source: path_text(&source)?,
        source_size: size,
        source_modified: modified,
        source_sha256: before,
        action: made.action.clone(),
        mode: made.mode.name().into(),
        fit_sha256: fit_sha,
        script_sha256: script_digest(),
        packet_sha256: safety::digest_with_cancel(&packet, cancel)?,
        samples: reply.samples,
        length_seconds: reply.length,
        fps: reply.fps,
    };
    write_json(&folder.join("made.json"), &saved)?;
    Ok(saved)
}

/// Keeps `export` as the packet of `source`'s `made` action with its stamp,
/// as `make_packet` does after Blender (tests and tools).
pub fn store_packet(project_root: &Path, source: &Path, fit: &Path, made: &MadeClip, export: &Export, cancel: &CancellationToken) -> Result<Made> {
    let source = check_source(source)?;
    let folder = packet_folder(project_root, &source, made);
    fs::create_dir_all(&folder)?;
    let packet = folder.join("packet.json");
    deadlock_core::safety::atomic_write(&packet, &serde_json::to_vec(export)?)?;
    let (size, modified) = stamp(&source)?;
    let saved = Made {
        version: STAMP_VERSION.into(),
        source: path_text(&source)?,
        source_size: size,
        source_modified: modified,
        source_sha256: safety::digest_with_cancel(&source, cancel)?,
        action: made.action.clone(),
        mode: made.mode.name().into(),
        fit_sha256: safety::digest_with_cancel(fit, cancel)?,
        script_sha256: script_digest(),
        packet_sha256: safety::digest_with_cancel(&packet, cancel)?,
        samples: export.samples.len(),
        length_seconds: export.length,
        fps: export.fps,
    };
    write_json(&folder.join("made.json"), &saved)?;
    Ok(saved)
}

fn read_packet(path: &Path) -> Result<Export> {
    let export: Export = serde_json::from_slice(&fs::read(path)?)?;
    if export.policy != PACKET_POLICY || export.samples.is_empty() || !export.length.is_finite() || export.length <= 0. {
        return Err(invalid(NOT_MADE));
    }
    Ok(export)
}

/// The kept packet for a build or the preview, with no Blender: refused with
/// `NOT_MADE` unless it was made for this file, action and mode and the
/// file, the fit and the packet still have the digests it was made from (the
/// file's stamp and the script's digest only decide reuse in `make_packet`,
/// so a touched file or an updated script never stops a build).
pub fn load(project_root: &Path, source: &Path, fit: &Path, made: &MadeClip, cancel: &CancellationToken) -> Result<(Made, Export)> {
    let not_made = || invalid(format!("{NOT_MADE}: {} ({})", made.action, source.display()));
    let source = check_source(source).map_err(|_| not_made())?;
    let folder = packet_folder(project_root, &source, made);
    let saved: Made = fs::read(folder.join("made.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).ok_or_else(not_made)?;
    let packet = folder.join("packet.json");
    if saved.version != STAMP_VERSION
        || saved.source != path_text(&source)?
        || saved.action != made.action
        || saved.mode != made.mode.name()
        || !packet.is_file()
        || safety::digest_with_cancel(fit, cancel)? != saved.fit_sha256
        || safety::digest_with_cancel(&source, cancel)? != saved.source_sha256
        || safety::digest_with_cancel(&packet, cancel)? != saved.packet_sha256
    {
        return Err(not_made());
    }
    Ok((saved, read_packet(&packet).map_err(|_| not_made())?))
}

/// Why the game's clip cannot be made from an animation, or None.
pub fn refusal(raw_clip: &[u8]) -> Result<Option<&'static str>> {
    let clip = Clip::decode(raw_clip).map_err(invalid)?;
    Ok(if clip.additive {
        Some(ADDITIVE)
    } else if clip.model_space_bones > 0 {
        Some(MODEL_SPACE)
    } else if clip.frames.len() < 2 {
        Some(SINGLE_POSE)
    } else if clip.frames.iter().flatten().any(|l| (l.scale - 1.).abs() > 1e-6) {
        Some(SCALED)
    } else {
        None
    })
}

/// What making one clip did, for the page and the receipt.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct MadeReport {
    pub action: String,
    pub mode: String,
    pub switches: Option<Switches>,
    pub clip_frames: usize,
    pub clip_seconds: f64,
    pub action_seconds: f64,
    /// Action seconds at the first and last clip frame.
    pub plays_from: f64,
    pub plays_to: f64,
    /// The action's length over the clip's (1: its own speed).
    pub stretch: f64,
    /// Bones that follow the animation.
    pub follow: Vec<String>,
    /// Bones that follow a bone of the animation rigidly, as the game's clip
    /// holds them (hand and leg IK targets).
    pub carried: Vec<String>,
    /// Bones between followed ones that the file has no bone for (rest).
    pub between: Vec<String>,
    /// How many bones keep the game's motion, by kind (fingers, gun, cloth,
    /// face, IK targets, other), with a few names of each.
    pub kept_count: usize,
    pub kept_kinds: BTreeMap<String, usize>,
    pub kept_examples: BTreeMap<String, Vec<String>>,
    /// Hero bones of the animation the game's clip does not have.
    pub not_in_clip: Vec<String>,
    /// Still tracks (fixed values in the game's clip).
    pub still_made_moving: Vec<String>,
    pub still_new_fixed_value: Vec<String>,
    pub still_held: Vec<String>,
    pub still_kept_game_value: Vec<String>,
    pub ranges_refitted: Vec<String>,
    pub ranges_clamped: Vec<String>,
    pub words_per_frame: (usize, usize),
    /// How far the clip's last frame was from its first before the loop blend.
    pub loop_gap_degrees: f64,
    pub loop_gap_inches: f64,
    pub loop_blend_seconds: Option<f64>,
    /// The horizontal travel `in_place` removed (inches), when on.
    pub drift_removed_inches: Option<f64>,
    /// The lowest foot against where the game's clip puts it: lowest and
    /// highest per frame (inches; negative is lower, below the floor).
    pub feet_lowest_inches: Option<f64>,
    pub feet_highest_inches: Option<f64>,
    /// Feet on the floor (`Switches::floor_lift`): the lift the clip holds,
    /// inches (absent when not applied).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub floor_lift_inches: Option<f64>,
    /// The off hand on the gun (`Switches::off_hand`): what the pass did, or
    /// (`off_hand_skipped`) why it did not run. Absent when it was not asked for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub off_hand: Option<clip_author::OffHand>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub off_hand_skipped: Option<String>,
}

fn kind_of(name: &str) -> &'static str {
    let n = name.to_ascii_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| n.contains(w));
    if has(&["iktarget", "_ik", "ik_"]) {
        "IK targets"
    } else if has(&["finger", "thumb", "index", "middle", "ring", "pinky"]) {
        "fingers"
    } else if has(&["weapon", "gun", "muzzle", "barrel", "magazine", "clip_", "trigger", "slide"]) {
        "gun"
    } else if has(&["cloth", "skirt", "cape", "coat", "hair", "tail", "strap", "hose", "chain", "bag", "tassel", "ribbon"]) {
        "cloth"
    } else if has(&["jaw", "eye", "brow", "lip", "cheek", "tongue", "lid", "mouth", "nose", "teeth"]) {
        "face"
    } else {
        "other"
    }
}

/// The game's clip with the packet's action (`made`'s timing, loop blend, in
/// place and bones, the `switches`). `skeleton` is the clip's `.vnmskel_c`.
pub fn author(raw_clip: &[u8], skeleton: &Skeleton, export: &Export, made: &MadeClip, switches: Switches) -> Result<(Vec<u8>, MadeReport)> {
    made.validate()?;
    if let Some(reason) = refusal(raw_clip)? {
        return Err(invalid(reason));
    }
    let data = container::read_blocks(raw_clip, "DATA").map_err(|e| invalid(e.to_string()))?;
    let value = &data.first().ok_or_else(|| invalid("The game's clip has no data."))?.root;
    let clip = Clip::from_value(value).map_err(invalid)?;
    let still = clip_encode::static_tracks(value).map_err(invalid)?;
    let options = Options {
        timing: made.part_start_seconds.map_or(Timing::Stretch, |start| Timing::Part { start }),
        loop_blend: made.loop_blend_seconds,
        bones: if made.bones == ClipBones::Keyed { Bones::Keyed } else { Bones::All },
        in_place: made.in_place,
        hold_still_tracks: !switches.still_to_moving,
        keep_fixed_values: !switches.new_fixed_values,
        floor_lift: switches.floor_lift,
        off_hand: switches.off_hand,
    };
    let (frames, authored) = clip_author::author(skeleton, &clip, export, &still, options).map_err(invalid)?;
    let ranges = if switches.refit_ranges { Ranges::Keep } else { Ranges::Never };
    let (bytes, encoded) = clip_encode::reencode_with(raw_clip, &frames, ranges, Statics::Rewrite).map_err(invalid)?;
    // The switches hold whatever authoring did.
    if !switches.still_to_moving && encoded.promoted_any() {
        return Err(invalid("A still track would start moving although that is switched off."));
    }
    if !switches.new_fixed_values && !(encoded.rotation_constant_rewritten.is_empty() && encoded.translation_constant_rewritten.is_empty()) {
        return Err(invalid("A still track would get a new fixed value although that is switched off."));
    }
    if !switches.refit_ranges && !encoded.ranges_changed.is_empty() {
        return Err(invalid("A value range would be re-fitted although that is switched off."));
    }
    let names = |v: &[usize]| -> Vec<String> {
        let mut out: Vec<String> = v.iter().map(|&i| skeleton.names[i].clone()).collect();
        out.sort();
        out.dedup();
        out
    };
    let mut kept_kinds = BTreeMap::new();
    let mut kept_examples: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for name in &authored.kept {
        let kind = kind_of(name).to_string();
        *kept_kinds.entry(kind.clone()).or_insert(0) += 1;
        let examples = kept_examples.entry(kind).or_default();
        if examples.len() < 3 {
            examples.push(name.clone());
        }
    }
    let n = clip.frames.len();
    let report = MadeReport {
        action: export.action.clone(),
        mode: made.mode.name().into(),
        switches: Some(switches),
        clip_frames: n,
        clip_seconds: clip.duration,
        action_seconds: export.length,
        plays_from: authored.source_span.0,
        plays_to: authored.source_span.1,
        stretch: authored.stretch,
        follow: authored.driven.clone(),
        carried: authored.followers.iter().map(|f| f.0.clone()).collect(),
        between: authored.between.clone(),
        kept_count: authored.kept.len(),
        kept_kinds,
        kept_examples,
        not_in_clip: authored.not_in_clip.clone(),
        still_made_moving: names(&[encoded.rotation_promoted.clone(), encoded.translation_promoted.clone(), encoded.scale_promoted.clone()].concat()),
        still_new_fixed_value: names(&[encoded.rotation_constant_rewritten.clone(), encoded.translation_constant_rewritten.clone()].concat()),
        still_held: authored.held_still.clone(),
        still_kept_game_value: authored.kept_game_fixed.clone(),
        ranges_refitted: names(&encoded.ranges_changed),
        ranges_clamped: names(&encoded.ranges_clamped),
        words_per_frame: encoded.words_per_frame,
        loop_gap_degrees: authored.loop_gap_degrees,
        loop_gap_inches: authored.loop_gap_inches,
        loop_blend_seconds: made.loop_blend_seconds,
        drift_removed_inches: made.in_place.then(|| authored.drift_removed.iter().map(|(_, d)| d[0].hypot(d[1])).fold(0., f64::max)),
        feet_lowest_inches: authored.feet_offset_inches.map(|f| f.0),
        feet_highest_inches: authored.feet_offset_inches.map(|f| f.1),
        floor_lift_inches: authored.floor_lift_inches,
        off_hand: authored.off_hand.clone(),
        off_hand_skipped: authored.off_hand_skipped.clone(),
    };
    Ok((bytes, report))
}

/// The digest a receipt keeps for a made clip's inputs besides the packet:
/// the row's choices and the switches.
pub fn choices_digest(made: &MadeClip, switches: Switches) -> String {
    format!("{:x}", Sha256::digest(serde_json::to_vec(&(made, switches)).unwrap_or_default()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_script_and_rust_agree_on_versions_and_sentences() {
        let script = include_str!("../resources/blender/animation_export.py");
        let fit = include_str!("../resources/blender/animation_fit.py");
        assert!(script.contains(&format!("POLICY = '{PACKET_POLICY}'")));
        assert!(script.contains(&format!("FIT_POLICY = '{FIT_POLICY}'")));
        assert!(fit.contains(&format!("POLICY = '{FIT_POLICY}'")) && fit.contains(&format!("FILE = '{FIT_FILE}'")));
        assert!(script.contains(&format!("SKELETON_DIFFERS = \"{SKELETON_DIFFERS}\"")));
        assert!(script.contains(&format!("SKELETON_ADVICE = '{SKELETON_ADVICE}'")));
        assert!(script.contains(&format!("NO_RIG = '{NO_RIG}'")));
        assert!(script.contains(&format!("NO_ACTION = '{NO_ACTION}'")));
        assert!(!script.contains("import worker") && !script.contains("from worker"), "its own entry, never worker.py");
    }

    /// FBX and glTF files are imported as the preparation imports them
    /// (worker.py `import_source`), so the armature's rest is the one the fit
    /// was saved from; making a clip only adds the animation (`use_anim`).
    #[test]
    fn animation_files_are_imported_with_the_preparations_settings() {
        let script = include_str!("../resources/blender/animation_export.py");
        let worker = include_str!("../resources/blender/worker.py");
        assert!(worker.contains("bpy.ops.import_scene.fbx(filepath=path, use_anim=False, automatic_bone_orientation=False)"), "the preparation's FBX import changed: change FBX_IMPORT with it");
        assert!(script.contains("FBX_IMPORT = {'use_anim': True, 'automatic_bone_orientation': False}"));
        assert!(worker.contains("bpy.ops.import_scene.gltf(filepath=path, merge_vertices=False, **optional)"), "the preparation's glTF import changed: change GLTF_IMPORT with it");
        assert!(worker.contains("optional = {'disable_bone_shape': True} if 'disable_bone_shape' in properties else {}"));
        assert!(script.contains("GLTF_IMPORT = {'merge_vertices': False}"));
        assert!(script.contains("optional = {'disable_bone_shape': True} if 'disable_bone_shape' in properties else {}"));
        assert!(script.contains("bpy.ops.import_scene.gltf(filepath=path, **GLTF_IMPORT, **optional)") && script.contains("bpy.ops.import_scene.fbx(filepath=path, **FBX_IMPORT)"));
        let suffixes: Vec<String> = deadlock_project::made_clip::SOURCE_SUFFIXES.iter().map(|s| format!("'.{s}'")).collect();
        assert!(script.contains(&format!("SUFFIXES = ({})", suffixes.join(", "))), "the script opens every file the saved format accepts");
    }

    #[test]
    fn the_switches_live_here_with_their_defaults() {
        assert_eq!(SWITCHES, Switches { new_fixed_values: true, refit_ranges: true, still_to_moving: true, floor_lift: true, off_hand: false });
    }

    #[test]
    fn switches_that_are_off_leave_saved_json_as_it_was() {
        // A switch that is off is left out of the receipt's choices digest and
        // the report's `switches`: the off hand is absent, the floor lift (on)
        // is written, and a set with both off keeps its earlier bytes.
        assert_eq!(serde_json::to_string(&SWITCHES).unwrap(), r#"{"new_fixed_values":true,"refit_ranges":true,"still_to_moving":true,"floor_lift":true}"#);
        let both_off = Switches { floor_lift: false, off_hand: false, ..SWITCHES };
        assert_eq!(serde_json::to_string(&both_off).unwrap(), r#"{"new_fixed_values":true,"refit_ranges":true,"still_to_moving":true}"#);
        let old: Switches = serde_json::from_str(r#"{"new_fixed_values":true,"refit_ranges":true,"still_to_moving":true}"#).unwrap();
        assert_eq!(old, both_off);
        assert_ne!(old, SWITCHES, "a receipt written before the lift was on does not match the app's switches");
        let on = Switches { floor_lift: true, off_hand: true, ..SWITCHES };
        assert!(serde_json::to_string(&on).unwrap().contains(r#""floor_lift":true,"off_hand":true"#));
        let report = serde_json::to_value(MadeReport::default()).unwrap();
        assert!(report.get("floor_lift_inches").is_none() && report.get("off_hand").is_none() && report.get("off_hand_skipped").is_none());
    }

    #[test]
    fn packets_are_per_file_action_and_mode() {
        let root = Path::new("C:/w/p");
        let wave = MadeClip::new("Wave");
        assert_eq!(packet_folder(root, Path::new("C:/a/Moves.blend"), &wave), packet_folder(root, Path::new("C:/a/moves.blend"), &wave));
        assert_ne!(packet_folder(root, Path::new("C:/a/m.blend"), &wave), packet_folder(root, Path::new("C:/a/m.blend"), &MadeClip::new("Run")));
        let mut other = wave.clone();
        other.mode = deadlock_project::made_clip::ClipMode::FromRest;
        assert_ne!(packet_folder(root, Path::new("C:/a/m.blend"), &wave), packet_folder(root, Path::new("C:/a/m.blend"), &other));
        // Timing, loop, in place and bones need no new packet.
        let mut timed = wave.clone();
        timed.part_start_seconds = Some(0.5);
        timed.in_place = false;
        assert_eq!(packet_folder(root, Path::new("C:/a/m.blend"), &wave), packet_folder(root, Path::new("C:/a/m.blend"), &timed));
    }

    #[test]
    fn missing_fits_and_packets_are_refused_with_their_sentences() {
        let temp = tempfile::tempdir().unwrap();
        let mut project = Project::default();
        assert!(fit_path(&project).unwrap_err().to_string().starts_with(NEEDS_PREPARED));
        let report = temp.path().join("prepared-x").join("preparation-report.json");
        fs::create_dir_all(report.parent().unwrap()).unwrap();
        project.preparation_report = report.to_string_lossy().into_owned();
        project.dmx = temp.path().join("prepared-x").join("prepared.dmx").to_string_lossy().into_owned();
        assert_eq!(fit_path(&project).unwrap_err().to_string(), NO_FIT);
        // Projects that can never get a fit by preparing again are told what the feature needs.
        let mut unrigged = project.clone();
        unrigged.model_input_mode = deadlock_project::ModelInputMode::UnriggedHumanoid;
        assert_eq!(fit_path(&unrigged).unwrap_err().to_string(), UNRIGGED_MODEL);
        let mut ready = Project::default();
        ready.model_input_mode = deadlock_project::ModelInputMode::Prepared;
        ready.dmx = "C:/models/hero.dmx".into();
        assert_eq!(fit_path(&ready).unwrap_err().to_string(), READY_MADE_MODEL);
        ready.dmx.clear();
        assert_eq!(fit_path(&ready).unwrap_err().to_string(), NEEDS_PREPARED, "nothing chosen yet");
        let mut rigged = Project::default();
        rigged.model_input_mode = deadlock_project::ModelInputMode::RiggedHumanoid;
        assert_eq!(fit_path(&rigged).unwrap_err().to_string(), NEEDS_PREPARED);
        fs::write(report.with_file_name(FIT_FILE), br#"{"policy":"animation-fit-v1"}"#).unwrap();
        let fit = fit_path(&project).unwrap();
        let blend = temp.path().join("moves.blend");
        fs::write(&blend, b"blend").unwrap();
        let error = load(temp.path(), &blend, &fit, &MadeClip::new("Wave"), &CancellationToken::new()).unwrap_err().to_string();
        assert!(error.starts_with(NOT_MADE), "{error}");
        for name in ["moves.fbx", "moves.glb", "moves.gltf"] {
            let other = temp.path().join(name);
            fs::write(&other, b"data").unwrap();
            assert!(check_source(&other).is_ok(), "{name}");
        }
        for name in ["moves.vnmclip_c", "moves.pmx"] {
            let other = temp.path().join(name);
            fs::write(&other, b"data").unwrap();
            assert!(check_source(&other).unwrap_err().to_string().starts_with(ONLY_ANIMATION_FILES), "{name}");
        }
    }

    #[test]
    fn a_gltf_too_long_for_blender_is_refused_with_its_sentence() {
        let temp = tempfile::tempdir().unwrap();
        let long = PathBuf::from(format!("C:\\{}\\moves.gltf", "x".repeat(270)));
        let error = openable(&long, temp.path()).unwrap_err().to_string();
        assert!(error.contains(GLTF_TOO_LONG) && error.contains("shorter folder"), "{error}");
        // A short path is opened where it is.
        let short = temp.path().join("moves.gltf");
        assert_eq!(openable(&short, temp.path()).unwrap(), short);
    }

    #[test]
    fn kept_bones_are_named_by_kind() {
        assert_eq!(kind_of("finger_index_1_L"), "fingers");
        assert_eq!(kind_of("weapon_hand_R"), "gun");
        assert_eq!(kind_of("arm_L_IKTARGET"), "IK targets");
        assert_eq!(kind_of("$cloth_cape_1"), "cloth");
        assert_eq!(kind_of("spine_2"), "other");
    }
}
