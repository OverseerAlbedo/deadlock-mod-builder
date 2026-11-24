//! Sound edits (the part of the user's file that plays, volume in dB, fades)
//! and the proof that a sound replacement without an edit builds exactly as
//! before: the digests below were taken from the encoder before sound edits
//! existed.
use deadlock_components::{audio, resource_fingerprint, resources::{self, ResourceKind}};
use deadlock_core::CancellationToken;
use deadlock_project::customization::ResourceReplacement;
use deadlock_resource::{binary, container, Kind};
use deadlock_vpk::{Vpk, resource_blocks};
use std::{fs, path::{Path, PathBuf}};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}
fn read(name: &str) -> Vec<u8> {
    fs::read(fixture(name)).unwrap()
}
fn wav(rate: u32, channels: u16, values: &[i16]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend(b"RIFF");
    out.extend((36 + values.len() as u32 * 2).to_le_bytes());
    out.extend(b"WAVEfmt ");
    out.extend(16u32.to_le_bytes());
    out.extend(1u16.to_le_bytes());
    out.extend(channels.to_le_bytes());
    out.extend(rate.to_le_bytes());
    out.extend((rate * channels as u32 * 2).to_le_bytes());
    out.extend((channels * 2).to_le_bytes());
    out.extend(16u16.to_le_bytes());
    out.extend(b"data");
    out.extend((values.len() as u32 * 2).to_le_bytes());
    for v in values {
        out.extend(v.to_le_bytes());
    }
    out
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
/// A PCM16 CTRL sound of `frames` frames at 8 kHz mono, looping from `loop_start` (-1: no loop).
fn ctrl_pcm(frames: u32, loop_start: i64, value: i16) -> Vec<u8> {
    let raw = read("sound-ctrl.bin");
    let layout = resource_blocks(&raw).unwrap();
    let block = layout.blocks.iter().find(|b| b.name == "CTRL").unwrap();
    let mut doc = binary::decode(&raw[block.offset..block.offset + block.size]).unwrap();
    let sound = doc.root.get_mut("m_vSound").unwrap();
    let rate = match sound.get("m_nRate").unwrap().kind { Kind::U64(v) => v as u32, Kind::I64(v) => v as u32, _ => 8000 };
    let channels = match sound.get("m_nChannels").unwrap().kind { Kind::U64(v) => v as u32, Kind::I64(v) => v as u32, _ => 1 };
    let payload: Vec<u8> = (0..frames * channels).flat_map(|_| value.to_le_bytes()).collect();
    sound.get_mut("m_nSampleCount").unwrap().kind = Kind::U64(frames as u64);
    sound.get_mut("m_nStreamingSize").unwrap().kind = Kind::U64(payload.len() as u64);
    sound.get_mut("m_flDuration").unwrap().kind = Kind::F64(frames as f64 / rate as f64);
    sound.get_mut("m_nLoopStart").unwrap().kind = Kind::I64(loop_start);
    let ctrl = binary::encode(&doc.root, doc.format_guid).unwrap();
    let mut original = container::rebuild(&raw[..layout.declared_size], &std::collections::BTreeMap::from([("CTRL".into(), ctrl)])).unwrap();
    original.extend(payload);
    original
}
const HERO: &str = "models/heroes_staging/hornet_v3/hornet.vmdl";
const RIFLE: &str = "sounds/heroes/hornet/rifle.vsnd_c";

/// Every unedited case of the replacement encoder, as one digest: outputs and
/// reports of `prepare_audio`, and the staged files and receipts (without the
/// temporary source path).
fn unedited_digest() -> String {
    let cancel = CancellationToken::new();
    let mut parts: Vec<String> = vec![];
    let short = wav(8000, 1, &[1000, -2000, 3000]);
    let long = wav(8000, 1, &(0..40).map(|i| (i * 500 - 10000) as i16).collect::<Vec<_>>());
    let stereo = wav(16000, 2, &(0..64).map(|i| ((i * 977) % 20000 - 10000) as i16).collect::<Vec<_>>());
    for (name, original) in [("data", read("sound-data.bin")), ("ctrl", read("sound-ctrl.bin")), ("mp3", mp3_original())] {
        for (source, ext) in [(&short, "wav"), (&long, "wav"), (&stereo, "wav"), (&read("extras/tone.mp3"), "mp3"), (&read("extras/tone.flac"), "flac")] {
            for volume in [0u16, 100, 150, 200] {
                for timing in ["exact", "pad", "trim"] {
                    let text = match audio::prepare_audio(&original, source, ext, volume, timing, &cancel) {
                        Ok(out) => format!("{}|{}", resource_fingerprint(&out.resource), serde_json::to_string(&out.report).unwrap()),
                        Err(e) => format!("error: {e}"),
                    };
                    parts.push(format!("{name}/{ext}/{volume}/{timing}: {text}"));
                }
            }
        }
    }
    let archive = Vpk::open(fixture("components_dir.vpk")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("voice.wav");
    fs::write(&source, &long).unwrap();
    let quiet = dir.path().join("short.wav");
    fs::write(&quiet, &short).unwrap();
    for (case, (file, volume, fit)) in [(&source, None, None), (&source, Some(100), Some("pad")), (&source, Some(150), Some("trim")), (&source, Some(40), Some("trim")),
        (&quiet, Some(100), Some("pad")), (&quiet, Some(170), Some("pad"))].into_iter().enumerate() {
        let source = file;
        let row = ResourceReplacement {
            enabled: Some(true), source: Some(source.to_string_lossy().into()), target: Some(RIFLE.into()),
            allow_shared: Some(false), volume_percent: volume, audio_fit: fit.map(String::from), ..Default::default()
        };
        let game = dir.path().join(format!("game-{case}"));
        let text = match resources::stage(&archive, HERO, &[row], &game, ResourceKind::Audio, &cancel) {
            Ok(receipt) => {
                let mut json = serde_json::to_value(&receipt).unwrap();
                json["entries"][0]["source"] = serde_json::Value::Null;
                format!("{}|{}", resource_fingerprint(&fs::read(game.join(RIFLE)).unwrap()), json)
            }
            Err(e) => format!("error: {e}"),
        };
        parts.push(format!("stage {case} {volume:?} {fit:?}: {text}"));
    }
    if std::env::var_os("DMB_PRINT_BASELINE").is_some() { eprintln!("{}", parts.join("\n")); }
    resource_fingerprint(parts.join("\n").as_bytes())
}

#[test]
fn a_replacement_without_an_edit_builds_byte_for_byte_as_before() {
    let digest = unedited_digest();
    // Taken from the encoder before sound edits existed
    // (180 `prepare_audio` cases, 100 of them refusals, and six staged rows).
    assert_eq!(digest, "7be4e43663bec68f5460a7ee7596d01850ed9a3b6ce4b7304cf07dd4bf91127d", "unedited replacement output changed");
}

use deadlock_project::customization::SoundEdit;
use deadlock_components::service::ComponentService;
use deadlock_core::{TimingRecorder, process::{Runner, RunnerOptions}};
use deadlock_project::{Project, Settings};

fn samples16(raw: &[u8]) -> Vec<i16> {
    let wave = audio::playable_wave(raw).unwrap();
    wave[44..].chunks_exact(2).map(|v| i16::from_le_bytes([v[0], v[1]])).collect()
}
fn edit(start: f64, db: f64, fade_in: f64, fade_out: f64) -> SoundEdit {
    SoundEdit { start_seconds: start, volume_db: db, fade_in_seconds: fade_in, fade_out_seconds: fade_out }
}
fn render(original: &[u8], source: &[u8], percent: u16, e: SoundEdit) -> audio::Rendered {
    let cancel = CancellationToken::new();
    let decoded = audio::decode_source(source, "wav", &cancel).unwrap();
    audio::render_edit(original, &decoded, percent, &e, &cancel).unwrap()
}

#[test]
fn the_frame_takes_the_part_from_its_start_and_silence_fills_the_rest() {
    let original = ctrl_pcm(100, -1, 0);
    let meta = audio::metadata(&original).unwrap();
    assert_eq!((meta.channels, meta.rate), (1, 8000), "the fixture is mono 8 kHz");
    let ramp: Vec<i16> = (0..300).map(|i| (i * 10) as i16).collect();
    let source = wav(8000, 1, &ramp);
    // From frame 40: 100 frames of the file.
    let out = render(&original, &source, 100, edit(40. / 8000., 0., 0., 0.));
    assert_eq!(samples16(&out.audio.resource), ramp[40..140].to_vec());
    let window = out.audio.report.window.clone().unwrap();
    assert_eq!((window.start_frame, window.file_frames, out.audio.report.padded_frames), (40, 100, 0));
    assert_eq!(window.policy, audio::WINDOW_POLICY);
    // From frame 250 only 50 frames are left: silence fills the other 50.
    let out = render(&original, &source, 100, edit(250. / 8000., 0., 0., 0.));
    let mut expected = ramp[250..].to_vec();
    expected.resize(100, 0);
    assert_eq!(samples16(&out.audio.resource), expected);
    assert_eq!(out.audio.report.padded_frames, 50);
    // A shorter file than the game's sound: all of it, then silence.
    let out = render(&original, &wav(8000, 1, &ramp[..30]), 100, edit(0., 0., 0., 0.));
    let mut expected = ramp[..30].to_vec();
    expected.resize(100, 0);
    assert_eq!(samples16(&out.audio.resource), expected);
    // The game's contract is kept.
    audio::validate_rendered_sound(&original, &out.audio.resource).unwrap();
    let after = audio::metadata(&out.audio.resource).unwrap();
    assert_eq!((after.frames, after.rate, after.channels, after.loop_start, after.duration), (meta.frames, meta.rate, meta.channels, meta.loop_start, meta.duration));
    // Past the end of the file: refused, not silence.
    let cancel = CancellationToken::new();
    let decoded = audio::decode_source(&source, "wav", &cancel).unwrap();
    let late = audio::render_edit(&original, &decoded, 100, &edit(300. / 8000., 0., 0., 0.), &cancel).err().unwrap().to_string();
    assert!(late.contains("past the end of your file"), "{late}");
    // Out of the saved format's range: refused.
    for bad in [edit(-1., 0., 0., 0.), edit(0., 13., 0., 0.), edit(0., f64::NAN, 0., 0.), edit(0., 0., 61., 0.), edit(0., 0., 0., f64::INFINITY)] {
        assert!(audio::render_edit(&original, &decoded, 100, &bad, &cancel).is_err(), "{bad:?}");
    }
    assert!(audio::render_edit(&original, &decoded, 201, &edit(0., 0., 0., 0.), &cancel).is_err());
}

#[test]
fn the_window_is_the_old_conversion_cut_at_the_frame() {
    // Resampled and down-mixed: start 0 equals the old "trim" path byte for byte.
    let original = ctrl_pcm(400, -1, 0);
    let stereo = wav(22050, 2, &(0..4000).map(|i| (((i as f64) * 0.05).sin() * 9000.) as i16).collect::<Vec<_>>());
    let cancel = CancellationToken::new();
    let old = audio::prepare_audio(&original, &stereo, "wav", 100, "trim", &cancel).unwrap();
    let new = render(&original, &stereo, 100, edit(0., 0., 0., 0.));
    assert_eq!(new.audio.resource, old.resource);
    assert!(new.audio.report.resampled && new.audio.report.channels_converted);
    // A later start equals the old conversion of the whole file, cut there.
    let ramp: Vec<i16> = (0..2000).map(|i| ((i * 37) % 30000 - 15000) as i16).collect();
    let long_original = ctrl_pcm(4000, -1, 0);
    let source = wav(16000, 1, &ramp);
    let whole = audio::prepare_audio(&long_original, &source, "wav", 100, "pad", &cancel).unwrap();
    let all = samples16(&whole.resource);
    let window = samples16(&render(&original, &source, 100, edit(123. / 8000., 0., 0., 0.)).audio.resource);
    assert_eq!(window, all[123..523].to_vec());
    // An MP3 game sound becomes PCM16 as a replacement of it does.
    let mp3 = mp3_original();
    let out = render(&mp3, &wav(8000, 1, &[1000; 2000]), 100, edit(0.01, -6., 0., 0.));
    audio::validate_rendered_sound(&mp3, &out.audio.resource).unwrap();
    assert_eq!(audio::metadata(&out.audio.resource).unwrap().format, "PCM16");
    // A compiled sound can be the file too (its samples).
    let compiled = ctrl_pcm(300, -1, 2000);
    let decoded = audio::decode_source(&compiled, "vsnd_c", &cancel).unwrap();
    let out = audio::render_edit(&original, &decoded, 100, &edit(0., 0., 0., 0.), &cancel).unwrap();
    assert!(samples16(&out.audio.resource).iter().take(300).all(|v| *v == 2000));
    // MP3, FLAC and OGG files decode through the same path.
    for (name, ext) in [("extras/tone.mp3", "mp3"), ("extras/tone.flac", "flac"), ("extras/tone.ogg", "ogg")] {
        let decoded = audio::decode_source(&read(name), ext, &cancel).unwrap();
        // 0.16 s of tone (the MP3 without a gapless header decodes longer).
        assert!(decoded.duration() >= 0.15 && decoded.duration() < 0.4, "{name}: {}", decoded.duration());
        assert!(decoded.peaks(16).iter().any(|p| *p > 0.1), "{name}");
        audio::render_edit(&original, &decoded, 100, &edit(0.02, -3., 0.01, 0.01), &cancel).unwrap();
    }
    assert!(audio::decode_source(b"text", "txt", &cancel).is_err());
}

#[test]
fn volume_is_in_db_with_the_old_percent_and_never_clips() {
    let original = ctrl_pcm(100, -1, 0);
    let source = wav(8000, 1, &[4000; 100]);
    // +6.0206 dB doubles; -6.0206 dB halves; 150% multiplies on top.
    let twice = 20. * 2f64.log10();
    assert_eq!(samples16(&render(&original, &source, 100, edit(0., twice, 0., 0.)).audio.resource), vec![8000; 100]);
    assert_eq!(samples16(&render(&original, &source, 100, edit(0., -twice, 0., 0.)).audio.resource), vec![2000; 100]);
    assert_eq!(samples16(&render(&original, &source, 150, edit(0., twice, 0., 0.)).audio.resource), vec![12000; 100]);
    let report = render(&original, &source, 150, edit(0., 0., 0., 0.)).audio.report.window.unwrap();
    assert!((report.requested_gain_db.unwrap() - 20. * 1.5f64.log10()).abs() < 1e-9);
    assert!(!report.gain_limited);
    // 0%: silence, no gain in dB.
    let muted = render(&original, &source, 0, edit(0., 3., 0., 0.));
    assert!(samples16(&muted.audio.resource).iter().all(|v| *v == 0));
    assert_eq!(muted.audio.report.window.unwrap().applied_gain_db, None);
    // Louder than full scale: the gain is held so the peak sits at CEILING_DB.
    let loud = wav(8000, 1, &(0..100).map(|i| if i % 2 == 0 { 29491 } else { -29491 }).collect::<Vec<_>>()); // 0.9 of full scale
    let out = render(&original, &loud, 100, edit(0., 12., 0., 0.));
    let window = out.audio.report.window.clone().unwrap();
    assert!(window.gain_limited);
    assert_eq!(out.audio.report.clipped_samples, 0);
    let ceiling = 10f64.powf(audio::CEILING_DB / 20.);
    let applied = window.applied_gain_db.unwrap();
    assert!((applied - 20. * (ceiling / (29491. / 32768.)).log10()).abs() < 1e-4, "{applied}");
    let peak = samples16(&out.audio.resource).iter().map(|v| (*v as i32).abs()).max().unwrap();
    assert!(peak <= (ceiling * 32768.).round() as i32 && peak > 32000, "{peak}");
    // The same in 8-bit sounds.
    let eight = {
        let mut eight = read("sound-data.bin");
        let layout = resource_blocks(&eight).unwrap();
        let at = data_offset(&eight);
        eight[at + 2] = 1;
        eight[at + 28..at + 32].copy_from_slice(&8u32.to_le_bytes());
        eight.truncate(layout.declared_size);
        eight.extend([128; 8]);
        eight
    };
    let out = render(&eight, &loud, 100, edit(0., 12., 0., 0.));
    audio::validate_rendered_sound(&eight, &out.audio.resource).unwrap();
    assert_eq!(out.audio.report.clipped_samples, 0);
    assert!(out.audio.report.window.unwrap().gain_limited);
}

#[test]
fn fades_are_linear_shortened_to_the_sound_and_end_where_the_file_ends() {
    let original = ctrl_pcm(100, -1, 0);
    let source = wav(8000, 1, &[8000; 400]);
    // 10 frames in, 20 frames out.
    let out = render(&original, &source, 100, edit(0., 0., 10. / 8000., 20. / 8000.));
    let v = samples16(&out.audio.resource);
    assert_eq!(&v[..4], &[0, 800, 1600, 2400]);
    assert_eq!(v[10], 8000);
    assert_eq!(v[99], 0);
    assert_eq!(v[89], 4000); // 10 of 20 steps from the end
    assert!(v[11..80].iter().all(|x| *x == 8000));
    let w = out.audio.report.window.unwrap();
    assert_eq!((w.fade_in_frames, w.fade_out_frames, w.fade_in_shortened, w.fade_out_shortened), (10, 20, false, false));
    // Longer than the sound: shortened to it, never an error (one edit fits every variation).
    let out = render(&original, &source, 100, edit(0., 0., 30., 0.5));
    let w = out.audio.report.window.unwrap();
    assert_eq!((w.fade_in_frames, w.fade_out_frames, w.fade_in_shortened, w.fade_out_shortened), (100, 100, true, true));
    // A short file: the fade out ends where the file ends, not in the silence after it.
    let out = render(&original, &wav(8000, 1, &[8000; 40]), 100, edit(0., 0., 0., 10. / 8000.));
    let v = samples16(&out.audio.resource);
    assert_eq!(v[39], 0);
    assert_eq!(v[30], 7200);
    assert!(v[40..].iter().all(|x| *x == 0));
    assert_eq!(out.audio.report.window.unwrap().fade_out_frames, 10);
}

#[test]
fn a_looping_sound_keeps_its_loop_points_and_has_no_fade_out() {
    let original = ctrl_pcm(100, 30, 0);
    let before = audio::metadata(&original).unwrap();
    assert_eq!(before.loop_start, 30);
    let source = wav(8000, 1, &[8000; 400]);
    let out = render(&original, &source, 100, edit(0.002, 0., 50. / 8000., 20. / 8000.));
    let after = audio::metadata(&out.audio.resource).unwrap();
    assert_eq!((after.loop_start, after.loop_end, after.frames), (before.loop_start, before.loop_end, before.frames));
    audio::validate_rendered_sound(&original, &out.audio.resource).unwrap();
    let w = out.audio.report.window.unwrap();
    assert!(w.looping);
    assert_eq!((w.fade_in_frames, w.fade_out_frames, w.fade_in_shortened, w.fade_out_shortened), (30, 0, true, true));
    let v = samples16(&out.audio.resource);
    assert!(v[30..].iter().all(|x| *x == 8000), "the looping part plays at one level");
    // A loop from the first frame: no fades at all.
    let out = render(&ctrl_pcm(100, 0, 0), &source, 100, edit(0., 0., 0.01, 0.));
    assert_eq!(out.audio.report.window.unwrap().fade_in_frames, 0);
}

fn sine(rate: u32, seconds: f64, amplitude: f64, hz: f64) -> Vec<f32> {
    (0..(rate as f64 * seconds) as usize).map(|i| (amplitude * (2. * std::f64::consts::PI * hz * i as f64 / rate as f64).sin()) as f32).collect()
}
fn pcm16(values: &[f32]) -> Vec<i16> {
    values.iter().map(|v| (*v * 32768.).round().clamp(-32768., 32767.) as i16).collect()
}

#[test]
fn loudness_is_bs1770_gated_and_matching_sets_the_volume() {
    // A full-scale 997 Hz sine in one channel measures -3.01 LUFS (BS.1770's reference).
    let full = audio::loudness(&sine(48000, 2., 1., 997.), 1, 48000).unwrap();
    assert!((full + 3.01).abs() < 0.05, "{full}");
    let half = audio::loudness(&sine(48000, 2., 0.5, 997.), 1, 48000).unwrap();
    assert!((half - full + 6.02).abs() < 0.02, "{half}");
    // Gating: a second of silence after the tone lowers it only by the three
    // blocks across the edge (0.34 dB), where a plain mean square would drop 1.76 dB.
    let mut padded = sine(48000, 2., 0.5, 997.);
    padded.extend(vec![0.; 48000]);
    let gated = audio::loudness(&padded, 1, 48000).unwrap();
    assert!((half - gated - 0.34).abs() < 0.05, "{gated}");
    assert_eq!(audio::loudness(&vec![0.; 48000], 1, 48000), None);
    // Shorter than one block: still measured.
    assert!(audio::loudness(&sine(48000, 0.1, 0.5, 997.), 1, 48000).is_some());
    // Matching: a game sound 6 dB louder than the file asks for about +6 dB.
    let rate = 8000;
    let game_values = pcm16(&sine(rate, 0.6, 0.4, 440.));
    let mut game = ctrl_pcm(game_values.len() as u32, -1, 0);
    let offset = audio::metadata(&game).unwrap().payload_offset;
    game.truncate(offset);
    for v in &game_values {
        game.extend(v.to_le_bytes());
    }
    let cancel = CancellationToken::new();
    let (values, channels, game_rate) = audio::sound_samples(&game, &cancel).unwrap();
    let game_lufs = audio::loudness(&values, channels, game_rate);
    let file = wav(rate, 1, &pcm16(&sine(rate, 2., 0.2, 440.)));
    let rendered = render(&game, &file, 100, edit(0.5, 0., 0., 0.));
    let matched = audio::matched_volume(game_lufs, &rendered, 100).unwrap();
    assert!((matched.volume_db - 6.0).abs() < 0.2, "{matched:?}");
    assert!(!matched.limited_by_clipping && !matched.limited_by_range);
    // With the matched volume the result measures like the game's sound.
    let louder = render(&game, &file, 100, edit(0.5, matched.volume_db, 0., 0.));
    let (v, c, r) = audio::sound_samples(&louder.audio.resource, &cancel).unwrap();
    assert!((audio::loudness(&v, c, r).unwrap() - game_lufs.unwrap()).abs() < 0.2);
    // The older percent counts: at 200% the match is 6 dB lower.
    let at_200 = audio::matched_volume(game_lufs, &render(&game, &file, 200, edit(0.5, 0., 0., 0.)), 200).unwrap();
    assert!((at_200.volume_db - (matched.volume_db - 6.02)).abs() < 0.15, "{at_200:?}");
    // A quiet file whose match would clip is held below the ceiling.
    let spiky: Vec<i16> = (0..16000).map(|i| if i % 800 == 0 { 30000 } else { ((i as f64 * 0.3).sin() * 300.) as i16 }).collect();
    let rendered = render(&game, &wav(rate, 1, &spiky), 100, edit(0., 0., 0., 0.));
    let held = audio::matched_volume(game_lufs, &rendered, 100).unwrap();
    assert!(held.limited_by_clipping, "{held:?}");
    let limit = audio::CEILING_DB - 20. * (rendered.peak_before_gain as f64).log10();
    assert!(held.volume_db <= limit + 1e-9);
    // Silence on either side, or 0%: no match.
    assert_eq!(audio::matched_volume(None, &rendered, 100), None);
    assert_eq!(audio::matched_volume(game_lufs, &rendered, 0), None);
}

#[test]
fn peaks_cover_the_file_in_equal_parts() {
    let values: Vec<f32> = (0..1000).map(|i| if i == 999 { -0.9 } else { (i % 10) as f32 / 100. }).collect();
    let p = audio::peaks(&values, 1, 10);
    assert_eq!(p.len(), 10);
    assert!((p[0] - 0.09).abs() < 1e-6 && (p[9] - 0.9).abs() < 1e-6);
    assert_eq!(audio::peaks(&values[..4], 1, 100).len(), 4, "never more buckets than frames");
    assert_eq!(audio::peaks(&[0.1, -0.5, 0.2, 0.3], 2, 2), vec![0.5, 0.3]);
}

#[test]
fn one_edit_fits_every_variation_whatever_its_length() {
    // "Replace all N variations with this part": the same file and edit on
    // sounds of different lengths render, with fades held to each one.
    let source = wav(8000, 1, &[6000; 2000]);
    let e = edit(0.05, -3., 0.02, 0.03);
    for (frames, loop_start) in [(40, -1), (100, -1), (1600, -1), (800, 100)] {
        let original = ctrl_pcm(frames, loop_start, 0);
        let out = render(&original, &source, 100, e);
        audio::validate_rendered_sound(&original, &out.audio.resource).unwrap();
        let w = out.audio.report.window.unwrap();
        assert!(w.fade_in_frames <= frames && w.fade_out_frames <= frames);
        assert_eq!(audio::metadata(&out.audio.resource).unwrap().frames, frames);
    }
}

/// A 16-bit WAV header for `samples` samples, followed by `samples` samples of a
/// slow ramp written in pieces (no whole file in memory).
fn write_long_wav(path: &Path, rate: u32, channels: u16, samples: u64) {
    use std::io::Write;
    let mut file = std::io::BufWriter::new(fs::File::create(path).unwrap());
    let bytes = samples * 2;
    file.write_all(b"RIFF").unwrap();
    file.write_all(&(36 + bytes as u32).to_le_bytes()).unwrap();
    file.write_all(b"WAVEfmt ").unwrap();
    for v in [16u32.to_le_bytes().to_vec(), 1u16.to_le_bytes().to_vec(), channels.to_le_bytes().to_vec(), rate.to_le_bytes().to_vec(),
        (rate * channels as u32 * 2).to_le_bytes().to_vec(), (channels * 2).to_le_bytes().to_vec(), 16u16.to_le_bytes().to_vec()] {
        file.write_all(&v).unwrap();
    }
    file.write_all(b"data").unwrap();
    file.write_all(&(bytes as u32).to_le_bytes()).unwrap();
    let mut chunk = Vec::with_capacity(1 << 20);
    for i in 0..samples {
        // The frame number in seconds, as a level: the part taken can be read back.
        let second = (i / channels as u64) / rate as u64;
        chunk.extend_from_slice(&((second as i16 % 300) * 100).to_le_bytes());
        if chunk.len() >= 1 << 20 { file.write_all(&chunk).unwrap(); chunk.clear(); }
    }
    file.write_all(&chunk).unwrap();
}

#[test]
fn a_long_file_is_streamed_for_an_edit_and_still_refused_without_one() {
    // Three minutes of 48 kHz stereo: 17.3 million samples, past the 16 million
    // the old encoder takes, and 34.6 MB.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("long.wav");
    let seconds = 180u64;
    write_long_wav(&path, 48000, 2, seconds * 48000 * 2);
    let cancel = CancellationToken::new();
    let original = ctrl_pcm(800, -1, 0); // 0.1 s at 8 kHz
    let raw = fs::read(&path).unwrap();
    let old = audio::prepare_audio(&original, &raw, "wav", 100, "trim", &cancel).unwrap_err().to_string();
    assert!(old.contains("16-million-sample"), "the old limit stays: {old}");
    drop(raw);
    let decoded = audio::decode_source_file(&path, "wav", &cancel).unwrap();
    assert_eq!((decoded.rate(), decoded.channels(), decoded.frames()), (48000, 2, seconds as usize * 48000));
    assert_eq!(decoded.bytes(), seconds as usize * 48000 * 2 * 2, "16-bit samples");
    assert_eq!(decoded.peaks(480).len(), 480);
    // Second 150 of the file: level 150 × 100 (the part was taken from there).
    let out = audio::render_edit(&original, &decoded, 100, &edit(150.02, 0., 0., 0.), &cancel).unwrap();
    assert!(samples16(&out.audio.resource).iter().all(|v| *v == 15000));
    // The build's path streams the file and hashes it in pieces.
    let (digest, built) = audio::prepare_edited_file(&original, &path, "wav", 100, &edit(150.02, 0., 0., 0.), &cancel).unwrap();
    assert_eq!(digest, resource_fingerprint(&fs::read(&path).unwrap()));
    assert_eq!(built.resource, out.audio.resource);
    // The same from memory gives the same samples.
    let from_memory = audio::decode_source(&fs::read(&path).unwrap(), "wav", &cancel).unwrap();
    assert_eq!(audio::render_edit(&original, &from_memory, 100, &edit(150.02, 0., 0., 0.), &cancel).unwrap().audio.resource, out.audio.resource);
}

#[test]
fn the_editor_s_own_limit_names_its_minutes() {
    let dir = tempfile::tempdir().unwrap();
    let cancel = CancellationToken::new();
    // A WAV whose header declares one sample more than the limit (zeros, made sparse).
    let path = dir.path().join("too-long.wav");
    write_long_wav(&path, 48000, 2, 0);
    let samples = audio::EDIT_MAX_SAMPLES as u64 + 2;
    let mut head = fs::read(&path).unwrap();
    head[4..8].copy_from_slice(&(36 + samples as u32 * 2).to_le_bytes());
    head[40..44].copy_from_slice(&(samples as u32 * 2).to_le_bytes());
    fs::write(&path, &head).unwrap();
    fs::OpenOptions::new().write(true).open(&path).unwrap().set_len(44 + samples * 2).unwrap();
    let refused = audio::decode_source_file(&path, "wav", &cancel).err().unwrap().to_string();
    assert!(refused.contains(&format!("at most {} minutes", audio::EDIT_MAX_MINUTES)), "{refused}");
    assert_eq!(audio::EDIT_MAX_MINUTES, 20);
    // Other checks stay: a truncated WAV and an unknown format.
    let short = dir.path().join("short.wav");
    write_long_wav(&short, 8000, 1, 100);
    let mut cut = fs::read(&short).unwrap();
    cut.pop();
    fs::write(&short, cut).unwrap();
    assert!(audio::decode_source_file(&short, "wav", &cancel).err().unwrap().to_string().contains("truncated"));
    assert!(audio::decode_source_file(&short, "aiff", &cancel).is_err());
}

fn quiet_log(_: &str) {}

#[test]
fn an_edited_row_builds_records_its_edit_and_a_changed_edit_needs_a_rebuild() {
    let dir = tempfile::tempdir().unwrap();
    let cancel = CancellationToken::new();
    let settings = Settings {
        workspace: dir.path().join("workspace").to_string_lossy().into(),
        pak: fixture("components_dir.vpk").to_string_lossy().into(),
        ..Default::default()
    };
    let runner = Runner::new(dir.path().join("logs"), cancel.clone(), RunnerOptions::default()).unwrap();
    let clock = TimingRecorder::new("sound edit");
    let service = ComponentService { settings: &settings, runner: &runner, cancel: &cancel, clock: &clock, log: &quiet_log };
    let original = Vpk::open(&settings.pak).unwrap().read(RIFLE).unwrap();
    let meta = audio::metadata(&original).unwrap();
    let rate = meta.rate;
    let wave = dir.path().join("long.wav");
    let file: Vec<i16> = (0..4000).map(|i| ((i * 13) % 20000 - 10000) as i16).collect();
    fs::write(&wave, wav(rate, meta.channels as u16, &file)).unwrap();
    let row = ResourceReplacement {
        enabled: Some(true), source: Some(wave.to_string_lossy().into()), target: Some(RIFLE.into()), allow_shared: Some(false),
        volume_percent: Some(100), audio_fit: Some("pad".into()),
        start_seconds: Some(100. / rate as f64), volume_db: Some(-6.), fade_in_seconds: Some(2. / rate as f64), fade_out_seconds: Some(2. / rate as f64),
        ..Default::default()
    };
    let mut project = Project::default();
    project.original_model = HERO.into();
    project.customization.enabled = Some(true);
    project.customization.model_enabled = Some(false);
    project.customization.audio = Some(vec![row.clone()]);
    // "pad" with a longer file would be refused; the edit's frame takes its part instead.
    let output = dir.path().join("game");
    let receipt = service.compile(&project, &output).unwrap().unwrap();
    let built = fs::read(output.join(RIFLE)).unwrap();
    let expected = audio::prepare_edited_audio(&original, &fs::read(&wave).unwrap(), "wav", 100, &row.sound_edit().unwrap(), &cancel).unwrap();
    assert_eq!(built, expected.resource, "the build writes what the preview renders");
    audio::validate_rendered_sound(&original, &built).unwrap();
    let entry = &receipt.audio.entries[0];
    assert_eq!(entry.sound_edit, row.sound_edit());
    let json = serde_json::to_value(entry).unwrap();
    assert_eq!(json["sound_edit"]["volume_db"], -6.0);
    assert_eq!(json["audio_edit"]["window"]["policy"], audio::WINDOW_POLICY);
    assert_eq!(json["audio_edit"]["window"]["start_frame"], 100);
    service.verify_snapshot(&project, &receipt, &output).unwrap();
    // The receipt read back from JSON still matches the project.
    let reread: deadlock_components::service::ComponentReceipt = serde_json::from_str(&serde_json::to_string(&receipt).unwrap()).unwrap();
    service.verify_snapshot(&project, &reread, &output).unwrap();
    // A changed fade, or the edit removed: the build is stale.
    let mut changed = project.clone();
    changed.customization.audio = Some(vec![ResourceReplacement { fade_out_seconds: Some(3. / rate as f64), ..row.clone() }]);
    let stale = service.verify_snapshot(&changed, &receipt, &output).unwrap_err().to_string();
    assert!(stale.contains("part, volume or fades changed"), "{stale}");
    changed.customization.audio = Some(vec![ResourceReplacement {
        start_seconds: None, volume_db: None, fade_in_seconds: None, fade_out_seconds: None, ..row.clone()
    }]);
    assert!(service.verify_snapshot(&changed, &receipt, &output).is_err());
    // Without the edit the old path runs: "trim" takes the first part, and
    // the receipt has no sound_edit key.
    changed.customization.audio = Some(vec![ResourceReplacement {
        start_seconds: None, volume_db: None, fade_in_seconds: None, fade_out_seconds: None, audio_fit: Some("trim".into()), ..row.clone()
    }]);
    let plain = service.compile(&changed, &dir.path().join("game2")).unwrap().unwrap();
    let json = serde_json::to_value(&plain.audio.entries[0]).unwrap();
    assert!(json.get("sound_edit").is_none() && json["audio_edit"].get("window").is_none());
    // An edit on a silenced row is refused before anything is written.
    let mut silenced = project.clone();
    silenced.customization.audio = Some(vec![ResourceReplacement { source: None, silence: Some(true), ..row.clone() }]);
    assert!(service.compile(&silenced, &dir.path().join("game3")).is_err());
    let archive = Vpk::open(&settings.pak).unwrap();
    let staged = resources::stage(&archive, HERO, &[ResourceReplacement { source: None, silence: Some(true), ..row }], &dir.path().join("game4"), ResourceKind::Audio, &cancel);
    assert!(staged.unwrap_err().to_string().contains("silenced sound has no file"));
    assert!(!dir.path().join("game4").join(RIFLE).exists());
}
