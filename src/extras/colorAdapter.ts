/** Extras → Colors works on one list of materials in two modes, chosen by
 * `modelEnabled(project)`:
 * - game hero (no replacement model): the hero's own compiled materials from the
 *   catalog, saved as `customization.hero_materials` rows (Hero colors);
 * - replacement: the replacement model's materials (`project.materials`), saved as
 *   Surface changes rows (`customization.appearance.materials`), the same rows
 *   the Appearance page edits.
 * The page is written against `ColorAdapter` only. Every function here is pure:
 * an edit returns the next project (or null when nothing may change). */
import type { AppearanceMaterial, HeroColorRange, HeroMaterial, Project } from "../models";
import type { ComponentCatalog, HeroMaterialEntry } from "../components/AssetCatalog";
import { changeCustomization } from "../components/customization";
import { modelEnabled } from "../build/readiness";
import type { ExtraPreviewRequest } from "./models";

export type Rgb = [number, number, number];
export type ColorMode = "hero" | "replacement";
/** The color values a material can hold (the same names in both row types). */
export interface ColorValues {
  color?: Rgb;
  hue?: number;
  saturation?: number;
  brightness?: number;
  ranges?: HeroColorRange[];
}
export type ColorKey = keyof ColorValues;
export const COLOR_KEYS: ColorKey[] = ["color", "hue", "saturation", "brightness", "ranges"];
export interface ColorCapabilities { tint: boolean; hue: boolean; saturation: boolean; brightness: boolean; ranges: boolean }
export type ColorState = "changed" | "paused" | "shared" | "view-only" | "missing" | "original";
/** Where a material's base-color image comes from: the game (Extras preview) or a file on this PC. */
export type TextureSource =
  | { kind: "hero"; request: ExtraPreviewRequest; identity: string }
  | { kind: "file"; path: string }
  | null;
export interface ColorMaterial {
  /** Hero: the catalog `.vmat_c` target; replacement: the `project.materials` key. */
  id: string;
  label: string;
  group: string;
  texture: TextureSource;
  capabilities: ColorCapabilities;
  /** Why a capability is missing, or a caution about it; shown in its section. */
  notes: { tint?: string; whole?: string; ranges?: string };
  state: ColorState;
  /** The saved color values (null: none saved). */
  values: ColorValues | null;
  /** The row is included in the build (Include in build). */
  enabled: boolean;
  /** Changes can be made now (false: view only, missing, or the row limit is reached). */
  editable: boolean;
  /** Why it cannot be changed (view-only reason, the row limit). */
  blocked: string;
  /** The color the game (or the material) multiplies the base color by before any change. */
  originalTint: Rgb;
  /** The plain base color, for a material without a base-color image (replacement only). */
  baseColor: Rgb | null;
  missing: boolean;
  /** Other models using this material (hero: shared materials). */
  sharedWith: string[];
  allowShared: boolean;
  /** Label/value pairs for File details. */
  details: [string, string][];
  /** The hero catalog entry (game hero mode only). */
  entry?: HeroMaterialEntry;
  /** The preview only approximates the build (game shaders; plain colors and 16-bit or HDR images). */
  approximate: boolean;
  /** The row also holds other surface changes the page keeps untouched (replacement). */
  otherChanges: string[];
}
export interface ColorEdit extends ColorValues { enabled?: boolean; allow_shared?: boolean }
export interface ColorAdapter {
  mode: ColorMode;
  materials: ColorMaterial[];
  /** The rows are switched on (customization on and the section on). */
  active: boolean;
  /** At most this many rows. */
  limit: number;
  full: boolean;
  /** How many materials carry saved colors. */
  changed: number;
  /** Next project with these values set and `removed` keys cleared; null when nothing may change. */
  edit(id: string, changes: ColorEdit, removed?: ColorKey[]): Project | null;
  /** Next project with this material's colors removed (Remove color / Reset). */
  remove(id: string): Project;
  /** Next project with every material's colors removed. */
  resetAll(): Project;
}

export const RANGE_LIMIT = 8;
export const HERO_LIMIT = 64;
export const APPEARANCE_LIMIT = 256;
const fold = (value: string) => value.toLowerCase();
const WHITE: Rgb = [1, 1, 1];
export function stem(path: string) {
  return (path.split("/").pop() ?? path).replace(/_c$/, "").replace(/\.vmat$/, "");
}
/** Short labels of the models a shared hero material is also used by. */
export const usedByLabels = (paths: string[]) => paths.map((path) => stem(path.replace(/\.vmdl_c$/, "")));
const neutral = (values: ColorValues) =>
  !values.color && (values.hue ?? 0) === 0 && (values.saturation ?? 1) === 1 && (values.brightness ?? 1) === 1 && !values.ranges?.length;
/** Only the color keys of a row, without unset ones. */
export function colorValues(row: ColorValues | undefined | null): ColorValues | null {
  if (!row) return null;
  const out: ColorValues = {};
  for (const key of COLOR_KEYS) if (row[key] !== undefined && row[key] !== null) (out as Record<string, unknown>)[key] = row[key];
  return Object.keys(out).length ? out : null;
}
const pct = (value: number) => `${Math.round(value * 100)}%`;
/** What a material's saved colors change, for the list ("Tint · 1 color range"). */
export function changeSummary(values: ColorValues | null): string {
  if (!values) return "";
  const parts: string[] = [];
  if (values.color) parts.push("Tint");
  if ((values.hue ?? 0) !== 0) parts.push(`Hue ${values.hue! > 0 ? "+" : ""}${Math.round(values.hue!)}°`);
  if ((values.saturation ?? 1) !== 1) parts.push(`Saturation ${pct(values.saturation!)}`);
  if ((values.brightness ?? 1) !== 1) parts.push(`Brightness ${pct(values.brightness!)}`);
  const ranges = values.ranges?.length ?? 0;
  if (ranges) parts.push(`${ranges} color range${ranges === 1 ? "" : "s"}`);
  return parts.join(" · ") || "No change yet";
}

// ---------------------------------------------------------------- game hero

/** A hero material from the catalog with its saved color, if any. */
export interface HeroColorItem {
  entry: HeroMaterialEntry;
  index: number;
  row?: HeroMaterial;
  missing: boolean;
}
export function heroColorItems(entries: HeroMaterialEntry[], rows: HeroMaterial[]): HeroColorItem[] {
  const indexes = new Map<string, number>();
  rows.forEach((row, index) => {
    const key = fold(row.target ?? "");
    if (!indexes.has(key) || row.enabled !== false) indexes.set(key, index);
  });
  const known = new Set(entries.map((entry) => fold(entry.target)));
  return [
    ...entries.map((entry) => {
      const index = indexes.get(fold(entry.target)) ?? -1;
      return { entry, index, row: rows[index], missing: false };
    }),
    // Saved colors for materials the game no longer lists, so they can be removed.
    ...rows.flatMap((row, index) =>
      row.target && !known.has(fold(row.target))
        ? [{
            entry: { path: row.target.replace(/_c$/, ""), target: row.target, label: stem(row.target), shader: "", supported: false, reason: "This material is not in the loaded game files.", tint: null, albedo: null, tint_masked: false, color_texture: null, used_by: [], shared: false, original_sha256: "" },
            index, row, missing: true,
          }]
        : [],
    ),
  ];
}
/** The folder a hero material sits in (`…/haze/materials/x.vmat` → haze): its list group. */
export const heroGroup = (path: string) => path.split("/").slice(-3, -2)[0] ?? "";

export function heroColorAdapter(project: Project, catalog: ComponentCatalog | null, revision = 0): ColorAdapter {
  const colors = project.customization.hero_materials ?? {};
  const rows = colors.materials ?? [];
  const active = Boolean(project.customization.enabled) && colors.enabled !== false;
  const full = rows.length >= HERO_LIMIT;
  const items = heroColorItems(catalog?.materials ?? [], rows);
  const setRows = (materials: HeroMaterial[], activate = true) =>
    changeCustomization(project, { hero_materials: { ...colors, enabled: colors.enabled ?? true, materials } }, activate);
  const find = (id: string) => items.find((item) => fold(item.entry.target) === fold(id));
  const materials = items.map((item): ColorMaterial => {
    const { entry, row, missing } = item;
    const values = colorValues(row);
    const state: ColorState = row
      ? row.enabled === false || !active ? "paused" : "changed"
      : missing ? "missing" : !entry.supported ? "view-only" : entry.shared ? "shared" : "original";
    const editable = entry.supported && !missing && (Boolean(row) || !full);
    const reason = entry.reason.replace(/^Integrity check failed:\s*/, "");
    return {
      id: entry.target,
      label: entry.label,
      group: missing ? "Saved, not in the game" : heroGroup(entry.path) || "Materials",
      texture: entry.color_texture && !missing
        ? { kind: "hero", request: { hero_path: project.original_model, target: entry.target, kind: "materials", max_dimension: 512 }, identity: `${entry.original_sha256}:${revision}` }
        : null,
      capabilities: { tint: Boolean(entry.tint), hue: false, saturation: Boolean(entry.albedo), brightness: Boolean(entry.albedo), ranges: Boolean(entry.color_texture) },
      notes: {
        tint: entry.tint
          ? entry.tint_masked ? "The game limits this material's tint to some areas (its tint mask)." : "The tint covers this whole material, including any skin it holds; check the preview."
          : "This material has no tint setting.",
        whole: entry.albedo ? undefined : "This material has no saturation or brightness setting.",
      },
      state,
      values,
      enabled: row?.enabled !== false,
      editable,
      blocked: missing ? "This material is no longer in the game files" : !entry.supported ? reason : !editable ? `Hero colors hold at most ${HERO_LIMIT} materials. Remove one before adding another.` : "",
      originalTint: entry.tint ?? WHITE,
      baseColor: null,
      missing,
      sharedWith: entry.shared ? usedByLabels(entry.used_by) : [],
      allowShared: row?.allow_shared ?? false,
      details: [
        ["Game material", entry.target],
        ...(entry.color_texture ? [["Base-color texture", entry.color_texture] as [string, string]] : []),
        ...(entry.shader ? [["Shader", entry.shader] as [string, string]] : []),
        ...(entry.albedo ? [["Game saturation · brightness", `${pct(entry.albedo[1])} · ${pct(entry.albedo[2])}`] as [string, string]] : []),
      ],
      entry,
      approximate: true,
      otherChanges: [],
    };
  });
  return {
    mode: "hero",
    materials,
    active,
    limit: HERO_LIMIT,
    full,
    changed: rows.length,
    edit(id, changes, removed = []) {
      const item = find(id);
      if (!item) return null;
      const base: HeroMaterial = item.row ?? { enabled: true, target: item.entry.target, original_sha256: item.entry.original_sha256, allow_shared: false };
      const next: HeroMaterial = { ...base, ...changes };
      removed.forEach((key) => delete next[key]);
      if (next.ranges && !next.ranges.length) delete next.ranges;
      // A color that changes nothing is not kept (the build would refuse it);
      // color ranges keep it while they are being set up.
      const empty = neutral(next);
      if (item.index >= 0)
        return setRows(empty ? rows.filter((_, index) => index !== item.index) : rows.map((row, index) => (index === item.index ? next : row)), !empty);
      if (!empty && rows.length < HERO_LIMIT && item.entry.supported && !item.missing) return setRows([...rows, next]);
      return null;
    },
    remove(id) {
      const item = find(id);
      return item && item.index >= 0 ? setRows(rows.filter((_, i) => i !== item.index), false) : project;
    },
    resetAll() {
      return setRows([], false);
    },
  };
}

// -------------------------------------------------------------- replacement

/** Base-color images the build can recolor (`color_adjust.rs image_format`). */
export const EDITABLE_IMAGE = /\.(png|jpe?g|tga|tiff?|exr)$/i;
const OTHER_KEYS = ["roughness", "metalness", "opacity", "emission"] as const;
/** The Surface changes row the page edits for a material: the last enabled one
 * (the build refuses two enabled rows for one material), else the last one. */
export function appearanceRowIndex(rows: AppearanceMaterial[], target: string): number {
  let fallback = -1;
  for (let i = rows.length - 1; i >= 0; i--) {
    if (rows[i].target !== target) continue;
    if (rows[i].enabled !== false) return i;
    if (fallback < 0) fallback = i;
  }
  return fallback;
}
/** Anything on a row besides its color values, target and switch (roughness, metalness …). */
const otherKeys = (row: AppearanceMaterial) => Object.keys(row).filter((key) => !["enabled", "target", ...COLOR_KEYS].includes(key) && row[key] !== undefined);
const folder = (name: string) => {
  const parts = name.split("/");
  return parts.length > 1 ? parts[parts.length - 2] : "";
};

export function replacementColorAdapter(project: Project): ColorAdapter {
  const appearance = project.customization.appearance ?? {};
  const rows = appearance.materials ?? [];
  const active = Boolean(project.customization.enabled) && appearance.enabled === true;
  const full = rows.length >= APPEARANCE_LIMIT;
  const names = Object.keys(project.materials);
  // Group by folder when the names have folders to tell apart; one group otherwise.
  const folders = new Set(names.map(folder));
  const grouped = folders.size > 1;
  const setRows = (materials: AppearanceMaterial[]) =>
    changeCustomization(project, { appearance: { ...appearance, enabled: true, materials } });
  const saved = rows.flatMap((row) => (row.target && !project.materials[row.target] && colorValues(row) ? [row.target] : []));
  const ids = [...names, ...new Set(saved)];
  const materials = ids.map((id): ColorMaterial => {
    const material = project.materials[id];
    const missing = !material;
    const index = appearanceRowIndex(rows, id);
    const row = index >= 0 ? rows[index] : undefined;
    const values = colorValues(row);
    const image = material?.textures?.color || "";
    const kept = material?.mode === "keep";
    const ranges = Boolean(image) && EDITABLE_IMAGE.test(image) && !kept;
    const state: ColorState = values ? (row!.enabled === false || !active ? "paused" : "changed") : missing ? "missing" : "original";
    const editable = !missing && (Boolean(row) || !full);
    const other = row ? otherKeys(row) : [];
    return {
      id,
      label: stem(id.split("/").pop() ?? id),
      group: missing ? "Saved, not in the model" : grouped ? folder(id) || "Materials" : "Materials",
      texture: image && !missing ? { kind: "file", path: image } : null,
      capabilities: { tint: !missing, hue: !missing, saturation: !missing, brightness: !missing, ranges: ranges && !missing },
      notes: {
        tint: kept ? "This material keeps its source VMAT: the build changes it only when it uses pbr.vfx." : "The tint multiplies this material's base color.",
        whole: image ? undefined : "No base-color image is assigned in Materials; the plain base color is adjusted instead.",
        ranges: ranges ? undefined
          : kept ? "Color ranges need a generated material; this one keeps its source VMAT."
          : image ? "Color ranges need a PNG, JPEG, TGA, TIFF or EXR base-color image."
          : "This material has a plain base color, not an image: color ranges need a base-color image. Use Tint, Hue shift, Saturation and Brightness instead.",
      },
      state,
      values,
      enabled: row?.enabled !== false,
      editable,
      blocked: missing ? "This material is no longer in the prepared model" : !editable ? `Surface changes hold at most ${APPEARANCE_LIMIT} rows. Remove one on the Appearance page before adding another.` : "",
      originalTint: WHITE,
      baseColor: image ? null : material?.color ?? [0.5, 0.5, 0.5],
      missing,
      sharedWith: [],
      allowShared: false,
      details: [
        ["Material", id],
        ...(image ? [["Base-color image", image] as [string, string]] : []),
        ["Material handling", kept ? "Keep source VMAT" : `Generated (${material?.preset ?? "generic"})`],
      ],
      approximate: !image || /\.(exr|tiff?)$/i.test(image),
      otherChanges: other,
    };
  });
  return {
    mode: "replacement",
    materials,
    active,
    limit: APPEARANCE_LIMIT,
    full,
    changed: materials.filter((material) => material.values).length,
    edit(id, changes, removed = []) {
      const index = appearanceRowIndex(rows, id);
      if (!project.materials[id] && index < 0) return null;
      const base: AppearanceMaterial = index >= 0 ? rows[index] : { enabled: true, target: id };
      const next: AppearanceMaterial = { ...base, ...changes } as AppearanceMaterial;
      delete (next as Record<string, unknown>).allow_shared;
      removed.forEach((key) => delete next[key]);
      if (next.ranges && !next.ranges.length) delete next.ranges;
      // Hue, saturation and brightness that change nothing are not kept (the
      // Appearance page then shows Adjust colors off, as the build treats it).
      if ((next.hue ?? 0) === 0 && (next.saturation ?? 1) === 1 && (next.brightness ?? 1) === 1) {
        delete next.hue;
        delete next.saturation;
        delete next.brightness;
      }
      const empty = neutral(next) && !otherKeys(next).length;
      if (index >= 0) return setRows(empty ? rows.filter((_, i) => i !== index) : rows.map((row, i) => (i === index ? next : row)));
      if (!neutral(next) && rows.length < APPEARANCE_LIMIT) return setRows([...rows, next]);
      return null;
    },
    remove(id) {
      const index = appearanceRowIndex(rows, id);
      if (index < 0) return project;
      return setRows(strip(rows, (_, i) => i === index));
    },
    resetAll() {
      return setRows(strip(rows, (row) => Boolean(colorValues(row))));
    },
  };
}
/** Rows with the color values of the chosen ones removed; a row left with nothing is dropped. */
function strip(rows: AppearanceMaterial[], chosen: (row: AppearanceMaterial, index: number) => boolean) {
  return rows.flatMap((row, index) => {
    if (!chosen(row, index)) return [row];
    const next = { ...row };
    for (const key of COLOR_KEYS) delete next[key];
    return otherKeys(next).length ? [next] : [];
  });
}
/** Saved hero colors that a build with a replacement model leaves out. */
export const savedHeroColors = (project: Project) => (project.customization.hero_materials?.materials ?? []).length;
export function colorAdapter(project: Project, catalog: ComponentCatalog | null, revision = 0): ColorAdapter {
  return modelEnabled(project) ? replacementColorAdapter(project) : heroColorAdapter(project, catalog, revision);
}
export { OTHER_KEYS };
