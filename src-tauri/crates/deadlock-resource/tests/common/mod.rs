//! Optional test data is read from the `private-assets/` folder at the
//! repository root, or wherever `DMB_PRIVATE_ASSETS` points
//! (docs/DEVELOPMENT.md). A test that needs it says so on stderr and passes
//! without it, so a plain checkout tests clean.
#![allow(dead_code)]
use std::path::PathBuf;

/// The optional data folder (it may not exist).
pub fn private_assets() -> PathBuf {
    std::env::var_os("DMB_PRIVATE_ASSETS")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../private-assets"))
}

/// A file from the optional data folder, or `None` with a note when the
/// folder is not there.
pub fn private_asset(path: &str) -> Option<Vec<u8>> {
    let file = private_assets().join(path);
    match std::fs::read(&file) {
        Ok(bytes) => Some(bytes),
        Err(_) => {
            eprintln!("skipped: private asset {path} is not checked out ({})", file.display());
            None
        }
    }
}

/// A private text file; see [`private_asset`].
pub fn private_text(path: &str) -> Option<String> {
    private_asset(path).map(|bytes| String::from_utf8(bytes).expect("private asset is UTF-8"))
}

/// The path of a private file when it exists (for code that opens paths
/// itself), or `None` with a note.
pub fn private_path(path: &str) -> Option<PathBuf> {
    let file = private_assets().join(path);
    if file.is_file() {
        Some(file)
    } else {
        eprintln!("skipped: private asset {path} is not checked out ({})", file.display());
        None
    }
}
