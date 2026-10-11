//! End-to-end manifest tampering checks use the real asset-only contracts and VPK
//! writer, without a native compiler or playable game-resource claim.
use deadlock_core::{
    process::{Runner, RunnerOptions},
    safety, CancellationToken, TimingRecorder,
};
use deadlock_pipeline::{build::BuildService, inspect::ModelPipeline, workspace};
use deadlock_project::{customization::ResourceReplacement, Project, Settings};
use std::{
    fs,
    path::{Path, PathBuf},
};
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../deadlock-components/tests/fixtures")
        .join(name)
}
#[test]
fn component_receipt_cannot_be_removed_or_replaced_with_unselected_targets() {
    let temp = tempfile::tempdir().unwrap();
    let token = CancellationToken::new();
    let clock = TimingRecorder::new("receipt-gates");
    let runner = Runner::new(
        temp.path().join("logs"),
        token.clone(),
        RunnerOptions::default(),
    )
    .unwrap();
    let mut settings = Settings::default();
    settings.workspace = temp.path().join("workspace").display().to_string();
    settings.deadlock = temp.path().join("deadlock").display().to_string();
    settings.pak = fixture("components_dir.vpk").display().to_string();
    let original = Path::new(&settings.deadlock).join("game/citadel/pak01_dir.vpk");
    safety::atomic_copy(Path::new(&settings.pak), &original, &token).unwrap();
    let mut project = Project::default();
    project.original_model = "models/heroes_staging/hornet_v3/hornet.vmdl".into();
    project.install = false;
    project.customization.enabled = Some(true);
    project.customization.model_enabled = Some(false);
    project.customization.audio = Some(vec![ResourceReplacement {
        source: Some(fixture("samples.wav").display().to_string()),
        target: Some("sounds/heroes/hornet/rifle.vsnd_c".into()),
        ..Default::default()
    }]);
    let service = BuildService {
        model: ModelPipeline {
            settings: &settings,
            runner: &runner,
            cancel: &token,
            clock: &clock,
            log: &|_| {},
            reader_root: temp.path(),
        },
    };
    let result = service.build(&mut project, false, false).unwrap();
    let output = result.vpk.unwrap();
    let before = safety::digest(&output).unwrap();
    let manifest_path = workspace::root(&settings, &project)
        .unwrap()
        .join("build-manifest.json");
    let valid: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
    assert!(service
        .read_manifest(&project)
        .unwrap()
        .customization
        .is_some());
    let installed = Path::new(&settings.deadlock).join("game/citadel/addons/pak18_dir.vpk");
    let mut missing = valid.clone();
    missing["customization"] = serde_json::Value::Null;
    safety::write_json(&manifest_path, &missing).unwrap();
    assert!(service
        .read_manifest(&project)
        .unwrap_err()
        .to_string()
        .contains("receipt"));
    assert!(service
        .install(&mut project, &output, false)
        .unwrap_err()
        .to_string()
        .contains("receipt"));
    assert!(!installed.exists());
    // Both assets-only and model-plus-assets require the typed component receipt.
    let mut model = project.clone();
    model.customization.model_enabled = Some(true);
    missing["project"] = serde_json::to_value(&model).unwrap();
    safety::write_json(&manifest_path, &missing).unwrap();
    assert!(service
        .read_manifest(&model)
        .unwrap_err()
        .to_string()
        .contains("receipt"));
    // Editing the saved and current selections together cannot silently authorize
    // packaged extras with no review/receipt either.
    model.customization.audio = None;
    missing["project"] = serde_json::to_value(&model).unwrap();
    safety::write_json(&manifest_path, &missing).unwrap();
    assert!(service
        .read_manifest(&model)
        .unwrap_err()
        .to_string()
        .contains("selected targets"));
    let mut added = valid.clone();
    added["package"]["files"]["sounds/shared/unreviewed.vsnd_c"] =
        serde_json::Value::String("a".repeat(64));
    safety::write_json(&manifest_path, &added).unwrap();
    assert!(service
        .read_manifest(&project)
        .unwrap_err()
        .to_string()
        .contains("selected targets"));
    let mut removed = valid.clone();
    removed["customization"]["files"] = serde_json::json!({});
    safety::write_json(&manifest_path, &removed).unwrap();
    assert!(service
        .read_manifest(&project)
        .unwrap_err()
        .to_string()
        .contains("receipt"));
    let mut changed = valid.clone();
    changed["customization"]["files"]["sounds/heroes/hornet/rifle.vsnd_c"] =
        serde_json::Value::String("b".repeat(64));
    safety::write_json(&manifest_path, &changed).unwrap();
    assert!(service
        .read_manifest(&project)
        .unwrap_err()
        .to_string()
        .contains("hashes"));
    assert_eq!(before, safety::digest(&output).unwrap());
    assert!(!installed.exists());
    // The original receipt still reaches fresh current-game validation and installs.
    safety::write_json(&manifest_path, &valid).unwrap();
    service.install(&mut project, &output, false).unwrap();
    assert_eq!(before, safety::digest(&installed).unwrap());
}
