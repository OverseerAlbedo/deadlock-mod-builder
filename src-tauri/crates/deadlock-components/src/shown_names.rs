//! Hero and item names in an interface language, as the game itself writes
//! them. Display only: every identity (roster hero names, item tokens, saved
//! rows) stays English, and nothing here is part of a build, a receipt or a
//! catalog.
//!
//! The game keeps its names as loose text files beside its archive, one per
//! language (the game's own word: `english`, `schinese`, …):
//! `resource/localization/citadel_gc_hero_names/citadel_gc_hero_names_<language>.txt`
//! and `resource/localization/citadel_gc_mod_names/citadel_gc_mod_names_<language>.txt`.
//! Both are read with `item_names::pairs` (tokens block only, the first of a
//! repeated key wins, escapes resolved).
//!
//! * Heroes: a hero's name token is `<hero definition key>:n`. A roster hero is
//!   joined to its definition key through the game's hero definitions (the
//!   definitions that load the roster model, `heroes::GameModels::target`).
//!   Several keys for one model are narrowed to the one whose English text is
//!   the roster name, or accepted when they all give the same text. Without
//!   definitions (or when they name no key), a hero name token whose English
//!   text is exactly the roster name, and the only one with that text, is used.
//! * Items: an item's name token (`item_names::is_item_token`), keyed casefolded
//!   as `item_names::game_names` keys it.
//! * An entry is left out when its text in the language is empty, a `#`
//!   placeholder, or equal to its English text, or when the English file lacks
//!   the token.
//! * Nothing here fails: a missing folder, a stub or unreadable file, or an
//!   interface language without a game language gives empty maps.
use crate::heroes::{GameModels, GameTarget, Hero};
use crate::item_names::{self, MAX_FILE_BYTES};
use deadlock_core::text::casefold;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// The hero names folder, under `resource/localization/` beside the archive.
pub const HERO_FOLDER: &str = "citadel_gc_hero_names";
/// A hero names file is `<HERO_PREFIX><language>.txt`.
pub const HERO_PREFIX: &str = "citadel_gc_hero_names_";
/// The game's word for English.
pub const ENGLISH: &str = item_names::ENGLISH;

/// Interface language id → the game's word for that language. One line per
/// language; later ones would be, for example, `("ru", "russian")`,
/// `("es", "latam")`, `("pt-BR", "brazilian")`, `("ja", "japanese")`.
pub const LANGUAGES: &[(&str, &str)] = &[("zh-CN", "schinese")];

/// The game's word for an interface language id (ASCII case ignored), `None`
/// for English, an empty id or an id without a game language.
pub fn game_language(id: &str) -> Option<&'static str> {
    LANGUAGES.iter().find(|(known, _)| known.eq_ignore_ascii_case(id.trim())).map(|(_, word)| *word)
}

/// The names shown for one interface language.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ShownNames {
    /// The interface language id the answer is for.
    pub language: String,
    /// Roster hero name (English, as the app shows it) → the game's name in the language.
    pub heroes: BTreeMap<String, String>,
    /// Casefolded item token → the game's item name in the language.
    pub items: BTreeMap<String, String>,
    /// Roster heroes without a name in the language (sorted).
    pub missing: Vec<String>,
}

fn localization(archive: &Path) -> PathBuf {
    archive.parent().unwrap_or(Path::new("")).join("resource").join("localization")
}
/// A hero names file beside an archive.
pub fn hero_file(archive: &Path, language: &str) -> PathBuf {
    localization(archive).join(HERO_FOLDER).join(format!("{HERO_PREFIX}{language}.txt"))
}
/// An item names file beside an archive.
pub fn item_file(archive: &Path, language: &str) -> PathBuf {
    item_names::folder(archive).join(format!("{}{language}.txt", item_names::PREFIX))
}
/// The four files an answer for `language` reads (hero English, hero language,
/// item English, item language), for keeping an answer while they are the same.
pub fn files(archive: &Path, language: &str) -> [PathBuf; 4] {
    [hero_file(archive, ENGLISH), hero_file(archive, language), item_file(archive, ENGLISH), item_file(archive, language)]
}

/// The tokens of a names file (casefolded key → text, escapes resolved);
/// empty when the file is missing, linked, too large or not UTF-8.
pub fn tokens(path: &Path) -> BTreeMap<String, String> {
    let readable = deadlock_vpk::check_unlinked_path(path, false).is_ok()
        && std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.len() <= MAX_FILE_BYTES);
    let Some(text) = readable.then(|| std::fs::read(path).ok()).flatten().and_then(|raw| String::from_utf8(raw).ok()) else {
        return BTreeMap::new();
    };
    let mut out = BTreeMap::new();
    for pair in item_names::pairs(&text).into_iter().filter(|p| p.in_tokens) {
        out.entry(casefold(&pair.key)).or_insert_with(|| item_names::unescape(&text[pair.value]));
    }
    out
}

/// A hero's name token (casefolded): `<definition key>:n`.
fn hero_token(key: &str) -> String {
    format!("{}:n", casefold(key))
}
/// Whether a casefolded token is a hero's name token (not its search or sort words).
fn is_hero_token(key: &str) -> bool {
    key.strip_suffix(":n").is_some_and(|stem| stem.starts_with("hero_") && !stem.ends_with("_search") && !stem.ends_with("_sort") && !stem.contains('/'))
}
/// The shown text of a token in the language, when it is a real name.
fn shown(english: &BTreeMap<String, String>, language: &BTreeMap<String, String>, key: &str) -> Option<String> {
    let english = english.get(key)?;
    let text = language.get(key)?;
    let trimmed = text.trim();
    (!trimmed.is_empty() && !trimmed.starts_with('#') && text != english).then(|| text.clone())
}

/// The name token joined to a roster hero (see the module notes), or `None`.
pub fn hero_join(hero: &Hero, english: &BTreeMap<String, String>, definitions: Option<&GameModels>) -> Option<String> {
    if let Some(definitions) = definitions {
        let keys = match definitions.target(&hero.path).ok().flatten() {
            Some(GameTarget::Active { heroes } | GameTarget::PreRelease { heroes } | GameTarget::Disabled { heroes }) => heroes,
            _ => vec![],
        };
        let tokens: BTreeSet<String> = keys.iter().map(|k| hero_token(k)).filter(|t| english.contains_key(t)).collect();
        if tokens.len() == 1 {
            return tokens.into_iter().next();
        }
        let named: Vec<&String> = tokens.iter().filter(|t| english.get(*t).is_some_and(|text| *text == hero.name)).collect();
        if named.len() == 1 {
            return Some(named[0].clone());
        }
        if !tokens.is_empty() {
            // Several definitions with different English names: only when they
            // all read the same is the first one taken.
            let texts: BTreeSet<&String> = tokens.iter().filter_map(|t| english.get(t)).collect();
            return (texts.len() == 1).then(|| tokens.into_iter().next()).flatten();
        }
    }
    let same: Vec<&String> = english.iter().filter(|(key, text)| is_hero_token(key) && **text == hero.name).map(|(key, _)| key).collect();
    (same.len() == 1).then(|| same[0].clone())
}

/// The names of one interface language beside `archive`. `definitions` is asked
/// for the game's hero definitions only when the language's hero file has tokens.
pub fn read(archive: &Path, id: &str, roster: &[Hero], definitions: impl FnOnce() -> Option<GameModels>) -> ShownNames {
    let mut out = ShownNames { language: id.to_owned(), ..Default::default() };
    let Some(language) = game_language(id) else { return out };
    let [hero_english, hero_language, item_english, item_language] = files(archive, language);
    let language_heroes = tokens(&hero_language);
    if !language_heroes.is_empty() {
        let english = tokens(&hero_english);
        if !english.is_empty() {
            let definitions = definitions();
            for hero in roster {
                if let Some(text) = hero_join(hero, &english, definitions.as_ref()).and_then(|token| shown(&english, &language_heroes, &token)) {
                    out.heroes.insert(hero.name.clone(), text);
                }
            }
        }
    }
    let language_items = tokens(&item_language);
    if !language_items.is_empty() {
        let english = tokens(&item_english);
        for key in english.keys().filter(|k| item_names::is_item_token(k)) {
            if let Some(text) = shown(&english, &language_items, key) {
                out.items.insert(key.clone(), text);
            }
        }
    }
    let mut missing: Vec<String> = roster.iter().filter(|h| !out.heroes.contains_key(&h.name)).map(|h| h.name.clone()).collect();
    missing.sort();
    missing.dedup();
    out.missing = missing;
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadlock_resource::{Kind, Value};

    fn hero(name: &str, path: &str) -> Hero {
        serde_json::from_value(serde_json::json!({"name": name, "path": path})).unwrap()
    }
    fn definitions(rows: &[(&str, &str)]) -> GameModels {
        let doc = Value::new(Kind::Object(
            rows.iter().map(|(k, m)| (k.to_string(), Value::new(Kind::Object(vec![("m_strModelName".into(), Value::new(Kind::String(m.to_string())))])))).collect(),
        ));
        GameModels::from_definition(&doc).unwrap()
    }
    /// A names file in the game's layout: byte-order mark, CRLF, a header key,
    /// a comment and a tokens block.
    fn names(language: &str, rows: &[(&str, &str)]) -> String {
        let mut text = format!("\u{feff}\"lang\"\r\n{{\r\n\t\"Language\"\t\t\"{language}\"\r\n\t\"Tokens\"\r\n\t{{\r\n\t\t// Names\r\n");
        for (key, value) in rows {
            text.push_str(&format!("\t\t\"{key}\" \"{value}\"\r\n"));
        }
        text.push_str("\t}\r\n}\r\n");
        text
    }
    fn write(archive: &Path, folder: &str, prefix: &str, language: &str, text: &str) {
        let dir = localization(archive).join(folder);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{prefix}{language}.txt")), text).unwrap();
    }
    fn heroes_file(archive: &Path, language: &str, rows: &[(&str, &str)]) {
        write(archive, HERO_FOLDER, HERO_PREFIX, language, &names(language, rows));
    }
    fn items_file(archive: &Path, language: &str, rows: &[(&str, &str)]) {
        write(archive, item_names::FOLDER, item_names::PREFIX, language, &names(language, rows));
    }
    fn roster() -> Vec<Hero> {
        vec![
            hero("Sample One", "models/heroes_wip/sample_one/sample_one.vmdl"),
            hero("Sample Two", "models/heroes_wip/sample_two/sample_two.vmdl"),
            hero("Sample Three", "models/heroes_wip/sample_three/sample_three.vmdl"),
            hero("Sample Four", "models/heroes_wip/sample_four/sample_four.vmdl"),
        ]
    }
    fn sample_game(archive: &Path) {
        heroes_file(
            archive,
            "english",
            &[
                ("hero_alpha:n", "Sample One"),
                ("hero_alpha_search:n", "Sample One"),
                ("hero_beta:n", "Old Name"),
                ("hero_gamma:n", "Sample Three"),
                ("hero_delta:n", "Sample Four"),
                ("Steam_RP_hero_alpha", "Sample One"),
            ],
        );
        heroes_file(
            archive,
            "schinese",
            &[
                ("hero_alpha:n", "样本一"),
                ("hero_alpha_search:n", "样本一 yangben"),
                ("hero_beta:n", "样本二"),
                ("hero_gamma:n", "Sample Three"),
                ("hero_delta:n", ""),
                ("hero_only_here:n", "只在这里"),
            ],
        );
        items_file(
            archive,
            "english",
            &[
                ("upgrade_sample_clip", "Sample Clip"),
                ("upgrade_sample_clip_search", "Sample Clip SC"),
                ("upgrade_english_only", "English Only"),
                ("upgrade_same", "Same Name"),
                ("upgrade_quoted", "The \\\"Quoted\\\" One"),
                ("upgrade_sample_clip/modifier_sample", "Sample Clip"),
            ],
        );
        items_file(
            archive,
            "schinese",
            &[
                ("Upgrade_Sample_Clip", "样本弹匣"),
                ("upgrade_sample_clip_search", "样本弹匣 yangben"),
                ("upgrade_same", "Same Name"),
                ("upgrade_quoted", "带\\\"引号\\\"的"),
                ("upgrade_sample_clip/modifier_sample", "样本弹匣"),
                ("upgrade_language_only", "只有中文"),
            ],
        );
    }
    fn sample_definitions() -> GameModels {
        definitions(&[
            ("hero_alpha", "models/heroes_wip/sample_one/sample_one.vmdl"),
            ("hero_beta", "models/heroes_wip/sample_two/sample_two.vmdl"),
            ("hero_gamma", "models/heroes_wip/sample_three/sample_three.vmdl"),
            ("hero_delta", "models/heroes_wip/sample_four/sample_four.vmdl"),
        ])
    }

    #[test]
    fn interface_languages_map_to_the_game_s_words() {
        assert_eq!(game_language("zh-CN"), Some("schinese"));
        assert_eq!(game_language("zh-cn"), Some("schinese"));
        for id in ["en", "", "xx", "english", "schinese"] {
            assert_eq!(game_language(id), None, "{id}");
        }
    }

    #[test]
    fn heroes_and_items_get_the_language_s_names() {
        let temp = tempfile::tempdir().unwrap();
        let archive = temp.path().join("pak01_dir.vpk");
        sample_game(&archive);
        let answer = read(&archive, "zh-CN", &roster(), || Some(sample_definitions()));
        assert_eq!(answer.language, "zh-CN");
        // Joined through the definitions, also where the English texts differ.
        assert_eq!(answer.heroes.get("Sample One").map(String::as_str), Some("样本一"));
        assert_eq!(answer.heroes.get("Sample Two").map(String::as_str), Some("样本二"));
        // Equal to English, or empty: left out.
        assert!(!answer.heroes.contains_key("Sample Three"));
        assert!(!answer.heroes.contains_key("Sample Four"));
        assert_eq!(answer.heroes.len(), 2);
        assert_eq!(answer.missing, ["Sample Four", "Sample Three"]);
        // Items: the name token only, keyed casefolded; the escaped quote resolved.
        assert_eq!(answer.items.get("upgrade_sample_clip").map(String::as_str), Some("样本弹匣"));
        assert_eq!(answer.items.get("upgrade_quoted").map(String::as_str), Some("带\"引号\"的"));
        assert_eq!(answer.items.len(), 2, "{:?}", answer.items);
    }

    #[test]
    fn without_definitions_only_an_exact_unique_english_name_joins() {
        let temp = tempfile::tempdir().unwrap();
        let archive = temp.path().join("pak01_dir.vpk");
        sample_game(&archive);
        let answer = read(&archive, "zh-CN", &roster(), || None);
        assert_eq!(answer.heroes.get("Sample One").map(String::as_str), Some("样本一"));
        assert!(!answer.heroes.contains_key("Sample Two"), "its English text differs from the roster name");
        // Two tokens with the roster name: no guess.
        heroes_file(&archive, "english", &[("hero_alpha:n", "Sample One"), ("hero_alpha2:n", "Sample One")]);
        heroes_file(&archive, "schinese", &[("hero_alpha:n", "样本一"), ("hero_alpha2:n", "样本一b")]);
        let answer = read(&archive, "zh-CN", &roster(), || None);
        assert!(answer.heroes.is_empty(), "{:?}", answer.heroes);
    }

    #[test]
    fn several_definitions_for_one_model_narrow_to_the_roster_name() {
        let temp = tempfile::tempdir().unwrap();
        let archive = temp.path().join("pak01_dir.vpk");
        heroes_file(&archive, "english", &[("hero_alpha:n", "Sample One"), ("hero_alpha_old:n", "Older One")]);
        heroes_file(&archive, "schinese", &[("hero_alpha:n", "样本一"), ("hero_alpha_old:n", "旧的")]);
        let both = definitions(&[("hero_alpha", "models/heroes_wip/sample_one/sample_one.vmdl"), ("hero_alpha_old", "models/heroes_wip/sample_one/sample_one.vmdl")]);
        let answer = read(&archive, "zh-CN", &roster()[..1], || Some(both));
        assert_eq!(answer.heroes.get("Sample One").map(String::as_str), Some("样本一"));
    }

    #[test]
    fn stubs_missing_folders_and_unknown_languages_give_empty_maps() {
        let temp = tempfile::tempdir().unwrap();
        let archive = temp.path().join("pak01_dir.vpk");
        let mut asked = false;
        // No folder at all.
        let answer = read(&archive, "zh-CN", &roster(), || {
            asked = true;
            None
        });
        assert!(answer.heroes.is_empty() && answer.items.is_empty());
        assert_eq!(answer.missing.len(), 4);
        assert!(!asked, "the definitions are not read without the language's names");
        // Stub language files (no tokens).
        sample_game(&archive);
        heroes_file(&archive, "schinese", &[]);
        items_file(&archive, "schinese", &[]);
        let answer = read(&archive, "zh-CN", &roster(), || panic!("not needed for a stub"));
        assert!(answer.heroes.is_empty() && answer.items.is_empty());
        // Not UTF-8: read as empty, never an error.
        std::fs::write(hero_file(&archive, "schinese"), [0xff, 0xfe, 0x00, 0x22]).unwrap();
        assert!(read(&archive, "zh-CN", &roster(), || None).heroes.is_empty());
        // English, an empty id and an unknown id: empty, nothing read.
        for id in ["en", "", "xx-YY"] {
            let answer = read(&archive, id, &roster(), || panic!("nothing to read"));
            assert_eq!(answer, ShownNames { language: id.into(), ..Default::default() });
        }
    }

    #[test]
    fn hero_name_tokens_leave_out_search_sort_and_presence_words() {
        assert!(is_hero_token("hero_alpha:n"));
        for key in ["hero_alpha_search:n", "hero_alpha_sort:n", "steam_rp_hero_alpha", "hero_alpha", "upgrade_alpha:n"] {
            assert!(!is_hero_token(key), "{key}");
        }
    }
}
