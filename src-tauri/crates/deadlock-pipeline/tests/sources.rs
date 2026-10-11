use deadlock_core::{
    process::{CommandSpec, ProcessResult, ToolExecutor},
    CancellationToken, CoreError, Result, TimingRecorder,
};
use deadlock_pipeline::{sources::SourceService, workspace};
use deadlock_project::{Project, Settings};
use std::{fs, path::Path};
const MODEL: &str = "models/heroes/test/test.vmdl";
const TEXT:&str="{ rootNode = { _class = \"RootNode\" children = [{ _class = \"RenderMeshList\" children = [{ _class = \"RenderMeshFile\" name = \"body\" filename = \"models/heroes/test/old.dmx\" }] }] } }";
struct NoTools;
impl ToolExecutor for NoTools {
    fn run(&self, _: &CommandSpec) -> Result<ProcessResult> {
        Err(CoreError::Integrity(
            "Imported-source path must not run a tool".into(),
        ))
    }
}
fn write(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

#[test]
fn imports_original_paths_reuses_hash_cache_and_invalidates_changed_input() {
    let temp = tempfile::tempdir().unwrap();
    let imported = temp.path().join("imported");
    write(&imported.join(MODEL), TEXT.as_bytes());
    let mut project = Project::default();
    project.original_model = format!("{MODEL}_c");
    project.imported_source = imported.display().to_string();
    let mut settings = Settings::default();
    settings.workspace = temp.path().join("work").display().to_string();
    let token = CancellationToken::new();
    let clock = TimingRecorder::new("original-source");
    let service = SourceService {
        settings: &settings,
        runner: &NoTools,
        cancel: &token,
        clock: &clock,
        log: &|_| {},
    };
    let base = service.ensure_base(&project).unwrap();
    assert_eq!(fs::read(base.join(MODEL)).unwrap(), TEXT.as_bytes());
    assert_eq!(service.ensure_base(&project).unwrap(), base);
    assert!(clock
        .report()
        .steps
        .iter()
        .any(|s| s.name == "Reuse cached original sources"));
    write(
        &imported.join(MODEL),
        format!("{TEXT}\n// authored change\n").as_bytes(),
    );
    service.ensure_base(&project).unwrap();
    assert!(fs::read_to_string(base.join(MODEL))
        .unwrap()
        .contains("authored change"));
    assert_eq!(
        fs::read_dir(
            workspace::root(&settings, &project)
                .unwrap()
                .join("backups")
        )
        .unwrap()
        .count(),
        1
    );
}

#[test]
fn failed_replacement_cache_keeps_previous_original_sources() {
    let temp = tempfile::tempdir().unwrap();
    let imported = temp.path().join("imported");
    write(&imported.join(MODEL), TEXT.as_bytes());
    let mut project = Project::default();
    project.original_model = MODEL.into();
    project.imported_source = imported.display().to_string();
    let mut settings = Settings::default();
    settings.workspace = temp.path().join("work").display().to_string();
    let token = CancellationToken::new();
    let clock = TimingRecorder::new("original-source");
    let service = SourceService {
        settings: &settings,
        runner: &NoTools,
        cancel: &token,
        clock: &clock,
        log: &|_| {},
    };
    let base = service.ensure_base(&project).unwrap();
    write(&imported.join(MODEL), b"malformed source");
    assert!(service.ensure_base(&project).is_err());
    assert_eq!(fs::read(base.join(MODEL)).unwrap(), TEXT.as_bytes());
    let root = workspace::root(&settings, &project).unwrap();
    assert!(!fs::read_dir(root).unwrap().any(|e| e
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with("base-pending-")));
}

#[test]
fn missing_sources_and_cancellation_fail_without_partial_cache() {
    let temp = tempfile::tempdir().unwrap();
    let mut project = Project::default();
    project.original_model = MODEL.into();
    let mut settings = Settings::default();
    settings.workspace = temp.path().join("work").display().to_string();
    let token = CancellationToken::new();
    let clock = TimingRecorder::new("original-source");
    let service = SourceService {
        settings: &settings,
        runner: &NoTools,
        cancel: &token,
        clock: &clock,
        log: &|_| {},
    };
    assert!(service.ensure_base(&project).is_err());
    let imported = temp.path().join("imported");
    write(&imported.join(MODEL), TEXT.as_bytes());
    project.imported_source = imported.display().to_string();
    token.cancel();
    assert!(matches!(
        service.ensure_base(&project),
        Err(CoreError::Cancelled)
    ));
    assert!(!workspace::root(&settings, &project)
        .unwrap()
        .join("base")
        .exists());
}

#[test]
fn game_archive_sources_are_shared_per_hero_and_game_version() {
    use deadlock_pipeline::sources::{base_dir, BaseFingerprint};
    let temp = tempfile::tempdir().unwrap();
    let pak = temp.path().join("pak01_dir.vpk");
    write(&pak, b"game archive index");
    let mut settings = Settings::default();
    settings.workspace = temp.path().join("work").display().to_string();
    settings.pak = pak.display().to_string();
    let fingerprint = BaseFingerprint {
        model: MODEL.into(),
        pak_index_sha256: Some(deadlock_core::safety::digest(&pak).unwrap()),
        pak: Some(pak.canonicalize().unwrap()),
        ..Default::default()
    };
    // Two projects of the same hero that each extracted their own copy earlier;
    // the second one also decompiled an extra material.
    let projects: Vec<Project> = (0..2)
        .map(|i| {
            let mut project = Project::default();
            project.original_model = MODEL.into();
            let root = workspace::root(&settings, &project).unwrap();
            write(&root.join("base").join(MODEL), TEXT.as_bytes());
            if i == 1 {
                write(&root.join("base/materials/extra.vmat"), b"extra");
            }
            write(
                &root.join("base-fingerprint.json"),
                &serde_json::to_vec(&fingerprint).unwrap(),
            );
            project
        })
        .collect();
    let token = CancellationToken::new();
    let clock = TimingRecorder::new("original-source");
    let service = SourceService {
        settings: &settings,
        runner: &NoTools,
        cancel: &token,
        clock: &clock,
        log: &|_| {},
    };
    let first = service.ensure_base(&projects[0]).unwrap();
    let second = service.ensure_base(&projects[1]).unwrap();
    assert_eq!(first, second);
    assert!(first.starts_with(temp.path().join("work/shared-originals")));
    assert_eq!(fs::read(first.join(MODEL)).unwrap(), TEXT.as_bytes());
    assert_eq!(fs::read(first.join("materials/extra.vmat")).unwrap(), b"extra");
    for project in &projects {
        let root = workspace::root(&settings, project).unwrap();
        assert!(!root.join("base").exists());
        assert_eq!(base_dir(&settings, project).unwrap(), first);
    }
    // Reuse needs no tool and keeps the shared copy.
    assert_eq!(service.ensure_base(&projects[0]).unwrap(), first);
}
