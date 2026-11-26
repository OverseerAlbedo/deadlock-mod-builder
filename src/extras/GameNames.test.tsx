import { useState } from "react";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Project } from "../models";
import type { ComponentCatalog } from "../components/AssetCatalog";
import { MIRROR_KEY, setLanguage } from "../i18n";
import { forgetGameNames, loadGameNames, type GameNames } from "../i18n/gameNames";
import { HeroCopyNotice } from "../components/HeroCopyNotice";
import { SeparateGun, type SeparateGunProps } from "../preparation/SeparateGun";
import { ExtrasWorkspace } from "./ExtrasPage";
import { clearExtraMediaCache } from "./media";
import { forgetExtrasTabs } from "./extrasTabs";
import { forgetItemCatalog, Items } from "./Items";
import { itemCatalogFixture } from "./itemFixtures";
import type { ItemNameRow, ItemNamesState } from "./itemNames";
import reference from "../../src-tauri/crates/deadlock-project/tests/fixtures/reference.json";

// The game's own names in the interface language on the Extras pages and the copy notice:
// shown (tiles, side panel, chips, sentences) and searched, while what is saved and sent stays
// the English name, the token or the name the user typed.
const native = vi.hoisted(() => ({ validateAssetPath: vi.fn(), previewExtra: vi.fn(), itemCatalog: vi.fn(), itemNamesState: vi.fn(), gameNames: vi.fn(), discoverWeaponModels: vi.fn() }));
vi.mock("../api", async (original) => ({ ...(await original<typeof import("../api")>()), api: native }));
vi.mock("./AudioPlayer", () => ({ AudioPlayer: () => <div /> }));

const base = { ...reference.cases[1].expected, original_model: "models/heroes_staging/haze/haze.vmdl", hero_name: "Haze", customization: { enabled: true, model_enabled: false } } as unknown as Project;
let items = itemCatalogFixture();
/** Made-up names in the shape the command answers with: Close Quarters and Extended Magazine. */
function chineseNames(): GameNames {
  return {
    language: "zh-CN",
    heroes: { Haze: "海兹", Vindicta: "维迪塔" },
    items: { [items.items[0].items[0].toLowerCase()]: "近战专家", upgrade_clip_size: "扩容弹匣", item_projectile_test_01: "投射物测试" },
    missing: [],
  };
}
function namesState(rows: ItemNameRow[]): ItemNamesState {
  const names: Record<string, string> = {};
  for (const item of items.items) if (item.items[0]) names[item.items[0].toLowerCase()] = item.label;
  return { found: true, names, languages: ["english", "schinese"], rows: rows.map((r) => ({ item: r.item, known: true })), unknown: [] };
}
function packet() {
  const head = new TextEncoder().encode(JSON.stringify({ mime_type: "image/png", details: {} }));
  const buffer = new ArrayBuffer(4 + head.length + 8);
  new DataView(buffer).setUint32(0, head.length, true);
  new Uint8Array(buffer, 4, head.length).set(head);
  return buffer;
}
function ItemsPage() {
  const [p, set] = useState(() => structuredClone(base));
  return (
    <>
      <Items project={p} update={set} disabled={false} canLoad revision={0} />
      <output data-testid="saved">{JSON.stringify(p)}</output>
    </>
  );
}
const saved = () => JSON.parse(screen.getByTestId("saved").textContent!) as Project;
const library = () => within(screen.getByRole("region", { name: /^(Item icons|物品图标)$/ }));
const searchBox = () => library().getByRole("textbox", { name: /^(Search items|搜索物品)$/ });
const catalog: ComponentCatalog = { hero_path: base.original_model, icons: [], audio: [], effects: [], animations: [], variants: [], animation_unavailable: null, warnings: [] };

beforeEach(async () => {
  vi.clearAllMocks();
  clearExtraMediaCache();
  forgetItemCatalog();
  forgetExtrasTabs();
  forgetGameNames();
  items = itemCatalogFixture();
  native.validateAssetPath.mockImplementation(async (path) => path);
  native.previewExtra.mockImplementation(async () => packet());
  native.itemCatalog.mockImplementation(async () => structuredClone(items));
  native.itemNamesState.mockImplementation(async (rows: ItemNameRow[]) => namesState(rows));
  native.gameNames.mockImplementation(async () => chineseNames());
  vi.stubGlobal("URL", Object.assign(URL, { createObjectURL: vi.fn(() => "blob:test"), revokeObjectURL: vi.fn() }));
});
afterEach(async () => {
  cleanup();
  await act(() => setLanguage("en"));
  localStorage.removeItem(MIRROR_KEY);
  forgetGameNames();
});
async function inChinese(withNames = true) {
  await act(() => setLanguage("zh-CN"));
  if (withNames) await act(() => loadGameNames("zh-CN"));
}

describe("Game item names on the Items page", () => {
  it("tiles and the side panel show the game's Chinese names, and the search finds an item by either name", async () => {
    await inChinese();
    render(<ItemsPage />);
    expect(await library().findByRole("button", { name: "近战专家" })).toHaveTextContent("近战专家");
    expect(library().getByRole("button", { name: "扩容弹匣" })).toBeInTheDocument();
    // An item the game names in no other language keeps its English name.
    expect(library().getByRole("button", { name: "High-Velocity Rounds" })).toBeInTheDocument();
    fireEvent.change(searchBox(), { target: { value: "扩容" } });
    expect(library().getByRole("button", { name: "扩容弹匣" })).toBeInTheDocument();
    expect(library().queryByRole("button", { name: "近战专家" })).toBeNull();
    fireEvent.change(searchBox(), { target: { value: "close quarters" } });
    expect(library().getByRole("button", { name: "近战专家" })).toBeInTheDocument();
    expect(library().queryByRole("button", { name: "扩容弹匣" })).toBeNull();
    fireEvent.change(searchBox(), { target: { value: "" } });
    fireEvent.click(library().getByRole("button", { name: "扩容弹匣" }));
    const field = await screen.findByRole("textbox", { name: "物品名称" });
    expect(field).toHaveValue("扩容弹匣");
    expect(screen.getByText("游戏中为扩容弹匣")).toBeInTheDocument();
    // The other record's name, as the game shows it.
    expect(screen.getByText(/投射物测试/)).toBeInTheDocument();
  });

  it("a rename is saved exactly as typed under the item's token, and the game's shown name removes it again", async () => {
    await inChinese();
    render(<ItemsPage />);
    fireEvent.click(await library().findByRole("button", { name: "近战专家" }));
    const field = await screen.findByRole("textbox", { name: "物品名称" });
    // Unchanged: the game's shown name saves nothing.
    expect(screen.getByRole("button", { name: "保存" })).toBeDisabled();
    fireEvent.change(field, { target: { value: "Close 近战" } });
    fireEvent.click(screen.getByRole("button", { name: "保存" }));
    const token = items.items[0].items[0];
    expect(saved().customization.item_names).toEqual([{ item: token, enabled: true, name: "Close 近战" }]);
    expect(await library().findByRole("button", { name: /^Close 近战/ })).toHaveTextContent("近战专家");
    fireEvent.change(screen.getByRole("textbox", { name: "物品名称" }), { target: { value: "近战专家" } });
    fireEvent.click(screen.getByRole("button", { name: "保存" }));
    expect(saved().customization.item_names ?? []).toEqual([]);
  });

  it("with empty maps the Items page shows the English names in Chinese mode and saves as before", async () => {
    native.gameNames.mockImplementation(async () => ({ language: "zh-CN", heroes: {}, items: {}, missing: [] }));
    await inChinese();
    render(<ItemsPage />);
    fireEvent.click(await library().findByRole("button", { name: "Close Quarters" }));
    expect(await screen.findByRole("textbox", { name: "物品名称" })).toHaveValue("Close Quarters");
    fireEvent.change(searchBox(), { target: { value: "近战" } });
    expect(library().queryByRole("button", { name: "Close Quarters" })).toBeNull();
  });

  it("follows a live language switch: Chinese names at once, English again in English", async () => {
    render(<ItemsPage />);
    expect(await library().findByRole("button", { name: "Close Quarters" })).toBeInTheDocument();
    expect(native.gameNames).not.toHaveBeenCalled();
    await inChinese();
    expect(await library().findByRole("button", { name: "近战专家" })).toBeInTheDocument();
    await act(() => setLanguage("en"));
    expect(library().getByRole("button", { name: "Close Quarters" })).toBeInTheDocument();
    expect(library().queryByRole("button", { name: "近战专家" })).toBeNull();
  });
});

describe("Game hero names in the pages' own sentences", () => {
  it("the Extras page shows the hero's Chinese name in its chip and its sentences, and keeps the English in the project", async () => {
    await inChinese();
    const update = vi.fn();
    render(<ExtrasWorkspace project={structuredClone(base)} update={update} disabled={false} catalog={catalog} fail={vi.fn()} reviewBuild={vi.fn()} />);
    expect(document.querySelector(".extra-hero-chip")).toHaveTextContent("海兹");
    await waitFor(() => expect(screen.getAllByText(/海兹/).length).toBeGreaterThan(1));
    for (const call of update.mock.calls) expect((call[0] as Project).hero_name).toBe("Haze");
  });

  it("the separate gun panel names the hero as the game does, while its operation keeps the English label", async () => {
    native.discoverWeaponModels.mockResolvedValue([]);
    await inChinese();
    const operate = vi.fn(async (_label: string, op: () => Promise<unknown>) => { await op(); });
    const props = { project: structuredClone(base), analysis: {}, options: {}, busy: false, operate, change: vi.fn(), projectPath: null, saveProject: vi.fn(), updateProject: vi.fn() } as unknown as SeparateGunProps;
    render(<SeparateGun {...props} />);
    await waitFor(() => expect(operate).toHaveBeenCalled());
    expect(operate.mock.calls[0][0]).toBe("Could not find Haze's gun model");
    await waitFor(() => expect(native.discoverWeaponModels).toHaveBeenCalled());
    expect((await screen.findAllByText(/海兹/)).length).toBeGreaterThan(0);
    expect(document.body.textContent).not.toMatch(/Haze/);
  });

  it("the copy notice names both heroes as the game does, and in English as they come", async () => {
    const notice = { to: "Haze", from_hero: "Vindicta", carried: ["Body roles"], not_carried: ["12 sounds"], remade: ["Joint matches"] };
    await inChinese();
    const view = render(<HeroCopyNotice notice={notice} dismiss={vi.fn()} />);
    expect(screen.getByRole("heading", { name: "海兹的副本" })).toBeInTheDocument();
    expect(view.container).toHaveTextContent("维迪塔");
    expect(view.container).not.toHaveTextContent("Vindicta");
    await act(() => setLanguage("en"));
    expect(screen.getByRole("heading", { name: "Copy for Haze" })).toBeInTheDocument();
    expect(view.container).toHaveTextContent("Vindicta");
  });
});
