// Extras → Effects, Advanced: each part's kind, its Shown / Hidden switch and a 3D part's own
// settings ("This part"). The Simple view renders none of it and saves what it always saved.
import { useState } from "react";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Project } from "../models";
import type { ComponentCatalog } from "../components/AssetCatalog";
import { ExtrasWorkspace } from "./ExtrasPage";
import { clearExtraMediaCache } from "./media";
import { forgetTrees } from "./effectTreesLoad";
import { HistoryContext, type HistoryControls } from "../history/useProjectHistory";
import { DAGGER, DAGGER_GLOW, DAGGER_LIGHT, DAGGER_MODEL, effectTreesWithControls, previewAll } from "./effectFixtures";
import { advancedText, changesSomething, cleanEdit, withPartSetting } from "./effectEdits";
import { LIGHT_MAX, LIGHT_MIN } from "./effectTypes";
import reference from "../../src-tauri/crates/deadlock-project/tests/fixtures/reference.json";

const native = vi.hoisted(() => ({
  validateAssetPath: vi.fn(),
  previewExtra: vi.fn(),
  discoverExtras: vi.fn(),
  effectTrees: vi.fn(),
  effectPreview: vi.fn(),
  cancel: vi.fn(),
}));
vi.mock("../api", async (original) => ({ ...(await original<typeof import("../api")>()), api: native }));

const initial = {
  ...reference.cases[1].expected,
  hero_name: "Test hero",
  customization: { enabled: true },
} as unknown as Project;
const catalog: ComponentCatalog = { hero_path: initial.original_model, icons: [], audio: [], effects: [], animations: [], variants: [], animation_unavailable: null, warnings: [] };
const dagger = effectTreesWithControls.trees.find((t) => t.target === DAGGER)!;
beforeEach(() => {
  vi.clearAllMocks();
  forgetTrees();
  clearExtraMediaCache();
  localStorage.clear();
  native.validateAssetPath.mockImplementation(async (p) => p);
  native.previewExtra.mockRejectedValue({ message: "no preview" });
  native.discoverExtras.mockResolvedValue(catalog);
  native.effectTrees.mockResolvedValue(effectTreesWithControls);
  native.cancel.mockResolvedValue(undefined);
  native.effectPreview.mockImplementation(async (_p, target, _edit, pick) =>
    previewAll(effectTreesWithControls.trees.find((t) => t.target === target)!, [125, 77, 255], pick ? { pick: { hue: 90, saturation: 1, brightness: 1, result: [125, 77, 255], result_hex: "#7D4DFF", exact: true } } : {}),
  );
});
function Harness({ start = initial, history }: { start?: Project; history?: HistoryControls }) {
  const [p, set] = useState(structuredClone(start));
  const page = (
    <>
      <ExtrasWorkspace project={p} update={set} disabled={false} catalog={catalog} fail={vi.fn()} reviewBuild={vi.fn()} discover={async () => undefined} />
      <output data-testid="saved">{JSON.stringify(p)}</output>
    </>
  );
  return history ? <HistoryContext.Provider value={history}>{page}</HistoryContext.Provider> : page;
}
const saved = () => JSON.parse(screen.getByTestId("saved").textContent!) as Project;
async function openDagger(start?: Project, history?: HistoryControls) {
  render(<Harness start={start} history={history} />);
  fireEvent.click(screen.getByRole("tab", { name: /Effects/ }));
  fireEvent.click(await screen.findByRole("option", { name: /Dagger/ }));
  await screen.findByRole("heading", { name: "Sleep · dagger" });
}
const view = () => within(screen.getByRole("region", { name: "Selected effect" }));
const editor = () => within(screen.getByRole("complementary", { name: "Effect editor" }));
const rowOf = (label: string) => within(view().getByText(label, { selector: "strong" }).closest("li")!);
const modelRow = () => rowOf("Dagger model");
const rowEdit = () => saved().customization.effect_edits?.[0]?.edit;
function typeValue(box: HTMLElement, text: string) {
  fireEvent.focus(box);
  fireEvent.change(box, { target: { value: text } });
  fireEvent.keyDown(box, { key: "Enter" });
}

describe("Extras → Effects: a part's own settings", () => {
  it("Simple shows none of them and saves exactly what it saved before", async () => {
    await openDagger();
    expect(view().queryByRole("button", { name: /^(Shown|Hidden):/ })).toBeNull();
    expect(view().queryByRole("button", { name: /This part/ })).toBeNull();
    expect(view().queryByText("3D model")).toBeNull();
    expect(view().queryByText(/its brightness cannot be changed yet/)).toBeNull();
    expect(editor().queryByText(/has its own Size/)).toBeNull();
    fireEvent.click(editor().getByRole("button", { name: "Neon purple #7D4DFF" }));
    await waitFor(() => expect(saved().customization.effect_edits).toEqual([{ target: DAGGER, edit: { hue: 90 } }]));
  });

  it("a Simple edit keeps the per-part settings a row already has", async () => {
    const parts = { [DAGGER_GLOW]: { hidden: true } };
    await openDagger({ ...initial, customization: { ...initial.customization, effect_edits: [{ target: DAGGER, edit: { parts } }] } } as Project);
    expect(editor().getByRole("note")).toHaveTextContent("Advanced settings are active: 1 part hidden.");
    fireEvent.click(editor().getByRole("button", { name: "Neon purple #7D4DFF" }));
    await waitFor(() => expect(rowEdit()).toEqual({ hue: 90, parts }));
  });

  it("hides a part with its switch: a row of its own, the strips dimmed and a line that says so", async () => {
    localStorage.setItem("dmb.effectsMode", "advanced");
    await openDagger();
    expect(view().getAllByText("Glow sprites", { selector: ".effects-kind" }).length).toBe(2);
    expect(view().getByText("3D model", { selector: ".effects-kind" })).toBeVisible();
    const glow = view().getByRole("button", { name: "Shown: Glow" });
    fireEvent.click(glow);
    expect(rowEdit()).toEqual({ parts: { [DAGGER_GLOW]: { hidden: true } } });
    expect(view().getByRole("button", { name: "Hidden: Glow" })).toHaveAttribute("aria-pressed", "true");
    expect(view().getByText("Hidden: this part is not drawn. The rest of the effect is unchanged.")).toBeVisible();
    expect(view().getByText("1 part hidden")).toBeVisible();
    fireEvent.click(view().getByRole("button", { name: "Hidden: Glow" }));
    expect(saved().customization.effect_edits).toBeUndefined();
    // The top holds the other parts: hiding it says they keep working.
    fireEvent.click(view().getByRole("button", { name: "Shown: Dagger" }));
    expect(view().getByText("Hidden: this part is not drawn. The rest of the effect is unchanged; the parts below it keep working.")).toBeVisible();
  });

  it("gives a 3D part its own Size, Opacity and Tint, says what the preview cannot show, and resets it", async () => {
    localStorage.setItem("dmb.effectsMode", "advanced");
    await openDagger();
    expect(view().getByText("This part has no color of its own in the game's files.")).toBeVisible();
    expect(editor().getByText(/reaches 3 of 4 parts · the 3D part has its own Size/)).toBeVisible();
    fireEvent.click(modelRow().getByRole("button", { name: /This part/ }));
    const panel = within(modelRow().getByRole("group", { name: "This part" }));
    // With the Tint right below, the line above it goes; the panel says why the Tint is there.
    expect(view().queryByText("This part has no color of its own in the game's files.")).toBeNull();
    expect(panel.getByText(/The game's files give this part no color of its own; the tint colors the whole object/)).toBeVisible();
    typeValue(panel.getByRole("textbox", { name: "Dagger model size value" }), "300%");
    expect(rowEdit()).toEqual({ parts: { [DAGGER_MODEL]: { size: 3 } } });
    typeValue(panel.getByRole("textbox", { name: "Dagger model opacity value" }), "20%");
    expect(rowEdit()).toEqual({ parts: { [DAGGER_MODEL]: { size: 3, opacity: 0.2 } } });
    expect(panel.getAllByText("The moving preview does not show this.")).toHaveLength(1);
    const hex = panel.getByRole("textbox", { name: "Tint hex color" });
    typeValue(hex, "#9628FF");
    fireEvent.blur(hex);
    await waitFor(() => expect(rowEdit()).toEqual({ parts: { [DAGGER_MODEL]: { size: 3, opacity: 0.2, tint: [150, 40, 255] } } }));
    fireEvent.click(panel.getByRole("button", { name: "Remove tint" }));
    expect(rowEdit()).toEqual({ parts: { [DAGGER_MODEL]: { size: 3, opacity: 0.2 } } });
    // Back to 100% is not saved.
    typeValue(panel.getByRole("textbox", { name: "Dagger model opacity value" }), "100%");
    expect(rowEdit()).toEqual({ parts: { [DAGGER_MODEL]: { size: 3 } } });
    fireEvent.click(panel.getByRole("button", { name: "Reset this part" }));
    expect(saved().customization.effect_edits).toBeUndefined();
  });

  it("a light part follows the effect's colors and has its own Light brightness, 0 to 50", async () => {
    localStorage.setItem("dmb.effectsMode", "advanced");
    await openDagger();
    expect(view().getByText("A light on nearby surfaces. It takes the effect's colors.")).toBeVisible();
    const light = rowOf("Hit light");
    expect(light.getByRole("button", { name: "Shown: Hit light" })).toBeVisible();
    fireEvent.click(light.getByRole("button", { name: /This part/ }));
    const panel = within(light.getByRole("group", { name: "This part" }));
    // A light shows no 3D settings, only its brightness, with what stays as it is.
    expect(panel.queryByText(/^Size:/)).toBeNull();
    expect(panel.queryByText(/3D parts are not reached/)).toBeNull();
    expect(panel.getByText("1 of its 2 lights stays as it is: the game sets this light's brightness while it plays.")).toBeVisible();
    expect(panel.getByText(/At 0 the light is dimmer but may not go out; to remove it, hide the part\./)).toBeVisible();
    const box = panel.getByRole("textbox", { name: "Hit light light brightness value" });
    expect(box).toHaveValue("100%");
    typeValue(box, "5000%");
    expect(rowEdit()).toEqual({ parts: { [DAGGER_LIGHT]: { light: 50 } } });
    expect(panel.getByText("The moving preview does not show this.")).toBeVisible();
    // 0 is kept (it dims); more than 50 is held at 50; 1 is not saved.
    typeValue(box, "0%");
    expect(rowEdit()).toEqual({ parts: { [DAGGER_LIGHT]: { light: 0 } } });
    typeValue(box, "9000%");
    expect(rowEdit()).toEqual({ parts: { [DAGGER_LIGHT]: { light: 50 } } });
    typeValue(box, "100%");
    expect(saved().customization.effect_edits).toBeUndefined();
  });

  it("a light part whose brightness cannot be changed shows the reason and no control", async () => {
    localStorage.setItem("dmb.effectsMode", "advanced");
    const why = "the game sets this light's brightness while it plays";
    const trees = structuredClone(effectTreesWithControls);
    const hit = trees.trees.find((t) => t.target === DAGGER)!.parts.find((p) => p.path === DAGGER_LIGHT)!;
    hit.controls.light = { offered: false, reason: why, player_shows: false };
    hit.kind.note = `A light on nearby surfaces. It takes the effect's colors. Its brightness stays as it is: ${why}.`;
    native.effectTrees.mockResolvedValue(trees);
    await openDagger();
    const light = rowOf("Hit light");
    expect(light.getByText(hit.kind.note)).toBeVisible();
    expect(light.queryByRole("button", { name: /This part/ })).toBeNull();
    expect(light.queryByRole("slider", { name: /light brightness/ })).toBeNull();
  });

  it("Simple shows no Light brightness but counts a part that has one", async () => {
    await openDagger({ ...initial, customization: { ...initial.customization, effect_edits: [{ target: DAGGER, edit: { parts: { [DAGGER_LIGHT]: { light: 0 } } } }] } } as Project);
    expect(view().queryByRole("slider", { name: /light brightness/ })).toBeNull();
    expect(editor().getByRole("note")).toHaveTextContent("Advanced settings are active: 1 part has its own settings.");
  });

  it("a slider drag is one step of the project history", async () => {
    localStorage.setItem("dmb.effectsMode", "advanced");
    const gesture = vi.fn();
    const history: HistoryControls = { canUndo: false, canRedo: false, undo: vi.fn(), redo: vi.fn(), gesture, fence: vi.fn() };
    await openDagger(undefined, history);
    fireEvent.click(modelRow().getByRole("button", { name: /This part/ }));
    const slider = within(modelRow().getByRole("group", { name: "This part" })).getByRole("slider", { name: "Dagger model size" });
    fireEvent.pointerDown(slider);
    expect(gesture).toHaveBeenLastCalledWith(true);
    fireEvent.change(slider, { target: { value: "2" } });
    fireEvent.change(slider, { target: { value: "2.5" } });
    fireEvent.pointerUp(window);
    expect(gesture).toHaveBeenLastCalledWith(false);
    expect(rowEdit()).toEqual({ parts: { [DAGGER_MODEL]: { size: 2.5 } } });
  });
});

describe("effectEdits: the per-part map", () => {
  it("drops what changes nothing, keeps one entry per part case-blind and is absent when empty", () => {
    expect(cleanEdit({ hue: 30 })).toEqual({ hue: 30 });
    expect(cleanEdit({ parts: { a: { size: 1, opacity: 1, hidden: false } } })).toEqual({});
    expect(changesSomething({ parts: { a: { hidden: true } } })).toBe(true);
    expect(changesSomething({ parts: { a: { size: 1 } } })).toBe(false);
    expect(cleanEdit({ parts: { "P/B.vpcf_c": { size: 40, tint: [300, -2, 9.6] as never }, "p/b.vpcf_c": { hidden: true }, "p/a.vpcf_c": { opacity: 0.5 } } })).toEqual({
      parts: { "p/a.vpcf_c": { opacity: 0.5 }, "P/B.vpcf_c": { size: 16, tint: [255, 0, 10] } },
    });
    const one = withPartSetting({}, "p/a.vpcf_c", { hidden: true });
    expect(withPartSetting(one, "P/A.vpcf_c", { size: 2 }).parts).toEqual({ "P/A.vpcf_c": { hidden: true, size: 2 } });
    expect(withPartSetting(one, "p/a.vpcf_c", null).parts).toEqual({});
    expect(dagger.parts.length).toBe(4);
  });

  it("keeps Light brightness from 0 to 50, drops 1 and counts a part with it", () => {
    expect(cleanEdit({ parts: { a: { light: 1 } } })).toEqual({});
    expect(cleanEdit({ parts: { a: { light: 0 } } })).toEqual({ parts: { a: { light: 0 } } });
    expect(cleanEdit({ parts: { a: { light: 80 } } })).toEqual({ parts: { a: { light: 50 } } });
    expect(cleanEdit({ parts: { a: { light: -3 } } })).toEqual({ parts: { a: { light: 0 } } });
    expect(cleanEdit({ parts: { a: { light: Number.NaN } } })).toEqual({});
    expect(changesSomething({ parts: { a: { light: 0 } } })).toBe(true);
    expect(changesSomething({ parts: { a: { light: 1 } } })).toBe(false);
    expect(withPartSetting(withPartSetting({}, "a", { light: 20 }), "a", { light: undefined }).parts).toEqual({});
    expect(advancedText({ parts: { a: { light: 0 }, b: { hidden: true } } })).toBe("Advanced settings are active: 1 part hidden, 1 part has its own settings.");
    expect([LIGHT_MIN, LIGHT_MAX]).toEqual([0, 50]);
  });
});
