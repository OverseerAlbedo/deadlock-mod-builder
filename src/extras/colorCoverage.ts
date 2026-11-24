/** How much of a material's base-color image a color range selects ("selects 12% of this
 * material"), from the original pixels at the preview's working size (`Pixels.small`, at most
 * 512 px). Computed with the range's own weight (`rangeWeight`, the build's function), cached
 * per image and per selection (hue, width, saturation and brightness ranges, softness: a shift
 * does not change what is selected), and at most once per animation frame (and every
 * COVERAGE_INTERVAL ms) while dragging, so a drag keeps its frames for the recolor. */
import { useEffect, useRef, useState } from "react";
import { hsv, rangeWeight, withDefaults, type ColorRangeValues } from "./heroColorRanges";
import { rangeMasks, type MaterialParts } from "./colorParts";
import type { Rgb } from "./colorAdapter";

export interface ImageLike { data: Uint8ClampedArray; width: number; height: number }
export interface Coverage {
  /** Share of the image the range selects (weights added up), 0–1. */
  fraction: number;
  /** Share of the image in the range's hue band: what it selects, plus colored pixels (not grey,
   * white or black) of that hue outside its saturation and brightness ranges. */
  hueFraction: number;
  /** The weighted mean original color of what it selects; null when it selects nothing. */
  mean: Rgb | null;
}
/** A selection this small counts as nothing (well under one pixel in a thousand). */
export const NOTHING = 0.0005;

const SELECTION_KEYS = ["hue", "hue_width", "saturation_range", "brightness_range", "softness"] as const;
/** What a range selects: its selection values and, for a range for one part, the part's spot
 * (absent otherwise, so the key of every other range is what it always was). */
export function selectionKey(range: ColorRangeValues): string {
  const full = withDefaults(range);
  const key = SELECTION_KEYS.map((name) => full[name]);
  return JSON.stringify(full.part ? [...key, full.part] : key);
}
const cache = new WeakMap<object, Map<string, Coverage>>();
/** `parts`: a range for one part needs its material's parts (colorParts.ts) to know its mask; the
 * cache key holds whether they were there. */
const coverageKey = (range: ColorRangeValues, parts?: MaterialParts | null) => selectionKey(range) + (range.part && parts ? "|parts" : "");
export function cachedCoverage(image: ImageLike, range: ColorRangeValues, parts?: MaterialParts | null): Coverage | undefined {
  return cache.get(image)?.get(coverageKey(range, parts));
}
/** The coverage of one range on one image (cached). A range for one part counts only its part's
 * pixels. */
export function rangeCoverage(range: ColorRangeValues, image: ImageLike, parts?: MaterialParts | null): Coverage {
  const key = coverageKey(range, parts);
  let known = cache.get(image);
  if (!known) { known = new Map(); cache.set(image, known); }
  const hit = known.get(key);
  if (hit) return hit;
  const full = withDefaults(range);
  const mask = rangeMasks([full], parts, image.width, image.height)?.[0] ?? null;
  // The hue band alone: the same range with every saturation and brightness.
  const band = { ...full, saturation_range: [0, 1] as [number, number], brightness_range: [0, 1] as [number, number] };
  const data = image.data;
  let sum = 0, hue = 0, r = 0, g = 0, b = 0;
  const pixels = data.length / 4;
  for (let i = 0; i < data.length; i += 4) {
    if (mask && mask[i / 4] !== 1) continue;
    const color = hsv(data[i] / 255, data[i + 1] / 255, data[i + 2] / 255);
    const weight = rangeWeight(full, color);
    if (weight > 0) { sum += weight; r += data[i] * weight; g += data[i + 1] * weight; b += data[i + 2] * weight; hue++; }
    // The hue band only matters for a range that selects nothing (it says why).
    else if (color[1] >= 0.1 && color[2] >= 0.06 && rangeWeight(band, color) > 0) hue++;
  }
  const result: Coverage = {
    fraction: pixels ? sum / pixels : 0,
    hueFraction: pixels ? hue / pixels : 0,
    mean: sum > 0 ? [r / sum / 255, g / sum / 255, b / sum / 255] : null,
  };
  known.set(key, result);
  if (known.size > 64) known.delete(known.keys().next().value!);
  return result;
}
/** "12%", "<1%" for a little, "0%" for nothing. */
export function percentText(fraction: number) {
  if (fraction < NOTHING) return "0%";
  if (fraction < 0.01) return "<1%";
  return `${Math.round(fraction * 100)}%`;
}
/** Why a range selects nothing, when it can be known (null: it selects something). */
export function nothingReason(coverage: Coverage): string | null {
  if (coverage.fraction >= NOTHING) return null;
  return coverage.hueFraction < NOTHING
    ? "the image has no color in that hue: grey and white areas have no hue to select."
    : "the image's colors in that hue are paler, darker or brighter than its Saturation and Brightness ranges.";
}

const frame = (callback: () => void) =>
  typeof requestAnimationFrame === "function" ? requestAnimationFrame(callback) : (setTimeout(callback, 16) as unknown as number);
const cancelFrame = (id: number) => (typeof cancelAnimationFrame === "function" ? cancelAnimationFrame(id) : clearTimeout(id));

/** While dragging, the share is measured again at most this often (ms). */
export const COVERAGE_INTERVAL = 100;
/** The coverage of each range on `image`, computed at most once per animation frame; until a
 * new value is ready the last one shown stays (no flicker while a slider is dragged). */
export function useCoverages(image: ImageLike | null, ranges: ColorRangeValues[], parts?: MaterialParts | null): (Coverage | null)[] {
  const latest = useRef({ image, ranges, parts });
  latest.current = { image, ranges, parts };
  const shown = useRef<(Coverage | null)[]>([]);
  const pending = useRef(0);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const last = useRef(0);
  const [, redraw] = useState(0);
  const signature = ranges.map((range) => coverageKey(range, parts)).join("|");
  useEffect(() => {
    if (!image || pending.current || timer.current) return;
    if (ranges.every((range) => cachedCoverage(image, range, parts))) return;
    const run = () => {
      timer.current = undefined;
      pending.current = frame(() => {
        pending.current = 0;
        last.current = Date.now();
        const now = latest.current;
        if (now.image) for (const range of now.ranges) rangeCoverage(range, now.image, now.parts);
        redraw((v) => v + 1);
      });
    };
    const wait = COVERAGE_INTERVAL - (Date.now() - last.current);
    if (wait > 0) timer.current = setTimeout(run, wait);
    else run();
    // eslint-disable-next-line react-hooks/exhaustive-deps -- the signature stands for the ranges
  }, [image, signature]);
  useEffect(() => () => { if (pending.current) cancelFrame(pending.current); clearTimeout(timer.current); }, []);
  const out = ranges.map((range, index) => (image ? cachedCoverage(image, range, parts) ?? shown.current[index] ?? null : null));
  shown.current = out;
  return out;
}
