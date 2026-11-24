import { describe, expect, it, vi } from "vitest";
import type { Project } from "../models";
import { changesSince } from "../build/changes";
import { decodePreview } from "../preview/packet";
import {
  acceptedSource,
  cleanFit,
  cleanScale,
  cleanTurn,
  isDefaultFit,
  objectCounts,
  objectStatus,
  pausedObjectRows,
  percentOf,
  savedObjectRows,
  turned,
  unturned,
  withBase,
  withEnabled,
  withFit,
  withObjectRows,
  withScale,
  withSize,
  withSource,
  withoutRow,
} from "./objectRows";
import { FitQueue, groupTitle, isMessage, kindText, needsSettings, objectGroups, parseFitAnswer } from "./objectCatalog";
import { DAGGER, TRIANGLE, daggerPacket, fitAnswer, hazeObjects, hollidayObjects, KEG_KIND, kelvinObjects, viscousObjects, wandPacket } from "./objectFixtures";
import { collisionLines } from "./ObjectScene";
import { MAX_ABILITY_OBJECTS, OBJECTS_NO_BLENDER, OBJECTS_NO_GAME, OBJECT_NEEDS_CURRENT_SDK, OBJECT_NOT_PREPARED, type AbilityObjectRow } from "./objectTypes";

const project = (rows?: AbilityObjectRow[]) => ({ id: "p", name: "x", original_model: "models/heroes_staging/haze/haze.vmdl", customization: { enabled: true, ...(rows ? { ability_objects: rows } : {}) } }) as unknown as Project;

describe("ability object rows", () => {
  it("adds a row with exactly target, enabled, source and an empty fit", () => {
    expect(withSource([], DAGGER, "C:\\art\\wand.blend")).toEqual([{ target: DAGGER, enabled: true, source: "C:\\art\\wand.blend", fit: {} }]);
    expect(Object.keys(withSource([], DAGGER, "a.fbx")[0])).toEqual(["target", "enabled", "source", "fit"]);
  });
  it("keeps one row per target ignoring case, its spelling, fit and pause when the file changes", () => {
    const rows = [{ target: "Models/Abilities/Haze_Dagger_Model.vmdl", enabled: false, source: "a.fbx", fit: { scale: 2 } }];
    expect(withSource(rows, DAGGER, "b.glb")).toEqual([{ target: "Models/Abilities/Haze_Dagger_Model.vmdl", enabled: false, source: "b.glb", fit: { scale: 2 } }]);
  });
  it("refuses a new row past the limit and leaves the rows as they are", () => {
    const rows = Array.from({ length: MAX_ABILITY_OBJECTS }, (_, i) => ({ target: `models/a/${i}.vmdl`, enabled: true, source: "a.fbx", fit: {} }));
    expect(withSource(rows, DAGGER, "a.fbx")).toBe(rows);
    expect(withSource(rows, "MODELS/A/3.VMDL", "b.fbx")[3].source).toBe("b.fbx");
  });
  it("drops unknown keys a row came with (the backend refuses them)", () => {
    const odd = [{ target: DAGGER, enabled: true, source: "a.fbx", fit: { scale: 1, extra: 1 }, note: "x" } as unknown as AbilityObjectRow];
    expect(withFit(odd, DAGGER, { size: "keep" })).toEqual([{ target: DAGGER, enabled: true, source: "a.fbx", fit: { size: "keep" } }]);
    expect(withEnabled(odd, DAGGER, false)).toEqual([{ target: DAGGER, enabled: false, source: "a.fbx", fit: {} }]);
  });
  it("saves the default fit as {} and keeps only what differs", () => {
    expect(cleanFit({ size: "match", scale: 1, turns: { roll: 0, tip: 4, turn: 0 } })).toEqual({});
    expect(isDefaultFit({ scale: 1.001 })).toBe(true);
    expect(cleanFit({ size: "keep", scale: 0.5, turns: { turn: 3 } })).toEqual({ size: "keep", scale: 0.5, turns: { turn: 3 } });
    expect(cleanFit({ scale: 100 })).toEqual({ scale: 20 });
    expect(cleanFit({ scale: 0.001 })).toEqual({ scale: 0.05 });
    expect(cleanScale(1.234)).toBe(1.23);
    expect(cleanScale(Number.NaN)).toBe(1);
  });
  it("keeps standing on the game's base only when set, and only as true", () => {
    expect(cleanFit({ on_base: true })).toEqual({ on_base: true });
    expect(cleanFit({ on_base: false })).toEqual({});
    expect(withBase({ scale: 1.5 }, true)).toEqual({ scale: 1.5, on_base: true });
    expect(withBase({ scale: 1.5, on_base: true }, false)).toEqual({ scale: 1.5 });
    expect(isDefaultFit({ on_base: false })).toBe(true);
    expect(isDefaultFit({ on_base: true })).toBe(false);
    // Other fit changes keep it.
    expect(turned({ on_base: true }, "turn")).toEqual({ turns: { turn: 1 }, on_base: true });
    expect(withSize({ on_base: true }, "keep")).toEqual({ size: "keep", on_base: true });
    expect(withFit([{ target: DAGGER, enabled: true, source: "a.fbx", fit: {} }], DAGGER, { on_base: true })[0].fit).toEqual({ on_base: true });
  });
  it("wraps quarter turns 0–3 and resets them", () => {
    expect([0, 1, 2, 3, 4, 5, -1].map(cleanTurn)).toEqual([0, 1, 2, 3, 0, 1, 3]);
    let fit = {};
    for (let i = 0; i < 3; i++) fit = turned(fit, "roll");
    expect(fit).toEqual({ turns: { roll: 3 } });
    expect(turned(fit, "roll")).toEqual({});
    expect(unturned({ scale: 2, turns: { tip: 1 } })).toEqual({ scale: 2 });
    expect(withSize({ size: "keep" }, "match")).toEqual({});
    expect(withScale({}, 1.5)).toEqual({ scale: 1.5 });
    expect(percentOf({ scale: 1.5 })).toBe(150);
    expect(percentOf({})).toBe(100);
  });
  it("removes the key from the customization with the last row", () => {
    const one = project([{ target: DAGGER, enabled: true, source: "a.fbx", fit: {} }]);
    const none = withObjectRows(one, withoutRow(savedObjectRows(one), DAGGER.toUpperCase()), false);
    expect("ability_objects" in none.customization).toBe(false);
    expect(withObjectRows(project(), [{ target: DAGGER, enabled: true, source: "a.fbx", fit: {} }], true).customization.ability_objects).toHaveLength(1);
  });
  it("counts an emptied list (a hero change) as nothing and pauses and resumes every row", () => {
    expect(objectCounts(project([]))).toEqual({ saved: 0, enabled: 0 });
    const rows = [{ target: DAGGER, enabled: true, source: "a.fbx", fit: {} }, { target: TRIANGLE, enabled: false, source: "b.fbx", fit: { scale: 2 } }];
    expect(objectCounts(project(rows))).toEqual({ saved: 2, enabled: 1 });
    expect(pausedObjectRows(rows, false).map((r) => r.enabled)).toEqual([false, false]);
    expect(pausedObjectRows(rows, true)).toEqual([{ ...rows[0] }, { ...rows[1], enabled: true }]);
  });
  it("accepts .blend, .fbx, .glb and .gltf files only", () => {
    expect(["a.blend", "B.FBX", "c.glb", "d.gltf", "e.pmx", "f.obj", "blend"].map(acceptedSource)).toEqual([true, true, true, true, false, false, false]);
  });
  it("gives a row Replaced, Paused or nothing", () => {
    expect(objectStatus(null, true)).toBe(null);
    expect(objectStatus({ target: DAGGER, enabled: true, source: "a.fbx" }, true)).toBe("Replaced");
    expect(objectStatus({ target: DAGGER, enabled: false, source: "a.fbx" }, true)).toBe("Paused");
    expect(objectStatus({ target: DAGGER, source: "a.fbx" }, false)).toBe("Paused");
  });
  it("names the rows in the changes list and the undo toast", () => {
    expect(changesSince(project(), project([{ target: DAGGER, enabled: true, source: "a.fbx", fit: {} }]))).toEqual(["Ability objects"]);
  });
});

describe("the hero's objects", () => {
  it("groups rows under their ability in the catalog's order, saved rows the catalog lacks last", () => {
    const groups = objectGroups(kelvinObjects(), [{ target: "models/gone/old.vmdl", source: "a.fbx" }], "Kelvin", false);
    expect(groups.map((g) => [g.title, g.items.length])).toEqual([["1 · Frost Grenade", 1], ["2 · Ice Path", 2], ["4 · Frozen Shelter", 1], ["Gun", 1], ["Saved, not listed now", 1]]);
    expect(objectGroups(kelvinObjects(), [], "Kelvin", true)).toEqual([]);
    expect(objectGroups(null, [{ target: DAGGER, source: "a.fbx" }], "Haze", true)[0].title).toBe("Saved replacements");
  });
  it("says what kind an object is from its structure", () => {
    const [grenade, path, , dome] = kelvinObjects().rows;
    // Haze's triangle is offered with the engine's kind; not offered, its one bone names it.
    const triangle = hazeObjects().rows[1];
    expect([grenade, { ...path, kind: undefined, offered: false }, { ...dome, kind: undefined, offered: false }, { ...triangle, kind: undefined, offered: false }].map(kindText)).toEqual(["Simple object", "Animated object", "Object with collision", "Object with a skeleton"]);
    // Viscous's cube is offered with its kept collision: the engine's kind names it.
    expect(kindText(viscousObjects().rows[0])).toBe("An object the game places, with collision and attachment points");
    expect(kindText(triangle)).toBe("An object an effect draws, with one bone");
    expect(groupTitle({ group: 300, ability_name: "Shiv's Rage", ability: "x" }, "Shiv")).toBe("Innate · Shiv's Rage");
  });
  it("takes the kind of an object a switch opened from the engine, never from its structure", () => {
    const keg = hollidayObjects().rows[0];
    expect(kindText(keg)).toBe(KEG_KIND.label);
    expect(kindText({ ...keg, kind: undefined })).toBe("Simple object");
    expect(kindText({ ...keg, offered: false })).toBe("Object with a skeleton");
  });
  it("outlines collision shapes as line pairs: a capsule's rings, sides and caps, a sphere's three circles", () => {
    const capsule = collisionLines([{ shape: "capsule", start: [0, 0, 0], end: [0, 0, 10], radius: 2 }]);
    expect(capsule.length % 6).toBe(0);
    const points = Array.from({ length: capsule.length / 3 }, (_, i) => capsule.slice(i * 3, i * 3 + 3));
    // Every point lies on the capsule's surface: 2 from the segment.
    for (const [x, y, z] of points) expect(Math.hypot(x, y, z < 0 ? z : z > 10 ? z - 10 : 0)).toBeCloseTo(2, 6);
    expect(Math.max(...points.map((p) => p[2]))).toBeCloseTo(12, 6);
    expect(Math.min(...points.map((p) => p[2]))).toBeCloseTo(-2, 6);
    const sphere = collisionLines([{ shape: "sphere", center: [1, 2, 3], radius: 4 }]);
    for (let i = 0; i < sphere.length; i += 3) expect(Math.hypot(sphere[i] - 1, sphere[i + 1] - 2, sphere[i + 2] - 3)).toBeCloseTo(4, 6);
    expect(collisionLines([])).toEqual([]);
  });
  it("outlines a kept hull or mesh shape as the twelve edges of its bounds", () => {
    const box = collisionLines([{ shape: "box", min: [-1, -2, 0], max: [1, 2, 4] }]);
    expect(box.length).toBe(12 * 6);
    for (let i = 0; i < box.length; i += 6) {
      const [a, b] = [box.slice(i, i + 3), box.slice(i + 3, i + 6)];
      // Each edge runs along one axis, between two corners.
      expect([0, 1, 2].filter((k) => a[k] !== b[k]).length).toBe(1);
      for (const p of [a, b]) expect([p[0] === -1 || p[0] === 1, p[1] === -2 || p[1] === 2, p[2] === 0 || p[2] === 4]).toEqual([true, true, true]);
    }
  });
  it("knows the engine's messages with or without the integrity prefix", () => {
    expect(isMessage(`Integrity check failed: ${OBJECT_NOT_PREPARED}`, OBJECT_NOT_PREPARED)).toBe(true);
    expect([OBJECTS_NO_GAME, OBJECTS_NO_BLENDER, OBJECT_NEEDS_CURRENT_SDK, OBJECT_NOT_PREPARED].map(needsSettings)).toEqual([true, true, true, false]);
  });
  it("reads a fit answer into packets the preview decoder takes", () => {
    const answer = parseFitAnswer(fitAnswer(DAGGER, [daggerPacket()], wandPacket()));
    expect(answer.view.target).toBe(DAGGER);
    expect(answer.game).toHaveLength(1);
    expect(decodePreview(answer.game[0]).metadata.meshes[0].index_count).toBe(72);
    expect(decodePreview(answer.yours!).metadata.meshes[0].vertex_count).toBe(72);
    expect(parseFitAnswer(fitAnswer(DAGGER, [daggerPacket()], null)).yours).toBe(null);
    expect(() => parseFitAnswer(new ArrayBuffer(2))).toThrow();
  });
});

describe("the fit queue", () => {
  it("runs one request at a time, keeps only the newest waiting one and drops old answers", async () => {
    const applied: string[] = [], started: string[] = [];
    const releases: (() => void)[] = [];
    const queue = new FitQueue<string>((v) => applied.push(v), vi.fn());
    const ask = (name: string) => () => { started.push(name); return new Promise<string>((resolve) => releases.push(() => resolve(name))); };
    queue.request(ask("a"));
    queue.request(ask("b"));
    queue.request(ask("c"));
    await Promise.resolve(); await Promise.resolve();
    expect(started).toEqual(["a"]);
    releases[0]();
    await new Promise((r) => setTimeout(r, 0));
    expect(applied).toEqual([]);
    expect(started).toEqual(["a", "c"]);
    releases[1]();
    await new Promise((r) => setTimeout(r, 0));
    expect(applied).toEqual(["c"]);
  });
});

import { cleanFollow as followOf, withFollow as rowsWithFollow, withPartBone as partOn, withFit as rowsWithFit, withSource as rowsWithSource } from "./objectRows";

describe("how a model follows an object with several bones", () => {
  const target = "models/heroes_wip/mcginnis/healing_canister/healing_canister.vmdl";
  const plain = [{ target, enabled: true, source: "C:/art/parts.gltf", fit: {} }];
  it("is absent unless set, so rows without it keep their keys", () => {
    expect(rowsWithFit(plain, target, { scale: 2 })[0]).toEqual({ target, enabled: true, source: "C:/art/parts.gltf", fit: { scale: 2 } });
    expect(Object.keys(rowsWithSource(plain, target, "C:/art/b.gltf")[0])).toEqual(["target", "enabled", "source", "fit"]);
  });
  it("keeps only the keys of its way, and survives other edits", () => {
    expect(followOf({ way: "one_piece", bone: "main", parts: { a: "b" } })).toEqual({ way: "one_piece", bone: "main" });
    expect(followOf({ way: "same_skeleton", bone: "main" })).toEqual({ way: "same_skeleton" });
    expect(followOf({ way: "by_parts", parts: { bar: "full_bottom", "": "x" } })).toEqual({ way: "by_parts", parts: { bar: "full_bottom" } });
    expect(followOf({ way: "spin" } as never)).toBeUndefined();
    const followed = rowsWithFollow(plain, target, partOn(undefined, "bar", "full_bottom"));
    expect(followed[0].follow).toEqual({ way: "by_parts", parts: { bar: "full_bottom" } });
    expect(rowsWithFit(followed, target, { scale: 2 })[0].follow).toEqual(followed[0].follow);
    expect(partOn({ way: "one_piece", bone: "main" }, "column", "main")).toEqual({ way: "by_parts", parts: { column: "main" } });
  });
});
