// The player in Extras → Effects: it starts when an effect is shown on Edit colors (not when
// Extras opens), follows the selection and the edit, stops when the view is left, and the page
// works as before beside it.
import { useState } from "react";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import type { Project } from "../models";
import type { ComponentCatalog } from "../components/AssetCatalog";
import { ExtrasWorkspace } from "./ExtrasPage";
import { clearExtraMediaCache } from "./media";
import { forgetTrees } from "./effectTreesLoad";
import { BOMB, SMOKE, effectTrees, previewAll } from "./effectFixtures";
import reference from "../../src-tauri/crates/deadlock-project/tests/fixtures/reference.json";

const native = vi.hoisted(() => ({
  validateAssetPath: vi.fn(),
  previewExtra: vi.fn(),
  discoverExtras: vi.fn(),
  effectTrees: vi.fn(),
  effectPreview: vi.fn(),
  cancel: vi.fn(),
}));
vi.mock("../api", async (original) => ({ ...(await original<typeof import("../api")>()), api: native }));
const player = vi.hoisted(() => ({
  play: vi.fn(),
  edit: vi.fn(),
  control: vi.fn(),
  stop: vi.fn(),
  frame: vi.fn(),
  listen: vi.fn(() => () => undefined),
  draw: vi.fn(async () => undefined),
}));
vi.mock("./EffectPlayer", async (original) => ({ ...(await original<typeof import("./EffectPlayer")>()), desktopBridge: () => player }));

const initial = { ...reference.cases[1].expected, hero_name: "Test hero", customization: { enabled: true } } as unknown as Project;
const catalog: ComponentCatalog = {
  hero_path: initial.original_model, icons: [], audio: [],
  effects: [{ path: SMOKE, target: SMOKE, label: "Smoke file", supported: true, original_sha256: "e".repeat(64) }],
  animations: [], variants: [], animation_unavailable: null, warnings: [],
};
const ready = { state: { state: "ready", driver: "GPU" }, generation: 1, overlay: [], superseded: false, overlay_ms: 1, reply_ms: 1, loaded: { notes: {} } };
beforeEach(() => {
  forgetTrees();
  clearExtraMediaCache();
  localStorage.clear();
  native.validateAssetPath.mockImplementation(async (p) => p);
  native.previewExtra.mockRejectedValue({ message: "no preview" });
  native.discoverExtras.mockResolvedValue(catalog);
  native.effectTrees.mockResolvedValue(effectTrees);
  native.effectPreview.mockImplementation(async (_p, target, _e, pick) =>
    previewAll(effectTrees.trees.find((t) => t.target === target)!, [125, 77, 255], pick ? { pick: { hue: 90, saturation: 1, brightness: 1, result: [125, 77, 255], result_hex: "#7D4DFF", exact: true } } : {}),
  );
  player.play.mockResolvedValue(ready);
  player.edit.mockResolvedValue({ ...ready, generation: 2 });
  player.control.mockResolvedValue(ready);
  player.stop.mockResolvedValue(undefined);
  player.frame.mockImplementation(() => new Promise((r) => setTimeout(() => r(new ArrayBuffer(0)), 20)));
});
function Harness() {
  const [p, set] = useState(structuredClone(initial));
  return <ExtrasWorkspace project={p} update={set} disabled={false} catalog={catalog} fail={vi.fn()} reviewBuild={vi.fn()} discover={async () => undefined} />;
}

it("plays the shown effect with the current edit, follows the edit and stops on leaving the view", async () => {
  render(<Harness />);
  expect(player.play).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("tab", { name: /Effects/ }));
  await screen.findByRole("option", { name: /Explode/ });
  await waitFor(() => expect(player.play).toHaveBeenCalled());
  expect(player.play.mock.calls[0][1]).toBe(BOMB);
  expect(await screen.findByRole("img", { name: /Moving preview of/ })).toBeInTheDocument();
  // The strips are still there below it.
  expect(screen.getByText("Parts and their colors")).toBeInTheDocument();
  // A preset changes the edit: "yours" follows (the edit call carries the new hue).
  fireEvent.click(screen.getByRole("button", { name: /^Red #/ }));
  await waitFor(() => expect(player.edit).toHaveBeenCalled());
  expect(player.edit.mock.calls.at(-1)![1]).toBe(BOMB);
  expect(player.edit.mock.calls.at(-1)![2]).toMatchObject({ hue: 90 });
  // Leaving Edit colors stops the helper.
  fireEvent.click(screen.getByRole("radio", { name: "Replace a file" }));
  await waitFor(() => expect(player.stop).toHaveBeenCalled());
});
