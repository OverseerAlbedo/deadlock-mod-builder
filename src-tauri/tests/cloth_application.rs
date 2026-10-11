use deadlock_core::TimingStatus;
use deadlock_desktop::app::Application;
use deadlock_project::Project;
use std::fs;
#[test]
fn failed_cloth_preview_keeps_source_settings_and_durable_evidence() {
    let temp = tempfile::tempdir().unwrap();
    let app = Application::new(temp.path().join("state")).unwrap();
    let mut settings = app.settings().unwrap();
    settings.workspace = temp.path().join("work").to_str().unwrap().into();
    app.save_settings(&settings).unwrap();
    let source = temp.path().join("garment.dmx");
    let bytes = b"malformed source must remain untouched";
    fs::write(&source, bytes).unwrap();
    let mut project = Project::default();
    project.original_model = "models/test/hero.vmdl".into();
    project.dmx = source.to_str().unwrap().into();
    let before = project.clone();
    assert!(app.cloth_setup(project.clone()).is_err());
    assert_eq!(project, before);
    assert_eq!(fs::read(&source).unwrap(), bytes);
    assert_eq!(app.settings().unwrap(), settings);
    assert!(!app.is_busy());
    let root = deadlock_pipeline::workspace::root(&settings, &project).unwrap();
    let failure: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("automatic-cloth-analysis.json")).unwrap())
            .unwrap();
    assert_eq!(failure["status"], "failed");
    assert_eq!(failure["runtime_ready"], false);
    assert_eq!(failure["published"], false);
    let timings = app.get_timings().unwrap();
    assert_eq!(timings.status, TimingStatus::Failed);
    assert!(timings
        .steps
        .iter()
        .any(|s| s.name == "Read cloth source DMX" && s.status == TimingStatus::Failed));
    assert!(app
        .log_text()
        .unwrap()
        .contains("Step time: Read cloth source DMX"));
    assert!(app.skirt_analyze(project).is_err());
    assert!(!app.is_busy());
    let failure: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("skirt-physics-analysis.json")).unwrap())
            .unwrap();
    assert_eq!(failure["published"], false);
}
