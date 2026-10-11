//! Hash-bound, non-mutating source staging and local glTF dependency closure.
use crate::{Result, invalid, path_text, write_json};
use deadlock_project::asset_paths;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::Read,
    path::{Component, Path, PathBuf},
};
pub const SOURCE_SUFFIXES: [&str; 4] = ["glb", "gltf", "fbx", "blend"];
pub fn suffix(path: &Path) -> String {
    path.extension()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_lowercase()
}
pub fn normalized_input(value: &str) -> Result<PathBuf> {
    let mut value = value.trim();
    if value.len() >= 2
        && ((value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\'')))
    {
        value = &value[1..value.len() - 1];
    }
    if value.trim().is_empty() {
        return Err(invalid(
            "No source model selected. Choose a replacement Blender, GLB, glTF or FBX file in Prepare.",
        ));
    }
    if value.contains(['\0', '\n', '\r']) {
        return Err(invalid("Invalid source path: null or line break."));
    }
    let path =
        if value == "~" || value.starts_with("~/") || cfg!(windows) && value.starts_with("~\\") {
            let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
                .ok_or_else(|| invalid("Cannot resolve the user home directory."))?;
            PathBuf::from(home).join(value.get(2..).unwrap_or_default())
        } else {
            PathBuf::from(value)
        };
    Ok(asset_paths::resolve(&path)?)
}
pub fn resolve_source(value: &str) -> Result<PathBuf> {
    let path = normalized_input(value)?;
    let metadata = fs::metadata(&path).map_err(|e| {
        invalid(format!(
            "Source model cannot be accessed: {e}\nReceived: {value:?}\nResolved: {path:?}"
        ))
    })?;
    if !metadata.is_file() {
        return Err(invalid(
            "Source model must be a regular file, not a directory.",
        ));
    }
    if !SOURCE_SUFFIXES.contains(&suffix(&path).as_str()) {
        return Err(invalid(format!(
            "Unsupported source extension. Select the replacement Blender, GLB, glTF or FBX file: {}",
            path.display()
        )));
    }
    Ok(path)
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PackedImage {
    pub origin: String,
    pub sha256: String,
    #[serde(flatten)]
    pub details: std::collections::BTreeMap<String, serde_json::Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ManifestEntry {
    pub relative: String,
    pub sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub packed_images: Option<Vec<PackedImage>>,
}
pub trait BlendStager {
    fn stage_blend(&self, source: &Path, destination: &Path) -> Result<Vec<PackedImage>>;
}
fn decode_uri(uri: &str) -> Result<String> {
    let bytes = uri.as_bytes();
    let mut output = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(a), Some(b)) = (
                (bytes[i + 1] as char).to_digit(16),
                (bytes[i + 2] as char).to_digit(16),
            ) {
                let value = (a * 16 + b) as u8;
                output.push(value);
                i += 3;
                continue;
            }
        }
        output.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(output)
        .map_err(|_| invalid("glTF external dependency URI is not valid UTF-8."))
}
fn dependency(source: &Path, uri: &str) -> Result<PathBuf> {
    let value = decode_uri(uri)?;
    let relative = Path::new(&value);
    let scheme = uri.split_once(':').is_some_and(|(before, _)| {
        !before.is_empty()
            && before
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic())
            && before
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || ['+', '-', '.'].contains(&c))
    });
    if scheme
        || uri.starts_with("//")
        || relative.is_absolute()
        || relative
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::Prefix(_)))
        || uri.contains('\\')
        || value.contains('\\')
        || value.contains('\0')
    {
        return Err(invalid(format!(
            "glTF external dependencies must be local paths inside the source folder: {uri}"
        )));
    }
    let parent = source
        .parent()
        .ok_or_else(|| invalid("Source has no parent directory."))?;
    let path = asset_paths::resolve(&parent.join(relative))?;
    if !path.starts_with(parent) || !path.is_file() {
        return Err(invalid(format!(
            "Missing or escaping glTF dependency: {uri}"
        )));
    }
    Ok(path)
}
fn gltf_data(source: &Path) -> Result<serde_json::Value> {
    if suffix(source) == "gltf" {
        return Ok(serde_json::from_reader(File::open(source)?)?);
    }
    let mut input = File::open(source)?;
    let mut header = [0; 20];
    input
        .read_exact(&mut header)
        .map_err(|_| invalid("Truncated GLB header."))?;
    let word = |i| u32::from_le_bytes(header[i..i + 4].try_into().unwrap());
    let length = word(12);
    if word(0) != 0x46546c67
        || word(4) != 2
        || word(16) != 0x4e4f534a
        || word(8) as u64 != input.metadata()?.len()
        || length > 64 * 1024 * 1024
    {
        return Err(invalid("Unsupported or invalid GLB 2 container."));
    }
    let mut bytes = vec![0; length as usize];
    input.read_exact(&mut bytes)?;
    Ok(serde_json::from_slice(&bytes)?)
}
fn collect_textures(
    directory: &Path,
    files: &mut BTreeSet<PathBuf>,
    cancel: &deadlock_core::CancellationToken,
) -> Result<()> {
    for entry in fs::read_dir(directory)? {
        cancel.check()?;
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_symlink() {
            continue;
        }
        if ty.is_dir() {
            collect_textures(&entry.path(), files, cancel)?;
        } else if ty.is_file()
            && ["png", "jpg", "jpeg", "tga", "tif", "tiff", "bmp"]
                .contains(&suffix(&entry.path()).as_str())
        {
            files.insert(entry.path());
        }
        if files.len() > 5000 {
            return Err(invalid(
                "FBX texture folder too large. Export this character into a dedicated folder.",
            ));
        }
    }
    Ok(())
}
pub fn stage_source(
    value: &str,
    destination: &Path,
    blend_stager: Option<&dyn BlendStager>,
) -> Result<PathBuf> {
    stage_source_with_cancel(
        value,
        destination,
        blend_stager,
        &deadlock_core::CancellationToken::new(),
    )
}
pub fn stage_source_with_cancel(
    value: &str,
    destination: &Path,
    blend_stager: Option<&dyn BlendStager>,
    cancel: &deadlock_core::CancellationToken,
) -> Result<PathBuf> {
    cancel.check()?;
    let digest = |path: &Path| {
        deadlock_core::safety::digest_with_cancel(path, cancel).map_err(crate::Error::from)
    };
    let source = resolve_source(value)?;
    let destination = asset_paths::resolve(destination)?;
    let filename = source
        .file_name()
        .ok_or_else(|| invalid("Source filename missing."))?;
    let local = destination.join(filename);
    if local == source {
        return Err(invalid(
            "The source and staged copy must be different files.",
        ));
    }
    fs::create_dir_all(&destination)?;
    if suffix(&source) == "blend" {
        let bridge=blend_stager.ok_or_else(||invalid("Blender files need the configured Blender importer to pack their textures. Use Import and Analyze in Prepare."))?;
        let before = digest(&source)?;
        let packed_images = bridge.stage_blend(&source, &local)?;
        if digest(&source)? != before {
            return Err(invalid("Source changed during import."));
        }
        if !local.is_file() {
            return Err(invalid(
                "Blender did not create the packed source copy. See the Blender log.",
            ));
        }
        write_json(
            &destination.join("source-manifest.json"),
            &vec![ManifestEntry {
                relative: path_text(Path::new(filename))?,
                sha256: digest(&local)?,
                original_sha256: Some(before),
                transform: Some("blender-packed-source-v1".into()),
                packed_images: Some(packed_images),
            }],
        )?;
        return Ok(local);
    }
    let mut files = BTreeSet::from([source.clone()]);
    if ["gltf", "glb"].contains(&suffix(&source).as_str()) {
        let data = gltf_data(&source)?;
        for kind in ["buffers", "images"] {
            if let Some(items) = data.get(kind) {
                let items = items
                    .as_array()
                    .ok_or_else(|| invalid("glTF buffers and images must be arrays."))?;
                for item in items {
                    let Some(uri) = item.get("uri") else { continue };
                    let uri = uri
                        .as_str()
                        .ok_or_else(|| invalid("glTF dependency URI must be text."))?;
                    if uri.is_empty() || uri.starts_with("data:") {
                        continue;
                    }
                    files.insert(dependency(&source, uri)?);
                }
            }
        }
    }
    if suffix(&source) == "fbx" {
        collect_textures(source.parent().unwrap(), &mut files, cancel)?;
    }
    let mut manifest = Vec::new();
    for file in files {
        cancel.check()?;
        let relative = file
            .strip_prefix(source.parent().unwrap())
            .map_err(|_| invalid("Escaping source dependency."))?;
        let dest = destination.join(relative);
        if asset_paths::resolve(&dest)? == file {
            return Err(invalid(
                "Source and staged dependency must be different files.",
            ));
        }
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        let before = digest(&file)?;
        deadlock_core::safety::atomic_copy(&file, &dest, cancel)?;
        if digest(&dest)? != before || digest(&file)? != before {
            return Err(invalid(format!(
                "Source changed during import: {}",
                file.display()
            )));
        }
        manifest.push(ManifestEntry {
            relative: path_text(relative)?.replace(std::path::MAIN_SEPARATOR, "/"),
            sha256: before,
            original_sha256: None,
            transform: None,
            packed_images: None,
        });
    }
    write_json(&destination.join("source-manifest.json"), &manifest)?;
    Ok(local)
}
