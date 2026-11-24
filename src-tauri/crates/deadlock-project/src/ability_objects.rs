//! The saved form of ability object replacements (`Customization.ability_objects`):
//! one row per game model an ability draws (a thrown dagger, a projectile shape),
//! with the user's file and how it is fitted. Which models may be replaced, the
//! fit maths and the build live in `deadlock-components` (`ability_objects`,
//! `object_fit`) and `deadlock-pipeline`; this is the plain data and its limits,
//! here so the project file can hold and check it.
//!
//! Loading never depends on the game: a row whose model the game no longer
//! offers is refused by the build (with help), never by load.
use crate::{asset_paths::relative_asset, Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// The most ability object rows a project holds.
pub const MAX_ABILITY_OBJECTS: usize = 32;
/// The smallest and largest extra scale of a fit.
pub const FIT_SCALE: (f64, f64) = (0.05, 20.0);
/// Quarter turns about one axis: 0 to this.
pub const MAX_QUARTER_TURNS: u8 = 3;
/// The longest target or source path.
pub const MAX_PATH: usize = 1024;

/// How the user's object is sized before `scale` and the turns.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FitSize {
    /// Scaled and placed to the game object's own bounds and long axis.
    #[default]
    Match,
    /// The file's own size and origin (metres as game inches, the file's axes).
    Keep,
}
/// Quarter turns (90°) about the game object's X (roll), Y (tip) and Z (turn) axes,
/// applied in that order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Turns {
    #[serde(default)]
    pub roll: u8,
    #[serde(default)]
    pub tip: u8,
    #[serde(default)]
    pub turn: u8,
}
fn one() -> f64 {
    1.
}
fn yes() -> bool {
    true
}
/// The fit of one object. Every field has a default, so `{}` is "match, scale 1, no turns".
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectFit {
    #[serde(default)]
    pub size: FitSize,
    /// An extra uniform scale after the size rule.
    #[serde(default = "one")]
    pub scale: f64,
    #[serde(default)]
    pub turns: Turns,
    /// Stands the object on the game object's base: its lowest point along the
    /// game's up axis (Z) meets the game object's, instead of the centres
    /// meeting. Absent unless set, so a fit without it keeps its bytes.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub on_base: bool,
}
impl Default for ObjectFit {
    fn default() -> Self {
        Self { size: FitSize::Match, scale: 1., turns: Turns::default(), on_base: false }
    }
}
impl ObjectFit {
    pub fn validate(&self) -> Result<()> {
        if !self.scale.is_finite() || self.scale < FIT_SCALE.0 || self.scale > FIT_SCALE.1 {
            return Err(error(format!("An ability object's scale must be from {} to {}.", FIT_SCALE.0, FIT_SCALE.1)));
        }
        if [self.turns.roll, self.turns.tip, self.turns.turn].iter().any(|t| *t > MAX_QUARTER_TURNS) {
            return Err(error(format!("An ability object turns at most {MAX_QUARTER_TURNS} quarter turns about each axis.")));
        }
        Ok(())
    }
}
/// One saved ability object replacement.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AbilityObjectRow {
    /// The game model (`.vmdl`, as the game names it), its id.
    pub target: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    /// The user's file (`.blend`, `.fbx`, `.glb`, `.gltf`); empty until chosen.
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub fit: ObjectFit,
    /// How the user's model follows a game object with several bones; absent
    /// unless set, so rows without it keep their bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub follow: Option<Follow>,
}
/// The three ways the user's model follows the bones of a game object.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FollowWay {
    /// The whole model follows one bone (`bone`, else the bone carrying most of the game's mesh).
    OnePiece,
    /// Each separate object of the file follows one bone (`parts`, else the engine's suggestion).
    ByParts,
    /// The file's armature is named as the game object's bones: its own weights are used.
    SameSkeleton,
}
/// The follow setting of a row.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Follow {
    pub way: FollowWay,
    /// One piece: the bone it follows (absent: the engine's default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bone: Option<String>,
    /// By parts: the file's object name → the bone it follows (a part not named: the suggestion).
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub parts: std::collections::BTreeMap<String, String>,
}
/// The most parts a follow setting names, and the longest part or bone name.
pub const MAX_FOLLOW_PARTS: usize = 256;
pub const MAX_FOLLOW_NAME: usize = 256;
impl Follow {
    pub fn validate(&self) -> Result<()> {
        let bad = |n: &str| n.is_empty() || n.len() > MAX_FOLLOW_NAME || n.contains('\0');
        if self.parts.len() > MAX_FOLLOW_PARTS || self.bone.as_deref().is_some_and(bad) || self.parts.iter().any(|(k, v)| bad(k) || bad(v)) {
            return Err(error("Invalid bone or part name in an ability object's follow setting."));
        }
        match self.way {
            FollowWay::OnePiece if !self.parts.is_empty() => Err(error("In one piece, an ability object follows one bone, not parts.")),
            FollowWay::ByParts if self.bone.is_some() => Err(error("By parts, each part names its own bone.")),
            FollowWay::SameSkeleton if self.bone.is_some() || !self.parts.is_empty() => Err(error("With the same skeleton, your file's own weights are used.")),
            _ => Ok(()),
        }
    }
}
impl AbilityObjectRow {
    /// The compiled game path the build replaces (`….vmdl_c`).
    pub fn compiled_target(&self) -> Result<String> {
        Ok(format!("{}_c", target_path(&self.target)?))
    }
}
fn error(text: impl Into<String>) -> Error {
    Error::Validation(text.into())
}
/// A row's target: a relative `.vmdl` path (not the compiled `_c` form).
pub fn target_path(path: &str) -> Result<String> {
    if path.is_empty() || path.len() > MAX_PATH || path.contains('\0') {
        return Err(error("Invalid ability object path."));
    }
    let path = relative_asset(path)?;
    if !deadlock_core::text::casefold(&path).ends_with(".vmdl") {
        return Err(error("An ability object must be a game model (.vmdl)."));
    }
    Ok(path)
}
/// File types an ability object may come from.
pub const SOURCE_SUFFIXES: [&str; 4] = ["blend", "fbx", "glb", "gltf"];
/// Rows: at most `MAX_ABILITY_OBJECTS`, one per target (case-blind), each fit valid,
/// sources of a supported type.
pub fn validate_rows(rows: &[AbilityObjectRow]) -> Result<()> {
    if rows.len() > MAX_ABILITY_OBJECTS {
        return Err(error(format!("Ability objects support at most {MAX_ABILITY_OBJECTS} replacements.")));
    }
    let mut seen = BTreeSet::new();
    for row in rows {
        let target = target_path(&row.target)?;
        if !seen.insert(deadlock_core::text::casefold(&target)) {
            return Err(error(format!("Two ability object rows replace the same model: {}", row.target)));
        }
        if row.source.len() > MAX_PATH || row.source.contains('\0') {
            return Err(error("Invalid ability object source path."));
        }
        if !row.source.is_empty() {
            let suffix = std::path::Path::new(&row.source).extension().and_then(|s| s.to_str()).unwrap_or_default().to_ascii_lowercase();
            if !SOURCE_SUFFIXES.contains(&suffix.as_str()) {
                return Err(error("An ability object comes from a Blender, FBX, GLB or glTF file."));
            }
        }
        row.fit.validate()?;
        if let Some(follow) = &row.follow {
            follow.validate()?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_round_trip_with_defaults_and_keep_their_limits() {
        let rows: Vec<AbilityObjectRow> = serde_json::from_value(serde_json::json!([
            {"target": "models/abilities/sample_dagger_model.vmdl", "source": "C:/files/shape.fbx"},
            {"target": "models/abilities/sample_orb.vmdl", "enabled": false, "fit": {"size": "keep", "scale": 2.5, "turns": {"tip": 2}}}
        ]))
        .unwrap();
        assert!(rows[0].enabled, "enabled by default");
        assert_eq!(rows[0].fit, ObjectFit::default());
        assert_eq!(rows[1].fit.size, FitSize::Keep);
        assert_eq!(rows[1].fit.turns, Turns { roll: 0, tip: 2, turn: 0 });
        validate_rows(&rows).unwrap();
        let back: Vec<AbilityObjectRow> = serde_json::from_value(serde_json::to_value(&rows).unwrap()).unwrap();
        assert_eq!(back, rows);
        assert_eq!(rows[0].compiled_target().unwrap(), "models/abilities/sample_dagger_model.vmdl_c");
        // Unknown keys are refused in the row, the fit and the turns.
        for bad in [
            serde_json::json!({"target": "models/a.vmdl", "colour": 1}),
            serde_json::json!({"target": "models/a.vmdl", "fit": {"stretch": 2}}),
            serde_json::json!({"target": "models/a.vmdl", "fit": {"turns": {"spin": 1}}}),
            serde_json::json!({"target": "models/a.vmdl", "fit": {"size": "stretch"}}),
        ] {
            assert!(serde_json::from_value::<AbilityObjectRow>(bad.clone()).is_err(), "{bad}");
        }
        let row = |target: &str, fit: ObjectFit| AbilityObjectRow { target: target.into(), enabled: true, source: String::new(), fit, follow: None };
        // One row per target, case-blind; targets are relative .vmdl paths.
        assert!(validate_rows(&[row("models/a.vmdl", ObjectFit::default()), row("MODELS/A.vmdl", ObjectFit::default())]).is_err());
        for target in ["models/a.vmdl_c", "models/a.vmat", "../a.vmdl", "", "C:/models/a.vmdl"] {
            assert!(validate_rows(&[row(target, ObjectFit::default())]).is_err(), "{target}");
        }
        for fit in [
            ObjectFit { scale: 0.01, ..Default::default() },
            ObjectFit { scale: 21., ..Default::default() },
            ObjectFit { scale: f64::NAN, ..Default::default() },
            ObjectFit { turns: Turns { roll: 4, tip: 0, turn: 0 }, ..Default::default() },
        ] {
            assert!(validate_rows(&[row("models/a.vmdl", fit)]).is_err(), "{fit:?}");
        }
        assert!(validate_rows(&[row("models/a.vmdl", ObjectFit { scale: FIT_SCALE.0, turns: Turns { roll: 3, tip: 3, turn: 3 }, ..Default::default() })]).is_ok());
        let with_source = |source: &str| AbilityObjectRow { source: source.into(), ..row("models/a.vmdl", ObjectFit::default()) };
        assert!(validate_rows(&[with_source("C:/files/shape.pmx")]).is_err(), "MMD files are not objects");
        assert!(validate_rows(&[with_source("C:/files/shape.GLB")]).is_ok());
        let many: Vec<_> = (0..=MAX_ABILITY_OBJECTS).map(|i| row(&format!("models/a{i}.vmdl"), ObjectFit::default())).collect();
        assert!(validate_rows(&many).is_err());
    }

    #[test]
    fn how_a_model_follows_is_absent_unless_set_and_checked_per_way() {
        let plain: AbilityObjectRow = serde_json::from_value(serde_json::json!({"target": "models/a.vmdl"})).unwrap();
        assert!(plain.follow.is_none());
        assert!(serde_json::to_value(&plain).unwrap().get("follow").is_none(), "rows without it keep their bytes");
        let parts: AbilityObjectRow = serde_json::from_value(serde_json::json!({"target": "models/a.vmdl", "follow": {"way": "by_parts", "parts": {"bar": "full_bottom"}}})).unwrap();
        assert_eq!(parts.follow.as_ref().unwrap().way, FollowWay::ByParts);
        validate_rows(&[parts.clone()]).unwrap();
        assert_eq!(serde_json::from_value::<AbilityObjectRow>(serde_json::to_value(&parts).unwrap()).unwrap(), parts);
        for bad in [
            serde_json::json!({"target": "models/a.vmdl", "follow": {"way": "by_parts", "spin": 1}}),
            serde_json::json!({"target": "models/a.vmdl", "follow": {"way": "sideways"}}),
        ] {
            assert!(serde_json::from_value::<AbilityObjectRow>(bad.clone()).is_err(), "{bad}");
        }
        let with = |follow: serde_json::Value| -> AbilityObjectRow { serde_json::from_value(serde_json::json!({"target": "models/a.vmdl", "follow": follow})).unwrap() };
        assert!(validate_rows(&[with(serde_json::json!({"way": "one_piece", "bone": "main"}))]).is_ok());
        assert!(validate_rows(&[with(serde_json::json!({"way": "one_piece", "parts": {"a": "b"}}))]).is_err());
        assert!(validate_rows(&[with(serde_json::json!({"way": "by_parts", "bone": "main"}))]).is_err());
        assert!(validate_rows(&[with(serde_json::json!({"way": "same_skeleton", "bone": "main"}))]).is_err());
        assert!(validate_rows(&[with(serde_json::json!({"way": "one_piece", "bone": ""}))]).is_err());
    }

    #[test]
    fn standing_on_the_base_is_absent_unless_set() {
        let plain: ObjectFit = serde_json::from_value(serde_json::json!({})).unwrap();
        assert!(!plain.on_base);
        assert_eq!(serde_json::to_value(plain).unwrap(), serde_json::json!({"size": "match", "scale": 1.0, "turns": {"roll": 0, "tip": 0, "turn": 0}}), "a fit without it keeps its bytes");
        let standing: ObjectFit = serde_json::from_value(serde_json::json!({"on_base": true})).unwrap();
        assert!(standing.on_base);
        assert_eq!(serde_json::to_value(standing).unwrap()["on_base"], serde_json::json!(true));
        assert!(serde_json::from_value::<ObjectFit>(serde_json::json!({"on_base": 1})).is_err());
    }
}
