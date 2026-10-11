//! Transaction tests use labeled non-playable resources. They qualify archive
//! handling and mandatory gates, not resourcecompiler or game compatibility.
use deadlock_core::{
    process::{Runner, RunnerOptions},
    safety, CancellationToken, CoreError, Result, TimingRecorder,
};
use deadlock_pipeline::{
    package::{PackageService, ResourceValidator},
    workspace::{self, WorkspaceLease},
};
use deadlock_project::{Project, Settings};
use deadlock_vpk::{Hashes, Vpk};
use std::{
    cell::RefCell,
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

const MODEL: &str = "models/heroes/test/test.vmdl_c";
fn write(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}
struct Gate {
    reject: bool,
    current_original: RefCell<Vec<u8>>,
}
impl ResourceValidator for Gate {
    fn validate_frozen(
        &self,
        _: &Project,
        _: &Path,
        _: &Hashes,
        _: &CancellationToken,
        _: &TimingRecorder,
    ) -> Result<()> {
        if self.reject {
            Err(CoreError::Integrity("fixture gate rejects model".into()))
        } else {
            Ok(())
        }
    }
    fn validate_install(
        &self,
        _: &Project,
        _: &Vpk,
        original: &Vpk,
        _: &str,
        _: &CancellationToken,
        _: &TimingRecorder,
    ) -> Result<()> {
        *self.current_original.borrow_mut() = original.read(MODEL).unwrap();
        if self.reject {
            Err(CoreError::Integrity("fixture gate rejects install".into()))
        } else {
            Ok(())
        }
    }
}
fn archive(root: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let data = root.join(format!("data-{name}"));
    write(&data.join(MODEL), bytes);
    let output = root.join(format!("{name}_dir.vpk"));
    deadlock_vpk::write_vpk(&data, &[MODEL.into()], &output, None, 2, &mut || Ok(())).unwrap();
    output
}
fn config(root: &Path) -> (Settings, Project) {
    let mut settings = Settings::default();
    settings.workspace = root.join("workspace").display().to_string();
    settings.deadlock = root.join("game-install").display().to_string();
    let mut project = Project::default();
    project.original_model = MODEL.into();
    (settings, project)
}

#[test]
fn freezes_packages_verifies_and_times_actual_work() {
    let temp = tempfile::tempdir().unwrap();
    let (settings, project) = config(temp.path());
    let compiled = temp.path().join("compiled");
    write(&compiled.join(MODEL), b"FIXTURE NOT PLAYABLE");
    let token = CancellationToken::new();
    let runner = Runner::new(
        temp.path().join("logs"),
        token.clone(),
        RunnerOptions::default(),
    )
    .unwrap();
    let clock = TimingRecorder::new("test-package");
    let gate = Gate {
        reject: false,
        current_original: RefCell::new(vec![]),
    };
    let service = PackageService {
        settings: &settings,
        runner: &runner,
        cancel: &token,
        clock: &clock,
        validator: &gate,
        log: &|_| {},
    };
    let built = service
        .package(
            &project,
            &compiled,
            Hashes::new(),
            BTreeSet::from([MODEL.into()]),
            false,
        )
        .unwrap();
    assert_eq!(
        Vpk::open(&built.path).unwrap().read(MODEL).unwrap(),
        b"FIXTURE NOT PLAYABLE"
    );
    assert_eq!(built.sha256, safety::digest(&built.path).unwrap());
    assert!(built.packaging.archive_md5_verified && built.packaging.staged_sha256_verified);
    let names = clock
        .report()
        .steps
        .iter()
        .map(|s| s.name.clone())
        .collect::<Vec<_>>();
    for expected in [
        "Freeze and verify package snapshot",
        "Validate frozen models and components",
        "Validate all packaged dependencies",
        "Write VPK payload",
        "Verify packaged CRC and content hashes",
        "Verify VPK footer hashes",
        "Publish verified VPK",
        "Clean package snapshot",
    ] {
        assert!(names.iter().any(|n| n == expected), "{expected}");
    }
    assert!(service
        .package(
            &project,
            &compiled,
            Hashes::new(),
            BTreeSet::from([MODEL.into()]),
            false
        )
        .is_err());
}

#[test]
fn rejected_frozen_model_preserves_previous_output_and_cleans_stage() {
    let temp = tempfile::tempdir().unwrap();
    let (settings, project) = config(temp.path());
    let compiled = temp.path().join("compiled");
    write(&compiled.join(MODEL), b"CANDIDATE FIXTURE");
    let root = workspace::root(&settings, &project).unwrap();
    let output = root.join("output/pak18_dir.vpk");
    write(&output, b"PREVIOUS OUTPUT");
    let token = CancellationToken::new();
    let runner = Runner::new(
        temp.path().join("logs"),
        token.clone(),
        RunnerOptions::default(),
    )
    .unwrap();
    let clock = TimingRecorder::new("rejected-package");
    let gate = Gate {
        reject: true,
        current_original: RefCell::new(vec![]),
    };
    let service = PackageService {
        settings: &settings,
        runner: &runner,
        cancel: &token,
        clock: &clock,
        validator: &gate,
        log: &|_| {},
    };
    assert!(service
        .package(
            &project,
            &compiled,
            Hashes::new(),
            BTreeSet::from([MODEL.into()]),
            true
        )
        .is_err());
    assert_eq!(fs::read(output).unwrap(), b"PREVIOUS OUTPUT");
    assert!(!fs::read_dir(root).unwrap().any(|entry| entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with("stage-")));
}

#[test]
fn install_revalidates_current_game_preserves_existing_and_backs_up_authorized_overwrite() {
    let temp = tempfile::tempdir().unwrap();
    let (mut settings, project) = config(temp.path());
    let candidate = archive(temp.path(), "candidate", b"CANDIDATE FIXTURE");
    let stale = archive(temp.path(), "stale", b"STALE GAME");
    settings.pak = stale.display().to_string();
    let actual = archive(temp.path(), "current", b"CURRENT GAME");
    let original = Path::new(&settings.deadlock).join("game/citadel/pak01_dir.vpk");
    write(&original, &fs::read(actual).unwrap());
    let target = Path::new(&settings.deadlock).join("game/citadel/addons/pak18_dir.vpk");
    write(&target, b"PREVIOUS ADDON");
    let token = CancellationToken::new();
    let runner = Runner::new(
        temp.path().join("logs"),
        token.clone(),
        RunnerOptions::default(),
    )
    .unwrap();
    let clock = TimingRecorder::new("install");
    let gate = Gate {
        reject: false,
        current_original: RefCell::new(vec![]),
    };
    let service = PackageService {
        settings: &settings,
        runner: &runner,
        cancel: &token,
        clock: &clock,
        validator: &gate,
        log: &|_| {},
    };
    assert!(service.install(&project, &candidate, false).is_err());
    assert_eq!(fs::read(&target).unwrap(), b"PREVIOUS ADDON");
    assert_eq!(gate.current_original.borrow().as_slice(), b"CURRENT GAME");
    assert_eq!(service.install(&project, &candidate, true).unwrap(), target);
    assert_eq!(
        safety::digest(&target).unwrap(),
        safety::digest(&candidate).unwrap()
    );
    let root = workspace::root(&settings, &project).unwrap();
    let backups = fs::read_dir(root.join("backups"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(backups.len(), 1);
    assert_eq!(fs::read(&backups[0]).unwrap(), b"PREVIOUS ADDON");
    assert!(!fs::read_dir(root).unwrap().any(|e| e
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with("install-package-")));
    let report = clock.report();
    assert!(report
        .steps
        .iter()
        .any(|step| step.name == "Clean installation snapshot"
            && step.status == deadlock_core::TimingStatus::Succeeded));
}

#[test]
fn rejected_install_and_cancellation_never_publish() {
    let temp = tempfile::tempdir().unwrap();
    let (settings, project) = config(temp.path());
    let candidate = archive(temp.path(), "candidate", b"CANDIDATE FIXTURE");
    let actual = archive(temp.path(), "current", b"CURRENT GAME");
    write(
        &Path::new(&settings.deadlock).join("game/citadel/pak01_dir.vpk"),
        &fs::read(actual).unwrap(),
    );
    let token = CancellationToken::new();
    let runner = Runner::new(
        temp.path().join("logs"),
        token.clone(),
        RunnerOptions::default(),
    )
    .unwrap();
    let clock = TimingRecorder::new("install");
    let gate = Gate {
        reject: true,
        current_original: RefCell::new(vec![]),
    };
    let service = PackageService {
        settings: &settings,
        runner: &runner,
        cancel: &token,
        clock: &clock,
        validator: &gate,
        log: &|_| {},
    };
    assert!(service.install(&project, &candidate, true).is_err());
    let target = Path::new(&settings.deadlock).join("game/citadel/addons/pak18_dir.vpk");
    assert!(!target.exists());
    token.cancel();
    assert!(matches!(
        service.install(&project, &candidate, true),
        Err(CoreError::Cancelled)
    ));
    assert!(!target.exists());
}

#[test]
fn workspace_leases_and_install_roots_do_not_alias_protected_content() {
    let temp = tempfile::tempdir().unwrap();
    let (mut settings, project) = config(temp.path());
    settings.workspace = Path::new(&settings.deadlock)
        .join("work")
        .display()
        .to_string();
    assert!(workspace::root(&settings, &project).is_err());
    settings.workspace = temp.path().join("work").display().to_string();
    let root = workspace::root(&settings, &project).unwrap();
    let lease = WorkspaceLease::acquire(&root).unwrap();
    assert!(WorkspaceLease::acquire(&root).is_err());
    drop(lease);
    assert!(WorkspaceLease::acquire(&root).is_ok());
}

#[cfg(unix)]
#[test]
fn install_refuses_addon_symlink_even_with_overwrite() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let (settings, project) = config(temp.path());
    let candidate = archive(temp.path(), "candidate", b"CANDIDATE FIXTURE");
    write(
        &Path::new(&settings.deadlock).join("game/citadel/pak01_dir.vpk"),
        &fs::read(&candidate).unwrap(),
    );
    let outside = temp.path().join("outside");
    fs::create_dir_all(&outside).unwrap();
    symlink(
        &outside,
        Path::new(&settings.deadlock).join("game/citadel/addons"),
    )
    .unwrap();
    let token = CancellationToken::new();
    let runner = Runner::new(
        temp.path().join("logs"),
        token.clone(),
        RunnerOptions::default(),
    )
    .unwrap();
    let clock = TimingRecorder::new("install");
    let gate = Gate {
        reject: false,
        current_original: RefCell::new(vec![]),
    };
    let service = PackageService {
        settings: &settings,
        runner: &runner,
        cancel: &token,
        clock: &clock,
        validator: &gate,
        log: &|_| {},
    };
    assert!(service.install(&project, &candidate, true).is_err());
    assert_eq!(fs::read_dir(outside).unwrap().count(), 0);
}

#[cfg(windows)]
mod windows_install_cleanup {
    use super::*;
    use std::{cell::Cell, os::windows::fs::OpenOptionsExt};

    struct SharingGate {
        snapshot: RefCell<Option<PathBuf>>,
    }
    impl ResourceValidator for SharingGate {
        fn validate_frozen(
            &self,
            _: &Project,
            _: &Path,
            _: &Hashes,
            _: &CancellationToken,
            _: &TimingRecorder,
        ) -> Result<()> {
            panic!("Installation test must not use a frozen-source approval");
        }
        fn validate_install(
            &self,
            _: &Project,
            candidate: &Vpk,
            original: &Vpk,
            _: &str,
            _: &CancellationToken,
            _: &TimingRecorder,
        ) -> Result<()> {
            assert_eq!(candidate.read(MODEL).unwrap(), b"CANDIDATE FIXTURE");
            assert_eq!(original.read(MODEL).unwrap(), b"CURRENT GAME");
            let error = fs::OpenOptions::new()
                .read(true)
                .share_mode(0)
                .open(&candidate.path)
                .unwrap_err();
            assert_eq!(
                error.raw_os_error(),
                Some(32),
                "Snapshot must stay protected during validation"
            );
            *self.snapshot.borrow_mut() = Some(candidate.path.clone());
            Ok(())
        }
    }

    #[test]
    fn verified_install_releases_snapshot_handle_before_cleanup() {
        let temp = tempfile::tempdir().unwrap();
        let (settings, project) = config(temp.path());
        let candidate = archive(temp.path(), "candidate", b"CANDIDATE FIXTURE");
        let current = archive(temp.path(), "current", b"CURRENT GAME");
        write(
            &Path::new(&settings.deadlock).join("game/citadel/pak01_dir.vpk"),
            &fs::read(current).unwrap(),
        );
        let token = CancellationToken::new();
        let runner = Runner::new(
            temp.path().join("logs"),
            token.clone(),
            RunnerOptions::default(),
        )
        .unwrap();
        let clock = TimingRecorder::new("Windows installation cleanup");
        let gate = SharingGate {
            snapshot: RefCell::new(None),
        };
        let released = Cell::new(false);
        let log = |message: &str| {
            if message == "Installed to Deadlock addons" {
                let path = gate.snapshot.borrow();
                let exclusive = fs::OpenOptions::new()
                    .read(true)
                    .share_mode(0)
                    .open(path.as_ref().unwrap())
                    .expect("Builder must close its snapshot reader before cleanup");
                drop(exclusive);
                released.set(true);
            }
        };
        let service = PackageService {
            settings: &settings,
            runner: &runner,
            cancel: &token,
            clock: &clock,
            validator: &gate,
            log: &log,
        };
        let installed = service.install(&project, &candidate, false).unwrap();
        assert!(released.get());
        assert_eq!(
            safety::digest(&installed).unwrap(),
            safety::digest(&candidate).unwrap()
        );
        assert!(!gate
            .snapshot
            .borrow()
            .as_ref()
            .unwrap()
            .parent()
            .unwrap()
            .exists());
        assert!(clock
            .report()
            .steps
            .iter()
            .all(|step| step.status == deadlock_core::TimingStatus::Succeeded));
    }
}
