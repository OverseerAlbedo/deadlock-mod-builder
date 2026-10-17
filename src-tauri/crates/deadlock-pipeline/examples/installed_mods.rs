//! Development aid: the installed-mods scan with the app's settings, without the UI.
//!
//!   installed_mods SETTINGS.json            (read-only scan of the game's addons folder)
//!   installed_mods SETTINGS.json --update FILE.vpk BACKUPS_DIR
use deadlock_core::CancellationToken;
use deadlock_pipeline::mod_update;
use deadlock_project::Settings;
use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let settings: Settings = serde_json::from_slice(&std::fs::read(args.first().expect("SETTINGS.json")).unwrap()).unwrap();
    let cancel = CancellationToken::new();
    if args.len() == 4 && args[1] == "--update" {
        let report = mod_update::update(&settings, Path::new(&args[2]), Path::new(&args[3]), 3, &cancel, &|m| println!("  {m}")).unwrap();
        println!("{report:#?}");
        return;
    }
    for item in mod_update::scan(&settings, &cancel).unwrap() {
        println!("{:<34} {:>7.1} MB  {:?}", item.file, item.bytes as f64 / 1048576., item.status);
        for model in &item.models {
            println!("    {:<16} {:?}  {}", model.hero.clone().unwrap_or_default(), model.convention, model.path);
        }
        if !item.note.is_empty() {
            println!("    note: {}", item.note);
        }
    }
}
