import { describe, expect, it } from "vitest";
import { cachedCoverage, nothingReason, percentText, rangeCoverage } from "./colorCoverage";

/** 10 pixels: 3 blue, 1 dark blue, 4 red, 2 grey. */
function image() {
  const px = [[61, 111, 214], [61, 111, 214], [61, 111, 214], [20, 35, 70], [200, 50, 58], [200, 50, 58], [200, 50, 58], [200, 50, 58], [128, 128, 128], [240, 240, 240]];
  return { data: new Uint8ClampedArray(px.flatMap((p) => [...p, 255])), width: 10, height: 1 };
}
const blue = { hue: 220, hue_width: 20, saturation_range: [0.36, 1] as [number, number], brightness_range: [0.39, 1] as [number, number], softness: 0 };

describe("Colors: what a range selects", () => {
  it("measures the share of the image, the mean color and the hue band", () => {
    const img = image();
    const coverage = rangeCoverage(blue, img);
    expect(coverage.fraction).toBeCloseTo(0.3, 5);
    expect(coverage.mean!.map((v) => Math.round(v * 255))).toEqual([61, 111, 214]);
    // The dark blue is in the hue band but darker than the brightness range.
    expect(coverage.hueFraction).toBeCloseTo(0.4, 5);
    expect(percentText(coverage.fraction)).toBe("30%");
    expect(nothingReason(coverage)).toBeNull();
    // Cached per image and selection: a shift does not count.
    expect(cachedCoverage(img, { ...blue, hue_shift: 90 })).toBe(coverage);
  });
  it("says why a range selects nothing", () => {
    const img = image();
    const green = rangeCoverage({ ...blue, hue: 120 }, img);
    expect(green.fraction).toBe(0);
    expect(green.mean).toBeNull();
    expect(nothingReason(green)).toMatch(/no color in that hue: grey and white areas have no hue to select/);
    const pale = rangeCoverage({ ...blue, saturation_range: [0, 0.05] }, img);
    expect(nothingReason(pale)).toMatch(/paler, darker or brighter/);
    expect(percentText(0.004)).toBe("<1%");
    expect(percentText(0)).toBe("0%");
  });
});
