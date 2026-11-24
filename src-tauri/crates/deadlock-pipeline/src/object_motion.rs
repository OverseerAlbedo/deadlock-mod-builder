//! The game's own animations of an ability object with several bones, for the
//! fit view's player: the object's compiled clips (named by its animation
//! graph) decoded with `deadlock_preparation::clip_decode`, as skin matrices
//! (pose · bind⁻¹, model space) per bone and frame, which the page applies to
//! the game's mesh and to the user's fitted, bound model alike. Read-only; it
//! never needs the operation lease and depends on nothing of the hero's
//! movement preview.
use crate::invalid;
use deadlock_core::{text::casefold, Result};
use deadlock_preparation::clip_decode::{self, Clip, Skeleton as ClipSkeleton};
use deadlock_skeleton::transforms::{multiply, Matrix4};
use deadlock_vpk::Vpk;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

/// Frames per second of the packet, and its most frames per clip (40 s at 30
/// frames a second; the longest clip of a listed object is 30 s).
pub const FPS: f64 = 30.;
pub const MAX_FRAMES: usize = 1200;
/// Said when an object's animations are not in a form the app reads.
pub const NOT_READ: &str = "The game animates this object in a way the app cannot show here; it shows your model at rest.";
/// Said under the player for a clip longer than the packet holds.
pub fn cut_line(shown: f64, duration: f64) -> String {
    format!("Shows the first {shown:.1} s of this {duration:.1} s animation.")
}
/// Said under the player for a layer clip (played over another one).
pub fn layer_line(base: &str, held: bool) -> String {
    if held {
        format!("The game starts this one over the others while the ability runs and sets its speed; here it plays over the last pose of {base}.")
    } else {
        format!("The game starts this one over the others while the ability runs and sets its speed; here it plays over {base}.")
    }
}

/// One of the object's clips.
#[derive(Debug, Clone, Serialize)]
pub struct ClipInfo {
    /// The compiled clip (`….vnmclip_c`).
    pub file: String,
    pub label: String,
    pub duration: f64,
    pub additive: bool,
    /// Whether the object's animation graph lets the clip loop (absent when
    /// no clip node of a graph plays it).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loops: Option<bool>,
}
/// The header of a motion packet; the data is `frames × bones` row-major 3×4
/// float32 matrices in `bones` order.
#[derive(Debug, Clone, Serialize)]
pub struct MotionHeader {
    pub bones: Vec<String>,
    pub clips: Vec<ClipInfo>,
    /// The clip in the data (absent when none could be read).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clip: Option<String>,
    pub fps: f64,
    pub frames: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// What the player shows differently from the game, one sentence each
    /// (a shortened clip, the pose a layer clip is played over).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub lines: Vec<String>,
}
fn compiled(path: &str) -> String {
    let p = casefold(path);
    if p.ends_with("_c") { p } else { format!("{p}_c") }
}
fn refs(archive: &Vpk, path: &str) -> Vec<String> {
    archive.read(path).ok().and_then(|raw| deadlock_vpk::resource_references(&raw).ok()).unwrap_or_default().into_iter().map(|r| compiled(&r)).collect()
}
/// A readable clip name: its file stem without the words it shares with the object's stem.
pub fn clip_label(clip: &str, object: &str) -> String {
    let stem = |p: &str| p.rsplit('/').next().unwrap_or(p).trim_end_matches("_c").trim_end_matches(".vnmclip").trim_end_matches(".vmdl").to_owned();
    let own: BTreeSet<String> = stem(object).split('_').map(str::to_owned).collect();
    let clip_stem = stem(clip);
    let words: Vec<&str> = clip_stem.split('_').filter(|w| !w.is_empty()).collect::<Vec<_>>();
    let start = words.iter().take_while(|w| own.contains(**w)).count();
    let kept = if start < words.len() { &words[start..] } else { &words[..] };
    let text = kept.join(" ");
    let mut chars = text.chars();
    chars.next().map(|c| c.to_uppercase().collect::<String>() + chars.as_str()).unwrap_or_default()
}
/// The object's clips (named by its animation graphs) and its animation skeleton.
pub fn object_clips(archive: &Vpk, target: &str) -> (Vec<String>, Option<String>) {
    let model = refs(archive, &compiled(target));
    let skeleton = model.iter().find(|r| r.ends_with(".vnmskel_c")).cloned();
    let mut clips = BTreeSet::new();
    for graph in model.iter().filter(|r| r.ends_with(".vnmgraph_c")) {
        let mut pending = vec![graph.clone()];
        let mut seen = BTreeSet::new();
        while let Some(p) = pending.pop() {
            if !seen.insert(p.clone()) || seen.len() > 512 {
                continue;
            }
            for r in refs(archive, &p) {
                if r.ends_with(".vnmclip_c") {
                    clips.insert(r);
                } else if r.ends_with(".vnmgraph_c") || r.ends_with(".vnmgraphvariation_c") {
                    pending.push(r);
                }
            }
        }
    }
    (clips.into_iter().collect(), skeleton)
}
fn bind(skeleton: &deadlock_skeleton::Skeleton) -> Vec<Matrix4> {
    let mut out: Vec<Matrix4> = vec![];
    for i in 0..skeleton.names.len() {
        let m = clip_decode::matrix(skeleton.positions[i], skeleton.rotations[i], skeleton.scales.get(i).copied().unwrap_or(1.));
        let p = skeleton.parents[i];
        out.push(if p >= 0 && (p as usize) < out.len() { multiply(&out[p as usize], &m) } else { m });
    }
    out
}
fn invert(m: &Matrix4) -> Matrix4 {
    // Rigid (with uniform scale) inverse: R⁻¹ = Rᵀ / s², t' = −R⁻¹ t.
    let s2 = (0..3).map(|r| m[r][0] * m[r][0]).sum::<f64>().max(1e-12);
    let mut out = [[0.; 4]; 4];
    for r in 0..3 {
        for c in 0..3 {
            out[r][c] = m[c][r] / s2;
        }
    }
    for r in 0..3 {
        out[r][3] = -(0..3).map(|c| out[r][c] * m[c][3]).sum::<f64>();
    }
    out[3][3] = 1.;
    out
}
/// The motion packet of `clip` (default: the longest non-additive clip) on the
/// object `target` whose compiled model is `raw`.
pub fn motion(archive: &Vpk, target: &str, raw: &[u8], clip: Option<&str>) -> Result<(MotionHeader, Vec<u8>)> {
    let Ok(model) = crate::ability_objects::game_skeleton(raw) else {
        return Ok((MotionHeader { bones: vec![], clips: vec![], clip: None, fps: FPS, frames: 0, note: Some(NOT_READ.into()), lines: vec![] }, vec![]));
    };
    let rest = bind(&model);
    let (files, skeleton) = object_clips(archive, target);
    let loops = graph_loops(archive, target);
    let mut header = MotionHeader { bones: model.names.clone(), clips: vec![], clip: None, fps: FPS, frames: 0, note: None, lines: vec![] };
    let skeleton = skeleton.and_then(|s| archive.read(&s).ok()).and_then(|raw| ClipSkeleton::decode(&raw).ok());
    let mut decoded: BTreeMap<String, Clip> = BTreeMap::new();
    for file in &files {
        if let Some(c) = archive.read(file).ok().and_then(|raw| Clip::decode(&raw).ok()) {
            header.clips.push(ClipInfo { file: file.clone(), label: clip_label(file, target), duration: c.duration, additive: c.additive, loops: loops.get(file).copied() });
            decoded.insert(file.clone(), c);
        }
    }
    let Some(skeleton) = skeleton.filter(|_| !decoded.is_empty()) else {
        header.note = Some(NOT_READ.into());
        return Ok((header, vec![]));
    };
    let chosen = match clip {
        Some(c) => Some(compiled(c)).filter(|c| decoded.contains_key(c)).ok_or_else(|| invalid(format!("The object has no such animation: {c}")))?,
        None => match header.clips.iter().filter(|c| !c.additive).max_by(|a, b| a.duration.total_cmp(&b.duration)) {
            Some(c) => c.file.clone(),
            // Only layer clips: nothing to play them over; said, never an error.
            None => {
                header.note = Some(NOT_READ.into());
                return Ok((header, vec![]));
            }
        },
    };
    let base = header.clips.iter().filter(|c| !c.additive).max_by(|a, b| a.duration.total_cmp(&b.duration)).map(|c| (c.clone(), &decoded[&c.file]));
    let played = &decoded[&chosen];
    // A layer clip is shown over the pose the base clip holds when the graph
    // plays the base once, else over the base looping under it.
    let over = base.as_ref().filter(|_| played.additive).map(|(info, clip)| {
        let held = info.loops != Some(true);
        header.lines.push(layer_line(&info.label, held));
        (*clip, held)
    });
    let Some(poses) = sample(&skeleton, played, over, &mut header) else {
        header.note = Some(NOT_READ.into());
        return Ok((header, vec![]));
    };
    let root = model.parents.iter().position(|p| *p < 0).map(|i| (model.names[i].clone(), model.rotations[i]));
    let mut data = Vec::with_capacity(poses.len() * model.names.len() * 48);
    for mut pose in poses.into_iter() {
        if let Some((name, turn)) = &root {
            clip_decode::turn_root(&mut pose, name, *turn);
        }
        for (i, name) in model.names.iter().enumerate() {
            // A bone the clip does not hold stays at rest.
            let skin = pose.get(name).map_or([[1., 0., 0., 0.], [0., 1., 0., 0.], [0., 0., 1., 0.], [0., 0., 0., 1.]], |p| multiply(p, &invert(&rest[i])));
            for row in skin.iter().take(3) {
                for v in row {
                    data.extend_from_slice(&(*v as f32).to_le_bytes());
                }
            }
        }
    }
    header.frames = data.len() / (model.names.len().max(1) * 48);
    header.clip = Some(chosen);
    Ok((header, data))
}

/// How many frames the packet holds for a clip of `duration` seconds: one
/// every 1/`FPS` s with the clip's last moment always included, at most
/// `MAX_FRAMES`; the second value is true when that cut the clip short.
pub fn frame_count(duration: f64) -> (usize, bool) {
    // A clip stored at the packet's own rate (duration a whole number of frames,
    // within float32 rounding) keeps exactly its stored frames.
    let steps = duration.max(0.) * FPS;
    let whole = if (steps - steps.round()).abs() < 0.01 { steps.round() } else { steps.ceil() } as usize + 1;
    (whole.min(MAX_FRAMES), whole > MAX_FRAMES)
}

/// Model-space poses of `clip` (a layer clip over `over`: the base clip and
/// whether its last pose is held), sampled by `frame_count`. Writes the cut
/// line into `header`. None when the clip does not fit its skeleton.
fn sample(skeleton: &ClipSkeleton, clip: &Clip, over: Option<(&Clip, bool)>, header: &mut MotionHeader) -> Option<Vec<BTreeMap<String, Matrix4>>> {
    if clip.frames.first().map(Vec::len) != Some(skeleton.names.len()) || (clip.additive && over.is_none()) {
        return None;
    }
    if over.is_some_and(|(base, _)| base.frames.first().map(Vec::len) != Some(skeleton.names.len())) {
        return None;
    }
    let (count, cut) = frame_count(clip.duration);
    if cut {
        header.lines.insert(0, cut_line((count - 1) as f64 / FPS, clip.duration));
    }
    let poses = (0..count)
        .map(|f| {
            let t = (f as f64 / FPS).min(clip.duration);
            let mut locals = clip.locals_at(t);
            if let Some((base, held)) = over {
                let bt = if held { base.duration } else if base.duration > 0. { t % base.duration } else { 0. };
                locals = clip_decode::layer(&base.locals_at(bt), &locals);
            }
            skeleton.names.iter().cloned().zip(skeleton.model_space(&locals)).collect()
        })
        .collect();
    Some(poses)
}

/// Whether each clip may loop, from the object's animation graphs' clip
/// nodes (`m_bAllowLooping`, the clip named by `m_nDataSlotIdx` in
/// `m_resources`); a clip played by several nodes loops when any of them does.
pub fn graph_loops(archive: &Vpk, target: &str) -> BTreeMap<String, bool> {
    let mut out = BTreeMap::new();
    let mut pending: Vec<String> = refs(archive, &compiled(target)).into_iter().filter(|r| r.ends_with(".vnmgraph_c")).collect();
    let mut seen = BTreeSet::new();
    while let Some(graph) = pending.pop() {
        if !seen.insert(graph.clone()) || seen.len() > 512 {
            continue;
        }
        pending.extend(refs(archive, &graph).into_iter().filter(|r| r.ends_with(".vnmgraph_c") || r.ends_with(".vnmgraphvariation_c")));
        let Some(raw) = archive.read(&graph).ok() else { continue };
        for block in deadlock_resource::container::read_blocks(&raw, "DATA").unwrap_or_default() {
            node_loops(&block.root, &mut out);
        }
    }
    out
}
fn node_loops(root: &deadlock_resource::Value, out: &mut BTreeMap<String, bool>) {
    let resources: Vec<String> = root.get("m_resources").and_then(|r| r.array().ok()).map(|a| a.iter().map(|r| r.string().map(compiled).unwrap_or_default()).collect()).unwrap_or_default();
    let Some(nodes) = root.get("m_nodes").and_then(|n| n.array().ok()) else { return };
    for node in nodes {
        if node.get("_class").and_then(|c| c.string().ok()) != Some("CNmClipNode::CDefinition") {
            continue;
        }
        let (Some(slot), Some(looping)) = (node.get("m_nDataSlotIdx").and_then(|s| s.integer().ok()), node.get("m_bAllowLooping").map(|l| matches!(l.kind, deadlock_resource::Kind::Bool(true)))) else { continue };
        if let Some(file) = usize::try_from(slot).ok().and_then(|s| resources.get(s)).filter(|f| !f.is_empty()) {
            *out.entry(file.clone()).or_insert(false) |= looping;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clip_names_drop_the_objects_own_words() {
        let object = "models/heroes_wip/mcginnis/healing_canister/healing_canister.vmdl";
        assert_eq!(clip_label("models/heroes_wip/mcginnis/healing_canister/healing_canister_deploy.vnmclip_c", object), "Deploy");
        assert_eq!(clip_label("x/healing_canister_drain_liquid.vnmclip_c", object), "Drain liquid");
        assert_eq!(clip_label("x/healing_canister.vnmclip_c", object), "Healing canister");
    }

    #[test]
    fn a_clip_keeps_its_stored_frames_and_only_a_long_one_is_cut() {
        // 30 s stored at 30 frames a second: 901 frames, nothing cut.
        assert_eq!(frame_count(30.), (901, false));
        // 29 steps stored as a float32 duration.
        assert_eq!(frame_count(0.966_699_957_847_595_2), (30, false));
        // A duration between frames keeps its last moment as an extra frame.
        assert_eq!(frame_count(0.95), (30, false));
        assert_eq!(frame_count(50.), (MAX_FRAMES, true));
        assert_eq!(cut_line(39.97, 50.), "Shows the first 40.0 s of this 50.0 s animation.");
    }

    #[test]
    fn the_graph_says_which_clips_loop() {
        use deadlock_resource::{Kind, Value};
        let s = |t: &str| Value::new(Kind::String(t.into()));
        let node = |class: &str, slot: i64, looping: bool| {
            Value::new(Kind::Object(vec![("_class".into(), s(class)), ("m_nDataSlotIdx".into(), Value::new(Kind::I64(slot))), ("m_bAllowLooping".into(), Value::new(Kind::Bool(looping)))]))
        };
        let root = Value::new(Kind::Object(vec![
            ("m_resources".into(), Value::new(Kind::Array(vec![s("a/deploy.vnmclip"), s("a/drain.vnmclip"), s("a/idle.vnmclip")]))),
            ("m_nodes".into(), Value::new(Kind::Array(vec![node("CNmClipNode::CDefinition", 0, false), node("CNmClipNode::CDefinition", 1, false), node("CNmClipNode::CDefinition", 2, true), node("CNmClipNode::CDefinition", 1, true), node("CNmSpeedScaleNode::CDefinition", 0, true)]))),
        ]));
        let mut out = BTreeMap::new();
        node_loops(&root, &mut out);
        assert_eq!(out, BTreeMap::from([("a/deploy.vnmclip_c".to_string(), false), ("a/drain.vnmclip_c".to_string(), true), ("a/idle.vnmclip_c".to_string(), true)]));
    }

    #[test]
    fn a_layer_clip_plays_over_the_pose_the_base_holds() {
        let skeleton = ClipSkeleton { id: "s".into(), names: vec!["root".into(), "lid".into()], parents: vec![-1, 0], reference: vec![clip_decode::Local::IDENTITY; 2] };
        let at = |z: f64| clip_decode::Local { position: [0., 0., z], rotation: [0., 0., 0., 1.], scale: 1. };
        // The base rises from 0 to 10 over 1 s and holds; the layer lowers the lid 3.
        let base = Clip { skeleton: "s".into(), duration: 1., additive: false, frames: vec![vec![at(0.), at(0.)], vec![at(10.), at(0.)]], root_motion: vec![], model_space_bones: 0 };
        let zero = clip_decode::Local { position: [0.; 3], rotation: [0., 0., 0., 1.], scale: 0. };
        let lower = clip_decode::Local { position: [0., 0., -3.], ..zero };
        let layer = Clip { skeleton: "s".into(), duration: 0.5, additive: true, frames: vec![vec![zero, zero], vec![zero, lower]], root_motion: vec![], model_space_bones: 0 };
        let mut header = MotionHeader { bones: vec![], clips: vec![], clip: None, fps: FPS, frames: 0, note: None, lines: vec![] };
        let poses = sample(&skeleton, &layer, Some((&base, true)), &mut header).unwrap();
        assert_eq!(poses.len(), 16);
        let z = |p: &BTreeMap<String, Matrix4>, b: &str| p[b][2][3];
        assert!((z(&poses[0], "root") - 10.).abs() < 1e-9 && (z(&poses[0], "lid") - 10.).abs() < 1e-9);
        assert!((z(&poses[15], "lid") - 7.).abs() < 1e-9, "{}", z(&poses[15], "lid"));
        assert!(header.lines.is_empty());
        // A layer clip without a base is not shown as a pose.
        assert!(sample(&skeleton, &layer, None, &mut header).is_none());
    }

    #[test]
    fn a_skin_matrix_at_rest_is_the_identity() {
        let skeleton = deadlock_skeleton::Skeleton { names: vec!["a".into(), "b".into()], parents: vec![-1, 0], positions: vec![[1., 2., 3.], [0., 5., 0.]], rotations: vec![[0.5, 0.5, 0.5, 0.5], [0., 0., 0.7071068, 0.7071068]], scales: vec![1., 1.], flags: vec![] };
        let rest = bind(&skeleton);
        for m in &rest {
            let i = multiply(m, &invert(m));
            for r in 0..4 {
                for c in 0..4 {
                    assert!((i[r][c] - f64::from(u8::from(r == c))).abs() < 1e-6, "{i:?}");
                }
            }
        }
    }
}
