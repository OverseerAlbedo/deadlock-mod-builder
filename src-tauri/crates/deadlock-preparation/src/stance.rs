//! The hero holding its gun, for the gun fit preview: the first frame of the
//! hero's idle-with-gun clip (decompiled with the hero's sources) applied to
//! the hero's rest skeleton. The result is a skin matrix per bone (rest to
//! stance), used to pose the preview body and to carry each gun with its hand.
//! Preview only: it never changes the model, the fit or the build.
use deadlock_dmx::{Dmx, Value};
use deadlock_skeleton::transforms::{identity, multiply, Matrix4, PreparedBone};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

/// Clips that show the hero standing with the gun ready, best first.
pub const CLIPS: [&str; 4] = ["weapon_stand_idle", "primary_stand_idle", "stand_idle", "idle"];

/// Other standing idles, best first: with the gun, plain, then ability, item
/// or out-of-combat stances.
pub fn rank(stem: &str) -> Option<(u8, usize)> {
    let stem = stem.to_ascii_lowercase();
    if !stem.contains("stand_idle") || stem.contains("crouch") {
        return None;
    }
    let group = if ["weapon", "primary", "gun", "crossbow", "rifle", "pistol"].iter().any(|w| stem.contains(w)) {
        0
    } else if ["item", "reload", "combat", "sleep", "shop", "invis", "bomb", "drain", "ability", "cast"].iter().any(|w| stem.contains(w)) {
        2
    } else {
        1
    };
    Some((group, stem.len()))
}

/// What a `.vnmclip` written by Source 2 Viewer 21.1 or later (a
/// `CNmClipDocument`) says: the DMX of its take and the frames it plays.
struct ClipDocument {
    source: PathBuf,
    /// `m_nStartFrame`..`m_nEndFrame` of the take, when the clip names them.
    range: Option<(f64, f64)>,
}

fn is_clip_file(path: &Path) -> bool {
    path.extension().is_some_and(|x| x.eq_ignore_ascii_case("vnmclip"))
}

/// The take of a newer `.vnmclip`. Source 2 Viewer 21.1 writes a clip's DMX
/// where `m_sourceFilename` names it (often `dmx/animation/…`, one take shared
/// by several clips that each play a frame range of it), placed relative to the
/// folder the `.vnmclip` went to: from the game archive that is the archive
/// path, from a single compiled model the `.vnmclip` files land beside the
/// `.vmdl` and the takes under another parent. The DMX is therefore the longest
/// tail of `m_sourceFilename` found below an ancestor of the clip's folder
/// (single file names only in that folder), never above the sources' root, the
/// folder holding `models`. Older versions wrote no `_class` and the clip's
/// own frames as a `.dmx` beside the `.vnmclip` (`animation_files`).
fn clip_document(clip: &Path) -> Option<ClipDocument> {
    let document = deadlock_resource::text::Document::parse(std::fs::read_to_string(clip).ok()?).ok()?;
    let scalar = |key: &str| document.root.get(key).and_then(|n| n.scalar()).map(str::trim);
    if scalar("_class") != Some("CNmClipDocument") {
        return None;
    }
    let source = scalar("m_sourceFilename")?.replace('\\', "/");
    let parts: Vec<&str> = source.split('/').collect();
    if !source.to_ascii_lowercase().ends_with(".dmx") || parts.iter().any(|p| p.is_empty() || *p == "." || *p == ".." || p.contains(':')) {
        return None;
    }
    let folder = clip.parent()?;
    let root = folder.ancestors().find(|a| a.file_name().is_some_and(|n| n.eq_ignore_ascii_case("models")))?.parent()?;
    let ancestors: Vec<&Path> = folder.ancestors().take_while(|a| a.starts_with(root)).collect();
    let source = (1..=parts.len()).rev().find_map(|tail| {
        let relative: PathBuf = parts[parts.len() - tail..].iter().collect();
        ancestors.iter().enumerate().filter(|(up, _)| tail > 1 || *up == 0).map(|(_, a)| a.join(&relative)).find(|p| p.is_file())
    })?;
    let frame = |key: &str| scalar(key).and_then(|v| v.parse::<f64>().ok());
    let range = match (frame("m_nStartFrame"), frame("m_nEndFrame")) {
        (Some(start), Some(end)) if start >= 0. && end >= start => Some((start, end)),
        _ => None,
    };
    Some(ClipDocument { source, range })
}

/// Whether a clip is a `.vnmclip` clip document of Source 2 Viewer 21.1 or
/// later whose take was found (`clip_document`). That layout writes
/// `m_additiveType` for every additive clip, so such a document without the
/// field is a pose of its own; `additive_authoring` still says None for it.
pub fn is_clip_document(clip: &Path) -> bool {
    is_clip_file(clip) && clip_document(clip).is_some()
}

/// How a clip's `.vnmclip` says it is authored (`m_additiveType`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Additive {
    /// "None": a pose of its own.
    Pose,
    /// "RelativeToFrame": the change from one frame of the same take
    /// (`m_additiveBaseFrame` "FirstFrame" = 0, "UserSpecifiedFrame" =
    /// `m_nAdditiveBaseFrameIdx`), frames counted from the take's start.
    Frame(usize),
    /// "RelativeToSkeleton", "RelativeToAnimation" or a kind not known here:
    /// a layer whose reference the preview cannot rebuild.
    Other(String),
}

/// What the `.vnmclip` of a clip says about additive authoring. Both Source 2
/// Viewer layouts write `m_additiveType` in the `.vnmclip` (21.1 in the clip
/// document itself, older versions beside the clip's own `.dmx`); clips that
/// are not additive have no such field. None when there is no `.vnmclip` or
/// it has no `m_additiveType` (then only the pose itself can tell).
pub fn additive_authoring(clip: &Path) -> Option<Additive> {
    let document = if is_clip_file(clip) { clip.to_path_buf() } else { clip.with_extension("vnmclip") };
    additive_from_text(&std::fs::read_to_string(document).ok()?)
}

fn additive_from_text(text: &str) -> Option<Additive> {
    let document = deadlock_resource::text::Document::parse(text.to_string()).ok()?;
    let scalar = |key: &str| document.root.get(key).and_then(|n| n.scalar()).map(str::trim);
    let kind = scalar("m_additiveType")?;
    Some(match kind {
        "" | "None" => Additive::Pose,
        "RelativeToFrame" => match scalar("m_additiveBaseFrame") {
            Some("FirstFrame") => Additive::Frame(0),
            Some("UserSpecifiedFrame") => match scalar("m_nAdditiveBaseFrameIdx").and_then(|v| v.parse::<usize>().ok()) {
                Some(index) => Additive::Frame(index),
                None => Additive::Other(format!("{kind} without a frame")),
            },
            other => Additive::Other(format!("{kind} {}", other.unwrap_or("(no base frame)"))),
        },
        other => Additive::Other(other.to_string()),
    })
}

fn collect_animation_files(root: &Path, depth: usize, dmx: &mut Vec<PathBuf>, clips: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else { return };
    for path in entries.filter_map(|e| e.ok()).map(|e| e.path()) {
        if path.is_dir() {
            if depth > 0 {
                collect_animation_files(&path, depth - 1, dmx, clips);
            }
        } else if path.extension().is_some_and(|x| x.eq_ignore_ascii_case("dmx")) {
            dmx.push(path);
        } else if is_clip_file(&path) {
            clips.push(path);
        }
    }
}

/// Animation files below `root`, a few levels deep: (is an AnimGraph2 clip,
/// path). A clip is a newer `.vnmclip` whose take was found (`clip_document`;
/// the path is the `.vnmclip`, so the clip keeps its own name and frame range),
/// or a `.dmx` next to an older `.vnmclip` of the same name. Clip folders
/// differ per hero (`clips`, Warden's `clip`, Sinclair's `dmx/animation/ag2`).
/// Takes of newer clips are not listed again; other `.dmx` files are
/// older-style animations or meshes.
fn animation_files(root: &Path, depth: usize, out: &mut Vec<(bool, PathBuf)>) {
    let (mut dmx, mut clips) = (vec![], vec![]);
    collect_animation_files(root, depth, &mut dmx, &mut clips);
    let key = |p: &Path| p.to_string_lossy().to_lowercase();
    let mut takes = BTreeSet::new();
    let mut newer = BTreeSet::new();
    for clip in clips {
        if let Some(document) = clip_document(&clip) {
            takes.insert(key(&document.source));
            newer.insert(key(&clip));
            out.push((true, clip));
        }
    }
    for path in dmx {
        if takes.contains(&key(&path)) {
            continue;
        }
        let beside = path.with_extension("vnmclip");
        out.push((beside.is_file() && !newer.contains(&key(&beside)), path));
    }
}

/// The idle-with-gun clip of the hero's decompiled sources. AnimGraph2 clips
/// come first, the hero's own model folder before the rest of its sources (Lady
/// Geist's model is in heroes_wip/geist, her clips in heroes_staging/ghost), then
/// older-style animations; within each, the names in `CLIPS`, then `rank`.
pub fn find_clip(base: &Path, model_folder: &Path) -> Option<PathBuf> {
    find_named(base, model_folder, &CLIPS, rank)
}

/// Rank of a clip name that has all of `must` and none of `avoid`: names with
/// a gun word first, ability/item/hidden-state names last, shorter first.
pub fn rank_words(stem: &str, must: &[&str], avoid: &[&str]) -> Option<(u8, usize)> {
    let stem = stem.to_ascii_lowercase();
    if !must.iter().all(|w| stem.contains(w)) || avoid.iter().any(|w| stem.contains(w)) {
        return None;
    }
    let group = if ["weapon", "primary", "gun", "crossbow", "rifle", "pistol"].iter().any(|w| stem.contains(w)) {
        0
    } else if ["item", "reload", "combat", "sleep", "shop", "invis", "hideout", "bomb", "drain", "ability", "cast", "zipline", "rope"].iter().any(|w| stem.contains(w)) {
        2
    } else {
        1
    };
    Some((group, stem.len()))
}

/// A clip of the hero's sources by exact names (best first), then by `rank`,
/// searched like `find_clip`.
pub fn find_named(base: &Path, model_folder: &Path, names: &[&str], rank: impl Fn(&str) -> Option<(u8, usize)>) -> Option<PathBuf> {
    find_ranked(base, model_folder, names, rank).into_iter().next()
}

/// Every matching clip of the hero's sources, best first: AnimGraph2 clips
/// before older-style animations, the hero's own model folder before the rest
/// of its sources; within each, the exact `names` in order, then by `rank`.
pub fn find_ranked(base: &Path, model_folder: &Path, names: &[&str], rank: impl Fn(&str) -> Option<(u8, usize)>) -> Vec<PathBuf> {
    let mut files = vec![];
    animation_files(&base.join("models"), 8, &mut files);
    files.sort();
    let stem = |p: &Path| p.file_stem().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
    let mut out: Vec<PathBuf> = vec![];
    for (clip, near) in [(true, true), (true, false), (false, true), (false, false)] {
        let pool: Vec<&PathBuf> = files.iter().filter(|(c, p)| *c == clip && p.starts_with(model_folder) == near).map(|(_, p)| p).collect();
        for name in names {
            out.extend(pool.iter().filter(|p| stem(p) == *name).map(|p| (*p).clone()));
        }
        let mut ranked: Vec<_> = pool.iter().filter_map(|p| Some((rank(&stem(p))?, (*p).clone()))).collect();
        ranked.sort();
        out.extend(ranked.into_iter().map(|(_, p)| p));
    }
    let mut seen = BTreeSet::new();
    out.retain(|p| seen.insert(p.clone()));
    out
}

/// Model-space transform of every joint in the clip's first frame. It walks
/// the clip's own joint tree, which may differ from the compiled skeleton's
/// (Haze's pelvis hangs under root_motion in her clips but is a root in game).
pub fn first_frame(clip: &Path) -> Option<BTreeMap<String, Matrix4>> {
    frames(clip, 30., 1).and_then(|(_, mut f)| (!f.is_empty()).then(|| f.remove(0)))
}

/// One animated value of a clip: key times in seconds and their values.
struct Track<const N: usize> {
    times: Vec<f64>,
    values: Vec<[f64; N]>,
}
impl<const N: usize> Track<N> {
    /// The value at `t` (seconds), linear between keys and held at the ends.
    fn at(&self, t: f64) -> [f64; N] {
        let i = self.times.partition_point(|&k| k <= t);
        if i == 0 {
            return self.values[0];
        }
        if i >= self.values.len() {
            return self.values[self.values.len() - 1];
        }
        let (a, b) = (self.times[i - 1], self.times[i]);
        let f = if b > a { (t - a) / (b - a) } else { 0. };
        let (x, mut y) = (self.values[i - 1], self.values[i]);
        if N == 4 && (0..4).map(|k| x[k] * y[k]).sum::<f64>() < 0. {
            y = y.map(|v| -v); // the shorter way round for rotations
        }
        std::array::from_fn(|k| x[k] + (y[k] - x[k]) * f)
    }
}

/// A clip's animated transforms (by DmeTransform element) and its joint tree.
struct ClipTracks {
    dmx: Dmx,
    positions: BTreeMap<i32, Track<3>>,
    rotations: BTreeMap<i32, Track<4>>,
    /// Seconds to the last key (of the clip's frame range).
    length: f64,
    /// Seconds into the take where the clip starts (a newer `.vnmclip`'s
    /// `m_nStartFrame`); 0 for a DMX of its own.
    offset: f64,
    /// Frames per second of the take.
    rate: f64,
}
impl ClipTracks {
    /// A clip's `.dmx`, or a newer `.vnmclip`: its take, from its start frame
    /// to its end frame.
    fn load(clip: &Path) -> Option<Self> {
        let (file, range) = if is_clip_file(clip) {
            let document = clip_document(clip)?;
            (document.source, document.range)
        } else {
            (clip.to_path_buf(), None)
        };
        let dmx = Dmx::load(&file).ok()?;
        let rate = dmx.elements.iter().find(|e| e.kind == "DmeChannelsClip").and_then(|c| match c.get("frameRate") {
            Some(Value::Integer(r)) if *r > 0 => Some(f64::from(*r)),
            Some(Value::Float(r)) if *r > 0. => Some(f64::from(*r)),
            _ => None,
        }).unwrap_or(30.);
        let mut this = Self { dmx, positions: BTreeMap::new(), rotations: BTreeMap::new(), length: 0., offset: 0., rate };
        fn track<const N: usize>(times: Vec<f64>, values: Vec<f64>, rate: f64) -> Option<Track<N>> {
            let values: Vec<[f64; N]> = values.chunks_exact(N).map(|c| std::array::from_fn(|k| c[k])).collect();
            if values.is_empty() {
                return None;
            }
            let times = if times.len() == values.len() { times } else { (0..values.len()).map(|i| i as f64 / rate).collect() };
            Some(Track { times, values })
        }
        let mut channels = vec![];
        for channel in this.dmx.elements.iter().filter(|e| e.kind == "DmeChannel") {
            let (Some(to), Some(attribute), Some(log)) = (
                channel.get("toElement").and_then(Value::as_ref_index),
                channel.get("toAttribute").and_then(Value::as_str),
                channel.get("log").and_then(Value::as_ref_index),
            ) else {
                continue;
            };
            channels.push((to, attribute.to_string(), log));
        }
        for (to, attribute, log) in channels {
            let Some((times, values)) = this.layer(log) else { continue };
            match attribute.as_str() {
                "position" => if let Some(t) = track::<3>(times, values, rate) { this.positions.insert(to, t); },
                "orientation" => if let Some(t) = track::<4>(times, values, rate) { this.rotations.insert(to, t); },
                _ => {}
            }
        }
        this.length = this.positions.values().map(|t| *t.times.last().unwrap()).chain(this.rotations.values().map(|t| *t.times.last().unwrap())).fold(0., f64::max);
        if let Some((start, end)) = range {
            this.offset = (start / rate).min(this.length);
            this.length = ((end - start) / rate).min(this.length - this.offset);
        }
        Some(this)
    }
    fn element(&self, index: i32) -> Option<&deadlock_dmx::Element> {
        usize::try_from(index).ok().and_then(|i| self.dmx.elements.get(i))
    }
    fn floats(&self, value: &Value, count: usize) -> Option<Vec<f64>> {
        let bytes = self.dmx.value_bytes(value)?;
        (bytes.len() >= count * 4).then(|| bytes[..count * 4].chunks_exact(4).map(|c| f64::from(f32::from_le_bytes([c[0], c[1], c[2], c[3]]))).collect())
    }
    fn refs(&self, value: Option<&Value>) -> Vec<i32> {
        match value {
            Some(Value::ElementRef(i)) => vec![*i],
            Some(v) => self.dmx.integer_values(v).unwrap_or_default(),
            None => vec![],
        }
    }
    /// The first layer of a log: its key times (DmeTime ticks of 1/10000 s) and values.
    fn layer(&self, log: i32) -> Option<(Vec<f64>, Vec<f64>)> {
        let log = self.element(log)?;
        let layer = match log.get("layers") {
            Some(layers) => self.element(*self.refs(Some(layers)).first()?)?,
            None => log,
        };
        let values = self.dmx.value_bytes(layer.get("values")?).map(|b| b.chunks_exact(4).map(|c| f64::from(f32::from_le_bytes([c[0], c[1], c[2], c[3]]))).collect()).unwrap_or_default();
        let times = layer.get("times").and_then(|t| self.dmx.integer_values(t).ok()).map(|t| t.into_iter().map(|v| f64::from(v) / 10_000.).collect()).unwrap_or_default();
        Some((times, values))
    }
    /// Root joints of the clip's DmeModel.
    fn roots(&self) -> Option<Vec<i32>> {
        let model = self.dmx.elements.iter().find(|e| e.kind == "DmeModel")?;
        Some(self.refs(model.get("children")))
    }
    /// The animated local transform of a DmeTransform `t` seconds after the
    /// clip's start (its first key, plus the start frame of a newer clip).
    fn animated(&self, transform: i32, t: f64) -> (Option<[f64; 3]>, Option<[f64; 4]>) {
        (
            self.positions.get(&transform).map(|k| k.at(k.times[0] + self.offset + t)),
            self.rotations.get(&transform).map(|k| k.at(k.times[0] + self.offset + t)),
        )
    }
    /// The local transform of a joint's DmeTransform at `t`: animated, else its rest value.
    fn local(&self, transform: Option<i32>, t: f64) -> ([f64; 3], [f64; 4]) {
        let (position, rotation) = transform.map_or((None, None), |tr| self.animated(tr, t));
        let rest = transform.and_then(|tr| self.element(tr));
        (
            position.or_else(|| rest.and_then(|r| r.get("position")).and_then(|v| self.floats(v, 3)).map(|v| [v[0], v[1], v[2]])).unwrap_or([0.; 3]),
            rotation.or_else(|| rest.and_then(|r| r.get("orientation")).and_then(|v| self.floats(v, 4)).map(|v| [v[0], v[1], v[2], v[3]])).unwrap_or([0., 0., 0., 1.]),
        )
    }
    /// The DmeTransform of each joint, by joint name.
    fn transforms(&self) -> BTreeMap<String, i32> {
        self.dmx.elements.iter().filter(|e| e.kind == "DmeJoint").filter_map(|j| Some((j.name.clone(), j.get("transform").and_then(Value::as_ref_index)?))).collect()
    }
}

fn quaternion_product(a: [f64; 4], b: [f64; 4]) -> [f64; 4] {
    let ([ax, ay, az, aw], [bx, by, bz, bw]) = (a, b);
    [
        aw * bx + ax * bw + ay * bz - az * by,
        aw * by - ax * bz + ay * bw + az * bx,
        aw * bz + ax * by - ay * bx + az * bw,
        aw * bw - ax * bx - ay * by - az * bz,
    ]
}

/// Model-space joint transforms of a clip sampled `fps` times a second (at
/// most `limit` frames), and its length in seconds.
pub fn frames(clip: &Path, fps: f64, limit: usize) -> Option<(f64, Vec<BTreeMap<String, Matrix4>>)> {
    frames_layered(clip, None, fps, limit)
}

/// `frames`, with an additive clip played on top: its local transforms are
/// added to the base clip's (positions added, rotations composed) joint by
/// joint, as the game plays a landing impact over the stand. The length is the
/// additive clip's; the base loops under it.
pub fn frames_layered(clip: &Path, additive: Option<&Path>, fps: f64, limit: usize) -> Option<(f64, Vec<BTreeMap<String, Matrix4>>)> {
    sample(clip, additive.map(|a| (a, None)), fps, limit)
}

/// `frames` of `clip` with a "RelativeToFrame" additive clip played on top:
/// each joint's change from frame `reference` of the layer's own take
/// (positions as differences, rotations as reference⁻¹ · rotation) is applied
/// to the base clip's local transform. The decompiled take of such a clip
/// already holds the change from its base frame for most joints, but
/// `root_motion` and the `pelvis` under it carry constant rotations (the axis
/// turn and its inverse) that are not part of the change; taking the change
/// from the reference frame leaves exactly the authored motion. None when the
/// layer cannot be read.
pub fn frames_relative(clip: &Path, layer: &Path, reference: usize, fps: f64, limit: usize) -> Option<(f64, Vec<BTreeMap<String, Matrix4>>)> {
    sample(clip, Some((layer, Some(reference))), fps, limit)
}

/// The time (seconds into the layer clip) at which a "RelativeToFrame"
/// additive clip leans the body most when played on `over` (the clip it
/// will be held on): the largest turn of the pelvis-to-head line of `over`'s
/// first frame with the layer on top, against that frame without it (a lean
/// clip is a one-shot curve that rises to its full lean and eases back).
/// Without a pelvis and a head, the largest sum over the layer's joints of the
/// rotation from frame `reference`. None when a clip cannot be read.
pub fn layer_peak(layer: &Path, reference: usize, over: &Path) -> Option<f64> {
    let tracks = ClipTracks::load(layer)?;
    let steps = (tracks.length * tracks.rate).round().max(0.) as usize;
    let times: Vec<f64> = (0..=steps).map(|f| f as f64 / tracks.rate).collect();
    let line = |pose: &BTreeMap<String, Matrix4>| {
        let find = |name: &str| pose.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, m)| [m[0][3], m[1][3], m[2][3]]);
        let (p, h) = (find("pelvis")?, find("head")?);
        let d = [h[0] - p[0], h[1] - p[1], h[2] - p[2]];
        let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        (l > 1e-6).then(|| d.map(|v| v / l))
    };
    let plain = frames(over, 30., 1).and_then(|(_, p)| p.into_iter().next());
    if let Some(rest) = plain.as_ref().and_then(line) {
        let mut best: Option<(f64, f64)> = None;
        for &t in &times {
            let Some(d) = frames_held(over, layer, reference, t, 1., 30., 1).and_then(|(_, p)| p.first().and_then(line)) else { continue };
            let angle = (d[0] * rest[0] + d[1] * rest[1] + d[2] * rest[2]).clamp(-1., 1.).acos();
            if best.is_none_or(|(_, b)| angle > b) {
                best = Some((t, angle));
            }
        }
        if best.is_some() {
            return best.map(|(t, _)| t);
        }
    }
    let transforms = tracks.transforms();
    let at_reference = reference as f64 / tracks.rate - tracks.offset;
    let mut best: Option<(f64, f64)> = None;
    for &t in &times {
        let total: f64 = transforms.values().filter_map(|&tr| {
            let (_, q) = tracks.animated(tr, t);
            let (_, r) = tracks.animated(tr, at_reference);
            let ([x, y, z, w], q) = (r?, q?);
            let d = quaternion_product([-x, -y, -z, w], q);
            let n = d.iter().map(|v| v * v).sum::<f64>().sqrt().max(1e-12);
            Some(2. * (d[3].abs() / n).min(1.).acos())
        }).sum();
        if best.is_none_or(|(_, b)| total > b) {
            best = Some((t, total));
        }
    }
    best.map(|(t, _)| t)
}

/// `frames` of `clip` (its whole length) with a "RelativeToFrame" additive
/// clip held at `held` seconds of its own time and scaled by `weight` (0 plays
/// the clip as it is, 1 the layer's full change): each joint's change from
/// frame `reference` of the layer's take, positions as differences scaled by
/// `weight`, rotations as reference⁻¹ · rotation taken `weight` of the way from
/// no turn. The jump's landing layer goes through `frames_relative`, not here.
pub fn frames_held(clip: &Path, layer: &Path, reference: usize, held: f64, weight: f64, fps: f64, limit: usize) -> Option<(f64, Vec<BTreeMap<String, Matrix4>>)> {
    let base = ClipTracks::load(clip)?;
    let layer = ClipTracks::load(layer)?;
    let names = layer.transforms();
    let at_reference = reference as f64 / layer.rate - layer.offset;
    let count = ((base.length * fps).floor() as usize + 1).min(limit.max(1));
    let roots = base.roots()?;
    let mut out = vec![];
    for frame in 0..count {
        let t = frame as f64 / fps;
        let mut pose = BTreeMap::new();
        let mut seen = BTreeSet::new();
        let mut stack: Vec<(i32, Matrix4)> = roots.iter().map(|&c| (c, identity())).collect();
        while let Some((index, parent)) = stack.pop() {
            if !seen.insert(index) {
                continue;
            }
            let Some(joint) = base.element(index) else { continue };
            let transform = joint.get("transform").and_then(Value::as_ref_index);
            let (mut position, mut rotation) = base.local(transform, t);
            if let Some(&tr) = names.get(&joint.name) {
                (position, rotation) = weighted_local((position, rotation), layer.animated(tr, held), layer.animated(tr, at_reference), weight);
            }
            let world = multiply(&parent, &local(position, rotation, 1.));
            if joint.kind == "DmeJoint" {
                pose.insert(joint.name.clone(), world);
            }
            stack.extend(base.refs(joint.get("children")).into_iter().map(|c| (c, world)));
        }
        if pose.is_empty() {
            return None;
        }
        out.push(pose);
    }
    Some((base.length, out))
}

/// A base joint's local transform with `weight` of a layer's change from its
/// reference values on top (see `frames_held`).
fn weighted_local(base: ([f64; 3], [f64; 4]), at: (Option<[f64; 3]>, Option<[f64; 4]>), reference: (Option<[f64; 3]>, Option<[f64; 4]>), weight: f64) -> ([f64; 3], [f64; 4]) {
    let (mut position, mut rotation) = base;
    if let Some(p) = at.0 {
        let r = reference.0.unwrap_or([0.; 3]);
        position = std::array::from_fn(|k| position[k] + (p[k] - r[k]) * weight);
    }
    if let Some(q) = at.1 {
        let mut delta = match reference.1 {
            Some([x, y, z, w]) => quaternion_product([-x, -y, -z, w], q),
            None => q,
        };
        if delta[3] < 0. {
            delta = delta.map(|v| -v);
        }
        let mixed: [f64; 4] = std::array::from_fn(|k| (if k == 3 { 1. } else { 0. }) * (1. - weight) + delta[k] * weight);
        let n = mixed.iter().map(|v| v * v).sum::<f64>().sqrt().max(1e-12);
        rotation = quaternion_product(rotation, mixed.map(|v| v / n));
    }
    (position, rotation)
}

/// The local transform of a base joint with an additive layer's values at `t`
/// on top: `reference` (the layer's values at its reference frame) given, the
/// change from it; without, the layer's values themselves (positions added,
/// rotations composed after the base rotation).
fn layered_local(base: ([f64; 3], [f64; 4]), at: (Option<[f64; 3]>, Option<[f64; 4]>), reference: Option<(Option<[f64; 3]>, Option<[f64; 4]>)>) -> ([f64; 3], [f64; 4]) {
    let (mut position, mut rotation) = base;
    let (reference_position, reference_rotation) = reference.unwrap_or((None, None));
    if let Some(p) = at.0 {
        let r = reference_position.unwrap_or([0.; 3]);
        position = std::array::from_fn(|k| position[k] + p[k] - r[k]);
    }
    if let Some(q) = at.1 {
        let delta = match reference_rotation {
            Some([x, y, z, w]) => quaternion_product([-x, -y, -z, w], q),
            None => q,
        };
        rotation = quaternion_product(rotation, delta);
    }
    (position, rotation)
}

fn sample(clip: &Path, additive: Option<(&Path, Option<usize>)>, fps: f64, limit: usize) -> Option<(f64, Vec<BTreeMap<String, Matrix4>>)> {
    let base = ClipTracks::load(clip)?;
    let reference = additive.and_then(|(_, r)| r);
    let layer = additive.and_then(|(a, _)| ClipTracks::load(a));
    if reference.is_some() && layer.is_none() {
        return None;
    }
    let layer_transforms = layer.as_ref().map(|l| l.transforms());
    let length = layer.as_ref().map_or(base.length, |l| l.length);
    let count = ((length * fps).floor() as usize + 1).min(limit.max(1));
    let roots = base.roots()?;
    let mut out = vec![];
    for frame in 0..count {
        let t = frame as f64 / fps;
        let base_t = if layer.is_some() && base.length > 0. { t % base.length } else { t };
        let mut pose = BTreeMap::new();
        let mut seen = BTreeSet::new();
        let mut stack: Vec<(i32, Matrix4)> = roots.iter().map(|&c| (c, identity())).collect();
        while let Some((index, parent)) = stack.pop() {
            if !seen.insert(index) {
                continue;
            }
            let Some(joint) = base.element(index) else { continue };
            let transform = joint.get("transform").and_then(Value::as_ref_index);
            let (mut position, mut rotation) = base.local(transform, base_t);
            if let (Some(layer), Some(names)) = (&layer, &layer_transforms) {
                if let Some(&tr) = names.get(&joint.name) {
                    // The reference frame counts from the take's start, not the clip's.
                    let at_reference = reference.map(|r| layer.animated(tr, r as f64 / layer.rate - layer.offset));
                    (position, rotation) = layered_local((position, rotation), layer.animated(tr, t), at_reference);
                }
            }
            let world = multiply(&parent, &local(position, rotation, 1.));
            if joint.kind == "DmeJoint" {
                pose.insert(joint.name.clone(), world);
            }
            stack.extend(base.refs(joint.get("children")).into_iter().map(|c| (c, world)));
        }
        if pose.is_empty() {
            return None;
        }
        out.push(pose);
    }
    Some((length, out))
}

fn local(position: [f64; 3], rotation: [f64; 4], scale: f64) -> Matrix4 {
    let n = rotation.iter().map(|v| v * v).sum::<f64>().sqrt().max(1e-12);
    let [x, y, z, w] = rotation.map(|v| v / n);
    let r = [
        [1. - 2. * (y * y + z * z), 2. * (x * y - z * w), 2. * (x * z + y * w)],
        [2. * (x * y + z * w), 1. - 2. * (x * x + z * z), 2. * (y * z - x * w)],
        [2. * (x * z - y * w), 2. * (y * z + x * w), 1. - 2. * (x * x + y * y)],
    ];
    let mut m = [[0.; 4]; 4];
    for i in 0..3 {
        for j in 0..3 {
            m[i][j] = r[i][j] * scale;
        }
        m[i][3] = position[i];
    }
    m[3][3] = 1.;
    m
}

fn inverse_rigid_scaled(m: &Matrix4) -> Option<Matrix4> {
    // Rotation times a uniform scale: the inverse is the transpose over scale².
    let s2 = (0..3).map(|i| m[i][0] * m[i][0]).sum::<f64>();
    if s2 <= 1e-18 {
        return None;
    }
    let mut out = [[0.; 4]; 4];
    for i in 0..3 {
        for j in 0..3 {
            out[i][j] = m[j][i] / s2;
        }
        out[i][3] = -(0..3).map(|k| out[i][k] * m[k][3]).sum::<f64>();
    }
    out[3][3] = 1.;
    Some(out)
}

/// Rest-to-stance skin matrix of every bone: bones in the clip take its
/// first-frame transform, the others follow their parent at their rest offset.
pub fn skin_matrices(target: &[PreparedBone], held: &BTreeMap<String, Matrix4>) -> BTreeMap<String, Matrix4> {
    let mut rest: BTreeMap<&str, Matrix4> = BTreeMap::new();
    let mut posed: BTreeMap<&str, Matrix4> = BTreeMap::new();
    let mut out = BTreeMap::new();
    for bone in target {
        let bind = local(bone.local_position, bone.local_rotation, bone.local_scale);
        let parent_rest = bone.parent.as_deref().and_then(|p| rest.get(p)).copied();
        let parent_posed = bone.parent.as_deref().and_then(|p| posed.get(p)).copied();
        let rest_world = parent_rest.map_or(bind, |p| multiply(&p, &bind));
        let posed_world = held.get(&bone.name).copied().unwrap_or_else(|| parent_posed.map_or(bind, |p| multiply(&p, &bind)));
        if let Some(inverse) = inverse_rigid_scaled(&rest_world) {
            out.insert(bone.name.clone(), multiply(&posed_world, &inverse));
        }
        rest.insert(&bone.name, rest_world);
        posed.insert(&bone.name, posed_world);
    }
    out
}

/// Model-space transform of every bone of `target` in a pose, as
/// `skin_matrices` places it: a joint of the pose where the pose has it, any
/// other bone following its parent at its rest offset (`target` lists parents
/// first).
pub fn posed_joints(target: &[PreparedBone], held: &BTreeMap<String, Matrix4>) -> BTreeMap<String, Matrix4> {
    let mut posed: BTreeMap<String, Matrix4> = BTreeMap::new();
    for bone in target {
        let bind = local(bone.local_position, bone.local_rotation, bone.local_scale);
        let parent_posed = bone.parent.as_deref().and_then(|p| posed.get(p)).copied();
        let world = held.get(&bone.name).copied().unwrap_or_else(|| parent_posed.map_or(bind, |p| multiply(&p, &bind)));
        posed.insert(bone.name.clone(), world);
    }
    posed
}

/// Cloth-simulated bones (coats, skirts, straps) take the skin matrix of their
/// nearest ancestor that is not simulated. The game moves them with its cloth
/// solver, their clip values are not a pose, and their compiled bind can differ
/// from the one the mesh was skinned with (Yamato's skirt and Abrams' coat
/// turned into spikes and slabs). Following the ancestor keeps their rest shape.
pub fn follow_unsimulated(mut skin: BTreeMap<String, Matrix4>, target: &[PreparedBone], simulated: &BTreeSet<String>) -> BTreeMap<String, Matrix4> {
    if simulated.is_empty() {
        return skin;
    }
    let parents: BTreeMap<&str, Option<&str>> = target.iter().map(|b| (b.name.as_str(), b.parent.as_deref())).collect();
    for bone in target.iter().filter(|b| simulated.contains(&b.name)) {
        let mut at = bone.parent.as_deref();
        while let Some(name) = at.filter(|n| simulated.contains(*n)) {
            at = parents.get(name).copied().flatten();
        }
        let carrier = at.and_then(|n| skin.get(n)).copied().unwrap_or_else(identity);
        skin.insert(bone.name.clone(), carrier);
    }
    skin
}

/// Skin weights of one decompiled mesh with implausible bones taken out: a bone
/// whose main vertices lie on average further from it than `reach` is a wrong
/// index in the decompiled mesh (thousands of Abrams' coat vertices weighted to
/// `muzzle_fx`, 59 inches away). Those weights are dropped; a vertex left
/// without weight follows its nearest bone. For the holding pose view only.
pub fn repair_weights(points: &[[f64; 3]], rows: &mut [Vec<(String, f64)>], bones: &BTreeMap<String, [f64; 3]>, reach: f64) -> usize {
    let distance = |a: [f64; 3], b: [f64; 3]| (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f64>().sqrt();
    let mut spread: BTreeMap<&str, (f64, usize)> = BTreeMap::new();
    for (point, row) in points.iter().zip(rows.iter()) {
        for (name, _) in row.iter().filter(|(_, w)| *w >= 0.5) {
            if let Some(bone) = bones.get(name) {
                let entry = spread.entry(name.as_str()).or_default();
                entry.0 += distance(*point, *bone);
                entry.1 += 1;
            }
        }
    }
    let wrong: BTreeSet<String> = spread.into_iter().filter(|(_, (sum, n))| sum / *n as f64 > reach).map(|(name, _)| name.to_string()).collect();
    if wrong.is_empty() {
        return 0;
    }
    let mut repaired = 0;
    for (point, row) in points.iter().zip(rows.iter_mut()) {
        if !row.iter().any(|(name, _)| wrong.contains(name)) {
            continue;
        }
        repaired += 1;
        row.retain(|(name, _)| !wrong.contains(name));
        if row.is_empty() {
            if let Some((name, _)) = bones.iter().filter(|(n, _)| !wrong.contains(*n)).min_by(|a, b| distance(*point, *a.1).total_cmp(&distance(*point, *b.1))) {
                row.push((name.clone(), 1.));
            }
        }
    }
    repaired
}

/// A point moved by its skin weights (unweighted points stay put).
pub fn skin_point(point: [f64; 3], weights: &[(String, f64)], skin: &BTreeMap<String, Matrix4>) -> [f64; 3] {
    let total: f64 = weights.iter().filter(|(n, _)| skin.contains_key(n)).map(|(_, w)| w).sum();
    if total <= 1e-9 {
        return point;
    }
    let mut out = [0.; 3];
    for (name, weight) in weights {
        let Some(m) = skin.get(name) else { continue };
        for i in 0..3 {
            out[i] += weight / total * (m[i][0] * point[0] + m[i][1] * point[1] + m[i][2] * point[2] + m[i][3]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    fn bone(name: &str, parent: Option<&str>, position: [f64; 3]) -> PreparedBone {
        PreparedBone {
            name: name.into(),
            parent: parent.map(Into::into),
            local_position: position,
            local_rotation: [0., 0., 0., 1.],
            local_scale: 1.,
            position: [0.; 3],
            matrix: [[0.; 4]; 4],
            inverse_bind_matrix: [[0.; 4]; 4],
            length: 0.,
        }
    }
    #[test]
    fn the_clip_with_the_gun_ready_is_found_wherever_the_hero_keeps_its_clips() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path();
        let clip = |path: &Path| {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, b"").unwrap();
            std::fs::write(path.with_extension("vnmclip"), b"").unwrap();
        };
        let model = base.join("models/heroes_wip/priest");
        let clips = model.join("clips");
        for name in ["crossbow_item_stand_idle", "crossbow_stand_idle", "reload_stand_idle", "crouch_stand_idle"] {
            clip(&clips.join(format!("{name}.dmx")));
        }
        // An older-style animation with a better name loses to an AnimGraph2 clip.
        std::fs::write(model.join("priest_weapon_stand_idle.dmx"), b"").unwrap();
        assert_eq!(find_clip(base, &model).unwrap(), clips.join("crossbow_stand_idle.dmx"));
        // The hero's own folder wins; other folders of its sources come next
        // (Geist's clips are in heroes_staging/ghost).
        let ghost = base.join("models/heroes_staging/ghost/clips/weapon_stand_idle.dmx");
        clip(&ghost);
        assert_eq!(find_clip(base, &model).unwrap(), clips.join("crossbow_stand_idle.dmx"));
        assert_eq!(find_clip(base, &base.join("models/heroes_wip/geist")).unwrap(), ghost);
        // Warden's `clip` folder and Sinclair's dmx/animation/ag2.
        let sinclair = base.join("models/heroes_staging/magician_v2");
        clip(&sinclair.join("dmx/animation/ag2/weapon_stand_idle.dmx"));
        std::fs::write(sinclair.join("magician_primary_stand_idle.dmx"), b"").unwrap();
        assert_eq!(find_clip(base, &sinclair).unwrap(), sinclair.join("dmx/animation/ag2/weapon_stand_idle.dmx"));
        let warden = base.join("models/heroes_staging/warden");
        clip(&warden.join("clip/weapon_stand_idle.dmx"));
        assert_eq!(find_clip(base, &warden).unwrap(), warden.join("clip/weapon_stand_idle.dmx"));
        // Older sources without AnimGraph2 clips use the hero's own animations.
        let old = tempfile::tempdir().unwrap();
        let haze = old.path().join("models/heroes_staging/haze");
        std::fs::create_dir_all(&haze).unwrap();
        std::fs::write(haze.join("haze_primary_stand_idle.dmx"), b"").unwrap();
        std::fs::write(haze.join("haze_body.dmx"), b"").unwrap();
        assert_eq!(find_clip(old.path(), &haze).unwrap(), haze.join("haze_primary_stand_idle.dmx"));
        assert!(find_clip(&base.join("none"), &base.join("none")).is_none());
    }
    /// A `.vnmclip` as Source 2 Viewer 21.1 writes it.
    fn newer_clip(path: &Path, source: &str, range: Option<(u32, u32)>) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let range = range.map(|(s, e)| format!("\tm_nStartFrame = {s}\n\tm_nEndFrame = {e}\n")).unwrap_or_default();
        let text = format!("<!-- kv3 encoding:text:version{{e21c7f3c-8a33-41c5-9977-a76d3a32aa0d}} format:generic:version{{7412167c-06e9-4698-aff2-e63eb59037e7}} -->\n{{\n\t_class = \"CNmClipDocument\"\n\tm_nVersion = 1\n\tm_sourceFilename = \"{source}\"\n\tm_animationSkeletonName = \"models/x/x.vnmskel\"\n{range}\tm_bonesToSampleInModelSpace = [  ]\n\tm_eventTracks = [  ]\n}}\n");
        std::fs::write(path, text).unwrap();
    }
    fn touch(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"").unwrap();
    }
    #[test]
    fn newer_clips_are_found_through_the_take_their_vnmclip_names() {
        // From the game archive (21.1): the clip keeps its name, its take is at
        // the archive path it names and is not listed again on its own.
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path();
        let haze = base.join("models/heroes_staging/haze");
        newer_clip(&haze.join("clips/weapon_stand_idle.vnmclip"), "models/heroes_staging/haze/dmx/animation/primary_idle.dmx", None);
        touch(&haze.join("dmx/animation/primary_idle.dmx"));
        touch(&haze.join("haze_primary_stand_idle.dmx"));
        newer_clip(&haze.join("clips/reload_idle_quick.vnmclip"), "models/heroes_staging/haze/dmx/animation/primary_reload.dmx", Some((40, 60)));
        newer_clip(&haze.join("clips/reload_idle.vnmclip"), "models/heroes_staging/haze/dmx/animation/primary_reload.dmx", None);
        touch(&haze.join("dmx/animation/primary_reload.dmx"));
        // A take beside its own clip (Haze's corner leans) is that clip, once.
        newer_clip(&haze.join("clips/corner_lean_stand_idle.vnmclip"), "models/heroes_staging/haze/clips/corner_lean_stand_idle.dmx", None);
        touch(&haze.join("clips/corner_lean_stand_idle.dmx"));
        // A clip whose take was not written, or that names a path outside the sources, is no clip.
        newer_clip(&haze.join("clips/missing_stand_idle.vnmclip"), "models/heroes_staging/haze/dmx/animation/missing.dmx", None);
        newer_clip(&haze.join("clips/escape_stand_idle.vnmclip"), "../outside.dmx", None);
        touch(&base.join("outside.dmx"));
        assert_eq!(find_clip(base, &haze).unwrap(), haze.join("clips/weapon_stand_idle.vnmclip"));
        let all = find_ranked(base, &haze, &CLIPS, rank);
        assert_eq!(all, vec![haze.join("clips/weapon_stand_idle.vnmclip"), haze.join("clips/corner_lean_stand_idle.vnmclip"), haze.join("haze_primary_stand_idle.dmx")]);
        let mut files = vec![];
        animation_files(&base.join("models"), 8, &mut files);
        assert!(!files.iter().any(|(_, p)| p.ends_with("dmx/animation/primary_idle.dmx") || p.ends_with("clips/corner_lean_stand_idle.dmx")), "{files:?}");
        // Each clip of a shared take keeps its own frame range.
        let quick = clip_document(&haze.join("clips/reload_idle_quick.vnmclip")).unwrap();
        assert_eq!((quick.source, quick.range), (haze.join("dmx/animation/primary_reload.dmx"), Some((40., 60.))));
        assert_eq!(clip_document(&haze.join("clips/reload_idle.vnmclip")).unwrap().range, None);
        // From a single compiled model (21.1): the .vnmclip files sit beside
        // the .vmdl and the takes under another parent (Lady Geist, whose clips
        // name heroes_staging/ghost).
        let flat = tempfile::tempdir().unwrap();
        let geist = flat.path().join("models/heroes_wip/geist");
        touch(&geist.join("geist.vmdl"));
        newer_clip(&geist.join("weapon_stand_idle.vnmclip"), "models/heroes_staging/ghost/dmx/animation/primary_idle.dmx", None);
        touch(&flat.path().join("models/heroes_wip/dmx/animation/primary_idle.dmx"));
        newer_clip(&geist.join("reload_quick_stand_idle.vnmclip"), "models/heroes_staging/ghost/clips/reload_quick_stand_idle.dmx", None);
        touch(&geist.join("reload_quick_stand_idle.dmx"));
        touch(&geist.join("geist_primary_stand_idle.dmx"));
        assert_eq!(clip_document(&geist.join("weapon_stand_idle.vnmclip")).unwrap().source, flat.path().join("models/heroes_wip/dmx/animation/primary_idle.dmx"));
        assert_eq!(find_ranked(flat.path(), &geist, &CLIPS, rank), vec![geist.join("weapon_stand_idle.vnmclip"), geist.join("reload_quick_stand_idle.vnmclip"), geist.join("geist_primary_stand_idle.dmx")]);
        // An older .vnmclip (no _class) keeps the older rule: its .dmx beside it.
        let old = tempfile::tempdir().unwrap();
        let wraith = old.path().join("models/heroes_wip/wraith/clips");
        touch(&wraith.join("weapon_stand_idle.dmx"));
        std::fs::write(wraith.join("weapon_stand_idle.vnmclip"), "{\n\tm_sourceFilename = \"models/heroes_wip/wraith/dmx/animation/primary_idle.dmx\"\n}\n").unwrap();
        assert!(clip_document(&wraith.join("weapon_stand_idle.vnmclip")).is_none());
        assert_eq!(find_clip(old.path(), &old.path().join("models/heroes_wip/wraith")).unwrap(), wraith.join("weapon_stand_idle.dmx"));
    }
    #[test]
    fn the_clip_document_says_whether_a_clip_is_additive() {
        let temp = tempfile::tempdir().unwrap();
        let clips = temp.path().join("models/heroes_staging/ghost/clips");
        let document = |name: &str, fields: &str, class: bool| {
            std::fs::create_dir_all(&clips).unwrap();
            let class = if class { "\t_class = \"CNmClipDocument\"\n" } else { "" };
            let text = format!("<!-- kv3 encoding:text:version{{e21c7f3c-8a33-41c5-9977-a76d3a32aa0d}} format:generic:version{{7412167c-06e9-4698-aff2-e63eb59037e7}} -->\n{{\n{class}\tm_sourceFilename = \"models/heroes_staging/ghost/dmx/animation/{name}.dmx\"\n{fields}\tm_eventTracks = [  ]\n}}\n");
            let path = clips.join(format!("{name}.vnmclip"));
            std::fs::write(&path, text).unwrap();
            path
        };
        let frame = |base: &str, index: i32| format!("\tm_additiveType = \"RelativeToFrame\"\n\tm_additiveBaseFilename = \"\"\n\tm_additiveBaseFrame = \"{base}\"\n\tm_nAdditiveBaseFrameIdx = {index}\n");
        // Lady Geist's landing (21.1): the change from a chosen frame.
        assert_eq!(additive_authoring(&document("jump_impact", &frame("UserSpecifiedFrame", 0), true)), Some(Additive::Frame(0)));
        assert_eq!(additive_authoring(&document("aim_weapon_idle", &frame("UserSpecifiedFrame", 30), true)), Some(Additive::Frame(30)));
        assert_eq!(additive_authoring(&document("landing_impact_idle", &frame("FirstFrame", 0), true)), Some(Additive::Frame(0)));
        assert!(matches!(additive_authoring(&document("bad_index", &frame("UserSpecifiedFrame", -1), true)), Some(Additive::Other(_))));
        assert!(matches!(additive_authoring(&document("bad_base", &frame("LastFrame", 0), true)), Some(Additive::Other(_))));
        // Other references cannot be rebuilt by the preview.
        let kind = |k: &str| format!("\tm_additiveType = \"{k}\"\n\tm_additiveBaseFilename = \"models/heroes_staging/haze/dmx/animation/idle_hideout.dmx\"\n\tm_additiveBaseFrame = \"FirstFrame\"\n\tm_nAdditiveBaseFrameIdx = -1\n");
        assert_eq!(additive_authoring(&document("aim_hideout_idle_up", &kind("RelativeToAnimation"), true)), Some(Additive::Other("RelativeToAnimation".into())));
        assert_eq!(additive_authoring(&document("corner_lean_idle", &kind("RelativeToSkeleton"), true)), Some(Additive::Other("RelativeToSkeleton".into())));
        assert_eq!(additive_authoring(&document("posed", "\tm_additiveType = \"None\"\n", true)), Some(Additive::Pose));
        // Clips that are not additive have no such field: the pose decides.
        assert_eq!(additive_authoring(&document("jump_ground", "", true)), None);
        // Such a document is still known as one (its take found), so the preview
        // can play it as a pose of its own.
        let take = temp.path().join("models/heroes_staging/ghost/dmx/animation/jump_ground.dmx");
        std::fs::create_dir_all(take.parent().unwrap()).unwrap();
        std::fs::write(&take, b"").unwrap();
        assert!(is_clip_document(&clips.join("jump_ground.vnmclip")));
        assert!(!is_clip_document(&take));
        // Older Source 2 Viewer: no _class, the clip is the .dmx beside its .vnmclip.
        document("jump_impact_old", &frame("FirstFrame", 0), false);
        assert!(!is_clip_document(&clips.join("jump_impact_old.vnmclip")));
        assert_eq!(additive_authoring(&clips.join("jump_impact_old.dmx")), Some(Additive::Frame(0)));
        // An older-style animation without a .vnmclip, or a missing file.
        assert_eq!(additive_authoring(&clips.join("geist_hard_landing.dmx")), None);
        assert_eq!(additive_authoring(&temp.path().join("none.vnmclip")), None);
    }
    /// Quaternion of a turn about an axis.
    fn turn(axis: [f64; 3], angle: f64) -> [f64; 4] {
        let n = axis.iter().map(|v| v * v).sum::<f64>().sqrt();
        let s = (angle / 2.).sin() / n;
        [axis[0] * s, axis[1] * s, axis[2] * s, (angle / 2.).cos()]
    }
    fn close(a: &Matrix4, b: &Matrix4) -> bool {
        (0..3).all(|i| (0..4).all(|j| (a[i][j] - b[i][j]).abs() < 1e-9))
    }
    #[test]
    fn a_relative_to_frame_layer_adds_only_the_change_from_its_reference_frame() {
        // As decompiled: root_motion carries the axis turn C, the pelvis under
        // it the inverse turn (a root in game), in the base clip and in the
        // additive take alike; the take's other joints hold the change.
        let c = [0.5, 0.5, 0.5, 0.5];
        let c_inverse = [-0.5, -0.5, -0.5, 0.5];
        let pose = turn([0., 0., 1.], 0.4); // the stand's pelvis, in game
        let dip = turn([0., 1., 0.], 0.3); // the landing's pelvis change
        let bend = turn([1., 0., 0.], 0.5); // the landing's spine change
        let base_root = ([0.; 3], c);
        let base_pelvis = ([0.6, 60., 2.5], quaternion_product(c_inverse, pose));
        let base_spine = ([4.7, 0., 0.], turn([0., 0., 1.], 0.1));
        // The take: frame 0 is the reference (no change), frame 1 the dip.
        let take = |f: usize| {
            let (d, b, drop) = if f == 0 { ([0., 0., 0., 1.], [0., 0., 0., 1.], 0.) } else { (dip, bend, -2.) };
            [(Some([0.; 3]), Some(c)), (Some([0., drop, 0.]), Some(quaternion_product(c_inverse, d))), (Some([0.; 3]), Some(b))]
        };
        let world = |locals: [([f64; 3], [f64; 4]); 3]| {
            let root = local(locals[0].0, locals[0].1, 1.);
            let pelvis = multiply(&root, &local(locals[1].0, locals[1].1, 1.));
            (root, pelvis, multiply(&pelvis, &local(locals[2].0, locals[2].1, 1.)))
        };
        let bases = [base_root, base_pelvis, base_spine];
        let layered = |f: usize, reference: Option<usize>| {
            let (at, refs) = (take(f), reference.map(take));
            world(std::array::from_fn(|k| layered_local(bases[k], at[k], refs.map(|r| r[k]))))
        };
        let (root, pelvis, spine) = world(bases);
        // At the reference frame the stand stays exactly as it is.
        let (r0, p0, s0) = layered(0, Some(0));
        assert!(close(&r0, &root) && close(&p0, &pelvis) && close(&s0, &spine));
        // Later frames: the root keeps its turn, the pelvis dips and turns by
        // the change in its own frame, the spine bends on top of the stand.
        let (r1, p1, s1) = layered(1, Some(0));
        assert!(close(&r1, &root));
        let expected_pelvis = multiply(&root, &local([0.6, 58., 2.5], quaternion_product(base_pelvis.1, dip), 1.));
        assert!(close(&p1, &expected_pelvis), "{p1:?}");
        assert!(close(&s1, &multiply(&expected_pelvis, &local(base_spine.0, quaternion_product(base_spine.1, bend), 1.))));
        // The take's values added as they are turn the whole body over (the
        // root's turn twice): what the preview played before.
        let (raw_root, raw_pelvis, _) = layered(0, None);
        assert!(!close(&raw_root, &root));
        assert!((raw_pelvis[2][3] - pelvis[2][3]).abs() > 30., "{raw_pelvis:?}");
    }
    #[test]
    fn a_held_layer_adds_its_weight_of_the_change_from_the_reference_frame() {
        let base = ([0.6, 60., 2.5], turn([0., 0., 1.], 0.4));
        let reference = (Some([0., 1., 0.]), Some(turn([1., 0., 0.], 0.2)));
        let lean = turn([0., 1., 0.], 0.6);
        let at = (Some([0., -5., 0.]), Some(quaternion_product(turn([1., 0., 0.], 0.2), lean)));
        let m = |(p, q): ([f64; 3], [f64; 4])| local(p, q, 1.);
        // Weight 0: the clip as it is, exactly.
        assert_eq!(weighted_local(base, at, reference, 0.), base);
        // Weight 1: the same as the jump's layering with that reference.
        assert!(close(&m(weighted_local(base, at, reference, 1.)), &m(layered_local(base, at, Some(reference)))));
        // Half the weight: half the drop and half the turn.
        let half = weighted_local(base, at, reference, 0.5);
        assert!((half.0[1] - 57.).abs() < 1e-12);
        assert!(close(&m(half), &m(([0.6, 57., 2.5], quaternion_product(base.1, turn([0., 1., 0.], 0.3))))));
        // A joint the layer does not animate keeps the clip's values.
        assert_eq!(weighted_local(base, (None, None), (None, None), 1.), base);
    }
    #[test]
    fn a_turned_shoulder_moves_the_hand_and_its_gun_with_it() {
        let target = vec![bone("root", None, [0., 0., 0.]), bone("arm", Some("root"), [0., 0., 10.]), bone("hand", Some("arm"), [5., 0., 0.])];
        // The clip turns the arm 90° around Z: the hand at (5, 0, 10) goes to (0, 5, 10).
        let h = std::f64::consts::FRAC_1_SQRT_2;
        let held = BTreeMap::from([("arm".to_string(), local([0., 0., 10.], [0., 0., h, h], 1.))]);
        let skin = skin_matrices(&target, &held);
        let moved = skin_point([5., 0., 10.], &[("hand".into(), 1.)], &skin);
        assert!((moved[0]).abs() < 1e-9 && (moved[1] - 5.).abs() < 1e-9 && (moved[2] - 10.).abs() < 1e-9, "{moved:?}");
        // A gun point 2 units in front of the hand turns with it.
        let gun = skin_point([7., 0., 10.], &[("hand".into(), 1.)], &skin);
        assert!((gun[1] - 7.).abs() < 1e-9, "{gun:?}");
        // Bones the clip does not move keep their rest place; unweighted points stay.
        assert_eq!(skin_point([1., 2., 3.], &[("root".into(), 1.)], &skin), [1., 2., 3.]);
        assert_eq!(skin_point([1., 2., 3.], &[], &skin), [1., 2., 3.]);
        // A simulated bone under the hand moves exactly with the hand (or with
        // the root when every ancestor is simulated).
        let mut cloth = target.clone();
        cloth.push(bone("strap", Some("hand"), [1., 0., 0.]));
        cloth.push(bone("strap_tip", Some("strap"), [1., 0., 0.]));
        let simulated = BTreeSet::from(["strap".to_string(), "strap_tip".to_string()]);
        let skin = follow_unsimulated(skin_matrices(&cloth, &held), &cloth, &simulated);
        assert_eq!(skin["strap_tip"], skin["hand"]);
        // A bone named for vertices far from it is a wrong index: they follow the nearest bone.
        let bones = BTreeMap::from([("root".to_string(), [0., 0., 0.]), ("hand".to_string(), [5., 0., 10.]), ("muzzle".to_string(), [60., 0., 10.])]);
        let points = [[5., 1., 10.], [5., 0., 11.], [0., 1., 0.]];
        let mut rows = vec![vec![("muzzle".to_string(), 1.)], vec![("hand".to_string(), 0.6), ("muzzle".to_string(), 0.4)], vec![("root".to_string(), 1.)]];
        assert_eq!(repair_weights(&points, &mut rows, &bones, 30.), 2);
        assert_eq!(rows, vec![vec![("hand".to_string(), 1.)], vec![("hand".to_string(), 0.6)], vec![("root".to_string(), 1.)]]);
        let all = BTreeSet::from(["root".to_string()]);
        assert_eq!(follow_unsimulated(skin_matrices(&cloth, &held), &cloth, &all)["root"], identity());
    }
}
