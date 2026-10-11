use crate::*;
use deadlock_dmx::DmxSummary;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) const SOURCE_EXTENSIONS: &[&str] = &[
    "vmat", "vtex", "png", "tga", "jpg", "jpeg", "tif", "tiff", "exr",
];
pub fn texture_references(text: &str) -> Vec<String> {
    let re = regex::Regex::new(r#"(?i)"([^"\r\n]+\.(?:png|tga|jpg|jpeg|tif|tiff|exr|vtex|vmat))""#)
        .expect("constant regex");
    re.captures_iter(text)
        .map(|v| v[1].to_owned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}
fn fingerprint(root: &Path, cancel: &CancellationToken) -> Result<Hashes> {
    let mut found = Hashes::new();
    for file in walk_files(root, cancel)? {
        found.insert(
            relative_file(root, &file)?,
            safety::digest_with_cancel(&file, cancel)?,
        );
    }
    Ok(found)
}
fn copy_tree(root: &Path, destination: &Path, cancel: &CancellationToken) -> Result<()> {
    for file in walk_files(root, cancel)? {
        let relative = relative_file(root, &file)?;
        copy_file(&file, &inside(destination, &relative)?, cancel)?;
    }
    Ok(())
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct OverlayFingerprint {
    base: Hashes,
    overlay: Hashes,
}
pub fn effective_sources(
    base: &Path,
    overlay: Option<&Path>,
    destination: &Path,
    cancel: &CancellationToken,
) -> Result<PathBuf> {
    let Some(overlay) = overlay else {
        return Ok(base.to_owned());
    };
    deadlock_vpk::check_unlinked_path(base, false)?;
    deadlock_vpk::check_unlinked_path(overlay, false)?;
    deadlock_vpk::check_unlinked_path(destination, true)?;
    if !overlay.is_dir() {
        return Err(invalid(format!(
            "Source material overlay not found: {}",
            overlay.display()
        )));
    }
    let base = base.canonicalize()?;
    let overlay = overlay.canonicalize()?;
    for file in walk_files(&overlay, cancel)? {
        let extension = file
            .extension()
            .and_then(|p| p.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !SOURCE_EXTENSIONS.contains(&extension.as_str()) {
            return Err(invalid(format!(
                "Material overlay contains an unsupported source file: {}",
                file.display()
            )));
        }
    }
    // Existing parent canonicalization is performed before destructive changes.
    let parent = destination
        .parent()
        .ok_or_else(|| invalid("Overlay destination has no parent."))?;
    fs::create_dir_all(parent)?;
    let parent = parent.canonicalize()?;
    let destination = parent.join(
        destination
            .file_name()
            .ok_or_else(|| invalid("Overlay destination has no name."))?,
    );
    for input in [&base, &overlay] {
        if destination.starts_with(input) || input.starts_with(&destination) {
            return Err(invalid(
                "Effective material output must be separate from its source trees.",
            ));
        }
    }
    let before = OverlayFingerprint {
        base: fingerprint(&base, cancel)?,
        overlay: fingerprint(&overlay, cancel)?,
    };
    let mut expected = before.base.clone();
    expected.extend(before.overlay.clone());
    let marker = parent.join("overlay-fingerprint.json");
    if destination.is_dir() && marker.is_file() {
        let cached: std::result::Result<OverlayFingerprint, _> =
            serde_json::from_str(&read_text(&marker)?);
        if cached.is_ok_and(|v| v == before) && fingerprint(&destination, cancel)? == expected {
            return Ok(destination);
        }
    }
    let pending = tempfile::Builder::new()
        .prefix("overlay-pending-")
        .tempdir_in(&parent)?;
    copy_tree(&base, pending.path(), cancel)?;
    copy_tree(&overlay, pending.path(), cancel)?;
    if fingerprint(pending.path(), cancel)? != expected
        || fingerprint(&base, cancel)? != before.base
        || fingerprint(&overlay, cancel)? != before.overlay
    {
        return Err(invalid(
            "Material source changed while creating the effective overlay.",
        ));
    }
    cancel.check()?;
    let backup = tempfile::Builder::new()
        .prefix("overlay-backup-")
        .tempdir_in(&parent)?;
    let previous = backup.path().join("previous");
    let had_previous = destination.exists();
    if had_previous {
        deadlock_vpk::check_unlinked_path(&destination, false)?;
        fs::rename(&destination, &previous)?;
    }
    if let Err(error) = deadlock_vpk::publish_directory(pending.path(), &destination) {
        if had_previous {
            fs::rename(&previous, &destination)?;
        }
        return Err(error.into());
    }
    safety::write_json(&marker, &before)?;
    Ok(destination)
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MaterialAudit {
    pub identities: usize,
    pub unique_materials: usize,
    pub direct_source_dependencies: Vec<String>,
    pub mapping_preserved: bool,
    pub remap_preserved: bool,
}
pub fn validate_materials(
    summary: &DmxSummary,
    source: &Path,
    mapping_file: Option<&Path>,
    remap_file: Option<&Path>,
) -> Result<MaterialAudit> {
    fn mapping(path: Option<&Path>) -> Result<Option<BTreeMap<String, serde_json::Value>>> {
        path.map(|p| Ok(serde_json::from_str(&read_text(p)?)?))
            .transpose()
    }
    let mapping = mapping(mapping_file)?;
    let remap = mapping_fn(remap_file)?;
    for item in &summary.material_identities {
        if let Some(map) = &mapping {
            let row = map.get(&item.name).ok_or_else(|| {
                invalid(format!(
                    "Material mapping differs from the DMX identity: {}",
                    item.name
                ))
            })?;
            if !row.is_object() {
                return Err(invalid(format!(
                    "Material mapping differs from the DMX identity: {}",
                    item.name
                )));
            }
            if remap.is_none()
                && row
                    .get("source2_material_path")
                    .filter(|v| !v.is_null())
                    .is_some_and(|v| v.as_str() != Some(&item.path))
            {
                return Err(invalid(format!(
                    "Material mapping differs from the DMX identity: {}",
                    item.name
                )));
            }
        }
        if let Some(map) = &remap {
            if map
                .get(&item.name)
                .and_then(|v| v.get("vmat"))
                .and_then(|v| v.as_str())
                != Some(&item.path)
            {
                return Err(invalid(format!(
                    "Material remap differs from the DMX identity: {}",
                    item.name
                )));
            }
        }
    }
    let mut references = BTreeSet::new();
    for path in &summary.materials {
        let material = inside(source, path)?;
        if !material.is_file() {
            return Err(invalid(format!(
                "Supplied material overlay is missing directly referenced VMAT: {path}"
            )));
        }
        for dependency in texture_references(&read_text(&material)?) {
            if !inside(source, &dependency)?.is_file() {
                return Err(invalid(format!(
                    "Missing source dependency {dependency} referenced by {path}"
                )));
            }
            references.insert(dependency);
        }
    }
    Ok(MaterialAudit {
        identities: summary.material_identities.len(),
        unique_materials: summary.materials.len(),
        direct_source_dependencies: references.into_iter().collect(),
        mapping_preserved: mapping.is_some(),
        remap_preserved: remap.is_some(),
    })
}
fn mapping_fn(path: Option<&Path>) -> Result<Option<BTreeMap<String, serde_json::Value>>> {
    path.map(|p| Ok(serde_json::from_str(&read_text(p)?)?))
        .transpose()
}
