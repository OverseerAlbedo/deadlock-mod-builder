//! Qualify the compiler's near-pole Euler conversion for added source bones.
//! This is an explicit source-derived import transform, not a larger error
//! budget. Original bones and all native model/solver bytes remain untouched.
use super::*;

pub const POLICY: &str = "source-qualified-singular-rotation-import-v1";
const COMPONENT_LIMIT: f64 = 0.0001; // Same strict transfer component budget.

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BoneProof {
    pub name: String,
    pub source_rotation: [f64; 4],
    pub imported_rotation: [f64; 4],
    pub horizontal_axis_length: f64,
    pub native_component_residual: f64,
    pub animation_component_residual: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Proof {
    pub policy: String,
    pub bones: Vec<BoneProof>,
    pub raw_source_identity: Option<deadlock_skeleton::IdentityAudit>,
    pub raw_source_scale: Option<transforms::ScaleAudit>,
    pub solver_rest_bones_checked: usize,
    pub max_solver_rest_rotation_residual: f64,
    pub max_solver_rest_position_error_mm: f64,
    pub native_values_modified: bool,
}

fn delta(a: &[f64; 4], b: &[f64; 4]) -> f64 {
    let minus = a.iter().zip(b).map(|(a,b)|(a-b).abs()).fold(0., f64::max);
    let plus = a.iter().zip(b).map(|(a,b)|(a+b).abs()).fold(0., f64::max);
    minus.min(plus)
}

/// Infer only the documented MatrixAngles singular branch (xy <= 0.001).
/// Forward/left columns determine pitch/yaw; roll becomes zero. The supplied
/// native capture corroborates this conversion, independently of bone names.
/// Formula reference: ValveSoftware/source-sdk-2013, mathlib_base.cpp,
/// MatrixAngles. This is not a claim that Source 2's compiler source is public.
fn imported_rotation(q: [f64; 4]) -> Result<Option<([f64; 4], f64)>> {
    let matrix = fitting::rotation(q)?;
    let xy = matrix[0][0].hypot(matrix[1][0]);
    if xy > 0.001 {
        return Ok(None);
    }
    let pitch = (-matrix[2][0]).atan2(xy);
    let yaw = (-matrix[0][1]).atan2(matrix[1][1]);
    let (sp, cp) = (pitch * 0.5).sin_cos();
    let (sy, cy) = (yaw * 0.5).sin_cos();
    Ok(Some(([-sp * sy, sp * cy, cp * sy, cp * cy], xy)))
}

pub(super) fn reference(
    actual: &Skeleton,
    original: &Skeleton,
    source: &SourceBinding,
    animations: &[Value],
) -> Result<(SourceBinding, Proof)> {
    let mut mapped = source.clone();
    let mut proof = Proof::default();
    let lookup = actual.indices();
    let source_lookup = source.skeleton.indices();
    for name in &source.extra_names {
        if original.names.contains(name) {
            return Err(invalid("Rotation import cannot authorize an original hero bone."));
        }
        let Some(&at) = lookup.get(name.as_str()) else { continue; };
        let &from = source_lookup.get(name.as_str())
            .ok_or_else(|| invalid("Rotation import source bone is missing."))?;
        let raw = source.skeleton.rotations[from];
        if delta(&raw, &actual.rotations[at]) < COMPONENT_LIMIT {
            continue;
        }
        let Some((predicted, xy)) = imported_rotation(raw)? else { continue; };
        let residual = delta(&predicted, &actual.rotations[at]);
        if animations.len() != 1 {
            return Err(invalid("Rotation import needs one complete native animation rest table."));
        }
        bindings::animation_bindings(&animations[0], actual)?;
        let bones = field(field(&animations[0], "m_decodeKey")?, "m_boneArray")?.array()?;
        let matches: Vec<_> = bones.iter().filter(|b| b.get("m_name")
            .and_then(|n| n.string().ok()) == Some(name.as_str())).collect();
        if matches.len() != 1 {
            return Err(invalid("Rotation import needs unique animation rest evidence."));
        }
        let animation = fitting::vector::<4>(field(matches[0], "m_quat")?)?;
        fitting::rotation(animation)?;
        // If AGRP still agrees with the unconverted source, let the existing
        // DATA-only reconciliation repair drift without changing its authority.
        if delta(&raw, &animation) < COMPONENT_LIMIT { continue; }
        let animation_residual = delta(&predicted, &animation);
        if animation_residual >= COMPONENT_LIMIT {
            return Err(invalid(format!("Animation rest rotation disagrees with source-derived compiler import: {name}")));
        }
        mapped.skeleton.rotations[from] = predicted;
        // DATA must subsequently pass the unchanged transfer check against
        // this reference, or be reconciled from AGRP by the existing path.
        proof.bones.push(BoneProof {
            name: name.clone(), source_rotation: raw, imported_rotation: predicted,
            horizontal_axis_length: xy, native_component_residual: residual,
            animation_component_residual: animation_residual,
        });
    }
    if !proof.bones.is_empty() { proof.policy = POLICY.into(); }
    Ok((mapped, proof))
}

/// Called only after full source, animation, mesh inverse-bind and fit checks.
/// Preserve the existing raw-source angular, scale and world-joint budgets as
/// well, so an import conversion cannot accumulate unchecked chain displacement.
pub(super) fn audit(
    actual: &Skeleton, original: &Skeleton, raw_source: &SourceBinding,
    physics: &Value, proof: &mut Proof,
) -> Result<()> {
    if proof.bones.is_empty() { return Ok(()); }
    let mut raw = extensions::project_skeleton(&raw_source.skeleton, &actual.names)?;
    let originals = original.indices();
    for (i, name) in raw.names.iter().enumerate() {
        if let Some(&j) = originals.get(name.as_str()) {
            raw.positions[i] = original.positions[j];
            raw.rotations[i] = original.rotations[j];
            raw.scales[i] = original.scales[j];
        }
    }
    proof.raw_source_identity = Some(deadlock_skeleton::skeleton_identity(actual, &raw)?);
    proof.raw_source_scale = Some(transforms::audit_scale_preservation(actual, &raw, false)?);
    let state = transforms::bind_state(actual)?;
    let fe = field(physics, "m_pFeModel")?;
    let names = strings(field(fe, "m_CtrlName")?)?;
    let poses = field(fe, "m_InitPose")?.array()?;
    if names.len() != poses.len() || names.iter().collect::<BTreeSet<_>>().len() != names.len() {
        return Err(invalid("Rotation import needs complete unique solver rest evidence."));
    }
    let indices = actual.indices();
    for row in &proof.bones {
        let control = names.iter().position(|n| n == &row.name)
            .ok_or_else(|| invalid(format!("Rotation import lacks solver rest evidence: {}", row.name)))?;
        let at = indices[row.name.as_str()];
        let pose = fitting::vector::<8>(&poses[control])?;
        let rotation = fitting::rotation([pose[4], pose[5], pose[6], pose[7]])?;
        let mut position_error = 0_f64;
        let mut rotation_error = 0_f64;
        for r in 0..3 {
            position_error = position_error.max((state.world[at][r][3] - pose[r]).abs() * 25.4);
            for c in 0..3 {
                rotation_error = rotation_error.max(
                    (state.world[at][r][c] / state.accumulated[at] - rotation[r][c]).abs());
            }
        }
        if position_error > transforms::WORLD_POSITION_TOLERANCE_MM || rotation_error > 1e-4 {
            return Err(invalid(format!("Solver rest pose disagrees with source-derived compiler rotation: {}", row.name)));
        }
        proof.solver_rest_bones_checked += 1;
        proof.max_solver_rest_position_error_mm = proof.max_solver_rest_position_error_mm.max(position_error);
        proof.max_solver_rest_rotation_residual = proof.max_solver_rest_rotation_residual.max(rotation_error);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn captured_import_is_predicted_without_bone_or_model_names() {
        let source = [0.5179427862167358, -0.4815255105495453, 0.5179712772369385, 0.48122161626815796];
        let native = [0.5178450345993042, -0.4812697172164917, 0.5180689096450806, 0.4814777672290802];
        let (predicted, xy) = imported_rotation(source).unwrap().unwrap();
        assert!(delta(&source, &native) > COMPONENT_LIMIT);
        assert!(delta(&predicted, &native) < 4e-7);
        assert!(xy > 0.00043 && xy < 0.00044);
        assert!(delta(&predicted, &imported_rotation(source.map(|v| -v)).unwrap().unwrap().0) < 1e-12);
    }
    #[test]
    fn ordinary_rotations_never_acquire_an_import_exception() {
        for q in [[0., 0., 0., 1.], [0.5; 4], [0., 0.6, 0., 0.8]] {
            assert!(imported_rotation(q).unwrap().is_none());
        }
    }
    #[test]
    fn conversion_cannot_bypass_the_existing_raw_source_angular_budget() {
        let pitch = 89.945_f64.to_radians() * 0.5;
        let source_q = [pitch.cos(), 0., -pitch.sin(), 0.]; // roll = 180 degrees
        let imported = imported_rotation(source_q).unwrap().unwrap().0;
        let raw = Skeleton { names: vec!["root".into(), "arbitrary_branch".into()],
            parents: vec![-1,0], positions: vec![[0.;3];2],
            rotations: vec![[0.,0.,0.,1.],source_q], scales: vec![1.;2], flags: vec![0x40;2] };
        let mut actual = raw.clone(); actual.rotations[1] = imported;
        let original = extensions::project_skeleton(&raw, &["root".into()]).unwrap();
        let source = SourceBinding { original_sha256: String::new(), source_sha256: String::new(),
            skeleton: raw, extra_names: vec!["arbitrary_branch".into()], required_extra_names: vec!["arbitrary_branch".into()] };
        let mut proof = Proof { bones: vec![BoneProof { name: "arbitrary_branch".into(),
            source_rotation: source_q, imported_rotation: imported, horizontal_axis_length: 0.00096,
            native_component_residual: 0., animation_component_residual: 0. }], ..Proof::default() };
        let error = audit(&actual,&original,&source,&Value::new(Kind::Null),&mut proof).unwrap_err();
        assert!(error.to_string().contains("Compiled skeleton bind transform changed: arbitrary_branch"));
    }
}
