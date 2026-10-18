//! Hero color ranges: which base-color pixels a range selects, and how it
//! changes them. A range selects by hue (a centre and a half width that wraps
//! around red), saturation and brightness (HSV value), with a soft edge, and
//! shifts the hue and scales saturation and brightness like the replacement
//! model color editor (`deadlock-materials color_adjust`). Every range selects by
//! the game's original color; the changes of several ranges add up in order.
//! Alpha is never touched. The same function is `src/extras/heroColorRanges.ts`
//! (the Extras preview); both tests check `tests/fixtures/hero-color-ranges.json`.
use crate::{Result, fail};
use serde::{Deserialize, Serialize};

/// Degrees of hue and fraction of saturation or brightness that softness 1 fades over.
pub const HUE_FADE: f64 = 30.;
pub const LEVEL_FADE: f64 = 0.15;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ColorRange {
    /// Centre hue in degrees, 0 to 360.
    pub hue: f64,
    /// Half width in degrees, 0 to 180; 180 selects every hue.
    pub hue_width: f64,
    pub saturation_range: [f64; 2],
    pub brightness_range: [f64; 2],
    /// 0 is a hard edge; 1 fades over `HUE_FADE` and `LEVEL_FADE` outside the range.
    pub softness: f64,
    /// Degrees, -180 to 180.
    pub hue_shift: f64,
    /// Factors, 0 to 2; 1 keeps the original.
    pub saturation: f64,
    pub brightness: f64,
}
impl Default for ColorRange {
    fn default() -> Self {
        Self {
            hue: 0.,
            hue_width: 180.,
            saturation_range: [0., 1.],
            brightness_range: [0., 1.],
            softness: 0.25,
            hue_shift: 0.,
            saturation: 1.,
            brightness: 1.,
        }
    }
}

fn smooth(fade: f64, inside: f64) -> f64 {
    if inside >= 0. {
        1.
    } else if fade <= 0. || inside <= -fade {
        0.
    } else {
        let t = 1. + inside / fade;
        t * t * (3. - 2. * t)
    }
}

pub fn hsv(rgb: [f64; 3]) -> [f64; 3] {
    let [r, g, b] = rgb;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let h = if delta <= 1e-12 {
        0.
    } else if max == r {
        ((g - b) / delta).rem_euclid(6.)
    } else if max == g {
        (b - r) / delta + 2.
    } else {
        (r - g) / delta + 4.
    } * 60.;
    [h, if max > 1e-12 { delta / max } else { 0. }, max]
}
pub fn rgb([h, s, v]: [f64; 3]) -> [f64; 3] {
    let c = v * s;
    let x = c * (1. - ((h / 60.).rem_euclid(2.) - 1.).abs());
    let m = v - c;
    let (r, g, b) = match (h / 60.).floor() as i32 {
        0 => (c, x, 0.),
        1 => (x, c, 0.),
        2 => (0., c, x),
        3 => (0., x, c),
        4 => (x, 0., c),
        _ => (c, 0., x),
    };
    [r + m, g + m, b + m]
}

impl ColorRange {
    pub fn validate(&self) -> Result<()> {
        let check = |label: &str, value: f64, low: f64, high: f64| {
            if !value.is_finite() || value < low || value > high {
                Err(fail(format!("A color range's {label} must be from {low} to {high}.")))
            } else {
                Ok(())
            }
        };
        check("hue", self.hue, 0., 360.)?;
        check("hue width", self.hue_width, 0., 180.)?;
        for (label, [low, high]) in [("saturation range", self.saturation_range), ("brightness range", self.brightness_range)] {
            check(label, low, 0., 1.)?;
            check(label, high, 0., 1.)?;
            if low > high {
                return Err(fail(format!("A color range's {label} starts above its end.")));
            }
        }
        check("softness", self.softness, 0., 1.)?;
        check("hue shift", self.hue_shift, -180., 180.)?;
        check("saturation", self.saturation, 0., 2.)?;
        check("brightness", self.brightness, 0., 2.)
    }
    /// The range would leave every color as it is.
    pub fn changes_nothing(&self) -> bool {
        self.hue_shift.rem_euclid(360.) == 0. && self.saturation == 1. && self.brightness == 1.
    }
    /// How much the range selects a color given as HSV (0 to 1).
    pub fn weight(&self, [h, s, v]: [f64; 3]) -> f64 {
        let hue = if self.hue_width >= 180. {
            1.
        } else {
            let distance = ((h - self.hue + 180.).rem_euclid(360.) - 180.).abs();
            smooth(self.softness * HUE_FADE, self.hue_width - distance)
        };
        let level = |value: f64, [low, high]: [f64; 2]| {
            smooth(self.softness * LEVEL_FADE, value - low) * smooth(self.softness * LEVEL_FADE, high - value)
        };
        hue * level(s, self.saturation_range) * level(v, self.brightness_range)
    }
    fn adjust(&self, rgb_value: [f64; 3]) -> [f64; 3] {
        let [h, s, v] = hsv(rgb_value);
        rgb([
            (h + self.hue_shift).rem_euclid(360.),
            (s * self.saturation).clamp(0., 1.),
            (v * self.brightness).clamp(0., 1.),
        ])
    }
}

/// One 8-bit sRGB texel after every range; alpha is kept.
pub fn recolor(ranges: &[ColorRange], texel: [u8; 4]) -> [u8; 4] {
    let original = [texel[0] as f64 / 255., texel[1] as f64 / 255., texel[2] as f64 / 255.];
    let selection = hsv(original);
    let mut color = original;
    for range in ranges {
        let weight = range.weight(selection);
        if weight <= 0. {
            continue;
        }
        let changed = range.adjust(color);
        for (value, target) in color.iter_mut().zip(changed) {
            *value += (target - *value) * weight;
        }
    }
    let byte = |v: f64| (v * 255.).round().clamp(0., 255.) as u8;
    [byte(color[0]), byte(color[1]), byte(color[2]), texel[3]]
}
