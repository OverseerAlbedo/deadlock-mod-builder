//! Colors of an effect tree for the Effects editor: color strips a page can
//! draw without knowing particle internals, the tree's palette, its main
//! color, the edit a page sends (`TreeEdit`) and the one copy of the color
//! maths that both the preview and an edited file use (`TreeEdit::color`).
//!
//! Strips: every color slot `effect_edit::colors` lists belongs to one strip
//! of a part: `constant` (one color), `random` (two colors, each particle
//! picks between them), `fade` (from the particle's starting color to another
//! over its life) or `gradient` (stops at positions). A stop names its slot
//! (the slot's place in the file); a fade's start is an estimate taken from the
//! part's own starting color and names that slot as its `source`.
//!
//! Weights estimate how much a color shows: the part's particle count (square
//! root, so many small sparks do not outweigh one large glow), its radius, its
//! renderer (sprites, ropes and trails fully; models, projected decals and
//! status-effect shaders at half; lights at a quarter; none at zero), the
//! renderers' overbright factor (square root, at most 16) and alpha scale, the
//! slot's alpha, and for additive renderers the color's own brightness (black
//! adds nothing). Within a part: a gradient stop counts for the share of the
//! gradient it covers, a random pair half each, a fade target for the share of
//! the life it shows, the system color fully only when nothing sets the
//! color (0.05 when an initializer or a color-setting operator overwrites it,
//! 0.3 when an operator only scales it), an outline 0.2, a lighting tint by
//! its tint share.
use crate::effect_edit::{self, ColorSlot, ColorSource, EffectEdit, Location, Multiplier, PartControls, Step};
use crate::{Result, fail};
use deadlock_resource::{Kind, Value};
use serde::Serialize;
use std::collections::BTreeSet;

/// Channels may differ by this many levels (of 255) and still be one palette
/// color; a replacement's `from` matches a slot within the same distance.
pub const MERGE_LEVELS: u8 = 6;
/// Below this HSV saturation a color is grey (or white).
pub const GREY_SATURATION: f64 = 0.12;
/// Below this HSV value a color is black.
pub const BLACK_VALUE: f64 = 0.15;
/// White: grey and at least this bright.
pub const WHITE_VALUE: f64 = 0.9;
/// `near` replacements: hue within this many degrees …
pub const NEAR_HUE: f64 = 20.;
/// … and saturation and value within this much (0–1 scale).
pub const NEAR_SV: f64 = 0.3;
/// Two greys are near when their values differ by at most this.
pub const NEAR_GREY_VALUE: f64 = 0.12;
/// A main color candidate needs at least this saturation and value.
pub const MAIN_SATURATION: f64 = 0.25;
pub const MAIN_VALUE: f64 = 0.2;
/// Candidates within this hue distance of a family's heaviest color join it.
pub const FAMILY_HUE: f64 = 30.;

// ---------------------------------------------------------------- HSV

/// Hue (degrees, 0 for greys), saturation and value (0–1), as `shift_color`.
pub fn hsv(rgb: [u8; 3]) -> (f64, f64, f64) {
    let [r, g, b] = rgb.map(|c| f64::from(c) / 255.);
    let max = r.max(g).max(b);
    let delta = max - r.min(g).min(b);
    let h = if delta == 0. {
        0.
    } else if max == r {
        60. * ((g - b) / delta).rem_euclid(6.)
    } else if max == g {
        60. * ((b - r) / delta + 2.)
    } else {
        60. * ((r - g) / delta + 4.)
    };
    (h, if max == 0. { 0. } else { delta / max }, max)
}
/// The color of hue `h` (degrees), saturation and value (clamped to 0–1).
pub fn from_hsv(h: f64, s: f64, v: f64) -> [u8; 3] {
    let h = h.rem_euclid(360.);
    let (s, v) = (s.clamp(0., 1.), v.clamp(0., 1.));
    let c = v * s;
    let x = c * (1. - ((h / 60.).rem_euclid(2.) - 1.).abs());
    let m = v - c;
    let (r, g, b) = match (h / 60.).floor() as u8 {
        0 => (c, x, 0.),
        1 => (x, c, 0.),
        2 => (0., c, x),
        3 => (0., x, c),
        4 => (x, 0., c),
        _ => (c, 0., x),
    };
    [r, g, b].map(|v| ((v + m) * 255.).round().clamp(0., 255.) as u8)
}
pub fn hex(rgb: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2])
}
fn hue_distance(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(360.);
    d.min(360. - d)
}
fn levels(a: [u8; 3], b: [u8; 3]) -> u8 {
    a.iter().zip(b).map(|(x, y)| x.abs_diff(y)).max().unwrap_or(0)
}
/// `grey`, `white` or `black` for neutral colors, else `None`.
pub fn neutral(rgb: [u8; 3]) -> Option<&'static str> {
    let (_, s, v) = hsv(rgb);
    if v < BLACK_VALUE {
        Some("black")
    } else if s < GREY_SATURATION {
        Some(if v >= WHITE_VALUE { "white" } else { "grey" })
    } else {
        None
    }
}
/// The `near` rule: two blacks; two greys whose values differ by at most
/// `NEAR_GREY_VALUE`; or two colors within `NEAR_HUE` degrees of hue and
/// `NEAR_SV` of saturation and of value. A grey is never near a color.
pub fn near(a: [u8; 3], b: [u8; 3]) -> bool {
    let ((ha, sa, va), (hb, sb, vb)) = (hsv(a), hsv(b));
    if va < BLACK_VALUE || vb < BLACK_VALUE {
        return va < BLACK_VALUE && vb < BLACK_VALUE;
    }
    match (sa < GREY_SATURATION, sb < GREY_SATURATION) {
        (true, true) => (va - vb).abs() <= NEAR_GREY_VALUE,
        (false, false) => hue_distance(ha, hb) <= NEAR_HUE && (sa - sb).abs() <= NEAR_SV && (va - vb).abs() <= NEAR_SV,
        _ => false,
    }
}

// ---------------------------------------------------------------- the edit

/// The saved edit types live in `deadlock_project::effect_edit` (the project
/// file holds them); `TreeEditExt` adds the color maths here.
pub use deadlock_project::effect_edit::{ColorReplacement, PartSettings, TreeEdit, MAX_COLOR_REPLACEMENTS};
/// What decided a slot's new color.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Rule {
    /// A replacement's `from` matched (within `MERGE_LEVELS`): it is `to`.
    Replaced,
    /// A `near` replacement moved it with `from`.
    Near,
    /// The whole-effect shift.
    Shifted,
    /// Its part is excluded: it keeps the game's colors entirely.
    Excluded,
    /// Nothing changes it.
    Unchanged,
}
/// Why a part is left out of an edit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LeftOut {
    /// The user left it out (`excluded`): nothing about it changes.
    User,
    /// It lies outside the hero's own folders (shared with other heroes) and
    /// the user did not choose it (`shared`): nothing about it changes.
    Shared,
}
/// The color maths of a `TreeEdit`.
pub trait TreeEditExt {
    /// Whether and why a part is left out; `outside_hero` is
    /// `effect_trees::outside_hero` for the part. `excluded` wins over `shared`.
    fn left_out(&self, part: &str, outside_hero: bool) -> Option<LeftOut>;
    /// `TreeEdit::validate` as this crate's error.
    fn check(&self) -> Result<()>;
    /// The whole-effect shift as the engine's edit (no multipliers).
    fn shift(&self) -> EffectEdit;
    fn is_excluded(&self, part: &str) -> bool;
    /// The new color of a slot color `rgb` in a part, and what decided it:
    /// an excluded part keeps every color; else a replacement whose `from`
    /// matches (within `MERGE_LEVELS`) gives `to`, a `near` replacement moves a
    /// close color with it, and otherwise the whole-effect shift applies. The
    /// one copy of this rule: the preview and the edited file both use it.
    fn color(&self, excluded: bool, rgb: [u8; 3]) -> ([u8; 3], Rule);
    /// The renderer multipliers other than 1.
    fn multipliers(&self) -> Vec<(Multiplier, f64)>;
    fn multiplier(&self, m: Multiplier) -> f64;
    /// One part of the tree with this edit applied (DATA only; the engine's
    /// checks, `validate_effect` included): a left-out part gets no change
    /// at all. The build writes this.
    fn edit_part(&self, raw: &[u8], part: &str, outside_hero: bool) -> Result<effect_edit::EditedFile>;
    /// The part's own settings (`TreeEdit.parts`, case-blind) as the engine
    /// applies them; neutral when it has none. `edit_part` applies them only
    /// to a part that is not left out.
    fn controls(&self, part: &str) -> PartControls;
}
impl TreeEditExt for TreeEdit {
    fn check(&self) -> Result<()> {
        self.validate().map_err(fail)
    }
    fn shift(&self) -> EffectEdit {
        EffectEdit { hue: self.hue, saturation: self.saturation, brightness: self.brightness, ..Default::default() }
    }
    fn is_excluded(&self, part: &str) -> bool {
        let part = deadlock_core::text::casefold(part);
        self.excluded.iter().any(|p| deadlock_core::text::casefold(p) == part)
    }
    fn left_out(&self, part: &str, outside_hero: bool) -> Option<LeftOut> {
        if self.is_excluded(part) {
            return Some(LeftOut::User);
        }
        let key = deadlock_core::text::casefold(part);
        (outside_hero && !self.shared.iter().any(|p| deadlock_core::text::casefold(p) == key)).then_some(LeftOut::Shared)
    }
    fn color(&self, excluded: bool, rgb: [u8; 3]) -> ([u8; 3], Rule) {
        if excluded {
            return (rgb, Rule::Excluded);
        }
        let exact = self
            .colors
            .iter()
            .filter(|r| levels(r.from, rgb) <= MERGE_LEVELS)
            .min_by_key(|r| levels(r.from, rgb));
        if let Some(r) = exact {
            return (r.to, Rule::Replaced);
        }
        let close = self
            .colors
            .iter()
            .filter(|r| r.near && near(r.from, rgb))
            .min_by(|a, b| distance(a.from, rgb).total_cmp(&distance(b.from, rgb)));
        if let Some(r) = close {
            return (follow(r.from, r.to, rgb), Rule::Near);
        }
        let next = effect_edit::shift_color(rgb, &self.shift());
        (next, if next == rgb { Rule::Unchanged } else { Rule::Shifted })
    }
    fn multipliers(&self) -> Vec<(Multiplier, f64)> {
        [(Multiplier::Size, self.size), (Multiplier::Opacity, self.opacity), (Multiplier::Glow, self.glow)]
            .into_iter()
            .filter(|(_, v)| *v != 1.)
            .collect()
    }
    fn multiplier(&self, m: Multiplier) -> f64 {
        match m {
            Multiplier::Size => self.size,
            Multiplier::Opacity => self.opacity,
            Multiplier::Glow => self.glow,
        }
    }
    fn edit_part(&self, raw: &[u8], part: &str, outside_hero: bool) -> Result<effect_edit::EditedFile> {
        self.check()?;
        match self.left_out(part, outside_hero) {
            // A left-out part keeps the game's file as it is (no color, no multiplier).
            Some(_) => effect_edit::edit_file_with(raw, &|slot: &ColorSlot| slot.rgb, &[]),
            None => effect_edit::edit_file_controlled(raw, &|slot: &ColorSlot| self.color(false, slot.rgb).0, &self.multipliers(), &self.controls(part)),
        }
    }
    fn controls(&self, part: &str) -> PartControls {
        self.part_settings(part).map_or_else(PartControls::default, |s| PartControls { hidden: s.hidden, size: s.size, opacity: s.opacity, tint: s.tint, light: s.light })
    }
}
fn distance(a: [u8; 3], b: [u8; 3]) -> f64 {
    let ((ha, sa, va), (hb, sb, vb)) = (hsv(a), hsv(b));
    hue_distance(ha, hb) / 180. + (sa - sb).abs() + (va - vb).abs()
}
/// `rgb` moved the way `from` moves to `to`.
fn follow(from: [u8; 3], to: [u8; 3], rgb: [u8; 3]) -> [u8; 3] {
    let ((hf, sf, vf), (ht, st, vt), (h, s, v)) = (hsv(from), hsv(to), hsv(rgb));
    let value = if vf > 0. { v * vt / vf } else { vt };
    if sf < GREY_SATURATION {
        // A grey source has no hue to turn from: its near greys take the target's hue and saturation.
        return from_hsv(ht, st, value);
    }
    let saturation = s * st / sf;
    if s < GREY_SATURATION {
        return from_hsv(h, s, value);
    }
    from_hsv(h + (ht - hf), saturation, value)
}

// ---------------------------------------------------------------- pick a color

/// The whole-effect shift that takes the main color to a picked color.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Pick {
    pub hue: f64,
    pub saturation: f64,
    pub brightness: f64,
    /// The main color after the shift.
    pub result: [u8; 3],
    pub result_hex: String,
    /// True when `result` is within one level per channel of the pick.
    pub exact: bool,
    /// Why the pick cannot be reached, when it cannot.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// A note about the result (black hides additive glows), when there is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}
/// Hue turn, saturation and brightness scales (within the edit's 0–4 range)
/// such that `shift_color(main)` is `picked` within one level per channel
/// whenever the scales needed are in range. Picking a grey or white sets
/// saturation 0 and hue 0 (every color turns grey); picking black sets
/// brightness 0. A grey or very pale main color cannot follow a colored pick
/// (saturation would need more than 4): `exact` is false and `reason` says so;
/// a main color too dark for a bright pick likewise.
pub fn pick_shift(main: [u8; 3], picked: [u8; 3]) -> Pick {
    let ((hm, sm, vm), (hp, sp, vp)) = (hsv(main), hsv(picked));
    let mut reason = None;
    let hue = if sp == 0. || sm == 0. {
        0.
    } else {
        let d = (hp - hm).rem_euclid(360.);
        if d > 180. { d - 360. } else { d }
    };
    let saturation = if sp == 0. {
        0.
    } else if sm == 0. || sp / sm > 4. {
        reason = Some("The effect's main color is too grey to take this color; choose a more colorful swatch as the main color, or change single colors instead.".to_owned());
        4.
    } else {
        sp / sm
    };
    let brightness = if vp == 0. {
        0.
    } else if vm == 0. || vp / vm > 4. {
        reason.get_or_insert_with(|| "The effect's main color is too dark to reach this brightness.".to_owned());
        4.
    } else {
        vp / vm
    };
    let result = effect_edit::shift_color(main, &EffectEdit { hue, saturation, brightness, ..Default::default() });
    let exact = levels(result, picked) <= 1;
    if exact {
        reason = None;
    } else if reason.is_none() {
        reason = Some("This color cannot be reached exactly from the effect's main color.".to_owned());
    }
    let warning = (vp < BLACK_VALUE).then(|| "Black makes glowing (additive) parts invisible.".to_owned());
    Pick { hue, saturation, brightness, result, result_hex: hex(result), exact, reason, warning }
}

// ---------------------------------------------------------------- strips

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StripKind {
    Constant,
    Random,
    Fade,
    Gradient,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Stop {
    /// 0–1 along the strip (a gradient's authored position, clamped).
    pub position: f64,
    pub rgb: [u8; 3],
    pub hex: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alpha: Option<u8>,
    /// The slot this stop shows (its place in the file), or `None` for an
    /// estimate.
    pub slot: Option<String>,
    /// For an estimated stop (a fade's start): the slot it was taken from, or
    /// `None` when the part has no color of its own (white).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// How much this stop shows (estimate, same unit for the whole tree).
    pub weight: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Strip {
    /// Stable id within the part: the place of the color (or its gradient) in the file.
    pub id: String,
    pub kind: StripKind,
    /// What the color does, in plain words.
    pub role: String,
    /// The particle function that holds it.
    pub function: String,
    pub stops: Vec<Stop>,
}

fn last_key(location: &Location) -> &str {
    match location.0.last() {
        Some(Step::Key(k)) => k,
        _ => "",
    }
}
/// The location up to (not including) the step `key`.
fn before_key(location: &Location, key: &str) -> Option<Location> {
    let i = location.0.iter().position(|s| matches!(s, Step::Key(k) if k == key))?;
    Some(Location(location.0[..i].to_vec()))
}
fn float(value: Option<&Value>) -> Option<f64> {
    let value = value?;
    match &value.kind {
        Kind::F32(_) | Kind::F64(_) | Kind::I64(_) | Kind::U64(_) => value.number().ok(),
        Kind::Object(_) => {
            let kind = value.get("m_nType").and_then(|v| v.string().ok()).unwrap_or("");
            match kind {
                "PF_TYPE_LITERAL" => value.get("m_flLiteralValue").and_then(|v| v.number().ok()),
                "PF_TYPE_RANDOM_UNIFORM" | "PF_TYPE_RANDOM_BIASED" => {
                    let min = value.get("m_flRandomMin").and_then(|v| v.number().ok()).unwrap_or(0.);
                    let max = value.get("m_flRandomMax").and_then(|v| v.number().ok()).unwrap_or(1.);
                    Some((min + max) / 2.)
                }
                _ => None,
            }
        }
        _ => None,
    }
}
fn driver(input: Option<&Value>) -> &'static str {
    let kind = input
        .and_then(|i| i.get("m_FloatInterp"))
        .and_then(|f| f.get("m_nType"))
        .and_then(|t| t.string().ok())
        .unwrap_or("");
    match kind {
        "PF_TYPE_PARTICLE_AGE_NORMALIZED" | "PF_TYPE_PARTICLE_AGE" => "over its life",
        "PF_TYPE_PARTICLE_NUMBER_NORMALIZED" | "PF_TYPE_PARTICLE_NUMBER" => "across its particles",
        "PF_TYPE_RANDOM_UNIFORM" | "PF_TYPE_RANDOM_BIASED" => "picked at random along it",
        "PF_TYPE_COLLECTION_AGE" => "over the effect's time",
        "PF_TYPE_PARTICLE_NOISE" => "changing with noise",
        "" => "",
        _ => "chosen by the game while it plays",
    }
}
fn with_driver(base: &str, driver: &str) -> String {
    if driver.is_empty() { base.to_owned() } else { format!("{base} {driver}") }
}

/// Weights of one part, from `part_weight` and the file.
struct Weights<'a> {
    root: &'a Value,
    part: f64,
    additive: bool,
    constant: f64,
}
impl Weights<'_> {
    fn of(&self, rgb: [u8; 3], alpha: Option<u8>, share: f64) -> f64 {
        let mut w = self.part * share;
        if let Some(a) = alpha {
            w *= f64::from(a) / 255.;
        }
        if self.additive {
            w *= hsv(rgb).2;
        }
        w
    }
    /// Share of a source in its part (before alpha and brightness).
    fn share(&self, slot: &ColorSlot) -> f64 {
        let function = |loc: &Location| Location(loc.0.iter().take(2).cloned().collect::<Vec<_>>());
        match slot.source {
            ColorSource::Constant => self.constant,
            ColorSource::Random => {
                if last_key(&slot.location).starts_with("m_Tint") {
                    let op = function(&slot.location).get(self.root);
                    0.5 * float(op.and_then(|o| o.get("m_flTintPerc"))).unwrap_or(0.).clamp(0., 1.)
                } else {
                    0.5
                }
            }
            ColorSource::Fade => {
                let op = function(&slot.location).get(self.root);
                if slot.function == "C_OP_ColorInterpolateRandom" {
                    return 0.25;
                }
                let start = float(op.and_then(|o| o.get("m_flFadeStartTime"))).unwrap_or(0.).clamp(0., 1.);
                let end = float(op.and_then(|o| o.get("m_flFadeEndTime"))).unwrap_or(1.).clamp(start, 1.);
                1. - (start + end) / 2.
            }
            ColorSource::Renderer if last_key(&slot.location) == "m_OutlineColor" => 0.2,
            _ => 1.,
        }
    }
}

/// The share of the system color (`m_ConstantColor`) in what a part shows:
/// `CONSTANT_REPLACED` when an initializer, or an operator setting the color
/// outright, overwrites it; `CONSTANT_SCALED` when an operator only scales it;
/// else 1.
fn constant_share(root: &Value) -> f64 {
    let color = |op: &Value, key: &str| [None, Some(6), Some(22)].contains(&op.get(key).and_then(|v| v.integer().ok()));
    let class = |op: &Value| op.get("_class").and_then(|v| v.string().ok()).unwrap_or("").to_owned();
    let list = |key: &str| root.get(key).and_then(|v| v.array().ok()).unwrap_or(&[]).to_vec();
    let initialized = list("m_Initializers").iter().any(|op| match class(op).as_str() {
        "C_INIT_RandomColor" => color(op, "m_nFieldOutput"),
        "C_INIT_InitVec" => color(op, "m_nOutputField"),
        _ => false,
    });
    let mut scaled = false;
    let mut replaced = initialized;
    for op in list("m_Operators") {
        if class(&op) == "C_OP_SetVec" && color(&op, "m_nOutputField") {
            let method = op.get("m_nSetMethod").and_then(|v| v.string().ok()).unwrap_or("");
            if method.contains("SCALE") || method.contains("ADD") {
                scaled = true;
            } else {
                replaced = true;
            }
        }
    }
    if replaced {
        CONSTANT_REPLACED
    } else if scaled {
        CONSTANT_SCALED
    } else {
        1.
    }
}
/// Share of an overwritten system color (it only shows for a moment, if at all).
pub const CONSTANT_REPLACED: f64 = 0.05;
/// Share of a system color an operator scales.
pub const CONSTANT_SCALED: f64 = 0.3;

/// The part's color strips, each stop weighted; `part` is the part's
/// visibility (`PartLook::weight`) and `additive` its blending.
pub fn strips(root: &Value, colors: &[ColorSlot], part: f64, additive: bool) -> Vec<Strip> {
    let weights = Weights { root, part, additive, constant: constant_share(root) };
    let id = |l: &Location| l.to_string();
    let stop = |slot: &ColorSlot, position: f64, share: f64| Stop {
        position,
        rgb: slot.rgb,
        hex: hex(slot.rgb),
        alpha: slot.alpha,
        slot: Some(id(&slot.location)),
        source: None,
        weight: weights.of(slot.rgb, slot.alpha, weights.share(slot) * share),
    };
    // The part's starting color, for fades.
    let base = colors
        .iter()
        .find(|s| s.source == ColorSource::Random && last_key(&s.location) == "m_ColorMin")
        .or_else(|| colors.iter().find(|s| s.source == ColorSource::VectorLiteral && s.location.0.first() == Some(&Step::Key("m_Initializers".into()))))
        .or_else(|| colors.iter().find(|s| s.source == ColorSource::Constant));
    let mut out: Vec<Strip> = vec![];
    let mut done: BTreeSet<String> = BTreeSet::new();
    for slot in colors {
        let key = last_key(&slot.location).to_owned();
        if slot.location.0.iter().any(|s| matches!(s, Step::Key(k) if k == "m_Stops")) {
            // Gradient stops: one strip per gradient, in authored order.
            let Some(gradient) = before_key(&slot.location, "m_Stops") else { continue };
            let gid = id(&gradient);
            if !done.insert(gid.clone()) {
                continue;
            }
            let members: Vec<&ColorSlot> = colors.iter().filter(|s| s.location.starts_with(&gradient)).collect();
            let mut positions: Vec<f64> = members
                .iter()
                .map(|s| {
                    let mut at = s.location.clone();
                    at.0.pop();
                    at.0.push(Step::Key("m_flPosition".into()));
                    float(at.get(root)).unwrap_or(0.).clamp(0., 1.)
                })
                .collect();
            if positions.iter().all(|p| *p == 0.) && positions.len() > 1 {
                let n = (positions.len() - 1) as f64;
                positions = (0..positions.len()).map(|i| i as f64 / n).collect();
            }
            // Each stop covers half the distance to its neighbours; the ends reach 0 and 1.
            let mut order: Vec<usize> = (0..members.len()).collect();
            order.sort_by(|a, b| positions[*a].total_cmp(&positions[*b]));
            let mut covers = vec![0.; members.len()];
            for (k, &i) in order.iter().enumerate() {
                let left = if k == 0 { 0. } else { (positions[order[k - 1]] + positions[i]) / 2. };
                let right = if k + 1 == order.len() { 1. } else { (positions[i] + positions[order[k + 1]]) / 2. };
                covers[i] = (right - left).max(0.);
            }
            let total: f64 = covers.iter().sum();
            let stops = members
                .iter()
                .zip(&positions)
                .zip(&covers)
                .map(|((s, p), c)| stop(s, *p, if total > 0. { c / total } else { 1. / members.len() as f64 }))
                .collect();
            let input = before_key(&gradient, "m_Gradient").and_then(|i| i.get(root));
            let role = match slot.source {
                ColorSource::TextureGradient => "texture replaced by a gradient of these colors".to_owned(),
                ColorSource::Renderer => with_driver("tint applied when drawn, changing", driver(input)),
                _ => with_driver("color", driver(input)),
            };
            out.push(Strip { id: gid, kind: StripKind::Gradient, role, function: slot.function.clone(), stops });
            continue;
        }
        match slot.source {
            ColorSource::Random => {
                let tint = key.starts_with("m_Tint");
                let Some(op) = before_key(&slot.location, &key) else { continue };
                let gid = format!("{}{}", id(&op), if tint { ".tint" } else { "" });
                if !done.insert(gid.clone()) {
                    continue;
                }
                let pair: Vec<&ColorSlot> = colors
                    .iter()
                    .filter(|s| s.location.starts_with(&op) && last_key(&s.location).starts_with("m_Tint") == tint)
                    .collect();
                let role = if tint { "lighting tint, random between these" } else { "starting color, random between these" };
                let kind = if pair.len() > 1 { StripKind::Random } else { StripKind::Constant };
                let stops = pair.iter().enumerate().map(|(i, s)| stop(s, i as f64, 1.)).collect();
                out.push(Strip { id: gid, kind, role: role.into(), function: slot.function.clone(), stops });
            }
            ColorSource::Fade if slot.function == "C_OP_ColorInterpolateRandom" => {
                let Some(op) = before_key(&slot.location, &key) else { continue };
                let gid = id(&op);
                if !done.insert(gid.clone()) {
                    continue;
                }
                let pair: Vec<&ColorSlot> = colors.iter().filter(|s| s.location.starts_with(&op)).collect();
                let stops = pair.iter().enumerate().map(|(i, s)| stop(s, i as f64, 1.)).collect();
                out.push(Strip { id: gid, kind: StripKind::Random, role: "fades to a random color between these".into(), function: slot.function.clone(), stops });
            }
            ColorSource::Fade => {
                let start = match base {
                    Some(b) => Stop { position: 0., rgb: b.rgb, hex: hex(b.rgb), alpha: b.alpha, slot: None, source: Some(id(&b.location)), weight: 0. },
                    None => Stop { position: 0., rgb: [255; 3], hex: hex([255; 3]), alpha: None, slot: None, source: None, weight: 0. },
                };
                out.push(Strip {
                    id: id(&slot.location),
                    kind: StripKind::Fade,
                    role: "fades to this color over its life".into(),
                    function: slot.function.clone(),
                    stops: vec![start, stop(slot, 1., 1.)],
                });
            }
            _ => {
                let role = match (slot.source, key.as_str()) {
                    (ColorSource::Constant, _) => "the part's own color",
                    (ColorSource::VectorLiteral, _) => "color set to this",
                    (_, "m_OutlineColor") => "outline color",
                    (_, "m_vColorBlend") => "blend color when drawn",
                    (_, "m_vecHeadColorScale") => "tint at the trail's head",
                    (_, "m_vecTailColorScale") => "tint at the trail's tail",
                    _ => "tint applied when drawn",
                };
                out.push(Strip {
                    id: id(&slot.location),
                    kind: StripKind::Constant,
                    role: role.into(),
                    function: slot.function.clone(),
                    stops: vec![stop(slot, 0., 1.)],
                });
            }
        }
    }
    out
}

// ---------------------------------------------------------------- palette and main color

/// One distinct color of a tree (near-identical colors merged).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Swatch {
    pub rgb: [u8; 3],
    pub hex: String,
    /// Every distinct color merged into this swatch (`rgb` first).
    pub members: Vec<[u8; 3]>,
    /// Share of the tree's estimated visible color (0–1).
    pub weight: f64,
    /// Parts (paths) that use it.
    pub parts: Vec<String>,
    /// Number of stops (slots) that use it.
    pub slots: usize,
    /// `grey`, `white` or `black`, else absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub neutral: Option<&'static str>,
}
/// A color use for the palette: color, weight and part.
pub struct Use<'a> {
    pub rgb: [u8; 3],
    pub weight: f64,
    pub part: &'a str,
}
/// Distinct colors, heaviest first; colors within `MERGE_LEVELS` of a
/// heavier swatch's color join it.
pub fn palette(uses: &[Use]) -> Vec<Swatch> {
    let mut distinct: Vec<([u8; 3], f64, BTreeSet<&str>, usize)> = vec![];
    for u in uses {
        match distinct.iter_mut().find(|d| d.0 == u.rgb) {
            Some(d) => {
                d.1 += u.weight;
                d.2.insert(u.part);
                d.3 += 1;
            }
            None => distinct.push((u.rgb, u.weight, BTreeSet::from([u.part]), 1)),
        }
    }
    distinct.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    let total: f64 = distinct.iter().map(|d| d.1).sum();
    let mut out: Vec<(Swatch, BTreeSet<&str>)> = vec![];
    for (rgb, weight, parts, slots) in distinct {
        let target = out
            .iter_mut()
            .filter(|(s, _)| levels(s.rgb, rgb) <= MERGE_LEVELS)
            .min_by_key(|(s, _)| levels(s.rgb, rgb));
        match target {
            Some((swatch, set)) => {
                swatch.members.push(rgb);
                swatch.weight += weight;
                swatch.slots += slots;
                set.extend(parts);
            }
            None => out.push((
                Swatch { rgb, hex: hex(rgb), members: vec![rgb], weight, parts: vec![], slots, neutral: neutral(rgb) },
                parts,
            )),
        }
    }
    let mut out: Vec<Swatch> = out
        .into_iter()
        .map(|(mut s, parts)| {
            s.parts = parts.into_iter().map(str::to_owned).collect();
            s.weight = if total > 0. { s.weight / total } else { 0. };
            s
        })
        .collect();
    out.sort_by(|a, b| b.weight.total_cmp(&a.weight).then(a.rgb.cmp(&b.rgb)));
    out
}
/// The tree's main color.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Main {
    pub rgb: [u8; 3],
    pub hex: String,
    /// Index into the tree's palette.
    pub swatch: usize,
    /// Share of the colorful weight its hue family holds (0–1).
    pub share: f64,
}
/// The main color: among swatches with at least `MAIN_SATURATION` and
/// `MAIN_VALUE` (greys, white and black never win), weighted by weight ×
/// saturation × value (vivid colors read as the effect's color; dark smoke
/// reads as dark, not as its hue), hue families form around the heaviest colors (`FAMILY_HUE`);
/// the heaviest family's heaviest color is the main color. `None` when the
/// tree has no colorful swatch.
pub fn main_color(palette: &[Swatch]) -> Option<Main> {
    let candidates: Vec<(usize, f64, f64)> = palette
        .iter()
        .enumerate()
        .filter_map(|(i, s)| {
            let (h, sat, v) = hsv(s.rgb);
            (sat >= MAIN_SATURATION && v >= MAIN_VALUE).then_some((i, h, s.weight * sat * v))
        })
        .collect();
    let total: f64 = candidates.iter().map(|c| c.2).sum();
    let mut order = candidates.clone();
    order.sort_by(|a, b| b.2.total_cmp(&a.2).then(a.0.cmp(&b.0)));
    let mut taken = BTreeSet::new();
    let mut best: Option<(usize, f64)> = None;
    for (seed, hue, _) in &order {
        if taken.contains(seed) {
            continue;
        }
        let mut family = 0.;
        for (i, h, w) in &order {
            if !taken.contains(i) && hue_distance(*h, *hue) <= FAMILY_HUE {
                family += w;
                taken.insert(*i);
            }
        }
        if best.is_none_or(|(_, w)| family > w) {
            best = Some((*seed, family));
        }
    }
    let (swatch, family) = best?;
    let rgb = palette[swatch].rgb;
    Some(Main { rgb, hex: hex(rgb), swatch, share: if total > 0. { family / total } else { 0. } })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hsv_round_trips_every_grid_color() {
        for r in (0..=255).step_by(15) {
            for g in (0..=255).step_by(17) {
                for b in (0..=255).step_by(51) {
                    let c = [r as u8, g as u8, b as u8];
                    let (h, s, v) = hsv(c);
                    assert!(levels(from_hsv(h, s, v), c) <= 1, "{c:?}");
                }
            }
        }
    }

    #[test]
    fn near_is_hue_saturation_and_value_within_limits() {
        let teal = [20, 200, 180];
        assert!(near(teal, from_hsv(hsv(teal).0 + 19., hsv(teal).1, hsv(teal).2)));
        assert!(!near(teal, from_hsv(hsv(teal).0 + 22., hsv(teal).1, hsv(teal).2)));
        assert!(!near(teal, from_hsv(hsv(teal).0, hsv(teal).1 - 0.35, hsv(teal).2)));
        assert!(!near(teal, from_hsv(hsv(teal).0, hsv(teal).1, hsv(teal).2 - 0.35)));
        assert!(near([120, 120, 120], [140, 140, 140]), "two greys close in value");
        assert!(!near([120, 120, 120], [180, 180, 180]));
        assert!(!near([120, 120, 120], [120, 90, 90]), "a grey is never near a color");
        assert!(near([5, 0, 20], [30, 10, 0]), "two blacks");
        assert!(!near([5, 0, 20], teal));
        // Hue wraps around red.
        assert!(near(from_hsv(355., 0.8, 0.8), from_hsv(10., 0.8, 0.8)));
    }

    #[test]
    fn replacements_then_shift_then_exclusion() {
        let edit = TreeEdit {
            hue: 120.,
            colors: vec![
                ColorReplacement { from: [200, 40, 40], to: [40, 40, 200], near: false },
                ColorReplacement { from: [20, 200, 180], to: [200, 180, 20], near: true },
            ],
            ..Default::default()
        };
        // Exact match (within MERGE_LEVELS) becomes `to`; an excluded part keeps every color.
        assert_eq!(edit.color(false, [203, 42, 38]), ([40, 40, 200], Rule::Replaced));
        assert_eq!(edit.color(true, [200, 40, 40]), ([200, 40, 40], Rule::Excluded));
        assert_eq!(edit.color(true, from_hsv(hsv([20, 200, 180]).0 + 5., 0.8, 0.7)).1, Rule::Excluded);
        // A near color keeps its difference from `from`.
        let shade = from_hsv(hsv([20, 200, 180]).0 + 10., 0.7, 0.6);
        let (moved, rule) = edit.color(false, shade);
        assert_eq!(rule, Rule::Near);
        let (hf, ht, hm) = (hsv([20, 200, 180]).0, hsv([200, 180, 20]).0, hsv(moved).0);
        assert!(hue_distance(hm, hsv(shade).0 + (ht - hf)) < 1.5, "{moved:?}");
        // Near replacements need `near`.
        assert_eq!(edit.color(false, from_hsv(hsv([200, 40, 40]).0 + 5., 0.8, 0.78)).1, Rule::Shifted);
        // Otherwise the shift, unless excluded; greys keep their value under a hue turn.
        assert_eq!(edit.color(false, [255, 0, 0]), ([0, 255, 0], Rule::Shifted));
        assert_eq!(edit.color(true, [255, 0, 0]), ([255, 0, 0], Rule::Excluded));
        assert_eq!(edit.color(false, [90, 90, 90]), ([90, 90, 90], Rule::Unchanged));
        // A grey source moves its near greys onto the target's hue.
        let grey = TreeEdit { colors: vec![ColorReplacement { from: [100, 100, 100], to: [200, 0, 0], near: true }], ..Default::default() };
        let (g, rule) = grey.color(false, [110, 110, 110]);
        assert_eq!(rule, Rule::Near);
        assert!(g[0] > 200 && g[1] == 0 && g[2] == 0, "{g:?}");
        // A replacement from a swatch's color (without `near`) changes every member merged into it.
        let uses = [Use { rgb: [200, 40, 40], weight: 1., part: "a" }, Use { rgb: [205, 44, 36], weight: 0.5, part: "b" }];
        let swatch = &palette(&uses)[0];
        assert_eq!(swatch.members.len(), 2);
        let one = TreeEdit { colors: vec![ColorReplacement { from: swatch.rgb, to: [0, 0, 255], near: false }], ..Default::default() };
        for member in &swatch.members {
            assert_eq!(one.color(false, *member), ([0, 0, 255], Rule::Replaced), "{member:?}");
        }
        // The identity changes nothing.
        let identity = TreeEdit::default();
        assert_eq!(identity.color(false, [12, 34, 56]), ([12, 34, 56], Rule::Unchanged));
        assert!(identity.multipliers().is_empty());
        // Serde: `{}` is the identity; unknown keys are refused.
        assert_eq!(serde_json::from_str::<TreeEdit>("{}").unwrap(), identity);
        assert!(serde_json::from_str::<TreeEdit>(r#"{"speed": 2}"#).is_err());
        assert!(TreeEdit { saturation: 5., ..Default::default() }.validate().is_err());
        assert!(TreeEdit { size: 0., ..Default::default() }.validate().is_err());
        assert!(TreeEdit { hue: f64::NAN, ..Default::default() }.validate().is_err());
    }

    #[test]
    fn picking_a_color_maps_the_main_color_onto_it() {
        let mut checked = 0;
        for h in (0..360).step_by(23) {
            for s in [0.3, 0.55, 0.8, 1.] {
                for v in [0.35, 0.6, 0.85, 1.] {
                    let main = from_hsv(f64::from(h), s, v);
                    for ph in (0..360).step_by(41) {
                        for ps in [0., 0.2, 0.6, 1.] {
                            for pv in [0., 0.1, 0.5, 1.] {
                                let picked = from_hsv(f64::from(ph), ps, pv);
                                let pick = pick_shift(main, picked);
                                let (_, sm, vm) = hsv(main);
                                let (_, sp, vp) = hsv(picked);
                                let reachable = (sm == 0. && sp == 0. || sm > 0. && sp / sm <= 4.) && (vp == 0. || vm > 0. && vp / vm <= 4.);
                                if reachable {
                                    assert!(pick.exact, "{main:?} -> {picked:?}: {pick:?}");
                                    assert!(pick.reason.is_none());
                                }
                                assert!((0.0..=4.).contains(&pick.saturation) && (0.0..=4.).contains(&pick.brightness));
                                assert!((-180.0..=180.).contains(&pick.hue));
                                checked += 1;
                            }
                        }
                    }
                }
            }
        }
        assert!(checked > 10_000);
        let teal = [20, 200, 180];
        // Grey, white and black picks.
        let grey = pick_shift(teal, [128, 128, 128]);
        assert!(grey.exact && grey.saturation == 0. && grey.hue == 0.);
        let white = pick_shift(teal, [255, 255, 255]);
        assert!(white.exact && white.result == [255, 255, 255]);
        let black = pick_shift(teal, [0, 0, 0]);
        assert!(black.exact && black.brightness == 0. && black.warning.is_some());
        // A nearly grey main color cannot reach a vivid pick: said so, not hidden.
        let pale = pick_shift([120, 118, 118], [200, 0, 200]);
        assert!(!pale.exact && pale.reason.as_deref().unwrap().contains("too grey"), "{pale:?}");
        assert_eq!(pale.saturation, 4.);
        // A dark main color cannot reach white … but greys can.
        let dark = pick_shift([40, 10, 10], [255, 255, 255]);
        assert!(!dark.exact && dark.reason.as_deref().unwrap().contains("too dark"));
        assert!(pick_shift([128, 128, 128], [64, 64, 64]).exact);
    }

    #[test]
    fn palette_merges_near_identical_colors_and_main_ignores_neutrals() {
        // The explosion's shape: much black and grey smoke, a teal family split
        // into a lighter and a greener shade, a small red touch.
        let uses = [
            Use { rgb: [22, 16, 16], weight: 5.6, part: "smoke" },
            Use { rgb: [24, 18, 15], weight: 0.4, part: "smoke" },
            Use { rgb: [84, 103, 96], weight: 1.8, part: "gas" },
            Use { rgb: [109, 213, 185], weight: 1.5, part: "core" },
            Use { rgb: [40, 190, 140], weight: 1.0, part: "streak" },
            Use { rgb: [111, 215, 183], weight: 0.5, part: "streak" },
            Use { rgb: [230, 30, 30], weight: 0.9, part: "bits" },
            Use { rgb: [255, 255, 255], weight: 3.0, part: "flash" },
        ];
        let palette = palette(&uses);
        assert_eq!(palette[0].rgb, [22, 16, 16]);
        assert_eq!(palette[0].members, [[22, 16, 16], [24, 18, 15]]);
        assert_eq!(palette[0].neutral, Some("black"));
        let core = palette.iter().find(|s| s.rgb == [109, 213, 185]).unwrap();
        assert_eq!(core.members.len(), 2);
        assert_eq!(core.parts, ["core", "streak"]);
        assert!((palette.iter().map(|s| s.weight).sum::<f64>() - 1.).abs() < 1e-9);
        assert_eq!(palette.iter().find(|s| s.rgb == [255, 255, 255]).unwrap().neutral, Some("white"));
        let main = main_color(&palette).unwrap();
        // Teal wins over red although the red swatch alone is close, and black and white never win.
        assert_eq!(main.rgb, [109, 213, 185]);
        assert!(main.share > 0.6, "{main:?}");
        assert!(main_color(&palette[..1]).is_none(), "only black: no main color");
    }

    fn obj(rows: Vec<(&str, Value)>) -> Value {
        Value::new(Kind::Object(rows.into_iter().map(|(k, v)| (k.to_owned(), v)).collect()))
    }
    fn s(t: &str) -> Value {
        Value::new(Kind::String(t.into()))
    }
    fn f(n: f64) -> Value {
        Value::new(Kind::F64(n))
    }
    fn color(c: &[i64]) -> Value {
        Value::new(Kind::Array(c.iter().map(|n| Value::new(Kind::I64(*n))).collect()))
    }
    fn arr(rows: Vec<Value>) -> Value {
        Value::new(Kind::Array(rows))
    }

    fn strips_of(root: &Value, colors: &[ColorSlot]) -> Vec<Strip> {
        strips(root, colors, 1., false)
    }

    #[test]
    fn strips_take_each_source_as_a_page_draws_it() {
        let root = obj(vec![
            ("_class", s("CParticleSystemDefinition")),
            ("m_ConstantColor", color(&[10, 20, 30, 255])),
            ("m_Initializers", arr(vec![obj(vec![("_class", s("C_INIT_RandomColor")), ("m_ColorMin", color(&[255, 0, 0])), ("m_ColorMax", color(&[0, 0, 255]))])])),
            (
                "m_Operators",
                arr(vec![
                    obj(vec![("_class", s("C_OP_ColorInterpolate")), ("m_ColorFade", color(&[0, 255, 0])), ("m_flFadeStartTime", f(0.5))]),
                    obj(vec![
                        ("_class", s("C_OP_SetVec")),
                        (
                            "m_InputValue",
                            obj(vec![
                                ("m_nType", s("PVEC_TYPE_FLOAT_INTERP_GRADIENT")),
                                ("m_FloatInterp", obj(vec![("m_nType", s("PF_TYPE_PARTICLE_AGE_NORMALIZED"))])),
                                (
                                    "m_Gradient",
                                    obj(vec![(
                                        "m_Stops",
                                        arr(vec![
                                            obj(vec![("m_flPosition", f(0.)), ("m_Color", color(&[255, 255, 0]))]),
                                            obj(vec![("m_flPosition", f(0.75)), ("m_Color", color(&[0, 255, 255]))]),
                                            obj(vec![("m_flPosition", f(1.)), ("m_Color", color(&[0, 0, 0]))]),
                                        ]),
                                    )]),
                                ),
                            ]),
                        ),
                    ]),
                ]),
            ),
        ]);
        let colors = effect_edit::colors(&root).colors;
        let strips = strips(&root, &colors, 10., true);
        let kinds: Vec<(StripKind, &str)> = strips.iter().map(|s| (s.kind, s.id.as_str())).collect();
        assert_eq!(
            kinds,
            [
                (StripKind::Constant, "m_ConstantColor"),
                (StripKind::Random, "m_Initializers[0]"),
                (StripKind::Fade, "m_Operators[0].m_ColorFade"),
                (StripKind::Gradient, "m_Operators[1].m_InputValue.m_Gradient"),
            ]
        );
        // The system color is overwritten by the random initializer and the gradient.
        assert!((strips[0].stops[0].weight - 10. * CONSTANT_REPLACED * hsv([10, 20, 30]).2).abs() < 1e-9);
        // An operator that only scales the color leaves the system color a share.
        let scaled = obj(vec![
            ("m_ConstantColor", color(&[200, 20, 30])),
            ("m_Operators", arr(vec![obj(vec![("_class", s("C_OP_SetVec")), ("m_nSetMethod", s("PARTICLE_SET_SCALE_INITIAL_VALUE")), ("m_InputValue", obj(vec![("m_nType", s("PVEC_TYPE_LITERAL_COLOR")), ("m_LiteralColor", color(&[255, 255, 255]))]))])])),
        ]);
        let c = effect_edit::colors(&scaled).colors;
        assert_eq!(strips_of(&scaled, &c)[0].stops[0].weight, CONSTANT_SCALED);
        let plain = obj(vec![("m_ConstantColor", color(&[200, 20, 30]))]);
        assert_eq!(strips_of(&plain, &effect_edit::colors(&plain).colors)[0].stops[0].weight, 1.);
        // The fade starts at the part's starting color (the random minimum), an estimate.
        assert_eq!(strips[2].stops[0].rgb, [255, 0, 0]);
        assert_eq!(strips[2].stops[0].slot, None);
        assert_eq!(strips[2].stops[0].source.as_deref(), Some("m_Initializers[0].m_ColorMin"));
        // Fade over 50–100 % of its life shows for a quarter.
        assert!((strips[2].stops[1].weight - 10. * 0.25).abs() < 1e-9);
        // Gradient stops cover 0.375, 0.5 and 0.125 of the life; black adds nothing (additive).
        let g: Vec<f64> = strips[3].stops.iter().map(|s| s.weight).collect();
        assert!((g[0] - 3.75).abs() < 1e-9 && (g[1] - 5.).abs() < 1e-9 && g[2] == 0., "{g:?}");
        assert_eq!(strips[3].role, "color over its life");
        assert_eq!(strips[3].stops[1].position, 0.75);
    }
}
