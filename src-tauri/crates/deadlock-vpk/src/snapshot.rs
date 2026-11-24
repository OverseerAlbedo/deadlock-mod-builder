//! Independent, read-only copies verified against stable compiler-output bytes.
use crate::{
    key, relative_asset,
    stable::{check_unlinked_path, hash_copy, resolve_new, Baseline},
    Check, Error, Hashes, Result,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::path::Path;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct SnapshotOptions {
    pub support: Hashes,
    pub required: BTreeSet<String>,
    pub attempts: usize,
    pub settle_time: Duration,
    pub retry_time: Duration,
}
impl Default for SnapshotOptions {
    fn default() -> Self {
        Self {
            support: Hashes::new(),
            required: BTreeSet::new(),
            attempts: 5,
            settle_time: Duration::from_millis(250),
            retry_time: Duration::from_millis(250),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotReceipt {
    pub status: String,
    pub method: String,
    pub attempts: usize,
    pub retries: Vec<String>,
    pub file_count: usize,
    pub files: Hashes,
    pub read_only: bool,
    pub source_sha256_rechecked: bool,
    pub snapshot_sha256_verified: bool,
}

/// Remove only a caller-owned private snapshot. Reject links instead of traversing.
/// Windows read-only copied files are made writable before removal.
pub fn remove_snapshot(root: &Path) -> Result<()> {
    if !root.try_exists().map_err(|e| Error::io(root, e))? {
        // A broken symlink is not a missing private snapshot.
        if fs::symlink_metadata(root).is_ok() {
            check_unlinked_path(root, false)?;
        }
        return Ok(());
    }
    check_unlinked_path(root, false)?;
    for entry in fs::read_dir(root).map_err(|e| Error::io(root, e))? {
        let entry = entry.map_err(|e| Error::io(root, e))?;
        let path = entry.path();
        check_unlinked_path(&path, false)?;
        if entry.file_type().map_err(|e| Error::io(&path, e))?.is_dir() {
            remove_snapshot(&path)?;
        } else {
            #[cfg(windows)]
            {
                let mut permissions = fs::metadata(&path)
                    .map_err(|e| Error::io(&path, e))?
                    .permissions();
                permissions.set_readonly(false);
                fs::set_permissions(&path, permissions).map_err(|e| Error::io(&path, e))?;
            }
            fs::remove_file(&path).map_err(|e| Error::io(&path, e))?;
        }
    }
    fs::remove_dir(root).map_err(|e| Error::io(root, e))
}

fn pause(duration: Duration, check: &mut Check<'_>) -> Result<()> {
    let start = Instant::now();
    loop {
        check()?;
        let Some(left) = duration.checked_sub(start.elapsed()) else {
            return Ok(());
        };
        if left.is_zero() {
            return Ok(());
        }
        std::thread::sleep(left.min(Duration::from_millis(50)));
    }
}
struct PendingSnapshot(tempfile::TempDir);
impl PendingSnapshot {
    fn path(&self) -> &Path {
        self.0.path()
    }
}
impl Drop for PendingSnapshot {
    fn drop(&mut self) {
        let _ = remove_snapshot(self.path());
    }
}
type Inventory = BTreeMap<String, Baseline>;
/// The compiled files (`…_c`) under `root`, plus the other files `named`
/// (casefolded relative paths a package requires by name, such as the item
/// names text files); any other file that is not compiled is left out.
fn inventory(root: &Path, named: &BTreeSet<String>, check: &mut Check<'_>) -> Result<Inventory> {
    check_unlinked_path(root, false)?;
    let mut found = BTreeMap::new();
    let mut seen = BTreeSet::new();
    let mut folders = vec![root.to_owned()];
    while let Some(folder) = folders.pop() {
        check()?;
        let mut entries = fs::read_dir(&folder)
            .map_err(|e| Error::io(&folder, e))?
            .collect::<std::io::Result<Vec<_>>>()
            .map_err(|e| Error::io(&folder, e))?;
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            check()?;
            let path = entry.path();
            let typ = entry.file_type().map_err(|e| Error::io(&path, e))?;
            if typ.is_dir() || typ.is_symlink() {
                // Directory symlinks cannot disguise trees; conservatively reject
                // even a non-resource link instead of determining its target type.
                check_unlinked_path(&path, false)?;
            }
            if typ.is_dir() {
                folders.push(path);
                continue;
            }
            let filename = entry.file_name();
            let Some(filename) = filename.to_str() else {
                return Err(Error::invalid("Compiled output path is not UTF-8."));
            };
            let compiled = filename.ends_with("_c");
            if !compiled && named.is_empty() {
                continue;
            }
            let rel = path
                .strip_prefix(root)
                .map_err(|_| Error::invalid("Compiled output escapes source root."))?
                .to_str()
                .ok_or_else(|| Error::invalid("Compiled output path is not UTF-8."))?
                .replace('\\', "/");
            if !compiled && !named.contains(&key(&rel)) {
                continue;
            }
            let rel = relative_asset(&rel)?;
            if !seen.insert(key(&rel)) {
                return Err(Error::invalid(format!(
                    "Duplicate/case-colliding package path: {rel}"
                )));
            }
            found.insert(rel, Baseline::capture(&path)?);
        }
    }
    Ok(found)
}
fn same_inventory(before: &Inventory, after: &Inventory, phase: &str) -> Result<()> {
    if !before.keys().eq(after.keys()) {
        return Err(Error::Unstable(format!(
            "Compiled output inventory changed {phase}"
        )));
    }
    for (path, baseline) in before {
        if !baseline.same(&after[path]) {
            return Err(Error::Unstable(format!("{path}: changed {phase}")));
        }
    }
    Ok(())
}
fn attempt(
    source: &Path,
    pending: &Path,
    options: &SnapshotOptions,
    check: &mut Check<'_>,
) -> Result<Hashes> {
    // Required files that are not compiled resources (text files) are taken by name.
    let named: BTreeSet<String> = options.required.iter().filter(|p| !p.ends_with("_c")).map(|p| key(p)).collect();
    let before = inventory(source, &named, check)?;
    pause(options.settle_time, check)?;
    same_inventory(&before, &inventory(source, &named, check)?, "during quiet interval")?;
    let missing: Vec<_> = options
        .required
        .iter()
        .filter(|p| !before.contains_key(*p))
        .collect();
    if !missing.is_empty() {
        return Err(Error::Unstable(format!(
            "Required compiled outputs missing: {missing:?}"
        )));
    }
    let mut hashes = Hashes::new();
    for (rel, baseline) in &before {
        check()?;
        hashes.insert(
            rel.clone(),
            hash_copy(&source.join(rel), baseline, None, check)?,
        );
    }
    let mut selected = Hashes::new();
    for (rel, baseline) in &before {
        check()?;
        if !options.required.contains(rel) && options.support.get(rel) == hashes.get(rel) {
            continue;
        }
        let destination = pending.join(rel);
        let parent = destination
            .parent()
            .ok_or_else(|| Error::invalid("Snapshot file has no parent."))?;
        fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        check_unlinked_path(&destination, true)?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination)
            .map_err(|e| Error::io(&destination, e))?;
        let copied = hash_copy(&source.join(rel), baseline, Some(&mut output), check)?;
        drop(output);
        if hashes.get(rel) != Some(&copied) {
            return Err(Error::Unstable(format!(
                "{rel}: SHA-256 changed while creating snapshot"
            )));
        }
        let copied_baseline = Baseline::capture(&destination)?;
        let staged = hash_copy(&destination, &copied_baseline, None, check)?;
        if staged != copied {
            return Err(Error::Unstable(format!(
                "{rel}: copied snapshot SHA-256 did not match source"
            )));
        }
        selected.insert(rel.clone(), copied);
    }
    if selected.is_empty() {
        return Err(Error::invalid("No compiled files selected for packaging."));
    }
    pause(options.settle_time, check)?;
    same_inventory(&before, &inventory(source, &named, check)?, "after snapshot copy")?;
    for (rel, baseline) in &before {
        check()?;
        let final_hash = hash_copy(&source.join(rel), baseline, None, check)?;
        if hashes.get(rel) != Some(&final_hash) {
            return Err(Error::Unstable(format!(
                "{rel}: SHA-256 changed after snapshot copy"
            )));
        }
    }
    same_inventory(
        &before,
        &inventory(source, &named, check)?,
        "at snapshot completion",
    )?;
    for rel in selected.keys() {
        let file = pending.join(rel);
        let mut permissions = fs::metadata(&file)
            .map_err(|e| Error::io(&file, e))?
            .permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&file, permissions).map_err(|e| Error::io(&file, e))?;
    }
    Ok(selected)
}

/// Retry entire independent-copy attempts; never mix outputs from attempts.
/// The source is inventoried and fully rehashed, including excluded support files.
/// A caller must own the destination's parent. Publication is one no-replace directory rename.
pub fn create_snapshot(
    source: &Path,
    destination: &Path,
    options: &SnapshotOptions,
    check: &mut Check<'_>,
    log: &mut dyn FnMut(&str),
) -> Result<SnapshotReceipt> {
    if options.attempts == 0 {
        return Err(Error::invalid("Snapshot attempts must be positive."));
    }
    for name in options.required.iter().chain(options.support.keys()) {
        relative_asset(name)?;
    }
    check_unlinked_path(source, true)?;
    check_unlinked_path(destination, true)?;
    let source = source.canonicalize().map_err(|e| Error::io(source, e))?;
    let destination = resolve_new(destination)?;
    if !source.is_dir() {
        return Err(Error::invalid("Compiled output folder does not exist."));
    }
    if destination
        .try_exists()
        .map_err(|e| Error::io(&destination, e))?
    {
        return Err(Error::invalid("Package snapshot destination must be new."));
    }
    if destination.starts_with(&source) || source.starts_with(&destination) {
        return Err(Error::invalid(
            "Package snapshot must be separate from the compiled output tree.",
        ));
    }
    let parent = destination
        .parent()
        .ok_or_else(|| Error::invalid("Snapshot destination has no parent."))?;
    fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    check_unlinked_path(parent, false)?;
    let mut retries = Vec::new();
    for index in 1..=options.attempts {
        check()?;
        let name = destination
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| Error::invalid("Snapshot destination is not UTF-8."))?;
        let pending = PendingSnapshot(
            tempfile::Builder::new()
                .prefix(&format!("{name}.copying-"))
                .tempdir_in(parent)
                .map_err(|e| Error::io(parent, e))?,
        );
        let outcome = attempt(&source, pending.path(), options, check);
        match outcome {
            Ok(files) => {
                check()?;
                check_unlinked_path(&destination, true)?;
                if destination
                    .try_exists()
                    .map_err(|e| Error::io(&destination, e))?
                {
                    return Err(Error::invalid(
                        "Package snapshot destination appeared during copying.",
                    ));
                }
                publish_directory(pending.path(), &destination)?;
                log(&format!(
                    "Package snapshot frozen: {} files; SHA-256 verified (attempt {index}/{})",
                    files.len(),
                    options.attempts
                ));
                return Ok(SnapshotReceipt {
                    status: "complete".into(),
                    method: "verified-independent-copies".into(),
                    attempts: index,
                    retries,
                    file_count: files.len(),
                    files,
                    read_only: true,
                    source_sha256_rechecked: true,
                    snapshot_sha256_verified: true,
                });
            }
            Err(error) => {
                // Explicit removal handles readonly files on Windows, then the
                // TempDir guard harmlessly sees a missing directory.
                remove_snapshot(pending.path())?;
                let retryable = matches!(&error, Error::Unstable(_))
                    || matches!(&error, Error::Io { source, .. } if matches!(source.kind(), std::io::ErrorKind::NotFound | std::io::ErrorKind::PermissionDenied) || sharing_violation(source));
                if !retryable {
                    return Err(error);
                }
                retries.push(error.to_string());
                log(&format!(
                    "Package outputs not stable (attempt {index}/{}): {error}",
                    options.attempts
                ));
                if index == options.attempts {
                    return Err(Error::invalid(format!("Package snapshot could not stabilize after {} attempts. No VPK was published. Last change: {error}", options.attempts)));
                }
            }
        }
        pause(options.retry_time, check)?;
    }
    Err(Error::invalid("Snapshot attempt loop exhausted."))
}

/// A compiler or other program still writing an output makes Windows report a
/// sharing or lock violation (ERROR_SHARING_VIOLATION 32, ERROR_LOCK_VIOLATION 33)
/// rather than a permission error. Like a changing file, it is worth another attempt.
fn sharing_violation(error: &std::io::Error) -> bool {
    cfg!(windows) && matches!(error.raw_os_error(), Some(32 | 33))
}

// Do not silently replace even an empty destination directory if it appears
// after the final existence check. Linux/Apple use the OS no-replace primitive;
// Windows uses MoveFileExW without MOVEFILE_REPLACE_EXISTING (std's rename may
// replace an empty directory with POSIX semantics). Unsupported operating
// systems fail closed instead of using POSIX overwrite semantics.
pub fn publish_directory(source: &Path, destination: &Path) -> Result<()> {
    check_unlinked_path(source, false)?;
    check_unlinked_path(destination, true)?;
    if !source.is_dir() {
        return Err(Error::invalid(
            "Directory publication requires a source directory.",
        ));
    }
    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_vendor = "apple",
        target_os = "redox"
    ))]
    {
        rustix::fs::renameat_with(
            rustix::fs::CWD,
            source,
            rustix::fs::CWD,
            destination,
            rustix::fs::RenameFlags::NOREPLACE,
        )
        .map_err(|e| Error::io(destination, e.into()))
    }
    #[cfg(windows)]
    {
        // tempfile's no-clobber persist is MoveFileExW(source, destination, 0):
        // it moves a directory on the same volume and fails if anything already
        // exists at the destination. Cleanup is disabled so a failed attempt
        // never tries to delete the source. Both paths are verbatim because
        // MoveFileExW gets them as given and fails from 260 characters.
        let mut pending =
            tempfile::TempPath::try_from_path(deadlock_core::safety::long_path(source))
                .map_err(|e| Error::io(source, e))?;
        pending.disable_cleanup(true);
        pending
            .persist_noclobber(deadlock_core::safety::long_path(destination))
            .map_err(|e| Error::io(destination, e.error))
    }
    #[cfg(not(any(
        target_os = "linux",
        target_os = "android",
        target_vendor = "apple",
        target_os = "redox",
        windows
    )))]
    {
        let _ = (source, destination);
        Err(Error::invalid("Atomic no-replace snapshot directory publication is unqualified on this operating system."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn publication_does_not_replace_an_existing_empty_directory() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("pending");
        let destination = root.path().join("existing");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("payload"), "new").unwrap();
        fs::create_dir(&destination).unwrap();
        assert!(publish_directory(&source, &destination).is_err());
        assert!(source.join("payload").exists());
        assert!(destination.is_dir());
        assert_eq!(fs::read_dir(destination).unwrap().count(), 0);
    }
    /// A text file is frozen only when the package requires it by name; other
    /// files that are not compiled stay out, as before.
    #[test]
    fn a_required_text_file_is_frozen_and_other_text_files_are_not() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("compiled");
        let names = "resource/localization/sample_names/sample_names_english.txt";
        for (path, bytes) in [("models/a.vmdl_c", "model"), (names, "names"), ("resource/notes.txt", "notes"), ("models/a.vmdl", "source")] {
            fs::create_dir_all(source.join(path).parent().unwrap()).unwrap();
            fs::write(source.join(path), bytes).unwrap();
        }
        let options = |required: &[&str]| SnapshotOptions { required: required.iter().map(|p| p.to_string()).collect(), settle_time: Duration::ZERO, ..Default::default() };
        let mut check = || Ok(());
        let frozen = create_snapshot(&source, &root.path().join("one"), &options(&["models/a.vmdl_c", names]), &mut check, &mut |_| {}).unwrap();
        assert_eq!(frozen.files.keys().collect::<Vec<_>>(), ["models/a.vmdl_c", names]);
        let plain = create_snapshot(&source, &root.path().join("two"), &options(&["models/a.vmdl_c"]), &mut check, &mut |_| {}).unwrap();
        assert_eq!(plain.files.keys().collect::<Vec<_>>(), ["models/a.vmdl_c"], "without a named text file, only compiled files");
    }
    /// Issue #7: MoveFileExW needs verbatim paths from 260 characters on.
    #[cfg(windows)]
    #[test]
    fn publication_moves_a_directory_past_260_characters() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("pending");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("payload"), "new").unwrap();
        let mut destination = root.path().join("snapshots");
        while destination.as_os_str().len() < 300 {
            destination.push("snapshot folder");
        }
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        publish_directory(&source, &destination).unwrap();
        assert_eq!(fs::read(destination.join("payload")).unwrap(), b"new");
        assert!(!source.exists());
        // An existing long destination still refuses and keeps the source.
        fs::create_dir(&source).unwrap();
        fs::write(source.join("payload"), "newer").unwrap();
        assert!(publish_directory(&source, &destination).is_err());
        assert_eq!(fs::read(source.join("payload")).unwrap(), b"newer");
        assert_eq!(fs::read(destination.join("payload")).unwrap(), b"new");
    }
}
