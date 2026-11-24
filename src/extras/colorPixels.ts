/** What the Colors preview draws for one material, from its original base-color pixels.
 * - Replacement model: exact, as the build: color ranges first (`recolorPixels`, the build's
 *   function), then hue, saturation and brightness (`adjustPixels`, Rust `color_adjust`);
 *   the tint is the material color.
 * - Game hero: color ranges are exact (the build recolors a copy of the texture); the tint,
 *   saturation and brightness are the game shader's, so they stay the approximation the page
 *   has always shown (the CSS `saturate()` and `brightness()` filters of the material's albedo
 *   adjustment, here in pixel form); the tint is the material color. */
import type { HeroMaterialEntry } from "../components/AssetCatalog";
import type { HeroColorRange } from "../models";
import { adjustPixels, type ColorAdjustment } from "../preview/colorAdjust";
import { changesNothing, hsv, rangeWeight, recolorPixels, recolorTexel, withDefaults, type ColorRange } from "./heroColorRanges";
import type { ColorMode, ColorValues, Rgb } from "./colorAdapter";

export interface PixelRecipe {
  ranges: ColorRange[];
  /** Exact hue/saturation/brightness (replacement). */
  adjust: ColorAdjustment | null;
  /** The game's approximation: CSS `saturate()` and `brightness()` factors. */
  approximate: { saturate: number; brightness: number } | null;
}
export const PLAIN: PixelRecipe = { ranges: [], adjust: null, approximate: null };
/** The enabled ranges of saved colors, with defaults for unset values. */
export function activeRanges(row?: { ranges?: HeroColorRange[] } | null): ColorRange[] {
  return (row?.ranges ?? []).filter((range) => range.enabled !== false).map(withDefaults);
}
/** The enabled ranges that change something: the ranges a build recolors the texture with
 * (Rust `color_ranges::enabled`). */
export function changingRanges(row?: { ranges?: HeroColorRange[] } | null): ColorRange[] {
  return activeRanges(row).filter((range) => !changesNothing(range));
}
/** The recipe for one material in one view ("original": Game / Original; "yours": with the saved colors). */
export function recipeFor(mode: ColorMode, view: "original" | "yours", values: ColorValues | null, entry?: HeroMaterialEntry | null): PixelRecipe {
  const shown = view === "yours" ? values : null;
  const ranges = activeRanges(shown);
  if (mode === "replacement") {
    const adjust = { hue: shown?.hue ?? 0, saturation: shown?.saturation ?? 1, brightness: shown?.brightness ?? 1 };
    return { ranges, adjust: adjust.hue === 0 && adjust.saturation === 1 && adjust.brightness === 1 ? null : adjust, approximate: null };
  }
  const albedo = entry?.albedo ?? [1, 1, 1];
  const approximate = { saturate: albedo[1] * (shown?.saturation ?? 1), brightness: albedo[2] * (shown?.brightness ?? 1) };
  return { ranges, adjust: null, approximate: approximate.saturate === 1 && approximate.brightness === 1 ? null : approximate };
}
export const recipeKey = (recipe: PixelRecipe) => JSON.stringify(recipe);
export const isPlain = (recipe: PixelRecipe) => !recipe.ranges.length && !recipe.adjust && !recipe.approximate;

/** CSS `saturate(s)` then `brightness(b)` on 8-bit RGBA, in place (the Filter Effects matrices). */
export function approximatePixels(data: Uint8ClampedArray, saturate: number, brightness: number) {
  const s = saturate, b = brightness;
  const m = [
    0.213 + 0.787 * s, 0.715 - 0.715 * s, 0.072 - 0.072 * s,
    0.213 - 0.213 * s, 0.715 + 0.285 * s, 0.072 - 0.072 * s,
    0.213 - 0.213 * s, 0.715 - 0.715 * s, 0.072 + 0.928 * s,
  ];
  for (let i = 0; i < data.length; i += 4) {
    const r = data[i], g = data[i + 1], bl = data[i + 2];
    // Uint8ClampedArray rounds and clamps on assignment, as the filter's 8-bit result.
    data[i] = (m[0] * r + m[1] * g + m[2] * bl) * b;
    data[i + 1] = (m[3] * r + m[4] * g + m[5] * bl) * b;
    data[i + 2] = (m[6] * r + m[7] * g + m[8] * bl) * b;
  }
}
/** Where the selection map goes: `out` is RGBA, one texel for every `step` × `step` block of the
 * `width` × height image (its top-left pixel), so the glow costs a quarter at step 2. */
export interface SelectionTarget {
  range: ColorRange; out: Uint8ClampedArray; width: number; step: number;
  /** A range for one part: its part's mask at the source's size (colorParts.ts `rangeMasks`). */
  mask?: Uint8Array | null;
}
export const selectionSize = (width: number, height: number, step: number) => ({ width: Math.ceil(width / step), height: Math.ceil(height / step) });
function select(target: SelectionTarget, data: Uint8ClampedArray, i: number, outWidth: number): number {
  const pixel = i / 4, x = pixel % target.width, y = (pixel - x) / target.width;
  if (x % target.step || y % target.step) return -1;
  const weight = target.mask && target.mask[pixel] !== 1 ? 0 : rangeWeight(target.range, hsv(data[i] / 255, data[i + 1] / 255, data[i + 2] / 255));
  const at = ((y / target.step) * outWidth + x / target.step) * 4;
  target.out[at] = target.out[at + 1] = target.out[at + 2] = weight * 255;
  target.out[at + 3] = 255;
  return weight;
}
/** What one color range selects, as a map the 3D view can light: each pixel's weight (the
 * build's `rangeWeight` of its original color) in RGB, alpha opaque. Returns the summed weight. */
export function selectionPixels(target: SelectionTarget, data: Uint8ClampedArray): number {
  const outWidth = Math.ceil(target.width / target.step);
  let sum = 0;
  for (let i = 0; i < data.length; i += 4) { const weight = select(target, data, i, outWidth); if (weight > 0) sum += weight; }
  return sum;
}
/** The recipe applied to RGBA pixels in place. With `selection`, what that range selects is
 * written to `selection.out` in the same pass over the pixels (the recolor reads the same
 * original color), for the selection glow while a range is dragged. */
export function applyRecipe(data: Uint8ClampedArray, recipe: PixelRecipe, selection?: SelectionTarget, reach?: (Uint8Array | null)[]) {
  if (selection) {
    const outWidth = Math.ceil(selection.width / selection.step);
    const texel: [number, number, number, number] = [0, 0, 0, 255];
    for (let i = 0; i < data.length; i += 4) {
      select(selection, data, i, outWidth);
      if (recipe.ranges.length) {
        texel[0] = data[i]; texel[1] = data[i + 1]; texel[2] = data[i + 2];
        const pixel = i / 4;
        const [r, g, b] = recolorTexel(recipe.ranges, texel, reach ? (index) => { const m = reach[index]; return !m || m[pixel] === 1; } : undefined);
        data[i] = r; data[i + 1] = g; data[i + 2] = b;
      }
    }
  } else if (recipe.ranges.length) recolorPixels(recipe.ranges, data, false, reach);
  if (recipe.adjust) adjustPixels(data, recipe.adjust);
  if (recipe.approximate) approximatePixels(data, recipe.approximate.saturate, recipe.approximate.brightness);
}/** A plain base color (a material without an image) after the recipe, as the build changes it. */
export function recipeColor(color: Rgb, recipe: PixelRecipe): Rgb {
  const data = new Uint8ClampedArray([...color.map((v) => Math.round(Math.min(1, Math.max(0, v)) * 255)), 255]);
  applyRecipe(data, { ...recipe, ranges: [] });
  return [data[0] / 255, data[1] / 255, data[2] / 255];
}

/** The image pixel under a click on an `object-fit: cover` element, or null outside the image. */
export function coverPixel(box: { left: number; top: number; width: number; height: number }, x: number, y: number, width: number, height: number) {
  const scale = Math.max(box.width / width, box.height / height);
  return boxPixel(box, x, y, width, height, scale);
}
/** The image pixel under a click on an `object-fit: contain` element, or null outside the image. */
export function containPixel(box: { left: number; top: number; width: number; height: number }, x: number, y: number, width: number, height: number) {
  const scale = Math.min(box.width / width, box.height / height);
  return boxPixel(box, x, y, width, height, scale);
}
function boxPixel(box: { left: number; top: number; width: number; height: number }, x: number, y: number, width: number, height: number, scale: number) {
  const px = Math.floor((x - box.left - (box.width - width * scale) / 2) / scale);
  const py = Math.floor((y - box.top - (box.height - height * scale) / 2) / scale);
  return px >= 0 && py >= 0 && px < width && py < height ? { x: px, y: py } : null;
}
export interface Box { left: number; top: number; width: number; height: number }
/** The box a `width` × `height` image is drawn in when it is fitted whole inside `box`
 * (`object-fit: contain`, centred): the box the Texture view's image and its selection outline
 * share, so a click is read against what is on screen. */
export function containBox(box: Box, width: number, height: number): Box {
  const scale = Math.min(box.width / width, box.height / height);
  const w = width * scale, h = height * scale;
  return { left: box.left + (box.width - w) / 2, top: box.top + (box.height - h) / 2, width: w, height: h };
}
/** Where a click at (x, y) lands on an image drawn in `drawn` (in image pixels, fractional:
 * 0 is the left edge of the first pixel), or null outside the image. */
export function imagePoint(drawn: Box, x: number, y: number, width: number, height: number) {
  if (drawn.width <= 0 || drawn.height <= 0) return null;
  const px = ((x - drawn.left) / drawn.width) * width, py = ((y - drawn.top) / drawn.height) * height;
  return px >= 0 && py >= 0 && px < width && py < height ? { x: px, y: py } : null;
}
/** How far around a click the color is read: about 3 × 3 screen pixels. */
export const PICK_AREA_PX = 3;
/** The average RGB of the pixels of `image` within `radius` image pixels of (x, y) (a square,
 * cut at the image's edges; at least the pixel under the point), rounded to bytes. Pixels whose
 * alpha is below 8 are left out unless nothing else is there. */
export function areaAverage(image: { data: Uint8ClampedArray; width: number; height: number }, x: number, y: number, radius: number): [number, number, number] {
  const clampX = (v: number) => Math.min(image.width - 1, Math.max(0, v)), clampY = (v: number) => Math.min(image.height - 1, Math.max(0, v));
  const x0 = clampX(Math.floor(x - radius)), x1 = clampX(Math.floor(x + radius));
  const y0 = clampY(Math.floor(y - radius)), y1 = clampY(Math.floor(y + radius));
  const points: [number, number][] = [];
  for (let py = y0; py <= y1; py++) for (let px = x0; px <= x1; px++) points.push([px, py]);
  return averageOf(image, points);
}
/** The average RGB of the pixels a list of texture coordinates shows (one per sample; a pixel
 * two samples land on counts twice, as it covers more of the screen), rounded to bytes. */
export function uvAverage(image: { data: Uint8ClampedArray; width: number; height: number }, samples: { u: number; v: number }[]): [number, number, number] {
  return averageOf(image, samples.map(({ u, v }) => { const p = uvPixel(u, v, image.width, image.height); return [p.x, p.y] as [number, number]; }));
}
function averageOf(image: { data: Uint8ClampedArray; width: number }, points: [number, number][]): [number, number, number] {
  const sum = (opaqueOnly: boolean) => {
    let r = 0, g = 0, b = 0, n = 0;
    for (const [x, y] of points) {
      const at = (y * image.width + x) * 4;
      if (opaqueOnly && image.data[at + 3] < 8) continue;
      r += image.data[at]; g += image.data[at + 1]; b += image.data[at + 2]; n++;
    }
    return n ? [Math.round(r / n), Math.round(g / n), Math.round(b / n)] as [number, number, number] : null;
  };
  return sum(true) ?? sum(false) ?? [0, 0, 0];
}
/** How far an image's channels may differ and still count as one flat color (rounding of a
 * compressed texture). */
export const FLAT_SPREAD = 2;
const flatCache = new WeakMap<object, boolean>();
/** True when an image is one flat color (a 1 × 1 image, or the same color everywhere): there is
 * no color in it to pick out. */
export function flatImage(image: { data: Uint8ClampedArray; width: number; height: number }): boolean {
  const known = flatCache.get(image);
  if (known !== undefined) return known;
  const { data } = image;
  // Fully clear pixels show nothing, so they do not count (unless the image has nothing else).
  let first = -1;
  for (let i = 0; i < data.length && first < 0; i += 4) if (data[i + 3] >= 8) first = i;
  const opaque = first >= 0;
  if (!opaque) first = 0;
  let flat = true;
  for (let i = 0; i < data.length && flat; i += 4) {
    if (opaque && data[i + 3] < 8) continue;
    if (Math.abs(data[i] - data[first]) > FLAT_SPREAD || Math.abs(data[i + 1] - data[first + 1]) > FLAT_SPREAD || Math.abs(data[i + 2] - data[first + 2]) > FLAT_SPREAD) flat = false;
  }
  flatCache.set(image, flat);
  return flat;
}
/** The pixel a texture coordinate shows: wrapped into 0–1 (repeat wrapping), top row first
 * (the textures are not flipped: v = 0 is the image's first row). */
export function uvPixel(u: number, v: number, width: number, height: number) {
  const wrap = (t: number) => t - Math.floor(t);
  return { x: Math.min(width - 1, Math.floor(wrap(u) * width)), y: Math.min(height - 1, Math.floor(wrap(v) * height)) };
}
/** The RGB bytes of one pixel. */
export function pixelAt(image: ImageData, x: number, y: number): [number, number, number] {
  const at = (y * image.width + x) * 4;
  return [image.data[at], image.data[at + 1], image.data[at + 2]];
}
/** True when every texture coordinate is zero (nothing to pick from on the model). */
export function zeroUvs(uvs: Float32Array | null | undefined) {
  if (!uvs || !uvs.length) return true;
  for (let i = 0; i < uvs.length; i++) if (Math.abs(uvs[i]) > 1e-6) return false;
  return true;
}
