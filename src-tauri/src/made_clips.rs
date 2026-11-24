//! Motion → Make it from my animation: the commands a page uses.
//!
//! * `made_clip_check`: whether the hero's clip can be made from an
//!   animation (not additive, no model-space bones) and whether the prepared
//!   model has the saved fit. Read-only and lease-free.
//! * `made_clip_actions`: the actions of the user's animation file (Blender,
//!   FBX or glTF) and whether
//!   its skeleton is the prepared one (Blender, under the operation lease).
//! * `make_clip`: samples the row's action into its packet (Blender, under
//!   the operation lease; a current packet is reused without Blender), then
//!   makes the clip once as the build does and answers its report.
//! * `made_clip_report` / `made_clip_preview`: the clip the build would write
//!   for a row (its report, or the Motion page player's packet), with no
//!   Blender and no lease; the newest request wins (an older one answers
//!   `superseded`). Changing the timing, loop blend, in place or bones needs
//!   no Blender: only the action, the mode or the file do.
use crate::app::{AppError, Application, Result};
use deadlock_components::made_clips::{self, MadeInputs};
use deadlock_core::CancellationToken;
use deadlock_preparation::clip_making::{self as making, ActionList, Made, MadeReport};
use deadlock_project::{customization::ResourceReplacement, Project};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Messages of these commands (the page and troubleshooting compare them).
pub const MADE_NO_BLENDER: &str = "Clips are made from an animation with Blender. Choose blender.exe in Settings";
pub const MADE_SUPERSEDED: &str = "superseded";
pub const MADE_NOT_A_ROW: &str = "Choose the animation file and the animation the clip is made from";

/// What `made_clip_check` answers.
#[derive(Debug, Clone, Serialize)]
pub struct MadeClipCheck {
    pub target: String,
    /// Why this clip cannot be made from an animation (`clip_making::ADDITIVE`
    /// or `MODEL_SPACE`), or none.
    pub refusal: Option<String>,
    /// Why the prepared model cannot be used yet (`NEEDS_PREPARED`, `NO_FIT`), or none.
    pub fit_error: Option<String>,
}
/// What `make_clip` answers.
#[derive(Debug, Clone, Serialize)]
pub struct MadeClipAnswer {
    pub target: String,
    /// The packet's stamp (file, action, mode, samples, length, frames per second).
    pub made: Made,
    pub report: MadeReport,
}

type Key = (String, String, String);
/// The last made clip (target, row, packet digest) → bytes and report.
static LAST: Mutex<Option<(Key, Arc<(Vec<u8>, MadeReport)>)>> = Mutex::new(None);
static PREVIEW: Mutex<Option<CancellationToken>> = Mutex::new(None);
static REPORT: Mutex<Option<CancellationToken>> = Mutex::new(None);

fn newest(slot: &Mutex<Option<CancellationToken>>) -> CancellationToken {
    let cancel = CancellationToken::new();
    if let Some(older) = slot.lock().unwrap_or_else(|e| e.into_inner()).replace(cancel.clone()) {
        older.cancel();
    }
    cancel
}
fn superseded<T>(result: Result<T>, cancel: &CancellationToken) -> Result<T> {
    match result {
        Err(error) if error.kind == "cancelled" || cancel.is_cancelled() => Err(AppError::new("cancelled", MADE_SUPERSEDED, "A newer request replaced this one.")),
        other => other,
    }
}
fn made_row(row: &ResourceReplacement) -> Result<()> {
    if row.made.is_none() || row.source.as_deref().unwrap_or_default().trim().is_empty() {
        return Err(AppError::validation(MADE_NOT_A_ROW, "The row has no animation file or no animation."));
    }
    row.check_made("animations").map_err(|e| AppError::validation("This clip cannot be made", e))
}

impl Application {
    fn made_inputs(&self, project: &Project) -> Result<MadeInputs> {
        let settings = self.settings()?;
        let root = deadlock_pipeline::workspace::root(&settings, project)?;
        Ok(MadeInputs::for_project(root, project))
    }

    /// Whether `target` can be made from an animation for this project.
    pub fn made_clip_check(&self, project: Project, target: String) -> Result<MadeClipCheck> {
        let archive = self.clip_archive()?;
        let refusal = made_clips::refusal(&archive, &target)?.map(str::to_string);
        let fit_error = making::fit_path(&project).err().map(|e| e.to_string());
        Ok(MadeClipCheck { target, refusal, fit_error })
    }

    /// The actions of an animation file (Blender, under the operation lease).
    pub fn made_clip_actions(&self, project: Project, source: String) -> Result<ActionList> {
        project.validate_action().map_err(|e| AppError::validation("Select a valid character project", e))?;
        let fit = making::fit_path(&project).map_err(|e| AppError::validation(&e.to_string(), "The prepared model has no fit for making clips."))?;
        let (list, _) = self.operation("List the animations of a file", |clock, cancel| {
            let settings = self.settings()?;
            if settings.blender.is_empty() || !Path::new(&settings.blender).is_file() {
                return Err(AppError::validation(MADE_NO_BLENDER, "No Blender is configured."));
            }
            let path = PathBuf::from(&source);
            deadlock_vpk::check_unlinked_path(&path, false).map_err(|e| AppError::validation("Unsupported animation file", e))?;
            making::check_source(&path).map_err(|e| AppError::validation(making::ONLY_ANIMATION_FILES, e))?;
            let (runner, _observer) = self.create_runner(clock, cancel)?;
            let bridge = deadlock_preparation::blender::BlenderBridge { executable: PathBuf::from(&settings.blender), executor: Arc::new(runner) };
            let root = deadlock_pipeline::workspace::root(&settings, &project)?;
            self.step(clock, "Read the animations of the file", || {
                making::list_actions(&bridge, &root, &path, &fit).map_err(|e| AppError::validation("The file's animations could not be read", e))
            })
        })?;
        Ok(list)
    }

    /// Samples the row's action into its packet (Blender, under the operation
    /// lease) and makes the clip once as the build does.
    pub fn make_clip(&self, project: Project, target: String, row: ResourceReplacement) -> Result<MadeClipAnswer> {
        project.validate_action().map_err(|e| AppError::validation("Select a valid character project", e))?;
        made_row(&row)?;
        let made = row.made.clone().unwrap_or_else(|| deadlock_project::made_clip::MadeClip::new(""));
        let fit = making::fit_path(&project).map_err(|e| AppError::validation(&e.to_string(), "The prepared model has no fit for making clips."))?;
        let archive = self.clip_archive()?;
        if let Some(reason) = made_clips::refusal(&archive, &target)? {
            return Err(AppError::validation(reason, &target));
        }
        let (stamp, _) = self.operation("Make a clip from an animation", |clock, cancel| {
            let settings = self.settings()?;
            if settings.blender.is_empty() || !Path::new(&settings.blender).is_file() {
                return Err(AppError::validation(MADE_NO_BLENDER, "No Blender is configured."));
            }
            let path = PathBuf::from(row.source.as_deref().unwrap_or_default());
            deadlock_vpk::check_unlinked_path(&path, false).map_err(|e| AppError::validation("Unsupported animation file", e))?;
            let (runner, _observer) = self.create_runner(clock, cancel)?;
            let bridge = deadlock_preparation::blender::BlenderBridge { executable: PathBuf::from(&settings.blender), executor: Arc::new(runner) };
            let root = deadlock_pipeline::workspace::root(&settings, &project)?;
            self.step(clock, "Sample the animation in Blender", || {
                making::make_packet(&bridge, &root, &path, &fit, &made, cancel).map_err(|e| AppError::validation("The clip could not be made from the animation", e))
            })
        })?;
        let (_, report) = self.made_clip(&project, &target, &row, &CancellationToken::new())?;
        Ok(MadeClipAnswer { target, made: stamp, report })
    }

    /// The clip the build would write for `row` (bytes and report), kept for
    /// the last row so the report and the preview share one making.
    fn made_clip(&self, project: &Project, target: &str, row: &ResourceReplacement, cancel: &CancellationToken) -> Result<(Vec<u8>, MadeReport)> {
        made_row(row)?;
        let archive = self.clip_archive()?;
        let inputs = self.made_inputs(project)?;
        let fit = inputs.fit.clone().map_err(|e| AppError::validation(&e, "The prepared model has no fit for making clips."))?;
        let made = row.made.as_ref().expect("checked");
        let (stamp, _) = making::load(&inputs.project_root, Path::new(row.source.as_deref().unwrap_or_default()), &fit, made, cancel)
            .map_err(|e| AppError::validation(making::NOT_MADE, e))?;
        let key = (deadlock_core::text::casefold(target), serde_json::to_string(row).unwrap_or_default(), stamp.packet_sha256.clone());
        if let Some((held, value)) = LAST.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            if *held == key {
                return Ok((value.0.clone(), value.1.clone()));
            }
        }
        cancel.check()?;
        let authored = made_clips::author_row(&archive, target, row, &inputs, cancel)?;
        let value = Arc::new((authored.bytes, authored.report));
        *LAST.lock().unwrap_or_else(|e| e.into_inner()) = Some((key, value.clone()));
        Ok((value.0.clone(), value.1.clone()))
    }

    /// The report of the clip `row` makes (no Blender, no lease, newest wins).
    pub fn made_clip_report(&self, project: Project, target: String, row: ResourceReplacement) -> Result<MadeReport> {
        let cancel = newest(&REPORT);
        superseded(self.made_clip(&project, &target, &row, &cancel).map(|(_, report)| report), &cancel)
    }

    /// The clip `row` makes, played on the prepared character (the Motion
    /// page player's packet; no Blender, no lease, newest wins).
    pub fn made_clip_preview(&self, project: Project, target: String, row: ResourceReplacement) -> Result<Vec<u8>> {
        let cancel = newest(&PREVIEW);
        let result = (|| {
            let (bytes, _) = self.made_clip(&project, &target, &row, &cancel)?;
            let archive = self.clip_archive()?;
            let character = self.clip_character(&project, &cancel)?;
            Ok(deadlock_pipeline::clip_preview::packet(&archive, &character, &target, deadlock_pipeline::clip_preview::Source::Made(&bytes), &cancel)?)
        })();
        superseded(result, &cancel)
    }
}

#[cfg(feature = "desktop")]
pub mod commands {
    use super::*;
    type State<'a> = tauri::State<'a, Arc<Application>>;
    async fn background<T: Send + 'static>(work: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
        tauri::async_runtime::spawn_blocking(work).await.map_err(|e| AppError::new("internal", "The clip-making worker stopped", e))?
    }
    #[tauri::command]
    pub async fn made_clip_check(state: State<'_>, project: Project, target: String) -> Result<MadeClipCheck> {
        let app = state.inner().clone();
        background(move || app.made_clip_check(project, target)).await
    }
    #[tauri::command]
    pub async fn made_clip_actions(state: State<'_>, project: Project, source: String) -> Result<ActionList> {
        let app = state.inner().clone();
        background(move || app.made_clip_actions(project, source)).await
    }
    #[tauri::command]
    pub async fn make_clip(state: State<'_>, project: Project, target: String, row: ResourceReplacement) -> Result<MadeClipAnswer> {
        let app = state.inner().clone();
        background(move || app.make_clip(project, target, row)).await
    }
    #[tauri::command]
    pub async fn made_clip_report(state: State<'_>, project: Project, target: String, row: ResourceReplacement) -> Result<MadeReport> {
        let app = state.inner().clone();
        background(move || app.made_clip_report(project, target, row)).await
    }
    #[tauri::command]
    pub async fn made_clip_preview(state: State<'_>, project: Project, target: String, row: ResourceReplacement) -> Result<tauri::ipc::Response> {
        let app = state.inner().clone();
        background(move || app.made_clip_preview(project, target, row)).await.map(tauri::ipc::Response::new)
    }
}
