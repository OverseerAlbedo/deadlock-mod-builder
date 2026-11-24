//! The movement preview's cloth packet: the cloth model the game simulates for
//! automatic secondary motion, predicted before any build from the proxy that
//! Analyze authors (`Setup.proxy`), so the page can run a particle solver on it.
//!
//! The compiler's rules below were read from compiled models of the same
//! project state and are checked against such compiled models:
//! - nodes: the proxy's static points that share an edge with a moving point
//!   (in point order), the body bones carrying them (first use), the collider
//!   bones not yet listed, then the moving points ordered by their edge-hop
//!   distance from the static points (ties by point order), then the moving
//!   bones (simulated groups' bones in group order that carry a moving point);
//! - bone placement: a bone's position node is the first point of its ring;
//!   which two ring edges the compiler takes for the frame is not derived (the
//!   ring is rigid, so any two give the same placement while it keeps its
//!   shape); the packet takes x from the second point to the first and y
//!   from the third to the second;
//! - rods: every mesh edge with a moving end, rigid at its bind length
//!   (weight 0.5, or 0 on a static end); across each edge, faces paired in
//!   face order (first with second, third with fourth...) give a bend rod
//!   between their opposite corners whose limits are the corners' distance
//!   folded shut and laid flat about that edge, merged over every such edge
//!   as the smallest and largest (weight by inverse mass);
//! - inverse mass of a moving point: 1 / (8 × the length of its edges);
//! - per point: attraction g²/(1+g²) of the pose pull, damping 30 × drag,
//!   gravity 360 × the gravity scale, travel limits of 0.03 in and more kept;
//! - collision layers from the `cloth_collision_layer_0..3` streams.
//!
//! What the app changes after compiling (strand roll removal, fabric and
//! streamer rods, attachment limits) is applied by running the same functions
//! the build runs (`deadlock_cloth::generated`) on the predicted model.
//!
//! The packet is a file of its own in the workspace (`FILE_NAME`), never a
//! serialized field of `Setup` or the inspection report: those are hashed for
//! build freshness.
use crate::{
    invalid,
    parts::Kind,
    secondary::Setup,
    surface, Result,
};
use deadlock_dmx::Dmx;
use deadlock_project::customization::MotionTuning;
use deadlock_resource::{Kind as V, Value};
use deadlock_skeleton::transforms;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// Version of the packet's layout; the page refuses other versions.
pub const VERSION: u32 = 1;
/// The packet's file in the project workspace, written by Analyze.
pub const FILE_NAME: &str = "preview-cloth.json";
/// Compiled gravity of a gravity scale of 1 (`secondary_auto::GAME_GRAVITY`).
pub const GAME_GRAVITY: f64 = crate::secondary_auto::GAME_GRAVITY;
/// Compiled point damping per unit of authored drag.
pub const GAME_DAMPING: f64 = 30.;
/// The smallest travel limit the compiler keeps (in inches, as float32). Seen
/// kept at 0.03 and dropped at 0.01 and 0.005; values in between are unproven.
pub const MIN_TRAVEL: f32 = 0.03;
/// Bend rods of the compiler: mass per inch of a point's edges.
pub const MASS_PER_EDGE_INCH: f64 = 8.;
/// The mask of a capsule collider and of a point on every layer.
pub const CAPSULE_LAYERS: u16 = 15;
pub const ALL_LAYERS: u16 = 0xffff;

/// A travel cap applied after Fine-tune, in order (see `secondary.rs`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(tag = "cap", content = "inches", rename_all = "snake_case")]
pub enum Cap {
    /// Exactly `HELD_ROOT_TRAVEL`, whatever Swing range says.
    Held,
    /// At most this; a free point (0) stays free (`cap_travel`).
    AtMost(f64),
}

/// One proxy point's authored values before Fine-tune (gravity as a scale,
/// travel in inches) and the caps applied after it.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct PointMotion {
    pub kind: Option<Kind>,
    pub goal: f64,
    pub drag: f64,
    pub gravity: f64,
    pub travel: f64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub caps: Vec<Cap>,
}

/// The game's per-node values (compiled units: attraction 0–1 per step,
/// damping, gravity in inches per second squared, travel in inches, 0 = free).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct GameValues {
    pub attraction: f64,
    pub damping: f64,
    pub gravity: f64,
    pub travel: f64,
}

/// Authored values to the game's (the compiler's mapping). Inputs are rounded
/// to float32 as the proxy streams store them.
pub fn game_values(goal: f64, drag: f64, gravity: f64, travel: f64) -> GameValues {
    let g = goal as f32 as f64;
    let t = travel as f32;
    GameValues {
        attraction: g * g / (1. + g * g),
        damping: GAME_DAMPING * (drag as f32 as f64),
        gravity: (gravity * GAME_GRAVITY) as f32 as f64,
        travel: if t >= MIN_TRAVEL { t as f64 } else { 0. },
    }
}

/// An attachment limit the build adds after compiling (`attachment_limits`):
/// in compiled units.
pub const ATTACHMENT_ATTRACTION: f64 = 0.5;
pub const ATTACHMENT_DAMPING: f64 = 11.4;

/// A node's game values from its authored values, the project's Fine-tune of
/// its part type (`None` when that type has no non-zero setting), its caps and
/// an attachment limit (radius in inches), in the build's order.
pub fn node_values(m: &PointMotion, tuning: Option<&MotionTuning>, attachment: Option<f64>) -> GameValues {
    let mut maps = [m.goal, m.drag, 1., m.gravity, m.travel, 0., 0., 0.];
    if let Some(t) = tuning {
        crate::secondary_auto::tune(&mut maps, t);
    }
    for cap in &m.caps {
        maps[4] = match *cap {
            Cap::Held => crate::secondary_auto::HELD_ROOT_TRAVEL,
            Cap::AtMost(v) => crate::secondary_auto::cap_travel(maps[4], Some(v)),
        };
    }
    let mut v = game_values(maps[0], maps[1], maps[3], maps[4]);
    if let Some(radius) = attachment {
        let f = |x: f64| x as f32 as f64;
        v.attraction = v.attraction.max(ATTACHMENT_ATTRACTION);
        v.damping = v.damping.max(ATTACHMENT_DAMPING);
        v.travel = if v.travel > 0. { f(v.travel.min(radius)) } else { f(radius) };
    }
    v
}

/// The Fine-tune settings the build applies (all-zero settings are ignored).
pub fn effective_tuning(tuning: &BTreeMap<String, MotionTuning>) -> BTreeMap<String, MotionTuning> {
    let mut t = tuning.clone();
    t.retain(|_, t| [t.stiffness, t.damping, t.gravity, t.swing].iter().any(|v| v.is_some_and(|v| v != 0.)));
    t
}

/// The tuning key of a part type.
pub fn kind_key(kind: Kind) -> String {
    serde_json::to_value(kind).ok().and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_default()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Bone {
    pub name: String,
    /// Bind transform in model space: x, y, z, scale, qx, qy, qz, qw.
    pub bind: [f64; 8],
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeMotion {
    #[serde(flatten)]
    pub motion: PointMotion,
    /// Index of the movement preview node (`Setup.preview_nodes`) of the
    /// node's bone, for labels and slider grouping.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview_node: Option<usize>,
    /// An attachment limit added after compiling (radius in inches).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attachment: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Rods {
    pub a: Vec<u32>,
    pub b: Vec<u32>,
    pub min: Vec<f32>,
    pub max: Vec<f32>,
    /// Share of a correction that moves node `a` (node `b` moves the rest).
    pub weight0: Vec<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Collider {
    pub bone: u32,
    /// Sphere centres in the bone's bind frame (inches), and the radius.
    pub ends: [[f64; 3]; 2],
    pub radius: f64,
    pub layers: u16,
}

/// How a moving bone is placed from its nodes: `y` = unit(Y1 − Y0), `x` = the
/// part of (X1 − X0) across y, made unit, `z` = x × y; rotation = that frame ·
/// `adjust`; position = node `target` − rotation · `offset`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Base {
    pub bone: u32,
    pub target: u32,
    pub offset: [f32; 3],
    pub x: [u32; 2],
    pub y: [u32; 2],
    pub adjust: [f32; 4],
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Packet {
    pub version: u32,
    pub source_sha256: String,
    pub proxy_sha256: String,
    pub bones: Vec<Bone>,
    /// Nodes `0..static_nodes` follow their bone exactly; the rest are simulated.
    pub static_nodes: usize,
    /// Bind position of every node in model space.
    pub positions: Vec<[f32; 3]>,
    /// The bone carrying each node (its animated place is that bone's
    /// animated transform applied to the bind position).
    pub node_bones: Vec<u32>,
    pub inv_mass: Vec<f32>,
    /// Collision layer mask of every node.
    pub layers: Vec<u16>,
    /// Motion values of each simulated node (index − `static_nodes`).
    pub motion: Vec<NodeMotion>,
    pub rods: Rods,
    pub colliders: Vec<Collider>,
    pub bases: Vec<Base>,
}

/// The predicted compiled cloth model: before (`native`) and after the build's
/// own changes (`built`), as FeModel values, and the packet.
pub struct Prediction {
    pub native: Value,
    pub built: Value,
    pub packet: Packet,
}

fn f(v: f64) -> Value { Value::new(V::F32(v as f32)) }
fn int(v: i64) -> Value { Value::new(V::I64(v)) }
fn arr(v: impl IntoIterator<Item = Value>) -> Value { Value::new(V::Array(v.into_iter().collect())) }
fn obj(rows: Vec<(&str, Value)>) -> Value { Value::new(V::Object(rows.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())) }
fn num(v: &Value) -> f64 { v.number().unwrap_or(f64::NAN) }
fn idx(v: &Value) -> usize { v.integer().map(|n| n as usize).unwrap_or(usize::MAX) }
fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] { [a[0] - b[0], a[1] - b[1], a[2] - b[2]] }
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 { a[0] * b[0] + a[1] * b[1] + a[2] * b[2] }
fn len(a: [f64; 3]) -> f64 { dot(a, a).sqrt() }
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] { [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]] }
fn unit(a: [f64; 3]) -> Result<[f64; 3]> {
    let l = len(a);
    if !(l > 1e-9) { return Err(invalid("Preview cloth: collapsed bone frame.")); }
    Ok(a.map(|v| v / l))
}
/// Rotation matrix (rows) to a quaternion x, y, z, w.
fn quat(m: [[f64; 3]; 3]) -> [f64; 4] {
    let t = m[0][0] + m[1][1] + m[2][2];
    let q = if t > 0. {
        let s = (t + 1.).sqrt() * 2.;
        [(m[2][1] - m[1][2]) / s, (m[0][2] - m[2][0]) / s, (m[1][0] - m[0][1]) / s, s / 4.]
    } else {
        let i = (0..3).max_by(|&a, &b| m[a][a].total_cmp(&m[b][b])).unwrap();
        let (j, k) = ((i + 1) % 3, (i + 2) % 3);
        let s = (1. + m[i][i] - m[j][j] - m[k][k]).max(0.).sqrt() * 2.;
        let mut q = [0.; 4];
        q[i] = s / 4.;
        q[j] = (m[j][i] + m[i][j]) / s;
        q[k] = (m[k][i] + m[i][k]) / s;
        q[3] = (m[k][j] - m[j][k]) / s;
        q
    };
    let n = q.iter().map(|v| v * v).sum::<f64>().sqrt();
    q.map(|v| v / n)
}
fn qmul(a: [f64; 4], b: [f64; 4]) -> [f64; 4] {
    [
        a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
        a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
        a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
        a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
    ]
}
fn conj(q: [f64; 4]) -> [f64; 4] { [-q[0], -q[1], -q[2], q[3]] }
/// Rotates `v` by the unit quaternion `q`.
pub fn rotate(q: [f64; 4], v: [f64; 3]) -> [f64; 3] {
    let u = [q[0], q[1], q[2]];
    let t = cross(u, v).map(|x| 2. * x);
    let c = cross(u, t);
    [v[0] + q[3] * t[0] + c[0], v[1] + q[3] * t[1] + c[1], v[2] + q[3] * t[2] + c[2]]
}
/// The frame of a base (see [`Base`]) from four node positions.
pub fn base_frame(x0: [f64; 3], x1: [f64; 3], y0: [f64; 3], y1: [f64; 3]) -> Result<[f64; 4]> {
    let y = unit(sub(y1, y0))?;
    let raw = sub(x1, x0);
    let x = unit(sub(raw, y.map(|v| v * dot(raw, y))))?;
    let z = cross(x, y);
    Ok(quat(std::array::from_fn(|r| [x[r], y[r], z[r]])))
}
/// A bone's placement from its base: rotation (x, y, z, w) and position.
pub fn place(base_q: [f64; 4], adjust: [f64; 4], target: [f64; 3], offset: [f64; 3]) -> ([f64; 4], [f64; 3]) {
    let q = qmul(base_q, adjust);
    (q, sub(target, rotate(q, offset)))
}

/// The proxy as the compiler reads it.
struct Proxy {
    positions: Vec<[f64; 3]>,
    bone: Vec<usize>,
    enabled: Vec<bool>,
    faces: Vec<[usize; 3]>,
    streams: BTreeMap<String, Vec<f64>>,
    skeleton: deadlock_skeleton::Skeleton,
}
fn read_proxy(bytes: &[u8]) -> Result<Proxy> {
    let d = Dmx::parse(bytes.to_vec())?;
    let binds = surface::binds(&d)?;
    let skeleton = binds.skeleton.clone();
    let index: BTreeMap<&str, usize> = skeleton.names.iter().enumerate().map(|(i, n)| (n.as_str(), i)).collect();
    let (state, vertex) = d.elements.iter().enumerate()
        .find(|(_, e)| e.kind == "DmeVertexData" && e.get("cloth_enable$0").is_some())
        .ok_or_else(|| invalid("Preview cloth: the proxy has no cloth state."))?;
    let raw = surface::floats(&d, vertex, "position$0", 42, 3)?;
    let positions: Vec<[f64; 3]> = raw.chunks_exact(3).map(|v| [v[0], v[1], v[2]]).collect();
    let count = positions.len();
    let corners = surface::ints(&d, vertex, "position$0Indices", 34)?;
    let slots = vertex.attributes.iter().find(|a| a.name == "jointCount")
        .and_then(|a| d.raw_attribute(a).try_into().ok().map(|b: [u8; 4]| i32::from_le_bytes(b) as usize))
        .ok_or_else(|| invalid("Preview cloth: the proxy has no joint count."))?;
    let joints = surface::ints(&d, vertex, "blendindices$0", 34)?;
    let weights = surface::floats(&d, vertex, "blendweights$0", 35, 1)?;
    if slots == 0 || joints.len() != count * slots || weights.len() != joints.len() {
        return Err(invalid("Preview cloth: proxy weights do not match its points."));
    }
    let mut bone = Vec::with_capacity(count);
    for i in 0..count {
        let k = (0..slots).max_by(|&a, &b| weights[i * slots + a].total_cmp(&weights[i * slots + b])).unwrap();
        let element = binds.palette.get(joints[i * slots + k] as usize).copied()
            .ok_or_else(|| invalid("Preview cloth: proxy joint is out of range."))?;
        bone.push(*index.get(d.elements[element].name.as_str()).ok_or_else(|| invalid("Preview cloth: proxy joint is not a bone."))?);
    }
    let mut streams = BTreeMap::new();
    for a in &vertex.attributes {
        if !a.name.starts_with("cloth_") || a.name.ends_with("Indices") { continue; }
        let values = surface::floats(&d, vertex, &a.name, 35, 1)?;
        let at = surface::ints(&d, vertex, &format!("{}Indices", a.name), 34)?;
        if at.len() != corners.len() { return Err(invalid("Preview cloth: a proxy stream does not match its corners.")); }
        let mut per = vec![0.; count];
        for (c, &p) in corners.iter().enumerate() {
            per[p as usize] = *values.get(at[c] as usize).ok_or_else(|| invalid("Preview cloth: proxy stream index is out of range."))?;
        }
        streams.insert(a.name.clone(), per);
    }
    let enabled = streams.get("cloth_enable$0").ok_or_else(|| invalid("Preview cloth: proxy has no cloth_enable stream."))?
        .iter().map(|v| *v > 0.5).collect();
    let mesh = d.elements.iter()
        .find(|e| e.kind == "DmeMesh" && surface::reference(&d, e, "bindState", "DmeVertexData").ok() == Some(state))
        .ok_or_else(|| invalid("Preview cloth: the proxy has no mesh."))?;
    let mut faces = vec![];
    for set in surface::references(&d, mesh, "faceSets")? {
        let mut poly = vec![];
        for c in surface::ints(&d, &d.elements[set], "faces", 34)? {
            if c >= 0 { poly.push(corners[c as usize] as usize); continue; }
            if poly.len() != 3 { return Err(invalid("Preview cloth: the proxy has a face that is not a triangle.")); }
            faces.push([poly[0], poly[1], poly[2]]);
            poly.clear();
        }
    }
    Ok(Proxy { positions, bone, enabled, faces, streams, skeleton })
}

/// Predicts the compiled cloth model of an automatic-motion setup and builds
/// the packet. `None` for setups that do not author their own proxy (the
/// bundled presets).
pub fn predict(setup: &Setup) -> Result<Option<Prediction>> {
    if setup.preset != "automatic" || setup.reused_accepted_reference || setup.proxy.is_empty() {
        return Ok(None);
    }
    let p = read_proxy(&setup.proxy)?;
    if setup.preview_points.len() != p.positions.len() {
        return Err(invalid("Preview cloth: point motions do not match the proxy."));
    }
    let sk = &p.skeleton;
    let state = transforms::bind_state(sk)?;
    let bind = |i: usize| -> [f64; 8] {
        let s = state.accumulated[i];
        let w = &state.world[i];
        let q = quat(std::array::from_fn(|r| std::array::from_fn(|c| w[r][c] / s)));
        [w[0][3], w[1][3], w[2][3], s, q[0], q[1], q[2], q[3]]
    };
    let n = p.positions.len();
    let key = |a: usize, b: usize| if a < b { (a, b) } else { (b, a) };
    // Edges, each with its faces in face order.
    let mut edges: BTreeMap<(usize, usize), Vec<usize>> = BTreeMap::new();
    let mut edge_order = vec![];
    for (fi, t) in p.faces.iter().enumerate() {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            let k = key(a, b);
            let entry = edges.entry(k).or_insert_with(|| { edge_order.push(k); vec![] });
            entry.push(fi);
        }
    }
    let mut neighbours = vec![BTreeSet::new(); n];
    for &(a, b) in &edge_order { neighbours[a].insert(b); neighbours[b].insert(a); }
    // Node order.
    let kept_static: Vec<usize> = (0..n).filter(|&i| !p.enabled[i] && neighbours[i].iter().any(|&j| p.enabled[j])).collect();
    let mut level = vec![usize::MAX; n];
    let mut queue = VecDeque::new();
    for i in 0..n { if !p.enabled[i] { level[i] = 0; queue.push_back(i); } }
    while let Some(a) = queue.pop_front() {
        for &b in &neighbours[a] { if level[b] == usize::MAX { level[b] = level[a] + 1; queue.push_back(b); } }
    }
    let mut dynamic: Vec<usize> = (0..n).filter(|&i| p.enabled[i]).collect();
    dynamic.sort_by_key(|&i| (level[i], i));
    // A group bone that carries no moving point is not part of the compiled
    // cloth model: it simply follows its parent bone.
    let carried: BTreeSet<usize> = dynamic.iter().map(|&i| p.bone[i]).collect();
    let driven: Vec<usize> = setup.simulated_groups().flat_map(|g| g.bones.iter())
        .map(|b| sk.indices().get(b.as_str()).copied().ok_or_else(|| invalid(format!("Preview cloth: moving bone is missing: {b}"))))
        .collect::<Result<Vec<_>>>()?
        .into_iter().filter(|b| carried.contains(b)).collect();
    let driven_set: BTreeSet<usize> = driven.iter().copied().collect();
    let mut carriers = vec![];
    for &i in &kept_static {
        let b = p.bone[i];
        if !carriers.contains(&b) && !driven_set.contains(&b) { carriers.push(b); }
    }
    let mut collider_only = vec![];
    for c in &setup.body_colliders {
        let b = *sk.indices().get(c.bone.as_str()).ok_or_else(|| invalid("Preview cloth: collider bone is missing."))?;
        if !carriers.contains(&b) && !collider_only.contains(&b) && !driven_set.contains(&b) { collider_only.push(b); }
    }
    // Controls: (name, kind) with kind Point(index) or Bone(index).
    enum Ctrl { Point(usize), Bone(usize) }
    let mut ctrls: Vec<Ctrl> = kept_static.iter().map(|&i| Ctrl::Point(i)).collect();
    ctrls.extend(carriers.iter().chain(&collider_only).map(|&b| Ctrl::Bone(b)));
    let static_count = ctrls.len();
    ctrls.extend(dynamic.iter().map(|&i| Ctrl::Point(i)));
    let first_driven = ctrls.len();
    ctrls.extend(driven.iter().map(|&b| Ctrl::Bone(b)));
    let mut node_of_point = vec![usize::MAX; n];
    let mut ctrl_of_bone = BTreeMap::new();
    for (c, ctrl) in ctrls.iter().enumerate() {
        match *ctrl { Ctrl::Point(i) => node_of_point[i] = c, Ctrl::Bone(b) => { ctrl_of_bone.insert(b, c); } }
    }
    let parent_ctrl = |b: usize, allowed: &dyn Fn(usize) -> bool| -> i64 {
        let mut at = sk.parents[b];
        while at >= 0 {
            if allowed(at as usize) { return ctrl_of_bone[&(at as usize)] as i64; }
            at = sk.parents[at as usize];
        }
        -1
    };
    let carrier_set: BTreeSet<usize> = carriers.iter().copied().collect();
    let mut names = vec![];
    let mut poses = vec![];
    let mut parents = vec![];
    let mut inv_mass = vec![];
    let point_bind = |i: usize| -> [f64; 8] { let b = bind(p.bone[i]); let x = p.positions[i]; [x[0], x[1], x[2], 1., b[4], b[5], b[6], b[7]] };
    // Inverse masses of moving points.
    let mut edge_length = vec![0.; n];
    for &(a, b) in &edge_order { let l = len(sub(p.positions[a], p.positions[b])); edge_length[a] += l; edge_length[b] += l; }
    let point_inv = |i: usize| -> f64 { if p.enabled[i] { 1. / (MASS_PER_EDGE_INCH * edge_length[i]) } else { 0. } };
    for (c, ctrl) in ctrls.iter().enumerate() {
        match *ctrl {
            Ctrl::Point(i) => {
                names.push(format!("$cloth_m0p{i}"));
                poses.push(point_bind(i));
                parents.push(ctrl_of_bone.get(&p.bone[i]).map_or(-1, |&c| c as i64));
                inv_mass.push((point_inv(i) as f32) as f64);
            }
            Ctrl::Bone(b) => {
                names.push(sk.names[b].clone());
                poses.push(bind(b));
                parents.push(if c >= first_driven {
                    parent_ctrl(b, &|a| carrier_set.contains(&a) || driven_set.contains(&a))
                } else if carrier_set.contains(&b) {
                    parent_ctrl(b, &|a| carrier_set.contains(&a))
                } else { -1 });
                inv_mass.push(if c >= first_driven { 1. } else { 0. });
            }
        }
    }
    let pos = |c: usize| [poses[c][0], poses[c][1], poses[c][2]];
    // Rods.
    let mut rods: Vec<(usize, usize, f64, f64, f64)> = vec![];
    for &(a, b) in &edge_order {
        if !p.enabled[a] && !p.enabled[b] { continue; }
        let (na, nb) = { let (x, y) = (node_of_point[a], node_of_point[b]); if x < y { (x, y) } else { (y, x) } };
        let l = len(sub(p.positions[a], p.positions[b]));
        let w = if na < static_count { 0. } else { 0.5 };
        rods.push((na, nb, l, l, w));
    }
    let mut bends: BTreeMap<(usize, usize), (f64, f64)> = BTreeMap::new();
    let mut bend_order = vec![];
    for &(x, y) in &edge_order {
        let faces = &edges[&(x, y)];
        for pair in faces.chunks(2) {
            if pair.len() != 2 { continue; }
            let opposite = |f: usize| p.faces[f].into_iter().find(|&v| v != x && v != y);
            let (Some(a), Some(b)) = (opposite(pair[0]), opposite(pair[1])) else { continue };
            if a == b || (!p.enabled[a] && !p.enabled[b]) || edges.contains_key(&key(a, b)) { continue; }
            if node_of_point[a] == usize::MAX || node_of_point[b] == usize::MAX { continue; }
            let e = sub(p.positions[y], p.positions[x]);
            let el = len(e);
            if el <= 1e-12 { continue; }
            let u = e.map(|v| v / el);
            let (da, db) = (sub(p.positions[a], p.positions[x]), sub(p.positions[b], p.positions[x]));
            let (ta, tb) = (dot(da, u), dot(db, u));
            let ha = len(sub(da, u.map(|v| v * ta)));
            let hb = len(sub(db, u.map(|v| v * tb)));
            let fold = ((ta - tb).powi(2) + (ha - hb).powi(2)).sqrt();
            let flat = ((ta - tb).powi(2) + (ha + hb).powi(2)).sqrt();
            let k = key(a, b);
            match bends.get_mut(&k) {
                Some(v) => { v.0 = v.0.min(fold); v.1 = v.1.max(flat); }
                None => { bends.insert(k, (fold, flat)); bend_order.push(k); }
            }
        }
    }
    for k in bend_order {
        let (fold, flat) = bends[&k];
        let (x, y) = (node_of_point[k.0], node_of_point[k.1]);
        let (na, nb) = if x < y { (x, y) } else { (y, x) };
        let (ia, ib) = (inv_mass[na], inv_mass[nb]);
        let w = if ia + ib > 0. { ia / (ia + ib) } else { 0. };
        rods.push((na, nb, fold, flat, w));
    }
    // Per-node values from the authored (tuned) streams, as the compiler reads them.
    let stream = |name: &str, i: usize| p.streams.get(name).map_or(0., |v| v[i]);
    let mut integrators = vec![];
    let mut strays = vec![];
    for (c, ctrl) in ctrls.iter().enumerate() {
        let (att, damp, grav) = match *ctrl {
            Ctrl::Point(i) if c >= static_count => {
                let g = game_values(stream("cloth_goal_strength$0", i), stream("cloth_drag$0", i), stream("cloth_gravity$0", i) / GAME_GRAVITY, stream("cloth_stray_radius$0", i));
                if g.travel > 0. { strays.push((c, g.travel)); }
                (g.attraction, g.damping, stream("cloth_gravity$0", i))
            }
            _ => (0., 0., 0.),
        };
        integrators.push(obj(vec![("flAnimationForceAttraction", f(att)), ("flAnimationVertexAttraction", f(att)), ("flGravity", f(grav)), ("flPointDamping", f(damp))]));
    }
    let layers: Vec<u16> = (0..ctrls.len()).map(|c| match ctrls[c] {
        Ctrl::Point(i) => (0..4).fold(ALL_LAYERS, |m, l| match p.streams.get(&format!("cloth_collision_layer_{l}$0")) {
            Some(v) if v[i] < 0.5 => m & !(1 << l),
            _ => m,
        }),
        Ctrl::Bone(_) => ALL_LAYERS,
    }).collect();
    // Bone placement: each moving bone from its first three points.
    let mut bases = vec![];
    let mut reverse = vec![];
    for c in first_driven..ctrls.len() {
        let Ctrl::Bone(b) = ctrls[c] else { continue };
        let mut ring: Vec<usize> = dynamic.iter().copied().filter(|&i| p.bone[i] == b).collect();
        ring.sort();
        if ring.len() < 3 { return Err(invalid(format!("Preview cloth: {} has fewer than three moving points.", sk.names[b]))); }
        let r = [node_of_point[ring[0]], node_of_point[ring[1]], node_of_point[ring[2]]];
        let (x, y, target) = ([r[1], r[0]], [r[2], r[1]], r[0]);
        let frame = base_frame(pos(x[0]), pos(x[1]), pos(y[0]), pos(y[1]))?;
        let q = [poses[c][4], poses[c][5], poses[c][6], poses[c][7]];
        let adjust = qmul(conj(frame), q);
        let offset = rotate(conj(q), sub(pos(target), pos(c)));
        bases.push(obj(vec![
            ("nNode", int(c as i64)), ("nDummy", arr([int(0), int(0), int(0)])),
            ("nNodeX0", int(x[0] as i64)), ("nNodeX1", int(x[1] as i64)), ("nNodeY0", int(y[0] as i64)), ("nNodeY1", int(y[1] as i64)),
            ("qAdjust", arr(adjust.map(f))),
        ]));
        reverse.push(obj(vec![("nBoneCtrl", int(c as i64)), ("nTargetNode", int(target as i64)), ("vOffset", arr(offset.map(f)))]));
    }
    let offsets: Vec<Value> = (0..first_driven).filter_map(|c| match ctrls[c] {
        Ctrl::Point(i) => {
            let pc = ctrl_of_bone.get(&p.bone[i])?;
            let pb = poses[*pc];
            let local = rotate(conj([pb[4], pb[5], pb[6], pb[7]]), sub(pos(c), pos(*pc))).map(|v| v / pb[3]);
            Some(obj(vec![("nCtrlParent", int(*pc as i64)), ("nCtrlChild", int(c as i64)), ("vOffset", arr(local.map(f)))]))
        }
        _ => None,
    }).collect();
    let rod_value = |r: &(usize, usize, f64, f64, f64)| obj(vec![
        ("nNode", arr([int(r.0 as i64), int(r.1 as i64)])), ("flMaxDist", f(r.3)), ("flMinDist", f(r.2)),
        ("flWeight0", f(r.4)), ("flRelaxationFactor", f(1.)),
    ]);
    let simd_rods: Vec<Value> = rods.iter().map(|r| obj(vec![
        ("nNode", arr([arr((0..4).map(|_| int(r.0 as i64))), arr((0..4).map(|_| int(r.1 as i64)))])),
        ("f4MaxDist", arr((0..4).map(|_| f(r.3)))), ("f4MinDist", arr((0..4).map(|_| f(r.2)))),
        ("f4Weight0", arr((0..4).map(|_| f(r.4)))), ("f4RelaxationFactor", arr((0..4).map(|_| f(1.)))),
    ])).collect();
    let stray_rows: Vec<Value> = strays.iter().map(|&(c, t)| obj(vec![("flMaxDist", f(t)), ("flRelaxationFactor", f(1.)), ("nNode", arr([int(c as i64), int(c as i64)]))])).collect();
    let simd_strays: Vec<Value> = strays.chunks(4).map(|chunk| {
        let lane = |k: usize| chunk[k.min(chunk.len() - 1)];
        obj(vec![
            ("flMaxDist", arr((0..4).map(|k| f(lane(k).1)))), ("flRelaxationFactor", arr((0..4).map(|_| f(1.)))),
            ("nNode", arr([arr((0..4).map(|k| int(lane(k).0 as i64))), arr((0..4).map(|k| int(lane(k).0 as i64)))])),
        ])
    }).collect();
    let capsules: Vec<Value> = setup.body_colliders.iter().map(|c| {
        let b = sk.indices()[c.bone.as_str()];
        obj(vec![
            ("nNode", int(ctrl_of_bone[&b] as i64)), ("nCollisionMask", int(CAPSULE_LAYERS as i64)),
            ("vSphere", arr(c.endpoints_local.iter().map(|e| arr([f(e[0]), f(e[1]), f(e[2]), f(c.radius_local)])))),
        ])
    }).collect();
    let native = obj(vec![
        ("m_nNodeCount", int(ctrls.len() as i64)),
        ("m_nStaticNodes", int(static_count as i64)),
        ("m_nRotLockStaticNodes", int(static_count as i64)),
        ("m_nFirstPositionDrivenNode", int(first_driven as i64)),
        ("m_CtrlName", arr(names.iter().map(|s| Value::new(V::String(s.clone()))))),
        ("m_InitPose", arr(poses.iter().map(|q| arr(q.map(f))))),
        ("m_SkelParents", arr(parents.iter().map(|&v| int(v)))),
        ("m_NodeInvMasses", arr(inv_mass.iter().map(|&v| f(v)))),
        ("m_CtrlOffsets", arr(offsets)),
        ("m_NodeIntegrator", arr(integrators)),
        ("m_Rods", arr(rods.iter().map(rod_value))),
        ("m_SimdRods", arr(simd_rods)),
        ("m_AnimStrayRadii", arr(stray_rows)),
        ("m_SimdAnimStrayRadii", arr(simd_strays)),
        ("m_NodeBases", arr(bases)),
        ("m_SimdNodeBases", arr([])),
        ("m_ReverseOffsets", arr(reverse)),
        ("m_TaperedCapsuleRigids", arr(capsules)),
        ("m_TreeCollisionMasks", arr(layers[static_count..].iter().map(|&m| int(m as i64)))),
    ]);
    // The build's own changes after compiling, by the build's functions.
    let patch = deadlock_cloth::generated::roll_lock::apply(&native, sk, &setup.roll_lock_chains)?;
    let (built, sheet) = deadlock_cloth::generated::sheet_stability::apply(&patch.fe, &setup.fabric_stability())?;
    let attachments: BTreeMap<usize, f64> = sheet.attachments.bones.iter()
        .flat_map(|b| b.points.iter().map(move |&p| (p, b.radius_inches))).collect();
    // The packet, from the built model.
    let get = |k: &str| -> Result<Vec<Value>> { Ok(built.get(k).ok_or_else(|| invalid(format!("Preview cloth: {k} is missing.")))?.array()?.to_vec()) };
    let mut bones: Vec<usize> = carriers.iter().chain(&collider_only).chain(&driven).copied().collect();
    bones.dedup();
    let bone_index: BTreeMap<usize, u32> = bones.iter().enumerate().map(|(i, &b)| (b, i as u32)).collect();
    if let Some(c) = (0..ctrls.len()).find(|&c| !bone_index.contains_key(&match ctrls[c] { Ctrl::Point(i) => p.bone[i], Ctrl::Bone(b) => b })) {
        return Err(invalid(format!("Preview cloth: node {} is carried by a bone outside the packet.", names[c])));
    }
    let bone_of_ctrl = |c: usize| -> u32 { match ctrls[c] { Ctrl::Point(i) => bone_index[&p.bone[i]], Ctrl::Bone(b) => bone_index[&b] } };
    let node_count = first_driven;
    let mut out_rods = Rods::default();
    for r in get("m_Rods")? {
        let nodes = r.get("nNode").map(|v| v.array().map(|a| a.to_vec())).transpose()?.unwrap_or_default();
        let (a, b) = (idx(&nodes[0]), idx(&nodes[1]));
        out_rods.a.push(a as u32); out_rods.b.push(b as u32);
        out_rods.min.push(num(r.get("flMinDist").unwrap()) as f32);
        out_rods.max.push(num(r.get("flMaxDist").unwrap()) as f32);
        out_rods.weight0.push(num(r.get("flWeight0").unwrap()) as f32);
    }
    let out_bases = get("m_NodeBases")?.iter().map(|b| {
        let c = idx(b.get("nNode").unwrap());
        let r = get("m_ReverseOffsets")?.into_iter().find(|r| idx(r.get("nBoneCtrl").unwrap()) == c)
            .ok_or_else(|| invalid("Preview cloth: a moving bone has no position node."))?;
        let three = |v: &Value| -> Result<Vec<f64>> { Ok(v.array()?.iter().map(num).collect()) };
        let o = three(r.get("vOffset").unwrap())?;
        let q = three(b.get("qAdjust").unwrap())?;
        Ok(Base {
            bone: bone_of_ctrl(c),
            target: idx(r.get("nTargetNode").unwrap()) as u32,
            offset: [o[0] as f32, o[1] as f32, o[2] as f32],
            x: [idx(b.get("nNodeX0").unwrap()) as u32, idx(b.get("nNodeX1").unwrap()) as u32],
            y: [idx(b.get("nNodeY0").unwrap()) as u32, idx(b.get("nNodeY1").unwrap()) as u32],
            adjust: [q[0] as f32, q[1] as f32, q[2] as f32, q[3] as f32],
        })
    }).collect::<Result<Vec<_>>>()?;
    let preview_node = |i: usize| setup.preview_nodes.iter().position(|v| v.bone == sk.names[p.bone[i]]);
    let packet = Packet {
        version: VERSION,
        source_sha256: setup.source_sha256.clone(),
        proxy_sha256: setup.proxy_sha256.clone(),
        bones: bones.iter().map(|&b| Bone { name: sk.names[b].clone(), bind: bind(b) }).collect(),
        static_nodes: static_count,
        positions: (0..node_count).map(|c| pos(c).map(|v| v as f32)).collect(),
        node_bones: (0..node_count).map(bone_of_ctrl).collect(),
        inv_mass: inv_mass[..node_count].iter().map(|&v| v as f32).collect(),
        layers: layers[..node_count].to_vec(),
        motion: (static_count..node_count).map(|c| {
            let Ctrl::Point(i) = ctrls[c] else { unreachable!() };
            NodeMotion { motion: setup.preview_points[i].clone(), preview_node: preview_node(i), attachment: attachments.get(&c).copied() }
        }).collect(),
        rods: out_rods,
        colliders: setup.body_colliders.iter().map(|c| Collider {
            bone: bone_index[&sk.indices()[c.bone.as_str()]],
            ends: c.endpoints_local,
            radius: c.radius_local,
            layers: CAPSULE_LAYERS,
        }).collect(),
        bases: out_bases,
    };
    Ok(Some(Prediction { native, built, packet }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Deserialize)]
    struct Case { goal: f64, drag: f64, gravity: f64, travel: f64, attraction: f64, damping: f64, game_gravity: f64, game_travel: f64 }
    /// The shared fixture the movement preview's TypeScript checks against.
    #[test]
    fn game_values_match_the_shared_fixture() {
        let cases: Vec<Case> = serde_json::from_str(include_str!("../tests/fixtures/preview-cloth-game-values.json")).unwrap();
        assert!(cases.len() >= 8);
        for c in cases {
            let v = game_values(c.goal, c.drag, c.gravity, c.travel);
            assert!((v.attraction - c.attraction).abs() < 1e-7, "{c:?}", c = (c.goal, v.attraction));
            assert!((v.damping - c.damping).abs() < 1e-5);
            assert!((v.gravity - c.game_gravity).abs() < 1e-4);
            assert!((v.travel - c.game_travel).abs() < 1e-7);
        }
    }
    #[test]
    fn held_and_attachment_caps_follow_the_build() {
        let m = PointMotion { kind: Some(Kind::Hair), goal: 1., drag: 0., gravity: 0., travel: 0.2, caps: vec![Cap::Held] };
        let swing = MotionTuning { swing: Some(1.), ..Default::default() };
        assert!((node_values(&m, Some(&swing), None).travel - 0.03).abs() < 1e-7);
        let free = PointMotion { travel: 0., caps: vec![], ..m.clone() };
        let v = node_values(&free, None, Some(0.4));
        assert_eq!((v.attraction, v.damping), (0.5, ATTACHMENT_DAMPING));
        assert!((v.travel - 0.4).abs() < 1e-6);
    }
}
