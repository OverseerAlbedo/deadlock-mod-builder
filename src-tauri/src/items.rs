//! Extras → Items: the game's item catalog, built when the Items tab is first
//! opened (never by `discover_extras` or at project open), and its pictures.
//! The catalog is kept per game archive (path, size and modification time of
//! `pak01_dir.vpk`), so a game update builds it again; Reload assets asks for
//! a new one. Pictures are read from the kept archive one at a time, only for
//! the tiles the page shows (`preview_extra` with kind `items`).
use crate::app::{AppError, Application, Result};
use crate::catalogs::ExtraPreviewRequest;
use deadlock_components::item_icons::{self, ItemCatalog};
use deadlock_components::item_names;
use deadlock_core::{text::casefold, CancellationToken};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

/// Messages of the Items commands (matched by `troubleshooting.rs`).
pub const ITEMS_NOT_LOADED: &str = "The item list is not loaded. Open Extras → Items again";
pub const ITEMS_GAME_CHANGED: &str = "The game files changed since the items were loaded. Click Reload assets in Extras → Items";
pub const ITEMS_NO_GAME: &str = "Choose the current Deadlock pak01_dir.vpk in Setup to see the game's items";

/// The Items page's view of renamed items (`item_names_state`).
#[derive(Debug, Clone, serde::Serialize)]
pub struct ItemNamesState {
    /// The game's item names (`found`, `names`: token → English name, `languages`).
    #[serde(flatten)]
    pub game: item_names::GameNames,
    /// Each saved row, in row order.
    pub rows: Vec<item_names::RowState>,
    /// Saved rows naming an item the game does not have now.
    pub unknown: Vec<String>,
}

struct ItemCache {
    pak: String,
    stamp: (u64, u128),
    archive: Arc<deadlock_vpk::Vpk>,
    catalog: Arc<ItemCatalog>,
    /// Casefolded target → the picture's fingerprint.
    pictures: BTreeMap<String, String>,
}
fn cache() -> &'static Mutex<Option<ItemCache>> {
    static CACHE: OnceLock<Mutex<Option<ItemCache>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}
/// Whether a catalog is kept (tests: nothing about items runs before the tab opens).
pub fn loaded() -> bool {
    cache().lock().unwrap_or_else(|e| e.into_inner()).is_some()
}
fn stamp(pak: &str) -> Result<(u64, u128)> {
    let meta = std::fs::metadata(pak).map_err(|e| AppError::validation(ITEMS_NO_GAME, e))?;
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    Ok((meta.len(), modified))
}

impl Application {
    /// The game's items for Extras → Items, kept until the game archive changes
    /// or `reload` asks for a new read.
    pub fn item_catalog(&self, reload: bool) -> Result<ItemCatalog> {
        let settings = self.settings()?;
        if settings.pak.is_empty() || !Path::new(&settings.pak).is_file() {
            return Err(AppError::validation(ITEMS_NO_GAME, "No game archive is configured."));
        }
        let current = stamp(&settings.pak)?;
        if !reload {
            let kept = cache().lock().unwrap_or_else(|e| e.into_inner());
            if let Some(kept) = kept.as_ref().filter(|c| c.pak == settings.pak && c.stamp == current) {
                return Ok((*kept.catalog).clone());
            }
        }
        // A read-only job on the game archive (about half a second): like the
        // previews it takes no operation lease, so opening Items while the
        // hero's assets load, or Reload assets (which also reloads those), never
        // fails as busy. One read at a time; a second caller gets its result.
        static READING: Mutex<()> = Mutex::new(());
        let _reading = READING.lock().unwrap_or_else(|e| e.into_inner());
        if !reload {
            let kept = cache().lock().unwrap_or_else(|e| e.into_inner());
            if let Some(kept) = kept.as_ref().filter(|c| c.pak == settings.pak && c.stamp == current) {
                return Ok((*kept.catalog).clone());
            }
        }
        let started = std::time::Instant::now();
        let cancel = CancellationToken::new();
        let catalog = {
            let archive = deadlock_vpk::Vpk::open(&settings.pak).map_err(|e| AppError::validation("Could not read the game archive", e))?;
            let names = item_icons::Names::beside(Path::new(&settings.pak));
            let catalog = item_icons::catalog(&archive, &names, &cancel)?;
            let count = |c: &str| catalog.items.iter().filter(|r| r.category == c).count();
            self.append_log(
                "info",
                &format!(
                    "Items: {} pictures (weapon {}, vitality {}, spirit {}, legendary {}, misc {}) read in {:.0} ms",
                    catalog.items.len(),
                    count("weapon"),
                    count("vitality"),
                    count("spirit"),
                    count("legendary"),
                    count("misc"),
                    started.elapsed().as_secs_f64() * 1000.
                ),
            )?;
            let pictures = catalog
                .items
                .iter()
                .filter_map(|r| r.original_sha256.as_ref().map(|h| (casefold(&r.target), h.clone())))
                .collect();
            *cache().lock().unwrap_or_else(|e| e.into_inner()) = Some(ItemCache {
                pak: settings.pak.clone(),
                stamp: current,
                archive: Arc::new(archive),
                catalog: Arc::new(catalog.clone()),
                pictures,
            });
            catalog
        };
        Ok(catalog)
    }

    /// Renamed items for the Items page: the game's item name tokens with their
    /// English names, the languages a build writes, and each saved row's state
    /// (`known` false: the game has no such item now). Read-only and lease-free;
    /// the game's names are kept per names folder by each file's size and
    /// modified time. Called by the page only (never at project open).
    pub fn item_names_state(&self, rows: &[deadlock_project::item_names::ItemNameRow]) -> Result<ItemNamesState> {
        let settings = self.settings()?;
        if settings.pak.is_empty() || !Path::new(&settings.pak).is_file() {
            return Err(AppError::validation(ITEMS_NO_GAME, "No game archive is configured."));
        }
        let archive = Path::new(&settings.pak);
        let stamp = item_names::folder_stamp(archive);
        static KEPT: Mutex<Option<(String, Vec<(String, u64, u128)>, Arc<item_names::GameNames>)>> = Mutex::new(None);
        let kept = KEPT.lock().unwrap_or_else(|e| e.into_inner()).as_ref().filter(|(pak, s, _)| *pak == settings.pak && *s == stamp).map(|(_, _, g)| g.clone());
        let game = match kept {
            Some(game) => game,
            None => {
                let game = Arc::new(item_names::game_names(archive));
                *KEPT.lock().unwrap_or_else(|e| e.into_inner()) = Some((settings.pak.clone(), stamp, game.clone()));
                game
            }
        };
        let rows = item_names::row_states(&game, rows);
        let unknown = rows.iter().filter(|r| !r.known).map(|r| r.item.clone()).collect();
        Ok(ItemNamesState { game: (*game).clone(), rows, unknown })
    }

    /// One item picture (or the user's image fitted to it) for the Items page.
    /// Read-only and bounded like the other Extras previews: no operation lease.
    pub fn preview_item_icon(&self, request: &ExtraPreviewRequest) -> Result<Vec<u8>> {
        let settings = self.settings()?;
        let target = deadlock_core::safety::relative_asset(&request.target)?;
        let (archive, expected) = {
            let kept = cache().lock().unwrap_or_else(|e| e.into_inner());
            let kept = kept.as_ref().ok_or_else(|| AppError::validation(ITEMS_NOT_LOADED, "The game's items have not been read yet."))?;
            if kept.pak != settings.pak || stamp(&settings.pak)? != kept.stamp {
                return Err(AppError::validation(ITEMS_GAME_CHANGED, &target));
            }
            let hash = kept
                .pictures
                .get(&casefold(&target))
                .ok_or_else(|| AppError::validation("Choose a picture from the game's item list", &target))?;
            (kept.archive.clone(), hash.clone())
        };
        let original = archive.read(&target).map_err(|e| AppError::validation(ITEMS_GAME_CHANGED, e))?;
        if deadlock_components::resource_fingerprint(&original) != expected {
            return Err(AppError::validation(ITEMS_GAME_CHANGED, &target));
        }
        let cancel = CancellationToken::new();
        let size = request.max_dimension.unwrap_or(512).clamp(32, 1024);
        let png = match request.source.as_deref().filter(|s| !s.is_empty()).map(Path::new) {
            Some(path) => {
                deadlock_vpk::check_unlinked_path(path, false).map_err(|e| AppError::validation("Unsupported replacement file", e))?;
                crate::intake::validate_asset_path(path, crate::intake::AssetKind::Icon)?;
                deadlock_components::icons::preview_fitted_image(path, &original, request.fit.as_deref().unwrap_or("contain"), size, &cancel)?
            }
            None => deadlock_components::icons::preview_icon(&original, size, &cancel)?,
        };
        let metadata = deadlock_components::icons::texture_metadata(&original)?;
        crate::catalogs::media("image/png", serde_json::json!({"width": metadata.display_width, "height": metadata.display_height}), png)
    }
}

#[cfg(feature = "desktop")]
pub mod commands {
    use super::*;
    type State<'a> = tauri::State<'a, Arc<Application>>;
    #[tauri::command]
    pub async fn item_catalog(state: State<'_>, reload: Option<bool>) -> Result<ItemCatalog> {
        let app = state.inner().clone();
        tauri::async_runtime::spawn_blocking(move || app.item_catalog(reload.unwrap_or(false)))
            .await
            .map_err(|e| AppError::new("internal", "The native catalog worker stopped", e))?
    }
    #[tauri::command]
    pub async fn item_names_state(state: State<'_>, rows: Vec<deadlock_project::item_names::ItemNameRow>) -> Result<ItemNamesState> {
        let app = state.inner().clone();
        tauri::async_runtime::spawn_blocking(move || app.item_names_state(&rows))
            .await
            .map_err(|e| AppError::new("internal", "The native catalog worker stopped", e))?
    }
}
