//! Apply the shared hair profile to the visible strand, not its helper bones.
use crate::{
    invalid,
    parts::{Kind, Part},
    secondary::{Group, HairLengthAdjustment},
    surface_geometry::{norm, sub},
    Result,
};
use deadlock_skeleton::{transforms::BindState, Skeleton};
use std::collections::BTreeMap;

pub const POLICY: &str = "bounded-length-adaptive-hair-motion-v3";

/// Keep short hair's accepted goals and reduce pose attraction on longer
/// strands. This is a bounded authoring heuristic, not a prediction of motion.
/// The reference comes from the character, never the other selected hair
/// groups, the number of joints, or an export helper's length.
pub(crate) fn length_adjustment(
    group: &Group,
    part: &Part,
    sk: &Skeleton,
    state: &BindState,
    choice: Option<&str>,
) -> Option<HairLengthAdjustment> {
    if part.kind != Kind::Hair
        || part.weighted_chain.len() < 2
        || !part.chain_length_inches.is_finite()
        || part.chain_length_inches <= 1e-4
        || choice.is_some_and(|s| s != "automatic")
    {
        return None;
    }
    let named = |name: &str| sk.names.iter().position(|n| n.eq_ignore_ascii_case(name));
    let position = |i: usize| std::array::from_fn(|r| state.world[i][r][3]);
    let torso = named("head").zip(named("pelvis"))
        .map(|(head, pelvis)| norm(sub(position(head), position(pelvis))))
        .filter(|length| length.is_finite() && *length > 1e-4);
    let (limit, reference) = if let Some(torso) = torso {
        (torso * 0.5, "head_to_pelvis")
    } else {
        // Prepared DMX uses inches. Honor inherited scale for rigs without the
        // standard body landmarks rather than deriving size from hair itself.
        let anchor = sk.indices()[group.anchor.as_str()];
        (12. * state.accumulated[anchor], "attachment_scale")
    };
    if !limit.is_finite() || limit <= 1e-4 {
        return None;
    }
    let multiplier = (limit / part.chain_length_inches).clamp(0.25, 1.);
    (multiplier < 1.).then(|| HairLengthAdjustment {
        root: part.root.clone(),
        strand_length_inches: part.chain_length_inches,
        short_hair_limit_inches: limit,
        goal_strength_multiplier: multiplier,
        scale_reference: reference.into(),
    })
}

/// Normalized distance along the weighted chain. Leading helpers share the
/// first visible bone's root setting; trailing helpers share the tip setting.
/// Intermediate helpers interpolate between the same weighted endpoints, so
/// adding export helpers cannot change the visible bones' movement profile.
pub(crate) fn coordinates(
    group: &Group,
    part: &Part,
    sk: &Skeleton,
    state: &BindState,
) -> Result<Option<BTreeMap<String, f64>>> {
    if part.kind != Kind::Hair || part.weighted_chain.len() < 2 {
        return Ok(None);
    }
    let lookup = sk.indices();
    let position = |n: &str| std::array::from_fn(|r| state.world[lookup[n]][r][3]);
    let chain = &part.weighted_chain;
    let lengths = chain
        .windows(2)
        .map(|pair| norm(sub(position(&pair[1]), position(&pair[0]))))
        .collect::<Vec<_>>();
    let total: f64 = lengths.iter().sum();
    if !total.is_finite() || total <= 1e-4 {
        // Part analysis flags this unsupported strand for review.
        return Ok(None);
    }
    let mut coordinates = BTreeMap::from([(chain[0].clone(), 0.)]);
    let mut traveled = 0.;
    for (pair, length) in chain.windows(2).zip(lengths) {
        let start = traveled / total;
        traveled += length;
        let end = (traveled / total).clamp(0., 1.);
        coordinates.insert(pair[1].clone(), end);

        let mut path = vec![pair[1].as_str()];
        let mut at = lookup[pair[1].as_str()];
        while sk.names[at] != pair[0] {
            let parent = sk.parents[at];
            if parent < 0 || path.len() > group.bones.len() {
                return Err(invalid("Hair movement needs one connected weighted chain."));
            }
            at = parent as usize;
            path.push(sk.names[at].as_str());
        }
        path.reverse();
        let distances = path
            .windows(2)
            .map(|p| norm(sub(position(p[1]), position(p[0]))))
            .collect::<Vec<_>>();
        let helper_length: f64 = distances.iter().sum();
        if !helper_length.is_finite() || helper_length <= 1e-5 {
            return Err(invalid("Hair movement contains a collapsed segment."));
        }
        let mut helper_distance = 0.;
        for (name, distance) in path.iter().skip(1).zip(distances) {
            helper_distance += distance;
            // Do not round the weighted endpoints through the helper path.
            coordinates.entry((*name).into()).or_insert_with(|| {
                start + (end - start) * (helper_distance / helper_length).clamp(0., 1.)
            });
        }
    }
    // Groups are parent-first. Unweighted side branches retain their nearest
    // weighted/path parent's value; they never extend the strand's length.
    for name in &group.bones {
        if !coordinates.contains_key(name) {
            let parent = sk.parents[lookup[name.as_str()]];
            let t = if parent < 0 {
                0.
            } else {
                coordinates
                    .get(&sk.names[parent as usize])
                    .copied()
                    .unwrap_or(0.)
            };
            coordinates.insert(name.clone(), t);
        }
    }
    Ok(Some(coordinates))
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadlock_skeleton::transforms;

    fn rig(length: f64) -> (Skeleton, Group, Part) {
        let sk = Skeleton {
            names: vec!["pelvis".into(), "head".into(), "dmb_secondary_hair".into(), "dmb_secondary_tip".into()],
            parents: vec![-1, 0, 1, 2],
            positions: vec![[0.; 3], [0., 24., 0.], [0.; 3], [0., -length, 0.]],
            rotations: vec![[0., 0., 0., 1.]; 4],
            scales: vec![1.; 4],
            flags: vec![0; 4],
        };
        let group = Group {
            source_root: "hair".into(), root: sk.names[2].clone(), anchor: "head".into(),
            bones: sk.names[2..].to_vec(), weighted_bones: sk.names[2..].to_vec(),
        };
        let part = Part {
            root: "hair".into(), kind: Kind::Hair, reason: String::new(),
            weighted_chain: sk.names[2..].to_vec(), chain_length_inches: length,
            shape_radii_inches: [1.; 3], roll_locked: true, review_recommended: false,
        };
        (sk, group, part)
    }

    #[test]
    fn body_relative_softening_is_bounded_continuous_and_scale_independent() {
        for (length, expected) in [(4., 1.), (12., 1.), (12.00001, 12./12.00001), (24., 0.5), (48., 0.25), (1200., 0.25)] {
            let (mut sk, group, mut part) = rig(length);
            let state = transforms::bind_state(&sk).unwrap();
            let original = length_adjustment(&group, &part, &sk, &state, None);
            let actual = original.as_ref().map_or(1., |p| p.goal_strength_multiplier);
            assert!((actual - expected).abs() < 1e-10);
            if let Some(adjustment) = original {
                assert_eq!(adjustment.scale_reference, "head_to_pelvis");
            }
            sk.scales[0] = 3.;
            sk.positions[0] = [100., -200., 300.];
            sk.rotations[0] = [0., 0., std::f64::consts::FRAC_1_SQRT_2, std::f64::consts::FRAC_1_SQRT_2];
            part.chain_length_inches *= 3.;
            let state = transforms::bind_state(&sk).unwrap();
            let changed = length_adjustment(&group, &part, &sk, &state, None);
            assert!((changed.map_or(1., |p| p.goal_strength_multiplier) - expected).abs() < 1e-10);
        }
    }

    #[test]
    fn softening_never_changes_other_parts_or_explicit_movement_choices() {
        let (sk, group, mut part) = rig(48.);
        let state = transforms::bind_state(&sk).unwrap();
        for choice in ["soft", "stiff", "balanced"] {
            assert!(length_adjustment(&group, &part, &sk, &state, Some(choice)).is_none());
        }
        assert!(length_adjustment(&group, &part, &sk, &state, Some("automatic")).is_some());
        for kind in [Kind::Tail, Kind::Cloth, Kind::Streamer, Kind::Ear, Kind::Accessory] {
            part.kind = kind;
            assert!(length_adjustment(&group, &part, &sk, &state, None).is_none());
        }
    }
}
