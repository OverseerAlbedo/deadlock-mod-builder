use deadlock_core::{
    process::CommandSpec,
    safety::{digest_with_cancel, relative_asset},
    CancellationToken, CoreError, Result,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReaderIdentity {
    pub executable: PathBuf,
    pub version: String,
    pub format: String,
    pub platform: String,
    pub manifest_sha256: String,
    pub executable_sha256: String,
}
#[derive(Deserialize)]
struct Manifest {
    version: String,
    format: String,
    platforms: BTreeMap<String, Platform>,
}
#[derive(Deserialize)]
struct Platform {
    entrypoint: String,
    files: BTreeMap<String, String>,
}
fn damaged(reason: impl std::fmt::Display) -> CoreError {
    CoreError::Integrity(format!(
        "The bundled full-precision cloth reader is missing or damaged. Extract the complete current source package into a fresh folder and launch it again. {reason}"
    ))
}
pub fn identity(root: &Path, cancel: &CancellationToken) -> Result<ReaderIdentity> {
    let identity = identity_for(root, std::env::consts::OS, std::env::consts::ARCH, cancel)?;
    // These hashes belong to the reviewed binaries shipped with this application.
    // A user-editable manifest alone is not an independent reader trust anchor.
    let executable = match identity.platform.as_str() {
        "linux-x64" => "3562369cf144d7ed8abde22cf21f9fcbb6668eef0dbed3365bddee38ee8920b5",
        "win-x64" => "454e7db9f0a142077813e0b147dbc18a402226ad69633d915b2e8682fbc81694",
        _ => return Err(damaged("unreviewed platform")),
    };
    if identity.manifest_sha256
        != "7dc6e6ee9124aa0aa089de076ab8a097773e6df05977d24779d5e2f8783fcaed"
        || identity.executable_sha256 != executable
    {
        return Err(damaged(
            "the manifest or executable is not the packaged reviewed reader",
        ));
    }
    Ok(identity)
}
/// Verify a supplied local manifest (fixtures/offline inspection only).
/// Production execution must use `identity`, which also enforces packaged trust pins.
pub fn identity_for(
    root: &Path,
    system: &str,
    machine: &str,
    cancel: &CancellationToken,
) -> Result<ReaderIdentity> {
    let rid = match (system, machine.to_ascii_lowercase().as_str()) {
        ("windows" | "Windows", "x86_64" | "amd64") => "win-x64",
        ("linux" | "Linux", "x86_64" | "amd64") => "linux-x64",
        _ => {
            return Err(CoreError::Unsupported(
                "The bundled full-precision cloth reader requires Windows x64 or Linux x64.".into(),
            ));
        }
    };
    crate::acquisition::no_links(root).map_err(damaged)?;
    let manifest_path = root.join("manifest.json");
    crate::acquisition::no_links(&manifest_path).map_err(damaged)?;
    if fs::metadata(&manifest_path).map_err(damaged)?.len() > 1024 * 1024 {
        return Err(damaged("reader manifest exceeds its size limit"));
    }
    let raw = fs::read(&manifest_path).map_err(damaged)?;
    let manifest: Manifest = serde_json::from_slice(&raw).map_err(damaged)?;
    if manifest.format != "round-trip-v1" {
        return Err(damaged("unsupported reader format"));
    }
    let build = manifest
        .platforms
        .get(rid)
        .ok_or_else(|| damaged("platform absent from manifest"))?;
    if build.files.is_empty() || !build.files.contains_key(&build.entrypoint) {
        return Err(damaged("missing reader entry point"));
    }
    for (name, expected) in &build.files {
        let relative = relative_asset(name).map_err(damaged)?;
        let parts: Vec<_> = relative.split('/').collect();
        if parts.len() != 2
            || parts[0] != rid
            || expected.len() != 64
            || !expected.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(damaged("invalid reader file path or digest"));
        }
        let path = root.join(relative);
        crate::acquisition::no_links(&path).map_err(damaged)?;
        if fs::symlink_metadata(&path)
            .map_err(damaged)?
            .file_type()
            .is_symlink()
            || !path.is_file()
            || digest_with_cancel(&path, cancel)? != *expected
        {
            return Err(damaged(format!("missing or changed reader file: {name}")));
        }
    }
    Ok(ReaderIdentity {
        executable: root.join(&build.entrypoint),
        version: manifest.version,
        format: manifest.format,
        platform: rid.into(),
        manifest_sha256: format!("{:x}", Sha256::digest(&raw)),
        executable_sha256: build.files[&build.entrypoint].clone(),
    })
}
impl ReaderIdentity {
    pub fn command(&self, resource: &Path, block: &str) -> Result<CommandSpec> {
        if !matches!(block, "PHYS" | "DATA") {
            return Err(CoreError::Unsupported(
                "Full-precision cloth validation supports PHYS and DATA blocks only.".into(),
            ));
        }
        Ok(CommandSpec::new(&self.executable)
            .arg("-i")
            .arg(resource)
            .arg("-b")
            .arg(block))
    }
}
