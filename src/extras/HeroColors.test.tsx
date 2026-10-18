import { useState } from "react";
import { fireEvent, render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Project } from "../models";
import type { ComponentCatalog, HeroMaterialEntry } from "../components/AssetCatalog";
import { ExtrasWorkspace } from "./ExtrasPage";
import { heroColorItems } from "./HeroColors";
import { clearExtraMediaCache } from "./media";
import reference from "../../src-tauri/crates/deadlock-project/tests/fixtures/reference.json";
const native = vi.hoisted(() => ({ validateAssetPath: vi.fn(), previewExtra: vi.fn() }));
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
const start = { ...reference.cases[1].expected, hero_name: "Haze", customization: { enabled: true } } as unknown as Project;
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
function openColors() {
  fireEvent.click(screen.getByRole("tab", { name: /Colors/ }));
}

describe("Hero colors", () => {
  it("recolors a hero material without a replacement model", async () => {
    render(<Harness />);
    openColors();
    expect(screen.getByRole("note")).toHaveTextContent("Colors recolor the game's own model");
    fireEvent.click(screen.getByRole("button", { name: "Build without a replacement model" }));
    expect(saved().customization.model_enabled).toBe(false);
    expect(screen.queryByRole("note")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /hazev2/ }));
    expect(await screen.findAllByAltText("hazev2 base color")).toHaveLength(2);
    expect(native.previewExtra).toHaveBeenCalledWith(expect.objectContaining({ kind: "materials", target: body }));
    fireEvent.click(screen.getByLabelText("Change tint"));
    fireEvent.click(screen.getByRole("button", { name: "Black" }));
    fireEvent.change(screen.getByLabelText("Brightness"), { target: { value: "0.5" } });
    expect(saved().customization.hero_materials).toEqual({
      enabled: true,
      materials: [{ enabled: true, target: body, original_sha256: "a".repeat(64), allow_shared: false, color: [0, 0, 0], brightness: 0.5 }],
    });
    expect(screen.getByRole("tab", { name: /Colors/ })).toHaveTextContent("3");
    // Taking every change back drops the color instead of saving an empty one.
    fireEvent.click(screen.getByLabelText("Change tint"));
    fireEvent.change(screen.getByLabelText("Brightness"), { target: { value: "1" } });
    expect(saved().customization.hero_materials?.materials).toEqual([]);
  });
  it("marks shared materials, offers only their settings and keeps unsupported ones view-only", () => {
    render(<Harness project={{ ...start, customization: { enabled: true, model_enabled: false } }} />);
    openColors();
    const list = screen.getByRole("list", { name: "Materials" });
    expect(within(list).getByRole("button", { name: /haze_v2_gun/ })).toHaveTextContent("Shared");
    expect(within(list).getByRole("button", { name: /decal/ })).toHaveTextContent("View only");
    fireEvent.click(within(list).getByRole("button", { name: /haze_v2_gun/ }));
    expect(screen.getByText(/Also used by haze_ult/)).toBeInTheDocument();
    expect(screen.getByText("This material has no saturation or brightness setting.")).toBeInTheDocument();
    fireEvent.click(screen.getByLabelText("Change tint"));
    fireEvent.click(screen.getByLabelText("Recolor everywhere it is used"));
    expect(saved().customization.hero_materials?.materials?.[0]).toMatchObject({ target: gun, allow_shared: true, color: [1, 1, 1] });
    fireEvent.click(within(list).getByRole("button", { name: /decal/ }));
    expect(screen.getByText(/projected_decals.vfx shader/)).toBeInTheDocument();
    expect(screen.queryByLabelText("Change tint")).not.toBeInTheDocument();
  });
  it("keeps saved colors of materials the game no longer lists so they can be removed", () => {
    const gone = "models/heroes_staging/haze/materials/old.vmat_c";
    const items = heroColorItems(catalog.materials!, [{ target: gone, color: [0, 0, 0] }, { target: body.toUpperCase(), brightness: 0 }]);
    expect(items.find((item) => item.entry.target === body)?.index).toBe(1);
    expect(items.find((item) => item.entry.target === gone)).toMatchObject({ missing: true, index: 0 });
    render(<Harness project={{ ...start, customization: { enabled: true, model_enabled: false, hero_materials: { enabled: true, materials: [{ enabled: true, target: gone, color: [0, 0, 0] }] } } }} />);
    openColors();
    fireEvent.click(screen.getByRole("button", { name: /old/ }));
    expect(screen.getByText("This material is no longer in the game files")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Remove color" }));
    expect(saved().customization.hero_materials?.materials).toEqual([]);
  });
  it("pauses hero colors with the other extras", () => {
    render(<Harness project={{ ...start, customization: { enabled: true, model_enabled: false, hero_materials: { enabled: true, materials: [{ enabled: true, target: body, color: [0, 0, 0] }] } } }} />);
    expect(screen.getByText("1 active replacement")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Pause all extras" }));
    expect(saved().customization.hero_materials?.materials?.[0].enabled).toBe(false);
  });
});
