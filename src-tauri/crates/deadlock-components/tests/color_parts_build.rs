//! A hero color whose color range changes one part, through the component
//! build and its checks, on synthetic game files: with the app's switches only
//! the part's pixels change, the receipt records the part's area and the model it
//! came from, and the checks make the same texture again.
use deadlock_components::{CancellationToken, block_refit, service::{ComponentService, HeroMeshes}, texture_recolor};
use deadlock_core::{TimingRecorder, process::{Runner, RunnerOptions}};
use deadlock_project::{
    Project, Settings,
    color_parts::PartMesh,
    color_ranges::{self, ColorPart},
    customization::{HeroColorRange, HeroMaterial, HeroMaterials},
};
use deadlock_vpk::Vpk;
use std::{collections::BTreeMap, fs, path::Path};

const HERO: &str = "models/heroes_staging/sample/sample.vmdl";
const BODY: &str = "models/heroes_staging/sample/materials/body.vmat";
const COLOR: &str = "models/heroes_staging/sample/materials/body_color_psd_00000000.vtex";

fn resource(blocks: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
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
    out
}
fn material() -> Vec<u8> {
    use deadlock_resource::{Annotation, Kind, Value, binary};
    let named = |name: &str, key: &str, value: Value| Value::new(Kind::Object(vec![("m_name".into(), Value::new(Kind::String(name.into()))), (key.into(), value)]));
    let data = Value::new(Kind::Object(vec![
        ("m_materialName".into(), Value::new(Kind::String(BODY.into()))),
        ("m_shaderName".into(), Value::new(Kind::String("pbr.vfx".into()))),
        ("m_vectorParams".into(), Value::new(Kind::Array(vec![named("g_vColorTint1", "m_value", Value::new(Kind::Array([1., 1., 1., 0.].iter().map(|v| Value::new(Kind::F64(*v))).collect())))]))),
        ("m_textureParams".into(), Value::new(Kind::Array(vec![named("g_tColor", "m_pValue", Value { annotation: Annotation::Resource, kind: Kind::String(COLOR.into()) })]))),
    ]));
    let table = texture_recolor::reference_table(&[(deadlock_vpk::resource_id(COLOR), COLOR.to_string())]);
    resource(&[(b"RERL", table), (b"DATA", binary::encode(&data, [3; 16]).unwrap())])
}
/// An 8x8 BC7 texture of one orange (four blocks), one mip, a one-block fallback.
fn texture() -> Vec<u8> {
    let mut template = [0u8; 16];
    template[0] = 1 << 5;
    let block = block_refit::refit_bc7(&template, &[[140, 80, 36, 0]; 16]).unwrap();
    let mut data = vec![0u8; 40];
    data[0..2].copy_from_slice(&1u16.to_le_bytes());
    data[20..22].copy_from_slice(&8u16.to_le_bytes());
    data[22..24].copy_from_slice(&8u16.to_le_bytes());
    data[24..26].copy_from_slice(&1u16.to_le_bytes());
    data[26] = 20;
    data[27] = 1;
    data[32..36].copy_from_slice(&8u32.to_le_bytes());
    data[36..40].copy_from_slice(&1u32.to_le_bytes());
    data.resize(52, 0);
    data[40..44].copy_from_slice(&1u32.to_le_bytes());
    data[44..48].copy_from_slice(&8u32.to_le_bytes());
    data[48..52].copy_from_slice(&16u32.to_le_bytes());
    data.extend([0u8; 16]);
    let mut raw = resource(&[(b"DATA", data)]);
    for _ in 0..4 {
        raw.extend(block);
    }
    raw
}
fn game(root: &Path, model: &[u8]) -> std::path::PathBuf {
    let files = BTreeMap::from([
        (format!("{HERO}_c"), resource(&[(b"RERL", texture_recolor::reference_table(&[(deadlock_vpk::resource_id(BODY), BODY.to_string())])), (b"DATA", model.to_vec())])),
        (format!("{BODY}_c"), material()),
        (format!("{COLOR}_c"), texture()),
    ]);
    let source = root.join("game-files");
    for (path, raw) in &files {
        let file = source.join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, raw).unwrap();
    }
    let output = root.join("game_dir.vpk");
    deadlock_vpk::write_vpk(&source, &files.keys().cloned().collect::<Vec<_>>(), &output, None, 2, &mut || Ok(())).unwrap();
    output
}
/// The hero's "meshes": a quad over the top-left part of the image and a
/// separate quad over the bottom half.
struct Meshes;
impl HeroMeshes for Meshes {
    fn hero_meshes(&self, _: &Project, material: &str) -> Result<Vec<PartMesh>, String> {
        assert_eq!(material, BODY);
        let quad = |x: f32, uv: [f32; 4]| PartMesh {
            positions: vec![x, 0., 0., x + 1., 0., 0., x + 1., 1., 0., x, 1., 0.],
            uvs: vec![uv[0], uv[1], uv[2], uv[1], uv[2], uv[3], uv[0], uv[3]],
            indices: vec![0, 1, 2, 0, 2, 3],
        };
        Ok(vec![quad(0., [0.125, 0.125, 0.5, 0.375]), quad(10., [0.125, 0.625, 0.875, 0.875])])
    }
}
fn project(part: ColorPart) -> Project {
    let mut project = Project::default();
    project.original_model = HERO.into();
    project.customization.enabled = Some(true);
    project.customization.model_enabled = Some(false);
    project.customization.hero_materials = Some(HeroMaterials {
        enabled: Some(true),
        materials: Some(vec![HeroMaterial {
            enabled: Some(true),
            target: Some(format!("{BODY}_c")),
            ranges: Some(vec![HeroColorRange { hue: Some(25.), hue_width: Some(12.), saturation_range: Some([0.5, 1.]), hue_shift: Some(180.), part: Some(part), ..Default::default() }]),
            ..Default::default()
        }]),
        ..Default::default()
    });
    project
}

#[test]
fn a_part_range_recolors_only_its_part_and_is_refused_when_its_switch_is_turned_off() {
    let temp = tempfile::tempdir().unwrap();
    let pak = game(temp.path(), b"model one");
    let cancel = CancellationToken::new();
    let settings = Settings { workspace: temp.path().join("workspace").to_string_lossy().into(), pak: pak.to_string_lossy().into(), ..Default::default() };
    let runner = Runner::new(temp.path().join("logs"), cancel.clone(), RunnerOptions::default()).unwrap();
    let clock = TimingRecorder::new("color parts");
    let log = |_: &str| {};
    let service = ComponentService { settings: &settings, runner: &runner, cancel: &cancel, clock: &clock, log: &log };
    let output = temp.path().join("game");
    let project = project(ColorPart { u: 0.3, v: 0.25 });
    // With the app's switches (both on).
    // Without the reader of the hero's meshes the build refuses rather than recolor everything.
    let unread = service.compile(&project, &output).unwrap_err().to_string();
    assert!(unread.contains("could not be read"), "{unread}");
    let receipt = service.compile_full(&project, &output, None, Some(&Meshes)).unwrap().unwrap();
    let record = receipt.materials.entries[0].texture.as_ref().unwrap();
    let parts = record.parts.as_ref().expect("the part's area is in the receipt");
    assert_eq!((parts.areas.len(), parts.areas[0].pieces, parts.areas[0].triangles.len()), (1, 1, 2));
    let built = fs::read(output.join(&record.target)).unwrap();
    let meta = deadlock_components::icons::texture_metadata(&built).unwrap();
    let block = |i: usize| block_refit::decode_bc7(built[meta.payload_offset + i * 16..meta.payload_offset + i * 16 + 16].try_into().unwrap());
    let blue = |t: [u8; 4]| t[2] > t[0];
    // Pixels 1-3 across and 1-2 down are the part, one pixel of reach around them.
    for (b, (bx, by)) in [(0, (0, 0)), (1, (4, 0)), (2, (0, 4)), (3, (4, 4))] {
        for (t, texel) in block(b).iter().enumerate() {
            let (x, y) = (bx + t % 4, by + t / 4);
            assert_eq!(blue(*texel), x <= 4 && y <= 3, "pixel {x},{y}: {texel:?}");
        }
    }
    // The checks make the same texture again from the receipt.
    service.verify_snapshot(&project, &receipt, &output).unwrap();
    let package = temp.path().join("pak19_dir.vpk");
    deadlock_vpk::write_vpk(&output, &receipt.files.keys().cloned().collect::<Vec<_>>(), &package, Some(&receipt.files), 2, &mut || Ok(())).unwrap();
    service.verify_install(&project, &receipt, &Vpk::open(&package).unwrap(), &Vpk::open(&pak).unwrap()).unwrap();
    // A game update to the hero's model means building again.
    let updated = game(&temp.path().join("updated"), b"model two");
    let moved = Settings { pak: updated.to_string_lossy().into(), ..settings.clone() };
    let service2 = ComponentService { settings: &moved, runner: &runner, cancel: &cancel, clock: &clock, log: &log };
    let changed = service2.verify_snapshot(&project, &receipt, &output).unwrap_err().to_string();
    assert!(changed.contains("hero's model changed"), "{changed}");
    // A spot between the parts: pick the part again.
    let lost = service.compile_full(&project_with(ColorPart { u: 0.75, v: 0.25 }), &output, None, Some(&Meshes)).unwrap_err().to_string();
    assert!(lost.contains("Pick the part again"), "{lost}");
    // With the switch turned off (test builds only) the build refuses the range.
    #[cfg(feature = "object-test-switches")]
    {
        color_ranges::open_for_test(Some(color_ranges::Switches { set_colour: true, part_only: false }));
        let off = service.compile_full(&project, &output, None, Some(&Meshes)).unwrap_err().to_string();
        color_ranges::open_for_test(None);
        assert!(off.contains(color_ranges::PART_ONLY_OFF), "{off}");
    }
}
fn project_with(part: ColorPart) -> Project {
    project(part)
}