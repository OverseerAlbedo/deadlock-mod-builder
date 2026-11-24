/** Extras → Colors with the color switches on, as in the app (heroColorRanges.ts COLOR_SWITCHES): a
 * grey pick given a new color and This part only / Everywhere this color; and what the page shows
 * when a switch is turned off (test override only). The 3D view is a stub; the scene is a made-up mesh of two separate
 * quads over a 16 × 16 fake texture (rows 0–7 blue, 8–11 red, 12–15 grey). */
import { useState } from "react";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Project } from "../models";
import type { ComponentCatalog, HeroMaterialEntry } from "../components/AssetCatalog";
import { ExtrasWorkspace } from "./ExtrasPage";
import { clearExtraMediaCache } from "./media";
import { hsv, openColorSwitchesForTest, recolorTexel, withDefaults } from "./heroColorRanges";
import reference from "../../src-tauri/crates/deadlock-project/tests/fixtures/reference.json";

const native = vi.hoisted(() => ({ validateAssetPath: vi.fn(), previewExtra: vi.fn(), previewImage: vi.fn(), preview: vi.fn() }));
vi.mock("../api", async (original) => ({ ...(await original<typeof import("../api")>()), api: native }));
const texture = vi.hoisted(() => {
  const color = (y: number) => (y < 8 ? [61, 111, 214] : y < 12 ? [200, 50, 58] : [200, 200, 200]);
  const data = new Uint8ClampedArray(16 * 16 * 4);
  for (let y = 0; y < 16; y++) for (let x = 0; x < 16; x++) data.set([...color(y), 255], (y * 16 + x) * 4);
  const image = { data, width: 16, height: 16 };
  return { image, pixels: { width: 16, height: 16, scaled: false, full: image, small: image, source: null as unknown } };
});
vi.mock("./colorImages", async (original) => {
  const actual = await original<typeof import("./colorImages")>();
  return {
    ...actual,
    usePixels: (urls: Record<string, string>) => Object.fromEntries(Object.entries(urls).filter(([, url]) => url).map(([id]) => [id, texture.pixels])),
    useColorTextures: () => ({ textures: {}, selection: undefined, version: 0, stats: { lastMs: 0, updates: 0, size: "" } }),
  };
});
const body = "models/heroes_staging/haze/materials/hazev2.vmat_c";
/** One mesh: quad A over u 0.125–0.375, v 0.0625–0.4375 (blue), quad B over u 0.625–0.875, same v. */
const scene = vi.hoisted(() => {
  const positions = new Float32Array([0, 0, 0, 1, 0, 0, 1, 1, 0, 0, 1, 0, 10, 0, 0, 11, 0, 0, 11, 1, 0, 10, 1, 0]);
  const uvs = new Float32Array([0.125, 0.0625, 0.375, 0.0625, 0.375, 0.4375, 0.125, 0.4375, 0.625, 0.0625, 0.875, 0.0625, 0.875, 0.4375, 0.625, 0.4375]);
  const indices = new Uint32Array([0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7]);
  const buffer = new ArrayBuffer((positions.length + uvs.length + indices.length) * 4);
  new Float32Array(buffer, 0, positions.length).set(positions);
  new Float32Array(buffer, positions.length * 4, uvs.length).set(uvs);
  new Uint32Array(buffer, (positions.length + uvs.length) * 4, indices.length).set(indices);
  return { buffer, positions: { offset: 0, count: positions.length }, uvs: { offset: positions.length * 4, count: uvs.length }, indices: { offset: (positions.length + uvs.length) * 4, count: indices.length } };
});
vi.mock("./colorScenes", async (original) => {
  const actual = await original<typeof import("./colorScenes")>();
  return {
    ...actual,
    useScene: () => ({
      status: "ready",
      message: "",
      scene: { buffer: scene.buffer, dataOffset: 0, metadata: { meshes: [{ material: "models/heroes_staging/haze/materials/hazev2.vmat", target: "models/heroes_staging/haze/materials/hazev2.vmat_c", positions: scene.positions, uvs: scene.uvs, indices: scene.indices }], bounds: { min: [0, 0, 0], max: [11, 1, 1] } } },
    }),
  };
});
vi.mock("./ColorScene", () => ({ default: () => <div data-testid="scene" /> }));

const entry: HeroMaterialEntry = {
  path: body.replace(/_c$/, ""), target: body, label: "hazev2", shader: "pbr.vfx", supported: true, reason: "", tint: [1, 1, 1], albedo: [1, 1, 1],
  tint_masked: false, color_texture: "models/heroes_staging/haze/materials/hazev2_color.vtex_c", used_by: [], shared: false, original_sha256: "a".repeat(64),
};
const catalog: ComponentCatalog = { hero_path: "", icons: [], audio: [], effects: [], animations: [], variants: [], animation_unavailable: null, warnings: [], materials: [entry] };
const start = { ...reference.cases[1].expected, hero_name: "Haze", customization: { enabled: true, model_enabled: false } } as unknown as Project;
const heroWith = (row: Record<string, unknown>) => ({ ...start, customization: { enabled: true, model_enabled: false, hero_materials: { enabled: true, materials: [{ enabled: true, target: body, original_sha256: "a".repeat(64), ...row }] } } }) as Project;

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
  native.previewExtra.mockResolvedValue(packet());
  vi.stubGlobal("URL", Object.assign(URL, { createObjectURL: vi.fn(() => "blob:test"), revokeObjectURL: vi.fn() }));
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(null);
  // Every element is a 40 × 40 box: the 16 × 16 texture is drawn 2.5 × larger.
  vi.spyOn(Element.prototype, "getBoundingClientRect").mockReturnValue({ left: 0, top: 0, width: 40, height: 40, right: 40, bottom: 40, x: 0, y: 0, toJSON: () => ({}) } as DOMRect);
});
afterEach(() => { openColorSwitchesForTest(null); cleanup(); vi.restoreAllMocks(); });

function Harness({ project }: { project: Project }) {
  const [p, set] = useState(() => structuredClone(project));
  return (
    <>
      <ExtrasWorkspace project={p} update={set} disabled={false} catalog={{ ...catalog, hero_path: p.original_model }} fail={vi.fn()} />
      <output data-testid="saved">{JSON.stringify(p)}</output>
    </>
  );
}
const ranges = () => (JSON.parse(screen.getByTestId("saved").textContent!) as Project).customization.hero_materials?.materials?.[0]?.ranges;
async function openTexture(project: Project) {
  render(<Harness project={project} />);
  fireEvent.click(screen.getByRole("tab", { name: /Colors/ }));
  fireEvent.click(within(screen.getByRole("listbox", { name: "Materials" })).getByRole("option", { name: /hazev2/ }));
  await screen.findByTestId("scene");
  fireEvent.click(screen.getByRole("radio", { name: "Texture" }));
}
function chooseColor(hex: string) {
  const chooser = screen.getByRole("group", { name: "Choose the new color" });
  fireEvent.change(within(chooser).getByLabelText("New color hex color"), { target: { value: hex } });
  fireEvent.blur(within(chooser).getByLabelText("New color hex color"));
  fireEvent.click(within(chooser).getByRole("button", { name: "Done" }));
}

describe("Colors: give a grey a new color (switch set_colour)", () => {
  it("accepts a grey pick, says it is given the color, and saves a range that turns the grey into it", async () => {
    await openTexture(heroWith({}));
    fireEvent.click(screen.getByRole("button", { name: "Recolor another color" }));
    fireEvent.click(document.querySelectorAll(".color-texture-stage")[0], { clientX: 10, clientY: 35 });
    expect(screen.queryByText(/That spot is grey/)).not.toBeInTheDocument();
    expect(screen.getByText(/has no hue of its own/)).toBeInTheDocument();
    expect(screen.queryByRole("radiogroup", { name: /what it changes/ })).not.toBeInTheDocument();
    chooseColor("#7A4BE0");
    const [range] = ranges()!;
    expect(range).toMatchObject({ hue: 0, hue_width: 180, saturation_range: [0, 0.1], brightness_range: [0.53, 1], hue_shift: 0, saturation: 1 });
    expect(range.set_hue).toBeCloseTo(259, 0);
    expect(range).not.toHaveProperty("part");
    const out = recolorTexel([withDefaults(range)], [200, 200, 200, 255]);
    [122, 75, 224].forEach((v, i) => expect(Math.abs(out[i] - v)).toBeLessThanOrEqual(3));
    // The blue and red rows stay as they were.
    expect(recolorTexel([withDefaults(range)], [61, 111, 214, 255])).toEqual([61, 111, 214, 255]);
  });
});

describe("Colors: This part only (switch part_only)", () => {
  it("offers This part only and Everywhere this color with their shares and saves the spot only for the part", async () => {
    await openTexture(heroWith({}));
    fireEvent.click(screen.getByRole("button", { name: "Recolor another color" }));
    fireEvent.click(document.querySelectorAll(".color-texture-stage")[0], { clientX: 10, clientY: 10 });
    const choice = screen.getByRole("radiogroup", { name: "The new color: what it changes" });
    // Quad A is pixels 2–5 across and 1–6 down; with one pixel of reach 6 × 8 of the 256: 19%.
    await waitFor(() => expect(within(choice).getByRole("radio", { name: "This part only · 19%" })).toHaveAttribute("aria-checked", "true"));
    expect(within(choice).getByRole("radio", { name: "Everywhere this color · 50%" })).toBeInTheDocument();
    chooseColor("#E2B23A");
    const [range] = ranges()!;
    expect(range.part).toEqual({ u: 0.25, v: 0.25 });
    // The card keeps the choice; Everywhere drops the spot and This part only brings it back.
    const card = screen.getByRole("group", { name: "Range 1" });
    const scope = within(card).getByRole("radiogroup", { name: "Range 1: what it changes" });
    await waitFor(() => expect(within(scope).getByRole("radio", { name: "This part only · 19%" })).toBeInTheDocument());
    fireEvent.click(within(scope).getByRole("radio", { name: /Everywhere this color/ }));
    expect(ranges()![0]).not.toHaveProperty("part");
    fireEvent.click(within(scope).getByRole("radio", { name: /This part only/ }));
    expect(ranges()![0].part).toEqual({ u: 0.25, v: 0.25 });
  });
  it("picks everywhere when the spot is on no part, and a choice of Everywhere saves no spot", async () => {
    await openTexture(heroWith({}));
    fireEvent.click(screen.getByRole("button", { name: "Recolor another color" }));
    // u 0.5 lies between the quads.
    fireEvent.click(document.querySelectorAll(".color-texture-stage")[0], { clientX: 20, clientY: 10 });
    expect(screen.getByText(/That spot is on no part of the model/)).toBeInTheDocument();
    chooseColor("#E2B23A");
    expect(ranges()![0]).not.toHaveProperty("part");
    fireEvent.click(screen.getByRole("button", { name: "Recolor another color" }));
    fireEvent.click(document.querySelectorAll(".color-texture-stage")[0], { clientX: 10, clientY: 10 });
    fireEvent.click(within(screen.getByRole("radiogroup", { name: "The new color: what it changes" })).getByRole("radio", { name: /Everywhere this color/ }));
    chooseColor("#00C040");
    expect(ranges()![1]).not.toHaveProperty("part");
  });
});

describe("Colors: a switch turned off (the test override; both are on in the app)", () => {
  it("refuses a grey pick, offers no part choice, and says a saved range of either kind stops the build", async () => {
    openColorSwitchesForTest({ set_colour: false, part_only: false });
    await openTexture(heroWith({ ranges: [{ enabled: true, hue: 220, hue_width: 20, hue_shift: 90, part: { u: 0.25, v: 0.25 } }, { enabled: true, saturation_range: [0, 0.1], set_hue: 270, set_saturation: 0.6 }] }));
    expect(within(screen.getByRole("group", { name: "Range 1" })).getByText(/keeps This part only switched off/)).toBeInTheDocument();
    expect(within(screen.getByRole("group", { name: "Range 2" })).getByText(/keeps new colors for grey, white and black switched off/)).toBeInTheDocument();
    expect(screen.queryByRole("radiogroup", { name: /what it changes/ })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Recolor another color" }));
    fireEvent.click(document.querySelectorAll(".color-texture-stage")[0], { clientX: 10, clientY: 10 });
    expect(screen.queryByRole("radiogroup", { name: /what it changes/ })).not.toBeInTheDocument();
    fireEvent.keyDown(window, { key: "Escape" });
    fireEvent.click(screen.getByRole("button", { name: "Recolor another color" }));
    fireEvent.click(document.querySelectorAll(".color-texture-stage")[0], { clientX: 10, clientY: 35 });
    expect(screen.getByText("That spot is grey or white: it has no hue to select or shift.")).toBeInTheDocument();
    expect(hsv(200 / 255, 200 / 255, 200 / 255)[1]).toBe(0);
  });
});
