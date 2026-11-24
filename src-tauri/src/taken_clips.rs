//! Motion → Take it from another hero's animation: the commands a page uses.
//! All only read the game archive (and, for the preview, the prepared
//! character) and take no operation lease; what they read is kept per game
//! archive (path, size and modification time), as `motion_catalog` keeps the
//! clip lists.
//!
//! * `taken_clip_heroes`: every roster hero but this one, each with whether
//!   its body can be carried onto this hero (and why not).
//! * `taken_clip_check`: whether this hero's clip can be taken at all (a
//!   layer or a clip stored in a form the app does not write cannot).
//! * `taken_clip_sources`: the other hero's clips for one of this hero's
//!   clips, the same name first, then the same group and direction, each with
//!   its length and whether it can be used; and the set the clip belongs to
//!   (the dash: `dash_ground` and the four air dashes), each member matched
//!   by name, so one click can write a row for each.
//! * `taken_clip_report` / `taken_clip_preview`: the clip the build would
//!   write for a row (its report, or the Motion player's packet, the form it
//!   plays for made clips), made exactly as the build makes it; the newest
//!   request wins (an older one answers `superseded`).
use crate::app::{AppError, Application, Result};
use deadlock_components::hero_clips::{self, ClipSet, SourceClip, SourceHero, TakenReport};
use deadlock_components::motion::CatalogClip;
use deadlock_core::CancellationToken;
use deadlock_project::{customization::ResourceReplacement, Project};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

/// Messages of these commands (the page and troubleshooting compare them).
pub const TAKEN_SUPERSEDED: &str = "superseded";
pub const TAKEN_NOT_A_ROW: &str = "Choose the other hero and the clip this clip is taken from";
/// Followed by ": " and the clip.
pub const TAKEN_NOT_A_CLIP: &str = "Choose one of this hero's clips to take from another hero";

/// What `taken_clip_check` answers: why this hero's clip cannot be taken from
/// another hero (`hero_clips::TARGET_ADDITIVE` or `TARGET_NOT_FULL`), or
/// nothing when it can.
#[derive(Debug, Clone, Serialize)]
pub struct TakenClipCheck {
    pub target: String,
    pub refusal: Option<String>,
}

/// What `taken_clip_sources` answers.
#[derive(Debug, Clone, Serialize)]
pub struct TakenSources {
    pub target: String,
    pub hero: String,
    pub clips: Vec<SourceClip>,
    /// The set this hero's clip belongs to, when it is in one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub set: Option<ClipSet>,
}

type Stamp = (u64, u128);
fn stamp(path: &Path) -> Option<Stamp> {
    let meta = std::fs::metadata(path).ok()?;
    let modified = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_nanos()).unwrap_or(0);
    Some((meta.len(), modified))
}
/// The hero lists read so far, per hero (casefolded model path), for one game archive.
type Heroes = (String, Stamp, BTreeMap<String, Arc<Vec<SourceHero>>>);
fn heroes() -> &'static Mutex<Option<Heroes>> {
    static KEPT: OnceLock<Mutex<Option<Heroes>>> = OnceLock::new();
    KEPT.get_or_init(|| Mutex::new(None))
}
type Key = (String, String);
/// The last taken clip (target, row) → bytes and report, so the report and
/// the preview share one making; dropped when the game archive changes.
type Last = (String, Stamp, Key, Arc<(Vec<u8>, TakenReport)>);
static LAST: Mutex<Option<Last>> = Mutex::new(None);
static PREVIEW: Mutex<Option<CancellationToken>> = Mutex::new(None);
static REPORT: Mutex<Option<CancellationToken>> = Mutex::new(None);

fn newest(slot: &Mutex<Option<CancellationToken>>) -> CancellationToken {
    let cancel = CancellationToken::new();
    if let Some(older) = slot.lock().unwrap_or_else(|e| e.into_inner()).replace(cancel.clone()) {
        older.cancel();
    }
    cancel
}
fn superseded<T>(result: Result<T>, cancel: &CancellationToken) -> Result<T> {
    match result {
        Err(error) if error.kind == "cancelled" || cancel.is_cancelled() => Err(AppError::new("cancelled", TAKEN_SUPERSEDED, "A newer request replaced this one.")),
        other => other,
    }
}
fn taken_row(row: &ResourceReplacement) -> Result<()> {
    if row.taken.is_none() {
        return Err(AppError::validation(TAKEN_NOT_A_ROW, "The row names no other hero's clip."));
    }
    row.check_taken("animations").map_err(|e| AppError::validation("This clip cannot be taken from another hero", e))
}
fn clips_of(list: &crate::clips::MotionCatalog) -> Vec<CatalogClip> {
    list.animations.iter().map(|a| CatalogClip { row: a.row.clone(), details: a.details.clone() }).collect()
}

impl Application {
    fn archive_stamp(&self) -> Result<(String, Stamp)> {
        let settings = self.settings()?;
        let current = stamp(Path::new(&settings.pak)).filter(|_| !settings.pak.is_empty()).ok_or_else(|| AppError::validation(crate::clips::CLIPS_NO_GAME, "No game archive is configured."))?;
        Ok((settings.pak, current))
    }

    /// The heroes a clip can be taken from for this project's hero (lease-free, kept per archive).
    pub fn taken_clip_heroes(&self, project: Project) -> Result<Vec<SourceHero>> {
        let (pak, current) = self.archive_stamp()?;
        let hero = deadlock_core::text::casefold(&project.original_model);
        {
            let kept = heroes().lock().unwrap_or_else(|e| e.into_inner());
            if let Some(list) = kept.as_ref().filter(|(p, s, _)| *p == pak && *s == current).and_then(|(_, _, h)| h.get(&hero)) {
                return Ok((**list).clone());
            }
        }
        let archive = self.clip_archive()?;
        let list = Arc::new(hero_clips::source_heroes(&archive, &project.original_model, &CancellationToken::new())?);
        let mut kept = heroes().lock().unwrap_or_else(|e| e.into_inner());
        if !kept.as_ref().is_some_and(|(p, s, _)| *p == pak && *s == current) {
            *kept = Some((pak, current, BTreeMap::new()));
        }
        if let Some((_, _, h)) = kept.as_mut() {
            h.insert(hero, list.clone());
        }
        Ok((*list).clone())
    }

    /// Whether this hero's clip `target` can be taken from another hero at all
    /// (lease-free; asked when the Motion page opens the third way).
    pub fn taken_clip_check(&self, project: Project, target: String) -> Result<TakenClipCheck> {
        let own = clips_of(&*self.hero_catalog(&project.original_model, false)?);
        let wanted = deadlock_core::text::casefold(&target);
        let chosen = own.iter().find(|c| deadlock_core::text::casefold(&c.row.target) == wanted).ok_or_else(|| AppError::validation(&format!("{TAKEN_NOT_A_CLIP}: {target}"), "The clip is not in the hero's list."))?;
        let refusal = hero_clips::target_refusal(&*self.clip_archive()?, &chosen.row.target)?.map(str::to_string);
        Ok(TakenClipCheck { target, refusal })
    }

    /// The other hero's clips for this hero's clip `target`, and its set (lease-free).
    pub fn taken_clip_sources(&self, project: Project, target: String, hero: String) -> Result<TakenSources> {
        let own = clips_of(&*self.hero_catalog(&project.original_model, false)?);
        let wanted = deadlock_core::text::casefold(&target);
        let chosen = own.iter().find(|c| deadlock_core::text::casefold(&c.row.target) == wanted).ok_or_else(|| AppError::validation(&format!("{TAKEN_NOT_A_CLIP}: {target}"), "The clip is not in the hero's list."))?;
        if deadlock_core::text::casefold(&deadlock_components::compiled(&hero)?) == deadlock_core::text::casefold(&deadlock_components::compiled(&project.original_model)?) {
            return Err(AppError::validation(hero_clips::SAME_HERO, &hero));
        }
        let known = deadlock_components::heroes::roster()?.iter().any(|h| deadlock_components::compiled(&h.path).ok().map(|p| deadlock_core::text::casefold(&p)) == deadlock_components::compiled(&hero).ok().map(|p| deadlock_core::text::casefold(&p)));
        if !known || !self.clip_archive()?.entries.contains_key(&deadlock_core::text::casefold(&deadlock_components::compiled(&hero)?)) {
            return Err(AppError::validation(&format!("{}: {hero}", hero_clips::NO_SOURCE_HERO), "The hero is not in the roster or not in this game."));
        }
        let other = clips_of(&*self.hero_catalog(&hero, false)?);
        Ok(TakenSources { clips: hero_clips::source_clips(chosen, &other), set: hero_clips::clip_set(&chosen.row.label, &own, &other), target, hero })
    }

    /// The clip the build would write for `row` (bytes and report), kept for the last row.
    fn taken_clip(&self, project: &Project, target: &str, row: &ResourceReplacement, cancel: &CancellationToken) -> Result<(Vec<u8>, TakenReport)> {
        taken_row(row)?;
        let (pak, current) = self.archive_stamp()?;
        let key = (deadlock_core::text::casefold(target), serde_json::to_string(row).unwrap_or_default());
        if let Some((p, s, held, value)) = LAST.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            if *p == pak && *s == current && *held == key {
                return Ok((value.0.clone(), value.1.clone()));
            }
        }
        let archive = self.clip_archive()?;
        let taken = row.taken.as_ref().expect("checked");
        cancel.check()?;
        let made = hero_clips::author_row(&archive, &project.original_model, target, taken, cancel)?;
        let report = hero_clips::report(&archive, taken, &made)?;
        let value = Arc::new((made.bytes, report));
        *LAST.lock().unwrap_or_else(|e| e.into_inner()) = Some((pak, current, key, value.clone()));
        Ok((value.0.clone(), value.1.clone()))
    }

    /// The report of the clip `row` makes (no lease, newest wins).
    pub fn taken_clip_report(&self, project: Project, target: String, row: ResourceReplacement) -> Result<TakenReport> {
        let cancel = newest(&REPORT);
        superseded(self.taken_clip(&project, &target, &row, &cancel).map(|(_, report)| report), &cancel)
    }

    /// The clip `row` makes, played on the prepared character (the Motion
    /// player's packet, as for made clips; no lease, newest wins).
    pub fn taken_clip_preview(&self, project: Project, target: String, row: ResourceReplacement) -> Result<Vec<u8>> {
        let cancel = newest(&PREVIEW);
        let result = (|| {
            let (bytes, _) = self.taken_clip(&project, &target, &row, &cancel)?;
            let archive = self.clip_archive()?;
            let character = self.clip_character(&project, &cancel)?;
            Ok(deadlock_pipeline::clip_preview::packet(&archive, &character, &target, deadlock_pipeline::clip_preview::Source::Taken(&bytes), &cancel)?)
        })();
        superseded(result, &cancel)
    }
}

#[cfg(feature = "desktop")]
pub mod commands {
    use super::*;
    type State<'a> = tauri::State<'a, Arc<Application>>;
    async fn background<T: Send + 'static>(work: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
        tauri::async_runtime::spawn_blocking(work).await.map_err(|e| AppError::new("internal", "The native clip worker stopped", e))?
    }
    #[tauri::command]
    pub async fn taken_clip_heroes(state: State<'_>, project: Project) -> Result<Vec<SourceHero>> {
        let app = state.inner().clone();
        background(move || app.taken_clip_heroes(project)).await
    }
    #[tauri::command]
    pub async fn taken_clip_check(state: State<'_>, project: Project, target: String) -> Result<TakenClipCheck> {
        let app = state.inner().clone();
        background(move || app.taken_clip_check(project, target)).await
    }
    #[tauri::command]
    pub async fn taken_clip_sources(state: State<'_>, project: Project, target: String, hero: String) -> Result<TakenSources> {
        let app = state.inner().clone();
        background(move || app.taken_clip_sources(project, target, hero)).await
    }
    #[tauri::command]
    pub async fn taken_clip_report(state: State<'_>, project: Project, target: String, row: ResourceReplacement) -> Result<TakenReport> {
        let app = state.inner().clone();
        background(move || app.taken_clip_report(project, target, row)).await
    }
    #[tauri::command]
    pub async fn taken_clip_preview(state: State<'_>, project: Project, target: String, row: ResourceReplacement) -> Result<tauri::ipc::Response> {
        let app = state.inner().clone();
        background(move || app.taken_clip_preview(project, target, row)).await.map(tauri::ipc::Response::new)
    }
}
