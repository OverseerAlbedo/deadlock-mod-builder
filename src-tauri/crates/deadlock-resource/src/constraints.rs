//! Necessary rest-pose capsule/rod feasibility; never a simulation or publication grant.
use crate::{
    cloth::{self, Status},
    Kind, Value,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};

const QUATERNION_ENVELOPE: f64 = 8.0 / (1u64 << 23) as f64;
#[derive(Debug)]
enum Problem {
    Pending(String),
    Invalid(String),
}
impl From<crate::Error> for Problem {
    fn from(e: crate::Error) -> Self {
        Self::Invalid(e.to_string())
    }
}
type Check<T> = std::result::Result<T, Problem>;
fn pending(reason: impl Into<String>) -> Problem {
    Problem::Pending(reason.into())
}
fn bad(reason: impl Into<String>) -> Problem {
    Problem::Invalid(reason.into())
}
fn array<'a>(v: &'a Value, k: &str) -> Check<&'a [Value]> {
    v.get(k)
        .ok_or_else(|| pending(format!("missing solver field: {k}")))?
        .array()
        .map_err(Into::into)
}
fn integer(v: &Value, low: i128, high: i128) -> Check<i128> {
    let n = cloth::integer(v)?;
    if n < low || n > high {
        return Err(bad("invalid constraint index, count or collision mask"));
    }
    Ok(n)
}
fn vector<const N: usize>(v: &Value) -> Check<[f64; N]> {
    let values = cloth::numbers(v)?;
    values
        .try_into()
        .map_err(|_| bad("invalid constraint vector shape"))
}
fn norm(values: &[f64]) -> f64 {
    let scale = values.iter().map(|v| v.abs()).fold(0.0, f64::max);
    if scale == 0.0 {
        return 0.0;
    }
    scale * sum(values.iter().map(|v| (v / scale) * (v / scale))).sqrt()
}
fn sum(values: impl IntoIterator<Item = f64>) -> f64 {
    let mut partials: Vec<f64> = Vec::new();
    for mut x in values {
        let mut i = 0;
        for j in 0..partials.len() {
            let mut y = partials[j];
            if x.abs() < y.abs() {
                std::mem::swap(&mut x, &mut y);
            }
            let hi = x + y;
            let lo = y - (hi - x);
            if lo != 0.0 {
                partials[i] = lo;
                i += 1;
            }
            x = hi;
        }
        partials.truncate(i);
        partials.push(x);
    }
    partials.iter().rev().copied().sum()
}
fn rotate(point: [f64; 3], quaternion: [f64; 4]) -> Check<[f64; 3]> {
    let n = norm(&quaternion);
    if n == 0.0 {
        return Err(bad("zero-length rest quaternion"));
    }
    if (n - 1.0).abs() > QUATERNION_ENVELOPE {
        return Err(pending(
            "rest quaternion outside supported float32 unit-rounding envelope",
        ));
    }
    let [x, y, z, w] = quaternion.map(|v| v / n);
    let [a, b, c] = point;
    Ok([
        (1.0 - 2.0 * y * y - 2.0 * z * z) * a
            + (2.0 * x * y - 2.0 * z * w) * b
            + (2.0 * x * z + 2.0 * y * w) * c,
        (2.0 * x * y + 2.0 * z * w) * a
            + (1.0 - 2.0 * x * x - 2.0 * z * z) * b
            + (2.0 * y * z - 2.0 * x * w) * c,
        (2.0 * x * z - 2.0 * y * w) * a
            + (2.0 * y * z + 2.0 * x * w) * b
            + (1.0 - 2.0 * x * x - 2.0 * y * y) * c,
    ])
}
fn penetration(point: [f64; 3], start: [f64; 3], end: [f64; 3], radius: f64) -> f64 {
    let edge = std::array::from_fn::<_, 3, _>(|i| end[i] - start[i]);
    let length = norm(&edge);
    let closest = if length > 0.0 {
        let direction = edge.map(|v| v / length);
        let distance = sum((0..3).map(|i| (point[i] - start[i]) * direction[i])).clamp(0.0, length);
        std::array::from_fn(|i| start[i] + distance * direction[i])
    } else {
        start
    };
    radius - norm(&std::array::from_fn::<_, 3, _>(|i| point[i] - closest[i]))
}
fn ulp(value: f64) -> f64 {
    if value == 0.0 {
        return f64::from_bits(1);
    }
    let n = value.abs();
    f64::from_bits(n.to_bits() + 1) - n
}
fn fields(v: &Value, wanted: &[&str]) -> Check<()> {
    let keys: BTreeSet<_> = v.object()?.iter().map(|(k, _)| k.as_str()).collect();
    if keys != wanted.iter().copied().collect() {
        return Err(pending("unknown or incomplete constraint record schema"));
    }
    Ok(())
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Conflict {
    pub capsule_index: usize,
    pub capsule_control: String,
    pub rod_index: usize,
    pub static_node: usize,
    pub dynamic_node: usize,
    pub static_control: String,
    pub dynamic_control: String,
    pub anchor_penetration_mm: f64,
    pub rod_maximum_length_mm: f64,
    pub incompatibility_lower_bound_mm: f64,
    pub incompatibility_upper_bound_mm: f64,
    pub transform_uncertainty_bound_mm: f64,
    pub comparison_roundoff_bound_mm: f64,
    pub collider_mask: u16,
    pub dynamic_tree_collision_mask: Option<u16>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Evidence {
    pub compiled_resources_modified: bool,
    pub runtime_ready: bool,
    pub build_authorized: bool,
    pub runtime_collision_execution_verified: bool,
    pub collision_tree_topology_and_execution_verified: bool,
    pub animation_and_other_constraint_feasibility_checked: bool,
    pub cloth_solver_present: Option<bool>,
    pub capsule_count: Option<usize>,
    pub applicable_uniform_capsule_constraints: Option<bool>,
    pub uniform_tree_collision_mask: Option<u16>,
    pub mixed_mask_conservative_superset: bool,
    pub maximum_quaternion_norm_deviation: f64,
    pub scalar_simd_rod_coverage_status: Option<Status>,
    pub node_count: usize,
    pub static_nodes: usize,
    pub dynamic_nodes: usize,
    pub boundary_rod_count: usize,
    pub checked_rod_capsule_pairs: usize,
    pub mask_excluded_rod_capsule_pairs: usize,
    pub fourth_slot_geometric_uncertainty_bound_mm: f64,
    pub quaternion_geometric_uncertainty_bound_mm: f64,
    pub combined_transform_uncertainty_bound_mm: f64,
    pub maximum_comparison_roundoff_bound_mm: f64,
    pub uncertain_rod_capsule_pairs: usize,
    pub conflicting_rod_capsule_pairs: usize,
    pub distinct_rods: usize,
    pub distinct_dynamic_nodes: usize,
    pub distinct_anchors: usize,
    pub by_capsule: BTreeMap<String, usize>,
    pub worst_examples: Vec<Conflict>,
    pub uncertain_examples: Vec<Conflict>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Audit {
    pub status: Status,
    pub reason: Option<String>,
    pub evidence: Evidence,
}
pub fn audit(physics: &Value) -> Audit {
    let mut evidence = Evidence::default();
    let result = check(physics, &mut evidence);
    let (status, reason) = match result {
        Ok(()) => (Status::Passed, None),
        Err(Problem::Pending(r)) => (Status::Pending, Some(r)),
        Err(Problem::Invalid(r)) => (Status::Failed, Some(r)),
    };
    Audit {
        status,
        reason,
        evidence,
    }
}
fn check(physics: &Value, e: &mut Evidence) -> Check<()> {
    physics.object()?;
    let fe = physics
        .get("m_pFeModel")
        .ok_or_else(|| pending("physics has no solver field"))?;
    if matches!(fe.kind, Kind::Null) {
        e.cloth_solver_present = Some(false);
        return Ok(());
    }
    fe.object()?;
    e.cloth_solver_present = Some(true);
    let capsules = array(fe, "m_TaperedCapsuleRigids")?;
    e.capsule_count = Some(capsules.len());
    for key in [
        "m_VertexMaps",
        "m_VertexMapValues",
        "m_DynNodeVertexSet",
        "m_TaperedCapsuleStretches",
        "m_SphereRigids",
        "m_CollisionPlanes",
        "m_SDFRigids",
        "m_BoxRigids",
        "m_SimdRodsAnim",
    ] {
        if !array(fe, key)?.is_empty() {
            return Err(pending(format!(
                "unsupported exclusions, collider or animated rods: {key}"
            )));
        }
    }
    if capsules.is_empty() {
        e.applicable_uniform_capsule_constraints = Some(false);
        return Ok(());
    }
    let count = integer(
        fe.get("m_nNodeCount")
            .ok_or_else(|| pending("missing node count"))?,
        0,
        65535,
    )? as usize;
    let static_ = integer(
        fe.get("m_nStaticNodes")
            .ok_or_else(|| pending("missing static count"))?,
        0,
        count as i128,
    )? as usize;
    let dynamic = count - static_;
    let poses = array(fe, "m_InitPose")?
        .iter()
        .map(vector::<8>)
        .collect::<Check<Vec<_>>>()?;
    let masses = array(fe, "m_NodeInvMasses")?
        .iter()
        .map(cloth::number)
        .collect::<crate::Result<Vec<_>>>()?;
    let names = array(fe, "m_CtrlName")?
        .iter()
        .map(Value::string)
        .collect::<crate::Result<Vec<_>>>()?;
    let masks = array(fe, "m_TreeCollisionMasks")?
        .iter()
        .map(|v| integer(v, 0, 65535).map(|v| v as u16))
        .collect::<Check<Vec<_>>>()?;
    if poses.len() != count
        || masses.len() != count
        || names.len() != count
        || names.iter().any(|n| n.is_empty())
        || masks.len() != (2 * dynamic).saturating_sub(1)
    {
        return Err(bad("inconsistent node, control, pose or tree-mask arrays"));
    }
    if masses[..static_].iter().any(|v| *v != 0.0) || masses[static_..].iter().any(|v| *v <= 0.0) {
        return Err(bad("inverse masses disagree with static/dynamic prefix"));
    }
    let mixed = masks.iter().copied().collect::<BTreeSet<_>>().len() > 1;
    let node_mask = if mixed {
        None
    } else {
        Some(masks.first().copied().unwrap_or(0))
    };
    e.uniform_tree_collision_mask = node_mask;
    e.mixed_mask_conservative_superset = mixed;
    for pose in &poses {
        rotate(
            [0.; 3],
            pose[4..]
                .try_into()
                .map_err(|_| bad("invalid quaternion"))?,
        )?;
        e.maximum_quaternion_norm_deviation = e
            .maximum_quaternion_norm_deviation
            .max((norm(&pose[4..]) - 1.0).abs());
    }
    let coverage = cloth::audit_rods(physics);
    e.scalar_simd_rod_coverage_status = Some(coverage.status);
    match coverage.status {
        Status::Failed => {
            return Err(bad(format!(
                "complete scalar/SIMD rod coverage failed: {:?}",
                coverage.reason
            )))
        }
        Status::Pending => return Err(pending("complete scalar/SIMD rod coverage unsupported")),
        Status::Passed => (),
    }
    let mut boundary = Vec::new();
    for (index, rod) in array(fe, "m_Rods")?.iter().enumerate() {
        fields(
            rod,
            &[
                "nNode",
                "flMaxDist",
                "flMinDist",
                "flWeight0",
                "flRelaxationFactor",
            ],
        )?;
        let nodes = cloth::array(rod, "nNode")?;
        if nodes.len() != 2 {
            return Err(bad("rod must have two nodes"));
        }
        let a = integer(&nodes[0], 0, count as i128 - 1)? as usize;
        let b = integer(&nodes[1], 0, count as i128 - 1)? as usize;
        let min = cloth::number(cloth::field(rod, "flMinDist")?)?;
        let max = cloth::number(cloth::field(rod, "flMaxDist")?)?;
        let weight = cloth::number(cloth::field(rod, "flWeight0")?)?;
        let relaxation = cloth::number(cloth::field(rod, "flRelaxationFactor")?)?;
        if min < 0.0 || max < min || !(0.0..=1.0).contains(&weight) || relaxation < 0.0 {
            return Err(bad("invalid rod limits, weight or relaxation"));
        }
        if relaxation == 0.0 {
            return Err(pending("zero-relaxation rod semantics unsupported"));
        }
        if (a < static_) != (b < static_) {
            let (anchor, node) = if a < static_ { (a, b) } else { (b, a) };
            if weight != if a == anchor { 0.0 } else { 1.0 } {
                return Err(pending(
                    "static-anchor rod weight differs from fixed endpoint",
                ));
            }
            boundary.push((index, anchor, node, max));
        }
    }
    let mut conflicts = Vec::new();
    let mut uncertain = Vec::new();
    let mut excluded = 0;
    let mut checked = 0;
    let mut maximum_slot = 0.0f64;
    let mut maximum_quaternion = 0.0f64;
    let mut maximum_ambiguity = 0.0f64;
    let mut maximum_roundoff = 0.0f64;
    for (capsule_index, capsule) in capsules.iter().enumerate() {
        fields(
            capsule,
            &[
                "vSphere",
                "nNode",
                "nCollisionMask",
                "nVertexMapIndex",
                "nFlags",
            ],
        )?;
        let control = integer(cloth::field(capsule, "nNode")?, 0, count as i128 - 1)? as usize;
        if control >= static_ {
            return Err(pending(
                "dynamically driven capsule is not a fixed rest obstacle",
            ));
        }
        let mask = integer(cloth::field(capsule, "nCollisionMask")?, 0, 65535)? as u16;
        if integer(cloth::field(capsule, "nFlags")?, 0, 65535)? != 0
            || integer(cloth::field(capsule, "nVertexMapIndex")?, 0, 65535)? != 65535
        {
            return Err(pending(
                "flagged capsule or vertex-map exclusion unsupported",
            ));
        }
        let spheres = cloth::array(capsule, "vSphere")?;
        if spheres.len() != 2 {
            return Err(bad("capsule must contain two endpoint spheres"));
        }
        let first = vector::<4>(&spheres[0])?;
        let second = vector::<4>(&spheres[1])?;
        if first[3] < 0.0 || second[3] < 0.0 {
            return Err(bad("negative capsule radius"));
        }
        if first[3] != second[3] {
            return Err(pending("tapered capsule semantics unsupported"));
        }
        let pose = &poses[control];
        if (pose[3] - 1.0).abs() > 1e-5 {
            return Err(pending(
                "non-unit InitPose fourth-slot semantics unsupported",
            ));
        }
        let q: [f64; 4] = pose[4..]
            .try_into()
            .map_err(|_| bad("invalid quaternion"))?;
        let mut centers = [[0.; 3]; 2];
        for (i, sphere) in [first, second].iter().enumerate() {
            let rotated = rotate([sphere[0], sphere[1], sphere[2]], q)?;
            centers[i] = std::array::from_fn(|j| rotated[j] + pose[j]);
        }
        let extent = norm(&first[..3]).max(norm(&second[..3])) + first[3];
        let slot = (pose[3] - 1.0).abs() * extent;
        let quaternion = 2.0 * (norm(&q).powi(2) - 1.0).abs() * extent * pose[3].abs().max(1.0);
        let ambiguity = slot + quaternion;
        maximum_slot = maximum_slot.max(slot);
        maximum_quaternion = maximum_quaternion.max(quaternion);
        maximum_ambiguity = maximum_ambiguity.max(ambiguity);
        let mut penetrations = HashMap::new();
        for &(index, anchor, node, maximum) in &boundary {
            if mask == 0 || (!mixed && mask & node_mask.unwrap_or(0) == 0) {
                excluded += 1;
                continue;
            }
            checked += 1;
            let penetration = *penetrations.entry(anchor).or_insert_with(|| {
                penetration(
                    [poses[anchor][0], poses[anchor][1], poses[anchor][2]],
                    centers[0],
                    centers[1],
                    first[3],
                )
            });
            let lower = penetration - maximum - ambiguity;
            let upper = penetration - maximum + ambiguity;
            let scale = centers
                .iter()
                .flatten()
                .chain(poses[anchor][..3].iter())
                .map(|v| v.abs())
                .fold(1.0f64.max(maximum).max(first[3]), f64::max);
            let roundoff = 1e-5 + 64.0 * ulp(scale);
            maximum_roundoff = maximum_roundoff.max(roundoff);
            if [penetration, lower, upper, roundoff]
                .iter()
                .any(|v| !v.is_finite())
            {
                return Err(bad("non-finite capsule geometry arithmetic"));
            }
            if upper + roundoff > 0.0 {
                let row = Conflict {
                    capsule_index,
                    capsule_control: names[control].into(),
                    rod_index: index,
                    static_node: anchor,
                    dynamic_node: node,
                    static_control: names[anchor].into(),
                    dynamic_control: names[node].into(),
                    anchor_penetration_mm: penetration * 25.4,
                    rod_maximum_length_mm: maximum * 25.4,
                    incompatibility_lower_bound_mm: (lower - roundoff) * 25.4,
                    incompatibility_upper_bound_mm: (upper + roundoff) * 25.4,
                    transform_uncertainty_bound_mm: ambiguity * 25.4,
                    comparison_roundoff_bound_mm: roundoff * 25.4,
                    collider_mask: mask,
                    dynamic_tree_collision_mask: node_mask,
                };
                if lower > roundoff {
                    conflicts.push(row);
                } else {
                    uncertain.push(row);
                }
            }
        }
    }
    conflicts.sort_by(|a, b| {
        b.incompatibility_lower_bound_mm
            .total_cmp(&a.incompatibility_lower_bound_mm)
    });
    uncertain.sort_by(|a, b| {
        b.incompatibility_upper_bound_mm
            .total_cmp(&a.incompatibility_upper_bound_mm)
    });
    e.node_count = count;
    e.static_nodes = static_;
    e.dynamic_nodes = dynamic;
    e.boundary_rod_count = boundary.len();
    e.checked_rod_capsule_pairs = checked;
    e.mask_excluded_rod_capsule_pairs = excluded;
    e.fourth_slot_geometric_uncertainty_bound_mm = maximum_slot * 25.4;
    e.quaternion_geometric_uncertainty_bound_mm = maximum_quaternion * 25.4;
    e.combined_transform_uncertainty_bound_mm = maximum_ambiguity * 25.4;
    e.maximum_comparison_roundoff_bound_mm = maximum_roundoff * 25.4;
    e.conflicting_rod_capsule_pairs = conflicts.len();
    e.uncertain_rod_capsule_pairs = uncertain.len();
    e.distinct_rods = conflicts
        .iter()
        .map(|r| r.rod_index)
        .collect::<BTreeSet<_>>()
        .len();
    e.distinct_dynamic_nodes = conflicts
        .iter()
        .map(|r| r.dynamic_node)
        .collect::<BTreeSet<_>>()
        .len();
    e.distinct_anchors = conflicts
        .iter()
        .map(|r| r.static_node)
        .collect::<BTreeSet<_>>()
        .len();
    for row in &conflicts {
        *e.by_capsule.entry(row.capsule_control.clone()).or_default() += 1;
    }
    e.worst_examples = conflicts.iter().take(20).cloned().collect();
    e.uncertain_examples = uncertain.iter().take(20).cloned().collect();
    if !conflicts.is_empty() {
        if mixed {
            return Err(pending("all-colliding superset contains possible conflicts; independent solver-node/leaf map required"));
        }
        return Err(bad("static-anchor rod maxima and capsule exclusion cannot both be satisfied at declared rest pose"));
    }
    if !uncertain.is_empty() {
        return Err(pending(
            "transform or arithmetic uncertainty straddles rest-constraint threshold",
        ));
    }
    Ok(())
}
