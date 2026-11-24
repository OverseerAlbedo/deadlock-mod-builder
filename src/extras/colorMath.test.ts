import { beforeEach, describe, expect, it, vi } from "vitest";
import { fromHsb, keepHsb, parseHex, recentColors, rememberColor, RECENT_KEY, sameColor, toHex, toHsb } from "./colorMath";
import type { Rgb } from "./colorAdapter";

describe("Color picker arithmetic", () => {
  it("reads and writes hex colors", () => {
    expect(parseHex("#5600D8")).toEqual([0x56 / 255, 0, 0xd8 / 255]);
    expect(parseHex("5600d8")).toEqual(parseHex("#5600D8"));
    expect(parseHex(" #f80 ")).toEqual([1, 0x88 / 255, 0]);
    for (const bad of ["", "#12345", "#ggg000", "red", "#1234567"]) expect(parseHex(bad)).toBeNull();
    expect(toHex([0x56 / 255, 0, 0xd8 / 255])).toBe("#5600D8");
    expect(toHex([1.4, -0.2, 0.5])).toBe("#FF0080");
  });
  it("converts between RGB and HSB both ways at 8-bit precision", () => {
    expect(toHsb([1, 0, 0])).toEqual([0, 1, 1]);
    expect(fromHsb([120, 1, 0.5])).toEqual([0, 0.5, 0]);
    expect(fromHsb([360, 1, 1])).toEqual([1, 0, 0]);
    for (let r = 0; r < 256; r += 17) for (let g = 0; g < 256; g += 51) for (let b = 0; b < 256; b += 85) {
      const color: Rgb = [r / 255, g / 255, b / 255];
      expect(sameColor(fromHsb(toHsb(color)), color), toHex(color)).toBe(true);
      expect(parseHex(toHex(color))).toEqual(color);
    }
  });
  it("keeps the hue and saturation a black or grey color no longer tells", () => {
    const purple: [number, number, number] = [264, 1, 0.85];
    expect(keepHsb([0, 0, 0], purple)).toEqual([264, 1, 0]);
    expect(keepHsb([0.5, 0.5, 0.5], purple)).toEqual([264, 0, 0.5]);
    // The same color as the HSB values: they stay exactly as they are.
    expect(keepHsb(fromHsb(purple), purple)).toBe(purple);
    expect(keepHsb([1, 0, 0], purple)).toEqual([0, 1, 1]);
  });
});

describe("Recent colors", () => {
  beforeEach(() => localStorage.clear());
  it("keeps the last twelve tints, newest first, each once", () => {
    for (let i = 0; i < 14; i++) rememberColor([i / 20, 0, 0]);
    rememberColor([2 / 20, 0, 0]);
    const list = recentColors();
    expect(list).toHaveLength(12);
    expect(list[0]).toBe(toHex([2 / 20, 0, 0]));
    expect(new Set(list).size).toBe(12);
    expect(JSON.parse(localStorage.getItem(RECENT_KEY)!)).toEqual(list);
  });
  it("is safe without storage or with damaged data", () => {
    localStorage.setItem(RECENT_KEY, "{not json");
    expect(recentColors()).toEqual([]);
    localStorage.setItem(RECENT_KEY, JSON.stringify(["#FFFFFF", 4, "nope"]));
    expect(recentColors()).toEqual(["#FFFFFF"]);
    const get = vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => { throw new Error("blocked"); });
    const set = vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => { throw new Error("blocked"); });
    expect(recentColors()).toEqual([]);
    expect(rememberColor([1, 0, 0])).toEqual(["#FF0000"]);
    get.mockRestore();
    set.mockRestore();
  });
});
