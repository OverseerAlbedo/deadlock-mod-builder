//! Whether a hero has extreme proportions, read from the hero's compiled
//! skeleton so the notice appears when the hero is picked (before preparation).
//! The thresholds must match `retarget.py` (`LONG_ARM`, `ARM_ASYMMETRY`,
//! `SHORT_LEGS`), which applies the special fit during preparation.
use crate::app::{AppError, Application, Result};
use deadlock_resource::container;
use deadlock_skeleton::{transforms, Skeleton};
use serde::Serialize;

pub const LONG_ARM: f64 = 1.05;
pub const ARM_ASYMMETRY: f64 = 0.10;
pub const SHORT_LEGS: f64 = 1.45;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct HeroProportions {
    /// "arms" (a long or uneven arm) and/or "legs" (short legs).
    pub extremes: Vec<String>,
    /// Upper arm plus forearm, as a share of the pelvis-to-head length.
    pub arm_left: f64,
    pub arm_right: f64,
    /// Thigh plus shin (average of both sides), same unit.
    pub legs: f64,
}

/// Arm and leg lengths of a Deadlock hero skeleton (pelvis, head, arm_upper_L,
/// arm_lower_L, hand_L, leg_upper_L, leg_lower_L, ankle_L and the right side).
pub fn measure(skeleton: &Skeleton) -> Result<Option<HeroProportions>> {
    let state = transforms::bind_state(skeleton).map_err(|e| AppError::validation("Hero skeleton could not be read", e))?;
    let names = skeleton.indices();
    let at = |name: &str| names.get(name).map(|&i| [state.world[i][0][3], state.world[i][1][3], state.world[i][2][3]]);
    let length = |a: &str, b: &str| Some({
        let (a, b) = (at(a)?, at(b)?);
        ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
    });
    let measured = (|| {
        let torso = length("pelvis", "head").filter(|t| *t > 1e-6)?;
        let arm = |s: &str| Some((length(&format!("arm_upper_{s}"), &format!("arm_lower_{s}"))? + length(&format!("arm_lower_{s}"), &format!("hand_{s}"))?) / torso);
        let leg = |s: &str| Some((length(&format!("leg_upper_{s}"), &format!("leg_lower_{s}"))? + length(&format!("leg_lower_{s}"), &format!("ankle_{s}"))?) / torso);
        Some((arm("L")?, arm("R")?, (leg("L")? + leg("R")?) / 2.))
    })();
    let Some((arm_left, arm_right, legs)) = measured else { return Ok(None) };
    let mut extremes = vec![];
    if arm_left.max(arm_right) > LONG_ARM || (arm_left - arm_right).abs() / arm_left.min(arm_right).max(1e-6) > ARM_ASYMMETRY {
        extremes.push("arms".into());
    }
    if legs < SHORT_LEGS {
        extremes.push("legs".into());
    }
    let round = |v: f64| (v * 1000.).round() / 1000.;
    Ok(Some(HeroProportions { extremes, arm_left: round(arm_left), arm_right: round(arm_right), legs: round(legs) }))
}

impl Application {
    /// Proportions of the hero model at `path` (as listed by the hero picker),
    /// read from the configured game archive. None when the skeleton does not
    /// have the usual hero bones.
    pub fn hero_proportions(&self, path: &str) -> Result<Option<HeroProportions>> {
        let settings = self.settings()?;
        if settings.pak.is_empty() {
            return Ok(None);
        }
        let compiled = if path.ends_with("_c") { path.to_owned() } else { format!("{path}_c") };
        let raw = deadlock_vpk::Vpk::open(&settings.pak)
            .and_then(|v| v.read(&compiled))
            .map_err(|e| AppError::validation("Hero model could not be read from the game archive", e))?;
        let data = container::read_blocks(&raw, "DATA").map_err(|e| AppError::validation("Hero model could not be read", e))?;
        let skeleton = data
            .first()
            .and_then(|d| d.root.get("m_modelSkeleton"))
            .ok_or_else(|| AppError::validation("Hero model has no skeleton", "The compiled model has no m_modelSkeleton."))?;
        measure(&Skeleton::from_value(skeleton).map_err(|e| AppError::validation("Hero skeleton could not be read", e))?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    /// A T-posed hero (Z up), pelvis-to-head 0.55, with chosen arm and leg lengths.
    fn hero(upper: f64, fore_l: f64, fore_r: f64, thigh: f64, shin: f64) -> Skeleton {
        let mut bones: Vec<(String, i32, [f64; 3])> = vec![("pelvis".into(), -1, [0., 0., 1.]), ("head".into(), 0, [0., 0., 0.55])];
        for (s, x, fore) in [("L", 1., fore_l), ("R", -1., fore_r)] {
            let base = bones.len() as i32;
            bones.push((format!("arm_upper_{s}"), 0, [0.17 * x, 0., 0.42]));
            bones.push((format!("arm_lower_{s}"), base, [upper * x, 0., 0.]));
            bones.push((format!("hand_{s}"), base + 1, [fore * x, 0., 0.]));
            bones.push((format!("leg_upper_{s}"), 0, [0.1 * x, 0., -0.05]));
            bones.push((format!("leg_lower_{s}"), base + 3, [0., 0., -thigh]));
            bones.push((format!("ankle_{s}"), base + 4, [0., 0., -shin]));
        }
        let names: Vec<String> = bones.iter().map(|b| b.0.clone()).collect();
        let parents: Vec<i32> = bones.iter().map(|b| b.1).collect();
        let positions: Vec<[f64; 3]> = bones.iter().map(|b| b.2).collect();
        let n = names.len();
        Skeleton { names, parents, positions, rotations: vec![[0., 0., 0., 1.]; n], scales: vec![1.; n], flags: vec![0; n] }
    }
    #[test]
    fn long_or_uneven_arms_and_short_legs_are_extreme_like_in_preparation() {
        // Haze-like: arms 0.80, legs 1.73 of the torso.
        let normal = measure(&hero(0.21, 0.23, 0.23, 0.48, 0.47)).unwrap().unwrap();
        assert!(normal.extremes.is_empty(), "{normal:?}");
        // Shiv-like: a long right forearm (1.19 against 0.96).
        let shiv = measure(&hero(0.26, 0.27, 0.395, 0.5, 0.5)).unwrap().unwrap();
        assert_eq!(shiv.extremes, vec!["arms".to_string()]);
        // Wraith-like: legs 1.37.
        let wraith = measure(&hero(0.25, 0.25, 0.25, 0.38, 0.375)).unwrap().unwrap();
        assert_eq!(wraith.extremes, vec!["legs".to_string()]);
        // A skeleton without hero bones gives no notice.
        let mut odd = hero(0.21, 0.23, 0.23, 0.48, 0.47);
        odd.names[0] = "root".into();
        assert!(measure(&odd).unwrap().is_none());
    }
}

#[cfg(test)]
mod game_tests {
    /// Reads real heroes from the game archive in DMB_PAK (run with --ignored).
    #[test]
    #[ignore = "needs the Deadlock game archive"]
    fn real_heroes() {
        let Ok(pak) = std::env::var("DMB_PAK") else { return };
        let vpk = deadlock_vpk::Vpk::open(&pak).unwrap();
        for (path, expected) in [("models/heroes_staging/shiv/shiv.vmdl_c", "arms"), ("models/heroes_wip/wraith/wraith.vmdl_c", "legs"),
                                 ("models/heroes_staging/haze/haze.vmdl_c", ""), ("models/heroes_staging/hornet_v3/hornet.vmdl_c", "")] {
            let raw = vpk.read(path).unwrap();
            let data = deadlock_resource::container::read_blocks(&raw, "DATA").unwrap();
            let skeleton = deadlock_skeleton::Skeleton::from_value(data[0].root.get("m_modelSkeleton").unwrap()).unwrap();
            let p = super::measure(&skeleton).unwrap().unwrap();
            println!("{path}: {p:?}");
            assert_eq!(p.extremes.join(","), expected, "{path}");
        }
    }
}
