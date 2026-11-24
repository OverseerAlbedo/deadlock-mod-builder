//! The saved form of renamed shop items (`Customization.item_names`): one row
//! per item, naming the item by its name token in the game's names file
//! (`upgrade_close_range`) and the name the shop shows instead. Which tokens a
//! rename changes, and the files the build writes, live in
//! `deadlock-components` (`item_names`); this is the plain data and its limits,
//! here so the project file can hold and check it.
//!
//! Loading never depends on the game: a row for an item the game no longer has
//! is refused by Analyze and the build (with help), never by load. Item names
//! are the same for every hero, like item icons.
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// The most renamed items a project holds.
pub const MAX_ITEM_NAMES: usize = 256;
/// The longest new name, in characters, after trimming.
pub const MAX_NAME_CHARS: usize = 32;
/// The longest item token.
pub const MAX_ITEM_TOKEN: usize = 128;
/// Characters a new name may not hold: the names file format reads `"` and `\`
/// as its own (end of the text, an escape), and the game's text display reads
/// `<` `>` as markup and `{` `}` `%` as format codes.
pub const REFUSED_CHARS: [char; 7] = ['"', '\\', '<', '>', '{', '}', '%'];

pub const NAME_EMPTY: &str = "Type a name for the item.";
pub const NAME_TOO_LONG: &str = "An item name is at most 32 characters.";
pub const NAME_ONE_LINE: &str = "An item name is one line without control characters.";
pub const NAME_REFUSED_CHAR: &str =
    "An item name cannot hold \" \\ < > { } or %: the game's names file and text display read them as their own codes.";
pub const NAME_LEADING_HASH: &str = "An item name cannot start with #: the game's names file reads that as a reference to another text.";

fn yes() -> bool {
    true
}
/// One renamed item.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemNameRow {
    /// The item's name token in the game's names file (`upgrade_close_range`).
    pub item: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    /// The name the shop shows; trimmed by `clean_name` when saved.
    pub name: String,
}
fn error(text: impl Into<String>) -> Error {
    Error::Validation(text.into())
}
/// The name as it is written: trimmed, then checked against the limits. The
/// same rules (and messages) as `src/extras/itemNames.ts cleanName`.
pub fn clean_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(error(NAME_EMPTY));
    }
    if name.chars().any(char::is_control) {
        return Err(error(NAME_ONE_LINE));
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(error(NAME_TOO_LONG));
    }
    if name.contains(REFUSED_CHARS) {
        return Err(error(NAME_REFUSED_CHAR));
    }
    if name.starts_with('#') {
        return Err(error(NAME_LEADING_HASH));
    }
    Ok(name.to_owned())
}
/// An item token: a plain key of the names file (letters, digits, `_`; no
/// header key, no `_search` token, no `/` buff path).
pub fn valid_item(item: &str) -> bool {
    !item.is_empty() && item.len() <= MAX_ITEM_TOKEN && item.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}
/// Rows: at most `MAX_ITEM_NAMES`, one per item (case-blind), each with a
/// valid item token and a name `clean_name` accepts as saved (already trimmed).
pub fn validate_rows(rows: &[ItemNameRow]) -> Result<()> {
    if rows.len() > MAX_ITEM_NAMES {
        return Err(error(format!("Item names support at most {MAX_ITEM_NAMES} renamed items.")));
    }
    let mut seen = BTreeSet::new();
    for row in rows {
        if !valid_item(&row.item) {
            return Err(error(format!("Invalid item in a renamed item row: {:?}", row.item)));
        }
        if !seen.insert(row.item.to_ascii_lowercase()) {
            return Err(error(format!("Two renamed item rows name the same item: {}", row.item)));
        }
        if clean_name(&row.name)? != row.name {
            return Err(error(NAME_EMPTY_OR_UNTRIMMED));
        }
    }
    Ok(())
}
/// A saved name with spaces around it (the page and `clean_name` trim them).
pub const NAME_EMPTY_OR_UNTRIMMED: &str = "An item name has no spaces at its start or end.";

#[cfg(test)]
mod tests {
    use super::*;
    fn row(item: &str, name: &str) -> ItemNameRow {
        ItemNameRow { item: item.into(), enabled: true, name: name.into() }
    }
    #[test]
    fn rows_round_trip_and_keep_their_limits() {
        let rows: Vec<ItemNameRow> = serde_json::from_value(serde_json::json!([
            {"item": "upgrade_sample_one", "name": "New Name"},
            {"item": "upgrade_sample_two", "enabled": false, "name": "Zweiter Name"}
        ]))
        .unwrap();
        assert!(rows[0].enabled, "enabled by default");
        validate_rows(&rows).unwrap();
        assert_eq!(serde_json::from_value::<Vec<ItemNameRow>>(serde_json::to_value(&rows).unwrap()).unwrap(), rows);
        for bad in [
            serde_json::json!({"item": "upgrade_a", "name": "A", "colour": 1}),
            serde_json::json!({"item": "upgrade_a"}),
            serde_json::json!({"name": "A"}),
        ] {
            assert!(serde_json::from_value::<ItemNameRow>(bad.clone()).is_err(), "{bad}");
        }
        assert!(validate_rows(&[row("upgrade_a", "A"), row("UPGRADE_A", "B")]).is_err(), "one row per item, case-blind");
        for item in ["", "upgrade_a_search/x", "upgrade a", "upgrade_a\0", &"u".repeat(MAX_ITEM_TOKEN + 1)] {
            assert!(validate_rows(&[row(item, "A")]).is_err(), "{item:?}");
        }
        let many: Vec<_> = (0..=MAX_ITEM_NAMES).map(|i| row(&format!("upgrade_{i}"), "A")).collect();
        assert!(validate_rows(&many).is_err());
        assert!(validate_rows(&many[..MAX_ITEM_NAMES]).is_ok());
        assert_eq!(validate_rows(&[row("upgrade_a", " A")]).unwrap_err().to_string().contains(NAME_EMPTY_OR_UNTRIMMED), true);
    }
    #[test]
    fn names_are_one_trimmed_line_without_the_format_s_own_characters() {
        assert_eq!(clean_name("  Close Call  ").unwrap(), "Close Call");
        assert_eq!(clean_name(&"é".repeat(MAX_NAME_CHARS)).unwrap().chars().count(), MAX_NAME_CHARS, "characters, not bytes");
        assert_eq!(clean_name("回復の儀式").unwrap(), "回復の儀式");
        assert_eq!(clean_name("Mid # sign").unwrap(), "Mid # sign");
        let message = |name: &str| clean_name(name).unwrap_err().to_string();
        assert!(message("   ").contains(NAME_EMPTY));
        assert!(message(&"a".repeat(MAX_NAME_CHARS + 1)).contains(NAME_TOO_LONG));
        for name in ["two\nlines", "tab\there", "bell\u{7}"] {
            assert!(message(name).contains(NAME_ONE_LINE), "{name:?}");
        }
        for c in REFUSED_CHARS {
            assert!(message(&format!("a{c}b")).contains(NAME_REFUSED_CHAR), "{c}");
        }
        assert!(message("#upgrade_a").contains(NAME_LEADING_HASH));
        assert!(message("  #a").contains(NAME_LEADING_HASH), "after trimming");
    }
}
