//! Explicit equipment overrides remain separate from the baseline mapping review.
use super::{canonical, invalid, lod_base, Analysis, Mappings, Result, DISABLE, KEEP};
use deadlock_project::customization::{EquipmentMesh, Face, Prop};
use deadlock_project::{Customization, MeshMapping};
use std::collections::{BTreeMap, BTreeSet};
fn choices(state: &Customization) -> Option<&[EquipmentMesh]> {
    if !state.active() {
        return None;
    }
    state
        .equipment
        .as_ref()
        .filter(|e| e.enabled.unwrap_or(false))
        .map(|e| e.meshes.as_deref().unwrap_or_default())
}
pub fn selected_props(state: &Customization) -> Result<Vec<Prop>> {
    state.validate()?;
    if !state.active() {
        return Ok(Vec::new());
    }
    Ok(state
        .equipment
        .as_ref()
        .filter(|e| e.enabled.unwrap_or(false))
        .and_then(|e| e.props.as_ref())
        .map(|p| {
            p.iter()
                .filter(|p| p.enabled.unwrap_or(true))
                .cloned()
                .collect()
        })
        .unwrap_or_default())
}
pub fn face_selection(state: &Customization) -> Result<Option<Face>> {
    state.validate()?;
    Ok(if state.active() {
        state
            .face
            .as_ref()
            .filter(|f| f.enabled.unwrap_or(false))
            .cloned()
    } else {
        None
    })
}
pub fn apply_mappings(
    original: &Analysis,
    mappings: &Mappings,
    state: &Customization,
    source_names: &BTreeSet<String>,
) -> Result<Mappings> {
    state.validate()?;
    let mut result = mappings.clone();
    let Some(rows) = choices(state) else {
        return Ok(result);
    };
    let nodes: BTreeSet<_> = original.meshes.iter().map(|n| n.name.as_str()).collect();
    let mut assigned = BTreeMap::new();
    for row in rows {
        let target = row.target.as_deref().unwrap_or_default();
        if !nodes.contains(target) {
            return Err(invalid(format!(
                "Choose equipment from the original character mesh list: {target:?}"
            )));
        }
        let value = match row.action.as_deref().unwrap_or("keep") {
            "replace" => {
                let sources = row.sources.as_deref().unwrap_or_default();
                if sources.is_empty() {
                    return Err(invalid(format!(
                        "Choose the prepared replacement mesh(es) for {target}"
                    )));
                }
                let missing = sources
                    .iter()
                    .filter(|n| !source_names.contains(*n))
                    .cloned()
                    .collect::<BTreeSet<_>>();
                if !missing.is_empty() {
                    return Err(invalid(format!(
                        "Equipment mesh is not present in the prepared model: {}",
                        missing.into_iter().collect::<Vec<_>>().join(", ")
                    )));
                }
                if sources.len() == 1 {
                    MeshMapping::One(sources[0].clone())
                } else {
                    MeshMapping::Many(sources.to_vec())
                }
            }
            "hide" => MeshMapping::One(DISABLE.into()),
            _ => MeshMapping::One(KEEP.into()),
        };
        let affected = if row.include_lods.unwrap_or(true) {
            let family = canonical(&lod_base(target));
            nodes
                .iter()
                .filter(|n| canonical(&lod_base(n)) == family)
                .copied()
                .collect::<Vec<_>>()
        } else {
            vec![target]
        };
        for name in affected {
            if assigned.get(name).is_some_and(|old| old != &value) {
                return Err(invalid(format!(
                    "Equipment choices conflict across the LOD family: {name}"
                )));
            }
            assigned.insert(name.into(), value.clone());
        }
    }
    result.extend(assigned);
    if !result.is_empty()
        && result
            .values()
            .all(|v| v == &MeshMapping::One(DISABLE.into()))
    {
        return Err(invalid("Keep at least one visible character mesh."));
    }
    Ok(result)
}
pub fn project_mappings(
    original: &Analysis,
    effective: &Mappings,
    baseline: &Mappings,
    state: &Customization,
) -> Result<Mappings> {
    state.validate()?;
    let mut result = effective.clone();
    let Some(rows) = choices(state) else {
        return Ok(result);
    };
    let nodes: BTreeSet<_> = original.meshes.iter().map(|n| n.name.as_str()).collect();
    let mut controlled = BTreeSet::new();
    let mut source_names = BTreeSet::new();
    for row in rows {
        let target = row.target.as_deref().unwrap_or_default();
        if !nodes.contains(target) {
            return Err(invalid(
                "Equipment target changed; analyze the selected character again.",
            ));
        }
        if row.include_lods.unwrap_or(true) {
            let family = canonical(&lod_base(target));
            controlled.extend(
                nodes
                    .iter()
                    .filter(|n| canonical(&lod_base(n)) == family)
                    .copied(),
            );
        } else {
            controlled.insert(target);
        }
        source_names.extend(row.sources.as_deref().unwrap_or_default().iter().cloned());
    }
    let expected = apply_mappings(original, baseline, state, &source_names)?;
    for name in controlled {
        if effective.get(name) != expected.get(name) {
            return Err(invalid(format!(
                "This mesh is controlled by Equipment. Change or remove its Equipment choice before editing the legacy mesh mapping: {name}"
            )));
        }
        if let Some(value) = baseline.get(name) {
            result.insert(name.into(), value.clone());
        } else {
            result.remove(name);
        }
    }
    Ok(result)
}
