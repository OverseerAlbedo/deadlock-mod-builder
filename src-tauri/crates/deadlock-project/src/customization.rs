//! Optional schema-1 fields preserve their original presence. Unknown extension
//! fields are retained as JSON, not interpreted as build or execution authority.
use crate::{asset_paths::relative_asset, Error, Result};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

macro_rules! choices {
    ($name:ident { $($field:ident : $ty:ty),* $(,)? }) => {
        #[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
        pub struct $name {
            $(#[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "present")]
              pub $field: Option<$ty>,)*
            #[serde(flatten)] pub extensions: BTreeMap<String, Value>,
        }
    };
}
// Unfinished preparation reviews intentionally preserve partial roles/landmarks
// and algorithm-specific fields. Processing validates this document, not save.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct PreparationOptions {
    #[serde(flatten)]
    pub values: BTreeMap<String, Value>,
}
fn present<'de, D, T>(deserializer: D) -> std::result::Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}
/// Material numeric overrides distinguish an absent key from explicit `null`,
/// which the legacy planner uses to inherit the selected preset's value.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum NumericChoice {
    Value(f64),
    Inherit(()),
}
impl NumericChoice {
    pub fn value(&self) -> Option<f64> {
        match self {
            Self::Value(value) => Some(*value),
            Self::Inherit(()) => None,
        }
    }
}
choices!(MaterialChoices {
    preset: String, textures: BTreeMap<String, String>, color: [f64; 3], confirmed: bool,
    mode: String, roughness: NumericChoice, metalness: NumericChoice, existing: bool,
    ambiguous: Vec<String>, candidates: BTreeMap<String, Vec<String>>
});
impl MaterialChoices {
    pub fn remove_computed(&mut self) {
        self.existing = None;
        self.ambiguous = None;
        self.candidates = None;
    }
}
// silence (sounds only): the game plays nothing for `target`; the row has no
// source file. Absent unless set, so other rows save exactly as before.
// start_seconds, volume_db, fade_in_seconds, fade_out_seconds (sounds with a
// file only): a sound edit, see `SoundEdit`. Absent unless set.
// made (animations only): the clip is made from an action of the Blender
// file in `source`, see `made_clip::MadeClip`. Absent unless set.
// taken (animations only): the clip is taken from another hero's clip, see
// `taken_clip::TakenClip`; the row has no source file. Absent unless set.
choices!(ResourceReplacement {
    enabled: bool,
    source: String,
    target: String,
    allow_shared: bool,
    original_sha256: String,
    fit: String,
    volume_percent: u16,
    audio_fit: String,
    silence: bool,
    start_seconds: f64,
    volume_db: f64,
    fade_in_seconds: f64,
    fade_out_seconds: f64,
    made: crate::made_clip::MadeClip,
    taken: crate::taken_clip::TakenClip
});
/// Where the part of the user's file that plays starts, at most (seconds).
pub const SOUND_START_MAX: f64 = 3600.0;
/// Volume of a sound edit, in dB relative to the file.
pub const SOUND_VOLUME_DB: (f64, f64) = (-24.0, 12.0);
/// The longest fade the saved format accepts (seconds); the build also keeps
/// each fade within the game sound's length.
pub const SOUND_FADE_MAX: f64 = 60.0;
/// A sound edit: which part of the user's file fills the game sound's fixed
/// length (from `start_seconds`), its volume and its fades. The game's length,
/// rate, channels and loop points never change.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub struct SoundEdit {
    pub start_seconds: f64,
    pub volume_db: f64,
    pub fade_in_seconds: f64,
    pub fade_out_seconds: f64,
}
impl SoundEdit {
    /// The saved format's limits (finite, within range).
    pub fn validate(&self) -> Result<()> {
        number(self.start_seconds, 0., SOUND_START_MAX, "Starts at (seconds)")?;
        number(self.volume_db, SOUND_VOLUME_DB.0, SOUND_VOLUME_DB.1, "Volume (dB)")?;
        number(self.fade_in_seconds, 0., SOUND_FADE_MAX, "Fade in (seconds)")?;
        number(self.fade_out_seconds, 0., SOUND_FADE_MAX, "Fade out (seconds)")
    }
}
impl ResourceReplacement {
    /// The row's sound edit, when any of its four fields is set (missing ones are 0).
    pub fn sound_edit(&self) -> Option<SoundEdit> {
        let fields = [self.start_seconds, self.volume_db, self.fade_in_seconds, self.fade_out_seconds];
        fields.iter().any(Option::is_some).then(|| SoundEdit {
            start_seconds: self.start_seconds.unwrap_or(0.),
            volume_db: self.volume_db.unwrap_or(0.),
            fade_in_seconds: self.fade_in_seconds.unwrap_or(0.),
            fade_out_seconds: self.fade_out_seconds.unwrap_or(0.),
        })
    }
    /// Refuses a sound edit on anything but a sound with a file (`kind` names the row list).
    pub fn check_sound_edit(&self, kind: &str) -> Result<()> {
        let Some(edit) = self.sound_edit() else { return Ok(()) };
        if kind != "audio" {
            return Err(error(format!("Only sounds can be trimmed, faded or made louder; {kind} rows cannot.")));
        }
        if self.silenced() {
            return Err(error("A silenced sound has no file to trim, fade or make louder. Remove the sound edit or give it a file."));
        }
        edit.validate()
    }
    /// Refuses a made clip on anything but an animation row, outside its
    /// limits, or with a source that is not a Blender, FBX or glTF file.
    pub fn check_made(&self, kind: &str) -> Result<()> {
        let Some(made) = &self.made else { return Ok(()) };
        if kind != "animations" {
            return Err(error(format!("{} {kind} rows cannot.", crate::made_clip::ONLY_ANIMATIONS)));
        }
        made.validate()?;
        let source = self.source.as_deref().unwrap_or_default().trim();
        if !source.is_empty() && !crate::made_clip::is_animation_file(source) {
            return Err(error(crate::made_clip::NEEDS_ANIMATION_FILE));
        }
        Ok(())
    }
    /// Refuses a clip taken from another hero on anything but an animation
    /// row, beside `made`, with a file of its own or outside its limits.
    pub fn check_taken(&self, kind: &str) -> Result<()> {
        let Some(taken) = &self.taken else { return Ok(()) };
        if kind != "animations" {
            return Err(error(format!("{} {kind} rows cannot.", crate::taken_clip::ONLY_ANIMATIONS)));
        }
        if self.made.is_some() {
            return Err(error(crate::taken_clip::NOT_BOTH));
        }
        if !self.source.as_deref().unwrap_or_default().trim().is_empty() {
            return Err(error(crate::taken_clip::NO_FILE));
        }
        taken.validate()
    }
    /// A sound row that silences its game file instead of replacing it. A
    /// file wins over `silence` (an older app keeps the unknown key when the
    /// user gives the row a file), so such a row is a replacement and the
    /// project still loads.
    pub fn silenced(&self) -> bool {
        self.silence.unwrap_or(false) && self.source.as_deref().unwrap_or_default().trim().is_empty()
    }
}
choices!(Clothing {
    enabled: bool,
    mode: String,
    source: String
});
choices!(Physics {
    enabled: bool,
    source: String,
    hero_path: String
});
choices!(ClothSetup { meshes: Vec<String>, attachment: String, collision: String });
choices!(SecondaryMotion {
    enabled: bool, preset: String, roots: Vec<String>, profiles: BTreeMap<String, String>, part_kinds: BTreeMap<String, String>,
    tuning: BTreeMap<String, MotionTuning>, use_source_springs: bool
});
// Fine-tuning of one part type (keyed by type: hair, tail, breast ...): each
// value from -1 (much less) to 1 (much more); 0 or missing keeps the automatic value.
choices!(MotionTuning { stiffness: f64, damping: f64, gravity: f64, swing: f64 });
/// Part types the Physics page offers (rigid parts do not move, so they have no tuning).
pub const PART_KINDS: [&str; 11] = ["hair", "streamer", "tail", "ear", "cloth", "long_skirt", "accessory", "breast", "butt", "charm", "rigid"];
choices!(Appearance { enabled: bool, materials: Vec<AppearanceMaterial> });
// hue: experimental color editor hue shift in degrees (-180 to 180); saturation and
// brightness: factors from 0 to 2 where 1 keeps the original. ranges: color
// ranges like a hero color's (`HeroColorRange`, at most 8), applied to the
// base color before hue, saturation and brightness.
choices!(AppearanceMaterial {
    enabled: bool,
    target: String,
    color: [f64; 3],
    roughness: f64,
    metalness: f64,
    opacity: f64,
    emission: f64,
    hue: f64,
    saturation: f64,
    brightness: f64,
    ranges: Vec<HeroColorRange>
});
// Hero colors: the hero's own compiled game materials recolored in place, for
// projects without a replacement model. target is the compiled .vmat_c; color
// replaces the tint, saturation and brightness are factors from 0 to 2 where 1
// keeps the original.
choices!(HeroMaterials { enabled: bool, materials: Vec<HeroMaterial> });
choices!(HeroMaterial {
    enabled: bool,
    target: String,
    color: [f64; 3],
    saturation: f64,
    brightness: f64,
    allow_shared: bool,
    original_sha256: String,
    ranges: Vec<HeroColorRange>
});
// A color range recolors only the base-color pixels it selects (a new texture,
// `deadlock-components color_ranges`): hue centre (0-360) and half width
// (0-180, 180 = every hue), saturation and brightness ranges (0-1), softness
// (0-1), then a hue shift (-180-180) and saturation and brightness factors (0-2).
// set_hue (0-360) and set_saturation (0-1), always together: a new color
// instead of the shift; lift (-1-1): added to the brightness; part {u, v}: only
// the part of the model at that spot. All four absent unless set (`color_ranges`
// switches; a project that holds them is saved with customization schema 2,
// `NEWER_RANGES_SCHEMA`, which an older app refuses to open).
choices!(HeroColorRange {
    enabled: bool,
    hue: f64,
    hue_width: f64,
    saturation_range: [f64; 2],
    brightness_range: [f64; 2],
    softness: f64,
    hue_shift: f64,
    saturation: f64,
    brightness: f64,
    set_hue: f64,
    set_saturation: f64,
    lift: f64,
    part: crate::color_ranges::ColorPart
});
impl HeroColorRange {
    /// The range holds a field an app before these fields would ignore.
    pub fn needs_newer_app(&self) -> bool {
        self.set_hue.is_some() || self.set_saturation.is_some() || self.lift.is_some() || self.part.is_some()
    }
}
/// The customization schema of a project whose color ranges use `set_hue`,
/// `set_saturation`, `lift` or `part`. 0.13.0 refuses to open any schema but 1
/// ("Unsupported character extras version."), so it never builds such a range
/// with the colors it would make of it.
pub const NEWER_RANGES_SCHEMA: u32 = 2;
choices!(Equipment { enabled: bool, meshes: Vec<EquipmentMesh>, props: Vec<Prop>, variants: Vec<Variant> });
choices!(EquipmentMesh { target: String, action: String, sources: Vec<String>, include_lods: bool });
choices!(Prop {
    enabled: bool,
    source: String,
    bone: String,
    translation: [f64; 3],
    rotation: [f64; 3],
    scale: f64
});
choices!(Variant {
    enabled: bool,
    model: String,
    project: String
});
choices!(Face { enabled: bool, mode: String, weights: BTreeMap<String, f64> });
// Face layers a source keeps fully transparent until an expression fades them
// in (blush, tears, other): true shows that kind in game, unset or false hides
// it. Applied when material files are written, so no new preparation is needed.
choices!(FaceLayers { blush: bool, tears: bool, other: bool });
impl FaceLayers {
    /// Whether layers of `kind` (blush, tears or other) are shown.
    pub fn shown(&self, kind: &str) -> bool {
        match kind {
            "blush" => self.blush,
            "tears" => self.tears,
            _ => self.other,
        }
        .unwrap_or(false)
    }
}
choices!(Customization {
    schema: u32, enabled: bool, model_enabled: bool, hero_path: String,
    icons: Vec<ResourceReplacement>, item_icons: Vec<ResourceReplacement>, animations: Vec<ResourceReplacement>,
    effects: Vec<ResourceReplacement>, audio: Vec<ResourceReplacement>,
    clothing: Clothing, physics: Physics, cloth_setup: ClothSetup, secondary_motion: SecondaryMotion,
    appearance: Appearance, equipment: Equipment, face: Face, face_layers: FaceLayers,
    hero_materials: HeroMaterials,
    effect_edits: Vec<crate::effect_edit::EffectEditRow>,
    ability_objects: Vec<crate::ability_objects::AbilityObjectRow>,
    item_names: Vec<crate::item_names::ItemNameRow>
});
fn error(text: impl Into<String>) -> Error {
    Error::Validation(text.into())
}
fn clean(value: &str, label: &str) -> Result<()> {
    if value.contains('\0') {
        return Err(error(format!("Invalid {label} path.")));
    }
    Ok(())
}
fn number(value: f64, low: f64, high: f64, label: &str) -> Result<()> {
    if !value.is_finite() || value < low || value > high {
        return Err(error(format!(
            "{label} must be a finite number from {low} to {high}."
        )));
    }
    Ok(())
}
/// The color ranges of one hero color or appearance row (`owner` starts the
/// message about their number).
fn color_ranges(ranges: &[HeroColorRange], owner: &str) -> Result<()> {
    if ranges.len() > crate::color_ranges::MAX_RANGES {
        return Err(error(format!("{owner} holds at most {} color ranges.", crate::color_ranges::MAX_RANGES)));
    }
    for range in ranges {
        for (v, low, high, label) in [
            (range.hue, 0., 360., "Range hue"),
            (range.hue_width, 0., 180., "Range hue width"),
            (range.softness, 0., 1., "Range softness"),
            (range.hue_shift, -180., 180., "Hue shift"),
            (range.saturation, 0., 2., "Range saturation"),
            (range.brightness, 0., 2., "Range brightness"),
            (range.set_hue, 0., 360., "Range new hue"),
            (range.set_saturation, 0., 1., "Range new saturation"),
            (range.lift, -crate::color_ranges::LIFT_LIMIT, crate::color_ranges::LIFT_LIMIT, "Range lift"),
            (range.part.map(|p| p.u), -crate::color_ranges::PART_LIMIT, crate::color_ranges::PART_LIMIT, "Range part"),
            (range.part.map(|p| p.v), -crate::color_ranges::PART_LIMIT, crate::color_ranges::PART_LIMIT, "Range part"),
        ] {
            if let Some(v) = v {
                number(v, low, high, label)?;
            }
        }
        if range.set_hue.is_some() != range.set_saturation.is_some() {
            return Err(error(crate::color_ranges::GIVEN_COLOUR_PAIR));
        }
        for (pair, label) in [(range.saturation_range, "Saturation range"), (range.brightness_range, "Brightness range")] {
            if let Some([from, to]) = pair {
                number(from, 0., 1., label)?;
                number(to, 0., 1., label)?;
                if from > to {
                    return Err(error(format!("{label} starts above its end.")));
                }
            }
        }
    }
    Ok(())
}
fn lower(value: &str) -> String {
    deadlock_core::text::casefold(value)
}
fn sources(values: &[String]) -> Result<()> {
    let mut seen = BTreeSet::new();
    for value in values {
        if value.is_empty() || value == "@keep" || value == "@disable" || !seen.insert(value) {
            return Err(error(
                "Equipment replacement sources must be unique prepared mesh names.",
            ));
        }
    }
    Ok(())
}
impl Customization {
    pub fn active(&self) -> bool {
        self.enabled.unwrap_or(false)
    }
    pub fn model_enabled(&self) -> bool {
        !self.active() || self.model_enabled.unwrap_or(true)
    }
    /// Enabled effect edits a build would have to apply (Extras on).
    pub fn enabled_effect_edits(&self) -> usize {
        if !self.active() {
            return 0;
        }
        self.effect_edits.as_deref().unwrap_or_default().iter().filter(|r| r.enabled).count()
    }

    /// Every saved color range (hero colors and Surface changes rows).
    pub fn color_ranges(&self) -> impl Iterator<Item = &HeroColorRange> {
        let hero = self.hero_materials.iter().flat_map(|h| h.materials.iter().flatten()).flat_map(|row| row.ranges.iter().flatten());
        let appearance = self.appearance.iter().flat_map(|a| a.materials.iter().flatten()).flat_map(|row| row.ranges.iter().flatten());
        hero.chain(appearance)
    }
    /// The schema a saved file gets: `NEWER_RANGES_SCHEMA` while a color range
    /// holds a field older apps would ignore, back to 1 when none does; other
    /// projects keep exactly what they had (absent stays absent).
    pub fn mark_for_older_apps(&mut self) {
        if self.color_ranges().any(HeroColorRange::needs_newer_app) {
            self.schema = Some(NEWER_RANGES_SCHEMA);
        } else if self.schema == Some(NEWER_RANGES_SCHEMA) {
            self.schema = Some(1);
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.schema.is_some_and(|v| v != 1 && v != NEWER_RANGES_SCHEMA) {
            return Err(error("Unsupported character extras version."));
        }
        for (kind, rows, suffix) in [
            ("icons", &self.icons, ".vtex_c"),
            ("animations", &self.animations, ".vnmclip_c"),
            ("effects", &self.effects, ".vpcf_c"),
            ("audio", &self.audio, ".vsnd_c"),
        ] {
            let rows = rows.as_deref().unwrap_or_default();
            if rows.len() > 128 {
                return Err(error(format!(
                    "Character extras {kind} must contain at most 128 replacements."
                )));
            }
            let mut seen = BTreeSet::new();
            for row in rows {
                let target = row.target.as_deref().unwrap_or_default();
                clean(target, kind)?;
                clean(row.source.as_deref().unwrap_or_default(), kind)?;
                if !target.is_empty() {
                    relative_asset(target)?;
                    if !lower(target).ends_with(suffix) {
                        return Err(error(format!(
                            "{kind} destination must be an existing {suffix} game resource."
                        )));
                    }
                    if row.enabled.unwrap_or(true) && !seen.insert(lower(target)) {
                        return Err(error(format!(
                            "Two enabled replacements target the same game asset: {target}"
                        )));
                    }
                }
                if kind == "icons"
                    && row
                        .fit
                        .as_ref()
                        .is_some_and(|v| !["contain", "cover", "stretch"].contains(&v.as_str()))
                {
                    return Err(error("Icon fit must be contain, cover or stretch."));
                }
                if let Some(hash) = &row.original_sha256 {
                    if !hash.is_empty()
                        && (hash.len() != 64
                            || !hash
                                .bytes()
                                .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v)))
                    {
                        return Err(error("Invalid original character asset fingerprint."));
                    }
                }
                if row.volume_percent.is_some_and(|v| v > 200) {
                    return Err(error("Replacement volume must be between 0 and 200 percent."));
                }
                if row.audio_fit.as_ref().is_some_and(|v| !["exact", "pad", "trim"].contains(&v.as_str())) {
                    return Err(error("Audio timing must be exact, pad or trim."));
                }
                if row.silence == Some(true) && kind != "audio" {
                    return Err(error(format!("Only sounds can be silenced; {kind} rows need a file.")));
                }
                row.check_sound_edit(kind)?;
                row.check_made(kind)?;
                row.check_taken(kind)?;
            }
        }
        crate::item_icons::validate(self.item_icons.as_deref().unwrap_or_default(), self.icons.as_deref().unwrap_or_default())?;
        if let Some(clothing) = &self.clothing {
            clean(
                clothing.source.as_deref().unwrap_or_default(),
                "garment source",
            )?;
            if clothing
                .mode
                .as_ref()
                .is_some_and(|v| !["weighted", "simulated"].contains(&v.as_str()))
            {
                return Err(error("Unknown clothing movement mode."));
            }
        }
        if let Some(physics) = &self.physics {
            clean(
                physics.source.as_deref().unwrap_or_default(),
                "tested cloth physics source",
            )?;
            if let Some(hero) = &physics.hero_path {
                clean(hero, "tested cloth physics hero_path")?;
                if !hero.is_empty() {
                    relative_asset(hero)?;
                }
            }
        }
        if let Some(motion) = &self.secondary_motion {
            if motion.preset.as_deref().is_some_and(|v| !["automatic", "tangtang_r2", "tangtang_reference"].contains(&v)) {
                return Err(error("Unknown secondary motion preset."));
            }
            if let Some(roots) = &motion.roots {
                if roots.is_empty() || roots.len() > 128 || roots.iter().any(|r| r.is_empty() || r.contains('\0'))
                    || roots.iter().collect::<BTreeSet<_>>().len() != roots.len() {
                    return Err(error("Select unique secondary motion groups (1–128)."));
                }
            }
            if motion.profiles.as_ref().is_some_and(|p| p.len() > 128 || p.iter().any(|(k,v)| k.is_empty() || !["automatic", "balanced", "stiff", "soft"].contains(&v.as_str()))) {
                return Err(error("Secondary group movement must be automatic, balanced, stiff or soft."));
            }
            if motion.part_kinds.as_ref().is_some_and(|p|p.len()>128 || p.iter().any(|(k,v)|k.is_empty() || !PART_KINDS.contains(&v.as_str()))) {
                return Err(error("Choose a supported moving-part type."));
            }
            if let Some(tuning) = &motion.tuning {
                let in_range = |v: &Option<f64>| v.is_none_or(|v| v.is_finite() && (-1.0..=1.0).contains(&v));
                if tuning.iter().any(|(kind, t)| kind == "rigid" || !PART_KINDS.contains(&kind.as_str())
                    || ![&t.stiffness, &t.damping, &t.gravity, &t.swing].into_iter().all(in_range)) {
                    return Err(error("Movement fine-tuning must name a moving part type and stay between -100% and +100%."));
                }
            }
            if self.active() && motion.enabled == Some(true) && self.physics.as_ref().is_some_and(|p| p.enabled == Some(true)) {
                return Err(error("Choose generated secondary motion or a tested cloth reference, not both."));
            }
        }
        if let Some(cloth) = &self.cloth_setup {
            let rows = cloth.meshes.as_deref().unwrap_or_default();
            if rows.len() > 1 || rows.iter().any(|x| x.trim().is_empty() || x.contains('\0')) {
                return Err(error(
                    "Choose one garment mesh at a time for automatic cloth setup.",
                ));
            }
            if cloth
                .attachment
                .as_ref()
                .is_some_and(|v| !["top", "bottom"].contains(&v.as_str()))
            {
                return Err(error(
                    "Choose the top or bottom attachment edge for cloth setup.",
                ));
            }
            if cloth
                .collision
                .as_ref()
                .is_some_and(|v| !["legs", "none"].contains(&v.as_str()))
            {
                return Err(error(
                    "Choose leg collision or no collision for cloth setup.",
                ));
            }
        }
        if let Some(appearance) = &self.appearance {
            let rows = appearance.materials.as_deref().unwrap_or_default();
            if rows.len() > 256 {
                return Err(error("Appearance supports at most 256 material overrides."));
            }
            let mut seen = BTreeSet::new();
            for row in rows {
                let target = row.target.as_deref().unwrap_or_default();
                if !target.is_empty() {
                    relative_asset(target)?;
                    if !lower(target).ends_with(".vmat") {
                        return Err(error("Appearance selects a source .vmat material."));
                    }
                    if row.enabled.unwrap_or(true) && !seen.insert(lower(target)) {
                        return Err(error("Duplicate appearance override."));
                    }
                }
                if let Some(rgb) = row.color {
                    for x in rgb {
                        number(x, 0., 1., "Color")?;
                    }
                }
                for (v, label) in [
                    (row.roughness, "Roughness"),
                    (row.metalness, "Metalness"),
                    (row.opacity, "Opacity"),
                ] {
                    if let Some(v) = v {
                        number(v, 0., 1., label)?;
                    }
                }
                if let Some(v) = row.emission {
                    number(v, 0., 64., "Emission")?;
                }
                if let Some(v) = row.hue {
                    number(v, -180., 180., "Hue shift")?;
                }
                for (v, label) in [(row.saturation, "Saturation"), (row.brightness, "Brightness")] {
                    if let Some(v) = v {
                        number(v, 0., 2., label)?;
                    }
                }
                color_ranges(row.ranges.as_deref().unwrap_or_default(), "A material")?;
            }
        }
        if let Some(colors) = &self.hero_materials {
            let rows = colors.materials.as_deref().unwrap_or_default();
            if rows.len() > 64 {
                return Err(error("Hero colors support at most 64 materials."));
            }
            let mut seen = BTreeSet::new();
            for row in rows {
                let target = row.target.as_deref().unwrap_or_default();
                clean(target, "hero material")?;
                if !target.is_empty() {
                    relative_asset(target)?;
                    if !lower(target).ends_with(".vmat_c") {
                        return Err(error("Hero colors change a compiled .vmat_c game material."));
                    }
                    if row.enabled.unwrap_or(true) && !seen.insert(lower(target)) {
                        return Err(error(format!("Two enabled hero colors change the same material: {target}")));
                    }
                }
                if let Some(rgb) = row.color {
                    for x in rgb {
                        number(x, 0., 1., "Tint")?;
                    }
                }
                for (v, label) in [(row.saturation, "Saturation"), (row.brightness, "Brightness")] {
                    if let Some(v) = v {
                        number(v, 0., 2., label)?;
                    }
                }
                color_ranges(row.ranges.as_deref().unwrap_or_default(), "A hero color")?;
                if let Some(hash) = row.original_sha256.as_deref().filter(|h| !h.is_empty()) {
                    if hash.len() != 64 || !hash.bytes().all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v)) {
                        return Err(error("Invalid original hero material fingerprint."));
                    }
                }
            }
        }
        if let Some(equipment) = &self.equipment {
            let meshes = equipment.meshes.as_deref().unwrap_or_default();
            if meshes.len() > 256 {
                return Err(error("Equipment supports at most 256 mesh choices."));
            }
            let mut seen = BTreeSet::new();
            for row in meshes {
                let target = row.target.as_deref().unwrap_or_default();
                if !target.is_empty() && !seen.insert(target) {
                    return Err(error("Duplicate equipment mesh choice."));
                }
                if row
                    .action
                    .as_ref()
                    .is_some_and(|v| !["replace", "hide", "keep"].contains(&v.as_str()))
                {
                    return Err(error("Choose Replace, Hide or Keep for equipment."));
                }
                sources(row.sources.as_deref().unwrap_or_default())?;
            }
            let props = equipment.props.as_deref().unwrap_or_default();
            if props.len() > 64 {
                return Err(error("Equipment supports at most 64 attached props."));
            }
            for row in props {
                clean(row.source.as_deref().unwrap_or_default(), "prop source")?;
                clean(row.bone.as_deref().unwrap_or_default(), "prop bone")?;
                for vector in [row.translation, row.rotation].into_iter().flatten() {
                    if vector.iter().any(|x| !x.is_finite()) {
                        return Err(error(
                            "Prop transform requires three finite numeric values.",
                        ));
                    }
                }
                if row.scale.is_some_and(|s| !s.is_finite() || s <= 0.) {
                    return Err(error("Prop scale must be a positive finite number."));
                }
            }
            let variants = equipment.variants.as_deref().unwrap_or_default();
            if variants.len() > 16 {
                return Err(error("Choose at most 16 alternate model projects."));
            }
            let mut seen = BTreeSet::new();
            for row in variants {
                clean(
                    row.project.as_deref().unwrap_or_default(),
                    "alternate project",
                )?;
                let target = row.model.as_deref().unwrap_or_default();
                clean(target, "alternate model")?;
                if !target.is_empty() {
                    relative_asset(target)?;
                    if !target.ends_with(".vmdl") && !target.ends_with(".vmdl_c") {
                        return Err(error(
                            "An alternate model must target a VMDL game resource.",
                        ));
                    }
                    if row.enabled.unwrap_or(true)
                        && !seen.insert(lower(target.strip_suffix("_c").unwrap_or(target)))
                    {
                        return Err(error("Choose only one project for each alternate model."));
                    }
                }
            }
        }
        crate::effect_edit::validate_rows(self.effect_edits.as_deref().unwrap_or_default())?;
        crate::ability_objects::validate_rows(self.ability_objects.as_deref().unwrap_or_default())?;
        crate::item_names::validate_rows(self.item_names.as_deref().unwrap_or_default())?;
        if let Some(face) = &self.face {
            if face.mode.as_ref().is_some_and(|v| v != "static") {
                return Err(error(
                    "Only explicit static facial shape baking is supported.",
                ));
            }
            if let Some(weights) = &face.weights {
                if weights.len() > 512 {
                    return Err(error("Face supports at most 512 static shape values."));
                }
                for (name, value) in weights {
                    if name.is_empty() || name.contains('\0') {
                        return Err(error("Invalid facial shape name."));
                    }
                    number(*value, 0., 1., "Static facial shape")?;
                }
            }
        }
        Ok(())
    }
    pub fn map_sources(&mut self, mut transform: impl FnMut(&str) -> Result<String>) -> Result<()> {
        self.validate()?;
        fn change(
            value: &mut Option<String>,
            transform: &mut impl FnMut(&str) -> Result<String>,
        ) -> Result<()> {
            if let Some(source) = value {
                if !source.is_empty() {
                    *source = transform(source)?;
                }
            }
            Ok(())
        }
        for rows in [
            &mut self.icons,
            &mut self.item_icons,
            &mut self.animations,
            &mut self.effects,
            &mut self.audio,
        ]
        .into_iter()
        .flatten()
        {
            for row in rows {
                change(&mut row.source, &mut transform)?;
            }
        }
        if let Some(v) = &mut self.clothing {
            change(&mut v.source, &mut transform)?;
        }
        for row in self.ability_objects.iter_mut().flatten() {
            if !row.source.is_empty() {
                row.source = transform(&row.source)?;
            }
        }
        if let Some(v) = &mut self.physics {
            change(&mut v.source, &mut transform)?;
        }
        if let Some(v) = &mut self.equipment {
            if let Some(rows) = &mut v.props {
                for row in rows {
                    change(&mut row.source, &mut transform)?;
                }
            }
            if let Some(rows) = &mut v.variants {
                for row in rows {
                    change(&mut row.project, &mut transform)?;
                }
            }
        }
        Ok(())
    }
    pub fn clear_targets(&mut self) {
        for rows in [
            &mut self.icons,
            &mut self.animations,
            &mut self.effects,
            &mut self.audio,
        ]
        .into_iter()
        .flatten()
        {
            for row in rows {
                row.target = Some(String::new());
                row.original_sha256 = None;
            }
        }
        if let Some(v) = &mut self.appearance {
            if let Some(rows) = &mut v.materials {
                for row in rows {
                    row.target = Some(String::new());
                }
            }
        }
        // Effect edits belong to one hero's effects.
        if self.effect_edits.is_some() {
            self.effect_edits = Some(Vec::new());
        }
        // Ability objects replace one hero's ability models.
        if self.ability_objects.is_some() {
            self.ability_objects = Some(Vec::new());
        }
        // Item icons and item names are the same for every hero: kept.
        // Hero colors belong to one hero's materials and hold no user files.
        if let Some(v) = &mut self.hero_materials {
            if v.materials.is_some() {
                v.materials = Some(Vec::new());
            }
        }
        if let Some(v) = &mut self.equipment {
            if let Some(rows) = &mut v.meshes {
                for row in rows {
                    row.target = Some(String::new());
                }
            }
            if let Some(rows) = &mut v.props {
                for row in rows {
                    row.bone = Some(String::new());
                }
            }
            if let Some(rows) = &mut v.variants {
                for row in rows {
                    row.model = Some(String::new());
                }
            }
        }
        self.hero_path = None;
        if let Some(v) = &mut self.secondary_motion { v.enabled = Some(false); }
        if let Some(v) = &mut self.physics {
            v.enabled = Some(false);
            v.hero_path = None;
        }
        if let Some(v) = &mut self.cloth_setup {
            v.meshes = Some(Vec::new());
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn effect_edits_are_absent_unless_set_and_validated() {
        // A document without the field saves without it.
        let empty: super::Customization = serde_json::from_value(serde_json::json!({"enabled": true})).unwrap();
        assert!(!serde_json::to_string(&empty).unwrap().contains("effect_edits"));
        assert_eq!(empty.enabled_effect_edits(), 0);
        let row = serde_json::json!({"target": "particles/weapon_fx/ghost/ghost_tracer.vpcf_c", "edit": {"hue": 115}});
        let mut with: super::Customization = serde_json::from_value(serde_json::json!({"enabled": true, "effect_edits": [row.clone()]})).unwrap();
        assert!(with.validate().is_ok());
        assert_eq!(with.enabled_effect_edits(), 1);
        // Turned off, or Extras off: nothing to build.
        let off: super::Customization = serde_json::from_value(serde_json::json!({"enabled": false, "effect_edits": [row.clone()]})).unwrap();
        assert_eq!(off.enabled_effect_edits(), 0);
        let disabled: super::Customization = serde_json::from_value(serde_json::json!({"enabled": true, "effect_edits": [{"target": "particles/a.vpcf_c", "enabled": false}]})).unwrap();
        assert_eq!(disabled.enabled_effect_edits(), 0);
        // Two rows for one effect are refused; unknown keys inside an edit too.
        let twice: super::Customization = serde_json::from_value(serde_json::json!({"effect_edits": [row.clone(), {"target": "PARTICLES/weapon_fx/ghost/ghost_tracer.vpcf_c"}]})).unwrap();
        assert!(twice.validate().is_err());
        assert!(serde_json::from_value::<super::Customization>(serde_json::json!({"effect_edits": [{"target": "particles/a.vpcf_c", "edit": {"speed": 2}}]})).is_err());
        // Another hero: the edits are cleared.
        with.clear_targets();
        assert_eq!(with.effect_edits, Some(vec![]));
    }
    #[test]
    fn item_names_are_absent_unless_set_validated_and_kept_for_another_hero() {
        let empty: super::Customization = serde_json::from_value(serde_json::json!({"enabled": true, "item_icons": []})).unwrap();
        assert!(!serde_json::to_string(&empty).unwrap().contains("item_names"));
        let row = serde_json::json!({"item": "upgrade_sample", "name": "New Name"});
        let mut with: super::Customization = serde_json::from_value(serde_json::json!({"enabled": true, "item_names": [row.clone()]})).unwrap();
        assert!(with.validate().is_ok());
        let twice: super::Customization = serde_json::from_value(serde_json::json!({"item_names": [row.clone(), {"item": "UPGRADE_sample", "name": "B"}]})).unwrap();
        assert!(twice.validate().is_err());
        let bad: super::Customization = serde_json::from_value(serde_json::json!({"item_names": [{"item": "upgrade_sample", "name": "Say \"hi\""}]})).unwrap();
        assert!(bad.validate().unwrap_err().to_string().contains(crate::item_names::NAME_REFUSED_CHAR));
        assert!(serde_json::from_value::<super::Customization>(serde_json::json!({"item_names": [{"item": "upgrade_a", "name": "A", "lang": "x"}]})).is_err());
        // The same for every hero: another hero keeps them.
        with.clear_targets();
        assert_eq!(with.item_names.as_ref().unwrap().len(), 1);
    }
    #[test]
    fn ability_objects_are_absent_unless_set_and_cleared_for_another_hero() {
        let empty: super::Customization = serde_json::from_value(serde_json::json!({"enabled": true, "icons": []})).unwrap();
        let text = serde_json::to_string(&empty).unwrap();
        assert!(!text.contains("ability_objects"), "{text}");
        let row = serde_json::json!({"target": "models/abilities/sample_dagger_model.vmdl", "source": "C:/files/shape.fbx", "fit": {"scale": 1.5}});
        let mut with: super::Customization = serde_json::from_value(serde_json::json!({"enabled": true, "ability_objects": [row.clone()]})).unwrap();
        assert!(with.validate().is_ok());
        // The source is a user file: it follows a project move like the others.
        with.map_sources(|path| Ok(path.replace("C:/files", "D:/moved"))).unwrap();
        assert_eq!(with.ability_objects.as_ref().unwrap()[0].source, "D:/moved/shape.fbx");
        let twice: super::Customization = serde_json::from_value(serde_json::json!({"ability_objects": [row.clone(), {"target": "MODELS/abilities/sample_dagger_model.vmdl"}]})).unwrap();
        assert!(twice.validate().is_err());
        assert!(serde_json::from_value::<super::Customization>(serde_json::json!({"ability_objects": [{"target": "models/a.vmdl", "size": "match"}]})).is_err());
        with.clear_targets();
        assert_eq!(with.ability_objects, Some(vec![]));
    }
    #[test]
    fn a_silenced_sound_is_a_row_without_a_file_and_absent_unless_set() {
        let target = "sounds/abilities/haze/haze_bullet_flurry_cast_start.vsnd_c";
        let check = |value: serde_json::Value| {
            serde_json::from_value::<super::Customization>(value).unwrap().validate().map_err(|e| e.to_string())
        };
        // A replacement row saves exactly as before: no silence key.
        let replacement = serde_json::json!({"enabled": true, "source": "C:/sfx/a.wav", "target": target, "volume_percent": 100, "audio_fit": "pad"});
        let row: super::ResourceReplacement = serde_json::from_value(replacement.clone()).unwrap();
        assert!(!row.silenced());
        assert_eq!(serde_json::to_value(&row).unwrap(), replacement);
        // A silenced row round-trips with its flag and no source.
        let silent = serde_json::json!({"enabled": true, "target": target, "allow_shared": false, "silence": true});
        let row: super::ResourceReplacement = serde_json::from_value(silent.clone()).unwrap();
        assert!(row.silenced());
        assert_eq!(serde_json::to_value(&row).unwrap(), silent);
        assert!(check(serde_json::json!({"audio": [silent.clone()]})).is_ok());
        assert!(check(serde_json::json!({"audio": [{"target": target, "silence": false, "source": "a.wav"}]})).is_ok());
        // An older app keeps the unknown key when the user gives the row a
        // file: the file wins and the project still loads.
        let both = serde_json::json!({"target": target, "silence": true, "source": "C:/sfx/a.wav"});
        assert!(check(serde_json::json!({"audio": [both.clone()]})).is_ok());
        assert!(!serde_json::from_value::<super::ResourceReplacement>(both).unwrap().silenced());
        let mut moved: super::Customization = serde_json::from_value(serde_json::json!({"audio": [silent.clone(), {"target": "sounds/b.vsnd_c", "source": "b.wav"}]})).unwrap();
        moved.map_sources(|s| Ok(format!("moved/{s}"))).unwrap();
        assert_eq!(serde_json::to_value(&moved.audio.as_ref().unwrap()[0]).unwrap(), silent, "a silenced row has no path to move");
        assert_eq!(
            check(serde_json::json!({"icons": [{"target": "panorama/images/a.vtex_c", "silence": true}]})).unwrap_err(),
            "Only sounds can be silenced; icons rows need a file."
        );
        assert!(check(serde_json::json!({"item_icons": [{"target": "panorama/images/upgrades/a.vtex_c", "silence": true}]})).unwrap_err().contains("Only sounds can be silenced"));
        // Silenced rows count toward the 128 limit and the one-row-per-sound rule.
        let many: Vec<_> = (0..129).map(|i| serde_json::json!({"target": format!("sounds/a/{i}.vsnd_c"), "silence": true})).collect();
        assert!(check(serde_json::json!({"audio": many})).unwrap_err().contains("at most 128"));
        assert!(check(serde_json::json!({"audio": [silent.clone(), {"target": target, "source": "b.wav"}]})).unwrap_err().contains("same game asset"));
        assert!(serde_json::from_value::<super::Customization>(serde_json::json!({"audio": [{"target": target, "silence": "yes"}]})).is_err());
    }
    #[test]
    fn a_sound_edit_is_four_optional_fields_on_a_sound_with_a_file() {
        let target = "sounds/abilities/haze/haze_bullet_flurry_cast_start.vsnd_c";
        let check = |value: serde_json::Value| {
            serde_json::from_value::<super::Customization>(value).unwrap().validate().map_err(|e| e.to_string())
        };
        // Without an edit the row saves exactly as before and has no edit.
        let plain = serde_json::json!({"enabled": true, "source": "C:/sfx/a.wav", "target": target, "volume_percent": 100, "audio_fit": "pad"});
        let row: super::ResourceReplacement = serde_json::from_value(plain.clone()).unwrap();
        assert_eq!(row.sound_edit(), None);
        assert_eq!(serde_json::to_value(&row).unwrap(), plain);
        // One field makes an edit; the others are 0.
        let edited = serde_json::json!({"enabled": true, "source": "C:/sfx/a.wav", "target": target, "start_seconds": 2.6, "volume_db": -6.5, "fade_in_seconds": 0.15, "fade_out_seconds": 0.37});
        let row: super::ResourceReplacement = serde_json::from_value(edited.clone()).unwrap();
        assert_eq!(row.sound_edit(), Some(super::SoundEdit { start_seconds: 2.6, volume_db: -6.5, fade_in_seconds: 0.15, fade_out_seconds: 0.37 }));
        assert_eq!(serde_json::to_value(&row).unwrap(), edited, "an edit round-trips");
        assert!(check(serde_json::json!({"audio": [edited]})).is_ok());
        let only = serde_json::from_value::<super::ResourceReplacement>(serde_json::json!({"source": "a.wav", "fade_out_seconds": 1.0})).unwrap();
        assert_eq!(only.sound_edit(), Some(super::SoundEdit { fade_out_seconds: 1.0, ..Default::default() }));
        // Ranges.
        for (key, value) in [("start_seconds", -0.1), ("start_seconds", 3600.5), ("volume_db", -24.5), ("volume_db", 12.5),
            ("fade_in_seconds", -1.0), ("fade_in_seconds", 61.0), ("fade_out_seconds", 60.5)] {
            let mut row = serde_json::json!({"target": target, "source": "a.wav"});
            row[key] = serde_json::json!(value);
            assert!(check(serde_json::json!({"audio": [row]})).is_err(), "{key} {value}");
        }
        for (key, value) in [("start_seconds", 0.0), ("start_seconds", 3600.0), ("volume_db", -24.0), ("volume_db", 12.0), ("fade_in_seconds", 60.0)] {
            let mut row = serde_json::json!({"target": target, "source": "a.wav"});
            row[key] = serde_json::json!(value);
            assert!(check(serde_json::json!({"audio": [row]})).is_ok(), "{key} {value}");
        }
        // Not finite: refused by the check itself (JSON cannot carry it).
        let infinite = super::ResourceReplacement { source: Some("a.wav".into()), volume_db: Some(f64::NAN), ..Default::default() };
        assert!(infinite.check_sound_edit("audio").is_err());
        assert!(serde_json::from_value::<super::Customization>(serde_json::json!({"audio": [{"target": target, "volume_db": "loud"}]})).is_err());
        // Sounds with a file only.
        for kind in ["icons", "effects", "animations"] {
            let suffix = match kind { "icons" => "panorama/images/a.vtex_c", "effects" => "particles/a.vpcf_c", _ => "anims/a.vnmclip_c" };
            let message = check(serde_json::json!({kind: [{"target": suffix, "source": "a.png", "volume_db": 3.0}]})).unwrap_err();
            assert!(message.contains("Only sounds can be trimmed"), "{kind}: {message}");
        }
        assert!(check(serde_json::json!({"item_icons": [{"target": "panorama/images/upgrades/a.vtex_c", "source": "a.png", "start_seconds": 1.0}]})).unwrap_err().contains("Only sounds can be trimmed"));
        let silenced = check(serde_json::json!({"audio": [{"target": target, "silence": true, "fade_in_seconds": 0.5}]})).unwrap_err();
        assert!(silenced.contains("silenced sound has no file"), "{silenced}");
        // A file wins over silence, so such a row may carry an edit.
        assert!(check(serde_json::json!({"audio": [{"target": target, "silence": true, "source": "a.wav", "fade_in_seconds": 0.5}]})).is_ok());
        // Edited rows keep the 128-row limit.
        let many: Vec<_> = (0..129).map(|i| serde_json::json!({"target": format!("sounds/a/{i}.vsnd_c"), "source": "a.wav", "volume_db": 1.0})).collect();
        assert!(check(serde_json::json!({"audio": many})).unwrap_err().contains("at most 128"));
    }
    #[test]
    fn every_part_type_the_physics_page_offers_is_accepted() {
        for kind in ["hair", "streamer", "tail", "ear", "cloth", "long_skirt", "accessory", "breast", "butt", "charm", "rigid"] {
            let choice: super::Customization = serde_json::from_value(serde_json::json!({"secondary_motion": {"part_kinds": {"hip_L": kind}}})).unwrap();
            assert!(choice.validate().is_ok(), "{kind}");
        }
        let unknown: super::Customization = serde_json::from_value(serde_json::json!({"secondary_motion": {"part_kinds": {"hip_L": "wing"}}})).unwrap();
        assert!(unknown.validate().is_err());
    }
    #[test]
    fn hero_colors_change_compiled_materials_with_values_in_range() {
        let check = |rows: serde_json::Value| {
            let choice: super::Customization = serde_json::from_value(serde_json::json!({"hero_materials": {"enabled": true, "materials": rows}})).unwrap();
            choice.validate().is_ok()
        };
        let target = "models/heroes_staging/haze/materials/hazev2.vmat_c";
        assert!(check(serde_json::json!([{"target": target, "color": [0.0, 0.0, 0.0], "saturation": 0.5, "brightness": 2.0}])));
        assert!(check(serde_json::json!([{"target": ""}])), "an empty row is saved; the build asks for a material");
        assert!(!check(serde_json::json!([{"target": "models/heroes_staging/haze/materials/hazev2.vmat"}])), "a source .vmat is not a game file");
        assert!(!check(serde_json::json!([{"target": target}, {"target": target.to_uppercase()}])));
        assert!(check(serde_json::json!([{"target": target}, {"target": target, "enabled": false}])));
        assert!(!check(serde_json::json!([{"target": target, "color": [1.5, 0.0, 0.0]}])));
        assert!(!check(serde_json::json!([{"target": target, "brightness": 2.5}])));
        assert!(!check(serde_json::json!([{"target": target, "original_sha256": "abc"}])));
        assert!(!check(serde_json::json!([{"target": "../outside.vmat_c"}])));
        let range = |r: serde_json::Value| check(serde_json::json!([{"target": target, "ranges": [r]}]));
        assert!(range(serde_json::json!({"hue": 26.0, "hue_width": 12.0, "saturation_range": [0.6, 1.0], "brightness_range": [0.2, 1.0], "softness": 0.25, "hue_shift": 180.0})));
        assert!(range(serde_json::json!({"brightness": 0.0})), "unset values take their defaults");
        assert!(!range(serde_json::json!({"hue": 361.0})));
        assert!(!range(serde_json::json!({"hue_width": 200.0})));
        assert!(!range(serde_json::json!({"hue_shift": -190.0})));
        assert!(!range(serde_json::json!({"saturation_range": [0.8, 0.2]})), "a range must not end before it starts");
        assert!(!range(serde_json::json!({"brightness_range": [0.0, 1.5]})));
        assert!(!check(serde_json::json!([{"target": target, "ranges": vec![serde_json::json!({}); 9]}])));
        let mut choice: super::Customization = serde_json::from_value(serde_json::json!({"hero_materials": {"enabled": true, "materials": [{"target": target}]}})).unwrap();
        choice.clear_targets();
        assert_eq!(choice.hero_materials.unwrap().materials, Some(vec![]), "another hero's materials are not kept");
    }
    #[test]
    fn appearance_rows_take_color_ranges_like_hero_colors() {
        let check = |ranges: serde_json::Value| {
            let choice: super::Customization = serde_json::from_value(serde_json::json!({"appearance": {"enabled": true, "materials": [{"target": "m/body.vmat", "ranges": ranges}]}})).unwrap();
            choice.validate().map_err(|e| e.to_string())
        };
        assert!(check(serde_json::json!([{"hue": 26.0, "hue_width": 12.0, "saturation_range": [0.6, 1.0], "hue_shift": 180.0}])).is_ok());
        assert!(check(serde_json::json!([{}])).is_ok(), "unset values take their defaults");
        assert_eq!(check(serde_json::json!(vec![serde_json::json!({}); 9])).unwrap_err(), "A material holds at most 8 color ranges.");
        assert!(check(serde_json::json!([{"hue": 361.0}])).unwrap_err().contains("Range hue"));
        assert!(check(serde_json::json!([{"brightness_range": [0.8, 0.2]}])).unwrap_err().contains("starts above its end"));
        // Rows without ranges serialize as before.
        let row: super::AppearanceMaterial = serde_json::from_value(serde_json::json!({"target": "m/body.vmat", "hue": 20.0})).unwrap();
        assert_eq!(serde_json::to_value(&row).unwrap(), serde_json::json!({"target": "m/body.vmat", "hue": 20.0}));
    }
    #[test]
    fn fine_tuning_names_a_moving_part_type_and_stays_in_range() {
        let check = |tuning: serde_json::Value| {
            let choice: super::Customization = serde_json::from_value(serde_json::json!({"secondary_motion": {"tuning": tuning}})).unwrap();
            choice.validate().is_ok()
        };
        assert!(check(serde_json::json!({"hair": {"stiffness": 0.3, "swing": -1.0}, "butt": {"damping": 1.0}})));
        assert!(!check(serde_json::json!({"rigid": {"stiffness": 0.3}})));
        assert!(!check(serde_json::json!({"wing": {"stiffness": 0.3}})));
        assert!(!check(serde_json::json!({"hair": {"gravity": 1.5}})));
    }
    #[test]
    fn a_made_clip_is_an_animation_row_with_an_animation_file_and_absent_unless_set() {
        let check = |value: serde_json::Value| serde_json::from_value::<super::Customization>(value).map_err(|e| e.to_string()).and_then(|c| c.validate().map_err(|e| e.to_string()));
        let target = "models/heroes/x/clips/idle.vnmclip_c";
        let made = serde_json::json!({"enabled": true, "source": "C:/anim/moves.blend", "target": target, "made": {"action": "Wave", "part_start_seconds": 0.5}});
        assert!(check(serde_json::json!({"animations": [made.clone()]})).is_ok());
        let row: super::ResourceReplacement = serde_json::from_value(made.clone()).unwrap();
        assert_eq!(row.made.as_ref().unwrap().action, "Wave");
        assert_eq!(serde_json::to_value(&row).unwrap()["made"], serde_json::json!({"action": "Wave", "mode": "as_it_looks", "part_start_seconds": 0.5, "bones": "all"}));
        // A row without it saves without the key.
        let plain: super::ResourceReplacement = serde_json::from_value(serde_json::json!({"source": "a.vnmclip_c", "target": target})).unwrap();
        assert!(!serde_json::to_string(&plain).unwrap().contains("made"));
        // Other kinds, other files and bad values are refused.
        let icon = serde_json::json!({"icons": [{"source": "a.blend", "target": "panorama/images/a.vtex_c", "made": {"action": "Wave"}}]});
        assert_eq!(check(icon).unwrap_err(), "Only animations can be made from an animation; icons rows cannot.");
        let item = serde_json::json!({"item_icons": [{"source": "a.png", "target": "panorama/images/upgrades/a.vtex_c", "made": {"action": "Wave"}}]});
        assert!(check(item).unwrap_err().starts_with(crate::made_clip::ONLY_ANIMATIONS));
        for other in ["C:/anim/moves.fbx", "C:/anim/Moves.GLB", "C:/anim/moves.gltf"] {
            assert!(check(serde_json::json!({"animations": [{"source": other, "target": target, "made": {"action": "Take 001"}}]})).is_ok(), "{other}");
        }
        for wrong in ["C:/anim/moves.vnmclip_c", "C:/anim/moves.blend1", "C:/anim/blend", "C:/anim.fbx/moves"] {
            let row = serde_json::json!({"animations": [{"source": wrong, "target": target, "made": {"action": "Wave"}}]});
            assert_eq!(check(row).unwrap_err(), crate::made_clip::NEEDS_ANIMATION_FILE, "{wrong}");
        }
        let late = serde_json::json!({"animations": [{"source": "C:/anim/moves.blend", "target": target, "made": {"action": "Wave", "loop_blend_seconds": 11.0}}]});
        assert!(check(late).is_err());
        assert!(check(serde_json::json!({"animations": [{"source": "a.blend", "target": target, "made": {"action": "Wave", "extra": 1}}]})).is_err());
    }
    #[test]
    fn a_taken_clip_is_an_animation_row_without_a_file_and_absent_unless_set() {
        use crate::taken_clip as taken;
        let check = |value: serde_json::Value| serde_json::from_value::<super::Customization>(value).map_err(|e| e.to_string()).and_then(|c| c.validate().map_err(|e| e.to_string()));
        let target = "models/heroes/x/clips/dash_ground.vnmclip_c";
        let from = serde_json::json!({"hero": "models/heroes/y/y.vmdl", "clip": "models/heroes/y/clips/dash_ground.vnmclip_c"});
        let row = serde_json::json!({"enabled": true, "target": target, "allow_shared": false, "taken": from});
        assert!(check(serde_json::json!({"animations": [row.clone()]})).is_ok());
        let parsed: super::ResourceReplacement = serde_json::from_value(row.clone()).unwrap();
        assert_eq!(parsed.taken.as_ref().unwrap().arms, taken::TakenArms::Source);
        assert_eq!(serde_json::to_value(&parsed).unwrap()["taken"], serde_json::json!({"hero": "models/heroes/y/y.vmdl", "clip": "models/heroes/y/clips/dash_ground.vnmclip_c", "timing": "stretched", "arms": "source"}));
        // Rows without it save without the key, made rows included.
        let plain: super::ResourceReplacement = serde_json::from_value(serde_json::json!({"source": "a.vnmclip_c", "target": target})).unwrap();
        assert!(!serde_json::to_string(&plain).unwrap().contains("taken"));
        let made: super::ResourceReplacement = serde_json::from_value(serde_json::json!({"source": "a.blend", "target": target, "made": {"action": "Wave"}})).unwrap();
        assert!(!serde_json::to_string(&made).unwrap().contains("taken"));
        // Other kinds, a file, made beside it and bad values are refused.
        let icon = serde_json::json!({"icons": [{"target": "panorama/images/a.vtex_c", "taken": from}]});
        assert_eq!(check(icon).unwrap_err(), "Only animations can be taken from another hero; icons rows cannot.");
        let item = serde_json::json!({"item_icons": [{"target": "panorama/images/upgrades/a.vtex_c", "taken": from}]});
        assert!(check(item).unwrap_err().starts_with(taken::ONLY_ANIMATIONS));
        let both = serde_json::json!({"animations": [{"source": "a.blend", "target": target, "made": {"action": "Wave"}, "taken": from}]});
        assert_eq!(check(both).unwrap_err(), taken::NOT_BOTH);
        let file = serde_json::json!({"animations": [{"source": "a.vnmclip_c", "target": target, "taken": from}]});
        assert_eq!(check(file).unwrap_err(), taken::NO_FILE);
        let start = serde_json::json!({"animations": [{"target": target, "taken": {"hero": "models/heroes/y/y.vmdl", "clip": "models/heroes/y/clips/dash_ground.vnmclip_c", "start_seconds": 1.0}}]});
        assert_eq!(check(start).unwrap_err(), taken::BAD_START);
        assert!(check(serde_json::json!({"animations": [{"target": target, "taken": {"hero": "models/heroes/y/y.vmdl", "clip": "c.vnmclip_c", "extra": 1}}]})).is_err());
    }
}
