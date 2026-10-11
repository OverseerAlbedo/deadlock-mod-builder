use deadlock_vpk::{create_snapshot, remove_snapshot, write_vpk, Error, SnapshotOptions, Vpk};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

fn options() -> SnapshotOptions {
    SnapshotOptions {
        settle_time: Duration::ZERO,
        retry_time: Duration::ZERO,
        ..Default::default()
    }
}
fn source(root: &Path) -> PathBuf {
    let path = root.join("compiled");
    fs::create_dir(&path).unwrap();
    fs::write(path.join("material.vmat_c"), b"original compiled bytes").unwrap();
    path
}
fn pending(root: &Path) -> Option<PathBuf> {
    fs::read_dir(root)
        .unwrap()
        .filter_map(|e| e.ok())
        .find(|e| e.file_name().to_string_lossy().contains(".copying-"))
        .map(|e| e.path())
}
fn clean(root: &Path, destination: &Path) {
    assert!(!destination.exists());
    assert!(pending(root).is_none());
}

#[test]
fn snapshot_copies_independent_readonly_bytes_and_packages_exactly() {
    let temp = tempfile::tempdir().unwrap();
    let source = source(temp.path());
    let destination = temp.path().join("stage");
    let original = fs::read(source.join("material.vmat_c")).unwrap();
    let receipt = create_snapshot(
        &source,
        &destination,
        &options(),
        &mut || Ok(()),
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(receipt.attempts, 1);
    assert!(
        receipt.snapshot_sha256_verified && receipt.source_sha256_rechecked && receipt.read_only
    );
    let copied = destination.join("material.vmat_c");
    assert!(copied.metadata().unwrap().permissions().readonly());
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        assert_ne!(
            copied.metadata().unwrap().ino(),
            source.join("material.vmat_c").metadata().unwrap().ino()
        );
    }
    fs::write(source.join("material.vmat_c"), b"later compiler output").unwrap();
    let output = temp.path().join("pak18_dir.vpk");
    write_vpk(
        &destination,
        &receipt.files.keys().cloned().collect::<Vec<_>>(),
        &output,
        Some(&receipt.files),
        2,
        &mut || Ok(()),
    )
    .unwrap();
    let archive = Vpk::open(output).unwrap();
    assert_eq!(archive.read("material.vmat_c").unwrap(), original);
    archive
        .verify_files(Some(&receipt.files), &mut || Ok(()))
        .unwrap();
    archive.verify_v2_hashes(&mut || Ok(())).unwrap();
    remove_snapshot(&destination).unwrap();
    assert!(!destination.exists());
}
#[test]
fn required_missing_outputs_fail_after_bounded_retries_and_cleanup() {
    let temp = tempfile::tempdir().unwrap();
    let source = source(temp.path());
    let destination = temp.path().join("stage");
    let mut opts = options();
    opts.attempts = 2;
    opts.required.insert("missing.vmdl_c".into());
    let mut messages = Vec::new();
    let error = create_snapshot(&source, &destination, &opts, &mut || Ok(()), &mut |line| {
        messages.push(line.to_owned())
    })
    .unwrap_err();
    assert!(error.to_string().contains("after 2 attempts"));
    assert!(error.to_string().contains("missing.vmdl_c"));
    assert_eq!(messages.len(), 2);
    clean(temp.path(), &destination);
}
#[test]
fn unchanged_support_is_omitted_unless_required_and_all_sources_hashed() {
    let temp = tempfile::tempdir().unwrap();
    let source = source(temp.path());
    let destination = temp.path().join("stage");
    fs::write(source.join("original.vtex_c"), b"original texture").unwrap();
    fs::write(source.join("ignored.txt"), b"not compiled").unwrap();
    let mut opts = options();
    for name in ["material.vmat_c", "original.vtex_c"] {
        opts.support.insert(
            name.into(),
            format!("{:x}", Sha256::digest(fs::read(source.join(name)).unwrap())),
        );
    }
    opts.required.insert("material.vmat_c".into());
    let receipt =
        create_snapshot(&source, &destination, &opts, &mut || Ok(()), &mut |_| {}).unwrap();
    assert_eq!(
        receipt.files.keys().cloned().collect::<Vec<_>>(),
        ["material.vmat_c"]
    );
    assert!(!destination.join("original.vtex_c").exists());
    remove_snapshot(&destination).unwrap();
}
#[test]
fn invalid_empty_unsupported_snapshot_selections_fail_without_publication() {
    let temp = tempfile::tempdir().unwrap();
    let source = source(temp.path());
    let destination = temp.path().join("stage");
    let mut opts = options();
    opts.attempts = 0;
    assert!(create_snapshot(&source, &destination, &opts, &mut || Ok(()), &mut |_| {}).is_err());
    opts = options();
    opts.required.insert("../escape".into());
    assert!(create_snapshot(&source, &destination, &opts, &mut || Ok(()), &mut |_| {}).is_err());
    fs::remove_file(source.join("material.vmat_c")).unwrap();
    assert!(create_snapshot(
        &source,
        &destination,
        &options(),
        &mut || Ok(()),
        &mut |_| {}
    )
    .unwrap_err()
    .to_string()
    .contains("No compiled"));
    clean(temp.path(), &destination);
}
#[test]
fn existing_snapshot_and_overlapping_roots_are_preserved() {
    let temp = tempfile::tempdir().unwrap();
    let source = source(temp.path());
    let destination = temp.path().join("stage");
    fs::create_dir(&destination).unwrap();
    fs::write(destination.join("keep"), b"original").unwrap();
    assert!(create_snapshot(
        &source,
        &destination,
        &options(),
        &mut || Ok(()),
        &mut |_| {}
    )
    .is_err());
    assert_eq!(fs::read(destination.join("keep")).unwrap(), b"original");
    assert!(create_snapshot(
        &source,
        &source.join("stage"),
        &options(),
        &mut || Ok(()),
        &mut |_| {}
    )
    .is_err());
}
#[test]
fn snapshot_cancellation_after_partial_copy_removes_private_files() {
    let temp = tempfile::tempdir().unwrap();
    let source = source(temp.path());
    let destination = temp.path().join("stage");
    let result = create_snapshot(
        &source,
        &destination,
        &options(),
        &mut || {
            if pending(temp.path()).is_some_and(|p| p.join("material.vmat_c").exists()) {
                Err(Error::Cancelled)
            } else {
                Ok(())
            }
        },
        &mut |_| {},
    );
    assert!(matches!(result, Err(Error::Cancelled)));
    clean(temp.path(), &destination);
}
#[test]
fn cancellation_after_readonly_freeze_still_cleans_snapshot() {
    let temp = tempfile::tempdir().unwrap();
    let source = source(temp.path());
    let destination = temp.path().join("stage");
    let result = create_snapshot(
        &source,
        &destination,
        &options(),
        &mut || {
            if pending(temp.path()).is_some_and(|p| {
                p.join("material.vmat_c")
                    .metadata()
                    .is_ok_and(|m| m.permissions().readonly())
            }) {
                Err(Error::Cancelled)
            } else {
                Ok(())
            }
        },
        &mut |_| {},
    );
    assert!(matches!(result, Err(Error::Cancelled)));
    clean(temp.path(), &destination);
}
#[test]
fn destination_appearing_during_copy_is_preserved() {
    let temp = tempfile::tempdir().unwrap();
    let source = source(temp.path());
    let destination = temp.path().join("stage");
    let result = create_snapshot(
        &source,
        &destination,
        &options(),
        &mut || {
            if !destination.exists()
                && pending(temp.path()).is_some_and(|p| p.join("material.vmat_c").exists())
            {
                fs::create_dir(&destination).unwrap();
                fs::write(destination.join("keep"), "user content").unwrap();
            }
            Ok(())
        },
        &mut |_| {},
    );
    assert!(result.is_err());
    assert_eq!(fs::read(destination.join("keep")).unwrap(), b"user content");
    assert!(pending(temp.path()).is_none());
}
#[test]
fn snapshot_case_collisions_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let source = source(temp.path());
    let destination = temp.path().join("stage");
    fs::write(source.join("Straße.vmat_c"), "one").unwrap();
    fs::write(source.join("STRASSE.vmat_c"), "two").unwrap();
    assert!(create_snapshot(
        &source,
        &destination,
        &options(),
        &mut || Ok(()),
        &mut |_| {}
    )
    .unwrap_err()
    .to_string()
    .contains("case-colliding"));
    clean(temp.path(), &destination);
}

#[cfg(unix)]
mod unix {
    use super::*;
    use std::os::unix::fs::symlink;
    #[test]
    fn mutation_during_copy_discards_whole_attempt_and_retries() {
        let temp = tempfile::tempdir().unwrap();
        let source = source(temp.path());
        let destination = temp.path().join("stage");
        let mut changed = false;
        let receipt = create_snapshot(
            &source,
            &destination,
            &options(),
            &mut || {
                if !changed
                    && pending(temp.path()).is_some_and(|p| p.join("material.vmat_c").exists())
                {
                    fs::write(source.join("material.vmat_c"), "new compiler output").unwrap();
                    changed = true;
                }
                Ok(())
            },
            &mut |_| {},
        )
        .unwrap();
        assert!(changed);
        assert_eq!(receipt.attempts, 2);
        assert_eq!(receipt.retries.len(), 1);
        assert_eq!(
            fs::read(destination.join("material.vmat_c")).unwrap(),
            b"new compiler output"
        );
        remove_snapshot(&destination).unwrap();
        assert!(pending(temp.path()).is_none());
    }
    #[test]
    fn continually_changing_inputs_fail_after_bounded_attempts() {
        let temp = tempfile::tempdir().unwrap();
        let source = source(temp.path());
        let destination = temp.path().join("stage");
        let mut changed_attempts = std::collections::BTreeSet::new();
        let mut opts = options();
        opts.attempts = 3;
        let result = create_snapshot(
            &source,
            &destination,
            &opts,
            &mut || {
                if let Some(p) = pending(temp.path()) {
                    if p.join("material.vmat_c").exists() && changed_attempts.insert(p) {
                        fs::write(
                            source.join("material.vmat_c"),
                            format!("changed output {}", changed_attempts.len()),
                        )
                        .unwrap();
                    }
                }
                Ok(())
            },
            &mut |_| {},
        );
        assert!(result.unwrap_err().to_string().contains("after 3 attempts"));
        assert_eq!(changed_attempts.len(), 3);
        clean(temp.path(), &destination);
    }
    #[test]
    fn late_added_resource_restarts_inventory() {
        let temp = tempfile::tempdir().unwrap();
        let source = source(temp.path());
        let destination = temp.path().join("stage");
        let mut changed = false;
        let receipt = create_snapshot(
            &source,
            &destination,
            &options(),
            &mut || {
                if !changed
                    && pending(temp.path()).is_some_and(|p| {
                        p.join("material.vmat_c")
                            .metadata()
                            .is_ok_and(|m| m.len() > 0)
                    })
                {
                    fs::write(source.join("late.vtex_c"), "late").unwrap();
                    changed = true;
                }
                Ok(())
            },
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(receipt.attempts, 2);
        assert!(receipt.files.contains_key("late.vtex_c"));
        remove_snapshot(&destination).unwrap();
    }
    #[test]
    fn same_size_mutation_with_restored_mtime_is_rejected_by_ctime() {
        let temp = tempfile::tempdir().unwrap();
        let source = source(temp.path());
        let output = temp.path().join("pak18_dir.vpk");
        let file = source.join("material.vmat_c");
        let before = file.metadata().unwrap();
        let mut changed = false;
        let result = write_vpk(
            &source,
            &["material.vmat_c".into()],
            &output,
            None,
            2,
            &mut || {
                if output.exists() && !changed {
                    fs::write(&file, vec![b'X'; before.len() as usize]).unwrap();
                    let times = fs::FileTimes::new().set_modified(before.modified().unwrap());
                    fs::OpenOptions::new()
                        .write(true)
                        .open(&file)
                        .unwrap()
                        .set_times(times)
                        .unwrap();
                    changed = true;
                }
                Ok(())
            },
        );
        assert!(result.unwrap_err().to_string().contains("changed"));
        assert!(!output.exists());
    }
    #[test]
    fn atomic_replacement_is_rejected_and_pending_archive_cleaned() {
        let temp = tempfile::tempdir().unwrap();
        let source = source(temp.path());
        let output = temp.path().join("pak18_dir.vpk");
        let file = source.join("material.vmat_c");
        let mut changed = false;
        let result = write_vpk(
            &source,
            &["material.vmat_c".into()],
            &output,
            None,
            2,
            &mut || {
                if output.exists() && !changed {
                    let replacement = temp.path().join("replacement");
                    fs::write(&replacement, fs::read(&file).unwrap()).unwrap();
                    fs::rename(replacement, &file).unwrap();
                    changed = true;
                }
                Ok(())
            },
        );
        assert!(result.is_err());
        assert!(!output.exists());
    }
    #[test]
    fn symlinks_ancestors_missing_parent_traversal_and_linked_destinations_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let source = source(temp.path());
        let destination = temp.path().join("stage");
        let output = temp.path().join("pak18_dir.vpk");
        let alias = temp.path().join("alias");
        symlink(&source, &alias).unwrap();
        for root in [alias.clone(), temp.path().join("missing/../alias")] {
            assert!(
                create_snapshot(&root, &destination, &options(), &mut || Ok(()), &mut |_| {})
                    .is_err()
            );
            assert!(write_vpk(
                &root,
                &["material.vmat_c".into()],
                &output,
                None,
                2,
                &mut || Ok(())
            )
            .is_err());
        }
        symlink(source.join("material.vmat_c"), source.join("linked.vmat_c")).unwrap();
        assert!(create_snapshot(
            &source,
            &destination,
            &options(),
            &mut || Ok(()),
            &mut |_| {}
        )
        .is_err());
        clean(temp.path(), &destination);
        assert!(write_vpk(
            &source,
            &["linked.vmat_c".into()],
            &output,
            None,
            2,
            &mut || Ok(())
        )
        .is_err());
        let outside = temp.path().join("outside");
        fs::create_dir(&outside).unwrap();
        let linked_parent = temp.path().join("linked-output");
        symlink(&outside, &linked_parent).unwrap();
        assert!(write_vpk(
            &source,
            &["material.vmat_c".into()],
            &linked_parent.join("pak18_dir.vpk"),
            None,
            2,
            &mut || Ok(())
        )
        .is_err());
    }
    #[test]
    fn reader_rejects_post_index_replacement() {
        let temp = tempfile::tempdir().unwrap();
        let source = source(temp.path());
        let output = temp.path().join("pak18_dir.vpk");
        write_vpk(
            &source,
            &["material.vmat_c".into()],
            &output,
            None,
            2,
            &mut || Ok(()),
        )
        .unwrap();
        let archive = Vpk::open(&output).unwrap();
        let replacement = temp.path().join("replacement");
        fs::write(&replacement, fs::read(&output).unwrap()).unwrap();
        fs::rename(&replacement, &output).unwrap();
        assert!(archive.read("material.vmat_c").is_err());
    }
}

#[cfg(windows)]
mod windows {
    use super::*;
    #[test]
    fn extended_drive_roots_and_nested_paths_are_checked_as_complete_paths() {
        let temp = tempfile::tempdir().unwrap();
        let folder = temp.path().join("Input With Spaces");
        fs::create_dir(&folder).unwrap();
        let file = folder.join("model.vmdl_c");
        fs::write(&file, b"path fixture").unwrap();
        let extended = file.canonicalize().unwrap();
        assert!(extended.to_str().unwrap().starts_with(r"\\?\"));
        deadlock_vpk::check_unlinked_path(&file, false).unwrap();
        deadlock_vpk::check_unlinked_path(&extended, false).unwrap();
        let root = extended.ancestors().last().unwrap();
        deadlock_vpk::check_unlinked_path(root, false).unwrap();
        deadlock_vpk::check_unlinked_path(&folder.join("new/output.vpk"), true).unwrap();
        assert!(
            deadlock_vpk::check_unlinked_path(&folder.join("new/../output.vpk"), true).is_err()
        );
        assert!(deadlock_vpk::check_unlinked_path(std::path::Path::new(r"\\?\C:"), true).is_err());
    }
    #[test]
    fn open_reader_denies_writes_deletes_and_atomic_replacement() {
        let temp = tempfile::tempdir().unwrap();
        let source = source(temp.path());
        let output = temp.path().join("pak18_dir.vpk");
        write_vpk(
            &source,
            &["material.vmat_c".into()],
            &output,
            None,
            2,
            &mut || Ok(()),
        )
        .unwrap();
        let archive = Vpk::open(&output).unwrap();
        assert!(fs::write(&output, "overwrite").is_err());
        assert!(fs::remove_file(&output).is_err());
        archive.verify_v2_hashes(&mut || Ok(())).unwrap();
        drop(archive);
        fs::write(&output, "writable after reader closes").unwrap();
    }
    #[test]
    fn preexisting_writer_sharing_violation_is_retried() {
        let temp = tempfile::tempdir().unwrap();
        let source = source(temp.path());
        let destination = temp.path().join("stage");
        let mut writer = Some(
            fs::OpenOptions::new()
                .write(true)
                .open(source.join("material.vmat_c"))
                .unwrap(),
        );
        let receipt = create_snapshot(
            &source,
            &destination,
            &options(),
            &mut || Ok(()),
            &mut |_| {
                writer.take();
            },
        )
        .unwrap();
        assert_eq!(receipt.attempts, 2);
        remove_snapshot(&destination).unwrap();
    }
}
