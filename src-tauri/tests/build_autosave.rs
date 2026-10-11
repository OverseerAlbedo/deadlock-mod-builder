use deadlock_core::{timing::read_report, TimingStatus};
use deadlock_desktop::app::Application;
use deadlock_project::{asset_paths, Project};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn files(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for entry in fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            found.extend(files(&path));
        } else {
            found.push(path);
        }
    }
    found
}

#[test]
fn requested_project_is_saved_before_missing_source_failure_without_publishing() {
    let temp = tempfile::tempdir().unwrap();
    let app = Application::new(temp.path().join("state")).unwrap();
    let mut settings = app.settings().unwrap();
    settings.workspace = temp.path().join("work").to_str().unwrap().into();
    settings.deadlock = temp.path().join("untouched-game").to_str().unwrap().into();
    app.save_settings(&settings).unwrap();

    fs::create_dir(temp.path().join("inputs")).unwrap();
    let path = temp.path().join("projects/character.dmb.json");
    let mut project = Project::default();
    project.original_model = "models/heroes_staging/hornet_v3/hornet.vmdl".into();
    project.hero_name = "Vindicta".into();
    // Saved projects reload paths in resolved native form (Windows separators and
    // long names, e.g. for a RUNNER~1 temp folder), so start from that form.
    project.dmx = asset_paths::resolve(&temp.path().join("inputs"))
        .unwrap()
        .join("source_missing.dmx")
        .to_str()
        .unwrap()
        .into();
    project.name = "prior_saved_choices".into();
    project.save(&path).unwrap();
    project.name = "latest_build_choices".into();
    project.vpk_number = 29;
    project.auto_split = true;
    project.split_target = 320000;
    project.compile_model = true;
    project.build_vpk = true;
    project.install = true;
    let previous_bytes = fs::read(&path).unwrap();

    let error = app
        .build_project(project.clone(), false, false, Some(path.clone()))
        .unwrap_err();
    assert!(error.to_string().contains("source_missing.dmx"), "{error}");
    assert_ne!(fs::read(&path).unwrap(), previous_bytes);
    let saved = Project::load(&path).unwrap();
    assert_eq!(saved.id, project.id);
    assert_eq!(saved.name, "latest_build_choices");
    assert_eq!(saved.original_model, project.original_model);
    assert_eq!(saved.dmx, project.dmx);
    assert_eq!((saved.vpk_number, saved.split_target), (29, 320000));
    assert!(saved.auto_split && saved.compile_model && saved.build_vpk && saved.install);
    assert_eq!(app.settings().unwrap(), settings);

    let timings = app.get_timings().unwrap();
    assert_eq!(timings.operation, "Build project");
    assert_eq!(timings.status, TimingStatus::Failed);
    assert!(timings.finished_utc.is_some());
    let save_at = timings
        .steps
        .iter()
        .position(|s| s.name == "Save project before build")
        .unwrap();
    let failure_at = timings
        .steps
        .iter()
        .position(|s| s.status == TimingStatus::Failed)
        .unwrap();
    assert!(save_at < failure_at);
    assert_eq!(timings.steps[save_at].status, TimingStatus::Succeeded);
    assert!(timings
        .steps
        .iter()
        .any(|s| s.name == "Analyze replacement" && s.status == TimingStatus::Failed));
    assert!(timings
        .steps
        .iter()
        .any(|s| s.name == "Release build workspace" && s.status == TimingStatus::Succeeded));
    assert!(!timings
        .steps
        .iter()
        .any(|s| s.name == "Save final project state"));
    let evidence = temp.path().join("state/operations").join(&timings.run_id);
    let persisted = read_report(&evidence.join("timings.json")).unwrap();
    assert_eq!(persisted.status, TimingStatus::Failed);
    assert_eq!(persisted.steps.len(), timings.steps.len());
    assert_eq!(
        persisted.events.len(),
        fs::read_to_string(evidence.join("timing-events.jsonl"))
            .unwrap()
            .lines()
            .count()
    );
    let log = app.log_text().unwrap();
    assert!(log.contains("Step time: Save project before build"));
    assert!(log.contains("Step time: Analyze replacement"));
    assert!(log.contains("source_missing.dmx"));

    let root = deadlock_pipeline::workspace::root(&settings, &project).unwrap();
    assert!(!root.join("build.lock").exists());
    assert!(!root.join("build-manifest.json").exists());
    assert!(!app.is_busy());
    assert!(!files(temp.path()).iter().any(|p| p
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("vpk"))));
    assert!(!Path::new(&settings.deadlock).exists());
    // An actual next operation can acquire ownership after the failed build.
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("crates/deadlock-dmx/tests/fixtures/basic-v9.dmx");
    app.analyze_dmx(&fixture).unwrap();
    assert!(!app.is_busy());
}
