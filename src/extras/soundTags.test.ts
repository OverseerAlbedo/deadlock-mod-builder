import { describe, expect, it } from "vitest";
import type { ResourceReplacement } from "../models";
import { assetItems, isSilenced } from "./models";
import { soundCatalog, soundEntry } from "./soundFixtures";
import {
  MAX_REPLACEMENTS,
  changedStatus,
  newSilencedRow,
  rowStatus,
  silenceAll,
  silenceRefusal,
  silencedRow,
  withSource,
  filterRows,
  replaceAll,
  shownText,
  soundRows,
  tagCounts,
  type SoundFilter,
} from "./soundTags";

const none: SoundFilter = { query: "", tags: [], voice: [], replacedOnly: false };
const rows = soundRows(assetItems(soundCatalog, []));
const names = (list: typeof rows) => list.map((r) => r.name);

describe("sound rows", () => {
  it("folds variations into one row and orders groups abilities first", () => {
    expect(rows).toHaveLength(8);
    const fire = rows.find((r) => r.name === "Assassinate · fire")!;
    expect(fire.items.map((i) => i.entry.sound?.variation)).toEqual([1, 2, 3, 4, 5]);
    expect(fire.fromEvent).toBe(true);
    expect(rows.find((r) => r.name === "Careful Haze")!.items).toHaveLength(2);
    expect(rows.map((r) => [r.groupName, r.name])).toEqual([
      ["Assassinate", "Assassinate · fire"],
      ["Assassinate", "Assassinate · scope in"],
      ["Flight", "Flight · cast"],
      ["Reload", "Reload · start"],
      ["Ability voice lines", "Uses ability 4"],
      ["Pings", "Careful Haze"],
      ["Lines from Lady Geist", "Kill Vindicta"],
      ["Footsteps", "Footstep"],
    ]);
  });
  it("keeps one row per file when folding is off (choosing a destination)", () => {
    expect(soundRows(assetItems(soundCatalog, []), false)).toHaveLength(soundCatalog.length);
  });
  it("falls back to the label for entries without sound info", () => {
    const [row] = soundRows(assetItems([{ path: "sounds/x/voice.vsnd_c", target: "sounds/x/voice.vsnd_c", label: "Voice line", supported: true }], []));
    expect(row.name).toBe("Voice line");
    expect(row.tags).toEqual(["other"]);
  });
});

describe("sound filters", () => {
  it("one tag, several tags (all of them) and voice sub-kinds (any of them)", () => {
    expect(names(filterRows(rows, { ...none, tags: ["weapon"] }))).toEqual(["Reload · start"]);
    expect(names(filterRows(rows, { ...none, tags: ["hero", "ability"] }))).toEqual([
      "Assassinate · fire", "Assassinate · scope in", "Flight · cast", "Uses ability 4",
    ]);
    expect(names(filterRows(rows, { ...none, tags: ["ability", "voice"] }))).toEqual(["Uses ability 4"]);
    expect(names(filterRows(rows, { ...none, tags: ["voice"], voice: ["ping", "about"] }))).toEqual([
      "Careful Haze", "Kill Vindicta",
    ]);
    // Sub-kinds only apply with Voice chosen.
    expect(filterRows(rows, { ...none, tags: ["hero"], voice: ["ping"] })).toHaveLength(7);
    expect(names(filterRows(rows, { ...none, tags: ["other"] }))).toEqual(["Footstep"]);
  });
  it("searches names, file names, tag names, voice kinds and event names", () => {
    expect(names(filterRows(rows, { ...none, query: "weapon" }))).toEqual(["Reload · start"]);
    expect(names(filterRows(rows, { ...none, query: "fire_03" }))).toEqual(["Assassinate · fire"]);
    expect(names(filterRows(rows, { ...none, query: "leap" }))).toEqual(["Flight · cast"]);
    expect(names(filterRows(rows, { ...none, query: "pings haze" }))).toEqual(["Careful Haze"]);
    expect(names(filterRows(rows, { ...none, query: "about this hero" }))).toEqual(["Kill Vindicta"]);
  });
  it("counts rows per tag for the current search, not the chosen tags", () => {
    const { tags, voice } = tagCounts(rows, { ...none, tags: ["voice"] });
    expect(Object.fromEntries(tags)).toEqual({ hero: 7, ability: 4, weapon: 1, voice: 3, other: 1 });
    expect(Object.fromEntries(voice)).toEqual({ ping: 1, ability: 1, about: 1, kill: 1 });
    expect(tagCounts(rows, { ...none, query: "assassinate" }).tags.get("hero")).toBe(2);
  });
  it("replaced only covers families with any replaced variation", () => {
    const saved: ResourceReplacement[] = [{ target: soundCatalog[1].target, source: "a.wav", enabled: true }];
    const replaced = filterRows(soundRows(assetItems(soundCatalog, saved)), { ...none, replacedOnly: true });
    expect(names(replaced)).toEqual(["Assassinate · fire"]);
  });
  it("says what is shown", () => {
    expect(shownText(4, 8, { ...none, tags: ["hero", "ability"] })).toBe(
      "Showing 4 sounds that are both Hero and Ability · 8 files with their variations",
    );
    expect(shownText(1, 1, { ...none, tags: ["voice"], voice: ["ping"] })).toBe("Showing 1 sound tagged Voice (Pings)");
  });
});

describe("replace all variations", () => {
  const fire = () => soundRows(assetItems(soundCatalog, saved0)).find((r) => r.name === "Assassinate · fire")!;
  const saved0: ResourceReplacement[] = [
    { enabled: false, target: soundCatalog[1].target, source: "old.wav", volume_percent: 150, audio_fit: "trim", allow_shared: false },
  ];
  it("sets every variation it may and says why it left the others", () => {
    const result = replaceAll(saved0, fire().items, "C:\\new.wav", false);
    expect(result.set.map((i) => i.entry.sound?.variation)).toEqual([1, 2, 4]);
    expect(result.refused.map((r) => [r.item.entry.sound?.variation, r.reason.split(".")[0]])).toEqual([
      [3, "Shared with other heroes"],
      [5, "View only: Unsupported sound codec"],
    ]);
    // The existing row keeps its own choices; new rows look like a single replacement.
    expect(result.rows[0]).toEqual({ ...saved0[0], source: "C:\\new.wav" });
    expect(result.rows.slice(1)).toEqual([
      { enabled: true, source: "C:\\new.wav", target: soundCatalog[0].target, allow_shared: false, original_sha256: "sha-vindicta_assassinate_fire_01", volume_percent: 100, audio_fit: "pad" },
      { enabled: true, source: "C:\\new.wav", target: soundCatalog[3].target, allow_shared: false, original_sha256: "sha-vindicta_assassinate_fire_04", volume_percent: 100, audio_fit: "pad" },
    ]);
    const shared = replaceAll(saved0, fire().items, "C:\\new.wav", true);
    expect(shared.set).toHaveLength(4);
    expect(shared.rows.find((r) => r.target === soundCatalog[2].target)?.allow_shared).toBe(true);
  });
  it("respects the replacement limit, compiled sources and WAV support", () => {
    const full = Array.from({ length: MAX_REPLACEMENTS - 1 }, (_, i) => ({ target: `sounds/other/${i}.vsnd_c`, source: "x.wav" }));
    const items = soundRows(assetItems(soundCatalog, full)).find((r) => r.name === "Assassinate · fire")!.items;
    const result = replaceAll(full, items, "a.wav", true);
    expect(result.set).toHaveLength(1);
    expect(result.rows).toHaveLength(MAX_REPLACEMENTS);
    expect(result.refused.filter((r) => r.reason.includes("128 replacements"))).toHaveLength(3);
    expect(replaceAll([], items, "a.vsnd_c", true).set).toHaveLength(0);
    const noWav = soundRows(assetItems([soundEntry("sounds/a/x_01.vsnd_c", { name: "X", group: "other:", group_name: "X", tags: [] }, { wav_supported: false })], []))[0].items;
    expect(replaceAll([], noWav, "a.wav", false).refused[0].reason).toMatch(/cannot take a converted file/);
  });
});

describe("silenced sounds", () => {
  const fire = () => soundRows(assetItems(soundCatalog, [])).find((r) => r.name === "Assassinate · fire")!;
  it("count as changes in the row state, alone or mixed with replacements", () => {
    const target = (n: number) => soundCatalog[n].target;
    const state = (saved: ResourceReplacement[]) =>
      rowStatus(soundRows(assetItems(soundCatalog, saved)).find((r) => r.name === "Assassinate · fire")!, true);
    expect(state([{ target: target(0), silence: true }])).toBe("1 of 5 silenced");
    expect(state([{ target: target(0), silence: true }, { target: target(1), source: "a.wav" }])).toBe("2 of 5 changed");
    expect(state([{ target: target(0), source: "a.wav" }])).toBe("1 of 5 replaced");
    const flight = soundRows(assetItems(soundCatalog, [{ target: target(6), silence: true }])).find((r) => r.name === "Flight · cast")!;
    expect(rowStatus(flight, true)).toBe("Silenced");
    expect(rowStatus(flight, false)).toBe("Paused");
    for (const s of ["Replaced", "Silenced", "Changed", "1 of 5 silenced", "2 of 5 changed", "1 of 5 replaced"]) expect(changedStatus(s)).toBe(true);
    for (const s of ["Original", "Paused", "Shared", "View only", "Needs attention"]) expect(changedStatus(s)).toBe(false);
  });
  it("switch between a file and silence without leaving the other's keys", () => {
    const replacement: ResourceReplacement = { enabled: false, target: "t", source: "a.wav", allow_shared: true, volume_percent: 150, audio_fit: "trim", original_sha256: "h" };
    const silent = silencedRow(replacement);
    expect(silent).toEqual({ enabled: false, target: "t", allow_shared: true, original_sha256: "h", silence: true });
    expect(withSource(silent, "b.wav")).toEqual({ enabled: false, target: "t", allow_shared: true, original_sha256: "h", volume_percent: 100, audio_fit: "pad", source: "b.wav" });
    // A replacement given another file is unchanged otherwise (as before).
    expect(withSource(replacement, "b.wav")).toEqual({ ...replacement, source: "b.wav" });
  });
  it("silence all follows every replace-all rule", () => {
    const result = silenceAll([], fire().items, false);
    expect(result.set.map((i) => i.entry.target)).toEqual([soundCatalog[0].target, soundCatalog[1].target, soundCatalog[3].target]);
    expect(result.rows.every((r) => r.silence === true && !("source" in r))).toBe(true);
    expect(result.refused.map((r) => r.reason)).toEqual([
      expect.stringMatching(/Also silence the shared variations/),
      expect.stringMatching(/^View only: Unsupported sound codec/),
    ]);
    expect(silenceAll([], fire().items, true).rows.find((r) => r.target === soundCatalog[2].target)?.allow_shared).toBe(true);
    const full: ResourceReplacement[] = Array.from({ length: MAX_REPLACEMENTS }, (_, i) => ({ target: `x/${i}`, source: "a.wav" }));
    expect(silenceAll(full, fire().items, true).set).toHaveLength(0);
    const noWav = soundRows(assetItems([soundEntry("sounds/a/x_01.vsnd_c", { name: "X", group: "other:", group_name: "X", tags: [] }, { wav_supported: false })], []))[0].items;
    expect(silenceAll([], noWav, false).refused[0].reason).toMatch(/cannot be silenced/);
    expect(silenceRefusal(noWav[0])).toMatch(/cannot be silenced/);
    expect(newSilencedRow(fire().items[0])).toEqual({ enabled: true, target: soundCatalog[0].target, allow_shared: false, original_sha256: soundCatalog[0].original_sha256, silence: true });
  });
});

describe("a silenced row an older app gave a file", () => {
  it("is a replacement: the file wins", () => {
    const both: ResourceReplacement = { target: soundCatalog[6].target, silence: true, source: "a.wav" };
    expect(isSilenced(both)).toBe(false);
    expect(isSilenced({ target: "t", silence: true })).toBe(true);
    const flight = soundRows(assetItems(soundCatalog, [both])).find((r) => r.name === "Flight · cast")!;
    expect(rowStatus(flight, true)).toBe("Replaced");
  });
});
