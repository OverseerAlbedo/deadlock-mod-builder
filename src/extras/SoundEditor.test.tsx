import { useState } from "react";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Project, ResourceReplacement } from "../models";
import type { ComponentCatalog } from "../components/AssetCatalog";
import { ExtrasWorkspace } from "./ExtrasPage";
import { clearExtraMediaCache } from "./media";
import { soundCatalog } from "./soundFixtures";
import { editValues, hasEdit, replaceAllWithPart, withEdit, withoutEdit } from "./soundEdit";
import { assetItems } from "./models";
import { silencedRow, withSource } from "./soundTags";
import reference from "../../src-tauri/crates/deadlock-project/tests/fixtures/reference.json";

const native = vi.hoisted(() => ({ validateAssetPath: vi.fn(), previewExtra: vi.fn(), soundEditPreview: vi.fn() }));
vi.mock("../api", async (original) => ({
  ...(await original<typeof import("../api")>()),
  api: native,
}));
vi.mock("./AudioPlayer", () => ({
  AudioPlayer: ({ label }: { label: string }) => <div data-testid={`player-${label}`} />,
}));

const single = soundCatalog[6]; // Flight · cast, one file
const family = soundCatalog[0]; // Assassinate · fire, five variations
const base = {
  ...reference.cases[1].expected,
  hero_name: "Vindicta",
  customization: { enabled: true },
} as unknown as Project;
const catalog: ComponentCatalog = {
  hero_path: base.original_model,
  icons: [],
  audio: soundCatalog,
  effects: [],
  animations: [],
  variants: [],
  animation_unavailable: null,
  warnings: [],
};
function packet(mime_type: string, details: unknown, body = 8) {
  const h = new TextEncoder().encode(JSON.stringify({ mime_type, details }));
  const buffer = new ArrayBuffer(4 + h.length + body);
  new DataView(buffer).setUint32(0, h.length, true);
  new Uint8Array(buffer, 4, h.length).set(h);
  return buffer;
}
/** An answer like Rust's for a 10 s file and a 2 s game sound. */
function answer(row: ResourceReplacement, extra: Record<string, unknown> = {}) {
  const edited = hasEdit(row);
  return {
    edited,
    source: { duration: 10, rate: 48000, channels: 1, peaks: Array.from({ length: 480 }, (_, i) => (i % 10) / 10) },
    game: { duration: 2, rate: 44100, channels: 2, frames: 88200, loop_start: -1, loop_end: 0, loudness: -12, peaks: Array(480).fill(0.4) },
    result: {
      peaks: Array(480).fill(0.3),
      loudness: -15,
      report: {
        padded_frames: 0,
        clipped_samples: 0,
        window: edited
          ? { policy: "extras-audio-window-v1", start_frame: 0, file_frames: 88200, requested_gain_db: row.volume_db ?? 0, applied_gain_db: row.volume_db ?? 0,
              gain_limited: false, fade_in_frames: 0, fade_out_frames: 0, fade_in_shortened: false, fade_out_shortened: false, looping: false }
          : null,
      },
    },
    refusal: null,
    matched: { volume_db: 3, limited_by_clipping: false, limited_by_range: false },
    percent: row.volume_percent ?? 100,
    percent_db: 20 * Math.log10((row.volume_percent ?? 100) / 100),
    ...extra,
  };
}
let respond: (row: ResourceReplacement) => Record<string, unknown> = (row) => answer(row);
beforeEach(() => {
  vi.clearAllMocks();
  clearExtraMediaCache();
  respond = (row) => answer(row);
  native.validateAssetPath.mockImplementation(async (path) => path);
  native.previewExtra.mockImplementation(async () => packet("audio/wav", {}));
  native.soundEditPreview.mockImplementation(async ({ row }: { row: ResourceReplacement }) => packet("audio/wav", respond(row)));
  vi.stubGlobal("URL", Object.assign(URL, { createObjectURL: vi.fn(() => "blob:test"), revokeObjectURL: vi.fn() }));
});
function project(rows: ResourceReplacement[]) {
  const p = structuredClone(base);
  p.customization.audio = rows;
  return p;
}
const fileRow = (extra: Partial<ResourceReplacement> = {}): ResourceReplacement => ({
  enabled: true, source: "C:\\music\\song.wav", target: single.target, allow_shared: false,
  original_sha256: single.original_sha256 ?? undefined, volume_percent: 100, audio_fit: "pad", ...extra,
});
let updates = 0;
function Harness({ start }: { start: Project }) {
  const [p, set] = useState(start);
  return (
    <>
      <ExtrasWorkspace project={p} update={(next) => { updates++; set(next); }} disabled={false} catalog={catalog} fail={vi.fn()} />
      <output data-testid="saved">{JSON.stringify(p)}</output>
    </>
  );
}
const saved = () => JSON.parse(screen.getByTestId("saved").textContent!) as Project;
const row0 = () => saved().customization.audio![0];
async function openEditor(rows: ResourceReplacement[], sound = /Flight · cast/) {
  render(<Harness start={project(rows)} />);
  fireEvent.click(screen.getByRole("tab", { name: /Sounds/ }));
  fireEvent.click(screen.getByRole("button", { name: sound }));
  updates = 0;
  fireEvent.click(screen.getByRole("button", { name: "Edit the part, volume and fades" }));
  const editor = within(screen.getByRole("region", { name: "Sound editor" }));
  await waitFor(() => expect(editor.getByText(/The game fixes this sound’s length: 2.00 s/)).toBeVisible());
  return editor;
}
function typeValue(editor: ReturnType<typeof within>, label: string, text: string) {
  const box = editor.getByLabelText(`${label} value`);
  fireEvent.focus(box);
  fireEvent.change(box, { target: { value: text } });
  fireEvent.keyDown(box, { key: "Enter" });
}

describe("Extras → Sounds editor", () => {
  it("opens on a sound with a file, draws the frame as long as the game's sound and writes nothing", async () => {
    const editor = await openEditor([fileRow()]);
    expect(updates).toBe(0);
    expect(saved().customization.audio).toEqual([fileRow()]);
    expect(editor.getByText("Vindicta · Flight · cast")).toBeVisible();
    expect(editor.getByText("song.wav")).toBeVisible();
    const frame = editor.getByTestId("sound-frame");
    expect(frame.style.left).toBe("0%");
    expect(frame.style.width).toBe("20%");
    expect(editor.getByText(/it is always 2.00 s long/)).toBeVisible();
    expect(editor.getByLabelText("Starts at value")).toHaveValue("0.00 s");
    expect(editor.getByLabelText("Volume value")).toHaveValue("0 dB");
    expect(editor.getByText("What the game will play · 2.00 s")).toBeVisible();
    expect(editor.getByText("The game’s sound, for comparison")).toBeVisible();
    // The preview asked for the saved row as it is.
    expect(native.soundEditPreview).toHaveBeenCalledWith(expect.objectContaining({ hero_path: base.original_model, row: fileRow() }));
    // No Undo or Redo here: the app's own undo covers these edits.
    expect(editor.queryByRole("button", { name: /Undo|Redo/ })).toBeNull();
    fireEvent.click(editor.getByRole("button", { name: "Back to sounds" }));
    expect(screen.queryByRole("region", { name: "Sound editor" })).toBeNull();
    expect(updates).toBe(0);
  });

  it("writes typed values and slider steps to the row, inside the saved format's ranges", async () => {
    const editor = await openEditor([fileRow()]);
    typeValue(editor, "Starts at", "2.6");
    expect(row0().start_seconds).toBe(2.6);
    expect(editor.getByTestId("sound-frame").style.left).toBe("26%");
    typeValue(editor, "Volume", "+3");
    expect(row0().volume_db).toBe(3);
    typeValue(editor, "Volume", "−40 dB");
    expect(row0().volume_db).toBe(-24);
    typeValue(editor, "Fade in", "0.15");
    fireEvent.change(editor.getByLabelText("Fade out"), { target: { value: "0.37" } });
    expect(row0()).toMatchObject({ start_seconds: 2.6, volume_db: -24, fade_in_seconds: 0.15, fade_out_seconds: 0.37, source: "C:\\music\\song.wav", volume_percent: 100, audio_fit: "pad" });
    // Not a number: nothing is written.
    const before = JSON.stringify(row0());
    typeValue(editor, "Fade in", "soft");
    expect(JSON.stringify(row0())).toBe(before);
    // The latest preview is of the edited row.
    await waitFor(() => expect(native.soundEditPreview).toHaveBeenLastCalledWith(expect.objectContaining({ row: expect.objectContaining({ fade_out_seconds: 0.37 }) })));
  });

  it("drags the frame and writes the start once, when it is let go", async () => {
    const editor = await openEditor([fileRow({ start_seconds: 1 })]);
    const area = editor.getByTestId("sound-frame").parentElement!;
    area.getBoundingClientRect = () => ({ left: 0, width: 1000, top: 0, height: 100, right: 1000, bottom: 100, x: 0, y: 0, toJSON: () => ({}) });
    const frame = editor.getByTestId("sound-frame");
    updates = 0;
    fireEvent.pointerDown(frame, { clientX: 150, pointerId: 1 });
    fireEvent.pointerMove(frame, { clientX: 250, pointerId: 1 });
    expect(updates).toBe(0);
    expect(editor.getByTestId("sound-frame").style.left).toBe("20%");
    fireEvent.pointerUp(frame, { clientX: 250, pointerId: 1 });
    expect(updates).toBe(1);
    expect(row0().start_seconds).toBe(2);
    // Never past the last place the whole frame fits (10 s file, 2 s sound).
    fireEvent.pointerDown(frame, { clientX: 200, pointerId: 1 });
    fireEvent.pointerMove(frame, { clientX: 2000, pointerId: 1 });
    fireEvent.pointerUp(frame, { clientX: 2000, pointerId: 1 });
    expect(row0().start_seconds).toBe(8);
    // Arrow keys move it by 0.01 s (Shift: 0.1 s).
    fireEvent.keyDown(frame, { key: "ArrowLeft", shiftKey: true });
    expect(row0().start_seconds).toBe(7.9);
  });

  it("matches the game's loudness, explains the clipping protection and resets every edit", async () => {
    respond = (row) => answer(row, hasEdit(row) && (row.volume_db ?? 0) > 5
      ? { result: { ...answer(row).result, report: { padded_frames: 0, clipped_samples: 0, window: { ...answer(row).result.report.window, gain_limited: true, applied_gain_db: 4.2 } } } }
      : {});
    const editor = await openEditor([fileRow({ volume_percent: 150 })]);
    expect(editor.getByText(/An earlier volume of 150% \(\+3\.5 dB\) also applies/)).toBeVisible();
    fireEvent.click(editor.getByRole("button", { name: "Match the game’s loudness" }));
    expect(row0().volume_db).toBe(3);
    typeValue(editor, "Volume", "9");
    // Rust applied +4.2 dB in all, 3.5 dB of it the older 150%: the Volume value is held at +0.7 dB.
    await waitFor(() => expect(editor.getByText(/Volume held at \+0\.7 dB: louder would clip/)).toBeVisible());
    fireEvent.click(editor.getByRole("button", { name: "Reset edits" }));
    expect(row0()).toEqual(fileRow({ volume_percent: 100 }));
    expect(editor.getByRole("button", { name: "Reset edits" })).toBeDisabled();
  });

  it("draws a looping sound's loop points on the frame and has no fade out for it", async () => {
    respond = (row) => answer(row, { game: { ...answer(row).game, loop_start: 22050, loop_end: 0 } });
    const editor = await openEditor([fileRow({ start_seconds: 1 })]);
    const loop = editor.getByTitle("Loop start (the game's)");
    expect(loop.style.left).toBe("25%");
    expect(editor.getByLabelText("Fade out")).toBeDisabled();
  });

  it("offers the part in the frame when the old timing refuses a longer file", async () => {
    respond = (row) => hasEdit(row) ? answer(row) : answer(row, { result: null, refusal: "Your sound is 10.00s; the original is 2.00s. Enable Trim longer audio to use the first 2.00s, or choose a shorter clip." });
    const editor = await openEditor([fileRow()]);
    // The editor's own sentence: it never names a control the editor does not have.
    expect(editor.getByText(/Your file \(10.00 s\) is longer than the game’s sound \(2.00 s\), so nothing plays yet/)).toBeVisible();
    expect(editor.queryByText(/Enable Trim/)).toBeNull();
    fireEvent.click(editor.getByRole("button", { name: "Use the part in the frame" }));
    expect(row0().start_seconds).toBe(0);
    await waitFor(() => expect(editor.queryByText(/so nothing plays yet/)).toBeNull());
  });

  it("replaces all variations with this part under the rules of replace-all", async () => {
    const editor = await openEditor([{ ...fileRow({ target: family.target, original_sha256: family.original_sha256 ?? undefined }), start_seconds: 2.5, fade_in_seconds: 0.2 }], /Assassinate · fire/);
    fireEvent.click(editor.getByRole("button", { name: "Replace all 5 variations with this part" }));
    fireEvent.click(editor.getByRole("button", { name: "Replace 5 variations" }));
    await waitFor(() => expect(screen.queryByRole("region", { name: "Sound editor" })).toBeNull());
    const rows = saved().customization.audio!;
    // Shared and view-only variations are refused, as by "Replace all".
    expect(rows.map((r) => r.target)).toEqual([soundCatalog[0].target, soundCatalog[1].target, soundCatalog[3].target]);
    expect(rows.every((r) => r.source === "C:\\music\\song.wav" && r.start_seconds === 2.5 && r.fade_in_seconds === 0.2 && r.volume_db === undefined)).toBe(true);
    expect(screen.getByText(/3 of 5 variations now use song.wav/)).toBeVisible();
  });

  it("replaces all variations with the part in the frame even when nothing was edited yet", async () => {
    const editor = await openEditor([fileRow({ target: family.target, original_sha256: family.original_sha256 ?? undefined, volume_percent: 150 })], /Assassinate · fire/);
    fireEvent.click(editor.getByRole("button", { name: "Replace all 5 variations with this part" }));
    fireEvent.click(editor.getByRole("button", { name: "Replace 5 variations" }));
    await waitFor(() => expect(screen.queryByRole("region", { name: "Sound editor" })).toBeNull());
    const rows = saved().customization.audio!;
    expect(rows).toHaveLength(3);
    // Every variation plays the part in the frame (start 0) at the same level, so "pad" never refuses the longer file.
    expect(rows.every((r) => r.start_seconds === 0 && r.volume_percent === 150 && r.volume_db === undefined)).toBe(true);
  });

  it("shows why the file cannot be read where the file is drawn", async () => {
    native.soundEditPreview.mockRejectedValue({ kind: "validation", message: "This clip exceeds the 16-million-sample editing limit. Choose a shorter clip.", details: "" });
    render(<Harness start={project([fileRow()])} />);
    fireEvent.click(screen.getByRole("tab", { name: /Sounds/ }));
    fireEvent.click(screen.getByRole("button", { name: /Flight · cast/ }));
    fireEvent.click(screen.getByRole("button", { name: "Edit the part, volume and fades" }));
    const editor = within(screen.getByRole("region", { name: "Sound editor" }));
    await waitFor(() => expect(editor.getByRole("alert")).toHaveTextContent(/16-million-sample editing limit/));
    expect(editor.queryByText("Reading your file…")).toBeNull();
  });

  it("silencing an edited sound drops the edit with the file", async () => {
    const editor = await openEditor([fileRow({ volume_db: 3, start_seconds: 1 })]);
    fireEvent.click(editor.getByRole("button", { name: "Silence this sound" }));
    expect(row0()).toEqual({ enabled: true, target: single.target, allow_shared: false, original_sha256: single.original_sha256, silence: true });
    expect(screen.queryByRole("region", { name: "Sound editor" })).toBeNull();
  });

  it("leads a file too long for a replacement without an edit to the editor", async () => {
    for (const message of [
      "Your sound is 360.00s; the original is 1.50s. Enable Trim longer audio to use the first 1.50s, or choose a shorter clip.",
      "This clip exceeds the 16-million-sample editing limit. Choose a shorter clip.",
      "Replacement files must be smaller than 128 MiB",
    ]) {
      native.previewExtra.mockImplementation(async ({ source }: { source?: string }) => {
        if (source) throw { kind: "validation", message, details: "" };
        return packet("audio/wav", {});
      });
      const view = render(<Harness start={project([fileRow({ source: "C:\\music\\six-minutes.mp3" })])} />);
      fireEvent.click(screen.getByRole("tab", { name: /Sounds/ }));
      fireEvent.click(screen.getByRole("button", { name: /Flight · cast/ }));
      const inspector = within(screen.getByRole("complementary", { name: "Replacement editor" }));
      await waitFor(() => expect(inspector.getByText("This file is longer than the game’s sound: choose the part that plays.")).toBeVisible());
      expect(inspector.queryByText(/Enable Trim/)).toBeNull();
      updates = 0;
      fireEvent.click(inspector.getByRole("button", { name: "Choose the part that plays" }));
      expect(screen.getByRole("region", { name: "Sound editor" })).toBeVisible();
      expect(updates).toBe(0);
      view.unmount();
      clearExtraMediaCache();
    }
    // Other preview errors are shown as before.
    native.previewExtra.mockImplementation(async ({ source }: { source?: string }) => {
      if (source) throw { kind: "validation", message: "Choose mono or stereo audio between 8 kHz and 192 kHz.", details: "" };
      return packet("audio/wav", {});
    });
    render(<Harness start={project([fileRow()])} />);
    fireEvent.click(screen.getByRole("tab", { name: /Sounds/ }));
    fireEvent.click(screen.getByRole("button", { name: /Flight · cast/ }));
    await waitFor(() => expect(screen.getByText(/Choose mono or stereo audio/)).toBeVisible());
    expect(screen.queryByRole("button", { name: "Choose the part that plays" })).toBeNull();
  });

  it("shows only the way into the editor for an edited row in the inspector", async () => {
    render(<Harness start={project([fileRow({ start_seconds: 2.6, volume_db: 3, fade_in_seconds: 0.15, fade_out_seconds: 0.37 })])} />);
    fireEvent.click(screen.getByRole("tab", { name: /Sounds/ }));
    fireEvent.click(screen.getByRole("button", { name: /Flight · cast/ }));
    const inspector = within(screen.getByRole("complementary", { name: "Replacement editor" }));
    expect(inspector.getByText("From 2.60 s · +3.0 dB · fade in 0.15 s, fade out 0.37 s")).toBeVisible();
    expect(inspector.queryByLabelText("Replacement volume percent")).toBeNull();
    expect(inspector.queryByLabelText("Sound timing")).toBeNull();
    expect(inspector.getByText(/Edited: listen to exactly what the game will play/)).toBeVisible();
    expect(native.soundEditPreview).not.toHaveBeenCalled();
  });
});

describe("sound edit rows", () => {
  it("rounds and limits values, resets and strips them", () => {
    const row = fileRow({ volume_percent: 150 });
    const edited = withEdit(row, { start_seconds: 2.34567, volume_db: 99, fade_in_seconds: -1, fade_out_seconds: Number.NaN });
    expect(editValues(edited)).toEqual({ start_seconds: 2.346, volume_db: 12, fade_in_seconds: 0, fade_out_seconds: 0 });
    expect(withoutEdit(edited)).toEqual({ ...row, volume_percent: 100 });
    const { volume_percent: _unset, ...noPercent } = fileRow();
    expect(withoutEdit(noPercent)).not.toHaveProperty("volume_percent");
    expect(hasEdit(row)).toBe(false);
    expect(hasEdit(withEdit(row, { fade_out_seconds: 0 }))).toBe(true);
    expect(silencedRow(edited)).toEqual({ enabled: true, target: single.target, allow_shared: false, original_sha256: single.original_sha256, silence: true });
    // A new file: the part starts at 0 of it; volume and fades stay.
    expect(withSource(edited, "C:\\other.wav")).toEqual({ ...edited, start_seconds: 0, source: "C:\\other.wav" });
    expect(withSource(row, "C:\\other.wav")).toEqual({ ...row, source: "C:\\other.wav" });
  });
  it("replace-all with a part keeps the 128-row limit and the shared choice", () => {
    const items = assetItems(soundCatalog.slice(0, 5), []);
    const full = Array.from({ length: 127 }, (_, i) => ({ enabled: true, target: `sounds/x/${i}.vsnd_c`, source: "a.wav" }));
    const result = replaceAllWithPart(full, items, "C:\\b.wav", { volume_db: -3 }, true);
    expect(result.rows).toHaveLength(128);
    expect(result.set).toHaveLength(1);
    expect(result.rows[127]).toMatchObject({ source: "C:\\b.wav", volume_db: -3, target: soundCatalog[0].target });
    expect(result.refused.some((r) => /already have 128/.test(r.reason))).toBe(true);
    const compiled = replaceAllWithPart([], items, "C:\\b.vsnd_c", { volume_db: -3 }, true);
    expect(compiled.set).toHaveLength(0);
  });
});

describe("preview requests", () => {
  it("sends one at a time and only the newest waiting one", async () => {
    const pending: ((v: ArrayBuffer) => void)[] = [];
    native.soundEditPreview.mockImplementation(({ row }: { row: ResourceReplacement }) => new Promise<ArrayBuffer>((resolve) => pending.push(() => resolve(packet("audio/wav", answer(row))))));
    render(<Harness start={project([fileRow()])} />);
    fireEvent.click(screen.getByRole("tab", { name: /Sounds/ }));
    fireEvent.click(screen.getByRole("button", { name: /Flight · cast/ }));
    fireEvent.click(screen.getByRole("button", { name: "Edit the part, volume and fades" }));
    await waitFor(() => expect(native.soundEditPreview).toHaveBeenCalledTimes(1));
    const editor = within(screen.getByRole("region", { name: "Sound editor" }));
    await act(async () => pending.shift()!(new ArrayBuffer(0)));
    await waitFor(() => expect(editor.getByLabelText("Volume value")).toBeEnabled());
    typeValue(editor, "Volume", "1");
    await waitFor(() => expect(native.soundEditPreview).toHaveBeenCalledTimes(2));
    typeValue(editor, "Volume", "2");
    typeValue(editor, "Volume", "3");
    expect(native.soundEditPreview).toHaveBeenCalledTimes(2);
    await act(async () => pending.shift()!(new ArrayBuffer(0)));
    await waitFor(() => expect(native.soundEditPreview).toHaveBeenCalledTimes(3));
    expect(native.soundEditPreview.mock.calls[2][0].row.volume_db).toBe(3);
    await act(async () => pending.shift()!(new ArrayBuffer(0)));
    expect(native.soundEditPreview).toHaveBeenCalledTimes(3);
  });
});
