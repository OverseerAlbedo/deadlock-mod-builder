//! "Silence this sound": a sound row without a file builds the original's whole
//! timeline as silence through the replacement encoder, and flows through the
//! receipt, snapshot verification and install like a replacement.
use deadlock_components::{audio, resources::{self, ResourceKind}, service::ComponentService};
use deadlock_core::{CancellationToken, TimingRecorder, process::{Runner, RunnerOptions}};
use deadlock_project::{Project, Settings, customization::ResourceReplacement};
use deadlock_resource::{binary, container, Kind};
use deadlock_vpk::{Vpk, resource_blocks};
use std::{fs, path::{Path, PathBuf}};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}
fn read(name: &str) -> Vec<u8> {
    fs::read(fixture(name)).unwrap()
}
fn data_offset(raw: &[u8]) -> usize {
    resource_blocks(raw).unwrap().blocks.iter().find(|b| b.name == "DATA").unwrap().offset
}
fn mp3_original() -> Vec<u8> {
    let mut out = read("sound-data.bin");
    let layout = resource_blocks(&out).unwrap();
    let at = data_offset(&out);
    let payload = read("extras/tone.mp3");
    out.truncate(layout.declared_size);
    out[at + 2] = 2;
    out[at + 8..at + 12].copy_from_slice(&1280u32.to_le_bytes());
    out[at + 12..at + 16].copy_from_slice(&0.16f32.to_le_bytes());
    out[at + 28..at + 32].copy_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend(payload);
    out
}
fn ctrl_mp3_original(loop_start: i64) -> Vec<u8> {
    let raw = read("sound-ctrl.bin");
    let layout = resource_blocks(&raw).unwrap();
    let block = layout.blocks.iter().find(|b| b.name == "CTRL").unwrap();
    let mut doc = binary::decode(&raw[block.offset..block.offset + block.size]).unwrap();
    let sound = doc.root.get_mut("m_vSound").unwrap();
    let payload = read("extras/tone.mp3");
    sound.get_mut("m_nFormat").unwrap().kind = Kind::String("MP3".into());
    sound.get_mut("m_nSampleCount").unwrap().kind = Kind::U64(1280);
    sound.get_mut("m_nStreamingSize").unwrap().kind = Kind::U64(payload.len() as u64);
    sound.get_mut("m_flDuration").unwrap().kind = Kind::F64(0.16);
    sound.get_mut("m_nLoopStart").unwrap().kind = Kind::I64(loop_start);
    let ctrl = binary::encode(&doc.root, doc.format_guid).unwrap();
    let mut original = container::rebuild(&raw[..layout.declared_size], &std::collections::BTreeMap::from([("CTRL".into(), ctrl)])).unwrap();
    original.extend(payload);
    original
}
fn eight_bit() -> Vec<u8> {
    let mut eight = read("sound-data.bin");
    let layout = resource_blocks(&eight).unwrap();
    let at = data_offset(&eight);
    eight[at + 2] = 1;
    eight[at + 28..at + 32].copy_from_slice(&8u32.to_le_bytes());
    eight.truncate(layout.declared_size);
    eight.extend([0, 64, 100, 128, 150, 180, 200, 255]);
    eight
}
/// The same sound with a loop from frame 2 (legacy DATA layout).
fn looping() -> Vec<u8> {
    let mut raw = read("sound-data.bin");
    let at = data_offset(&raw);
    raw[at + 4..at + 8].copy_from_slice(&2i32.to_le_bytes());
    raw
}

#[test]
fn silence_keeps_the_original_timeline_loops_and_voice_metadata() {
    let cancel = CancellationToken::new();
    let cases = [
        ("PCM16 DATA", read("sound-data.bin"), "PCM16"),
        ("PCM16 CTRL", read("sound-ctrl.bin"), "PCM16"),
        ("PCM8", eight_bit(), "PCM8"),
        ("looping PCM16", looping(), "PCM16"),
        ("MP3 DATA", mp3_original(), "PCM16"),
        ("MP3 CTRL", ctrl_mp3_original(-1), "PCM16"),
        ("looping MP3 CTRL", ctrl_mp3_original(0), "PCM16"),
    ];
    for (name, original, format) in cases {
        let copy = original.clone();
        let before = audio::metadata(&original).unwrap();
        let silent = audio::silent_audio(&original, &cancel).unwrap();
        let after = audio::metadata(&silent.resource).unwrap();
        assert_eq!(after.format, format, "{name}");
        assert_eq!(
            (after.rate, after.channels, after.frames, after.loop_start, after.loop_end, after.kind.as_str(), after.resource_version),
            (before.rate, before.channels, before.frames, before.loop_start, before.loop_end, before.kind.as_str(), before.resource_version),
            "{name}"
        );
        assert_eq!(after.duration, before.duration, "{name}");
        audio::validate_rendered_sound(&original, &silent.resource).unwrap();
        assert!(audio::is_silent(&silent.resource).unwrap(), "{name}");
        // It plays as silence of the original's length.
        let wave = audio::playable_wave(&silent.resource).unwrap();
        assert_eq!(wave.len() - 44, before.frames as usize * before.channels as usize * (if format == "PCM8" { 1 } else { 2 }), "{name}");
        assert!(wave[44..].iter().all(|v| *v == if format == "PCM8" { 128 } else { 0 }), "{name}");
        assert_eq!(silent.report.policy, audio::SILENCE_POLICY);
        assert_eq!(original, copy, "{name}: the original is never changed");
        // The same samples a 0% replacement gives.
        if name == "PCM16 DATA" {
            let original_wave = audio::playable_wave(&original).unwrap();
            let muted = audio::prepare_audio(&original, &original_wave, "wav", 0, "exact", &cancel).unwrap();
            assert_eq!(audio::playable_wave(&muted.resource).unwrap(), wave);
        }
    }
    // A PCM original keeps every byte before its samples.
    let original = read("sound-data.bin");
    let meta = audio::metadata(&original).unwrap();
    let silent = audio::silent_audio(&original, &cancel).unwrap().resource;
    assert_eq!(&silent[..meta.payload_offset], &original[..meta.payload_offset]);
    let mut noisy = silent.clone();
    *noisy.last_mut().unwrap() = 1;
    assert!(!audio::is_silent(&noisy).unwrap());
}

#[test]
fn a_procedural_or_broken_sound_cannot_be_silenced_either() {
    let cancel = CancellationToken::new();
    let mut truncated = read("sound-data.bin");
    truncated.pop();
    assert!(audio::silent_audio(&truncated, &cancel).is_err());
    // Legacy MP3 with codec settings: refused like a converted replacement.
    let mut legacy = mp3_original();
    let at = data_offset(&legacy);
    legacy[at + 24..at + 28].copy_from_slice(&4i32.to_le_bytes());
    assert!(audio::silent_audio(&legacy, &cancel).is_err());
}

fn quiet(_: &str) {}
struct Fixture {
    dir: tempfile::TempDir,
    settings: Settings,
    cancel: CancellationToken,
    clock: TimingRecorder,
    runner: Runner,
}
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let cancel = CancellationToken::new();
        let settings = Settings {
            workspace: dir.path().join("workspace").to_string_lossy().into(),
            pak: fixture("components_dir.vpk").to_string_lossy().into(),
            ..Default::default()
        };
        let runner = Runner::new(dir.path().join("logs"), cancel.clone(), RunnerOptions::default()).unwrap();
        Self { dir, settings, cancel, clock: TimingRecorder::new("silence"), runner }
    }
    fn service(&self) -> ComponentService<'_> {
        ComponentService { settings: &self.settings, runner: &self.runner, cancel: &self.cancel, clock: &self.clock, log: &quiet }
    }
}
fn project(rows: Vec<ResourceReplacement>) -> Project {
    let mut project = Project::default();
    project.original_model = "models/heroes_staging/hornet_v3/hornet.vmdl".into();
    project.customization.enabled = Some(true);
    project.customization.model_enabled = Some(false);
    project.customization.audio = Some(rows);
    project
}
const RIFLE: &str = "sounds/heroes/hornet/rifle.vsnd_c";
fn silenced(target: &str) -> ResourceReplacement {
    ResourceReplacement { enabled: Some(true), target: Some(target.into()), allow_shared: Some(false), silence: Some(true), ..Default::default() }
}

#[test]
fn a_silenced_row_builds_verifies_and_installs_like_a_replacement() {
    let fx = Fixture::new();
    let service = fx.service();
    let mut project = project(vec![silenced(RIFLE)]);
    let inspection = service.inspect(&project).unwrap();
    assert_eq!(inspection.audio.len(), 1);
    let output = fx.dir.path().join("game");
    let receipt = service.compile(&project, &output).unwrap().unwrap();
    let built = fs::read(output.join(RIFLE)).unwrap();
    assert!(audio::is_silent(&built).unwrap());
    let original = Vpk::open(&fx.settings.pak).unwrap().read(RIFLE).unwrap();
    audio::validate_rendered_sound(&original, &built).unwrap();
    let entry = &receipt.audio.entries[0];
    assert!(entry.silenced);
    assert_eq!(entry.method, "silence");
    assert_eq!(entry.source, PathBuf::new());
    assert!(entry.source_sha256.is_empty());
    let json = serde_json::to_value(entry).unwrap();
    assert_eq!(json["silenced"], true);
    assert_eq!(json["audio_edit"]["policy"], audio::SILENCE_POLICY);
    assert_eq!(receipt.files.len(), 1);
    service.verify_snapshot(&project, &receipt, &output).unwrap();
    let package = fx.dir.path().join("pak20_dir.vpk");
    deadlock_vpk::write_vpk(&output, &receipt.files.keys().cloned().collect::<Vec<_>>(), &package, Some(&receipt.files), 2, &mut || Ok(())).unwrap();
    let candidate = Vpk::open(&package).unwrap();
    let current = Vpk::open(&fx.settings.pak).unwrap();
    service.verify_install(&project, &receipt, &candidate, &current).unwrap();

    // Switched to a file for the same sound: the build is stale.
    let wave = fx.dir.path().join("voice.wav");
    fs::write(&wave, audio::playable_wave(&original).unwrap()).unwrap();
    project.customization.audio = Some(vec![ResourceReplacement {
        enabled: Some(true), source: Some(wave.to_string_lossy().into()), target: Some(RIFLE.into()), ..Default::default()
    }]);
    let stale = service.verify_snapshot(&project, &receipt, &output).unwrap_err().to_string();
    assert!(stale.contains("silenced or given a file"), "{stale}");
    assert!(service.verify_install(&project, &receipt, &candidate, &current).is_err());
    // And the other way round.
    let replaced = service.compile(&project, &fx.dir.path().join("game2")).unwrap().unwrap();
    assert!(!replaced.audio.entries[0].silenced);
    assert!(serde_json::to_value(&replaced.audio.entries[0]).unwrap().get("silenced").is_none(), "replacement receipts are unchanged");
    project.customization.audio = Some(vec![silenced(RIFLE)]);
    assert!(service.verify_snapshot(&project, &replaced, &fx.dir.path().join("game2")).is_err());

    // A silenced output that was changed afterwards is refused.
    project.customization.audio = Some(vec![silenced(RIFLE)]);
    let mut tampered = receipt.clone();
    let mut noisy = built.clone();
    *noisy.last_mut().unwrap() = 7;
    fs::write(output.join(RIFLE), &noisy).unwrap();
    tampered.audio.entries[0].output_sha256 = deadlock_components::resource_fingerprint(&noisy);
    let refused = resources::verify_compiled(&current, &tampered.audio, &output, &fx.cancel).unwrap_err().to_string();
    assert!(refused.contains("no longer silent"), "{refused}");
}

#[test]
fn silence_follows_the_shared_sound_rule_and_an_empty_row_still_needs_a_file() {
    let archive = Vpk::open(fixture("components_dir.vpk")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let cancel = CancellationToken::new();
    let hero = "models/heroes_staging/hornet_v3/hornet.vmdl";
    let shared = silenced("sounds/shared/gun.vsnd_c");
    let refused = resources::stage(&archive, hero, &[shared.clone()], dir.path(), ResourceKind::Audio, &cancel).unwrap_err().to_string();
    assert!(refused.contains("shared-asset choice"), "{refused}");
    let allowed = ResourceReplacement { allow_shared: Some(true), ..shared };
    let receipt = resources::stage(&archive, hero, &[allowed], dir.path(), ResourceKind::Audio, &cancel).unwrap();
    assert!(receipt.entries[0].silenced && receipt.entries[0].allow_shared);
    // A row without a file that is not silenced keeps the old message.
    let fx = Fixture::new();
    let empty = project(vec![ResourceReplacement { enabled: Some(true), target: Some(RIFLE.into()), ..Default::default() }]);
    let message = fx.service().inspect(&empty).unwrap_err().to_string();
    assert!(message.contains("Add a source file for each enabled character replacement"), "{message}");
    // Silence on an effect row is refused by the saved format.
    let mut effect = project(vec![]);
    effect.customization.effects = Some(vec![ResourceReplacement { target: Some("particles/heroes/hornet/trail.vpcf_c".into()), silence: Some(true), ..Default::default() }]);
    assert!(fx.service().inspect(&effect).unwrap_err().to_string().contains("Only sounds can be silenced"));
}
