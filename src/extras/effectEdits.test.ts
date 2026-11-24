import { describe, expect, it } from "vitest";
import type { Project } from "../models";
import {
  advancedText,
  changesSomething,
  cleanEdit,
  byFileName,
  groupTrees,
  isFiltering,
  matches,
  shownGroups,
  normalHue,
  leftOut,
  withPart,
  sharedLeftOut,
  alsoUsedReach,
  partUnreached,
  pausedRows,
  replacementConflicts,
  stripBackground,
  stripText,
  tagCounts,
  unreachedReason,
  withColor,
  withEdit,
  withEnabled,
  withExcluded,
  withPick,
  withRows,
} from "./effectEdits";
import { BOMB, CORE, DARK, DEBRIS, HEAD, OLD_LEAP, RED, SMOKE, TEAL, TRACER, effectTrees, effectTreesWithOtherFiles } from "./effectFixtures";

const project = (customization: Record<string, unknown> = {}) => ({ id: "p", customization: { schema: 1, ...customization } }) as unknown as Project;

describe("effect edits", () => {
  it("saves only what changes, within the saved limits", () => {
    expect(cleanEdit({ hue: 0, saturation: 1, brightness: 1, size: 1, glow: 1, opacity: 1, excluded: [], colors: [] })).toEqual({});
    expect(cleanEdit({ hue: 370, saturation: 9, size: 40, excluded: [CORE, CORE.toUpperCase()] })).toEqual({ hue: 10, saturation: 4, size: 16, excluded: [CORE] });
    expect(normalHue(-190)).toBe(170);
    expect(normalHue(180)).toBe(180);
    expect(normalHue(-180)).toBe(180);
    expect(cleanEdit({ hue: 360 })).toEqual({});
  });
  it("counts parts left out and a same-colour replacement as no change by themselves", () => {
    expect(changesSomething({ excluded: [CORE] })).toBe(false);
    expect(changesSomething({ colors: [{ from: TEAL, to: TEAL }] })).toBe(false);
    expect(changesSomething({ colors: [{ from: TEAL, to: TEAL }], hue: 30 })).toBe(true);
    expect(changesSomething({ glow: 1.5 })).toBe(true);
  });
  it("keeps one row per target and drops a row that changes nothing", () => {
    let rows = withEdit([], BOMB, { hue: 40, excluded: [] }, [1, 2, 3]);
    expect(rows).toEqual([{ target: BOMB, edit: { hue: 40 }, main: [1, 2, 3] }]);
    rows = withEnabled(rows, BOMB.toUpperCase(), false);
    rows = withEdit(rows, BOMB.toUpperCase(), { hue: 50 });
    expect(rows).toEqual([{ target: BOMB, enabled: false, edit: { hue: 50 } }]);
    expect(pausedRows(rows, true)).toEqual([{ target: BOMB, edit: { hue: 50 } }]);
    expect(withEdit(rows, BOMB, { excluded: [CORE] })).toEqual([]);
  });
  it("leaves effect_edits out of the project when empty", () => {
    const withOne = withRows(project(), withEdit([], BOMB, { hue: 10 }), true);
    expect(withOne.customization.effect_edits).toHaveLength(1);
    expect(withOne.customization.enabled).toBe(true);
    expect("effect_edits" in withRows(withOne, [], false).customization).toBe(false);
  });
  it("sets, updates and removes single colours and parts left out", () => {
    let edit = withColor({}, RED, [0, 0, 255]);
    edit = withColor(edit, RED, [0, 255, 0], true);
    expect(edit.colors).toEqual([{ from: RED, to: [0, 255, 0], near: true }]);
    expect(withColor(edit, RED, null).colors).toEqual([]);
    edit = withExcluded(withExcluded({}, SMOKE, true), CORE, true);
    expect(withExcluded(edit, SMOKE, false).excluded).toEqual([CORE]);
    expect(advancedText({ excluded: [CORE], colors: [{ from: RED, to: TEAL }] })).toBe("Advanced settings are active: 1 part left out, 1 single color changed.");
    expect(withPick({ size: 2, hue: 5 }, { hue: 90, saturation: 0.9, brightness: 1.1 })).toEqual({ size: 2, hue: 90, saturation: 0.9, brightness: 1.1 });
  });
  it("filters and groups the list in the groups' order", () => {
    const filter = { query: "", tags: [], changedOnly: false };
    const groups = groupTrees(effectTrees, effectTrees.trees.filter((t) => matches(t, { ...filter, tags: ["weapon"] }, [])));
    expect(groups.map((g) => g.name)).toEqual(["Weapon · Test Gun"]);
    expect(effectTrees.trees.filter((t) => matches(t, { ...filter, query: "big expl" }, [])).map((t) => t.label)).toEqual(["Explode"]);
    expect(effectTrees.trees.filter((t) => matches(t, { ...filter, changedOnly: true }, [{ target: BOMB.toUpperCase(), edit: { hue: 1 } }]))).toHaveLength(1);
    expect(Object.fromEntries(tagCounts(effectTrees.trees, filter, []))).toEqual({ ability: 1, weapon: 1, hero: 2 });
  });
  it("keeps an ability without files for its note and folds the other files until opened or filtered", () => {
    const data = effectTreesWithOtherFiles;
    const none = { query: "", tags: [], changedOnly: false };
    expect(isFiltering(none)).toBe(false);
    expect(isFiltering({ ...none, query: "  " })).toBe(false);
    expect(isFiltering({ ...none, changedOnly: true })).toBe(true);
    const all = groupTrees(data, data.trees, false);
    expect(all.map((g) => [g.name, g.trees.length, Boolean(g.note), Boolean(g.folded)])).toEqual([
      ["Big Bomb", 1, false, false],
      ["Far Reach", 0, true, false],
      ["Weapon · Test Gun", 1, false, false],
      ["Other files in Test hero's folders", 2, true, true],
    ]);
    // While a filter is on, the empty group has nothing to say.
    expect(groupTrees(data, data.trees).map((g) => g.name)).not.toContain("Far Reach");
    const folded = shownGroups(all, new Set(), false, [], "")[3];
    expect([folded.trees.length, folded.hidden, folded.open]).toEqual([0, 2, false]);
    // Edited and selected effects stay in view.
    const kept = shownGroups(all, new Set(), false, [{ target: OLD_LEAP.toUpperCase(), edit: { hue: 3 } }], HEAD)[3];
    expect([kept.trees.map((t) => t.target), kept.hidden]).toEqual([[HEAD, OLD_LEAP], 0]);
    expect(shownGroups(all, new Set(["unlinked"]), false, [], "")[3]).toMatchObject({ hidden: 0, open: true });
    expect(shownGroups(all, new Set(), true, [], "")[3]).toMatchObject({ hidden: 0, open: true });
    expect(shownGroups(all, new Set(), false, [], "")[0]).toMatchObject({ hidden: 0, open: true });
    // Only folded effects: they are shown, so the page has one to select.
    const onlyFolded = groupTrees({ ...data, groups: data.groups.filter((g) => g.key === "unlinked") }, data.trees.filter((t) => t.group === "unlinked"), false);
    expect(shownGroups(onlyFolded, new Set(), false, [], "")[0]).toMatchObject({ hidden: 0, open: true });
    expect(shownGroups(onlyFolded, new Set(), false, [], "")[0].trees).toHaveLength(2);
  });
  it("draws strips with positions, alpha and two random halves, and names them", () => {
    const stops = [{ position: 1, rgb: RED, hex: "#C81E28", alpha: 128 }, { position: 0.2, rgb: DARK, hex: "#1E2120" }];
    expect(stripBackground("gradient", stops)).toBe("linear-gradient(90deg, rgb(30 33 32) 20%, rgb(200 30 40 / 0.502) 100%)");
    expect(stripBackground("random", stops)).toBe("linear-gradient(90deg, rgb(200 30 40 / 0.502) 0% 50%, rgb(30 33 32) 50% 100%)");
    expect(stripText("gradient", stops)).toBe("#1E2120 at 20%, #C81E28 (50% opaque) at 100%");
  });
  it("says why a multiplier reaches no part", () => {
    expect(unreachedReason("opacity", effectTrees.trees[0].multipliers.opacity)).toBe("Its parts are 3D models, ground decals or lights, which have no opacity setting.");
    expect(unreachedReason("size", effectTrees.trees[0].multipliers.size)).toBeNull();
    expect(unreachedReason("glow", { changes: 0, adds: 0, no_setting: 0, game_input: 2, parts_reached: 0, visible_parts: 2 })).toBe("The game sets its brightness while the effect plays.");
  });
  it("says per part why Size, Brightness or Opacity do not reach it", () => {
    const [, core, smoke] = effectTrees.trees[0].parts;
    expect(partUnreached(core)).toEqual(["Opacity does not reach it: the game sets it while the effect plays."]);
    expect(partUnreached(smoke)).toEqual(["Size, Brightness and Opacity do not reach it: a 3D model has no such setting."]);
    expect(partUnreached({ visible: true, renderers: ["C_OP_RenderProjected"], multipliers: [] })).toEqual(["Size, Brightness and Opacity do not reach it: a ground decal has no such setting."]);
    expect(partUnreached({ visible: false, renderers: [], multipliers: [] })).toEqual([]);
  });
  it("finds files replaced on Replace a file that belong to an edited effect", () => {
    const p = project({ effects: [{ target: SMOKE, source: "x.vpcf_c" }], effect_edits: [{ target: BOMB, edit: { hue: 9 } }] });
    expect(replacementConflicts(p, effectTrees).map((c) => c.part)).toEqual(["Smoke"]);
    expect(replacementConflicts(project({ effects: [{ target: SMOKE, source: "x" }] }), effectTrees)).toEqual([]);
  });
});

describe("parts other heroes use", () => {
  const tracer = effectTrees.trees[1];
  const debris = tracer.parts[1];
  it("are left out unless ticked in; excluded wins; saved within the limits", () => {
    expect(leftOut({}, debris)).toBe("shared");
    expect(leftOut({}, tracer.parts[0])).toBeNull();
    const on = withPart({}, debris, true);
    expect(on).toEqual({ excluded: [], shared: [DEBRIS] });
    expect(leftOut(on, debris)).toBeNull();
    expect(withPart(on, debris, false).shared).toEqual([]);
    expect(withPart({}, tracer.parts[0], false).excluded).toEqual([TRACER]);
    expect(cleanEdit({ hue: 5, shared: [DEBRIS, DEBRIS.toUpperCase()] })).toEqual({ hue: 5, shared: [DEBRIS] });
    expect(cleanEdit({ hue: 5, shared: [DEBRIS], excluded: [DEBRIS.toUpperCase()] })).toEqual({ hue: 5, excluded: [DEBRIS.toUpperCase()] });
    expect(cleanEdit({ shared: [] })).toEqual({});
    expect(changesSomething({ shared: [DEBRIS] })).toBe(false);
    expect(leftOut({ shared: [DEBRIS], excluded: [DEBRIS] }, debris)).toBe("user");
  });
  it("count as Advanced settings only when ticked in", () => {
    expect(advancedText({})).toBe("");
    expect(advancedText({ shared: [DEBRIS] })).toBe("Advanced settings are active: 1 part shared with other heroes included.");
    expect(sharedLeftOut(tracer, {}).map((p) => p.label)).toEqual(["Impact debris"]);
    expect(sharedLeftOut(tracer, { shared: [DEBRIS] })).toEqual([]);
  });
  it("tell whether the change shows where the effect is also used", () => {
    expect(alsoUsedReach(tracer, {})).toBe("kept");
    expect(alsoUsedReach(tracer, { shared: [DEBRIS] })).toBe("shows");
    expect(alsoUsedReach(effectTrees.trees[0], {})).toBe("shows");
    const mixed = { ...tracer, also_used_by: { ...tracer.also_used_by!, effects: [...tracer.also_used_by!.effects, { path: "x", label: "X", parts: [TRACER] }] } };
    expect(alsoUsedReach(mixed, {})).toBe("partly");
  });
});

describe("effects placed by their file name", () => {
  it("marks a group only when every one of its effects is there by name, filtered or not", () => {
    const [bomb, gun] = effectTrees.trees;
    const second = { ...gun, target: "particles/weapon_fx/test/test_muzzle.vpcf_c", grouped_by: "name" as const };
    const data = { ...effectTrees, trees: [bomb, { ...gun, grouped_by: "name" as const }, second] };
    expect(groupTrees(data, data.trees, false).map((g) => [g.key, Boolean(g.byName)])).toEqual([["ability:bomb", false], ["weapon:gun", true]]);
    // One weapon effect linked by the game's data: the group is not marked, even while a filter hides it.
    const mixed = { ...data, trees: [bomb, gun, second] };
    expect(groupTrees(mixed, [second]).map((g) => [g.key, Boolean(g.byName)])).toEqual([["weapon:gun", false]]);
    expect([byFileName(second), byFileName(gun)]).toEqual([true, false]);
  });
});
