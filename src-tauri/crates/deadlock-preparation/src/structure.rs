//! Structural humanoid recognition for rigs whose bone names carry no usable
//! anatomy (generic `Bone.001` chains, Rigify DEF layers, DAZ, Character
//! Creator, hand-made rigs, translated names ...).
//!
//! Roles come from the skin skeleton's topology and rest positions: the two
//! leg branches reach the floor, the pelvis is their common ancestor with the
//! head, the arms are the laterally extreme branches of the spine, and limb
//! joints are chosen by segment proportions. Left/right comes from side tokens
//! in the names when they agree, otherwise from the toes' forward direction.
//! Every proposal is re-checked by the ordinary `identify` hierarchy, side and
//! spacing tests; nothing here bypasses review or conversion validation.
use crate::algorithms::{self, Bone, ROLES};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const POLICY: &str = "structural-humanoid-detection-v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Detection {
    pub policy: String,
    pub roles: BTreeMap<String, String>,
    /// Source-space direction that points from the feet to the head.
    pub up: [f64; 3],
    /// Euler XYZ rotation in degrees that turns `up` into +Z.
    pub rotation: [f64; 3],
    pub sides_from: String,
    pub notes: Vec<String>,
}

const SECONDARY: [&str; 22] = [
    "hair", "ponytail", "braid", "skirt", "cloth", "cape", "coat", "robe", "tail", "ribbon", "streamer",
    "breast", "bust", "chain", "髪", "裙", "スカート", "尻尾", "リボン", "胸", "tie", "belt",
];
const HELPER: [&str; 9] = ["twist", "roll", "helper", "corrective", "adjust", "bend_", "捩", "_ty_", "_tz_"];

fn lower_name(name: &str) -> String {
    name.rsplit(':').next().unwrap_or(name).to_lowercase()
}
fn secondary(name: &str) -> bool {
    let n = lower_name(name);
    SECONDARY.iter().any(|s| n.contains(s))
}
fn helper(name: &str) -> bool {
    let n = lower_name(name);
    HELPER.iter().any(|s| n.contains(s))
}

/// Side from naming tokens: `.L`, `_l`, `L_`, `Left`, DAZ `lShldr`, Biped `Bip01 L`, 左/右.
pub fn name_side(name: &str) -> Option<char> {
    let original = name.rsplit(':').next().unwrap_or(name);
    if original.starts_with('左') || original.contains("左") && !original.contains("右") {
        return Some('L');
    }
    if original.starts_with('右') || original.contains("右") && !original.contains("左") {
        return Some('R');
    }
    // DAZ / camel case: lShldrBend, rForearm.
    let mut chars = original.chars();
    if let (Some(first), Some(second)) = (chars.next(), chars.next()) {
        if second.is_ascii_uppercase() {
            if first == 'l' {
                return Some('L');
            }
            if first == 'r' {
                return Some('R');
            }
        }
    }
    let lower = original.to_lowercase();
    let mut left = lower.contains("left");
    let mut right = lower.contains("right");
    let tokens: Vec<&str> = lower
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|t| !t.is_empty())
        .collect();
    for token in &tokens {
        match *token {
            "l" | "lt" | "lft" | "lf" => left = true,
            "r" | "rt" | "rgt" | "rg" => right = true,
            _ => {}
        }
    }
    // Trailing single letter after digits/words: "armL", "Hand_L0" handled above;
    // "upperarml" style compacts are ambiguous and ignored on purpose.
    match (left, right) {
        (true, false) => Some('L'),
        (false, true) => Some('R'),
        _ => None,
    }
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn norm(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}
fn horizontal(a: [f64; 3]) -> [f64; 3] {
    [a[0], a[1], 0.]
}

/// Rotation (row-major) that maps `up` onto +Z, using quarter turns only.
fn rotation_for(up: [f64; 3]) -> ([[f64; 3]; 3], [f64; 3]) {
    let candidates: [([f64; 3], [[f64; 3]; 3], [f64; 3]); 6] = [
        ([0., 0., 1.], [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]], [0., 0., 0.]),
        // +Y up -> rotate +90 about X: (x, y, z) -> (x, -z, y)
        ([0., 1., 0.], [[1., 0., 0.], [0., 0., -1.], [0., 1., 0.]], [90., 0., 0.]),
        ([0., -1., 0.], [[1., 0., 0.], [0., 0., 1.], [0., -1., 0.]], [-90., 0., 0.]),
        // +X up -> rotate -90 about Y: (x, y, z) -> (-z, y, x)
        ([1., 0., 0.], [[0., 0., -1.], [0., 1., 0.], [1., 0., 0.]], [0., -90., 0.]),
        ([-1., 0., 0.], [[0., 0., 1.], [0., 1., 0.], [-1., 0., 0.]], [0., 90., 0.]),
        ([0., 0., -1.], [[1., 0., 0.], [0., -1., 0.], [0., 0., -1.]], [180., 0., 0.]),
    ];
    for (axis, matrix, euler) in candidates {
        if dot(axis, up) > 0.999 {
            return (matrix, euler);
        }
    }
    (candidates[0].1, candidates[0].2)
}
fn apply(m: &[[f64; 3]; 3], p: [f64; 3]) -> [f64; 3] {
    [dot(m[0], p), dot(m[1], p), dot(m[2], p)]
}
pub const UP_CANDIDATES: [[f64; 3]; 6] = [
    [0., 0., 1.],
    [0., 1., 0.],
    [0., -1., 0.],
    [1., 0., 0.],
    [-1., 0., 0.],
    [0., 0., -1.],
];

/// Bones rotated so that `up` becomes +Z; skin centroids/bounds rotate too.
pub fn rotated(bones: &[Bone], up: [f64; 3]) -> Vec<Bone> {
    let (m, _) = rotation_for(up);
    bones
        .iter()
        .map(|b| {
            let mut copy = b.clone();
            copy.position = apply(&m, b.position);
            copy
        })
        .collect()
}

struct Tree<'a> {
    bones: &'a [Bone],
    parent: Vec<Option<usize>>,
    children: Vec<Vec<usize>>,
    pos: Vec<[f64; 3]>,
    skin: Vec<f64>,
    weighted: Vec<bool>,
    in_skin: Vec<bool>,
    deform: Vec<bool>,
}

impl<'a> Tree<'a> {
    fn new(bones: &'a [Bone], weighted: &BTreeSet<&str>, positions: Vec<[f64; 3]>) -> Option<Self> {
        let index: BTreeMap<&str, usize> = bones.iter().enumerate().map(|(i, b)| (b.name.as_str(), i)).collect();
        if index.len() != bones.len() {
            return None;
        }
        let parent: Vec<Option<usize>> = bones
            .iter()
            .map(|b| b.parent.as_deref().and_then(|p| index.get(p).copied()))
            .collect();
        let mut children = vec![vec![]; bones.len()];
        for (i, p) in parent.iter().enumerate() {
            if let Some(p) = p {
                children[*p].push(i);
            }
        }
        // Reject cycles before any upward walk.
        for start in 0..bones.len() {
            let mut seen = BTreeSet::new();
            let mut cursor = Some(start);
            while let Some(i) = cursor {
                if !seen.insert(i) {
                    return None;
                }
                cursor = parent[i];
            }
        }
        let weighted_flags: Vec<bool> = bones.iter().map(|b| weighted.contains(b.name.as_str())).collect();
        let mut in_skin = weighted_flags.clone();
        for i in 0..bones.len() {
            if weighted_flags[i] {
                let mut cursor = parent[i];
                while let Some(p) = cursor {
                    if in_skin[p] && !weighted_flags[p] {
                        // already marked through another branch; keep walking for safety
                    }
                    in_skin[p] = true;
                    cursor = parent[p];
                }
            }
        }
        let skin = bones
            .iter()
            .map(|b| {
                b.details
                    .get("skin")
                    .and_then(|s| s.get("vertices"))
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.)
            })
            .collect();
        let deform = bones
            .iter()
            .map(|b| b.details.get("deform").and_then(|v| v.as_bool()) != Some(false))
            .collect();
        if positions.iter().flatten().any(|v| !v.is_finite()) {
            return None;
        }
        Some(Self { bones, parent, children, pos: positions, skin, weighted: weighted_flags, in_skin, deform })
    }
    fn name(&self, i: usize) -> &str {
        &self.bones[i].name
    }
    fn ancestors(&self, i: usize) -> Vec<usize> {
        let mut out = vec![i];
        let mut cursor = self.parent[i];
        while let Some(p) = cursor {
            out.push(p);
            cursor = self.parent[p];
        }
        out
    }
    fn is_ancestor(&self, a: usize, b: usize) -> bool {
        let mut cursor = Some(b);
        while let Some(i) = cursor {
            if i == a {
                return true;
            }
            cursor = self.parent[i];
        }
        false
    }
    fn lca(&self, nodes: &[usize]) -> Option<usize> {
        let first = nodes.first()?;
        let mut common: Vec<usize> = self.ancestors(*first);
        for n in &nodes[1..] {
            let set: BTreeSet<usize> = self.ancestors(*n).into_iter().collect();
            common.retain(|a| set.contains(a));
        }
        common.first().copied()
    }
    /// Path from ancestor `a` down to descendant `b`, inclusive.
    fn path(&self, a: usize, b: usize) -> Option<Vec<usize>> {
        let mut out = vec![];
        let mut cursor = Some(b);
        while let Some(i) = cursor {
            out.push(i);
            if i == a {
                out.reverse();
                return Some(out);
            }
            cursor = self.parent[i];
        }
        None
    }
    fn subtree(&self, root: usize) -> Vec<usize> {
        let mut out = vec![];
        let mut pending = vec![root];
        while let Some(i) = pending.pop() {
            out.push(i);
            pending.extend(self.children[i].iter().copied());
        }
        out
    }
    fn skin_children(&self, i: usize) -> Vec<usize> {
        self.children[i].iter().copied().filter(|c| self.in_skin[*c]).collect()
    }
}

struct Frame {
    floor: f64,
    height: f64,
}

fn frame(tree: &Tree) -> Option<Frame> {
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for (i, p) in tree.pos.iter().enumerate() {
        if tree.weighted[i] {
            lo = lo.min(p[2]);
            hi = hi.max(p[2]);
        }
    }
    let height = hi - lo;
    (height.is_finite() && height > 1e-6).then_some(Frame { floor: lo, height })
}

/// Choose three joints (upper, middle, end) on a limb path by proportions.
/// Returns the plausibility cost (lower is better) with the chosen joints.
fn limb(tree: &Tree, path: &[usize], tip: usize, h: f64, leg: bool, origin: [f64; 3]) -> Option<(f64, [usize; 3])> {
    let (ratio, total, end_lo, end_hi) = if leg { (1.0, 0.47, 0.0, 0.2) } else { (1.12, 0.32, 0.0, 0.17) };
    let mut best: Option<(f64, [usize; 3])> = None;
    let n = path.len();
    for i in 0..n {
        for j in i + 1..n {
            for k in j + 1..n {
                let (a, b, c) = (tree.pos[path[i]], tree.pos[path[j]], tree.pos[path[k]]);
                let s1 = norm(sub(b, a));
                let s2 = norm(sub(c, b));
                if s1 < h * 0.03 || s2 < h * 0.03 {
                    continue;
                }
                if leg && !(a[2] > b[2] && b[2] > c[2]) {
                    continue;
                }
                let end = norm(sub(tree.pos[tip], c)) / h;
                let mut cost = 3. * ((s1 / s2) / ratio).ln().powi(2);
                // Shoulders and hip sockets sit away from the body's centre line;
                // clavicle roots and hip helpers do not.
                let offset = norm(horizontal(sub(a, origin))) / h;
                if offset < if leg { 0.02 } else { 0.04 } {
                    cost += 1.5;
                }
                cost += (((s1 + s2) / h - total) / 0.12).powi(2);
                let clamped = end.clamp(end_lo, end_hi);
                cost += ((end - clamped) / 0.04).powi(2);
                for (index, role) in [(path[i], 0usize), (path[j], 1), (path[k], 2)] {
                    if helper(tree.name(index)) {
                        cost += 1.5;
                    }
                    if !tree.weighted[index] {
                        cost += 0.6;
                    }
                    if !tree.deform[index] {
                        cost += 0.4;
                    }
                    let name_role = algorithms::name_role(tree.name(index)).unwrap_or("");
                    let expected = if leg { ["thigh", "shin", "foot"][role] } else { ["upperarm", "forearm", "hand"][role] };
                    if name_role.starts_with(expected) {
                        cost -= 0.8;
                    }
                }
                if !leg && tree.skin_children(path[k]).len() >= 2 {
                    cost -= 0.4; // finger branching at the wrist
                }
                if best.as_ref().is_none_or(|(c, _)| cost < *c) {
                    best = Some((cost, [path[i], path[j], path[k]]));
                }
            }
        }
    }
    best
}

struct Hypothesis {
    cost: f64,
    roles: BTreeMap<String, String>,
    sides_from: String,
    notes: Vec<String>,
}

fn need<T>(value: Option<T>, message: &str) -> Result<T, String> {
    value.ok_or_else(|| message.to_string())
}

/// Try every plausible pair of floor-contact chains as the feet; keep the
/// anatomically most plausible complete hypothesis. Long skirts or hair that
/// reach the floor are rejected because no arms/spine fit around them.
fn solve(tree: &Tree) -> Result<Hypothesis, String> {
    let Frame { floor, height: h } = need(frame(tree), "the skin skeleton has no vertical extent")?;
    let low: BTreeSet<usize> = (0..tree.bones.len())
        .filter(|i| tree.weighted[*i] && tree.pos[*i][2] < floor + 0.18 * h)
        .collect();
    let mut tips: Vec<usize> = low
        .iter()
        .copied()
        .filter(|i| tree.children[*i].iter().filter(|c| tree.weighted[**c]).all(|c| low.contains(c)))
        .collect();
    tips.sort_by(|a, b| {
        let named = |i: usize| u8::from(secondary(tree.name(i)));
        named(*a).cmp(&named(*b)).then(tree.pos[*a][2].total_cmp(&tree.pos[*b][2]))
    });
    tips.truncate(32);
    if tips.len() < 2 {
        return Err("fewer than two skin joints reach the floor".into());
    }
    let mut best: Option<Hypothesis> = None;
    let mut reason = String::from("two separate legs reaching the floor were not found");
    for x in 0..tips.len() {
        for y in x + 1..tips.len() {
            let (a, b) = (tips[x], tips[y]);
            if norm(horizontal(sub(tree.pos[a], tree.pos[b]))) <= 0.03 * h {
                continue;
            }
            match solve_feet(tree, floor, h, a, b) {
                Ok(found) => {
                    if best.as_ref().is_none_or(|current| found.cost < current.cost) {
                        best = Some(found);
                    }
                }
                Err(message) => reason = message,
            }
        }
    }
    best.ok_or(reason)
}

fn solve_feet(tree: &Tree, floor: f64, h: f64, foot_a: usize, foot_b: usize) -> Result<Hypothesis, String> {
    let mut notes = vec![];
    let split = need(tree.lca(&[foot_a, foot_b]), "the feet are on separate skeletons")?;
    if split == foot_a || split == foot_b {
        return Err("one foot chain contains the other".into());
    }
    let branch = |foot: usize| -> Option<usize> {
        let path = tree.path(split, foot)?;
        path.get(1).copied()
    };
    let (leg_a, leg_b) = (need(branch(foot_a), "invalid leg")?, need(branch(foot_b), "invalid leg")?);
    if leg_a == leg_b {
        return Err("both feet hang from the same branch".into());
    }
    let leg_nodes: BTreeSet<usize> = tree.subtree(leg_a).into_iter().chain(tree.subtree(leg_b)).collect();
    let path_a = need(tree.path(leg_a, foot_a), "invalid leg")?;
    let path_b = need(tree.path(leg_b, foot_b), "invalid leg")?;
    // Highest skin joint outside the legs, then the pelvis as the common ancestor.
    let top = need(
        (0..tree.bones.len())
            .filter(|i| tree.weighted[*i] && !leg_nodes.contains(i))
            .max_by(|a, b| tree.pos[*a][2].total_cmp(&tree.pos[*b][2])),
        "no upper body above the legs",
    )?;
    if tree.pos[top][2] < floor + 0.6 * h {
        return Err("no upper body above the legs".into());
    }
    let mut pelvis = need(tree.lca(&[foot_a, foot_b, top]), "legs and upper body are on separate skeletons")?;
    if tree.pos[pelvis][2] < floor + 0.3 * h {
        // A floor-level root above legs and spine is not an anatomical pelvis.
        let spine_path = need(tree.path(pelvis, top), "no spine path")?;
        pelvis = need(
            spine_path.iter().copied().find(|i| {
                tree.pos[*i][2] >= floor + 0.38 * h && tree.is_ancestor(*i, foot_a) && tree.is_ancestor(*i, foot_b)
            }),
            "the joint carrying both legs is at floor level",
        )?;
    }
    if tree.pos[pelvis][2] > floor + 0.75 * h {
        return Err("the legs join the body too high".into());
    }
    let pelvis_pos = tree.pos[pelvis];
    let mut lateral = horizontal(sub(tree.pos[foot_a], tree.pos[foot_b]));
    let width = norm(lateral);
    if width < 1e-9 {
        return Err("the feet are not separated".into());
    }
    lateral = lateral.map(|v| v / width);
    let spine_path = need(tree.path(pelvis, top), "no spine path")?;
    let on_spine: BTreeSet<usize> = spine_path.iter().copied().collect();
    let attach = |i: usize| -> Option<usize> {
        let mut cursor = Some(i);
        while let Some(n) = cursor {
            if on_spine.contains(&n) {
                return Some(n);
            }
            cursor = tree.parent[n];
        }
        None
    };
    let mut arm_tips = [None, None];
    for (slot, sign) in [(0usize, 1.), (1usize, -1.)] {
        let mut best: Option<(f64, usize)> = None;
        for i in 0..tree.bones.len() {
            if !tree.weighted[i] || leg_nodes.contains(&i) || on_spine.contains(&i) || secondary(tree.name(i)) {
                continue;
            }
            let Some(a) = attach(i) else { continue };
            if a == pelvis || tree.pos[a][2] < pelvis_pos[2] + 0.04 * h {
                continue;
            }
            let side = sign * dot(sub(tree.pos[i], pelvis_pos), lateral);
            if side < 0.06 * h {
                continue;
            }
            if best.as_ref().is_none_or(|(s, _)| side > *s) {
                best = Some((side, i));
            }
        }
        arm_tips[slot] = best.map(|(_, i)| i);
    }
    let arm_a_tip = need(arm_tips[0], "no arm reaches out on one side")?;
    let arm_b_tip = need(arm_tips[1], "no arm reaches out on the other side")?;
    let attach_a = need(attach(arm_a_tip), "arm is not attached to the spine")?;
    let attach_b = need(attach(arm_b_tip), "arm is not attached to the spine")?;
    // The chest is the spine joint carrying both arms (the lower one if they differ).
    let chest = if tree.is_ancestor(attach_a, attach_b) { attach_a } else { attach_b };
    if chest == pelvis {
        return Err("the arms attach to the pelvis".into());
    }
    let p_index = need(spine_path.iter().position(|i| *i == pelvis), "pelvis is off the spine")?;
    let c_index = need(spine_path.iter().position(|i| *i == chest), "chest is off the spine")?;
    if c_index <= p_index + 1 {
        return Err("no separate spine joint between pelvis and chest".into());
    }
    let middle = (tree.pos[pelvis][2] + tree.pos[chest][2]) / 2.;
    let spine = need(
        spine_path[p_index + 1..c_index].iter().copied().min_by(|a, b| {
            let score = |i: usize| (tree.pos[i][2] - middle).abs() / h + if tree.weighted[i] { 0. } else { 0.05 };
            score(*a).total_cmp(&score(*b))
        }),
        "no spine joint",
    )?;
    let above: Vec<usize> = spine_path[c_index + 1..].to_vec();
    if above.len() < 2 {
        return Err("no separate neck and head joints above the chest".into());
    }
    let max_skin = above.iter().map(|i| tree.skin[*i]).fold(0., f64::max);
    let head = need(
        above[1..]
            .iter()
            .copied()
            .enumerate()
            .min_by(|(ia, a), (ib, b)| {
                let score = |index: usize, i: usize| {
                    let mut s = index as f64 * 0.15;
                    if max_skin > 0. {
                        s -= tree.skin[i] / max_skin;
                    }
                    if secondary(tree.name(i)) {
                        s += 2.;
                    }
                    if tree.pos[i][2] < tree.pos[chest][2] + 0.04 * h {
                        s += 1.;
                    }
                    if algorithms::name_role(tree.name(i)) == Some("head") {
                        s -= 2.;
                    }
                    s
                };
                score(*ia, *a).total_cmp(&score(*ib, *b))
            })
            .map(|(_, i)| i),
        "no head joint",
    )?;
    let neck = above[0];
    let (leg_a_cost, leg_a_roles) = need(limb(tree, &path_a, foot_a, h, true, pelvis_pos), "leg joints could not be separated")?;
    let (leg_b_cost, leg_b_roles) = need(limb(tree, &path_b, foot_b, h, true, pelvis_pos), "leg joints could not be separated")?;
    let arm_path = |tip: usize, a: usize| -> Option<Vec<usize>> { Some(tree.path(a, tip)?[1..].to_vec()) };
    let arm_a_path = need(arm_path(arm_a_tip, attach_a), "no arm path")?;
    let arm_b_path = need(arm_path(arm_b_tip, attach_b), "no arm path")?;
    let (arm_a_cost, arm_a) = need(limb(tree, &arm_a_path, arm_a_tip, h, false, tree.pos[attach_a]), "arm joints could not be separated")?;
    let (arm_b_cost, arm_b) = need(limb(tree, &arm_b_path, arm_b_tip, h, false, tree.pos[attach_b]), "arm joints could not be separated")?;
    // Which branch is anatomical left?
    let vote = |nodes: &[usize]| {
        let mut l = 0i32;
        for n in nodes {
            match name_side(tree.name(*n)) {
                Some('L') => l += 1,
                Some('R') => l -= 1,
                _ => {}
            }
        }
        l
    };
    let leg_a_is_plus = dot(sub(tree.pos[leg_a_roles[0]], pelvis_pos), lateral) > 0.;
    let (plus_leg, minus_leg, plus_leg_path, minus_leg_path) = if leg_a_is_plus {
        (leg_a_roles, leg_b_roles, &path_a, &path_b)
    } else {
        (leg_b_roles, leg_a_roles, &path_b, &path_a)
    };
    let (plus_arm, minus_arm, plus_arm_path, minus_arm_path) = (arm_a, arm_b, &arm_a_path, &arm_b_path);
    let plus_vote = vote(plus_leg_path) + vote(plus_arm_path);
    let minus_vote = vote(minus_leg_path) + vote(minus_arm_path);
    let sides_from;
    let plus_is_left = if plus_vote > 0 && minus_vote < 0 {
        sides_from = "names";
        true
    } else if plus_vote < 0 && minus_vote > 0 {
        sides_from = "names";
        false
    } else {
        // Toes point forward. Left = up x forward.
        let mut forward = [0.; 3];
        for (foot, tip) in [(plus_leg[2], *plus_leg_path.last().unwrap()), (minus_leg[2], *minus_leg_path.last().unwrap())] {
            let delta = sub(tree.pos[tip], tree.pos[foot]);
            forward = [forward[0] + delta[0], forward[1] + delta[1], 0.];
        }
        if norm(forward) > 0.03 * h {
            sides_from = "toes";
            let left = [-forward[1], forward[0], 0.];
            dot(left, lateral) > 0.
        } else {
            sides_from = "default facing";
            notes.push("Left/right was assumed from the default facing direction (-Y). Check the arms and legs in the mapping preview.".into());
            lateral[0] > 0.
        }
    };
    let (left_leg, right_leg, left_arm, right_arm) = if plus_is_left {
        (plus_leg, minus_leg, plus_arm, minus_arm)
    } else {
        (minus_leg, plus_leg, minus_arm, plus_arm)
    };
    let mut roles = BTreeMap::new();
    let mut put = |role: &str, i: usize| {
        roles.insert(role.to_string(), tree.name(i).to_string());
    };
    put("pelvis", pelvis);
    put("spine", spine);
    put("chest", chest);
    put("neck", neck);
    put("head", head);
    for (side, leg, arm) in [("L", left_leg, left_arm), ("R", right_leg, right_arm)] {
        put(&format!("thigh_{side}"), leg[0]);
        put(&format!("shin_{side}"), leg[1]);
        put(&format!("foot_{side}"), leg[2]);
        put(&format!("upperarm_{side}"), arm[0]);
        put(&format!("forearm_{side}"), arm[1]);
        put(&format!("hand_{side}"), arm[2]);
    }
    if roles.values().collect::<BTreeSet<_>>().len() != ROLES.len() {
        return Err("two body roles resolved to the same joint".into());
    }
    let positions: Vec<Bone> = tree
        .bones
        .iter()
        .zip(&tree.pos)
        .map(|(b, p)| Bone { position: *p, ..b.clone() })
        .collect();
    match algorithms::identify(&positions, &roles) {
        Ok(check) if check.errors.is_empty() => {}
        Ok(check) => return Err(check.errors.join("; ")),
        Err(error) => return Err(error.to_string()),
    }
    // Symmetric limbs are more plausible than mismatched ones.
    let length = |r: [usize; 3]| norm(sub(tree.pos[r[1]], tree.pos[r[0]])) + norm(sub(tree.pos[r[2]], tree.pos[r[1]]));
    let asymmetry = ((length(leg_a_roles) - length(leg_b_roles)).abs() + (length(arm_a) - length(arm_b)).abs()) / h;
    let cost = leg_a_cost + leg_b_cost + arm_a_cost + arm_b_cost + asymmetry * 10.;
    Ok(Hypothesis { cost, roles, sides_from: sides_from.into(), notes })
}

fn weighted_set(weighted: &[String]) -> BTreeSet<&str> {
    // Skin evidence is required: parallel unweighted control chains are
    // indistinguishable from the deforming chain without it.
    weighted.iter().map(String::as_str).collect()
}

/// Detect roles assuming the given source `up` direction.
pub fn detect_with_up(bones: &[Bone], weighted: &[String], up: [f64; 3]) -> Option<Detection> {
    let names = weighted_set(weighted);
    if names.is_empty() {
        return None;
    }
    let rotated_bones = rotated(bones, up);
    let positions = rotated_bones.iter().map(|b| b.position).collect();
    let tree = Tree::new(bones, &names, positions)?;
    let found = solve(&tree).ok()?;
    let check = algorithms::identify(&rotated_bones, &found.roles).ok()?;
    if !check.errors.is_empty() {
        return None;
    }
    let (_, rotation) = rotation_for(up);
    Some(Detection { policy: POLICY.into(), roles: found.roles, up, rotation, sides_from: found.sides_from, notes: found.notes })
}

/// Why structural recognition did not produce a valid humanoid (for review messages).
pub fn explain(bones: &[Bone], weighted: &[String]) -> String {
    let names = weighted_set(weighted);
    if names.is_empty() {
        return "no skin weights were found".into();
    }
    let Some(tree) = Tree::new(bones, &names, bones.iter().map(|b| b.position).collect()) else {
        return "the bone hierarchy is cyclic or has duplicate names".into();
    };
    match solve(&tree) {
        Err(reason) => reason,
        Ok(found) => match algorithms::identify(bones, &found.roles) {
            Ok(result) if result.errors.is_empty() => "recognized".into(),
            Ok(result) => result.errors.join("; "),
            Err(error) => error.to_string(),
        },
    }
}

/// Upright detection only (the inspected source is already Z-up).
pub fn detect(bones: &[Bone], weighted: &[String]) -> Option<Detection> {
    detect_with_up(bones, weighted, [0., 0., 1.])
}

/// When the source is not upright, return the quarter-turn rotation that makes it so.
/// Named roles are tried first; structural recognition is the fallback.
pub fn orientation(bones: &[Bone], weighted: &[String], named: &BTreeMap<String, String>) -> Option<Detection> {
    let upright_names = named.len() == ROLES.len()
        && algorithms::identify(bones, named).is_ok_and(|r| r.errors.is_empty());
    if upright_names || detect(bones, weighted).is_some() {
        return None;
    }
    for up in UP_CANDIDATES.iter().skip(1) {
        if named.len() == ROLES.len() {
            let turned = rotated(bones, *up);
            if algorithms::identify(&turned, named).is_ok_and(|r| r.errors.is_empty()) {
                let (_, rotation) = rotation_for(*up);
                return Some(Detection {
                    policy: POLICY.into(),
                    roles: named.clone(),
                    up: *up,
                    rotation,
                    sides_from: "names".into(),
                    notes: vec![],
                });
            }
        }
        if let Some(found) = detect_with_up(bones, weighted, *up) {
            return Some(found);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn side_tokens() {
        for (name, side) in [
            ("upperarm_l", 'L'), ("Arm.R", 'R'), ("lShldrBend", 'L'), ("rForearmBend", 'R'),
            ("CC_Base_L_Upperarm", 'L'), ("Bip01 R Thigh", 'R'), ("LeftHand", 'L'), ("左腕", 'L'),
            ("DEF-thigh.L.001", 'L'), ("mixamorig:RightUpLeg", 'R'),
        ] {
            assert_eq!(name_side(name), Some(side), "{name}");
        }
        for name in ["Bone.003", "spine_02", "Hips", "lowerJaw", "Head"] {
            assert_eq!(name_side(name), None, "{name}");
        }
    }
}
