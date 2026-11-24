//! Effect edits (Extras → Effects) through the real asset-only build, its
//! manifest check and install, on synthetic particle systems in a hero folder.
use deadlock_components::effect_build::{Outcome, EFFECT_NOT_IN_GAME};
use deadlock_components::effect_edit::{self, EffectEdit};
use deadlock_core::{
    process::{Runner, RunnerOptions},
    safety, CancellationToken, TimingRecorder,
};
use deadlock_pipeline::{build::BuildService, inspect::ModelPipeline};
use deadlock_project::{
    customization::ResourceReplacement,
    effect_edit::{EffectEditRow, TreeEdit},
    Project, Settings,
};
use deadlock_resource::{binary, container, Annotation, Kind, Value};
use deadlock_vpk::{write_vpk, Vpk};
use std::{collections::BTreeMap, fs, path::Path};

const SHOT: &str = "particles/abilities/hornet/hornet_shot.vpcf_c";
const CORE: &str = "particles/abilities/hornet/hornet_shot_core.vpcf_c";
const SHARED: &str = "particles/abilities/hornet/hornet_shared.vpcf_c";
const BLAST: &str = "particles/abilities/hornet/hornet_blast.vpcf_c";
const BROKEN: &str = "particles/abilities/hornet/hornet_broken.vpcf_c";
const BAD: &str = "particles/abilities/hornet/hornet_bad.vpcf_c";

fn fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../deadlock-components/tests/fixtures").join(name)
}
fn s(t: &str) -> Value {
    Value::new(Kind::String(t.into()))
}
fn obj(rows: Vec<(&str, Value)>) -> Value {
    Value::new(Kind::Object(rows.into_iter().map(|(k, v)| (k.to_owned(), v)).collect()))
}
/// A particle system with one color, a sprite and children.
fn system(rgb: [i64; 3], kids: &[&str]) -> Vec<u8> {
    let color = Value::new(Kind::Array(rgb.iter().chain(&[255]).map(|n| Value::new(Kind::I64(*n))).collect()));
    let children = kids
        .iter()
        .map(|k| {
            let mut path = s(&k.replace(".vpcf_c", ".vpcf"));
            path.annotation = Annotation::Resource;
            obj(vec![("m_ChildRef", path)])
        })
        .collect();
    let root = obj(vec![
        ("_class", s("CParticleSystemDefinition")),
        ("m_ConstantColor", color),
        ("m_Renderers", Value::new(Kind::Array(vec![obj(vec![("_class", s("C_OP_RenderSprites"))])]))),
        ("m_Children", Value::new(Kind::Array(children))),
    ]);
    resource(&root)
}
/// `root` as the DATA of the fixture particle resource.
fn resource(root: &Value) -> Vec<u8> {
    let base = fs::read(fixture("particle.bin")).unwrap();
    let layout = container::layout(&base).unwrap();
    let block = container::unique(&layout.blocks, "DATA").unwrap();
    let doc = binary::decode(container::payload(&base, block).unwrap()).unwrap();
    container::rebuild(&base, &BTreeMap::from([("DATA".to_owned(), binary::encode(root, doc.format_guid).unwrap())])).unwrap()
}

struct World {
    temp: tempfile::TempDir,
    settings: Settings,
    token: CancellationToken,
    runner: Runner,
}
/// The hero fixture plus particle systems: two trees sharing a part, and
/// (with `broken`) a tree whose child cannot be read.
fn world(broken: bool) -> World {
    let temp = tempfile::tempdir().unwrap();
    let token = CancellationToken::new();
    let tree = temp.path().join("game-source");
    let base = Vpk::open(fixture("components_dir.vpk")).unwrap();
    let mut paths = vec![];
    for entry in base.entries.values() {
        safety::atomic_write(&tree.join(&entry.path), base.read(&entry.path).unwrap()).unwrap();
        paths.push(entry.path.clone());
    }
    let mut files = vec![
        (SHOT, system([200, 40, 40], &[CORE, SHARED])),
        (CORE, system([20, 200, 180], &[])),
        (SHARED, system([40, 200, 40], &[])),
        (BLAST, system([40, 40, 200], &[SHARED])),
    ];
    if broken {
        files.push((BROKEN, system([90, 90, 200], &[BAD])));
        files.push((BAD, resource(&obj(vec![("_class", s("NotAParticleSystem"))]))));
    }
    for (path, raw) in files {
        safety::atomic_write(&tree.join(path), raw).unwrap();
        paths.push(path.into());
    }
    let pak = temp.path().join("pak42_dir.vpk");
    write_vpk(&tree, &paths, &pak, None, 2, &mut || Ok(())).unwrap();
    let mut settings = Settings::default();
    settings.workspace = temp.path().join("workspace").display().to_string();
    settings.deadlock = temp.path().join("deadlock").display().to_string();
    settings.pak = pak.display().to_string();
    safety::atomic_copy(&pak, &Path::new(&settings.deadlock).join("game/citadel/pak01_dir.vpk"), &token).unwrap();
    let runner = Runner::new(temp.path().join("logs"), token.clone(), RunnerOptions::default()).unwrap();
    World { temp, settings, token, runner }
}
fn project() -> Project {
    let mut project = Project::default();
    project.original_model = "models/heroes_staging/hornet_v3/hornet.vmdl".into();
    project.install = false;
    project.customization.enabled = Some(true);
    project.customization.model_enabled = Some(false);
    project
}
fn edit(target: &str, hue: f64) -> EffectEditRow {
    EffectEditRow { target: target.into(), enabled: true, edit: TreeEdit { hue, ..Default::default() }, main: None }
}

#[test]
fn effect_edits_build_install_and_follow_their_collision_rules() {
    let w = world(false);
    let clock = TimingRecorder::new("effect edit build");
    let service = BuildService {
        model: ModelPipeline { settings: &w.settings, runner: &w.runner, cancel: &w.token, clock: &clock, log: &|_| {}, reader_root: w.temp.path() },
    };
    // The user's own file for the core (an edit of the game's file, so it passes the effect check).
    let base = Vpk::open(&w.settings.pak).unwrap();
    let own = effect_edit::edit_file(&base.read(CORE).unwrap(), &EffectEdit { hue: 30., ..Default::default() }).unwrap().bytes;
    let own_path = w.temp.path().join("own_core.vpcf_c");
    fs::write(&own_path, &own).unwrap();
    let mut project = project();
    project.customization.effects = Some(vec![ResourceReplacement {
        enabled: Some(true),
        source: Some(own_path.display().to_string()),
        target: Some(CORE.into()),
        ..Default::default()
    }]);
    project.customization.effect_edits = Some(vec![edit(SHOT, 90.), edit(BLAST, 180.)]);
    let result = service.build(&mut project, false, false).unwrap();
    let output = result.vpk.unwrap();
    let built = Vpk::open(&output).unwrap();
    // Every edited part, the shared one once, and the user's own core.
    assert_eq!(built.entries.keys().collect::<Vec<_>>(), [BLAST, SHARED, SHOT, CORE]);
    assert_eq!(built.read(CORE).unwrap(), own, "the user's own file wins for its part");
    let shared_first = effect_edit::edit_file(&base.read(SHARED).unwrap(), &EffectEdit { hue: 90., ..Default::default() }).unwrap().bytes;
    assert_eq!(built.read(SHARED).unwrap(), shared_first, "the shared part follows the first row");
    // The receipt names each outcome and the two collisions in plain words.
    let manifest = service.read_manifest(&project).unwrap().customization.unwrap();
    let trees = &manifest.effect_edits.trees;
    let outcome = |tree: usize, path: &str| trees[tree].parts.iter().find(|p| p.path == path).unwrap().outcome;
    assert_eq!(outcome(0, CORE), Outcome::UserFile);
    assert_eq!(outcome(0, SHARED), Outcome::Written);
    assert_eq!(outcome(1, SHARED), Outcome::OtherEffect);
    assert_eq!(manifest.effect_edits.notes().len(), 2);
    assert_eq!(manifest.effect_edits.entries.len(), 3);
    // Install, then a changed edit stops a reinstall.
    service.install(&mut project, &output, false).unwrap();
    let installed = Path::new(&w.settings.deadlock).join("game/citadel/addons").join(output.file_name().unwrap());
    assert_eq!(fs::read(&installed).unwrap(), fs::read(&output).unwrap());
    project.customization.effect_edits = Some(vec![edit(SHOT, 91.), edit(BLAST, 180.)]);
    let error = service.install(&mut project, &output, true).unwrap_err().to_string();
    // The saved project in the package no longer matches (the edit check behind it says "Effect edits changed").
    assert!(error.contains("differs from the current project") || error.contains("Effect edits changed"), "{error}");
    // Paused rows and Extras off leave the edits out.
    project.customization.effect_edits = Some(vec![EffectEditRow { enabled: false, ..edit(SHOT, 90.) }]);
    project.customization.effects = None;
    assert!(service.build(&mut project, true, false).is_err(), "nothing left to build without a model");
    // A target that is not one of the hero's effect tops is refused.
    project.customization.effect_edits = Some(vec![edit(SHARED, 90.)]);
    let error = service.build(&mut project, true, false).unwrap_err().to_string();
    assert!(error.contains(EFFECT_NOT_IN_GAME), "{error}");
}

/// Lady Geist's seven effect trees built through the real asset-only build from
/// the installed game (`DMB_PAK`, read only), compared file by file with
/// reference packages made earlier with known settings (`DMB_EFFECT_VPKS`, the
/// folder holding one package in each set folder): `1-recolor` for a hue turn
/// of +115, `4-size-everywhere` for the same with size × 3.
/// By default the two default bullet-impact parts every hero shares are left
/// out, so the build holds exactly the files both sides share; choosing them
/// (`shared`) adds them.
#[test]
#[ignore = "needs the game's pak01_dir.vpk in DMB_PAK and the reference packages in DMB_EFFECT_VPKS"]
fn lady_geist_builds_match_the_reference_packages() {
    let Ok(pak) = std::env::var("DMB_PAK") else { return };
    let Ok(vpks) = std::env::var("DMB_EFFECT_VPKS") else { return };
    const TOPS: [&str; 7] = [
        "particles/weapon_fx/ghost/ghost_tracer.vpcf_c",
        "particles/weapon_fx/ghost/ghost_muzzle_flash.vpcf_c",
        "particles/weapon_fx/ghost/ghost_tracer_impact.vpcf_c",
        "particles/abilities/ghost/ghost_blood_bomb_projectile.vpcf_c",
        "particles/abilities/ghost/ghost_blood_bomb_warning.vpcf_c",
        "particles/abilities/ghost/ghost_blood_bomb_explode.vpcf_c",
        "particles/abilities/ghost/ghost_blood_bomb_spilled_blood.vpcf_c",
    ];
    const DEFAULTS: [&str; 2] = ["particles/weapon_fx/default_tracer_impact_debris.vpcf_c", "particles/weapon_fx/default_tracer_impact_dust.vpcf_c"];
    for (set, size, tick) in [("1-recolor", 1., false), ("4-size-everywhere", 3., false), ("1-recolor", 1., true)] {
        let temp = tempfile::tempdir().unwrap();
        let token = CancellationToken::new();
        let mut settings = Settings::default();
        settings.workspace = temp.path().join("workspace").display().to_string();
        settings.deadlock = temp.path().join("deadlock").display().to_string();
        settings.pak = pak.clone();
        let runner = Runner::new(temp.path().join("logs"), token.clone(), RunnerOptions::default()).unwrap();
        let clock = TimingRecorder::new("effect edit e2e");
        let service = BuildService {
            model: ModelPipeline { settings: &settings, runner: &runner, cancel: &token, clock: &clock, log: &|_| {}, reader_root: temp.path() },
        };
        let mut project = project();
        project.original_model = "models/heroes_wip/geist/geist.vmdl".into();
        project.customization.effect_edits = Some(
            TOPS.iter()
                .map(|t| {
                    let shared = if tick && t.ends_with("ghost_tracer_impact.vpcf_c") { DEFAULTS.map(String::from).to_vec() } else { vec![] };
                    EffectEditRow { target: (*t).into(), enabled: true, edit: TreeEdit { hue: 115., size, shared, ..Default::default() }, main: None }
                })
                .collect(),
        );
        let started = std::time::Instant::now();
        let output = service.build(&mut project, false, false).unwrap().vpk.unwrap();
        let built = Vpk::open(&output).unwrap();
        let test = Vpk::open(reference_package(&vpks, set)).unwrap();
        let (mut same, mut different, mut only_built, mut only_test) = (vec![], vec![], vec![], vec![]);
        for path in built.entries.keys() {
            match test.entries.contains_key(path) {
                true if built.read(path).unwrap() == test.read(path).unwrap() => same.push(path.clone()),
                true => different.push(path.clone()),
                false => only_built.push(path.clone()),
            }
        }
        let game = Vpk::open(&pak).unwrap();
        for path in test.entries.keys().filter(|p| !built.entries.contains_key(*p)) {
            let unchanged = effect_edit::document(&game.read(path).unwrap()).unwrap().root.canonical_eq(&effect_edit::document(&test.read(path).unwrap()).unwrap().root);
            only_test.push(format!("{path}{}", if unchanged { " (the same values as the game's file)" } else { " (DIFFERENT values)" }));
        }
        eprintln!(
            "{set}{}: built {} files in {:.1} s; byte-identical {}; different {:?}; only in the build {:?}; only in the reference {:?}",
            if tick { " (shared parts chosen)" } else { "" },
            built.entries.len(),
            started.elapsed().as_secs_f64(),
            same.len(),
            different,
            only_built,
            only_test
        );
        assert!(different.is_empty(), "{set}: files differ: {different:?}");
        assert!(only_test.iter().all(|p| p.ends_with("(the same values as the game's file)")), "{set}: {only_test:?}");
        assert_eq!(only_built, if tick { DEFAULTS.map(String::from).to_vec() } else { vec![] }, "{set}");
    }
}

/// A part's own settings (Advanced → the part's Shown / Hidden switch and
/// its own Size, Opacity and Tint) built through the real asset-only build
/// for Haze from the installed game (`DMB_PAK`, read only), compared file by
/// file with reference packages made earlier with these settings (`DMB_EFFECT_CONTROL_VPKS`,
/// the folder holding `1-hide-renderers`, `3-model-size`, `4-model-opacity`
/// and `5-model-colour`). Each part's settings go into the row of the first
/// of Haze's effects that holds it. The build must write exactly the edited
/// parts, each byte for byte as in the reference package. In `5-model-colour`
/// only the dagger is a part's own tint; that set's other file is a recolor.
#[test]
#[ignore = "needs the game's pak01_dir.vpk in DMB_PAK and the reference packages in DMB_EFFECT_CONTROL_VPKS"]
fn haze_part_settings_match_the_reference_packages() {
    use deadlock_components::{effect_build, effect_trees, resources::{self, ResourceKind}};
    use deadlock_project::effect_edit::PartSettings;
    let Ok(pak) = std::env::var("DMB_PAK") else { return };
    let Ok(vpks) = std::env::var("DMB_EFFECT_CONTROL_VPKS") else { return };
    const HAZE: &str = "models/heroes_staging/haze/haze.vmdl";
    const TRAIL: &str = "particles/weapon_fx/haze/haze_tracer_trail.vpcf_c";
    const MUZZLE: &str = "particles/weapon_fx/haze/haze_muzzle_flash.vpcf_c";
    const BULLET: &str = "particles/weapon_fx/haze/haze_tracer_bullet_model.vpcf_c";
    const DAGGER: &str = "particles/abilities/haze/haze_sleep_dagger_model.vpcf_c";
    const POSES: &str = "particles/abilities/haze/haze_flurry_poses_model.vpcf_c";
    let hidden = PartSettings { hidden: true, ..Default::default() };
    let size = PartSettings { size: Some(3.), ..Default::default() };
    let opacity = PartSettings { opacity: Some(0.2), ..Default::default() };
    let tint = PartSettings { tint: Some([150, 40, 255]), ..Default::default() };
    let sets: [(&str, Vec<(&str, PartSettings)>); 4] = [
        ("1-hide-renderers", vec![(TRAIL, hidden.clone()), (MUZZLE, hidden)]),
        ("3-model-size", vec![(BULLET, size.clone()), (DAGGER, size.clone()), (POSES, size)]),
        ("4-model-opacity", vec![(BULLET, opacity.clone()), (DAGGER, opacity.clone()), (POSES, opacity)]),
        ("5-model-colour", vec![(DAGGER, tint)]),
    ];
    let game = Vpk::open(&pak).unwrap();
    let token = CancellationToken::new();
    let catalog = resources::discover(&game, HAZE, ResourceKind::Effects, &token).unwrap();
    let (trees, _) = effect_trees::tree_files(&game, &catalog, &token).unwrap();
    let outside = |part: &str| effect_trees::outside_hero(catalog.iter().find(|r| r.target.eq_ignore_ascii_case(part)));
    let mut failures = vec![];
    let mut refused: BTreeMap<String, Option<String>> = BTreeMap::new();
    for (set, settings) in sets {
        // One row per tree, in the trees' order; each part in the first tree that holds it.
        let mut rows: Vec<EffectEditRow> = vec![];
        for (part, s) in &settings {
            let tree = trees.iter().find(|t| t.parts.iter().any(|p| p.eq_ignore_ascii_case(part))).unwrap_or_else(|| panic!("{part} is in none of Haze's effects"));
            let at = match rows.iter().position(|r| r.target.eq_ignore_ascii_case(&tree.top)) {
                Some(at) => at,
                None => {
                    rows.push(EffectEditRow { target: tree.top.clone(), enabled: true, edit: TreeEdit::default(), main: None });
                    rows.len() - 1
                }
            };
            rows[at].edit.parts.insert((*part).into(), s.clone());
            if outside(part) {
                rows[at].edit.shared.push((*part).into());
            }
            eprintln!("{set}: {part} in the row of {}{}", tree.top, if outside(part) { " (chosen as a shared part)" } else { "" });
        }
        // Trees the asset-only build refuses whatever the edit (its dependency check
        // follows a part's model to files the game archive lacks): the same refusal for a
        // plain recolor of the tree. Their parts are compared through the build's own plan.
        for r in &rows {
            refused.entry(r.target.clone()).or_insert_with(|| {
                let recolor = EffectEditRow { target: r.target.clone(), enabled: true, edit: TreeEdit { hue: 90., size: 1.5, ..Default::default() }, main: None };
                match build(&pak, &token, vec![recolor]) {
                    Err(e) if e.contains("dependency is missing") => {
                        eprintln!("a plain recolor of {} is refused by the build too: {e}", r.target);
                        Some(e)
                    }
                    _ => None,
                }
            });
        }
        let (built_rows, planned_rows): (Vec<EffectEditRow>, Vec<EffectEditRow>) = rows.into_iter().partition(|r| refused[&r.target].is_none());
        let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        if !built_rows.is_empty() {
            let (_temp, output) = build(&pak, &token, built_rows).unwrap_or_else(|e| panic!("{set}: {e}"));
            let built = Vpk::open(&output).unwrap();
            for path in built.entries.keys() {
                files.insert(path.clone(), built.read(path).unwrap());
            }
        }
        if !planned_rows.is_empty() {
            let hero = effect_build::HeroTrees::read(&game, HAZE, &token).unwrap();
            for row in &planned_rows {
                let (_, written) = hero.files(&game, row, &Default::default(), &token).unwrap();
                files.extend(written.into_iter().map(|(p, b)| (p.to_lowercase(), b)));
            }
        }
        let test = Vpk::open(reference_package(&vpks, set)).unwrap();
        let expected: Vec<String> = settings.iter().map(|(p, _)| p.to_string()).collect();
        let written: Vec<String> = files.keys().cloned().collect();
        let mut wanted = expected.clone();
        wanted.sort();
        if written != wanted {
            failures.push(format!("{set}: the build wrote {written:?}, expected {wanted:?}"));
        }
        for part in &expected {
            let ours = files.get(part).cloned();
            let theirs = test.read(part).unwrap();
            match ours {
                Some(bytes) if bytes == theirs => eprintln!("{set}: {part}: byte-identical ({} bytes)", bytes.len()),
                Some(bytes) => {
                    let same = effect_edit::document(&bytes).unwrap().root.canonical_eq(&effect_edit::document(&theirs).unwrap().root);
                    failures.push(format!("{set}: {part}: bytes differ ({} vs {} bytes; values {})", bytes.len(), theirs.len(), if same { "equal" } else { "DIFFERENT" }));
                }
                None => failures.push(format!("{set}: {part} was not written")),
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// A part's own Light brightness on five light parts (Graves's weapon beam:
/// two `C_OP_RenderOmni2Light`; Infernus's gun and Concussive Combustion:
/// three `C_OP_RenderStandardLight`), from the installed game (`DMB_PAK`,
/// read only), compared with reference packages made earlier with known
/// settings (`DMB_EFFECT_LIGHT_VPKS`, the folder holding `8-lights-off` = every
/// brightness number at 0, and `9-lights-bright` = every number × 50).
/// Multipliers 0 and 50 must give those files byte for byte: through the
/// engine (`TreeEdit::edit_part`, the one source of bytes) and through the
/// build's own plan for each hero (`HeroTrees::files`, each part in the row
/// of the first of the hero's effects that holds it). Every part must offer
/// the setting.
#[test]
#[ignore = "needs the game's pak01_dir.vpk in DMB_PAK and the reference packages in DMB_EFFECT_LIGHT_VPKS"]
fn light_brightness_matches_the_reference_packages() {
    use deadlock_components::effect_colors::TreeEditExt;
    use deadlock_components::{effect_build, effect_trees, resources::{self, ResourceKind}};
    use deadlock_project::effect_edit::PartSettings;
    let Ok(pak) = std::env::var("DMB_PAK") else { return };
    let Ok(vpks) = std::env::var("DMB_EFFECT_LIGHT_VPKS") else { return };
    const GRAVES: &str = "models/heroes_wip/necro/necro.vmdl";
    const INFERNUS: &str = "models/heroes_wip/inferno/inferno.vmdl";
    let parts: [(&str, &str); 5] = [
        (GRAVES, "particles/weapon_fx/necro/necro_weapon_endpoint.vpcf_c"),
        (GRAVES, "particles/weapon_fx/necro/necro_weapon_source.vpcf_c"),
        (INFERNUS, "particles/weapon_fx/inferno/inferno_muzzle_flash_light.vpcf_c"),
        (INFERNUS, "particles/abilities/inferno/inferno_fire_bomb_charge_light.vpcf_c"),
        (INFERNUS, "particles/abilities/inferno/inferno_fire_bomb_explode_light.vpcf_c"),
    ];
    let game = Vpk::open(&pak).unwrap();
    let token = CancellationToken::new();
    let mut failures = vec![];
    for (set, factor) in [("8-lights-off", 0.), ("9-lights-bright", 50.)] {
        let test = Vpk::open(reference_package(&vpks, set)).unwrap();
        let mut listed: Vec<String> = test.entries.keys().cloned().collect();
        listed.sort();
        let mut wanted: Vec<String> = parts.iter().map(|(_, p)| p.to_string()).collect();
        wanted.sort();
        assert_eq!(listed, wanted, "{set}: the reference package holds other files");
        let light = PartSettings { light: Some(factor), ..Default::default() };
        let mut planned: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        for hero in [GRAVES, INFERNUS] {
            let catalog = resources::discover(&game, hero, ResourceKind::Effects, &token).unwrap();
            let (trees, _) = effect_trees::tree_files(&game, &catalog, &token).unwrap();
            let outside = |part: &str| effect_trees::outside_hero(catalog.iter().find(|r| r.target.eq_ignore_ascii_case(part)));
            let mut rows: Vec<EffectEditRow> = vec![];
            for (_, part) in parts.iter().filter(|(h, _)| *h == hero) {
                let tree = trees.iter().find(|t| t.parts.iter().any(|p| p.eq_ignore_ascii_case(part))).unwrap_or_else(|| panic!("{part} is in none of the hero's effects"));
                let at = match rows.iter().position(|r| r.target.eq_ignore_ascii_case(&tree.top)) {
                    Some(at) => at,
                    None => {
                        rows.push(EffectEditRow { target: tree.top.clone(), enabled: true, edit: TreeEdit::default(), main: None });
                        rows.len() - 1
                    }
                };
                rows[at].edit.parts.insert((*part).into(), light.clone());
                if outside(part) {
                    rows[at].edit.shared.push((*part).into());
                }
                eprintln!("{set}: {part} in the row of {}{}", tree.top, if outside(part) { " (chosen as a shared part)" } else { "" });
                // The engine on the part alone.
                let raw = game.read(part).unwrap();
                let root = effect_edit::document(&raw).unwrap().root;
                assert_eq!(effect_edit::light_refusal(&root), None, "{part} must offer Light brightness");
                let ours = rows[at].edit.edit_part(&raw, part, outside(part)).unwrap().bytes;
                let theirs = test.read(part).unwrap();
                if ours == theirs {
                    eprintln!("{set}: engine: {part}: byte-identical ({} bytes)", ours.len());
                } else {
                    failures.push(format!("{set}: engine: {part}: bytes differ ({} vs {} bytes)", ours.len(), theirs.len()));
                }
            }
            let trees = effect_build::HeroTrees::from_catalog(&game, &catalog, &token).unwrap();
            for row in &rows {
                let (_, written) = trees.files(&game, row, &Default::default(), &token).unwrap();
                planned.extend(written.into_iter().map(|(p, b)| (p.to_lowercase(), b)));
            }
        }
        let written: Vec<String> = planned.keys().cloned().collect();
        if written != wanted {
            failures.push(format!("{set}: the build's plan wrote {written:?}, expected {wanted:?}"));
        }
        for part in &wanted {
            match planned.get(part) {
                Some(bytes) if *bytes == test.read(part).unwrap() => eprintln!("{set}: build plan: {part}: byte-identical"),
                Some(_) => failures.push(format!("{set}: build plan: {part}: bytes differ")),
                None => failures.push(format!("{set}: build plan: {part} was not written")),
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// The one `*_dir.vpk` reference package in the folder `set` of `folder`.
fn reference_package(folder: &str, set: &str) -> std::path::PathBuf {
    let place = Path::new(folder).join(set);
    let found: Vec<_> = fs::read_dir(&place)
        .unwrap_or_else(|e| panic!("{}: {e}", place.display()))
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.file_name().and_then(|name| name.to_str()).is_some_and(|name| name.to_ascii_lowercase().ends_with("_dir.vpk")))
        .collect();
    assert_eq!(found.len(), 1, "one *_dir.vpk in {}: {found:?}", place.display());
    found.into_iter().next().unwrap()
}

/// An assets-only build of Haze with `rows` from the game at `pak`: the temp
/// folder (kept alive) and the VPK, or the build's error.
fn build(pak: &str, token: &CancellationToken, rows: Vec<EffectEditRow>) -> Result<(tempfile::TempDir, std::path::PathBuf), String> {
    let temp = tempfile::tempdir().unwrap();
    let mut s = Settings::default();
    s.workspace = temp.path().join("workspace").display().to_string();
    s.deadlock = temp.path().join("deadlock").display().to_string();
    s.pak = pak.into();
    let runner = Runner::new(temp.path().join("logs"), token.clone(), RunnerOptions::default()).unwrap();
    let clock = TimingRecorder::new("effect part settings e2e");
    let service = BuildService { model: ModelPipeline { settings: &s, runner: &runner, cancel: token, clock: &clock, log: &|_| {}, reader_root: temp.path() } };
    let mut project = project();
    project.original_model = "models/heroes_staging/haze/haze.vmdl".into();
    project.customization.effect_edits = Some(rows);
    let output = service.build(&mut project, false, false).map_err(|e| e.to_string())?.vpk.ok_or("no VPK")?;
    Ok((temp, output))
}

#[test]
fn projects_without_effect_edits_never_read_the_effect_trees() {
    // The hero's effects include a tree whose child cannot be read: reading the
    // trees fails, so a project that builds proves it never read them.
    let w = world(true);
    let clock = TimingRecorder::new("effect edit cost");
    let service = BuildService {
        model: ModelPipeline { settings: &w.settings, runner: &w.runner, cancel: &w.token, clock: &clock, log: &|_| {}, reader_root: w.temp.path() },
    };
    let base = Vpk::open(&w.settings.pak).unwrap();
    let own = effect_edit::edit_file(&base.read(SHOT).unwrap(), &EffectEdit { hue: 30., ..Default::default() }).unwrap().bytes;
    let own_path = w.temp.path().join("own_shot.vpcf_c");
    fs::write(&own_path, &own).unwrap();
    let mut project = project();
    project.customization.effects = Some(vec![ResourceReplacement {
        enabled: Some(true),
        source: Some(own_path.display().to_string()),
        target: Some(SHOT.into()),
        ..Default::default()
    }]);
    let output = service.build(&mut project, false, false).unwrap().vpk.unwrap();
    assert_eq!(Vpk::open(&output).unwrap().entries.keys().collect::<Vec<_>>(), [SHOT]);
    // With an effect edit the trees are read, and the broken one stops the build.
    project.customization.effect_edits = Some(vec![edit(BLAST, 90.)]);
    assert!(service.build(&mut project, true, false).is_err());
}
