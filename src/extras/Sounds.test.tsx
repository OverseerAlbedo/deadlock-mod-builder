import { useState } from "react";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Project } from "../models";
import type { ComponentCatalog } from "../components/AssetCatalog";
import { ExtrasWorkspace } from "./ExtrasPage";
import { clearExtraMediaCache } from "./media";
import { soundCatalog, soundEntry } from "./soundFixtures";
import reference from "../../src-tauri/crates/deadlock-project/tests/fixtures/reference.json";

const native = vi.hoisted(() => ({ validateAssetPath: vi.fn(), previewExtra: vi.fn() }));
vi.mock("../api", async (original) => ({
  ...(await original<typeof import("../api")>()),
  api: native,
}));
vi.mock("./AudioPlayer", () => ({
  AudioPlayer: ({ label, volume }: { label: string; volume: number }) => (
    <div data-testid={`player-${label}`}>{volume ?? 100}% audition</div>
  ),
}));
const initial = {
  ...reference.cases[1].expected,
  hero_name: "Vindicta",
  customization: { enabled: true, physics: { enabled: true, source: "cloth.vmdl_c" } },
} as unknown as Project;
const catalog: ComponentCatalog = {
  hero_path: initial.original_model,
  icons: [{ path: "panorama/images/hero.vtex_c", target: "panorama/images/hero.vtex_c", label: "Hero portrait", supported: true, width: 128, height: 128 }],
  audio: soundCatalog,
  effects: [{ path: "particles/hero/fx.vpcf_c", target: "particles/hero/fx.vpcf_c", label: "Trail effect", supported: true }],
  animations: [],
  variants: [],
  animation_unavailable: null,
  warnings: [],
};
function packet(mime_type: string) {
  const h = new TextEncoder().encode(JSON.stringify({ mime_type, details: {} }));
  const buffer = new ArrayBuffer(4 + h.length + 8);
  new DataView(buffer).setUint32(0, h.length, true);
  new Uint8Array(buffer, 4, h.length).set(h);
  return buffer;
}
beforeEach(() => {
  vi.clearAllMocks();
  clearExtraMediaCache();
  native.validateAssetPath.mockImplementation(async (path) => path);
  native.previewExtra.mockImplementation(async ({ kind }) => packet(kind === "icons" ? "image/png" : "audio/wav"));
  vi.stubGlobal("URL", Object.assign(URL, { createObjectURL: vi.fn(() => "blob:test"), revokeObjectURL: vi.fn() }));
});
function Harness({ start = initial, assets = catalog }: { start?: Project; assets?: ComponentCatalog }) {
  const [p, set] = useState(structuredClone(start));
  return (
    <>
      <ExtrasWorkspace project={p} update={set} disabled={false} catalog={assets} fail={vi.fn()} />
      <output data-testid="saved">{JSON.stringify(p)}</output>
    </>
  );
}
const saved = () => JSON.parse(screen.getByTestId("saved").textContent!) as Project;
function openSounds(start?: Project, assets?: ComponentCatalog) {
  render(<Harness start={start} assets={assets} />);
  fireEvent.click(screen.getByRole("tab", { name: /Sounds/ }));
  return within(screen.getByRole("region", { name: "Sounds assets" }));
}
const rowNames = (library: ReturnType<typeof openSounds>) =>
  library.queryAllByRole("button", { name: /^(?!Play|Stop)/ })
    .filter((b) => b.classList.contains("sounds-row-main"))
    .map((b) => b.querySelector("strong")!.textContent);
const chip = (name: string) => screen.getByRole("button", { name: new RegExp(`^${name}\\s*[\\d,]+$`) });
async function drop(selector: string, path: string) {
  const node = document.querySelector(`${selector} [data-native-drop], ${selector}[data-native-drop]`);
  expect(node).toBeTruthy();
  await act(async () => node!.dispatchEvent(new CustomEvent("dmb-file-drop", { detail: [path] })));
}

describe("Extras → Sounds", () => {
  it("groups rows under headings with counts and folds variations into one row", () => {
    const library = openSounds();
    expect(rowNames(library)).toEqual([
      "Assassinate · fire", "Assassinate · scope in", "Flight · cast", "Reload · start",
      "Uses ability 4", "Careful Haze", "Kill Vindicta", "Footstep",
    ]);
    const headings = library.getAllByRole("heading", { level: 3 }).map((h) => h.textContent);
    expect(headings.slice(0, 3)).toEqual(["Assassinate· 2 sounds", "Flight· 1 sound", "Reload· 1 sound"]);
    expect(library.getByText("5 variations · the game picks one at random")).toBeVisible();
    expect(library.getByText("2 lines")).toBeVisible();
    expect(library.getByText("vindicta_assassinate_scope_in")).toBeVisible();
    expect(library.getByText(/Showing 8 sounds · 13 files/)).toBeVisible();
    // Nothing is loaded for the list itself.
    expect(native.previewExtra).not.toHaveBeenCalled();
  });
  it("filters by one tag, by several (all of them) and by voice sub-kinds, with counts", () => {
    const library = openSounds();
    expect(chip("Hero")).toHaveTextContent("Hero7");
    expect(chip("Voice")).toHaveTextContent("Voice3");
    fireEvent.click(chip("Weapon"));
    expect(rowNames(library)).toEqual(["Reload · start"]);
    fireEvent.click(chip("Weapon"));
    fireEvent.click(chip("Hero"));
    fireEvent.click(chip("Ability"));
    expect(rowNames(library)).toEqual(["Assassinate · fire", "Assassinate · scope in", "Flight · cast", "Uses ability 4"]);
    expect(library.getByText(/that are both Hero and Ability/)).toBeVisible();
    expect(screen.queryByRole("group", { name: "Voice line kinds" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Clear" }));
    fireEvent.click(chip("Voice"));
    const kinds = within(screen.getByRole("group", { name: "Voice line kinds" }));
    expect(kinds.getByRole("button", { name: /Emotes/ })).toBeDisabled();
    fireEvent.click(kinds.getByRole("button", { name: /^Pings/ }));
    fireEvent.click(kinds.getByRole("button", { name: /^Other heroes about this hero/ }));
    expect(rowNames(library)).toEqual(["Careful Haze", "Kill Vindicta"]);
    fireEvent.click(chip("Other"));
    expect(library.getByText("No sounds with these tags")).toBeVisible();
  });
  it("searches names, file names and tag names", () => {
    const library = openSounds();
    fireEvent.change(screen.getByLabelText("Search sounds"), { target: { value: "weapon" } });
    expect(rowNames(library)).toEqual(["Reload · start"]);
    fireEvent.change(screen.getByLabelText("Search sounds"), { target: { value: "scope_in" } });
    expect(rowNames(library)).toEqual(["Assassinate · scope in"]);
    fireEvent.change(screen.getByLabelText("Search sounds"), { target: { value: "whistle" } });
    expect(library.getByText("No matching sounds")).toBeVisible();
  });
  it("opens a family on the inspector with a variation picker; each variation is replaced on its own", async () => {
    openSounds();
    fireEvent.click(screen.getByRole("button", { name: /Assassinate · fire/ }));
    const inspector = within(screen.getByRole("complementary", { name: "Replacement editor" }));
    expect(inspector.getByRole("heading", { name: "Assassinate · fire" })).toBeVisible();
    expect(inspector.getByText("Hero")).toBeVisible();
    expect(inspector.getByText("Ability")).toBeVisible();
    const picker = inspector.getByLabelText("Variation") as HTMLSelectElement;
    expect(Array.from(picker.options).map((o) => o.textContent)).toEqual(["1 of 5", "2 of 5", "3 of 5", "4 of 5", "5 of 5"]);
    expect(inspector.getByText("vindicta_assassinate_fire_01")).toBeVisible();
    fireEvent.change(picker, { target: { value: soundCatalog[3].target } });
    expect(inspector.getByText("vindicta_assassinate_fire_04")).toBeVisible();
    await drop(".extra-edit-controls", "C:\\sfx\\shot.wav");
    await waitFor(() => expect(saved().customization.audio).toEqual([
      { enabled: true, source: "C:\\sfx\\shot.wav", target: soundCatalog[3].target, allow_shared: false, original_sha256: soundCatalog[3].original_sha256, volume_percent: 100, audio_fit: "pad" },
    ]));
    expect(screen.getByRole("button", { name: /Assassinate · fire/ })).toHaveTextContent("1 of 5 replaced");
    expect(saved().customization.physics).toEqual(initial.customization.physics);
  });
  it("replaces all variations with one file and names the ones it could not set", async () => {
    openSounds();
    fireEvent.click(screen.getByRole("button", { name: /Assassinate · fire/ }));
    fireEvent.click(screen.getByRole("button", { name: "Replace all 5 variations with one file" }));
    await drop(".sounds-replace-all", "C:\\sfx\\shot.mp3");
    await waitFor(() => expect(saved().customization.audio).toHaveLength(3));
    expect(saved().customization.audio!.map((r) => r.target)).toEqual([soundCatalog[0].target, soundCatalog[1].target, soundCatalog[3].target]);
    expect(saved().customization.audio!.every((r) => r.source === "C:\\sfx\\shot.mp3" && r.allow_shared === false && r.volume_percent === 100 && r.audio_fit === "pad")).toBe(true);
    const report = within(document.querySelector(".sounds-report") as HTMLElement);
    expect(report.getByText(/3 of 5 variations now use shot.mp3/)).toBeVisible();
    expect(report.getByText(/Shared with other heroes/)).toBeVisible();
    expect(report.getByText(/View only: Unsupported sound codec/)).toBeVisible();
    // With the shared choice ticked, the shared variation is set too (and allowed).
    fireEvent.click(screen.getByRole("button", { name: "Replace all 5 variations with one file" }));
    fireEvent.click(screen.getByLabelText(/Also replace the shared variations/));
    await drop(".sounds-replace-all", "C:\\sfx\\shot2.wav");
    await waitFor(() => expect(saved().customization.audio).toHaveLength(4));
    expect(saved().customization.audio!.find((r) => r.target === soundCatalog[2].target)?.allow_shared).toBe(true);
    expect(saved().customization.audio!.every((r) => r.source === "C:\\sfx\\shot2.wav")).toBe(true);
  });
  it("shows only replaced sounds when asked", () => {
    const start = { ...initial, customization: { ...initial.customization, audio: [{ enabled: true, target: soundCatalog[7].target, source: "r.wav" }] } } as Project;
    const library = openSounds(start);
    fireEvent.click(screen.getByRole("button", { name: /^Replaced only/ }));
    expect(rowNames(library)).toEqual(["Reload · start"]);
    expect(library.getByRole("button", { name: /Reload · start/ })).toHaveTextContent("Replaced");
  });
  it("plays one original at a time and loads only the one asked for", async () => {
    const library = openSounds();
    // (The Icons tab shown first asked for its thumbnail; only sound requests count here.)
    const sounds = () => native.previewExtra.mock.calls.map(([r]) => r).filter((r) => r.kind === "audio");
    expect(sounds()).toHaveLength(0);
    const plays = library.getAllByRole("button", { name: "Play original" });
    fireEvent.click(plays[1]);
    await waitFor(() => expect(sounds()).toHaveLength(1));
    expect(sounds()[0]).toMatchObject({ kind: "audio", target: soundCatalog[5].target });
    expect(plays[1]).toHaveAccessibleName("Stop original");
    fireEvent.click(plays[2]);
    await waitFor(() => expect(sounds()).toHaveLength(2));
    expect(sounds()[1]).toMatchObject({ target: soundCatalog[6].target });
    expect(plays[1]).toHaveAccessibleName("Play original");
    expect(library.getAllByRole("button", { name: "Stop original" })).toEqual([plays[2]]);
    fireEvent.click(library.getByRole("button", { name: "Stop original" }));
    expect(library.queryAllByRole("button", { name: "Stop original" })).toHaveLength(0);
  });
  it("pages large catalogs and lists files one by one while choosing a destination", () => {
    const many = Array.from({ length: 1700 }, (_, i) =>
      soundEntry(`sounds/vo/hornet/ping/hornet_ping_line_${i}_01.vsnd_c`, { name: `Line ${i}`, group: "voice:pings", group_name: "Pings", tags: ["hero", "voice"], voice: ["ping"] }),
    );
    const start = { ...initial, customization: { ...initial.customization, audio: [{ enabled: true, target: "sounds/gone.vsnd_c", source: "old.wav" }] } } as Project;
    const t = performance.now();
    const library = openSounds(start, { ...catalog, audio: [...soundCatalog, ...many] });
    expect(library.getAllByRole("button", { name: "Play original" }).length + library.queryAllByRole("button", { name: "Stop original" }).length).toBeLessThanOrEqual(40);
    expect(library.getByText(/^1–40 of 1,709$/)).toBeVisible();
    const first = performance.now() - t;
    fireEvent.click(screen.getByRole("button", { name: /^Replaced only/ }));
    fireEvent.click(screen.getByRole("button", { name: /gone/ }));
    fireEvent.click(screen.getByRole("button", { name: "Choose destination" }));
    expect(library.getByText(/Choose a replaceable sound/)).toBeVisible();
    // Families are unfolded: each fire variation is its own row now.
    fireEvent.change(screen.getByLabelText("Search sounds"), { target: { value: "assassinate fire" } });
    // Variation 5 is view only; the shared variation 3 is still a valid destination.
    expect(rowNames(library)).toHaveLength(4);
    fireEvent.click(library.getAllByRole("button", { name: /Assassinate · fire/ })[0]);
    expect(saved().customization.audio![0]).toMatchObject({ source: "old.wav", target: soundCatalog[0].target, allow_shared: false });
    expect(first).toBeLessThan(5000);
  });
  it("silences one sound without a file, then takes a file instead, and back", async () => {
    openSounds();
    fireEvent.click(screen.getByRole("button", { name: /Flight · cast/ }));
    const inspector = within(screen.getByRole("complementary", { name: "Replacement editor" }));
    expect(inspector.getByText("No file needed: the game plays nothing for this sound.")).toBeVisible();
    fireEvent.click(inspector.getByRole("button", { name: "Silence this sound" }));
    const target = soundCatalog[6].target;
    expect(saved().customization.audio).toEqual([
      { enabled: true, target, allow_shared: false, original_sha256: soundCatalog[6].original_sha256, silence: true },
    ]);
    expect(saved().customization.enabled).toBe(true);
    expect(screen.getByRole("button", { name: /Flight · cast/ })).toHaveTextContent("Silenced");
    expect(inspector.getByText("Silenced: the game plays nothing for this sound.")).toBeVisible();
    expect(inspector.queryByRole("button", { name: "Silence this sound" })).toBeNull();
    // Volume and timing belong to a file; nothing is previewed for silence.
    expect(inspector.queryByLabelText("Replacement volume percent")).toBeNull();
    expect(inspector.queryByLabelText("Sound timing")).toBeNull();
    expect(native.previewExtra.mock.calls.some(([r]) => r.source)).toBe(false);
    expect(inspector.getByText("Silence (no file)")).toBeInTheDocument();
    // Search finds it; Replaced only shows it.
    fireEvent.change(screen.getByLabelText("Search sounds"), { target: { value: "silenced" } });
    expect(screen.getByRole("button", { name: /Flight · cast/ })).toBeVisible();
    fireEvent.change(screen.getByLabelText("Search sounds"), { target: { value: "" } });
    expect(screen.getByText("1 silenced")).toBeVisible();
    // A file replaces the silence and brings a file's defaults.
    await drop(".extra-edit-controls", "C:\\sfx\\song.mp3");
    await waitFor(() => expect(saved().customization.audio).toEqual([
      { enabled: true, target, allow_shared: false, original_sha256: soundCatalog[6].original_sha256, volume_percent: 100, audio_fit: "pad", source: "C:\\sfx\\song.mp3" },
    ]));
    expect(screen.getByRole("button", { name: /Flight · cast/ })).toHaveTextContent("Replaced");
    expect(inspector.getByText("Instead of your file: the game plays nothing for this sound.")).toBeVisible();
    fireEvent.click(inspector.getByRole("button", { name: "Silence this sound" }));
    expect(saved().customization.audio).toEqual([
      { enabled: true, target, allow_shared: false, original_sha256: soundCatalog[6].original_sha256, silence: true },
    ]);
  });
  it("pauses and removes a silenced sound like a replacement, and counts it as a change", () => {
    const start = { ...initial, customization: { ...initial.customization, audio: [{ enabled: true, target: soundCatalog[6].target, silence: true }] } } as Project;
    render(<ExtrasWorkspace project={start} update={vi.fn()} disabled={false} catalog={catalog} fail={vi.fn()} reviewBuild={vi.fn()} />);
    expect(screen.getByText("1 change")).toBeVisible();
    expect(screen.getByRole("tab", { name: /Sounds/ }).querySelector("i[title='Contains replacements']")).toBeTruthy();
    expect(screen.getByRole("button", { name: /Build & install/ })).toBeEnabled();
    cleanup();
    openSounds(start);
    expect(screen.getByRole("button", { name: /Flight · cast/ })).toHaveTextContent("Silenced");
    fireEvent.click(screen.getByRole("button", { name: /Flight · cast/ }));
    const inspector = within(screen.getByRole("complementary", { name: "Replacement editor" }));
    fireEvent.click(inspector.getByRole("checkbox", { name: /Include in build/ }));
    expect(saved().customization.audio![0]).toMatchObject({ enabled: false, silence: true });
    expect(screen.getByRole("button", { name: /Flight · cast/ })).toHaveTextContent("Paused");
    fireEvent.click(inspector.getByRole("checkbox", { name: /Include in build/ }));
    fireEvent.click(screen.getByRole("button", { name: "Pause all extras" }));
    expect(saved().customization.audio![0]).toMatchObject({ enabled: false, silence: true });
    fireEvent.click(screen.getByRole("button", { name: "Resume all extras" }));
    fireEvent.click(inspector.getByRole("button", { name: "Remove silence" }));
    expect(saved().customization.audio).toEqual([]);
    expect(screen.getByRole("button", { name: /Flight · cast/ })).toHaveTextContent("Original");
  });
  it("silences all variations with the replace-all rules and names the ones it could not set", () => {
    // An existing replacement of variation 2 keeps its inclusion, loses its file.
    const start = { ...initial, customization: { ...initial.customization, audio: [
      { enabled: false, target: soundCatalog[1].target, source: "C:\\sfx\\old.wav", allow_shared: false, volume_percent: 150, audio_fit: "trim" as const },
    ] } } as Project;
    openSounds(start);
    fireEvent.click(screen.getByRole("button", { name: /Assassinate · fire/ }));
    fireEvent.click(screen.getByRole("button", { name: "Silence all 5 variations" }));
    const group = within(screen.getByRole("group", { name: "Silence all variations" }));
    fireEvent.click(group.getByRole("button", { name: "Silence 5 variations" }));
    const rows = saved().customization.audio!;
    expect(rows.map((r) => r.target)).toEqual([soundCatalog[1].target, soundCatalog[0].target, soundCatalog[3].target]);
    expect(rows.every((r) => r.silence === true && r.source === undefined && r.volume_percent === undefined && r.audio_fit === undefined)).toBe(true);
    expect(rows[0].enabled).toBe(false);
    const report = within(document.querySelector(".sounds-report") as HTMLElement);
    expect(report.getByText(/3 of 5 variations are now silenced/)).toBeVisible();
    expect(report.getByText(/Tick “Also silence the shared variations”/)).toBeVisible();
    expect(report.getByText(/View only: Unsupported sound codec/)).toBeVisible();
    expect((screen.getByLabelText("Variation") as HTMLSelectElement).options[0].textContent).toBe("1 of 5 · silenced");
    // With the shared choice, the shared variation is silenced (and allowed).
    fireEvent.click(screen.getByRole("button", { name: "Silence all 5 variations" }));
    fireEvent.click(screen.getByLabelText(/Also silence the shared variations/));
    fireEvent.click(screen.getByRole("button", { name: "Silence 5 variations" }));
    expect(saved().customization.audio!.find((r) => r.target === soundCatalog[2].target)).toMatchObject({ silence: true, allow_shared: true });
    expect(saved().customization.audio).toHaveLength(4);
  });
  it("never silences a view-only sound or a 129th row", () => {
    const full = Array.from({ length: 128 }, (_, i) => ({ enabled: true, target: `sounds/other/${i}.vsnd_c`, source: `${i}.wav` }));
    const start = { ...initial, customization: { ...initial.customization, audio: full } } as Project;
    openSounds(start);
    // (The 128 saved rows are not in this catalog; they list first.)
    fireEvent.change(screen.getByLabelText("Search sounds"), { target: { value: "flight" } });
    fireEvent.click(screen.getByRole("button", { name: /Flight · cast/ }));
    const inspector = within(screen.getByRole("complementary", { name: "Replacement editor" }));
    expect(inspector.getByRole("button", { name: "Silence this sound" })).toBeDisabled();
    fireEvent.change(screen.getByLabelText("Search sounds"), { target: { value: "assassinate fire" } });
    fireEvent.click(screen.getByRole("button", { name: /Assassinate · fire/ }));
    fireEvent.click(screen.getByRole("button", { name: "Silence all 5 variations" }));
    fireEvent.click(screen.getByRole("button", { name: "Silence 5 variations" }));
    expect(saved().customization.audio).toHaveLength(128);
    expect(within(document.querySelector(".sounds-report") as HTMLElement).getAllByText(/already have 128 replacements/)).toHaveLength(3);
    // Variation 5 is view only: no silence choice.
    fireEvent.change(screen.getByLabelText("Variation"), { target: { value: soundCatalog[4].target } });
    expect(screen.queryByRole("button", { name: "Silence this sound" })).toBeNull();
  });
  it("refuses silence for a game format it cannot rewrite", () => {
    const legacy = soundCatalog.map((e, i) => (i === 6 ? { ...e, wav_supported: false } : e));
    openSounds(undefined, { ...catalog, audio: legacy });
    fireEvent.click(screen.getByRole("button", { name: /Flight · cast/ }));
    expect(screen.getByRole("button", { name: "Silence this sound" })).toBeDisabled();
    expect(screen.getByText("Its game format cannot be rewritten, so it cannot be silenced.")).toBeVisible();
  });
  it("leaves Icons and Effects as they were", () => {
    render(<Harness />);
    expect(screen.getByRole("region", { name: "Icons assets" })).toBeVisible();
    expect(screen.getByRole("button", { name: "All assets" })).toBeVisible();
    fireEvent.click(screen.getByRole("tab", { name: /Effects/ }));
    // Effects opens on Edit colors (Effects.tsx); the file library is its Replace a file view.
    fireEvent.click(screen.getByRole("radio", { name: "Replace a file" }));
    const effects = within(screen.getByRole("region", { name: "Effects assets" }));
    expect(effects.getByRole("button", { name: /Trail effect/ })).toBeVisible();
    expect(screen.getByRole("button", { name: "Changed (0)" })).toBeVisible();
    expect(screen.queryByRole("button", { name: /^Replaced only/ })).toBeNull();
  });
});
