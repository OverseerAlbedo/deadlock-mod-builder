//! Scale-aware source proxies and motion maps for independently identified parts.
use crate::{
    invalid,
    parts::{Kind, Part},
    secondary::Group,
    surface::{self, Editable},
    surface_geometry::{cross, dot, norm, sub, Surface},
    Result,
};
use deadlock_dmx::Dmx;
use deadlock_skeleton::{transforms::BindState, Skeleton};
use std::collections::BTreeMap;
pub(crate) type Maps = [f64; 8];
const MAPS: [&str; 8] = [
    "cloth_goal_strength$0",
    "cloth_drag$0",
    "cloth_antishrink$0",
    "cloth_gravity$0",
    "cloth_stray_radius$0",
    "cloth_stray_radius_stretchiness$0",
    "cloth_suspenders$0",
    "cloth_stretch$0",
];
/// Node gravity in the game's unit. Valve's own cloth (ModelDoc ClothChain,
/// `gravity_z = 1`) compiles to 360 per node in every hero, while the compiler
/// copies proxy streams as they are. The gravity scale used for tuning and the
/// movement preview (0 to 1.5) is therefore written multiplied by this; before
/// October 17, 2026 it was written as is, so automatic parts had almost no
/// gravity in game.
pub const GAME_GRAVITY: f64 = 360.;
fn unit(v: [f64; 3]) -> Result<[f64; 3]> {
    let n = norm(v);
    if n <= 1e-6 || !n.is_finite() {
        return Err(invalid("Moving part has a collapsed direction."));
    }
    Ok(v.map(|v| v / n))
}
pub(crate) fn parameters(
    kind: Kind,
    t: f64,
    root: bool,
    length: f64,
    radius: f64,
    choice: Option<&str>,
) -> Result<Maps> {
    if kind == Kind::Breast {
        return breast_parameters(length, choice);
    }
    if kind == Kind::Butt {
        return butt_parameters(length, choice);
    }
    if kind == Kind::Charm {
        return charm_parameters(t, root, length, choice);
    }
    let (mut goal, mut drag, gravity) = match kind {
        Kind::Hair => (if root { 1. } else { 0.6 - 0.2 * t }, 0., 0.),
        Kind::Ear => (if root { 1. } else { 0.6 - 0.2 * t }, 0., 0.),
        Kind::Cloth => (if root { 1. } else { 0.75 - 0.25 * t }, 0.15, 1.),
        Kind::LongSkirt => (if root { 1. } else { 0.12 - 0.09 * t }, 0.15, 1.),
        Kind::Streamer => (
            if root { 1. } else { 0.12 - 0.06 * t },
            if root { 0.38 } else { 0.18 },
            1.,
        ),
        Kind::Tail | Kind::Accessory => (0.65 - 0.30 * t, 0.15, 1.),
        Kind::Rigid | Kind::Breast | Kind::Butt | Kind::Charm => (1., 0., 0.),
    };
    match choice.unwrap_or("automatic") {
        "automatic" => {}
        "balanced" => {
            let p = crate::secondary::profile("balanced")?;
            goal = p.goal_at_hem + (p.goal_at_anchor - p.goal_at_hem) * (1. - t).powi(2);
            drag = p.drag;
        }
        "stiff" => {
            goal = (goal + 0.25).min(1.);
            drag = (drag + 0.05).min(1.);
        }
        "soft" => {
            goal = (goal * 0.7).max(0.01);
        }
        _ => return Err(invalid("Unknown part movement setting.")),
    }
    let travel = if kind == Kind::Streamer {
        if root {
            radius * 0.22
        } else {
            length * t * 0.60 + radius
        }
    } else {
        0.
    };
    // Return strength controls settling, not fabric length. Use the native
    // inextensible tethers as well so stronger attraction does not merely hide
    // accumulated chain stretch. Accepted reference presets bypass this path.
    let tether = if kind == Kind::Streamer { 1. } else { 0. };
    Ok([goal, drag, 1., gravity, travel, 0., tether, 0.])
}
/// The user's fine-tuning of a part type (Physics → Fine-tune movement), on top
/// of the automatic values of one moving node. Each setting goes from -1 (much
/// less) through 0 (automatic) to 1 (much more): stiffness moves the pose pull
/// toward 1 or down to a quarter, damping toward 0.5 or to none, gravity toward
/// 1.5 or to none, and swing scales a travel limit between a quarter and two
/// and a half times. Parts without a travel limit (0, free) stay free.
pub(crate) fn tune(maps: &mut Maps, tuning: &deadlock_project::customization::MotionTuning) {
    let toward = |value: f64, k: f64, high: f64, low_share: f64| {
        if k >= 0. { value + (high - value).max(0.) * k } else { value * (1. + (1. - low_share) * k) }
    };
    let k = |v: Option<f64>| v.unwrap_or(0.).clamp(-1., 1.);
    maps[0] = toward(maps[0], k(tuning.stiffness), 1., 0.25).clamp(0.01, 1.);
    maps[1] = toward(maps[1], k(tuning.damping), 0.5, 0.).clamp(0., 1.);
    maps[3] = toward(maps[3], k(tuning.gravity), 1.5, 0.).max(0.);
    if maps[4] > 0. {
        let s = k(tuning.swing);
        maps[4] *= if s >= 0. { 1. + 1.5 * s } else { 1. + 0.75 * s };
    }
}
/// Breast motion in the style of stylized action games: a visible, springy
/// bounce that lags the body and settles after a few swings, without sagging.
/// The travel limit (about half the breast radius) caps the swing so the pair
/// never folds into the chest. In game, the first tuning (pull 0.8, damping 0.5,
/// travel 0.3 in) barely moved. `size` is the breast radius in inches.
fn breast_parameters(size: f64, choice: Option<&str>) -> Result<Maps> {
    let (goal, drag, travel_scale) = match choice.unwrap_or("automatic") {
        "automatic" | "balanced" => (0.45, 0.1, 1.),
        "stiff" => (0.62, 0.15, 0.7),
        "soft" => (0.3, 0.06, 1.35),
        _ => return Err(invalid("Unknown part movement setting.")),
    };
    let travel = ((size * 0.45).clamp(0.4, 1.5) * travel_scale).min(2.);
    Ok([goal, drag, 1., 0., travel, 0., 0., 0.])
}
/// Butt motion: a small, quickly settling bounce, stiffer and shorter than the
/// breast bounce (a butt is held by the hips and moves less). No gravity, so it
/// never sags into the legs; the travel limit (about a quarter of its radius)
/// keeps it from sinking into the hips. `size` is the radius in inches. Needs an
/// in-game check.
fn butt_parameters(size: f64, choice: Option<&str>) -> Result<Maps> {
    let (goal, drag, travel_scale) = match choice.unwrap_or("automatic") {
        "automatic" | "balanced" => (0.6, 0.15, 1.),
        "stiff" => (0.75, 0.2, 0.7),
        "soft" => (0.45, 0.1, 1.3),
        _ => return Err(invalid("Unknown part movement setting.")),
    };
    let travel = ((size * 0.25).clamp(0.25, 0.8) * travel_scale).min(1.1);
    Ok([goal, drag, 1., 0., travel, 0., 0., 0.])
}
/// Strands with more weighted bones than this get their pose pull divided out:
/// every node is pulled back to its pose, so a 19-bone strand with the same
/// per-node pull is about four times stiffer than a 5-bone one (Yvonne's
/// accepted long ponytails have 5). Shorter strands keep their tuning exactly.
pub(crate) const DENSE_CHAIN_NODES: usize = 6;
/// Pose-pull multiplier for a strand with `nodes` weighted bones.
pub(crate) fn dense_chain_multiplier(nodes: usize) -> f64 {
    if nodes > DENSE_CHAIN_NODES { DENSE_CHAIN_NODES as f64 / nodes as f64 } else { 1. }
}
/// Streamers with more weighted joints than this get their drag divided out.
/// Drag slows every node, so a long, finely jointed streamer (SiViolet's
/// 30-joint sleeves) hung almost still for a second before it began to swing,
/// while TangTang's 7-joint streamers with the same drag swing freely. Valve's
/// own long cloth uses no drag at all.
pub(crate) const LONG_STREAMER_JOINTS: usize = 10;
/// Drag multiplier for a streamer with `joints` weighted joints.
pub(crate) fn streamer_drag_multiplier(joints: usize) -> f64 {
    if joints > LONG_STREAMER_JOINTS { LONG_STREAMER_JOINTS as f64 / joints as f64 } else { 1. }
}
/// Body vertices (skinned to bones outside every moving part), used to keep
/// long strands from swinging through the body: a node whose travel limit is
/// smaller than its distance to the body cannot reach the body.
pub(crate) struct Body {
    pub points: Vec<[f64; 3]>,
}
impl Body {
    pub(crate) fn clearance(&self, p: [f64; 3]) -> Option<f64> {
        self.points.iter().map(|q| norm(sub(*q, p))).min_by(f64::total_cmp)
    }
}
/// Travel limit for a long strand node, capped by its clearance to the body.
/// The floor (an inch, or 15% of the free travel) keeps tips that rest on the
/// body moving a little instead of pinning them.
pub(crate) fn body_capped_travel(free: f64, clearance: Option<f64>, radius: f64) -> f64 {
    match clearance {
        Some(c) if free > 0. => free.min((c - radius).max((free * 0.15).max(1.).min(free))),
        _ => free,
    }
}
/// A charm on a gun (keychain, pendant, strap): a light swing that lags the gun
/// and settles within a few swings. Its pivot stays on the gun, there is no
/// gravity, so it never sags into the gun when the gun is held sideways, and
/// the travel limit (about a third of the charm's reach) keeps the swing small.
/// `length` is the charm's reach from its pivot in inches. Needs an in-game check.
fn charm_parameters(t: f64, root: bool, length: f64, choice: Option<&str>) -> Result<Maps> {
    let (goal, drag, travel_scale) = match choice.unwrap_or("automatic") {
        "automatic" | "balanced" => (0.5 - 0.15 * t, 0.3, 1.),
        "stiff" => (0.7 - 0.15 * t, 0.4, 0.6),
        "soft" => (0.35 - 0.1 * t, 0.2, 1.4),
        _ => return Err(invalid("Unknown part movement setting.")),
    };
    let travel = ((length * 0.35).clamp(0.2, 1.2) * travel_scale).min(1.6);
    Ok([if root { 1. } else { goal }, drag, 1., 0., travel, 0., 0., 0.])
}
/// The bones of a part that get proxy points (the streamer and breast paths
/// choose their own). A part whose mesh hangs on one bone (a hair bun, a knot)
/// moves as that bone alone: the bones below it that carry no mesh (an MMD
/// helper chain such as SiViolet's wingB under each bun) get no points. Swinging
/// freely, they twisted the bun's small ring and flung the right bun around.
/// Every other part keeps all its bones, mesh or not.
pub(crate) fn simulated_bones<'a>(group: &'a Group, lookup: &BTreeMap<&str, usize>, sk: &Skeleton) -> Vec<&'a String> {
    if group.weighted_bones.len() != 1 {
        return group.bones.iter().collect();
    }
    // The weighted bone and the group bones above it.
    let members: std::collections::BTreeSet<usize> = group.bones.iter().map(|n| lookup[n.as_str()]).collect();
    let mut keep = std::collections::BTreeSet::new();
    let mut at = lookup[group.weighted_bones[0].as_str()] as i32;
    while at >= 0 && members.contains(&(at as usize)) {
        keep.insert(at as usize);
        at = sk.parents[at as usize];
    }
    group.bones.iter().filter(|n| keep.contains(&lookup[n.as_str()])).collect()
}
pub(crate) fn point(
    s: &mut Surface,
    maps: &mut Vec<Maps>,
    p: [f64; 3],
    bone: usize,
    enabled: bool,
    settings: Maps,
) {
    s.points.push(p);
    s.influences.push(BTreeMap::from([(bone, 1.)]));
    s.enabled.push(enabled);
    maps.push(settings);
}
fn ring(
    s: &mut Surface,
    maps: &mut Vec<Maps>,
    center: [f64; 3],
    x: [f64; 3],
    z: [f64; 3],
    r: f64,
    bone: usize,
    dynamic: bool,
    settings: Maps,
) -> [usize; 3] {
    let start = s.points.len();
    for a in [
        0.,
        std::f64::consts::TAU / 3.,
        std::f64::consts::TAU * 2. / 3.,
    ] {
        point(
            s,
            maps,
            std::array::from_fn(|k| center[k] + r * (a.cos() * x[k] + a.sin() * z[k])),
            bone,
            dynamic,
            settings,
        );
    }
    let ids = [start, start + 1, start + 2];
    s.faces.push(ids);
    ids
}
pub(crate) fn append(
    s: &mut Surface,
    group: &Group,
    part: &Part,
    slots: &BTreeMap<&str, usize>,
    lookup: &BTreeMap<&str, usize>,
    sk: &Skeleton,
    state: &BindState,
    choice: Option<&str>,
    skirt: Option<&crate::long_skirt::SkirtGroup>,
    maps: &mut Vec<Maps>,
    partner: Option<[f64; 3]>,
    body: Option<&Body>,
) -> Result<Option<[usize; 3]>> {
    if let Some(skirt) = skirt {
        return crate::skirt_surface::append(s,group,part,slots,lookup,sk,state,choice,skirt,maps).map(|_| None);
    }
    if matches!(part.kind, Kind::Breast | Kind::Butt) {
        return append_breast(s, group, part, slots, lookup, sk, state, choice, maps, partner).map(Some);
    }
    let pos = |name: &str| std::array::from_fn::<_, 3, _>(|r| state.world[lookup[name]][r][3]);
    let axis = |name: &str, c: usize| {
        std::array::from_fn::<_, 3, _>(|r| {
            state.world[lookup[name]][r][c] / state.accumulated[lookup[name]]
        })
    };
    let center = pos(&group.root);
    let pitch = if part.weighted_chain.len() > 1 {
        part.chain_length_inches / (part.weighted_chain.len() - 1) as f64
    } else {
        group
            .bones
            .iter()
            .map(|n| norm(sub(pos(n), center)))
            .fold(0.0, f64::max)
            .max(part.shape_radii_inches[0])
            .max(0.1)
    };
    let radius = (pitch
        * if part.kind == Kind::Streamer {
            0.12
        } else {
            0.08
        })
    .clamp(0.05, 0.45);
    let offset = (radius * 3.).max((pitch * 0.25).min(1.));
    let static_maps = [0., 0., 1., 0., 0., 0., 0., 0.];
    if part.kind == Kind::Streamer {
        let chain = &part.weighted_chain;
        let tangent = unit(sub(pos(chain.last().unwrap()), pos(&chain[0])))?;
        let horizontal = cross(tangent, [0., 0., -1.]);
        let width = if norm(horizontal) > 1e-5 {
            unit(horizontal)?
        } else {
            let x = axis(&chain[0], 0);
            unit(sub(x, tangent.map(|v| v * dot(x, tangent))))?
        };
        let normal = unit(cross(width, tangent))?;
        let anchor_center =
            std::array::from_fn::<_, 3, _>(|r| pos(&chain[0])[r] - tangent[r] * offset);
        let start = s.points.len();
        for (x, z) in [(-1., 0.), (1., 0.), (0., 1.)] {
            point(
                s,
                maps,
                std::array::from_fn(|k| anchor_center[k] + radius * (x * width[k] + z * normal[k])),
                slots[group.anchor.as_str()],
                false,
                static_maps,
            );
        }
        s.faces.push([start, start + 1, start + 2]);
        let mut previous = None;
        let mut traveled = 0.;
        for (level, name) in chain.iter().enumerate() {
            let p = pos(name);
            if level > 0 {
                traveled += norm(sub(p, pos(&chain[level - 1])));
            }
            let delta = if level + 1 < chain.len() {
                sub(pos(&chain[level + 1]), p)
            } else {
                let last = sub(p, pos(&chain[level - 1]));
                unit(last)?.map(|v| v * pitch)
            };
            // Three non-collinear points per bone retain the native compact
            // fitting representation used by the accepted streamer. The flat
            // open strip can hinge at each width edge; no closed tube braces it.
            let at = s.points.len();
            let t = (traveled / part.chain_length_inches).clamp(0., 1.);
            let mut settings = parameters(
                part.kind,
                t,
                level == 0,
                part.chain_length_inches,
                radius,
                choice,
            )?;
            if choice.is_none_or(|c| c == "automatic") {
                settings[1] *= streamer_drag_multiplier(chain.len());
            }
            for (fraction, sign) in [(0., -1.), (0., 1.), (0.5, -1.)] {
                point(
                    s,
                    maps,
                    std::array::from_fn(|r| p[r] + fraction * delta[r] + sign * radius * width[r]),
                    slots[name.as_str()],
                    true,
                    settings,
                );
            }
            s.faces.push([at, at + 1, at + 2]);
            if let Some(a) = previous {
                s.faces
                    .extend([[a + 1, at + 1, a + 2], [a + 2, at + 1, at]]);
            } else {
                s.faces.extend([
                    [start, at, at + 1],
                    [start, at + 1, start + 1],
                    [start + 1, at + 1, start + 2],
                ]);
            }
            previous = Some(at);
        }
    } else {
        let hair_coordinates = crate::hair_motion::coordinates(group, part, sk, state)?;
        let hair_length = hair_coordinates.as_ref().and_then(|_| {
            crate::hair_motion::length_adjustment(group, part, sk, state, choice)
        });
        let hair_limits = hair_length.as_ref()
            .map(|length| crate::hair_limits::plan(group, part, sk, state, length))
            .transpose()?;
        let x = axis(&group.root, 0);
        let y = axis(&group.root, 1);
        let z = axis(&group.root, 2);
        let dense = part.weighted_chain.len() > DENSE_CHAIN_NODES && matches!(part.kind, Kind::Hair | Kind::Tail | Kind::Accessory);
        let fixed_center: [f64; 3] = std::array::from_fn(|r| center[r] - y[r] * offset);
        let fixed = ring(
            s,
            maps,
            fixed_center,
            x,
            z,
            radius,
            slots[group.anchor.as_str()],
            false,
            static_maps,
        );
        let max_distance = group
            .bones
            .iter()
            .map(|n| norm(sub(pos(n), center)))
            .fold(0f64, f64::max)
            .max(pitch);
        let mut frames = BTreeMap::new();
        for name in simulated_bones(group, lookup, sk) {
            let i = lookup[name.as_str()];
            let p = pos(name);
            let t = skirt.map_or_else(|| hair_coordinates.as_ref().map_or_else(
                || (norm(sub(p, center)) / max_distance).clamp(0., 1.),
                |coordinates| coordinates[name],
            ), |skirt| skirt.coordinates[name]);
            let root = hair_coordinates.as_ref().map_or(name == &group.root, |_| t == 0.);
            let mut settings = parameters(
                part.kind,
                t,
                root,
                max_distance,
                radius,
                choice,
            )?;
            if !root {
                if let Some(adjustment) = &hair_length {
                    settings[0] *= adjustment.goal_strength_multiplier;
                }
                if dense && choice.is_none_or(|c| c == "automatic") {
                    settings[0] = (settings[0] * dense_chain_multiplier(part.weighted_chain.len())).max(0.03);
                }
            }
            // Long tails and accessory strands: tethers stop the chain from
            // stretching under gravity, and a travel limit along the strand
            // keeps a thin tail from flailing (zero would mean unbounded).
            if dense && matches!(part.kind, Kind::Tail | Kind::Accessory) && skirt.is_none() {
                settings[4] = (max_distance * t * 0.6).max(radius * 2.);
                settings[6] = 1.;
            }
            if let Some(limits) = &hair_limits {
                settings[4] = limits.travel_radius_by_bone[name];
                // Native suspenders connect back to fixed supports, preventing
                // length error accumulating through a long chain on impulses.
                // Zero stretchiness retains firm boundaries without adding
                // goal attraction, gravity, damping, or bend stiffness.
                settings[6] = 1.;
            }
            if let Some(skirt) = skirt {
                settings[4] = skirt.travel_radius_by_bone[name];
                settings[6] = 1.;
            }
            // Long, dense strands must not swing through the body.
            if dense && !root && skirt.is_none() && matches!(part.kind, Kind::Hair | Kind::Tail | Kind::Accessory) {
                settings[4] = body_capped_travel(settings[4], body.and_then(|b| b.clearance(p)), radius);
            }
            let from = if name == &group.root {
                fixed_center
            } else {
                pos(&sk.names[sk.parents[i] as usize])
            };
            let (ring_x, ring_z) = ring_axes(axis(name, 0), axis(name, 2), from, p);
            let ids = ring(
                s,
                maps,
                p,
                ring_x,
                ring_z,
                radius,
                slots[name.as_str()],
                true,
                settings,
            );
            let parent = if name == &group.root {
                fixed
            } else {
                *frames
                    .get(&sk.names[sk.parents[i] as usize])
                    .ok_or_else(|| invalid("Automatic secondary hierarchy is not parent-first."))?
            };
            connect(s, parent, ids);
            frames.insert(name.clone(), ids);
        }
    }
    Ok(None)
}
/// Breast proxy: a soft-body bounce rather than a dangling strand. The fixed
/// ring sits behind the breast toward the chest (not along the bone's own axis,
/// which points sideways on mirrored rigs and pulled both breasts inward). The
/// moving ring is sized to the breast so small node offsets do not become large
/// rotations. `partner` is the other breast on the same anchor, if any. Returns
/// the moving ring so a left/right pair can be linked.
fn append_breast(
    s: &mut Surface,
    group: &Group,
    part: &Part,
    slots: &BTreeMap<&str, usize>,
    lookup: &BTreeMap<&str, usize>,
    sk: &Skeleton,
    state: &BindState,
    choice: Option<&str>,
    maps: &mut Vec<Maps>,
    partner: Option<[f64; 3]>,
) -> Result<[usize; 3]> {
    let pos = |name: &str| std::array::from_fn::<_, 3, _>(|r| state.world[lookup[name]][r][3]);
    let bone = part.weighted_chain.first().unwrap_or(&group.root);
    let center = pos(bone);
    // Breast size from its weighted vertices; a sensible default otherwise.
    let size = if part.shape_radii_inches[0] > 0.3 { part.shape_radii_inches[0].clamp(0.8, 4.) } else { 1.5 };
    let anchor = pos(&group.anchor);
    let toward = sub(anchor, center);
    let up = [0., 0., 1.];
    let mut back = [toward[0], toward[1], 0.];
    // The chest anchor is on the midline; with the other breast known, keep only
    // the backward part so the spring does not lean toward the cleavage.
    if let Some(other) = partner {
        let lateral = [other[0] - center[0], other[1] - center[1], 0.];
        if norm(lateral) > size * 0.05 {
            let lateral = unit(lateral)?;
            back = sub(back, lateral.map(|v| v * dot(back, lateral)));
        }
    }
    if norm(back) < size * 0.05 {
        back = [0., 1., 0.];
    }
    let back = unit(back)?;
    let side = unit(cross(back, up))?;
    let lift = unit(cross(side, back))?;
    let radius = (size * 0.35).clamp(0.3, 1.2);
    let offset = (size * 0.6).clamp(0.5, 2.);
    let static_maps = [0., 0., 1., 0., 0., 0., 0., 0.];
    let fixed = ring(
        s,
        maps,
        std::array::from_fn(|k| center[k] + back[k] * offset),
        side,
        lift,
        radius,
        slots[group.anchor.as_str()],
        false,
        static_maps,
    );
    let mut settings = parameters(part.kind, 0., true, size, radius, choice)?;
    // Never swing far enough to reach the other breast (or cheek).
    if let Some(other) = partner {
        let gap = norm(sub(other, center));
        if gap > 1e-3 {
            settings[4] = settings[4].min(gap * 0.3);
        }
    }
    let moving = ring(s, maps, center, side, lift, radius, slots[bone.as_str()], true, settings);
    connect(s, fixed, moving);
    // Everything else in the group (ribbons or cape strips hanging from the
    // breast, an unweighted helper above it) still needs its own nodes, or it
    // would be frozen. It swings like a charm: a small bounded swing without
    // gravity, so a strip on the chest cannot sag or flap through the body.
    let mut frames = BTreeMap::from([(bone.clone(), moving)]);
    let others: Vec<&String> = group.bones.iter().filter(|n| *n != bone).collect();
    if !others.is_empty() {
        let parent_of = |name: &str| {
            let at = sk.parents[lookup[name]];
            (at >= 0).then(|| sk.names[at as usize].clone())
        };
        let reach = others.iter().map(|n| norm(sub(pos(n), center))).fold(0f64, f64::max).max(1e-3);
        let pitch = others
            .iter()
            .filter_map(|n| parent_of(n).filter(|p| lookup.contains_key(p.as_str())).map(|p| norm(sub(pos(n), pos(&p)))))
            .fold(0f64, f64::max)
            .max(0.1);
        let small = (pitch * 0.08).clamp(0.05, 0.45);
        let axis = |name: &str, c: usize| {
            std::array::from_fn::<_, 3, _>(|r| state.world[lookup[name]][r][c] / state.accumulated[lookup[name]])
        };
        for name in others {
            let t = (norm(sub(pos(name), center)) / reach).clamp(0., 1.);
            let maps_for = parameters(Kind::Charm, t, false, reach, small, choice.filter(|c| *c != "automatic"))?;
            let from = parent_of(name).filter(|p| lookup.contains_key(p.as_str())).map_or(center, |p| pos(&p));
            let (ring_x, ring_z) = ring_axes(axis(name, 0), axis(name, 2), from, pos(name));
            let ids = ring(s, maps, pos(name), ring_x, ring_z, small, slots[name.as_str()], true, maps_for);
            let parent = parent_of(name).and_then(|p| frames.get(&p).copied()).unwrap_or(fixed);
            connect(s, parent, ids);
            frames.insert(name.clone(), ids);
        }
    }
    Ok(moving)
}
/// Link the moving rings of a left/right breast pair on the same anchor, so the
/// two move together and keep their spacing instead of swinging into each other.
pub(crate) fn link_breast_pairs(s: &mut Surface, rings: &[(String, [usize; 3])]) {
    let mut by_anchor: BTreeMap<&str, Vec<[usize; 3]>> = BTreeMap::new();
    for (anchor, ring) in rings {
        by_anchor.entry(anchor.as_str()).or_default().push(*ring);
    }
    for pair in by_anchor.values().filter(|v| v.len() == 2) {
        connect(s, pair[0], pair[1]);
    }
}
/// The plane a bone's ring lies in: the bone's own X/Z frame, unless the chain
/// runs within that plane (rigs whose bone Y is not along the bone, such as MMD
/// tip bones). Then the ring would lie flat against its neighbour and its
/// connecting faces would collapse, so it is drawn across the chain instead.
/// Ordinary rigs keep exactly their own frame.
pub(crate) fn ring_axes(x: [f64; 3], z: [f64; 3], from: [f64; 3], at: [f64; 3]) -> ([f64; 3], [f64; 3]) {
    let step = sub(at, from);
    let (length, normal) = (norm(step), cross(x, z));
    if length <= 1e-6 || norm(normal) <= 1e-9 {
        return (x, z);
    }
    let along = step.map(|v| v / length);
    if (dot(along, normal) / norm(normal)).abs() >= 0.5 {
        return (x, z);
    }
    let mut across = sub(x, along.map(|v| v * dot(x, along)));
    if norm(across) < 0.3 {
        across = sub(z, along.map(|v| v * dot(z, along)));
    }
    let across = across.map(|v| v / norm(across));
    (across, cross(along, across))
}
pub(crate) fn connect(s: &mut Surface, a: [usize; 3], b: [usize; 3]) {
    let cost = |k: usize| {
        (0..3)
            .map(|i| norm(sub(s.points[a[i]], s.points[b[(i + k) % 3]])).powi(2))
            .sum::<f64>()
    };
    // The closest pairing, but never one whose faces collapse to a line.
    let area = |p: usize, q: usize, r: usize| norm(cross(sub(s.points[q], s.points[p]), sub(s.points[r], s.points[p])));
    let flat = |k: usize| {
        let scale = cost(k).max(1e-12);
        (0..3).any(|i| {
            let j = (i + 1) % 3;
            area(a[i], b[(i + k) % 3], b[(j + k) % 3]) <= 1e-9 * scale || area(a[i], b[(j + k) % 3], a[j]) <= 1e-9 * scale
        })
    };
    let shift = (0..3)
        .filter(|&k| !flat(k))
        .min_by(|&p, &q| cost(p).total_cmp(&cost(q)))
        .unwrap_or_else(|| (0..3).min_by(|&p, &q| cost(p).total_cmp(&cost(q))).unwrap());
    let b: [usize; 3] = std::array::from_fn(|i| b[(i + shift) % 3]);
    for i in 0..3 {
        let j = (i + 1) % 3;
        s.faces.extend([[a[i], b[i], b[j]], [a[i], b[j], a[j]]]);
    }
}
/// Writes the motion streams of every proxy point, and one all-zero
/// `cloth_vertex_set_<name>` stream per name in `hero_sets`. The compiler turns
/// each into a named vertex map with no nodes in it. The replaced hero's own
/// animations stiffen cloth by those names (Violet's run start, jumps, dashes
/// and slides fire `AE_CL_CLOTH_STIFFEN` on `pony_tail_vm`, `apron_*_vm` and
/// more); without a map of that name the game stiffened the whole replacement
/// cloth, which then stood still for about two seconds whenever she started or
/// stopped running. An empty map of the same name gives those events nothing
/// to stiffen.
pub(crate) fn write_maps(raw: &[u8], maps: &[Maps], hero_sets: &[String]) -> Result<Vec<u8>> {
    let d = Dmx::parse(raw.to_vec())?;
    let mut out = Editable::new(&d);
    let states = d
        .elements
        .iter()
        .enumerate()
        .filter(|(_, e)| e.kind == "DmeVertexData" && e.get("cloth_enable$0").is_some())
        .collect::<Vec<_>>();
    if states.len() != 1 {
        return Err(invalid("Automatic proxy needs one cloth state."));
    }
    let (state, vertex) = states[0];
    let positions = surface::floats(&d, vertex, "position$0", 42, 3)?;
    let corners = surface::ints(&d, vertex, "position$0Indices", 34)?;
    if positions.len() != maps.len() * 3
        || corners.iter().any(|&i| i < 0 || i as usize >= maps.len())
        || maps.iter().flatten().any(|v| !v.is_finite() || *v < 0.)
    {
        return Err(invalid("Automatic proxy maps do not match its points."));
    }
    let mut formats = surface::strings(vertex, "vertexFormat")?;
    for (k, name) in MAPS.iter().enumerate() {
        // Preserve old proxy bytes when no bounded long part is present. Other parts
        // in a mixed proxy keep the compiler defaults: no tethers/stretch.
        if k >= 6 && !maps.iter().any(|m| m[6] > 0.) { continue; }
        out.set(
            state,
            name,
            35,
            surface::numeric_f32(maps.iter().map(|m| if k == 3 { m[k] * GAME_GRAVITY } else { m[k] })),
        )?;
        out.set(
            state,
            &format!("{name}Indices"),
            34,
            surface::numeric_i32(corners.iter().map(|&i| i as usize))?,
        )?;
        if !formats.iter().any(|s| s == name) {
            formats.push((*name).into());
        }
    }
    for set in hero_sets {
        if !valid_set_name(set) {
            return Err(invalid(format!("Unsupported cloth vertex set name: {set}")));
        }
        let name = format!("cloth_vertex_set_{set}$0");
        out.set(state, &name, 35, surface::numeric_f32(maps.iter().map(|_| 0.)))?;
        out.set(state, &format!("{name}Indices"), 34, surface::numeric_i32(corners.iter().map(|&i| i as usize))?)?;
        if !formats.contains(&name) {
            formats.push(name);
        }
    }
    out.set(state, "vertexFormat", 37, surface::string_array(&formats)?)?;
    let bytes = out.bytes()?;
    let checked = Dmx::parse(bytes.clone())?;
    if surface::binds(&checked)?.skeleton != surface::binds(&d)?.skeleton {
        return Err(invalid("Automatic maps changed the skeleton."));
    }
    Ok(bytes)
}

/// A vertex set name the proxy can carry: 1 to 64 ASCII letters, digits or
/// underscores (Valve's are like `pony_tail_vm`).
pub(crate) fn valid_set_name(name: &str) -> bool {
    (1..=64).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

#[cfg(test)]
mod breast_tests {
    use super::*;
    fn surface() -> Surface {
        Surface {
            points: vec![],
            faces: vec![],
            influences: vec![],
            enabled: vec![],
            initial_proxy_positions: 0,
            fixed_positions_not_on_dynamic_faces: 0,
            components: 0,
            degenerate_triangles_excluded: 0,
            overlay_followers: serde_json::Value::Null,
        }
    }
    #[test]
    fn a_left_right_pair_on_one_anchor_is_linked_and_single_breasts_are_not() {
        let mut s = surface();
        let mut maps = vec![];
        let settings = breast_parameters(2., None).unwrap();
        let left = ring(&mut s, &mut maps, [2., -2., 50.], [1., 0., 0.], [0., 0., 1.], 0.5, 1, true, settings);
        let right = ring(&mut s, &mut maps, [-2., -2., 50.], [1., 0., 0.], [0., 0., 1.], 0.5, 2, true, settings);
        let lone = ring(&mut s, &mut maps, [0., -2., 40.], [1., 0., 0.], [0., 0., 1.], 0.5, 3, true, settings);
        let before = s.faces.len();
        link_breast_pairs(&mut s, &[("chest".into(), left), ("chest".into(), right), ("pelvis".into(), lone)]);
        let added = &s.faces[before..];
        assert_eq!(added.len(), 6);
        assert!(added.iter().all(|f| f.iter().all(|i| left.contains(i) || right.contains(i))));
    }
    #[test]
    fn chains_running_within_a_ring_plane_get_a_crossing_ring_and_no_collapsed_faces() {
        // MMD tip bones (尻2.L -> 尻2錘.L in janeVen): the tip lies along -Z,
        // inside the X/Z ring plane. The old rings were coplanar and one
        // connecting face collapsed to a line.
        let (x, z) = ([0., 1., 0.], [-0.764, 0., 0.645]);
        let from = [-1.123, 5.706, 59.3];
        let at = [-0.335, 5.706, 58.636];
        let (rx, rz) = ring_axes(x, z, from, at);
        let along = { let d = sub(at, from); d.map(|v| v / norm(d)) };
        assert!(dot(rx, along).abs() < 1e-9 && dot(rz, along).abs() < 1e-9 && (norm(rx) - 1.).abs() < 1e-9);
        // An ordinary chain along the ring normal keeps the bone's own frame.
        let normal = cross(x, z);
        assert_eq!(ring_axes(x, z, from, std::array::from_fn(|k| from[k] + normal[k])), (x, z));
        // Even coplanar rings connect without a collapsed face.
        let mut s = surface();
        let mut maps = vec![];
        let settings = breast_parameters(2., None).unwrap();
        let a = ring(&mut s, &mut maps, from, x, z, 0.2, 1, true, settings);
        let b = ring(&mut s, &mut maps, at, x, z, 0.2, 2, true, settings);
        connect(&mut s, a, b);
        for f in &s.faces[1..] {
            let area = norm(cross(sub(s.points[f[1]], s.points[f[0]]), sub(s.points[f[2]], s.points[f[0]])));
            assert!(area > 1e-6, "{f:?}");
        }
    }
    #[test]
    fn breast_settings_are_a_damped_bounce_without_sag() {
        for choice in [None, Some("balanced"), Some("stiff"), Some("soft")] {
            let [goal, drag, _, gravity, travel, _, tether, _] = breast_parameters(2.5, choice).unwrap();
            assert!((0.25..=0.65).contains(&goal) && drag <= 0.2 && gravity == 0. && tether == 0.);
            assert!(travel >= 0.4 && travel <= 2.);
        }
        assert!(breast_parameters(2.5, Some("soft")).unwrap()[4] > breast_parameters(2.5, Some("stiff")).unwrap()[4]);
        assert!(breast_parameters(2.5, Some("wild")).is_err());
    }
}

#[cfg(test)]
mod new_motion_tests {
    use super::*;
    #[test]
    fn butt_bounce_is_its_own_smaller_stiffer_setting() {
        let [goal, drag, _, gravity, travel, ..] = butt_parameters(3., None).unwrap();
        let breast = breast_parameters(3., None).unwrap();
        assert!(goal > breast[0] && travel < breast[4] && gravity == 0. && drag > 0.);
        assert!(butt_parameters(3., Some("soft")).unwrap()[4] > butt_parameters(3., Some("stiff")).unwrap()[4]);
        assert_eq!(parameters(Kind::Butt, 0.5, false, 3., 0.3, None).unwrap(), butt_parameters(3., None).unwrap());
        assert!(butt_parameters(3., Some("wild")).is_err());
    }
    #[test]
    fn dense_strands_keep_the_pull_of_a_five_or_six_bone_strand() {
        for nodes in 0..=DENSE_CHAIN_NODES { assert_eq!(dense_chain_multiplier(nodes), 1.); }
        // The friend's 19-bone hair strand and 25-bone tail.
        assert!((dense_chain_multiplier(19) - 6. / 19.).abs() < 1e-12);
        assert!((dense_chain_multiplier(25) * 25. - 6.).abs() < 1e-12);
    }
    #[test]
    fn a_part_hanging_on_one_bone_leaves_its_helper_bones_still() {
        let sk = Skeleton {
            names: ["head", "bun", "wing1", "wing2", "strand1", "strand2", "strand3"].map(String::from).to_vec(),
            parents: vec![-1, 0, 1, 2, 0, 4, 5],
            positions: vec![[0.; 3]; 7],
            rotations: vec![[0., 0., 0., 1.]; 7],
            scales: vec![1.; 7],
            flags: vec![0; 7],
        };
        let lookup: BTreeMap<&str, usize> = sk.names.iter().enumerate().map(|(i, n)| (n.as_str(), i)).collect();
        let group = |bones: &[&str], weighted: &[&str]| Group {
            source_root: bones[0].into(), root: bones[0].into(), anchor: "head".into(),
            bones: bones.iter().map(|s| s.to_string()).collect(),
            weighted_bones: weighted.iter().map(|s| s.to_string()).collect(),
        };
        let names = |v: Vec<&String>| v.into_iter().cloned().collect::<Vec<_>>();
        // A bun: only the bun bone carries mesh; the wing helper chain stays still.
        assert_eq!(names(simulated_bones(&group(&["bun", "wing1", "wing2"], &["bun"]), &lookup, &sk)), ["bun"]);
        // Mesh on a lower bone keeps the bones above it.
        assert_eq!(names(simulated_bones(&group(&["strand1", "strand2", "strand3"], &["strand2"]), &lookup, &sk)), ["strand1", "strand2"]);
        // A strand with mesh on several bones keeps every bone, mesh or not.
        assert_eq!(names(simulated_bones(&group(&["strand1", "strand2", "strand3"], &["strand1", "strand2"]), &lookup, &sk)), ["strand1", "strand2", "strand3"]);
    }
    #[test]
    fn long_streamers_keep_the_drag_of_a_ten_joint_streamer() {
        // TangTang's 7-joint streamers keep their drag exactly.
        for joints in 0..=LONG_STREAMER_JOINTS { assert_eq!(streamer_drag_multiplier(joints), 1.); }
        // SiViolet's 30-joint sleeves: a third of the drag.
        assert!((streamer_drag_multiplier(30) - 1. / 3.).abs() < 1e-12);
        let drag = parameters(Kind::Streamer, 0.5, false, 56., 0.2, None).unwrap()[1] * streamer_drag_multiplier(30);
        assert!((drag * 30. - 1.8).abs() < 1e-9, "{drag}");
    }
    #[test]
    fn fine_tuning_moves_each_setting_from_its_automatic_value() {
        let tuning = |stiffness: f64, damping: f64, gravity: f64, swing: f64| deadlock_project::customization::MotionTuning {
            stiffness: Some(stiffness), damping: Some(damping), gravity: Some(gravity), swing: Some(swing), ..Default::default()
        };
        let auto: Maps = [0.4, 0.1, 1., 0., 2., 0., 0., 0.];
        // Zero keeps the automatic values exactly.
        let mut same = auto;
        tune(&mut same, &tuning(0., 0., 0., 0.));
        assert_eq!(same, auto);
        // Full stiffness pulls to the pose; the least keeps a quarter.
        let mut stiff = auto;
        tune(&mut stiff, &tuning(1., 1., 1., 1.));
        assert_eq!([stiff[0], stiff[1], stiff[3], stiff[4]], [1., 0.5, 1.5, 5.]);
        let mut loose = auto;
        tune(&mut loose, &tuning(-1., -1., -1., -1.));
        assert!((loose[0] - 0.1).abs() < 1e-12 && loose[1] == 0. && loose[3] == 0. && (loose[4] - 0.5).abs() < 1e-12);
        // A part that swings freely (travel 0) stays free; the rest of the row is untouched.
        let mut free: Maps = [0.4, 0., 1., 1., 0., 0., 1., 0.];
        tune(&mut free, &tuning(0.5, 0., 0., -1.));
        assert_eq!(free[4], 0.);
        assert_eq!([free[2], free[5], free[6], free[7]], [1., 0., 1., 0.]);
        assert!((free[0] - 0.7).abs() < 1e-12);
    }
    #[test]
    fn body_clearance_caps_long_strand_travel_but_never_pins_it() {
        let body = Body { points: vec![[0., 0., 0.], [10., 0., 0.]] };
        assert_eq!(body.clearance([0., 3., 0.]), Some(3.));
        // Far from the body: unchanged. Near it: limited to the gap.
        assert_eq!(body_capped_travel(4., Some(20.), 0.2), 4.);
        assert!((body_capped_travel(24., Some(5.2), 0.2) - 5.).abs() < 1e-12);
        // Resting on the body: an inch or 15% of the free travel still moves.
        assert!((body_capped_travel(24., Some(0.3), 0.2) - 3.6).abs() < 1e-12);
        assert!((body_capped_travel(4., Some(0.3), 0.2) - 1.).abs() < 1e-12);
        assert_eq!(body_capped_travel(0.5, Some(0.1), 0.2), 0.5);
        // Unbounded (zero) travel and no body stay as they are.
        assert_eq!(body_capped_travel(0., Some(0.1), 0.2), 0.);
        assert_eq!(body_capped_travel(6., None, 0.2), 6.);
    }
}
