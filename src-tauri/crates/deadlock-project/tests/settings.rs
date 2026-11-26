//! Settings → Appearance: Language, Default fit for new models and Load 3D previews automatically.
use deadlock_project::{Settings, DEFAULT_FIT_MODES};
use std::fs;

#[test]
fn a_settings_file_without_the_new_fields_means_preserve_source_proportions_and_no_automatic_previews() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    // A settings file from 0.13.0: none of the new fields.
    fs::write(&path, r#"{"workspace":"C:/Work","theme":"violet","keep_backups":2}"#).unwrap();
    let settings = Settings::load(&path).unwrap();
    assert_eq!(settings.default_fit_mode, "preserve_proportions");
    assert_eq!(settings.default_fit(), "preserve_proportions");
    assert!(!settings.auto_previews);
    // The other fields are read as before.
    assert_eq!((settings.workspace.as_str(), settings.theme.as_str(), settings.keep_backups), ("C:/Work", "violet", 2));
    let fresh = Settings::default();
    assert_eq!((fresh.default_fit_mode.as_str(), fresh.auto_previews), ("preserve_proportions", false));
}

#[test]
fn both_settings_survive_a_save_and_a_load() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    let settings = Settings { default_fit_mode: "match_target".into(), auto_previews: true, ..Default::default() };
    settings.save(&path).unwrap();
    let saved: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(saved["default_fit_mode"], "match_target");
    assert_eq!(saved["auto_previews"], true);
    let loaded = Settings::load(&path).unwrap();
    assert_eq!(loaded, settings);
    assert_eq!(loaded.default_fit(), "match_target");
}

#[test]
fn an_unknown_default_fit_loads_and_falls_back_to_preserve_source_proportions() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    fs::write(&path, r#"{"default_fit_mode":"something_else","auto_previews":true}"#).unwrap();
    let settings = Settings::load(&path).unwrap();
    assert_eq!(settings.default_fit(), "preserve_proportions");
    assert!(settings.auto_previews);
    assert_eq!(DEFAULT_FIT_MODES, ["preserve_proportions", "match_target"]);
}

#[test]
fn a_settings_file_without_a_language_means_not_chosen_yet() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    // A settings file written before the language field existed.
    fs::write(&path, r#"{"workspace":"C:/Work","theme":"mint","default_fit_mode":"match_target","auto_previews":true}"#).unwrap();
    let settings = Settings::load(&path).unwrap();
    assert_eq!(settings.language, "");
    assert_eq!((settings.workspace.as_str(), settings.theme.as_str(), settings.default_fit(), settings.auto_previews), ("C:/Work", "mint", "match_target", true));
    assert_eq!(Settings::default().language, "");
}

#[test]
fn the_language_survives_a_save_and_a_load_and_an_unknown_one_is_kept_as_written() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    let settings = Settings { language: "zh-CN".into(), ..Default::default() };
    settings.save(&path).unwrap();
    let saved: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(saved["language"], "zh-CN");
    assert_eq!(Settings::load(&path).unwrap(), settings);
    // The page decides what an unknown tag means (English); loading never fails on it.
    fs::write(&path, r#"{"language":"xx-YY"}"#).unwrap();
    assert_eq!(Settings::load(&path).unwrap().language, "xx-YY");
}
