/** Color picker arithmetic: hex text, HSB (hue 0–360°, saturation and brightness
 * 0–1, the HSV the color ranges use) and RGB, and the recent colors kept on this PC. */
import { hsv, rgb } from "./heroColorRanges";
import type { Rgb } from "./colorAdapter";

const clamp01 = (value: number) => Math.min(1, Math.max(0, value));
const byte = (value: number) => Math.round(clamp01(value) * 255);
/** `#RRGGBB` of a 0–1 color. */
export function toHex(color: Rgb): string {
  return "#" + color.map((v) => byte(v).toString(16).padStart(2, "0")).join("").toUpperCase();
}
/** A typed hex color: `#5600D8`, `5600d8`, `#56d` (short form); null when it is not one. */
export function parseHex(text: string): Rgb | null {
  const plain = text.trim().replace(/^#/, "");
  const full = /^[0-9a-f]{3}$/i.test(plain) ? plain.split("").map((c) => c + c).join("") : plain;
  if (!/^[0-9a-f]{6}$/i.test(full)) return null;
  return [0, 2, 4].map((at) => parseInt(full.slice(at, at + 2), 16) / 255) as Rgb;
}
export type Hsb = [number, number, number];
export const toHsb = (color: Rgb): Hsb => hsv(...(color.map(clamp01) as Rgb));
export function fromHsb([h, s, b]: Hsb): Rgb {
  const hue = ((h % 360) + 360) % 360;
  return rgb(hue, clamp01(s), clamp01(b)).map(clamp01) as Rgb;
}
/** The same 8-bit color (what the hex box and the build's byte values show). */
export const sameColor = (a: Rgb, b: Rgb) => a.every((v, i) => byte(v) === byte(b[i]));
/** HSB for a color, keeping the hue (and saturation) the user had when the color
 * alone no longer tells them (black or grey). */
export function keepHsb(color: Rgb, previous: Hsb | null): Hsb {
  const next = toHsb(color);
  if (!previous || !sameColor(fromHsb(previous), color)) {
    if (previous && (next[1] === 0 || next[2] === 0)) return [previous[0], next[2] === 0 ? previous[1] : next[1], next[2]];
    return next;
  }
  return previous;
}
/** CSS for a 0–1 color. */
export const cssColor = (color: Rgb) => `rgb(${color.map(byte).join(" ")})`;
/** Readable text (black or white) on a color swatch. */
export function textOn(color: Rgb) {
  const lin = color.map((v) => (v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4));
  const luminance = 0.2126 * lin[0] + 0.7152 * lin[1] + 0.0722 * lin[2];
  return luminance > 0.18 ? "#000" : "#fff";
}

export const RECENT_KEY = "dmb.recentColors";
export const RECENT_LIMIT = 12;
/** The last tints the user set (newest first), as hex; empty when storage is unavailable. */
export function recentColors(): string[] {
  try {
    const value = JSON.parse(localStorage.getItem(RECENT_KEY) ?? "[]");
    return Array.isArray(value) ? value.filter((v): v is string => typeof v === "string" && parseHex(v) !== null).slice(0, RECENT_LIMIT) : [];
  } catch {
    return [];
  }
}
/** Remember a tint (moved to the front when already listed); returns the new list. */
export function rememberColor(color: Rgb): string[] {
  const hex = toHex(color);
  const next = [hex, ...recentColors().filter((v) => v.toUpperCase() !== hex)].slice(0, RECENT_LIMIT);
  try {
    localStorage.setItem(RECENT_KEY, JSON.stringify(next));
  } catch {
    /* storage unavailable: the list lasts for this view only */
  }
  return next;
}
