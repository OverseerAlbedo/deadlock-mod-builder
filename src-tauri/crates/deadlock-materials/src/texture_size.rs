//! Adapt only copied build textures to the native compiler's power-of-two grid.
//! Full UV coverage is retained: no padding, cropping or UV rewrites. Already
//! compatible files stay byte-identical, including their original metadata.
use crate::*;
use image::{DynamicImage, ImageDecoder, ImageEncoder, ImageFormat, ImageReader};
use serde::Serialize;
use std::{collections::BTreeSet, io::Cursor};

pub const TEXTURE_SIZE_POLICY: &str = "staged-power-of-two-textures-v1";
const MAX_BYTES: u64 = 512 * 1024 * 1024;
const MAX_PIXELS: u64 = 64 * 1024 * 1024;

#[derive(Debug, Serialize)]
pub struct TextureResize {
    pub path: String,
    pub original_dimensions: [u32; 2],
    pub dimensions: [u32; 2],
    pub original_sha256: String,
    pub sha256: String,
}
#[derive(Debug, Serialize)]
pub struct TextureSizeReceipt {
    pub policy: String,
    pub checked_textures: usize,
    pub resized: Vec<TextureResize>,
}

fn format(path: &Path) -> Option<ImageFormat> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "png" => Some(ImageFormat::Png),
        "jpg" | "jpeg" => Some(ImageFormat::Jpeg),
        "tga" => Some(ImageFormat::Tga),
        "tif" | "tiff" => Some(ImageFormat::Tiff),
        "exr" => Some(ImageFormat::OpenExr),
        _ => None,
    }
}
// imageops' filtered float path clamps samples to [0, 1]. HDR textures can
// contain negative values and values above one, so interpolate their channels
// explicitly. Pixel-centre sampling preserves the entire normalized UV grid.
fn resize_float<const N: usize>(raw: &[f32], width: u32, height: u32, w: u32, h: u32, cancel: &CancellationToken) -> Result<Vec<f32>> {
    let mut output = vec![0.; w as usize * h as usize * N];
    for y in 0..h {
        if y % 32 == 0 { cancel.check()?; }
        let sy = ((y as f64 + 0.5) * height as f64 / h as f64 - 0.5).clamp(0., (height-1) as f64);
        let y0 = sy.floor() as u32; let y1 = (y0+1).min(height-1); let fy = sy-y0 as f64;
        for x in 0..w {
            let sx = ((x as f64 + 0.5) * width as f64 / w as f64 - 0.5).clamp(0., (width-1) as f64);
            let x0 = sx.floor() as u32; let x1 = (x0+1).min(width-1); let fx = sx-x0 as f64;
            for c in 0..N {
                let at = |xx, yy| raw[(yy as usize * width as usize + xx as usize)*N+c] as f64;
                output[(y as usize*w as usize+x as usize)*N+c] =
                    ((at(x0,y0)*(1.-fx)+at(x1,y0)*fx)*(1.-fy)+(at(x0,y1)*(1.-fx)+at(x1,y1)*fx)*fy) as f32;
            }
        }
    }
    Ok(output)
}
fn resize(path: &Path, relative: &str, cancel: &CancellationToken) -> Result<Option<TextureResize>> {
    cancel.check()?;
    let Some(format) = format(path) else { return Ok(None); };
    if path.metadata()?.len() > MAX_BYTES {
        return Err(invalid(format!("Texture is too large for automatic dimension checking: {relative}")));
    }
    let raw = fs::read(path)?;
    let fail = |e: String| invalid(format!("Cannot prepare texture {relative}: {e}"));
    let mut reader = ImageReader::with_format(Cursor::new(&raw), format);
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(MAX_BYTES);
    limits.max_image_width = Some(16384);
    limits.max_image_height = Some(16384);
    reader.limits(limits);
    let mut decoder = reader.into_decoder().map_err(|e| fail(e.to_string()))?;
    let (width, height) = decoder.dimensions();
    if width.is_power_of_two() && height.is_power_of_two() { return Ok(None); }
    if width == 0 || height == 0 {
        return Err(fail("image dimensions must be positive".into()));
    }
    let w = width.checked_next_power_of_two().ok_or_else(|| fail("width overflow".into()))?;
    let h = height.checked_next_power_of_two().ok_or_else(|| fail("height overflow".into()))?;
    let pixels = u64::from(w) * u64::from(h);
    if pixels > MAX_PIXELS || pixels * u64::from(decoder.color_type().bytes_per_pixel()) > MAX_BYTES {
        return Err(fail("power-of-two conversion exceeds the bounded texture memory budget; reduce its resolution".into()));
    }
    let icc = if format==ImageFormat::Tiff {
        // TIFF's legacy get_tag_u8_vec rejects BYTE tags decoded as unsigned
        // values. The typed API preserves these profiles instead of dropping
        // them through image's .ok() metadata fallback.
        tiff::decoder::Decoder::new(Cursor::new(&raw)).map_err(|e|fail(e.to_string()))?
            .find_tag_unsigned_vec::<u8>(tiff::tags::Tag::IccProfile).map_err(|e|fail(e.to_string()))?
    } else { decoder.icc_profile().map_err(|e| fail(e.to_string()))? };
    let exif = decoder.exif_metadata().map_err(|e| fail(e.to_string()))?;
    let image = DynamicImage::from_decoder(decoder).map_err(|e| fail(e.to_string()))?;
    cancel.check()?;
    let resized = match image {
        DynamicImage::ImageRgb32F(ref pixels) => DynamicImage::ImageRgb32F(image::ImageBuffer::from_raw(w,h,resize_float::<3>(pixels.as_raw(),width,height,w,h,cancel)?).unwrap()),
        DynamicImage::ImageRgba32F(ref pixels) => DynamicImage::ImageRgba32F(image::ImageBuffer::from_raw(w,h,resize_float::<4>(pixels.as_raw(),width,height,w,h,cancel)?).unwrap()),
        _ => image.resize_exact(w, h, image::imageops::FilterType::Lanczos3),
    };
    cancel.check()?;
    let mut out = Vec::new();
    match format {
        ImageFormat::Png => {
            let mut encoder = image::codecs::png::PngEncoder::new(&mut out);
            if let Some(icc) = icc { encoder.set_icc_profile(icc).map_err(|e| fail(e.to_string()))?; }
            if let Some(exif) = exif { encoder.set_exif_metadata(exif).map_err(|e| fail(e.to_string()))?; }
            encoder.write_image(resized.as_bytes(), w, h, resized.color().into()).map_err(|e| fail(e.to_string()))?;
        }
        ImageFormat::Jpeg => {
            let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 95);
            if let Some(icc) = icc { encoder.set_icc_profile(icc).map_err(|e| fail(e.to_string()))?; }
            if let Some(exif) = exif { encoder.set_exif_metadata(exif).map_err(|e| fail(e.to_string()))?; }
            encoder.encode_image(&resized).map_err(|e| fail(e.to_string()))?;
        }
        ImageFormat::Tiff => {
            let mut encoder = image::codecs::tiff::TiffEncoder::new(Cursor::new(&mut out));
            if let Some(icc) = icc { encoder.set_icc_profile(icc).map_err(|e| fail(e.to_string()))?; }
            encoder.write_image(resized.as_bytes(), w, h, resized.color().into()).map_err(|e| fail(e.to_string()))?;
        }
        _ => resized.write_to(&mut Cursor::new(&mut out), format).map_err(|e| fail(e.to_string()))?,
    }
    // Check encoded dimensions before publishing. The staging copy is the only
    // file replaced; source stability is checked again by the pipeline.
    let actual = ImageReader::with_format(Cursor::new(&out), format).into_dimensions().map_err(|e| fail(e.to_string()))?;
    if actual != (w, h) { return Err(fail("encoded dimensions differ".into())); }
    cancel.check()?;
    use sha2::{Digest, Sha256};
    let before = format!("{:x}", Sha256::digest(&raw));
    if safety::digest_with_cancel(path, cancel)? != before {
        return Err(fail("staged texture changed during conversion".into()));
    }
    safety::atomic_write(path, &out)?;
    Ok(Some(TextureResize { path: relative.into(), original_dimensions: [width, height], dimensions: [w, h],
        original_sha256: before, sha256: format!("{:x}", Sha256::digest(&out)) }))
}

/// `content` must be the builder-owned copied content tree, never a user's
/// source folder. Follow actual VMAT/VTEX references, including nested VTEXes.
pub fn normalize_staged_textures(content: &Path, materials: &[String], cancel: &CancellationToken) -> Result<TextureSizeReceipt> {
    let mut pending = materials.to_vec();
    let mut visited = BTreeSet::new();
    let mut textures = BTreeSet::new();
    while let Some(relative) = pending.pop() {
        cancel.check()?;
        if !visited.insert(relative.clone()) { continue; }
        if visited.len() > 20000 { return Err(invalid("Material texture dependency graph exceeds the supported budget.")); }
        let path = inside(content, &relative)?;
        if !path.is_file() { continue; } // Engine-provided defaults are resolved by the native compiler.
        if format(&path).is_some() { textures.insert(relative); continue; }
        let extension=path.extension().and_then(|s|s.to_str()).unwrap_or("").to_ascii_lowercase();
        if !matches!(extension.as_str(), "vmat" | "vtex") { continue; }
        for reference in texture_references(&read_text(&path)?) {
            let decoded = serde_json::from_str::<String>(&format!("\"{reference}\"")).unwrap_or(reference);
            pending.push(safety::relative_asset(&decoded.replace('\\', "/"))?);
        }
    }
    let mut result = TextureSizeReceipt { policy: TEXTURE_SIZE_POLICY.into(), checked_textures: textures.len(), resized: vec![] };
    for relative in textures {
        if let Some(change) = resize(&inside(content, &relative)?, &relative, cancel)? { result.resized.push(change); }
    }
    Ok(result)
}
