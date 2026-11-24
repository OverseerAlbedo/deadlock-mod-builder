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
use std::collections::{BTreeMap, BTreeSet};
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
/// movement preview (0 to 1.5) is therefore written multiplied by this; written
/// as is, automatic parts would have almost no gravity in game.
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
/// Breast motion: a slow, soft bounce that lags the body and settles, held only
/// by its pose pull, drag and travel limit (no links to the chest or the other
/// breast, no body collision, no gravity, so it never sags). The travel limit
/// (about half the breast radius, scaled by the setting) keeps the swing inside
/// the chest's reach. The automatic values are pull 0.1425, drag 0.236 and
/// travel × 0.945 after Fine-tune (rounded); Stiffer is close to a pull of 0.22,
/// with a shorter reach; More movement is looser and reaches further. The
/// earlier values (pull 0.45, 0.62 stiffer, 0.30 more movement) bounced fast
/// against the old rigid links; the first tuning (pull 0.8, damping 0.5, travel
/// 0.3 in) barely moved. `size` is the breast radius in inches.
fn breast_parameters(size: f64, choice: Option<&str>) -> Result<Maps> {
    let (goal, drag, travel_scale) = match choice.unwrap_or("automatic") {
        "automatic" | "balanced" => (0.14, 0.24, 0.95),
        "stiff" => (0.22, 0.24, 0.8),
        "soft" => (0.1, 0.18, 1.35),
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
/// per-node pull is about four times stiffer than a 5-bone one (the
/// accepted long ponytails of one test model have 5). Shorter strands keep their tuning exactly.
pub(crate) const DENSE_CHAIN_NODES: usize = 6;
/// Pose pull of a strip hanging from an arm, below its root and at its hem
/// (`arm_strip`).
pub(crate) const ARM_STRIP_GOAL: [f64; 2] = [0.45, 0.06];
/// Drag of a strip hanging from an arm: Valve's arm cloth has none.
pub(crate) const ARM_STRIP_DRAG: f64 = 0.03;
/// A strip hanging from an arm lies on it (one model's shoulder capes
/// R_capeC/D, 0.2–1 in from the body). As a free streamer it lagged behind the
/// arm and the arm passed through it; held to a travel limit it stood still.
/// Valve's own arm cloth tracks the arm instead (Wraith's sleeves: attraction
/// 0.51, 0.13, 0.016, 0.008 down the sleeve, no damping; Violet's front tassel
/// 0.22 to 0.008): a firm pose pull just below the shoulder that fades toward
/// the hem, and little drag, so the arm cannot outrun the cloth while the hem
/// still swings. The arm colliders do the rest.
fn arm_strip(settings: &mut Maps, t: f64, choice: Option<&str>) {
    let [root, hem] = ARM_STRIP_GOAL;
    let mut goal = hem + (root - hem) * (1. - t).powi(2);
    match choice.unwrap_or("automatic") {
        "stiff" => goal = (goal + 0.25).min(1.),
        "soft" => goal = (goal * 0.7).max(0.01),
        _ => {}
    }
    settings[0] = goal;
    settings[1] = ARM_STRIP_DRAG;
}
/// Pose pull of a tail modelled straight and level (`Part::level_rest`): the
/// tail's own value at its root, fading to this at the tip.
pub(crate) const LEVEL_TAIL_GOAL: [f64; 2] = [0.65, 0.05];
/// Travel limit of a straight, level tail's joints, in rest distances from the
/// tail's base. A user's MMD tail (50 in, straight out behind the hips) kept
/// pointing out in game with the tail travel (0.6 of that distance, 0.90 to
/// 28.8 in) and pull (0.65 at the root, 0.26 at the tip): a chord of 2·d·sin(θ/2)
/// let it droop only about 30°, Fine-tune or not. Hanging straight down needs
/// about 1.41 times the distance; 1.5 allows it and leaves Swing range
/// something to scale (0 would mean free and stop the slider acting).
pub(crate) const LEVEL_TAIL_TRAVEL: f64 = 1.5;
/// A straight, level tail's pose pull at `t` along it (0 root, 1 tip), after
/// the part's movement setting as `arm_strip` applies it. Its rest pose is a
/// modelling pose, so the pull fades to almost nothing and gravity hangs it.
/// The fade starts from the root value divided out over a dense chain
/// (`dense`, `dense_chain_multiplier`), so no joint near the base pulls harder
/// than the tail's ordinary values: it must bend there to hang.
fn level_tail_goal(t: f64, dense: f64, choice: Option<&str>) -> f64 {
    let [root, tip] = LEVEL_TAIL_GOAL;
    let root = (root * dense).max(tip);
    let goal = tip + (root - tip) * (1. - t).powi(2);
    match choice.unwrap_or("automatic") {
        "stiff" => (goal + 0.25).min(1.),
        "soft" => (goal * 0.7).max(0.01),
        _ => goal,
    }
}
/// Travel limit (inches) of a held root (`held_root_ring`): the root joint of
/// a held ear or automatic hair strand stays on the head while it keeps
/// simulating. A pull of 1 compiles to attraction 0.5, not a lock, and a fixed
/// node would leave the weighted bone without the cloth flags the compiled
/// check requires, so its nodes may not leave their place by more than
/// this (Valve's own hair roots: attraction 1 and 0.3 to 0.4 in).
/// - Ears: on a model with upright ears every open edge of each ear (59 per
///   ear; the ear shares no vertex with the head) sat on the first joint,
///   which moved freely: in game the base lifted off the head and the hollow
///   inside of the ear showed.
/// - Hair: what covers the back and sides of the head on a model is the
///   strands' root and second joint (0.6 to 1.3 in above the scalp; the hair
///   that does not move does not reach there). The root could slide 0.42 in on
///   a ring 0.14 to 0.18 in across, so it turned almost freely, and in game
///   the bare head showed when the hero walked backwards.
///
/// The compiler drops a limit of 0.005 or 0.01 in (no limit at all) and keeps
/// 0.03, so it is absolute (never scaled with the model) and Swing range never
/// scales it (`TravelCap::Held`).
pub(crate) const HELD_ROOT_TRAVEL: f64 = 0.03;
/// Share of the part's cross radius used as the radius of a held root's ring
/// and of the head ring it hangs from (at least the ordinary ring, at most an
/// inch): on an ordinary 0.15 in ring even 0.03 in of travel could turn the
/// root by over 10°. Ears use their smaller cross radius (0.58 in on one
/// model), hair its first (0.15 to 0.5 in on one model, from 0.1 to 0.2).
pub(crate) const HELD_ROOT_RING: f64 = 0.5;
/// Ears whose first joint is held to the head (`HELD_ROOT_TRAVEL`, wider
/// rings, `EAR_SHAFT_GOAL`): a chain of at least three weighted joints, so the
/// ear still moves from the second one. Shorter ears keep the ordinary values.
/// Held under every movement setting.
pub(crate) fn ear_base_held(part: &Part) -> bool {
    part.kind == Kind::Ear && part.weighted_chain.len() >= 3
}
/// The radius of a held root's rings, or `None` when the root is not held.
/// `automatic_hair` is whether the part is automatic hair with a measured
/// strand (`hair_motion::reference`, which needs two weighted joints and the
/// Automatic movement setting): its root is held. Ears follow
/// `ear_base_held`.
pub(crate) fn held_root_ring(part: &Part, automatic_hair: bool, ordinary: f64) -> Option<f64> {
    let cross = if ear_base_held(part) {
        part.shape_radii_inches[1].min(part.shape_radii_inches[2])
    } else if automatic_hair && part.kind == Kind::Hair {
        part.shape_radii_inches[1]
    } else {
        return None;
    };
    Some((cross * HELD_ROOT_RING).clamp(ordinary, 1.))
}
/// Pose pull of the first joint below a held hair root (compiled about 0.45,
/// from 0.15 to 0.23): its skin lies on the scalp next to the root's. The next
/// joint gets `HAIR_UPPER_SHARE` of the way there (compiled 0.29 to 0.33) and
/// the rest of the strand keeps its own pull, so it swings as before.
/// Automatic hair only; Fine-tune's Stiffness acts on top.
pub(crate) const HAIR_UPPER_GOAL: f64 = 0.9;
/// How far the second and third joint of a held hair strand (weighted joints
/// 1 and 2 from the root) move toward `HAIR_UPPER_GOAL`.
pub(crate) const HAIR_UPPER_SHARE: [f64; 2] = [1., 0.5];
/// The share of `HAIR_UPPER_GOAL` for a held hair strand's bone, by the place
/// in the weighted chain of the bone itself or, for an export helper, of the
/// nearest weighted joint above it (so a helper moves like the joint it
/// follows, as `hair_motion::coordinates` keeps them); lower joints: 0.
pub(crate) fn hair_upper_share(part: &Part, sk: &Skeleton, lookup: &BTreeMap<&str, usize>, bone: &str) -> f64 {
    let mut at = lookup[bone] as i32;
    while at >= 0 {
        if let Some(k) = part.weighted_chain.iter().position(|b| *b == sk.names[at as usize]) {
            return if (1..=2).contains(&k) { HAIR_UPPER_SHARE[k - 1] } else { 0. };
        }
        at = sk.parents[at as usize];
    }
    0.
}
/// A travel limit `generate_inner` holds after Fine-tune (the movement preview
/// applies the same through `PreviewNode.max_travel`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum TravelCap {
    /// A held root: exactly `HELD_ROOT_TRAVEL`, whatever Swing range says.
    Held,
    /// At most this (the automatic value): Swing range can shorten it, never
    /// lengthen it. The second and third joint of a held hair strand travel no
    /// further than their distance to the body (`body_capped_travel`, at least
    /// an inch): on one model 2.1 → 1.0 in and 3.9 → 1.0 in, and with the root
    /// held and the firmer pull the bare head no longer showed in game.
    AtMost(f64),
}
/// Pose pull of a held ear's joints after the first, from its base to its tip
/// (straight between, by distance; Automatic movement only): firmer than
/// hair's 0.6 to 0.4, so an upright ear wobbles and sways without flopping.
/// Compiled attraction about g²/(1+g²): 0.38 to 0.27 on that model (hair's 0.6
/// to 0.4 compiled 0.23 to 0.14 there; Valve's own ears on one hero are 0.61
/// and 0.42).
pub(crate) const EAR_SHAFT_GOAL: [f64; 2] = [0.85, 0.6];
/// Gravity scale of automatic short hair below its root: strands from
/// `HAIR_GRAVITY_FROM` of the short-hair limit up to the limit itself (half the
/// head-to-pelvis length; longer strands are the softened long hair). A third
/// of the game's gravity (compiled 120 of 360), so short hair settles downward
/// within its jump-protection travel instead of floating in its modelled pose.
/// Valve's own hair has full gravity with pulls of 0.06 to 0.34 and travel
/// limits of 1 to 3 in. Long hair keeps its own gravity: none below
/// `HANGING_HAIR`, the hang from there, as before. Fine-tune's Gravity acts
/// on top.
pub(crate) const HAIR_GRAVITY: f64 = 1. / 3.;
/// Share of the short-hair limit (half the head-to-pelvis length) below which
/// a strand keeps no gravity: tiny strands and tufts.
pub(crate) const HAIR_GRAVITY_FROM: f64 = 0.25;
/// Torso lengths (head to pelvis) from which a hair strand hangs under gravity
/// (past the knees; full gravity half a torso further).
pub(crate) const HANGING_HAIR: f64 = 2.5;
/// Pose-pull multiplier for a strand with `nodes` weighted bones.
pub(crate) fn dense_chain_multiplier(nodes: usize) -> f64 {
    if nodes > DENSE_CHAIN_NODES { DENSE_CHAIN_NODES as f64 / nodes as f64 } else { 1. }
}
/// Streamers with more weighted joints than this get their drag divided out.
/// Drag slows every node, so a long, finely jointed streamer (a model's
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
/// helper chain such as a wingB chain under each bun) get no points. Swinging
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
/// A breast's travel limit when the other breast hangs from the same anchor:
/// never far enough to reach it. Breasts are not linked to each other (see
/// `append_breast`), so this also caps Fine-tune's Swing range (`cap_travel`).
pub(crate) fn breast_travel_cap(center: [f64; 3], partner: Option<[f64; 3]>) -> Option<f64> {
    let gap = norm(sub(partner?, center));
    (gap > 1e-3).then_some(gap * 0.3)
}
/// A travel limit after Fine-tune, held to `cap` (a hard maximum); a part
/// without a limit (0, free) stays free. The movement preview applies the
/// same cap after its `tune` (`PreviewNode.max_travel`).
pub(crate) fn cap_travel(travel: f64, cap: Option<f64>) -> f64 {
    match cap {
        Some(cap) if travel > 0. => travel.min(cap),
        _ => travel,
    }
}
#[cfg(test)]
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
    append_capped(s, group, part, slots, lookup, sk, state, choice, skirt, maps, partner, body, &mut BTreeMap::new())
}
/// The part's proxy points and maps. `caps` receives the travel limits that
/// must hold after Fine-tune, by bone slot (`TravelCap`).
pub(crate) fn append_capped(
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
    caps: &mut BTreeMap<usize, TravelCap>,
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
        let hair_reference = hair_coordinates.as_ref().and_then(|_| {
            crate::hair_motion::reference(group, part, sk, state, choice)
        });
        // Strands longer than the short-hair limit (half the head-to-pelvis
        // length) get a lower pose pull; every automatic strand gets the jump
        // protection (height limit, travel limit, tethers).
        let hair_length = hair_reference.as_ref().filter(|a| a.goal_strength_multiplier < 1.);
        let hair_limits = hair_reference.as_ref()
            .map(|length| crate::hair_limits::plan(group, part, sk, state, length))
            .transpose()?;
        let x = axis(&group.root, 0);
        let y = axis(&group.root, 1);
        let z = axis(&group.root, 2);
        let dense = part.weighted_chain.len() > DENSE_CHAIN_NODES && matches!(part.kind, Kind::Hair | Kind::Tail | Kind::Accessory);
        let level_tail = part.kind == Kind::Tail && part.level_rest.is_some() && skirt.is_none();
        let fixed_center: [f64; 3] = std::array::from_fn(|r| center[r] - y[r] * offset);
        // A held root's ring (and the head ring it hangs from) spans the
        // part, so the small offsets its travel limit allows turn the root
        // very little (as the breast ring is sized to the breast).
        let held_ring = held_root_ring(part, hair_reference.is_some(), radius);
        let base_radius = held_ring.unwrap_or(radius);
        let fixed = ring(
            s,
            maps,
            fixed_center,
            x,
            z,
            base_radius,
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
        // Long strips inside a cloth part move like streamers below the joints
        // they share with other strips (parts::Strip): each such bone, helpers
        // included, with its distance along the strip, the strip's length and
        // its weighted joints.
        let mut strip_bones = BTreeMap::<&str, (f64, f64, usize)>::new();
        if part.kind == Kind::Cloth && skirt.is_none() {
            for strip in &part.strips {
                let mut along = vec![0.];
                for w in strip.path.windows(2) {
                    along.push(along[along.len() - 1] + norm(sub(pos(&w[1]), pos(&w[0]))));
                }
                let total = along[along.len() - 1];
                for k in strip.branch.max(1)..strip.path.len() {
                    let previous = &strip.path[k - 1];
                    let mut at = lookup[strip.path[k].as_str()];
                    while at != lookup[previous.as_str()] && sk.parents[at] >= 0 {
                        let bone = sk.names[at].as_str();
                        strip_bones.entry(bone).or_insert((along[k - 1] + norm(sub(pos(bone), pos(previous))), total, strip.path.len()));
                        at = sk.parents[at] as usize;
                    }
                }
            }
        }
        // Hair reaching past the knees hangs: gravity and drag grow from none at
        // HANGING_HAIR torso lengths to a streamer's half a torso further, and
        // its travel lets it reach that hang. A test model's braid (3.1 torso
        // lengths) floated out sideways with neither; shorter hair (2.45 torso
        // lengths and less on the test projects) keeps hair's no-gravity motion
        // (`HAIR_GRAVITY` is for short hair only).
        let hang = hair_length.filter(|a| a.scale_reference == "head_to_pelvis").map_or(0., |a| {
            let torsos = a.strand_length_inches / (2. * a.short_hair_limit_inches);
            ((torsos - HANGING_HAIR) / 0.5).clamp(0., 1.)
        });
        let up = match (sk.indices().get("head"), sk.indices().get("pelvis")) {
            (Some(&h), Some(&v)) => unit(sub(pos(&sk.names[h]), pos(&sk.names[v]))).unwrap_or([0., 0., 1.]),
            _ => [0., 0., 1.],
        };
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
                if let Some(&(along, total, joints)) = strip_bones.get(name.as_str()) {
                    let t = (along / total).clamp(0., 1.);
                    settings = parameters(Kind::Streamer, t, false, total, radius, choice)?;
                    if choice.is_none_or(|c| c == "automatic") {
                        settings[1] *= streamer_drag_multiplier(joints);
                    }
                    if crate::secondary::arm_anchor(&group.anchor) {
                        arm_strip(&mut settings, t, choice);
                    }
                }
                if let Some(adjustment) = hair_length {
                    settings[0] *= adjustment.goal_strength_multiplier;
                }
                // Automatic short hair settles under light gravity (the root
                // keeps none); longer hair keeps its own (`hang`).
                if hair_length.is_none() && hair_reference.as_ref().is_some_and(|r| r.strand_length_inches >= HAIR_GRAVITY_FROM * r.short_hair_limit_inches) {
                    settings[3] = settings[3].max(HAIR_GRAVITY);
                }
                // A held ear's shaft is firmer than hair (Automatic only).
                if ear_base_held(part) && choice.is_none_or(|c| c == "automatic") {
                    let [base, tip] = EAR_SHAFT_GOAL;
                    settings[0] = base + (tip - base) * t;
                }
                if dense && choice.is_none_or(|c| c == "automatic") {
                    settings[0] = (settings[0] * dense_chain_multiplier(part.weighted_chain.len())).max(0.03);
                }
                // Below a held hair root the upper strand stays on the head:
                // the next joint gets `HAIR_UPPER_GOAL`, the one after it part
                // of the way there; never a lower pull than before.
                if held_ring.is_some() && part.kind == Kind::Hair {
                    settings[0] += (HAIR_UPPER_GOAL - settings[0]).max(0.) * hair_upper_share(part, sk, lookup, name);
                }
            }
            // Long tails and accessory strands: tethers stop the chain from
            // stretching under gravity, and a travel limit along the strand
            // keeps a thin tail from flailing (zero would mean unbounded).
            if dense && matches!(part.kind, Kind::Tail | Kind::Accessory) && skirt.is_none() {
                settings[4] = (max_distance * t * 0.6).max(radius * 2.);
                settings[6] = 1.;
            }
            // A tail modelled straight and level hangs: its pull fades to the
            // tip and its travel lets it reach the hang. Short ones get the
            // tethers and limit too (they are free otherwise), so the weak pull
            // cannot let them stretch under gravity.
            if level_tail {
                if !root {
                    let divided = if dense && choice.is_none_or(|c| c == "automatic") { dense_chain_multiplier(part.weighted_chain.len()) } else { 1. };
                    settings[0] = level_tail_goal(t, divided, choice);
                }
                settings[4] = (norm(sub(p, center)) * LEVEL_TAIL_TRAVEL).max(radius * 2.);
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
            if hang > 0. && !root {
                settings[3] = settings[3].max(hang);
                settings[1] = settings[1].max(0.18 * hang);
                if settings[4] > 0. {
                    let start = pos(&part.weighted_chain[0]);
                    let along = t * hair_length.map_or(0., |a| a.strand_length_inches);
                    let hanging = std::array::from_fn(|r| start[r] - up[r] * along);
                    settings[4] = settings[4].max(norm(sub(p, hanging)) * hang);
                }
            }
            if let Some(skirt) = skirt {
                settings[4] = skirt.travel_radius_by_bone[name];
                settings[6] = 1.;
            }
            // Long, dense strands must not swing through the body. A hanging
            // tail is exempt: its clearance is about its distance from the
            // hips, which would hold it near 60° of droop; the leg and hip
            // colliders keep it out of the body.
            // Dense short hair keeps its jump-protection travel whole: it lies
            // on the head, where the body cap would leave about an inch. (Its
            // travel used to be free, which the cap leaves alone, so this keeps
            // the cap where it always acted.)
            let short_hair = part.kind == Kind::Hair && hair_length.is_none();
            if dense && !root && !level_tail && !short_hair && skirt.is_none() && matches!(part.kind, Kind::Hair | Kind::Tail | Kind::Accessory) {
                settings[4] = body_capped_travel(settings[4], body.and_then(|b| b.clearance(p)), radius);
            }
            // The two joints below a held hair root travel no further than
            // their distance to the body (at least an inch), and never further
            // after Fine-tune (`TravelCap::AtMost`).
            if held_ring.is_some() && part.kind == Kind::Hair && hair_upper_share(part, sk, lookup, name) > 0. {
                settings[4] = body_capped_travel(settings[4], body.and_then(|b| b.clearance(p)), radius);
                caps.insert(slots[name.as_str()], TravelCap::AtMost(settings[4]));
            }
            // A held root stays on the head (`HELD_ROOT_TRAVEL`); the part
            // moves from its next joint.
            if held_ring.is_some() && root {
                settings[4] = HELD_ROOT_TRAVEL;
                caps.insert(slots[name.as_str()], TravelCap::Held);
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
                if root { base_radius } else { radius },
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
/// the moving ring, so a left/right butt pair can be linked and a breast ring
/// kept out of the body colliders (`write_maps`).
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
    if let Some(cap) = breast_travel_cap(center, partner) {
        settings[4] = settings[4].min(cap);
    }
    let moving = ring(s, maps, center, side, lift, radius, slots[bone.as_str()], true, settings);
    // A breast's moving ring keeps only its own rigid triangle, held by its
    // pose pull and travel limit: rigid links to the chest ring (and to the
    // other breast, `link_breast_pairs`) let it turn only about them, like a
    // door on its hinges, and it turned round and clipped at a large swing.
    // Butts keep their links. The fixed ring stays for the pieces below
    // (`unwrap_or(fixed)`) and so the colliders still measure the same
    // anchors (`body_colliders::Clearance`).
    if part.kind != Kind::Breast {
        connect(s, fixed, moving);
    }
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
/// Collision layer maps of the proxy (the capsules the app authors are on
/// layers 0 to 3). A point with 0 on every layer collides with none of them.
const COLLISION_LAYERS: [&str; 4] = [
    "cloth_collision_layer_0$0",
    "cloth_collision_layer_1$0",
    "cloth_collision_layer_2$0",
    "cloth_collision_layer_3$0",
];
/// Writes the motion maps. `no_body_collision` lists the points kept out of
/// the body colliders (a breast's moving ring: the character's own arm pushed
/// it out and every shot's recoil threw it); every other point keeps 1 on
/// every layer. Without such points no layer map is written at all.
pub(crate) fn write_maps(raw: &[u8], maps: &[Maps], hero_sets: &[String], no_body_collision: &BTreeSet<usize>) -> Result<Vec<u8>> {
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
    if no_body_collision.iter().any(|&i| i >= maps.len()) {
        return Err(invalid("Automatic proxy collision points do not match its points."));
    }
    if !no_body_collision.is_empty() {
        for name in COLLISION_LAYERS {
            out.set(state, name, 35, surface::numeric_f32((0..maps.len()).map(|i| if no_body_collision.contains(&i) { 0. } else { 1. })))?;
            out.set(state, &format!("{name}Indices"), 34, surface::numeric_i32(corners.iter().map(|&i| i as usize))?)?;
            if !formats.iter().any(|s| s == name) {
                formats.push(name.into());
            }
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
    pub(super) fn surface() -> Surface {
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
        // MMD tip bones (尻2.L -> 尻2錘.L on one model): the tip lies along -Z,
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
            assert!((0.1..=0.22).contains(&goal) && (0.18..=0.24).contains(&drag) && gravity == 0. && tether == 0.);
            assert!(travel >= 0.4 && travel <= 2.);
        }
        // A 2.5 in breast: base travel 1.125 in, scaled 0.95 / 0.8 / 1.35.
        let row = |c: Option<&str>| { let m = breast_parameters(2.5, c).unwrap(); [m[0], m[1], m[4]] };
        let near = |a: [f64; 3], b: [f64; 3]| a.iter().zip(b).all(|(a, b)| (a - b).abs() < 1e-12);
        assert!(near(row(None), [0.14, 0.24, 1.125 * 0.95]) && row(Some("balanced")) == row(None));
        assert!(near(row(Some("stiff")), [0.22, 0.24, 0.9]));
        assert!(near(row(Some("soft")), [0.1, 0.18, 1.125 * 1.35]));
        // The ceiling holds for the largest breasts.
        assert_eq!(breast_parameters(10., Some("soft")).unwrap()[4], 2.);
        assert!(breast_parameters(2.5, Some("soft")).unwrap()[4] > breast_parameters(2.5, Some("stiff")).unwrap()[4]);
        assert!(breast_parameters(2.5, Some("wild")).is_err());
    }
}
#[cfg(test)]
mod breast_link_tests {
    use super::*;
    use super::breast_tests::surface;
    #[test]
    fn a_breast_ring_has_no_chest_link_and_a_butt_keeps_its_own() {
        let sk = Skeleton {
            names: ["spine_3", "breast"].map(String::from).to_vec(), parents: vec![-1, 0],
            positions: vec![[0., 0., 50.], [2., -3., -2.]], rotations: vec![[0., 0., 0., 1.]; 2], scales: vec![1.; 2], flags: vec![0; 2],
        };
        let lookup: BTreeMap<&str, usize> = sk.names.iter().enumerate().map(|(i, n)| (n.as_str(), i)).collect();
        let state = deadlock_skeleton::transforms::bind_state(&sk).unwrap();
        let group = Group { source_root: "breast".into(), root: "breast".into(), anchor: "spine_3".into(), bones: vec!["breast".into()], weighted_bones: vec!["breast".into()] };
        for kind in [Kind::Breast, Kind::Butt] {
            let part = Part { root: "breast".into(), kind, reason: String::new(), weighted_chain: vec!["breast".into()], chain_length_inches: 0.,
                shape_radii_inches: [2.5, 2., 2.], roll_locked: false, review_recommended: false, strips: vec![], level_rest: None };
            let (mut s, mut maps) = (surface(), vec![]);
            let moving = append(&mut s, &group, &part, &lookup, &lookup, &sk, &state, None, None, &mut maps, Some([-2., -3., 48.]), None).unwrap().unwrap();
            assert_eq!(moving, [3, 4, 5]);
            let linked = s.faces.iter().any(|f| f.iter().any(|i| *i < 3) && f.iter().any(|i| *i >= 3));
            assert_eq!(linked, kind == Kind::Butt, "{kind:?}");
            // The fixed ring stays either way, and the cap toward the partner (4 in away) holds.
            assert_eq!(s.points.len(), 6);
            assert!(maps[3][4] <= 0.3 * 4. + 1e-12);
        }
    }
}
#[cfg(test)]
mod level_tail_tests {
    use super::*;
    use super::breast_tests::surface;
    /// A pelvis and an eight-joint tail of `pitch` inches per joint along `d`.
    fn tail_rig(d: [f64; 3], pitch: f64) -> (Skeleton, Group, Part) {
        let mut names = vec!["pelvis".to_string()];
        names.extend((1..=8).map(|k| format!("tail{k}")));
        let positions = (0..9).map(|k| match k { 0 => [0., 0., 38.], 1 => [0., 4., 0.], _ => d.map(|v| v * pitch) }).collect();
        let sk = Skeleton { names: names.clone(), parents: (-1..8).collect(), positions, rotations: vec![[0., 0., 0., 1.]; 9], scales: vec![1.; 9], flags: vec![0; 9] };
        let bones = names[1..].to_vec();
        let group = Group { source_root: "tail1".into(), root: "tail1".into(), anchor: "pelvis".into(), bones: bones.clone(), weighted_bones: bones.clone() };
        let part = Part { root: "tail1".into(), kind: Kind::Tail, reason: String::new(), weighted_chain: bones, chain_length_inches: 7. * pitch,
            shape_radii_inches: [20., 1., 1.], roll_locked: true, review_recommended: false, strips: vec![], level_rest: None };
        (sk, group, part)
    }
    /// Each moving bone's row (pose pull, drag, gravity, travel, tethers) after `append`.
    fn tail_rows(sk: &Skeleton, group: &Group, part: &Part, choice: Option<&str>, body: Option<&Body>) -> Vec<[f64; 5]> {
        let lookup: BTreeMap<&str, usize> = sk.names.iter().enumerate().map(|(i, n)| (n.as_str(), i)).collect();
        let state = deadlock_skeleton::transforms::bind_state(sk).unwrap();
        let mut s = surface();
        let mut maps = vec![];
        append(&mut s, group, part, &lookup, &lookup, sk, &state, choice, None, &mut maps, None, body).unwrap();
        maps[3..].chunks(3).map(|ring| { assert!(ring.iter().all(|m| *m == ring[0])); [ring[0][0], ring[0][1], ring[0][3], ring[0][4], ring[0][6]] }).collect()
    }
    #[test]
    fn a_straight_level_tail_hangs_and_every_other_tail_keeps_its_values() {
        // The MMD tail: straight back and almost level, 7.1 in per joint.
        let d = [0., 0.99, -0.11].map(|v: f64| v / (0.99f64 * 0.99 + 0.11 * 0.11).sqrt());
        let (sk, group, mut part) = tail_rig(d, 7.1);
        // Body vertices at the hips cap an ordinary dense tail's travel.
        let body = Body { points: vec![[0., 3., 37.], [0., 2., 30.]] };
        let before = tail_rows(&sk, &group, &part, None, Some(&body));
        part.level_rest = crate::parts::level_rest(&(1..9).map(|k| [0., 4. + d[1] * 7.1 * (k - 1) as f64, 38. + d[2] * 7.1 * (k - 1) as f64]).collect::<Vec<_>>(), [0., 0., 1.]);
        assert!(part.level_rest.is_some());
        let after = tail_rows(&sk, &group, &part, None, Some(&body));
        assert_eq!(before.len(), 8);
        assert_eq!(after[0], before[0], "the root keeps the tail's pull and travel");
        assert!((after[0][0] - 0.65).abs() < 1e-12);
        for (k, (a, b)) in after.iter().zip(&before).enumerate().skip(1) {
            let distance = 7.1 * k as f64;
            let t = k as f64 / 7.;
            // Today: 0.65 - 0.30 t, divided out over eight joints; 0.6 of the distance at most.
            assert!((b[0] - (0.65 - 0.3 * t) * 0.75).abs() < 1e-9 && b[3] <= distance * 0.6 + 1e-9);
            assert!((a[0] - (0.05 + (0.65 * 0.75 - 0.05) * (1. - t).powi(2))).abs() < 1e-9, "{k}");
            assert!(a[0] < b[0], "{k}: no joint pulls harder than before");
            assert!((a[3] - distance * LEVEL_TAIL_TRAVEL).abs() < 1e-9, "{k}: not body capped");
            assert_eq!([a[1], a[2], a[4]], [b[1], b[2], b[4]], "drag, gravity and tethers as the tail has them");
        }
        assert!((after[7][0] - 0.05).abs() < 1e-12);
        // A movement setting moves the fade as it moves an arm strip's.
        let stiff = tail_rows(&sk, &group, &part, Some("stiff"), None);
        assert!((stiff[7][0] - 0.3).abs() < 1e-12 && (stiff[0][0] - 0.9).abs() < 1e-12);
        // A short tail (no tethers or limit of its own) gets them with the fade.
        let short = Part { weighted_chain: part.weighted_chain[..5].to_vec(), ..part.clone() };
        let rows = tail_rows(&sk, &group, &short, None, None);
        assert!(rows.iter().all(|r| r[3] > 0. && r[4] == 1.));
        assert_eq!(tail_rows(&sk, &group, &Part { level_rest: None, ..short }, None, None)[1][3..], [0., 0.]);
        // A tail hanging as modelled has no level rest and keeps today's rows.
        let hanging = [0., 0.6, -0.8];
        let (sk, group, part) = tail_rig(hanging, 7.1);
        assert_eq!(crate::parts::level_rest(&(1..9).map(|k| std::array::from_fn(|r| hanging[r] * 7.1 * k as f64)).collect::<Vec<_>>(), [0., 0., 1.]), None);
        let rows = tail_rows(&sk, &group, &part, None, Some(&body));
        assert!(rows.iter().enumerate().skip(1).all(|(k, r)| (r[0] - (0.65 - 0.3 * k as f64 / 7.) * 0.75).abs() < 1e-9));
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
        // 30-joint sleeves: a third of the drag.
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
        // A breast's travel cap holds after Fine-tune (same numbers as the
        // preview test): full swing would take 2 to 5, the cap keeps 3.
        let mut swung = auto;
        tune(&mut swung, &tuning(0., 0., 0., 1.));
        assert_eq!(cap_travel(swung[4], Some(3.)), 3.);
        assert_eq!(cap_travel(1., Some(3.)), 1.);
        assert_eq!(cap_travel(5., None), 5.);
        assert_eq!(cap_travel(0., Some(3.)), 0.);
        assert_eq!(breast_travel_cap([0., 0., 0.], Some([0., 10., 0.])), Some(3.));
        assert_eq!(breast_travel_cap([0., 0., 0.], None), None);
        assert_eq!(breast_travel_cap([0., 0., 0.], Some([0., 0., 0.0001])), None);
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

#[cfg(test)]
mod hair_ear_tests {
    use super::*;
    use super::breast_tests::surface;
    use sha2::{Digest, Sha256};
    /// A pelvis, a head 24 in above it (so the short-hair limit is 12 in) and
    /// a chain of `n` joints `pitch` inches apart along `d`, hanging from the
    /// head 3 in behind it.
    pub(super) fn chain_rig(kind: Kind, n: usize, pitch: f64, d: [f64; 3]) -> (Skeleton, Group, Part) {
        let length = norm(d);
        let d = d.map(|v| v / length);
        let mut names = vec!["pelvis".to_string(), "head".to_string()];
        names.extend((1..=n).map(|k| format!("strand{k}")));
        let positions = (0..n + 2).map(|k| match k { 0 => [0., 0., 40.], 1 => [0., 0., 24.], 2 => [0., -3., 0.], _ => d.map(|v| v * pitch) }).collect();
        let sk = Skeleton { names: names.clone(), parents: (-1..n as i32 + 1).collect(), positions, rotations: vec![[0., 0., 0., 1.]; n + 2], scales: vec![1.; n + 2], flags: vec![0; n + 2] };
        let bones = names[2..].to_vec();
        let group = Group { source_root: "strand1".into(), root: "strand1".into(), anchor: "head".into(), bones: bones.clone(), weighted_bones: bones.clone() };
        let part = Part { root: "strand1".into(), kind, reason: String::new(), weighted_chain: bones, chain_length_inches: (n - 1) as f64 * pitch,
            shape_radii_inches: [(n - 1) as f64 * pitch, 1., 0.8], roll_locked: true, review_recommended: false, strips: vec![], level_rest: None };
        (sk, group, part)
    }
    /// The maps, points and faces `append` authors for the part.
    pub(super) fn run(sk: &Skeleton, group: &Group, part: &Part, choice: Option<&str>, body: Option<&Body>) -> (Vec<Maps>, Vec<[f64; 3]>, Vec<[usize; 3]>) {
        let lookup: BTreeMap<&str, usize> = sk.names.iter().enumerate().map(|(i, n)| (n.as_str(), i)).collect();
        let state = deadlock_skeleton::transforms::bind_state(sk).unwrap();
        let mut s = surface();
        let mut maps = vec![];
        append(&mut s, group, part, &lookup, &lookup, sk, &state, choice, None, &mut maps, None, body).unwrap();
        (maps, s.points, s.faces)
    }
    fn digest((maps, points, faces): &(Vec<Maps>, Vec<[f64; 3]>, Vec<[usize; 3]>)) -> String {
        let mut h = Sha256::new();
        for row in maps { for v in row { h.update(v.to_bits().to_le_bytes()); } }
        for p in points { for v in p { h.update(v.to_bits().to_le_bytes()); } }
        for f in faces { for v in f { h.update((*v as u64).to_le_bytes()); } }
        format!("{:x}", h.finalize())[..16].to_string()
    }
    const DOWN: [f64; 3] = [0., -0.2, -0.98];
    const UP: [f64; 3] = [0.3, 0., 0.95];
    fn body() -> Body { Body { points: vec![[0., -4., 55.], [0., -5., 45.], [0., -4., 35.]] } }
    /// Parts the hair and ear rules must not touch, with the digest of what
    /// `append` authored for them before those rules (hair jump protection on
    /// short strands, hair gravity, the held ear base) were added. A deliberate
    /// change to one of these parts updates its digest here. The two hanging
    /// strands' digests were updated once for the held hair root and upper
    /// joints (`HELD_ROOT_TRAVEL`, `HAIR_UPPER_GOAL`); their gravity and their
    /// lower joints are as before (`lower_joints_of_a_held_strand_keep_their_values`).
    #[test]
    fn other_parts_author_exactly_what_they_did_before_the_hair_and_ear_rules() {
        let body = body();
        let cases: Vec<(&str, Kind, usize, f64, [f64; 3], Option<&str>)> = vec![
            ("tail", Kind::Tail, 4, 2., DOWN, None),
            ("streamer", Kind::Streamer, 4, 2., DOWN, None),
            ("cloth", Kind::Cloth, 4, 2., DOWN, None),
            ("accessory", Kind::Accessory, 8, 2., DOWN, None),
            ("breast", Kind::Breast, 2, 2., DOWN, None),
            ("butt", Kind::Butt, 2, 2., DOWN, None),
            ("charm", Kind::Charm, 3, 1., DOWN, None),
            ("rigid", Kind::Rigid, 3, 1., DOWN, None),
            ("ear with two joints", Kind::Ear, 2, 1.5, UP, None),
            ("hair, soft", Kind::Hair, 4, 1.5, DOWN, Some("soft")),
            ("hair, stiff", Kind::Hair, 4, 1.5, DOWN, Some("stiff")),
            ("hair, 3 torso lengths", Kind::Hair, 9, 9., DOWN, None),
            ("hair, 2.8 torso lengths", Kind::Hair, 9, 8.4, DOWN, None),
        ];
        let mut lines = vec![];
        for (name, kind, n, pitch, d, choice) in cases {
            let (sk, group, part) = chain_rig(kind, n, pitch, d);
            lines.push(format!("{name}: {}", digest(&run(&sk, &group, &part, choice, Some(&body)))));
        }
        let expected = [
            "tail: 35275ffe230a3df3",
            "streamer: 28942a3f52b41013",
            "cloth: 6156580a67d693a4",
            "accessory: 274f261da9ad972a",
            "breast: 042e777e9dbebaf9",
            "butt: 5f909d0e136cce03",
            "charm: d798425238390153",
            "rigid: 070c0b36f9b795ea",
            "ear with two joints: ddf1459f6dc4a8cd",
            "hair, soft: 2dc7dbaa09934f82",
            "hair, stiff: 39532d1d74d59343",
            "hair, 3 torso lengths: d555342004263226",
            "hair, 2.8 torso lengths: 63cfb7ff2d2a8f46",
        ];
        assert_eq!(lines, expected, "{}", lines.join("\n"));
    }
    /// Each moving ring's row; every point of a ring has the same values.
    fn rows(maps: &[Maps]) -> Vec<Maps> {
        maps[3..].chunks(3).map(|ring| { assert!(ring.iter().all(|m| *m == ring[0])); ring[0] }).collect()
    }
    fn ring_radius(points: &[[f64; 3]], ring: usize) -> f64 {
        let p = &points[ring * 3..ring * 3 + 3];
        let c: [f64; 3] = std::array::from_fn(|k| (p[0][k] + p[1][k] + p[2][k]) / 3.);
        norm(sub(p[0], c))
    }
    /// Body vertices on the scalp beside a strand's upper joints.
    fn scalp() -> Body { Body { points: vec![[0., -2.5, 62.4], [0., -2.5, 61.], [0., -4., 35.]] } }
    #[test]
    fn short_hair_gets_jump_protection_a_held_root_a_firm_upper_strand_and_light_gravity() {
        // A 4.5 in strand, under the 12 in short-hair limit.
        let (sk, group, part) = chain_rig(Kind::Hair, 4, 1.5, DOWN);
        let state = deadlock_skeleton::transforms::bind_state(&sk).unwrap();
        let reference = crate::hair_motion::reference(&group, &part, &sk, &state, None).unwrap();
        assert_eq!(reference.goal_strength_multiplier, 1.);
        let limits = crate::hair_limits::plan(&group, &part, &sk, &state, &reference).unwrap();
        let body = scalp();
        let lookup: BTreeMap<&str, usize> = sk.names.iter().enumerate().map(|(i, n)| (n.as_str(), i)).collect();
        let (mut s, mut maps, mut caps) = (surface(), vec![], BTreeMap::new());
        append_capped(&mut s, &group, &part, &lookup, &lookup, &sk, &state, None, None, &mut maps, None, Some(&body), &mut caps).unwrap();
        let rows = rows(&maps);
        assert_eq!(rows.len(), 4);
        let radius = (1.5f64 * 0.08).clamp(0.05, 0.45);
        // The head ring and the root ring are half the strand's cross radius
        // (1 in); the rings below keep the ordinary size.
        assert!((ring_radius(&s.points, 0) - 0.5).abs() < 1e-9 && (ring_radius(&s.points, 1) - 0.5).abs() < 1e-9);
        assert!((2..5).all(|r| (ring_radius(&s.points, r) - radius).abs() < 1e-9));
        for (k, row) in rows.iter().enumerate() {
            let name = format!("strand{}", k + 1);
            let t = k as f64 / 3.;
            let own = 0.6 - 0.2 * t;
            let goal = match k { 0 => 1., 1 => HAIR_UPPER_GOAL, 2 => own + (HAIR_UPPER_GOAL - own) * 0.5, _ => own };
            assert!((row[0] - goal).abs() < 1e-12, "{k}: pull {}", row[0]);
            assert_eq!(row[3], if k == 0 { 0. } else { HAIR_GRAVITY }, "{k}: gravity, none at the root");
            let p = s.points[3 + 3 * k..6 + 3 * k].iter().fold([0.; 3], |a, q| std::array::from_fn(|r| a[r] + q[r] / 3.));
            let travel = match k {
                0 => HELD_ROOT_TRAVEL,
                1 | 2 => body_capped_travel(limits.travel_radius_by_bone[&name], body.clearance(p), radius),
                _ => limits.travel_radius_by_bone[&name],
            };
            assert!((row[4] - travel).abs() < 1e-12, "{k}: travel {} against {travel}", row[4]);
            assert_eq!(caps.get(&lookup[name.as_str()]).copied(), match k { 0 => Some(TravelCap::Held), 1 | 2 => Some(TravelCap::AtMost(row[4])), _ => None }, "{k}");
            assert_eq!(row[6], 1., "{k}: tethers");
            assert_eq!([row[1], row[2], row[5], row[7]], [0., 1., 0., 0.]);
        }
        // The scalp right beside it holds the second joint closer (at least an inch).
        assert!(rows[1][4] < limits.travel_radius_by_bone["strand2"] && rows[1][4] >= 1.);
        // The report carries the held root's value.
        assert_eq!(limits.travel_radius_by_bone["strand1"], HELD_ROOT_TRAVEL);
        // An explicit movement setting keeps the old rows: no hold, no limit,
        // no gravity, ordinary rings.
        let (soft, points, _) = run(&sk, &group, &part, Some("soft"), Some(&body));
        let soft = rows_of(soft);
        assert!(soft.iter().all(|r| r[3] == 0. && r[4] == 0. && r[6] == 0.));
        assert!((soft[1][0] - (0.6 - 0.2 / 3.) * 0.7).abs() < 1e-12);
        assert!((0..5).all(|r| (ring_radius(&points, r) - radius).abs() < 1e-9));
    }
    #[test]
    fn a_two_joint_strand_holds_its_root_and_firms_its_tip() {
        // Two joints: the tip is the joint below the root, so it takes the
        // upper pull and the body-held travel.
        let (sk, group, part) = chain_rig(Kind::Hair, 2, 2., DOWN);
        let rows = rows(&run(&sk, &group, &part, None, Some(&scalp())).0);
        assert_eq!((rows[0][0], rows[0][4]), (1., HELD_ROOT_TRAVEL));
        assert!((rows[1][0] - HAIR_UPPER_GOAL).abs() < 1e-12);
        assert!(rows[1][4] > 0.);
    }
    #[test]
    fn lower_joints_of_a_held_strand_keep_their_values() {
        // Long hair (softened, 9 joints): only the root and the two joints
        // below it change; a strand without the rules (Stiff) gives the same
        // lower rows apart from the setting's own pull.
        let (sk, group, part) = chain_rig(Kind::Hair, 9, 2.5, DOWN);
        let body = scalp();
        let automatic = rows_of(run(&sk, &group, &part, None, Some(&body)).0);
        let state = deadlock_skeleton::transforms::bind_state(&sk).unwrap();
        let reference = crate::hair_motion::reference(&group, &part, &sk, &state, None).unwrap();
        assert!(reference.goal_strength_multiplier < 1.);
        let limits = crate::hair_limits::plan(&group, &part, &sk, &state, &reference).unwrap();
        for (k, row) in automatic.iter().enumerate().skip(3) {
            let t = k as f64 / 8.;
            let own = (0.6 - 0.2 * t) * reference.goal_strength_multiplier * dense_chain_multiplier(9);
            assert!((row[0] - own.max(0.03)).abs() < 1e-12, "{k}");
            assert!(row[4] <= limits.travel_radius_by_bone[&format!("strand{}", k + 1)] + 1e-12);
        }
    }
    #[test]
    fn hair_gravity_starts_at_a_quarter_of_the_short_hair_limit() {
        // The limit is 12 in, so gravity starts at 3 in.
        for (pitch, gravity) in [(2.999 / 3., 0.), (1., HAIR_GRAVITY), (0.5, 0.)] {
            let (sk, group, part) = chain_rig(Kind::Hair, 4, pitch, DOWN);
            let rows = rows(&run(&sk, &group, &part, None, None).0);
            assert_eq!(rows[0][3], 0., "the root never has gravity");
            assert!(rows[1..].iter().all(|r| r[3] == gravity), "{pitch}");
            // Short strands get the jump protection on either side of it.
            assert!(rows.iter().all(|r| r[4] > 0. && r[6] == 1.));
        }
    }
    #[test]
    fn only_short_hair_gets_the_light_gravity_and_long_hair_keeps_its_own() {
        // The short-hair limit is 12 in (a torso of 24 in). Just under it: the
        // light gravity. Just over it (softened long hair): none, as before.
        // From HANGING_HAIR (60 in) the hang ramps from 0 to 1 at 72 in.
        for (length, gravity) in [(11.99, HAIR_GRAVITY), (12., HAIR_GRAVITY), (12.01, 0.), (58.8, 0.), (60., 0.), (61.2, 0.1), (67.2, 0.6), (72., 1.), (84., 1.)] {
            let (sk, group, part) = chain_rig(Kind::Hair, 9, length / 8., DOWN);
            let rows = rows(&run(&sk, &group, &part, None, None).0);
            assert_eq!(rows[0][3], 0., "{length}: the root never has gravity");
            assert!(rows[1..].iter().all(|r| (r[3] - gravity).abs() < 1e-9), "{length}: {:?}", rows[1]);
        }
    }
    #[test]
    fn an_ear_of_three_or_more_joints_keeps_its_base_on_the_head() {
        let (sk, group, mut part) = chain_rig(Kind::Ear, 5, 0.75, UP);
        part.shape_radii_inches = [3., 1.2, 0.8];
        let ordinary = (0.75f64 * 0.08).clamp(0.05, 0.45);
        let (maps, points, _) = run(&sk, &group, &part, None, None);
        let rows = rows(&maps);
        // The head ring and the base ring span the ear's base (half its smaller
        // cross radius); the rings above keep the ordinary size.
        assert!((ring_radius(&points, 0) - 0.4).abs() < 1e-9);
        assert!((ring_radius(&points, 1) - 0.4).abs() < 1e-9);
        assert!((2..6).all(|r| (ring_radius(&points, r) - ordinary).abs() < 1e-9));
        assert_eq!((rows[0][0], rows[0][4]), (1., HELD_ROOT_TRAVEL));
        let [base, tip] = EAR_SHAFT_GOAL;
        for (k, row) in rows.iter().enumerate().skip(1) {
            let t = k as f64 / 4.;
            assert!((row[0] - (base + (tip - base) * t)).abs() < 1e-12, "{k}");
            assert_eq!([row[1], row[3], row[4], row[6]], [0., 0., 0., 0.], "{k}: no drag, gravity, limit or tethers");
        }
        // A movement setting keeps its own pull; the base stays held.
        let stiff = rows_of(run(&sk, &group, &part, Some("stiff"), None).0);
        assert_eq!(stiff[0][4], HELD_ROOT_TRAVEL);
        assert!(stiff.iter().enumerate().skip(1).all(|(k, r)| (r[0] - (0.85 - 0.2 * k as f64 / 4.)).abs() < 1e-12));
        // The smallest ear's rings never shrink below the ordinary size, the largest stop at an inch.
        part.shape_radii_inches = [3., 0.01, 0.01];
        assert!((ring_radius(&run(&sk, &group, &part, None, None).1, 1) - ordinary).abs() < 1e-9);
        part.shape_radii_inches = [3., 5., 5.];
        assert!((ring_radius(&run(&sk, &group, &part, None, None).1, 1) - 1.).abs() < 1e-9);
        // Two joints: nothing to hold (the digest above keeps its values).
        assert!(!ear_base_held(&Part { weighted_chain: part.weighted_chain[..2].to_vec(), ..part.clone() }));
        assert!(!ear_base_held(&Part { kind: Kind::Hair, ..part }));
    }
    fn rows_of(maps: Vec<Maps>) -> Vec<Maps> { rows(&maps) }
}
