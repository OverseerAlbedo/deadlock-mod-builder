//! Real bundled reader integration on typed synthetic resources and the accepted
//! native cloth reference. These tests do not run Valve's compiler or a game.
#![cfg(all(
    target_arch = "x86_64",
    any(target_os = "linux", target_os = "windows")
))]
use deadlock_core::{
    process::{CommandSpec, ProcessResult, Runner, RunnerOptions, ToolExecutor},
    safety, CancellationToken, CoreError, Result, TimingRecorder,
};
use deadlock_pipeline::motion::{selection, MotionService, Restoration, ValidatedMotion};
use deadlock_project::{Customization, Project};
use deadlock_resource::{binary, cloth, container, Kind};
use deadlock_skeleton::extensions::SourceBinding;
mod common;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    },
};
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap()
}
fn fixture(name: &str) -> Vec<u8> {
    fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../deadlock-cloth/tests/fixtures")
            .join(name),
    )
    .unwrap()
}
fn reader() -> PathBuf {
    root().join("resources/native/cloth-reader")
}
fn project(temp: &Path, reference: &Path) -> Project {
    let dmx = temp.join("replacement.dmx");
    fs::write(
        &dmx,
        b"motion boundary source identity only; no compiler exercised",
    )
    .unwrap();
    let mut p = Project::default();
    p.dmx = dmx.display().to_string();
    p.original_model = "models/heroes/test/test.vmdl_c".into();
    p.customization=serde_json::from_value::<Customization>(serde_json::json!({"enabled":true,"physics":{"enabled":true,"source":reference.display().to_string(),"hero_path":p.original_model}})).unwrap();
    p
}
// Fault injection is confined to this test's temporary raw-log files. Production
// Runner deliberately seals completed logs read-only.
fn corrupt_test_log(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut permissions = fs::metadata(path)?.permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        permissions.set_mode(0o600);
    }
    #[cfg(windows)]
    permissions.set_readonly(false);
    fs::set_permissions(path, permissions)?;
    fs::write(path, bytes)
}
struct ObservedRunner {
    inner: Runner,
    calls: AtomicUsize,
    corrupt: bool,
    mutate: Option<PathBuf>,
    outputs: Mutex<Vec<PathBuf>>,
}
impl ObservedRunner {
    fn new(path: &Path, cancel: CancellationToken) -> Self {
        Self {
            inner: Runner::new(path, cancel, RunnerOptions::default()).unwrap(),
            calls: AtomicUsize::new(0),
            corrupt: false,
            mutate: None,
            outputs: Mutex::new(vec![]),
        }
    }
}
impl ToolExecutor for ObservedRunner {
    fn run(&self, spec: &CommandSpec) -> Result<ProcessResult> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let mut result = self.inner.run(spec)?;
        self.outputs
            .lock()
            .unwrap()
            .push(result.stdout_path.clone());
        if self.corrupt {
            let text = result.read_stdout_utf8(512 * 1024 * 1024)?;
            let corrupt = text.replace("0.123456789", "0.123456788");
            corrupt_test_log(&result.stdout_path, corrupt.as_bytes())
                .map_err(|e| CoreError::io("Inject reader fixture fault", e))?;
            result.stdout_bytes = corrupt.len() as u64;
        }
        if let Some(path) = &self.mutate {
            fs::write(path, b"simulated external writer")
                .map_err(|e| CoreError::io("Inject concurrent compiler-output change", e))?;
        }
        Ok(result)
    }
    fn quiescent(&self) -> Result<Box<dyn Send>> {
        self.inner.quiescent().map(|v| Box::new(v) as Box<dyn Send>)
    }
}
#[test]
fn accepted_native_transfer_matches_python_and_actual_reader_cache_rechecks_raw_evidence() {
    let temp = tempfile::tempdir().unwrap();
    let directory = temp.path().join("motion");
    let token = CancellationToken::new();
    let clock = TimingRecorder::new("native-cloth-boundary");
    let runner = ObservedRunner::new(&temp.path().join("logs"), token.clone());
    let reader = reader();
    let service = MotionService {
        directory: &directory,
        reader_root: &reader,
        runner: &runner,
        cancel: &token,
        clock: &clock,
        log: &|_| {},
    };
    // The compiled Vindicta references are private assets; the test skips with a note without them.
    let (Some(original_path), Some(reference_path)) = (
        common::private_path("physics-references/Vindicta-R12/cloth_supported_hair.vmdl_c"),
        common::private_path("physics-references/Vindicta-R12-Balanced/cloth_supported_hair_balanced.vmdl_c"),
    ) else { return };
    let original = fs::read(&original_path).unwrap();
    let reference = fs::read(&reference_path).unwrap();
    assert_eq!(
        safety::digest(&reference_path).unwrap(),
        cloth::BALANCED_REFERENCE_SHA256
    );
    let manifest: serde_json::Value =
        serde_json::from_slice(&fixture("native-transfer.json")).unwrap();
    let layout = container::layout(&reference).unwrap();
    let read = |name| {
        binary::decode(
            container::payload(&reference, container::unique(&layout.blocks, name).unwrap())
                .unwrap(),
        )
        .unwrap()
    };
    let mut data = read("DATA");
    let mut physics = read("PHYS");
    physics.root.get_mut("m_pFeModel").unwrap().kind = Kind::Null;
    let flags = data
        .root
        .get_mut("m_modelSkeleton")
        .unwrap()
        .get_mut("m_nFlag")
        .unwrap();
    if let Kind::Array(flags) = &mut flags.kind {
        for flag in flags {
            flag.kind = Kind::I64((flag.integer().unwrap() & !0x400008) as i64);
        }
    } else {
        panic!("flags");
    }
    let candidate = container::rebuild(
        &reference,
        &BTreeMap::from([
            (
                "DATA".into(),
                binary::encode(&data.root, data.format_guid).unwrap(),
            ),
            (
                "PHYS".into(),
                binary::encode(&physics.root, physics.format_guid).unwrap(),
            ),
        ]),
    )
    .unwrap();
    let output = temp.path().join("compiled.vmdl_c");
    fs::write(&output, &candidate).unwrap();
    assert_eq!(
        safety::digest(&output).unwrap(),
        manifest["candidate_sha256"].as_str().unwrap()
    );
    let project = project(temp.path(), &reference_path);
    let contract = service
        .inspect_tested(&project, &original, None)
        .unwrap()
        .unwrap();
    assert_eq!(contract.independent_reference.blocks.len(), 2);
    assert!(!contract.independent_reference.numeric_tolerance_applied);
    let (receipt, audit) = service
        .restore(&project, &original, &output, Some(&contract), None)
        .unwrap();
    let Restoration::Tested {
        receipt,
        native_model_backup,
    } = receipt
    else {
        panic!("mode")
    };
    assert_eq!(fs::read(native_model_backup).unwrap(), candidate);
    assert!(receipt.audit.motion.passed);
    assert_eq!(
        safety::digest(&output).unwrap(),
        manifest["output_sha256"].as_str().unwrap()
    );
    let ValidatedMotion::Tested { audit, independent } = audit else {
        panic!("mode")
    };
    assert!(audit.all_tested_solver_fields_preserved);
    assert!(independent
        .blocks
        .values()
        .all(|v| v.values_checked > 1000 && !v.numeric_tolerance_applied));
    let before = runner.calls.load(Ordering::SeqCst);
    let built = fs::read(&output).unwrap();
    service.independent(&built).unwrap();
    assert_eq!(runner.calls.load(Ordering::SeqCst), before);
    assert!(clock
        .report()
        .steps
        .iter()
        .any(|s| s.name == "Reuse full-precision audit"));
    // A lost or modified raw log invalidates the cache and executes the pinned
    // reader again; a cached success flag cannot authorize missing evidence.
    corrupt_test_log(
        &independent.processes[0].stdout_path,
        b"modified cached log",
    )
    .unwrap();
    service.independent(&built).unwrap();
    assert_eq!(runner.calls.load(Ordering::SeqCst), before + 2);
    assert_eq!(
        safety::digest(&original_path).unwrap(),
        manifest["original_sha256"].as_str().unwrap()
    );
    assert_eq!(
        safety::digest(&reference_path).unwrap(),
        cloth::BALANCED_REFERENCE_SHA256
    );
}
#[test]
fn rejected_independent_numeric_output_keeps_compiler_bytes_and_failure_logs() {
    let temp = tempfile::tempdir().unwrap();
    let directory = temp.path().join("motion");
    let reader = reader();
    let token = CancellationToken::new();
    let clock = TimingRecorder::new("reader-failure");
    let mut runner = ObservedRunner::new(&temp.path().join("logs"), token.clone());
    runner.corrupt = true;
    let service = MotionService {
        directory: &directory,
        reader_root: &reader,
        runner: &runner,
        cancel: &token,
        clock: &clock,
        log: &|_| {},
    };
    let original = fixture("tested-original.vmdl_c");
    let reference = temp.path().join("reference.vmdl_c");
    fs::write(&reference, fixture("tested-reference.vmdl_c")).unwrap();
    let project = project(temp.path(), &reference);
    let mut source: SourceBinding = serde_json::from_slice(&fixture("tested-source.json")).unwrap();
    source.source_sha256 = safety::digest(Path::new(&project.dmx)).unwrap();
    // Establish the contract through the actual uncompromised reader first.
    let clean = ObservedRunner::new(&temp.path().join("clean-logs"), token.clone());
    let clean_dir = temp.path().join("clean-motion");
    let clean_service = MotionService {
        directory: &clean_dir,
        reader_root: &reader,
        runner: &clean,
        cancel: &token,
        clock: &clock,
        log: &|_| {},
    };
    let contract = clean_service
        .inspect_tested(&project, &original, Some(&source))
        .unwrap()
        .unwrap();
    let candidate = fixture("tested-candidate.vmdl_c");
    let output = temp.path().join("compiled.vmdl_c");
    fs::write(&output, &candidate).unwrap();
    let error = service
        .restore(&project, &original, &output, Some(&contract), Some(&source))
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("round-trip")
            || error.contains("precision")
            || error.contains("mismatch")
            || error.contains("no tolerance"),
        "{error}"
    );
    assert_eq!(fs::read(&output).unwrap(), candidate);
    assert!(fs::read_dir(&directory).unwrap().any(|e| e
        .unwrap()
        .path()
        .to_string_lossy()
        .ends_with("failure.json")));
    assert!(runner
        .outputs
        .lock()
        .unwrap()
        .iter()
        .any(|p| fs::read_to_string(p).unwrap().contains("0.123456788")));
    assert_eq!(
        fs::read_dir(directory.join("native-model"))
            .unwrap()
            .count(),
        1
    );
}
#[test]
fn changed_output_and_cancelled_work_never_commit_a_cloth_replacement() {
    let temp = tempfile::tempdir().unwrap();
    let directory = temp.path().join("motion");
    let reader = reader();
    let token = CancellationToken::new();
    let clock = TimingRecorder::new("motion-race");
    let output = temp.path().join("compiled.vmdl_c");
    let mut runner = ObservedRunner::new(&temp.path().join("logs"), token.clone());
    runner.mutate = Some(output.clone());
    let service = MotionService {
        directory: &directory,
        reader_root: &reader,
        runner: &runner,
        cancel: &token,
        clock: &clock,
        log: &|_| {},
    };
    let original = fixture("simple-original.vmdl_c");
    let candidate = fixture("simple-candidate.vmdl_c");
    fs::write(&output, &candidate).unwrap();
    let project = Project::default();
    let error = service
        .restore(&project, &original, &output, None, None)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("changed during motion validation"),
        "{error}"
    );
    assert_eq!(fs::read(&output).unwrap(), b"simulated external writer");
    fs::write(&output, &candidate).unwrap();
    token.cancel();
    assert!(matches!(
        service.restore(&project, &original, &output, None, None),
        Err(CoreError::Cancelled)
    ));
    assert_eq!(fs::read(&output).unwrap(), candidate);
}
#[test]
fn selection_requires_matching_hero_and_replacement_model() {
    let temp = tempfile::tempdir().unwrap();
    let reference = temp.path().join("reference.vmdl_c");
    fs::write(&reference, fixture("tested-reference.vmdl_c")).unwrap();
    let mut project = project(temp.path(), &reference);
    assert_eq!(
        selection(&project).unwrap().unwrap(),
        reference.canonicalize().unwrap()
    );
    project.original_model = "models/other.vmdl_c".into();
    assert!(selection(&project).is_err());
    project.customization.enabled = Some(false);
    assert!(selection(&project).unwrap().is_none());
}
