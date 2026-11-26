/** Recolor by picking: a color range made from two colors, the one clicked on the character
 * (its original base-color pixel) and the new color the user chooses. Only the range's existing
 * fields are set; the range maths (`heroColorRanges.ts`, Rust `color_ranges`) is not changed.
 *
 * What it selects, around the picked pixel's hue h, saturation s and brightness v (HSV, 0–1):
 * - hue h (one decimal), a half width of PICK_HUE_WIDTH (20°) and the default softness
 *   (0.25, which fades over a further 7.5°): the shades of one color, not its neighbours;
 * - saturation s ± PICK_SATURATION_SPAN (0.35), never below PICK_SATURATION_FLOOR (0.1), so grey
 *   and white areas next to the color stay out;
 * - brightness v ± PICK_BRIGHTNESS_SPAN (0.45), never below 0.04: fabric shading (folds, shadow
 *   sides) keeps its hue and saturation but changes a lot in brightness.
 * The picked pixel always lies inside every window, so the range selects it fully (weight 1).
 *
 * What it changes it to: a range turns a selected color (h, s, v) into
 * ((h + hue_shift) mod 360, min(1, s × saturation), min(1, v × brightness)). So, from the color
 * the range starts from (the picked pixel after any earlier enabled range that also selects it)
 * to the chosen color (h', s', v'):
 *   hue_shift = h' − h wrapped to −180…180 (0 when the chosen color is grey: no hue to reach),
 *   saturation = s' / s, brightness = v' / v,
 * both capped at the limit of 0–2 the build checks. Reachable: any hue, anything paler or darker
 * down to grey (saturation 0) and black (brightness 0), and up to twice as saturated or bright.
 * Not reachable: more than twice the saturation or brightness (the closest color is used and
 * the page says so). A grey, white or black pick has no hue to select or shift: it is given the
 * chosen color instead (see "grey, white and black" below; refused, with Whole material and Tint
 * offered, only if the switch `set_colour` is turned off). */
import type { HeroColorRange } from "../models";
import { changesNothing, hsv, recolorTexel, rgb, usesSetColour, withDefaults, type ColorRangeValues } from "./heroColorRanges";
import type { Rgb } from "./colorAdapter";
import { mc, t, tc } from "../i18n";
// i18n-area: colors

export const PICK_HUE_WIDTH = 20;
export const PICK_SATURATION_SPAN = 0.35;
export const PICK_SATURATION_FLOOR = 0.1;
export const PICK_BRIGHTNESS_SPAN = 0.45;
export const PICK_SOFTNESS = 0.25;
/** Below this saturation a pixel counts as grey or white; below GREY_DARK brightness as black. */
export const GREY_SATURATION = 0.1;
export const GREY_DARK = 0.06;
/** The range's change factors: 0–2 (`color_ranges` validate). */
export const FACTOR_LIMIT = 2;

export type Bytes = [number, number, number];
const unit = (bytes: Bytes): Rgb => [bytes[0] / 255, bytes[1] / 255, bytes[2] / 255];
const round = (value: number, places: number) => Math.round(value * 10 ** places) / 10 ** places;
const clamp = (value: number, low: number, high: number) => Math.min(high, Math.max(low, value));

/** Why a picked pixel cannot make a range: "grey" (grey or white) or "black"; null when it can. */
export function huelessPick(bytes: Bytes): "grey" | "black" | null {
  const [, s, v] = hsv(...unit(bytes));
  if (v < GREY_DARK) return "black";
  if (s < GREY_SATURATION) return "grey";
  return null;
}

/** The selection part of a range around a picked pixel (see the file comment). */
export function selectionAround(bytes: Bytes): Pick<Required<ColorRangeValues>, "hue" | "hue_width" | "saturation_range" | "brightness_range" | "softness"> {
  const [h, s, v] = hsv(...unit(bytes));
  return {
    hue: round(h, 1) % 360,
    hue_width: PICK_HUE_WIDTH,
    // Rounded outward, so the picked pixel stays inside (a grey or black pick is refused first).
    saturation_range: [Math.floor(Math.max(PICK_SATURATION_FLOOR, s - PICK_SATURATION_SPAN) * 100) / 100, Math.ceil(Math.min(1, s + PICK_SATURATION_SPAN) * 100) / 100],
    brightness_range: [Math.floor(Math.max(0.04, v - PICK_BRIGHTNESS_SPAN) * 100) / 100, Math.ceil(Math.min(1, v + PICK_BRIGHTNESS_SPAN) * 100) / 100],
    softness: PICK_SOFTNESS,
  };
}

export interface Change {
  hue_shift: number;
  saturation: number;
  brightness: number;
  /** What the range makes of `from` (the chosen color, or the closest it can reach). */
  reached: Rgb;
  /** The chosen color is out of reach: "saturation" (more than twice as saturated) and/or "brightness". */
  limits: ("saturation" | "brightness")[];
}
/** The change that turns `from` into `to` under the range maths (see the file comment). */
export function changeToward(from: Rgb, to: Rgb): Change {
  const [h, s, v] = hsv(...from);
  const [h2, s2, v2] = hsv(...to);
  const limits: Change["limits"] = [];
  const factor = (target: number, source: number, kind: "saturation" | "brightness") => {
    if (target <= 1e-6) return 0;
    if (source <= 1e-6) { limits.push(kind); return 1; }
    const wanted = target / source;
    if (wanted > FACTOR_LIMIT + 1e-9) { limits.push(kind); return FACTOR_LIMIT; }
    return clamp(round(wanted, 3), 0, FACTOR_LIMIT);
  };
  const saturation = factor(s2, s, "saturation");
  const brightness = factor(v2, v, "brightness");
  // A grey target has no hue to reach; a shift there would only matter for colors around it.
  let shift = s2 < 1e-3 || v2 < 1e-3 ? 0 : ((h2 - h + 540) % 360) - 180;
  shift = round(shift, 1);
  if (shift <= -180) shift = 180;
  const reached = rgb(((h + shift) % 360 + 360) % 360, clamp(s * saturation, 0, 1), clamp(v * brightness, 0, 1));
  return { hue_shift: shift === 0 ? 0 : shift, saturation, brightness, reached, limits };
}

// ---------------------------------------------------------------- grey, white and black
// With the switch `set_colour` (heroColorRanges.ts COLOR_SWITCHES, on) a grey, white or black pick
// makes a range that GIVES what it selects the chosen color instead of shifting it:
// - selection, whatever the hue (hue 0, hue_width 180, softness PICK_SOFTNESS):
//   grey or white (s < GREY_SATURATION): saturation 0 … s + GREY_SATURATION_SPAN (rounded up to
//   0.01) and brightness v ± GREY_BRIGHTNESS_SPAN (rounded outward, within 0–1), so light grey
//   and dark grey of one material can be told apart;
//   black (v < GREY_DARK): every saturation (a dark pixel's saturation is noise) and brightness
//   0 … v + BLACK_BRIGHTNESS_SPAN (rounded up);
// - change: set_hue h' (one decimal) and set_saturation s' (three decimals) of the chosen color;
//   brightness factor v' / v (three decimals, 0–2) for grey and white, 1 for black; lift the
//   rest, v' − v × factor (three decimals, written only when at least 0.005), so black can
//   become a dark color and shading is kept.
export const GREY_SATURATION_SPAN = 0.1;
export const GREY_BRIGHTNESS_SPAN = 0.25;
export const BLACK_BRIGHTNESS_SPAN = 0.15;
/** The selection part of a range around a grey, white or black pick. */
export function greySelectionAround(bytes: Bytes): Pick<Required<ColorRangeValues>, "hue" | "hue_width" | "saturation_range" | "brightness_range" | "softness"> {
  const [, s, v] = hsv(...unit(bytes));
  const up = (x: number) => Math.ceil(Math.min(1, x) * 100) / 100, down = (x: number) => Math.floor(Math.max(0, x) * 100) / 100;
  const black = v < GREY_DARK;
  return {
    hue: 0,
    hue_width: 180,
    saturation_range: black ? [0, 1] : [0, up(s + GREY_SATURATION_SPAN)],
    brightness_range: black ? [0, up(v + BLACK_BRIGHTNESS_SPAN)] : [down(v - GREY_BRIGHTNESS_SPAN), up(v + GREY_BRIGHTNESS_SPAN)],
    softness: PICK_SOFTNESS,
  };
}
/** The new color a grey, white or black pick gives (see above), from `from` (the color the range
 * starts from) to `to`. */
export function givenChange(from: Rgb, to: Rgb): Pick<ColorRangeValues, "set_hue" | "set_saturation" | "brightness" | "lift"> & { limits: Change["limits"] } {
  const [, , v] = hsv(...from);
  const [h2, s2, v2] = hsv(...to);
  const factor = v < GREY_DARK ? 1 : clamp(round(v2 / v, 3), 0, FACTOR_LIMIT);
  const rest = round(v2 - v * factor, 3);
  const lift = clamp(rest, -1, 1);
  return {
    set_hue: round(h2, 1) % 360,
    set_saturation: clamp(round(s2, 3), 0, 1),
    brightness: factor,
    ...(Math.abs(lift) >= 0.005 ? { lift } : {}),
    limits: rest !== lift ? ["brightness"] : [],
  };
}

export interface PickResult {
  range: HeroColorRange;
  /** What every enabled range makes of the picked pixel with this range in place (0–1). */
  result: Rgb;
  limits: Change["limits"];
  /** Another enabled range also changes the picked color, so the result differs from the chosen one. */
  otherRange: boolean;
}
/** The range made from a picked pixel and a chosen color. `ranges` are the material's ranges
 * (with `index` replaced, or the new one appended when index is null); `keep` keeps that range's
 * selection (Change the new color…) instead of selecting around the picked pixel. */
export function pickRange(bytes: Bytes, to: Rgb, ranges: HeroColorRange[], index: number | null, keep = false): PickResult {
  const at = index ?? ranges.length;
  const before = ranges.slice(0, at).filter((r) => r.enabled !== false).map(withDefaults);
  // The range adjusts the color earlier ranges have left (it selects by the original one).
  const start = recolorTexel(before, [...bytes, 255]);
  const from = unit([start[0], start[1], start[2]]);
  const old = index !== null ? ranges[index] : undefined;
  // A grey, white or black pick (or a range made from one) is given the new color (switch set_colour).
  const given = keep && old ? usesSetColour(old) : huelessPick(bytes) !== null;
  let range: HeroColorRange;
  let limits: Change["limits"];
  let reached: Rgb;
  if (given) {
    const selection = keep && old ? {} : greySelectionAround(bytes);
    const { limits: capped, ...change } = givenChange(from, to);
    const base: HeroColorRange = { ...withDefaults({ ...(old ?? {}), ...selection }), hue_shift: 0, saturation: 1 };
    delete base.lift;
    range = { enabled: true, ...base, ...change };
    limits = capped;
    reached = recolorTexel([withDefaults({ ...range, hue_width: 180, saturation_range: [0, 1], brightness_range: [0, 1] })], [...bytes, 255]).slice(0, 3).map((c) => c / 255) as Rgb;
  } else {
    const change = changeToward(from, to);
    const selection = keep && old ? {} : selectionAround(bytes);
    range = {
      // Picking a color for a range puts it in use.
      enabled: true,
      ...withDefaults({ ...(old ?? {}), ...selection }),
      hue_shift: change.hue_shift,
      saturation: change.saturation,
      brightness: change.brightness,
    };
    // A colored pick shifts: a new color an earlier pick of this range gave does not stay.
    delete range.set_hue; delete range.set_saturation; delete range.lift;
    limits = change.limits;
    reached = change.reached;
  }
  const all = [...ranges];
  all[at] = range;
  const out = recolorTexel(all.filter((r) => r.enabled !== false).map(withDefaults), [...bytes, 255]);
  const result: Rgb = [out[0] / 255, out[1] / 255, out[2] / 255];
  // Only a later range can move the result away from the color this range reaches.
  const target = reached.map((c) => Math.round(clamp(c, 0, 1) * 255));
  const otherRange = out.slice(0, 3).some((c, i) => Math.abs(c - target[i]) > 3);
  return { range, result, limits, otherRange };
}

// ---------------------------------------------------------------- names

/** A plain name for a color: "blue", "dark red", "white" … */
export function colorName(color: Rgb): string {
  const [h, s, v] = hsv(...(color.map((c) => clamp(c, 0, 1)) as Rgb));
  if (v < 0.12) return "black";
  if (s < 0.12) return v > 0.85 ? "white" : v < 0.35 ? "dark grey" : "grey";
  let name =
    h < 12 || h >= 345 ? "red"
    : h < 40 ? (v < 0.55 ? "brown" : "orange")
    : h < 52 ? (s > 0.45 && v < 0.96 ? "gold" : "yellow")
    : h < 68 ? "yellow"
    : h < 160 ? "green"
    : h < 185 ? "teal"
    : h < 200 ? "cyan"
    : h < 255 ? "blue"
    : h < 290 ? "purple"
    : h < 330 ? "pink"
    : "red";
  if (name === "pink" && v < 0.5) name = "purple";
  const shade = name === "brown" || name === "gold" ? "" : v < 0.4 ? "dark " : s < 0.35 && v > 0.75 ? "light " : "";
  return shade + name;
}
const capital = (text: string) => text.charAt(0).toUpperCase() + text.slice(1);
/** "Blue → gold"; the same name on both sides says how it changes ("Blue → darker blue"). */
export function rangeName(from: Rgb, to: Rgb, changes: boolean): string {
  const a = colorName(from), b = colorName(to);
  if (!changes) return `${capital(a)} · no change yet`;
  if (a !== b) return `${capital(a)} → ${b}`;
  const [, s1, v1] = hsv(...from), [, s2, v2] = hsv(...to);
  const how = v2 < v1 - 0.08 ? "darker" : v2 > v1 + 0.08 ? "lighter" : s2 < s1 - 0.08 ? "paler" : s2 > s1 + 0.08 ? "stronger" : "shifted";
  return `${capital(a)} → ${how} ${b.replace(/^(dark|light) /, "")}`;
}
/** Every name `colorName` gives, marked for translation (the names stay English as values). */
const COLOR_NAMES: readonly string[] = [
  mc("color name", "black"), mc("color name", "white"), mc("color name", "grey"), mc("color name", "dark grey"),
  mc("color name", "brown"), mc("color name", "gold"),
  mc("color name", "red"), mc("color name", "dark red"), mc("color name", "light red"),
  mc("color name", "orange"), mc("color name", "dark orange"), mc("color name", "light orange"),
  mc("color name", "yellow"), mc("color name", "dark yellow"), mc("color name", "light yellow"),
  mc("color name", "green"), mc("color name", "dark green"), mc("color name", "light green"),
  mc("color name", "teal"), mc("color name", "dark teal"), mc("color name", "light teal"),
  mc("color name", "cyan"), mc("color name", "dark cyan"), mc("color name", "light cyan"),
  mc("color name", "blue"), mc("color name", "dark blue"), mc("color name", "light blue"),
  mc("color name", "purple"), mc("color name", "dark purple"), mc("color name", "light purple"),
  mc("color name", "pink"), mc("color name", "dark pink"), mc("color name", "light pink"),
];
const shownName = (name: string) => (COLOR_NAMES.includes(name) ? tc("color name", name) : name);
/** A color's plain name in the language shown (`colorName` as shown). */
export const shownColorName = (color: Rgb) => shownName(colorName(color));
/** `rangeName` in the language shown, as whole messages: "Blue → gold", "Blue → darker blue". */
export function shownRangeName(from: Rgb, to: Rgb, changes: boolean): string {
  const a = colorName(from), b = colorName(to);
  const first = capital(shownName(a));
  if (!changes) return t("{from} · no change yet", { from: first });
  if (a !== b) return t("{from} → {to}", { from: first, to: shownName(b) });
  const [, s1, v1] = hsv(...from), [, s2, v2] = hsv(...to);
  const base = shownName(b.replace(/^(dark|light) /, ""));
  if (v2 < v1 - 0.08) return t("{from} → darker {to}", { from: first, to: base });
  if (v2 > v1 + 0.08) return t("{from} → lighter {to}", { from: first, to: base });
  if (s2 < s1 - 0.08) return t("{from} → paler {to}", { from: first, to: base });
  if (s2 > s1 + 0.08) return t("{from} → stronger {to}", { from: first, to: base });
  return t("{from} → shifted {to}", { from: first, to: base });
}
/** A range changes something (a shift, or a factor other than 1). */
export const rangeChanges = (range: HeroColorRange) => !changesNothing(withDefaults(range));

// ---------------------------------------------------------------- tint

/** A tint whose darkest channel is below this multiplies every color with that channel down to
 * less than 40%: other hues turn dark and toward the tint, hiding recolored parts. */
export const TINT_DARK_LIMIT = 0.4;
/** The tint would hide recolored parts: a tint is set, its darkest channel is below
 * TINT_DARK_LIMIT and at least one color range is in use. */
export function tintHidesRanges(tint: Rgb | undefined | null, ranges: HeroColorRange[] | undefined): boolean {
  if (!tint) return false;
  if (!(ranges ?? []).some((r) => r.enabled !== false)) return false;
  return Math.min(...tint) < TINT_DARK_LIMIT;
}
