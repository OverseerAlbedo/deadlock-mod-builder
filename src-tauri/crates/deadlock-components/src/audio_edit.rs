//! Deterministic preview/build audio processing. Source files are never rewritten.
use super::*;
use crate::CancellationToken;
use std::{collections::BTreeMap, io::Cursor};
use symphonia::core::{audio::SampleBuffer, errors::Error as DecodeError, io::MediaSourceStream, probe::Hint};

const MAX_SAMPLES: usize = 16_000_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditReport {
    pub policy: String,
    pub volume_percent: u16,
    pub source_duration: f64,
    pub output_duration: f64,
    pub resampled: bool,
    pub channels_converted: bool,
    pub padded_frames: u32,
    pub trimmed_frames: u32,
    pub clipped_samples: usize,
    pub peak_before_gain: f32,
}
#[derive(Debug)]
pub struct PreparedAudio { pub resource: Vec<u8>, pub report: EditReport }
struct Samples { rate: u32, channels: usize, values: Vec<f32> }

fn decode(raw: &[u8], extension: &str, cancel: &CancellationToken) -> Result<Samples> {
    if raw.is_empty() || raw.len() > MAX_RESOURCE_BYTES {return Err(fail("Choose an audio file smaller than 128 MiB."));}
    if extension == "wav" && (raw.get(..4) != Some(b"RIFF") || raw.get(8..12) != Some(b"WAVE") || u32v(raw,4)? as usize+8 != raw.len()) {
        return Err(fail("The WAV is truncated or has an invalid RIFF size."));
    }
    let stream = MediaSourceStream::new(Box::new(Cursor::new(raw.to_vec())), Default::default());
    let mut hint = Hint::new(); hint.with_extension(extension);
    let opts = symphonia::core::formats::FormatOptions {enable_gapless: true, ..Default::default()};
    let mut format = symphonia::default::get_probe().format(&hint, stream, &opts, &Default::default()).map_err(fail)?.format;
    let track = format.default_track().ok_or_else(|| fail("This file contains no audio track."))?;
    let id = track.id;
    if track.codec_params.n_frames.zip(track.codec_params.channels).is_some_and(|(frames,channels)| frames.saturating_mul(channels.count() as u64)>MAX_SAMPLES as u64) {
        return Err(fail("This clip exceeds the 16-million-sample editing limit. Choose a shorter clip."));
    }
    let mut decoder = symphonia::default::get_codecs().make(&track.codec_params, &Default::default()).map_err(fail)?;
    let mut out = Samples {rate:0, channels:0, values:vec![]};
    loop {
        cancel.check()?;
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(DecodeError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(fail(format!("Could not read the complete audio: {e}"))),
        };
        if packet.track_id() != id {continue;}
        let data = decoder.decode(&packet).map_err(|e| fail(format!("Could not decode the complete audio: {e}")))?;
        let rate = data.spec().rate;
        let channels = data.spec().channels.count();
        if ![1,2].contains(&channels) || !(8000..=192000).contains(&rate) {
            return Err(fail("Choose mono or stereo audio between 8 kHz and 192 kHz."));
        }
        if out.rate != 0 && (out.rate != rate || out.channels != channels) {
            return Err(fail("The audio changes sample rate or channels midway through."));
        }
        if data.frames().saturating_mul(channels).saturating_add(out.values.len()) > MAX_SAMPLES {
            return Err(fail("This clip exceeds the 16-million-sample editing limit. Choose a shorter clip."));
        }
        let mut buffer = SampleBuffer::<f32>::new(data.capacity() as u64, *data.spec());
        buffer.copy_interleaved_ref(data);
        if buffer.samples().iter().any(|s| !s.is_finite()) {return Err(fail("Audio contains invalid sample values."));}
        out.rate = rate; out.channels = channels;
        out.values.extend_from_slice(buffer.samples());
    }
    if out.values.is_empty() {return Err(fail("This audio has no playable samples."));}
    Ok(out)
}

fn samples(raw: &[u8], cancel: &CancellationToken) -> Result<Samples> {
    let meta = metadata(raw)?;
    let payload = &raw[meta.payload_offset..];
    if meta.frames as usize * meta.channels as usize > MAX_SAMPLES {return Err(fail("This sound is too long for the audio editor."));}
    let values = match meta.format.as_str() {
        "PCM8" => payload.iter().map(|v| (*v as f32-128.)/128.).collect(),
        "PCM16" => payload.chunks_exact(2).map(|v| i16::from_le_bytes([v[0],v[1]]) as f32/32768.).collect(),
        "MP3" => {
            let mut data = decode(payload, "mp3", cancel)?;
            if data.rate != meta.rate || data.channels != meta.channels as usize {return Err(fail("Decoded audio does not match the game sound format."));}
            let count = meta.frames as usize * meta.channels as usize;
            // MP3 frames include encoder padding; retain the authored sample timeline.
            if data.values.len().abs_diff(count) > 2304*data.channels {return Err(fail("Decoded MP3 does not match the game sound timeline."));}
            data.values.resize(count, 0.);
            data.values
        }
        _ => return Err(fail("A preview is not available for this sound format.")),
    };
    cancel.check()?;
    Ok(Samples {rate:meta.rate, channels:meta.channels as usize, values})
}

fn pcm(values: &[f32], bits: u32, volume: u16) -> (Vec<u8>, usize) {
    let gain = volume as f32 / 100.;
    let mut clipped = 0;
    let mut out = Vec::with_capacity(values.len()*(bits as usize/8));
    for value in values {
        let sample = value*gain;
        if sample > 1. || sample < -1. {clipped += 1;}
        if bits == 8 {out.push((sample*128.+128.).round().clamp(0.,255.) as u8);}
        else {out.extend_from_slice(&((sample*32768.).round().clamp(-32768.,32767.) as i16).to_le_bytes());}
    }
    (out, clipped)
}

fn wave(samples: &Samples) -> Vec<u8> {
    let (payload, _) = pcm(&samples.values, 16, 100);
    let mut out = Vec::with_capacity(44+payload.len());
    out.extend(b"RIFF");out.extend((36+payload.len() as u32).to_le_bytes());out.extend(b"WAVEfmt ");
    out.extend(16u32.to_le_bytes());out.extend(1u16.to_le_bytes());out.extend((samples.channels as u16).to_le_bytes());
    out.extend(samples.rate.to_le_bytes());out.extend((samples.rate*samples.channels as u32*2).to_le_bytes());
    out.extend((samples.channels as u16*2).to_le_bytes());out.extend(16u16.to_le_bytes());
    out.extend(b"data");out.extend((payload.len() as u32).to_le_bytes());out.extend(payload);out
}

pub fn preview_audio(raw: &[u8], cancel: &CancellationToken) -> Result<Vec<u8>> {Ok(wave(&samples(raw,cancel)?))}

fn converted(source: &Samples, rate: u32, channels: usize, cancel: &CancellationToken) -> Result<Vec<f32>> {
    let frames = source.values.len()/source.channels;
    let count = ((frames as f64*rate as f64/source.rate as f64).round() as usize).max(1);
    if count.saturating_mul(channels) > MAX_SAMPLES {return Err(fail("Converted audio is too large. Choose a shorter clip."));}
    let at = |frame:usize, channel:usize| -> f32 {
        if channels == 1 && source.channels == 2 {(source.values[frame*2]+source.values[frame*2+1])*0.5}
        else {source.values[frame*source.channels+channel.min(source.channels-1)]}
    };
    let mut out = Vec::with_capacity(count*channels);
    // Windowed-sinc resampling avoids the aliasing of linear downsampling.
    let cutoff = (rate as f64/source.rate as f64).min(1.)*0.94;
    for frame in 0..count {
        if frame%4096 == 0 {cancel.check()?;}
        for channel in 0..channels {
            if rate == source.rate {out.push(at(frame.min(frames-1),channel));continue;}
            let t = frame as f64*source.rate as f64/rate as f64;
            let center = t.floor() as i64;
            let (mut sum, mut norm) = (0.,0.);
            for j in center-24..=center+24 {
                let distance = t-j as f64;
                if distance.abs()>24. {continue;}
                let x = std::f64::consts::PI*distance*cutoff;
                let kernel = if x.abs()<1e-12 {1.} else {x.sin()/x};
                let weight = kernel*(0.5+0.5*(std::f64::consts::PI*distance/24.).cos());
                sum += at(j.clamp(0,frames as i64-1) as usize,channel) as f64*weight;
                norm += weight;
            }
            out.push((sum/norm) as f32);
        }
    }
    Ok(out)
}

fn encode_resource(original: &[u8], payload: &[u8], bits: u32) -> Result<Vec<u8>> {
    let meta = metadata(original)?;
    let layout = resource_blocks(original).map_err(fail)?;
    let block = layout.blocks.iter().find(|b| b.name == meta.kind).unwrap();
    let mut body = original[block.offset..block.offset+block.size].to_vec();
    let new_format = if bits == 8 {"PCM8"} else {"PCM16"};
    if meta.kind == "CTRL" {
        let mut doc = binary::decode(&body).map_err(fail)?;
        let sound = doc.root.get_mut("m_vSound").unwrap();
        sound.get_mut("m_nFormat").unwrap().kind = Kind::String(new_format.into());
        sound.get_mut("m_nStreamingSize").unwrap().kind = Kind::U64(payload.len() as u64);
        body = binary::encode(&doc.root, doc.format_guid).map_err(fail)?;
    } else {
        if meta.format == "MP3" && (i32v(&body,24)? != 0 || (meta.resource_version < 3 && meta.loop_start != -1)) {
            return Err(fail("This legacy compressed sound has codec-specific playback settings. Use a compatible authored VSND file."));
        }
        if meta.resource_version == 4 {body[2] = if bits == 8 {1} else {0};}
        else {
            let info = (u32v(&body,0)? & !(3 | (31<<2) | (3<<12))) | 1 | (bits<<2) | (1<<12);
            body[..4].copy_from_slice(&info.to_le_bytes());
        }
        body[28..32].copy_from_slice(&(payload.len() as u32).to_le_bytes());
    }
    let mut out = deadlock_resource::container::rebuild(&original[..layout.declared_size],
        &BTreeMap::from([(meta.kind,body)])).map_err(fail)?;
    out.extend_from_slice(payload);
    validate_rendered_sound(original,&out)?;
    Ok(out)
}

/// The only additional contract permitted is MP3 -> PCM16. Rates, sample counts,
/// loops, voice metadata, resource references and opaque blocks remain identical.
pub fn validate_rendered_sound(original: &[u8], output: &[u8]) -> Result<Verification> {
    if let Ok(valid) = validate_sound(original,output) {return Ok(valid);}
    let mut old = read(original)?;let mut new = read(output)?;
    let a=&old.meta;let b=&new.meta;
    if a.format != "MP3" || b.format != "PCM16" || a.kind != b.kind || a.resource_version != b.resource_version
        || (a.rate,a.channels,a.frames,a.loop_start,a.loop_end)!=(b.rate,b.channels,b.frames,b.loop_start,b.loop_end)
        || a.duration != b.duration || old.opaque != new.opaque {
        return Err(fail("Audio conversion changed protected playback or voice metadata."));
    }
    for sound in [&mut old,&mut new] {
        match &mut sound.contract {
            Contract::Bytes(body) => {
                if i32v(body,24)? != 0 || (sound.meta.resource_version<3 && sound.meta.loop_start != -1) {
                    return Err(fail("Cannot convert this codec-specific legacy audio contract."));
                }
                if sound.meta.resource_version==4 {body[2]=0;}
                else {let normalized=u32v(body,0)? & !(3 | (31<<2) | (3<<12));body[..4].copy_from_slice(&normalized.to_le_bytes());}
            }
            Contract::Typed(root) => {root.get_mut("m_vSound").unwrap().get_mut("m_nFormat").unwrap().kind=Kind::String("PCM16".into());}
        }
    }
    let same = match (&old.contract,&new.contract) {
        (Contract::Bytes(a),Contract::Bytes(b))=>a==b,
        (Contract::Typed(a),Contract::Typed(b))=>a.canonical_eq(b),_=>false,
    };
    let mut a=resource_references(original).map_err(fail)?;let mut b=resource_references(output).map_err(fail)?;a.sort();b.sort();
    if !same || a!=b {return Err(fail("Audio conversion changed the original sound contract."));}
    Ok(Verification {status:"verified".into(),metadata:new.meta,native_validated:false,in_game_validated:false})
}

pub fn prepare_audio(original: &[u8], source: &[u8], extension: &str, volume: u16, timing: &str, cancel: &CancellationToken) -> Result<PreparedAudio> {
    if volume>200 || !["exact","pad","trim"].contains(&timing) {return Err(fail("Choose a volume from 0 to 200 percent and a supported timing mode."));}
    let meta=metadata(original)?;
    // Preserve the original strict path and byte output for old projects at 100%.
    let strict = if extension=="vsnd_c" {
        validate_rendered_sound(original,source)?;Some(source.to_vec())
    } else if extension=="wav" && timing=="exact" {Some(wav_replacement(original,source)?)}
    else {None};
    if volume == 100 {
        if let Some(resource) = &strict {
            return Ok(PreparedAudio {resource:resource.clone(),report:EditReport {
                policy:"extras-audio-gain-v1".into(),volume_percent:volume,
                source_duration:meta.duration,output_duration:meta.duration,resampled:false,
                channels_converted:false,padded_frames:0,trimmed_frames:0,clipped_samples:0,peak_before_gain:0.,
            }});
        }
    }
    if meta.frames as usize*meta.channels as usize>MAX_SAMPLES {return Err(fail("This sound is too large for audio editing."));}
    let source = if let Some(raw)=&strict {samples(raw,cancel)?} else {
        if !["wav","mp3","flac","ogg"].contains(&extension) {return Err(fail("Choose WAV, MP3, FLAC, OGG or a compatible VSND file."));}
        decode(source,extension,cancel)?
    };
    let frames=source.values.len()/source.channels;
    if timing == "exact" && (source.rate != meta.rate || source.channels != meta.channels as usize) {
        return Err(fail("Exact audio must match the original rate and channels. Choose Match original timing for automatic conversion."));
    }
    let source_duration=frames as f64/source.rate as f64;
    let mut values=converted(&source,meta.rate,meta.channels as usize,cancel)?;
    let converted_frames=values.len()/meta.channels as usize;
    if timing=="exact" && converted_frames!=meta.frames as usize {return Err(fail("This clip does not match the original length. Choose Match original timing."));}
    if converted_frames>meta.frames as usize && timing!="trim" {
        return Err(fail(format!("Your sound is {source_duration:.2}s; the original is {:.2}s. Enable Trim longer audio to use the first {:.2}s, or choose a shorter clip.",meta.duration,meta.duration)));
    }
    let peak=values.iter().fold(0f32,|a,b|a.max(b.abs()));
    values.resize(meta.frames as usize*meta.channels as usize,0.);
    let bits=if meta.format=="PCM8" {8} else {16};
    let (payload, clipped)=pcm(&values,bits,volume);
    cancel.check()?;
    let resource=if volume==100 && strict.is_some() {strict.unwrap()} else {encode_resource(original,&payload,bits)?};
    Ok(PreparedAudio {resource,report:EditReport {
        policy:"extras-audio-gain-v1".into(),volume_percent:volume,source_duration,output_duration:meta.duration,
        resampled:source.rate!=meta.rate,channels_converted:source.channels!=meta.channels as usize,
        padded_frames:(meta.frames as usize).saturating_sub(converted_frames) as u32,
        trimmed_frames:converted_frames.saturating_sub(meta.frames as usize) as u32,
        clipped_samples:clipped,peak_before_gain:peak,
    }})
}
