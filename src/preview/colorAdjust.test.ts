import { describe, expect, it } from 'vitest';
import fixture from '../../src-tauri/crates/deadlock-materials/tests/fixtures/color-adjust.json';
import { adjustPixels, adjustRgb } from './colorAdjust';

describe('replacement color editor preview', () => {
  it('matches Rust Adjustment::apply on the shared fixture', () => {
    expect(fixture.cases.length).toBeGreaterThan(60);
    for (const item of fixture.cases) {
      const result = adjustRgb(item.rgb as [number, number, number], item, item.clamp);
      expect(result, `${item.name} ${item.rgb} ${item.hue}/${item.saturation}/${item.brightness}`).toEqual(item.expected);
    }
  });

  it('matches the build 8-bit image path byte for byte, alpha untouched', () => {
    for (const item of fixture.eight_bit.adjustments) {
      const data = new Uint8ClampedArray(fixture.eight_bit.pixels);
      adjustPixels(data, item.adjustment);
      expect([...data], JSON.stringify(item.adjustment)).toEqual(item.expected);
    }
  });

  it('defaults to clamping and leaves pixels alone without a change', () => {
    const brighter = adjustRgb([0.8, 0.4, 0.2], { hue: 0, saturation: 1, brightness: 2 });
    [1, 0.5, 0.25].forEach((value, i) => expect(brighter[i]).toBeCloseTo(value, 12));
    const data = new Uint8ClampedArray([1, 2, 3, 4]);
    adjustPixels(data, { hue: 0, saturation: 1, brightness: 1 });
    expect([...data]).toEqual([1, 2, 3, 4]);
  });
});
