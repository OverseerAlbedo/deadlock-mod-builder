//! Cleanup state, rather than operation success/cancellation, controls reuse.
use deadlock_core::{
    process::{CommandSpec, ProcessResult, Runner, RunnerOptions, ToolExecutor},
    CancellationToken, CoreError, Result, TimingRecorder,
};
use deadlock_pipeline::{build::BuildService, inspect::ModelPipeline, workspace::WorkspaceLease};
use deadlock_project::{Project, Settings};
use std::{fs, path::Path, sync::Mutex};
struct Poisoned;
impl ToolExecutor for Poisoned {
    fn run(&self, _: &CommandSpec) -> Result<ProcessResult> {
        Err(CoreError::Integrity("fixture never launches tools".into()))
    }
    fn completed_tree_guard(&self) -> Result<Box<dyn Send>> {
        Err(CoreError::Integrity(
            "owned process termination is unproven".into(),
        ))
    }
}
#[test]
fn poisoned_build_keeps_exact_lock_and_files_blocks_fresh_runner_reuse() {
    let temp = tempfile::tempdir().unwrap();
    let mut settings = Settings::default();
    settings.workspace = temp.path().join("work").display().to_string();
    let mut p = Project::default();
    p.original_model = "models/heroes/test/test.vmdl_c".into();
    p.compile_model = false;
    p.build_vpk = false;
    let cancel = CancellationToken::new();
    let clock = TimingRecorder::new("poisoned-build");
    let logs = Mutex::new(vec![]);
    let log = |s: &str| logs.lock().unwrap().push(s.to_string());
    let service = BuildService {
        model: ModelPipeline {
            settings: &settings,
            runner: &Poisoned,
            cancel: &cancel,
            clock: &clock,
            log: &log,
            reader_root: Path::new("unused"),
        },
    };
    let root = service.model.root(&p).unwrap();
    fs::write(root.join("compiler-input.keep"), b"live tool input").unwrap();
    assert!(service.build(&mut p, false, false).is_err());
    assert!(root.join("build.lock").exists());
    assert_eq!(
        fs::read(root.join("compiler-input.keep")).unwrap(),
        b"live tool input"
    );
    let evidence: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("build.lock.recovery.json")).unwrap()).unwrap();
    assert_eq!(evidence["workspace_reusable"], false);
    assert!(logs
        .lock()
        .unwrap()
        .iter()
        .any(|s| s.contains("lock retained")));
    let fresh = Runner::new(
        temp.path().join("new-logs"),
        CancellationToken::new(),
        RunnerOptions::default(),
    )
    .unwrap();
    assert!(WorkspaceLease::acquire_for_runner(&root, &fresh).is_err());
}
#[test]
fn clean_cancellation_releases_build_install_and_raii_leases() {
    let temp = tempfile::tempdir().unwrap();
    let mut settings = Settings::default();
    settings.workspace = temp.path().join("work").display().to_string();
    let mut p = Project::default();
    p.original_model = "models/heroes/test/test.vmdl_c".into();
    p.compile_model = false;
    p.build_vpk = false;
    let cancel = CancellationToken::new();
    let clock = TimingRecorder::new("clean-cancel");
    let runner = Runner::new(
        temp.path().join("logs"),
        cancel.clone(),
        RunnerOptions::default(),
    )
    .unwrap();
    let service = BuildService {
        model: ModelPipeline {
            settings: &settings,
            runner: &runner,
            cancel: &cancel,
            clock: &clock,
            log: &|_| {},
            reader_root: Path::new("unused"),
        },
    };
    let root = service.model.root(&p).unwrap();
    cancel.cancel();
    assert!(service.build(&mut p, false, false).is_err());
    assert!(!root.join("build.lock").exists());
    assert!(!root.join("build.lock.recovery.json").exists());
    assert!(service
        .install(&mut p, Path::new("missing.vpk"), false)
        .is_err());
    assert!(!root.join("build.lock").exists());
    let lease = WorkspaceLease::acquire_for_runner(&root, &runner).unwrap();
    drop(lease);
    assert!(!root.join("build.lock").exists());
}
#[test]
fn poisoned_raii_cleanup_retains_lock_on_early_return_or_unwind() {
    let temp = tempfile::tempdir().unwrap();
    {
        let _lease = WorkspaceLease::acquire_for_runner(temp.path(), &Poisoned).unwrap();
    }
    assert!(temp.path().join("build.lock").exists());
    assert!(WorkspaceLease::acquire(temp.path()).is_err());
}
