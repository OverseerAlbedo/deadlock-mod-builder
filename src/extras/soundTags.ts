// Extras → Sounds: rows, filters and "Replace all" over the tags, names and
// variation families that Rust discovery computes (`sound_tags.rs`). No tag rule
// lives here; this file only groups, filters and counts what each row carries.
import type { CatalogEntry, SoundInfo } from "../components/AssetCatalog";
import type { ResourceReplacement } from "../models";
import { assetLabel, detailNumber, isSilenced, replacementStatus, type AssetItem } from "./models";
// A sound row can also silence its sound (`silence`, no file): `silencedRow`,
// `silenceAll`. It counts as a change everywhere a replacement does.

export const MAX_REPLACEMENTS = 128;
export type TagKey = "hero" | "ability" | "weapon" | "voice" | "other";
export const TAGS: { key: TagKey; label: string }[] = [
  { key: "hero", label: "Hero" },
  { key: "ability", label: "Ability" },
  { key: "weapon", label: "Weapon" },
  { key: "voice", label: "Voice" },
  { key: "other", label: "Other" },
];
export const VOICE_KINDS: { key: string; label: string }[] = [
  { key: "ping", label: "Pings" },
  { key: "emote", label: "Emotes and efforts" },
  { key: "ability", label: "Ability lines" },
  { key: "match_start", label: "Match start" },
  { key: "kill", label: "Kills" },
  { key: "select", label: "Hero select" },
  { key: "about", label: "Other heroes about this hero" },
];
const tagLabel = (key: string) => TAGS.find((t) => t.key === key)?.label ?? key;
const kindLabel = (key: string) =>
  VOICE_KINDS.find((k) => k.key === key)?.label ?? key;

/** Sound info of an entry; catalogs made before tagging get a plain fallback. */
export function soundOf(entry: CatalogEntry): SoundInfo {
  return (
    entry.sound ?? {
      name: assetLabel(entry),
      group: "other:",
      group_name: "Other sounds",
      group_order: 400,
      tags: [],
      family: entry.target || entry.path,
      family_source: "name",
    }
  );
}
/** Main tags plus `other` for a sound with none of ability, weapon or voice. */
export function tagsOf(info: SoundInfo): TagKey[] {
  const tags = info.tags.filter((t): t is TagKey =>
    ["hero", "ability", "weapon", "voice"].includes(t),
  );
  return tags.some((t) => t !== "hero") ? tags : [...tags, "other"];
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
      groupName: missing ? "Saved replacements to place again" : info.group_name,
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
function searchMatches(row: SoundRow, query: string) {
  const words = query.toLowerCase().replace(/[_\-/.]+/g, " ").split(/\s+/).filter(Boolean);
  return words.every((w) => row.search.includes(w));
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
      f.tags.every((t) => row.tags.includes(t)) &&
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
    for (const t of row.tags) tags.set(t, (tags.get(t) ?? 0) + 1);
    if (f.tags.every((t) => row.tags.includes(t)))
      for (const k of row.voice) voice.set(k, (voice.get(k) ?? 0) + 1);
  }
  return { tags, voice };
}
/** What the filter line says. */
export function shownText(count: number, files: number, f: SoundFilter) {
  const noun = count === 1 ? "sound" : "sounds";
  const parts: string[] = [];
  if (f.tags.length === 1) parts.push(`tagged ${tagLabel(f.tags[0])}`);
  else if (f.tags.length > 1) {
    const labels = f.tags.map(tagLabel);
    parts.push(
      `that are ${labels.length === 2 ? "both " : "all of "}${labels.slice(0, -1).join(", ")} and ${labels[labels.length - 1]}`,
    );
  }
  if (f.tags.includes("voice") && f.voice.length)
    parts.push(`(${f.voice.map(kindLabel).join(" or ")})`);
  if (f.replacedOnly) parts.push("with a replacement");
  if (f.query.trim()) parts.push(`matching “${f.query.trim()}”`);
  const variations =
    files > count ? ` · ${files.toLocaleString("en-US")} files with their variations` : "";
  return `Showing ${count.toLocaleString("en-US")} ${noun}${parts.length ? " " + parts.join(" ") : ""}${variations}`;
}
export function lengthText(items: AssetItem[]) {
  const values = items
    .map((i) => detailNumber(i.entry, "duration"))
    .filter((v): v is number => v !== null);
  if (!values.length) return "";
  const lo = Math.min(...values),
    hi = Math.max(...values),
    text = (v: number) => (v < 10 ? v.toFixed(1) : Math.round(v).toString());
  return text(lo) === text(hi) ? `${text(lo)} s` : `${text(lo)}–${text(hi)} s`;
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
        reason: `View only: ${(entry.reason || "this sound cannot be replaced yet").replace(/^Integrity check failed:\s*/, "")}`,
      });
    } else if (compiled) {
      refused.push({
        item,
        reason:
          "A compiled .vsnd_c file only fits the sound it was made for. Choose a WAV, MP3, FLAC or OGG file to replace every variation.",
      });
    } else if (entry.wav_supported === false) {
      refused.push({
        item,
        reason:
          "Its game format cannot take a converted file. Replace it on its own with a matching .vsnd_c.",
      });
    } else if (entry.shared && !includeShared && !(item.row?.allow_shared ?? false)) {
      refused.push({
        item,
        reason:
          "Shared with other heroes. Tick “Also replace the shared variations” to include it.",
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
        reason: `Sounds already have ${MAX_REPLACEMENTS} replacements. Remove one before adding another.`,
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
    return `View only: ${(item.entry.reason || "this sound cannot be changed yet").replace(/^Integrity check failed:\s*/, "")}`;
  if (item.entry.wav_supported === false)
    return "Its game format cannot be rewritten, so it cannot be silenced.";
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
        reason: "Shared with other heroes. Tick “Also silence the shared variations” to include it.",
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
        reason: `Sounds already have ${MAX_REPLACEMENTS} replacements. Remove one before adding another.`,
      });
    } else {
      rows.push(newSilencedRow(item, Boolean(entry.shared && includeShared)));
      set.push(item);
    }
  }
  return { rows, set, refused };
}
