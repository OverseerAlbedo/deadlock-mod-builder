//! Serialized model-22 bind proof. Reads actual f32 fields and graph ownership.
use crate::{
    invalid,
    transforms::{self, ScaleAudit},
    IdentityAudit, Result, Skeleton,
};
use deadlock_dmx::{Dmx, Element, Value};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
pub(crate) fn refs(d: &Dmx, e: &Element, key: &str) -> Result<Vec<usize>> {
    let Some(v) = e.get(key) else {
        return Ok(vec![]);
    };
    d.integer_values(v)
        .map_err(|e| invalid(e.to_string()))?
        .into_iter()
        .map(|i| {
            usize::try_from(i)
                .ok()
                .filter(|i| *i < d.elements.len())
                .ok_or_else(|| invalid("Missing or invalid DMX element reference"))
        })
        .collect()
}
pub(crate) fn index(d: &Dmx, e: &Element, key: &str) -> Result<usize> {
    e.get(key)
        .and_then(Value::as_ref_index)
        .and_then(|i| usize::try_from(i).ok())
        .filter(|i| *i < d.elements.len())
        .ok_or_else(|| invalid(format!("Missing or invalid DMX {key} element reference")))
}
pub(crate) fn unique_attrs(e: &Element) -> Result<()> {
    if e.attributes
        .iter()
        .map(|a| &a.name)
        .collect::<BTreeSet<_>>()
        .len()
        != e.attributes.len()
    {
        return Err(invalid(format!(
            "Duplicate transform attributes: {}",
            e.name
        )));
    }
    Ok(())
}
pub(crate) fn transform(d: &Dmx, i: usize, explicit: bool) -> Result<([f64; 3], [f64; 4], f64)> {
    let e = d
        .elements
        .get(i)
        .filter(|e| e.kind == "DmeTransform")
        .ok_or_else(|| invalid("Invalid bind transform element"))?;
    unique_attrs(e)?;
    fn vector<const N: usize>(d: &Dmx, e: &Element, key: &str, kind: u8) -> Result<[f64; N]> {
        let a = e
            .attributes
            .iter()
            .find(|a| a.name == key && a.type_code == kind && !a.array)
            .ok_or_else(|| invalid(format!("Prepared DMX requires typed {key}: {}", e.name)))?;
        let raw = d.raw_attribute(a);
        if raw.len() != N * 4 {
            return Err(invalid("Invalid transform bytes"));
        }
        let mut v = [0.; N];
        for (dst, b) in v.iter_mut().zip(raw.chunks_exact(4)) {
            *dst = f32::from_le_bytes(b.try_into().unwrap()) as f64;
        }
        if v.iter().any(|v| !v.is_finite()) {
            return Err(invalid("Non-finite transform values"));
        }
        Ok(v)
    }
    let p = vector(d, e, "position", 10)?;
    let q = vector(d, e, "orientation", 13)?;
    let scale = match e.attributes.iter().find(|a| a.name == "scale") {
        None if !explicit => 1.,
        Some(a) if a.type_code == 3 && !a.array => match a.value {
            Value::Float(f) => f as f64,
            _ => return Err(invalid("Invalid scale bytes")),
        },
        _ => return Err(invalid("Missing or invalid explicit scalar float32 scale")),
    };
    if !scale.is_finite() || scale <= 0. {
        return Err(invalid("Positive finite local scale required"));
    }
    Ok((p, q, scale))
}
fn identity(d: &Dmx, i: usize, explicit: bool) -> Result<()> {
    let (p, q, s) = transform(d, i, explicit)?;
    if p != [0.; 3] || q != [0., 0., 0., 1.] && q != [0., 0., 0., -1.] || s != 1. {
        return Err(invalid(
            "Prepared DMX model/mesh transform must be identity for canonical binds",
        ));
    }
    Ok(())
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BindAudit {
    pub bind_identity: IdentityAudit,
    pub target_scale_audit: ScaleAudit,
    pub current_and_bind_equal: bool,
    pub joint_palette_entries: usize,
    pub world_inverse_bind_consistent: bool,
    pub serialized_local_transforms_checked: bool,
}
pub fn audit_dmx_bind(d: &Dmx, reference: &Skeleton, explicit: bool) -> Result<BindAudit> {
    let models: Vec<_> = d
        .elements
        .iter()
        .enumerate()
        .filter(|(_, e)| e.kind == "DmeModel" && e.get("jointList").is_some_and(|v| v.len() > 0))
        .collect();
    if models.len() != 1 {
        return Err(invalid(
            "Prepared DMX requires one unambiguous model joint palette",
        ));
    }
    let (mi, model) = models[0];
    let root = d
        .elements
        .first()
        .filter(|e| e.kind == "DmElement")
        .ok_or_else(|| invalid("Prepared DMX root missing"))?;
    unique_attrs(root)?;
    for key in ["model", "skeleton"] {
        if !root
            .attributes
            .iter()
            .any(|a| a.name == key && a.type_code == 1 && !a.array)
            || index(d, root, key)? != mi
        {
            return Err(invalid(
                "Root model/skeleton does not reference validated model",
            ));
        }
    }
    unique_attrs(model)?;
    identity(d, index(d, model, "transform")?, explicit)?;
    let palette = refs(d, model, "jointList")?;
    let palette_set: BTreeSet<_> = palette.iter().copied().collect();
    if palette_set.len() != palette.len() {
        return Err(invalid("Duplicate joint palette elements"));
    }
    let joints: BTreeMap<_, _> = d
        .elements
        .iter()
        .enumerate()
        .filter(|(_, e)| e.kind == "DmeJoint")
        .collect();
    let order: Vec<_> = palette
        .iter()
        .copied()
        .filter(|i| joints.contains_key(i))
        .collect();
    let names: Vec<_> = order.iter().map(|i| joints[i].name.clone()).collect();
    if order.iter().copied().collect::<BTreeSet<_>>() != joints.keys().copied().collect()
        || names.iter().collect::<BTreeSet<_>>().len() != names.len()
        || names.iter().collect::<BTreeSet<_>>() != reference.names.iter().collect()
        || joints
            .values()
            .map(|e| e.uuid)
            .collect::<BTreeSet<_>>()
            .len()
            != joints.len()
    {
        return Err(invalid(
            "Prepared joint names/identities differ from original skeleton",
        ));
    }
    let states: Vec<_> = refs(d, model, "baseStates")?
        .into_iter()
        .map(|i| &d.elements[i])
        .collect();
    if states
        .iter()
        .any(|e| !["DmeTransformList", "DmeTransformsList"].contains(&e.kind.as_str()))
    {
        return Err(invalid("Invalid bind transform-list element"));
    }
    let binds: Vec<_> = states.into_iter().filter(|e| e.name == "bind").collect();
    if binds.len() != 1 {
        return Err(invalid(
            "Prepared DMX requires one explicit bind transform list",
        ));
    }
    let bind_refs = refs(d, binds[0], "transforms")?;
    if bind_refs.len() != palette.len() {
        return Err(invalid("Bind transform list differs from joint palette"));
    }
    let mut parents = BTreeMap::new();
    for (&i, j) in &joints {
        let children = refs(d, j, "children")?;
        if children.iter().collect::<BTreeSet<_>>().len() != children.len()
            || children.iter().any(|c| !joints.contains_key(c))
        {
            return Err(invalid(
                "Joint children must contain only directly parented joints",
            ));
        }
        for c in children {
            if parents.insert(c, i).is_some() {
                return Err(invalid("Duplicate/multiple joint parents"));
            }
        }
    }
    let children = refs(d, model, "children")?;
    let roots: BTreeSet<_> = children
        .iter()
        .copied()
        .filter(|i| joints.contains_key(i))
        .collect();
    if roots
        != joints
            .keys()
            .copied()
            .filter(|i| !parents.contains_key(i))
            .collect()
    {
        return Err(invalid("Root joint references disagree with hierarchy"));
    }
    let expected: BTreeSet<_> = roots
        .into_iter()
        .chain(palette.iter().copied().filter(|i| !joints.contains_key(i)))
        .collect();
    if children.iter().copied().collect::<BTreeSet<_>>() != expected
        || children.iter().collect::<BTreeSet<_>>().len() != children.len()
    {
        return Err(invalid(
            "Mesh DAGs must be direct children of identity model",
        ));
    }
    for (i, e) in d.elements.iter().enumerate() {
        if i != mi
            && !joints.contains_key(&i)
            && refs(d, e, "children")?
                .iter()
                .any(|i| palette_set.contains(i))
        {
            return Err(invalid(
                "Palette element has unsupported/shared graph parent",
            ));
        }
    }
    let mut observed = BTreeMap::new();
    for (&i, &b) in palette.iter().zip(&bind_refs) {
        let dag = &d.elements[i];
        if !["DmeJoint", "DmeDag"].contains(&dag.kind.as_str()) {
            return Err(invalid("Invalid palette DAG kind"));
        }
        let t = index(d, dag, "transform")?;
        let current = transform(d, t, explicit)?;
        if current != transform(d, b, explicit)? {
            return Err(invalid(format!(
                "Current and bind transforms differ: {}",
                dag.name
            )));
        }
        if joints.contains_key(&i) {
            observed.insert(i, current);
        } else {
            identity(d, t, explicit)?;
        }
    }
    let lookup: BTreeMap<_, _> = order
        .iter()
        .enumerate()
        .map(|(at, &i)| (i, at as i32))
        .collect();
    let mut actual = Skeleton {
        names,
        parents: vec![],
        positions: vec![],
        rotations: vec![],
        scales: vec![],
        flags: vec![],
    };
    for i in order {
        actual
            .parents
            .push(parents.get(&i).map(|p| lookup[p]).unwrap_or(-1));
        let (p, q, s) = observed[&i];
        actual.positions.push(p);
        actual.rotations.push(q);
        actual.scales.push(s);
    }
    Ok(BindAudit {
        bind_identity: crate::skeleton_identity(&actual, reference)?,
        target_scale_audit: transforms::audit_scale_preservation(&actual, reference, true)?,
        current_and_bind_equal: true,
        joint_palette_entries: palette.len(),
        world_inverse_bind_consistent: true,
        serialized_local_transforms_checked: true,
    })
}
