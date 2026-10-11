//! Bounds act at the edge of free motion; they do not increase pose attraction.
use crate::{
    hair_motion, invalid,
    parts::Part,
    secondary::{Group, HairLengthAdjustment},
    surface_geometry::{dot, norm, sub},
    Result,
};
use deadlock_skeleton::{transforms::BindState, Skeleton};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Serialize)]
pub struct JumpProtection {
    pub root: String,
    pub anchor: String,
    pub height_reference: String,
    pub reference_position_world: [f64; 3],
    pub up_axis_world: [f64; 3],
    pub ceiling_from_reference_inches: f64,
    pub stretch_tethers: bool,
    pub travel_radius_by_bone: BTreeMap<String, f64>,
}

/// Use the same ring size as the existing hair proxy. The conservative radius
/// subtraction below bounds all three points, regardless of ring orientation.
pub(crate) fn ring_radius(part: &Part) -> f64 {
    (part.chain_length_inches / (part.weighted_chain.len() - 1) as f64 * 0.08)
        .clamp(0.05, 0.45)
}

pub(crate) fn plan(
    group: &Group,
    part: &Part,
    sk: &Skeleton,
    state: &BindState,
    length: &HairLengthAdjustment,
) -> Result<JumpProtection> {
    let lookup = sk.indices();
    let pos = |name: &str| std::array::from_fn(|r| state.world[lookup[name]][r][3]);
    let named = |name: &str| sk.names.iter().find(|n| n.eq_ignore_ascii_case(name));
    let body = named("head").zip(named("pelvis")).and_then(|(head, pelvis)| {
        let direction = sub(pos(head), pos(pelvis));
        let size = norm(direction);
        (size > 1e-4 && size.is_finite()).then(|| (pos(head), direction.map(|v| v / size)))
    });
    let (reference, up, reference_name) = if let Some((head, up)) = body {
        (head, up, "head_to_pelvis")
    } else {
        // Prepared Source models are Z-up. For an unrecognized body use the
        // attachment's local Z, so this fallback rotates/scales with the rig.
        let anchor = lookup[group.anchor.as_str()];
        let axis = std::array::from_fn(|r| state.world[anchor][r][2]);
        (pos(&group.anchor), axis.map(|v| v / norm(axis)), "attachment_local_z")
    };
    let radius = ring_radius(part);
    // Never clip the authored hairstyle (high ponytail roots/upward helpers
    // can already be above the head joint). Allow a small body-scaled margin.
    let highest = group.bones.iter()
        .map(|n| dot(sub(pos(n), reference), up))
        .fold(0_f64, f64::max);
    let ceiling = highest + radius + length.short_hair_limit_inches * 0.12;
    let coordinates = hair_motion::coordinates(group, part, sk, state)?
        .ok_or_else(|| invalid("Long-hair travel protection needs a connected strand."))?;
    let mut limits = BTreeMap::new();
    for name in &group.bones {
        let free_travel = part.chain_length_inches * coordinates[name] * 0.8
            + length.short_hair_limit_inches * 0.03;
        let height_travel = ceiling - dot(sub(pos(name), reference), up) - radius;
        let travel = free_travel.min(height_travel);
        if !travel.is_finite() || travel <= 0. || !(travel as f32).is_finite() {
            return Err(invalid("Long-hair travel limit is outside native float range."));
        }
        // Zero means *unbounded* to the native compiler, not pinned.
        limits.insert(name.clone(), travel);
    }
    Ok(JumpProtection {
        root: part.root.clone(), anchor: group.anchor.clone(),
        height_reference: reference_name.into(), reference_position_world: reference,
        up_axis_world: up, ceiling_from_reference_inches: ceiling,
        stretch_tethers: true, travel_radius_by_bone: limits,
    })
}
