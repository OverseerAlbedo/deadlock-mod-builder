/** The replacement model's color editor (hue, saturation, brightness) for the
 * preview: the same arithmetic as Rust `deadlock-materials color_adjust`
 * (`Adjustment::apply`), which the build uses on the base-color image. Both
 * tests check `src-tauri/crates/deadlock-materials/tests/fixtures/color-adjust.json`,
 * so keep the two identical. */

export interface ColorAdjustment {
  /** Degrees, -180–180. */
  hue: number;
  /** Factor, 0 (grey) to 2; 1 keeps the original. */
  saturation: number;
  /** Factor, 0 (black) to 2; 1 keeps the original. */
  brightness: number;
}

/** Rust `f64::rem_euclid`. */
const remEuclid = (x: number, m: number) => {
  const r = x % m;
  return r < 0 ? r + m : r;
};

/** One RGB color (channels 0–1) adjusted. `clamp` keeps the brightness at
 * most 1 (8- and 16-bit images); HDR colors may exceed one. */
export function adjustRgb(rgb: [number, number, number], a: ColorAdjustment, clamp = true): [number, number, number] {
  const [r, g, b] = rgb;
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const delta = max - min;
  let h = (delta <= 1e-12 ? 0 : max === r ? remEuclid((g - b) / delta, 6) : max === g ? (b - r) / delta + 2 : (r - g) / delta + 4) * 60;
  let s = max > 1e-12 ? delta / max : 0;
  let v = max;
  h = remEuclid(h + a.hue, 360);
  s = Math.min(1, Math.max(0, s * a.saturation));
  v *= a.brightness;
  if (clamp) v = Math.min(1, Math.max(0, v));
  const c = v * s;
  const x = c * (1 - Math.abs(remEuclid(h / 60, 2) - 1));
  const m = v - c;
  const sector = Math.floor(h / 60);
  const [rr, gg, bb] =
    sector === 0 ? [c, x, 0] : sector === 1 ? [x, c, 0] : sector === 2 ? [0, c, x] : sector === 3 ? [0, x, c] : sector === 4 ? [x, 0, c] : [c, 0, x];
  return [rr + m, gg + m, bb + m];
}

/** The build's 8-bit path: the image crate reads a byte as the f32 b / 255,
 * the adjusted value is stored as f32 and written back as round(clamp(v) × 255)
 * in f32. */
const toByte = (value: number) => {
  const f = Math.fround(value);
  const clamped = !(f < 1) ? 1 : Math.max(f, 0);
  return Math.round(Math.fround(clamped * 255));
};

/** RGBA bytes (canvas `ImageData.data`) adjusted in place; alpha is untouched. */
export function adjustPixels(data: Uint8ClampedArray, a: ColorAdjustment): void {
  if (a.hue === 0 && a.saturation === 1 && a.brightness === 1) return;
  for (let i = 0; i < data.length; i += 4) {
    const [r, g, b] = adjustRgb([Math.fround(data[i] / 255), Math.fround(data[i + 1] / 255), Math.fround(data[i + 2] / 255)], a, true);
    data[i] = toByte(r);
    data[i + 1] = toByte(g);
    data[i + 2] = toByte(b);
  }
}
