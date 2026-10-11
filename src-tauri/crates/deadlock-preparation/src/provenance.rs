//! Geometry selections and dependency hashes bind a preparation receipt to its inputs.
use crate::{
    Result, digest, invalid, path_text,
    source::{self, ManifestEntry},
};
use deadlock_project::{
    Project,
    customization::{Face, Prop},
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SourceProof {
    pub relative: String,
    pub sha256: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClothingSelection {
    pub source: String,
    pub mode: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_files: Vec<SourceProof>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PropSelection {
    #[serde(flatten)]
    pub choice: Prop,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_files: Vec<SourceProof>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GeometrySelection {
    pub props: Vec<Prop>,
    pub face: Option<Face>,
}
pub fn clothing_selection(project: &Project) -> Result<Option<ClothingSelection>> {
    if !project.customization.active() {
        return Ok(None);
    }
    let Some(clothing) = project
        .customization
        .clothing
        .as_ref()
        .filter(|c| c.enabled.unwrap_or(false))
    else {
        return Ok(None);
    };
    let text = clothing.source.as_deref().unwrap_or_default();
    Ok(Some(ClothingSelection {
        source: if text.trim().is_empty() {
            String::new()
        } else {
            path_text(&source::normalized_input(text)?)?
        },
        mode: clothing.mode.as_deref().unwrap_or("weighted").into(),
        source_files: vec![],
    }))
}
pub fn customization_selection(project: &Project) -> Result<GeometrySelection> {
    let mut props = deadlock_model::equipment::selected_props(&project.customization)
        .map_err(|e| invalid(e.to_string()))?;
    for prop in &mut props {
        let text = prop.source.as_deref().unwrap_or_default();
        prop.source = Some(if text.trim().is_empty() {
            String::new()
        } else {
            path_text(&source::normalized_input(text)?)?
        });
        prop.translation = Some(prop.translation.unwrap_or([0.; 3]));
        prop.rotation = Some(prop.rotation.unwrap_or([0.; 3]));
        prop.scale = Some(prop.scale.unwrap_or(1.));
        prop.enabled = Some(true);
    }
    let face = deadlock_model::equipment::face_selection(&project.customization)
        .map_err(|e| invalid(e.to_string()))?
        .map(|f| Face {
            enabled: Some(true),
            mode: Some("static".into()),
            weights: Some(f.weights.unwrap_or_default()),
            ..Default::default()
        });
    Ok(GeometrySelection { props, face })
}
fn original_path(source: &Path, relative: &str) -> PathBuf {
    if relative.is_empty() {
        source.to_owned()
    } else {
        source.parent().unwrap_or(Path::new(".")).join(relative)
    }
}
pub fn verify_files(source: &str, old_source: &str, files: &[SourceProof]) -> Result<()> {
    if source.is_empty()
        || source::suffix(Path::new(source)) != source::suffix(Path::new(old_source))
    {
        return Err(invalid(
            "Source content or format changed; import and prepare again.",
        ));
    }
    for entry in files {
        let path = original_path(Path::new(source), &entry.relative);
        if digest(&path).ok().as_deref() != Some(entry.sha256.as_str()) {
            return Err(invalid(format!(
                "Changed or missing source input: {}",
                path.display()
            )));
        }
    }
    Ok(())
}
pub fn check_clothing_selection(
    project: &Project,
    record: Option<&ClothingSelection>,
) -> Result<()> {
    let current = clothing_selection(project)?;
    match (current, record) {
        (None, None) => Ok(()),
        (Some(now), Some(old)) if now.mode == old.mode => {
            if !old.source_files.is_empty() {
                verify_files(&now.source, &old.source, &old.source_files)
            } else if &now == old {
                Ok(())
            } else {
                Err(invalid(
                    "Clothing selection changed. Import and prepare the garment again.",
                ))
            }
        }
        _ => Err(invalid(
            "Clothing selection changed. Import and prepare the garment again.",
        )),
    }
}
fn relative_path(path: &Path, parent: &Path) -> Result<String> {
    if let Ok(relative) = path.strip_prefix(parent) {
        return path_text(relative);
    }
    let left: Vec<_> = path.components().collect();
    let right: Vec<_> = parent.components().collect();
    let common = left.iter().zip(&right).take_while(|(a, b)| a == b).count();
    if common == 0 {
        return path_text(path);
    }
    let mut result = PathBuf::new();
    for _ in common..right.len() {
        result.push("..");
    }
    for c in &left[common..] {
        result.push(c.as_os_str());
    }
    path_text(&result)
}
pub fn source_proof(original: &Path, staged: &Path) -> Result<Vec<SourceProof>> {
    let manifest: Vec<ManifestEntry> = serde_json::from_slice(&fs::read(
        staged
            .parent()
            .ok_or_else(|| invalid("Staged source has no parent."))?
            .join("source-manifest.json"),
    )?)?;
    let filename = original
        .file_name()
        .and_then(|v| v.to_str())
        .ok_or_else(|| invalid("Source filename cannot be encoded."))?;
    let mut files = Vec::new();
    for entry in manifest {
        files.push(SourceProof {
            relative: if entry.relative == filename {
                String::new()
            } else {
                entry.relative
            },
            sha256: entry.original_sha256.unwrap_or(entry.sha256),
        });
        for image in entry.packed_images.unwrap_or_default() {
            if image.origin == "already-packed" {
                continue;
            }
            files.push(SourceProof {
                relative: relative_path(Path::new(&image.origin), original.parent().unwrap())?,
                sha256: image.sha256,
            });
        }
    }
    Ok(files)
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PropRecord {
    pub selection: PropSelection,
    pub source: String,
    pub source_sha256: String,
    pub analysis: crate::service::Inspection,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct GeometryRecord {
    #[serde(default)]
    pub props: Vec<PropRecord>,
    pub face: Option<Face>,
}
pub fn check_customization_selection(project: &Project, record: &GeometryRecord) -> Result<()> {
    let current = customization_selection(project)?;
    if current.face != record.face || current.props.len() != record.props.len() {
        return Err(invalid(
            "Attached props or static facial shapes changed. Import and prepare again.",
        ));
    }
    for (now, entry) in current.props.iter().zip(&record.props) {
        let mut observed = now.clone();
        observed.source = None;
        let mut expected = entry.selection.choice.clone();
        expected.source = None;
        if observed != expected || entry.selection.source_files.is_empty() {
            return Err(invalid(
                "Attached prop selection changed or its old receipt has no source proof. Import and prepare again.",
            ));
        }
        verify_files(
            now.source.as_deref().unwrap_or_default(),
            entry.selection.choice.source.as_deref().unwrap_or_default(),
            &entry.selection.source_files,
        )?;
    }
    Ok(())
}
