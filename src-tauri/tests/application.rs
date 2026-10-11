use deadlock_core::TimingStatus;
use deadlock_desktop::{
    app::Application,
    intake::{validate_asset_path, AssetKind},
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("crates/deadlock-dmx/tests/fixtures")
        .join(name)
}

#[test]
fn real_dmx_analysis_records_each_stage_and_durable_log() {
    let temp = tempfile::tempdir().unwrap();
    let app = Application::new(temp.path().to_path_buf()).unwrap();
    let events = Arc::new(Mutex::new(Vec::new()));
    let seen = events.clone();
    app.set_timing_sink(Arc::new(move |event| {
        seen.lock().unwrap().push(event.clone())
    }));
    let result = app.analyze_dmx(&fixture("basic-v9.dmx")).unwrap();
    assert_eq!(result.summary.summary.meshes.len(), 2);
    assert_eq!(
        result
            .timings
            .steps
            .iter()
            .map(|step| step.name.as_str())
            .collect::<Vec<_>>(),
        vec![
            "Read DMX",
            "Parse DMX",
            "Inspect meshes, materials and bones"
        ]
    );
    assert!(result
        .timings
        .steps
        .iter()
        .all(|step| step.status == TimingStatus::Succeeded));
    assert_eq!(result.timings.status, TimingStatus::Succeeded);
    assert!(!app.is_busy());
    let folder = temp.path().join("operations").join(&result.timings.run_id);
    let persisted = deadlock_core::timing::read_report(&folder.join("timings.json")).unwrap();
    assert_eq!(persisted.run_id, result.timings.run_id);
    assert_eq!(
        fs::read_to_string(folder.join("timing-events.jsonl"))
            .unwrap()
            .lines()
            .count(),
        result.timings.events.len()
    );
    assert_eq!(events.lock().unwrap().len(), result.timings.events.len());
    let log = app.log_text().unwrap();
    for step in &result.timings.steps {
        assert!(log.contains(&format!("Step time: {}", step.name)));
    }
    let exported = temp.path().join("copied.log");
    app.save_log(&exported).unwrap();
    assert_eq!(fs::read_to_string(exported).unwrap(), log);
}

#[test]
fn failed_parse_still_saves_timings_and_releases_operation() {
    let temp = tempfile::tempdir().unwrap();
    let app = Application::new(temp.path().join("state")).unwrap();
    let bad = temp.path().join("bad.dmx");
    fs::write(&bad, b"wrong input").unwrap();
    assert!(app.analyze_dmx(&bad).is_err());
    let timings = app.get_timings().unwrap();
    assert_eq!(timings.status, TimingStatus::Failed);
    assert_eq!(timings.steps.last().unwrap().status, TimingStatus::Failed);
    assert!(temp
        .path()
        .join("state/operations")
        .join(timings.run_id)
        .join("timings.json")
        .is_file());
    assert!(!app.is_busy());
    assert!(app.analyze_dmx(&fixture("basic-v9.dmx")).is_ok());
}

#[test]
fn project_and_settings_round_trip_through_native_service() {
    let temp = tempfile::tempdir().unwrap();
    let app = Application::new(temp.path().join("state")).unwrap();
    let mut settings = app.settings().unwrap();
    settings.workspace = temp.path().join("work").display().to_string();
    app.save_settings(&settings).unwrap();
    assert_eq!(app.settings().unwrap(), settings);
    let mut doc = app.new_project().unwrap();
    doc.project.name = "test_character".into();
    let path = temp.path().join("project.dmb.json");
    let saved = app.save_project(&path, doc.project).unwrap();
    let opened = app.open_project(&path).unwrap();
    assert_eq!(saved.project, opened.project);
    assert_eq!(saved.path, opened.path);
}

#[test]
fn included_physics_requires_exact_hero_and_reference_hash() {
    let temp = tempfile::tempdir().unwrap();
    let app = Application::with_resources(temp.path().join("state"), temp.path().join("resources"))
        .unwrap();
    assert!(app.included_physics("models/heroes/another.vmdl").is_err());
    let reference = temp.path().join(
        "resources/physics-references/Vindicta-R12-Balanced/cloth_supported_hair_balanced.vmdl_c",
    );
    fs::create_dir_all(reference.parent().unwrap()).unwrap();
    fs::write(reference, b"changed solver").unwrap();
    assert!(app
        .included_physics("models/heroes_staging/hornet_v3/hornet.vmdl_c")
        .is_err());
    assert_eq!(app.get_timings().unwrap().status, TimingStatus::Failed);
}

#[test]
fn intake_is_shared_and_does_not_accept_wrong_types() {
    let temp = tempfile::tempdir().unwrap();
    let image = temp.path().join("image.PNG");
    fs::write(&image, b"content validated later").unwrap();
    assert_eq!(
        validate_asset_path(&image, AssetKind::Icon).unwrap(),
        image.canonicalize().unwrap().to_str().unwrap()
    );
    assert!(validate_asset_path(&image, AssetKind::Physics).is_err());
    assert!(validate_asset_path(temp.path(), AssetKind::TextureDirectory).is_ok());
    assert!(validate_asset_path(temp.path(), AssetKind::Icon).is_err());
}
