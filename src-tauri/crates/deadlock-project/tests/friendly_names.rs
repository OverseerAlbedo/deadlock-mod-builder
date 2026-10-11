use deadlock_project::{asset_paths, Project};
use std::fs;

#[test]
fn names_with_capitals_and_spaces_round_trip_without_renaming() {
    let temp = tempfile::tempdir().unwrap();
    for name in [
        "vin",
        "VIN",
        "Vindicta Balanced",
        "Vindicta V26 R12",
        "My_Skin 2",
    ] {
        let mut project = Project::default();
        project.name = name.into();
        project.original_model = "models/heroes_staging/hornet_v3/hornet.vmdl_c".into();
        let id = project.id.clone();
        let path = temp.path().join(format!("{name}.dmb.json"));
        project.validate_action().unwrap();
        project.save(&path).unwrap();
        let loaded = Project::load(&path).unwrap();
        assert_eq!(loaded.name, name);
        assert_eq!(loaded.id, id);
        assert_eq!(loaded.original_model, project.original_model);
        assert!(asset_paths::relative_asset("../escape").is_err());
    }
}

#[test]
fn unsafe_or_empty_names_still_fail_without_overwriting_saved_project() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("project.dmb.json");
    let mut project = Project::default();
    project.name = "Vindicta Balanced".into();
    project.save(&path).unwrap();
    let original = fs::read(&path).unwrap();
    for name in [
        "",
        "   ",
        "../skin",
        "C:\\skin",
        "bad\nname",
        "Bad/Name",
        "A*B",
    ] {
        project.name = name.into();
        assert!(project.save(&path).is_err(), "{name:?}");
        assert_eq!(fs::read(&path).unwrap(), original);
    }
    project.name = "A".repeat(42);
    assert!(project.validate_draft().is_err());
}

#[cfg(windows)]
#[test]
fn extended_and_ordinary_drive_paths_resolve_to_the_same_asset() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("Source With Spaces.dmx");
    fs::write(&file, b"path fixture").unwrap();
    let extended = file.canonicalize().unwrap();
    assert!(extended.to_str().unwrap().starts_with(r"\\?\"));
    assert_eq!(
        asset_paths::resolve(&file).unwrap(),
        asset_paths::resolve(&extended).unwrap()
    );
    let root = extended.ancestors().last().unwrap();
    assert!(asset_paths::resolve(root).unwrap().is_absolute());
    let absent = extended
        .parent()
        .unwrap()
        .join("Missing Folder")
        .join("project.dmb.json");
    let resolved = asset_paths::resolve(&absent).unwrap();
    assert!(resolved.ends_with("Missing Folder/project.dmb.json"));
}
