//! Typed, read-only cloth proof prerequisites. These diagnostics alone never
//! authorize physics transfer, packaging, or installation.
use crate::{invalid, Annotation, Kind, Result, Value};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap, HashSet};

pub const MOTION_MASK: i128 = 0x8 | 0x400000;
pub const BALANCED_REFERENCE_SHA256: &str =
    "27dd44ea4db29f671cff53d0dd8ad171792fe5f12f45b05aca5db65e0a6b5b0c";
pub(crate) fn field<'a>(v: &'a Value, key: &str) -> Result<&'a Value> {
    v.get(key)
        .ok_or_else(|| invalid(format!("missing field {key}")))
}
pub(crate) fn integer(v: &Value) -> Result<i128> {
    v.integer()
}
pub(crate) fn number(v: &Value) -> Result<f64> {
    let n = match v.kind {
        Kind::I64(n) => n as f64,
        Kind::U64(n) => n as f64,
        Kind::F32(n) => n as f64,
        Kind::F64(n) => n,
        _ => return Err(invalid("expected finite number")),
    };
    if !n.is_finite() {
        return Err(invalid("non-finite number"));
    }
    Ok(n)
}
pub(crate) fn integers(v: &Value) -> Result<Vec<i128>> {
    v.array()?.iter().map(integer).collect()
}
pub(crate) fn numbers(v: &Value) -> Result<Vec<f64>> {
    v.array()?.iter().map(number).collect()
}
pub(crate) fn array<'a>(v: &'a Value, key: &str) -> Result<&'a [Value]> {
    field(v, key)?.array()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Passed,
    Failed,
    Pending,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StructureProof {
    pub node_count: usize,
    pub static_nodes: usize,
    pub collision_tree_nodes: usize,
    pub leaf_to_solver_node_mapping_assumed: bool,
}
pub fn self_contained(value: &Value) -> Result<()> {
    if value.annotation != Annotation::Unspecified {
        return Err(invalid(
            "cloth solver contains external resource annotations",
        ));
    }
    match &value.kind {
        Kind::Array(v) => {
            for x in v {
                self_contained(x)?;
            }
        }
        Kind::Object(v) => {
            for (_, x) in v {
                self_contained(x)?;
            }
        }
        _ => (),
    }
    Ok(())
}
pub fn structure(physics: &Value) -> Result<StructureProof> {
    let fe = field(physics, "m_pFeModel")?;
    fe.object()?;
    let count = integer(field(fe, "m_nNodeCount")?)?;
    let static_ = integer(field(fe, "m_nStaticNodes")?)?;
    let locked = integer(field(fe, "m_nRotLockStaticNodes")?)?;
    if !(0 <= locked
        && locked <= static_
        && static_ < count
        && count <= 65535
        && count - static_ <= 32768)
    {
        return Err(invalid(
            "invalid cloth static/dynamic counts or uint16 collision-tree capacity",
        ));
    }
    let count = count as usize;
    let static_ = static_ as usize;
    for key in [
        "m_CtrlName",
        "m_CtrlHash",
        "m_SkelParents",
        "m_NodeInvMasses",
        "m_NodeIntegrator",
        "m_InitPose",
    ] {
        if array(fe, key)?.len() != count {
            return Err(invalid(format!("incomplete per-node data: {key}")));
        }
    }
    for (index, mass) in array(fe, "m_NodeInvMasses")?.iter().enumerate() {
        let mass = number(mass)?;
        if (index < static_ && mass != 0.0) || (index >= static_ && mass <= 0.0) {
            return Err(invalid(
                "inverse masses disagree with static/dynamic partition",
            ));
        }
    }
    for parent in array(fe, "m_SkelParents")? {
        let p = integer(parent)?;
        if p < -1 || p >= count as i128 {
            return Err(invalid("invalid control parent"));
        }
    }
    let weights = array(fe, "m_FitWeights")?;
    let matrices = array(fe, "m_FitMatrices")?;
    let mut begin = 0;
    for matrix in matrices {
        let node = integer(field(matrix, "nNode")?)?;
        let end = integer(field(matrix, "nEnd")?)?;
        let dynamic = integer(field(matrix, "nBeginDynamic")?)?;
        if !(0 <= node
            && node < count as i128
            && begin <= dynamic
            && dynamic <= end
            && end <= weights.len() as i128)
        {
            return Err(invalid("invalid fit node or weight offsets"));
        }
        begin = end;
    }
    if begin != weights.len() as i128 {
        return Err(invalid("unbound fit weights"));
    }
    for row in weights {
        let node = integer(field(row, "nNode")?)?;
        if !(0..count as i128).contains(&node) {
            return Err(invalid("invalid fit-weight node"));
        }
    }
    let leaves = count - static_;
    let size = 2 * leaves - 1;
    let parents = integers(field(fe, "m_TreeParents")?)?;
    let masks = integers(field(fe, "m_TreeCollisionMasks")?)?;
    let children = array(fe, "m_TreeChildren")?;
    if parents.len() != size
        || masks.len() != size
        || children.len() != leaves - 1
        || parents.last() != Some(&65535)
    {
        return Err(invalid("incomplete collision tree"));
    }
    let mut seen = HashSet::new();
    for (index, row) in children.iter().enumerate() {
        let at = index + leaves;
        let nodes = integers(field(row, "nChild")?)?;
        if nodes.len() != 2 || nodes[0] == nodes[1] {
            return Err(invalid("invalid collision-tree child pair"));
        }
        for n in nodes {
            if n < 0 || n >= at as i128 || !seen.insert(n) || parents[n as usize] != at as i128 {
                return Err(invalid(
                    "cyclic, duplicated or inconsistent collision-tree index",
                ));
            }
        }
    }
    if seen.len() != size - 1 || masks.iter().any(|n| !(0..=65535).contains(n)) {
        return Err(invalid("collision-tree coverage/masks invalid"));
    }
    Ok(StructureProof {
        node_count: count,
        static_nodes: static_,
        collision_tree_nodes: size,
        leaf_to_solver_node_mapping_assumed: false,
    })
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OffsetRelation {
    pub control: String,
    pub parent: String,
    pub relation: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DerivedControl {
    pub control: String,
    pub anchors: Vec<String>,
    pub offset_relations: Vec<OffsetRelation>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlBindings {
    pub skeleton_controls: Vec<String>,
    pub derived_controls: Vec<DerivedControl>,
}
pub fn control_bindings(
    controls: &[String],
    names: &[String],
    solver: &Value,
) -> Result<ControlBindings> {
    let skeleton: HashSet<_> = names.iter().collect();
    let direct: BTreeSet<_> = controls
        .iter()
        .filter(|v| skeleton.contains(v))
        .cloned()
        .collect();
    let mut parents: HashMap<usize, Vec<(usize, String)>> = HashMap::new();
    let mut parent_order = Vec::new();
    for key in ["m_CtrlOffsets", "m_CtrlOsOffsets", "m_CtrlSoftOffsets"] {
        let Some(relations) = solver.get(key) else {
            continue;
        };
        for row in relations.array()? {
            let p = integer(field(row, "nCtrlParent")?)?;
            let c = integer(field(row, "nCtrlChild")?)?;
            if p < 0 || c < 0 || p >= controls.len() as i128 || c >= controls.len() as i128 {
                return Err(invalid("out-of-range authored control offset"));
            }
            if key != "m_CtrlOsOffsets" && numbers(field(row, "vOffset")?)?.len() != 3 {
                return Err(invalid("invalid three-component control offset"));
            }
            if key == "m_CtrlSoftOffsets" {
                number(field(row, "flAlpha")?)?;
            }
            let c = c as usize;
            if !parents.contains_key(&c) {
                parent_order.push(c);
            }
            parents.entry(c).or_default().push((p as usize, key.into()));
        }
    }
    let mut visited = HashSet::new();
    for start in parent_order {
        let mut stack = vec![(start, false)];
        let mut active = HashSet::new();
        while let Some((at, leaving)) = stack.pop() {
            if leaving {
                active.remove(&at);
                visited.insert(at);
            } else if active.contains(&at) {
                return Err(invalid(format!(
                    "cycle in authored control offsets: {}",
                    controls[at]
                )));
            } else if !visited.contains(&at) {
                active.insert(at);
                stack.push((at, true));
                if let Some(rows) = parents.get(&at) {
                    stack.extend(rows.iter().map(|(p, _)| (*p, false)));
                }
            }
        }
    }
    let mut derived = Vec::new();
    for (i, name) in controls.iter().enumerate() {
        if direct.contains(name) {
            continue;
        }
        let mut pending = vec![i];
        let mut seen = HashSet::new();
        let mut anchors = BTreeSet::new();
        let mut relations = Vec::new();
        let mut unbound = false;
        while let Some(at) = pending.pop() {
            if !seen.insert(at) {
                continue;
            }
            if direct.contains(&controls[at]) {
                anchors.insert(controls[at].clone());
            } else if let Some(rows) = parents.get(&at) {
                for (p, key) in rows {
                    relations.push(OffsetRelation {
                        control: controls[at].clone(),
                        parent: controls[*p].clone(),
                        relation: key.clone(),
                    });
                    pending.push(*p);
                }
            } else {
                unbound = true;
            }
        }
        if unbound || anchors.is_empty() {
            return Err(invalid(format!("solver control is neither a skeleton bone nor wholly derived from bound controls: {name}")));
        }
        derived.push(DerivedControl {
            control: name.clone(),
            anchors: anchors.into_iter().collect(),
            offset_relations: relations,
        });
    }
    Ok(ControlBindings {
        skeleton_controls: direct.into_iter().collect(),
        derived_controls: derived,
    })
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RodEvidence {
    pub node_count: usize,
    pub scalar_array_count: usize,
    pub simd_pack_count: usize,
    pub scalar_unique_count: usize,
    pub simd_unique_count: usize,
    pub scalar_duplicate_records: usize,
    pub simd_duplicate_lanes: usize,
    pub scalar_records_absent_from_simd: usize,
    pub simd_records_absent_from_scalar: usize,
    pub scalar_occurrences_missing_from_simd: usize,
    pub numeric_tolerance_applied: bool,
    pub compiled_resources_modified: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RodAudit {
    pub status: Status,
    pub reason: Option<String>,
    pub evidence: RodEvidence,
}
const SCALAR: [&str; 4] = ["flMaxDist", "flMinDist", "flWeight0", "flRelaxationFactor"];
const SIMD: [&str; 4] = ["f4MaxDist", "f4MinDist", "f4Weight0", "f4RelaxationFactor"];
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct RodKey {
    nodes: [usize; 2],
    values: [u64; 4],
}
fn rod_key(nodes: &[i128], values: &[f64], count: usize) -> Result<RodKey> {
    if nodes.len() != 2
        || values.len() != 4
        || nodes.iter().any(|v| *v < 0 || *v >= count as i128)
        || values.iter().any(|v| !v.is_finite())
    {
        return Err(invalid("invalid rod endpoints or parameters"));
    }
    // Python Counter numeric keys identify signed zero. No tolerance or rounding.
    Ok(RodKey {
        nodes: [nodes[0] as usize, nodes[1] as usize],
        values: std::array::from_fn(|i| {
            if values[i] == 0.0 {
                0
            } else {
                values[i].to_bits()
            }
        }),
    })
}
pub fn audit_rods(physics: &Value) -> RodAudit {
    let mut evidence = RodEvidence::default();
    let check = (|| -> Result<Option<String>> {
        physics.object()?;
        let Some(fe) = physics.get("m_pFeModel") else {
            return Ok(Some("missing solver field".into()));
        };
        if matches!(fe.kind, Kind::Null) {
            return Ok(None);
        }
        fe.object()?;
        if ["m_nNodeCount", "m_Rods", "m_SimdRods"]
            .iter()
            .any(|k| fe.get(k).is_none())
        {
            return Ok(Some("solver rod schema incomplete".into()));
        }
        let count = integer(field(fe, "m_nNodeCount")?)?;
        if count < 0 {
            return Err(invalid("negative node count"));
        }
        let count = usize::try_from(count).map_err(|_| invalid("node count overflow"))?;
        let scalar = array(fe, "m_Rods")?;
        let simd = array(fe, "m_SimdRods")?;
        evidence.node_count = count;
        evidence.scalar_array_count = scalar.len();
        evidence.simd_pack_count = simd.len();
        for (rows, fields) in [(scalar, SCALAR), (simd, SIMD)] {
            for row in rows {
                let keys: BTreeSet<_> = row.object()?.iter().map(|(k, _)| k.as_str()).collect();
                let expected: BTreeSet<_> = std::iter::once("nNode").chain(fields).collect();
                if keys != expected {
                    return Ok(Some("unknown or incomplete rod record schema".into()));
                }
            }
        }
        let mut a: HashMap<RodKey, usize> = HashMap::new();
        let mut b: HashMap<RodKey, usize> = HashMap::new();
        for row in scalar {
            let nodes = integers(field(row, "nNode")?)?;
            let values = SCALAR
                .iter()
                .map(|key| number(field(row, key)?))
                .collect::<Result<Vec<_>>>()?;
            *a.entry(rod_key(&nodes, &values, count)?).or_default() += 1;
        }
        for row in simd {
            let nodes = array(row, "nNode")?;
            if nodes.len() != 2 {
                return Err(invalid("SIMD rod requires two node lanes"));
            }
            let n0 = integers(&nodes[0])?;
            let n1 = integers(&nodes[1])?;
            let values = SIMD
                .iter()
                .map(|key| numbers(field(row, key)?))
                .collect::<Result<Vec<_>>>()?;
            if n0.len() != 4 || n1.len() != 4 || values.iter().any(|v| v.len() != 4) {
                return Err(invalid("SIMD rods require exactly four complete lanes"));
            }
            for lane in 0..4 {
                let values = values.iter().map(|v| v[lane]).collect::<Vec<_>>();
                *b.entry(rod_key(&[n0[lane], n1[lane]], &values, count)?)
                    .or_default() += 1;
            }
        }
        evidence.scalar_unique_count = a.len();
        evidence.simd_unique_count = b.len();
        evidence.scalar_duplicate_records = scalar.len() - a.len();
        evidence.simd_duplicate_lanes = simd.len() * 4 - b.len();
        evidence.scalar_records_absent_from_simd = a.keys().filter(|k| !b.contains_key(*k)).count();
        evidence.simd_records_absent_from_scalar = b.keys().filter(|k| !a.contains_key(*k)).count();
        evidence.scalar_occurrences_missing_from_simd = a
            .iter()
            .map(|(key, n)| n.saturating_sub(*b.get(key).unwrap_or(&0)))
            .sum();
        if evidence.scalar_records_absent_from_simd > 0
            || evidence.simd_records_absent_from_scalar > 0
            || evidence.scalar_occurrences_missing_from_simd > 0
        {
            return Err(invalid(
                "scalar and SIMD rods disagree on complete constraint records",
            ));
        }
        Ok(None)
    })();
    match check {
        Ok(None) => RodAudit {
            status: Status::Passed,
            reason: None,
            evidence,
        },
        Ok(Some(reason)) => RodAudit {
            status: Status::Pending,
            reason: Some(reason),
            evidence,
        },
        Err(error) => RodAudit {
            status: Status::Failed,
            reason: Some(error.to_string()),
            evidence,
        },
    }
}

/// Retain every body-physics field and annotation when comparing solver edits.
pub fn same_body_physics(a: &Value, b: &Value) -> Result<bool> {
    let a_rows = a.object()?;
    let b_rows = b.object()?;
    let a_rows: Vec<_> = a_rows.iter().filter(|(k, _)| k != "m_pFeModel").collect();
    let b_rows: Vec<_> = b_rows.iter().filter(|(k, _)| k != "m_pFeModel").collect();
    Ok(a.annotation == b.annotation
        && a_rows.len() == b_rows.len()
        && a_rows
            .iter()
            .zip(b_rows)
            .all(|((k, v), (l, w))| k == l && v.canonical_eq(w)))
}
