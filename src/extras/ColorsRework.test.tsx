/** Extras → Colors, the reworked page: steps and their summaries, pointing (both ways), recolor
 * by picking end to end, a grey pick, what a range selects, the warnings, Undo and Redo. The 3D
 * view is a stub that records its props (no WebGL here), and the images are a small fake
 * texture (jsdom has no 2D canvas). */
import { useRef, useState } from "react";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Project } from "../models";
import type { ComponentCatalog, HeroMaterialEntry } from "../components/AssetCatalog";
import { ExtrasWorkspace } from "./ExtrasPage";
import { clearExtraMediaCache } from "./media";
import { HistoryContext, useProjectHistory, useUndoKeys } from "../history/useProjectHistory";
import { recolorTexel, withDefaults } from "./heroColorRanges";
import reference from "../../src-tauri/crates/deadlock-project/tests/fixtures/reference.json";

const native = vi.hoisted(() => ({ validateAssetPath: vi.fn(), previewExtra: vi.fn(), previewImage: vi.fn(), preview: vi.fn() }));
vi.mock("../api", async (original) => ({ ...(await original<typeof import("../api")>()), api: native }));
/** The fake texture: rows of blue, blue, red and grey (4 × 4). */
const texture = vi.hoisted(() => {
  const rows = [[61, 111, 214], [61, 111, 214], [200, 50, 58], [128, 128, 128]];
  const data = new Uint8ClampedArray(rows.flatMap((p) => [...p, 255, ...p, 255, ...p, 255, ...p, 255]));
  const image = { data, width: 4, height: 4 };
  return { image, pixels: { width: 4, height: 4, scaled: false, full: image, small: image, source: null as unknown } };
});
vi.mock("./colorImages", async (original) => {
  const actual = await original<typeof import("./colorImages")>();
  return {
    ...actual,
    usePixels: (urls: Record<string, string>) => Object.fromEntries(Object.entries(urls).filter(([, url]) => url).map(([id]) => [id, texture.pixels])),
    useColorTextures: () => ({ textures: {}, selection: undefined, version: 0, stats: { lastMs: 0, updates: 0, size: "" } }),
  };
});
/** The scene: ready, one mesh per material of the test (set per test). */
const view = vi.hoisted(() => ({ meshes: [] as { material: string; target?: string; weapon?: boolean; default_hidden?: boolean; hidden_reason?: string }[], props: null as null | Record<string, unknown> }));
vi.mock("./colorScenes", async (original) => {
  const actual = await original<typeof import("./colorScenes")>();
  const uvs = new Float32Array([0.1, 0.1, 0.1, 0.1, 0.1, 0.1]);
  return {
    ...actual,
    useScene: () => ({
      status: "ready",
      message: "",
      scene: { buffer: uvs.buffer, dataOffset: 0, metadata: { meshes: view.meshes.map((m) => ({ ...m, uvs: { offset: 0, count: 6 }, positions: { offset: 0, count: 6 } })), bounds: { min: [0, 0, 0], max: [1, 1, 2] } } },
    }),
  };
});
vi.mock("./ColorScene", () => ({
  default: (props: Record<string, unknown>) => {
    view.props = props;
    const highlight = props.highlight as { kind: string; id: string } | null;
    return <div data-testid="scene" data-highlight={highlight ? `${highlight.kind}:${highlight.id}` : ""} />;
  },
}));

const body = "models/heroes_staging/haze/materials/hazev2.vmat_c";
const gun = "models/heroes_staging/haze_v2/materials/haze_v2_gun.vmat_c";
const entry = (target: string): HeroMaterialEntry => ({
  path: target.replace(/_c$/, ""), target, label: target.split("/").pop()!.replace(".vmat_c", ""), shader: "pbr.vfx",
  supported: true, reason: "", tint: [1, 1, 1], albedo: [1, 1, 1], tint_masked: false,
  color_texture: target.replace(".vmat_c", "_color.vtex_c"), used_by: [], shared: false, original_sha256: "a".repeat(64),
});
const catalog: ComponentCatalog = { hero_path: "", icons: [], audio: [], effects: [], animations: [], variants: [], animation_unavailable: null, warnings: [], materials: [entry(body), entry(gun)] };
const start = { ...reference.cases[1].expected, hero_name: "Haze", customization: { enabled: true, model_enabled: false } } as unknown as Project;
const cloth = "materials/body/cloth.vmat";
const replacement = {
  ...start,
  dmx: "C:/work/prepared.dmx",
  materials: { [cloth]: { mode: "generate", preset: "cloth", textures: { color: "C:/textures/cloth.png" } } },
  customization: { enabled: true, appearance: { enabled: true, materials: [{ enabled: true, target: cloth, roughness: 0.4 }] } },
} as unknown as Project;
const blueRange = { enabled: true, hue: 220, hue_width: 20, saturation_range: [0.36, 1] as [number, number], brightness_range: [0.39, 1] as [number, number], softness: 0, hue_shift: -176, saturation: 1.1, brightness: 1.05 };
const greenRange = { ...blueRange, hue: 120 };

function packet() {
  const head = new TextEncoder().encode(JSON.stringify({ mime_type: "image/png", details: {} }));
  const buffer = new ArrayBuffer(4 + head.length + 8);
  new DataView(buffer).setUint32(0, head.length, true);
  new Uint8Array(buffer, 4, head.length).set(head);
  return buffer;
}
beforeEach(() => {
  vi.clearAllMocks();
  clearExtraMediaCache();
  localStorage.clear();
  view.meshes = [{ material: body.replace(/_c$/, ""), target: body }, { material: gun.replace(/_c$/, ""), target: gun }];
  view.props = null;
  native.previewExtra.mockResolvedValue(packet());
  native.previewImage.mockResolvedValue(new ArrayBuffer(8));
  vi.stubGlobal("URL", Object.assign(URL, { createObjectURL: vi.fn(() => "blob:test"), revokeObjectURL: vi.fn() }));
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(null);
  // Every element is a 40 × 40 box: the 4 × 4 texture is drawn 10 × larger.
  vi.spyOn(Element.prototype, "getBoundingClientRect").mockReturnValue({ left: 0, top: 0, width: 40, height: 40, right: 40, bottom: 40, x: 0, y: 0, toJSON: () => ({}) } as DOMRect);
});
afterEach(() => vi.restoreAllMocks());

/** The page with the app's project history around it, as `App.tsx` keeps it (the same hook,
 * the same one key handler). */
function Harness({ project }: { project: Project }) {
  const [p, set] = useState(() => structuredClone(project));
  const current = useRef(p);
  const write = (next: Project) => { current.current = next; set(next); return true; };
  const { controls, record } = useProjectHistory({ read: () => current.current, write });
  useUndoKeys(controls);
  const update = (next: Project) => { record(current.current, next); write(next); };
  return (
    <HistoryContext.Provider value={controls}>
      <ExtrasWorkspace project={p} update={update} disabled={false} catalog={{ ...catalog, hero_path: p.original_model }} fail={vi.fn()} />
      <output data-testid="saved">{JSON.stringify(p)}</output>
    </HistoryContext.Provider>
  );
}
const saved = () => JSON.parse(screen.getByTestId("saved").textContent!) as Project;
const heroRow = () => saved().customization.hero_materials?.materials?.[0];
const list = () => screen.getByRole("listbox", { name: "Materials" });
const choose = (name: RegExp) => fireEvent.click(within(list()).getByRole("option", { name }));
const step = (name: RegExp) => screen.getByRole("button", { name });
const heroWith = (row: Record<string, unknown>) => ({ ...start, customization: { enabled: true, model_enabled: false, hero_materials: { enabled: true, materials: [{ enabled: true, target: body, original_sha256: "a".repeat(64), allow_shared: false, ...row }] } } }) as Project;
async function open(project: Project, material: RegExp) {
  render(<Harness project={project} />);
  fireEvent.click(screen.getByRole("tab", { name: /Colors/ }));
  choose(material);
  await screen.findByTestId("scene");
}

describe("Colors: the editor's three steps", () => {
  it("folds each step to a line that says what it does, one open at a time unless Shift is held", async () => {
    await open(heroWith({ color: [237 / 255, 59 / 255, 59 / 255], brightness: 0.5, ranges: [blueRange] }), /hazev2/);
    expect(step(/^1\. Recolor parts: 1 color changed$/)).toHaveAttribute("aria-expanded", "true");
    expect(step(/^2\. Whole material: Brightness 50%$/)).toHaveAttribute("aria-expanded", "false");
    expect(step(/^3\. Tint: #ED3B3B · darkens other colors$/)).toHaveAttribute("aria-expanded", "false");
    expect(screen.getByText("Applied in this order: recolored parts first, then the whole material, then the tint on top.")).toBeInTheDocument();
    fireEvent.click(step(/^2\. Whole material/));
    expect(step(/^1\. Recolor parts/)).toHaveAttribute("aria-expanded", "false");
    fireEvent.click(step(/^3\. Tint/), { shiftKey: true });
    expect(step(/^2\. Whole material/)).toHaveAttribute("aria-expanded", "true");
    expect(step(/^3\. Tint/)).toHaveAttribute("aria-expanded", "true");
    // The tint warning, with one click back to the original tint.
    const warning = screen.getByText(/This tint darkens every other color of the material/).closest("[role=note]")!;
    fireEvent.click(within(warning as HTMLElement).getByRole("button", { name: "Back to the original tint" }));
    expect(heroRow()).not.toHaveProperty("color");
    expect(step(/^3\. Tint: original$/)).toBeInTheDocument();
    expect(screen.queryByText(/This tint darkens every other color/)).not.toBeInTheDocument();
  });
  it("shows how much a range selects and says when it selects nothing, with the reason", async () => {
    await open(heroWith({ ranges: [blueRange, greenRange] }), /hazev2/);
    const first = screen.getByRole("group", { name: "Range 1" });
    await waitFor(() => expect(first).toHaveTextContent("selects 50% of this material"));
    expect(within(first).getByText("Blue → gold")).toBeInTheDocument();
    const second = screen.getByRole("group", { name: "Range 2" });
    expect(second).toHaveTextContent("selects nothing in this material");
    expect(within(second).getByRole("note")).toHaveTextContent("the image has no color in that hue: grey and white areas have no hue to select.");
    // Fine-tune holds today's sliders, unchanged.
    fireEvent.click(within(first).getByRole("button", { name: /Fine-tune this range/ }));
    for (const name of ["Range 1 hue", "Range 1 hue width", "Range 1 saturation from", "Range 1 brightness to", "Range 1 softness", "Range 1 hue shift", "Range 1 saturation", "Range 1 brightness"]) expect(within(first).getByLabelText(name)).toBeInTheDocument();
  });
});

describe("Colors: pointing", () => {
  it("lights a material pointed at in the list and marks the row of the part pointed at on the character", async () => {
    await open(heroWith({ brightness: 0.5 }), /hazev2/);
    fireEvent.pointerEnter(within(list()).getByRole("option", { name: /haze_v2_gun/ }));
    expect(screen.getByTestId("scene")).toHaveAttribute("data-highlight", `material:${gun}`);
    expect(screen.getByText("haze_v2_gun · what you are pointing at")).toBeInTheDocument();
    fireEvent.pointerLeave(list().querySelector("[role=listbox]") ?? list());
    expect(screen.getByTestId("scene")).toHaveAttribute("data-highlight", "");
    // Keyboard: moving to a row lights it too.
    fireEvent.focus(list());
    fireEvent.keyDown(list(), { key: "ArrowDown" });
    expect(screen.getByTestId("scene")).toHaveAttribute("data-highlight", `material:${gun}`);
    // The other way: the part under the pointer marks its row.
    act(() => (view.props!.hover as (id: string | null) => void)(body));
    expect(within(list()).getByRole("option", { name: /hazev2/ })).toHaveClass("pointed");
    act(() => (view.props!.hover as (id: string | null) => void)(null));
    expect(within(list()).getByRole("option", { name: /hazev2/ })).not.toHaveClass("pointed");
  });
  it("lights what a range selects while its card is pointed at", async () => {
    await open(heroWith({ ranges: [blueRange] }), /hazev2/);
    fireEvent.pointerEnter(screen.getByRole("group", { name: "Range 1" }));
    expect(screen.getByTestId("scene")).toHaveAttribute("data-highlight", `selection:${body}`);
    expect(screen.getByText("Showing what Range 1 covers")).toBeInTheDocument();
    await waitFor(() => expect(screen.getByText("what Range 1 covers · 50% of hazev2")).toBeInTheDocument());
  });
});

describe("Colors: recolor by picking", () => {
  it("picks a color on the character, lights what it covers, and saves a range that turns it into the new color", async () => {
    await open(heroWith({ brightness: 0.9 }), /hazev2/);
    fireEvent.click(screen.getByRole("button", { name: "Recolor another color" }));
    expect(screen.getByText("Click a color on hazev2")).toBeInTheDocument();
    act(() => (view.props!.pick as (hit: unknown) => void)({ id: body, mesh: 0, u: 0.1, v: 0.1, x: 50, y: 60, flip: false }));
    // Before anything is saved, the picked color lights up with how much it covers.
    expect(heroRow()?.ranges).toBeUndefined();
    expect(screen.getByTestId("scene")).toHaveAttribute("data-highlight", `selection:${body}`);
    await waitFor(() => expect(document.querySelector(".color-pick-pop")).toHaveTextContent("This blue selects 50% of hazev2"));
    const chooser = screen.getByRole("group", { name: "Choose the new color" });
    expect(within(chooser).getByRole("button", { name: "Done" })).toBeDisabled();
    fireEvent.change(within(chooser).getByLabelText("New color hex color"), { target: { value: "#E2B23A" } });
    fireEvent.blur(within(chooser).getByLabelText("New color hex color"));
    fireEvent.click(within(chooser).getByRole("button", { name: "Done" }));
    const [range] = heroRow()!.ranges!;
    expect(range).toMatchObject({ enabled: true, hue_width: 20, softness: 0.25 });
    // Applying the saved range to the picked pixel gives the chosen color, within rounding.
    const out = recolorTexel([withDefaults(range)], [61, 111, 214, 255]);
    [226, 178, 58].forEach((v, i) => expect(Math.abs(out[i] - v)).toBeLessThanOrEqual(2));
    expect(screen.queryByRole("group", { name: "Choose the new color" })).not.toBeInTheDocument();
    expect(step(/^1\. Recolor parts: 1 color changed$/)).toBeInTheDocument();
    // Undo takes the whole pick back, also with the keys when Done took the focus away; Redo brings it again.
    fireEvent.keyDown(document.body, { key: "z", ctrlKey: true });
    expect(heroRow()?.ranges).toBeUndefined();
    fireEvent.keyDown(document.body, { key: "y", ctrlKey: true });
    expect(heroRow()?.ranges).toHaveLength(1);
    fireEvent.click(screen.getByRole("button", { name: "Undo" }));
    expect(heroRow()?.ranges).toBeUndefined();
    fireEvent.click(screen.getByRole("button", { name: "Redo" }));
    expect(heroRow()?.ranges).toHaveLength(1);
  });
  it("picks in the Texture view of a replacement model, and Change the new color keeps what it selects", async () => {
    view.meshes = [{ material: cloth }];
    await open(replacement, /cloth/);
    fireEvent.click(screen.getByRole("radio", { name: "Texture" }));
    fireEvent.click(screen.getByRole("button", { name: "Recolor another color" }));
    fireEvent.click(document.querySelectorAll(".color-texture-stage")[0], { clientX: 5, clientY: 5 });
    const chooser = screen.getByRole("group", { name: "Choose the new color" });
    fireEvent.change(within(chooser).getByLabelText("New color hex color"), { target: { value: "#FF0000" } });
    fireEvent.blur(within(chooser).getByLabelText("New color hex color"));
    fireEvent.click(within(chooser).getByRole("button", { name: "Done" }));
    const row = () => saved().customization.appearance!.materials![0];
    expect(row()).toMatchObject({ enabled: true, target: cloth, roughness: 0.4 });
    expect(saved().customization.appearance!.enabled).toBe(true);
    const first = row().ranges![0];
    let out = recolorTexel([withDefaults(first)], [61, 111, 214, 255]);
    [255, 0, 0].forEach((v, i) => expect(Math.abs(out[i] - v)).toBeLessThanOrEqual(2));
    // Change the new color: the selection stays, the change follows the new color.
    fireEvent.click(screen.getByRole("button", { name: "Change the new color…" }));
    const again = screen.getByRole("group", { name: "Choose the new color" });
    fireEvent.change(within(again).getByLabelText("New color hex color"), { target: { value: "#00C040" } });
    fireEvent.blur(within(again).getByLabelText("New color hex color"));
    fireEvent.click(within(again).getByRole("button", { name: "Done" }));
    const second = row().ranges![0];
    expect(second).toMatchObject({ hue: first.hue, hue_width: first.hue_width, saturation_range: first.saturation_range, brightness_range: first.brightness_range, softness: first.softness });
    out = recolorTexel([withDefaults(second)], [61, 111, 214, 255]);
    [0, 192, 64].forEach((v, i) => expect(Math.abs(out[i] - v)).toBeLessThanOrEqual(3));
    // Esc cancels from the new color's hex box too (unless it holds an edit of its own).
    fireEvent.click(screen.getByRole("button", { name: "Change the new color…" }));
    const box = within(screen.getByRole("group", { name: "Choose the new color" })).getByLabelText("New color hex color");
    fireEvent.focus(box);
    fireEvent.keyDown(box, { key: "Escape" });
    expect(screen.queryByRole("group", { name: "Choose the new color" })).not.toBeInTheDocument();
  });
  it("gives a grey spot the new color, saves nothing before Done, and Esc cancels", async () => {
    view.meshes = [{ material: cloth }];
    await open(replacement, /cloth/);
    fireEvent.click(screen.getByRole("radio", { name: "Texture" }));
    fireEvent.click(screen.getByRole("button", { name: "Recolor another color" }));
    fireEvent.click(document.querySelectorAll(".color-texture-stage")[0], { clientX: 5, clientY: 35 });
    expect(screen.queryByText(/That spot is grey/)).not.toBeInTheDocument();
    const chooser = screen.getByRole("group", { name: "Choose the new color" });
    expect(within(chooser).getByText(/Grey has no hue of its own/)).toBeInTheDocument();
    expect(saved().customization.appearance!.materials![0]).not.toHaveProperty("ranges");
    fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.queryByRole("group", { name: "Choose the new color" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Recolor another color" })).toBeEnabled();
    // Done saves a range that gives the grey the chosen color.
    fireEvent.click(screen.getByRole("button", { name: "Recolor another color" }));
    fireEvent.click(document.querySelectorAll(".color-texture-stage")[0], { clientX: 5, clientY: 35 });
    const again = screen.getByRole("group", { name: "Choose the new color" });
    fireEvent.change(within(again).getByLabelText("New color hex color"), { target: { value: "#7A4BE0" } });
    fireEvent.blur(within(again).getByLabelText("New color hex color"));
    fireEvent.click(within(again).getByRole("button", { name: "Done" }));
    const [range] = saved().customization.appearance!.materials![0].ranges!;
    expect(range.set_hue).toBeCloseTo(259, 0);
    const out = recolorTexel([withDefaults(range)], [128, 128, 128, 255]);
    expect(out[2]).toBeGreaterThan(out[1] + 30);
  });
});

describe("Colors: Undo and Redo", () => {
  it("takes a drag back as one step, with the buttons and the keys, but leaves text fields alone", async () => {
    await open(start, /hazev2/);
    fireEvent.click(step(/^2\. Whole material/));
    const slider = screen.getByLabelText("Brightness");
    expect(screen.getByRole("button", { name: "Undo" })).toBeDisabled();
    fireEvent.pointerDown(slider);
    for (const value of ["0.9", "0.8", "0.7"]) fireEvent.change(slider, { target: { value } });
    fireEvent.pointerUp(window);
    expect(heroRow()).toMatchObject({ brightness: 0.7 });
    fireEvent.change(slider, { target: { value: "0.6" } });
    const page = screen.getByRole("tabpanel");
    fireEvent.keyDown(page, { key: "z", ctrlKey: true });
    expect(heroRow()).toMatchObject({ brightness: 0.7 });
    fireEvent.keyDown(page, { key: "z", ctrlKey: true });
    // One step took the whole drag back: the project is as it was before it (no hero colors).
    expect(heroRow()).toBeUndefined();
    expect(saved().customization).toEqual(start.customization);
    expect(screen.getByRole("button", { name: "Undo" })).toBeDisabled();
    fireEvent.keyDown(page, { key: "y", ctrlKey: true });
    expect(heroRow()).toMatchObject({ brightness: 0.7 });
    fireEvent.keyDown(page, { key: "Z", ctrlKey: true, shiftKey: true });
    expect(heroRow()).toMatchObject({ brightness: 0.6 });
    // Ctrl+Z in a text field belongs to the field.
    fireEvent.click(step(/^3\. Tint/));
    fireEvent.keyDown(screen.getByLabelText("Tint hex color"), { key: "z", ctrlKey: true });
    expect(heroRow()).toMatchObject({ brightness: 0.6 });
  });
  it("undoes Reset all colors, and Pause all extras is a step of the same history", async () => {
    await open(heroWith({ color: [0, 0, 0] }), /hazev2/);
    fireEvent.click(screen.getByRole("button", { name: "Reset all colors…" }));
    fireEvent.click(within(screen.getByRole("dialog", { name: "Reset all colors" })).getByRole("button", { name: "Reset all" }));
    expect(saved().customization.hero_materials?.materials).toEqual([]);
    fireEvent.click(screen.getByRole("button", { name: "Undo" }));
    expect(heroRow()).toMatchObject({ target: body, color: [0, 0, 0], enabled: true });
    // Pause all extras is an edit of the project like any other: the one history takes it back.
    fireEvent.click(screen.getByRole("button", { name: "Pause all extras" }));
    expect(heroRow()?.enabled).toBe(false);
    expect(screen.getByRole("button", { name: "Redo" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "Undo" }));
    expect(heroRow()).toMatchObject({ color: [0, 0, 0], enabled: true });
    // The reset was taken back before the pause, and the pause dropped its Redo: nothing is left.
    expect(screen.getByRole("button", { name: "Undo" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "Redo" }));
    expect(heroRow()?.enabled).toBe(false);
  });
});

describe("Colors: the Weapon filter", () => {
  const hiddenMeshes = () => [...(view.props!.hidden as Set<number>)];
  it("lists the materials the marked weapon parts use, says which the body uses too, and draws hidden weapon parts", async () => {
    // A held part of the body material on the weapon, and the gun itself, which the view hides at first.
    view.meshes = [
      { material: body.replace(/_c$/, ""), target: body },
      { material: body.replace(/_c$/, ""), target: body, weapon: true },
      { material: gun.replace(/_c$/, ""), target: gun, weapon: true, default_hidden: true, hidden_reason: "gun away from the hand (placed by animation)" },
      // The gun's glass, an effect layer: drawn solid it would cover the gun, so it stays hidden.
      { material: gun.replace(/_c$/, ""), target: gun, weapon: true, default_hidden: true, hidden_reason: "glow or effect layer drawn over the model (F_ADDITIVE_BLEND)" },
    ];
    await open(start, /hazev2/);
    expect(hiddenMeshes()).toEqual([2, 3]);
    fireEvent.click(screen.getByRole("radio", { name: "Weapon 2" }));
    expect(within(list()).getAllByRole("option").map((o) => o.textContent)).toEqual([
      expect.stringContaining("hazev2"),
      expect.stringContaining("haze_v2_gun"),
    ]);
    expect(within(list()).getByRole("option", { name: /hazev2/ })).toHaveTextContent("Used by the weapon and the body");
    expect(within(list()).getByRole("option", { name: /haze_v2_gun/ })).not.toHaveTextContent("Used by the weapon and the body");
    // The hidden gun is drawn (and so clickable) while the filter is on, and the view says so.
    expect(hiddenMeshes()).toEqual([3]);
    expect(view.props!.frame).toEqual({ min: [0, 0, 0], max: [1, 1, 2] });
    expect(screen.getByText("Weapon: 1 part the game places by animation is shown where the model keeps it.")).toBeInTheDocument();
    act(() => (view.props!.select as (id: string) => void)(gun));
    expect(within(list()).getByRole("option", { selected: true })).toHaveTextContent("haze_v2_gun");
    fireEvent.click(screen.getByRole("radio", { name: /^All/ }));
    expect(hiddenMeshes()).toEqual([2, 3]);
    expect(view.props!.frame).toBeUndefined();
    expect(within(list()).getByRole("option", { name: /hazev2/ })).toHaveTextContent("Used by the weapon and the body");
  });
  it("is not offered when no part is marked as the weapon", async () => {
    await open(start, /hazev2/);
    expect(screen.getByRole("radio", { name: /^All/ })).toBeInTheDocument();
    expect(screen.queryByRole("radio", { name: /^Weapon/ })).not.toBeInTheDocument();
  });
  it("shows a replacement model's gun materials only when its scene marks the fitted gun", async () => {
    view.meshes = [{ material: cloth, weapon: true }];
    await open(replacement, /cloth/);
    expect(screen.getByRole("radio", { name: "Weapon 1" })).toBeInTheDocument();
    cleanup();
    view.meshes = [{ material: cloth }];
    await open(replacement, /cloth/);
    expect(screen.queryByRole("radio", { name: /^Weapon/ })).not.toBeInTheDocument();
  });
  it("loads a replacement model through the scene that marks its gun, or the plain preview", async () => {
    const { loadReplacementScene } = await vi.importActual<typeof import("./colorScenes")>("./colorScenes");
    const marked = new ArrayBuffer(4);
    native.preview.mockResolvedValue(new ArrayBuffer(8));
    expect(await loadReplacementScene(replacement)()).toHaveProperty("byteLength", 8);
    expect(native.preview).toHaveBeenCalledWith("C:/work/prepared.dmx");
    Object.assign(native, { replacementColorScene: vi.fn().mockResolvedValue(marked) });
    try {
      expect(await loadReplacementScene(replacement)()).toBe(marked);
      expect((native as unknown as { replacementColorScene: ReturnType<typeof vi.fn> }).replacementColorScene).toHaveBeenCalledWith(replacement);
    } finally {
      delete (native as { replacementColorScene?: unknown }).replacementColorScene;
    }
  });
});

describe("Colors: reading a pick as it is seen", () => {
  it("decides a grey pick on the average of the area around the click, and says what Picked shows", async () => {
    await open(heroWith({ brightness: 0.9 }), /hazev2/);
    fireEvent.click(screen.getByRole("button", { name: "Recolor another color" }));
    // The click's own pixel is blue, but nearly all of the area around it is the grey row: a grey
    // pick, which is given the new color.
    const grey = Array.from({ length: 12 }, () => ({ u: 0.1, v: 0.9 }));
    act(() => (view.props!.pick as (hit: unknown) => void)({ id: body, mesh: 0, u: 0.1, v: 0.1, x: 50, y: 60, w: 400, samples: [{ u: 0.1, v: 0.1 }, ...grey] }));
    expect(within(screen.getByRole("group", { name: "Choose the new color" })).getByText(/has no hue of its own/)).toBeInTheDocument();
    // Mostly blue around the click: a pick, with the line saying the swatch is the image's own color.
    act(() => (view.props!.pick as (hit: unknown) => void)({ id: body, mesh: 0, u: 0.1, v: 0.1, x: 50, y: 60, w: 400, samples: [{ u: 0.1, v: 0.1 }, { u: 0.1, v: 0.3 }, { u: 0.1, v: 0.9 }] }));
    const chooser = screen.getByRole("group", { name: "Choose the new color" });
    expect(within(chooser).getByText("Picked is the image's own color around the spot; the character is drawn darker by its lighting.")).toBeInTheDocument();
  });
  it("says a material whose image is one flat color cannot be picked by color, and points to the Tint", async () => {
    const flat = { data: new Uint8ClampedArray([240, 240, 240, 255]), width: 1, height: 1 };
    const kept = { full: texture.pixels.full, small: texture.pixels.small, width: texture.pixels.width, height: texture.pixels.height };
    Object.assign(texture.pixels, { full: flat, small: flat, width: 1, height: 1 });
    try {
      await open(heroWith({ brightness: 0.9 }), /hazev2/);
      fireEvent.click(screen.getByRole("button", { name: "Recolor another color" }));
      expect(screen.getByText("This material's image is one flat color, so there is no color in it to pick out.")).toBeInTheDocument();
      fireEvent.click(screen.getByRole("button", { name: "Open Tint" }));
      expect(step(/^3\. Tint/)).toHaveAttribute("aria-expanded", "true");
      expect(heroRow()).not.toHaveProperty("ranges");
    } finally {
      Object.assign(texture.pixels, kept);
    }
  });
});
