//! Installed mods after a game update: which VPKs in the game's addons folder
//! carry hero models compiled before the game's October 16, 2026 attachment
//! change (camera pivots rotated by 90 degrees in a match), and repairing them
//! in place without their sources. Any VPK is handled, not only this app's:
//! the hero's current compiled model in the game archive is the authority
//! (`deadlock_resource::attachments`). The original VPK is kept as a backup.
use crate::{invalid, vpk_error, Result};
use deadlock_core::{safety, CancellationToken, CoreError};
use deadlock_project::Settings;
use deadlock_resource::attachments::{self, Convention};
use deadlock_vpk::Vpk;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Every hero model matches the game's current convention.
    UpToDate,
    /// At least one hero model uses the previous convention and can be updated.
    NeedsUpdate,
    /// The VPK replaces no hero model (materials, sounds, icons…): unaffected.
    NoHeroModel,
    /// A hero model whose attachments match neither convention: left alone.
    Unknown,
    /// Needs an update but cannot be rewritten here (see `note`).
    CannotUpdate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModModel {
    /// The game model path the VPK replaces.
    pub path: String,
    /// The hero's name when the roster knows the model.
    pub hero: Option<String>,
    pub convention: Convention,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledMod {
    pub path: String,
    pub file: String,
    pub bytes: u64,
    pub models: Vec<ModModel>,
    pub status: Status,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateReport {
    pub path: String,
    pub file: String,
    /// Hero models whose attachments were rewritten.
    pub updated: Vec<String>,
    /// Custom attachments converted by the rule, per model.
    pub converted: Vec<String>,
    pub backup: String,
}

/// After compiling: the freshly compiled model at `output` takes the attachment
/// values of `game_model` (the game's current compiled model of the hero).
/// Only the attachment lists of the compiled model change; its skeleton,
/// meshes and everything else stay. Returns how many attachments were taken.
pub fn take_game_attachments(output: &Path, game_model: &[u8]) -> Result<usize> {
    let compiled = fs::read(output).map_err(|e| CoreError::io("Read compiled model", e))?;
    let Some(update) = attachments::update(&compiled, game_model, false).map_err(|e| invalid(e.to_string()))? else {
        return Ok(0);
    };
    // The result must still be the compiled model: same model data block
    // (skeleton, meshes, materials). Anything else would replace the build.
    let before = deadlock_resource::container::read_blocks(&compiled, "DATA").map_err(|e| invalid(e.to_string()))?;
    let after = deadlock_resource::container::read_blocks(&update.bytes, "DATA").map_err(|e| invalid(e.to_string()))?;
    if before.len() != after.len() || before.iter().zip(&after).any(|(a, b)| !a.root.canonical_eq(&b.root)) {
        return Err(invalid("Taking the game's attachments would change the compiled model's data; the compiled model was left as it is."));
    }
    safety::atomic_write(output, &update.bytes)?;
    Ok(update.taken_from_game)
}

fn addons(settings: &Settings) -> Result<PathBuf> {
    if settings.deadlock.is_empty() {
        return Err(invalid("Choose the Deadlock folder in Settings first."));
    }
    let folder = Path::new(&settings.deadlock).join("game/citadel/addons");
    if !folder.is_dir() {
        return Err(invalid(format!("The game's addons folder was not found: {}", folder.display())));
    }
    Ok(folder)
}

fn hero_models(vpk: &Vpk) -> Vec<String> {
    vpk.entries
        .keys()
        .filter(|k| k.ends_with(".vmdl_c") && (k.starts_with("models/heroes") || k.starts_with("models/heroes_staging") || k.starts_with("models/heroes_wip")))
        .cloned()
        .collect()
}

fn single_file(vpk: &Vpk) -> bool {
    vpk.entries.values().all(|e| e.archive == 0x7fff)
}

/// Every VPK in the game's addons folder (the game's own `pak01_dir.vpk` left out)
/// with the state of the hero models it carries.
pub fn scan(settings: &Settings, cancel: &CancellationToken) -> Result<Vec<InstalledMod>> {
    let folder = addons(settings)?;
    let game = Vpk::open(&settings.pak).map_err(|e| invalid(format!("The game archive could not be read: {e}")))?;
    let mut out = vec![];
    let mut names: Vec<PathBuf> = fs::read_dir(&folder)
        .map_err(|e| CoreError::io("Read addons folder", e))?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("vpk")))
        .collect();
    names.sort();
    for path in names {
        cancel.check()?;
        let file = path.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
        if file.eq_ignore_ascii_case("pak01_dir.vpk") || !file.to_ascii_lowercase().ends_with("_dir.vpk") {
            continue;
        }
        let bytes = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        let vpk = match Vpk::open(&path) {
            Ok(vpk) => vpk,
            Err(error) => {
                out.push(InstalledMod { path: path.display().to_string(), file, bytes, models: vec![], status: Status::Unknown, note: format!("Not readable as a VPK: {error}") });
                continue;
            }
        };
        let mut models = vec![];
        let mut note = String::new();
        for model_path in hero_models(&vpk) {
            cancel.check()?;
            let Ok(original) = game.read(&model_path) else {
                note = format!("{model_path} is not a model of the installed game.");
                continue;
            };
            let candidate = vpk.read(&model_path).map_err(vpk_error)?;
            let convention = attachments::convention(&candidate, &original).map_err(|e| invalid(e.to_string()))?;
            let hero = deadlock_components::heroes::display_name(model_path.trim_end_matches("_c"));
            models.push(ModModel { path: model_path, hero, convention });
        }
        let status = if models.is_empty() {
            Status::NoHeroModel
        } else if models.iter().any(|m| m.convention == Convention::Previous) {
            if single_file(&vpk) { Status::NeedsUpdate } else {
                note = "This VPK is split into several files; rebuild the mod instead.".into();
                Status::CannotUpdate
            }
        } else if models.iter().any(|m| m.convention == Convention::Unknown) {
            if note.is_empty() { note = "Its attachments match neither the current nor the previous game convention; it was left alone.".into(); }
            Status::Unknown
        } else {
            Status::UpToDate
        };
        if status == Status::NeedsUpdate {
            note = "Built before the game's October 16, 2026 update: camera pivots are rotated in a match.".into();
        }
        out.push(InstalledMod { path: path.display().to_string(), file, bytes, models, status, note });
    }
    Ok(out)
}

/// Rewrites the hero models of one installed VPK to the game's current
/// attachment convention. The VPK is rebuilt from its own entries (nothing else
/// changes), verified by reopening, and the previous file is kept under
/// `backups` (pruned to `keep_backups`).
pub fn update(
    settings: &Settings,
    path: &Path,
    backups: &Path,
    keep_backups: usize,
    cancel: &CancellationToken,
    log: &dyn Fn(&str),
) -> Result<UpdateReport> {
    let folder = addons(settings)?;
    let canonical = fs::canonicalize(path).map_err(|e| CoreError::io("Resolve VPK path", e))?;
    if fs::canonicalize(&folder).map(|f| !canonical.starts_with(&f)).unwrap_or(true) {
        return Err(invalid("Only VPKs in the game's addons folder are updated here."));
    }
    let file = path.file_name().and_then(|n| n.to_str()).ok_or_else(|| invalid("Unsupported VPK filename."))?.to_string();
    if file.eq_ignore_ascii_case("pak01_dir.vpk") {
        return Err(invalid("pak01_dir.vpk is the game's own archive and is never changed."));
    }
    let game = Vpk::open(&settings.pak).map_err(|e| invalid(format!("The game archive could not be read: {e}")))?;
    let vpk = Vpk::open(path).map_err(|e| invalid(format!("{file} could not be read as a VPK: {e}")))?;
    if !single_file(&vpk) {
        return Err(invalid(format!("{file} is split into several files; rebuild the mod instead.")));
    }
    let stage = tempfile::Builder::new()
        .prefix(".dmb-mod-update-")
        .tempdir_in(path.parent().ok_or_else(|| invalid("VPK needs a parent folder."))?)
        .map_err(|e| CoreError::io("Create update staging folder", e))?;
    let mut files = vec![];
    let mut updated = vec![];
    let mut converted = vec![];
    for relative in vpk.entries.keys() {
        cancel.check()?;
        let mut bytes = vpk.read(relative).map_err(vpk_error)?;
        let is_hero_model = hero_models(&vpk).contains(relative);
        if is_hero_model {
            if let Ok(original) = game.read(relative) {
                if let Some(change) = attachments::update(&bytes, &original, true).map_err(|e| invalid(format!("{relative}: {e}")))? {
                    // Reopen the result before trusting it.
                    if attachments::convention(&change.bytes, &original).map_err(|e| invalid(e.to_string()))? != Convention::Current {
                        return Err(invalid(format!("{relative}: the rewritten model does not match the game's convention; nothing was changed.")));
                    }
                    log(&format!("{file}: {relative}: {} attachments taken from the game's model, {} custom converted ({} mesh blocks)", change.taken_from_game, change.converted.len(), change.blocks));
                    converted.extend(change.converted.iter().map(|c| format!("{relative}: {c}")));
                    bytes = change.bytes;
                    updated.push(relative.clone());
                }
            }
        }
        let target = stage.path().join(relative.replace('/', std::path::MAIN_SEPARATOR_STR));
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| CoreError::io("Create staging folder", e))?;
        }
        safety::atomic_write(&target, &bytes)?;
        files.push(relative.clone());
    }
    if updated.is_empty() {
        return Err(invalid(format!("{file} needs no update.")));
    }
    // Release the original archive before replacing it (Windows refuses to
    // replace an open file).
    let version = vpk.version;
    let entry_count = vpk.entries.len();
    drop(vpk);
    // The writer refuses an output inside the folder it packs, so the rebuilt
    // archive goes to a sibling temporary folder (removed with it on any error).
    let output = tempfile::Builder::new()
        .prefix(".dmb-mod-update-out-")
        .tempdir_in(path.parent().ok_or_else(|| invalid("VPK needs a parent folder."))?)
        .map_err(|e| CoreError::io("Create update output folder", e))?;
    let pending = output.path().join("rebuilt.vpk");
    let mut check = || cancel.check().map_err(|_| deadlock_vpk::Error::Cancelled);
    deadlock_vpk::write_vpk(stage.path(), &files, &pending, None, version, &mut check).map_err(vpk_error)?;
    let rebuilt = Vpk::open(&pending).map_err(vpk_error)?;
    if rebuilt.entries.len() != entry_count {
        return Err(invalid("The rebuilt VPK lost entries; nothing was changed."));
    }
    for relative in &updated {
        let original = game.read(relative).map_err(vpk_error)?;
        let model = rebuilt.read(relative).map_err(vpk_error)?;
        if attachments::convention(&model, &original).map_err(|e| invalid(e.to_string()))? != Convention::Current {
            return Err(invalid(format!("{relative} in the rebuilt VPK does not match the game; nothing was changed.")));
        }
    }
    drop(rebuilt);
    fs::create_dir_all(backups).map_err(|e| CoreError::io("Create backup folder", e))?;
    crate::package::publish_copy(&pending, path, backups, keep_backups, true, cancel)?;
    let backup = fs::read_dir(backups)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with(&file)))
        .max_by_key(|p| fs::metadata(p).and_then(|m| m.modified()).ok())
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    log(&format!("{file}: updated in place; the previous file is kept in {}", backups.display()));
    Ok(UpdateReport { path: path.display().to_string(), file, updated, converted, backup })
}
