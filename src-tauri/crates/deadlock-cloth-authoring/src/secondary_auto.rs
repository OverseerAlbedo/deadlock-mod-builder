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
) -> Result<()> {
    if let Some(skirt) = skirt {
        return crate::skirt_surface::append(s,group,part,slots,lookup,sk,state,choice,skirt,maps);
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
            let settings = parameters(
                part.kind,
                t,
                level == 0,
                part.chain_length_inches,
                radius,
                choice,
            )?;
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
        let fixed = ring(
            s,
            maps,
            std::array::from_fn(|r| center[r] - y[r] * offset),
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
        for name in &group.bones {
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
            let ids = ring(
                s,
                maps,
                p,
                axis(name, 0),
                axis(name, 2),
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
    Ok(())
}
pub(crate) fn connect(s: &mut Surface, a: [usize; 3], b: [usize; 3]) {
    let cost = |k: usize| {
        (0..3)
            .map(|i| norm(sub(s.points[a[i]], s.points[b[(i + k) % 3]])).powi(2))
            .sum::<f64>()
    };
    let shift = (0..3).min_by(|&a, &b| cost(a).total_cmp(&cost(b))).unwrap();
    let b: [usize; 3] = std::array::from_fn(|i| b[(i + shift) % 3]);
    for i in 0..3 {
        let j = (i + 1) % 3;
        s.faces.extend([[a[i], b[i], b[j]], [a[i], b[j], a[j]]]);
    }
}
pub(crate) fn write_maps(raw: &[u8], maps: &[Maps]) -> Result<Vec<u8>> {
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
            surface::numeric_f32(maps.iter().map(|m| m[k])),
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
    out.set(state, "vertexFormat", 37, surface::string_array(&formats)?)?;
    let bytes = out.bytes()?;
    let checked = Dmx::parse(bytes.clone())?;
    if surface::binds(&checked)?.skeleton != surface::binds(&d)?.skeleton {
        return Err(invalid("Automatic maps changed the skeleton."));
    }
    Ok(bytes)
}
