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
    /// False for a hero whose files are in the game but who cannot be picked in
    /// a match yet. Since October 2026 the game's own definitions say so
    /// (`m_eHeroDevelopmentState`, see [`GameTarget::PreRelease`]); this flag is
    /// the fallback for an archive whose definitions cannot be read.
    #[serde(default = "playable")]
    pub playable: bool,
}
fn playable() -> bool {
    true
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

/// Which model the installed game loads for each hero definition
/// (`scripts/heroes.vdata`, `m_strModelName`). A readable hero name, an existing
/// model file and a successful build do not establish this: the game can select
/// a different model of the same hero (Rem loads `familiar_wip.vmdl`).
#[derive(Debug, Clone, Default)]
pub struct GameModels {
    /// Casefolded `.vmdl` path → hero definition keys that load it.
    pub by_model: BTreeMap<String, Vec<String>>,
    /// Hero definitions the game marks `m_bDisabled` (heroes in development).
    pub disabled: BTreeSet<String>,
    /// Hero definitions whose `m_eHeroDevelopmentState` is `PreRelease`: the
    /// files are in the game but the hero cannot be picked in a match yet
    /// (Nurse Harrow until October 20, 2026).
    pub pre_release: BTreeSet<String>,
}
#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum GameTarget {
    /// Hero definitions in the installed game load this model.
    Active { heroes: Vec<String> },
    /// A roster model of a hero whose game definition loads another model.
    Unused { model: String },
    /// The hero's definitions load several different models.
    Ambiguous { models: Vec<String> },
    /// Only hero definitions the game disables load this model (in development).
    Disabled { heroes: Vec<String> },
    /// The enabled hero definitions loading this model are all pre-release: the
    /// hero is announced but cannot be picked in a match yet.
    PreRelease { heroes: Vec<String> },
}
fn model_key(path: &str) -> String {
    casefold(path).trim_end_matches("_c").to_owned()
}
impl GameModels {
    pub fn from_definition(doc: &deadlock_resource::Value) -> Result<Self> {
        let mut by_model: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut disabled = BTreeSet::new();
        let mut pre_release = BTreeSet::new();
        for (key, hero) in doc.object().map_err(fail)? {
            if let Some(model) = hero.get("m_strModelName").and_then(|v| v.string().ok()).filter(|m| !m.is_empty()) {
                by_model.entry(model_key(model)).or_default().push(key.clone());
            }
            if matches!(hero.get("m_bDisabled").map(|v| &v.kind), Some(deadlock_resource::Kind::Bool(true))) {
                disabled.insert(key.clone());
            }
            if hero.get("m_eHeroDevelopmentState").and_then(|v| v.string().ok()) == Some("EHeroDevState_PreRelease") {
                pre_release.insert(key.clone());
            }
        }
        Ok(Self { by_model, disabled, pre_release })
    }
    /// `None` when the archive has no hero definitions (for example a partial archive).
    pub fn read(archive: &deadlock_vpk::Vpk) -> Result<Option<Self>> {
        let Some(key) = ["scripts/heroes.vdata_c", "scripts/heroes.vdata"]
            .into_iter()
            .find(|k| archive.entries.contains_key(*k))
        else {
            return Ok(None);
        };
        let doc = crate::icon_catalog::definition(&crate::read(archive, key)?)?;
        Self::from_definition(&doc).map(Some)
    }
    pub fn target(&self, model: &str) -> Result<Option<GameTarget>> {
        let key = model_key(model);
        if let Some(heroes) = self.by_model.get(&key) {
            if heroes.iter().all(|h| self.disabled.contains(h)) {
                return Ok(Some(GameTarget::Disabled { heroes: heroes.clone() }));
            }
            let enabled: Vec<String> = heroes.iter().filter(|h| !self.disabled.contains(*h)).cloned().collect();
            if enabled.iter().all(|h| self.pre_release.contains(h)) {
                return Ok(Some(GameTarget::PreRelease { heroes: enabled }));
            }
            return Ok(Some(GameTarget::Active { heroes: enabled }));
        }
        // Only hero body models from the roster are compared; other models in a
        // hero folder (weapons, ability props) are not the hero's body.
        let roster_body = roster()?.iter().any(|hero| {
            casefold(&hero.path) == key || hero.extras.iter().any(|m| casefold(&m.path) == key)
        });
        if !roster_body {
            return Ok(None);
        }
        let owners = owner_keys(&key);
        let models: BTreeSet<&String> = self
            .by_model
            .keys()
            .filter(|m| !owner_keys(m).is_disjoint(&owners) && !owner_keys(m).contains("gen_man"))
            .collect();
        Ok(match models.len() {
            0 => None,
            1 => Some(GameTarget::Unused { model: models.into_iter().next().unwrap().clone() }),
            _ => Some(GameTarget::Ambiguous { models: models.into_iter().cloned().collect() }),
        })
    }
    /// An explanation when a project's target is not what the game loads.
    pub fn target_warning(&self, model: &str) -> Result<Option<String>> {
        let name = |path: &str| {
            roster().ok().and_then(|r| r.into_iter().find(|h| !owner_keys(&h.path).is_disjoint(&owner_keys(path))).map(|h| h.name))
                .unwrap_or_else(|| "this hero".into())
        };
        Ok(match self.target(model)? {
            Some(GameTarget::Unused { model: active }) => Some(format!(
                "The installed game loads {active} for {}. This project replaces {}, which the game does not load for that hero, so the original character would still be visible. Choose {} in Overview → Character, then prepare and build again.",
                name(&active), model_key(model), name(&active)
            )),
            Some(GameTarget::Ambiguous { models }) => Some(format!(
                "The installed game's definitions for {} load different models ({}). Check which one appears in game before building.",
                name(model), models.join(", ")
            )),
            _ => None,
        })
    }
}

#[cfg(test)]
mod game_model_tests {
    use super::*;
    use deadlock_resource::{Kind, Value};
    fn definitions(rows: &[(&str, &str)]) -> GameModels {
        let doc = Value::new(Kind::Object(rows.iter().map(|(k, m)| {
            (k.to_string(), Value::new(Kind::Object(vec![("m_strModelName".into(), Value::new(Kind::String(m.to_string())))])))
        }).collect()));
        GameModels::from_definition(&doc).unwrap()
    }
    #[test]
    fn pre_release_and_disabled_heroes_come_from_the_game_definitions() {
        // The October 16, 2026 game: Violet released, Nurse Harrow announced, Trapper switched off.
        let hero = |model: &str, state: Option<&str>, disabled: bool| {
            let mut fields = vec![("m_strModelName".to_string(), Value::new(Kind::String(model.to_string()))), ("m_bDisabled".to_string(), Value::new(Kind::Bool(disabled)))];
            if let Some(state) = state {
                fields.push(("m_eHeroDevelopmentState".to_string(), Value::new(Kind::String(state.to_string()))));
            }
            Value::new(Kind::Object(fields))
        };
        let doc = Value::new(Kind::Object(vec![
            ("hero_artist".to_string(), hero("models/heroes_wip/artist/artist.vmdl", Some("EHeroDevState_Release"), false)),
            ("hero_nurse".to_string(), hero("models/heroes_wip/nurse/nurse.vmdl", Some("EHeroDevState_PreRelease"), false)),
            ("hero_trapper".to_string(), hero("models/heroes_wip/gen_man/gen_man.vmdl", Some("EHeroDevState_Release"), true)),
            ("hero_haze".to_string(), hero("models/heroes_staging/haze/haze.vmdl", None, false)),
        ]));
        let game = GameModels::from_definition(&doc).unwrap();
        assert_eq!(game.target("models/heroes_wip/artist/artist.vmdl_c").unwrap(), Some(GameTarget::Active { heroes: vec!["hero_artist".into()] }));
        assert_eq!(game.target("models/heroes_wip/nurse/nurse.vmdl_c").unwrap(), Some(GameTarget::PreRelease { heroes: vec!["hero_nurse".into()] }));
        assert_eq!(game.target("models/heroes_wip/gen_man/gen_man.vmdl").unwrap(), Some(GameTarget::Disabled { heroes: vec!["hero_trapper".into()] }));
        // Older archives without the state field: released heroes stay active.
        assert_eq!(game.target("models/heroes_staging/haze/haze.vmdl").unwrap(), Some(GameTarget::Active { heroes: vec!["hero_haze".into()] }));
        assert_eq!(game.target_warning("models/heroes_wip/nurse/nurse.vmdl").unwrap(), None);
    }
    #[test]
    fn roster_models_the_game_does_not_load_are_reported() {
        let game = definitions(&[
            ("hero_familiar", "models/heroes_wip/familiar/familiar_wip.vmdl"),
            ("hero_atlas", "models/heroes_wip/abrams/abrams.vmdl"),
            ("hero_targetdummy", "models/heroes_wip/gen_man/gen_man.vmdl"),
        ]);
        assert_eq!(game.target("models/heroes_wip/familiar/familiar_wip.vmdl_c").unwrap(), Some(GameTarget::Active { heroes: vec!["hero_familiar".into()] }));
        assert_eq!(game.target("models/heroes_wip/familiar/familiar.vmdl").unwrap(), Some(GameTarget::Unused { model: "models/heroes_wip/familiar/familiar_wip.vmdl".into() }));
        let warning = game.target_warning("models/heroes_wip/familiar/familiar.vmdl").unwrap().unwrap();
        assert!(warning.contains("familiar_wip.vmdl for Rem"), "{warning}");
        assert_eq!(game.target_warning("models/heroes_wip/abrams/abrams.vmdl").unwrap(), None);
        // Other models in a hero folder are not hero bodies.
        assert_eq!(game.target("models/heroes_wip/familiar/familiar_bubble.vmdl").unwrap(), None);
    }
    #[test]
    fn every_roster_hero_matches_the_recorded_game_definitions() {
        let input: serde_json::Value = serde_json::from_str(include_str!("../tests/fixtures/icons/game-image-inventory.json")).unwrap();
        let mut game = GameModels::default();
        for (key, hero) in input["heroes"].as_object().unwrap() {
            if let Some(model) = hero["m_strModelName"].as_str() {
                game.by_model.entry(model_key(model)).or_default().push(key.clone());
            }
        }
        for hero in roster().unwrap() {
            assert!(matches!(game.target(&hero.path).unwrap(), Some(GameTarget::Active { .. })), "{} targets {} but the game loads another model", hero.name, hero.path);
        }
    }
}
/// The roster name of a hero body model (main or alternate), for labels only.
pub fn display_name(path: &str) -> Option<String> {
    let key = model_key(path);
    roster().ok()?.into_iter().find(|hero| {
        casefold(&hero.path) == key || hero.extras.iter().any(|m| casefold(&m.path) == key)
    }).map(|hero| hero.name)
}

#[cfg(test)]
mod availability_tests {
    use super::*;
    use deadlock_resource::{Kind, Value};
    #[test]
    fn heroes_in_development_and_unreleased_heroes_are_marked() {
        let hero = |model: &str, disabled: bool| {
            Value::new(Kind::Object(vec![
                ("m_strModelName".into(), Value::new(Kind::String(model.into()))),
                ("m_bDisabled".into(), Value::new(Kind::Bool(disabled))),
            ]))
        };
        let doc = Value::new(Kind::Object(vec![
            ("hero_chrono".into(), hero("models/heroes_staging/chrono/chrono.vmdl", false)),
            ("hero_swan".into(), hero("models/heroes_staging/chrono/chrono.vmdl", true)),
            ("hero_wrecker".into(), hero("models/heroes_staging/wrecker/wrecker.vmdl", true)),
        ]));
        let game = GameModels::from_definition(&doc).unwrap();
        // A model a playable hero loads stays active (disabled definitions sharing it are left out).
        assert_eq!(game.target("models/heroes_staging/chrono/chrono.vmdl").unwrap(), Some(GameTarget::Active { heroes: vec!["hero_chrono".into()] }));
        assert_eq!(game.target("models/heroes_staging/wrecker/wrecker.vmdl").unwrap(), Some(GameTarget::Disabled { heroes: vec!["hero_wrecker".into()] }));
        // The roster's fallback flag: Nurse Harrow until her release on October 20, 2026
        // (Violet was released on October 16). With game data the pre-release state decides.
        let roster = roster().unwrap();
        let unplayable: Vec<&str> = roster.iter().filter(|h| !h.playable).map(|h| h.name.as_str()).collect();
        assert_eq!(unplayable, ["Nurse Harrow"]);
    }
}
