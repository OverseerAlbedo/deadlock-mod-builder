//! Read-only fitting proof. Source identity, flags, mesh bindings, capacity and
//! immutable compiler-snapshot checks remain the caller's responsibility.
//!
//! Native three-point fits use NodeBases + ReverseOffsets instead of FitMatrices.
//! The captured TangTang compiler output independently supplies both the axis
//! adjustment and rest transforms, allowing their relationship to be checked.
use super::{number, usize_field};
use crate::{field, invalid, strings, Result};
use deadlock_resource::Value;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Proof {
    pub matrix_fitted_bones: Vec<String>,
    pub compact_fitted_bones: Vec<String>,
    #[serde(default)]
    pub separate_position_support_bones: Vec<String>,
    pub max_compact_rotation_residual: f64,
    pub max_compact_offset_residual_inches: f64,
}
impl Proof {
    pub fn fitted_bones(&self) -> BTreeSet<String> {
        self.matrix_fitted_bones
            .iter()
            .chain(&self.compact_fitted_bones)
            .cloned()
            .collect()
    }
}
type Vec3 = [f64; 3];
type Mat3 = [[f64; 3]; 3];
pub(super) fn sub(a: Vec3, b: Vec3) -> Vec3 {
    std::array::from_fn(|i| a[i] - b[i])
}
pub(super) fn dot(a: Vec3, b: Vec3) -> f64 {
    (0..3).map(|i| a[i] * b[i]).sum()
}
pub(super) fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
pub(super) fn unit(a: Vec3) -> Result<Vec3> {
    let length = dot(a, a).sqrt();
    if !length.is_finite() || length <= 1e-12 {
        return Err(invalid("Degenerate native compact orientation support."));
    }
    Ok(a.map(|v| v / length))
}
pub(super) fn vector<const N: usize>(v: &Value) -> Result<[f64; N]> {
    let a = v.array()?;
    if a.len() != N {
        return Err(invalid("Invalid native fitting vector length."));
    }
    let mut out = [0.; N];
    for (o, v) in out.iter_mut().zip(a) {
        *o = number(v)?;
    }
    Ok(out)
}
fn pose(v: &Value) -> Result<[f64; 8]> {
    let p = vector::<8>(v)?;
    if p[3] <= 0. {
        return Err(invalid("Invalid native fitting rest scale."));
    }
    Ok(p)
}
fn position(p: [f64; 8]) -> Vec3 {
    [p[0], p[1], p[2]]
}
pub(super) fn rotation(q: [f64; 4]) -> Result<Mat3> {
    let n = q.iter().map(|v| v * v).sum::<f64>().sqrt();
    if !n.is_finite() || (n - 1.).abs() > 1e-3 {
        return Err(invalid("Invalid native fitting quaternion."));
    }
    let [x, y, z, w] = q.map(|v| v / n);
    Ok([
        [
            1. - 2. * (y * y + z * z),
            2. * (x * y - z * w),
            2. * (x * z + y * w),
        ],
        [
            2. * (x * y + z * w),
            1. - 2. * (x * x + z * z),
            2. * (y * z - x * w),
        ],
        [
            2. * (x * z - y * w),
            2. * (y * z + x * w),
            1. - 2. * (x * x + y * y),
        ],
    ])
}
fn rows<'a>(v: &'a Value, key: &str) -> Result<&'a [Value]> {
    match v.get(key) {
        Some(v) => Ok(v.array()?),
        None => Ok(&[]),
    }
}
fn require_selected(name: &str, selected: &BTreeSet<String>) -> Result<()> {
    if !selected.contains(name) {
        return Err(invalid(format!(
            "Native solver drives an unselected or body joint: {name}"
        )));
    }
    Ok(())
}

/// The captured ribbon compiler output uses a fifth point for position. Prove
/// its ownership and direct, active, fixed-length connection to every basis
/// point. Mere proximity, a shared name, or multi-hop connectivity is not proof.
/// The caller separately checks dynamic partitions, basis/rest reconstruction,
/// full scalar/SIMD rod coverage, source identity and the immutable native FE.
fn separate_position_support(
    fe: &Value,
    poses: &[Value],
    node: usize,
    point: usize,
    indices: &[usize],
    offset: Vec3,
) -> Result<()> {
    let parents = field(fe, "m_SkelParents")?.array()?;
    if parents.len() != poses.len() || parents[point].integer()? != node as i128 {
        return Err(invalid(
            "Separate compact position support has an unrelated parent.",
        ));
    }
    let mut owner = None;
    for row in rows(fe, "m_CtrlOffsets")? {
        if usize_field(row, "nCtrlChild")? == point && owner.replace(row).is_some() {
            return Err(invalid(
                "Duplicate separate compact position control offset.",
            ));
        }
    }
    let owner =
        owner.ok_or_else(|| invalid("Separate compact position support has no control offset."))?;
    let authored = vector::<3>(field(owner, "vOffset")?)?;
    if usize_field(owner, "nCtrlParent")? != node
        || (0..3).any(|i| (authored[i] - offset[i]).abs() > 1e-4)
    {
        return Err(invalid(
            "Separate compact position support disagrees with its bone binding.",
        ));
    }
    let required = indices.iter().copied().collect::<BTreeSet<_>>();
    let mut connected = BTreeSet::new();
    let origin = position(pose(&poses[point])?);
    for rod in rows(fe, "m_Rods")? {
        let ends = field(rod, "nNode")?.array()?;
        if ends.len() != 2 {
            return Err(invalid("Invalid compact support rod endpoints."));
        }
        let ends = ends
            .iter()
            .map(|v| {
                usize::try_from(v.integer()?)
                    .map_err(|_| invalid("Invalid compact support rod index."))
            })
            .collect::<Result<Vec<_>>>()?;
        let other = if ends[0] == point {
            ends[1]
        } else if ends[1] == point {
            ends[0]
        } else {
            continue;
        };
        if !required.contains(&other) {
            continue;
        }
        let min = number(field(rod, "flMinDist")?)?;
        let max = number(field(rod, "flMaxDist")?)?;
        let weight = number(field(rod, "flWeight0")?)?;
        let relaxation = number(field(rod, "flRelaxationFactor")?)?;
        let delta = sub(origin, position(pose(&poses[other])?));
        if min <= 0.
            || min != max
            || weight <= 0.
            || weight >= 1.
            || relaxation <= 0.
            || (dot(delta, delta).sqrt() - max).abs() > 1e-4
            || !connected.insert(other)
        {
            return Err(invalid(
                "Invalid or duplicate direct rod for separate compact position support.",
            ));
        }
    }
    if connected != required {
        return Err(invalid(
            "Separate compact position support lacks direct rods to every orientation point.",
        ));
    }
    Ok(())
}

/// Recognize explicit matrix fits and native compact three-point fits. Never
/// accepts names/flags alone or rewrites any solver table to obtain coverage.
pub fn audit(
    fe: &Value,
    skeleton_names: &[String],
    selected: &BTreeSet<String>,
    weighted: &BTreeSet<String>,
) -> Result<Proof> {
    audit_with_roll_lock(fe, skeleton_names, selected, weighted, &BTreeMap::new())
}
// Only rows deterministically reconstructed from the immutable native snapshot
// by roll_lock::apply may use a fixed roll reference. Public audit stays strict.
pub(super) fn audit_with_roll_lock(
    fe: &Value, skeleton_names: &[String], selected: &BTreeSet<String>,
    weighted: &BTreeSet<String>, locked: &BTreeMap<usize, Value>,
) -> Result<Proof> {
    let names = strings(field(fe, "m_CtrlName")?)?;
    let poses = field(fe, "m_InitPose")?.array()?;
    let masses = field(fe, "m_NodeInvMasses")?.array()?;
    let static_ = usize_field(fe, "m_nStaticNodes")?;
    let count = usize_field(fe, "m_nNodeCount")?;
    if count != names.len()
        || count != poses.len()
        || count != masses.len()
        || static_ >= count
        || names.iter().collect::<BTreeSet<_>>().len() != names.len()
        || selected.is_empty()
        || weighted.is_empty()
        || !weighted.is_subset(selected)
    {
        return Err(invalid(
            "Incomplete native fitting identities or rest data.",
        ));
    }
    let skeleton: BTreeSet<_> = skeleton_names.iter().map(String::as_str).collect();
    let weights = field(fe, "m_FitWeights")?.array()?;
    let mut begin = 0;
    let mut fitted = BTreeSet::new();
    let mut proof = Proof::default();
    for matrix in field(fe, "m_FitMatrices")?.array()? {
        let end = usize_field(matrix, "nEnd")?;
        let dynamic = usize_field(matrix, "nBeginDynamic")?;
        let node = usize_field(matrix, "nNode")?;
        if begin > dynamic || dynamic > end || end > weights.len() || node >= count {
            return Err(invalid("Invalid native joint fitting ranges."));
        }
        let target = &names[node];
        let mut points = vec![];
        for w in &weights[dynamic..end] {
            let i = usize_field(w, "nNode")?;
            let weight = number(field(w, "flWeight")?)?;
            if weight < 0. || i < static_ || i >= count || number(&masses[i])? <= 0. {
                return Err(invalid("Joint fitting uses invalid dynamic support."));
            }
            if weight > 0. {
                points.push(position(pose(&poses[i])?));
            }
        }
        if !points.is_empty() && skeleton.contains(target.as_str()) {
            require_selected(target, selected)?;
            let noncollinear = points.first().is_some_and(|p| {
                points.iter().skip(1).any(|a| {
                    points.iter().skip(1).any(|b| {
                        let c = cross(sub(*a, *p), sub(*b, *p));
                        dot(c, c) > 1e-12
                    })
                })
            });
            if !noncollinear || !fitted.insert(target.clone()) {
                return Err(invalid(format!(
                    "Missing independent native orientation support: {target}"
                )));
            }
            proof.matrix_fitted_bones.push(target.clone());
        }
        begin = end;
    }
    if begin != weights.len() {
        return Err(invalid("Unbound native fit weights."));
    }

    let bases = rows(fe, "m_NodeBases")?;
    let reverses = rows(fe, "m_ReverseOffsets")?;
    let mut reverse = BTreeMap::new();
    for row in reverses {
        let bone = usize_field(row, "nBoneCtrl")?;
        let point = usize_field(row, "nTargetNode")?;
        vector::<3>(field(row, "vOffset")?)?;
        if bone >= count || point >= count || reverse.insert(bone, row).is_some() {
            return Err(invalid(
                "Invalid or duplicated native reverse-offset binding.",
            ));
        }
    }
    // A SIMD-only or conflicting representation needs its own proof; never
    // ignore it and certify an unrelated scalar record for the same bone.
    if !reverses.is_empty() && !rows(fe, "m_SimdNodeBases")?.is_empty() {
        return Err(invalid("Unsupported SIMD compact joint fitting; scalar and SIMD correspondence needs verification."));
    }
    let mut seen = BTreeSet::new();
    for base in bases {
        let node = usize_field(base, "nNode")?;
        if node >= count || !seen.insert(node) {
            return Err(invalid("Invalid or duplicated native node basis."));
        }
        let target = &names[node];
        if !skeleton.contains(target.as_str()) {
            continue;
        }
        require_selected(target, selected)?;
        let row = reverse.remove(&node).ok_or_else(|| {
            invalid(format!(
                "Native compact fit has no position binding: {target}"
            ))
        })?;
        let first_driven = usize_field(fe, "m_nFirstPositionDrivenNode")?;
        if first_driven < static_ || node < first_driven {
            return Err(invalid(
                "Compact fitted joint is outside the position-driven partition.",
            ));
        }
        let indices = ["nNodeX0", "nNodeX1", "nNodeY0", "nNodeY1"]
            .map(|k| usize_field(base, k))
            .into_iter()
            .collect::<Result<Vec<_>>>()?;
        let roll_locked = locked.get(&node).is_some_and(|expected| expected.canonical_eq(base));
        if locked.contains_key(&node) && !roll_locked {
            return Err(invalid("Strand orientation differs from the verified native transform."));
        }
        for (axis, &i) in indices.iter().enumerate() {
            if roll_locked && axis < 2 {
                if i >= static_ || number(&masses[i])? != 0. {
                    return Err(invalid("Strand roll reference is not fixed."));
                }
                continue;
            }
            if i < static_ || i >= first_driven || i >= count || number(&masses[i])? <= 0. {
                return Err(invalid(format!(
                    "Compact joint fit uses invalid dynamic support: {target}"
                )));
            }
        }
        let p = indices
            .iter()
            .map(|&i| pose(&poses[i]).map(position))
            .collect::<Result<Vec<_>>>()?;
        // Native basis: Y1-Y0, X1-X0 projected off Y, then X cross Y.
        // This reconstructs all 262 captured native qAdjust/rest relationships.
        let raw_x = sub(p[1], p[0]);
        let y = unit(sub(p[3], p[2]))?;
        let projection = dot(raw_x, y);
        let x = sub(raw_x, y.map(|v| v * projection));
        if dot(x, x) <= dot(raw_x, raw_x) * 1e-12 {
            return Err(invalid(format!(
                "Degenerate native compact orientation support: {target}"
            )));
        }
        let x = unit(x)?;
        let z = cross(x, y);
        let basis: Mat3 = std::array::from_fn(|r| [x[r], y[r], z[r]]);
        let adjust = rotation(vector::<4>(field(base, "qAdjust")?)?)?;
        let rest = pose(&poses[node])?;
        let expected = rotation([rest[4], rest[5], rest[6], rest[7]])?;
        let mut rotation_error: f64 = 0.;
        for r in 0..3 {
            for c in 0..3 {
                let actual: f64 = (0..3).map(|k| basis[r][k] * adjust[k][c]).sum();
                rotation_error = rotation_error.max((actual - expected[r][c]).abs());
            }
        }
        if rotation_error > 1e-4 {
            return Err(invalid(format!(
                "Compact joint basis disagrees with native rest orientation: {target}"
            )));
        }
        let point = usize_field(row, "nTargetNode")?;
        if point < static_ || point >= first_driven || number(&masses[point])? <= 0. {
            return Err(invalid(format!(
                "Compact position binding uses invalid dynamic support: {target}"
            )));
        }
        let offset = vector::<3>(field(row, "vOffset")?)?;
        if !indices.contains(&point) {
            separate_position_support(fe, poses, node, point, &indices, offset)?;
            proof.separate_position_support_bones.push(target.clone());
        }
        let point_pos = position(pose(&poses[point])?);
        let mut offset_error: f64 = 0.;
        for r in 0..3 {
            // Native reverse offsets are rigid bone-local vectors (no scale).
            let delta: f64 = (0..3).map(|c| expected[r][c] * offset[c]).sum();
            offset_error = offset_error.max((rest[r] + delta - point_pos[r]).abs());
        }
        if offset_error > 1e-4 || !fitted.insert(target.clone()) {
            return Err(invalid(format!(
                "Invalid or duplicate native compact position fit: {target}"
            )));
        }
        proof.max_compact_rotation_residual =
            proof.max_compact_rotation_residual.max(rotation_error);
        proof.max_compact_offset_residual_inches =
            proof.max_compact_offset_residual_inches.max(offset_error);
        proof.compact_fitted_bones.push(target.clone());
    }
    if !reverse.is_empty() {
        return Err(invalid(
            "Native reverse offsets have no verified compact bone basis.",
        ));
    }
    if !weighted.is_subset(&fitted) {
        let missing = weighted.difference(&fitted).cloned().collect::<Vec<_>>();
        return Err(invalid(format!(
            "Native secondary fitting is incomplete: {}",
            missing.join(", ")
        )));
    }
    Ok(proof)
}
