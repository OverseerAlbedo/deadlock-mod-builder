//! Hero color ranges on a compiled base-color texture (`.vtex_c`): every mip and
//! the low-resolution fallback are recolored from their own decoded texels, so no
//! mip is filtered again. Only compressed blocks whose texels change are encoded
//! again (`block_refit`, alpha byte-exact); every other byte, including the
//! header flags, format, mip count, editor data and the display rectangle, stays
//! the game's. The header's four reflectivity values (the mean linear color) move
//! by the change of the recolored top mip. The material then names the new
//! texture (`point_texture`) under `TEXTURE_FOLDER`, a folder the game does not use.
use crate::{
    CancellationToken, Result,
    block_refit::{self, RefitError, Texels},
    color_ranges::{self, ColorRange},
    fail, icons,
};
use deadlock_core::text::casefold;
use deadlock_resource::{Kind, Value};

pub const TEXTURE_FOLDER: &str = "materials/dmb_hero_colors";
const BC7: u8 = 20;
const FALLBACK: u32 = 1;
const DISPLAY_RECT: u32 = 3;

#[derive(Debug, Clone, PartialEq)]
pub struct RecoloredTexture {
    pub bytes: Vec<u8>,
    pub blocks: usize,
    pub changed_blocks: usize,
}

fn dword(raw: &[u8], at: usize) -> Result<u32> {
    raw.get(at..at + 4)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
        .ok_or_else(|| fail("The base-color texture header is truncated."))
}
fn mip_bytes(meta: &icons::TextureMetadata, level: u8) -> usize {
    let w = (usize::from(meta.width) >> level).max(1);
    let h = (usize::from(meta.height) >> level).max(1);
    w.div_ceil(4) * h.div_ceil(4) * 16
}
fn linear(value: u8) -> f64 {
    let s = value as f64 / 255.;
    if s <= 0.04045 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) }
}

/// Why a texture cannot take color ranges, or `None` when it can.
pub fn unsupported(raw: &[u8]) -> Option<String> {
    let meta = match icons::texture_metadata(raw) {
        Ok(meta) => meta,
        Err(error) => return Some(format!("The base-color texture could not be read: {error}")),
    };
    if meta.format != BC7 {
        return Some(format!("Color ranges need a BC7 base-color texture; this one is {}.", meta.format_name));
    }
    if meta.depth != 1 || meta.flags & (16 | 32 | 64) != 0 {
        return Some("Color ranges need a flat base-color texture, not a cube, volume or array.".into());
    }
    if meta.extra_types.iter().any(|kind| ![FALLBACK, DISPLAY_RECT].contains(kind)) || !meta.mip_sizes.is_empty() {
        return Some("The base-color texture stores data that color ranges cannot keep.".into());
    }
    if let Err(error) = icons::validate_gpu_pixels(&meta, raw) {
        return Some(format!("The base-color texture has an unexpected layout: {error}"));
    }
    None
}

/// Where the fallback blocks (BC3) are, when the texture has them.
fn fallback(raw: &[u8], data: usize, data_end: usize) -> Result<Option<(usize, usize)>> {
    let table = data + 32 + dword(raw, data + 32)? as usize;
    for i in 0..dword(raw, data + 36)? as usize {
        let pos = table + i * 12;
        if dword(raw, pos)? == FALLBACK {
            let start = pos + 4 + dword(raw, pos + 4)? as usize;
            let size = dword(raw, pos + 8)? as usize;
            if size % 16 != 0 || start + size > data_end {
                return Err(fail("The base-color texture's fallback image has an unexpected size."));
            }
            return Ok(Some((start, size)));
        }
    }
    Ok(None)
}

fn refit_error(error: RefitError) -> crate::CoreError {
    fail(match error {
        RefitError::Alpha => "A recolored texture block would change the material's alpha (metalness).",
        RefitError::Decoder => "A base-color texture block could not be read for recoloring.",
    })
}

/// The texture with `ranges` applied to every mip and to its fallback image.
pub fn recolor_texture(raw: &[u8], ranges: &[ColorRange], cancel: &CancellationToken) -> Result<RecoloredTexture> {
    if let Some(reason) = unsupported(raw) {
        return Err(fail(reason));
    }
    for range in ranges {
        range.validate()?;
    }
    let meta = icons::texture_metadata(raw)?;
    let layout = deadlock_vpk::resource_blocks(raw).map_err(fail)?;
    let data = layout.blocks.iter().find(|b| b.name == "DATA").ok_or_else(|| fail("The texture has no DATA block."))?;
    let mut out = raw.to_vec();
    let mut stats = RecoloredTexture { bytes: vec![], blocks: 0, changed_blocks: 0 };
    let mut refit = |bytes: &mut [u8], bc3: bool, sums: Option<&mut [[f64; 3]; 2]>| -> Result<()> {
        let mut sums = sums;
        for (index, chunk) in bytes.chunks_exact_mut(16).enumerate() {
            if index % 4096 == 0 {
                cancel.check()?;
            }
            let block: [u8; 16] = (&*chunk).try_into().unwrap();
            let original: Texels = if bc3 { block_refit::decode_bc3(&block) } else { block_refit::decode_bc7(&block) };
            let target = original.map(|texel| color_ranges::recolor(ranges, texel));
            stats.blocks += 1;
            if target != original {
                stats.changed_blocks += 1;
                let new = if bc3 { block_refit::refit_bc3(&block, &target) } else { block_refit::refit_bc7(&block, &target) }
                    .map_err(refit_error)?;
                chunk.copy_from_slice(&new);
            }
            if let Some(sums) = sums.as_deref_mut() {
                let after = if target == original {
                    original
                } else {
                    let block: [u8; 16] = (&*chunk).try_into().unwrap();
                    block_refit::decode_bc7(&block)
                };
                for (sum, texels) in sums.iter_mut().zip([&original, &after]) {
                    for texel in texels {
                        for (c, value) in sum.iter_mut().enumerate() {
                            *value += linear(texel[c]);
                        }
                    }
                }
            }
        }
        Ok(())
    };
    // Mips are stored smallest first; the last one is the top mip.
    let mut offset = meta.payload_offset;
    let mut sums = [[0.; 3]; 2];
    for level in (0..meta.mips).rev() {
        let size = mip_bytes(&meta, level);
        let top = level == 0;
        refit(&mut out[offset..offset + size], false, top.then_some(&mut sums))?;
        offset += size;
    }
    if let Some((start, size)) = fallback(raw, data.offset, data.offset + data.size)? {
        refit(&mut out[start..start + size], true, None)?;
    }
    // Reflectivity: the game's values moved by the change of the mean linear color.
    let texels = (usize::from(meta.width).div_ceil(4) * usize::from(meta.height).div_ceil(4) * 16) as f64;
    for c in 0..3 {
        let at = data.offset + 4 + c * 4;
        let value = f32::from_le_bytes(raw[at..at + 4].try_into().unwrap()) as f64;
        let moved = (value + (sums[1][c] - sums[0][c]) / texels).max(0.) as f32;
        if !moved.is_finite() {
            return Err(fail("The recolored texture's reflectivity is not a number."));
        }
        out[at..at + 4].copy_from_slice(&moved.to_le_bytes());
    }
    if out.len() != raw.len() || icons::texture_metadata(&out)? != meta {
        return Err(fail("The recolored texture changed outside its colors."));
    }
    stats.bytes = out;
    Ok(stats)
}

/// The package path of a recolored texture: `TEXTURE_FOLDER`, the game
/// texture's name and a fingerprint of everything that made it.
pub fn texture_path(original: &str, fingerprint: &str) -> Result<String> {
    let name = original.rsplit('/').next().unwrap_or(original);
    let name = name.strip_suffix("_c").unwrap_or(name);
    let stem = name.strip_suffix(".vtex").unwrap_or(name);
    let stem: String = casefold(stem)
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' })
        .take(64)
        .collect();
    let tag = fingerprint.get(..16).filter(|t| t.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or_else(|| fail("A recolored texture needs a fingerprint."))?;
    Ok(format!("{TEXTURE_FOLDER}/{stem}_{tag}.vtex"))
}

/// Point the material's texture parameter `param` at `path` (a `.vtex`). Returns
/// the texture it named before.
pub fn point_texture(root: &mut Value, param: &str, path: &str) -> Result<String> {
    let Some(Kind::Array(rows)) = root.get_mut("m_textureParams").map(|v| &mut v.kind) else {
        return Err(fail("The material has no texture parameters."));
    };
    let row = rows
        .iter_mut()
        .find(|row| row.get("m_name").and_then(|v| v.string().ok()) == Some(param))
        .ok_or_else(|| fail(format!("The material has no {param} texture.")))?;
    let value = row.get_mut("m_pValue").ok_or_else(|| fail(format!("The material's {param} has no texture.")))?;
    let Kind::String(old) = &mut value.kind else {
        return Err(fail(format!("The material's {param} is not a texture path.")));
    };
    Ok(std::mem::replace(old, path.to_owned()))
}

/// Every texture path the material's parameters name.
pub fn texture_paths(root: &Value) -> Vec<String> {
    root.get("m_textureParams")
        .and_then(|v| v.array().ok())
        .unwrap_or(&[])
        .iter()
        .filter_map(|row| row.get("m_pValue").and_then(|v| v.string().ok()).map(str::to_owned))
        .collect()
}

/// The reference table (RERL payload) as (id, name) pairs.
pub fn references(payload: &[u8]) -> Result<Vec<(u64, String)>> {
    let bad = || fail("The material's reference table is invalid.");
    let start = dword(payload, 0)? as usize;
    let count = dword(payload, 4)? as usize;
    if count > 4096 || start.checked_add(count * 16).is_none_or(|end| end > payload.len()) {
        return Err(bad());
    }
    (0..count)
        .map(|i| {
            let at = start + i * 16;
            let id = u64::from_le_bytes(payload[at..at + 8].try_into().unwrap());
            let delta = u64::from_le_bytes(payload[at + 8..at + 16].try_into().unwrap());
            let name_at = (at + 8).checked_add(usize::try_from(delta).map_err(|_| bad())?).ok_or_else(bad)?;
            let tail = payload.get(name_at..).ok_or_else(bad)?;
            let end = tail.iter().position(|b| *b == 0).ok_or_else(bad)?;
            let name = std::str::from_utf8(&tail[..end]).map_err(|_| bad())?;
            Ok((id, name.to_owned()))
        })
        .collect()
}
/// A reference table payload laid out like the game's: the entries, then the names.
pub fn reference_table(entries: &[(u64, String)]) -> Vec<u8> {
    let mut out = vec![0u8; 8 + 16 * entries.len()];
    out[..4].copy_from_slice(&8u32.to_le_bytes());
    out[4..8].copy_from_slice(&(entries.len() as u32).to_le_bytes());
    for (i, (id, name)) in entries.iter().enumerate() {
        let at = 8 + i * 16;
        let delta = (out.len() - at - 8) as u64;
        out[at..at + 8].copy_from_slice(&id.to_le_bytes());
        out[at + 8..at + 16].copy_from_slice(&delta.to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.push(0);
    }
    out
}
/// The reference table after a texture parameter moved from `old` to `new`:
/// `old` is dropped when nothing in `still_named` uses it, and `new` is added in
/// name order (as the material compiler writes it).
pub fn moved_references(entries: &[(u64, String)], old: &str, new: &str, still_named: &[String]) -> Vec<(u64, String)> {
    let keep_old = still_named.iter().any(|n| casefold(n) == casefold(old));
    let mut out: Vec<_> = entries
        .iter()
        .filter(|(_, name)| keep_old || casefold(name) != casefold(old))
        .filter(|(_, name)| casefold(name) != casefold(new))
        .cloned()
        .collect();
    let position = out.iter().position(|(_, name)| casefold(name) > casefold(new)).unwrap_or(out.len());
    out.insert(position, (deadlock_vpk::resource_id(new), new.to_owned()));
    out
}
