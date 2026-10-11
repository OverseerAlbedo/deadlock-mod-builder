//! Settings → Storage: disk use per project, Clear actions and retention. The
//! app remembers which saved project files use which workspace so that data a
//! saved project still references is never removed.
use crate::app::{AppError, Application, Result};
use deadlock_pipeline::storage::{self, ClearRequest, ClearResult, KnownProject, StorageReport};
use deadlock_project::{Project, Settings};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageClear {
    pub result: ClearResult,
    pub report: StorageReport,
}

/// Saved project files by project ID.
type Registry = BTreeMap<String, BTreeSet<String>>;

const MAX_PROJECT_BYTES: u64 = 16 * 1024 * 1024;

fn project_id(path: &Path) -> Option<String> {
    let meta = fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > MAX_PROJECT_BYTES {
        return None;
    }
    let value: serde_json::Value = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
    let id = value.get("id")?.as_str()?;
    (storage::is_project_id(id) && value.get("schema").is_some()).then(|| id.to_owned())
}
fn inside(path: &Path, folder: &Path) -> bool {
    let normalize = |p: &Path| {
        let text = p.to_string_lossy().replace('\\', "/");
        if cfg!(windows) {
            text.to_lowercase()
        } else {
            text
        }
    };
    let (path, folder) = (normalize(path), normalize(folder));
    path.starts_with(&format!("{}/", folder.trim_end_matches('/')))
}

impl Application {
    fn registry_path(&self) -> PathBuf {
        self.state_directory.join("known-projects.json")
    }
    fn registry(&self) -> Registry {
        fs::read(self.registry_path())
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }
    /// Record that `path` is a saved file of `project`. A failure here only
    /// costs the Storage page some knowledge, so it is logged, not raised.
    pub(crate) fn remember_project(&self, path: &Path, project: &Project) {
        let Ok(path) = deadlock_project::asset_paths::resolve(path) else {
            return;
        };
        let mut registry = self.registry();
        let entry = registry.entry(project.id.clone()).or_default();
        if !entry.insert(path.to_string_lossy().into_owned()) {
            return;
        }
        if let Err(error) = deadlock_core::safety::write_json(&self.registry_path(), &registry) {
            let _ = self.append_log(
                "warning",
                &format!("The list of saved projects could not be updated: {error}"),
            );
        }
    }
    /// Saved project files outside the workspace that still carry their ID.
    fn known_projects(&self, settings: &Settings, open: Option<&Project>) -> BTreeMap<String, KnownProject> {
        let workspace = PathBuf::from(&settings.workspace);
        let mut candidates = self.registry();
        if !settings.last_project.is_empty() {
            if let Some(id) = project_id(Path::new(&settings.last_project)) {
                candidates
                    .entry(id)
                    .or_default()
                    .insert(settings.last_project.clone());
            }
        }
        let mut known: BTreeMap<String, KnownProject> = BTreeMap::new();
        for (id, files) in candidates {
            for file in files {
                let path = PathBuf::from(&file);
                if (settings.workspace.is_empty() || !inside(&path, &workspace))
                    && project_id(&path).as_deref() == Some(id.as_str())
                {
                    known.entry(id.clone()).or_default().files.push(path);
                }
            }
        }
        if let Some(project) = open {
            known.entry(project.id.clone()).or_default().open = Some(project.clone());
        }
        known
    }
    pub fn storage_report(&self, open: Option<Project>) -> Result<StorageReport> {
        self.operation("Measure stored data", |clock, cancel| {
            let settings = self.settings()?;
            self.step(clock, "Measure workspace and compiler folders", || {
                let known = self.known_projects(&settings, open.as_ref());
                storage::report(&settings, &known, cancel).map_err(AppError::from)
            })
        })
        .map(|(report, _)| report)
    }
    pub fn storage_clear(&self, request: ClearRequest, open: Option<Project>) -> Result<StorageClear> {
        self.operation("Clear stored data", |clock, cancel| {
            let settings = self.settings()?;
            let known = self.known_projects(&settings, open.as_ref());
            let result = self.step(clock, "Remove selected data", || {
                storage::clear(&settings, &known, &request, cancel).map_err(AppError::from)
            })?;
            self.append_log(
                "info",
                &format!(
                    "Removed {} item(s), {:.1} MB",
                    result.removed.len(),
                    result.removed_bytes as f64 / 1_048_576.0
                ),
            )?;
            for skipped in &result.skipped {
                self.append_log("warning", &format!("Not removed: {skipped}"))?;
            }
            let report = self.step(clock, "Measure workspace and compiler folders", || {
                storage::report(&settings, &known, cancel).map_err(AppError::from)
            })?;
            Ok(StorageClear { result, report })
        })
        .map(|(value, _)| value)
    }
    /// Look for saved project files in a folder the user chooses, so projects
    /// saved before this list existed are recognised.
    pub fn storage_find_projects(&self, folder: &Path) -> Result<usize> {
        self.operation("Find saved projects", |clock, cancel| {
            self.step(clock, "Search the folder for project files", || {
                let mut found = 0;
                let mut pending = vec![(folder.to_path_buf(), 0usize)];
                let mut visited = 0usize;
                while let Some((current, depth)) = pending.pop() {
                    cancel.check()?;
                    let Ok(entries) = fs::read_dir(&current) else { continue };
                    for entry in entries.flatten() {
                        visited += 1;
                        if visited > 200_000 {
                            return Ok(found);
                        }
                        let Ok(kind) = entry.file_type() else { continue };
                        let path = entry.path();
                        if kind.is_dir() && depth < 6 {
                            pending.push((path, depth + 1));
                        } else if kind.is_file()
                            && path
                                .file_name()
                                .and_then(|n| n.to_str())
                                .is_some_and(|n| n.to_ascii_lowercase().ends_with(".json"))
                        {
                            if let Some(id) = project_id(&path) {
                                let mut project = Project::default();
                                project.id = id;
                                self.remember_project(&path, &project);
                                found += 1;
                            }
                        }
                    }
                }
                Ok(found)
            })
        })
        .map(|(count, _)| count)
    }
    /// Retention after Inspect, Convert or Accept. Call after releasing the
    /// workspace lease; a held lock leaves everything in place.
    pub(crate) fn prune_preparation(&self, settings: &Settings, current: &[&Project], keep: Option<&Path>) {
        let Some(first) = current.first() else { return };
        let files = self
            .known_projects(settings, None)
            .remove(&first.id)
            .map(|k| k.files)
            .unwrap_or_default();
        let log = |message: &str| {
            let _ = self.append_log("info", message);
        };
        storage::prune_preparation(settings, current, &files, keep, &log);
    }
}

#[cfg(feature = "desktop")]
pub mod commands {
    use super::*;
    use std::sync::Arc;
    type State<'a> = tauri::State<'a, Arc<Application>>;
    async fn background<T: Send + 'static>(work: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
        tauri::async_runtime::spawn_blocking(work)
            .await
            .map_err(|e| AppError::new("internal", "The storage worker stopped", e))?
    }
    #[tauri::command]
    pub async fn storage_report(state: State<'_>, project: Option<Project>) -> Result<StorageReport> {
        let app = state.inner().clone();
        background(move || app.storage_report(project)).await
    }
    #[tauri::command]
    pub async fn storage_clear(
        state: State<'_>,
        request: ClearRequest,
        project: Option<Project>,
    ) -> Result<StorageClear> {
        let app = state.inner().clone();
        background(move || app.storage_clear(request, project)).await
    }
    #[tauri::command]
    pub async fn storage_find_projects(state: State<'_>, folder: PathBuf) -> Result<usize> {
        let app = state.inner().clone();
        background(move || app.storage_find_projects(&folder)).await
    }
}
