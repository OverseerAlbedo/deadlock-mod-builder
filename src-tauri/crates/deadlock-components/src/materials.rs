//! Hero colors: the hero's own compiled game materials recolored in place, for
//! projects without a replacement model. Only parameters the compiled material
//! already has are changed: the tint (`g_vColorTint1`) and the saturation and
//! brightness factors of its albedo adjustment
//! (`g_vAlbedoContrastSaturationBrightness1`, x contrast, y saturation, z
//! brightness). The DATA block is decoded, edited and encoded again; every other
//! block keeps its exact bytes (color ranges, below, also rewrite the reference
//! table), so the shader stays the game's own and, without ranges, so do the
//! textures and references, and the package holds only the material at its game path.
//! Roughness, metalness, opacity and emission are baked into textures or need
//! shader options these materials do not have, so they are not offered.
//!
//! Color ranges recolor only the base-color pixels they select (a real hue
//! shift): the game's `g_tColor` texture is recolored into a new texture under
//! `texture_recolor::TEXTURE_FOLDER` (`texture_recolor`, alpha byte-exact), and
//! the material's `g_tColor` and reference table (RERL) name it instead.
use crate::{
    CancellationToken, Result,
    color_ranges::ColorRange,
    compiled, fail, read, read_file, sha, texture_recolor,
};
use deadlock_core::{safety, text::casefold};
use deadlock_project::customization::HeroMaterial;
use deadlock_resource::{Kind, Value, binary, container};
use deadlock_vpk::Vpk;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

pub const RECOLOR_POLICY: &str = "compiled-material-parameters-v1";
/// Color ranges: `color_ranges`, `block_refit` and `texture_recolor` as they are now.
pub const RANGE_POLICY: &str = "hero-color-ranges-v1";
/// Shaders whose tint and albedo parameters were checked on the game's hero materials.
pub const SHADERS: [&str; 1] = ["pbr.vfx"];
const TINT: &str = "g_vColorTint1";
const ALBEDO: &str = "g_vAlbedoContrastSaturationBrightness1";
const TINT_MASK: &str = "g_tTintMaskRimLightMask";
const COLOR: &str = "g_tColor";

/// One of the hero's own materials as the Extras page lists it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MaterialRow {
    /// The material as the hero's model names it (`.vmat`).
    pub path: String,
    /// The compiled game file that a recolor replaces (`.vmat_c`).
    pub target: String,
    pub label: String,
    pub shader: String,
    pub supported: bool,
    pub reason: String,
    /// The game's tint (RGB) when the material has one.
    pub tint: Option<[f64; 3]>,
    /// The game's albedo contrast, saturation and brightness when the material has them.
    pub albedo: Option<[f64; 3]>,
    /// The tint applies only where the material's own tint mask allows it.
    pub tint_masked: bool,
    /// The compiled base-color texture, for the preview.
    pub color_texture: Option<String>,
    /// Other game models that use this material (a recolor changes them too).
    pub used_by: Vec<String>,
    pub shared: bool,
    pub original_sha256: String,
}

/// What one hero color changes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Recolor {
    pub color: Option<[f64; 3]>,
    pub saturation: f64,
    pub brightness: f64,
}
impl Recolor {
    pub fn from_row(row: &HeroMaterial) -> Self {
        Self {
            color: row.color,
            saturation: row.saturation.unwrap_or(1.),
            brightness: row.brightness.unwrap_or(1.),
        }
    }
    fn adjusts_albedo(&self) -> bool {
        self.saturation != 1. || self.brightness != 1.
    }
    pub fn is_empty(&self) -> bool {
        self.color.is_none() && !self.adjusts_albedo()
    }
    fn validate(&self) -> Result<()> {
        if self
            .color
            .is_some_and(|c| c.iter().any(|v| !v.is_finite() || !(0.0..=1.0).contains(v)))
        {
            return Err(fail("A hero color tint needs three values from 0 to 1."));
        }
        for (label, value) in [("Saturation", self.saturation), ("Brightness", self.brightness)] {
            if !value.is_finite() || !(0.0..=2.0).contains(&value) {
                return Err(fail(format!("{label} must be from 0 to 2.")));
            }
        }
        Ok(())
    }
}

/// The enabled color ranges of a hero color that change something, with defaults
/// for unset values.
pub fn ranges(row: &HeroMaterial) -> Vec<ColorRange> {
    let base = ColorRange::default();
    row.ranges
        .as_deref()
        .unwrap_or_default()
        .iter()
        .filter(|r| r.enabled.unwrap_or(true))
        .map(|r| ColorRange {
            hue: r.hue.unwrap_or(base.hue),
            hue_width: r.hue_width.unwrap_or(base.hue_width),
            saturation_range: r.saturation_range.unwrap_or(base.saturation_range),
            brightness_range: r.brightness_range.unwrap_or(base.brightness_range),
            softness: r.softness.unwrap_or(base.softness),
            hue_shift: r.hue_shift.unwrap_or(base.hue_shift),
            saturation: r.saturation.unwrap_or(base.saturation),
            brightness: r.brightness.unwrap_or(base.brightness),
        })
        .filter(|r| !r.changes_nothing())
        .collect()
}

/// The recolored base-color texture of one hero color with ranges.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct RangeTexture {
    pub policy: String,
    /// The game's compiled texture (`.vtex_c`) that `g_tColor` named.
    pub source: String,
    pub source_sha256: String,
    /// The packaged texture (`.vtex_c`) under `texture_recolor::TEXTURE_FOLDER`.
    pub target: String,
    pub output_sha256: String,
    pub ranges: Vec<ColorRange>,
    pub changed_blocks: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct MaterialEntry {
    pub target: String,
    pub original_sha256: String,
    pub output_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<[f64; 3]>,
    pub saturation: f64,
    pub brightness: f64,
    #[serde(default)]
    pub used_by: Vec<String>,
    pub allow_shared: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub texture: Option<RangeTexture>,
}

/// The packaged textures of hero colors with ranges (files the game does not have).
pub fn packaged_textures(receipt: &MaterialReceipt) -> Vec<String> {
    receipt.entries.iter().filter_map(|e| e.texture.as_ref().map(|t| t.target.clone())).collect()
}

/// Where the recolored texture goes: fixed by the material, the game texture and the ranges.
fn range_texture_path(material: &str, source: &str, source_sha256: &str, ranges: &[ColorRange]) -> Result<String> {
    let fingerprint = sha(
        format!(
            "{RANGE_POLICY}|{}|{}|{source_sha256}|{}",
            casefold(material),
            casefold(source),
            serde_json::to_string(ranges).map_err(fail)?
        )
        .as_bytes(),
    );
    texture_recolor::texture_path(source, &fingerprint)
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct MaterialReceipt {
    pub schema: u32,
    pub policy: String,
    pub hero_path: String,
    pub entries: Vec<MaterialEntry>,
}

fn label(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    let name = name.strip_suffix("_c").unwrap_or(name);
    name.strip_suffix(".vmat").unwrap_or(name).to_owned()
}
fn named<'a>(rows: &'a [Value], name: &str) -> Option<&'a Value> {
    rows.iter()
        .find(|row| row.get("m_name").and_then(|v| v.string().ok()) == Some(name))
}
fn rows<'a>(root: &'a Value, key: &str) -> &'a [Value] {
    root.get(key).and_then(|v| v.array().ok()).unwrap_or(&[])
}
fn vector(row: &Value) -> Option<[f64; 4]> {
    let values = row.get("m_value")?.array().ok()?;
    if values.len() != 4 {
        return None;
    }
    let mut out = [0.; 4];
    for (slot, value) in out.iter_mut().zip(values) {
        *slot = value.number().ok().filter(|v| v.is_finite())?;
    }
    Some(out)
}
fn texture(root: &Value, name: &str) -> Option<String> {
    named(rows(root, "m_textureParams"), name)?
        .get("m_pValue")?
        .string()
        .ok()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

struct Material {
    data: binary::Document,
    shader: String,
    tint: Option<[f64; 4]>,
    albedo: Option<[f64; 4]>,
}
fn material(raw: &[u8], name: &str) -> Result<Material> {
    let layout = container::layout(raw)
        .map_err(|_| fail(format!("{name} is not a complete compiled material.")))?;
    let block = container::unique(&layout.blocks, "DATA")
        .map_err(|_| fail(format!("{name} needs exactly one material DATA block.")))?;
    let data = binary::decode(container::payload(raw, block).map_err(fail)?)
        .map_err(|e| fail(format!("{name} could not be read as a compiled material: {e}")))?;
    let root = &data.root;
    if root.get("m_materialName").is_none() || root.get("m_vectorParams").is_none() {
        return Err(fail(format!("{name} is not a compiled material.")));
    }
    let shader = root
        .get("m_shaderName")
        .and_then(|v| v.string().ok())
        .unwrap_or_default()
        .to_owned();
    let vectors = rows(root, "m_vectorParams");
    let tint = named(vectors, TINT).and_then(vector);
    let albedo = named(vectors, ALBEDO).and_then(vector);
    Ok(Material { data, shader, tint, albedo })
}
fn supported(shader: &str) -> bool {
    SHADERS.iter().any(|s| casefold(s) == casefold(shader))
}
fn unsupported_shader(name: &str, shader: &str) -> String {
    format!(
        "{name} uses the {} shader. Hero colors support pbr.vfx materials only.",
        if shader.is_empty() { "unknown" } else { shader }
    )
}

/// Inspect one compiled material for the Extras list.
pub fn inspect(path: &str, raw: &[u8]) -> Result<MaterialRow> {
    let target = compiled(path)?;
    let name = label(path);
    let parsed = material(raw, &name)?;
    let root = &parsed.data.root;
    let tint_mask = texture(root, TINT_MASK);
    let reason = if !supported(&parsed.shader) {
        unsupported_shader(&name, &parsed.shader)
    } else if parsed.tint.is_none() && parsed.albedo.is_none() {
        format!("{name} has no tint, saturation or brightness setting to change.")
    } else {
        String::new()
    };
    Ok(MaterialRow {
        path: target.strip_suffix("_c").unwrap_or(&target).to_owned(),
        target: target.clone(),
        label: name,
        shader: parsed.shader.clone(),
        supported: reason.is_empty(),
        reason,
        tint: parsed.tint.map(|v| [v[0], v[1], v[2]]),
        albedo: parsed.albedo.map(|v| [v[0], v[1], v[2]]),
        // The game's default masks are plain white: the whole material takes the tint.
        tint_masked: tint_mask.is_some_and(|m| !casefold(&m).starts_with("materials/default/")),
        color_texture: texture(root, COLOR).map(|t| compiled(&t)).transpose()?,
        used_by: vec![],
        shared: false,
        original_sha256: sha(raw),
    })
}

fn set_vector(root: &mut Value, name: &str, change: impl Fn(usize, f64) -> f64) -> Result<bool> {
    let Some(Kind::Array(rows)) = root.get_mut("m_vectorParams").map(|v| &mut v.kind) else {
        return Ok(false);
    };
    let Some(row) = rows
        .iter_mut()
        .find(|row| row.get("m_name").and_then(|v| v.string().ok()) == Some(name))
    else {
        return Ok(false);
    };
    let Some(Kind::Array(values)) = row.get_mut("m_value").map(|v| &mut v.kind) else {
        return Ok(false);
    };
    if values.len() != 4 {
        return Ok(false);
    }
    for (index, value) in values.iter_mut().enumerate().take(3) {
        let next = change(index, value.number().map_err(fail)?);
        if !next.is_finite() {
            return Err(fail(format!("{name} would not stay a finite number.")));
        }
        // Keep each value's own number kind.
        value.kind = match value.kind {
            Kind::F32(_) => Kind::F32(next as f32),
            _ => Kind::F64(next),
        };
    }
    Ok(true)
}

/// The compiled material with `edit` applied. Refuses shaders and parameters
/// that are not verified, and never adds a parameter the material lacks.
pub fn recolor(raw: &[u8], name: &str, edit: &Recolor) -> Result<Vec<u8>> {
    edit_material(raw, name, edit, None)
}
/// `recolor`, with the material's base color (`g_tColor`) naming the recolored
/// texture `texture` (a `.vtex` path) in its parameters and its reference table.
/// The texture `g_tColor` named before leaves the table unless another
/// parameter still names it. Returns the material and that previous texture.
pub fn recolor_with_texture(raw: &[u8], name: &str, edit: &Recolor, texture: &str) -> Result<(Vec<u8>, String)> {
    let mut previous = String::new();
    let output = edit_material(raw, name, edit, Some((texture, &mut previous)))?;
    Ok((output, previous))
}
fn edit_material(raw: &[u8], name: &str, edit: &Recolor, texture: Option<(&str, &mut String)>) -> Result<Vec<u8>> {
    edit.validate()?;
    let name = label(name);
    if edit.is_empty() && texture.is_none() {
        return Err(fail(format!(
            "Hero color {name} changes nothing. Set a tint, saturation, brightness or a color range, or remove it."
        )));
    }
    let mut parsed = material(raw, &name)?;
    if !supported(&parsed.shader) {
        return Err(fail(unsupported_shader(&name, &parsed.shader)));
    }
    let root = &mut parsed.data.root;
    if let Some(color) = edit.color {
        if !set_vector(root, TINT, |index, _| color[index])? {
            return Err(fail(format!(
                "{name} has no tint setting. Use saturation and brightness instead."
            )));
        }
    }
    if edit.adjusts_albedo() {
        let factors = [1., edit.saturation, edit.brightness];
        if !set_vector(root, ALBEDO, |index, value| value * factors[index])? {
            return Err(fail(format!(
                "{name} has no saturation or brightness setting. Use Tint instead."
            )));
        }
    }
    let mut blocks = BTreeMap::new();
    let mut expected_references = None;
    if let Some((path, previous)) = texture {
        let old = texture_recolor::point_texture(root, COLOR, path)?;
        let layout = container::layout(raw).map_err(fail)?;
        let table = container::unique(&layout.blocks, "RERL")
            .map_err(|_| fail(format!("{name} needs exactly one reference table.")))?;
        let entries = texture_recolor::references(container::payload(raw, table).map_err(fail)?)?;
        if !entries.iter().any(|(_, n)| casefold(n) == casefold(&old)) {
            return Err(fail(format!("{name}'s reference table does not list its base color {old}.")));
        }
        let moved = texture_recolor::moved_references(&entries, &old, path, &texture_recolor::texture_paths(root));
        expected_references = Some(moved.iter().map(|(_, n)| n.clone()).collect::<Vec<_>>());
        blocks.insert("RERL".to_owned(), texture_recolor::reference_table(&moved));
        *previous = old;
    }
    let encoded = binary::encode(&parsed.data.root, parsed.data.format_guid).map_err(fail)?;
    blocks.insert("DATA".to_owned(), encoded);
    let output = container::rebuild(raw, &blocks).map_err(fail)?;
    // The new file must read back as the edited material, and every other block
    // must still be the game's exact bytes.
    let check = material(&output, &name)?;
    if !check.data.root.canonical_eq(&parsed.data.root) {
        return Err(fail(format!("{name} did not read back as the recolored material.")));
    }
    if let Some(names) = expected_references {
        if deadlock_vpk::resource_references(&output).map_err(fail)? != names {
            return Err(fail(format!("{name}'s reference table did not read back.")));
        }
    }
    let (before, after) = (container::layout(raw).map_err(fail)?, container::layout(&output).map_err(fail)?);
    if before.blocks.len() != after.blocks.len()
        || before.blocks.iter().zip(&after.blocks).any(|(a, b)| {
            a.name != b.name
                || (!blocks.contains_key(&a.name)
                    && container::payload(raw, a).ok() != container::payload(&output, b).ok())
        })
    {
        return Err(fail(format!("{name} changed outside its material settings.")));
    }
    Ok(output)
}

/// The recolored base-color texture of `row`'s ranges, written to `game`, and the
/// receipt record for it.
fn stage_texture(
    archive: &Vpk,
    row: &MaterialRow,
    ranges: &[ColorRange],
    game: &Path,
    cancel: &CancellationToken,
) -> Result<RangeTexture> {
    let source = row.color_texture.clone().ok_or_else(|| {
        fail(format!("{} has no base-color texture, so color ranges cannot change it.", row.label))
    })?;
    let raw = read(archive, &source)?;
    let source_sha256 = sha(&raw);
    if let Some(reason) = texture_recolor::unsupported(&raw) {
        return Err(fail(format!("Color ranges cannot change {}: {reason}", row.label)));
    }
    // Other errors, a cancel among them, pass through as they are.
    let recolored = texture_recolor::recolor_texture(&raw, ranges, cancel)?;
    if recolored.changed_blocks == 0 {
        return Err(fail(format!(
            "The color ranges of {} select none of its base color. Widen a range, or pick a color from the preview.",
            row.label
        )));
    }
    let path = range_texture_path(&row.target, &source, &source_sha256, ranges)?;
    let target = compiled(&path)?;
    if archive.entries.contains_key(&casefold(&target)) {
        return Err(fail(format!("The game already has a file at {target}; hero colors will not replace it.")));
    }
    safety::atomic_write(&safety::inside(game, &target)?, &recolored.bytes)?;
    Ok(RangeTexture {
        policy: RANGE_POLICY.into(),
        source,
        source_sha256,
        target,
        output_sha256: sha(&recolored.bytes),
        ranges: ranges.to_vec(),
        changed_blocks: recolored.changed_blocks,
    })
}
/// The material and texture a receipt entry expects, from the current game files.
fn expected_outputs(archive: &Vpk, entry: &MaterialEntry, original: &[u8], cancel: &CancellationToken) -> Result<(Vec<u8>, Option<Vec<u8>>)> {
    let edit = Recolor { color: entry.color, saturation: entry.saturation, brightness: entry.brightness };
    let Some(texture) = &entry.texture else {
        return Ok((recolor(original, &entry.target, &edit)?, None));
    };
    let name = label(&entry.target);
    if texture.policy != RANGE_POLICY {
        return Err(fail("Hero color ranges were built by another version. Build again."));
    }
    let raw = read(archive, &texture.source)?;
    if sha(&raw) != texture.source_sha256 {
        return Err(fail(format!("The game texture of {name} changed (a game update). Build again.")));
    }
    let path = range_texture_path(&entry.target, &texture.source, &texture.source_sha256, &texture.ranges)?;
    if casefold(&compiled(&path)?) != casefold(&texture.target) {
        return Err(fail(format!("The recolored texture of {name} is not where its receipt says.")));
    }
    let (material, previous) = recolor_with_texture(original, &entry.target, &edit, &path)?;
    if casefold(&compiled(&previous)?) != casefold(&texture.source) {
        return Err(fail(format!("{name} no longer names the base-color texture its colors were made from. Build again.")));
    }
    let recolored = texture_recolor::recolor_texture(&raw, &texture.ranges, cancel)?;
    Ok((material, Some(recolored.bytes)))
}

/// The resource references of one archive entry, reading only up to the end of
/// its reference table.
fn references(archive: &Vpk, path: &str, cancel: &CancellationToken) -> Result<Vec<String>> {
    /// Where the reference table is (start, length), once the block table has been read.
    fn table(data: &[u8]) -> Option<Option<(usize, usize)>> {
        let at = |p: usize| -> Option<usize> {
            Some(u32::from_le_bytes(data.get(p..p + 4)?.try_into().ok()?) as usize)
        };
        let blocks = 8 + at(8)?;
        for i in 0..at(12)?.min(64) {
            let p = blocks + i * 12;
            if data.get(p..p + 4)? == b"RERL" {
                return Some(Some((p + 4 + at(p + 4)?, at(p + 8)?)));
            }
        }
        Some(None)
    }
    let mut data = Vec::new();
    let mut enough = false;
    let result = archive.stream(
        path,
        false,
        &mut || if cancel.is_cancelled() { Err(deadlock_vpk::Error::Cancelled) } else { Ok(()) },
        &mut |chunk| {
            data.extend_from_slice(chunk);
            match table(&data) {
                Some(None) => enough = true,
                Some(Some((start, length))) if data.len() >= start.saturating_add(length) => enough = true,
                _ => return Ok(()),
            }
            // The rest of the model is not needed.
            Err(deadlock_vpk::Error::Cancelled)
        },
    );
    match result {
        Ok(()) => {}
        Err(deadlock_vpk::Error::Cancelled) if enough => {}
        Err(deadlock_vpk::Error::Cancelled) => {
            cancel.check()?;
            return Err(fail(format!("Reading {path} stopped.")));
        }
        Err(error) => return Err(fail(error)),
    }
    let Some(Some((start, length))) = table(&data) else {
        return Ok(vec![]);
    };
    let Some(payload) = data.get(start..start.saturating_add(length)) else {
        return Ok(vec![]);
    };
    // The reference table alone in a minimal resource, read by the usual parser
    // (its name offsets are relative to the table itself).
    let mut alone = Vec::with_capacity(28 + payload.len());
    alone.extend(((28 + payload.len()) as u32).to_le_bytes());
    alone.extend(12u16.to_le_bytes());
    alone.extend(0u16.to_le_bytes());
    alone.extend(8u32.to_le_bytes());
    alone.extend(1u32.to_le_bytes());
    alone.extend(b"RERL");
    alone.extend(8u32.to_le_bytes());
    alone.extend((payload.len() as u32).to_le_bytes());
    alone.extend(payload);
    Ok(deadlock_vpk::resource_references(&alone).unwrap_or_default())
}

/// Every game model other than `hero` that uses one of `wanted` (casefolded
/// `.vmat` paths), by material.
pub fn users(
    archive: &Vpk,
    hero: &str,
    wanted: &BTreeSet<String>,
    cancel: &CancellationToken,
) -> Result<BTreeMap<String, Vec<String>>> {
    let hero = casefold(&compiled(hero)?);
    let mut found = BTreeMap::<String, Vec<String>>::new();
    if wanted.is_empty() {
        return Ok(found);
    }
    for (key, entry) in &archive.entries {
        if !key.ends_with(".vmdl_c") || *key == hero {
            continue;
        }
        cancel.check()?;
        for reference in references(archive, &entry.path, cancel)? {
            let reference = casefold(reference.strip_suffix("_c").unwrap_or(&reference));
            if wanted.contains(&reference) {
                let models = found.entry(reference).or_default();
                if !models.contains(&entry.path) {
                    models.push(entry.path.clone());
                }
            }
        }
    }
    Ok(found)
}

/// The materials the hero's own compiled model uses, in reference order.
fn hero_materials(archive: &Vpk, hero_path: &str) -> Result<Vec<String>> {
    let raw = read(archive, &compiled(hero_path)?)?;
    let mut seen = BTreeSet::new();
    Ok(deadlock_vpk::resource_references(&raw)
        .map_err(fail)?
        .into_iter()
        .filter(|r| casefold(r).ends_with(".vmat") && seen.insert(casefold(r)))
        .collect())
}

/// The hero's own materials with what each allows, and which other models share them.
pub fn discover(archive: &Vpk, hero_path: &str, cancel: &CancellationToken) -> Result<Vec<MaterialRow>> {
    let mut rows = Vec::new();
    for path in hero_materials(archive, hero_path)? {
        cancel.check()?;
        let target = compiled(&path)?;
        if !archive.entries.contains_key(&casefold(&target)) {
            continue;
        }
        let raw = read(archive, &target)?;
        rows.push(match inspect(&path, &raw) {
            Ok(row) => row,
            Err(error) => MaterialRow {
                path: path.clone(),
                target,
                label: label(&path),
                shader: String::new(),
                supported: false,
                reason: error.to_string(),
                tint: None,
                albedo: None,
                tint_masked: false,
                color_texture: None,
                used_by: vec![],
                shared: false,
                original_sha256: sha(&raw),
            },
        });
    }
    let wanted = rows.iter().map(|r| casefold(&r.path)).collect();
    let shared = users(archive, hero_path, &wanted, cancel)?;
    for row in &mut rows {
        if let Some(models) = shared.get(&casefold(&row.path)) {
            row.used_by = models.clone();
            row.shared = true;
        }
    }
    Ok(rows)
}

/// Selected hero colors, recolored into `game` at their game paths.
pub fn stage(
    archive: &Vpk,
    hero_path: &str,
    selected: &[HeroMaterial],
    game: &Path,
    cancel: &CancellationToken,
) -> Result<MaterialReceipt> {
    let mut receipt = MaterialReceipt {
        schema: 1,
        policy: RECOLOR_POLICY.into(),
        hero_path: hero_path.into(),
        entries: vec![],
    };
    if selected.is_empty() {
        return Ok(receipt);
    }
    let catalog: BTreeMap<_, _> = discover(archive, hero_path, cancel)?
        .into_iter()
        .map(|row| (casefold(&row.target), row))
        .collect();
    for choice in selected {
        cancel.check()?;
        let target = choice.target.as_deref().unwrap_or_default();
        let row = catalog.get(&casefold(target)).ok_or_else(|| {
            fail(format!(
                "{target} is not one of this hero's materials. Load character assets again and choose a material from the list."
            ))
        })?;
        if !row.supported {
            return Err(fail(&row.reason));
        }
        if choice
            .original_sha256
            .as_deref()
            .is_some_and(|hash| !hash.is_empty() && *hash != row.original_sha256)
        {
            return Err(fail(format!(
                "The game material {} changed since you chose it (a game update). Load character assets again, check the color, then build.",
                row.label
            )));
        }
        let allow_shared = choice.allow_shared.unwrap_or(false);
        if row.shared && !allow_shared {
            return Err(fail(format!(
                "{} is also used by {} other game model{} ({}). Turn on Recolor everywhere it is used, or choose another material.",
                row.label,
                row.used_by.len(),
                if row.used_by.len() == 1 { "" } else { "s" },
                row.used_by.iter().take(3).map(|p| label_model(p)).collect::<Vec<_>>().join(", ")
            )));
        }
        let original = read(archive, &row.target)?;
        if sha(&original) != row.original_sha256 {
            return Err(fail("A game material changed while building. Build again."));
        }
        let edit = Recolor::from_row(choice);
        let ranges = ranges(choice);
        let (output, texture) = if ranges.is_empty() {
            (recolor(&original, &row.target, &edit)?, None)
        } else {
            let texture = stage_texture(archive, row, &ranges, game, cancel)?;
            let path = texture.target.strip_suffix("_c").unwrap_or(&texture.target);
            let (output, previous) = recolor_with_texture(&original, &row.target, &edit, path)?;
            if casefold(&compiled(&previous)?) != casefold(&texture.source) {
                return Err(fail(format!("{} changed its base-color texture while building. Build again.", row.label)));
            }
            (output, Some(texture))
        };
        safety::atomic_write(&safety::inside(game, &row.target)?, &output)?;
        receipt.entries.push(MaterialEntry {
            target: row.target.clone(),
            original_sha256: row.original_sha256.clone(),
            output_sha256: sha(&output),
            color: edit.color,
            saturation: edit.saturation,
            brightness: edit.brightness,
            used_by: row.used_by.clone(),
            allow_shared,
            texture,
        });
    }
    let mut seen = BTreeSet::new();
    if packaged_textures(&receipt).iter().any(|t| !seen.insert(casefold(t))) {
        return Err(fail("Two hero colors made the same recolored texture."));
    }
    Ok(receipt)
}
fn label_model(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).trim_end_matches("_c").to_owned()
}

/// The staged files are exactly the recolor of the current game materials.
pub fn verify_compiled(
    archive: &Vpk,
    receipt: &MaterialReceipt,
    game: &Path,
    cancel: &CancellationToken,
) -> Result<()> {
    if receipt.entries.is_empty() {
        return Ok(());
    }
    if receipt.schema != 1 || receipt.policy != RECOLOR_POLICY {
        return Err(fail("Hero colors were built by another version. Build again."));
    }
    let owned = hero_materials(archive, &receipt.hero_path)?
        .iter()
        .map(|p| compiled(p).map(|c| casefold(&c)))
        .collect::<Result<BTreeSet<_>>>()?;
    for entry in &receipt.entries {
        cancel.check()?;
        if !owned.contains(&casefold(&entry.target)) {
            return Err(fail(format!(
                "{} is no longer one of this hero's materials. Load character assets again.",
                label(&entry.target)
            )));
        }
        let original = read(archive, &entry.target)?;
        if sha(&original) != entry.original_sha256 {
            return Err(fail(format!(
                "The game material {} changed (a game update). Build again.",
                label(&entry.target)
            )));
        }
        let (expected, texture) = expected_outputs(archive, entry, &original, cancel)?;
        let staged = read_file(&safety::inside(game, &entry.target)?)?;
        if staged != expected || sha(&staged) != entry.output_sha256 {
            return Err(fail(format!(
                "The recolored material {} differs from its build receipt.",
                label(&entry.target)
            )));
        }
        if let (Some(expected), Some(record)) = (texture, &entry.texture) {
            let staged = read_file(&safety::inside(game, &record.target)?)?;
            if staged != expected || sha(&staged) != record.output_sha256 {
                return Err(fail(format!(
                    "The recolored texture of {} differs from its build receipt.",
                    label(&entry.target)
                )));
            }
        }
    }
    Ok(())
}

/// The project's current hero colors are the ones in the receipt.
pub fn same_choices(selected: &[HeroMaterial], receipt: &MaterialReceipt) -> bool {
    selected.len() == receipt.entries.len()
        && selected.iter().all(|row| {
            let edit = Recolor::from_row(row);
            receipt.entries.iter().any(|entry| {
                casefold(&entry.target) == casefold(row.target.as_deref().unwrap_or_default())
                    && entry.color == edit.color
                    && entry.saturation == edit.saturation
                    && entry.brightness == edit.brightness
                    && entry.allow_shared == row.allow_shared.unwrap_or(false)
                    && entry.texture.as_ref().map(|t| t.ranges.clone()).unwrap_or_default() == ranges(row)
            })
        })
}
