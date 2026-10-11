//! Read-only numeric skin checks. This receipt never grants native acceptance.
use crate::{invalid, Result};
use deadlock_dmx::{Dmx, Element, Value};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
pub(crate) fn ints(d: &Dmx, v: Option<&Value>) -> Result<Vec<i32>> {
    v.map(|v| d.integer_values(v).map_err(|e| invalid(e.to_string())))
        .transpose()
        .map(Option::unwrap_or_default)
}
pub(crate) fn floats(d: &Dmx, v: Option<&Value>) -> Result<Vec<f64>> {
    match v {
        None => Ok(vec![]),
        Some(v @ Value::Packed { base: 3, .. }) => Ok(d
            .value_bytes(v)
            .unwrap()
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes(b.try_into().unwrap()) as f64)
            .collect()),
        _ => Err(invalid("Expected scalar float32 skin stream")),
    }
}
pub(crate) fn palettes(d: &Dmx) -> Result<BTreeSet<Vec<i32>>> {
    d.elements
        .iter()
        .filter(|e| e.kind == "DmeModel" && e.get("jointList").is_some_and(|v| v.len() > 0))
        .map(|e| ints(d, e.get("jointList")))
        .collect()
}
pub(crate) fn state<'a>(d: &'a Dmx, e: &Element) -> Option<&'a Element> {
    e.get("bindState")
        .or_else(|| e.get("currentState"))
        .and_then(Value::as_ref_index)
        .and_then(|i| usize::try_from(i).ok())
        .and_then(|i| d.elements.get(i))
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MeshSkin {
    pub name: String,
    pub vertices: usize,
    pub joint_count: i32,
    pub errors: Vec<String>,
    pub pending: Vec<String>,
    pub passed: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SkinningAudit {
    pub meshes: Vec<MeshSkin>,
    pub joint_palette_entries: Option<usize>,
    pub numeric_checks_passed: bool,
}
pub fn skinning(d: &Dmx) -> Result<SkinningAudit> {
    let summary = d.summary().map_err(|e| invalid(e.to_string()))?;
    let palettes = palettes(d)?;
    let palette = if palettes.len() == 1 {
        palettes.first()
    } else {
        None
    };
    let mut meshes = vec![];
    for m in summary.meshes {
        let st = state(d, &d.elements[m.element]);
        let get = |a: &str, b: &str| st.and_then(|s| s.get(a).or_else(|| s.get(b)));
        let weights = floats(d, get("blendweights$0", "jointWeights"))?;
        let indices = ints(d, get("blendindices$0", "jointIndices"))?;
        let slots = match st.and_then(|s| s.get("jointCount")) {
            Some(Value::Integer(n)) => *n,
            _ => 0,
        };
        let mut errors = vec![];
        let mut pending = vec![];
        if weights.is_empty() || indices.is_empty() {
            errors.push("Skin weights/joint indices are missing".into());
        } else if !(1..=64).contains(&slots) {
            errors.push("Invalid or unsupported influences-per-vertex jointCount".into());
        } else {
            let count = m
                .vertices
                .checked_mul(slots as usize)
                .ok_or_else(|| invalid("Skin count overflow"))?;
            if weights.len() != count || indices.len() != weights.len() {
                errors.push("Weight/index counts do not match vertices × jointCount; inspect exporter/indexed layout".into());
            }
            let invalid_weights = weights.iter().any(|w| !w.is_finite() || *w < 0.);
            if invalid_weights {
                errors.push("Weights contain negative or non-finite values".into());
            }
            if !invalid_weights && weights.len() == count {
                let bad = weights
                    .chunks(slots as usize)
                    .filter(|w| (w.iter().sum::<f64>() - 1.).abs() > 0.02)
                    .count();
                if bad > 0 {
                    errors.push(format!(
                        "{bad} vertices have weights that do not sum to 1 within 0.02"
                    ));
                }
            }
            if let Some(p) = palette {
                let bad = indices
                    .iter()
                    .zip(&weights)
                    .filter(|(i, w)| **w > 0. && (**i < 0 || **i as usize >= p.len()))
                    .count();
                if bad > 0 {
                    errors.push(format!(
                        "{bad} positive-weight influences point outside the joint palette"
                    ));
                } else {
                    let bad = indices
                        .iter()
                        .zip(&weights)
                        .filter(|(_, w)| **w > 0.)
                        .filter(|(i, _)| {
                            usize::try_from(p[**i as usize])
                                .ok()
                                .and_then(|at| d.elements.get(at))
                                .is_none_or(|e| e.kind != "DmeJoint")
                        })
                        .count();
                    if bad > 0 {
                        errors.push(format!(
                            "{bad} positive-weight influences reference non-bone palette entries"
                        ));
                    }
                }
            } else {
                pending.push(
                    "No unique DmeModel joint palette; index-to-bone validation needs review"
                        .into(),
                );
            }
        }
        let passed = errors.is_empty() && pending.is_empty();
        meshes.push(MeshSkin {
            name: m.name,
            vertices: m.vertices,
            joint_count: slots,
            errors,
            pending,
            passed,
        });
    }
    Ok(SkinningAudit {
        numeric_checks_passed: meshes.iter().all(|m| m.passed),
        joint_palette_entries: palette.map(Vec::len),
        meshes,
    })
}
