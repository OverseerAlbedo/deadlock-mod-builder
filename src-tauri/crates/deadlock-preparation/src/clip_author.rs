//! New poses for one of the hero's compiled clips from an action that
//! `resources/blender/animation_export.py` retargeted onto the hero's bones
//! (the packet). The output is per frame, per track of the CLIP's skeleton
//! (`.vnmskel_c`, track order), parent-space values as `clip_decode` returns
//! them, ready for `clip_encode::reencode_with`.
//!
//! Time: the clip keeps its own frame count n and length L; frame f is at
//! t = f / (n − 1) × L. `Timing::Stretch` plays the whole action over L;
//! `Timing::Part { start }` plays L seconds of it from `start` at its own
//! speed. Between the action's samples rotations are slerped and positions
//! interpolated linearly. `loop_blend` (seconds) eases the last part of the
//! clip into its first frame so the last frame equals the first.
//!
//! Bones, per clip track:
//! - root (`root_motion`): always the game's (root motion is never authored);
//! - a hero bone with a driver: rotation D · bind, where D is the anchor's
//!   actual world change (its new world rotation · its bind⁻¹) times the
//!   action's change relative to that anchor (`y`), or `y` alone without an
//!   anchor. Its translation is the game's local translation for that frame
//!   (the hero's bone lengths), except a bone without an anchor (the pelvis),
//!   which takes the action's position;
//! - `Bones::Keyed`: only bones whose source bone has keys in the action are
//!   driven; the others are treated as below;
//! - a bone between two driven bones that the source has no bone for (the
//!   hero's spine_0 when the source spine drives spine_1) takes the hero's
//!   rest rotation (the clip skeleton's reference pose), so a rest action
//!   gives the prepared rest exactly; its translation stays the game's;
//! - any other bone (fingers, weapon, hoses) keeps the game's local values
//!   under its (new) parent;
//! - a bone outside the driven chains that the game's clip holds rigidly to a
//!   driven bone or one of its descendants (hand IK targets to the hands, leg
//!   IK targets to the ankles in runs; decided by the clip's values, not by
//!   name) follows that bone rigidly: new = new(c) · game(c)⁻¹ · game(t); its
//!   children follow it.
//!
//! Still tracks (the game's clip stores a fixed rotation or translation):
//! - `hold_still_tracks` (the "make a still track moving" switch OFF): no
//!   still track moves. A driven bone whose rotation is still keeps one local
//!   rotation, the action's at the first frame, and the driven bones hanging
//!   from it take their own world rotation (the action's `delta`), so they
//!   still point where the action puts them; any other still rotation or
//!   translation that would move is held at its first frame's value.
//! - `keep_fixed_values` (the "give a still track a new fixed value" switch
//!   OFF): a still track that does not move keeps the game's value.
use crate::clip_decode::{normalize, product, Clip, Local, Skeleton};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub type Result<T> = std::result::Result<T, String>;
type Q = [f64; 4];
type V = [f64; 3];

/// Must equal `animation_export.py POLICY`.
pub const POLICY: &str = "source-action-retarget-v2";
/// A bone counts as held rigidly to another when their relative transform
/// changes by at most this many degrees plus inches over the clip.
pub const RIGID: f64 = 0.1;

/// The packet `animation_export.py` writes for one action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Export {
    pub policy: String,
    pub action: String,
    pub fps: f64,
    pub length: f64,
    pub bones: Vec<BoneRow>,
    pub samples: Vec<Sample>,
    #[serde(default)]
    pub unmatched_hero_bones: Vec<String>,
    /// "as_it_looks" or "from_rest"; both write the same fields, so
    /// authoring does not depend on it.
    #[serde(default)]
    pub mode: String,
    /// Bone names the action has keys for.
    #[serde(default)]
    pub keyed_source_bones: Vec<String>,
    /// The hero's foot bones (feet below or above the floor).
    #[serde(default)]
    pub feet: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoneRow {
    pub hero: String,
    pub source: String,
    pub anchor: Option<String>,
    pub keyed: bool,
    pub bind_rotation: Q,
    #[serde(default)]
    pub shoulder_bridge: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sample {
    pub time: f64,
    pub bones: BTreeMap<String, SampleBone>,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SampleBone {
    pub y: Q,
    pub delta: Q,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<V>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Timing {
    Stretch,
    Part { start: f64 },
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Bones {
    All,
    Keyed,
}
#[derive(Debug, Clone, Copy)]
pub struct Options {
    pub timing: Timing,
    pub loop_blend: Option<f64>,
    pub bones: Bones,
    /// Keep the action in place: remove the straight-line horizontal drift of
    /// every bone that takes the action's position (the pelvis) over the part
    /// of the action the clip plays. At clip frame f (of n) the position moves
    /// back by f / (n − 1) × (its horizontal position at the last frame − at
    /// the first), in the hero's model space (x forward, y left); the height
    /// (z), the sway about that line and the first frame's place are kept,
    /// and the last frame ends over the first. Applied before `loop_blend`.
    pub in_place: bool,
    /// No still track of the game's clip moves (see the module notes).
    pub hold_still_tracks: bool,
    /// A still track that does not move keeps the game's value.
    pub keep_fixed_values: bool,
    /// Feet on the floor: one vertical lift for the whole clip, added to the
    /// bones that take the action's position (the pelvis) in model space, so
    /// that the made clip's lowest foot point over the clip is the game's
    /// lowest over the clip (`floor_lift`). Bones carried by the body move
    /// with it; root motion and every other track keep their values.
    pub floor_lift: bool,
    /// The off hand stays on the gun (`off_hand`): per frame the off hand
    /// takes the game's grip relative to the gun hand, and the off arm is
    /// solved to reach it.
    pub off_hand: bool,
}

/// What the off-hand pass did (`Options::off_hand`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct OffHand {
    /// The hand that holds the gun (its bones hang from it) and the other.
    pub gun_hand: String,
    pub off_hand: String,
    /// Frames the off hand could not reach (the arm fully stretched, or the
    /// target closer than the bent arm allows) and the worst distance left.
    pub out_of_reach_frames: usize,
    pub worst_miss_inches: f64,
    /// How much the game's own grip (off hand relative to gun hand) changes
    /// over the clip: 0 when the game holds the gun the same way throughout.
    /// The pass copies the game's grip in every frame, also where the game's
    /// off hand lets go of the gun.
    pub game_grip_change_degrees: f64,
    pub game_grip_change_inches: f64,
}

/// Why the off-hand pass did not run although it was asked for.
pub const OFF_HAND_NO_GUN_HAND: &str = "No single gun hand: the gun's bones hang from both hands or from neither";
pub const OFF_HAND_NO_ARM: &str = "No off arm: the other hand, its forearm and upper arm are not all made from the animation";

#[derive(Debug, Clone, Default, Serialize)]
pub struct Report {
    /// Clip bones that take the action's rotation.
    pub driven: Vec<String>,
    /// Driven hero bones the clip skeleton does not have.
    pub not_in_clip: Vec<String>,
    /// (bone, the bone it follows, its rigidity in the game's clip).
    pub followers: Vec<(String, String, f64)>,
    /// Bones between driven bones the source has no bone for (rest rotation).
    pub between: Vec<String>,
    /// Bones that keep the game's motion (everything else but the root).
    pub kept: Vec<String>,
    /// Action seconds played at the first and last clip frame.
    pub source_span: (f64, f64),
    /// The action's length over the clip's when stretched (1 = same speed).
    pub stretch: f64,
    /// How far the last frame was from the first before `loop_blend`.
    pub loop_gap_degrees: f64,
    pub loop_gap_inches: f64,
    /// `in_place`: (bone, horizontal drift removed over the clip, inches x and y).
    pub drift_removed: Vec<(String, [f64; 2])>,
    /// Bones whose still rotation or translation was held at its first frame.
    pub held_still: Vec<String>,
    /// Bones whose still values were put back to the game's.
    pub kept_game_fixed: Vec<String>,
    /// Lowest and highest (new − game) height of the lowest foot per frame,
    /// inches (negative: below where the game's clip puts it).
    pub feet_offset_inches: Option<(f64, f64)>,
    /// `floor_lift`: the lift the made clip holds (inches, up positive), when
    /// it was applied.
    pub floor_lift_inches: Option<f64>,
    /// `off_hand`: what the pass did, or why it did not run.
    pub off_hand: Option<OffHand>,
    pub off_hand_skipped: Option<String>,
}

pub fn conj(q: Q) -> Q {
    [-q[0], -q[1], -q[2], q[3]]
}
pub fn rotate(q: Q, v: V) -> V {
    let p = product(product(q, [v[0], v[1], v[2], 0.]), conj(q));
    [p[0], p[1], p[2]]
}
fn add(a: V, b: V) -> V {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn length(a: V) -> f64 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}
pub fn slerp(a: Q, b: Q, t: f64) -> Q {
    let mut b = b;
    let mut d: f64 = (0..4).map(|k| a[k] * b[k]).sum();
    if d < 0. {
        b = b.map(|x| -x);
        d = -d;
    }
    if d > 0.9995 {
        return normalize(std::array::from_fn(|k| a[k] + (b[k] - a[k]) * t));
    }
    let theta = d.min(1.).acos();
    let (s, sa, sb) = (theta.sin(), ((1. - t) * theta).sin(), (t * theta).sin());
    normalize(std::array::from_fn(|k| (a[k] * sa + b[k] * sb) / s))
}
fn lerp(a: V, b: V, t: f64) -> V {
    std::array::from_fn(|k| a[k] + (b[k] - a[k]) * t)
}
fn smooth(x: f64) -> f64 {
    let x = x.clamp(0., 1.);
    x * x * (3. - 2. * x)
}
/// Degrees between two rotations (normalised first: near 1 the arc cosine
/// turns a 1e-7 length error into hundredths of a degree).
pub fn degrees(a: Q, b: Q) -> f64 {
    crate::clip_encode::angle(normalize(a), normalize(b)).to_degrees()
}

/// Model-space (rotation, position) of every bone of one frame (scale 1).
pub fn worlds(skel: &Skeleton, frame: &[Local]) -> Result<Vec<(Q, V)>> {
    let mut out: Vec<(Q, V)> = Vec::with_capacity(frame.len());
    for (i, l) in frame.iter().enumerate() {
        if (l.scale - 1.).abs() > 1e-6 {
            return Err(format!("{} is scaled in the game's clip; scaled tracks are not covered", skel.names[i]));
        }
        out.push(compose(if skel.parents[i] < 0 { None } else { Some(out[skel.parents[i] as usize]) }, l));
    }
    Ok(out)
}
fn compose(parent: Option<(Q, V)>, l: &Local) -> (Q, V) {
    match parent {
        None => (l.rotation, l.position),
        Some((pq, pp)) => (normalize(product(pq, l.rotation)), add(pp, rotate(pq, l.position))),
    }
}
fn relative(a: (Q, V), b: (Q, V)) -> (Q, V) {
    (normalize(product(conj(a.0), b.0)), rotate(conj(a.0), sub(b.1, a.1)))
}

/// The action at `s` seconds (held at its ends).
pub fn sample_at(export: &Export, s: f64) -> BTreeMap<String, SampleBone> {
    let samples = &export.samples;
    let last = samples.len() - 1;
    let s = s.clamp(samples[0].time, samples[last].time);
    let k = samples.partition_point(|x| x.time <= s).saturating_sub(1).min(last.saturating_sub(1));
    let (a, b) = (&samples[k], &samples[(k + 1).min(last)]);
    let span = b.time - a.time;
    let u = if span > 0. { ((s - a.time) / span).clamp(0., 1.) } else { 0. };
    a.bones
        .iter()
        .map(|(name, x)| {
            let y = b.bones.get(name).copied().unwrap_or(*x);
            let position = match (x.position, y.position) {
                (Some(p), Some(q)) => Some(lerp(p, q, u)),
                (p, _) => p,
            };
            (name.clone(), SampleBone { y: slerp(x.y, y.y, u), delta: slerp(x.delta, y.delta, u), position })
        })
        .collect()
}

fn subtree(skel: &Skeleton, root: usize) -> BTreeSet<usize> {
    let mut set = BTreeSet::from([root]);
    for i in root + 1..skel.names.len() {
        if skel.parents[i] >= 0 && set.contains(&(skel.parents[i] as usize)) {
            set.insert(i);
        }
    }
    set
}
fn depth(skel: &Skeleton, mut i: usize) -> usize {
    let mut d = 0;
    while skel.parents[i] >= 0 {
        i = skel.parents[i] as usize;
        d += 1;
    }
    d
}

/// The new frames (and what was done) for `clip` from `export`. `still`: per
/// track, whether the game's clip stores its rotation and its translation as
/// fixed values (`clip_encode::static_tracks`).
pub fn author(skel: &Skeleton, clip: &Clip, export: &Export, still: &[(bool, bool)], options: Options) -> Result<(Vec<Vec<Local>>, Report)> {
    if export.policy != POLICY {
        return Err(format!("the action packet has policy {}, expected {POLICY}", export.policy));
    }
    if clip.additive {
        return Err("additive clips are not covered".into());
    }
    if clip.model_space_bones > 0 {
        return Err("clips with model-space sampled bones are not covered".into());
    }
    let n = clip.frames.len();
    if n < 2 || export.samples.is_empty() {
        return Err("needs a clip with at least two frames and an action with samples".into());
    }
    if still.len() != skel.names.len() || clip.frames.iter().any(|f| f.len() != skel.names.len()) {
        return Err("the clip's tracks do not match its skeleton".into());
    }
    let rotation_static: Vec<bool> = still.iter().map(|s| s.0).collect();
    let index: BTreeMap<&str, usize> = skel.names.iter().enumerate().map(|(i, n)| (n.as_str(), i)).collect();
    let rows: BTreeMap<&str, &BoneRow> = export.bones.iter().map(|r| (r.hero.as_str(), r)).collect();
    let mut report = Report::default();
    let mut driven: Vec<Option<(&BoneRow, Option<usize>)>> = vec![None; skel.names.len()];
    let mut held = vec![false; skel.names.len()];
    for row in &export.bones {
        let Some(&i) = index.get(row.hero.as_str()) else {
            report.not_in_clip.push(row.hero.clone());
            continue;
        };
        if i == 0 || skel.parents[i] < 0 {
            continue; // the root is never authored
        }
        if options.bones == Bones::Keyed && !row.keyed {
            continue;
        }
        let anchor = match &row.anchor {
            None => None,
            Some(a) => {
                let &ai = index.get(a.as_str()).ok_or_else(|| format!("anchor {a} of {} is not in the clip skeleton", row.hero))?;
                let mut p = skel.parents[i];
                while p >= 0 && p as usize != ai {
                    p = skel.parents[p as usize];
                }
                if p < 0 {
                    return Err(format!("anchor {a} is not an ancestor of {} in the clip skeleton", row.hero));
                }
                if !rows.contains_key(a.as_str()) {
                    return Err(format!("anchor {a} of {} has no row", row.hero));
                }
                Some(ai)
            }
        };
        driven[i] = Some((row, anchor));
        if options.hold_still_tracks && rotation_static[i] {
            held[i] = true;
        }
        report.driven.push(row.hero.clone());
    }
    // In Keyed mode an anchor may not be driven: its child then turns
    // relative to the game's pose of it, as the action turns it relative to
    // its own anchor (a wave over the game's stance).
    let clip_length = clip.duration;
    let times: Vec<f64> = (0..n).map(|f| f as f64 / (n - 1) as f64 * clip_length).collect();
    let source: Vec<f64> = match options.timing {
        Timing::Stretch => {
            report.stretch = if clip_length > 0. { export.length / clip_length } else { 1. };
            times.iter().map(|t| if clip_length > 0. { t / clip_length * export.length } else { 0. }).collect()
        }
        Timing::Part { start } => {
            if !start.is_finite() || start < 0. || start + clip_length > export.length + 1e-6 {
                return Err(format!("{PART_TOO_LONG} (from {start:.3} s the clip needs {clip_length:.3} s, the action has {:.3} s)", export.length));
            }
            report.stretch = 1.;
            times.iter().map(|t| start + t).collect()
        }
    };
    report.source_span = (source[0], source[n - 1]);
    let mut sampled: Vec<BTreeMap<String, SampleBone>> = source.iter().map(|&s| sample_at(export, s)).collect();
    if options.in_place {
        let names: Vec<String> = sampled[0].iter().filter(|(_, b)| b.position.is_some()).map(|(k, _)| k.clone()).collect();
        for name in names {
            let (Some(a), Some(b)) = (sampled[0][&name].position, sampled[n - 1].get(&name).and_then(|x| x.position)) else { continue };
            let drift = [b[0] - a[0], b[1] - a[1]];
            for (f, frame) in sampled.iter_mut().enumerate() {
                let u = f as f64 / (n - 1) as f64;
                if let Some(p) = frame.get_mut(&name).and_then(|x| x.position.as_mut()) {
                    p[0] -= u * drift[0];
                    p[1] -= u * drift[1];
                }
            }
            report.drift_removed.push((name, drift));
        }
    }
    let first = sampled[0].clone();
    for (name, a) in &first {
        if let Some(b) = sampled[n - 1].get(name) {
            report.loop_gap_degrees = report.loop_gap_degrees.max(degrees(a.delta, b.delta));
            if let (Some(p), Some(q)) = (a.position, b.position) {
                report.loop_gap_inches = report.loop_gap_inches.max(length(sub(p, q)));
            }
        }
    }
    if let Some(window) = options.loop_blend.filter(|w| *w > 0.) {
        for f in 0..n {
            let u = smooth((times[f] - (clip_length - window)) / window);
            if u <= 0. {
                continue;
            }
            for (name, bone) in sampled[f].iter_mut() {
                if let Some(target) = first.get(name) {
                    bone.y = slerp(bone.y, target.y, u);
                    bone.delta = slerp(bone.delta, target.delta, u);
                    if let (Some(p), Some(q)) = (bone.position, target.position) {
                        bone.position = Some(lerp(p, q, u));
                    }
                }
            }
        }
    }
    let game: Vec<Vec<(Q, V)>> = clip.frames.iter().map(|f| worlds(skel, f)).collect::<Result<_>>()?;
    // Followers: bones outside the driven chains held rigidly to a changed bone.
    let mut changed = BTreeSet::new();
    for i in 0..skel.names.len() {
        if driven[i].is_some() {
            changed.extend(subtree(skel, i));
        }
    }
    let mut followers: Vec<(usize, usize)> = vec![];
    for t in 1..skel.names.len() {
        if changed.contains(&t) {
            continue;
        }
        let own = subtree(skel, t);
        let mut best_unchanged = f64::INFINITY;
        let mut candidates: Vec<(f64, usize, usize)> = vec![];
        for c in 0..skel.names.len() {
            if own.contains(&c) {
                continue;
            }
            let r0 = relative(game[0][c], game[0][t]);
            let dev = (0..n)
                .map(|f| {
                    let r = relative(game[f][c], game[f][t]);
                    degrees(r.0, r0.0) + length(sub(r.1, r0.1))
                })
                .fold(0f64, f64::max);
            if changed.contains(&c) {
                // Ties go to a driven bone, then to the deepest one.
                let rank = (if driven[c].is_some() { 0 } else { 1 }) * 1000 + (999 - depth(skel, c).min(999));
                candidates.push((dev, rank, c));
            } else {
                best_unchanged = best_unchanged.min(dev);
            }
        }
        let best = candidates.iter().map(|c| c.0).fold(f64::INFINITY, f64::min);
        if best <= RIGID && best <= best_unchanged {
            let &(dev, _, c) = candidates.iter().filter(|c| c.0 <= best + 0.005).min_by_key(|c| c.1).unwrap();
            followers.push((t, c));
            report.followers.push((skel.names[t].clone(), skel.names[c].clone(), dev));
        }
    }
    let follower_set: BTreeSet<usize> = followers.iter().map(|f| f.0).collect();
    followers.retain(|&(t, _)| {
        let mut p = skel.parents[t];
        while p >= 0 {
            if follower_set.contains(&(p as usize)) {
                return false;
            }
            p = skel.parents[p as usize];
        }
        true
    });
    report.followers.retain(|(name, _, _)| followers.iter().any(|&(t, _)| skel.names[t] == *name));
    let between: Vec<bool> = (0..skel.names.len())
        .map(|i| {
            if driven[i].is_some() || skel.parents[i] < 0 {
                return false;
            }
            let mut p = skel.parents[i];
            let mut above = false;
            while p >= 0 {
                above |= driven[p as usize].is_some();
                p = skel.parents[p as usize];
            }
            above && subtree(skel, i).into_iter().any(|d| d != i && driven[d].is_some())
        })
        .collect();
    report.between = (0..skel.names.len()).filter(|&i| between[i]).map(|i| skel.names[i].clone()).collect();
    let arm = if options.off_hand {
        // Bones with a row in the action (made from it, or able to be: in
        // `Bones::Keyed` the gun arm may keep the game's motion).
        let flags: Vec<bool> = (0..skel.names.len()).map(|i| skel.parents[i] >= 0 && rows.contains_key(skel.names[i].as_str())).collect();
        match off_arm(skel, &flags) {
            Ok(arm) => Some(arm),
            Err(reason) => {
                report.off_hand_skipped = Some(reason.to_string());
                None
            }
        }
    } else {
        None
    };
    // Everything from here to the still tracks runs once, or twice for the
    // floor lift (once to measure, once with the lift).
    let build = |lift: f64, report: &mut Report| -> Result<(Vec<Vec<Local>>, Option<OffHand>)> {
    report.kept_game_fixed.clear();
    // The off arm's joints keep what the pass gave them.
    let solved = |i: usize| arm.as_ref().is_some_and(|a| [a.shoulder, a.elbow, a.hand].contains(&i));
    let mut frames = Vec::with_capacity(n);
    let mut held_local: Vec<Q> = vec![[0., 0., 0., 1.]; skel.names.len()];
    let mut grip = arm.as_ref().map(|a| OffHand { gun_hand: skel.names[a.gun].clone(), off_hand: skel.names[a.hand].clone(), ..OffHand::default() });
    for f in 0..n {
        let local = &clip.frames[f];
        let mut new: Vec<(Q, V)> = Vec::with_capacity(skel.names.len());
        for i in 0..skel.names.len() {
            let parent = if skel.parents[i] < 0 { None } else { Some(new[skel.parents[i] as usize]) };
            let world = match driven[i] {
                Some((row, anchor)) => {
                    let bone = sampled[f].get(&row.hero).ok_or_else(|| format!("the action has no sample for {}", row.hero))?;
                    let change = match anchor {
                        Some(a) if held[a] => bone.delta,
                        Some(a) => {
                            let bind = rows[skel.names[a].as_str()].bind_rotation;
                            product(product(new[a].0, conj(bind)), bone.y)
                        }
                        None => bone.y,
                    };
                    let mut q = normalize(product(change, row.bind_rotation));
                    if held[i] {
                        let parent_rotation = parent.map(|p| p.0).unwrap_or([0., 0., 0., 1.]);
                        if f == 0 {
                            held_local[i] = normalize(product(conj(parent_rotation), q));
                        }
                        q = normalize(product(parent_rotation, held_local[i]));
                    }
                    let p = match (anchor, bone.position) {
                        (None, Some(p)) => {
                            let mut w = add(new[0].1, rotate(new[0].0, p));
                            if lift != 0. {
                                w[2] += lift;
                            }
                            w
                        }
                        _ => compose(parent, &local[i]).1,
                    };
                    (q, p)
                }
                None if between[i] => compose(parent, &Local { rotation: skel.reference[i].rotation, ..local[i] }),
                None => compose(parent, &local[i]),
            };
            new.push(world);
        }
        if let (Some(a), Some(grip)) = (&arm, grip.as_mut()) {
            // The game's grip placed on the made gun hand; the off arm reaches for it.
            let now = relative(game[f][a.gun], game[f][a.hand]);
            let target = (normalize(product(new[a.gun].0, now.0)), add(new[a.gun].1, rotate(new[a.gun].0, now.1)));
            let fallback = sub(game[f][a.elbow].1, game[f][a.shoulder].1);
            let solved = reach(new[a.shoulder].1, new[a.elbow].1, new[a.hand].1, target.1, fallback);
            let s = new[a.shoulder].1;
            let up = swing(sub(new[a.elbow].1, s), sub(solved.elbow, s));
            turn_about(&mut new, &a.shoulder_tree, s, up);
            let e = new[a.elbow].1;
            let fore = swing(sub(new[a.hand].1, e), sub(solved.hand, e));
            turn_about(&mut new, &a.elbow_tree, e, fore);
            let h = new[a.hand].1;
            let turn = normalize(product(target.0, conj(new[a.hand].0)));
            turn_about(&mut new, &a.hand_tree, h, turn);
            if solved.miss > 1e-6 {
                grip.out_of_reach_frames += 1;
                grip.worst_miss_inches = grip.worst_miss_inches.max(solved.miss);
            }
            let first = relative(game[0][a.gun], game[0][a.hand]);
            grip.game_grip_change_degrees = grip.game_grip_change_degrees.max(degrees(now.0, first.0));
            grip.game_grip_change_inches = grip.game_grip_change_inches.max(length(sub(now.1, first.1)));
        }
        for &(t, c) in &followers {
            let rel = relative(game[f][c], game[f][t]);
            new[t] = (normalize(product(new[c].0, rel.0)), add(new[c].1, rotate(new[c].0, rel.1)));
            for d in subtree(skel, t).into_iter().skip(1) {
                new[d] = compose(Some(new[skel.parents[d] as usize]), &local[d]);
            }
        }
        let locals: Vec<Local> = (0..skel.names.len())
            .map(|i| {
                let (rotation, position) = if skel.parents[i] < 0 { new[i] } else { relative(new[skel.parents[i] as usize], new[i]) };
                Local { position, rotation, scale: local[i].scale }
            })
            .collect();
        frames.push(locals);
    }
    // Untouched tracks keep the game's exact values (no float noise from the
    // world round trip), so still tracks stay still.
    for i in 0..skel.names.len() {
        if let Some((row, anchor)) = driven[i] {
            let own_position = anchor.is_none() && export.samples[0].bones.get(&row.hero).is_some_and(|b| b.position.is_some());
            if !own_position {
                for f in 0..n {
                    frames[f][i].position = clip.frames[f][i].position;
                }
            }
        }
        if held[i] {
            for frame in frames.iter_mut() {
                frame[i].rotation = held_local[i];
            }
        }
        if driven[i].is_none() && !followers.iter().any(|&(t, _)| t == i) && !solved(i) {
            for f in 0..n {
                frames[f][i] = if between[i] { Local { rotation: skel.reference[i].rotation, ..clip.frames[f][i] } } else { clip.frames[f][i] };
            }
        }
    }
    report.kept = (1..skel.names.len())
        .filter(|&i| driven[i].is_none() && !between[i] && !followers.iter().any(|&(t, _)| t == i) && !solved(i))
        .map(|i| skel.names[i].clone())
        .collect();
    // Still tracks.
    let still_rot = |frames: &[Vec<Local>], i: usize| frames.iter().all(|f| crate::clip_encode::angle(f[i].rotation, frames[0][i].rotation) <= crate::clip_encode::STATIC_ROTATION);
    let still_pos = |frames: &[Vec<Local>], i: usize| frames.iter().all(|f| (0..3).all(|k| (f[i].position[k] - frames[0][i].position[k]).abs() <= crate::clip_encode::STATIC_POSITION));
    let mut held_names = BTreeSet::new();
    for i in 0..skel.names.len() {
        if held[i] {
            held_names.insert(i);
        }
        if options.hold_still_tracks {
            if still[i].0 && !still_rot(&frames, i) {
                let q = frames[0][i].rotation;
                frames.iter_mut().for_each(|f| f[i].rotation = q);
                held_names.insert(i);
            }
            if still[i].1 && !still_pos(&frames, i) {
                let p = frames[0][i].position;
                frames.iter_mut().for_each(|f| f[i].position = p);
                held_names.insert(i);
            }
        }
        if options.keep_fixed_values {
            let mut kept = false;
            if still[i].0 && still_rot(&frames, i) && crate::clip_encode::angle(frames[0][i].rotation, clip.frames[0][i].rotation) > crate::clip_encode::STATIC_ROTATION {
                for (f, frame) in frames.iter_mut().enumerate() {
                    frame[i].rotation = clip.frames[f][i].rotation;
                }
                kept = true;
            }
            if still[i].1 && still_pos(&frames, i) && (0..3).any(|k| (frames[0][i].position[k] - clip.frames[0][i].position[k]).abs() > crate::clip_encode::STATIC_POSITION) {
                for (f, frame) in frames.iter_mut().enumerate() {
                    frame[i].position = clip.frames[f][i].position;
                }
                kept = true;
            }
            if kept {
                report.kept_game_fixed.push(skel.names[i].clone());
            }
        }
    }
    report.held_still = held_names.into_iter().map(|i| skel.names[i].clone()).collect();
    Ok((frames, grip))
    };
    let feet: Vec<usize> = export.feet.iter().filter_map(|n| index.get(n.as_str()).copied()).collect();
    let (mut frames, grip) = build(0., &mut report)?;
    if options.floor_lift && !feet.is_empty() && export.bones.iter().any(|r| r.anchor.is_none() && driven[index.get(r.hero.as_str()).copied().unwrap_or(0)].is_some()) {
        // The feet and everything below them (toes): the lowest point over the
        // whole clip, the made clip's against the game's.
        let points: BTreeSet<usize> = feet.iter().flat_map(|&i| subtree(skel, i)).collect();
        let lowest = |frames: &[Vec<Local>]| -> Result<f64> {
            let mut low = f64::INFINITY;
            for frame in frames {
                let w = worlds(skel, frame)?;
                low = points.iter().map(|&i| w[i].1[2]).fold(low, f64::min);
            }
            Ok(low)
        };
        let theirs = game.iter().flat_map(|w| points.iter().map(move |&i| w[i].1[2])).fold(f64::INFINITY, f64::min);
        let before = lowest(&frames)?;
        let lift = theirs - before;
        if lift.is_finite() && lift != 0. {
            let (lifted, lifted_grip) = build(lift, &mut report)?;
            // What the final frames hold (a fixed value put back to the
            // game's, for example, undoes it).
            let achieved = lowest(&lifted)? - before;
            if achieved.abs() > 1e-6 {
                report.floor_lift_inches = Some(achieved);
                frames = lifted;
                report.off_hand = lifted_grip;
            } else {
                build(0., &mut report)?;
                report.off_hand = grip;
            }
        } else {
            report.off_hand = grip;
        }
    } else {
        report.off_hand = grip;
    }
    // Feet: the lowest foot per frame against the game's.
    if !feet.is_empty() {
        let (mut low, mut high) = (f64::INFINITY, f64::NEG_INFINITY);
        for f in 0..n {
            let w = worlds(skel, &frames[f])?;
            let mine = feet.iter().map(|&i| w[i].1[2]).fold(f64::INFINITY, f64::min);
            let theirs = feet.iter().map(|&i| game[f][i].1[2]).fold(f64::INFINITY, f64::min);
            low = low.min(mine - theirs);
            high = high.max(mine - theirs);
        }
        report.feet_offset_inches = Some((low, high));
    }
    Ok((frames, report))
}

/// The off arm the off-hand pass solves: the gun hand, the other hand, its
/// forearm and upper arm (each the nearest made bone above the one before),
/// and the bones that turn with each joint.
#[derive(Debug, Clone, PartialEq)]
pub struct Arm {
    pub gun: usize,
    pub hand: usize,
    pub elbow: usize,
    pub shoulder: usize,
    pub shoulder_tree: Vec<usize>,
    pub elbow_tree: Vec<usize>,
    pub hand_tree: Vec<usize>,
}

fn nearest(skel: &Skeleton, driven: &[bool], i: usize) -> Option<usize> {
    let mut p = skel.parents[i];
    while p >= 0 {
        if driven[p as usize] {
            return Some(p as usize);
        }
        p = skel.parents[p as usize];
    }
    None
}
fn ancestors(skel: &Skeleton, mut i: usize) -> Vec<usize> {
    let mut out = vec![i];
    while skel.parents[i] >= 0 {
        i = skel.parents[i] as usize;
        out.push(i);
    }
    out
}

/// The off arm from the hero's skeleton: the gun hand is the one made bone
/// that the gun's bones (names with "weapon") hang from; a gun that hangs
/// from both hands (two guns) or from neither (from the root) has none. The
/// off hand is the other made bone at the gun hand's depth whose forearm and
/// upper arm are made bones too, on another branch (the other arm, not a
/// leg: the deepest branch point wins, and a tie is no answer).
pub fn off_arm(skel: &Skeleton, driven: &[bool]) -> std::result::Result<Arm, &'static str> {
    let hands: BTreeSet<usize> = (0..skel.names.len()).filter(|&i| skel.names[i].to_ascii_lowercase().contains("weapon")).filter_map(|i| nearest(skel, driven, i)).collect();
    if hands.len() != 1 {
        return Err(OFF_HAND_NO_GUN_HAND);
    }
    let gun = *hands.iter().next().unwrap();
    // A hand ends its chain of made bones (the chest above an unmade arm is not one).
    if subtree(skel, gun).into_iter().any(|d| d != gun && driven[d]) {
        return Err(OFF_HAND_NO_GUN_HAND);
    }
    let Some(gun_elbow) = nearest(skel, driven, gun) else { return Err(OFF_HAND_NO_ARM) };
    let Some(gun_shoulder) = nearest(skel, driven, gun_elbow) else { return Err(OFF_HAND_NO_ARM) };
    let gun_line = ancestors(skel, gun);
    let mut best: Vec<(usize, usize, usize, usize)> = vec![];
    for h in 0..skel.names.len() {
        if !driven[h] || h == gun || depth(skel, h) != depth(skel, gun) || gun_line.contains(&h) || ancestors(skel, h).contains(&gun) {
            continue;
        }
        let Some(e) = nearest(skel, driven, h) else { continue };
        let Some(s) = nearest(skel, driven, e) else { continue };
        if e == gun_elbow || s == gun_shoulder || gun_line.contains(&s) {
            continue;
        }
        let Some(&branch) = ancestors(skel, s).iter().find(|a| gun_line.contains(a)) else { continue };
        best.push((depth(skel, branch), h, e, s));
    }
    let top = best.iter().map(|b| b.0).max().ok_or(OFF_HAND_NO_ARM)?;
    let chosen: Vec<_> = best.iter().filter(|b| b.0 == top).collect();
    if chosen.len() != 1 {
        return Err(OFF_HAND_NO_ARM);
    }
    let (_, hand, elbow, shoulder) = *chosen[0];
    let tree = |i: usize| subtree(skel, i).into_iter().collect::<Vec<_>>();
    Ok(Arm { gun, hand, elbow, shoulder, shoulder_tree: tree(shoulder), elbow_tree: tree(elbow), hand_tree: tree(hand) })
}

/// Where a two-bone arm reaches: the elbow and the hand.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reach {
    pub elbow: V,
    pub hand: V,
    /// How far the hand stays from the target (0 when reached).
    pub miss: f64,
}

fn unit(v: V) -> Option<V> {
    let l = length(v);
    (l > 1e-12).then(|| [v[0] / l, v[1] / l, v[2] / l])
}
fn dot(a: V, b: V) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn scale(a: V, s: f64) -> V {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn cross(a: V, b: V) -> V {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
/// The part of `v` across the unit axis `u`, as a unit vector, if any.
fn across(v: V, u: V) -> Option<V> {
    let w = sub(v, scale(u, dot(v, u)));
    (length(w) > 1e-6 * length(v).max(1.)).then(|| unit(w)).flatten()
}

/// Two-bone reach: shoulder `s`, elbow `e`, hand `h` (their distances are
/// the arm's lengths) towards `target`. The elbow stays on the side of the
/// line from the shoulder to the target that it has now; for an arm with no
/// side (straight, or bent exactly along that line) it takes the side of
/// `fallback` (the game's elbow offset from its shoulder in that frame).
/// Out of reach, the arm reaches as far as it goes along the same line.
pub fn reach(s: V, e: V, h: V, target: V, fallback: V) -> Reach {
    let (a, b) = (length(sub(e, s)), length(sub(h, e)));
    let to = sub(target, s);
    let d = length(to);
    let u = unit(to).or_else(|| unit(sub(h, s))).unwrap_or([1., 0., 0.]);
    let reach = d.clamp((a - b).abs(), a + b);
    let x = if reach > 1e-12 { (a * a - b * b + reach * reach) / (2. * reach) } else { 0. };
    let height = (a * a - x * x).max(0.).sqrt();
    let side = across(sub(e, s), u)
        .or_else(|| across(fallback, u))
        .or_else(|| across([0., 0., -1.], u))
        .or_else(|| across([1., 0., 0.], u))
        .unwrap_or([0., 1., 0.]);
    let elbow = add(add(s, scale(u, x)), scale(side, height));
    Reach { elbow, hand: add(s, scale(u, reach)), miss: (d - reach).abs() }
}

/// The smallest rotation taking the direction `from` onto `to`.
pub fn swing(from: V, to: V) -> Q {
    let (Some(a), Some(b)) = (unit(from), unit(to)) else { return [0., 0., 0., 1.] };
    let c = dot(a, b);
    if c < -1. + 1e-12 {
        let axis = unit(cross(a, [1., 0., 0.])).or_else(|| unit(cross(a, [0., 1., 0.]))).unwrap();
        return [axis[0], axis[1], axis[2], 0.];
    }
    let v = cross(a, b);
    normalize([v[0], v[1], v[2], 1. + c])
}

/// Turns the bones `tree` rigidly by `q` about the point `pivot`.
fn turn_about(new: &mut [(Q, V)], tree: &[usize], pivot: V, q: Q) {
    for &i in tree {
        new[i] = (normalize(product(q, new[i].0)), add(pivot, rotate(q, sub(new[i].1, pivot))));
    }
}

/// The start of the error when a part of the action is too short for the clip.
pub const PART_TOO_LONG: &str = "The part of the animation that plays does not fit: it starts too late for the clip's length";

#[cfg(test)]
mod tests {
    use super::*;

    fn skel() -> Skeleton {
        Skeleton {
            id: "t".into(),
            names: ["root", "pelvis", "arm", "hand", "ik"].map(String::from).to_vec(),
            parents: vec![-1, 0, 1, 2, 0],
            reference: vec![Local::IDENTITY; 5],
        }
    }
    fn local(p: V) -> Local {
        Local { position: p, rotation: [0., 0., 0., 1.], scale: 1. }
    }
    fn row(hero: &str, anchor: Option<&str>) -> BoneRow {
        BoneRow { hero: hero.into(), source: hero.into(), anchor: anchor.map(String::from), keyed: true, bind_rotation: [0., 0., 0., 1.], shoulder_bridge: false }
    }
    fn export(name: &str, bones: Vec<BoneRow>, samples: Vec<Sample>) -> Export {
        Export { policy: POLICY.into(), action: name.into(), fps: 4., length: 1., bones, samples, unmatched_hero_bones: vec![], mode: "as_it_looks".into(), keyed_source_bones: vec![], feet: vec![] }
    }
    fn options() -> Options {
        Options { timing: Timing::Stretch, loop_blend: None, bones: Bones::All, in_place: false, hold_still_tracks: false, keep_fixed_values: false, floor_lift: false, off_hand: false }
    }
    fn turn(z: f64) -> Q {
        [0., 0., (z / 2.).sin(), (z / 2.).cos()]
    }

    #[test]
    fn a_rest_action_gives_the_bind_pose_and_ik_targets_follow_their_hand() {
        let s = skel();
        let mut frames = vec![];
        for f in 0..3 {
            let a = (f as f64 * 15f64).to_radians();
            let arm = Local { position: [0., 0., 10.], rotation: turn(a), scale: 1. };
            let b = (f as f64 * 10f64).to_radians();
            let hand = Local { position: [5., 0., 0.], rotation: [(b / 2.).sin(), 0., 0., (b / 2.).cos()], scale: 1. };
            let mut pose = vec![local([0.; 3]), local([0., 0., 40.]), arm, hand, Local::IDENTITY];
            let w = worlds(&s, &pose).unwrap();
            pose[4] = Local { position: w[3].1, rotation: w[3].0, scale: 1. };
            frames.push(pose);
        }
        let clip = Clip { skeleton: "t".into(), duration: 1., additive: false, frames, root_motion: vec![], model_space_bones: 0 };
        let still = SampleBone { y: [0., 0., 0., 1.], delta: [0., 0., 0., 1.], position: None };
        let mut bones = BTreeMap::new();
        bones.insert("pelvis".to_string(), SampleBone { position: Some([0., 0., 40.]), ..still });
        bones.insert("arm".to_string(), still);
        let e = export("rest", vec![row("pelvis", None), row("arm", Some("pelvis"))], vec![Sample { time: 0., bones: bones.clone() }, Sample { time: 1., bones }]);
        let (out, report) = author(&s, &clip, &e, &[(false, false); 5], options()).unwrap();
        assert_eq!(report.followers.len(), 1, "{report:?}");
        assert_eq!(report.followers[0].1, "hand");
        assert_eq!(report.kept, vec!["hand".to_string()]);
        for f in 0..3 {
            let w = worlds(&s, &out[f]).unwrap();
            assert!(degrees(w[2].0, [0., 0., 0., 1.]) < 1e-9, "the arm is at its bind rotation");
            assert!(length(sub(w[4].1, w[3].1)) < 1e-9 && degrees(w[4].0, w[3].0) < 1e-9, "the IK target is on the hand");
        }
    }

    #[test]
    fn in_place_removes_the_straight_line_drift_and_keeps_height_and_sway() {
        let s = skel();
        let frames: Vec<Vec<Local>> = (0..5).map(|_| vec![local([0.; 3]), local([0., 0., 40.]), local([0., 0., 10.]), local([5., 0., 0.]), Local::IDENTITY]).collect();
        let clip = Clip { skeleton: "t".into(), duration: 1., additive: false, frames, root_motion: vec![], model_space_bones: 0 };
        let still = SampleBone { y: [0., 0., 0., 1.], delta: [0., 0., 0., 1.], position: None };
        let samples: Vec<Sample> = (0..5)
            .map(|k| {
                let t = k as f64 / 4.;
                let sway = 2. * (std::f64::consts::PI * 2. * t).sin();
                let mut bones = BTreeMap::new();
                bones.insert("pelvis".to_string(), SampleBone { position: Some([3. + 40. * t, sway, 40. + (std::f64::consts::PI * 4. * t).cos()]), ..still });
                Sample { time: t, bones }
            })
            .collect();
        let e = export("walk", vec![row("pelvis", None)], samples);
        let (out, report) = author(&s, &clip, &e, &[(false, false); 5], Options { in_place: true, ..options() }).unwrap();
        assert_eq!(report.drift_removed.len(), 1);
        assert!((report.drift_removed[0].1[0] - 40.).abs() < 1e-9);
        for (k, f) in out.iter().enumerate() {
            let t = k as f64 / 4.;
            let p = f[1].position;
            assert!((p[0] - 3.).abs() < 1e-9, "forward travel removed, the first frame's place kept: {p:?}");
            assert!((p[1] - 2. * (std::f64::consts::PI * 2. * t).sin()).abs() < 1e-9, "sway kept");
            assert!((p[2] - 40. - (std::f64::consts::PI * 4. * t).cos()).abs() < 1e-9, "height kept");
        }
    }

    /// The game holds the pelvis rotation still; the action turns it.
    fn turning() -> (Skeleton, Clip, Export) {
        let s = skel();
        let frames: Vec<Vec<Local>> = (0..5).map(|_| vec![local([0.; 3]), local([0., 0., 40.]), local([0., 0., 10.]), local([5., 0., 0.]), local([5., 0., 50.])]).collect();
        let clip = Clip { skeleton: "t".into(), duration: 1., additive: false, frames, root_motion: vec![], model_space_bones: 0 };
        let samples: Vec<Sample> = (0..5)
            .map(|k| {
                let t = k as f64 / 4.;
                let q = turn(0.2 + 0.3 * t);
                let mut bones = BTreeMap::new();
                bones.insert("pelvis".to_string(), SampleBone { y: q, delta: q, position: Some([0., 0., 40.]) });
                bones.insert("arm".to_string(), SampleBone { y: [0., 0., 0., 1.], delta: q, position: None });
                Sample { time: t, bones }
            })
            .collect();
        (s, clip, export("turn", vec![row("pelvis", None), row("arm", Some("pelvis"))], samples))
    }

    #[test]
    fn still_tracks_move_when_allowed_and_are_held_otherwise() {
        let (s, clip, e) = turning();
        let still = [(false, false), (true, true), (false, false), (false, false), (false, false)];
        // Allowed: the pelvis turns over the clip.
        let (out, report) = author(&s, &clip, &e, &still, options()).unwrap();
        assert!(degrees(out[4][1].rotation, out[0][1].rotation) > 10.);
        assert!(report.held_still.is_empty());
        // Held: one rotation, the first frame's; the arm still points where the action puts it.
        let (out, report) = author(&s, &clip, &e, &still, Options { hold_still_tracks: true, ..options() }).unwrap();
        assert!(out.iter().all(|f| degrees(f[1].rotation, out[0][1].rotation) < 1e-9));
        assert!((degrees(out[0][1].rotation, turn(0.2))) < 1e-9, "the first frame's rotation");
        let w = worlds(&s, &out[4]).unwrap();
        assert!(degrees(w[2].0, turn(0.5)) < 1e-9, "the arm takes its own world rotation");
        assert_eq!(report.held_still, vec!["pelvis".to_string()]);
    }

    #[test]
    fn still_tracks_that_do_not_move_take_a_new_value_or_keep_the_games() {
        let (s, clip, mut e) = turning();
        for sample in &mut e.samples {
            let q = turn(0.4);
            sample.bones.insert("pelvis".into(), SampleBone { y: q, delta: q, position: Some([0., 0., 40.]) });
        }
        let still = [(false, false), (true, true), (false, false), (false, false), (false, false)];
        let (out, report) = author(&s, &clip, &e, &still, options()).unwrap();
        assert!(degrees(out[2][1].rotation, turn(0.4)) < 1e-9, "a new fixed value");
        assert!(report.kept_game_fixed.is_empty());
        let (out, report) = author(&s, &clip, &e, &still, Options { keep_fixed_values: true, ..options() }).unwrap();
        assert!(degrees(out[2][1].rotation, [0., 0., 0., 1.]) < 1e-9, "the game's value");
        assert_eq!(report.kept_game_fixed, vec!["pelvis".to_string()]);
    }

    #[test]
    fn timing_stretches_or_plays_a_part_at_its_own_speed() {
        let (s, clip, mut e) = turning();
        e.length = 2.;
        for (k, sample) in e.samples.iter_mut().enumerate() {
            sample.time = k as f64 / 2.;
        }
        let (_, report) = author(&s, &clip, &e, &[(false, false); 5], options()).unwrap();
        assert!((report.stretch - 2.).abs() < 1e-12 && report.source_span == (0., 2.));
        let (_, report) = author(&s, &clip, &e, &[(false, false); 5], Options { timing: Timing::Part { start: 0.5 }, ..options() }).unwrap();
        assert_eq!((report.stretch, report.source_span), (1., (0.5, 1.5)));
        let error = author(&s, &clip, &e, &[(false, false); 5], Options { timing: Timing::Part { start: 1.5 }, ..options() }).unwrap_err();
        assert!(error.starts_with(PART_TOO_LONG), "{error}");
    }

    #[test]
    fn a_loop_blend_closes_the_gap() {
        let (s, clip, e) = turning();
        let (out, report) = author(&s, &clip, &e, &[(false, false); 5], Options { loop_blend: Some(0.5), ..options() }).unwrap();
        assert!((report.loop_gap_degrees - 0.3f64.to_degrees()).abs() < 1e-6);
        assert!(degrees(out[4][1].rotation, out[0][1].rotation) < 1e-9);
    }

    fn close(a: V, b: V) -> bool {
        length(sub(a, b)) < 1e-9
    }

    /// root, pelvis, leg, foot (ankle) and a toe below it. The game stands the
    /// foot at 0 and the toe at -1; the action bobs the pelvis at 37..38.5,
    /// under a root that turns and moves forward.
    fn standing() -> (Skeleton, Clip, Export) {
        let s = Skeleton { id: "t".into(), names: ["root", "pelvis", "leg", "foot", "toe"].map(String::from).to_vec(), parents: vec![-1, 0, 1, 2, 3], reference: vec![Local::IDENTITY; 5] };
        let frames: Vec<Vec<Local>> = (0..5)
            .map(|f| {
                let root = Local { position: [f as f64 * 3., 0., 0.], rotation: turn(f as f64 * 0.1), scale: 1. };
                vec![root, local([0., 0., 40.]), local([0., 0., -20.]), local([0., 0., -20.]), local([5., 0., -1.])]
            })
            .collect();
        let clip = Clip { skeleton: "t".into(), duration: 1., additive: false, frames, root_motion: vec![], model_space_bones: 0 };
        let heights = [38., 37., 38.5, 37.5, 38.];
        let still = SampleBone { y: [0., 0., 0., 1.], delta: [0., 0., 0., 1.], position: None };
        let samples = (0..5)
            .map(|k| {
                let mut bones = BTreeMap::new();
                bones.insert("pelvis".to_string(), SampleBone { position: Some([0., 0., heights[k]]), ..still });
                Sample { time: k as f64 / 4., bones }
            })
            .collect();
        let mut e = export("stand", vec![row("pelvis", None)], samples);
        e.feet = vec!["foot".into()];
        (s, clip, e)
    }

    #[test]
    fn the_floor_lift_is_one_lift_for_the_whole_clip_from_the_lowest_points() {
        let (s, clip, e) = standing();
        let still = [(false, false); 5];
        let (plain, report) = author(&s, &clip, &e, &still, options()).unwrap();
        assert_eq!(report.floor_lift_inches, None, "off: nothing lifted");
        let (lifted, report) = author(&s, &clip, &e, &still, Options { floor_lift: true, ..options() }).unwrap();
        // The made toe's lowest is 37 - 41 = -4, the game's -1: a lift of 3.
        assert!((report.floor_lift_inches.unwrap() - 3.).abs() < 1e-9, "{report:?}");
        for f in 0..5 {
            let (a, b) = (worlds(&s, &plain[f]).unwrap(), worlds(&s, &lifted[f]).unwrap());
            assert!(close(b[1].1, add(a[1].1, [0., 0., 3.])), "the body is lifted by 3 in model space in every frame");
            assert_eq!(plain[f][0], lifted[f][0], "root motion untouched");
            assert_eq!(&plain[f][2..], &lifted[f][2..], "the tracks below keep their values");
        }
        let lowest = (0..5).map(|f| worlds(&s, &lifted[f]).unwrap()[4].1[2]).fold(f64::INFINITY, f64::min);
        assert!((lowest + 1.).abs() < 1e-9, "the lowest toe is the game's lowest");
        // The user's own bobbing stays: the height changes keep their size.
        let z = |f: usize| worlds(&s, &lifted[f]).unwrap()[1].1[2];
        assert!((z(1) - z(0) + 1.).abs() < 1e-9 && (z(2) - z(1) - 1.5).abs() < 1e-9);
        // A still pelvis position the game's value is kept for (the fixed-value
        // switch off) gives no lift at all.
        let mut flat = e.clone();
        for sample in &mut flat.samples {
            sample.bones.get_mut("pelvis").unwrap().position = Some([0., 0., 38.]);
        }
        let held = [(false, false), (false, true), (false, false), (false, false), (false, false)];
        let (_, report) = author(&s, &clip, &flat, &held, Options { floor_lift: true, keep_fixed_values: true, ..options() }).unwrap();
        assert_eq!(report.floor_lift_inches, None, "{report:?}");
    }

    #[test]
    fn the_arm_reaches_with_the_elbow_on_its_side_or_stretches_towards_the_target() {
        let (s, h) = ([0., 0., 0.], [10., 0., 0.]);
        let down = [5., 0., -75f64.sqrt()];
        // Reachable: lengths 10 and 10, target 12 away; the elbow stays below.
        let r = reach(s, down, h, [12., 0., 0.], [0., 3., 0.]);
        assert!(close(r.elbow, [6., 0., -8.]) && close(r.hand, [12., 0., 0.]) && r.miss < 1e-12, "{r:?}");
        // The same with the elbow above the line: it stays above.
        let r = reach(s, [5., 0., 75f64.sqrt()], h, [12., 0., 0.], [0., 3., 0.]);
        assert!(close(r.elbow, [6., 0., 8.]), "{r:?}");
        // Out of reach: fully stretched towards the target, 10 short.
        let r = reach(s, down, h, [30., 0., 0.], [0., 3., 0.]);
        assert!(close(r.elbow, [10., 0., 0.]) && close(r.hand, [20., 0., 0.]) && (r.miss - 10.).abs() < 1e-12, "{r:?}");
        // Too close for the bent arm (lengths 10 and 4, target 2 away).
        let r = reach(s, [10., 0., 0.], [10., 0., -4.], [2., 0., 0.], [0., 3., 0.]);
        assert!((r.miss - 4.).abs() < 1e-12 && close(r.hand, [6., 0., 0.]), "{r:?}");
        // A straight arm has no side: the fallback (the game's elbow) decides.
        let r = reach(s, [10., 0., 0.], [20., 0., 0.], [12., 0., 0.], [0., 3., 0.]);
        assert!(close(r.elbow, [6., 8., 0.]) && close(r.hand, [12., 0., 0.]), "{r:?}");
    }

    /// root, pelvis, spine, both arms (clavicle, upper, lower, hand), a gun
    /// bone under the right hand and a finger under the left. Arms straight
    /// out to the sides at rest.
    fn armed(gun_on_both: bool) -> Skeleton {
        let mut names: Vec<String> = ["root", "pelvis", "spine", "clav_L", "upper_L", "lower_L", "hand_L", "clav_R", "upper_R", "lower_R", "hand_R", "weapon_R", "finger_L"].map(String::from).to_vec();
        let mut parents = vec![-1, 0, 1, 2, 3, 4, 5, 2, 7, 8, 9, 10, 6];
        if gun_on_both {
            names.push("weapon_L".into());
            parents.push(6);
        }
        let n = names.len();
        Skeleton { id: "t".into(), names, parents, reference: vec![Local::IDENTITY; n] }
    }
    fn armed_pose(s: &Skeleton) -> Vec<Local> {
        let mut pose = vec![local([0.; 3]), local([0., 0., 40.]), local([0., 0., 10.]), local([0., 5., 0.]), local([0., 5., 0.]), local([0., 10., 0.]), local([0., 10., 0.]), local([0., -5., 0.]), local([0., -5., 0.]), local([0., -10., 0.]), local([0., -10., 0.]), local([2., 0., 0.]), local([0., 2., 0.])];
        pose.truncate(s.names.len());
        while pose.len() < s.names.len() {
            pose.push(local([2., 0., 0.]));
        }
        pose
    }
    fn arm_export(left_up: f64, right_turn: f64) -> Export {
        let id = SampleBone { y: [0., 0., 0., 1.], delta: [0., 0., 0., 1.], position: None };
        let lift = [(left_up / 2.).sin(), 0., 0., (left_up / 2.).cos()];
        let swing = turn(right_turn);
        let names = [("pelvis", None), ("spine", Some("pelvis")), ("clav_L", Some("spine")), ("upper_L", Some("clav_L")), ("lower_L", Some("upper_L")), ("hand_L", Some("lower_L")), ("clav_R", Some("spine")), ("upper_R", Some("clav_R")), ("lower_R", Some("upper_R")), ("hand_R", Some("lower_R"))];
        let samples = (0..2)
            .map(|k| {
                let mut bones = BTreeMap::new();
                for (name, _) in names {
                    let b = match name {
                        "pelvis" => SampleBone { position: Some([0., 0., 40.]), ..id },
                        "upper_L" => SampleBone { y: lift, delta: lift, position: None },
                        "upper_R" => SampleBone { y: swing, delta: swing, position: None },
                        _ => id,
                    };
                    bones.insert(name.to_string(), b);
                }
                Sample { time: k as f64, bones }
            })
            .collect();
        export("arms", names.iter().map(|(n, a)| row(n, *a)).collect(), samples)
    }

    #[test]
    fn the_off_arm_is_found_from_the_guns_bones_and_never_guessed() {
        let s = armed(false);
        let driven: Vec<bool> = (0..s.names.len()).map(|i| (1..=10).contains(&i)).collect();
        let arm = off_arm(&s, &driven).unwrap();
        assert_eq!((arm.gun, arm.hand, arm.elbow, arm.shoulder), (10, 6, 5, 4));
        assert_eq!(arm.hand_tree, vec![6, 12], "the finger turns with the hand");
        assert_eq!(off_arm(&armed(true), &[driven.clone(), vec![false]].concat()), Err(OFF_HAND_NO_GUN_HAND), "a gun in each hand");
        let mut no_left = driven.clone();
        no_left[5] = false;
        no_left[4] = false;
        assert_eq!(off_arm(&s, &no_left), Err(OFF_HAND_NO_ARM));
    }

    #[test]
    fn the_off_hand_takes_the_games_grip_on_the_made_gun_hand() {
        let s = armed(false);
        // The game holds the hands 60 apart (arms straight out): the grip is
        // the left hand 60 to the left of the right one.
        let frames: Vec<Vec<Local>> = (0..3).map(|_| armed_pose(&s)).collect();
        let clip = Clip { skeleton: "t".into(), duration: 1., additive: false, frames, root_motion: vec![], model_space_bones: 0 };
        let still = vec![(false, false); s.names.len()];
        // The action lifts the left arm 40 degrees; the right stays.
        let e = arm_export(40f64.to_radians(), 0.);
        let (plain, report) = author(&s, &clip, &e, &still, options()).unwrap();
        assert!(report.off_hand.is_none() && report.off_hand_skipped.is_none());
        let (held, report) = author(&s, &clip, &e, &still, Options { off_hand: true, ..options() }).unwrap();
        let grip = report.off_hand.clone().unwrap();
        assert_eq!((grip.gun_hand.as_str(), grip.off_hand.as_str(), grip.out_of_reach_frames), ("hand_R", "hand_L", 0), "{grip:?}");
        assert!(grip.game_grip_change_degrees < 1e-9 && grip.game_grip_change_inches < 1e-9);
        for f in 0..3 {
            let w = worlds(&s, &held[f]).unwrap();
            let g = worlds(&s, &clip.frames[f]).unwrap();
            let (mine, theirs) = (relative(w[10], w[6]), relative(g[10], g[6]));
            assert!(degrees(mine.0, theirs.0) < 1e-7 && length(sub(mine.1, theirs.1)) < 1e-7, "the game's grip");
            for i in [0, 1, 2, 3, 7, 8, 9, 10, 11] {
                assert_eq!(held[f][i], plain[f][i], "{} is outside the off arm", s.names[i]);
            }
            assert_eq!(held[f][12], plain[f][12], "the finger keeps its local values");
        }
        // The right arm swings forward 90 degrees: the grip is now out of reach.
        let e = arm_export(0., 90f64.to_radians());
        let (_, report) = author(&s, &clip, &e, &still, Options { off_hand: true, ..options() }).unwrap();
        let grip = report.off_hand.unwrap();
        assert_eq!(grip.out_of_reach_frames, 3);
        assert!(grip.worst_miss_inches > 1., "{grip:?}");
        // Two guns: asked for, not run, and said why.
        let two = armed(true);
        let frames: Vec<Vec<Local>> = (0..3).map(|_| armed_pose(&two)).collect();
        let clip = Clip { skeleton: "t".into(), duration: 1., additive: false, frames, root_motion: vec![], model_space_bones: 0 };
        let (_, report) = author(&two, &clip, &arm_export(0.3, 0.), &vec![(false, false); two.names.len()], Options { off_hand: true, ..options() }).unwrap();
        assert_eq!((report.off_hand, report.off_hand_skipped.as_deref()), (None, Some(OFF_HAND_NO_GUN_HAND)));
    }
}
