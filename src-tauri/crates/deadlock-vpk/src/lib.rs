//! Streaming, integrity-checked VPK I/O and opaque Source 2 resource envelopes.
//! No resource payload decoding, executable invocation, publication or installation.
#![forbid(unsafe_code)]

mod envelope;
mod reader;
mod snapshot;
mod stable;
mod writer;

pub use envelope::{resource_blocks, resource_id, resource_references, ResourceBlock, ResourceLayout};
pub use reader::{Entry, Model, Vpk};
pub use snapshot::{
    create_snapshot, publish_directory, remove_snapshot, SnapshotOptions, SnapshotReceipt,
};
pub use stable::check_unlinked_path;
pub use writer::{write_vpk, PackageReceipt};

pub type Hashes = std::collections::BTreeMap<String, String>;
pub type Result<T> = std::result::Result<T, Error>;
pub type Check<'a> = dyn FnMut() -> Result<()> + 'a;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Invalid(String),
    #[error("Package input changed: {0}")]
    Unstable(String),
    #[error("Operation cancelled.")]
    Cancelled,
    #[error("{path}: {source}")]
    Io {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
}
impl Error {
    pub(crate) fn invalid(message: impl Into<String>) -> Self {
        Self::Invalid(message.into())
    }
    pub(crate) fn io(path: impl Into<std::path::PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}

pub const MAX_TREE_BYTES: u64 = 128 * 1024 * 1024;
pub const MAX_ARCHIVE_BYTES: u64 = (1 << 31) - 1;
pub const MAX_READ_BYTES: u64 = 512 * 1024 * 1024;
pub(crate) const CHUNK_BYTES: usize = 1024 * 1024;

/// Normalize separators, then reject traversal and Windows-special characters.
/// Retains spelling and Unicode; comparisons use the Python baseline's full casefold.
pub fn relative_asset(value: &str) -> Result<String> {
    let value = value.replace('\\', "/");
    if value.is_empty()
        || value.starts_with('/')
        || value.split('/').any(|s| matches!(s, "" | "." | ".."))
    {
        return Err(Error::invalid(format!(
            "Unsafe game-relative asset path: {value:?}"
        )));
    }
    if value.chars().any(|c| ":\0\r\n*?\"<>|".contains(c)) {
        return Err(Error::invalid(format!("Invalid asset path: {value:?}")));
    }
    Ok(value)
}

pub(crate) fn key(s: &str) -> String {
    deadlock_core::text::casefold(s)
}
pub(crate) fn hex(bytes: impl AsRef<[u8]>) -> String {
    bytes
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
pub(crate) fn u16_at(data: &[u8], offset: usize) -> Result<u16> {
    let end = offset
        .checked_add(2)
        .ok_or_else(|| Error::invalid("Binary offset overflow."))?;
    let value = data
        .get(offset..end)
        .ok_or_else(|| Error::invalid("Truncated binary input."))?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}
pub(crate) fn u32_at(data: &[u8], offset: usize) -> Result<u32> {
    let end = offset
        .checked_add(4)
        .ok_or_else(|| Error::invalid("Binary offset overflow."))?;
    let value = data
        .get(offset..end)
        .ok_or_else(|| Error::invalid("Truncated binary input."))?;
    Ok(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}
