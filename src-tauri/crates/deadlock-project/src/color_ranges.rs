//! Color ranges: which base-color pixels a range selects, and how it changes
//! them. A range selects by hue (a centre and a half width that wraps around
//! red), saturation and brightness (HSV value), with a soft edge, and shifts the
//! hue and scales saturation and brightness like the replacement model color
//! editor (`deadlock-materials color_adjust`). Every range selects by the
//! original color; the changes of several ranges add up in order. Alpha is never
//! touched. A range may also (switches in `Switches`, both on) give what it selects a new
//! hue and saturation and lift its brightness (`set_hue`, `set_saturation`,
//! `lift`: grey, white and black have no hue to shift), and may reach only the
//! part of the model it was picked on (`part`, `crate::color_parts`). A range
//! without these fields gives exactly the colors it gave before they existed.
//!
//! The one build copy of the function: hero colors
//! (`deadlock-components color_ranges`, which re-exports it) and replacement
//! materials (`deadlock-materials color_adjust`) both use it. The same function
//! is `src/extras/heroColorRanges.ts` (the preview); both tests check
//! `deadlock-components/tests/fixtures/hero-color-ranges.json`.
use crate::customization::HeroColorRange;
use serde::{Deserialize, Serialize};

/// Degrees of hue and fraction of saturation or brightness that softness 1 fades over.
pub const HUE_FADE: f64 = 30.;
pub const LEVEL_FADE: f64 = 0.15;
/// Ranges one material holds at most.
pub const MAX_RANGES: usize = 8;
/// How far a range's `lift` may move brightness (fraction of full brightness).
pub const LIFT_LIMIT: f64 = 1.;
/// How far from the image a part's spot may lie (texture coordinates repeat).
pub const PART_LIMIT: f64 = 1000.;

/// What color ranges may do beyond selecting and shifting; each has its own
/// switch (both are on in the app):
/// - `set_colour`: a range gives the colors it selects a new hue and
///   saturation (`set_hue`, `set_saturation`) and may lift their brightness
///   (`lift`), so grey, white and black can be recolored;
/// - `part_only`: a range changes only the part of the model it was picked on
///   (`part`, `crate::color_parts`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Switches {
    pub set_colour: bool,
    pub part_only: bool,
}
impl Switches {
    pub const NONE: Self = Self { set_colour: false, part_only: false };
    pub const ALL: Self = Self { set_colour: true, part_only: true };
}
/// The switches in the app: both abilities on (`src/extras/heroColorRanges.ts
/// COLOR_SWITCHES` must be equal; a test reads that file). The build still
/// refuses a range that needs a switch turned off here.
pub const SWITCHES: Switches = Switches::ALL;
#[cfg(feature = "color-test-switches")]
static TEST_SWITCHES: std::sync::Mutex<Option<Switches>> = std::sync::Mutex::new(None);
/// Other switches for an example or end-to-end test only (feature
/// `color-test-switches`, never enabled by the app's own build).
#[cfg(feature = "color-test-switches")]
pub fn open_for_test(switches: Option<Switches>) {
    *TEST_SWITCHES.lock().unwrap_or_else(|e| e.into_inner()) = switches;
}
/// The switches in force: `SWITCHES` (or a test's own, see `open_for_test`).
pub fn switches() -> Switches {
    #[cfg(feature = "color-test-switches")]
    if let Some(test) = *TEST_SWITCHES.lock().unwrap_or_else(|e| e.into_inner()) {
        return test;
    }
    SWITCHES
}
/// The build's refusal of a range that uses a switched-off ability.
pub const SET_COLOUR_OFF: &str = "This version of the app cannot give a color range a new color of its own yet. Remove that range, or recolor the part with a hue shift.";
pub const PART_ONLY_OFF: &str = "This version of the app cannot recolor one part only yet. Set that range to Everywhere this color, or remove it.";

/// The spot of the model a range was picked on (texture coordinates): the
/// range then changes only the image area of the part there (`color_parts`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ColorPart {
    pub u: f64,
    pub v: f64,
}

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
    /// A new color for what the range selects (switch `set_colour`): hue in
    /// degrees (0 to 360) and saturation (0 to 1), always set together; they
    /// replace `hue_shift` and `saturation`. Absent unless set, so ranges
    /// without them serialize (receipts, digests) exactly as before.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub set_hue: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub set_saturation: Option<f64>,
    /// Added to the brightness after `brightness` scales it (-1 to 1), so
    /// black can become a dark color (switch `set_colour`). Absent unless set
    /// and not 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lift: Option<f64>,
    /// Only the part of the model at this spot (switch `part_only`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part: Option<ColorPart>,
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
            set_hue: None,
            set_saturation: None,
            lift: None,
            part: None,
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
    /// A saved range with defaults for its unset values.
    pub fn from_choice(choice: &HeroColorRange) -> Self {
        let base = Self::default();
        Self {
            hue: choice.hue.unwrap_or(base.hue),
            hue_width: choice.hue_width.unwrap_or(base.hue_width),
            saturation_range: choice.saturation_range.unwrap_or(base.saturation_range),
            brightness_range: choice.brightness_range.unwrap_or(base.brightness_range),
            softness: choice.softness.unwrap_or(base.softness),
            hue_shift: choice.hue_shift.unwrap_or(base.hue_shift),
            saturation: choice.saturation.unwrap_or(base.saturation),
            brightness: choice.brightness.unwrap_or(base.brightness),
            set_hue: choice.set_hue,
            set_saturation: choice.set_saturation,
            // A lift of 0 is no lift: the range stays the one it was without it.
            lift: choice.lift.filter(|lift| *lift != 0.),
            part: choice.part,
        }
    }
    /// The range gives what it selects a new color (`set_hue` and `set_saturation`).
    pub fn gives_colour(&self) -> bool {
        self.set_hue.is_some() || self.set_saturation.is_some()
    }
    /// The range uses an ability behind `set_colour`.
    pub fn uses_set_colour(&self) -> bool {
        self.gives_colour() || self.lift.is_some_and(|lift| lift != 0.)
    }
    /// The range refused when a switch it needs is off (`switches()`).
    pub fn check_switches(&self, switches: Switches) -> std::result::Result<(), String> {
        if self.uses_set_colour() && !switches.set_colour {
            return Err(SET_COLOUR_OFF.into());
        }
        if self.part.is_some() && !switches.part_only {
            return Err(PART_ONLY_OFF.into());
        }
        Ok(())
    }
    /// A plain message when a value is outside its range.
    pub fn validate(&self) -> std::result::Result<(), String> {
        let check = |label: &str, value: f64, low: f64, high: f64| {
            if !value.is_finite() || value < low || value > high {
                Err(format!("A color range's {label} must be from {low} to {high}."))
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
                return Err(format!("A color range's {label} starts above its end."));
            }
        }
        check("softness", self.softness, 0., 1.)?;
        check("hue shift", self.hue_shift, -180., 180.)?;
        check("saturation", self.saturation, 0., 2.)?;
        check("brightness", self.brightness, 0., 2.)?;
        if self.set_hue.is_some() != self.set_saturation.is_some() {
            return Err(GIVEN_COLOUR_PAIR.into());
        }
        if let (Some(hue), Some(saturation)) = (self.set_hue, self.set_saturation) {
            check("new hue", hue, 0., 360.)?;
            check("new saturation", saturation, 0., 1.)?;
        }
        if let Some(lift) = self.lift {
            check("lift", lift, -LIFT_LIMIT, LIFT_LIMIT)?;
        }
        if let Some(part) = self.part {
            check("part", part.u, -PART_LIMIT, PART_LIMIT)?;
            check("part", part.v, -PART_LIMIT, PART_LIMIT)?;
        }
        Ok(())
    }
    /// The range would leave every color as it is. A range that gives a new
    /// color or lifts brightness always changes something (a grey turned to a
    /// grey of its own hue is still a change the user chose).
    pub fn changes_nothing(&self) -> bool {
        !self.uses_set_colour() && self.hue_shift.rem_euclid(360.) == 0. && self.saturation == 1. && self.brightness == 1.
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
    /// The changed color. Without `set_hue`, `set_saturation` and `lift` it is
    /// exactly the old shift and scale: hue + `hue_shift`, saturation ×
    /// `saturation`, brightness × `brightness` (both clamped to 0–1). With
    /// them: hue `set_hue`, saturation `set_saturation`, brightness
    /// clamp(v × `brightness` + `lift`, 0, 1).
    fn adjust(&self, rgb_value: [f64; 3]) -> [f64; 3] {
        let [h, s, v] = hsv(rgb_value);
        if !self.uses_set_colour() {
            return rgb([
                (h + self.hue_shift).rem_euclid(360.),
                (s * self.saturation).clamp(0., 1.),
                (v * self.brightness).clamp(0., 1.),
            ]);
        }
        let (hue, saturation) = match (self.set_hue, self.set_saturation) {
            (Some(hue), Some(saturation)) => (hue.rem_euclid(360.), saturation.clamp(0., 1.)),
            _ => ((h + self.hue_shift).rem_euclid(360.), (s * self.saturation).clamp(0., 1.)),
        };
        let lift = self.lift.unwrap_or(0.);
        rgb([hue, saturation, (v * self.brightness + lift).clamp(0., 1.)])
    }
}
/// The message when only one of a given color's two values is saved.
pub const GIVEN_COLOUR_PAIR: &str = "A color range's new color needs both its hue and its saturation.";

/// The enabled ranges of `rows` that change something, with defaults for unset values.
pub fn enabled(rows: &[HeroColorRange]) -> Vec<ColorRange> {
    rows.iter()
        .filter(|r| r.enabled.unwrap_or(true))
        .map(ColorRange::from_choice)
        .filter(|r| !r.changes_nothing())
        .collect()
}
/// Refuses ranges that use an ability whose switch is off (`switches()`).
pub fn check_switches(ranges: &[ColorRange]) -> std::result::Result<(), String> {
    let switches = switches();
    ranges.iter().try_for_each(|range| range.check_switches(switches))
}

/// One color (channels 0 to 1) after every range. A range's change keeps
/// brightness at most 1, and a color brighter than 1 (HDR) lies above every
/// brightness range, so it is selected only within the soft edge
/// (`softness × LEVEL_FADE` above 1) and left as it is beyond that.
pub fn recolor_rgb(ranges: &[ColorRange], original: [f64; 3]) -> [f64; 3] {
    recolor_rgb_where(ranges, original, |_| true)
}
/// `recolor_rgb`, with `inside(i)` false where range `i` does not reach (a
/// range with a `part` outside its part's image area): that range then
/// leaves the color as it is. With `inside` always true it is `recolor_rgb`.
pub fn recolor_rgb_where(ranges: &[ColorRange], original: [f64; 3], inside: impl Fn(usize) -> bool) -> [f64; 3] {
    let selection = hsv(original);
    let mut color = original;
    for (index, range) in ranges.iter().enumerate() {
        if !inside(index) {
            continue;
        }
        let weight = range.weight(selection);
        if weight <= 0. {
            continue;
        }
        let changed = range.adjust(color);
        for (value, target) in color.iter_mut().zip(changed) {
            *value += (target - *value) * weight;
        }
    }
    color
}

/// One 8-bit sRGB texel after every range; alpha is kept.
pub fn recolor(ranges: &[ColorRange], texel: [u8; 4]) -> [u8; 4] {
    recolor_where(ranges, texel, |_| true)
}
/// `recolor` with `inside` (see `recolor_rgb_where`).
pub fn recolor_where(ranges: &[ColorRange], texel: [u8; 4], inside: impl Fn(usize) -> bool) -> [u8; 4] {
    let color = recolor_rgb_where(ranges, [texel[0] as f64 / 255., texel[1] as f64 / 255., texel[2] as f64 / 255.], inside);
    let byte = |v: f64| (v * 255.).round().clamp(0., 255.) as u8;
    [byte(color[0]), byte(color[1]), byte(color[2]), texel[3]]
}
