use deadlock_components::{
    audio, heroes, motion,
    resources::{self, ResourceKind},
};
use deadlock_core::{CancellationToken, safety};
use deadlock_project::customization::ResourceReplacement;
use deadlock_vpk::Vpk;
use std::{
    fs,
    path::{Path, PathBuf},
};
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}
fn read(name: &str) -> Vec<u8> {
    fs::read(fixture(name)).unwrap()
}
fn json(name: &str) -> serde_json::Value {
    serde_json::from_slice(&read(name)).unwrap()
}
#[test]
fn sound_metadata_and_pcm_replacement_match_python() {
    for name in ["sound-data", "sound-ctrl"] {
        let original = read(&format!("{name}.bin"));
        assert_eq!(
            serde_json::to_value(audio::metadata(&original).unwrap()).unwrap(),
            json(&format!("{name}.json"))
        );
        let output = audio::wav_replacement(&original, &read("samples.wav")).unwrap();
        assert_eq!(output, read(&format!("{name}-replaced.bin")));
        assert!(
            !audio::validate_sound(&original, &output)
                .unwrap()
                .native_validated
        );
        let wave = audio::playable_wave(&output).unwrap();
        assert_eq!(audio::wav_replacement(&original, &wave).unwrap(), output);
        assert!(audio::wav_replacement(&original, &read("bad-rate.wav")).is_err());
    }
}
#[test]
fn sound_rejects_metadata_changes_and_truncated_or_extra_samples() {
    let original = read("sound-data.bin");
    assert!(audio::validate_sound(&original, &read("bad-loop.bin")).is_err());
    let mut bad = original.clone();
    bad.pop();
    assert!(audio::metadata(&bad).is_err());
    bad = original.clone();
    bad.push(1);
    assert!(audio::metadata(&bad).is_err());
    assert!(audio::wav_replacement(&original, b"RIFF").is_err());
}
#[test]
fn particles_preserve_controlpoints_and_accept_cosmetic_color() {
    let original = read("particle.bin");
    let good = read("particle-replaced.bin");
    assert_eq!(
        serde_json::to_value(resources::validate_effect(&original, &good).unwrap()).unwrap(),
        json("particle.json")
    );
    assert!(resources::validate_effect(&original, &read("particle-bad-control.bin")).is_err());
    let mut trailing = good;
    trailing.push(0);
    assert!(resources::validate_effect(&original, &trailing).is_err());
}
#[test]
fn animation_pose_contract_and_discovery_match_python() {
    let cancel = CancellationToken::new();
    let original = read("clip.bin");
    assert_eq!(
        serde_json::to_value(
            motion::validate_clip(&original, &read("clip-replaced.bin"), &cancel).unwrap()
        )
        .unwrap(),
        json("clip.json")
    );
    for name in ["clip-bad-root.bin", "clip-bad-offset.bin"] {
        assert!(motion::validate_clip(&original, &read(name), &cancel).is_err());
    }
    let archive = Vpk::open(fixture("components_dir.vpk")).unwrap();
    assert_eq!(
        serde_json::to_value(
            motion::discover(
                &archive,
                "models/heroes_wip/test/test.vmdl_c",
                &cancel,
                |_| {}
            )
            .unwrap()
        )
        .unwrap(),
        json("animations.json")
    );
}
#[test]
fn authoritative_alias_discovery_and_shared_asset_opt_in() {
    let archive = Vpk::open(fixture("components_dir.vpk")).unwrap();
    let cancel = CancellationToken::new();
    let model = "models/heroes_staging/hornet_v3/hornet.vmdl";
    assert_eq!(
        serde_json::to_value(heroes::tokens(model).unwrap()).unwrap(),
        json("hero-tokens.json")
    );
    let rows = resources::discover(&archive, model, ResourceKind::Audio, &cancel).unwrap();
    let expected = json("audio-catalog.json");
    assert_eq!(rows.len(), expected.as_array().unwrap().len());
    for (row, expected) in rows.iter().zip(expected.as_array().unwrap()) {
        assert_eq!(row.target, expected["target"]);
        assert_eq!(row.association.shared, expected["shared"]);
        assert!(row.supported);
        assert_eq!(row.metadata.as_ref().unwrap(), &expected["metadata"]);
    }
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("replacement.wav");
    fs::write(&source, read("samples.wav")).unwrap();
    let mut selected = ResourceReplacement {
        source: Some(source.to_string_lossy().into()),
        target: Some("sounds/shared/gun.vsnd_c".into()),
        ..Default::default()
    };
    assert!(
        resources::stage(
            &archive,
            model,
            &[selected.clone()],
            dir.path(),
            ResourceKind::Audio,
            &cancel
        )
        .is_err()
    );
    selected.allow_shared = Some(true);
    let receipt = resources::stage(
        &archive,
        model,
        &[selected],
        dir.path(),
        ResourceKind::Audio,
        &cancel,
    )
    .unwrap();
    assert_eq!(
        resources::verify_compiled(&archive, &receipt, dir.path(), &cancel)
            .unwrap()
            .len(),
        1
    );
    safety::atomic_write(
        &dir.path().join("sounds/shared/gun.vsnd_c"),
        read("sound-data.bin"),
    )
    .unwrap();
    assert!(resources::verify_compiled(&archive, &receipt, dir.path(), &cancel).is_err());
}

#[test]
fn component_service_asset_only_build_freeze_and_install_bind_current_originals() {
    use deadlock_components::service::ComponentService;
    use deadlock_core::{
        TimingRecorder,
        process::{Runner, RunnerOptions},
    };
    use deadlock_project::{Project, Settings};
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path().join("workspace");
    let settings = Settings {
        workspace: workspace.to_string_lossy().into(),
        pak: fixture("components_dir.vpk").to_string_lossy().into(),
        ..Default::default()
    };
    let mut project = Project::default();
    project.original_model = "models/heroes_staging/hornet_v3/hornet.vmdl".into();
    project.customization.enabled = Some(true);
    project.customization.model_enabled = Some(false);
    project.customization.audio = Some(vec![ResourceReplacement {
        source: Some(fixture("samples.wav").to_string_lossy().into()),
        target: Some("sounds/heroes/hornet/rifle.vsnd_c".into()),
        ..Default::default()
    }]);
    project.customization.effects = Some(vec![ResourceReplacement {
        source: Some(dir.path().join("custom.vpcf_c").to_string_lossy().into()),
        target: Some("particles/heroes/hornet/trail.vpcf_c".into()),
        ..Default::default()
    }]);
    fs::write(
        dir.path().join("custom.vpcf_c"),
        read("particle-replaced.bin"),
    )
    .unwrap();
    let cancel = CancellationToken::new();
    let clock = TimingRecorder::new("component fixture");
    let runner = Runner::new(
        dir.path().join("logs"),
        cancel.clone(),
        RunnerOptions::default(),
    )
    .unwrap();
    let log = |_: &str| {};
    let service = ComponentService {
        settings: &settings,
        runner: &runner,
        cancel: &cancel,
        clock: &clock,
        log: &log,
    };
    let inspection = service.inspect(&project).unwrap();
    assert!(!inspection.native_compile_required);
    assert!(!inspection.model_enabled);
    let output = dir.path().join("output");
    let receipt = service.compile(&project, &output).unwrap().unwrap();
    assert_eq!(receipt.files.len(), 2);
    assert!(!receipt.native_validated);
    service
        .verify_snapshot(&project, &receipt, &output)
        .unwrap();
    let package = dir.path().join("pak18_dir.vpk");
    deadlock_vpk::write_vpk(
        &output,
        &receipt.files.keys().cloned().collect::<Vec<_>>(),
        &package,
        Some(&receipt.files),
        2,
        &mut || Ok(()),
    )
    .unwrap();
    let candidate = Vpk::open(package).unwrap();
    let current = Vpk::open(fixture("components_dir.vpk")).unwrap();
    service
        .verify_install(&project, &receipt, &candidate, &current)
        .unwrap();
    let mut forged = receipt.clone();
    forged.original_resources.clear();
    assert!(service.verify_snapshot(&project, &forged, &output).is_err());
    project.customization.audio.as_mut().unwrap()[0].target =
        Some("sounds/shared/gun.vsnd_c".into());
    assert!(
        service
            .verify_install(&project, &receipt, &candidate, &current)
            .is_err()
    );
    assert!(
        clock
            .report()
            .steps
            .iter()
            .any(|s| s.name == "Hash character asset dependencies")
    );
}

#[test]
fn authored_animation_snapshots_its_dmx_and_requires_fresh_compiled_contract() {
    let archive = Vpk::open(fixture("components_dir.vpk")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let cancel = CancellationToken::new();
    let selected = ResourceReplacement {
        source: Some(fixture("action.vnmclip").to_string_lossy().into()),
        target: Some("models/heroes_wip/test/idle.vnmclip_c".into()),
        ..Default::default()
    };
    let content = dir.path().join("content");
    let game = dir.path().join("game");
    let receipt = motion::stage(
        &archive,
        "models/heroes_wip/test/test.vmdl_c",
        &[selected],
        &content,
        &game,
        &cancel,
        false,
    )
    .unwrap();
    assert_eq!(
        receipt.compile_sources,
        vec!["models/heroes_wip/test/idle.vnmclip"]
    );
    assert_eq!(receipt.replacements[0].status, "awaiting_native_compile");
    let text = fs::read_to_string(content.join(&receipt.compile_sources[0])).unwrap();
    assert!(text.contains("models/_dmb_custom/animations/"));
    assert!(text.contains("m_eventTracks = []"));
    assert!(!game.exists());
    assert!(motion::verify_compiled(&archive, &receipt, &game, &cancel).is_err());
    safety::atomic_write(
        &game.join("models/heroes_wip/test/idle.vnmclip_c"),
        read("clip-replaced.bin"),
    )
    .unwrap();
    assert_eq!(
        motion::verify_compiled(&archive, &receipt, &game, &cancel)
            .unwrap()
            .len(),
        1
    );
    let mut project = deadlock_project::Project::default();
    project.customization.animations = Some(vec![ResourceReplacement {
        source: Some(fixture("action.vnmclip").to_string_lossy().into()),
        ..Default::default()
    }]);
    let paths = deadlock_components::service::input_paths(&project).unwrap();
    assert!(paths.iter().any(|p| p.ends_with("animation.dmx")));
}
#[test]
fn quaternion_validation_and_registered_variant_intent_are_conservative() {
    let cancel = CancellationToken::new();
    let rotation = read("clip-rotation.bin");
    assert!(motion::validate_clip(&rotation, &rotation, &cancel).is_ok());
    assert!(motion::validate_clip(&rotation, &read("clip-bad-rotation.bin"), &cancel).is_err());
    let archive = Vpk::open(fixture("variants_dir.vpk")).unwrap();
    let model = "models/heroes_wip/werewolf/werewolf.vmdl";
    assert_eq!(
        serde_json::to_value(
            deadlock_components::variants::discover_variants(&archive, model).unwrap()
        )
        .unwrap(),
        json("variants.json")
    );
    let mut parent = deadlock_project::Project::default();
    parent.original_model = model.into();
    let mut child = deadlock_project::Project::default();
    child.original_model = "models/heroes_wip/werewolf/transform/werewolf_transform.vmdl".into();
    child.dmx = "prepared.dmx".into();
    assert!(
        deadlock_components::variants::validate_variant_project(&parent, &child, &archive)
            .unwrap()
            .requires_full_model_validation
    );
    child.id = parent.id.clone();
    assert!(
        deadlock_components::variants::validate_variant_project(&parent, &child, &archive).is_err()
    );
    child.id = "a".repeat(32);
    child.original_model = "models/heroes_wip/ratking/ratking.vmdl".into();
    assert!(
        deadlock_components::variants::validate_variant_project(&parent, &child, &archive).is_err()
    );
}
