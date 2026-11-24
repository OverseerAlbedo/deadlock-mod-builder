//! Clips taken from another hero (`ResourceReplacement.taken`): the one place
//! that turns such a row into the clip's bytes, used by the build
//! (`motion::stage_with`), the install check, the Motion page's report and
//! preview. From the game's files only (no Blender, no prepared model):
//!
//! 1. The other hero's compiled clip is decoded, and both heroes' clip
//!    skeletons (`.vnmskel_c`, the skeleton each clip names) are read.
//! 2. The body bones both heroes have by name (`BODY`) are driven "as they
//!    look": each takes the other hero's bone's orientation relative to the
//!    root, with a per-bone correction for how the two heroes' bones are
//!    rolled about their own axis (for the hands: the frame against the
//!    palm). Fingers are never driven; with `arms: own` neither are the arm
//!    and hand chains (`ARMS`), whose tracks stay this hero's. The pelvis
//!    takes the other hero's movement from its rest, scaled by leg length.
//! 3. The result is the packet the author of clips made from an animation
//!    takes (`clip_author::Export`), authored through `clip_making::author`
//!    with the app's `SWITCHES` and checked with the unchanged
//!    `motion::validate_clip`. This hero's clip is the template: its length,
//!    frames, events, root motion and every bone the packet does not drive
//!    stay the game's.
//!
//! A pair of heroes whose bodies do not carry over (a hero without one of the
//! `BODY` bones, or a body bone that hangs from another body bone than in the
//! other hero) is refused from the bones, never from a hero's name.
use crate::{compiled, fail, made_clips, motion, sha, CancellationToken, Result};
use deadlock_core::text::casefold;
use deadlock_preparation::clip_author::{self, BoneRow, Export, Sample, SampleBone};
use deadlock_preparation::clip_decode::{normalize, product, Clip, Local, Skeleton};
use deadlock_preparation::clip_making::{self, MadeReport, Switches, SWITCHES};
use deadlock_project::customization::ResourceReplacement;
use deadlock_project::made_clip::MadeClip;
use deadlock_project::taken_clip::{TakenArms, TakenClip};
use deadlock_vpk::Vpk;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicUsize, Ordering};

/// The retarget's version (kept in the receipt; a change asks for a new build).
pub const POLICY: &str = "hero-clip-as-it-looks-v1";

/// The body bones a clip is carried on. Both heroes need every one of them.
pub const BODY: &[&str] = &[
    "pelvis", "spine_0", "spine_1", "spine_2", "spine_3", "neck_0", "head", "clavicle_L", "arm_upper_L", "arm_lower_L", "hand_L", "clavicle_R", "arm_upper_R", "arm_lower_R", "hand_R", "leg_upper_L", "leg_lower_L", "ankle_L", "ball_L",
    "leg_upper_R", "leg_lower_R", "ankle_R", "ball_R",
];
/// The arm and hand chains `arms: own` leaves to this hero's own motion.
pub const ARMS: &[&str] = &["clavicle_L", "arm_upper_L", "arm_lower_L", "hand_L", "clavicle_R", "arm_upper_R", "arm_lower_R", "hand_R"];

// Refusals (the page and troubleshooting compare them; TS `takenTypes.ts`).
/// This hero's clip is a layer: nothing can be taken onto it.
pub const TARGET_ADDITIVE: &str = "This clip cannot be taken from another hero. The game adds it on top of other clips (a lean or a layer, not a full pose), and only full poses can be taken. A compiled clip file can still replace it.";
/// This hero's clip holds a single pose, model-space bones or scaled bones.
pub const TARGET_NOT_FULL: &str = "This clip cannot be taken from another hero. The game stores it in a way the app does not write (a single pose, bones sampled in the model's space, or bones that change size). A compiled clip file can still replace it.";
/// The other hero's clip is a layer.
pub const SOURCE_ADDITIVE: &str = "That clip of the other hero cannot be used. The game adds it on top of other clips (a lean or a layer, not a full pose): choose one of that hero's full-pose clips.";
/// The other hero's clip samples bones in the model's space.
pub const SOURCE_MODEL_SPACE: &str = "That clip of the other hero cannot be used. The game samples some of its bones in the model's space, which the app does not read: choose another of that hero's clips.";
/// Followed by ": " and the hero's model path.
pub const NO_SOURCE_HERO: &str = "The hero this clip is taken from is not in the game any more. Choose another hero for the clip on the Motion page";
/// Followed by ": " and the clip's path.
pub const NO_SOURCE_CLIP: &str = "The other hero's clip this clip is taken from is not in the game any more. Choose another of that hero's clips on the Motion page";
/// A clip taken from the project's own hero.
pub const SAME_HERO: &str = "A clip is taken from another hero, not from this hero itself. Choose another hero for the clip on the Motion page.";
/// Followed by " (" the bones ")." (which hero lacks which bones, or which bone hangs differently).
pub const BODY_NOT_CARRIED: &str = "These two heroes do not share the body a clip is carried on";
/// Followed by ": " and the decoder's words.
pub const SOURCE_UNREADABLE: &str = "The other hero's clip could not be read";
/// Followed by " (from X s this clip needs Y s, the other hero's clip has Z s)".
pub const PART_TOO_LONG: &str = "The part of the other hero's clip that plays does not fit: it starts too late for this clip's length";
/// The install check, when the other hero's clip or a skeleton changed in the game since the build.
pub const GAME_CHANGED: &str = "The other hero's clip or a hero's skeleton changed in the game since this build. Rebuild before installing.";
/// The install check, when a taken row differs from the build.
pub const CHANGED_SINCE_BUILD: &str = "A clip taken from another hero changed since this build. Rebuild before installing.";

static OTHER_HERO_READS: AtomicUsize = AtomicUsize::new(0);
/// How many times another hero's files were read for a taken clip (tests:
/// a project without taken rows reads none).
pub fn other_hero_reads() -> usize {
    OTHER_HERO_READS.load(Ordering::SeqCst)
}
fn read_other(archive: &Vpk, path: &str) -> Result<Vec<u8>> {
    OTHER_HERO_READS.fetch_add(1, Ordering::SeqCst);
    crate::read(archive, path)
}

// ---------------------------------------------------------------- the retarget

type Q = [f64; 4];
type V = [f64; 3];

fn conj(q: Q) -> Q {
    [-q[0], -q[1], -q[2], q[3]]
}
fn rotate(q: Q, v: V) -> V {
    let p = product(product(q, [v[0], v[1], v[2], 0.]), conj(q));
    [p[0], p[1], p[2]]
}
fn mul(a: Q, b: Q) -> Q {
    normalize(product(a, b))
}
fn add(a: V, b: V) -> V {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn scale(a: V, s: f64) -> V {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn len(a: V) -> f64 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}
fn dist(a: V, b: V) -> f64 {
    len(sub(a, b))
}
fn unit(a: V) -> V {
    let l = len(a).max(1e-12);
    scale(a, 1. / l)
}
fn cross(a: V, b: V) -> V {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
/// The roll about the bone's own x axis that takes rest frame `a` onto rest
/// frame `b` once their x axes point the same way: a⁻¹ · swing(b x → a x) · b.
/// The identity when both heroes use the same frame for that bone.
fn frame_fix(a: Q, b: Q) -> Q {
    let x = [1., 0., 0.];
    mul(conj(a), mul(clip_author::swing(rotate(b, x), rotate(a, x)), b))
}
/// A rotation whose x axis is `x` and whose xz plane holds `z`.
fn frame(x: V, z: V) -> Q {
    let x = unit(x);
    let y = unit(cross(z, x));
    let z = cross(x, y);
    // Rotation matrix columns x, y, z to a quaternion.
    let m = [[x[0], y[0], z[0]], [x[1], y[1], z[1]], [x[2], y[2], z[2]]];
    let t = m[0][0] + m[1][1] + m[2][2];
    let q = if t > 0. {
        let s = (t + 1.).sqrt() * 2.;
        [(m[2][1] - m[1][2]) / s, (m[0][2] - m[2][0]) / s, (m[1][0] - m[0][1]) / s, s / 4.]
    } else if m[0][0] > m[1][1] && m[0][0] > m[2][2] {
        let s = (1. + m[0][0] - m[1][1] - m[2][2]).sqrt() * 2.;
        [s / 4., (m[0][1] + m[1][0]) / s, (m[0][2] + m[2][0]) / s, (m[2][1] - m[1][2]) / s]
    } else if m[1][1] > m[2][2] {
        let s = (1. + m[1][1] - m[0][0] - m[2][2]).sqrt() * 2.;
        [(m[0][1] + m[1][0]) / s, s / 4., (m[1][2] + m[2][1]) / s, (m[0][2] - m[2][0]) / s]
    } else {
        let s = (1. + m[2][2] - m[0][0] - m[1][1]).sqrt() * 2.;
        [(m[0][2] + m[2][0]) / s, (m[1][2] + m[2][1]) / s, s / 4., (m[1][0] - m[0][1]) / s]
    };
    normalize(q)
}

/// Model-space (rotation, position) of every bone; scale is ignored (the
/// reference poses and the clips used here hold scale 1).
fn worlds(skel: &Skeleton, frame: &[Local]) -> Vec<(Q, V)> {
    let mut out: Vec<(Q, V)> = Vec::with_capacity(frame.len());
    for (i, l) in frame.iter().enumerate() {
        out.push(if skel.parents[i] < 0 {
            (l.rotation, l.position)
        } else {
            let (pq, pp) = out[skel.parents[i] as usize];
            (mul(pq, l.rotation), add(pp, rotate(pq, l.position)))
        });
    }
    out
}
/// The same, relative to the root bone (`root_motion`) of that frame.
fn rooted(skel: &Skeleton, frame: &[Local]) -> Vec<(Q, V)> {
    let w = worlds(skel, frame);
    let (rq, rp) = w[0];
    w.iter().map(|&(q, p)| (mul(conj(rq), q), rotate(conj(rq), sub(p, rp)))).collect()
}

/// One hero's clip skeleton with its rest pose in model space.
#[derive(Debug, Clone)]
pub struct Body {
    pub skeleton: Skeleton,
    /// The skeleton file's digest (the receipt keeps it).
    pub skeleton_sha256: String,
    index: BTreeMap<String, usize>,
    rest: Vec<(Q, V)>,
}
impl Body {
    pub fn new(skeleton: Skeleton, skeleton_sha256: String) -> Self {
        let index = skeleton.names.iter().enumerate().map(|(i, n)| (n.clone(), i)).collect();
        let rest = worlds(&skeleton, &skeleton.reference);
        Self { skeleton, skeleton_sha256, index, rest }
    }
    pub fn at(&self, name: &str) -> Option<usize> {
        self.index.get(name).copied()
    }
    fn p(&self, name: &str) -> Option<V> {
        self.at(name).map(|i| self.rest[i].1)
    }
    /// Hip to knee to ankle, both legs averaged.
    pub fn leg(&self) -> f64 {
        let side = |s: &str| -> f64 {
            match (self.p(&format!("leg_upper_{s}")), self.p(&format!("leg_lower_{s}")), self.p(&format!("ankle_{s}"))) {
                (Some(a), Some(b), Some(c)) => dist(a, b) + dist(b, c),
                _ => f64::NAN,
            }
        };
        (side("L") + side("R")) / 2.
    }
    fn has_ancestor(&self, i: usize, name: &str) -> bool {
        let mut q = self.skeleton.parents[i];
        while q >= 0 {
            if self.skeleton.names[q as usize] == name {
                return true;
            }
            q = self.skeleton.parents[q as usize];
        }
        false
    }
}

/// The hand's palm frame in model space at rest (x: wrist to the middle
/// finger's base; z: from the pinky's base to the index finger's), when the
/// hero has those bones.
fn palm(h: &Body, side: &str) -> Option<Q> {
    let p = |n: &str| h.p(&format!("{n}_{side}"));
    let (w, m, i) = (p("hand")?, p("finger_middle_0")?, p("finger_index_0")?);
    let o = p("finger_pinky_0").or_else(|| p("finger_ring_0"))?;
    Some(frame(sub(m, w), sub(i, o)))
}
/// The correction for bone `name` from A's rest frame to B's: the palm frame
/// for hands (the palm faces where A's palm faces), else the roll about x.
fn bone_fix(a: &Body, b: &Body, name: &str) -> Q {
    let (ia, ib) = (a.at(name).expect("a shared bone"), b.at(name).expect("a shared bone"));
    if let Some(side) = name.strip_prefix("hand_") {
        if let (Some(fa), Some(fb)) = (palm(a, side), palm(b, side)) {
            // A's local offset to its palm, then back out of B's: G_B = W_A · O_A · O_B⁻¹.
            let oa = mul(conj(a.rest[ia].0), fa);
            let ob = mul(conj(b.rest[ib].0), fb);
            return mul(oa, conj(ob));
        }
    }
    frame_fix(a.rest[ia].0, b.rest[ib].0)
}

/// Why a clip of `source` cannot be carried onto `target` (the words after
/// `BODY_NOT_CARRIED`), or None. `names` are the two heroes' names for the words.
pub fn body_mismatch(source: &Body, target: &Body, names: (&str, &str)) -> Option<String> {
    let mut missing = vec![];
    for (body, name) in [(source, names.0), (target, names.1)] {
        let lacks: Vec<&str> = BODY.iter().copied().filter(|b| body.at(b).is_none()).collect();
        if !lacks.is_empty() {
            missing.push(format!("{name} has no {}", lacks.join(", ")));
        }
    }
    if !missing.is_empty() {
        return Some(missing.join("; "));
    }
    // Every body bone but the pelvis hangs from a body bone, the same one in both.
    for name in BODY.iter().copied().filter(|b| *b != "pelvis") {
        let (s, t) = (source.at(name).expect("checked"), target.at(name).expect("checked"));
        let anchor = |body: &Body, i: usize| {
            let mut p = body.skeleton.parents[i];
            while p >= 0 && !BODY.contains(&body.skeleton.names[p as usize].as_str()) {
                p = body.skeleton.parents[p as usize];
            }
            (p >= 0).then(|| body.skeleton.names[p as usize].clone())
        };
        let (a, b) = (anchor(source, s), anchor(target, t));
        if a != b || a.is_none() {
            let hang = |n: &Option<String>| n.clone().unwrap_or_else(|| "no body bone".into());
            return Some(format!("{name} hangs from {} on {} and from {} on {}", hang(&a), names.0, hang(&b), names.1));
        }
    }
    None
}

/// The driven bones, in the target's order, each with its nearest driven
/// ancestor in the target's skeleton (None: under the root) that is also an
/// ancestor in the source (by name). Fingers are never driven; with
/// `TakenArms::Own` neither are `ARMS`.
pub fn driven_bones(a: &Body, b: &Body, arms: TakenArms) -> Vec<(usize, Option<usize>)> {
    let mut set = BTreeSet::new();
    for (i, n) in b.skeleton.names.iter().enumerate() {
        if i > 0 && BODY.contains(&n.as_str()) && a.at(n).is_some() && !(arms == TakenArms::Own && ARMS.contains(&n.as_str())) {
            set.insert(i);
        }
    }
    let mut out = vec![];
    for &i in &set {
        let mut p = b.skeleton.parents[i];
        while p >= 0 && !set.contains(&(p as usize)) {
            p = b.skeleton.parents[p as usize];
        }
        let anchor = (p >= 0).then_some(p as usize);
        // The anchor must also be an ancestor in the source (by name).
        let anchor = anchor.filter(|&an| a.has_ancestor(a.at(&b.skeleton.names[i]).expect("shared"), &b.skeleton.names[an]));
        out.push((i, anchor));
    }
    out
}

/// The packet the existing author takes, made from hero A's decoded clip
/// instead of a Blender action ("as it looks", with the frame correction).
/// Samples are A's stored frames. Returns the packet and the size (B's leg
/// length over A's).
pub fn export(a: &Body, b: &Body, src: &Clip, arms: TakenArms, label: &str) -> std::result::Result<(Export, f64), String> {
    let size = b.leg() / a.leg();
    if !size.is_finite() {
        return Err("both heroes need two legs".into());
    }
    if src.frames.first().map(Vec::len) != Some(a.skeleton.names.len()) {
        return Err("the clip's bone tracks do not match its skeleton".into());
    }
    let bones = driven_bones(a, b, arms);
    let name = |i: usize| b.skeleton.names[i].clone();
    let rows: Vec<BoneRow> = bones
        .iter()
        .map(|&(i, anchor)| BoneRow { hero: name(i), source: name(i), anchor: anchor.map(name), keyed: true, bind_rotation: b.rest[i].0, shoulder_bridge: false })
        .collect();
    // The bone's frame in B against A's (a roll about its axis; the identity
    // for heroes that share the convention), and its index in A.
    let fixes: Vec<(usize, Q)> = bones.iter().map(|&(i, _)| (a.at(&b.skeleton.names[i]).expect("shared"), bone_fix(a, b, &b.skeleton.names[i]))).collect();
    let mut samples = vec![];
    let dt = src.frame_time();
    for (k, frame) in src.frames.iter().enumerate() {
        let w = rooted(&a.skeleton, frame);
        // Target world rotation per driven bone, in the target's order (anchors first).
        let mut g: BTreeMap<usize, Q> = BTreeMap::new();
        let mut out = BTreeMap::new();
        for (&(i, anchor), &(s, fix)) in bones.iter().zip(&fixes) {
            let target = mul(w[s].0, fix);
            g.insert(i, target);
            let delta = mul(target, conj(b.rest[i].0));
            let y = match anchor {
                Some(an) => mul(conj(mul(g[&an], conj(b.rest[an].0))), delta),
                None => delta,
            };
            let position = anchor.is_none().then(|| add(b.rest[i].1, scale(sub(w[s].1, a.rest[s].1), size)));
            out.insert(name(i), SampleBone { y, delta, position });
        }
        samples.push(Sample { time: k as f64 * dt, bones: out });
    }
    let feet: Vec<String> = ["ankle_L", "ankle_R"].iter().filter(|n| b.at(n).is_some()).map(|n| n.to_string()).collect();
    Ok((
        Export {
            policy: clip_author::POLICY.into(),
            action: label.into(),
            fps: if dt > 0. { 1. / dt } else { 30. },
            length: src.duration,
            bones: rows,
            samples,
            unmatched_hero_bones: vec![],
            mode: "as_it_looks".into(),
            keyed_source_bones: vec![],
            feet,
        },
        size,
    ))
}

/// The row the author takes for a taken clip: the whole other clip stretched,
/// or its part from the start at its own speed; never in place (the target
/// clip's own root motion carries the travel), every driven bone.
pub fn made_for(taken: &TakenClip, label: &str) -> MadeClip {
    let mut made = MadeClip::new(label.chars().take(deadlock_project::made_clip::MAX_ACTION_CHARS).collect::<String>());
    made.in_place = false;
    made.part_start_seconds = taken.part_start();
    made
}

// ---------------------------------------------------------------- the build

/// What a receipt keeps about a taken clip (absent for every other row).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TakenEntry {
    pub taken: TakenClip,
    pub switches: Switches,
    pub policy: String,
    pub source_clip_sha256: String,
    /// The other hero's clip skeleton and this hero's (paths and digests).
    pub source_skeleton: String,
    pub source_skeleton_sha256: String,
    pub target_skeleton: String,
    pub target_skeleton_sha256: String,
}

/// One taken clip: its bytes (checked with `validate_clip`), the game's clip,
/// the author's report and what the receipt keeps.
pub struct Taken {
    pub bytes: Vec<u8>,
    pub original: Vec<u8>,
    pub report: MadeReport,
    pub entry: TakenEntry,
    /// This hero's leg length over the other hero's.
    pub size: f64,
    /// Body bones both heroes have that follow the other hero.
    pub driven: Vec<String>,
    /// Body bones left to this hero's own motion by `arms: own`.
    pub own: Vec<String>,
}

fn skeleton_path(id: &str) -> String {
    if id.ends_with("_c") { id.to_string() } else { format!("{id}_c") }
}
/// The roster hero named by `model` (case-blind, `.vmdl` or `.vmdl_c`).
fn roster_hero(model: &str) -> Result<Option<crate::heroes::Hero>> {
    let wanted = casefold(&compiled(model)?);
    Ok(crate::heroes::roster()?.into_iter().find(|h| compiled(&h.path).map(|p| casefold(&p) == wanted).unwrap_or(false)))
}
/// A hero's name for the words (the roster's, else the model's file stem).
pub fn hero_name(model: &str) -> String {
    roster_hero(model).ok().flatten().map(|h| h.name).unwrap_or_else(|| model.rsplit('/').next().unwrap_or(model).trim_end_matches("_c").trim_end_matches(".vmdl").to_string())
}

/// The clip `target` of `target_model` taken from `taken` with `SWITCHES`.
pub fn author_row(archive: &Vpk, target_model: &str, target: &str, taken: &TakenClip, cancel: &CancellationToken) -> Result<Taken> {
    author_with(archive, target_model, target, taken, SWITCHES, cancel)
}

/// Checks the other hero and its clip: in the roster, not this hero, in the
/// game, one of that hero's clips (as the Motion page lists them), a full pose.
/// Returns the clip's bytes and decode.
fn source_clip(archive: &Vpk, target_model: &str, taken: &TakenClip, cancel: &CancellationToken) -> Result<(Vec<u8>, Clip)> {
    if casefold(&compiled(&taken.hero)?) == casefold(&compiled(target_model)?) {
        return Err(fail(SAME_HERO));
    }
    let hero = roster_hero(&taken.hero)?;
    let model = compiled(&taken.hero)?;
    if hero.is_none() || !archive.entries.contains_key(&casefold(&model)) {
        return Err(fail(format!("{NO_SOURCE_HERO}: {}", taken.hero)));
    }
    OTHER_HERO_READS.fetch_add(1, Ordering::SeqCst);
    let listed = motion::discover(archive, &taken.hero, cancel, |_| {})?;
    let wanted = casefold(&compiled(&taken.clip)?);
    let row = listed.iter().find(|r| casefold(&r.target) == wanted).ok_or_else(|| fail(format!("{NO_SOURCE_CLIP}: {}", taken.clip)))?;
    let raw = read_other(archive, &row.target)?;
    let clip = Clip::decode(&raw).map_err(|e| fail(format!("{SOURCE_UNREADABLE}: {e}")))?;
    if clip.additive {
        return Err(fail(SOURCE_ADDITIVE));
    }
    if clip.model_space_bones > 0 {
        return Err(fail(SOURCE_MODEL_SPACE));
    }
    Ok((raw, clip))
}

/// This hero's clip, refused with the taken-clip sentences.
fn target_clip(archive: &Vpk, target: &str) -> Result<Vec<u8>> {
    let original = crate::read(archive, &compiled(target)?)?;
    match clip_making::refusal(&original).map_err(fail)? {
        Some(clip_making::ADDITIVE) => Err(fail(TARGET_ADDITIVE)),
        Some(_) => Err(fail(TARGET_NOT_FULL)),
        None => Ok(original),
    }
}

/// Why this hero's clip `target` cannot be taken from another hero
/// (`TARGET_ADDITIVE` or `TARGET_NOT_FULL`, the sentences `author_row` refuses
/// it with), or None when it can: what the Motion page says before any row is
/// written.
pub fn target_refusal(archive: &Vpk, target: &str) -> Result<Option<&'static str>> {
    let original = crate::read(archive, &compiled(target)?)?;
    Ok(match clip_making::refusal(&original).map_err(fail)? {
        Some(clip_making::ADDITIVE) => Some(TARGET_ADDITIVE),
        Some(_) => Some(TARGET_NOT_FULL),
        None => None,
    })
}

/// `author_row` with other switches (tests and comparisons only).
pub fn author_with(archive: &Vpk, target_model: &str, target: &str, taken: &TakenClip, switches: Switches, cancel: &CancellationToken) -> Result<Taken> {
    taken.validate().map_err(fail)?;
    let original = target_clip(archive, target)?;
    let (source_raw, src) = source_clip(archive, target_model, taken, cancel)?;
    cancel.check()?;
    let source_skeleton = skeleton_path(&src.skeleton);
    let source_bytes = read_other(archive, &source_skeleton)?;
    let a = Body::new(Skeleton::decode(&source_bytes).map_err(|e| fail(format!("{SOURCE_UNREADABLE}: {e}")))?, sha(&source_bytes));
    let game = Clip::decode(&original).map_err(fail)?;
    if let Some(start) = taken.part_start() {
        if start + game.duration > src.duration + 1e-6 {
            return Err(fail(format!("{PART_TOO_LONG} (from {start:.3} s this clip needs {:.3} s, the other hero's clip has {:.3} s)", game.duration, src.duration)));
        }
    }
    let target_skeleton = skeleton_path(&game.skeleton);
    let target_bytes = crate::read(archive, &target_skeleton)?;
    let b = Body::new(Skeleton::decode(&target_bytes).map_err(fail)?, sha(&target_bytes));
    let names = (hero_name(&taken.hero), hero_name(target_model));
    if let Some(why) = body_mismatch(&a, &b, (&names.0, &names.1)) {
        return Err(fail(format!("{BODY_NOT_CARRIED} ({why}).")));
    }
    let stem = taken.clip.rsplit('/').next().unwrap_or(&taken.clip).trim_end_matches(".vnmclip_c");
    let label = format!("{} {stem}", names.0);
    let (packet, size) = export(&a, &b, &src, taken.arms, &label).map_err(|e| fail(format!("{SOURCE_UNREADABLE}: {e}")))?;
    let made = made_for(taken, &label);
    cancel.check()?;
    let (bytes, report) = clip_making::author(&original, &b.skeleton, &packet, &made, switches).map_err(fail)?;
    motion::validate_clip(&original, &bytes, cancel)?;
    let driven = packet.bones.iter().map(|r| r.hero.clone()).collect();
    let own = if taken.arms == TakenArms::Own { ARMS.iter().filter(|n| b.at(n).is_some() && a.at(n).is_some()).map(|n| n.to_string()).collect() } else { vec![] };
    let entry = TakenEntry {
        taken: taken.clone(),
        switches,
        policy: POLICY.into(),
        source_clip_sha256: sha(&source_raw),
        source_skeleton,
        source_skeleton_sha256: a.skeleton_sha256.clone(),
        target_skeleton,
        target_skeleton_sha256: b.skeleton_sha256.clone(),
    };
    Ok(Taken { bytes, original, report, entry, size, driven, own })
}

/// Whether the rows' taken clips are the ones a receipt's entries were built
/// from (rows matched by target, case-blind). Rows and entries without
/// `taken` always agree here.
pub fn same_rows(rows: &[ResourceReplacement], entries: &[motion::AnimationEntry]) -> bool {
    rows.iter().all(|row| {
        let target = casefold(row.target.as_deref().unwrap_or_default());
        let entry = entries.iter().find(|e| casefold(&e.target) == target || casefold(&e.target) == format!("{target}_c"));
        match (&row.taken, entry.and_then(|e| e.taken.as_ref())) {
            (None, None) => true,
            (Some(taken), Some(kept)) => *taken == kept.taken && kept.switches == SWITCHES && kept.policy == POLICY,
            _ => false,
        }
    })
}

/// The install check: the other hero's clip and both skeletons are the ones
/// the build read (only read when there are taken entries).
pub fn verify_entries(archive: &Vpk, entries: &[motion::AnimationEntry], cancel: &CancellationToken) -> Result<()> {
    for entry in entries {
        let Some(kept) = &entry.taken else { continue };
        cancel.check()?;
        let digest = |path: &str, other: bool| -> Option<String> {
            let raw = if other { read_other(archive, path) } else { crate::read(archive, path) };
            raw.ok().map(|r| sha(&r))
        };
        if kept.policy != POLICY
            || digest(&kept.taken.clip, true).as_deref() != Some(kept.source_clip_sha256.as_str())
            || digest(&kept.source_skeleton, true).as_deref() != Some(kept.source_skeleton_sha256.as_str())
            || digest(&kept.target_skeleton, false).as_deref() != Some(kept.target_skeleton_sha256.as_str())
        {
            return Err(fail(GAME_CHANGED));
        }
    }
    Ok(())
}

/// A short digest of the bytes (the preview's cache key).
pub fn digest(bytes: &[u8]) -> String {
    made_clips::digest(bytes)
}

// ---------------------------------------------------------------- for the page

/// A hero's clip skeleton as its model names it (the first `.vnmskel` of the
/// model's references): what the hero list compares, without reading clips.
pub fn model_body(archive: &Vpk, model: &str) -> Result<Body> {
    let path = compiled(model)?;
    let references = deadlock_vpk::resource_references(&crate::read(archive, &path)?).map_err(fail)?;
    let mut skeletons: Vec<String> = references.iter().filter(|s| s.ends_with(".vnmskel") || s.ends_with(".vnmskel_c")).map(|s| skeleton_path(s)).collect();
    skeletons.sort_by_key(|s| casefold(s));
    let first = skeletons.first().ok_or_else(|| fail("The hero's model names no animation skeleton."))?;
    let raw = crate::read(archive, first)?;
    Ok(Body::new(Skeleton::decode(&raw).map_err(fail)?, sha(&raw)))
}

/// One hero a clip can be taken from (the page's hero list).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceHero {
    /// The roster's model path (what a row's `taken.hero` holds).
    pub hero: String,
    pub name: String,
    /// Whether its body can be carried onto this hero.
    pub usable: bool,
    /// Why not (one sentence), when not usable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Every roster hero but `target_model`, each with whether its clips can be
/// carried onto it (decided from both skeletons' bones).
pub fn source_heroes(archive: &Vpk, target_model: &str, cancel: &CancellationToken) -> Result<Vec<SourceHero>> {
    let own = casefold(&compiled(target_model)?);
    let target_name = hero_name(target_model);
    let target = model_body(archive, target_model);
    let mut out = vec![];
    for hero in crate::heroes::roster()? {
        cancel.check()?;
        if casefold(&compiled(&hero.path)?) == own {
            continue;
        }
        let reason = if !archive.entries.contains_key(&casefold(&compiled(&hero.path)?)) {
            Some(format!("{NO_SOURCE_HERO}: {}", hero.path))
        } else {
            match (&target, &model_body(archive, &hero.path)) {
                (Err(e), _) | (_, Err(e)) => Some(format!("{SOURCE_UNREADABLE}: {e}")),
                (Ok(t), Ok(s)) => body_mismatch(&s, t, (&hero.name, &target_name)).map(|why| format!("{BODY_NOT_CARRIED} ({why}).")),
            }
        };
        out.push(SourceHero { hero: hero.path.clone(), name: hero.name.clone(), usable: reason.is_none(), reason });
    }
    Ok(out)
}

/// One clip of the other hero, for a chosen clip of this hero.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceClip {
    /// Its game path (what a row's `taken.clip` holds).
    pub clip: String,
    pub label: String,
    pub name: String,
    pub group: crate::clip_names::Group,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<crate::clip_names::Direction>,
    pub seconds: f64,
    pub frames: u32,
    /// 0: the same name; 1: the same group and direction; 2: the same group; 3: the rest.
    pub rank: u8,
    pub usable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// The other hero's clips for this hero's clip `target`, the same name first,
/// then the same group and direction, then the same group, then the rest (by
/// name within each). Copies the compiler derived (`x.vnmclip+…`) are left
/// out; layers are listed but not usable.
pub fn source_clips(target: &motion::CatalogClip, other: &[motion::CatalogClip]) -> Vec<SourceClip> {
    let mut out: Vec<SourceClip> = other
        .iter()
        .filter(|c| c.details.derived_from.is_none())
        .map(|c| {
            let rank = if c.row.label.eq_ignore_ascii_case(&target.row.label) {
                0
            } else if c.details.group == target.details.group && c.details.direction == target.details.direction {
                1
            } else if c.details.group == target.details.group {
                2
            } else {
                3
            };
            let reason = c.row.summary.additive.then(|| SOURCE_ADDITIVE.to_string());
            SourceClip {
                clip: c.row.target.clone(),
                label: c.row.label.clone(),
                name: c.details.name.clone(),
                group: c.details.group,
                direction: c.details.direction,
                seconds: c.row.summary.duration,
                frames: c.row.summary.frames,
                rank,
                usable: reason.is_none(),
                reason,
            }
        })
        .collect();
    out.sort_by(|a, b| (a.rank, casefold(&a.label)).cmp(&(b.rank, casefold(&b.label))));
    out
}

/// Clips that belong together: taking one of them from another hero usually
/// means taking all of them (each is still its own row).
pub const SETS: &[(&str, &[&str])] = &[
    ("dash", &["dash_ground", "dash_air_forward", "dash_air_back", "dash_air_left", "dash_air_right"]),
    ("slide", &["slide_start", "slide_loop", "slide_getup"]),
];
/// One clip of a set: this hero's clip and the other hero's clip of the same name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetMember {
    pub label: String,
    /// This hero's clip (its game path), when the hero has it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// The other hero's clip of the same name, when that hero has it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clip: Option<String>,
    /// Both clips are there and both are full poses.
    pub usable: bool,
}
/// The set this hero's clip `label` belongs to, with every member matched by name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClipSet {
    pub name: String,
    pub members: Vec<SetMember>,
}
/// The set `label` belongs to (case-blind), its members found in this hero's
/// clips (`own`) and the other hero's (`other`) by name; None when the clip
/// is in no set.
pub fn clip_set(label: &str, own: &[motion::CatalogClip], other: &[motion::CatalogClip]) -> Option<ClipSet> {
    let (name, labels) = SETS.iter().find(|(_, labels)| labels.iter().any(|l| l.eq_ignore_ascii_case(label)))?;
    let find = |clips: &[motion::CatalogClip], l: &str| clips.iter().find(|c| c.row.label.eq_ignore_ascii_case(l) && c.details.derived_from.is_none()).map(|c| (c.row.target.clone(), c.row.summary.additive));
    let members = labels
        .iter()
        .map(|l| {
            let (target, clip) = (find(own, l), find(other, l));
            let usable = matches!((&target, &clip), (Some((_, false)), Some((_, false))));
            SetMember { label: (*l).to_string(), target: target.map(|t| t.0), clip: clip.map(|c| c.0), usable }
        })
        .collect();
    Some(ClipSet { name: (*name).to_string(), members })
}

/// What a taken clip makes, for the page ("What the app makes").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TakenReport {
    /// The author's own report (the same one a made clip has).
    pub made: MadeReport,
    /// The other hero's name.
    pub source_name: String,
    /// Body bones that follow the other hero.
    pub driven: Vec<String>,
    /// Body bones left to this hero's own motion by `arms: own`.
    pub own: Vec<String>,
    /// This hero's bones the other hero has no bone of that name for (they
    /// keep this hero's own motion).
    pub not_matched: Vec<String>,
    /// How much faster (positive) or slower (negative) the other hero's clip
    /// plays, in percent (0 at its own speed).
    pub speed_percent: f64,
    /// The one lift of the whole clip that puts the feet on this hero's floor (inches).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub floor_lift_inches: Option<f64>,
    /// This hero's leg length over the other hero's.
    pub size: f64,
}
/// The report of `author_row`'s clip.
pub fn report(archive: &Vpk, taken: &TakenClip, made: &Taken) -> Result<TakenReport> {
    let source = Skeleton::decode(&crate::read(archive, &made.entry.source_skeleton)?).map_err(fail)?;
    let target = Skeleton::decode(&crate::read(archive, &made.entry.target_skeleton)?).map_err(fail)?;
    let theirs: BTreeSet<&str> = source.names.iter().map(String::as_str).collect();
    let not_matched = target.names.iter().skip(1).filter(|n| !theirs.contains(n.as_str())).cloned().collect();
    let r = &made.report;
    Ok(TakenReport {
        made: r.clone(),
        source_name: hero_name(&taken.hero),
        driven: made.driven.clone(),
        own: made.own.clone(),
        not_matched,
        speed_percent: if taken.part_start().is_some() || !(r.stretch > 0.) { 0. } else { (r.stretch - 1.) * 100. },
        floor_lift_inches: r.floor_lift_inches,
        size: made.size,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turn(axis: usize, angle: f64) -> Q {
        let mut q = [0., 0., 0., (angle / 2.).cos()];
        q[axis] = (angle / 2.).sin();
        q
    }
    /// A made-up humanoid: root, the body bones, three finger bases on the left
    /// hand for a palm, and a weapon under the root, without the `drop` bones.
    /// `roll` turns the left upper arm's rest frame about its own axis.
    fn humanoid(drop: &[&str], roll: f64) -> Skeleton {
        let mut names = vec!["root_motion".to_string()];
        let mut parents = vec![-1];
        let mut reference = vec![Local::IDENTITY];
        let add = |name: &str, parent: &str, position: V, rotation: Q, names: &mut Vec<String>, parents: &mut Vec<i32>, reference: &mut Vec<Local>| {
            let p = names.iter().position(|n| n == parent).map_or(0, |p| p as i32);
            names.push(name.into());
            parents.push(p);
            reference.push(Local { position, rotation, scale: 1. });
        };
        let chain: &[(&str, &str, V, Q)] = &[
            ("pelvis", "root_motion", [0., 0., 40.], [0., 0., 0., 1.]),
            ("spine_0", "pelvis", [5., 0., 0.], turn(1, -std::f64::consts::FRAC_PI_2)),
            ("spine_1", "spine_0", [5., 0., 0.], [0., 0., 0., 1.]),
            ("spine_2", "spine_1", [5., 0., 0.], [0., 0., 0., 1.]),
            ("spine_3", "spine_2", [5., 0., 0.], [0., 0., 0., 1.]),
            ("neck_0", "spine_3", [5., 0., 0.], [0., 0., 0., 1.]),
            ("head", "neck_0", [4., 0., 0.], [0., 0., 0., 1.]),
            ("clavicle_L", "spine_3", [3., 2., 0.], turn(2, 1.2)),
            ("arm_upper_L", "clavicle_L", [4., 0., 0.], turn(0, roll)),
            ("arm_lower_L", "arm_upper_L", [10., 0., 0.], [0., 0., 0., 1.]),
            ("hand_L", "arm_lower_L", [10., 0., 0.], [0., 0., 0., 1.]),
            ("finger_middle_0_L", "hand_L", [3., 0., 0.], [0., 0., 0., 1.]),
            ("finger_index_0_L", "hand_L", [3., 1., 0.], [0., 0., 0., 1.]),
            ("finger_pinky_0_L", "hand_L", [3., -1., 0.], [0., 0., 0., 1.]),
            ("clavicle_R", "spine_3", [3., -2., 0.], turn(2, -1.2)),
            ("arm_upper_R", "clavicle_R", [-4., 0., 0.], [0., 0., 0., 1.]),
            ("arm_lower_R", "arm_upper_R", [-10., 0., 0.], [0., 0., 0., 1.]),
            ("hand_R", "arm_lower_R", [-10., 0., 0.], [0., 0., 0., 1.]),
            ("leg_upper_L", "pelvis", [0., 4., -2.], turn(1, std::f64::consts::FRAC_PI_2)),
            ("leg_lower_L", "leg_upper_L", [18., 0., 0.], [0., 0., 0., 1.]),
            ("ankle_L", "leg_lower_L", [18., 0., 0.], [0., 0., 0., 1.]),
            ("ball_L", "ankle_L", [3., 0., 3.], [0., 0., 0., 1.]),
            ("leg_upper_R", "pelvis", [0., -4., -2.], turn(1, std::f64::consts::FRAC_PI_2)),
            ("leg_lower_R", "leg_upper_R", [-18., 0., 0.], [0., 0., 0., 1.]),
            ("ankle_R", "leg_lower_R", [-18., 0., 0.], [0., 0., 0., 1.]),
            ("ball_R", "ankle_R", [-3., 0., 3.], [0., 0., 0., 1.]),
            ("weapon", "root_motion", [10., 0., 40.], [0., 0., 0., 1.]),
        ];
        for (name, parent, position, rotation) in chain {
            if !drop.contains(name) {
                add(name, parent, *position, *rotation, &mut names, &mut parents, &mut reference);
            }
        }
        Skeleton { id: "models/test/test.vnmskel".into(), names, parents, reference }
    }
    fn body(drop: &[&str], roll: f64) -> Body {
        Body::new(humanoid(drop, roll), String::new())
    }
    /// A clip of `skel`'s rest pose with the left upper arm turned by `angle` about z.
    fn clip(skel: &Skeleton, angle: f64, frames: usize) -> Clip {
        let arm = skel.names.iter().position(|n| n == "arm_upper_L").unwrap();
        let frames = (0..frames)
            .map(|k| {
                let mut f = skel.reference.clone();
                f[arm].rotation = mul(f[arm].rotation, turn(2, angle * k as f64 / (frames.max(2) - 1) as f64));
                f
            })
            .collect();
        Clip { skeleton: skel.id.clone(), duration: 1., additive: false, frames, root_motion: vec![], model_space_bones: 0 }
    }

    #[test]
    fn the_body_bones_both_heroes_have_are_driven_and_fingers_never() {
        let (a, b) = (body(&[], 0.), body(&[], 0.));
        assert_eq!(body_mismatch(&a, &b, ("A", "B")), None);
        let driven: Vec<&str> = driven_bones(&a, &b, TakenArms::Source).iter().map(|&(i, _)| b.skeleton.names[i].as_str()).collect();
        let mut expected: Vec<&str> = BODY.to_vec();
        expected.sort_by_key(|n| b.at(n).unwrap());
        assert_eq!(driven, expected);
        assert!(!driven.iter().any(|n| n.starts_with("finger_") || *n == "weapon" || *n == "root_motion"));
        // Each bone's anchor is its nearest driven ancestor; the pelvis hangs from the root.
        for (i, anchor) in driven_bones(&a, &b, TakenArms::Source) {
            let name = &b.skeleton.names[i];
            assert_eq!(anchor.is_none(), name == "pelvis", "{name}");
        }
        // Own arms: the arm and hand chains are not driven, everything else is.
        let own: Vec<&str> = driven_bones(&a, &b, TakenArms::Own).iter().map(|&(i, _)| b.skeleton.names[i].as_str()).collect();
        assert_eq!(own.len(), BODY.len() - ARMS.len());
        assert!(own.iter().all(|n| !ARMS.contains(n)));
    }

    #[test]
    fn heroes_without_the_body_are_refused_from_their_bones() {
        let full = body(&[], 0.);
        let short_spine = body(&["spine_1", "spine_2"], 0.);
        assert_eq!(body_mismatch(&full, &short_spine, ("Haze", "Other")).unwrap(), "Other has no spine_1, spine_2");
        let both = body_mismatch(&body(&["head"], 0.), &body(&["ball_L"], 0.), ("A", "B")).unwrap();
        assert_eq!(both, "A has no head; B has no ball_L");
        // A body bone that hangs from another body bone than in the other hero.
        let mut moved = humanoid(&[], 0.);
        let neck = moved.names.iter().position(|n| n == "neck_0").unwrap();
        moved.parents[neck] = moved.names.iter().position(|n| n == "spine_1").unwrap() as i32;
        let reason = body_mismatch(&full, &Body::new(moved, String::new()), ("A", "B")).unwrap();
        assert_eq!(reason, "neck_0 hangs from spine_3 on A and from spine_1 on B");
    }

    #[test]
    fn as_it_looks_copies_the_pose_and_corrects_the_roll_of_a_bone() {
        // B's left upper arm is rolled half a turn about its own axis at rest: the
        // made arm still points and turns where A's does, rolled as B's own.
        let (a, b) = (body(&[], 0.), body(&[], std::f64::consts::PI));
        let src = clip(&a.skeleton, 0.8, 5);
        let (export, size) = export(&a, &b, &src, TakenArms::Source, "A clip").unwrap();
        assert!((size - 1.).abs() < 1e-12);
        assert_eq!(export.samples.len(), 5);
        assert_eq!(export.mode, "as_it_looks");
        let arm_a = a.at("arm_upper_L").unwrap();
        let arm_b = b.at("arm_upper_L").unwrap();
        for (k, sample) in export.samples.iter().enumerate() {
            let w = rooted(&a.skeleton, &src.frames[k]);
            let made = sample.bones["arm_upper_L"].delta;
            // The made world rotation is delta · rest(B); its x axis is A's.
            let world = mul(made, b.rest[arm_b].0);
            let (xa, xb) = (rotate(w[arm_a].0, [1., 0., 0.]), rotate(world, [1., 0., 0.]));
            assert!(dist(xa, xb) < 1e-9, "frame {k}");
            // And B's own rest roll is kept: y axes are opposite.
            let (ya, yb) = (rotate(w[arm_a].0, [0., 1., 0.]), rotate(world, [0., 1., 0.]));
            assert!(dist(ya, scale(yb, -1.)) < 1e-9, "frame {k}");
        }
        // Only the pelvis carries a position.
        assert!(export.samples[0].bones.iter().all(|(n, s)| s.position.is_some() == (n == "pelvis")));
        // With own arms the arm is not in the packet.
        let (own, _) = super::export(&a, &b, &src, TakenArms::Own, "A clip").unwrap();
        assert!(!own.bones.iter().any(|r| ARMS.contains(&r.hero.as_str())));
        assert!(!own.samples[0].bones.contains_key("arm_upper_L"));
    }

    fn listed(label: &str, group: crate::clip_names::Group, direction: Option<crate::clip_names::Direction>, additive: bool) -> motion::CatalogClip {
        motion::CatalogClip {
            row: motion::AnimationRow {
                target: format!("models/heroes/a/clips/{label}.vnmclip_c"),
                label: label.into(),
                summary: motion::Summary { skeleton: "s".into(), frames: 31, duration: 1., tracks: 3, events: 0, additive },
            },
            details: motion::ClipDetails { name: label.into(), group, direction, graph_named: true, derived_from: None, ends_where_starts: false },
        }
    }

    #[test]
    fn the_other_heros_clips_come_same_name_first_then_group_and_direction() {
        use crate::clip_names::{Direction::*, Group::*};
        let target = listed("dash_ground", Movement, Some(Forward), false);
        let mut derived = listed("dash_ground.vnmclip+non_additive", Movement, Some(Forward), false);
        derived.details.derived_from = Some("x".into());
        let other = vec![
            listed("zz_emote", Other, None, false),
            listed("run_back", Movement, Some(Backward), false),
            listed("weapon_run_n", Movement, Some(Forward), false),
            listed("lean_forward", Movement, Some(Forward), true),
            listed("Dash_Ground", Movement, None, false),
            derived,
        ];
        let clips = source_clips(&target, &other);
        let order: Vec<(&str, u8, bool)> = clips.iter().map(|c| (c.label.as_str(), c.rank, c.usable)).collect();
        assert_eq!(order, [("Dash_Ground", 0, true), ("lean_forward", 1, false), ("weapon_run_n", 1, true), ("run_back", 2, true), ("zz_emote", 3, true)]);
        assert_eq!(clips[1].reason.as_deref(), Some(SOURCE_ADDITIVE));
        assert_eq!((clips[0].seconds, clips[0].frames), (1., 31));
    }

    #[test]
    fn a_dash_is_a_set_of_five_matched_by_name() {
        use crate::clip_names::Group::*;
        let own: Vec<_> = ["dash_ground", "dash_air_forward", "dash_air_back", "dash_air_left", "dash_air_right", "jump"].iter().map(|l| listed(l, Movement, None, false)).collect();
        let mut other: Vec<_> = ["dash_ground", "dash_air_forward", "dash_air_back", "dash_air_left"].iter().map(|l| listed(l, Movement, None, false)).collect();
        other[3].row.summary.additive = true;
        let set = clip_set("DASH_AIR_BACK", &own, &other).unwrap();
        assert_eq!(set.name, "dash");
        let shown: Vec<(&str, bool, bool, bool)> = set.members.iter().map(|m| (m.label.as_str(), m.target.is_some(), m.clip.is_some(), m.usable)).collect();
        assert_eq!(shown, [("dash_ground", true, true, true), ("dash_air_forward", true, true, true), ("dash_air_back", true, true, true), ("dash_air_left", true, true, false), ("dash_air_right", true, false, false)]);
        assert!(clip_set("jump", &own, &other).is_none());
        assert_eq!(clip_set("slide_loop", &own, &other).unwrap().members.len(), 3);
    }

    #[test]
    fn a_taken_clip_maps_onto_the_made_clip_options() {
        let mut taken = TakenClip::new("models/heroes/a/a.vmdl", "models/heroes/a/clips/run.vnmclip_c");
        let made = made_for(&taken, "Other run");
        assert_eq!((made.part_start_seconds, made.in_place, made.mode, made.bones), (None, false, deadlock_project::made_clip::ClipMode::AsItLooks, deadlock_project::made_clip::ClipBones::All));
        taken.timing = deadlock_project::taken_clip::TakenTiming::OwnSpeed;
        assert_eq!(made_for(&taken, "x").part_start_seconds, Some(0.));
        taken.start_seconds = Some(0.4);
        assert_eq!(made_for(&taken, "x").part_start_seconds, Some(0.4));
        assert_eq!(made_for(&taken, &"y".repeat(400)).action.chars().count(), 256);
    }
}
