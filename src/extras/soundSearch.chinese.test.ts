import { act } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { MIRROR_KEY, setLanguage } from "../i18n";
import { assetItems } from "./models";
import { soundCatalog } from "./soundFixtures";
import { filterRows, soundRows, tagCounts, type SoundFilter } from "./soundTags";

// The Sounds search in Chinese: the tag and voice-kind names the chips show match, English
// names keep matching, and file and sound names are matched as they are.
const none: SoundFilter = { query: "", tags: [], voice: [], replacedOnly: false };
const rows = soundRows(assetItems(soundCatalog, []));
const names = (list: typeof rows) => list.map((r) => r.name);

beforeEach(async () => { await act(() => setLanguage("zh-CN")); });
afterEach(async () => { await act(() => setLanguage("en")); localStorage.removeItem(MIRROR_KEY); });

describe("Sounds search in Chinese", () => {
  it("finds rows by the Chinese names of their tags and voice kinds", () => {
    expect(names(filterRows(rows, { ...none, query: "语音" }))).toEqual(names(filterRows(rows, { ...none, tags: ["voice"] })));
    expect(names(filterRows(rows, { ...none, query: "武器" }))).toEqual(["Reload · start"]);
    expect(names(filterRows(rows, { ...none, query: "标记" }))).toEqual(["Careful Haze"]);
    // A Chinese word and a word of the sound's name together.
    expect(names(filterRows(rows, { ...none, query: "标记 haze" }))).toEqual(["Careful Haze"]);
    expect(tagCounts(rows, { ...none, query: "语音" }).tags.get("voice")).toBe(3);
  });
  it("keeps matching the English names, file names and sound names", () => {
    expect(names(filterRows(rows, { ...none, query: "weapon" }))).toEqual(["Reload · start"]);
    expect(names(filterRows(rows, { ...none, query: "fire_03" }))).toEqual(["Assassinate · fire"]);
    expect(names(filterRows(rows, { ...none, query: "pings haze" }))).toEqual(["Careful Haze"]);
    expect(names(filterRows(rows, { ...none, query: "语音 nothing-like-this" }))).toEqual([]);
  });
  it("matches only English names again after switching back", async () => {
    await act(() => setLanguage("en"));
    expect(filterRows(rows, { ...none, query: "语音" })).toEqual([]);
    expect(names(filterRows(rows, { ...none, query: "weapon" }))).toEqual(["Reload · start"]);
  });
});
