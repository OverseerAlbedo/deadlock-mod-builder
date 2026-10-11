use std::{io, path::PathBuf};
use thiserror::Error;

pub type Result<T> = std::result::Result<T, CoreError>;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("Operation cancelled.")]
    Cancelled,
    #[error("Tool timed out after {seconds:.3} seconds: {program}")]
    Timeout { program: String, seconds: f64 },
    #[error("Unsafe game-relative asset path: {0:?}")]
    UnsafePath(String),
    #[error("Path escapes workspace: {0}")]
    PathEscape(PathBuf),
    #[error(
        "Windows native-tool path is too long ({units} UTF-16 code units; limit 259): {path}. Choose a shorter workspace; keep the original asset filename."
    )]
    NativePathTooLong { path: PathBuf, units: usize },
    #[error("{context}: {source}")]
    Io {
        context: String,
        #[source]
        source: io::Error,
    },
    #[error("Invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Invalid timing transition: {0}")]
    Timing(String),
    #[error(
        "Tool output exceeded {limit} bytes. Partial logs are retained; incomplete output was not returned for validation."
    )]
    OutputLimit { limit: u64 },
    #[error("Tool {program} failed (exit code {exit_code}).\n{diagnostics}\nFull stdout: {stdout}\nFull stderr: {stderr}", exit_code = .code.map_or_else(|| "unavailable".to_owned(), |value| value.to_string()))]
    ToolFailed {
        program: String,
        code: Option<i32>,
        stdout: PathBuf,
        stderr: PathBuf,
        diagnostics: String,
    },
    #[error("Integrity check failed: {0}")]
    Integrity(String),
    #[error("Unsupported operation: {0}")]
    Unsupported(String),
}

impl CoreError {
    pub fn io(context: impl Into<String>, source: io::Error) -> Self {
        Self::Io {
            context: context.into(),
            source,
        }
    }
}
