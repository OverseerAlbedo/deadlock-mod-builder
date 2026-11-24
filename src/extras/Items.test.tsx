import { useState } from "react";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Project } from "../models";
import type { ComponentCatalog } from "../components/AssetCatalog";
import { ExtrasWorkspace } from "./ExtrasPage";
import { clearExtraMediaCache } from "./media";
import { forgetItemCatalog, ITEM_NOTE, NO_ITEM_NAME_NOTE } from "./Items";
import { itemCatalogFixture } from "./itemFixtures";
import type { ItemCatalog } from "./itemCatalog";
import { NAME_REFUSED_CHAR, NAME_TOO_LONG, NAME_EMPTY, type ItemNameRow, type ItemNamesState } from "./itemNames";
import { ITEM_NAME_NOTES } from "./ItemName";
import { MISSING_NAMES_TITLE } from "./Items";
import { categoryCounts, itemAssets, itemGroups, placeText, tierName } from "./itemCatalog";
import reference from "../../src-tauri/crates/deadlock-project/tests/fixtures/reference.json";

const native = vi.hoisted(() => ({ validateAssetPath: vi.fn(), previewExtra: vi.fn(), itemCatalog: vi.fn(), itemNamesState: vi.fn() }));
vi.mock("../api", async (original) => ({
  ...(await original<typeof import("../api")>()),
  api: native,
}));
const initial = {
  ...reference.cases[1].expected,
  hero_name: "Vindicta",
  customization: { enabled: true },
} as unknown as Project;
const heroCatalog: ComponentCatalog = {
  hero_path: initial.original_model,
  icons: [{ path: "panorama/images/hero.vtex_c", target: "panorama/images/hero.vtex_c", label: "Hero portrait", supported: true, width: 128, height: 128 }],
  audio: [],
  effects: [],
  animations: [],
  variants: [],
  animation_unavailable: null,
  warnings: [],
};
function packet() {
  const h = new TextEncoder().encode(JSON.stringify({ mime_type: "image/png", details: {} }));
  const buffer = new ArrayBuffer(4 + h.length + 8);
  new DataView(buffer).setUint32(0, h.length, true);
  new Uint8Array(buffer, 4, h.length).set(h);
  return buffer;
}
/** The fixture with each item record named as the game names its items (`upgrade_…`). */
function namedCatalog(): ItemCatalog {
  const catalog = itemCatalogFixture();
  for (const item of catalog.items) {
    if (item.category === "misc") item.items = [];
    else if (!item.items[0].startsWith("upgrade_clip")) item.items = [`upgrade_${item.label.toLowerCase().replace(/[^a-z0-9]+/g, "_")}`, ...item.items.slice(1)];
  }
  return catalog;
}
/** What the backend answers for these rows: the game's English names of the named catalog. */
function namesState(rows: ItemNameRow[], found = true): ItemNamesState {
  const names: Record<string, string> = {};
  if (found) for (const item of namedCatalog().items) if (item.items[0]) names[item.items[0].toLowerCase()] = item.label;
  const states = rows.map((r) => ({ item: r.item, known: r.item.toLowerCase() in names, ...(names[r.item.toLowerCase()] ? { game_name: names[r.item.toLowerCase()] } : {}) }));
  return { found, names, languages: found ? ["english", "german"] : [], rows: states, unknown: states.filter((r) => !r.known).map((r) => r.item) };
}
beforeEach(() => {
  vi.clearAllMocks();
  clearExtraMediaCache();
  forgetItemCatalog();
  native.validateAssetPath.mockImplementation(async (path) => path);
  native.previewExtra.mockImplementation(async () => packet());
  native.itemCatalog.mockImplementation(async () => namedCatalog());
  native.itemNamesState.mockImplementation(async (rows: ItemNameRow[]) => namesState(rows));
  writes.length = 0;
  vi.stubGlobal("URL", Object.assign(URL, { createObjectURL: vi.fn(() => "blob:test"), revokeObjectURL: vi.fn() }));
});
/** Every project write of the page (one write = one step of the app's history). */
const writes: Project[] = [];
function Harness({ start = initial, canDiscover = true }: { start?: Project; canDiscover?: boolean }) {
  const [p, set] = useState(structuredClone(start));
  const [before, setBefore] = useState<Project[]>([]);
  const update = (next: Project) => {
    writes.push(next);
    setBefore((list) => [...list, p]);
    set(next);
  };
  // An outside change of the project, as the app's Undo makes it.
  const undo = () => {
    if (!before.length) return;
    set(before[before.length - 1]);
    setBefore((list) => list.slice(0, -1));
  };
  return (
    <>
      <ExtrasWorkspace project={p} update={update} disabled={false} catalog={heroCatalog} canDiscover={canDiscover} discover={vi.fn()} fail={vi.fn()} openSetup={vi.fn()} />
      <button type="button" data-testid="undo" onClick={undo} />
      <output data-testid="saved">{JSON.stringify(p)}</output>
    </>
  );
}
const saved = () => JSON.parse(screen.getByTestId("saved").textContent!) as Project;
const previewed = () => native.previewExtra.mock.calls.map(([r]) => r as { kind: string; target: string }).filter((r) => r.kind === "items");
async function openItems(props: { start?: Project; canDiscover?: boolean } = {}) {
  render(<Harness {...props} />);
  fireEvent.click(screen.getByRole("tab", { name: /^Items/ }));
  const library = within(screen.getByRole("region", { name: "Item icons" }));
  if (props.canDiscover !== false) await library.findByRole("region", { name: "Tier I" });
  return library;
}
async function drop(path: string) {
  const node = document.querySelector(".items-inspector [data-native-drop]");
  expect(node).toBeTruthy();
  await act(async () => node!.dispatchEvent(new CustomEvent("dmb-file-drop", { detail: [path] })));
}

describe("Extras → Items", () => {
  it("reads nothing about items until the Items tab is opened", async () => {
    render(<Harness />);
    expect(screen.getByRole("tab", { name: /^Items/ })).toHaveTextContent("—");
    fireEvent.click(screen.getByRole("tab", { name: /Sounds/ }));
    fireEvent.click(screen.getByRole("tab", { name: /Icons/ }));
    await act(async () => {});
    expect(native.itemCatalog).not.toHaveBeenCalled();
    expect(previewed().some((r) => r.kind === "items")).toBe(false);
    fireEvent.click(screen.getByRole("tab", { name: /^Items/ }));
    await screen.findByRole("button", { name: "Close Quarters" });
    expect(native.itemCatalog).toHaveBeenCalledTimes(1);
    expect(native.itemCatalog).toHaveBeenCalledWith(false);
    // The tab shows the count once read. Coming back shows the list at once and
    // asks the app again (kept per game archive there), so a game update shows.
    expect(screen.getByRole("tab", { name: /^Items/ })).toHaveTextContent("9");
    fireEvent.click(screen.getByRole("tab", { name: /Icons/ }));
    native.itemCatalog.mockImplementation(async () => ({ ...itemCatalogFixture(), items: itemCatalogFixture().items.filter((i) => i.label !== "Close Quarters") }));
    fireEvent.click(screen.getByRole("tab", { name: /^Items/ }));
    expect(screen.getByRole("button", { name: "Close Quarters" })).toBeInTheDocument();
    await waitFor(() => expect(screen.queryByRole("button", { name: "Close Quarters" })).toBeNull());
    expect(native.itemCatalog).toHaveBeenCalledTimes(2);
    expect(native.itemCatalog).toHaveBeenLastCalledWith(false);
  });

  it("shows the open category's tiers with their prices and loads only its pictures", async () => {
    const library = await openItems();
    expect(library.getByText(/^Replace the icons of the shop’s items with your own pictures, or rename them · /)).toHaveTextContent(
      "Replace the icons of the shop’s items with your own pictures, or rename them · 9 items from your game · icons load when you open this page",
    );
    const tiers = library.getAllByRole("region").map((r) => r.getAttribute("aria-label"));
    expect(tiers).toEqual(["Tier I", "Tier II", "Tier IV"]);
    const first = within(library.getByRole("region", { name: "Tier I" }));
    expect(first.getByText("800")).toBeInTheDocument();
    expect(first.getAllByRole("button").map((b) => b.textContent)).toEqual(["Close Quarters", "Extended Magazine"]);
    expect(library.getByRole("region", { name: "Tier II" })).toHaveTextContent("1,600");
    await waitFor(() => expect(previewed().length).toBe(4));
    expect(previewed().every((r) => r.kind === "items" && r.target.includes("/items/weapon/"))).toBe(true);
    // Category tabs carry their counts and are a tablist.
    const tabs = within(library.getByRole("tablist", { name: "Item categories" })).getAllByRole("tab");
    expect(tabs.map((t) => t.textContent)).toEqual(["Weapon4", "Vitality2", "Spirit2", "Legendary1· 1 off", "Misc1"]);
    fireEvent.keyDown(tabs[0], { key: "ArrowRight" });
    expect(library.getByRole("tab", { name: /Vitality/ })).toHaveAttribute("aria-selected", "true");
    expect(document.activeElement).toBe(library.getByRole("tab", { name: /Vitality/ }));
    await waitFor(() => expect(previewed().some((r) => r.target.includes("/items/vitality/"))).toBe(true));
    expect(previewed().some((r) => r.target.includes("/items/spirit/"))).toBe(false);
  });

  it("puts Street Brawl items in Legendary without a price, switched-off ones greyed out after them", async () => {
    const library = await openItems();
    fireEvent.click(library.getByRole("tab", { name: /Legendary/ }));
    expect(library.getAllByRole("region").map((r) => r.getAttribute("aria-label"))).toEqual(["Tier V", "Switched off in the game"]);
    const legendary = library.getByRole("region", { name: "Tier V" });
    expect(legendary).toHaveTextContent("Street Brawl");
    expect(legendary).not.toHaveTextContent("souls");
    // Greyed out but selectable and replaceable, with the reason in its tooltip and the editor.
    const off = library.getByRole("button", { name: "Apex Combat, switched off in the game" });
    expect(off).toHaveClass("off");
    expect(off.getAttribute("title")).toMatch(/does not show it now/);
    fireEvent.click(off);
    const editor = within(screen.getByRole("complementary", { name: "Replacement editor" }));
    expect(editor.getByText("Switched off in the game")).toBeInTheDocument();
    expect(editor.getByText("Not in the game now.")).toBeInTheDocument();
    await drop("C:/art/apex.png");
    expect(saved().customization.item_icons).toEqual([expect.objectContaining({ target: "panorama/images/items/brawl/apex_combat_psd.vtex_c" })]);
    expect(library.getByRole("button", { name: "Apex Combat, switched off in the game, replaced" })).toBeInTheDocument();
    // Misc holds only what is neither a shop item nor Legendary.
    fireEvent.click(library.getByRole("tab", { name: /Misc/ }));
    expect(library.getAllByRole("region").map((r) => r.getAttribute("aria-label"))).toEqual(["Seasonal pictures"]);
  });

  it("hides Misc when the game has nothing for it, and shows it for a saved picture the game dropped", async () => {
    native.itemCatalog.mockImplementation(async () => ({ ...itemCatalogFixture(), items: itemCatalogFixture().items.filter((i) => i.category !== "misc") }));
    const library = await openItems();
    expect(library.queryByRole("tab", { name: /Misc/ })).toBeNull();
    document.body.innerHTML = "";
    forgetItemCatalog();
    const start = structuredClone(initial);
    // A row on an older item icon, no longer offered.
    start.customization.item_icons = [{ enabled: true, source: "a.png", target: "panorama/images/upgrades/mods_armor/bullet_shield_psd.vtex_c", fit: "contain" }];
    const again = await openItems({ start });
    fireEvent.click(again.getByRole("tab", { name: /Misc/ }));
    expect(again.getByRole("region", { name: "Saved, not in the game now" })).toBeInTheDocument();
  });

  it("searches every category and groups the results by category and tier", async () => {
    const library = await openItems();
    fireEvent.change(library.getByRole("textbox", { name: "Search items" }), { target: { value: "extra" } });
    const groups = library.getAllByRole("region").map((r) => r.getAttribute("aria-label"));
    expect(groups).toEqual(["Vitality · Tier I", "Spirit · Tier I"]);
    expect(library.getByText(/2 items match “extra” in every category/)).toBeInTheDocument();
    expect(library.getByRole("tab", { name: /Weapon/ })).toHaveAttribute("aria-selected", "false");
    // Item records named in the data are found too.
    fireEvent.change(library.getByRole("textbox", { name: "Search items" }), { target: { value: "projectile test" } });
    expect(library.getAllByRole("button", { name: /Extended Magazine/ })).toHaveLength(1);
    fireEvent.change(library.getByRole("textbox", { name: "Search items" }), { target: { value: "zzz" } });
    expect(library.getByText("No matching items")).toBeInTheDocument();
    fireEvent.click(library.getByRole("button", { name: "Show all items" }));
    expect(library.getByRole("region", { name: "Tier I" })).toBeInTheDocument();
  });

  it("replaces an item icon as its own list, with the icon editor's controls and the note", async () => {
    const library = await openItems();
    fireEvent.click(library.getByRole("button", { name: "High-Velocity Rounds" }));
    const editor = within(screen.getByRole("complementary", { name: "Replacement editor" }));
    expect(editor.getByRole("heading", { name: "High-Velocity Rounds" })).toBeInTheDocument();
    expect(editor.getByText("SELECTED ITEM")).toBeInTheDocument();
    expect(editor.getByText("Tier II · 1,600 souls")).toBeInTheDocument();
    expect(editor.getByText(ITEM_NOTE)).toBeInTheDocument();
    expect(editor.queryByText(/Include this shared replacement/)).toBeNull();
    await drop("C:/art/rounds.png");
    const rows = saved().customization.item_icons!;
    expect(rows).toEqual([
      expect.objectContaining({ enabled: true, source: "C:/art/rounds.png", target: "panorama/images/items/weapon/high_velocity_rounds_psd.vtex_c", fit: "contain", allow_shared: false }),
    ]);
    expect(saved().customization.icons).toBeUndefined();
    expect(saved().customization.enabled).toBe(true);
    // The replaced dot, the editor's fit choice and its preview of the user's image.
    expect(library.getByRole("button", { name: "High-Velocity Rounds, replaced" })).toBeInTheDocument();
    fireEvent.click(editor.getByRole("button", { name: /Fill & crop/ }));
    expect(saved().customization.item_icons![0].fit).toBe("cover");
    await waitFor(() =>
      expect(previewed().some((r) => r.kind === "items" && (r as { source?: string }).source === "C:/art/rounds.png")).toBe(true),
    );
    expect(editor.getByText("Item icon · the same for every hero")).toBeInTheDocument();
    fireEvent.click(editor.getByRole("checkbox", { name: "Include in build" }));
    expect(saved().customization.item_icons![0].enabled).toBe(false);
    // The Extras header and the tab count it; Replaced only shows it.
    expect(screen.getByText("1 change")).toBeInTheDocument();
    fireEvent.click(library.getByRole("button", { name: /Changed only/ }));
    expect(library.getAllByRole("listitem")).toHaveLength(1);
    fireEvent.click(library.getByRole("tab", { name: /Spirit/ }));
    expect(library.getByText("No changed Spirit items")).toBeInTheDocument();
    // The Icons tab never sees item rows.
    fireEvent.click(screen.getByRole("tab", { name: /Icons/ }));
    expect(screen.queryByText("Saved destination unavailable")).toBeNull();
    fireEvent.click(screen.getByRole("tab", { name: /^Items/ }));
    fireEvent.click(await screen.findByRole("button", { name: "High-Velocity Rounds, paused" }));
    fireEvent.click(screen.getByRole("button", { name: /Remove replacement/ }));
    expect(saved().customization.item_icons).toEqual([]);
  });

  it("removes every item icon only after asking", async () => {
    const start = structuredClone(initial);
    start.customization.item_icons = [
      { enabled: true, source: "a.png", target: "panorama/images/items/weapon/glass_cannon_psd.vtex_c", fit: "contain" },
      { enabled: true, source: "b.png", target: "panorama/images/items/gone_psd.vtex_c", fit: "contain" },
    ];
    const library = await openItems({ start });
    expect(library.getByText("2 new icons")).toBeInTheDocument();
    // A saved picture the game no longer lists stays visible in Misc.
    fireEvent.click(library.getByRole("tab", { name: /Misc/ }));
    expect(library.getByRole("region", { name: "Saved, not in the game now" })).toBeInTheDocument();
    fireEvent.click(library.getByRole("button", { name: "Remove all…" }));
    fireEvent.click(library.getByRole("button", { name: "Keep" }));
    expect(saved().customization.item_icons).toHaveLength(2);
    fireEvent.click(library.getByRole("button", { name: "Remove all…" }));
    fireEvent.click(library.getByRole("button", { name: "Remove" }));
    expect(saved().customization.item_icons).toEqual([]);
  });

  it("asks for the game when none is set and offers a retry when reading fails", async () => {
    let library = await openItems({ canDiscover: false });
    expect(library.getByText("Connect your game to get started")).toBeInTheDocument();
    expect(native.itemCatalog).not.toHaveBeenCalled();
    document.body.innerHTML = "";
    native.itemCatalog.mockRejectedValueOnce({ message: "The game's item list could not be read" });
    render(<Harness />);
    fireEvent.click(screen.getByRole("tab", { name: /^Items/ }));
    library = within(screen.getByRole("region", { name: "Item icons" }));
    expect(await library.findByText("The game’s items could not be read")).toBeInTheDocument();
    fireEvent.click(library.getByRole("button", { name: "Try again" }));
    await library.findByRole("button", { name: "Close Quarters" });
    expect(native.itemCatalog).toHaveBeenLastCalledWith(true);
  });
});

const named = (item: string, name: string, enabled = true): ItemNameRow => ({ item, enabled, name });
function withNames(rows: ItemNameRow[], icons: Project["customization"]["item_icons"] = undefined): Project {
  const start = structuredClone(initial);
  start.customization = { ...start.customization, item_names: rows, ...(icons ? { item_icons: icons } : {}) };
  return start;
}
const editor = () => within(screen.getByRole("complementary", { name: "Replacement editor" }));
async function nameField() {
  // The field shows once the game's names were read.
  return editor().findByRole("textbox", { name: "Item name" });
}

describe("Extras → Items: renamed items", () => {
  it("shows the name field on selection and writes nothing on open, select, typing, Cancel and Escape", async () => {
    render(<Harness />);
    await act(async () => {});
    expect(native.itemNamesState).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("tab", { name: /^Items/ }));
    const library = within(screen.getByRole("region", { name: "Item icons" }));
    fireEvent.click(await library.findByRole("button", { name: "Close Quarters" }));
    expect(native.itemNamesState).toHaveBeenCalledTimes(1);
    expect(native.itemNamesState).toHaveBeenCalledWith([]);
    const field = await nameField();
    // No Rename button any more: the field is the heading.
    expect(editor().queryByRole("button", { name: "Rename" })).toBeNull();
    expect(editor().getByText("SELECTED ITEM")).toBeInTheDocument();
    expect(editor().queryByText(/RENAMING/)).toBeNull();
    // The chips sit on the eyebrow's row.
    expect(field.closest("header")!.querySelector(".items-eyebrow-row .items-pills")).toHaveTextContent("Original");
    expect(field).toHaveValue("Close Quarters");
    expect(editor().getByText("14 / 32")).toBeInTheDocument();
    expect(editor().getByText("Close Quarters in the game")).toBeInTheDocument();
    for (const note of ITEM_NAME_NOTES) expect(editor().getByText(note)).toBeInTheDocument();
    // Unchanged: Save and Cancel wait; nothing to undo to, so no "Use the game's name".
    expect(editor().getByRole("button", { name: "Save" })).toBeDisabled();
    expect(editor().getByRole("button", { name: "Cancel" })).toBeDisabled();
    expect(editor().queryByRole("button", { name: /Use the game's name/ })).toBeNull();
    fireEvent.change(field, { target: { value: "Dome Shot" } });
    expect(editor().getByText("9 / 32")).toBeInTheDocument();
    expect(editor().getByRole("button", { name: "Save" })).toBeEnabled();
    expect(editor().getByRole("button", { name: "Cancel" })).toBeEnabled();
    fireEvent.click(editor().getByRole("button", { name: "Cancel" }));
    expect(editor().getByRole("textbox", { name: "Item name" })).toHaveValue("Close Quarters");
    expect(editor().getByRole("button", { name: "Cancel" })).toBeDisabled();
    // Escape puts the saved name back as well.
    fireEvent.change(editor().getByRole("textbox", { name: "Item name" }), { target: { value: "Dome Shot" } });
    fireEvent.keyDown(editor().getByRole("textbox", { name: "Item name" }), { key: "Escape" });
    expect(editor().getByRole("textbox", { name: "Item name" })).toHaveValue("Close Quarters");
    expect(editor().getByRole("button", { name: "Save" })).toBeDisabled();
    expect(writes).toHaveLength(0);
    expect(saved().customization.item_names).toBeUndefined();
  });

  it("leaves the keyboard focus on the list when an item is selected", async () => {
    const library = await openItems();
    const tile = library.getByRole("button", { name: "Close Quarters" });
    tile.focus();
    fireEvent.click(tile);
    const field = await nameField();
    expect(document.activeElement).toBe(tile);
    expect(field).not.toHaveFocus();
  });

  it("drops an unsaved draft on another selection and on an undo", async () => {
    const library = await openItems();
    fireEvent.click(library.getByRole("button", { name: "Close Quarters" }));
    fireEvent.change(await nameField(), { target: { value: "Dome Shot" } });
    fireEvent.click(library.getByRole("button", { name: "Extended Magazine" }));
    expect(await nameField()).toHaveValue("Extended Magazine");
    fireEvent.click(library.getByRole("button", { name: "Close Quarters" }));
    expect(await nameField()).toHaveValue("Close Quarters");
    // Saved, then a draft, then an undo: the field shows the name saved before.
    fireEvent.change(editor().getByRole("textbox", { name: "Item name" }), { target: { value: "Dome Shot" } });
    fireEvent.click(editor().getByRole("button", { name: "Save" }));
    expect(writes).toHaveLength(1);
    fireEvent.change(editor().getByRole("textbox", { name: "Item name" }), { target: { value: "Dome Shot II" } });
    fireEvent.click(screen.getByTestId("undo"));
    expect(saved().customization.item_names).toBeUndefined();
    expect(editor().getByRole("textbox", { name: "Item name" })).toHaveValue("Close Quarters");
    expect(editor().getByRole("button", { name: "Save" })).toBeDisabled();
    expect(editor().getByRole("button", { name: "Cancel" })).toBeDisabled();
    expect(writes).toHaveLength(1);
  });

  it("saves one row in one step with Save or Enter, and shows the new name everywhere", async () => {
    const library = await openItems();
    fireEvent.click(library.getByRole("button", { name: "Close Quarters" }));
    fireEvent.change(await nameField(), { target: { value: "  Dome Shot " } });
    fireEvent.click(editor().getByRole("button", { name: "Save" }));
    expect(writes).toHaveLength(1);
    expect(saved().customization.item_names).toEqual([{ item: "upgrade_close_quarters", enabled: true, name: "Dome Shot" }]);
    expect(saved().customization.enabled).toBe(true);
    // After saving: the new name in the field, the game's name under it, the chip.
    expect(editor().getByRole("textbox", { name: "Item name" })).toHaveValue("Dome Shot");
    expect(editor().getByRole("heading", { name: "Dome Shot" })).toBeInTheDocument();
    expect(editor().getByText("Close Quarters in the game")).toBeInTheDocument();
    expect(editor().getByText("Renamed")).toBeInTheDocument();
    expect(editor().getByRole("button", { name: "Save" })).toBeDisabled();
    expect(editor().getByRole("button", { name: "Use the game's name" })).toBeInTheDocument();
    // The tile: new name with the pencil mark, the game's name under it; the order stays.
    const tile = library.getByRole("button", { name: "Dome Shot, renamed from Close Quarters" });
    expect(tile.querySelector(".items-name-mark")).toBeTruthy();
    expect(tile.querySelector(".items-was")).toHaveTextContent("Close Quarters");
    expect(within(library.getByRole("region", { name: "Tier I" })).getAllByRole("listitem")[0]).toContainElement(tile);
    // Enter saves another name, again one step.
    const field = editor().getByRole("textbox", { name: "Item name" });
    fireEvent.change(field, { target: { value: "Dome Shot II" } });
    fireEvent.keyDown(field, { key: "Enter" });
    expect(writes).toHaveLength(2);
    expect(saved().customization.item_names).toEqual([{ item: "upgrade_close_quarters", enabled: true, name: "Dome Shot II" }]);
    expect(editor().getByRole("textbox", { name: "Item name" })).toHaveValue("Dome Shot II");
    // Enter on an unchanged name writes nothing.
    fireEvent.keyDown(editor().getByRole("textbox", { name: "Item name" }), { key: "Enter" });
    expect(writes).toHaveLength(2);
  });

  it("shows the helper's own sentence for a refused name and keeps Save disabled", async () => {
    const library = await openItems();
    fireEvent.click(library.getByRole("button", { name: "Close Quarters" }));
    const field = await nameField();
    for (const [value, sentence] of [
      ['Dome "Shot"', NAME_REFUSED_CHAR],
      ["   ", NAME_EMPTY],
      ["A".repeat(33), NAME_TOO_LONG],
    ]) {
      fireEvent.change(field, { target: { value } });
      expect(editor().getByRole("alert")).toHaveTextContent(sentence);
      expect(editor().getByRole("button", { name: "Save" })).toBeDisabled();
      fireEvent.keyDown(field, { key: "Enter" });
    }
    expect(editor().getByText("33 / 32")).toBeInTheDocument();
    expect(writes).toHaveLength(0);
  });

  it("removes the rename when the game's name is saved, or with Use the game's name", async () => {
    const library = await openItems({ start: withNames([named("upgrade_close_quarters", "Dome Shot"), named("upgrade_clip_size", "Big Clip")]) });
    fireEvent.click(library.getByRole("button", { name: "Dome Shot, renamed from Close Quarters" }));
    expect(await nameField()).toHaveValue("Dome Shot");
    fireEvent.change(editor().getByRole("textbox", { name: "Item name" }), { target: { value: "Close Quarters" } });
    fireEvent.click(editor().getByRole("button", { name: "Save" }));
    expect(writes).toHaveLength(1);
    expect(saved().customization.item_names).toEqual([named("upgrade_clip_size", "Big Clip")]);
    expect(editor().getByRole("textbox", { name: "Item name" })).toHaveValue("Close Quarters");
    expect(editor().queryByRole("button", { name: /Use the game's name/ })).toBeNull();
    fireEvent.click(library.getByRole("button", { name: "Big Clip, renamed from Extended Magazine" }));
    expect(await nameField()).toHaveValue("Big Clip");
    fireEvent.click(editor().getByRole("button", { name: "Use the game's name" }));
    expect(writes).toHaveLength(2);
    // The key goes with the last row.
    expect(saved().customization).not.toHaveProperty("item_names");
    expect(editor().getByRole("heading", { name: "Extended Magazine" })).toBeInTheDocument();
    expect(editor().getByRole("textbox", { name: "Item name" })).toHaveValue("Extended Magazine");
  });

  it("searches by the new or the game's name and counts names in the chips and Changed only", async () => {
    const icons = [{ enabled: true, source: "a.png", target: "panorama/images/items/weapon/close_quarters_psd.vtex_c", fit: "contain" as const }];
    const library = await openItems({ start: withNames([named("upgrade_close_quarters", "Dome Shot"), named("upgrade_high_velocity_rounds", "Fast Rounds")], icons) });
    expect(library.getByText("1 new icon")).toBeInTheDocument();
    expect(library.getByText("2 renamed")).toBeInTheDocument();
    const filter = library.getByRole("button", { name: /Changed only/ });
    expect(filter).toHaveTextContent("Changed only2");
    expect(library.getByRole("textbox", { name: "Search items" })).toHaveAttribute("placeholder", "Search items by new or game name…");
    fireEvent.change(library.getByRole("textbox", { name: "Search items" }), { target: { value: "fast" } });
    expect(library.getAllByRole("listitem")).toHaveLength(1);
    expect(library.getByRole("button", { name: "Fast Rounds, renamed from High-Velocity Rounds" })).toBeInTheDocument();
    fireEvent.change(library.getByRole("textbox", { name: "Search items" }), { target: { value: "velocity" } });
    expect(library.getByRole("button", { name: "Fast Rounds, renamed from High-Velocity Rounds" })).toBeInTheDocument();
    fireEvent.change(library.getByRole("textbox", { name: "Search items" }), { target: { value: "" } });
    fireEvent.click(filter);
    expect(library.getAllByRole("listitem").map((l) => l.textContent)).toEqual(["Dome ShotClose Quarters", "Fast RoundsHigh-Velocity Rounds"]);
    // The panel's chips for a renamed item with a new icon.
    fireEvent.click(library.getByRole("button", { name: /^Dome Shot, renamed from Close Quarters/ }));
    expect(editor().getByText("Renamed")).toBeInTheDocument();
    expect(editor().getByText("New icon")).toBeInTheDocument();
  });

  it("removes every new icon and name in one step after saying what goes", async () => {
    const icons = [{ enabled: true, source: "a.png", target: "panorama/images/items/weapon/glass_cannon_psd.vtex_c", fit: "contain" as const }];
    const library = await openItems({ start: withNames([named("upgrade_close_quarters", "Dome Shot"), named("upgrade_clip_size", "Big Clip")], icons) });
    fireEvent.click(library.getByRole("button", { name: "Remove all…" }));
    expect(library.getByRole("group", { name: "Remove every item icon and name" })).toHaveTextContent("Remove 1 new icon and 2 names?");
    fireEvent.click(library.getByRole("button", { name: "Remove" }));
    expect(writes).toHaveLength(1);
    expect(saved().customization.item_icons).toEqual([]);
    expect(saved().customization).not.toHaveProperty("item_names");
    expect(library.queryByRole("button", { name: "Remove all…" })).toBeNull();
  });

  it("lists a saved name for an item the game no longer has, quietly and removable", async () => {
    const library = await openItems({ start: withNames([named("upgrade_gone_item", "Old Name"), named("upgrade_close_quarters", "Dome Shot")]) });
    const list = within(await screen.findByRole("region", { name: MISSING_NAMES_TITLE }));
    expect(list.getByText("Old Name")).toBeInTheDocument();
    expect(list.getByText("upgrade_gone_item")).toBeInTheDocument();
    expect(library.getByRole("button", { name: "Dome Shot, renamed from Close Quarters" })).toBeInTheDocument();
    fireEvent.click(list.getByRole("button", { name: "Remove the name Old Name" }));
    expect(writes).toHaveLength(1);
    expect(saved().customization.item_names).toEqual([named("upgrade_close_quarters", "Dome Shot")]);
    expect(screen.queryByRole("region", { name: MISSING_NAMES_TITLE })).toBeNull();
  });

  it("shows the plain name and no field, and lists nothing as missing, when the game's names cannot be read", async () => {
    native.itemNamesState.mockImplementation(async (rows: ItemNameRow[]) => namesState(rows, false));
    let library = await openItems({ start: withNames([named("upgrade_close_quarters", "Dome Shot")]) });
    fireEvent.click(library.getByRole("button", { name: "Dome Shot, renamed from Close Quarters" }));
    await act(async () => {});
    expect(editor().queryByRole("textbox", { name: "Item name" })).toBeNull();
    expect(editor().queryByRole("button", { name: "Save" })).toBeNull();
    expect(editor().getByRole("heading", { name: "Dome Shot" })).toBeVisible();
    expect(screen.queryByRole("region", { name: MISSING_NAMES_TITLE })).toBeNull();
    document.body.innerHTML = "";
    forgetItemCatalog();
    native.itemNamesState.mockRejectedValue({ message: "No game archive is configured." });
    library = await openItems();
    fireEvent.click(library.getByRole("button", { name: "Close Quarters" }));
    await act(async () => {});
    expect(editor().queryByRole("textbox", { name: "Item name" })).toBeNull();
    expect(editor().getByRole("heading", { name: "Close Quarters" })).toBeInTheDocument();
    expect(library.getByRole("button", { name: "Extended Magazine" })).toBeInTheDocument();
  });

  it("shows no field for a Misc picture", async () => {
    const library = await openItems();
    fireEvent.click(library.getByRole("tab", { name: /Misc/ }));
    const tiles = library.getAllByRole("listitem");
    fireEvent.click(within(tiles[0]).getByRole("button"));
    await act(async () => {});
    expect(editor().getByText("SELECTED ITEM")).toBeInTheDocument();
    expect(editor().queryByRole("textbox", { name: "Item name" })).toBeNull();
    // It says why: the game's names file names no item for it.
    expect(editor().getByText(NO_ITEM_NAME_NOTE)).toBeInTheDocument();
    // An item that can be renamed has no such line.
    fireEvent.click(library.getByRole("tab", { name: /Weapon/ }));
    fireEvent.click(await library.findByRole("button", { name: "Close Quarters" }));
    await nameField();
    expect(editor().queryByText(NO_ITEM_NAME_NOTE)).toBeNull();
  });

  it("says nothing about a Misc picture's name while the names are read, or when they cannot be read", async () => {
    let answer: (value: ItemNamesState) => void = () => undefined;
    native.itemNamesState.mockImplementation(() => new Promise<ItemNamesState>((resolve) => (answer = resolve)));
    let library = await openItems();
    fireEvent.click(library.getByRole("tab", { name: /Misc/ }));
    fireEvent.click(within(library.getAllByRole("listitem")[0]).getByRole("button"));
    await act(async () => {});
    expect(editor().getByText("SELECTED ITEM")).toBeInTheDocument();
    expect(editor().queryByText(NO_ITEM_NAME_NOTE)).toBeNull();
    await act(async () => answer(namesState([])));
    expect(editor().getByText(NO_ITEM_NAME_NOTE)).toBeInTheDocument();
    // The game's names file was not found: nothing can be renamed, for another reason.
    document.body.innerHTML = "";
    forgetItemCatalog();
    native.itemNamesState.mockImplementation(async (rows: ItemNameRow[]) => namesState(rows, false));
    library = await openItems();
    fireEvent.click(library.getByRole("tab", { name: /Misc/ }));
    fireEvent.click(within(library.getAllByRole("listitem")[0]).getByRole("button"));
    await act(async () => {});
    expect(editor().getByText("SELECTED ITEM")).toBeInTheDocument();
    expect(editor().queryByText(NO_ITEM_NAME_NOTE)).toBeNull();
    // Or could not be read at all.
    document.body.innerHTML = "";
    forgetItemCatalog();
    native.itemNamesState.mockRejectedValue({ message: "No game archive is configured." });
    library = await openItems();
    fireEvent.click(library.getByRole("tab", { name: /Misc/ }));
    fireEvent.click(within(library.getAllByRole("listitem")[0]).getByRole("button"));
    await act(async () => {});
    expect(editor().queryByText(NO_ITEM_NAME_NOTE)).toBeNull();
  });

  it("counts names in the Extras header, the tab dot and Pause all", async () => {
    const library = await openItems({ start: withNames([named("upgrade_close_quarters", "Dome Shot"), named("upgrade_clip_size", "Big Clip")]) });
    expect(screen.getByText("2 changes")).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /^Items/ }).querySelector("i[title='Contains replacements']")).toBeTruthy();
    expect(screen.getByText(/2 active changes/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Pause all extras" }));
    expect(writes).toHaveLength(1);
    expect(saved().customization.item_names).toEqual([named("upgrade_close_quarters", "Dome Shot", false), named("upgrade_clip_size", "Big Clip", false)]);
    fireEvent.click(screen.getByRole("button", { name: "Resume all extras" }));
    expect(saved().customization.item_names).toEqual([named("upgrade_close_quarters", "Dome Shot"), named("upgrade_clip_size", "Big Clip")]);
    expect(library.getByText("2 renamed")).toBeInTheDocument();
  });
});

describe("item catalog grouping", () => {
  it("names tiers, places and counts as the page shows them", () => {
    expect([1, 2, 3, 4, 5].map(tierName)).toEqual(["Tier I", "Tier II", "Tier III", "Tier IV", "Tier V"]);
    const catalog = itemCatalogFixture();
    const items = itemAssets(catalog, [{ target: catalog.items[1].target, source: "x.png" }]);
    expect(itemGroups(items, "weapon").map((g) => [g.title, g.price, g.items.length])).toEqual([
      ["Tier I", 800, 2],
      ["Tier II", 1600, 1],
      ["Tier IV", 6400, 1],
    ]);
    expect(placeText(catalog.items[8])).toBe("Tier V · Street Brawl");
    expect(placeText(catalog.items[9])).toBe("Tier V · Street Brawl");
    expect(itemGroups(items, "legendary").map((g) => g.title)).toEqual(["Tier V", "Switched off in the game"]);
    expect(categoryCounts(items).get("legendary")).toEqual({ all: 1, off: 1, replaced: 0, rows: 2 });
    expect(categoryCounts(items).get("weapon")).toEqual({ all: 4, off: 0, replaced: 1, rows: 4 });
  });
});
