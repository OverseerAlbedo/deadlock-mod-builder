//! Reviewed choices only. Presets never grant conversion/build approval.
use crate::{
    Result,
    algorithms::{self, Bone},
    invalid,
    service::Analysis,
};
use deadlock_project::PreparationOptions;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const POLICY: &str = "rig-mapping-preset-v1";
#[derive(Debug, Serialize, Deserialize)]
pub struct Preset {
    pub policy: String,
    pub fingerprint: String,
    pub options: PreparationOptions,
}

pub fn fingerprint(
    source: &[Bone],
    weighted: &[String],
    target: &impl Serialize,
    rotation: [f64; 3],
) -> Result<String> {
    let mut source = source.to_vec();
    source.sort_by(|a, b| a.name.cmp(&b.name));
    // Mesh statistics may change while the exact rig remains the same. They are
    // recalculated on every inspection and are not conversion authority.
    for bone in &mut source {
        bone.details.remove("skin");
    }
    let mut weighted = weighted.to_vec();
    weighted.sort();
    let bytes = serde_json::to_vec(&(
        POLICY,
        crate::automap::POLICY,
        source,
        weighted,
        target,
        rotation,
    ))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
pub fn key(analysis: &Analysis) -> Result<String> {
    fingerprint(
        &analysis.analysis.bones,
        &analysis.analysis.weighted_bones,
        &analysis.target,
        analysis.rotation,
    )
}
pub fn capture(analysis: &Analysis, options: &PreparationOptions) -> Result<Preset> {
    let target = analysis.target.iter().map(Bone::from).collect::<Vec<_>>();
    crate::fitting::mode(options)?;
    let errors = algorithms::review_issues(
        &target,
        &analysis.analysis.bones,
        &analysis.analysis.weighted_bones,
        "rigged_humanoid",
        "rigged_humanoid",
        options,
    );
    if !errors.is_empty() || options.values.get("reviewed").and_then(|v| v.as_bool()) != Some(true)
    {
        return Err(invalid(
            "Complete the mapping review before saving a reusable preset.",
        ));
    }
    let mut choices = PreparationOptions::default();
    for name in [
        "source_roles",
        "target_roles",
        "mapping",
        "mapping_anchors",
        "preserve_physics_bones",
        "physics_roots",
        "automatic_secondary_motion",
        "influence_limit",
        "fit_mode",
        "fit_arm_joints",
        "fit_finger_curl",
        "arm_skinning",
    ] {
        if let Some(value) = options.values.get(name) {
            choices.values.insert(name.into(), value.clone());
        }
    }
    Ok(Preset {
        policy: POLICY.into(),
        fingerprint: key(analysis)?,
        options: choices,
    })
}
pub fn restore(preset: Preset, fingerprint: &str) -> Result<PreparationOptions> {
    if preset.policy != POLICY || preset.fingerprint != fingerprint {
        return Err(invalid(
            "Saved mapping belongs to a different rig or mapping version.",
        ));
    }
    let mut options = PreparationOptions::default();
    for name in [
        "source_roles",
        "target_roles",
        "mapping",
        "mapping_anchors",
        "preserve_physics_bones",
        "physics_roots",
        "influence_limit",
        "automatic_secondary_motion",
        "fit_mode",
        "fit_arm_joints",
        "fit_finger_curl",
        "arm_skinning",
    ] {
        if let Some(value) = preset.options.values.get(name) {
            options.values.insert(name.into(), value.clone());
        }
    }
    crate::fitting::mode(&options)?;
    options.values.insert("reviewed".into(), false.into());
    options
        .values
        .insert("follow_motion_reviewed".into(), false.into());
    Ok(options)
}
