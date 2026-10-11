//! Shared native file intake used by browse, manual entry and Explorer drops.
use crate::app::{AppError, Result};
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetKind {
    TextureDirectory,
    Texture,
    Physics,
    Garment,
    Icon,
    Animation,
    Effect,
    Audio,
    Prop,
    VariantProject,
}

pub fn validate_asset_path(path: &Path, kind: AssetKind) -> Result<String> {
    let path = path
        .canonicalize()
        .map_err(|error| AppError::io("Choose an existing local file or folder", error))?;
    let valid = match kind {
        AssetKind::TextureDirectory => path.is_dir(),
        _ => {
            let suffix = path
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();
            let allowed: &[&str] = match kind {
                AssetKind::Physics => &["vmdl_c"],
                AssetKind::Garment | AssetKind::Prop => &["blend", "glb", "gltf", "fbx"],
                AssetKind::Icon => &["png", "jpg", "jpeg"],
                AssetKind::Texture => &["png", "tga", "jpg", "jpeg", "tif", "tiff", "exr"],
                AssetKind::Animation => &["vnmclip", "vnmclip_c", "blend", "fbx", "glb", "gltf"],
                AssetKind::Effect => &["vpcf_c"],
                AssetKind::Audio => &["wav", "mp3", "flac", "ogg", "vsnd_c"],
                AssetKind::VariantProject => &["json"],
                AssetKind::TextureDirectory => unreachable!(),
            };
            path.is_file() && allowed.contains(&suffix.as_str())
        }
    };
    if !valid {
        return Err(AppError::validation(
            "Unsupported file or folder",
            format!("{} is not supported for {kind:?}", path.display()),
        ));
    }
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| AppError::validation("Unsupported path", "Choose a Unicode filename."))
}
