/** Hero color ranges for the Extras preview: the same function as Rust
 * `deadlock-components color_ranges` (which the build uses). A range selects
 * base-color pixels by hue (a centre and a half width that wraps around red),
 * saturation and brightness (HSV value) with a soft edge, and shifts the hue
 * and scales saturation and brightness. Every range selects by the original
 * color; changes of several ranges add up in order. Alpha is never touched.
 * Both tests check `src-tauri/crates/deadlock-components/tests/fixtures/hero-color-ranges.json`. */

export interface ColorRangeValues {
  /** Centre hue in degrees, 0–360. */
  hue?: number;
  /** Half width in degrees, 0–180; 180 selects every hue. */
  hue_width?: number;
  saturation_range?: [number, number];
  brightness_range?: [number, number];
  /** 0 is a hard edge; 1 fades over HUE_FADE degrees and LEVEL_FADE outside the range. */
  softness?: number;
  /** Degrees, -180–180. */
  hue_shift?: number;
  /** Factors, 0–2; 1 keeps the original. */
  saturation?: number;
  brightness?: number;
}
export type ColorRange = Required<ColorRangeValues>;
export type Rgba = [number, number, number, number];

export const HUE_FADE = 30;
export const LEVEL_FADE = 0.15;
export const RANGE_DEFAULTS: ColorRange = {
  hue: 0,
  hue_width: 180,
  saturation_range: [0, 1],
  brightness_range: [0, 1],
  softness: 0.25,
  hue_shift: 0,
  saturation: 1,
  brightness: 1,
};
export function withDefaults(range: ColorRangeValues): ColorRange {
  const value = { ...RANGE_DEFAULTS };
  for (const key of Object.keys(RANGE_DEFAULTS) as (keyof ColorRange)[]) {
    if (range[key] !== undefined && range[key] !== null) (value as Record<string, unknown>)[key] = range[key];
  }
  return value;
}

/** Rust `f64::rem_euclid`. */
const remEuclid = (x: number, m: number) => {
  const r = x % m;
  return r < 0 ? r + m : r;
};
function smooth(fade: number, inside: number) {
  if (inside >= 0) return 1;
  if (fade <= 0 || inside <= -fade) return 0;
  const t = 1 + inside / fade;
  return t * t * (3 - 2 * t);
}
export function hsv(r: number, g: number, b: number): [number, number, number] {
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const delta = max - min;
  const h =
    (delta <= 1e-12 ? 0 : max === r ? remEuclid((g - b) / delta, 6) : max === g ? (b - r) / delta + 2 : (r - g) / delta + 4) * 60;
  return [h, max > 1e-12 ? delta / max : 0, max];
}
export function rgb(h: number, s: number, v: number): [number, number, number] {
  const c = v * s;
  const x = c * (1 - Math.abs(remEuclid(h / 60, 2) - 1));
  const m = v - c;
  const sector = Math.floor(h / 60);
  const [r, g, b] =
    sector === 0 ? [c, x, 0] : sector === 1 ? [x, c, 0] : sector === 2 ? [0, c, x] : sector === 3 ? [0, x, c] : sector === 4 ? [x, 0, c] : [c, 0, x];
  return [r + m, g + m, b + m];
}
/** How much `range` selects a color given as HSV (0–1, hue in degrees). */
export function rangeWeight(range: ColorRange, [h, s, v]: [number, number, number]) {
  const hue =
    range.hue_width >= 180 ? 1 : smooth(range.softness * HUE_FADE, range.hue_width - Math.abs(remEuclid(h - range.hue + 180, 360) - 180));
  const level = (value: number, [low, high]: [number, number]) =>
    smooth(range.softness * LEVEL_FADE, value - low) * smooth(range.softness * LEVEL_FADE, high - value);
  return hue * level(s, range.saturation_range) * level(v, range.brightness_range);
}
function adjust(range: ColorRange, color: [number, number, number]) {
  const [h, s, v] = hsv(...color);
  return rgb(
    remEuclid(h + range.hue_shift, 360),
    Math.min(1, Math.max(0, s * range.saturation)),
    Math.min(1, Math.max(0, v * range.brightness)),
  );
}
const byte = (v: number) => Math.min(255, Math.max(0, Math.round(v * 255)));

/** One 8-bit texel after every range; alpha is kept. */
export function recolorTexel(ranges: ColorRange[], [r, g, b, a]: Rgba): Rgba {
  const original: [number, number, number] = [r / 255, g / 255, b / 255];
  const selection = hsv(...original);
  const color: [number, number, number] = [...original];
  for (const range of ranges) {
    const weight = rangeWeight(range, selection);
    if (weight <= 0) continue;
    const changed = adjust(range, color);
    for (let i = 0; i < 3; i++) color[i] += (changed[i] - color[i]) * weight;
  }
  return [byte(color[0]), byte(color[1]), byte(color[2]), a];
}

/** RGBA pixels (canvas `ImageData.data`) recolored in place. With `mask`, each
 * pixel shows how much the ranges select it instead (white = fully). */
export function recolorPixels(ranges: ColorRange[], data: Uint8ClampedArray, mask = false) {
  for (let i = 0; i < data.length; i += 4) {
    if (mask) {
      const selection = hsv(data[i] / 255, data[i + 1] / 255, data[i + 2] / 255);
      const weight = Math.min(1, ranges.reduce((sum, range) => sum + rangeWeight(range, selection), 0));
      data[i] = data[i + 1] = data[i + 2] = byte(weight);
    } else {
      const [r, g, b] = recolorTexel(ranges, [data[i], data[i + 1], data[i + 2], 255]);
      data[i] = r;
      data[i + 1] = g;
      data[i + 2] = b;
    }
  }
}

/** A range around one picked color: its hue ± 15 degrees, saturation and
 * brightness ± 0.2 (a grey pick takes every hue). */
export function rangeAround([r, g, b]: [number, number, number], keep: ColorRangeValues = {}): ColorRangeValues {
  const [h, s, v] = hsv(r / 255, g / 255, b / 255);
  const round = (x: number) => Math.round(x * 100) / 100;
  const around = (x: number): [number, number] => [round(Math.max(0, x - 0.2)), round(Math.min(1, x + 0.2))];
  return {
    ...keep,
    hue: Math.round(h),
    hue_width: s < 0.12 ? 180 : 15,
    saturation_range: around(s),
    brightness_range: around(v),
  };
}
