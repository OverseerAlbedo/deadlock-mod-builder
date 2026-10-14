//! Native preparation orchestration. Python is used only by Blender's embedded
//! interpreter for the unchanged, qualified authoring algorithms.
#![forbid(unsafe_code)]
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read, path::Path};
pub mod algorithms;
pub mod automap;
pub mod arm_helpers;
pub mod leg_chains;
pub mod blender;
pub mod stance;
pub mod following;
pub mod fitting;
pub mod mapping_presets;
pub mod material_recovery;
pub mod provenance;
pub mod service;
pub mod source;
pub mod secondary_bones;
pub mod structure;
pub mod weapon;
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(
        "No Blender path has been saved on this PC. If a path is already shown in Settings, click Save settings or Validate Blender to apply it. Otherwise open Settings → Advanced configuration → Blender executable and select blender.exe first. Blender is required for model preparation and Blender/FBX previews; an already prepared DMX does not need it."
    )]
    BlenderNotConfigured,
    #[error(
        "The saved Blender path does not point to a file on this PC: {0}\nOpen Settings → Advanced configuration → Blender executable, select the installed blender.exe, then click Save settings."
    )]
    BlenderNotFound(String),
    #[error("{0}")]
    Validation(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Project(#[from] deadlock_project::Error),
    #[error(transparent)]
    Core(#[from] deadlock_core::CoreError),
}
pub type Result<T> = std::result::Result<T, Error>;
pub(crate) fn invalid(message: impl Into<String>) -> Error {
    Error::Validation(message.into())
}
pub fn digest(path: &Path) -> Result<String> {
    let mut input = File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let n = input.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        digest.update(&buffer[..n]);
    }
    Ok(format!("{:x}", digest.finalize()))
}
pub(crate) fn path_text(path: &Path) -> Result<String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| invalid("Path cannot be represented losslessly as Unicode."))
}
pub(crate) fn write_json(path: &Path, value: &impl serde::Serialize) -> Result<()> {
    let mut data = serde_json::to_vec_pretty(value)?;
    data.push(b'\n');
    deadlock_core::safety::atomic_write(path, &data).map_err(Error::from)
}
