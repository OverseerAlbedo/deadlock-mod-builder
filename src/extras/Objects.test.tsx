import { useEffect, useRef, useState } from "react";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Project } from "../models";
import type { ComponentCatalog } from "../components/AssetCatalog";
import { HistoryContext, useProjectHistory, useUndoKeys } from "../history/useProjectHistory";
import { ExtrasWorkspace } from "./ExtrasPage";
import { COLLISION_NOTE, FIRST_KIND_KEPT, forgetObjects, OWN_COLORS_SHOW } from "./Objects";
import { CANISTER, canisterAnswer, mcginnisObjects, motionAnswer, blockPacket, BREAK_PIECES_REASON, FOLDERS_REASON, DAGGER, daggerPacket, fitAnswer, greyTalonObjects, hazeObjects, hollidayObjects, KEG, kegAnswer, KEG_KIND, kelvinObjects, solomonObjects, wandPacket, ICE_DOME, NOT_DRAWN_REASON, CUBE, CUBE_KIND, cubeAnswer, viscousObjects, OWL_KIND, TRIANGLE, KEPT_GAME_COLLISION, BISHOP_KIND, KEPT_BREAK_PIECES, FILE_DETAIL_LINES, mcginnisTurretObjects, TURRET_KIND, KEPT_RANGE_MARKERS, KEPT_BODY_GROUP_PARTS } from "./objectFixtures";
import { BRIEFCASE, BRIEFCASE_KIND, KEPT_GLOW_SHELL, pocketObjects } from "./objectFixtures";
import { ICE_PATH, ICE_PATH_KIND, KEPT_SHAPE } from "./objectFixtures";
import { HELPER, HELPER_ATTACHED, HELPER_KIND, KEPT_GAME_CLOTH, remObjects } from "./objectFixtures";
import { babaObjects, calicoObjects, CAT_FORM, CAT_FORM_KIND, KEPT_SHAWL, PIGEON, PIGEON_KIND } from "./objectFixtures";
import { ASSISTANT, ASSISTANT_KIND, KEPT_COLLISION, sinclairObjects } from "./objectFixtures";
import { deadmanDannyObjects, MISSILE, MISSILE_KIND, missileAnswer, PER_STATE_LINE } from "./objectFixtures";
import { abramsObjects, doormanObjects, DOOR, DOOR_KIND, GROUND_IMPACT, GROUND_IMPACT_KIND, gravesHandObjects, HAND, HAND_KIND, KEPT_DOOR_FRAME, KEPT_DOOR_PORTAL, KEPT_PIECES } from "./objectFixtures";
import { GAME_LOOK_ALWAYS, GAME_LOOK_SOMETIMES, KEPT_GAME_ANIMATIONS, KEPT_MATERIAL_ANIMATION_JITTER, LAUNCHPAD, LAUNCHPAD_KIND, mcginnisWallObjects, WALL, WALL_FOUND_BY, WALL_KIND, WALL_PHYSICS } from "./objectFixtures";
import { OBJECT_MOTION_KEPT, OBJECT_MOTION_NOT_READ, OBJECT_NOT_PREPARED, SCENERY_NOTE, type AbilityObjectRow, type ObjectFit } from "./objectTypes";
import { ObjectPlayer } from "./ObjectMoving";
import { itemCatalogFixture } from "./itemFixtures";
import reference from "../../src-tauri/crates/deadlock-project/tests/fixtures/reference.json";

const native = vi.hoisted(() => ({ abilityObjectCatalog: vi.fn(), prepareAbilityObject: vi.fn(), abilityObjectFit: vi.fn(), abilityObjectMotion: vi.fn(), validateAssetPath: vi.fn(), previewExtra: vi.fn(), itemCatalog: vi.fn() }));
vi.mock("../api", async (original) => ({ ...(await original<typeof import("../api")>()), api: native }));
const dialog = vi.hoisted(() => ({ openFor: vi.fn() }));
vi.mock("../dialogs", async (original) => ({ ...(await original<typeof import("../dialogs")>()), openFor: dialog.openFor }));
// WebGL cannot run here: the view counts its mounts and shows what it was given.
const scene = vi.hoisted(() => ({ mounts: 0, last: null as null | { mode: string; yours: boolean; collision: boolean } }));
vi.mock("./ObjectScene", async () => {
  const { useEffect } = await import("react");
  return {
    default: ({ answer, mode, collision }: { answer: { yours: ArrayBuffer | null; view: { target: string } }; mode: string; collision?: boolean }) => {
      useEffect(() => { scene.mounts++; }, []);
      scene.last = { mode, yours: Boolean(answer.yours), collision: Boolean(collision) };
      return <div data-testid="object-scene">{answer.view.target} {mode} {answer.yours ? "yours" : "game only"}{collision ? " collision" : ""}</div>;
    },
  };
});

const haze = { ...reference.cases[1].expected, original_model: "models/heroes_staging/haze/haze.vmdl", hero_name: "Haze", customization: { enabled: true } } as unknown as Project;
const heroCatalog: ComponentCatalog = { hero_path: haze.original_model, icons: [], audio: [], effects: [], animations: [], variants: [], animation_unavailable: null, warnings: [] };
const withRows = (rows: AbilityObjectRow[]) => ({ ...haze, customization: { enabled: true, ability_objects: rows } }) as Project;
const order: string[] = [];
let operate: ReturnType<typeof vi.fn>;

function Harness({ start = haze, auto = false }: { start?: Project; auto?: boolean }) {
  const [p, set] = useState(() => structuredClone(start));
  const current = useRef(p);
  const write = (next: Project) => { current.current = next; set(next); return true; };
  const { controls, record } = useProjectHistory({ read: () => current.current, write });
  useUndoKeys(controls);
  const update = (next: Project) => { order.push("write"); record(current.current, next); write(next); };
  useEffect(() => { (window as unknown as { controls: typeof controls }).controls = controls; });
  return (
    <HistoryContext.Provider value={controls}>
      <ExtrasWorkspace project={p} update={update} disabled={false} catalog={heroCatalog} canDiscover discover={vi.fn()} fail={vi.fn()} openSetup={vi.fn()} reviewBuild={vi.fn()} operate={operate as never} autoPreviews={auto} />
      <output data-testid="saved">{JSON.stringify(p)}</output>
    </HistoryContext.Provider>
  );
}
const saved = () => JSON.parse(screen.getByTestId("saved").textContent!) as Project;
const rows = () => saved().customization.ability_objects;
const controls = () => (window as unknown as { controls: { canUndo: boolean; undo: () => void } }).controls;
const panel = () => within(screen.getByRole("complementary", { name: "Object details" }));
async function open(props: { start?: Project; auto?: boolean } = {}) {
  render(<Harness {...props} />);
  fireEvent.click(screen.getByRole("tab", { name: /^Objects/ }));
  return within(await screen.findByRole("region", { name: "Objects" }));
}
const fitOf = (call: unknown[]) => call[3] as ObjectFit;

beforeEach(() => {
  vi.clearAllMocks();
  forgetObjects();
  order.length = 0;
  scene.mounts = 0;
  operate = vi.fn(async (_label: string, op: () => Promise<unknown>) => { order.push("operate"); return op(); });
  native.abilityObjectCatalog.mockImplementation(async () => hazeObjects());
  native.prepareAbilityObject.mockImplementation(async (_p, target, source) => ({ target, source, root: "models/dmb_objects/x", prepared: { counts: { meshes: 1, vertices: 24, triangles: 12, materials: 1 }, warnings: [] } }));
  native.abilityObjectFit.mockImplementation(async (_p, target, source) => fitAnswer(target, [daggerPacket()], source ? wandPacket() : null));
  native.validateAssetPath.mockImplementation(async (p) => p);
  native.itemCatalog.mockImplementation(async () => itemCatalogFixture());
  dialog.openFor.mockResolvedValue("C:\\art\\my_wand.blend");
});

describe("Extras → Objects", () => {
  it("asks for nothing until the tab opens, then once, and lists the objects under their ability", async () => {
    render(<Harness />);
    expect(screen.getByRole("tab", { name: /^Objects/ })).toHaveTextContent("—");
    fireEvent.click(screen.getByRole("tab", { name: /Sounds/ }));
    await act(async () => {});
    expect(native.abilityObjectCatalog).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("tab", { name: /^Objects/ }));
    const list = within(await screen.findByRole("region", { name: "Objects" }));
    expect(await list.findByRole("button", { name: "Dagger" })).toBeInTheDocument();
    expect(native.abilityObjectCatalog).toHaveBeenCalledTimes(1);
    expect(list.getByRole("region", { name: "1 · Sleep Dagger" })).toBeInTheDocument();
    expect(list.getByText("also used by Mina and Vyper")).toBeInTheDocument();
    // The triangle an effect draws, with one bone: offered with the engine's kind line.
    expect(list.getByRole("button", { name: "Triangle" })).toHaveTextContent("An object an effect draws, with one bone");
    // Every listed object can be replaced: the line says so without a count.
    expect(list.getByText(/^2 objects belong to Haze's abilities\. Plain shapes/)).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /^Objects/ })).toHaveTextContent("2");
    fireEvent.click(screen.getByRole("tab", { name: /Icons/ }));
    fireEvent.click(screen.getByRole("tab", { name: /^Objects/ }));
    await act(async () => {});
    expect(native.abilityObjectCatalog).toHaveBeenCalledTimes(1);
    expect(native.abilityObjectFit).not.toHaveBeenCalled();
  });

  it("writes nothing when a row is selected, and offers no file for an object that is not offered", async () => {
    native.abilityObjectCatalog.mockImplementation(async () => ({ ...kelvinObjects(), hero_path: haze.original_model }));
    const list = await open();
    await list.findByRole("button", { name: "Bomb" });
    const before = screen.getByTestId("saved").textContent;
    // A row that is not offered says why, without hovering.
    expect(list.getByRole("button", { name: "Ice dome, not offered yet" })).toHaveTextContent(NOT_DRAWN_REASON);
    fireEvent.click(list.getByRole("button", { name: "Ice dome, not offered yet" }));
    fireEvent.click(list.getByRole("button", { name: "Bomb" }));
    fireEvent.click(list.getByRole("button", { name: "Ice dome, not offered yet" }));
    expect(screen.getByTestId("saved").textContent).toBe(before);
    expect(order).toEqual([]);
    expect(controls().canUndo).toBe(false);
    expect(panel().getByText(NOT_DRAWN_REASON)).toBeInTheDocument();
    expect(panel().queryByRole("button", { name: /Choose file/ })).toBeNull();
    expect(native.abilityObjectFit).not.toHaveBeenCalled();
  });

  it("writes the row as one step BEFORE the preparation starts, then refreshes the view by itself", async () => {
    await open();
    fireEvent.click(await panel().findByRole("button", { name: "Choose file…" }));
    await screen.findByTestId("object-scene");
    expect(order.slice(0, 2)).toEqual(["write", "operate"]);
    expect(order.filter((o) => o === "write")).toHaveLength(1);
    expect(rows()).toEqual([{ target: DAGGER, enabled: true, source: "C:\\art\\my_wand.blend", fit: {} }]);
    const [prepareProject, target, source] = native.prepareAbilityObject.mock.calls[0];
    expect((prepareProject as Project).customization.ability_objects).toHaveLength(1);
    expect([target, source]).toEqual([DAGGER, "C:\\art\\my_wand.blend"]);
    expect(screen.getByTestId("object-scene")).toHaveTextContent("yours");
    expect(native.abilityObjectFit).toHaveBeenCalledTimes(1);
    expect(panel().getByText("my_wand.blend")).toBeInTheDocument();
    act(() => controls().undo());
    expect(rows()).toBeUndefined();
  });

  it("after a failed preparation shows the game's object alone and offers to prepare again", async () => {
    await open({ start: withRows([{ target: DAGGER, enabled: true, source: "C:\\art\\first.fbx", fit: {} }]) });
    fireEvent.click(await screen.findByRole("button", { name: "Show this object" }));
    expect(await screen.findByTestId("object-scene")).toHaveTextContent("yours");
    native.prepareAbilityObject.mockRejectedValueOnce({ kind: "validation", message: "The object's file could not be prepared" });
    // As the engine answers: a file whose preparation failed has nothing prepared to draw.
    native.abilityObjectFit.mockImplementation(async (_p, target, source) => {
      if (source === "C:\\art\\my_wand.blend") throw { kind: "validation", message: `Integrity check failed: ${OBJECT_NOT_PREPARED}` };
      return fitAnswer(target, [daggerPacket()], source ? wandPacket() : null);
    });
    operate.mockImplementationOnce(async (_l: string, op: () => Promise<unknown>) => { order.push("operate"); try { return await op(); } catch { return undefined; } });
    fireEvent.click(panel().getByRole("button", { name: "Change file…" }));
    expect(await panel().findByRole("button", { name: "Prepare it again" })).toBeInTheDocument();
    expect(rows()![0].source).toBe("C:\\art\\my_wand.blend");
    // The view is asked again after the failed preparation; its answer arrives a render later.
    await waitFor(() => expect(screen.getByTestId("object-scene")).toHaveTextContent("game only"));
    expect(panel().queryByText(/^Prepared:/)).toBeNull();
  });

  it("refuses a file of another kind without writing", async () => {
    dialog.openFor.mockResolvedValue("C:\\art\\model.pmx");
    await open();
    fireEvent.click(await panel().findByRole("button", { name: "Choose file…" }));
    expect(await panel().findByText(/An ability object comes from a Blender, FBX, GLB or glTF file/)).toBeInTheDocument();
    expect(order).toEqual([]);
  });

  it("loads the view only on a click, and draws a not-prepared file's object alone with Prepare it again", async () => {
    native.abilityObjectFit.mockImplementation(async (_p, target, source) => {
      if (source) throw { kind: "validation", message: `Integrity check failed: ${OBJECT_NOT_PREPARED}` };
      return fitAnswer(target, [daggerPacket()], null);
    });
    await open({ start: withRows([{ target: DAGGER, enabled: true, source: "C:\\art\\old.fbx", fit: {} }]) });
    await panel().findByText("old.fbx");
    await act(async () => {});
    expect(native.abilityObjectFit).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Show this object" }));
    expect(await screen.findByTestId("object-scene")).toHaveTextContent("game only");
    expect(operate).toHaveBeenCalledWith("The game's object could not be shown", expect.any(Function), true, true, false);
    expect(native.abilityObjectFit.mock.calls.map((c) => c[2])).toEqual(["C:\\art\\old.fbx", null]);
    fireEvent.click(panel().getByRole("button", { name: "Prepare it again" }));
    await waitFor(() => expect(native.prepareAbilityObject).toHaveBeenCalledTimes(1));
    // The same file again: nothing to write.
    expect(order.filter((o) => o === "write")).toHaveLength(0);
  });

  it("loads the view by itself with Load 3D previews automatically", async () => {
    await open({ auto: true });
    expect(await screen.findByTestId("object-scene")).toHaveTextContent("game only");
    expect(native.abilityObjectFit).toHaveBeenCalledTimes(1);
  });

  it("makes a slider drag one undo step and asks for the newest fit only", async () => {
    await open({ start: withRows([{ target: DAGGER, enabled: true, source: "C:\\art\\wand.fbx", fit: {} }]) });
    fireEvent.click(await screen.findByRole("button", { name: "Show this object" }));
    await screen.findByTestId("object-scene");
    expect(native.abilityObjectFit).toHaveBeenCalledTimes(1);
    const releases: (() => void)[] = [];
    native.abilityObjectFit.mockImplementation((_p, target, source, fit) => new Promise((resolve) => releases.push(() => resolve(fitAnswer(target, [daggerPacket()], source ? wandPacket(fit.scale ?? 1) : null, fit)))));
    const slider = panel().getByRole("slider", { name: "Adjust size" });
    fireEvent.pointerDown(slider);
    fireEvent.change(slider, { target: { value: "120" } });
    fireEvent.change(slider, { target: { value: "150" } });
    fireEvent.change(slider, { target: { value: "180" } });
    fireEvent.pointerUp(slider);
    expect(rows()![0].fit).toEqual({ scale: 1.8 });
    await act(async () => {});
    // One running (120%), the newest (180%) waiting; 150% was never asked.
    expect(native.abilityObjectFit.mock.calls.slice(1).map(fitOf)).toEqual([{ scale: 1.2 }]);
    await act(async () => { releases[0](); });
    await waitFor(() => expect(native.abilityObjectFit.mock.calls.slice(1).map(fitOf)).toEqual([{ scale: 1.2 }, { scale: 1.8 }]));
    act(() => controls().undo());
    expect(rows()![0].fit).toEqual({});
    expect(controls().canUndo).toBe(false);
  });

  it("turns, resizes and resets the fit, saving the default as {}", async () => {
    await open({ start: withRows([{ target: DAGGER, enabled: true, source: "a.fbx", fit: {} }]) });
    fireEvent.click(await panel().findByRole("button", { name: /Roll 90°/ }));
    fireEvent.click(panel().getByRole("button", { name: /Tip up 90°/ }));
    expect(rows()![0].fit).toEqual({ turns: { roll: 1, tip: 1 } });
    fireEvent.click(panel().getByRole("button", { name: "Keep my size" }));
    fireEvent.change(panel().getByRole("spinbutton", { name: "Adjust size percent" }), { target: { value: "1500" } });
    expect(rows()![0].fit).toEqual({ size: "keep", scale: 15, turns: { roll: 1, tip: 1 } });
    fireEvent.click(panel().getByRole("button", { name: "Reset turns" }));
    fireEvent.click(panel().getByRole("button", { name: "Match the game's object" }));
    fireEvent.change(panel().getByRole("spinbutton", { name: "Adjust size percent" }), { target: { value: "100" } });
    expect(rows()![0].fit).toEqual({});
  });

  it("never remounts the view for another object, a fit, the list filter, the view mode or another tab", async () => {
    native.abilityObjectCatalog.mockImplementation(async () => ({ ...kelvinObjects(), hero_path: haze.original_model }));
    const list = await open({ auto: true, start: withRows([{ target: "models/abilities/kelvin_fx_bomb_model.vmdl", enabled: true, source: "a.fbx", fit: {} }]) });
    await screen.findByTestId("object-scene");
    fireEvent.click(list.getByRole("button", { name: "Ice projectile" }));
    await waitFor(() => expect(native.abilityObjectFit).toHaveBeenCalledTimes(2));
    fireEvent.click(list.getByRole("button", { name: "Ice dome, not offered yet" }));
    fireEvent.click(list.getByRole("button", { name: "Bomb, replaced" }));
    fireEvent.click(panel().getByRole("button", { name: /Turn round 90°/ }));
    fireEvent.click(list.getByRole("button", { name: "Replaced" }));
    fireEvent.click(list.getByRole("button", { name: "All" }));
    fireEvent.click(screen.getByRole("button", { name: "The game's" }));
    expect(screen.getByTestId("object-scene")).toHaveTextContent("game");
    fireEvent.click(screen.getByRole("tab", { name: /Items/ }));
    fireEvent.click(screen.getByRole("tab", { name: /^Objects/ }));
    await act(async () => {});
    expect(scene.mounts).toBe(1);
  });

  it("counts the rows in the page header and the tab, and pauses and resumes them with the other extras", async () => {
    await open({ start: withRows([{ target: DAGGER, enabled: true, source: "a.fbx", fit: {} }]) });
    expect(screen.getByText("1 change")).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /^Objects/ }).querySelector("i")).not.toBeNull();
    expect(screen.getByText("1 replaced")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Pause all extras" }));
    expect(rows()).toEqual([{ target: DAGGER, enabled: false, source: "a.fbx", fit: {} }]);
    expect(await screen.findByRole("button", { name: "Dagger, paused" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Resume all extras" }));
    expect(rows()![0].enabled).toBe(true);
  });

  it("asks before removing every replacement, and removes the key with the last row", async () => {
    await open({ start: withRows([{ target: DAGGER, enabled: true, source: "a.fbx", fit: {} }]) });
    fireEvent.click(screen.getByRole("button", { name: "Remove all replacements…" }));
    expect(rows()).toHaveLength(1);
    fireEvent.click(screen.getByRole("button", { name: "Remove all" }));
    expect("ability_objects" in saved().customization).toBe(false);
    expect(screen.getByText("Nothing replaced")).toBeInTheDocument();
  });

  it("says what material the game draws an object in", async () => {
    await open();
    expect(await panel().findByText(/The game draws this object in a material of its own \(gray metal\)\./)).toBeInTheDocument();
    expect(panel().getByText(/Your object's shape shows in game; its colors and images do not\./)).toBeInTheDocument();
    expect(panel().getByText(/Also used by: Mina and Vyper\./)).toBeInTheDocument();
    expect(panel().queryByText(OWN_COLORS_SHOW)).toBeNull();
  });

  it("says a material the game does not override shows in game", async () => {
    native.abilityObjectCatalog.mockImplementation(async () => ({ ...kelvinObjects(), hero_path: haze.original_model }));
    await open();
    expect(await panel().findByText(OWN_COLORS_SHOW)).toBeInTheDocument();
    expect(panel().queryByText(/a material of its own/)).toBeNull();
  });

  it("changes nothing for the objects offered today: no kind line, no collision, no Stands on, the same kept items", async () => {
    await open({ start: withRows([{ target: DAGGER, enabled: true, source: "C:\\art\\wand.fbx", fit: {} }]) });
    const list = within(screen.getByRole("region", { name: "Objects" }));
    expect(await list.findByRole("button", { name: "Dagger, replaced" })).toHaveTextContent("Simple object");
    fireEvent.click(screen.getByRole("button", { name: "Show this object" }));
    expect(await screen.findByTestId("object-scene")).toHaveTextContent("yours");
    expect(scene.last?.collision).toBe(false);
    expect(screen.queryByRole("button", { name: "Collision shown" })).toBeNull();
    expect(screen.queryByText(COLLISION_NOTE)).toBeNull();
    expect(panel().queryByText("Stands on")).toBeNull();
    expect(panel().getAllByRole("listitem").map((li) => li.textContent)).toEqual(FIRST_KIND_KEPT);
    expect(panel().queryByText(/the game's collision/)).toBeNull();
    // The fit the page saves for them never carries the new key.
    fireEvent.click(panel().getByRole("button", { name: /Turn round 90°/ }));
    expect(rows()![0].fit).toEqual({ turns: { turn: 1 } });
  });

  it("offers an effect-drawn object with one bone with its kind line, offers body groups, closes what cannot be built, and lists what a hero now offers", async () => {
    native.abilityObjectCatalog.mockImplementation(async () => ({ ...greyTalonObjects(), hero_path: haze.original_model }));
    const list = await open();
    const arrow = await list.findByRole("button", { name: "Arrow" });
    expect(arrow).toHaveTextContent("An object an effect draws, with one bone");
    // Offered since the game's own animation blocks are kept on graph-played objects.
    expect(list.getByRole("button", { name: "Owl" })).toHaveTextContent(OWL_KIND.label);
    expect(list.queryByText(FOLDERS_REASON)).toBeNull();
    cleanup();
    forgetObjects();
    native.abilityObjectCatalog.mockImplementation(async () => ({ ...solomonObjects(), hero_path: haze.original_model }));
    const solomon = await open();
    expect(await solomon.findByRole("button", { name: "Chesspiece rook" })).toHaveTextContent("An object an effect draws, with collision, attachment points, hit areas and several materials");
    expect(solomon.getByRole("button", { name: "Knightshield" })).toHaveTextContent("An object an effect draws, with several materials");
    expect(solomon.getByRole("button", { name: "Tile" })).toHaveTextContent("An object an effect draws, with several materials and body groups");
    expect(solomon.getByRole("button", { name: "Rook crown" })).toHaveTextContent("An object an effect draws, with one bone and body groups");
    // Offered since the game's own break pieces and the order of its game data are written back.
    expect(solomon.getByRole("button", { name: "Chesspiece bishop" })).toHaveTextContent(BISHOP_KIND.label);
    expect(solomon.getByRole("button", { name: "Queen" })).toHaveTextContent(BISHOP_KIND.label);
    expect(solomon.getByRole("button", { name: "Rabbit" })).toHaveTextContent("Animated object · 49 moving parts");
    expect(solomon.queryByText(BREAK_PIECES_REASON)).toBeNull();
    expect(solomon.getByRole("button", { name: "Hand" })).toHaveTextContent("Animated object · 23 moving parts");
    // Every listed object can be replaced: the line names no smaller count.
    expect(solomon.getByText(/9 objects belong to Haze's abilities\./)).toBeInTheDocument();
    expect(solomon.queryByText(/can be replaced so far/)).toBeNull();
  });

  it("lists the game's own break pieces under what stays the game's, and none of the written file details", async () => {
    native.abilityObjectCatalog.mockImplementation(async () => ({ ...solomonObjects(), hero_path: haze.original_model }));
    const list = await open();
    fireEvent.click(await list.findByRole("button", { name: "Chesspiece bishop" }));
    const kept = panel().getAllByRole("listitem").map((li) => li.textContent);
    expect(kept).toEqual([...BISHOP_KIND.kept, FIRST_KIND_KEPT[1]]);
    expect(kept).toContain(KEPT_BREAK_PIECES);
    fireEvent.click(list.getByRole("button", { name: "Rabbit" }));
    for (const line of FILE_DETAIL_LINES) expect(screen.queryByText(line)).toBeNull();
  });

  it("lists a part kept as the game's own under what stays the game's, and none of the written file details", async () => {
    native.abilityObjectCatalog.mockImplementation(async () => ({ ...mcginnisTurretObjects(), hero_path: haze.original_model }));
    const list = await open();
    fireEvent.click(await list.findByRole("button", { name: "Turret" }));
    const kept = panel().getAllByRole("listitem").map((li) => li.textContent);
    expect(kept).toEqual([...TURRET_KIND.kept, FIRST_KIND_KEPT[1]]);
    expect(kept).toContain(KEPT_RANGE_MARKERS);
    expect(kept).toContain(KEPT_BODY_GROUP_PARTS);
    for (const line of FILE_DETAIL_LINES) expect(screen.queryByText(line)).toBeNull();
  });

  it("offers the door, the ground impact and the hand with the engine's kind line and kept items, and no control of their own", async () => {
    const inputs = () => ["button", "combobox", "checkbox", "slider", "spinbutton", "radio"].flatMap((role) => panel().queryAllByRole(role).map((c) => `${role}:${c.getAttribute("aria-label") ?? c.textContent}`));
    const all = { ...mcginnisTurretObjects(), hero_path: haze.original_model };
    all.rows = [...all.rows, ...doormanObjects().rows, ...abramsObjects().rows, ...gravesHandObjects().rows];
    native.abilityObjectCatalog.mockImplementation(async () => all);
    const list = await open();
    // An object of a kind offered before: its controls are what the new kinds may show.
    fireEvent.click(await list.findByRole("button", { name: "Turret" }));
    const before = inputs();
    const kept = () => panel().getAllByRole("listitem").map((li) => li.textContent);
    // The door: its leaves replaced, the frame and the portal surface the game's own.
    const door = list.getByRole("button", { name: "Door" });
    expect(door).toHaveTextContent(DOOR_KIND.label);
    fireEvent.click(door);
    expect(kept()).toEqual([...DOOR_KIND.kept, FIRST_KIND_KEPT[1]]);
    expect(kept()).toEqual(expect.arrayContaining([KEPT_DOOR_FRAME, KEPT_DOOR_PORTAL, KEPT_BODY_GROUP_PARTS]));
    expect(panel().queryByText(GAME_LOOK_ALWAYS)).toBeNull();
    expect(panel().queryByText(GAME_LOOK_SOMETIMES)).toBeNull();
    expect(inputs()).toEqual(before);
    // The ground impact: each state shows your object, the second one's other pieces the game's own.
    const ground = list.getByRole("button", { name: "Ground impact" });
    expect(ground).toHaveTextContent(GROUND_IMPACT_KIND.label);
    fireEvent.click(ground);
    expect(kept()).toEqual([FIRST_KIND_KEPT[0], ...GROUND_IMPACT_KIND.kept, FIRST_KIND_KEPT[1]]);
    expect(kept()).toContain(KEPT_PIECES);
    expect(kept()).not.toContain(KEPT_BODY_GROUP_PARTS);
    expect(inputs()).toEqual(before);
    // The hand: nothing beyond its kinds offered before (its bone constraints are file detail).
    const hand = list.getByRole("button", { name: "Hand" });
    expect(hand).toHaveTextContent(HAND_KIND.label);
    fireEvent.click(hand);
    expect(kept()).toEqual([FIRST_KIND_KEPT[0], KEPT_GAME_ANIMATIONS, "Its bones: each part of yours moves and turns with the bone it follows", FIRST_KIND_KEPT[1]]);
    expect(inputs()).toEqual(before);
    expect([DOOR, GROUND_IMPACT, HAND]).toEqual(["models/heroes_wip/doorman/doorman_door.vmdl", "models/particle/ground_impact_fx_model.vmdl", "models/heroes_wip/necro/hand/necro_hand.vmdl"]);
  });

  it("lists the glow shell made from your object under what stays the game's", async () => {
    native.abilityObjectCatalog.mockImplementation(async () => ({ ...pocketObjects(), hero_path: haze.original_model }));
    const list = await open();
    const briefcase = await list.findByRole("button", { name: "Briefcase" });
    expect(briefcase).toHaveTextContent(BRIEFCASE_KIND.label);
    fireEvent.click(briefcase);
    const kept = panel().getAllByRole("listitem").map((li) => li.textContent);
    expect(kept).toEqual([FIRST_KIND_KEPT[0], ...BRIEFCASE_KIND.kept, FIRST_KIND_KEPT[1]]);
    expect(kept).toContain(KEPT_GLOW_SHELL);
    expect(panel().getByRole("button", { name: /Choose file/ })).toBeInTheDocument();
    expect(BRIEFCASE).toBe("models/abilities/pocket_briefcase_model.vmdl");
  });

  it("offers an object animated on one bone with its kept animation listed first", async () => {
    native.abilityObjectCatalog.mockImplementation(async () => ({ ...hollidayObjects(), hero_path: haze.original_model }));
    const list = await open();
    const pad = await list.findByRole("button", { name: "Cowboy launchpad" });
    expect(pad).toHaveTextContent(LAUNCHPAD_KIND.label);
    fireEvent.click(pad);
    const kept = panel().getAllByRole("listitem").map((li) => li.textContent);
    expect(kept).toEqual([...LAUNCHPAD_KIND.kept, FIRST_KIND_KEPT[1]]);
    expect(kept[0]).toBe(KEPT_GAME_ANIMATIONS);
    expect(panel().getByRole("button", { name: /Choose file/ })).toBeInTheDocument();
    expect(LAUNCHPAD).toBe("models/particle/cowboy_launchpad.vmdl");
  });

  it("offers the travelling wall with the kept line about its animation, and no line about a file name", async () => {
    native.abilityObjectCatalog.mockImplementation(async () => ({ ...mcginnisWallObjects(), hero_path: haze.original_model }));
    const list = await open();
    fireEvent.click(await list.findByRole("button", { name: "Wall" }));
    const kept = panel().getAllByRole("listitem").map((li) => li.textContent);
    expect(kept).toEqual([FIRST_KIND_KEPT[0], ...WALL_KIND.kept, FIRST_KIND_KEPT[1]]);
    expect(kept).toContain(KEPT_MATERIAL_ANIMATION_JITTER);
    expect(screen.queryByText(WALL_FOUND_BY)).toBeNull();
    expect(WALL).toBe("models/abilities/engineer_wall.vmdl");
  });

  it("offers an object whose shape the game changes with the kept line about its shape, and no control of its own", async () => {
    native.abilityObjectCatalog.mockImplementation(async () => ({ ...kelvinObjects(), hero_path: haze.original_model }));
    const list = await open();
    const inputs = () => ["button", "combobox", "checkbox", "slider", "spinbutton", "radio"].flatMap((role) => panel().queryAllByRole(role).map((c) => `${role}:${c.getAttribute("aria-label") ?? c.textContent}`));
    fireEvent.click(await list.findByRole("button", { name: "Bomb" }));
    const plain = inputs();
    const path = list.getByRole("button", { name: "Ice path body" });
    expect(path).toHaveTextContent(ICE_PATH_KIND.label);
    fireEvent.click(path);
    const kept = panel().getAllByRole("listitem").map((li) => li.textContent);
    expect(kept).toEqual([...ICE_PATH_KIND.kept, FIRST_KIND_KEPT[1]]);
    expect(kept).toContain(KEPT_SHAPE);
    expect(screen.getAllByText(KEPT_SHAPE)).toHaveLength(1);
    // The same controls as a plain object an effect draws: nothing new to set for the shape.
    expect(panel().queryByRole("combobox")).toBeNull();
    expect(inputs()).toEqual(plain);
    expect(ICE_PATH).toBe("models/props_gameplay/ice_path_body.vmdl");
  });

  it("offers an object whose cloth the build keeps as the game's own with the kept line about its cloth, and no control of its own", async () => {
    native.abilityObjectCatalog.mockImplementation(async () => ({ ...remObjects(), hero_path: haze.original_model }));
    const list = await open();
    const inputs = () => ["button", "combobox", "checkbox", "slider", "spinbutton", "radio"].flatMap((role) => panel().queryAllByRole(role).map((c) => `${role}:${c.getAttribute("aria-label") ?? c.textContent}`));
    // An animated object with several bones and no cloth: the controls to compare with.
    fireEvent.click(await list.findByRole("button", { name: "Helper attached" }));
    const plain = inputs();
    expect(panel().queryByText(KEPT_GAME_CLOTH)).toBeNull();
    const helper = list.getByRole("button", { name: "Helper npc" });
    expect(helper).toHaveTextContent(HELPER_KIND.label);
    fireEvent.click(helper);
    const kept = panel().getAllByRole("listitem").map((li) => li.textContent);
    expect(kept).toEqual([...HELPER_KIND.kept, FIRST_KIND_KEPT[1]]);
    expect(screen.getAllByText(KEPT_GAME_CLOTH)).toHaveLength(1);
    // A physics block that holds only cloth is no collision: no collision toggle, no "sticks out" line.
    expect(panel().queryByText(KEPT_GAME_COLLISION)).toBeNull();
    expect(panel().queryByRole("checkbox", { name: /Collision/ })).toBeNull();
    // The same controls as the object without cloth: nothing new to set for the cloth.
    expect(inputs()).toEqual(plain);
    expect([HELPER, HELPER_ATTACHED]).toEqual(["models/abilities/familiar/familiar_helper_npc.vmdl", "models/abilities/familiar/familiar_helper_attached.vmdl"]);
  });

  it("offers the cat form with its shape, collision and cloth lines, each once, and no control of its own", async () => {
    native.abilityObjectCatalog.mockImplementation(async () => ({ ...calicoObjects(), hero_path: haze.original_model }));
    const list = await open();
    const cat = await list.findByRole("button", { name: "Catform" });
    expect(cat).toHaveTextContent(CAT_FORM_KIND.label);
    fireEvent.click(cat);
    const kept = panel().getAllByRole("listitem").map((li) => li.textContent);
    expect(kept).toEqual([...CAT_FORM_KIND.kept, FIRST_KIND_KEPT[1]]);
    for (const line of [KEPT_SHAPE, KEPT_GAME_COLLISION, KEPT_GAME_CLOTH]) expect(screen.getAllByText(line)).toHaveLength(1);
    // Nothing to set for the shape, the reordered collision or the cloth.
    expect(panel().queryAllByRole("checkbox").filter((c) => /shape|cloth|order/i.test(c.getAttribute("aria-label") ?? c.textContent ?? ""))).toEqual([]);
    expect(CAT_FORM).toBe("models/abilities/nano_catform_model.vmdl");
  });

  it("offers the pigeon with its cloth line and its shawl named once, and no control of its own", async () => {
    native.abilityObjectCatalog.mockImplementation(async () => ({ ...babaObjects(), hero_path: haze.original_model }));
    const list = await open();
    const pigeon = await list.findByRole("button", { name: "Pigeon transform" });
    expect(pigeon).toHaveTextContent(PIGEON_KIND.label);
    fireEvent.click(pigeon);
    const kept = panel().getAllByRole("listitem").map((li) => li.textContent);
    expect(kept).toEqual([...PIGEON_KIND.kept, FIRST_KIND_KEPT[1]]);
    expect(screen.getAllByText(KEPT_SHAWL)).toHaveLength(1);
    expect(screen.getAllByText(KEPT_GAME_CLOTH)).toHaveLength(1);
    // No collision: no collision toggle; nothing to set for the shawl or the levels.
    expect(panel().queryByText(KEPT_GAME_COLLISION)).toBeNull();
    expect(panel().queryByRole("checkbox", { name: /Collision/ })).toBeNull();
    expect(panel().queryAllByRole("checkbox").filter((c) => /shawl|level/i.test(c.getAttribute("aria-label") ?? c.textContent ?? ""))).toEqual([]);
    expect(PIGEON).toBe("models/heroes_wip/baba/baba_pigeon_transform.vmdl");
  });

  it("offers Sinclair's assistant with its collision and cloth lines, each once, and no control of its own", async () => {
    native.abilityObjectCatalog.mockImplementation(async () => ({ ...sinclairObjects(), hero_path: haze.original_model }));
    const list = await open();
    const assistant = await list.findByRole("button", { name: "Cloneturret" });
    expect(assistant).toHaveTextContent(ASSISTANT_KIND.label);
    fireEvent.click(assistant);
    const kept = panel().getAllByRole("listitem").map((li) => li.textContent);
    expect(kept).toEqual([...ASSISTANT_KIND.kept, FIRST_KIND_KEPT[1]]);
    for (const line of [KEPT_COLLISION, KEPT_GAME_CLOTH]) expect(screen.getAllByText(line)).toHaveLength(1);
    // Its collision is made again (capsules), not kept: no kept-collision line; nothing to set for the cloth.
    expect(panel().queryByText(KEPT_GAME_COLLISION)).toBeNull();
    expect(panel().queryAllByRole("checkbox").filter((c) => /cloth|proxy|name/i.test(c.getAttribute("aria-label") ?? c.textContent ?? ""))).toEqual([]);
    expect(ASSISTANT).toBe("models/abilities/magician_cloneturret_model.vmdl");
  });

  it("says when effects draw the object in one of the game's own looks, where the colors note goes", async () => {
    native.abilityObjectCatalog.mockImplementation(async () => ({ ...mcginnisWallObjects(), hero_path: haze.original_model }));
    const list = await open();
    fireEvent.click(await list.findByRole("button", { name: "Wall" }));
    const line = panel().getByText(GAME_LOOK_ALWAYS);
    expect(line).toHaveClass("objects-warn");
    expect(panel().queryByText(OWN_COLORS_SHOW)).toBeNull();
    cleanup();
    forgetObjects();
    // Sometimes: the colors note stays, with the line after it.
    const sometimes = mcginnisWallObjects();
    sometimes.rows[0] = { ...sometimes.rows[0], game_look: { always: false, line: GAME_LOOK_SOMETIMES } };
    native.abilityObjectCatalog.mockImplementation(async () => ({ ...sometimes, hero_path: haze.original_model }));
    const again = await open();
    fireEvent.click(await again.findByRole("button", { name: "Wall" }));
    expect(panel().getByText(OWN_COLORS_SHOW)).toBeInTheDocument();
    expect(panel().getByText(GAME_LOOK_SOMETIMES)).toHaveClass("objects-note");
    expect(panel().queryByText(GAME_LOOK_ALWAYS)).toBeNull();
  });

  it("says where an object found by its file name comes from, in the engine's words", async () => {
    native.abilityObjectCatalog.mockImplementation(async () => ({ ...mcginnisWallObjects(true), hero_path: haze.original_model }));
    const list = await open();
    fireEvent.click(await list.findByRole("button", { name: "Wall, as placed" }));
    const line = panel().getByText(WALL_FOUND_BY);
    expect(line).toHaveClass("objects-found-by");
    expect(panel().getByRole("button", { name: /Choose file/ })).toBeInTheDocument();
    expect(WALL_PHYSICS).toBe("models/abilities/engineer_wall_physics.vmdl");
  });

  it("closes a model the game uses only as an invisible blocker, with the engine's reason", async () => {
    native.abilityObjectCatalog.mockImplementation(async () => ({ ...kelvinObjects(), hero_path: haze.original_model }));
    const list = await open();
    const dome = await list.findByRole("button", { name: "Ice dome, not offered yet" });
    expect(dome).toHaveTextContent(NOT_DRAWN_REASON);
    fireEvent.click(dome);
    expect(panel().getByText(NOT_DRAWN_REASON)).toBeInTheDocument();
    expect(panel().queryByRole("button", { name: /Choose file/ })).toBeNull();
    expect(screen.queryByRole("button", { name: "Show this object" })).toBeNull();
    expect(native.abilityObjectFit).not.toHaveBeenCalled();
    expect(ICE_DOME).toBe("models/abilities/ice_dome.vmdl");
  });

  it("says a scenery model may show in the map too, as the catalog marks it, whether or not it is replaced", async () => {
    native.abilityObjectCatalog.mockImplementation(async () => ({ ...hazeObjects(), scenery: [TRIANGLE] }));
    const list = await open();
    await list.findByRole("button", { name: "Dagger" });
    expect(panel().queryByText(SCENERY_NOTE)).toBeNull();
    fireEvent.click(list.getByRole("button", { name: "Triangle" }));
    const note = panel().getByText(SCENERY_NOTE);
    expect(note).toHaveClass("objects-plain");
    expect(note).toHaveAttribute("role", "note");
    expect(screen.getByText(SCENERY_NOTE)).toBe(note);
    cleanup();
    forgetObjects();
    native.abilityObjectCatalog.mockImplementation(async () => ({ ...hazeObjects(), scenery: [TRIANGLE] }));
    await open({ start: withRows([{ target: TRIANGLE, enabled: true, source: "C:\\art\\block.gltf", fit: {} }]) });
    expect(await panel().findByText(SCENERY_NOTE)).toBeInTheDocument();
    cleanup();
    forgetObjects();
    // A catalog without the key marks nothing.
    native.abilityObjectCatalog.mockImplementation(async () => ({ ...hazeObjects(), hero_path: haze.original_model }));
    const plain = await open();
    fireEvent.click(await plain.findByRole("button", { name: "Triangle" }));
    expect(screen.queryByText(SCENERY_NOTE)).toBeNull();
  });

  it("offers an object whose hull collision the build keeps as the game's own: the kind line, the kept item, the outline and the line", async () => {
    native.abilityObjectCatalog.mockImplementation(async () => ({ ...viscousObjects(), hero_path: haze.original_model }));
    native.abilityObjectFit.mockImplementation(async (_p, _target, source) => cubeAnswer(source ? blockPacket() : null));
    const list = await open({ start: withRows([{ target: CUBE, enabled: true, source: "C:\\art\\block.gltf", fit: {} }]) });
    expect(await list.findByRole("button", { name: "Cube, replaced" })).toHaveTextContent(CUBE_KIND.label);
    expect(panel().getAllByRole("listitem").map((li) => li.textContent)).toEqual([...CUBE_KIND.kept, FIRST_KIND_KEPT[1]]);
    expect(panel().getByText(KEPT_GAME_COLLISION)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Show this object" }));
    expect(await screen.findByTestId("object-scene")).toHaveTextContent("yours collision");
    expect(screen.getByRole("button", { name: "Collision shown" })).toHaveAttribute("aria-pressed", "true");
    expect(panel().getByText(/^Your object fits the game's collision/)).toHaveClass("objects-fits");
    expect(scene.mounts).toBe(1);
  });

  describe("an object with collision (the keg, offered with the app's switches)", () => {
    const keg = (fit: ObjectFit = {}) => withRows([{ target: KEG, enabled: true, source: "C:\\art\\block.gltf", fit }]);
    beforeEach(() => {
      native.abilityObjectCatalog.mockImplementation(async () => ({ ...hollidayObjects(), hero_path: haze.original_model }));
      native.abilityObjectFit.mockImplementation(async (_p, _target, source, fit) => kegAnswer(source ? blockPacket() : null, fit));
    });
    it("says what kind it is in the engine's words and lists what stays the game's", async () => {
      const list = await open({ start: keg() });
      expect(await list.findByRole("button", { name: "Explosive, replaced" })).toHaveTextContent(KEG_KIND.label);
      expect(panel().getByText(`${KEG_KIND.label}.`)).toBeInTheDocument();
      expect(panel().getAllByRole("listitem").map((li) => li.textContent)).toEqual([...KEG_KIND.kept, FIRST_KIND_KEPT[1]]);
    });
    it("draws the collision with a toggle and shows the engine's line about how far yours sticks out", async () => {
      await open({ start: keg() });
      fireEvent.click(await screen.findByRole("button", { name: "Show this object" }));
      expect(await screen.findByTestId("object-scene")).toHaveTextContent("yours collision");
      const toggle = screen.getByRole("button", { name: "Collision shown" });
      expect(toggle).toHaveAttribute("aria-pressed", "true");
      expect(screen.getByText(COLLISION_NOTE)).toBeInTheDocument();
      expect(screen.getByText("your object inside the game's collision")).toBeInTheDocument();
      const line = panel().getByText(/sticks out of the game's collision by up to 24\.8 in/);
      expect(line).toHaveClass("objects-warn");
      const before = screen.getByTestId("saved").textContent;
      fireEvent.click(toggle);
      expect(toggle).toHaveAttribute("aria-pressed", "false");
      expect(scene.last?.collision).toBe(false);
      expect(screen.queryByText(COLLISION_NOTE)).toBeNull();
      expect(screen.getByTestId("saved").textContent).toBe(before);
      expect(scene.mounts).toBe(1);
    });
    it("calls a close fit calm", async () => {
      native.abilityObjectFit.mockImplementation(async (_p, _target, source, fit) => kegAnswer(source ? blockPacket() : null, fit, 18.4));
      await open({ start: keg() });
      fireEvent.click(await screen.findByRole("button", { name: "Show this object" }));
      await screen.findByTestId("object-scene");
      expect(panel().getByText(/^Your object fits the game's collision/)).toHaveClass("objects-fits");
    });
    it("offers Stands on and saves it as one step, absent when centred", async () => {
      await open({ start: keg() });
      fireEvent.click(await panel().findByRole("button", { name: "The game's base" }));
      expect(rows()![0].fit).toEqual({ on_base: true });
      fireEvent.click(panel().getByRole("button", { name: "Centred on the game's" }));
      expect(rows()![0].fit).toEqual({});
      act(() => controls().undo());
      expect(rows()![0].fit).toEqual({ on_base: true });
    });
  });

  it("keeps a saved row the list no longer has, removable", async () => {
    await open({ start: withRows([{ target: "models/abilities/gone.vmdl", enabled: true, source: "a.fbx", fit: {} }]) });
    const list = within(screen.getByRole("region", { name: "Objects" }));
    fireEvent.click(await list.findByRole("button", { name: "Gone, replaced" }));
    fireEvent.click(panel().getByRole("button", { name: "Remove" }));
    expect(rows()).toBeUndefined();
  });
});

describe("an object the game animates (several bones, opened for a test)", () => {
  const start = (follow?: AbilityObjectRow["follow"]) => withRows([{ target: CANISTER, enabled: true, source: "C:\art\parts.gltf", fit: {}, ...(follow ? { follow } : {}) }]);
  beforeEach(() => {
    native.abilityObjectCatalog.mockImplementation(async () => ({ ...mcginnisObjects(), hero_path: haze.original_model }));
    native.abilityObjectFit.mockImplementation(async (_p, _t, source) => canisterAnswer(source ? blockPacket() : null));
    native.abilityObjectMotion.mockImplementation(async () => motionAnswer());
  });
  it("names the kind, offers the three ways, lists the parts with the suggestion, and writes one step per change", async () => {
    const list = await open({ auto: true, start: start() });
    expect(await list.findByRole("button", { name: /Healing canister/ })).toHaveTextContent("Animated object · 3 moving parts");
    const ways = await panel().findByRole("radiogroup", { name: "How your model moves" });
    // The approved picture's three ways; the way the app takes for this file carries the badge.
    expect(within(ways).getAllByRole("radio").map((r) => r.querySelector("strong")?.firstChild?.textContent)).toEqual(["In one piece", "By parts", "My model has the same skeleton"]);
    await waitFor(() => expect(within(ways).getByRole("radio", { name: /By parts/ })).toHaveAttribute("aria-checked", "true"));
    expect(within(ways).getByRole("radio", { name: /By parts/ })).toHaveTextContent("suggested for this object");
    expect(within(ways).getByRole("radio", { name: /same skeleton/ })).toBeEnabled();
    expect(within(ways).getByRole("radio", { name: /same skeleton/ })).not.toHaveTextContent("Not offered yet.");
    expect(panel().getByText("3 separate objects found in the file: column, collar, bar.")).toBeInTheDocument();
    expect(panel().getByText("A part moves and turns with what it follows; it does not bend or stretch. The app suggests the nearest moving part; change any of them here.")).toBeInTheDocument();
    expect(panel().getByRole("combobox", { name: "bar follows" })).toHaveValue("main");
    const before = screen.getByTestId("saved").textContent;
    fireEvent.click(list.getByRole("button", { name: /Healing canister/ }));
    expect(screen.getByTestId("saved").textContent).toBe(before);
    fireEvent.change(panel().getByRole("combobox", { name: "collar follows" }), { target: { value: "full_top" } });
    expect(rows()![0].follow).toEqual({ way: "by_parts", parts: { collar: "full_top" } });
    fireEvent.click(within(ways).getByRole("radio", { name: /In one piece/ }));
    expect(rows()![0].follow).toEqual({ way: "one_piece" });
    // Undo takes back the way, one step: the part's bone stays as it was chosen.
    act(() => controls().undo());
    expect(rows()![0].follow).toEqual({ way: "by_parts", parts: { collar: "full_top" } });
    act(() => controls().undo());
    expect(rows()![0].follow).toBeUndefined();
    // The player under the view: the object's animations by readable name.
    const animation = await screen.findByRole("group", { name: "Animation" });
    expect(within(animation).getByRole("button", { name: "Deploy" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("button", { name: /^(Pause|Play)$/ })).toBeInTheDocument();
    expect(screen.getByText(/^The game's own animations of this object, played on your model.*The game plays it once and holds its last pose\.$/)).toBeInTheDocument();
    expect(panel().getByText(/Drain liquid \(1\.0 s, played over the others\)/)).toBeInTheDocument();
    expect(panel().getByText("Its animations, and when the game plays them")).toBeInTheDocument();
  });
  it("never remounts the 3D view across the way, a part's bone, the animation and play / pause", async () => {
    native.abilityObjectMotion.mockImplementation(async (_p, _t, clip) => motionAnswer(clip ?? undefined));
    await open({ auto: true, start: start() });
    const ways = await panel().findByRole("radiogroup", { name: "How your model moves" });
    await screen.findByTestId("object-scene");
    const animation = await screen.findByRole("group", { name: "Animation" });
    expect(scene.mounts).toBe(1);
    fireEvent.change(panel().getByRole("combobox", { name: "collar follows" }), { target: { value: "full_top" } });
    fireEvent.click(within(ways).getByRole("radio", { name: /In one piece/ }));
    fireEvent.click(within(ways).getByRole("radio", { name: /By parts/ }));
    fireEvent.click(within(animation).getByRole("button", { name: "Drain liquid" }));
    await waitFor(() => expect(native.abilityObjectMotion).toHaveBeenLastCalledWith(expect.anything(), CANISTER, "x/healing_canister_drain_liquid.vnmclip_c"));
    await waitFor(() => expect(within(screen.getByRole("group", { name: "Animation" })).getByRole("button", { name: "Drain liquid" })).toHaveAttribute("aria-pressed", "true"));
    const toggle = screen.getByRole("button", { name: /^(Pause|Play)$/ });
    const first = toggle.textContent;
    fireEvent.click(toggle);
    expect(screen.getByRole("button", { name: /^(Pause|Play)$/ }).textContent).not.toBe(first);
    fireEvent.click(screen.getByRole("button", { name: /^(Pause|Play)$/ }));
    await act(async () => {});
    expect(scene.mounts).toBe(1);
  });
  it("offers a states object the two ways that always build, with no bone to pick and the app's line", async () => {
    native.abilityObjectCatalog.mockImplementation(async () => ({ ...deadmanDannyObjects(), hero_path: haze.original_model }));
    native.abilityObjectFit.mockImplementation(async (_p, _t, source) => missileAnswer(source ? blockPacket() : null));
    const list = await open({ auto: true, start: withRows([{ target: MISSILE, enabled: true, source: "C:\art\parts.gltf", fit: {} }]) });
    expect(await list.findByRole("button", { name: /Missile/ })).toHaveTextContent(MISSILE_KIND.label);
    const ways = await panel().findByRole("radiogroup", { name: "How your model moves" });
    await waitFor(() => expect(within(ways).getByRole("radio", { name: /By parts/ })).toHaveAttribute("aria-checked", "true"));
    // The three ways stay, the same skeleton with its existing help.
    expect(within(ways).getAllByRole("radio").map((r) => r.querySelector("strong")?.firstChild?.textContent)).toEqual(["In one piece", "By parts", "My model has the same skeleton"]);
    expect(within(ways).getByRole("radio", { name: /same skeleton/ })).toBeEnabled();
    // By parts: the parts are named, the app's line replaces the bone pick.
    expect(panel().getByText("2 separate objects found in the file: lower, upper.")).toBeInTheDocument();
    expect(panel().getByText(PER_STATE_LINE)).toBeInTheDocument();
    expect(panel().queryByRole("combobox")).toBeNull();
    expect(panel().getByText("A part moves and turns with what it follows; it does not bend or stretch.")).toBeInTheDocument();
    expect(panel().queryByText(/change any of them here/)).toBeNull();
    // In one piece: no "Follows" pick either; the choice is one step.
    fireEvent.click(within(ways).getByRole("radio", { name: /In one piece/ }));
    expect(rows()![0].follow).toEqual({ way: "one_piece" });
    expect(panel().queryByRole("combobox")).toBeNull();
    expect(panel().getByText(PER_STATE_LINE)).toBeInTheDocument();
    // The same skeleton uses the file's own weights: the line does not apply there.
    fireEvent.click(within(ways).getByRole("radio", { name: /same skeleton/ }));
    expect(rows()![0].follow).toEqual({ way: "same_skeleton" });
    expect(panel().queryByText(PER_STATE_LINE)).toBeNull();
    const kept = panel().getAllByRole("listitem").map((li) => li.textContent);
    for (const line of MISSILE_KIND.kept) expect(kept).toContain(line);
  });
  it("leaves the first kind and the round-2 kinds as they were: no ways, no player", async () => {
    native.abilityObjectCatalog.mockImplementation(async () => hazeObjects());
    const list = await open();
    await list.findByRole("button", { name: "Dagger" });
    expect(screen.queryByRole("radiogroup", { name: "How your model moves" })).toBeNull();
    expect(screen.queryByRole("group", { name: "Animation" })).toBeNull();
    expect(native.abilityObjectMotion).not.toHaveBeenCalled();
  });
});

describe("the player of an object whose animation the build keeps as the game's own", () => {
  const still = () => {
    const json = new TextEncoder().encode(JSON.stringify({ mime_type: "application/x-dmb-object-motion", details: { bones: [], clips: [], fps: 30, frames: 0, note: OBJECT_MOTION_NOT_READ } }));
    const out = new Uint8Array(4 + json.length);
    new DataView(out.buffer).setUint32(0, json.length, true);
    out.set(json, 4);
    return out.buffer;
  };
  it("says calmly that the game's animation is kept and cannot be played here", async () => {
    const motion = vi.fn();
    render(<ObjectPlayer target="models/abilities/sample_trap.vmdl" load={async () => still()} onMotion={motion} onFrame={() => {}} onClips={() => {}} keptAnimation />);
    expect((await screen.findByRole("status")).textContent).toBe(OBJECT_MOTION_KEPT);
    expect(screen.queryByRole("alert")).toBeNull();
    expect(motion).toHaveBeenCalledWith(null);
    cleanup();
    render(<ObjectPlayer target="models/abilities/sample_trap.vmdl" load={async () => still()} onMotion={() => {}} onFrame={() => {}} onClips={() => {}} />);
    expect((await screen.findByRole("status")).textContent).toBe(OBJECT_MOTION_NOT_READ);
  });
  it("says the game's own object plays until a file is chosen, and your model once it plays", async () => {
    const load = async () => motionAnswer();
    render(<ObjectPlayer target="models/abilities/sample_trap.vmdl" load={load} onMotion={() => {}} onFrame={() => {}} onClips={() => {}} />);
    expect(await screen.findByText(/^The game's own animations of this object\. Choose a file to see them played on your model\./)).toBeInTheDocument();
    expect(screen.queryByText(/played on your model, so you can see/)).toBeNull();
    cleanup();
    render(<ObjectPlayer target="models/abilities/sample_trap.vmdl" load={load} onMotion={() => {}} onFrame={() => {}} onClips={() => {}} plays="game" />);
    expect(await screen.findByText(/^The game's own animations of this object, played on the game's object\./)).toBeInTheDocument();
    cleanup();
    render(<ObjectPlayer target="models/abilities/sample_trap.vmdl" load={load} onMotion={() => {}} onFrame={() => {}} onClips={() => {}} plays="yours" />);
    expect(await screen.findByText(/^The game's own animations of this object, played on your model, so you can see each part move before you build\./)).toBeInTheDocument();
  });
});