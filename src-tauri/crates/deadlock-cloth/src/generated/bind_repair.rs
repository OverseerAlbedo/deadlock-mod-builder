//! Reconcile a drifted generated DATA rest table with independently compiled
//! animation rest values. Source binds and unchanged mesh inverse binds must
//! both corroborate the replacement. No validation tolerances are widened.
use super::*;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Proof {
    pub policy: String,
    pub native_sha256: String,
    pub reconciled_sha256: String,
    pub corrected_bones: Vec<String>,
    pub max_position_component_correction_inches: f64,
    pub max_scale_correction: f64,
    pub source_and_animation_agree: bool,
    pub unchanged_mesh_bindings_checked: bool,
    pub solver_rest_bones_checked: usize,
    pub max_solver_rest_position_error_mm: f64,
    pub max_solver_rest_rotation_residual: f64,
    pub only_custom_data_rest_values_changed: bool,
}

/// Which custom binds are reconciled: freshly generated motion bones, whose
/// cloth solver rest pose is cross-checked too, or source bones kept next to
/// the hero's restored cloth (Preserve original physics), which are never
/// solver controls (`runtime::audit_extension` rejects that).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Bones {
    Generated,
    Kept,
}

pub(super) fn prepare(
    original: &[u8],
    native: &[u8],
    source: &SourceBinding,
) -> Result<(Vec<u8>, Proof)> {
    let decoded = Decoded::read(native)?;
    let actual = skeleton(&decoded.data.root)?;
    let original_sk = original_identity(original, &actual)?;
    let (mapped, _) = mapped_source(&actual, &original_sk, source)?;
    let (mapped, _) = rotation_import::reference(&actual, &original_sk, &mapped, &decoded.animations)?;
    reconcile(original, native, decoded, &actual, &original_sk, &mapped, Bones::Generated)
}
/// Preserve original physics with kept source bones: the compiler stores
/// their model-skeleton locals with rounding that grows down long chains
/// (scale 1.000126 eight joints down a hair chain, measured on SiViolet),
/// while its animation rest table keeps the source values. The same
/// reconciliation as for generated bones, without the solver cross-check.
pub(crate) fn prepare_kept(
    original: &[u8],
    native: &[u8],
    source: &SourceBinding,
) -> Result<(Vec<u8>, Proof)> {
    let decoded = Decoded::read(native)?;
    let actual = skeleton(&decoded.data.root)?;
    let original_sk = original_identity(original, &actual)?;
    // The compiler's spelling of source names (aliases are repaired afterwards).
    let (mapped, _) = mapped_source(&actual, &original_sk, source)?;
    reconcile(original, native, decoded, &actual, &original_sk, &mapped, Bones::Kept)
}
/// The original skeleton, after checking the compiled original bones keep it exactly.
fn original_identity(original: &[u8], actual: &Skeleton) -> Result<Skeleton> {
    let layout = container::layout(original)?;
    let original_data = decode_unique(original, &layout.blocks, "DATA")?;
    let original_sk = skeleton(&original_data.root)?;
    let projected = extensions::project_skeleton(actual, &original_sk.names)?;
    deadlock_skeleton::check_transfer(&projected, &original_sk)?;
    transforms::audit_scale_preservation(&projected, &original_sk, false)?;
    Ok(original_sk)
}
fn reconcile(
    original: &[u8],
    native: &[u8],
    mut decoded: Decoded,
    actual: &Skeleton,
    original_sk: &Skeleton,
    mapped: &SourceBinding,
    kind: Bones,
) -> Result<(Vec<u8>, Proof)> {
    if extensions::source_binding(actual, original_sk, mapped, &hash(original)).is_ok() {
        return Ok((native.to_vec(), Proof::default()));
    }

    if decoded.animations.len() != 1 {
        return Err(invalid(
            "Cannot reconcile generated binds without one complete native animation rest table.",
        ));
    }
    let group = &decoded.animations[0];
    bindings::animation_bindings(group, &actual)?;
    let bones = field(field(group, "m_decodeKey")?, "m_boneArray")?.array()?;
    let rest = bones
        .iter()
        .map(|bone| Ok((field(bone, "m_name")?.string()?.to_owned(), bone)))
        .collect::<Result<BTreeMap<_, _>>>()?;
    if rest.len() != actual.names.len() || actual.names.iter().any(|n| !rest.contains_key(n)) {
        return Err(invalid(
            "Generated bind reconciliation needs animation rest evidence for every model bone.",
        ));
    }
    let original_names = original_sk.names.iter().collect::<BTreeSet<_>>();
    let custom = mapped.extra_names.iter().collect::<BTreeSet<_>>();
    let mut proof = Proof {
        policy: match kind {
            Bones::Generated => "source-qualified-animation-rest-reconciliation-v1",
            Bones::Kept => "source-qualified-animation-rest-reconciliation-kept-v1",
        }
        .into(),
        native_sha256: hash(native),
        ..Proof::default()
    };
    // Do not replace the array, bone order, parent table, flags, or any other
    // DATA field. Each replacement is copied from the immutable AGRP snapshot.
    let table = field_mut(&mut decoded.data.root, "m_modelSkeleton")?;
    for (i, name) in actual.names.iter().enumerate() {
        if original_names.contains(name) {
            continue;
        }
        if !custom.contains(name) {
            return Err(invalid(format!(
                "Unqualified generated bind identity: {name}"
            )));
        }
        let mut changed = false;
        for (key, rest_key) in [
            ("m_bonePosParent", "m_pos"),
            ("m_boneRotParent", "m_quat"),
            ("m_boneScaleParent", "m_scale"),
        ] {
            let replacement = field(rest[name], rest_key)?;
            let Kind::Array(values) = &mut field_mut(table, key)?.kind else {
                return Err(invalid("Invalid generated bind array."));
            };
            if !values[i].canonical_eq(replacement) {
                values[i] = replacement.clone();
                changed = true;
            }
        }
        if changed {
            proof.corrected_bones.push(name.clone());
        }
    }
    let corrected = skeleton(&decoded.data.root)?;
    let binding = extensions::source_binding(&corrected, &original_sk, &mapped, &hash(original))?;
    let custom = binding.source_bones_checked.iter().cloned().collect();
    // This checks complete CTRL/mesh palettes, both inverse-bind multiplication
    // orders, animation parents/channels/rest, and inherited scale. All tables
    // except the custom DATA locals are the original compiler bytes.
    let audit = match kind {
        Bones::Generated => bindings::audit_bindings_with_generated_bones,
        Bones::Kept => bindings::audit_bindings_with_custom_bones,
    };
    audit(
        &decoded.data.root,
        &decoded.ctrl.root,
        &decoded.meshes,
        &decoded.animations,
        &decoded.layout.blocks,
        &custom,
    )?;
    // A valid mesh/animation pair is insufficient if the existing cloth solver
    // was compiled in another rest pose. Cross-check named solver controls too;
    // solver points/offsets/constraints themselves are never rewritten. Kept
    // bones are never solver controls; the hero's solver is restored unchanged.
    if kind == Bones::Generated {
        let fe = field(&decoded.physics.root, "m_pFeModel")?;
        let names = strings(field(fe, "m_CtrlName")?)?;
        let poses = field(fe, "m_InitPose")?.array()?;
        if names.len() != poses.len() || names.iter().collect::<BTreeSet<_>>().len() != names.len() {
            return Err(invalid(
                "Invalid solver rest authority for generated bind reconciliation.",
            ));
        }
        let state = transforms::bind_state(&corrected)?;
        for name in &proof.corrected_bones {
            let Some(control) = names.iter().position(|n| n == name) else {
                continue;
            };
            let bone = corrected.indices()[name.as_str()];
            let pose = fitting::vector::<8>(&poses[control])?;
            let rotation = fitting::rotation([pose[4], pose[5], pose[6], pose[7]])?;
            let mut position_error = 0_f64;
            let mut rotation_error = 0_f64;
            for row in 0..3 {
                position_error =
                    position_error.max((state.world[bone][row][3] - pose[row]).abs() * 25.4);
                for col in 0..3 {
                    rotation_error = rotation_error.max(
                        (state.world[bone][row][col] / state.accumulated[bone] - rotation[row][col])
                            .abs(),
                    );
                }
            }
            if position_error > transforms::WORLD_POSITION_TOLERANCE_MM || rotation_error > 1e-4 {
                return Err(invalid(format!("Generated solver rest pose disagrees with source-qualified animation binds: {name}")));
            }
            proof.solver_rest_bones_checked += 1;
            proof.max_solver_rest_position_error_mm =
                proof.max_solver_rest_position_error_mm.max(position_error);
            proof.max_solver_rest_rotation_residual =
                proof.max_solver_rest_rotation_residual.max(rotation_error);
        }
    }
    for (i, name) in actual.names.iter().enumerate() {
        if original_names.contains(name) {
            continue;
        }
        proof.max_scale_correction = proof
            .max_scale_correction
            .max((actual.scales[i] - corrected.scales[i]).abs());
        for k in 0..3 {
            proof.max_position_component_correction_inches = proof
                .max_position_component_correction_inches
                .max((actual.positions[i][k] - corrected.positions[i][k]).abs());
        }
    }
    let output = container::rebuild(
        native,
        &BTreeMap::from([(
            "DATA".into(),
            binary::encode(&decoded.data.root, decoded.data.format_guid)?,
        )]),
    )?;
    unchanged_blocks(native, &output, &["DATA"])?;
    let roundtrip = Decoded::read(&output)?;
    if !roundtrip.data.root.canonical_eq(&decoded.data.root) {
        return Err(invalid(
            "Reconciled generated binds failed typed round-trip verification.",
        ));
    }
    proof.reconciled_sha256 = hash(&output);
    proof.source_and_animation_agree = true;
    proof.unchanged_mesh_bindings_checked = true;
    proof.only_custom_data_rest_values_changed = true;
    Ok((output, proof))
}
