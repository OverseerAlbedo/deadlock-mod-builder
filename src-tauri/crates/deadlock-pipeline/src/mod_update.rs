//! Installed mods after a game update: which VPKs in the game's addons folder
//! carry hero models compiled before the game's October 16, 2026 attachment
//! change (camera pivots rotated by 90 degrees in a match), and repairing them
//! in place without their sources. Any VPK is handled, not only this app's:
//! the hero's current compiled model in the game archive is the authority
//! (`deadlock_resource::attachments`). The original VPK is kept as a backup.
//!
//! The scan also says what every VPK is, from the paths in its directory only
//! (`deadlock_components::vpk_contents`): heroes, kinds of change, a content
//! identity for the user's labels, and the game files two VPKs both hold.
use crate::{invalid, vpk_error, Result};
use deadlock_components::effect_catalog::{self, FileUse};
use deadlock_components::vpk_contents::{self, Contents, HeroTable};
use deadlock_core::{safety, CancellationToken, CoreError};
use deadlock_project::Settings;
use deadlock_resource::attachments::{self, Convention};
use deadlock_vpk::Vpk;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

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
    /// The content identity of its directory (`vpk_contents::identity`); empty
    /// when the file could not be read.
    #[serde(default)]
    pub identity: String,
    /// Why the file could not be read as a VPK; absent otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unreadable: Option<String>,
    /// What it changes, from the paths in its directory.
    #[serde(default)]
    pub contents: Contents,
    /// Other scanned VPKs that hold some of the same game files.
    #[serde(default)]
    pub shared: Vec<SharedFiles>,
}

/// Game files two VPKs both hold. Which one the game uses is not decided here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharedFiles {
    pub file: String,
    pub path: String,
    pub count: usize,
    /// Up to `vpk_contents::SAMPLE_PATHS` of them, sorted.
    pub paths: Vec<String>,
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

/// A chunk of a split VPK (`pak05_000.vpk` beside `pak05_dir.vpk`): read
/// through its `_dir.vpk`. A `…_001.vpk` without such a directory file is a
/// VPK of its own.
fn chunk_file(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else { return false };
    let lower = name.to_ascii_lowercase();
    let Some((stem, n)) = lower.strip_suffix(".vpk").and_then(|stem| stem.rsplit_once('_')) else { return false };
    n.len() == 3 && n.bytes().all(|b| b.is_ascii_digit()) && path.with_file_name(format!("{}_dir.vpk", &name[..stem.len()])).is_file()
}

/// The VPK files of the addons folder, sorted (chunks of split VPKs left out).
/// Links to files count as files.
fn vpk_files(folder: &Path) -> Result<Vec<PathBuf>> {
    let mut names: Vec<PathBuf> = fs::read_dir(folder)
        .map_err(|e| CoreError::io("Read addons folder", e))?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.is_file())
        .filter(|path| path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("vpk")))
        .filter(|path| !chunk_file(path))
        .collect();
    names.sort();
    Ok(names)
}

/// The game definitions' file users, read once per game archive (path, size,
/// modification time, entry count) and only when a VPK needs them.
fn definition_users(game: &Vpk, cancel: &CancellationToken) -> Option<Arc<BTreeMap<String, FileUse>>> {
    static CACHE: Mutex<Option<(String, Option<Arc<BTreeMap<String, FileUse>>>)>> = Mutex::new(None);
    let key = effect_catalog::archive_key(game);
    if let Some((cached, users)) = CACHE.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        if *cached == key {
            return users.clone();
        }
    }
    let users = match effect_catalog::Definitions::read(game, cancel) {
        Ok(Some(defs)) => defs.file_users().ok().map(Arc::new),
        _ => None,
    };
    if !cancel.is_cancelled() {
        *CACHE.lock().unwrap_or_else(|e| e.into_inner()) = Some((key, users.clone()));
    }
    users
}

/// The content identity of one VPK (its directory only).
pub fn identity_of(path: &Path) -> Result<String> {
    let vpk = Vpk::open(path).map_err(vpk_error)?;
    Ok(vpk_contents::identity(vpk.entries.values()))
}

/// One read of a VPK's directory: what the scan keeps after closing it.
struct Read {
    /// Casefolded path → (CRC32, size).
    paths: BTreeMap<String, (u32, u64)>,
    mod_: InstalledMod,
}

fn read_one(path: &Path, game: &Vpk, table: &HeroTable, users: &mut dyn FnMut() -> Option<Arc<BTreeMap<String, FileUse>>>, cancel: &CancellationToken) -> Result<Read> {
    let file = path.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
    let bytes = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let blank = |status: Status, note: String| InstalledMod {
        path: path.display().to_string(),
        file: file.clone(),
        bytes,
        models: vec![],
        status,
        note,
        identity: String::new(),
        unreadable: None,
        contents: Contents::default(),
        shared: vec![],
    };
    let vpk = match Vpk::open(path) {
        Ok(vpk) => vpk,
        Err(error) => {
            let mut row = blank(Status::Unknown, format!("Not readable as a VPK: {error}"));
            row.unreadable = Some(error.to_string());
            return Ok(Read { paths: BTreeMap::new(), mod_: row });
        }
    };
    let names: Vec<&str> = vpk.entries.values().map(|e| e.path.as_str()).collect();
    let wanted = vpk_contents::wants_definitions(table, &names);
    let found = if wanted { users() } else { None };
    let contents = vpk_contents::classify(table, &names, found.as_deref());
    let identity = vpk_contents::identity(vpk.entries.values());
    let mut models = vec![];
    let mut note = String::new();
    for model_path in hero_models(&vpk) {
        cancel.check()?;
        let Ok(original) = game.read(&model_path) else {
            note = format!("{model_path} is not a model of the installed game.");
            continue;
        };
        // A model that cannot be read or compared is this file's problem, not the scan's.
        let convention = match vpk.read(&model_path).map_err(|e| e.to_string()).and_then(|candidate| attachments::convention(&candidate, &original).map_err(|e| e.to_string())) {
            Ok(convention) => convention,
            Err(error) => {
                note = format!("{model_path} could not be checked: {error}");
                Convention::Unknown
            }
        };
        let hero = table.body(&model_path).map(str::to_owned).or_else(|| deadlock_components::heroes::display_name(model_path.trim_end_matches("_c")));
        models.push(ModModel { path: model_path, hero, convention });
    }
    let game_name = file.eq_ignore_ascii_case("pak01_dir.vpk");
    let status = if models.is_empty() {
        Status::NoHeroModel
    } else if models.iter().any(|m| m.convention == Convention::Previous) {
        if !single_file(&vpk) {
            note = "This VPK is split into several files; rebuild the mod instead.".into();
            Status::CannotUpdate
        } else if game_name {
            note = "Built before the game's October 16, 2026 update, but pak01_dir.vpk is never changed here; rebuild the mod instead.".into();
            Status::CannotUpdate
        } else {
            Status::NeedsUpdate
        }
    } else if models.iter().any(|m| m.convention == Convention::Unknown) {
        if note.is_empty() {
            note = "Its attachments match neither the current nor the previous game convention; it was left alone.".into();
        }
        Status::Unknown
    } else {
        Status::UpToDate
    };
    if status == Status::NeedsUpdate {
        note = "Built before the game's October 16, 2026 update: camera pivots are rotated in a match.".into();
    }
    // Item names files: the game reads a mod's file instead of its own, so the
    // game's tokens the file lacks show without a name (compared with the
    // game's current file of the same language beside the archive; read only).
    let names_folder = deadlock_components::item_names::folder(&game.path);
    let mut missing = vec![];
    for (key, entry) in vpk.entries.iter().filter(|(k, _)| deadlock_components::item_names::is_names_file(k)) {
        cancel.check()?;
        let name = key.rsplit('/').next().unwrap_or_default();
        let language = name.strip_prefix(deadlock_components::item_names::PREFIX).and_then(|n| n.strip_suffix(".txt")).unwrap_or(name);
        let (Ok(ours), Ok(theirs)) = (vpk.read(&entry.path), fs::read(names_folder.join(name))) else { continue };
        if let Some(count) = deadlock_components::item_names::missing_from(&ours, &theirs) {
            missing.push((language.to_owned(), count));
        }
    }
    if let Some(names_note) = deadlock_components::item_names::outdated_note(&missing) {
        note = if note.is_empty() { names_note } else { format!("{note} {names_note}") };
    }
    let paths = vpk.entries.iter().map(|(k, e)| (k.clone(), (e.crc, e.length as u64 + e.preload.len() as u64))).collect();
    // Release the archive before the next one: the handle denies writes.
    drop(vpk);
    let mut row = blank(status, note);
    row.models = models;
    row.identity = identity;
    row.contents = contents;
    Ok(Read { paths, mod_: row })
}

/// Every VPK in the game's addons folder with what it holds and the state of
/// the hero models it carries. A file that cannot be read is listed with the
/// reason. Nothing is written.
pub fn scan(settings: &Settings, cancel: &CancellationToken) -> Result<Vec<InstalledMod>> {
    let folder = addons(settings)?;
    let game = Vpk::open(&settings.pak).map_err(|e| invalid(format!("The game archive could not be read: {e}")))?;
    let table = HeroTable::new()?;
    let mut loaded: Option<Option<Arc<BTreeMap<String, FileUse>>>> = None;
    let mut users = || loaded.get_or_insert_with(|| definition_users(&game, cancel)).clone();
    let mut reads = vec![];
    for path in vpk_files(&folder)? {
        cancel.check()?;
        reads.push(read_one(&path, &game, &table, &mut users, cancel)?);
    }
    cancel.check()?;
    let sets: Vec<BTreeMap<String, (u32, u64)>> = reads.iter().map(|r| r.paths.clone()).collect();
    let mut out: Vec<InstalledMod> = reads.into_iter().map(|r| r.mod_).collect();
    let shared = shared_files(&sets, &out);
    for (row, shared) in out.iter_mut().zip(shared) {
        row.shared = shared;
    }
    Ok(out)
}

const COMPILER_DEFAULTS: &str = "materials/default/";

/// Per VPK, the other VPKs holding some of the same game files with other
/// contents. Files with the same bytes in both change nothing whichever is
/// loaded; files at the archive's root (`readme.txt`) are not game files; and
/// `materials/default/` holds the compiler's copies of the engine's default
/// textures, which every compiled mod carries under the same names (their
/// bytes differ only between compiler versions). None of these count.
fn shared_files(sets: &[BTreeMap<String, (u32, u64)>], mods: &[InstalledMod]) -> Vec<Vec<SharedFiles>> {
    let mut out = vec![vec![]; sets.len()];
    for i in 0..sets.len() {
        for j in i + 1..sets.len() {
            let both: Vec<&String> = sets[i]
                .iter()
                .filter(|(path, record)| path.contains('/') && !path.starts_with(COMPILER_DEFAULTS) && sets[j].get(*path).is_some_and(|other| other != *record))
                .map(|(path, _)| path)
                .collect();
            if both.is_empty() {
                continue;
            }
            let paths: Vec<String> = both.iter().take(vpk_contents::SAMPLE_PATHS).map(|p| p.to_string()).collect();
            out[i].push(SharedFiles { file: mods[j].file.clone(), path: mods[j].path.clone(), count: both.len(), paths: paths.clone() });
            out[j].push(SharedFiles { file: mods[i].file.clone(), path: mods[i].path.clone(), count: both.len(), paths });
        }
    }
    out
}

/// A VPK in a project's output folder of the workspace (`<workspace>/<project
/// id>/output/*.vpk`) with its content identity: a build of that project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnOutput {
    pub identity: String,
    pub project_id: String,
    pub path: String,
}

/// Every VPK this app built that is still in a project's output folder.
/// Only directories are read; unreadable files are skipped.
pub fn own_outputs(workspace: &Path, cancel: &CancellationToken) -> Result<Vec<OwnOutput>> {
    let mut out = vec![];
    let Ok(projects) = fs::read_dir(workspace) else { return Ok(out) };
    let mut folders: Vec<(String, PathBuf)> = projects
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|e| e.file_name().to_str().filter(|n| crate::storage::is_project_id(n)).map(|n| (n.to_owned(), e.path())))
        .collect();
    folders.sort();
    for (id, folder) in folders {
        let Ok(files) = fs::read_dir(folder.join("output")) else { continue };
        let mut files: Vec<PathBuf> = files.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("vpk"))).collect();
        files.sort();
        for path in files {
            cancel.check()?;
            if let Ok(identity) = identity_of(&path) {
                out.push(OwnOutput { identity, project_id: id.clone(), path: path.display().to_string() });
            }
        }
    }
    Ok(out)
}

/// The project name and hero recorded by a project's last build
/// (`<workspace>/<id>/build-manifest.json`, then its `project.dmb.json`).
pub fn project_summary(workspace: &Path, project_id: &str) -> Option<(String, String)> {
    if !crate::storage::is_project_id(project_id) {
        return None;
    }
    let root = workspace.join(project_id);
    let read = |path: PathBuf, nested: bool| -> Option<(String, String)> {
        let meta = fs::metadata(&path).ok()?;
        if !meta.is_file() || meta.len() > crate::bundle::MAX_RECEIPT_BYTES {
            return None;
        }
        let value: serde_json::Value = serde_json::from_slice(&fs::read(&path).ok()?).ok()?;
        let project = if nested { value.get("project")? } else { &value };
        (project.get("id")?.as_str()? == project_id).then(|| {
            (
                project.get("name").and_then(|v| v.as_str()).unwrap_or_default().to_owned(),
                project.get("hero_name").and_then(|v| v.as_str()).unwrap_or_default().to_owned(),
            )
        })
    };
    read(root.join("build-manifest.json"), true).or_else(|| read(root.join("project.dmb.json"), false))
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
