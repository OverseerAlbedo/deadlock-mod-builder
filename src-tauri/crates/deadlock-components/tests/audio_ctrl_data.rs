//! Version-5 sounds have a zero-length DATA placeholder, metadata in CTRL,
//! and streaming samples after the declared resource size. Exercise the full
//! editing path, including a DATA placeholder that precedes CTRL.
use deadlock_components::{audio, resources::{self, ResourceKind}, service::ComponentService};
use deadlock_core::{CancellationToken, TimingRecorder, process::{Runner, RunnerOptions}};
use deadlock_project::{Project, Settings, customization::ResourceReplacement};
use deadlock_resource::{Kind, binary, container};
use deadlock_vpk::{Vpk, resource_blocks};
use std::{collections::BTreeMap, fs, path::{Path, PathBuf}};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}
fn read(name: &str) -> Vec<u8> { fs::read(fixture(name)).unwrap() }
fn wav(rate: u32, channels: u16, samples: &[i16]) -> Vec<u8> {
    let mut out = b"RIFF".to_vec();
    out.extend((36 + samples.len() as u32 * 2).to_le_bytes());
    out.extend(b"WAVEfmt "); out.extend(16u32.to_le_bytes());
    out.extend(1u16.to_le_bytes()); out.extend(channels.to_le_bytes());
    out.extend(rate.to_le_bytes()); out.extend((rate * channels as u32 * 2).to_le_bytes());
    out.extend((channels * 2).to_le_bytes()); out.extend(16u16.to_le_bytes());
    out.extend(b"data"); out.extend((samples.len() as u32 * 2).to_le_bytes());
    for sample in samples { out.extend(sample.to_le_bytes()); }
    out
}
fn samples(raw: &[u8]) -> Vec<i16> {
    audio::preview_audio(raw, &CancellationToken::new()).unwrap()[44..]
        .chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect()
}
fn rewrite_ctrl(raw: &[u8], edit: impl FnOnce(&mut deadlock_resource::Value)) -> Vec<u8> {
    let layout = resource_blocks(raw).unwrap();
    let block = layout.blocks.iter().find(|b| b.name == "CTRL").unwrap();
    let mut doc = binary::decode(&raw[block.offset..block.offset + block.size]).unwrap();
    edit(&mut doc.root);
    let body = binary::encode(&doc.root, doc.format_guid).unwrap();
    let mut out = container::rebuild(&raw[..layout.declared_size], &BTreeMap::from([("CTRL".into(), body)])).unwrap();
    out.extend_from_slice(&raw[layout.declared_size..]);
    out
}
fn add_data(raw: &[u8], data: &[u8], before_ctrl: bool) -> Vec<u8> {
    let layout = resource_blocks(raw).unwrap();
    let mut blocks: Vec<_> = layout.blocks.iter().map(|b|
        (b.name.clone(), raw[b.offset..b.offset + b.size].to_vec())).collect();
    let at = if before_ctrl { blocks.iter().position(|(name, _)| name == "CTRL").unwrap() } else { blocks.len() };
    blocks.insert(at, ("DATA".into(), data.to_vec()));
    let mut out = raw[..16].to_vec();
    out[6..8].copy_from_slice(&5u16.to_le_bytes());
    out[8..12].copy_from_slice(&8u32.to_le_bytes());
    out[12..16].copy_from_slice(&(blocks.len() as u32).to_le_bytes());
    out.resize(16 + 12 * blocks.len(), 0);
    for (i, (name, bytes)) in blocks.iter().enumerate() {
        out.resize(out.len().div_ceil(16) * 16, 0);
        let pos = 16 + i * 12;
        let offset = out.len() - (pos + 4);
        out[pos..pos + 4].copy_from_slice(name.as_bytes());
        out[pos + 4..pos + 8].copy_from_slice(&(offset as u32).to_le_bytes());
        out[pos + 8..pos + 12].copy_from_slice(&(bytes.len() as u32).to_le_bytes());
        out.extend(bytes);
    }
    let size = out.len() as u32;
    out[..4].copy_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&raw[layout.declared_size..]);
    out
}
fn original(format: &str, class: &str, before_ctrl: bool) -> Vec<u8> {
    let frames = if format == "MP3" { 1280 } else { 8 };
    let payload = match format {
        "MP3" => read("extras/tone.mp3"),
        "PCM8" => vec![128, 132, 124, 128, 140, 116, 128, 128],
        _ => [0i16, 1000, -1000, 0, 2000, -2000, 0, 0].iter().flat_map(|v| v.to_le_bytes()).collect(),
    };
    let mut raw = rewrite_ctrl(&read("sound-ctrl.bin"), |root| {
        root.get_mut("_class").unwrap().kind = Kind::String(class.into());
        let sound = root.get_mut("m_vSound").unwrap();
        sound.get_mut("m_nFormat").unwrap().kind = Kind::String(format.into());
        sound.get_mut("m_nSampleCount").unwrap().kind = Kind::U64(frames);
        sound.get_mut("m_nStreamingSize").unwrap().kind = Kind::U64(payload.len() as u64);
        sound.get_mut("m_flDuration").unwrap().kind = Kind::F64(frames as f64 / 8000.);
    });
    raw.truncate(resource_blocks(&raw).unwrap().declared_size);
    raw.extend(payload);
    add_data(&raw, &[], before_ctrl)
}
fn assert_placeholder(raw: &[u8]) {
    let layout = resource_blocks(raw).unwrap();
    let data = layout.blocks.iter().find(|b| b.name == "DATA").unwrap();
    assert_eq!(data.size, 0);
    assert_eq!(audio::metadata(raw).unwrap().payload_offset, layout.declared_size);
}
fn evidence(name: &str, bytes: &[u8]) {
    if let Ok(path) = std::env::var("DMB_SOUND_LAYOUT_EVIDENCE") {
        fs::create_dir_all(&path).unwrap();
        fs::write(Path::new(&path).join(name), bytes).unwrap();
    }
}

#[test]
fn ctrl_with_empty_data_supports_pcm_preview_replacement_and_gain() {
    let cancel = CancellationToken::new();
    for format in ["PCM8", "PCM16"] {
        for class in ["CVoiceContainerDefault", "CVoiceContainerEnvelope"] {
            for before in [true, false] {
                let raw = original(format, class, before);
                let copy = raw.clone();
                assert_placeholder(&raw);
                assert_eq!(samples(&raw).len(), 8);
                let source = wav(8000, 1, &[1024; 8]);
                for (gain, expected) in [(0, 0), (100, 1024), (200, 2048)] {
                    let edited = audio::prepare_audio(&raw, &source, "wav", gain, "pad", &cancel).unwrap();
                    assert_eq!(samples(&edited.resource), vec![expected; 8]);
                    assert_placeholder(&edited.resource);
                    audio::validate_sound(&raw, &edited.resource).unwrap();
                }
                // Strict, compatible replacement keeps the entire metadata prefix byte-exact.
                let exact = audio::playable_wave(&raw).unwrap();
                assert_eq!(audio::wav_replacement(&raw, &exact).unwrap(), raw);
                assert_eq!(raw, copy);
            }
        }
    }
}

#[test]
fn ctrl_with_empty_data_preserves_voice_metadata_during_mp3_to_pcm_conversion() {
    let cancel = CancellationToken::new();
    for before in [true, false] {
        let raw = original("MP3", "CVoiceContainerDefault", before);
        assert_eq!(samples(&raw).len(), 1280);
        let output = audio::prepare_audio(&raw, &wav(8000, 1, &[1000; 1280]), "wav", 175, "pad", &cancel).unwrap();
        assert_eq!(samples(&output.resource), vec![1750; 1280]);
        assert_placeholder(&output.resource);
        audio::validate_rendered_sound(&raw, &output.resource).unwrap();
        let changed_loop = rewrite_ctrl(&output.resource, |r|
            r.get_mut("m_vSound").unwrap().get_mut("m_nLoopStart").unwrap().kind = Kind::I64(0));
        assert!(audio::validate_rendered_sound(&raw, &changed_loop).is_err());
        if before {
            evidence("original-ctrl-data.vsnd_c", &raw);
            evidence("converted-ctrl-data.vsnd_c", &output.resource);
        }
    }
}

#[test]
fn mp3_seek_header_is_not_counted_as_audio_and_its_frame_count_is_checked() {
    let tone = read("extras/tone.mp3");
    // Fixture: five 216-byte MPEG-2.5 Layer-III mono frames at 8 kHz,
    // each with 576 samples. The side-info area is nine bytes long.
    assert_eq!(tone.len(), 5 * 216);
    assert_eq!(&tone[..4], &[0xff, 0xe3, 0x38, 0xc4]);
    for tag in [b"Xing", b"Info"] {
        let mut frame = vec![0; 216]; frame[..4].copy_from_slice(&tone[..4]);
        frame[13..17].copy_from_slice(tag);
        frame[17..21].copy_from_slice(&1u32.to_be_bytes());
        frame[21..25].copy_from_slice(&5u32.to_be_bytes());
        frame.extend(&tone);
        let mut raw = original("MP3", "CVoiceContainerDefault", true);
        raw = rewrite_ctrl(&raw, |root| {
            let sound = root.get_mut("m_vSound").unwrap();
            sound.get_mut("m_nSampleCount").unwrap().kind = Kind::U64(800);
            sound.get_mut("m_flDuration").unwrap().kind = Kind::F64(0.1);
            sound.get_mut("m_nStreamingSize").unwrap().kind = Kind::U64(frame.len() as u64);
        });
        let offset = resource_blocks(&raw).unwrap().declared_size;
        raw.truncate(offset); raw.extend(frame);
        let meta = audio::metadata(&raw).unwrap();
        assert_eq!(meta.mp3_frames, Some(6)); assert_eq!(meta.mp3_samples, Some(2880));
        assert_eq!(samples(&raw).len(), 800);
        let mut bad = raw.clone(); bad[offset + 21..offset + 25].copy_from_slice(&4u32.to_be_bytes());
        assert!(audio::metadata(&bad).unwrap_err().to_string().contains("frame count"));
        let mut bad = raw; bad[offset + 17..offset + 21].copy_from_slice(&16u32.to_be_bytes());
        assert!(audio::metadata(&bad).unwrap_err().to_string().contains("Xing/Info"));
    }
}

#[test]
fn ambiguous_nonempty_or_duplicate_data_and_procedural_sounds_remain_rejected() {
    let ctrl = read("sound-ctrl.bin");
    for before in [true, false] {
        let nonempty = add_data(&ctrl, &[0; 48], before);
        assert!(audio::metadata(&nonempty).unwrap_err().to_string().contains("non-empty DATA"));
        let valid = add_data(&ctrl, &[], before);
        let duplicate = add_data(&valid, &[], before);
        assert!(audio::metadata(&duplicate).unwrap_err().to_string().contains("duplicate"));
        let procedural = rewrite_ctrl(&valid, |root|
            root.get_mut("_class").unwrap().kind = Kind::String("CVoiceContainerRandomSampler".into()));
        assert!(audio::metadata(&procedural).unwrap_err().to_string().contains("procedural"));
        let mut short = valid.clone(); short.pop();
        assert!(audio::metadata(&short).is_err());
        let mut long = valid; long.push(0);
        assert!(audio::metadata(&long).is_err());
    }
}

#[test]
fn ctrl_data_catalog_build_and_vpk_install_verification_use_the_same_support() {
    let dir = tempfile::tempdir().unwrap();
    let target = "sounds/heroes/hornet/rifle.vsnd_c";
    let raw = original("PCM16", "CVoiceContainerDefault", true);
    let base = Vpk::open(fixture("components_dir.vpk")).unwrap();
    let inputs = dir.path().join("inputs");
    let mut names = Vec::new();
    for entry in base.entries.values() {
        let name = &entry.path;
        let bytes = if name == target { raw.clone() } else { base.read(name).unwrap() };
        let path = inputs.join(name); fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap(); names.push(name.clone());
    }
    let pak = dir.path().join("base_dir.vpk");
    deadlock_vpk::write_vpk(&inputs, &names, &pak, None, 2, &mut || Ok(())).unwrap();
    let cancel = CancellationToken::new();
    let archive = Vpk::open(&pak).unwrap();
    let hero = "models/heroes_staging/hornet_v3/hornet.vmdl";
    let catalog = resources::discover(&archive, hero, ResourceKind::Audio, &cancel).unwrap();
    let row = catalog.iter().find(|r| r.target == target).unwrap();
    assert!(row.supported && row.wav_supported, "{}", row.reason);
    assert_eq!(samples(&archive.read(target).unwrap()).len(), 8);
    let settings = Settings { workspace: dir.path().join("workspace").to_string_lossy().into(), pak: pak.to_string_lossy().into(), ..Default::default() };
    let runner = Runner::new(dir.path().join("logs"), cancel.clone(), RunnerOptions::default()).unwrap();
    let clock = TimingRecorder::new("CTRL DATA audio"); let log = |_: &str| {};
    let service = ComponentService { settings: &settings, runner: &runner, cancel: &cancel, clock: &clock, log: &log };
    let mut project = Project::default(); project.original_model = hero.into();
    project.customization.enabled = Some(true); project.customization.model_enabled = Some(false);
    let source = dir.path().join("replacement.wav"); let source_bytes = wav(8000, 1, &[1000; 8]);
    fs::write(&source, &source_bytes).unwrap();
    project.customization.audio = Some(vec![ResourceReplacement { source: Some(source.to_string_lossy().into()), target: Some(target.into()), volume_percent: Some(175), audio_fit: Some("pad".into()), ..Default::default() }]);
    let output = dir.path().join("game"); let receipt = service.compile(&project, &output).unwrap().unwrap();
    service.verify_snapshot(&project, &receipt, &output).unwrap();
    let package = dir.path().join("pak19_dir.vpk");
    deadlock_vpk::write_vpk(&output, &receipt.files.keys().cloned().collect::<Vec<_>>(), &package, Some(&receipt.files), 2, &mut || Ok(())).unwrap();
    let candidate = Vpk::open(package).unwrap();
    service.verify_install(&project, &receipt, &candidate, &archive).unwrap();
    assert_eq!(samples(&candidate.read(target).unwrap()), vec![1750; 8]);
    assert_placeholder(&candidate.read(target).unwrap());
    assert_eq!(archive.read(target).unwrap(), raw);
    assert_eq!(fs::read(source).unwrap(), source_bytes);
}

#[test]
#[ignore = "independent upstream files; set DMB_SOUND_LAYOUT_REAL_FIXTURES to the pinned VRF fixture directory"]
fn upstream_version_five_sound_can_be_previewed_and_replaced() {
    let path = PathBuf::from(std::env::var("DMB_SOUND_LAYOUT_REAL_FIXTURES").unwrap());
    let original = fs::read(path.join("impulse.vsnd_c")).unwrap();
    let meta = audio::metadata(&original).unwrap();
    println!("Upstream impulse metadata: {}", serde_json::to_string(&meta).unwrap());
    assert_eq!(meta.resource_version, 5); assert_placeholder(&original);
    assert_eq!(samples(&original).len(), meta.frames as usize * meta.channels as usize);
    let replacement = wav(meta.rate, meta.channels as u16, &vec![1000; meta.frames as usize * meta.channels as usize]);
    let output = audio::prepare_audio(&original, &replacement, "wav", 175, "pad", &CancellationToken::new()).unwrap();
    audio::validate_rendered_sound(&original, &output.resource).unwrap();
    assert_eq!(samples(&output.resource), vec![1750; meta.frames as usize * meta.channels as usize]);
    assert_placeholder(&output.resource);
    evidence("upstream-impulse-converted.vsnd_c", &output.resource);
    let procedural = fs::read(path.join("random_switch_container.vsnd_c")).unwrap();
    assert!(audio::metadata(&procedural).unwrap_err().to_string().contains("procedural"));
}
