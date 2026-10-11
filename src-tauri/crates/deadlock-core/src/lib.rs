//! Shared infrastructure. Timings are measured at operation boundaries, never inferred from logs.
#![forbid(unsafe_code)]

pub mod cancellation;
pub mod error;
pub mod process;
pub mod safety;
pub mod text;
pub mod timing;
mod timing_import;

pub use cancellation::CancellationToken;
pub use error::{CoreError, Result};
pub use timing::{
    StepGuard, StepId, StepTiming, TimingEvent, TimingRecorder, TimingReport, TimingStatus,
};
