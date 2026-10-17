//! Native source-only cloth analysis and authoring. No publication authority.
#![forbid(unsafe_code)]
use sha2::{Digest, Sha256};
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Core(#[from] deadlock_core::CoreError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Resource(#[from] deadlock_resource::Error),
    #[error(transparent)]
    Skeleton(#[from] deadlock_skeleton::Error),
    #[error(transparent)]
    Dmx(#[from] deadlock_dmx::DmxError),
    #[error(transparent)]
    Model(#[from] deadlock_model::Error),
    #[error(transparent)]
    Project(#[from] deadlock_project::Error),
    #[error(transparent)]
    Cloth(#[from] deadlock_cloth::Error),
}
pub type Result<T> = std::result::Result<T, Error>;
pub(crate) fn invalid(message: impl Into<String>) -> Error {
    Error::Invalid(message.into())
}
pub(crate) fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub mod proxy_source;

pub mod inventory;
pub mod physics;
pub mod surface;

pub mod automatic;

mod surface_geometry;

pub mod motion;
pub mod secondary;
pub mod parts;
pub mod body_colliders;
mod secondary_auto;
mod hair_motion;
pub mod hair_limits;
pub mod long_skirt;
pub mod attachments;
mod fabric_cohesion;
mod skirt_surface;
mod tangtang_fit;
/// The private TangTang data behind the presets, when compiled in.
pub mod tangtang_assets;
