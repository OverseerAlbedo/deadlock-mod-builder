//! Animation pose edits retain authoritative timing, skeletons, events and root motion.
use crate::{CancellationToken, Result, compiled, fail, read, read_file, sha};
use deadlock_core::{safety, text::casefold};
use deadlock_project::customization::ResourceReplacement;
use deadlock_resource::{
    Kind, Value, binary,
    text::{Document, Edit, Node, NodeKind},
};
use deadlock_vpk::{ResourceLayout, Vpk, resource_blocks, resource_references};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};
const POSE: [&str; 3] = [
    "m_compressedPoseData",
    "m_trackCompressionSettings",
    "m_compressedPoseOffsets",
];
const TRACK: [&str; 9] = [
    "m_translationRangeX",
    "m_translationRangeY",
    "m_translationRangeZ",
    "m_scaleRange",
    "m_constantRotation",
    "m_bIsRotationStatic",
    "m_bIsTranslationStatic",
    "m_bIsScaleStatic",
    "m_nTrackReadOffset",
];
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Summary {
    pub skeleton: String,
    pub frames: u32,
    pub duration: f64,
    pub tracks: usize,
    pub events: usize,
    pub additive: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipVerification {
    #[serde(flatten)]
    pub summary: Summary,
    pub status: String,
    pub original_sha256: String,
    pub output_sha256: String,
    pub in_game_validated: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnimationRow {
    pub target: String,
    pub label: String,
    #[serde(flatten)]
    pub summary: Summary,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnimationEntry {
    pub target: String,
    pub source: PathBuf,
    pub source_sha256: String,
    pub original_sha256: String,
    pub output_sha256: Option<String>,
    pub status: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AnimationReceipt {
    pub files: Vec<String>,
    pub compile_sources: Vec<String>,
    pub replacements: Vec<AnimationEntry>,
}
fn field<'a>(v: &'a Value, key: &str) -> Result<&'a Value> {
    v.get(key)
        .ok_or_else(|| fail(format!("Animation DATA lacks required clip field: {key}")))
}
fn number(v: &Value) -> Result<f64> {
    let value = match v.kind {
        Kind::I64(n) => n as f64,
        Kind::U64(n) => n as f64,
        _ => v.number().map_err(fail)?,
    };
    if !value.is_finite() {
        return Err(fail("Non-finite animation value"));
    }
    Ok(value)
}
fn boolean(v: &Value) -> Result<bool> {
    match v.kind {
        Kind::Bool(b) => Ok(b),
        _ => Err(fail("Animation flags must be booleans.")),
    }
}
fn validate_pose(v: &Value, cancel: &CancellationToken, depth: usize) -> Result<()> {
    cancel.check()?;
    if depth > 128 {
        return Err(fail(
            "Secondary animation nesting exceeds its supported bound.",
        ));
    }
    v.object().map_err(fail)?;
    for key in POSE.into_iter().chain([
        "m_skeleton",
        "m_nNumFrames",
        "m_flDuration",
        "m_bIsAdditive",
        "m_events",
        "m_syncTrack",
        "m_rootMotion",
    ]) {
        field(v, key)?;
    }
    let skeleton = safety::relative_asset(field(v, "m_skeleton")?.string().map_err(fail)?)?;
    if !skeleton.ends_with(".vnmskel") {
        return Err(fail("Animation has no valid VNMSKEL skeleton reference."));
    }
    field(v, "m_events")?.array().map_err(fail)?;
    field(v, "m_syncTrack")?.object().map_err(fail)?;
    field(v, "m_rootMotion")?.object().map_err(fail)?;
    let secondaries = v
        .get("m_secondaryAnimations")
        .map(|v| v.array())
        .transpose()
        .map_err(fail)?
        .unwrap_or_default();
    let frames = field(v, "m_nNumFrames")?.integer().map_err(fail)?;
    let duration = number(field(v, "m_flDuration")?)?;
    boolean(field(v, "m_bIsAdditive")?)?;
    if !(1..=1_000_000).contains(&frames) || duration < 0.0 || frames > 1 && duration <= 0.0 {
        return Err(fail(
            "Animation has invalid frame count, duration or additive flag.",
        ));
    }
    let frames = frames as usize;
    let tracks = field(v, "m_trackCompressionSettings")?
        .array()
        .map_err(fail)?;
    let offsets = field(v, "m_compressedPoseOffsets")?.array().map_err(fail)?;
    let Kind::Blob(data) = &field(v, "m_compressedPoseData")?.kind else {
        return Err(fail("Animation compressed pose layout is incomplete."));
    };
    if tracks.is_empty() || tracks.len() > 10000 || offsets.len() != frames || data.len() % 2 != 0 {
        return Err(fail("Animation compressed pose layout is incomplete."));
    }
    let mut words = 0usize;
    let mut rotations = vec![];
    for track in tracks {
        cancel.check()?;
        let keys: BTreeSet<_> = track
            .object()
            .map_err(fail)?
            .iter()
            .map(|(k, _)| k.as_str())
            .collect();
        if keys.iter().any(|key| !TRACK.contains(key))
            || TRACK[..8].iter().any(|key| !keys.contains(key))
        {
            return Err(fail(
                "Animation uses an unreviewed or incomplete track compression layout.",
            ));
        }
        for name in TRACK[..4].iter() {
            let range = field(track, name)?;
            let rows = range.object().map_err(fail)?;
            if rows.len() != 2 {
                return Err(fail("Animation has invalid compression ranges."));
            }
            let start = number(field(range, "m_flRangeStart")?)?;
            let length = number(field(range, "m_flRangeLength")?)?;
            let max = f32::MAX as f64;
            if length < 0.0 || start.abs() > max || length > max || (start + length).abs() > max {
                return Err(fail(
                    "Animation compression range overflows native float32 transforms.",
                ));
            }
        }
        let q = field(track, "m_constantRotation")?.array().map_err(fail)?;
        if q.len() != 4 {
            return Err(fail("Animation has an invalid constant rotation."));
        }
        let squared = q
            .iter()
            .map(|v| number(v).map(|n| n * n))
            .collect::<Result<Vec<_>>>()?
            .iter()
            .sum::<f64>();
        if (squared - 1.0).abs() > 0.02 {
            return Err(fail("Animation has an invalid constant rotation."));
        }
        let (rotation, translation, scale) = (
            boolean(field(track, "m_bIsRotationStatic")?)?,
            boolean(field(track, "m_bIsTranslationStatic")?)?,
            boolean(field(track, "m_bIsScaleStatic")?)?,
        );
        if track
            .get("m_nTrackReadOffset")
            .is_some_and(|v| v.integer().ok() != Some(words as i128))
        {
            return Err(fail("Animation has unsupported track read offsets."));
        }
        if !rotation {
            rotations.push(words);
            words += 3;
        }
        if !translation {
            words += 3;
        }
        if !scale {
            words += 1;
        }
    }
    if offsets
        .iter()
        .enumerate()
        .any(|(i, v)| v.integer().ok() != Some((i * words) as i128))
        || data.len() != 2 * frames * words
    {
        return Err(fail(
            "Animation has an unsupported noncontiguous pose sample layout.",
        ));
    }
    let step = 2f64.sqrt() / 32767.0;
    let tolerance = 4.0 * step;
    for frame in 0..frames {
        if frame % 256 == 0 {
            cancel.check()?;
        }
        for &rotation in &rotations {
            let p = 2 * (frame * words + rotation);
            let a = u16::from_le_bytes([data[p], data[p + 1]]);
            let b = u16::from_le_bytes([data[p + 2], data[p + 3]]);
            let c = u16::from_le_bytes([data[p + 4], data[p + 5]]);
            if c > 32767 {
                return Err(fail("Animation has invalid compressed quaternion bits."));
            }
            let components = [a, b, c].map(|word| (word & 32767) as f64 * step - 1.0 / 2f64.sqrt());
            let squared = components.iter().map(|v| v * v).sum::<f64>();
            let omitted = (1.0 - squared).max(0.0).sqrt();
            if squared > 1.0 + tolerance || components.iter().any(|v| v.abs() > omitted + tolerance)
            {
                return Err(fail(
                    "Animation has an invalid compressed quaternion: omitted component must be largest.",
                ));
            }
        }
    }
    for secondary in secondaries {
        validate_pose(secondary, cancel, depth + 1)?;
    }
    Ok(())
}
fn resource(raw: &[u8], cancel: &CancellationToken) -> Result<(ResourceLayout, Value)> {
    let layout = resource_blocks(raw).map_err(fail)?;
    if layout.declared_size != raw.len() {
        return Err(fail(
            "Animation clip has unsupported external/trailing payloads.",
        ));
    }
    let data: Vec<_> = layout.blocks.iter().filter(|b| b.name == "DATA").collect();
    if data.len() != 1 {
        return Err(fail("Animation clip requires exactly one DATA block."));
    }
    let b = data[0];
    let typed = binary::decode(&raw[b.offset..b.offset + b.size])
        .map_err(fail)?
        .root;
    validate_pose(&typed, cancel, 0)?;
    Ok((layout, typed))
}
fn summary(v: &Value) -> Result<Summary> {
    Ok(Summary {
        skeleton: field(v, "m_skeleton")?.string().map_err(fail)?.into(),
        frames: field(v, "m_nNumFrames")?.integer().map_err(fail)? as u32,
        duration: number(field(v, "m_flDuration")?)?,
        tracks: field(v, "m_trackCompressionSettings")?
            .array()
            .map_err(fail)?
            .len(),
        events: field(v, "m_events")?.array().map_err(fail)?.len(),
        additive: boolean(field(v, "m_bIsAdditive")?)?,
    })
}
pub fn validate_clip(
    original: &[u8],
    replacement: &[u8],
    cancel: &CancellationToken,
) -> Result<ClipVerification> {
    let (old_layout, old) = resource(original, cancel)?;
    let (new_layout, new) = resource(replacement, cancel)?;
    if old_layout.resource_version != new_layout.resource_version {
        return Err(fail(
            "Animation resource version differs from the selected original clip.",
        ));
    }
    if summary(&old)?.tracks != summary(&new)?.tracks {
        return Err(fail(
            "Animation bone track count differs from the original skeleton.",
        ));
    }
    let contract = |v: &Value| -> Result<BTreeMap<String, Value>> {
        Ok(v.object()
            .map_err(fail)?
            .iter()
            .filter(|(key, _)| !POSE.contains(&key.as_str()))
            .cloned()
            .collect())
    };
    let (a, b) = (contract(&old)?, contract(&new)?);
    if a.len() != b.len()
        || a.iter()
            .any(|(key, value)| b.get(key).is_none_or(|v| !value.canonical_eq(v)))
    {
        return Err(fail(
            "Animation replacement changes protected clip metadata. Keep the original skeleton, timing, events, root motion, curves and graph contract.",
        ));
    }
    let mut a = resource_references(original).map_err(fail)?;
    let mut b = resource_references(replacement).map_err(fail)?;
    a.sort();
    b.sort();
    if a != b {
        return Err(fail(
            "Animation replacement changes original resource dependencies.",
        ));
    }
    let opaque = |raw: &[u8], layout: &ResourceLayout| {
        layout
            .blocks
            .iter()
            .filter(|b| !["REDI", "RED2", "RERL", "DATA"].contains(&b.name.as_str()))
            .map(|b| (b.name.clone(), raw[b.offset..b.offset + b.size].to_vec()))
            .collect::<Vec<_>>()
    };
    if opaque(original, &old_layout) != opaque(replacement, &new_layout) {
        return Err(fail(
            "Animation replacement changes unsupported runtime blocks.",
        ));
    }
    Ok(ClipVerification {
        summary: summary(&new)?,
        status: "verified".into(),
        original_sha256: sha(original),
        output_sha256: sha(replacement),
        in_game_validated: false,
    })
}
pub fn discover(
    archive: &Vpk,
    model: &str,
    cancel: &CancellationToken,
    mut log: impl FnMut(&str),
) -> Result<Vec<AnimationRow>> {
    let original = compiled(model)?;
    if !original.ends_with(".vmdl_c") {
        return Err(fail(
            "Select a hero model before choosing animation replacements.",
        ));
    }
    let refs = resource_references(&read(archive, &original)?).map_err(fail)?;
    let skeletons: BTreeSet<_> = refs
        .iter()
        .filter(|s| s.ends_with(".vnmskel") || s.ends_with(".vnmskel_c"))
        .map(|s| s.trim_end_matches("_c").to_owned())
        .collect();
    if skeletons.is_empty() {
        return Err(fail(
            "The selected model has no discoverable AnimGraph 2 skeleton reference.",
        ));
    }
    let prefix = original
        .rsplit_once('/')
        .map(|(parent, _)| format!("{parent}/"))
        .unwrap_or_default();
    let mut candidates: BTreeSet<_> = archive
        .entries
        .values()
        .filter(|e| e.path.starts_with(&prefix) && e.path.ends_with(".vnmclip_c"))
        .map(|e| e.path.clone())
        .collect();
    let mut pending: Vec<_> = refs
        .into_iter()
        .filter(|s| s.ends_with(".vnmgraph") || s.ends_with(".vnmgraph_c"))
        .collect();
    let mut seen = BTreeSet::new();
    while let Some(current) = pending.pop() {
        cancel.check()?;
        let current = compiled(&current)?;
        if !seen.insert(current.clone()) {
            continue;
        }
        if seen.len() > 20000 {
            return Err(fail(
                "Animation dependency discovery exceeded its bounded resource limit.",
            ));
        }
        if !archive.entries.contains_key(&casefold(&current)) {
            continue;
        }
        for reference in resource_references(&read(archive, &current)?).map_err(fail)? {
            if reference.ends_with(".vnmclip") || reference.ends_with(".vnmclip_c") {
                candidates.insert(compiled(&reference)?);
            } else if reference.ends_with(".vnmgraph") || reference.ends_with(".vnmgraph_c") {
                pending.push(reference);
            }
        }
    }
    let mut rows = vec![];
    for target in candidates {
        cancel.check()?;
        let Some(entry) = archive.entries.get(&casefold(&target)) else {
            continue;
        };
        match resource(&read(archive, &target)?, cancel) {
            Ok((_, value)) => {
                let summary = summary(&value)?;
                if skeletons.contains(&summary.skeleton) {
                    rows.push(AnimationRow {
                        target: entry.path.clone(),
                        label: target
                            .rsplit('/')
                            .next()
                            .unwrap()
                            .trim_end_matches(".vnmclip_c")
                            .into(),
                        summary,
                    });
                }
            }
            Err(error) => {
                cancel.check()?;
                log(&format!(
                    "Animation unavailable for replacement: {target}: {error}"
                ));
            }
        }
    }
    Ok(rows)
}
fn source_string(node: &Node) -> Result<&str> {
    match &node.kind {
        NodeKind::Scalar {
            value,
            quoted: true,
        } => Ok(value),
        NodeKind::Typed { name, value } if name == "resource" => source_string(value),
        _ => Err(fail(
            "Animation source filename must be a relative path string.",
        )),
    }
}
pub fn source_dependency(source: &Path) -> Result<Option<PathBuf>> {
    if source
        .extension()
        .is_none_or(|v| !v.eq_ignore_ascii_case("vnmclip"))
        || !source.is_file()
    {
        return Ok(None);
    }
    let raw = read_file(source)?;
    let text = std::str::from_utf8(&raw)
        .map_err(fail)?
        .trim_start_matches('\u{feff}');
    let doc = Document::parse(text).map_err(fail)?;
    let Some(field) = doc.root.get("m_sourceFilename") else {
        return Ok(None);
    };
    let reference = source_string(field)?;
    if reference.is_empty() {
        Ok(None)
    } else {
        Ok(Some(safety::inside(
            source.parent().unwrap_or(Path::new(".")),
            reference,
        )?))
    }
}
pub fn stage(
    archive: &Vpk,
    model: &str,
    replacements: &[ResourceReplacement],
    content: &Path,
    game: &Path,
    cancel: &CancellationToken,
    dry_run: bool,
) -> Result<AnimationReceipt> {
    let mut receipt = AnimationReceipt::default();
    let selected: Vec<_> = replacements
        .iter()
        .filter(|r| r.enabled.unwrap_or(true))
        .collect();
    if selected.is_empty() {
        return Ok(receipt);
    }
    let candidates: BTreeMap<_, _> = discover(archive, model, cancel, |_| {})?
        .into_iter()
        .map(|row| (casefold(&row.target), row))
        .collect();
    let mut seen = BTreeSet::new();
    let mut prepared = vec![];
    for item in selected {
        cancel.check()?;
        let requested = safety::relative_asset(item.target.as_deref().unwrap_or_default())?;
        let row = candidates.get(&casefold(&requested)).ok_or_else(|| {
            fail(format!(
                "Choose an animation belonging to the selected hero: {requested}"
            ))
        })?;
        let target = row.target.clone();
        if !seen.insert(casefold(&target)) {
            return Err(fail("Two animation replacements target the same clip."));
        }
        let source = safety::resolve_path(Path::new(item.source.as_deref().unwrap_or_default()))?;
        let raw = read_file(&source)?;
        let original = read(archive, &target)?;
        let mut entry = AnimationEntry {
            target: target.clone(),
            source: source.clone(),
            source_sha256: sha(&raw),
            original_sha256: sha(&original),
            output_sha256: None,
            status: "awaiting_native_compile".into(),
        };
        let ext = source
            .extension()
            .and_then(|v| v.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if ext == "vnmclip_c" {
            let audit = validate_clip(&original, &raw, cancel)?;
            entry.output_sha256 = Some(audit.output_sha256);
            entry.status = "verified".into();
            prepared.push((entry, raw, None));
        } else if ext == "vnmclip" {
            let doc = Document::parse(
                std::str::from_utf8(&raw)
                    .map_err(fail)?
                    .trim_start_matches('\u{feff}'),
            )
            .map_err(fail)?;
            let field = doc
                .root
                .get("m_sourceFilename")
                .ok_or_else(|| fail("VNMCLIP must name an animation DMX."))?;
            let source_ref = safety::relative_asset(source_string(field)?)?;
            if !casefold(&source_ref).ends_with(".dmx") {
                return Err(fail(
                    "VNMCLIP source must name an animation DMX relative to its document folder.",
                ));
            }
            let animation = read_file(&safety::inside(source.parent().unwrap(), &source_ref)?)?;
            let dmx =
                deadlock_dmx::Dmx::parse_with_cancel(animation.clone(), cancel).map_err(fail)?;
            if !dmx.elements.iter().any(|e| e.kind == "DmeChannelsClip") {
                return Err(fail(
                    "The referenced DMX contains no animation clip; export an animation, not only a mesh.",
                ));
            }
            let expected = &row.summary.skeleton;
            if doc
                .root
                .get("m_animationSkeletonName")
                .map(source_string)
                .transpose()?
                != Some(expected.as_str())
            {
                return Err(fail(format!(
                    "VNMCLIP must use the selected clip's original animation skeleton: {expected}"
                )));
            }
            if doc
                .root
                .get("m_additiveBaseFilename")
                .map(source_string)
                .transpose()?
                .is_some_and(|s| !s.is_empty())
            {
                return Err(fail(
                    "External additive-base authoring inputs require a separate reviewed import; use a compiled VNMCLIP replacement for now.",
                ));
            }
            let mut skeletons = vec![expected.clone()];
            if let Some(secondary) = doc.root.get("m_secondaryAnimationSkeletonNames") {
                for node in secondary
                    .array()
                    .ok_or_else(|| fail("Secondary animation skeletons must be an array."))?
                {
                    skeletons.push(source_string(node)?.into());
                }
            }
            for skeleton in skeletons {
                if !archive
                    .entries
                    .contains_key(&casefold(&compiled(&skeleton)?))
                {
                    return Err(fail(format!(
                        "Animation skeleton is missing from the selected game: {skeleton}"
                    )));
                }
            }
            let target = format!(
                "models/_dmb_custom/animations/{}.dmx",
                &sha(&animation)[..24]
            );
            let text = doc
                .apply(&[Edit {
                    start: field.start,
                    end: field.end,
                    replacement: serde_json::to_string(&target).map_err(fail)?,
                }])
                .map_err(fail)?;
            prepared.push((entry, text.into_bytes(), Some((target, animation))));
        } else {
            return Err(fail(
                "Animation replacement needs a compiled .vnmclip_c or authored .vnmclip with animation DMX. Blender/FBX/GLB action export and retargeting are not implemented; the file was not packaged.",
            ));
        }
    }
    for (entry, raw, dependency) in prepared {
        cancel.check()?;
        if let Some((dmx, animation)) = dependency {
            let source = entry.target.trim_end_matches("_c").to_owned();
            if !dry_run {
                safety::atomic_write(&safety::inside(content, &dmx)?, animation)?;
                safety::atomic_write(&safety::inside(content, &source)?, raw)?;
            }
            receipt.compile_sources.push(source);
        } else {
            if !dry_run {
                safety::atomic_write(&safety::inside(game, &entry.target)?, raw)?;
            }
            receipt.files.push(entry.target.clone());
        }
        receipt.replacements.push(entry);
    }
    Ok(receipt)
}
pub fn verify_compiled(
    archive: &Vpk,
    receipt: &AnimationReceipt,
    game: &Path,
    cancel: &CancellationToken,
) -> Result<Vec<ClipVerification>> {
    let mut results = vec![];
    let mut seen = BTreeSet::new();
    for item in &receipt.replacements {
        cancel.check()?;
        let target = safety::relative_asset(&item.target)?;
        if !seen.insert(casefold(&target)) {
            return Err(fail("Duplicate animation target."));
        }
        let original = read(archive, &target)?;
        if sha(&original) != item.original_sha256 {
            return Err(fail(
                "Original animation changed since staging; analyze replacements again.",
            ));
        }
        let raw = read_file(&safety::inside(game, &target)?)?;
        if item.status == "verified" && item.output_sha256.as_ref() != Some(&sha(&raw)) {
            return Err(fail("Staged animation changed after verification."));
        }
        results.push(validate_clip(&original, &raw, cancel)?);
    }
    Ok(results)
}
