//! Renamed shop items through the real asset-only build: the package holds the
//! whole names file of every language with tokens, the manifest check binds it
//! to the rows, and install refuses changed rows and a changed game list. On a
//! made-up names folder beside the hero fixture.
use deadlock_components::item_names;
use deadlock_core::{
    process::{Runner, RunnerOptions},
    safety, CancellationToken, TimingRecorder,
};
use deadlock_pipeline::{build::BuildService, inspect::ModelPipeline};
use deadlock_project::{item_names::ItemNameRow, Project, Settings};
use deadlock_vpk::{write_vpk, Vpk};
use std::{fs, path::Path};

const ENGLISH: &str = "resource/localization/citadel_gc_mod_names/citadel_gc_mod_names_english.txt";
const GERMAN: &str = "resource/localization/citadel_gc_mod_names/citadel_gc_mod_names_german.txt";

/// A made-up names file: an item with a search token and a buff name, and a plain item.
fn names(language: &str, rite: &str, plain: &str) -> String {
    format!(
        "\u{feff}\"lang\"\r\n{{\r\n\t\"Language\"\t\t\"{language}\"\r\n\t\"Tokens\"\r\n\t{{\r\n\t\t\"upgrade_sample_rite\"\t\t\"{rite}\"\r\n\t\t\"upgrade_sample_rite_search\"\t\"{rite} SR\"\r\n\t\t\"modifier_sample_regen\"\t\"{rite}\"\r\n\t\t\"upgrade_plain_item\"\t\"{plain}\"\r\n\t}}\r\n}}\r\n"
    )
}
struct World {
    temp: tempfile::TempDir,
    settings: Settings,
    token: CancellationToken,
    runner: Runner,
}
fn world() -> World {
    let temp = tempfile::tempdir().unwrap();
    let token = CancellationToken::new();
    let tree = temp.path().join("game-source");
    let base = Vpk::open(Path::new(env!("CARGO_MANIFEST_DIR")).join("../deadlock-components/tests/fixtures/components_dir.vpk")).unwrap();
    let mut paths = vec![];
    for entry in base.entries.values() {
        safety::atomic_write(&tree.join(&entry.path), base.read(&entry.path).unwrap()).unwrap();
        paths.push(entry.path.clone());
    }
    // The writer refuses the game's own name: written as another number and renamed.
    let pak = temp.path().join("deadlock/game/citadel/pak01_dir.vpk");
    fs::create_dir_all(pak.parent().unwrap()).unwrap();
    let spare = pak.with_file_name("pak42_dir.vpk");
    write_vpk(&tree, &paths, &spare, None, 2, &mut || Ok(())).unwrap();
    fs::rename(&spare, &pak).unwrap();
    let folder = item_names::folder(&pak);
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("citadel_gc_mod_names_english.txt"), names("English", "Sample Rite", "Plain Item")).unwrap();
    fs::write(folder.join("citadel_gc_mod_names_german.txt"), names("German", "Probe Ritus", "Schlichtes Ding")).unwrap();
    fs::write(folder.join("citadel_gc_mod_names_danish.txt"), "\u{feff}\"lang\"\r\n{\r\n\t\"Tokens\"\r\n\t{\r\n\t}\r\n}\r\n").unwrap();
    let mut settings = Settings::default();
    settings.workspace = temp.path().join("workspace").display().to_string();
    settings.deadlock = temp.path().join("deadlock").display().to_string();
    settings.pak = pak.display().to_string();
    fs::create_dir_all(Path::new(&settings.deadlock).join("game/citadel/addons")).unwrap();
    let runner = Runner::new(temp.path().join("logs"), token.clone(), RunnerOptions::default()).unwrap();
    World { temp, settings, token, runner }
}
fn row(item: &str, name: &str) -> ItemNameRow {
    ItemNameRow { item: item.into(), enabled: true, name: name.into() }
}
fn project() -> Project {
    let mut project = Project::default();
    project.original_model = "models/heroes_staging/hornet_v3/hornet.vmdl".into();
    project.install = false;
    project.customization.enabled = Some(true);
    project.customization.model_enabled = Some(false);
    project.customization.item_names = Some(vec![row("upgrade_sample_rite", "New Rite"), row("upgrade_plain_item", "Plainer")]);
    project
}

#[test]
fn renamed_items_build_as_whole_names_files_and_install_only_while_current() {
    let w = world();
    let clock = TimingRecorder::new("item names build");
    let service = BuildService {
        model: ModelPipeline { settings: &w.settings, runner: &w.runner, cancel: &w.token, clock: &clock, log: &|_| {}, reader_root: w.temp.path() },
    };
    let mut project = project();
    let output = service.build(&mut project, false, false).unwrap().vpk.unwrap();
    let built = Vpk::open(&output).unwrap();
    assert_eq!(built.entries.keys().collect::<Vec<_>>(), [ENGLISH, GERMAN], "the two languages with tokens, nothing else");
    let english = String::from_utf8(built.read(ENGLISH).unwrap()).unwrap();
    // The app's switches: the buff name takes the new name too.
    assert_eq!(english, names("English", "Sample Rite", "Plain Item").replace("\"Sample Rite\"", "\"New Rite\"").replace("\"Sample Rite SR\"", "\"Sample Rite SR New Rite\"").replace("\"Plain Item\"", "\"Plainer\""));
    drop(built); // an open archive denies the rebuild below its write
    let manifest = service.read_manifest(&project).unwrap().customization.unwrap();
    assert_eq!(manifest.item_names.files.len(), 2);
    assert_eq!(manifest.item_names.switches, item_names::SWITCHES);
    assert!(english.contains("\"modifier_sample_regen\"\t\"New Rite\""));
    assert!(manifest.original_resources.is_empty(), "loose text files are not game archive files");

    // Install, then another name stops a reinstall.
    service.install(&mut project, &output, false).unwrap();
    let installed = Path::new(&w.settings.deadlock).join("game/citadel/addons").join(output.file_name().unwrap());
    assert_eq!(fs::read(&installed).unwrap(), fs::read(&output).unwrap());
    let mut renamed = project.clone();
    renamed.customization.item_names = Some(vec![row("upgrade_sample_rite", "Newer Rite"), row("upgrade_plain_item", "Plainer")]);
    assert!(service.install(&mut renamed, &output, true).is_err());
    // A game update that changed a names file: the install is refused, saying so.
    let folder = item_names::folder(Path::new(&w.settings.pak));
    fs::write(folder.join("citadel_gc_mod_names_german.txt"), names("German", "Probe Ritus", "Neues Ding")).unwrap();
    let error = service.install(&mut project, &output, true).unwrap_err().to_string();
    assert!(error.contains(item_names::GAME_NAMES_CHANGED), "{error}");
    // Built again from the current files, it installs.
    let output = service.build(&mut project, true, false).unwrap().vpk.unwrap();
    service.install(&mut project, &output, true).unwrap();
    // A row for an item the game does not have stops the build, naming it.
    project.customization.item_names = Some(vec![row("upgrade_gone", "Gone")]);
    let error = service.build(&mut project, true, false).unwrap_err().to_string();
    assert!(error.contains(item_names::UNKNOWN_ITEM) && error.contains("upgrade_gone"), "{error}");
}
