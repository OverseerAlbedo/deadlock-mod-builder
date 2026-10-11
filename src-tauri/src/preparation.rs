//! Native application entry points for optional Blender preparation and outfit files.
//! Desktop commands dispatch these blocking methods onto Tauri's worker runtime.
use crate::app::{AppError, Application, Result};
use deadlock_core::{process::Runner, CancellationToken, CoreError, TimingRecorder};
use deadlock_preparation::{
    algorithms::{self, Bone, ReviewIssue},
    blender::BlenderBridge,
    service::{self, Analysis, PreparationResult, PreparationService, Receipt},
};
use deadlock_project::{asset_paths, presets::OutfitPreset, PreparationOptions, Project};
use deadlock_skeleton::Skeleton;
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Arc,
};

pub(crate) fn failure(error: deadlock_preparation::Error, cancel: &CancellationToken) -> AppError {
    if cancel.is_cancelled() {
        CoreError::Cancelled.into()
    } else {
        match error {
            error @ deadlock_preparation::Error::BlenderNotConfigured => {
                AppError::validation("Blender is not configured", error)
            }
            error @ deadlock_preparation::Error::BlenderNotFound(_) => {
                AppError::validation("Blender executable not found", error)
            }
            deadlock_preparation::Error::Core(error) => error.into(),
            other => AppError::validation("Preparation could not be completed", other),
        }
    }
}
fn text(path: &Path) -> Result<String> {
    asset_paths::text(path)
        .map_err(|e| AppError::validation("Path cannot be represented without loss", e))
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResumedPreparation {
    pub analysis: Analysis,
    pub result: PreparationResult,
    pub report: PreparationReview,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreparedMeshSummary {
    pub name: String,
    pub vertices: usize,
    pub polygons: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreparationReview {
    pub target_model: String,
    pub dmx_sha256: String,
    pub options: PreparationOptions,
    pub warnings: Vec<String>,
    pub attached_prop_meshes: Vec<String>,
    pub materials: std::collections::BTreeMap<String, deadlock_project::MaterialChoices>,
    pub meshes: Vec<PreparedMeshSummary>,
    pub rest_alignment_policy: String,
    pub native_compiler_validated: bool,
    pub in_game_validated: bool,
}
impl From<&Receipt> for PreparationReview {
    fn from(receipt: &Receipt) -> Self {
        Self {
            target_model: receipt.target_model.clone(),
            dmx_sha256: receipt.dmx_sha256.clone(),
            options: receipt.options.clone(),
            warnings: receipt.prepared.warnings.clone(),
            attached_prop_meshes: receipt.attached_prop_meshes.clone(),
            materials: receipt.prepared.materials.clone(),
            meshes: receipt
                .prepared
                .meshes
                .iter()
                .map(|mesh| PreparedMeshSummary {
                    name: mesh.name.clone(),
                    vertices: mesh.positions.len(),
                    polygons: mesh.faces.len(),
                })
                .collect(),
            rest_alignment_policy: receipt.prepared.rest_alignment.policy.clone(),
            native_compiler_validated: receipt.native_compiler_validated,
            in_game_validated: receipt.in_game_validated,
        }
    }
}

impl Application {
    pub fn preparation_suggest(
        &self,
        analysis: &Analysis,
        options: &PreparationOptions,
    ) -> Result<algorithms::Suggestions> {
        let target = analysis.target.iter().map(Bone::from).collect::<Vec<_>>();
        let mut choices = options.clone();
        let mut preset_note = None;
        if deadlock_preparation::following::map(options, "mapping").is_empty() {
            let key = deadlock_preparation::mapping_presets::key(analysis)
                .map_err(|e| AppError::validation("Mapping identity could not be read", e))?;
            let path = self
                .state_directory
                .join("mapping-presets")
                .join(format!("{key}.json"));
            if path.is_file() {
                let load = || -> deadlock_preparation::Result<PreparationOptions> {
                    let bytes = std::fs::read(&path)?;
                    deadlock_preparation::mapping_presets::restore(
                        serde_json::from_slice(&bytes)?,
                        &key,
                    )
                };
                match load() {
                    Ok(saved) => {
                        choices = saved;
                        preset_note = Some("Saved mapping for this rig pair loaded. Review the model before conversion.".into());
                    }
                    Err(_) => {
                        preset_note = Some(
                            "Saved mapping could not be used. Fresh suggestions were generated."
                                .into(),
                        );
                    }
                }
            }
        }
        let mut result = deadlock_preparation::automap::suggest(
            &analysis.analysis.bones,
            &analysis.analysis.weighted_bones,
            &target,
            &choices,
        )
        .map_err(|e| AppError::validation("Bone suggestions could not be generated", e))?;
        if let Some(assistance) = result.assistance.as_mut() {
            assistance.preset = preset_note;
        }
        Ok(result)
    }
    pub fn preparation_save_mapping_preset(
        &self,
        analysis: &Analysis,
        options: &PreparationOptions,
    ) -> Result<()> {
        let preset = deadlock_preparation::mapping_presets::capture(analysis, options)
            .map_err(|e| AppError::validation("Mapping preset could not be saved", e))?;
        let root = self.state_directory.join("mapping-presets");
        deadlock_vpk::check_unlinked_path(&root, true)
            .map_err(|e| AppError::validation("Invalid preset directory", e))?;
        std::fs::create_dir_all(&root)
            .map_err(|e| AppError::validation("Preset directory could not be created", e))?;
        let path = root.join(format!("{}.json", preset.fingerprint));
        deadlock_vpk::check_unlinked_path(&path, true)
            .map_err(|e| AppError::validation("Invalid preset path", e))?;
        let bytes = serde_json::to_vec_pretty(&preset)
            .map_err(|e| AppError::validation("Invalid mapping preset", e))?;
        deadlock_core::safety::atomic_write(&path, &bytes)?;
        Ok(())
    }
    fn preparation_service(
        &self,
        settings: &deadlock_project::Settings,
        project: &Project,
        clock: &TimingRecorder,
        runner: Runner,
    ) -> Result<PreparationService> {
        let root = deadlock_pipeline::workspace::root(settings, project)?;
        Ok(PreparationService::new(
            root,
            BlenderBridge {
                executable: PathBuf::from(&settings.blender),
                executor: Arc::new(runner),
            },
        )
        .with_timing(clock.clone(), None))
    }
    /// Always inspect the selected original again. A frontend-supplied skeleton
    /// or an old JSON receipt never authorizes a new target rig.
    fn preparation_original(
        &self,
        settings: &deadlock_project::Settings,
        project: &Project,
        clock: &TimingRecorder,
        cancel: &CancellationToken,
        runner: &Runner,
    ) -> Result<(PathBuf, Skeleton, String)> {
        let log = |message: &str| {
            let _ = self.append_log("info", message);
        };
        let reader_root = self.native_reader_root();
        let pipeline = deadlock_pipeline::inspect::ModelPipeline {
            settings,
            runner,
            cancel,
            clock,
            log: &log,
            reader_root: &reader_root,
        };
        let original = pipeline.inspect_original(project, None)?;
        let model_text = original.verified_source(cancel)?;
        let compiled = original.compiled_snapshot.clone().ok_or_else(|| {
            AppError::validation(
                "Select the original compiled hero",
                "Choose the current game VPK in Setup or the offline original compiled model.",
            )
        })?;
        let raw = original.compiled_bytes(cancel)?;
        let skeleton = self.step(clock, "Read original target skeleton", || {
            cancel.check()?;
            let blocks = deadlock_resource::container::read_blocks(&raw, "DATA")
                .map_err(|e| AppError::validation("Original hero DATA could not be read", e))?;
            if blocks.len() != 1 {
                return Err(AppError::validation(
                    "Original hero has no unique target skeleton",
                    "Exactly one embedded DATA block is required.",
                ));
            }
            let value = blocks[0].root.get("m_modelSkeleton").ok_or_else(|| {
                AppError::validation(
                    "Original hero has no target skeleton",
                    "m_modelSkeleton is missing from the selected original DATA.",
                )
            })?;
            let skeleton = Skeleton::from_value(value)
                .map_err(|e| AppError::validation("Original skeleton failed validation", e))?;
            cancel.check()?;
            Ok(skeleton)
        })?;
        Ok((compiled, skeleton, model_text))
    }
    /// Measure the original model's gun from its decompiled sources. Never
    /// blocks inspection: a model without readable gun data simply has none.
    fn weapon_reference(
        &self,
        settings: &deadlock_project::Settings,
        project: &Project,
        skeleton: &Skeleton,
        model_text: &str,
        clock: &TimingRecorder,
        cancel: &CancellationToken,
    ) -> Option<deadlock_preparation::weapon::Reference> {
        let result = self.step(clock, "Measure the original gun", || {
            cancel.check()?;
            let base = deadlock_pipeline::sources::base_dir(settings, project)?;
            let (target, _) = deadlock_skeleton::transforms::prepare_target(skeleton)
                .map_err(|e| AppError::validation("Original skeleton failed validation", e))?;
            let separate = project.model_input_mode == deadlock_project::ModelInputMode::Weapon;
            deadlock_preparation::weapon::reference_from_sources(&base, model_text, &target, separate, cancel)
                .map_err(|e| AppError::validation("The original gun could not be measured", e))
        });
        match result {
            Ok(reference) => Some(reference),
            Err(error) => {
                let _ = self.append_log("warning", &format!("Gun customization unavailable for this model: {error}"));
                None
            }
        }
    }
    pub fn preparation_inspect(
        &self,
        project: Project,
        source: String,
        rotation: [f64; 3],
    ) -> Result<Analysis> {
        self.operation("Import and analyze preparation source",|clock,cancel|{
            project.validate_action().map_err(|e|AppError::validation("Select the target hero before preparation",e))?;
            let settings=self.settings()?;
            deadlock_preparation::blender::check_executable(Path::new(&settings.blender)).map_err(|e|failure(e,cancel))?;
            let root=deadlock_pipeline::workspace::root(&settings,&project)?;
            let(runner,_logs)=self.create_runner(clock,cancel)?;
            let lease_runner=runner.clone();
            let lease=deadlock_pipeline::workspace::WorkspaceLease::acquire_for_runner(&root,&lease_runner)?;
            let(compiled,skeleton,model_text)=self.preparation_original(&settings,&project,clock,cancel,&runner)?;
            cancel.check()?;
            let reference=self.weapon_reference(&settings,&project,&skeleton,&model_text,clock,cancel);
            let service=self.preparation_service(&settings,&project,clock,runner)?.with_cancel(cancel.clone()).with_weapon_reference(reference);
            let mut analysis=service.inspect(&project,&source,rotation,&compiled,skeleton).map_err(|e|failure(e,cancel))?;
            let mut complete=PreparationOptions::default();
            complete.values.insert("complete_mapping".into(),true.into());
            analysis.suggestions=self.preparation_suggest(&analysis,&complete)?;
            cancel.check()?;
            // Earlier unused inspections are removed once this one is complete.
            drop(lease);
            self.prune_preparation(&settings,&[&project],Some(Path::new(&analysis.root)));
            self.append_log("info","Source imported into a separate preparation workspace. Review target/source roles and mappings before preparing.")?;
            Ok(analysis)
        }).map(|(value,_)|value)
    }
    pub fn preparation_weapon_preview(
        &self,
        project: &Project,
        analysis: &Analysis,
        options: &PreparationOptions,
    ) -> Result<deadlock_preparation::weapon::Preview> {
        service::weapon_preview(project, analysis, options)
            .map_err(|e| AppError::validation("The gun fit could not be computed", e))
    }
    pub fn preparation_review(
        &self,
        project: &Project,
        analysis: &Analysis,
        options: &PreparationOptions,
    ) -> Result<Vec<ReviewIssue>> {
        service::review_issues(project, analysis, options)
            .map_err(|e| AppError::validation("Preparation review failed", e))
    }
    pub fn preparation_convert(
        &self,
        mut project: Project,
        analysis: Analysis,
        options: PreparationOptions,
        correction: bool,
        correction_blend: Option<String>,
    ) -> Result<PreparationResult> {
        self.operation(if correction{"Re-export corrected character"}else{"Prepare character model"},|clock,cancel|{
            if let Some(path)=correction_blend.filter(|p|!p.is_empty()){if !correction{return Err(AppError::validation("Choose correction mode","A correction Blender file is only accepted when re-exporting a corrected scene."));}project.preparation_blend=path;}
            let settings=self.settings()?;
            deadlock_preparation::blender::check_executable(Path::new(&settings.blender)).map_err(|e|failure(e,cancel))?;
            let root=deadlock_pipeline::workspace::root(&settings,&project)?;
            let(runner,_logs)=self.create_runner(clock,cancel)?;
            let lease_runner=runner.clone();
            let lease=deadlock_pipeline::workspace::WorkspaceLease::acquire_for_runner(&root,&lease_runner)?;
            let(compiled,skeleton,_)=self.preparation_original(&settings,&project,clock,cancel,&runner)?;
            self.step(clock,"Verify reviewed target against current game",||{cancel.check()?;if skeleton!=analysis.target_skeleton{return Err(AppError::validation("Target skeleton changed","Import and analyze the original replacement again before preparing."));}Ok(())})?;
            let service=self.preparation_service(&settings,&project,clock,runner)?.with_cancel(cancel.clone());
            let result=service.prepare(&project,&analysis,options,correction,&compiled).map_err(|e|failure(e,cancel))?;
            cancel.check()?;
            // Older unaccepted results of this run are replaced by this one.
            drop(lease);
            self.prune_preparation(&settings,&[&project],Some(Path::new(&analysis.root)));
            self.append_log("info","Prepared DMX passed its native bind and stream checks. Review it before accepting; compiler and in-game validation are still required.")?;
            Ok(result)
        }).map(|(value,_)|value)
    }
    pub fn preparation_accept(
        &self,
        project: Project,
        result: PreparationResult,
    ) -> Result<Project> {
        self.operation("Accept prepared character",|clock,cancel|{
            let settings=self.settings()?;
            let root=deadlock_pipeline::workspace::root(&settings,&project)?;
            let _lease=deadlock_pipeline::workspace::WorkspaceLease::acquire(&root)?;
            let allowed=asset_paths::resolve(&root.join("humanoid_preparation")).map_err(|e|AppError::validation("Preparation workspace could not be resolved",e))?;
            for path in [&result.dmx,&result.blend,&result.report]{let path=asset_paths::resolve(Path::new(path)).map_err(|e|AppError::validation("Preparation result path could not be resolved",e))?;if !path.starts_with(&allowed)||!path.is_file(){return Err(AppError::validation("Prepared result is outside this project workspace",text(&path)?));}}
            let accepted=self.step(clock,"Verify and accept prepared model",||{cancel.check()?;let p=service::handoff(&project,&result).map_err(|e|failure(e,cancel))?;service::check_handoff(&p).map_err(|e|failure(e,cancel))?;cancel.check()?;Ok(p)})?;
            drop(_lease);
            let run=Path::new(&accepted.preparation_report).parent().and_then(Path::parent).map(Path::to_path_buf);
            self.prune_preparation(&settings,&[&accepted],run.as_deref());
            self.append_log("info","Prepared source, material choices and skeleton reference accepted. Run Analyze to review build mappings.")?;
            Ok(accepted)
        }).map(|(value,_)|value)
    }
    pub fn preparation_resume(&self, project: Project) -> Result<ResumedPreparation> {
        self.operation("Resume preparation review", |clock, cancel| {
            self.step(clock, "Resolve saved preparation and review", || {
                cancel.check()?;
                let (analysis, result, report) =
                    service::resume(&project).map_err(|e| failure(e, cancel))?;
                cancel.check()?;
                Ok(ResumedPreparation {
                    analysis,
                    result,
                    report: PreparationReview::from(&report),
                })
            })
        })
        .map(|(value, _)| value)
    }
    pub fn preparation_open_blend(&self, project: Project) -> Result<()> {
        self.operation("Open prepared character in Blender",|clock,cancel|{
            let settings=self.settings()?;
            let root=deadlock_pipeline::workspace::root(&settings,&project)?;
            self.step(clock,"Open workspace correction scene",||{cancel.check()?;let spec=deadlock_preparation::blender::correction_command(Path::new(&settings.blender),Path::new(&project.preparation_blend),&root.join("humanoid_preparation")).map_err(|e|failure(e,cancel))?;let mut child=Command::new(&spec.program).args(&spec.args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().map_err(|e|AppError::io("Blender could not open the correction scene",e))?;std::thread::spawn(move||{let _=child.wait();});Ok(())})?;
            self.append_log("info","Prepared scene opened in Blender with automatic script execution disabled. Save corrections, then use Re-export corrected character.")
        }).map(|(value,_)|value)
    }
    pub fn outfit_save(&self, project: Project, path: &Path, name: &str) -> Result<()> {
        self.operation("Save character outfit", |clock, cancel| {
            self.step(clock, "Validate and save outfit choices", || {
                cancel.check()?;
                OutfitPreset::save(&project, path, name)
                    .map_err(|e| AppError::validation("Outfit could not be saved", e))?;
                cancel.check()?;
                Ok(())
            })
        })
        .map(|(value, _)| value)
    }
    pub fn outfit_load(&self, project: Project, path: &Path) -> Result<Project> {
        self.operation("Load character outfit", |clock, cancel| {
            let preset = self.step(clock, "Read and resolve outfit sources", || {
                cancel.check()?;
                OutfitPreset::load(path)
                    .map_err(|e| AppError::validation("Outfit could not be read", e))
            })?;
            self.step(clock, "Apply hero-bound outfit choices", || {
                cancel.check()?;
                preset
                    .apply(&project)
                    .map_err(|e| AppError::validation("Outfit could not be applied", e))
            })
        })
        .map(|(value, _)| value)
    }
}

/// Registration paths are `crate::preparation::commands::<name>`.
#[cfg(feature = "desktop")]
pub mod commands {
    use super::*;
    use tauri::State;
    type AppState<'a> = State<'a, Arc<Application>>;
    async fn background<T: Send + 'static>(
        operation: impl FnOnce() -> Result<T> + Send + 'static,
    ) -> Result<T> {
        tauri::async_runtime::spawn_blocking(operation)
            .await
            .map_err(|error| {
                AppError::new(
                    "internal",
                    "The preparation operation stopped unexpectedly",
                    error,
                )
            })?
    }
    #[tauri::command]
    pub async fn preparation_suggest(
        state: AppState<'_>,
        analysis: Analysis,
        options: PreparationOptions,
    ) -> Result<algorithms::Suggestions> {
        let app = state.inner().clone();
        background(move || app.preparation_suggest(&analysis, &options)).await
    }
    #[tauri::command]
    pub async fn preparation_save_mapping_preset(
        state: AppState<'_>,
        analysis: Analysis,
        options: PreparationOptions,
    ) -> Result<()> {
        let app = state.inner().clone();
        background(move || app.preparation_save_mapping_preset(&analysis, &options)).await
    }
    #[tauri::command]
    pub async fn preparation_inspect(
        state: AppState<'_>,
        project: Project,
        source: String,
        rotation: [f64; 3],
    ) -> Result<Analysis> {
        let app = state.inner().clone();
        background(move || app.preparation_inspect(project, source, rotation)).await
    }
    #[tauri::command]
    pub async fn preparation_weapon_preview(
        state: AppState<'_>,
        project: Project,
        analysis: Analysis,
        options: PreparationOptions,
    ) -> Result<deadlock_preparation::weapon::Preview> {
        let app = state.inner().clone();
        background(move || app.preparation_weapon_preview(&project, &analysis, &options)).await
    }
    #[tauri::command]
    pub async fn preparation_review(
        state: AppState<'_>,
        project: Project,
        analysis: Analysis,
        options: PreparationOptions,
    ) -> Result<Vec<ReviewIssue>> {
        let app = state.inner().clone();
        background(move || app.preparation_review(&project, &analysis, &options)).await
    }
    #[tauri::command]
    pub async fn preparation_convert(
        state: AppState<'_>,
        project: Project,
        analysis: Analysis,
        options: PreparationOptions,
        correction: bool,
        correction_blend: Option<String>,
    ) -> Result<PreparationResult> {
        let app = state.inner().clone();
        background(move || {
            app.preparation_convert(project, analysis, options, correction, correction_blend)
        })
        .await
    }
    #[tauri::command]
    pub async fn preparation_accept(
        state: AppState<'_>,
        project: Project,
        result: PreparationResult,
    ) -> Result<Project> {
        let app = state.inner().clone();
        background(move || app.preparation_accept(project, result)).await
    }
    #[tauri::command]
    pub async fn preparation_resume(
        state: AppState<'_>,
        project: Project,
    ) -> Result<ResumedPreparation> {
        let app = state.inner().clone();
        background(move || app.preparation_resume(project)).await
    }
    #[tauri::command]
    pub async fn preparation_open_blend(state: AppState<'_>, project: Project) -> Result<()> {
        let app = state.inner().clone();
        background(move || app.preparation_open_blend(project)).await
    }
    #[tauri::command]
    pub async fn outfit_save(
        state: AppState<'_>,
        project: Project,
        path: PathBuf,
        name: String,
    ) -> Result<()> {
        let app = state.inner().clone();
        background(move || app.outfit_save(project, &path, &name)).await
    }
    #[tauri::command]
    pub async fn outfit_load(
        state: AppState<'_>,
        project: Project,
        path: PathBuf,
    ) -> Result<Project> {
        let app = state.inner().clone();
        background(move || app.outfit_load(project, &path)).await
    }
}
