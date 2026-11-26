// Extras → Sounds: rows, filters and "Replace all" over the tags, names and
// variation families that Rust discovery computes (`sound_tags.rs`). No tag rule
// lives here; this file only groups, filters and counts what each row carries.
// i18n-area: sounds
import type { CatalogEntry, SoundInfo } from "../components/AssetCatalog";
import type { ResourceReplacement } from "../models";
import { m, mc, t, tc, tp } from "../i18n";
import { assetLabel, detailNumber, isSilenced, replacementStatus, type AssetItem } from "./models";
// A sound row can also silence its sound (`silence`, no file): `silencedRow`,
// `silenceAll`. It counts as a change everywhere a replacement does.

export const MAX_REPLACEMENTS = 128;
export type TagKey = "hero" | "ability" | "weapon" | "voice" | "other";
// Labels stay English here (the search text holds them); they are translated where drawn.
export const TAGS: { key: TagKey; label: string }[] = [
  { key: "hero", label: mc("sound tag", "Hero") },
  { key: "ability", label: mc("sound tag", "Ability") },
  { key: "weapon", label: mc("sound tag", "Weapon") },
  { key: "voice", label: mc("sound tag", "Voice") },
  { key: "other", label: mc("sound tag", "Other") },
];
export const VOICE_KINDS: { key: string; label: string }[] = [
  { key: "ping", label: m("Pings") },
  { key: "emote", label: m("Emotes and efforts") },
  { key: "ability", label: m("Ability lines") },
  { key: "match_start", label: m("Match start") },
  { key: "kill", label: m("Kills") },
  { key: "select", label: m("Hero select") },
  { key: "about", label: m("Other heroes about this hero") },
];
/** A tag's name as shown. */
export const tagName = (key: string) => tc("sound tag", tagLabel(key));
/** A voice sub-kind's name as shown. */
export const kindName = (key: string) => t(kindLabel(key));
const tagLabel = (key: string) => TAGS.find((tag) => tag.key === key)?.label ?? key;
const kindLabel = (key: string) =>
  VOICE_KINDS.find((k) => k.key === key)?.label ?? key;

// The two group names made here (the others come from the game's catalog as they are).
const OTHER_SOUNDS = m("Other sounds");
const TO_PLACE_AGAIN = m("Saved replacements to place again");
/** A group's name as shown: the two made here translated, the catalog's as it comes. */
export const groupText = (name: string) => (name === OTHER_SOUNDS || name === TO_PLACE_AGAIN ? t(name) : name);
/** Sound info of an entry; catalogs made before tagging get a plain fallback. */
export function soundOf(entry: CatalogEntry): SoundInfo {
  return (
    entry.sound ?? {
      name: assetLabel(entry),
      group: "other:",
      group_name: OTHER_SOUNDS,
      group_order: 400,
      tags: [],
      family: entry.target || entry.path,
      family_source: "name",
    }
  );
}
/** Main tags plus `other` for a sound with none of ability, weapon or voice. */
export function tagsOf(info: SoundInfo): TagKey[] {
  const tags = info.tags.filter((tag): tag is TagKey =>
    ["hero", "ability", "weapon", "voice"].includes(tag),
  );
  return tags.some((tag) => tag !== "hero") ? tags : [...tags, "other"];
}

export interface SoundRow {
  key: string;
  /** Variations in order; one item for a sound without variations. */
  items: AssetItem[];
  name: string;
  group: string;
  groupName: string;
  groupOrder: number;
  tags: TagKey[];
  voice: string[];
  fromEvent: boolean;
  search: string;
}
const MISSING_GROUP = "saved:";

/** One row per variation family (or per file when `fold` is off). */
export function soundRows(items: AssetItem[], fold = true): SoundRow[] {
  const families = new Map<string, AssetItem[]>();
  for (const item of items) {
    const key = item.missing
      ? item.key
      : fold
        ? `family:${soundOf(item.entry).family}`
        : item.key;
    const list = families.get(key);
    if (list) list.push(item);
    else families.set(key, [item]);
  }
  const rows: SoundRow[] = [];
  for (const [key, list] of families) {
    list.sort(
      (a, b) =>
        (soundOf(a.entry).variation ?? 0) - (soundOf(b.entry).variation ?? 0) ||
        a.entry.path.localeCompare(b.entry.path),
    );
    const info = soundOf(list[0].entry);
    const tags = [...new Set(list.flatMap((i) => tagsOf(soundOf(i.entry))))];
    const voice = [...new Set(list.flatMap((i) => soundOf(i.entry).voice ?? []))];
    const missing = list[0].missing;
    const row: SoundRow = {
      key,
      items: list,
      name: info.name,
      group: missing ? MISSING_GROUP : info.group,
      groupName: missing ? TO_PLACE_AGAIN : info.group_name,
      groupOrder: missing ? -1 : info.group_order,
      tags: missing ? [] : tags,
      voice: missing ? [] : voice,
      fromEvent: info.family_source === "event" && list.length > 1,
      search: "",
    };
    row.search = [
      row.name,
      row.groupName,
      ...row.tags.map(tagLabel),
      ...row.voice.map(kindLabel),
      ...list.flatMap((i) => [
        i.entry.path,
        i.entry.label,
        i.row?.source ?? "",
        isSilenced(i.row) ? "silenced" : "",
        ...(soundOf(i.entry).events ?? []),
      ]),
    ]
      .join(" ")
      .toLowerCase()
      .replace(/[_\-/.]+/g, " ");
    rows.push(row);
  }
  return rows.sort(
    (a, b) =>
      a.groupOrder - b.groupOrder ||
      a.groupName.localeCompare(b.groupName) ||
      a.name.localeCompare(b.name) ||
      a.items[0].entry.path.localeCompare(b.items[0].entry.path),
  );
}

export interface SoundFilter {
  query: string;
  tags: TagKey[];
  /** Voice sub-kinds; a row matches any of them. Only used with the Voice tag. */
  voice: string[];
  replacedOnly: boolean;
  replaceableOnly?: boolean;
}
/** The row's tag, voice-kind and made-here group names as they are shown in the current language
 * (in English the same words the search text already holds). */
function shownNames(row: SoundRow) {
  return [...row.tags.map(tagName), ...row.voice.map(kindName), groupText(row.groupName)].join(" ").toLowerCase();
}
/** Every search word is in the row's search text (English names, file and sound names as they are)
 * or in the names the row's chips and heading show in the current language. */
function searchMatches(row: SoundRow, query: string) {
  const words = query.toLowerCase().replace(/[_\-/.]+/g, " ").split(/\s+/).filter(Boolean);
  if (!words.length) return true;
  let shown: string | undefined;
  return words.every((w) => row.search.includes(w) || (shown ??= shownNames(row)).includes(w));
}
function baseMatches(row: SoundRow, f: SoundFilter) {
  return (
    searchMatches(row, f.query) &&
    (!f.replacedOnly || row.items.some((i) => i.row)) &&
    (!f.replaceableOnly || row.items.some((i) => i.entry.supported))
  );
}
/** Rows with every chosen tag (and, with Voice, any chosen sub-kind). */
export function filterRows(rows: SoundRow[], f: SoundFilter) {
  return rows.filter(
    (row) =>
      baseMatches(row, f) &&
      f.tags.every((tag) => row.tags.includes(tag)) &&
      (!f.tags.includes("voice") ||
        f.voice.length === 0 ||
        f.voice.some((k) => row.voice.includes(k))),
  );
}
/** Rows per tag and per voice sub-kind for the current search (not the tag choice). */
export function tagCounts(rows: SoundRow[], f: SoundFilter) {
  const tags = new Map<string, number>(),
    voice = new Map<string, number>();
  for (const row of rows) {
    if (!baseMatches(row, f)) continue;
    for (const tag of row.tags) tags.set(tag, (tags.get(tag) ?? 0) + 1);
    if (f.tags.every((tag) => row.tags.includes(tag)))
      for (const k of row.voice) voice.set(k, (voice.get(k) ?? 0) + 1);
  }
  return { tags, voice };
}
/** What the filter line says: the count, then one self-contained clause per active filter
 * (each its own message, so a language can word and join them its own way), then the files. */
export function shownText(count: number, files: number, f: SoundFilter) {
  const parts: string[] = [];
  if (f.tags.length === 1) parts.push(t("tagged {tag}", { tag: tagName(f.tags[0]) }));
  else if (f.tags.length === 2) parts.push(t("that are both {first} and {second}", { first: tagName(f.tags[0]), second: tagName(f.tags[1]) }));
  else if (f.tags.length > 2) {
    const names = f.tags.map(tagName);
    parts.push(t("that are all of {list} and {last}", { list: names.slice(0, -1).join(tc("list", ", ")), last: names[names.length - 1] }));
  }
  if (f.tags.includes("voice") && f.voice.length)
    parts.push(t("({kinds})", { kinds: f.voice.map(kindName).join(tc("list", " or ")) }));
  if (f.replacedOnly) parts.push(t("with a replacement"));
  if (f.query.trim()) parts.push(t("matching “{query}”", { query: f.query.trim() }));
  const variations =
    files > count ? ` · ${tp(files, { one: "{count} file with its variations", other: "{count} files with their variations" }, { count: files.toLocaleString("en-US") })}` : "";
  const head = tp(count, { one: "Showing {count} sound", other: "Showing {count} sounds" }, { count: count.toLocaleString("en-US") });
  return `${head}${parts.length ? tc("between filter clauses", " ") + parts.join(tc("between filter clauses", " ")) : ""}${variations}`;
}
export function lengthText(items: AssetItem[]) {
  const values = items
    .map((i) => detailNumber(i.entry, "duration"))
    .filter((v): v is number => v !== null);
  if (!values.length) return "";
  const lo = Math.min(...values),
    hi = Math.max(...values),
    text = (v: number) => (v < 10 ? v.toFixed(1) : Math.round(v).toString());
  return text(lo) === text(hi) ? t("{value} s", { value: text(lo) }) : t("{low}–{high} s", { low: text(lo), high: text(hi) });
}
/** The row's state as the old library showed it, for a family summed up. */
export function rowStatus(row: SoundRow, active: boolean) {
  const replaced = row.items.filter((i) => i.row);
  if (!replaced.length) {
    if (row.items.every((i) => !i.entry.supported)) return "View only";
    if (row.items.some((i) => i.entry.shared)) return "Shared";
    return "Original";
  }
  const states = replaced.map((i) => replacementStatus(i, active));
  if (states.includes("Needs attention")) return "Needs attention";
  if (states.every((s) => s === "Paused")) return "Paused";
  // Silenced sounds are changes like replacements; a mix of both is "changed".
  const silent = replaced.filter((i) => isSilenced(i.row)).length;
  const word = silent === 0 ? "Replaced" : silent === replaced.length ? "Silenced" : "Changed";
  return replaced.length === row.items.length || row.items.length === 1
    ? word
    : `${replaced.length} of ${row.items.length} ${word.toLowerCase()}`;
}
/** A status of `replacementStatus` or `rowStatus` as shown (the status itself stays English). */
export function statusText(status: string) {
  const some = /^(\d+) of (\d+) (replaced|silenced|changed)$/.exec(status);
  if (!some) return t(status);
  const values = { done: some[1], total: some[2] };
  return some[3] === "replaced" ? t("{done} of {total} replaced", values) : some[3] === "silenced" ? t("{done} of {total} silenced", values) : t("{done} of {total} changed", values);
}
/** States `rowStatus` gives a row the user changed (shown in the change colour). */
export function changedStatus(status: string) {
  return ["Replaced", "Silenced", "Changed"].includes(status) || / (replaced|silenced|changed)$/.test(status);
}

/** A row that silences its sound: no file, volume, timing or sound edit (they belong to a file). */
export function silencedRow(row: ResourceReplacement): ResourceReplacement {
  const {
    source: _source, volume_percent: _volume, audio_fit: _fit,
    start_seconds: _start, volume_db: _db, fade_in_seconds: _in, fade_out_seconds: _out,
    ...rest
  } = row;
  return { ...rest, silence: true };
}
/**
 * A row given a file. A silenced row drops silence and gets a file's defaults
 * back. An edited row keeps its volume and fades, and its part starts at 0 of
 * the new file (the old start belonged to the old file).
 */
export function withSource(row: ResourceReplacement, source: string): ResourceReplacement {
  if (typeof row.start_seconds === "number" && row.source !== source) row = { ...row, start_seconds: 0 };
  if (row.silence === undefined) return { ...row, source };
  const { silence: _silence, ...rest } = row;
  return { volume_percent: 100, audio_fit: "pad", ...rest, source };
}

// The reasons "Replace all" and "Silence all" give (kept English in the result; drawn with `reasonText`).
const VIEW_ONLY = "View only: ";
const CANNOT_REPLACE = m("this sound cannot be replaced yet");
const CANNOT_CHANGE = m("this sound cannot be changed yet");
const COMPILED_ONLY = m("A compiled .vsnd_c file only fits the sound it was made for. Choose a WAV, MP3, FLAC or OGG file to replace every variation.");
const NO_CONVERTED = m("Its game format cannot take a converted file. Replace it on its own with a matching .vsnd_c.");
const SHARED_REPLACE = m("Shared with other heroes. Tick “Also replace the shared variations” to include it.");
const SHARED_SILENCE = m("Shared with other heroes. Tick “Also silence the shared variations” to include it.");
const NOT_REWRITABLE = m("Its game format cannot be rewritten, so it cannot be silenced.");
const TOO_MANY = `Sounds already have ${MAX_REPLACEMENTS} replacements. Remove one before adding another.`;
/** A reason of `replaceAll`, `silenceAll` or `silenceRefusal` as shown; a reason from the
 * game's catalog (after "View only:") stays as it comes. */
export function reasonText(reason: string) {
  if (reason.startsWith(VIEW_ONLY)) {
    const rest = reason.slice(VIEW_ONLY.length);
    return t("View only: {reason}", { reason: rest === CANNOT_REPLACE || rest === CANNOT_CHANGE ? t(rest) : rest });
  }
  if (reason === TOO_MANY) return t("Sounds already have {count} replacements. Remove one before adding another.", { count: MAX_REPLACEMENTS });
  return t(reason);
}

export interface ReplaceAllResult {
  rows: ResourceReplacement[];
  set: AssetItem[];
  refused: { item: AssetItem; reason: string }[];
}
/**
 * "Replace all N variations with one file": one replacement row per variation
 * with the same source, keeping each existing row's volume, timing, inclusion
 * and shared choice. Every rule of a single replacement holds; whatever it
 * cannot set is returned with the reason.
 */
export function replaceAll(
  saved: ResourceReplacement[],
  items: AssetItem[],
  source: string,
  includeShared: boolean,
): ReplaceAllResult {
  const rows = saved.map((row) => ({ ...row }));
  const set: AssetItem[] = [],
    refused: ReplaceAllResult["refused"] = [];
  const compiled = /\.vsnd_c$/i.test(source);
  for (const item of items) {
    const { entry } = item;
    if (item.missing || !entry.supported) {
      refused.push({
        item,
        reason: `${VIEW_ONLY}${(entry.reason || CANNOT_REPLACE).replace(/^Integrity check failed:\s*/, "")}`,
      });
    } else if (compiled) {
      refused.push({
        item,
        reason: COMPILED_ONLY,
      });
    } else if (entry.wav_supported === false) {
      refused.push({
        item,
        reason: NO_CONVERTED,
      });
    } else if (entry.shared && !includeShared && !(item.row?.allow_shared ?? false)) {
      refused.push({
        item,
        reason: SHARED_REPLACE,
      });
    } else if (item.index >= 0) {
      rows[item.index] = {
        ...withSource(rows[item.index], source),
        ...(entry.shared && includeShared ? { allow_shared: true } : {}),
      };
      set.push(item);
    } else if (rows.length >= MAX_REPLACEMENTS) {
      refused.push({
        item,
        reason: TOO_MANY,
      });
    } else {
      rows.push({
        enabled: true,
        source,
        target: entry.target,
        allow_shared: Boolean(entry.shared && includeShared),
        ...(entry.original_sha256 ? { original_sha256: entry.original_sha256 } : {}),
        volume_percent: 100,
        audio_fit: "pad",
      });
      set.push(item);
    }
  }
  return { rows, set, refused };
}

/** Why a sound cannot be silenced, or "" when it can (the build's own rules). */
export function silenceRefusal(item: AssetItem) {
  if (item.missing || !item.entry.supported)
    return `${VIEW_ONLY}${(item.entry.reason || CANNOT_CHANGE).replace(/^Integrity check failed:\s*/, "")}`;
  if (item.entry.wav_supported === false)
    return NOT_REWRITABLE;
  return "";
}
/** A new row that silences `item` (shared sounds still need the shared choice). */
export function newSilencedRow(item: AssetItem, allowShared = false): ResourceReplacement {
  return {
    enabled: true,
    target: item.entry.target,
    allow_shared: allowShared,
    ...(item.entry.original_sha256 ? { original_sha256: item.entry.original_sha256 } : {}),
    silence: true,
  };
}
/**
 * "Silence all N variations": one silenced row per variation, with every rule
 * of "Replace all": view only, the game format, the shared choice and the
 * 128-row limit. Inclusion and the shared choice of existing rows are kept.
 */
export function silenceAll(
  saved: ResourceReplacement[],
  items: AssetItem[],
  includeShared: boolean,
): ReplaceAllResult {
  const rows = saved.map((row) => ({ ...row }));
  const set: AssetItem[] = [],
    refused: ReplaceAllResult["refused"] = [];
  for (const item of items) {
    const { entry } = item;
    const reason = silenceRefusal(item);
    if (reason) refused.push({ item, reason });
    else if (entry.shared && !includeShared && !(item.row?.allow_shared ?? false)) {
      refused.push({
        item,
        reason: SHARED_SILENCE,
      });
    } else if (item.index >= 0) {
      rows[item.index] = {
        ...silencedRow(rows[item.index]),
        ...(entry.shared && includeShared ? { allow_shared: true } : {}),
      };
      set.push(item);
    } else if (rows.length >= MAX_REPLACEMENTS) {
      refused.push({
        item,
        reason: TOO_MANY,
      });
    } else {
      rows.push(newSilencedRow(item, Boolean(entry.shared && includeShared)));
      set.push(item);
    }
  }
  return { rows, set, refused };
}
