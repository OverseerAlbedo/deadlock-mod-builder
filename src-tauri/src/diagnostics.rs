//! Tool diagnostics, validated Blender configuration and explicit folder opening.
use crate::app::{AppError, Application, Result};
use deadlock_core::{safety, CoreError};
use deadlock_project::{Project, Settings};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticReport {
    pub path: PathBuf,
    pub report: serde_json::Value,
}
impl Application {
    pub fn tool_candidates(
        &self,
        settings: Settings,
    ) -> Result<Vec<deadlock_tools::toolchain::Candidate>> {
        self.operation("Find model compiler candidates", |clock, cancel| {
            let detected = self.step(clock, "Detect local compiler installations", || {
                deadlock_tools::detection::detect(&settings, &[], cancel).map_err(AppError::from)
            })?;
            self.step(clock, "Inspect candidate paths", || {
                deadlock_tools::toolchain::candidates(&detected, &[]).map_err(AppError::from)
            })
        })
        .map(|(v, _)| v)
    }
    pub fn tool_diagnostics(&self, settings: Settings, classes: bool) -> Result<DiagnosticReport> {
        self.operation(
            if classes {
                "Scan compiler class evidence"
            } else {
                "Audit tool configuration"
            },
            |clock, cancel| {
                let report = if classes {
                    let toolset = deadlock_tools::toolchain::select(&settings, false)?;
                    self.step(clock, "Read compiler binary class evidence", || {
                        deadlock_tools::binary_audit::scan_tree(&toolset.root, cancel, |line| {
                            let _ = self.append_log("info", line);
                        })
                        .map_err(AppError::from)
                    })?
                } else {
                    self.step(clock, "Read compiler dependencies and mounts", || {
                        deadlock_tools::toolchain::audit_with_cancel(&settings, cancel)
                            .map_err(AppError::from)
                    })?
                };
                let path = self
                    .state_directory
                    .join("operations")
                    .join(clock.report().run_id)
                    .join("tool-diagnostics.json");
                self.step(clock, "Save tool diagnostic report", || {
                    safety::write_json(&path, &report).map_err(AppError::from)
                })?;
                Ok(DiagnosticReport { path, report })
            },
        )
        .map(|(v, _)| v)
    }
    pub fn validate_blender(
        &self,
        executable: Option<PathBuf>,
    ) -> Result<deadlock_preparation::blender::BlenderValidation> {
        self.operation("Validate Blender", |clock, cancel| {
            let mut settings = self.settings()?;
            let (runner, _logs) = self.create_runner(clock, cancel)?;
            let path = executable
                .filter(|p| !p.as_os_str().is_empty())
                .or_else(|| (!settings.blender.trim().is_empty()).then(|| PathBuf::from(&settings.blender)))
                .or_else(deadlock_preparation::blender::detect)
                .ok_or_else(|| {
                    AppError::validation(
                        "Blender is not configured",
                        "No Blender path is selected and no local installation was found automatically. Open Settings → Advanced configuration → Blender executable, select blender.exe, then click Save settings.",
                    )
                })?;
            let validated = self.step(clock, "Launch Blender and inspect version", || {
                deadlock_preparation::blender::validate(&path, &runner).map_err(|e| {
                    match e {
                        deadlock_preparation::Error::BlenderNotConfigured
                        | deadlock_preparation::Error::BlenderNotFound(_)
                        | deadlock_preparation::Error::Core(_) => crate::preparation::failure(e, cancel),
                        e => AppError::validation("Blender validation failed", e),
                    }
                })
            })?;
            self.step(clock, "Save validated Blender path", || {
                cancel.check()?;
                // Keep unrelated persisted settings. The UI may have other
                // unsaved edits, which this Blender-only action must not commit.
                settings.blender = validated.path.clone();
                self.persist_settings(&settings)
            })?;
            Ok(validated)
        })
        .map(|(v, _)| v)
    }
    pub fn open_output_folder(&self, project: Project) -> Result<PathBuf> {
        self.operation("Open output folder", |clock, cancel| {
            let settings = self.settings()?;
            let path = deadlock_pipeline::workspace::root(&settings, &project)?.join("output");
            deadlock_vpk::check_unlinked_path(&path, false)
                .map_err(|e| AppError::validation("Output folder is unavailable", e))?;
            if !path.is_dir() {
                return Err(AppError::validation(
                    "Build a package first",
                    "This project has no output folder yet.",
                ));
            }
            cancel.check()?;
            self.step(clock, "Open verified project output directory", || {
                #[cfg(windows)]
                let program = "explorer.exe";
                #[cfg(target_os = "macos")]
                let program = "open";
                #[cfg(all(not(windows), not(target_os = "macos")))]
                let program = "xdg-open";
                let mut command = std::process::Command::new(program);
                command
                    .arg(&path)
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null());
                let mut child = command
                    .spawn()
                    .map_err(|e| AppError::from(CoreError::io("Open system file browser", e)))?;
                std::thread::spawn(move || {
                    let _ = child.wait();
                });
                Ok(())
            })?;
            Ok(path)
        })
        .map(|(v, _)| v)
    }
}
#[cfg(feature = "desktop")]
pub mod commands {
    use super::*;
    use std::sync::Arc;
    use tauri::State;
    type AppState<'a> = State<'a, Arc<Application>>;
    async fn run<T: Send + 'static>(f: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
        tauri::async_runtime::spawn_blocking(f)
            .await
            .map_err(|e| AppError::new("internal", "Tool diagnostic worker stopped", e))?
    }
    #[tauri::command]
    pub async fn tool_candidates(
        state: AppState<'_>,
        settings: Settings,
    ) -> Result<Vec<deadlock_tools::toolchain::Candidate>> {
        let app = state.inner().clone();
        run(move || app.tool_candidates(settings)).await
    }
    #[tauri::command]
    pub async fn tool_diagnostics(
        state: AppState<'_>,
        settings: Settings,
        classes: bool,
    ) -> Result<DiagnosticReport> {
        let app = state.inner().clone();
        run(move || app.tool_diagnostics(settings, classes)).await
    }
    #[tauri::command]
    pub async fn validate_blender(
        state: AppState<'_>,
        executable: Option<PathBuf>,
    ) -> Result<deadlock_preparation::blender::BlenderValidation> {
        let app = state.inner().clone();
        run(move || app.validate_blender(executable)).await
    }
    #[tauri::command]
    pub async fn open_output_folder(state: AppState<'_>, project: Project) -> Result<PathBuf> {
        let app = state.inner().clone();
        run(move || app.open_output_folder(project)).await
    }
}
