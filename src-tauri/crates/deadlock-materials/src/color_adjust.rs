//! Experimental color editor: hue, saturation and brightness of a material's
//! base color. Only the builder's staged copy changes: an adjusted copy of the
//! base-color image is written next to it and the staged VMAT points to it.
//! Original images, the user's texture folder and other materials sharing the
//! image are unchanged.
//!
//! Color ranges (`AppearanceMaterial.ranges`) use the hero colors' function
//! (`deadlock_project::color_ranges`) and come before hue, saturation and
//! brightness. The preview's copy of `Adjustment::apply` is
//! `src/preview/colorAdjust.ts`; both tests check `tests/fixtures/color-adjust.json`.
use crate::*;
use deadlock_project::{
    color_parts::{self, PartArea, PartMeshes},
    color_ranges::{self, ColorRange},
    customization::AppearanceMaterial,
};
use image::{DynamicImage, ImageFormat, ImageReader};
use std::io::Cursor;

pub const COLOR_POLICY: &str = "experimental-base-color-hsb-v1";
/// Added to the adjusted copy's digest only when a row has ranges.
pub const RANGE_POLICY: &str = "base-color-ranges-v1";
const MAX_BYTES: u64 = 512 * 1024 * 1024;
const COLOR_KEYS: [&str; 2] = ["TextureColor1", "TextureColor"];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Adjustment {
    /// Degrees, -180 to 180.
    pub hue: f64,
    /// Factor, 0 (grey) to 2; 1 keeps the original.
    pub saturation: f64,
    /// Factor, 0 (black) to 2; 1 keeps the original.
    pub brightness: f64,
}
impl Adjustment {
    pub fn from_row(row: &AppearanceMaterial) -> Option<Self> {
        let value = Self {
            hue: row.hue.unwrap_or(0.),
            saturation: row.saturation.unwrap_or(1.),
            brightness: row.brightness.unwrap_or(1.),
        };
        (value.hue != 0. || value.saturation != 1. || value.brightness != 1.).then_some(value)
    }
    pub fn validate(&self) -> Result<()> {
        if !self.hue.is_finite() || !(-180.0..=180.0).contains(&self.hue) {
            return Err(invalid("Hue shift must be from -180 to 180 degrees."));
        }
        for (label, value) in [("Saturation", self.saturation), ("Brightness", self.brightness)] {
            if !value.is_finite() || !(0.0..=2.0).contains(&value) {
                return Err(invalid(format!("{label} must be from 0 to 2.")));
            }
        }
        Ok(())
    }
    /// Adjust one encoded RGB color. `clamp` keeps 8/16-bit colors in range;
    /// HDR colors may exceed one.
    pub fn apply(&self, rgb: [f64; 3], clamp: bool) -> [f64; 3] {
        let [r, g, b] = rgb;
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let delta = max - min;
        let mut h = if delta <= 1e-12 {
            0.
        } else if max == r {
            ((g - b) / delta).rem_euclid(6.)
        } else if max == g {
            (b - r) / delta + 2.
        } else {
            (r - g) / delta + 4.
        } * 60.;
        let mut s = if max > 1e-12 { delta / max } else { 0. };
        let mut v = max;
        h = (h + self.hue).rem_euclid(360.);
        s = (s * self.saturation).clamp(0., 1.);
        v *= self.brightness;
        if clamp {
            v = v.clamp(0., 1.);
        }
        let c = v * s;
        let x = c * (1. - ((h / 60.).rem_euclid(2.) - 1.).abs());
        let m = v - c;
        let (r, g, b) = match (h / 60.).floor() as i32 {
            0 => (c, x, 0.),
            1 => (x, c, 0.),
            2 => (0., c, x),
            3 => (0., x, c),
            4 => (x, 0., c),
            _ => (c, 0., x),
        };
        [r + m, g + m, b + m]
    }
}

fn image_format(path: &str) -> Option<ImageFormat> {
    match path.rsplit('.').next()?.to_ascii_lowercase().as_str() {
        "png" => Some(ImageFormat::Png),
        "jpg" | "jpeg" => Some(ImageFormat::Jpeg),
        "tga" => Some(ImageFormat::Tga),
        "tif" | "tiff" => Some(ImageFormat::Tiff),
        "exr" => Some(ImageFormat::OpenExr),
        _ => None,
    }
}

/// Everything the color editor changes on one material's base color: color
/// ranges first (they select by the colors the user picked from), then hue,
/// saturation and brightness.
#[derive(Debug, Clone, PartialEq)]
pub struct ColorEdit {
    pub adjustment: Option<Adjustment>,
    /// Enabled ranges that change something (`color_ranges::enabled`).
    pub ranges: Vec<ColorRange>,
    /// The image areas of ranges for one part (`ColorRange::part`), read from
    /// the prepared model; empty without such ranges.
    pub parts: Vec<PartArea>,
}
impl ColorEdit {
    pub fn from_row(row: &AppearanceMaterial) -> Option<Self> {
        let edit = Self {
            adjustment: Adjustment::from_row(row),
            ranges: color_ranges::enabled(row.ranges.as_deref().unwrap_or_default()),
            parts: vec![],
        };
        (edit.adjustment.is_some() || !edit.ranges.is_empty()).then_some(edit)
    }
    /// Some range changes one part only.
    pub fn has_parts(&self) -> bool {
        self.ranges.iter().any(|r| r.part.is_some())
    }
    pub fn validate(&self) -> Result<()> {
        if let Some(adjustment) = &self.adjustment {
            adjustment.validate()?;
        }
        if self.ranges.len() > color_ranges::MAX_RANGES {
            return Err(invalid(format!("A material holds at most {} color ranges.", color_ranges::MAX_RANGES)));
        }
        for range in &self.ranges {
            range.validate().map_err(invalid)?;
        }
        color_ranges::check_switches(&self.ranges).map_err(invalid)?;
        Ok(())
    }
    /// One color (channels 0 to 1, as `Adjustment::apply` takes it).
    pub fn apply(&self, rgb: [f64; 3], clamp: bool) -> [f64; 3] {
        let ranged = if self.ranges.is_empty() { rgb } else { color_ranges::recolor_rgb(&self.ranges, rgb) };
        self.adjustment.map_or(ranged, |a| a.apply(ranged, clamp))
    }
    /// The adjusted copy's name digest. Without ranges it is exactly the
    /// digest the editor used before ranges existed, so copies keep their names.
    fn digest(&self, relative: &str) -> String {
        use sha2::{Digest, Sha256};
        let a = self.adjustment.unwrap_or(Adjustment { hue: 0., saturation: 1., brightness: 1. });
        let mut input = format!("{COLOR_POLICY}|{relative}|{}|{}|{}", a.hue, a.saturation, a.brightness);
        if !self.ranges.is_empty() {
            input.push_str(&format!("|ranges|{RANGE_POLICY}|{}", serde_json::to_string(&self.ranges).unwrap_or_default()));
        }
        if !self.parts.is_empty() {
            input.push_str(&format!("|parts|{}", serde_json::to_string(&self.parts).unwrap_or_default()));
        }
        format!("{:x}", Sha256::digest(input.as_bytes()))
    }
}

fn adjust_image(raw: &[u8], format: ImageFormat, edit: &ColorEdit, cancel: &CancellationToken) -> Result<(Vec<u8>, &'static str)> {
    let fail = |e: String| invalid(format!("The base color image could not be adjusted: {e}"));
    let mut reader = ImageReader::with_format(Cursor::new(raw), format);
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(MAX_BYTES);
    limits.max_image_width = Some(16384);
    limits.max_image_height = Some(16384);
    reader.limits(limits);
    let mut image = reader.decode().map_err(|e| fail(e.to_string()))?;
    let (width, height) = (image.width(), image.height());
    let float = matches!(image, DynamicImage::ImageRgb32F(_) | DynamicImage::ImageRgba32F(_));
    let wide = matches!(image, DynamicImage::ImageLuma16(_) | DynamicImage::ImageLumaA16(_) | DynamicImage::ImageRgb16(_) | DynamicImage::ImageRgba16(_));
    // 8-bit images take the ranges texel by texel exactly as hero colors do
    // (`color_ranges::recolor`), so the preview's `recolorPixels` then
    // `adjustPixels` shows the build's bytes. 16-bit and HDR images take them on
    // their own values (`recolor_rgb`) below.
    let eight_bit = !float && !wide;
    // Ranges for one part reach only their part's pixels, marked at this image's size.
    let masks = if edit.parts.is_empty() { vec![] } else { color_parts::masks(edit.ranges.len(), &edit.parts, width as usize, height as usize) };
    let reaches = |pixel: usize, range: usize| masks.get(range).and_then(|m| m.as_ref()).is_none_or(|mask| mask[pixel] == 1);
    if eight_bit && !edit.ranges.is_empty() {
        let mut bytes = image.to_rgba8();
        for (index, pixel) in bytes.pixels_mut().enumerate() {
            if index % 65536 == 0 {
                cancel.check()?;
            }
            pixel.0 = if masks.is_empty() { color_ranges::recolor(&edit.ranges, pixel.0) } else { color_ranges::recolor_where(&edit.ranges, pixel.0, |r| reaches(index, r)) };
        }
        image = DynamicImage::ImageRgba8(bytes);
    }
    let mut pixels = image.to_rgba32f();
    for (index, pixel) in pixels.pixels_mut().enumerate() {
        if index % 65536 == 0 {
            cancel.check()?;
        }
        let mut rgb = [pixel[0] as f64, pixel[1] as f64, pixel[2] as f64];
        if !eight_bit && !edit.ranges.is_empty() {
            rgb = if masks.is_empty() { color_ranges::recolor_rgb(&edit.ranges, rgb) } else { color_ranges::recolor_rgb_where(&edit.ranges, rgb, |r| reaches(index, r)) };
        }
        if let Some(adjustment) = edit.adjustment {
            rgb = adjustment.apply(rgb, !float);
        }
        pixel[0] = rgb[0] as f32;
        pixel[1] = rgb[1] as f32;
        pixel[2] = rgb[2] as f32;
    }
    let adjusted = DynamicImage::ImageRgba32F(pixels);
    let mut out = Vec::new();
    let extension = if float {
        adjusted.write_to(&mut Cursor::new(&mut out), ImageFormat::OpenExr).map_err(|e| fail(e.to_string()))?;
        "exr"
    } else {
        let encoded = if wide { DynamicImage::ImageRgba16(adjusted.to_rgba16()) } else { DynamicImage::ImageRgba8(adjusted.to_rgba8()) };
        encoded.write_to(&mut Cursor::new(&mut out), ImageFormat::Png).map_err(|e| fail(e.to_string()))?;
        "png"
    };
    let check = ImageReader::with_format(Cursor::new(&out), if float { ImageFormat::OpenExr } else { ImageFormat::Png })
        .into_dimensions()
        .map_err(|e| fail(e.to_string()))?;
    if check != (width, height) {
        return Err(fail("encoded dimensions differ".into()));
    }
    Ok((out, extension))
}

fn parse_vector(value: &str) -> Option<Vec<f64>> {
    let inner = value.trim().strip_prefix('[')?.strip_suffix(']')?;
    let values = inner.split_whitespace().map(|v| v.parse::<f64>().ok()).collect::<Option<Vec<_>>>()?;
    (values.len() >= 3 && values.iter().all(|v| v.is_finite())).then_some(values)
}

/// Adjust the base color of a staged VMAT. `content` is the builder-owned
/// staging tree; `text` is the VMAT text after the other surface changes.
pub fn adjust_material(content: &Path, target: &str, text: &str, row: &AppearanceMaterial, cancel: &CancellationToken) -> Result<String> {
    adjust_material_with(content, target, text, row, None, cancel)
}
/// A part range on a material without a base-color image (one plain color for the whole material).
pub const PART_NEEDS_IMAGE: &str = "has a plain base color, so a color range cannot change one part of it. Set the range to Everywhere this color.";
/// `adjust_material`, with the prepared model's meshes for color ranges that
/// change one part (`ColorRange::part`); without them such a range is refused.
pub fn adjust_material_with(content: &Path, target: &str, text: &str, row: &AppearanceMaterial, parts: Option<&dyn PartMeshes>, cancel: &CancellationToken) -> Result<String> {
    let Some(mut edit) = ColorEdit::from_row(row) else {
        return Ok(text.into());
    };
    edit.validate()?;
    let (params, _) = crate::appearance::layer(text)?;
    let Some((_, token)) = COLOR_KEYS.iter().find_map(|k| params.get(*k).map(|t| (*k, t))) else {
        return Err(invalid(format!("{target} has no base color to adjust. Use Tint instead, or choose a material with a base-color image.")));
    };
    if edit.has_parts() {
        if parse_vector(&token.value).is_some() {
            return Err(invalid(format!("{target} {PART_NEEDS_IMAGE}")));
        }
        let meshes = parts
            .ok_or_else(|| invalid(format!("{target}: {}", color_parts::NO_PARTS)))?
            .part_meshes(target)
            .map_err(|e| invalid(format!("{target}: {} {e}", color_parts::NO_PARTS)))?;
        edit.parts = color_parts::areas(&edit.ranges, &meshes).map_err(|e| invalid(format!("{target}: {e}")))?;
    }
    let replacement = if let Some(values) = parse_vector(&token.value) {
        let rgb = edit.apply([values[0], values[1], values[2]], true);
        let rest = values[3..].iter().map(|v| crate::appearance::number(*v)).collect::<Vec<_>>();
        format!(
            "[{}]",
            rgb.iter().map(|v| crate::appearance::number(*v)).chain(rest).collect::<Vec<_>>().join(" ")
        )
    } else {
        let relative = safety::relative_asset(&token.value.replace('\\', "/"))?;
        let format = image_format(&relative).ok_or_else(|| {
            invalid(format!("{target}: the base color is not an editable image ({relative}). Use Tint, or choose a PNG, JPEG, TGA, TIFF or EXR base-color image."))
        })?;
        let source = inside(content, &relative)?;
        if !source.is_file() {
            return Err(invalid(format!("{target}: the base-color image {relative} is not in the staged sources.")));
        }
        if source.metadata()?.len() > MAX_BYTES {
            return Err(invalid(format!("{target}: the base-color image is too large to adjust.")));
        }
        let raw = fs::read(&source)?;
        let (out, extension) = adjust_image(&raw, format, &edit, cancel)?;
        let tag = edit.digest(&relative);
        let stem = relative.rsplit_once('.').map_or(relative.as_str(), |(stem, _)| stem);
        let adjusted = format!("{stem}_dmb_{}.{extension}", &tag[..10]);
        cancel.check()?;
        safety::atomic_write(&inside(content, &adjusted)?, &out)?;
        adjusted
    };
    let mut result = text.to_owned();
    result.replace_range(token.start..token.end, &crate::quote(&replacement));
    crate::appearance::layer(&result)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn row(hue: f64, saturation: f64, brightness: f64) -> AppearanceMaterial {
        AppearanceMaterial { hue: Some(hue), saturation: Some(saturation), brightness: Some(brightness), ..Default::default() }
    }
    fn close(a: [f64; 3], b: [f64; 3]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-6)
    }
    #[test]
    fn hue_saturation_and_brightness_follow_hsv() {
        let neutral = Adjustment { hue: 0., saturation: 1., brightness: 1. };
        assert!(Adjustment::from_row(&row(0., 1., 1.)).is_none());
        assert!(close(neutral.apply([0.2, 0.4, 0.6], true), [0.2, 0.4, 0.6]));
        let shift = Adjustment { hue: 120., ..neutral };
        assert!(close(shift.apply([1., 0., 0.], true), [0., 1., 0.]));
        let grey = Adjustment { saturation: 0., ..neutral };
        assert!(close(grey.apply([1., 0.5, 0.], true), [1., 1., 1.]));
        let darker = Adjustment { brightness: 0.5, ..neutral };
        assert!(close(darker.apply([0.8, 0.4, 0.2], true), [0.4, 0.2, 0.1]));
        let brighter = Adjustment { brightness: 2., ..neutral };
        assert!(close(brighter.apply([0.8, 0.4, 0.2], true), [1., 0.5, 0.25]));
        assert!(close(brighter.apply([0.8, 0.4, 0.2], false), [1.6, 0.8, 0.4]));
        assert!(Adjustment { hue: 181., ..neutral }.validate().is_err());
        assert!(Adjustment { saturation: 2.5, ..neutral }.validate().is_err());
    }
    #[test]
    fn adjusts_a_copy_of_the_staged_image_and_constant_colors() {
        let temp = tempfile::tempdir().unwrap();
        let mut image = image::RgbaImage::new(2, 1);
        image.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
        image.put_pixel(1, 0, image::Rgba([0, 0, 255, 128]));
        let texture = temp.path().join("models/hero/textures/body.png");
        fs::create_dir_all(texture.parent().unwrap()).unwrap();
        image.save(&texture).unwrap();
        let original = fs::read(&texture).unwrap();
        let vmat = "Layer0\n{\n\tshader \"pbr.vfx\"\n\tTextureColor1 \"models/hero/textures/body.png\"\n}\n";
        let out = adjust_material(temp.path(), "m.vmat", vmat, &row(120., 1., 1.), &CancellationToken::new()).unwrap();
        assert_eq!(fs::read(&texture).unwrap(), original);
        let (params, _) = crate::appearance::layer(&out).unwrap();
        let adjusted = &params["TextureColor1"].value;
        assert!(adjusted.starts_with("models/hero/textures/body_dmb_") && adjusted.ends_with(".png"));
        let result = image::open(temp.path().join(adjusted)).unwrap().to_rgba8();
        assert_eq!(result.get_pixel(0, 0).0, [0, 255, 0, 255]);
        assert_eq!(result.get_pixel(1, 0).0, [255, 0, 0, 128]);
        let constant = "Layer0 { shader pbr.vfx TextureColor1 \"[1 0 0 1]\" }";
        let out = adjust_material(temp.path(), "m.vmat", constant, &row(0., 1., 0.5), &CancellationToken::new()).unwrap();
        assert_eq!(crate::appearance::layer(&out).unwrap().0["TextureColor1"].value, "[0.5 0 0 1]");
        let compiled = "Layer0 { shader pbr.vfx TextureColor1 \"materials/x.vtex\" }";
        assert!(adjust_material(temp.path(), "m.vmat", compiled, &row(10., 1., 1.), &CancellationToken::new()).is_err());
        assert_eq!(adjust_material(temp.path(), "m.vmat", compiled, &row(0., 1., 1.), &CancellationToken::new()).unwrap(), compiled);
    }

    // ---- The shared fixture with `src/preview/colorAdjust.ts`.
    const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/color-adjust.json");
    fn fixture_cases() -> Vec<([f64; 3], Adjustment, bool, &'static str)> {
        let a = |hue: f64, saturation: f64, brightness: f64| Adjustment { hue, saturation, brightness };
        let mut cases = vec![];
        for (rgb, name) in [([0., 0., 0.], "black"), ([0.5, 0.5, 0.5], "grey"), ([1., 1., 1.], "white"), ([0.2, 0.2, 0.2], "dark grey")] {
            cases.push((rgb, a(90., 1.5, 1.2), true, name));
            cases.push((rgb, a(-30., 2., 2.), false, name));
        }
        for (rgb, name) in [
            ([1., 0., 0.], "red"),
            ([0., 1., 0.], "green"),
            ([0., 0., 1.], "blue"),
            ([1., 1., 0.], "yellow"),
            ([0., 1., 1.], "cyan"),
            ([1., 0., 1.], "magenta"),
        ] {
            for hue in [120., -120., 60., 180., -180.] {
                cases.push((rgb, a(hue, 1., 1.), true, name));
            }
        }
        // Hue wrap: past 360 and below 0.
        cases.push(([1., 0., 0.2], a(30., 1., 1.), true, "wrap up"));
        cases.push(([1., 0.2, 0.], a(-30., 1., 1.), true, "wrap down"));
        cases.push(([1., 0., 1e-17], a(0.5, 1., 1.), true, "hue just below 360"));
        for (rgb, name) in [([0.8, 0.4, 0.2], "orange"), ([0.2, 0.4, 0.6], "blue grey"), ([0.9, 0.85, 0.1], "olive")] {
            cases.push((rgb, a(0., 0., 1.), true, name));
            cases.push((rgb, a(0., 2., 1.), true, name));
            cases.push((rgb, a(0., 1., 0.), true, name));
            cases.push((rgb, a(0., 1., 2.), true, name));
            cases.push((rgb, a(0., 1., 2.), false, name));
            cases.push((rgb, a(37.5, 0.7, 1.3), true, name));
            cases.push((rgb, a(-123.4, 1.6, 0.45), false, name));
        }
        // HDR values above one, kept or clamped.
        cases.push(([1.5, 0.75, 0.25], a(20., 1., 1.5), false, "hdr"));
        cases.push(([1.5, 0.75, 0.25], a(20., 1., 1.), true, "hdr clamped"));
        cases.push(([4., 4., 4.], a(0., 1., 0.5), false, "hdr grey"));
        cases
    }
    fn fixture_pixels() -> (Vec<[u8; 4]>, Vec<Adjustment>) {
        let steps = [0u8, 1, 51, 127, 128, 204, 254, 255];
        let mut pixels = vec![];
        for (i, r) in steps.iter().enumerate() {
            for (j, g) in steps.iter().enumerate() {
                for (k, b) in steps.iter().enumerate() {
                    pixels.push([*r, *g, *b, ((i * 100 + j * 10 + k) % 256) as u8]);
                }
            }
        }
        pixels.extend([[140, 80, 36, 9], [37, 201, 99, 0], [254, 1, 128, 200], [3, 2, 1, 255]]);
        let a = |hue: f64, saturation: f64, brightness: f64| Adjustment { hue, saturation, brightness };
        (pixels, vec![a(120., 1., 1.), a(-45., 0.5, 1.5), a(180., 2., 0.5), a(0., 0., 1.), a(0., 1., 2.), a(-180., 1.3, 0.7), a(33.3, 1.7, 1.1), a(7., 1., 1.)])
    }
    /// The build's 8-bit path: a PNG through `adjust_image`.
    fn eight_bit(pixels: &[[u8; 4]], adjustment: Adjustment) -> Vec<[u8; 4]> {
        let mut image = image::RgbaImage::new(pixels.len() as u32, 1);
        for (x, p) in pixels.iter().enumerate() {
            image.put_pixel(x as u32, 0, image::Rgba(*p));
        }
        let mut raw = vec![];
        DynamicImage::ImageRgba8(image).write_to(&mut Cursor::new(&mut raw), ImageFormat::Png).unwrap();
        let edit = ColorEdit { adjustment: Some(adjustment), ranges: vec![], parts: vec![] };
        let (out, _) = adjust_image(&raw, ImageFormat::Png, &edit, &CancellationToken::new()).unwrap();
        image::load_from_memory(&out).unwrap().to_rgba8().pixels().map(|p| p.0).collect()
    }
    fn adjustment_json(a: &Adjustment) -> serde_json::Value {
        serde_json::json!({"hue": a.hue, "saturation": a.saturation, "brightness": a.brightness})
    }
    fn fixture_document() -> serde_json::Value {
        let cases = fixture_cases()
            .into_iter()
            .map(|(rgb, a, clamp, name)| {
                serde_json::json!({"name": name, "rgb": rgb, "hue": a.hue, "saturation": a.saturation, "brightness": a.brightness, "clamp": clamp, "expected": a.apply(rgb, clamp)})
            })
            .collect::<Vec<_>>();
        let (pixels, adjustments) = fixture_pixels();
        let flat = |p: &[[u8; 4]]| p.iter().flatten().copied().collect::<Vec<_>>();
        let eight = adjustments
            .iter()
            .map(|a| serde_json::json!({"adjustment": adjustment_json(a), "expected": flat(&eight_bit(&pixels, *a))}))
            .collect::<Vec<_>>();
        serde_json::json!({
            "about": "Adjustment::apply (deadlock-materials color_adjust.rs) and src/preview/colorAdjust.ts must give these values. `cases` are adjustRgb on 0-1 channels (exact); `eight_bit` is the build's 8-bit image path (adjustPixels), RGBA bytes. Written by `cargo test -p deadlock-materials write_color_adjust_fixture -- --ignored`.",
            "cases": cases,
            "eight_bit": {"pixels": flat(&pixels), "adjustments": eight},
        })
    }
    #[test]
    #[ignore = "writes tests/fixtures/color-adjust.json"]
    fn write_color_adjust_fixture() {
        // Compact: the 8-bit section holds a few thousand bytes.
        let text = serde_json::to_string(&fixture_document()).unwrap() + "\n";
        fs::write(FIXTURE, text).unwrap();
    }
    #[test]
    fn color_adjust_matches_the_shared_fixture() {
        let saved: serde_json::Value = serde_json::from_str(&fs::read_to_string(FIXTURE).unwrap()).unwrap();
        let cases = saved["cases"].as_array().unwrap();
        assert!(cases.len() > 60);
        for case in cases {
            let rgb: [f64; 3] = serde_json::from_value(case["rgb"].clone()).unwrap();
            let expected: [f64; 3] = serde_json::from_value(case["expected"].clone()).unwrap();
            let a = Adjustment {
                hue: case["hue"].as_f64().unwrap(),
                saturation: case["saturation"].as_f64().unwrap(),
                brightness: case["brightness"].as_f64().unwrap(),
            };
            // serde_json reads decimals to within an ulp or so (no
            // float_roundtrip feature); the TS test compares them exactly.
            let got = a.apply(rgb, case["clamp"].as_bool().unwrap());
            assert!(got.iter().zip(expected).all(|(g, e)| (g - e).abs() <= 1e-15 * e.abs().max(1.)), "{case} {got:?}");
        }
        let pixels: Vec<u8> = serde_json::from_value(saved["eight_bit"]["pixels"].clone()).unwrap();
        let pixels = pixels.chunks(4).map(|c| [c[0], c[1], c[2], c[3]]).collect::<Vec<_>>();
        for item in saved["eight_bit"]["adjustments"].as_array().unwrap() {
            let a = Adjustment {
                hue: item["adjustment"]["hue"].as_f64().unwrap(),
                saturation: item["adjustment"]["saturation"].as_f64().unwrap(),
                brightness: item["adjustment"]["brightness"].as_f64().unwrap(),
            };
            let expected: Vec<u8> = serde_json::from_value(item["expected"].clone()).unwrap();
            let got = eight_bit(&pixels, a).into_iter().flatten().collect::<Vec<_>>();
            assert_eq!(got, expected, "{a:?}");
            assert!(got.chunks(4).zip(&pixels).all(|(o, i)| o[3] == i[3]), "alpha is untouched");
        }
        // The preview reads a byte as fround(b / 255) in f64; the image crate
        // divides in f32. They agree for every byte, and alpha survives.
        for b in 0..=255u8 {
            assert_eq!(b as f32 / 255., ((b as f64) / 255.) as f32);
            let float = image::ImageBuffer::from_pixel(1, 1, image::Rgba([b as f32 / 255.; 4]));
            assert_eq!(DynamicImage::ImageRgba32F(float).to_rgba8().get_pixel(0, 0).0, [b; 4]);
        }
    }

    // ---- Color ranges on replacement materials.
    use deadlock_project::customization::HeroColorRange;
    fn orange_to_blue() -> HeroColorRange {
        HeroColorRange {
            hue: Some(25.),
            hue_width: Some(12.),
            saturation_range: Some([0.5, 1.]),
            brightness_range: Some([0.2, 1.]),
            softness: Some(0.25),
            hue_shift: Some(180.),
            ..Default::default()
        }
    }
    fn ranged(hue: f64, saturation: f64, brightness: f64, ranges: Vec<HeroColorRange>) -> AppearanceMaterial {
        AppearanceMaterial { ranges: Some(ranges), ..row(hue, saturation, brightness) }
    }
    fn write_png(dir: &Path, pixels: &[[u8; 4]]) -> PathBuf {
        let mut image = image::RgbaImage::new(pixels.len() as u32, 1);
        for (x, p) in pixels.iter().enumerate() {
            image.put_pixel(x as u32, 0, image::Rgba(*p));
        }
        let texture = dir.join("models/hero/textures/body.png");
        fs::create_dir_all(texture.parent().unwrap()).unwrap();
        image.save(&texture).unwrap();
        texture
    }
    const VMAT: &str = "Layer0\n{\n\tshader \"pbr.vfx\"\n\tTextureColor1 \"models/hero/textures/body.png\"\n}\n";
    fn adjusted(dir: &Path, row: &AppearanceMaterial) -> (String, Vec<[u8; 4]>) {
        let out = adjust_material(dir, "m.vmat", VMAT, row, &CancellationToken::new()).unwrap();
        let name = crate::appearance::layer(&out).unwrap().0["TextureColor1"].value.clone();
        let pixels = image::open(dir.join(&name)).unwrap().to_rgba8().pixels().map(|p| p.0).collect();
        (name, pixels)
    }
    #[test]
    fn rows_without_ranges_keep_their_adjusted_copy_name_and_bytes() {
        // The digest input of a row without ranges is the one used before ranges existed.
        let edit = ColorEdit::from_row(&row(120., 0.5, 1.5)).unwrap();
        use sha2::{Digest, Sha256};
        let old = format!("{:x}", Sha256::digest(format!("{COLOR_POLICY}|a/b.png|120|0.5|1.5").as_bytes()));
        assert_eq!(edit.digest("a/b.png"), old);
        // Empty, disabled and do-nothing ranges are no ranges.
        let temp = tempfile::tempdir().unwrap();
        write_png(temp.path(), &[[200, 100, 50, 255], [10, 20, 30, 40]]);
        let plain = adjusted(temp.path(), &row(120., 0.5, 1.5));
        let off = HeroColorRange { enabled: Some(false), ..orange_to_blue() };
        let idle = HeroColorRange { hue_shift: Some(0.), ..Default::default() };
        for ranges in [vec![], vec![off], vec![idle]] {
            let raw_before = fs::read(temp.path().join(&plain.0)).unwrap();
            let same = adjusted(temp.path(), &ranged(120., 0.5, 1.5, ranges));
            assert_eq!(same, plain);
            assert_eq!(fs::read(temp.path().join(&same.0)).unwrap(), raw_before);
        }
        // A row with neither ranges nor adjustment changes nothing.
        assert!(ColorEdit::from_row(&ranged(0., 1., 1., vec![])).is_none());
        assert_eq!(adjust_material(temp.path(), "m.vmat", VMAT, &ranged(0., 1., 1., vec![]), &CancellationToken::new()).unwrap(), VMAT);
    }
    #[test]
    fn ranges_come_before_hue_saturation_and_brightness() {
        let temp = tempfile::tempdir().unwrap();
        let orange = [204, 102, 41, 77];
        let grey = [90, 90, 90, 200];
        write_png(temp.path(), &[orange, grey]);
        let ranges = vec![orange_to_blue()];
        let selected = color_ranges::enabled(&ranges);
        // Ranges alone: exactly the hero colors' texel function, alpha kept.
        let (_, only) = adjusted(temp.path(), &ranged(0., 1., 1., ranges.clone()));
        assert_eq!(only, vec![color_ranges::recolor(&selected, orange), grey]);
        assert!(only[0][2] > only[0][0], "orange turned blue");
        // Then the hue, saturation and brightness of the 8-bit path on those bytes.
        let a = Adjustment { hue: 40., saturation: 0.8, brightness: 1.2 };
        let (both_name, both) = adjusted(temp.path(), &ranged(40., 0.8, 1.2, ranges.clone()));
        assert_eq!(both, eight_bit(&only, a));
        // The other order would select nothing: the hue shift moves orange away first.
        let shifted = eight_bit(&[orange], a)[0];
        assert_ne!(both[0], color_ranges::recolor(&selected, shifted));
        // A changed range is a new copy; the same choices are the same copy.
        let (plain_name, _) = adjusted(temp.path(), &row(40., 0.8, 1.2));
        assert_ne!(both_name, plain_name);
        let wider = HeroColorRange { hue_width: Some(20.), ..orange_to_blue() };
        assert_ne!(adjusted(temp.path(), &ranged(40., 0.8, 1.2, vec![wider])).0, both_name);
        assert_eq!(adjusted(temp.path(), &ranged(40., 0.8, 1.2, ranges)).0, both_name);
    }
    #[test]
    fn ranges_change_constant_colors_and_wide_images() {
        let temp = tempfile::tempdir().unwrap();
        let selected = color_ranges::enabled(&[orange_to_blue()]);
        let constant = "Layer0 { shader pbr.vfx TextureColor1 \"[0.8 0.4 0.16 1]\" }";
        let out = adjust_material(temp.path(), "m.vmat", constant, &ranged(0., 1., 1., vec![orange_to_blue()]), &CancellationToken::new()).unwrap();
        let expected = color_ranges::recolor_rgb(&selected, [0.8, 0.4, 0.16]);
        let value = crate::appearance::layer(&out).unwrap().0["TextureColor1"].value.clone();
        let parsed = parse_vector(&value).unwrap();
        assert!(parsed[..3].iter().zip(expected).all(|(a, b)| (a - b).abs() < 1e-5), "{value}");
        assert!(parsed[2] > parsed[0] && parsed[3] == 1.);
        // 16-bit: ranges on the 16-bit values themselves, written back as 16 bits.
        let mut wide = image::ImageBuffer::<image::Rgba<u16>, Vec<u16>>::new(2, 1);
        wide.put_pixel(0, 0, image::Rgba([52428, 26214, 10486, 30000]));
        wide.put_pixel(1, 0, image::Rgba([20000, 20000, 20000, 65535]));
        let texture = temp.path().join("models/hero/textures/body.png");
        fs::create_dir_all(texture.parent().unwrap()).unwrap();
        DynamicImage::ImageRgba16(wide).save(&texture).unwrap();
        let out = adjust_material(temp.path(), "m.vmat", VMAT, &ranged(0., 1., 1., vec![orange_to_blue()]), &CancellationToken::new()).unwrap();
        let name = crate::appearance::layer(&out).unwrap().0["TextureColor1"].value.clone();
        let result = image::open(temp.path().join(name)).unwrap();
        assert!(matches!(result, DynamicImage::ImageRgba16(_)));
        let result = result.to_rgba16();
        let source = [52428f32 / 65535., 26214. / 65535., 10486. / 65535.].map(|v| v as f64);
        let want = color_ranges::recolor_rgb(&selected, source).map(|v| ((v as f32).clamp(0., 1.) * 65535.).round() as u16);
        assert_eq!(&result.get_pixel(0, 0).0[..3], &want[..]);
        assert_eq!(result.get_pixel(0, 0).0[3], 30000);
        assert_eq!(result.get_pixel(1, 0).0, [20000, 20000, 20000, 65535], "a grey outside the range is unchanged");
        // HDR: a color brighter than 1 lies above every brightness range.
        let range = ColorRange { brightness_range: [0., 1.], softness: 1., hue_shift: 90., ..ColorRange::default() };
        assert_eq!(color_ranges::recolor_rgb(&[range], [2.0, 1.0, 0.5]), [2.0, 1.0, 0.5], "beyond the soft edge: untouched");
        let edge = color_ranges::recolor_rgb(&[range], [1.05, 0.5, 0.2]);
        assert!(edge != [1.05, 0.5, 0.2] && edge.iter().all(|v| *v <= 1.05), "within the soft edge: blended toward a clamped color");
    }
    #[test]
    fn range_values_are_validated_with_plain_messages() {
        let too_many = ranged(0., 1., 1., vec![orange_to_blue(); 9]);
        let error = ColorEdit::from_row(&too_many).unwrap().validate().unwrap_err().to_string();
        assert!(error.contains("at most 8 color ranges"), "{error}");
        let bad = ColorEdit { adjustment: None, ranges: vec![ColorRange { hue_width: 200., ..ColorRange::default() }], parts: vec![] };
        let error = bad.validate().unwrap_err().to_string();
        assert!(error.contains("hue width must be from 0 to 180"), "{error}");
        let state: deadlock_project::Customization = serde_json::from_value(serde_json::json!({
            "appearance": {"enabled": true, "materials": [{"target": "m.vmat", "ranges": vec![serde_json::json!({}); 9]}]}
        }))
        .unwrap();
        assert!(crate::appearance::validate_appearance(&state).unwrap_err().to_string().contains("at most 8 color ranges"));
    }
}
