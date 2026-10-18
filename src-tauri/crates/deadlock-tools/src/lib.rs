//! Configured local compiler/decompiler tools. Presence and static evidence do not certify native compatibility.
#![forbid(unsafe_code)]
pub mod acquisition;
pub mod cloth_reader;
pub mod compiler;
pub mod toolchain;
pub use deadlock_core::{CoreError, Result};

pub mod pe;

pub mod binary_audit;

pub mod manual_setup;

pub mod detection;

pub mod setup_checks;

pub mod steam_qr;

pub mod validation;
