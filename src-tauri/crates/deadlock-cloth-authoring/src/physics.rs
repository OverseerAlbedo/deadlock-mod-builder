//! Read-only hash-bound garment-chain and leg-capsule proposals, never solver authority.
use crate::{hash, invalid, Result};
use deadlock_core::{safety, CancellationToken};
use deadlock_dmx::Dmx;
use deadlock_resource::{binary, container, Kind, Value};
use deadlock_skeleton::{
    check_transfer,
    source_identity::{source_contract, SourceContract},
    source_skeleton::SourceAuditReport,
    transforms::{self, BindState, Matrix4},
    Skeleton,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};
pub const POLICY: &str = "skirt-collision-analysis-v1";
const CHUNK: usize = 65536;
const ROLES: [&str; 7] = [
    "pelvis", "thigh_L", "shin_L", "foot_L", "thigh_R", "shin_R", "foot_R",
];
#[derive(Debug, Clone)]
pub struct OriginalGeometry {
    pub physics: Option<Value>,
    pub state: BindState,
    pub diagnostics: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capsule {
    pub endpoints: [[f64; 3]; 2],
    pub radius: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct AnalysisOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roots: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub roles: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collision_meshes: Option<Vec<String>>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainProposal {
    pub root: String,
    pub anchor: String,
    pub bones: Vec<String>,
    pub pinned_bones: Vec<String>,
    pub simulated_bones: Vec<String>,
    pub weighted_bones: Vec<String>,
    pub world_positions: Vec<[f64; 3]>,
    pub segment_lengths: Vec<f64>,
    pub terminal_orientation_verified: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColliderProposal {
    pub bone: String,
    pub tip_bone: String,
    pub origin: String,
    pub endpoints_local: [[f64; 3]; 2],
    pub radius_local: f64,
    pub endpoints_world: [[f64; 3]; 2],
    pub radius_world: f64,
    pub fit_position_count: Option<usize>,
    pub fit_approved: bool,
    pub overlapping_position_count: usize,
    pub max_penetration: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Summary {
    pub chain_count: usize,
    pub custom_bone_count: usize,
    pub pinned_bone_count: usize,
    pub leg_collider_count: usize,
    pub overlapping_position_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub garment_position_count: Option<usize>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhysicsAnalysis {
    pub policy: String,
    pub source_sha256: String,
    pub original_sha256: Option<String>,
    pub units: String,
    pub millimeters_per_unit: f64,
    pub runtime_ready: bool,
    pub ready_for_authoring: bool,
    pub setup_ready_for_review: bool,
    pub source_modified: bool,
    pub weights_modified: bool,
    pub solver_written: bool,
    pub fit_approved: bool,
    pub pending_gates: Vec<String>,
    pub blockers: Vec<String>,
    pub warnings: Vec<String>,
    pub limitations: Vec<String>,
    pub chains: Vec<ChainProposal>,
    pub leg_colliders: Vec<ColliderProposal>,
    pub roles: BTreeMap<String, String>,
    pub summary: Summary,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_snapshot_verified: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role_candidates: Option<BTreeMap<String, Vec<String>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unselected_required_extras: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collision_meshes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<AnalysisOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analysis_sha256: Option<String>,
}
fn field<'a>(value: &'a Value, key: &str) -> Result<&'a Value> {
    value
        .get(key)
        .ok_or_else(|| invalid(format!("Original resource is missing {key}")))
}
fn number(value: &Value) -> Option<f64> {
    let n = match value.kind {
        Kind::I64(v) => v as f64,
        Kind::U64(v) => v as f64,
        Kind::F32(v) => v as f64,
        Kind::F64(v) => v,
        _ => return None,
    };
    n.is_finite().then_some(n)
}
fn vector<const N: usize>(value: &Value) -> Option<[f64; N]> {
    let values = value.array().ok()?;
    if values.len() != N {
        return None;
    }
    let mut result = [0.0; N];
    for (i, v) in values.iter().enumerate() {
        result[i] = number(v)?;
    }
    Some(result)
}
pub fn original(
    snapshot: &Path,
    expected_sha: &str,
    audit: &SourceAuditReport,
    cancel: &CancellationToken,
) -> Result<OriginalGeometry> {
    if expected_sha.len() != 64
        || !expected_sha
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(invalid(
            "Skirt analysis needs an audited original compiled-model SHA256.",
        ));
    }
    if !snapshot.is_file() || fs::metadata(snapshot)?.len() > 512 * 1024 * 1024 {
        return Err(invalid(
            "Skirt analysis original compiled snapshot is missing or too large.",
        ));
    }
    cancel.check()?;
    let raw = fs::read(snapshot)?;
    if hash(&raw) != expected_sha {
        return Err(invalid(
            "Skirt analysis original compiled snapshot changed; analyze the hero again.",
        ));
    }
    let layout = container::layout(&raw)?;
    let data = binary::decode(container::payload(
        &raw,
        container::unique(&layout.blocks, "DATA")?,
    )?)?
    .root;
    let blocks: Vec<_> = layout.blocks.iter().filter(|b| b.name == "PHYS").collect();
    let mut diagnostics = vec![];
    let physics = if blocks.len() == 1 {
        Some(binary::decode(container::payload(&raw, blocks[0])?)?.root)
    } else {
        diagnostics.push(format!("Original model has {} PHYS blocks; no unambiguous ragdoll capsules were selected. Collision proposals use source geometry where available.",blocks.len()));
        None
    };
    if audit.runtime_motion.resource_sha256.as_deref() != Some(expected_sha)
        || audit.compiled_sha256.as_deref() != Some(expected_sha)
    {
        return Err(invalid(
            "Skirt analysis original audit digest does not match the compiled snapshot.",
        ));
    }
    let reference = audit
        .runtime_motion
        .skeleton
        .as_ref()
        .ok_or_else(|| invalid("Skirt analysis original audit skeleton is missing."))?;
    let skeleton = Skeleton::from_value(field(&data, "m_modelSkeleton")?)?;
    check_transfer(&skeleton, reference)?;
    transforms::audit_scale_preservation(&skeleton, reference, false)?;
    if skeleton.flags != reference.flags {
        return Err(invalid(
            "Skirt analysis original audit flags differ from the compiled snapshot.",
        ));
    }
    cancel.check()?;
    Ok(OriginalGeometry {
        physics,
        state: transforms::bind_state(&skeleton)?,
        diagnostics,
    })
}
pub fn distance(point: [f64; 3], a: [f64; 3], b: [f64; 3]) -> Result<f64> {
    let delta = sub(b, a);
    let length2 = dot(delta, delta);
    if !length2.is_finite() || length2 <= 1e-12 {
        return Err(invalid(
            "Skirt collider has a zero or invalid segment length.",
        ));
    }
    let t = (dot(sub(point, a), delta) / length2).clamp(0.0, 1.0);
    Ok(norm(sub(
        point,
        std::array::from_fn(|i| a[i] + t * delta[i]),
    )))
}
fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| a[i] - b[i])
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.iter().zip(b).map(|(a, b)| a * b).sum()
}
fn norm(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}
fn transform(matrix: &Matrix4, point: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| matrix[i][3] + (0..3).map(|j| matrix[i][j] * point[j]).sum::<f64>())
}
pub fn ragdoll_capsules(original: Option<&OriginalGeometry>, bone: &str) -> Vec<Capsule> {
    fn checked(original: Option<&OriginalGeometry>, bone: &str) -> Option<Vec<Capsule>> {
        let original = original?;
        let physics = original.physics.as_ref()?;
        let names = physics
            .get("m_boneNames")?
            .array()
            .ok()?
            .iter()
            .map(|v| v.string().ok())
            .collect::<Option<Vec<_>>>()?;
        let parts = physics.get("m_parts")?.array().ok()?;
        let binds = physics.get("m_bindPose")?.array().ok()?;
        if names.iter().copied().collect::<BTreeSet<_>>().len() != names.len()
            || parts.len() != names.len()
            || binds.len() != names.len()
        {
            return None;
        }
        let i = names.iter().position(|n| *n == bone)?;
        let bind = vector::<12>(&binds[i])?;
        let world = &original.state.world[original
            .state
            .skeleton
            .names
            .iter()
            .position(|n| n == bone)?];
        for row in 0..3 {
            for col in 0..4 {
                let max = if col == 3 { 0.001 } else { 0.0001 };
                if (bind[4 * row + col] - world[row][col]).abs() > max {
                    return None;
                }
            }
        }
        let capsules = parts[i].get("m_rnShape")?.get("m_capsules")?.array().ok()?;
        if !(1..=8).contains(&capsules.len()) {
            return None;
        }
        let mut result = vec![];
        for item in capsules {
            let capsule = item.get("m_Capsule")?;
            let points = capsule.get("m_vCenter")?.array().ok()?;
            if points.len() != 2 {
                return None;
            }
            let endpoints = [vector::<3>(&points[0])?, vector::<3>(&points[1])?];
            let radius = number(capsule.get("m_flRadius")?)?;
            if radius <= 0.0 || norm(sub(endpoints[0], endpoints[1])) <= 1e-6 {
                return None;
            }
            result.push(Capsule { endpoints, radius });
        }
        Some(result)
    }
    checked(original, bone).unwrap_or_default()
}
#[derive(Debug)]
struct Stream {
    name: String,
    points: Vec<[f64; 3]>,
    weights: Vec<Vec<f64>>,
    joints: Vec<Vec<usize>>,
    drawn: Vec<bool>,
}
fn drawn(
    d: &Dmx,
    mesh: &deadlock_dmx::Element,
    state: &deadlock_dmx::Element,
    count: usize,
) -> Result<Vec<bool>> {
    let key = if state.get("position$0Indices").is_some() {
        "position$0Indices"
    } else {
        "positionsIndices"
    };
    let corners = crate::surface::ints(d, state, key, 34)?;
    if corners.is_empty() {
        return Err(invalid(
            "Skirt analysis cannot establish referenced garment positions.",
        ));
    }
    let mut drawn = vec![false; count];
    for face in crate::surface::references(d, mesh, "faceSets")? {
        let face = &d.elements[face];
        if face.kind != "DmeFaceSet" {
            return Err(invalid("Invalid skirt source face set."));
        }
        let mut polygon = vec![];
        for corner in crate::surface::ints(d, face, "faces", 34)? {
            if corner == -1 {
                if polygon.iter().copied().collect::<BTreeSet<usize>>().len() >= 3 {
                    for position in &polygon {
                        drawn[*position] = true;
                    }
                }
                polygon.clear();
            } else {
                let corner =
                    usize::try_from(corner).map_err(|_| invalid("Invalid skirt source corner."))?;
                let position = corners
                    .get(corner)
                    .and_then(|p| usize::try_from(*p).ok())
                    .filter(|p| *p < count)
                    .ok_or_else(|| invalid("Skirt source position index is out of bounds."))?;
                polygon.push(position);
            }
        }
        if !polygon.is_empty() {
            return Err(invalid("Skirt source face is unterminated."));
        }
    }
    Ok(drawn)
}
fn streams(
    d: &Dmx,
    selected: &BTreeSet<String>,
    palette_count: usize,
    cancel: &CancellationToken,
) -> Result<Vec<Stream>> {
    let mut result = vec![];
    for mesh in d
        .elements
        .iter()
        .filter(|m| m.kind == "DmeMesh" && selected.contains(&m.name))
    {
        cancel.check()?;
        let key = if mesh.get("bindState").is_some() {
            "bindState"
        } else {
            "currentState"
        };
        let state = &d.elements[crate::surface::reference(d, mesh, key, "DmeVertexData")?];
        let position = if state.get("position$0").is_some() {
            "position$0"
        } else {
            "positions"
        };
        let weight = if state.get("blendweights$0").is_some() {
            "blendweights$0"
        } else {
            "jointWeights"
        };
        let joint = if state.get("blendindices$0").is_some() {
            "blendindices$0"
        } else {
            "jointIndices"
        };
        let points = crate::surface::floats(d, state, position, 42, 3)?
            .chunks_exact(3)
            .map(|p| [p[0], p[1], p[2]])
            .collect::<Vec<_>>();
        let width = match state.get("jointCount") {
            Some(deadlock_dmx::Value::Integer(v)) if (1..=64).contains(v) => *v as usize,
            _ => {
                return Err(invalid(
                    "Skirt analysis requires a valid position-aligned skin influence stream.",
                ))
            }
        };
        let weights = crate::surface::floats(d, state, weight, 35, 1)?;
        let joints = crate::surface::ints(d, state, joint, 34)?;
        if points.is_empty()
            || points.iter().flatten().any(|n| !n.is_finite())
            || weights.len() != points.len() * width
            || joints.len() != weights.len()
            || joints
                .iter()
                .any(|n| *n < 0 || *n as usize >= palette_count)
            || weights.iter().any(|n| !n.is_finite() || *n < 0.0)
            || weights
                .chunks_exact(width)
                .any(|w| (w.iter().map(|v| *v as f32).sum::<f32>() as f64 - 1.0).abs() > 0.00201)
        {
            return Err(invalid(
                "Skirt analysis encountered invalid or unweighted source positions.",
            ));
        }
        let mask = drawn(d, mesh, state, points.len())?;
        result.push(Stream {
            name: mesh.name.clone(),
            points,
            weights: weights.chunks_exact(width).map(|s| s.to_vec()).collect(),
            joints: joints
                .chunks_exact(width)
                .map(|s| s.iter().map(|j| *j as usize).collect())
                .collect(),
            drawn: mask,
        });
    }
    Ok(result)
}
fn quantile95(mut samples: Vec<f64>) -> f64 {
    samples.sort_by(f64::total_cmp);
    let index = (samples.len() - 1) as f64 * 0.95;
    let lower = index.floor() as usize;
    let upper = index.ceil() as usize;
    samples[lower] + (samples[upper] - samples[lower]) * (index - lower as f64)
}
pub fn analyze(
    dmx: &Dmx,
    report: &SourceAuditReport,
    snapshot: Option<&Path>,
    extension: Option<&SourceContract>,
    options: &AnalysisOptions,
    cancel: &CancellationToken,
) -> Result<PhysicsAnalysis> {
    cancel.check()?;
    let before = hash(dmx.bytes());
    let mut result=PhysicsAnalysis{policy:POLICY.into(),source_sha256:before.clone(),original_sha256:report.compiled_sha256.clone(),units:"Source 2 model inches".into(),millimeters_per_unit:25.4,runtime_ready:false,ready_for_authoring:false,setup_ready_for_review:false,source_modified:false,weights_modified:false,solver_written:false,fit_approved:false,pending_gates:vec!["Validate selected garment surface, fixed boundary and direct proxy binding.".into(),"Native compiler validation of this garment and its animated leg colliders.".into(),"Independent original hair-solver preservation and in-game motion validation.".into()],blockers:vec![],warnings:vec![],limitations:vec!["Current source-contract discovery requires an original with audited runtime cloth; originals without runtime cloth are not supported by this analyzer yet.".into()],chains:vec![],leg_colliders:vec![],roles:BTreeMap::new(),summary:Summary::default(),original_snapshot_verified:None,role_candidates:None,unselected_required_extras:None,collision_meshes:None,options:None,analysis_sha256:None};
    if report.runtime_motion.cloth_nodes == 0 {
        result.limitations = vec!["An original without runtime cloth can provide a verified skeleton for source-only geometry proposals. This does not establish a new native solver; compiler and in-game validation remain required.".into()];
    }
    let Some(extension) = extension else {
        result.blockers.push("No validated selected weighted custom-bone extension; analyze a rigged DMX with garment bones.".into());
        return Ok(result);
    };
    let selected: BTreeSet<_> = extension.selected_meshes.iter().cloned().collect();
    let fresh = source_contract(dmx, report, &selected, &before)?
        .ok_or_else(|| invalid("Skirt analysis source binding contract is stale or changed."))?;
    if fresh != *extension {
        return Err(invalid(
            "Skirt analysis source binding contract is stale or changed.",
        ));
    }
    let original = if let Some(snapshot) = snapshot {
        Some(original(
            snapshot,
            report.compiled_sha256.as_deref().unwrap_or_default(),
            report,
            cancel,
        )?)
    } else {
        None
    };
    result.original_snapshot_verified = Some(original.is_some());
    if let Some(original) = &original {
        result.warnings.extend(original.diagnostics.clone());
    } else {
        result.blockers.push(
            "Original compiled snapshot is unavailable; re-analyze the selected hero.".into(),
        );
    }
    let state = transforms::bind_state(&fresh.skeleton)?;
    let names = &state.skeleton.names;
    let parents = &state.skeleton.parents;
    let by_name: BTreeMap<_, _> = names
        .iter()
        .enumerate()
        .map(|(i, n)| (n.as_str(), i))
        .collect();
    let extras: BTreeSet<_> = fresh.extra_names.iter().cloned().collect();
    let needed: BTreeSet<_> = fresh.required_extra_names.iter().cloned().collect();
    let weighted: BTreeSet<_> = fresh
        .weighted_extra_bones
        .iter()
        .flat_map(|r| r.bones.iter())
        .map(|n| fresh.source_aliases.get(n).unwrap_or(n).clone())
        .collect();
    let original_names: BTreeSet<_> = report
        .runtime_motion
        .skeleton
        .as_ref()
        .ok_or_else(|| invalid("Original runtime skeleton missing."))?
        .names
        .iter()
        .cloned()
        .collect();
    let mut candidates: BTreeMap<String, Vec<String>> = ROLES
        .iter()
        .map(|role| (role.to_string(), vec![]))
        .collect();
    for name in names {
        if let Some(role) = deadlock_preparation::algorithms::name_role(name) {
            if original_names.contains(name) {
                if let Some(values) = candidates.get_mut(role) {
                    values.push(name.clone());
                }
            }
        }
    }
    for (role, name) in &options.roles {
        if !candidates.contains_key(role) {
            return Err(invalid("Invalid skirt collision landmark overrides."));
        }
        if !original_names.contains(name) {
            return Err(invalid(format!(
                "Skirt collision landmarks must name original hero bones: {role}"
            )));
        }
        candidates.insert(role.clone(), vec![name.clone()]);
    }
    for role in ROLES {
        let values = &candidates[role];
        if values.len() == 1 {
            result.roles.insert(role.into(), values[0].clone());
        } else {
            result.blockers.push(format!(
                "Assign an unambiguous {role} bone; candidates: {}",
                if values.is_empty() {
                    "none".into()
                } else {
                    values.join(", ")
                }
            ));
        }
    }
    result.role_candidates = Some(candidates);
    let position = |name: &str| {
        let m = &state.world[by_name[name]];
        [m[0][3], m[1][3], m[2][3]]
    };
    let ancestor = |child: &str, parent: &str| {
        let mut at = by_name[child] as i32;
        while at >= 0 {
            if names[at as usize] == parent {
                return true;
            }
            at = parents[at as usize];
        }
        false
    };
    let mut roots: Vec<String> = names
        .iter()
        .filter(|name| {
            needed.contains(*name)
                && (parents[by_name[name.as_str()]] < 0
                    || !extras.contains(&names[parents[by_name[name.as_str()]] as usize]))
        })
        .cloned()
        .collect();
    if let Some(explicit) = &options.roots {
        let chosen: BTreeSet<_> = explicit.iter().collect();
        if chosen.len() != explicit.len() || !chosen.is_subset(&roots.iter().collect()) {
            return Err(invalid(
                "Skirt chain selection must use validated weighted extra-branch roots.",
            ));
        }
        roots = explicit.clone();
    }
    let pelvis = result.roles.get("pelvis").cloned();
    for root in roots {
        cancel.check()?;
        let mut chain = vec![root.clone()];
        let mut at = root.as_str();
        loop {
            let children: Vec<_> = names
                .iter()
                .filter(|name| {
                    needed.contains(*name) && parents[by_name[name.as_str()]] == by_name[at] as i32
                })
                .collect();
            if children.is_empty() {
                break;
            }
            if children.len() != 1 {
                result
                    .blockers
                    .push(format!("Extra branch is not a linear skirt chain: {root}"));
                chain.clear();
                break;
            }
            at = children[0];
            chain.push(at.into());
        }
        if chain.is_empty() {
            continue;
        }
        let parent = parents[by_name[root.as_str()]];
        if parent < 0 || pelvis.as_ref() != Some(&names[parent as usize]) {
            result.blockers.push(format!(
                "Skirt branch must have an explicitly resolved pelvis anchor: {root}"
            ));
            continue;
        }
        let points: Vec<_> = chain.iter().map(|n| position(n)).collect();
        let lengths: Vec<_> = points.windows(2).map(|p| norm(sub(p[1], p[0]))).collect();
        if chain.len() < 3
            || lengths.iter().any(|n| *n <= 1e-6)
            || points.windows(2).any(|p| p[1][2] - p[0][2] >= -1e-6)
        {
            result.blockers.push(format!(
                "Skirt chain needs at least two nonzero downward links: {root}"
            ));
            continue;
        }
        result.chains.push(ChainProposal {
            root: root.clone(),
            anchor: names[parent as usize].clone(),
            pinned_bones: vec![root],
            simulated_bones: chain[1..].to_vec(),
            weighted_bones: chain
                .iter()
                .filter(|n| weighted.contains(*n))
                .cloned()
                .collect(),
            bones: chain,
            world_positions: points,
            segment_lengths: lengths,
            terminal_orientation_verified: false,
        });
    }
    if result.chains.is_empty() {
        result
            .blockers
            .push("No suitable selected downward garment chains were found.".into());
    }
    let selected_bones: BTreeSet<_> = result
        .chains
        .iter()
        .flat_map(|c| c.bones.iter().cloned())
        .collect();
    let unselected: Vec<_> = needed.difference(&selected_bones).cloned().collect();
    if !unselected.is_empty() {
        if options.roots.is_none() {
            result.blockers.push("Extra branches could not all be identified as skirt chains; select roots explicitly.".into());
        } else {
            result.warnings.push("Unselected custom bones remain ordinary skinned bones; their weights are preserved.".into());
        }
    }
    result.unselected_required_extras = Some(unselected);
    if result.roles.len() == ROLES.len() {
        let pelvis = pelvis.as_ref().unwrap();
        for side in ["L", "R"] {
            let roles: Vec<_> = ["thigh", "shin", "foot"]
                .iter()
                .map(|role| result.roles[&format!("{role}_{side}")].as_str())
                .collect();
            if !ancestor(roles[0], pelvis)
                || !ancestor(roles[1], roles[0])
                || !ancestor(roles[2], roles[1])
                || !(position(roles[0])[2] > position(roles[1])[2]
                    && position(roles[1])[2] > position(roles[2])[2])
            {
                result.blockers.push(format!(
                    "Leg landmarks do not form a downward pelvis/thigh/shin/foot hierarchy: {side}"
                ));
            }
        }
        let left = position(&result.roles["thigh_L"]);
        let right = position(&result.roles["thigh_R"]);
        if ((left[0] - right[0]).powi(2) + (left[1] - right[1]).powi(2)).sqrt() <= 1e-6 {
            result
                .blockers
                .push("Left and right thigh landmarks are not spatially separated.".into());
        }
    }
    let collision_meshes = options
        .collision_meshes
        .clone()
        .unwrap_or_else(|| selected.iter().cloned().collect());
    let collision_set: BTreeSet<_> = collision_meshes.iter().cloned().collect();
    if collision_set.len() != collision_meshes.len() || !collision_set.is_subset(&selected) {
        return Err(invalid(
            "Collision-body meshes must be selected source meshes.",
        ));
    }
    result.collision_meshes = Some(collision_meshes);
    let model = dmx
        .elements
        .iter()
        .find(|e| e.kind == "DmeModel" && e.get("jointList").is_some_and(|v| !v.is_empty()))
        .ok_or_else(|| invalid("Skirt source joint palette is missing."))?;
    let palette_names: Vec<_> = crate::surface::references(dmx, model, "jointList")?
        .into_iter()
        .map(|i| {
            let name = &dmx.elements[i].name;
            fresh.source_aliases.get(name).unwrap_or(name).clone()
        })
        .collect();
    let skirt_slots: BTreeSet<_> = palette_names
        .iter()
        .enumerate()
        .filter(|(_, n)| selected_bones.contains(*n))
        .map(|(i, _)| i)
        .collect();
    let extra_slots: BTreeSet<_> = palette_names
        .iter()
        .enumerate()
        .filter(|(_, n)| extras.contains(*n))
        .map(|(i, _)| i)
        .collect();
    let streams = streams(dmx, &selected, palette_names.len(), cancel)?;
    let legs_valid = result.roles.len() == ROLES.len()
        && !result
            .blockers
            .iter()
            .any(|s| s.starts_with("Leg landmarks") || s.starts_with("Left and right"));
    if legs_valid {
        for side in ["L", "R"] {
            for (start_role, end_role) in [("thigh", "shin"), ("shin", "foot")] {
                cancel.check()?;
                let bone = &result.roles[&format!("{start_role}_{side}")];
                let tip = &result.roles[&format!("{end_role}_{side}")];
                let index = by_name[bone.as_str()];
                let mut proposals = ragdoll_capsules(original.as_ref(), bone);
                let mut origin = "original_ragdoll_capsule";
                let mut fit_count = None;
                if proposals.is_empty() {
                    let (a, b) = (position(bone), position(tip));
                    let delta = sub(b, a);
                    let length2 = dot(delta, delta);
                    let slot = palette_names
                        .iter()
                        .position(|n| n == bone)
                        .ok_or_else(|| invalid("Leg landmark absent from palette."))?;
                    let mut samples = vec![];
                    let mut count = 0usize;
                    for stream in &streams {
                        if !collision_set.contains(&stream.name) {
                            continue;
                        }
                        let stride = stream.points.len().div_ceil(CHUNK).max(1);
                        for i in 0..stream.points.len() {
                            if i % CHUNK == 0 {
                                cancel.check()?;
                            }
                            let (p, w, j) =
                                (stream.points[i], &stream.weights[i], &stream.joints[i]);
                            let dominant = w
                                .iter()
                                .zip(j)
                                .filter(|(_, j)| **j == slot)
                                .map(|(w, _)| *w as f32)
                                .sum::<f32>()
                                >= 0.5;
                            let custom = w
                                .iter()
                                .zip(j)
                                .any(|(w, j)| *w > 0.0 && extra_slots.contains(j));
                            let t = dot(sub(p, a), delta) / length2;
                            if dominant && !custom && (0.1..=0.9).contains(&t) && stream.drawn[i] {
                                count += 1;
                                if i % stride == 0 {
                                    samples.push(distance(p, a, b)?);
                                }
                            }
                        }
                    }
                    if samples.is_empty() {
                        result.blockers.push(format!(
                            "No reliable dominant-weight collision-body geometry for {bone}"
                        ));
                        continue;
                    }
                    let radius = quantile95(samples);
                    if !radius.is_finite() || radius <= 1e-6 {
                        result.blockers.push(format!(
                            "Collision fit has a zero or invalid radius for {bone}"
                        ));
                        continue;
                    }
                    proposals.push(Capsule {
                        endpoints: [
                            transform(&state.inverse[index], a),
                            transform(&state.inverse[index], b),
                        ],
                        radius: radius / state.accumulated[index],
                    });
                    fit_count = Some(count);
                    origin = "dominant_weight_geometry_p95_proposal";
                }
                for proposal in proposals {
                    result.leg_colliders.push(ColliderProposal {
                        bone: bone.clone(),
                        tip_bone: tip.clone(),
                        origin: origin.into(),
                        endpoints_world: proposal
                            .endpoints
                            .map(|p| transform(&state.world[index], p)),
                        radius_world: proposal.radius * state.accumulated[index],
                        endpoints_local: proposal.endpoints,
                        radius_local: proposal.radius,
                        fit_position_count: fit_count,
                        fit_approved: false,
                        overlapping_position_count: 0,
                        max_penetration: 0.0,
                    });
                }
            }
        }
    }
    let (mut overlap_total, mut garment_count) = (0, 0);
    for stream in &streams {
        for i in 0..stream.points.len() {
            if i % CHUNK == 0 {
                cancel.check()?;
            }
            if !stream.drawn[i]
                || !stream.weights[i]
                    .iter()
                    .zip(&stream.joints[i])
                    .any(|(w, j)| *w > 0.0 && skirt_slots.contains(j))
            {
                continue;
            }
            garment_count += 1;
            let mut overlap = false;
            for collider in &mut result.leg_colliders {
                let penetration = collider.radius_world
                    - distance(
                        stream.points[i],
                        collider.endpoints_world[0],
                        collider.endpoints_world[1],
                    )?;
                if penetration > 0.0 {
                    collider.overlapping_position_count += 1;
                    overlap = true;
                }
                collider.max_penetration = collider.max_penetration.max(penetration);
            }
            if overlap {
                overlap_total += 1;
            }
        }
    }
    result.summary = Summary {
        chain_count: result.chains.len(),
        custom_bone_count: selected_bones.len(),
        pinned_bone_count: result.chains.len(),
        leg_collider_count: result.leg_colliders.len(),
        overlapping_position_count: overlap_total,
        garment_position_count: Some(garment_count),
    };
    if overlap_total > 0 {
        result.blockers.push(format!("Proposed leg capsules overlap {overlap_total} garment positions at rest; fit review is required."));
    }
    result.warnings.extend(["Collider dimensions are proposals; ragdoll shapes are not automatically suitable for cloth.","Bind-pose overlaps inspect garment positions, not continuous triangles or animated motion.","Skirt bone chains alone do not establish cross-panel cloth constraints or terminal rotations."].map(str::to_string));
    if hash(dmx.bytes()) != before {
        return Err(invalid("DMX bytes changed during skirt physics analysis."));
    }
    if let Some(snapshot) = snapshot {
        if safety::digest_with_cancel(snapshot, cancel)?
            != report.compiled_sha256.as_deref().unwrap_or_default()
        {
            return Err(invalid(
                "Original model changed during skirt physics analysis.",
            ));
        }
    }
    result.setup_ready_for_review = result.blockers.is_empty();
    result.options = Some(options.clone());
    let mut value = serde_json::to_value(&result)?;
    value.sort_all_objects();
    result.analysis_sha256 = Some(hash(&serde_json::to_vec(&value)?));
    Ok(result)
}
