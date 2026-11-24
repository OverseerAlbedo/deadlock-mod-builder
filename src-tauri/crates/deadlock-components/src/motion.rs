//! Animation pose edits retain authoritative timing, skeletons, events and root motion.
use crate::{CancellationToken, CoreError, Result, compiled, fail, read, read_file, sha};
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
    /// A clip made from an animation (`source` is the Blender file): what it
    /// was made from. Absent for every other row, so their receipts stay
    /// byte for byte.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub made: Option<crate::made_clips::MadeEntry>,
    /// A clip taken from another hero (`source` is the other hero's clip):
    /// what it was taken from and the digests of the game files it was made
    /// of. Absent for every other row, so their receipts stay byte for byte.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub taken: Option<crate::hero_clips::TakenEntry>,
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
/// One event of a clip as a mark on the Motion page's time bar: where it
/// starts (a fraction of the clip, as the clip stores it), its kind and a
/// word for it. Sound events are named by their tag (`footstep`), never by a
/// sound file; animation events by their class without the `AE_` prefix.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventMark {
    pub at: f64,
    /// sound, foot, effect, game (a legacy animation event), tag, transition or other.
    pub kind: String,
    pub word: String,
}
fn event_time(v: Option<&Value>) -> Option<f64> {
    let v = v?;
    match v.kind {
        Kind::Object(_) => event_time(v.get("m_flValue")),
        _ => number(v).ok(),
    }
}
fn spoken(raw: &str) -> String {
    let text = raw.replace(['_', '-'], " ").trim().to_ascii_lowercase();
    let text = match text.as_str() {
        "mvmt short" | "mvmt" => "movement".to_string(),
        "jumpland" => "landing".to_string(),
        other => other.to_string(),
    };
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
        None => String::new(),
    }
}
/// The time-bar marks of a decoded clip's `m_events` (empty when it has none).
pub fn event_marks(clip: &Value) -> Vec<EventMark> {
    let Some(Ok(events)) = clip.get("m_events").map(|e| e.array()) else {
        return vec![];
    };
    let text = |e: &Value, key: &str| e.get(key).and_then(|v| v.string().ok()).unwrap_or_default().to_string();
    events
        .iter()
        .filter_map(|e| {
            let at = event_time(e.get("m_flStartTime"))?.clamp(0., 1.);
            let class = text(e, "_class");
            let (kind, word) = match class.as_str() {
                "CNmTaggedSoundEvent" => ("sound", spoken(&text(e, "m_name"))),
                "CNmSoundEvent" => ("sound", "Sound".into()),
                "CNmFootEvent" => ("foot", "Foot".into()),
                "CNmParticleEvent" => ("effect", "Effect".into()),
                "CNmLegacyEvent" => {
                    let name = text(e, "m_animEventClassName");
                    ("game", spoken(name.trim_start_matches("AE_CL_").trim_start_matches("AE_SV_").trim_start_matches("AE_")))
                }
                "CNmIDEvent" => ("tag", text(e, "m_ID")),
                "CNmTransitionEvent" => ("transition", spoken(&text(e, "m_rule"))),
                _ => ("other", spoken(class.trim_start_matches("CNm").trim_end_matches("Event"))),
            };
            Some(EventMark { at, kind: kind.into(), word: if word.is_empty() { "Event".into() } else { word } })
        })
        .collect()
}
pub fn validate_clip(
    original: &[u8],
    replacement: &[u8],
    cancel: &CancellationToken,
) -> Result<ClipVerification> {
    let comparison = compare(original, replacement, cancel)?;
    if let Some(error) = comparison.first_error() {
        return Err(fail(error));
    }
    Ok(ClipVerification {
        summary: comparison.new.clone(),
        status: "verified".into(),
        original_sha256: sha(original),
        output_sha256: sha(replacement),
        in_game_validated: false,
    })
}

/// A build error as the app shows it (its message, without the kind).
pub fn said(error: &CoreError) -> String {
    match error {
        CoreError::Integrity(message) | CoreError::Unsupported(message) => message.clone(),
        other => other.to_string(),
    }
}

/// The build's messages for a compiled replacement that does not fit, in the
/// order `validate_clip` checks them.
pub const VERSION_DIFFERS: &str = "Animation resource version differs from the selected original clip.";
pub const TRACKS_DIFFER: &str = "Animation bone track count differs from the original skeleton.";
pub const METADATA_CHANGED: &str = "Animation replacement changes protected clip metadata. Keep the original skeleton, timing, events, root motion, curves and graph contract.";
pub const DEPENDENCIES_CHANGED: &str = "Animation replacement changes original resource dependencies.";
pub const BLOCKS_CHANGED: &str = "Animation replacement changes unsupported runtime blocks.";

/// Clip keys each fit check covers; every other key (and the version,
/// references and other blocks) belongs to "nothing else is changed".
const SKELETON_KEYS: [&str; 2] = ["m_skeleton", "m_modelSpaceBoneSamplingIndices"];
const LENGTH_KEYS: [&str; 2] = ["m_nNumFrames", "m_flDuration"];
const EVENT_KEYS: [&str; 3] = ["m_events", "m_rootMotion", "m_syncTrack"];

/// Everything `validate_clip` compares between the game's clip and a
/// compiled replacement, worked out once. Its verdict (`first_error`) is the
/// build's; the fit checks (`checks`) only present it.
struct Comparison {
    old: Summary,
    new: Summary,
    old_value: Value,
    new_value: Value,
    version: bool,
    /// Keys outside the pose data that differ (or are on one side only).
    differing: BTreeSet<String>,
    /// Whether the resource references match, or the reading error.
    references: std::result::Result<bool, String>,
    opaque: bool,
}
fn compare(original: &[u8], replacement: &[u8], cancel: &CancellationToken) -> Result<Comparison> {
    let (old_layout, old) = resource(original, cancel)?;
    let (new_layout, new) = resource(replacement, cancel)?;
    let contract = |v: &Value| -> Result<BTreeMap<String, Value>> {
        Ok(v.object()
            .map_err(fail)?
            .iter()
            .filter(|(key, _)| !POSE.contains(&key.as_str()))
            .cloned()
            .collect())
    };
    let (a, b) = (contract(&old)?, contract(&new)?);
    let differing = a
        .keys()
        .chain(b.keys())
        .filter(|key| match (a.get(*key), b.get(*key)) {
            (Some(x), Some(y)) => !x.canonical_eq(y),
            _ => true,
        })
        .cloned()
        .collect();
    let references = (|| -> std::result::Result<bool, String> {
        let mut a = resource_references(original).map_err(|e| e.to_string())?;
        let mut b = resource_references(replacement).map_err(|e| e.to_string())?;
        a.sort();
        b.sort();
        Ok(a == b)
    })();
    let opaque = |raw: &[u8], layout: &ResourceLayout| {
        layout
            .blocks
            .iter()
            .filter(|b| !["REDI", "RED2", "RERL", "DATA"].contains(&b.name.as_str()))
            .map(|b| (b.name.clone(), raw[b.offset..b.offset + b.size].to_vec()))
            .collect::<Vec<_>>()
    };
    Ok(Comparison {
        old: summary(&old)?,
        new: summary(&new)?,
        version: old_layout.resource_version == new_layout.resource_version,
        differing,
        references,
        opaque: opaque(original, &old_layout) == opaque(replacement, &new_layout),
        old_value: old,
        new_value: new,
    })
}
impl Comparison {
    /// The first rule the replacement breaks, as the build says it.
    fn first_error(&self) -> Option<String> {
        if !self.version {
            return Some(VERSION_DIFFERS.into());
        }
        if self.old.tracks != self.new.tracks {
            return Some(TRACKS_DIFFER.into());
        }
        if !self.differing.is_empty() {
            return Some(METADATA_CHANGED.into());
        }
        match &self.references {
            Err(error) => return Some(error.clone()),
            Ok(false) => return Some(DEPENDENCIES_CHANGED.into()),
            Ok(true) => {}
        }
        if !self.opaque {
            return Some(BLOCKS_CHANGED.into());
        }
        None
    }
    fn differs(&self, keys: &[&'static str]) -> Vec<&'static str> {
        keys.iter().copied().filter(|k| self.differing.contains(*k)).collect()
    }
    fn checks(&self) -> Vec<FitCheck> {
        let (old, new) = (&self.old, &self.new);
        let skeleton = self.differs(&SKELETON_KEYS);
        let skeleton_check = if old.tracks != new.tracks {
            FitCheck::fail("skeleton", format!("Bone tracks differ: {} here, {} in the game's clip", new.tracks, old.tracks))
        } else if skeleton.contains(&"m_skeleton") {
            FitCheck::fail("skeleton", format!("Made for another skeleton: {}", new.skeleton))
        } else if !skeleton.is_empty() {
            FitCheck::fail("skeleton", "The bones sampled in model space differ from the game's clip".into())
        } else {
            FitCheck::pass("skeleton", format!("Same skeleton and bone tracks ({})", new.tracks))
        };
        let length_check = if self.differs(&LENGTH_KEYS).is_empty() {
            FitCheck::pass("length", format!("Same length: {:.2} s, {} frames", new.duration, new.frames))
        } else {
            FitCheck::fail("length", format!("Length differs: {:.2} s, {} frames here; the game's clip is {:.2} s, {} frames", new.duration, new.frames, old.duration, old.frames))
        };
        let events = self.differs(&EVENT_KEYS);
        let events_check = if events.is_empty() {
            FitCheck::pass("events", "Same events, root motion and timing marks".into())
        } else {
            let mut parts = vec![];
            if events.contains(&"m_events") {
                let list = |v: &Value| v.get("m_events").and_then(|e| e.array().ok()).map(<[Value]>::to_vec).unwrap_or_default();
                let (a, b) = (list(&self.old_value), list(&self.new_value));
                let changed = (0..a.len().max(b.len())).filter(|&i| match (a.get(i), b.get(i)) {
                    (Some(x), Some(y)) => !x.canonical_eq(y),
                    _ => true,
                }).count();
                parts.push(format!("events differ from the original ({changed} of {})", a.len().max(b.len())));
            }
            if events.contains(&"m_rootMotion") {
                parts.push("root motion differs".into());
            }
            if events.contains(&"m_syncTrack") {
                parts.push("timing marks differ".into());
            }
            let text = parts.join(", ");
            FitCheck::fail("events", text[..1].to_uppercase() + &text[1..])
        };
        let mut other: Vec<String> = self
            .differing
            .iter()
            .filter(|k| !SKELETON_KEYS.contains(&k.as_str()) && !LENGTH_KEYS.contains(&k.as_str()) && !EVENT_KEYS.contains(&k.as_str()))
            .map(|k| k.trim_start_matches("m_").to_string())
            .collect();
        if !self.version {
            other.insert(0, "resource version".into());
        }
        match &self.references {
            Err(_) => other.push("its references could not be read".into()),
            Ok(false) => other.push("resource references".into()),
            Ok(true) => {}
        }
        if !self.opaque {
            other.push("other data blocks".into());
        }
        let other_check = if other.is_empty() {
            FitCheck::pass("unchanged", "Nothing else in the file is changed".into())
        } else {
            FitCheck::fail("unchanged", format!("Other parts of the file changed: {}", other.join(", ")))
        };
        vec![skeleton_check, length_check, events_check, other_check]
    }
}

/// One fit check of a replacement, as the Motion page lists it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FitCheck {
    pub id: String,
    pub label: String,
    /// "pass", "fail", "not_checked" (an earlier check stopped it) or
    /// "after_compile" (checked once compiled, at build).
    pub status: String,
}
impl FitCheck {
    fn pass(id: &str, label: String) -> Self {
        Self { id: id.into(), label, status: "pass".into() }
    }
    fn fail(id: &str, label: String) -> Self {
        Self { id: id.into(), label, status: "fail".into() }
    }
    fn with(id: &str, label: &str, status: &str) -> Self {
        Self { id: id.into(), label: label.into(), status: status.into() }
    }
}

/// Whether a replacement file fits the game's clip, as checks. A presentation
/// of what the build does: `fits` is false exactly when the build refuses the
/// file, and `reason` is then the build's own message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FitReport {
    /// "compiled" (a `.vnmclip_c`), "authored" (a `.vnmclip` and its DMX),
    /// "unreadable" (the file cannot be read as a clip) or "unsupported"
    /// (another kind of file).
    pub kind: String,
    pub fits: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub checks: Vec<FitCheck>,
    /// The replacement's own numbers (a compiled clip that could be read).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<Summary>,
}

/// The fit checks of a compiled replacement against the game's clip. An
/// unreadable game clip is an error; an unreadable replacement is a report.
pub fn compiled_fit(original: &[u8], replacement: &[u8], cancel: &CancellationToken) -> Result<FitReport> {
    resource(original, cancel)?;
    match compare(original, replacement, cancel) {
        Ok(comparison) => {
            let reason = comparison.first_error();
            Ok(FitReport { kind: "compiled".into(), fits: reason.is_none(), reason, checks: comparison.checks(), summary: Some(comparison.new.clone()) })
        }
        Err(CoreError::Cancelled) => Err(CoreError::Cancelled),
        Err(error) => Ok(FitReport {
            kind: "unreadable".into(),
            fits: false,
            reason: Some(said(&error)),
            checks: vec![FitCheck::fail("readable", "The file could not be read as a compiled animation clip".into())],
            summary: None,
        }),
    }
}

const AFTER_COMPILE: [(&str, &str); 4] = [
    ("skeleton", "Same skeleton and bone tracks"),
    ("length", "Same length and frames"),
    ("events", "Same events, root motion and timing marks"),
    ("unchanged", "Nothing else in the file is changed"),
];
const AUTHORED_STEPS: [(&str, &str); 5] = [
    ("document", "A .vnmclip that names an animation DMX"),
    ("animation", "The DMX holds an animation"),
    ("skeleton_name", "Made for this clip's skeleton"),
    ("additive_base", "No separate additive base file"),
    ("skeletons_in_game", "Every skeleton it names is in the game"),
];

/// The fit checks of an authored `.vnmclip` (with the animation DMX it
/// names): the checks `stage` runs before compiling, in its order, then the
/// compiled checks marked "after_compile".
pub fn authored_fit(archive: &Vpk, skeleton: &str, source: &Path, cancel: &CancellationToken) -> Result<FitReport> {
    let mut statuses: Vec<(&str, std::result::Result<(), String>)> = vec![];
    let outcome = match read_file(source) {
        Ok(raw) => authored(archive, skeleton, source, &raw, cancel, |id, result| statuses.push((id, result.as_ref().map(|_| ()).map_err(|e| e.to_string())))),
        Err(error) => {
            statuses.push(("document", Err(error.to_string())));
            Err(error)
        }
    };
    if let Err(CoreError::Cancelled) = outcome {
        return Err(CoreError::Cancelled);
    }
    let reason = outcome.err().map(|e| said(&e));
    let mut checks: Vec<FitCheck> = AUTHORED_STEPS
        .iter()
        .map(|(id, label)| {
            let status = match statuses.iter().find(|(s, _)| s == id) {
                Some((_, Ok(()))) => "pass",
                Some((_, Err(_))) => "fail",
                None => "not_checked",
            };
            FitCheck::with(id, label, status)
        })
        .collect();
    checks.extend(AFTER_COMPILE.iter().map(|(id, label)| FitCheck::with(id, label, "after_compile")));
    Ok(FitReport { kind: "authored".into(), fits: reason.is_none(), reason, checks, summary: None })
}

/// What an authored `.vnmclip` stages: the document (its source field's
/// span) and the animation DMX bytes.
struct Authored {
    doc: Document,
    field: (usize, usize),
    animation: Vec<u8>,
}
/// The pre-compile checks of an authored `.vnmclip`, in `stage`'s order;
/// `step` hears each check's outcome (the first failure ends them).
fn authored(archive: &Vpk, expected: &str, source: &Path, raw: &[u8], cancel: &CancellationToken, mut step: impl FnMut(&'static str, &Result<()>)) -> Result<Authored> {
    fn record<T>(step: &mut impl FnMut(&'static str, &Result<()>), id: &'static str, result: Result<T>) -> Result<T> {
        match result {
            Ok(value) => {
                step(id, &Ok(()));
                Ok(value)
            }
            Err(error) => {
                let message = error.to_string();
                let cancelled = matches!(error, CoreError::Cancelled);
                step(id, &Err(fail(&message)));
                Err(if cancelled { CoreError::Cancelled } else { error })
            }
        }
    }
    let (doc, field, animation) = record(&mut step, "document", (|| {
        let doc = Document::parse(
            std::str::from_utf8(raw)
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
        let span = (field.start, field.end);
        Ok((doc, span, animation))
    })())?;
    record(&mut step, "animation", (|| {
        let dmx = deadlock_dmx::Dmx::parse_with_cancel(animation.clone(), cancel).map_err(fail)?;
        if !dmx.elements.iter().any(|e| e.kind == "DmeChannelsClip") {
            return Err(fail(
                "The referenced DMX contains no animation clip; export an animation, not only a mesh.",
            ));
        }
        Ok(())
    })())?;
    record(&mut step, "skeleton_name", (|| {
        if doc
            .root
            .get("m_animationSkeletonName")
            .map(source_string)
            .transpose()?
            != Some(expected)
        {
            return Err(fail(format!(
                "VNMCLIP must use the selected clip's original animation skeleton: {expected}"
            )));
        }
        Ok(())
    })())?;
    record(&mut step, "additive_base", (|| {
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
        Ok(())
    })())?;
    record(&mut step, "skeletons_in_game", (|| {
        let mut skeletons = vec![expected.to_string()];
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
        Ok(())
    })())?;
    Ok(Authored { doc, field, animation })
}

/// The message the build gives for a replacement file that is neither a
/// compiled nor an authored clip.
pub const UNSUPPORTED_SOURCE: &str = "Animation replacement needs a compiled .vnmclip_c or authored .vnmclip with animation DMX; the file was not packaged. An animation in a Blender, FBX or glTF file is made into a clip with Make it from my animation on the Motion page.";

/// The skeletons the hero's model names (a clip of the hero uses one).
fn model_skeletons(archive: &Vpk, model: &str) -> Result<BTreeSet<String>> {
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
    Ok(skeletons)
}

/// The Motion page's fit checks for one replacement file of one of the
/// hero's clips: the game's clip is read from the archive and must use one of
/// the hero model's skeletons. Read-only; it writes nothing.
pub fn fit(archive: &Vpk, model: &str, target: &str, source: &Path, cancel: &CancellationToken) -> Result<FitReport> {
    let skeletons = model_skeletons(archive, model)?;
    let target = compiled(target)?;
    if !casefold(&target).ends_with(".vnmclip_c") {
        return Err(fail(format!("Choose an animation belonging to the selected hero: {target}")));
    }
    let original = read(archive, &target)?;
    let (_, value) = resource(&original, cancel)?;
    let skeleton = summary(&value)?.skeleton;
    if !skeletons.contains(&skeleton) {
        return Err(fail(format!("Choose an animation belonging to the selected hero: {target}")));
    }
    let source = safety::resolve_path(source)?;
    let ext = source.extension().and_then(|v| v.to_str()).unwrap_or_default().to_ascii_lowercase();
    match ext.as_str() {
        "vnmclip_c" => match read_file(&source) {
            Ok(raw) => compiled_fit(&original, &raw, cancel),
            Err(error) => Ok(FitReport {
                kind: "unreadable".into(),
                fits: false,
                reason: Some(said(&error)),
                checks: vec![FitCheck::fail("readable", "The file could not be read".into())],
                summary: None,
            }),
        },
        "vnmclip" => authored_fit(archive, &skeleton, &source, cancel),
        _ => Ok(FitReport {
            kind: "unsupported".into(),
            fits: false,
            reason: Some(UNSUPPORTED_SOURCE.into()),
            checks: vec![FitCheck::fail("kind", "A compiled .vnmclip_c, or a .vnmclip that names an animation DMX".into())],
            summary: None,
        }),
    }
}
pub fn discover(
    archive: &Vpk,
    model: &str,
    cancel: &CancellationToken,
    log: impl FnMut(&str),
) -> Result<Vec<AnimationRow>> {
    Ok(catalog(archive, model, cancel, log)?.into_iter().map(|clip| clip.row).collect())
}

/// What the Motion page shows about one clip, beside its `AnimationRow`:
/// computed from the game's data at discovery, never in the page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipDetails {
    /// The readable name and list group (`clip_names`).
    pub name: String,
    pub group: crate::clip_names::Group,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<crate::clip_names::Direction>,
    /// An animation graph of the hero names this clip.
    #[serde(default)]
    pub graph_named: bool,
    /// A copy the compiler derived from another clip (`x.vnmclip+non_additive`):
    /// the clip it was made from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub derived_from: Option<String>,
    /// The last stored frame equals the first (within 0.5° and 0.05 in for
    /// every bone): the clip ends where it starts. Says nothing about whether
    /// the game loops it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub ends_where_starts: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogClip {
    #[serde(flatten)]
    pub row: AnimationRow,
    #[serde(flatten)]
    pub details: ClipDetails,
}

/// The root motion displacement of a decoded clip (x forward, y left).
fn root_travel(value: &Value) -> Option<(f64, f64)> {
    let total = value.get("m_rootMotion")?.get("m_totalDelta")?.array().ok()?;
    Some((total.first()?.number().ok()?, total.get(1)?.number().ok()?))
}

fn ends_where_starts(value: &Value) -> bool {
    let Ok(clip) = deadlock_preparation::clip_decode::Clip::from_value(value) else {
        return false;
    };
    if clip.frames.len() < 2 {
        return false;
    }
    let (first, last) = (&clip.frames[0], &clip.frames[clip.frames.len() - 1]);
    first.iter().zip(last).all(|(a, b)| {
        let dot = (0..4).map(|k| a.rotation[k] * b.rotation[k]).sum::<f64>().abs().min(1.);
        let degrees = 2. * dot.acos().to_degrees();
        let inches = (0..3).map(|k| (a.position[k] - b.position[k]).powi(2)).sum::<f64>().sqrt();
        degrees < 0.5 && inches < 0.05 && (a.scale - b.scale).abs() < 1e-3
    })
}

/// Every clip of the hero `discover` lists, with what the Motion page shows
/// about it (`ClipDetails`). The rows are exactly `discover`'s.
pub fn catalog(
    archive: &Vpk,
    model: &str,
    cancel: &CancellationToken,
    mut log: impl FnMut(&str),
) -> Result<Vec<CatalogClip>> {
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
    // Clips an animation graph of the hero names (casefolded).
    let mut named = BTreeSet::new();
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
                let clip = compiled(&reference)?;
                named.insert(casefold(&clip));
                candidates.insert(clip);
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
                    let label: String = target
                        .rsplit('/')
                        .next()
                        .unwrap()
                        .trim_end_matches(".vnmclip_c")
                        .into();
                    let described = crate::clip_names::describe_moving(&label, root_travel(&value));
                    let derived_from = label
                        .split_once(".vnmclip+")
                        .map(|(stem, _)| format!("{}{stem}.vnmclip_c", &entry.path[..entry.path.len() - entry.path.rsplit('/').next().unwrap_or("").len()]));
                    rows.push(CatalogClip {
                        details: ClipDetails {
                            name: described.name,
                            group: described.group,
                            direction: described.direction,
                            graph_named: named.contains(&casefold(&target)),
                            derived_from,
                            ends_where_starts: ends_where_starts(&value),
                        },
                        row: AnimationRow {
                            target: entry.path.clone(),
                            label,
                            summary,
                        },
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
    stage_with(archive, model, replacements, content, game, cancel, dry_run, None)
}
/// `stage`, with where clips made from an animation find their packets and
/// the prepared model's fit (`made`); a made row without it is refused.
#[allow(clippy::too_many_arguments)]
pub fn stage_with(
    archive: &Vpk,
    model: &str,
    replacements: &[ResourceReplacement],
    content: &Path,
    game: &Path,
    cancel: &CancellationToken,
    dry_run: bool,
    made: Option<&crate::made_clips::MadeInputs>,
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
        if let Some(taken) = &item.taken {
            // Taken from another hero: authored from the game's files, no Blender.
            let authored = crate::hero_clips::author_row(archive, model, &target, taken, cancel)?;
            let entry = AnimationEntry {
                target: target.clone(),
                source: PathBuf::from(&taken.clip),
                source_sha256: authored.entry.source_clip_sha256.clone(),
                original_sha256: sha(&authored.original),
                output_sha256: Some(sha(&authored.bytes)),
                status: "verified".into(),
                made: None,
                taken: Some(authored.entry),
            };
            prepared.push((entry, authored.bytes, None));
            continue;
        }
        if let Some(clip) = &item.made {
            // Made from an animation: authored from its packet, no Blender.
            let inputs = made.ok_or_else(|| fail(deadlock_preparation::clip_making::NOT_MADE))?;
            let authored = crate::made_clips::author_row(archive, &target, item, inputs, cancel)?;
            let entry = AnimationEntry {
                target: target.clone(),
                source: PathBuf::from(&authored.stamp.source),
                source_sha256: authored.stamp.source_sha256.clone(),
                original_sha256: sha(&authored.original),
                output_sha256: Some(sha(&authored.bytes)),
                status: "verified".into(),
                made: Some(crate::made_clips::entry(clip, &authored, deadlock_preparation::clip_making::SWITCHES)),
                taken: None,
            };
            prepared.push((entry, authored.bytes, None));
            continue;
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
            made: None,
            taken: None,
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
            let Authored { doc, field, animation } = authored(archive, &row.summary.skeleton, &source, &raw, cancel, |_, _| {})?;
            let target = format!(
                "models/_dmb_custom/animations/{}.dmx",
                &sha(&animation)[..24]
            );
            let text = doc
                .apply(&[Edit {
                    start: field.0,
                    end: field.1,
                    replacement: serde_json::to_string(&target).map_err(fail)?,
                }])
                .map_err(fail)?;
            prepared.push((entry, text.into_bytes(), Some((target, animation))));
        } else {
            return Err(fail(UNSUPPORTED_SOURCE));
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
