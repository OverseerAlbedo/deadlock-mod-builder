//! PCM sample replacement preserves loop, timing, voice, and opaque resource data.
use crate::{MAX_RESOURCE_BYTES, Result, fail};
use deadlock_resource::{Kind, Value, binary};
use deadlock_vpk::{resource_blocks, resource_references};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
#[path = "audio_edit.rs"]
mod edit;
pub use edit::{EditReport, PreparedAudio, prepare_audio, preview_audio, validate_rendered_sound};
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Metadata {
    pub format: String,
    pub rate: u32,
    pub frames: u32,
    pub channels: u32,
    pub loop_start: i64,
    pub loop_end: i64,
    pub duration: f64,
    pub stream_size: usize,
    pub bits: u32,
    pub resource_version: u16,
    pub kind: String,
    pub payload_offset: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mp3_frames: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mp3_samples: Option<u32>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Verification {
    pub status: String,
    pub metadata: Metadata,
    pub native_validated: bool,
    pub in_game_validated: bool,
}
#[derive(Debug)]
enum Contract {
    Bytes(Vec<u8>),
    Typed(Value),
}
struct Sound {
    meta: Metadata,
    contract: Contract,
    opaque: Vec<(String, Vec<u8>)>,
}
fn bytes<const N: usize>(raw: &[u8], offset: usize) -> Result<[u8; N]> {
    raw.get(
        offset
            ..offset
                .checked_add(N)
                .ok_or_else(|| fail("Sound offset overflow"))?,
    )
    .ok_or_else(|| fail("Truncated sound data"))?
    .try_into()
    .map_err(fail)
}
fn u32v(raw: &[u8], p: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(bytes(raw, p)?))
}
fn i32v(raw: &[u8], p: usize) -> Result<i32> {
    Ok(i32::from_le_bytes(bytes(raw, p)?))
}
fn integer(value: &Value, key: &str) -> Result<i128> {
    value
        .get(key)
        .ok_or_else(|| fail("Sound CTRL is missing required playback metadata."))?
        .integer()
        .map_err(fail)
}
fn mp3(payload: &[u8], rate: u32, channels: u32) -> Result<(u32, u32)> {
    let (mut offset, mut count, mut samples) = (0usize, 0u32, 0u32);
    let mut info_frames = None;
    let mut info_header = false;
    while offset < payload.len() {
        let word = u32::from_be_bytes(bytes(payload, offset)?);
        let (version, layer, br, sr) = (
            (word >> 19) & 3,
            (word >> 17) & 3,
            (word >> 12) & 15,
            (word >> 10) & 3,
        );
        if word >> 21 != 0x7ff || version == 1 || layer != 1 || br == 0 || br == 15 || sr == 3 {
            return Err(fail(
                "Sound has an unsupported or malformed MP3 frame stream.",
            ));
        }
        let sample_rate = [44100, 48000, 32000][sr as usize]
            / if version == 3 {
                1
            } else if version == 2 {
                2
            } else {
                4
            };
        let channel_count = if (word >> 6) & 3 == 3 { 1 } else { 2 };
        if sample_rate != rate || channel_count != channels {
            return Err(fail(
                "MP3 frame rate/channels disagree with the sound metadata.",
            ));
        }
        let rates = if version == 3 {
            [
                0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320,
            ]
        } else {
            [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160]
        };
        let size = ((if version == 3 { 144 } else { 72 }) * rates[br as usize] * 1000 / sample_rate
            + ((word >> 9) & 1)) as usize;
        if size < 4 || offset + size > payload.len() {
            return Err(fail("Sound has a truncated MP3 frame."));
        }
        let mut metadata_frame = false;
        if offset == 0 {
            // Xing/Info occupies a complete MPEG frame, but contains seek and
            // encoder metadata rather than audio. Symphonia skips it too.
            // Count the actual audio frames without weakening timeline limits.
            let side = if version == 3 {
                if channels == 1 { 17 } else { 32 }
            } else if channels == 1 { 9 } else { 17 };
            let at = 4 + side;
            let frame = &payload[..size];
            if matches!(frame.get(at..at + 4), Some(b"Xing" | b"Info"))
                && frame[4..at].iter().all(|b| *b == 0)
            {
                let flags = u32::from_be_bytes(bytes(frame, at + 4)?);
                let fields_size = 8 + if flags & 1 != 0 { 4 } else { 0 }
                    + if flags & 2 != 0 { 4 } else { 0 }
                    + if flags & 4 != 0 { 100 } else { 0 }
                    + if flags & 8 != 0 { 4 } else { 0 };
                if flags & !15 != 0 || at + fields_size > size {
                    return Err(fail("Sound has a malformed MP3 Xing/Info header."));
                }
                if flags & 1 != 0 {
                    info_frames = Some(u32::from_be_bytes(bytes(frame, at + 8)?));
                }
                metadata_frame = true;
                info_header = true;
            }
        }
        offset += size;
        count += 1;
        if !metadata_frame { samples += if version == 3 { 1152 } else { 576 }; }
    }
    if count == 0 {
        return Err(fail("Sound contains no MP3 samples."));
    }
    if samples == 0 || info_frames.is_some_and(|declared| declared != count - u32::from(info_header)) {
        return Err(fail("MP3 Xing/Info frame count disagrees with the audio stream."));
    }
    Ok((count, samples))
}
fn read(raw: &[u8]) -> Result<Sound> {
    if raw.is_empty() || raw.len() > MAX_RESOURCE_BYTES {
        return Err(fail("Sound resource must be smaller than 128 MiB."));
    }
    let layout = resource_blocks(raw).map_err(fail)?;
    let names: BTreeSet<_> = layout.blocks.iter().map(|b| b.name.as_str()).collect();
    if names.len() != layout.blocks.len() {
        return Err(fail("Sound contains ambiguous duplicate resource blocks."));
    }
    let (kind, mut meta, contract) = if names.contains("CTRL") {
        // Version-5 sounds retain an empty DATA placeholder; CTRL owns their
        // metadata and samples start at the resource's declared size, not at
        // the placeholder (which can precede CTRL). Keep DATA in the opaque
        // contract so replacements retain it. Two non-empty headers remain
        // ambiguous and must not be silently interpreted as this layout.
        if layout.blocks.iter().any(|b| b.name == "DATA" && b.size != 0) {
            return Err(fail(
                "Sound CTRL conflicts with a non-empty DATA block; this mixed layout is not supported.",
            ));
        }
        let b = layout.blocks.iter().find(|b| b.name == "CTRL").unwrap();
        let mut root = binary::decode(&raw[b.offset..b.offset + b.size])
            .map_err(fail)?
            .root;
        let class = root
            .get("_class")
            .ok_or_else(|| fail("Sound CTRL class is missing"))?
            .string()
            .map_err(fail)?;
        if !["CVoiceContainerDefault", "CVoiceContainerEnvelope"].contains(&class) {
            return Err(fail(
                "This procedural sound container cannot be replaced by one audio file.",
            ));
        }
        let sound = root
            .get("m_vSound")
            .ok_or_else(|| fail("Sound CTRL is missing playback metadata."))?;
        let format = sound
            .get("m_nFormat")
            .ok_or_else(|| fail("Sound format missing"))?
            .string()
            .map_err(fail)?
            .to_owned();
        let duration = sound
            .get("m_flDuration")
            .ok_or_else(|| fail("Sound duration missing"))?;
        let duration = match duration.kind {
            Kind::I64(v) => v as f64,
            Kind::U64(v) => v as f64,
            _ => duration.number().map_err(fail)?,
        };
        let meta = Metadata {
            bits: match format.as_str() {
                "PCM8" => 8,
                "PCM16" => 16,
                _ => 0,
            },
            format,
            rate: integer(sound, "m_nRate")?.try_into().map_err(fail)?,
            frames: integer(sound, "m_nSampleCount")?.try_into().map_err(fail)?,
            channels: integer(sound, "m_nChannels")?.try_into().map_err(fail)?,
            loop_start: integer(sound, "m_nLoopStart")?.try_into().map_err(fail)?,
            loop_end: integer(sound, "m_nLoopEnd")?.try_into().map_err(fail)?,
            duration,
            stream_size: integer(sound, "m_nStreamingSize")?
                .try_into()
                .map_err(fail)?,
            resource_version: layout.resource_version,
            kind: "CTRL".into(),
            payload_offset: layout.declared_size,
            mp3_frames: None,
            mp3_samples: None,
        };
        if let Kind::Object(rows) = &mut root.get_mut("m_vSound").unwrap().kind {
            rows.retain(|(name, _)| name != "m_nStreamingSize");
        }
        ("CTRL", meta, Contract::Typed(root))
    } else {
        let b = layout
            .blocks
            .iter()
            .find(|b| b.name == "DATA")
            .ok_or_else(|| fail("Sound requires one unique DATA block."))?;
        let body = &raw[b.offset..b.offset + b.size];
        let version = layout.resource_version;
        let min = 32
            + if version >= 1 { 8 } else { 0 }
            + if version >= 2 { 4 } else { 0 }
            + if version >= 4 { 4 } else { 0 };
        if version > 4 || body.len() < min {
            return Err(fail("Unsupported or truncated legacy sound DATA header."));
        }
        let (rate, format, channels, bits) = if version == 4 {
            let format = match body[2] {
                0 => "PCM16",
                1 => "PCM8",
                2 => "MP3",
                3 => "ADPCM",
                _ => "unknown",
            }
            .to_string();
            let bits = if format == "PCM16" {
                16
            } else if format == "PCM8" {
                8
            } else {
                0
            };
            (
                u16::from_le_bytes(bytes(body, 0)?) as u32,
                format,
                body[3] as u32,
                bits,
            )
        } else {
            let info = u32v(body, 0)?;
            let (bits, typ, codec) = ((info >> 2) & 31, info & 3, (info >> 12) & 3);
            (
                (info >> 14) & 0x1ffff,
                if typ == 2 {
                    "MP3".into()
                } else if typ == 1 && codec == 1 {
                    format!("PCM{bits}")
                } else {
                    "unsupported".into()
                },
                (info >> 7) & 3,
                bits,
            )
        };
        let sentence = u32v(body, 16)? as usize;
        let header = u32v(body, 20)? as usize;
        let header_size = i32v(body, 24)?;
        if sentence != 0 && (16 + sentence < min || 16 + sentence > body.len() - 20) {
            return Err(fail("Sound sentence metadata points outside DATA."));
        }
        if header_size < 0
            || header_size > 0
                && (20 + header < min || 20 + header + (header_size as usize) > body.len())
        {
            return Err(fail("Sound codec header points outside DATA."));
        }
        if version >= 1 && body[32..40].iter().any(|b| *b != 0)
            || version >= 2 && body[40..44].iter().any(|b| *b != 0)
        {
            return Err(fail("Sound DATA uses unknown playback or morph metadata."));
        }
        let meta = Metadata {
            format,
            rate,
            channels,
            bits,
            frames: u32v(body, 8)?,
            loop_start: i32v(body, 4)? as i64,
            loop_end: if version == 4 {
                i32v(body, 44)? as i64
            } else {
                0
            },
            duration: f32::from_le_bytes(bytes(body, 12)?) as f64,
            stream_size: u32v(body, 28)? as usize,
            resource_version: version,
            kind: "DATA".into(),
            payload_offset: b.offset + b.size,
            mp3_frames: None,
            mp3_samples: None,
        };
        if meta.payload_offset != layout.declared_size {
            return Err(fail(
                "Sound DATA is not immediately before its streaming samples.",
            ));
        }
        let mut stable = body.to_vec();
        stable[28..32].fill(0);
        ("DATA", meta, Contract::Bytes(stable))
    };
    if !["PCM8", "PCM16", "MP3"].contains(&meta.format.as_str())
        || !(8000..=192000).contains(&meta.rate)
        || ![1, 2].contains(&meta.channels)
        || !(1..=192000 * 3600).contains(&meta.frames)
        || !meta.duration.is_finite()
        || meta.duration <= 0.0
    {
        return Err(fail(
            "Unsupported or invalid sound format, sample rate, channel count or duration.",
        ));
    }
    if meta.loop_start < -1
        || meta.loop_start >= meta.frames as i64
        || meta.loop_end < -1
        || meta.loop_end > meta.frames as i64
        || meta.loop_end > 0 && meta.loop_end <= meta.loop_start
    {
        return Err(fail("Sound loop points exceed its sample timeline."));
    }
    let expected = meta.frames as f64 / meta.rate as f64;
    if (meta.duration - expected).abs() > 1.0 / meta.rate as f64 + expected * 1.2e-7 {
        return Err(fail(
            "Sound duration disagrees with its rate and sample count.",
        ));
    }
    let payload = &raw[meta.payload_offset..];
    if payload.is_empty() || payload.len() != meta.stream_size {
        return Err(fail(
            "Sound streaming payload is truncated or has unexpected trailing bytes.",
        ));
    }
    if meta.format.starts_with("PCM") {
        if payload.len() as u64
            != meta.frames as u64 * meta.channels as u64 * (meta.bits / 8) as u64
        {
            return Err(fail(
                "PCM byte count disagrees with the sound sample count.",
            ));
        }
    } else {
        let (frames, samples) = mp3(payload, meta.rate, meta.channels)?;
        if samples.abs_diff(meta.frames) > 2 * 1152 {
            return Err(fail("MP3 frame samples disagree with the sound timeline."));
        }
        meta.mp3_frames = Some(frames);
        meta.mp3_samples = Some(samples);
    }
    let opaque = layout
        .blocks
        .iter()
        .filter(|b| !["REDI", "RED2", kind].contains(&b.name.as_str()))
        .map(|b| (b.name.clone(), raw[b.offset..b.offset + b.size].to_vec()))
        .collect();
    Ok(Sound {
        meta,
        contract,
        opaque,
    })
}
pub fn metadata(raw: &[u8]) -> Result<Metadata> {
    Ok(read(raw)?.meta)
}
pub fn validate_sound(original: &[u8], replacement: &[u8]) -> Result<Verification> {
    let old = read(original)?;
    let new = read(replacement)?;
    if (old.meta.resource_version, &old.meta.kind) != (new.meta.resource_version, &new.meta.kind) {
        return Err(fail(
            "Audio replacement changes the original sound resource version or container type.",
        ));
    }
    let same = match (&old.contract, &new.contract) {
        (Contract::Bytes(a), Contract::Bytes(b)) => a == b,
        (Contract::Typed(a), Contract::Typed(b)) => a.canonical_eq(b),
        _ => false,
    };
    if !same || old.opaque != new.opaque {
        return Err(fail(
            "Audio replacement changes protected playback, loop, voice or opaque metadata. Keep the original sound contract.",
        ));
    }
    let mut a = resource_references(original).map_err(fail)?;
    let mut b = resource_references(replacement).map_err(fail)?;
    a.sort();
    b.sort();
    if a != b {
        return Err(fail(
            "Audio replacement changes the original sound dependencies.",
        ));
    }
    if old.meta.mp3_samples != new.meta.mp3_samples {
        return Err(fail("MP3 replacement changes the encoded sample timeline."));
    }
    Ok(Verification {
        status: "verified".into(),
        metadata: new.meta,
        native_validated: false,
        in_game_validated: false,
    })
}
pub fn wav_replacement(original: &[u8], wav: &[u8]) -> Result<Vec<u8>> {
    let meta = metadata(original)?;
    if !["PCM8", "PCM16"].contains(&meta.format.as_str()) {
        return Err(fail(
            "This original uses compressed audio. Import a compatible authored .vsnd_c; WAV replacement requires a PCM original.",
        ));
    }
    if wav.len() > MAX_RESOURCE_BYTES
        || wav.get(..4) != Some(b"RIFF")
        || wav.get(8..12) != Some(b"WAVE")
    {
        return Err(fail("Cannot read a complete PCM WAV."));
    }
    let end = (u32v(wav, 4)? as usize)
        .checked_add(8)
        .ok_or_else(|| fail("WAV size overflow"))?;
    if end > wav.len() {
        return Err(fail("Truncated WAV."));
    }
    let (mut position, mut format, mut payload) = (12, None, None);
    while position + 8 <= end {
        let length = u32v(wav, position + 4)? as usize;
        let start = position + 8;
        let stop = start
            .checked_add(length)
            .filter(|v| *v <= end)
            .ok_or_else(|| fail("Truncated WAV chunk"))?;
        match &wav[position..position + 4] {
            b"fmt " => {
                if format.is_some() || length < 16 {
                    return Err(fail("Ambiguous or incomplete WAV format."));
                }
                let kind = u16::from_le_bytes(bytes(wav, start)?);
                let channels = u16::from_le_bytes(bytes(wav, start + 2)?) as u32;
                let rate = u32v(wav, start + 4)?;
                let bits = u16::from_le_bytes(bytes(wav, start + 14)?) as u32;
                if kind != 1 {
                    if kind != 0xfffe
                        || length < 40
                        || u16::from_le_bytes(bytes(wav, start + 16)?) < 22
                        || &wav[start + 24..start + 40]
                            != b"\x01\0\0\0\0\0\x10\0\x80\0\0\xaa\0\x38\x9b\x71"
                    {
                        return Err(fail("WAV must contain uncompressed PCM samples."));
                    }
                }
                format = Some((rate, channels, bits));
            }
            b"data" => {
                if payload.is_some() {
                    return Err(fail("Ambiguous WAV data chunks."));
                }
                payload = Some(&wav[start..stop]);
            }
            _ => {}
        }
        position = stop + (length & 1);
    }
    let payload = payload.ok_or_else(|| fail("WAV sample data is missing."))?;
    if format != Some((meta.rate, meta.channels, meta.bits)) || payload.len() != meta.stream_size {
        return Err(fail(format!(
            "Export this WAV as {} Hz, {} channel(s), {}-bit PCM, exactly {} samples. Audio is never silently cut or resampled.",
            meta.rate, meta.channels, meta.bits, meta.frames
        )));
    }
    let mut output = original[..meta.payload_offset].to_vec();
    output.extend_from_slice(payload);
    validate_sound(original, &output)?;
    Ok(output)
}
pub fn playable_wave(raw: &[u8]) -> Result<Vec<u8>> {
    let meta = metadata(raw)?;
    if !["PCM8", "PCM16"].contains(&meta.format.as_str()) {
        return Err(fail(
            "This sound is compressed; audition requires an external decoder.",
        ));
    }
    let payload = &raw[meta.payload_offset..];
    let mut out = Vec::with_capacity(44 + payload.len());
    out.extend(b"RIFF");
    out.extend((36 + payload.len() as u32).to_le_bytes());
    out.extend(b"WAVEfmt ");
    out.extend(16u32.to_le_bytes());
    out.extend(1u16.to_le_bytes());
    out.extend((meta.channels as u16).to_le_bytes());
    out.extend(meta.rate.to_le_bytes());
    out.extend((meta.rate * meta.channels * (meta.bits / 8)).to_le_bytes());
    out.extend(((meta.channels * meta.bits / 8) as u16).to_le_bytes());
    out.extend((meta.bits as u16).to_le_bytes());
    out.extend(b"data");
    out.extend((payload.len() as u32).to_le_bytes());
    out.extend(payload);
    Ok(out)
}
