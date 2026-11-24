/** Extras → Objects: the saved rows (`customization.ability_objects`) and the page's pure
 * helpers. Rows are built field by field (the backend refuses unknown keys): never UI state in
 * a row, one row per target ignoring case, at most MAX_ABILITY_OBJECTS, and the key is left out
 * of the customization when the last row goes. A fit equal to the default (match, scale 1, no
 * turns, centred) is saved as `{}`. */
import type { Customization, Project } from "../models";
import { changeCustomization } from "../components/customization";
import {
  MAX_ABILITY_OBJECTS,
  MAX_FOLLOW_PARTS,
  OBJECT_MAX_QUARTER_TURNS,
  OBJECT_SCALE_MAX,
  OBJECT_SCALE_MIN,
  OBJECT_SOURCE_SUFFIXES,
  type AbilityObjectRow,
  type ObjectFit,
  type ObjectFollow,
  type ObjectTurns,
} from "./objectTypes";

const fold = (value: string) => value.toLowerCase();
const TURN_KEYS = ["roll", "tip", "turn"] as const;
export type TurnAxis = (typeof TURN_KEYS)[number];

/** The saved rows (an absent key or an emptied list are both "none"). */
export function savedObjectRows(project: Project): AbilityObjectRow[] {
  const rows = project.customization.ability_objects;
  return Array.isArray(rows) ? rows : [];
}
export const objectRowOf = (rows: AbilityObjectRow[], target: string) => rows.find((row) => fold(row.target) === fold(target)) ?? null;

/** A scale inside the saved limits, rounded to whole percent (so 100% is exactly 1). */
export function cleanScale(scale: number | undefined): number {
  if (scale === undefined || !Number.isFinite(scale)) return 1;
  const percent = Math.round(scale * 100);
  return Math.min(OBJECT_SCALE_MAX, Math.max(OBJECT_SCALE_MIN, percent / 100));
}
/** A quarter-turn count as 0–3 (wraps; anything not a number is 0). */
export function cleanTurn(value: number | undefined): number {
  if (value === undefined || !Number.isFinite(value)) return 0;
  const count = OBJECT_MAX_QUARTER_TURNS + 1;
  return ((Math.round(value) % count) + count) % count;
}
/** The fit without its default parts: `{}` for match, scale 1 and no turns. */
export function cleanFit(fit: ObjectFit | undefined): ObjectFit {
  const out: ObjectFit = {};
  if (fit?.size === "keep") out.size = "keep";
  const scale = cleanScale(fit?.scale);
  if (scale !== 1) out.scale = scale;
  const turns: ObjectTurns = {};
  for (const key of TURN_KEYS) {
    const value = cleanTurn(fit?.turns?.[key]);
    if (value) turns[key] = value;
  }
  if (Object.keys(turns).length) out.turns = turns;
  // Standing on the game's base is absent unless set (a fit without it keeps its bytes).
  if (fit?.on_base === true) out.on_base = true;
  return out;
}
export const isDefaultFit = (fit: ObjectFit | undefined) => Object.keys(cleanFit(fit)).length === 0;
/** The fit with one more quarter turn about `axis` (3 wraps to 0). */
export const turned = (fit: ObjectFit | undefined, axis: TurnAxis): ObjectFit => {
  const clean = cleanFit(fit);
  return cleanFit({ ...clean, turns: { ...clean.turns, [axis]: cleanTurn(clean.turns?.[axis]) + 1 } });
};
/** The fit without turns (Reset). */
export const unturned = (fit: ObjectFit | undefined): ObjectFit => cleanFit({ ...cleanFit(fit), turns: {} });
export const withSize = (fit: ObjectFit | undefined, size: "match" | "keep"): ObjectFit => cleanFit({ ...cleanFit(fit), size });
export const withScale = (fit: ObjectFit | undefined, scale: number): ObjectFit => cleanFit({ ...cleanFit(fit), scale });
/** Stands on the game object's base (true) or centred on it (false: the key goes). */
export const withBase = (fit: ObjectFit | undefined, onBase: boolean): ObjectFit => cleanFit({ ...cleanFit(fit), on_base: onBase });
/** The size slider's percent for a saved scale. */
export const percentOf = (fit: ObjectFit | undefined) => Math.round(cleanScale(fit?.scale) * 100);

/** One row as the backend takes it: exactly these keys. */
function row(target: string, enabled: boolean, source: string, fit: ObjectFit | undefined, follow?: ObjectFollow): AbilityObjectRow {
  const clean = cleanFollow(follow);
  return clean ? { target, enabled, source, fit: cleanFit(fit), follow: clean } : { target, enabled, source, fit: cleanFit(fit) };
}
/** A follow setting as the backend takes it (exactly its keys; the bone only in one piece, parts only by parts). */
export function cleanFollow(follow: ObjectFollow | undefined): ObjectFollow | undefined {
  if (!follow || !["one_piece", "by_parts", "same_skeleton"].includes(follow.way)) return undefined;
  if (follow.way === "one_piece") return follow.bone ? { way: "one_piece", bone: follow.bone } : { way: "one_piece" };
  if (follow.way === "by_parts") {
    const parts = Object.fromEntries(Object.entries(follow.parts ?? {}).filter(([k, v]) => k && v).slice(0, MAX_FOLLOW_PARTS).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)));
    return Object.keys(parts).length ? { way: "by_parts", parts } : { way: "by_parts" };
  }
  return { way: "same_skeleton" };
}
/** The rows with `target`'s follow setting set (nothing happens without a row). */
export function withFollow(rows: AbilityObjectRow[], target: string, follow: ObjectFollow): AbilityObjectRow[] {
  return rows.map((r) => (fold(r.target) === fold(target) ? row(r.target, r.enabled !== false, r.source ?? "", r.fit, follow) : r));
}
/** The follow setting with one part following `bone` (the way becomes by parts). */
export function withPartBone(follow: ObjectFollow | undefined, part: string, bone: string): ObjectFollow {
  return { way: "by_parts", parts: { ...(follow?.way === "by_parts" ? follow.parts : {}), [part]: bone } };
}
/** Is this a file an object can come from (.blend, .fbx, .glb, .gltf)? */
export function acceptedSource(path: string): boolean {
  const dot = path.lastIndexOf(".");
  return dot > 0 && (OBJECT_SOURCE_SUFFIXES as readonly string[]).includes(fold(path.slice(dot + 1)));
}
/** The rows with `source` as `target`'s file. A new row is `{target, enabled: true, source, fit:
 * {}}` (refused, rows unchanged, when MAX_ABILITY_OBJECTS are saved); an existing row keeps its
 * own target spelling, its fit and whether it is included. */
export function withSource(rows: AbilityObjectRow[], target: string, source: string): AbilityObjectRow[] {
  const at = rows.findIndex((r) => fold(r.target) === fold(target));
  if (at < 0) return rows.length >= MAX_ABILITY_OBJECTS ? rows : [...rows, row(target, true, source, {})];
  const old = rows[at];
  return rows.map((r, i) => (i === at ? row(old.target, old.enabled !== false, source, old.fit, old.follow) : r));
}
/** The rows with `target`'s fit set (nothing happens without a row). */
export function withFit(rows: AbilityObjectRow[], target: string, fit: ObjectFit): AbilityObjectRow[] {
  return rows.map((r) => (fold(r.target) === fold(target) ? row(r.target, r.enabled !== false, r.source ?? "", fit, r.follow) : r));
}
export function withEnabled(rows: AbilityObjectRow[], target: string, enabled: boolean): AbilityObjectRow[] {
  return rows.map((r) => (fold(r.target) === fold(target) ? row(r.target, enabled, r.source ?? "", r.fit, r.follow) : r));
}
export const withoutRow = (rows: AbilityObjectRow[], target: string) => rows.filter((r) => fold(r.target) !== fold(target));
/** Pause all extras (false) / Resume all extras (true). */
export const pausedObjectRows = (rows: AbilityObjectRow[], enabled: boolean) => rows.map((r) => row(r.target, enabled, r.source ?? "", r.fit));
/** The project with these rows; the key is left out when there are none. */
export function withObjectRows(project: Project, rows: AbilityObjectRow[], activate: boolean): Project {
  const next = changeCustomization(project, { ability_objects: rows }, activate);
  if (rows.length) return next;
  const { ability_objects: _drop, ...customization } = next.customization;
  return { ...next, customization: customization as Customization };
}
/** Counts for the Extras page header, tab dot and Pause all. */
export function objectCounts(project: Project) {
  const rows = savedObjectRows(project);
  return { saved: rows.length, enabled: rows.filter((r) => r.enabled !== false).length };
}
/** A row's badge: Replaced (included, with a file), Paused (saved but not included, or
 * customization off), or nothing. */
export function objectStatus(saved: AbilityObjectRow | null, active: boolean): "Replaced" | "Paused" | null {
  if (!saved) return null;
  if (!active || saved.enabled === false) return "Paused";
  return saved.source ? "Replaced" : null;
}
