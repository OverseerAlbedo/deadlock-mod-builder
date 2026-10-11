//! Whole build transactions with a synthetic compiler and the real pinned
//! independent cloth reader. These resources are not playable game models.
#![cfg(all(
    target_arch = "x86_64",
    any(target_os = "linux", target_os = "windows")
))]
use deadlock_core::{
    process::{CommandSpec, ProcessResult, Runner, RunnerOptions, ToolExecutor},
    safety, CancellationToken, CoreError, Result, TimingRecorder, TimingStatus,
};
use deadlock_pipeline::{
    build::BuildService,
    inspect::ModelPipeline,
    workspace::{self, WorkspaceLease},
};
use deadlock_project::{MeshMapping, Project, Settings};
use deadlock_resource::{binary, container, Kind};
use deadlock_vpk::Vpk;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    },
};
const MODEL: &str = "models/test/model.vmdl";
const MATERIAL: &str = "models/test/cloth.vmat";
fn fixture(name: &str) -> Vec<u8> {
    fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/model-compile")
            .join(name),
    )
    .unwrap()
}
fn write(path: &Path, bytes: impl AsRef<[u8]>) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}
// A minimal structurally valid resource without dependencies; material parsing
// and full model validation remain separate responsibilities in production.
fn empty_resource() -> Vec<u8> {
    let mut b = Vec::new();
    b.extend(36u32.to_le_bytes());
    b.extend(12u16.to_le_bytes());
    b.extend(1u16.to_le_bytes());
    b.extend(8u32.to_le_bytes());
    b.extend(1u32.to_le_bytes());
    b.extend(b"RERL");
    b.extend(8u32.to_le_bytes());
    b.extend(8u32.to_le_bytes());
    b.extend(8u32.to_le_bytes());
    b.extend(0u32.to_le_bytes());
    b
}
fn original_archive(root: &Path, original: &[u8]) -> PathBuf {
    let source = root.join("game-fixture");
    write(&source.join(format!("{MODEL}_c")), original);
    let mut names = vec![format!("{MODEL}_c")];
    for r in deadlock_vpk::resource_references(original).unwrap() {
        let name = format!("{r}_c");
        write(&source.join(&name), empty_resource());
        names.push(name);
    }
    let archive = root.join(format!(
        "original-{}_dir.vpk",
        uuid::Uuid::new_v4().simple()
    ));
    deadlock_vpk::write_vpk(&source, &names, &archive, None, 2, &mut || Ok(())).unwrap();
    archive
}
fn setup(root: &Path) -> (Settings, Project) {
    let mut s = Settings::default();
    s.workspace = root.join("workspace").display().to_string();
    s.csdk = root.join("csdk").display().to_string();
    s.deadlock = root.join("deadlock").display().to_string();
    s.cswin = root.join("cswin").display().to_string();
    s.model_backend = "cswin".into();
    s.resourcecompiler = root
        .join("csdk/game/bin/win64/resourcecompiler.exe")
        .display()
        .to_string();
    write(
        Path::new(&s.resourcecompiler),
        b"synthetic material compiler identity",
    );
    write(
        &root.join("cswin/game/bin/win64/resourcecompiler.exe"),
        b"synthetic model compiler identity",
    );
    write(&root.join("cswin/game/csgo/gameinfo.gi"), b"fixture");
    write(
        &root.join("csdk/game/citadel/gameinfo.gi"),
        b"SDK game fixture",
    );
    let original = fixture("model.vmdl_c");
    let archive = original_archive(root, &original);
    let current = Path::new(&s.deadlock).join("game/citadel/pak01_dir.vpk");
    write(&current, fs::read(archive).unwrap());
    s.pak = current.display().to_string();
    let mut p = Project::default();
    p.original_model = format!("{MODEL}_c");
    p.imported_source = root.join("source").display().to_string();
    p.generate_materials = false;
    p.meshes = BTreeMap::from([
        ("body".into(), MeshMapping::One("body".into())),
        ("cape".into(), MeshMapping::One("cape".into())),
    ]);
    write(
        &Path::new(&p.imported_source).join(MODEL),
        fixture("model.vmdl"),
    );
    write(
        &Path::new(&p.imported_source).join(MATERIAL),
        b"Layer0\n{\n shader \"citadel_character.vfx\"\n}\n",
    );
    let block = container::read_blocks(&original, "DATA").unwrap().remove(0);
    let skeleton =
        deadlock_skeleton::Skeleton::from_value(block.root.get("m_modelSkeleton").unwrap())
            .unwrap();
    let mesh = |name: &str| serde_json::json!({"name":name,"positions":[[0.,0.,0.],[1.,0.,0.],[0.,1.,0.]],"normals":[[0.,0.,1.],[0.,0.,1.],[0.,0.,1.]],"uvs":[[[0.,0.],[1.,0.],[0.,1.]]],"colors":[],"weights":[[["root",1.]],[["root",1.]],[["root",1.]]],"faces":[{"vertices":[0,1,2],"corners":[0,1,2],"material":MATERIAL}]});
    let report: deadlock_skeleton::export::PreparedReport =
        serde_json::from_value(serde_json::json!({"meshes":[mesh("body"),mesh("cape")]})).unwrap();
    p.dmx = root.join("replacement.dmx").display().to_string();
    deadlock_skeleton::export::export_dmx(&report, &skeleton, Path::new(&p.dmx)).unwrap();
    (s, p)
}
fn stripped_candidate() -> Vec<u8> {
    let original = fixture("model.vmdl_c");
    let mut data = container::read_blocks(&original, "DATA").unwrap().remove(0);
    let mut phys = container::read_blocks(&original, "PHYS").unwrap().remove(0);
    phys.root.get_mut("m_pFeModel").unwrap().kind = Kind::Null;
    if let Kind::Array(flags) = &mut data
        .root
        .get_mut("m_modelSkeleton")
        .unwrap()
        .get_mut("m_nFlag")
        .unwrap()
        .kind
    {
        for f in flags {
            f.kind = Kind::I64((f.integer().unwrap() & !0x400008) as i64);
        }
    } else {
        panic!("fixture flags");
    }
    container::rebuild(
        &original,
        &BTreeMap::from([
            (
                "DATA".into(),
                binary::encode(&data.root, data.format_guid).unwrap(),
            ),
            (
                "PHYS".into(),
                binary::encode(&phys.root, phys.format_guid).unwrap(),
            ),
        ]),
    )
    .unwrap()
}
struct CompilerAndReader {
    inner: Runner,
    token: CancellationToken,
    logs: PathBuf,
    material_compiler: PathBuf,
    model_compiler: PathBuf,
    model_output: PathBuf,
    material_output: PathBuf,
    model_bytes: Vec<u8>,
    cancel_after_compile: bool,
    materials: AtomicUsize,
    models: AtomicUsize,
    readers: AtomicUsize,
    outputs: Mutex<Vec<PathBuf>>,
}
impl CompilerAndReader {
    fn new(root: &Path, s: &Settings, p: &Project, token: &CancellationToken) -> Self {
        let addon = format!("dmb_{}", &p.id[..16]);
        Self {
            inner: Runner::new(
                root.join("reader-logs"),
                token.clone(),
                RunnerOptions::default(),
            )
            .unwrap(),
            token: token.clone(),
            logs: root.join("compiler-logs"),
            material_compiler: PathBuf::from(&s.resourcecompiler),
            model_compiler: Path::new(&s.cswin).join("game/bin/win64/resourcecompiler.exe"),
            model_output: deadlock_tools::toolchain::select(s, true)
                .unwrap()
                .paths(&p.id)
                .unwrap()
                .game
                .join(format!("{MODEL}_c")),
            material_output: Path::new(&s.csdk)
                .join("game/citadel_addons")
                .join(addon)
                .join(format!("{MATERIAL}_c")),
            model_bytes: stripped_candidate(),
            cancel_after_compile: false,
            materials: AtomicUsize::new(0),
            models: AtomicUsize::new(0),
            readers: AtomicUsize::new(0),
            outputs: Mutex::new(vec![]),
        }
    }
}
impl ToolExecutor for CompilerAndReader {
    fn run(&self, spec: &CommandSpec) -> Result<ProcessResult> {
        if spec.program != self.material_compiler && spec.program != self.model_compiler {
            self.readers.fetch_add(1, Ordering::SeqCst);
            let result = self.inner.run(spec)?;
            self.outputs
                .lock()
                .unwrap()
                .push(result.stdout_path.clone());
            return Ok(result);
        }
        self.token.check()?;
        let path = spec
            .args
            .windows(2)
            .find(|a| a[0] == "-i")
            .map(|a| PathBuf::from(&a[1]))
            .expect("compiler input");
        let source = fs::read_to_string(&path).unwrap();
        let probe = source.contains("dmb_modeldoc_probe");
        let text = if probe {
            "No valid format conversion from 'dmb_modeldoc_probe' to 'modeldoc41'"
        } else if spec.program == self.material_compiler {
            let sdk_game = self
                .material_compiler
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join("citadel");
            let selected_game = spec
                .args
                .windows(2)
                .find(|args| args[0] == "-game")
                .expect("Material compilation must select the SDK game explicitly");
            assert_eq!(Path::new(&selected_game[1]), sdk_game);
            assert!(spec.args.iter().any(|arg| arg == "-nop4"));
            self.materials.fetch_add(1, Ordering::SeqCst);
            write(&self.material_output, empty_resource());
            "Material fixture compiled"
        } else {
            self.models.fetch_add(1, Ordering::SeqCst);
            write(&self.model_output, &self.model_bytes);
            if self.cancel_after_compile {
                self.token.cancel();
            }
            "Model fixture compiled"
        };
        let id = uuid::Uuid::new_v4().simple().to_string();
        let stdout = self.logs.join(format!("{id}.stdout"));
        let stderr = self.logs.join(format!("{id}.stderr"));
        write(&stdout, text);
        write(&stderr, []);
        Ok(ProcessResult {
            command_id: id.clone(),
            code: Some(if probe { 1 } else { 0 }),
            success: !probe,
            stdout_path: stdout,
            stderr_path: stderr,
            receipt_path: self.logs.join(format!("{id}.json")),
            elapsed_ns: 1,
            stdout_bytes: text.len() as u64,
            stderr_bytes: 0,
            dropped_live_events: 0,
        })
    }
    fn completed_tree_guard(&self) -> Result<Box<dyn Send>> {
        self.inner
            .completed_tree_guard()
            .map(|v| Box::new(v) as Box<dyn Send>)
    }
    fn quiescent(&self) -> Result<Box<dyn Send>> {
        self.inner
            .quiescent()
            .map(|barrier| Box::new(barrier) as Box<dyn Send>)
    }
}
fn reader() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../resources/native/cloth-reader")
        .canonicalize()
        .unwrap()
}
fn service<'a>(
    s: &'a Settings,
    runner: &'a dyn ToolExecutor,
    token: &'a CancellationToken,
    clock: &'a TimingRecorder,
    reader: &'a Path,
) -> BuildService<'a> {
    BuildService {
        model: ModelPipeline {
            settings: s,
            runner,
            cancel: token,
            clock,
            log: &|_| {},
            reader_root: reader,
        },
    }
}
fn target(s: &Settings, p: &Project) -> PathBuf {
    Path::new(&s.deadlock).join(format!(
        "game/citadel/addons/pak{:02}_dir.vpk",
        p.vpk_number
    ))
}
#[test]
fn full_build_restores_cloth_verifies_every_boundary_installs_and_reuses_material_cache() {
    let temp = tempfile::tempdir().unwrap();
    let (s, mut p) = setup(temp.path());
    let token = CancellationToken::new();
    let clock = TimingRecorder::new("full-build");
    let runner = CompilerAndReader::new(temp.path(), &s, &p, &token);
    let reader = reader();
    let service = service(&s, &runner, &token, &clock, &reader);
    let result = service.build(&mut p, false, false).unwrap();
    assert!(result.compiled);
    let output = result.vpk.unwrap();
    let installed = result.installed.unwrap();
    assert_eq!(installed, target(&s, &p));
    assert_eq!(
        safety::digest(&output).unwrap(),
        safety::digest(&installed).unwrap()
    );
    let archive = Vpk::open(&output).unwrap();
    let restored = archive.read(&format!("{MODEL}_c")).unwrap();
    assert_ne!(restored, runner.model_bytes);
    let audit = deadlock_cloth::audit_resource(&restored).unwrap();
    assert!(audit.cloth_nodes > 0);
    // An open reader denies replacement on Windows; close it before rebuilding.
    drop(archive);
    let manifest = service.read_manifest(&p).unwrap();
    assert_eq!(
        manifest.original_model_sha256,
        Some(
            safety::digest(
                &Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/fixtures/model-compile/model.vmdl_c")
            )
            .unwrap()
        )
    );
    assert!(manifest.motion_restoration.is_some());
    assert!(manifest.package.packaging.archive_md5_verified);
    assert_eq!(runner.materials.load(Ordering::SeqCst), 1);
    assert_eq!(runner.models.load(Ordering::SeqCst), 1);
    assert!(runner.readers.load(Ordering::SeqCst) > 0);
    let before = safety::digest(&output).unwrap();
    assert!(service.build(&mut p, false, false).is_err());
    assert_eq!(safety::digest(&output).unwrap(), before);
    service.build(&mut p, true, true).unwrap();
    assert_eq!(runner.materials.load(Ordering::SeqCst), 1);
    assert_eq!(runner.models.load(Ordering::SeqCst), 2);
    assert!(clock
        .report()
        .steps
        .iter()
        .any(|s| s.status == TimingStatus::Cached && s.name.contains("Build material")));
    // Neither a corrupt material payload nor modified old reader stdout may
    // acquire authority from its previous success receipt.
    let cache = workspace::root(&s, &p).unwrap().join("material-cache");
    for entry in fs::read_dir(cache).unwrap() {
        let path = entry
            .unwrap()
            .path()
            .join("files")
            .join(format!("{MATERIAL}_c"));
        if path.is_file() {
            write(&path, b"CORRUPTED MATERIAL CACHE");
        }
    }
    let readers_before = runner.readers.load(Ordering::SeqCst);
    for path in runner.outputs.lock().unwrap().iter() {
        let mut permissions = fs::metadata(path).unwrap().permissions();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            permissions.set_mode(0o600);
        }
        #[cfg(windows)]
        permissions.set_readonly(false);
        fs::set_permissions(path, permissions).unwrap();
        write(path, b"CORRUPTED PREVIOUS READER OUTPUT");
    }
    service.build(&mut p, true, true).unwrap();
    assert_eq!(runner.materials.load(Ordering::SeqCst), 2);
    assert_eq!(runner.models.load(Ordering::SeqCst), 3);
    assert!(runner.readers.load(Ordering::SeqCst) > readers_before);
    for name in [
        "Analyze replacement",
        "Prepare model and materials",
        "Build materials",
        "Restore or import runtime cloth",
        "Validate compiled skeleton and meshes",
        "Freeze and verify package snapshot",
        "Validate frozen models and components",
        "Verify packaged CRC and content hashes",
        "Validate current-game installation contracts",
        "Atomically copy installed VPK",
        "Release build workspace",
    ] {
        assert!(
            clock.report().steps.iter().any(|s| s.name == name),
            "missing step {name}"
        );
    }
    assert!(clock
        .report()
        .steps
        .iter()
        .all(|s| s.status != TimingStatus::Running));
}
#[test]
fn existing_package_rejects_new_current_game_even_when_skeleton_and_cloth_match() {
    let temp = tempfile::tempdir().unwrap();
    let (s, mut p) = setup(temp.path());
    p.install = false;
    let token = CancellationToken::new();
    let clock = TimingRecorder::new("current-game-change");
    let runner = CompilerAndReader::new(temp.path(), &s, &p, &token);
    let reader = reader();
    let service = service(&s, &runner, &token, &clock, &reader);
    let output = service.build(&mut p, false, false).unwrap().vpk.unwrap();
    let raw = fixture("model.vmdl_c");
    let mut data = container::read_blocks(&raw, "DATA").unwrap().remove(0);
    data.root
        .get_mut("m_otherValue")
        .unwrap()
        .get_mut("attachment")
        .unwrap()
        .kind = Kind::String("changed_engine_attachment".into());
    let changed = container::rebuild(
        &raw,
        &BTreeMap::from([(
            "DATA".into(),
            binary::encode(&data.root, data.format_guid).unwrap(),
        )]),
    )
    .unwrap();
    let replacement = original_archive(temp.path(), &changed);
    write(Path::new(&s.pak), fs::read(replacement).unwrap());
    write(&target(&s, &p), b"PREVIOUS ADDON");
    let error = service.install(&mut p, &output, true).unwrap_err();
    assert!(
        error.to_string().contains("original") || error.to_string().contains("game"),
        "{error}"
    );
    assert_eq!(fs::read(target(&s, &p)).unwrap(), b"PREVIOUS ADDON");
}
#[test]
fn corrupt_package_cannot_replace_existing_addon() {
    let temp = tempfile::tempdir().unwrap();
    let (s, mut p) = setup(temp.path());
    p.install = false;
    let token = CancellationToken::new();
    let clock = TimingRecorder::new("corrupt-package");
    let runner = CompilerAndReader::new(temp.path(), &s, &p, &token);
    let reader = reader();
    let service = service(&s, &runner, &token, &clock, &reader);
    let output = service.build(&mut p, false, false).unwrap().vpk.unwrap();
    let mut bytes = fs::read(&output).unwrap();
    let n = bytes.len();
    bytes[n - 1] ^= 1;
    write(&output, bytes);
    write(&target(&s, &p), b"PREVIOUS ADDON");
    assert!(service.install(&mut p, &output, true).is_err());
    assert_eq!(fs::read(target(&s, &p)).unwrap(), b"PREVIOUS ADDON");
}
#[test]
fn invalid_compiler_output_and_cancel_do_not_publish_or_keep_the_workspace_locked() {
    for cancel in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let (s, mut p) = setup(temp.path());
        let token = CancellationToken::new();
        let clock = TimingRecorder::new("failed-build");
        let mut runner = CompilerAndReader::new(temp.path(), &s, &p, &token);
        if cancel {
            runner.cancel_after_compile = true;
        } else {
            runner.model_bytes = b"bad compiler bytes".to_vec();
        }
        let reader = reader();
        let service = service(&s, &runner, &token, &clock, &reader);
        let error = service.build(&mut p, false, false).unwrap_err();
        if cancel {
            assert!(matches!(error, CoreError::Cancelled), "{error}");
            assert!(clock
                .report()
                .steps
                .iter()
                .any(|s| s.status == TimingStatus::Cancelled));
        }
        let root = workspace::root(&s, &p).unwrap();
        assert!(!root.join("output/pak18_dir.vpk").exists());
        assert!(!target(&s, &p).exists());
        assert!(!root.join("build-manifest.json").exists());
        drop(WorkspaceLease::acquire(&root).unwrap());
    }
}
#[test]
fn source_only_diagnostic_compile_checks_ag2_without_publishing() {
    let temp = tempfile::tempdir().unwrap();
    let (mut s, mut p) = setup(temp.path());
    s.pak.clear();
    p.build_vpk = false;
    p.install = false;
    p.ag2_mode = deadlock_project::Ag2Mode::Cswin;
    // A source-only model without external graph dependencies can compile for
    // diagnosis. Models referencing AG2 still require actual compiled graphs.
    let source = String::from_utf8(fixture("model.vmdl")).unwrap();
    let source = source
        .lines()
        .filter(|line| !line.contains("NmSkeletonList") && !line.contains("AnimGraph2List"))
        .collect::<Vec<_>>()
        .join("\n");
    write(&Path::new(&p.imported_source).join(MODEL), source);
    let token = CancellationToken::new();
    let clock = TimingRecorder::new("diagnostic");
    let mut runner = CompilerAndReader::new(temp.path(), &s, &p, &token);
    let mut data = container::read_blocks(&runner.model_bytes, "DATA")
        .unwrap()
        .remove(0);
    for name in ["m_vecNmSkeletonRefs", "m_animGraph2Refs"] {
        data.root.get_mut(name).unwrap().kind = Kind::Array(vec![]);
    }
    runner.model_bytes = container::rebuild(
        &runner.model_bytes,
        &BTreeMap::from([(
            "DATA".into(),
            binary::encode(&data.root, data.format_guid).unwrap(),
        )]),
    )
    .unwrap();
    let reader = reader();
    let service = service(&s, &runner, &token, &clock, &reader);
    let result = service.build(&mut p, false, false).unwrap();
    assert!(result.compiled);
    assert!(result.vpk.is_none() && result.installed.is_none() && result.manifest.is_none());
    assert!(!target(&s, &p).exists());
    p.build_vpk = true;
    assert!(service.build(&mut p, false, false).is_err());
    assert!(!workspace::root(&s, &p)
        .unwrap()
        .join("output/pak18_dir.vpk")
        .exists());
}
