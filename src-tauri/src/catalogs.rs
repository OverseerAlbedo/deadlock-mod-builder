//! Read-only current-game catalogs and bounded local image thumbnails.
use crate::app::{AppError, Application, Result};
use deadlock_components::service::{ComponentCatalog, ComponentService};
use deadlock_components::variants::Variant;
use deadlock_project::Project;
use std::path::Path;
use std::{collections::BTreeMap, sync::Arc};
use serde::{Deserialize, Serialize};
use deadlock_core::{CancellationToken, text::casefold};

pub(crate) struct PreviewCatalog {
    pak: String,
    hero: String,
    archive: Arc<deadlock_vpk::Vpk>,
    assets: BTreeMap<String, (String, String)>,
}

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

fn media(mime: &str, details: serde_json::Value, data: Vec<u8>) -> Result<Vec<u8>> {
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
            *self.extras_catalog.lock().unwrap_or_else(|e| e.into_inner()) = Some(PreviewCatalog {
                pak:settings.pak.clone(),hero:project.original_model.clone(),archive:Arc::new(archive),assets,
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
        let (png, _) = self.operation("Preview local image", |clock, cancel| {
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
            return media("image/png", serde_json::json!({"texture":texture,"tint":row.tint,"albedo":row.albedo}), png);
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
}

fn preview_file(path: &Path) -> Result<Vec<u8>> {
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
    pub async fn preview_extra(state: State<'_>, request: ExtraPreviewRequest) -> Result<tauri::ipc::Response> {
        let app = state.inner().clone();
        background(move || app.preview_extra(request)).await.map(tauri::ipc::Response::new)
    }
}
