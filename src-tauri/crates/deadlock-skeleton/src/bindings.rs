//! Read-only compiled palette, inverse-bind, and animation rest-binding validation.
use crate::transforms::{audit_scale_preservation, bind_state, identity, multiply, ScaleAudit};
use crate::{
    array, check_transfer, field, integer, invalid, number, quaternion_component_delta,
    skeleton_identity, string, vector, IdentityAudit, Result, Skeleton,
};
use deadlock_resource::Value;
use deadlock_vpk::ResourceBlock;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
fn ints(value: &Value) -> Result<Vec<i64>> {
    array(value)?.iter().map(integer).collect()
}
fn strings(value: &Value) -> Result<Vec<String>> {
    array(value)?
        .iter()
        .map(|v| string(v).map(str::to_string))
        .collect()
}
fn ordinal(item: &Value, new: &str, old: &str) -> Result<usize> {
    usize::try_from(integer(
        item.get(new)
            .or_else(|| item.get(old))
            .ok_or_else(|| invalid("Missing embedded binding index"))?,
    )?)
    .map_err(|_| invalid("Invalid embedded binding index"))
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshRow {
    pub name: Option<String>,
    pub mesh_index: usize,
    pub remapped_bones: usize,
    pub extra_remap_entries_bounds_checked: usize,
    pub identities_match: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshAudit {
    #[serde(flatten)]
    pub identity: IdentityAudit,
    pub meshes: Vec<MeshRow>,
    pub compiler_arrays_modified: bool,
    pub vertex_blendindices_decoded: bool,
    pub live_ag2_deformation_validated: bool,
}
pub fn audit_meshes(
    data: &Value,
    ctrl: &Value,
    mdats: &[Value],
    blocks: &[ResourceBlock],
    reference: &Skeleton,
) -> Result<MeshAudit> {
    let skeleton = Skeleton::from_value(field(data, "m_modelSkeleton")?)?;
    let identity = skeleton_identity(&skeleton, reference)?;
    let starts = ints(field(data, "m_remappingTableStarts")?)?;
    let table = ints(field(data, "m_remappingTable")?)?;
    if starts.is_empty()
        || starts[0] != 0
        || starts.windows(2).any(|v| v[0] > v[1])
        || starts.iter().any(|&i| i < 0 || i as usize > table.len())
        || table
            .iter()
            .any(|&i| i < 0 || i as usize >= skeleton.names.len())
    {
        return Err(invalid("Invalid compiled mesh remapping table"));
    }
    let ordinals: Vec<_> = blocks
        .iter()
        .enumerate()
        .filter(|(_, b)| b.name == "MDAT")
        .map(|(i, _)| i)
        .collect();
    if ordinals.len() != mdats.len() {
        return Err(invalid("Every embedded mesh DATA block is required"));
    }
    let by_block: BTreeMap<_, _> = ordinals.into_iter().zip(mdats).collect();
    let entries = array(field(ctrl, "embedded_meshes")?)?;
    if entries.is_empty() {
        return Err(invalid(
            "Compiled identity audit requires embedded mesh metadata",
        ));
    }
    let mut meshes = vec![];
    for item in entries {
        let index = ordinal(item, "m_nMeshIndex", "mesh_index")?;
        let block = ordinal(item, "m_nDataBlock", "data_block")?;
        let mesh = by_block
            .get(&block)
            .ok_or_else(|| invalid("CTRL mesh points to missing mesh block"))?;
        if index >= starts.len() {
            return Err(invalid("CTRL mesh points to missing skeleton slice"));
        }
        let bones = array(field(field(mesh, "m_skeleton")?, "m_bones")?)?;
        let start = starts[index] as usize;
        let end = starts.get(index + 1).copied().unwrap_or(table.len() as i64) as usize;
        let remap = &table[start..end];
        if bones.is_empty() || remap.len() < bones.len() {
            return Err(invalid("Compiled mesh remap shorter than bone list"));
        }
        let local_names = bones
            .iter()
            .map(|b| string(field(b, "m_boneName")?))
            .collect::<Result<BTreeSet<_>>>()?;
        for (bone, &model_index) in bones.iter().zip(remap) {
            let name = string(field(bone, "m_boneName")?)?;
            let model_index = model_index as usize;
            if name != skeleton.names[model_index] {
                return Err(invalid(format!(
                    "Compiled mesh remap bone identity mismatch: {name}"
                )));
            }
            let mut parent = skeleton.parents[model_index];
            while parent >= 0 && !local_names.contains(skeleton.names[parent as usize].as_str()) {
                parent = skeleton.parents[parent as usize];
            }
            let expected = if parent < 0 {
                ""
            } else {
                skeleton.names[parent as usize].as_str()
            };
            let observed = bone
                .get("m_parentName")
                .map(string)
                .transpose()?
                .unwrap_or("");
            if expected != observed {
                return Err(invalid(format!(
                    "Compiled mesh remap parent identity mismatch: {name}"
                )));
            }
        }
        meshes.push(MeshRow {
            name: item
                .get("m_Name")
                .or_else(|| item.get("name"))
                .map(string)
                .transpose()?
                .map(str::to_string),
            mesh_index: index,
            remapped_bones: bones.len(),
            extra_remap_entries_bounds_checked: remap.len() - bones.len(),
            identities_match: true,
        });
    }
    Ok(MeshAudit {
        identity,
        meshes,
        compiler_arrays_modified: false,
        vertex_blendindices_decoded: false,
        live_ag2_deformation_validated: false,
    })
}

pub fn animation_bindings(group: &Value, skeleton: &Skeleton) -> Result<()> {
    skeleton.validate(false)?;
    let key = field(group, "m_decodeKey")?;
    let bones = array(field(key, "m_boneArray")?)?;
    let names = bones
        .iter()
        .map(|b| string(field(b, "m_name")?))
        .collect::<Result<Vec<_>>>()?;
    let model = skeleton.indices();
    if names.iter().collect::<BTreeSet<_>>().len() != names.len()
        || names.iter().any(|n| !model.contains_key(n))
    {
        return Err(invalid(
            "Animation decode skeleton has duplicate/unknown identities",
        ));
    }
    for bone in bones {
        let parent = integer(field(bone, "m_parent")?)?;
        if parent < -1 || parent >= bones.len() as i64 {
            return Err(invalid(
                "Animation decode skeleton has invalid parent indices",
            ));
        }
        let name = string(field(bone, "m_name")?)?;
        let observed = if parent < 0 {
            None
        } else {
            Some(names[parent as usize])
        };
        if observed != skeleton.parent_name(model[name]) {
            return Err(invalid(format!(
                "Animation decode parent identity mismatch: {name}"
            )));
        }
    }
    let mut count = 0;
    let mut all_indices = vec![];
    for channel in array(field(key, "m_dataChannelArray")?)? {
        let elements = strings(field(channel, "m_szElementNameArray")?)?;
        let indices = ints(field(channel, "m_nElementIndexArray")?)?;
        let masks = ints(field(channel, "m_nElementMaskArray")?)?;
        if elements.len() != indices.len() || elements.len() != masks.len() {
            return Err(invalid(
                "Animation channel binding arrays have inconsistent lengths",
            ));
        }
        count += elements.len();
        all_indices.extend(indices);
        if string(field(channel, "m_szChannelClass")?)? != "BoneChannel" {
            continue;
        }
        let variable = string(field(channel, "m_szVariableName")?)?;
        let kind = integer(field(channel, "m_nType")?)?;
        if ![("Position", 3), ("Angle", 4), ("Scale", 1)].contains(&(variable, kind)) {
            return Err(invalid("Unsupported bone animation channel layout"));
        }
        if elements.iter().collect::<BTreeSet<_>>().len() != elements.len()
            || elements.iter().any(|n| !names.contains(&n.as_str()))
        {
            return Err(invalid("Animation channel duplicate/unknown bone identity"));
        }
    }
    all_indices.sort_unstable();
    if integer(field(key, "m_nChannelElements")?)? != count as i64
        || all_indices.iter().copied().ne(0..count as i64)
    {
        return Err(invalid("Animation channel element count is inconsistent"));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InverseMeshRow {
    pub mesh_ordinal: usize,
    pub bone_count: usize,
    pub max_linear_residual: f64,
    pub worst_linear_bone: String,
    pub max_translation_residual_inches: f64,
    pub worst_translation_bone: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InverseAudit {
    pub meshes: Vec<InverseMeshRow>,
    pub both_multiplication_orders_checked: bool,
    pub linear_component_limit: f64,
    pub translation_component_limit_inches: f64,
    pub max_linear_residual: f64,
    pub max_translation_residual_inches: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_qualified_custom_bones: Option<CustomBindLimits>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomBindLimits {
    pub names: Vec<String>,
    pub linear_component_limit: f64,
    pub translation_component_limit_inches: f64,
}
pub fn inverse_binds(mdats: &[Value], skeleton: &Skeleton) -> Result<InverseAudit> {
    inverse_binds_with_custom_bones(mdats, skeleton, &BTreeSet::new())
}
/// Numeric subcheck. The caller must separately prove `custom` using selected
/// source authority; production callers use `audit_bindings_with_custom_bones`.
pub fn inverse_binds_with_custom_bones(
    mdats: &[Value],
    skeleton: &Skeleton,
    custom: &BTreeSet<String>,
) -> Result<InverseAudit> {
    // This compares two independently reconstructed *compiled* bind tables.
    // Only source-qualified ordinary additions use accumulated-bind budgets.
    // Original hero / tested physics bones retain their existing tight limits.
    // Native compilers introduce small
    // hierarchy/decomposition differences even when source locals pass the
    // unchanged transfer gate. No matrix or authored scale is rewritten.
    const LINEAR_LIMIT: f64 = crate::transforms::WORLD_SCALE_TOLERANCE;
    const TRANSLATION_LIMIT: f64 = crate::transforms::WORLD_POSITION_TOLERANCE_MM / 25.4;
    let state = bind_state(skeleton)?;
    let lookup = skeleton.indices();
    let mut meshes = vec![];
    for (ordinal, mesh) in mdats.iter().enumerate() {
        let bones = array(field(field(mesh, "m_skeleton")?, "m_bones")?)?;
        if bones.is_empty() {
            return Err(invalid("Compiled mesh has no skeleton bones"));
        }
        let mut row = InverseMeshRow {
            mesh_ordinal: ordinal,
            bone_count: bones.len(),
            max_linear_residual: 0.,
            worst_linear_bone: String::new(),
            max_translation_residual_inches: 0.,
            worst_translation_bone: String::new(),
        };
        for bone in bones {
            let name = string(field(bone, "m_boneName")?)?;
            let at = *lookup
                .get(name)
                .ok_or_else(|| invalid("Unknown inverse-bind bone"))?;
            let values = vector::<12>(field(bone, "m_invBindPose")?)?;
            let mut inverse = identity();
            for (row, data) in inverse.iter_mut().take(3).zip(values.chunks_exact(4)) {
                row.copy_from_slice(data);
            }
            let world = &state.world[at];
            let mut linear = 0_f64;
            let mut translation = 0_f64;
            for residual in [multiply(world, &inverse), multiply(&inverse, world)] {
                for i in 0..3 {
                    for j in 0..4 {
                        let delta = (residual[i][j] - if i == j { 1. } else { 0. }).abs();
                        if !delta.is_finite() {
                            return Err(invalid("Nonfinite inverse bind residual"));
                        }
                        if j == 3 {
                            translation = translation.max(delta);
                        } else {
                            linear = linear.max(delta);
                        }
                    }
                }
            }
            let (linear_limit, translation_limit) = if custom.contains(name) {
                (LINEAR_LIMIT, TRANSLATION_LIMIT)
            } else {
                (1e-4, 0.001)
            };
            if linear >= linear_limit || translation >= translation_limit {
                return Err(invalid(format!(
                    "Compiled mesh inverse bind differs from model skeleton: {name} (linear residual {linear:.9}; translation residual {:.6} mm; limits {linear_limit} and {:.3} mm)",
                    translation * 25.4, translation_limit * 25.4
                )));
            }
            if row.worst_linear_bone.is_empty() || linear > row.max_linear_residual {
                row.max_linear_residual = linear;
                row.worst_linear_bone = name.into();
            }
            if row.worst_translation_bone.is_empty()
                || translation > row.max_translation_residual_inches
            {
                row.max_translation_residual_inches = translation;
                row.worst_translation_bone = name.into();
            }
        }
        meshes.push(row);
    }
    if meshes.is_empty() {
        return Err(invalid("No embedded mesh inverse-bind tables"));
    }
    Ok(InverseAudit {
        max_linear_residual: meshes
            .iter()
            .map(|m| m.max_linear_residual)
            .fold(0., f64::max),
        max_translation_residual_inches: meshes
            .iter()
            .map(|m| m.max_translation_residual_inches)
            .fold(0., f64::max),
        meshes,
        both_multiplication_orders_checked: true,
        linear_component_limit: 1e-4,
        translation_component_limit_inches: 0.001,
        source_qualified_custom_bones: if custom.is_empty() {
            None
        } else {
            Some(CustomBindLimits {
                names: custom.iter().cloned().collect(),
                linear_component_limit: LINEAR_LIMIT,
                translation_component_limit_inches: TRANSLATION_LIMIT,
            })
        },
    })
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnimationRestAudit {
    pub bone_count: usize,
    pub rest_identity: IdentityAudit,
    pub scale_preservation: ScaleAudit,
    pub max_position_component_delta_inches: f64,
    pub max_quaternion_component_delta: f64,
    pub max_local_scale_delta: f64,
}
pub fn animation_rest_bindings(
    ctrl: &Value,
    agrps: &[Value],
    blocks: &[ResourceBlock],
    skeleton: &Skeleton,
) -> Result<Vec<AnimationRestAudit>> {
    let ordinals: Vec<_> = blocks
        .iter()
        .enumerate()
        .filter(|(_, b)| b.name == "AGRP")
        .map(|(i, _)| i)
        .collect();
    if ordinals.len() != agrps.len() {
        return Err(invalid("Every animation decode group is required"));
    }
    let control = ctrl.get("embedded_animation");
    if agrps.is_empty() {
        if control.is_some_and(|v| match &v.kind {
            deadlock_resource::Kind::Null => false,
            deadlock_resource::Kind::Object(v) if v.is_empty() => false,
            _ => true,
        }) {
            return Err(invalid("Animation control points to missing decode group"));
        }
        return Ok(vec![]);
    }
    if agrps.len() != 1 {
        return Err(invalid("Unsupported embedded animation group bindings"));
    }
    let control = control.ok_or_else(|| invalid("Missing embedded animation control"))?;
    for (key, name) in [
        ("group_data_block", "AGRP"),
        ("anim_data_block", "ANIM"),
        ("seqgroup_data_block", "ASEQ"),
    ] {
        let target = integer(field(control, key)?)?;
        let found: Vec<_> = blocks
            .iter()
            .enumerate()
            .filter(|(_, b)| b.name == name)
            .map(|(i, _)| i as i64)
            .collect();
        if found != [target] {
            return Err(invalid(format!(
                "Missing/duplicate/mismatched {name} control"
            )));
        }
    }
    let lookup = skeleton.indices();
    let mut results = vec![];
    for group in agrps {
        animation_bindings(group, skeleton)?;
        let bones = array(field(field(group, "m_decodeKey")?, "m_boneArray")?)?;
        let mut reference = Skeleton {
            names: vec![],
            parents: vec![],
            positions: vec![],
            rotations: vec![],
            scales: vec![],
            flags: vec![],
        };
        let mut observed = reference.clone();
        for bone in bones {
            let name = string(field(bone, "m_name")?)?;
            let at = lookup[name];
            let parent = i32::try_from(integer(field(bone, "m_parent")?)?)
                .map_err(|_| invalid("Animation parent outside i32"))?;
            for s in [&mut reference, &mut observed] {
                s.names.push(name.into());
                s.parents.push(parent);
                s.flags.push(skeleton.flags[at]);
            }
            reference.positions.push(skeleton.positions[at]);
            reference.rotations.push(skeleton.rotations[at]);
            reference.scales.push(skeleton.scales[at]);
            observed.positions.push(vector(field(bone, "m_pos")?)?);
            observed.rotations.push(vector(field(bone, "m_quat")?)?);
            observed.scales.push(number(field(bone, "m_scale")?)?);
        }
        let identity = check_transfer(&observed, &reference)?;
        let scale = audit_scale_preservation(&observed, &reference, false)?;
        results.push(AnimationRestAudit {
            bone_count: bones.len(),
            rest_identity: identity,
            scale_preservation: scale,
            max_position_component_delta_inches: observed
                .positions
                .iter()
                .zip(&reference.positions)
                .flat_map(|(a, b)| a.iter().zip(b).map(|(a, b)| (a - b).abs()))
                .fold(0., f64::max),
            max_quaternion_component_delta: observed
                .rotations
                .iter()
                .zip(&reference.rotations)
                .map(|(a, b)| quaternion_component_delta(a, b))
                .fold(0., f64::max),
            max_local_scale_delta: observed
                .scales
                .iter()
                .zip(&reference.scales)
                .map(|(a, b)| (a - b).abs())
                .fold(0., f64::max),
        });
    }
    Ok(results)
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BindingAudit {
    pub passed: bool,
    pub model_bone_count: usize,
    pub mesh_binding_audit: MeshAudit,
    pub inverse_bind_audit: InverseAudit,
    pub animation_rest_audit: Vec<AnimationRestAudit>,
    pub compiler_arrays_modified: bool,
    pub vertex_blendindices_decoded: bool,
    pub live_ag2_deformation_validated: bool,
}
pub fn audit_bindings(
    data: &Value,
    ctrl: &Value,
    mdats: &[Value],
    agrps: &[Value],
    blocks: &[ResourceBlock],
) -> Result<BindingAudit> {
    audit_bindings_with_custom_bones(data, ctrl, mdats, agrps, blocks, &BTreeSet::new())
}
/// Callers must first prove these ordinary additions against the selected
/// source's names, hierarchy, local transforms and scale. Original and tested
/// solver bones must never be included in `custom`.
pub fn audit_bindings_with_custom_bones(
    data: &Value,
    ctrl: &Value,
    mdats: &[Value],
    agrps: &[Value],
    blocks: &[ResourceBlock],
    custom: &BTreeSet<String>,
) -> Result<BindingAudit> {
    audit_bindings_source_policy(data,ctrl,mdats,agrps,blocks,custom,false)
}
/// For freshly generated motion only: callers first verify the complete source
/// binding and selected joint-fit coverage. Uses the same numerical budgets;
/// original and imported tested-solver checks are unchanged.
pub fn audit_bindings_with_generated_bones(
    data:&Value,ctrl:&Value,mdats:&[Value],agrps:&[Value],blocks:&[ResourceBlock],custom:&BTreeSet<String>,
)->Result<BindingAudit>{audit_bindings_source_policy(data,ctrl,mdats,agrps,blocks,custom,true)}
fn audit_bindings_source_policy(
    data:&Value,ctrl:&Value,mdats:&[Value],agrps:&[Value],blocks:&[ResourceBlock],custom:&BTreeSet<String>,generated:bool,
)->Result<BindingAudit>{
    let permitted=crate::extensions::ORDINARY_FLAGS | if generated {deadlock_resource::cloth::MOTION_MASK as u32}else{0};
    for name in ["DATA", "CTRL"] {
        if blocks.iter().filter(|b| b.name == name).count() != 1 {
            return Err(invalid("Exactly one DATA and CTRL block is required"));
        }
    }
    let skeleton = Skeleton::from_value(field(data, "m_modelSkeleton")?)?;
    if custom.iter().any(|name| !skeleton.names.contains(name)) {
        return Err(invalid(
            "Custom inverse-bind authority contains absent bones",
        ));
    }
    for (i, name) in skeleton.names.iter().enumerate() {
        if custom.contains(name)
            && skeleton
                .flags
                .get(i)
                .is_none_or(|f| f & !permitted != 0)
        {
            return Err(invalid(
                "Custom inverse-bind budget requires ordinary source bones",
            ));
        }
    }
    check_transfer(&skeleton, &skeleton)?;
    bind_state(&skeleton)?;
    let ordinals: BTreeSet<_> = blocks
        .iter()
        .enumerate()
        .filter(|(_, b)| b.name == "MDAT")
        .map(|(i, _)| i)
        .collect();
    let entries = array(field(ctrl, "embedded_meshes")?)?;
    let starts = ints(field(data, "m_remappingTableStarts")?)?;
    if ordinals.is_empty()
        || ordinals.len() != mdats.len()
        || entries.len() != mdats.len()
        || starts.len() != mdats.len()
    {
        return Err(invalid("Every embedded mesh and remap slice is required"));
    }
    let bound_blocks = entries
        .iter()
        .map(|v| ordinal(v, "m_nDataBlock", "data_block"))
        .collect::<Result<Vec<_>>>()?;
    let indices = entries
        .iter()
        .map(|v| ordinal(v, "m_nMeshIndex", "mesh_index"))
        .collect::<Result<Vec<_>>>()?;
    if bound_blocks.iter().copied().collect::<BTreeSet<_>>() != ordinals
        || indices.iter().copied().collect::<BTreeSet<_>>() != (0..starts.len()).collect()
    {
        return Err(invalid(
            "CTRL requires unique complete embedded mesh bindings",
        ));
    }
    let names: BTreeSet<_> = skeleton.names.iter().map(String::as_str).collect();
    for mesh in mdats {
        let bones = array(field(field(mesh, "m_skeleton")?, "m_bones")?)?;
        let local = bones
            .iter()
            .map(|b| string(field(b, "m_boneName")?))
            .collect::<Result<Vec<_>>>()?;
        if local.is_empty()
            || local.iter().copied().collect::<BTreeSet<_>>().len() != local.len()
            || local
                .iter()
                .any(|name| name.is_empty() || !names.contains(name))
        {
            return Err(invalid(
                "Compiled mesh missing/duplicate/unknown bone identities",
            ));
        }
    }
    Ok(BindingAudit {
        passed: true,
        model_bone_count: skeleton.names.len(),
        mesh_binding_audit: audit_meshes(data, ctrl, mdats, blocks, &skeleton)?,
        inverse_bind_audit: inverse_binds_with_custom_bones(mdats, &skeleton, custom)?,
        animation_rest_audit: animation_rest_bindings(ctrl, agrps, blocks, &skeleton)?,
        compiler_arrays_modified: false,
        vertex_blendindices_decoded: false,
        live_ag2_deformation_validated: false,
    })
}
