//! Build & install → Installed mods: the scan of the game's addons folder with
//! what this app knows about each file. Two small records live in the app's
//! state folder, never in the game folder or inside a VPK, both keyed by the
//! file's content identity (`vpk_contents::identity`), so they stay with the
//! file when a mod loader renames or renumbers it:
//! * `installed-vpks.json`: written after every install by this app (which
//!   project, which hero);
//! * `vpk-labels.json`: the labels the user typed.
//!
//! "Made here" is claimed only on an identity match: an install record, or a
//! VPK still in a workspace project's output folder.
use crate::app::{AppError, Application, Result};
use deadlock_pipeline::mod_update::{self, InstalledMod};
use deadlock_project::Project;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

/// The longest label kept, in characters.
pub const MAX_LABEL_CHARS: usize = 80;
/// At most this many labels and install records are kept (oldest dropped).
const MAX_ROWS: usize = 2000;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InstallRecord {
    pub project_id: String,
    pub project_name: String,
    pub hero: String,
    /// The installed file's name when it was installed.
    pub file: String,
    /// Milliseconds since 1970.
    pub recorded: u64,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Label {
    pub text: String,
    /// The file's name when the label was set (for the record only).
    pub file: String,
    pub updated: u64,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Records {
    schema: u32,
    records: BTreeMap<String, InstallRecord>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Labels {
    schema: u32,
    labels: BTreeMap<String, Label>,
}

/// Which project of this app built the file, when that is certain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MadeHere {
    pub project_id: String,
    /// Empty when no record of the project's name is left.
    pub project_name: String,
    pub hero: String,
    /// A saved project file of that project on this PC, when one exists.
    pub project_file: Option<String>,
    /// `install` (recorded when this app installed it) or `output` (the same
    /// contents as a VPK in the project's output folder).
    pub source: String,
    /// Outputs of several projects hold these same files (assets-only builds
    /// with the same extras): made here, but no one project is named.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub several: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledModView {
    #[serde(flatten)]
    pub installed: InstalledMod,
    pub label: Option<String>,
    pub made: Option<MadeHere>,
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}
fn read<T: for<'de> Deserialize<'de> + Default>(path: &Path) -> T {
    fs::read(path).ok().and_then(|raw| serde_json::from_slice(&raw).ok()).unwrap_or_default()
}
fn prune<T>(rows: &mut BTreeMap<String, T>, stamp: impl Fn(&T) -> u64) {
    while rows.len() > MAX_ROWS {
        let Some(oldest) = rows.iter().min_by_key(|(_, v)| stamp(v)).map(|(k, _)| k.clone()) else { break };
        rows.remove(&oldest);
    }
}
fn valid_identity(identity: &str) -> bool {
    identity
        .strip_prefix(deadlock_components::vpk_contents::IDENTITY_VERSION)
        .and_then(|rest| rest.strip_prefix(':'))
        .is_some_and(|hex| hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
}
/// A label as it is kept: trimmed, control characters removed, at most
/// `MAX_LABEL_CHARS` characters. Empty means "no label".
pub fn clean_label(text: &str) -> String {
    let text: String = text.chars().filter(|c| !c.is_control()).collect();
    text.trim().chars().take(MAX_LABEL_CHARS).collect::<String>().trim().to_owned()
}

/// Names and records joined onto a scan. Pure, for tests.
pub fn join(
    scan: Vec<InstalledMod>,
    records: &BTreeMap<String, InstallRecord>,
    labels: &BTreeMap<String, Label>,
    outputs: &[mod_update::OwnOutput],
    project: &dyn Fn(&str) -> (Option<(String, String)>, Option<String>),
) -> Vec<InstalledModView> {
    scan.into_iter()
        .map(|installed| {
            let identity = installed.identity.clone();
            let known = !identity.is_empty();
            let label = known.then(|| labels.get(&identity)).flatten().map(|l| l.text.clone()).filter(|t| !t.is_empty());
            let made = if !known {
                None
            } else if let Some(record) = records.get(&identity) {
                let (summary, file) = project(&record.project_id);
                let (name, hero) = summary.unwrap_or_default();
                Some(MadeHere {
                    project_id: record.project_id.clone(),
                    project_name: if record.project_name.is_empty() { name } else { record.project_name.clone() },
                    hero: if record.hero.is_empty() { hero } else { record.hero.clone() },
                    project_file: file,
                    source: "install".into(),
                    several: false,
                })
            } else {
                let projects: std::collections::BTreeSet<&str> = outputs.iter().filter(|o| o.identity == identity).map(|o| o.project_id.as_str()).collect();
                match projects.len() {
                    0 => None,
                    1 => {
                        let id = projects.into_iter().next().unwrap_or_default();
                        let (summary, file) = project(id);
                        let (name, hero) = summary.unwrap_or_default();
                        Some(MadeHere { project_id: id.to_owned(), project_name: name, hero, project_file: file, source: "output".into(), several: false })
                    }
                    _ => Some(MadeHere { project_id: String::new(), project_name: String::new(), hero: String::new(), project_file: None, source: "output".into(), several: true }),
                }
            };
            InstalledModView { installed, label, made }
        })
        .collect()
}

impl Application {
    fn records_path(&self) -> PathBuf {
        self.state_directory.join("installed-vpks.json")
    }
    fn labels_path(&self) -> PathBuf {
        self.state_directory.join("vpk-labels.json")
    }
    /// A saved project file of `id` on this PC: a remembered one that still
    /// carries the ID, else the workspace copy.
    fn project_file(&self, settings: &deadlock_project::Settings, id: &str) -> Option<String> {
        if let Some(found) = self.known_projects(settings, None).remove(id).and_then(|k| k.files.into_iter().find(|f| f.is_file())) {
            return Some(found.display().to_string());
        }
        if settings.workspace.is_empty() || !deadlock_pipeline::storage::is_project_id(id) {
            return None;
        }
        let copy = Path::new(&settings.workspace).join(id).join("project.dmb.json");
        copy.is_file().then(|| copy.display().to_string())
    }

    /// Every VPK in the game's addons folder with what it holds, the state of
    /// its hero models, its label and, when certain, the project that made it.
    pub fn installed_mods(&self) -> Result<Vec<InstalledModView>> {
        self.operation("Check installed mods", |_, token| {
            let settings = self.settings()?;
            let scan = mod_update::scan(&settings, token).map_err(AppError::from)?;
            let outputs = if settings.workspace.is_empty() {
                vec![]
            } else {
                mod_update::own_outputs(Path::new(&settings.workspace), token).map_err(AppError::from)?
            };
            let records: Records = read(&self.records_path());
            let labels: Labels = read(&self.labels_path());
            let workspace = PathBuf::from(&settings.workspace);
            let project = |id: &str| {
                let summary = (!settings.workspace.is_empty()).then(|| mod_update::project_summary(&workspace, id)).flatten();
                (summary, self.project_file(&settings, id))
            };
            Ok(join(scan, &records.records, &labels.labels, &outputs, &project))
        })
        .map(|(mods, _)| mods)
    }

    /// Keeps `text` as the label of the VPK with this content identity (an
    /// empty text removes it). Only the app's state folder is written; no
    /// operation lease is taken, so a label can be typed while the app works.
    pub fn set_installed_mod_label(&self, identity: &str, file: &str, text: &str) -> Result<Option<String>> {
        if !valid_identity(identity) {
            return Err(AppError::validation("The label could not be saved", "Scan the installed mods again; this file's identity is not known."));
        }
        let text = clean_label(text);
        let mut labels: Labels = read(&self.labels_path());
        labels.schema = 1;
        if text.is_empty() {
            labels.labels.remove(identity);
        } else {
            labels.labels.insert(identity.to_owned(), Label { text: text.clone(), file: file.chars().take(260).collect(), updated: now() });
            prune(&mut labels.labels, |l| l.updated);
        }
        deadlock_core::safety::write_json(&self.labels_path(), &labels).map_err(|e| AppError::validation("The label could not be saved", e))?;
        Ok((!text.is_empty()).then_some(text))
    }

    /// After an install by this app: remember which project the installed
    /// contents came from. A failure only costs the Installed mods page some
    /// knowledge, so it is logged, not raised.
    pub(crate) fn record_install(&self, project: &Project, installed: &Path) {
        let result = mod_update::identity_of(installed).map_err(|e| e.to_string()).and_then(|identity| {
            let mut records: Records = read(&self.records_path());
            records.schema = 1;
            records.records.insert(
                identity,
                InstallRecord {
                    project_id: project.id.clone(),
                    project_name: project.name.clone(),
                    hero: project.hero_name.clone(),
                    file: installed.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_owned(),
                    recorded: now(),
                },
            );
            prune(&mut records.records, |r| r.recorded);
            deadlock_core::safety::write_json(&self.records_path(), &records).map_err(|e| e.to_string())
        });
        if let Err(error) = result {
            let _ = self.append_log("warning", &format!("The installed file could not be recorded for Installed mods: {error}"));
        }
    }

    /// After Update rewrote a file in place: its label and install record
    /// follow the new contents.
    pub(crate) fn carry_identity(&self, before: &str, after: &str) {
        if before.is_empty() || after.is_empty() || before == after {
            return;
        }
        let mut labels: Labels = read(&self.labels_path());
        if let Some(label) = labels.labels.remove(before) {
            labels.labels.entry(after.to_owned()).or_insert(label);
            let _ = deadlock_core::safety::write_json(&self.labels_path(), &labels);
        }
        let mut records: Records = read(&self.records_path());
        if let Some(record) = records.records.remove(before) {
            records.records.entry(after.to_owned()).or_insert(record);
            let _ = deadlock_core::safety::write_json(&self.records_path(), &records);
        }
    }

    /// The addons folder, or a file directly in it, for Open / Show in folder.
    /// Anything else is refused.
    pub fn addons_target(&self, file: Option<&Path>) -> Result<PathBuf> {
        let settings = self.settings()?;
        if settings.deadlock.is_empty() {
            return Err(AppError::validation("The addons folder is not known", "Choose the Deadlock folder in Settings first."));
        }
        let folder = PathBuf::from(&settings.deadlock).join("game/citadel/addons");
        let folder = fs::canonicalize(&folder).map_err(|e| AppError::io("The game's addons folder was not found", e))?;
        let Some(file) = file else { return Ok(folder) };
        // The file's folder is resolved, not the file: a linked VPK is shown where it is listed.
        let parent = file.parent().map(fs::canonicalize).transpose().map_err(|e| AppError::io("The file is no longer in the addons folder", e))?;
        let name = file.file_name().filter(|n| *n != ".." && *n != ".");
        match (parent, name) {
            (Some(parent), Some(name)) if parent == folder && file.is_file() => Ok(folder.join(name)),
            _ => Err(AppError::validation("Only files in the game's addons folder are shown here", file.display().to_string())),
        }
    }
    /// A saved project file of a project this app knows, for Show in folder.
    pub fn known_project_file(&self, path: &Path) -> Result<PathBuf> {
        let settings = self.settings()?;
        let target = fs::canonicalize(path).map_err(|e| AppError::io("The project file was not found", e))?;
        let known = self.known_projects(&settings, None).into_values().flat_map(|k| k.files).filter_map(|f| fs::canonicalize(f).ok()).any(|f| f == target);
        let workspace = !settings.workspace.is_empty()
            && fs::canonicalize(&settings.workspace).is_ok_and(|w| target.starts_with(w))
            && target.file_name().and_then(|n| n.to_str()) == Some("project.dmb.json");
        if known || workspace {
            Ok(target)
        } else {
            Err(AppError::validation("Only saved project files this app knows are shown here", target.display().to_string()))
        }
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
            .map_err(|e| AppError::new("internal", "The installed-mods worker stopped", e))?
    }
    #[tauri::command]
    pub async fn set_installed_mod_label(state: State<'_>, identity: String, file: String, label: String) -> Result<Option<String>> {
        let app = state.inner().clone();
        background(move || app.set_installed_mod_label(&identity, &file, &label)).await
    }
    /// Opens the addons folder, or shows one of its files selected in Explorer.
    #[tauri::command]
    pub async fn show_installed_mod(state: State<'_>, path: Option<PathBuf>) -> Result<()> {
        let app = state.inner().clone();
        background(move || {
            let target = app.addons_target(path.as_deref())?;
            let shown = if path.is_some() { tauri_plugin_opener::reveal_item_in_dir(&target) } else { tauri_plugin_opener::open_path(&target, None::<&str>) };
            shown.map_err(|e| AppError::new("io", "The folder could not be opened", e))
        })
        .await
    }
    /// Shows a saved project file of a known project selected in Explorer.
    #[tauri::command]
    pub async fn show_project_file(state: State<'_>, path: PathBuf) -> Result<()> {
        let app = state.inner().clone();
        background(move || {
            let target = app.known_project_file(&path)?;
            tauri_plugin_opener::reveal_item_in_dir(&target).map_err(|e| AppError::new("io", "The folder could not be opened", e))
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadlock_components::vpk_contents::Contents;
    use deadlock_pipeline::mod_update::{OwnOutput, Status};

    const A: &str = "vpk-tree-v1:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const B: &str = "vpk-tree-v1:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const C: &str = "vpk-tree-v1:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
    const ID: &str = "0123456789abcdef0123456789abcdef";

    fn installed(file: &str, identity: &str) -> InstalledMod {
        InstalledMod {
            path: format!("C:/Game/addons/{file}"),
            file: file.into(),
            bytes: 1,
            models: vec![],
            status: Status::NoHeroModel,
            note: String::new(),
            identity: identity.into(),
            unreadable: None,
            contents: Contents::default(),
            shared: vec![],
        }
    }

    #[test]
    fn records_labels_and_outputs_are_joined_by_identity_only() {
        let records = BTreeMap::from([(A.to_owned(), InstallRecord { project_id: ID.into(), project_name: "Sample".into(), hero: "Haze".into(), file: "pak05_dir.vpk".into(), recorded: 1 })]);
        let labels = BTreeMap::from([(A.to_owned(), Label { text: "Main skin".into(), file: "pak05_dir.vpk".into(), updated: 1 }), (C.to_owned(), Label { text: "From a friend".into(), file: "pak09_dir.vpk".into(), updated: 1 })]);
        let outputs = vec![OwnOutput { identity: B.into(), project_id: ID.into(), path: "C:/ws/out/pak02_dir.vpk".into() }];
        let project = |_: &str| (Some(("Sample".to_owned(), "Haze".to_owned())), None);
        // The recorded file was renumbered by a loader: the record and label follow its contents.
        let rows = join(vec![installed("pak11_dir.vpk", A), installed("pak02_dir.vpk", B), installed("pak09_dir.vpk", C), installed("broken.vpk", "")], &records, &labels, &outputs, &project);
        assert_eq!(rows[0].label.as_deref(), Some("Main skin"));
        let made = rows[0].made.as_ref().unwrap();
        assert_eq!((made.project_name.as_str(), made.source.as_str(), made.project_file.as_deref()), ("Sample", "install", None));
        assert_eq!(rows[1].made.as_ref().unwrap().source, "output");
        assert!(rows[1].label.is_none());
        // A label without a record is only a label: no project is claimed.
        assert_eq!(rows[2].label.as_deref(), Some("From a friend"));
        assert!(rows[2].made.is_none());
        assert!(rows[3].made.is_none() && rows[3].label.is_none());
        // Two projects whose outputs hold the same files: made here, but no project is named.
        let other = "fedcba9876543210fedcba9876543210";
        let outputs = vec![
            OwnOutput { identity: B.into(), project_id: ID.into(), path: "C:/ws/a/pak02_dir.vpk".into() },
            OwnOutput { identity: B.into(), project_id: other.into(), path: "C:/ws/b/pak04_dir.vpk".into() },
            OwnOutput { identity: B.into(), project_id: ID.into(), path: "C:/ws/a/pak03_dir.vpk".into() },
        ];
        let rows = join(vec![installed("pak02_dir.vpk", B)], &BTreeMap::new(), &BTreeMap::new(), &outputs, &project);
        let made = rows[0].made.as_ref().unwrap();
        assert!(made.several && made.project_id.is_empty() && made.project_name.is_empty());
        let rows = join(vec![installed("pak02_dir.vpk", B)], &BTreeMap::new(), &BTreeMap::new(), &outputs[..1], &project);
        assert!(!rows[0].made.as_ref().unwrap().several);
    }

    #[test]
    fn labels_are_kept_in_the_state_folder_and_follow_an_update() {
        let temp = tempfile::tempdir().unwrap();
        let app = Application::new(temp.path().join("state")).unwrap();
        assert_eq!(app.set_installed_mod_label(A, "pak05_dir.vpk", "  Main\u{7} skin  ").unwrap().as_deref(), Some("Main skin"));
        let long = "x".repeat(200);
        assert_eq!(app.set_installed_mod_label(B, "pak06_dir.vpk", &long).unwrap().unwrap().chars().count(), MAX_LABEL_CHARS);
        assert!(app.set_installed_mod_label("not an identity", "x.vpk", "label").is_err());
        let labels: Labels = read(&app.labels_path());
        assert_eq!(labels.labels[A].text, "Main skin");
        app.carry_identity(A, C);
        let labels: Labels = read(&app.labels_path());
        assert!(!labels.labels.contains_key(A) && labels.labels[C].text == "Main skin");
        assert_eq!(app.set_installed_mod_label(C, "pak05_dir.vpk", "   ").unwrap(), None);
        let labels: Labels = read(&app.labels_path());
        assert!(!labels.labels.contains_key(C));
    }
}
