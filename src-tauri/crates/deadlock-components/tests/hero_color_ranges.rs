//! Hero color ranges: block refits, the range function, texture recoloring and
//! the asset-only build with its checks, on synthetic blocks, textures and
//! materials only (never game files).
use deadlock_components::{
    CancellationToken,
    block_refit::{self, Texels},
    color_ranges::{self, ColorRange},
    materials,
    service::ComponentService,
    texture_recolor,
};
use deadlock_core::{
    TimingRecorder,
    process::{Runner, RunnerOptions},
};
use deadlock_project::{
    Project, Settings,
    customization::{HeroColorRange, HeroMaterial, HeroMaterials},
};
use deadlock_resource::{Annotation, Kind, Value, binary, container};
use deadlock_vpk::Vpk;
use serde_json::Value as Json;
use std::{collections::BTreeMap, fs, path::Path};

/// Deterministic pseudo-random blocks.
struct Noise(u64);
impl Noise {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn block(&mut self, mode: u32) -> [u8; 16] {
        let bits = ((self.next() as u128) << 64 | self.next() as u128) & !((1u128 << (mode + 1)) - 1);
        (bits | 1 << mode).to_le_bytes()
    }
}
fn set(block: &mut [u8; 16], at: u32, len: u32, value: u32) {
    let mut bits = u128::from_le_bytes(*block);
    let mask = ((1u128 << len) - 1) << at;
    bits = (bits & !mask) | ((value as u128) << at & mask);
    *block = bits.to_le_bytes();
}
fn mode(block: &[u8; 16]) -> u32 {
    u128::from_le_bytes(*block).trailing_zeros()
}
/// The texels with the hue turned by `degrees` (alpha kept).
fn turned(texels: &Texels, degrees: f64) -> Texels {
    let range = ColorRange { hue_shift: degrees, ..ColorRange::default() };
    texels.map(|t| color_ranges::recolor(&[range], t))
}
fn error(a: &Texels, b: &Texels) -> i64 {
    a.iter().zip(b).map(|(p, q)| (0..3).map(|c| (p[c] as i64 - q[c] as i64).pow(2)).sum::<i64>()).sum()
}
fn alpha(texels: &Texels) -> [u8; 16] {
    texels.map(|t| t[3])
}
/// A BC7 block holding `texels` with one alpha everywhere, made by the refit
/// itself from an empty mode 5 block.
fn encode(texels: &Texels) -> [u8; 16] {
    let mut template = [0u8; 16];
    set(&mut template, 5, 1, 1);
    set(&mut template, 50, 8, texels[0][3] as u32);
    set(&mut template, 58, 8, texels[0][3] as u32);
    block_refit::refit_bc7(&template, texels).unwrap()
}
fn solid(rgba: [u8; 4]) -> Texels {
    [rgba; 16]
}

#[test]
fn bc7_refits_keep_alpha_exact_in_every_mode_rotation_and_partition() {
    let mut noise = Noise(0x9e3779b97f4a7c15);
    let mut seen = [0usize; 8];
    for round in 0..4000 {
        let m = (round % 8) as u32;
        let mut block = noise.block(m);
        if m == 7 {
            // Every two-subset partition at least once.
            set(&mut block, 8, 6, (round / 8 % 64) as u32);
        }
        let original = block_refit::decode_bc7(&block);
        let target = turned(&original, 120.);
        let refit = block_refit::refit_bc7(&block, &target).unwrap_or_else(|e| panic!("mode {m}: {e:?}"));
        let decoded = block_refit::decode_bc7(&refit);
        assert_eq!(alpha(&decoded), alpha(&original), "mode {m} round {round}");
        assert!(error(&decoded, &target) <= error(&original, &target), "mode {m} round {round} got worse");
        seen[mode(&refit) as usize] += 1;
        if m >= 4 {
            assert_eq!(mode(&refit), m, "modes with alpha keep their mode");
        } else {
            assert_eq!(mode(&refit), 5, "modes without alpha become mode 5");
            assert!(alpha(&decoded).iter().all(|a| *a == 255));
        }
    }
    assert!(seen[4] > 0 && seen[5] > 0 && seen[6] > 0 && seen[7] > 0);
}

#[test]
fn bc7_refit_of_unchanged_colors_stays_close_and_keeps_fixed_bits() {
    let mut noise = Noise(42);
    for round in 0..2000 {
        let m = 4 + (round % 4) as u32;
        let block = noise.block(m);
        let original = block_refit::decode_bc7(&block);
        let refit = block_refit::refit_bc7(&block, &original).unwrap();
        let decoded = block_refit::decode_bc7(&refit);
        let worst = decoded.iter().zip(&original).flat_map(|(p, q)| (0..3).map(move |c| (p[c] as i32 - q[c] as i32).abs())).max().unwrap();
        assert!(worst <= 4, "mode {m} round {round}: off by {worst}");
        let bits = |b: &[u8; 16], at: u32, len: u32| (u128::from_le_bytes(*b) >> at) & ((1u128 << len) - 1);
        match m {
            // Rotation, index selection, the alpha endpoints and (rotation 0) alpha indices.
            4 => {
                assert_eq!(bits(&refit, 5, 3), bits(&block, 5, 3));
                if bits(&block, 5, 2) == 0 {
                    assert_eq!(bits(&refit, 38, 12), bits(&block, 38, 12));
                }
            }
            5 => {
                assert_eq!(bits(&refit, 6, 2), bits(&block, 6, 2));
                if bits(&block, 6, 2) == 0 {
                    assert_eq!(bits(&refit, 50, 16), bits(&block, 50, 16));
                    assert_eq!(bits(&refit, 97, 31), bits(&block, 97, 31));
                }
            }
            // Alpha endpoints, p-bits and indices.
            6 => {
                assert_eq!(bits(&refit, 49, 14), bits(&block, 49, 14));
                assert_eq!(bits(&refit, 63, 65), bits(&block, 63, 65));
            }
            _ => {
                assert_eq!(bits(&refit, 8, 6), bits(&block, 8, 6));
                assert_eq!(bits(&refit, 74, 54), bits(&block, 74, 54));
            }
        }
    }
}

#[test]
fn bc7_refit_follows_a_recolor_closely() {
    // Two colors split across the block, like a clothing edge: only one turns.
    let mut texels = [[0u8; 4]; 16];
    for (t, texel) in texels.iter_mut().enumerate() {
        *texel = if t % 4 < 2 { [140, 80, 36, 0] } else { [181, 160, 148, 0] };
    }
    let block = encode(&texels);
    let range = ColorRange { hue: 25., hue_width: 10., saturation_range: [0.5, 1.], hue_shift: 180., ..ColorRange::default() };
    let target = block_refit::decode_bc7(&block).map(|t| color_ranges::recolor(&[range], t));
    let decoded = block_refit::decode_bc7(&block_refit::refit_bc7(&block, &target).unwrap());
    let worst = decoded.iter().zip(&target).flat_map(|(p, q)| (0..3).map(move |c| (p[c] as i32 - q[c] as i32).abs())).max().unwrap();
    assert!(worst <= 6, "off by {worst}");
    assert!(decoded[0][2] > decoded[0][0], "the orange half turned blue");
    assert!((decoded[3][0] as i32 - 181).abs() <= 4, "the beige half stayed");
}

#[test]
fn reserved_bc7_blocks_and_bc3_blocks_keep_their_alpha() {
    let reserved = [0u8; 16];
    let target = solid([10, 200, 30, 0]);
    let refit = block_refit::refit_bc7(&reserved, &target).unwrap();
    assert_eq!(alpha(&block_refit::decode_bc7(&refit)), [0; 16]);

    let mut noise = Noise(7);
    for _ in 0..500 {
        let mut block = [0u8; 16];
        block.copy_from_slice(&(noise.next() as u128 | (noise.next() as u128) << 64).to_le_bytes());
        let original = block_refit::decode_bc3(&block);
        let target = turned(&original, -90.);
        let refit = block_refit::refit_bc3(&block, &target).unwrap();
        assert_eq!(refit[..8], block[..8], "the alpha half stays byte for byte");
        let decoded = block_refit::decode_bc3(&refit);
        assert_eq!(alpha(&decoded), alpha(&original));
        assert!(error(&decoded, &target) <= error(&original, &target));
    }
}

#[test]
fn ranges_select_by_hue_saturation_and_brightness_with_a_soft_edge() {
    let orange = ColorRange { hue: 25., hue_width: 10., saturation_range: [0.5, 1.], brightness_range: [0.2, 1.], softness: 0.5, hue_shift: 180., ..ColorRange::default() };
    let pick = |rgb: [u8; 3]| orange.weight(color_ranges::hsv(rgb.map(|v| v as f64 / 255.)));
    assert_eq!(pick([140, 80, 36]), 1., "orange clothes");
    assert_eq!(pick([181, 160, 148]), 0., "beige skin has the hue but not the saturation");
    assert_eq!(pick([30, 30, 36]), 0., "navy");
    let edge = pick([140, 95, 75]);
    assert!(edge > 0. && edge < 1., "just outside the saturation range fades: {edge}");
    // Hue wraps around red.
    let red = ColorRange { hue: 350., hue_width: 15., hue_shift: 90., ..ColorRange::default() };
    assert_eq!(red.weight([5., 1., 1.]), 1.);
    assert_eq!(red.weight([180., 1., 1.]), 0.);
    // Every hue, a hard edge.
    let dark = ColorRange { brightness_range: [0., 0.3], softness: 0., brightness: 0., ..ColorRange::default() };
    assert_eq!(color_ranges::recolor(&[dark], [40, 40, 60, 7]), [0, 0, 0, 7]);
    assert_eq!(color_ranges::recolor(&[dark], [200, 40, 60, 7]), [200, 40, 60, 7]);
    // Validation and no-ops.
    assert!(orange.validate().is_ok());
    assert!(ColorRange { hue_width: 181., ..orange }.validate().is_err());
    assert!(ColorRange { saturation_range: [0.8, 0.2], ..orange }.validate().is_err());
    assert!(ColorRange { brightness: f64::NAN, ..orange }.validate().is_err());
    assert!(ColorRange::default().changes_nothing());
    assert!(!ColorRange { hue_shift: -180., ..ColorRange::default() }.changes_nothing());
}

#[test]
fn ranges_match_the_shared_fixture() {
    let fixture: Json = serde_json::from_str(include_str!("fixtures/hero-color-ranges.json")).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let ranges: Vec<ColorRange> = serde_json::from_value(case["ranges"].clone()).unwrap();
        for (color, expected) in case["colors"].as_array().unwrap().iter().zip(case["expected"].as_array().unwrap()) {
            let color: [u8; 4] = serde_json::from_value(color.clone()).unwrap();
            let expected: [u8; 4] = serde_json::from_value(expected.clone()).unwrap();
            assert_eq!(color_ranges::recolor(&ranges, color), expected, "{} {color:?}", case["name"]);
        }
    }
}

/// A compiled texture: DATA with the header, a fallback (kind 1, BC3) and a
/// display rectangle (kind 3), then the mips smallest first.
fn texture(format: u8, flags: u16, size: u16, mips: &[Vec<[u8; 16]>], fallback: &[[u8; 16]]) -> Vec<u8> {
    let mut data = vec![0u8; 40];
    data[0..2].copy_from_slice(&1u16.to_le_bytes());
    data[2..4].copy_from_slice(&flags.to_le_bytes());
    for (i, v) in [0.25f32, 0.2, 0.1, 0.].iter().enumerate() {
        data[4 + i * 4..8 + i * 4].copy_from_slice(&v.to_le_bytes());
    }
    data[20..22].copy_from_slice(&size.to_le_bytes());
    data[22..24].copy_from_slice(&size.to_le_bytes());
    data[24..26].copy_from_slice(&1u16.to_le_bytes());
    data[26] = format;
    data[27] = mips.len() as u8;
    data[32..36].copy_from_slice(&8u32.to_le_bytes());
    data[36..40].copy_from_slice(&2u32.to_le_bytes());
    let mut rect = vec![0u8; 40];
    rect[2..4].copy_from_slice(&size.to_le_bytes());
    rect[4..6].copy_from_slice(&size.to_le_bytes());
    let fallback_bytes: Vec<u8> = fallback.concat();
    let items = [(1u32, fallback_bytes), (3u32, rect)];
    data.resize(40 + items.len() * 12, 0);
    for (i, (kind, bytes)) in items.iter().enumerate() {
        let pos = 40 + i * 12;
        let offset = data.len() - pos - 4;
        data[pos..pos + 4].copy_from_slice(&kind.to_le_bytes());
        data[pos + 4..pos + 8].copy_from_slice(&(offset as u32).to_le_bytes());
        data[pos + 8..pos + 12].copy_from_slice(&(bytes.len() as u32).to_le_bytes());
        data.extend_from_slice(bytes);
    }
    let mut raw = vec![];
    raw.extend(((28 + data.len()) as u32).to_le_bytes());
    raw.extend(12u16.to_le_bytes());
    raw.extend(0u16.to_le_bytes());
    raw.extend(8u32.to_le_bytes());
    raw.extend(1u32.to_le_bytes());
    raw.extend(b"DATA");
    raw.extend(8u32.to_le_bytes());
    raw.extend((data.len() as u32).to_le_bytes());
    raw.extend(&data);
    for mip in mips.iter().rev() {
        raw.extend(mip.concat());
    }
    raw
}

#[test]
fn textures_recolor_only_blocks_in_range_and_keep_everything_else() {
    let orange = solid([140, 80, 36, 0]);
    let beige = solid([181, 160, 148, 0]);
    let metal = solid([150, 82, 40, 255]);
    // 8x8 top mip (four blocks), 4x4 second mip.
    let top = vec![encode(&orange), encode(&beige), encode(&metal), encode(&beige)];
    let small = vec![encode(&orange)];
    let mut fallback = [0u8; 16];
    fallback[..8].copy_from_slice(&[255, 255, 0, 0, 0, 0, 0, 0]);
    fallback[8..10].copy_from_slice(&0x8a24u16.to_le_bytes());
    fallback[10..12].copy_from_slice(&0x8a24u16.to_le_bytes());
    let raw = texture(20, 0, 8, &[top.clone(), small], &[fallback]);
    let before = deadlock_components::icons::texture_metadata(&raw).unwrap();
    assert_eq!(texture_recolor::unsupported(&raw), None);

    let range = ColorRange { hue: 25., hue_width: 12., saturation_range: [0.5, 1.], hue_shift: 180., ..ColorRange::default() };
    let result = texture_recolor::recolor_texture(&raw, &[range], &CancellationToken::new()).unwrap();
    let out = &result.bytes;
    assert_eq!(out.len(), raw.len());
    assert_eq!(deadlock_components::icons::texture_metadata(out).unwrap(), before);
    assert_eq!((result.blocks, result.changed_blocks), (6, 4), "orange, metal, the small mip and the fallback");
    let payload = before.payload_offset;
    let block = |raw: &[u8], i: usize| -> [u8; 16] { raw[payload + i * 16..payload + i * 16 + 16].try_into().unwrap() };
    // Stored smallest first: the 4x4 mip, then the four top blocks.
    assert_ne!(block(out, 0), block(&raw, 0));
    assert_ne!(block(out, 1), block(&raw, 1), "orange turned");
    assert_eq!(block(out, 2), block(&raw, 2), "beige kept byte for byte");
    assert_eq!(block(out, 4), block(&raw, 4));
    for i in 0..5 {
        let (a, b) = (block_refit::decode_bc7(&block(&raw, i)), block_refit::decode_bc7(&block(out, i)));
        assert_eq!(alpha(&a), alpha(&b), "block {i} alpha");
    }
    let turned = block_refit::decode_bc7(&block(out, 1));
    assert!(turned[0][2] > turned[0][0], "orange became blue: {:?}", turned[0]);
    let metal_after = block_refit::decode_bc7(&block(out, 3));
    assert!(metal_after[0][2] > metal_after[0][0] && metal_after[0][3] == 255, "metal parts recolor too, alpha kept");
    // The fallback keeps its alpha half and turns blue.
    let fb_at = 28 + 40 + 24;
    assert_eq!(out[fb_at..fb_at + 8], raw[fb_at..fb_at + 8]);
    let fb = block_refit::decode_bc3(out[fb_at..fb_at + 16].try_into().unwrap());
    assert!(fb[0][2] > fb[0][0], "fallback recolored: {:?}", fb[0]);
    // Reflectivity follows the color: less red, more blue; alpha untouched.
    let float = |raw: &[u8], i: usize| f32::from_le_bytes(raw[28 + 4 + i * 4..28 + 8 + i * 4].try_into().unwrap());
    assert!(float(out, 0) < float(&raw, 0) && float(out, 2) > float(&raw, 2));
    assert_eq!(float(out, 3), float(&raw, 3));
    // Nothing else moved: the header up to the floats and after them, and the display rectangle.
    assert_eq!(out[..32], raw[..32]);
    assert_eq!(out[44..fb_at], raw[44..fb_at]);
    assert_eq!(out[fb_at + 16..payload], raw[fb_at + 16..payload]);

    // A range that selects nothing leaves the texture's bytes as they were.
    let none = ColorRange { hue: 200., hue_width: 5., saturation_range: [0.9, 1.], hue_shift: 30., ..ColorRange::default() };
    let same = texture_recolor::recolor_texture(&raw, &[none], &CancellationToken::new()).unwrap();
    assert_eq!(same.changed_blocks, 0);
    assert_eq!(same.bytes, raw);
}

#[test]
fn textures_that_cannot_keep_their_data_are_refused() {
    let top = vec![encode(&solid([140, 80, 36, 0]))];
    assert!(texture_recolor::unsupported(&texture(20, 0, 4, &[top.clone()], &[])).is_none());
    assert!(texture_recolor::unsupported(&texture(2, 0, 4, &[top.clone()], &[])).unwrap().contains("BC7"));
    assert!(texture_recolor::unsupported(&texture(20, 16, 4, &[top.clone()], &[])).unwrap().contains("cube"));
    let mut truncated = texture(20, 0, 4, &[top], &[]);
    truncated.pop();
    assert!(texture_recolor::unsupported(&truncated).is_some());
    let range = ColorRange { hue_shift: 30., ..ColorRange::default() };
    assert!(texture_recolor::recolor_texture(&truncated, &[range], &CancellationToken::new()).is_err());
}

#[test]
fn recolored_texture_paths_stay_in_their_own_folder() {
    let path = texture_recolor::texture_path(
        "models/heroes_staging/haze/materials/haze_v2_color_psd_c9873321.vtex_c",
        "0123456789abcdef0123456789abcdef",
    )
    .unwrap();
    assert_eq!(path, "materials/dmb_hero_colors/haze_v2_color_psd_c9873321_0123456789abcdef.vtex");
    assert!(texture_recolor::texture_path("a/B C.vtex", "xyz").is_err());
    assert_eq!(texture_recolor::texture_path("a/B C.vtex", "ffffffffffffffff").unwrap(), "materials/dmb_hero_colors/b_c_ffffffffffffffff.vtex");
}

#[test]
fn reference_tables_move_a_texture_in_name_order() {
    let name = |s: &str| (deadlock_vpk::resource_id(s), s.to_owned());
    let entries = vec![name("materials/default/a.vtex"), name("models/x/color.vtex"), name("models/x/normal.vtex")];
    let payload = texture_recolor::reference_table(&entries);
    assert_eq!(texture_recolor::references(&payload).unwrap(), entries);
    let new = "materials/dmb_hero_colors/color_0123456789abcdef.vtex";
    let moved = texture_recolor::moved_references(&entries, "models/x/color.vtex", new, &[]);
    assert_eq!(moved, vec![name("materials/default/a.vtex"), name(new), name("models/x/normal.vtex")]);
    // A texture another parameter still names stays.
    let kept = texture_recolor::moved_references(&entries, "models/x/color.vtex", new, &["models/x/color.vtex".into()]);
    assert_eq!(kept.len(), 4);
    assert_eq!(deadlock_vpk::resource_id("animation/skeletons/characters/viewmodel.vnmskel"), 0x9174afe96ca9ada4);
}

const HERO: &str = "models/heroes_staging/haze/haze.vmdl";
const BODY: &str = "models/heroes_staging/haze/materials/body.vmat";
const COLOR: &str = "models/heroes_staging/haze/materials/body_color_psd_00000000.vtex";
const NORMAL: &str = "models/heroes_staging/haze/materials/body_normal_00000000.vtex";

/// A compiled resource from embedded blocks (16-byte aligned).
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
fn named(name: &str, key: &str, value: Value) -> Value {
    Value::new(Kind::Object(vec![("m_name".into(), Value::new(Kind::String(name.into()))), (key.into(), value)]))
}
fn texture_value(path: &str) -> Value {
    Value { annotation: Annotation::Resource, kind: Kind::String(path.into()) }
}
fn body_material() -> Vec<u8> {
    let numbers = |v: [f64; 4]| Value::new(Kind::Array(v.iter().map(|x| Value::new(Kind::F64(*x))).collect()));
    let data = Value::new(Kind::Object(vec![
        ("m_materialName".into(), Value::new(Kind::String(BODY.into()))),
        ("m_shaderName".into(), Value::new(Kind::String("pbr.vfx".into()))),
        ("m_vectorParams".into(), Value::new(Kind::Array(vec![
            named("g_vAlbedoContrastSaturationBrightness1", "m_value", numbers([1., 1.12, 1.21, 0.])),
            named("g_vColorTint1", "m_value", numbers([1., 1., 1., 0.])),
        ]))),
        ("m_textureParams".into(), Value::new(Kind::Array(vec![
            named("g_tColor", "m_pValue", texture_value(COLOR)),
            named("g_tNormalRoughness", "m_pValue", texture_value(NORMAL)),
        ]))),
    ]));
    let entries: Vec<(u64, String)> = [COLOR, NORMAL].iter().map(|n| (deadlock_vpk::resource_id(n), n.to_string())).collect();
    resource(&[
        (b"RERL", texture_recolor::reference_table(&entries)),
        (b"RED2", b"opaque edit information".to_vec()),
        (b"DATA", binary::encode(&data, [5; 16]).unwrap()),
        (b"INSG", b"opaque input signature".to_vec()),
    ])
}
/// Orange clothes and beige skin side by side, alpha (metalness) 0.
fn body_texture() -> Vec<u8> {
    let orange = encode(&solid([140, 80, 36, 0]));
    let beige = encode(&solid([181, 160, 148, 0]));
    let mut fallback = [0u8; 16];
    fallback[8..12].copy_from_slice(&[0x24, 0x8a, 0x24, 0x8a]);
    texture(20, 0, 8, &[vec![orange, beige, orange, beige], vec![orange]], &[fallback])
}
fn game(root: &Path, color: Vec<u8>) -> std::path::PathBuf {
    let files = BTreeMap::from([
        (format!("{HERO}_c"), resource(&[(b"RERL", texture_recolor::reference_table(&[(deadlock_vpk::resource_id(BODY), BODY.to_string())]))])),
        (format!("{BODY}_c"), body_material()),
        (format!("{COLOR}_c"), color),
        (format!("{NORMAL}_c"), b"NORMAL FIXTURE, NOT A GAME FILE".to_vec()),
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
fn orange_to_blue() -> HeroColorRange {
    HeroColorRange {
        enabled: Some(true),
        hue: Some(25.),
        hue_width: Some(12.),
        saturation_range: Some([0.5, 1.]),
        hue_shift: Some(180.),
        ..Default::default()
    }
}
fn ranged(ranges: Vec<HeroColorRange>) -> Project {
    let mut project = Project::default();
    project.original_model = HERO.into();
    project.customization.enabled = Some(true);
    project.customization.model_enabled = Some(false);
    project.customization.hero_materials = Some(HeroMaterials {
        enabled: Some(true),
        materials: Some(vec![HeroMaterial { enabled: Some(true), target: Some(format!("{BODY}_c")), ranges: Some(ranges), ..Default::default() }]),
        ..Default::default()
    });
    project
}
fn data_root(raw: &[u8]) -> Value {
    let layout = container::layout(raw).unwrap();
    binary::decode(container::payload(raw, container::unique(&layout.blocks, "DATA").unwrap()).unwrap()).unwrap().root
}
fn service_parts(root: &Path, pak: &Path) -> (Settings, CancellationToken, TimingRecorder, Runner) {
    let cancel = CancellationToken::new();
    let settings = Settings { workspace: root.join("workspace").to_string_lossy().into(), pak: pak.to_string_lossy().into(), ..Default::default() };
    let runner = Runner::new(root.join("logs"), cancel.clone(), RunnerOptions::default()).unwrap();
    (settings, cancel, TimingRecorder::new("hero color ranges"), runner)
}

#[test]
fn a_range_build_packages_the_material_and_its_recolored_texture_and_checks_both() {
    let temp = tempfile::tempdir().unwrap();
    let pak = game(temp.path(), body_texture());
    let (settings, cancel, clock, runner) = service_parts(temp.path(), &pak);
    let log = |_: &str| {};
    let service = ComponentService { settings: &settings, runner: &runner, cancel: &cancel, clock: &clock, log: &log };
    let mut project = ranged(vec![orange_to_blue()]);
    let output = temp.path().join("game");
    let receipt = service.compile(&project, &output).unwrap().unwrap();

    let entry = &receipt.materials.entries[0];
    let texture = entry.texture.as_ref().expect("a texture record");
    assert_eq!(texture.source, format!("{COLOR}_c"));
    assert!(texture.target.starts_with("materials/dmb_hero_colors/body_color_psd_00000000_") && texture.target.ends_with(".vtex_c"), "{}", texture.target);
    assert_eq!(texture.changed_blocks, 4, "two orange top blocks, the small mip and the fallback");
    let material_target = format!("{BODY}_c");
    let mut keys: Vec<_> = receipt.files.keys().cloned().collect();
    keys.sort();
    assert_eq!(keys, [texture.target.clone(), material_target.clone()]);

    // The material names the new texture for g_tColor and in its reference table;
    // the old color texture leaves the table, the normal map stays.
    let built = fs::read(output.join(&material_target)).unwrap();
    let new_path = texture.target.strip_suffix("_c").unwrap();
    let rows = data_root(&built).get("m_textureParams").unwrap().array().unwrap().to_vec();
    let color = rows.iter().find(|r| r.get("m_name").unwrap().string().unwrap() == "g_tColor").unwrap();
    assert_eq!(color.get("m_pValue").unwrap().string().unwrap(), new_path);
    assert_eq!(color.get("m_pValue").unwrap().annotation, Annotation::Resource);
    let layout = container::layout(&built).unwrap();
    let rerl = texture_recolor::references(container::payload(&built, container::unique(&layout.blocks, "RERL").unwrap()).unwrap()).unwrap();
    assert_eq!(rerl, vec![(deadlock_vpk::resource_id(new_path), new_path.to_string()), (deadlock_vpk::resource_id(NORMAL), NORMAL.to_string())]);
    let original = body_material();
    let before = container::layout(&original).unwrap();
    for (a, b) in before.blocks.iter().zip(&layout.blocks).filter(|(a, _)| !["DATA", "RERL"].contains(&a.name.as_str())) {
        assert_eq!(container::payload(&original, a).unwrap(), container::payload(&built, b).unwrap(), "{} stays", a.name);
    }

    // The texture: orange turned blue, beige kept, alpha kept.
    let recolored = fs::read(output.join(&texture.target)).unwrap();
    let meta = deadlock_components::icons::texture_metadata(&recolored).unwrap();
    let block = |raw: &[u8], i: usize| -> [u8; 16] { raw[meta.payload_offset + i * 16..meta.payload_offset + i * 16 + 16].try_into().unwrap() };
    let game_texture = body_texture();
    assert_eq!(block(&recolored, 2), block(&game_texture, 2), "beige kept byte for byte");
    let turned = block_refit::decode_bc7(&block(&recolored, 1));
    assert!(turned[0][2] > turned[0][0] && turned[0][3] == 0, "{:?}", turned[0]);

    // Snapshot and install checks pass, then fail for changed choices and files.
    service.verify_snapshot(&project, &receipt, &output).unwrap();
    let package = temp.path().join("pak19_dir.vpk");
    deadlock_vpk::write_vpk(&output, &receipt.files.keys().cloned().collect::<Vec<_>>(), &package, Some(&receipt.files), 2, &mut || Ok(())).unwrap();
    let candidate = Vpk::open(&package).unwrap();
    let current = Vpk::open(&pak).unwrap();
    service.verify_install(&project, &receipt, &candidate, &current).unwrap();
    let mut wider = orange_to_blue();
    wider.hue_width = Some(20.);
    project = ranged(vec![wider]);
    assert!(service.verify_snapshot(&project, &receipt, &output).unwrap_err().to_string().contains("Hero colors changed"));
    assert!(service.verify_install(&project, &receipt, &candidate, &current).is_err());
    project = ranged(vec![orange_to_blue()]);
    let mut tampered = recolored.clone();
    tampered[meta.payload_offset + 16 + 5] ^= 1;
    fs::write(output.join(&texture.target), &tampered).unwrap();
    assert!(service.verify_snapshot(&project, &receipt, &output).is_err(), "a changed texture is refused");
    fs::write(output.join(&texture.target), &recolored).unwrap();
    service.verify_snapshot(&project, &receipt, &output).unwrap();

    // A game update to the texture alone needs a new build.
    let mut updated = body_texture();
    let last = updated.len() - 3;
    updated[last] ^= 0x40;
    let pak = game(&temp.path().join("updated"), updated);
    let settings = Settings { pak: pak.to_string_lossy().into(), ..settings.clone() };
    let service = ComponentService { settings: &settings, runner: &runner, cancel: &cancel, clock: &clock, log: &log };
    assert!(service.verify_snapshot(&project, &receipt, &output).is_err(), "the game texture changed");
}

#[test]
fn range_builds_refuse_ranges_that_select_nothing_and_keep_tint_edits() {
    let temp = tempfile::tempdir().unwrap();
    let pak = game(temp.path(), body_texture());
    let (settings, cancel, clock, runner) = service_parts(temp.path(), &pak);
    let log = |_: &str| {};
    let service = ComponentService { settings: &settings, runner: &runner, cancel: &cancel, clock: &clock, log: &log };
    let output = temp.path().join("game");
    let nothing = HeroColorRange { hue: Some(200.), hue_width: Some(5.), saturation_range: Some([0.9, 1.]), hue_shift: Some(40.), ..Default::default() };
    let error = service.compile(&ranged(vec![nothing]), &output).unwrap_err().to_string();
    assert!(error.contains("select none of its base color"), "{error}");
    // A range that changes nothing is ignored; with nothing else the color changes nothing.
    let idle = HeroColorRange { hue_shift: Some(0.), ..Default::default() };
    let error = service.compile(&ranged(vec![idle]), &output).unwrap_err().to_string();
    assert!(error.contains("changes nothing"), "{error}");
    // Tint and a range together: both in the one material.
    let mut project = ranged(vec![orange_to_blue()]);
    project.customization.hero_materials.as_mut().unwrap().materials.as_mut().unwrap()[0].color = Some([0.5, 0.5, 0.5]);
    let receipt = service.compile(&project, &output).unwrap().unwrap();
    let built = fs::read(output.join(format!("{BODY}_c"))).unwrap();
    let tint: Vec<f64> = data_root(&built).get("m_vectorParams").unwrap().array().unwrap().iter()
        .find(|r| r.get("m_name").unwrap().string().unwrap() == "g_vColorTint1").unwrap().get("m_value").unwrap().array().unwrap()
        .iter().map(|v| v.number().unwrap()).collect();
    assert_eq!(tint, [0.5, 0.5, 0.5, 0.]);
    assert!(receipt.materials.entries[0].texture.is_some());
    let rows = project.customization.hero_materials.as_ref().unwrap().materials.clone().unwrap();
    assert!(materials::same_choices(&rows, &receipt.materials));
}
