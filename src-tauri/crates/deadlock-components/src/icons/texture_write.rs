//! Panorama replacement storage, independent of a game's texture compiler.
//! Preserve layout and runtime metadata. Fallback thumbnails, compressed-mip
//! sizes and compiler edit records describe the old pixels and are regenerated
//! or omitted, never copied over as though they described the replacement.
use super::*;

pub(super) fn output_format(meta: &TextureMetadata) -> u8 {
    match meta.format {15 | 16 => 16, 17 | 18 => 18, _ => 4}
}

pub(super) fn output_flags(meta: &TextureMetadata) -> u16 {
    if [16, 18].contains(&output_format(meta)) { return meta.flags; }
    // The RGBA payload is already decoded color, not dilated/YCoCg data.
    // YCoCg uses a linear API surface to prevent hardware sRGB conversion;
    // ordinary replacement color instead uses the standard sRGB surface.
    meta.flags & !(128 | 256 | if meta.flags & 256 != 0 {512} else {0})
}

pub(super) fn output_budget(meta: &TextureMetadata) -> Result<()> {
    let size: usize = (0..meta.mips).map(|m| {
        (usize::from(meta.width) >> m).max(1) * (usize::from(meta.height) >> m).max(1) * 4
    }).sum();
    if size > crate::MAX_RESOURCE_BYTES.saturating_sub(1024 * 1024) {
        return Err(fail("Replacement icon mipmaps exceed the 128 MiB texture budget."));
    }
    Ok(())
}

fn encode_png(image: &RgbaImage) -> Result<Vec<u8>> {
    let mut out = vec![];
    image::codecs::png::PngEncoder::new(&mut out).write_image(image.as_raw(), image.width(), image.height(), image::ExtendedColorType::Rgba8).map_err(fail)?;
    Ok(out)
}

pub(super) fn build(original: &[u8], png: &[u8], cancel: &CancellationToken) -> Result<Vec<u8>> {
    let meta = texture_metadata(original)?;
    output_budget(&meta)?;
    let layout = deadlock_vpk::resource_blocks(original).map_err(fail)?;
    let block = layout.blocks.iter().find(|b| b.name == "DATA").ok_or_else(|| fail("Missing icon DATA"))?;
    let source_data = &original[block.offset..block.offset + block.size];
    let format = output_format(&meta);
    let flags = output_flags(&meta);
    let (fitted, _) = decode_image(png, cancel)?;
    if fitted.dimensions() != (meta.display_width.into(), meta.display_height.into()) {
        return Err(fail("Prepared icon does not match the original display rectangle."));
    }
    let payload = if [16, 18].contains(&format) {
        png.to_vec()
    } else {
        // Content occupies the same top-left display rectangle; GPU alignment
        // padding stays transparent instead of stretching the visible image.
        let mut canvas = RgbaImage::new(meta.width.into(), meta.height.into());
        image::imageops::replace(&mut canvas, &fitted, 0, 0);
        let base = encode_png(&canvas)?;
        let mut payload = Vec::new();
        // Source 2 stores the smallest mip first. Generate only the original
        // number of levels, including intentionally shortened/custom chains.
        for level in (0..meta.mips).rev() {
            cancel.check()?;
            let (w, h) = ((u32::from(meta.width) >> level).max(1), (u32::from(meta.height) >> level).max(1));
            let (mip, _) = resize_png(&base, w, h, "stretch", cancel)?;
            let mut image = decode_image(&mip, cancel)?.0;
            if flags & 512 != 0 {
                // A genuinely linear original keeps its API color space.
                for pixel in image.pixels_mut() { for value in &mut pixel.0[..3] {
                    let s = *value as f32 / 255.;
                    let linear = if s <= 0.04045 {s / 12.92} else {((s + 0.055) / 1.055).powf(2.4)};
                    *value = (linear * 255.).round() as u8;
                }}
            }
            payload.extend(image.into_raw());
        }
        payload
    };

    // Rebuild the DATA extra table with the original display/range metadata.
    // FALLBACK_BITS is optional derived artwork; retaining it could briefly
    // display the OLD icon. Uncompressed output needs no compressed-mip table.
    let mut data = source_data[..40].to_vec();
    data[2..4].copy_from_slice(&flags.to_le_bytes());
    data[26] = format;
    let table = 32 + dword(source_data, 32)? as usize;
    let mut extras = Vec::new();
    for i in 0..dword(source_data, 36)? as usize {
        let pos = table + i * 12;
        let kind = dword(source_data, pos)?;
        if kind == 3 {
            let start = pos + 4 + dword(source_data, pos + 4)? as usize;
            let size = dword(source_data, pos + 8)? as usize;
            extras.push((kind, &source_data[start..start + size]));
        }
    }
    data[32..36].copy_from_slice(&8u32.to_le_bytes());
    data[36..40].copy_from_slice(&(extras.len() as u32).to_le_bytes());
    data.resize(40 + extras.len() * 12, 0);
    for (i, (kind, bytes)) in extras.iter().enumerate() {
        let pos = 40 + i * 12;
        let offset = data.len() - pos - 4;
        data[pos..pos + 4].copy_from_slice(&kind.to_le_bytes());
        data[pos + 4..pos + 8].copy_from_slice(&(offset as u32).to_le_bytes());
        data[pos + 8..pos + 12].copy_from_slice(&(bytes.len() as u32).to_le_bytes());
        data.extend_from_slice(bytes);
    }
    // Retain non-editor resource blocks. DATA must be last, immediately before
    // its external pixel stream. All block references are relative offsets.
    let mut blocks: Vec<(&str, &[u8])> = layout.blocks.iter()
        .filter(|b| !["DATA", "REDI", "RED2"].contains(&b.name.as_str()))
        .map(|b| (b.name.as_str(), &original[b.offset..b.offset + b.size])).collect();
    blocks.push(("DATA", &data));
    let mut out = original[..16].to_vec();
    out[8..12].copy_from_slice(&8u32.to_le_bytes());
    out[12..16].copy_from_slice(&(blocks.len() as u32).to_le_bytes());
    out.resize(16 + blocks.len() * 12, 0);
    for (i, (name, bytes)) in blocks.iter().enumerate() {
        out.resize(out.len().div_ceil(16) * 16, 0);
        let pos = 16 + i * 12;
        let offset = out.len() - pos - 4;
        out[pos..pos + 4].copy_from_slice(name.as_bytes());
        out[pos + 4..pos + 8].copy_from_slice(&(offset as u32).to_le_bytes());
        out[pos + 8..pos + 12].copy_from_slice(&(bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(bytes);
    }
    let declared = out.len() as u32;
    out[..4].copy_from_slice(&declared.to_le_bytes());
    out.extend(payload);
    if out.len() > crate::MAX_RESOURCE_BYTES {return Err(fail("Replacement icon exceeds the resource size budget."));}
    texture_metadata(&out)?;
    cancel.check()?;
    Ok(out)
}
