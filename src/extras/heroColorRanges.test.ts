import { describe, expect, it } from 'vitest';
import fixture from '../../src-tauri/crates/deadlock-components/tests/fixtures/hero-color-ranges.json';
import { changesNothing, COLOR_SWITCHES, colorSwitches, hsv, openColorSwitchesForTest, rangeAround, rangeWeight, recolorPixels, recolorTexel, switchRefusal, withDefaults, type Rgba } from './heroColorRanges';

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

  it('give a grey a new color, keep a range without the new fields as it was', () => {
    const violet = withDefaults({ saturation_range: [0, 0.2], brightness_range: [0.55, 1], set_hue: 270, set_saturation: 0.6 });
    const out = recolorTexel([violet], [200, 200, 200, 7]);
    const [h, s, v] = hsv(out[0] / 255, out[1] / 255, out[2] / 255);
    expect(Math.abs(h - 270)).toBeLessThan(2);
    expect(Math.abs(s - 0.6)).toBeLessThan(0.02);
    expect(Math.abs(v - 200 / 255)).toBeLessThan(0.01);
    expect(out[3]).toBe(7);
    // Unset and a lift of 0 add no key: the range is the one it always was.
    expect(withDefaults({ hue_shift: 30, lift: 0 })).toEqual({ ...withDefaults({}), hue_shift: 30 });
    expect(Object.keys(withDefaults({ hue: 3 })).sort()).toEqual(["brightness", "brightness_range", "hue", "hue_shift", "hue_width", "saturation", "saturation_range", "softness"]);
    expect(changesNothing(withDefaults({ set_hue: 0, set_saturation: 0 }))).toBe(false);
    expect(changesNothing(withDefaults({ lift: 0 }))).toBe(true);
    expect(changesNothing(withDefaults({ part: { u: 0.1, v: 0.2 } }))).toBe(true);
    // A range kept out of a texel leaves it to the others.
    const orange = withDefaults({ hue: 25, hue_width: 10, saturation_range: [0.5, 1], hue_shift: 180 });
    expect(recolorTexel([orange], [140, 80, 36, 9], () => false)).toEqual([140, 80, 36, 9]);
    expect(recolorTexel([orange], [140, 80, 36, 9], () => true)).toEqual(recolorTexel([orange], [140, 80, 36, 9]));
  });

  it('has both abilities on in the app, and refuses a range only for a switch turned off', () => {
    // The app's switch set (equal to Rust color_ranges::SWITCHES): both on.
    expect(COLOR_SWITCHES).toEqual({ set_colour: true, part_only: true });
    expect(colorSwitches()).toBe(COLOR_SWITCHES);
    expect(switchRefusal({ set_hue: 1, set_saturation: 1, lift: 0.2, part: { u: 0, v: 0 } })).toBe("");
    // The rule itself, for a switch turned off.
    const off = { set_colour: false, part_only: false };
    expect(switchRefusal({ set_hue: 1, set_saturation: 1 }, off)).toBe("set_colour");
    expect(switchRefusal({ lift: 0.2 }, off)).toBe("set_colour");
    expect(switchRefusal({ lift: 0 }, off)).toBe("");
    expect(switchRefusal({ hue_shift: 30, part: { u: 0, v: 0 } }, off)).toBe("part_only");
    expect(switchRefusal({ hue_shift: 30, part: { u: 0, v: 0 } }, { set_colour: false, part_only: true })).toBe("");
    openColorSwitchesForTest(off);
    expect(switchRefusal({ set_hue: 1, set_saturation: 1 })).toBe("set_colour");
    openColorSwitchesForTest(null);
    expect(colorSwitches()).toBe(COLOR_SWITCHES);
  });

  it('pick a range around a color, all hues for greys', () => {
    expect(rangeAround([140, 80, 36])).toMatchObject({ hue: 25, hue_width: 15, saturation_range: [0.54, 0.94], brightness_range: [0.35, 0.75] });
    expect(rangeAround([30, 30, 34], { hue_shift: 40 })).toMatchObject({ hue_width: 180, hue_shift: 40 });
  });
});
