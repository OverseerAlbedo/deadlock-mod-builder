//! Synthetic source-only assembly through the typed parser and actual source pipeline.
#![cfg(all(
    target_arch = "x86_64",
    any(target_os = "linux", target_os = "windows")
))]
use deadlock_core::{
    process::{CommandSpec, ProcessResult, Runner, RunnerOptions, ToolExecutor},
    safety, CancellationToken, CoreError, Result, TimingRecorder,
};
use deadlock_pipeline::{
    cloth_authoring::{ClothAnalysis, ClothAuthoringService},
    inspect::ModelPipeline,
};
use deadlock_project::{Project, Settings};
use deadlock_resource::text::Document;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/cloth-authoring")
        .join(name)
}
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap()
}
fn write(path: &Path, bytes: impl AsRef<[u8]>) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap()
}
fn setup(root: &Path) -> (Project, Settings) {
    let mut project: Project =
        serde_json::from_slice(&fs::read(fixture("project.json")).unwrap()).unwrap();
    project.dmx = root.join("source.dmx").display().to_string();
    write(
        Path::new(&project.dmx),
        fs::read(fixture("source.dmx")).unwrap(),
    );
    project.imported_source = root.join("base").display().to_string();
    write(
        &Path::new(&project.imported_source).join(project.source_model()),
        fs::read(fixture("original.vmdl")).unwrap(),
    );
    project.original_compiled = root.join("original.vmdl_c").display().to_string();
    write(
        Path::new(&project.original_compiled),
        fs::read(fixture("original.vmdl_c")).unwrap(),
    );
    let mut settings = Settings::default();
    settings.workspace = root.join("workspaces").display().to_string();
    (project, settings)
}
struct ReaderOnly {
    inner: Runner,
    calls: AtomicUsize,
}
impl ToolExecutor for ReaderOnly {
    fn completed_tree_guard(&self) -> Result<Box<dyn Send>> {
        self.inner
            .completed_tree_guard()
            .map(|v| Box::new(v) as Box<dyn Send>)
    }
    fn run(&self, spec: &CommandSpec) -> Result<ProcessResult> {
        if !spec
            .program
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with("Dmb.ClothReader"))
        {
            return Err(CoreError::Integrity(
                "Source-only fixture forbids native compiler, packer and other tools".into(),
            ));
        }
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.inner.run(spec)
    }
}
#[test]
fn isolated_native_review_workspace_has_all_inputs_and_no_build_authority() {
    let temp = tempfile::tempdir().unwrap();
    let (p, s) = setup(temp.path());
    let before = p.clone();
    let original_source = fs::read(&p.dmx).unwrap();
    let original_model = fs::read(Path::new(&p.imported_source).join(p.source_model())).unwrap();
    let cancel = CancellationToken::new();
    let clock = TimingRecorder::new("cloth-authoring");
    let runner = ReaderOnly {
        inner: Runner::new(
            temp.path().join("logs"),
            cancel.clone(),
            RunnerOptions::default(),
        )
        .unwrap(),
        calls: AtomicUsize::new(0),
    };
    let reader = root().join("resources/native/cloth-reader");
    let model = ModelPipeline {
        settings: &s,
        runner: &runner,
        cancel: &cancel,
        clock: &clock,
        log: &|_| {},
        reader_root: &reader,
    };
    let service = ClothAuthoringService { model: &model };
    let analysis = service.analyze(&p).unwrap();
    assert!(analysis.report_path.is_file());
    let ClothAnalysis::Skirt(report) = &analysis.analysis else {
        panic!()
    };
    assert!(!report.analysis.runtime_ready);
    assert!(report.analysis.original_snapshot_verified == Some(true));
    let receipt = service
        .create_workspace(&p, false)
        .unwrap_or_else(|e| panic!("{e}; steps {:?}", clock.report().steps));
    assert_eq!(receipt.status, "authored_for_native_review");
    assert_ne!(receipt.workspace_project, p.id);
    assert!(
        !receipt.runtime_ready
            && !receipt.build_authorized
            && !receipt.published
            && !receipt.installed
            && !receipt.native_compiler_run
    );
    assert_eq!(p, before);
    assert_eq!(fs::read(&p.dmx).unwrap(), original_source);
    assert_eq!(
        fs::read(Path::new(&p.imported_source).join(p.source_model())).unwrap(),
        original_model
    );
    let authored = fs::read_to_string(&receipt.model_path).unwrap();
    let doc = Document::parse(&authored).unwrap();
    assert_eq!(doc.objects(Some("ClothProxyMeshFile")).len(), 1);
    assert_eq!(doc.objects(Some("ClothShapeCapsule")).len(), 4);
    assert!(
        Document::parse(&fs::read_to_string(&receipt.preview_before_model_path).unwrap())
            .unwrap()
            .objects(Some("ClothProxyMeshFile"))
            .is_empty()
    );
    assert_eq!(
        fs::read_to_string(&receipt.preview_model_path).unwrap(),
        deadlock_model::ag2::preview_source(&authored).unwrap()
    );
    assert!(receipt.files.contains_key("START_HERE.md"));
    assert!(!receipt.files.contains_key("build.lock"));
    assert!(!receipt.workspace_path.join("build.lock").exists());
    for (name, sha) in &receipt.files {
        assert_eq!(
            safety::digest(&receipt.workspace_path.join(name)).unwrap(),
            *sha,
            "{name}"
        )
    }
    assert_eq!(runner.calls.load(Ordering::SeqCst), 0);
    assert!(clock
        .report()
        .steps
        .iter()
        .any(|r| r.name == "Fingerprint cloth review workspace"));
    let saved: serde_json::Value =
        serde_json::from_slice(&fs::read(&receipt.report_path).unwrap()).unwrap();
    assert_eq!(saved["build_authorized"], false);
}
#[test]
fn automatic_preview_without_collision_is_isolated_and_saved_report_is_not_authority() {
    let temp = tempfile::tempdir().unwrap();
    let (mut p, s) = setup(temp.path());
    p.customization.cloth_setup = Some(
        serde_json::from_value(
            serde_json::json!({"meshes":["body"],"attachment":"top","collision":"none"}),
        )
        .unwrap(),
    );
    let cancel = CancellationToken::new();
    let clock = TimingRecorder::new("automatic-cloth");
    let runner = ReaderOnly {
        inner: Runner::new(
            temp.path().join("logs"),
            cancel.clone(),
            RunnerOptions::default(),
        )
        .unwrap(),
        calls: AtomicUsize::new(0),
    };
    let reader = root().join("resources/native/cloth-reader");
    let model = ModelPipeline {
        settings: &s,
        runner: &runner,
        cancel: &cancel,
        clock: &clock,
        log: &|_| {},
        reader_root: &reader,
    };
    let service = ClothAuthoringService { model: &model };
    // This synthetic three-vertex surface has two pins; the >50% automatic pin
    // limit intentionally refuses it. A forged previous success must not help.
    let path = model
        .root(&p)
        .unwrap()
        .join("automatic-cloth-analysis.json");
    write(&path, b"{\"status\":\"analyzed\",\"runtime_ready\":true}");
    let error = service.create_workspace(&p, true).unwrap_err();
    assert!(error.to_string().contains("attachment"), "{error}");
    let saved: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert_eq!(saved["status"], "failed");
    assert_eq!(saved["runtime_ready"], false);
    write(
        Path::new(&p.dmx),
        fs::read(fixture("automatic-source.dmx")).unwrap(),
    );
    let success = service.create_workspace(&p, true).unwrap();
    assert!(success.automatic_surface_setup);
    assert!(!success.runtime_ready && !success.build_authorized && !success.native_compiler_run);
    let doc = Document::parse(&fs::read_to_string(&success.model_path).unwrap()).unwrap();
    assert!(doc.objects(Some("ClothShapeCapsule")).is_empty());
    assert_eq!(doc.objects(Some("ClothProxyMeshFile")).len(), 1);
    cancel.cancel();
    assert!(matches!(service.analyze(&p), Err(CoreError::Cancelled)));
    assert!(!model.root(&p).unwrap().join("build.lock").exists());
}
#[test]
fn authoring_rejects_added_garment_unprepared_inputs_and_existing_workspace_lease() {
    let temp = tempfile::tempdir().unwrap();
    let (mut p, s) = setup(temp.path());
    let cancel = CancellationToken::new();
    let clock = TimingRecorder::new("authoring-gates");
    let runner = ReaderOnly {
        inner: Runner::new(
            temp.path().join("logs"),
            cancel.clone(),
            RunnerOptions::default(),
        )
        .unwrap(),
        calls: AtomicUsize::new(0),
    };
    let reader = root().join("resources/native/cloth-reader");
    let model = ModelPipeline {
        settings: &s,
        runner: &runner,
        cancel: &cancel,
        clock: &clock,
        log: &|_| {},
        reader_root: &reader,
    };
    let service = ClothAuthoringService { model: &model };
    p.customization = serde_json::from_value(
        serde_json::json!({"enabled":true,"clothing":{"enabled":true,"source":"unused.dmx"}}),
    )
    .unwrap();
    assert!(service
        .create_workspace(&p, false)
        .unwrap_err()
        .to_string()
        .contains("includes the garment"));
    p.customization = Default::default();
    p.model_input_mode = deadlock_project::ModelInputMode::UnriggedHumanoid;
    assert!(service
        .create_workspace(&p, false)
        .unwrap_err()
        .to_string()
        .contains("game-ready"));
    p.model_input_mode = deadlock_project::ModelInputMode::Prepared;
    let lease =
        deadlock_pipeline::workspace::WorkspaceLease::acquire(&model.root(&p).unwrap()).unwrap();
    assert!(service
        .create_workspace(&p, false)
        .unwrap_err()
        .to_string()
        .contains("already owns"));
    drop(lease);
    assert_eq!(runner.calls.load(Ordering::SeqCst), 0);
}
