//! Extras → Effects commands through the application layer the desktop
//! commands call: `discover_extras`, `effect_trees` (built once, then kept),
//! `effect_preview` (never behind the operation lease) and `hero_scene`.
//!
//! `the_effect_commands_on_the_real_game` is ignored and needs `DMB_PAK` (the
//! game's `pak01_dir.vpk`); with `DMB_SOURCE2VIEWER` (the Source 2 Viewer
//! CLI) it also loads the hero's 3D view and checks that previews answer
//! while that operation holds the lease. Nothing has a default path.
//!
//! The JSON the commands return is checked against the interfaces in
//! `src/extras/effectTypes.ts`: every field an interface declares without `?`
//! is present, and every field the JSON has is declared.
use deadlock_components::effect_colors::{PartSettings, TreeEdit};
use deadlock_components::effect_trees::PickRequest;
use deadlock_core::safety;
use deadlock_desktop::{app::Application, catalogs};
use deadlock_project::{Project, Settings};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

const GEIST: &str = "models/heroes_wip/geist/geist.vmdl";
const BOMB: &str = "particles/abilities/ghost/ghost_blood_bomb_explode.vpcf_c";
const NOT_LOADED_YET: &str = "Load this character’s assets first";

/// The interfaces of `effectTypes.ts`: name → (field → optional).
fn interfaces() -> BTreeMap<String, BTreeMap<String, bool>> {
    let text = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/extras/effectTypes.ts")).unwrap();
    let mut out = BTreeMap::new();
    let mut current: Option<(String, BTreeMap<String, bool>)> = None;
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("export interface ") {
            let name = rest.split([' ', '{']).next().unwrap().to_owned();
            current = Some((name, BTreeMap::new()));
            continue;
        }
        if line.starts_with('}') {
            if let Some((name, fields)) = current.take() {
                out.insert(name, fields);
            }
            continue;
        }
        if let Some((_, fields)) = current.as_mut() {
            if line.starts_with("/**") || line.starts_with('*') || line.starts_with("//") {
                continue;
            }
            let Some((key, _)) = line.split_once(':') else { continue };
            let key = key.trim();
            let (key, optional) = match key.strip_suffix('?') {
                Some(k) => (k, true),
                None => (key, false),
            };
            if !key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                fields.insert(key.to_owned(), optional);
            }
        }
    }
    out
}

/// Checks `value` against interface `name`, and the nested objects the page reads.
fn conforms(types: &BTreeMap<String, BTreeMap<String, bool>>, name: &str, value: &Value, at: &str, problems: &mut BTreeSet<String>) {
    let Some(fields) = types.get(name) else { panic!("effectTypes.ts has no interface {name}") };
    let Some(object) = value.as_object() else {
        problems.insert(format!("{at}: {name} is not an object"));
        return;
    };
    for (key, optional) in fields {
        if !optional && !object.contains_key(key) {
            problems.insert(format!("{name}.{key} is missing (at {at})"));
        }
    }
    for key in object.keys() {
        if !fields.contains_key(key) {
            problems.insert(format!("{name}.{key} is sent but not declared (at {at})"));
        }
    }
    // Nested objects: field → interface (arrays of them too).
    let nested: &[(&str, &str)] = match name {
        "EffectTrees" => &[("groups", "EffectGroup"), ("trees", "EffectTree"), ("counts", "EffectCounts")],
        "EffectTree" => &[("parts", "EffectPart"), ("palette", "EffectSwatch"), ("main", "EffectMain"), ("also_used_by", "EffectAlsoUsed"), ("left_out", "EffectLeftOut")],
        "EffectPart" => &[("strips", "EffectStrip"), ("may_not_follow", "EffectNote"), ("does_not_follow", "EffectNote"), ("multipliers", "EffectSite"), ("kind", "EffectPartKind"), ("controls", "EffectOffers")],
        "EffectOffers" => &[("hide", "EffectOffer"), ("size", "EffectOffer"), ("opacity", "EffectOffer"), ("tint", "EffectOffer")],
        "EffectStrip" => &[("stops", "EffectStop")],
        "EffectAlsoUsed" => &[("records", "EffectRecordUse"), ("effects", "EffectUse")],
        "EffectPreview" => &[("parts", "EffectPartPreview"), ("palette", "EffectColorChange"), ("main", "EffectColorChange"), ("pick", "EffectPick")],
        "EffectPartPreview" => &[("strips", "EffectStripPreview"), ("multipliers", "EffectSitePreview"), ("controls", "EffectControlPreview")],
        "EffectStripPreview" => &[("stops", "EffectStopPreview")],
        _ => &[],
    };
    for (key, inner) in nested {
        match object.get(*key) {
            Some(Value::Array(items)) => {
                for (i, item) in items.iter().enumerate() {
                    conforms(types, inner, item, &format!("{at}.{key}[{i}]"), problems);
                }
            }
            Some(Value::Null) | None => {}
            Some(item) => conforms(types, inner, item, &format!("{at}.{key}"), problems),
        }
    }
    // `multipliers` of a tree: a record of site counts.
    if name == "EffectTree" {
        if let Some(Value::Object(map)) = object.get("multipliers") {
            for (k, v) in map {
                conforms(types, "EffectSiteCounts", v, &format!("{at}.multipliers.{k}"), problems);
            }
        }
    }
}

fn settings_for(temp: &Path, pak: &str) -> Settings {
    let mut settings = Settings::default();
    settings.pak = pak.into();
    settings.workspace = temp.join("workspaces").display().to_string();
    if let Ok(cli) = std::env::var("DMB_SOURCE2VIEWER") {
        settings.source2viewer = cli;
    }
    settings
}

#[test]
#[ignore = "needs DMB_PAK (the game's pak01_dir.vpk); DMB_SOURCE2VIEWER adds the hero's 3D view"]
fn the_effect_commands_on_the_real_game() {
    let Ok(pak) = std::env::var("DMB_PAK") else { return };
    let temp = tempfile::tempdir().unwrap();
    let app = Application::new(temp.path().join("state")).unwrap();
    app.save_settings(&settings_for(temp.path(), &pak)).unwrap();
    let mut project = Project::default();
    project.original_model = GEIST.into();
    let types = interfaces();

    // Before the assets are loaded both commands say so (and the page offers Load character assets).
    let error = app.effect_trees(project.clone()).unwrap_err();
    assert!(error.to_string().contains(NOT_LOADED_YET), "{error}");
    assert!(error.help.is_some(), "{error}");
    let error = app.effect_preview(project.clone(), BOMB.into(), TreeEdit::default(), None, None).unwrap_err();
    assert!(error.to_string().contains(NOT_LOADED_YET), "{error}");

    let started = std::time::Instant::now();
    app.discover_extras(project.clone()).unwrap();
    eprintln!("discover_extras: {:.0} ms", started.elapsed().as_secs_f64() * 1000.);
    // Loaded assets, but no trees yet: the preview asks for them.
    let error = app.effect_preview(project.clone(), BOMB.into(), TreeEdit::default(), None, None).unwrap_err();
    assert!(error.to_string().contains(catalogs::EFFECTS_NOT_LOADED), "{error}");
    assert!(error.help.is_some(), "{error}");

    // First call builds (an operation), the second returns the kept trees (none).
    let before = app.get_timings().unwrap().run_id;
    let started = std::time::Instant::now();
    let trees = app.effect_trees(project.clone()).unwrap();
    eprintln!("effect_trees (first): {:.0} ms, {} trees", started.elapsed().as_secs_f64() * 1000., trees.trees.len());
    let built = app.get_timings().unwrap().run_id;
    assert_ne!(built, before, "the first call is an operation");
    let started = std::time::Instant::now();
    let again = app.effect_trees(project.clone()).unwrap();
    eprintln!("effect_trees (kept): {:.2} ms", started.elapsed().as_secs_f64() * 1000.);
    assert_eq!(app.get_timings().unwrap().run_id, built, "the kept trees need no operation");
    assert_eq!(serde_json::to_value(&again).unwrap(), serde_json::to_value(&trees).unwrap());
    // The assets loaded again for the same hero and game (a second opening of the tab): the trees stay.
    let started = std::time::Instant::now();
    app.discover_extras(project.clone()).unwrap();
    eprintln!("discover_extras (again): {:.0} ms", started.elapsed().as_secs_f64() * 1000.);
    let discovered = app.get_timings().unwrap().run_id;
    let started = std::time::Instant::now();
    let kept = app.effect_trees(project.clone()).unwrap();
    eprintln!("effect_trees (after loading the assets again): {:.2} ms", started.elapsed().as_secs_f64() * 1000.);
    assert_eq!(app.get_timings().unwrap().run_id, discovered, "the trees are kept over a new discovery");
    assert_eq!(serde_json::to_value(&kept).unwrap(), serde_json::to_value(&trees).unwrap());

    let json = serde_json::to_value(&trees).unwrap();
    let mut problems = BTreeSet::new();
    conforms(&types, "EffectTrees", &json, "trees", &mut problems);
    let bomb = trees.trees.iter().find(|t| t.target.eq_ignore_ascii_case(BOMB)).unwrap_or_else(|| panic!("{BOMB} is one of Lady Geist's trees"));
    assert!(bomb.main.is_some(), "the bomb has a main color");

    // Previews: identity, a hue turn, a pick.
    let identity = app.effect_preview(project.clone(), BOMB.into(), TreeEdit::default(), None, None).unwrap();
    assert_eq!(identity.changed.colors, 0);
    let turn = TreeEdit { hue: 115., ..Default::default() };
    let turned = app.effect_preview(project.clone(), BOMB.into(), turn.clone(), None, None).unwrap();
    assert!(turned.changed.colors > 0, "a hue turn changes colors");
    let main = turned.main.as_ref().unwrap();
    assert_ne!(main.before, main.after);
    let picked = app.effect_preview(project.clone(), BOMB.into(), TreeEdit::default(), Some(PickRequest { color: [140, 60, 255], main: None }), None).unwrap();
    let pick = picked.pick.as_ref().expect("a pick answers its shift");
    assert!(pick.hue.abs() > 1., "{pick:?}");
    // A part's own settings on every part (each says what it does or why not).
    let parts = bomb.parts.iter().map(|p| (p.path.clone(), PartSettings { hidden: true, size: Some(2.), opacity: Some(0.5), tint: Some([150, 40, 255]), light: Some(2.) })).collect();
    let parted = app.effect_preview(project.clone(), BOMB.into(), TreeEdit { parts, ..Default::default() }, None, None).unwrap();
    assert!(parted.parts.iter().all(|p| !p.controls.is_empty()));
    for (name, value) in [("identity", &identity), ("turned", &turned), ("picked", &picked), ("parted", &parted)] {
        conforms(&types, "EffectPreview", &serde_json::to_value(value).unwrap(), name, &mut problems);
    }
    let unknown = app.effect_preview(project.clone(), "particles/nothing.vpcf_c".into(), TreeEdit::default(), None, None).unwrap_err();
    eprintln!("unknown target: {unknown}");
    assert!(problems.is_empty(), "the JSON differs from effectTypes.ts:\n{}", problems.into_iter().collect::<Vec<_>>().join("\n"));

    // The hero's 3D view, while previews keep answering behind its lease.
    if std::env::var("DMB_SOURCE2VIEWER").is_err() {
        eprintln!("DMB_SOURCE2VIEWER is not set: hero_scene is not checked");
        return;
    }
    let (packet, during) = std::thread::scope(|s| {
        let scene = s.spawn(|| app.hero_scene(project.clone()));
        let waited = std::time::Instant::now();
        while !app.is_busy() && !scene.is_finished() && waited.elapsed().as_secs() < 20 {
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let mut during = vec![];
        if app.is_busy() {
            during.push(("effect_preview", app.effect_preview(project.clone(), BOMB.into(), turn.clone(), None, None).map(|_| ()).map_err(|e| e.to_string())));
            during.push(("effect_trees (kept)", app.effect_trees(project.clone()).map(|_| ()).map_err(|e| e.to_string())));
            during.push(("still busy afterwards", if app.is_busy() { Ok(()) } else { Err("hero_scene finished before the calls were made".to_owned()) }));
        }
        (scene.join().unwrap(), during)
    });
    assert!(!during.is_empty(), "hero_scene held the lease");
    for (what, result) in during {
        assert!(result.is_ok(), "{what} while hero_scene holds the lease: {result:?}");
    }
    let packet = packet.unwrap();
    assert_eq!(&packet[..4], b"DMBP");
    let length = u32::from_le_bytes(packet[8..12].try_into().unwrap()) as usize;
    let metadata: Value = serde_json::from_slice(&packet[16..16 + length]).unwrap();
    let meshes = metadata["meshes"].as_array().unwrap();
    assert!(!meshes.is_empty(), "the hero has meshes");
    assert!(meshes.iter().any(|m| m["target"].is_string()), "meshes name their Colors rows");
    eprintln!("hero_scene: {} meshes, {} bytes", meshes.len(), packet.len());
}

/// A game archive that changes on disk after the trees were read is refused
/// by both commands; a new discovery reads it again. Synthetic archive: one
/// hero model and one particle system in the hero's folder.
#[test]
fn a_changed_game_archive_is_refused_until_the_assets_are_loaded_again() {
    const HERO: &str = "models/heroes_staging/hornet_v3/hornet.vmdl_c";
    const EFFECT: &str = "particles/abilities/hornet/hornet_test_glow.vpcf_c";
    let temp = tempfile::tempdir().unwrap();
    let app = Application::new(temp.path().join("state")).unwrap();
    let particle = std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("crates/deadlock-components/tests/fixtures/particle.bin")).unwrap();
    let tree = temp.path().join("tree");
    let write = |extra: &[(&str, &[u8])], pak: &Path| {
        let mut files: Vec<(&str, &[u8])> = vec![(HERO, &particle), (EFFECT, &particle)];
        files.extend_from_slice(extra);
        for (path, raw) in &files {
            safety::atomic_write(&tree.join(path), raw).unwrap();
        }
        deadlock_vpk::write_vpk(&tree, &files.iter().map(|f| f.0.to_string()).collect::<Vec<_>>(), pak, None, 2, &mut || Ok(())).unwrap();
    };
    let pak = temp.path().join("pak18_dir.vpk");
    write(&[], &pak);
    app.save_settings(&settings_for(temp.path(), &pak.display().to_string())).unwrap();
    let mut project = Project::default();
    project.original_model = HERO.into();
    let catalog = app.discover_extras(project.clone()).unwrap();
    assert!(catalog.effects.iter().any(|r| r.target.eq_ignore_ascii_case(EFFECT)), "{:?}", catalog.warnings);
    let trees = app.effect_trees(project.clone()).unwrap();
    assert_eq!(trees.trees.len(), 1);
    app.effect_preview(project.clone(), EFFECT.into(), TreeEdit { hue: 90., ..Default::default() }, None, None).unwrap();

    // The game updates: the archive's modification time changes (the kept
    // catalog holds the file open, so only its attributes can be written here).
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_WRITE_ATTRIBUTES: u32 = 0x100;
        let file = std::fs::OpenOptions::new().access_mode(FILE_WRITE_ATTRIBUTES).open(&pak).unwrap();
        file.set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(3600)).unwrap();
    }
    let error = app.effect_trees(project.clone()).unwrap_err();
    assert!(error.to_string().contains(catalogs::EFFECTS_GAME_CHANGED), "{error}");
    assert!(error.help.is_some());
    let error = app.effect_preview(project.clone(), EFFECT.into(), TreeEdit::default(), None, None).unwrap_err();
    assert!(error.to_string().contains(catalogs::EFFECTS_GAME_CHANGED), "{error}");
    assert!(error.help.is_some());

    // The effect player is refused the same way (and never started).
    let error = app.effect_player_play(project.clone(), EFFECT.into(), TreeEdit::default(), None).unwrap_err();
    assert!(error.to_string().contains(catalogs::EFFECTS_GAME_CHANGED), "{error}");
    assert!(error.help.is_some());
    assert_eq!(app.effect_player().pid(), None);

    // Reload assets reads the archive again.
    app.discover_extras(project.clone()).unwrap();
    assert_eq!(app.effect_trees(project.clone()).unwrap().trees.len(), 1);
    app.effect_preview(project.clone(), EFFECT.into(), TreeEdit::default(), None, None).unwrap();
}

/// Loading the same hero's assets again (opening Extras → Effects again,
/// Reload assets with an unchanged game) keeps the trees already read:
/// `effect_trees` then answers without an operation. Assets loaded for
/// another hero in between start empty, so the trees are read again.
/// Synthetic archive: two hero models with one particle system each.
#[test]
fn loading_the_same_assets_again_keeps_the_trees_and_another_hero_does_not() {
    const HERO: &str = "models/heroes_staging/hornet_v3/hornet.vmdl_c";
    const EFFECT: &str = "particles/abilities/hornet/hornet_test_glow.vpcf_c";
    const OTHER: &str = "models/heroes_wip/geist/geist.vmdl_c";
    const OTHER_EFFECT: &str = "particles/abilities/ghost/ghost_test_glow.vpcf_c";
    let temp = tempfile::tempdir().unwrap();
    let app = Application::new(temp.path().join("state")).unwrap();
    let particle = std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("crates/deadlock-components/tests/fixtures/particle.bin")).unwrap();
    let tree = temp.path().join("tree");
    let files = [HERO, EFFECT, OTHER, OTHER_EFFECT];
    for path in files {
        safety::atomic_write(&tree.join(path), &particle).unwrap();
    }
    let pak = temp.path().join("pak18_dir.vpk");
    deadlock_vpk::write_vpk(&tree, &files.iter().map(|f| f.to_string()).collect::<Vec<_>>(), &pak, None, 2, &mut || Ok(())).unwrap();
    app.save_settings(&settings_for(temp.path(), &pak.display().to_string())).unwrap();
    let mut project = Project::default();
    project.original_model = HERO.into();
    let mut other = Project::default();
    other.original_model = OTHER.into();
    let run = || app.get_timings().unwrap().run_id;

    app.discover_extras(project.clone()).unwrap();
    let trees = app.effect_trees(project.clone()).unwrap();
    assert_eq!(trees.trees.len(), 1);

    // The same hero and archive again: the trees are kept.
    app.discover_extras(project.clone()).unwrap();
    let discovered = run();
    let again = app.effect_trees(project.clone()).unwrap();
    assert_eq!(run(), discovered, "the kept trees need no operation");
    assert_eq!(serde_json::to_value(&again).unwrap(), serde_json::to_value(&trees).unwrap());
    app.effect_preview(project.clone(), EFFECT.into(), TreeEdit { hue: 90., ..Default::default() }, None, None).unwrap();

    // Another hero: its own catalog, without the first hero's trees.
    app.discover_extras(other.clone()).unwrap();
    let error = app.effect_trees(project.clone()).unwrap_err();
    assert!(error.to_string().contains(NOT_LOADED_YET), "{error}");
    let discovered = run();
    let others = app.effect_trees(other.clone()).unwrap();
    assert_ne!(run(), discovered, "another hero's trees are read");
    assert!(others.trees.iter().all(|t| !t.target.eq_ignore_ascii_case(EFFECT)));

    // Back to the first hero: read again, not taken from before the other hero.
    app.discover_extras(project.clone()).unwrap();
    let discovered = run();
    assert_eq!(app.effect_trees(project.clone()).unwrap().trees.len(), 1);
    assert_ne!(run(), discovered, "the trees are read again after another hero");
}

/// A project without a replacement model and without any extra (a build
/// refuses it: nothing to build) still plays the game's effect: "yours" is
/// then what a build writes for it, nothing. Synthetic archive: one hero
/// model and one particle system in the hero's folder.
#[test]
fn the_player_plays_the_games_effect_when_there_is_nothing_to_build() {
    const HERO: &str = "models/heroes_staging/hornet_v3/hornet.vmdl_c";
    const EFFECT: &str = "particles/abilities/hornet/hornet_test_glow.vpcf_c";
    let temp = tempfile::tempdir().unwrap();
    let app = Application::new(temp.path().join("state")).unwrap();
    let particle = std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("crates/deadlock-components/tests/fixtures/particle.bin")).unwrap();
    let tree = temp.path().join("tree");
    for path in [HERO, EFFECT] {
        safety::atomic_write(&tree.join(path), &particle).unwrap();
    }
    let pak = temp.path().join("pak18_dir.vpk");
    deadlock_vpk::write_vpk(&tree, &[HERO.to_string(), EFFECT.to_string()], &pak, None, 2, &mut || Ok(())).unwrap();
    app.save_settings(&settings_for(temp.path(), &pak.display().to_string())).unwrap();
    let mut project = Project::default();
    project.original_model = HERO.into();
    project.customization = serde_json::from_value(serde_json::json!({"enabled": true, "model_enabled": false})).unwrap();
    app.discover_extras(project.clone()).unwrap();
    app.effect_trees(project.clone()).unwrap();

    let answer = app.effect_player_play(project.clone(), EFFECT.into(), TreeEdit::default(), None);
    // An unsaved edit is only checked for not being refused here: this
    // particle has no colors to turn.
    let edited = app.effect_player_play(project.clone(), EFFECT.into(), TreeEdit { hue: 90., ..Default::default() }, None);
    app.effect_player_stop();
    let answer = answer.unwrap_or_else(|e| panic!("the player plays the game's effect: {e}"));
    assert!(answer.overlay.is_empty(), "nothing is written: {:?}", answer.overlay);
    assert!(answer.error.as_deref().is_none_or(|e| !e.contains(deadlock_components::service::NOTHING_TO_BUILD)), "{answer:?}");
    edited.unwrap_or_else(|e| panic!("an edit is not refused either: {e}"));
    assert_eq!(app.effect_player().pid(), None);
}
