//! Original bone identity, exact local scale, inherited binds and typed resource gates.
#![forbid(unsafe_code)]
use deadlock_resource::{Kind, Value};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};
pub mod aliases;
pub mod bindings;
pub mod dmx_bind;
pub mod export;
pub mod extensions;
pub mod skinning;
pub mod source_bones;
pub mod source_identity;
pub mod source_skeleton;
pub mod transforms;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub String);
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Error {}
pub type Result<T> = std::result::Result<T, Error>;
pub(crate) fn invalid(message: impl Into<String>) -> Error {
    Error(message.into())
}
pub(crate) fn field<'a>(v: &'a Value, key: &str) -> Result<&'a Value> {
    v.get(key)
        .ok_or_else(|| invalid(format!("Missing compiled field {key}")))
}
pub(crate) fn array(v: &Value) -> Result<&[Value]> {
    v.array().map_err(|e| invalid(e.to_string()))
}
pub(crate) fn string(v: &Value) -> Result<&str> {
    v.string().map_err(|e| invalid(e.to_string()))
}
pub(crate) fn integer(v: &Value) -> Result<i64> {
    i64::try_from(v.integer().map_err(|e| invalid(e.to_string()))?)
        .map_err(|_| invalid("Integer outside supported signed range"))
}
pub(crate) fn number(v: &Value) -> Result<f64> {
    let n = match v.kind {
        Kind::F32(v) => v as f64,
        Kind::F64(v) => v,
        Kind::I64(v) => v as f64,
        Kind::U64(v) => v as f64,
        _ => return Err(invalid("Expected numeric bind component")),
    };
    if !n.is_finite() {
        return Err(invalid("Non-finite bind component"));
    }
    Ok(n)
}
pub(crate) fn vector<const N: usize>(v: &Value) -> Result<[f64; N]> {
    let data = array(v)?;
    if data.len() != N {
        return Err(invalid("Incorrect bind vector width"));
    }
    let mut result = [0.; N];
    for (dst, src) in result.iter_mut().zip(data) {
        *dst = number(src)?;
    }
    Ok(result)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Skeleton {
    #[serde(rename = "m_boneName")]
    pub names: Vec<String>,
    #[serde(rename = "m_nParent")]
    pub parents: Vec<i32>,
    #[serde(rename = "m_bonePosParent")]
    pub positions: Vec<[f64; 3]>,
    #[serde(rename = "m_boneRotParent")]
    pub rotations: Vec<[f64; 4]>,
    #[serde(rename = "m_boneScaleParent")]
    pub scales: Vec<f64>,
    #[serde(rename = "m_nFlag", default)]
    pub flags: Vec<u32>,
}
impl Skeleton {
    pub fn from_value(value: &Value) -> Result<Self> {
        let s = Self {
            names: array(field(value, "m_boneName")?)?
                .iter()
                .map(|v| string(v).map(str::to_string))
                .collect::<Result<_>>()?,
            parents: array(field(value, "m_nParent")?)?
                .iter()
                .map(|v| {
                    i32::try_from(integer(v)?).map_err(|_| invalid("Parent index outside i32"))
                })
                .collect::<Result<_>>()?,
            positions: array(field(value, "m_bonePosParent")?)?
                .iter()
                .map(vector)
                .collect::<Result<_>>()?,
            rotations: array(field(value, "m_boneRotParent")?)?
                .iter()
                .map(vector)
                .collect::<Result<_>>()?,
            scales: array(field(value, "m_boneScaleParent")?)?
                .iter()
                .map(number)
                .collect::<Result<_>>()?,
            flags: match value.get("m_nFlag") {
                None => vec![],
                Some(v) => array(v)?
                    .iter()
                    .map(|v| {
                        u32::try_from(integer(v)?).map_err(|_| invalid("Bone flag outside u32"))
                    })
                    .collect::<Result<_>>()?,
            },
        };
        s.validate(false)?;
        Ok(s)
    }
    pub fn validate(&self, require_flags: bool) -> Result<()> {
        let n = self.names.len();
        if n == 0
            || [
                self.parents.len(),
                self.positions.len(),
                self.rotations.len(),
                self.scales.len(),
            ]
            .iter()
            .any(|&len| len != n)
            || (!self.flags.is_empty() && self.flags.len() != n)
            || (require_flags && self.flags.len() != n)
        {
            return Err(invalid("Incomplete skeleton arrays"));
        }
        if self.names.iter().any(|s| s.is_empty())
            || self.names.iter().collect::<BTreeSet<_>>().len() != n
        {
            return Err(invalid("Missing or duplicate bone identities"));
        }
        for i in 0..n {
            if self.parents[i] < -1 || self.parents[i] >= n as i32 {
                return Err(invalid(format!("Invalid parent index: {}", self.names[i])));
            }
            if !self.positions[i]
                .iter()
                .chain(&self.rotations[i])
                .chain([&self.scales[i]])
                .all(|v| v.is_finite())
                || norm(&self.rotations[i]) < 0.99
            {
                return Err(invalid(format!(
                    "Invalid compiled bind transform: {}",
                    self.names[i]
                )));
            }
        }
        let mut done = vec![false; n];
        for i in 0..n {
            let mut active = BTreeSet::new();
            let mut at = Some(i);
            while let Some(node) = at {
                if done[node] {
                    break;
                }
                if !active.insert(node) {
                    return Err(invalid("Cyclic bone hierarchy"));
                }
                at = if self.parents[node] < 0 {
                    None
                } else {
                    Some(self.parents[node] as usize)
                };
            }
            for node in active {
                done[node] = true;
            }
        }
        Ok(())
    }
    pub fn parent_name(&self, index: usize) -> Option<&str> {
        self.parents
            .get(index)
            .copied()
            .filter(|&p| p >= 0)
            .and_then(|p| self.names.get(p as usize))
            .map(String::as_str)
    }
    pub fn indices(&self) -> BTreeMap<&str, usize> {
        self.names
            .iter()
            .enumerate()
            .map(|(i, n)| (n.as_str(), i))
            .collect()
    }
}
pub(crate) fn norm<const N: usize>(v: &[f64; N]) -> f64 {
    v.iter().map(|x| x * x).sum::<f64>().sqrt()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IdentityAudit {
    pub bone_count: usize,
    pub original_to_compiled: Vec<usize>,
    pub reordered_bones: usize,
    pub name_and_parent_identity_passed: bool,
    pub max_translation_error_m: f64,
    pub max_rotation_error_degrees: f64,
}
pub fn skeleton_identity(actual: &Skeleton, reference: &Skeleton) -> Result<IdentityAudit> {
    actual.validate(false)?;
    reference.validate(false)?;
    let lookup = actual.indices();
    if actual.names.len() != reference.names.len()
        || reference
            .names
            .iter()
            .any(|n| !lookup.contains_key(n.as_str()))
    {
        return Err(invalid(
            "Compiled skeleton lost, duplicated or added required bone identities",
        ));
    }
    let mut max_translation = 0_f64;
    let mut max_rotation = 0_f64;
    let mut mapping = vec![];
    for (i, name) in reference.names.iter().enumerate() {
        let j = lookup[name.as_str()];
        mapping.push(j);
        if actual.parent_name(j) != reference.parent_name(i) {
            return Err(invalid(format!(
                "Compiled skeleton parent identity changed: {name}"
            )));
        }
        let a = &actual.positions[j];
        let b = &reference.positions[i];
        let distance = a
            .iter()
            .zip(b)
            .map(|(x, y)| (x - y) * (x - y))
            .sum::<f64>()
            .sqrt();
        max_translation = max_translation.max(distance * 0.0254);
        let qa = &actual.rotations[j];
        let qb = &reference.rotations[i];
        let angle = (2.
            * ((qa.iter().zip(qb).map(|(x, y)| x * y).sum::<f64>().abs() / (norm(qa) * norm(qb)))
                .min(1.))
            .acos())
        .to_degrees();
        max_rotation = max_rotation.max(angle);
        if distance > 0.01 || angle > 0.1 || (actual.scales[j] - reference.scales[i]).abs() > 1e-4 {
            return Err(invalid(format!(
                "Compiled skeleton bind transform changed: {name}"
            )));
        }
    }
    Ok(IdentityAudit {
        bone_count: actual.names.len(),
        reordered_bones: mapping
            .iter()
            .enumerate()
            .filter(|(i, j)| *i != **j)
            .count(),
        original_to_compiled: mapping,
        name_and_parent_identity_passed: true,
        max_translation_error_m: max_translation,
        max_rotation_error_degrees: max_rotation,
    })
}
pub fn check_transfer(actual: &Skeleton, reference: &Skeleton) -> Result<IdentityAudit> {
    for s in [actual, reference] {
        s.validate(true)?;
        if s.rotations
            .iter()
            .any(|q| !(0.99..=1.01).contains(&norm(q)))
        {
            return Err(invalid("Invalid bind quaternion norm for physics transfer"));
        }
    }
    let audit = skeleton_identity(actual, reference)?;
    for (i, &j) in audit.original_to_compiled.iter().enumerate() {
        let p = actual.positions[j]
            .iter()
            .zip(reference.positions[i])
            .map(|(x, y)| (x - y).abs())
            .fold(0., f64::max);
        let q = quaternion_component_delta(&actual.rotations[j], &reference.rotations[i]);
        if p >= 0.001 || q >= 0.0001 {
            return Err(invalid(format!(
                "Bind transform outside original-physics transfer budget: {}",
                reference.names[i]
            )));
        }
    }
    Ok(audit)
}
pub(crate) fn quaternion_component_delta(a: &[f64; 4], b: &[f64; 4]) -> f64 {
    let minus = a
        .iter()
        .zip(b)
        .map(|(x, y)| (x - y).abs())
        .fold(0., f64::max);
    let plus = a
        .iter()
        .zip(b)
        .map(|(x, y)| (x + y).abs())
        .fold(0., f64::max);
    minus.min(plus)
}
