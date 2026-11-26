import { useState } from "react";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Project } from "../models";
import type { ComponentCatalog } from "../components/AssetCatalog";
import { MIRROR_KEY, setLanguage } from "../i18n";
import { ExtrasWorkspace } from "./ExtrasPage";
import { clearExtraMediaCache } from "./media";
import { forgetTrees } from "./effectTreesLoad";
import { forgetExtrasTabs } from "./extrasTabs";
import { MODE_KEY } from "./effectEdits";
import { EffectPlayer, FPS_KEY, resetSessionView, type PlayerBridge } from "./EffectPlayer";
import type { PlayerAnswer } from "./effectPlayerTypes";
import { BOMB, effectTrees, previewAll } from "./effectFixtures";
import reference from "../../src-tauri/crates/deadlock-project/tests/fixtures/reference.json";

// Extras → Effects in Chinese: the trees load and the page answers in Chinese, while what
// the page saves (rows, the Simple/Advanced mode, the frame rate) stays identifiers and
// numbers, and the names, notes and reasons from the backend come as they are.
const native = vi.hoisted(() => ({
  validateAssetPath: vi.fn(),
  previewExtra: vi.fn(),
  discoverExtras: vi.fn(),
  effectTrees: vi.fn(),
  effectPreview: vi.fn(),
  cancel: vi.fn(),
}));
vi.mock("../api", async (original) => ({ ...(await original<typeof import("../api")>()), api: native }));

const initial = {
  ...reference.cases[1].expected,
  hero_name: "Test hero",
  customization: { enabled: true },
} as unknown as Project;
const catalog: ComponentCatalog = {
  hero_path: initial.original_model, icons: [], audio: [],
  effects: [{ path: "particles/x.vpcf_c", target: "particles/x.vpcf_c", label: "Smoke file", supported: true }],
  animations: [], variants: [], animation_unavailable: null, warnings: [],
};
function Harness() {
  const [p, set] = useState(structuredClone(initial));
  return (
    <>
      <ExtrasWorkspace project={p} update={set} disabled={false} catalog={catalog} fail={vi.fn()} reviewBuild={vi.fn()} discover={async () => undefined} />
      <output data-testid="saved">{JSON.stringify(p)}</output>
    </>
  );
}
const saved = () => JSON.parse(screen.getByTestId("saved").textContent!) as Project;
async function openEffects() {
  render(<Harness />);
  fireEvent.click(screen.getByRole("tab", { name: /^特效/ }));
  await screen.findByRole("option", { name: /Explode/ });
}
const editor = () => within(screen.getByRole("complementary", { name: "特效编辑器" }));

beforeEach(async () => {
  vi.clearAllMocks();
  forgetTrees();
  forgetExtrasTabs();
  clearExtraMediaCache();
  resetSessionView();
  localStorage.clear();
  native.previewExtra.mockRejectedValue({ message: "no preview" });
  native.discoverExtras.mockResolvedValue(catalog);
  native.effectTrees.mockResolvedValue(effectTrees);
  native.cancel.mockResolvedValue(undefined);
  native.effectPreview.mockImplementation(async (_p, target, _edit, pick) =>
    previewAll(effectTrees.trees.find((tree) => tree.target === target)!, [125, 77, 255], pick ? { pick: { hue: 90, saturation: 1, brightness: 1, result: [125, 77, 255], result_hex: "#7D4DFF", exact: true } } : {}),
  );
  await act(() => setLanguage("zh-CN"));
});
afterEach(async () => {
  cleanup();
  await act(() => setLanguage("en"));
  localStorage.removeItem(MIRROR_KEY);
});

describe("Extras → Effects in Chinese", () => {
  it("loads the effects, keeps the backend's names and notes, and recolors with a preset", async () => {
    await openEffects();
    expect(native.effectTrees).toHaveBeenCalledTimes(1);
    const list = within(screen.getByRole("listbox", { name: "特效" }));
    // Group names and the "Also used by" note come from the backend as they are.
    expect(list.getByRole("group", { name: "Big Bomb" })).toBeVisible();
    const view = within(screen.getByRole("region", { name: "已选中的特效" }));
    expect(view.getByText("Also used by: Another ability (Other hero). 你的更改在那里也会显示。")).toBeVisible();
    expect(view.getByText("大小、亮度和不透明度对其无效：3D 模型没有此设置。")).toBeVisible();
    fireEvent.click(editor().getByRole("button", { name: "霓虹紫 #7D4DFF" }));
    await waitFor(() => expect(saved().customization.effect_edits).toEqual([{ target: BOMB, edit: { hue: 90 } }]));
    expect(screen.getByText("1 项更改")).toBeVisible();
    expect(list.getByRole("option", { name: /Explode/ })).toHaveTextContent("已更改");
  });

  it("switches to Advanced by its Chinese name and remembers the mode as an identifier", async () => {
    await openEffects();
    fireEvent.click(editor().getByRole("radio", { name: "高级" }));
    expect(localStorage.getItem(MODE_KEY)).toBe("advanced");
    expect(editor().getByRole("heading", { name: /此特效中的颜色/ })).toBeVisible();
    fireEvent.click(editor().getByRole("radio", { name: "简单" }));
    expect(localStorage.getItem(MODE_KEY)).toBe("simple");
  });

  it("a failed load keeps its message and help from the backend under the Chinese title", async () => {
    const refused = { message: "Load this character's assets first.", help: { cause: "x", steps: ["Press Reload assets."], action: null } };
    native.effectTrees.mockRejectedValueOnce(refused).mockRejectedValueOnce(refused);
    render(<Harness />);
    fireEvent.click(screen.getByRole("tab", { name: /^特效/ }));
    expect(await screen.findByRole("alert")).toHaveTextContent("无法读取特效。 Load this character's assets first. Press Reload assets.");
    const list = within(screen.getByRole("region", { name: "特效" }));
    fireEvent.click(list.getByRole("button", { name: "重试" }));
    await screen.findByRole("option", { name: /Explode/ });
  });

  it("switches language live on an open effect", async () => {
    await openEffects();
    expect(screen.getByRole("heading", { name: "部件及其颜色" })).toBeVisible();
    await act(() => setLanguage("en"));
    expect(screen.getByRole("heading", { name: "Parts and their colors" })).toBeVisible();
    expect(screen.getByRole("listbox", { name: "Effects" })).toBeVisible();
  });
});

describe("The effect player in Chinese", () => {
  const tree = effectTrees.trees.find((item) => item.target === BOMB)!;
  const project = { id: "a".repeat(32), original_model: "models/heroes_wip/test/test.vmdl" } as unknown as Project;
  const answer = (extra: Partial<PlayerAnswer> = {}): PlayerAnswer => ({ state: { state: "ready", driver: "GPU" }, generation: 1, overlay: [], superseded: false, overlay_ms: 1, reply_ms: 1, loaded: { effect: BOMB, notes: {} }, ...extra });
  function bridge(play: PlayerAnswer) {
    return {
      play: vi.fn(async () => play),
      edit: vi.fn(async () => answer()),
      control: vi.fn(async () => answer({ loaded: undefined })),
      stop: vi.fn(async () => undefined),
      frame: vi.fn(async () => { await new Promise((resolve) => setTimeout(resolve, 10)); return new ArrayBuffer(0); }),
      listen: vi.fn(() => () => undefined),
      draw: vi.fn(async () => undefined),
    } as unknown as PlayerBridge;
  }

  it("plays with Chinese controls and saves the frame rate as a number", async () => {
    render(<EffectPlayer project={project} tree={tree} edit={{}} bridge={bridge(answer())} />);
    const player = within(await screen.findByRole("region", { name: "特效预览" }));
    expect(player.getByRole("checkbox", { name: "与原版特效对比" })).toBeInTheDocument();
    fireEvent.change(player.getByRole("combobox", { name: "帧率" }), { target: { value: "144" } });
    expect(localStorage.getItem(FPS_KEY)).toBe("144");
    expect(player.getByRole("radio", { name: "黑色背景" })).toBeInTheDocument();
  });

  it("an unavailable player says why in Chinese, with the helper's message as it comes", async () => {
    render(<EffectPlayer project={project} tree={tree} edit={{}} bridge={bridge(answer({ state: { state: "unavailable", reason: "opengl", message: "OpenGL 4.6 is not available." } }))} />);
    expect(await screen.findByRole("note")).toHaveTextContent("动态预览无法在此电脑上运行：OpenGL 4.6 is not available. 下方的颜色条会显示你的编辑。");
    await act(() => setLanguage("en"));
    expect(screen.getByRole("note")).toHaveTextContent("The moving preview cannot run on this computer: OpenGL 4.6 is not available. The color strips below show your edit.");
  });
});
