//! Native application services shared by the desktop commands and integration tests.
#![forbid(unsafe_code)]

pub const APPLICATION_NAME: &str = "Deadlock Mod Builder";
pub mod app;
pub mod autosave;
pub mod catalogs;
pub mod clips;
pub mod cloth_authoring;
pub mod diagnostics;
pub mod effect_player;
pub mod installed_mods;
pub mod intake;
pub mod items;
pub mod made_clips;
pub mod objects;
pub mod preparation;
pub mod setup;
pub mod storage;
pub mod hero_fit;
pub mod physics_presets;
pub mod privacy;
pub mod setup_test;
pub mod sound_edit;
pub mod taken_clips;
pub mod troubleshooting;
pub mod vpk_slots;
pub mod workflows;

#[cfg(feature = "desktop")]
mod desktop;
#[cfg(feature = "desktop")]
pub use desktop::run;
