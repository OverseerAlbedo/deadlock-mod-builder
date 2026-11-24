//! The Motion page's commands: the hero's clip list, the fit checks of a
//! replacement file and the clip played on the prepared character. All only read (the game
//! archive, the prepared DMX, the chosen file) and take no operation lease, so
//! selecting a clip or a file while a build or Analyze runs never fails as
//! busy. The game archive and the prepared character are kept between calls
//! (by path, size and modification time), so a click on the next clip reads
//! only that clip.
use crate::app::{AppError, Application, Result};
use deadlock_core::CancellationToken;
use deadlock_pipeline::clip_preview::{self, Character};
use deadlock_project::Project;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

/// The message when no game archive is set.
pub const CLIPS_NO_GAME: &str = "Choose the current Deadlock pak01_dir.vpk in Setup to see the hero's animations";

type Stamp = (u64, u128);
fn stamp(path: &Path) -> Option<Stamp> {
    let meta = std::fs::metadata(path).ok()?;
    let modified = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_nanos()).unwrap_or(0);
    Some((meta.len(), modified))
}
struct Kept<T> {
    path: String,
    stamp: Stamp,
    value: Arc<T>,
}
fn archives() -> &'static Mutex<Option<Kept<deadlock_vpk::Vpk>>> {
    static KEPT: OnceLock<Mutex<Option<Kept<deadlock_vpk::Vpk>>>> = OnceLock::new();
    KEPT.get_or_init(|| Mutex::new(None))
}
/// The clip lists read so far, per hero (casefolded model path), for the kept
/// game archive (its path and stamp): a game update reads them again.
type Lists = (String, Stamp, std::collections::BTreeMap<String, Arc<MotionCatalog>>);
fn lists() -> &'static Mutex<Option<Lists>> {
    static KEPT: OnceLock<Mutex<Option<Lists>>> = OnceLock::new();
    KEPT.get_or_init(|| Mutex::new(None))
}
/// The Motion page's list of the hero's clips (the catalog's animation rows,
/// without icons, effects or sounds).
#[derive(Debug, Clone, serde::Serialize)]
pub struct MotionCatalog {
    pub hero_path: String,
    pub animations: Vec<deadlock_components::service::AnimationDiscovery>,
}
fn characters() -> &'static Mutex<Option<Kept<Character>>> {
    static KEPT: OnceLock<Mutex<Option<Kept<Character>>>> = OnceLock::new();
    KEPT.get_or_init(|| Mutex::new(None))
}

impl Application {
    pub(crate) fn clip_archive(&self) -> Result<Arc<deadlock_vpk::Vpk>> {
        let settings = self.settings()?;
        let pak = Path::new(&settings.pak);
        let current = stamp(pak).filter(|_| !settings.pak.is_empty()).ok_or_else(|| AppError::validation(CLIPS_NO_GAME, "No game archive is configured."))?;
        let mut kept = archives().lock().unwrap_or_else(|e| e.into_inner());
        if let Some(kept) = kept.as_ref().filter(|k| k.path == settings.pak && k.stamp == current) {
            return Ok(kept.value.clone());
        }
        let archive = Arc::new(deadlock_vpk::Vpk::open(pak).map_err(|e| AppError::validation("Could not read the game archive", e))?);
        *kept = Some(Kept { path: settings.pak.clone(), stamp: current, value: archive.clone() });
        Ok(archive)
    }
    pub(crate) fn clip_character(&self, project: &Project, cancel: &CancellationToken) -> Result<Arc<Character>> {
        let dmx = Path::new(&project.dmx);
        let current = stamp(dmx).filter(|_| !project.dmx.is_empty()).ok_or_else(|| AppError::validation("Prepare the model first: the clip plays on the prepared character", "No prepared model."))?;
        {
            let kept = characters().lock().unwrap_or_else(|e| e.into_inner());
            if let Some(kept) = kept.as_ref().filter(|k| k.path == project.dmx && k.stamp == current) {
                return Ok(kept.value.clone());
            }
        }
        let character = Arc::new(Character::load(dmx, cancel)?);
        *characters().lock().unwrap_or_else(|e| e.into_inner()) = Some(Kept { path: project.dmx.clone(), stamp: current, value: character.clone() });
        Ok(character)
    }

    /// The hero's clips for the Motion page (`motion::catalog`: exactly the
    /// rows the build accepts, with names and groups). Read-only and lease-free;
    /// kept per game archive and hero, so opening the page again is instant.
    /// One read at a time; a second caller gets the first one's result.
    pub fn motion_catalog(&self, project: Project, reload: bool) -> Result<MotionCatalog> {
        Ok((*self.hero_catalog(&project.original_model, reload)?).clone())
    }

    /// `motion_catalog` for any hero's model (the Motion page's own hero, or
    /// the hero a clip is taken from), kept the same way.
    pub(crate) fn hero_catalog(&self, model: &str, reload: bool) -> Result<Arc<MotionCatalog>> {
        let settings = self.settings()?;
        let current = stamp(Path::new(&settings.pak)).filter(|_| !settings.pak.is_empty()).ok_or_else(|| AppError::validation(CLIPS_NO_GAME, "No game archive is configured."))?;
        let hero = deadlock_core::text::casefold(model);
        let held = |kept: &Option<Lists>| kept.as_ref().filter(|(pak, s, _)| *pak == settings.pak && *s == current).and_then(|(_, _, heroes)| heroes.get(&hero).cloned());
        if !reload {
            if let Some(list) = held(&lists().lock().unwrap_or_else(|e| e.into_inner())) {
                return Ok(list);
            }
        }
        static READING: Mutex<()> = Mutex::new(());
        let _reading = READING.lock().unwrap_or_else(|e| e.into_inner());
        if !reload {
            if let Some(list) = held(&lists().lock().unwrap_or_else(|e| e.into_inner())) {
                return Ok(list);
            }
        }
        let started = std::time::Instant::now();
        let archive = self.clip_archive()?;
        let cancel = CancellationToken::new();
        let log = |line: &str| {
            let _ = self.append_log("info", line);
        };
        let clips = deadlock_components::motion::catalog(&archive, model, &cancel, log)?;
        let list = Arc::new(MotionCatalog {
            hero_path: model.to_string(),
            animations: clips
                .into_iter()
                .map(|clip| deadlock_components::service::AnimationDiscovery { path: clip.row.target.clone(), row: clip.row, details: clip.details, supported: true })
                .collect(),
        });
        let _ = self.append_log("info", &format!("Motion: {} clips listed in {:.0} ms", list.animations.len(), started.elapsed().as_secs_f64() * 1000.));
        let mut kept = lists().lock().unwrap_or_else(|e| e.into_inner());
        if !kept.as_ref().is_some_and(|(pak, s, _)| *pak == settings.pak && *s == current) {
            *kept = Some((settings.pak.clone(), current, Default::default()));
        }
        if let Some((_, _, heroes)) = kept.as_mut() {
            heroes.insert(hero, list.clone());
        }
        Ok(list)
    }

    /// Whether `source` fits the hero's clip `target`, as the build's checks.
    pub fn clip_fit(&self, project: Project, target: String, source: String) -> Result<deadlock_components::motion::FitReport> {
        let archive = self.clip_archive()?;
        let cancel = CancellationToken::new();
        let path = Path::new(&source);
        deadlock_vpk::check_unlinked_path(path, false).map_err(|e| AppError::validation("Unsupported replacement file", e))?;
        Ok(deadlock_components::motion::fit(&archive, &project.original_model, &target, path, &cancel)?)
    }

    /// One clip on the prepared character (`which`: "game", or "yours" with
    /// `source`), in the movement preview's packet form.
    pub fn clip_preview(&self, project: Project, target: String, which: String, source: Option<String>) -> Result<Vec<u8>> {
        let started = std::time::Instant::now();
        let archive = self.clip_archive()?;
        let cancel = CancellationToken::new();
        let character = self.clip_character(&project, &cancel)?;
        let source_path = source.as_deref().filter(|s| !s.is_empty()).map(Path::new);
        let played = match (which.as_str(), source_path) {
            ("game", _) => clip_preview::Source::Game,
            ("yours", Some(path)) => {
                deadlock_vpk::check_unlinked_path(path, false).map_err(|e| AppError::validation("Unsupported replacement file", e))?;
                let ext = path.extension().and_then(|v| v.to_str()).unwrap_or_default().to_ascii_lowercase();
                match ext.as_str() {
                    "vnmclip_c" => clip_preview::Source::Compiled(path),
                    "vnmclip" => clip_preview::Source::Authored(path),
                    _ => return Err(AppError::validation(deadlock_components::motion::UNSUPPORTED_SOURCE, path.display().to_string())),
                }
            }
            _ => return Err(AppError::validation("Choose the game's clip or your replacement to play", which)),
        };
        let packet = clip_preview::packet(&archive, &character, &target, played, &cancel)?;
        let _ = self.append_log("info", &format!("Motion: {} clip packet for {target} ({} KB) in {:.0} ms", which, packet.len() / 1024, started.elapsed().as_secs_f64() * 1000.));
        Ok(packet)
    }
}

#[cfg(feature = "desktop")]
pub mod commands {
    use super::*;
    type State<'a> = tauri::State<'a, Arc<Application>>;
    #[tauri::command]
    pub async fn motion_catalog(state: State<'_>, project: Project, reload: Option<bool>) -> Result<MotionCatalog> {
        let app = state.inner().clone();
        tauri::async_runtime::spawn_blocking(move || app.motion_catalog(project, reload.unwrap_or(false)))
            .await
            .map_err(|e| AppError::new("internal", "The native clip worker stopped", e))?
    }
    #[tauri::command]
    pub async fn clip_fit(state: State<'_>, project: Project, target: String, source: String) -> Result<deadlock_components::motion::FitReport> {
        let app = state.inner().clone();
        tauri::async_runtime::spawn_blocking(move || app.clip_fit(project, target, source))
            .await
            .map_err(|e| AppError::new("internal", "The native clip worker stopped", e))?
    }
    #[tauri::command]
    pub async fn clip_preview(state: State<'_>, project: Project, target: String, which: String, source: Option<String>) -> Result<tauri::ipc::Response> {
        let app = state.inner().clone();
        tauri::async_runtime::spawn_blocking(move || app.clip_preview(project, target, which, source))
            .await
            .map_err(|e| AppError::new("internal", "The native clip worker stopped", e))?
            .map(tauri::ipc::Response::new)
    }
}
