//! A clip made from an animation: the optional `made` object of an animation
//! row (`customization.animations`). The row's `source` is the animation file
//! (a Blender, FBX or glTF file);
//! this object says which action and how it fills the game's clip. Absent
//! unless set, so every other row saves exactly as before; refused on every
//! other kind of row and with unknown keys.
use crate::{Error, Result};
use serde::{Deserialize, Serialize};

/// The longest action name kept (characters).
pub const MAX_ACTION_CHARS: usize = 256;
/// The latest start of the part that plays (seconds).
pub const MAX_PART_START: f64 = 3600.0;
/// The longest loop blend (seconds).
pub const MAX_LOOP_BLEND: f64 = 10.0;
/// The files a clip is made from (lower case, without the dot): Blender, FBX
/// and glTF (each FBX take or glTF animation is one action).
pub const SOURCE_SUFFIXES: [&str; 4] = ["blend", "fbx", "glb", "gltf"];
/// The refusal of `made` on another kind of row (then "<kind> rows cannot.").
pub const ONLY_ANIMATIONS: &str = "Only animations can be made from an animation;";
/// The refusal of a made clip whose file is not one of `SOURCE_SUFFIXES`.
pub const NEEDS_ANIMATION_FILE: &str = "A clip made from an animation needs a Blender, FBX or glTF file (.blend, .fbx, .glb or .gltf) as its source.";

/// Whether `source` is a file a clip can be made from.
pub fn is_animation_file(source: &str) -> bool {
    let lower = source.trim().to_lowercase();
    lower.rsplit_once('.').is_some_and(|(stem, suffix)| !stem.is_empty() && SOURCE_SUFFIXES.contains(&suffix))
}

/// How the pose is carried over.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClipMode {
    /// Arms, legs and head point the way they do in Blender.
    #[default]
    AsItLooks,
    /// The change from the rig's own rest pose, added to the hero's.
    FromRest,
}
impl ClipMode {
    pub fn name(self) -> &'static str {
        match self {
            Self::AsItLooks => "as_it_looks",
            Self::FromRest => "from_rest",
        }
    }
}
/// Which bones follow the animation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClipBones {
    /// Every matched bone.
    #[default]
    All,
    /// Only the bones the action has keys for.
    Keyed,
}
fn yes() -> bool {
    true
}
fn is_true(v: &bool) -> bool {
    *v
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MadeClip {
    /// The action's name in the file (an FBX take, a glTF animation).
    pub action: String,
    #[serde(default)]
    pub mode: ClipMode,
    /// None: the whole action stretched to the clip's length. A number: the
    /// part of the action from this second, at its own speed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part_start_seconds: Option<f64>,
    /// The last seconds of the clip ease into its first frame.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loop_blend_seconds: Option<f64>,
    /// The straight-line travel across the floor is removed (on by default).
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub in_place: bool,
    #[serde(default)]
    pub bones: ClipBones,
}
fn error(text: impl Into<String>) -> Error {
    Error::Validation(text.into())
}
impl MadeClip {
    pub fn new(action: impl Into<String>) -> Self {
        Self { action: action.into(), mode: ClipMode::AsItLooks, part_start_seconds: None, loop_blend_seconds: None, in_place: true, bones: ClipBones::All }
    }
    /// The saved format's limits.
    pub fn validate(&self) -> Result<()> {
        if self.action.trim().is_empty() || self.action.chars().count() > MAX_ACTION_CHARS || self.action.contains('\0') {
            return Err(error(format!("Choose the animation (action) the clip is made from: a name of 1 to {MAX_ACTION_CHARS} characters.")));
        }
        if let Some(start) = self.part_start_seconds {
            if !start.is_finite() || !(0. ..=MAX_PART_START).contains(&start) {
                return Err(error(format!("The part that plays must start from 0 to {MAX_PART_START} seconds into the animation.")));
            }
        }
        if let Some(blend) = self.loop_blend_seconds {
            if !blend.is_finite() || blend <= 0. || blend > MAX_LOOP_BLEND {
                return Err(error(format!("The loop blend must be longer than 0 and at most {MAX_LOOP_BLEND} seconds.")));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_written_only_when_they_differ() {
        let made: MadeClip = serde_json::from_value(serde_json::json!({"action": "Wave"})).unwrap();
        assert_eq!(made, MadeClip::new("Wave"));
        assert_eq!(serde_json::to_value(&made).unwrap(), serde_json::json!({"action": "Wave", "mode": "as_it_looks", "bones": "all"}));
        let full = serde_json::json!({"action": "Wave", "mode": "from_rest", "part_start_seconds": 0.5, "loop_blend_seconds": 0.25, "in_place": false, "bones": "keyed"});
        let made: MadeClip = serde_json::from_value(full.clone()).unwrap();
        assert_eq!(serde_json::to_value(&made).unwrap(), full);
        made.validate().unwrap();
    }

    #[test]
    fn limits_and_unknown_keys_are_refused() {
        assert!(serde_json::from_value::<MadeClip>(serde_json::json!({"action": "Wave", "speed": 2})).is_err());
        assert!(serde_json::from_value::<MadeClip>(serde_json::json!({"action": "Wave", "mode": "other"})).is_err());
        for bad in [
            serde_json::json!({"action": ""}),
            serde_json::json!({"action": "x".repeat(257)}),
            serde_json::json!({"action": "Wave", "part_start_seconds": -0.1}),
            serde_json::json!({"action": "Wave", "part_start_seconds": 3600.5}),
            serde_json::json!({"action": "Wave", "loop_blend_seconds": 0.0}),
            serde_json::json!({"action": "Wave", "loop_blend_seconds": 10.5}),
        ] {
            assert!(serde_json::from_value::<MadeClip>(bad.clone()).unwrap().validate().is_err(), "{bad}");
        }
        for good in [serde_json::json!({"action": "x".repeat(256), "part_start_seconds": 3600.0, "loop_blend_seconds": 10.0})] {
            serde_json::from_value::<MadeClip>(good).unwrap().validate().unwrap();
        }
    }
}
