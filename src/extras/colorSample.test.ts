import { describe, expect, it } from "vitest";
import { choosePick, PICK_RAYS, samplesAround, type Cast, type RayHit } from "./colorSample";

// Meshes: 0 the face, 1 a smoke shell just in front of it, 2 the hair farther in front, 3 a hand.
const materialOf = (mesh: number) => ["face", "smoke", "hair", "hand"][mesh] ?? null;
const hit = (mesh: number, distance: number, u = 0.5, v = 0.5): RayHit => ({ mesh, distance, u, v });

describe("Colors picking on the character: which surface is picked", () => {
  it("picks the nearest surface when it is the selected material, or nothing is selected", () => {
    expect(choosePick([hit(0, 10)], materialOf, "face", 1)).toEqual({ hit: hit(0, 10), behind: false });
    expect(choosePick([hit(1, 9.8), hit(0, 10)], materialOf, null, 1)).toEqual({ hit: hit(1, 9.8), behind: false });
    expect(choosePick([], materialOf, "face", 1)).toBeNull();
  });
  it("picks the selected material through a layer of another material just in front of it", () => {
    expect(choosePick([hit(1, 9.8), hit(0, 10)], materialOf, "face", 1)).toEqual({ hit: hit(0, 10), behind: true });
  });
  it("keeps the nearest surface when the selected material is not under the pointer, or lies farther behind", () => {
    expect(choosePick([hit(3, 8)], materialOf, "face", 1)).toEqual({ hit: hit(3, 8), behind: false });
    expect(choosePick([hit(2, 7), hit(0, 10)], materialOf, "face", 1)).toEqual({ hit: hit(2, 7), behind: false });
  });
});

describe("Colors picking on the character: the area read around the click", () => {
  const offsets = (cast: Cast) => {
    const seen: [number, number][] = [];
    return { seen, cast: ((dx, dy) => { seen.push([dx, dy]); return cast(dx, dy); }) as Cast };
  };
  it("reads a square of about 3 × 3 screen pixels, the click's own coordinate first", () => {
    const { seen, cast } = offsets((dx, dy) => [hit(0, 10, 0.5 + dx / 100, 0.5 + dy / 100)]);
    const samples = samplesAround(cast, hit(0, 10), false, 1);
    expect(samples).toHaveLength(PICK_RAYS * PICK_RAYS);
    expect(samples[0]).toEqual({ u: 0.5, v: 0.5 });
    const xs = seen.map(([dx]) => dx);
    expect(Math.min(...xs)).toBeCloseTo(-1.2);
    expect(Math.max(...xs)).toBeCloseTo(1.2);
  });
  it("leaves out rays that land on another part, so a border never mixes two parts", () => {
    // Right of the click is the hand.
    const cast: Cast = (dx) => (dx > 0 ? [hit(3, 8, 0.9, 0.9), hit(0, 10)] : [hit(0, 10, 0.4, 0.5)]);
    const samples = samplesAround(cast, hit(0, 10), false, 1);
    expect(samples).toHaveLength(3 * PICK_RAYS);
    expect(samples.every((s) => s.u < 0.6)).toBe(true);
  });
  it("reads through the layer in front for a pick behind it, within the same depth", () => {
    const cast: Cast = (dx) => (dx > 0 ? [hit(2, 7), hit(0, 10, 0.6, 0.5)] : [hit(1, 9.8), hit(0, 10, 0.4, 0.5)]);
    const samples = samplesAround(cast, hit(0, 10), true, 1);
    expect(samples).toHaveLength(1 + 3 * PICK_RAYS - 1);
    expect(samples.every((s) => s.u < 0.6)).toBe(true);
  });
});
