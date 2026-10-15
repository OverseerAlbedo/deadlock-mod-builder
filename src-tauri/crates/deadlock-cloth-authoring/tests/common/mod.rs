//! Data captured from real third-party characters (Daki, TangTang) lives
//! outside the public repository: a separate private repository cloned to
//! `private-assets/` at the repository root, or wherever `DMB_PRIVATE_ASSETS`
//! points (docs/DEVELOPMENT.md). A test that needs it says so on stderr and
//! passes without it, so the public checkout tests clean.
#![allow(dead_code)]
use std::path::PathBuf;

/// The private assets folder (it may not exist).
pub fn private_assets() -> PathBuf {
    std::env::var_os("DMB_PRIVATE_ASSETS")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../private-assets"))
}

/// A private file (`fixtures/daki-rig.json`, `cloth-authoring/…`), or `None`
/// with a note when the private assets are not checked out.
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
