use crate::{invalid, step, vpk_error, workspace};
use deadlock_core::{
    process::{CommandSpec, ToolExecutor},
    safety, CancellationToken, CoreError, Result, TimingRecorder,
};
use deadlock_project::{Project, Settings};
use deadlock_vpk::{Hashes, SnapshotOptions, SnapshotReceipt, Vpk};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

/// Implementations perform model/motion/component/bundle contracts. There is no
/// default permissive implementation: archive integrity alone cannot authorize
/// publication or installation.
pub trait ResourceValidator {
    fn validate_frozen(
        &self,
        project: &Project,
        root: &Path,
        files: &Hashes,
        cancel: &CancellationToken,
        clock: &TimingRecorder,
    ) -> Result<()>;
    fn validate_install(
        &self,
        project: &Project,
        candidate: &Vpk,
        current_game: &Vpk,
        archive_sha256: &str,
        cancel: &CancellationToken,
        clock: &TimingRecorder,
    ) -> Result<()>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackagingReceipt {
    pub backend: String,
    pub version: u32,
    pub single_file: bool,
    pub file_count: usize,
    pub crc_verified: bool,
    pub staged_sha256_verified: bool,
    pub archive_md5_verified: bool,
    pub executable: Option<PathBuf>,
    pub snapshot: SnapshotReceipt,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishedPackage {
    pub path: PathBuf,
    pub sha256: String,
    pub files: Hashes,
    pub packaging: PackagingReceipt,
}

struct PrivateDirectory<'a>(PathBuf, &'a dyn ToolExecutor);
impl<'a> PrivateDirectory<'a> {
    fn new(root: &Path, prefix: &str, runner: &'a dyn ToolExecutor) -> Self {
        Self(
            root.join(format!("{prefix}-{}", uuid::Uuid::new_v4().simple())),
            runner,
        )
    }
}
impl Drop for PrivateDirectory<'_> {
    fn drop(&mut self) {
        if let Ok(_guard) = self.1.completed_tree_guard() {
            let _ = deadlock_vpk::remove_snapshot(&self.0);
        }
    }
}
struct PendingArchive<'a>(PathBuf, &'a dyn ToolExecutor);
impl Drop for PendingArchive<'_> {
    fn drop(&mut self) {
        let Ok(_guard) = self.1.completed_tree_guard() else {
            return;
        };
        let _ = fs::remove_file(&self.0);
        // External packers can produce numbered siblings. Only remove siblings
        // bearing this transaction's unpredictable private prefix.
        if let (Some(parent), Some(name)) =
            (self.0.parent(), self.0.file_stem().and_then(|s| s.to_str()))
        {
            let prefix = format!("{}_", name.strip_suffix("_dir").unwrap_or(name));
            if let Ok(entries) = fs::read_dir(parent) {
                for entry in entries.flatten() {
                    let name = entry.file_name();
                    let name = name.to_string_lossy();
                    if name.starts_with(&prefix) && name.ends_with(".vpk") {
                        let _ = fs::remove_file(entry.path());
                    }
                }
            }
        }
    }
}

pub struct PackageService<'a> {
    pub settings: &'a Settings,
    pub runner: &'a dyn ToolExecutor,
    pub cancel: &'a CancellationToken,
    pub clock: &'a TimingRecorder,
    pub validator: &'a dyn ResourceValidator,
    pub log: &'a dyn Fn(&str),
}
impl PackageService<'_> {
    pub fn package(
        &self,
        project: &Project,
        compiled: &Path,
        support: Hashes,
        required: BTreeSet<String>,
        overwrite: bool,
    ) -> Result<PublishedPackage> {
        project
            .validate_action()
            .map_err(|e| invalid(e.to_string()))?;
        let root = workspace::root(self.settings, project)?;
        let output = root
            .join("output")
            .join(format!("pak{:02}_dir.vpk", project.vpk_number));
        deadlock_vpk::check_unlinked_path(&output, true).map_err(vpk_error)?;
        if output.exists() && !overwrite {
            return Err(invalid(format!(
                "Confirm replacement of the existing output before building: {}",
                output.display()
            )));
        }
        let stage = PrivateDirectory::new(&root, "stage", self.runner);
        let snapshot = step(
            self.clock,
            "Freeze and verify package snapshot",
            None,
            || {
                let _gate = step(
                    self.clock,
                    "Wait for owned compiler processes",
                    None,
                    || self.runner.quiescent(),
                )?;
                let mut check = || {
                    self.cancel
                        .check()
                        .map_err(|_| deadlock_vpk::Error::Cancelled)
                };
                let mut log = |line: &str| (self.log)(line);
                deadlock_vpk::create_snapshot(
                    compiled,
                    &stage.0,
                    &SnapshotOptions {
                        support,
                        required,
                        ..Default::default()
                    },
                    &mut check,
                    &mut log,
                )
                .map_err(vpk_error)
            },
        )?;
        safety::write_json(&root.join("packaging-snapshot.json"), &snapshot)?;
        step(
            self.clock,
            "Validate frozen models and components",
            None,
            || {
                self.validator.validate_frozen(
                    project,
                    &stage.0,
                    &snapshot.files,
                    self.cancel,
                    self.clock,
                )
            },
        )?;
        step(
            self.clock,
            "Validate all packaged dependencies",
            None,
            || {
                let originals = if self.settings.pak.is_empty() {
                    None
                } else {
                    Some(Vpk::open(&self.settings.pak).map_err(vpk_error)?)
                };
                validate_dependencies(&stage.0, &snapshot.files, originals.as_ref(), self.cancel)
            },
        )?;
        let pending = PendingArchive(
            root.join(format!("pending_{}_dir.vpk", uuid::Uuid::new_v4().simple())),
            self.runner,
        );
        let receipt = self.pack(&stage.0, &snapshot.files, &pending.0, snapshot.clone())?;
        let sha256 = safety::digest_with_cancel(&pending.0, self.cancel)?;
        step(self.clock, "Publish verified VPK", None, || {
            publish_copy(
                &pending.0,
                &output,
                &root.join("backups"),
                self.settings.keep_backups as usize,
                overwrite,
                self.cancel,
            )?;
            if safety::digest_with_cancel(&output, self.cancel)? != sha256 {
                return Err(invalid("Published package hash changed."));
            }
            Ok(())
        })?;
        step(self.clock, "Clean package snapshot", None, || {
            let _guard = self.runner.completed_tree_guard()?;
            deadlock_vpk::remove_snapshot(&stage.0).map_err(vpk_error)
        })?;
        Ok(PublishedPackage {
            path: output,
            sha256,
            files: snapshot.files,
            packaging: receipt,
        })
    }

    pub(crate) fn pack(
        &self,
        stage: &Path,
        files: &Hashes,
        pending: &Path,
        snapshot: SnapshotReceipt,
    ) -> Result<PackagingReceipt> {
        let mut check = || {
            self.cancel
                .check()
                .map_err(|_| deadlock_vpk::Error::Cancelled)
        };
        let executable = match self.settings.vpk_backend.as_str() {
            "builtin" => {
                step(self.clock, "Write VPK payload", None, || {
                    deadlock_vpk::write_vpk(
                        stage,
                        &files.keys().cloned().collect::<Vec<_>>(),
                        pending,
                        Some(files),
                        2,
                        &mut check,
                    )
                    .map_err(vpk_error)
                })?;
                None
            }
            "valve" => {
                let tool = Path::new(&self.settings.vpk_tool);
                if !tool.is_file() {
                    return Err(invalid("Optional Valve packer is selected but vpk.exe is missing. Choose Built-in under Setup → Packaging, or select vpk.exe."));
                }
                let list = pending.with_extension("files.txt");
                let text = files
                    .keys()
                    .map(|r| format!("\"{}\"\n", r.replace('/', "\\")))
                    .collect::<String>();
                safety::atomic_write(&list, text.as_bytes())?;
                let command = CommandSpec::new(tool)
                    .arg("a")
                    .arg(pending)
                    .arg(format!("@{}", list.display()))
                    .current_dir(stage);
                let result = step(self.clock, "Run optional Valve VPK packer", None, || {
                    self.runner.run(&command)
                });
                if let Ok(_guard) = self.runner.completed_tree_guard() {
                    let _ = fs::remove_file(list);
                }
                result?;
                if !pending.is_file() {
                    return Err(invalid(
                        "Optional VPK tool did not create the requested archive.",
                    ));
                }
                Some(tool.to_path_buf())
            }
            _ => {
                return Err(invalid(
                    "Choose Built-in or Optional Valve vpk.exe under Setup → Packaging.",
                ))
            }
        };
        let archive = Vpk::open(pending).map_err(vpk_error)?;
        require_single_file(&archive)?;
        step(
            self.clock,
            "Verify packaged CRC and content hashes",
            None,
            || {
                archive
                    .verify_files(Some(files), &mut check)
                    .map_err(vpk_error)
            },
        )?;
        if archive.version == 2 {
            step(self.clock, "Verify VPK footer hashes", None, || {
                archive.verify_v2_hashes(&mut check).map_err(vpk_error)
            })?;
        }
        Ok(PackagingReceipt {
            backend: self.settings.vpk_backend.clone(),
            version: archive.version,
            single_file: true,
            file_count: files.len(),
            crc_verified: true,
            staged_sha256_verified: true,
            archive_md5_verified: archive.version == 2,
            executable,
            snapshot,
        })
    }

    /// Validation reads exactly the private copy that will be installed, and the
    /// original archive from the CURRENT game installation on every call.
    pub fn install(&self, project: &Project, source: &Path, overwrite: bool) -> Result<PathBuf> {
        project
            .validate_action()
            .map_err(|e| invalid(e.to_string()))?;
        if self.settings.deadlock.is_empty() {
            return Err(invalid(
                "Set the Deadlock installation folder before installing.",
            ));
        }
        let root = workspace::root(self.settings, project)?;
        let private = PrivateDirectory::new(&root, "install-package", self.runner);
        let snapshot = private.0.join("package_dir.vpk");
        step(
            self.clock,
            "Copy private installation snapshot",
            None,
            || safety::atomic_copy(source, &snapshot, self.cancel),
        )?;
        let mut permissions = fs::metadata(&snapshot)
            .map_err(|e| CoreError::io("Inspect installation copy", e))?
            .permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&snapshot, permissions)
            .map_err(|e| CoreError::io("Freeze installation copy", e))?;
        let game = Path::new(&self.settings.deadlock);
        let original = game.join("game/citadel/pak01_dir.vpk");
        if !original.is_file() {
            return Err(invalid(
                "Deadlock folder does not contain game/citadel/pak01_dir.vpk.",
            ));
        }
        let addon = game.join("game/citadel/addons");
        deadlock_vpk::check_unlinked_path(&addon, true).map_err(vpk_error)?;
        fs::create_dir_all(&addon).map_err(|e| CoreError::io("Create addons folder", e))?;
        let target = addon.join(format!("pak{:02}_dir.vpk", project.vpk_number));
        deadlock_vpk::check_unlinked_path(&target, true).map_err(vpk_error)?;
        let archive = Vpk::open(&snapshot).map_err(vpk_error)?;
        require_single_file(&archive)?;
        if project.customization.model_enabled()
            && !archive
                .entries
                .contains_key(&deadlock_core::text::casefold(&format!(
                    "{}_c",
                    project.source_model()
                )))
        {
            return Err(invalid(
                "VPK does not contain this project's replacement model.",
            ));
        }
        let mut check = || {
            self.cancel
                .check()
                .map_err(|_| deadlock_vpk::Error::Cancelled)
        };
        step(
            self.clock,
            "Verify installation archive integrity",
            None,
            || {
                archive.verify_files(None, &mut check).map_err(vpk_error)?;
                if archive.version == 2 {
                    archive.verify_v2_hashes(&mut check).map_err(vpk_error)?;
                }
                Ok(())
            },
        )?;
        let sha = safety::digest_with_cancel(&snapshot, self.cancel)?;
        let originals = Vpk::open(original).map_err(vpk_error)?;
        step(
            self.clock,
            "Validate current-game installation contracts",
            None,
            || {
                self.validator.validate_install(
                    project,
                    &archive,
                    &originals,
                    &sha,
                    self.cancel,
                    self.clock,
                )
            },
        )?;
        step(self.clock, "Atomically copy installed VPK", None, || {
            publish_copy(
                &snapshot,
                &target,
                &root.join("backups"),
                self.settings.keep_backups as usize,
                overwrite,
                self.cancel,
            )
        })?;
        if safety::digest_with_cancel(&target, self.cancel)? != sha {
            return Err(invalid("Installed VPK did not pass copy verification."));
        }
        // Vpk retains a file-identity handle that denies writes and deletion on
        // Windows. Keep it through validation and the verified installation,
        // then release it before changing permissions/removing the snapshot.
        drop(archive);
        drop(originals);
        (self.log)("Installed to Deadlock addons");
        step(self.clock, "Clean installation snapshot", None, || {
            let _guard = self.runner.completed_tree_guard()?;
            deadlock_vpk::remove_snapshot(&private.0).map_err(vpk_error)
        })?;
        Ok(target)
    }
}

fn require_single_file(archive: &Vpk) -> Result<()> {
    if archive.entries.values().any(|e| e.archive != 0x7fff) {
        Err(invalid("Only a verified single-file VPK can be installed."))
    } else {
        Ok(())
    }
}
pub fn validate_dependencies(
    root: &Path,
    files: &Hashes,
    original: Option<&Vpk>,
    cancel: &CancellationToken,
) -> Result<()> {
    let keys = files
        .keys()
        .map(|path| deadlock_core::text::casefold(path))
        .collect::<BTreeSet<_>>();
    for relative in files.keys() {
        cancel.check()?;
        let path = safety::inside(root, relative)?;
        let file = fs::File::open(&path).map_err(|e| CoreError::io("Read compiled resource", e))?;
        if file
            .metadata()
            .map_err(|e| CoreError::io("Inspect compiled resource", e))?
            .len()
            > deadlock_vpk::MAX_READ_BYTES
        {
            return Err(invalid(
                "Compiled resource exceeds bounded validation size.",
            ));
        }
        let mut raw = Vec::new();
        file.take(deadlock_vpk::MAX_READ_BYTES + 1)
            .read_to_end(&mut raw)
            .map_err(|e| CoreError::io("Read compiled resource", e))?;
        if raw.len() as u64 > deadlock_vpk::MAX_READ_BYTES {
            return Err(invalid("Compiled resource grew beyond validation limit."));
        }
        for reference in deadlock_vpk::resource_references(&raw).map_err(vpk_error)? {
            let path = if reference.ends_with("_c") {
                reference
            } else {
                format!("{reference}_c")
            };
            let key = deadlock_core::text::casefold(&path);
            if !keys.contains(&key) && !original.is_some_and(|vpk| vpk.entries.contains_key(&key)) {
                return Err(invalid(format!(
                    "Compiled dependency missing from mod and game: {path}"
                )));
            }
        }
    }
    Ok(())
}

/// An independently hashed copy is committed atomically. Without overwrite
/// authorization, persist_noclobber closes the check/rename race as well.
pub(crate) fn publish_copy(
    source: &Path,
    target: &Path,
    backups: &Path,
    keep_backups: usize,
    overwrite: bool,
    cancel: &CancellationToken,
) -> Result<()> {
    deadlock_vpk::check_unlinked_path(target, true).map_err(vpk_error)?;
    if target
        .file_name()
        .and_then(|v| v.to_str())
        .is_some_and(|v| v.eq_ignore_ascii_case("pak01_dir.vpk"))
    {
        return Err(invalid(
            "pak01_dir.vpk is reserved for the original game; choose pak02–pak99.",
        ));
    }
    let parent = target
        .parent()
        .ok_or_else(|| invalid("Output needs a parent folder."))?;
    fs::create_dir_all(parent).map_err(|e| CoreError::io("Create output folder", e))?;
    let mut pending = tempfile::Builder::new()
        .prefix(".dmb-publish-")
        .tempfile_in(parent)
        .map_err(|e| CoreError::io("Create publication copy", e))?;
    let mut input =
        fs::File::open(source).map_err(|e| CoreError::io("Open verified package", e))?;
    let mut buffer = vec![0; 1024 * 1024];
    loop {
        cancel.check()?;
        let count = input
            .read(&mut buffer)
            .map_err(|e| CoreError::io("Read verified package", e))?;
        if count == 0 {
            break;
        }
        pending
            .write_all(&buffer[..count])
            .map_err(|e| CoreError::io("Write publication copy", e))?;
    }
    pending
        .flush()
        .and_then(|_| pending.as_file().sync_all())
        .map_err(|e| CoreError::io("Sync publication copy", e))?;
    if safety::digest_with_cancel(pending.path(), cancel)?
        != safety::digest_with_cancel(source, cancel)?
    {
        return Err(invalid(
            "Publication copy differs from the validated package.",
        ));
    }
    cancel.check()?;
    deadlock_vpk::check_unlinked_path(target, true).map_err(vpk_error)?;
    if target.exists() {
        if !overwrite {
            return Err(invalid(format!(
                "Confirm overwrite before replacing {}.",
                target.display()
            )));
        }
        let name = target
            .file_name()
            .and_then(|v| v.to_str())
            .ok_or_else(|| invalid("Unsupported output filename."))?;
        let backup = backups.join(format!("{name}.{}.bak", uuid::Uuid::new_v4().simple()));
        safety::atomic_copy(target, &backup, cancel)?;
    }
    cancel.check()?;
    if overwrite {
        pending
            .persist(target)
            .map_err(|e| CoreError::io("Publish verified package", e.error))?;
    } else {
        pending
            .persist_noclobber(target)
            .map_err(|e| CoreError::io("Publish new verified package", e.error))?;
    }
    // Older replaced copies beyond the retention setting are removed only
    // after the new package is in place.
    crate::storage::prune_backups(backups, keep_backups, &|_| {});
    Ok(())
}

#[cfg(test)]
mod cleanup_tests {
    use super::*;
    struct Cleanup(bool);
    impl ToolExecutor for Cleanup {
        fn run(&self, _: &CommandSpec) -> Result<deadlock_core::process::ProcessResult> {
            unreachable!()
        }
        fn completed_tree_guard(&self) -> Result<Box<dyn Send>> {
            if self.0 {
                Ok(Box::new(()))
            } else {
                Err(invalid("packer tree still owns input files"))
            }
        }
    }
    #[test]
    fn unproven_packer_cleanup_retains_private_inputs_and_archive_siblings() {
        for clean in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let runner = Cleanup(clean);
            let stage = PrivateDirectory::new(temp.path(), "stage", &runner);
            fs::create_dir_all(&stage.0).unwrap();
            fs::write(stage.0.join("model.vmdl_c"), b"packer input").unwrap();
            let path = stage.0.clone();
            let pending = PendingArchive(temp.path().join("pending_token_dir.vpk"), &runner);
            fs::write(&pending.0, b"output").unwrap();
            fs::write(temp.path().join("pending_token_000.vpk"), b"chunk").unwrap();
            let output = pending.0.clone();
            drop(pending);
            drop(stage);
            assert_eq!(path.exists(), !clean);
            assert_eq!(output.exists(), !clean);
            assert_eq!(temp.path().join("pending_token_000.vpk").exists(), !clean);
        }
    }
}
