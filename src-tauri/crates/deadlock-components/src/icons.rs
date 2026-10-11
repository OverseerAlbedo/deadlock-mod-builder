//! Explicit Panorama image targets with immutable source and texture contracts.
use crate::{CancellationToken, Result, fail, read, read_file, sha};
use deadlock_core::{safety, text::casefold};
use deadlock_project::customization::ResourceReplacement;
use deadlock_vpk::Vpk;
use image::{DynamicImage, ImageDecoder, ImageEncoder, RgbaImage, metadata::Orientation};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    io::Cursor,
    path::{Path, PathBuf},
};
pub const MAX_IMAGE_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_IMAGE_PIXELS: u64 = 32 * 1024 * 1024;
mod texture_write;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ImageInfo {
    pub width: u32,
    pub height: u32,
    pub format: String,
    pub has_alpha: bool,
    pub color_space: String,
    pub profile_converted: bool,
    pub sha256: String,
    pub source: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TextureMetadata {
    pub version: u16,
    pub flags: u16,
    pub width: u16,
    pub height: u16,
    pub display_width: u16,
    pub display_height: u16,
    pub depth: u16,
    pub format: u8,
    pub format_name: String,
    pub mips: u8,
    pub picmip: u32,
    pub extra_types: Vec<u32>,
    pub payload_offset: usize,
    pub declared_size: usize,
    pub motion_distance: i16,
    pub range_values: [f32; 8],
    pub mip_sizes: Vec<u32>,
    pub compressed_mips: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IconDiscovery {
    pub target: String,
    pub path: String,
    pub label: String,
    pub supported: bool,
    pub reason: String,
    pub width: u16,
    pub height: u16,
    pub original_sha256: Option<String>,
    pub metadata: Option<TextureMetadata>,
    pub method: Option<String>,
    #[serde(default)]
    pub shared: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IconEntry {
    pub target: String,
    pub method: String,
    pub source: String,
    pub source_sha256: String,
    pub source_image: ImageInfo,
    pub prepared_image: String,
    pub prepared_sha256: String,
    pub fit: String,
    pub original_sha256: String,
    pub original_metadata: TextureMetadata,
    pub source_vtex: String,
    pub prepared_resource: Option<String>,
    pub prepared_resource_sha256: Option<String>,
    pub descriptor_sha256: Option<String>,
    pub compiled_sha256: Option<String>,
    pub metadata_verified: bool,
    #[serde(default)]
    pub allow_shared: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IconReceipt {
    pub version: u32,
    pub entries: Vec<IconEntry>,
    pub native_validated: bool,
    pub in_game_validated: bool,
}
impl IconEntry {
    pub fn prepared_inputs(&self) -> Vec<(&str, &str)> {
        let mut out = vec![(self.prepared_image.as_str(), self.prepared_sha256.as_str())];
        if let (Some(path), Some(hash)) = (&self.prepared_resource, &self.prepared_resource_sha256)
        {
            out.push((path, hash));
        }
        if let Some(hash) = &self.descriptor_sha256 {
            out.push((&self.source_vtex, hash));
        }
        out
    }
}
fn word(raw: &[u8], offset: usize) -> Result<u16> {
    Ok(u16::from_le_bytes(
        raw.get(offset..offset + 2)
            .ok_or_else(|| fail("Truncated icon header"))?
            .try_into()
            .unwrap(),
    ))
}
fn dword(raw: &[u8], offset: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(
        raw.get(offset..offset + 4)
            .ok_or_else(|| fail("Truncated icon header"))?
            .try_into()
            .unwrap(),
    ))
}
fn format_name(format: u8) -> Option<&'static str> {
    match format {
        1 => Some("DXT1"),
        2 => Some("DXT5"),
        3 => Some("I8"),
        4 => Some("RGBA8888"),
        15 => Some("JPEG_RGBA8888"),
        16 => Some("PNG_RGBA8888"),
        17 => Some("JPEG_DXT5"),
        18 => Some("PNG_DXT5"),
        20 => Some("BC7"),
        21 => Some("ATI2N"),
        22 => Some("IA88"),
        27 => Some("ATI1N"),
        28 => Some("BGRA8888"),
        32 => Some("A8"),
        33 => Some("R8_UNORM"),
        _ => None,
    }
}
fn dimensions(width: u32, height: u32) -> Result<()> {
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_IMAGE_PIXELS {
        Err(fail(
            "Image exceeds the supported 32-megapixel size or has empty dimensions.",
        ))
    } else {
        Ok(())
    }
}

pub fn texture_metadata(raw: &[u8]) -> Result<TextureMetadata> {
    if raw.len() > crate::MAX_RESOURCE_BYTES {
        return Err(fail("Icon resource exceeds the supported 128 MiB limit."));
    }
    let container = deadlock_vpk::resource_blocks(raw).map_err(fail)?;
    let blocks: Vec<_> = container
        .blocks
        .iter()
        .filter(|b| b.name == "DATA")
        .collect();
    if blocks.len() != 1 || blocks[0].size < 40 {
        return Err(fail("Icon texture has no unique, complete DATA header."));
    }
    let block = blocks[0];
    let start = block.offset;
    let end = start + block.size;
    let version = word(raw, start)?;
    let flags = word(raw, start + 2)?;
    let width = word(raw, start + 20)?;
    let height = word(raw, start + 22)?;
    let depth = word(raw, start + 24)?;
    let format = raw[start + 26];
    let mips = raw[start + 27];
    if version != 1 || depth == 0 || mips == 0 {
        return Err(fail("Unsupported or invalid icon texture header."));
    }
    dimensions(width as u32, height as u32)?;
    let picmip = dword(raw, start + 28)?;
    let extra = dword(raw, start + 32)? as usize;
    let count = dword(raw, start + 36)? as usize;
    let table = start
        .checked_add(32)
        .and_then(|x| x.checked_add(extra))
        .ok_or_else(|| fail("Invalid icon extra-data table"))?;
    let table_end = table
        .checked_add(
            count
                .checked_mul(12)
                .ok_or_else(|| fail("Invalid icon extra-data table"))?,
        )
        .ok_or_else(|| fail("Invalid icon extra-data table"))?;
    if count > 0 && (table < start + 40 || table_end > end) {
        return Err(fail("Icon texture has an invalid extra-data table."));
    }
    let mut result = TextureMetadata {
        version,
        flags,
        width,
        height,
        display_width: width,
        display_height: height,
        depth,
        format,
        format_name: format_name(format)
            .map(str::to_owned)
            .unwrap_or_else(|| format!("format {format}")),
        mips,
        picmip,
        extra_types: vec![],
        payload_offset: end,
        declared_size: container.declared_size,
        motion_distance: 0,
        range_values: [0.; 8],
        mip_sizes: vec![],
        compressed_mips: false,
    };
    let mut ranges = vec![];
    for i in 0..count {
        let pos = table + i * 12;
        let kind = dword(raw, pos)?;
        let offset = dword(raw, pos + 4)? as usize;
        let size = dword(raw, pos + 8)? as usize;
        let item = pos
            .checked_add(4)
            .and_then(|p| p.checked_add(offset))
            .ok_or_else(|| fail("Invalid icon extra data"))?;
        let item_end = item
            .checked_add(size)
            .ok_or_else(|| fail("Invalid icon extra data"))?;
        if item < table_end
            || item_end > end
            || result.extra_types.contains(&kind)
            || ranges.iter().any(|&(a, b)| item < b && a < item_end)
        {
            return Err(fail(
                "Icon texture has invalid, overlapping or duplicate extra data.",
            ));
        }
        ranges.push((item, item_end));
        result.extra_types.push(kind);
        if kind == 3 {
            if size < 40 {
                return Err(fail("Icon texture display rectangle is truncated."));
            }
            let dw = word(raw, item + 2)?;
            let dh = word(raw, item + 4)?;
            if dw > 0 && dh > 0 && dw <= width && dh <= height {
                result.display_width = if dw != 1 || width == 4 { dw } else { width };
                result.display_height = if dh != 1 || height == 4 { dh } else { height };
            }
            result.motion_distance = word(raw, item + 6)? as i16;
            for j in 0..8 {
                result.range_values[j] = f32::from_bits(dword(raw, item + 8 + j * 4)?);
            }
            if result.range_values.iter().any(|v| !v.is_finite()) {
                return Err(fail("Icon texture contains non-finite range metadata."));
            }
        } else if kind == 4 {
            if size < 12 {
                return Err(fail("Icon compressed mip table is truncated."));
            }
            let mode = dword(raw, item)?;
            let offset = dword(raw, item + 4)? as usize;
            let n = dword(raw, item + 8)? as usize;
            let values = item
                .checked_add(4)
                .and_then(|p| p.checked_add(offset))
                .ok_or_else(|| fail("Invalid mip offset"))?;
            if mode > 1
                || n != mips as usize
                || values < item + 12
                || values.checked_add(n * 4).is_none_or(|end| end > item_end)
            {
                return Err(fail("Icon compressed mip table is invalid."));
            }
            result.mip_sizes = (0..n)
                .map(|i| dword(raw, values + i * 4))
                .collect::<Result<_>>()?;
            result.compressed_mips = mode == 1;
        }
    }
    result.extra_types.sort_unstable();
    Ok(result)
}

pub fn validate_gpu_pixels(meta: &TextureMetadata, raw: &[u8]) -> Result<()> {
    if meta.payload_offset != meta.declared_size {
        return Err(fail("GPU icon uses an unsupported resource/pixel layout."));
    }
    if meta.mips > 16 - meta.width.max(meta.height).leading_zeros() as u8 {
        return Err(fail("Icon texture has more mip levels than its dimensions allow."));
    }
    let expected: Vec<usize> = (0..meta.mips)
        .map(|level| {
            let width = (meta.width as usize)
                .checked_shr(level as u32)
                .unwrap_or(0)
                .max(1);
            let height = (meta.height as usize)
                .checked_shr(level as u32)
                .unwrap_or(0)
                .max(1);
            if [1, 2, 20, 21, 27].contains(&meta.format) {
                width.div_ceil(4) * height.div_ceil(4) * if [1, 27].contains(&meta.format) { 8 } else { 16 }
            } else {
                width * height * match meta.format {3 | 32 | 33 => 1, 22 => 2, _ => 4}
            }
        })
        .collect();
    let sizes: Vec<_> = if meta.mip_sizes.is_empty() {
        expected.clone()
    } else {
        meta.mip_sizes.iter().map(|v| *v as usize).collect()
    };
    let payload = raw
        .get(meta.payload_offset..)
        .ok_or_else(|| fail("Missing icon pixels"))?;
    if sizes.len() != expected.len()
        || sizes.iter().try_fold(0usize, |a, b| a.checked_add(*b)) != Some(payload.len())
    {
        return Err(fail(
            "Icon texture has truncated or unexpected mip pixel data.",
        ));
    }
    let mut offset = 0;
    for (&stored, &size) in sizes.iter().zip(&expected).rev() {
        if stored == 0 || stored > size || (!meta.compressed_mips && stored != size) {
            return Err(fail("Icon texture has an invalid stored mip size."));
        }
        if stored < size {
            let pixels = lz4_flex::block::decompress(&payload[offset..offset + stored], size)
                .map_err(|_| fail("Icon texture contains an invalid compressed mip."))?;
            if pixels.len() != size {
                return Err(fail("Icon compressed mip decoded to the wrong size."));
            }
        }
        offset += stored;
    }
    Ok(())
}
pub fn method(
    meta: &TextureMetadata,
    raw: &[u8],
    cancel: &CancellationToken,
) -> Result<&'static str> {
    if meta.depth != 1 || meta.flags & (16 | 32 | 64) != 0 {
        return Err(fail(
            "This image is an array, cube or volume texture; a single icon cannot replace it.",
        ));
    }
    if meta.extra_types.contains(&2) {
        return Err(fail(
            "This image is a sprite sheet. Its atlas needs a dedicated layout workflow.",
        ));
    }
    if meta.flags & !1023 != 0 || meta.extra_types.iter().any(|n| ![1, 3, 4].contains(n)) {
        return Err(fail("This texture has an unknown or virtual texture layout. Its original is kept; this is an app format limitation."));
    }
    if [15, 16, 17, 18].contains(&meta.format) {
        if meta.mips != 1 || meta.extra_types.contains(&4) {
            return Err(fail(
                "This embedded PNG has mip or auxiliary data that cannot be preserved safely.",
            ));
        }
        if meta.payload_offset != meta.declared_size {
            return Err(fail("Embedded PNG uses an unsupported resource layout."));
        }
        let payload = raw
            .get(meta.payload_offset..)
            .ok_or_else(|| fail("Missing embedded PNG"))?;
        let (_, info) = decode_image(payload, cancel)?;
        let png = [16, 18].contains(&meta.format);
        if info.format != if png { "PNG" } else { "JPEG" }
            || (info.width, info.height) != (meta.display_width as u32, meta.display_height as u32)
        {
            return Err(fail(
                "Original embedded PNG dimensions disagree with its texture metadata.",
            ));
        }
        if (png && !payload.ends_with(b"\0\0\0\0IEND\xaeB`\x82")) || (!png && !payload.ends_with(b"\xff\xd9")) {
            return Err(fail(
                "Embedded PNG contains an unsupported trailing payload.",
            ));
        }
        Ok(if png && !meta.extra_types.contains(&1) { "embedded_png" } else { "panorama_texture" })
    } else {
        if ![1, 2, 3, 4, 20, 21, 22, 27, 28, 32, 33].contains(&meta.format) {
            return Err(fail(
                "This icon uses unsupported texture processing flags, auxiliary data or format; its settings will not be discarded.",
            ));
        }
        let expected = if meta.flags & 8 != 0 {
            1
        } else {
            16 - meta.width.max(meta.height).leading_zeros() as u8
        };
        validate_gpu_pixels(meta, raw)?;
        if meta.flags & !15 != 0 || meta.extra_types.contains(&1)
            || (meta.display_width, meta.display_height) != (meta.width, meta.height)
            || meta.mips != expected || !meta.width.is_power_of_two() || !meta.height.is_power_of_two()
            || ![1, 2, 4, 20, 28].contains(&meta.format)
        {
            texture_write::output_budget(meta)?;
            Ok("panorama_texture")
        } else {
            Ok("native_vtex")
        }
    }
}

fn png_contract(raw: &[u8]) -> Result<bool> {
    if !raw.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err(fail("Invalid PNG signature."));
    }
    let mut pos = 8;
    let mut icc = false;
    let mut first = true;
    let mut image_data = false;
    let mut frame_control = false;
    while pos < raw.len() {
        let header = raw
            .get(pos..pos + 8)
            .ok_or_else(|| fail("Truncated PNG chunk."))?;
        let size = u32::from_be_bytes(header[..4].try_into().unwrap()) as usize;
        let end = pos
            .checked_add(12)
            .and_then(|p| p.checked_add(size))
            .ok_or_else(|| fail("PNG chunk overflow"))?;
        let chunk = raw
            .get(pos + 4..end)
            .ok_or_else(|| fail("Truncated PNG chunk pixels."))?;
        if crc32fast::hash(&chunk[..chunk.len() - 4])
            != u32::from_be_bytes(chunk[chunk.len() - 4..].try_into().unwrap())
        {
            return Err(fail("PNG chunk CRC mismatch."));
        }
        let kind = &header[4..8];
        if first {
            if kind != b"IHDR" || size != 13 {
                return Err(fail("Missing PNG dimensions."));
            }
            dimensions(
                u32::from_be_bytes(chunk[4..8].try_into().unwrap()),
                u32::from_be_bytes(chunk[8..12].try_into().unwrap()),
            )?;
            first = false;
        }
        if kind == b"acTL" {
            if size != 8 || u32::from_be_bytes(chunk[4..8].try_into().unwrap()) != 1 {
                return Err(fail("Choose a single, still PNG or JPEG image."));
            }
        }
        if kind == b"fcTL" {
            if image_data || frame_control {
                return Err(fail("Choose a single, still PNG or JPEG image."));
            }
            frame_control = true;
        }
        if kind == b"IDAT" {
            image_data = true;
        }
        if kind == b"iCCP" {
            icc = true;
        }
        if kind == b"IEND" {
            if size != 0 {
                return Err(fail("Malformed PNG end chunk."));
            }
            return Ok(icc);
        }
        pos = end;
    }
    Err(fail("Truncated PNG: no complete end chunk."))
}
fn color_convert(
    data: &[u8],
    layout: moxcms::Layout,
    profile: &[u8],
    width: u32,
    height: u32,
    cancel: &CancellationToken,
) -> Result<Vec<u8>> {
    if profile.len() > 16 * 1024 * 1024 {
        return Err(fail("Embedded color profile exceeds 16 MiB."));
    }
    let source = moxcms::ColorProfile::new_from_slice(profile).map_err(|e| {
        fail(format!(
            "The image color profile could not be converted: {e}"
        ))
    })?;
    let transform = source
        .create_transform_8bit(
            layout,
            &moxcms::ColorProfile::new_srgb(),
            moxcms::Layout::Rgba,
            moxcms::TransformOptions::default(),
        )
        .map_err(|e| {
            fail(format!(
                "The image color profile could not be converted: {e}"
            ))
        })?;
    let channels = match layout {
        moxcms::Layout::Gray => 1,
        moxcms::Layout::GrayAlpha => 2,
        moxcms::Layout::Rgb => 3,
        _ => 4,
    };
    let mut output = vec![0; width as usize * height as usize * 4];
    if data.len() != width as usize * height as usize * channels {
        return Err(fail("Color profile channel dimensions disagree."));
    }
    for (src, dst) in data
        .chunks_exact(width as usize * channels)
        .zip(output.chunks_exact_mut(width as usize * 4))
    {
        cancel.check()?;
        transform.transform(src, dst).map_err(|e| {
            fail(format!(
                "The image color profile could not be converted: {e}"
            ))
        })?;
    }
    Ok(output)
}
pub fn decode_image(raw: &[u8], cancel: &CancellationToken) -> Result<(RgbaImage, ImageInfo)> {
    cancel.check()?;
    if raw.is_empty() || raw.len() > MAX_IMAGE_BYTES {
        return Err(fail("Choose a PNG or JPEG smaller than 64 MiB."));
    }
    let format = image::guess_format(raw).map_err(fail)?;
    let (mut image, profile_converted, has_alpha) = match format {
        image::ImageFormat::Png => {
            let profile_present = png_contract(raw)?;
            let mut reader = image::ImageReader::with_format(Cursor::new(raw), format);
            let mut limits = image::Limits::default();
            limits.max_alloc = Some(384 * 1024 * 1024);
            reader.limits(limits);
            let mut decoder = reader.into_decoder().map_err(fail)?;
            let (width, height) = decoder.dimensions();
            dimensions(width, height)?;
            let has_alpha = decoder.color_type().has_alpha();
            let profile = decoder.icc_profile().map_err(fail)?;
            if profile_present && profile.is_none() {
                return Err(fail("The image color profile could not be converted."));
            }
            let orientation = decoder
                .exif_metadata()
                .map_err(fail)?
                .as_deref()
                .and_then(Orientation::from_exif_chunk)
                .unwrap_or(Orientation::NoTransforms);
            let decoded = DynamicImage::from_decoder(decoder).map_err(fail)?;
            let mut converted = if let Some(profile) = &profile {
                let color = moxcms::ColorProfile::new_from_slice(profile)
                    .map_err(|e| fail(format!("Invalid image color profile: {e}")))?;
                let grayscale = matches!(
                    decoded.color(),
                    image::ColorType::L8
                        | image::ColorType::La8
                        | image::ColorType::L16
                        | image::ColorType::La16
                );
                if color.color_space
                    != if grayscale {
                        moxcms::DataColorSpace::Gray
                    } else {
                        moxcms::DataColorSpace::Rgb
                    }
                {
                    return Err(fail(
                        "The image color profile does not match its pixel channels.",
                    ));
                }
                let (pixels, layout) = if color.color_space == moxcms::DataColorSpace::Gray {
                    if has_alpha {
                        (
                            decoded.to_luma_alpha8().into_raw(),
                            moxcms::Layout::GrayAlpha,
                        )
                    } else {
                        (decoded.to_luma8().into_raw(), moxcms::Layout::Gray)
                    }
                } else {
                    (decoded.to_rgba8().into_raw(), moxcms::Layout::Rgba)
                };
                DynamicImage::ImageRgba8(
                    RgbaImage::from_raw(
                        width,
                        height,
                        color_convert(&pixels, layout, profile, width, height, cancel)?,
                    )
                    .ok_or_else(|| fail("Invalid converted image dimensions"))?,
                )
            } else {
                DynamicImage::ImageRgba8(decoded.to_rgba8())
            };
            converted.apply_orientation(orientation);
            (converted.to_rgba8(), profile.is_some(), has_alpha)
        }
        image::ImageFormat::Jpeg => {
            let mut decoder = jpeg_decoder::Decoder::new(Cursor::new(raw));
            decoder.set_max_decoding_buffer_size(384 * 1024 * 1024);
            decoder.read_info().map_err(fail)?;
            let info = decoder
                .info()
                .ok_or_else(|| fail("Missing JPEG dimensions"))?;
            let (width, height) = (info.width as u32, info.height as u32);
            dimensions(width, height)?;
            let pixels = decoder.decode().map_err(fail)?;
            let orientation = decoder
                .exif_data()
                .and_then(Orientation::from_exif_chunk)
                .unwrap_or(Orientation::NoTransforms);
            let profile = decoder.icc_profile();
            if raw.windows(12).any(|w| w == b"ICC_PROFILE\0") && profile.is_none() {
                return Err(fail("The JPEG color profile is incomplete or malformed."));
            }
            let (layout, pixels) = match info.pixel_format {
                jpeg_decoder::PixelFormat::L8 => (moxcms::Layout::Gray, pixels),
                jpeg_decoder::PixelFormat::RGB24 => (moxcms::Layout::Rgb, pixels),
                jpeg_decoder::PixelFormat::CMYK32 => (moxcms::Layout::Rgba, pixels),
                jpeg_decoder::PixelFormat::L16 => (
                    moxcms::Layout::Gray,
                    pixels
                        .chunks_exact(2)
                        .map(|b| (u16::from_ne_bytes([b[0], b[1]]) >> 8) as u8)
                        .collect(),
                ),
            };
            let rgba = if let Some(profile) = &profile {
                let color = moxcms::ColorProfile::new_from_slice(profile)
                    .map_err(|e| fail(format!("Invalid image color profile: {e}")))?;
                let expected = match info.pixel_format {
                    jpeg_decoder::PixelFormat::CMYK32 => moxcms::DataColorSpace::Cmyk,
                    jpeg_decoder::PixelFormat::RGB24 => moxcms::DataColorSpace::Rgb,
                    _ => moxcms::DataColorSpace::Gray,
                };
                if color.color_space != expected {
                    return Err(fail(
                        "The JPEG color profile does not match its pixel channels.",
                    ));
                }
                color_convert(&pixels, layout, profile, width, height, cancel)?
            } else {
                let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
                let channels = match info.pixel_format {
                    jpeg_decoder::PixelFormat::RGB24 => 3,
                    jpeg_decoder::PixelFormat::CMYK32 => 4,
                    _ => 1,
                };
                for pixel in pixels.chunks_exact(channels) {
                    match channels {
                        1 => rgba.extend_from_slice(&[pixel[0], pixel[0], pixel[0], 255]),
                        3 => rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]),
                        _ => {
                            let k = 255 - pixel[3] as u32;
                            rgba.extend_from_slice(&[
                                ((255 - pixel[0] as u32) * k / 255) as u8,
                                ((255 - pixel[1] as u32) * k / 255) as u8,
                                ((255 - pixel[2] as u32) * k / 255) as u8,
                                255,
                            ]);
                        }
                    }
                }
                rgba
            };
            let mut decoded = DynamicImage::ImageRgba8(
                RgbaImage::from_raw(width, height, rgba)
                    .ok_or_else(|| fail("Invalid JPEG pixel dimensions"))?,
            );
            decoded.apply_orientation(orientation);
            (decoded.to_rgba8(), profile.is_some(), false)
        }
        _ => return Err(fail("Choose a single, still PNG or JPEG image.")),
    };
    cancel.check()?;
    let info = ImageInfo {
        width: image.width(),
        height: image.height(),
        format: if format == image::ImageFormat::Png {
            "PNG"
        } else {
            "JPEG"
        }
        .into(),
        has_alpha,
        color_space: "sRGB".into(),
        profile_converted,
        sha256: sha(raw),
        source: String::new(),
    };
    // Color transforms must never modify source opacity.
    image
        .set_color_space(image::metadata::Cicp::SRGB)
        .map_err(fail)?;
    Ok((image, info))
}
pub fn inspect_image(path: &Path, cancel: &CancellationToken) -> Result<ImageInfo> {
    if !path
        .extension()
        .and_then(|s| s.to_str())
        .is_some_and(|s| ["png", "jpg", "jpeg"].contains(&casefold(s).as_str()))
    {
        return Err(fail("Choose an existing PNG or JPEG image file."));
    }
    let raw = read_file(path)?;
    let (_, mut info) = decode_image(&raw, cancel)?;
    info.source = deadlock_project::asset_paths::resolve(path)
        .map_err(fail)?
        .to_string_lossy()
        .into_owned();
    Ok(info)
}

/// Read-only previews also cover flat textures that cannot be safely replaced.
/// This never relaxes the stricter compilation checks in `method`.
pub fn preview_texture(raw: &[u8], max_dimension: u32, cancel: &CancellationToken) -> Result<Vec<u8>> {
    let meta = texture_metadata(raw)?;
    let (width, height) = (u32::from(meta.width), u32::from(meta.height));
    dimensions(width, height)?;
    if meta.depth != 1 || meta.flags & (16 | 32 | 64) != 0 {
        return Err(fail("This texture needs a cube, volume or array viewer."));
    }
    let image = if [15, 16, 17, 18].contains(&meta.format) {
        decode_image(raw.get(meta.payload_offset..).ok_or_else(|| fail("Missing image payload"))?, cancel)?.0
    } else {
        if ![1, 2, 3, 4, 20, 21, 22, 27, 28, 32, 33].contains(&meta.format) {
            return Err(fail("A preview is not available for this texture format."));
        }
        validate_gpu_pixels(&meta, raw)?;
        let channels = match meta.format {3 | 32 | 33 => 1, 22 => 2, _ => 4};
        let expected = if [1, 2, 20, 21, 27].contains(&meta.format) {
            width.div_ceil(4) as usize * height.div_ceil(4) as usize * if [1, 27].contains(&meta.format) {8} else {16}
        } else {width as usize * height as usize * channels};
        let stored = meta.mip_sizes.first().copied().map(|n| n as usize).unwrap_or(expected);
        let packed = &raw[raw.len()-stored..];
        let pixels = if stored < expected {
            lz4_flex::block::decompress(packed, expected).map_err(fail)?
        } else {packed.to_vec()};
        let mut out = RgbaImage::new(width, height);
        if [3, 4, 22, 28, 32, 33].contains(&meta.format) {
            for (dst, src) in out.pixels_mut().zip(pixels.chunks_exact(channels)) {
                *dst = image::Rgba(match meta.format {
                    3 | 33 => [src[0], src[0], src[0], 255],
                    22 => [src[0], src[0], src[0], src[1]],
                    28 => [src[2], src[1], src[0], src[3]],
                    32 => [255, 255, 255, src[0]],
                    _ => [src[0], src[1], src[2], src[3]],
                });
            }
        } else {
            let stride = if [1, 27].contains(&meta.format) {8} else {16};
            for (index, block) in pixels.chunks_exact(stride).enumerate() {
                if index % 1024 == 0 {cancel.check()?;}
                let mut rgba = [0u8; 64];
                match meta.format {
                    1 => bcdec_rs::bc1(block, &mut rgba, 16),
                    2 => bcdec_rs::bc3(block, &mut rgba, 16),
                    20 => bcdec_rs::bc7(block, &mut rgba, 16),
                    27 => {
                        let mut r = [0; 16]; bcdec_rs::bc4(block, &mut r, 4, false);
                        for (p, v) in rgba.chunks_exact_mut(4).zip(r) {p.copy_from_slice(&[v, v, v, 255]);}
                    }
                    21 => {
                        let mut rg = [0; 32]; bcdec_rs::bc5(block, &mut rg, 8, false);
                        for (p, v) in rgba.chunks_exact_mut(4).zip(rg.chunks_exact(2)) {p.copy_from_slice(&[v[0], v[1], 0, 255]);}
                    }
                    _ => unreachable!(),
                }
                let x = (index as u32 % width.div_ceil(4)) * 4;
                let y = (index as u32 / width.div_ceil(4)) * 4;
                for row in 0..4.min(height-y) { for col in 0..4.min(width-x) {
                    let i = ((row*4+col)*4) as usize;
                    out.put_pixel(x+col, y+row, image::Rgba(rgba[i..i+4].try_into().unwrap()));
                }}
            }
        }
        if meta.flags & 512 != 0 && meta.flags & 256 == 0 {
            for pixel in out.pixels_mut() {for v in &mut pixel.0[..3] {
                let l = *v as f32 / 255.;
                let s = if l <= 0.0031308 {12.92 * l} else {1.055 * l.powf(1. / 2.4) - 0.055};
                *v = (s * 255.).round().clamp(0., 255.) as u8;
            }}
        }
        // Keep the original display rectangle, including non-power-of-two UI art.
        image::imageops::crop_imm(&out, 0, 0, u32::from(meta.display_width).min(width), u32::from(meta.display_height).min(height)).to_image()
    };
    thumbnail(image, max_dimension, cancel)
}

fn thumbnail(image: RgbaImage, max_dimension: u32, cancel: &CancellationToken) -> Result<Vec<u8>> {
    cancel.check()?;
    let maximum = max_dimension.clamp(32, 1024);
    let scale = (maximum as f64 / image.width().max(image.height()) as f64).min(1.);
    let mut png = vec![];
    image::codecs::png::PngEncoder::new(&mut png).write_image(image.as_raw(), image.width(), image.height(), image::ExtendedColorType::Rgba8).map_err(fail)?;
    resize_png(&png, (image.width() as f64*scale).round().max(1.) as u32,
        (image.height() as f64*scale).round().max(1.) as u32, "contain", cancel).map(|v| v.0)
}

pub fn preview_fitted_image(path: &Path, original: &[u8], fit: &str, max_dimension: u32, cancel: &CancellationToken) -> Result<Vec<u8>> {
    let meta = texture_metadata(original)?;
    let info = inspect_image(path, cancel)?;
    let raw = read_file(path)?;
    if sha(&raw) != info.sha256 {return Err(fail("Image changed during preview."));}
    let (png, _) = resize_png(&raw, meta.display_width.into(), meta.display_height.into(), fit, cancel)?;
    thumbnail(decode_image(&png, cancel)?.0, max_dimension, cancel)
}
pub fn resize_png(
    raw: &[u8],
    width: u32,
    height: u32,
    fit: &str,
    cancel: &CancellationToken,
) -> Result<(Vec<u8>, ImageInfo)> {
    dimensions(width, height)?;
    let (image, info) = decode_image(raw, cancel)?;
    if !["contain", "cover", "stretch"].contains(&fit) {
        return Err(fail(
            "Choose fit inside, fill/crop, or stretch for the icon.",
        ));
    }
    if image.dimensions() == (width, height) {
        let mut bytes = vec![];
        image::codecs::png::PngEncoder::new(&mut bytes)
            .write_image(
                image.as_raw(),
                width,
                height,
                image::ExtendedColorType::Rgba8,
            )
            .map_err(fail)?;
        return Ok((bytes, info));
    }
    let filter = image::imageops::FilterType::Lanczos3;
    // Filter premultiplied colors to retain transparent edge color correctly.
    let mut linear = image::Rgba32FImage::new(image.width(), image.height());
    for (dst, src) in linear.pixels_mut().zip(image.pixels()) {
        let a = src[3] as f32 / 255.;
        // Float image channels are normalized to 0..1. Supplying byte-scale
        // RGB here makes the resampler clamp colors and turns resized art dark.
        *dst = image::Rgba([src[0] as f32 / 255. * a, src[1] as f32 / 255. * a, src[2] as f32 / 255. * a, a]);
    }
    let resized = match fit {
        "stretch" => image::imageops::resize(&linear, width, height, filter),
        "cover" => {
            // Crop before scaling so extreme aspect ratios cannot allocate an
            // unbounded intermediate image outside the requested pixel budget.
            let ratio = width as f64 / height as f64;
            let (w, h) = if linear.width() as f64 / linear.height() as f64 > ratio {
                (
                    (linear.height() as f64 * ratio).round().max(1.) as u32,
                    linear.height(),
                )
            } else {
                (
                    linear.width(),
                    (linear.width() as f64 / ratio).round().max(1.) as u32,
                )
            };
            let cropped = image::imageops::crop_imm(
                &linear,
                (linear.width() - w) / 2,
                (linear.height() - h) / 2,
                w,
                h,
            )
            .to_image();
            image::imageops::resize(&cropped, width, height, filter)
        }
        "contain" => {
            let scale =
                (width as f64 / linear.width() as f64).min(height as f64 / linear.height() as f64);
            let w = (linear.width() as f64 * scale).round_ties_even().max(1.) as u32;
            let h = (linear.height() as f64 * scale).round_ties_even().max(1.) as u32;
            let scaled = image::imageops::resize(&linear, w, h, filter);
            let mut output = image::Rgba32FImage::new(width, height);
            image::imageops::replace(
                &mut output,
                &scaled,
                ((width - w) / 2) as i64,
                ((height - h) / 2) as i64,
            );
            output
        }
        _ => {
            return Err(fail(
                "Choose fit inside, fill/crop, or stretch for the icon.",
            ));
        }
    };
    cancel.check()?;
    let mut rgba = RgbaImage::new(width, height);
    for (dst, src) in rgba.pixels_mut().zip(resized.pixels()) {
        let a = src[3].clamp(0., 1.);
        let color = |v: f32| {
            if a > 0. {
                (v / a * 255.).round().clamp(0., 255.) as u8
            } else {
                0
            }
        };
        *dst = image::Rgba([
            color(src[0]),
            color(src[1]),
            color(src[2]),
            (a * 255.).round() as u8,
        ]);
    }
    let mut bytes = vec![];
    image::codecs::png::PngEncoder::new(&mut bytes)
        .write_image(
            rgba.as_raw(),
            width,
            height,
            image::ExtendedColorType::Rgba8,
        )
        .map_err(fail)?;
    Ok((bytes, info))
}

pub fn discover(
    archive: &Vpk,
    original_model: &str,
    cancel: &CancellationToken,
) -> Result<Vec<IconDiscovery>> {
    Ok(discover_with_warnings(archive,original_model,cancel)?.0)
}
pub fn discover_with_warnings(
    archive: &Vpk, original_model: &str, cancel: &CancellationToken,
) -> Result<(Vec<IconDiscovery>,Vec<String>)> {
    let paths = crate::icon_catalog::discover(archive,original_model,cancel)?;
    let mut result = vec![];
    for (path,shared) in paths.paths {
        cancel.check()?;
        let entry = &archive.entries[&casefold(&path)];
        let mut item = IconDiscovery {
            target: entry.path.clone(),
            path: entry.path.clone(),
            label: entry
                .path
                .rsplit('/')
                .next()
                .unwrap_or_default()
                .trim_end_matches(".vtex_c")
                .trim_end_matches(".vsvg_c")
                .replace('_', " "),
            supported: false,
            reason: String::new(),
            width: 0,
            height: 0,
            original_sha256: None,
            metadata: None,
            method: None,
            shared,
        };
        let outcome = (|| -> Result<()> {
            let raw = read(archive, &entry.path)?;
            item.original_sha256 = Some(sha(&raw));
            if casefold(&path).ends_with(".vsvg_c") {
                let preview = crate::vector_icons::preview(&raw)?;
                item.width = preview.width;
                item.height = preview.height;
                item.method = Some("vector_preview".into());
                return Err(fail("Vector image: preview available. Replacing compiled vector artwork is not supported yet."));
            }
            let meta = texture_metadata(&raw)?;
            item.width = meta.display_width;
            item.height = meta.display_height;
            item.metadata = Some(meta.clone());
            item.method = Some(method(&meta, &raw, cancel)?.into());
            Ok(())
        })();
        match outcome {
            Ok(()) => item.supported = true,
            Err(deadlock_core::CoreError::Cancelled) => {
                return Err(deadlock_core::CoreError::Cancelled);
            }
            Err(e) => item.reason = e.to_string(),
        };
        result.push(item);
    }
    Ok((result,paths.warnings))
}
fn inside(root: &Path, path: &str) -> Result<PathBuf> {
    deadlock_vpk::check_unlinked_path(root, true).map_err(fail)?;
    let path = safety::relative_asset(path)?;
    deadlock_vpk::check_unlinked_path(&root.join(&path), true).map_err(fail)?;
    safety::inside(root, &path)
}
pub fn stage(
    archive: &Vpk,
    replacements: &[ResourceReplacement],
    content: &Path,
    original_model: &str,
    cancel: &CancellationToken,
) -> Result<IconReceipt> {
    let allowed = if original_model.is_empty() {
        None
    } else {
        Some(
            discover(archive, original_model, cancel)?
                .into_iter()
                .map(|r| (casefold(&r.target),r.shared))
                .collect::<std::collections::BTreeMap<_,_>>(),
        )
    };
    let mut seen = BTreeSet::new();
    let mut entries = vec![];
    for replacement in replacements.iter().filter(|r| r.enabled.unwrap_or(true)) {
        cancel.check()?;
        let target = safety::relative_asset(replacement.target.as_deref().unwrap_or_default())?;
        let key = casefold(&target);
        if !seen.insert(key.clone()) {
            return Err(fail(format!(
                "Two image replacements target the same game image: {target}"
            )));
        }
        if !key.starts_with("panorama/images/")
            || !key.ends_with(".vtex_c")
            || !archive.entries.contains_key(&key)
            || allowed.as_ref().is_some_and(|a| !a.contains_key(&key))
        {
            return Err(fail(format!(
                "Choose an image discovered for the selected hero in the current game VPK: {target}"
            )));
        }
        if allowed.as_ref().and_then(|a|a.get(&key)).copied().unwrap_or(false) && !replacement.allow_shared.unwrap_or(false) {
            return Err(fail("This icon is shared. Enable Include this shared replacement before building."));
        }
        let target = archive.entries[&key].path.clone();
        let original = read(archive, &target)?;
        let original_sha256 = sha(&original);
        if replacement
            .original_sha256
            .as_ref()
            .is_some_and(|s| !s.is_empty() && *s != original_sha256)
        {
            return Err(fail(
                "The game image changed after selection. Find this hero's images again.",
            ));
        }
        let meta = texture_metadata(&original)?;
        let method = method(&meta, &original, cancel)?.to_owned();
        let source = Path::new(replacement.source.as_deref().unwrap_or_default());
        let source_info = inspect_image(source, cancel)?;
        let raw = read_file(source)?;
        if sha(&raw) != source_info.sha256 {
            return Err(fail(
                "The icon changed while being imported. Drop it again.",
            ));
        }
        let fit = replacement.fit.as_deref().unwrap_or("contain");
        let (png, _) = resize_png(
            &raw,
            meta.display_width as u32,
            meta.display_height as u32,
            fit,
            cancel,
        )?;
        let tag = &sha(target.as_bytes())[..16];
        let image_rel = format!("panorama/images/dmb_sources/{tag}.png");
        safety::atomic_write(&inside(content, &image_rel)?, &png)?;
        let mut item = IconEntry {
            target: target.clone(),
            method: method.clone(),
            source: source_info.source.clone(),
            source_sha256: source_info.sha256.clone(),
            source_image: source_info,
            prepared_image: image_rel.clone(),
            prepared_sha256: sha(&png),
            fit: fit.into(),
            original_sha256,
            original_metadata: meta.clone(),
            allow_shared: replacement.allow_shared.unwrap_or(false),
            source_vtex: String::new(),
            prepared_resource: None,
            prepared_resource_sha256: None,
            descriptor_sha256: None,
            compiled_sha256: None,
            metadata_verified: false,
        };
        if ["embedded_png", "panorama_texture"].contains(&method.as_str()) {
            let compiled = if method == "panorama_texture" {
                texture_write::build(&original, &png, cancel)?
            } else {
                let mut compiled = original[..meta.payload_offset].to_vec();
                compiled.extend(&png);
                compiled
            };
            let prepared = format!("panorama/images/dmb_sources/{tag}.vtex_c");
            safety::atomic_write(&inside(content, &prepared)?, &compiled)?;
            item.prepared_resource = Some(prepared);
            item.prepared_resource_sha256 = Some(sha(&compiled));
        } else {
            item.source_vtex = target.strip_suffix("_c").unwrap_or(&target).into();
            let descriptor = texture_source(&image_rel, &meta)?;
            safety::atomic_write(&inside(content, &item.source_vtex)?, descriptor.as_bytes())?;
            item.descriptor_sha256 = Some(sha(descriptor.as_bytes()));
        }
        if safety::digest_with_cancel(source, cancel)? != item.source_sha256 {
            return Err(fail("Source icon changed during preparation."));
        }
        entries.push(item);
    }
    Ok(IconReceipt {
        version: 1,
        entries,
        native_validated: false,
        in_game_validated: false,
    })
}
pub fn texture_source(image_path: &str, metadata: &TextureMetadata) -> Result<String> {
    let path = serde_json::to_string(&safety::relative_asset(image_path)?).map_err(fail)?;
    let format = format_name(metadata.format).ok_or_else(|| fail("Unsupported icon format"))?;
    let clamp = [1, 2, 4]
        .map(|bit| if metadata.flags & bit != 0 { "1" } else { "0" })
        .join(" ");
    let no_lod = if metadata.flags & 8 != 0 { "1" } else { "0" };
    Ok(format!(
        include_str!("icon_recipe.txt"),
        image_path = path,
        format = format,
        clamp = clamp,
        no_lod = no_lod
    ))
}
pub fn verify_compiled(
    receipt: &mut IconReceipt,
    game: &Path,
    cancel: &CancellationToken,
) -> Result<Vec<String>> {
    if receipt.version != 1 {
        return Err(fail("Unsupported icon receipt version."));
    }
    let mut targets = vec![];
    let mut seen = BTreeSet::new();
    for item in &mut receipt.entries {
        cancel.check()?;
        let key = casefold(&item.target);
        if !key.starts_with("panorama/images/") || !key.ends_with(".vtex_c") {
            return Err(fail("Icon receipt target is not a Panorama image."));
        }
        if !seen.insert(casefold(&item.target)) {
            return Err(fail("Duplicated icon target in receipt."));
        }
        let raw = read_file(&inside(game, &item.target)?)?;
        let meta = texture_metadata(&raw)?;
        let mut expected = item.original_metadata.clone();
        if item.method == "panorama_texture" {
            expected.flags = texture_write::output_flags(&item.original_metadata);
            expected.format = texture_write::output_format(&item.original_metadata);
            expected.extra_types.retain(|kind| *kind == 3);
            if meta.extra_types != expected.extra_types {
                return Err(fail("Replacement icon runtime metadata changed."));
            }
        }
        let old = &expected;
        if (
            meta.version,
            meta.flags,
            meta.width,
            meta.height,
            meta.display_width,
            meta.display_height,
            meta.depth,
            meta.format,
            meta.mips,
            meta.picmip,
            meta.motion_distance,
        ) != (
            old.version,
            old.flags,
            old.width,
            old.height,
            old.display_width,
            old.display_height,
            old.depth,
            old.format,
            old.mips,
            old.picmip,
            old.motion_distance,
        ) || meta.range_values != old.range_values
        {
            return Err(fail(format!(
                "Icon compiler changed required texture settings: {}",
                item.target
            )));
        }
        let actual_method = method(&meta, &raw, cancel)?;
        if item.method != "panorama_texture" && item.method != actual_method {
            return Err(fail("Icon replacement method changed."));
        }
        if ["embedded_png", "panorama_texture"].contains(&item.method.as_str())
            && item.prepared_resource_sha256.as_deref() != Some(sha(&raw).as_str())
        {
            return Err(fail("Prepared image resource changed before packaging."));
        }
        if raw.len() <= meta.payload_offset {
            return Err(fail(
                "Icon compiler produced metadata but no texture pixels.",
            ));
        }
        item.compiled_sha256 = Some(sha(&raw));
        item.metadata_verified = true;
        targets.push(item.target.clone());
    }
    Ok(targets)
}
pub fn compile_entries(
    receipt: &mut IconReceipt,
    content: &Path,
    game: &Path,
    compile_resource: &mut dyn FnMut(&Path, &Path) -> Result<()>,
    cancel: &CancellationToken,
) -> Result<Vec<String>> {
    for item in &receipt.entries {
        cancel.check()?;
        for (path, expected) in item.prepared_inputs() {
            if safety::digest_with_cancel(&inside(content, path)?, cancel)? != expected {
                return Err(fail(
                    "Prepared icon or texture settings changed. Build again from the original image.",
                ));
            }
        }
        let output = inside(game, &item.target)?;
        if output.exists() {
            std::fs::remove_file(&output).map_err(fail)?;
        }
        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent).map_err(fail)?;
        }
        match item.method.as_str() {
            "embedded_png" | "panorama_texture" => {
                let source = item
                    .prepared_resource
                    .as_deref()
                    .ok_or_else(|| fail("Missing prepared icon resource"))?;
                safety::atomic_write(&output, &read_file(&inside(content, source)?)?)?;
            }
            "native_vtex" => compile_resource(&inside(content, &item.source_vtex)?, &output)?,
            _ => return Err(fail("Unknown icon replacement method.")),
        }
    }
    verify_compiled(receipt, game, cancel)
}

/// Bounded PNG IPC thumbnail, normalized using the same EXIF/ICC checks.
pub fn preview_image(
    path: &Path,
    max_dimension: u32,
    cancel: &CancellationToken,
) -> Result<Vec<u8>> {
    if !(1..=1024).contains(&max_dimension) {
        return Err(fail("Thumbnail maximum dimension must be 1 through 1024."));
    }
    let info = inspect_image(path, cancel)?;
    let raw = read_file(path)?;
    if sha(&raw) != info.sha256 {
        return Err(fail("Image changed during preview."));
    }
    let scale = (max_dimension as f64 / info.width as f64)
        .min(max_dimension as f64 / info.height as f64)
        .min(1.);
    let width = (info.width as f64 * scale).round().max(1.) as u32;
    let height = (info.height as f64 * scale).round().max(1.) as u32;
    let (png, _) = resize_png(&raw, width, height, "contain", cancel)?;
    cancel.check()?;
    Ok(png)
}
