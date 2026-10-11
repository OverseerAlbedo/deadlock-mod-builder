//! Attachment-relative roll removal. Recomputed from the immutable compiler
//! snapshot at build AND install. No model names, control numbers or world axes.
use super::{
    fitting::{cross, dot, rotation, sub, unit, vector},
    number, usize_field,
};
use crate::{field, field_mut, invalid, strings, Result};
use deadlock_resource::{Kind, Value};
use deadlock_skeleton::Skeleton;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const POLICY: &str = "attachment-relative-secondary-roll-v2";
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Chain {
    pub anchor: String,
    pub bones: Vec<String>,
    /// Hair/tail curves can need a different transverse attachment reference
    /// at each joint. False retains the accepted streamer transform exactly.
    #[serde(default)]
    pub per_joint_reference: bool,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Proof {
    pub policy: String,
    pub chains: Vec<ChainProof>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChainProof {
    pub anchor: String,
    pub bones: Vec<String>,
    pub reference: [usize; 2],
    pub minimum_pole_angle_degrees: f64,
    #[serde(default)]
    pub joint_references: Vec<[usize; 2]>,
}
pub struct Patch {
    pub fe: Value,
    pub proof: Proof,
    pub(crate) bases: BTreeMap<usize, Value>,
}
fn integer(n: usize) -> Value {
    Value::new(Kind::I64(n as i64))
}
pub(super) fn quat(m: [[f64; 3]; 3]) -> [f64; 4] {
    let t = m[0][0] + m[1][1] + m[2][2];
    let q = if t > 0. {
        let s = (t + 1.).sqrt() * 2.;
        [
            (m[2][1] - m[1][2]) / s,
            (m[0][2] - m[2][0]) / s,
            (m[1][0] - m[0][1]) / s,
            s / 4.,
        ]
    } else {
        let i = (0..3).max_by(|&a, &b| m[a][a].total_cmp(&m[b][b])).unwrap();
        let j = (i + 1) % 3;
        let k = (i + 2) % 3;
        let s = (1. + m[i][i] - m[j][j] - m[k][k]).max(0.).sqrt() * 2.;
        let mut q = [0.; 4];
        q[i] = s / 4.;
        q[j] = (m[j][i] + m[i][j]) / s;
        q[k] = (m[k][i] + m[i][k]) / s;
        q[3] = (m[k][j] - m[j][k]) / s;
        q
    };
    let n = q.iter().map(|v| v * v).sum::<f64>().sqrt();
    q.map(|v| (v / n) as f32 as f64)
}
/// Each chain must follow the render hierarchy (unweighted intervening helpers
/// are allowed), own its position controls, and reach its OWN fixed supports.
pub fn apply(fe: &Value, skeleton: &Skeleton, chains: &[Chain]) -> Result<Patch> {
    let mut result = Patch {
        fe: fe.clone(),
        proof: Proof {
            policy: POLICY.into(),
            chains: vec![],
        },
        bases: BTreeMap::new(),
    };
    if chains.is_empty() {
        return Ok(result);
    }
    let names = strings(field(fe, "m_CtrlName")?)?;
    let poses = field(fe, "m_InitPose")?.array()?;
    let masses = field(fe, "m_NodeInvMasses")?.array()?;
    let parents = field(fe, "m_SkelParents")?.array()?;
    let static_ = usize_field(fe, "m_nStaticNodes")?;
    let locked_static = usize_field(fe, "m_nRotLockStaticNodes")?;
    let driven = usize_field(fe, "m_nFirstPositionDrivenNode")?;
    if names.len() != poses.len()
        || poses.len() != masses.len()
        || parents.len() != poses.len()
        || locked_static > static_
        || static_ > driven
        || driven > poses.len()
    {
        return Err(invalid("Invalid streamer solver partitions."));
    }
    if !field(fe, "m_SimdNodeBases")?.array()?.is_empty() {
        return Err(invalid(
            "Strand roll removal requires verified scalar native orientation fits.",
        ));
    }
    let ctrl = names
        .iter()
        .enumerate()
        .map(|(i, n)| (n.as_str(), i))
        .collect::<BTreeMap<_, _>>();
    let sk = skeleton.indices();
    let pos = |i: usize| -> Result<[f64; 3]> {
        let p = vector::<8>(&poses[i])?;
        Ok([p[0], p[1], p[2]])
    };
    let mut reverse = BTreeMap::new();
    for row in field(fe, "m_ReverseOffsets")?.array()? {
        if reverse
            .insert(
                usize_field(row, "nBoneCtrl")?,
                usize_field(row, "nTargetNode")?,
            )
            .is_some()
        {
            return Err(invalid("Duplicate streamer position fit."));
        }
    }
    // Traverse position rods only: fitted outputs and shared animated body
    // controls must never bridge two otherwise unrelated proxy components.
    let mut edges = vec![vec![]; driven];
    for row in field(fe, "m_Rods")?.array()? {
        let a = field(row, "nNode")?.array()?;
        if a.len() != 2 {
            return Err(invalid("Invalid streamer support rod."));
        }
        let a = usize::try_from(a[0].integer()?).map_err(|_| invalid("Invalid rod index."))?;
        let b = usize::try_from(field(row, "nNode")?.array()?[1].integer()?)
            .map_err(|_| invalid("Invalid rod index."))?;
        if a < driven && b < driven {
            edges[a].push(b);
            edges[b].push(a);
        }
    }
    let mut occupied = BTreeSet::new();
    let mut bases = field(fe, "m_NodeBases")?.array()?.to_vec();
    for chain in chains {
        if chain.bones.len() < 2 || chain.bones.len() > 512 {
            return Err(invalid(
                "Strand needs a connected chain of at least two weighted joints.",
            ));
        }
        let anchor = *sk
            .get(chain.anchor.as_str())
            .ok_or_else(|| invalid("Strand body attachment is missing."))?;
        let anchor_ctrl = *ctrl
            .get(chain.anchor.as_str())
            .ok_or_else(|| invalid("Strand native attachment is missing."))?;
        let nodes = chain
            .bones
            .iter()
            .map(|n| {
                ctrl.get(n.as_str())
                    .copied()
                    .ok_or_else(|| invalid(format!("Strand native joint is missing: {n}")))
            })
            .collect::<Result<Vec<_>>>()?;
        let mut previous = anchor;
        for (name, &node) in chain.bones.iter().zip(&nodes) {
            if node < driven || !occupied.insert(node) {
                return Err(invalid(
                    "Strand chains overlap or include non-fitted controls.",
                ));
            }
            let mut at = *sk
                .get(name.as_str())
                .ok_or_else(|| invalid("Strand render joint is missing."))?;
            let mut seen = BTreeSet::new();
            while at != previous {
                if !seen.insert(at) || skeleton.parents[at] < 0 {
                    return Err(invalid(
                        "Strand weighted joints do not form one attachment chain.",
                    ));
                }
                at = skeleton.parents[at] as usize;
            }
            previous = sk[name.as_str()];
        }
        let points = nodes
            .iter()
            .map(|n| {
                reverse
                    .get(n)
                    .copied()
                    .ok_or_else(|| invalid("Strand has no compact position support."))
            })
            .collect::<Result<Vec<_>>>()?;
        for (&point, &node) in points.iter().zip(&nodes) {
            if point < static_
                || point >= driven
                || number(&masses[point])? <= 0.
                || parents[point].integer()? != node as i128
            {
                return Err(invalid("Strand position support belongs to another part."));
            }
        }
        let mut connected = BTreeSet::new();
        let mut pending = vec![points[0]];
        while let Some(i) = pending.pop() {
            if connected.insert(i) {
                pending.extend(edges[i].iter().copied());
            }
        }
        if points.iter().any(|i| !connected.contains(i)) {
            return Err(invalid("Strand position supports are disconnected."));
        }
        let fixed = connected
            .iter()
            .copied()
            .filter(|&i| {
                i < locked_static
                    && number(&masses[i]).ok() == Some(0.)
                    && parents[i].integer().ok() == Some(anchor_ctrl as i128)
            })
            .collect::<Vec<_>>();
        let directions = points
            .windows(2)
            .map(|p| unit(sub(pos(p[1])?, pos(p[0])?)))
            .collect::<Result<Vec<_>>>()?;
        let mut candidates = vec![];
        for (j, &a) in fixed.iter().enumerate() {
            for &b in &fixed[j + 1..] {
                let Ok(x) = unit(sub(pos(b)?, pos(a)?)) else {
                    continue;
                };
                let scores = directions
                    .iter()
                    .map(|&y| 1. - dot(x, y).powi(2))
                    .collect::<Vec<_>>();
                candidates.push(([a, b], scores));
            }
        }
        // Keep the first equal candidate, matching the accepted streamer
        // implementation. Never lower the 60-degree rest-conditioning gate.
        let best = |joint: Option<usize>| -> Result<(f64, [usize; 2])> {
            let mut best = None;
            for (reference, scores) in &candidates {
                let score = joint.map_or_else(
                    || scores.iter().copied().fold(1f64, f64::min),
                    |j| scores[j],
                );
                if best
                    .as_ref()
                    .is_none_or(|v: &(f64, [usize; 2])| score > v.0)
                {
                    best = Some((score, *reference));
                }
            }
            best.ok_or_else(|| invalid("Moving strand has no fixed attachment reference."))
        };
        let shared = best(None)?;
        let mut references = vec![];
        let mut minimum_pole: f64 = 90.;
        for (j, &node) in nodes.iter().enumerate() {
            // Prefer one reference for the whole strand to avoid differential
            // roll between adjacent joints. Only curved strands that cannot
            // share a well-conditioned reference use per-joint choices.
            let (score, reference) = if chain.per_joint_reference && shared.0 < 0.75 {
                best(Some(j.min(directions.len() - 1)))?
            } else {
                shared
            };
            let pole = score.clamp(0., 1.).sqrt().asin().to_degrees();
            if pole < 60. {
                return Err(invalid(format!(
                    "{} has no well-conditioned attachment reference for twist protection. Review this part's rig before building.",
                    chain.bones[j]
                )));
            }
            minimum_pole = minimum_pole.min(pole);
            references.push(reference);
            let raw_x = sub(pos(reference[1])?, pos(reference[0])?);
            let pair = if j + 1 < points.len() {
                [points[j], points[j + 1]]
            } else {
                [points[j - 1], points[j]]
            };
            let y = unit(sub(pos(pair[1])?, pos(pair[0])?))?;
            let x = unit(sub(raw_x, y.map(|v| v * dot(raw_x, y))))?;
            let z = cross(x, y);
            let b: [[f64; 3]; 3] = std::array::from_fn(|r| [x[r], y[r], z[r]]);
            let p = vector::<8>(&poses[node])?;
            let r = rotation([p[4], p[5], p[6], p[7]])?;
            let q = quat(std::array::from_fn(|a| {
                std::array::from_fn(|c| (0..3).map(|k| b[k][a] * r[k][c]).sum())
            }));
            let rows = bases
                .iter()
                .enumerate()
                .filter(|(_, r)| usize_field(r, "nNode").ok() == Some(node))
                .map(|(i, _)| i)
                .collect::<Vec<_>>();
            if rows.len() != 1 {
                return Err(invalid(
                    "Strand lacks a unique native compact orientation fit.",
                ));
            }
            let row = &mut bases[rows[0]];
            for (key, index) in ["nNodeX0", "nNodeX1", "nNodeY0", "nNodeY1"]
                .into_iter()
                .zip([reference[0], reference[1], pair[0], pair[1]])
            {
                *field_mut(row, key)? = integer(index);
            }
            *field_mut(row, "qAdjust")? = Value::new(Kind::Array(
                q.into_iter()
                    .map(|n| Value::new(Kind::F32(n as f32)))
                    .collect(),
            ));
            result.bases.insert(node, row.clone());
        }
        result.proof.chains.push(ChainProof {
            anchor: chain.anchor.clone(),
            bones: chain.bones.clone(),
            reference: shared.1,
            minimum_pole_angle_degrees: minimum_pole,
            joint_references: references,
        });
    }
    *field_mut(&mut result.fe, "m_NodeBases")? = Value::new(Kind::Array(bases));
    Ok(result)
}
