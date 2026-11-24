import { describe, expect, it } from "vitest";
import type { Project } from "../models";
import type { ComponentCatalog, HeroMaterialEntry } from "../components/AssetCatalog";
import reference from "../../src-tauri/crates/deadlock-project/tests/fixtures/reference.json";
import { appearanceRowIndex, changeSummary, colorAdapter, heroColorAdapter, replacementColorAdapter, HERO_LIMIT, APPEARANCE_LIMIT } from "./colorAdapter";

const base = structuredClone(reference.cases[1].expected) as unknown as Project;
const hero = (customization: Project["customization"]): Project => ({ ...base, customization: { model_enabled: false, ...customization } } as Project);
const target = (name: string) => `models/heroes_wip/sample/materials/${name}.vmat_c`;
const entry = (name: string, values: Partial<HeroMaterialEntry> = {}): HeroMaterialEntry => ({
  path: target(name).replace(/_c$/, ""), target: target(name), label: name, shader: "pbr.vfx", supported: true, reason: "",
  tint: [0.9, 0.8, 0.7], albedo: [1, 1.1, 1.2], tint_masked: false, color_texture: target(`${name}_color`).replace(".vmat_c", ".vtex_c"),
  used_by: [], shared: false, original_sha256: "b".repeat(64), ...values,
});
const catalog = (entries: HeroMaterialEntry[]): ComponentCatalog => ({ hero_path: base.original_model, icons: [], audio: [], effects: [], animations: [], variants: [], animation_unavailable: null, warnings: [], materials: entries });
const coat = "materials/outfit/coat.vmat", boots = "materials/outfit/boots.vmat", face = "materials/head/face.vmat";
const replacement = (rows: NonNullable<NonNullable<Project["customization"]["appearance"]>["materials"]>, enabled = true): Project => ({
  ...base,
  dmx: "prepared.dmx",
  materials: {
    [coat]: { mode: "generate", preset: "cloth", textures: { color: "C:/tex/coat.png" } },
    [boots]: { mode: "keep", textures: { color: "C:/tex/boots.png" } },
    [face]: { mode: "generate", textures: { color: "C:/tex/face.dds" }, color: [0.7, 0.5, 0.4] },
    "materials/outfit/belt.vmat": { mode: "generate", textures: {}, color: [0.2, 0.2, 0.2] },
  },
  customization: { enabled: true, appearance: { enabled, materials: rows } },
} as unknown as Project);

describe("Colors adapter: game hero", () => {
  it("is chosen when the project builds without a replacement model", () => {
    expect(colorAdapter(hero({ enabled: true }), catalog([entry("coat")])).mode).toBe("hero");
    expect(colorAdapter({ ...base, customization: { enabled: true } } as Project, catalog([entry("coat")])).mode).toBe("replacement");
  });
  it("offers each setting only where the game material has it", () => {
    const adapter = heroColorAdapter(hero({ enabled: true }), catalog([
      entry("coat"),
      entry("glow", { tint: null, albedo: null, color_texture: null }),
      entry("decal", { supported: false, reason: "Integrity check failed: decal uses another shader." }),
    ]));
    const [coatItem, glow, decal] = adapter.materials;
    expect(coatItem.capabilities).toEqual({ tint: true, hue: false, saturation: true, brightness: true, ranges: true });
    expect(coatItem.originalTint).toEqual([0.9, 0.8, 0.7]);
    expect(coatItem.texture).toEqual({ kind: "hero", request: { hero_path: base.original_model, target: target("coat"), kind: "materials", max_dimension: 512 }, identity: `${"b".repeat(64)}:0` });
    expect(glow.capabilities).toEqual({ tint: false, hue: false, saturation: false, brightness: false, ranges: false });
    expect(glow.notes).toMatchObject({ tint: "This material has no tint setting.", whole: "This material has no saturation or brightness setting." });
    expect(glow.texture).toBeNull();
    expect(decal).toMatchObject({ state: "view-only", editable: false, blocked: "decal uses another shader." });
    expect(adapter.edit(decal.id, { color: [0, 0, 0] })).toBeNull();
  });
  it("writes rows, keeps a color only when it changes something and removes it", () => {
    const project = hero({ enabled: true });
    const adapter = heroColorAdapter(project, catalog([entry("coat")]));
    // Nothing changed: no row.
    expect(adapter.edit(target("coat"), { saturation: 1 })).toBeNull();
    const tinted = adapter.edit(target("coat"), { color: [0, 0, 0] })!;
    expect(tinted.customization.hero_materials).toEqual({ enabled: true, materials: [{ enabled: true, target: target("coat"), original_sha256: "b".repeat(64), allow_shared: false, color: [0, 0, 0] }] });
    const next = heroColorAdapter(tinted, catalog([entry("coat")]));
    expect(next.materials[0]).toMatchObject({ state: "changed", values: { color: [0, 0, 0] } });
    // Removing the last value removes the row (and does not switch customization on).
    const back = next.edit(target("coat"), {}, ["color"])!;
    expect(back.customization.hero_materials?.materials).toEqual([]);
    // Ranges alone keep the row while they are being set up; an empty list is dropped.
    const ranged = next.edit(target("coat"), { ranges: [{ enabled: true, hue_shift: 0 }] }, ["color"])!;
    expect(ranged.customization.hero_materials?.materials?.[0].ranges).toHaveLength(1);
    const cleared = heroColorAdapter(ranged, catalog([entry("coat")])).edit(target("coat"), { ranges: [] })!;
    expect(cleared.customization.hero_materials?.materials).toEqual([]);
    expect(heroColorAdapter(tinted, catalog([entry("coat")])).remove(target("coat")).customization.hero_materials?.materials).toEqual([]);
  });
  it("holds at most 64 materials and pauses with the section", () => {
    const rows = Array.from({ length: HERO_LIMIT }, (_, i) => ({ enabled: true, target: target(`m${i}`), color: [0, 0, 0] as [number, number, number] }));
    const full = heroColorAdapter(hero({ enabled: true, hero_materials: { enabled: false, materials: rows } }), catalog([entry("coat"), entry("m0")]));
    expect(full.full).toBe(true);
    expect(full.materials.find((m) => m.id === target("coat"))).toMatchObject({ editable: false, blocked: "Hero colors hold at most 64 materials. Remove one before adding another." });
    expect(full.edit(target("coat"), { color: [1, 0, 0] })).toBeNull();
    // Saved ones stay editable; the section switched off shows them paused.
    expect(full.materials.find((m) => m.id === target("m0"))).toMatchObject({ editable: true, state: "paused" });
    expect(full.resetAll().customization.hero_materials?.materials).toEqual([]);
  });
  it("lists shared materials and saved colors the game no longer has", () => {
    const shared = entry("coat", { shared: true, used_by: ["models/heroes_wip/sample/sample_alt.vmdl_c", "models/x/other.vmdl_c"] });
    const adapter = heroColorAdapter(hero({ enabled: true, hero_materials: { materials: [{ target: target("gone"), brightness: 0 }] } }), catalog([shared]));
    expect(adapter.materials[0]).toMatchObject({ state: "shared", sharedWith: ["sample_alt", "other"] });
    expect(adapter.materials[1]).toMatchObject({ id: target("gone"), missing: true, state: "changed", group: "Saved, not in the game", blocked: "This material is no longer in the game files" });
  });
});

describe("Colors adapter: replacement model", () => {
  it("lists the replacement's materials with what each can change", () => {
    const adapter = replacementColorAdapter(replacement([]));
    const byId = Object.fromEntries(adapter.materials.map((m) => [m.id, m]));
    expect(adapter.mode).toBe("replacement");
    expect(byId[coat].capabilities).toEqual({ tint: true, hue: true, saturation: true, brightness: true, ranges: true });
    expect(byId[coat].texture).toEqual({ kind: "file", path: "C:/tex/coat.png" });
    expect(byId[coat].group).toBe("outfit");
    // A kept source VMAT and an image the build cannot recolor take no color ranges.
    expect(byId[boots].capabilities.ranges).toBe(false);
    expect(byId[boots].notes.ranges).toMatch(/keeps its source VMAT/);
    expect(byId[face].capabilities.ranges).toBe(false);
    expect(byId[face].notes.ranges).toMatch(/PNG, JPEG, TGA, TIFF or EXR/);
    // A plain base color: hue, saturation and brightness change that color; no ranges.
    const belt = byId["materials/outfit/belt.vmat"];
    expect(belt.capabilities).toMatchObject({ hue: true, saturation: true, ranges: false });
    expect(belt.baseColor).toEqual([0.2, 0.2, 0.2]);
    expect(belt.notes.ranges).toMatch(/plain base color/);
  });
  it("edits the Surface changes row in place and keeps its other values", () => {
    const project = replacement([{ enabled: true, target: coat, roughness: 0.3, emission: 2 }]);
    const adapter = replacementColorAdapter(project);
    expect(adapter.materials.find((m) => m.id === coat)).toMatchObject({ values: null, state: "original", otherChanges: ["roughness", "emission"] });
    const next = adapter.edit(coat, { hue: 30, color: [1, 0, 0], ranges: [{ enabled: true, hue_shift: 90 }] })!;
    expect(next.customization.appearance).toEqual({ enabled: true, materials: [{ enabled: true, target: coat, roughness: 0.3, emission: 2, hue: 30, color: [1, 0, 0], ranges: [{ enabled: true, hue_shift: 90 }] }] });
    expect(next.customization.enabled).toBe(true);
    const removed = replacementColorAdapter(next).remove(coat);
    expect(removed.customization.appearance?.materials).toEqual([{ enabled: true, target: coat, roughness: 0.3, emission: 2 }]);
  });
  it("switches Surface changes on when a color is set, as the Appearance page does", () => {
    const next = replacementColorAdapter(replacement([], false)).edit(boots, { brightness: 0.5 })!;
    expect(next.customization.appearance).toEqual({ enabled: true, materials: [{ enabled: true, target: boots, brightness: 0.5 }] });
    expect(replacementColorAdapter(replacement([{ enabled: true, target: boots, brightness: 0.5 }], false)).materials.find((m) => m.id === boots)?.state).toBe("paused");
  });
  it("keeps nothing that changes nothing", () => {
    const adapter = replacementColorAdapter(replacement([{ enabled: true, target: coat, hue: 10 }]));
    // Back to neutral: the color editor values go and the row with them.
    expect(adapter.edit(coat, { hue: 0 })!.customization.appearance?.materials).toEqual([]);
    // A neutral value on a material without a row adds none.
    expect(adapter.edit(boots, { saturation: 1, hue: 0 })).toBeNull();
    // Neutral hue/saturation/brightness are removed together, other values stay.
    const kept = replacementColorAdapter(replacement([{ enabled: true, target: coat, hue: 10, saturation: 1, brightness: 1, color: [0, 0, 1] }])).edit(coat, { hue: 0 })!;
    expect(kept.customization.appearance?.materials).toEqual([{ enabled: true, target: coat, color: [0, 0, 1] }]);
  });
  it("edits the row the build uses: the last enabled one for a material", () => {
    const rows = [{ enabled: false, target: coat, hue: 5 }, { enabled: true, target: coat, hue: 20 }, { enabled: true, target: "" }, { enabled: false, target: coat, hue: 9 }];
    expect(appearanceRowIndex(rows, coat)).toBe(1);
    expect(appearanceRowIndex([{ enabled: false, target: coat }, { enabled: false, target: coat }], coat)).toBe(1);
    const next = replacementColorAdapter(replacement(rows)).edit(coat, { hue: 40 })!;
    expect(next.customization.appearance?.materials).toEqual([rows[0], { enabled: true, target: coat, hue: 40 }, rows[2], rows[3]]);
    // Reset all clears colors from every row and keeps rows that still hold something (and empty targets).
    const reset = replacementColorAdapter(replacement([...rows, { enabled: true, target: boots, color: [1, 1, 1], metalness: 1 }])).resetAll();
    expect(reset.customization.appearance?.materials).toEqual([{ enabled: true, target: "" }, { enabled: true, target: boots, metalness: 1 }]);
  });
  it("holds at most 256 rows and lists saved colors of materials the model no longer has", () => {
    const rows = Array.from({ length: APPEARANCE_LIMIT }, (_, i) => ({ enabled: true, target: `materials/x/m${i}.vmat`, color: [1, 1, 1] as [number, number, number] }));
    const adapter = replacementColorAdapter(replacement(rows));
    expect(adapter.full).toBe(true);
    expect(adapter.edit(coat, { hue: 10 })).toBeNull();
    expect(adapter.materials.find((m) => m.id === coat)?.blocked).toMatch(/at most 256 rows/);
    const missing = adapter.materials.find((m) => m.id === "materials/x/m0.vmat")!;
    expect(missing).toMatchObject({ missing: true, group: "Saved, not in the model", capabilities: { tint: false } });
    expect(adapter.remove(missing.id).customization.appearance?.materials).toHaveLength(APPEARANCE_LIMIT - 1);
  });
  it("says what changed", () => {
    expect(changeSummary({ color: [0, 0, 0], ranges: [{}] })).toBe("Tint · 1 color range");
    expect(changeSummary({ saturation: 1.2 })).toBe("Saturation 120%");
    expect(changeSummary({ hue: -30, brightness: 0.5, ranges: [{}, {}] })).toBe("Hue -30° · Brightness 50% · 2 color ranges");
    expect(changeSummary(null)).toBe("");
  });
});
