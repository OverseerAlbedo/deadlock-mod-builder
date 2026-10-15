//! The TangTang presets need the reviewed cloth proxies, which live outside the
//! public repository: a separate private repository cloned to `private-assets/`
//! at the repository root, or wherever `DMB_PRIVATE_ASSETS` points (see
//! docs/DEVELOPMENT.md). When the files are present the crate compiles them in
//! (`cfg(tangtang_assets)`, paths through `DMB_TANGTANG_ASSETS`); otherwise the
//! presets report themselves as not included and everything else builds the
//! same. After cloning the private assets into an existing checkout run
//! `cargo clean -p deadlock-cloth-authoring`: a folder that did not exist at the
//! previous build could not be watched.
use std::path::{Path, PathBuf};

const FILES: [&str; 3] = ["tangtang-r2-groups.json", "tangtang-r2-proxy.dmx", "tangtang-hinged-streamers-proxy.dmx"];

fn main() {
    println!("cargo:rustc-check-cfg=cfg(tangtang_assets)");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=DMB_PRIVATE_ASSETS");
    let root = match std::env::var_os("DMB_PRIVATE_ASSETS") {
        Some(dir) => PathBuf::from(dir),
        None => Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../private-assets"),
    };
    let dir = root.join("cloth-authoring");
    if dir.is_dir() {
        // A directory is scanned recursively. A missing path must not be
        // watched: Cargo would treat it as changed and rebuild every time.
        println!("cargo:rerun-if-changed={}", dir.display());
    }
    if FILES.iter().all(|name| dir.join(name).is_file()) {
        println!("cargo:rustc-env=DMB_TANGTANG_ASSETS={}", dir.display());
        println!("cargo:rustc-cfg=tangtang_assets");
    }
}
