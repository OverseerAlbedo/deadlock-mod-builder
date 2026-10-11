//! Native transaction orchestration. Resource-specific checks are mandatory at
//! frozen-output and installation boundaries, including install-existing.
#![forbid(unsafe_code)]
pub mod compilation;
pub mod motion;
pub mod secondary_motion;
pub mod package;
pub mod sources;
pub mod storage;
pub mod workspace;
use deadlock_core::{CoreError, Result, TimingRecorder, TimingStatus};

pub(crate) fn invalid(message: impl Into<String>) -> CoreError {
    CoreError::Integrity(message.into())
}
pub(crate) fn vpk_error(error: deadlock_vpk::Error) -> CoreError {
    match error {
        deadlock_vpk::Error::Cancelled => CoreError::Cancelled,
        other => invalid(other.to_string()),
    }
}
pub fn step<T>(
    clock: &TimingRecorder,
    name: &str,
    parent: Option<&str>,
    work: impl FnOnce() -> Result<T>,
) -> Result<T> {
    let guard = clock.start_step(name, parent)?;
    match work() {
        Ok(value) => {
            guard.success()?;
            Ok(value)
        }
        Err(error) => {
            let _ = guard.finish(
                if matches!(error, CoreError::Cancelled) {
                    TimingStatus::Cancelled
                } else {
                    TimingStatus::Failed
                },
                Some(error.to_string()),
            );
            Err(error)
        }
    }
}

pub mod inspect;

pub mod model_compile;

pub mod build;
mod build_bundle;
pub mod bundle;
pub mod cloth_authoring;
pub mod prepare;
