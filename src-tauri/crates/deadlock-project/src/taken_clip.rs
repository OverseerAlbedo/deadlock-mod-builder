//! A clip taken from another hero: the optional `taken` object of an
//! animation row (`customization.animations`). The game's clip of another
//! hero (`clip`, a compiled clip of the hero `hero`) is carried onto this
//! hero's clip (the row's `target`) at build, from the game's files only.
//! Absent unless set, so every other row saves exactly as before; refused on
//! every other kind of row, beside `made`, with a file of its own and with
//! unknown keys.
use crate::{asset_paths::relative_asset, Error, Result};
use serde::{Deserialize, Serialize};

/// The longest hero or clip path kept (characters).
pub const MAX_PATH_CHARS: usize = 512;
/// The latest start of the part that plays at its own speed (seconds).
pub const MAX_START: f64 = 3600.0;
/// The refusal of `taken` on another kind of row (then "<kind> rows cannot.").
pub const ONLY_ANIMATIONS: &str = "Only animations can be taken from another hero;";
/// The refusal of a row that has both `made` and `taken`.
pub const NOT_BOTH: &str = "A clip is either made from your animation or taken from another hero, not both. Choose one way for this clip.";
/// The refusal of a taken clip that also names a file of its own.
pub const NO_FILE: &str = "A clip taken from another hero has no file of its own. Remove the file, or choose A compiled clip file for this clip instead.";
/// The refusal of a hero or clip path outside the saved format.
pub const BAD_SOURCE: &str = "Choose the other hero (its model, .vmdl) and one of its clips (.vnmclip_c) for the clip taken from another hero.";
/// The refusal of a start time with the whole clip stretched, or out of range.
pub const BAD_START: &str = "The part of the other hero's clip that plays must start from 0 to 3600 seconds into it, and only when it plays at its own speed.";

/// How the other hero's clip fills this hero's clip, whose length the game fixes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TakenTiming {
    /// The whole clip, faster or slower, over this hero's clip.
    #[default]
    Stretched,
    /// The part from `start_seconds` (0 when absent) at its own speed.
    OwnSpeed,
}
/// Which bones follow the other hero.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TakenArms {
    /// The whole body follows the other hero.
    #[default]
    Source,
    /// The arms and hands keep this hero's own motion from the game's clip.
    Own,
}
impl TakenTiming {
    pub fn name(self) -> &'static str {
        match self {
            Self::Stretched => "stretched",
            Self::OwnSpeed => "own_speed",
        }
    }
}
impl TakenArms {
    pub fn name(self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Own => "own",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TakenClip {
    /// The other hero: its roster model path (`models/…/<hero>.vmdl`).
    pub hero: String,
    /// The other hero's clip: its compiled game path (`….vnmclip_c`).
    pub clip: String,
    #[serde(default)]
    pub timing: TakenTiming,
    /// With `own_speed`: where the part that plays starts (seconds; absent is 0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_seconds: Option<f64>,
    #[serde(default)]
    pub arms: TakenArms,
}
fn error(text: impl Into<String>) -> Error {
    Error::Validation(text.into())
}
fn path_ok(value: &str, suffix: &str) -> bool {
    let lower = value.trim().to_lowercase();
    !value.contains('\0') && value.chars().count() <= MAX_PATH_CHARS && lower.ends_with(suffix) && lower.len() > suffix.len() && relative_asset(value).is_ok()
}
impl TakenClip {
    /// The whole clip of `hero` stretched, with the whole body following it.
    pub fn new(hero: impl Into<String>, clip: impl Into<String>) -> Self {
        Self { hero: hero.into(), clip: clip.into(), timing: TakenTiming::Stretched, start_seconds: None, arms: TakenArms::Source }
    }
    /// Where the part that plays starts, when it plays at its own speed.
    pub fn part_start(&self) -> Option<f64> {
        (self.timing == TakenTiming::OwnSpeed).then(|| self.start_seconds.unwrap_or(0.))
    }
    /// The saved format's limits.
    pub fn validate(&self) -> Result<()> {
        if !path_ok(&self.hero, ".vmdl") || !path_ok(&self.clip, ".vnmclip_c") {
            return Err(error(BAD_SOURCE));
        }
        if let Some(start) = self.start_seconds {
            if self.timing != TakenTiming::OwnSpeed || !start.is_finite() || !(0. ..=MAX_START).contains(&start) {
                return Err(error(BAD_START));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const HERO: &str = "models/heroes/other/other.vmdl";
    const CLIP: &str = "models/heroes/other/clips/dash_ground.vnmclip_c";

    #[test]
    fn defaults_are_the_whole_clip_and_the_whole_body() {
        let taken: TakenClip = serde_json::from_value(json!({"hero": HERO, "clip": CLIP})).unwrap();
        assert_eq!(taken, TakenClip::new(HERO, CLIP));
        assert_eq!(serde_json::to_value(&taken).unwrap(), json!({"hero": HERO, "clip": CLIP, "timing": "stretched", "arms": "source"}));
        assert_eq!(taken.part_start(), None);
        let full = json!({"hero": HERO, "clip": CLIP, "timing": "own_speed", "start_seconds": 0.25, "arms": "own"});
        let taken: TakenClip = serde_json::from_value(full.clone()).unwrap();
        assert_eq!(serde_json::to_value(&taken).unwrap(), full);
        assert_eq!(taken.part_start(), Some(0.25));
        taken.validate().unwrap();
        let from_start: TakenClip = serde_json::from_value(json!({"hero": HERO, "clip": CLIP, "timing": "own_speed"})).unwrap();
        assert_eq!(from_start.part_start(), Some(0.));
    }

    #[test]
    fn limits_and_unknown_keys_are_refused() {
        assert!(serde_json::from_value::<TakenClip>(json!({"hero": HERO, "clip": CLIP, "speed": 2})).is_err());
        assert!(serde_json::from_value::<TakenClip>(json!({"hero": HERO, "clip": CLIP, "timing": "other"})).is_err());
        assert!(serde_json::from_value::<TakenClip>(json!({"hero": HERO, "clip": CLIP, "arms": "left"})).is_err());
        assert!(serde_json::from_value::<TakenClip>(json!({"hero": HERO})).is_err());
        for (bad, sentence) in [
            (json!({"hero": "", "clip": CLIP}), BAD_SOURCE),
            (json!({"hero": "models/heroes/other/other.vmdl_c", "clip": CLIP}), BAD_SOURCE),
            (json!({"hero": HERO, "clip": "models/heroes/other/clips/dash_ground.vnmclip"}), BAD_SOURCE),
            (json!({"hero": "../other.vmdl", "clip": CLIP}), BAD_SOURCE),
            (json!({"hero": "C:/other.vmdl", "clip": CLIP}), BAD_SOURCE),
            (json!({"hero": HERO, "clip": format!("models/{}.vnmclip_c", "x".repeat(600))}), BAD_SOURCE),
            (json!({"hero": HERO, "clip": CLIP, "start_seconds": 0.5}), BAD_START),
            (json!({"hero": HERO, "clip": CLIP, "timing": "own_speed", "start_seconds": -0.1}), BAD_START),
            (json!({"hero": HERO, "clip": CLIP, "timing": "own_speed", "start_seconds": 3600.5}), BAD_START),
        ] {
            let error = serde_json::from_value::<TakenClip>(bad.clone()).unwrap().validate().unwrap_err().to_string();
            assert_eq!(error, sentence, "{bad}");
        }
        serde_json::from_value::<TakenClip>(json!({"hero": HERO, "clip": CLIP, "timing": "own_speed", "start_seconds": 3600.0})).unwrap().validate().unwrap();
    }
}
