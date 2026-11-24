// A small hand-written effect catalog for the Effects page tests (shapes as effect_trees.rs writes them).
import type { EffectMultiplier, EffectOffers, EffectPart, EffectSite, EffectSiteStatus, EffectPreview, EffectSiteCounts, EffectTree, EffectTrees, Rgb } from "./effectTypes";

const counts = (reached: number, visible: number, extra: Partial<EffectSiteCounts> = {}): EffectSiteCounts => ({ changes: reached, adds: 0, no_setting: 0, game_input: 0, parts_reached: reached, visible_parts: visible, ...extra });
const no = (reason: string, player_shows = true) => ({ offered: false, reason, player_shows });
const yes = (player_shows = true) => ({ offered: true, player_shows });
/** What a sprite part takes: Hide only (a part that draws nothing takes nothing). */
function offers(visible: boolean): EffectOffers {
  return {
    hide: visible ? yes() : no("it draws nothing itself"),
    size: no("only 3D model parts have a size of their own"),
    opacity: no("only 3D model parts have an opacity of their own", false),
    tint: no("only a 3D model part takes a tint of its own", false),
    light: no("no light in this part", false),
  };
}
const hex = (c: Rgb) => "#" + c.map((v) => v.toString(16).padStart(2, "0")).join("").toUpperCase();
const part = (path: string, label: string, extra: Partial<EffectPart> = {}): EffectPart => ({
  path, label, parent: null, depth: 1, children: [], catalog: "hero_directory", shared: false, outside_hero: false, in_trees: [], renderers: ["C_OP_RenderSprites"], blend: "additive",
  visible: true, weight: 0.5, strips: [], may_not_follow: [], does_not_follow: [], multipliers: [],
  kind: { kind: "sprites", label: "Glow sprites", mixed: false }, controls: offers(extra.visible !== false), ...extra,
});
const site = (multiplier: EffectMultiplier, status: EffectSiteStatus): EffectSite => ({ multiplier, renderer: "r", location: "m_Renderers[0]", value: null, applies: status === "changes" || status === "adds", status, reason: "" });
export const TEAL: Rgb = [60, 230, 200];
export const DARK: Rgb = [30, 33, 32];
export const RED: Rgb = [200, 30, 40];
export const BOMB = "particles/abilities/test/test_bomb.vpcf_c";
export const CORE = "particles/abilities/test/test_bomb_core.vpcf_c";
export const SMOKE = "particles/abilities/test/test_bomb_smoke.vpcf_c";
export const TRACER = "particles/weapon_fx/test/test_tracer.vpcf_c";
export const DEBRIS = "particles/weapon_fx/default_impact_debris.vpcf_c";
const bomb: EffectTree = {
  target: BOMB, label: "Explode", name: "Big Bomb · explode", group: "ability:bomb", group_name: "Big Bomb", tags: ["hero", "ability"], records: ["ability_bomb"],
  parts: [
    part(BOMB, "Explode", { depth: 0, visible: false, weight: 0, renderers: [], children: [CORE, SMOKE] }),
    part(CORE, "Core", { parent: BOMB, size_limit: 0.05, multipliers: [site("size", "changes"), site("glow", "adds"), site("opacity", "game_input")], strips: [{ id: "m_ConstantColor", kind: "constant", role: "the part's own color", function: "CParticleSystemDefinition", stops: [{ position: 0, rgb: TEAL, hex: hex(TEAL), alpha: 255, slot: "m_ConstantColor", weight: 0.6 }] }] }),
    part(SMOKE, "Smoke", {
      parent: BOMB, in_trees: [BOMB, TRACER], renderers: ["C_OP_RenderModels"], multipliers: [site("size", "no_setting"), site("glow", "no_setting"), site("opacity", "no_setting")],
      strips: [{ id: "m_Gradient", kind: "gradient", role: "color over its life", function: "C_OP_ColorInterpolate", stops: [{ position: 0.2, rgb: DARK, hex: hex(DARK), slot: "a", weight: 0.2 }, { position: 1, rgb: RED, hex: hex(RED), alpha: 128, slot: "b", weight: 0.2 }] }],
      may_not_follow: [{ kind: "model_material", function: "C_OP_RenderModels", location: "m_Renderers[0]", reason: "a 3D model drawn without the particle color (its tint is switched off)" }],
    }),
  ],
  missing: [],
  palette: [
    { rgb: TEAL, hex: hex(TEAL), members: [TEAL], weight: 0.6, parts: [CORE], slots: 1 },
    { rgb: DARK, hex: hex(DARK), members: [DARK], weight: 0.2, parts: [SMOKE], slots: 1, neutral: "black" },
    { rgb: RED, hex: hex(RED), members: [RED], weight: 0.2, parts: [SMOKE], slots: 1 },
  ],
  main: { rgb: TEAL, hex: hex(TEAL), swatch: 0, share: 0.75 },
  also_used_by: { records: [], effects: [], note: "Also used by: Another ability (Other hero)" },
  shared_with_trees: [TRACER],
  follow: { may_not: 1, does_not: 0 },
  multipliers: { size: counts(1, 2), glow: counts(2, 2), opacity: counts(0, 2, { game_input: 1, no_setting: 1 }) },
  grouped_by: "definition",
  outside_parts: 0,
};
const tracer: EffectTree = {
  target: TRACER, label: "Tracer", name: "Weapon · tracer", group: "weapon:gun", group_name: "Weapon", tags: ["hero", "weapon"], records: ["gun"],
  parts: [
    part(TRACER, "Tracer", { depth: 0, children: [DEBRIS], strips: [{ id: "c", kind: "constant", role: "the part's own color", function: "x", stops: [{ position: 0, rgb: [255, 170, 0], hex: "#FFAA00", slot: "c", weight: 1 }] }] }),
    part(DEBRIS, "Impact debris", { parent: TRACER, shared: true, outside_hero: true, strips: [{ id: "d", kind: "constant", role: "the part's own color", function: "x", stops: [{ position: 0, rgb: [90, 80, 70], hex: "#5A5046", slot: "d", weight: 0.3 }] }] }),
  ],
  also_used_by: { records: [], effects: [{ path: "particles/weapon_fx/default_impact.vpcf_c", label: "Default impact", parts: [DEBRIS] }], note: "Also used by: Default impact (effect)" },
  missing: [], palette: [{ rgb: [255, 170, 0], hex: "#FFAA00", members: [[255, 170, 0]], weight: 1, parts: [TRACER], slots: 1 }],
  main: { rgb: [255, 170, 0], hex: "#FFAA00", swatch: 0, share: 1 }, shared_with_trees: [], follow: { may_not: 0, does_not: 0 },
  multipliers: { size: counts(1, 1), glow: counts(1, 1), opacity: counts(1, 1) },
  grouped_by: "definition",
  outside_parts: 1,
};
export const effectTrees: EffectTrees = {
  schema: 1, policy: "effect-edit-v1", hero: "hero_test", hero_name: "Test hero", sources: { definitions: true, names: true },
  groups: [
    { key: "ability:bomb", name: "Big Bomb", order: 101, kind: "ability", trees: 1 },
    { key: "weapon:gun", name: "Weapon", order: 200, kind: "weapon", record_name: "Test Gun", trees: 1 },
  ],
  trees: [bomb, tracer],
  counts: { trees: 2, parts: 4, files: 4, linked: 2, unlinked: 0, with_note: 1, files_in_several_trees: 1, may_not_follow: 1, does_not_follow: 0, left_out: 0 },
  warnings: ["One effect file could not be read."],
};
export const OLD_LEAP = "particles/abilities/test/test_old_leap.vpcf_c";
export const HEAD = "particles/heroes/test/test_head_ambient.vpcf_c";
const otherFile = (target: string, label: string): EffectTree => ({
  ...tracer,
  target, label, name: label, group: "unlinked", group_name: "Other files in Test hero's folders", tags: ["hero"], records: [], grouped_by: "none",
  parts: [part(target, label, { depth: 0 })], also_used_by: undefined, shared_with_trees: [], outside_parts: 0,
});
/** The catalog with an ability that has no files of the hero's folders, and a folded group of other files. */
export const effectTreesWithOtherFiles: EffectTrees = {
  ...effectTrees,
  groups: [
    ...effectTrees.groups,
    { key: "ability:far", name: "Far Reach", order: 103, kind: "ability", trees: 0, note: "The game's definitions link none of Test hero's own effect files to this ability. If it has any, they are under Other files." },
    { key: "unlinked", name: "Other files in Test hero's folders", order: 900, kind: "unlinked", trees: 2, folded: true, note: "Nothing in the game's definitions, the hero's model or its animations names these files. The game may still play some of them, or none." },
  ],
  trees: [...effectTrees.trees, otherFile(HEAD, "Head ambient"), otherFile(OLD_LEAP, "Old leap")],
};
export const DAGGER = "particles/abilities/test/test_dagger.vpcf_c";
export const DAGGER_MODEL = "particles/abilities/test/test_dagger_model.vpcf_c";
export const DAGGER_GLOW = "particles/abilities/test/test_dagger_glow.vpcf_c";
export const DAGGER_LIGHT = "particles/abilities/test/test_dagger_light.vpcf_c";
const orange: Rgb = [229, 140, 63];
const strip = (id: string, rgb: Rgb) => [{ id, kind: "constant" as const, role: "the part's own color", function: "x", stops: [{ position: 0, rgb, hex: hex(rgb), slot: id, weight: 0.3 }] }];
/** A tree with a 3D model without a color (Size, Opacity and Tint of its own), a glow and a light. */
const dagger: EffectTree = {
  ...tracer,
  target: DAGGER, label: "Dagger", name: "Sleep · dagger", group: "ability:bomb", group_name: "Big Bomb", tags: ["hero", "ability"], records: ["ability_bomb"], outside_parts: 0, also_used_by: undefined,
  parts: [
    part(DAGGER, "Dagger", { depth: 0, children: [DAGGER_MODEL, DAGGER_GLOW, DAGGER_LIGHT], strips: strip("t", [255, 200, 120]) }),
    part(DAGGER_MODEL, "Dagger model", {
      parent: DAGGER, renderers: ["C_OP_RenderModels"], multipliers: [site("size", "no_setting"), site("glow", "no_setting"), site("opacity", "no_setting")],
      kind: { kind: "model", label: "3D model", mixed: false, note: "This part has no color of its own in the game's files." },
      controls: { hide: yes(), size: yes(), opacity: yes(false), tint: yes(false), light: no("no light in this part", false) },
    }),
    part(DAGGER_GLOW, "Glow", { parent: DAGGER, strips: strip("g", [255, 160, 60]) }),
    part(DAGGER_LIGHT, "Hit light", {
      parent: DAGGER, renderers: ["C_OP_RenderStandardLight"], strips: strip("l", orange),
      kind: { kind: "light", label: "Light", mixed: false, note: "A light on nearby surfaces. It takes the effect's colors." },
      controls: { ...offers(true), light: { offered: true, player_shows: false, note: "1 of its 2 lights stays as it is: the game sets this light's brightness while it plays" } },
    }),
  ],
  palette: [{ rgb: orange, hex: hex(orange), members: [orange], weight: 1, parts: [DAGGER_LIGHT], slots: 1 }],
  main: { rgb: orange, hex: hex(orange), swatch: 0, share: 1 },
  multipliers: { size: counts(3, 4), glow: counts(3, 4), opacity: counts(3, 4) },
};
/** The catalog with the dagger tree (per-part controls). */
export const effectTreesWithControls: EffectTrees = { ...effectTrees, trees: [...effectTrees.trees, dagger] };
/** A preview that turns every color into `to` (what the test needs, not real maths). */
export function previewAll(tree: EffectTree, to: Rgb, extra: Partial<EffectPreview> = {}): EffectPreview {
  return {
    target: tree.target,
    edit: { hue: 0, saturation: 1, brightness: 1, excluded: [], shared: [], colors: [], size: 1, glow: 1, opacity: 1 },
    parts: tree.parts.map((p) => ({ path: p.path, excluded: p.outside_hero, left_out: p.outside_hero ? ("shared" as const) : null, multipliers: [], hidden: false, controls: [], strips: p.strips.map((s) => ({ id: s.id, kind: s.kind, role: s.role, stops: s.stops.map((st) => ({ position: st.position, alpha: st.alpha, before: st.rgb, before_hex: st.hex, after: to, after_hex: hex(to), slot: st.slot, rule: "shifted" as const })) })) })),
    palette: tree.palette.map((s) => ({ before: s.rgb, before_hex: s.hex, after: to, after_hex: hex(to) })),
    main: tree.main ? { before: tree.main.rgb, before_hex: tree.main.hex, after: to, after_hex: hex(to) } : null,
    changed: { colors: 1, multiplied: 0, not_multiplied: 0, hidden: 0, part_settings: 0 },
    ...extra,
  };
}
