import { useState } from "react";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Project } from "../models";
import type { ComponentCatalog } from "../components/AssetCatalog";
import { ExtrasWorkspace } from "./ExtrasPage";
import { clearExtraMediaCache } from "./media";
import { forgetTrees } from "./effectTreesLoad";
import { EFFECTS_NOT_LOADED } from "./effectTypes";
import { BOMB, CORE, DEBRIS, OLD_LEAP, RED, SMOKE, TRACER, effectTrees, effectTreesWithOtherFiles, previewAll } from "./effectFixtures";
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
  customization: { enabled: true, physics: { enabled: true, source: "cloth.vmdl_c" } },
} as unknown as Project;
const catalog: ComponentCatalog = {
  hero_path: initial.original_model, icons: [], audio: [],
  effects: [{ path: SMOKE, target: SMOKE, label: "Smoke file", supported: true, original_sha256: "e".repeat(64) }],
  animations: [], variants: [], animation_unavailable: null, warnings: [],
};
beforeEach(() => {
  vi.clearAllMocks();
  forgetTrees();
  clearExtraMediaCache();
  localStorage.clear();
  native.validateAssetPath.mockImplementation(async (p) => p);
  native.previewExtra.mockRejectedValue({ message: "no preview" });
  native.discoverExtras.mockResolvedValue(catalog);
  native.effectTrees.mockResolvedValue(effectTrees);
  native.cancel.mockResolvedValue(undefined);
  native.effectPreview.mockImplementation(async (_p, target, _edit, pick) =>
    previewAll(effectTrees.trees.find((t) => t.target === target)!, [125, 77, 255], pick ? { pick: { hue: 90, saturation: 1, brightness: 1, result: [125, 77, 255], result_hex: "#7D4DFF", exact: true } } : {}),
  );
});
function Harness({ start = initial }: { start?: Project }) {
  const [p, set] = useState(structuredClone(start));
  return (
    <>
      <ExtrasWorkspace project={p} update={set} disabled={false} catalog={catalog} fail={vi.fn()} reviewBuild={vi.fn()} discover={async () => undefined} />
      <output data-testid="saved">{JSON.stringify(p)}</output>
    </>
  );
}
const saved = () => JSON.parse(screen.getByTestId("saved").textContent!) as Project;
async function openEffects(start?: Project) {
  render(<Harness start={start} />);
  fireEvent.click(screen.getByRole("tab", { name: /Effects/ }));
  await screen.findByRole("option", { name: /Explode/ });
}
const editor = () => within(screen.getByRole("complementary", { name: "Effect editor" }));
function typeValue(box: HTMLElement, text: string) {
  fireEvent.focus(box);
  fireEvent.change(box, { target: { value: text } });
  fireEvent.keyDown(box, { key: "Enter" });
}

describe("Extras → Effects", () => {
  it("says quietly when an effect or a whole group is there by its file name only", async () => {
    const byName = { ...effectTrees, trees: effectTrees.trees.map((t) => (t.target === TRACER ? { ...t, grouped_by: "name" as const } : t)) };
    native.effectTrees.mockResolvedValue(byName);
    native.effectPreview.mockImplementation(async (_p, target) => previewAll(byName.trees.find((t) => t.target === target)!, [125, 77, 255]));
    await openEffects();
    const list = within(screen.getByRole("listbox", { name: "Effects" }));
    // The bomb is linked by the game's data: no line, in its group or on it.
    expect(within(list.getByRole("group", { name: "Big Bomb" })).queryByText("Grouped by their file names.")).toBeNull();
    const view = within(screen.getByRole("region", { name: "Selected effect" }));
    expect(view.queryByText(/Grouped by its file name/)).toBeNull();
    // The weapon's only effect is there by its name: the group says so once.
    expect(within(list.getByRole("group", { name: "Weapon · Test Gun" })).getByText("Grouped by their file names.")).toBeVisible();
    fireEvent.click(list.getByRole("option", { name: /Tracer/ }));
    expect(await view.findByText("Grouped by its file name: the game's data does not link it to Weapon.")).toBeVisible();
  });

  it("loads the trees from the catalog the backend holds and shows groups, parts, strips and notes", async () => {
    await openEffects();
    // The backend answers from the catalog it holds: no new discovery.
    expect(native.effectTrees).toHaveBeenCalledTimes(1);
    expect(native.discoverExtras).not.toHaveBeenCalled();
    const list = within(screen.getByRole("listbox", { name: "Effects" }));
    expect(list.getByRole("group", { name: "Big Bomb" })).toBeVisible();
    expect(list.getByRole("group", { name: "Weapon · Test Gun" })).toBeVisible();
    const view = within(screen.getByRole("region", { name: "Selected effect" }));
    expect(view.getByText("Also used by: Another ability (Other hero). Your change shows there too.")).toBeVisible();
    expect(view.getByRole("img", { name: "Game: #1E2120 at 20%, #C81E28 (50% opaque) at 100%" })).toBeVisible();
    expect(view.getByText("May keep its colors: a 3D model drawn without the particle color (its tint is switched off).")).toBeVisible();
    expect(view.queryByText(/This part keeps its own colors: a model/)).toBeNull();
    expect(view.getByText(/Also part of Weapon · tracer/)).toBeVisible();
    expect(view.getByText(/at most 5% of the screen/)).toBeVisible();
    expect(view.getByText("Draws nothing itself; holds the parts below")).toBeVisible();
    expect(view.getByText("Size, Brightness and Opacity do not reach it: a 3D model has no such setting.")).toBeVisible();
    expect(view.getByText("Opacity does not reach it: the game sets it while the effect plays.")).toBeVisible();
    expect(list.getByRole("option", { name: /Explode/ })).toHaveTextContent("Also used");
    expect(screen.getByText("One effect file could not be read.")).toBeInTheDocument();
    // Nothing to preview yet: an unchanged effect is drawn from the trees alone.
    expect(native.effectPreview).not.toHaveBeenCalled();
  });

  it("maps the main color onto a preset through the backend's pick and saves the shift", async () => {
    await openEffects();
    fireEvent.click(editor().getByRole("button", { name: "Neon purple #7D4DFF" }));
    await waitFor(() => expect(saved().customization.effect_edits).toEqual([{ target: BOMB, edit: { hue: 90 } }]));
    expect(native.effectPreview).toHaveBeenCalledWith(expect.anything(), BOMB, {}, { color: [125, 77, 255], main: null }, null);
    expect(screen.getByText("1 changed")).toBeVisible();
    expect(screen.getByText("1 change")).toBeVisible();
    expect(screen.getByRole("tab", { name: /Effects/ }).querySelector("i")).not.toBeNull();
    expect(within(screen.getByRole("listbox", { name: "Effects" })).getByRole("option", { name: /Explode/ })).toHaveTextContent("Changed");
    await waitFor(() => expect(screen.getAllByRole("img", { name: /^Yours: #7D4DFF/ }).length).toBeGreaterThan(0));
  });

  it("says in plain words when the main color cannot reach the pick", async () => {
    native.effectPreview.mockImplementation(async (_p, target, _e, pick) =>
      previewAll(effectTrees.trees.find((t) => t.target === target)!, [10, 10, 10], pick ? { pick: { hue: 0, saturation: 1, brightness: 0.1, result: [10, 10, 10], result_hex: "#0A0A0A", exact: false, reason: "the main color is already as dark as it goes", warning: "Black makes glowing parts invisible." } } : {}),
    );
    await openEffects();
    fireEvent.click(editor().getByRole("button", { name: "Black" }));
    expect(await screen.findByText(/The main color can only get to #0A0A0A: the main color is already as dark as it goes/)).toBeVisible();
    expect(screen.getByText("Black makes glowing parts invisible.")).toBeVisible();
  });

  it("uses a chosen main color for the next pick and saves it with the row", async () => {
    await openEffects();
    fireEvent.click(editor().getByRole("button", { name: "Not the main color?" }));
    fireEvent.click(editor().getByRole("button", { name: "Use #C81E28 as the main color" }));
    expect(saved().customization.effect_edits).toBeUndefined();
    fireEvent.click(editor().getByRole("button", { name: "Blue #408CFF" }));
    await waitFor(() => expect(saved().customization.effect_edits).toEqual([{ target: BOMB, edit: { hue: 90 }, main: RED }]));
    expect(native.effectPreview).toHaveBeenLastCalledWith(expect.anything(), BOMB, {}, { color: [64, 140, 255], main: RED }, RED);
  });

  it("Advanced: leaves a part out, shifts exactly, and Simple then names what Advanced set", async () => {
    await openEffects();
    fireEvent.click(editor().getByRole("radio", { name: "Advanced" }));
    expect(localStorage.getItem("dmb.effectsMode")).toBe("advanced");
    fireEvent.click(screen.getByRole("checkbox", { name: "Recolor Smoke" }));
    // Leaving a part out changes nothing by itself: no row yet, but the tick stays.
    expect(saved().customization.effect_edits).toBeUndefined();
    expect(screen.getByRole("checkbox", { name: "Recolor Smoke" })).not.toBeChecked();
    expect(screen.getByText("Left out: keeps the game's look (its colors, size, brightness and opacity).")).toBeVisible();
    typeValue(editor().getByRole("textbox", { name: "Shift hue value" }), "30");
    expect(saved().customization.effect_edits).toEqual([{ target: BOMB, edit: { hue: 30, excluded: [SMOKE] } }]);
    await waitFor(() => expect(native.effectPreview).toHaveBeenCalledWith(expect.anything(), BOMB, { hue: 30, excluded: [SMOKE] }, null, null));
    fireEvent.click(editor().getByRole("radio", { name: "Simple" }));
    expect(editor().getByRole("note")).toHaveTextContent("Advanced settings are active: 1 part left out.");
    expect(saved().customization.effect_edits![0].edit).toEqual({ hue: 30, excluded: [SMOKE] });
  });

  it("Advanced: gives one palette color its own color, near colors too, and back", async () => {
    localStorage.setItem("dmb.effectsMode", "advanced");
    await openEffects();
    fireEvent.click(within(editor().getByRole("group", { name: "Colors in this effect" })).getByRole("button", { name: /^#C81E28/ }));
    typeValue(editor().getByRole("textbox", { name: "Replacement color hex color" }), "#00FF00");
    fireEvent.blur(editor().getByRole("textbox", { name: "Replacement color hex color" }));
    await waitFor(() => expect(saved().customization.effect_edits).toEqual([{ target: BOMB, edit: { colors: [{ from: RED, to: [0, 255, 0] }] } }]));
    fireEvent.click(editor().getByRole("checkbox", { name: "Also change colors close to it" }));
    expect(saved().customization.effect_edits![0].edit!.colors).toEqual([{ from: RED, to: [0, 255, 0], near: true }]);
    fireEvent.click(editor().getByRole("button", { name: "Follow the whole-effect change" }));
    expect(saved().customization.effect_edits).toBeUndefined();
  });

  it("multipliers say what they reach; opacity is off with a reason", async () => {
    await openEffects();
    expect(editor().getByText("reaches 1 of 2 parts")).toBeVisible();
    expect(editor().getByRole("slider", { name: "Effect opacity" })).toBeDisabled();
    expect(editor().getByText("Its parts are 3D models, ground decals or lights, which have no opacity setting.")).toBeVisible();
    typeValue(editor().getByRole("textbox", { name: "Effect size value" }), "150%");
    expect(saved().customization.effect_edits).toEqual([{ target: BOMB, edit: { size: 1.5 } }]);
  });

  it("includes, pauses, resets one effect and resets all after a confirmation", async () => {
    const start = { ...initial, customization: { ...initial.customization, effect_edits: [{ target: BOMB, edit: { hue: 20 } }, { target: TRACER, edit: { glow: 2 } }] } } as Project;
    await openEffects(start);
    fireEvent.click(editor().getByRole("checkbox", { name: "Include in build" }));
    expect(saved().customization.effect_edits![0]).toEqual({ target: BOMB, enabled: false, edit: { hue: 20 } });
    expect(within(screen.getByRole("listbox", { name: "Effects" })).getByRole("option", { name: /Explode/ })).toHaveTextContent("Paused");
    fireEvent.click(screen.getByRole("button", { name: "Pause all extras" }));
    expect(saved().customization.effect_edits!.every((r) => r.enabled === false)).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "Resume all extras" }));
    expect(saved().customization.effect_edits!.every((r) => r.enabled === undefined)).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "Pause all extras" }));
    fireEvent.click(editor().getByRole("button", { name: "Reset effect" }));
    expect(saved().customization.effect_edits).toEqual([{ target: TRACER, enabled: false, edit: { glow: 2 } }]);
    fireEvent.click(screen.getByRole("button", { name: "Reset all…" }));
    fireEvent.click(within(screen.getByRole("dialog", { name: "Reset all effects" })).getByRole("button", { name: "Reset all" }));
    expect("effect_edits" in saved().customization).toBe(false);
  });

  it("shows a failure as one alert line with Try again, and a cancel as no failure", async () => {
    const refused = { message: "Load this character's assets first.", help: { cause: "x", steps: ["Press Reload assets."], action: null } };
    // Refused before and after the discovery it leads to.
    native.effectTrees.mockRejectedValueOnce(refused).mockRejectedValueOnce(refused);
    render(<Harness />);
    fireEvent.click(screen.getByRole("tab", { name: /Effects/ }));
    expect(await screen.findByRole("alert")).toHaveTextContent("The effects could not be read. Load this character's assets first. Press Reload assets.");
    const list = within(screen.getByRole("region", { name: "Effects" }));
    expect(list.getByRole("button", { name: "Reload assets" })).toBeVisible();
    fireEvent.click(list.getByRole("button", { name: "Try again" }));
    await screen.findByRole("option", { name: /Explode/ });
    forgetTrees();
    native.effectTrees.mockRejectedValueOnce({ kind: "cancelled", message: "Operation cancelled" });
    fireEvent.click(screen.getAllByRole("button", { name: "Reload assets" })[0]);
    expect(await screen.findByText("Loading was cancelled.")).toBeVisible();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("asks for the trees again on every opening (the backend's previews need them), showing the last ones meanwhile", async () => {
    const view = render(<Harness />);
    fireEvent.click(screen.getByRole("tab", { name: /Effects/ }));
    await screen.findByRole("option", { name: /Explode/ });
    view.unmount();
    native.effectTrees.mockReturnValueOnce(new Promise(() => {}));
    render(<Harness />);
    fireEvent.click(screen.getByRole("tab", { name: /Effects/ }));
    expect(screen.getByRole("option", { name: /Explode/ })).toBeVisible();
    await waitFor(() => expect(native.effectTrees).toHaveBeenCalledTimes(2));
    expect(native.discoverExtras).not.toHaveBeenCalled();
  });

  it("discovers the hero's assets and asks once more only when the backend refuses the trees", async () => {
    native.effectTrees.mockRejectedValueOnce({ message: "Load this character’s assets first" });
    await openEffects();
    expect(native.effectTrees).toHaveBeenCalledTimes(2);
    expect(native.discoverExtras).toHaveBeenCalledTimes(1);
    expect(native.effectTrees.mock.invocationCallOrder[0]).toBeLessThan(native.discoverExtras.mock.invocationCallOrder[0]);
    expect(native.discoverExtras.mock.invocationCallOrder[0]).toBeLessThan(native.effectTrees.mock.invocationCallOrder[1]);
  });

  it("does not discover again when the trees' load was cancelled or the backend was busy", async () => {
    native.effectTrees.mockRejectedValueOnce({ kind: "busy", message: "Another operation is running" });
    render(<Harness />);
    fireEvent.click(screen.getByRole("tab", { name: /Effects/ }));
    expect(await screen.findByText("Another operation was running. Load them again when it is done.")).toBeVisible();
    expect(native.effectTrees).toHaveBeenCalledTimes(1);
    expect(native.discoverExtras).not.toHaveBeenCalled();
  });

  it("keeps a load's failure when the page is disabled while it runs", async () => {
    let fail: (e: unknown) => void = () => undefined;
    native.effectTrees.mockReturnValueOnce(new Promise((_, reject) => { fail = reject; }));
    const props = { project: initial, update: vi.fn(), catalog, fail: vi.fn(), discover: async () => undefined };
    const view = render(<ExtrasWorkspace {...props} disabled={false} />);
    fireEvent.click(screen.getByRole("tab", { name: /Effects/ }));
    await waitFor(() => expect(native.effectTrees).toHaveBeenCalledTimes(1));
    view.rerender(<ExtrasWorkspace {...props} disabled />);
    // The second ask, after the discovery the refusal leads to, fails too.
    native.effectTrees.mockRejectedValueOnce({ message: "The game archive could not be read." });
    await act(async () => fail({ message: "The game archive could not be read." }));
    view.rerender(<ExtrasWorkspace {...props} disabled={false} />);
    expect(await screen.findByRole("alert")).toHaveTextContent("The game archive could not be read.");
    expect(native.effectTrees).toHaveBeenCalledTimes(2);
    expect(native.discoverExtras).toHaveBeenCalledTimes(1);
  });

  it("reloads the trees and previews again when the backend lost its effect index", async () => {
    const start = { ...initial, customization: { ...initial.customization, effect_edits: [{ target: BOMB, edit: { hue: 20 } }] } } as Project;
    native.effectPreview.mockRejectedValueOnce({ message: EFFECTS_NOT_LOADED });
    await openEffects(start);
    await waitFor(() => expect(native.effectTrees).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(native.effectPreview).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(screen.queryByText(/The preview could not be made/)).toBeNull());
  });

  it("cancels a slow first load from the page", async () => {
    native.effectTrees.mockReturnValue(new Promise(() => {}));
    render(<Harness />);
    fireEvent.click(screen.getByRole("tab", { name: /Effects/ }));
    fireEvent.click(await screen.findByRole("button", { name: "Cancel" }));
    expect(native.cancel).toHaveBeenCalled();
  });

  it("keeps the old Replace a file view and names files changed on both sides", async () => {
    const start = { ...initial, customization: { ...initial.customization, effect_edits: [{ target: BOMB, edit: { hue: 20 } }], effects: [{ enabled: true, source: "C:/mods/smoke.vpcf_c", target: SMOKE, allow_shared: false }] } } as Project;
    await openEffects(start);
    expect(screen.getByText(/Your own file is used for Smoke \(Replace a file\); this edit changes the effect's other parts/)).toBeVisible();
    expect(screen.getByText(/Your own file is used for this part \(Replace a file\)/)).toBeVisible();
    fireEvent.click(screen.getByRole("radio", { name: "Replace a file" }));
    const library = within(screen.getByRole("region", { name: "Effects assets" }));
    expect(library.getByRole("button", { name: /Smoke file/ })).toBeVisible();
    expect(screen.getByText(/Your file is used for/)).toHaveTextContent("Your file is used for Smoke; the color edit of Big Bomb · explode changes that effect's other parts.");
    fireEvent.click(library.getByRole("button", { name: /Smoke file/ }));
    expect(screen.getByRole("button", { name: "Remove replacement" })).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "Remove replacement" }));
    expect(saved().customization.effects).toEqual([]);
    fireEvent.click(screen.getByRole("radio", { name: "Edit colors" }));
    await screen.findByRole("option", { name: /Explode/ });
    expect(screen.queryByText(/Your own file is used for/)).toBeNull();
    act(() => undefined);
  });

  it("states which edit is used for a part two edited effects share: the one changed first", async () => {
    const start = { ...initial, customization: { ...initial.customization, effect_edits: [{ target: TRACER, edit: { glow: 2 } }, { target: BOMB, edit: { hue: 20 } }] } } as Project;
    await openEffects(start);
    expect(screen.getByText("Also part of Weapon · tracer, which was changed first: that effect's edit is used for this part.")).toBeVisible();
  });

  it("leaves parts other heroes use out until they are ticked in Advanced", async () => {
    await openEffects();
    fireEvent.click(within(screen.getByRole("listbox", { name: "Effects" })).getByRole("option", { name: /Tracer/ }));
    const view = () => within(screen.getByRole("region", { name: "Selected effect" }));
    expect(view().getByText("Also used by: Default impact (effect). The parts they share with this effect are used by other heroes and stay as the game has them unless you tick them in Advanced.")).toBeVisible();
    expect(editor().getByText(/^1 part keeps the game's look because other heroes use it too; Advanced can include it./)).toBeVisible();
    expect(editor().queryByText(/Advanced settings are active/)).toBeNull();
    fireEvent.click(editor().getByRole("radio", { name: "Advanced" }));
    const tick = screen.getByRole("checkbox", { name: "Recolor Impact debris" });
    expect(tick).not.toBeChecked();
    expect(editor().getAllByText("reaches 1 of 1 part (1 that other heroes use is left out)")).toHaveLength(3);
    expect(view().getByText("Left out: other heroes use this part too, so it keeps the game's look. Ticking it changes it for them too.")).toBeVisible();
    fireEvent.click(tick);
    // Ticking in changes nothing by itself: kept as a page draft.
    expect(saved().customization.effect_edits).toBeUndefined();
    expect(screen.getByRole("checkbox", { name: "Recolor Impact debris" })).toBeChecked();
    expect(view().getByText("Included: other heroes use this part too, and your change shows on theirs as well.")).toBeVisible();
    expect(editor().getAllByText("reaches 1 of 1 part, plus 1 you included that other heroes use")).toHaveLength(3);
    expect(view().getByText("Also used by: Default impact (effect). Your change shows there too.")).toBeVisible();
    typeValue(editor().getByRole("textbox", { name: "Effect size value" }), "150%");
    expect(saved().customization.effect_edits).toEqual([{ target: TRACER, edit: { shared: [DEBRIS], size: 1.5 } }]);
    await waitFor(() => expect(native.effectPreview).toHaveBeenCalledWith(expect.anything(), TRACER, { shared: [DEBRIS], size: 1.5 }, null, null));
    fireEvent.click(editor().getByRole("radio", { name: "Simple" }));
    expect(editor().getByText("Advanced settings are active: 1 part shared with other heroes included.")).toBeVisible();
    expect(editor().queryByText(/keeps the game's look because other heroes/)).toBeNull();
    // Unticking a hero's own part goes to excluded, as before.
    fireEvent.click(editor().getByRole("radio", { name: "Advanced" }));
    fireEvent.click(screen.getByRole("checkbox", { name: "Recolor Tracer" }));
    expect(saved().customization.effect_edits![0].edit).toEqual({ excluded: [TRACER], shared: [DEBRIS], size: 1.5 });
  });

  it("filters by tag, search and Changed", async () => {
    const start = { ...initial, customization: { ...initial.customization, effect_edits: [{ target: TRACER, edit: { glow: 2 } }] } } as Project;
    await openEffects(start);
    const list = () => within(screen.getByRole("region", { name: "Effects" }));
    fireEvent.click(list().getByRole("button", { name: /Weapon/ }));
    expect(list().queryByRole("option", { name: /Explode/ })).toBeNull();
    fireEvent.click(list().getByRole("button", { name: /Weapon/ }));
    fireEvent.click(list().getByRole("radio", { name: /Changed 1/ }));
    expect(list().getAllByRole("option").map((o) => o.textContent)).toEqual([expect.stringContaining("Tracer")]);
    fireEvent.click(list().getByRole("radio", { name: /All 2/ }));
    fireEvent.change(list().getByRole("textbox", { name: "Search effects" }), { target: { value: "nothing like it" } });
    expect(list().getByText("No matching effects")).toBeVisible();
    expect(CORE).toBeTruthy();
  });

  it("says what the list is, explains an ability without files and folds the other files, which search still reaches", async () => {
    native.effectTrees.mockResolvedValue(effectTreesWithOtherFiles);
    await openEffects();
    const list = () => within(screen.getByRole("region", { name: "Effects" }));
    const box = () => within(screen.getByRole("listbox", { name: "Effects" }));
    expect(list().getByText("Effects from Test hero's own folders, grouped by ability.")).toBeVisible();
    // The ability without files of the hero's folders keeps its place, with its note.
    expect(box().getAllByRole("group").map((g) => g.getAttribute("aria-label"))).toEqual(["Big Bomb", "Far Reach", "Weapon · Test Gun", "Other files in Test hero's folders"]);
    expect(box().getByText(/link none of Test hero's own effect files to this ability/)).toBeVisible();
    const fold = box().getByRole("button", { name: "Other files in Test hero's folders, 2 effects" });
    expect(fold).toHaveAttribute("aria-expanded", "false");
    expect(box().getByText(/The game may still play some of them, or none/)).toBeVisible();
    expect(box().queryByRole("option", { name: /Head ambient/ })).toBeNull();
    expect(box().getByRole("button", { name: "Show 2 effects" })).toBeVisible();
    // Search reaches into the folded group; the empty ability group stays out of a search.
    fireEvent.change(list().getByRole("textbox", { name: "Search effects" }), { target: { value: "head" } });
    expect(box().getAllByRole("option").map((o) => o.textContent)).toEqual([expect.stringContaining("Head ambient")]);
    expect(box().queryByRole("group", { name: "Far Reach" })).toBeNull();
    fireEvent.click(list().getByRole("button", { name: "Clear search" }));
    expect(box().queryByRole("option", { name: /Head ambient/ })).toBeNull();
    // Opened, it shows every effect; closed again, they fold away.
    fireEvent.click(box().getByRole("button", { name: /Other files in Test hero's folders/ }));
    expect(box().getByRole("button", { name: /Other files in Test hero's folders/ })).toHaveAttribute("aria-expanded", "true");
    expect(box().getAllByRole("option").map((o) => o.textContent?.match(/Explode|Tracer|Head ambient|Old leap/)?.[0])).toEqual(["Explode", "Tracer", "Head ambient", "Old leap"]);
    fireEvent.click(box().getByRole("option", { name: /Old leap/ }));
    fireEvent.click(box().getByRole("button", { name: /Other files in Test hero's folders/ }));
    // The selected effect stays in view while its group is folded.
    expect(box().getByRole("option", { name: /Old leap/ })).toHaveAttribute("aria-selected", "true");
    expect(box().queryByRole("option", { name: /Head ambient/ })).toBeNull();
  });

  it("always shows an effect the user edited in a folded group, and the Changed filter finds it", async () => {
    native.effectTrees.mockResolvedValue(effectTreesWithOtherFiles);
    const start = { ...initial, customization: { ...initial.customization, effect_edits: [{ target: OLD_LEAP, edit: { hue: 30 } }] } } as Project;
    await openEffects(start);
    const list = () => within(screen.getByRole("region", { name: "Effects" }));
    const box = () => within(screen.getByRole("listbox", { name: "Effects" }));
    expect(box().getByRole("option", { name: /Old leap/ })).toHaveTextContent("Changed");
    expect(box().queryByRole("option", { name: /Head ambient/ })).toBeNull();
    expect(box().getByRole("button", { name: "Show 1 more effect" })).toBeVisible();
    fireEvent.click(list().getByRole("radio", { name: /Changed 1/ }));
    expect(box().getAllByRole("option").map((o) => o.textContent)).toEqual([expect.stringContaining("Old leap")]);
    // The saved row is untouched by folding.
    expect(saved().customization.effect_edits).toEqual([{ target: OLD_LEAP, edit: { hue: 30 } }]);
  });
});
