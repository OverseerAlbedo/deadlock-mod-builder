use crate::{invalid, vpk_error};
use deadlock_core::{CoreError, Result};
use deadlock_project::{asset_paths, Project, Settings};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

pub fn root(settings: &Settings, project: &Project) -> Result<PathBuf> {
    project
        .validate_draft()
        .map_err(|e| invalid(e.to_string()))?;
    if settings.workspace.is_empty() {
        return Err(invalid("Choose a workspace in Setup."));
    }
    let workspace =
        asset_paths::resolve(Path::new(&settings.workspace)).map_err(|e| invalid(e.to_string()))?;
    let root = workspace.join(&project.id);
    for path in [
        &settings.deadlock,
        &settings.csdk,
        &settings.cswin,
        &settings.optional_cs2,
    ] {
        if !path.is_empty()
            && root.starts_with(
                asset_paths::resolve(Path::new(path)).map_err(|e| invalid(e.to_string()))?,
            )
        {
            return Err(invalid("Workspace must be outside the Deadlock, CSDK and selected model-tool installations."));
        }
    }
    deadlock_vpk::check_unlinked_path(&root, true).map_err(vpk_error)?;
    fs::create_dir_all(&root).map_err(|e| CoreError::io("Create project workspace", e))?;
    Ok(root)
}

/// A crash leaves this lock for explicit recovery; a normal error never leaves a
/// live owner behind. Drop only removes the exact lock this lease wrote.
pub struct WorkspaceLease {
    path: PathBuf,
    identity: String,
    retained: bool,
}
impl WorkspaceLease {
    pub fn acquire(root: &Path) -> Result<Self> {
        deadlock_vpk::check_unlinked_path(root, false).map_err(vpk_error)?;
        let path = root.join("build.lock");
        let identity = format!("{}\n{}\n", uuid::Uuid::new_v4(), std::process::id());
        let mut file = OpenOptions::new().create_new(true).write(true).open(&path)
            .map_err(|e| if e.kind() == std::io::ErrorKind::AlreadyExists {
                invalid(format!("A build already owns this workspace. If a previous process crashed, close it before removing {}.", path.display()))
            } else { CoreError::io("Create workspace lock", e) })?;
        if let Err(e) = file
            .write_all(identity.as_bytes())
            .and_then(|_| file.sync_all())
        {
            let _ = fs::remove_file(&path);
            return Err(CoreError::io("Save workspace lock", e));
        }
        Ok(Self {
            path,
            identity,
            retained: false,
        })
    }
}
impl Drop for WorkspaceLease {
    fn drop(&mut self) {
        if !self.retained && fs::read_to_string(&self.path).ok().as_deref() == Some(&self.identity)
        {
            let _ = fs::remove_file(&self.path);
        }
    }
}

/// Process-backed operations retain their exact lock if tree/capture cleanup
/// cannot be proven. Normal cancellation is not a reason to retain a clean lock.
pub struct RunnerWorkspaceLease<'a> {
    lease: Option<WorkspaceLease>,
    runner: &'a dyn deadlock_core::process::ToolExecutor,
}
impl WorkspaceLease {
    pub fn acquire_for_runner<'a>(
        root: &Path,
        runner: &'a dyn deadlock_core::process::ToolExecutor,
    ) -> Result<RunnerWorkspaceLease<'a>> {
        Ok(RunnerWorkspaceLease {
            lease: Some(Self::acquire(root)?),
            runner,
        })
    }
}
impl RunnerWorkspaceLease<'_> {
    fn finish(&mut self) -> Result<()> {
        let Some(mut lease) = self.lease.take() else {
            return Ok(());
        };
        match self.runner.completed_tree_guard() {
            Ok(_barrier) => {
                drop(lease);
                Ok(())
            }
            Err(error) => {
                lease.retained = true;
                let message = format!("Workspace lock retained because owned tool cleanup is unproven: {error}. Close any remaining tool processes before recovering {}.", lease.path.display());
                let evidence = lease.path.with_file_name("build.lock.recovery.json");
                let report = serde_json::json!({"status":"recovery_required","workspace_lock":lease.path,"owner":lease.identity,"reason":error.to_string(),"workspace_reusable":false});
                // Keeping the original lock is authoritative even if the disk
                // cannot accept supplementary diagnostics.
                let _ = deadlock_core::safety::write_json(&evidence, &report);
                Err(invalid(message))
            }
        }
    }
    pub fn release(mut self) -> Result<()> {
        self.finish()
    }
}
impl Drop for RunnerWorkspaceLease<'_> {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}
