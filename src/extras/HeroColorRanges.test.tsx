import { useState } from "react";
import { fireEvent, render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Project } from "../models";
import type { ComponentCatalog, HeroMaterialEntry } from "../components/AssetCatalog";
import { ExtrasWorkspace } from "./ExtrasPage";
import { activeRanges, coverPixel } from "./HeroColors";
import { RANGE_DEFAULTS } from "./heroColorRanges";
import { clearExtraMediaCache } from "./media";
import reference from "../../src-tauri/crates/deadlock-project/tests/fixtures/reference.json";
const native = vi.hoisted(() => ({ validateAssetPath: vi.fn(), previewExtra: vi.fn() }));
vi.mock("../api", async (original) => ({ ...(await original<typeof import("../api")>()), api: native }));

const body = "models/heroes_staging/haze/materials/hazev2.vmat_c";
const entry: HeroMaterialEntry = {
  path: body.replace(/_c$/, ""), target: body, label: "hazev2", shader: "pbr.vfx", supported: true, reason: "",
  tint: [1, 1, 1], albedo: [1, 1.12, 1.21], tint_masked: false, color_texture: "models/heroes_staging/haze/materials/haze_v2_color_psd_c9873321.vtex_c",
  used_by: [], shared: false, original_sha256: "a".repeat(64),
};
const catalog: ComponentCatalog = { hero_path: "", icons: [], audio: [], effects: [], animations: [], variants: [], animation_unavailable: null, warnings: [], materials: [entry] };
const start = { ...reference.cases[1].expected, hero_name: "Haze", customization: { enabled: true, model_enabled: false } } as unknown as Project;
function packet(details: Record<string, unknown> = {}) {
  const head = new TextEncoder().encode(JSON.stringify({ mime_type: "image/png", details }));
  const buffer = new ArrayBuffer(4 + head.length + 8);
  new DataView(buffer).setUint32(0, head.length, true);
  new Uint8Array(buffer, 4, head.length).set(head);
  return buffer;
}
beforeEach(() => {
  vi.clearAllMocks();
  clearExtraMediaCache();
  native.previewExtra.mockResolvedValue(packet());
  vi.stubGlobal("URL", Object.assign(URL, { createObjectURL: vi.fn(() => "blob:test"), revokeObjectURL: vi.fn() }));
});
function Harness({ project = start }: { project?: Project }) {
  const [p, set] = useState(structuredClone(project));
  return (
    <>
      <ExtrasWorkspace project={p} update={set} disabled={false} catalog={{ ...catalog, hero_path: p.original_model }} fail={vi.fn()} />
      <output data-testid="saved">{JSON.stringify(p)}</output>
    </>
  );
}
const saved = () => JSON.parse(screen.getByTestId("saved").textContent!) as Project;
const ranges = () => saved().customization.hero_materials?.materials?.[0]?.ranges;
function openBody() {
  fireEvent.click(screen.getByRole("tab", { name: /Colors/ }));
  fireEvent.click(within(screen.getByRole("listbox", { name: "Materials" })).getByRole("option", { name: /hazev2/ }));
}

describe("Hero color ranges", () => {
  it("add, edit and remove a color range on a hero material", async () => {
    render(<Harness />);
    openBody();
    await screen.findAllByAltText("hazev2 base color");
    fireEvent.click(screen.getByRole("button", { name: "Add color range" }));
    expect(ranges()).toEqual([{ enabled: true, ...RANGE_DEFAULTS }]);
    // A range that changes nothing yet makes no recolored copy (the build leaves it out).
    expect(screen.getByText(/its textures stay the game's own/)).toBeInTheDocument();
    expect(screen.queryByText(/recolored copy/)).toBeNull();
    // A range added by hand opens its Fine-tune; picking is "Recolor another color" (ColorsRework.test).
    expect(screen.getByRole("button", { name: /Hide fine-tuning/ })).toHaveAttribute("aria-expanded", "true");
    expect(within(screen.getByRole("group", { name: "Range 1" })).getByText(/no change yet/)).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("Range 1 hue shift"), { target: { value: "120" } });
    fireEvent.change(screen.getByLabelText("Range 1 saturation from"), { target: { value: "0.8" } });
    fireEvent.change(screen.getByLabelText("Range 1 saturation to"), { target: { value: "0.5" } });
    expect(ranges()?.[0]).toMatchObject({ hue_shift: 120, saturation_range: [0.5, 0.5] });
    // Typed values: a band and the selected hue width.
    fireEvent.focus(screen.getByLabelText("Range 1 brightness range value"));
    fireEvent.change(screen.getByLabelText("Range 1 brightness range value"), { target: { value: "20–90%" } });
    fireEvent.keyDown(screen.getByLabelText("Range 1 brightness range value"), { key: "Enter" });
    fireEvent.focus(screen.getByLabelText("Range 1 hue width value"));
    fireEvent.change(screen.getByLabelText("Range 1 hue width value"), { target: { value: "±25" } });
    fireEvent.blur(screen.getByLabelText("Range 1 hue width value"));
    expect(ranges()?.[0]).toMatchObject({ brightness_range: [0.2, 0.9], hue_width: 25 });
    // Every limit of today: softness 0–1, hue shift ±180, change factors 0–2.
    expect(screen.getByLabelText("Range 1 softness")).toHaveAttribute("max", "1");
    expect(screen.getByLabelText("Range 1 hue shift")).toHaveAttribute("min", "-180");
    expect(screen.getByLabelText("Range 1 saturation")).toHaveAttribute("max", "2");
    expect(screen.getByLabelText("Range 1 brightness")).toHaveAttribute("max", "2");
    // The material's own Brightness slider stays separate from the range's.
    fireEvent.click(screen.getByRole("button", { name: /^2\. Whole material/ }), { shiftKey: true });
    fireEvent.change(screen.getByLabelText("Brightness"), { target: { value: "0.5" } });
    expect(saved().customization.hero_materials?.materials?.[0]).toMatchObject({ brightness: 0.5, ranges: [expect.objectContaining({ hue_shift: 120 })] });
    // A range in use: a recolored copy of the texture is packaged, and nothing says the textures stay the game's.
    expect(screen.getByText(/adds a recolored copy of its base-color texture/)).toBeInTheDocument();
    expect(screen.getByText("The mod gets a recolored copy of this material's base-color texture.")).toBeInTheDocument();
    expect(screen.queryByText(/its textures stay the game's own/)).toBeNull();
    fireEvent.click(screen.getByLabelText("Use"));
    expect(ranges()?.[0].enabled).toBe(false);
    // No range in use: only the sentence that is true now.
    expect(screen.getByText(/its textures stay the game's own/)).toBeInTheDocument();
    expect(screen.queryByText(/recolored copy/)).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Remove" }));
    expect(ranges()).toBeUndefined();
    fireEvent.change(screen.getByLabelText("Brightness"), { target: { value: "1" } });
    expect(saved().customization.hero_materials?.materials).toEqual([]);
  });
  it("holds at most eight ranges", () => {
    const eight = Array.from({ length: 8 }, () => ({ enabled: true, ...RANGE_DEFAULTS }));
    render(<Harness project={{ ...start, customization: { enabled: true, model_enabled: false, hero_materials: { enabled: true, materials: [{ enabled: true, target: body, ranges: eight }] } } } as Project} />);
    openBody();
    // A name the list column cuts carries its full name and its file.
    expect(within(screen.getByRole("listbox", { name: "Materials" })).getByText("hazev2", { selector: "strong" })).toHaveAttribute("title", `hazev2\n${body}`);
    expect(screen.getByRole("button", { name: "Add color range" })).toBeDisabled();
    expect(screen.getByText("A material holds at most 8 color ranges.")).toBeInTheDocument();
  });
  it("says why a base-color texture cannot take color ranges", async () => {
    native.previewExtra.mockResolvedValue(packet({ ranges_unsupported: "Color ranges need a BC7 base-color texture; this one is DXT5." }));
    render(<Harness />);
    openBody();
    expect(await screen.findByText("Color ranges need a BC7 base-color texture; this one is DXT5.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Add color range" })).not.toBeInTheDocument();
  });
  it("maps a click on the cover-fitted preview to the image pixel and skips paused ranges", () => {
    // A 100x100 image shown in a 200x100 box is scaled 2x and cropped top and bottom.
    const box = { left: 10, top: 20, width: 200, height: 100 };
    expect(coverPixel(box, 10, 20 + 50, 100, 100)).toEqual({ x: 0, y: 50 });
    expect(coverPixel(box, 209, 119, 100, 100)).toEqual({ x: 99, y: 74 });
    expect(coverPixel(box, 5, 20, 100, 100)).toBeNull();
    expect(activeRanges({ ranges: [{ hue_shift: 30 }, { enabled: false, hue_shift: 60 }] }).map((r) => r.hue_shift)).toEqual([30]);
  });
});
