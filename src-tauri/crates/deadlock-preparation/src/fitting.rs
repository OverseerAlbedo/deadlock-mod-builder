//! Explicit geometry fitting choices; original hero bind authority is unchanged.
use crate::{invalid, Result};
use deadlock_project::PreparationOptions;

pub const MATCH_TARGET: &str = "match_target";
pub const PRESERVE_PROPORTIONS: &str = "preserve_proportions";
pub const PROPORTIONS_POLICY: &str = "source-proportions-v1";
/// Feet on the floor choices; must equal `retarget.py FLOOR_MODES`.
pub const FLOOR_MODES: [&str; 3] = ["ankles", "legs", "lift"];
/// Leg joints choices (Preserve source proportions only); must equal
/// `retarget.py LEG_JOINT_MODES` and `models.ts LEG_JOINT_MODES`. "pelvis" is
/// the fit as before (the default and every older project).
pub const LEG_JOINT_MODES: [&str; 5] = ["pelvis", "hips", "scaled", "scaled_joints", "joints"];
/// Leg joints choices that put the legs on the hero's joints; Feet on the floor
/// "legs" is refused with them (`retarget.py LEGS_ON_HERO_JOINTS`).
pub const LEGS_ON_HERO_JOINTS: [&str; 2] = ["scaled_joints", "joints"];

pub fn arm_skinning(options: &PreparationOptions) -> Result<&str> {
    match options.values.get("arm_skinning") {
        None => Ok("source"), // Existing projects keep their accepted skinning.
        Some(value) => match value.as_str() {
            Some("source") => Ok("source"),
            Some("smooth_twist") => Ok("smooth_twist"),
            _ => Err(invalid("Choose a supported Arm skinning option in Conversion review.")),
        },
    }
}

pub fn arm_joints(options: &PreparationOptions) -> Result<bool> {
    match options.values.get("fit_arm_joints") {
        None => Ok(true),
        Some(value) => value.as_bool().ok_or_else(|| invalid("Model fitting: Align arms to hero joints must be enabled or disabled.")),
    }
}

/// Bend the source fingers to the hero's resting curl (on unless turned off).
pub fn finger_curl(options: &PreparationOptions) -> Result<bool> {
    match options.values.get("fit_finger_curl") {
        None => Ok(true),
        Some(value) => value.as_bool().ok_or_else(|| invalid("Model fitting: Bend fingers like the hero must be enabled or disabled.")),
    }
}

/// Model fitting -> Feet on the floor (`retarget.py FLOOR_MODES`): "ankles"
/// keeps the fit lining up the ankles (the default and every older project).
pub fn floor(options: &PreparationOptions) -> Result<&str> {
    match options.values.get("fit_floor") {
        None => Ok(FLOOR_MODES[0]),
        Some(value) => value
            .as_str()
            .and_then(|mode| FLOOR_MODES.iter().copied().find(|known| *known == mode))
            .ok_or_else(|| invalid("Model fitting: choose a supported Feet on the floor option.")),
    }
}

/// Model fitting -> Leg joints (`retarget.py LEG_JOINT_MODES`), used with
/// Preserve source proportions only: "pelvis" keeps the fit as before.
pub fn leg_joints(options: &PreparationOptions) -> Result<&str> {
    match options.values.get("fit_leg_joints") {
        None => Ok(LEG_JOINT_MODES[0]),
        Some(value) => value
            .as_str()
            .and_then(|mode| LEG_JOINT_MODES.iter().copied().find(|known| *known == mode))
            .ok_or_else(|| invalid("Model fitting: choose a supported Leg joints option.")),
    }
}

pub const LEG_FLOOR_REFUSED: &str = "Model fitting: Feet on the floor \u{2192} Shorten or lengthen the legs cannot be used with a Leg joints choice that puts the legs on the hero's joints; it would take them off the joints again. Choose another Feet on the floor option.";

pub fn mode(options: &PreparationOptions) -> Result<&str> {
    arm_joints(options)?;
    finger_curl(options)?;
    let floor = floor(options)?;
    let legs = leg_joints(options)?;
    arm_skinning(options)?;
    let mode = match options.values.get("fit_mode") {
        None => MATCH_TARGET,
        Some(value) => match value.as_str() {
            Some(MATCH_TARGET) => MATCH_TARGET,
            Some(PRESERVE_PROPORTIONS) => PRESERVE_PROPORTIONS,
            _ => return Err(invalid("Choose a supported Model fitting option in Conversion review.")),
        },
    };
    // Match hero proportions ignores Leg joints, so a leftover choice never stops it.
    if mode == PRESERVE_PROPORTIONS && floor == "legs" && LEGS_ON_HERO_JOINTS.contains(&legs) {
        return Err(invalid(LEG_FLOOR_REFUSED));
    }
    Ok(mode)
}

pub fn check_policy(options: &PreparationOptions, policy: &str) -> Result<()> {
    let valid = match mode(options)? {
        PRESERVE_PROPORTIONS => policy == PROPORTIONS_POLICY,
        _ => [crate::service::REST_POLICY, crate::following::POLICY].contains(&policy),
    };
    if !valid {
        return Err(invalid("Prepared geometry does not match the selected Model fitting option. Convert the original source again and choose Use prepared model."));
    }
    Ok(())
}

pub fn check_unchanged(previous: &PreparationOptions, current: &PreparationOptions) -> Result<()> {
    if mode(previous)? != mode(current)? || arm_skinning(previous)? != arm_skinning(current)? || floor(previous)? != floor(current)? || (mode(current)? == PRESERVE_PROPORTIONS && (arm_joints(previous)? != arm_joints(current)? || leg_joints(previous)? != leg_joints(current)?)) {
        return Err(invalid("Model fitting changed. Convert the original source and choose Use prepared model before building or re-exporting corrections."));
    }
    Ok(())
}
