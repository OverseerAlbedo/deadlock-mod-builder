//! Autosave and recovery. The open project is saved every few minutes while it
//! has unsaved changes to `state/autosave/<project id>/<unix ms>.json` (the
//! newest five are kept). A session marker tells the next start whether the
//! app closed normally; after a crash the app offers the newest autosave.
//! Autosaves are plain file writes and never take the operation lock.
use crate::app::{AppError, Application, ProjectDocument, Result};
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// Autosaves kept per project.
pub const KEEP: usize = 5;
/// Autosave folders of projects untouched this long are removed at start.
pub const MAX_AGE: Duration = Duration::from_secs(14 * 24 * 60 * 60);

#[derive(Debug, Clone, Serialize)]
pub struct Autosave {
    /// File name inside the project's autosave folder.
    pub file: String,
    pub id: String,
    pub name: String,
    /// The project file it belongs to (none for a project never saved).
    pub path: Option<String>,
    /// Milliseconds since 1970.
    pub saved_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Recovery {
    /// The last session ended without closing the app (crash, power loss …).
    pub unclean: bool,
    /// The newest autosave of each project, newest first.
    pub autosaves: Vec<Autosave>,
}

fn valid_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

fn io(message: &str) -> impl FnOnce(std::io::Error) -> AppError + '_ {
    move |error| AppError::io(message, error)
}

/// The autosave files of one folder, oldest first (names are the save time).
fn files(folder: &Path) -> Vec<(u64, PathBuf)> {
    let mut out: Vec<(u64, PathBuf)> = fs::read_dir(folder)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let path = e.path();
            let stamp = path.file_stem()?.to_str()?.parse().ok()?;
            (path.extension()? == "json").then_some((stamp, path))
        })
        .collect();
    out.sort();
    out
}

impl Application {
    fn autosave_root(&self) -> PathBuf {
        self.state_directory.join("autosave")
    }
    fn session_marker(&self) -> PathBuf {
        self.state_directory.join("session-open")
    }
    fn unclean_marker(&self) -> PathBuf {
        self.state_directory.join("session-unclean")
    }

    /// At start: a marker left by the last session means it did not close
    /// normally (kept as `session-unclean` until recovery is answered). Also
    /// removes autosave folders untouched for two weeks.
    pub fn begin_session(&self) {
        let marker = self.session_marker();
        if marker.is_file() {
            let _ = fs::rename(&marker, self.unclean_marker());
        }
        let _ = fs::write(&marker, now_ms().to_string());
        for entry in fs::read_dir(self.autosave_root()).into_iter().flatten().filter_map(|e| e.ok()) {
            let newest = files(&entry.path()).last().map(|(stamp, _)| *stamp);
            if newest.is_none_or(|stamp| now_ms().saturating_sub(stamp) > MAX_AGE.as_millis() as u64) {
                let _ = fs::remove_dir_all(entry.path());
            }
        }
    }
    /// When the app closes normally.
    pub fn end_session(&self) {
        let _ = fs::remove_file(self.session_marker());
    }

    pub fn autosave_project(&self, document: &ProjectDocument) -> Result<Autosave> {
        let id = &document.project.id;
        if !valid_id(id) {
            return Err(AppError::validation("Autosave skipped", "The project has no valid ID."));
        }
        let folder = self.autosave_root().join(id);
        fs::create_dir_all(&folder).map_err(io("Could not create the autosave folder"))?;
        // Never two saves with one name, even within a millisecond.
        let stamp = now_ms().max(files(&folder).last().map_or(0, |(s, _)| s + 1));
        let file = folder.join(format!("{stamp}.json"));
        deadlock_core::safety::write_json(&file, document)?;
        let saved = files(&folder);
        for (_, old) in &saved[..saved.len().saturating_sub(KEEP)] {
            let _ = fs::remove_file(old);
        }
        Ok(Autosave {
            file: format!("{stamp}.json"),
            id: id.clone(),
            name: document.project.name.clone(),
            path: document.path.clone(),
            saved_ms: stamp,
        })
    }

    /// After an explicit save or a build: the project file is current again.
    pub fn clear_autosaves(&self, id: &str) -> Result<()> {
        if !valid_id(id) {
            return Ok(());
        }
        match fs::remove_dir_all(self.autosave_root().join(id)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(AppError::io("Could not remove old autosaves", e)),
            _ => Ok(()),
        }
    }

    /// What the start can offer: the newest autosave of each project, and
    /// whether the last session crashed.
    pub fn recovery(&self) -> Recovery {
        let mut autosaves = vec![];
        for entry in fs::read_dir(self.autosave_root()).into_iter().flatten().filter_map(|e| e.ok()) {
            let id = entry.file_name().to_string_lossy().into_owned();
            if !valid_id(&id) {
                continue;
            }
            let Some((stamp, path)) = files(&entry.path()).pop() else { continue };
            let Ok(document) = fs::read(&path).map_err(|_| ()).and_then(|b| serde_json::from_slice::<ProjectDocument>(&b).map_err(|_| ())) else { continue };
            autosaves.push(Autosave {
                file: format!("{stamp}.json"),
                id,
                name: document.project.name,
                path: document.path,
                saved_ms: stamp,
            });
        }
        autosaves.sort_by(|a, b| b.saved_ms.cmp(&a.saved_ms));
        Recovery { unclean: self.unclean_marker().is_file(), autosaves }
    }
    /// The recovery question was answered (restored or declined).
    pub fn dismiss_recovery(&self) {
        let _ = fs::remove_file(self.unclean_marker());
    }

    pub fn restore_autosave(&self, id: &str, file: &str) -> Result<ProjectDocument> {
        let stamp = file.strip_suffix(".json").filter(|s| !s.is_empty() && s.bytes().all(|c| c.is_ascii_digit()));
        if !valid_id(id) || stamp.is_none() {
            return Err(AppError::validation("Autosave could not be restored", "Unknown autosave."));
        }
        let bytes = fs::read(self.autosave_root().join(id).join(file)).map_err(io("Autosave could not be read"))?;
        let document: ProjectDocument =
            serde_json::from_slice(&bytes).map_err(|e| AppError::validation("Autosave could not be restored", e))?;
        if document.project.id != id {
            return Err(AppError::validation("Autosave could not be restored", "The autosave belongs to another project."));
        }
        Ok(document)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadlock_project::Project;

    fn document(name: &str) -> ProjectDocument {
        let mut project = Project::default();
        project.id = "0123456789abcdef0123456789abcdef".into();
        project.name = name.into();
        ProjectDocument { path: Some(r"C:\Projects\yvonne.dmb.json".into()), project }
    }

    #[test]
    fn keeps_the_newest_five_and_offers_them_after_a_crash() {
        let temp = tempfile::tempdir().unwrap();
        let app = Application::new(temp.path().join("state")).unwrap();
        app.begin_session();
        assert!(!app.recovery().unclean);
        for n in 0..7 {
            app.autosave_project(&document(&format!("Yvonne {n}"))).unwrap();
        }
        let folder = app.autosave_root().join("0123456789abcdef0123456789abcdef");
        assert_eq!(files(&folder).len(), KEEP);
        // The app was killed: the next start finds the marker.
        let next = Application::new(temp.path().join("state")).unwrap();
        next.begin_session();
        let recovery = next.recovery();
        assert!(recovery.unclean);
        assert_eq!(recovery.autosaves.len(), 1);
        let newest = &recovery.autosaves[0];
        assert_eq!(newest.name, "Yvonne 6");
        let restored = next.restore_autosave(&newest.id, &newest.file).unwrap();
        assert_eq!(restored.project.name, "Yvonne 6");
        assert_eq!(restored.path.as_deref(), Some(r"C:\Projects\yvonne.dmb.json"));
        next.dismiss_recovery();
        assert!(!next.recovery().unclean);
        // A normal close leaves no marker; a save clears the autosaves.
        next.end_session();
        let after = Application::new(temp.path().join("state")).unwrap();
        after.begin_session();
        assert!(!after.recovery().unclean);
        after.clear_autosaves(&newest.id).unwrap();
        assert!(after.recovery().autosaves.is_empty());
        // Names from outside never leave the autosave folder.
        assert!(after.restore_autosave("../../x", "1.json").is_err());
        assert!(after.restore_autosave(&newest.id, "../settings.json").is_err());
    }
}
