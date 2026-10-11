//! f64 local T*R*S composition; explicit f32 only at DMX serialization boundary.
use crate::{invalid, norm, skeleton_identity, Result, Skeleton};
use serde::{Deserialize, Serialize};
pub const SCALE_POLICY: &str = "source2-local-scale-dmx-v1";
pub const LOCAL_SCALE_TOLERANCE: f64 = 1e-4;
pub const WORLD_SCALE_TOLERANCE: f64 = 1e-3;
pub const WORLD_POSITION_TOLERANCE_MM: f64 = 0.5;
pub type Matrix4 = [[f64; 4]; 4];
pub fn identity() -> Matrix4 {
    let mut value = [[0.; 4]; 4];
    for (i, row) in value.iter_mut().enumerate() {
        row[i] = 1.;
    }
    value
}
pub fn multiply(a: &Matrix4, b: &Matrix4) -> Matrix4 {
    let mut out = [[0.; 4]; 4];
    for i in 0..4 {
        for j in 0..4 {
            for k in 0..4 {
                out[i][j] += a[i][k] * b[k][j];
            }
        }
    }
    out
}
fn inverse(value: &Matrix4) -> Result<Matrix4> {
    let mut a = *value;
    let mut result = identity();
    for col in 0..4 {
        let pivot = (col..4)
            .max_by(|&aidx, &bidx| a[aidx][col].abs().total_cmp(&a[bidx][col].abs()))
            .unwrap();
        if a[pivot][col] == 0. || !a[pivot][col].is_finite() {
            return Err(invalid("Target accumulated bind is not invertible"));
        }
        a.swap(col, pivot);
        result.swap(col, pivot);
        let divisor = a[col][col];
        for k in 0..4 {
            a[col][k] /= divisor;
            result[col][k] /= divisor;
        }
        for row in 0..4 {
            if row == col {
                continue;
            }
            let factor = a[row][col];
            for k in 0..4 {
                a[row][k] -= factor * a[col][k];
                result[row][k] -= factor * result[col][k];
            }
        }
    }
    if result.iter().flatten().any(|v| !v.is_finite()) {
        return Err(invalid("Inverse bind is not finite"));
    }
    Ok(result)
}
pub fn float32(value: f64) -> Result<f64> {
    let out = value as f32;
    if !out.is_finite() {
        return Err(invalid(
            "Target bind value cannot be represented as finite DMX float32",
        ));
    }
    Ok(out as f64)
}
#[derive(Debug, Clone)]
pub struct BindState {
    pub skeleton: Skeleton,
    pub world: Vec<Matrix4>,
    pub rigid: Vec<Matrix4>,
    pub inverse: Vec<Matrix4>,
    pub accumulated: Vec<f64>,
}
pub fn bind_state(skeleton: &Skeleton) -> Result<BindState> {
    skeleton.validate(false)?;
    for &scale in &skeleton.scales {
        if scale <= 0. || float32(scale)? <= 0. {
            return Err(invalid(
                "Target hero requires positive finite float32 scale",
            ));
        }
    }
    let n = skeleton.names.len();
    let mut result = BindState {
        skeleton: skeleton.clone(),
        world: vec![identity(); n],
        rigid: vec![identity(); n],
        inverse: vec![identity(); n],
        accumulated: vec![0.; n],
    };
    let mut done = vec![false; n];
    for start in 0..n {
        let mut chain = vec![];
        let mut at = Some(start);
        while let Some(i) = at {
            if done[i] {
                break;
            }
            chain.push(i);
            at = if skeleton.parents[i] < 0 {
                None
            } else {
                Some(skeleton.parents[i] as usize)
            };
        }
        for i in chain.into_iter().rev() {
            let q = skeleton.rotations[i];
            let length = norm(&q);
            let [x, y, z, w] = q.map(|v| v / length);
            let mut rigid = identity();
            rigid[0][..3].copy_from_slice(&[
                1. - 2. * y * y - 2. * z * z,
                2. * x * y - 2. * z * w,
                2. * x * z + 2. * y * w,
            ]);
            rigid[1][..3].copy_from_slice(&[
                2. * x * y + 2. * z * w,
                1. - 2. * x * x - 2. * z * z,
                2. * y * z - 2. * x * w,
            ]);
            rigid[2][..3].copy_from_slice(&[
                2. * x * z - 2. * y * w,
                2. * y * z + 2. * x * w,
                1. - 2. * x * x - 2. * y * y,
            ]);
            for row in 0..3 {
                rigid[row][3] = skeleton.positions[i][row];
            }
            let mut full = rigid;
            for row in full.iter_mut().take(3) {
                for v in row.iter_mut().take(3) {
                    *v *= skeleton.scales[i];
                }
            }
            let mut cumulative = skeleton.scales[i];
            let parent = skeleton.parents[i];
            if parent >= 0 {
                let parent = parent as usize;
                full = multiply(&result.world[parent], &full);
                rigid = multiply(&result.rigid[parent], &rigid);
                cumulative *= result.accumulated[parent];
            }
            if full.iter().flatten().any(|v| !v.is_finite())
                || !cumulative.is_finite()
                || cumulative <= 0.
            {
                return Err(invalid("Target accumulated bind is not finite/invertible"));
            }
            let inverse = inverse(&full)?;
            for residual in [multiply(&full, &inverse), multiply(&inverse, &full)] {
                for row in 0..4 {
                    for col in 0..4 {
                        if (residual[row][col] - if row == col { 1. } else { 0. }).abs() > 1e-7 {
                            return Err(invalid("Target world/inverse bind consistency failed"));
                        }
                    }
                }
            }
            result.world[i] = full;
            result.rigid[i] = rigid;
            result.inverse[i] = inverse;
            result.accumulated[i] = cumulative;
            done[i] = true;
        }
    }
    Ok(result)
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScaleRow {
    pub name: String,
    pub original_scale: f64,
    pub export_scale: f64,
    pub local_scale_delta: f64,
    pub cumulative_scale: f64,
    pub original_cumulative_scale: f64,
    pub world_scale_delta: f64,
    pub world_joint_delta_mm: f64,
    pub omitted_scale_world_joint_drift_mm: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScaleAudit {
    pub policy: String,
    pub bone_count: usize,
    pub local_scale_tolerance: f64,
    pub world_scale_tolerance: f64,
    pub world_joint_tolerance_mm: f64,
    pub max_local_scale_delta: f64,
    pub max_world_scale_delta: f64,
    pub max_world_joint_delta_mm: f64,
    pub max_omitted_scale_world_joint_drift_mm: f64,
    pub max_omitted_scale_world_scale_drift: f64,
    pub approximated_bones: usize,
    pub scaled_bones: usize,
    pub bones: Vec<ScaleRow>,
    pub original_skeleton_modified: bool,
    pub exact_scale_preservation: bool,
    pub world_inverse_bind_consistent: bool,
    pub native_compiler_validated: bool,
}
fn position_distance(a: &Matrix4, b: &Matrix4) -> f64 {
    (0..3)
        .map(|i| (a[i][3] - b[i][3]).powi(2))
        .sum::<f64>()
        .sqrt()
        * 25.4
}
fn compare_states(actual: &BindState, reference: &BindState, exact: bool) -> Result<ScaleAudit> {
    let lookup = actual.skeleton.indices();
    let mut rows = vec![];
    let mut exact_preserved = true;
    for (i, name) in reference.skeleton.names.iter().enumerate() {
        let &j = lookup
            .get(name.as_str())
            .ok_or_else(|| invalid("Missing target bone"))?;
        let original = reference.skeleton.scales[i];
        let observed = actual.skeleton.scales[j];
        if exact && observed != float32(original)? {
            return Err(invalid(format!(
                "DMX local scale differs from original float32 value: {name}"
            )));
        }
        let local_delta = (observed - original).abs();
        let world_delta = (actual.accumulated[j] - reference.accumulated[i]).abs();
        let position_delta = position_distance(&actual.world[j], &reference.world[i]);
        if local_delta > LOCAL_SCALE_TOLERANCE
            || world_delta > WORLD_SCALE_TOLERANCE
            || position_delta > WORLD_POSITION_TOLERANCE_MM
        {
            return Err(invalid(format!("Target bind preservation budget exceeded at {name}: local scale difference {local_delta}; cumulative scale difference {world_delta}; world joint difference {position_delta} mm")));
        }
        exact_preserved &= observed == original || observed == float32(original)?;
        rows.push(ScaleRow {
            name: name.clone(),
            original_scale: original,
            export_scale: observed,
            local_scale_delta: local_delta,
            cumulative_scale: actual.accumulated[j],
            original_cumulative_scale: reference.accumulated[i],
            world_scale_delta: world_delta,
            world_joint_delta_mm: position_delta,
            omitted_scale_world_joint_drift_mm: position_distance(
                &reference.world[i],
                &reference.rigid[i],
            ),
        });
    }
    Ok(ScaleAudit {
        policy: SCALE_POLICY.into(),
        bone_count: rows.len(),
        local_scale_tolerance: LOCAL_SCALE_TOLERANCE,
        world_scale_tolerance: WORLD_SCALE_TOLERANCE,
        world_joint_tolerance_mm: WORLD_POSITION_TOLERANCE_MM,
        max_local_scale_delta: rows.iter().map(|r| r.local_scale_delta).fold(0., f64::max),
        max_world_scale_delta: rows.iter().map(|r| r.world_scale_delta).fold(0., f64::max),
        max_world_joint_delta_mm: rows
            .iter()
            .map(|r| r.world_joint_delta_mm)
            .fold(0., f64::max),
        max_omitted_scale_world_joint_drift_mm: rows
            .iter()
            .map(|r| r.omitted_scale_world_joint_drift_mm)
            .fold(0., f64::max),
        max_omitted_scale_world_scale_drift: reference
            .accumulated
            .iter()
            .map(|s| (s - 1.).abs())
            .fold(0., f64::max),
        approximated_bones: 0,
        scaled_bones: reference
            .skeleton
            .scales
            .iter()
            .filter(|&&s| s != 1.)
            .count(),
        bones: rows,
        original_skeleton_modified: false,
        exact_scale_preservation: exact_preserved,
        world_inverse_bind_consistent: true,
        native_compiler_validated: false,
    })
}
pub fn audit_scale_preservation(
    actual: &Skeleton,
    reference: &Skeleton,
    require_exact_float32_scale: bool,
) -> Result<ScaleAudit> {
    skeleton_identity(actual, reference)?;
    compare_states(
        &bind_state(actual)?,
        &bind_state(reference)?,
        require_exact_float32_scale,
    )
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PreparedBone {
    pub name: String,
    pub parent: Option<String>,
    pub local_position: [f64; 3],
    pub local_rotation: [f64; 4],
    pub local_scale: f64,
    pub position: [f64; 3],
    pub matrix: Matrix4,
    pub inverse_bind_matrix: Matrix4,
    pub length: f64,
}
pub fn prepare_target(skeleton: &Skeleton) -> Result<(Vec<PreparedBone>, ScaleAudit)> {
    let state = bind_state(skeleton)?;
    let bones = (0..skeleton.names.len())
        .map(|i| PreparedBone {
            name: skeleton.names[i].clone(),
            parent: skeleton.parent_name(i).map(str::to_string),
            local_position: skeleton.positions[i],
            local_rotation: skeleton.rotations[i],
            local_scale: skeleton.scales[i],
            position: [
                state.world[i][0][3],
                state.world[i][1][3],
                state.world[i][2][3],
            ],
            matrix: state.world[i],
            inverse_bind_matrix: state.inverse[i],
            length: 1.,
        })
        .collect();
    Ok((bones, compare_states(&state, &state, false)?))
}
