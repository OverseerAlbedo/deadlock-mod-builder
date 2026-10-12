//! Gun customization: fit a gun taken from the character source onto the
//! hero's own gun (bundled in the hero model) or onto a separate weapon model.
//!
//! Both guns are measured the same way - barrel axis, muzzle end, grip and the
//! side the grip hangs from - and the source gun is rotated, uniformly scaled
//! and moved so that its grip and barrel line up with the original. The
//! original gun's skin weights are transferred by proximity, so the new gun
//! follows the same weapon bones (recoil, reload parts) and keeps the original
//! gun's size and muzzle position. Hero bind transforms are never modified.
use crate::{algorithms::Bone, invalid, Result};
use deadlock_skeleton::transforms::PreparedBone;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub const POLICY: &str = "weapon-fit-v1";
const MAX_REFERENCE_POINTS: usize = 6_000;

pub type Vec3 = [f64; 3];

/// A possible gun found in the source by the Blender inspection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Candidate {
    pub id: String,
    pub objects: Vec<String>,
    pub vertices: u64,
    pub bounds: [Vec3; 2],
    pub sample: Vec<Vec3>,
    #[serde(default)]
    pub attach_bone: Option<String>,
    #[serde(default)]
    pub attach_kind: String,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub elongation: f64,
    /// The gun comes from the Weapon tab's gun file, not the character file.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub gun_file: bool,
    /// Deform bones the gun objects are weighted to (its own bones and charms).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bones: Vec<String>,
}

/// Marks the bones of each offered gun candidate (its attach bone and the bones
/// its objects are weighted to) with the candidate id under `weapon`, so moving
/// parts can tell a charm hanging from the gun (see secondary_bones).
pub fn mark_gun_bones(bones: &mut [Bone], candidates: &[Candidate]) {
    for candidate in candidates {
        for bone in bones.iter_mut().filter(|b| candidate.attach_bone.as_ref() == Some(&b.name) || candidate.bones.contains(&b.name)) {
            let marks = bone.details.entry("weapon".into()).or_insert_with(|| serde_json::json!([]));
            if let Some(list) = marks.as_array_mut().filter(|l| !l.iter().any(|v| v == &candidate.id)) {
                list.push(candidate.id.clone().into());
            }
        }
    }
}

/// The marks of `mark_gun_bones`, sent to Blender with the conversion.
pub fn gun_marks(bones: &[Bone]) -> BTreeMap<String, Vec<String>> {
    bones
        .iter()
        .filter_map(|b| {
            let ids: Vec<String> = b.details.get("weapon")?.as_array()?.iter().filter_map(|v| v.as_str().map(str::to_string)).collect();
            (!ids.is_empty()).then(|| (b.name.clone(), ids))
        })
        .collect()
}

/// Blender request to add the gun file's meshes to the scene (inspection and
/// conversion send the same one).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GunFile {
    /// Staged copy of the gun file in the preparation workspace.
    pub source: String,
    /// Character bone that holds the gun (the right hand), when the rig has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hand: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Attachment {
    pub name: String,
    pub position: Vec3,
}

/// The hero's original gun, measured from the decompiled original model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Reference {
    pub policy: String,
    /// "meshes" when original gun geometry exists, "skeleton" for bones only.
    pub source: String,
    /// The project's model is itself the weapon (separate weapon model).
    #[serde(default)]
    pub separate_model: bool,
    /// Original render meshes that show the gun, including lower LODs.
    #[serde(default)]
    pub meshes: Vec<String>,
    /// Base-LOD gun meshes used for measuring.
    #[serde(default)]
    pub primary: Vec<String>,
    /// Every original render mesh of the model (for mesh mapping).
    #[serde(default)]
    pub model_meshes: Vec<String>,
    #[serde(default)]
    pub points: Vec<Vec3>,
    #[serde(default)]
    pub weights: Vec<Vec<(String, f64)>>,
    #[serde(default)]
    pub bones: Vec<String>,
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    #[serde(default)]
    pub notes: Vec<String>,
}

fn one() -> f64 {
    1.
}
/// User choices stored as `options.weapon`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Choice {
    #[serde(default)]
    pub enabled: bool,
    /// "replace" (fit to the hero gun), "remove" (leave it out) or "keep" (follows the hand bones).
    #[serde(default)]
    pub action: String,
    #[serde(default)]
    pub objects: Vec<String>,
    /// "match" the original gun's size, or "keep_size" relative to the body.
    #[serde(default)]
    pub fit: String,
    #[serde(default)]
    pub flip: bool,
    /// Degrees around the barrel.
    #[serde(default)]
    pub roll: f64,
    #[serde(default = "one")]
    pub scale: f64,
    /// Inches along the fitted gun's forward, up and right directions.
    #[serde(default)]
    pub offset: Vec3,
    /// "auto" (copy the original gun's weights) or "rigid" (one bone).
    #[serde(default)]
    pub skin: String,
    #[serde(default)]
    pub bone: String,
}
impl Default for Choice {
    fn default() -> Self {
        Self {
            enabled: false,
            action: String::new(),
            objects: vec![],
            fit: "match".into(),
            flip: false,
            roll: 0.,
            scale: 1.,
            offset: [0.; 3],
            skin: "auto".into(),
            bone: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Skin {
    Rigid { bone: String },
    Transfer { points: Vec<Vec3>, weights: Vec<Vec<(String, f64)>> },
}

/// Sent to Blender with the preparation job.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Plan {
    pub policy: String,
    pub objects: Vec<String>,
    pub action: String,
    pub matrix: [[f64; 4]; 4],
    pub skin: Skin,
    pub mesh_prefix: String,
    /// Original render meshes the fitted gun replaces.
    pub replace: Vec<String>,
    /// The hero bone the fitted gun mainly follows; charms hang from it.
    #[serde(default)]
    pub anchor: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Frame {
    pub grip: Vec3,
    pub muzzle: Vec3,
    pub forward: Vec3,
    pub up: Vec3,
    pub length: f64,
    pub confidence: String,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Fit {
    pub policy: String,
    pub source: Frame,
    pub target: Frame,
    pub matrix: [[f64; 4]; 4],
    pub scale: f64,
    pub muzzle_offset_mm: f64,
    pub skin: String,
    pub skin_bones: Vec<String>,
    /// The bone with the most of the gun's skin weight.
    #[serde(default)]
    pub anchor: String,
    pub notes: Vec<String>,
}

/// Lightweight data for the fit preview: both guns in hero model space.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Preview {
    pub fit: Fit,
    pub fitted: Vec<Vec3>,
    pub reference: Vec<Vec3>,
}

// ---------------------------------------------------------------- vector math
fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn add(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn mul(a: Vec3, s: f64) -> Vec3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn dot(a: Vec3, b: Vec3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn norm(a: Vec3) -> f64 {
    dot(a, a).sqrt()
}
fn unit(a: Vec3) -> Option<Vec3> {
    let n = norm(a);
    (n.is_finite() && n > 1e-12).then(|| mul(a, 1. / n))
}
fn perp(a: Vec3, axis: Vec3) -> Vec3 {
    sub(a, mul(axis, dot(a, axis)))
}
fn mean(points: &[Vec3]) -> Vec3 {
    let n = points.len().max(1) as f64;
    let s = points.iter().fold([0.; 3], |acc, p| add(acc, *p));
    mul(s, 1. / n)
}
fn rotate(v: Vec3, axis: Vec3, degrees: f64) -> Vec3 {
    let (s, c) = degrees.to_radians().sin_cos();
    add(add(mul(v, c), mul(cross(axis, v), s)), mul(axis, dot(axis, v) * (1. - c)))
}

/// Eigen decomposition of a symmetric 3x3 matrix (Jacobi), sorted descending.
fn eigen(mut a: [[f64; 3]; 3]) -> ([f64; 3], [Vec3; 3]) {
    let mut v = [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]];
    for _ in 0..50 {
        let (mut p, mut q, mut largest) = (0, 1, 0.);
        for i in 0..3 {
            for j in i + 1..3 {
                if a[i][j].abs() > largest {
                    largest = a[i][j].abs();
                    p = i;
                    q = j;
                }
            }
        }
        if largest < 1e-15 {
            break;
        }
        let theta = (a[q][q] - a[p][p]) / (2. * a[p][q]);
        let t = theta.signum() / (theta.abs() + (theta * theta + 1.).sqrt());
        let t = if theta == 0. { 1. } else { t };
        let c = 1. / (t * t + 1.).sqrt();
        let s = t * c;
        let mut next = a;
        for k in 0..3 {
            next[p][k] = c * a[p][k] - s * a[q][k];
            next[q][k] = s * a[p][k] + c * a[q][k];
        }
        let copy = next;
        for k in 0..3 {
            next[k][p] = c * copy[k][p] - s * copy[k][q];
            next[k][q] = s * copy[k][p] + c * copy[k][q];
        }
        a = next;
        for row in &mut v {
            let (x, y) = (row[p], row[q]);
            row[p] = c * x - s * y;
            row[q] = s * x + c * y;
        }
    }
    let mut pairs: Vec<(f64, Vec3)> = (0..3).map(|i| (a[i][i], [v[0][i], v[1][i], v[2][i]])).collect();
    pairs.sort_by(|x, y| y.0.total_cmp(&x.0));
    ([pairs[0].0, pairs[1].0, pairs[2].0], [pairs[0].1, pairs[1].1, pairs[2].1])
}

fn principal(points: &[Vec3]) -> (Vec3, [f64; 3], [Vec3; 3]) {
    let c = mean(points);
    let mut m = [[0.; 3]; 3];
    for p in points {
        let d = sub(*p, c);
        for i in 0..3 {
            for j in 0..3 {
                m[i][j] += d[i] * d[j];
            }
        }
    }
    let n = points.len().max(1) as f64;
    for row in &mut m {
        for x in row.iter_mut() {
            *x /= n;
        }
    }
    let (values, vectors) = eigen(m);
    (c, values, vectors)
}

/// Measure a gun from its points. Hints come from the hand holding it and
/// from muzzle bones/attachments when they exist.
pub fn measure(points: &[Vec3], grip_hint: Option<Vec3>, muzzle_hint: Option<Vec3>) -> Result<Frame> {
    if points.len() < 4 || points.iter().flatten().any(|v| !v.is_finite()) {
        return Err(invalid("The gun has too little valid geometry to measure."));
    }
    let mut notes = vec![];
    let (centre, values, axes) = principal(points);
    let mut forward = axes[0];
    let t = |p: &Vec3, f: Vec3| dot(sub(*p, centre), f);
    let (lo, hi) = points.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), p| {
        let x = t(p, forward);
        (a.min(x), b.max(x))
    });
    let length = hi - lo;
    if !(length > 1e-9) {
        return Err(invalid("The gun has no length to measure."));
    }
    let mut confidence = "high";
    if values[1] > values[0] * 0.7 {
        notes.push("The gun is nearly as wide as it is long; check the barrel direction.".into());
        confidence = "low";
    }
    // Which end is the muzzle?
    let mut decided = false;
    if let Some(m) = muzzle_hint {
        let x = t(&m, forward);
        if x.abs() > length * 0.1 {
            if x < 0. {
                forward = mul(forward, -1.);
            }
            decided = true;
        }
    }
    if !decided {
        if let Some(g) = grip_hint {
            let x = t(&g, forward);
            if (x - lo - (hi - x)).abs() > length * 0.04 {
                if hi - x < x - lo {
                    forward = mul(forward, -1.);
                }
                decided = true;
            }
        }
    }
    if !decided {
        // The thinner end is the muzzle.
        let radius = |end: f64| {
            let slab: Vec<Vec3> = points.iter().copied().filter(|p| (t(p, forward) - end).abs() < length * 0.12).collect();
            if slab.is_empty() {
                return f64::INFINITY;
            }
            let c = mean(&slab);
            slab.iter().map(|p| norm(perp(sub(*p, c), forward))).sum::<f64>() / slab.len() as f64
        };
        if radius(lo) < radius(hi) {
            forward = mul(forward, -1.);
        }
        notes.push("Muzzle end was judged from the gun's shape (thinner end).".into());
        if confidence == "high" {
            confidence = "medium";
        }
    }
    // The whole-gun principal axis leans toward a heavy grip or stock. Refine
    // it with the front part (barrel/slide), which is the true line of fire.
    for _ in 0..3 {
        let top = points.iter().map(|p| t(p, forward)).fold(f64::NEG_INFINITY, f64::max);
        let front: Vec<Vec3> = points.iter().copied().filter(|p| t(p, forward) >= top - length * 0.45).collect();
        if front.len() < 4 {
            break;
        }
        let (_, front_values, front_axes) = principal(&front);
        if front_values[0] < front_values[1] * 2. {
            break; // not elongated enough to define a barrel
        }
        let mut refined = front_axes[0];
        if dot(refined, forward) < 0. {
            refined = mul(refined, -1.);
        }
        if dot(refined, forward) < 0.8 {
            break;
        }
        forward = refined;
    }
    let along = |p: &Vec3| t(p, forward);
    let (front_lo, front_hi) = points.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), p| (a.min(along(p)), b.max(along(p))));
    let length = front_hi - front_lo;
    let cap: Vec<Vec3> = points.iter().copied().filter(|p| along(p) >= front_hi - length * 0.04).collect();
    let muzzle = mean(&cap);
    let barrel: Vec<Vec3> = points.iter().copied().filter(|p| along(p) >= front_hi - length * 0.35).collect();
    let barrel_point = mean(&barrel);
    // Which way is up? The grip hangs below the barrel line.
    let mut up = None;
    if let Some(g) = grip_hint {
        let d = perp(sub(g, barrel_point), forward);
        if norm(d) > length * 0.04 {
            up = unit(mul(d, -1.));
        }
    }
    if up.is_none() {
        let offsets: Vec<Vec3> = points.iter().map(|p| perp(sub(*p, barrel_point), forward)).collect();
        let m = mean(&offsets);
        let rms = (offsets.iter().map(|d| dot(*d, *d)).sum::<f64>() / offsets.len() as f64).sqrt();
        if norm(m) > rms * 0.15 {
            up = unit(mul(m, -1.));
        }
    }
    let up = match up {
        Some(u) => u,
        None => {
            notes.push("The gun looks symmetric around its barrel; its roll is a guess - check it in the preview.".into());
            confidence = "low";
            let candidate = perp([0., 0., 1.], forward);
            unit(candidate).or_else(|| unit(perp(axes[1], forward))).unwrap_or(axes[1])
        }
    };
    // The grip: the hand position when known, otherwise the handle below the barrel.
    let grip = match grip_hint {
        Some(g) => g,
        None => {
            let drop: Vec<(f64, Vec3)> = points.iter().map(|p| (-dot(perp(sub(*p, barrel_point), forward), up), *p)).collect();
            let deepest = drop.iter().map(|d| d.0).fold(f64::NEG_INFINITY, f64::max);
            let handle: Vec<Vec3> = drop.iter().filter(|d| d.0 >= deepest * 0.5).map(|d| d.1).collect();
            if deepest > length * 0.08 && !handle.is_empty() {
                mean(&handle)
            } else {
                // No handle (rifle-like stock line): use the rear third.
                let rear: Vec<Vec3> = points.iter().copied().filter(|p| along(p) <= front_lo + length * 0.35).collect();
                mean(&rear)
            }
        }
    };
    let _ = centre;
    Ok(Frame { grip, muzzle, forward, up, length, confidence: confidence.into(), notes })
}

fn right(frame: &Frame) -> Vec3 {
    cross(frame.forward, frame.up)
}

fn name_has(name: &str, words: &[&str]) -> bool {
    let n = name.to_lowercase();
    words.iter().any(|w| n.contains(w))
}

/// Bones that carry a hero's gun: subtrees of weapon/gun bones (not IK aims).
pub fn weapon_bones(target: &[Bone]) -> BTreeSet<String> {
    let roots: Vec<&Bone> = target
        .iter()
        .filter(|b| name_has(&b.name, &["weapon", "gun"]) && !name_has(&b.name, &["target", "aim", "intent", "ik"]))
        .collect();
    let mut out = BTreeSet::new();
    for root in roots {
        let mut pending = vec![root.name.clone()];
        while let Some(name) = pending.pop() {
            if out.insert(name.clone()) {
                pending.extend(target.iter().filter(|b| b.parent.as_deref() == Some(name.as_str())).map(|b| b.name.clone()));
            }
        }
    }
    out
}

/// The bone a gun follows when no original gun geometry tells us better.
pub fn weapon_root(target: &[Bone]) -> Option<String> {
    let exact = target.iter().find(|b| ["weapon", "weapon_bone", "gun", "weapon_root"].contains(&b.name.to_lowercase().as_str()));
    if let Some(b) = exact {
        return Some(b.name.clone());
    }
    target
        .iter()
        .filter(|b| name_has(&b.name, &["weapon", "gun"]) && !name_has(&b.name, &["target", "aim", "intent", "ik", "pivot", "hand", "tip", "end"]))
        .map(|b| b.name.clone())
        .next()
}

fn muzzle_bone(target: &[Bone]) -> Option<&Bone> {
    target
        .iter()
        .find(|b| name_has(&b.name, &["muzzle"]))
        .or_else(|| target.iter().find(|b| name_has(&b.name, &["weapontip", "barrel", "fire"])))
}

fn grip_bone<'a>(target: &'a [Bone], weapon: &BTreeSet<String>) -> Option<&'a Bone> {
    target
        .iter()
        .filter(|b| weapon.contains(&b.name) && name_has(&b.name, &["hand", "grip"]) && !name_has(&b.name, &["_l", "left"]))
        .find(|b| norm(b.position) > 1e-6 || !b.position.iter().all(|v| *v == 0.))
}

fn inside(point: Vec3, points: &[Vec3], margin: f64) -> bool {
    let (lo, hi) = points.iter().fold(([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]), |(lo, hi), p| {
        ([lo[0].min(p[0]), lo[1].min(p[1]), lo[2].min(p[2])], [hi[0].max(p[0]), hi[1].max(p[1]), hi[2].max(p[2])])
    });
    (0..3).all(|k| point[k] >= lo[k] - margin && point[k] <= hi[k] + margin)
}

/// Measure the hero's original gun (or estimate it from weapon bones).
pub fn target_frame(reference: &Reference, target: &[Bone]) -> Result<Frame> {
    let weapon = weapon_bones(target);
    if reference.source == "meshes" && reference.points.len() >= 4 {
        let (_, _, axes) = principal(&reference.points);
        let length_hint = {
            let ts: Vec<f64> = reference.points.iter().map(|p| dot(*p, axes[0])).collect();
            ts.iter().cloned().fold(f64::NEG_INFINITY, f64::max) - ts.iter().cloned().fold(f64::INFINITY, f64::min)
        };
        let margin = length_hint * 0.2;
        let muzzle = reference
            .attachments
            .iter()
            .find(|a| name_has(&a.name, &["muzzle", "barrel", "tip"]))
            .map(|a| a.position)
            .or_else(|| muzzle_bone(target).map(|b| b.position))
            .filter(|p| inside(*p, &reference.points, margin));
        let grip = grip_bone(target, &weapon).map(|b| b.position).filter(|p| inside(*p, &reference.points, margin));
        let mut frame = measure(&reference.points, grip, muzzle)?;
        frame.notes.insert(0, "Measured from the hero's original gun meshes.".into());
        return Ok(frame);
    }
    // Skeleton only: weapon root/grip and muzzle bones give position and barrel.
    let root = weapon_root(target).ok_or_else(|| {
        invalid("This hero's skeleton has no weapon bone to fit a gun to. Choose 'Follow the hand' or 'Leave it out' for the gun.")
    })?;
    let root_bone = target.iter().find(|b| b.name == root).unwrap();
    let grip = grip_bone(target, &weapon).unwrap_or(root_bone);
    let muzzle = muzzle_bone(target).ok_or_else(|| invalid("This hero has a weapon bone but no muzzle bone; the gun direction cannot be measured. Choose 'Follow the hand'."))?;
    let forward = unit(sub(muzzle.position, grip.position)).ok_or_else(|| invalid("The hero's muzzle and grip bones coincide; the gun direction cannot be measured."))?;
    let reach = norm(sub(muzzle.position, grip.position));
    // Roll from the weapon bone's own axes: the axis most perpendicular to the barrel.
    let matrix: Option<[[f64; 4]; 4]> = root_bone.details.get("matrix").and_then(|m| serde_json::from_value(m.clone()).ok());
    let mut up = None;
    if let Some(m) = matrix {
        for column in [2usize, 1, 0] {
            let axis = [m[0][column], m[1][column], m[2][column]];
            let p = perp(axis, forward);
            if norm(p) > 0.7 * norm(axis) {
                up = unit(p);
                break;
            }
        }
    }
    let up = up.or_else(|| unit(perp([0., 0., 1.], forward))).or_else(|| unit(perp([0., 1., 0.], forward))).unwrap();
    Ok(Frame {
        grip: grip.position,
        muzzle: muzzle.position,
        forward,
        up,
        length: reach / 0.7,
        confidence: "low".into(),
        notes: vec!["Estimated from the hero's weapon and muzzle bones (no original gun mesh). Check the roll in the preview.".into()],
    })
}

/// Grip hint for the source gun: the nearer hand of the character.
pub fn source_grip(candidate: &Candidate, source: &[Bone], roles: &BTreeMap<String, String>) -> Option<Vec3> {
    let points = &candidate.sample;
    let size = norm(sub(candidate.bounds[1], candidate.bounds[0]));
    let mut best: Option<(f64, Vec3)> = None;
    for role in ["hand_R", "hand_L"] {
        let Some(bone) = roles.get(role).and_then(|n| source.iter().find(|b| &b.name == n)) else { continue };
        let distance = points.iter().map(|p| norm(sub(*p, bone.position))).fold(f64::INFINITY, f64::min);
        if distance < size * 0.5 && best.as_ref().is_none_or(|b| distance < b.0) {
            best = Some((distance, bone.position));
        }
    }
    if let Some((_, hand)) = best {
        return Some(hand);
    }
    // A bone the gun is attached to that sits inside the gun (grip bone).
    candidate
        .attach_bone
        .as_ref()
        .and_then(|n| source.iter().find(|b| &b.name == n))
        .map(|b| b.position)
        .filter(|p| inside(*p, points, size * 0.05))
}

fn frame_matrix(frame: &Frame) -> [Vec3; 3] {
    [frame.forward, frame.up, right(frame)]
}

pub fn choice(options: &deadlock_project::PreparationOptions) -> Result<Choice> {
    match options.values.get("weapon") {
        None | Some(serde_json::Value::Null) => Ok(Choice::default()),
        Some(value) => {
            let choice: Choice = serde_json::from_value(value.clone()).map_err(|_| invalid("Invalid weapon choices. Reset the Weapon tab."))?;
            if !["", "replace", "remove", "keep"].contains(&choice.action.as_str()) {
                return Err(invalid("Choose what to do with the gun: fit it to the hero's gun, leave it out, or keep it on the hand."));
            }
            if !["", "match", "keep_size"].contains(&choice.fit.as_str()) || !["", "auto", "rigid"].contains(&choice.skin.as_str()) {
                return Err(invalid("Choose a supported weapon fit option."));
            }
            if !(choice.scale.is_finite() && choice.scale > 0.05 && choice.scale < 20.) || !choice.roll.is_finite() || choice.offset.iter().any(|v| !v.is_finite() || v.abs() > 500.) {
                return Err(invalid("Weapon scale, roll and offset must be finite and within range."));
            }
            Ok(choice)
        }
    }
}

/// Compute the source->target fit for the chosen gun.
pub fn fit(
    candidate: &Candidate,
    reference: &Reference,
    choice: &Choice,
    source: &[Bone],
    source_roles: &BTreeMap<String, String>,
    target: &[Bone],
    body_scale: Option<f64>,
) -> Result<Fit> {
    let mut notes = vec![];
    // A gun from the gun file that was moved into the hand sits there because
    // of its measured grip, so the hand adds nothing: measure the handle.
    let grip_hint = if candidate.attach_kind == "placed" { None } else { source_grip(candidate, source, source_roles) };
    let mut source_frame = measure(&candidate.sample, grip_hint, None)?;
    let target_frame = target_frame(reference, target)?;
    if choice.flip {
        source_frame.forward = mul(source_frame.forward, -1.);
        notes.push("Barrel direction flipped by you.".into());
    }
    if choice.roll != 0. {
        source_frame.up = rotate(source_frame.up, source_frame.forward, -choice.roll);
    }
    let fs = frame_matrix(&source_frame);
    let ft = frame_matrix(&target_frame);
    // R = Ft * Fs^T (columns are forward, up, right).
    let mut rotation = [[0.; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            rotation[i][j] = (0..3).map(|k| ft[k][i] * fs[k][j]).sum();
        }
    }
    let mut scale = match choice.fit.as_str() {
        "keep_size" => match body_scale {
            Some(s) if s.is_finite() && s > 0. => s,
            _ => {
                notes.push("Body scale is unavailable here, so the gun was matched to the original gun's size.".into());
                target_frame.length / source_frame.length
            }
        },
        _ => {
            if reference.source == "meshes" {
                target_frame.length / source_frame.length
            } else {
                // Skeleton-only targets: grip and muzzle bones fix the size.
                norm(sub(target_frame.muzzle, target_frame.grip)) / norm(sub(source_frame.muzzle, source_frame.grip)).max(1e-9)
            }
        }
    };
    scale *= choice.scale;
    if !(scale.is_finite() && scale > 0.) {
        return Err(invalid("The weapon scale could not be computed."));
    }
    let apply = |p: Vec3| -> Vec3 {
        let r = [dot(rotation[0], p), dot(rotation[1], p), dot(rotation[2], p)];
        mul(r, scale)
    };
    let offset = add(add(mul(target_frame.forward, choice.offset[0]), mul(target_frame.up, choice.offset[1])), mul(right(&target_frame), choice.offset[2]));
    let translation = sub(add(target_frame.grip, offset), apply(source_frame.grip));
    let mut matrix = [[0.; 4]; 4];
    for i in 0..3 {
        for j in 0..3 {
            matrix[i][j] = rotation[i][j] * scale;
        }
        matrix[i][3] = translation[i];
    }
    matrix[3][3] = 1.;
    let fitted_muzzle = add(apply(source_frame.muzzle), translation);
    let muzzle_offset_mm = norm(sub(fitted_muzzle, target_frame.muzzle)) * 25.4;
    if muzzle_offset_mm > 25.4 * 2. {
        notes.push(format!("The muzzle ends {:.0} mm from the original muzzle. Shots and muzzle flashes still come from the original position.", muzzle_offset_mm));
    }
    let (skin, skin_bones) = skin_choice(reference, choice, target)?;
    let anchor = if skin == "transfer" { heaviest_bone(reference).unwrap_or_else(|| skin_bones[0].clone()) } else { skin_bones[0].clone() };
    Ok(Fit { policy: POLICY.into(), source: source_frame, target: target_frame, matrix, scale, muzzle_offset_mm, skin, skin_bones, anchor, notes })
}

/// The bone carrying the most of the original gun's skin weight.
fn heaviest_bone(reference: &Reference) -> Option<String> {
    let mut totals: BTreeMap<&str, f64> = BTreeMap::new();
    for row in &reference.weights {
        for (name, weight) in row {
            *totals.entry(name.as_str()).or_default() += weight;
        }
    }
    totals.iter().filter(|(_, w)| **w > 0.).max_by(|a, b| a.1.total_cmp(b.1)).map(|(n, _)| n.to_string())
}

fn skin_choice(reference: &Reference, choice: &Choice, target: &[Bone]) -> Result<(String, Vec<String>)> {
    let names: BTreeSet<&str> = target.iter().map(|b| b.name.as_str()).collect();
    if !choice.bone.is_empty() {
        if !names.contains(choice.bone.as_str()) {
            return Err(invalid(format!("The chosen weapon bone is not in the hero skeleton: {}", choice.bone)));
        }
        return Ok(("rigid".into(), vec![choice.bone.clone()]));
    }
    let mut totals: BTreeMap<&str, f64> = BTreeMap::new();
    for row in &reference.weights {
        for (name, weight) in row {
            *totals.entry(name.as_str()).or_default() += weight;
        }
    }
    let total: f64 = totals.values().sum();
    if total > 0. {
        let (bone, top) = totals.iter().max_by(|a, b| a.1.total_cmp(b.1)).map(|(n, w)| (n.to_string(), *w)).unwrap();
        if choice.skin == "rigid" || top / total > 0.98 {
            return Ok(("rigid".into(), vec![bone]));
        }
        let mut bones: Vec<String> = totals.iter().filter(|(_, w)| **w / total > 0.01).map(|(n, _)| n.to_string()).collect();
        bones.sort();
        return Ok(("transfer".into(), bones));
    }
    let root = weapon_root(target).ok_or_else(|| invalid("This hero's skeleton has no weapon bone to attach the gun to."))?;
    Ok(("rigid".into(), vec![root]))
}

pub fn plan(fit: &Fit, candidate: &Candidate, reference: &Reference, action: &str) -> Plan {
    let skin = if fit.skin == "transfer" {
        let stride = (reference.points.len() / MAX_REFERENCE_POINTS).max(1);
        Skin::Transfer {
            points: reference.points.iter().step_by(stride).copied().collect(),
            weights: reference.weights.iter().step_by(stride).cloned().collect(),
        }
    } else {
        Skin::Rigid { bone: fit.skin_bones[0].clone() }
    };
    Plan {
        policy: POLICY.into(),
        objects: candidate.objects.clone(),
        action: action.into(),
        matrix: fit.matrix,
        skin,
        mesh_prefix: "weapon_".into(),
        replace: if reference.separate_model { reference.model_meshes.clone() } else { reference.meshes.clone() },
        anchor: fit.anchor.clone(),
    }
}

pub fn removal(candidate: &Candidate) -> Plan {
    Plan {
        policy: POLICY.into(),
        objects: candidate.objects.clone(),
        action: "remove".into(),
        matrix: [[1., 0., 0., 0.], [0., 1., 0., 0.], [0., 0., 1., 0.], [0., 0., 0., 1.]],
        skin: Skin::Rigid { bone: String::new() },
        mesh_prefix: "weapon_".into(),
        replace: vec![],
        anchor: String::new(),
    }
}

/// How plausible a candidate is as the held gun (higher is better).
pub fn score(candidate: &Candidate, source: &[Bone], roles: &BTreeMap<String, String>) -> f64 {
    let mut score = 0.;
    if !candidate.keywords.is_empty() {
        score += 3.;
    }
    let hand = |role: &str| roles.get(role).cloned();
    if let Some(bone) = &candidate.attach_bone {
        let mut cursor = Some(bone.clone());
        let mut depth = 0;
        while let Some(name) = cursor {
            if Some(&name) == hand("hand_R").as_ref() || Some(&name) == hand("hand_L").as_ref() {
                score += 2.5;
                break;
            }
            if roles.values().any(|r| r == &name) && depth > 0 {
                break;
            }
            cursor = source.iter().find(|b| b.name == name).and_then(|b| b.parent.clone());
            depth += 1;
        }
        if name_has(bone, &["weapon", "gun", "prop"]) {
            score += 2.;
        }
    }
    if source_grip(candidate, source, roles).is_some() {
        score += 1.;
    }
    if candidate.elongation >= 1.8 {
        score += 1.;
    }
    score
}

/// Choose the most likely gun automatically (only clear cases).
pub fn best_candidate<'a>(candidates: &'a [Candidate], source: &[Bone], roles: &BTreeMap<String, String>) -> Option<&'a Candidate> {
    candidates
        .iter()
        .map(|c| (score(c, source, roles), c))
        .filter(|(s, _)| *s >= 3.5)
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, c)| c)
}

/// The gun a fresh inspection suggests: the gun file's gun, then a clear
/// candidate; a weapon model falls back to its largest candidate (a gun-only
/// file with an unnamed, unrigged gun is one whole-file candidate).
pub fn suggest<'a>(candidates: &'a [Candidate], source: &[Bone], roles: &BTreeMap<String, String>, weapon_model: bool) -> Option<&'a Candidate> {
    candidates
        .iter()
        .find(|c| c.gun_file)
        .or_else(|| best_candidate(candidates, source, roles))
        .or_else(|| if weapon_model { candidates.iter().max_by_key(|c| c.vertices) } else { None })
}

pub fn preview(fit: &Fit, candidate: &Candidate, reference: &Reference) -> Preview {
    let m = fit.matrix;
    let fitted = candidate
        .sample
        .iter()
        .map(|p| [0, 1, 2].map(|i| m[i][0] * p[0] + m[i][1] * p[1] + m[i][2] * p[2] + m[i][3]))
        .collect();
    let stride = (reference.points.len() / 4096).max(1);
    Preview { fit: fit.clone(), fitted, reference: reference.points.iter().step_by(stride).copied().collect() }
}

// ------------------------------------------------------- original model reading

fn lod_base(name: &str) -> String {
    deadlock_model::lod_base(name)
}

fn keyword_mesh(name: &str) -> bool {
    name_has(name, &["gun", "weapon", "pistol", "rifle", "revolver", "shotgun", "smg", "blaster", "launcher", "cannon", "wpn"])
}

/// Read the original model's gun from its decompiled sources.
///
/// `model_text` is the original VMDL; `base` is the folder holding its DMX
/// files. For a separate weapon model every render mesh is the weapon.
pub fn reference_from_sources(
    base: &Path,
    model_text: &str,
    target: &[PreparedBone],
    separate_model: bool,
    cancel: &deadlock_core::CancellationToken,
) -> Result<Reference> {
    let analysis = deadlock_model::analyze(model_text).map_err(|e| invalid(e.to_string()))?;
    let document = deadlock_resource::text::Document::parse(model_text).map_err(|e| invalid(e.to_string()))?;
    let bones: Vec<Bone> = target.iter().map(Bone::from).collect();
    let weapon = weapon_bones(&bones);
    let mut reference = Reference {
        policy: POLICY.into(),
        source: "skeleton".into(),
        separate_model,
        model_meshes: analysis.meshes.iter().map(|m| m.name.clone()).collect(),
        attachments: attachments(model_text, target),
        ..Default::default()
    };
    let mut previews: BTreeMap<String, Option<deadlock_preview::Preview>> = BTreeMap::new();
    let mut gun_bases = BTreeSet::new();
    for node in document.objects(Some("RenderMeshFile")) {
        let Some(name) = node.get("name").and_then(|n| n.scalar()) else { continue };
        let Some(file) = node.get("filename").and_then(|n| n.scalar()) else { continue };
        if lod_base(name) != name {
            continue; // lower LODs follow their base mesh
        }
        cancel.check()?;
        let preview = previews.entry(file.to_string()).or_insert_with(|| {
            let path = deadlock_core::safety::inside(base, file).ok()?;
            let dmx = deadlock_dmx::Dmx::load(&path).ok()?;
            deadlock_preview::from_dmx(&dmx, file.to_string(), cancel).ok()
        });
        let Some(preview) = preview.as_ref() else { continue };
        // Respect the node's import filter when several meshes share one DMX.
        let filter = node.get("import_filter");
        let exclude_by_default = filter.and_then(|f| f.get("exclude_by_default")).and_then(|v| v.scalar()) == Some("true");
        let listed: BTreeSet<String> = filter
            .and_then(|f| f.get("exception_list"))
            .and_then(|l| l.array())
            .map(|l| l.iter().filter_map(|v| v.scalar().map(str::to_string)).collect())
            .unwrap_or_default();
        let mut points = vec![];
        let mut weights = vec![];
        for info in &preview.metadata.meshes {
            if filter.is_some() && exclude_by_default != listed.contains(&info.source_name) {
                continue;
            }
            let floats = |slice: &deadlock_preview::BufferSlice| -> Vec<f32> {
                preview.data[slice.offset..slice.offset + slice.count * 4]
                    .chunks_exact(4)
                    .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                    .collect()
            };
            let positions = floats(&info.positions);
            let rows: Vec<Vec<(String, f64)>> = match &info.skin {
                Some(skin) => {
                    let data = floats(&skin.data);
                    (0..info.vertex_count)
                        .map(|v| {
                            (0..skin.influences)
                                .filter_map(|slot| {
                                    let at = (v * skin.influences + slot) * 2;
                                    let joint = data.get(at)?.round() as usize;
                                    let weight = f64::from(*data.get(at + 1)?);
                                    if weight > 1e-4 { Some((skin.joint_names.get(joint)?.clone(), weight)) } else { None }
                                })
                                .collect()
                        })
                        .collect()
                }
                None => vec![vec![]; info.vertex_count],
            };
            for (chunk, row) in positions.chunks_exact(3).zip(rows) {
                points.push([f64::from(chunk[0]), f64::from(chunk[1]), f64::from(chunk[2])]);
                weights.push(row);
            }
        }
        if points.is_empty() {
            continue;
        }
        let total: f64 = weights.iter().flatten().map(|(_, w)| w).sum();
        let on_weapon: f64 = weights.iter().flatten().filter(|(n, _)| weapon.contains(n)).map(|(_, w)| w).sum();
        let is_gun = separate_model || (total > 0. && on_weapon / total >= 0.85) || (keyword_mesh(name) && total > 0. && on_weapon / total >= 0.5);
        if !is_gun {
            continue;
        }
        gun_bases.insert(deadlock_model::canonical(name));
        reference.primary.push(name.to_string());
        reference.points.extend(points);
        reference.weights.extend(weights);
    }
    reference.meshes = analysis
        .meshes
        .iter()
        .filter(|m| gun_bases.contains(&deadlock_model::canonical(&lod_base(&m.name))))
        .map(|m| m.name.clone())
        .collect();
    if !reference.points.is_empty() {
        reference.source = "meshes".into();
        let mut used: BTreeSet<String> = BTreeSet::new();
        for row in &reference.weights {
            for (name, _) in row {
                used.insert(name.clone());
            }
        }
        reference.bones = used.into_iter().collect();
        if reference.points.len() > MAX_REFERENCE_POINTS * 2 {
            let stride = reference.points.len() / (MAX_REFERENCE_POINTS * 2) + 1;
            reference.points = reference.points.iter().step_by(stride).copied().collect();
            reference.weights = reference.weights.iter().step_by(stride).cloned().collect();
        }
    } else if !separate_model {
        reference.notes.push("This hero model has no gun mesh of its own; its gun is a separate model.".into());
    }
    Ok(reference)
}

fn attachments(model_text: &str, target: &[PreparedBone]) -> Vec<Attachment> {
    let Ok(document) = deadlock_resource::text::Document::parse(model_text) else { return vec![] };
    let mut out = vec![];
    for node in document.objects(Some("Attachment")) {
        let Some(name) = node.get("name").and_then(|n| n.scalar()) else { continue };
        let vector = |key: &str| -> Option<Vec3> {
            let values: Vec<f64> = node.get(key)?.array()?.iter().filter_map(|v| v.scalar()?.parse().ok()).collect();
            (values.len() == 3).then(|| [values[0], values[1], values[2]])
        };
        let origin = vector("relative_origin").unwrap_or([0.; 3]);
        let parent = node.get("parent_bone").and_then(|n| n.scalar()).unwrap_or_default();
        let position = match target.iter().find(|b| b.name == parent) {
            Some(bone) => {
                let m = bone.matrix;
                [0, 1, 2].map(|i| m[i][0] * origin[0] + m[i][1] * origin[1] + m[i][2] * origin[2] + m[i][3])
            }
            None if parent.is_empty() => origin,
            None => continue,
        };
        out.push(Attachment { name: name.to_string(), position });
    }
    out
}

/// Mesh mappings that put the fitted gun in place of the original gun meshes.
pub fn mesh_mappings(plan_meshes: &[String], replace: &[String]) -> BTreeMap<String, deadlock_project::MeshMapping> {
    let mut out = BTreeMap::new();
    if plan_meshes.is_empty() {
        return out;
    }
    let value = if plan_meshes.len() == 1 {
        deadlock_project::MeshMapping::One(plan_meshes[0].clone())
    } else {
        deadlock_project::MeshMapping::Many(plan_meshes.to_vec())
    };
    for name in replace {
        out.insert(name.clone(), value.clone());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pistol(origin: Vec3, forward: Vec3, up: Vec3, size: f64) -> Vec<Vec3> {
        let right = cross(forward, up);
        let mut points = vec![];
        // Slide/barrel: long box above the grip.
        for i in 0..=20 {
            for (a, b) in [(-1., -1.), (-1., 1.), (1., -1.), (1., 1.)] {
                let t = -0.06 + 0.24 * i as f64 / 20.;
                let p = add(add(add(origin, mul(forward, t * size)), mul(up, (0.05 + 0.02 * a) * size)), mul(right, 0.015 * b * size));
                points.push(p);
            }
        }
        // Grip: box hanging down at the rear.
        for i in 0..=10 {
            for (a, b) in [(-1., -1.), (-1., 1.), (1., -1.), (1., 1.)] {
                let d = -0.08 + 0.1 * i as f64 / 10.;
                let p = add(add(add(origin, mul(up, d * size)), mul(forward, 0.025 * a * size)), mul(right, 0.015 * b * size));
                points.push(p);
            }
        }
        points
    }
    #[test]
    fn measures_barrel_up_and_grip_without_hints() {
        let points = pistol([1., 2., 3.], [0., -1., 0.], [0., 0., 1.], 1.);
        let frame = measure(&points, None, None).unwrap();
        assert!(dot(frame.forward, [0., -1., 0.]) > 0.99, "{:?}", frame.forward);
        assert!(dot(frame.up, [0., 0., 1.]) > 0.95, "{:?}", frame.up);
        assert!(frame.grip[2] < 3.0);
    }
    #[test]
    fn fit_maps_source_gun_onto_reference_gun() {
        // Source pistol held in the right hand, pointing -Y; hero pistol points +Z, twice as big.
        let source_points = pistol([-0.74, 0., 1.45], [0., -1., 0.], [0., 0., 1.], 1.);
        let reference_points = pistol([0., 0., 0.], [0., 0., 1.], [1., 0., 0.], 40.);
        let candidate = Candidate {
            id: "Pistol".into(), objects: vec!["Pistol".into()], vertices: source_points.len() as u64,
            bounds: [[-0.8, -0.2, 1.3], [-0.7, 0.2, 1.6]], sample: source_points.clone(),
            attach_bone: Some("hand_R".into()), attach_kind: "bone_parent".into(), keywords: vec!["pistol".into()], elongation: 3., gun_file: false, bones: vec![],
        };
        let reference = Reference {
            policy: POLICY.into(), source: "meshes".into(), points: reference_points.clone(),
            weights: vec![vec![("weapon".into(), 1.)]; reference_points.len()], ..Default::default()
        };
        let target: Vec<Bone> = serde_json::from_value(serde_json::json!([{"name":"weapon","parent":null,"position":[0,0,0]}])).unwrap();
        let fit = fit(&candidate, &reference, &Choice::default(), &[], &BTreeMap::new(), &target, None).unwrap();
        assert!((fit.scale - 40.).abs() < 0.5, "{}", fit.scale);
        let m = fit.matrix;
        let apply = |p: Vec3| [0, 1, 2].map(|i| m[i][0] * p[0] + m[i][1] * p[1] + m[i][2] * p[2] + m[i][3]);
        let fitted: Vec<Vec3> = source_points.iter().map(|p| apply(*p)).collect();
        // Every fitted point lands on the reference gun.
        for (a, b) in fitted.iter().zip(&reference_points) {
            assert!(norm(sub(*a, *b)) < 0.5, "{a:?} vs {b:?}");
        }
        assert_eq!(fit.skin, "rigid");
        assert_eq!(fit.skin_bones, vec!["weapon".to_string()]);
    }
}
