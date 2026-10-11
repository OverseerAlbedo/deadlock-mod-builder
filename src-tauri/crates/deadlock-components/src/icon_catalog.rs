//! Image ownership comes from the installed game's hero/ability definitions.
//! Names remain a fallback and include unused portraits and alternate versions.
use crate::{Result, fail, heroes, read};
use deadlock_core::{CancellationToken, safety, text::casefold};
use deadlock_resource::{Kind, Value, container, text::{Document, Node, NodeKind}};
use deadlock_vpk::Vpk;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default, Debug)]
pub struct CatalogPaths {
    /// Value is true for an explicit reference without a hero-specific name.
    pub paths: BTreeMap<String, bool>,
    pub warnings: Vec<String>,
}

fn from_text(node: &Node) -> Value {
    Value::new(match &node.kind {
        NodeKind::Object(rows) => Kind::Object(rows.iter().map(|(k,v)|(k.clone(),from_text(v))).collect()),
        NodeKind::Array(rows) => Kind::Array(rows.iter().map(from_text).collect()),
        NodeKind::Typed { value, .. } => return from_text(value),
        NodeKind::Scalar { value, .. } => Kind::String(value.clone()),
        NodeKind::Blob(value) => Kind::Blob(value.clone()),
    })
}

pub fn definition(raw: &[u8]) -> Result<Value> {
    if let Ok(text) = std::str::from_utf8(raw) {
        let text = text.trim_start_matches('\u{feff}').trim_start();
        if text.starts_with("<!--") || text.starts_with('{') {
            return Ok(from_text(&Document::parse(text).map_err(fail)?.root));
        }
    }
    let mut documents = container::read_blocks(raw, "DATA").map_err(fail)?;
    if documents.len() != 1 { return Err(fail("Expected one game definition DATA block.")); }
    Ok(documents.remove(0).root)
}

fn model_key(path: &str) -> String { casefold(path).trim_end_matches("_c").to_string() }

/// Translate a game Panorama URI into the exact compiled archive name.
pub fn image_path(value: &str) -> Option<String> {
    let value = value.strip_prefix("file://{images}/")
        .map(|tail|format!("panorama/images/{tail}"))
        .or_else(||value.strip_prefix("s2r://panorama/images/").map(|tail|format!("panorama/images/{tail}")))
        .or_else(||value.starts_with("panorama/images/").then(||value.to_string()))?;
    let value = casefold(&safety::relative_asset(&value).ok()?);
    if value.ends_with(".vtex_c") || value.ends_with(".vsvg_c") { return Some(value); }
    let (stem, extension) = value.rsplit_once('.')?;
    Some(match extension {
        "svg" | "vsvg" => format!("{stem}.vsvg_c"),
        "vtex" => format!("{stem}.vtex_c"),
        "png" | "psd" | "tga" | "jpg" | "jpeg" | "webp" => format!("{stem}_{extension}.vtex_c"),
        _ => return None,
    })
}

fn image_strings(value: &Value, images: &mut BTreeSet<String>) {
    match &value.kind {
        Kind::String(s) => { if let Some(p) = image_path(s) { images.insert(p); } },
        Kind::Object(rows) => for (_,v) in rows { image_strings(v,images); },
        Kind::Array(rows) => for v in rows { image_strings(v,images); },
        _ => {},
    }
}

/// Public for the reproducible, independent whole-roster inventory audit.
pub fn referenced_images(heroes_doc: &Value, abilities_doc: Option<&Value>, model: &str) -> Result<BTreeSet<String>> {
    let tokens = heroes::tokens(model)?;
    let rows = heroes_doc.object().map_err(fail)?;
    let exact: Vec<_> = rows.iter().filter(|(_,v)|v.get("m_strModelName").and_then(|v|v.string().ok())
        .is_some_and(|p|model_key(p)==model_key(model))).collect();
    let named: Vec<_> = exact.iter().copied().filter(|(key,_)|tokens.contains(key.strip_prefix("hero_").unwrap_or(key))).collect();
    let selected: Vec<_> = if !named.is_empty() { named } else if exact.is_empty() {
        rows.iter().filter(|(key,_)|tokens.contains(key.strip_prefix("hero_").unwrap_or(key))).collect()
    } else if exact.len()==1 { exact } else {
        return Err(fail("Multiple hero definitions use this model, so their image ownership is ambiguous."));
    };
    let mut images = BTreeSet::new();
    let mut pending = Vec::new();
    for (_,hero) in selected {
        image_strings(hero,&mut images);
        if let Some(bound) = hero.get("m_mapBoundAbilities").and_then(|v|v.object().ok()) {
            pending.extend(bound.iter().filter_map(|(_,v)|v.string().ok()).map(str::to_owned));
        }
    }
    // Follow only named ability records reachable from this hero, including
    // sub-abilities/weapon sets. Do not collect every ability in the game.
    let mut visited = BTreeSet::new();
    if let Some(abilities) = abilities_doc {
        let records: BTreeMap<_,_> = abilities.object().map_err(fail)?.iter().map(|(k,v)|(k.as_str(),v)).collect();
        fn links(value: &Value, records: &BTreeMap<&str,&Value>, pending: &mut Vec<String>) {
            match &value.kind {
                Kind::String(s) if records.contains_key(s.as_str()) => pending.push(s.clone()),
                Kind::Object(rows) => for (_,v) in rows { links(v,records,pending); },
                Kind::Array(rows) => for v in rows { links(v,records,pending); },
                _ => {},
            }
        }
        while let Some(key) = pending.pop() {
            if !visited.insert(key.clone()) { continue; }
            if visited.len()>4096 { return Err(fail("Hero image references exceed 4,096 ability records.")); }
            if let Some(value) = records.get(key.as_str()) {
                image_strings(value,&mut images);
                links(value,&records,&mut pending);
            }
        }
    }
    Ok(images)
}

pub fn image_resource(path: &str) -> bool {
    path.starts_with("panorama/images/") && (path.ends_with(".vtex_c") || path.ends_with(".vsvg_c"))
}

fn variant(path: &str, references: &BTreeSet<String>) -> bool {
    let Some((stem,ext)) = path.rsplit_once('.') else { return false; };
    let Some((base,hash)) = stem.rsplit_once('_') else { return false; };
    hash.len()==8 && hash.bytes().all(|c|c.is_ascii_hexdigit()) && references.contains(&format!("{base}.{ext}"))
}

pub fn select_paths<'a>(paths: impl IntoIterator<Item=&'a str>, model: &str, references: &BTreeSet<String>) -> Result<BTreeMap<String,bool>> {
    let tokens = heroes::tokens(model)?;
    let mut result = BTreeMap::new();
    for path in paths {
        let key = casefold(path);
        if !image_resource(&key) { continue; }
        let named = heroes::matches(path,&tokens);
        if named || references.contains(&key) || variant(&key,references) {
            result.insert(path.to_string(),!named);
        }
    }
    Ok(result)
}

pub fn discover(archive: &Vpk, model: &str, cancel: &CancellationToken) -> Result<CatalogPaths> {
    let mut result = CatalogPaths::default();
    let mut load = |name: &str| -> Result<Option<Value>> {
        cancel.check()?;
        let key = [format!("scripts/{name}.vdata_c"),format!("scripts/{name}.vdata")]
            .into_iter().find(|p|archive.entries.contains_key(p));
        let Some(key) = key else { return Ok(None); };
        match read(archive,&key).and_then(|raw|definition(&raw)) {
            Ok(value) => Ok(Some(value)),
            Err(deadlock_core::CoreError::Cancelled) => Err(deadlock_core::CoreError::Cancelled),
            Err(e) => { result.warnings.push(format!("Could not read {name} image references: {e}. Name-based images are still shown.")); Ok(None) },
        }
    };
    let heroes_doc = load("heroes")?;
    let abilities_doc = if heroes_doc.is_some() { load("abilities")? } else { None };
    let references = if let Some(doc) = &heroes_doc {
        match referenced_images(doc,abilities_doc.as_ref(),model) {
            Ok(value) => value,
            Err(e) => { result.warnings.push(format!("Could not resolve this hero's image references: {e}. Name-based images are still shown.")); BTreeSet::new() },
        }
    } else { BTreeSet::new() };
    result.paths = select_paths(archive.entries.values().map(|e|e.path.as_str()),model,&references)?;
    for path in &references {
        if !archive.entries.contains_key(path) {
            result.warnings.push(format!("The game definition references an image absent from this archive: {path}. No replacement destination was invented."));
        }
    }
    cancel.check()?;
    Ok(result)
}
