import { useState } from "react";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Project } from "../models";
import type { ComponentCatalog, HeroMaterialEntry } from "../components/AssetCatalog";
import { MIRROR_KEY, setLanguage } from "../i18n";
import { clearExtraMediaCache } from "./media";
import { forgetItemCatalog, Items } from "./Items";
import { itemCatalogFixture } from "./itemFixtures";
import { NAME_REFUSED_CHAR, type ItemNameRow, type ItemNamesState } from "./itemNames";
import { forgetObjects, Objects } from "./Objects";
import { daggerPacket, fitAnswer, hazeObjects, wandPacket } from "./objectFixtures";
import { HeroColors } from "./HeroColors";
import reference from "../../src-tauri/crates/deadlock-project/tests/fixtures/reference.json";

// The Colors, Items and Objects tabs driven in Chinese: their flows work by their Chinese
// names, and what they save stays as numbers, targets, paths and the names the user typed.
const native = vi.hoisted(() => ({
  validateAssetPath: vi.fn(), previewExtra: vi.fn(), previewImage: vi.fn(), preview: vi.fn(), itemCatalog: vi.fn(), itemNamesState: vi.fn(),
  abilityObjectCatalog: vi.fn(), prepareAbilityObject: vi.fn(), abilityObjectFit: vi.fn(), abilityObjectMotion: vi.fn(),
}));
vi.mock("../api", async (original) => ({ ...(await original<typeof import("../api")>()), api: native }));
const dialog = vi.hoisted(() => ({ openFor: vi.fn() }));
vi.mock("../dialogs", async (original) => ({ ...(await original<typeof import("../dialogs")>()), openFor: dialog.openFor }));
// WebGL cannot run here: the object view shows what it was given.
vi.mock("./ObjectScene", () => ({
  default: ({ answer, mode }: { answer: { yours: ArrayBuffer | null; view: { target: string } }; mode: string }) => <div data-testid="object-scene">{answer.view.target} {mode} {answer.yours ? "yours" : "game only"}</div>,
}));

const base = { ...reference.cases[1].expected, original_model: "models/heroes_staging/haze/haze.vmdl", hero_name: "Haze", customization: { enabled: true, model_enabled: false } } as unknown as Project;
function packet() {
  const head = new TextEncoder().encode(JSON.stringify({ mime_type: "image/png", details: {} }));
  const buffer = new ArrayBuffer(4 + head.length + 8);
  new DataView(buffer).setUint32(0, head.length, true);
  new Uint8Array(buffer, 4, head.length).set(head);
  return buffer;
}
function namesState(rows: ItemNameRow[]): ItemNamesState {
  const names: Record<string, string> = {};
  for (const item of items.items) if (item.items[0]) names[item.items[0].toLowerCase()] = item.label;
  return { found: true, names, languages: ["english", "schinese"], rows: rows.map((r) => ({ item: r.item, known: true })), unknown: [] };
}
const body = "models/heroes_staging/haze/materials/hazev2.vmat_c";
const material = (target: string): HeroMaterialEntry => ({
  path: target.replace(/_c$/, ""), target, label: target.split("/").pop()!.replace(".vmat_c", ""), shader: "pbr.vfx",
  supported: true, reason: "", tint: [1, 1, 1], albedo: [1, 1.12, 1.21], tint_masked: false,
  color_texture: target.replace(".vmat_c", "_color_psd_00000000.vtex_c"), used_by: [], shared: false, original_sha256: "a".repeat(64),
});
const colorCatalog: ComponentCatalog = { hero_path: base.original_model, icons: [], audio: [], effects: [], animations: [], variants: [], animation_unavailable: null, warnings: [], materials: [material(body)] };
let operate: ReturnType<typeof vi.fn>;
/** One item catalog per test (the fixture numbers its item records on every call). */
let items = itemCatalogFixture();
const fail = vi.fn();

function Page({ tab }: { tab: "items" | "objects" | "colors" }) {
  const [p, set] = useState(() => structuredClone(base));
  return (
    <>
      {tab === "items" && <Items project={p} update={set} disabled={false} canLoad revision={0} />}
      {tab === "objects" && <Objects project={p} update={set} disabled={false} canLoad hero="Haze" revision={0} shown operate={operate as never} fail={fail} />}
      {tab === "colors" && <HeroColors project={p} update={set} disabled={false} catalog={colorCatalog} discovering={false} canDiscover reload={vi.fn()} revision={0} heroName="Haze" />}
      <output data-testid="saved">{JSON.stringify(p)}</output>
    </>
  );
}
const saved = () => JSON.parse(screen.getByTestId("saved").textContent!) as Project;

beforeEach(async () => {
  vi.clearAllMocks();
  clearExtraMediaCache();
  forgetItemCatalog();
  forgetObjects();
  native.validateAssetPath.mockImplementation(async (path) => path);
  native.previewExtra.mockImplementation(async () => packet());
  native.previewImage.mockResolvedValue(new ArrayBuffer(8));
  native.preview.mockRejectedValue(new Error("The prepared model could not be read."));
  items = itemCatalogFixture();
  native.itemCatalog.mockImplementation(async () => structuredClone(items));
  native.itemNamesState.mockImplementation(async (rows: ItemNameRow[]) => namesState(rows));
  native.abilityObjectCatalog.mockImplementation(async () => hazeObjects());
  native.prepareAbilityObject.mockImplementation(async (_p, target, source) => ({ target, source, root: "models/dmb_objects/x", prepared: { counts: { meshes: 1, vertices: 24, triangles: 12, materials: 1 }, warnings: [] } }));
  native.abilityObjectFit.mockImplementation(async (_p, target, source) => fitAnswer(target, [daggerPacket()], source ? wandPacket() : null));
  operate = vi.fn(async (_label: string, op: () => Promise<unknown>) => op());
  dialog.openFor.mockResolvedValue("C:\\art\\my_wand.blend");
  vi.stubGlobal("URL", Object.assign(URL, { createObjectURL: vi.fn(() => "blob:test"), revokeObjectURL: vi.fn() }));
  await act(() => setLanguage("zh-CN"));
});
afterEach(async () => {
  cleanup();
  await act(() => setLanguage("en"));
  localStorage.removeItem(MIRROR_KEY);
});

describe("Extras in Chinese", () => {
  it("Items reads the game's items, shows the game's category words and switches live", async () => {
    render(<Page tab="items" />);
    const library = within(screen.getByRole("region", { name: "物品图标" }));
    expect(await library.findByRole("region", { name: "I 级" })).toHaveTextContent("800 魂魄");
    expect(native.itemCatalog).toHaveBeenCalledWith(false);
    expect(library.getByRole("tab", { name: /^武器/ })).toHaveAttribute("aria-selected", "true");
    fireEvent.click(library.getByRole("tab", { name: /^活力/ }));
    expect(library.getByRole("button", { name: "Extra Health" })).toBeInTheDocument();
    fireEvent.click(library.getByRole("tab", { name: /^传说/ }));
    expect(library.getByRole("region", { name: "已在游戏中关闭" })).toBeInTheDocument();
    // The search works by the names as they come, and says so in Chinese.
    fireEvent.change(library.getByRole("textbox", { name: "搜索物品" }), { target: { value: "extra" } });
    expect(library.getByText("所有分类中有 2 个物品与“extra”匹配")).toBeInTheDocument();
    await act(() => setLanguage("en"));
    expect(screen.getByRole("region", { name: "Item icons" })).toBeInTheDocument();
    expect(screen.getByText("2 items match “extra” in every category")).toBeInTheDocument();
    await act(() => setLanguage("zh-CN"));
    expect(screen.getByRole("region", { name: "物品图标" })).toBeInTheDocument();
  });

  it("Items saves a new name exactly as typed and shows the name rules as they come", async () => {
    render(<Page tab="items" />);
    fireEvent.click(await screen.findByRole("button", { name: "Close Quarters" }));
    const field = await screen.findByRole("textbox", { name: "物品名称" });
    fireEvent.change(field, { target: { value: "Bad <name" } });
    expect(screen.getByRole("alert")).toHaveTextContent(NAME_REFUSED_CHAR);
    expect(screen.getByRole("button", { name: "保存" })).toBeDisabled();
    fireEvent.change(field, { target: { value: "近战 Close" } });
    fireEvent.click(screen.getByRole("button", { name: "保存" }));
    expect(saved().customization.item_names).toEqual([{ item: items.items[0].items[0], enabled: true, name: "近战 Close" }]);
    expect(await screen.findByText("已重命名")).toBeInTheDocument();
    expect(screen.getByText("游戏中为 Close Quarters")).toBeInTheDocument();
  });

  it("Items says in Chinese when the game's items cannot be read, with the reason as it comes", async () => {
    native.itemCatalog.mockRejectedValue({ kind: "tool", message: "Choose the original game VPK in Setup." });
    render(<Page tab="items" />);
    expect(await screen.findByRole("heading", { name: "无法读取游戏的物品" })).toBeInTheDocument();
    expect(screen.getByText(/Choose the original game VPK in Setup\./)).toBeInTheDocument();
    native.itemCatalog.mockImplementation(async () => structuredClone(items));
    fireEvent.click(screen.getByRole("button", { name: "重试" }));
    expect(await screen.findByRole("button", { name: "Close Quarters" })).toBeInTheDocument();
  });

  it("Objects shows an object through the app's runner, saves the file and the fit as values, and keeps the English failure title", async () => {
    render(<Page tab="objects" />);
    const list = within(await screen.findByRole("region", { name: "技能物体" }));
    expect(await list.findByRole("region", { name: "1 · Sleep Dagger" })).toBeInTheDocument();
    expect(list.getByText(/^Haze 的技能共有 2 个物体。/)).toBeInTheDocument();
    fireEvent.click(list.getByRole("button", { name: "Dagger" }));
    fireEvent.click(await screen.findByRole("button", { name: "显示此物体" }));
    await waitFor(() => expect(native.abilityObjectFit).toHaveBeenCalled());
    expect(operate.mock.calls[0][0]).toBe("The game's object could not be shown");
    expect(await screen.findByText("游戏的物体，半透明，边缘为虚线。选择一个文件即可在旁边查看你的物体。")).toBeInTheDocument();
    const details = within(screen.getByRole("complementary", { name: "物体详情" }));
    fireEvent.click(details.getByRole("button", { name: "选择文件…" }));
    await waitFor(() => expect(native.prepareAbilityObject).toHaveBeenCalled());
    expect(dialog.openFor.mock.calls[0][1]).toMatchObject({ title: "选择你的物体的模型文件" });
    expect(operate.mock.calls.map((call) => call[0])).toContain("Your object file could not be prepared");
    fireEvent.click(await details.findByRole("button", { name: "保持我的大小" }));
    fireEvent.click(details.getByRole("button", { name: "⟲ 滚转 90°" }));
    expect(saved().customization.ability_objects).toEqual([{ target: "models/abilities/haze_dagger_model.vmdl", enabled: true, source: "C:\\art\\my_wand.blend", fit: { size: "keep", turns: { roll: 1 } } }]);
    await act(() => setLanguage("en"));
    expect(screen.getByRole("complementary", { name: "Object details" })).toBeInTheDocument();
  });

  it("Objects says in Chinese when the view cannot load, with the reason as it comes", async () => {
    native.abilityObjectFit.mockRejectedValue({ kind: "tool", message: "The decompiler stopped." });
    render(<Page tab="objects" />);
    fireEvent.click(await screen.findByRole("button", { name: "Dagger" }));
    fireEvent.click(await screen.findByRole("button", { name: "显示此物体" }));
    expect(await screen.findByText(/^视图无法加载：.*The decompiler stopped\./)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "重试" })).toBeInTheDocument();
  });

  it("Colors edits a hero material by its Chinese names, saves numbers only, and switches live", async () => {
    render(<Page tab="colors" />);
    fireEvent.click(within(screen.getByRole("listbox", { name: "材质" })).getByRole("option", { name: /hazev2/ }));
    // The 3D view cannot be read here: the page says so in Chinese and offers the Texture tab.
    expect(await screen.findByText(/^无法在 3D 中显示英雄的模型：/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "重试" })).toBeInTheDocument();
    const tint = screen.getByRole("button", { name: /^3\. 色调：/ });
    if (tint.getAttribute("aria-expanded") !== "true") fireEvent.click(tint, { shiftKey: true });
    fireEvent.click(screen.getByRole("button", { name: "黑色" }));
    const whole = screen.getByRole("button", { name: /^2\. 整个材质：/ });
    if (whole.getAttribute("aria-expanded") !== "true") fireEvent.click(whole, { shiftKey: true });
    fireEvent.change(screen.getByLabelText("亮度"), { target: { value: "0.5" } });
    expect(saved().customization.hero_materials).toEqual({
      enabled: true,
      materials: [{ enabled: true, target: body, original_sha256: "a".repeat(64), allow_shared: false, color: [0, 0, 0], brightness: 0.5 }],
    });
    expect(screen.getByRole("option", { name: /hazev2/ })).toHaveTextContent("色调 · 亮度 50%");
    expect(screen.getByText("已更改 1 项")).toBeInTheDocument();
    // A color range added by hand keeps only numbers in the project.
    const first = screen.getByRole("button", { name: /^1\. 局部改色：/ });
    if (first.getAttribute("aria-expanded") !== "true") fireEvent.click(first, { shiftKey: true });
    fireEvent.click(screen.getByRole("button", { name: "添加颜色范围" }));
    const range = saved().customization.hero_materials?.materials?.[0].ranges?.[0] ?? {};
    for (const value of Object.values(range)) expect(typeof value === "string").toBe(false);
    expect(screen.getByRole("group", { name: "范围 1" })).toBeInTheDocument();
    await act(() => setLanguage("en"));
    expect(screen.getByRole("heading", { name: "Colors" })).toBeInTheDocument();
    expect(screen.getByRole("group", { name: "Range 1" })).toBeInTheDocument();
    await act(() => setLanguage("zh-CN"));
    expect(screen.getByRole("heading", { name: "颜色" })).toBeInTheDocument();
  });
});
