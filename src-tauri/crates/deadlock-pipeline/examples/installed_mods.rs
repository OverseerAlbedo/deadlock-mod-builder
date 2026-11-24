//! Development aid: the installed-mods scan with the app's settings, without the UI.
//!
//!   installed_mods SETTINGS.json            (read-only scan of the game's addons folder)
//!   installed_mods SETTINGS.json --update FILE.vpk BACKUPS_DIR
//!   installed_mods SETTINGS.json --game     (classify the game archive's own paths:
//!                                             files per hero, paths with several heroes)
//!
//! The scan prints each file's name, its contents summary and whether a VPK
//! of a workspace project's output folder has the same identity; project
//! names and paths are not printed.
use deadlock_components::vpk_contents::{self, HeroTable, Kind};
use deadlock_core::CancellationToken;
use deadlock_pipeline::mod_update;
use deadlock_project::Settings;
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let settings: Settings = serde_json::from_slice(&std::fs::read(args.first().expect("SETTINGS.json")).unwrap()).unwrap();
    let cancel = CancellationToken::new();
    if args.len() == 4 && args[1] == "--update" {
        let report = mod_update::update(&settings, Path::new(&args[2]), Path::new(&args[3]), 3, &cancel, &|m| println!("  {m}")).unwrap();
        println!("{report:#?}");
        return;
    }
    if args.get(1).map(String::as_str) == Some("--game") {
        let game = deadlock_vpk::Vpk::open(&settings.pak).unwrap();
        let table = HeroTable::new().unwrap();
        let mut per_hero: BTreeMap<String, usize> = BTreeMap::new();
        let mut several: BTreeMap<String, usize> = BTreeMap::new();
        let started = Instant::now();
        for path in game.entries.keys() {
            let heroes = table.heroes_of(path);
            for hero in &heroes {
                *per_hero.entry(hero.clone()).or_default() += 1;
            }
            if heroes.len() > 1 {
                let key = heroes.into_iter().collect::<Vec<_>>().join(" + ");
                *several.entry(key).or_default() += 1;
                if several.len() <= 40 {
                    println!("  several: {path}");
                }
            }
        }
        println!("{} paths classified in {:.2} s", game.entries.len(), started.elapsed().as_secs_f64());
        for (hero, n) in per_hero {
            println!("  {hero:<16} {n}");
        }
        for (heroes, n) in several {
            println!("  {heroes:<40} {n}");
        }
        // Top-level folders per hero, to spot a hero word matching unrelated folders.
        let mut folders: BTreeMap<String, usize> = BTreeMap::new();
        for path in game.entries.keys() {
            let top: String = path.split('/').take(if path.starts_with("models/") || path.starts_with("panorama/") { 2 } else { 1 }).collect::<Vec<_>>().join("/");
            for hero in table.heroes_of(path) {
                *folders.entry(format!("{top} {hero}")).or_default() += 1;
            }
        }
        let tops: std::collections::BTreeSet<String> = folders.keys().map(|k| k.split(' ').next().unwrap_or_default().to_owned()).collect();
        for top in tops {
            let rows: Vec<String> = folders.iter().filter(|(k, _)| k.split(' ').next() == Some(&top)).map(|(k, n)| format!("{}={n}", &k[top.len() + 1..])).collect();
            println!("  {top}: {} heroes; {}", rows.len(), rows.iter().take(6).cloned().collect::<Vec<_>>().join(", "));
        }
        // The game definitions: read time and who uses the ability objects.
        let started = Instant::now();
        let defs = deadlock_components::effect_catalog::Definitions::read(&game, &cancel).unwrap();
        let read = started.elapsed().as_secs_f64();
        let users = defs.as_ref().map(|d| d.file_users().unwrap()).unwrap_or_default();
        println!("definitions read: {} in {:.2} s ({:.2} s with file users), {} files named", defs.is_some(), read, started.elapsed().as_secs_f64(), users.len());
        let objects: Vec<&String> = game.entries.keys().filter(|p| p.starts_with("models/abilities/") && p.ends_with(".vmdl_c")).collect();
        let (mut none, mut one, mut many, mut every) = (vec![], vec![], vec![], vec![]);
        for path in &objects {
            match users.get(*path) {
                Some(u) if u.every_hero => every.push(path),
                Some(u) if u.heroes.len() > 1 => many.push((path, u.heroes.iter().cloned().collect::<Vec<_>>().join(", "))),
                Some(u) if u.heroes.len() == 1 => one.push((path, u.heroes.iter().next().cloned().unwrap_or_default())),
                _ => none.push(path),
            }
        }
        println!("ability objects by the definitions: {} (no hero {}, one hero {}, several {}, every hero {})", objects.len(), none.len(), one.len(), many.len(), every.len());
        for (path, heroes) in one.iter().take(5).chain(many.iter().take(5)) {
            println!("    {path}: {heroes}");
        }
        // With the file-name rule for ability objects (the scan's full classification).
        let (mut named, mut named_two, mut still) = (0, vec![], vec![]);
        for path in &none {
            let heroes = table.named_in_file(path);
            match heroes.len() {
                0 => still.push(path),
                1 => named += 1,
                _ => named_two.push((path, heroes.into_iter().collect::<Vec<_>>().join(", "))),
            }
        }
        println!("ability objects after the file-name rule: one hero {}, two or more {}, still none {}", one.len() + named, many.len() + named_two.len(), still.len());
        for (path, heroes) in &named_two {
            println!("    two from the name: {path}: {heroes}");
        }
        for path in still.iter().take(40) {
            println!("    still none: {path}");
        }
        // Every path's heroes in the scan's classification against the rules before the
        // file-name rule (folder/image names, then the definitions): only ability objects may differ.
        let mut changed = vec![];
        for path in game.entries.keys() {
            let c = vpk_contents::classify(&table, &[path.as_str()], Some(&users));
            let now: std::collections::BTreeSet<String> = c.heroes.iter().map(|h| h.name.clone()).collect();
            let mut before = table.heroes_of(path);
            let kind = vpk_contents::kind(&table, path);
            if kind == Kind::CharacterModel {
                before = table.body(path).map(|h| [h.to_owned()].into()).unwrap_or_default();
            } else if before.is_empty() && kind != Kind::ItemIcon && matches!(kind, Kind::AbilityObject | Kind::OtherModel | Kind::Gun | Kind::Effect) {
                if let Some(u) = users.get(path) {
                    if !u.every_hero {
                        before = u.heroes.clone();
                    }
                }
            }
            if now != before {
                changed.push((path.clone(), kind));
            }
        }
        let outside: Vec<&(String, Kind)> = changed.iter().filter(|(p, _)| !p.starts_with("models/abilities/")).collect();
        println!("paths whose heroes the file-name rule changed: {} (outside models/abilities/: {})", changed.len(), outside.len());
        for (path, _) in outside.iter().take(10) {
            println!("    outside: {path}");
        }
        return;
    }
    let started = Instant::now();
    let scan = mod_update::scan(&settings, &cancel).unwrap();
    let scanned = started.elapsed().as_secs_f64();
    let outputs = if settings.workspace.is_empty() { vec![] } else { mod_update::own_outputs(Path::new(&settings.workspace), &cancel).unwrap() };
    println!("scan {:.2} s, own outputs {:.2} s ({} outputs)", scanned, started.elapsed().as_secs_f64() - scanned, outputs.len());
    if let Some(at) = args.iter().position(|a| a == "--json") {
        // For the page harness: the scan as the page receives it (no project data).
        std::fs::write(&args[at + 1], serde_json::to_vec_pretty(&scan).unwrap()).unwrap();
    }
    let mut owners: BTreeMap<&str, std::collections::BTreeSet<&str>> = BTreeMap::new();
    for output in &outputs {
        owners.entry(output.identity.as_str()).or_default().insert(output.project_id.as_str());
    }
    println!("output identities under more than one project: {}", owners.values().filter(|p| p.len() > 1).count());
    for item in &scan {
        let made = outputs.iter().any(|o| o.identity == item.identity && !item.identity.is_empty());
        let heroes: Vec<String> = item.contents.heroes.iter().map(|h| format!("{} ({})", h.name, h.files)).collect();
        let kinds: Vec<String> = item.contents.kinds.iter().map(|(k, n)| format!("{n} {}", name(*k))).collect();
        println!("{:<22} {:>7.1} MB  {:?}  made here: {}", item.file, item.bytes as f64 / 1048576., item.status, if made { "yes" } else { "no" });
        if let Some(reason) = &item.unreadable {
            println!("    unreadable: {reason}");
            continue;
        }
        println!("    heroes: {}{}", if heroes.is_empty() { "none".into() } else { heroes.join(", ") }, if item.contents.every_hero > 0 { format!("; every hero: {}", item.contents.every_hero) } else { String::new() });
        println!("    {} files: {}", item.contents.files, kinds.join(", "));
        for object in &item.contents.ability_objects {
            println!("    ability object: {} {:?}{}", object.path, object.heroes, if object.every_hero { " (every hero)" } else { "" });
        }
        for shared in &item.shared {
            println!("    shares {} files with {}: {}", shared.count, shared.file, shared.paths.join(", "));
        }
        if !item.note.is_empty() {
            println!("    note: {}", item.note);
        }
    }
}

fn name(kind: Kind) -> String {
    serde_json::to_value(kind).unwrap().as_str().unwrap().replace('_', " ")
}
