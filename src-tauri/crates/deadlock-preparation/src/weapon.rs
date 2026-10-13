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
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::Path;

pub const POLICY: &str = "weapon-fit-v1";
const MAX_REFERENCE_POINTS: usize = 6_000;
/// Triangle budgets of the fit preview's solid shapes (see `simplify`).
const PREVIEW_BODY_TRIANGLES: usize = 8_000;
const PREVIEW_GUN_TRIANGLES: usize = 5_000;

pub type Vec3 = [f64; 3];

/// A small triangle mesh for the fit preview, simplified so the inspection
/// stays small; positions are in the space of the points it goes with.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Shape {
    /// x, y, z of each vertex.
    pub positions: Vec<f32>,
    /// Three vertex indices per triangle.
    pub triangles: Vec<u32>,
}

impl Shape {
    pub fn is_empty(&self) -> bool {
        self.triangles.is_empty()
    }
}

/// Simplify a mesh for the preview by vertex clustering: the vertices in one
/// grid cell become one vertex at their mean, and the grid gets coarser until
/// at most `max` triangles are left. Rough, but fast, and it keeps the outline.
pub fn simplify(points: &[[f32; 3]], triangles: &[[u32; 3]], max: usize) -> Shape {
    let valid = |p: &[f32; 3]| p.iter().all(|v| v.is_finite());
    let (mut lo, mut hi) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
    for p in points.iter().filter(|p| valid(p)) {
        for i in 0..3 {
            lo[i] = lo[i].min(p[i]);
            hi[i] = hi[i].max(p[i]);
        }
    }
    let extent = (0..3).map(|i| hi[i] - lo[i]).fold(0f32, f32::max);
    if triangles.is_empty() || max == 0 || !(extent.is_finite() && extent > 0.) {
        return Shape::default();
    }
    let mut cells = 128f32;
    loop {
        let size = extent / cells;
        let mut ids: HashMap<[i32; 3], u32> = HashMap::new();
        let mut sums: Vec<[f64; 4]> = vec![];
        let cell: Vec<Option<u32>> = points
            .iter()
            .map(|p| {
                if !valid(p) {
                    return None;
                }
                let key = [0, 1, 2].map(|i| ((p[i] - lo[i]) / size).floor() as i32);
                let id = *ids.entry(key).or_insert_with(|| {
                    sums.push([0.; 4]);
                    sums.len() as u32 - 1
                });
                let sum = &mut sums[id as usize];
                for i in 0..3 {
                    sum[i] += f64::from(p[i]);
                }
                sum[3] += 1.;
                Some(id)
            })
            .collect();
        let of = |i: u32| cell.get(i as usize).copied().flatten();
        let mut seen = HashSet::new();
        let mut kept = vec![];
        for t in triangles {
            let (Some(a), Some(b), Some(c)) = (of(t[0]), of(t[1]), of(t[2])) else { continue };
            if a == b || b == c || a == c {
                continue;
            }
            let mut key = [a, b, c];
            key.sort_unstable();
            if seen.insert(key) {
                kept.push([a, b, c]);
            }
        }
        if kept.len() <= max || cells <= 8. {
            // Keep only the cells the triangles use, numbered in order of use,
            // at about a two-thousandth of the size (short numbers in the JSON).
            let scale = 10f64.powi((3 - f64::from(extent).log10().floor() as i32).clamp(0, 6));
            let mut number: HashMap<u32, u32> = HashMap::new();
            let mut shape = Shape { positions: vec![], triangles: Vec::with_capacity(kept.len() * 3) };
            for triangle in &kept {
                for &id in triangle {
                    let next = number.len() as u32;
                    let index = *number.entry(id).or_insert_with(|| {
                        let sum = sums[id as usize];
                        shape.positions.extend((0..3).map(|i| ((sum[i] / sum[3] * scale).round() / scale) as f32));
                        next
                    });
                    shape.triangles.push(index);
                }
            }
            return shape;
        }
        // Triangles fall roughly with the square of the cell count.
        cells = (cells * (max as f32 / kept.len() as f32).sqrt().clamp(0.5, 0.9)).max(8.);
    }
}

/// A mesh being collected for `simplify`.
#[derive(Default)]
struct RawShape {
    points: Vec<[f32; 3]>,
    triangles: Vec<[u32; 3]>,
}

impl RawShape {
    fn append(&mut self, other: &RawShape) {
        let base = self.points.len() as u32;
        self.points.extend_from_slice(&other.points);
        self.triangles.extend(other.triangles.iter().map(|t| t.map(|i| i + base)));
    }
}

/// Where the gun's named parts are (their centres in the source scene): parts
/// named for its front (muzzle, barrel, nose, emitter ...), its rear (stock,
/// rear ...) and its grip. They tell which end is the muzzle and which side is
/// down when the shape alone cannot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Hints {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub front: Option<Vec3>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rear: Option<Vec3>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grip: Option<Vec3>,
}

impl Hints {
    pub fn is_empty(&self) -> bool {
        self.front.is_none() && self.rear.is_none() && self.grip.is_none()
    }
}

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
    /// The hand a gun-file gun belongs to ("right" or "left") when the file
    /// holds a pair; set by the Blender import from its names or position.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub side: Option<String>,
    /// Centres of the parts named for the gun's front, rear and grip.
    #[serde(default, skip_serializing_if = "Hints::is_empty")]
    pub hints: Hints,
    /// The gun as a small solid mesh for the fit preview.
    #[serde(default, skip_serializing_if = "Shape::is_empty")]
    pub shape: Shape,
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
    /// Character bone that holds the left gun of a pair (the left hand).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left_hand: Option<String>,
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
    /// The hand this reference describes ("right" or "left") when it is one
    /// gun of a dual-wield hero (see `Reference::hand`); empty otherwise.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub side: String,
    /// A dual-wield hero's two guns, each read on its own (Haze holds a pistol
    /// in each hand). Empty for heroes with one gun.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sides: Vec<Side>,
    /// A sample of the hero's body (the meshes that are not the gun), shown
    /// around the guns in the fit preview.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub body: Vec<Vec3>,
    /// Gun-named meshes whose files could not be read (the gun may be in the
    /// model even though it was not measured).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unreadable: Vec<String>,
    /// The hero's gun (all of its guns) as a small solid mesh for the preview.
    #[serde(default, skip_serializing_if = "Shape::is_empty")]
    pub shape: Shape,
    /// The hero's body as a small solid mesh for the preview.
    #[serde(default, skip_serializing_if = "Shape::is_empty")]
    pub body_shape: Shape,
}

/// One gun of a dual-wield hero: its meshes (all detail levels) and the
/// points and skin weights of its base mesh.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Side {
    /// "right" or "left".
    pub side: String,
    pub meshes: Vec<String>,
    pub primary: Vec<String>,
    pub points: Vec<Vec3>,
    pub weights: Vec<Vec<(String, f64)>>,
    pub bones: Vec<String>,
    #[serde(default, skip_serializing_if = "Shape::is_empty")]
    pub shape: Shape,
}

impl Reference {
    /// Whether the hero holds a gun in each hand.
    pub fn dual(&self) -> bool {
        self.sides.iter().any(|s| s.side == "right") && self.sides.iter().any(|s| s.side == "left")
    }
    /// The reference for one hand of a dual-wield hero (the whole reference
    /// for a hero with one gun).
    pub fn hand(&self, side: &str) -> Reference {
        let Some(gun) = self.sides.iter().find(|s| s.side == side) else { return self.clone() };
        Reference {
            meshes: gun.meshes.clone(),
            primary: gun.primary.clone(),
            points: gun.points.clone(),
            weights: gun.weights.clone(),
            bones: gun.bones.clone(),
            side: side.into(),
            sides: vec![],
            body: vec![],
            unreadable: vec![],
            shape: gun.shape.clone(),
            body_shape: Shape::default(),
            ..self.clone()
        }
    }
}

/// Which hand a bone or object name belongs to. Tags such as `[L]`, `(R)`,
/// `.L` or `_R` decide first; otherwise words (left, right, l, r, 左, 右).
pub fn side_of(name: &str) -> Option<&'static str> {
    let lower = name.to_lowercase();
    let tagged = |tag: char| {
        [format!("[{tag}]"), format!("({tag})")].iter().any(|t| lower.contains(t.as_str()))
            || lower.ends_with(&format!(".{tag}"))
            || lower.ends_with(&format!("_{tag}"))
    };
    match (tagged('l'), tagged('r')) {
        (true, false) => return Some("left"),
        (false, true) => return Some("right"),
        _ => {}
    }
    // Words split at separators and camel case: weaponHand_L -> weapon, hand, l.
    let mut parts: Vec<String> = vec![];
    let mut word = String::new();
    let mut previous_lower = false;
    for c in name.chars() {
        if !c.is_alphanumeric() {
            parts.extend((!word.is_empty()).then(|| std::mem::take(&mut word).to_lowercase()));
            previous_lower = false;
            continue;
        }
        if c.is_uppercase() && previous_lower {
            parts.extend((!word.is_empty()).then(|| std::mem::take(&mut word).to_lowercase()));
        }
        previous_lower = c.is_lowercase();
        word.push(c);
    }
    parts.extend((!word.is_empty()).then(|| word.to_lowercase()));
    let left = parts.iter().any(|p| p == "l" || p.starts_with("left")) || lower.contains('左');
    let right = parts.iter().any(|p| p == "r" || p.starts_with("right")) || lower.contains('右');
    match (left, right) {
        (true, false) => Some("left"),
        (false, true) => Some("right"),
        _ => None,
    }
}

fn one() -> f64 {
    1.
}
fn yes() -> bool {
    true
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
    /// The gun for the hero's left hand (dual-wield heroes only); the fields
    /// above are the right hand's, or the only gun's.
    #[serde(default)]
    pub left: Hand,
    /// Join each gun's parts into one mesh; materials and bone weights stay.
    #[serde(default = "yes")]
    pub merge: bool,
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
            left: Hand::default(),
            merge: true,
        }
    }
}
impl Choice {
    /// The choice that fits the left hand's gun: the left hand's own
    /// adjustments with the shared size, skinning and merge choices.
    pub fn for_left(&self) -> Choice {
        Choice {
            objects: self.left.objects.clone(),
            flip: self.left.flip,
            roll: self.left.roll,
            scale: self.left.scale,
            offset: self.left.offset,
            bone: self.left.bone.clone(),
            left: Hand::default(),
            ..self.clone()
        }
    }
}

/// The left hand of a dual-wield hero.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Hand {
    /// Your gun for this hand; empty uses a copy of the right hand's gun.
    #[serde(default)]
    pub objects: Vec<String>,
    /// Keep the hero's own gun in this hand (only the right gun is replaced).
    #[serde(default)]
    pub keep: bool,
    #[serde(default)]
    pub flip: bool,
    #[serde(default)]
    pub roll: f64,
    #[serde(default = "one")]
    pub scale: f64,
    #[serde(default)]
    pub offset: Vec3,
    #[serde(default)]
    pub bone: String,
}
impl Default for Hand {
    fn default() -> Self {
        Self { objects: vec![], keep: false, flip: false, roll: 0., scale: 1., offset: [0.; 3], bone: String::new() }
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
    /// The hand this plan fits ("right", "left"), empty for a hero with one gun.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub side: String,
    /// The left hand's gun of a dual-wield hero.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left: Option<Box<Plan>>,
    /// This hand uses a copy of the right hand's gun objects.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub copy: bool,
    /// Join the gun's parts into one mesh (materials and weights are kept).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub merge: bool,
    /// Guns from the gun file that no hand uses; they are left out.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub drop: Vec<Vec<String>>,
}

impl Default for Plan {
    fn default() -> Self {
        Plan {
            policy: POLICY.into(),
            objects: vec![],
            action: String::new(),
            matrix: [[1., 0., 0., 0.], [0., 1., 0., 0.], [0., 0., 1., 0.], [0., 0., 0., 1.]],
            skin: Skin::Rigid { bone: String::new() },
            mesh_prefix: "weapon_".into(),
            replace: vec![],
            anchor: String::new(),
            side: String::new(),
            left: None,
            copy: false,
            merge: false,
            drop: vec![],
        }
    }
}

impl Plan {
    /// The plan of each hand (the right or only gun first).
    pub fn hands(&self) -> Vec<&Plan> {
        std::iter::once(self).chain(self.left.as_deref()).collect()
    }
    /// Candidate ids of the guns this conversion leaves out (left-out hands and
    /// unused gun-file guns); moving parts on them are left out too.
    pub fn removed_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.hands().into_iter().filter(|h| h.action == "remove").map(|h| h.objects.join("+")).collect();
        ids.extend(self.drop.iter().map(|objects| objects.join("+")));
        ids
    }
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
    /// How the barrel direction was found: "hint" (a muzzle bone or point),
    /// "names" (the gun's part names), "grip" (the muzzle is the end away
    /// from the grip), "shape" (the thinner end), "bones" (skeleton only),
    /// "flipped" (by the user) or "flipped_names" (by the user, against the
    /// front the part names gave).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub direction: String,
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
    /// Each hand of a dual-wield hero (the right hand is also the fields above).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hands: Vec<HandPreview>,
    /// A sample of the hero's body around the guns.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub body: Vec<Vec3>,
}

/// The fit preview of one hand; no fit when the hero keeps its own gun there.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HandPreview {
    pub side: String,
    #[serde(default)]
    pub fit: Option<Fit>,
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
    let mut direction = "";
    if let Some(m) = muzzle_hint {
        let x = t(&m, forward);
        if x.abs() > length * 0.1 {
            if x < 0. {
                forward = mul(forward, -1.);
            }
            direction = "hint";
        }
    }
    if direction.is_empty() {
        if let Some(g) = grip_hint {
            let x = t(&g, forward);
            if (x - lo - (hi - x)).abs() > length * 0.04 {
                if hi - x < x - lo {
                    forward = mul(forward, -1.);
                }
                direction = "grip";
            }
        }
    }
    if direction.is_empty() {
        direction = "shape";
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
    let muzzle = end_cap(points, forward);
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
    Ok(Frame { grip, muzzle, forward, up, length, confidence: confidence.into(), notes, direction: direction.into() })
}

/// The centre of the gun's last 4% along `forward`: its muzzle.
fn end_cap(points: &[Vec3], forward: Vec3) -> Vec3 {
    let along = |p: &Vec3| dot(*p, forward);
    let (lo, hi) = points.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), p| (a.min(along(p)), b.max(along(p))));
    let cap: Vec<Vec3> = points.iter().copied().filter(|p| along(p) >= hi - (hi - lo) * 0.04).collect();
    mean(&cap)
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
        let fits = |p: &Vec3| inside(*p, &reference.points, margin);
        let (grip, muzzle) = if reference.side.is_empty() {
            let muzzle = reference
                .attachments
                .iter()
                .find(|a| name_has(&a.name, &["muzzle", "barrel", "tip"]))
                .map(|a| a.position)
                .or_else(|| muzzle_bone(target).map(|b| b.position))
                .filter(fits);
            (grip_bone(target, &weapon).map(|b| b.position).filter(fits), muzzle)
        } else {
            // One gun of a pair: the hints of this hand (bones of the other
            // hand never count) that lie on this gun.
            let own = |name: &str| side_of(name).is_none_or(|s| s == reference.side);
            let muzzle = reference
                .attachments
                .iter()
                .filter(|a| name_has(&a.name, &["muzzle", "barrel", "tip"]) && own(&a.name))
                .map(|a| a.position)
                .find(fits)
                .or_else(|| target.iter().filter(|b| name_has(&b.name, &["muzzle"]) && own(&b.name)).map(|b| b.position).find(fits))
                .or_else(|| target.iter().filter(|b| name_has(&b.name, &["weapontip", "barrel", "fire"]) && own(&b.name)).map(|b| b.position).find(fits));
            let grip = target
                .iter()
                .filter(|b| weapon.contains(&b.name) && name_has(&b.name, &["hand", "grip"]) && own(&b.name) && norm(b.position) > 1e-6)
                .map(|b| b.position)
                .find(fits);
            (grip, muzzle)
        };
        let mut frame = measure(&reference.points, grip, muzzle)?;
        frame.notes.insert(0, match reference.side.as_str() {
            "" => "Measured from the hero's original gun meshes.".into(),
            side => format!("Measured from the hero's original {side}-hand gun."),
        });
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
        direction: "bones".into(),
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
            let hands = [(choice.scale, choice.roll, choice.offset), (choice.left.scale, choice.left.roll, choice.left.offset)];
            if hands.iter().any(|(scale, roll, offset)| !(scale.is_finite() && *scale > 0.05 && *scale < 20.) || !roll.is_finite() || offset.iter().any(|v| !v.is_finite() || v.abs() > 500.)) {
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
    let hints = &candidate.hints;
    // A gun from the gun file that was moved into the hand sits there because
    // of its measured grip, so the hand adds nothing: use the parts named as
    // the grip, or measure the handle.
    let grip_hint = if candidate.attach_kind == "placed" { hints.grip } else { source_grip(candidate, source, source_roles).or(hints.grip) };
    // Parts named for the front (Nose, Muzzle ...) or the rear (Stock, Rear
    // ...) tell which end is the muzzle; both together are the surest.
    let centre = mean(&candidate.sample);
    let muzzle_hint = match (hints.front, hints.rear) {
        (Some(front), Some(rear)) => Some(add(centre, sub(front, rear))),
        (Some(front), None) => Some(front),
        (None, Some(rear)) => Some(sub(mul(centre, 2.), rear)),
        (None, None) => None,
    };
    let mut source_frame = measure(&candidate.sample, grip_hint, muzzle_hint)?;
    if source_frame.direction == "hint" {
        source_frame.direction = "names".into();
        source_frame.notes.push("The barrel direction was read from the part names (front and rear parts).".into());
    }
    let target_frame = target_frame(reference, target)?;
    if choice.flip {
        source_frame.forward = mul(source_frame.forward, -1.);
        // The other end of the gun is the muzzle now.
        source_frame.muzzle = end_cap(&candidate.sample, source_frame.forward);
        // Flipping a front the part names gave is worth a second look.
        source_frame.direction = if source_frame.direction == "names" { "flipped_names" } else { "flipped" }.into();
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
        objects: candidate.objects.clone(),
        action: action.into(),
        matrix: fit.matrix,
        skin,
        replace: if reference.separate_model { reference.model_meshes.clone() } else { reference.meshes.clone() },
        anchor: fit.anchor.clone(),
        side: reference.side.clone(),
        ..Plan::default()
    }
}

pub fn removal(candidate: &Candidate) -> Plan {
    Plan { objects: candidate.objects.clone(), action: "remove".into(), ..Plan::default() }
}

/// What gun planning needs from an inspection.
pub struct Context<'a> {
    pub candidates: &'a [Candidate],
    pub reference: Option<&'a Reference>,
    /// Source bones (as inspected) and their humanoid roles.
    pub source: &'a [Bone],
    pub roles: &'a BTreeMap<String, String>,
    /// Hero bones with their bind matrices under `matrix`.
    pub target: &'a [Bone],
    pub body_scale: Option<f64>,
    /// A separate weapon model project (the gun is the whole model).
    pub weapon_model: bool,
}

fn find_candidate<'a>(candidates: &'a [Candidate], objects: &[String]) -> Result<&'a Candidate> {
    let wanted: BTreeSet<_> = objects.iter().collect();
    candidates
        .iter()
        .find(|c| c.objects.iter().collect::<BTreeSet<_>>() == wanted)
        .ok_or_else(|| invalid("The chosen gun is no longer in the inspected source. Inspect the source again and choose the gun in the Weapon tab."))
}

/// The hand a candidate is held in: the gun file's mark, otherwise the hand
/// its bone hangs from, otherwise the side its attach bone is named for.
pub fn candidate_side(candidate: &Candidate, source: &[Bone], roles: &BTreeMap<String, String>) -> Option<&'static str> {
    match candidate.side.as_deref() {
        Some("left") => return Some("left"),
        Some("right") => return Some("right"),
        _ => {}
    }
    let bone = candidate.attach_bone.as_ref()?;
    let mut cursor = Some(bone.clone());
    let mut steps = 0;
    while let Some(name) = cursor {
        if roles.get("hand_R") == Some(&name) {
            return Some("right");
        }
        if roles.get("hand_L") == Some(&name) {
            return Some("left");
        }
        if (steps > 0 && roles.values().any(|r| r == &name)) || steps >= 512 {
            break;
        }
        cursor = source.iter().find(|b| b.name == name).and_then(|b| b.parent.clone());
        steps += 1;
    }
    side_of(bone)
}

/// The weapon plan for a choice: the right (or only) gun, the left gun of a
/// dual-wield hero, and the gun-file guns no hand uses. None when the gun is
/// not customized (or follows the hand).
pub fn plan_choice(choice: &Choice, cx: &Context) -> Result<Option<Plan>> {
    if !cx.weapon_model && (!choice.enabled || choice.action == "keep" || choice.action.is_empty()) {
        return Ok(None);
    }
    if choice.objects.is_empty() {
        if cx.weapon_model {
            return Ok(None);
        }
        return Err(invalid("Choose which object in the source is the gun (Weapon tab)."));
    }
    let candidate = find_candidate(cx.candidates, &choice.objects)?;
    let left_candidate = if choice.left.objects.is_empty() { None } else { Some(find_candidate(cx.candidates, &choice.left.objects)?) };
    // Gun-file guns are only in the scene to be the gun: a pair whose second
    // gun no hand uses leaves that gun out.
    let unused = |used: &[&Candidate]| -> Vec<Vec<String>> {
        if cx.weapon_model {
            return vec![];
        }
        cx.candidates.iter().filter(|c| c.gun_file && !used.iter().any(|u| u.id == c.id)).map(|c| c.objects.clone()).collect()
    };
    if choice.action == "remove" && !cx.weapon_model {
        let mut plan = removal(candidate);
        if let Some(left) = left_candidate.filter(|l| l.id != candidate.id) {
            plan.left = Some(Box::new(Plan { side: "left".into(), ..removal(left) }));
            plan.side = "right".into();
        }
        plan.drop = unused(&[candidate, left_candidate.unwrap_or(candidate)]);
        return Ok(Some(plan));
    }
    let reference = cx.reference.ok_or_else(|| invalid("The hero's original gun has not been measured. Inspect the source again."))?;
    if !cx.weapon_model && reference.meshes.is_empty() {
        return Err(invalid(
            "This hero's gun is a separate model, so it cannot be replaced from the character file. Use Equipment → Weapon model to create a weapon project for it, and choose 'Leave it out' here.",
        ));
    }
    let action = if cx.weapon_model { "weapon_model" } else { "replace" };
    let fitted = |gun: &Candidate, reference: &Reference, choice: &Choice| -> Result<Plan> {
        let fit = fit(gun, reference, choice, cx.source, cx.roles, cx.target, cx.body_scale)?;
        Ok(Plan { merge: choice.merge, ..plan(&fit, gun, reference, action) })
    };
    if cx.weapon_model || !reference.dual() {
        let mut main = fitted(candidate, reference, choice)?;
        main.drop = unused(&[candidate]);
        return Ok(Some(main));
    }
    // A dual-wield hero: each hand's gun is fitted to the hero's gun in that hand.
    let mut main = fitted(candidate, &reference.hand("right"), choice)?;
    let mut used = vec![candidate];
    if !choice.left.keep {
        let gun = left_candidate.unwrap_or(candidate);
        let mut left = fitted(gun, &reference.hand("left"), &choice.for_left())?;
        left.mesh_prefix = "weapon_l_".into();
        left.copy = gun.id == candidate.id;
        used.push(gun);
        main.left = Some(Box::new(left));
    }
    main.drop = unused(&used);
    Ok(Some(main))
}

/// The gun choice a fresh inspection suggests. A dual-wield hero gets the
/// right gun in the right hand and, when there is a second gun, the left gun
/// in the left hand (otherwise a copy of the right one).
pub fn suggest_choice(cx: &Context) -> Option<Choice> {
    let best = suggest(cx.candidates, cx.source, cx.roles, cx.weapon_model)?;
    let reference = cx.reference;
    let action = if cx.weapon_model || reference.is_some_and(|r| !r.meshes.is_empty()) {
        "replace"
    } else if reference.is_some_and(|r| r.notes.iter().any(|n| n.contains("separate model"))) {
        "remove"
    } else {
        "keep"
    };
    let mut choice = Choice { enabled: true, action: action.into(), objects: best.objects.clone(), ..Default::default() };
    if cx.weapon_model {
        return Some(choice);
    }
    let dual = reference.is_some_and(Reference::dual);
    if !best.gun_file && !dual {
        return Some(choice); // a single gun from the character file: the clearest candidate
    }
    // The gun in each hand: the suggested gun when it is in that hand, otherwise
    // the likeliest gun of the same kind (the gun file's pair, or the
    // character's own guns) held there.
    let best_side = candidate_side(best, cx.source, cx.roles);
    let in_hand = |side: &str| -> Option<&Candidate> {
        if best_side == Some(side) {
            return Some(best);
        }
        cx.candidates
            .iter()
            .filter(|c| c.gun_file == best.gun_file && candidate_side(c, cx.source, cx.roles) == Some(side))
            .max_by(|a, b| score(a, cx.source, cx.roles).total_cmp(&score(b, cx.source, cx.roles)).then(a.vertices.cmp(&b.vertices)))
    };
    if let Some(right) = in_hand("right") {
        choice.objects = right.objects.clone();
    }
    if dual {
        if let Some(left) = in_hand("left").filter(|l| l.objects != choice.objects) {
            choice.left.objects = left.objects.clone();
        }
    }
    Some(choice)
}

/// The fit preview of a choice: the right (or only) gun, and for a dual-wield
/// hero each hand.
pub fn preview_choice(choice: &Choice, cx: &Context) -> Result<Preview> {
    let candidate = find_candidate(cx.candidates, &choice.objects).map_err(|_| invalid("Choose which object in the source is the gun."))?;
    let reference = cx.reference.ok_or_else(|| invalid("The hero's original gun has not been measured. Inspect the source again."))?;
    let thin = |points: &[Vec3]| -> Vec<Vec3> { points.iter().step_by((points.len() / 4096).max(1)).copied().collect() };
    if cx.weapon_model || !reference.dual() {
        let fit = fit(candidate, reference, choice, cx.source, cx.roles, cx.target, cx.body_scale)?;
        return Ok(Preview { body: reference.body.clone(), ..preview(&fit, candidate, reference) });
    }
    let right = reference.hand("right");
    let fit_right = fit(candidate, &right, choice, cx.source, cx.roles, cx.target, cx.body_scale)?;
    let mut result = preview(&fit_right, candidate, &right);
    let left = reference.hand("left");
    let left_hand = if choice.left.keep {
        HandPreview { side: "left".into(), fit: None, fitted: vec![], reference: thin(&left.points) }
    } else {
        let gun = if choice.left.objects.is_empty() { candidate } else { find_candidate(cx.candidates, &choice.left.objects)? };
        let fit_left = fit(gun, &left, &choice.for_left(), cx.source, cx.roles, cx.target, cx.body_scale)?;
        let shown = preview(&fit_left, gun, &left);
        HandPreview { side: "left".into(), fit: Some(shown.fit), fitted: shown.fitted, reference: shown.reference }
    };
    result.hands = vec![
        HandPreview { side: "right".into(), fit: Some(result.fit.clone()), fitted: result.fitted.clone(), reference: result.reference.clone() },
        left_hand,
    ];
    result.body = reference.body.clone();
    Ok(result)
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

/// The gun a fresh inspection suggests: the gun file's gun (the right-hand one
/// of a pair), then a clear candidate; a weapon model falls back to its largest
/// candidate (a gun-only file with an unnamed, unrigged gun is one whole-file
/// candidate).
pub fn suggest<'a>(candidates: &'a [Candidate], source: &[Bone], roles: &BTreeMap<String, String>, weapon_model: bool) -> Option<&'a Candidate> {
    candidates
        .iter()
        .find(|c| c.gun_file && c.side.as_deref() == Some("right"))
        .or_else(|| candidates.iter().find(|c| c.gun_file))
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
    Preview { fit: fit.clone(), fitted, reference: reference.points.iter().step_by(stride).copied().collect(), hands: vec![], body: vec![] }
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
    // Each base gun mesh with the hand its weights belong to, for dual-wield
    // heroes, and its triangles for the preview.
    let mut guns: Vec<(String, Option<&'static str>, Vec<Vec3>, Vec<Vec<(String, f64)>>, RawShape)> = vec![];
    let mut body: Vec<Vec3> = vec![];
    let mut body_shape = RawShape::default();
    let mut unreadable = vec![];
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
        let Some(preview) = preview.as_ref() else {
            if keyword_mesh(name) {
                unreadable.push(name.to_string());
            }
            continue;
        };
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
        let mut shape = RawShape::default();
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
            if let Some(indices) = preview.data.get(info.indices.offset..info.indices.offset + info.indices.count * 4) {
                let base = shape.points.len() as u32;
                shape.points.extend(positions.chunks_exact(3).map(|c| [c[0], c[1], c[2]]));
                let index = |b: &[u8]| base + u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
                shape.triangles.extend(indices.chunks_exact(12).map(|t| [index(&t[0..4]), index(&t[4..8]), index(&t[8..12])]));
            }
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
            body.extend(points.iter().step_by((points.len() / 2_000).max(1)).copied());
            body_shape.append(&shape);
            continue;
        }
        // The hand of a gun: nearly all of its sided weight on one hand's bones.
        let (mut left, mut right) = (0., 0.);
        for (bone, weight) in weights.iter().flatten() {
            match side_of(bone) {
                Some("left") => left += weight,
                Some("right") => right += weight,
                _ => {}
            }
        }
        let side = if left + right < total * 0.5 {
            None
        } else if left >= (left + right) * 0.8 {
            Some("left")
        } else if right >= (left + right) * 0.8 {
            Some("right")
        } else {
            None
        };
        gun_bases.insert(deadlock_model::canonical(name));
        reference.primary.push(name.to_string());
        reference.points.extend(points.iter().copied());
        reference.weights.extend(weights.iter().cloned());
        guns.push((name.to_string(), side, points, weights, shape));
    }
    let lods = |bases: &BTreeSet<String>| -> Vec<String> {
        analysis.meshes.iter().filter(|m| bases.contains(&deadlock_model::canonical(&lod_base(&m.name)))).map(|m| m.name.clone()).collect()
    };
    let used_bones = |weights: &[Vec<(String, f64)>]| -> Vec<String> {
        weights.iter().flatten().map(|(n, _)| n.clone()).collect::<BTreeSet<_>>().into_iter().collect()
    };
    let thin = |points: &mut Vec<Vec3>, weights: &mut Vec<Vec<(String, f64)>>| {
        if points.len() > MAX_REFERENCE_POINTS * 2 {
            let stride = points.len() / (MAX_REFERENCE_POINTS * 2) + 1;
            *points = points.iter().step_by(stride).copied().collect();
            *weights = weights.iter().step_by(stride).cloned().collect();
        }
    };
    reference.meshes = lods(&gun_bases);
    if !reference.points.is_empty() {
        reference.source = "meshes".into();
        reference.bones = used_bones(&reference.weights);
        thin(&mut reference.points, &mut reference.weights);
        // A gun in each hand (Haze): every gun mesh belongs to one hand and
        // both hands have one. Each hand is then fitted on its own.
        let all_sided = !separate_model && guns.iter().all(|g| g.1.is_some());
        if all_sided && ["left", "right"].iter().all(|s| guns.iter().any(|g| g.1 == Some(*s))) {
            for side in ["right", "left"] {
                let mut gun = Side { side: side.into(), ..Default::default() };
                let mut bases = BTreeSet::new();
                let mut shape = RawShape::default();
                for (name, _, points, weights, part) in guns.iter().filter(|g| g.1 == Some(side)) {
                    bases.insert(deadlock_model::canonical(name));
                    gun.primary.push(name.clone());
                    gun.points.extend(points.iter().copied());
                    gun.weights.extend(weights.iter().cloned());
                    shape.append(part);
                }
                gun.meshes = lods(&bases);
                gun.bones = used_bones(&gun.weights);
                thin(&mut gun.points, &mut gun.weights);
                gun.shape = simplify(&shape.points, &shape.triangles, PREVIEW_GUN_TRIANGLES);
                reference.sides.push(gun);
            }
        }
        if reference.sides.is_empty() {
            let mut shape = RawShape::default();
            for gun in &guns {
                shape.append(&gun.4);
            }
            reference.shape = simplify(&shape.points, &shape.triangles, PREVIEW_GUN_TRIANGLES);
        }
    } else if !unreadable.is_empty() {
        reference.notes.push(format!(
            "The hero's gun meshes ({}) could not be read from the game files, so the gun cannot be replaced. Leave your gun out or keep it in the hand.",
            unreadable.join(", ")
        ));
        reference.unreadable = unreadable;
    } else if !separate_model {
        reference.notes.push("This hero model has no gun mesh of its own; its gun is a separate model.".into());
    }
    if body.len() > 4_000 {
        body = body.iter().step_by(body.len() / 4_000 + 1).copied().collect();
    }
    reference.body = body;
    reference.body_shape = simplify(&body_shape.points, &body_shape.triangles, PREVIEW_BODY_TRIANGLES);
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

/// Records in Blender's weapon receipt which original meshes each fitted gun
/// replaces (per hand for a dual-wield hero).
pub fn record_replacements(audit: &mut serde_json::Value, plan: &Plan) {
    let Some(object) = audit.as_object_mut() else { return };
    object.insert("replace".into(), serde_json::json!(plan.replace));
    if let Some(hands) = object.get_mut("hands").and_then(|h| h.as_array_mut()) {
        for hand in hands {
            let side = hand.get("side").and_then(|s| s.as_str()).unwrap_or_default().to_string();
            if let (Some(planned), Some(hand)) = (plan.hands().into_iter().find(|h| h.side == side), hand.as_object_mut()) {
                hand.insert("replace".into(), serde_json::json!(planned.replace));
            }
        }
    }
}

/// Mesh mappings from a weapon receipt: each hand's fitted gun replaces that
/// hand's original gun (all detail levels).
pub fn receipt_mappings(audit: &serde_json::Value) -> BTreeMap<String, deadlock_project::MeshMapping> {
    let list = |value: Option<&serde_json::Value>| -> Vec<String> { value.and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default() };
    match audit.get("hands").and_then(|h| h.as_array()).filter(|h| !h.is_empty()) {
        Some(hands) => hands.iter().flat_map(|hand| mesh_mappings(&list(hand.get("meshes")), &list(hand.get("replace")))).collect(),
        None => mesh_mappings(&list(audit.get("meshes")), &list(audit.get("replace"))),
    }
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
    /// A gun whose front is wider than its back (an emitter or a muzzle
    /// brake), so "the thinner end is the muzzle" gets it backwards.
    fn blaster(forward: Vec3, up: Vec3) -> Vec<Vec3> {
        let right = cross(forward, up);
        let mut points = vec![];
        for i in 0..=40 {
            let t = -0.15 + 0.3 * i as f64 / 40.;
            let radius = if t > 0.1 { 0.06 } else { 0.02 };
            for k in 0..8 {
                let angle = k as f64 * std::f64::consts::FRAC_PI_4;
                points.push(add(add(mul(forward, t), mul(up, radius * angle.cos())), mul(right, radius * angle.sin())));
            }
        }
        points
    }
    #[test]
    fn simplified_shapes_keep_their_outline_within_the_triangle_budget() {
        // A finely divided unit sphere (57,600 triangles).
        let (rings, segments) = (120u32, 240u32);
        let mut points = vec![];
        for r in 0..=rings {
            for s in 0..segments {
                let (theta, phi) = (std::f32::consts::PI * r as f32 / rings as f32, std::f32::consts::TAU * s as f32 / segments as f32);
                points.push([theta.sin() * phi.cos(), theta.sin() * phi.sin(), theta.cos()]);
            }
        }
        let mut triangles = vec![];
        for r in 0..rings {
            for s in 0..segments {
                let (a, b) = (r * segments + s, r * segments + (s + 1) % segments);
                triangles.push([a, a + segments, b]);
                triangles.push([b, a + segments, b + segments]);
            }
        }
        let shape = simplify(&points, &triangles, 2_000);
        let count = shape.triangles.len() / 3;
        assert!(count > 200 && count <= 2_000, "{count}");
        let vertices = shape.positions.len() / 3;
        assert!(shape.triangles.iter().all(|&i| (i as usize) < vertices));
        for p in shape.positions.chunks_exact(3) {
            let radius = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
            assert!((radius - 1.).abs() < 0.1, "{radius}");
        }
        assert!(shape.positions.chunks_exact(3).map(|p| p[2]).fold(f32::MIN, f32::max) > 0.9);
        assert!(simplify(&points, &[], 10).is_empty());
        // Small meshes are kept as they are.
        let small = simplify(&[[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]], &[[0, 1, 2]], 10);
        assert_eq!((small.positions.len(), small.triangles), (9, vec![0, 1, 2]));
    }
    #[test]
    fn part_names_tell_the_muzzle_end_and_flipping_moves_the_muzzle() {
        let forward = [1., 0., 0.];
        let mut candidate = gun("Blaster", None, [0., 0., 0.]);
        candidate.sample = blaster(forward, [0., 0., 1.]);
        // By its shape alone the thin rear looks like the muzzle.
        assert!(measure(&candidate.sample, None, None).unwrap().forward[0] < -0.9);
        let points = pistol([0., 0., 0.], [0., 0., 1.], [1., 0., 0.], 40.);
        let reference = Reference {
            policy: POLICY.into(), source: "meshes".into(), weights: vec![vec![("weapon".into(), 1.)]; points.len()], points, ..Default::default()
        };
        let target: Vec<Bone> = serde_json::from_value(serde_json::json!([{"name":"weapon","parent":null,"position":[0,0,0]}])).unwrap();
        // Nose and emitter parts at the front, the rear bumper at the back.
        candidate.hints = Hints { front: Some([0.13, 0., 0.]), rear: Some([-0.12, 0., 0.]), grip: None };
        let named = fit(&candidate, &reference, &Choice::default(), &[], &BTreeMap::new(), &target, None).unwrap();
        assert!(dot(named.source.forward, forward) > 0.99, "{:?}", named.source.forward);
        assert_eq!(named.source.direction, "names");
        assert!(named.source.muzzle[0] > 0.1, "{:?}", named.source.muzzle);
        // Only a rear part named is enough too.
        candidate.hints.front = None;
        assert!(fit(&candidate, &reference, &Choice::default(), &[], &BTreeMap::new(), &target, None).unwrap().source.forward[0] > 0.99);
        // Flipping turns the gun around: its muzzle is then the other end, so
        // the reported muzzle position follows the new front.
        let flipped = fit(&candidate, &reference, &Choice { flip: true, ..Choice::default() }, &[], &BTreeMap::new(), &target, None).unwrap();
        assert_eq!(flipped.source.direction, "flipped_names");
        assert!(flipped.source.muzzle[0] < -0.1, "{:?}", flipped.source.muzzle);
        let m = flipped.matrix;
        let apply = |p: Vec3| [0, 1, 2].map(|i| m[i][0] * p[0] + m[i][1] * p[1] + m[i][2] * p[2] + m[i][3]);
        let ahead = |p: Vec3| dot(sub(apply(p), flipped.target.grip), flipped.target.forward);
        let front = candidate.sample.iter().map(|p| ahead(*p)).fold(f64::MIN, f64::max);
        assert!(ahead(flipped.source.muzzle) > front * 0.9, "{} vs {front}", ahead(flipped.source.muzzle));
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
            attach_bone: Some("hand_R".into()), attach_kind: "bone_parent".into(), keywords: vec!["pistol".into()], elongation: 3., gun_file: false, bones: vec![], side: None, hints: Hints::default(), shape: Shape::default(),
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
    #[test]
    fn hands_are_read_from_tags_suffixes_and_words() {
        for (name, side) in [
            ("weapon_hand_L", Some("left")), ("weaponHand_R", Some("right")), ("magazine_L", Some("left")),
            ("charm_pivot.L", Some("left")), ("mixamorig:LeftHand", Some("left")), ("RightHand", Some("right")),
            ("Barrel jacket • continuous curved magenta shell [L]", Some("left")),
            // The [L] tag names the gun; "R" here is the right side of that gun.
            ("Front SWITCH marking R [L]", Some("left")), ("Grip panel screw R0 head [R]", Some("right")),
            ("手首.R", Some("right")), ("左手", Some("left")), ("muzzle_fx", None), ("cleft_plate", None), ("Pistol", None),
        ] {
            assert_eq!(side_of(name), side, "{name}");
        }
    }
    fn gun(id: &str, side: Option<&str>, origin: Vec3) -> Candidate {
        let sample = pistol(origin, [0., -1., 0.], [0., 0., 1.], 1.);
        Candidate {
            id: id.into(), objects: vec![id.into()], vertices: sample.len() as u64, bounds: [[-1., -1., -1.], [1., 1., 1.]], sample,
            attach_bone: None, attach_kind: "placed".into(), keywords: vec![], elongation: 3., gun_file: true, bones: vec![],
            side: side.map(str::to_string), hints: Hints::default(), shape: Shape::default(),
        }
    }
    fn dual_reference() -> Reference {
        let right = pistol([-10., 0., 0.], [0., 0., 1.], [1., 0., 0.], 40.);
        let left = pistol([10., 0., 0.], [0., 0., 1.], [1., 0., 0.], 40.);
        let side = |name: &str, points: &[Vec3], bone: &str| Side {
            side: name.into(), meshes: vec![format!("gun_{name}"), format!("gun_{name}_lod1")], primary: vec![format!("gun_{name}")],
            points: points.to_vec(), weights: vec![vec![(bone.to_string(), 1.)]; points.len()], bones: vec![bone.into()], shape: Shape::default(),
        };
        Reference {
            policy: POLICY.into(), source: "meshes".into(),
            meshes: vec!["gun_right".into(), "gun_right_lod1".into(), "gun_left".into(), "gun_left_lod1".into()],
            points: [right.clone(), left.clone()].concat(),
            weights: vec![vec![("weapon_hand_R".into(), 1.)]; right.len() + left.len()],
            sides: vec![side("right", &right, "weapon_hand_R"), side("left", &left, "weapon_hand_L")],
            ..Default::default()
        }
    }
    #[test]
    fn dual_wield_heroes_get_a_fitted_gun_in_each_hand() {
        let reference = dual_reference();
        assert!(reference.dual());
        let target: Vec<Bone> = serde_json::from_value(serde_json::json!([
            {"name":"weapon_hand_R","parent":null,"position":[-10,0,0]}, {"name":"weapon_hand_L","parent":null,"position":[10,0,0]}])).unwrap();
        let candidates = vec![gun("gun_file_R", Some("right"), [0., 0., 0.]), gun("gun_file_L", Some("left"), [0., 1., 0.])];
        let cx = Context { candidates: &candidates, reference: Some(&reference), source: &[], roles: &BTreeMap::new(), target: &target, body_scale: None, weapon_model: false };
        let choice = suggest_choice(&cx).unwrap();
        assert_eq!((choice.action.as_str(), choice.objects.clone(), choice.left.objects.clone()), ("replace", vec!["gun_file_R".to_string()], vec!["gun_file_L".to_string()]));
        let plan = plan_choice(&choice, &cx).unwrap().unwrap();
        let left = plan.left.as_deref().unwrap();
        assert_eq!((plan.side.as_str(), plan.replace.clone()), ("right", vec!["gun_right".to_string(), "gun_right_lod1".to_string()]));
        assert_eq!((left.side.as_str(), left.replace.clone(), left.objects.clone()), ("left", vec!["gun_left".to_string(), "gun_left_lod1".to_string()], vec!["gun_file_L".to_string()]));
        assert_eq!((left.mesh_prefix.as_str(), left.copy, plan.merge, left.merge), ("weapon_l_", false, true, true));
        assert!(matches!(&left.skin, Skin::Rigid { bone } if bone == "weapon_hand_L"), "{:?}", left.skin);
        // Each gun lands on the hero's gun in its own hand.
        let at = |m: [[f64; 4]; 4], p: Vec3| [0, 1, 2].map(|i| m[i][0] * p[0] + m[i][1] * p[1] + m[i][2] * p[2] + m[i][3]);
        assert!(at(plan.matrix, candidates[0].sample[0])[0] < 0. && at(left.matrix, candidates[1].sample[0])[0] > 0.);
        // One gun from the file goes into both hands as a copy; the left gun can also stay the hero's.
        let single = Choice { left: Hand::default(), ..choice.clone() };
        let copied = plan_choice(&single, &cx).unwrap().unwrap();
        assert!(copied.left.as_deref().is_some_and(|l| l.copy && l.objects == vec!["gun_file_R".to_string()]));
        assert_eq!(copied.drop, vec![vec!["gun_file_L".to_string()]]);
        let kept = plan_choice(&Choice { left: Hand { keep: true, ..Hand::default() }, ..choice.clone() }, &cx).unwrap().unwrap();
        assert!(kept.left.is_none() && kept.replace == vec!["gun_right".to_string(), "gun_right_lod1".to_string()]);
        // The preview shows both hands.
        let preview = preview_choice(&choice, &cx).unwrap();
        assert_eq!(preview.hands.iter().map(|h| (h.side.as_str(), h.fit.is_some())).collect::<Vec<_>>(), vec![("right", true), ("left", true)]);
        // Leaving the guns out removes both and every unused gun-file gun.
        let removed = plan_choice(&Choice { action: "remove".into(), ..choice }, &cx).unwrap().unwrap();
        assert_eq!(removed.removed_ids(), vec!["gun_file_R".to_string(), "gun_file_L".to_string()]);
    }
    #[test]
    fn a_hero_with_one_gun_uses_the_right_gun_of_a_pair_and_leaves_the_other_out() {
        let reference = Reference { sides: vec![], ..dual_reference().hand("right") };
        let reference = Reference { side: String::new(), ..reference };
        let target: Vec<Bone> = serde_json::from_value(serde_json::json!([{"name":"weapon_hand_R","parent":null,"position":[-10,0,0]}])).unwrap();
        let candidates = vec![gun("gun_file_L", Some("left"), [0., 1., 0.]), gun("gun_file_R", Some("right"), [0., 0., 0.])];
        let cx = Context { candidates: &candidates, reference: Some(&reference), source: &[], roles: &BTreeMap::new(), target: &target, body_scale: None, weapon_model: false };
        let choice = suggest_choice(&cx).unwrap();
        assert_eq!((choice.objects.clone(), choice.left.objects.clone()), (vec!["gun_file_R".to_string()], vec![]));
        let plan = plan_choice(&choice, &cx).unwrap().unwrap();
        assert!(plan.left.is_none() && plan.side.is_empty());
        assert_eq!(plan.drop, vec![vec!["gun_file_L".to_string()]]);
    }
    #[test]
    fn receipts_map_each_hand_to_its_own_original_gun() {
        let plan = Plan {
            side: "right".into(), replace: vec!["gun_right".into()],
            left: Some(Box::new(Plan { side: "left".into(), replace: vec!["gun_left".into()], ..removal(&gun("b", None, [0.; 3])) })),
            ..removal(&gun("a", None, [0.; 3]))
        };
        let mut audit = serde_json::json!({"meshes": ["weapon_gun", "weapon_l_gun"], "hands": [
            {"side": "right", "meshes": ["weapon_gun"]}, {"side": "left", "meshes": ["weapon_l_gun"]}]});
        record_replacements(&mut audit, &plan);
        let mappings = receipt_mappings(&audit);
        assert_eq!(mappings.get("gun_right"), Some(&deadlock_project::MeshMapping::One("weapon_gun".into())));
        assert_eq!(mappings.get("gun_left"), Some(&deadlock_project::MeshMapping::One("weapon_l_gun".into())));
        // Single-gun receipts keep their earlier meaning.
        let single = serde_json::json!({"meshes": ["weapon_a", "weapon_b"], "replace": ["gun", "gun_lod1"]});
        assert_eq!(receipt_mappings(&single).len(), 2);
    }
}
