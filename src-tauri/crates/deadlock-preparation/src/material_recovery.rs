//! Read-only image recovery for already prepared projects. A content-keyed cache
//! avoids reopening Blender on each material review and works without Blender
//! once the images are recovered. Geometry and DMX files are never rewritten.
use crate::{
    blender::{self, BlenderBridge, Request},
    invalid, path_text, Result,
};
use deadlock_core::{safety, CancellationToken};
use deadlock_project::{asset_paths, MaterialChoices};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::Path};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterialRecovery {
    pub status: String,
    pub materials: BTreeMap<String, MaterialChoices>,
    #[serde(default)]
    pub warnings: Vec<String>,
}
fn cached_images_exist(value: &MaterialRecovery, destination: &Path) -> bool {
    value.status == "ok"
        && value.materials.values().all(|material| {
            let chosen = material.textures.iter().flat_map(|m| m.values());
            let candidates = material
                .candidates
                .iter()
                .flat_map(|m| m.values())
                .flatten();
            chosen.chain(candidates).all(|p| {
                let p = Path::new(p);
                !p.as_os_str().is_empty()
                    && p.starts_with(destination.join("textures"))
                    && p.is_file()
            })
        })
}
pub fn recover(
    bridge: &BlenderBridge,
    source: &Path,
    hero_root: &str,
    cache_root: &Path,
    cancel: &CancellationToken,
) -> Result<MaterialRecovery> {
    cancel.check()?;
    let source = asset_paths::resolve(source)?;
    let source_hash = safety::digest_with_cancel(&source, cancel)?;
    let mut digest = Sha256::new();
    digest.update(source_hash.as_bytes());
    digest.update(hero_root.as_bytes());
    digest.update(include_bytes!("../resources/blender/material_textures.py"));
    digest.update(include_bytes!("../resources/blender/worker.py"));
    digest.update(include_bytes!("../resources/blender/source_rigs.py"));
    digest.update(include_bytes!("../resources/blender/scene_isolation.py"));
    let key = format!("{:x}", digest.finalize());
    let destination = asset_paths::resolve(&cache_root.join(&key[..28]))?;
    let receipt = destination.join("blender-result.json");
    if let Ok(bytes) = fs::read(&receipt) {
        if let Ok(cached) = serde_json::from_slice::<MaterialRecovery>(&bytes) {
            if cached_images_exist(&cached, &destination) {
                return Ok(cached);
            }
        }
    }
    blender::validate(&bridge.executable, bridge.executor.as_ref())?;
    let recovered: MaterialRecovery = bridge.execute(
        &Request::RecoverMaterials {
            source: path_text(&source)?,
            hero_root: hero_root.into(),
        },
        &destination,
    )?;
    if safety::digest_with_cancel(&source, cancel)? != source_hash {
        return Err(invalid(
            "Prepared Blender file changed during texture analysis. Analyze materials again.",
        ));
    }
    if !cached_images_exist(&recovered, &destination) {
        return Err(invalid(
            "Recovered material images are missing or outside the texture workspace.",
        ));
    }
    Ok(recovered)
}
