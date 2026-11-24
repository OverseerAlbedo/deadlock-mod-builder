import { afterEach, describe, expect, it, vi } from "vitest";
import { FADE_IN, GLOW_CYAN, GLOW_YELLOW, PULSES, PULSE_SECONDS, STEADY, dimLevel, fadeOut, glowColorFor, glowLevel, prefersReducedMotion, pulsing } from "./colorGlow";

afterEach(() => vi.unstubAllGlobals());

describe("Colors: the pointing glow", () => {
  it("pulses twice, then stays at a soft steady glow", () => {
    const peaks = [0.5, 1.5].map((k) => k * PULSE_SECONDS);
    for (const t of peaks) expect(glowLevel(t, false)).toBeCloseTo(1, 5);
    expect(glowLevel(PULSE_SECONDS, false)).toBeCloseTo(STEADY, 5);
    expect(glowLevel(PULSE_SECONDS * PULSES + 0.01, false)).toBe(STEADY);
    expect(glowLevel(5, false)).toBe(STEADY);
    expect(glowLevel(0, false)).toBe(0);
    expect(pulsing(0.5, false)).toBe(true);
    expect(pulsing(PULSE_SECONDS * PULSES, false)).toBe(false);
    expect(dimLevel(FADE_IN)).toBe(1);
    expect(fadeOut(0)).toBe(1);
    expect(fadeOut(1)).toBe(0);
  });
  it("with reduced motion shows the steady glow without pulsing", () => {
    for (const t of [0.3, PULSE_SECONDS / 2, PULSE_SECONDS * 1.5, 4]) expect(glowLevel(t, true)).toBeLessThanOrEqual(STEADY);
    expect(glowLevel(PULSE_SECONDS / 2, true)).toBe(STEADY);
    expect(pulsing(FADE_IN, true)).toBe(false);
    vi.stubGlobal("matchMedia", (query: string) => ({ matches: query.includes("reduce") }));
    expect(prefersReducedMotion()).toBe(true);
    vi.stubGlobal("matchMedia", () => ({ matches: false }));
    expect(prefersReducedMotion()).toBe(false);
  });
  it("glows yellow on blue-green materials and cyan elsewhere", () => {
    expect(glowColorFor([0.8, 0.2, 0.2])).toEqual(GLOW_CYAN);
    expect(glowColorFor([0.1, 0.6, 0.7])).toEqual(GLOW_YELLOW);
    expect(glowColorFor([0.2, 0.3, 0.9])).toEqual(GLOW_YELLOW);
    expect(glowColorFor([0.5, 0.5, 0.52])).toEqual(GLOW_CYAN);
    expect(glowColorFor(null)).toEqual(GLOW_CYAN);
  });
});
