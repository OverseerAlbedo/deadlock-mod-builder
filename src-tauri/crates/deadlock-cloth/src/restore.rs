use crate::runtime::{audit_extension, needs_extension};
use crate::*;
use deadlock_resource::{cloth, Kind};
use deadlock_skeleton::{
    aliases::{self, AliasRepair},
    check_transfer,
    extensions::SourceBinding,
    IdentityAudit,
};
use std::collections::BTreeMap;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FlagChange {
    pub name: String,
    pub old_flags: u32,
    pub new_flags: u32,
}
pub(crate) fn replace_motion_flags(
    data: &mut Value,
    reference: &Skeleton,
    allow_extra: bool,
) -> Result<Vec<FlagChange>> {
    let names = strings(field(field(data, "m_modelSkeleton")?, "m_boneName")?)?;
    let reference: BTreeMap<_, _> = reference.names.iter().zip(&reference.flags).collect();
    let flags = field_mut(field_mut(data, "m_modelSkeleton")?, "m_nFlag")?;
    let Kind::Array(flags) = &mut flags.kind else {
        return Err(invalid("incomplete motion flag array"));
    };
    if flags.len() != names.len() {
        return Err(invalid("incomplete motion flag array"));
    }
    let mut changed = vec![];
    for (name, flag) in names.iter().zip(flags) {
        let Some(expected) = reference.get(name) else {
            if allow_extra {
                continue;
            }
            return Err(invalid("replacement has unqualified extra bone"));
        };
        let old = u32::try_from(flag.integer()?).map_err(|_| invalid("invalid bone flags"))?;
        let new = (old & !(cloth::MOTION_MASK as u32)) | (**expected & cloth::MOTION_MASK as u32);
        if old != new {
            changed.push(FlagChange {
                name: name.clone(),
                old_flags: old,
                new_flags: new,
            });
        }
        flag.kind = Kind::I64(new as i64);
    }
    Ok(changed)
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RestorationReceipt {
    pub status: String,
    pub original_sha256: String,
    pub input_sha256: String,
    pub output_sha256: String,
    pub prior_cloth_nodes: usize,
    pub restored_cloth_nodes: usize,
    pub static_nodes: usize,
    pub ropes: usize,
    pub control_names: Vec<String>,
    pub control_bindings: Option<cloth::ControlBindings>,
    pub restored_flags_by_name: Vec<FlagChange>,
    pub skeleton_identity: Option<IdentityAudit>,
    pub skeleton_extension: Option<ExtensionProof>,
    pub bone_name_restoration: Option<AliasRepair>,
    pub unchanged_blocks: Vec<UnchangedBlock>,
    pub physics_payload_exact_original: bool,
    pub data_changed_only_cloth_procedural_flags: bool,
    pub data_changed_only_motion_flags_and_authorized_names: bool,
    pub in_game_motion_tested: bool,
    pub independent_reader_validation_required: bool,
    /// Kept source bones whose rounded model-skeleton locals were replaced by
    /// the compiler's own animation rest values before checking them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kept_bind_repair: Option<crate::generated::bind_repair::Proof>,
}
/// Preserve original physics with kept source bones (hair, tails kept by
/// preparation but not simulated): the compiler rounds their model-skeleton
/// locals more the further down a chain they are, past the source binding
/// limits, while its animation rest table keeps the source values. They get
/// the same reconciliation as generated bones (`bind_repair`), with no wider
/// tolerances. Unchanged when the bones already match or the reconciliation
/// cannot be proven; the usual check then reports as before.
fn kept_binds(
    original: &[u8],
    canonical: &[u8],
    expected: &RuntimeAudit,
    source: &SourceBinding,
) -> Result<(Vec<u8>, Option<crate::generated::bind_repair::Proof>)> {
    let actual = audit_resource(canonical)?;
    if !needs_extension(expected, &actual)
        || deadlock_skeleton::extensions::source_binding(&actual.skeleton, &expected.skeleton, source, &hash(original)).is_ok()
    {
        return Ok((canonical.to_vec(), None));
    }
    match crate::generated::bind_repair::prepare_kept(original, canonical, source) {
        Ok((output, proof)) if !proof.corrected_bones.is_empty() => {
            unchanged_blocks(canonical, &output, &["DATA"])?;
            Ok((output, Some(proof)))
        }
        _ => Ok((canonical.to_vec(), None)),
    }
}
pub fn restore(
    original: &[u8],
    candidate: &[u8],
    source: Option<&SourceBinding>,
) -> Result<(Vec<u8>, RestorationReceipt)> {
    // Resources with no cloth (including no PHYS block) need no restoration.
    // Audit external dependencies before allowing the no-op path.
    let expected = audit_resource(original)?;
    // Before the alias repair, which checks the kept bones against the source.
    let (reconciled, kept_bind_repair) = match source {
        Some(source) => kept_binds(original, candidate, &expected, source)?,
        None => (candidate.to_vec(), None),
    };
    let (canonical, names) = if expected.cloth_nodes == 0 && source.is_none() {
        (reconciled, None)
    } else {
        aliases::repair_aliases_with_source(original, &reconciled, true, source)?
    };
    let actual = audit_resource(&canonical)?;
    let mut receipt = RestorationReceipt {
        status: "not_required".into(),
        original_sha256: hash(original),
        input_sha256: hash(candidate),
        output_sha256: hash(&canonical),
        prior_cloth_nodes: actual.cloth_nodes,
        restored_cloth_nodes: 0,
        static_nodes: 0,
        ropes: 0,
        control_names: vec![],
        control_bindings: None,
        restored_flags_by_name: vec![],
        skeleton_identity: None,
        skeleton_extension: None,
        bone_name_restoration: names,
        unchanged_blocks: vec![],
        physics_payload_exact_original: false,
        data_changed_only_cloth_procedural_flags: false,
        data_changed_only_motion_flags_and_authorized_names: false,
        in_game_motion_tested: false,
        independent_reader_validation_required: true,
        kept_bind_repair,
    };
    if expected.cloth_nodes == 0 {
        return Ok((canonical, receipt));
    }
    let extended = needs_extension(&expected, &actual);
    if source.is_some() && !extended {
        return Err(invalid("compiled output lost required custom source bones"));
    }
    let identity = if extended {
        let proof = audit_extension(original, &canonical, &expected, &actual, source)?;
        let identity = proof.identity.original_identity.clone();
        receipt.skeleton_extension = Some(proof);
        identity
    } else {
        check_transfer(&actual.skeleton, &expected.skeleton)?
    };
    receipt.skeleton_identity = Some(identity);
    let original_layout = container::layout(original)?;
    let original_physics_block = container::unique(&original_layout.blocks, "PHYS")?;
    let physics = decode_unique(original, &original_layout.blocks, "PHYS")?;
    crate::tested::require_body_bones(&physics.root, &actual.skeleton)?;
    let check = validate_motion(original, &canonical, source)?;
    if check.passed {
        receipt.status = "already_preserved".into();
        receipt.skeleton_extension = check.skeleton_extension;
        if receipt.bone_name_restoration.is_some() {
            receipt.data_changed_only_motion_flags_and_authorized_names = true;
            receipt.unchanged_blocks =
                unchanged_blocks(candidate, &canonical, &["DATA", "MDAT", "AGRP"])?;
        } else if receipt.kept_bind_repair.is_some() {
            receipt.unchanged_blocks = unchanged_blocks(candidate, &canonical, &["DATA"])?;
        }
        return Ok((canonical, receipt));
    }
    if actual.cloth_nodes > 0 && actual.solver_sha256 != expected.solver_sha256 {
        return Err(invalid(
            "compiled model has different authored cloth solver; refusing automatic replacement",
        ));
    }
    let layout = container::layout(&canonical)?;
    let mut data = decode_unique(&canonical, &layout.blocks, "DATA")?;
    let before = data.root.clone();
    let before_flags = field(field(&before, "m_modelSkeleton")?, "m_nFlag")?.clone();
    receipt.restored_flags_by_name =
        replace_motion_flags(&mut data.root, &expected.skeleton, extended)?;
    let replacements = BTreeMap::from([
        (
            "DATA".to_owned(),
            binary::encode(&data.root, data.format_guid)?,
        ),
        (
            "PHYS".to_owned(),
            container::payload(original, original_physics_block)?.to_vec(),
        ),
    ]);
    let output = container::rebuild(&canonical, &replacements)?;
    let checked_layout = container::layout(&output)?;
    let mut checked = decode_unique(&output, &checked_layout.blocks, "DATA")?;
    if !checked.root.canonical_eq(&data.root) {
        return Err(invalid("restored DATA failed typed verification"));
    }
    *field_mut(field_mut(&mut checked.root, "m_modelSkeleton")?, "m_nFlag")? = before_flags;
    if !checked.root.canonical_eq(&before) {
        return Err(invalid(
            "restoration changed DATA beyond permitted motion flags",
        ));
    }
    if container::payload(&output, container::unique(&checked_layout.blocks, "PHYS")?)?
        != container::payload(original, original_physics_block)?
    {
        return Err(invalid("original PHYS payload not preserved exactly"));
    }
    receipt.unchanged_blocks = unchanged_blocks(&canonical, &output, &["DATA", "PHYS"])?;
    let result = validate_motion(original, &output, source)?;
    if !result.passed {
        return Err(invalid(format!(
            "post-restoration motion verification failed: {}",
            result.errors.join("; ")
        )));
    }
    receipt.status = "restored".into();
    receipt.output_sha256 = hash(&output);
    receipt.skeleton_extension = result.skeleton_extension;
    receipt.restored_cloth_nodes = expected.cloth_nodes;
    receipt.static_nodes = expected.static_nodes;
    receipt.ropes = expected.ropes;
    receipt.control_names = expected.controls;
    receipt.control_bindings = Some(expected.control_bindings);
    receipt.physics_payload_exact_original = true;
    receipt.data_changed_only_cloth_procedural_flags =
        receipt.bone_name_restoration.is_none() && receipt.kept_bind_repair.is_none();
    receipt.data_changed_only_motion_flags_and_authorized_names =
        receipt.bone_name_restoration.is_some();
    if receipt.bone_name_restoration.is_some() {
        receipt.unchanged_blocks =
            unchanged_blocks(candidate, &output, &["DATA", "PHYS", "MDAT", "AGRP"])?;
    }
    Ok((output, receipt))
}
