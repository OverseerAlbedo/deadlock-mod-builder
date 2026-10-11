//! Blocking application workflows. Tauri commands execute these on worker threads.
use crate::app::{AppError, Application, Result};
use deadlock_pipeline::{
    build::{BuildResult, BuildService},
    inspect::ModelPipeline,
    model_compile::OriginalValidation,
    prepare::PreparedModel,
};
use deadlock_project::Project;
use serde::Serialize;
use std::path::Path;
#[derive(Debug, Serialize)]
pub struct ProjectInspection {
    pub project: Project,
    pub report: InspectionDetails,
}
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum InspectionDetails {
    Model(deadlock_pipeline::inspect::InspectionReport),
    Components(deadlock_components::service::ComponentInspection),
}
#[derive(Debug, Serialize)]
pub struct ProjectPreparation {
    pub project: Project,
    pub prepared: PreparedModel,
}
#[derive(Debug, Serialize)]
pub struct ProjectBuild {
    pub project: Project,
    pub result: BuildResult,
    pub path: std::path::PathBuf,
}
impl Application {
    fn with_pipeline<T>(
        &self,
        name: &str,
        work: impl FnOnce(&BuildService<'_>) -> Result<T>,
    ) -> Result<T> {
        self.operation(name, |clock, token| {
            let settings = self.settings()?;
            let (runner, _logs) = self.create_runner(clock, token)?;
            let reader = self.native_reader_root();
            let log = |message: &str| {
                let _ = self.append_log("info", message);
            };
            let service = BuildService {
                model: ModelPipeline {
                    settings: &settings,
                    runner: &runner,
                    cancel: token,
                    clock,
                    log: &log,
                    reader_root: &reader,
                },
            };
            work(&service)
        })
        .map(|(value, _)| value)
    }
    pub fn inspect_project(&self, mut project: Project) -> Result<ProjectInspection> {
        self.with_pipeline("Analyze project", |service| {
            let root = service.model.root(&project)?;
            let _lease = deadlock_pipeline::workspace::WorkspaceLease::acquire_for_runner(
                &root,
                service.model.runner,
            )?;
            let report = if project.customization.model_enabled() {
                let report = service.model.inspect(&mut project)?;
                project.meshes = report.project_mappings.clone();
                project.materials = report.materials.clone();
                InspectionDetails::Model(report)
            } else {
                InspectionDetails::Components(
                    deadlock_components::service::ComponentService {
                        settings: service.model.settings,
                        runner: service.model.runner,
                        cancel: service.model.cancel,
                        clock: service.model.clock,
                        log: service.model.log,
                    }
                    .inspect(&project)?,
                )
            };
            Ok(ProjectInspection { project, report })
        })
    }
    pub fn prepare_project(
        &self,
        mut project: Project,
        offline: bool,
    ) -> Result<ProjectPreparation> {
        self.with_pipeline("Prepare project sources", |service| {
            let root = service.model.root(&project)?;
            let _lease = deadlock_pipeline::workspace::WorkspaceLease::acquire_for_runner(
                &root,
                service.model.runner,
            )?;
            let report = service.model.inspect(&mut project)?;
            let prepared = service.model.prepare(&mut project, &report, offline)?;
            Ok(ProjectPreparation { project, prepared })
        })
    }
    pub fn build_project(
        &self,
        mut project: Project,
        overwrite_output: bool,
        overwrite_install: bool,
        project_path: Option<std::path::PathBuf>,
    ) -> Result<ProjectBuild> {
        self.with_pipeline("Build project", |service| {
            let path = match project_path {
                Some(path) => path,
                None => service.model.root(&project)?.join("project.dmb.json"),
            };
            service
                .model
                .clock
                .measure("Save project before build", None, |_| {
                    project
                        .save(&path)
                        .map_err(|e| deadlock_core::CoreError::Integrity(e.to_string()))
                })?;
            let result = service.build(&mut project, overwrite_output, overwrite_install)?;
            service
                .model
                .clock
                .measure("Save final project state", None, |_| {
                    project
                        .save(&path)
                        .map_err(|e| deadlock_core::CoreError::Integrity(e.to_string()))
                })?;
            self.remember_project(&path, &project);
            if service.model.settings.clear_build_data_after_build {
                // Optional: caches and compiler folders the next build recreates.
                let cleared = service.model.clock.measure("Clear rebuildable build data", None, |_| {
                    deadlock_pipeline::storage::clear_after_build(service.model.settings, &project, service.model.cancel)
                })?;
                (service.model.log)(&format!(
                    "Cleared {:.1} MB of rebuildable build data",
                    cleared.removed_bytes as f64 / 1_048_576.0
                ));
            }
            Ok(ProjectBuild {
                project,
                result,
                path,
            })
        })
    }
    pub fn install_project(
        &self,
        mut project: Project,
        path: &Path,
        overwrite: bool,
    ) -> Result<std::path::PathBuf> {
        self.with_pipeline("Install verified package", |service| {
            service
                .install(&mut project, path, overwrite)
                .map_err(AppError::from)
        })
    }
    pub fn validate_original(&self, project: Project, compile: bool) -> Result<OriginalValidation> {
        self.with_pipeline("Validate original model toolchain", |service| {
            service
                .model
                .validate_original(&project, compile)
                .map_err(AppError::from)
        })
    }
}
