/** Extras → Effects: the saved effect edits (`customization.effect_edits`) and the page's
 * pure helpers (filters, groups, strips, multiplier texts). No colour maths: what an edit
 * does to a colour comes from Rust through `api.effectPreview`. */
// i18n-area: effects
import type { Customization, Project } from "../models";
import { m, mc, t, tc, tp } from "../i18n";
import { changeCustomization } from "../components/customization";
import type {
  EffectEdit,
  EffectEditRow,
  EffectMultiplier,
  EffectPart,
  EffectPartSetting,
  EffectSiteCounts,
  EffectStop,
  EffectStopPreview,
  EffectStripKind,
  EffectSwatch,
  EffectTag,
  EffectTree,
  EffectTrees,
  Rgb,
} from "./effectTypes";
import { LIGHT_MAX, LIGHT_MIN } from "./effectTypes";

/** Limits the backend accepts (effectTypes `EffectEditRow`). */
export const MAX_ROWS = 256;
export const MAX_COLORS = 64;
export const MULTIPLIER_MIN = 0.1;
export const MULTIPLIER_MAX = 4;
export const FACTOR_MAX = 4;

const EPS = 1e-6;
const fold = (value: string) => value.toLowerCase();
export const sameRgb = (a: Rgb | null | undefined, b: Rgb | null | undefined) =>
  Boolean(a && b && a[0] === b[0] && a[1] === b[1] && a[2] === b[2]);
const neutral = (value: number | undefined, base: number) => value === undefined || Math.abs(value - base) < EPS;
/** A hue shift that turns nothing (0 or a whole turn). */
const noTurn = (hue: number | undefined) => hue === undefined || Math.abs(((hue % 360) + 360) % 360) < EPS || Math.abs((((hue % 360) + 360) % 360) - 360) < EPS;

export const MAX_EXCLUDED = 256;
const clamp = (value: number, min: number, max: number) => Math.min(max, Math.max(min, value));
/** A hue shift as one turn, -180 (exclusive) to 180. */
export function normalHue(hue: number) {
  const h = ((((hue + 180) % 360) + 360) % 360) - 180;
  return h === -180 ? 180 : h;
}
/** The edit without its neutral fields: what is saved and sent. */
export function cleanEdit(edit: EffectEdit): EffectEdit {
  const out: EffectEdit = {};
  // Within the saved limits: hue as one turn (-180, 180], factors 0–4, multipliers above 0 and at most 16.
  if (!noTurn(edit.hue)) out.hue = normalHue(edit.hue!);
  if (!neutral(edit.saturation, 1)) out.saturation = clamp(edit.saturation!, 0, FACTOR_MAX);
  if (!neutral(edit.brightness, 1)) out.brightness = clamp(edit.brightness!, 0, FACTOR_MAX);
  const seen = new Set<string>();
  const excluded = (edit.excluded ?? []).filter((path) => !seen.has(fold(path)) && Boolean(seen.add(fold(path)))).slice(0, MAX_EXCLUDED);
  if (excluded.length) out.excluded = excluded;
  // Parts used by other heroes the user ticked in (left out by default); `excluded` wins.
  const sharedSeen = new Set<string>(seen);
  const shared = (edit.shared ?? []).filter((path) => !sharedSeen.has(fold(path)) && Boolean(sharedSeen.add(fold(path)))).slice(0, MAX_EXCLUDED);
  if (shared.length) out.shared = shared;
  const colors = [...new Map((edit.colors ?? []).map((c) => [c.from.join(","), c])).values()].slice(0, MAX_COLORS);
  if (colors.length)
    out.colors = colors.map((c) => ({ from: [...c.from] as Rgb, to: [...c.to] as Rgb, ...(c.near ? { near: true } : {}) }));
  for (const key of ["size", "glow", "opacity"] as const) if (!neutral(edit[key], 1)) out[key] = clamp(edit[key]!, 0.01, 16);
  const parts = cleanParts(edit.parts);
  if (parts) out.parts = parts;
  return out;
}
/** One part's settings without what changes nothing: hidden only when true, factors other
 * than 1 within the saved limits (above 0, at most 16), a tint as three whole 0–255 numbers. */
export function cleanPartSetting(setting: EffectPartSetting | undefined): EffectPartSetting | null {
  if (!setting) return null;
  const out: EffectPartSetting = {};
  if (setting.hidden) out.hidden = true;
  for (const key of ["size", "opacity"] as const) if (!neutral(setting[key], 1)) out[key] = clamp(setting[key]!, 0.01, 16);
  if (setting.tint) out.tint = setting.tint.map((v) => Math.round(clamp(v, 0, 255))) as Rgb;
  // Light brightness: 0 to 50 (0 is a real value: it dims the light).
  if (typeof setting.light === "number" && Number.isFinite(setting.light) && !neutral(setting.light, 1)) out.light = clamp(setting.light, LIGHT_MIN, LIGHT_MAX);
  return Object.keys(out).length ? out : null;
}
/** The per-part map: one entry per part (case-blind, the first kept), neutral entries
 * dropped, keys in a stable order, at most `MAX_EXCLUDED`; undefined when empty. */
function cleanParts(parts: Record<string, EffectPartSetting> | undefined): Record<string, EffectPartSetting> | undefined {
  if (!parts) return undefined;
  const seen = new Set<string>();
  const kept: [string, EffectPartSetting][] = [];
  for (const [path, setting] of Object.entries(parts)) {
    if (seen.has(fold(path))) continue;
    seen.add(fold(path));
    const clean = cleanPartSetting(setting);
    if (clean) kept.push([path, clean]);
  }
  kept.sort((a, b) => (fold(a[0]) < fold(b[0]) ? -1 : fold(a[0]) > fold(b[0]) ? 1 : 0));
  return kept.length ? Object.fromEntries(kept.slice(0, MAX_EXCLUDED)) : undefined;
}
/** A part's own settings in the edit (case-blind), or an empty object. */
export function partSetting(edit: EffectEdit, path: string): EffectPartSetting {
  const key = Object.keys(edit.parts ?? {}).find((p) => fold(p) === fold(path));
  return key ? edit.parts![key] : {};
}
/** The edit with a part's settings changed (`patch` fields set; `undefined` removes one) or, with `null`, removed. */
export function withPartSetting(edit: EffectEdit, path: string, patch: Partial<Record<keyof EffectPartSetting, EffectPartSetting[keyof EffectPartSetting] | undefined>> | null): EffectEdit {
  const rest = Object.fromEntries(Object.entries(edit.parts ?? {}).filter(([p]) => fold(p) !== fold(path)));
  const merged = patch === null ? null : cleanPartSetting({ ...partSetting(edit, path), ...patch } as EffectPartSetting);
  const parts = merged ? { ...rest, [path]: merged } : rest;
  if (merged && Object.keys(parts).length > MAX_EXCLUDED) return edit;
  return { ...edit, parts };
}
/** Parts of the tree this edit hides (left-out parts get no setting, so they are not counted). */
export const hiddenParts = (tree: EffectTree, edit: EffectEdit) =>
  tree.parts.filter((p) => p.controls?.hide.offered && partSetting(edit, p.path).hidden && leftOut(edit, p) === null);
/** The whole-effect shift (hue, saturation, brightness) is not neutral. */
export const hasShift = (edit: EffectEdit) => !noTurn(edit.hue) || !neutral(edit.saturation, 1) || !neutral(edit.brightness, 1);
/** Does the edit change anything in the game's files? Parts left out and a chosen main
 * colour change nothing by themselves; a replacement by the same colour only matters
 * when a shift would otherwise move that colour. */
export function changesSomething(edit: EffectEdit): boolean {
  const shift = hasShift(edit);
  if (shift) return true;
  if ((edit.colors ?? []).some((c) => !sameRgb(c.from, c.to))) return true;
  if (Object.values(edit.parts ?? {}).some((s) => cleanPartSetting(s) !== null)) return true;
  return (["size", "glow", "opacity"] as const).some((key) => !neutral(edit[key], 1));
}
/** What a preview answer stands for: the effect, the edit and the chosen main colour. */
export const editKey = (target: string, edit: EffectEdit, main?: Rgb | null) => `${fold(target)}\n${JSON.stringify(cleanEdit(edit))}\n${main ? main.join(",") : ""}`;

export function savedRows(project: Project): EffectEditRow[] {
  const rows = project.customization.effect_edits;
  return Array.isArray(rows) ? rows : [];
}
export const rowOf = (rows: EffectEditRow[], target: string) => rows.find((row) => fold(row.target) === fold(target)) ?? null;
export const rowActive = (row: EffectEditRow | null, active: boolean) => Boolean(row && active && row.enabled !== false);

/** The rows with `target`'s edit set: kept (cleaned, `enabled` and order kept) when it
 * changes something, removed otherwise. `main` is stored only with a kept row. */
export function withEdit(rows: EffectEditRow[], target: string, edit: EffectEdit, main?: Rgb | null): EffectEditRow[] {
  const at = rows.findIndex((row) => fold(row.target) === fold(target));
  if (!changesSomething(edit)) return at < 0 ? rows : rows.filter((_, i) => i !== at);
  const old = at < 0 ? null : rows[at];
  const row: EffectEditRow = {
    target: old?.target ?? target,
    ...(old?.enabled === false ? { enabled: false } : {}),
    edit: cleanEdit(edit),
    ...(main ? { main: [...main] as Rgb } : {}),
  };
  if (at < 0) return rows.length >= MAX_ROWS ? rows : [...rows, row];
  return rows.map((r, i) => (i === at ? row : r));
}
export const withEnabled = (rows: EffectEditRow[], target: string, enabled: boolean) =>
  rows.map((row) => (fold(row.target) === fold(target) ? (enabled ? withoutEnabled(row) : { ...row, enabled: false }) : row));
const withoutEnabled = ({ enabled: _drop, ...row }: EffectEditRow) => row;
export const withoutRow = (rows: EffectEditRow[], target: string) => rows.filter((row) => fold(row.target) !== fold(target));
/** Pause all extras / Resume all extras. */
export const pausedRows = (rows: EffectEditRow[], enabled: boolean) => rows.map((row) => (enabled ? withoutEnabled(row) : { ...row, enabled: false }));
/** The project with these rows; the key is left out when there are none. */
export function withRows(project: Project, rows: EffectEditRow[], activate: boolean): Project {
  const next = changeCustomization(project, { effect_edits: rows }, activate);
  if (rows.length) return next;
  const { effect_edits: _drop, ...customization } = next.customization;
  return { ...next, customization: customization as Customization };
}
/** Counts for the Extras page header, tab dot and Pause all. */
export function effectEditCounts(project: Project) {
  const rows = savedRows(project);
  return { saved: rows.length, enabled: rows.filter((row) => row.enabled !== false).length };
}

/** Advanced settings Simple does not show: parts left out, shared parts ticked in, single
 * colours changed. Shared parts left out by default are not settings. */
export function advancedSettings(edit: EffectEdit) {
  const excluded = new Set((edit.excluded ?? []).map(fold));
  const own = Object.values(edit.parts ?? {}).map(cleanPartSetting).filter((s): s is EffectPartSetting => s !== null);
  return {
    excluded: excluded.size,
    shared: (edit.shared ?? []).filter((p) => !excluded.has(fold(p))).length,
    colors: edit.colors?.length ?? 0,
    hidden: own.filter((s) => s.hidden).length,
    adjusted: own.filter((s) => s.size !== undefined || s.opacity !== undefined || s.tint || s.light !== undefined).length,
  };
}
/** Items joined as a list: "a, b and c" (each language its own separators). */
export function joinAnd(items: string[]): string {
  if (items.length < 2) return items[0] ?? "";
  return items.slice(0, -1).join(tc("list", ", ")) + tc("list", " and ") + items[items.length - 1];
}
/** One item of the list each is a whole phrase with its count; the list is joined in the sentence. */
export function advancedText(edit: EffectEdit): string {
  const { excluded, shared, colors, hidden, adjusted } = advancedSettings(edit);
  const parts = [
    excluded ? tp(excluded, { one: "{count} part left out", other: "{count} parts left out" }) : "",
    shared ? tp(shared, { one: "{count} part shared with other heroes included", other: "{count} parts shared with other heroes included" }) : "",
    colors ? tp(colors, { one: "{count} single color changed", other: "{count} single colors changed" }) : "",
    hidden ? tp(hidden, { one: "{count} part hidden", other: "{count} parts hidden" }) : "",
    adjusted ? tp(adjusted, { one: "{count} part has its own settings", other: "{count} parts have their own settings" }) : "",
  ].filter(Boolean);
  return parts.length ? t("Advanced settings are active: {list}.", { list: parts.join(tc("list", ", ")) }) : "";
}
/** Why a part keeps the game's look: the user unticked it, or other heroes use it and it was
 * not ticked in (`shared` parts are left out unless listed in the edit's `shared`). */
export function leftOut(edit: EffectEdit, part: Pick<EffectPart, "path" | "outside_hero">): "user" | "shared" | null {
  if (isExcluded(edit, part.path)) return "user";
  if (part.outside_hero && !(edit.shared ?? []).some((p) => fold(p) === fold(part.path))) return "shared";
  return null;
}
/** Tick (`on`) or untick a part: a hero's own part through `excluded`, a shared part through `shared`. */
export function withPart(edit: EffectEdit, part: Pick<EffectPart, "path" | "outside_hero">, on: boolean): EffectEdit {
  const next = withExcluded(edit, part.path, part.outside_hero ? false : !on);
  if (!part.outside_hero) return next;
  const rest = (edit.shared ?? []).filter((p) => fold(p) !== fold(part.path));
  return { ...next, shared: on ? [...rest, part.path] : rest };
}
/** Shared parts of a tree that keep the game's look because nothing ticked them in. */
export const sharedLeftOut = (tree: EffectTree, edit: EffectEdit) => tree.parts.filter((p) => p.visible && leftOut(edit, p) === "shared");
/** How "Also used by" relates to the edit: users that see the change, only through shared
 * parts left out (they keep the game's look), or a mix (or not known). */
export function alsoUsedReach(tree: EffectTree, edit: EffectEdit): "shows" | "kept" | "partly" {
  const users = [...(tree.also_used_by?.records ?? []), ...(tree.also_used_by?.effects ?? [])];
  const kept = new Set(sharedLeftOut(tree, edit).map((p) => fold(p.path)));
  if (!users.length || !kept.size) return "shows";
  const through = users.map((u) => u.parts.length > 0 && u.parts.every((p) => kept.has(fold(p))));
  if (through.every(Boolean)) return "kept";
  return users.some((u) => u.parts.some((p) => kept.has(fold(p)))) || users.some((u) => !u.parts.length) ? "partly" : "shows";
}
export function withExcluded(edit: EffectEdit, path: string, out: boolean): EffectEdit {
  const rest = (edit.excluded ?? []).filter((p) => fold(p) !== fold(path));
  return { ...edit, excluded: out ? [...rest, path] : rest };
}
export const isExcluded = (edit: EffectEdit, path: string) => (edit.excluded ?? []).some((p) => fold(p) === fold(path));
export const replacementOf = (edit: EffectEdit, from: Rgb) => (edit.colors ?? []).find((c) => sameRgb(c.from, from)) ?? null;
/** Sets (or, with `to` null, removes) the replacement of one palette colour. */
export function withColor(edit: EffectEdit, from: Rgb, to: Rgb | null, near?: boolean): EffectEdit {
  const old = replacementOf(edit, from);
  const rest = (edit.colors ?? []).filter((c) => !sameRgb(c.from, from));
  if (!to) return { ...edit, colors: rest };
  if (!old && rest.length >= MAX_COLORS) return edit;
  const keepNear = near ?? old?.near ?? false;
  const entry = { from: [...from] as Rgb, to: [...to] as Rgb, ...(keepNear ? { near: true } : {}) };
  return { ...edit, colors: old ? (edit.colors ?? []).map((c) => (sameRgb(c.from, from) ? entry : c)) : [...rest, entry] };
}
/** The pick's shift merged into the latest edit (nothing else of the answer is taken). */
export const withPick = (edit: EffectEdit, pick: { hue: number; saturation: number; brightness: number }): EffectEdit => ({
  ...edit,
  hue: pick.hue,
  saturation: pick.saturation,
  brightness: pick.brightness,
});
export const withoutShift = (edit: EffectEdit): EffectEdit => {
  const { hue: _h, saturation: _s, brightness: _b, ...rest } = edit;
  return rest;
};

// ── The list ────────────────────────────────────────────────────────────
export const TAGS: { key: EffectTag; label: string }[] = [
  { key: "ability", label: mc("effect tag", "Ability") },
  { key: "weapon", label: mc("effect tag", "Weapon") },
  { key: "hero", label: mc("effect tag", "Hero") },
];
/** A tag's name as shown. */
export const tagName = (label: string) => tc("effect tag", label);
export interface TreeFilter {
  query: string;
  tags: EffectTag[];
  changedOnly: boolean;
}
const searchText = (tree: EffectTree) => fold(`${tree.label} ${tree.name} ${tree.group_name} ${tree.target}`);
export function matches(tree: EffectTree, filter: TreeFilter, rows: EffectEditRow[]) {
  const words = fold(filter.query).split(/\s+/).filter(Boolean);
  const text = searchText(tree);
  return (
    words.every((word) => text.includes(word)) &&
    filter.tags.every((tag) => tree.tags.includes(tag)) &&
    (!filter.changedOnly || rowOf(rows, tree.target) !== null)
  );
}
/** How many trees each tag would show with the other filters as they are. */
export function tagCounts(trees: EffectTree[], filter: TreeFilter, rows: EffectEditRow[]) {
  const base = trees.filter((tree) => matches(tree, { ...filter, tags: [] }, rows));
  return new Map(TAGS.map(({ key }) => [key, base.filter((tree) => filter.tags.every((tag) => tree.tags.includes(tag)) && tree.tags.includes(key)).length]));
}
export interface TreeGroup {
  key: string;
  name: string;
  trees: EffectTree[];
  note?: string;
  folded?: boolean;
  /** Every effect of the group is in it by its file name only. */
  byName?: boolean;
}
/** The quiet line for an effect (or a whole group) placed by its file name only. */
export const BY_NAME = m("Grouped by its file name");
export const BY_NAME_GROUP = m("Grouped by their file names");
/** The name of the group for trees in no known group (the others come from Rust as they are). */
export const OTHER_EFFECTS = m("Other effects");
export const byFileName = (tree: Pick<EffectTree, "grouped_by">) => tree.grouped_by === "name";
export const isFiltering = (filter: TreeFilter) => Boolean(fold(filter.query).trim() || filter.tags.length || filter.changedOnly);
/** The visible trees under their groups, in the groups' order (unknown groups last).
 * Without a filter, an ability group with no tree of its own stays, for its note. */
export function groupTrees(data: EffectTrees, visible: EffectTree[], filtering = true): TreeGroup[] {
  const groups = [...data.groups].sort((a, b) => a.order - b.order);
  const known = new Set(groups.map((g) => g.key));
  const out: (TreeGroup & { empty: boolean })[] = groups.map((g) => {
    const own = data.trees.filter((tree) => tree.group === g.key);
    return {
      key: g.key,
      name: g.record_name && g.record_name !== g.name ? `${g.name} · ${g.record_name}` : g.name,
      trees: visible.filter((tree) => tree.group === g.key),
      ...(g.note ? { note: g.note } : {}),
      ...(g.folded ? { folded: true } : {}),
      ...(own.length > 0 && own.every(byFileName) ? { byName: true } : {}),
      empty: g.trees === 0 && Boolean(g.note),
    };
  });
  const strays = visible.filter((tree) => !known.has(tree.group));
  if (strays.length) out.push({ key: "", name: OTHER_EFFECTS, trees: strays, empty: false });
  return out.filter((g) => g.trees.length || (g.empty && !filtering)).map(({ empty: _, ...g }) => g);
}
export interface ShownGroup extends TreeGroup {
  /** Trees of a folded group that are not shown now. */
  hidden: number;
  /** A folded group that shows all its trees (opened, or a filter reaches into it). */
  open: boolean;
}
/** What the list shows of each group. A folded group shows only effects the user edited
 * and the selected one until it is opened; while a filter is on it shows every match. */
export function shownGroups(groups: TreeGroup[], opened: ReadonlySet<string>, filtering: boolean, rows: EffectEditRow[], selected: string): ShownGroup[] {
  const out = groups.map((g) => {
    if (!g.folded || filtering || opened.has(g.key)) return { ...g, hidden: 0, open: true };
    const trees = g.trees.filter((tree) => rowOf(rows, tree.target) !== null || fold(tree.target) === fold(selected));
    return { ...g, trees, hidden: g.trees.length - trees.length, open: false };
  });
  // A hero whose only effects are folded: show them, so one is selected and editable.
  if (!out.some((g) => g.trees.length)) return groups.map((g) => ({ ...g, hidden: 0, open: true }));
  return out;
}
/** A row's small swatch: the effect's strongest colours (colourful first). */
export function treeSwatches(tree: EffectTree, count = 3): EffectSwatch[] {
  const colorful = tree.palette.filter((s) => !s.neutral);
  return (colorful.length ? colorful : tree.palette).slice(0, count);
}
/** The parts of a tree (for "also part of"), by path. */
export function treeNames(data: EffectTrees | null) {
  return new Map((data?.trees ?? []).map((tree) => [fold(tree.target), tree.name || tree.label]));
}
/** Other effects of this hero that hold this part (`in_trees` without the tree itself). */
export function alsoPartOf(names: Map<string, string>, tree: EffectTree, inTrees: string[]) {
  return inTrees.filter((other) => fold(other) !== fold(tree.target)).map((other) => names.get(fold(other)) ?? other.split("/").pop()!.replace(/\.vpcf_c$/, ""));
}
/** The main colour's palette index: the user's choice when it is in the palette, else the detected one. */
export function mainIndex(tree: EffectTree, chosen: Rgb | null | undefined): number | null {
  if (chosen) {
    const at = tree.palette.findIndex((s) => sameRgb(s.rgb, chosen) || s.members.some((m) => sameRgb(m, chosen)));
    if (at >= 0) return at;
  }
  return tree.main ? tree.main.swatch : null;
}

// ── Colour strips ───────────────────────────────────────────────────────
export interface StripStop {
  position: number;
  rgb: Rgb;
  hex: string;
  alpha?: number;
}
export const gameStops = (stops: EffectStop[]): StripStop[] => stops.map((s) => ({ position: s.position, rgb: s.rgb, hex: s.hex, alpha: s.alpha }));
export const afterStops = (stops: EffectStopPreview[]): StripStop[] => stops.map((s) => ({ position: s.position, rgb: s.after, hex: s.after_hex, alpha: s.alpha }));
const css = (s: StripStop) => {
  const a = s.alpha === undefined ? 1 : Math.max(0, Math.min(1, s.alpha / 255));
  return a >= 1 ? `rgb(${s.rgb.join(" ")})` : `rgb(${s.rgb.join(" ")} / ${Math.round(a * 1000) / 1000})`;
};
const pct = (value: number) => `${Math.round(Math.max(0, Math.min(1, value)) * 10000) / 100}%`;
/** CSS background of a strip: one colour, two halves (random between two), or stops at their positions. */
export function stripBackground(kind: EffectStripKind, stops: StripStop[]): string {
  if (!stops.length) return "transparent";
  if (stops.length === 1) return `linear-gradient(90deg, ${css(stops[0])}, ${css(stops[0])})`;
  if (kind === "random") {
    const share = 100 / stops.length;
    return `linear-gradient(90deg, ${stops.map((s, i) => `${css(s)} ${Math.round(i * share * 100) / 100}% ${Math.round((i + 1) * share * 100) / 100}%`).join(", ")})`;
  }
  const sorted = [...stops].sort((a, b) => a.position - b.position);
  return `linear-gradient(90deg, ${sorted.map((s) => `${css(s)} ${pct(s.position)}`).join(", ")})`;
}
const colorText = (s: StripStop) => (s.alpha !== undefined && s.alpha < 255 ? t("{hex} ({percent}% opaque)", { hex: s.hex, percent: Math.round((s.alpha / 255) * 100) }) : s.hex);
/** The strip in words (its accessible name and tooltip). */
export function stripText(kind: EffectStripKind, stops: StripStop[]): string {
  if (!stops.length) return t("no color");
  if (kind === "random") return t("random between {colors}", { colors: stops.map(colorText).join(tc("list", " and ")) });
  if (stops.length === 1) return colorText(stops[0]);
  return [...stops].sort((a, b) => a.position - b.position).map((s) => t("{color} at {percent}%", { color: colorText(s), percent: Math.round(s.position * 100) })).join(tc("list", ", "));
}
export const KIND_TEXT: Record<EffectStripKind, string> = {
  constant: m("one color"),
  random: m("two colors, random"),
  fade: m("fades from its starting color"),
  gradient: m("color over its life"),
};

// ── Size, Brightness, Opacity ───────────────────────────────────────────
export const MULTIPLIERS: { key: EffectMultiplier; label: string }[] = [
  { key: "size", label: m("Size") },
  { key: "glow", label: m("Brightness") },
  { key: "opacity", label: m("Opacity") },
];
export function reachText(counts: EffectSiteCounts | undefined): string {
  if (!counts) return "";
  return tp(counts.visible_parts, { one: "reaches {reached} of {count} part", other: "reaches {reached} of {count} parts" }, { reached: counts.parts_reached });
}
/** Advanced, under Size: the 3D parts that have a Size of their own (empty when none). */
export function ownSizeText(tree: EffectTree): string {
  const n = tree.parts.filter((p) => p.visible && p.controls?.size.offered).length;
  return n === 0 ? "" : n === 1 ? t("the 3D part has its own Size") : tp(n, { one: "the {count} 3D part has its own Size", other: "the {count} 3D parts have their own Size" });
}
/** Why a multiplier reaches no part, in plain words (null when it reaches one). */
export function unreachedReason(key: EffectMultiplier, counts: EffectSiteCounts | undefined): string | null {
  if (counts && counts.parts_reached > 0) return null;
  if (!counts || counts.visible_parts === 0) return t("This effect draws nothing this could change.");
  if (counts.game_input && !counts.no_setting)
    return key === "size" ? t("The game sets its size while the effect plays.") : key === "glow" ? t("The game sets its brightness while the effect plays.") : t("The game sets its opacity while the effect plays.");
  return key === "size"
    ? t("Its parts are 3D models, ground decals or lights, which have no size setting.")
    : key === "glow"
      ? t("Its parts are 3D models, ground decals or lights, which have no brightness setting.")
      : t("Its parts are 3D models, ground decals or lights, which have no opacity setting.");
}
// Why a slider does not reach a part, by the kind of part (whole phrases, kept English as keys).
const KIND_WORDS: [RegExp, string][] = [
  [/RenderModels/i, m("a 3D model has no such setting")],
  [/RenderProjected|Decal/i, m("a ground decal has no such setting")],
  [/Light/i, m("a light has no such setting")],
];
const OTHER_KIND = m("this kind of part has no such setting");
const GAME_SETS = m("the game sets it while the effect plays");
/** Why Size, Brightness or Opacity do not reach a part, in plain words (empty when all reach it). */
export function partUnreached(part: Pick<EffectPart, "visible" | "renderers" | "multipliers">): string[] {
  if (!part.visible) return [];
  const notes = new Map<string, string[]>();
  for (const { key, label } of MULTIPLIERS) {
    const sites = part.multipliers.filter((m) => m.multiplier === key);
    if (sites.some((m) => m.status === "changes" || m.status === "adds")) continue;
    const kind = KIND_WORDS.find(([re]) => part.renderers.some((r) => re.test(r)))?.[1] ?? OTHER_KIND;
    const why = sites.some((site) => site.status === "game_input") ? GAME_SETS : kind;
    notes.set(why, [...(notes.get(why) ?? []), label]);
  }
  return [...notes].map(([why, labels]) =>
    tp(labels.length, { one: "{labels} does not reach it: {why}.", other: "{labels} do not reach it: {why}." }, { labels: joinAnd(labels.map((label) => t(label))), why: t(why) }));
}

// ── Main colour presets ─────────────────────────────────────────────────
export const PRESETS: { name: string; rgb: Rgb }[] = [
  { name: m("Red"), rgb: [255, 69, 58] },
  { name: m("Orange"), rgb: [255, 149, 46] },
  { name: m("Yellow"), rgb: [255, 214, 64] },
  { name: m("Green"), rgb: [88, 214, 104] },
  { name: m("Teal"), rgb: [46, 204, 185] },
  { name: m("Blue"), rgb: [64, 140, 255] },
  { name: m("Neon purple"), rgb: [125, 77, 255] },
  { name: m("Pink"), rgb: [255, 95, 176] },
  { name: m("White"), rgb: [255, 255, 255] },
];
export const hexOf = (rgb: Rgb) => "#" + rgb.map((v) => Math.round(v).toString(16).padStart(2, "0")).join("").toUpperCase();
/** The colour pickers work in 0–1, the effect data in bytes. */
export const toUnit = (rgb: Rgb): [number, number, number] => [rgb[0] / 255, rgb[1] / 255, rgb[2] / 255];
export const toByte = (rgb: [number, number, number]): Rgb => rgb.map((v) => Math.round(Math.max(0, Math.min(1, v)) * 255)) as Rgb;

export const MODE_KEY = "dmb.effectsMode";
export type EffectsMode = "simple" | "advanced";
export function savedMode(): EffectsMode {
  try {
    return localStorage.getItem(MODE_KEY) === "advanced" ? "advanced" : "simple";
  } catch {
    return "simple";
  }
}
export function rememberMode(mode: EffectsMode) {
  try {
    localStorage.setItem(MODE_KEY, mode);
  } catch {
    /* storage unavailable: the choice lasts for this view only */
  }
}

/** Files that have a user replacement (Replace a file) and belong to an effect with a colour edit. */
export function replacementConflicts(project: Project, data: EffectTrees | null) {
  const replaced = new Map((project.customization.effects ?? []).filter((r) => r.target).map((r) => [fold(r.target!), r]));
  const rows = savedRows(project);
  const out: { path: string; part: string; tree: EffectTree }[] = [];
  if (!data || !replaced.size || !rows.length) return out;
  for (const tree of data.trees) {
    if (!rowOf(rows, tree.target)) continue;
    for (const part of tree.parts) if (replaced.has(fold(part.path))) out.push({ path: part.path, part: part.label, tree });
  }
  return out;
}
export const replacedPaths = (project: Project) => new Set((project.customization.effects ?? []).flatMap((r) => (r.target ? [fold(r.target)] : [])));
export const isReplaced = (set: Set<string>, path: string) => set.has(fold(path));
