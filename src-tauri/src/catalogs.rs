//! Read-only current-game catalogs and bounded local image thumbnails.
use crate::app::{AppError, Application, Result};
use deadlock_components::service::{ComponentCatalog, ComponentService};
use deadlock_components::variants::Variant;
use deadlock_project::Project;
use std::path::Path;
use std::{collections::BTreeMap, sync::{Arc, Mutex}};
use deadlock_components::effect_colors::TreeEdit;
use deadlock_components::effect_trees::{EffectPreview, EffectTrees, HeroEffects, PickRequest};
use serde::{Deserialize, Serialize};
use deadlock_core::{CancellationToken, text::casefold};

pub(crate) struct PreviewCatalog {
    pak: String,
    hero: String,
    archive: Arc<deadlock_vpk::Vpk>,
    assets: BTreeMap<String, (String, String)>,
    /// The Colors catalog rows, for the hero's 3D view (`hero_scene`).
    materials: Vec<deadlock_components::materials::MaterialRow>,
    /// The Effects catalog rows, for the effect editor's trees (`effect_trees`).
    effect_rows: Arc<Vec<deadlock_components::resources::CatalogRow>>,
    /// The hero's decoded effect trees with the game archive's identity
    /// (`effect_catalog::archive_key`) they were read from. Made by the first
    /// `effect_trees` after the assets were loaded; kept when the assets are
    /// loaded again for the same archive file, hero, archive identity and
    /// Effects rows (`kept_effects`), empty again for another hero or game archive.
    effects: Arc<Mutex<Option<(String, Arc<HeroEffects>)>>>,
    /// The game archive's identity when the assets were loaded.
    archive_key: String,
}

/// What a loaded catalog was read for: the game archive file, the hero, the
/// archive's identity and the Effects rows the trees are grouped from.
struct CatalogIdentity<'a> {
    pak: &'a str,
    hero: &'a str,
    archive_key: &'a str,
    effect_rows: &'a [deadlock_components::resources::CatalogRow],
}

impl CatalogIdentity<'_> {
    fn same(&self, other: &CatalogIdentity<'_>) -> bool {
        self.pak == other.pak
            && self.hero == other.hero
            && self.archive_key == other.archive_key
            && serde_json::to_value(self.effect_rows).ok() == serde_json::to_value(other.effect_rows).ok()
    }
}

/// The effects slot of a catalog loaded again: the old catalog's slot (and the
/// trees it holds) when it was read for the same archive file, hero, archive
/// identity and Effects rows, so opening Extras → Effects again does not decode
/// the trees again; else an empty slot.
fn kept_effects<T>(old: Option<(CatalogIdentity<'_>, &Arc<Mutex<Option<T>>>)>, new: CatalogIdentity<'_>) -> Arc<Mutex<Option<T>>> {
    match old {
        Some((identity, slot)) if identity.same(&new) => slot.clone(),
        _ => Arc::new(Mutex::new(None)),
    }
}

/// Messages of the effect editor's commands (matched by `troubleshooting.rs`).
pub const EFFECTS_NOT_LOADED: &str = "The effect list is not loaded. Open Extras → Effects again";
pub const EFFECTS_GAME_CHANGED: &str = "The game files changed since the effects were loaded. Click Reload assets in Extras";

#[derive(Debug, Clone, Deserialize)]
pub struct ExtraPreviewRequest {
    pub hero_path: String,
    pub target: String,
    pub kind: String,
    pub source: Option<String>,
    pub fit: Option<String>,
    pub audio_fit: Option<String>,
    pub max_dimension: Option<u32>,
}

#[derive(Serialize)]
struct MediaHeader { mime_type: String, details: serde_json::Value }

pub(crate) fn media(mime: &str, details: serde_json::Value, data: Vec<u8>) -> Result<Vec<u8>> {
    let header = serde_json::to_vec(&MediaHeader {mime_type:mime.into(), details})
        .map_err(|e| AppError::new("internal", "Could not encode the preview", e))?;
    let mut bytes = Vec::with_capacity(4+header.len()+data.len());
    bytes.extend_from_slice(&(header.len() as u32).to_le_bytes());bytes.extend(header);bytes.extend(data);Ok(bytes)
}

impl Application {
    pub fn discover_components(&self, project: Project) -> Result<ComponentCatalog> {
        self.discover_components_selected(project, false)
    }
    pub fn discover_extras(&self, project: Project) -> Result<ComponentCatalog> {
        self.discover_components_selected(project, true)
    }
    fn discover_components_selected(&self, project: Project, extras_only: bool) -> Result<ComponentCatalog> {
        let (catalog, _) = self.operation("Find character assets", |clock, cancel| {
            project
                .validate_action()
                .map_err(|e| AppError::validation("Select a valid character project", e))?;
            let settings = self.settings()?;
            let (runner, _observer) = self.create_runner(clock, cancel)?;
            let log = |line: &str| {
                let _ = self.append_log("info", line);
            };
            let service = ComponentService {
                settings: &settings,
                runner: &runner,
                cancel,
                clock,
                log: &log,
            };
            let catalog = if extras_only {service.discover_extras(&project)?} else {service.discover(&project)?};
            cancel.check()?;
            let mut assets = BTreeMap::new();
            for row in &catalog.icons {
                if let Some(hash) = &row.original_sha256 {assets.insert(casefold(&row.target), ("icons".into(), hash.clone()));}
            }
            for (kind, rows) in [("audio", &catalog.audio), ("effects", &catalog.effects)] {
                for row in rows {
                    if let Some(hash) = &row.original_sha256 {assets.insert(casefold(&row.target), (kind.into(), hash.clone()));}
                }
            }
            for row in &catalog.materials {
                assets.insert(casefold(&row.target), ("materials".into(), row.original_sha256.clone()));
            }
            let archive = deadlock_vpk::Vpk::open(&settings.pak)
                .map_err(|e| AppError::validation("Could not open the character preview catalog", e))?;
            let archive_key = deadlock_components::effect_catalog::archive_key(&archive);
            let mut cache = self.extras_catalog.lock().unwrap_or_else(|e| e.into_inner());
            let effects = kept_effects(
                cache.as_ref().map(|old| (CatalogIdentity {pak:&old.pak, hero:&old.hero, archive_key:&old.archive_key, effect_rows:&old.effect_rows}, &old.effects)),
                CatalogIdentity {pak:&settings.pak, hero:&project.original_model, archive_key:&archive_key, effect_rows:&catalog.effects},
            );
            *cache = Some(PreviewCatalog {
                pak:settings.pak.clone(),hero:project.original_model.clone(),archive:Arc::new(archive),assets,materials:catalog.materials.clone(),
                archive_key,
                effect_rows:Arc::new(catalog.effects.clone()),effects,
            });
            Ok(catalog)
        })?;
        Ok(catalog)
    }
    pub fn discover_variants(&self, project: Project) -> Result<Vec<Variant>> {
        let (variants, _) = self.operation("Find alternate character forms", |clock, cancel| {
            project
                .validate_action()
                .map_err(|e| AppError::validation("Select a valid character project", e))?;
            let settings = self.settings()?;
            self.step(clock, "Read current game model catalog", || {
                cancel.check()?;
                if settings.pak.is_empty() {
                    return Err(AppError::validation(
                        "Choose the original game VPK in Setup",
                        "No game VPK is configured.",
                    ));
                }
                let archive = deadlock_vpk::Vpk::open(&settings.pak)
                    .map_err(|e| AppError::validation("Could not read current game VPK", e))?;
                let original = deadlock_components::compiled(&project.original_model)?;
                if !archive
                    .entries
                    .contains_key(&deadlock_core::text::casefold(&original))
                {
                    return Err(AppError::validation(
                        "Selected hero is missing from the current game VPK",
                        original,
                    ));
                }
                let variants = deadlock_components::variants::discover_variants(
                    &archive,
                    &project.original_model,
                )?;
                cancel.check()?;
                Ok(variants)
            })
        })?;
        Ok(variants)
    }
    pub fn discover_weapon_models(&self, project: Project) -> Result<Vec<Variant>> {
        let (weapons, _) = self.operation("Find the hero's separate weapon models", |clock, cancel| {
            project
                .validate_action()
                .map_err(|e| AppError::validation("Select a valid character project", e))?;
            let settings = self.settings()?;
            self.step(clock, "Search the game models for weapons", || {
                cancel.check()?;
                if settings.pak.is_empty() {
                    return Err(AppError::validation(
                        "Choose the original game VPK in Setup",
                        "No game VPK is configured.",
                    ));
                }
                let archive = deadlock_vpk::Vpk::open(&settings.pak)
                    .map_err(|e| AppError::validation("Could not read current game VPK", e))?;
                let weapons = deadlock_components::variants::discover_weapons(&archive, &project.original_model)?;
                cancel.check()?;
                Ok(weapons)
            })
        })?;
        Ok(weapons)
    }
    pub fn preview_image(&self, path: &Path, max_dimension: u32) -> Result<Vec<u8>> {
        let (png, _) = self.waiting_operation("Preview local image", |clock, cancel| {
            self.step(clock, "Validate thumbnail image input", || {
                // Reject links before intake canonicalization resolves them.
                deadlock_vpk::check_unlinked_path(path, false)
                    .map_err(|e| AppError::validation("Unsupported image input", e))?;
                crate::intake::validate_asset_path(path, crate::intake::AssetKind::Icon)
            })?;
            self.step(clock, "Decode, orient and resize thumbnail", || {
                let png = deadlock_components::icons::preview_image(path, max_dimension, cancel)?;
                cancel.check()?;
                Ok(png)
            })
        })?;
        Ok(png)
    }

    /// Thumbnails are bounded, read-only jobs. They do not acquire the global
    /// build lease, emit busy events, or prevent changing the selected asset.
    pub fn preview_extra(&self, request: ExtraPreviewRequest) -> Result<Vec<u8>> {
        // Extras → Items: the game's item pictures, the same for every hero (items.rs).
        if request.kind == "items" {
            return self.preview_item_icon(&request);
        }
        let settings = self.settings()?;
        let target = deadlock_core::safety::relative_asset(&request.target)?;
        let (archive, expected) = {
            let cache = self.extras_catalog.lock().unwrap_or_else(|e| e.into_inner());
            let cache = cache.as_ref().filter(|c| c.pak == settings.pak && c.hero == request.hero_path)
                .ok_or_else(|| AppError::validation("Load this character’s assets first", "The character or game archive changed."))?;
            let (kind, hash) = cache.assets.get(&casefold(&target))
                .filter(|(kind,_)| kind == &request.kind)
                .ok_or_else(|| AppError::validation("Choose an asset from this character’s catalog", &target))?;
            let _ = kind;
            (cache.archive.clone(), hash.clone())
        };
        let entry = archive.entries.get(&casefold(&target))
            .ok_or_else(|| AppError::validation("This game asset is missing", &target))?;
        if entry.length as usize + entry.preload.len() > deadlock_components::MAX_RESOURCE_BYTES {
            return Err(AppError::validation("This asset is too large to preview", &target));
        }
        let original = archive.read(&target).map_err(|e| AppError::validation("Reload character assets", e))?;
        if deadlock_components::resource_fingerprint(&original) != expected {
            return Err(AppError::validation("This game asset changed. Reload character assets.", &target));
        }
        let cancel = CancellationToken::new();
        if request.kind == "materials" {
            // The game's base-color texture; the page shows the recolor over it.
            let row = deadlock_components::materials::inspect(&target, &original)?;
            let texture = row.color_texture.clone()
                .ok_or_else(|| AppError::validation("This material has no base-color image to preview", &target))?;
            let entry = archive.entries.get(&casefold(&texture))
                .ok_or_else(|| AppError::validation("The material's base-color image is missing from the game", &texture))?;
            if entry.length as usize + entry.preload.len() > deadlock_components::MAX_RESOURCE_BYTES {
                return Err(AppError::validation("This image is too large to preview", &texture));
            }
            let raw = archive.read(&texture).map_err(|e| AppError::validation("Reload character assets", e))?;
            let png = deadlock_components::icons::preview_color_texture(&raw, request.max_dimension.unwrap_or(512).clamp(32, 1024), &cancel)?;
            // Why color ranges cannot rewrite this texture, if they cannot (null when they can).
            let ranges = deadlock_components::texture_recolor::unsupported(&raw);
            return media("image/png", serde_json::json!({"texture":texture,"tint":row.tint,"albedo":row.albedo,"ranges_unsupported":ranges}), png);
        }
        let source = request.source.as_deref().filter(|s| !s.is_empty()).map(Path::new);
        if let Some(path) = source {
            deadlock_vpk::check_unlinked_path(path, false).map_err(|e| AppError::validation("Unsupported replacement file", e))?;
            crate::intake::validate_asset_path(path, match request.kind.as_str() {
                "icons" => crate::intake::AssetKind::Icon,
                "audio" => crate::intake::AssetKind::Audio,
                "effects" => crate::intake::AssetKind::Effect,
                _ => return Err(AppError::validation("Unknown Extras category", &request.kind)),
            })?;
        }
        match request.kind.as_str() {
            "icons" => {
                if casefold(&target).ends_with(".vsvg_c") {
                    if source.is_some() { return Err(AppError::validation("Vector images are preview-only", "Choose a texture icon to use an image replacement.")); }
                    let preview = deadlock_components::vector_icons::preview(&original)?;
                    return media("image/svg+xml",serde_json::json!({"width":preview.width,"height":preview.height}),preview.svg);
                }
                let size = request.max_dimension.unwrap_or(512).clamp(32, 1024);
                let png = if let Some(path) = source {
                    deadlock_components::icons::preview_fitted_image(path,&original,request.fit.as_deref().unwrap_or("contain"),size,&cancel)?
                } else {deadlock_components::icons::preview_texture(&original,size,&cancel)?};
                let metadata = deadlock_components::icons::texture_metadata(&original)?;
                media("image/png",serde_json::json!({"width":metadata.display_width,"height":metadata.display_height}),png)
            }
            "audio" => {
                let (raw, details) = if let Some(path) = source {
                    let raw = preview_file(path)?;
                    let extension = path.extension().and_then(|s| s.to_str()).unwrap_or_default().to_ascii_lowercase();
                    let result = deadlock_components::audio::prepare_audio(&original,&raw,&extension,100,request.audio_fit.as_deref().unwrap_or("exact"),&cancel)?;
                    (result.resource,serde_json::to_value(result.report).map_err(|e| AppError::new("internal","Could not read audio details",e))?)
                } else {(original,serde_json::json!({}))};
                media("audio/wav",details,deadlock_components::audio::preview_audio(&raw,&cancel)?)
            }
            "effects" => {
                let (raw, verified) = if let Some(path) = source {
                    let raw = preview_file(path)?;
                    deadlock_components::resources::validate_effect(&original,&raw)?;
                    (raw,true)
                } else {(original,false)};
                media("application/json",effect_details(&raw,verified)?,vec![])
            }
            _ => Err(AppError::validation("Unknown Extras category", request.kind)),
        }
    }

    /// A game file of the loaded Extras catalog (`kind` as `preview_extra`
    /// takes it), checked against the fingerprint the catalog recorded. No lease.
    pub(crate) fn extras_original(&self, hero_path: &str, target: &str, kind: &str) -> Result<(String, Vec<u8>)> {
        let settings = self.settings()?;
        let target = deadlock_core::safety::relative_asset(target)?;
        let (archive, expected) = {
            let cache = self.extras_catalog.lock().unwrap_or_else(|e| e.into_inner());
            let cache = cache.as_ref().filter(|c| c.pak == settings.pak && c.hero == hero_path)
                .ok_or_else(|| AppError::validation("Load this character’s assets first", "The character or game archive changed."))?;
            let (_, hash) = cache.assets.get(&casefold(&target)).filter(|(k, _)| k == kind)
                .ok_or_else(|| AppError::validation("Choose an asset from this character’s catalog", &target))?;
            (cache.archive.clone(), hash.clone())
        };
        let entry = archive.entries.get(&casefold(&target))
            .ok_or_else(|| AppError::validation("This game asset is missing", &target))?;
        if entry.length as usize + entry.preload.len() > deadlock_components::MAX_RESOURCE_BYTES {
            return Err(AppError::validation("This asset is too large to preview", &target));
        }
        let original = archive.read(&target).map_err(|e| AppError::validation("Reload character assets", e))?;
        if deadlock_components::resource_fingerprint(&original) != expected {
            return Err(AppError::validation("This game asset changed. Reload character assets.", &target));
        }
        Ok((target, original))
    }

    /// The game's own hero as one preview packet for the Colors page (`scene`).
    /// Needs the character's assets loaded (`discover_extras`), whose catalog
    /// rows the meshes name, and the hero's decompiled sources, which
    /// `ensure_base` reuses or makes. That cache is keyed by the game archive's
    /// index digest, so a game update makes new sources (or stops with the
    /// usual message when they cannot be made) instead of showing old ones.
    pub fn hero_scene(&self, project: Project) -> Result<Vec<u8>> {
        let (packet, _) = self.operation("Load the hero's 3D view", |clock, cancel| {
            project
                .validate_action()
                .map_err(|e| AppError::validation("Select a valid character project", e))?;
            let settings = self.settings()?;
            let (rows, archive) = {
                let cache = self.extras_catalog.lock().unwrap_or_else(|e| e.into_inner());
                let cache = cache
                    .as_ref()
                    .filter(|c| c.pak == settings.pak && c.hero == project.original_model)
                    .ok_or_else(|| AppError::validation("Load this character’s assets first", "The character or game archive changed."))?;
                (cache.materials.clone(), cache.archive.clone())
            };
            let (runner, _observer) = self.create_runner(clock, cancel)?;
            let log = |line: &str| {
                let _ = self.append_log("info", line);
            };
            let sources = deadlock_pipeline::sources::SourceService { settings: &settings, runner: &runner, cancel, clock, log: &log };
            let base = sources.ensure_base(&project)?;
            let (packet, report) = self.step(clock, "Read the hero's meshes", || scene::build(&base, project.source_model(), &rows, &archive, cancel))?;
            self.append_log(
                "info",
                &format!(
                    "Hero 3D view: {} meshes from {} files, {} left out, {} hidden at first",
                    report.kept.len(),
                    report.files.len(),
                    report.dropped.len(),
                    report.hidden.len()
                ),
            )?;
            Ok(packet)
        })?;
        Ok(packet)
    }

    /// The prepared replacement model for the Extras → Colors 3D view: the
    /// ordinary preview of `project.dmx`, with the meshes of the user's fitted
    /// gun marked `weapon` from the preparation receipt (`scene::prepared_gun_meshes`).
    /// Without a readable receipt, or without a fitted gun in it, the packet is
    /// exactly the ordinary preview.
    pub fn replacement_color_scene(&self, project: Project) -> Result<Vec<u8>> {
        // A preview load: it waits its turn behind a running operation.
        let (packet, _) = self.waiting_operation("Load model preview", |clock, cancel| {
            let packet = deadlock_preview::load_packet(Path::new(&project.dmx), cancel, clock).map_err(AppError::from)?;
            let guns = scene::prepared_guns_of(Path::new(&project.preparation_report));
            cancel.check()?;
            scene::mark_weapon_meshes(&packet, &guns, cancel)
        })?;
        Ok(packet)
    }

    /// The hero's effect trees for Extras → Effects. Needs the character's
    /// assets loaded (`discover_extras`), whose Effects rows it groups. The
    /// first call per loaded catalog reads the game's definitions and scans
    /// its particle systems (seconds; kept per game archive), then decodes the
    /// hero's trees; later calls return the kept trees at once, without the
    /// operation lease (also after the assets were loaded again for the same
    /// hero and game archive). The first build takes the lease like `hero_scene`: it
    /// runs for seconds, shows in the status bar and can be cancelled. A game
    /// archive that changed after the assets were loaded is refused
    /// (`EFFECTS_GAME_CHANGED`).
    pub fn effect_trees(&self, project: Project) -> Result<EffectTrees> {
        // Kept trees are returned without the lease (reopening the tab during a build works).
        {
            let (archive, _, slot, loaded) = self.effects_slot(&project)?;
            let key = deadlock_components::effect_catalog::archive_key(&archive);
            if key != loaded {
                return Err(AppError::validation(EFFECTS_GAME_CHANGED, "The game archive changed after the assets were loaded."));
            }
            let kept = slot.lock().unwrap_or_else(|e| e.into_inner()).clone();
            if let Some((k, effects)) = kept {
                if k == key {
                    return Ok(effects.trees.clone());
                }
            }
        }
        let (trees, _) = self.operation("Load the hero's effects", |clock, cancel| {
            project
                .validate_action()
                .map_err(|e| AppError::validation("Select a valid character project", e))?;
            let (archive, rows, slot, loaded) = self.effects_slot(&project)?;
            let key = deadlock_components::effect_catalog::archive_key(&archive);
            if key != loaded {
                return Err(AppError::validation(EFFECTS_GAME_CHANGED, "The game archive changed after the assets were loaded."));
            }
            let game = self.step(clock, "Read the game's ability definitions and effects", || {
                Ok(deadlock_components::effect_catalog::game_index(&archive, cancel)?)
            })?;
            let effects = self.step(clock, "Read the hero's effects", || {
                Ok(HeroEffects::build(&archive, &project.original_model, &rows, &game, cancel)?)
            })?;
            cancel.check()?;
            let c = &effects.trees.counts;
            self.append_log("info", &format!("Hero effects: {} effects ({} parts, {} not linked to an ability, {} also used elsewhere)", c.trees, c.parts, c.unlinked, c.with_note))?;
            let trees = effects.trees.clone();
            *slot.lock().unwrap_or_else(|e| e.into_inner()) = Some((key, Arc::new(effects)));
            Ok(trees)
        })?;
        Ok(trees)
    }

    /// One effect before and after an edit, for the editor's strips while a
    /// slider moves. Read-only and fast (tens of microseconds on the game's
    /// largest trees): it takes no operation lease, so it never waits for a
    /// build or blocks one, and it locks the catalog only to copy two
    /// pointers. Needs `effect_trees` first.
    pub fn effect_preview(&self, project: Project, target: String, edit: TreeEdit, pick: Option<PickRequest>, main: Option<[u8; 3]>) -> Result<EffectPreview> {
        let (archive, _, slot, _) = self.effects_slot(&project)?;
        let (key, effects) = slot
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .ok_or_else(|| AppError::validation(EFFECTS_NOT_LOADED, "Effects have not been read for this character yet."))?;
        if deadlock_components::effect_catalog::archive_key(&archive) != key {
            return Err(AppError::validation(EFFECTS_GAME_CHANGED, &target));
        }
        Ok(effects.preview(&target, &edit, pick.as_ref(), main)?)
    }

    /// The archive the loaded effects were read from and its identity (the
    /// effect player plays from it); refused like `effect_preview` when the
    /// game changed or the effects are not loaded.
    #[allow(clippy::type_complexity)]
    pub(crate) fn effects_archive(&self, project: &Project) -> Result<(Arc<deadlock_vpk::Vpk>, String, Arc<Vec<deadlock_components::resources::CatalogRow>>)> {
        let (archive, rows, slot, _) = self.effects_slot(project)?;
        let (key, _) = slot
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .ok_or_else(|| AppError::validation(EFFECTS_NOT_LOADED, "Effects have not been read for this character yet."))?;
        if deadlock_components::effect_catalog::archive_key(&archive) != key {
            return Err(AppError::validation(EFFECTS_GAME_CHANGED, "The game archive changed after the effects were loaded."));
        }
        Ok((archive, key, rows))
    }

    /// The loaded catalog's archive, Effects rows and effects slot for `project`.
    #[allow(clippy::type_complexity)]
    fn effects_slot(&self, project: &Project) -> Result<(Arc<deadlock_vpk::Vpk>, Arc<Vec<deadlock_components::resources::CatalogRow>>, Arc<Mutex<Option<(String, Arc<HeroEffects>)>>>, String)> {
        let settings = self.settings()?;
        let cache = self.extras_catalog.lock().unwrap_or_else(|e| e.into_inner());
        let cache = cache
            .as_ref()
            .filter(|c| c.pak == settings.pak && c.hero == project.original_model)
            .ok_or_else(|| AppError::validation("Load this character’s assets first", "The character or game archive changed."))?;
        Ok((cache.archive.clone(), cache.effect_rows.clone(), cache.effects.clone(), cache.archive_key.clone()))
    }
}

/// Size and modification time of a local image, so the character preview can tell whether
/// a texture it already shows changed on disk without reading it again.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct FileStamp {
    pub len: u64,
    pub modified_ms: u64,
}

/// One stamp per path, or `None` for a path `preview_image` would refuse (a link, a missing
/// file, not a PNG or JPEG) or that cannot be read. Read-only; it does not record timings.
pub fn file_stamps(paths: &[std::path::PathBuf]) -> Vec<Option<FileStamp>> {
    paths
        .iter()
        .map(|path| {
            deadlock_vpk::check_unlinked_path(path, false).ok()?;
            crate::intake::validate_asset_path(path, crate::intake::AssetKind::Icon).ok()?;
            let meta = std::fs::metadata(path).ok()?;
            let modified = meta.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?;
            Some(FileStamp { len: meta.len(), modified_ms: modified.as_millis() as u64 })
        })
        .collect()
}

pub(crate) fn preview_file(path: &Path) -> Result<Vec<u8>> {
    if std::fs::metadata(path).map_err(|e| AppError::io("Could not read replacement file",e))?.len() > deadlock_components::MAX_RESOURCE_BYTES as u64 {
        return Err(AppError::validation("Replacement files must be smaller than 128 MiB",path.display()));
    }
    use std::io::Read;
    let file=std::fs::File::open(path).map_err(|e| AppError::io("Could not read replacement file",e))?;
    let mut raw=Vec::new();
    file.take(deadlock_components::MAX_RESOURCE_BYTES as u64+1).read_to_end(&mut raw).map_err(|e| AppError::io("Could not read replacement file",e))?;
    if raw.len()>deadlock_components::MAX_RESOURCE_BYTES{return Err(AppError::validation("Replacement files must be smaller than 128 MiB",path.display()));}
    Ok(raw)
}

fn effect_details(raw: &[u8], replacement_verified: bool) -> Result<serde_json::Value> {
    let layout = deadlock_vpk::resource_blocks(raw).map_err(|e| AppError::validation("Could not inspect particle structure", e))?;
    let block = layout.blocks.iter().find(|b| b.name == "DATA")
        .ok_or_else(|| AppError::validation("Particle structure is unavailable", "DATA block is missing"))?;
    let doc = deadlock_resource::binary::decode(&raw[block.offset..block.offset+block.size])
        .map_err(|e| AppError::validation("Particle structure is unavailable", e))?;
    let mut groups = BTreeMap::new();
    for (key,label) in [("m_Renderers","Renderers"),("m_Emitters","Emitters"),("m_Operators","Operators"),("m_Children","Child effects")] {
        let rows = doc.root.get(key).and_then(|v| v.array().ok()).unwrap_or(&[]);
        let classes:Vec<_> = rows.iter().filter_map(|v| v.get("_class").and_then(|v| v.string().ok())).take(24).map(|s|s.chars().take(160).collect::<String>()).collect();
        groups.insert(label,serde_json::json!({"count":rows.len(),"classes":classes}));
    }
    let references = deadlock_vpk::resource_references(raw).map_err(|e| AppError::validation("Particle references are unavailable", e))?;
    let reference_count=references.len();
    let references:Vec<_>=references.into_iter().take(64).map(|s|s.chars().take(240).collect::<String>()).collect();
    Ok(serde_json::json!({"groups":groups,"references":references,"reference_count":reference_count,"replacement_verified":replacement_verified}))
}

/// The Extras → Colors 3D view of the game's own hero: its LOD 0 render meshes
/// from the decompiled sources, in one preview packet (`src/preview/packet.ts`)
/// without skin, bones or vertex colours, each mesh naming the Colors catalog
/// row it shows.
pub mod scene {
    use crate::app::{AppError, Result};
    use deadlock_components::materials::{self, MaterialRow};
    use deadlock_core::{text::casefold, CancellationToken};
    use deadlock_pipeline::hero_meshes::Step;
    use deadlock_preview::{BoneInfo, Bounds, BufferSlice, MeshInfo, Preview};
    use deadlock_resource::text::{Document, Node};
    use serde::Serialize;
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::Path;

    /// Shader switches of an effect layer drawn over the body (Vindicta's
    /// `ghost_glow`, Lady Geist's `ghost_armglow`); see `effect_layer`.
    /// `F_TRANSLUCENT` alone is not one: it also marks surfaces such as a lace
    /// skirt and glasses (every hero material in the game checked).
    pub const EFFECT_FEATURES: [&str; 1] = ["F_ADDITIVE_BLEND"];
    /// A gun counts as held when its bounds come this close to the hand its
    /// weapon bone hangs from (inches), at least.
    const HAND_REACH: f64 = 3.;
    /// Share of a mesh's skin weight on weapon bones that makes it a gun
    /// (`deadlock-preparation weapon.rs`).
    const GUN_SHARE: f64 = 0.85;
    /// Why a gun that is not held is hidden at first. The Colors page draws
    /// weapon parts hidden for this reason (and no other) while its Weapon
    /// filter is on: `GUN_AWAY_REASON` in `src/extras/colorScenes.ts`.
    pub const GUN_AWAY: &str = "gun away from the hand (placed by animation)";

    #[derive(Debug, Clone, Serialize)]
    pub struct SceneMesh {
        #[serde(flatten)]
        pub mesh: MeshInfo,
        /// The Colors catalog row's compiled material (`.vmat_c`), when one matches.
        pub target: Option<String>,
        /// The model's render mesh (`RenderMeshFile` name).
        pub node: String,
        /// The body group whose default choice shows this mesh.
        pub body_group: Option<String>,
        pub default_hidden: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub hidden_reason: Option<String>,
        /// A part of the hero's weapon: at least `GUN_SHARE` of its skin
        /// weight on weapon bones, held or not (the Colors page's Weapon
        /// filter). Written only when true.
        #[serde(skip_serializing_if = "std::ops::Not::not")]
        pub weapon: bool,
    }
    #[derive(Debug, Clone, Serialize, PartialEq)]
    pub struct Remap {
        pub from: String,
        pub to: String,
        /// The catalog rows of `from` and `to`, when the hero has them.
        pub from_target: Option<String>,
        pub to_target: Option<String>,
    }
    #[derive(Debug, Clone, Serialize, PartialEq)]
    pub struct MaterialGroup {
        pub name: String,
        pub remaps: Vec<Remap>,
    }
    /// The packet's metadata: the preview format plus the scene's own fields.
    #[derive(Debug, Clone, Serialize)]
    pub struct SceneMetadata {
        pub schema_version: u32,
        pub source: String,
        pub source_sha256: String,
        pub up_axis: String,
        pub warnings: Vec<String>,
        pub bounds: Bounds,
        pub meshes: Vec<SceneMesh>,
        pub bones: Vec<BoneInfo>,
        pub material_groups: Vec<MaterialGroup>,
        /// Catalog rows (`.vmat_c`) no mesh of the scene shows (a material
        /// group may point them at one).
        pub rows_without_mesh: Vec<String>,
        /// Mesh materials (`.vmat`) without a catalog row.
        pub materials_without_row: Vec<String>,
    }
    /// What the scene left out, for development checks and the logs.
    #[derive(Debug, Clone, Default, Serialize)]
    pub struct Report {
        pub files: Vec<String>,
        pub kept: Vec<String>,
        pub dropped: Vec<(String, String)>,
        pub hidden: Vec<(String, String)>,
        pub guns: Vec<String>,
        pub rows_without_mesh: Vec<String>,
        pub materials_without_row: Vec<String>,
        pub load_ms: f64,
    }
    impl Report {
        pub fn describe(&self) -> String {
            let mut out = String::new();
            out += &format!("  files {} ({:.0} ms): {}\n", self.files.len(), self.load_ms, self.files.join(", "));
            out += &format!("  kept {}: {}\n", self.kept.len(), self.kept.join(", "));
            for (what, why) in &self.dropped {
                out += &format!("  dropped {what}: {why}\n");
            }
            for (what, why) in &self.hidden {
                out += &format!("  hidden {what}: {why}\n");
            }
            for line in &self.guns {
                out += &format!("  gun {line}\n");
            }
            if !self.rows_without_mesh.is_empty() {
                out += &format!("  rows without a mesh: {}\n", self.rows_without_mesh.join(", "));
            }
            if !self.materials_without_row.is_empty() {
                out += &format!("  mesh materials without a row: {}\n", self.materials_without_row.join(", "));
            }
            out
        }
    }

    fn scalar(node: &Node, key: &str) -> Option<String> {
        node.get(key).and_then(|v| v.scalar()).map(str::to_owned)
    }
    fn invalid(message: &str, details: impl ToString) -> AppError {
        AppError::validation(message, details)
    }

    /// The preview packet layout of `deadlock_preview::Preview::packet`
    /// (`DMBP`, version 1, metadata length, 0, metadata padded to 4 bytes,
    /// buffer), for metadata that adds the scene's own fields. A test checks
    /// that both write the same bytes.
    pub fn encode(metadata: &impl Serialize, data: &[u8], cancel: &CancellationToken) -> Result<Vec<u8>> {
        cancel.check()?;
        let json = serde_json::to_vec(metadata).map_err(|e| AppError::new("internal", "Could not encode the hero's 3D view", e))?;
        if json.len() > 16 * 1024 * 1024 || 16 + json.len() + 3 + data.len() > 512 * 1024 * 1024 {
            return Err(invalid("The hero's 3D view is too large", format!("{} bytes of metadata, {} of geometry", json.len(), data.len())));
        }
        let padded = (json.len() + 3) & !3;
        let mut output = Vec::with_capacity(16 + padded + data.len());
        output.extend_from_slice(b"DMBP");
        output.extend_from_slice(&1u32.to_le_bytes());
        output.extend_from_slice(&(json.len() as u32).to_le_bytes());
        output.extend_from_slice(&0u32.to_le_bytes());
        output.extend(json);
        output.resize(16 + padded, 0);
        output.extend_from_slice(data);
        cancel.check()?;
        Ok(output)
    }
    /// The prepared meshes that are the user's fitted gun: the meshes of a
    /// preparation receipt (read from `receipt`) that carry a `weapon` record
    /// (written by the gun fit for every gun mesh it exports), by name. Only
    /// the names and the records are kept: older receipts hold every mesh's
    /// geometry (hundreds of MB), which is skipped while reading. Empty when
    /// the receipt cannot be read.
    pub fn prepared_gun_meshes(receipt: impl std::io::Read) -> BTreeSet<String> {
        #[derive(serde::Deserialize)]
        struct Receipt {
            #[serde(default)]
            meshes: Vec<ReceiptMesh>,
        }
        #[derive(serde::Deserialize)]
        struct ReceiptMesh {
            #[serde(default)]
            name: String,
            #[serde(default)]
            weapon: Option<serde_json::Value>,
        }
        serde_json::from_reader::<_, Receipt>(receipt)
            .map(|r| r.meshes.into_iter().filter(|m| m.weapon.as_ref().is_some_and(|w| w.is_object()) && !m.name.is_empty()).map(|m| m.name).collect())
            .unwrap_or_default()
    }
    /// `prepared_gun_meshes` of the receipt file `path`, kept for the last
    /// receipt read (by path, size and modification time): an older receipt
    /// with full geometry takes seconds to read, and the page asks on every
    /// opening.
    pub fn prepared_guns_of(path: &Path) -> BTreeSet<String> {
        static LAST: std::sync::Mutex<Option<(std::path::PathBuf, u64, Option<std::time::SystemTime>, BTreeSet<String>)>> = std::sync::Mutex::new(None);
        let Ok(meta) = std::fs::metadata(path) else { return BTreeSet::new() };
        let stamp = (meta.len(), meta.modified().ok());
        let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((p, len, modified, guns)) = last.as_ref() {
            if p == path && (*len, *modified) == stamp {
                return guns.clone();
            }
        }
        let guns = std::fs::File::open(path).map(|file| prepared_gun_meshes(std::io::BufReader::new(file))).unwrap_or_default();
        *last = Some((path.to_path_buf(), stamp.0, stamp.1, guns.clone()));
        guns
    }
    /// A preview packet (`encode`'s layout) with `weapon: true` on its meshes
    /// whose source mesh is named in `guns`. The same bytes when none is.
    pub fn mark_weapon_meshes(packet: &[u8], guns: &BTreeSet<String>, cancel: &CancellationToken) -> Result<Vec<u8>> {
        let unreadable = || invalid("The model preview could not be read", "The preview packet is not in the expected layout.");
        let word = |at: usize| packet.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
        if guns.is_empty() {
            return Ok(packet.to_vec());
        }
        if packet.get(..4) != Some(b"DMBP".as_slice()) || word(4) != Some(1) {
            return Err(unreadable());
        }
        let length = word(8).ok_or_else(unreadable)? as usize;
        let padded = (length + 3) & !3;
        let (Some(json), Some(data)) = (packet.get(16..16 + length), packet.get(16 + padded..)) else { return Err(unreadable()) };
        let mut metadata: serde_json::Value = serde_json::from_slice(json).map_err(|_| unreadable())?;
        let mut marked = 0;
        for mesh in metadata.get_mut("meshes").and_then(|m| m.as_array_mut()).into_iter().flatten() {
            let gun = mesh.get("source_name").and_then(|n| n.as_str()).is_some_and(|n| guns.contains(n));
            if let (true, Some(fields)) = (gun, mesh.as_object_mut()) {
                fields.insert("weapon".into(), true.into());
                marked += 1;
            }
        }
        if marked == 0 {
            return Ok(packet.to_vec());
        }
        encode(&metadata, data, cancel)
    }
    // Which meshes are the hero (LOD 0, body groups, import filters): the pipeline's
    // one choice, shared with the build of color ranges that change one part.
    pub use deadlock_pipeline::hero_meshes::{lod0, lower_lod};
    pub use deadlock_pipeline::carried::{body_groups, filter_keeps};
    pub fn material_groups(document: &Document, rows: &[MaterialRow]) -> Vec<MaterialGroup> {
        let target = |path: &str| materials::row_for_mesh_material(rows, path).map(|r| r.target.clone());
        document
            .objects(None)
            .into_iter()
            .filter(|n| matches!(n.get("_class").and_then(|v| v.scalar()), Some("MaterialGroup" | "DefaultMaterialGroup")))
            .filter_map(|group| {
                let remaps: Vec<Remap> = group
                    .get("remaps")
                    .and_then(|r| r.array())?
                    .iter()
                    .filter_map(|r| {
                        let (from, to) = (scalar(r, "from")?, scalar(r, "to")?);
                        Some(Remap { from_target: target(&from), to_target: target(&to), from, to })
                    })
                    .collect();
                (!remaps.is_empty()).then(|| MaterialGroup { name: scalar(group, "name").unwrap_or_default(), remaps })
            })
            .collect()
    }
    /// An effect layer drawn over the body rather than a surface of its own:
    /// its compiled material blends additively (`EFFECT_FEATURES`; in the game
    /// these are glows, magic effects, and some lenses and glass, which the
    /// view cannot draw as the game does either). Returns the switch that says so.
    pub fn effect_layer(features: &BTreeMap<String, i64>) -> Option<&'static str> {
        EFFECT_FEATURES.into_iter().find(|f| features.get(*f).is_some_and(|v| *v != 0))
    }

    fn floats(data: &[u8], slice: &BufferSlice) -> Vec<f32> {
        data.get(slice.offset..slice.offset + slice.count * 4)
            .unwrap_or_default()
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect()
    }
    fn bounds_of(positions: &[f32]) -> Option<([f64; 3], [f64; 3])> {
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for p in positions.chunks_exact(3) {
            for k in 0..3 {
                lo[k] = lo[k].min(f64::from(p[k]));
                hi[k] = hi[k].max(f64::from(p[k]));
            }
        }
        lo[0].is_finite().then_some((lo, hi))
    }
    fn box_distance(point: [f64; 3], (lo, hi): ([f64; 3], [f64; 3])) -> f64 {
        (0..3).map(|k| (lo[k] - point[k]).max(point[k] - hi[k]).max(0.).powi(2)).sum::<f64>().sqrt()
    }

    /// Whether a mesh is a gun (most of its skin weight on weapon bones) and,
    /// if so, whether it is held: its bounds reach the bone its weapon bones
    /// hang from (a hand), which must not be the skeleton's root. Guns modelled
    /// at the origin, posed into the hand only by animation, are not held.
    fn gun(preview: &Preview, info: &MeshInfo, weapon: &BTreeSet<String>, bounds: ([f64; 3], [f64; 3])) -> Option<(bool, String)> {
        let skin = info.skin.as_ref()?;
        let data = floats(&preview.data, &skin.data);
        let mut totals: BTreeMap<&str, f64> = BTreeMap::new();
        for v in 0..info.vertex_count {
            for slot in 0..skin.influences {
                let at = (v * skin.influences + slot) * 2;
                let (Some(joint), Some(weight)) = (data.get(at), data.get(at + 1)) else { continue };
                if let Some(name) = skin.joint_names.get(joint.round() as usize) {
                    *totals.entry(name).or_default() += f64::from(*weight);
                }
            }
        }
        let total: f64 = totals.values().sum();
        let on_weapon: f64 = totals.iter().filter(|(n, _)| weapon.contains(**n)).map(|(_, w)| w).sum();
        if total <= 0. || on_weapon / total < GUN_SHARE {
            return None;
        }
        let carrier = totals.iter().filter(|(n, _)| weapon.contains(**n)).max_by(|a, b| a.1.total_cmp(b.1)).map(|(n, _)| *n)?;
        let bones = &preview.metadata.bones;
        let index = |name: &str| bones.iter().position(|b| b.name == name);
        // The nearest ancestor outside the weapon bones.
        let mut at = index(carrier);
        while let Some(i) = at {
            if !weapon.contains(&bones[i].name) {
                break;
            }
            at = usize::try_from(bones[i].parent).ok();
        }
        let Some(hand) = at.filter(|i| bones[*i].parent >= 0) else {
            return Some((false, format!("{carrier} hangs from the skeleton's root")));
        };
        let (lo, hi) = bounds;
        let size = (0..3).map(|k| (hi[k] - lo[k]).powi(2)).sum::<f64>().sqrt();
        let distance = box_distance(bones[hand].position, bounds);
        let held = distance <= HAND_REACH.max(size * 0.1);
        Some((held, format!("{carrier} under {} at {distance:.1} in (gun {size:.1} in)", bones[hand].name)))
    }

    /// The scene of the hero model `model` (its `.vmdl` path) in the decompiled
    /// sources `base`, with `rows` from the Colors catalog and the game archive
    /// for the materials' shader switches.
    pub fn build(base: &Path, model: &str, rows: &[MaterialRow], archive: &deadlock_vpk::Vpk, cancel: &CancellationToken) -> Result<(Vec<u8>, Report)> {
        let started = std::time::Instant::now();
        let model_path = deadlock_core::safety::inside(base, model)?;
        let text = std::fs::read_to_string(&model_path).map_err(|e| AppError::io("The hero's decompiled model could not be read", e))?;
        let document = Document::parse(text.as_str()).map_err(|e| invalid("The hero's decompiled model could not be read", e))?;
        let chosen = deadlock_pipeline::hero_meshes::choose(base, &document, cancel)?;
        let mut report = Report::default();
        let mut kept: Vec<SceneMesh> = vec![];
        let mut data: Vec<u8> = vec![];
        let mut warnings = vec![];
        let mut features: BTreeMap<String, Option<BTreeMap<String, i64>>> = BTreeMap::new();
        for step in &chosen.steps {
            cancel.check()?;
            let (node, file, body_group, chosen_meshes, left_out) = match step {
                Step::OtherChoice { node, group, choice } => {
                    report.dropped.push((node.clone(), format!("only body group {group} choice {choice} shows it")));
                    continue;
                }
                Step::Unreadable { node, file } => {
                    report.dropped.push((node.clone(), format!("{file} could not be read")));
                    warnings.push(format!("The mesh {node} could not be read from the decompiled sources."));
                    continue;
                }
                Step::Node { node, file, body_group, meshes, left_out } => (node, file, body_group, meshes, left_out),
            };
            let Some(Some(preview)) = chosen.previews.get(file) else { continue };
            if !report.files.contains(file) {
                report.files.push(file.clone());
            }
            let bones: Vec<deadlock_preparation::algorithms::Bone> = preview
                .metadata
                .bones
                .iter()
                .map(|b| deadlock_preparation::algorithms::Bone {
                    name: b.name.clone(),
                    parent: usize::try_from(b.parent).ok().and_then(|p| preview.metadata.bones.get(p)).map(|p| p.name.clone()),
                    position: b.position,
                    details: Default::default(),
                })
                .collect();
            let weapon = deadlock_preparation::weapon::weapon_bones(&bones);
            for source_name in left_out {
                report.dropped.push((format!("{node}/{source_name}"), "left out by the render mesh's import filter".into()));
            }
            for info in chosen_meshes.iter().map(|index| &preview.metadata.meshes[*index]) {
                let positions = floats(&preview.data, &info.positions);
                let Some(bounds) = bounds_of(&positions) else { continue };
                let row = materials::row_for_mesh_material(rows, &info.material);
                let mut hidden_reason = None;
                if let Some(row) = row {
                    let flags = features.entry(row.target.clone()).or_insert_with(|| {
                        let raw = archive.read(&row.target).ok()?;
                        materials::shader_features(&raw, &row.target).ok()
                    });
                    if let Some(feature) = flags.as_ref().and_then(effect_layer) {
                        hidden_reason = Some(format!("glow or effect layer drawn over the model ({feature})"));
                    }
                }
                let found = gun(preview, info, &weapon, bounds);
                let is_weapon = found.is_some();
                if let Some((held, why)) = found {
                    report.guns.push(format!("{node}/{}: {} ({why})", info.name, if held { "held" } else { "not held" }));
                    if !held && hidden_reason.is_none() {
                        hidden_reason = Some(GUN_AWAY.into());
                    }
                }
                let mut copy = |slice: &BufferSlice| {
                    let offset = data.len();
                    data.extend_from_slice(&preview.data[slice.offset..slice.offset + slice.count * 4]);
                    BufferSlice { offset, count: slice.count }
                };
                let mesh = MeshInfo {
                    name: format!("{node}/{}", info.name),
                    source_name: info.source_name.clone(),
                    material: info.material.clone(),
                    vertex_count: info.vertex_count,
                    index_count: info.index_count,
                    positions: copy(&info.positions),
                    normals: copy(&info.normals),
                    uvs: info.uvs.as_ref().map(&mut copy),
                    colors: None,
                    indices: copy(&info.indices),
                    skin: None,
                };
                if let Some(reason) = &hidden_reason {
                    report.hidden.push((mesh.name.clone(), reason.clone()));
                }
                report.kept.push(mesh.name.clone());
                kept.push(SceneMesh {
                    target: row.map(|r| r.target.clone()),
                    node: node.clone(),
                    body_group: body_group.clone(),
                    default_hidden: hidden_reason.is_some(),
                    hidden_reason,
                    weapon: is_weapon,
                    mesh,
                });
            }
        }
        if kept.is_empty() {
            return Err(invalid("The hero's model has no mesh to show", format!("No LOD 0 render mesh of {model} could be read.")));
        }
        // Bounds of what the page shows at first (every mesh when all are hidden).
        let shown: Vec<&SceneMesh> = if kept.iter().any(|m| !m.default_hidden) { kept.iter().filter(|m| !m.default_hidden).collect() } else { kept.iter().collect() };
        let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
        for mesh in shown {
            if let Some((a, b)) = bounds_of(&floats(&data, &mesh.mesh.positions)) {
                for k in 0..3 {
                    lo[k] = lo[k].min(a[k]);
                    hi[k] = hi[k].max(b[k]);
                }
            }
        }
        let used: BTreeSet<String> = kept.iter().map(|m| casefold(&m.mesh.material)).collect();
        report.rows_without_mesh = rows.iter().filter(|r| !used.contains(&casefold(&r.path))).map(|r| r.target.clone()).collect();
        let mut missing: Vec<String> = kept.iter().filter(|m| m.target.is_none()).map(|m| m.mesh.material.clone()).collect();
        missing.dedup();
        report.materials_without_row = missing.into_iter().collect::<BTreeSet<_>>().into_iter().collect();
        if !report.materials_without_row.is_empty() {
            // The sources and the catalog come from the same game files, so this
            // means the catalog was loaded before a game update.
            warnings.push(format!(
                "{} of the hero's materials are not in the loaded material list. Load character assets again.",
                report.materials_without_row.len()
            ));
        }
        let metadata = SceneMetadata {
            schema_version: 1,
            source: model.into(),
            source_sha256: deadlock_core::safety::digest_reader(&mut text.as_bytes(), cancel)?,
            up_axis: "Z".into(),
            warnings,
            bounds: Bounds { min: lo, max: hi },
            meshes: kept,
            bones: vec![],
            material_groups: material_groups(&document, rows),
            rows_without_mesh: report.rows_without_mesh.clone(),
            materials_without_row: report.materials_without_row.clone(),
        };
        let packet = encode(&metadata, &data, cancel)?;
        report.load_ms = started.elapsed().as_secs_f64() * 1000.;
        Ok((packet, report))
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        const MODEL: &str = r#"<!-- kv3 encoding:text:version{e21c7f3c-8a33-41c5-9977-a76d3a32aa0d} format:modeldoc29:version{3cec427c-1b0e-4d48-a90a-0436f33a6041} -->
{
	rootNode =
	{
		_class = "RootNode"
		children =
		[
			{
				_class = "RenderMeshList"
				children =
				[
					{ _class = "RenderMeshFile" name = "body" filename = "m/body.dmx" },
					{ _class = "RenderMeshFile" name = "body_lod1" filename = "m/body_lod1.dmx" },
					{ _class = "RenderMeshFile" name = "head" filename = "m/head.dmx" },
					{ _class = "RenderMeshFile" name = "head_alt" filename = "m/head_alt.dmx" },
					{ _class = "RenderMeshFile" name = "armor" filename = "m/shared.dmx" import_filter = { exclude_by_default = true exception_list = [ "armor" ] } },
				]
			},
			{
				_class = "LODGroupList"
				children =
				[
					{ _class = "LODGroup" switch_threshold = 20.0 mesh_references = [ { mesh_name = "body_lod1" } ] },
					{ _class = "LODGroup" switch_threshold = 0.0 mesh_references = [ { mesh_name = "body" }, { mesh_name = "head" }, { mesh_name = "head_alt" }, { mesh_name = "armor" } ] },
				]
			},
			{
				_class = "BodyGroupList"
				children =
				[
					{
						_class = "BodyGroup"
						name = "head"
						children =
						[
							{ _class = "BodyGroupChoice" name = "base" meshes = [ "head" ] },
							{ _class = "BodyGroupChoice" name = "alt" meshes = [ "head", "head_alt" ] },
						]
					},
				]
			},
			{
				_class = "MaterialGroupList"
				children =
				[
					{ _class = "DefaultMaterialGroup" name = "default" remaps = [  ] },
					{ _class = "MaterialGroup" name = "smoke" remaps = [ { _class = "BaseMaterialRemap" from = "m/materials/Head.vmat" to = "m/materials/head_invis.vmat" } ] },
				]
			},
		]
	}
}
"#;
        fn row(path: &str) -> MaterialRow {
            MaterialRow {
                path: path.into(),
                target: format!("{path}_c"),
                label: String::new(),
                shader: "pbr.vfx".into(),
                supported: true,
                reason: String::new(),
                tint: None,
                albedo: None,
                tint_masked: false,
                color_texture: None,
                used_by: vec![],
                shared: false,
                original_sha256: String::new(),
            }
        }
        #[test]
        fn lod0_is_the_lowest_threshold_and_body_groups_keep_their_first_choice() {
            let document = Document::parse(MODEL).unwrap();
            let names = ["body", "body_lod1", "head", "head_alt", "armor"].map(String::from);
            assert_eq!(lod0(&document, &names), ["body", "head", "head_alt", "armor"], "the lowest threshold, not the first group");
            let groups = body_groups(&document);
            assert_eq!(groups.len(), 1);
            assert_eq!(groups[0].0, "head");
            assert!(groups[0].1.contains("head"));
            assert_eq!(groups[0].2.get("head_alt").map(String::as_str), Some("alt"), "only another choice shows it");
            assert!(!groups[0].2.contains_key("head"), "also in the first choice");
            // Without LOD groups: every render mesh that is not a lower level.
            let flat = Document::parse(MODEL.replace("LODGroup\"", "NotALodGroup\"")).unwrap();
            assert_eq!(lod0(&flat, &names), ["body", "head", "head_alt", "armor"]);
            for (name, lower) in [("body_lod1", true), ("bodyLOD2", true), ("body lod 3", true), ("body_lod0", false), ("body", false), ("lodge", false), ("cloud9", false)] {
                assert_eq!(lower_lod(name), lower, "{name}");
            }
        }
        #[test]
        fn import_filters_material_groups_and_effect_layers() {
            let document = Document::parse(MODEL).unwrap();
            let armor = document.objects(Some("RenderMeshFile")).into_iter().find(|n| scalar(n, "name").as_deref() == Some("armor")).unwrap();
            let filter = armor.get("import_filter");
            assert!(filter_keeps(filter, "armor"));
            assert!(!filter_keeps(filter, "cape"), "excluded by default");
            assert!(filter_keeps(None, "cape"));
            // Rows match mesh materials casefolded, never edited.
            let rows = [row("m/materials/head.vmat"), row("m/materials/head_invis.vmat")];
            assert_eq!(materials::row_for_mesh_material(&rows, "M/Materials/HEAD.vmat").map(|r| r.target.as_str()), Some("m/materials/head.vmat_c"));
            assert!(materials::row_for_mesh_material(&rows, "m/materials/head").is_none());
            assert_eq!(
                material_groups(&document, &rows),
                [MaterialGroup {
                    name: "smoke".into(),
                    remaps: vec![Remap {
                        from: "m/materials/Head.vmat".into(),
                        to: "m/materials/head_invis.vmat".into(),
                        from_target: Some("m/materials/head.vmat_c".into()),
                        to_target: Some("m/materials/head_invis.vmat_c".into()),
                    }],
                }],
                "groups without remaps are left out"
            );
            let features = |pairs: &[(&str, i64)]| pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect::<BTreeMap<_, _>>();
            assert_eq!(effect_layer(&features(&[("F_ADDITIVE_BLEND", 1), ("F_TRANSLUCENT", 1)])), Some("F_ADDITIVE_BLEND"));
            assert_eq!(effect_layer(&features(&[("F_TRANSLUCENT", 1)])), None, "a translucent lace skirt is a surface");
            assert_eq!(effect_layer(&features(&[("F_ADDITIVE_BLEND", 0), ("F_SELF_ILLUM", 1)])), None);
        }
        #[test]
        fn the_packet_is_the_preview_format() {
            let metadata = deadlock_preview::Metadata {
                schema_version: 1,
                source: "m/hero.vmdl".into(),
                source_sha256: "0".repeat(64),
                up_axis: "Z".into(),
                warnings: vec!["w".into()],
                bounds: Bounds { min: [0.; 3], max: [1.; 3] },
                meshes: vec![],
                bones: vec![],
            };
            let data = vec![1u8, 2, 3, 4, 5, 6, 7, 8];
            let preview = Preview { metadata: metadata.clone(), data: data.clone() };
            let cancel = CancellationToken::new();
            assert_eq!(encode(&metadata, &data, &cancel).unwrap(), preview.packet(&cancel).unwrap());
        }

        /// A preview with bones root → hand_R → weapon_R and one triangle per
        /// mesh, each skinned wholly to the named joint.
        fn skinned(meshes: &[(&str, &str)]) -> Preview {
            let mut data: Vec<u8> = vec![];
            let mut push = |values: &[f32]| {
                let offset = data.len();
                values.iter().for_each(|v| data.extend_from_slice(&v.to_le_bytes()));
                BufferSlice { offset, count: values.len() }
            };
            let joints = ["root", "hand_R", "weapon_R"];
            let infos = meshes
                .iter()
                .map(|(name, joint)| {
                    let index = joints.iter().position(|j| j == joint).unwrap() as f32;
                    MeshInfo {
                        name: name.to_string(),
                        source_name: name.to_string(),
                        material: format!("m/materials/{name}.vmat"),
                        vertex_count: 3,
                        index_count: 3,
                        positions: push(&[0., 0., 50., 1., 0., 50., 0., 1., 50.]),
                        normals: push(&[0., 0., 1., 0., 0., 1., 0., 0., 1.]),
                        uvs: None,
                        colors: None,
                        indices: push(&[0., 1., 2.]),
                        skin: Some(deadlock_preview::SkinInfo {
                            influences: 1,
                            joint_names: joints.map(String::from).to_vec(),
                            width: 0,
                            height: 0,
                            data: push(&[index, 1., index, 1., index, 1.]),
                        }),
                    }
                })
                .collect();
            let bone = |name: &str, parent: i32, position: [f64; 3]| BoneInfo { name: name.into(), parent, position };
            Preview {
                metadata: deadlock_preview::Metadata {
                    schema_version: 1,
                    source: "m/hero.vmdl".into(),
                    source_sha256: "0".repeat(64),
                    up_axis: "Z".into(),
                    warnings: vec![],
                    bounds: Bounds { min: [0., 0., 50.], max: [1., 1., 50.] },
                    meshes: infos,
                    bones: vec![bone("root", -1, [0.; 3]), bone("hand_R", 0, [0., 0., 50.]), bone("weapon_R", 1, [0., 0., 50.])],
                },
                data,
            }
        }
        #[test]
        fn weapon_parts_are_marked_by_their_weight_on_weapon_bones() {
            let preview = skinned(&[("gun", "weapon_R"), ("sleeve", "hand_R")]);
            let weapon: BTreeSet<String> = ["weapon_R".to_string()].into();
            let found: Vec<bool> = preview
                .metadata
                .meshes
                .iter()
                .map(|info| gun(&preview, info, &weapon, bounds_of(&floats(&preview.data, &info.positions)).unwrap()).is_some())
                .collect();
            assert_eq!(found, [true, false], "only the mesh on the weapon bone is a weapon part, whatever its name");
            // The flag is written only when true, so scenes without a weapon keep their bytes.
            let mesh = |weapon: bool| SceneMesh {
                mesh: preview.metadata.meshes[0].clone(),
                target: None,
                node: "gun".into(),
                body_group: None,
                default_hidden: false,
                hidden_reason: None,
                weapon,
            };
            let text = |weapon: bool| serde_json::to_value(mesh(weapon)).unwrap();
            assert_eq!(text(true)["weapon"], serde_json::json!(true));
            assert!(text(false).get("weapon").is_none());
            // The page reveals weapon parts hidden for exactly this reason.
            let page = include_str!("../../src/extras/colorScenes.ts");
            assert!(page.contains(&format!("GUN_AWAY_REASON = {GUN_AWAY:?};")), "colorScenes.ts must hold the same reason");
        }
        #[test]
        fn a_prepared_models_fitted_gun_is_marked_from_its_receipt() {
            let receipt = serde_json::json!({"meshes": [
                {"name": "Body", "materials": ["m/body.vmat"]},
                {"name": "weapon_Gun", "materials": ["m/gun.vmat"], "weapon": {"policy": "weapon-fit-v1", "source_object": ["Gun"]}},
                {"name": "weapon_note", "weapon": "not a gun record"},
            ]});
            let guns = prepared_gun_meshes(receipt.to_string().as_bytes());
            assert_eq!(guns, ["weapon_Gun".to_string()].into());
            assert!(prepared_gun_meshes(br#"{"status": "ok"}"#.as_slice()).is_empty());
            assert!(prepared_gun_meshes(b"not a receipt".as_slice()).is_empty());
            // From a file, read again when the file changes; nothing for a missing file.
            let file = std::env::temp_dir().join(format!("dmb-receipt-{}.json", std::process::id()));
            std::fs::write(&file, receipt.to_string()).unwrap();
            assert_eq!(prepared_guns_of(&file), guns);
            std::fs::write(&file, r#"{"meshes": [{"name": "weapon_Other", "weapon": {}}, {"name": "Body"}]}"#).unwrap();
            assert_eq!(prepared_guns_of(&file), ["weapon_Other".to_string()].into());
            std::fs::remove_file(&file).unwrap();
            assert!(prepared_guns_of(&file).is_empty());

            let cancel = CancellationToken::new();
            let preview = skinned(&[("Body", "hand_R"), ("weapon_Gun", "weapon_R")]);
            let packet = preview.packet(&cancel).unwrap();
            let marked = mark_weapon_meshes(&packet, &guns, &cancel).unwrap();
            let length = u32::from_le_bytes(marked[8..12].try_into().unwrap()) as usize;
            let metadata: serde_json::Value = serde_json::from_slice(&marked[16..16 + length]).unwrap();
            let flags: Vec<_> = metadata["meshes"].as_array().unwrap().iter().map(|m| m.get("weapon").cloned()).collect();
            assert_eq!(flags, [None, Some(serde_json::json!(true))]);
            assert!(marked.ends_with(&preview.data), "the geometry is unchanged");
            // Nothing to mark: exactly the ordinary preview.
            assert_eq!(mark_weapon_meshes(&packet, &["Other".to_string()].into(), &cancel).unwrap(), packet);
            assert_eq!(mark_weapon_meshes(&packet, &BTreeSet::new(), &cancel).unwrap(), packet);
            assert!(mark_weapon_meshes(b"NOPE", &guns, &cancel).is_err());
        }
    }
}

#[cfg(feature = "desktop")]
pub mod commands {
    use super::*;
    use std::{path::PathBuf, sync::Arc};
    type State<'a> = tauri::State<'a, Arc<Application>>;
    async fn background<T: Send + 'static>(
        work: impl FnOnce() -> Result<T> + Send + 'static,
    ) -> Result<T> {
        tauri::async_runtime::spawn_blocking(work)
            .await
            .map_err(|e| AppError::new("internal", "The native catalog worker stopped", e))?
    }
    #[tauri::command]
    pub async fn discover_components(
        state: State<'_>,
        project: Project,
    ) -> Result<ComponentCatalog> {
        let app = state.inner().clone();
        background(move || app.discover_components(project)).await
    }
    #[tauri::command]
    pub async fn discover_extras(state: State<'_>, project: Project) -> Result<ComponentCatalog> {
        let app = state.inner().clone();
        background(move || app.discover_extras(project)).await
    }
    #[tauri::command]
    pub async fn discover_variants(state: State<'_>, project: Project) -> Result<Vec<Variant>> {
        let app = state.inner().clone();
        background(move || app.discover_variants(project)).await
    }
    #[tauri::command]
    pub async fn discover_weapon_models(state: State<'_>, project: Project) -> Result<Vec<Variant>> {
        let app = state.inner().clone();
        background(move || app.discover_weapon_models(project)).await
    }
    #[tauri::command]
    pub async fn preview_image(
        state: State<'_>,
        path: PathBuf,
        max_dimension: Option<u32>,
    ) -> Result<tauri::ipc::Response> {
        let app = state.inner().clone();
        background(move || app.preview_image(&path, max_dimension.unwrap_or(256)))
            .await
            .map(tauri::ipc::Response::new)
    }
    #[tauri::command]
    pub async fn file_stamps(paths: Vec<PathBuf>) -> Result<Vec<Option<super::FileStamp>>> {
        background(move || Ok(super::file_stamps(&paths))).await
    }
    #[tauri::command]
    pub async fn preview_extra(state: State<'_>, request: ExtraPreviewRequest) -> Result<tauri::ipc::Response> {
        let app = state.inner().clone();
        background(move || app.preview_extra(request)).await.map(tauri::ipc::Response::new)
    }
    #[tauri::command]
    pub async fn hero_scene(state: State<'_>, project: Project) -> Result<tauri::ipc::Response> {
        let app = state.inner().clone();
        background(move || app.hero_scene(project)).await.map(tauri::ipc::Response::new)
    }
    #[tauri::command]
    pub async fn replacement_color_scene(state: State<'_>, project: Project) -> Result<tauri::ipc::Response> {
        let app = state.inner().clone();
        background(move || app.replacement_color_scene(project)).await.map(tauri::ipc::Response::new)
    }
    #[tauri::command]
    pub async fn effect_trees(state: State<'_>, project: Project) -> Result<super::EffectTrees> {
        let app = state.inner().clone();
        background(move || app.effect_trees(project)).await
    }
    #[tauri::command]
    pub async fn effect_preview(
        state: State<'_>,
        project: Project,
        target: String,
        edit: super::TreeEdit,
        pick: Option<super::PickRequest>,
        main: Option<[u8; 3]>,
    ) -> Result<super::EffectPreview> {
        let app = state.inner().clone();
        background(move || app.effect_preview(project, target, edit, pick, main)).await
    }
}

#[cfg(test)]
mod kept_effects_tests {
    use super::*;
    use deadlock_components::resources::{Association, CatalogRow};

    fn row(target: &str) -> CatalogRow {
        CatalogRow {
            path: target.into(), target: target.into(), label: target.into(),
            association: Association { association: "hero_directory".into(), shared: false },
            supported: true, reason: String::new(), original_sha256: Some("00".into()),
            metadata: None, wav_supported: false, sound: None,
        }
    }
    fn identity<'a>(pak: &'a str, hero: &'a str, key: &'a str, rows: &'a [CatalogRow]) -> CatalogIdentity<'a> {
        CatalogIdentity { pak, hero, archive_key: key, effect_rows: rows }
    }

    #[test]
    fn loading_the_same_heros_assets_again_keeps_the_decoded_effects() {
        let rows = vec![row("particles/a.vpcf")];
        let slot = Arc::new(Mutex::new(Some(("key".to_string(), 7u32))));
        let kept = kept_effects(Some((identity("pak01_dir.vpk", "models/heroes/haze/haze.vmdl", "key", &rows), &slot)), identity("pak01_dir.vpk", "models/heroes/haze/haze.vmdl", "key", &rows.clone()));
        assert!(Arc::ptr_eq(&kept, &slot));
        assert_eq!(*kept.lock().unwrap(), Some(("key".to_string(), 7)));
    }

    #[test]
    fn another_hero_archive_identity_or_effect_list_starts_empty() {
        let rows = vec![row("particles/a.vpcf")];
        let other_rows = vec![row("particles/b.vpcf")];
        let slot = Arc::new(Mutex::new(Some(("key".to_string(), 7u32))));
        let old = || Some((identity("pak01_dir.vpk", "models/heroes/haze/haze.vmdl", "key", &rows), &slot));
        for new in [
            identity("pak01_dir.vpk", "models/heroes_wip/geist/geist.vmdl", "key", &rows),
            identity("other/pak01_dir.vpk", "models/heroes/haze/haze.vmdl", "key", &rows),
            identity("pak01_dir.vpk", "models/heroes/haze/haze.vmdl", "changed", &rows),
            identity("pak01_dir.vpk", "models/heroes/haze/haze.vmdl", "key", &other_rows),
        ] {
            let fresh = kept_effects(old(), new);
            assert!(!Arc::ptr_eq(&fresh, &slot));
            assert!(fresh.lock().unwrap().is_none());
        }
        assert!(kept_effects::<u32>(None, identity("pak01_dir.vpk", "models/heroes/haze/haze.vmdl", "key", &rows)).lock().unwrap().is_none());
    }
}
