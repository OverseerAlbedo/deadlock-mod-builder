//! Diagnostic source identity and independently derived additive source authority.
use crate::{
    dmx_bind, invalid, skinning,
    source_skeleton::{canonical_aliases, SourceAuditReport},
    Result, Skeleton,
};
use deadlock_dmx::{Dmx, Value};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Difference {
    pub original_count: usize,
    pub rebuilt_count: usize,
    pub missing: Vec<String>,
    pub extra: Vec<String>,
    pub duplicates: Vec<String>,
    pub reference_duplicates: Vec<String>,
}
fn difference(a: &[String], r: &[String]) -> Difference {
    fn counts(a: &[String]) -> BTreeMap<String, usize> {
        let mut c = BTreeMap::new();
        for n in a {
            *c.entry(n.clone()).or_insert(0) += 1;
        }
        c
    }
    let ac = counts(a);
    let rc = counts(r);
    Difference {
        original_count: r.len(),
        rebuilt_count: a.len(),
        missing: rc
            .keys()
            .filter(|n| !ac.contains_key(*n))
            .cloned()
            .collect(),
        extra: ac
            .keys()
            .filter(|n| !rc.contains_key(*n))
            .cloned()
            .collect(),
        duplicates: ac
            .iter()
            .filter(|(_, c)| **c > 1)
            .map(|(n, _)| n.clone())
            .collect(),
        reference_duplicates: rc
            .iter()
            .filter(|(_, c)| **c > 1)
            .map(|(n, _)| n.clone())
            .collect(),
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ParentChange {
    pub bone: String,
    pub original_parent: Option<String>,
    pub source_parent: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IdentityInspection {
    pub required: bool,
    pub passed: bool,
    pub errors: Vec<String>,
    pub source_modified: bool,
    pub bind_transforms_checked: bool,
    pub native_compiler_validated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_names: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_names: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity_difference: Option<Difference>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_aliases: Option<BTreeMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changed_parents: Option<Vec<ParentChange>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required_names_and_parents_match: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub joint_palette_entries: Option<usize>,
}
struct Graph {
    joints: BTreeMap<usize, String>,
    parents: BTreeMap<usize, usize>,
    canonical: BTreeMap<String, (usize, Option<String>)>,
    aliases: BTreeMap<String, String>,
}
fn graph(d: &Dmx, original: &SourceAuditReport) -> Result<Graph> {
    let joints: BTreeMap<_, _> = d
        .elements
        .iter()
        .enumerate()
        .filter(|(_, e)| e.kind == "DmeJoint")
        .map(|(i, e)| (i, e.name.clone()))
        .collect();
    if joints.values().any(String::is_empty)
        || joints.values().collect::<BTreeSet<_>>().len() != joints.len()
    {
        return Err(invalid(
            "Replacement DMX has missing or duplicate bone names",
        ));
    }
    if joints
        .keys()
        .map(|i| d.elements[*i].uuid)
        .collect::<BTreeSet<_>>()
        .len()
        != joints.len()
    {
        return Err(invalid(
            "Replacement DMX has duplicate bone element identities",
        ));
    }
    let mut parents = BTreeMap::new();
    for &i in joints.keys() {
        for child in skinning::ints(d, d.elements[i].get("children"))?
            .into_iter()
            .filter_map(|i| usize::try_from(i).ok())
        {
            if joints.contains_key(&child) && parents.insert(child, i).is_some() {
                return Err(invalid("Replacement DMX joint has multiple parents"));
            }
        }
    }
    for &i in joints.keys() {
        let mut seen = BTreeSet::new();
        let mut at = i;
        while let Some(&parent) = parents.get(&at) {
            if !seen.insert(at) {
                return Err(invalid("Replacement DMX has a cyclic bone hierarchy"));
            }
            at = parent;
        }
    }
    let authority = canonical_aliases(original)?;
    let name = |n: &String| authority.get(n).unwrap_or(n).clone();
    let aliases = joints
        .values()
        .filter(|n| authority.contains_key(*n))
        .map(|n| (n.clone(), name(n)))
        .collect();
    let mut canonical = BTreeMap::new();
    for (&i, n) in &joints {
        if canonical
            .insert(name(n), (i, parents.get(&i).map(|p| name(&joints[p]))))
            .is_some()
        {
            return Err(invalid("Ambiguous canonical source bone identity"));
        }
    }
    Ok(Graph {
        joints,
        parents,
        canonical,
        aliases,
    })
}
pub fn inspect_identity(d: &Dmx, original: &SourceAuditReport) -> IdentityInspection {
    let mut out = IdentityInspection {
        required: false,
        passed: true,
        errors: vec![],
        source_modified: false,
        bind_transforms_checked: false,
        native_compiler_validated: false,
        original_sha256: None,
        source_names: None,
        original_names: None,
        identity_difference: None,
        source_aliases: None,
        changed_parents: None,
        required_names_and_parents_match: None,
        joint_palette_entries: None,
    };
    if original.runtime_motion.status != "audited" {
        return out;
    }
    if original.runtime_motion.cloth_nodes == 0 {
        let Some(reference) = original.runtime_motion.skeleton.as_ref() else {
            return out;
        };
        // Ordinary no-cloth projects retain their existing diagnostic behavior.
        // An actual additional source bone needs the same hash-bound discovery
        // and native binding contract as an addition to a cloth-bearing model.
        let aliases = canonical_aliases(original).unwrap_or_default();
        let names: BTreeSet<_> = reference.names.iter().collect();
        if d.elements
            .iter()
            .filter(|e| e.kind == "DmeJoint")
            .all(|e| names.contains(aliases.get(&e.name).unwrap_or(&e.name)))
        {
            return out;
        }
    }
    out.required = true;
    out.passed = false;
    out.original_sha256 = original.compiled_sha256.clone();
    let run = (|| -> Result<()> {
        let reference = original
            .runtime_motion
            .skeleton
            .as_ref()
            .ok_or_else(|| invalid("Original skeleton missing"))?;
        let names: Vec<_> = d
            .elements
            .iter()
            .filter(|e| e.kind == "DmeJoint")
            .map(|e| e.name.clone())
            .collect();
        out.source_names = Some(names.clone());
        out.original_names = Some(reference.names.clone());
        out.identity_difference = Some(difference(&names, &reference.names));
        let g = graph(d, original)?;
        out.source_aliases = Some(g.aliases.clone());
        let canonical: Vec<_> = g.canonical.keys().cloned().collect();
        let diff = difference(&canonical, &reference.names);
        let matched = diff.missing.is_empty() && diff.extra.is_empty();
        out.identity_difference = Some(diff);
        if !matched {
            return Err(invalid(
                "Replacement DMX skeleton differs from the selected original",
            ));
        }
        let changes: Vec<_> = reference
            .names
            .iter()
            .enumerate()
            .filter_map(|(i, n)| {
                let expected = reference.parent_name(i).map(str::to_string);
                let actual = g.canonical[n].1.clone();
                (expected != actual).then(|| ParentChange {
                    bone: n.clone(),
                    original_parent: expected,
                    source_parent: actual,
                })
            })
            .collect();
        let parents_match = changes.is_empty();
        out.changed_parents = Some(changes);
        if !parents_match {
            return Err(invalid(
                "Replacement DMX bone parents differ from the selected original",
            ));
        }
        let palettes = skinning::palettes(d)?;
        if palettes.len() != 1 {
            return Err(invalid("Replacement DMX must have one unambiguous joint palette for runtime cloth preservation"));
        }
        let palette = palettes.first().unwrap();
        let entries: Vec<_> = palette
            .iter()
            .filter_map(|i| usize::try_from(*i).ok())
            .filter(|i| g.joints.contains_key(i))
            .collect();
        if entries.len() != g.joints.len()
            || entries.iter().collect::<BTreeSet<_>>().len() != g.joints.len()
        {
            return Err(invalid(
                "Replacement DMX joint palette is missing or duplicates required bones",
            ));
        }
        out.required_names_and_parents_match = Some(true);
        out.joint_palette_entries = Some(palette.len());
        Ok(())
    })();
    match run {
        Ok(()) => out.passed = true,
        Err(e) => out.errors.push(e.to_string()),
    }
    out
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WeightedExtra {
    pub mesh: String,
    pub bones: Vec<String>,
    pub weighted_positions: usize,
}
fn drawn(
    d: &Dmx,
    mesh: &deadlock_dmx::Element,
    state: &deadlock_dmx::Element,
    count: usize,
) -> Option<Vec<bool>> {
    let p = state.get("position$0").or_else(|| state.get("positions"))?;
    let corners = skinning::ints(
        d,
        state
            .get("position$0Indices")
            .or_else(|| state.get("positionsIndices")),
    )
    .ok()?;
    if p.len() != count || corners.is_empty() {
        return None;
    }
    let mut drawn = vec![false; count];
    for i in dmx_bind::refs(d, mesh, "faceSets").ok()? {
        let f = &d.elements[i];
        if f.kind != "DmeFaceSet" {
            return None;
        }
        let mut polygon = vec![];
        for corner in skinning::ints(d, f.get("faces")).ok()? {
            if corner == -1 {
                if polygon.iter().collect::<BTreeSet<_>>().len() >= 3 {
                    for p in &polygon {
                        drawn[*p] = true;
                    }
                }
                polygon.clear();
            } else {
                let c = usize::try_from(corner).ok()?;
                let p = usize::try_from(*corners.get(c)?).ok()?;
                if p >= count {
                    return None;
                }
                polygon.push(p);
            }
        }
        if !polygon.is_empty() {
            return None;
        }
    }
    Some(drawn)
}
pub fn weighted_extras(
    d: &Dmx,
    extra: &BTreeSet<String>,
    selected: &BTreeSet<String>,
) -> Result<Vec<WeightedExtra>> {
    if extra.is_empty() || selected.is_empty() {
        return Ok(vec![]);
    }
    let palettes = skinning::palettes(d)?;
    if palettes.len() != 1 {
        return Ok(vec![]);
    }
    let palette = palettes.first().unwrap();
    let slots: BTreeMap<_, _> = palette
        .iter()
        .enumerate()
        .filter_map(|(i, index)| {
            usize::try_from(*index)
                .ok()
                .and_then(|at| d.elements.get(at))
                .filter(|e| e.kind == "DmeJoint" && extra.contains(&e.name))
                .map(|e| (i as i32, e.name.clone()))
        })
        .collect();
    let mut out = vec![];
    for mesh in d
        .elements
        .iter()
        .filter(|e| e.kind == "DmeMesh" && selected.contains(&e.name))
    {
        let Some(st) = skinning::state(d, mesh) else {
            continue;
        };
        let Ok(weights) = skinning::floats(
            d,
            st.get("blendweights$0").or_else(|| st.get("jointWeights")),
        ) else {
            continue;
        };
        let Ok(indices) = skinning::ints(
            d,
            st.get("blendindices$0").or_else(|| st.get("jointIndices")),
        ) else {
            continue;
        };
        let n = match st.get("jointCount") {
            Some(Value::Integer(n)) if (1..=64).contains(n) => *n as usize,
            _ => continue,
        };
        if weights.len() != indices.len() || weights.len() % n != 0 {
            continue;
        }
        let Some(drawn) = drawn(d, mesh, st, weights.len() / n) else {
            continue;
        };
        let mut positions = BTreeSet::new();
        let mut used = BTreeSet::new();
        for (at, (i, w)) in indices.iter().zip(&weights).enumerate() {
            if *w > 0. && drawn[at / n] {
                if let Some(name) = slots.get(i) {
                    positions.insert(at / n);
                    used.insert(name.clone());
                }
            }
        }
        if !used.is_empty() {
            out.push(WeightedExtra {
                mesh: mesh.name.clone(),
                bones: used.into_iter().collect(),
                weighted_positions: positions.len(),
            });
        }
    }
    Ok(out)
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SourceContract {
    pub policy: String,
    pub source_sha256: String,
    pub original_sha256: String,
    pub skeleton: Skeleton,
    pub extra_names: Vec<String>,
    pub required_extra_names: Vec<String>,
    pub weighted_extra_bones: Vec<WeightedExtra>,
    pub selected_meshes: Vec<String>,
    pub source_aliases: BTreeMap<String, String>,
    pub source_bind_audit: dmx_bind::BindAudit,
    pub implicit_unit_scale_names: Vec<String>,
    pub original_names_and_parents_preserved: bool,
    pub source_modified: bool,
    pub weights_modified: bool,
    pub custom_cloth_solver_created: bool,
    pub native_compiler_validated: bool,
}
pub fn source_contract(
    d: &Dmx,
    original: &SourceAuditReport,
    selected: &BTreeSet<String>,
    sha: &str,
) -> Result<Option<SourceContract>> {
    let inspect = inspect_identity(d, original);
    let extras: BTreeSet<_> = inspect
        .identity_difference
        .as_ref()
        .map(|v| v.extra.iter().cloned().collect())
        .unwrap_or_default();
    let weighted = if inspect.required {
        weighted_extras(d, &extras, selected)?
    } else {
        vec![]
    };
    if weighted.is_empty() {
        return Ok(None);
    }
    if sha != format!("{:x}", Sha256::digest(d.bytes())) {
        return Err(invalid(
            "Custom-bone source contract does not match analyzed DMX bytes",
        ));
    }
    if d.format != "model" || d.format_version != 22 {
        return Err(invalid(
            "Custom-bone source requires audited model-22 format",
        ));
    }
    let reference = original
        .runtime_motion
        .skeleton
        .as_ref()
        .ok_or_else(|| invalid("Original skeleton missing"))?;
    let g = graph(d, original)?;
    let models: Vec<_> = d
        .elements
        .iter()
        .filter(|e| e.kind == "DmeModel" && e.get("jointList").is_some_and(|v| v.len() > 0))
        .collect();
    if models.len() != 1 {
        return Err(invalid("Custom-bone source requires one joint palette"));
    }
    let model = models[0];
    if model.get("axisSystem").is_some() {
        let axis = &d.elements[dmx_bind::index(d, model, "axisSystem")?];
        dmx_bind::unique_attrs(axis)?;
        if axis.kind != "DmeAxisSystem"
            || [("upAxis", 3), ("forwardParity", 1), ("coordSys", 0)]
                .iter()
                .any(|(k, n)| {
                    !axis.attributes.iter().any(|a| {
                        a.name == *k
                            && a.type_code == 2
                            && !a.array
                            && matches!(a.value,Value::Integer(v)if v==*n)
                    })
                })
        {
            return Err(invalid(
                "Custom-bone source requires canonical Z-up DMX axis system",
            ));
        }
    }
    let palette = dmx_bind::refs(d, model, "jointList")?;
    let order: Vec<_> = palette
        .iter()
        .copied()
        .filter(|i| g.joints.contains_key(i))
        .collect();
    if order.len() != g.joints.len()
        || order.iter().copied().collect::<BTreeSet<_>>() != g.joints.keys().copied().collect()
    {
        return Err(invalid("Custom-bone palette missing or duplicates joints"));
    }
    let originals: BTreeSet<_> = reference.names.iter().cloned().collect();
    if originals.iter().any(|n| !g.canonical.contains_key(n)) {
        return Err(invalid(
            "Custom-bone source must retain every original bone",
        ));
    }
    for (i, n) in reference.names.iter().enumerate() {
        if g.canonical[n].1.as_deref() != reference.parent_name(i) {
            return Err(invalid(format!(
                "Custom-bone source changed original parent: {n}"
            )));
        }
    }
    let extra: BTreeSet<_> = g
        .canonical
        .keys()
        .filter(|n| !originals.contains(*n))
        .cloned()
        .collect();
    if original
        .runtime_motion
        .controls
        .iter()
        .any(|n| extra.contains(n))
    {
        return Err(invalid(
            "Custom-bone source collides with original solver control",
        ));
    }
    for n in &extra {
        let mut at = n.as_str();
        let mut seen = BTreeSet::new();
        while extra.contains(at) {
            if !seen.insert(at) {
                return Err(invalid("Cyclic extra bone branch"));
            }
            at = g.canonical[at]
                .1
                .as_deref()
                .ok_or_else(|| invalid("Custom branch must descend from original bone"))?;
        }
        if !originals.contains(at) {
            return Err(invalid("Custom branch must descend from original bone"));
        }
    }
    let mut required = BTreeSet::new();
    for row in &weighted {
        for n in &row.bones {
            let mut at = g.aliases.get(n).unwrap_or(n).as_str();
            while extra.contains(at) {
                required.insert(at.to_string());
                at = g.canonical[at].1.as_deref().unwrap();
            }
        }
    }
    let lookup: BTreeMap<_, _> = order
        .iter()
        .enumerate()
        .map(|(i, &at)| (at, i as i32))
        .collect();
    let mut s = Skeleton {
        names: order.iter().map(|i| g.joints[i].clone()).collect(),
        parents: vec![],
        positions: vec![],
        rotations: vec![],
        scales: vec![],
        flags: vec![0; order.len()],
    };
    let mut implicit = vec![];
    for i in order {
        let joint = &d.elements[i];
        let t = dmx_bind::index(d, joint, "transform")?;
        let (p, q, scale) = dmx_bind::transform(d, t, false)?;
        if !(0.99..=1.01).contains(&crate::norm(&q)) {
            return Err(invalid("Invalid custom-bone bind quaternion"));
        }
        if d.elements[t].get("scale").is_none() {
            implicit.push(g.aliases.get(&joint.name).unwrap_or(&joint.name).clone());
        }
        s.parents
            .push(g.parents.get(&i).map(|p| lookup[p]).unwrap_or(-1));
        s.positions.push(p);
        s.rotations.push(q);
        s.scales.push(scale);
    }
    let bind = dmx_bind::audit_dmx_bind(d, &s, false)?;
    s.names = s
        .names
        .iter()
        .map(|n| g.aliases.get(n).unwrap_or(n).clone())
        .collect();
    Ok(Some(SourceContract {
        policy: "additive-source-bind-v1".into(),
        source_sha256: sha.into(),
        original_sha256: original
            .compiled_sha256
            .clone()
            .ok_or_else(|| invalid("Missing original hash"))?,
        skeleton: s,
        extra_names: extra.into_iter().collect(),
        required_extra_names: required.into_iter().collect(),
        weighted_extra_bones: weighted,
        selected_meshes: selected.iter().cloned().collect(),
        source_aliases: g.aliases,
        source_bind_audit: bind,
        implicit_unit_scale_names: implicit,
        original_names_and_parents_preserved: true,
        source_modified: false,
        weights_modified: false,
        custom_cloth_solver_created: false,
        native_compiler_validated: false,
    }))
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preflight {
    #[serde(flatten)]
    pub identity: IdentityInspection,
    pub selected_meshes: Vec<String>,
    pub weighted_extra_bones: Vec<WeightedExtra>,
    pub blocked: bool,
    pub compiled_identity_check_still_required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extension: Option<SourceContract>,
}
pub fn preflight(
    d: &Dmx,
    original: &SourceAuditReport,
    selected: &BTreeSet<String>,
    sha: Option<&str>,
) -> Result<Preflight> {
    let mut identity = inspect_identity(d, original);
    let extra = identity
        .identity_difference
        .as_ref()
        .map(|v| v.extra.iter().cloned().collect())
        .unwrap_or_default();
    let weighted = if identity.required {
        weighted_extras(d, &extra, selected)?
    } else {
        vec![]
    };
    let mut blocked = !weighted.is_empty();
    let mut extension = None;
    if blocked {
        let hash = sha
            .map(str::to_string)
            .unwrap_or_else(|| format!("{:x}", Sha256::digest(d.bytes())));
        match source_contract(d, original, selected, &hash) {
            Ok(Some(contract)) => {
                extension = Some(contract);
                blocked = false;
                identity.bind_transforms_checked = true
            }
            Ok(None) => identity
                .errors
                .push("Custom bone source evidence is incomplete".into()),
            Err(e) => identity.errors.push(e.to_string()),
        }
    }
    Ok(Preflight {
        compiled_identity_check_still_required: identity.required,
        identity,
        selected_meshes: selected.iter().cloned().collect(),
        weighted_extra_bones: weighted,
        blocked,
        extension,
    })
}
