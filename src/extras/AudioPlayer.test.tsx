import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { AudioPlayer } from "./AudioPlayer";
const sources: {
  start: ReturnType<typeof vi.fn>;
  stop: ReturnType<typeof vi.fn>;
  disconnect: ReturnType<typeof vi.fn>;
  onended: null | (() => void);
  buffer: unknown;
  connect: ReturnType<typeof vi.fn>;
}[] = [];
const gains: {
  gain: { value: number; setTargetAtTime: ReturnType<typeof vi.fn> };
  connect: ReturnType<typeof vi.fn>;
  disconnect: ReturnType<typeof vi.fn>;
}[] = [];
let resume: () => Promise<void> = () => Promise.resolve();
class Context {
  currentTime = 0;
  destination = {};
  resume() {
    return resume();
  }
  async decodeAudioData() {
    return {
      duration: 2,
      numberOfChannels: 1,
      getChannelData: () => new Float32Array([0, 0.7, -0.6, 0.1]),
    };
  }
  createBufferSource() {
    const node = {
      start: vi.fn(),
      stop: vi.fn(),
      disconnect: vi.fn(),
      buffer: null,
      onended: null,
      connect: vi.fn((next) => next),
    };
    sources.push(node);
    return node;
  }
  createGain() {
    const node = {
      gain: { value: 1, setTargetAtTime: vi.fn() },
      connect: vi.fn((next) => next),
      disconnect: vi.fn(),
    };
    gains.push(node);
    return node;
  }
  createWaveShaper() {
    return { curve: null, connect: vi.fn((next) => next), disconnect: vi.fn() };
  }
}
const bytes = new ArrayBuffer(8);
beforeEach(() => {
  sources.length = 0;
  gains.length = 0;
  resume = () => Promise.resolve();
  vi.stubGlobal("AudioContext", Context);
});
it("auditions actual gain, updates a playing sound, mutes, and stops other players", async () => {
  const Pair = ({ volume }: { volume: number }) => (
    <>
      <AudioPlayer bytes={bytes} label="original" />
      <AudioPlayer bytes={bytes} label="replacement" volume={volume} />
    </>
  );
  const view = render(<Pair volume={150} />);
  await waitFor(() =>
    expect(
      screen.getByRole("button", { name: "Play replacement" }),
    ).toBeEnabled(),
  );
  fireEvent.click(screen.getByRole("button", { name: "Play replacement" }));
  await waitFor(() => expect(sources).toHaveLength(1));
  expect(gains[0].gain.value).toBe(1.5);
  expect(sources[0].start).toHaveBeenCalledWith(0, 0);
  view.rerender(<Pair volume={200} />);
  expect(gains[0].gain.setTargetAtTime).toHaveBeenLastCalledWith(2, 0, 0.008);
  expect(screen.getByText(/Peaks will clip/)).toBeVisible();
  view.rerender(<Pair volume={0} />);
  expect(gains[0].gain.setTargetAtTime).toHaveBeenLastCalledWith(0, 0, 0.008);
  fireEvent.click(screen.getByRole("button", { name: "Play original" }));
  await waitFor(() => expect(sources).toHaveLength(2));
  expect(sources[0].stop).toHaveBeenCalledTimes(1);
  expect(gains[1].gain.value).toBe(1);
  fireEvent.change(screen.getByLabelText("Seek original"), {
    target: { value: "1.25" },
  });
  await waitFor(() => expect(sources).toHaveLength(3));
  expect(sources[2].start).toHaveBeenCalledWith(0, 1.25);
  view.unmount();
  expect(sources[2].stop).toHaveBeenCalled();
  expect(sources[2].disconnect).toHaveBeenCalled();
});
it("does not start a pending audition after the asset is closed or a build begins", async () => {
  let release!: () => void;
  resume = () =>
    new Promise((resolve) => {
      release = resolve;
    });
  const view = render(<AudioPlayer bytes={bytes} label="replacement" />);
  await waitFor(() =>
    expect(
      screen.getByRole("button", { name: "Play replacement" }),
    ).toBeEnabled(),
  );
  fireEvent.click(screen.getByRole("button", { name: "Play replacement" }));
  view.unmount();
  await act(async () => release());
  expect(sources).toHaveLength(0);
  const next = render(<AudioPlayer bytes={bytes} label="replacement" />);
  await waitFor(() =>
    expect(
      screen.getByRole("button", { name: "Play replacement" }),
    ).toBeEnabled(),
  );
  fireEvent.click(screen.getByRole("button", { name: "Play replacement" }));
  next.rerender(<AudioPlayer bytes={bytes} label="replacement" disabled />);
  await act(async () => release());
  expect(sources).toHaveLength(0);
});
