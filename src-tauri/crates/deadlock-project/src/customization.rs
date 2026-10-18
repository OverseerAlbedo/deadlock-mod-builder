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
choices!(ResourceReplacement {
    enabled: bool,
    source: String,
    target: String,
    allow_shared: bool,
    original_sha256: String,
    fit: String,
    volume_percent: u16,
    audio_fit: String
});
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
// brightness: factors from 0 to 2 where 1 keeps the original.
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
    brightness: f64
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
choices!(HeroColorRange {
    enabled: bool,
    hue: f64,
    hue_width: f64,
    saturation_range: [f64; 2],
    brightness_range: [f64; 2],
    softness: f64,
    hue_shift: f64,
    saturation: f64,
    brightness: f64
});
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
    icons: Vec<ResourceReplacement>, animations: Vec<ResourceReplacement>,
    effects: Vec<ResourceReplacement>, audio: Vec<ResourceReplacement>,
    clothing: Clothing, physics: Physics, cloth_setup: ClothSetup, secondary_motion: SecondaryMotion,
    appearance: Appearance, equipment: Equipment, face: Face, face_layers: FaceLayers,
    hero_materials: HeroMaterials
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
    pub fn validate(&self) -> Result<()> {
        if self.schema.is_some_and(|v| v != 1) {
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
            }
        }
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
                let ranges = row.ranges.as_deref().unwrap_or_default();
                if ranges.len() > 8 {
                    return Err(error("A hero color holds at most 8 color ranges."));
                }
                for range in ranges {
                    for (v, low, high, label) in [
                        (range.hue, 0., 360., "Range hue"),
                        (range.hue_width, 0., 180., "Range hue width"),
                        (range.softness, 0., 1., "Range softness"),
                        (range.hue_shift, -180., 180., "Hue shift"),
                        (range.saturation, 0., 2., "Range saturation"),
                        (range.brightness, 0., 2., "Range brightness"),
                    ] {
                        if let Some(v) = v {
                            number(v, low, high, label)?;
                        }
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
}
