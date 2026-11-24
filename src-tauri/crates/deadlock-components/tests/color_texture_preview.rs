//! The Colors page's base-color previews (`icons::preview_color_texture`)
//! decode the smallest stored mip that still covers the requested size.
use deadlock_components::icons;
use deadlock_core::CancellationToken;

fn meta(width: u16, height: u16, mips: u8, display: (u16, u16), format: u8) -> icons::TextureMetadata {
    icons::TextureMetadata {
        version: 1,
        flags: 0,
        width,
        height,
        display_width: display.0,
        display_height: display.1,
        depth: 1,
        format,
        format_name: "BC7".into(),
        mips,
        picmip: 0,
        extra_types: vec![],
        payload_offset: 0,
        declared_size: 0,
        motion_distance: 0,
        range_values: [0.; 8],
        mip_sizes: vec![],
        compressed_mips: false,
    }
}

#[test]
fn the_preview_level_is_the_smallest_mip_that_covers_the_size() {
    assert_eq!(icons::preview_level(&meta(4096, 4096, 13, (4096, 4096), 20), 512), 3);
    assert_eq!(icons::preview_level(&meta(4096, 4096, 13, (4096, 4096), 20), 1024), 2);
    assert_eq!(icons::preview_level(&meta(2048, 1024, 12, (2048, 1024), 20), 512), 2);
    assert_eq!(icons::preview_level(&meta(512, 512, 10, (512, 512), 20), 512), 0, "already the size");
    assert_eq!(icons::preview_level(&meta(256, 256, 9, (256, 256), 20), 512), 0, "smaller than the size");
    assert_eq!(icons::preview_level(&meta(4096, 4096, 1, (4096, 4096), 20), 512), 0, "only one mip stored");
    assert_eq!(icons::preview_level(&meta(4096, 4096, 13, (3000, 4096), 20), 512), 0, "a display rectangle keeps mip 0");
    assert_eq!(icons::preview_level(&meta(4096, 4096, 13, (4096, 4096), 16), 512), 0, "an encoded image has no mips");
    assert_eq!(icons::preview_level(&meta(2052, 2052, 12, (2052, 2052), 20), 512), 2, "only levels that halve both sides exactly");
}

/// A real texture: `DMB_PAK` = pak01_dir.vpk, `DMB_COLOR_TEXTURES` = game
/// paths of compiled base-color textures (comma separated).
#[test]
#[ignore = "needs DMB_PAK and DMB_COLOR_TEXTURES"]
fn previews_from_a_mip_stay_close_to_the_full_image() {
    let pak = std::env::var("DMB_PAK").expect("DMB_PAK");
    let archive = deadlock_vpk::Vpk::open(&pak).unwrap();
    let cancel = CancellationToken::new();
    for path in std::env::var("DMB_COLOR_TEXTURES").expect("DMB_COLOR_TEXTURES").split(',') {
        let raw = archive.read(path.trim()).unwrap();
        let info = icons::texture_metadata(&raw).unwrap();
        for size in [512u32, 1024] {
            let started = std::time::Instant::now();
            let full = icons::preview_color_texture_level(&raw, size, 0, &cancel).unwrap();
            let full_ms = started.elapsed().as_secs_f64() * 1000.;
            let started = std::time::Instant::now();
            let fast = icons::preview_color_texture(&raw, size, &cancel).unwrap();
            let fast_ms = started.elapsed().as_secs_f64() * 1000.;
            let (a, b) = (image::load_from_memory(&full).unwrap().to_rgba8(), image::load_from_memory(&fast).unwrap().to_rgba8());
            assert_eq!(a.dimensions(), b.dimensions(), "{path} {size}");
            assert!(b.pixels().all(|p| p[3] == 255), "alpha stays opaque");
            let (mut sum, mut worst, mut count) = (0f64, 0u8, 0f64);
            for (p, q) in a.pixels().zip(b.pixels()) {
                for k in 0..3 {
                    let d = p[k].abs_diff(q[k]);
                    sum += f64::from(d);
                    worst = worst.max(d);
                    count += 1.;
                }
            }
            let mean = sum / count;
            // The level read is the right one: mip n+1 is close to a 2×2 box
            // average of mip n (a wrong offset would give a different image).
            let level = icons::preview_level(&info, size);
            if level > 0 {
                let larger = icons::preview_color_texture_level(&raw, size * 2, level - 1, &cancel).unwrap();
                let larger = image::load_from_memory(&larger).unwrap().to_rgba8();
                if larger.width() == b.width() * 2 {
                    let mut total = 0f64;
                    for (x, y, q) in b.enumerate_pixels() {
                        for k in 0..3 {
                            let quad: u32 = [(0, 0), (1, 0), (0, 1), (1, 1)].iter().map(|(dx, dy)| u32::from(larger.get_pixel(2 * x + dx, 2 * y + dy)[k])).sum();
                            total += (f64::from(quad) / 4. - f64::from(q[k])).abs();
                        }
                    }
                    let box_mean = total / count;
                    println!("    mip {level} against a box average of mip {}: mean {box_mean:.2}", level - 1);
                    assert!(box_mean < 2.0, "{path} {size}: the decoded mip does not follow the larger one ({box_mean})");
                }
            }
            if std::env::var_os("DMB_EACH_LEVEL").is_some() {
                for level in 1..=icons::preview_level(&info, size) {
                    let other = icons::preview_color_texture_level(&raw, size, level, &cancel).unwrap();
                    let c = image::load_from_memory(&other).unwrap().to_rgba8();
                    let total: f64 = a.pixels().zip(c.pixels()).map(|(p, q)| (0..3).map(|k| f64::from(p[k].abs_diff(q[k]))).sum::<f64>()).sum();
                    println!("    level {level}: mean {:.2}", total / count);
                }
            }
            println!(
                "{path} {}x{} -> {size}: level {} {fast_ms:.0} ms (mip 0 {full_ms:.0} ms), mean difference {mean:.2}, largest {worst}",
                info.width,
                info.height,
                icons::preview_level(&info, size)
            );
            // A stored mip is Valve's own downscale, not the thumbnail filter's,
            // so the two are not byte-equal: on the hero textures checked they
            // differ by 0.1 to 2.4 levels of 255 on average, more at the edges
            // of fine detail and UV islands (up to about 110 in single pixels).
            // The mip is what the game itself draws at that size.
            assert!(mean < 3.0, "{path} {size}: mean difference {mean}");
        }
    }
}
