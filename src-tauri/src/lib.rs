//! Native application services shared by the desktop commands and integration tests.
#![forbid(unsafe_code)]

pub const APPLICATION_NAME: &str = "Deadlock Mod Builder";
pub mod app;
pub mod catalogs;
pub mod cloth_authoring;
pub mod diagnostics;
pub mod intake;
pub mod preparation;
pub mod setup;
pub mod vpk_slots;
pub mod workflows;

#[cfg(feature = "desktop")]
mod desktop;
#[cfg(feature = "desktop")]
pub use desktop::run;
