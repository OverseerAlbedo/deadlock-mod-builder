use crate::*;
use deadlock_resource::{cloth, Kind};
use deadlock_skeleton::{
    extensions::{self, ExtensionAudit, SourceBinding, SourceBindingAudit},
    IdentityAudit,
};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RuntimeAudit {
    pub status: String,
    pub resource_sha256: String,
    pub cloth_nodes: usize,
    pub static_nodes: usize,
    pub ropes: usize,
    pub solver_sha256: Option<String>,
    pub solver_fingerprint_algorithm: String,
    pub controls: Vec<String>,
    pub control_bindings: cloth::ControlBindings,
    pub skeleton: Skeleton,
    pub bone_motion_flags: BTreeMap<String, u32>,
    pub in_game_motion_tested: bool,
}
pub(crate) fn summarize(
    physics: &Value,
    data: &Value,
    resource_sha256: String,
) -> Result<RuntimeAudit> {
    physics.object()?;
    let skeleton = skeleton(data)?;
    skeleton.validate(true)?;
    deadlock_skeleton::skeleton_identity(&skeleton, &skeleton)?;
    let mut count = 0;
    let mut static_ = 0;
    let mut ropes = 0;
    let mut controls = vec![];
    let mut solver_sha256 = None;
    let empty = Value::new(Kind::Object(vec![]));
    let solver = physics
        .get("m_pFeModel")
        .filter(|v| !matches!(v.kind, Kind::Null));
    if let Some(solver) = solver {
        solver.object()?;
        let n = field(solver, "m_nNodeCount")?.integer()?;
        let s = field(solver, "m_nStaticNodes")?.integer()?;
        let r = field(solver, "m_nRopeCount")?.integer()?;
        controls = strings(field(solver, "m_CtrlName")?)?;
        if n < 0
            || s < 0
            || s > n
            || r < 0
            || controls.iter().any(|v| v.is_empty())
            || controls.iter().collect::<BTreeSet<_>>().len() != controls.len()
            || (n > 0 && controls.is_empty())
        {
            return Err(invalid("invalid cloth solver counts/controls"));
        }
        count = usize::try_from(n).map_err(|_| invalid("cloth node count overflow"))?;
        static_ = usize::try_from(s).map_err(|_| invalid("static node count overflow"))?;
        ropes = usize::try_from(r).map_err(|_| invalid("rope count overflow"))?;
        if let Some(hashes) = solver.get("m_CtrlHash") {
            let hashes = hashes.array()?;
            if hashes.len() != controls.len()
                || hashes.iter().any(|v| {
                    v.integer()
                        .map_or(true, |n| !(0..=u32::MAX as i128).contains(&n))
                })
            {
                return Err(invalid("invalid or incomplete control hashes"));
            }
        }
        // Deterministic typed hashing retains order, annotations, unknown fields,
        // float64 bits and exact float32 expansion; no decimal or JSON coercion.
        solver_sha256 = Some(deadlock_resource::value::canonical_sha256(solver)?);
    }
    let control_bindings =
        cloth::control_bindings(&controls, &skeleton.names, solver.unwrap_or(&empty))?;
    let bone_motion_flags = skeleton
        .names
        .iter()
        .cloned()
        .zip(skeleton.flags.iter().map(|f| f & cloth::MOTION_MASK as u32))
        .collect();
    Ok(RuntimeAudit {
        status: "audited".into(),
        resource_sha256,
        cloth_nodes: count,
        static_nodes: static_,
        ropes,
        solver_sha256,
        solver_fingerprint_algorithm: "canonical-kv3-v1-sha256".into(),
        controls,
        control_bindings,
        skeleton,
        bone_motion_flags,
        in_game_motion_tested: false,
    })
}
pub fn audit_resource(raw: &[u8]) -> Result<RuntimeAudit> {
    let layout = container::layout(raw)?;
    reject_external_physics(raw)?;
    let data = decode_unique(raw, &layout.blocks, "DATA")?;
    let physics = if layout.blocks.iter().any(|b| b.name == "PHYS") {
        decode_unique(raw, &layout.blocks, "PHYS")?.root
    } else {
        Value::new(Kind::Object(vec![]))
    };
    summarize(&physics, &data.root, hash(raw))
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExtensionProof {
    #[serde(flatten)]
    pub identity: ExtensionAudit,
    pub compiled_bindings: bindings::BindingAudit,
    pub original_sha256: String,
    pub candidate_sha256: String,
    pub source_binding: Option<SourceBindingAudit>,
    pub source_binding_checked: bool,
}
pub(crate) fn needs_extension(original: &RuntimeAudit, candidate: &RuntimeAudit) -> bool {
    let a: BTreeSet<_> = original.skeleton.names.iter().collect();
    let b: BTreeSet<_> = candidate.skeleton.names.iter().collect();
    a.len() < b.len() && a.is_subset(&b)
}
pub(crate) fn audit_extension(
    original_raw: &[u8],
    candidate_raw: &[u8],
    original: &RuntimeAudit,
    candidate: &RuntimeAudit,
    source: Option<&SourceBinding>,
) -> Result<ExtensionProof> {
    let identity = extensions::extension_identity(&candidate.skeleton, &original.skeleton)?;
    let layout = container::layout(original_raw)?;
    // Ordinary additions need the same compiled/source proof even when the hero
    // has no FE solver. Absence of PHYS is valid, not a reason to fabricate one.
    if original.cloth_nodes == 0 && source.is_none() {
        return Err(invalid(
            "custom bones on a model without cloth require source binding authority",
        ));
    }
    let mut controls: BTreeSet<String> = original.controls.iter().cloned().collect();
    if layout.blocks.iter().any(|b| b.name == "PHYS") {
        let physics = decode_unique(original_raw, &layout.blocks, "PHYS")?;
        if let Some(names) = physics.root.get("m_boneNames") {
            controls.extend(strings(names)?);
        }
        crate::tested::require_body_bones(&physics.root, &candidate.skeleton)?;
    }
    if identity.extra_names.iter().any(|n| controls.contains(n)) {
        return Err(invalid(
            "custom bones collide with original physics controls",
        ));
    }
    let candidate_layout = container::layout(candidate_raw)?;
    let original_sha256 = hash(original_raw);
    let candidate_sha256 = hash(candidate_raw);
    let source_binding = source
        .map(|s| {
            extensions::source_binding(&candidate.skeleton, &original.skeleton, s, &original_sha256)
        })
        .transpose()?;
    let custom: BTreeSet<String> = source_binding
        .as_ref()
        .map(|p| p.source_bones_checked.iter().cloned().collect())
        .unwrap_or_default();
    let compiled_bindings = if candidate_layout.blocks.iter().any(|b| b.name == "PHYS") {
        // Retain the embedded physical block binding gate whenever PHYS exists.
        Decoded::read(candidate_raw)?.bindings_with_custom_bones(&custom)?
    } else {
        let data = decode_unique(candidate_raw, &candidate_layout.blocks, "DATA")?;
        let ctrl = decode_unique(candidate_raw, &candidate_layout.blocks, "CTRL")?;
        let read = |name| -> Result<Vec<Value>> {
            container::read_blocks(candidate_raw, name)?
                .into_iter()
                .map(|d| Ok(d.root))
                .collect()
        };
        bindings::audit_bindings_with_custom_bones(
            &data.root,
            &ctrl.root,
            &read("MDAT")?,
            &read("AGRP")?,
            &candidate_layout.blocks,
            &custom,
        )?
    };
    Ok(ExtensionProof {
        identity,
        compiled_bindings,
        original_sha256,
        candidate_sha256,
        source_binding,
        source_binding_checked: source.is_some(),
    })
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MotionValidation {
    pub audit_version: u32,
    pub original: RuntimeAudit,
    pub rebuilt: RuntimeAudit,
    pub passed: bool,
    pub required: bool,
    pub errors: Vec<String>,
    pub skeleton_identity: Option<IdentityAudit>,
    pub changed_motion_flag_bones: Vec<String>,
    pub skeleton_extension: Option<ExtensionProof>,
    pub in_game_motion_tested: bool,
    pub compiled_resources_modified: bool,
}
pub(crate) fn compare(
    original: RuntimeAudit,
    rebuilt: RuntimeAudit,
    projected: Option<&Skeleton>,
    extension: Option<ExtensionProof>,
) -> MotionValidation {
    let required = original.cloth_nodes > 0;
    let mut errors = vec![];
    let mut identity = None;
    let mut changed = vec![];
    if required {
        if original.cloth_nodes != rebuilt.cloth_nodes {
            errors.push("cloth node count changed".into());
        }
        if original.static_nodes != rebuilt.static_nodes {
            errors.push("static node count changed".into());
        }
        if original.ropes != rebuilt.ropes {
            errors.push("rope count changed".into());
        }
        if original.solver_sha256 != rebuilt.solver_sha256 {
            errors.push(
                "original cloth solver payload missing or changed (all typed fields compared)"
                    .into(),
            );
        }
        if original.control_bindings != rebuilt.control_bindings {
            errors.push("original skeleton/derived-control bindings changed".into());
        }
        match deadlock_skeleton::skeleton_identity(
            projected.unwrap_or(&rebuilt.skeleton),
            &original.skeleton,
        ) {
            Ok(proof) => identity = Some(proof),
            Err(error) => errors.push(error.to_string()),
        }
        changed = original
            .bone_motion_flags
            .iter()
            .filter(|(n, f)| rebuilt.bone_motion_flags.get(*n) != Some(f))
            .map(|(n, _)| n.clone())
            .collect();
        if !changed.is_empty() {
            errors.push("cloth/procedural bone flags differ by bone name".into());
        }
    }
    MotionValidation {
        audit_version: 2,
        original,
        rebuilt,
        passed: errors.is_empty(),
        required,
        errors,
        skeleton_identity: identity,
        changed_motion_flag_bones: changed,
        skeleton_extension: extension,
        in_game_motion_tested: false,
        compiled_resources_modified: false,
    }
}
pub fn validate_motion(
    original_raw: &[u8],
    candidate_raw: &[u8],
    source: Option<&SourceBinding>,
) -> Result<MotionValidation> {
    let original = audit_resource(original_raw)?;
    let candidate = audit_resource(candidate_raw)?;
    if needs_extension(&original, &candidate) {
        let proof = audit_extension(original_raw, candidate_raw, &original, &candidate, source)?;
        let projected =
            extensions::project_skeleton(&candidate.skeleton, &original.skeleton.names)?;
        Ok(compare(original, candidate, Some(&projected), Some(proof)))
    } else {
        if source.is_some() {
            return Err(invalid("compiled output lost required custom source bones"));
        }
        Ok(compare(original, candidate, None, None))
    }
}

/// Fresh compiled binding proof for ordinary skeleton additions, including
/// originals without cloth. Model validation uses this independently of whether
/// preserving an original FE solver is required.
pub fn audit_skeleton_extension(
    original_raw: &[u8],
    candidate_raw: &[u8],
    source: Option<&SourceBinding>,
) -> Result<ExtensionProof> {
    let original = audit_resource(original_raw)?;
    let candidate = audit_resource(candidate_raw)?;
    audit_extension(original_raw, candidate_raw, &original, &candidate, source)
}
