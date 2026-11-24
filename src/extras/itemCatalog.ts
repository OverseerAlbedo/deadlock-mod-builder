// Extras → Items: the game's item catalog (Rust `item_icons::ItemCatalog`) and
// how the page groups it: category tabs, then tier rows (shop and Legendary)
// or Misc groups, in the order the catalog gives.
import type { ResourceReplacement } from "../models";
import type { CatalogEntry } from "../components/AssetCatalog";
import { assetItems, type AssetItem } from "./models";

export type ItemCategory = "weapon" | "vitality" | "spirit" | "legendary" | "misc";
/** One item picture (Rust `item_icons::ItemIcon`). */
export interface ItemIcon extends CatalogEntry {
  category: ItemCategory;
  /** The tier ("1" … "5") for shop and Legendary rows, "off" for Legendary items
   * the game switches off; a Misc group key for Misc. */
  group: string;
  tier: number | null;
  /** Souls, from the game's price per tier (shop rows only). */
  price: number | null;
  /** Item records whose picture this is, and their names (owner first). */
  items: string[];
  names: string[];
  /** For Misc rows: the item's shop category, when it has one. */
  slot: string | null;
  method: string | null;
  /** The game switches this item off (disabled or in development): not in the game now. */
  switched_off?: boolean;
}
export interface ItemCatalog {
  items: ItemIcon[];
  prices: number[];
  names_found: boolean;
  warnings: string[];
}
export const ITEM_CATEGORIES: { key: ItemCategory; label: string }[] = [
  { key: "weapon", label: "Weapon" },
  { key: "vitality", label: "Vitality" },
  { key: "spirit", label: "Spirit" },
  { key: "legendary", label: "Legendary" },
  { key: "misc", label: "Misc" },
];
/** Misc groups (Rust `item_icons::MISC_GROUPS`, same keys and order). */
export const MISC_GROUPS: { key: string; label: string; note: string }[] = [
  { key: "unsold", label: "Items not in the shop now", note: "Switched off, in development or test items" },
  { key: "seasonal", label: "Seasonal pictures", note: "From the game's seasonal item folder" },
  { key: "unused", label: "Pictures no item uses", note: "In the item folders, named by no item" },
  { key: "saved", label: "Saved, not in the game now", note: "Your saved item icons whose picture the game no longer lists" },
];
export const MAX_ITEM_ICONS = 512;
/** The Legendary group of Street Brawl items the game switches off (Rust group "off"). */
export const SWITCHED_OFF = { label: "Switched off in the game", note: "Not in Street Brawl now · you can still prepare an icon" };
/** Said of a switched-off item: on its tile, in its chips and in the editor. */
export const SWITCHED_OFF_REASON = "The game has switched this item off, so it does not show it now.";

const ROMAN = ["", "I", "II", "III", "IV", "V"];
export function tierName(tier: number | null | string) {
  const n = Number(tier);
  return `Tier ${ROMAN[n] ?? n}`;
}
export function souls(price: number) {
  return price.toLocaleString("en-US");
}
export function categoryLabel(key: string) {
  return ITEM_CATEGORIES.find((c) => c.key === key)?.label ?? "Misc";
}
export function itemOf(item: AssetItem): ItemIcon | null {
  return item.missing ? null : (item.entry as ItemIcon);
}
export function categoryOf(item: AssetItem): ItemCategory {
  return itemOf(item)?.category ?? "misc";
}
/** "Tier I · 800 souls", "Street Brawl item", "Older item icons" … */
export function placeText(entry: ItemIcon) {
  if (entry.category === "legendary") return `${tierName(entry.tier ?? 5)} · Street Brawl`;
  if (entry.category === "misc") return MISC_GROUPS.find((g) => g.key === entry.group)?.label ?? "Misc";
  return entry.price ? `${tierName(entry.tier)} · ${souls(entry.price)} souls` : tierName(entry.tier);
}

export function itemAssets(catalog: ItemCatalog | null, rows: ResourceReplacement[]) {
  return assetItems(catalog?.items ?? [], rows);
}

export interface ItemGroup {
  key: string;
  category: ItemCategory;
  title: string;
  price: number | null;
  note: string;
  items: AssetItem[];
}
function groupKey(item: AssetItem) {
  const entry = itemOf(item);
  return entry ? `${entry.category}:${entry.group}` : "misc:saved";
}
/** Rows of one category (or every category for search results), in catalog order. */
export function itemGroups(items: AssetItem[], category: ItemCategory | null): ItemGroup[] {
  const groups = new Map<string, ItemGroup>();
  // Saved rows without a picture come first in Misc, so they are easy to find.
  const ordered = [...items.filter((i) => i.missing), ...items.filter((i) => !i.missing)];
  for (const item of ordered) {
    const cat = categoryOf(item);
    if (category && cat !== category) continue;
    const key = groupKey(item);
    let group = groups.get(key);
    if (!group) {
      const entry = itemOf(item);
      const misc = cat === "misc" ? MISC_GROUPS.find((g) => g.key === (entry?.group ?? "saved")) : cat === "legendary" && entry?.group === "off" ? { ...SWITCHED_OFF, key: "off" } : null;
      group = {
        key,
        category: cat,
        title: misc ? misc.label : tierName(entry?.tier ?? entry?.group ?? ""),
        price: cat === "legendary" || cat === "misc" ? null : entry?.price ?? null,
        note: misc ? misc.note : cat === "legendary" ? "Street Brawl" : "",
        items: [],
      };
      groups.set(key, group);
    }
    group.items.push(item);
  }
  const list = [...groups.values()];
  if (category === null) {
    const rank = (c: ItemCategory) => ITEM_CATEGORIES.findIndex((k) => k.key === c);
    list.sort((a, b) => rank(a.category) - rank(b.category));
  }
  return list;
}

/** Whether the item matches every word of the search; `also` adds names of its own (a new name). */
export function matches(item: AssetItem, query: string, also: string[] = []) {
  const q = query.trim().toLowerCase();
  if (!q) return true;
  const entry = itemOf(item);
  // Names and the game's internal item names; not the file path, whose folder
  // (items/spirit/…) would match every item of a category.
  const text = [item.entry.label, ...also, ...(entry?.names ?? []), ...(entry?.items ?? []), ...(entry ? [] : [item.entry.target])]
    .join(" ")
    .toLowerCase();
  return q.split(/\s+/).every((word) => text.includes(word));
}
/** The items the search and "Changed only" show; `renamed` gives an item's saved new name
 * (searched too, and a change of its own). */
export function filterItems(items: AssetItem[], query: string, changedOnly: boolean, renamed: (item: AssetItem) => string | null = () => null) {
  return items.filter((item) => {
    const name = renamed(item);
    return (!changedOnly || item.row || name !== null) && matches(item, query, name === null ? [] : [name]);
  });
}
export function categoryCounts(items: AssetItem[]) {
  // `all` counts what the game has now; switched-off items are counted in `off`.
  const counts = new Map<ItemCategory, { all: number; off: number; replaced: number; rows: number }>();
  for (const { key } of ITEM_CATEGORIES) counts.set(key, { all: 0, off: 0, replaced: 0, rows: 0 });
  for (const item of items) {
    const count = counts.get(categoryOf(item))!;
    count.rows++;
    if (!item.missing) {
      if (itemOf(item)?.switched_off) count.off++;
      else count.all++;
    }
    if (item.row) count.replaced++;
  }
  return counts;
}
