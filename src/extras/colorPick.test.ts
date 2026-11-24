import { describe, expect, it } from "vitest";
import { hsv, rangeWeight, recolorTexel, withDefaults } from "./heroColorRanges";
import { changeToward, colorName, givenChange, greySelectionAround, huelessPick, pickRange, rangeChanges, rangeName, selectionAround, tintHidesRanges, type Bytes } from "./colorPick";
import type { Rgb } from "./colorAdapter";

const unit = (b: Bytes): Rgb => [b[0] / 255, b[1] / 255, b[2] / 255];
const bytes = (c: Rgb) => c.map((v) => Math.round(v * 255));
/** Applies the saved ranges to one pixel exactly as the build does. */
const apply = (ranges: Parameters<typeof withDefaults>[0][], pixel: Bytes) => recolorTexel(ranges.map(withDefaults), [...pixel, 255]).slice(0, 3);
const close = (a: number[], b: number[], within = 2) => a.every((v, i) => Math.abs(v - b[i]) <= within);

describe("Recolor by picking: the range maths", () => {
  it("selects around the picked pixel, which it always selects fully", () => {
    const blue: Bytes = [61, 111, 214];
    const s = selectionAround(blue);
    const [h, sat, v] = hsv(...unit(blue));
    expect(s.hue).toBeCloseTo(h, 1);
    expect(s.hue_width).toBe(20);
    expect(s.softness).toBe(0.25);
    expect(s.saturation_range[0]).toBeLessThanOrEqual(sat);
    expect(s.saturation_range[1]).toBeGreaterThanOrEqual(sat);
    expect(s.saturation_range[0]).toBeGreaterThanOrEqual(0.1);
    expect(s.brightness_range[0]).toBeLessThanOrEqual(v);
    expect(s.brightness_range[1]).toBeGreaterThanOrEqual(v);
    expect(rangeWeight(withDefaults(s), hsv(...unit(blue)))).toBe(1);
    // Grey and white stay out of a colored pick.
    expect(rangeWeight(withDefaults(s), hsv(0.8, 0.8, 0.8))).toBe(0);
  });
  it("turns the picked color into the chosen one within rounding", () => {
    const cases: [Bytes, Rgb][] = [
      [[61, 111, 214], [226 / 255, 178 / 255, 58 / 255]], // blue → gold
      [[200, 50, 58], [40 / 255, 200 / 255, 90 / 255]], // red → green
      [[120, 20, 160], [70 / 255, 10 / 255, 90 / 255]], // purple → darker, paler
      [[60, 140, 60], [0.5, 0.5, 0.5]], // green → grey (saturation 0)
      [[60, 140, 60], [0, 0, 0]], // green → black (brightness 0)
      [[230, 120, 30], [1, 1, 1]], // orange → white (brightness ≤ 2×, saturation 0)
    ];
    for (const [picked, chosen] of cases) {
      const { range, result, limits, otherRange } = pickRange(picked, chosen, [], null);
      expect(limits).toEqual([]);
      expect(otherRange).toBe(false);
      expect(close(apply([range], picked), bytes(chosen))).toBe(true);
      expect(close(bytes(result), bytes(chosen))).toBe(true);
      // Every value stays within what the build accepts.
      expect(range.hue_shift!).toBeGreaterThanOrEqual(-180);
      expect(range.hue_shift!).toBeLessThanOrEqual(180);
      for (const f of [range.saturation!, range.brightness!]) { expect(f).toBeGreaterThanOrEqual(0); expect(f).toBeLessThanOrEqual(2); }
      expect(range.hue!).toBeGreaterThanOrEqual(0);
      expect(range.hue!).toBeLessThan(360);
    }
  });
  it("says when the chosen color is out of reach and gets as close as it can", () => {
    // A dark, pale blue cannot become a bright, strong yellow: at most twice as bright and saturated.
    const { range, result, limits } = pickRange([40, 45, 60], [1, 0.9, 0], [], null);
    expect(limits).toEqual(["saturation", "brightness"]);
    expect(range.saturation).toBe(2);
    expect(range.brightness).toBe(2);
    const [, s, v] = hsv(...result);
    const [, s0, v0] = hsv(40 / 255, 45 / 255, 60 / 255);
    expect(s).toBeCloseTo(Math.min(1, s0 * 2), 2);
    expect(v).toBeCloseTo(v0 * 2, 2);
    expect(changeToward([0.5, 0.2, 0.2], [0.5, 0.2, 0.2])).toMatchObject({ hue_shift: 0, saturation: 1, brightness: 1, limits: [] });
  });
  it("refuses grey, white and black picks: they have no hue to select or shift", () => {
    expect(huelessPick([128, 128, 128])).toBe("grey");
    expect(huelessPick([250, 248, 246])).toBe("grey");
    expect(huelessPick([8, 4, 10])).toBe("black");
    expect(huelessPick([61, 111, 214])).toBeNull();
  });
  it("starts from what earlier ranges made of the color and notes a later range that changes it", () => {
    const red: Bytes = [200, 40, 40];
    const earlier = { hue: 0, hue_width: 30, hue_shift: 120 }; // red → green first
    const chosen: Rgb = [0.2, 0.3, 0.9];
    const after = pickRange(red, chosen, [earlier], null);
    expect(close(apply([earlier, after.range], red), bytes(chosen))).toBe(true);
    expect(after.otherRange).toBe(false);
    // Picked again for a first range, the red → green range after it changes the result again.
    const before = pickRange(red, chosen, [{ hue: 200, hue_width: 10, hue_shift: 30 }, earlier], 0);
    expect(before.otherRange).toBe(true);
  });
  it("gives a grey, white or black pick the chosen color (switch set_colour), keeping its shading", () => {
    const violet: Rgb = [0.48, 0.3, 0.88];
    for (const picked of [[200, 200, 200], [250, 248, 246], [96, 96, 98]] as Bytes[]) {
      const { range, result, limits } = pickRange(picked, violet, [], null);
      expect(limits).toEqual([]);
      expect(range).toMatchObject({ hue: 0, hue_width: 180, softness: 0.25, hue_shift: 0, saturation: 1 });
      expect(range.saturation_range![0]).toBe(0);
      expect(range.saturation_range![1]).toBeLessThanOrEqual(0.2);
      expect(rangeWeight(withDefaults(range), hsv(...unit(picked)))).toBe(1);
      expect(close(bytes(result), bytes(violet), 3)).toBe(true);
      // A colored pixel of the same brightness stays out.
      expect(rangeWeight(withDefaults(range), hsv(0.8, 0.2, 0.2))).toBe(0);
    }
    // Light grey and dark grey of one material are told apart by brightness.
    const light = pickRange([210, 210, 210], violet, [], null).range;
    expect(rangeWeight(withDefaults(light), hsv(60 / 255, 60 / 255, 60 / 255))).toBe(0);
    // Black: every saturation, a brightness ceiling, factor 1 and a lift up to the chosen color.
    const black = pickRange([8, 4, 10], [0.5, 0.1, 0.1], [], null);
    expect(black.range).toMatchObject({ saturation_range: [0, 1], brightness_range: [0, 0.19], brightness: 1 });
    expect(black.range.lift).toBeCloseTo(0.5 - 10 / 255, 3);
    expect(close(apply([black.range], [8, 4, 10]), [128, 26, 26], 3)).toBe(true);
    expect(greySelectionAround([8, 4, 10]).saturation_range).toEqual([0, 1]);
    // Twice as bright and more: the factor stops at 2 and the lift does the rest.
    expect(givenChange([0.2, 0.2, 0.2], [0.9, 0.9, 1])).toMatchObject({ brightness: 2, lift: 0.6, limits: [] });
    expect(givenChange([0.5, 0.5, 0.5], [0.5, 0.25, 0.25])).toEqual({ set_hue: 0, set_saturation: 0.5, brightness: 1, limits: [] });
  });
  it("keeps a range's selection when only its new color changes", () => {
    const old = { enabled: true, hue: 220, hue_width: 32, saturation_range: [0.35, 1] as [number, number], brightness_range: [0.2, 1] as [number, number], softness: 0.2, hue_shift: 10, saturation: 1, brightness: 1 };
    const { range } = pickRange([61, 111, 214], [1, 0, 0], [old], 0, true);
    expect(range).toMatchObject({ hue: 220, hue_width: 32, saturation_range: [0.35, 1], brightness_range: [0.2, 1], softness: 0.2 });
    expect(close(apply([range], [61, 111, 214]), [255, 0, 0], 3)).toBe(true);
  });
});

describe("Colors: names and warnings", () => {
  it("names colors and ranges plainly", () => {
    expect(colorName([61 / 255, 111 / 255, 214 / 255])).toBe("blue");
    expect(colorName([226 / 255, 178 / 255, 58 / 255])).toBe("gold");
    expect(colorName([0.5, 0.5, 0.5])).toBe("grey");
    expect(colorName([0.02, 0.02, 0.02])).toBe("black");
    expect(rangeName([61 / 255, 111 / 255, 214 / 255], [226 / 255, 178 / 255, 58 / 255], true)).toBe("Blue → gold");
    expect(rangeName([0.8, 0.1, 0.1], [0.5, 0.06, 0.06], true)).toBe("Red → darker red");
    expect(rangeName([0.8, 0.1, 0.1], [0.8, 0.1, 0.1], false)).toBe("Red · no change yet");
    expect(rangeChanges({ hue_shift: 0 })).toBe(false);
    expect(rangeChanges({ brightness: 0.5 })).toBe(true);
  });
  it("warns about a dark tint only when a color range is in use", () => {
    const range = [{ hue_shift: 40 }];
    expect(tintHidesRanges([237 / 255, 59 / 255, 59 / 255], range)).toBe(true); // #ED3B3B
    expect(tintHidesRanges([0, 0, 0], range)).toBe(true);
    expect(tintHidesRanges([0.9, 0.8, 0.7], range)).toBe(false);
    expect(tintHidesRanges([237 / 255, 59 / 255, 59 / 255], [])).toBe(false);
    expect(tintHidesRanges([237 / 255, 59 / 255, 59 / 255], [{ enabled: false, hue_shift: 40 }])).toBe(false);
    expect(tintHidesRanges(undefined, range)).toBe(false);
  });
});
