/** The Colors page's "pointing" light: a material (or the pixels a color range selects) pulses
 * twice in a bright color, then keeps a soft steady glow while it is pointed at, and fades out
 * when it is let go; the rest of the character dims a little. Pure numbers here; the 3D view
 * writes them into its materials from the render loop (`ColorScene`), never through React state
 * per frame. With `prefers-reduced-motion` the glow is steady from the start. */
import { hsv } from "./heroColorRanges";
import type { Rgb } from "./colorAdapter";

/** Seconds of one pulse; two pulses, then the steady level. */
export const PULSE_SECONDS = 0.55;
export const PULSES = 2;
export const STEADY = 0.45;
export const FADE_IN = 0.12;
export const FADE_OUT = 0.3;
/** How much the rest of the character is darkened at most (0.3: to 70%). */
export const DIM = 0.3;
/** Bright cyan, and yellow for materials that are already blue-green. */
export const GLOW_CYAN: Rgb = [0.24, 0.95, 1];
export const GLOW_YELLOW: Rgb = [1, 0.86, 0.18];

/** The glow level (0–1) `t` seconds after pointing started. */
export function glowLevel(t: number, reduced: boolean): number {
  if (t <= 0) return 0;
  const fade = Math.min(1, t / FADE_IN);
  if (reduced || t >= PULSE_SECONDS * PULSES) return STEADY * fade;
  const phase = (t % PULSE_SECONDS) / PULSE_SECONDS;
  return (STEADY + (1 - STEADY) * Math.sin(Math.PI * phase)) * fade;
}
/** The pulses are still running (the view must keep drawing frames). */
export const pulsing = (t: number, reduced: boolean) => t < (reduced ? FADE_IN : PULSE_SECONDS * PULSES);
/** The dimming of the rest (0–1 of DIM), `t` seconds after pointing started. */
export const dimLevel = (t: number) => (t <= 0 ? 0 : Math.min(1, t / FADE_IN));
/** What is left `t` seconds after letting go (1 → 0 over FADE_OUT). */
export const fadeOut = (t: number) => Math.max(0, 1 - t / FADE_OUT);

/** The glow color for a material whose typical color is `color`: cyan, or yellow when the
 * material is already blue-green (hue 150–250° with some color), where cyan would not stand out. */
export function glowColorFor(color: Rgb | null | undefined): Rgb {
  if (!color) return GLOW_CYAN;
  const [h, s, v] = hsv(...(color.map((c) => Math.min(1, Math.max(0, c))) as Rgb));
  return s > 0.2 && v > 0.12 && h >= 150 && h <= 250 ? GLOW_YELLOW : GLOW_CYAN;
}

/** The user asked for less motion (no pulsing). */
export function prefersReducedMotion(): boolean {
  try {
    return typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;
  } catch {
    return false;
  }
}
