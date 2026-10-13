use crate::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::time::UNIX_EPOCH;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum EngineInput {
    Hash(String),
    Metadata((u64, i64)),
}
pub type EngineFingerprint = BTreeMap<String, EngineInput>;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CachePlan {
    pub material: String,
    pub key: String,
    pub cacheable: bool,
    pub source: Hashes,
    pub engine: EngineFingerprint,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CacheStoreReceipt {
    pub cacheable: bool,
    pub files: Hashes,
    pub missing_dependencies: Vec<String>,
}
fn sorted_json(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => quote(s),
        serde_json::Value::Array(v) => format!(
            "[{}]",
            v.iter().map(sorted_json).collect::<Vec<_>>().join(", ")
        ),
        serde_json::Value::Object(v) => {
            let map: BTreeMap<_, _> = v.iter().collect();
            format!(
                "{{{}}}",
                map.into_iter()
                    .map(|(k, v)| format!("{}: {}", quote(k), sorted_json(v)))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
        _ => value.to_string(),
    }
}
pub fn cache_key_from_inputs(source: &Hashes, engine: &EngineFingerprint) -> Result<String> {
    use sha2::{Digest, Sha256};
    let value = serde_json::json!({"source":source,"engine":engine});
    Ok(format!(
        "{:x}",
        Sha256::digest(sorted_json(&value).as_bytes())
    ))
}
fn metadata(path: &Path) -> Result<EngineInput> {
    deadlock_vpk::check_unlinked_path(path, false)?;
    let info = path.metadata()?;
    let modified = info.modified()?;
    let ns = match modified.duration_since(UNIX_EPOCH) {
        Ok(value) => {
            i128::try_from(value.as_nanos()).map_err(|_| invalid("Engine timestamp overflow."))?
        }
        Err(value) => -i128::try_from(value.duration().as_nanos())
            .map_err(|_| invalid("Engine timestamp overflow."))?,
    };
    let ns = i64::try_from(ns)
        .map_err(|_| invalid("Engine timestamp exceeds supported nanosecond range."))?;
    Ok(EngineInput::Metadata((info.len(), ns)))
}
pub fn plan_cache(
    material: &str,
    content: &Path,
    compiler: &Path,
    csdk: &Path,
    cancel: &CancellationToken,
) -> Result<CachePlan> {
    let material = safety::relative_asset(material)?;
    let source = inside(content, &material)?;
    if !compiler.is_file() {
        return Err(invalid("Select resourcecompiler.exe in Setup."));
    }
    deadlock_vpk::check_unlinked_path(compiler, false)?;
    let text = read_text(&source)?;
    let refs = regex::Regex::new(
        r#"(?i)"((?:[^"\\]|\\.)+\.(?:png|tga|jpg|jpeg|tif|tiff|exr|vtex|vmat))""#,
    )
    .expect("constant cache reference regex");
    let mut files = Hashes::new();
    files.insert(
        material.clone(),
        safety::digest_with_cancel(&source, cancel)?,
    );
    let mut cacheable = true;
    for capture in refs.captures_iter(&text) {
        cancel.check()?;
        let reference = &capture[1];
        let path = inside(content, &reference.replace("\\\\", "/"))?;
        let extension = path
            .extension()
            .and_then(|p| p.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if path.is_file() && !matches!(extension.as_str(), "vmat" | "vtex") {
            files.insert(reference.into(), safety::digest_with_cancel(&path, cancel)?);
        } else {
            cacheable = false;
        }
    }
    let mut engine = EngineFingerprint::new();
    engine.insert(
        "compiler".into(),
        EngineInput::Hash(safety::digest_with_cancel(compiler, cancel)?),
    );
    let gameinfo = csdk.join("game/citadel/gameinfo.gi");
    if gameinfo.is_file() {
        deadlock_vpk::check_unlinked_path(&gameinfo, false)?;
        engine.insert(
            "explicit_gameinfo".into(),
            EngineInput::Hash(safety::digest_with_cancel(&gameinfo, cancel)?),
        );
    }
    if let Some(parent) = compiler.parent() {
        for file in fs::read_dir(parent)? {
            cancel.check()?;
            let file = file?.path();
            let name = file.file_name().and_then(|p| p.to_str()).unwrap_or("");
            if name.ends_with(".dll") && file.is_file() {
                engine.insert(path_text(&file)?, metadata(&file)?);
            }
        }
    }
    for sub in ["game/citadel", "game/core"] {
        let folder = csdk.join(sub);
        if folder.is_dir() {
            deadlock_vpk::check_unlinked_path(&folder, false)?;
            for file in fs::read_dir(folder)? {
                cancel.check()?;
                let file = file?.path();
                if file.is_file()
                    && file
                        .file_name()
                        .and_then(|p| p.to_str())
                        .is_some_and(|v| v.starts_with("shaders"))
                {
                    engine.insert(path_text(&file)?, metadata(&file)?);
                }
            }
        }
    }
    Ok(CachePlan {
        material,
        key: cache_key_from_inputs(&files, &engine)?,
        cacheable,
        source: files,
        engine,
    })
}
/// Capture opaque compiler outputs before invocation, for changed-output discovery.
pub fn compiled_inventory(game: &Path, cancel: &CancellationToken) -> Result<Hashes> {
    InventoryCache::default().inventory(game, cancel)
}
/// `compiled_inventory` across several calls: a file keeps its hash while its
/// size and modification time are unchanged, so a build that compiles many
/// materials rehashes only what the compiler wrote since the last call. A
/// stale entry cannot pass unnoticed: `store_compiled` digests every file it
/// copies against these hashes.
#[derive(Default)]
pub struct InventoryCache {
    known: std::collections::HashMap<std::path::PathBuf, (u64, Option<std::time::SystemTime>, String)>,
}
impl InventoryCache {
    pub fn inventory(&mut self, game: &Path, cancel: &CancellationToken) -> Result<Hashes> {
        let mut files = Hashes::new();
        if !game.is_dir() {
            return Ok(files);
        }
        for file in walk_files(game, cancel)? {
            let rel = relative_file(game, &file)?;
            if !rel.ends_with("_c") {
                continue;
            }
            let metadata = file.metadata()?;
            let stamp = (metadata.len(), metadata.modified().ok());
            let hash = match self.known.get(&file) {
                Some((len, modified, hash)) if (*len, *modified) == stamp => hash.clone(),
                _ => {
                    let hash = safety::digest_with_cancel(&file, cancel)?;
                    self.known.insert(file.clone(), (stamp.0, stamp.1, hash.clone()));
                    hash
                }
            };
            files.insert(rel, hash);
        }
        Ok(files)
    }
}
fn compiled_references(path: &Path) -> Result<Vec<String>> {
    if path.metadata()?.len() > deadlock_vpk::MAX_READ_BYTES {
        return Err(invalid(
            "Compiled material dependency exceeds the 512 MiB validation limit.",
        ));
    }
    let bytes = fs::read(path)?;
    deadlock_vpk::resource_blocks(&bytes)?;
    Ok(deadlock_vpk::resource_references(&bytes)?)
}
/// Returns false for a cache miss/corrupt cached payload, preserving generated
/// output until all cached hashes are checked. Unsafe paths remain hard errors.
pub fn restore_cached(
    cache: &Path,
    material: &str,
    game: &Path,
    cancel: &CancellationToken,
) -> Result<bool> {
    let material = safety::relative_asset(material)?;
    let manifest = inside(cache, "manifest.json")?;
    if !manifest.is_file() {
        return Ok(false);
    }
    let entries: Hashes = serde_json::from_str(&read_text(&manifest)?)?;
    if !entries.contains_key(&(material + "_c")) {
        return Ok(false);
    }
    for (rel, sha) in &entries {
        cancel.check()?;
        if !rel.ends_with("_c") {
            return Err(invalid("Material cache contains a noncompiled asset."));
        }
        let file = inside(&cache.join("files"), rel)?;
        if !file.is_file() || safety::digest_with_cancel(&file, cancel)? != *sha {
            return Ok(false);
        }
        for reference in compiled_references(&file)? {
            let compiled = if reference.ends_with("_c") {
                reference
            } else {
                reference + "_c"
            };
            if !entries.contains_key(&compiled) {
                return Ok(false);
            }
        }
        inside(game, rel)?;
    }
    for rel in entries.keys() {
        copy_file(
            &inside(&cache.join("files"), rel)?,
            &inside(game, rel)?,
            cancel,
        )?;
    }
    Ok(true)
}
/// Store changed compiled outputs plus the transitive RERL dependency graph,
/// including unchanged shared dependencies. Missing dependencies disable caching.
pub fn store_compiled(
    cache: &Path,
    material: &str,
    game: &Path,
    before: &Hashes,
    cancel: &CancellationToken,
) -> Result<CacheStoreReceipt> {
    let after = compiled_inventory(game, cancel)?;
    store_compiled_with(cache, material, game, before, after, cancel)
}
/// `store_compiled` with the inventory after compilation already taken.
pub fn store_compiled_with(
    cache: &Path,
    material: &str,
    game: &Path,
    before: &Hashes,
    after: Hashes,
    cancel: &CancellationToken,
) -> Result<CacheStoreReceipt> {
    let material = safety::relative_asset(material)?;
    let model = format!("{material}_c");
    let mut entries: Hashes = after
        .iter()
        .filter(|(rel, hash)| *rel == &model || before.get(*rel) != Some(*hash))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let mut pending = vec![model];
    let mut seen = BTreeSet::new();
    let mut missing = BTreeSet::new();
    while let Some(rel) = pending.pop() {
        cancel.check()?;
        if !seen.insert(rel.clone()) {
            continue;
        }
        let file = inside(game, &rel)?;
        if !file.is_file() {
            missing.insert(rel);
            continue;
        }
        entries.insert(rel, safety::digest_with_cancel(&file, cancel)?);
        for reference in compiled_references(&file)? {
            pending.push(if reference.ends_with("_c") {
                reference
            } else {
                reference + "_c"
            });
        }
    }
    if !missing.is_empty() {
        return Ok(CacheStoreReceipt {
            cacheable: false,
            files: entries,
            missing_dependencies: missing.into_iter().collect(),
        });
    }
    deadlock_vpk::check_unlinked_path(cache, true)?;
    let parent = cache
        .parent()
        .ok_or_else(|| invalid("Material cache has no parent."))?;
    fs::create_dir_all(parent)?;
    let staged = tempfile::Builder::new()
        .prefix("material-cache-pending-")
        .tempdir_in(parent)?;
    for (rel, sha) in &entries {
        let file = inside(game, rel)?;
        let destination = inside(&staged.path().join("files"), rel)?;
        copy_file(&file, &destination, cancel)?;
        if safety::digest_with_cancel(&destination, cancel)? != *sha
            || safety::digest_with_cancel(&file, cancel)? != *sha
        {
            return Err(invalid(format!(
                "Compiled material changed during cache creation: {rel}"
            )));
        }
    }
    safety::write_json(&staged.path().join("manifest.json"), &entries)?;
    cancel.check()?;
    let backup = tempfile::Builder::new()
        .prefix("material-cache-backup-")
        .tempdir_in(parent)?;
    let previous = backup.path().join("previous");
    let exists = cache.exists();
    if exists {
        fs::rename(cache, &previous)?;
    }
    if let Err(error) = deadlock_vpk::publish_directory(staged.path(), cache) {
        if exists {
            fs::rename(&previous, cache)?;
        }
        return Err(error.into());
    }
    Ok(CacheStoreReceipt {
        cacheable: true,
        files: entries,
        missing_dependencies: Vec::new(),
    })
}
