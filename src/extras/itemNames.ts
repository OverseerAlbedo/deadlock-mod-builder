/** Extras → Items: renamed shop items (`customization.item_names`, backend
 * `deadlock-project item_names.rs`). Rows are `{item, enabled, name}` built field by field
 * (the backend refuses unknown keys): one row per item token ignoring case, at most
 * MAX_ITEM_NAMES, the name trimmed and checked by `cleanName` with the same rules and
 * messages as Rust `clean_name` (a Rust test reads this file), and the key is left out of the
 * customization when the last row goes. Item names are the same for every hero. */
import type { Customization, Project } from "../models";
import { changeCustomization } from "../components/customization";

export interface ItemNameRow {
  item: string;
  enabled: boolean;
  name: string;
}

export const MAX_ITEM_NAMES = 256;
export const MAX_NAME_CHARS = 32;
export const NAME_EMPTY = "Type a name for the item.";
export const NAME_TOO_LONG = "An item name is at most 32 characters.";
export const NAME_ONE_LINE = "An item name is one line without control characters.";
export const NAME_REFUSED_CHAR = "An item name cannot hold \" \\ < > { } or %: the game's names file and text display read them as their own codes.";
export const NAME_LEADING_HASH = "An item name cannot start with #: the game's names file reads that as a reference to another text.";
/** Characters a name may not hold (Rust `REFUSED_CHARS`). */
export const REFUSED_CHARS = ["\"","\\","<",">","{","}","%"] as const;

/** Rust's `char::is_control`: the Unicode Cc category (C0, DEL and C1). */
const isControl = (c: string) => {
  const code = c.codePointAt(0) ?? 0;
  return code < 0x20 || (code >= 0x7f && code <= 0x9f);
};
/** Rust's `str::trim`: Unicode White_Space at both ends. */
const SPACE = "[\\t\\n\\v\\f\\r \\u0085\\u00a0\\u1680\\u2000-\\u200a\\u2028\\u2029\\u202f\\u205f\\u3000]";
const WHITE = new RegExp(`^${SPACE}+|${SPACE}+$`, "gu");

/** The name as it is saved (trimmed), or the reason it is refused. */
export function cleanName(name: string): { name: string } | { error: string } {
  const trimmed = name.replace(WHITE, "");
  const chars = [...trimmed];
  if (!chars.length) return { error: NAME_EMPTY };
  if (chars.some(isControl)) return { error: NAME_ONE_LINE };
  if (chars.length > MAX_NAME_CHARS) return { error: NAME_TOO_LONG };
  if (chars.some((c) => (REFUSED_CHARS as readonly string[]).includes(c))) return { error: NAME_REFUSED_CHAR };
  if (trimmed.startsWith("#")) return { error: NAME_LEADING_HASH };
  return { name: trimmed };
}

const fold = (value: string) => value.toLowerCase();
const row = (item: string, enabled: boolean, name: string): ItemNameRow => ({ item, enabled, name });

/** The saved rows (an absent key or an emptied list are both "none"). */
export function savedItemNames(project: Project): ItemNameRow[] {
  const rows = project.customization.item_names;
  return Array.isArray(rows) ? (rows as ItemNameRow[]) : [];
}
export const itemNameOf = (rows: ItemNameRow[], item: string) => rows.find((r) => fold(r.item) === fold(item)) ?? null;

/** The rows with `item` renamed to `name`. A new row is `{item, enabled: true, name}`; an
 * existing row keeps its own item spelling and whether it is included. Refused (rows
 * unchanged, with the reason) for a name `cleanName` refuses or when MAX_ITEM_NAMES are saved. */
export function withItemName(rows: ItemNameRow[], item: string, name: string): { rows: ItemNameRow[]; error?: string } {
  const clean = cleanName(name);
  if ("error" in clean) return { rows, error: clean.error };
  const at = rows.findIndex((r) => fold(r.item) === fold(item));
  if (at < 0) {
    if (rows.length >= MAX_ITEM_NAMES) return { rows, error: `Item names support at most ${MAX_ITEM_NAMES} renamed items.` };
    return { rows: [...rows, row(item, true, clean.name)] };
  }
  return { rows: rows.map((r, i) => (i === at ? row(r.item, r.enabled !== false, clean.name) : r)) };
}
/** Switch one renamed item on or off (nothing happens without a row). */
export function withItemNameEnabled(rows: ItemNameRow[], item: string, enabled: boolean): ItemNameRow[] {
  return rows.map((r) => (fold(r.item) === fold(item) ? row(r.item, enabled, r.name) : r));
}
export const withoutItemName = (rows: ItemNameRow[], item: string) => rows.filter((r) => fold(r.item) !== fold(item));
/** Pause all extras (false) / Resume all extras (true). */
export const pausedItemNames = (rows: ItemNameRow[], enabled: boolean) => rows.map((r) => row(r.item, enabled, r.name));
/** The project with these rows; the key is left out when there are none. */
export function withItemNameRows(project: Project, rows: ItemNameRow[], activate: boolean): Project {
  const next = changeCustomization(project, { item_names: rows }, activate);
  if (rows.length) return next;
  const { item_names: _drop, ...customization } = next.customization;
  return { ...next, customization: customization as Customization };
}
/** Counts for the Extras page header, tab dot and Pause all. */
export function itemNameCounts(project: Project) {
  const rows = savedItemNames(project);
  return { saved: rows.length, enabled: rows.filter((r) => r.enabled !== false).length };
}

/** One saved row against the game's names (backend `item_names::RowState`). */
export interface ItemNameRowState {
  item: string;
  /** The game's English name of the item now; absent when the game has no such item. */
  game_name?: string;
  /** Whether the game has the item (a switched-off row applies once switched on). */
  known: boolean;
}
/** `api.itemNamesState` (backend `items.rs ItemNamesState`): read-only, no lease, kept per
 * game names folder; asked by the page only, never at project open. */
export interface ItemNamesState {
  /** Whether the game's English names file with item names was found beside the archive. */
  found: boolean;
  /** Casefolded item token (`upgrade_close_range`) → the game's English name. */
  names: Record<string, string>;
  /** The languages a build writes one names file for. */
  languages: string[];
  /** Each saved row, in row order. */
  rows: ItemNameRowState[];
  /** Saved rows naming an item the game does not have now (Analyze lists them; the build refuses them). */
  unknown: string[];
}
