import { describe, expect, it } from "vitest";
import type { HeroMaterialEntry } from "../components/AssetCatalog";
import { adjustPixels } from "../preview/colorAdjust";
import { hsv, rangeWeight, recolorPixels, withDefaults } from "./heroColorRanges";
import { applyRecipe, approximatePixels, areaAverage, containBox, containPixel, flatImage, imagePoint, isPlain, recipeColor, recipeFor, selectionPixels, uvAverage, uvPixel, zeroUvs } from "./colorPixels";

const entry = { albedo: [1, 1.25, 0.8] } as HeroMaterialEntry;
const sample = () => new Uint8ClampedArray([220, 60, 40, 255, 40, 90, 200, 9, 128, 128, 128, 77]);

describe("Colors preview recipes", () => {
  it("replacement: exact, color ranges first, then hue, saturation and brightness", () => {
    const values = { hue: 40, saturation: 0.6, brightness: 1.3, ranges: [{ hue: 0, hue_width: 30, hue_shift: 120 }, { enabled: false, hue_shift: 50 }] };
    const recipe = recipeFor("replacement", "yours", values);
    expect(recipe).toEqual({ ranges: [withDefaults(values.ranges[0])], adjust: { hue: 40, saturation: 0.6, brightness: 1.3 }, approximate: null });
    const data = sample(), expected = sample();
    recolorPixels(recipe.ranges, expected);
    adjustPixels(expected, recipe.adjust!);
    applyRecipe(data, recipe);
    expect([...data]).toEqual([...expected]);
    expect([data[3], data[7], data[11]]).toEqual([255, 9, 77]);
    // The original view and an unchanged material draw the image as it is.
    expect(isPlain(recipeFor("replacement", "original", values))).toBe(true);
    expect(isPlain(recipeFor("replacement", "yours", null))).toBe(true);
  });
  it("game hero: exact ranges, then the game's saturation and brightness approximation (in both views)", () => {
    const values = { saturation: 0.5, ranges: [{ hue_shift: 90 }] };
    expect(recipeFor("hero", "original", values, entry)).toEqual({ ranges: [], adjust: null, approximate: { saturate: 1.25, brightness: 0.8 } });
    expect(recipeFor("hero", "yours", values, entry)).toEqual({ ranges: [withDefaults({ hue_shift: 90 })], adjust: null, approximate: { saturate: 0.625, brightness: 0.8 } });
    expect(isPlain(recipeFor("hero", "yours", null, { albedo: null } as HeroMaterialEntry))).toBe(true);
  });
  it("maps what a range selects for the glow, in the same pass as the recolor or alone", () => {
    const select = withDefaults({ hue: 0, hue_width: 20 });
    const values = { ranges: [{ hue: 0, hue_width: 20, hue_shift: 120 }] };
    const recipe = recipeFor("replacement", "yours", values);
    const together = sample(), alone = sample(), plain = sample();
    const out = new Uint8ClampedArray(together.length), map = new Uint8ClampedArray(together.length);
    applyRecipe(together, recipe, { range: select, out, width: 3, step: 1 });
    applyRecipe(plain, recipe);
    selectionPixels({ range: select, out: map, width: 3, step: 1 }, alone);
    // The colors are exactly the plain recolor; the map is the same either way.
    expect([...together]).toEqual([...plain]);
    expect([...out]).toEqual([...map]);
    // The red pixel is selected (white), the blue one is not (black), alpha opaque.
    expect([map[0], map[1], map[2], map[3]]).toEqual([255, 255, 255, 255]);
    expect([map[4], map[7]]).toEqual([0, 255]);
    expect(map[8]).toBe(Math.round(rangeWeight(select, hsv(128 / 255, 128 / 255, 128 / 255)) * 255));
    // At step 2 a 3 × 1 image maps to 2 × 1: pixels 0 and 2.
    const half = new Uint8ClampedArray(8);
    selectionPixels({ range: select, out: half, width: 3, step: 2 }, sample());
    expect([...half]).toEqual([255, 255, 255, 255, map[8], map[8], map[8], 255]);
  });
  it("approximates CSS saturate() and brightness()", () => {
    const grey = new Uint8ClampedArray([200, 100, 50, 255]);
    approximatePixels(grey, 0, 1);
    // saturate(0) is the luminance weights of the Filter Effects matrix.
    expect([...grey]).toEqual(Array(3).fill(Math.round(0.213 * 200 + 0.715 * 100 + 0.072 * 50)).concat(255));
    const same = new Uint8ClampedArray([200, 100, 50, 255]);
    approximatePixels(same, 1, 0.5);
    expect([...same]).toEqual([100, 50, 25, 255]);
  });
  it("changes a plain base color as the build changes it", () => {
    expect(recipeColor([1, 0, 0], recipeFor("replacement", "yours", { hue: 120 }))).toEqual([0, 1, 0]);
    expect(recipeColor([0.5, 0.5, 0.5], recipeFor("replacement", "yours", null))).toEqual([128 / 255, 128 / 255, 128 / 255]);
  });
});

describe("Colors picking", () => {
  it("maps a click on a contained image and a texture coordinate to a pixel", () => {
    // A 100x50 image in a 200x200 box is scaled 2x and centred vertically.
    const box = { left: 0, top: 0, width: 200, height: 200 };
    expect(containPixel(box, 0, 50, 100, 50)).toEqual({ x: 0, y: 0 });
    expect(containPixel(box, 199, 149, 100, 50)).toEqual({ x: 99, y: 49 });
    expect(containPixel(box, 100, 20, 100, 50)).toBeNull();
    // Repeat wrapping; v = 0 is the image's top row (textures are not flipped).
    expect(uvPixel(0.25, 0, 64, 32)).toEqual({ x: 16, y: 0 });
    expect(uvPixel(1.25, -0.5, 64, 32)).toEqual({ x: 16, y: 16 });
    expect(uvPixel(1, 1, 64, 32)).toEqual({ x: 0, y: 0 });
    expect(uvPixel(0.9999, 0.9999, 64, 32)).toEqual({ x: 63, y: 31 });
  });
  it("knows a part whose texture coordinates are all zero", () => {
    expect(zeroUvs(new Float32Array([0, 0, 0, 0]))).toBe(true);
    expect(zeroUvs(new Float32Array([0, 0, 0.5, 0]))).toBe(false);
    expect(zeroUvs(null)).toBe(true);
  });
});

describe("Colors picking: the pixel under the pointer on the Texture view", () => {
  const pixel = (stage: { left: number; top: number; width: number; height: number }, x: number, y: number, size = 512) => {
    const point = imagePoint(containBox(stage, size, size), x, y, size, size);
    return point && { x: Math.floor(point.x), y: Math.floor(point.y) };
  };
  it("reads the stacked layout (a stage wider than tall) against the image fitted whole inside it", () => {
    // Game and Yours stacked: a 400 × 244 stage; the 512 px image is drawn 244 × 244 in its middle.
    const stage = { left: 20, top: 100, width: 400, height: 244 };
    expect(containBox(stage, 512, 512)).toEqual({ left: 98, top: 100, width: 244, height: 244 });
    expect(pixel(stage, 98, 100)).toEqual({ x: 0, y: 0 });
    expect(pixel(stage, 220, 222)).toEqual({ x: 256, y: 256 });
    expect(pixel(stage, 341.9, 343.9)).toEqual({ x: 511, y: 511 });
    // Beside the drawn image (the stage's empty sides) nothing is picked.
    expect(pixel(stage, 60, 200)).toBeNull();
    expect(pixel(stage, 380, 200)).toBeNull();
  });
  it("reads the side-by-side layout (a stage taller than wide) the same way", () => {
    const stage = { left: 0, top: 0, width: 300, height: 500 };
    expect(containBox(stage, 512, 512)).toEqual({ left: 0, top: 100, width: 300, height: 300 });
    expect(pixel(stage, 0, 100)).toEqual({ x: 0, y: 0 });
    expect(pixel(stage, 150, 250)).toEqual({ x: 256, y: 256 });
    expect(pixel(stage, 150, 50)).toBeNull();
    expect(pixel(stage, 150, 420)).toBeNull();
    // A wide image in a tall stage: centred vertically.
    expect(containBox(stage, 1024, 512)).toEqual({ left: 0, top: 175, width: 300, height: 150 });
  });
  it("draws the selection outline in the image's own box at the live size", () => {
    // The outline is drawn at the live size (at most 512 px) over the full-size image: the same
    // aspect fits to the same box, in both layouts.
    for (const stage of [{ left: 20, top: 100, width: 400, height: 244 }, { left: 0, top: 0, width: 300, height: 500 }]) {
      expect(containBox(stage, 512, 512)).toEqual(containBox(stage, 2048, 2048));
      expect(containBox(stage, 512, 256)).toEqual(containBox(stage, 1024, 512));
    }
  });
});

describe("Colors picking: the color of the area under the pointer", () => {
  // 4 × 2: the left half red, the right half blue, the bottom-right pixel see-through.
  const data = new Uint8ClampedArray([
    200, 0, 0, 255, 200, 0, 0, 255, 0, 0, 200, 255, 0, 0, 200, 255,
    200, 0, 0, 255, 200, 0, 0, 255, 0, 0, 200, 255, 9, 9, 9, 0,
  ]);
  const image = { data, width: 4, height: 2 };
  it("averages the pixels around a point, cut at the edges, leaving out see-through pixels", () => {
    expect(areaAverage(image, 0.5, 0.5, 0.2)).toEqual([200, 0, 0]);
    expect(areaAverage(image, 2, 1, 0.5)).toEqual([100, 0, 100]);
    expect(areaAverage(image, 3.5, 1.5, 0.6)).toEqual([0, 0, 200]);
    expect(areaAverage(image, 3.5, 1.5, 0)).toEqual([9, 9, 9]);
  });
  it("averages the pixels a list of texture coordinates shows", () => {
    expect(uvAverage(image, [{ u: 0.1, v: 0.1 }])).toEqual([200, 0, 0]);
    expect(uvAverage(image, [{ u: 0.1, v: 0.1 }, { u: 0.1, v: 0.9 }, { u: 0.6, v: 0.1 }])).toEqual([133, 0, 67]);
  });
  it("knows an image of one flat color", () => {
    expect(flatImage({ data: new Uint8ClampedArray([250, 250, 250, 255]), width: 1, height: 1 })).toBe(true);
    expect(flatImage({ data: new Uint8ClampedArray([40, 90, 30, 255, 41, 89, 32, 255, 0, 0, 0, 0]), width: 3, height: 1 })).toBe(true);
    expect(flatImage(image)).toBe(false);
  });
});
