//! Extras → Items: the pictures of the game's items, from the game's own item
//! definitions, for every hero alike.
//!
//! * Items are the `scripts/abilities.vdata` records with `m_eAbilityType`
//!   `EAbilityType_Item`. The slot (`m_eItemSlotType`: `WeaponMod`, `Armor`,
//!   `Tech`) gives the shop category (Weapon, Vitality, Spirit), `m_iItemTier`
//!   (`EModTier_1` … `EModTier_5`) the tier and `m_strShopIconLarge` the picture
//!   the shop shows (`panorama/images/items/…`).
//! * Prices come from `scripts/generic_data.vdata` `m_nItemPricePerTier`, the
//!   game's price per tier (index = tier); items carry no price of their own.
//! * The shop sells an item when it has a slot, a tier from 1 to 4 and a
//!   picture, and is neither `m_bDisabled` (a boolean or the number 1),
//!   `m_bInDevelopment`, a base record (`_not_pickable`) nor a Street Brawl
//!   item.
//! * Legendary items are the records the game marks for Street Brawl
//!   (`m_eAbilityRequirements` `ERequirementStreetBrawl`, all tier 5). Those
//!   the game switches off (`m_bDisabled` or `m_bInDevelopment`) are the
//!   Legendary group `off`, shown greyed out after the others; they stay
//!   replaceable. Their tier's entry in the price table is a placeholder
//!   (9,999 souls): Street Brawl items are drafted, not bought, so no price is
//!   shown for them.
//! * Misc holds the item pictures in `panorama/images/items/` that are neither
//!   a shop item nor Legendary: pictures of other switched-off items, seasonal
//!   pictures, and pictures no item names. The older item glyphs
//!   (`panorama/images/upgrades/…`, still named as `m_strAbilityImage`) are not
//!   offered.
//! * One row per picture file: a picture several records name belongs to the
//!   sold item first, then a Legendary item, then the rest; every record that
//!   names it is listed.
//! * Names come from the game's English localisation beside the archive
//!   (`resource/localization/citadel_gc_mod_names/…_english.txt`); without it
//!   the internal names are made readable. The game's data gives no order
//!   inside a tier, so rows are sorted by name.
use crate::{CancellationToken, Result, fail, icon_catalog, icons, read, sha};
use deadlock_core::text::casefold;
use deadlock_resource::{Kind, Value};
use deadlock_vpk::Vpk;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Where the shop's item pictures are.
pub const ITEM_FOLDER: &str = "panorama/images/items/";
/// The localisation file with the item names (under `resource/localization/`).
pub const NAME_FILE: &str = "citadel_gc_mod_names/citadel_gc_mod_names_english.txt";
/// Messages of the item icon checks (matched by the app's error help).
pub const NOT_AN_ITEM_ICON: &str = "This picture is not one of the game's item icons now";
pub const ITEM_ICON_CHANGED: &str = "The game's item icon changed after it was chosen. Open Extras → Items, click Reload assets and choose it again.";
pub const ITEM_ICONS_UNREADABLE: &str = "The game's item list could not be read";
pub const EMPTY_ROW: &str = "Add a picture for each item icon you replace in Extras → Items, or remove its empty row.";

/// A category of the Items page, in page order.
pub const CATEGORIES: [&str; 5] = ["weapon", "vitality", "spirit", "legendary", "misc"];
/// Misc groups, in page order.
pub const MISC_GROUPS: [(&str, &str); 3] = [
    ("unsold", "Items not in the shop now"),
    ("seasonal", "Seasonal pictures"),
    ("unused", "Pictures no item uses"),
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ItemIcon {
    /// The compiled picture in the archive (its own spelling).
    pub target: String,
    pub path: String,
    /// The item's name (or the picture's, for pictures without an item).
    pub label: String,
    /// `weapon`, `vitality`, `spirit`, `legendary` or `misc`.
    pub category: String,
    /// The tier (`1` … `5`) for shop and Legendary rows, `off` for Legendary
    /// items the game switches off; a `MISC_GROUPS` key for Misc.
    pub group: String,
    pub tier: Option<u8>,
    /// Souls, from the game's price per tier (shop rows only).
    pub price: Option<u32>,
    /// The item records whose picture this is (internal names), owner first.
    pub items: Vec<String>,
    /// Their names, in the same order.
    pub names: Vec<String>,
    /// For Misc rows: the category and tier the item had, when it has one.
    pub slot: Option<String>,
    pub supported: bool,
    pub reason: String,
    pub width: u16,
    pub height: u16,
    pub original_sha256: Option<String>,
    pub method: Option<String>,
    /// Item pictures are the same for every hero; there is no shared-resource consent.
    pub shared: bool,
    pub association: String,
    /// The game switches this item off (`m_bDisabled` or `m_bInDevelopment`):
    /// it is not in the game now. Legendary `off` and Misc `unsold` rows.
    #[serde(default)]
    pub switched_off: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ItemCatalog {
    pub items: Vec<ItemIcon>,
    /// The game's price per tier (index = tier), as read.
    pub prices: Vec<u32>,
    /// Whether the game's English item names were found.
    pub names_found: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Default)]
struct Definition {
    key: String,
    slot: Option<&'static str>,
    tier: Option<u8>,
    off: bool,
    brawl: bool,
    shop_icon: Option<String>,
}
impl Definition {
    fn sold(&self) -> bool {
        !self.off && !self.brawl && self.slot.is_some() && self.tier.is_some_and(|t| (1..=4).contains(&t)) && self.shop_icon.is_some()
    }
    fn legendary(&self) -> bool {
        !self.off && self.brawl && self.shop_icon.is_some()
    }
    /// A Street Brawl item the game switches off.
    fn legendary_off(&self) -> bool {
        self.off && self.brawl && self.shop_icon.is_some()
    }
}

fn truthy(value: Option<&Value>) -> bool {
    match value.map(|v| &v.kind) {
        Some(Kind::Bool(b)) => *b,
        Some(Kind::I64(i)) => *i != 0,
        Some(Kind::U64(i)) => *i != 0,
        Some(Kind::String(s)) => s == "1" || s.eq_ignore_ascii_case("true"),
        _ => false,
    }
}
fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(|v| v.string().ok()).filter(|s| !s.is_empty())
}
/// An item picture the definitions name (casefolded compiled path).
fn picture(value: Option<&str>) -> Option<String> {
    let path = icon_catalog::image_path(value?)?;
    (path.starts_with(ITEM_FOLDER) && path.ends_with(".vtex_c")).then_some(path)
}

fn definitions(abilities: &Value) -> Result<Vec<Definition>> {
    let mut out = vec![];
    for (key, value) in abilities.object().map_err(fail)? {
        if text(value, "m_eAbilityType") != Some("EAbilityType_Item") || value.get("_not_pickable").is_some() {
            continue;
        }
        out.push(Definition {
            key: key.clone(),
            slot: match text(value, "m_eItemSlotType") {
                Some("EItemSlotType_WeaponMod") => Some("weapon"),
                Some("EItemSlotType_Armor") => Some("vitality"),
                Some("EItemSlotType_Tech") => Some("spirit"),
                _ => None,
            },
            tier: text(value, "m_iItemTier").and_then(|t| t.strip_prefix("EModTier_")).and_then(|t| t.parse().ok()),
            off: truthy(value.get("m_bDisabled")) || truthy(value.get("m_bInDevelopment")),
            brawl: text(value, "m_eAbilityRequirements").is_some_and(|r| r.split('|').any(|p| p.trim() == "ERequirementStreetBrawl")),
            shop_icon: picture(text(value, "m_strShopIconLarge")),
        });
    }
    Ok(out)
}

fn prices(generic: Option<&Value>) -> Vec<u32> {
    generic
        .and_then(|g| g.get("m_nItemPricePerTier"))
        .and_then(|v| v.array().ok())
        .map(|rows| {
            rows.iter()
                .map(|v| {
                    // Compiled definitions hold numbers; text ones hold them as words.
                    v.integer().ok().and_then(|i| u32::try_from(i).ok()).or_else(|| v.string().ok().and_then(|s| s.trim().parse().ok())).unwrap_or(0)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Item names from the game's localisation (`"key" "value"` pairs).
#[derive(Debug, Clone, Default)]
pub struct Names(BTreeMap<String, String>);
impl Names {
    /// Every pair of the file (header keys included), read by `item_names::pairs`,
    /// the one reader of the names format; the first of a repeated key wins.
    pub fn parse(text: &str) -> Self {
        let mut names = BTreeMap::new();
        for pair in crate::item_names::pairs(text) {
            names.entry(casefold(&pair.key)).or_insert_with(|| crate::item_names::unescape(&text[pair.value]));
        }
        Self(names)
    }
    /// The names beside a game archive (`<folder of pak01_dir.vpk>/resource/localization`).
    pub fn beside(archive: &Path) -> Self {
        archive
            .parent()
            .and_then(|folder| std::fs::read_to_string(folder.join("resource").join("localization").join(NAME_FILE)).ok())
            .map(|text| Self::parse(&text))
            .unwrap_or_default()
    }
    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.get(&casefold(key)).map(String::as_str).filter(|s| !s.trim().is_empty() && !s.starts_with('#'))
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    /// Every key read (casefolded, in sorted order), header keys such as
    /// `Language` included.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.0.keys().map(String::as_str)
    }
}

/// `upgrade_clip_size` → "Clip size"; `2025_snowball_psd.vtex_c` → "2025 snowball".
pub fn readable(name: &str) -> String {
    let stem = name.rsplit('/').next().unwrap_or(name);
    let stem = stem.trim_end_matches(".vtex_c").trim_end_matches("_psd").trim_end_matches("_png");
    let stem = stem.strip_prefix("upgrade_").unwrap_or(stem);
    let words: Vec<&str> = stem.split(['_', ' ']).filter(|w| !w.is_empty()).collect();
    let mut out = words.join(" ");
    if let Some(first) = out.get(..1) {
        out = first.to_uppercase() + &out[1..];
    }
    out
}

/// The game's item definitions and price table (`None` when the archive has no
/// ability definitions).
pub struct Definitions {
    items: Vec<Definition>,
    prices: Vec<u32>,
}
impl Definitions {
    pub fn read(archive: &Vpk, cancel: &CancellationToken) -> Result<Self> {
        let load = |name: &str| -> Result<Option<Value>> {
            cancel.check()?;
            let Some(key) = [format!("scripts/{name}.vdata_c"), format!("scripts/{name}.vdata")].into_iter().find(|k| archive.entries.contains_key(k)) else {
                return Ok(None);
            };
            Ok(Some(icon_catalog::definition(&read(archive, &key)?)?))
        };
        let abilities = load("abilities")?.ok_or_else(|| fail(format!("{ITEM_ICONS_UNREADABLE}: the game has no ability definitions (scripts/abilities.vdata_c).")))?;
        let generic = load("generic_data")?;
        Ok(Self { items: definitions(&abilities)?, prices: prices(generic.as_ref()) })
    }
}

/// The catalog's rows without texture details: which pictures are item icons
/// and where the page shows them. Cheap (no picture is read), so a build uses
/// it as its allow-list.
pub fn classify(archive: &Vpk, defs: &Definitions, names: &Names) -> Vec<ItemIcon> {
    let name = |key: &str| names.get(key).map(str::to_owned).unwrap_or_else(|| readable(key));
    let mut rows: BTreeMap<String, ItemIcon> = BTreeMap::new();
    let blank = |path: &str, label: String, category: &str, group: String| ItemIcon {
        target: archive.entries.get(path).map(|e| e.path.clone()).unwrap_or_else(|| path.to_owned()),
        path: archive.entries.get(path).map(|e| e.path.clone()).unwrap_or_else(|| path.to_owned()),
        label,
        category: category.into(),
        group,
        tier: None,
        price: None,
        items: vec![],
        names: vec![],
        slot: None,
        supported: false,
        reason: String::new(),
        width: 0,
        height: 0,
        original_sha256: None,
        method: None,
        shared: false,
        association: "item".into(),
        switched_off: false,
    };
    // Owners first: sold items, then Legendary, then switched-off Legendary, then the rest.
    let rank = |d: &Definition| if d.sold() { 0 } else if d.legendary() { 1 } else if d.legendary_off() { 2 } else { 3 };
    let mut ordered: Vec<&Definition> = defs.items.iter().filter(|d| d.shop_icon.is_some()).collect();
    ordered.sort_by_key(|d| rank(d));
    for def in ordered {
        let path = def.shop_icon.clone().unwrap();
        if !archive.entries.contains_key(&path) {
            continue;
        }
        let row = rows.entry(path.clone()).or_insert_with(|| {
            let (category, group, price) = if def.sold() {
                (def.slot.unwrap(), def.tier.unwrap().to_string(), defs.prices.get(def.tier.unwrap() as usize).copied().filter(|p| *p > 0))
            } else if def.legendary() {
                ("legendary", def.tier.map(|t| t.to_string()).unwrap_or_else(|| "5".into()), None)
            } else if def.legendary_off() {
                ("legendary", "off".to_owned(), None)
            } else {
                ("misc", "unsold".to_owned(), None)
            };
            let mut row = blank(&path, name(&def.key), category, group);
            row.tier = def.tier;
            row.price = price;
            row.switched_off = def.off && !def.sold() && !def.legendary();
            if category == "misc" {
                row.slot = def.slot.map(str::to_owned);
            }
            row
        });
        row.items.push(def.key.clone());
        row.names.push(name(&def.key));
    }
    // Pictures in the item folders no item names.
    for entry in archive.entries.values() {
        let key = casefold(&entry.path);
        if key.starts_with(ITEM_FOLDER) && key.ends_with(".vtex_c") && !rows.contains_key(&key) {
            let group = if key.starts_with(&format!("{ITEM_FOLDER}seasonal/")) { "seasonal" } else { "unused" };
            rows.insert(key.clone(), blank(&key, readable(&key), "misc", group.into()));
        }
    }
    let mut out: Vec<ItemIcon> = rows.into_values().collect();
    let category_rank = |c: &str| CATEGORIES.iter().position(|k| *k == c).unwrap_or(CATEGORIES.len());
    let group_rank = |row: &ItemIcon| {
        if row.category == "misc" {
            MISC_GROUPS.iter().position(|(k, _)| *k == row.group).unwrap_or(MISC_GROUPS.len())
        } else {
            row.group.parse::<usize>().unwrap_or(99)
        }
    };
    out.sort_by(|a, b| {
        (category_rank(&a.category), group_rank(a), casefold(&a.label), casefold(&a.target))
            .cmp(&(category_rank(&b.category), group_rank(b), casefold(&b.label), casefold(&b.target)))
    });
    out
}

/// The casefolded targets of every item icon (the build's allow-list).
pub fn allowed(archive: &Vpk, cancel: &CancellationToken) -> Result<BTreeSet<String>> {
    let defs = Definitions::read(archive, cancel)?;
    Ok(classify(archive, &defs, &Names::default()).into_iter().map(|r| casefold(&r.target)).collect())
}

/// The whole catalog: every row with its picture checked as the Icons tab
/// checks a hero's icons (size, format, whether it can be replaced).
pub fn catalog(archive: &Vpk, names: &Names, cancel: &CancellationToken) -> Result<ItemCatalog> {
    let defs = Definitions::read(archive, cancel)?;
    let mut warnings = vec![];
    if names.is_empty() {
        warnings.push("The game's English item names were not found beside the game archive; internal names are shown.".into());
    }
    if defs.prices.is_empty() {
        warnings.push("The game's price per tier was not found (scripts/generic_data.vdata_c); prices are not shown.".into());
    }
    let mut items = classify(archive, &defs, names);
    for item in &mut items {
        cancel.check()?;
        let outcome = (|| -> Result<()> {
            let raw = read(archive, &item.target)?;
            item.original_sha256 = Some(sha(&raw));
            let meta = icons::texture_metadata(&raw)?;
            item.width = meta.display_width;
            item.height = meta.display_height;
            item.method = Some(icons::method(&meta, &raw, cancel)?.into());
            Ok(())
        })();
        match outcome {
            Ok(()) => item.supported = true,
            Err(deadlock_core::CoreError::Cancelled) => return Err(deadlock_core::CoreError::Cancelled),
            Err(e) => item.reason = e.to_string(),
        }
    }
    Ok(ItemCatalog { items, prices: defs.prices, names_found: !names.is_empty(), warnings })
}

/// An empty receipt for projects and builds without item icons.
pub fn empty_receipt() -> icons::IconReceipt {
    icons::IconReceipt { version: 1, entries: vec![], native_validated: false, in_game_validated: false }
}

/// Stages the item icon rows as `icons::stage` stages a hero's icons, with the
/// game's item icons as the allow-list. Builds nothing when there are no rows.
pub fn stage(archive: &Vpk, replacements: &[deadlock_project::customization::ResourceReplacement], content: &Path, cancel: &CancellationToken) -> Result<icons::IconReceipt> {
    if !replacements.iter().any(|r| r.enabled.unwrap_or(true)) {
        return Ok(empty_receipt());
    }
    let allowed = allowed(archive, cancel)?.into_iter().map(|k| (k, false)).collect();
    icons::stage_allowed(archive, replacements, content, Some(allowed), &icons::ITEM_WORDS, cancel)
}

/// Every staged item icon is still an item icon of this game with the same
/// texture settings (install and frozen-build checks). Free without entries.
pub fn verify_entries(archive: &Vpk, receipt: &icons::IconReceipt, cancel: &CancellationToken) -> Result<()> {
    if receipt.entries.is_empty() {
        return Ok(());
    }
    let allowed = allowed(archive, cancel)?;
    for item in &receipt.entries {
        if !allowed.contains(&casefold(&item.target)) {
            return Err(fail(format!("{NOT_AN_ITEM_ICON}: {}", item.target)));
        }
        let original = read(archive, &item.target)?;
        if icons::texture_metadata(&original)? != item.original_metadata {
            return Err(fail("Saved item icon settings differ from the game's own item icon."));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn names_and_readable_fallbacks() {
        let names = Names::parse("\u{feff}\"lang\"\n{\n \"Tokens\"\n {\n  // \"upgrade_x\" \"no\"\n  \"upgrade_clip_size\"\t\t\"Extended Magazine\"\n  \"upgrade_quote\" \"Say \\\"hi\\\"\" // note\n  \"upgrade_empty\" \"\"\n }\n}\n");
        assert_eq!(names.get("UPGRADE_CLIP_SIZE"), Some("Extended Magazine"));
        assert_eq!(names.get("upgrade_quote"), Some("Say \"hi\""));
        assert_eq!(names.get("upgrade_x"), None, "commented out");
        assert_eq!(names.get("upgrade_empty"), None);
        assert_eq!(readable("upgrade_clip_size"), "Clip size");
        assert_eq!(readable("panorama/images/items/seasonal/2025_snowball_psd.vtex_c"), "2025 snowball");
    }
    #[test]
    fn disabled_comes_as_a_boolean_or_a_number() {
        let v = |k: Kind| Value::new(k);
        assert!(truthy(Some(&v(Kind::Bool(true)))));
        assert!(truthy(Some(&v(Kind::I64(1)))));
        assert!(!truthy(Some(&v(Kind::Bool(false)))));
        assert!(!truthy(None));
        assert_eq!(picture(Some("file://{images}/items/weapon/basic_magazine.psd")).as_deref(), Some("panorama/images/items/weapon/basic_magazine_psd.vtex_c"));
        assert_eq!(picture(Some("file://{images}/hud/abilities/lash/lash_death_slam.psd")), None, "a hero's ability picture is not an item icon");
        assert_eq!(picture(Some("file://{images}/hud/modifiers/icon_parry.svg")), None);
    }
}
