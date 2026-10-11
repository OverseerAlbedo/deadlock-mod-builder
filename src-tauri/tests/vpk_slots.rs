use deadlock_core::TimingStatus;
use deadlock_desktop::app::Application;
use deadlock_project::Project;
use std::fs;

fn configured(
    temp: &tempfile::TempDir,
) -> (Application, Project, std::path::PathBuf, std::path::PathBuf) {
    let app = Application::new(temp.path().join("state")).unwrap();
    let mut settings = app.settings().unwrap();
    settings.deadlock = temp.path().join("game install").to_str().unwrap().into();
    settings.workspace = temp.path().join("work").to_str().unwrap().into();
    app.save_settings(&settings).unwrap();
    let project = Project::default();
    let addons = std::path::Path::new(&settings.deadlock).join("game/citadel/addons");
    let output = std::path::Path::new(&settings.workspace)
        .join(&project.id)
        .join("output");
    (app, project, addons, output)
}

#[test]
fn scans_both_folders_chunks_and_case_without_reading_archive_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let (app, mut project, addons, output) = configured(&temp);
    fs::create_dir_all(&addons).unwrap();
    fs::create_dir_all(&output).unwrap();
    for name in [
        "PAK02_DIR.VPK",
        "pak3_004.vpk",
        "pak01.vpk",
        "pak999_dir.vpk",
        "pak06_00.vpk",
        "pak07_dir.vpk.bak",
    ] {
        fs::write(addons.join(name), b"intentionally not an archive").unwrap();
    }
    fs::write(output.join("pak04.vpk"), b"previous output").unwrap();
    // Legacy filename-only occupancy includes directory entries of matching name.
    fs::create_dir(output.join("pak05_099.vpk")).unwrap();
    project.vpk_number = 2;
    let choice = app.choose_free_vpk_slot(project.clone()).unwrap();
    assert_eq!(choice.slot, 6);
    assert_eq!(choice.filename, "pak06_dir.vpk");
    assert_eq!(choice.occupied_slots, [1, 2, 3, 4, 5]);
    assert_eq!(
        choice.scanned_folders,
        [addons.to_str().unwrap(), output.to_str().unwrap()]
    );
    assert_eq!(
        project.vpk_number, 2,
        "an explicit choice does not mutate the supplied draft"
    );
    project.vpk_number = 29;
    assert_eq!(
        app.choose_free_vpk_slot(project).unwrap().slot,
        29,
        "prefer the configured free slot"
    );
    assert_eq!(
        fs::read(output.join("pak04.vpk")).unwrap(),
        b"previous output"
    );
    assert!(!app.is_busy());
}

#[test]
fn missing_folders_are_not_created_and_slot_one_is_never_selected() {
    let temp = tempfile::tempdir().unwrap();
    let (app, mut project, addons, output) = configured(&temp);
    project.vpk_number = 1;
    let choice = app.choose_free_vpk_slot(project).unwrap();
    assert_eq!(choice.slot, 2);
    assert!(choice.occupied_slots.is_empty());
    assert!(!addons.exists());
    assert!(!output.exists());
    assert_eq!(app.get_timings().unwrap().status, TimingStatus::Succeeded);
}

#[test]
fn full_scan_and_unreadable_folder_are_reported_without_replacing_a_built_slot() {
    let temp = tempfile::tempdir().unwrap();
    let (app, mut project, addons, output) = configured(&temp);
    fs::create_dir_all(&addons).unwrap();
    for slot in 2..=99 {
        fs::write(addons.join(format!("pak{slot:02}_000.vpk")), b"chunk").unwrap();
    }
    project.vpk_number = 18;
    let scan = app.get_vpk_slots(project.clone()).unwrap();
    assert!(scan.configured_slot_occupied);
    assert_eq!(scan.configured_slot, 18);
    assert_eq!(scan.available_slot, None);
    assert!(app
        .choose_free_vpk_slot(project.clone())
        .unwrap_err()
        .to_string()
        .contains("02–99"));
    assert_eq!(app.get_timings().unwrap().status, TimingStatus::Failed);
    fs::remove_dir_all(&addons).unwrap();
    fs::write(&addons, b"a file cannot be scanned as a folder").unwrap();
    assert!(app
        .get_vpk_slots(project)
        .unwrap_err()
        .to_string()
        .contains("Could not check VPK slots"));
    assert!(!output.exists());
    assert!(!app.is_busy());
    assert_eq!(app.get_timings().unwrap().status, TimingStatus::Failed);
}

#[test]
fn no_game_configuration_does_not_claim_global_availability() {
    let temp = tempfile::tempdir().unwrap();
    let app = Application::new(temp.path().join("state")).unwrap();
    assert!(app
        .choose_free_vpk_slot(Project::default())
        .unwrap_err()
        .to_string()
        .contains("Select Deadlock"));
    assert!(!app.is_busy());
}
