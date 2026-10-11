//! Experimental color editor: hue, saturation and brightness of a material's
//! base color. Only the builder's staged copy changes: an adjusted copy of the
//! base-color image is written next to it and the staged VMAT points to it.
//! Original images, the user's texture folder and other materials sharing the
//! image are unchanged.
use crate::*;
use deadlock_project::customization::AppearanceMaterial;
use image::{DynamicImage, ImageFormat, ImageReader};
use std::io::Cursor;

pub const COLOR_POLICY: &str = "experimental-base-color-hsb-v1";
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

fn adjust_image(raw: &[u8], format: ImageFormat, adjustment: Adjustment, cancel: &CancellationToken) -> Result<(Vec<u8>, &'static str)> {
    let fail = |e: String| invalid(format!("The base color image could not be adjusted: {e}"));
    let mut reader = ImageReader::with_format(Cursor::new(raw), format);
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(MAX_BYTES);
    limits.max_image_width = Some(16384);
    limits.max_image_height = Some(16384);
    reader.limits(limits);
    let image = reader.decode().map_err(|e| fail(e.to_string()))?;
    let (width, height) = (image.width(), image.height());
    let float = matches!(image, DynamicImage::ImageRgb32F(_) | DynamicImage::ImageRgba32F(_));
    let wide = matches!(image, DynamicImage::ImageLuma16(_) | DynamicImage::ImageLumaA16(_) | DynamicImage::ImageRgb16(_) | DynamicImage::ImageRgba16(_));
    let mut pixels = image.to_rgba32f();
    for (index, pixel) in pixels.pixels_mut().enumerate() {
        if index % 65536 == 0 {
            cancel.check()?;
        }
        let rgb = adjustment.apply([pixel[0] as f64, pixel[1] as f64, pixel[2] as f64], !float);
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
    let Some(adjustment) = Adjustment::from_row(row) else {
        return Ok(text.into());
    };
    adjustment.validate()?;
    let (params, _) = crate::appearance::layer(text)?;
    let Some((_, token)) = COLOR_KEYS.iter().find_map(|k| params.get(*k).map(|t| (*k, t))) else {
        return Err(invalid(format!("{target} has no base color to adjust. Use Tint instead, or choose a material with a base-color image.")));
    };
    let replacement = if let Some(values) = parse_vector(&token.value) {
        let rgb = adjustment.apply([values[0], values[1], values[2]], true);
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
        let (out, extension) = adjust_image(&raw, format, adjustment, cancel)?;
        use sha2::{Digest, Sha256};
        let tag = format!(
            "{:x}",
            Sha256::digest(format!("{COLOR_POLICY}|{relative}|{}|{}|{}", adjustment.hue, adjustment.saturation, adjustment.brightness).as_bytes())
        );
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
}
