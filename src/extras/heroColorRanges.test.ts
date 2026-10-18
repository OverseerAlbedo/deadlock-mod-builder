import { describe, expect, it } from 'vitest';
import fixture from '../../src-tauri/crates/deadlock-components/tests/fixtures/hero-color-ranges.json';
import { hsv, rangeAround, rangeWeight, recolorPixels, recolorTexel, withDefaults, type Rgba } from './heroColorRanges';

describe('hero color ranges', () => {
  it('match the Rust build function on the shared fixture', () => {
    for (const item of fixture.cases) {
      const ranges = item.ranges.map((range) => withDefaults(range as Parameters<typeof withDefaults>[0]));
      item.colors.forEach((color, index) => {
        expect(recolorTexel(ranges, color as Rgba), `${item.name} ${color}`).toEqual(item.expected[index]);
      });
    }
  });

  it('select by hue, saturation and brightness and keep alpha', () => {
    const orange = withDefaults({ hue: 25, hue_width: 10, saturation_range: [0.5, 1], brightness_range: [0.2, 1], softness: 0.5, hue_shift: 180 });
    expect(rangeWeight(orange, hsv(140 / 255, 80 / 255, 36 / 255))).toBe(1);
    expect(rangeWeight(orange, hsv(181 / 255, 160 / 255, 148 / 255))).toBe(0);
    const turned = recolorTexel([orange], [140, 80, 36, 9]);
    expect(turned[2]).toBeGreaterThan(turned[0]);
    expect(turned[3]).toBe(9);
    expect(recolorTexel([orange], [181, 160, 148, 0])).toEqual([181, 160, 148, 0]);
  });

  it('recolor canvas pixels or show the selection mask', () => {
    const orange = withDefaults({ hue: 25, hue_width: 10, saturation_range: [0.5, 1], hue_shift: 180 });
    const pixels = new Uint8ClampedArray([140, 80, 36, 255, 181, 160, 148, 255]);
    recolorPixels([orange], pixels, true);
    expect([...pixels]).toEqual([255, 255, 255, 255, 0, 0, 0, 255]);
    const colors = new Uint8ClampedArray([140, 80, 36, 255]);
    recolorPixels([orange], colors);
    expect(colors[2]).toBeGreaterThan(colors[0]);
  });

  it('pick a range around a color, all hues for greys', () => {
    expect(rangeAround([140, 80, 36])).toMatchObject({ hue: 25, hue_width: 15, saturation_range: [0.54, 0.94], brightness_range: [0.35, 0.75] });
    expect(rangeAround([30, 30, 34], { hue_shift: 40 })).toMatchObject({ hue_width: 180, hue_shift: 40 });
  });
});
