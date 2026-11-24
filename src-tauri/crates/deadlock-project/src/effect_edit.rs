//! The saved form of Extras → Effects edits (`Customization.effect_edits`):
//! one row per effect tree, holding the edit the page sends. The color maths
//! and the file edit live in `deadlock-components` (`effect_colors`, the
//! `TreeEditExt` trait); this is the plain data and its limits, here so the
//! project file can hold and check it.
use crate::{asset_paths::relative_asset, Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// The most effect rows a project holds.
pub const MAX_EFFECT_EDITS: usize = 256;
/// The most per-color replacements one edit holds.
pub const MAX_COLOR_REPLACEMENTS: usize = 64;
/// The most parts one edit leaves out (an effect has at most this many files).
pub const MAX_EXCLUDED: usize = 256;
/// The longest target or part path.
pub const MAX_PATH: usize = 512;
/// The most parts with settings of their own in one edit.
pub const MAX_PART_SETTINGS: usize = MAX_EXCLUDED;

/// The settings of one part of an effect (Advanced: the part's own
/// controls). Every field is absent when unused; unknown keys are refused.
/// What each writes is decided in `deadlock-components effect_edit`
/// (`PartControls`); a left-out part gets none of it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartSettings {
    /// Not drawn: every renderer of the part's file is switched off.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hidden: bool,
    /// 3D-model `m_flRadiusScale` multiplier (above 0, at most 16).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<f64>,
    /// 3D-model `m_flAlphaScale` multiplier (above 0, at most 16).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opacity: Option<f64>,
    /// A color for a 3D model whose file has none (added as the part's
    /// `m_ConstantColor`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tint: Option<[u8; 3]>,
    /// Light brightness multiplier (0 to `LIGHT_MAX`) on the brightness number
    /// a light renderer's file already holds; 0 dims the light.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub light: Option<f64>,
}
/// The largest light brightness multiplier.
pub const LIGHT_MAX: f64 = 50.;
pub const LIGHT_RANGE: &str = "A part's light brightness must be from 0 to 50.";
impl PartSettings {
    /// True when the settings change nothing (shown, factors of 1, no tint).
    pub fn is_neutral(&self) -> bool {
        !self.hidden && self.size.is_none_or(|v| v == 1.) && self.opacity.is_none_or(|v| v == 1.) && self.tint.is_none() && self.light.is_none_or(|v| v == 1.)
    }
}


/// One color replaced by another wherever the tree uses it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColorReplacement {
    pub from: [u8; 3],
    pub to: [u8; 3],
    /// Also colors near `from` (see `deadlock-components effect_colors::near`);
    /// each keeps its difference from `from`.
    #[serde(default)]
    pub near: bool,
}
fn one() -> f64 {
    1.
}
fn yes() -> bool {
    true
}
/// The edit of one effect tree, as the page sends it. Every field has a
/// default, so `{}` is the identity; unknown keys are refused.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TreeEdit {
    /// Whole-effect hue turn in degrees (-360 to 360).
    #[serde(default)]
    pub hue: f64,
    /// Whole-effect saturation scale (HSV S), 0–4.
    #[serde(default = "one")]
    pub saturation: f64,
    /// Whole-effect brightness scale (HSV V), 0–4.
    #[serde(default = "one")]
    pub brightness: f64,
    /// Parts (compiled `.vpcf_c` paths) that keep the game's colors entirely.
    #[serde(default)]
    pub excluded: Vec<String>,
    /// Parts outside the hero's own folders (shared with other heroes) the
    /// user chose to change too; such parts are left out otherwise. A path in
    /// `excluded` stays left out. Paths that are not such a part of the tree
    /// are ignored.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shared: Vec<String>,
    /// Per-color replacements, checked before the whole-effect shift.
    #[serde(default)]
    pub colors: Vec<ColorReplacement>,
    /// Renderer `m_flRadiusScale` multiplier (above 0, at most 16).
    #[serde(default = "one")]
    pub size: f64,
    /// Renderer `m_flOverbrightFactor` multiplier (the page's Brightness).
    #[serde(default = "one")]
    pub glow: f64,
    /// Renderer `m_flAlphaScale` multiplier.
    #[serde(default = "one")]
    pub opacity: f64,
    /// Settings of single parts (compiled `.vpcf_c` path → settings), matched
    /// case-blind like `excluded`; absent from the JSON when empty. Paths that
    /// are no part of the tree are ignored.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub parts: BTreeMap<String, PartSettings>,
}
impl Default for TreeEdit {
    fn default() -> Self {
        Self { hue: 0., saturation: 1., brightness: 1., excluded: vec![], shared: vec![], colors: vec![], size: 1., glow: 1., opacity: 1., parts: BTreeMap::new() }
    }
}
fn error(text: impl Into<String>) -> Error {
    Error::Validation(text.into())
}
fn effect_path(path: &str, what: &str) -> Result<String> {
    if path.is_empty() || path.len() > MAX_PATH || path.contains('\0') {
        return Err(error(format!("Invalid {what} path.")));
    }
    let path = relative_asset(path)?;
    if !deadlock_core::text::casefold(&path).ends_with(".vpcf_c") {
        return Err(error(format!("An effect {what} must be a compiled .vpcf_c particle system.")));
    }
    Ok(path)
}
impl TreeEdit {
    pub fn validate(&self) -> Result<()> {
        if !self.hue.is_finite() || !(-360.0..=360.0).contains(&self.hue) || ![self.saturation, self.brightness].iter().all(|v| (0.0..=4.0).contains(v)) {
            return Err(error("Effect hue must be from -360 to 360 degrees and saturation and brightness between 0 and 4."));
        }
        if ![self.size, self.glow, self.opacity].iter().all(|v| *v > 0. && *v <= 16.) {
            return Err(error("Effect size, opacity and brightness multipliers must be above 0 and at most 16."));
        }
        if self.colors.len() > MAX_COLOR_REPLACEMENTS {
            return Err(error(format!("An effect edit holds at most {MAX_COLOR_REPLACEMENTS} color replacements.")));
        }
        if self.excluded.len() > MAX_EXCLUDED {
            return Err(error(format!("An effect edit leaves out at most {MAX_EXCLUDED} parts.")));
        }
        for (list, what) in [(&self.excluded, "left out"), (&self.shared, "chosen as a shared part")] {
            if list.len() > MAX_EXCLUDED {
                return Err(error(format!("An effect edit lists at most {MAX_EXCLUDED} parts as {what}.")));
            }
            let mut seen = BTreeSet::new();
            for part in list {
                if !seen.insert(deadlock_core::text::casefold(&effect_path(part, "part")?)) {
                    return Err(error(format!("A part is {what} twice: {part}")));
                }
            }
        }
        if self.parts.len() > MAX_PART_SETTINGS {
            return Err(error(format!("An effect edit holds settings for at most {MAX_PART_SETTINGS} parts.")));
        }
        let mut seen = BTreeSet::new();
        for (part, settings) in &self.parts {
            if !seen.insert(deadlock_core::text::casefold(&effect_path(part, "part")?)) {
                return Err(error(format!("A part has settings twice: {part}")));
            }
            if ![settings.size, settings.opacity].iter().flatten().all(|v| *v > 0. && *v <= 16.) {
                return Err(error("A part's size and opacity multipliers must be above 0 and at most 16."));
            }
            if settings.light.is_some_and(|v| !(0.0..=LIGHT_MAX).contains(&v)) {
                return Err(error(LIGHT_RANGE));
            }
        }
        Ok(())
    }
    /// The settings of `part` (case-blind), if any.
    pub fn part_settings(&self, part: &str) -> Option<&PartSettings> {
        let key = deadlock_core::text::casefold(part);
        self.parts.iter().find(|(p, _)| deadlock_core::text::casefold(p) == key).map(|(_, s)| s)
    }
    /// True when the edit changes nothing.
    pub fn is_identity(&self) -> bool {
        let rest = |e: &Self| Self { hue: 0., parts: BTreeMap::new(), ..e.clone() } == Self::default();
        self == &Self::default() || (self.hue.rem_euclid(360.) == 0. && self.parts.values().all(PartSettings::is_neutral) && rest(self))
    }
}
/// One saved effect edit.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectEditRow {
    /// The tree's top particle system (`.vpcf_c`), its id.
    pub target: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default)]
    pub edit: TreeEdit,
    /// The main color the user chose instead of the detected one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub main: Option<[u8; 3]>,
}
/// Rows: at most `MAX_EFFECT_EDITS`, one per target (case-blind), each edit valid.
pub fn validate_rows(rows: &[EffectEditRow]) -> Result<()> {
    if rows.len() > MAX_EFFECT_EDITS {
        return Err(error(format!("Effect edits support at most {MAX_EFFECT_EDITS} effects.")));
    }
    let mut seen = BTreeSet::new();
    for row in rows {
        let target = effect_path(&row.target, "target")?;
        if !seen.insert(deadlock_core::text::casefold(&target)) {
            return Err(error(format!("Two effect edits change the same effect: {}", row.target)));
        }
        row.edit.validate()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_round_trip_and_keep_their_limits() {
        let json = serde_json::json!([{"target": "particles/abilities/ghost/ghost_tracer.vpcf_c", "edit": {"hue": 115, "excluded": ["particles/abilities/ghost/ghost_tracer_model.vpcf_c"], "colors": [{"from": [1, 2, 3], "to": [4, 5, 6]}], "size": 1.5}, "main": [20, 200, 180]}]);
        let rows: Vec<EffectEditRow> = serde_json::from_value(json).unwrap();
        assert!(rows[0].enabled, "enabled by default");
        assert!(!rows[0].edit.colors[0].near);
        assert_eq!(rows[0].edit.glow, 1.);
        validate_rows(&rows).unwrap();
        let back: Vec<EffectEditRow> = serde_json::from_value(serde_json::to_value(&rows).unwrap()).unwrap();
        assert_eq!(back, rows);
        // `main` is absent when unset.
        let plain = EffectEditRow { target: "particles/a.vpcf_c".into(), enabled: true, edit: TreeEdit::default(), main: None };
        assert!(!serde_json::to_string(&plain).unwrap().contains("main"));
        // Unknown keys are refused, in the row and in the edit.
        assert!(serde_json::from_value::<EffectEditRow>(serde_json::json!({"target": "particles/a.vpcf_c", "speed": 2})).is_err());
        assert!(serde_json::from_value::<EffectEditRow>(serde_json::json!({"target": "particles/a.vpcf_c", "edit": {"speed": 2}})).is_err());
        // One row per target, case-blind; targets and parts are .vpcf_c; ranges.
        let row = |target: &str, edit: TreeEdit| EffectEditRow { target: target.into(), enabled: false, edit, main: None };
        assert!(validate_rows(&[row("particles/a.vpcf_c", TreeEdit::default()), row("PARTICLES/A.vpcf_c", TreeEdit::default())]).is_err());
        assert!(validate_rows(&[row("particles/a.vpcf", TreeEdit::default())]).is_err());
        assert!(validate_rows(&[row("../a.vpcf_c", TreeEdit::default())]).is_err());
        for bad in [
            TreeEdit { hue: 400., ..Default::default() },
            TreeEdit { saturation: 4.5, ..Default::default() },
            TreeEdit { opacity: 0., ..Default::default() },
            TreeEdit { excluded: vec!["particles/b.vmat_c".into()], ..Default::default() },
            TreeEdit { excluded: vec!["particles/b.vpcf_c".into(), "Particles/B.vpcf_c".into()], ..Default::default() },
            TreeEdit { colors: vec![ColorReplacement { from: [0; 3], to: [0; 3], near: false }; MAX_COLOR_REPLACEMENTS + 1], ..Default::default() },
        ] {
            assert!(validate_rows(&[row("particles/a.vpcf_c", bad.clone())]).is_err(), "{bad:?}");
        }
        let many: Vec<EffectEditRow> = (0..=MAX_EFFECT_EDITS).map(|i| row(&format!("particles/a{i}.vpcf_c"), TreeEdit::default())).collect();
        assert!(validate_rows(&many).is_err());
        assert!(TreeEdit { hue: 360., ..Default::default() }.is_identity());
        // Shared parts: absent from the JSON when empty, validated like `excluded`.
        assert!(!serde_json::to_string(&TreeEdit::default()).unwrap().contains("shared"));
        let with: TreeEdit = serde_json::from_value(serde_json::json!({"shared": ["particles/s.vpcf_c"]})).unwrap();
        assert_eq!(with.shared, ["particles/s.vpcf_c"]);
        assert!(with.validate().is_ok());
        assert!(TreeEdit { shared: vec!["particles/s.vpcf_c".into(), "PARTICLES/S.vpcf_c".into()], ..Default::default() }.validate().is_err());
        assert!(TreeEdit { shared: vec!["particles/s.vmat_c".into()], ..Default::default() }.validate().is_err());
        assert!(!TreeEdit { size: 2., ..Default::default() }.is_identity());
    }

    #[test]
    fn part_settings_are_absent_unless_set_and_keep_their_limits() {
        // Absent from the JSON when empty; each field absent when unused.
        assert!(!serde_json::to_string(&TreeEdit::default()).unwrap().contains("parts"));
        let edit: TreeEdit = serde_json::from_value(serde_json::json!({"parts": {
            "particles/a_model.vpcf_c": {"size": 3, "tint": [150, 40, 255]},
            "particles/a_line.vpcf_c": {"hidden": true}
        }}))
        .unwrap();
        edit.validate().unwrap();
        assert_eq!(edit.part_settings("PARTICLES/A_LINE.vpcf_c"), Some(&PartSettings { hidden: true, ..Default::default() }));
        assert_eq!(
            serde_json::to_value(&edit).unwrap()["parts"],
            serde_json::json!({"particles/a_line.vpcf_c": {"hidden": true}, "particles/a_model.vpcf_c": {"size": 3.0, "tint": [150, 40, 255]}})
        );
        // A hidden part alone is a change; neutral settings are not.
        assert!(!edit.is_identity());
        let neutral = TreeEdit { hue: 360., parts: BTreeMap::from([("particles/a.vpcf_c".into(), PartSettings { size: Some(1.), ..Default::default() })]), ..Default::default() };
        assert!(neutral.is_identity());
        // Unknown keys, limits, case-blind repeats and non-particle paths are refused.
        assert!(serde_json::from_value::<TreeEdit>(serde_json::json!({"parts": {"particles/a.vpcf_c": {"speed": 2}}})).is_err());
        assert!(serde_json::from_value::<TreeEdit>(serde_json::json!({"parts": {"particles/a.vpcf_c": {"tint": [1, 2, 300]}}})).is_err());
        let one = |path: &str, settings: PartSettings| TreeEdit { parts: BTreeMap::from([(path.into(), settings)]), ..Default::default() };
        for bad in [
            one("particles/a.vpcf_c", PartSettings { size: Some(0.), ..Default::default() }),
            one("particles/a.vpcf_c", PartSettings { opacity: Some(16.5), ..Default::default() }),
            one("particles/a.vpcf_c", PartSettings { size: Some(f64::NAN), ..Default::default() }),
            one("particles/a.vmat_c", PartSettings { hidden: true, ..Default::default() }),
            TreeEdit {
                parts: BTreeMap::from([("particles/a.vpcf_c".into(), PartSettings::default()), ("PARTICLES/A.vpcf_c".into(), PartSettings::default())]),
                ..Default::default()
            },
            TreeEdit { parts: (0..=MAX_PART_SETTINGS).map(|i| (format!("particles/a{i}.vpcf_c"), PartSettings { hidden: true, ..Default::default() })).collect(), ..Default::default() },
        ] {
            assert!(bad.validate().is_err(), "{bad:?}");
        }
        assert!(one("particles/a.vpcf_c", PartSettings { size: Some(16.), opacity: Some(0.01), ..Default::default() }).validate().is_ok());
    }

    #[test]
    fn light_brightness_is_absent_unless_set_and_keeps_its_range() {
        let one = |light: f64| TreeEdit { parts: BTreeMap::from([("particles/a_light.vpcf_c".into(), PartSettings { light: Some(light), ..Default::default() })]), ..Default::default() };
        // Absent from the JSON unless set; settings without it keep their JSON.
        assert_eq!(serde_json::to_value(PartSettings { size: Some(2.), ..Default::default() }).unwrap(), serde_json::json!({"size": 2.0}));
        assert_eq!(serde_json::to_value(&one(50.)).unwrap()["parts"], serde_json::json!({"particles/a_light.vpcf_c": {"light": 50.0}}));
        // 0 and 50 are allowed (0 dims); outside, or not a number, is refused.
        for ok in [0., 0.5, 50.] {
            one(ok).validate().unwrap();
        }
        for bad in [-0.1, 50.5, f64::NAN, f64::INFINITY] {
            assert_eq!(one(bad).validate().unwrap_err().to_string().contains(LIGHT_RANGE), true, "{bad}");
        }
        // 1 changes nothing; 0 is a change.
        assert!(one(1.).is_identity());
        assert!(!one(0.).is_identity());
        let read: PartSettings = serde_json::from_value(serde_json::json!({"light": 0})).unwrap();
        assert_eq!(read.light, Some(0.));
    }
}
