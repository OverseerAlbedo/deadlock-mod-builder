// A small item catalog shaped like Rust `item_icons::ItemCatalog`, for tests.
import type { ItemCatalog, ItemCategory, ItemIcon } from "./itemCatalog";

let n = 0;
export function itemEntry(
  category: ItemCategory,
  group: string,
  label: string,
  extra: Partial<ItemIcon> = {},
): ItemIcon {
  n++;
  const folder = category === "legendary" ? "brawl" : category === "misc" ? "seasonal" : category;
  const target = `panorama/images/items/${folder}/${label.toLowerCase().replace(/[^a-z0-9]+/g, "_")}_psd.vtex_c`;
  const tier = /^[1-5]$/.test(group) ? Number(group) : null;
  const prices = [0, 800, 1600, 3200, 6400];
  return {
    path: target,
    target,
    label,
    category,
    group,
    tier,
    price: category === "legendary" || category === "misc" || tier === null ? null : prices[tier],
    items: [`upgrade_${n}`],
    names: [label],
    slot: null,
    supported: true,
    reason: "",
    width: 200,
    height: 200,
    original_sha256: n.toString(16).padStart(64, "0"),
    method: "panorama_texture",
    shared: false,
    association: "item",
    ...extra,
  };
}
export function itemCatalogFixture(): ItemCatalog {
  return {
    items: [
      itemEntry("weapon", "1", "Close Quarters"),
      itemEntry("weapon", "1", "Extended Magazine", { items: ["upgrade_clip_size", "item_projectile_test_01"], names: ["Extended Magazine", "Projectile test"] }),
      itemEntry("weapon", "2", "High-Velocity Rounds"),
      itemEntry("weapon", "4", "Glass Cannon"),
      itemEntry("vitality", "1", "Extra Health"),
      itemEntry("vitality", "3", "Metal Skin"),
      itemEntry("spirit", "1", "Extra Spirit"),
      itemEntry("spirit", "4", "Echo Shard"),
      itemEntry("legendary", "5", "Seraphim Wings"),
      itemEntry("legendary", "off", "Apex Combat", { tier: 5, switched_off: true }),
      itemEntry("misc", "seasonal", "2025 snowball"),
    ],
    prices: [0, 800, 1600, 3200, 6400, 9999],
    names_found: true,
    warnings: [],
  };
}
