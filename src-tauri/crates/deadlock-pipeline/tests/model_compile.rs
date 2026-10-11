//! Typed synthetic compiler resources qualify gate wiring, never game compatibility.
use deadlock_core::{
    process::{CommandSpec, ProcessResult, ToolExecutor},
    safety, CancellationToken, CoreError, Result, TimingRecorder,
};
use deadlock_pipeline::{
    inspect::ModelPipeline,
    model_compile::{compiled_structure, verify_ag2_bytes},
    workspace,
};
use deadlock_project::{MeshMapping, Project, Settings};
use deadlock_resource::{binary, container, Kind, Value};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};
const MODEL: &str = "models/test/model.vmdl";
fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/model-compile")
}
fn write(p: &Path, data: impl AsRef<[u8]>) {
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, data).unwrap();
}
fn raw() -> Vec<u8> {
    fs::read(fixtures().join("model.vmdl_c")).unwrap()
}
fn source() -> String {
    fs::read_to_string(fixtures().join("model.vmdl")).unwrap()
}
fn setup(root: &Path) -> (Settings, Project, PathBuf, PathBuf) {
    let mut s = Settings::default();
    s.workspace = root.join("workspace").display().to_string();
    s.cswin = root.join("tools").display().to_string();
    s.model_backend = "cswin".into();
    s.model_format = "auto".into();
    write(
        &root.join("tools/game/bin/win64/resourcecompiler.exe"),
        b"SYNTHETIC TOOL IDENTITY",
    );
    write(&root.join("tools/game/csgo/gameinfo.gi"), b"fixture");
    let mut p = Project::default();
    p.original_model = format!("{MODEL}_c");
    let content = root.join("content");
    let game = root.join("game");
    write(&content.join(MODEL), source());
    write(
        &content.join("models/test/cloth.vmat"),
        b"DO NOT SEND MATERIAL SOURCE TO MODEL COMPILER",
    );
    for r in deadlock_model::ag2::source_nodes(&source()).unwrap() {
        write(&game.join(format!("{}_c", r.filename)), b"DEPENDENCY BYTES");
    }
    write(&game.join("models/test/cloth.vmat_c"), b"CSDK MATERIAL");
    (s, p, content, game)
}
struct CompilerDouble {
    output: PathBuf,
    logs: PathBuf,
    calls: AtomicUsize,
    failed: bool,
}
impl ToolExecutor for CompilerDouble {
    fn run(&self, s: &CommandSpec) -> Result<ProcessResult> {
        let n = self.calls.fetch_add(1, Ordering::SeqCst);
        let source = s
            .args
            .windows(2)
            .find(|a| a[0] == "-i")
            .map(|a| PathBuf::from(&a[1]))
            .unwrap();
        let text = fs::read_to_string(&source).unwrap();
        let probe = text.contains("dmb_modeldoc_probe");
        let out = if probe {
            "No valid format conversion from 'dmb_modeldoc_probe' to 'modeldoc41'"
        } else if self.failed {
            "ERROR: synthetic native compile rejection"
        } else {
            "Compiled synthetic model"
        };
        if !probe && !self.failed {
            write(&self.output, raw());
            write(
                &self.output.parent().unwrap().join("cloth.vmat_c"),
                b"MODEL COMPILER MATERIAL MUST NOT REPLACE CSDK",
            );
        }
        let stdout = self.logs.join(format!("{n}.stdout"));
        let stderr = self.logs.join(format!("{n}.stderr"));
        write(&stdout, out);
        write(&stderr, b"");
        Ok(ProcessResult {
            command_id: n.to_string(),
            code: Some(if probe || self.failed { 1 } else { 0 }),
            success: !probe && !self.failed,
            stdout_path: stdout,
            stderr_path: stderr,
            receipt_path: self.logs.join(format!("{n}.json")),
            elapsed_ns: 1,
            stdout_bytes: out.len() as u64,
            stderr_bytes: 0,
            dropped_live_events: 0,
        })
    }
    fn quiescent(&self) -> Result<Box<dyn Send>> {
        Ok(Box::new(()))
    }
}
struct NoTools;
impl ToolExecutor for NoTools {
    fn completed_tree_guard(&self) -> Result<Box<dyn Send>> {
        Ok(Box::new(()))
    }
    fn run(&self, _: &CommandSpec) -> Result<ProcessResult> {
        Err(CoreError::Integrity(
            "Offline plan cannot launch tools".into(),
        ))
    }
}
struct MutateAtValidationBarrier<'a> {
    runner: &'a dyn ToolExecutor,
    gates: AtomicUsize,
    output: PathBuf,
}
impl ToolExecutor for MutateAtValidationBarrier<'_> {
    fn completed_tree_guard(&self) -> Result<Box<dyn Send>> {
        self.runner.completed_tree_guard()
    }
    fn run(&self, spec: &CommandSpec) -> Result<ProcessResult> {
        self.runner.run(spec)
    }
    fn quiescent(&self) -> Result<Box<dyn Send>> {
        let guard = self.runner.quiescent()?;
        // Gate one deploys native output. Gate two protects the original-only
        // validation receipt after its typed DATA/graph checks have finished.
        if self.gates.fetch_add(1, Ordering::SeqCst) == 1 {
            write(&self.output, b"changed after typed model validation");
        }
        Ok(guard)
    }
}
#[test]
fn offline_compiler_plan_keeps_source_header_and_copies_only_model_sources() {
    let temp = tempfile::tempdir().unwrap();
    let (s, p, content, game) = setup(temp.path());
    let token = CancellationToken::new();
    let clock = TimingRecorder::new("offline");
    let pipeline = ModelPipeline {
        settings: &s,
        runner: &NoTools,
        cancel: &token,
        clock: &clock,
        log: &|_| {},
        reader_root: temp.path(),
    };
    let before = safety::digest(&content.join(MODEL)).unwrap();
    let plan = pipeline.prepare_cswin(&p, &content, &game, true).unwrap();
    assert!(!plan.compiled && plan.offline && !plan.format.header_rewritten);
    assert_eq!(plan.format.temporary_file_format, 28);
    assert_eq!(fs::read_to_string(&plan.temporary_model).unwrap(), source());
    assert_eq!(safety::digest(&content.join(MODEL)).unwrap(), before);
    assert!(!plan.content.join("models/test/cloth.vmat").exists());
    assert_eq!(
        fs::read(plan.game.join("models/test/cloth.vmat_c")).unwrap(),
        b"CSDK MATERIAL"
    );
}
#[test]
fn compiler_probe_deployment_and_failures_do_not_claim_native_success() {
    let temp = tempfile::tempdir().unwrap();
    let (s, p, content, game) = setup(temp.path());
    let paths = deadlock_tools::toolchain::select(&s, true)
        .unwrap()
        .paths(&p.id)
        .unwrap();
    let runner = CompilerDouble {
        output: paths.game.join(format!("{MODEL}_c")),
        logs: temp.path().join("logs"),
        calls: AtomicUsize::new(0),
        failed: false,
    };
    let token = CancellationToken::new();
    let clock = TimingRecorder::new("compile");
    let pipeline = ModelPipeline {
        settings: &s,
        runner: &runner,
        cancel: &token,
        clock: &clock,
        log: &|_| {},
        reader_root: temp.path(),
    };
    let plan = pipeline.compile_cswin(&p, &content, &game).unwrap();
    assert!(plan.compiled);
    assert!(!plan.format.native_migration_verified);
    assert_eq!(plan.format.compiler_target, Some(41));
    assert_eq!(plan.format.temporary_file_format, 28);
    assert_eq!(fs::read(game.join(format!("{MODEL}_c"))).unwrap(), raw());
    assert_eq!(
        fs::read(game.join("models/test/cloth.vmat_c")).unwrap(),
        b"CSDK MATERIAL"
    );
    assert_eq!(runner.calls.load(Ordering::SeqCst), 2);
    assert!(clock
        .report()
        .steps
        .iter()
        .any(|s| s.name == "Deploy freshly compiled model resources"));
    let failed = CompilerDouble {
        failed: true,
        output: runner.output.clone(),
        logs: runner.logs.clone(),
        calls: AtomicUsize::new(100),
    };
    let pipeline = ModelPipeline {
        runner: &failed,
        ..pipeline
    };
    assert!(pipeline.compile_cswin(&p, &content, &game).is_err());
    let receipt: serde_json::Value = serde_json::from_slice(
        &fs::read(workspace::root(&s, &p).unwrap().join("compiler-plan.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(receipt["compiled"], false);
    assert!(!receipt["compile_error"].as_str().unwrap().is_empty());
}
#[test]
fn missing_ag2_dependency_and_unsupported_downgrade_block_before_compile() {
    let temp = tempfile::tempdir().unwrap();
    let (mut s, p, content, game) = setup(temp.path());
    s.model_format = "40".into();
    let token = CancellationToken::new();
    let clock = TimingRecorder::new("reject");
    fs::remove_file(game.join("models/test/rig.vnmskel_c")).unwrap();
    let pipeline = ModelPipeline {
        settings: &s,
        runner: &NoTools,
        cancel: &token,
        clock: &clock,
        log: &|_| {},
        reader_root: temp.path(),
    };
    assert!(pipeline
        .prepare_cswin(&p, &content, &game, false)
        .unwrap_err()
        .to_string()
        .contains("dependencies"));
    let text = source().replacen(
        &deadlock_model::modeldoc::header(28).unwrap(),
        &deadlock_model::modeldoc::header(41).unwrap(),
        1,
    );
    write(&content.join(MODEL), text);
    assert!(pipeline
        .prepare_cswin(&p, &content, &game, false)
        .unwrap_err()
        .to_string()
        .contains("downgrade"));
    let receipt: serde_json::Value = serde_json::from_slice(
        &fs::read(workspace::root(&s, &p).unwrap().join("compiler-plan.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(receipt["state"], "source_format_rejected");
}
#[test]
fn deep_graph_bind_mesh_and_material_checks_reject_structurally_valid_corruption() {
    let raw = raw();
    let nodes = deadlock_model::ag2::source_nodes(&source()).unwrap();
    let data = container::read_blocks(&raw, "DATA").unwrap().remove(0);
    let skeleton =
        deadlock_skeleton::Skeleton::from_value(data.root.get("m_modelSkeleton").unwrap()).unwrap();
    let mappings = BTreeMap::from([
        ("body".into(), MeshMapping::One("replacement_body".into())),
        ("cape".into(), MeshMapping::One("replacement_cape".into())),
    ]);
    let materials = vec!["models/test/cloth.vmat".into()];
    assert!(compiled_structure(&raw, &skeleton, &nodes, &mappings, &materials).is_ok());
    assert!(
        compiled_structure(&raw, &skeleton, &nodes, &mappings, &["other.vmat".into()]).is_err()
    );
    let mut missing = mappings.clone();
    missing.remove("cape");
    assert!(compiled_structure(&raw, &skeleton, &nodes, &missing, &materials).is_err());
    let mut changed = data.root.clone();
    if let Kind::Array(values) = &mut changed.get_mut("m_animGraph2Refs").unwrap().kind {
        values[1].get_mut("m_sIdentifier").unwrap().kind = Kind::String("NotUI".into());
    }
    let changed = container::rebuild(
        &raw,
        &BTreeMap::from([(
            "DATA".into(),
            binary::encode(&changed, data.format_guid).unwrap(),
        )]),
    )
    .unwrap();
    assert!(verify_ag2_bytes(&nodes, &changed).is_err());
    let mut changed = data.root;
    let scale = changed
        .get_mut("m_modelSkeleton")
        .unwrap()
        .get_mut("m_boneScaleParent")
        .unwrap();
    if let Kind::Array(values) = &mut scale.kind {
        values[1] = Value::new(Kind::F64(2.));
    }
    let changed = container::rebuild(
        &raw,
        &BTreeMap::from([(
            "DATA".into(),
            binary::encode(&changed, data.format_guid).unwrap(),
        )]),
    )
    .unwrap();
    assert!(compiled_structure(&changed, &skeleton, &nodes, &mappings, &materials).is_err());
}
#[test]
fn original_validation_offline_does_not_open_replacement_or_claim_compilation() {
    let temp = tempfile::tempdir().unwrap();
    let (mut settings, mut project, content, _game) = setup(temp.path());
    project.imported_source = content.display().to_string();
    project.original_compiled = fixtures().join("model.vmdl_c").display().to_string();
    project.dmx = temp.path().join("DOES_NOT_EXIST.dmx").display().to_string();
    settings.pak.clear();
    let token = CancellationToken::new();
    let clock = TimingRecorder::new("original-only");
    let pipeline = ModelPipeline {
        settings: &settings,
        runner: &NoTools,
        cancel: &token,
        clock: &clock,
        log: &|_| {},
        reader_root: temp.path(),
    };
    let report = pipeline.validate_original(&project, false).unwrap();
    assert!(
        !report.compiled && !report.replacement_dmx_used && !report.packaged && !report.installed
    );
    assert_eq!(report.original.ag2.len(), 3);
    assert!(report.validation_evidence.is_none());
    assert!(!workspace::root(&settings, &project)
        .unwrap()
        .join("build.lock")
        .exists());
}
#[test]
fn original_compile_receipt_requires_deep_graph_proof_and_marks_double_non_native() {
    let temp = tempfile::tempdir().unwrap();
    let (mut settings, mut project, content, _) = setup(temp.path());
    project.imported_source = content.display().to_string();
    project.dmx = temp.path().join("UNUSED.dmx").display().to_string();
    let pakroot = temp.path().join("pak-input");
    write(&pakroot.join(format!("{MODEL}_c")), raw());
    let refs = deadlock_model::ag2::source_nodes(&source()).unwrap();
    let mut files = vec![format!("{MODEL}_c")];
    for n in &refs {
        let name = format!("{}_c", n.filename);
        write(&pakroot.join(&name), raw());
        files.push(name);
    }
    let pak = temp.path().join("original_dir.vpk");
    deadlock_vpk::write_vpk(&pakroot, &files, &pak, None, 1, &mut || Ok(())).unwrap();
    settings.pak = pak.display().to_string();
    let paths = deadlock_tools::toolchain::select(&settings, true)
        .unwrap()
        .paths(&project.id)
        .unwrap();
    let runner = CompilerDouble {
        output: paths.game.join(format!("{MODEL}_c")),
        logs: temp.path().join("logs"),
        calls: AtomicUsize::new(0),
        failed: false,
    };
    let token = CancellationToken::new();
    let clock = TimingRecorder::new("original-compile");
    let pipeline = ModelPipeline {
        settings: &settings,
        runner: &runner,
        cancel: &token,
        clock: &clock,
        log: &|_| {},
        reader_root: temp.path(),
    };
    let result = pipeline.validate_original(&project, true).unwrap();
    assert!(
        result.compiled && !result.replacement_dmx_used && !result.installed && !result.packaged
    );
    let evidence = result.validation_evidence.unwrap();
    assert!(!evidence.native && !evidence.native_migration_verified);
    assert_eq!(evidence.ag2_nodes_expected, evidence.ag2_nodes_checked);
    assert!(evidence.ag2_output_checked);
    assert_eq!(evidence.ag2_nodes_checked.len(), 3);
    let receipt = result.toolchain_evidence.unwrap();
    assert!(!receipt.native);
    assert_eq!(receipt.kind, "non_native_or_test_process_compile");
    let mutation = MutateAtValidationBarrier {
        runner: &runner,
        gates: AtomicUsize::new(0),
        output: workspace::root(&settings, &project)
            .unwrap()
            .join("original-test-game")
            .join(format!("{MODEL}_c")),
    };
    let pipeline = ModelPipeline {
        runner: &mutation,
        ..pipeline
    };
    let error = pipeline.validate_original(&project, true).unwrap_err();
    assert!(
        error.to_string().contains("changed during validation"),
        "{error}"
    );
    let failed: serde_json::Value = serde_json::from_slice(
        &fs::read(
            workspace::root(&settings, &project)
                .unwrap()
                .join("original-validation.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(failed["compiled"], false);
}
#[test]
#[cfg(all(
    target_arch = "x86_64",
    any(target_os = "linux", target_os = "windows")
))]
fn full_compiled_validation_uses_snapshot_fallback_and_real_fullprecision_reader() {
    use deadlock_core::process::{Runner, RunnerOptions};
    let temp = tempfile::tempdir().unwrap();
    let (mut settings, mut project, content, _) = setup(temp.path());
    settings.pak.clear();
    project.imported_source = content.display().to_string();
    project.original_compiled = fixtures().join("model.vmdl_c").display().to_string();
    project.generate_materials = false;
    project.meshes = BTreeMap::from([
        ("body".into(), MeshMapping::One("body".into())),
        ("cape".into(), MeshMapping::One("cape".into())),
    ]);
    let data = container::read_blocks(&raw(), "DATA").unwrap().remove(0);
    let skeleton =
        deadlock_skeleton::Skeleton::from_value(data.root.get("m_modelSkeleton").unwrap()).unwrap();
    let mesh = |name: &str| serde_json::json!({"name":name,"positions":[[0.,0.,0.],[1.,0.,0.],[0.,1.,0.]],"normals":[[0.,0.,1.],[0.,0.,1.],[0.,0.,1.]],"uvs":[[[0.,0.],[1.,0.],[0.,1.]]],"colors":[],"weights":[[["root",1.]],[["root",1.]],[["root",1.]]],"faces":[{"vertices":[0,1,2],"corners":[0,1,2],"material":"models/test/cloth.vmat"}]});
    let prepared: deadlock_skeleton::export::PreparedReport =
        serde_json::from_value(serde_json::json!({"meshes":[mesh("body"),mesh("cape")]})).unwrap();
    let path = temp.path().join("replacement.dmx");
    deadlock_skeleton::export::export_dmx(&prepared, &skeleton, &path).unwrap();
    project.dmx = path.display().to_string();
    let token = CancellationToken::new();
    let clock = TimingRecorder::new("full-model-validation");
    let runner = Runner::new(
        temp.path().join("reader-logs"),
        token.clone(),
        RunnerOptions::default(),
    )
    .unwrap();
    let reader = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../resources/native/cloth-reader")
        .canonicalize()
        .unwrap();
    let pipeline = ModelPipeline {
        settings: &settings,
        runner: &runner,
        cancel: &token,
        clock: &clock,
        log: &|_| {},
        reader_root: &reader,
    };
    let report = pipeline.inspect(&mut project).unwrap();
    assert!(!report.source_identity.blocked);
    let output = temp.path().join("candidate.vmdl_c");
    write(&output, raw());
    let validated = pipeline
        .validate_compiled(&project, &output, &report)
        .unwrap();
    assert!(validated.structure.mesh_mapping_passed);
    assert!(validated.structure.ag2_graph_identities_preserved);
    assert!(validated.target_scale_audit.is_some());
    assert!(project.skeleton_reference.is_empty());
    write(&path, b"changed source");
    assert!(pipeline
        .validate_compiled(&project, &output, &report)
        .is_err());
}
