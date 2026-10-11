use deadlock_core::{
    process::{CommandSpec, ProcessResult, ToolExecutor},
    CancellationToken, CoreError, Result, TimingRecorder, TimingStatus,
};
use deadlock_pipeline::inspect::ModelPipeline;
use deadlock_project::{MeshMapping, Project, Settings};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
};
const MODEL: &str = "models/heroes/test/test.vmdl";
const MAT: &str = "models/unusual/agent/body.vmat";
struct NoTools;
impl ToolExecutor for NoTools {
    fn run(&self, _: &CommandSpec) -> Result<ProcessResult> {
        Err(CoreError::Integrity(
            "Offline fixture must not invoke native tools".into(),
        ))
    }
}
fn write(p: &Path, b: &[u8]) {
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, b).unwrap();
}
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/model")
        .join(name)
}
fn setup() -> (tempfile::TempDir, Project, Settings) {
    let temp = tempfile::tempdir().unwrap();
    let imported = temp.path().join("imported");
    write(
        &imported.join(MODEL),
        &fs::read(fixture("original.vmdl")).unwrap(),
    );
    write(
        &imported.join(MAT),
        b"Layer0 { shader = \"citadel_character.vfx\" }",
    );
    let dmx = temp.path().join("replacement.dmx");
    write(
        &dmx,
        include_bytes!("../../deadlock-dmx/tests/fixtures/basic-v9.dmx"),
    );
    let mut p = Project::default();
    p.original_model = format!("{MODEL}_c");
    p.imported_source = imported.display().to_string();
    p.dmx = dmx.display().to_string();
    p.generate_materials = false;
    let mut s = Settings::default();
    s.workspace = temp.path().join("work").display().to_string();
    (temp, p, s)
}
fn pipeline<'a>(
    s: &'a Settings,
    c: &'a CancellationToken,
    t: &'a TimingRecorder,
) -> ModelPipeline<'a> {
    ModelPipeline {
        settings: s,
        runner: &NoTools,
        cancel: c,
        clock: t,
        log: &|_| {},
        reader_root: Path::new("__no_reader__"),
    }
}

#[test]
fn offline_inspection_and_prepared_model_match_python_golden_bytes() {
    let (_temp, mut project, settings) = setup();
    let token = CancellationToken::new();
    let clock = TimingRecorder::new("model-source-golden");
    let pipe = pipeline(&settings, &token, &clock);
    let report = pipe.inspect(&mut project).unwrap();
    assert!(!report.needs_review);
    let actual = serde_json::to_value(&report).unwrap();
    let expected: Value =
        serde_json::from_slice(&fs::read(fixture("inspection.json")).unwrap()).unwrap();
    for (key, value) in expected.as_object().unwrap() {
        assert_eq!(&actual[key], value, "{key}");
    }
    let prepared = pipe.prepare(&mut project, &report, true).unwrap();
    assert_eq!(
        fs::read(prepared.content.join(MODEL)).unwrap(),
        fs::read(fixture("prepared.vmdl")).unwrap()
    );
    let actual = serde_json::to_value(&prepared).unwrap();
    let expected: Value =
        serde_json::from_slice(&fs::read(fixture("prepared.json")).unwrap()).unwrap();
    for (key, value) in expected.as_object().unwrap() {
        assert_eq!(&actual[key], value, "{key}");
    }
    for path in prepared.mesh_sources.values() {
        assert_eq!(
            fs::read(prepared.content.join(path)).unwrap(),
            fs::read(fixture(
                Path::new(path).file_name().unwrap().to_str().unwrap()
            ))
            .unwrap()
        );
    }
    assert_eq!(
        fs::read(&project.dmx).unwrap(),
        include_bytes!("../../deadlock-dmx/tests/fixtures/basic-v9.dmx")
    );
    for name in [
        "Analyze replacement",
        "Read and parse replacement DMX",
        "Analyze DMX meshes, weights and bones",
        "Count model geometry",
        "Resolve mesh and LOD mappings",
        "Validate source skeleton identity",
        "Plan material and texture mappings",
        "Prepare source trees and ModelDoc",
        "Copy original source tree",
        "Save source provenance",
        "Apply material customization",
        "Verify preparation source stability",
    ] {
        let report = clock.report();
        let step = report
            .steps
            .iter()
            .find(|s| s.name == name)
            .unwrap_or_else(|| panic!("Missing timing for {name}"));
        assert_eq!(step.status, TimingStatus::Succeeded);
        assert!(step.inclusive_ns > 0);
    }
    assert!(clock
        .report()
        .steps
        .iter()
        .any(|s| s.name == "Generate material sources and textures"
            && s.status == TimingStatus::Skipped));
}

#[test]
fn changed_project_cannot_use_old_inspection_or_erase_generated_tree() {
    let (temp, mut p, s) = setup();
    let c = CancellationToken::new();
    let t = TimingRecorder::new("stale-project");
    let pipe = pipeline(&s, &c, &t);
    let report = pipe.inspect(&mut p).unwrap();
    let generated = pipe.root(&p).unwrap().join("prepared-content/protect.txt");
    write(&generated, b"unrelated");
    p.meshes
        .insert("body".into(), MeshMapping::One("weapon".into()));
    assert!(pipe
        .prepare(&mut p, &report, true)
        .unwrap_err()
        .to_string()
        .contains("Project choices changed"));
    assert_eq!(fs::read(generated).unwrap(), b"unrelated");
    assert!(!temp.path().join("compiled").exists());
}

#[test]
fn changed_dmx_and_effective_sources_and_reconciled_source_reject_stale_receipts() {
    for mode in ["dmx", "effective", "source_view"] {
        let (_temp, mut p, s) = setup();
        let c = CancellationToken::new();
        let t = TimingRecorder::new(mode);
        let pipe = pipeline(&s, &c, &t);
        let report = pipe.inspect(&mut p).unwrap();
        let path = match mode {
            "dmx" => PathBuf::from(&p.dmx),
            "effective" => report.effective_sources.join(MAT),
            _ => report.original.source_view.clone(),
        };
        let mut bytes = fs::read(&path).unwrap();
        bytes.extend_from_slice(b"changed");
        fs::write(path, bytes).unwrap();
        assert!(pipe.prepare(&mut p, &report, true).is_err(), "{mode}");
        assert!(!pipe.root(&p).unwrap().join("prepared.json").exists());
    }
}

#[test]
fn missing_material_and_ambiguous_mapping_require_review_before_preparation() {
    let (_temp, mut p, s) = setup();
    fs::remove_file(Path::new(&p.imported_source).join(MAT)).unwrap();
    p.auto_map = false;
    let c = CancellationToken::new();
    let t = TimingRecorder::new("review");
    let pipe = pipeline(&s, &c, &t);
    let report = pipe.inspect(&mut p).unwrap();
    assert!(report.needs_review);
    assert_eq!(report.unresolved_meshes, vec!["body"]);
    assert!(pipe
        .prepare(&mut p, &report, true)
        .unwrap_err()
        .to_string()
        .contains("Review mesh mappings"));
}

#[test]
fn imported_dmx_hash_and_unowned_output_and_cancellation_are_hard_gates() {
    let (_temp, mut p, s) = setup();
    let c = CancellationToken::new();
    let t = TimingRecorder::new("gates");
    let pipe = pipeline(&s, &c, &t);
    p.expected_dmx_sha256 = "0".repeat(64);
    assert!(pipe
        .inspect(&mut p)
        .unwrap_err()
        .to_string()
        .contains("package hash"));
    p.expected_dmx_sha256.clear();
    let report = pipe.inspect(&mut p).unwrap();
    let protected = pipe.root(&p).unwrap().join("prepared-content/unowned.txt");
    write(&protected, b"keep");
    assert!(pipe
        .prepare(&mut p, &report, true)
        .unwrap_err()
        .to_string()
        .contains("unowned"));
    assert_eq!(fs::read(protected).unwrap(), b"keep");
    c.cancel();
    assert!(matches!(
        pipe.prepare(&mut p, &report, true),
        Err(CoreError::Cancelled)
    ));
}

#[test]
fn automatically_split_preparation_revalidates_and_uses_generated_meshes() {
    let (_temp, mut p, s) = setup();
    write(
        Path::new(&p.dmx),
        include_bytes!("../../deadlock-dmx/tests/fixtures/split-v9.dmx"),
    );
    for material in ["models/a.vmat", "models/b.vmat"] {
        write(
            &Path::new(&p.imported_source).join(material),
            b"Layer0 { shader = \"citadel_character.vfx\" }",
        );
    }
    p.auto_split = true;
    p.split_target = 6;
    let c = CancellationToken::new();
    let t = TimingRecorder::new("split-prepare");
    let pipe = pipeline(&s, &c, &t);
    let report = pipe.inspect(&mut p).unwrap();
    assert!(report.automatic_split.is_some());
    assert!(!report.needs_review);
    let prepared = pipe.prepare(&mut p, &report, true).unwrap();
    assert!(prepared.mesh_sources.len() > 1);
    for path in prepared.mesh_sources.values() {
        let dmx = deadlock_dmx::Dmx::load(&prepared.content.join(path)).unwrap();
        assert_eq!(dmx.summary().unwrap().meshes.len(), 1);
    }
    let split = report.automatic_split.as_ref().unwrap();
    let file = split
        .directory
        .as_ref()
        .unwrap()
        .join(&split.groups[0].pieces[0].file);
    let mut bytes = fs::read(&file).unwrap();
    bytes[200] ^= 1;
    fs::write(file, bytes).unwrap();
    // Normalize choices back to their analysis state only for this stale split check.
    p.meshes = report.project_mappings.clone();
    p.materials.clear();
    assert!(deadlock_dmx::split::validate_bundle(
        &deadlock_dmx::Dmx::load(Path::new(&p.dmx)).unwrap(),
        split,
        split.directory.as_ref().unwrap(),
        &c
    )
    .is_err());
}

#[cfg(unix)]
#[test]
fn source_symlink_is_rejected_before_parsing() {
    let (temp, mut p, s) = setup();
    let real = PathBuf::from(&p.dmx);
    let link = temp.path().join("linked.dmx");
    std::os::unix::fs::symlink(real, &link).unwrap();
    p.dmx = link.display().to_string();
    let c = CancellationToken::new();
    let t = TimingRecorder::new("linked-source");
    assert!(pipeline(&s, &c, &t).inspect(&mut p).is_err());
}
