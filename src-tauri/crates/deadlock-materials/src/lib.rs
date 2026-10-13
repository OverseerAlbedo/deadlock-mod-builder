//! Native material planning, VMAT generation/appearance edits and source assets.
#![forbid(unsafe_code)]
mod appearance;
mod cache;
pub mod color_adjust;
mod native_paths;
mod overlay;
mod planner;
mod source_zip;
mod texture_size;

pub use appearance::{apply_materials, patch_materials, patch_text, validate_appearance};
pub use cache::{
    cache_key_from_inputs, compiled_inventory, plan_cache, restore_cached, store_compiled,
    store_compiled_with, CachePlan, CacheStoreReceipt, EngineFingerprint, EngineInput,
    InventoryCache,
};
pub use deadlock_project::MaterialChoices;
pub use overlay::{effective_sources, texture_references, validate_materials, MaterialAudit};
pub use native_paths::{check_native_paths, NATIVE_PATH_LIMIT};
pub use planner::{generate, material_text, plan, plan_with_bindings, texture_candidates, CHANNELS, PRESETS};
pub use source_zip::{import_zip, SourcePackageSelection};
pub use texture_size::{normalize_staged_textures, TextureResize, TextureSizeReceipt, TEXTURE_SIZE_POLICY};
pub type Plans = std::collections::BTreeMap<String, MaterialChoices>;
pub type Hashes = std::collections::BTreeMap<String, String>;
pub type Result<T> = std::result::Result<T, Error>;

use deadlock_core::{safety, CancellationToken};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Validation(String),
    #[error(transparent)]
    Core(#[from] deadlock_core::CoreError),
    #[error(transparent)]
    Vpk(#[from] deadlock_vpk::Error),
    #[error(transparent)]
    Project(#[from] deadlock_project::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Zip(#[from] zip::result::ZipError),
}
fn invalid(message: impl Into<String>) -> Error {
    Error::Validation(message.into())
}
fn inside(root: &Path, relative: &str) -> Result<PathBuf> {
    deadlock_vpk::check_unlinked_path(root, true)?;
    let relative = safety::relative_asset(relative)?;
    deadlock_vpk::check_unlinked_path(&root.join(&relative), true)?;
    Ok(safety::inside(root, &relative)?)
}
fn read_text(path: &Path) -> Result<String> {
    deadlock_vpk::check_unlinked_path(path, false)?;
    let value = fs::read_to_string(path)?;
    Ok(value.strip_prefix('\u{feff}').unwrap_or(&value).to_owned())
}
fn path_text(path: &Path) -> Result<String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| invalid("Material filesystem path is not valid Unicode."))
}
fn walk_files(root: &Path, cancel: &CancellationToken) -> Result<Vec<PathBuf>> {
    deadlock_vpk::check_unlinked_path(root, false)?;
    let mut stack = vec![root.to_owned()];
    let mut result = Vec::new();
    while let Some(folder) = stack.pop() {
        for entry in fs::read_dir(folder)? {
            cancel.check()?;
            let entry = entry?;
            let path = entry.path();
            deadlock_vpk::check_unlinked_path(&path, false)?;
            let kind = entry.file_type()?;
            if kind.is_dir() {
                stack.push(path);
            } else if kind.is_file() {
                result.push(path);
            } else {
                return Err(invalid("Material source contains a special file."));
            }
        }
    }
    result.sort();
    Ok(result)
}
fn relative_file(root: &Path, path: &Path) -> Result<String> {
    Ok(path_text(
        path.strip_prefix(root)
            .map_err(|_| invalid("Material file escapes source root."))?,
    )?
    .replace('\\', "/"))
}
fn copy_file(source: &Path, destination: &Path, cancel: &CancellationToken) -> Result<()> {
    deadlock_vpk::check_unlinked_path(source, false)?;
    deadlock_vpk::check_unlinked_path(destination, true)?;
    safety::atomic_copy(source, destination, cancel)?;
    Ok(())
}
fn quote(value: &str) -> String {
    // json.dumps default ensure_ascii=True, including surrogate-pair escapes.
    let encoded = serde_json::to_string(value).expect("serializing a Rust string is infallible");
    let mut out = String::with_capacity(encoded.len());
    for c in encoded.chars() {
        if c.is_ascii() {
            out.push(c);
        } else {
            for unit in c.encode_utf16(&mut [0; 2]).iter() {
                out.push_str(&format!("\\u{unit:04x}"));
            }
        }
    }
    out
}
