//! Exact, ordered Source 2 KV3 values and bounded resource transformations.
//! Binary serialization is validated independently of UI/JSON representation.
#![forbid(unsafe_code)]

pub mod attachments;
pub mod binary;
pub mod cloth;
pub mod constraints;
pub mod container;
pub mod independent;
pub mod text;
pub mod value;
pub use value::{Annotation, Kind, Value};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Unsupported or malformed KV3: {0}")]
    Invalid(String),
    #[error("{0}")]
    Resource(#[from] deadlock_vpk::Error),
}
pub type Result<T> = std::result::Result<T, Error>;
pub(crate) fn invalid(message: impl Into<String>) -> Error {
    Error::Invalid(message.into())
}
pub const MAX_BLOCK: usize = 128 * 1024 * 1024;
pub const MAX_COUNT: usize = 1_000_000;
pub const MAX_DEPTH: usize = 256;
