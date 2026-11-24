//! Item icons (`Customization.item_icons`, Extras → Items): replacements of the
//! game's item pictures. They are the same for every hero, so changing the
//! project's hero keeps them (`clear_targets` leaves them alone). Whether a
//! target really is one of the game's item pictures is checked against the
//! game's item definitions at build (`deadlock-components item_icons`).
use crate::{asset_paths::relative_asset, customization::ResourceReplacement, Error, Result};
use std::collections::BTreeSet;

/// The most item icon replacements one project holds (every picture of the
/// game's item catalog fits).
pub const MAX_ITEM_ICONS: usize = 512;

fn error(text: impl Into<String>) -> Error {
    Error::Validation(text.into())
}

/// The saved rows: at most `MAX_ITEM_ICONS`, Panorama textures, one enabled row
/// per picture, a known fit, and no picture also replaced in Icons.
pub fn validate(rows: &[ResourceReplacement], icons: &[ResourceReplacement]) -> Result<()> {
    if rows.len() > MAX_ITEM_ICONS {
        return Err(error(format!("Extras → Items holds at most {MAX_ITEM_ICONS} item icon replacements.")));
    }
    let lower = deadlock_core::text::casefold;
    let icon_targets: BTreeSet<String> = icons
        .iter()
        .filter(|r| r.enabled.unwrap_or(true))
        .filter_map(|r| r.target.as_deref())
        .map(lower)
        .collect();
    let mut seen = BTreeSet::new();
    for row in rows {
        let target = row.target.as_deref().unwrap_or_default();
        if row.silence == Some(true) {
            return Err(error("Only sounds can be silenced; item_icons rows need a file."));
        }
        row.check_sound_edit("item_icons")?;
        row.check_made("item_icons")?;
        row.check_taken("item_icons")?;
        for value in [target, row.source.as_deref().unwrap_or_default()] {
            if value.contains('\0') {
                return Err(error("Invalid item icon path."));
            }
        }
        if !target.is_empty() {
            relative_asset(target)?;
            let key = lower(target);
            if !key.starts_with("panorama/images/") || !key.ends_with(".vtex_c") {
                return Err(error("An item icon's destination must be one of the game's item pictures (.vtex_c)."));
            }
            if row.enabled.unwrap_or(true) {
                if !seen.insert(key.clone()) {
                    return Err(error(format!("Two enabled item icon replacements use the same picture: {target}")));
                }
                if icon_targets.contains(&key) {
                    return Err(error(format!("This picture is replaced both in Extras → Icons and in Extras → Items: {target}. Remove one of them.")));
                }
            }
        }
        if row.fit.as_ref().is_some_and(|v| !["contain", "cover", "stretch"].contains(&v.as_str())) {
            return Err(error("Icon fit must be contain, cover or stretch."));
        }
        if let Some(hash) = row.original_sha256.as_ref().filter(|h| !h.is_empty()) {
            if hash.len() != 64 || !hash.bytes().all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v)) {
                return Err(error("Invalid original character asset fingerprint."));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::customization::Customization;
    const TARGET: &str = "panorama/images/items/weapon/basic_magazine_psd.vtex_c";
    fn check(value: serde_json::Value) -> Result<Customization, String> {
        let choice: Customization = serde_json::from_value(value).map_err(|e| e.to_string())?;
        choice.validate().map_err(|e| e.to_string())?;
        Ok(choice)
    }
    #[test]
    fn item_icon_rows_are_their_own_list() {
        let saved = check(serde_json::json!({"enabled": true, "item_icons": [{"enabled": true, "source": "C:/art/mag.png", "target": TARGET, "fit": "cover"}]})).unwrap();
        assert_eq!(saved.item_icons.as_ref().unwrap().len(), 1);
        assert!(saved.icons.is_none(), "never an Icons row");
        // Old projects load and save unchanged: no field, nothing written.
        let old = serde_json::json!({"enabled": true, "icons": [{"target": "panorama/images/heroes/haze_card_psd.vtex_c", "source": "a.png"}]});
        let loaded = check(old.clone()).unwrap();
        assert_eq!(serde_json::to_value(&loaded).unwrap(), old);
        assert!(check(serde_json::json!({"item_icons": [{"target": "models/x.vmat_c"}]})).unwrap_err().contains("item pictures"));
        assert!(check(serde_json::json!({"item_icons": [{"target": TARGET}, {"target": TARGET.to_uppercase()}]})).unwrap_err().contains("same picture"));
        assert!(check(serde_json::json!({"item_icons": [{"target": TARGET}, {"target": TARGET, "enabled": false}]})).is_ok());
        assert!(check(serde_json::json!({"item_icons": [{"target": TARGET, "fit": "tile"}]})).unwrap_err().contains("fit"));
        assert!(check(serde_json::json!({"item_icons": [{"target": "../x.vtex_c"}]})).is_err());
        assert!(check(serde_json::json!({"icons": [{"target": TARGET}], "item_icons": [{"target": TARGET}]})).unwrap_err().contains("both in Extras → Icons and in Extras → Items"));
        let many: Vec<_> = (0..=super::MAX_ITEM_ICONS).map(|i| serde_json::json!({"target": format!("panorama/images/items/x{i}_psd.vtex_c")})).collect();
        assert!(check(serde_json::json!({"item_icons": many})).unwrap_err().contains("at most 512"));
        // More than the 128 rows of a hero category are fine.
        let rows: Vec<_> = (0..200).map(|i| serde_json::json!({"target": format!("panorama/images/items/x{i}_psd.vtex_c")})).collect();
        assert!(check(serde_json::json!({"item_icons": rows})).is_ok());
    }
    #[test]
    fn item_icons_stay_when_the_hero_changes_and_move_with_the_project() {
        let mut saved = check(serde_json::json!({"item_icons": [{"source": "art/mag.png", "target": TARGET, "original_sha256": "a".repeat(64)}]})).unwrap();
        saved.clear_targets();
        let row = &saved.item_icons.as_ref().unwrap()[0];
        assert_eq!(row.target.as_deref(), Some(TARGET), "item icons are the same for every hero");
        assert!(row.original_sha256.is_some());
        saved.map_sources(|s| Ok(format!("moved/{s}"))).unwrap();
        assert_eq!(saved.item_icons.unwrap()[0].source.as_deref(), Some("moved/art/mag.png"));
    }
}
