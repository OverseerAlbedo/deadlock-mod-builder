use deadlock_desktop::app::Application;
use deadlock_project::Project;
use std::fs;

const MODEL: &str = "models/heroes_staging/hornet_v3/hornet.vmdl_c";

#[test]
fn supplied_r12_project_accepts_friendly_name_and_retains_cloth_through_save_and_hero_selection() {
    let temp = tempfile::tempdir().unwrap();
    let app = Application::new(temp.path().join("state")).unwrap();
    let input = temp.path().join("original/project.dmb.json");
    fs::create_dir_all(input.parent().unwrap()).unwrap();
    fs::write(&input, include_bytes!("fixtures/r12-balanced-project.json")).unwrap();
    let mut project = Project::load(&input).unwrap();
    assert_eq!(project.name, "vin");
    project.validate_action().unwrap();
    let original_id = project.id.clone();
    let mappings = project.meshes.clone();
    let material_count = project.materials.len();
    project.name = "Vindicta Balanced R12".into();
    let path = temp
        .path()
        .join("My Projects/Vindicta Balanced R12.dmb.json");
    let saved = app.save_project(&path, project).unwrap();
    let opened = app.open_project(&path).unwrap();
    assert_eq!(opened.project.name, "Vindicta Balanced R12");
    assert_eq!(opened.project.id, original_id);
    assert_eq!(opened.project.meshes, mappings);
    assert_eq!(opened.project.materials.len(), material_count);
    assert_eq!(opened.project.customization, saved.project.customization);
    let physics = serde_json::to_value(&opened.project.customization).unwrap();
    assert_eq!(physics["physics"]["enabled"], true);
    assert_eq!(physics["physics"]["hero_path"], MODEL);

    // Hero selection only reads the archive index; these bytes are a labeled
    // non-playable fixture, never treated as a compiled game model.
    let data = temp.path().join("game-fixture");
    fs::create_dir_all(data.join(MODEL).parent().unwrap()).unwrap();
    fs::write(data.join(MODEL), b"ARCHIVE INDEX FIXTURE, NOT PLAYABLE").unwrap();
    let archive = temp.path().join("fixture_dir.vpk");
    deadlock_vpk::write_vpk(&data, &[MODEL.into()], &archive, None, 2, &mut || Ok(())).unwrap();
    let mut settings = app.settings().unwrap();
    settings.pak = archive.to_string_lossy().into_owned();
    app.save_settings(&settings).unwrap();
    let selected = app.select_hero(opened.project, MODEL).unwrap();
    assert_eq!(selected.name, "Vindicta Balanced R12");
    assert_eq!(selected.customization, saved.project.customization);
    assert_eq!(selected.meshes, mappings);
}
