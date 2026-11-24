//! The movement preview's cloth packet (`deadlock_cloth_authoring::preview_cloth`):
//! written by Analyze as a file of its own in the project workspace, never
//! inside the inspection report or the saved setup (those are hashed for build
//! freshness and returned with every inspection), and read back by the page
//! without the operation lease.
use deadlock_cloth_authoring::{preview_cloth, secondary::Setup};
use deadlock_core::{safety, CoreError, Result};
use deadlock_project::{Project, Settings};
use std::path::Path;

/// Writes the packet of an automatic-motion setup into the workspace. A setup
/// without a packet (bundled presets) or a failed prediction writes nothing:
/// the page reads a packet only for the proxy it was made for and otherwise
/// keeps its older simulation.
pub fn write(root: &Path, setup: &Setup) -> Result<bool> {
    // Never let the preview's prediction stop Analyze, not even by a panic.
    let predicted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| preview_cloth::predict(setup)));
    let Ok(Ok(Some(prediction))) = predicted else {
        return Ok(false);
    };
    safety::write_json(&root.join(preview_cloth::FILE_NAME), &prediction.packet)?;
    Ok(true)
}

/// The packet's JSON for this project when it was made for `proxy_sha256`
/// (the inspection report's `secondary_motion.proxy_sha256`) in this packet
/// version; `None` otherwise (a project analyzed before the packet existed,
/// or since changed).
pub fn read(settings: &Settings, project: &Project, proxy_sha256: &str) -> Result<Option<String>> {
    // Only looks: never creates the workspace folder (`workspace::root` would).
    if settings.workspace.is_empty() || project.validate_draft().is_err() {
        return Ok(None);
    }
    let Ok(workspace) = deadlock_project::asset_paths::resolve(Path::new(&settings.workspace)) else { return Ok(None) };
    read_file(&workspace.join(&project.id).join(preview_cloth::FILE_NAME), proxy_sha256)
}
fn read_file(path: &Path, proxy_sha256: &str) -> Result<Option<String>> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(CoreError::io("Read the movement preview cloth", e)),
    };
    #[derive(serde::Deserialize)]
    struct Header { version: u32, proxy_sha256: String }
    let Ok(header) = serde_json::from_str::<Header>(&text) else { return Ok(None) };
    Ok((header.version == preview_cloth::VERSION && header.proxy_sha256 == proxy_sha256).then_some(text))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_packet_is_read_only_for_its_own_proxy_and_version() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(preview_cloth::FILE_NAME);
        assert_eq!(read_file(&path, "abc").unwrap(), None);
        let text = format!(r#"{{"version":{},"proxy_sha256":"abc","source_sha256":"x"}}"#, preview_cloth::VERSION);
        std::fs::write(&path, &text).unwrap();
        assert_eq!(read_file(&path, "abc").unwrap().as_deref(), Some(text.as_str()));
        assert_eq!(read_file(&path, "other").unwrap(), None);
        std::fs::write(&path, format!(r#"{{"version":{},"proxy_sha256":"abc"}}"#, preview_cloth::VERSION + 1)).unwrap();
        assert_eq!(read_file(&path, "abc").unwrap(), None);
        std::fs::write(&path, "not json").unwrap();
        assert_eq!(read_file(&path, "abc").unwrap(), None);
    }
    #[test]
    fn reading_without_a_workspace_creates_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let mut settings = Settings::default();
        let mut project = Project::default();
        project.id = "0123456789abcdef0123456789abcdef".into();
        assert_eq!(read(&settings, &project, "abc").unwrap(), None);
        settings.workspace = dir.path().join("workspaces").to_string_lossy().into_owned();
        assert_eq!(read(&settings, &project, "abc").unwrap(), None);
        assert!(!dir.path().join("workspaces").exists());
    }
}
