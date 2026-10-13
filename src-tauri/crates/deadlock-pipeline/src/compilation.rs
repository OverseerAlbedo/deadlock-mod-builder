//! Compiler composition and verified material-cache reuse.
use crate::{invalid, workspace};
use deadlock_core::{
    process::{CommandSpec, ToolExecutor},
    safety, CancellationToken, CoreError, Result, TimingRecorder, TimingStatus,
};
use deadlock_project::{Project, Settings};
use deadlock_tools::compiler;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterialBuildReceipt {
    pub material: String,
    pub cache_key: String,
    pub reused: bool,
    pub cache_saved: bool,
    pub compiled_sha256: String,
}
pub struct CompilerService<'a> {
    pub settings: &'a Settings,
    pub runner: &'a dyn ToolExecutor,
    pub cancel: &'a CancellationToken,
    pub clock: &'a TimingRecorder,
    pub log: &'a dyn Fn(&str),
    /// Hashes of the compiled files, kept between materials of one build.
    pub inventory: std::cell::RefCell<deadlock_materials::InventoryCache>,
}
fn material_error(error: deadlock_materials::Error) -> CoreError {
    match error {
        deadlock_materials::Error::Core(e) => e,
        other => invalid(other.to_string()),
    }
}
impl CompilerService<'_> {
    pub fn compile_material(
        &self,
        project: &Project,
        material: &str,
        content: &Path,
        game: &Path,
        parent: Option<&str>,
    ) -> Result<MaterialBuildReceipt> {
        let span = self
            .clock
            .start_step(format!("Build material: {material}"), parent)?;
        let result = (|| {
            self.cancel.check()?;
            let target_game = deadlock_tools::toolchain::sdk_game(self.settings)?;
            let plan = self.clock.measure(
                "Fingerprint material and compiler inputs",
                Some(span.id()),
                |_| {
                    deadlock_materials::plan_cache(
                        material,
                        content,
                        Path::new(&self.settings.resourcecompiler),
                        Path::new(&self.settings.csdk),
                        self.cancel,
                    )
                    .map_err(material_error)
                },
            )?;
            let cache = workspace::root(self.settings, project)?
                .join("material-cache")
                .join(&plan.key);
            if plan.cacheable
                && self.clock.measure(
                    "Check and restore material cache",
                    Some(span.id()),
                    |_| {
                        deadlock_materials::restore_cached(&cache, material, game, self.cancel)
                            .map_err(material_error)
                    },
                )?
            {
                (self.log)(&format!("Reusing verified material: {material}"));
                return Ok(MaterialBuildReceipt {
                    material: material.into(),
                    cache_key: plan.key,
                    reused: true,
                    cache_saved: false,
                    compiled_sha256: safety::digest_with_cancel(
                        &safety::inside(game, &format!("{material}_c"))?,
                        self.cancel,
                    )?,
                });
            }
            let before = self.clock.measure(
                "Inventory compiled material dependencies",
                Some(span.id()),
                |_| {
                    self.inventory
                        .borrow_mut()
                        .inventory(game, self.cancel)
                        .map_err(material_error)
                },
            )?;
            let source = safety::inside(content, material)?;
            let output = safety::inside(game, &format!("{material}_c"))?;
            let compiled = compiler::compile_resource(
                self.runner,
                Path::new(&self.settings.resourcecompiler),
                &source,
                &output,
                Some(&target_game),
                self.clock,
                Some(span.id()),
                self.cancel,
            )?;
            let saved = if plan.cacheable {
                self.clock
                    .measure("Save verified material cache", Some(span.id()), |_| {
                        let after = self
                            .inventory
                            .borrow_mut()
                            .inventory(game, self.cancel)
                            .map_err(material_error)?;
                        deadlock_materials::store_compiled_with(
                            &cache,
                            material,
                            game,
                            &before,
                            after,
                            self.cancel,
                        )
                        .map_err(material_error)
                    })?
                    .cacheable
            } else {
                self.clock.skip_step(
                    "Save material cache",
                    Some(span.id()),
                    "Material has nested or unresolved source dependencies",
                )?;
                false
            };
            Ok(MaterialBuildReceipt {
                material: material.into(),
                cache_key: plan.key,
                reused: false,
                cache_saved: saved,
                compiled_sha256: compiled.output_sha256,
            })
        })();
        match result {
            Ok(receipt) => {
                if receipt.reused {
                    span.cached()?;
                } else {
                    span.success()?;
                }
                Ok(receipt)
            }
            Err(error) => {
                let _ = span.finish(
                    if matches!(error, CoreError::Cancelled) {
                        TimingStatus::Cancelled
                    } else {
                        TimingStatus::Failed
                    },
                    Some(error.to_string()),
                );
                Err(error)
            }
        }
    }
    pub fn adapter(&self, project: &Project, content: &Path, game: &Path) -> Result<()> {
        self.clock
            .measure("Compile using model adapter", None, |span| {
                let adapter = Path::new(&self.settings.ag2_adapter);
                if !adapter.is_file() {
                    return Err(invalid(
                        "AG2 adapter is selected but no adapter executable is configured in Setup.",
                    ));
                }
                let job = workspace::root(self.settings, project)?.join("ag2-job.json");
                let source = safety::inside(content, project.source_model())?;
                let output = safety::inside(game, &format!("{}_c", project.source_model()))?;
                match std::fs::remove_file(&output) {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => return Err(CoreError::io("Remove stale adapter output", e)),
                }
                #[derive(Serialize)]
                struct AdapterJob<'a> {
                    protocol: u32,
                    source_model: &'a Path,
                    compiled_model: &'a Path,
                    original_resource: &'a str,
                    original_pak: &'a str,
                    content_root: &'a Path,
                    game_root: &'a Path,
                    resourcecompiler: &'a str,
                }
                safety::write_json(
                    &job,
                    &AdapterJob {
                        protocol: 1,
                        source_model: &source,
                        compiled_model: &output,
                        original_resource: &project.original_model,
                        original_pak: &self.settings.pak,
                        content_root: content,
                        game_root: game,
                        resourcecompiler: &self.settings.resourcecompiler,
                    },
                )?;
                self.clock
                    .measure("Run explicit model adapter", Some(span.id()), |_| {
                        self.runner.run(&CommandSpec::new(adapter).arg(&job))
                    })?;
                if !output.is_file() {
                    return Err(invalid(
                        "AG2 adapter did not create the replacement compiled model.",
                    ));
                }
                let raw =
                    std::fs::read(&output).map_err(|e| CoreError::io("Read adapter output", e))?;
                let layout = deadlock_vpk::resource_blocks(&raw).map_err(crate::vpk_error)?;
                if !layout.blocks.iter().any(|b| b.name == "DATA") {
                    return Err(invalid("Adapter output has no model DATA block."));
                }
                Ok(())
            })
    }
}

/// Refuse inputs inside generated trees before deleting any builder-owned data.
pub fn reject_generated_inputs(project: &Project, folders: &[PathBuf]) -> Result<()> {
    let mut paths = vec![
        project.dmx.clone(),
        project.textures.clone(),
        project.imported_source.clone(),
        project.original_compiled.clone(),
        project.source_overlay.clone(),
        project.mesh_template.clone(),
        project.material_mapping.clone(),
        project.material_remap.clone(),
        project.skeleton_reference.clone(),
    ];
    for material in project.materials.values() {
        if let Some(textures) = &material.textures {
            paths.extend(textures.values().cloned());
        }
    }
    let mut state = project.customization.clone();
    state
        .map_sources(|path| {
            paths.push(path.into());
            Ok(path.into())
        })
        .map_err(|e| invalid(e.to_string()))?;
    let roots = folders
        .iter()
        .map(|p| deadlock_project::asset_paths::resolve(p).map_err(|e| invalid(e.to_string())))
        .collect::<Result<Vec<_>>>()?;
    for path in paths.into_iter().filter(|p| !p.is_empty()) {
        let path = deadlock_project::asset_paths::resolve(Path::new(&path))
            .map_err(|e| invalid(e.to_string()))?;
        if roots.iter().any(|root| path.starts_with(root)) {
            return Err(invalid(
                "Keep input DMX/textures outside the builder-owned generated addon folders.",
            ));
        }
    }
    Ok(())
}

pub fn clear_owned(
    project: &Project,
    folders: &[PathBuf],
    cancel: &CancellationToken,
) -> Result<()> {
    reject_generated_inputs(project, folders)?;
    // Validate ownership and every existing descendant in all roots before the
    // first removal. Unowned/nonempty directories are never adopted.
    for folder in folders {
        deadlock_vpk::check_unlinked_path(folder, true).map_err(crate::vpk_error)?;
        if folder.exists() {
            let marker = folder.join(".deadlock-builder.json");
            let items = std::fs::read_dir(folder)
                .map_err(|e| CoreError::io("Inspect generated folder", e))?
                .count();
            if items > 0 {
                let saved: serde_json::Value =
                    serde_json::from_slice(&std::fs::read(&marker).map_err(|_| {
                        invalid(format!(
                            "Refusing to clear an unowned generated directory: {}",
                            folder.display()
                        ))
                    })?)?;
                if saved.get("project").and_then(|v| v.as_str()) != Some(&project.id) {
                    return Err(invalid(format!(
                        "Refusing to clear another project's generated directory: {}",
                        folder.display()
                    )));
                }
            }
            crate::sources::files(folder, cancel)?;
        }
    }
    for folder in folders {
        cancel.check()?;
        std::fs::create_dir_all(folder).map_err(|e| CoreError::io("Create generated folder", e))?;
        for entry in
            std::fs::read_dir(folder).map_err(|e| CoreError::io("Inspect generated folder", e))?
        {
            cancel.check()?;
            let path = entry
                .map_err(|e| CoreError::io("Inspect generated entry", e))?
                .path();
            if path.is_dir() {
                deadlock_vpk::remove_snapshot(&path).map_err(crate::vpk_error)?;
            } else {
                std::fs::remove_file(path).map_err(|e| CoreError::io("Reset generated file", e))?;
            }
        }
        safety::write_json(
            &folder.join(".deadlock-builder.json"),
            &BTreeMap::from([("project", project.id.as_str()), ("builder", "rust-0.1.0")]),
        )?;
    }
    Ok(())
}
