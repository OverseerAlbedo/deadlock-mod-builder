use crate::runtime::{compare, summarize};
use crate::*;
use deadlock_resource::{cloth, constraints};
use deadlock_skeleton::{
    aliases::{self, AliasRepair},
    check_transfer,
    extensions::{self, SourceBinding, SourceBindingAudit},
    transforms::{self, ScaleAudit},
    IdentityAudit,
};
use std::collections::{BTreeMap, BTreeSet};
pub const POLICY: &str = "tested-native-cloth-solver-v1";
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Qualification {
    pub policy: String,
    pub reference_sha256: String,
    pub reference_summary: RuntimeAudit,
    pub structure: cloth::StructureProof,
    pub compiled_bindings: bindings::BindingAudit,
    pub capacity: CapacityProof,
    pub constraint_feasibility: constraints::Audit,
    pub native_animation_test_claimed_by_selection: bool,
    pub in_game_motion_tested: bool,
}
pub fn qualify_tested(raw: &[u8]) -> Result<Qualification> {
    if raw.len() > 256 * 1024 * 1024 {
        return Err(invalid(
            "tested cloth reference exceeds 256 MiB resource limit",
        ));
    }
    let decoded = Decoded::read(raw)?;
    cloth::self_contained(field(&decoded.physics.root, "m_pFeModel")?)?;
    let summary = summarize(&decoded.physics.root, &decoded.data.root, hash(raw))?;
    let structure = cloth::structure(&decoded.physics.root)?;
    let compiled_bindings = decoded.bindings()?;
    let capacity = crate::capacity::audit(
        &decoded.data.root,
        &decoded.ctrl.root,
        &decoded.meshes,
        &decoded.layout.blocks,
        &decoded.physics.root,
    );
    if capacity.status != cloth::Status::Passed {
        return Err(invalid(format!(
            "tested cloth index capacity is {:?}: {:?}; {:?}",
            capacity.status, capacity.mesh_remapping.reason, capacity.scalar_simd_rods.reason
        )));
    }
    let constraint_feasibility = constraints::audit(&decoded.physics.root);
    if constraint_feasibility.status != cloth::Status::Passed {
        return Err(invalid(format!(
            "tested cloth rest constraint feasibility is {:?}: {:?}",
            constraint_feasibility.status, constraint_feasibility.reason
        )));
    }
    Ok(Qualification {
        policy: POLICY.into(),
        reference_sha256: hash(raw),
        reference_summary: summary,
        structure,
        compiled_bindings,
        capacity,
        constraint_feasibility,
        native_animation_test_claimed_by_selection: true,
        in_game_motion_tested: false,
    })
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TargetProof {
    pub original_identity: IdentityAudit,
    pub original_scale: ScaleAudit,
    pub extra_names: Vec<String>,
    pub source_binding: Option<SourceBindingAudit>,
}
fn target(
    original_raw: &[u8],
    reference_raw: &[u8],
    source: Option<&SourceBinding>,
) -> Result<TargetProof> {
    let a = container::layout(original_raw)?;
    let b = container::layout(reference_raw)?;
    let original = skeleton(&decode_unique(original_raw, &a.blocks, "DATA")?.root)?;
    let reference = skeleton(&decode_unique(reference_raw, &b.blocks, "DATA")?.root)?;
    check_transfer(&original, &original)?;
    check_transfer(&reference, &reference)?;
    let projected = extensions::project_skeleton(&reference, &original.names)?;
    let original_identity = check_transfer(&projected, &original)?;
    let original_scale = transforms::audit_scale_preservation(&projected, &original, false)?;
    let names: BTreeSet<_> = original.names.iter().collect();
    let extra: BTreeSet<_> = reference
        .names
        .iter()
        .filter(|n| !names.contains(n))
        .cloned()
        .collect();
    for name in &extra {
        let mut at = reference
            .names
            .iter()
            .position(|n| n == name)
            .ok_or_else(|| invalid("missing reference bone"))?;
        while extra.contains(&reference.names[at]) {
            let p = reference.parents[at];
            if p < 0 {
                return Err(invalid(format!(
                    "tested cloth custom bone must descend from an original game bone: {name}"
                )));
            }
            at = p as usize;
        }
    }
    let source_binding = source
        .map(|s| extensions::source_binding(&reference, &original, s, &hash(original_raw)))
        .transpose()?;
    Ok(TargetProof {
        original_identity,
        original_scale,
        extra_names: extra.into_iter().collect(),
        source_binding,
    })
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TestedSelection {
    #[serde(flatten)]
    pub qualification: Qualification,
    pub target: TargetProof,
    pub original_sha256: String,
    pub original_cloth_solver_preserved: bool,
    pub original_rigid_body_physics_preserved: bool,
}
pub fn inspect_tested(
    original: &[u8],
    reference: &[u8],
    source: Option<&SourceBinding>,
) -> Result<TestedSelection> {
    inspect(original, reference, source).map_err(|error| {
        if !hero_own_cloth(original, reference) {
            return error;
        }
        let check = match error {
            Error::Invalid(message) => message,
            other => other.to_string(),
        };
        invalid(format!("{HERO_OWN_CLOTH} Check that failed: {check}"))
    })
}
/// Said when a reference that fails qualification carries the replaced hero's
/// own game cloth: usually the game's own model file, chosen as the tested
/// setup (issue #4, Silver's `werewolf.vmdl_c`). All nine game hero models
/// checked fail the checks above (inverse binds, vertex maps, tapered
/// capsules); only the explanation is added, nothing that passes changes.
const HERO_OWN_CLOTH: &str = "The tested cloth reference is the hero's own game cloth, not a setup made for your replacement model. To keep the hero's own hair and cloth, choose Preserve original physics.";
fn hero_own_cloth(original: &[u8], reference: &[u8]) -> bool {
    let solver = |raw: &[u8]| -> Option<Value> {
        let layout = container::layout(raw).ok()?;
        let physics = decode_unique(raw, &layout.blocks, "PHYS").ok()?;
        let fe = physics.root.get("m_pFeModel")?.clone();
        (!matches!(fe.kind, deadlock_resource::Kind::Null)).then_some(fe)
    };
    original == reference
        || matches!((solver(original), solver(reference)), (Some(a), Some(b)) if a.canonical_eq(&b))
}
fn inspect(
    original: &[u8],
    reference: &[u8],
    source: Option<&SourceBinding>,
) -> Result<TestedSelection> {
    let qualification = qualify_tested(reference)?;
    let target = target(original, reference, source)?;
    if !target.extra_names.is_empty() && source.is_none() {
        return Err(invalid("tested cloth uses custom bones absent from this replacement; use the matching rigged source"));
    }
    Ok(TestedSelection {
        qualification,
        target,
        original_sha256: hash(original),
        original_cloth_solver_preserved: false,
        original_rigid_body_physics_preserved: true,
    })
}
fn verify_expected(original: &[u8], reference: &[u8], expected: &TestedSelection) -> Result<()> {
    if expected.qualification.policy != POLICY
        || expected.qualification.reference_sha256 != hash(reference)
    {
        return Err(invalid(
            "tested cloth reference changed after analysis/build; analyze and rebuild",
        ));
    }
    if expected.original_sha256 != hash(original) {
        return Err(invalid(
            "original game model changed after analysis/build; analyze and rebuild",
        ));
    }
    Ok(())
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TestedAudit {
    pub policy: String,
    #[serde(flatten)]
    pub motion: MotionValidation,
    pub tested_physics: TestedSelection,
    pub scale_preservation: ScaleAudit,
    pub compiled_bindings: bindings::BindingAudit,
    pub original_cloth_solver_preserved: bool,
    pub original_rigid_body_physics_preserved: bool,
    pub all_tested_solver_fields_preserved: bool,
}
pub fn validate_tested(
    original: &[u8],
    reference: &[u8],
    candidate: &[u8],
    expected: &TestedSelection,
    source: Option<&SourceBinding>,
) -> Result<TestedAudit> {
    verify_expected(original, reference, expected)?;
    let proof = inspect_tested(original, reference, source)?;
    let actual = Decoded::read(candidate)?;
    let actual_summary = summarize(&actual.physics.root, &actual.data.root, hash(candidate))?;
    let result = compare(
        proof.qualification.reference_summary.clone(),
        actual_summary,
        None,
        None,
    );
    if !result.passed {
        return Err(invalid(format!(
            "tested cloth solver/bindings changed: {}",
            result.errors.join("; ")
        )));
    }
    let actual_sk = &result.rebuilt.skeleton;
    let reference_sk = &result.original.skeleton;
    check_transfer(actual_sk, reference_sk)?;
    let scale_preservation = transforms::audit_scale_preservation(actual_sk, reference_sk, false)?;
    let a = container::layout(original)?;
    let b = container::layout(reference)?;
    let original_physics = decode_unique(original, &a.blocks, "PHYS")?;
    let reference_physics = decode_unique(reference, &b.blocks, "PHYS")?;
    require_body_bones(&original_physics.root, actual_sk)?;
    if !field(&actual.physics.root, "m_pFeModel")?
        .canonical_eq(field(&reference_physics.root, "m_pFeModel")?)
    {
        return Err(invalid(
            "tested cloth FE payload changed (all typed fields compared)",
        ));
    }
    if !cloth::same_body_physics(&actual.physics.root, &original_physics.root)? {
        return Err(invalid(
            "original rigid-body physics changed around tested cloth solver",
        ));
    }
    let compiled_bindings = actual.bindings()?;
    cloth::structure(&actual.physics.root)?;
    Ok(TestedAudit {
        policy: POLICY.into(),
        motion: result,
        tested_physics: proof,
        scale_preservation,
        compiled_bindings,
        original_cloth_solver_preserved: false,
        original_rigid_body_physics_preserved: true,
        all_tested_solver_fields_preserved: true,
    })
}
pub(crate) fn require_body_bones(physics: &Value, skeleton: &Skeleton) -> Result<()> {
    if let Some(names) = physics.get("m_boneNames") {
        for name in strings(names)? {
            if !skeleton.names.contains(&name) {
                return Err(invalid(format!(
                    "original physics references absent replacement bone: {name}"
                )));
            }
        }
    }
    Ok(())
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TransferReceipt {
    pub policy: String,
    pub status: String,
    pub input_sha256: String,
    pub output_sha256: String,
    pub reference_sha256: String,
    pub original_sha256: String,
    pub bone_name_restoration: Option<AliasRepair>,
    pub audit: TestedAudit,
    pub original_cloth_solver_preserved: bool,
    pub original_rigid_body_physics_preserved: bool,
    pub mesh_weights_and_animation_samples_preserved: bool,
    pub in_game_motion_tested: bool,
    pub independent_reader_validation_required: bool,
}
pub fn apply_tested(
    original: &[u8],
    reference: &[u8],
    candidate: &[u8],
    expected: &TestedSelection,
    source: Option<&SourceBinding>,
) -> Result<(Vec<u8>, TransferReceipt)> {
    verify_expected(original, reference, expected)?;
    let proof = inspect_tested(original, reference, source)?;
    let (canonical, aliases) = aliases::repair_aliases(reference, candidate, false)?;
    let mut decoded = Decoded::read(&canonical)?;
    let ref_decoded = Decoded::read(reference)?;
    let original_layout = container::layout(original)?;
    let original_physics = decode_unique(original, &original_layout.blocks, "PHYS")?;
    let reference_sk = skeleton(&ref_decoded.data.root)?;
    let actual_sk = skeleton(&decoded.data.root)?;
    check_transfer(&actual_sk, &reference_sk)?;
    transforms::audit_scale_preservation(&actual_sk, &reference_sk, false)?;
    decoded.bindings()?;
    let before = summarize(&decoded.physics.root, &decoded.data.root, hash(&canonical))?;
    let original_summary = audit_resource(original)?;
    if before.solver_sha256.is_some()
        && before.solver_sha256 != proof.qualification.reference_summary.solver_sha256
        && before.solver_sha256 != original_summary.solver_sha256
    {
        return Err(invalid("compiler produced a different authored solver; refusing to overwrite unreviewed physics"));
    }
    let mut physics = original_physics.root.clone();
    *field_mut(&mut physics, "m_pFeModel")? =
        field(&ref_decoded.physics.root, "m_pFeModel")?.clone();
    super::restore::replace_motion_flags(&mut decoded.data.root, &reference_sk, false)?;
    if original_physics.format_guid != ref_decoded.physics.format_guid {
        return Err(invalid(
            "original and tested PHYS KV3 formats differ; native qualification required",
        ));
    }
    let replacements = BTreeMap::from([
        (
            "DATA".to_owned(),
            binary::encode(&decoded.data.root, decoded.data.format_guid)?,
        ),
        (
            "PHYS".to_owned(),
            binary::encode(&physics, original_physics.format_guid)?,
        ),
    ]);
    let output = container::rebuild(&canonical, &replacements)?;
    let audit = validate_tested(original, reference, &output, expected, source)?;
    unchanged_blocks(&canonical, &output, &["DATA", "PHYS"])?;
    let receipt = TransferReceipt {
        policy: POLICY.into(),
        status: "tested_solver_imported".into(),
        input_sha256: hash(candidate),
        output_sha256: hash(&output),
        reference_sha256: proof.qualification.reference_sha256,
        original_sha256: hash(original),
        bone_name_restoration: aliases,
        audit,
        original_cloth_solver_preserved: false,
        original_rigid_body_physics_preserved: true,
        mesh_weights_and_animation_samples_preserved: true,
        in_game_motion_tested: false,
        independent_reader_validation_required: true,
    };
    Ok((output, receipt))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)).unwrap()
    }
    #[test]
    fn a_failing_reference_with_the_hero_own_cloth_says_so() {
        // Issue #4: the game's own werewolf.vmdl_c chosen as the tested setup.
        let (original, reference) = (fixture("tested-original.vmdl_c"), fixture("tested-reference.vmdl_c"));
        assert!(hero_own_cloth(&original, &original));
        assert!(!hero_own_cloth(&original, &reference));
        // A real tested setup that fails keeps its own message.
        let other = inspect_tested(&original, &reference, None).unwrap_err().to_string();
        assert!(!other.contains(HERO_OWN_CLOTH), "{other}");
        match inspect_tested(&original, &original, None) {
            Err(error) => assert!(error.to_string().contains(HERO_OWN_CLOTH), "{error}"),
            Ok(_) => panic!("the fixture's own cloth qualified as a tested setup"),
        }
    }
}
