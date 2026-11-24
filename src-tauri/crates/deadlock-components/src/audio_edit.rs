//! Deterministic preview/build audio processing. Source files are never rewritten.
use super::*;
use crate::CancellationToken;
use std::{collections::BTreeMap, io::Cursor};
use symphonia::core::{audio::SampleBuffer, errors::Error as DecodeError, io::MediaSourceStream, probe::Hint};

pub(super) const MAX_SAMPLES: usize = 16_000_000;

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
    /// A sound edit's applied values (`render_edit`); absent otherwise, so
    /// reports of unedited replacements stay as before.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<WindowReport>,
}
/// What a sound edit did: the part of the file taken, the gain and fades as
/// applied (after the clipping protection and the length and loop limits).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WindowReport {
    pub policy: String,
    /// First frame taken from the file, at the game's rate.
    pub start_frame: u32,
    /// Frames of the file that play; the rest of the game's length is silence.
    pub file_frames: u32,
    /// Requested gain (Volume plus an older Volume percent), in dB; None at 0%.
    pub requested_gain_db: Option<f64>,
    /// The gain written, in dB; lower than requested when it would clip.
    pub applied_gain_db: Option<f64>,
    pub gain_limited: bool,
    pub fade_in_frames: u32,
    pub fade_out_frames: u32,
    /// A fade was shortened: to the sound's length, or by the loop rule.
    pub fade_in_shortened: bool,
    pub fade_out_shortened: bool,
    /// The game's sound loops: no fade out, and fade in ends at the loop start.
    pub looping: bool,
}
pub const WINDOW_POLICY: &str = "extras-audio-window-v1";
/// The loudest sample a sound edit writes: 0.1 dB below full scale.
pub const CEILING_DB: f64 = -0.1;
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
    converted_range(Source {values: &source.values, rate: source.rate, channels: source.channels}, rate, channels, 0, count, cancel)
}

/// A stored sample: f32 for the old encoder (unchanged), 16-bit for the
/// sound editor's decoded files (half the memory for a long song).
trait Sample: Copy { fn value(self) -> f32; }
impl Sample for f32 { #[inline] fn value(self) -> f32 {self} }
impl Sample for i16 { #[inline] fn value(self) -> f32 {self as f32/32768.} }
#[derive(Clone, Copy)]
struct Source<'a, T> { values: &'a [T], rate: u32, channels: usize }

/// Frames `from..from + count` of `converted`'s output, computed alone: the
/// same values the whole conversion gives for them (sound edits convert only
/// the part that plays).
fn converted_range<T: Sample>(source: Source<'_, T>, rate: u32, channels: usize, from: usize, count: usize, cancel: &CancellationToken) -> Result<Vec<f32>> {
    let frames = source.values.len()/source.channels;
    let at = |frame:usize, channel:usize| -> f32 {
        if channels == 1 && source.channels == 2 {(source.values[frame*2].value()+source.values[frame*2+1].value())*0.5}
        else {source.values[frame*source.channels+channel.min(source.channels-1)].value()}
    };
    let mut out = Vec::with_capacity(count*channels);
    // Windowed-sinc resampling avoids the aliasing of linear downsampling.
    let cutoff = (rate as f64/source.rate as f64).min(1.)*0.94;
    for frame in from..from+count {
        if (frame-from)%4096 == 0 {cancel.check()?;}
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

/// "Silence this sound": the original's whole timeline as digital silence (0 in
/// PCM16, 128 in PCM8) through the encoder a replacement uses, so rate,
/// channels, sample count, duration, loop points and voice metadata stay the
/// game's. A shorter sound would change the duration and loop points the
/// sound system reads, which the contract check refuses. MP3 originals become
/// PCM16 as a replacement of them does. No sample limit: nothing is decoded.
pub fn silent_audio(original: &[u8], cancel: &CancellationToken) -> Result<PreparedAudio> {
    let meta = metadata(original)?;
    let bits = if meta.format == "PCM8" {8} else {16};
    let count = meta.frames as usize * meta.channels as usize;
    if count.saturating_mul(bits as usize / 8) > MAX_RESOURCE_BYTES {return Err(fail("This sound is too long to silence."));}
    let payload = if bits == 8 {vec![128u8; count]} else {vec![0u8; count * 2]};
    cancel.check()?;
    let resource = if meta.format.starts_with("PCM") {
        let mut out = original[..meta.payload_offset].to_vec();
        out.extend_from_slice(&payload);
        validate_sound(original, &out)?;
        out
    } else {encode_resource(original, &payload, bits)?};
    Ok(PreparedAudio {resource, report: EditReport {
        policy: SILENCE_POLICY.into(), volume_percent: 0, source_duration: 0., output_duration: meta.duration,
        resampled: false, channels_converted: false, padded_frames: meta.frames, trimmed_frames: 0,
        clipped_samples: 0, peak_before_gain: 0., window: None,
    }})
}
pub const SILENCE_POLICY: &str = "extras-audio-silence-v1";

/// Whether a sound's samples are all digital silence (PCM only; a compressed
/// sound is never reported silent).
pub fn is_silent(raw: &[u8]) -> Result<bool> {
    let meta = metadata(raw)?;
    let payload = &raw[meta.payload_offset..];
    Ok(match meta.format.as_str() {
        "PCM8" => payload.iter().all(|v| *v == 128),
        "PCM16" => payload.iter().all(|v| *v == 0),
        _ => false,
    })
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
                channels_converted:false,padded_frames:0,trimmed_frames:0,clipped_samples:0,peak_before_gain:0.,window:None,
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
        clipped_samples:clipped,peak_before_gain:peak,window:None,
    }})
}

/// The longest file the sound editor opens: this many minutes of 48 kHz
/// stereo (`EDIT_MAX_SAMPLES` samples; longer at a lower rate or in mono).
pub const EDIT_MAX_MINUTES: usize = 20;
pub const EDIT_MAX_SAMPLES: usize = EDIT_MAX_MINUTES*60*48000*2;
/// The largest file the sound editor reads (a 20-minute 48 kHz stereo float WAV is 461 MB).
pub const EDIT_MAX_FILE_BYTES: u64 = 1 << 30;
fn too_long() -> crate::CoreError {
    fail(format!("This file is longer than the sound editor can open: at most {EDIT_MAX_MINUTES} minutes of 48 kHz stereo audio. Choose a shorter file."))
}

/// The user's file decoded for a sound edit, at its own rate and channels, as
/// 16-bit samples (2 bytes per sample: a 10-minute 48 kHz stereo song is
/// 115 MB). The game's sounds are 16-bit, so nothing audible is lost.
/// Decoding is the slow part, so the preview keeps one per file.
pub struct SourceAudio { rate: u32, channels: usize, values: Vec<i16>, peaks: std::sync::Mutex<Option<(usize, Vec<f32>)>> }
impl SourceAudio {
    pub fn rate(&self) -> u32 {self.rate}
    pub fn channels(&self) -> usize {self.channels}
    pub fn frames(&self) -> usize {self.values.len()/self.channels}
    pub fn duration(&self) -> f64 {self.frames() as f64/self.rate as f64}
    /// Bytes the decoded samples hold.
    pub fn bytes(&self) -> usize {self.values.len()*2}
    /// `buckets` peaks over the whole file (as `peaks`), kept for the next call.
    pub fn peaks(&self, buckets: usize) -> Vec<f32> {
        let mut held = self.peaks.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((n, p)) = held.as_ref() { if *n == buckets {return p.clone();} }
        let frames = self.frames();
        let buckets_ = buckets.clamp(1, 4096).min(frames.max(1));
        let p: Vec<f32> = (0..buckets_).map(|b| {
            let (from, to) = (b*frames/buckets_, ((b+1)*frames/buckets_).max(b*frames/buckets_+1).min(frames));
            self.values[from*self.channels..to*self.channels].iter().fold(0f32, |a, v| a.max(v.value().abs()))
        }).collect();
        *held = Some((buckets, p.clone()));
        p
    }
    fn source(&self) -> Source<'_, i16> {Source {values: &self.values, rate: self.rate, channels: self.channels}}
}
fn sixteen(v: f32) -> i16 {(v*32768.).round().clamp(-32768., 32767.) as i16}
fn from_samples(s: Samples) -> SourceAudio {
    SourceAudio {rate: s.rate, channels: s.channels, values: s.values.into_iter().map(sixteen).collect(), peaks: Default::default()}
}

/// Decodes WAV, MP3, FLAC or OGG for the sound editor packet by packet into
/// 16-bit samples, up to `EDIT_MAX_SAMPLES` (the old encoder's limits are not used here).
fn stream_decode(source: Box<dyn symphonia::core::io::MediaSource>, extension: &str, cancel: &CancellationToken) -> Result<SourceAudio> {
    if !["wav","mp3","flac","ogg"].contains(&extension) {return Err(fail("Choose WAV, MP3, FLAC, OGG or a compatible VSND file."));}
    let stream = MediaSourceStream::new(source, Default::default());
    let mut hint = Hint::new(); hint.with_extension(extension);
    let opts = symphonia::core::formats::FormatOptions {enable_gapless: true, ..Default::default()};
    let mut format = symphonia::default::get_probe().format(&hint, stream, &opts, &Default::default()).map_err(fail)?.format;
    let track = format.default_track().ok_or_else(|| fail("This file contains no audio track."))?;
    let id = track.id;
    let mut out = SourceAudio {rate: 0, channels: 0, values: vec![], peaks: Default::default()};
    if let Some((frames, channels)) = track.codec_params.n_frames.zip(track.codec_params.channels) {
        let total = frames.saturating_mul(channels.count() as u64);
        if total > EDIT_MAX_SAMPLES as u64 {return Err(too_long());}
        out.values.reserve_exact(total as usize);
    }
    let mut decoder = symphonia::default::get_codecs().make(&track.codec_params, &Default::default()).map_err(fail)?;
    loop {
        cancel.check()?;
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(DecodeError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(fail(format!("Could not read the complete audio: {e}"))),
        };
        if packet.track_id() != id {continue;}
        let data = decoder.decode(&packet).map_err(|e| fail(format!("Could not decode the complete audio: {e}")))?;
        let (rate, channels) = (data.spec().rate, data.spec().channels.count());
        if ![1,2].contains(&channels) || !(8000..=192000).contains(&rate) {
            return Err(fail("Choose mono or stereo audio between 8 kHz and 192 kHz."));
        }
        if out.rate != 0 && (out.rate != rate || out.channels != channels) {
            return Err(fail("The audio changes sample rate or channels midway through."));
        }
        if data.frames().saturating_mul(channels).saturating_add(out.values.len()) > EDIT_MAX_SAMPLES {return Err(too_long());}
        let mut buffer = SampleBuffer::<f32>::new(data.capacity() as u64, *data.spec());
        buffer.copy_interleaved_ref(data);
        if buffer.samples().iter().any(|s| !s.is_finite()) {return Err(fail("Audio contains invalid sample values."));}
        out.rate = rate; out.channels = channels;
        out.values.extend(buffer.samples().iter().map(|v| sixteen(*v)));
    }
    if out.values.is_empty() {return Err(fail("This audio has no playable samples."));}
    out.values.shrink_to_fit();
    Ok(out)
}
/// Decodes the user's file for a sound edit from memory: WAV, MP3, FLAC, OGG,
/// or the samples of a compiled sound.
pub fn decode_source(source: &[u8], extension: &str, cancel: &CancellationToken) -> Result<SourceAudio> {
    if extension == "vsnd_c" {return Ok(from_samples(samples(source, cancel)?));}
    if extension == "wav" && (source.get(..4) != Some(b"RIFF") || source.get(8..12) != Some(b"WAVE") || u32v(source,4)? as usize+8 != source.len()) {
        return Err(fail("The WAV is truncated or has an invalid RIFF size."));
    }
    stream_decode(Box::new(Cursor::new(source.to_vec())), extension, cancel)
}
/// Decodes the user's file for a sound edit straight from disk, without
/// reading it into memory first (up to `EDIT_MAX_FILE_BYTES`).
pub fn decode_source_file(path: &std::path::Path, extension: &str, cancel: &CancellationToken) -> Result<SourceAudio> {
    deadlock_vpk::check_unlinked_path(path, false).map_err(fail)?;
    let meta = std::fs::metadata(path).map_err(fail)?;
    if !meta.is_file() {return Err(fail("Choose an existing replacement file."));}
    if extension == "vsnd_c" {return Ok(from_samples(samples(&crate::read_file(path)?, cancel)?));}
    if meta.len() > EDIT_MAX_FILE_BYTES {return Err(too_long());}
    let mut file = std::fs::File::open(path).map_err(fail)?;
    if extension == "wav" {
        use std::io::{Read, Seek};
        let mut head = [0u8; 12];
        file.read_exact(&mut head).map_err(|_| fail("The WAV is truncated or has an invalid RIFF size."))?;
        if &head[..4] != b"RIFF" || &head[8..12] != b"WAVE" || u32v(&head,4)? as u64+8 != meta.len() {
            return Err(fail("The WAV is truncated or has an invalid RIFF size."));
        }
        file.rewind().map_err(fail)?;
    }
    stream_decode(Box::new(file), extension, cancel)
}
/// SHA-256 of a file read in pieces (a song for a sound edit can be large).
pub fn file_sha(path: &std::path::Path, cancel: &CancellationToken) -> Result<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut file = std::fs::File::open(path).map_err(fail)?;
    let (mut hash, mut chunk) = (Sha256::new(), vec![0u8; 1 << 20]);
    loop {
        cancel.check()?;
        let n = file.read(&mut chunk).map_err(fail)?;
        if n == 0 {break;}
        hash.update(&chunk[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

/// A sound edit rendered: the resource the build writes, and the samples it holds.
pub struct Rendered {
    pub audio: PreparedAudio,
    /// The written samples at the game's rate and channels (interleaved, -1…1).
    pub values: Vec<f32>,
    pub channels: usize,
    pub rate: u32,
    /// Loudness of the part that plays, after the fades and before any gain (`loudness`).
    pub loudness_before_gain: Option<f64>,
    /// Its peak (before gain).
    pub peak_before_gain: f32,
}

/// The sound edit: the game's length filled from `edit.start_seconds` of the
/// user's file (converted to the game's rate and channels; silence after the
/// file ends), then linear fades (fade in from the first frame, fade out
/// ending where the file's part ends), then the gain (`volume_percent` ×
/// `edit.volume_db`, lowered so no sample passes `CEILING_DB`). A fade longer than that
/// is shortened to it. When the game's sound loops, fade out is not applied
/// (it would dip on every loop) and fade in ends at the loop start. The rate,
/// channels, length, loop points and voice data stay the game's
/// (`encode_resource`, `validate_rendered_sound`).
pub fn render_edit(original: &[u8], source: &SourceAudio, volume_percent: u16, edit: &deadlock_project::customization::SoundEdit, cancel: &CancellationToken) -> Result<Rendered> {
    if volume_percent > 200 {return Err(fail("Choose a volume from 0 to 200 percent."));}
    edit.validate().map_err(fail)?;
    let meta = metadata(original)?;
    let (frames, channels, rate) = (meta.frames as usize, meta.channels as usize, meta.rate);
    if frames.saturating_mul(channels) > MAX_SAMPLES {return Err(fail("This sound is too large for audio editing."));}
    let src = source.source();
    let length = ((source.frames() as f64*rate as f64/src.rate as f64).round() as usize).max(1);
    let start = (edit.start_seconds*rate as f64).round() as usize;
    if start >= length {
        return Err(fail(format!("Starts at {:.2} s is past the end of your file ({:.2} s). Choose an earlier start.", edit.start_seconds, source.duration())));
    }
    let taken = frames.min(length-start);
    let mut values = converted_range(src, rate, channels, start, taken, cancel)?;
    values.resize(frames*channels, 0.);
    cancel.check()?;
    let looping = meta.loop_start >= 0;
    let wanted = |seconds: f64| (seconds*rate as f64).round() as usize;
    let (mut fade_in, mut fade_out) = (wanted(edit.fade_in_seconds), wanted(edit.fade_out_seconds));
    let (mut in_short, mut out_short) = (fade_in > taken, fade_out > taken);
    fade_in = fade_in.min(taken);
    fade_out = fade_out.min(taken);
    if looping {
        let loop_start = meta.loop_start as usize;
        if fade_in > loop_start {fade_in = loop_start; in_short = true;}
        if fade_out > 0 {fade_out = 0; out_short = true;}
    }
    for frame in 0..fade_in {
        let factor = frame as f32/fade_in as f32;
        for v in &mut values[frame*channels..(frame+1)*channels] {*v *= factor;}
    }
    for k in 0..fade_out {
        let frame = taken-1-k;
        let factor = k as f32/fade_out as f32;
        for v in &mut values[frame*channels..(frame+1)*channels] {*v *= factor;}
    }
    let peak = values.iter().fold(0f32,|a,b|a.max(b.abs()));
    let loudness_before_gain = loudness(&values[..taken*channels], channels, rate);
    let requested = volume_percent as f64/100.*10f64.powf(edit.volume_db/20.);
    let ceiling = 10f64.powf(CEILING_DB/20.);
    let limited = peak > 0. && peak as f64*requested > ceiling;
    let applied = if limited {ceiling/peak as f64} else {requested};
    let gain = applied as f32;
    for v in &mut values {*v *= gain;}
    let bits = if meta.format == "PCM8" {8} else {16};
    let (payload, clipped) = pcm(&values, bits, 100);
    cancel.check()?;
    let resource = encode_resource(original, &payload, bits)?;
    let db = |g: f64| (g > 0.).then(|| 20.*g.log10());
    Ok(Rendered {
        audio: PreparedAudio {resource, report: EditReport {
            policy: "extras-audio-gain-v1".into(), volume_percent, source_duration: source.duration(), output_duration: meta.duration,
            resampled: src.rate != rate, channels_converted: src.channels != channels,
            padded_frames: (frames-taken) as u32, trimmed_frames: (length-taken) as u32,
            clipped_samples: clipped, peak_before_gain: peak,
            window: Some(WindowReport {
                policy: WINDOW_POLICY.into(), start_frame: start as u32, file_frames: taken as u32,
                requested_gain_db: db(requested), applied_gain_db: db(applied), gain_limited: limited,
                fade_in_frames: fade_in as u32, fade_out_frames: fade_out as u32,
                fade_in_shortened: in_short, fade_out_shortened: out_short, looping,
            }),
        }},
        values, channels, rate, loudness_before_gain, peak_before_gain: peak,
    })
}

/// Decodes the user's file and renders the sound edit (the build's path).
pub fn prepare_edited_audio(original: &[u8], source: &[u8], extension: &str, volume_percent: u16, edit: &deadlock_project::customization::SoundEdit, cancel: &CancellationToken) -> Result<PreparedAudio> {
    let decoded = decode_source(source, extension, cancel)?;
    Ok(render_edit(original, &decoded, volume_percent, edit, cancel)?.audio)
}
/// The build's path for an edited row: the file's SHA-256 and the rendered
/// sound, streaming the file (`decode_source_file`).
pub fn prepare_edited_file(original: &[u8], path: &std::path::Path, extension: &str, volume_percent: u16, edit: &deadlock_project::customization::SoundEdit, cancel: &CancellationToken) -> Result<(String, PreparedAudio)> {
    let decoded = decode_source_file(path, extension, cancel)?;
    let digest = file_sha(path, cancel)?;
    Ok((digest, render_edit(original, &decoded, volume_percent, edit, cancel)?.audio))
}

/// The Volume (dB) at which the edit plays about as loud as the game's sound
/// (`game_loudness`, from `loudness`), within the Volume range and the
/// clipping protection, rounded down to 0.1 dB. None when either is silent or
/// the older Volume percent is 0.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct MatchedVolume {
    pub volume_db: f64,
    /// Lower than a match because louder would clip.
    pub limited_by_clipping: bool,
    /// Held at the end of the Volume range.
    pub limited_by_range: bool,
}
pub fn matched_volume(game_loudness: Option<f64>, rendered: &Rendered, volume_percent: u16) -> Option<MatchedVolume> {
    let (game, own) = (game_loudness?, rendered.loudness_before_gain?);
    if volume_percent == 0 {return None;}
    let percent_db = 20.*(volume_percent as f64/100.).log10();
    let wanted = game-(own+percent_db);
    let (low, high) = deadlock_project::customization::SOUND_VOLUME_DB;
    let mut db = wanted.clamp(low, high);
    let limited_by_range = db != wanted;
    let mut limited_by_clipping = false;
    if rendered.peak_before_gain > 0. {
        let most = CEILING_DB-20.*(rendered.peak_before_gain as f64).log10()-percent_db;
        if db > most {db = most.max(low); limited_by_clipping = true;}
    }
    Some(MatchedVolume {volume_db: (db*10.).floor()/10., limited_by_clipping, limited_by_range})
}

/// The largest absolute sample of each of `buckets` equal parts (all channels).
pub fn peaks(values: &[f32], channels: usize, buckets: usize) -> Vec<f32> {
    let frames = values.len()/channels.max(1);
    let buckets = buckets.clamp(1, 4096).min(frames.max(1));
    (0..buckets).map(|b| {
        let (from, to) = (b*frames/buckets, ((b+1)*frames/buckets).max(b*frames/buckets+1).min(frames));
        values[from*channels..to*channels].iter().fold(0f32,|a,v|a.max(v.abs()))
    }).collect()
}

/// A game sound's samples (-1…1, interleaved), its channels and rate.
pub fn sound_samples(raw: &[u8], cancel: &CancellationToken) -> Result<(Vec<f32>, usize, u32)> {
    let s = samples(raw, cancel)?;
    Ok((s.values, s.channels, s.rate))
}

/// Integrated loudness in LUFS as ITU-R BS.1770-4 measures it: K-weighting
/// (a high shelf and a high pass), mean square per channel summed over 400 ms
/// blocks with 75% overlap, an absolute gate at -70 LUFS and a relative gate
/// 10 LU below the gated mean. A sound shorter than one block is one block.
/// None for silence.
pub fn loudness(values: &[f32], channels: usize, rate: u32) -> Option<f64> {
    let channels = channels.max(1);
    let frames = values.len()/channels;
    if frames == 0 {return None;}
    let filters = [high_shelf(rate), high_pass(rate)];
    // Per frame: the K-weighted squares summed over channels.
    let mut energy = vec![0f64; frames];
    for channel in 0..channels {
        let mut state = [[0f64; 2]; 2];
        for frame in 0..frames {
            let mut x = values[frame*channels+channel] as f64;
            for (f, s) in filters.iter().zip(state.iter_mut()) {
                let y = f[0]*x+s[0];
                s[0] = f[1]*x-f[3]*y+s[1];
                s[1] = f[2]*x-f[4]*y;
                x = y;
            }
            energy[frame] += x*x;
        }
    }
    let mut sums = vec![0f64; frames+1];
    for (i, e) in energy.iter().enumerate() {sums[i+1] = sums[i]+e;}
    let block = ((0.4*rate as f64).round() as usize).max(1);
    let blocks: Vec<f64> = if frames <= block {vec![sums[frames]/frames as f64]} else {
        let step = (block/4).max(1);
        (0..=(frames-block)/step).map(|i| (sums[i*step+block]-sums[i*step])/block as f64).collect()
    };
    let lufs = |z: f64| -0.691+10.*z.log10();
    let gated: Vec<f64> = blocks.iter().copied().filter(|z| *z > 0. && lufs(*z) > -70.).collect();
    if gated.is_empty() {return None;}
    let relative = lufs(gated.iter().sum::<f64>()/gated.len() as f64)-10.;
    let kept: Vec<f64> = gated.into_iter().filter(|z| lufs(*z) > relative).collect();
    if kept.is_empty() {return None;}
    Some(lufs(kept.iter().sum::<f64>()/kept.len() as f64))
}
/// Normalised biquad [b0, b1, b2, a1, a2] of BS.1770's K-weighting stages at
/// `rate`, derived from the analogue prototypes; at 48 kHz they are the
/// standard's published coefficients.
fn high_shelf(rate: u32) -> [f64; 5] {
    let (f0, gain, q) = (1681.974450955533f64, 3.999843853973347f64, 0.7071752369554196f64);
    let k = (std::f64::consts::PI*f0/rate as f64).tan();
    let vh = 10f64.powf(gain/20.);
    let vb = vh.powf(0.4996667741545416);
    let a0 = 1.+k/q+k*k;
    [(vh+vb*k/q+k*k)/a0, 2.*(k*k-vh)/a0, (vh-vb*k/q+k*k)/a0, 2.*(k*k-1.)/a0, (1.-k/q+k*k)/a0]
}
fn high_pass(rate: u32) -> [f64; 5] {
    let (f0, q) = (38.13547087602444f64, 0.5003270373238773f64);
    let k = (std::f64::consts::PI*f0/rate as f64).tan();
    let a0 = 1.+k/q+k*k;
    [1., -2., 1., 2.*(k*k-1.)/a0, (1.-k/q+k*k)/a0]
}
