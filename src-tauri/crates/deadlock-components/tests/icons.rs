use deadlock_components::icons;
use deadlock_core::{CancellationToken, CoreError, safety};
use deadlock_project::customization::ResourceReplacement;
use deadlock_vpk::{Vpk, write_vpk};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
};
const HERO: &str = "models/heroes_staging/hornet_v3/hornet.vmdl_c";
const TARGET: &str = "panorama/images/heroes/vindicta_png.vtex_c";
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/icons")
        .join(name)
}
fn bytes(name: &str) -> Vec<u8> {
    fs::read(fixture(name)).unwrap()
}
fn archive(root: &Path, files: &[(&str, Vec<u8>)]) -> Vpk {
    let tree = root.join("archive-source");
    let mut paths = vec![];
    for (path, raw) in files {
        safety::atomic_write(&tree.join(path), raw).unwrap();
        paths.push((*path).to_string());
    }
    let path = root.join("pak18_dir.vpk");
    write_vpk(&tree, &paths, &path, None, 2, &mut || Ok(())).unwrap();
    Vpk::open(path).unwrap()
}
fn selection(source: &Path) -> ResourceReplacement {
    ResourceReplacement {
        source: Some(source.display().to_string()),
        target: Some(TARGET.into()),
        ..Default::default()
    }
}

#[test]
fn texture_metadata_and_qualification_match_python_reference() {
    let token = CancellationToken::new();
    let expected: Value = serde_json::from_slice(&bytes("textures.json")).unwrap();
    for (name, case) in expected.as_object().unwrap() {
        let raw = bytes(&format!("{name}.vtex_c"));
        let value = icons::texture_metadata(&raw);
        // The legacy reference intentionally refused these layouts. Alpha.47
        // preserves their dimensions through the new Panorama storage path.
        if ["unknown_flags", "gpu_padded"].contains(&name.as_str()) {
            assert_eq!(icons::method(&value.unwrap(), &raw, &token).unwrap(), "panorama_texture");
            continue;
        }
        if case["rejected"] == true {
            assert!(
                value
                    .and_then(|meta| icons::method(&meta, &raw, &token))
                    .is_err(),
                "{name}"
            );
        } else {
            let meta = value.unwrap();
            assert_eq!(
                serde_json::to_value(&meta).unwrap(),
                case["metadata"],
                "{name}"
            );
            assert_eq!(
                icons::method(&meta, &raw, &token).unwrap(),
                case["method"].as_str().unwrap()
            );
        }
    }
    let meta = icons::texture_metadata(&bytes("gpu.vtex_c")).unwrap();
    assert_eq!(
        icons::texture_source("panorama/images/dmb_sources/example.png", &meta).unwrap(),
        String::from_utf8(bytes("recipe.vtex")).unwrap()
    );
}

#[test]
fn image_decode_orientation_color_profile_and_alpha_match_python() {
    let token = CancellationToken::new();
    let expected: Value = serde_json::from_slice(&bytes("images.json")).unwrap();
    for (name, case) in expected.as_object().unwrap() {
        let result = icons::decode_image(&bytes(name), &token);
        if case["rejected"] == true {
            assert!(result.is_err(), "{name}");
            continue;
        }
        let (image, info) = result.unwrap_or_else(|e| panic!("{name}: {e}"));
        let value = serde_json::to_value(info).unwrap();
        for key in [
            "width",
            "height",
            "format",
            "has_alpha",
            "color_space",
            "profile_converted",
        ] {
            assert_eq!(value[key], case[key], "{name}/{key}");
        }
        for (pixel, key) in [
            (image.get_pixel(0, 0), "first_pixel"),
            (
                image.get_pixel(image.width() - 1, image.height() - 1),
                "last_pixel",
            ),
        ] {
            let want: Vec<u8> = serde_json::from_value(case[key].clone()).unwrap();
            // Distinct native JPEG/ICC decoders can round one color code differently.
            for (actual, expected) in pixel.0.iter().zip(want) {
                assert!(
                    actual.abs_diff(expected) <= 1,
                    "{name}/{key}: {:?} vs {:?}",
                    pixel.0,
                    case[key]
                );
            }
            assert_eq!(
                pixel[3],
                case[key][3].as_u64().unwrap() as u8,
                "alpha must be exact"
            );
        }
    }
}

#[test]
fn fit_modes_preserve_dimensions_transparent_padding_and_opacity() {
    let token = CancellationToken::new();
    for fit in ["contain", "cover", "stretch"] {
        let (raw, _) = icons::resize_png(&bytes("source.png"), 64, 64, fit, &token).unwrap();
        let (image, _) = icons::decode_image(&raw, &token).unwrap();
        assert_eq!(image.dimensions(), (64, 64));
        assert_eq!(image.get_pixel(32, 32)[3], 128);
        assert_eq!(image.get_pixel(32, 32).0, [255, 64, 32, 128], "resizing must retain color as well as opacity");
        assert_eq!(
            image.get_pixel(32, 0)[3],
            if fit == "contain" { 0 } else { 128 }
        );
    }
    assert!(icons::resize_png(&bytes("source.png"), 64, 64, "invented", &token).is_err());
}

#[test]
fn discovery_uses_authoritative_hero_aliases_and_token_boundaries() {
    let temp = tempfile::tempdir().unwrap();
    let texture = bytes("embedded.vtex_c");
    let archive = archive(
        temp.path(),
        &[
            (TARGET, texture.clone()),
            (
                "panorama/images/heroes/hornet_large_png.vtex_c",
                texture.clone(),
            ),
            (
                "panorama/images/heroes/vindicta2_png.vtex_c",
                texture.clone(),
            ),
            (
                "panorama/images/heroes/hornetnest_png.vtex_c",
                texture.clone(),
            ),
            ("panorama/images/heroes/haze_png.vtex_c", texture),
        ],
    );
    let rows = icons::discover(&archive, HERO, &CancellationToken::new()).unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|r| r.supported));
    assert!(rows.iter().any(|r| r.target == TARGET));
}

#[test]
fn embedded_png_preserves_exact_header_and_publishes_only_selected_target() {
    for name in ["embedded.vtex_c", "padded.vtex_c", "one_pixel.vtex_c"] {
        let temp = tempfile::tempdir().unwrap();
        let original = bytes(name);
        let archive = archive(temp.path(), &[(TARGET, original.clone())]);
        let content = temp.path().join("content");
        let game = temp.path().join("game");
        let cancel = CancellationToken::new();
        let mut receipt = icons::stage(
            &archive,
            &[selection(&fixture("source.png"))],
            &content,
            HERO,
            &cancel,
        )
        .unwrap();
        let targets = icons::compile_entries(
            &mut receipt,
            &content,
            &game,
            &mut |_, _| panic!("Embedded PNG does not invoke compiler"),
            &cancel,
        )
        .unwrap();
        assert_eq!(targets, vec![TARGET]);
        let output = fs::read(game.join(TARGET)).unwrap();
        let offset = icons::texture_metadata(&original).unwrap().payload_offset;
        assert_eq!(&output[..offset], &original[..offset]);
        assert_ne!(&output[offset..], &original[offset..]);
        assert_eq!(
            icons::texture_metadata(&output).unwrap(),
            icons::texture_metadata(&original).unwrap()
        );
        assert_eq!(archive.read(TARGET).unwrap(), original);
        assert!(receipt.entries[0].metadata_verified);
        assert!(!receipt.in_game_validated);
    }
}

#[test]
fn native_recipe_output_must_preserve_every_semantic_texture_field() {
    let temp = tempfile::tempdir().unwrap();
    let original = bytes("gpu.vtex_c");
    let archive = archive(temp.path(), &[(TARGET, original.clone())]);
    let content = temp.path().join("content");
    let game = temp.path().join("game");
    let cancel = CancellationToken::new();
    let mut receipt = icons::stage(
        &archive,
        &[selection(&fixture("source.png"))],
        &content,
        HERO,
        &cancel,
    )
    .unwrap();
    assert_eq!(receipt.entries[0].method, "native_vtex");
    icons::compile_entries(
        &mut receipt,
        &content,
        &game,
        &mut |_, out| safety::atomic_write(out, &original),
        &cancel,
    )
    .unwrap();
    let start = deadlock_vpk::resource_blocks(&original).unwrap().blocks[0].offset;
    for (offset, value) in [(2, 3_u8), (20, 32), (26, 2), (28, 1)] {
        let mut bad = original.clone();
        bad[start + offset] = value;
        assert!(
            icons::compile_entries(
                &mut receipt,
                &content,
                &game,
                &mut |_, out| safety::atomic_write(out, &bad),
                &cancel
            )
            .is_err()
        );
    }
}

#[test]
fn changed_prepared_inputs_stale_originals_duplicate_targets_and_wrong_hero_are_rejected() {
    for name in ["embedded.vtex_c", "gpu.vtex_c"] {
        let temp = tempfile::tempdir().unwrap();
        let archive = archive(temp.path(), &[(TARGET, bytes(name))]);
        let content = temp.path().join("content");
        let game = temp.path().join("game");
        let cancel = CancellationToken::new();
        let selected = selection(&fixture("source.png"));
        let mut receipt =
            icons::stage(&archive, &[selected.clone()], &content, HERO, &cancel).unwrap();
        let mutate = receipt.entries[0]
            .prepared_resource
            .clone()
            .unwrap_or_else(|| receipt.entries[0].source_vtex.clone());
        fs::write(content.join(mutate), b"changed").unwrap();
        assert!(
            icons::compile_entries(
                &mut receipt,
                &content,
                &game,
                &mut |_, _| panic!("Changed source cannot compile"),
                &cancel
            )
            .is_err()
        );
        assert!(
            icons::stage(
                &archive,
                &[selected.clone(), selected.clone()],
                &content,
                HERO,
                &cancel
            )
            .is_err()
        );
        let mut stale = selected.clone();
        stale.original_sha256 = Some("0".repeat(64));
        assert!(icons::stage(&archive, &[stale], &content, HERO, &cancel).is_err());
        assert!(
            icons::stage(
                &archive,
                &[selected],
                &content,
                "models/heroes/haze/haze.vmdl",
                &cancel
            )
            .is_err()
        );
    }
}

#[test]
fn malformed_png_jpeg_metadata_and_cancellation_fail_closed() {
    let cancel = CancellationToken::new();
    let png = bytes("source.png");
    let jpeg = bytes("oriented.jpg");
    for raw in [
        &png[..40],
        &jpeg[..100],
        &jpeg[..jpeg.len() - 2],
        b"not an image",
    ] {
        assert!(icons::decode_image(raw, &cancel).is_err());
    }
    let raw = bytes("embedded.vtex_c");
    for end in 0..68 {
        assert!(icons::texture_metadata(&raw[..end]).is_err());
    }
    let mut bad = raw.clone();
    let offset = deadlock_vpk::resource_blocks(&bad).unwrap().blocks[0].offset;
    bad[offset + 36..offset + 40].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(icons::texture_metadata(&bad).is_err());
    cancel.cancel();
    assert!(matches!(
        icons::decode_image(&png, &cancel),
        Err(CoreError::Cancelled)
    ));
}

#[test]
fn thumbnail_is_bounded_oriented_png_and_identity_resize_retains_exact_profile_pixels() {
    let cancel = CancellationToken::new();
    let png = icons::preview_image(&fixture("oriented.jpg"), 10, &cancel).unwrap();
    let (image, _) = icons::decode_image(&png, &cancel).unwrap();
    assert_eq!(image.dimensions(), (7, 10));
    assert!(icons::preview_image(&fixture("source.png"), 1025, &cancel).is_err());
    let (png, _) = icons::resize_png(&bytes("profile.png"), 64, 64, "contain", &cancel).unwrap();
    let (image, _) = icons::decode_image(&png, &cancel).unwrap();
    assert_eq!(image.get_pixel(0, 0).0, [40, 80, 120, 90]);
}

// Synthetic on-disk layout with the FALLBACK_BITS and 128-byte METADATA
// records seen in actual compiled UI textures. It is not a writer golden.
fn panorama_resource(format: u8, flags: u16, width: u16, height: u16, display: (u16, u16), mips: u8, payload: Vec<u8>) -> Vec<u8> {
    let mut data = vec![0u8; 40 + 24 + 1024 + 128];
    data[..2].copy_from_slice(&1u16.to_le_bytes());
    data[2..4].copy_from_slice(&flags.to_le_bytes());
    data[20..22].copy_from_slice(&width.to_le_bytes());
    data[22..24].copy_from_slice(&height.to_le_bytes());
    data[24..26].copy_from_slice(&1u16.to_le_bytes());
    data[26] = format; data[27] = mips;
    data[32..36].copy_from_slice(&8u32.to_le_bytes());
    data[36..40].copy_from_slice(&2u32.to_le_bytes());
    for (at, kind, offset, size) in [(40, 1u32, 20u32, 1024u32), (52, 3, 1032, 128)] {
        data[at..at+4].copy_from_slice(&kind.to_le_bytes());
        data[at+4..at+8].copy_from_slice(&offset.to_le_bytes());
        data[at+8..at+12].copy_from_slice(&size.to_le_bytes());
    }
    data[64..1088].fill(123); // Recognizable OLD fallback artwork must disappear.
    data[1090..1092].copy_from_slice(&display.0.to_le_bytes());
    data[1092..1094].copy_from_slice(&display.1.to_le_bytes());
    let mut raw = vec![];
    raw.extend((28 + data.len() as u32).to_le_bytes());
    raw.extend(12u16.to_le_bytes()); raw.extend(1u16.to_le_bytes());
    raw.extend(8u32.to_le_bytes()); raw.extend(1u32.to_le_bytes());
    raw.extend(b"DATA"); raw.extend(8u32.to_le_bytes()); raw.extend((data.len() as u32).to_le_bytes());
    raw.extend(data); raw.extend(payload); raw
}

fn replace_without_compiler(original: Vec<u8>, evidence: Option<&Path>) -> (Vec<u8>, icons::IconReceipt) {
    let temp = tempfile::tempdir().unwrap();
    let archive = archive(temp.path(), &[(TARGET, original.clone())]);
    let cancel = CancellationToken::new();
    let discovered = icons::discover(&archive, HERO, &cancel).unwrap();
    assert!(discovered[0].supported, "{}", discovered[0].reason);
    assert_eq!(discovered[0].method.as_deref(), Some("panorama_texture"));
    let content = temp.path().join("content"); let game = temp.path().join("game");
    let mut receipt = icons::stage(&archive, &[selection(&fixture("profile.png"))], &content, HERO, &cancel).unwrap();
    let files = icons::compile_entries(&mut receipt, &content, &game, &mut |_,_| panic!("Panorama replacements must not need resourcecompiler"), &cancel).unwrap();
    let package = temp.path().join("pak27_dir.vpk");
    write_vpk(&game, &files, &package, None, 2, &mut || Ok(())).unwrap();
    let output = Vpk::open(&package).unwrap().read(TARGET).unwrap();
    let preview = icons::preview_texture(&output, 1024, &cancel).unwrap();
    let (image, _) = icons::decode_image(&preview, &cancel).unwrap();
    let center = image.get_pixel(image.width()/2, image.height()/2).0;
    assert_eq!(center[3], 90, "replacement alpha must survive packaging");
    for (actual, expected) in center[..3].iter().zip([40u8,80,120]) {
        assert!(actual.abs_diff(expected) <= 2, "replacement colors must survive packaging: {center:?}");
    }
    assert_eq!(archive.read(TARGET).unwrap(), original);
    if let Some(path) = evidence {fs::write(path, &output).unwrap();}
    // Both metadata and same-length pixel tampering must be rejected later.
    let mut bad = output.clone(); *bad.last_mut().unwrap() ^= 1;
    fs::write(game.join(TARGET), bad).unwrap();
    assert!(icons::verify_compiled(&mut receipt, &game, &cancel).is_err());
    (output, receipt)
}

#[test]
fn panorama_flags_fallbacks_padding_and_custom_mips_are_replaceable_and_package_correctly() {
    for (w, h, display, mips, flags) in [(64,64,(50,50),1,395), (140,140,(137,137),2,11), (600,300,(600,300),3,779)] {
        let size = (0..mips).map(|m| (usize::from(w) >> m).max(1) * (usize::from(h) >> m).max(1) * 4).sum();
        let original = panorama_resource(4, flags, w, h, display, mips, vec![0; size]);
        let old = icons::texture_metadata(&original).unwrap();
        let (output, receipt) = replace_without_compiler(original, None);
        let meta = icons::texture_metadata(&output).unwrap();
        assert_eq!((meta.width, meta.height, meta.display_width, meta.display_height, meta.mips), (w,h,display.0,display.1,mips));
        assert_eq!(meta.format, 4); assert_eq!(meta.flags & (128 | 256), 0);
        assert_eq!(meta.extra_types, [3]); assert!(meta.mip_sizes.is_empty());
        assert_eq!(meta.range_values, old.range_values);
        assert_eq!(output.len()-meta.payload_offset, size);
        assert!(receipt.entries[0].metadata_verified);
    }
    for name in ["compressed.vtex_c", "gpu_padded.vtex_c"] {
        let mut original = bytes(name);
        let data = deadlock_vpk::resource_blocks(&original).unwrap().blocks[0].offset;
        original[data+2..data+4].copy_from_slice(&395u16.to_le_bytes());
        replace_without_compiler(original, None);
    }
}

#[test]
fn embedded_png_with_real_world_fallback_data_can_be_replaced() {
    let original = panorama_resource(18, 395, 64, 64, (64,64), 1, bytes("embedded.vtex_c")[68..].to_vec());
    let (output, _) = replace_without_compiler(original, None);
    let meta = icons::texture_metadata(&output).unwrap();
    assert_eq!((meta.format, meta.flags, meta.mips), (18,395,1));
    assert_eq!(meta.extra_types, [3]);
}

#[test]
fn unknown_virtual_and_multisurface_textures_still_require_a_specific_workflow() {
    let token = CancellationToken::new();
    for flags in [16u16, 32, 64, 1024, 2048] {
        let raw = panorama_resource(4, flags, 4, 4, (4,4), 1, vec![0;64]);
        let meta = icons::texture_metadata(&raw).unwrap();
        assert!(icons::method(&meta, &raw, &token).is_err());
    }
}

#[test]
fn captured_panorama_textures_when_supplied_round_trip_through_replacement_and_vpk() {
    // Optional real upstream samples are kept outside the distributable repo.
    let Ok(directory) = std::env::var("DMB_ICON_TEXTURE_CORPUS") else {return;};
    let output = Path::new(&directory).join("replaced"); fs::create_dir_all(&output).unwrap();
    let mut count = 0;
    for entry in fs::read_dir(&directory).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "vtex_c") {continue;}
        let raw = fs::read(&path).unwrap();
        replace_without_compiler(raw, Some(&output.join(path.file_name().unwrap())));
        count += 1;
    }
    assert!(count >= 6, "Supply all captured color-format samples");
}
