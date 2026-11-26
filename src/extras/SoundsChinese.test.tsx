import { useState } from "react";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Project } from "../models";
import type { ComponentCatalog } from "../components/AssetCatalog";
import { MIRROR_KEY, setLanguage } from "../i18n";
import { ExtrasWorkspace } from "./ExtrasPage";
import { clearExtraMediaCache } from "./media";
import { forgetExtrasTabs } from "./extrasTabs";
import { soundCatalog } from "./soundFixtures";
import reference from "../../src-tauri/crates/deadlock-project/tests/fixtures/reference.json";

// Extras → Sounds in Chinese: the tag filters are driven by their Chinese names, the rows
// the page writes hold targets, paths and numbers only, and sound names come as they are.
const native = vi.hoisted(() => ({ validateAssetPath: vi.fn(), previewExtra: vi.fn() }));
vi.mock("../api", async (original) => ({ ...(await original<typeof import("../api")>()), api: native }));
vi.mock("./AudioPlayer", () => ({ AudioPlayer: ({ label }: { label: string }) => <div data-testid={`player-${label}`} /> }));

const initial = {
  ...reference.cases[1].expected,
  hero_name: "Vindicta",
  customization: { enabled: true },
} as unknown as Project;
const catalog: ComponentCatalog = {
  hero_path: initial.original_model, icons: [], audio: soundCatalog, effects: [],
  animations: [], variants: [], animation_unavailable: null, warnings: [],
};
function packet(mime_type: string) {
  const head = new TextEncoder().encode(JSON.stringify({ mime_type, details: {} }));
  const buffer = new ArrayBuffer(4 + head.length + 8);
  new DataView(buffer).setUint32(0, head.length, true);
  new Uint8Array(buffer, 4, head.length).set(head);
  return buffer;
}
function Harness() {
  const [p, set] = useState(structuredClone(initial));
  return (
    <>
      <ExtrasWorkspace project={p} update={set} disabled={false} catalog={catalog} fail={vi.fn()} />
      <output data-testid="saved">{JSON.stringify(p)}</output>
    </>
  );
}
const saved = () => JSON.parse(screen.getByTestId("saved").textContent!) as Project;
function openSounds() {
  render(<Harness />);
  fireEvent.click(screen.getByRole("tab", { name: /^声音/ }));
  return within(screen.getByRole("region", { name: "声音资源" }));
}
const chip = (name: string) => screen.getByRole("button", { name: new RegExp(`^${name}\\s*[\\d,]+$`) });
async function drop(selector: string, path: string) {
  const node = document.querySelector(`${selector} [data-native-drop]`);
  expect(node).toBeTruthy();
  await act(async () => node!.dispatchEvent(new CustomEvent("dmb-file-drop", { detail: [path] })));
}

beforeEach(async () => {
  vi.clearAllMocks();
  clearExtraMediaCache();
  forgetExtrasTabs();
  native.validateAssetPath.mockImplementation(async (path) => path);
  native.previewExtra.mockImplementation(async () => packet("audio/wav"));
  vi.stubGlobal("URL", Object.assign(URL, { createObjectURL: vi.fn(() => "blob:test"), revokeObjectURL: vi.fn() }));
  await act(() => setLanguage("zh-CN"));
});
afterEach(async () => {
  cleanup();
  await act(() => setLanguage("en"));
  localStorage.removeItem(MIRROR_KEY);
});

describe("Extras → Sounds in Chinese", () => {
  it("filters by the Chinese tag names and says so in one line", () => {
    const library = openSounds();
    fireEvent.click(chip("英雄"));
    fireEvent.click(chip("技能"));
    expect(library.getByText(/^显示 4 个声音，同时带有“英雄”和“技能”标签/)).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "清空" }));
    fireEvent.click(chip("语音"));
    const kinds = within(screen.getByRole("group", { name: "语音类型" }));
    fireEvent.click(kinds.getByRole("button", { name: /^标记/ }));
    expect(library.getByText(/^显示 \d+ 个声音，标签为“语音”，（标记）/)).toBeVisible();
    // Sound names come from the game's catalog as they are.
    expect(library.getByRole("button", { name: /Careful Haze/ })).toBeVisible();
  });

  it("replaces all variations with one file: the rows hold targets, the path and numbers, the report is Chinese", async () => {
    openSounds();
    fireEvent.click(screen.getByRole("button", { name: /Assassinate · fire/ }));
    fireEvent.click(screen.getByRole("button", { name: "用一个文件替换全部 5 个变体" }));
    await drop(".sounds-replace-all", "C:\\sfx\\shot.mp3");
    await waitFor(() => expect(saved().customization.audio).toHaveLength(3));
    for (const row of saved().customization.audio!)
      expect(row).toEqual({ enabled: true, source: "C:\\sfx\\shot.mp3", target: expect.any(String), allow_shared: false, original_sha256: expect.any(String), volume_percent: 100, audio_fit: "pad" });
    const report = within(document.querySelector(".sounds-report") as HTMLElement);
    expect(report.getByText("5 个变体中已有 3 个使用 shot.mp3")).toBeVisible();
    expect(report.getByText(/与其他英雄共享。勾选“同时替换共享的变体”即可包含它。/)).toBeVisible();
    // A reason from the game's catalog stays as it comes, inside a Chinese sentence.
    expect(report.getByText(/仅供查看：Unsupported sound codec/)).toBeVisible();
  });

  it("silences one sound and shows its state in Chinese while the row says silence: true", async () => {
    openSounds();
    fireEvent.click(screen.getByRole("button", { name: /Reload · start/ }));
    fireEvent.click(screen.getByRole("button", { name: "将此声音静音" }));
    await waitFor(() => expect(saved().customization.audio).toHaveLength(1));
    expect(saved().customization.audio![0]).toMatchObject({ enabled: true, silence: true });
    expect(saved().customization.audio![0].source).toBeUndefined();
    expect(screen.getByText("已静音：游戏不会为此声音播放任何内容。")).toBeVisible();
    expect(screen.getByText("已静音 1 个")).toBeVisible();
  });

  it("switches language live with the filter kept", async () => {
    const library = openSounds();
    fireEvent.click(chip("武器"));
    expect(library.getByText(/^显示 1 个声音，标签为“武器”/)).toBeVisible();
    await act(() => setLanguage("en"));
    expect(library.getByText(/^Showing 1 sound tagged Weapon/)).toBeVisible();
    expect(chip("Weapon")).toHaveAttribute("aria-pressed", "true");
  });
});
