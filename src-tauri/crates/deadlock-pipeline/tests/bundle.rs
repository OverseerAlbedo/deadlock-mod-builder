//! Bundle transaction fixtures are intentionally non-playable. Mandatory member
//! callbacks inspect the actual assembled bytes; no native compiler is simulated.
use deadlock_core::{
    process::{Runner, RunnerOptions},
    safety, CancellationToken, CoreError, Result, TimingRecorder,
};
use deadlock_pipeline::{
    bundle::{self, BundleMemberBuilder, BundleService, MemberArtifact, ValidationPhase},
    package::{PackagingReceipt, PublishedPackage},
    workspace,
};
use deadlock_project::{Project, Settings};
use deadlock_vpk::{Hashes, SnapshotReceipt, Vpk};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
const MAIN: &str = "models/heroes_staging/shiv/shiv.vmdl_c";
const ALT: &str = "models/heroes_staging/shiv_ult/shiv_ult.vmdl_c";
const SHARED: &str = "materials/bundle/shared.vmat_c";
fn hash(v: &[u8]) -> String {
    format!("{:x}", Sha256::digest(v))
}
fn write(path: &Path, raw: &[u8]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, raw).unwrap();
}
fn archive(root: &Path, name: &str, files: &BTreeMap<String, Vec<u8>>) -> PathBuf {
    let source = root.join(format!("{name}-files"));
    for (path, raw) in files {
        write(&source.join(path), raw);
    }
    let output = root.join(format!("{name}_dir.vpk"));
    deadlock_vpk::write_vpk(
        &source,
        &files.keys().cloned().collect::<Vec<_>>(),
        &output,
        None,
        2,
        &mut || Ok(()),
    )
    .unwrap();
    output
}
fn resource(reference: &str) -> Vec<u8> {
    let mut rer = vec![];
    rer.extend(8u32.to_le_bytes());
    rer.extend(1u32.to_le_bytes());
    rer.extend(1u64.to_le_bytes());
    rer.extend(8i64.to_le_bytes());
    rer.extend(reference.as_bytes());
    rer.push(0);
    let len = 28 + rer.len();
    let mut out = vec![];
    out.extend((len as u32).to_le_bytes());
    out.extend(12u16.to_le_bytes());
    out.extend(1u16.to_le_bytes());
    out.extend(8u32.to_le_bytes());
    out.extend(1u32.to_le_bytes());
    out.extend(b"RERL");
    out.extend(8u32.to_le_bytes());
    out.extend((rer.len() as u32).to_le_bytes());
    out.extend(rer);
    out
}
struct Fixture {
    temp: tempfile::TempDir,
    settings: Settings,
    parent: Project,
    child: Project,
    child_path: PathBuf,
    original: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let mut settings = Settings::default();
        settings.workspace = root.join("workspace").display().to_string();
        settings.deadlock = root.join("deadlock").display().to_string();
        let originals = BTreeMap::from([
            (MAIN.into(), resource("materials/game/base.vmat")),
            (ALT.into(), b"ALT ORIGINAL NONPLAYABLE".to_vec()),
            (
                "materials/game/base.vmat_c".into(),
                resource("materials/game/base.vtex"),
            ),
            (
                "materials/game/base.vtex_c".into(),
                b"TEXTURE ORIGINAL NONPLAYABLE".to_vec(),
            ),
        ]);
        let original = archive(root, "game-original", &originals);
        settings.pak = original.display().to_string();
        write(
            &root.join("deadlock/game/citadel/pak01_dir.vpk"),
            &fs::read(&original).unwrap(),
        );
        let dmx = root.join("source.dmx");
        write(&dmx, b"FIXTURE SOURCE, NO COMPILER");
        let mut child = Project::default();
        child.name = "alt_fixture".into();
        child.original_model = ALT.into();
        child.dmx = dmx.display().to_string();
        child.compile_model = true;
        child.build_vpk = true;
        child.install = false;
        let child_path = root.join("alternate.dmb.json");
        child.save(&child_path).unwrap();
        let mut parent = Project::default();
        parent.original_model = MAIN.into();
        parent.dmx = dmx.display().to_string();
        parent.compile_model = true;
        parent.build_vpk = true;
        parent.install = false;
        parent.customization=serde_json::from_value(serde_json::json!({"enabled":true,"equipment":{"enabled":true,"variants":[{"model":ALT,"project":child_path.display().to_string()}]}})).unwrap();
        Self {
            temp,
            settings,
            parent,
            child,
            child_path,
            original,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Contract {
    target: String,
    model_sha: String,
    source_project: String,
}
#[derive(Clone, Copy, PartialEq)]
enum Fault {
    None,
    Preflight,
    SharedConflict,
    MemberHash,
    MissingModel,
    Validation,
}
struct Members<'a> {
    root: &'a Path,
    originals: &'a Vpk,
    fault: Fault,
    preflight: Cell<usize>,
    builds: Cell<usize>,
    validated: RefCell<Vec<(String, ValidationPhase)>>,
}
impl<'a> Members<'a> {
    fn new(root: &'a Path, originals: &'a Vpk, fault: Fault) -> Self {
        Self {
            root,
            originals,
            fault,
            preflight: Cell::new(0),
            builds: Cell::new(0),
            validated: RefCell::new(vec![]),
        }
    }
}
impl BundleMemberBuilder for Members<'_> {
    type Contract = Contract;
    fn preflight(&self, p: &Project) -> Result<()> {
        self.preflight.set(self.preflight.get() + 1);
        if self.fault == Fault::Preflight && p.original_model == ALT {
            return Err(CoreError::Integrity("fixture handoff rejection".into()));
        }
        Ok(())
    }
    fn build(&self, p: &mut Project) -> Result<MemberArtifact<Contract>> {
        assert!(!p.install && !bundle::active(p));
        assert_eq!(
            self.preflight.get(),
            2,
            "all handoffs before first compilation"
        );
        self.builds.set(self.builds.get() + 1);
        let target = p.original_model.clone();
        let model = format!("COMPILED NONPLAYABLE FIXTURE {target}").into_bytes();
        let model_sha = hash(&model);
        let material = if self.fault == Fault::SharedConflict && target == ALT {
            b"DIFFERENT SHARED FIXTURE".as_slice()
        } else {
            b"IDENTICAL SHARED FIXTURE".as_slice()
        };
        let shared = if target == ALT {
            "MATERIALS/BUNDLE/SHARED.vmat_c".to_owned()
        } else {
            SHARED.into()
        };
        let inputs = BTreeMap::from([(target.clone(), model), (shared, material.to_vec())]);
        let files = inputs
            .iter()
            .map(|(p, b)| (p.clone(), hash(b)))
            .collect::<Hashes>();
        let path = archive(self.root, &format!("member-{}", p.id), &inputs);
        let sha256 = safety::digest(&path)?;
        let snapshot = SnapshotReceipt {
            status: "fixture".into(),
            method: "nonplayable-test".into(),
            attempts: 1,
            retries: vec![],
            file_count: files.len(),
            files: files.clone(),
            read_only: true,
            source_sha256_rechecked: true,
            snapshot_sha256_verified: true,
        };
        let packaging = PackagingReceipt {
            backend: "builtin".into(),
            version: 2,
            single_file: true,
            file_count: files.len(),
            crc_verified: true,
            staged_sha256_verified: true,
            archive_md5_verified: true,
            executable: None,
            snapshot,
        };
        Ok(MemberArtifact {
            package: PublishedPackage {
                path,
                sha256: if self.fault == Fault::MemberHash {
                    "0".repeat(64)
                } else {
                    sha256
                },
                files,
                packaging,
            },
            original_model_sha256: hash(&self.originals.read(&target).unwrap()),
            rebuilt_model_sha256: if self.fault == Fault::MissingModel {
                None
            } else {
                Some(model_sha.clone())
            },
            contract: Contract {
                target,
                model_sha,
                source_project: p.id.clone(),
            },
            warnings: vec!["fixture warning".into()],
        })
    }
    fn validate_member(
        &self,
        saved: &Project,
        _current: &Project,
        root: &Path,
        files: &Hashes,
        current_game: &Vpk,
        contract: &Contract,
        phase: ValidationPhase,
    ) -> Result<()> {
        self.validated
            .borrow_mut()
            .push((saved.original_model.clone(), phase));
        if self.fault == Fault::Validation && saved.original_model == ALT {
            return Err(CoreError::Integrity(
                "fixture assembled validator rejects alternate".into(),
            ));
        }
        assert_eq!(saved.id, contract.source_project);
        assert_eq!(saved.original_model, contract.target);
        assert_eq!(
            safety::digest(&root.join(&contract.target))?,
            contract.model_sha
        );
        assert_eq!(files[&contract.target], contract.model_sha);
        assert!(current_game.entries.contains_key(&contract.target));
        for path in files.keys() {
            assert!(fs::metadata(root.join(path))
                .unwrap()
                .permissions()
                .readonly());
        }
        Ok(())
    }
}
fn no_private_bundle_files(settings: &Settings, p: &Project) {
    let root = workspace::root(settings, p).unwrap();
    if root.is_dir() {
        assert!(!fs::read_dir(root).unwrap().any(|e| {
            let e = e.unwrap();
            let n = e.file_name();
            let n = n.to_string_lossy();
            n.starts_with("bundle-") || n.starts_with("pending_")
        }));
    }
}
#[test]
fn merged_bundle_revalidates_assembled_bytes_and_current_game_before_install() {
    let fixture = Fixture::new();
    let originals = Vpk::open(&fixture.original).unwrap();
    let members = Members::new(fixture.temp.path(), &originals, Fault::None);
    let cancel = CancellationToken::new();
    let clock = TimingRecorder::new("bundle-fixture");
    let runner = Runner::new(
        fixture.temp.path().join("logs"),
        cancel.clone(),
        RunnerOptions::default(),
    )
    .unwrap();
    let service = BundleService {
        settings: &fixture.settings,
        runner: &runner,
        builder: &members,
        cancel: &cancel,
        clock: &clock,
        log: &|_| {},
    };
    let mut project = fixture.parent.clone();
    project.install = true;
    let result = service.build(&project, false, false).unwrap();
    assert_eq!(result.forms, 2);
    assert_eq!(result.package.files.len(), 3);
    assert_eq!(result.manifest.warnings.len(), 1);
    assert_eq!(members.builds.get(), 2);
    assert_eq!(members.validated.borrow().len(), 4);
    assert!(result.installed.as_ref().unwrap().is_file());
    assert_eq!(
        safety::digest(result.installed.as_ref().unwrap()).unwrap(),
        result.package.sha256
    );
    assert_eq!(result.manifest.bundle.original_resources.len(), 4);
    assert!(result
        .manifest
        .bundle
        .members
        .iter()
        .all(|r| r.project.id != fixture.parent.id
            && r.project.id != fixture.child.id
            && !r.project.install
            && !bundle::active(&r.project)));
    let loaded = service.read_manifest(&result.manifest_path).unwrap();
    let archive = Vpk::open(&result.package.path).unwrap();
    service
        .verify_install(
            &project,
            &archive,
            &originals,
            &loaded,
            &result.package.sha256,
        )
        .unwrap();
    assert_eq!(loaded.bundle.members[1].source_project_id, fixture.child.id);
    assert!(clock
        .report()
        .steps
        .iter()
        .any(|s| s.name == "Validate every form in the assembled bundle"));
    assert_eq!(
        Project::load(&fixture.child_path).unwrap().id,
        fixture.child.id
    );
    no_private_bundle_files(&fixture.settings, &project);
}
#[test]
fn preflight_conflicts_incomplete_receipts_and_assembled_gate_preserve_previous_output() {
    for fault in [
        Fault::Preflight,
        Fault::SharedConflict,
        Fault::MemberHash,
        Fault::MissingModel,
        Fault::Validation,
    ] {
        let fixture = Fixture::new();
        let originals = Vpk::open(&fixture.original).unwrap();
        let members = Members::new(fixture.temp.path(), &originals, fault);
        let cancel = CancellationToken::new();
        let clock = TimingRecorder::new("rejected-bundle");
        let runner = Runner::new(
            fixture.temp.path().join("logs"),
            cancel.clone(),
            RunnerOptions::default(),
        )
        .unwrap();
        let previous = workspace::root(&fixture.settings, &fixture.parent)
            .unwrap()
            .join("output/pak18_dir.vpk");
        write(&previous, b"PREVIOUS PACKAGE");
        let service = BundleService {
            settings: &fixture.settings,
            runner: &runner,
            builder: &members,
            cancel: &cancel,
            clock: &clock,
            log: &|_| {},
        };
        assert!(service.build(&fixture.parent, true, false).is_err());
        assert_eq!(fs::read(previous).unwrap(), b"PREVIOUS PACKAGE");
        if fault == Fault::Preflight {
            assert_eq!(members.builds.get(), 0);
        }
        no_private_bundle_files(&fixture.settings, &fixture.parent);
    }
}
#[test]
fn install_rejects_changed_projects_original_closure_and_unowned_or_changed_files() {
    let fixture = Fixture::new();
    let originals = Vpk::open(&fixture.original).unwrap();
    let members = Members::new(fixture.temp.path(), &originals, Fault::None);
    let cancel = CancellationToken::new();
    let clock = TimingRecorder::new("recheck-bundle");
    let runner = Runner::new(
        fixture.temp.path().join("logs"),
        cancel.clone(),
        RunnerOptions::default(),
    )
    .unwrap();
    let service = BundleService {
        settings: &fixture.settings,
        runner: &runner,
        builder: &members,
        cancel: &cancel,
        clock: &clock,
        log: &|_| {},
    };
    let built = service.build(&fixture.parent, false, false).unwrap();
    let archive = Vpk::open(&built.package.path).unwrap();
    let mut changed = built.manifest.clone();
    changed
        .bundle
        .original_resources
        .remove("materials/game/base.vtex_c");
    assert!(service
        .verify_install(
            &fixture.parent,
            &archive,
            &originals,
            &changed,
            &built.package.sha256
        )
        .is_err());
    let mut changed_game = originals
        .entries
        .values()
        .map(|e| (e.path.clone(), originals.read(&e.path).unwrap()))
        .collect::<BTreeMap<_, _>>();
    changed_game.insert("materials/game/base.vtex_c".into(), b"GAME UPDATE".to_vec());
    let newer = archive_file(fixture.temp.path(), "updated-game", &changed_game);
    assert!(service
        .verify_install(
            &fixture.parent,
            &archive,
            &Vpk::open(newer).unwrap(),
            &built.manifest,
            &built.package.sha256
        )
        .is_err());
    let mut child = fixture.child.clone();
    child.id = "f".repeat(32);
    child.save(&fixture.child_path).unwrap();
    assert!(service
        .verify_install(
            &fixture.parent,
            &archive,
            &originals,
            &built.manifest,
            &built.package.sha256
        )
        .is_err());
    fixture.child.clone().save(&fixture.child_path).unwrap();
    let mut parent = fixture.parent.clone();
    parent.customization.icons = Some(vec![serde_json::from_value(
        serde_json::json!({"target":"panorama/images/heroes/new.vtex_c","allow_shared":true}),
    )
    .unwrap()]);
    assert!(service
        .verify_install(
            &parent,
            &archive,
            &originals,
            &built.manifest,
            &built.package.sha256
        )
        .is_err());
    let mut inputs = archive
        .entries
        .values()
        .map(|e| (e.path.clone(), archive.read(&e.path).unwrap()))
        .collect::<BTreeMap<_, _>>();
    inputs.insert("materials/unreviewed.vmat_c".into(), b"UNOWNED".to_vec());
    let extra = archive_file(fixture.temp.path(), "extra", &inputs);
    let extra = Vpk::open(extra).unwrap();
    let mut manifest = built.manifest.clone();
    manifest.package.sha256 = safety::digest(&extra.path).unwrap();
    manifest
        .package
        .files
        .insert("materials/unreviewed.vmat_c".into(), hash(b"UNOWNED"));
    assert!(service
        .verify_install(
            &fixture.parent,
            &extra,
            &originals,
            &manifest,
            &manifest.package.sha256
        )
        .is_err());
    no_private_bundle_files(&fixture.settings, &fixture.parent);
}
fn archive_file(root: &Path, name: &str, files: &BTreeMap<String, Vec<u8>>) -> PathBuf {
    archive(root, name, files)
}
#[test]
fn recursive_duplicate_foreign_and_cancelled_members_fail_before_any_build() {
    let fixture = Fixture::new();
    let originals = Vpk::open(&fixture.original).unwrap();
    let members = Members::new(fixture.temp.path(), &originals, Fault::None);
    let cancel = CancellationToken::new();
    let clock = TimingRecorder::new("bad-selection");
    let runner = Runner::new(
        fixture.temp.path().join("logs"),
        cancel.clone(),
        RunnerOptions::default(),
    )
    .unwrap();
    let service = BundleService {
        settings: &fixture.settings,
        runner: &runner,
        builder: &members,
        cancel: &cancel,
        clock: &clock,
        log: &|_| {},
    };
    let mut child = fixture.child.clone();
    child.customization = fixture.parent.customization.clone();
    child.save(&fixture.child_path).unwrap();
    assert!(service.build(&fixture.parent, false, false).is_err());
    fixture.child.clone().save(&fixture.child_path).unwrap();
    let mut parent = fixture.parent.clone();
    let rows = parent
        .customization
        .equipment
        .as_mut()
        .unwrap()
        .variants
        .as_mut()
        .unwrap();
    rows.push(rows[0].clone());
    assert!(service.build(&parent, false, false).is_err());
    let mut child = fixture.child.clone();
    child.id = fixture.parent.id.clone();
    child.save(&fixture.child_path).unwrap();
    assert!(service.build(&fixture.parent, false, false).is_err());
    fixture.child.clone().save(&fixture.child_path).unwrap();
    cancel.cancel();
    assert!(matches!(
        service.build(&fixture.parent, false, false),
        Err(CoreError::Cancelled)
    ));
    assert_eq!(members.builds.get(), 0);
}
#[test]
fn project_snapshot_decode_uses_authored_directory_without_rereading_file() {
    let fixture = Fixture::new();
    let bytes = fs::read(&fixture.child_path).unwrap();
    write(&fixture.child_path, b"changed after bounded read");
    let loaded = Project::load_from_bytes(&fixture.child_path, &bytes).unwrap();
    assert_eq!(loaded.id, fixture.child.id);
    assert_eq!(loaded.dmx, fixture.child.dmx);
}
