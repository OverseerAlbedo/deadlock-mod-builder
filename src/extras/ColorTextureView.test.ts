import { describe, expect, it } from "vitest";
import { withDefaults } from "./heroColorRanges";
import { selectionOutline } from "./ColorTextureView";

describe("Colors: the selection outline in the Texture view", () => {
  it("outlines what a range selects with a light fill inside and nothing outside", () => {
    // 6 × 5: the first five columns red (selected), the last one blue (not).
    const data = new Uint8ClampedArray(6 * 5 * 4);
    for (let p = 0; p < 30; p++) data.set(p % 6 === 5 ? [40, 60, 220, 255] : [220, 40, 40, 255], p * 4);
    const out = new Uint8ClampedArray(data.length);
    selectionOutline({ data, width: 6, height: 5 }, withDefaults({ hue: 0, hue_width: 20 }), [0.24, 0.95, 1], out);
    const alpha = (x: number, y: number) => out[(y * 6 + x) * 4 + 3];
    expect(alpha(0, 0)).toBe(255);
    expect(alpha(2, 2)).toBe(72);
    expect(alpha(3, 2)).toBe(255); // two pixels from the unselected column
    expect(alpha(5, 2)).toBe(0);
    expect([...out.slice(0, 3)]).toEqual([61, 242, 255]);
  });
});
