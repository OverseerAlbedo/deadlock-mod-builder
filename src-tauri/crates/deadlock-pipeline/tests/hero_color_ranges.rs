//! Hero color ranges end to end on synthetic game files: a project without a
//! replacement model and a color range on one hero material builds a VPK
//! holding that material at its game path and its recolored texture in the
//! hero colors folder, and installs after fresh checks.
use deadlock_components::{block_refit, texture_recolor};
use deadlock_core::{
    process::{Runner, RunnerOptions},
    safety, CancellationToken, TimingRecorder,
};
use deadlock_pipeline::{build::BuildService, inspect::ModelPipeline};
use deadlock_project::{
    customization::{HeroColorRange, HeroMaterial, HeroMaterials},
    Project, Settings,
};
use deadlock_resource::{binary, Annotation, Kind, Value};
use deadlock_vpk::Vpk;
use std::{collections::BTreeMap, fs, path::Path};

const HERO: &str = "models/heroes_staging/haze/haze.vmdl";
const BODY: &str = "models/heroes_staging/haze/materials/body.vmat";
const TEXTURE: &str = "models/heroes_staging/haze/materials/body_color_psd_00000000.vtex";

fn resource(blocks: &[(&[u8; 4], Vec<u8>)], trailing: &[u8]) -> Vec<u8> {
    let mut offsets = vec![];
    let mut at = 16 + blocks.len() * 12;
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
        out.extend(((offsets[i] - (16 + i * 12 + 4)) as u32).to_le_bytes());
        out.extend((payload.len() as u32).to_le_bytes());
    }
    for (i, (_, payload)) in blocks.iter().enumerate() {
        out.resize(offsets[i], 0);
        out.extend(payload);
    }
    out.extend(trailing);
    out
}
fn references(paths: &[&str]) -> Vec<u8> {
    texture_recolor::reference_table(&paths.iter().map(|p| (deadlock_vpk::resource_id(p), p.to_string())).collect::<Vec<_>>())
}
fn named(name: &str, key: &str, value: Value) -> Value {
    Value::new(Kind::Object(vec![
        ("m_name".into(), Value::new(Kind::String(name.into()))),
        (key.into(), value),
    ]))
}
fn material() -> Vec<u8> {
    let numbers = Value::new(Kind::Array([1., 1., 1., 0.].iter().map(|v| Value::new(Kind::F64(*v))).collect()));
    let data = Value::new(Kind::Object(vec![
        ("m_materialName".into(), Value::new(Kind::String(BODY.into()))),
        ("m_shaderName".into(), Value::new(Kind::String("pbr.vfx".into()))),
        ("m_vectorParams".into(), Value::new(Kind::Array(vec![named("g_vColorTint1", "m_value", numbers)]))),
        ("m_textureParams".into(), Value::new(Kind::Array(vec![named(
            "g_tColor",
            "m_pValue",
            Value { annotation: Annotation::Resource, kind: Kind::String(TEXTURE.into()) },
        )]))),
    ]));
    resource(&[(b"RERL", references(&[TEXTURE])), (b"DATA", binary::encode(&data, [3; 16]).unwrap())], &[])
}
/// A 4x4 BC7 texture, one mip, half orange half beige with alpha 0 (made by
/// the refit from an empty mode 5 block).
fn texture() -> Vec<u8> {
    let mut texels = [[181u8, 160, 148, 0]; 16];
    for (t, texel) in texels.iter_mut().enumerate() {
        if t % 4 < 2 {
            *texel = [140, 80, 36, 0];
        }
    }
    let mut template = [0u8; 16];
    template[0] = 1 << 5;
    let pixels = block_refit::refit_bc7(&template, &texels).unwrap();
    let mut data = vec![0u8; 40];
    data[0..2].copy_from_slice(&1u16.to_le_bytes());
    data[20..22].copy_from_slice(&4u16.to_le_bytes());
    data[22..24].copy_from_slice(&4u16.to_le_bytes());
    data[24..26].copy_from_slice(&1u16.to_le_bytes());
    data[26] = 20;
    data[27] = 1;
    resource(&[(b"DATA", data)], &pixels)
}

#[test]
fn a_project_without_a_replacement_model_builds_and_installs_a_material_with_its_recolored_texture() {
    let temp = tempfile::tempdir().unwrap();
    let token = CancellationToken::new();
    let clock = TimingRecorder::new("hero-color-ranges");
    let runner = Runner::new(temp.path().join("logs"), token.clone(), RunnerOptions::default()).unwrap();
    let files = BTreeMap::from([
        (format!("{HERO}_c"), resource(&[(b"RERL", references(&[BODY]))], &[])),
        (format!("{BODY}_c"), material()),
        (format!("{TEXTURE}_c"), texture()),
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
            ranges: Some(vec![HeroColorRange {
                hue: Some(25.),
                hue_width: Some(12.),
                saturation_range: Some([0.5, 1.]),
                hue_shift: Some(180.),
                ..Default::default()
            }]),
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
    let names: Vec<_> = package.entries.values().map(|e| e.path.clone()).collect();
    assert_eq!(names.len(), 2, "{names:?}");
    let texture_path = names.iter().find(|p| p.starts_with("materials/dmb_hero_colors/")).expect("the recolored texture");
    assert!(names.contains(&format!("{BODY}_c")));
    let recolored = package.read(texture_path).unwrap();
    let pixels: [u8; 16] = recolored[recolored.len() - 16..].try_into().unwrap();
    let texels = block_refit::decode_bc7(&pixels);
    assert!(texels[0][2] > texels[0][0], "orange turned blue: {:?}", texels[0]);
    assert!((texels[3][0] as i32 - 181).abs() <= 4, "beige stayed: {:?}", texels[3]);
    assert!(texels.iter().all(|t| t[3] == 0), "alpha kept");
    assert_eq!(
        deadlock_vpk::resource_references(&package.read(&format!("{BODY}_c")).unwrap()).unwrap(),
        [texture_path.strip_suffix("_c").unwrap()]
    );
    let manifest = service.read_manifest(&project).unwrap();
    assert!(manifest.customization.unwrap().materials.entries[0].texture.is_some());
    let before = safety::digest(&output).unwrap();
    service.install(&mut project, &output, false).unwrap();
    let installed = Path::new(&settings.deadlock).join("game/citadel/addons/pak18_dir.vpk");
    assert_eq!(before, safety::digest(&installed).unwrap());
    assert_eq!(Vpk::open(&original).unwrap().read(&format!("{TEXTURE}_c")).unwrap(), texture(), "the game archive is untouched");
}
