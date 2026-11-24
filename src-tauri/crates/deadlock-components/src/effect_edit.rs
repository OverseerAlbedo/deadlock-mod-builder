//! Effect edits: a hero's compiled particle systems (`.vpcf_c`) recolored and
//! scaled in place, without authoring tools.
//!
//! An effect is a tree: a top particle system and the systems it names in
//! `m_Children[].m_ChildRef`, read with `read_tree`. `colors` lists every color
//! a file authors itself (with its location in the DATA document), and what
//! sets color in a way a file edit cannot change. `edit_file` applies one
//! `EffectEdit` to one file: a hue rotation with saturation and brightness
//! scales on every listed color, and optional multipliers on the renderers'
//! size, opacity and brightness. A multiplier changes a key that is a plain
//! number or a `PF_TYPE_LITERAL`; on sprite, rope and trail renderers an absent
//! key (value 1) is added as a `PF_TYPE_LITERAL` of the multiplier; inputs of
//! other types are left and reported. A multiplier of 1 changes and adds
//! nothing. Only the DATA block is encoded again (the
//! repository's canonical uncompressed KV3 v1); every other block keeps the
//! game's exact bytes, and the result must pass `resources::validate_effect`.
//!
//! Color sources (all integer 0–255 RGB or RGBA arrays; alpha is kept):
//! - the system's `m_ConstantColor` (absent = white; never added);
//! - `C_INIT_RandomColor` `m_ColorMin`/`m_ColorMax`/`m_TintMin`/`m_TintMax`;
//! - `C_OP_ColorInterpolate` `m_ColorFade`, `C_OP_ColorInterpolateRandom`
//!   `m_ColorFadeMin`/`m_ColorFadeMax`;
//! - `C_OP_SetVec` / `C_INIT_InitVec` writing color (field 6 or 22, absent =
//!   6) from a `PVEC_TYPE_LITERAL_COLOR` (`m_LiteralColor`) or a
//!   `PVEC_TYPE_FLOAT_INTERP_GRADIENT` (`m_Gradient.m_Stops[].m_Color`) input;
//! - renderer color scales (`m_vecColorScale`, trails' `m_vecHeadColorScale`
//!   and `m_vecTailColorScale`, `m_vColorBlend`) of those two input types, and
//!   `m_OutlineColor`;
//! - renderer texture gradients, only when `m_bReplaceTextureWithGradient` is
//!   true.
//!
//! A literal color or gradient stop is taken by its input type only: compiled
//! files carry stale `m_LiteralColor = [0, 0, 0]` and stale gradients in inputs
//! of other types, which the game never reads.
//!
//! Not changed (listed in `FileColors::fixed`): color inherited from parent
//! particles (an inherit function whose `m_nFieldOutput` is a color field; with
//! no field it inherits the radius, ValveResourceFormat's default, and is not
//! listed), color from control points or other non-literal
//! inputs, literal colors written to particle fields other than color
//! (`ghost_blood_bomb_explode_model` writes one to field 30), model renderers'
//! material colors, projected renderers' materials and colored textures.
use crate::{Result, compiled, fail, resources};
use deadlock_core::text::casefold;
use deadlock_resource::{Kind, Value, binary, container};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fmt,
};

pub const EFFECT_EDIT_POLICY: &str = "effect-edit-v1";
/// The most files `read_tree` follows from one top (the game's largest seen: 23).
pub const MAX_TREE_FILES: usize = 256;
/// Particle attributes that hold color: 6 tint (the default output of
/// `C_OP_SetVec`, `C_INIT_InitVec` and the color functions), 22 glow.
const COLOR_FIELDS: [i128; 2] = [6, 22];
const DEFAULT_FIELD: i128 = 6;
const LITERAL_COLOR: &str = "PVEC_TYPE_LITERAL_COLOR";
const GRADIENT: &str = "PVEC_TYPE_FLOAT_INTERP_GRADIENT";
const LITERAL_FLOAT: &str = "PF_TYPE_LITERAL";
const FUNCTION_LISTS: [&str; 3] = ["m_Initializers", "m_Operators", "m_Renderers"];
const RENDERER_SCALES: [&str; 4] = [
    "m_vecColorScale",
    "m_vecHeadColorScale",
    "m_vecTailColorScale",
    "m_vColorBlend",
];
/// Renderers that get an absent size, opacity or brightness key added: the
/// game reads an added `m_flRadiusScale` or `m_flAlphaScale` literal on these,
/// and `m_flOverbrightFactor` is added the same way. Model renderers only have
/// existing keys multiplied.
const ADDED_RENDERERS: [&str; 3] = ["C_OP_RenderSprites", "C_OP_RenderRopes", "C_OP_RenderTrails"];
/// A `PF_TYPE_LITERAL` float input.
fn literal_float(value: f64) -> Value {
    Value::new(Kind::Object(vec![
        ("m_nType".to_owned(), Value::new(Kind::String(LITERAL_FLOAT.to_owned()))),
        ("m_flLiteralValue".to_owned(), Value::new(Kind::F64(value))),
    ]))
}
/// Renderers whose size, opacity and brightness multipliers are edited.
const SCALED_RENDERERS: [&str; 4] = [
    "C_OP_RenderSprites",
    "C_OP_RenderRopes",
    "C_OP_RenderTrails",
    "C_OP_RenderModels",
];

// ---------------------------------------------------------------- locations

/// One step into a KV3 document.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum Step {
    Key(String),
    Index(usize),
}
/// Where a value is in a particle system's DATA document.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct Location(pub Vec<Step>);
impl Location {
    fn key(&self, key: &str) -> Self {
        let mut steps = self.0.clone();
        steps.push(Step::Key(key.to_owned()));
        Self(steps)
    }
    fn index(&self, index: usize) -> Self {
        let mut steps = self.0.clone();
        steps.push(Step::Index(index));
        Self(steps)
    }
    /// Whether `self` is `other` or inside it.
    pub fn starts_with(&self, other: &Location) -> bool {
        self.0.starts_with(&other.0)
    }
    pub fn get<'a>(&self, root: &'a Value) -> Option<&'a Value> {
        self.0.iter().try_fold(root, |value, step| match (step, &value.kind) {
            (Step::Key(key), _) => value.get(key),
            (Step::Index(i), Kind::Array(rows)) => rows.get(*i),
            _ => None,
        })
    }
    pub fn get_mut<'a>(&self, root: &'a mut Value) -> Option<&'a mut Value> {
        let mut value = root;
        for step in &self.0 {
            value = match step {
                Step::Key(key) => value.get_mut(key)?,
                Step::Index(i) => match &mut value.kind {
                    Kind::Array(rows) => rows.get_mut(*i)?,
                    _ => return None,
                },
            };
        }
        Some(value)
    }
}
impl fmt::Display for Location {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (n, step) in self.0.iter().enumerate() {
            match step {
                Step::Key(key) if n == 0 => write!(f, "{key}")?,
                Step::Key(key) => write!(f, ".{key}")?,
                Step::Index(i) => write!(f, "[{i}]")?,
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------- trees

/// One particle system of an effect tree.
#[derive(Clone, Debug)]
pub struct TreeFile {
    /// Compiled path as the parent (or the caller, for the top) names it.
    pub path: String,
    pub raw: Vec<u8>,
    /// Compiled paths of its `m_Children`, in order.
    pub children: Vec<String>,
}
#[derive(Clone, Debug)]
pub struct EffectTree {
    pub top: String,
    /// The top first, then the children breadth first; each file once.
    pub files: Vec<TreeFile>,
    /// Children the reader did not find.
    pub missing: Vec<String>,
}

/// The DATA document of a compiled particle system.
pub fn document(raw: &[u8]) -> Result<binary::Document> {
    let layout = container::layout(raw).map_err(|e| fail(format!("Not a complete compiled resource: {e}")))?;
    let block = container::unique(&layout.blocks, "DATA")
        .map_err(|_| fail("A particle system needs exactly one DATA block."))?;
    let doc = binary::decode(container::payload(raw, block).map_err(fail)?)
        .map_err(|e| fail(format!("The particle system's DATA could not be read: {e}")))?;
    match doc.root.get("_class").map(|c| c.string()) {
        Some(Ok("CParticleSystemDefinition")) => Ok(doc),
        _ => Err(fail("Resource DATA is not a particle system definition.")),
    }
}

/// Compiled paths of a particle system's children (`m_Children[].m_ChildRef`).
pub fn children(root: &Value) -> Result<Vec<String>> {
    let mut out = vec![];
    for row in root.get("m_Children").and_then(|v| v.array().ok()).unwrap_or(&[]) {
        if let Some(Ok(path)) = row.get("m_ChildRef").map(|v| v.string()) {
            if !path.is_empty() {
                out.push(compiled(path)?);
            }
        }
    }
    Ok(out)
}

/// The effect whose top system is `top` (`.vpcf` or `.vpcf_c`, any case):
/// the top and every system reached through `m_Children`, each once.
/// `read` returns a compiled file's bytes, or `None` when it does not exist.
pub fn read_tree(top: &str, mut read: impl FnMut(&str) -> Result<Option<Vec<u8>>>) -> Result<EffectTree> {
    let top = compiled(top)?;
    let mut seen = BTreeSet::from([casefold(&top)]);
    let mut queue = VecDeque::from([top.clone()]);
    let (mut files, mut missing) = (vec![], vec![]);
    while let Some(path) = queue.pop_front() {
        let Some(raw) = read(&path)? else {
            if files.is_empty() {
                return Err(fail(format!("The effect {top} does not exist in the game files.")));
            }
            missing.push(path);
            continue;
        };
        let children = children(&document(&raw).map_err(|e| fail(format!("{path}: {e}")))?.root)?;
        for child in &children {
            if seen.insert(casefold(child)) {
                if seen.len() > MAX_TREE_FILES {
                    return Err(fail(format!("The effect {top} has more than {MAX_TREE_FILES} systems.")));
                }
                queue.push_back(child.clone());
            }
        }
        files.push(TreeFile { path, raw, children });
    }
    Ok(EffectTree { top, files, missing })
}

/// A `read_tree` reader over a game archive.
pub fn archive_reader(archive: &deadlock_vpk::Vpk) -> impl FnMut(&str) -> Result<Option<Vec<u8>>> + '_ {
    move |path| {
        if archive.entries.contains_key(&casefold(path)) {
            crate::read(archive, path).map(Some)
        } else {
            Ok(None)
        }
    }
}

// ---------------------------------------------------------------- colors

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ColorSource {
    /// The system's `m_ConstantColor`.
    Constant,
    /// `C_INIT_RandomColor` range or tint.
    Random,
    /// `C_OP_ColorInterpolate(Random)` fade target.
    Fade,
    /// `C_OP_SetVec` / `C_INIT_InitVec` literal color.
    VectorLiteral,
    /// `C_OP_SetVec` / `C_INIT_InitVec` gradient stop.
    VectorGradient,
    /// Renderer color scale or blend (literal or gradient stop) and outline.
    Renderer,
    /// Renderer texture replaced by a gradient: a stop.
    TextureGradient,
}
/// One authored color.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ColorSlot {
    /// The integer array (3 or 4 values).
    pub location: Location,
    /// The function's class, or `CParticleSystemDefinition` for the system color.
    pub function: String,
    pub source: ColorSource,
    pub rgb: [u8; 3],
    pub alpha: Option<u8>,
}
/// Why a `Fixed` entry is not edited. Only the material kinds certainly keep
/// their look after a file edit; the others are not proven either way.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FixedKind {
    /// Color inherited from the parent system's particles.
    Inherited,
    /// Color from a control point (set by the game at run time).
    ControlPoint,
    /// Color from a vector input of another type than a literal or gradient.
    OtherInput,
    /// A color value written to a particle field that is not color.
    OtherField,
    /// A color key whose value is not a 0–255 integer array.
    NotColorArray,
    /// A function this module does not read writes color.
    OtherFunction,
    /// A model renderer: the model's own materials.
    ModelMaterial,
    /// A projected renderer: the projected material's texture.
    ProjectedMaterial,
}
impl FixedKind {
    /// True when a file edit certainly leaves this color as it is.
    pub fn certain(self) -> bool {
        matches!(self, Self::ModelMaterial | Self::ProjectedMaterial)
    }
}
/// Something that sets color that a file edit does not change.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Fixed {
    pub location: Location,
    pub function: String,
    pub kind: FixedKind,
    pub reason: String,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct FileColors {
    pub colors: Vec<ColorSlot>,
    pub fixed: Vec<Fixed>,
    /// False when the system has no `m_ConstantColor` (its particles start white).
    pub constant_color: bool,
}

fn class(op: &Value) -> &str {
    op.get("_class").and_then(|v| v.string().ok()).unwrap_or("")
}
fn field(op: &Value, key: &str) -> Option<i128> {
    op.get(key).and_then(|v| v.integer().ok())
}
fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(|v| v.string().ok()).unwrap_or("")
}
fn rgba(value: &Value) -> Option<([u8; 3], Option<u8>)> {
    let rows = value.array().ok()?;
    if !(3..=4).contains(&rows.len()) {
        return None;
    }
    let mut out = [0u8; 4];
    for (slot, row) in out.iter_mut().zip(rows) {
        *slot = u8::try_from(row.integer().ok()?).ok()?;
    }
    Some(([out[0], out[1], out[2]], (rows.len() == 4).then_some(out[3])))
}

struct Collector<'a> {
    root: &'a Value,
    out: FileColors,
}
impl Collector<'_> {
    fn slot(&mut self, location: Location, function: &str, source: ColorSource) {
        match location.get(self.root).map(|v| (v, rgba(v))) {
            Some((_, Some((rgb, alpha)))) => self.out.colors.push(ColorSlot {
                location,
                function: function.to_owned(),
                source,
                rgb,
                alpha,
            }),
            Some(_) => self.fixed(location, function, FixedKind::NotColorArray, "the color is not a 0–255 integer array"),
            None => {}
        }
    }
    fn fixed(&mut self, location: Location, function: &str, kind: FixedKind, reason: impl Into<String>) {
        self.out.fixed.push(Fixed { location, function: function.to_owned(), kind, reason: reason.into() });
    }
    fn stops(&mut self, gradient: Location, function: &str, source: ColorSource) {
        let stops = gradient.key("m_Stops");
        let count = stops.get(self.root).and_then(|v| v.array().ok()).map_or(0, <[Value]>::len);
        for i in 0..count {
            self.slot(stops.index(i).key("m_Color"), function, source);
        }
    }
    /// A color vector input (`CPerParticleVecInput`) or a plain color array.
    fn vector(&mut self, at: Location, function: &str, literal: ColorSource, gradient: ColorSource) {
        let Some(value) = at.get(self.root) else { return };
        if value.array().is_ok() {
            return self.slot(at, function, literal);
        }
        match text(value, "m_nType") {
            LITERAL_COLOR => self.slot(at.key("m_LiteralColor"), function, literal),
            GRADIENT => self.stops(at.key("m_Gradient"), function, gradient),
            other => {
                let kind = if other.contains("_CP_") { FixedKind::ControlPoint } else { FixedKind::OtherInput };
                self.fixed(at, function, kind, format!("color from a {} input", if other.is_empty() { "untyped" } else { other }))
            }
        }
    }
    fn function(&mut self, at: Location, op: &Value) {
        let name = class(op);
        let output = |key| field(op, key).unwrap_or(DEFAULT_FIELD);
        match name {
            "C_INIT_RandomColor" => {
                if COLOR_FIELDS.contains(&output("m_nFieldOutput")) {
                    for key in ["m_ColorMin", "m_ColorMax", "m_TintMin", "m_TintMax"] {
                        self.slot(at.key(key), name, ColorSource::Random);
                    }
                } else {
                    self.fixed(at, name, FixedKind::OtherField, format!("writes particle field {}, not color", output("m_nFieldOutput")));
                }
            }
            "C_OP_ColorInterpolate" | "C_OP_ColorInterpolateRandom" => {
                if COLOR_FIELDS.contains(&output("m_nFieldOutput")) {
                    for key in ["m_ColorFade", "m_ColorFadeMin", "m_ColorFadeMax"] {
                        self.slot(at.key(key), name, ColorSource::Fade);
                    }
                } else {
                    self.fixed(at, name, FixedKind::OtherField, format!("writes particle field {}, not color", output("m_nFieldOutput")));
                }
            }
            "C_OP_SetVec" | "C_INIT_InitVec" => {
                let target = output("m_nOutputField");
                let input = at.key("m_InputValue");
                if COLOR_FIELDS.contains(&target) {
                    self.vector(input, name, ColorSource::VectorLiteral, ColorSource::VectorGradient);
                } else if op.get("m_InputValue").is_some_and(|v| [LITERAL_COLOR, GRADIENT].contains(&text(v, "m_nType"))) {
                    self.fixed(input, name, FixedKind::OtherField, format!("a color written to particle field {target}, not to color"));
                }
            }
            "C_INIT_InheritFromParentParticles" | "C_OP_InheritFromParentParticles" | "C_OP_InheritFromParentParticlesV2" => {
                // Without `m_nFieldOutput` these inherit the radius (field 3),
                // not the color: only an explicit color field inherits color.
                if field(op, "m_nFieldOutput").is_some_and(|f| COLOR_FIELDS.contains(&f)) {
                    self.fixed(at, name, FixedKind::Inherited, "color inherited from the parent system's particles");
                }
            }
            _ if name.starts_with("C_OP_Render") => {
                for key in RENDERER_SCALES {
                    self.vector(at.key(key), name, ColorSource::Renderer, ColorSource::Renderer);
                }
                self.slot(at.key("m_OutlineColor"), name, ColorSource::Renderer);
                let textures = at.key("m_vecTexturesInput");
                let count = textures.get(self.root).and_then(|v| v.array().ok()).map_or(0, <[Value]>::len);
                for i in 0..count {
                    let texture = textures.index(i);
                    let replaced = texture
                        .key("m_bReplaceTextureWithGradient")
                        .get(self.root)
                        .is_some_and(|v| v.kind == Kind::Bool(true));
                    if replaced {
                        self.stops(texture.key("m_Gradient"), name, ColorSource::TextureGradient);
                    }
                }
                match name {
                    "C_OP_RenderModels" => self.fixed(at, name, FixedKind::ModelMaterial, "a model's own colors come from its materials"),
                    "C_OP_RenderProjected" => self.fixed(at, name, FixedKind::ProjectedMaterial, "the projected material's texture colors"),
                    _ => {}
                }
            }
            _ => {
                for key in ["m_nFieldOutput", "m_nOutputField"] {
                    if field(op, key).is_some_and(|f| COLOR_FIELDS.contains(&f)) {
                        let (kind, reason) = if casefold(name).contains("cp") || casefold(name).contains("controlpoint") {
                            (FixedKind::ControlPoint, "color from a control point")
                        } else {
                            (FixedKind::OtherFunction, "color set by a function this edit does not change")
                        };
                        self.fixed(at.clone(), name, kind, reason);
                    }
                }
            }
        }
    }
}

/// Every color the particle system `root` authors, and what it cannot change.
pub fn colors(root: &Value) -> FileColors {
    let mut c = Collector { root, out: FileColors::default() };
    let system = Location::default().key("m_ConstantColor");
    c.out.constant_color = system.get(root).is_some();
    c.slot(system, "CParticleSystemDefinition", ColorSource::Constant);
    for list in FUNCTION_LISTS {
        let rows = root.get(list).and_then(|v| v.array().ok()).unwrap_or(&[]);
        for (i, op) in rows.iter().enumerate() {
            c.function(Location::default().key(list).index(i), op);
        }
    }
    c.out
}

// ---------------------------------------------------------------- the edit

/// One edit for every file of an effect. Multipliers of 1 (or `None`) leave
/// the renderers alone; hue 0 with saturation and brightness 1 leaves colors.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct EffectEdit {
    /// Hue rotation in degrees.
    pub hue: f64,
    /// Saturation scale (HSV S), 0–4.
    pub saturation: f64,
    /// Brightness scale (HSV V), 0–4.
    pub brightness: f64,
    /// Renderer `m_flRadiusScale` multiplier.
    pub size: Option<f64>,
    /// Renderer `m_flAlphaScale` multiplier.
    pub opacity: Option<f64>,
    /// Renderer `m_flOverbrightFactor` multiplier.
    pub glow: Option<f64>,
}
impl Default for EffectEdit {
    fn default() -> Self {
        Self { hue: 0., saturation: 1., brightness: 1., size: None, opacity: None, glow: None }
    }
}
impl EffectEdit {
    fn validate(&self) -> Result<()> {
        if !self.hue.is_finite() || ![self.saturation, self.brightness].iter().all(|v| (0.0..=4.0).contains(v)) {
            return Err(fail("Effect hue must be a number and saturation and brightness between 0 and 4."));
        }
        if self.multipliers().any(|(_, v)| !(v > 0. && v <= 16.)) {
            return Err(fail("Effect size, opacity and brightness multipliers must be above 0 and at most 16."));
        }
        Ok(())
    }
    fn recolors(&self) -> bool {
        self.hue.rem_euclid(360.) != 0. || self.saturation != 1. || self.brightness != 1.
    }
    fn multipliers(&self) -> impl Iterator<Item = (Multiplier, f64)> {
        [
            (Multiplier::Size, self.size),
            (Multiplier::Opacity, self.opacity),
            (Multiplier::Glow, self.glow),
        ]
        .into_iter()
        .filter_map(|(m, v)| v.filter(|v| *v != 1.).map(|v| (m, v)))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Multiplier {
    Size,
    Opacity,
    Glow,
}
impl Multiplier {
    pub const ALL: [Multiplier; 3] = [Multiplier::Size, Multiplier::Opacity, Multiplier::Glow];
    /// The renderer key it multiplies.
    pub fn key(self) -> &'static str {
        match self {
            Multiplier::Size => "m_flRadiusScale",
            Multiplier::Opacity => "m_flAlphaScale",
            Multiplier::Glow => "m_flOverbrightFactor",
        }
    }
}

/// A renderer value a multiplier changed or left alone.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Scaled {
    pub multiplier: Multiplier,
    pub renderer: String,
    /// The number changed (the plain value or the literal's `m_flLiteralValue`),
    /// or the key's place when it was left alone.
    pub location: Location,
    pub before: Option<f64>,
    pub after: Option<f64>,
    /// Why it was left alone; empty when changed or added.
    pub left: String,
    /// True when the key was absent and is (or would be) added as a
    /// `PF_TYPE_LITERAL` of the multiplier (the absent key's value is 1).
    pub added: bool,
}

/// What `apply` (and `edit_file`) changed in one file.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct EditReport {
    pub colors: FileColors,
    /// Locations of the colors whose value changed.
    pub recolored: Vec<Location>,
    pub scaled: Vec<Scaled>,
    /// What the part's own settings (`PartControls`) changed or left.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub controlled: Vec<Controlled>,
}
impl EditReport {
    /// Every location `apply` changed: color arrays, scaled numbers and the
    /// part's own settings.
    pub fn changed(&self) -> Vec<Location> {
        let mut out = self.recolored.clone();
        out.extend(self.scaled.iter().filter(|s| s.left.is_empty()).map(|s| s.location.clone()));
        out.extend(self.controlled.iter().filter(|c| c.left.is_empty()).map(|c| c.location.clone()));
        out
    }
}

/// `rgb` with the edit's hue rotation and saturation and brightness scales
/// (HSV). Greys, white and black keep their value under a hue-only edit.
pub fn shift_color(rgb: [u8; 3], edit: &EffectEdit) -> [u8; 3] {
    if !edit.recolors() {
        return rgb;
    }
    let [r, g, b] = rgb.map(|c| f64::from(c) / 255.);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let hue = if delta == 0. {
        0.
    } else if max == r {
        60. * ((g - b) / delta).rem_euclid(6.)
    } else if max == g {
        60. * ((b - r) / delta + 2.)
    } else {
        60. * ((r - g) / delta + 4.)
    };
    let s = if max == 0. { 0. } else { delta / max };
    let h = (hue + edit.hue).rem_euclid(360.);
    let s = (s * edit.saturation).clamp(0., 1.);
    let v = (max * edit.brightness).clamp(0., 1.);
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

fn set_rgb(value: &mut Value, rgb: [u8; 3]) -> Result<()> {
    let Kind::Array(rows) = &mut value.kind else {
        return Err(fail("A color is not an array."));
    };
    for (row, next) in rows.iter_mut().zip(rgb) {
        // Keep each element's own integer kind.
        row.kind = match row.kind {
            Kind::I64(_) => Kind::I64(next.into()),
            Kind::U64(_) => Kind::U64(next.into()),
            _ => return Err(fail("A color element is not an integer.")),
        };
    }
    Ok(())
}

/// Where a multiplier would change a renderer, read only: `before` is the
/// number and `location` its place when it can be multiplied, else `left` says
/// why not and `location` is the key's place.
fn site(root: &Value, at: &Location, renderer: &str, multiplier: Multiplier) -> Scaled {
    site_with(root, at, renderer, multiplier, ADDED_RENDERERS.contains(&renderer))
}
/// `site` with the add rule given: `add_absent` lets an absent key be added.
fn site_with(root: &Value, at: &Location, renderer: &str, multiplier: Multiplier, add_absent: bool) -> Scaled {
    let key = at.key(multiplier.key());
    let mut out = Scaled {
        multiplier,
        renderer: renderer.to_owned(),
        location: key.clone(),
        before: None,
        after: None,
        left: String::new(),
        added: false,
    };
    let Some(value) = key.get(root) else {
        if add_absent {
            out.added = true;
        } else {
            out.left = format!("absent; {renderer} renderers do not get this setting added");
        }
        return out;
    };
    let (number, location) = if matches!(value.kind, Kind::F32(_) | Kind::F64(_)) {
        (value, key)
    } else if matches!(value.kind, Kind::Object(_)) {
        let kind = text(value, "m_nType");
        if kind != LITERAL_FLOAT {
            out.left = format!("{} input, not a literal", if kind.is_empty() { "untyped" } else { kind });
            return out;
        }
        match value.get("m_flLiteralValue") {
            Some(v) if matches!(v.kind, Kind::F32(_) | Kind::F64(_)) => (v, key.key("m_flLiteralValue")),
            _ => {
                out.left = "literal without a number".into();
                return out;
            }
        }
    } else {
        out.left = "not a number".into();
        return out;
    };
    out.before = Some(number.number().unwrap_or(0.));
    out.location = location;
    out
}

fn scale(root: &mut Value, at: &Location, renderer: &str, multiplier: Multiplier, factor: f64) -> Scaled {
    scale_with(root, at, renderer, multiplier, factor, ADDED_RENDERERS.contains(&renderer))
}
/// `scale` with the add rule given (`site_with`).
fn scale_with(root: &mut Value, at: &Location, renderer: &str, multiplier: Multiplier, factor: f64, add_absent: bool) -> Scaled {
    let mut out = site_with(root, at, renderer, multiplier, add_absent);
    if !out.left.is_empty() {
        return out;
    }
    if out.added {
        // The key the renderer lacked (its value is 1), as the game's own files
        // mostly write it on these renderers.
        if let Some(Value { kind: Kind::Object(rows), .. }) = at.get_mut(root) {
            rows.push((multiplier.key().to_owned(), literal_float(factor)));
            out.after = Some(factor);
        } else {
            out.added = false;
            out.left = "the renderer is not an object".into();
        }
        return out;
    }
    let Some(number) = out.location.get_mut(root) else { return out };
    let before = out.before.unwrap_or(0.);
    number.kind = match number.kind {
        Kind::F32(_) => Kind::F32(scaled_value(before, factor, true) as f32),
        _ => Kind::F64(scaled_value(before, factor, false)),
    };
    out.after = Some(number.number().unwrap_or(before * factor));
    out
}

/// A multiplied renderer value as the file stores it (`f32` values are rounded
/// to `f32`), so a preview shows the number an edited file holds.
pub fn scaled_value(before: f64, factor: f64, single: bool) -> f64 {
    let after = before * factor;
    if single { f64::from(after as f32) } else { after }
}

/// The renderers' size, opacity and brightness keys a multiplier would change
/// or leave alone (`Scaled` with `after` unset), without changing anything.
pub fn multiplier_sites(root: &Value) -> Vec<Scaled> {
    let mut out = vec![];
    for (i, renderer) in root.get("m_Renderers").and_then(|v| v.array().ok()).unwrap_or(&[]).iter().enumerate() {
        let name = class(renderer);
        if !SCALED_RENDERERS.contains(&name) {
            continue;
        }
        let at = Location::default().key("m_Renderers").index(i);
        for multiplier in Multiplier::ALL {
            out.push(site(root, &at, name, multiplier));
        }
    }
    out
}
/// Whether the number at a site is stored as `f32`.
pub fn single_precision(root: &Value, site: &Scaled) -> bool {
    site.location.get(root).is_some_and(|v| matches!(v.kind, Kind::F32(_)))
}

fn check_multipliers(multipliers: &[(Multiplier, f64)]) -> Result<()> {
    if multipliers.iter().any(|(_, v)| !(*v > 0. && *v <= 16.)) {
        return Err(fail("Effect size, opacity and brightness multipliers must be above 0 and at most 16."));
    }
    Ok(())
}

/// Apply `edit` to a particle system's DATA document.
pub fn apply(root: &mut Value, edit: &EffectEdit) -> Result<EditReport> {
    edit.validate()?;
    let shift = *edit;
    let multipliers: Vec<_> = edit.multipliers().collect();
    apply_with(root, &|slot: &ColorSlot| shift_color(slot.rgb, &shift), &multipliers)
}

/// Apply a color per listed slot (`recolor` returns the slot's new color;
/// returning `slot.rgb` leaves it) and renderer multipliers (a factor of 1
/// leaves that key alone) to a particle system's DATA document.
pub fn apply_with(root: &mut Value, recolor: &dyn Fn(&ColorSlot) -> [u8; 3], multipliers: &[(Multiplier, f64)]) -> Result<EditReport> {
    check_multipliers(multipliers)?;
    let colors = colors(root);
    let mut report = EditReport { colors, ..Default::default() };
    for slot in &report.colors.colors {
        let next = recolor(slot);
        if next != slot.rgb {
            let value = slot.location.get_mut(root).ok_or_else(|| fail("A color moved while editing."))?;
            set_rgb(value, next)?;
            report.recolored.push(slot.location.clone());
        }
    }
    let multipliers: Vec<_> = multipliers.iter().filter(|(_, v)| *v != 1.).copied().collect();
    if !multipliers.is_empty() {
        let count = root.get("m_Renderers").and_then(|v| v.array().ok()).map_or(0, <[Value]>::len);
        for i in 0..count {
            let at = Location::default().key("m_Renderers").index(i);
            let renderer = at.get(root).map(class).unwrap_or("").to_owned();
            if !SCALED_RENDERERS.contains(&renderer.as_str()) {
                continue;
            }
            for (multiplier, factor) in &multipliers {
                report.scaled.push(scale(root, &at, &renderer, *multiplier, *factor));
            }
        }
    }
    Ok(report)
}

/// A file after `edit_file`.
#[derive(Clone, Debug)]
pub struct EditedFile {
    pub bytes: Vec<u8>,
    pub report: EditReport,
    pub data_before: usize,
    pub data_after: usize,
}

/// One compiled particle system with `edit` applied. The DATA block is encoded
/// again even for an empty edit (the control); every other block keeps its
/// exact bytes, and the result must read back as the edited document and pass
/// `resources::validate_effect` against the original.
pub fn edit_file(raw: &[u8], edit: &EffectEdit) -> Result<EditedFile> {
    edit.validate()?;
    let shift = *edit;
    let multipliers: Vec<_> = edit.multipliers().collect();
    edit_file_with(raw, &|slot: &ColorSlot| shift_color(slot.rgb, &shift), &multipliers)
}

/// `edit_file` with a color per slot and multipliers, as `apply_with`.
pub fn edit_file_with(raw: &[u8], recolor: &dyn Fn(&ColorSlot) -> [u8; 3], multipliers: &[(Multiplier, f64)]) -> Result<EditedFile> {
    edit_file_controlled(raw, recolor, multipliers, &PartControls::default())
}

/// `edit_file_with` plus the part's own settings, applied after the colors
/// and the effect-wide multipliers (`apply_controls`). Without settings it is
/// `edit_file_with` exactly.
pub fn edit_file_controlled(raw: &[u8], recolor: &dyn Fn(&ColorSlot) -> [u8; 3], multipliers: &[(Multiplier, f64)], controls: &PartControls) -> Result<EditedFile> {
    let mut doc = document(raw)?;
    let mut report = apply_with(&mut doc.root, recolor, multipliers)?;
    report.controlled = apply_controls(&mut doc.root, controls)?;
    let layout = container::layout(raw).map_err(fail)?;
    let data_before = container::unique(&layout.blocks, "DATA").map_err(fail)?.size;
    let encoded = binary::encode(&doc.root, doc.format_guid).map_err(fail)?;
    let data_after = encoded.len();
    let blocks = BTreeMap::from([("DATA".to_owned(), encoded)]);
    let bytes = container::rebuild(raw, &blocks).map_err(fail)?;
    // The new file must read back as the edited system, and every other block
    // must still be the game's exact bytes.
    let check = document(&bytes)?;
    if !check.root.canonical_eq(&doc.root) || check.format_guid != doc.format_guid {
        return Err(fail("The edited particle system did not read back as edited."));
    }
    let after = container::layout(&bytes).map_err(fail)?;
    if layout.blocks.len() != after.blocks.len()
        || layout.blocks.iter().zip(&after.blocks).any(|(a, b)| {
            a.name != b.name
                || (a.name != "DATA" && container::payload(raw, a).ok() != container::payload(&bytes, b).ok())
        })
    {
        return Err(fail("The edited particle system changed outside its DATA block."));
    }
    resources::validate_effect(raw, &bytes)?;
    Ok(EditedFile { bytes, report, data_before, data_after })
}

// ---------------------------------------------------------------- one part's own settings

/// The 3D-model renderer: the only one the part's own Size and Opacity reach.
pub const MODEL_RENDERER: &str = "C_OP_RenderModels";
/// Particle-system key of the part's own color (absent = white).
const CONSTANT_COLOR: &str = "m_ConstantColor";
/// The key that switches a particle function off (absent = on).
pub const DISABLE_KEY: &str = "m_bDisableOperator";

/// The settings of one part (`deadlock_project::effect_edit::PartSettings`)
/// as `apply_controls` writes them, in this order:
/// - `hidden`: `m_bDisableOperator: true` on every renderer of the file (the
///   part keeps simulating, so its children and anything reading its
///   particles keep working);
/// - `size`, `opacity`: `m_flRadiusScale` / `m_flAlphaScale` on
///   `C_OP_RenderModels` renderers only: a plain number or a
///   `PF_TYPE_LITERAL` is multiplied (after any effect-wide multiplier), an
///   absent key is added as a `PF_TYPE_LITERAL` of the factor, an input the
///   game sets while playing is left and reported;
/// - `tint`: `m_ConstantColor` = `[r, g, b, 255]` (integers) added to the
///   system, only where `tint_refusal` finds nothing against it;
/// - `light`: the brightness number of each light renderer `light_sites`
///   finds changeable, multiplied in place (never added).
///
/// Added keys are appended after the object's existing keys. A factor of 1,
/// `hidden: false` and no tint change nothing.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PartControls {
    pub hidden: bool,
    pub size: Option<f64>,
    pub opacity: Option<f64>,
    pub tint: Option<[u8; 3]>,
    /// Light brightness multiplier, 0 to `LIGHT_MAX`.
    pub light: Option<f64>,
}
impl PartControls {
    pub fn is_neutral(&self) -> bool {
        !self.hidden && self.size.is_none_or(|v| v == 1.) && self.opacity.is_none_or(|v| v == 1.) && self.tint.is_none() && self.light.is_none_or(|v| v == 1.)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Control {
    Hide,
    Size,
    Opacity,
    Tint,
    Light,
}

/// The largest light brightness multiplier (`deadlock_project::effect_edit::LIGHT_MAX`).
pub use deadlock_project::effect_edit::{LIGHT_MAX, LIGHT_RANGE};
/// Light renderers whose brightness number a part's Light brightness
/// multiplies, with that key.
pub const LIGHT_KEYS: [(&str, &str); 2] = [("C_OP_RenderStandardLight", "m_flIntensity"), ("C_OP_RenderOmni2Light", "m_flBrightnessLumens")];
/// Every renderer class that draws a light (`effect_trees` calls these parts lights).
pub const LIGHT_RENDERERS: [&str; 3] = ["C_OP_RenderStandardLight", "C_OP_RenderOmni2Light", "C_OP_RenderDeferredLight"];
/// The point light type (also what a renderer without a light type is).
/// Only point lights are changed: for the other types (spot, capsule, and
/// the lights on effects only) the multiplier's effect is not known.
const POINT_LIGHT: &str = "PARTICLE_LIGHT_TYPE_POINT";
pub const LIGHT_NONE: &str = "no light in this part";
pub const LIGHT_GAME_SET: &str = "the game sets this light's brightness while it plays";
pub const LIGHT_ABSENT: &str = "this light has no brightness value in its file";
pub const LIGHT_KIND: &str = "this kind of light cannot be changed yet";

/// One light renderer of a part, read only.
#[derive(Clone, Debug, PartialEq)]
pub struct LightSite {
    pub renderer: String,
    /// The number a multiplier changes (the literal's `m_flLiteralValue`),
    /// or the renderer's place when it cannot be changed.
    pub location: Location,
    /// The file's number, when it can be changed.
    pub before: Option<f64>,
    /// The number is stored as `f32`.
    pub single: bool,
    /// Why it is left as it is (`LIGHT_GAME_SET`, `LIGHT_ABSENT`,
    /// `LIGHT_KIND`); `None` when it can be changed.
    pub refusal: Option<&'static str>,
}

/// Every light renderer of the part and whether its brightness can be
/// multiplied: the ONE rule for the offer and the file. Changeable is a
/// `C_OP_RenderStandardLight` `m_flIntensity` or a `C_OP_RenderOmni2Light`
/// `m_flBrightnessLumens` of a point light (no light type, or the point
/// type) that the file holds as a `PF_TYPE_LITERAL` with an `f32` or `f64`
/// number. An absent key is never added; any other input is the game's;
/// other light classes, other light types and other number forms are left.
pub fn light_sites(root: &Value) -> Vec<LightSite> {
    let mut out = vec![];
    for (i, renderer) in root.get("m_Renderers").and_then(|v| v.array().ok()).unwrap_or(&[]).iter().enumerate() {
        let name = class(renderer);
        if !LIGHT_RENDERERS.contains(&name) {
            continue;
        }
        let at = Location::default().key("m_Renderers").index(i);
        let mut site = LightSite { renderer: name.to_owned(), location: at.clone(), before: None, single: false, refusal: None };
        // Any light type other than a point light, or one that is not a name, is left.
        let other = renderer.get("m_nLightType").is_some_and(|t| t.string().ok().is_none_or(|s| s != POINT_LIGHT));
        let Some((_, key)) = LIGHT_KEYS.iter().find(|(c, _)| *c == name).filter(|_| !other) else {
            site.refusal = Some(LIGHT_KIND);
            out.push(site);
            continue;
        };
        site.refusal = match renderer.get(key) {
            None => Some(LIGHT_ABSENT),
            Some(value) if matches!(value.kind, Kind::Object(_)) => {
                if text(value, "m_nType") != LITERAL_FLOAT {
                    Some(LIGHT_GAME_SET)
                } else {
                    match value.get("m_flLiteralValue") {
                        Some(number) if matches!(number.kind, Kind::F32(_) | Kind::F64(_)) => {
                            site.location = at.key(key).key("m_flLiteralValue");
                            site.before = number.number().ok();
                            site.single = matches!(number.kind, Kind::F32(_));
                            None
                        }
                        _ => Some(LIGHT_ABSENT),
                    }
                }
            }
            Some(_) => Some(LIGHT_KIND),
        };
        if site.refusal.is_some() {
            site.location = at;
        }
        out.push(site);
    }
    out
}
/// A light brightness number after the multiplier, as the file stores it
/// (`f32` rounded); 0 is written as +0.
pub fn light_value(before: f64, factor: f64, single: bool) -> f64 {
    if factor == 0. { 0. } else { scaled_value(before, factor, single) }
}
/// Why the part takes no Light brightness (`None` when at least one of its
/// lights can be changed): `LIGHT_NONE` without a light, else the first
/// light's reason.
pub fn light_refusal(root: &Value) -> Option<&'static str> {
    let sites = light_sites(root);
    if sites.iter().any(|s| s.refusal.is_none()) {
        return None;
    }
    Some(sites.first().and_then(|s| s.refusal).unwrap_or(LIGHT_NONE))
}
/// A value one of the part's own settings changed, added or left.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Controlled {
    pub control: Control,
    /// The renderer's class (`CParticleSystemDefinition` for the tint; empty
    /// when the part has no renderer the setting applies to).
    pub renderer: String,
    /// The value changed or added, or the key's place when left alone.
    pub location: Location,
    pub before: Option<f64>,
    pub after: Option<f64>,
    /// True when the key was absent and is (or would be) added.
    pub added: bool,
    /// Why it was left alone; empty when changed or added.
    pub left: String,
}

/// How a 3D-model renderer's color is decided.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ModelColour {
    /// `m_bSuppressTint`: the particle color is not applied as the model's tint.
    pub suppress_tint: bool,
    /// `m_MaterialVars` fed from particle field 6 or 22 (color).
    pub colour_vars: bool,
}
impl ModelColour {
    /// The model takes the particle color as its tint, plainly.
    pub fn follows(&self) -> bool {
        !self.suppress_tint && !self.colour_vars
    }
}
pub fn model_colour(renderer: &Value) -> ModelColour {
    let vars = renderer.get("m_MaterialVars").and_then(|v| v.array().ok()).unwrap_or(&[]);
    ModelColour {
        suppress_tint: renderer.get("m_bSuppressTint").is_some_and(|v| v.kind == Kind::Bool(true)),
        colour_vars: vars.iter().any(|v| field(v, "m_nVariableField").is_some_and(|f| COLOR_FIELDS.contains(&f))),
    }
}

pub const TINT_NOT_MODEL: &str = "only a 3D model part takes a tint of its own";
pub const TINT_MIXED: &str = "it also draws other things, which would take the color too";
pub const TINT_SUPPRESSED: &str = "its model does not take the particle color (its tint is switched off)";
pub const TINT_MATERIAL: &str = "its model takes the color through its material's settings";
pub const TINT_HAS_COLORS: &str = "its file has colors of its own: the effect's color controls change them";
pub const TINT_OTHER: &str = "the game sets this part's color in another way";

/// Why the part cannot take a tint (`None` when it can): every renderer is
/// a 3D model that takes the particle color plainly (`ModelColour::follows`),
/// the system has no `m_ConstantColor` and no color slot (`colors`), and
/// nothing else in it sets color.
pub fn tint_refusal(root: &Value) -> Option<&'static str> {
    let renderers = root.get("m_Renderers").and_then(|v| v.array().ok()).unwrap_or(&[]);
    let models: Vec<&Value> = renderers.iter().filter(|r| class(r) == MODEL_RENDERER).collect();
    if models.is_empty() {
        return Some(TINT_NOT_MODEL);
    }
    if models.len() != renderers.len() {
        return Some(TINT_MIXED);
    }
    let colour: Vec<ModelColour> = models.iter().map(|m| model_colour(m)).collect();
    if colour.iter().any(|c| c.suppress_tint) {
        return Some(TINT_SUPPRESSED);
    }
    if colour.iter().any(|c| c.colour_vars) {
        return Some(TINT_MATERIAL);
    }
    let found = colors(root);
    if found.constant_color || !found.colors.is_empty() {
        return Some(TINT_HAS_COLORS);
    }
    if found.fixed.iter().any(|f| f.kind != FixedKind::ModelMaterial) {
        return Some(TINT_OTHER);
    }
    None
}

/// Where the part's own Size or Opacity would change a renderer, read only
/// (`Scaled` with `after` unset): `C_OP_RenderModels` renderers only, where
/// an absent key is added.
pub fn control_sites(root: &Value) -> Vec<Scaled> {
    let mut out = vec![];
    for (i, renderer) in root.get("m_Renderers").and_then(|v| v.array().ok()).unwrap_or(&[]).iter().enumerate() {
        if class(renderer) != MODEL_RENDERER {
            continue;
        }
        let at = Location::default().key("m_Renderers").index(i);
        for multiplier in [Multiplier::Size, Multiplier::Opacity] {
            out.push(site_with(root, &at, MODEL_RENDERER, multiplier, true));
        }
    }
    out
}

/// Apply one part's own settings to its DATA document (see `PartControls`).
pub fn apply_controls(root: &mut Value, controls: &PartControls) -> Result<Vec<Controlled>> {
    let mut out = vec![];
    if controls.is_neutral() {
        return Ok(out);
    }
    if [controls.size, controls.opacity].iter().flatten().any(|v| !(*v > 0. && *v <= 16.)) {
        return Err(fail("A part's size and opacity multipliers must be above 0 and at most 16."));
    }
    if controls.light.is_some_and(|v| !(0.0..=LIGHT_MAX).contains(&v)) {
        return Err(fail(LIGHT_RANGE));
    }
    let count = root.get("m_Renderers").and_then(|v| v.array().ok()).map_or(0, <[Value]>::len);
    let left = |control, why: &str| Controlled { control, renderer: String::new(), location: Location::default(), before: None, after: None, added: false, left: why.to_owned() };
    if controls.hidden {
        if count == 0 {
            out.push(left(Control::Hide, "it draws nothing itself (it has no renderer)"));
        }
        for i in 0..count {
            let at = Location::default().key("m_Renderers").index(i);
            let Some(renderer) = at.get_mut(root) else { continue };
            let name = class(renderer).to_owned();
            let place = at.key(DISABLE_KEY);
            let mut entry = Controlled { control: Control::Hide, renderer: name, location: place, before: None, after: Some(1.), added: false, left: String::new() };
            let Kind::Object(rows) = &mut renderer.kind else {
                entry.left = "the renderer is not an object".into();
                out.push(entry);
                continue;
            };
            match rows.iter_mut().find(|(k, _)| k == DISABLE_KEY) {
                Some((_, v)) if v.kind == Kind::Bool(true) => {
                    entry.before = Some(1.);
                    entry.after = None;
                    entry.left = "already switched off in the game's file".into();
                }
                Some((_, v)) => {
                    entry.before = Some(0.);
                    v.kind = Kind::Bool(true);
                }
                None => {
                    entry.added = true;
                    rows.push((DISABLE_KEY.to_owned(), Value::new(Kind::Bool(true))));
                }
            }
            out.push(entry);
        }
    }
    for (control, multiplier, factor) in [(Control::Size, Multiplier::Size, controls.size), (Control::Opacity, Multiplier::Opacity, controls.opacity)] {
        let Some(factor) = factor.filter(|f| *f != 1.) else { continue };
        let mut any = false;
        for i in 0..count {
            let at = Location::default().key("m_Renderers").index(i);
            if at.get(root).map(class) != Some(MODEL_RENDERER) {
                continue;
            }
            any = true;
            let s = scale_with(root, &at, MODEL_RENDERER, multiplier, factor, true);
            out.push(Controlled { control, renderer: s.renderer, location: s.location, before: s.before, after: s.after, added: s.added, left: s.left });
        }
        if !any {
            out.push(left(control, "this part is not a 3D model"));
        }
    }
    if let Some(rgb) = controls.tint {
        match tint_refusal(root) {
            Some(why) => out.push(Controlled { control: Control::Tint, ..left(Control::Tint, why) }),
            None => {
                let Kind::Object(rows) = &mut root.kind else { return Err(fail("A particle system is not an object.")) };
                let value = Value::new(Kind::Array([rgb[0], rgb[1], rgb[2], 255].iter().map(|c| Value::new(Kind::I64(i64::from(*c)))).collect()));
                rows.push((CONSTANT_COLOR.to_owned(), value));
                out.push(Controlled {
                    control: Control::Tint,
                    renderer: "CParticleSystemDefinition".into(),
                    location: Location::default().key(CONSTANT_COLOR),
                    before: None,
                    after: None,
                    added: true,
                    left: String::new(),
                });
            }
        }
    }
    // Light brightness: the numbers `light_sites` finds changeable, multiplied
    // in place keeping their width; nothing is added.
    if let Some(factor) = controls.light.filter(|f| *f != 1.) {
        let sites = light_sites(root);
        if sites.is_empty() {
            out.push(left(Control::Light, LIGHT_NONE));
        }
        for site in sites {
            let mut entry = Controlled { control: Control::Light, renderer: site.renderer.clone(), location: site.location.clone(), before: site.before, after: None, added: false, left: String::new() };
            match (site.refusal, site.before) {
                (None, Some(before)) => {
                    let number = site.location.get_mut(root).ok_or_else(|| fail("A light's brightness moved while editing."))?;
                    let after = light_value(before, factor, site.single);
                    number.kind = if site.single { Kind::F32(after as f32) } else { Kind::F64(after) };
                    entry.after = Some(after);
                }
                (why, _) => entry.left = why.unwrap_or(LIGHT_ABSENT).to_owned(),
            }
            out.push(entry);
        }
    }
    Ok(out)
}

/// Leaf locations whose values differ between two documents (canonically:
/// float widths and integer kinds that hold the same value are equal). A key
/// present in one only is reported at the key.
pub fn differences(a: &Value, b: &Value) -> Vec<Location> {
    fn walk(a: &Value, b: &Value, at: &Location, out: &mut Vec<Location>) {
        match (&a.kind, &b.kind) {
            (Kind::Object(x), Kind::Object(y)) if a.annotation == b.annotation => {
                let keys: BTreeSet<&str> = x.iter().chain(y).map(|(k, _)| k.as_str()).collect();
                for key in keys {
                    match (a.get(key), b.get(key)) {
                        (Some(p), Some(q)) => walk(p, q, &at.key(key), out),
                        _ => out.push(at.key(key)),
                    }
                }
            }
            (Kind::Array(x), Kind::Array(y)) if x.len() == y.len() && a.annotation == b.annotation => {
                for (i, (p, q)) in x.iter().zip(y).enumerate() {
                    walk(p, q, &at.index(i), out);
                }
            }
            _ => {
                if !a.canonical_eq(b) {
                    out.push(at.clone());
                }
            }
        }
    }
    let mut out = vec![];
    walk(a, b, &Location::default(), &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadlock_resource::Annotation;

    fn v(kind: Kind) -> Value {
        Value::new(kind)
    }
    fn s(text: &str) -> Value {
        v(Kind::String(text.into()))
    }
    fn f(n: f64) -> Value {
        v(Kind::F64(n))
    }
    fn i(n: i64) -> Value {
        v(Kind::I64(n))
    }
    fn color(c: &[i64]) -> Value {
        v(Kind::Array(c.iter().map(|n| i(*n)).collect()))
    }
    fn obj(rows: Vec<(&str, Value)>) -> Value {
        v(Kind::Object(rows.into_iter().map(|(k, v)| (k.to_owned(), v)).collect()))
    }
    fn arr(rows: Vec<Value>) -> Value {
        v(Kind::Array(rows))
    }
    fn gradient(kind: &str, stops: &[&[i64]]) -> Value {
        obj(vec![
            ("m_nType", s(kind)),
            ("m_LiteralColor", color(&[0, 0, 0])),
            (
                "m_Gradient",
                obj(vec![(
                    "m_Stops",
                    arr(stops.iter().map(|c| obj(vec![("m_flPosition", f(0.)), ("m_Color", color(c))])).collect()),
                )]),
            ),
        ])
    }
    fn literal_color(c: &[i64]) -> Value {
        obj(vec![("m_nType", s(LITERAL_COLOR)), ("m_LiteralColor", color(c))])
    }
    fn literal_float(n: f64) -> Value {
        obj(vec![("m_nType", s(LITERAL_FLOAT)), ("m_flLiteralValue", f(n))])
    }
    fn child(path: &str) -> Value {
        let mut value = s(path);
        value.annotation = Annotation::Resource;
        obj(vec![("m_ChildRef", value)])
    }
    /// A particle system that has one of every color source this module knows,
    /// stale literals and gradients it must ignore, and things it cannot change.
    fn system() -> Value {
        obj(vec![
            ("_class", s("CParticleSystemDefinition")),
            ("m_nMaxParticles", i(16)),
            ("m_ConstantColor", color(&[20, 200, 180, 255])),
            (
                "m_Initializers",
                arr(vec![
                    obj(vec![
                        ("_class", s("C_INIT_RandomColor")),
                        ("m_ColorMin", color(&[255, 0, 0])),
                        ("m_ColorMax", color(&[128, 128, 128])),
                    ]),
                    // A vector input of another type: stale literal color and gradient.
                    obj(vec![
                        ("_class", s("C_INIT_InitVec")),
                        ("m_nOutputField", i(21)),
                        ("m_InputValue", {
                            let mut g = gradient("PVEC_TYPE_LITERAL", &[&[10, 20, 30]]);
                            g.get_mut("m_LiteralColor").unwrap().kind = color(&[5, 6, 7]).kind;
                            g
                        }),
                    ]),
                    // No field: it inherits the radius, not color.
                    obj(vec![("_class", s("C_INIT_InheritFromParentParticles"))]),
                    obj(vec![
                        ("_class", s("C_INIT_InheritFromParentParticles")),
                        ("m_nFieldOutput", i(7)),
                    ]),
                    obj(vec![
                        ("_class", s("C_INIT_InheritFromParentParticles")),
                        ("m_nFieldOutput", i(6)),
                    ]),
                ]),
            ),
            (
                "m_Operators",
                arr(vec![
                    obj(vec![("_class", s("C_OP_ColorInterpolate")), ("m_ColorFade", color(&[0, 0, 255, 128]))]),
                    obj(vec![
                        ("_class", s("C_OP_SetVec")),
                        ("m_InputValue", gradient(GRADIENT, &[&[0, 255, 0], &[0, 0, 0]])),
                    ]),
                    // Literal color written to a field that is not color.
                    obj(vec![
                        ("_class", s("C_OP_SetVec")),
                        ("m_InputValue", literal_color(&[244, 52, 52])),
                        ("m_nOutputField", i(30)),
                    ]),
                    obj(vec![
                        ("_class", s("C_OP_SetVec")),
                        ("m_InputValue", obj(vec![("m_nType", s("PVEC_TYPE_CP_VALUE")), ("m_nCP", i(3))])),
                    ]),
                    obj(vec![("_class", s("C_OP_RemapCPtoVector")), ("m_nFieldOutput", i(6))]),
                ]),
            ),
            (
                "m_Renderers",
                arr(vec![
                    obj(vec![
                        ("_class", s("C_OP_RenderSprites")),
                        ("m_vecColorScale", literal_color(&[193, 255, 236])),
                        ("m_flRadiusScale", literal_float(2.)),
                        ("m_flOverbrightFactor", f(3.)),
                        (
                            "m_flAlphaScale",
                            obj(vec![("m_nType", s("PF_TYPE_COLLECTION_AGE")), ("m_flLiteralValue", f(0.))]),
                        ),
                        (
                            "m_vecTexturesInput",
                            arr(vec![
                                obj(vec![
                                    ("m_bReplaceTextureWithGradient", v(Kind::Bool(true))),
                                    ("m_Gradient", gradient(GRADIENT, &[&[115, 254, 255]]).get("m_Gradient").unwrap().clone()),
                                ]),
                                // Not replaced: its gradient is never read.
                                obj(vec![("m_Gradient", gradient(GRADIENT, &[&[9, 9, 200]]).get("m_Gradient").unwrap().clone())]),
                            ]),
                        ),
                    ]),
                    obj(vec![("_class", s("C_OP_RenderModels"))]),
                    obj(vec![("_class", s("C_OP_RenderStandardLight")), ("m_flRadiusScale", f(5.))]),
                ]),
            ),
            ("m_Children", arr(vec![child("particles/a/child.vpcf")])),
        ])
    }

    #[test]
    fn colors_are_found_by_input_type_and_field() {
        let root = system();
        let found = colors(&root);
        let names: Vec<String> = found.colors.iter().map(|c| c.location.to_string()).collect();
        assert_eq!(
            names,
            [
                "m_ConstantColor",
                "m_Initializers[0].m_ColorMin",
                "m_Initializers[0].m_ColorMax",
                "m_Operators[0].m_ColorFade",
                "m_Operators[1].m_InputValue.m_Gradient.m_Stops[0].m_Color",
                "m_Operators[1].m_InputValue.m_Gradient.m_Stops[1].m_Color",
                "m_Renderers[0].m_vecColorScale.m_LiteralColor",
                "m_Renderers[0].m_vecTexturesInput[0].m_Gradient.m_Stops[0].m_Color",
            ]
        );
        assert!(found.constant_color);
        assert_eq!(found.colors[3].alpha, Some(128));
        assert_eq!(found.colors[0].source, ColorSource::Constant);
        let fixed: Vec<(String, &str)> =
            found.fixed.iter().map(|f| (f.location.to_string(), f.reason.as_str())).collect();
        assert_eq!(
            fixed,
            [
                ("m_Initializers[4]".into(), "color inherited from the parent system's particles"),
                ("m_Operators[2].m_InputValue".into(), "a color written to particle field 30, not to color"),
                ("m_Operators[3].m_InputValue".into(), "color from a PVEC_TYPE_CP_VALUE input"),
                ("m_Operators[4]".into(), "color from a control point"),
                ("m_Renderers[1]".into(), "a model's own colors come from its materials"),
            ]
        );
    }

    #[test]
    fn hue_only_edits_keep_greys_and_identity_keeps_everything() {
        let hue = EffectEdit { hue: 123., ..Default::default() };
        for g in [0u8, 1, 74, 128, 254, 255] {
            assert_eq!(shift_color([g, g, g], &hue), [g, g, g]);
        }
        assert_eq!(shift_color([255, 0, 0], &EffectEdit { hue: 120., ..Default::default() }), [0, 255, 0]);
        assert_eq!(shift_color([255, 0, 0], &EffectEdit { hue: -120., ..Default::default() }), [0, 0, 255]);
        assert_eq!(shift_color([255, 0, 0], &EffectEdit { hue: 360., ..Default::default() }), [255, 0, 0]);
        // A full turn in two steps returns every color to within rounding.
        let half = EffectEdit { hue: 180., ..Default::default() };
        for c in [[20u8, 200, 180], [231, 22, 81], [29, 84, 68], [3, 2, 1]] {
            let back = shift_color(shift_color(c, &half), &half);
            assert!(c.iter().zip(back).all(|(a, b)| a.abs_diff(b) <= 1), "{c:?} {back:?}");
        }
        // Saturation 0 makes greys; brightness scales the brightest channel.
        assert_eq!(shift_color([255, 0, 0], &EffectEdit { saturation: 0., ..Default::default() }), [255, 255, 255]);
        assert_eq!(shift_color([200, 100, 0], &EffectEdit { brightness: 0.5, ..Default::default() }), [100, 50, 0]);
        let identity = EffectEdit::default();
        for r in (0..=255).step_by(17) {
            for g in (0..=255).step_by(51) {
                assert_eq!(shift_color([r, g, 7], &identity), [r, g, 7]);
            }
        }
    }

    #[test]
    fn apply_changes_only_listed_colors_and_literal_multipliers() {
        let original = system();
        let mut root = original.clone();
        let edit = EffectEdit { hue: 90., size: Some(1.6), glow: Some(1.5), opacity: Some(0.5), ..Default::default() };
        let report = apply(&mut root, &edit).unwrap();
        // Greys ([128,128,128], [0,0,0]) do not change under a hue edit.
        assert_eq!(report.recolored.len(), report.colors.colors.len() - 2);
        assert_only_changes(&original, &root, &report);
        // Alpha stays.
        assert_eq!(color(&[0, 0, 255, 128]).array().unwrap()[3], root.get("m_Operators").unwrap().array().unwrap()[0].get("m_ColorFade").unwrap().array().unwrap()[3]);
        let scaled: Vec<(String, Option<f64>, Option<f64>, &str)> = report
            .scaled
            .iter()
            .map(|s| (s.location.to_string(), s.before, s.after, s.left.as_str()))
            .collect();
        assert_eq!(scaled.len(), 6); // sprites and models; the light is not scaled
        assert_eq!(scaled[0], ("m_Renderers[0].m_flRadiusScale.m_flLiteralValue".into(), Some(2.), Some(3.2), ""));
        assert_eq!(scaled[1].0, "m_Renderers[0].m_flAlphaScale");
        assert_eq!(scaled[1].3, "PF_TYPE_COLLECTION_AGE input, not a literal");
        assert_eq!(scaled[2], ("m_Renderers[0].m_flOverbrightFactor".into(), Some(3.), Some(4.5), ""));
        assert!(scaled[3..].iter().all(|s| s.3.starts_with("absent")));
        assert_eq!(root.get("m_Renderers").unwrap().array().unwrap()[2].get("m_flRadiusScale").unwrap().kind, Kind::F64(5.));
        // An empty edit changes nothing.
        let mut same = original.clone();
        let report = apply(&mut same, &EffectEdit::default()).unwrap();
        assert!(report.changed().is_empty());
        assert!(same.canonical_eq(&original));
        assert!(apply(&mut same, &EffectEdit { size: Some(0.), ..Default::default() }).is_err());
        assert!(apply(&mut same, &EffectEdit { saturation: f64::NAN, ..Default::default() }).is_err());
    }

    /// Every difference lies in a location the report changed, and every
    /// changed location differs.
    fn assert_only_changes(before: &Value, after: &Value, report: &EditReport) {
        let found = differences(before, after);
        let changed = report.changed();
        assert!(!changed.is_empty());
        for l in &found {
            assert!(changed.iter().any(|c| l.starts_with(c)), "unexpected change at {l}");
        }
        for c in &changed {
            assert!(found.iter().any(|l| l.starts_with(c)), "no change at {c}");
        }
    }

    fn resource(root: &Value) -> Vec<u8> {
        let base = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/particle.bin"));
        let layout = container::layout(base).unwrap();
        let block = container::unique(&layout.blocks, "DATA").unwrap();
        let doc = binary::decode(container::payload(base, block).unwrap()).unwrap();
        let data = binary::encode(root, doc.format_guid).unwrap();
        container::rebuild(base, &BTreeMap::from([("DATA".to_owned(), data)])).unwrap()
    }

    #[test]
    fn absent_settings_are_added_on_sprites_ropes_and_trails_only() {
        let root = obj(vec![
            ("_class", s("CParticleSystemDefinition")),
            (
                "m_Renderers",
                arr(vec![
                    obj(vec![("_class", s("C_OP_RenderSprites"))]),
                    obj(vec![("_class", s("C_OP_RenderModels"))]),
                    obj(vec![("_class", s("C_OP_RenderTrails")), ("m_flRadiusScale", literal_float(2.))]),
                    obj(vec![
                        ("_class", s("C_OP_RenderRopes")),
                        ("m_flRadiusScale", obj(vec![("m_nType", s("PF_TYPE_COLLECTION_AGE"))])),
                    ]),
                ]),
            ),
        ]);
        let mut edited = root.clone();
        let report = apply(&mut edited, &EffectEdit { size: Some(3.), ..Default::default() }).unwrap();
        let rows: Vec<(String, Option<f64>, Option<f64>, bool, &str)> =
            report.scaled.iter().map(|s| (s.location.to_string(), s.before, s.after, s.added, s.left.as_str())).collect();
        assert_eq!(rows[0], ("m_Renderers[0].m_flRadiusScale".into(), None, Some(3.), true, ""), "added on a sprite");
        assert!(rows[1].4.starts_with("absent"), "never added on a model");
        assert_eq!(rows[2], ("m_Renderers[2].m_flRadiusScale.m_flLiteralValue".into(), Some(2.), Some(6.), false, ""));
        assert_eq!(rows[3].4, "PF_TYPE_COLLECTION_AGE input, not a literal");
        let added = &edited.get("m_Renderers").unwrap().array().unwrap()[0];
        assert!(added.get("m_flRadiusScale").unwrap().canonical_eq(&literal_float(3.)));
        assert_only_changes(&root, &edited, &report);
        // The read-only sites say the same.
        let sites = multiplier_sites(&root);
        assert!(sites[0].added && sites[0].left.is_empty() && sites[0].before.is_none());
        // A multiplier of 1 adds nothing: the bytes are those without it.
        let raw = resource(&root);
        let plain = edit_file(&raw, &EffectEdit { hue: 30., ..Default::default() }).unwrap();
        let one = edit_file(&raw, &EffectEdit { hue: 30., size: Some(1.), opacity: Some(1.), glow: Some(1.), ..Default::default() }).unwrap();
        assert_eq!(plain.bytes, one.bytes);
        let bigger = edit_file(&raw, &EffectEdit { opacity: Some(0.25), ..Default::default() }).unwrap();
        assert_ne!(bigger.bytes, plain.bytes);
    }

    #[test]
    fn edited_files_keep_every_other_block_and_pass_the_effect_check() {
        let raw = resource(&system());
        let edit = EffectEdit { hue: 200., size: Some(1.6), glow: Some(1.5), ..Default::default() };
        let edited = edit_file(&raw, &edit).unwrap();
        assert_eq!(edited.report.recolored.len(), 6);
        let back = document(&edited.bytes).unwrap().root;
        assert_only_changes(&document(&raw).unwrap().root, &back, &edited.report);
        let control = edit_file(&raw, &EffectEdit::default()).unwrap();
        assert!(document(&control.bytes).unwrap().root.canonical_eq(&document(&raw).unwrap().root));
        let (a, b) = (container::layout(&raw).unwrap(), container::layout(&edited.bytes).unwrap());
        for (x, y) in a.blocks.iter().zip(&b.blocks) {
            assert_eq!(x.name, y.name);
            if x.name != "DATA" {
                assert_eq!(container::payload(&raw, x).unwrap(), container::payload(&edited.bytes, y).unwrap());
            }
        }
        assert!(edit_file(b"not a resource", &edit).is_err());
    }

    #[test]
    fn trees_follow_children_once_case_blind_and_report_missing_ones() {
        let make = |kids: &[&str]| {
            let mut root = system();
            root.get_mut("m_Children").unwrap().kind = Kind::Array(kids.iter().map(|k| child(k)).collect());
            resource(&root)
        };
        let files = BTreeMap::from([
            ("particles/top.vpcf_c", make(&["particles/A.vpcf", "particles/b.vpcf"])),
            ("particles/a.vpcf_c", make(&["particles/b.vpcf", "particles/top.vpcf", "particles/gone.vpcf"])),
            ("particles/b.vpcf_c", make(&["particles/a.vpcf"])),
        ]);
        let tree = read_tree("particles/TOP.vpcf", |path| Ok(files.get(casefold(path).as_str()).cloned())).unwrap();
        assert_eq!(tree.top, "particles/TOP.vpcf_c");
        let paths: Vec<_> = tree.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, ["particles/TOP.vpcf_c", "particles/A.vpcf_c", "particles/b.vpcf_c"]);
        assert_eq!(tree.files[1].children, ["particles/b.vpcf_c", "particles/top.vpcf_c", "particles/gone.vpcf_c"]);
        assert_eq!(tree.missing, ["particles/gone.vpcf_c"]);
        assert!(read_tree("particles/none.vpcf", |_| Ok(None)).is_err());
    }

    fn plain_system(renderers: Vec<Value>, extra: Vec<(&str, Value)>) -> Value {
        let mut rows = vec![("_class", s("CParticleSystemDefinition")), ("m_nMaxParticles", i(8))];
        rows.extend(extra);
        rows.push(("m_Renderers", arr(renderers)));
        obj(rows)
    }
    fn controlled(raw: &[u8], controls: PartControls) -> (Value, EditedFile) {
        let edited = edit_file_controlled(raw, &|s: &ColorSlot| s.rgb, &[], &controls).unwrap();
        let back = document(&edited.bytes).unwrap().root;
        assert_only_changes(&document(raw).unwrap().root, &back, &edited.report);
        (back, edited)
    }

    #[test]
    fn hiding_switches_off_every_renderer_and_nothing_else() {
        let raw = resource(&plain_system(vec![obj(vec![("_class", s("C_OP_RenderSprites"))]), obj(vec![("_class", s("C_OP_RenderStandardLight"))])], vec![("m_Children", arr(vec![child("particles/a/kid.vpcf")]))]));
        let (back, edited) = controlled(&raw, PartControls { hidden: true, ..Default::default() });
        let rows = back.get("m_Renderers").unwrap().array().unwrap();
        assert!(rows.iter().all(|r| r.get(DISABLE_KEY).unwrap().kind == Kind::Bool(true)));
        // Appended after the renderer's own keys.
        let Kind::Object(keys) = &rows[0].kind else { panic!() };
        assert_eq!(keys.last().unwrap().0, DISABLE_KEY);
        assert_eq!(edited.report.controlled.len(), 2);
        assert!(back.get("m_Children").unwrap().canonical_eq(&arr(vec![child("particles/a/kid.vpcf")])), "children untouched");
        // Nothing to hide without renderers: reported, nothing changed.
        let none = edit_file_controlled(&resource(&plain_system(vec![], vec![])), &|s: &ColorSlot| s.rgb, &[], &PartControls { hidden: true, ..Default::default() }).unwrap();
        assert!(none.report.changed().is_empty() && !none.report.controlled[0].left.is_empty());
        // Neutral settings give the bytes of no settings.
        let plain = edit_file_with(&raw, &|s: &ColorSlot| s.rgb, &[]).unwrap();
        let neutral = edit_file_controlled(&raw, &|s: &ColorSlot| s.rgb, &[], &PartControls { size: Some(1.), opacity: Some(1.), ..Default::default() }).unwrap();
        assert_eq!(plain.bytes, neutral.bytes);
    }

    #[test]
    fn model_size_and_opacity_are_added_multiplied_or_left_on_models_only() {
        let root = plain_system(
            vec![
                obj(vec![("_class", s("C_OP_RenderModels"))]),
                obj(vec![("_class", s("C_OP_RenderModels")), ("m_flRadiusScale", literal_float(2.)), ("m_flAlphaScale", obj(vec![("m_nType", s("PF_TYPE_COLLECTION_AGE"))]))]),
                obj(vec![("_class", s("C_OP_RenderSprites"))]),
            ],
            vec![],
        );
        let raw = resource(&root);
        let (back, edited) = controlled(&raw, PartControls { size: Some(3.), opacity: Some(0.2), ..Default::default() });
        let rows = back.get("m_Renderers").unwrap().array().unwrap();
        assert!(rows[0].get("m_flRadiusScale").unwrap().canonical_eq(&literal_float(3.)), "added on a model");
        assert!(rows[0].get("m_flAlphaScale").unwrap().canonical_eq(&literal_float(0.2)));
        assert!(rows[1].get("m_flRadiusScale").unwrap().canonical_eq(&literal_float(6.)), "an existing literal is multiplied");
        assert!(rows[2].get("m_flRadiusScale").is_none(), "sprites are left to the effect-wide sliders");
        let left: Vec<&str> = edited.report.controlled.iter().map(|c| c.left.as_str()).collect();
        assert_eq!(left.iter().filter(|l| l.is_empty()).count(), 3);
        assert!(left.iter().any(|l| l.contains("PF_TYPE_COLLECTION_AGE")), "{left:?}");
        // After an effect-wide size on the same literal, the part's own size multiplies the result.
        let both = edit_file_controlled(&raw, &|s: &ColorSlot| s.rgb, &[(Multiplier::Size, 1.5)], &PartControls { size: Some(3.), ..Default::default() }).unwrap();
        let rows = document(&both.bytes).unwrap().root.get("m_Renderers").unwrap().array().unwrap().to_vec();
        assert!(rows[1].get("m_flRadiusScale").unwrap().canonical_eq(&literal_float(9.)));
        assert!(rows[0].get("m_flRadiusScale").unwrap().canonical_eq(&literal_float(3.)), "the effect-wide size still adds nothing on a model");
        // No model: nothing changes, and it says so.
        let sprite = resource(&plain_system(vec![obj(vec![("_class", s("C_OP_RenderSprites"))])], vec![]));
        let none = edit_file_controlled(&sprite, &|s: &ColorSlot| s.rgb, &[], &PartControls { size: Some(3.), ..Default::default() }).unwrap();
        assert!(none.report.changed().is_empty());
        assert!(edit_file_controlled(&sprite, &|s: &ColorSlot| s.rgb, &[], &PartControls { size: Some(0.), ..Default::default() }).is_err());
    }

    #[test]
    fn a_tint_is_added_only_to_a_model_that_takes_the_particle_color_and_has_none() {
        let model = |extra: Vec<(&str, Value)>| {
            let mut rows = vec![("_class", s("C_OP_RenderModels"))];
            rows.extend(extra);
            obj(rows)
        };
        let tint = PartControls { tint: Some([150, 40, 255]), ..Default::default() };
        let raw = resource(&plain_system(vec![model(vec![("m_hOverrideMaterial", s("materials/dev/a.vmat"))])], vec![]));
        assert_eq!(tint_refusal(&document(&raw).unwrap().root), None);
        let (back, _) = controlled(&raw, tint);
        assert!(back.get("m_ConstantColor").unwrap().canonical_eq(&color(&[150, 40, 255, 255])));
        let Kind::Object(keys) = &back.kind else { panic!() };
        assert_eq!(keys.last().unwrap().0, "m_ConstantColor", "appended last");
        let vars = arr(vec![obj(vec![("m_strVariable", s("$COLORTINT")), ("m_nVariableField", i(6))])]);
        for (root, why) in [
            (plain_system(vec![obj(vec![("_class", s("C_OP_RenderSprites"))])], vec![]), TINT_NOT_MODEL),
            (plain_system(vec![model(vec![]), obj(vec![("_class", s("C_OP_RenderSprites"))])], vec![]), TINT_MIXED),
            (plain_system(vec![model(vec![("m_bSuppressTint", v(Kind::Bool(true)))])], vec![]), TINT_SUPPRESSED),
            (plain_system(vec![model(vec![("m_MaterialVars", vars.clone())])], vec![]), TINT_MATERIAL),
            (plain_system(vec![model(vec![])], vec![("m_ConstantColor", color(&[1, 2, 3, 255]))]), TINT_HAS_COLORS),
            (plain_system(vec![model(vec![])], vec![("m_Operators", arr(vec![obj(vec![("_class", s("C_OP_RemapCPtoVector")), ("m_nFieldOutput", i(6))])]))]), TINT_OTHER),
        ] {
            assert_eq!(tint_refusal(&root), Some(why));
            let raw = resource(&root);
            let edited = edit_file_controlled(&raw, &|s: &ColorSlot| s.rgb, &[], &tint).unwrap();
            assert!(edited.report.changed().is_empty(), "{why}");
            assert_eq!(edited.report.controlled[0].left, why);
        }
        assert!(model_colour(&model(vec![("m_MaterialVars", vars)])).colour_vars);
    }

    #[test]
    fn light_brightness_multiplies_only_existing_literals_of_world_lights() {
        let light = |class: &str, extra: Vec<(&str, Value)>| {
            let mut rows = vec![("_class", s(class))];
            rows.extend(extra);
            obj(rows)
        };
        let single = obj(vec![("m_nType", s(LITERAL_FLOAT)), ("m_flLiteralValue", v(Kind::F32(0.75)))]);
        let root = plain_system(
            vec![
                light("C_OP_RenderSprites", vec![("m_flIntensity", literal_float(2.))]),
                light("C_OP_RenderStandardLight", vec![("m_flIntensity", literal_float(8.))]),
                light("C_OP_RenderOmni2Light", vec![("m_flBrightnessLumens", literal_float(500.)), ("m_flRange", literal_float(128.))]),
                light("C_OP_RenderStandardLight", vec![("m_nLightType", s("PARTICLE_LIGHT_TYPE_POINT")), ("m_flIntensity", single)]),
                light("C_OP_RenderStandardLight", vec![("m_nLightType", s("PARTICLE_LIGHT_TYPE_FX")), ("m_flIntensity", literal_float(3.))]),
                light("C_OP_RenderStandardLight", vec![]),
                light("C_OP_RenderOmni2Light", vec![("m_flBrightnessLumens", obj(vec![("m_nType", s("PF_TYPE_PARTICLE_AGE"))]))]),
                light("C_OP_RenderDeferredLight", vec![("m_flAlphaScale", f(1.))]),
                light("C_OP_RenderStandardLight", vec![("m_flIntensity", f(4.))]),
                light("C_OP_RenderStandardLight", vec![("m_nLightType", s("PARTICLE_LIGHT_TYPE_SPOT")), ("m_flIntensity", literal_float(6.))]),
                light("C_OP_RenderStandardLight", vec![("m_nLightType", s("PARTICLE_LIGHT_TYPE_CAPSULE")), ("m_flIntensity", literal_float(7.))]),
            ],
            vec![],
        );
        let refusals: Vec<Option<&str>> = light_sites(&root).iter().map(|s| s.refusal).collect();
        assert_eq!(
            refusals,
            [None, None, None, Some(LIGHT_KIND), Some(LIGHT_ABSENT), Some(LIGHT_GAME_SET), Some(LIGHT_KIND), Some(LIGHT_KIND), Some(LIGHT_KIND), Some(LIGHT_KIND)],
            "sprites are not lights"
        );
        assert_eq!(light_refusal(&root), None, "offered when one light can be changed");
        let raw = resource(&root);
        let (back, edited) = controlled(&raw, PartControls { light: Some(50.), ..Default::default() });
        let rows = back.get("m_Renderers").unwrap().array().unwrap();
        assert!(rows[1].get("m_flIntensity").unwrap().canonical_eq(&literal_float(400.)));
        assert!(rows[2].get("m_flBrightnessLumens").unwrap().canonical_eq(&literal_float(25000.)));
        assert!(rows[2].get("m_flRange").unwrap().canonical_eq(&literal_float(128.)), "the reach is left");
        let point = rows[3].get("m_flIntensity").unwrap().get("m_flLiteralValue").unwrap();
        assert_eq!(point.number().unwrap(), 37.5, "a light named as a point light is one");
        assert!(rows[4].get("m_flIntensity").unwrap().canonical_eq(&literal_float(3.)), "FX lights are left");
        assert!(rows[9].get("m_flIntensity").unwrap().canonical_eq(&literal_float(6.)), "spot lights are left");
        assert!(rows[10].get("m_flIntensity").unwrap().canonical_eq(&literal_float(7.)), "capsule lights are left");
        assert!(rows[5].get("m_flIntensity").is_none(), "an absent value is never added");
        assert!(rows[0].get("m_flIntensity").unwrap().canonical_eq(&literal_float(2.)));
        assert_eq!(rows[8].get("m_flIntensity").unwrap().kind, Kind::F64(4.), "a bare number is left");
        let changed: Vec<&Controlled> = edited.report.controlled.iter().filter(|c| c.left.is_empty()).collect();
        assert_eq!(changed.len(), 3);
        assert!(changed.iter().all(|c| c.control == Control::Light && !c.added));
        assert_eq!(edited.report.changed().len(), 3, "a part with only a light multiplier is written");
        // Keys keep their order: the number changes in place, nothing is appended.
        let keys = |v: &Value| match &v.kind {
            Kind::Object(rows) => rows.iter().map(|(k, _)| k.clone()).collect::<Vec<_>>(),
            _ => vec![],
        };
        let before = document(&raw).unwrap().root;
        for (a, b) in before.get("m_Renderers").unwrap().array().unwrap().iter().zip(rows) {
            assert_eq!(keys(a), keys(b));
        }
        // 0 dims to +0 (never -0); 1 gives the bytes of no setting.
        let (dim, _) = controlled(&raw, PartControls { light: Some(0.), ..Default::default() });
        let n = dim.get("m_Renderers").unwrap().array().unwrap()[1].get("m_flIntensity").unwrap().get("m_flLiteralValue").unwrap().number().unwrap();
        assert!(n == 0. && n.is_sign_positive());
        assert_eq!(light_value(-2., 0., false).to_bits(), 0f64.to_bits());
        let plain = edit_file_with(&raw, &|s: &ColorSlot| s.rgb, &[]).unwrap();
        assert_eq!(plain.bytes, edit_file_controlled(&raw, &|s: &ColorSlot| s.rgb, &[], &PartControls { light: Some(1.), ..Default::default() }).unwrap().bytes);
        // Out of range is refused.
        for bad in [-0.5, 50.5, f64::NAN] {
            assert!(edit_file_controlled(&raw, &|s: &ColorSlot| s.rgb, &[], &PartControls { light: Some(bad), ..Default::default() }).is_err(), "{bad}");
        }
        // Parts no light renderer changes: one reason, nothing written.
        for (renderers, why) in [
            (vec![light("C_OP_RenderSprites", vec![])], LIGHT_NONE),
            (vec![light("C_OP_RenderStandardLight", vec![])], LIGHT_ABSENT),
            (vec![light("C_OP_RenderOmni2Light", vec![("m_flBrightnessLumens", obj(vec![("m_nType", s("PF_TYPE_CONTROL_POINT_COMPONENT"))]))])], LIGHT_GAME_SET),
            (vec![light("C_OP_RenderStandardLight", vec![("m_nLightType", s("PARTICLE_LIGHT_TYPE_FX")), ("m_flIntensity", literal_float(1.))])], LIGHT_KIND),
            (vec![light("C_OP_RenderDeferredLight", vec![])], LIGHT_KIND),
        ] {
            let root = plain_system(renderers, vec![]);
            assert_eq!(light_refusal(&root), Some(why));
            let edited = edit_file_controlled(&resource(&root), &|s: &ColorSlot| s.rgb, &[], &PartControls { light: Some(2.), ..Default::default() }).unwrap();
            assert!(edited.report.changed().is_empty(), "{why}");
            assert_eq!(edited.report.controlled[0].left, why);
        }
    }

    /// Every particle system under `DMB_EFFECT_PREFIXES` (comma separated,
    /// default Lady Geist's folders) in `DMB_PAK`: the control round trip reads
    /// back value-equal and a recolor with multipliers passes the effect check.
    #[test]
    #[ignore = "needs the game's pak01_dir.vpk in DMB_PAK"]
    fn game_particles_round_trip_and_recolor() {
        let Ok(pak) = std::env::var("DMB_PAK") else { return };
        let archive = deadlock_vpk::Vpk::open(pak).unwrap();
        let prefixes = std::env::var("DMB_EFFECT_PREFIXES")
            .unwrap_or_else(|_| "particles/abilities/ghost/,particles/weapon_fx/ghost/".into());
        let prefixes: Vec<&str> = prefixes.split(',').collect();
        let edit = EffectEdit { hue: 110., size: Some(1.6), glow: Some(1.5), opacity: Some(0.8), ..Default::default() };
        let mut count = 0;
        for path in archive.entries.keys().filter(|p| p.ends_with(".vpcf_c") && prefixes.iter().any(|x| p.starts_with(x))) {
            let raw = crate::read(&archive, path).unwrap();
            let original = document(&raw).unwrap().root;
            let control = edit_file(&raw, &EffectEdit::default()).unwrap_or_else(|e| panic!("{path}: {e}"));
            assert!(document(&control.bytes).unwrap().root.canonical_eq(&original), "{path}");
            let edited = edit_file(&raw, &edit).unwrap_or_else(|e| panic!("{path}: {e}"));
            let changed = edited.report.changed();
            let mut found = differences(&original, &document(&edited.bytes).unwrap().root);
            found.retain(|l| !changed.iter().any(|c| l.starts_with(c)));
            assert!(found.is_empty(), "{path}: unexpected changes {found:?}");
            count += 1;
        }
        assert!(count > 0, "no particle systems under {prefixes:?}");
        eprintln!("{count} particle systems round-tripped and recolored");
    }
}
