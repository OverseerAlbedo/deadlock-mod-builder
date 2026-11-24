import { useState } from "react";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Project } from "../models";
import type { ComponentCatalog, HeroMaterialEntry } from "../components/AssetCatalog";
import { ExtrasWorkspace } from "./ExtrasPage";
import { heroColorItems } from "./HeroColors";
import { clearExtraMediaCache } from "./media";
import { changeCustomization } from "../components/customization";
import reference from "../../src-tauri/crates/deadlock-project/tests/fixtures/reference.json";
const native = vi.hoisted(() => ({ validateAssetPath: vi.fn(), previewExtra: vi.fn(), previewImage: vi.fn(), preview: vi.fn() }));
vi.mock("../api", async (original) => ({ ...(await original<typeof import("../api")>()), api: native }));

const body = "models/heroes_staging/haze/materials/hazev2.vmat_c";
const gun = "models/heroes_staging/haze_v2/materials/haze_v2_gun.vmat_c";
const decal = "models/heroes_staging/haze/materials/decal.vmat_c";
const entry = (target: string, values: Partial<HeroMaterialEntry> = {}): HeroMaterialEntry => ({
  path: target.replace(/_c$/, ""), target, label: target.split("/").pop()!.replace(".vmat_c", ""), shader: "pbr.vfx",
  supported: true, reason: "", tint: [1, 1, 1], albedo: [1, 1.12, 1.21], tint_masked: false,
  color_texture: target.replace(".vmat_c", "_color_psd_00000000.vtex_c"), used_by: [], shared: false, original_sha256: "a".repeat(64), ...values,
});
const catalog: ComponentCatalog = {
  hero_path: "", icons: [], audio: [], effects: [], animations: [], variants: [], animation_unavailable: null, warnings: [],
  materials: [
    entry(body),
    entry(gun, { shared: true, used_by: ["models/heroes_staging/haze/haze_ult.vmdl_c"], albedo: null }),
    entry(decal, { supported: false, shader: "projected_decals.vfx", reason: "decal uses the projected_decals.vfx shader. Hero colors support pbr.vfx materials only." }),
  ],
};
const start = { ...reference.cases[1].expected, hero_name: "Haze", customization: { enabled: true, model_enabled: false } } as unknown as Project;
/** The replacement mode project: two prepared materials (one with a base-color image) and a Surface changes row. */
const cloth = "materials/body/cloth.vmat", skin = "materials/body/skin.vmat";
const replacement = {
  ...start,
  dmx: "C:/work/prepared.dmx",
  materials: { [cloth]: { mode: "generate", preset: "cloth", textures: { color: "C:/textures/cloth.png" } }, [skin]: { mode: "generate", textures: {}, color: [0.8, 0.6, 0.5] } },
  customization: { enabled: true, appearance: { enabled: true, materials: [{ enabled: true, target: cloth, roughness: 0.4 }] } },
} as unknown as Project;
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
  native.previewImage.mockResolvedValue(new ArrayBuffer(8));
  native.preview.mockRejectedValue(new Error("The prepared model could not be read."));
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
const openColors = () => fireEvent.click(screen.getByRole("tab", { name: /Colors/ }));
const list = () => screen.getByRole("listbox", { name: "Materials" });
const choose = (name: RegExp) => fireEvent.click(within(list()).getByRole("option", { name }));
/** Opens one of the editor's steps (Shift keeps the others open). */
const openStep = (name: RegExp) => {
  const head = screen.getByRole("button", { name });
  if (head.getAttribute("aria-expanded") !== "true") fireEvent.click(head, { shiftKey: true });
};

describe("Hero colors", () => {
  it("recolors a hero material without a replacement model", async () => {
    render(<Harness />);
    openColors();
    choose(/hazev2/);
    // No hero model in this version: the Texture tab shows the game's base color.
    expect(await screen.findAllByAltText("hazev2 base color")).toHaveLength(2);
    expect(native.previewExtra).toHaveBeenCalledWith(expect.objectContaining({ kind: "materials", target: body, max_dimension: 512 }));
    await waitFor(() => expect(screen.getByText(/cannot show the hero's own model yet/)).toBeInTheDocument());
    openStep(/^3\. Tint/);
    fireEvent.click(screen.getByRole("button", { name: "Black" }));
    openStep(/^2\. Whole material/);
    fireEvent.change(screen.getByLabelText("Brightness"), { target: { value: "0.5" } });
    expect(saved().customization.hero_materials).toEqual({
      enabled: true,
      materials: [{ enabled: true, target: body, original_sha256: "a".repeat(64), allow_shared: false, color: [0, 0, 0], brightness: 0.5 }],
    });
    expect(within(list()).getByRole("option", { name: /hazev2/ })).toHaveTextContent("Tint · Brightness 50%");
    expect(screen.getByText("1 changed")).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /Colors/ })).toHaveTextContent("3");
    // Taking every change back drops the color instead of saving an empty one.
    fireEvent.click(screen.getByRole("button", { name: "Game color" }));
    fireEvent.change(screen.getByLabelText("Brightness"), { target: { value: "1" } });
    expect(saved().customization.hero_materials?.materials).toEqual([]);
  });
  it("marks shared materials, offers only their settings and keeps unsupported ones view-only", () => {
    render(<Harness />);
    openColors();
    expect(within(list()).getByRole("option", { name: /haze_v2_gun/ })).toHaveTextContent("Shared");
    expect(within(list()).getByRole("option", { name: /decal/ })).toHaveTextContent("View only");
    // Groups by the folder the material sits in.
    expect(within(list()).getByRole("group", { name: "haze_v2" })).toHaveTextContent("haze_v2_gun");
    choose(/haze_v2_gun/);
    expect(screen.getByText(/Also used by haze_ult/)).toBeInTheDocument();
    openStep(/^2\. Whole material/);
    expect(screen.getByText("This material has no saturation or brightness setting.")).toBeInTheDocument();
    expect(screen.queryByLabelText("Brightness")).not.toBeInTheDocument();
    openStep(/^3\. Tint/);
    fireEvent.change(screen.getByLabelText("Tint hex color"), { target: { value: "#ff0000" } });
    fireEvent.blur(screen.getByLabelText("Tint hex color"));
    fireEvent.click(screen.getByLabelText("Recolor everywhere it is used"));
    expect(saved().customization.hero_materials?.materials?.[0]).toMatchObject({ target: gun, allow_shared: true, color: [1, 0, 0] });
    choose(/decal/);
    expect(screen.getByText(/projected_decals.vfx shader/)).toBeInTheDocument();
    expect(screen.queryByLabelText("Tint hex color")).not.toBeInTheDocument();
  });
  it("says a material has no tint and keeps the tint mask hint", () => {
    const masked = { ...catalog, materials: [entry(body, { tint_masked: true }), entry(gun, { tint: null })] };
    function Masked() {
      const [p, set] = useState(structuredClone(start));
      return <ExtrasWorkspace project={p} update={set} disabled={false} catalog={{ ...masked, hero_path: p.original_model }} fail={vi.fn()} />;
    }
    render(<Masked />);
    openColors();
    choose(/hazev2/);
    openStep(/^3\. Tint/);
    expect(screen.getByText("The game limits this material's tint to some areas (its tint mask).")).toBeInTheDocument();
    choose(/haze_v2_gun/);
    openStep(/^3\. Tint: not available/);
    expect(screen.getByText("This material has no tint setting.")).toBeInTheDocument();
    expect(screen.queryByLabelText("Tint hex color")).not.toBeInTheDocument();
  });
  it("keeps saved colors of materials the game no longer lists so they can be removed", () => {
    const gone = "models/heroes_staging/haze/materials/old.vmat_c";
    const items = heroColorItems(catalog.materials!, [{ target: gone, color: [0, 0, 0] }, { target: body.toUpperCase(), brightness: 0 }]);
    expect(items.find((item) => item.entry.target === body)?.index).toBe(1);
    expect(items.find((item) => item.entry.target === gone)).toMatchObject({ missing: true, index: 0 });
    render(<Harness project={{ ...start, customization: { enabled: true, model_enabled: false, hero_materials: { enabled: true, materials: [{ enabled: true, target: gone, color: [0, 0, 0] }] } } }} />);
    openColors();
    choose(/old/);
    expect(screen.getByText("This material is no longer in the game files")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Remove color" }));
    expect(saved().customization.hero_materials?.materials).toEqual([]);
  });
  it("pauses hero colors with the other extras and with Include in build", () => {
    render(<Harness project={{ ...start, customization: { enabled: true, model_enabled: false, hero_materials: { enabled: true, materials: [{ enabled: true, target: body, color: [0, 0, 0] }] } } }} />);
    expect(screen.getByText("1 active change")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Pause all extras" }));
    expect(saved().customization.hero_materials?.materials?.[0].enabled).toBe(false);
    openColors();
    expect(within(list()).getByRole("option", { name: /hazev2/ })).toHaveTextContent("Paused");
    choose(/hazev2/);
    fireEvent.click(screen.getByLabelText("Include in build"));
    expect(saved().customization.hero_materials?.materials?.[0].enabled).toBe(true);
  });
  it("filters, searches and resets every color after a confirmation", () => {
    render(<Harness project={{ ...start, customization: { enabled: true, model_enabled: false, hero_materials: { enabled: true, materials: [{ enabled: true, target: body, color: [0, 0, 0] }] } } }} />);
    openColors();
    fireEvent.click(screen.getByRole("radio", { name: "Changed 1" }));
    expect(within(list()).getAllByRole("option")).toHaveLength(1);
    fireEvent.click(screen.getByRole("radio", { name: "All 3" }));
    fireEvent.change(screen.getByLabelText("Search materials"), { target: { value: "gun" } });
    expect(within(list()).getAllByRole("option").map((o) => o.textContent)).toEqual([expect.stringContaining("haze_v2_gun")]);
    fireEvent.click(screen.getByRole("button", { name: "Reset all colors…" }));
    expect(saved().customization.hero_materials?.materials).toHaveLength(1);
    fireEvent.click(within(screen.getByRole("dialog", { name: "Reset all colors" })).getByRole("button", { name: "Reset all" }));
    expect(saved().customization.hero_materials?.materials).toEqual([]);
  });
  it("moves the selection with the arrow keys", () => {
    render(<Harness />);
    openColors();
    choose(/hazev2/);
    fireEvent.keyDown(list(), { key: "ArrowDown" });
    expect(within(list()).getByRole("option", { selected: true })).toHaveTextContent("decal");
    fireEvent.keyDown(list(), { key: "Home" });
    expect(within(list()).getByRole("option", { selected: true })).toHaveTextContent("hazev2");
    expect(list()).toHaveAttribute("aria-activedescendant", within(list()).getByRole("option", { selected: true }).id);
  });
});

describe("Colors with a replacement model", () => {
  it("edits the replacement's Surface changes rows and keeps their other values", async () => {
    render(<Harness project={replacement} />);
    openColors();
    expect(screen.queryByText(/Colors recolor the game's own model/)).not.toBeInTheDocument();
    expect(within(list()).getAllByRole("option")).toHaveLength(2);
    choose(/cloth/);
    await waitFor(() => expect(native.previewImage).toHaveBeenCalledWith("C:/textures/cloth.png", 1024));
    await waitFor(() => expect(screen.getByText(/The prepared model could not be read/)).toBeInTheDocument());
    openStep(/^2\. Whole material/);
    fireEvent.change(screen.getByLabelText("Hue shift"), { target: { value: "40" } });
    openStep(/^3\. Tint/);
    fireEvent.click(screen.getByRole("button", { name: "Black" }));
    openStep(/^1\. Recolor parts/);
    fireEvent.click(screen.getByRole("button", { name: "Add color range" }));
    const row = () => saved().customization.appearance?.materials?.[0];
    expect(row()).toMatchObject({ enabled: true, target: cloth, roughness: 0.4, hue: 40, color: [0, 0, 0], ranges: [expect.objectContaining({ hue_width: 180 })] });
    // Remove color takes the colors away and keeps the roughness.
    fireEvent.click(screen.getByRole("button", { name: "Remove color" }));
    expect(saved().customization.appearance?.materials).toEqual([{ enabled: true, target: cloth, roughness: 0.4 }]);
  });
  it("says why a plain base color cannot take color ranges and adds a row only for a real change", () => {
    render(<Harness project={replacement} />);
    openColors();
    choose(/skin/);
    openStep(/^1\. Recolor parts: not available/);
    expect(screen.getByText(/This material has a plain base color, not an image/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Add color range" })).not.toBeInTheDocument();
    openStep(/^2\. Whole material/);
    fireEvent.change(screen.getByLabelText("Saturation"), { target: { value: "1" } });
    expect(saved().customization.appearance?.materials).toHaveLength(1);
    fireEvent.change(screen.getByLabelText("Saturation"), { target: { value: "1.2" } });
    expect(saved().customization.appearance?.materials?.[1]).toEqual({ enabled: true, target: skin, saturation: 1.2 });
    fireEvent.change(screen.getByLabelText("Saturation"), { target: { value: "1" } });
    expect(saved().customization.appearance?.materials).toHaveLength(1);
  });
  it("notes saved hero colors that a replacement build leaves out, with the switch", () => {
    const both = { ...replacement, customization: { ...replacement.customization, hero_materials: { enabled: true, materials: [{ enabled: true, target: body, color: [0, 0, 0] }] } } } as Project;
    render(<Harness project={both} />);
    openColors();
    expect(screen.getByRole("note")).toHaveTextContent("1 saved hero color not used with a replacement model");
    fireEvent.click(screen.getByRole("button", { name: "Build without a replacement model" }));
    expect(saved().customization.model_enabled).toBe(false);
    expect(within(list()).getByRole("option", { name: /hazev2/ })).toHaveTextContent("Changed");
  });
  it("counts the replacement's color rows in the Extras header and tab, and Pause all leaves them alone", () => {
    const colored = { ...replacement, customization: { ...replacement.customization, appearance: { enabled: true, materials: [{ enabled: true, target: cloth, roughness: 0.4, hue: 30 }] }, hero_materials: { enabled: true, materials: [{ enabled: true, target: body, color: [0, 0, 0] }] }, icons: [{ enabled: true, target: "panorama/x.vtex_c", source: "C:/x.png" }] } } as unknown as Project;
    render(<Harness project={colored} />);
    const tab = screen.getByRole("tab", { name: /Colors/ });
    // Two materials in the replacement; one color row; the saved hero color is not counted.
    expect(tab).toHaveTextContent("2");
    expect(within(tab).getByTitle("Contains color changes")).toBeInTheDocument();
    expect(screen.getByText("2 changes")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Pause all extras" }));
    expect(saved().customization.appearance?.materials?.[0]).toEqual({ enabled: true, target: cloth, roughness: 0.4, hue: 30 });
    expect(saved().customization.icons?.[0].enabled).toBe(false);
  });
  it("says in the footer that Pause all does not pause the replacement's color changes", () => {
    const colored = { ...replacement, customization: { ...replacement.customization, appearance: { enabled: true, materials: [{ enabled: true, target: cloth, roughness: 0.4, hue: 30 }] } } } as unknown as Project;
    render(<Harness project={colored} />);
    expect(screen.getByText("1 change")).toBeInTheDocument();
    // The footer keeps counting what Pause all extras pauses, and names the color change it leaves on.
    expect(screen.getByText("0 active changes")).toBeInTheDocument();
    expect(screen.getByText("Your replacement model stays included, with its 1 color change, which Pause all extras does not pause")).toBeInTheDocument();
  });
  it("counts hero colors in game hero mode", () => {
    render(<Harness project={{ ...start, customization: { enabled: true, model_enabled: false, hero_materials: { enabled: true, materials: [{ enabled: true, target: body, color: [0, 0, 0] }, { enabled: true, target: gun, brightness: 0.5 }] } } } as Project} />);
    const tab = screen.getByRole("tab", { name: /Colors/ });
    expect(tab).toHaveTextContent("3");
    expect(within(tab).getByTitle("Contains color changes")).toBeInTheDocument();
    expect(screen.getByText("2 changes")).toBeInTheDocument();
  });
  it("asks for a prepared model before the 3D view and opens Prepare model", () => {
    const open = vi.fn();
    render(<ExtrasWorkspace project={{ ...replacement, dmx: "" } as Project} update={vi.fn()} disabled={false} catalog={null} fail={vi.fn()} openPreparation={open} />);
    fireEvent.click(screen.getByRole("tab", { name: /Colors/ }));
    fireEvent.click(screen.getByRole("button", { name: /Open Prepare model/ }));
    expect(open).toHaveBeenCalledTimes(1);
  });
  it("asks for a prepared model before the 3D view", () => {
    render(<Harness project={{ ...replacement, dmx: "" } as Project} />);
    openColors();
    expect(screen.getByText("No prepared model yet")).toBeInTheDocument();
    expect(native.preview).not.toHaveBeenCalled();
  });
  it("offers to recolor the game's own hero when no model is prepared, and only switches the replacement model off", () => {
    const unprepared = { ...replacement, dmx: "" } as Project;
    render(<Harness project={unprepared} />);
    openColors();
    expect(screen.getByText("The build then changes the colors of the game's own model and includes no replacement model.")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Recolor the game's own Haze instead" }));
    // Exactly what Build without a replacement model writes.
    expect(saved()).toEqual(JSON.parse(JSON.stringify(changeCustomization(unprepared, { model_enabled: false }))));
    expect(saved().customization.model_enabled).toBe(false);
    // Now in game hero mode: the offer is gone.
    expect(screen.queryByRole("button", { name: /Recolor the game's own/ })).not.toBeInTheDocument();
  });
  it("asks for the hero first, with no recolor offer, when the project has no hero", () => {
    render(<Harness project={{ ...replacement, dmx: "", hero_name: "", original_model: "" } as unknown as Project} />);
    expect(screen.getByRole("heading", { name: "Choose the hero first" })).toBeInTheDocument();
    expect(screen.queryByRole("tab", { name: /Colors/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Recolor the game's own/ })).not.toBeInTheDocument();
  });
  it("does not offer to recolor the game's own hero when a model is prepared", () => {
    render(<Harness project={replacement} />);
    openColors();
    expect(screen.queryByRole("button", { name: /Recolor the game's own/ })).not.toBeInTheDocument();
    expect(screen.queryByText(/includes no replacement model/)).not.toBeInTheDocument();
  });
});
