//! Added branches retain original authority and may use only ordinary mesh/animation flags.
use crate::transforms::{audit_scale_preservation, bind_state, ScaleAudit};
use crate::{check_transfer, invalid, IdentityAudit, Result, Skeleton};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
pub const POLICY: &str = "ordinary-child-bone-extension-v1";
pub const ORDINARY_FLAGS: u32 = 0x40 | 0x80 | 0x3fc00;
pub fn project_skeleton(skeleton: &Skeleton, names: &[String]) -> Result<Skeleton> {
    skeleton.validate(true)?;
    let lookup = skeleton.indices();
    let indices: BTreeMap<_, _> = names
        .iter()
        .enumerate()
        .map(|(i, n)| (n.as_str(), i))
        .collect();
    if indices.len() != names.len() {
        return Err(invalid("Duplicate projection identities"));
    }
    let mut result = Skeleton {
        names: names.to_vec(),
        parents: vec![],
        positions: vec![],
        rotations: vec![],
        scales: vec![],
        flags: vec![],
    };
    for name in names {
        let &at = lookup
            .get(name.as_str())
            .ok_or_else(|| invalid(format!("Extended skeleton missing original identities: {name}")))?;
        result.parents.push(match skeleton.parent_name(at) {
            None => -1,
            Some(p) => *indices
                .get(p)
                .ok_or_else(|| invalid("Original bone reparented through custom bone"))?
                as i32,
        });
        result.positions.push(skeleton.positions[at]);
        result.rotations.push(skeleton.rotations[at]);
        result.scales.push(skeleton.scales[at]);
        result.flags.push(skeleton.flags[at]);
    }
    Ok(result)
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionAudit {
    pub policy: String,
    pub extra_names: Vec<String>,
    pub extra_count: usize,
    pub original_identity: IdentityAudit,
    pub original_scale: ScaleAudit,
    pub anchors: BTreeMap<String, String>,
    pub original_bone_count: usize,
    pub compiled_bone_count: usize,
    pub new_cloth_simulation: bool,
    pub in_game_motion_tested: bool,
}
pub fn extension_identity(actual: &Skeleton, original: &Skeleton) -> Result<ExtensionAudit> {
    check_transfer(original, original)?;
    check_transfer(actual, actual)?;
    bind_state(actual)?;
    let reference: BTreeSet<_> = original.names.iter().map(String::as_str).collect();
    let names: BTreeSet<_> = actual.names.iter().map(String::as_str).collect();
    if !reference.is_subset(&names) || reference.len() >= names.len() {
        return Err(invalid(
            "An extension requires an unchanged original subset plus additional source bones",
        ));
    }
    let projected = project_skeleton(actual, &original.names)?;
    let identity = check_transfer(&projected, original)?;
    let scale = audit_scale_preservation(&projected, original, false)?;
    let extras: Vec<_> = actual
        .names
        .iter()
        .filter(|n| !reference.contains(n.as_str()))
        .cloned()
        .collect();
    let mut anchors = BTreeMap::new();
    for name in &extras {
        let at = actual.names.iter().position(|n| n == name).unwrap();
        if actual.flags[at] & !ORDINARY_FLAGS != 0 {
            return Err(invalid(format!(
                "Custom bone requires unsupported runtime flags: {name}"
            )));
        }
        let mut parent = actual.parents[at];
        while parent >= 0 && !reference.contains(actual.names[parent as usize].as_str()) {
            parent = actual.parents[parent as usize];
        }
        if parent < 0 {
            return Err(invalid(format!(
                "Custom bone must descend from original bone: {name}"
            )));
        }
        anchors.insert(name.clone(), actual.names[parent as usize].clone());
    }
    Ok(ExtensionAudit {
        policy: POLICY.into(),
        extra_count: extras.len(),
        extra_names: extras,
        original_identity: identity,
        original_scale: scale,
        anchors,
        original_bone_count: original.names.len(),
        compiled_bone_count: actual.names.len(),
        new_cloth_simulation: false,
        in_game_motion_tested: false,
    })
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceBinding {
    pub original_sha256: String,
    pub source_sha256: String,
    pub skeleton: Skeleton,
    pub extra_names: Vec<String>,
    pub required_extra_names: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceBindingAudit {
    pub source_sha256: String,
    pub identity: IdentityAudit,
    pub scale: ScaleAudit,
    pub weights_reassigned: bool,
    pub source_bones_checked: Vec<String>,
}
pub fn source_binding(
    actual: &Skeleton,
    original: &Skeleton,
    source: &SourceBinding,
    original_sha: &str,
) -> Result<SourceBindingAudit> {
    if source.original_sha256 != original_sha {
        return Err(invalid(
            "Custom bone source authority belongs to another original model",
        ));
    }
    actual.validate(true)?;
    original.validate(true)?;
    source.skeleton.validate(true)?;
    let original_names: BTreeSet<_> = original.names.iter().map(String::as_str).collect();
    let extras: BTreeSet<_> = actual
        .names
        .iter()
        .map(String::as_str)
        .filter(|n| !original_names.contains(n))
        .collect();
    let allowed: BTreeSet<_> = source.extra_names.iter().map(String::as_str).collect();
    if !extras.is_subset(&allowed)
        || source
            .required_extra_names
            .iter()
            .any(|n| !extras.contains(n.as_str()))
    {
        return Err(invalid(
            "Compiled custom bones differ from selected source bindings",
        ));
    }
    let mut expected = Skeleton {
        names: actual.names.clone(),
        parents: vec![],
        positions: vec![],
        rotations: vec![],
        scales: vec![],
        flags: vec![],
    };
    let names = actual.indices();
    for name in &actual.names {
        let reference = if original_names.contains(name.as_str()) {
            original
        } else {
            &source.skeleton
        };
        let at = reference
            .names
            .iter()
            .position(|n| n == name)
            .ok_or_else(|| invalid("Missing retained extra source identity"))?;
        expected.parents.push(match reference.parent_name(at) {
            None => -1,
            Some(p) => *names
                .get(p)
                .ok_or_else(|| invalid("Compiled custom bone lost source parent"))?
                as i32,
        });
        expected.positions.push(reference.positions[at]);
        expected.rotations.push(reference.rotations[at]);
        expected.scales.push(reference.scales[at]);
        expected.flags.push(reference.flags[at]);
    }
    Ok(SourceBindingAudit {
        source_sha256: source.source_sha256.clone(),
        identity: check_transfer(actual, &expected)?,
        scale: audit_scale_preservation(actual, &expected, false)?,
        weights_reassigned: false,
        source_bones_checked: extras.into_iter().map(str::to_string).collect(),
    })
}
