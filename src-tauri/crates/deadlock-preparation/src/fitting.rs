//! Explicit geometry fitting choices; original hero bind authority is unchanged.
use crate::{invalid, Result};
use deadlock_project::PreparationOptions;

pub const MATCH_TARGET: &str = "match_target";
pub const PRESERVE_PROPORTIONS: &str = "preserve_proportions";
pub const PROPORTIONS_POLICY: &str = "source-proportions-v1";

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

pub fn mode(options: &PreparationOptions) -> Result<&str> {
    arm_joints(options)?;
    arm_skinning(options)?;
    match options.values.get("fit_mode") {
        None => Ok(MATCH_TARGET),
        Some(value) => match value.as_str() {
            Some(MATCH_TARGET) => Ok(MATCH_TARGET),
            Some(PRESERVE_PROPORTIONS) => Ok(PRESERVE_PROPORTIONS),
            _ => Err(invalid("Choose a supported Model fitting option in Conversion review.")),
        },
    }
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
    if mode(previous)? != mode(current)? || arm_skinning(previous)? != arm_skinning(current)? || (mode(current)? == PRESERVE_PROPORTIONS && arm_joints(previous)? != arm_joints(current)?) {
        return Err(invalid("Model fitting changed. Convert the original source and choose Use prepared model before building or re-exporting corrections."));
    }
    Ok(())
}
