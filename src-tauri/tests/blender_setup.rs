use deadlock_desktop::app::Application;
use deadlock_project::Project;
use std::{fs, path::Path};

#[test]
fn missing_blender_explains_the_reason_before_inspection_or_preview() {
    let temp = tempfile::tempdir().unwrap();
    let app = Application::new(temp.path().join("state")).unwrap();
    let source = temp.path().join("unchanged.blend");
    fs::write(
        &source,
        b"source must not be opened before checking Blender",
    )
    .unwrap();
    let original = fs::read(&source).unwrap();
    let mut project = Project::default();
    project.original_model = "models/heroes_staging/hornet_v3/hornet.vmdl".into();
    for (path, title, detail) in [
        (
            String::new(),
            "Blender is not configured",
            "No Blender path has been saved".to_owned(),
        ),
        (
            temp.path()
                .join("Other PC 雪/blender.exe")
                .display()
                .to_string(),
            "Blender executable not found",
            "Other PC 雪".to_owned(),
        ),
    ] {
        let mut settings = app.settings().unwrap();
        settings.blender = path;
        app.save_settings(&settings).unwrap();
        let inspected =
            app.preparation_inspect(project.clone(), source.display().to_string(), [0.; 3]);
        let previewed = app.preview_source(&source);
        for error in [inspected.unwrap_err(), previewed.unwrap_err()] {
            assert_eq!(error.kind, "validation");
            assert_eq!(error.message, title);
            assert!(error.details.contains(&detail), "{}", error.details);
            assert!(error
                .details
                .contains("Settings → Advanced configuration → Blender executable"));
            assert!(error.details.contains("Save settings"));
        }
        assert_eq!(app.settings().unwrap(), settings);
        assert!(!app.is_busy());
        assert_eq!(fs::read(&source).unwrap(), original);
    }
    assert!(app
        .log_entries()
        .iter()
        .all(|entry| !entry.message.contains("Full tool output:")));
    // GLB preview is independent of Blender. A malformed GLB reports its own
    // input error, even with no Blender path configured.
    let glb = temp.path().join("bad.glb");
    fs::write(&glb, b"not a GLB").unwrap();
    assert!(!app
        .preview_source(&glb)
        .unwrap_err()
        .message
        .contains("Blender"));
}

#[test]
fn blender_path_check_distinguishes_blank_directory_and_existing_file() {
    use deadlock_preparation::{blender::check_executable, Error};
    let temp = tempfile::tempdir().unwrap();
    for value in ["", "   "] {
        assert!(matches!(
            check_executable(Path::new(value)),
            Err(Error::BlenderNotConfigured)
        ));
    }
    assert!(matches!(
        check_executable(temp.path()),
        Err(Error::BlenderNotFound(_))
    ));
    let file = temp.path().join("blender.exe");
    fs::write(&file, b"version validation is a separate check").unwrap();
    check_executable(&file).unwrap();
}

#[cfg(unix)]
fn version_fixture(directory: &Path, version: &str) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    fs::create_dir_all(directory).unwrap();
    let executable = directory.join("blender");
    fs::write(&executable, format!("#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then\n  printf 'Blender {version}\\n'\nelse\n  echo 'regression fixture reached saved Blender' >&2\n  exit 29\nfi\n")).unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
    executable
}

#[cfg(unix)]
#[test]
fn successful_validation_saves_only_blender_and_preview_uses_it_without_restart() {
    let temp = tempfile::tempdir().unwrap();
    let state = temp.path().join("state");
    let app = Application::new(state.clone()).unwrap();
    let mut saved = app.settings().unwrap();
    saved.workspace = temp.path().join("keep workspace").display().to_string();
    saved.resourcecompiler = "keep material compiler".into();
    app.save_settings(&saved).unwrap();
    let executable = version_fixture(&temp.path().join("Blender 5.2 雪"), "5.2.2");
    let result = app.validate_blender(Some(executable.clone())).unwrap();
    assert!(result.launch_verified);
    assert!(!result.conversion_certified);
    saved.blender = result.path;
    assert_eq!(app.settings().unwrap(), saved);
    assert_eq!(Application::new(state).unwrap().settings().unwrap(), saved);
    // A subsequent validation with no argument uses the saved executable.
    assert_eq!(app.validate_blender(None).unwrap().path, saved.blender);
    let source = temp.path().join("unchanged.fbx");
    fs::write(&source, b"settings handoff fixture").unwrap();
    let error = app.preview_source(&source).unwrap_err();
    assert!(error.details.contains("regression fixture reached saved Blender"), "{error:?}");
    assert_eq!(fs::read(&source).unwrap(), b"settings handoff fixture");
    let mut project = Project::default();
    project.original_model = "models/heroes_staging/hornet_v3/hornet.vmdl".into();
    let error = app.preparation_inspect(project, source.display().to_string(), [0.; 3]).unwrap_err();
    assert_ne!(error.message, "Blender is not configured");
    assert_ne!(error.message, "Blender executable not found");
    assert_eq!(app.settings().unwrap(), saved);
    assert!(!app.is_busy());
}

#[cfg(unix)]
#[test]
fn failed_blender_validation_preserves_the_previous_settings() {
    let temp = tempfile::tempdir().unwrap();
    let state = temp.path().join("state");
    let app = Application::new(state.clone()).unwrap();
    let mut saved = app.settings().unwrap();
    saved.blender = "previous saved Blender".into();
    saved.workspace = "unchanged workspace".into();
    app.save_settings(&saved).unwrap();
    let before = fs::read(state.join("settings.json")).unwrap();
    let unsupported = version_fixture(&temp.path().join("unsupported"), "5.3.0");
    for path in [unsupported, temp.path().join("missing blender")] {
        assert!(app.validate_blender(Some(path)).is_err());
        assert_eq!(fs::read(state.join("settings.json")).unwrap(), before);
        assert_eq!(app.settings().unwrap(), saved);
        assert!(!app.is_busy());
    }
}
