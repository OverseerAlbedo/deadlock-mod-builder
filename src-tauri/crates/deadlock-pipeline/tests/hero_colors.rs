//! Hero colors end to end on synthetic game files: a project without a
//! replacement model and one recolored hero material builds a VPK holding only
//! that compiled material at its game path, and installs after fresh checks.
use deadlock_core::{
    process::{Runner, RunnerOptions},
    safety, CancellationToken, TimingRecorder,
};
use deadlock_pipeline::{build::BuildService, inspect::ModelPipeline};
use deadlock_project::{
    customization::{HeroMaterial, HeroMaterials},
    Project, Settings,
};
use deadlock_resource::{binary, container, Annotation, Kind, Value};
use deadlock_vpk::Vpk;
use std::{collections::BTreeMap, fs, path::Path};

const HERO: &str = "models/heroes_staging/haze/haze.vmdl";
const BODY: &str = "models/heroes_staging/haze/materials/body.vmat";
const TEXTURE: &str = "models/heroes_staging/haze/materials/body_color_psd_00000000.vtex";

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
        out.extend(*name);
        out.extend(((offsets[i] - (table + i * 12 + 4)) as u32).to_le_bytes());
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
    let mut name_at = 8 + paths.len() * 16;
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
fn named(name: &str, key: &str, value: Value) -> Value {
    Value::new(Kind::Object(vec![
        ("m_name".into(), Value::new(Kind::String(name.into()))),
        (key.into(), value),
    ]))
}
fn numbers(values: [f64; 4]) -> Value {
    Value::new(Kind::Array(values.iter().map(|v| Value::new(Kind::F64(*v))).collect()))
}
fn material() -> Vec<u8> {
    let data = Value::new(Kind::Object(vec![
        ("m_materialName".into(), Value::new(Kind::String(BODY.into()))),
        ("m_shaderName".into(), Value::new(Kind::String("pbr.vfx".into()))),
        ("m_vectorParams".into(), Value::new(Kind::Array(vec![
            named("g_vAlbedoContrastSaturationBrightness1", "m_value", numbers([1., 1.12, 1.21, 0.])),
            named("g_vColorTint1", "m_value", numbers([1., 1., 1., 0.])),
        ]))),
        ("m_textureParams".into(), Value::new(Kind::Array(vec![named(
            "g_tColor",
            "m_pValue",
            Value { annotation: Annotation::Resource, kind: Kind::String(TEXTURE.into()) },
        )]))),
    ]));
    resource(&[
        (b"RERL", references(&[TEXTURE])),
        (b"DATA", binary::encode(&data, [3; 16]).unwrap()),
    ])
}
fn vector(raw: &[u8], name: &str) -> Vec<f64> {
    let layout = container::layout(raw).unwrap();
    let block = layout.blocks.iter().find(|b| b.name == "DATA").unwrap();
    let root = binary::decode(container::payload(raw, block).unwrap()).unwrap().root;
    let row = root.get("m_vectorParams").unwrap().array().unwrap().iter()
        .find(|r| r.get("m_name").unwrap().string().unwrap() == name).unwrap().clone();
    row.get("m_value").unwrap().array().unwrap().iter().map(|v| v.number().unwrap()).collect()
}

#[test]
fn a_project_without_a_replacement_model_builds_and_installs_only_the_recolored_material() {
    let temp = tempfile::tempdir().unwrap();
    let token = CancellationToken::new();
    let clock = TimingRecorder::new("hero-colors");
    let runner = Runner::new(temp.path().join("logs"), token.clone(), RunnerOptions::default()).unwrap();
    let files = BTreeMap::from([
        (format!("{HERO}_c"), resource(&[(b"RERL", references(&[BODY]))])),
        (format!("{BODY}_c"), material()),
        (format!("{TEXTURE}_c"), b"TEXTURE FIXTURE, NOT A GAME FILE".to_vec()),
    ]);
    let source = temp.path().join("game-files");
    for (path, raw) in &files {
        let file = source.join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, raw).unwrap();
    }
    let mut settings = Settings::default();
    settings.workspace = temp.path().join("workspace").display().to_string();
    settings.deadlock = temp.path().join("deadlock").display().to_string();
    let written = temp.path().join("game_dir.vpk");
    deadlock_vpk::write_vpk(&source, &files.keys().cloned().collect::<Vec<_>>(), &written, None, 2, &mut || Ok(())).unwrap();
    let original = Path::new(&settings.deadlock).join("game/citadel/pak01_dir.vpk");
    safety::atomic_copy(&written, &original, &token).unwrap();
    settings.pak = original.display().to_string();
    let mut project = Project::default();
    project.original_model = HERO.into();
    project.install = false;
    project.customization.enabled = Some(true);
    project.customization.model_enabled = Some(false);
    project.customization.hero_materials = Some(HeroMaterials {
        enabled: Some(true),
        materials: Some(vec![HeroMaterial {
            target: Some(format!("{BODY}_c")),
            color: Some([0.05, 0.05, 0.05]),
            brightness: Some(0.5),
            ..Default::default()
        }]),
        ..Default::default()
    });
    let service = BuildService {
        model: ModelPipeline {
            settings: &settings,
            runner: &runner,
            cancel: &token,
            clock: &clock,
            log: &|_| {},
            reader_root: temp.path(),
        },
    };
    let result = service.build(&mut project, false, false).unwrap();
    let output = result.vpk.unwrap();
    let package = Vpk::open(&output).unwrap();
    let target = format!("{BODY}_c");
    assert_eq!(package.entries.keys().collect::<Vec<_>>(), [&target], "only the material, at its game path");
    let built = package.read(&target).unwrap();
    assert_eq!(vector(&built, "g_vColorTint1"), [0.05, 0.05, 0.05, 0.]);
    assert!((vector(&built, "g_vAlbedoContrastSaturationBrightness1")[2] - 0.605).abs() < 1e-9);
    let manifest = service.read_manifest(&project).unwrap();
    assert_eq!(manifest.customization.unwrap().materials.entries.len(), 1);
    let before = safety::digest(&output).unwrap();
    service.install(&mut project, &output, false).unwrap();
    let installed = Path::new(&settings.deadlock).join("game/citadel/addons/pak18_dir.vpk");
    assert_eq!(before, safety::digest(&installed).unwrap());
    // The original game archive is untouched.
    assert_eq!(Vpk::open(&original).unwrap().read(&target).unwrap(), material());
}
