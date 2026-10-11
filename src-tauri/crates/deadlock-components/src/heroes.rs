use crate::{Result, fail};
use deadlock_core::{safety::relative_asset, text::casefold};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Debug, Clone, Deserialize)]
pub struct Member {
    pub name: String,
    pub path: String,
}
#[derive(Debug, Clone, Deserialize)]
pub struct Hero {
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub extras: Vec<Member>,
}
pub fn roster() -> Result<Vec<Hero>> {
    serde_json::from_str(include_str!(
        "../../../../resources/catalog/hero_roster.json"
    ))
    .map_err(fail)
}
fn owner_keys(path: &str) -> BTreeSet<String> {
    let value = casefold(path);
    let parts: Vec<_> = value.split('/').collect();
    for (i, part) in parts.iter().enumerate() {
        if ["heroes", "heroes_staging", "heroes_wip"].contains(part) && i + 2 < parts.len() {
            let owner = parts[i + 1];
            return [owner.to_string(), strip_version(owner)].into();
        }
    }
    BTreeSet::new()
}
pub fn strip_version(value: &str) -> String {
    static PATTERN: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    PATTERN
        .get_or_init(|| regex::Regex::new(r"_v\d+$").unwrap())
        .replace(value, "")
        .into_owned()
}
fn normalize(value: &str) -> String {
    static PATTERN: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    PATTERN
        .get_or_init(|| regex::Regex::new(r"[^a-z0-9]+").unwrap())
        .replace_all(value, "_")
        .trim_matches('_')
        .into()
}
/// Authoritative model/roster aliases only; editable hero labels never grant ownership.
pub fn tokens(original_model: &str) -> Result<BTreeSet<String>> {
    let path = relative_asset(original_model)?;
    let folded = casefold(&path);
    let stem = folded
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .trim_end_matches("_c")
        .trim_end_matches(".vmdl")
        .to_owned();
    let owners = owner_keys(&path);
    let mut keys = owners.clone();
    keys.insert(stem);
    let mut names = keys.clone();
    let mut labels = BTreeSet::new();
    for hero in roster()? {
        if casefold(&hero.path) == folded.trim_end_matches("_c")
            || !owner_keys(&hero.path).is_disjoint(&owners)
        {
            labels.insert(hero.name.clone());
            names.insert(casefold(&hero.name));
        }
    }
    let aliases: BTreeMap<String, Vec<String>> = serde_json::from_str(include_str!(
        "../../../../resources/catalog/hero_aliases.json"
    ))
    .map_err(fail)?;
    for key in keys {
        if let Some(values) = aliases.get(&key) {
            labels.extend(values.iter().cloned());
        }
    }
    for (key, values) in aliases {
        if values.iter().any(|v| labels.contains(v)) {
            names.insert(casefold(&key));
            names.extend(values.iter().map(|v| casefold(v)));
        }
    }
    Ok(names
        .into_iter()
        .filter(|x| !x.is_empty())
        .map(|x| normalize(&x))
        .collect())
}
pub fn matches(path: &str, tokens: &BTreeSet<String>) -> bool {
    let value = format!("_{}_", normalize(&casefold(path)));
    tokens
        .iter()
        .any(|token| value.contains(&format!("_{token}_")))
}
pub fn owned(path: &str, tokens: &BTreeSet<String>) -> bool {
    let parts: Vec<_> = path.split('/').collect();
    parts[..parts.len().saturating_sub(1)]
        .iter()
        .any(|part| tokens.contains(&strip_version(&casefold(part))))
}
