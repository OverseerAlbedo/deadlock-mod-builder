//! Extras → Effects: a hero's effect trees, ready for a page to list and to
//! preview an edit (`HeroEffects::build`, `HeroEffects::preview`). Nothing
//! here writes a file.
//!
//! A tree is a top particle system of the hero's Effects catalog and the files
//! `effect_edit::read_tree` reaches from it. A top is a catalog file that no
//! other catalog file names as a child; files a child cycle leaves unreached
//! become tops too (in path order). Trees are grouped by the hero's own
//! ability that names the top (or else one of its files), in the hero's slot
//! order (abilities 1–4, weapon, melee, innate). Trees no own ability names go,
//! in this order: to the ability of a model an own ability names (a summon, a
//! turret: `grouped_by: reference`); else by name stem or folder
//! (`fallback_groups`); else by what the game's data says about them: named by
//! the hero's models (this one and the roster's other models of the hero), their
//! animation graphs, clips or sound events (`resources::model_particles`) → the
//! hero's own group (`grouped_by: model`); named by another live definition or
//! another particle system outside the hero's catalog → `ELSEWHERE_GROUP`;
//! nothing names them → by file name, unless a definition the game no longer
//! uses names them (a leftover): an own bound ability's internal name in the
//! file name (`name_match`); else the alias list
//! (`resources/catalog/effect_aliases.json`, `alias_match`); else a word of
//! exactly one own ability's display name (`display_match`); else the word
//! "ult" → the fourth ability (`names_ult`), the last two never for a head or
//! showcase effect; else a hero-level first word (`HERO_LEVEL_WORDS`) → the
//! hero's own group (all `grouped_by: name`); else `UNLINKED_GROUP`,
//! folded on the page. Game code can name particle files too, so that last
//! group is never called unused. Last, a tree that is a part's low-violence
//! substitute (`m_hLowViolenceDef`) goes where that part's tree went
//! (`reference`). A bound signature ability with no tree of its own still gets
//! an empty group with a note (its effects are files outside the hero's
//! folders). See `effect_catalog` for ownership and "Also used by", and
//! `effect_colors` for strips, weights, the palette and the main color.
//!
//! Following a recolor: a model renderer (its materials), a projected
//! renderer (its material's texture) and a texture that is itself colored
//! (alpha-weighted mean saturation above `COLORED_TEXTURE`) certainly keep
//! their own colors (`does_not_follow`). Inherited colors, control points,
//! other inputs, status-effect shaders and textures whose colors could not be
//! checked (kept outside the hero's game archive, or not decodable) are
//! uncertain (`may_not_follow`). A colored texture's own tint stays mixed in
//! while the part's color changes.
use crate::effect_catalog::{self, GameIndex, LeftOut, Usage, User};
use crate::effect_colors::{self as colors, Main, Pick, Rule, Strip, StripKind, Swatch, TreeEdit, TreeEditExt};
use crate::effect_edit::{self, FixedKind, Multiplier, Scaled};
use crate::resources::CatalogRow;
use crate::{CancellationToken, Result, compiled, fail, heroes, sound_tags};
use deadlock_core::text::casefold;
use deadlock_resource::{Kind, Value};
use deadlock_vpk::Vpk;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const EFFECT_TREES_SCHEMA: u32 = 1;
/// A renderer texture whose alpha-weighted mean color has at least this HSV
/// saturation is colored: its color stays whatever the edit.
pub const COLORED_TEXTURE: f64 = 0.2;
/// Mip size read for texture statistics.
const TEXTURE_SAMPLE: u32 = 64;
pub const UNLINKED_GROUP: &str = "unlinked";
/// Trees no own ability names but another live definition or effect does.
pub const ELSEWHERE_GROUP: &str = "elsewhere";
/// Group orders: the hero's own group, `ELSEWHERE_GROUP`, `UNLINKED_GROUP`.
const HERO_ORDER: u32 = 350;
const ELSEWHERE_ORDER: u32 = 800;
const UNLINKED_ORDER: u32 = 900;
/// The notes of the groups the page explains.
pub fn unlinked_note() -> String {
    "Nothing in the game's definitions, the hero's model or its animations names these files. The game may still play some of them, or none.".into()
}
pub fn elsewhere_note(hero: &str) -> String {
    format!("No ability of {hero} names these, but other game effects or definitions do (see Also used by).")
}
pub fn empty_ability_note(hero: &str) -> String {
    format!("The game's definitions link none of {hero}'s own effect files to this ability. If it has any, they are under Other files.")
}
/// At most this many users are named in a note; the rest are counted.
const NOTE_NAMES: usize = 6;

// ---------------------------------------------------------------- output types

#[derive(Debug, Clone, Serialize)]
pub struct EffectTrees {
    pub schema: u32,
    pub policy: &'static str,
    /// The hero model (`.vmdl`).
    pub hero: String,
    pub hero_name: String,
    /// Whether the game's definitions and English names were read.
    pub sources: Sources,
    pub groups: Vec<Group>,
    pub trees: Vec<TreeInfo>,
    pub counts: Counts,
    pub warnings: Vec<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct Sources {
    pub definitions: bool,
    pub names: bool,
}
#[derive(Debug, Clone, Default, Serialize)]
pub struct Counts {
    pub trees: usize,
    /// Parts over all trees (a file in two trees counts twice).
    pub parts: usize,
    /// Distinct files over all trees.
    pub files: usize,
    pub linked: usize,
    pub unlinked: usize,
    /// Trees with an "Also used by" note.
    pub with_note: usize,
    /// Files that belong to more than one tree.
    pub files_in_several_trees: usize,
    pub may_not_follow: usize,
    pub does_not_follow: usize,
    /// References left out of the notes by the live rule.
    pub left_out: usize,
}
#[derive(Debug, Clone, Serialize)]
pub struct Group {
    /// `ability:<record>`, `weapon:<record>`, `melee:<record>`, `innate:<record>`, `hero:<record>`, `elsewhere` or `unlinked`.
    pub key: String,
    pub name: String,
    pub order: u32,
    /// `ability`, `weapon`, `melee`, `innate`, `hero`, `elsewhere` or `unlinked`.
    pub kind: &'static str,
    /// One line the page shows under the group's name (absent when none).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// The page shows the group folded until opened (search, the Changed filter
    /// and edited effects still reach into it).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub folded: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub record: Option<String>,
    /// The game's name of the record when the group's name is generic ("Weapon").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub record_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slot: Option<String>,
    pub trees: usize,
}
#[derive(Debug, Clone, Serialize)]
pub struct TreeInfo {
    /// The top's compiled path: the tree's id.
    pub target: String,
    /// Name within its group ("Explode").
    pub label: String,
    /// Name with its group ("Essence Bomb · explode").
    pub name: String,
    pub group: String,
    pub group_name: String,
    /// How the group was found: `definition` (an own ability names the tree),
    /// `reference` (a model an own ability names, or the part whose low-violence
    /// substitute it is), `name` (by its file name only: it shares its name stem
    /// with that ability's trees, `fallback_groups`; it holds the ability's
    /// internal name, `name_match`; or a hero-level word, `HERO_LEVEL_WORDS`),
    /// `folder` (a weapon or melee folder), `model` (the hero's models or
    /// animations name it) or `none` (`elsewhere` and `unlinked`).
    pub grouped_by: &'static str,
    /// `hero`, `ability`, `weapon` (any combination).
    pub tags: Vec<&'static str>,
    /// Own records that name the top or one of its files.
    pub records: Vec<String>,
    pub parts: Vec<PartInfo>,
    /// Children the game files do not have.
    pub missing: Vec<String>,
    pub palette: Vec<Swatch>,
    pub main: Option<Main>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub also_used_by: Option<AlsoUsed>,
    /// References the live rule left out of the note.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub left_out: Vec<LeftOut>,
    /// Other trees of this hero that hold one of this tree's files.
    pub shared_with_trees: Vec<String>,
    pub follow: FollowCounts,
    /// Parts outside the hero's own folders (`PartInfo::outside_hero`): left out
    /// of an edit unless listed in `TreeEdit.shared`.
    pub outside_parts: usize,
    pub multipliers: BTreeMap<Multiplier, SiteCounts>,
    /// Own abilities whose internal names fit the file name equally well (`name_match`):
    /// the tree is left where it is. Not sent to the page.
    #[serde(skip)]
    pub name_candidates: Vec<String>,
    /// The rule that placed the tree (finer than `grouped_by`): `definition`,
    /// `summon_model`, `low_violence`, `stem`, `internal_name`, `hero_word`,
    /// `folder`, `model` or `none`. Not sent to the page.
    #[serde(skip)]
    pub rule: &'static str,
}
#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct FollowCounts {
    pub may_not: usize,
    pub does_not: usize,
}
#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct SiteCounts {
    /// Renderers whose existing setting the multiplier changes.
    pub changes: usize,
    /// Renderers that get the setting added (sprites, ropes and trails without it).
    pub adds: usize,
    /// Renderers of a kind this editor gives no such setting (model renderers without it).
    pub no_setting: usize,
    /// Renderers whose setting the game sets while playing (an input, not a number).
    pub game_input: usize,
    /// Parts the multiplier reaches (it changes or adds a setting there).
    pub parts_reached: usize,
    /// Parts that draw something.
    pub visible_parts: usize,
}
#[derive(Debug, Clone, Serialize)]
pub struct AlsoUsed {
    pub records: Vec<RecordUse>,
    /// Particle systems outside this hero's effects that use one of its files as a child.
    pub effects: Vec<EffectUse>,
    /// "Also used by: …", ready to show.
    pub note: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct RecordUse {
    #[serde(flatten)]
    pub user: User,
    pub text: String,
    /// The tree's files it names.
    pub parts: Vec<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct EffectUse {
    pub path: String,
    pub label: String,
    pub parts: Vec<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct PartInfo {
    pub path: String,
    pub label: String,
    pub parent: Option<String>,
    pub depth: usize,
    pub children: Vec<String>,
    /// The Effects catalog's association (`hero_directory`, `resource_reference`), or absent outside it.
    pub catalog: Option<String>,
    /// The catalog's `shared` flag (by folder; see `also_used_by` for real reuse).
    pub shared: bool,
    /// Outside the hero's own folders (`outside_hero`): an edit leaves it out
    /// unless the user lists it in `TreeEdit.shared`.
    pub outside_hero: bool,
    /// Other trees of this hero that hold this file.
    pub in_trees: Vec<String>,
    /// Renderer classes.
    pub renderers: Vec<String>,
    /// `additive`, `normal`, `mixed` or `none` (no color renderer).
    pub blend: &'static str,
    /// Whether it draws anything.
    pub visible: bool,
    /// Share of the tree's estimated visible color (0–1).
    pub weight: f64,
    pub strips: Vec<Strip>,
    pub may_not_follow: Vec<Note>,
    pub does_not_follow: Vec<Note>,
    pub multipliers: Vec<Site>,
    /// The smallest `m_flMaxSize` of its sprite renderers (a cap as a fraction
    /// of the screen): the part does not grow past it whatever the size
    /// multiplier ("size limited by the game"). Absent without a cap.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_limit: Option<f64>,
    /// What the part draws.
    pub kind: PartKind,
    /// Which of the part's own settings it takes, and why not.
    pub controls: ControlOffers,
    /// Where the part's own Size and Opacity would change it (`effect_edit::control_sites`).
    #[serde(skip)]
    pub own_sites: Vec<Site>,
    /// The part's light renderers and whether Light brightness changes them (`effect_edit::light_sites`).
    #[serde(skip)]
    pub light_sites: Vec<effect_edit::LightSite>,
}
/// What a part draws, named by its main renderer (the one that shows most:
/// sprites, ropes and trails, then models and decals, then lights).
#[derive(Debug, Clone, Serialize)]
pub struct PartKind {
    /// `line`, `sprites`, `trail`, `model`, `light`, `decal`, `other` or `none` (no renderer).
    pub kind: &'static str,
    /// "Line", "Glow sprites", "3D model", …; a mixed part lists its kinds, main first ("Glow sprites + Light").
    pub label: String,
    /// It draws more than one kind of thing.
    pub mixed: bool,
    /// One line about the part's kind (a light follows the effect's colors), when there is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}
/// One of the part's own settings: whether the part takes it.
#[derive(Debug, Clone, Serialize)]
pub struct Offer {
    pub offered: bool,
    /// Why not, in plain words (absent when offered).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub reason: String,
    /// Whether the moving preview (the effect player) shows this setting.
    pub player_shows: bool,
    /// When offered: what stays as it is (some of a part's lights), in plain words.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub note: String,
}
/// The part's own settings (`TreeEdit.parts`).
#[derive(Debug, Clone, Serialize)]
pub struct ControlOffers {
    pub hide: Offer,
    pub size: Offer,
    pub opacity: Offer,
    pub tint: Offer,
    /// Light brightness (`effect_edit::light_sites`).
    pub light: Offer,
}
#[derive(Debug, Clone, Serialize)]
pub struct Note {
    /// `inherited`, `control_point`, `other_input`, `other_field`, `other_function`,
    /// `status_effect`, `unchecked_texture`, `model_material`, `projected_material`, `colored_texture`.
    pub kind: &'static str,
    pub function: String,
    pub location: String,
    /// Plain words, without "may not" / "does not".
    pub reason: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct Site {
    pub multiplier: Multiplier,
    pub renderer: String,
    pub location: String,
    pub value: Option<f64>,
    /// True when a multiplier other than 1 changes or adds the setting.
    pub applies: bool,
    /// `changes` (an existing setting is multiplied), `adds` (the renderer
    /// lacks it and gets it, with the multiplier as its value), `no_setting`
    /// (this kind of part does not get the setting: model renderers) or
    /// `game_input` (the game sets the value while playing; left alone).
    pub status: &'static str,
    /// Why it does not apply (empty when it does).
    pub reason: String,
    #[serde(skip)]
    single: bool,
}
impl Site {
    fn from(root: &Value, s: Scaled) -> Self {
        let (status, reason) = site_status(&s);
        Site {
            single: effect_edit::single_precision(root, &s),
            multiplier: s.multiplier,
            renderer: s.renderer,
            location: s.location.to_string(),
            value: s.before,
            applies: s.left.is_empty(),
            status,
            reason,
        }
    }
}

// ---------------------------------------------------------------- preview types

/// "Pick a color": the color picked and, when the user chose another, the main color to map onto it.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PickRequest {
    pub color: [u8; 3],
    #[serde(default)]
    pub main: Option<[u8; 3]>,
}
#[derive(Debug, Clone, Serialize)]
pub struct EffectPreview {
    pub target: String,
    /// The edit as applied (after a pick, its hue, saturation and brightness are the pick's).
    pub edit: TreeEdit,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pick: Option<Pick>,
    pub parts: Vec<PartPreview>,
    pub palette: Vec<ColorChange>,
    pub main: Option<ColorChange>,
    pub changed: Changed,
}
#[derive(Debug, Clone, Serialize)]
pub struct ColorChange {
    pub before: [u8; 3],
    pub before_hex: String,
    pub after: [u8; 3],
    pub after_hex: String,
}
#[derive(Debug, Clone, Default, Serialize)]
pub struct Changed {
    /// Color stops (slots) whose color changes.
    pub colors: usize,
    /// Renderer values a multiplier changes.
    pub multiplied: usize,
    /// Renderer keys a multiplier other than 1 cannot change (absent or not a literal).
    pub not_multiplied: usize,
    /// Parts this edit hides (left-out parts are not counted).
    pub hidden: usize,
    /// Settings of parts that change or add something (a hidden part counts once).
    pub part_settings: usize,
}
/// What one of a part's own settings does in this edit.
#[derive(Debug, Clone, Serialize)]
pub struct ControlPreview {
    pub control: effect_edit::Control,
    pub renderer: String,
    pub location: String,
    pub before: Option<f64>,
    pub after: Option<f64>,
    /// The tint's color.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<[u8; 3]>,
    /// True when this edit changes or adds the value.
    pub applied: bool,
    /// `changes`, `adds`, `game_input` (the game sets it while playing; left
    /// alone), `not_offered` (the part does not take this setting) or
    /// `left_out` (the part is left out of the edit).
    pub status: &'static str,
    /// Why it is not applied (empty when it is).
    pub reason: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct PartPreview {
    pub path: String,
    /// True for every left-out part (by the user or as a shared part).
    pub excluded: bool,
    /// Why it is left out (nothing about it changes): `user` (in `excluded`),
    /// `shared` (outside the hero's folders and not chosen in `shared`), or absent.
    pub left_out: Option<colors::LeftOut>,
    pub strips: Vec<StripPreview>,
    pub multipliers: Vec<SitePreview>,
    /// This edit hides the part (its renderers are switched off).
    pub hidden: bool,
    /// The part's own settings in this edit (empty without any).
    pub controls: Vec<ControlPreview>,
}
#[derive(Debug, Clone, Serialize)]
pub struct StripPreview {
    pub id: String,
    pub kind: StripKind,
    pub role: String,
    pub stops: Vec<StopPreview>,
}
#[derive(Debug, Clone, Serialize)]
pub struct StopPreview {
    pub position: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alpha: Option<u8>,
    pub before: [u8; 3],
    pub before_hex: String,
    pub after: [u8; 3],
    pub after_hex: String,
    pub slot: Option<String>,
    pub rule: Rule,
}
#[derive(Debug, Clone, Serialize)]
pub struct SitePreview {
    pub multiplier: Multiplier,
    pub renderer: String,
    pub location: String,
    pub before: Option<f64>,
    pub after: Option<f64>,
    /// True when this edit changes the value.
    pub applied: bool,
    /// As `Site::status`.
    pub status: &'static str,
    /// Why a multiplier other than 1 cannot change it (empty otherwise).
    pub reason: String,
}

// ---------------------------------------------------------------- analysis of one file

fn class(v: &Value) -> &str {
    v.get("_class").and_then(|c| c.string().ok()).unwrap_or("")
}
fn list<'a>(root: &'a Value, key: &str) -> &'a [Value] {
    root.get(key).and_then(|v| v.array().ok()).unwrap_or(&[])
}
fn float(value: Option<&Value>) -> Option<f64> {
    let value = value?;
    match &value.kind {
        Kind::F32(_) | Kind::F64(_) | Kind::I64(_) | Kind::U64(_) => value.number().ok(),
        Kind::Object(_) => match value.get("m_nType").and_then(|v| v.string().ok()).unwrap_or("") {
            "PF_TYPE_LITERAL" => value.get("m_flLiteralValue").and_then(|v| v.number().ok()),
            "PF_TYPE_RANDOM_UNIFORM" | "PF_TYPE_RANDOM_BIASED" => {
                let min = value.get("m_flRandomMin").and_then(|v| v.number().ok()).unwrap_or(0.);
                let max = value.get("m_flRandomMax").and_then(|v| v.number().ok()).unwrap_or(1.);
                Some((min + max) / 2.)
            }
            _ => None,
        },
        _ => None,
    }
}
const COLOR_RENDERERS: [&str; 5] = ["C_OP_RenderSprites", "C_OP_RenderRopes", "C_OP_RenderTrails", "C_OP_RenderBlobs", "C_OP_RenderCables"];
fn draw_factor(renderer: &str) -> f64 {
    match renderer {
        r if COLOR_RENDERERS.contains(&r) => 1.,
        "C_OP_RenderModels" | "C_OP_RenderProjected" | "C_OP_RenderStatusEffectCitadel" => 0.5,
        "C_OP_RenderStandardLight" | "C_OP_RenderOmni2Light" | "C_OP_RenderDeferredLight" => 0.25,
        _ => 0.,
    }
}
struct Look {
    renderers: Vec<String>,
    blend: &'static str,
    additive: bool,
    visible: bool,
    weight: f64,
}
/// How much a part shows (see `effect_colors` for the rule).
fn look(root: &Value) -> Look {
    let renderers: Vec<String> = list(root, "m_Renderers").iter().map(|r| class(r).to_owned()).collect();
    let factor = renderers.iter().map(|r| draw_factor(r)).fold(0., f64::max);
    let mut count = 0.;
    let emitters = list(root, "m_Emitters");
    for e in emitters {
        count += match class(e) {
            "C_OP_InstantaneousEmitter" => float(e.get("m_nParticlesToEmit")).unwrap_or(1.),
            "C_OP_ContinuousEmitter" => {
                let rate = float(e.get("m_flEmitRate")).unwrap_or(100.);
                let duration = float(e.get("m_flEmissionDuration")).filter(|d| *d > 0.).unwrap_or(1.);
                rate * duration
            }
            "C_OP_MaintainEmitter" => float(e.get("m_nParticlesToMaintain")).unwrap_or(100.),
            "C_OP_NoiseEmitter" => 10.,
            _ => 1.,
        };
    }
    if emitters.is_empty() {
        count = 1.;
    }
    if let Some(max) = root.get("m_nMaxParticles").and_then(|v| v.integer().ok()).filter(|m| *m > 0) {
        count = count.min(max as f64);
    }
    let mut radius = float(root.get("m_flConstantRadius")).unwrap_or(5.);
    for op in list(root, "m_Initializers") {
        match class(op) {
            "C_INIT_RandomRadius" => {
                let min = float(op.get("m_flRadiusMin")).unwrap_or(1.);
                let max = float(op.get("m_flRadiusMax")).unwrap_or(1.);
                radius = (min + max) / 2.;
            }
            "C_INIT_InitFloat" if op.get("m_nOutputField").and_then(|v| v.integer().ok()).unwrap_or(3) == 3 => {
                if let Some(r) = float(op.get("m_InputValue")) {
                    radius = r;
                }
            }
            _ => {}
        }
    }
    let rows = list(root, "m_Renderers");
    let color: Vec<&Value> = rows.iter().filter(|r| COLOR_RENDERERS.contains(&class(r))).collect();
    let scale = color.iter().filter_map(|r| float(r.get("m_flRadiusScale"))).fold(None, |a: Option<f64>, b| Some(a.map_or(b, |a| a.max(b)))).unwrap_or(1.);
    let radius = (radius * scale).clamp(0.5, 100.);
    let overbright = rows.iter().filter_map(|r| float(r.get("m_flOverbrightFactor"))).fold(1., f64::max).clamp(1., 16.);
    let alpha = rows.iter().filter_map(|r| float(r.get("m_flAlphaScale"))).fold(None, |a: Option<f64>, b| Some(a.map_or(b, |a| a.max(b)))).unwrap_or(1.).clamp(0., 4.);
    let adds: Vec<bool> = color
        .iter()
        .map(|r| {
            let mode = r.get("m_nOutputBlendMode").and_then(|v| v.string().ok()).unwrap_or("");
            mode.contains("ADD") || mode.contains("LIGHTEN")
        })
        .collect();
    let blend = match (adds.iter().any(|a| *a), adds.iter().all(|a| *a)) {
        _ if adds.is_empty() => "none",
        (true, true) => "additive",
        (true, false) => "mixed",
        _ => "normal",
    };
    let weight = factor * count.max(0.).sqrt() * radius * overbright.sqrt() * alpha;
    Look { renderers, blend, additive: adds.iter().any(|a| *a), visible: factor > 0., weight }
}

/// A renderer texture's color: `Ok((saturation, mean))`, or why it was not checked.
type TextureStat = std::result::Result<(f64, [u8; 3]), &'static str>;
const TEXTURE_ELSEWHERE: &str = "is not in the game files, so its colors were not checked";
const TEXTURE_UNREAD: &str = "could not be decoded, so its colors were not checked";
/// A texture from the game archive or, for the engine's shared textures, its engine archive.
fn texture_stat(archive: &Vpk, engine: Option<&Vpk>, path: &str, cancel: &CancellationToken) -> TextureStat {
    let compiled = compiled(path).map_err(|_| TEXTURE_UNREAD)?;
    let key = casefold(&compiled);
    let source = if archive.entries.contains_key(&key) {
        archive
    } else {
        engine.filter(|e| e.entries.contains_key(&key)).ok_or(TEXTURE_ELSEWHERE)?
    };
    let raw = crate::read(source, &compiled).map_err(|_| TEXTURE_UNREAD)?;
    let image = crate::icons::texture_pixels(&raw, TEXTURE_SAMPLE, cancel).map_err(|_| TEXTURE_UNREAD)?;
    let (mut sum, mut total) = ([0f64; 3], 0f64);
    for p in image.pixels() {
        let w = f64::from(p[3]) / 255. * f64::from(p[0].max(p[1]).max(p[2])) / 255.;
        for k in 0..3 {
            sum[k] += w * f64::from(p[k]);
        }
        total += w;
    }
    if total <= 0. {
        return Ok((0., [0; 3]));
    }
    let mean = sum.map(|c| (c / total).round().clamp(0., 255.) as u8);
    Ok((colors::hsv(mean).1, mean))
}
/// Textures of the color renderers that show their own colors (not replaced
/// by a gradient, not alpha-only).
fn textures(root: &Value) -> Vec<(String, String, String)> {
    let mut out = vec![];
    for (i, r) in list(root, "m_Renderers").iter().enumerate() {
        if !COLOR_RENDERERS.contains(&class(r)) {
            continue;
        }
        for (j, t) in list(r, "m_vecTexturesInput").iter().enumerate() {
            if t.get("m_bReplaceTextureWithGradient").is_some_and(|v| v.kind == Kind::Bool(true)) {
                continue;
            }
            if t.get("m_nTextureChannels").and_then(|v| v.string().ok()) == Some("SPRITECARD_TEXTURE_CHANNEL_MIX_A") {
                continue;
            }
            if let Some(path) = t.get("m_hTexture").and_then(|v| v.string().ok()).filter(|p| !p.is_empty()) {
                out.push((format!("m_Renderers[{i}].m_vecTexturesInput[{j}]"), class(r).to_owned(), path.to_owned()));
            }
        }
    }
    out
}

struct Analysed {
    look: Look,
    strips: Vec<Strip>,
    may: Vec<Note>,
    does: Vec<Note>,
    sites: Vec<Site>,
    size_limit: Option<f64>,
    kind: PartKind,
    offers: ControlOffers,
    own_sites: Vec<Site>,
    light_sites: Vec<effect_edit::LightSite>,
}

/// A renderer class's kind of part and its label.
fn renderer_kind(class: &str) -> (&'static str, &'static str) {
    match class {
        "C_OP_RenderRopes" => ("line", "Line"),
        "C_OP_RenderSprites" => ("sprites", "Glow sprites"),
        "C_OP_RenderTrails" => ("trail", "Trail"),
        effect_edit::MODEL_RENDERER => ("model", "3D model"),
        "C_OP_RenderStandardLight" | "C_OP_RenderOmni2Light" | "C_OP_RenderDeferredLight" => ("light", "Light"),
        "C_OP_RenderProjected" => ("decal", "Decal"),
        _ => ("other", "Other"),
    }
}
/// A light: it takes the effect's colors (its brightness is the part's own
/// Light brightness when offered; else `light_note` adds why not).
pub const LIGHT_NOTE: &str = "A light on nearby surfaces. It takes the effect's colors.";
pub const PART_LIGHT_NOTE: &str = "Its light takes the effect's colors.";
pub const NO_OWN_COLOR_NOTE: &str = "This part has no color of its own in the game's files.";
/// A light part's note with why its brightness stays as it is, when it does.
fn light_note(base: &str, light: &Offer) -> String {
    if light.offered { base.to_owned() } else { format!("{base} Its brightness stays as it is: {}.", light.reason) }
}
/// "Some of its lights stay as they are": the reasons of the lights Light
/// brightness leaves, when others change (empty otherwise).
fn kept_lights(sites: &[effect_edit::LightSite]) -> String {
    let mut why: Vec<&str> = vec![];
    for r in sites.iter().filter_map(|s| s.refusal) {
        if !why.contains(&r) {
            why.push(r);
        }
    }
    if why.is_empty() || sites.iter().all(|s| s.refusal.is_some()) {
        return String::new();
    }
    let kept = sites.iter().filter(|s| s.refusal.is_some()).count();
    format!("{kept} of its {} lights stay{} as {}: {}", sites.len(), if kept == 1 { "s" } else { "" }, if kept == 1 { "it is" } else { "they are" }, why.join("; "))
}
/// What a part draws, named by its main renderer (`draw_factor`, the first of the strongest).
fn part_kind(root: &Value, tint: bool, light: &Offer) -> PartKind {
    let classes: Vec<&str> = list(root, "m_Renderers").iter().map(class).collect();
    let Some(main) = classes.iter().copied().fold(None, |best: Option<&str>, c| match best {
        Some(b) if draw_factor(b) >= draw_factor(c) => Some(b),
        _ => Some(c),
    }) else {
        return PartKind { kind: "none", label: "Draws nothing".into(), mixed: false, note: None };
    };
    let (kind, label) = renderer_kind(main);
    let mut labels = vec![label];
    for c in &classes {
        let other = renderer_kind(c).1;
        if !labels.contains(&other) {
            labels.push(other);
        }
    }
    let has_light = classes.iter().any(|c| renderer_kind(c).0 == "light");
    let note = if kind == "light" && labels.len() == 1 {
        Some(light_note(LIGHT_NOTE, light))
    } else if has_light {
        Some(light_note(PART_LIGHT_NOTE, light))
    } else if tint {
        Some(NO_OWN_COLOR_NOTE.to_owned())
    } else {
        None
    };
    PartKind { kind, label: labels.join(" + "), mixed: labels.len() > 1, note }
}
fn offer(refusal: Option<String>, player_shows: bool) -> Offer {
    Offer { offered: refusal.is_none(), reason: refusal.unwrap_or_default(), player_shows, note: String::new() }
}
/// Which of the part's own settings it takes (`effect_edit::apply_controls`'s rules).
fn offers(root: &Value, own: &[Site], lights: &[effect_edit::LightSite]) -> ControlOffers {
    let renderers = list(root, "m_Renderers");
    let hide = if renderers.is_empty() {
        Some("it draws nothing itself".to_owned())
    } else if renderers.iter().all(|r| r.get(effect_edit::DISABLE_KEY).is_some_and(|v| v.kind == Kind::Bool(true))) {
        Some("the game's file already switches it off".to_owned())
    } else {
        None
    };
    let scaled = |m: Multiplier, what: &str| {
        let sites: Vec<&Site> = own.iter().filter(|s| s.multiplier == m).collect();
        if sites.is_empty() {
            Some(format!("only 3D model parts have {what} of their own"))
        } else if sites.iter().any(|s| s.applies) {
            None
        } else {
            Some(format!("the game sets its {} while the effect plays", what.trim_start_matches("an ").trim_start_matches("a ")))
        }
    };
    ControlOffers {
        hide: offer(hide, true),
        size: offer(scaled(Multiplier::Size, "a size"), true),
        opacity: offer(scaled(Multiplier::Opacity, "an opacity"), false),
        tint: offer(effect_edit::tint_refusal(root).map(str::to_owned), false),
        // Whether the moving preview draws a light's brightness is not known: it says it does not show it.
        light: Offer { note: kept_lights(lights), ..offer(effect_edit::light_refusal(root).map(str::to_owned), false) },
    }
}

fn analyse(root: &Value, texture: &mut dyn FnMut(&str) -> TextureStat) -> Analysed {
    let look = look(root);
    let found = effect_edit::colors(root);
    let strips = colors::strips(root, &found.colors, look.weight, look.additive);
    let (mut may, mut does) = (vec![], vec![]);
    for f in &found.fixed {
        if f.kind == FixedKind::ModelMaterial {
            // A 3D model takes the particle color as its tint unless the tint
            // is switched off or the color goes through its material's settings.
            let colour = f.location.get(root).map(effect_edit::model_colour).unwrap_or_default();
            if colour.follows() {
                continue;
            }
            let reason = if colour.colour_vars { "a 3D model that takes the color through its material's settings" } else { "a 3D model drawn without the particle color (its tint is switched off)" };
            may.push(Note { kind: "model_material", function: f.function.clone(), location: f.location.to_string(), reason: reason.into() });
            continue;
        }
        let (kind, reason): (&'static str, String) = match f.kind {
            FixedKind::Inherited => ("inherited", "takes its color from its parent part's particles; whether it then shows the parent's new color is not proven".into()),
            FixedKind::ControlPoint => ("control_point", "the game sets this color while the effect plays (a control point)".into()),
            FixedKind::OtherInput => ("other_input", format!("the color comes from an input this editor does not change ({})", f.reason)),
            FixedKind::OtherField => ("other_field", "a color value written to another particle setting, not to its color".into()),
            FixedKind::NotColorArray | FixedKind::OtherFunction => ("other_function", f.reason.clone()),
            FixedKind::ModelMaterial => ("model_material", "a model drawn with its own materials".into()),
            FixedKind::ProjectedMaterial => ("projected_material", "a decal drawn with its material's texture".into()),
        };
        let note = Note { kind, function: f.function.clone(), location: f.location.to_string(), reason };
        if f.kind.certain() { does.push(note) } else { may.push(note) }
    }
    for (i, r) in list(root, "m_Renderers").iter().enumerate() {
        if class(r) == "C_OP_RenderStatusEffectCitadel" {
            may.push(Note { kind: "status_effect", function: class(r).into(), location: format!("m_Renderers[{i}]"), reason: "drawn with the game's status-effect shader".into() });
        }
    }
    for (location, renderer, path) in textures(root) {
        let name = path.rsplit('/').next().unwrap_or(&path).to_owned();
        match texture(&path) {
            Ok((s, mean)) if s > COLORED_TEXTURE => does.push(Note {
                kind: "colored_texture",
                function: renderer,
                location,
                reason: format!("the texture {name} has a color of its own ({}): the part's color changes, but that tint stays mixed in", colors::hex(mean)),
            }),
            Ok(_) => {}
            Err(why) => may.push(Note { kind: "unchecked_texture", function: renderer, location, reason: format!("the texture {name} {why}; if it is colored, that tint stays") }),
        }
    }
    let sites = effect_edit::multiplier_sites(root).into_iter().map(|s: Scaled| Site::from(root, s)).collect();
    let size_limit = list(root, "m_Renderers")
        .iter()
        .filter(|r| class(r) == "C_OP_RenderSprites")
        .filter_map(|r| float(r.get("m_flMaxSize")))
        .fold(None, |a: Option<f64>, b| Some(a.map_or(b, |a| a.min(b))));
    let own_sites: Vec<Site> = effect_edit::control_sites(root).into_iter().map(|s| Site::from(root, s)).collect();
    let light_sites = effect_edit::light_sites(root);
    let offers = offers(root, &own_sites, &light_sites);
    let kind = part_kind(root, offers.tint.offered, &offers.light);
    Analysed { look, strips, may, does, sites, size_limit, kind, offers, own_sites, light_sites }
}

/// A site's status (see `Site::status`) and its reason in plain words.
fn site_status(s: &Scaled) -> (&'static str, String) {
    if s.added {
        ("adds", format!("{} is added (it was not set, which means 1)", s.multiplier.key()))
    } else if s.left.is_empty() {
        ("changes", String::new())
    } else if s.left.starts_with("absent") {
        let what = match s.multiplier {
            Multiplier::Glow => "brightness",
            Multiplier::Size => "size",
            Multiplier::Opacity => "opacity",
        };
        ("no_setting", format!("this kind of part (a model) has no {what} setting to change"))
    } else {
        ("game_input", format!("the game sets {} while the effect plays ({})", s.multiplier.key(), s.left))
    }
}

// ---------------------------------------------------------------- names

/// Effects of one group that share a label get, in brackets, the words of their file names
/// that differ (or their folder, when the file names do not tell them apart). Display only:
/// groups, order and everything else of a tree stay as they are.
fn tell_apart_trees(trees: &mut [TreeInfo], hero: &[Vec<String>]) {
    let mut same: BTreeMap<(String, String), Vec<usize>> = BTreeMap::new();
    for (index, tree) in trees.iter().enumerate() {
        same.entry((tree.group.clone(), casefold(&tree.label))).or_default().push(index);
    }
    for indices in same.into_values().filter(|i| i.len() > 1) {
        let targets: Vec<&str> = indices.iter().map(|&i| trees[i].target.as_str()).collect();
        let Some(extra) = telling_words(&targets, hero) else { continue };
        for (&i, words) in indices.iter().zip(extra).filter(|(_, words)| !words.is_empty()) {
            let tree = &mut trees[i];
            let old = tree.label.clone();
            tree.label = format!("{old} ({words})");
            tree.name = if tree.name == old { tree.label.clone() } else { format!("{} · {}", tree.group_name, lower_first(&tree.label)) };
        }
    }
}

/// What tells effects with one label apart: the file-name words not every one of them has,
/// else the part of their folders that differs; None when neither does.
fn telling_words(targets: &[&str], hero: &[Vec<String>]) -> Option<Vec<String>> {
    let words: Vec<Vec<String>> = targets.iter().map(|t| particle_words(t, hero)).collect();
    let common: BTreeSet<&String> = words[0].iter().filter(|w| words.iter().all(|other| other.contains(w))).collect();
    let rest: Vec<String> = words.iter().map(|w| w.iter().filter(|x| !common.contains(x)).cloned().collect::<Vec<_>>().join(" ")).collect();
    // At most one of them may have no differing word: it keeps its label as it is.
    let distinct = |names: &[String]| names.iter().collect::<BTreeSet<_>>().len() == names.len();
    if distinct(&rest) {
        return Some(rest);
    }
    let folders = crate::icons::differing_folders(targets);
    distinct(&folders).then_some(folders)
}

fn particle_words(path: &str, hero: &[Vec<String>]) -> Vec<String> {
    let stem = casefold(path.rsplit('/').next().unwrap_or(path)).trim_end_matches(".vpcf_c").trim_end_matches(".vpcf").to_owned();
    let words: Vec<String> = sound_tags::words(&stem)
        .into_iter()
        .map(|w| match w.as_str() {
            "tgt" => "target".into(),
            "dmg" => "damage".into(),
            _ => w,
        })
        .collect();
    sound_tags::strip_leading(words, hero, &[])
}
fn record_words(record: &str, hero: &[Vec<String>]) -> Vec<String> {
    sound_tags::strip_leading(sound_tags::words(record), hero, &["citadel", "ability", "upgrade", "item", "weapon"])
}
/// `words` without the longest of `prefixes` it starts with (and is longer than).
fn without_prefix(words: &[String], prefixes: &[Vec<String>]) -> Option<Vec<String>> {
    prefixes
        .iter()
        .filter(|p| !p.is_empty() && words.len() >= p.len() && words.starts_with(p))
        .max_by_key(|p| p.len())
        .map(|p| words[p.len()..].to_vec())
}
fn lower_first(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(c) if text.chars().nth(1).is_none_or(|n| !n.is_uppercase()) => c.to_lowercase().collect::<String>() + chars.as_str(),
        Some(c) => c.to_string() + chars.as_str(),
        None => String::new(),
    }
}

// ---------------------------------------------------------------- the trees

struct TreePart {
    path: String,
    parent: Option<String>,
    depth: usize,
    children: Vec<String>,
}
struct BuiltTree {
    top: String,
    parts: Vec<TreePart>,
    missing: Vec<String>,
}

/// The trees of a catalog (casefolded target → row) and the raw files read
/// (casefolded path → bytes, `None` when the game does not have it).
#[allow(clippy::type_complexity)]
fn built_trees(archive: &Vpk, catalog: &BTreeMap<String, &CatalogRow>, cancel: &CancellationToken, warnings: &mut Vec<String>) -> Result<(Vec<BuiltTree>, BTreeMap<String, Option<Vec<u8>>>)> {    // Raw files and documents, each read once.
    let mut raws: BTreeMap<String, Option<Vec<u8>>> = BTreeMap::new();
    let mut read = |path: &str| -> Result<Option<Vec<u8>>> {
        let key = casefold(path);
        if let Some(raw) = raws.get(&key) {
            return Ok(raw.clone());
        }
        let raw = if archive.entries.contains_key(&key) { Some(crate::read(archive, path)?) } else { None };
        raws.insert(key, raw.clone());
        Ok(raw)
    };
    let mut children: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (key, row) in catalog.iter() {
        cancel.check()?;
        let Some(raw) = read(&row.target)? else { continue };
        match effect_edit::document(&raw).and_then(|d| effect_edit::children(&d.root)) {
            Ok(list) => {
                children.insert(key.clone(), list.iter().map(|c| casefold(c)).collect());
            }
            Err(e) => warnings.push(format!("{} could not be read: {e}", row.target)),
        }
    }
    let named: BTreeSet<&String> = children.values().flatten().collect();
    let mut tops: Vec<String> = children.keys().filter(|k| !named.contains(k)).map(|k| catalog[k].target.clone()).collect();
    let mut built: Vec<BuiltTree> = vec![];
    let mut reached: BTreeSet<String> = BTreeSet::new();
    let mut next = 0;
    loop {
        while next < tops.len() {
            cancel.check()?;
            let top = tops[next].clone();
            next += 1;
            let tree = effect_edit::read_tree(&top, &mut read)?;
            let mut parent: BTreeMap<String, (String, usize)> = BTreeMap::new();
            let mut parts = vec![];
            for file in &tree.files {
                let key = casefold(&file.path);
                reached.insert(key.clone());
                let (p, depth) = match parent.get(&key) {
                    Some((p, d)) => (Some(p.clone()), d + 1),
                    None => (None, 0),
                };
                for c in &file.children {
                    parent.entry(casefold(c)).or_insert((file.path.clone(), depth));
                }
                parts.push(TreePart { path: file.path.clone(), parent: p, depth, children: file.children.clone() });
            }
            built.push(BuiltTree { top: tree.top, parts, missing: tree.missing });
        }
        // Files only a child cycle reaches: the first unreached one becomes a top.
        match children.keys().find(|k| !reached.contains(*k)) {
            Some(k) => tops.push(catalog[k].target.clone()),
            None => break,
        }
    }
    drop(read);
    Ok((built, raws))
}

/// The rule for parts an edit leaves out unless the user chooses them: a part
/// outside the hero's own folders, that is not in the hero's Effects catalog
/// or in it as a linked (`shared`, association `resource_reference`) file.
pub fn outside_hero(row: Option<&CatalogRow>) -> bool {
    row.is_none_or(|r| r.association.shared)
}
/// One tree's files, for the build: the top and every part in tree order.
#[derive(Debug, Clone)]
pub struct TreeFiles {
    pub top: String,
    pub parts: Vec<String>,
}
/// The hero's effect trees from its Effects catalog (`resources::discover`), with the raw
/// game files they hold (casefolded path → bytes). Builds use this, not `HeroEffects::build`:
/// no definitions, names or colors are read.
pub fn tree_files(archive: &Vpk, rows: &[CatalogRow], cancel: &CancellationToken) -> Result<(Vec<TreeFiles>, BTreeMap<String, Vec<u8>>)> {
    let catalog: BTreeMap<String, &CatalogRow> = rows.iter().map(|r| (casefold(&r.target), r)).collect();
    let (built, raws) = built_trees(archive, &catalog, cancel, &mut vec![])?;
    let trees = built.into_iter().map(|t| TreeFiles { top: t.top, parts: t.parts.into_iter().map(|p| p.path).collect() }).collect();
    Ok((trees, raws.into_iter().filter_map(|(k, v)| Some((k, v?))).collect()))
}

/// A hero's effects, decoded once.
#[derive(Debug, Clone)]
pub struct HeroEffects {
    pub trees: EffectTrees,
    index: BTreeMap<String, usize>,
}

impl HeroEffects {
    /// The trees of `rows` (the hero's Effects catalog from `resources::discover`).
    pub fn build(archive: &Vpk, model: &str, rows: &[CatalogRow], game: &GameIndex, cancel: &CancellationToken) -> Result<Self> {
        let mut warnings = vec![];
        let tokens = heroes::tokens(model)?;
        let hero_words = sound_tags::token_words(&tokens);
        let hero_name = heroes::display_name(model).unwrap_or_else(|| effect_catalog::readable_internal(&heroes::strip_version(casefold(model).rsplit('/').next().unwrap_or_default().trim_end_matches(".vmdl_c").trim_end_matches(".vmdl")), &[]));
        let catalog: BTreeMap<String, &CatalogRow> = rows.iter().map(|r| (casefold(&r.target), r)).collect();
        let (built, raws) = built_trees(archive, &catalog, cancel, &mut warnings)?;
        // Per-file analysis (textures also from the engine's own archive).
        let engine = crate::engine_archive(&archive.path);
        let mut stats: BTreeMap<String, TextureStat> = BTreeMap::new();
        let mut analysed: BTreeMap<String, Analysed> = BTreeMap::new();
        // A part's low-violence substitute (`m_hLowViolenceDef`, casefolded compiled) → the part.
        let mut low_violence: BTreeMap<String, String> = BTreeMap::new();
        for tree in &built {
            for part in &tree.parts {
                let key = casefold(&part.path);
                if analysed.contains_key(&key) {
                    continue;
                }
                cancel.check()?;
                let Some(Some(raw)) = raws.get(&key) else { continue };
                let root = effect_edit::document(raw)?.root;
                if let Some(lv) = root.get(LOW_VIOLENCE).and_then(|v| v.string().ok()).and_then(|p| compiled(p).ok()) {
                    low_violence.entry(casefold(&lv)).or_insert_with(|| key.clone());
                }
                let mut texture = |path: &str| stats.entry(casefold(path)).or_insert_with(|| texture_stat(archive, engine.as_ref(), path, cancel)).clone();
                analysed.insert(key, analyse(&root, &mut texture));
            }
        }
        // Which trees hold each file.
        let mut holders: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for tree in &built {
            for part in &tree.parts {
                holders.entry(casefold(&part.path)).or_default().push(tree.top.clone());
            }
        }
        let usage = match &game.definitions {
            Some(defs) => Usage::new(defs, model, &game.names)?,
            None => {
                warnings.push("The game's ability definitions could not be read, so effects are not grouped by ability.".into());
                Usage::default()
            }
        };
        if game.names.is_empty() {
            warnings.push("The game's English names were not found beside the game files; internal names are shown.".into());
        }
        // Groups from the definitions, then the name and folder fallbacks.
        let naming = |file: &str| usage.own_files.get(file).into_iter().flatten().filter_map(|r| usage.own.get(r).map(|(slot, root)| (effect_catalog::slot_order(slot), slot.clone(), root.clone(), r.clone())));
        let by_definition: Vec<(Vec<String>, String, Option<Choice>)> = built
            .iter()
            .map(|tree| {
                let top = casefold(&tree.top);
                let choice = naming(&top).min().or_else(|| tree.parts.iter().flat_map(|p| naming(&casefold(&p.path))).min());
                (particle_words(&tree.top, &hero_words), top, choice)
            })
            .collect();
        let mut roots: BTreeMap<String, (u32, String, Vec<Vec<String>>)> = BTreeMap::new();
        for (slot, root) in usage.own.values() {
            let order = effect_catalog::slot_order(slot);
            let words = if slot.starts_with("ESlot_Signature") || slot.starts_with("ESlot_Ability_Innate") {
                let mut w = vec![record_words(root, &hero_words)];
                if let Some(name) = game.names.get(root) {
                    w.push(sound_tags::words(name));
                }
                w
            } else {
                vec![]
            };
            let entry = roots.entry(root.clone()).or_insert((order, slot.clone(), words));
            if order < entry.0 {
                entry.0 = order;
                entry.1 = slot.clone();
            }
        }
        let assigned = fallback_groups(&by_definition, &roots);
        let undecided = assigned.iter().any(|(_, by, _)| *by != "definition");
        // The hero's models: this one and the roster's other models of the same hero.
        let own_models = hero_models(model);
        // Real references the definitions do not hold: a model an own ability names (a
        // summon, a turret) names the file (`grouped_by: reference`).
        let mut by_model: BTreeMap<String, Choice> = BTreeMap::new();
        if undecided {
            for (named, records) in &usage.own_models {
                if own_models.contains(named) {
                    continue;
                }
                let Some(choice) = records.iter().filter_map(|r| usage.own.get(r).map(|(slot, root)| (effect_catalog::slot_order(slot), slot.clone(), root.clone(), r.clone()))).min() else { continue };
                for file in crate::resources::model_particles(archive, named, cancel).unwrap_or_default() {
                    by_model.entry(file).and_modify(|c| *c = c.clone().min(choice.clone())).or_insert_with(|| choice.clone());
                }
            }
        }
        let assigned: Vec<(Option<Choice>, &'static str, Vec<String>)> = built
            .iter()
            .zip(assigned)
            .map(|(tree, (choice, grouped_by, stem))| {
                // A file outside the hero's folders (an item's effect a summon also carries) is not moved.
                if grouped_by == "definition" || outside_hero(catalog.get(&casefold(&tree.top)).copied()) {
                    return (choice, grouped_by, stem);
                }
                match tree.parts.iter().filter_map(|p| by_model.get(&casefold(&p.path))).min() {
                    Some(c) if choice.as_ref().is_some_and(|old| old.2 == c.2) => (Some(c.clone()), "reference", stem),
                    Some(c) => (Some(c.clone()), "reference", vec![]),
                    None => (choice, grouped_by, stem),
                }
            })
            .collect();
        // Trees still without a group: what else in the game names them.
        let hero_root = usage.own.values().find(|(slot, _)| slot == "hero").map(|(_, root)| root.clone());
        let model_refs = if assigned.iter().any(|(c, _, _)| c.is_none()) {
            let mut found = BTreeSet::new();
            for (i, m) in own_models.iter().enumerate() {
                match crate::resources::model_particles(archive, m, cancel) {
                    Ok(files) => found.extend(files),
                    Err(e) if i == 0 => warnings.push(format!("The hero's model could not be read for the effects it names: {e}")),
                    Err(_) => {}
                }
            }
            found
        } else {
            BTreeSet::new()
        };
        let named_elsewhere = |file: &str| {
            usage.users.get(file).is_some_and(|records| records.iter().any(|r| usage.user.contains_key(r))) || game.children.parents(file).iter().any(|p| !catalog.contains_key(&casefold(p)))
        };
        // Named by a definition the game no longer uses (an ability no hero has, a test):
        // a leftover, which no file-name rule moves.
        let named_by_leftover = |file: &str| usage.users.get(file).is_some_and(|records| records.iter().any(|r| usage.left.contains_key(r)));
        // Own bound abilities (not weapons or melee) by their internal name, for `name_match`.
        let abilities: Vec<(String, Vec<String>)> = roots
            .iter()
            .filter(|(_, (_, slot, _))| slot.starts_with("ESlot_Signature") || slot.starts_with("ESlot_Ability_Innate"))
            .map(|(root, _)| (root.clone(), record_words(root, &hero_words)))
            .collect();
        // The same abilities by the words of their display name, for `display_match`.
        let display_names: Vec<(String, Vec<String>)> = abilities.iter().filter_map(|(root, _)| game.names.get(root).map(|name| (root.clone(), display_words(name)))).collect();
        // The fourth signature ability, for files named "ult".
        let ult: Option<String> = roots.iter().find(|(_, (_, slot, _))| slot == "ESlot_Signature_4").map(|(root, _)| root.clone());
        let mut candidates: Vec<Vec<String>> = vec![vec![]; built.len()];
        // Which rule placed each tree (`TreeInfo::rule`).
        let mut rules: Vec<&'static str> = assigned
            .iter()
            .map(|(_, by, _)| match *by {
                "reference" => "summon_model",
                "name" => "stem",
                other => other,
            })
            .collect();
        let mut assigned: Vec<(Option<Choice>, &'static str, Vec<String>, Rest)> = built
            .iter()
            .zip(assigned)
            .zip(candidates.iter_mut().zip(rules.iter_mut()))
            .map(|((tree, (choice, grouped_by, stem)), (candidates, rule))| {
                if choice.is_some() {
                    return (choice, grouped_by, stem, Rest::Linked);
                }
                *rule = "none";
                let files: Vec<String> = tree.parts.iter().map(|p| casefold(&p.path)).collect();
                match (rest_of(&files, &model_refs, &named_elsewhere), &hero_root) {
                    (Rest::Model, Some(root)) => {
                        *rule = "model";
                        (Some((HERO_ORDER, "hero".into(), root.clone(), root.clone())), "model", vec![], Rest::Model)
                    }
                    (Rest::Model, None) => (None, "none", vec![], Rest::Unlinked),
                    (Rest::Unlinked, _) if !files.iter().any(|f| named_by_leftover(f)) => {
                        // Only folded trees move by their file name: an own ability's internal
                        // name, else a hero-level name.
                        let words = particle_words(&tree.top, &hero_words);
                        match name_match(&words, &abilities) {
                            NameMatch::One(root) => {
                                let (order, slot, _) = &roots[&root];
                                *rule = "internal_name";
                                return (Some((*order, slot.clone(), root.clone(), root)), "name", vec![], Rest::Linked);
                            }
                            NameMatch::Several(list) => *candidates = list,
                            NameMatch::None => {}
                        }
                        // Then, in this order: the alias list, a word of an ability's display
                        // name, and "ult" for the fourth ability. A head or showcase effect
                        // stays the hero's whatever else its name holds (a showcase dagger is
                        // not the dagger ability's).
                        let hero_only = words.first().is_some_and(|w| w == "head" || w == "showcase");
                        let by_name = alias_match(&words, &tokens, &abilities)
                            .map(|root| (root, "alias"))
                            .or_else(|| match display_match(&words, &display_names) {
                                NameMatch::One(root) if !hero_only => Some((root, "display_name")),
                                _ => None,
                            })
                            .or_else(|| ult.clone().filter(|_| !hero_only && names_ult(&words)).map(|root| (root, "ult")));
                        if let Some((root, how)) = by_name {
                            let (order, slot, _) = &roots[&root];
                            *rule = how;
                            return (Some((*order, slot.clone(), root.clone(), root)), "name", vec![], Rest::Linked);
                        }
                        match &hero_root {
                            Some(root) if hero_level(&words) => {
                                *rule = "hero_word";
                                (Some((HERO_ORDER, "hero".into(), root.clone(), root.clone())), "name", vec![], Rest::Linked)
                            }
                            _ => (None, "none", vec![], Rest::Unlinked),
                        }
                    }
                    (rest, _) => (None, "none", vec![], rest),
                }
            })
            .collect();
        // A low-violence substitute goes where the part that names it went (`reference`).
        let tree_of: BTreeMap<String, usize> = built.iter().enumerate().flat_map(|(i, t)| t.parts.iter().map(move |p| (casefold(&p.path), i))).rev().collect();
        for i in 0..built.len() {
            if matches!(assigned[i].1, "definition" | "reference") {
                continue;
            }
            let Some(j) = low_violence.get(&casefold(&built[i].top)).and_then(|part| tree_of.get(part)).copied().filter(|j| *j != i) else { continue };
            if let (Some(choice), "definition" | "reference" | "name" | "folder" | "model") = (assigned[j].0.clone(), assigned[j].1) {
                // A tree its name already put there keeps the stem its label is made from.
                let stem = if assigned[i].0.as_ref().is_some_and(|c| c.2 == choice.2) { std::mem::take(&mut assigned[i].2) } else { vec![] };
                assigned[i] = (Some(choice), "reference", stem, Rest::Linked);
                rules[i] = "low_violence";
            }
        }
        let mut groups: BTreeMap<String, Group> = BTreeMap::new();
        let mut trees = vec![];
        let mut counts = Counts::default();
        for (((tree, (choice, grouped_by, stem, rest)), name_candidates), rule) in built.iter().zip(assigned).zip(candidates).zip(rules) {
            let top = casefold(&tree.top);
            let files: Vec<String> = tree.parts.iter().map(|p| casefold(&p.path)).collect();
            let mut records: BTreeSet<String> = BTreeSet::new();
            for f in &files {
                records.extend(usage.own_files.get(f).into_iter().flatten().cloned());
            }
            let group = match &choice {
                Some((order, slot, root, _)) => {
                    let kind: &'static str = match slot.as_str() {
                        s if s.starts_with("ESlot_Signature") => "ability",
                        "ESlot_Weapon_Primary" | "ESlot_Weapon_Secondary" => "weapon",
                        "ESlot_Weapon_Melee" => "melee",
                        "hero" => "hero",
                        s if s.starts_with("ESlot_Ability_Innate") => "innate",
                        _ => "ability",
                    };
                    let own_name = game.names.get(root).map(str::to_owned);
                    let name = match kind {
                        "weapon" if slot == "ESlot_Weapon_Secondary" => "Second weapon".to_owned(),
                        "weapon" => "Weapon".to_owned(),
                        "melee" => "Melee".to_owned(),
                        "hero" => hero_name.clone(),
                        _ => own_name.clone().unwrap_or_else(|| effect_catalog::readable_internal(root, &hero_words)),
                    };
                    groups.entry(format!("{kind}:{root}")).or_insert_with(|| Group {
                        key: format!("{kind}:{root}"),
                        name,
                        order: *order,
                        kind,
                        note: None,
                        folded: false,
                        record: Some(root.clone()),
                        record_name: own_name.filter(|_| matches!(kind, "weapon" | "melee")),
                        slot: Some(slot.clone()),
                        trees: 0,
                    })
                }
                None if rest == Rest::Elsewhere => groups.entry(ELSEWHERE_GROUP.into()).or_insert_with(|| Group {
                    key: ELSEWHERE_GROUP.into(),
                    name: "Used elsewhere in the game".into(),
                    order: ELSEWHERE_ORDER,
                    kind: "elsewhere",
                    note: Some(elsewhere_note(&hero_name)),
                    folded: false,
                    record: None,
                    record_name: None,
                    slot: None,
                    trees: 0,
                }),
                None => groups.entry(UNLINKED_GROUP.into()).or_insert_with(|| Group {
                    key: UNLINKED_GROUP.into(),
                    name: format!("Other files in {hero_name}'s folders"),
                    order: UNLINKED_ORDER,
                    kind: "unlinked",
                    // Folded and explained only when the definitions were read (else nothing is
                    // known about these files, and the warnings say why).
                    note: game.definitions.is_some().then(unlinked_note),
                    folded: game.definitions.is_some(),
                    record: None,
                    record_name: None,
                    slot: None,
                    trees: 0,
                }),
            };
            group.trees += 1;
            let (group_key, group_name, group_kind, group_order) = (group.key.clone(), group.name.clone(), group.kind, group.order);
            // Groups that are not an ability: names stay the effect's own.
            let ungrouped = matches!(group_kind, "unlinked" | "elsewhere");
            // Names.
            let prefixes: Vec<Vec<String>> = match &choice {
                Some((_, _, root, record)) => {
                    let mut p = vec![record_words(root, &hero_words), record_words(record, &hero_words)];
                    if stem.len() >= 2 {
                        p.push(stem.clone());
                    }
                    if group_kind == "ability" || group_kind == "innate" {
                        p.push(sound_tags::words(&group_name));
                    }
                    p
                }
                None => vec![],
            };
            let top_words = particle_words(&tree.top, &hero_words);
            let label = match without_prefix(&top_words, &prefixes) {
                Some(rest) if rest.is_empty() && !ungrouped => group_name.clone(),
                Some(rest) if !rest.is_empty() => sound_tags::readable(&rest),
                _ => sound_tags::readable(&top_words),
            };
            let name = if ungrouped || label == group_name { label.clone() } else { format!("{group_name} · {}", lower_first(&label)) };
            // Tags.
            let row = catalog.get(&top);
            let mut tags = vec![];
            if row.is_some_and(|r| !r.association.shared) {
                tags.push("hero");
            }
            if matches!(group_kind, "ability" | "innate") || top.contains("/abilities/") {
                tags.push("ability");
            }
            if matches!(group_kind, "weapon" | "melee") || top.contains("/weapon_fx/") {
                tags.push("weapon");
            }
            // Parts, palette, main color.
            let mut parts = vec![];
            let mut follow = FollowCounts::default();
            let mut outside_parts = 0;
            let mut multipliers: BTreeMap<Multiplier, SiteCounts> = Multiplier::ALL.iter().map(|m| (*m, SiteCounts::default())).collect();
            for part in &tree.parts {
                let key = casefold(&part.path);
                let Some(a) = analysed.get(&key) else { continue };
                let words = particle_words(&part.path, &hero_words);
                let label = if key == top {
                    label.clone()
                } else {
                    match without_prefix(&words, std::slice::from_ref(&top_words)).or_else(|| without_prefix(&words, &prefixes)) {
                        Some(rest) if !rest.is_empty() => sound_tags::readable(&rest),
                        _ => sound_tags::readable(&words),
                    }
                };
                follow.may_not += a.may.len();
                follow.does_not += a.does.len();
                let row = catalog.get(&key);
                let outside = outside_hero(row.copied());
                outside_parts += usize::from(outside);
                // The slider counts describe the default edit, which leaves shared parts out.
                for m in Multiplier::ALL.into_iter().filter(|_| !outside) {
                    let c = multipliers.entry(m).or_default();
                    c.visible_parts += usize::from(a.look.visible);
                    c.parts_reached += usize::from(a.sites.iter().any(|s| s.multiplier == m && s.applies));
                }
                for s in a.sites.iter().filter(|_| !outside) {
                    let c = multipliers.entry(s.multiplier).or_default();
                    match s.status {
                        "changes" => c.changes += 1,
                        "adds" => c.adds += 1,
                        "no_setting" => c.no_setting += 1,
                        _ => c.game_input += 1,
                    }
                }
                parts.push(PartInfo {
                    path: part.path.clone(),
                    label,
                    parent: part.parent.clone(),
                    depth: part.depth,
                    children: part.children.clone(),
                    catalog: row.map(|r| r.association.association.clone()),
                    shared: row.is_some_and(|r| r.association.shared),
                    outside_hero: outside,
                    in_trees: holders.get(&key).into_iter().flatten().filter(|t| casefold(t) != top).cloned().collect(),
                    renderers: a.look.renderers.clone(),
                    blend: a.look.blend,
                    visible: a.look.visible,
                    weight: 0.,
                    strips: a.strips.clone(),
                    may_not_follow: a.may.clone(),
                    does_not_follow: a.does.clone(),
                    multipliers: a.sites.clone(),
                    size_limit: a.size_limit,
                    kind: a.kind.clone(),
                    controls: a.offers.clone(),
                    own_sites: a.own_sites.clone(),
                    light_sites: a.light_sites.clone(),
                });
            }
            let uses: Vec<colors::Use> = parts
                .iter()
                .flat_map(|p| p.strips.iter().flat_map(move |s| s.stops.iter().filter(|t| t.slot.is_some()).map(move |t| colors::Use { rgb: t.rgb, weight: t.weight, part: &p.path })))
                .collect();
            let total: f64 = uses.iter().map(|u| u.weight).sum();
            let palette = colors::palette(&uses);
            let main = colors::main_color(&palette);
            for p in &mut parts {
                let own: f64 = p.strips.iter().flat_map(|s| &s.stops).filter(|t| t.slot.is_some()).map(|t| t.weight).sum();
                p.weight = round(if total > 0. { own / total } else { 0. });
                for s in &mut p.strips {
                    for t in &mut s.stops {
                        t.weight = round(if total > 0. { t.weight / total } else { 0. });
                    }
                }
            }
            let palette: Vec<Swatch> = palette.into_iter().map(|mut s| { s.weight = round(s.weight); s }).collect();
            let main = main.map(|mut m| { m.share = round(m.share); m });
            // Also used by.
            let mut users: BTreeMap<String, (User, BTreeSet<String>)> = BTreeMap::new();
            let mut left: BTreeSet<LeftOut> = BTreeSet::new();
            let mut effects: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
            for (f, part) in files.iter().zip(&tree.parts) {
                for record in usage.users.get(f).into_iter().flatten() {
                    if let Some(user) = usage.user.get(record) {
                        users.entry(record.clone()).or_insert_with(|| (user.clone(), BTreeSet::new())).1.insert(part.path.clone());
                    } else if let Some(reason) = usage.left.get(record) {
                        left.insert(LeftOut { record: record.clone(), reason: *reason });
                    }
                }
                for parent in game.children.parents(f) {
                    if !catalog.contains_key(&casefold(parent)) {
                        effects.entry(parent.clone()).or_default().insert(part.path.clone());
                    }
                }
            }
            let mut records_used: Vec<RecordUse> = users
                .into_values()
                .map(|(user, parts)| RecordUse { text: user.text(), user, parts: parts.into_iter().collect() })
                .collect();
            records_used.sort_by(|a, b| (a.user.kind.clone(), &a.text).cmp(&(b.user.kind.clone(), &b.text)));
            let effects: Vec<EffectUse> = effects
                .into_iter()
                .map(|(path, parts)| EffectUse { label: sound_tags::readable(&particle_words(&path, &[])), path, parts: parts.into_iter().collect() })
                .collect();
            let also_used_by = (!records_used.is_empty() || !effects.is_empty()).then(|| {
                let mut names: Vec<String> = records_used.iter().map(|r| r.text.clone()).collect();
                names.extend(effects.iter().map(|e| format!("{} (effect)", e.label)));
                let shown = names.len().min(NOTE_NAMES);
                let mut note = format!("Also used by: {}", names[..shown].join(", "));
                if names.len() > shown {
                    note += &format!(" and {} more", names.len() - shown);
                }
                AlsoUsed { records: records_used, effects, note }
            });
            let shared_with_trees: BTreeSet<String> = files.iter().flat_map(|f| holders.get(f).into_iter().flatten()).filter(|t| casefold(t) != top).cloned().collect();
            counts.trees += 1;
            counts.parts += parts.len();
            if ungrouped { counts.unlinked += 1 } else { counts.linked += 1 }
            counts.with_note += usize::from(also_used_by.is_some());
            counts.may_not_follow += follow.may_not;
            counts.does_not_follow += follow.does_not;
            counts.left_out += left.len();
            trees.push((
                group_order,
                TreeInfo {
                    target: tree.top.clone(),
                    label,
                    name,
                    group: group_key,
                    group_name,
                    grouped_by,
                    tags,
                    records: records.into_iter().collect(),
                    parts,
                    missing: tree.missing.clone(),
                    palette,
                    main,
                    also_used_by,
                    left_out: left.into_iter().collect(),
                    shared_with_trees: shared_with_trees.into_iter().collect(),
                    follow,
                    outside_parts,
                    multipliers,
                    name_candidates,
                    rule,
                },
            ));
        }
        trees.sort_by(|a, b| (a.0, &a.1.group, &a.1.label, &a.1.target).cmp(&(b.0, &b.1.group, &b.1.label, &b.1.target)));
        let mut trees: Vec<TreeInfo> = trees.into_iter().map(|(_, t)| t).collect();
        tell_apart_trees(&mut trees, &hero_words);
        counts.files = holders.len();
        counts.files_in_several_trees = holders.values().filter(|t| t.len() > 1).count();
        // Bound signature abilities without a tree: an empty group that says why.
        for (root, (order, slot, _)) in &roots {
            if !slot.starts_with("ESlot_Signature") || groups.values().any(|g| g.record.as_deref() == Some(root)) {
                continue;
            }
            let key = format!("ability:{root}");
            groups.insert(
                key.clone(),
                Group {
                    key,
                    name: game.names.get(root).map(str::to_owned).unwrap_or_else(|| effect_catalog::readable_internal(root, &hero_words)),
                    order: *order,
                    kind: "ability",
                    note: Some(empty_ability_note(&hero_name)),
                    folded: false,
                    record: Some(root.clone()),
                    record_name: None,
                    slot: Some(slot.clone()),
                    trees: 0,
                },
            );
        }
        let mut groups: Vec<Group> = groups.into_values().collect();
        groups.sort_by(|a, b| (a.order, &a.name).cmp(&(b.order, &b.name)));
        let index = trees.iter().enumerate().map(|(i, t)| (casefold(&t.target), i)).collect();
        Ok(Self {
            trees: EffectTrees {
                schema: EFFECT_TREES_SCHEMA,
                policy: effect_edit::EFFECT_EDIT_POLICY,
                hero: model.to_owned(),
                hero_name,
                sources: Sources { definitions: game.definitions.is_some(), names: !game.names.is_empty() },
                groups,
                trees,
                counts,
                warnings,
            },
            index,
        })
    }

    pub fn tree(&self, target: &str) -> Option<&TreeInfo> {
        let key = casefold(&compiled(target).ok()?);
        self.index.get(&key).map(|i| &self.trees.trees[*i])
    }

    /// The tree `target` before and after `edit`; with `pick`, the edit's
    /// hue, saturation and brightness become the pick's (`pick_shift` from
    /// the main color, or from `pick.main` when given).
    pub fn preview(&self, target: &str, edit: &TreeEdit, pick: Option<&PickRequest>, main: Option<[u8; 3]>) -> Result<EffectPreview> {
        let tree = self.tree(target).ok_or_else(|| fail(format!("{TREE_UNKNOWN}: {target}")))?;
        preview(tree, edit, pick, main)
    }
}

/// The message when a preview names a tree the loaded effects do not have.
pub const TREE_UNKNOWN: &str = "This effect is not one of this hero's effects. Load the effects again";
/// Why a multiplier does not reach a part the user left out.
pub const USER_LEFT_OUT_REASON: &str = "this part is left out of the edit";
/// Why a multiplier does not reach a shared part the user did not choose.
pub const SHARED_LEFT_OUT_REASON: &str = "this part is shared with other heroes and left out unless you choose it";
pub const PICK_NO_MAIN: &str = "This effect has no color for a picked color to replace (only greys, white or black). Change single colors instead";

/// A group choice: slot order, slot, the bound ability (root) and the record naming the tree.
type Choice = (u32, String, String, String);

/// Where a tree goes: `Linked` (a definition, name stem or folder links it to
/// an own record), else what else in the game names one of its files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Rest {
    Linked,
    /// The hero's model, animation graphs, clips or sound events.
    Model,
    /// Another live definition, or a particle system outside the hero's catalog.
    Elsewhere,
    /// Nothing the game's data shows.
    Unlinked,
}
/// The `Rest` of a tree no own record links (`files`: its casefolded paths).
fn rest_of(files: &[String], model: &BTreeSet<String>, named_elsewhere: &dyn Fn(&str) -> bool) -> Rest {
    if files.iter().any(|f| model.contains(f)) {
        Rest::Model
    } else if files.iter().any(|f| named_elsewhere(f)) {
        Rest::Elsewhere
    } else {
        Rest::Unlinked
    }
}

/// Groups for trees no definition links (`trees`: the top's words without hero
/// names, its casefolded path, the definition's choice). A group's stems are
/// the first two words (one when there is only one) of each tree the
/// definitions put in it, and the words of its ability's internal and game
/// names (abilities and innate abilities only; each counts as one tree). An
/// unlinked tree whose words start with a stem goes to that group when that
/// group has at least twice as many trees with a matching stem as every other
/// group together (`grouped_by: name`); a stem as common in two groups leaves
/// the tree unlinked. Then a tree in a `weapon_fx` folder goes to the primary
/// weapon and one in a `melee` folder to the melee group (`folder`), when the
/// hero has exactly one. `roots`: own bound ability → (order, slot, name word lists).
fn fallback_groups(trees: &[(Vec<String>, String, Option<Choice>)], roots: &BTreeMap<String, (u32, String, Vec<Vec<String>>)>) -> Vec<(Option<Choice>, &'static str, Vec<String>)> {
    let mut stems: BTreeMap<&str, Vec<Vec<String>>> = BTreeMap::new();
    for (words, _, choice) in trees {
        if let Some((_, _, root, _)) = choice {
            if !words.is_empty() {
                stems.entry(root.as_str()).or_default().push(words[..words.len().min(2)].to_vec());
            }
        }
    }
    for (root, (_, _, names)) in roots {
        for words in names.iter().filter(|w| !w.is_empty()) {
            stems.entry(root.as_str()).or_default().push(words[..words.len().min(2)].to_vec());
        }
    }
    let only = |slot: &str| {
        let found: Vec<(&String, &(u32, String, Vec<Vec<String>>))> = roots.iter().filter(|(_, (_, s, _))| s == slot).collect();
        (found.len() == 1).then(|| (found[0].1 .0, slot.to_owned(), found[0].0.clone(), found[0].0.clone()))
    };
    trees
        .iter()
        .map(|(words, path, choice)| {
            if let Some(c) = choice {
                return (Some(c.clone()), "definition", vec![]);
            }
            // Matching trees per group, and the longest stem matched there.
            let mut hits: BTreeMap<&str, (usize, Vec<String>)> = BTreeMap::new();
            for (root, list) in &stems {
                for stem in list.iter().filter(|s| words.len() >= s.len() && words.starts_with(s)) {
                    let hit = hits.entry(root).or_insert((0, vec![]));
                    hit.0 += 1;
                    if stem.len() > hit.1.len() {
                        hit.1 = stem.clone();
                    }
                }
            }
            let total: usize = hits.values().map(|h| h.0).sum();
            if let Some((root, (n, stem))) = hits.iter().max_by_key(|(r, h)| (h.0, std::cmp::Reverse(**r))) {
                if *n >= 2 * (total - n) {
                    if let Some((order, slot, _)) = roots.get(*root) {
                        return (Some((*order, slot.clone(), root.to_string(), root.to_string())), "name", stem.clone());
                    }
                }
                return (None, "none", vec![]);
            }
            let folder = if path.contains("/weapon_fx/") {
                only("ESlot_Weapon_Primary")
            } else if path.contains("/melee/") || words.first().is_some_and(|w| w == "melee") {
                only("ESlot_Weapon_Melee")
            } else {
                None
            };
            match folder {
                Some(c) => (Some(c), "folder", vec![]),
                None => (None, "none", vec![]),
            }
        })
        .collect()
}

/// The particle key naming a part's low-violence substitute.
const LOW_VIOLENCE: &str = "m_hLowViolenceDef";
/// Shortest internal name (letters, without separators) `name_match` uses.
const NAME_MATCH_MIN: usize = 4;
/// First words (after the hero's name) of hero-level effects: animation events
/// (`<hero>_anim_…`; 48 of the 53 such trees the game's data links on the
/// roster are named by the hero's model or animations), the head and the
/// showcase. `ambient` and `reload` are not here: the game uses them for the
/// weapon as often as for the hero.
pub const HERO_LEVEL_WORDS: [&str; 3] = ["anim", "head", "showcase"];
/// A tree whose name (words after the hero's name) starts with a hero-level word.
fn hero_level(words: &[String]) -> bool {
    words.first().is_some_and(|w| HERO_LEVEL_WORDS.contains(&w.as_str()))
}

/// The answer of `name_match`.
#[derive(Debug, Clone, PartialEq, Eq)]
enum NameMatch {
    None,
    One(String),
    /// Two or more abilities fit equally well: nothing is guessed.
    Several(Vec<String>),
}
/// The own ability whose internal name the file name holds (`words`: the top's
/// words without the hero's name; `abilities`: bound signature and innate
/// abilities with their internal name's words, without the hero's name and
/// `citadel`/`ability`). The name must be whole file-name words in a row,
/// compared without separators (`cluster_grenade` holds `clustergrenade`, and
/// `debuffdagger` holds `debuff_dagger`), at least `NAME_MATCH_MIN` letters.
/// A one-word name counts only at the start (after a leading `anim`), so a
/// generic word deep in a name (`…_dash_…`) attracts nothing. The longest name
/// wins; two abilities with names of the same length are not guessed.
fn name_match(words: &[String], abilities: &[(String, Vec<String>)]) -> NameMatch {
    let mut best: Vec<(usize, &str)> = vec![];
    for (root, name) in abilities {
        let joined: String = name.concat();
        if joined.chars().count() < NAME_MATCH_MIN {
            continue;
        }
        let starts: Vec<usize> = if name.len() == 1 {
            let mut s = vec![0];
            if words.first().is_some_and(|w| w == "anim") {
                s.push(1);
            }
            s
        } else {
            (0..words.len()).collect()
        };
        let fits = starts.into_iter().any(|i| {
            let mut run = String::new();
            for w in words.iter().skip(i) {
                run.push_str(w);
                if run.len() >= joined.len() {
                    break;
                }
            }
            run == joined
        });
        if fits {
            best.push((joined.len(), root));
        }
    }
    let Some(longest) = best.iter().map(|b| b.0).max() else { return NameMatch::None };
    let mut top: Vec<String> = best.into_iter().filter(|b| b.0 == longest).map(|b| b.1.to_owned()).collect();
    top.dedup();
    match top.len() {
        1 => NameMatch::One(top.remove(0)),
        _ => NameMatch::Several(top),
    }
}

/// A file-name alias (`resources/catalog/effect_aliases.json`): files of `hero`
/// whose name (after the hero's name) starts with `words` belong to the hero's
/// bound ability `ability` (its internal name), where neither the game's data
/// nor the ability's names say so (Haze's `max_stacks_…` files and Fixation,
/// whose internal name is "stacking damage").
#[derive(Debug, Clone, serde::Deserialize)]
struct Alias {
    hero: String,
    words: Vec<String>,
    ability: String,
}
fn aliases() -> &'static [Alias] {
    static LIST: std::sync::OnceLock<Vec<Alias>> = std::sync::OnceLock::new();
    LIST.get_or_init(|| serde_json::from_str(include_str!("../../../../resources/catalog/effect_aliases.json")).unwrap_or_default())
}
/// The own bound ability an alias gives a file (`hero`: the hero's name tokens).
/// An alias whose ability the hero does not have bound is ignored.
fn alias_match(words: &[String], hero: &BTreeSet<String>, abilities: &[(String, Vec<String>)]) -> Option<String> {
    alias_in(aliases(), words, hero, abilities)
}
fn alias_in(list: &[Alias], words: &[String], hero: &BTreeSet<String>, abilities: &[(String, Vec<String>)]) -> Option<String> {
    list.iter()
        .filter(|a| hero.contains(&casefold(&a.hero)) && !a.words.is_empty() && words.starts_with(&a.words))
        .find_map(|a| abilities.iter().map(|(root, _)| root).find(|root| casefold(root) == casefold(&a.ability)).cloned())
}

/// Shortest word (letters) `display_match` compares.
const DISPLAY_MATCH_MIN: usize = 4;
/// Words too common in effect file names to say which ability a file belongs to.
const DISPLAY_STOP_WORDS: [&str; 71] = [
    "ability", "active", "ambient", "aoe", "area", "base", "beam", "blast", "buff", "bullet", "burst", "cast", "channel", "charge", "charged", "charging", "core", "damage",
    "dark", "death", "debuff", "dust", "effect", "end", "enemy", "energy", "explode", "explosion", "fire", "flame", "flames", "friendly", "glow", "ground", "heal", "healing",
    "hero", "hit", "idle", "impact", "kill", "light", "loop", "main", "mark", "melee", "model", "muzzle", "passive", "player", "power", "proj", "projectile", "ring", "screen",
    "self", "shadow", "shield", "shot", "slow", "smoke", "soul", "speed", "spirit", "start", "status", "stun", "target", "time", "tracer", "trail",
];
/// The words of an ability's display name: lower case, apostrophes dropped
/// ("Djinn's Mark" → `djinns`, `mark`).
fn display_words(name: &str) -> Vec<String> {
    name.to_lowercase().replace(['\'', '\u{2019}'], "").split(|c: char| !c.is_ascii_alphanumeric()).filter(|w| !w.is_empty()).map(str::to_owned).collect()
}
/// What a name is compared by: its words and each pair of neighbours joined
/// (`rat`, `swarm` → `swarm`, `ratswarm`), without short words, numbers and
/// the common effect words.
fn match_tokens(words: &[String]) -> BTreeSet<String> {
    let mut out: BTreeSet<String> = words.iter().cloned().collect();
    out.extend(words.windows(2).map(|pair| pair.concat()));
    out.retain(|w| w.chars().count() >= DISPLAY_MATCH_MIN && !DISPLAY_STOP_WORDS.contains(&w.as_str()) && !w.chars().all(|c| c.is_ascii_digit()));
    out
}
/// The own ability whose DISPLAY name shares a word with the file name
/// (`words`: the top's words without the hero's name; `names`: bound signature
/// and innate abilities with their display name's words): Puddle Punch for
/// `puddle_…` and `glob_punch_…`, Rat Swarm for `ratswarm_…`. A word counts
/// with at least `DISPLAY_MATCH_MIN` letters and when it is not a common
/// effect word; a file that fits two abilities is not guessed.
fn display_match(words: &[String], names: &[(String, Vec<String>)]) -> NameMatch {
    let file = match_tokens(words);
    let mut hits: Vec<String> = names.iter().filter(|(_, name)| !match_tokens(name).is_disjoint(&file)).map(|(root, _)| root.clone()).collect();
    hits.dedup();
    match hits.len() {
        0 => NameMatch::None,
        1 => NameMatch::One(hits.remove(0)),
        _ => NameMatch::Several(hits),
    }
}
/// A file whose name has the word "ult" or "ultimate": the fourth ability's.
fn names_ult(words: &[String]) -> bool {
    words.iter().any(|w| w == "ult" || w == "ultimate")
}

/// The hero's models: `model` first, then the roster's other models of the same hero.
fn hero_models(model: &str) -> Vec<String> {
    let key = |p: &str| casefold(p).trim_end_matches("_c").to_owned();
    let mut out = vec![key(model)];
    if let Ok(roster) = heroes::roster() {
        if let Some(hero) = roster.iter().find(|h| key(&h.path) == out[0] || h.extras.iter().any(|e| key(&e.path) == out[0])) {
            for m in std::iter::once(&hero.path).chain(hero.extras.iter().map(|e| &e.path)) {
                if !out.contains(&key(m)) {
                    out.push(key(m));
                }
            }
        }
    }
    out
}

fn round(v: f64) -> f64 {
    // `+ 0.` turns -0 (from empty sums) into 0.
    (v * 10_000.).round() / 10_000. + 0.
}
fn change(before: [u8; 3], after: [u8; 3]) -> ColorChange {
    ColorChange { before, before_hex: colors::hex(before), after, after_hex: colors::hex(after) }
}

/// What the part's own settings in `edit` do (`effect_edit::apply_controls`
/// on the part's file): the same rules and arithmetic, from the part's
/// analysis. Empty when the part has no settings.
fn control_preview(part: &PartInfo, edit: &TreeEdit, left_out: Option<colors::LeftOut>) -> Vec<ControlPreview> {
    use effect_edit::Control;
    let c = edit.controls(&part.path);
    if c.is_neutral() {
        return vec![];
    }
    let entry = |control, applied: bool, status: &'static str, reason: String| ControlPreview { control, renderer: String::new(), location: String::new(), before: None, after: None, color: None, applied, status, reason };
    let wanted: Vec<Control> = [
        (Control::Hide, c.hidden),
        (Control::Size, c.size.is_some_and(|v| v != 1.)),
        (Control::Opacity, c.opacity.is_some_and(|v| v != 1.)),
        (Control::Tint, c.tint.is_some()),
        (Control::Light, c.light.is_some_and(|v| v != 1.)),
    ]
    .into_iter()
    .filter_map(|(k, on)| on.then_some(k))
    .collect();
    if let Some(why) = left_out {
        let reason = if why == colors::LeftOut::Shared { SHARED_LEFT_OUT_REASON } else { USER_LEFT_OUT_REASON };
        return wanted.into_iter().map(|k| ControlPreview { color: if k == Control::Tint { c.tint } else { None }, ..entry(k, false, "left_out", reason.into()) }).collect();
    }
    let mut out = vec![];
    for control in wanted {
        match control {
            Control::Hide => match part.controls.hide.offered {
                true => out.push(ControlPreview { renderer: part.renderers.join(", "), after: Some(1.), ..entry(control, true, "adds", String::new()) }),
                false => out.push(entry(control, false, "not_offered", part.controls.hide.reason.clone())),
            },
            Control::Size | Control::Opacity => {
                let (m, factor, offered) = if control == Control::Size { (Multiplier::Size, c.size.unwrap_or(1.), &part.controls.size) } else { (Multiplier::Opacity, c.opacity.unwrap_or(1.), &part.controls.opacity) };
                let sites: Vec<&Site> = part.own_sites.iter().filter(|s| s.multiplier == m).collect();
                if sites.is_empty() {
                    out.push(entry(control, false, "not_offered", offered.reason.clone()));
                }
                // The effect-wide multiplier is applied first where it reaches the same value.
                let wide = edit.multiplier(m);
                for s in sites {
                    let wide_applies = wide != 1. && part.multipliers.iter().any(|w| w.multiplier == m && w.applies && w.location == s.location);
                    let added = s.status == "adds";
                    let after = if !s.applies {
                        s.value
                    } else if added {
                        Some(factor)
                    } else {
                        s.value.map(|v| effect_edit::scaled_value(if wide_applies { effect_edit::scaled_value(v, wide, s.single) } else { v }, factor, s.single))
                    };
                    let before = if wide_applies { s.value.map(|v| effect_edit::scaled_value(v, wide, s.single)) } else { s.value };
                    out.push(ControlPreview {
                        control,
                        renderer: s.renderer.clone(),
                        location: s.location.clone(),
                        before,
                        after,
                        color: None,
                        applied: s.applies,
                        status: s.status,
                        reason: if s.applies { String::new() } else { s.reason.clone() },
                    });
                }
            }
            Control::Tint => match part.controls.tint.offered {
                true => out.push(ControlPreview { renderer: "CParticleSystemDefinition".into(), location: "m_ConstantColor".into(), color: c.tint, ..entry(control, true, "adds", String::new()) }),
                false => out.push(ControlPreview { color: c.tint, ..entry(control, false, "not_offered", part.controls.tint.reason.clone()) }),
            },
            // Each light as `apply_controls` writes it (`light_sites`; no effect-wide multiplier reaches a light).
            Control::Light => {
                let factor = c.light.unwrap_or(1.);
                if part.light_sites.is_empty() {
                    out.push(entry(control, false, "not_offered", effect_edit::LIGHT_NONE.into()));
                }
                for s in &part.light_sites {
                    let applied = s.refusal.is_none() && s.before.is_some();
                    let status = if applied { "changes" } else if s.refusal == Some(effect_edit::LIGHT_GAME_SET) { "game_input" } else { "not_offered" };
                    out.push(ControlPreview {
                        control,
                        renderer: s.renderer.clone(),
                        location: s.location.to_string(),
                        before: s.before,
                        after: if applied { s.before.map(|v| effect_edit::light_value(v, factor, s.single)) } else { s.before },
                        color: None,
                        applied,
                        status,
                        reason: s.refusal.unwrap_or_default().to_owned(),
                    });
                }
            }
        }
    }
    out
}

/// `HeroEffects::preview` for one tree.
pub fn preview(tree: &TreeInfo, edit: &TreeEdit, pick: Option<&PickRequest>, main: Option<[u8; 3]>) -> Result<EffectPreview> {
    edit.check()?;
    let mut edit = edit.clone();
    for path in &edit.excluded {
        if !tree.parts.iter().any(|p| casefold(&p.path) == casefold(path)) {
            return Err(fail(format!("{path} is not a part of this effect.")));
        }
    }
    // The main color: the pick's, else the user's choice, else the detected one.
    let chosen = pick.and_then(|p| p.main).or(main).or_else(|| tree.main.as_ref().map(|m| m.rgb));
    let pick = match pick {
        Some(request) => {
            let main = chosen.ok_or_else(|| fail(PICK_NO_MAIN))?;
            let result = colors::pick_shift(main, request.color);
            edit.hue = result.hue;
            edit.saturation = result.saturation;
            edit.brightness = result.brightness;
            Some(result)
        }
        None => None,
    };
    let mut changed = Changed::default();
    let mut parts = vec![];
    for part in &tree.parts {
        let left_out = edit.left_out(&part.path, part.outside_hero);
        let excluded = left_out.is_some();
        let untouched = excluded;
        let strips = part
            .strips
            .iter()
            .map(|s| StripPreview {
                id: s.id.clone(),
                kind: s.kind,
                role: s.role.clone(),
                stops: s
                    .stops
                    .iter()
                    .map(|t| {
                        let editable = t.slot.is_some() || t.source.is_some();
                        let (after, rule) = if excluded { (t.rgb, Rule::Excluded) } else if editable { edit.color(false, t.rgb) } else { (t.rgb, Rule::Unchanged) };
                        if t.slot.is_some() && after != t.rgb {
                            changed.colors += 1;
                        }
                        StopPreview {
                            position: t.position,
                            alpha: t.alpha,
                            before: t.rgb,
                            before_hex: t.hex.clone(),
                            after,
                            after_hex: colors::hex(after),
                            slot: t.slot.clone(),
                            rule,
                        }
                    })
                    .collect(),
            })
            .collect();
        let multipliers = part
            .multipliers
            .iter()
            .map(|s| {
                // A left-out part gets no multiplier either.
                let factor = if untouched { 1. } else { edit.multiplier(s.multiplier) };
                let applied = s.applies && factor != 1.;
                let added = s.status == "adds";
                if applied {
                    changed.multiplied += 1;
                } else if factor != 1. {
                    changed.not_multiplied += 1;
                }
                SitePreview {
                    multiplier: s.multiplier,
                    renderer: s.renderer.clone(),
                    location: s.location.clone(),
                    before: s.value,
                    after: if applied && added { Some(factor) } else if applied { s.value.map(|v| effect_edit::scaled_value(v, factor, s.single)) } else { s.value },
                    applied,
                    status: s.status,
                    reason: if untouched && edit.multiplier(s.multiplier) != 1. { if left_out == Some(colors::LeftOut::Shared) { SHARED_LEFT_OUT_REASON.into() } else { USER_LEFT_OUT_REASON.into() } } else if s.applies || factor == 1. { String::new() } else { s.reason.clone() },
                }
            })
            .collect();
        let controls = control_preview(part, &edit, left_out);
        let hidden = controls.iter().any(|c| c.control == effect_edit::Control::Hide && c.applied);
        changed.hidden += usize::from(hidden);
        changed.part_settings += controls.iter().filter(|c| c.applied).count();
        parts.push(PartPreview { path: part.path.clone(), excluded, left_out, strips, multipliers, hidden, controls });
    }
    let swatch_after = |s: &Swatch| {
        let excluded = !s.parts.is_empty() && s.parts.iter().all(|p| tree.parts.iter().find(|q| &q.path == p).is_some_and(|q| edit.left_out(p, q.outside_hero).is_some()));
        change(s.rgb, edit.color(excluded, s.rgb).0)
    };
    let palette = tree.palette.iter().map(swatch_after).collect();
    let main = chosen.map(|rgb| change(rgb, edit.color(false, rgb).0));
    Ok(EffectPreview { target: tree.target.clone(), edit, pick, parts, palette, main, changed })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn effects_with_one_label_are_told_apart_by_their_file_names_then_their_folders() {
        let hero = vec![vec!["nurse".to_owned()]];
        let by_name = telling_words(&["particles/abilities/nurse/nurse_heal_friendly.vpcf_c", "particles/abilities/nurse/nurse_heal_friendly_ring.vpcf_c"], &hero);
        // The one without a differing word keeps its label.
        assert_eq!(by_name, Some(vec![String::new(), "ring".to_owned()]));
        let three = telling_words(&["particles/abilities/nurse/nurse_beam_friendly_start.vpcf_c", "particles/abilities/nurse/nurse_beam_friendly_end.vpcf_c"], &hero);
        assert_eq!(three, Some(vec!["start".to_owned(), "end".to_owned()]));
        let by_folder = telling_words(&["particles/abilities/nurse/heal/friendly.vpcf_c", "particles/abilities/nurse/beam/friendly.vpcf_c"], &hero);
        assert_eq!(by_folder, Some(vec!["heal".to_owned(), "beam".to_owned()]));
        assert_eq!(telling_words(&["particles/a/friendly.vpcf_c", "particles/a/friendly.vpcf"], &hero), None);
    }
    use crate::effect_catalog::{ChildIndex, Names};
    use crate::resources::{Association, CatalogRow};
    use deadlock_resource::{Annotation, binary, container};

    fn v(kind: Kind) -> Value {
        Value::new(kind)
    }
    fn s(t: &str) -> Value {
        v(Kind::String(t.into()))
    }
    fn f(n: f64) -> Value {
        v(Kind::F64(n))
    }
    fn color(c: &[i64]) -> Value {
        v(Kind::Array(c.iter().map(|n| v(Kind::I64(*n))).collect()))
    }
    fn obj(rows: Vec<(&str, Value)>) -> Value {
        v(Kind::Object(rows.into_iter().map(|(k, v)| (k.to_owned(), v)).collect()))
    }
    fn arr(rows: Vec<Value>) -> Value {
        v(Kind::Array(rows))
    }
    fn literal(n: f64) -> Value {
        obj(vec![("m_nType", s("PF_TYPE_LITERAL")), ("m_flLiteralValue", f(n))])
    }
    fn system(constant: &[i64], kids: &[&str], renderer: Option<Value>) -> Value {
        let mut rows = vec![
            ("_class", s("CParticleSystemDefinition")),
            ("m_nMaxParticles", v(Kind::I64(32))),
            ("m_ConstantColor", color(constant)),
            ("m_Emitters", arr(vec![obj(vec![("_class", s("C_OP_InstantaneousEmitter")), ("m_nParticlesToEmit", literal(9.))])])),
            // Inherits the parent's radius (no field): not a color source.
            ("m_Initializers", arr(vec![obj(vec![("_class", s("C_INIT_InheritFromParentParticles"))])])),
        ];
        if let Some(r) = renderer {
            rows.push(("m_Renderers", arr(vec![r])));
        }
        rows.push((
            "m_Children",
            arr(kids
                .iter()
                .map(|k| {
                    let mut p = s(k);
                    p.annotation = Annotation::Resource;
                    obj(vec![("m_ChildRef", p)])
                })
                .collect()),
        ));
        obj(rows)
    }
    fn sprite(additive: bool) -> Value {
        let mut rows = vec![("_class", s("C_OP_RenderSprites")), ("m_flRadiusScale", literal(2.)), ("m_flOverbrightFactor", f(4.))];
        if additive {
            rows.push(("m_nOutputBlendMode", s("PARTICLE_OUTPUT_BLEND_MODE_ADD")));
            rows.push(("m_flMaxSize", literal(0.1)));
        }
        obj(rows)
    }
    fn resource(root: &Value) -> Vec<u8> {
        let base = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/particle.bin"));
        let layout = container::layout(base).unwrap();
        let block = container::unique(&layout.blocks, "DATA").unwrap();
        let doc = binary::decode(container::payload(base, block).unwrap()).unwrap();
        container::rebuild(base, &BTreeMap::from([("DATA".to_owned(), binary::encode(root, doc.format_guid).unwrap())])).unwrap()
    }

    /// Two trees sharing a part, a child cycle, a model renderer and a
    /// smoky dark part, analysed from documents only (no archive).
    fn files() -> BTreeMap<&'static str, Value> {
        BTreeMap::from([
            ("particles/abilities/ghost/ghost_bomb.vpcf_c", system(&[255, 255, 255, 255], &["particles/abilities/ghost/ghost_bomb_core.vpcf", "particles/abilities/ghost/ghost_bomb_smoke.vpcf"], None)),
            ("particles/abilities/ghost/ghost_bomb_core.vpcf_c", system(&[20, 200, 180, 255], &[], Some(sprite(true)))),
            ("particles/abilities/ghost/ghost_bomb_smoke.vpcf_c", system(&[30, 20, 20, 255], &[], Some(sprite(false)))),
            ("particles/abilities/ghost/ghost_wisp.vpcf_c", system(&[200, 40, 40, 255], &["particles/abilities/ghost/ghost_bomb_core.vpcf"], Some(obj(vec![("_class", s("C_OP_RenderModels"))])))),
            ("particles/abilities/ghost/ghost_loop_a.vpcf_c", system(&[90, 90, 255, 255], &["particles/abilities/ghost/ghost_loop_b.vpcf"], Some(sprite(true)))),
            ("particles/abilities/ghost/ghost_loop_b.vpcf_c", system(&[90, 90, 255, 255], &["particles/abilities/ghost/ghost_loop_a.vpcf"], Some(sprite(true)))),
        ])
    }

    #[test]
    fn unlinked_trees_join_the_ability_whose_trees_share_their_name_stem() {
        let w = |s: &str| s.split('_').map(String::from).collect::<Vec<_>>();
        let c = |order: u32, slot: &str, root: &str| Some((order, slot.to_owned(), root.to_owned(), root.to_owned()));
        let roots = BTreeMap::from([
            ("ability_bomb".to_owned(), (101, "ESlot_Signature_1".to_owned(), vec![w("bomb")])),
            ("ability_shards".to_owned(), (102, "ESlot_Signature_2".to_owned(), vec![w("shards")])),
            ("ability_swap".to_owned(), (103, "ESlot_Signature_3".to_owned(), vec![w("health_swap"), w("soul_exchange")])),
            ("weapon_set".to_owned(), (200, "ESlot_Weapon_Primary".to_owned(), vec![])),
        ]);
        let trees = vec![
            (w("blood_bomb_explode"), "particles/abilities/h/h_blood_bomb_explode.vpcf_c".to_owned(), c(101, "ESlot_Signature_1", "ability_bomb")),
            (w("blood_bomb_projectile"), "p".to_owned(), c(101, "ESlot_Signature_1", "ability_bomb")),
            // A shard tree the bomb's definition also names (a cast effect).
            (w("blood_shard_self"), "p".to_owned(), c(101, "ESlot_Signature_1", "ability_bomb")),
            (w("blood_shard_dagger"), "p".to_owned(), c(102, "ESlot_Signature_2", "ability_shards")),
            (w("blood_shard_impact"), "p".to_owned(), c(102, "ESlot_Signature_2", "ability_shards")),
            (w("tracer"), "particles/weapon_fx/h/h_tracer.vpcf_c".to_owned(), c(200, "ESlot_Weapon_Primary", "weapon_set")),
            // Unlinked:
            (w("blood_bomb_debuff"), "p".to_owned(), None),
            (w("blood_shard_halo"), "p".to_owned(), None),
            (w("health_swap_glow"), "p".to_owned(), None),
            (w("tracer_bullet"), "particles/weapon_fx/h/h_tracer_bullet.vpcf_c".to_owned(), None),
            (w("muzzle_glow"), "particles/weapon_fx/h/h_muzzle_glow.vpcf_c".to_owned(), None),
            (w("blood_rain"), "p".to_owned(), None),
            (w("mad_milk"), "p".to_owned(), None),
            (w("soul_exchange_flash"), "p".to_owned(), None),
        ];
        let out = fallback_groups(&trees, &roots);
        let got: Vec<(Option<&str>, &str)> = out.iter().map(|(c, by, _)| (c.as_ref().map(|c| c.2.as_str()), *by)).collect();
        assert_eq!(
            got[6..],
            [
                (Some("ability_bomb"), "name"),
                (Some("ability_shards"), "name"), // two shard trees against one: twice as many
                (Some("ability_swap"), "name"),   // the ability's internal name
                (Some("weapon_set"), "name"),     // a weapon tree's stem
                (Some("weapon_set"), "folder"),
                (None, "none"),                   // "blood" alone is no stem of anyone's
                (None, "none"),
                (Some("ability_swap"), "name"), // the ability's game name
            ]
        );
        assert_eq!(out[6].2, w("blood_bomb"));
        assert_eq!(out[0].1, "definition");
        // As common in two groups: unlinked.
        let tie = vec![
            (w("blood_shard_a"), "p".to_owned(), c(101, "ESlot_Signature_1", "ability_bomb")),
            (w("blood_shard_b"), "p".to_owned(), c(102, "ESlot_Signature_2", "ability_shards")),
            (w("blood_shard_c"), "p".to_owned(), None),
        ];
        let roots = BTreeMap::from([
            ("ability_bomb".to_owned(), (101, "ESlot_Signature_1".to_owned(), vec![])),
            ("ability_shards".to_owned(), (102, "ESlot_Signature_2".to_owned(), vec![])),
        ]);
        assert_eq!(fallback_groups(&tie, &roots)[2].1, "none");
    }

    #[test]
    fn a_file_name_holding_an_abilitys_internal_name_finds_it() {
        let w = |s: &str| s.split('_').map(String::from).collect::<Vec<_>>();
        let abilities = vec![
            ("ability_bomb".to_owned(), w("bomb")),
            ("ability_dark_mist".to_owned(), w("dark_mist")),
            ("ability_soul_rift".to_owned(), w("soul_rift")),
            ("ability_soul_rift_storm".to_owned(), w("soul_rift_storm")),
            ("ability_clustergrenade".to_owned(), w("clustergrenade")),
            ("ability_debuff_dagger".to_owned(), w("debuff_dagger")),
            ("ability_orb".to_owned(), w("orb")),
        ];
        let one = |r: &str| NameMatch::One(r.to_owned());
        assert_eq!(name_match(&w("dark_mist_cloud"), &abilities), one("ability_dark_mist"));
        assert_eq!(name_match(&w("cast_dark_mist"), &abilities), one("ability_dark_mist"), "a name of two words counts anywhere");
        assert_eq!(name_match(&w("bomb_trail"), &abilities), one("ability_bomb"));
        assert_eq!(name_match(&w("anim_bomb_throw"), &abilities), one("ability_bomb"), "after a leading anim");
        assert_eq!(name_match(&w("flash_bomb"), &abilities), NameMatch::None, "a one-word name only at the start");
        assert_eq!(name_match(&w("dark_mistral"), &abilities), NameMatch::None, "whole words only");
        assert_eq!(name_match(&w("soul_rift_storm_glow"), &abilities), one("ability_soul_rift_storm"), "the longest name wins");
        assert_eq!(name_match(&w("cluster_grenade_explode"), &abilities), one("ability_clustergrenade"), "separators are ignored");
        assert_eq!(name_match(&w("debuffdagger_hit"), &abilities), one("ability_debuff_dagger"));
        assert_eq!(name_match(&w("orb_glow"), &abilities), NameMatch::None, "names shorter than four letters attract nothing");
        assert_eq!(name_match(&w("dark_mist_soul_rift"), &abilities), NameMatch::Several(vec!["ability_dark_mist".into(), "ability_soul_rift".into()]), "two names as long: not guessed");
        assert!(hero_level(&w("head_ambient")) && hero_level(&w("anim_reload")) && hero_level(&w("showcase_dagger")));
        assert!(!hero_level(&w("ambient_glow")) && !hero_level(&w("bomb_head")));
    }

    #[test]
    fn a_file_name_sharing_a_word_with_one_abilitys_display_name_finds_it() {
        let w = |s: &str| s.split('_').map(String::from).collect::<Vec<_>>();
        assert_eq!(display_words("Djinn's Mark"), ["djinns", "mark"]);
        assert_eq!(display_words("Rule, Ratannia!"), ["rule", "ratannia"]);
        let names = vec![
            ("ability_telepunch".to_owned(), display_words("Puddle Punch")),
            ("ability_swarm".to_owned(), display_words("Rat Swarm")),
            ("ability_djinn".to_owned(), display_words("Djinn's Mark")),
            ("ability_shot".to_owned(), display_words("Power Shot")),
        ];
        let one = |r: &str| NameMatch::One(r.to_owned());
        assert_eq!(display_match(&w("puddle_aoe"), &names), one("ability_telepunch"));
        assert_eq!(display_match(&w("glob_punch_hit"), &names), one("ability_telepunch"));
        assert_eq!(display_match(&w("ratswarm_aoe"), &names), one("ability_swarm"), "two display words meet one file word");
        assert_eq!(display_match(&w("rat_swarm_aoe"), &names), one("ability_swarm"));
        assert_eq!(display_match(&w("djinns_reach_glow"), &names), one("ability_djinn"), "the apostrophe is dropped");
        assert_eq!(display_match(&w("rat_trail"), &names), NameMatch::None, "words under four letters attract nothing");
        assert_eq!(display_match(&w("shot_impact"), &names), NameMatch::None, "common effect words attract nothing");
        assert_eq!(display_match(&w("power_shot_impact"), &names), one("ability_shot"), "the whole display name in a row still counts");
        assert_eq!(display_match(&w("mark_debuff"), &names), NameMatch::None);
        assert_eq!(display_match(&w("punchline_glow"), &names), NameMatch::None, "whole words only");
        assert_eq!(display_match(&w("puddle_swarm"), &names), NameMatch::Several(vec!["ability_telepunch".into(), "ability_swarm".into()]), "two abilities fit: not guessed");
        assert!(names_ult(&w("ult_cast")) && names_ult(&w("blade_ultimate_glow")));
        assert!(!names_ult(&w("vault_glow")) && !names_ult(&w("cult")));
    }

    #[test]
    fn an_alias_gives_a_file_its_ability_only_for_its_hero_and_a_bound_ability() {
        let w = |s: &str| s.split('_').map(String::from).collect::<Vec<_>>();
        let list = vec![Alias { hero: "Sample".into(), words: w("max_stacks"), ability: "ability_stacking".into() }];
        let hero: BTreeSet<String> = ["sample".to_owned()].into();
        let other: BTreeSet<String> = ["another".to_owned()].into();
        let bound = vec![("ability_stacking".to_owned(), w("stacking"))];
        assert_eq!(alias_in(&list, &w("max_stacks_debuff"), &hero, &bound), Some("ability_stacking".to_owned()));
        assert_eq!(alias_in(&list, &w("debuff_max_stacks"), &hero, &bound), None, "the words lead the name");
        assert_eq!(alias_in(&list, &w("max_stacks_debuff"), &other, &bound), None, "another hero's files");
        assert_eq!(alias_in(&list, &w("max_stacks_debuff"), &hero, &[]), None, "an ability the hero does not have bound");
        // The shipped list: every entry is complete and names a hero of the roster.
        let roster: BTreeSet<String> = heroes::roster().unwrap().iter().flat_map(|h| heroes::tokens(&h.path).unwrap_or_default()).collect();
        assert!(!aliases().is_empty(), "the alias file must parse");
        for alias in aliases() {
            assert!(roster.contains(&casefold(&alias.hero)), "unknown hero in effect_aliases.json: {}", alias.hero);
            assert!(!alias.words.is_empty() && alias.words.iter().all(|word| !word.is_empty() && *word == casefold(word)), "{alias:?}");
            assert!(alias.ability.starts_with("ability_") || alias.ability.starts_with("citadel_ability_"), "{alias:?}");
        }
    }

    /// The model file of a summon: a resource whose references name `particles`.
    fn model_naming(particles: &[&str]) -> Vec<u8> {
        let base = resource(&system(&[0, 0, 0, 255], &[], None));
        let mut rerl = vec![0u8; 8 + 16 * particles.len()];
        rerl[..4].copy_from_slice(&8u32.to_le_bytes());
        rerl[4..8].copy_from_slice(&(particles.len() as u32).to_le_bytes());
        for (j, name) in particles.iter().enumerate() {
            let at = 8 + j * 16;
            let delta = rerl.len() - at - 8;
            rerl[at..at + 8].copy_from_slice(&(j as u64 + 1).to_le_bytes());
            rerl[at + 8..at + 16].copy_from_slice(&(delta as u64).to_le_bytes());
            rerl.extend_from_slice(name.as_bytes());
            rerl.push(0);
        }
        container::rebuild(&base, &BTreeMap::from([("RERL".to_owned(), rerl)])).unwrap()
    }

    #[test]
    fn references_and_file_names_group_trees_no_definition_links() {
        fn hero(model: &str, bound: &[(&str, &str)]) -> Value {
            obj(vec![("m_strModelName", s(model)), ("m_bDisabled", v(Kind::Bool(false))), ("m_mapBoundAbilities", obj(bound.iter().map(|(k, a)| (*k, s(a))).collect()))])
        }
        fn ability(fields: Vec<(&str, Value)>) -> Value {
            let mut rows = vec![("m_eAbilityType", s("EAbilityType_Signature"))];
            rows.extend(fields);
            obj(rows)
        }
        let p = |name: &str| format!("particles/abilities/ghost/{name}.vpcf");
        let heroes = obj(vec![(
            "hero_ghost",
            hero(
                "models/heroes_wip/geist/geist.vmdl",
                &[("ESlot_Signature_1", "ability_bomb"), ("ESlot_Signature_2", "ability_ghost_dark_mist"), ("ESlot_Signature_3", "ability_ghost_soul_rift"), ("ESlot_Signature_4", "ability_ghost_soul_rift_storm")],
            ),
        )]);
        let abilities = obj(vec![
            // The bomb's definition also names a file holding another ability's name.
            ("ability_bomb", ability(vec![("m_Particle", s(&p("ghost_bomb"))), ("m_Flash", s(&p("ghost_dark_mist_flash")))])),
            // No file of its own: only its name and its summon's model link files to it.
            ("ability_ghost_dark_mist", ability(vec![("m_TotemModel", s("models/abilities/ghost_totem.vmdl"))])),
            ("ability_ghost_soul_rift", ability(vec![])),
            ("ability_ghost_soul_rift_storm", ability(vec![])),
            // Bound to no hero: a leftover, which attracts nothing and keeps what it names folded.
            ("ability_ghost_old_trick", ability(vec![("m_Particle", s(&p("ghost_soul_rift_spark")))])),
        ]);
        let definitions = effect_catalog::Definitions::from_documents(&[("heroes".into(), heroes), ("abilities".into(), abilities)]);
        let mut core = system(&[20, 200, 180, 255], &[], Some(sprite(true)));
        if let Kind::Object(rows) = &mut core.kind {
            rows.push(("m_hLowViolenceDef".to_owned(), s(&p("ghost_low_gore"))));
        }
        let lone = |rgb: &[i64]| system(rgb, &[], Some(sprite(true)));
        let docs: Vec<(String, Value)> = vec![
            (p("ghost_bomb") + "_c", system(&[255, 255, 255, 255], &[&p("ghost_bomb_core")], None)),
            (p("ghost_bomb_core") + "_c", core),
            (p("ghost_low_gore") + "_c", lone(&[90, 20, 20, 255])),
            (p("ghost_dark_mist_flash") + "_c", lone(&[90, 90, 255, 255])),
            (p("ghost_dark_mist_cloud") + "_c", lone(&[90, 90, 255, 255])),
            (p("ghost_anim_dark_mist") + "_c", lone(&[90, 90, 255, 255])),
            (p("ghost_dark_mist_soul_rift") + "_c", lone(&[90, 90, 255, 255])),
            (p("ghost_soul_rift_storm_glow") + "_c", lone(&[90, 90, 255, 255])),
            (p("ghost_soul_rift_spark") + "_c", lone(&[90, 90, 255, 255])),
            (p("ghost_old_trick_fx") + "_c", lone(&[90, 90, 255, 255])),
            (p("ghost_flash_bomb") + "_c", lone(&[90, 90, 255, 255])),
            (p("ghost_head_glow") + "_c", lone(&[90, 90, 255, 255])),
            (p("ghost_totem_glow") + "_c", lone(&[90, 90, 255, 255])),
            // An item's effect the summon also carries, linked into the catalog: not moved.
            ("particles/upgrades/shared_buff.vpcf_c".to_owned(), lone(&[90, 90, 255, 255])),
        ];
        let mut raws: BTreeMap<String, Vec<u8>> = docs.iter().map(|(k, d)| (k.clone(), resource(d))).collect();
        let rows: Vec<CatalogRow> = raws
            .keys()
            .map(|p| CatalogRow {
                path: p.to_string(),
                target: p.to_string(),
                label: String::new(),
                association: if p.starts_with("particles/upgrades/") {
                    Association { association: "resource_reference".into(), shared: true }
                } else {
                    Association { association: "hero_directory".into(), shared: false }
                },
                supported: true,
                reason: String::new(),
                original_sha256: None,
                metadata: None,
                wav_supported: false,
                sound: None,
            })
            .collect();
        raws.insert("models/abilities/ghost_totem.vmdl_c".into(), model_naming(&[&p("ghost_totem_glow"), "particles/upgrades/shared_buff.vpcf"]));
        let game = GameIndex { key: String::new(), definitions: Some(definitions), names: Names::default(), children: ChildIndex::default(), load_ms: 0. };
        let effects = build_from(&raws, "models/heroes_wip/geist/geist.vmdl", &rows, &game).unwrap();
        let t = &effects.trees;
        let at = |name: &str| {
            let tree = effects.tree(&(p(name) + "_c")).unwrap_or_else(|| panic!("{name}"));
            (tree.group.as_str(), tree.grouped_by)
        };
        assert_eq!(at("ghost_bomb"), ("ability:ability_bomb", "definition"));
        // A definition link is never moved by a name.
        assert_eq!(at("ghost_dark_mist_flash"), ("ability:ability_bomb", "definition"));
        // A low-violence substitute follows the part that names it.
        assert_eq!(at("ghost_low_gore"), ("ability:ability_bomb", "reference"));
        // A model the ability names (a summon) names the file.
        assert_eq!(at("ghost_totem_glow"), ("ability:ability_ghost_dark_mist", "reference"));
        let shared = effects.tree("particles/upgrades/shared_buff.vpcf_c").unwrap();
        assert_eq!((shared.group.as_str(), shared.grouped_by), (UNLINKED_GROUP, "none"), "a shared file the summon names stays where it was");
        // The rule behind each placement, for the audit.
        let rule = |name: &str| effects.tree(&(p(name) + "_c")).unwrap().rule;
        assert_eq!(
            ["ghost_bomb", "ghost_low_gore", "ghost_totem_glow", "ghost_dark_mist_cloud", "ghost_head_glow", "ghost_flash_bomb"].map(rule),
            ["definition", "low_violence", "summon_model", "internal_name", "hero_word", "none"]
        );
        // The internal name in the file name, with no tree of the ability's to anchor it.
        assert_eq!(at("ghost_dark_mist_cloud"), ("ability:ability_ghost_dark_mist", "name"));
        assert_eq!(at("ghost_anim_dark_mist"), ("ability:ability_ghost_dark_mist", "name"), "an ability's name before the hero-level word");
        assert_eq!(at("ghost_soul_rift_storm_glow"), ("ability:ability_ghost_soul_rift_storm", "name"));
        // Two names as long: left where it was, with both reported.
        assert_eq!(at("ghost_dark_mist_soul_rift"), (UNLINKED_GROUP, "none"));
        assert_eq!(effects.tree(&(p("ghost_dark_mist_soul_rift") + "_c")).unwrap().name_candidates, ["ability_ghost_dark_mist", "ability_ghost_soul_rift"]);
        // Named by a leftover definition: no name rule moves it.
        assert_eq!(at("ghost_soul_rift_spark"), (UNLINKED_GROUP, "none"));
        // An ability the hero does not have attracts nothing; a one-word name counts only at the start.
        assert_eq!(at("ghost_old_trick_fx"), (UNLINKED_GROUP, "none"));
        assert_eq!(at("ghost_flash_bomb"), (UNLINKED_GROUP, "none"));
        // A hero-level name goes to the hero's own group.
        assert_eq!(at("ghost_head_glow"), ("hero:hero_ghost", "name"));
        let json = serde_json::to_value(effects.tree(&(p("ghost_head_glow") + "_c")).unwrap()).unwrap();
        assert_eq!(json["grouped_by"], "name");
        assert!(json.get("name_candidates").is_none(), "the candidates stay out of the page's JSON");
        // Only the ability whose files were ambiguous or leftovers keeps its empty group.
        let empty: Vec<&str> = t.groups.iter().filter(|g| g.kind == "ability" && g.trees == 0).map(|g| g.key.as_str()).collect();
        assert_eq!(empty, ["ability:ability_ghost_soul_rift"]);
    }

    #[test]
    fn unlinked_trees_are_sorted_by_what_names_them() {
        let model = BTreeSet::from(["particles/h/head.vpcf_c".to_owned()]);
        let elsewhere = |f: &str| f == "particles/h/shared_child.vpcf_c";
        let files = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(rest_of(&files(&["particles/h/head.vpcf_c"]), &model, &elsewhere), Rest::Model);
        // A child the model names is enough; the model wins over another user.
        assert_eq!(rest_of(&files(&["particles/h/top.vpcf_c", "particles/h/shared_child.vpcf_c", "particles/h/head.vpcf_c"]), &model, &elsewhere), Rest::Model);
        assert_eq!(rest_of(&files(&["particles/h/top.vpcf_c", "particles/h/shared_child.vpcf_c"]), &model, &elsewhere), Rest::Elsewhere);
        assert_eq!(rest_of(&files(&["particles/h/old_leap.vpcf_c"]), &model, &elsewhere), Rest::Unlinked);
    }

    #[test]
    fn abilities_come_in_slot_order_and_one_without_own_files_says_so() {
        fn hero(model: &str, bound: &[(&str, &str)]) -> Value {
            obj(vec![("m_strModelName", s(model)), ("m_bDisabled", v(Kind::Bool(false))), ("m_mapBoundAbilities", obj(bound.iter().map(|(k, a)| (*k, s(a))).collect()))])
        }
        fn ability(kind: &str, particle: &str) -> Value {
            obj(vec![("m_eAbilityType", s(kind)), ("m_Particle", s(particle))])
        }
        let heroes = obj(vec![(
            "hero_ghost",
            hero(
                "models/heroes_wip/geist/geist.vmdl",
                // Listed out of order on purpose: the list follows the slots.
                &[("ESlot_Signature_3", "ability_far"), ("ESlot_Weapon_Primary", "citadel_weapon_ghost_set"), ("ESlot_Signature_1", "ability_bomb"), ("ESlot_Signature_2", "ability_wisp")],
            ),
        )]);
        let abilities = obj(vec![
            ("ability_bomb", ability("EAbilityType_Signature", "particles/abilities/ghost/ghost_bomb.vpcf")),
            ("ability_wisp", ability("EAbilityType_Signature", "particles/abilities/ghost/ghost_wisp.vpcf")),
            // Its only effect is a file of another folder: no tree of the hero's.
            ("ability_far", ability("EAbilityType_Signature", "particles/generic/generic_arc.vpcf")),
            ("citadel_weapon_ghost_set", ability("EAbilityType_Weapon", "particles/weapon_fx/ghost/ghost_tracer.vpcf")),
        ]);
        let definitions = effect_catalog::Definitions::from_documents(&[("heroes".into(), heroes), ("abilities".into(), abilities)]);
        let docs = files();
        let raws: BTreeMap<String, Vec<u8>> = docs.iter().map(|(k, d)| (k.to_string(), resource(d))).collect();
        let rows: Vec<CatalogRow> = docs
            .keys()
            .map(|p| CatalogRow {
                path: p.to_string(),
                target: p.to_string(),
                label: String::new(),
                association: Association { association: "hero_directory".into(), shared: false },
                supported: true,
                reason: String::new(),
                original_sha256: None,
                metadata: None,
                wav_supported: false,
                sound: None,
            })
            .collect();
        let game = GameIndex { key: String::new(), definitions: Some(definitions), names: Names::default(), children: ChildIndex::default(), load_ms: 0. };
        let effects = build_from(&raws, "models/heroes_wip/geist/geist.vmdl", &rows, &game).unwrap();
        let t = &effects.trees;
        let groups: Vec<(&str, u32, usize, bool)> = t.groups.iter().map(|g| (g.key.as_str(), g.order, g.trees, g.note.is_some())).collect();
        assert_eq!(
            groups,
            [
                ("ability:ability_bomb", 101, 1, false),
                ("ability:ability_wisp", 102, 1, false),
                ("ability:ability_far", 103, 0, true),
                (UNLINKED_GROUP, 900, 1, true),
            ]
        );
        assert_eq!(t.groups[2].name, "Far");
        assert!(t.groups[3].folded, "with the definitions read, the other files are folded");
        assert_eq!(t.groups[3].note.as_deref(), Some(unlinked_note().as_str()));
        assert_eq!(serde_json::to_value(&t.groups[3]).unwrap()["folded"], true);
        assert_eq!(t.groups[2].note.as_deref(), Some(empty_ability_note(&t.hero_name).as_str()));
        let order: Vec<(&str, &str)> = t.trees.iter().map(|t| (t.group.as_str(), t.grouped_by)).collect();
        assert_eq!(order, [("ability:ability_bomb", "definition"), ("ability:ability_wisp", "definition"), (UNLINKED_GROUP, "none")]);
    }

    #[test]
    fn a_parts_own_size_on_top_of_the_effect_size_previews_what_the_file_holds() {
        // A 3D model whose file sets its size (an f32 literal) and its opacity (a plain f32):
        // the effect-wide multipliers reach those, the part's own settings multiply on top.
        const TOP: &str = "particles/abilities/ghost/ghost_statue.vpcf_c";
        let model = obj(vec![
            ("_class", s("C_OP_RenderModels")),
            ("m_flRadiusScale", obj(vec![("m_nType", s("PF_TYPE_LITERAL")), ("m_flLiteralValue", v(Kind::F32(0.7)))])),
            ("m_flAlphaScale", v(Kind::F32(0.3))),
        ]);
        let raws = BTreeMap::from([(TOP.to_owned(), resource(&system(&[200, 40, 40, 255], &[], Some(model))))]);
        let row = CatalogRow { path: TOP.into(), target: TOP.into(), label: String::new(), association: Association { association: "hero_directory".into(), shared: false }, supported: true, reason: String::new(), original_sha256: None, metadata: None, wav_supported: false, sound: None };
        let game = GameIndex { key: String::new(), definitions: None, names: Names::default(), children: ChildIndex::from_edges([]), load_ms: 0. };
        let effects = build_from(&raws, "models/heroes_wip/geist/geist.vmdl", &[row], &game).unwrap();
        let settings = colors::PartSettings { size: Some(3.), opacity: Some(0.7), ..Default::default() };
        let edit = TreeEdit { size: 1.5, opacity: 1.3, parts: BTreeMap::from([(TOP.to_owned(), settings)]), ..Default::default() };
        let preview = effects.preview(TOP, &edit, None, None).unwrap();
        let written = edit.edit_part(&raws[TOP], TOP, false).unwrap();
        let back = effect_edit::document(&written.bytes).unwrap().root;
        let controls = &preview.parts[0].controls;
        assert_eq!(controls.len(), 2);
        for c in controls {
            assert!(c.applied && c.status == "changes", "{c:?}");
            let found = effect_edit::control_sites(&back).into_iter().find(|s| s.location.to_string() == c.location).unwrap();
            assert_eq!(found.before, c.after, "{}", c.location);
        }
        // 0.7 × 1.5 (effect-wide), then × 3 (the part's own).
        let expected = 0.7 * 1.5 * 3.;
        assert!((controls[0].after.unwrap() - expected).abs() < 1e-6, "{:?}", controls[0].after);
    }

    #[test]
    fn light_brightness_is_offered_noted_and_previewed_as_the_file_holds_it() {
        const TOP: &str = "particles/abilities/ghost/ghost_glow.vpcf_c";
        const KID: &str = "particles/abilities/ghost/ghost_glow_light.vpcf_c";
        let lit = |n: f64| obj(vec![("m_nType", s("PF_TYPE_LITERAL")), ("m_flLiteralValue", v(Kind::F32(n as f32)))]);
        // The top: one changeable light and one without a brightness value.
        let mut top = system(&[200, 40, 40, 255], &[KID], None);
        if let Kind::Object(rows) = &mut top.kind {
            rows.push((
                "m_Renderers".into(),
                arr(vec![
                    obj(vec![("_class", s("C_OP_RenderOmni2Light")), ("m_flBrightnessLumens", lit(75.))]),
                    obj(vec![("_class", s("C_OP_RenderStandardLight"))]),
                ]),
            ));
        }
        // The child: a light whose brightness the game sets.
        let kid = system(&[200, 40, 40, 255], &[], Some(obj(vec![("_class", s("C_OP_RenderStandardLight")), ("m_flIntensity", obj(vec![("m_nType", s("PF_TYPE_PARTICLE_AGE"))]))])));
        let raws = BTreeMap::from([(TOP.to_owned(), resource(&top)), (KID.to_owned(), resource(&kid))]);
        let row = |p: &str| CatalogRow { path: p.into(), target: p.into(), label: String::new(), association: Association { association: "hero_directory".into(), shared: false }, supported: true, reason: String::new(), original_sha256: None, metadata: None, wav_supported: false, sound: None };
        let game = GameIndex { key: String::new(), definitions: None, names: Names::default(), children: ChildIndex::from_edges([]), load_ms: 0. };
        let effects = build_from(&raws, "models/heroes_wip/geist/geist.vmdl", &[row(TOP), row(KID)], &game).unwrap();
        let tree = effects.trees.trees.iter().find(|t| t.target == TOP).unwrap();
        let (top_part, kid_part) = (&tree.parts[0], tree.parts.iter().find(|p| p.path == KID).unwrap());
        assert!(top_part.controls.light.offered && !top_part.controls.light.player_shows);
        assert_eq!(top_part.controls.light.note, format!("1 of its 2 lights stays as it is: {}", effect_edit::LIGHT_ABSENT));
        assert_eq!(top_part.kind.note.as_deref(), Some(LIGHT_NOTE));
        assert!(!kid_part.controls.light.offered);
        assert_eq!(kid_part.controls.light.reason, effect_edit::LIGHT_GAME_SET);
        assert_eq!(kid_part.kind.note.as_deref(), Some(format!("{LIGHT_NOTE} Its brightness stays as it is: {}.", effect_edit::LIGHT_GAME_SET).as_str()));
        let json = serde_json::to_value(&top_part.controls).unwrap();
        assert!(json["tint"].get("note").is_none(), "an empty note is not sent");
        // The page's copies of the reason it compares and of the range.
        let types = include_str!("../../../../src/extras/effectTypes.ts");
        assert!(types.contains(&format!("export const LIGHT_NONE = '{}';", effect_edit::LIGHT_NONE)));
        assert!(types.contains(&format!("export const LIGHT_MAX = {};", effect_edit::LIGHT_MAX)));
        // The preview reports the number the written file holds.
        let settings = colors::PartSettings { light: Some(50.), ..Default::default() };
        let edit = TreeEdit { parts: BTreeMap::from([(TOP.to_owned(), settings.clone()), (KID.to_owned(), settings)]), ..Default::default() };
        let preview = effects.preview(TOP, &edit, None, None).unwrap();
        let back = effect_edit::document(&edit.edit_part(&raws[TOP], TOP, false).unwrap().bytes).unwrap().root;
        let top_preview = preview.parts.iter().find(|p| p.path == TOP).unwrap();
        let statuses: Vec<(&str, bool)> = top_preview.controls.iter().map(|c| (c.status, c.applied)).collect();
        assert_eq!(statuses, [("changes", true), ("not_offered", false)]);
        let site = effect_edit::light_sites(&back).into_iter().next().unwrap();
        assert_eq!(top_preview.controls[0].after, site.before);
        assert_eq!(site.before, Some(3750.));
        assert_eq!(top_preview.controls[0].location, site.location.to_string());
        let kid_preview = preview.parts.iter().find(|p| p.path == KID).unwrap();
        assert_eq!(kid_preview.controls[0].status, "game_input");
        assert_eq!(preview.changed.part_settings, 1);
        assert_eq!(edit.edit_part(&raws[KID], KID, false).unwrap().report.changed().len(), 0);
        // Left out: nothing is written and the preview says so.
        let out = TreeEdit { excluded: vec![TOP.into()], ..edit.clone() };
        let left = effects.preview(TOP, &out, None, None).unwrap();
        assert!(left.parts[0].controls.iter().all(|c| c.status == "left_out" && !c.applied));
        assert_eq!(out.edit_part(&raws[TOP], TOP, false).unwrap().bytes, TreeEdit::default().edit_part(&raws[TOP], TOP, false).unwrap().bytes);
    }

    #[test]
    fn trees_tops_cycles_strips_and_the_preview_matching_the_written_file() {
        // An archive-free build: feed `analyse` and the tree logic through a fake catalog.
        let docs = files();
        let raws: BTreeMap<String, Vec<u8>> = docs.iter().map(|(k, d)| (k.to_string(), resource(d))).collect();
        let rows: Vec<CatalogRow> = docs
            .keys()
            .map(|p| CatalogRow {
                path: p.to_string(),
                target: p.to_string(),
                label: String::new(),
                // The smoke is a linked file outside the hero's folders.
                association: if p.ends_with("_smoke.vpcf_c") {
                    Association { association: "resource_reference".into(), shared: true }
                } else {
                    Association { association: "hero_directory".into(), shared: false }
                },
                supported: true,
                reason: String::new(),
                original_sha256: None,
                metadata: None,
                wav_supported: false,
                sound: None,
            })
            .collect();
        let game = GameIndex { key: String::new(), definitions: None, names: Names::default(), children: ChildIndex::from_edges([("particles/abilities/other/other.vpcf_c".to_owned(), "particles/abilities/ghost/ghost_bomb_core.vpcf_c".to_owned())]), load_ms: 0. };
        let effects = build_from(&raws, "models/heroes_wip/geist/geist.vmdl", &rows, &game).unwrap();
        let t = &effects.trees;
        let targets: Vec<&str> = t.trees.iter().map(|t| t.target.as_str()).collect();
        // Tops: the bomb and the wisp; the loop's two files name each other, so the first becomes a top. Used elsewhere comes before nothing names it.
        assert_eq!(targets, ["particles/abilities/ghost/ghost_bomb.vpcf_c", "particles/abilities/ghost/ghost_wisp.vpcf_c", "particles/abilities/ghost/ghost_loop_a.vpcf_c"]);
        assert_eq!(t.counts.files_in_several_trees, 1, "the core is in the bomb and the wisp");
        let bomb = &t.trees[0];
        // No definitions: another game effect names the core, so the bomb and the wisp are
        // used elsewhere; nothing names the loop.
        assert_eq!(bomb.group, ELSEWHERE_GROUP);
        assert_eq!(t.trees[2].group, UNLINKED_GROUP);
        assert_eq!(t.trees[1].group, ELSEWHERE_GROUP);
        assert_eq!((t.counts.linked, t.counts.unlinked), (0, 3));
        let groups: Vec<(&str, u32, bool)> = t.groups.iter().map(|g| (g.key.as_str(), g.order, g.folded)).collect();
        // Without the definitions nothing is known about the other files: not folded, no note.
        assert_eq!(groups, [(ELSEWHERE_GROUP, 800, false), (UNLINKED_GROUP, 900, false)]);
        assert_eq!(t.groups[1].name, format!("Other files in {}'s folders", t.hero_name));
        assert_eq!(t.groups[1].note, None);
        let json = serde_json::to_value(&t.groups).unwrap();
        assert!(json[1].get("folded").is_none() && json[1].get("note").is_none(), "an open group writes no folded key");
        assert_eq!(bomb.name, "Bomb", "names in these groups stay the effect's own");
        assert_eq!(bomb.label, "Bomb");
        assert_eq!(bomb.tags, ["hero", "ability"]);
        let labels: Vec<&str> = bomb.parts.iter().map(|p| p.label.as_str()).collect();
        assert_eq!(labels, ["Bomb", "Core", "Smoke"]);
        assert_eq!(bomb.parts[1].in_trees, ["particles/abilities/ghost/ghost_wisp.vpcf_c"]);
        assert_eq!(bomb.parts[1].parent.as_deref(), Some("particles/abilities/ghost/ghost_bomb.vpcf_c"));
        assert_eq!(bomb.parts[0].blend, "none");
        assert!(!bomb.parts[0].visible, "the top draws nothing; its white has no weight");
        // The additive teal core wins; the dark smoke is black and never the main color.
        assert_eq!(bomb.main.as_ref().unwrap().rgb, [20, 200, 180]);
        let note = bomb.also_used_by.as_ref().unwrap();
        assert_eq!(note.effects[0].path, "particles/abilities/other/other.vpcf_c");
        assert_eq!(note.note, "Also used by: Other (effect)");
        // The wisp's model takes the particle color as its tint: it follows. The
        // effect-wide sliders add no setting to a model; the part's own Size and
        // Opacity do. It has a color of its own, so no tint.
        let wisp = &t.trees[1];
        assert!(wisp.parts[0].does_not_follow.is_empty() && wisp.parts[0].may_not_follow.is_empty());
        assert_eq!((wisp.parts[0].kind.kind, wisp.parts[0].kind.label.as_str()), ("model", "3D model"));
        let offers = &wisp.parts[0].controls;
        assert!(offers.hide.offered && offers.size.offered && offers.opacity.offered && !offers.opacity.player_shows);
        assert_eq!(offers.tint.reason, effect_edit::TINT_HAS_COLORS);
        assert_eq!(bomb.parts[1].controls.size.reason, "only 3D model parts have a size of their own");
        assert_eq!(bomb.parts[1].kind.label, "Glow sprites");
        assert!(!bomb.parts[0].controls.hide.offered, "the top draws nothing");
        assert_eq!(wisp.multipliers[&Multiplier::Size].no_setting, 1);
        assert_eq!(wisp.multipliers[&Multiplier::Glow].no_setting, 1, "models have no overbright");
        assert_eq!(wisp.parts[0].multipliers[0].status, "no_setting");
        assert_eq!(wisp.parts[0].multipliers[0].reason, "this kind of part (a model) has no size setting to change");
        let size = &bomb.multipliers[&Multiplier::Size];
        // Counts describe the default edit, which leaves the shared smoke out.
        assert_eq!((size.changes, size.adds, size.parts_reached, size.visible_parts), (1, 0, 1, 1));
        assert!(bomb.parts[2].outside_hero && !bomb.parts[1].outside_hero);
        assert_eq!(bomb.outside_parts, 1);
        // Sprites without an opacity setting get it added.
        let opacity = &bomb.multipliers[&Multiplier::Opacity];
        assert_eq!((opacity.changes, opacity.adds, opacity.parts_reached), (0, 1, 1));
        assert_eq!(bomb.parts[1].multipliers.iter().find(|s| s.multiplier == Multiplier::Opacity).unwrap().status, "adds");
        assert_eq!(bomb.parts[1].size_limit, Some(0.1), "the core's sprite caps its size");
        assert_eq!(bomb.parts[2].size_limit, None);
        assert!(bomb.parts.iter().all(|p| p.may_not_follow.is_empty()), "an inherit without a field is the radius");

        // The preview of an edit equals what the edited file holds.
        let edit = TreeEdit {
            hue: 100.,
            saturation: 0.8,
            excluded: vec!["particles/abilities/ghost/ghost_bomb_smoke.vpcf_c".into()],
            colors: vec![colors::ColorReplacement { from: [255, 255, 255], to: [250, 0, 250], near: false }],
            size: 1.5,
            glow: 2.,
            opacity: 0.5,
            ..Default::default()
        };
        let preview = effects.preview(&bomb.target, &edit, None, None).unwrap();
        assert!(preview.parts[2].excluded);
        for part in &preview.parts {
            let written = edit.edit_part(&raws[&part.path], &part.path, false).unwrap();
            let back = effect_edit::document(&written.bytes).unwrap().root;
            for strip in &part.strips {
                for stop in strip.stops.iter().filter(|s| s.slot.is_some()) {
                    let at = effect_edit::colors(&back).colors.into_iter().find(|c| Some(c.location.to_string()) == stop.slot).unwrap();
                    assert_eq!(at.rgb, stop.after, "{} {:?}", part.path, stop.slot);
                }
            }
            for site in part.multipliers.iter().filter(|s| s.applied) {
                // An added setting is a literal: its number sits one step deeper.
                let found = effect_edit::multiplier_sites(&back).into_iter().find(|s| s.location.to_string().starts_with(&site.location)).unwrap();
                assert_eq!(found.before, site.after, "{}", site.location);
            }
        }
        // The white top became the replacement's color; the excluded smoke kept its own.
        assert_eq!(preview.parts[0].strips[0].stops[0].after, [250, 0, 250]);
        assert_eq!(preview.parts[0].strips[0].stops[0].rule, Rule::Replaced);
        assert_eq!(preview.parts[2].strips[0].stops[0].rule, Rule::Excluded);
        // Size, glow and opacity reach the core; the excluded smoke keeps everything.
        assert_eq!(preview.changed.multiplied, 3);
        assert_eq!(preview.parts[2].left_out, Some(colors::LeftOut::User));
        // Unknown parts and trees are refused; a pick maps the main color.
        assert!(effects.preview(&bomb.target, &TreeEdit { excluded: vec!["particles/x.vpcf_c".into()], ..Default::default() }, None, None).is_err());
        assert!(effects.preview("particles/none.vpcf_c", &TreeEdit::default(), None, None).unwrap_err().to_string().contains(TREE_UNKNOWN));
        let picked = effects.preview(&bomb.target, &TreeEdit::default(), Some(&PickRequest { color: [125, 77, 255], main: None }), None).unwrap();
        let pick = picked.pick.as_ref().unwrap();
        assert!(pick.exact);
        assert_eq!(picked.main.as_ref().unwrap().after, pick.result);
        assert!(pick.result.iter().zip([125u8, 77, 255]).all(|(a, b)| a.abs_diff(b) <= 1));
        // Identity: nothing changes.
        // The shared smoke is left out by default, and changes once chosen.
        let turn = TreeEdit { hue: 120., size: 2., ..Default::default() };
        let default = effects.preview(&bomb.target, &turn, None, None).unwrap();
        assert_eq!(default.parts[2].left_out, Some(colors::LeftOut::Shared));
        assert!(default.parts[2].strips.iter().flat_map(|s| &s.stops).all(|s| s.rule == Rule::Excluded));
        assert!(default.parts[2].multipliers.iter().all(|m| !m.applied));
        assert_eq!(default.parts[2].multipliers[0].reason, SHARED_LEFT_OUT_REASON);
        let smoke = bomb.parts[2].path.clone();
        let chosen = effects.preview(&bomb.target, &TreeEdit { shared: vec![smoke.clone(), "particles/not_a_part.vpcf_c".into()], ..turn.clone() }, None, None).unwrap();
        assert_eq!(chosen.parts[2].left_out, None, "a chosen shared part changes; an unknown path is ignored");
        assert!(chosen.parts[2].multipliers.iter().any(|m| m.applied));
        // A written left-out part keeps the game's values; a chosen one follows the preview.
        let kept = turn.edit_part(&raws[&smoke], &smoke, true).unwrap();
        assert!(kept.report.changed().is_empty());
        // A main color the user chose is the answer's main color, without a pick.
        let chosen = effects.preview(&bomb.target, &TreeEdit { hue: 120., ..Default::default() }, None, Some([200, 40, 40])).unwrap();
        assert_eq!(chosen.main.as_ref().unwrap().before, [200, 40, 40]);
        assert_eq!(chosen.main.as_ref().unwrap().after, effect_edit::shift_color([200, 40, 40], &TreeEdit { hue: 120., ..Default::default() }.shift()));
        let same = effects.preview(&bomb.target, &TreeEdit::default(), None, None).unwrap();
        assert_eq!((same.changed.colors, same.changed.multiplied, same.changed.not_multiplied), (0, 0, 0));

        // A part's own settings: the preview equals the written file, a hidden
        // part alone is a change, and a left-out part gets none of them.
        let wisp_path = wisp.parts[0].path.clone();
        let own = |path: &str, s: colors::PartSettings| TreeEdit { parts: BTreeMap::from([(path.to_uppercase(), s)]), ..Default::default() };
        let model = own(&wisp_path, colors::PartSettings { size: Some(3.), opacity: Some(0.2), hidden: true, tint: Some([1, 2, 3]), light: None });
        let shown = effects.preview(&wisp.target, &model, None, None).unwrap();
        let p = &shown.parts[0];
        assert!(p.hidden);
        assert_eq!(shown.changed.hidden, 1);
        let written = model.edit_part(&raws[&wisp_path], &wisp_path, false).unwrap();
        let back = effect_edit::document(&written.bytes).unwrap().root;
        for c in p.controls.iter().filter(|c| c.applied && c.control != effect_edit::Control::Hide && c.control != effect_edit::Control::Tint) {
            let found = effect_edit::control_sites(&back).into_iter().find(|s| s.location.to_string().starts_with(&c.location)).unwrap();
            assert_eq!(found.before, c.after, "{}", c.location);
        }
        let renderer = &back.get("m_Renderers").unwrap().array().unwrap()[0];
        assert_eq!(renderer.get(effect_edit::DISABLE_KEY).unwrap().kind, Kind::Bool(true));
        let tint = p.controls.iter().find(|c| c.control == effect_edit::Control::Tint).unwrap();
        assert!(!tint.applied && tint.status == "not_offered", "the wisp has a color of its own");
        assert_eq!(written.report.controlled.iter().filter(|c| c.left.is_empty()).count(), 3);
        let hidden_only = own(&bomb.parts[1].path, colors::PartSettings { hidden: true, ..Default::default() });
        assert!(!hidden_only.is_identity());
        assert!(!hidden_only.edit_part(&raws[&bomb.parts[1].path], &bomb.parts[1].path, false).unwrap().report.changed().is_empty());
        let left = TreeEdit { excluded: vec![bomb.parts[1].path.clone()], ..hidden_only.clone() };
        assert!(left.edit_part(&raws[&bomb.parts[1].path], &bomb.parts[1].path, false).unwrap().report.changed().is_empty());
        let left_preview = effects.preview(&bomb.target, &left, None, None).unwrap();
        assert!(!left_preview.parts[1].hidden && left_preview.parts[1].controls[0].status == "left_out");
        // Model settings on a sprite part change nothing and say why.
        let wrong = own(&bomb.parts[1].path, colors::PartSettings { size: Some(2.), ..Default::default() });
        assert!(wrong.edit_part(&raws[&bomb.parts[1].path], &bomb.parts[1].path, false).unwrap().report.changed().is_empty());
        assert_eq!(effects.preview(&bomb.target, &wrong, None, None).unwrap().parts[1].controls[0].status, "not_offered");
    }
}

/// `HeroEffects::build` from raw files instead of an archive (tests).
#[cfg(test)]
fn build_from(raws: &BTreeMap<String, Vec<u8>>, model: &str, rows: &[CatalogRow], game: &GameIndex) -> Result<HeroEffects> {
    let dir = tempfile::tempdir().map_err(fail)?;
    let stage = dir.path().join("stage");
    for (path, raw) in raws {
        let target = stage.join(path);
        std::fs::create_dir_all(target.parent().unwrap()).map_err(fail)?;
        std::fs::write(&target, raw).map_err(fail)?;
    }
    let list: Vec<String> = raws.keys().cloned().collect();
    let vpk = dir.path().join("pak02_dir.vpk");
    deadlock_vpk::write_vpk(&stage, &list, &vpk, None, 2, &mut || Ok(())).map_err(fail)?;
    let archive = Vpk::open(&vpk).map_err(fail)?;
    HeroEffects::build(&archive, model, rows, game, &CancellationToken::new())
}
