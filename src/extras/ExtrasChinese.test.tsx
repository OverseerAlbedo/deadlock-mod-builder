import { useState } from "react";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Project } from "../models";
import type { ComponentCatalog } from "../components/AssetCatalog";
import { MIRROR_KEY, setLanguage } from "../i18n";
import { ExtrasWorkspace } from "./ExtrasPage";
import { clearExtraMediaCache } from "./media";
import { forgetExtrasTabs, openTabs } from "./extrasTabs";
import { soundCatalog } from "./soundFixtures";
import reference from "../../src-tauri/crates/deadlock-project/tests/fixtures/reference.json";

// The Extras page frame and Icons in Chinese: the tabs and filters are driven by their
// Chinese names, and what the page saves or remembers stays English or as given.
const native = vi.hoisted(() => ({ validateAssetPath: vi.fn(), previewExtra: vi.fn() }));
vi.mock("../api", async (original) => ({ ...(await original<typeof import("../api")>()), api: native }));
vi.mock("./AudioPlayer", () => ({ AudioPlayer: ({ label }: { label: string }) => <div data-testid={`player-${label}`} /> }));

const initial = {
  ...reference.cases[1].expected,
  hero_name: "Vindicta",
  customization: { enabled: true },
} as unknown as Project;
const catalog: ComponentCatalog = {
  hero_path: initial.original_model,
  icons: [{ path: "panorama/images/hero.vtex_c", target: "panorama/images/hero.vtex_c", label: "Hero portrait", supported: true, width: 128, height: 128 }],
  audio: soundCatalog,
  effects: [{ path: "particles/hero/fx.vpcf_c", target: "particles/hero/fx.vpcf_c", label: "Trail effect", supported: true }],
  animations: [],
  variants: [],
  animation_unavailable: null,
  warnings: ["One file could not be read."],
};
function packet(mime_type: string) {
  const head = new TextEncoder().encode(JSON.stringify({ mime_type, details: {} }));
  const buffer = new ArrayBuffer(4 + head.length + 8);
  new DataView(buffer).setUint32(0, head.length, true);
  new Uint8Array(buffer, 4, head.length).set(head);
  return buffer;
}
function Harness({ start = initial, assets = catalog, discover }: { start?: Project; assets?: ComponentCatalog | null; discover?: () => Promise<void> }) {
  const [p, set] = useState(structuredClone(start));
  return (
    <>
      <ExtrasWorkspace project={p} update={set} disabled={false} catalog={assets} discover={discover} fail={vi.fn()} reviewBuild={vi.fn()} />
      <output data-testid="saved">{JSON.stringify(p)}</output>
    </>
  );
}
const saved = () => JSON.parse(screen.getByTestId("saved").textContent!) as Project;
async function drop(path: string) {
  const node = document.querySelector("[data-native-drop]");
  expect(node).toBeTruthy();
  await act(async () => node!.dispatchEvent(new CustomEvent("dmb-file-drop", { detail: [path] })));
}

beforeEach(async () => {
  vi.clearAllMocks();
  clearExtraMediaCache();
  forgetExtrasTabs();
  native.validateAssetPath.mockImplementation(async (path) => path);
  native.previewExtra.mockImplementation(async ({ kind }) => packet(kind === "icons" ? "image/png" : "audio/wav"));
  vi.stubGlobal("URL", Object.assign(URL, { createObjectURL: vi.fn(() => "blob:test"), revokeObjectURL: vi.fn() }));
  await act(() => setLanguage("zh-CN"));
});
afterEach(async () => {
  cleanup();
  await act(() => setLanguage("en"));
  localStorage.removeItem(MIRROR_KEY);
});

describe("Extras in Chinese", () => {
  it("shows the frame and the Icons library in Chinese, and switches tabs by their Chinese names", () => {
    render(<Harness />);
    expect(screen.getByRole("heading", { name: /附加内容/ })).toBeVisible();
    const tabs = within(screen.getByRole("tablist", { name: "附加内容类别" }));
    expect(tabs.getByRole("tab", { name: /^图标/ })).toHaveAttribute("aria-selected", "true");
    const library = within(screen.getByRole("region", { name: "图标资源" }));
    expect(library.getByRole("heading", { name: "图标库" })).toBeVisible();
    expect(library.getByRole("textbox", { name: "搜索图标" })).toHaveAttribute("placeholder", "搜索图标…");
    expect(library.getByRole("button", { name: "全部资源" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByText("查找备注（1）")).toBeInTheDocument();
    // The tab is remembered by its id, never by its shown name.
    fireEvent.click(tabs.getByRole("tab", { name: /^声音/ }));
    expect(openTabs.get(initial.id)).toBe("audio");
    expect(screen.getByRole("region", { name: "声音资源" })).toBeVisible();
    fireEvent.click(tabs.getByRole("tab", { name: /^特效/ }));
    expect(openTabs.get(initial.id)).toBe("effects");
  });

  it("replaces an icon from a dropped file and saves the path and the fit as identifiers", async () => {
    render(<Harness />);
    fireEvent.click(screen.getByRole("button", { name: /Hero portrait/ }));
    const editor = within(screen.getByRole("complementary", { name: "替换编辑器" }));
    expect(editor.getByText("已选中的图标")).toBeVisible();
    expect(editor.getByText("将你的图标拖放到这里")).toBeVisible();
    await drop("C:\\Icons\\portrait.png");
    await waitFor(() => expect(saved().customization.icons).toHaveLength(1));
    expect(saved().customization.icons![0]).toMatchObject({ source: "C:\\Icons\\portrait.png", target: catalog.icons[0].target, fit: "contain" });
    fireEvent.click(editor.getByRole("button", { name: /填充并裁剪/ }));
    expect(saved().customization.icons![0].fit).toBe("cover");
    expect(screen.getByText("1 项更改")).toBeVisible();
    expect(editor.getByText("已替换")).toBeVisible();
    // The filter keys stay English; the shown name is Chinese.
    fireEvent.click(screen.getByRole("button", { name: "已更改（1）" }));
    expect(screen.getByRole("button", { name: /Hero portrait/ })).toBeVisible();
  });

  it("a refused file keeps the backend's message as it comes", async () => {
    native.validateAssetPath.mockRejectedValue({ message: "The file is not a PNG or JPEG image." });
    render(<Harness />);
    fireEvent.click(screen.getByRole("button", { name: /Hero portrait/ }));
    await drop("C:\\Icons\\notes.txt");
    expect(await screen.findByRole("alert")).toHaveTextContent("The file is not a PNG or JPEG image.");
    expect(saved().customization.icons ?? []).toHaveLength(0);
  });

  it("loads the character's assets from the empty state and pauses the extras", async () => {
    const discover = vi.fn(async () => undefined);
    const start = { ...initial, customization: { enabled: true, icons: [{ enabled: true, source: "C:\\a.png", target: catalog.icons[0].target, allow_shared: false, fit: "contain" }] } } as unknown as Project;
    render(<Harness start={start} assets={null} discover={discover} />);
    // The page discovers by itself once; the button asks again.
    await waitFor(() => expect(discover).toHaveBeenCalledTimes(1));
    fireEvent.click(within(screen.getByRole("region", { name: "图标资源" })).getByRole("button", { name: "加载英雄资源" }));
    expect(discover).toHaveBeenCalledTimes(2);
    expect(screen.getByText("1 项生效的更改")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "暂停全部附加内容" }));
    expect(saved().customization.icons![0].enabled).toBe(false);
    expect(screen.getByRole("button", { name: "恢复全部附加内容" })).toBeVisible();
  });

  it("switches back to English at once, keeping the open tab and the edit", async () => {
    render(<Harness />);
    fireEvent.click(screen.getByRole("tab", { name: /^声音/ }));
    expect(screen.getByRole("region", { name: "声音资源" })).toBeVisible();
    await act(() => setLanguage("en"));
    expect(screen.getByRole("region", { name: "Sounds assets" })).toBeVisible();
    expect(screen.getByRole("tab", { name: /^Sounds/ })).toHaveAttribute("aria-selected", "true");
    await act(() => setLanguage("zh-CN"));
    expect(screen.getByRole("region", { name: "声音资源" })).toBeVisible();
  });
});
