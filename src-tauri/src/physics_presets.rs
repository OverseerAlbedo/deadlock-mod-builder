//! Saved physics presets: a named set of Fine-tune movement values per part
//! type (hair, tail, breast ...), kept on this PC in `state/physics-presets.json`
//! and applied to any project. Part types make them work across models; the
//! moving parts' own names never go into a preset.
use crate::app::{AppError, Application, Result};
use deadlock_project::customization::{Customization, MotionTuning, SecondaryMotion};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};

pub const MAX_PRESETS: usize = 50;
pub const MAX_NAME_CHARS: usize = 40;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PhysicsPreset {
    pub name: String,
    pub tuning: BTreeMap<String, MotionTuning>,
}

fn invalid(details: &str) -> AppError {
    AppError::validation("The physics preset could not be saved", details)
}

impl Application {
    fn presets_path(&self) -> PathBuf {
        self.state_directory.join("physics-presets.json")
    }
    pub fn physics_presets(&self) -> Result<Vec<PhysicsPreset>> {
        match std::fs::read(self.presets_path()) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| AppError::validation("Saved physics presets could not be read", e)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
            Err(e) => Err(AppError::io("Saved physics presets could not be read", e)),
        }
    }
    /// Saves (or replaces, by name ignoring case) a preset; returns the list.
    pub fn save_physics_preset(&self, name: &str, tuning: BTreeMap<String, MotionTuning>) -> Result<Vec<PhysicsPreset>> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > MAX_NAME_CHARS || name.chars().any(char::is_control) {
            return Err(invalid(&format!("Give the preset a name of 1 to {MAX_NAME_CHARS} characters.")));
        }
        // The same rules as a project's own fine-tuning.
        let check = Customization { secondary_motion: Some(SecondaryMotion { tuning: Some(tuning.clone()), ..Default::default() }), ..Default::default() };
        check.validate().map_err(|e| invalid(&e.to_string()))?;
        let mut presets = self.physics_presets()?;
        presets.retain(|p| !p.name.eq_ignore_ascii_case(name));
        if presets.len() >= MAX_PRESETS {
            return Err(invalid(&format!("Up to {MAX_PRESETS} presets can be saved. Delete one first.")));
        }
        presets.push(PhysicsPreset { name: name.into(), tuning });
        presets.sort_by_key(|p| p.name.to_lowercase());
        deadlock_core::safety::write_json(&self.presets_path(), &presets)?;
        Ok(presets)
    }
    pub fn delete_physics_preset(&self, name: &str) -> Result<Vec<PhysicsPreset>> {
        let mut presets = self.physics_presets()?;
        presets.retain(|p| p.name != name);
        deadlock_core::safety::write_json(&self.presets_path(), &presets)?;
        Ok(presets)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn presets_are_saved_by_name_and_checked_like_a_project() {
        let temp = tempfile::tempdir().unwrap();
        let app = Application::new(temp.path().join("state")).unwrap();
        assert!(app.physics_presets().unwrap().is_empty());
        let soft = BTreeMap::from([("hair".to_string(), MotionTuning { stiffness: Some(-0.4), ..Default::default() })]);
        let list = app.save_physics_preset(" Soft hair ", soft.clone()).unwrap();
        assert_eq!(list, vec![PhysicsPreset { name: "Soft hair".into(), tuning: soft }]);
        // The same name replaces it; another name adds one, sorted.
        let bouncy = BTreeMap::from([("breast".to_string(), MotionTuning { swing: Some(0.5), ..Default::default() })]);
        app.save_physics_preset("soft HAIR", bouncy.clone()).unwrap();
        let list = app.save_physics_preset("Anime", bouncy).unwrap();
        assert_eq!(list.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["Anime", "soft HAIR"]);
        // Out of range values and unnamed presets are refused.
        let wild = BTreeMap::from([("hair".to_string(), MotionTuning { gravity: Some(3.), ..Default::default() })]);
        assert!(app.save_physics_preset("Wild", wild).is_err());
        assert!(app.save_physics_preset("  ", BTreeMap::new()).is_err());
        let list = app.delete_physics_preset("Anime").unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(app.physics_presets().unwrap(), list);
    }
}
