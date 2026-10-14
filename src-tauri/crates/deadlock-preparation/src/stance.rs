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

/// Animation files below `root`, a few levels deep: (is an AnimGraph2 clip,
/// path). Clips are a `.dmx` next to a `.vnmclip` of the same name; their
/// folders differ per hero (`clips`, Warden's `clip`, Sinclair's
/// `dmx/animation/ag2`). Other `.dmx` files are older-style animations or meshes.
fn animation_files(root: &Path, depth: usize, out: &mut Vec<(bool, PathBuf)>) {
    let Ok(entries) = std::fs::read_dir(root) else { return };
    for path in entries.filter_map(|e| e.ok()).map(|e| e.path()) {
        if path.is_dir() {
            if depth > 0 {
                animation_files(&path, depth - 1, out);
            }
        } else if path.extension().is_some_and(|x| x.eq_ignore_ascii_case("dmx")) {
            out.push((path.with_extension("vnmclip").is_file(), path));
        }
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
    /// Seconds to the last key.
    length: f64,
}
impl ClipTracks {
    fn load(clip: &Path) -> Option<Self> {
        let dmx = Dmx::load(clip).ok()?;
        let rate = dmx.elements.iter().find(|e| e.kind == "DmeChannelsClip").and_then(|c| match c.get("frameRate") {
            Some(Value::Integer(r)) if *r > 0 => Some(f64::from(*r)),
            Some(Value::Float(r)) if *r > 0. => Some(f64::from(*r)),
            _ => None,
        }).unwrap_or(30.);
        let mut this = Self { dmx, positions: BTreeMap::new(), rotations: BTreeMap::new(), length: 0. };
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
    /// The animated local transform of a DmeTransform `t` seconds after its first key.
    fn animated(&self, transform: i32, t: f64) -> (Option<[f64; 3]>, Option<[f64; 4]>) {
        (
            self.positions.get(&transform).map(|k| k.at(k.times[0] + t)),
            self.rotations.get(&transform).map(|k| k.at(k.times[0] + t)),
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
    let base = ClipTracks::load(clip)?;
    let layer = additive.and_then(ClipTracks::load);
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
                    let (dp, dq) = layer.animated(tr, t);
                    if let Some(dp) = dp {
                        position = [position[0] + dp[0], position[1] + dp[1], position[2] + dp[2]];
                    }
                    if let Some(dq) = dq {
                        rotation = quaternion_product(rotation, dq);
                    }
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
