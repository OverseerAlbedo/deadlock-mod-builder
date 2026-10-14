use deadlock_core::TimingStatus;
use deadlock_desktop::{app::Application, setup::AcquireAuth};
use deadlock_project::{Project, Settings};
use deadlock_tools::manual_setup::Options;
use std::{fs, path::Path};

#[test]
fn manual_preview_never_creates_sources_or_runs_downloader_and_times_the_action() {
    let temp = tempfile::tempdir().unwrap();
    let app = Application::new(temp.path().join("state")).unwrap();
    let options = Options {
        dd: temp.path().join("not-installed'DepotDownloader.exe"),
        toolroot: temp.path().join("tools"),
        manifests: temp.path().join("manifests"),
    };
    let report = app.setup_manual_preview(options.clone()).unwrap();
    assert!(report
        .commands
        .contains("not-installed''DepotDownloader.exe"));
    assert!(!report.compatible);
    assert!(!report.downloaded);
    assert!(report.plan_digest.is_none());
    assert!(!options.dd.exists());
    assert!(!options.toolroot.exists());
    assert!(!options.manifests.exists());
    let timing = app.get_timings().unwrap();
    assert_eq!(timing.status, TimingStatus::Succeeded);
    assert_eq!(timing.steps.len(), 1);
    assert!(!app.is_busy());
    assert!(app
        .log_text()
        .unwrap()
        .contains("Step time: Generate literal setup commands"));
}
#[test]
fn toolset_selection_preserves_other_settings_and_requires_explicit_persistence() {
    let temp = tempfile::tempdir().unwrap();
    let app = Application::new(temp.path().join("state")).unwrap();
    let mut settings = Settings::default();
    settings.blender = "Blender remains unchanged".into();
    settings.resourcecompiler = "material compiler remains unchanged".into();
    app.save_settings(&settings).unwrap();
    let root = temp.path().join("工具");
    fs::create_dir_all(root.join("game/bin/win64")).unwrap();
    fs::create_dir_all(root.join("game/csgo")).unwrap();
    fs::write(
        root.join("game/bin/win64/resourcecompiler.exe"),
        b"presence only",
    )
    .unwrap();
    fs::write(root.join("game/csgo/gameinfo.gi"), b"gameinfo").unwrap();
    let next = app.setup_use_toolset(settings.clone(), &root).unwrap();
    assert_eq!(next.blender, settings.blender);
    assert_eq!(next.resourcecompiler, settings.resourcecompiler);
    assert_eq!(next.model_backend, "cswin");
    assert_eq!(next.model_format, "auto");
    // Saved settings use the resolved long spelling; CI temp folders can be
    // 8.3 short names (C:\Users\RUNNER~1\...), so compare resolved paths.
    assert_eq!(
        Path::new(&next.cswin),
        deadlock_project::asset_paths::resolve(&root).unwrap()
    );
    assert_eq!(app.settings().unwrap(), settings);
    app.save_settings(&next).unwrap();
    assert_eq!(app.settings().unwrap(), next);
}
#[test]
fn setup_status_reports_model_files_without_certifying_or_running_a_compiler() {
    let temp = tempfile::tempdir().unwrap();
    let app = Application::new(temp.path().join("state")).unwrap();
    assert!(!app.setup_status(&Settings::default()).model_files_found);
    let root = temp.path().join("tools");
    fs::create_dir_all(root.join("game/bin/win64")).unwrap();
    fs::create_dir_all(root.join("game/csgo")).unwrap();
    fs::write(
        root.join("game/bin/win64/resourcecompiler.exe"),
        b"not executable",
    )
    .unwrap();
    fs::write(root.join("game/csgo/gameinfo.gi"), b"gameinfo").unwrap();
    let settings = app.setup_use_toolset(Settings::default(), &root).unwrap();
    let previous_operation = app.get_timings().map(|report| report.run_id);
    let status = app.setup_status(&settings);
    assert!(status.model_files_found);
    assert!(status.model_status.contains("Test it once with your hero"));
    assert_eq!(
        app.get_timings().map(|report| report.run_id),
        previous_operation
    );
    fs::remove_file(root.join("game/csgo/gameinfo.gi")).unwrap();
    assert!(!app.setup_status(&settings).model_files_found);
}
#[test]
fn credential_shape_cannot_accept_password_or_guard_code() {
    for value in [
        serde_json::json!({"mode":"qr","password":"secret"}),
        serde_json::json!({"mode":"username","username":"player","steam_guard":"123"}),
    ] {
        assert!(serde_json::from_value::<AcquireAuth>(value).is_err());
    }
    assert!(serde_json::from_value::<AcquireAuth>(
        serde_json::json!({"mode":"username","username":"player"})
    )
    .is_ok());
}
#[cfg(not(windows))]
#[test]
fn native_validation_reports_unsupported_host_without_copying_or_certifying() {
    let temp = tempfile::tempdir().unwrap();
    let app = Application::new(temp.path().join("state")).unwrap();
    let options = Options {
        dd: temp.path().join("dd"),
        toolroot: temp.path().join("tools"),
        manifests: temp.path().join("manifests"),
    };
    let error = app
        .setup_validate_existing(Project::default(), options.clone())
        .unwrap_err();
    assert!(error.message.contains("requires Windows"));
    assert!(!options.toolroot.exists());
    assert_eq!(app.get_timings().unwrap().status, TimingStatus::Failed);
    assert!(!app.is_busy());
    assert!(app
        .log_text()
        .unwrap()
        .contains("Step time: Check native validation requirements"));
}
#[cfg(unix)]
#[test]
fn manual_paths_reject_lossy_unicode_before_command_generation() {
    use std::os::unix::ffi::OsStringExt;
    let temp = tempfile::tempdir().unwrap();
    let app = Application::new(temp.path().join("state")).unwrap();
    let mut options = app.setup_defaults().unwrap().options;
    options.dd = std::ffi::OsString::from_vec(vec![b'd', 0xff, b'd']).into();
    assert!(app
        .setup_manual_preview(options)
        .unwrap_err()
        .message
        .contains("without loss"));
}
