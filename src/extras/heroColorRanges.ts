/** Hero color ranges for the Extras preview: the same function as Rust
 * `deadlock-components color_ranges` (which the build uses). A range selects
 * base-color pixels by hue (a centre and a half width that wraps around red),
 * saturation and brightness (HSV value) with a soft edge, and shifts the hue
 * and scales saturation and brightness. Every range selects by the original
 * color; changes of several ranges add up in order. Alpha is never touched.
 * With COLOR_SWITCHES (both on) a range may also give what it selects a new hue and saturation and
 * lift its brightness (`set_hue`, `set_saturation`, `lift`), and reach only one part of the model
 * (`part`, colorParts.ts). Without those fields a range is exactly what it was.
 * Both tests check `src-tauri/crates/deadlock-components/tests/fixtures/hero-color-ranges.json`. */

/** What color ranges may do beyond selecting and shifting (Rust `color_ranges::Switches`). */
export interface ColorSwitches { set_colour: boolean; part_only: boolean }
/** The app's switches: equal to Rust `color_ranges::SWITCHES` (a Rust test reads this line). Both
 * are on. */
export const COLOR_SWITCHES: ColorSwitches = { set_colour: true, part_only: true };
/** The switches the page uses: COLOR_SWITCHES, or a test's own (`openColorSwitchesForTest`). */
let switchesInForce: ColorSwitches = COLOR_SWITCHES;
export const colorSwitches = (): ColorSwitches => switchesInForce;
/** Tests and harnesses only: open switches the app keeps off (null puts COLOR_SWITCHES back). */
export function openColorSwitchesForTest(switches: ColorSwitches | null) { switchesInForce = switches ?? COLOR_SWITCHES; }

/** The spot of the model a range was picked on (texture coordinates). */
export interface ColorPart { u: number; v: number }
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
  /** A new color for what the range selects: hue (0–360) and saturation (0–1), always together;
   * they replace hue_shift and saturation. */
  set_hue?: number;
  set_saturation?: number;
  /** Added to the brightness after `brightness` scales it (-1–1). */
  lift?: number;
  /** Only the part of the model at this spot. */
  part?: ColorPart;
}
type CoreKey = "hue" | "hue_width" | "saturation_range" | "brightness_range" | "softness" | "hue_shift" | "saturation" | "brightness";
type NewKey = "set_hue" | "set_saturation" | "lift" | "part";
export type ColorRange = Required<Pick<ColorRangeValues, CoreKey>> & Pick<ColorRangeValues, NewKey>;
export type Rgba = [number, number, number, number];
const NEW_KEYS: NewKey[] = ["set_hue", "set_saturation", "lift", "part"];

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
  const value: ColorRange = { ...RANGE_DEFAULTS };
  for (const key of Object.keys(RANGE_DEFAULTS) as CoreKey[]) {
    if (range[key] !== undefined && range[key] !== null) (value as Record<string, unknown>)[key] = range[key];
  }
  // The new fields only when set (a lift of 0 is no lift), as Rust `ColorRange::from_choice`.
  for (const key of NEW_KEYS) {
    const set = range[key];
    if (set === undefined || set === null || (key === "lift" && set === 0)) continue;
    (value as Record<string, unknown>)[key] = set;
  }
  return value;
}
/** The range gives what it selects a new color, or lifts its brightness (switch `set_colour`). */
export const usesSetColour = (range: ColorRangeValues) =>
  range.set_hue != null || range.set_saturation != null || (range.lift != null && range.lift !== 0);
/** The range leaves every color as it is (Rust `ColorRange::changes_nothing`, on a range with defaults). */
export function changesNothing(range: ColorRange): boolean {
  return !usesSetColour(range) && ((range.hue_shift % 360) + 360) % 360 === 0 && range.saturation === 1 && range.brightness === 1;
}
/** Why the switches in force refuse a range, or "" (Rust `ColorRange::check_switches`). */
export function switchRefusal(range: ColorRangeValues, switches: ColorSwitches = colorSwitches()): "" | "set_colour" | "part_only" {
  if (usesSetColour(range) && !switches.set_colour) return "set_colour";
  if (range.part && !switches.part_only) return "part_only";
  return "";
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
const clamp01 = (x: number) => Math.min(1, Math.max(0, x));
/** The changed color (Rust `ColorRange::adjust`): without the new fields hue + hue_shift,
 * saturation × saturation, brightness × brightness; with them hue set_hue, saturation
 * set_saturation, brightness clamp(v × brightness + lift). */
function adjust(range: ColorRange, color: [number, number, number]) {
  const [h, s, v] = hsv(...color);
  if (!usesSetColour(range)) return rgb(remEuclid(h + range.hue_shift, 360), clamp01(s * range.saturation), clamp01(v * range.brightness));
  const given = range.set_hue != null && range.set_saturation != null;
  const hue = given ? remEuclid(range.set_hue!, 360) : remEuclid(h + range.hue_shift, 360);
  const saturation = given ? clamp01(range.set_saturation!) : clamp01(s * range.saturation);
  return rgb(hue, saturation, clamp01(v * range.brightness + (range.lift ?? 0)));
}
const byte = (v: number) => Math.min(255, Math.max(0, Math.round(v * 255)));

/** One 8-bit texel after every range; alpha is kept. With `inside`, range i changes the texel only
 * where inside(i) is true (a part range outside its part: Rust `recolor_where`). */
export function recolorTexel(ranges: ColorRange[], [r, g, b, a]: Rgba, inside?: (index: number) => boolean): Rgba {
  const original: [number, number, number] = [r / 255, g / 255, b / 255];
  const selection = hsv(...original);
  const color: [number, number, number] = [...original];
  for (let index = 0; index < ranges.length; index++) {
    if (inside && !inside(index)) continue;
    const range = ranges[index];
    const weight = rangeWeight(range, selection);
    if (weight <= 0) continue;
    const changed = adjust(range, color);
    for (let i = 0; i < 3; i++) color[i] += (changed[i] - color[i]) * weight;
  }
  return [byte(color[0]), byte(color[1]), byte(color[2]), a];
}

/** RGBA pixels (canvas `ImageData.data`) recolored in place. With `mask`, each
 * pixel shows how much the ranges select it instead (white = fully). */
export function recolorPixels(ranges: ColorRange[], data: Uint8ClampedArray, mask = false, reach?: (Uint8Array | null)[]) {
  for (let i = 0; i < data.length; i += 4) {
    // A range for one part reaches only the pixels its part mask marks (colorParts.ts).
    const pixel = i / 4;
    const inside = reach ? (index: number) => { const m = reach[index]; return !m || m[pixel] === 1; } : undefined;
    if (mask) {
      const selection = hsv(data[i] / 255, data[i + 1] / 255, data[i + 2] / 255);
      const weight = Math.min(1, ranges.reduce((sum, range, index) => sum + (!inside || inside(index) ? rangeWeight(range, selection) : 0), 0));
      data[i] = data[i + 1] = data[i + 2] = byte(weight);
    } else {
      const [r, g, b] = recolorTexel(ranges, [data[i], data[i + 1], data[i + 2], 255], inside);
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
