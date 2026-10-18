//! Explicit cosmetic replacements preserve each original resource contract.
#![forbid(unsafe_code)]
pub mod audio;
pub mod heroes;
pub mod icons;
pub mod icon_catalog;
pub mod materials;
pub mod vector_icons;
pub mod motion;
pub mod resources;
pub mod service;
pub mod variants;
pub use deadlock_core::{CancellationToken, CoreError, Result};
use sha2::{Digest, Sha256};
use std::path::Path;
pub const MAX_RESOURCE_BYTES: usize = 128 * 1024 * 1024;
pub(crate) fn fail(value: impl std::fmt::Display) -> CoreError {
    CoreError::Integrity(value.to_string())
}
pub(crate) fn sha(raw: &[u8]) -> String {
    format!("{:x}", Sha256::digest(raw))
}
pub fn resource_fingerprint(raw: &[u8]) -> String { sha(raw) }
pub fn compiled(path: &str) -> Result<String> {
    let value = deadlock_core::safety::relative_asset(path)?;
    Ok(if deadlock_core::text::casefold(&value).ends_with("_c") {
        value
    } else {
        value + "_c"
    })
}
pub(crate) fn read(archive: &deadlock_vpk::Vpk, path: &str) -> Result<Vec<u8>> {
    let entry = archive
        .entries
        .get(&deadlock_core::text::casefold(path))
        .ok_or_else(|| {
            fail(format!(
                "Character asset is absent from the current game VPK: {path}"
            ))
        })?;
    if entry.length as usize + entry.preload.len() > MAX_RESOURCE_BYTES {
        return Err(fail("Character asset exceeds the supported 128 MiB limit."));
    }
    archive.read(&entry.path).map_err(fail)
}
pub(crate) fn read_file(path: &Path) -> Result<Vec<u8>> {
    deadlock_vpk::check_unlinked_path(path, false).map_err(fail)?;
    if !path.is_file() || std::fs::metadata(path).map_err(fail)?.len() > MAX_RESOURCE_BYTES as u64 {
        return Err(fail(
            "Choose an existing replacement file smaller than 128 MiB.",
        ));
    }
    std::fs::read(path).map_err(fail)
}
