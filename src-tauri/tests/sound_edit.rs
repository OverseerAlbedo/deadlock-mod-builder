//! The sound editor's preview renders exactly what the build writes, without
//! the operation lease, for the character's loaded Extras catalog only.
use deadlock_components::audio;
use deadlock_core::{safety, CancellationToken};
use deadlock_desktop::{app::Application, sound_edit::SoundEditRequest};
use deadlock_project::{customization::ResourceReplacement, Project, Settings};
use deadlock_vpk::resource_blocks;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("crates/deadlock-components/tests/fixtures")
}
fn wav(rate: u32, values: &[i16]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend(b"RIFF");
    out.extend((36 + values.len() as u32 * 2).to_le_bytes());
    out.extend(b"WAVEfmt ");
    out.extend(16u32.to_le_bytes());
    out.extend(1u16.to_le_bytes());
    out.extend(1u16.to_le_bytes());
    out.extend(rate.to_le_bytes());
    out.extend((rate * 2).to_le_bytes());
    out.extend(2u16.to_le_bytes());
    out.extend(16u16.to_le_bytes());
    out.extend(b"data");
    out.extend((values.len() as u32 * 2).to_le_bytes());
    for v in values {
        out.extend(v.to_le_bytes());
    }
    out
}
/// The fixture's PCM16 DATA sound made 800 frames long (0.1 s at 8 kHz).
fn longer_sound() -> Vec<u8> {
    let mut raw = std::fs::read(root().join("sound-data.bin")).unwrap();
    let layout = resource_blocks(&raw).unwrap();
    let at = layout.blocks.iter().find(|b| b.name == "DATA").unwrap().offset;
    raw.truncate(layout.declared_size);
    raw[at + 8..at + 12].copy_from_slice(&800u32.to_le_bytes());
    raw[at + 12..at + 16].copy_from_slice(&0.1f32.to_le_bytes());
    raw[at + 28..at + 32].copy_from_slice(&1600u32.to_le_bytes());
    for i in 0..800 {
        raw.extend(((((i as f64) * 0.2).sin() * 8000.) as i16).to_le_bytes());
    }
    audio::metadata(&raw).unwrap();
    raw
}
fn plain_row(row: &ResourceReplacement) -> ResourceReplacement {
    ResourceReplacement { start_seconds: None, volume_db: None, fade_in_seconds: None, fade_out_seconds: None, ..row.clone() }
}
fn unpack(raw: Vec<u8>) -> (serde_json::Value, Vec<u8>) {
    let len = u32::from_le_bytes(raw[..4].try_into().unwrap()) as usize;
    (serde_json::from_slice(&raw[4..4 + len]).unwrap(), raw[4 + len..].to_vec())
}

#[test]
fn the_preview_is_the_build_s_own_render_and_takes_no_lease() {
    let temp = tempfile::tempdir().unwrap();
    let app = Application::new(temp.path().join("state")).unwrap();
    let tree = temp.path().join("tree");
    let hero = "models/heroes_staging/hornet_v3/hornet.vmdl_c";
    let sound = "sounds/heroes/hornet/voice.vsnd_c";
    let original = longer_sound();
    let model = deadlock_vpk::Vpk::open(root().join("components_dir.vpk")).unwrap().read(hero).unwrap();
    safety::atomic_write(&tree.join(hero), &model).unwrap();
    safety::atomic_write(&tree.join(sound), &original).unwrap();
    let pak = temp.path().join("pak18_dir.vpk");
    deadlock_vpk::write_vpk(&tree, &[hero.into(), sound.into()], &pak, None, 2, &mut || Ok(())).unwrap();
    app.save_settings(&Settings { pak: pak.display().to_string(), ..Default::default() }).unwrap();
    let mut project = Project::default();
    project.original_model = hero.into();
    app.discover_extras(project).unwrap();
    let file = temp.path().join("long.wav");
    let samples: Vec<i16> = (0..4000).map(|i| (((i as f64) * 0.05).sin() * 12000.) as i16).collect();
    std::fs::write(&file, wav(8000, &samples)).unwrap();
    let before = serde_json::to_value(app.get_timings().unwrap()).unwrap();
    let row = ResourceReplacement {
        enabled: Some(true), source: Some(file.display().to_string()), target: Some(sound.into()),
        volume_percent: Some(100), audio_fit: Some("pad".into()),
        start_seconds: Some(0.2), volume_db: Some(-3.), fade_in_seconds: Some(0.01), fade_out_seconds: Some(0.02),
        ..Default::default()
    };
    let request = |row: ResourceReplacement| SoundEditRequest { hero_path: hero.into(), row, buckets: Some(64) };
    let (head, wave) = unpack(app.sound_edit_preview(request(row.clone())).unwrap());
    assert_eq!(head["mime_type"], "audio/wav");
    let details = &head["details"];
    assert_eq!(details["edited"], true);
    assert_eq!(details["source"]["peaks"].as_array().unwrap().len(), 64);
    assert!((details["source"]["duration"].as_f64().unwrap() - 0.5).abs() < 1e-9);
    assert_eq!(details["game"]["frames"], 800);
    assert_eq!(details["game"]["loop_start"], -1);
    assert_eq!(details["result"]["report"]["window"]["start_frame"], 1600);
    assert!(details["refusal"].is_null());
    assert!(details["matched"]["volume_db"].is_number());
    // Exactly the build's output, as a WAV.
    let cancel = CancellationToken::new();
    let built = audio::prepare_edited_audio(&original, &std::fs::read(&file).unwrap(), "wav", 100, &row.sound_edit().unwrap(), &cancel).unwrap();
    assert_eq!(wave, audio::preview_audio(&built.resource, &cancel).unwrap());
    // Without an edit, "pad" refuses the longer file as the build would: a
    // refusal with the file's waveform, and nothing to play.
    let plain = ResourceReplacement { start_seconds: None, volume_db: None, fade_in_seconds: None, fade_out_seconds: None, ..row.clone() };
    let (head, wave) = unpack(app.sound_edit_preview(request(plain.clone())).unwrap());
    assert!(wave.is_empty());
    assert!(head["details"]["refusal"].as_str().unwrap().contains("Trim longer audio"), "{}", head["details"]["refusal"]);
    assert_eq!(head["details"]["edited"], false);
    assert_eq!(head["details"]["source"]["peaks"].as_array().unwrap().len(), 64);
    // "trim" plays the first part; the preview is the old encoder's output.
    let (_, wave) = unpack(app.sound_edit_preview(request(ResourceReplacement { audio_fit: Some("trim".into()), ..plain })).unwrap());
    let old = audio::prepare_audio(&original, &std::fs::read(&file).unwrap(), "wav", 100, "trim", &cancel).unwrap();
    assert_eq!(wave, audio::preview_audio(&old.resource, &cancel).unwrap());
    // A start past the end of the file is a refusal, not an error.
    let (head, _) = unpack(app.sound_edit_preview(request(ResourceReplacement { start_seconds: Some(0.6), ..row.clone() })).unwrap());
    assert!(head["details"]["refusal"].as_str().unwrap().contains("past the end"));
    // A file too long for the old encoder (3 min of 48 kHz stereo, past 16
    // million samples): without an edit the page is led to the editor; with one it plays.
    let song = temp.path().join("song.wav");
    let frames = 180 * 48000usize;
    let mut stereo = Vec::with_capacity(frames * 2);
    for i in 0..frames { let v = (((i as f64) * 0.01).sin() * 9000.) as i16; stereo.push(v); stereo.push(v); }
    let mut long = wav(48000, &stereo);
    long[22..24].copy_from_slice(&2u16.to_le_bytes());
    long[28..32].copy_from_slice(&(48000u32 * 4).to_le_bytes());
    long[32..34].copy_from_slice(&4u16.to_le_bytes());
    std::fs::write(&song, long).unwrap();
    let long_row = ResourceReplacement { source: Some(song.display().to_string()), ..plain_row(&row) };
    let (head, wave) = unpack(app.sound_edit_preview(request(long_row.clone())).unwrap());
    assert!(wave.is_empty());
    assert_eq!(head["details"]["refusal"], deadlock_desktop::sound_edit::LONG_FILE);
    assert!((head["details"]["source"]["duration"].as_f64().unwrap() - 180.).abs() < 1e-9);
    let (head, wave) = unpack(app.sound_edit_preview(request(ResourceReplacement { start_seconds: Some(120.), ..long_row })).unwrap());
    assert!(head["details"]["refusal"].is_null() && !wave.is_empty());
    assert_eq!(head["details"]["result"]["report"]["window"]["start_frame"], 960000);
    // Read-only and lease-free; the archive is untouched.
    assert!(!app.is_busy());
    assert_eq!(serde_json::to_value(app.get_timings().unwrap()).unwrap(), before);
    // Refused: values out of range, a silenced row, another hero.
    assert!(app.sound_edit_preview(request(ResourceReplacement { volume_db: Some(40.), ..row.clone() })).is_err());
    assert!(app.sound_edit_preview(request(ResourceReplacement { source: None, silence: Some(true), ..row.clone() })).is_err());
    let mut other = request(row);
    other.hero_path = "models/heroes_wip/wraith/wraith.vmdl".into();
    assert!(app.sound_edit_preview(other).is_err());
}
