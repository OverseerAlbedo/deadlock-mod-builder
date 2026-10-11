//! Source/template gates accept a hierarchy already validated by ModelDoc parsing.
use crate::{
    dmx_bind::{self, BindAudit},
    extensions, invalid,
    skinning::{self, SkinningAudit},
    source_identity::{self, SourceContract},
    source_skeleton::SourceAuditReport,
    transforms::{self, ScaleAudit},
    IdentityAudit, Result, Skeleton,
};
use deadlock_dmx::Dmx;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkeletonReference {
    pub sha256: String,
    #[serde(deserialize_with = "crate::source_skeleton::required_cli_skeleton")]
    pub skeleton: Skeleton,
    #[serde(default)]
    pub scale_policy: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceBoneAudit {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_bind_audit: Option<BindAudit>,
    pub bone_count: usize,
    pub joint_palette_entries: usize,
    pub joint_palette_names: Vec<Option<String>>,
    pub required_names_and_parents_match: bool,
    pub skinning: SkinningAudit,
}
pub fn source_bones(
    d: &Dmx,
    required: &BTreeMap<String, Option<String>>,
    reference: Option<&SkeletonReference>,
    original_sha: Option<&str>,
) -> Result<SourceBoneAudit> {
    let joints: BTreeMap<_, _> = d
        .elements
        .iter()
        .enumerate()
        .filter(|(_, e)| e.kind == "DmeJoint")
        .collect();
    let names: BTreeSet<_> = joints.values().map(|e| e.name.clone()).collect();
    if names.len() != joints.len()
        || names != required.keys().cloned().collect()
        || joints
            .values()
            .map(|e| e.uuid)
            .collect::<BTreeSet<_>>()
            .len()
            != joints.len()
    {
        return Err(invalid(
            "Replacement DMX bone identities differ from required template",
        ));
    }
    let palettes = skinning::palettes(d)?;
    if palettes.len() != 1 {
        return Err(invalid(
            "Replacement DMX must have one unambiguous joint palette",
        ));
    }
    let palette = palettes.first().unwrap();
    let entries: Vec<_> = palette
        .iter()
        .filter_map(|i| usize::try_from(*i).ok())
        .filter(|i| joints.contains_key(i))
        .collect();
    if entries.len() != joints.len()
        || entries.iter().collect::<BTreeSet<_>>().len() != joints.len()
    {
        return Err(invalid(
            "Required DMX joints missing or duplicated in palette",
        ));
    }
    let mut parents = BTreeMap::new();
    for e in joints.values() {
        for child in skinning::ints(d, e.get("children"))?
            .into_iter()
            .filter_map(|i| usize::try_from(i).ok())
        {
            if let Some(child) = joints.get(&child) {
                if parents.insert(child.name.clone(), e.name.clone()).is_some() {
                    return Err(invalid("DMX joint has more than one parent"));
                }
            }
        }
    }
    for (n, p) in required {
        if parents.get(n) != p.as_ref() {
            return Err(invalid(
                "Replacement DMX hierarchy differs from keep-bone template",
            ));
        }
    }
    let mut bind = None;
    if let Some(r) = reference {
        if original_sha.is_some_and(|s| s != r.sha256) {
            return Err(invalid(
                "Skeleton reference belongs to different original model",
            ));
        }
        r.skeleton.validate(false)?;
        if r.skeleton.names.iter().cloned().collect::<BTreeSet<_>>() != names {
            return Err(invalid(
                "Replacement bone names differ from original reference",
            ));
        }
        for (i, n) in r.skeleton.names.iter().enumerate() {
            if parents.get(n).map(String::as_str) != r.skeleton.parent_name(i) {
                return Err(invalid(format!(
                    "Replacement bone parent differs from original: {n}"
                )));
            }
        }
        if let Some(policy) = &r.scale_policy {
            if policy != transforms::SCALE_POLICY {
                return Err(invalid("Unknown prepared skeleton scale policy"));
            }
            bind = Some(dmx_bind::audit_dmx_bind(d, &r.skeleton, true)?);
        }
    }
    let skin = skinning::skinning(d)?;
    if !skin.numeric_checks_passed {
        return Err(invalid(format!(
            "Replacement skinning audit failed: {:?}",
            skin.meshes
        )));
    }
    Ok(SourceBoneAudit {
        target_bind_audit: bind,
        bone_count: joints.len(),
        joint_palette_entries: palette.len(),
        joint_palette_names: palette
            .iter()
            .map(|i| {
                usize::try_from(*i)
                    .ok()
                    .and_then(|i| joints.get(&i))
                    .map(|e| e.name.clone())
            })
            .collect(),
        required_names_and_parents_match: true,
        skinning: skin,
    })
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompositeBindAudit {
    #[serde(flatten)]
    pub source: BindAudit,
    pub original_projection_identity: IdentityAudit,
    pub original_projection_scale: ScaleAudit,
    pub full_composite_identity: IdentityAudit,
    pub full_composite_scale: ScaleAudit,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateContractAudit {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_bind_audit: Option<CompositeBindAudit>,
    pub bone_count: usize,
    pub joint_palette_entries: usize,
    pub joint_palette_names: Vec<Option<String>>,
    pub required_names_and_parents_match: bool,
    pub skinning: SkinningAudit,
    pub template_original_only: bool,
    pub template_bone_count: usize,
    pub extra_bones_authorized_by_source: Vec<String>,
    pub source_modified: bool,
    pub weights_modified: bool,
}
pub fn template_contract(
    d: &Dmx,
    required: &BTreeMap<String, Option<String>>,
    original: &SourceAuditReport,
    extension: &SourceContract,
    reference: Option<&SkeletonReference>,
) -> Result<TemplateContractAudit> {
    let selected = extension.selected_meshes.iter().cloned().collect();
    let fresh = source_identity::source_contract(d, original, &selected, &extension.source_sha256)?
        .ok_or_else(|| invalid("Custom-bone source contract missing"))?;
    if &fresh != extension {
        return Err(invalid(
            "Custom-bone template source contract is stale or changed",
        ));
    }
    let source = &fresh.skeleton;
    let target = original
        .runtime_motion
        .skeleton
        .as_ref()
        .ok_or_else(|| invalid("Original skeleton missing"))?;
    if required.keys().collect::<BTreeSet<_>>() != target.names.iter().collect() {
        return Err(invalid(
            "Custom-bone template must retain exactly original hero bones",
        ));
    }
    for (i, n) in target.names.iter().enumerate() {
        if required[n].as_deref() != target.parent_name(i) {
            return Err(invalid(format!(
                "Custom-bone compile template changed original parent: {n}"
            )));
        }
    }
    let mut bind = None;
    if let Some(r) = reference {
        if Some(r.sha256.as_str()) != original.compiled_sha256.as_deref() {
            return Err(invalid(
                "Skeleton reference belongs to different original model",
            ));
        }
        crate::check_transfer(&r.skeleton, target)?;
        transforms::audit_scale_preservation(&r.skeleton, target, false)?;
        if let Some(policy) = &r.scale_policy {
            if policy != transforms::SCALE_POLICY {
                return Err(invalid("Unknown prepared skeleton scale policy"));
            }
            let mut raw = source.clone();
            let reverse: BTreeMap<_, _> = fresh
                .source_aliases
                .iter()
                .map(|(exported, canonical)| (canonical, exported))
                .collect();
            for n in &mut raw.names {
                if let Some(exported) = reverse.get(n) {
                    *n = (*exported).clone();
                }
            }
            let source_bind = dmx_bind::audit_dmx_bind(d, &raw, true)?;
            let projection = extensions::project_skeleton(source, &r.skeleton.names)?;
            let mut expected = source.clone();
            let lookup = r.skeleton.indices();
            for (i, n) in source.names.iter().enumerate() {
                if let Some(&j) = lookup.get(n.as_str()) {
                    expected.positions[i] = r.skeleton.positions[j];
                    expected.rotations[i] = r.skeleton.rotations[j];
                    expected.scales[i] = r.skeleton.scales[j];
                }
            }
            bind = Some(CompositeBindAudit {
                source: source_bind,
                original_projection_identity: crate::check_transfer(&projection, &r.skeleton)?,
                original_projection_scale: transforms::audit_scale_preservation(
                    &projection,
                    &r.skeleton,
                    true,
                )?,
                full_composite_identity: crate::check_transfer(source, &expected)?,
                full_composite_scale: transforms::audit_scale_preservation(
                    source, &expected, true,
                )?,
            });
        }
    }
    let skin = skinning::skinning(d)?;
    if !skin.numeric_checks_passed {
        return Err(invalid(format!(
            "Replacement skinning audit failed: {:?}",
            skin.meshes
        )));
    }
    let model = d
        .elements
        .iter()
        .find(|e| e.kind == "DmeModel" && e.get("jointList").is_some_and(|v| v.len() > 0))
        .ok_or_else(|| invalid("Model joint palette missing"))?;
    let palette = dmx_bind::refs(d, model, "jointList")?;
    Ok(TemplateContractAudit {
        target_bind_audit: bind,
        bone_count: source.names.len(),
        joint_palette_entries: palette.len(),
        joint_palette_names: palette
            .iter()
            .map(|i| &d.elements[*i])
            .map(|e| {
                if e.kind == "DmeJoint" {
                    Some(fresh.source_aliases.get(&e.name).unwrap_or(&e.name).clone())
                } else {
                    None
                }
            })
            .collect(),
        required_names_and_parents_match: true,
        skinning: skin,
        template_original_only: true,
        template_bone_count: required.len(),
        extra_bones_authorized_by_source: fresh.extra_names,
        source_modified: false,
        weights_modified: false,
    })
}
