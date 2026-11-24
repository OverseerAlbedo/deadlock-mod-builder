import { describe, expect, it } from "vitest";
import fixture from "../../src-tauri/crates/deadlock-components/tests/fixtures/color-parts.json";
import { area, atSpot, mask, MaterialParts, pieces, rangeMasks, type PartMesh } from "./colorParts";
import { applyRecipe, selectionPixels } from "./colorPixels";
import { selectionOutline } from "./ColorTextureView";
import { rangeCoverage } from "./colorCoverage";
import { recolorTexel, withDefaults } from "./heroColorRanges";

const meshes: PartMesh[] = fixture.meshes.map((m) => ({ positions: new Float32Array(m.positions), uvs: new Float32Array(m.uvs), indices: new Uint32Array(m.indices) }));
const rows = (data: Uint8Array, width: number) => Array.from({ length: data.length / width }, (_, y) => [...data.slice(y * width, (y + 1) * width)].join(""));

describe("Colors: the part rule", () => {
  it("matches the Rust build on the shared fixture, bit for bit", () => {
    const numbers = pieces(meshes);
    expect(numbers.map((list) => [...list])).toEqual(fixture.pieces);
    for (const spot of fixture.spots) {
      const chosen = atSpot(meshes, numbers, spot.u, spot.v);
      expect(chosen, `${spot.u},${spot.v}`).toEqual(spot.pieces);
      const triangles = area(meshes, numbers, chosen);
      expect(triangles.length).toBe(spot.triangles);
      for (const size of spot.masks) expect(rows(mask(triangles, size.width, size.height), size.width), `${spot.u},${spot.v} ${size.width}x${size.height}`).toEqual(size.rows);
    }
  });
  it("draws, lights, outlines and measures a range for one part only inside its part", () => {
    // 16 × 16 pixels of one blue; the part at (0.25, 0.25) is the two joined quads' area.
    const parts = new MaterialParts(meshes);
    const image = () => { const data = new Uint8ClampedArray(16 * 16 * 4); for (let i = 0; i < data.length; i += 4) data.set([61, 111, 214, 255], i); return data; };
    const range = withDefaults({ hue: 220, hue_width: 20, saturation_range: [0.3, 1], hue_shift: 180, part: { u: 0.25, v: 0.25 } });
    const inside = mask(area(meshes, pieces(meshes), atSpot(meshes, pieces(meshes), 0.25, 0.25)), 16, 16);
    const turned = recolorTexel([range], [61, 111, 214, 255]);
    // The recolor (as the 3D view and the Texture view draw it).
    const drawn = image();
    applyRecipe(drawn, { ranges: [range], adjust: null, approximate: null }, undefined, rangeMasks([range], parts, 16, 16));
    for (let p = 0; p < 256; p++) expect([...drawn.slice(p * 4, p * 4 + 4)], `pixel ${p}`).toEqual(inside[p] ? turned : [61, 111, 214, 255]);
    // The same pass with the selection glow's map, at step 1.
    const lit = new Uint8ClampedArray(16 * 16 * 4);
    const both = image();
    applyRecipe(both, { ranges: [range], adjust: null, approximate: null }, { range, out: lit, width: 16, step: 1, mask: rangeMasks([range], parts, 16, 16)![0] }, rangeMasks([range], parts, 16, 16));
    expect(both).toEqual(drawn);
    for (let p = 0; p < 256; p++) expect(lit[p * 4], `glow ${p}`).toBe(inside[p] ? 255 : 0);
    const alone = new Uint8ClampedArray(16 * 16 * 4);
    selectionPixels({ range, out: alone, width: 16, step: 1, mask: rangeMasks([range], parts, 16, 16)![0] }, image());
    expect(alone).toEqual(lit);
    // The Texture view's outline: only inside the part.
    const outline = new Uint8ClampedArray(16 * 16 * 4);
    selectionOutline({ data: image(), width: 16, height: 16 }, range, [0, 1, 1], outline, rangeMasks([range], parts, 16, 16)![0]);
    for (let p = 0; p < 256; p++) if (!inside[p]) expect(outline[p * 4 + 3], `outline ${p}`).toBe(0);
    expect(outline.some((v, i) => i % 4 === 3 && v > 0)).toBe(true);
    // "selects N%": the part's share; without the model's parts a part range selects nothing.
    const shown = { data: image(), width: 16, height: 16 };
    const share = inside.reduce((sum, v) => sum + v, 0) / 256;
    expect(rangeCoverage(range, shown, parts).fraction).toBeCloseTo(share, 10);
    expect(rangeCoverage(range, { ...shown }, null).fraction).toBe(0);
    const { part: _part, ...everywhere } = range;
    expect(rangeCoverage(everywhere, { ...shown }, parts).fraction).toBe(1);
  });
  it("keeps a material's pieces and masks for the page", () => {
    const parts = new MaterialParts(meshes);
    expect(parts.at(0.5, 0.52)).toBeNull();
    expect(parts.at(0.25, 0.75)?.pieces).toEqual([1, 2]);
    const first = parts.mask(0.25, 0.25, 16, 16)!;
    expect(parts.mask(0.25, 0.25, 16, 16)).toBe(first);
    expect(rows(first, 16)).toEqual(fixture.spots[0].masks[0].rows);
  });
});
