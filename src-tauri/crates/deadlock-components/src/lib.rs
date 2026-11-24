//! Explicit cosmetic replacements preserve each original resource contract.
#![forbid(unsafe_code)]
pub mod ability_objects;
pub mod audio;
pub mod block_refit;
pub mod clip_names;
pub mod color_ranges;
pub mod effect_build;
pub mod effect_catalog;
pub mod effect_colors;
pub mod effect_edit;
pub mod effect_trees;
pub mod hero_clips;
pub mod heroes;
pub mod icons;
pub mod icon_catalog;
pub mod item_icons;
pub mod item_names;
pub mod keep_blocks;
pub mod write_back;
pub mod made_clips;
pub mod materials;
pub mod vector_icons;
pub mod motion;
pub mod object_fit;
pub mod object_follow;
pub mod object_glow_shells;
pub mod object_morphs;
pub mod object_parts;
pub mod object_states;
pub mod resources;
pub mod service;
pub mod sound_tags;
pub mod texture_recolor;
pub mod variants;
pub mod vpk_contents;
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
/// The engine's own archive beside a game archive (`game/core/pak01_dir.vpk`
/// for `game/citadel/pak01_dir.vpk`), when it exists. Shared engine assets
/// that game files reference (particle textures and materials among them) live
/// there, so a reference found in it is provided by the game.
pub fn engine_archive(game: &Path) -> Option<deadlock_vpk::Vpk> {
    let core = game.parent()?.parent()?.join("core").join("pak01_dir.vpk");
    core.is_file().then(|| deadlock_vpk::Vpk::open(&core).ok()).flatten()
}
/// Whether `path` (a compiled path) is in the game archive or its engine archive.
pub fn provided(archive: &deadlock_vpk::Vpk, engine: Option<&deadlock_vpk::Vpk>, path: &str) -> bool {
    let key = deadlock_core::text::casefold(path);
    archive.entries.contains_key(&key) || engine.is_some_and(|e| e.entries.contains_key(&key))
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
