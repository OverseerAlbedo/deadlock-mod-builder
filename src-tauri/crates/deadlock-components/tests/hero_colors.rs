//! Hero colors on synthetic compiled materials (never game files): the in-place
//! parameter edit, its refusals, discovery of shared materials and the
//! asset-only build, snapshot and install checks.
use deadlock_components::{
    materials::{self, Recolor},
    service::ComponentService,
};
use deadlock_core::{
    CancellationToken, TimingRecorder,
    process::{Runner, RunnerOptions},
};
use deadlock_project::{
    Project, Settings,
    customization::{HeroMaterial, HeroMaterials},
};
use deadlock_resource::{Annotation, Kind, Value, binary, container};
use deadlock_vpk::Vpk;
use std::{collections::BTreeMap, fs, path::Path};

const HERO: &str = "models/heroes_staging/haze/haze.vmdl";
const BODY: &str = "models/heroes_staging/haze/materials/body.vmat";
const PLAIN: &str = "models/heroes_staging/haze/materials/plain.vmat";
const ODD: &str = "models/heroes_staging/haze/materials/odd.vmat";
const SHARED: &str = "models/shared/materials/metal.vmat";
const TEXTURE: &str = "models/heroes_staging/haze/materials/body_color_psd_00000000.vtex";
const GUID: [u8; 16] = [7; 16];

fn resource(blocks: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
    let table = 16;
    let mut offsets = vec![];
    let mut at = table + blocks.len() * 12;
    for (_, payload) in blocks {
        at = (at + 15) & !15;
        offsets.push(at);
        at += payload.len();
    }
    let mut out = vec![];
    out.extend((at as u32).to_le_bytes());
    out.extend(12u16.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    out.extend(8u32.to_le_bytes());
    out.extend((blocks.len() as u32).to_le_bytes());
    for (i, (name, payload)) in blocks.iter().enumerate() {
        let field = table + i * 12 + 4;
        out.extend(*name);
        out.extend(((offsets[i] - field) as u32).to_le_bytes());
        out.extend((payload.len() as u32).to_le_bytes());
    }
    for (i, (_, payload)) in blocks.iter().enumerate() {
        out.resize(offsets[i], 0);
        out.extend(payload);
    }
    out
}
fn references(paths: &[&str]) -> Vec<u8> {
    let mut out = vec![];
    out.extend(8u32.to_le_bytes());
    out.extend((paths.len() as u32).to_le_bytes());
    let names = 8 + paths.len() * 16;
    let mut name_at = names;
    for (i, path) in paths.iter().enumerate() {
        out.extend((i as u64 + 1).to_le_bytes());
        out.extend(((name_at - (8 + i * 16 + 8)) as i64).to_le_bytes());
        name_at += path.len() + 1;
    }
    for path in paths {
        out.extend(path.as_bytes());
        out.push(0);
    }
    out
}
fn string(value: &str) -> Value {
    Value::new(Kind::String(value.into()))
}
fn object(rows: Vec<(&str, Value)>) -> Value {
    Value::new(Kind::Object(rows.into_iter().map(|(k, v)| (k.to_owned(), v)).collect()))
}
fn array(rows: Vec<Value>) -> Value {
    Value::new(Kind::Array(rows))
}
fn vector(name: &str, values: [f64; 4]) -> Value {
    object(vec![
        ("m_name", string(name)),
        ("m_value", array(values.iter().map(|v| Value::new(Kind::F64(*v))).collect())),
    ])
}
fn material(path: &str, shader: &str, vectors: Vec<Value>) -> Vec<u8> {
    let texture = Value { annotation: Annotation::Resource, kind: Kind::String(TEXTURE.into()) };
    let data = object(vec![
        ("m_materialName", string(path)),
        ("m_shaderName", string(shader)),
        ("m_intParams", array(vec![object(vec![("m_name", string("F_USE_NPR_LIGHTING")), ("m_nValue", Value::new(Kind::I64(1)))])])),
        ("m_floatParams", array(vec![])),
        ("m_vectorParams", array(vectors)),
        ("m_textureParams", array(vec![object(vec![("m_name", string("g_tColor")), ("m_pValue", texture)])])),
    ]);
    resource(&[
        (b"RERL", references(&[TEXTURE])),
        (b"RED2", b"opaque edit information".to_vec()),
        (b"DATA", binary::encode(&data, GUID).unwrap()),
        (b"INSG", b"opaque input signature".to_vec()),
    ])
}
fn body() -> Vec<u8> {
    material(BODY, "pbr.vfx", vec![
        vector("g_vAlbedoContrastSaturationBrightness1", [1., 1.12, 1.21, 0.]),
        vector("g_vColorTint1", [1., 1., 1., 0.]),
        vector("TextureRoughness1", [1., 1., 1., 0.]),
    ])
}
fn data(raw: &[u8]) -> Value {
    let layout = container::layout(raw).unwrap();
    let block = layout.blocks.iter().find(|b| b.name == "DATA").unwrap();
    binary::decode(container::payload(raw, block).unwrap()).unwrap().root
}
fn read_vector(raw: &[u8], name: &str) -> Vec<f64> {
    let root = data(raw);
    let row = root.get("m_vectorParams").unwrap().array().unwrap().iter()
        .find(|r| r.get("m_name").unwrap().string().unwrap() == name).unwrap().clone();
    row.get("m_value").unwrap().array().unwrap().iter().map(|v| v.number().unwrap()).collect()
}
fn close(a: &[f64], b: &[f64]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-9)
}
fn archive(root: &Path, files: &BTreeMap<String, Vec<u8>>) -> std::path::PathBuf {
    let source = root.join("game-files");
    for (path, raw) in files {
        let file = source.join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, raw).unwrap();
    }
    let output = root.join("game_dir.vpk");
    deadlock_vpk::write_vpk(&source, &files.keys().cloned().collect::<Vec<_>>(), &output, None, 2, &mut || Ok(())).unwrap();
    output
}
fn game_files() -> BTreeMap<String, Vec<u8>> {
    BTreeMap::from([
        (format!("{HERO}_c"), resource(&[(b"RERL", references(&[BODY, PLAIN, ODD, SHARED, "models/heroes_staging/haze/haze.vnmskel"]))])),
        ("models/props/crate.vmdl_c".into(), resource(&[(b"RERL", references(&[SHARED]))])),
        (format!("{BODY}_c"), body()),
        (format!("{PLAIN}_c"), material(PLAIN, "pbr.vfx", vec![vector("g_vColorTint1", [0.5, 0.5, 0.5, 0.])])),
        (format!("{ODD}_c"), material(ODD, "environment_blend.vfx", vec![vector("g_vColorTint1", [1., 1., 1., 0.])])),
        (format!("{SHARED}_c"), material(SHARED, "pbr.vfx", vec![vector("g_vColorTint1", [1., 1., 1., 0.])])),
        (format!("{TEXTURE}_c"), b"TEXTURE FIXTURE, NOT A GAME FILE".to_vec()),
    ])
}

#[test]
fn recolor_changes_only_the_tint_and_albedo_factors_and_keeps_every_other_block() {
    let original = body();
    let edit = Recolor { color: Some([0.1, 0.2, 0.3]), saturation: 0.5, brightness: 0. };
    let output = materials::recolor(&original, BODY, &edit).unwrap();
    assert_eq!(output, materials::recolor(&original, BODY, &edit).unwrap(), "deterministic");
    assert!(close(&read_vector(&output, "g_vColorTint1"), &[0.1, 0.2, 0.3, 0.]));
    assert!(close(&read_vector(&output, "g_vAlbedoContrastSaturationBrightness1"), &[1., 0.56, 0., 0.]));
    assert!(close(&read_vector(&output, "TextureRoughness1"), &[1., 1., 1., 0.]));
    let (before, after) = (container::layout(&original).unwrap(), container::layout(&output).unwrap());
    assert_eq!(before.blocks.iter().map(|b| &b.name).collect::<Vec<_>>(), after.blocks.iter().map(|b| &b.name).collect::<Vec<_>>());
    for (a, b) in before.blocks.iter().zip(&after.blocks).filter(|(a, _)| a.name != "DATA") {
        assert_eq!(container::payload(&original, a).unwrap(), container::payload(&output, b).unwrap(), "{}", a.name);
    }
    let (mut expected, actual) = (data(&original), data(&output));
    let Some(Kind::Array(rows)) = expected.get_mut("m_vectorParams").map(|v| &mut v.kind) else { panic!() };
    rows.retain(|r| !r.get("m_name").unwrap().string().unwrap().starts_with("g_v"));
    let mut trimmed = actual.clone();
    let Some(Kind::Array(rows)) = trimmed.get_mut("m_vectorParams").map(|v| &mut v.kind) else { panic!() };
    rows.retain(|r| !r.get("m_name").unwrap().string().unwrap().starts_with("g_v"));
    assert!(expected.canonical_eq(&trimmed), "nothing else in DATA changes");
    assert_eq!(deadlock_vpk::resource_references(&output).unwrap(), [TEXTURE]);
    // Tint alone leaves the albedo factors as the game set them.
    let tinted = materials::recolor(&original, BODY, &Recolor { color: Some([0., 0., 0.]), saturation: 1., brightness: 1. }).unwrap();
    assert!(close(&read_vector(&tinted, "g_vColorTint1"), &[0., 0., 0., 0.]));
    assert!(close(&read_vector(&tinted, "g_vAlbedoContrastSaturationBrightness1"), &[1., 1.12, 1.21, 0.]));
}

#[test]
fn recolor_refuses_other_shaders_missing_parameters_and_empty_or_invalid_changes() {
    let black = Recolor { color: Some([0., 0., 0.]), saturation: 1., brightness: 1. };
    let odd = material(ODD, "environment_blend.vfx", vec![vector("g_vColorTint1", [1., 1., 1., 0.])]);
    let error = materials::recolor(&odd, ODD, &black).unwrap_err().to_string();
    assert!(error.contains("environment_blend.vfx") && error.contains("pbr.vfx materials only"), "{error}");
    let plain = material(PLAIN, "pbr.vfx", vec![vector("g_vColorTint1", [0.5, 0.5, 0.5, 0.])]);
    assert!(materials::recolor(&plain, PLAIN, &black).is_ok());
    let error = materials::recolor(&plain, PLAIN, &Recolor { color: None, saturation: 1., brightness: 0.5 }).unwrap_err().to_string();
    assert!(error.contains("no saturation or brightness setting"), "{error}");
    let untinted = material(PLAIN, "pbr.vfx", vec![vector("g_vAlbedoContrastSaturationBrightness1", [1., 1., 1., 0.])]);
    let error = materials::recolor(&untinted, PLAIN, &black).unwrap_err().to_string();
    assert!(error.contains("no tint setting"), "{error}");
    let error = materials::recolor(&plain, PLAIN, &Recolor { color: None, saturation: 1., brightness: 1. }).unwrap_err().to_string();
    assert!(error.contains("changes nothing"), "{error}");
    assert!(materials::recolor(&plain, PLAIN, &Recolor { color: Some([1.5, 0., 0.]), saturation: 1., brightness: 1. }).is_err());
    assert!(materials::recolor(&plain, PLAIN, &Recolor { color: None, saturation: 2.5, brightness: 1. }).is_err());
    assert!(materials::recolor(b"not a resource at all", PLAIN, &black).is_err());
}

#[test]
fn discovery_lists_the_heros_materials_and_the_other_models_that_share_them() {
    let temp = tempfile::tempdir().unwrap();
    let pak = Vpk::open(archive(temp.path(), &game_files())).unwrap();
    let rows = materials::discover(&pak, HERO, &CancellationToken::new()).unwrap();
    assert_eq!(rows.iter().map(|r| r.path.as_str()).collect::<Vec<_>>(), [BODY, PLAIN, ODD, SHARED]);
    let row = |path: &str| rows.iter().find(|r| r.path == path).unwrap();
    assert!(row(BODY).supported && !row(BODY).shared && row(BODY).used_by.is_empty());
    assert_eq!(row(BODY).target, format!("{BODY}_c"));
    assert_eq!(row(BODY).tint, Some([1., 1., 1.]));
    assert!(close(&row(BODY).albedo.unwrap(), &[1., 1.12, 1.21]));
    assert_eq!(row(BODY).color_texture.as_deref(), Some(format!("{TEXTURE}_c").as_str()));
    assert!(row(PLAIN).supported && row(PLAIN).albedo.is_none());
    assert!(!row(ODD).supported && row(ODD).reason.contains("environment_blend.vfx"));
    assert!(row(SHARED).shared);
    assert_eq!(row(SHARED).used_by, ["models/props/crate.vmdl_c"]);
}

#[test]
fn the_material_preview_shows_the_base_color_opaque() {
    // Deadlock packs metalness into the base-color alpha (0 on cloth), which
    // would make the plain texture preview invisible.
    let cancel = CancellationToken::new();
    for name in ["embedded.vtex_c", "gpu.vtex_c", "compressed.vtex_c", "one_pixel.vtex_c"] {
        let raw = fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/icons").join(name)).unwrap();
        let plain = image::load_from_memory(&deadlock_components::icons::preview_texture(&raw, 64, &cancel).unwrap()).unwrap().to_rgba8();
        let opaque = image::load_from_memory(&deadlock_components::icons::preview_color_texture(&raw, 64, &cancel).unwrap()).unwrap().to_rgba8();
        assert_eq!(plain.dimensions(), opaque.dimensions(), "{name}");
        assert!(opaque.pixels().all(|p| p[3] == 255), "{name}");
    }
}

fn service_parts(root: &Path, pak: &Path) -> (Settings, CancellationToken, TimingRecorder, Runner) {
    let cancel = CancellationToken::new();
    let settings = Settings {
        workspace: root.join("workspace").to_string_lossy().into(),
        pak: pak.to_string_lossy().into(),
        ..Default::default()
    };
    let runner = Runner::new(root.join("logs"), cancel.clone(), RunnerOptions::default()).unwrap();
    (settings, cancel, TimingRecorder::new("hero colors"), runner)
}
fn colors_project(rows: Vec<HeroMaterial>) -> Project {
    let mut project = Project::default();
    project.original_model = HERO.into();
    project.customization.enabled = Some(true);
    project.customization.model_enabled = Some(false);
    project.customization.hero_materials = Some(HeroMaterials { enabled: Some(true), materials: Some(rows), ..Default::default() });
    project
}
fn black(target: &str) -> HeroMaterial {
    HeroMaterial { enabled: Some(true), target: Some(format!("{target}_c")), color: Some([0.05, 0.05, 0.05]), ..Default::default() }
}

#[test]
fn an_asset_only_build_holds_only_the_recolored_material_and_checks_it_again_at_install() {
    let temp = tempfile::tempdir().unwrap();
    let pak = archive(temp.path(), &game_files());
    let (settings, cancel, clock, runner) = service_parts(temp.path(), &pak);
    let log = |_: &str| {};
    let service = ComponentService { settings: &settings, runner: &runner, cancel: &cancel, clock: &clock, log: &log };
    let mut project = colors_project(vec![black(BODY)]);
    let output = temp.path().join("game");
    let receipt = service.compile(&project, &output).unwrap().unwrap();
    let target = format!("{BODY}_c");
    assert_eq!(receipt.files.keys().collect::<Vec<_>>(), [&target]);
    assert_eq!(receipt.materials.entries.len(), 1);
    let built = fs::read(output.join(&target)).unwrap();
    assert!(close(&read_vector(&built, "g_vColorTint1"), &[0.05, 0.05, 0.05, 0.]));
    service.verify_snapshot(&project, &receipt, &output).unwrap();
    let package = temp.path().join("pak19_dir.vpk");
    deadlock_vpk::write_vpk(&output, &receipt.files.keys().cloned().collect::<Vec<_>>(), &package, Some(&receipt.files), 2, &mut || Ok(())).unwrap();
    let candidate = Vpk::open(&package).unwrap();
    assert_eq!(candidate.entries.keys().collect::<Vec<_>>(), [&target]);
    let current = Vpk::open(&pak).unwrap();
    service.verify_install(&project, &receipt, &candidate, &current).unwrap();
    // A changed color needs a new build.
    project.customization.hero_materials.as_mut().unwrap().materials.as_mut().unwrap()[0].color = Some([1., 0., 0.]);
    assert!(service.verify_snapshot(&project, &receipt, &output).unwrap_err().to_string().contains("Hero colors changed"));
    assert!(service.verify_install(&project, &receipt, &candidate, &current).is_err());
    // A tampered package file is refused.
    project = colors_project(vec![black(BODY)]);
    fs::write(output.join(&target), materials::recolor(&body(), BODY, &Recolor { color: Some([1., 0., 0.]), saturation: 1., brightness: 1. }).unwrap()).unwrap();
    assert!(service.verify_snapshot(&project, &receipt, &output).is_err());
}

#[test]
fn hero_colors_stay_a_separate_mode_and_shared_materials_need_confirmation() {
    let temp = tempfile::tempdir().unwrap();
    let pak = archive(temp.path(), &game_files());
    let (settings, cancel, clock, runner) = service_parts(temp.path(), &pak);
    let log = |_: &str| {};
    let service = ComponentService { settings: &settings, runner: &runner, cancel: &cancel, clock: &clock, log: &log };
    let output = temp.path().join("game");
    let mut with_model = colors_project(vec![black(BODY)]);
    with_model.customization.model_enabled = Some(true);
    let error = service.compile(&with_model, &output).unwrap_err().to_string();
    assert!(error.contains("Turn off Include replacement model"), "{error}");
    let error = service.compile(&colors_project(vec![black(SHARED)]), &output).unwrap_err().to_string();
    assert!(error.contains("also used by 1 other game model (crate.vmdl)"), "{error}");
    let mut confirmed = black(SHARED);
    confirmed.allow_shared = Some(true);
    let receipt = service.compile(&colors_project(vec![confirmed]), &output).unwrap().unwrap();
    assert_eq!(receipt.materials.entries[0].used_by, ["models/props/crate.vmdl_c"]);
    let error = service.compile(&colors_project(vec![black(ODD)]), &output).unwrap_err().to_string();
    assert!(error.contains("pbr.vfx materials only"), "{error}");
    let error = service.compile(&colors_project(vec![HeroMaterial { target: Some("models/other/materials/x.vmat_c".into()), color: Some([0.; 3]), ..Default::default() }]), &output).unwrap_err().to_string();
    assert!(error.contains("not one of this hero's materials"), "{error}");
    let mut stale = black(BODY);
    stale.original_sha256 = Some("0".repeat(64));
    let error = service.compile(&colors_project(vec![stale]), &output).unwrap_err().to_string();
    assert!(error.contains("changed since you chose it"), "{error}");
    let mut paused = colors_project(vec![black(BODY)]);
    paused.customization.hero_materials.as_mut().unwrap().enabled = Some(false);
    assert!(service.compile(&paused, &output).is_err(), "nothing selected for an asset-only project");
}
