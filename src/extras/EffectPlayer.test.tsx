import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Project } from "../models";
import { EffectPlayer, FPS_KEY, nothingToDraw, playerNotes, resetSessionView, savedFps, type PlayerBridge } from "./EffectPlayer";
import { BOMB, effectTrees } from "./effectFixtures";
import { FRAME_PREFIX, parseFrame, type PlayerAnswer, type PlayerEvent, type PlayerFrame, type PlayerLoaded } from "./effectPlayerTypes";
import type { EffectEdit } from "./effectTypes";

const project = { id: "a".repeat(32), original_model: "models/heroes_wip/test/test.vmdl" } as unknown as Project;
const tree = effectTrees.trees.find((t) => t.target === BOMB)!;
const ready = { state: "ready", driver: "Test GPU" } as const;
const answer = (extra: Partial<PlayerAnswer> = {}): PlayerAnswer => ({ state: ready, generation: 1, overlay: [], superseded: false, overlay_ms: 1, reply_ms: 1, loaded: { effect: BOMB, notes: {} }, ...extra });

/** A frame as the bridge sends it: 16-byte header, 32-byte helper header, a fake JPEG. */
function frameBytes(counter: number, generation: number, opts: { time?: number; coverage?: number; flags?: number } = {}) {
  const buffer = new ArrayBuffer(FRAME_PREFIX + 32 + 4);
  const v = new DataView(buffer);
  [0x44, 0x4d, 0x42, 0x46].forEach((b, i) => v.setUint8(i, b));
  v.setUint32(4, counter, true);
  v.setFloat64(8, Date.now(), true);
  v.setUint32(16, counter, true);
  v.setUint32(20, generation, true);
  v.setFloat32(24, opts.time ?? counter / 30, true);
  v.setUint16(32, 640, true);
  v.setUint16(34, 360, true);
  v.setFloat32(36, opts.coverage ?? 0.2, true);
  v.setUint8(40, opts.flags ?? 0);
  return buffer;
}

function mockBridge() {
  const frames: ArrayBuffer[] = [];
  let handler: ((e: PlayerEvent) => void) | null = null;
  const drawn: PlayerFrame[] = [];
  const bridge = {
    play: vi.fn(async (_p: Project, _t: string, _e: EffectEdit, _v: unknown) => answer()),
    edit: vi.fn(async (_p: Project, _t: string, _e: EffectEdit) => answer({ generation: 2 })),
    control: vi.fn(async (_c: { command: string }) => answer({ loaded: undefined })),
    stop: vi.fn(async () => undefined),
    frame: vi.fn(async (after: number) => {
      const next = frames.findIndex((f) => new DataView(f).getUint32(4, true) > after);
      if (next >= 0) {
        // A burst: everything waiting, the newest wins (as the Rust slot does).
        const newest = frames.splice(0).pop()!;
        return newest;
      }
      await new Promise((r) => setTimeout(r, 10));
      return new ArrayBuffer(0);
    }),
    listen: vi.fn((h: (e: PlayerEvent) => void) => {
      handler = h;
      return () => { handler = null; };
    }),
    draw: vi.fn(async (_c: HTMLCanvasElement, f: PlayerFrame) => { drawn.push(f); }),
  };
  return { bridge: bridge as unknown as PlayerBridge & typeof bridge, frames, drawn, emit: (e: PlayerEvent) => act(() => handler?.(e)) };
}

beforeEach(() => {
  localStorage.clear();
  resetSessionView();
});

describe("EffectPlayer", () => {
  it("starts with the saved frame rate and session view, then draws only the newest frame", async () => {
    localStorage.setItem(FPS_KEY, "144");
    const m = mockBridge();
    render(<EffectPlayer project={project} tree={tree} edit={{ hue: 115 }} bridge={m.bridge} />);
    expect(screen.getByRole("status")).toHaveTextContent("Starting the effect player…");
    await waitFor(() => expect(m.bridge.play).toHaveBeenCalled());
    const [, target, edit, view] = m.bridge.play.mock.calls[0];
    expect(target).toBe(BOMB);
    expect(edit).toEqual({ hue: 115 });
    expect(view).toMatchObject({ fps: 144, compare: false, loop: true, speed: 1, background: "0b0c10", brightness: 0 });
    await screen.findByRole("img", { name: /Moving preview of .*your version/ });
    m.frames.push(frameBytes(1, 1), frameBytes(2, 1), frameBytes(3, 1, { time: 0.5 }));
    await waitFor(() => expect(m.drawn.length).toBeGreaterThan(0));
    expect(m.drawn.map((f) => f.counter)).toEqual([3]);
    await waitFor(() => expect(screen.getByText("0.50 s")).toBeInTheDocument());
    expect(screen.getByText("Yours")).toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: "Frame rate" })).toHaveValue("144");
  });

  it("an invalid or unreadable saved frame rate falls back to 30", () => {
    localStorage.setItem(FPS_KEY, "75");
    expect(savedFps()).toBe(30);
    localStorage.setItem(FPS_KEY, "nonsense");
    expect(savedFps()).toBe(30);
    const get = vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => { throw new Error("blocked"); });
    expect(savedFps()).toBe(30);
    get.mockRestore();
    localStorage.setItem(FPS_KEY, "60");
    expect(savedFps()).toBe(60);
  });

  it("a refused frame rate plays at what the helper takes and keeps the saved preference", async () => {
    const m = mockBridge();
    m.bridge.control.mockImplementation(async (c: { command: string }) =>
      c.command === "fps" ? answer({ loaded: undefined, notice: "This effect player cannot play at 144 frames per second here, so it plays at 120." }) : answer({ loaded: undefined }),
    );
    render(<EffectPlayer project={project} tree={tree} edit={{}} bridge={m.bridge} />);
    await screen.findByRole("img", { name: /Moving preview/ });
    fireEvent.change(screen.getByRole("combobox", { name: "Frame rate" }), { target: { value: "144" } });
    await screen.findByText(/plays at 120/);
    expect(m.bridge.control).toHaveBeenCalledWith({ command: "fps", value: 144 });
    expect(localStorage.getItem(FPS_KEY)).toBe("144");
    // Storage that throws does not break the choice.
    const set = vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => { throw new Error("blocked"); });
    fireEvent.change(screen.getByRole("combobox", { name: "Frame rate" }), { target: { value: "60" } });
    await waitFor(() => expect(m.bridge.control).toHaveBeenCalledWith({ command: "fps", value: 60 }));
    set.mockRestore();
  });

  it("no helper: one quiet line, no player", async () => {
    const m = mockBridge();
    m.bridge.play.mockResolvedValue(answer({ state: { state: "unavailable", reason: "missing", message: "x" }, generation: undefined }));
    render(<EffectPlayer project={project} tree={tree} edit={{}} bridge={m.bridge} />);
    const note = await screen.findByRole("note");
    expect(note).toHaveTextContent("The moving preview is not part of this copy of the app. The color strips below show your edit.");
    expect(screen.queryByRole("img")).toBeNull();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("no OpenGL 4.6: the helper's sentence, no error", async () => {
    const m = mockBridge();
    m.bridge.play.mockResolvedValue(answer({ state: { state: "unavailable", reason: "opengl", message: "OpenGL 4.6 is not available on this computer." }, generation: undefined }));
    render(<EffectPlayer project={project} tree={tree} edit={{}} bridge={m.bridge} />);
    expect(await screen.findByRole("note")).toHaveTextContent("The moving preview cannot run on this computer: OpenGL 4.6 is not available on this computer. The color strips below show your edit.");
  });

  it("stopped after an error: one line and Try again, which plays again", async () => {
    const m = mockBridge();
    m.bridge.play.mockResolvedValueOnce(answer({ state: { state: "failed", message: "The effect player stopped again." }, generation: undefined }));
    render(<EffectPlayer project={project} tree={tree} edit={{}} bridge={m.bridge} />);
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("The effect player stopped again. The color strips below still show your edit.");
    // Drawn as a small button, not as text.
    expect(screen.getByRole("button", { name: "Try again" })).toHaveClass("extra-secondary", "small");
    fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    await screen.findByRole("img", { name: /Moving preview/ });
    expect(m.bridge.play).toHaveBeenCalledTimes(2);
  });

  it("the helper dying later: a restarted line, then failed on the event", async () => {
    const m = mockBridge();
    render(<EffectPlayer project={project} tree={tree} edit={{}} bridge={m.bridge} />);
    await screen.findByRole("img", { name: /Moving preview/ });
    m.emit({ type: "restarted", message: "The effect player stopped unexpectedly. It was started again." });
    expect(await screen.findByText(/It was started again/)).toBeInTheDocument();
    m.emit({ type: "failed", message: "The effect player stopped again." });
    expect(await screen.findByRole("alert")).toHaveTextContent("stopped again");
  });

  it("a refused command shows one line and keeps the picture", async () => {
    const m = mockBridge();
    m.bridge.play.mockResolvedValue(answer({ error: "The effect was not found." }));
    render(<EffectPlayer project={project} tree={tree} edit={{}} bridge={m.bridge} />);
    expect(await screen.findByText("The preview could not show this: The effect was not found.")).toBeInTheDocument();
    expect(screen.getByRole("img", { name: /Moving preview/ })).toBeInTheDocument();
  });

  it("edits while dragging: one call at a time, the newest edit wins", async () => {
    const m = mockBridge();
    let finish: (a: PlayerAnswer) => void = () => undefined;
    m.bridge.edit.mockImplementation(() => new Promise<PlayerAnswer>((r) => { finish = r; }));
    const { rerender } = render(<EffectPlayer project={project} tree={tree} edit={{ hue: 10 }} bridge={m.bridge} />);
    await screen.findByRole("img", { name: /Moving preview/ });
    rerender(<EffectPlayer project={project} tree={tree} edit={{ hue: 20 }} bridge={m.bridge} />);
    await waitFor(() => expect(m.bridge.edit).toHaveBeenCalledTimes(1));
    expect(screen.getByText("Updating yours…")).toBeInTheDocument();
    for (const hue of [30, 40, 50]) rerender(<EffectPlayer project={project} tree={tree} edit={{ hue }} bridge={m.bridge} />);
    await new Promise((r) => setTimeout(r, 40));
    expect(m.bridge.edit).toHaveBeenCalledTimes(1);
    await act(async () => finish(answer({ generation: 2 })));
    await waitFor(() => expect(m.bridge.edit).toHaveBeenCalledTimes(2));
    expect(m.bridge.edit.mock.calls[1][2]).toEqual({ hue: 50 });
    await act(async () => finish(answer({ generation: 3 })));
    await waitFor(() => expect(screen.queryByText("Updating yours…")).toBeNull());
    expect(m.bridge.play).toHaveBeenCalledTimes(1);
  });

  it("controls send their commands; compare labels both halves", async () => {
    const m = mockBridge();
    render(<EffectPlayer project={project} tree={tree} edit={{}} bridge={m.bridge} />);
    await screen.findByRole("img", { name: /Moving preview/ });
    fireEvent.click(screen.getByRole("button", { name: "Pause" }));
    expect(m.bridge.control).toHaveBeenLastCalledWith({ command: "pause" });
    fireEvent.click(screen.getByRole("button", { name: "Play" }));
    expect(m.bridge.control).toHaveBeenLastCalledWith({ command: "resume" });
    fireEvent.click(screen.getByRole("button", { name: "Loop" }));
    expect(m.bridge.control).toHaveBeenLastCalledWith({ command: "loop", on: false });
    fireEvent.click(screen.getByRole("radio", { name: "¼×" }));
    expect(m.bridge.control).toHaveBeenLastCalledWith({ command: "speed", value: 0.25 });
    fireEvent.click(screen.getByRole("checkbox", { name: "Compare with the game effect" }));
    expect(m.bridge.control).toHaveBeenLastCalledWith({ command: "compare", on: true });
    expect(screen.getByText("Game")).toBeInTheDocument();
    expect(screen.getByRole("img", { name: /the game's version on the left, yours on the right/ })).toBeInTheDocument();
    expect(screen.getByText("The game's version and yours play in step.")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("radio", { name: "Slate background" }));
    expect(m.bridge.control).toHaveBeenLastCalledWith({ command: "background", color: "2a2e3a" });
    expect(screen.getAllByRole("radio", { name: /background/ })).toHaveLength(2);
    fireEvent.change(screen.getByRole("slider", { name: "View brightness" }), { target: { value: "2.5" } });
    expect(m.bridge.control).toHaveBeenLastCalledWith({ command: "brightness", stops: 2.5 });
    fireEvent.click(screen.getByRole("button", { name: "Fit to effect" }));
    expect(m.bridge.control).toHaveBeenLastCalledWith({ command: "fit" });
    fireEvent.click(screen.getByRole("button", { name: "Reset view" }));
    expect(m.bridge.control).toHaveBeenLastCalledWith({ command: "reset_view" });
    // Drag, wheel and keys: deltas summed per animation frame.
    const picture = screen.getByRole("img", { name: /Moving preview/ });
    fireEvent.pointerDown(picture, { clientX: 10, clientY: 10, pointerId: 1 });
    fireEvent.pointerMove(picture, { clientX: 20, clientY: 6, pointerId: 1 });
    fireEvent.pointerMove(picture, { clientX: 30, clientY: 6, pointerId: 1 });
    fireEvent.pointerUp(picture, { pointerId: 1 });
    await waitFor(() => expect(m.bridge.control).toHaveBeenCalledWith({ command: "orbit", dx: 20, dy: 4 }));
    fireEvent.wheel(picture, { deltaY: -200 });
    await waitFor(() => expect(m.bridge.control).toHaveBeenCalledWith({ command: "zoom", steps: 2 }));
    fireEvent.keyDown(picture, { key: "ArrowLeft" });
    await waitFor(() => expect(m.bridge.control).toHaveBeenCalledWith({ command: "orbit", dx: -12, dy: 0 }));
    // The session keeps these choices for the next effect shown (not the project).
    const { unmount } = render(<EffectPlayer project={project} tree={tree} edit={{}} bridge={mockBridge().bridge} />);
    unmount();
  });

  it("the session remembers the view choices; leaving stops the helper", async () => {
    const m = mockBridge();
    const first = render(<EffectPlayer project={project} tree={tree} edit={{}} bridge={m.bridge} />);
    await screen.findByRole("img", { name: /Moving preview/ });
    fireEvent.click(screen.getByRole("checkbox", { name: "Compare with the game effect" }));
    fireEvent.click(screen.getByRole("radio", { name: "½×" }));
    first.unmount();
    expect(m.bridge.stop).toHaveBeenCalled();
    const n = mockBridge();
    render(<EffectPlayer project={project} tree={tree} edit={{}} bridge={n.bridge} />);
    await waitFor(() => expect(n.bridge.play).toHaveBeenCalled());
    expect(n.bridge.play.mock.calls[0][3]).toMatchObject({ compare: true, speed: 0.5 });
    expect(localStorage.length).toBe(0);
  });

  it("a finished effect offers Play again, which restarts it", async () => {
    const m = mockBridge();
    render(<EffectPlayer project={project} tree={tree} edit={{}} bridge={m.bridge} />);
    await screen.findByRole("img", { name: /Moving preview/ });
    m.emit({ type: "finished", generation: 1 });
    fireEvent.click(await screen.findByRole("button", { name: "Play again" }));
    expect(m.bridge.control).toHaveBeenLastCalledWith({ command: "restart" });
  });

  it("says so when nothing can be drawn instead of a black box", async () => {
    const m = mockBridge();
    m.bridge.play.mockResolvedValue(answer({ loaded: { bounds: { empty: true }, notes: { cannotShow: ["This effect only gives off light, which has nothing to light here."] } } }));
    render(<EffectPlayer project={project} tree={tree} edit={{}} bridge={m.bridge} />);
    expect(await screen.findByText("Nothing to draw here")).toBeInTheDocument();
    expect(screen.getAllByText("This effect only gives off light, which has nothing to light here.").length).toBeGreaterThan(0);
  });

  it("an edit made while the player starts is sent once it plays", async () => {
    const m = mockBridge();
    let started: (a: PlayerAnswer) => void = () => undefined;
    m.bridge.play.mockImplementation(() => new Promise<PlayerAnswer>((r) => { started = r; }));
    const { rerender } = render(<EffectPlayer project={project} tree={tree} edit={{ hue: 10 }} bridge={m.bridge} />);
    await waitFor(() => expect(m.bridge.play).toHaveBeenCalled());
    rerender(<EffectPlayer project={project} tree={tree} edit={{ hue: 99 }} bridge={m.bridge} />);
    expect(m.bridge.edit).not.toHaveBeenCalled();
    await act(async () => started(answer()));
    await waitFor(() => expect(m.bridge.edit).toHaveBeenCalledTimes(1));
    expect(m.bridge.edit.mock.calls[0][2]).toEqual({ hue: 99 });
  });

  it("a refused play (effects not loaded, the game changed) stops with one line and Try again", async () => {
    const m = mockBridge();
    m.bridge.play.mockRejectedValueOnce({ message: "The game files changed since the effects were loaded. Click Reload assets in Extras" });
    render(<EffectPlayer project={project} tree={tree} edit={{}} bridge={m.bridge} />);
    expect(await screen.findByRole("alert")).toHaveTextContent("The game files changed since the effects were loaded.");
    expect(screen.getByRole("button", { name: "Try again" })).toBeInTheDocument();
  });

  it("pulls at most one frame per animation frame, whatever the stream's rate", async () => {
    const ticks: (() => void)[] = [];
    const raf = vi.spyOn(window, "requestAnimationFrame").mockImplementation((cb) => { ticks.push(() => cb(performance.now())); return ticks.length; });
    const m = mockBridge();
    let n = 0;
    m.bridge.frame.mockImplementation(async () => frameBytes(++n, 1)); // a frame is always waiting (a fast stream)
    render(<EffectPlayer project={project} tree={tree} edit={{}} bridge={m.bridge} />);
    await waitFor(() => expect(m.bridge.frame).toHaveBeenCalled());
    let fired = 0;
    for (let i = 0; i < 10; i++) {
      await act(async () => { const due = ticks.splice(0); fired++; due.forEach((t) => t()); await new Promise((r) => setTimeout(r, 5)); });
    }
    expect(m.bridge.frame.mock.calls.length).toBeLessThanOrEqual(fired + 1);
    expect(m.drawn.length).toBeLessThanOrEqual(fired);
    raf.mockRestore();
  });

  it("a finished effect plays again when new frames come (an edit restarts it)", async () => {
    const m = mockBridge();
    render(<EffectPlayer project={project} tree={tree} edit={{}} bridge={m.bridge} />);
    await screen.findByRole("img", { name: /Moving preview/ });
    m.frames.push(frameBytes(1, 1, { flags: 4 }));
    expect(await screen.findByRole("button", { name: "Play again" })).toBeInTheDocument();
    m.frames.push(frameBytes(2, 1));
    expect(await screen.findByRole("button", { name: "Pause" })).toBeInTheDocument();
  });

  it("a new effect loads in the same player", async () => {
    const m = mockBridge();
    const other = effectTrees.trees.find((t) => t.target !== BOMB)!;
    const { rerender } = render(<EffectPlayer project={project} tree={tree} edit={{}} bridge={m.bridge} />);
    await screen.findByRole("img", { name: /Moving preview/ });
    rerender(<EffectPlayer project={project} tree={other} edit={{}} bridge={m.bridge} />);
    await waitFor(() => expect(m.bridge.play).toHaveBeenCalledTimes(2));
    expect(m.bridge.play.mock.calls[1][1]).toBe(other.target);
    expect(m.bridge.stop).not.toHaveBeenCalled();
  });
});

describe("notes and frames", () => {
  it("names what the preview cannot show, in plain words", () => {
    const loaded: PlayerLoaded = {
      notes: {
        untintedModels: { renderers: 2, parts: ["particles/a/ghost_bomb_model.vpcf_c"], note: "x" },
        decals: { renderers: 1, parts: ["particles/a/ghost_bomb_proj.vpcf_c"], surface: "floor", note: "x" },
        cannotShow: ["Parts placed on a character have no model here."],
      },
      compareNote: "They match particle for particle.",
    };
    expect(playerNotes(loaded, false)).toEqual([
      "3D model parts (ghost bomb model) are drawn without their colors here; in the game they keep their own colors.",
      "Decals are shown on a flat floor; in the game they also land on walls and slopes.",
      "Parts placed on a character have no model here.",
    ]);
    expect(playerNotes(loaded, true).at(-1)).toBe("They match particle for particle.");
    expect(playerNotes(null, false)).toEqual([]);
    expect(nothingToDraw({ bounds: { size: "1,1,1" } }, 0.3)).toBeNull();
    expect(nothingToDraw({ notes: { cannotShow: ["Only light."] } }, 0)).toBe("Only light.");
  });
  it("reads the frame layout", () => {
    const f = parseFrame(frameBytes(7, 3, { time: 1.25, coverage: 0.5, flags: 1 | 4 }))!;
    expect(f).toMatchObject({ counter: 7, sequence: 7, generation: 3, time: 1.25, width: 640, height: 360, coverage: 0.5, compare: true, paused: false, finished: true });
    expect(f.jpeg.byteLength).toBe(4);
    expect(parseFrame(new ArrayBuffer(0))).toBeNull();
  });
});
