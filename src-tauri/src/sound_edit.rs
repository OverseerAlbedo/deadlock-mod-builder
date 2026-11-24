//! Extras → Sounds editor: the rendered result of a sound replacement row
//! (its file, the part of it that plays, its volume and fades) as the build
//! writes it, with waveform peaks of the file, the result and the game's sound.
//! Read-only: no operation lease, and the newest request wins (an older one
//! still running is cancelled and answers `superseded`).
use crate::app::{AppError, Application, Result};
use deadlock_components::audio;
use deadlock_core::CancellationToken;
use deadlock_project::customization::{ResourceReplacement, SoundEdit};
use serde::Deserialize;
use std::{path::{Path, PathBuf}, sync::{Arc, Mutex}, time::SystemTime};

#[derive(Debug, Clone, Deserialize)]
pub struct SoundEditRequest {
    pub hero_path: String,
    /// The saved row as it is (or would be) in the project.
    pub row: ResourceReplacement,
    /// Waveform buckets of each waveform (default 480).
    pub buckets: Option<usize>,
}

/// Message of a request a newer one replaced (the page ignores it).
pub const SUPERSEDED: &str = "superseded";
/// The refusal of a file too long to be used without a sound edit.
pub const LONG_FILE: &str = "This file is longer than the game's sound: choose the part that plays.";

type SourceKey = (PathBuf, u64, Option<SystemTime>);
/// The last decoded file and its peaks per bucket count.
static SOURCE: Mutex<Option<(SourceKey, Arc<audio::SourceAudio>)>> = Mutex::new(None);
static CURRENT: Mutex<Option<CancellationToken>> = Mutex::new(None);

fn source_audio(path: &Path, extension: &str, cancel: &CancellationToken) -> Result<Arc<audio::SourceAudio>> {
    let meta = std::fs::metadata(path).map_err(|e| AppError::io("Could not read replacement file", e))?;
    let key = (path.to_path_buf(), meta.len(), meta.modified().ok());
    if let Some((held, decoded)) = SOURCE.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        if *held == key {
            return Ok(decoded.clone());
        }
    }
    // Streamed from disk: a song can be far larger than the 128 MiB a file
    // without an edit may be (`audio::EDIT_MAX_MINUTES` is the editor's own limit).
    let decoded = Arc::new(audio::decode_source_file(path, extension, cancel)?);
    *SOURCE.lock().unwrap_or_else(|e| e.into_inner()) = Some((key, decoded.clone()));
    Ok(decoded)
}

fn rounded(values: Vec<f32>) -> Vec<f32> {
    values.into_iter().map(|v| (v.min(1.) * 1000.).round() / 1000.).collect()
}

impl Application {
    pub fn sound_edit_preview(&self, request: SoundEditRequest) -> Result<Vec<u8>> {
        let cancel = CancellationToken::new();
        if let Some(older) = CURRENT.lock().unwrap_or_else(|e| e.into_inner()).replace(cancel.clone()) {
            older.cancel();
        }
        let result = self.render_preview(&request, &cancel);
        match result {
            Err(error) if error.kind == "cancelled" || cancel.is_cancelled() => Err(AppError::new("cancelled", SUPERSEDED, "A newer preview replaced this one.")),
            other => other,
        }
    }

    fn render_preview(&self, request: &SoundEditRequest, cancel: &CancellationToken) -> Result<Vec<u8>> {
        let row = &request.row;
        row.check_sound_edit("audio").map_err(|e| AppError::validation("This sound edit cannot be used", e))?;
        let buckets = request.buckets.unwrap_or(480).clamp(32, 2048);
        let (_, original) = self.extras_original(&request.hero_path, row.target.as_deref().unwrap_or_default(), "audio")?;
        let source = row.source.as_deref().unwrap_or_default().trim();
        if source.is_empty() {
            return Err(AppError::validation("Choose a replacement file first", "This sound has no file."));
        }
        let path = Path::new(source);
        deadlock_vpk::check_unlinked_path(path, false).map_err(|e| AppError::validation("Unsupported replacement file", e))?;
        crate::intake::validate_asset_path(path, crate::intake::AssetKind::Audio)?;
        let extension = path.extension().and_then(|s| s.to_str()).unwrap_or_default().to_ascii_lowercase();
        let meta = audio::metadata(&original)?;
        let (game_values, game_channels, game_rate) = audio::sound_samples(&original, cancel)?;
        let game_loudness = audio::loudness(&game_values, game_channels, game_rate);
        let decoded = source_audio(path, &extension, cancel)?;
        cancel.check()?;
        let percent = row.volume_percent.unwrap_or(100);
        // Match the game's loudness creates an edit, so it is measured on one.
        let edit = row.sound_edit();
        let probe = audio::render_edit(&original, &decoded, percent, &edit.unwrap_or(SoundEdit::default()), cancel);
        let matched = probe.as_ref().ok().and_then(|r| audio::matched_volume(game_loudness, r, percent));
        // The result exactly as the build writes it (`resources::stage`).
        let rendered = match edit {
            Some(_) => probe.map(|r| r.audio),
            // A file the old encoder cannot take (over 128 MiB or 16 million
            // samples) is refused as the build would, without reading it again.
            None if std::fs::metadata(path).map(|m| m.len()).unwrap_or(u64::MAX) > deadlock_components::MAX_RESOURCE_BYTES as u64
                || decoded.frames().saturating_mul(decoded.channels()) > audio::LEGACY_MAX_SAMPLES =>
                Err(deadlock_core::CoreError::Integrity(LONG_FILE.into())),
            None => {
                let raw = crate::catalogs::preview_file(path)?;
                audio::prepare_audio(&original, &raw, &extension, percent, row.audio_fit.as_deref().unwrap_or("exact"), cancel)
            }
        };
        cancel.check()?;
        let (result, wave, refusal) = match rendered {
            Ok(prepared) => {
                let (values, channels, rate) = audio::sound_samples(&prepared.resource, cancel)?;
                let wave = audio::preview_audio(&prepared.resource, cancel)?;
                (serde_json::json!({
                    "peaks": rounded(audio::peaks(&values, channels, buckets)),
                    "loudness": audio::loudness(&values, channels, rate),
                    "report": prepared.report,
                }), wave, None)
            }
            Err(deadlock_core::CoreError::Cancelled) => return Err(deadlock_core::CoreError::Cancelled.into()),
            Err(error) => (serde_json::Value::Null, vec![], Some(error.to_string().trim_start_matches("Integrity check failed: ").to_string())),
        };
        let percent_db = (percent > 0).then(|| 20. * (percent as f64 / 100.).log10());
        let details = serde_json::json!({
            "edited": edit.is_some(),
            "source": {"duration": decoded.duration(), "rate": decoded.rate(), "channels": decoded.channels(), "peaks": rounded(decoded.peaks(buckets))},
            "game": {"duration": meta.duration, "rate": meta.rate, "channels": meta.channels, "frames": meta.frames,
                "loop_start": meta.loop_start, "loop_end": meta.loop_end, "loudness": game_loudness,
                "peaks": rounded(audio::peaks(&game_values, game_channels, buckets))},
            "result": result,
            "refusal": refusal,
            "matched": matched,
            "percent": percent,
            "percent_db": percent_db,
        });
        crate::catalogs::media("audio/wav", details, wave)
    }
}

#[cfg(feature = "desktop")]
pub mod commands {
    use super::*;
    type State<'a> = tauri::State<'a, Arc<Application>>;
    #[tauri::command]
    pub async fn sound_edit_preview(state: State<'_>, request: SoundEditRequest) -> Result<tauri::ipc::Response> {
        let app = state.inner().clone();
        tauri::async_runtime::spawn_blocking(move || app.sound_edit_preview(request))
            .await
            .map_err(|e| AppError::new("internal", "The native sound worker stopped", e))?
            .map(tauri::ipc::Response::new)
    }
}
