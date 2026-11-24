//! The game's own value of single fields of a model's `DATA` block written
//! into a rebuilt ability object (the kinds `ability_objects::Switches` names
//! `written_…`).
//!
//! A rebuilt object is compiled from the decompiled game object with only its
//! render meshes changed. A few details of its `DATA` block come out of the
//! compiler differently from the game's file (`ability_objects::ModelFacts`:
//! `rest_sign_flips`, `rest_scale`, `key_values_reordered`,
//! `unnamed_body_choices`). For each such fact the game's own value of exactly
//! the field it names is put into the rebuilt `DATA`, which is encoded again
//! (`binary::encode`) into the slot it had (`container::rebuild_ordinals`);
//! every other block keeps its bytes.
//!
//! What has to hold first, for every fact (`write`):
//! - the rebuilt file with the game's value in that field passes the whole
//!   read-back (`ability_objects::carried_check`), so the rebuilt file differed
//!   from the game's in that field only;
//! - within the field, only the difference the fact names: a resting turn only
//!   in sign, component for component, on the bones the sign rule predicts; a
//!   resting size only on the bones whose size is not 1, where the rebuild
//!   writes 1; the game data only in the order of the same entries; the body
//!   groups only in the choice names the older form leaves out, with the same
//!   groups in the same order;
//! - re-encoding changes no other row of `DATA`.
use crate::ability_objects::{self as objects, Shells, flatten, same_value};
use deadlock_resource::{Kind, Value, binary, container};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

/// A field of the model's `DATA` block a kind writes back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WrittenKind {
    /// `m_modelSkeleton.m_boneRotParent` (resting turns).
    RestSigns,
    /// `m_modelSkeleton.m_boneScaleParent` (resting sizes).
    RestScales,
    /// `m_modelInfo.m_keyValueText` (the game data), in the game's order.
    KeyOrder,
    /// `m_meshGroups` (the body groups' names).
    BodyGroupNames,
    /// `m_modelInfo.m_keyValueText` (the game data) with the game's own break pieces
    /// (`break_list`, `break_command_list`): the compiler writes the list from the
    /// decompiled `BreakPieceList` without the pieces' names and bind-pose flag, and
    /// writes no break commands at all.
    BreakPieces,
    /// `m_modelInfo.m_keyValueText` (the game data) with the game's own bone constraints
    /// (`BoneConstraintList`): the decompile writes them as angles and offsets to six decimals,
    /// and the compiler computes the numbers again (`same_constraints` says exactly what may differ).
    BoneConstraints,
    /// `m_modelSkeleton.m_boneRotParent` (resting turns) of a build that marks only some bones to be
    /// kept (`Switches::parts_kept_in_levels`): a bone no compiled mesh binds and the vmdl leaves
    /// unmarked comes out as the quaternion of its Euler angles, so with the other sign exactly where
    /// `object_parts::round_trip_flips` says so; only those bones may differ, and only in sign.
    UnboundRestSigns,
}
impl WrittenKind {
    /// The field's path in `DATA`.
    pub fn path(self) -> &'static [&'static str] {
        match self {
            WrittenKind::RestSigns | WrittenKind::UnboundRestSigns => &["m_modelSkeleton", "m_boneRotParent"],
            WrittenKind::RestScales => &["m_modelSkeleton", "m_boneScaleParent"],
            WrittenKind::KeyOrder => &["m_modelInfo", "m_keyValueText"],
            WrittenKind::BodyGroupNames => &["m_meshGroups"],
            WrittenKind::BreakPieces => &["m_modelInfo", "m_keyValueText"],
            WrittenKind::BoneConstraints => &["m_modelInfo", "m_keyValueText"],
        }
    }
    pub fn field(self) -> String {
        self.path().join(".")
    }
}

/// One field written back: which, the SHA-256 of the value as written into the
/// rebuilt model, and of the game's own value it was taken from (each the digest
/// of its flattened rows, `flatten`, one `path = value` line each). The two differ
/// only where the rebuilt skeleton lists the bones in another order than the game's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Written {
    pub kind: WrittenKind,
    pub field: String,
    pub sha256: String,
    pub game_sha256: String,
    /// For BoneConstraints: how many numbers of the rebuild's bone constraints differed from
    /// the game's (each within the rounding same_constraints allows) and the largest difference;
    /// for cloth made from a proxy mesh also every number accepted past that rounding, within
    /// `CLOTH_PROXY_CONSTRAINT_ROUNDING`, with the largest of those differences.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rounding: Option<String>,
}

fn sha(b: &[u8]) -> String {
    Sha256::digest(b).iter().map(|x| format!("{x:02x}")).collect()
}
fn get<'a>(root: &'a Value, path: &[&str]) -> Option<&'a Value> {
    path.iter().try_fold(root, |v, k| v.get(k))
}
fn get_mut<'a>(root: &'a mut Value, path: &[&str]) -> Option<&'a mut Value> {
    let mut at = root;
    for key in path {
        let Kind::Object(rows) = &mut at.kind else { return None };
        at = rows.iter_mut().find(|(k, _)| k == key).map(|(_, v)| v)?;
    }
    Some(at)
}
/// The digest of a value as `Written::sha256` records it.
pub fn digest(value: &Value) -> String {
    let rows: String = flatten(value).into_iter().map(|(k, v)| format!("{k} = {v}\n")).collect();
    sha(rows.as_bytes())
}
/// The model's `DATA` block: its ordinal and decoded document.
fn data(raw: &[u8]) -> std::result::Result<(usize, binary::Document), String> {
    let layout = container::layout(raw).map_err(|e| e.to_string())?;
    let (ordinal, block) = layout.blocks.iter().enumerate().find(|(_, b)| b.name == "DATA").ok_or("no DATA block")?;
    let payload = raw.get(block.offset..block.offset + block.size).ok_or("the DATA block is outside the file")?;
    Ok((ordinal, binary::decode(payload).map_err(|e| e.to_string())?))
}
/// The digest of `kind`'s field in a compiled model (`None` when it has none).
pub fn field_digest(raw: &[u8], kind: WrittenKind) -> Option<String> {
    let (_, doc) = data(raw).ok()?;
    get(&doc.root, kind.path()).map(digest)
}
fn names(root: &Value) -> Vec<String> {
    get(root, &["m_modelSkeleton", "m_boneName"]).and_then(|v| v.array().ok()).unwrap_or(&[]).iter().map(|n| n.string().unwrap_or("?").to_owned()).collect()
}
fn numbers(v: &Value) -> Vec<f64> {
    v.array().map(|a| a.iter().filter_map(|n| n.number().ok()).collect()).unwrap_or_default()
}
fn row(v: &Value) -> String {
    format!("{:?}", numbers(v))
}
/// Puts the game's value of the field of `kind` into `ours` (decoded `DATA`
/// roots), after checking that within the field only what the fact names differs.
#[cfg(test)]
fn copy_field(kind: WrittenKind, kinds: &BTreeSet<WrittenKind>, game: &Value, ours: &mut Value) -> std::result::Result<(), String> {
    copy_field_bound(kind, kinds, &BTreeSet::new(), false, game, ours)
}
/// `copy_field` with the bones some compiled mesh of the rebuilt model binds (`bound`, what
/// `WrittenKind::UnboundRestSigns` predicts by), and whether the object's cloth is made from a
/// cloth proxy mesh (`cloth_proxy`, what `CLOTH_PROXY_CONSTRAINT_ROUNDING` applies to).
fn copy_field_bound(kind: WrittenKind, kinds: &BTreeSet<WrittenKind>, bound: &BTreeSet<String>, cloth_proxy: bool, game: &Value, ours: &mut Value) -> std::result::Result<(), String> {
    let field = kind.field();
    // One field, written once: the resting turns with both sign rules together.
    let asked = kind;
    let kind = if kind == WrittenKind::UnboundRestSigns && kinds.contains(&WrittenKind::RestSigns) { return Ok(()) } else if kind == WrittenKind::UnboundRestSigns { WrittenKind::RestSigns } else { kind };
    let theirs = get(game, kind.path()).ok_or_else(|| format!("the game's model has no {field}"))?.clone();
    let mine = get(ours, kind.path()).ok_or_else(|| format!("the rebuilt model has no {field}"))?.clone();
    match kind {
        WrittenKind::RestSigns | WrittenKind::UnboundRestSigns | WrittenKind::RestScales => {
            // Bone by bone, by name (the rebuild may list the bones in another order).
            let (game_names, our_names) = (names(game), names(ours));
            let (a, b) = (theirs.array().map_err(|e| e.to_string())?, mine.array().map_err(|e| e.to_string())?);
            if a.len() != game_names.len() || b.len() != our_names.len() || game_names.len() != our_names.len() {
                return Err(format!("{field}: {} / {} rows for {} / {} bones", a.len(), b.len(), game_names.len(), our_names.len()));
            }
            let mut out = b.to_vec();
            let mut changed = 0;
            for (i, name) in our_names.iter().enumerate() {
                let j = game_names.iter().position(|n| n == name).ok_or_else(|| format!("{field}: the game's model has no bone {name}"))?;
                if kind == WrittenKind::RestSigns {
                    let (g, o) = (numbers(&a[j]), numbers(&b[i]));
                    if g.len() != 4 || o.len() != 4 {
                        return Err(format!("{field}: bone {name} is not a rotation"));
                    }
                    let same = (0..4).all(|k| same_value(&g[k].to_string(), &o[k].to_string()));
                    let flipped = (0..4).all(|k| same_value(&g[k].to_string(), &(-o[k]).to_string()));
                    let g4 = [g[0], g[1], g[2], g[3]];
                    let predicted = (asked == WrittenKind::RestSigns || kinds.contains(&WrittenKind::RestSigns)) && objects::rest_sign_flips(g4) || (asked == WrittenKind::UnboundRestSigns || kinds.contains(&WrittenKind::UnboundRestSigns)) && !bound.contains(name) && crate::object_parts::round_trip_flips(g4);
                    if predicted {
                        // A bone the sign rule predicts: the rebuild's turn is the game's,
                        // or the game's with the other sign, component for component.
                        if !same && !flipped {
                            return Err(format!("{field}: bone {name} differs in more than its sign: the game's {} / ours {}", row(&a[j]), row(&b[i])));
                        }
                        changed += usize::from(flipped && !same);
                    } else if !same {
                        return Err(format!("{field}: bone {name}, which the sign rule does not predict, differs: the game's {} / ours {}", row(&a[j]), row(&b[i])));
                    }
                    // Every bone takes the game's own value (by name), so the field says what the game's says.
                    out[i] = a[j].clone();
                } else {
                    let (g, o) = (a[j].number().map_err(|e| e.to_string())?, b[i].number().map_err(|e| e.to_string())?);
                    if !same_value(&g.to_string(), "1") {
                        // A resting size the read-back does not count as 1: the rebuild writes 1.
                        if !same_value(&o.to_string(), "1") && !same_value(&o.to_string(), &g.to_string()) {
                            return Err(format!("{field}: bone {name}: the game's size {g}, ours {o} (1 expected)"));
                        }
                        changed += usize::from(!same_value(&o.to_string(), &g.to_string()));
                    } else if !same_value(&g.to_string(), &o.to_string()) {
                        return Err(format!("{field}: bone {name}: the game's size {g}, ours {o}"));
                    }
                    out[i] = a[j].clone();
                }
            }
            // How many bones the rebuild wrote otherwise (none is fine: the game's value is written all the same).
            let _ = changed;
            *get_mut(ours, kind.path()).ok_or("no field")? = Value { annotation: mine.annotation, kind: Kind::Array(out) };
        }
        // Written once, by the break-piece kind or the bone-constraint kind, when an object has several.
        WrittenKind::KeyOrder if kinds.contains(&WrittenKind::BreakPieces) || kinds.contains(&WrittenKind::BoneConstraints) => {}
        WrittenKind::BreakPieces if kinds.contains(&WrittenKind::BoneConstraints) => {}
        WrittenKind::BoneConstraints => {
            let (g, o) = (theirs.string().map_err(|e| e.to_string())?, mine.string().map_err(|e| e.to_string())?);
            constraint_rounding_for(kinds, g, o, cloth_proxy).map_err(|e| format!("{field}: {e}"))?;
            *get_mut(ours, kind.path()).ok_or("no field")? = theirs;
        }
        WrittenKind::BreakPieces => {
            let (g, o) = (theirs.string().map_err(|e| e.to_string())?, mine.string().map_err(|e| e.to_string())?);
            let (Some(eg), Some(eo)) = (top_entries(g), top_entries(o)) else { return Err(format!("{field}: the game data could not be split into its entries")) };
            let piece = |e: &String| BREAK_KEYS.iter().any(|k| e.trim_start().starts_with(&format!("{k} ")));
            let (mut rest_g, mut rest_o): (Vec<&String>, Vec<&String>) = (eg.iter().filter(|e| !piece(e)).collect(), eo.iter().filter(|e| !piece(e)).collect());
            if kinds.contains(&WrittenKind::KeyOrder) {
                rest_g.sort();
                rest_o.sort();
            }
            if rest_g != rest_o {
                return Err(format!("{field}: the game data holds other entries or values than the game's besides its break pieces"));
            }
            // The rebuild's break list names the game's piece models, in the game's order.
            let models = |list: &[String], key: &str| -> Vec<String> { list.iter().filter(|e| e.trim_start().starts_with(&format!("{key} "))).flat_map(|e| resources(e)).collect() };
            if models(&eg, "break_list") != models(&eo, "break_list") {
                return Err(format!("{field}: the rebuild's break pieces are not the game's pieces: {:?} / {:?}", models(&eg, "break_list"), models(&eo, "break_list")));
            }
            *get_mut(ours, kind.path()).ok_or("no field")? = theirs;
        }
        WrittenKind::KeyOrder => {
            let (g, o) = (theirs.string().map_err(|e| e.to_string())?, mine.string().map_err(|e| e.to_string())?);
            let (eg, eo) = (top_entries(g), top_entries(o));
            if eg.is_none() || eo.is_none() {
                return Err(format!("{field}: the game data could not be split into its entries"));
            }
            let (mut eg, mut eo) = (eg.unwrap_or_default(), eo.unwrap_or_default());
            eg.sort();
            eo.sort();
            if eg != eo {
                return Err(format!("{field}: the game data holds other entries or values than the game's, not only another order"));
            }
            *get_mut(ours, kind.path()).ok_or("no field")? = theirs;
        }
        WrittenKind::BodyGroupNames => {
            let (a, b) = (theirs.array().map_err(|e| e.to_string())?, mine.array().map_err(|e| e.to_string())?);
            if a.len() != b.len() {
                return Err(format!("{field}: {} / {} entries", a.len(), b.len()));
            }
            for (x, y) in a.iter().zip(b) {
                let (x, y) = (x.string().map_err(|e| e.to_string())?, y.string().map_err(|e| e.to_string())?);
                // The same group and choice number; only the choice name the older form leaves out differs.
                let base = |s: &str| s.split_once("_#&").map_or(s, |(b, _)| b).to_owned();
                if base(x) != base(y) || x.contains("_#&") && x != y {
                    return Err(format!("{field}: {x:?} / {y:?} differ in more than the choice name"));
                }
            }
            *get_mut(ours, kind.path()).ok_or("no field")? = theirs;
        }
    }
    Ok(())
}
/// How the rebuild's game data `ours` differs from the game's `game` when the object has
/// bone constraints (`WrittenKind::BoneConstraints`, with `kinds` the object's other written
/// kinds): every entry the same text but the bone constraints, which may differ only as
/// `same_constraints` allows; break pieces (`BreakPieces`) left out, the rebuild's naming
/// the game's piece models in the game's order; the entries compared in sorted order when
/// the game data is in another order too (`KeyOrder`). Returns how many numbers differ and
/// the largest difference.
pub fn constraint_rounding(kinds: &BTreeSet<WrittenKind>, game: &str, ours: &str) -> std::result::Result<(usize, f64), String> {
    constraint_rounding_for(kinds, game, ours, false).map(|r| (r.count, r.largest))
}
/// What the bone-constraint check found: the numbers within the rounding `same_constraints`
/// allows and the largest of their differences, and the numbers past that rounding accepted
/// for an object whose cloth is made from a cloth proxy mesh (`CLOTH_PROXY_CONSTRAINT_ROUNDING`;
/// always empty for every other object), each as "game / rebuild", with the largest difference.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Rounding {
    pub count: usize,
    pub largest: f64,
    pub beyond: Vec<String>,
    pub beyond_largest: f64,
}
/// `constraint_rounding`, with `cloth_proxy` true only for an object whose cloth is made from a
/// cloth proxy mesh (`KeptKind::ClothProxy`): then a number past the ordinary rounding still
/// passes when it differs from the game's by at most `CLOTH_PROXY_CONSTRAINT_ROUNDING`, and is
/// listed in `Rounding::beyond`. Everything else is compared exactly as for every other object.
pub fn constraint_rounding_for(kinds: &BTreeSet<WrittenKind>, game: &str, ours: &str, cloth_proxy: bool) -> std::result::Result<Rounding, String> {
    let (Some(eg), Some(eo)) = (top_entries(game), top_entries(ours)) else { return Err("the game data could not be split into its entries".into()) };
    let pieces = kinds.contains(&WrittenKind::BreakPieces);
    let piece = |e: &String| pieces && BREAK_KEYS.iter().any(|k| e.trim_start().starts_with(&format!("{k} ")));
    if pieces {
        let models = |list: &[String]| -> Vec<String> { list.iter().filter(|e| e.trim_start().starts_with("break_list ")).flat_map(|e| resources(e)).collect() };
        if models(&eg) != models(&eo) {
            return Err("the rebuild's break pieces are not the game's pieces".into());
        }
    }
    let (mut rest_g, mut rest_o): (Vec<&String>, Vec<&String>) = (eg.iter().filter(|e| !piece(e)).collect(), eo.iter().filter(|e| !piece(e)).collect());
    if kinds.contains(&WrittenKind::KeyOrder) {
        rest_g.sort();
        rest_o.sort();
    }
    if rest_g.len() != rest_o.len() {
        return Err(format!("{} / {} entries in the game data", rest_g.len(), rest_o.len()));
    }
    let mut out = Rounding::default();
    for (x, y) in rest_g.iter().zip(&rest_o) {
        if x == y {
            continue;
        }
        if !(is_constraints(x) && is_constraints(y)) {
            return Err(format!("the game data holds other entries or values than the game's besides its bone constraints ({:?} / {:?})", x.lines().next().unwrap_or(""), y.lines().next().unwrap_or("")));
        }
        let r = same_constraints_for(x, y, cloth_proxy)?;
        out.count += r.count;
        out.largest = out.largest.max(r.largest);
        out.beyond.extend(r.beyond);
        out.beyond_largest = out.beyond_largest.max(r.beyond_largest);
    }
    Ok(out)
}
/// The largest difference a bone-constraint number may show (`same_constraints`): the decompile
/// writes a constraint's turns as angles and its offsets to six decimals, and the compiler turns
/// them back into the stored numbers; 4e-8 was the largest seen, so 1e-7 caps every allowance.
pub const CONSTRAINT_ROUNDING: f64 = 1e-7;
/// For an object whose cloth is made from a cloth proxy mesh only (`KeptKind::ClothProxy`,
/// switch `kept_cloth_proxy`; never any other object): a bone-constraint number that fails the
/// ordinary rounding (`same_constraints`) still passes when the rebuild's value and the game's
/// differ by at most this, in absolute value. The compiler computes such a model's constraints
/// again with float noise of up to 1.5e-7 (1 to 16 float32 steps). Each number accepted this way
/// is listed (`Rounding::beyond`, the receipt's note); the game's own text is written back all the same.
pub const CLOTH_PROXY_CONSTRAINT_ROUNDING: f64 = 2e-7;
/// The key values entry that holds the bone constraints.
pub const CONSTRAINT_KEY: &str = "BoneConstraintList";
fn is_constraints(entry: &str) -> bool {
    entry.trim_start().starts_with(&format!("{CONSTRAINT_KEY} "))
}
/// Whether the rebuild's bone-constraint entry `ours` is the game's `game` with its numbers
/// computed again from the decompile's six-decimal angles and offsets, and in nothing else: the same text token for token (classes,
/// names, bone hashes, brackets, every word), and each number token that differs is a
/// number with a fraction in both whose value in `ours` is the game's value rounded
/// to the fraction digits `ours` writes (half a unit of its last digit, plus one part in
/// 1e9 for the float the game's text was printed from), and never by more than
/// `CONSTRAINT_ROUNDING`. Integers (bone hashes, axes) must
/// be equal.
pub fn same_constraints(game: &str, ours: &str) -> std::result::Result<(usize, f64), String> {
    same_constraints_for(game, ours, false).map(|r| (r.count, r.largest))
}
/// `same_constraints`, with `cloth_proxy` (see `constraint_rounding_for`): a number token that
/// passes every other part of the rule but differs by more than its rounding is accepted, and
/// listed, when the difference is at most `CLOTH_PROXY_CONSTRAINT_ROUNDING`.
pub fn same_constraints_for(game: &str, ours: &str, cloth_proxy: bool) -> std::result::Result<Rounding, String> {
    let (a, b): (Vec<&str>, Vec<&str>) = (game.split_whitespace().collect(), ours.split_whitespace().collect());
    if a.len() != b.len() {
        return Err(format!("the bone constraints hold {} / {} words", a.len(), b.len()));
    }
    let mut out = Rounding::default();
    for (x, y) in a.iter().zip(&b) {
        if x == y {
            continue;
        }
        let (nx, ny) = (x.trim_end_matches(','), y.trim_end_matches(','));
        if x.ends_with(',') != y.ends_with(',') || x.len() - nx.len() > 1 || y.len() - ny.len() > 1 {
            return Err(format!("the bone constraints differ in {x:?} / {y:?}"));
        }
        let (Ok(gx), Ok(gy)) = (nx.parse::<f64>(), ny.parse::<f64>()) else { return Err(format!("the bone constraints differ in {x:?} / {y:?}")) };
        let digits = |s: &str| s.split_once('.').map(|(_, f)| f.trim_start_matches(|c: char| !c.is_ascii_digit()).chars().take_while(char::is_ascii_digit).count());
        let (Some(_), Some(dy)) = (digits(nx), digits(ny)) else { return Err(format!("the bone constraints differ in the whole number {x:?} / {y:?}")) };
        if nx.contains(['e', 'E']) || ny.contains(['e', 'E']) {
            return Err(format!("the bone constraints differ in {x:?} / {y:?}"));
        }
        let allowed = (0.5 * 10f64.powi(-(dy as i32)) + 1e-9 * gx.abs().max(1.)).min(CONSTRAINT_ROUNDING);
        let difference = (gx - gy).abs();
        if difference > allowed {
            if !cloth_proxy {
                return Err(format!("the bone constraints differ in {x:?} / {y:?} by more than rounding to {dy} digits"));
            }
            if difference > CLOTH_PROXY_CONSTRAINT_ROUNDING {
                return Err(format!("the bone constraints differ in {x:?} / {y:?} by more than rounding to {dy} digits and by more than the {CLOTH_PROXY_CONSTRAINT_ROUNDING:e} allowed for cloth made from a proxy mesh"));
            }
            out.beyond.push(format!("{nx} / {ny}"));
            out.beyond_largest = out.beyond_largest.max(difference);
            continue;
        }
        out.count += 1;
        out.largest = out.largest.max(difference);
    }
    Ok(out)
}
/// The key values entries that hold break pieces.
pub const BREAK_KEYS: [&str; 2] = ["break_list", "break_command_list"];
/// The resources a key values text names (`resource:"…"`), in order.
pub fn resources(text: &str) -> Vec<String> {
    text.split("resource:\"").skip(1).filter_map(|s| s.split('"').next()).map(str::to_owned).collect()
}
/// The top-level entries of a model's key values text (one tab in, `name = …`
/// up to the next such line or the closing brace), each as its text; `None`
/// when the text has no such layout.
pub fn top_entries(text: &str) -> Option<Vec<String>> {
    let lines: Vec<&str> = text.lines().collect();
    let open = lines.iter().position(|l| l.trim() == "{")?;
    let close = lines.iter().rposition(|l| l.trim() == "}")?;
    if close <= open {
        return None;
    }
    let mut out: Vec<String> = vec![];
    for line in &lines[open + 1..close] {
        let top = line.starts_with('\t') && !line.starts_with("\t\t") && line.contains(" = ");
        if top {
            out.push(String::new());
        }
        let last = out.last_mut()?;
        last.push_str(line);
        last.push('\n');
    }
    Some(out)
}

/// The rebuilt file `ours` with the game's own values of the fields of `kinds`
/// written into its `DATA`, and what was written; an error naming the problem
/// when the rebuilt file differs from the game's in anything else.
pub fn write(game: &[u8], ours: &[u8], kinds: &BTreeSet<WrittenKind>, shells: Option<Shells>) -> std::result::Result<(Vec<u8>, Vec<Written>), String> {
    write_for(game, ours, kinds, shells, false)
}
/// `write`, with `cloth_proxy` true only for an object whose cloth is made from a cloth proxy
/// mesh (`KeptKind::ClothProxy`): its bone constraints may then differ before the write by up to
/// `CLOTH_PROXY_CONSTRAINT_ROUNDING` (`constraint_rounding_for`), each such number named in the note.
pub fn write_for(game: &[u8], ours: &[u8], kinds: &BTreeSet<WrittenKind>, shells: Option<Shells>, cloth_proxy: bool) -> std::result::Result<(Vec<u8>, Vec<Written>), String> {
    let (_, game_doc) = data(game)?;
    let (ordinal, mut doc) = data(ours)?;
    let before = doc.root.clone();
    // What the bone constraints differed in before the game's own were written (the receipt's note).
    let text = |root: &Value| get(root, WrittenKind::BoneConstraints.path()).and_then(|t| t.string().ok()).map(str::to_owned).unwrap_or_default();
    let rounding = if kinds.contains(&WrittenKind::BoneConstraints) {
        let r = constraint_rounding_for(kinds, &text(&game_doc.root), &text(&doc.root), cloth_proxy).map_err(|e| format!("{}: {e}", WrittenKind::BoneConstraints.field()))?;
        let (n, largest) = (r.count, r.largest);
        let mut note = format!("{n} numbers of the bone constraints computed again by the compiler, the largest difference {largest:e}");
        if !r.beyond.is_empty() {
            note.push_str(&format!(
                "; {} numbers past that rounding accepted for cloth made from a proxy mesh (at most {CLOTH_PROXY_CONSTRAINT_ROUNDING:e}), the largest difference {:e}: {}",
                r.beyond.len(),
                r.beyond_largest,
                r.beyond.join(", ")
            ));
        }
        Some(note)
    } else {
        None
    };
    // The bones the rebuilt model's meshes bind (`WrittenKind::UnboundRestSigns` predicts by them).
    let bound: BTreeSet<String> = if kinds.contains(&WrittenKind::UnboundRestSigns) {
        container::read_blocks(ours, "MDAT")
            .map_err(|e| e.to_string())?
            .iter()
            .flat_map(|m| m.root.get("m_skeleton").and_then(|s| s.get("m_bones")).and_then(|b| b.array().ok()).unwrap_or(&[]).iter().filter_map(|b| b.get("m_boneName").and_then(|n| n.string().ok()).map(str::to_owned)).collect::<Vec<_>>())
            .collect()
    } else {
        BTreeSet::new()
    };
    for kind in kinds {
        copy_field_bound(*kind, kinds, &bound, cloth_proxy, &game_doc.root, &mut doc.root)?;
    }
    let fields: Vec<String> = kinds.iter().map(|k| format!(".{}", k.field())).collect();
    // Every model the game's break pieces name is in the rebuilt reference table, as in the game's.
    if kinds.contains(&WrittenKind::BreakPieces) {
        let named = get(&game_doc.root, WrittenKind::BreakPieces.path()).and_then(|t| t.string().ok()).map(resources).unwrap_or_default();
        let table = |raw: &[u8]| -> BTreeSet<String> { deadlock_vpk::resource_references(raw).unwrap_or_default().into_iter().map(|r| r.to_ascii_lowercase().trim_end_matches("_c").to_owned()).collect() };
        let (theirs, mine) = (table(game), table(ours));
        if let Some(missing) = named.iter().find(|r| { let r = r.to_ascii_lowercase(); theirs.contains(&r) && !mine.contains(&r) }) {
            return Err(format!("the game's break pieces name {missing}, which the rebuilt reference table lacks"));
        }
        if let Some(missing) = named.iter().find(|r| !theirs.contains(&r.to_ascii_lowercase())) {
            return Err(format!("the game's break pieces name {missing}, which the game's own reference table lacks"));
        }
    }
    let bytes = binary::encode(&doc.root, doc.format_guid).map_err(|e| e.to_string())?;
    let out = container::rebuild_ordinals(ours, &BTreeMap::from([(ordinal, bytes)])).map_err(|e| e.to_string())?;
    // The file as written, read back: its DATA is the rebuilt one, value for value (kinds and
    // annotations included), but for the written fields; every other block keeps its bytes.
    let (_, written_doc) = data(&out)?;
    let mut restored = written_doc.root.clone();
    for kind in kinds {
        let mine = get(&before, kind.path()).ok_or("no field")?.clone();
        *get_mut(&mut restored, kind.path()).ok_or("the written file lost a field")? = mine;
    }
    if restored != before {
        let (x, y) = (flatten(&before), flatten(&restored));
        let key = x.keys().chain(y.keys()).find(|k| x.get(*k) != y.get(*k)).cloned().unwrap_or_else(|| "a value's kind or annotation".into());
        return Err(format!("writing back changed {key} outside {}", fields.join(", ")));
    }
    let blocks = |raw: &[u8]| -> std::result::Result<Vec<(String, Vec<u8>)>, String> {
        let layout = container::layout(raw).map_err(|e| e.to_string())?;
        Ok(layout.blocks.iter().map(|b| (b.name.clone(), raw[b.offset..b.offset + b.size].to_vec())).collect())
    };
    let (a, b) = (blocks(ours)?, blocks(&out)?);
    if a.len() != b.len() || a.iter().zip(&b).enumerate().any(|(i, (x, y))| x.0 != y.0 || i != ordinal && x.1 != y.1) {
        return Err("writing back changed a block other than DATA".into());
    }
    // The rebuilt file with the game's values passes the whole read-back: it differed only there.
    let check = objects::carried_check(game, &out, shells).map_err(|e| e.to_string())?;
    if !check.differs.is_empty() {
        return Err(format!("the rebuilt model differs from the game's in more than {}: {}", fields.join(", "), check.differs.iter().take(6).cloned().collect::<Vec<_>>().join("; ")));
    }
    let written = kinds
        .iter()
        .map(|kind| Written { kind: *kind, field: kind.field(), sha256: get(&doc.root, kind.path()).map(digest).unwrap_or_default(), game_sha256: get(&game_doc.root, kind.path()).map(digest).unwrap_or_default(), rounding: if *kind == WrittenKind::BoneConstraints { rounding.clone() } else { None } })
        .collect();
    Ok((out, written))
}
/// What is no longer as the build wrote it (the install check): each written
/// field must hold the value the receipt recorded, which must be the game's own.
pub fn same_as_written(game: &[u8], ours: &[u8], written: &[Written]) -> Vec<String> {
    let mut out = vec![];
    for w in written {
        let (now, theirs) = (field_digest(ours, w.kind), field_digest(game, w.kind));
        if now.as_deref() != Some(w.sha256.as_str()) {
            out.push(format!("{} is not the value the build wrote", w.field));
        } else if theirs.as_deref() != Some(w.game_sha256.as_str()) {
            out.push(format!("{} is no longer the game's own value", w.field));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_game_data_splits_into_its_top_level_entries() {
        let text = "<!-- kv3 -->\n{\n\tA = \n\t{\n\t\tx = 1\n\t}\n\tB = [ 1, 2 ]\n}\n";
        assert_eq!(top_entries(text), Some(vec!["\tA = \n\t{\n\t\tx = 1\n\t}\n".to_owned(), "\tB = [ 1, 2 ]\n".to_owned()]));
        let reordered = "<!-- kv3 -->\n{\n\tB = [ 1, 2 ]\n\tA = \n\t{\n\t\tx = 1\n\t}\n}\n";
        let (mut a, mut b) = (top_entries(text).unwrap(), top_entries(reordered).unwrap());
        a.sort();
        b.sort();
        assert_eq!(a, b, "the same entries in another order");
        assert_eq!(top_entries("no braces"), None);
    }

    fn s(t: &str) -> Value {
        Value::new(Kind::String(t.into()))
    }
    fn f(n: f64) -> Value {
        Value::new(Kind::F64(n))
    }
    fn obj(rows: Vec<(&str, Value)>) -> Value {
        Value::new(Kind::Object(rows.into_iter().map(|(k, v)| (k.to_owned(), v)).collect()))
    }
    fn arr(items: Vec<Value>) -> Value {
        Value::new(Kind::Array(items))
    }
    fn skeleton(names: &[&str], rots: Vec<[f64; 4]>, scales: Vec<f64>) -> Value {
        obj(vec![(
            "m_modelSkeleton",
            obj(vec![
                ("m_boneName", arr(names.iter().map(|n| s(n)).collect())),
                ("m_boneRotParent", arr(rots.into_iter().map(|q| arr(q.into_iter().map(f).collect())).collect())),
                ("m_boneScaleParent", arr(scales.into_iter().map(f).collect())),
            ]),
        )])
    }

    #[test]
    fn a_resting_turn_is_written_only_where_the_sign_rule_predicts_a_flip() {
        // Bone b: w < 0 with a positive trace, so the rebuild writes -q.
        let q = [0.1, 0.2, 0.3, -0.927_361_849_549_570_4];
        assert!(objects::rest_sign_flips(q));
        let game = skeleton(&["a", "b"], vec![[0., 0., 0., 1.], q], vec![1., 1.]);
        let mut ours = skeleton(&["a", "b"], vec![[0., 0., 0., 1.], q.map(|c| -c)], vec![1., 1.]);
        copy_field(WrittenKind::RestSigns, &BTreeSet::new(), &game, &mut ours).unwrap();
        assert_eq!(flatten(&ours), flatten(&game));
        // Any difference beyond the sign is refused, and so is one on a bone the rule does not predict.
        let mut other = skeleton(&["a", "b"], vec![[0., 0., 0., 1.], [-0.1, -0.2, -0.31, 0.927]], vec![1., 1.]);
        assert!(copy_field(WrittenKind::RestSigns, &BTreeSet::new(), &game, &mut other).unwrap_err().contains("more than its sign"));
        let mut moved = skeleton(&["a", "b"], vec![[0., 0., 0.1, 0.995], q.map(|c| -c)], vec![1., 1.]);
        assert!(copy_field(WrittenKind::RestSigns, &BTreeSet::new(), &game, &mut moved).unwrap_err().contains("does not predict"));
        // The bones by name, whatever their order.
        let mut swapped = skeleton(&["b", "a"], vec![q.map(|c| -c), [0., 0., 0., 1.]], vec![1., 1.]);
        copy_field(WrittenKind::RestSigns, &BTreeSet::new(), &game, &mut swapped).unwrap();
        assert_eq!(numbers(&get(&swapped, WrittenKind::RestSigns.path()).unwrap().array().unwrap()[0]), q.to_vec());
    }

    #[test]
    fn an_unbound_resting_turn_is_written_only_where_its_euler_round_trip_flips() {
        // Bone b: a turn whose Euler angles give it back as -q (the negation of a near half turn
        // about x written as angles).
        let q = crate::object_parts::angle_quaternion([0.000_01, -1.3, -179.99]).map(|c| -c);
        assert!(crate::object_parts::round_trip_flips(q) && !objects::rest_sign_flips(q));
        let only = BTreeSet::from([WrittenKind::UnboundRestSigns]);
        let game = skeleton(&["a", "b"], vec![[0., 0., 0., 1.], q], vec![1., 1.]);
        let mut ours = skeleton(&["a", "b"], vec![[0., 0., 0., 1.], q.map(|c| -c)], vec![1., 1.]);
        copy_field_bound(WrittenKind::UnboundRestSigns, &only, &BTreeSet::new(), false, &game, &mut ours).unwrap();
        assert_eq!(flatten(&ours), flatten(&game));
        // A bone a rebuilt mesh binds is not predicted: its sign must already be the game's.
        let mut bound = skeleton(&["a", "b"], vec![[0., 0., 0., 1.], q.map(|c| -c)], vec![1., 1.]);
        assert!(copy_field_bound(WrittenKind::UnboundRestSigns, &only, &BTreeSet::from(["b".to_owned()]), false, &game, &mut bound).unwrap_err().contains("does not predict"));
        let mut same = skeleton(&["a", "b"], vec![[0., 0., 0., 1.], q], vec![1., 1.]);
        copy_field_bound(WrittenKind::UnboundRestSigns, &only, &BTreeSet::from(["b".to_owned()]), false, &game, &mut same).unwrap();
        // A bone whose round trip keeps its sign is not predicted, and more than a sign is refused.
        let keeps = [0.1, 0.2, 0.3, 0.927_361_849_549_570_4];
        assert!(!crate::object_parts::round_trip_flips(keeps));
        let game2 = skeleton(&["a", "b"], vec![[0., 0., 0., 1.], keeps], vec![1., 1.]);
        let mut flipped = skeleton(&["a", "b"], vec![[0., 0., 0., 1.], keeps.map(|c| -c)], vec![1., 1.]);
        assert!(copy_field_bound(WrittenKind::UnboundRestSigns, &only, &BTreeSet::new(), false, &game2, &mut flipped).unwrap_err().contains("does not predict"));
        let mut other = skeleton(&["a", "b"], vec![[0., 0., 0., 1.], [-0.9999, 0.0113, 0.01, 0.]], vec![1., 1.]);
        assert!(copy_field_bound(WrittenKind::UnboundRestSigns, &only, &BTreeSet::new(), false, &game, &mut other).unwrap_err().contains("more than its sign"));
        // The resting-turn rule of `RestSigns` alone does not predict this bone.
        let mut plain = skeleton(&["a", "b"], vec![[0., 0., 0., 1.], q.map(|c| -c)], vec![1., 1.]);
        assert!(copy_field(WrittenKind::RestSigns, &BTreeSet::new(), &game, &mut plain).unwrap_err().contains("does not predict"));
    }

    #[test]
    fn a_resting_size_is_written_only_where_the_rebuild_wrote_1() {
        let game = skeleton(&["a", "b"], vec![[0., 0., 0., 1.]; 2], vec![1., 0.9998]);
        let mut ours = skeleton(&["a", "b"], vec![[0., 0., 0., 1.]; 2], vec![1., 1.]);
        copy_field(WrittenKind::RestScales, &BTreeSet::new(), &game, &mut ours).unwrap();
        assert_eq!(flatten(&ours), flatten(&game));
        let mut other = skeleton(&["a", "b"], vec![[0., 0., 0., 1.]; 2], vec![0.9, 1.]);
        assert!(copy_field(WrittenKind::RestScales, &BTreeSet::new(), &game, &mut other).is_err());

    }

    #[test]
    fn body_group_names_differ_only_in_the_choice_name() {
        let game = obj(vec![("m_meshGroups", arr(vec![s("default_@0"), s("default_@1")]))]);
        let mut ours = obj(vec![("m_meshGroups", arr(vec![s("default_@0_#&unnamed_1"), s("default_@1_#&unnamed_2")]))]);
        copy_field(WrittenKind::BodyGroupNames, &BTreeSet::new(), &game, &mut ours).unwrap();
        assert_eq!(flatten(&ours), flatten(&game));
        let mut other = obj(vec![("m_meshGroups", arr(vec![s("default_@1_#&a"), s("default_@0_#&b")]))]);
        assert!(copy_field(WrittenKind::BodyGroupNames, &BTreeSet::new(), &game, &mut other).is_err());
        let mut fewer = obj(vec![("m_meshGroups", arr(vec![s("default_@0_#&a")]))]);
        assert!(copy_field(WrittenKind::BodyGroupNames, &BTreeSet::new(), &game, &mut fewer).is_err());
    }

    const CONSTRAINT: &str = "\tBoneConstraintList = \n\t[\n\t\t{\n\t\t\t_class = \"CTiltTwistConstraint\"\n\t\t\tm_qOffset = [ -0.71108705, -0.0027953044, 0.00008665223, 0.70309854 ]\n\t\t\tm_nBoneHash = 3760074545\n\t\t\tm_flWeight = 0.5\n\t\t},\n\t]\n";
    fn keys(entries: &[&str]) -> String {
        format!("<!-- kv3 -->\n{{\n{}}}\n", entries.concat())
    }

    #[test]
    fn bone_constraints_may_differ_only_in_the_rounding_of_their_numbers() {
        let none = BTreeSet::new();
        let game = keys(&["\tA = 1\n", CONSTRAINT]);
        // The same numbers computed again, within their rounding: accepted, counted.
        let rounded = keys(&["\tA = 1\n", &CONSTRAINT.replace("-0.71108705", "-0.711087").replace("0.00008665223", "0.0000867")]);
        let (n, largest) = constraint_rounding(&none, &game, &rounded).unwrap();
        assert_eq!(n, 2);
        assert!(largest < 1e-7, "{largest}");
        // Identical: nothing to count.
        assert_eq!(constraint_rounding(&none, &game, &game).unwrap().0, 0);
        // A number off by more than its last written digit, a changed bone hash or class,
        // another word, or another entry: refused.
        for wrong in [
            CONSTRAINT.replace("-0.71108705", "-0.711085"),
            // A coarse number: within half its own last digit, but past the cap.
            CONSTRAINT.replace("-0.71108705", "-0.7111"),
            CONSTRAINT.replace("3760074545", "3760074546"),
            CONSTRAINT.replace("CTiltTwistConstraint", "CAimConstraint"),
            CONSTRAINT.replace("m_flWeight = 0.5", "m_flWeight = 0.5 0"),
            CONSTRAINT.replace("0.5\n", "0.6\n"),
        ] {
            assert!(constraint_rounding(&none, &game, &keys(&["\tA = 1\n", &wrong])).is_err(), "{wrong}");
        }
        assert!(constraint_rounding(&none, &game, &keys(&["\tA = 2\n", CONSTRAINT])).is_err());
        // Another order of the entries only together with the order kind.
        let reordered = keys(&[CONSTRAINT, "\tA = 1\n"]);
        assert!(constraint_rounding(&none, &game, &reordered).is_err());
        assert_eq!(constraint_rounding(&BTreeSet::from([WrittenKind::KeyOrder]), &game, &reordered).unwrap().0, 0);
        // Written once, as the game's own text, whichever kinds share the field.
        let both = BTreeSet::from([WrittenKind::KeyOrder, WrittenKind::BoneConstraints]);
        let field = |t: &str| obj(vec![("m_modelInfo", obj(vec![("m_keyValueText", s(t))]))]);
        let mut ours = field(&keys(&[&CONSTRAINT.replace("-0.71108705", "-0.711087"), "\tA = 1\n"]));
        for kind in &both {
            copy_field(*kind, &both, &field(&game), &mut ours).unwrap();
        }
        assert_eq!(flatten(&ours), flatten(&field(&game)));
    }

    #[test]
    fn cloth_from_a_proxy_mesh_lets_through_constraint_numbers_up_to_its_own_bound_only() {
        let none = BTreeSet::new();
        let game = keys(&["\tA = 1\n", CONSTRAINT]);
        // 1.5e-7 off a number written to 11 decimals: past the ordinary rounding.
        let near_constraint = CONSTRAINT.replace("0.00008665223", "0.00008680223");
        let near = keys(&["\tA = 1\n", &near_constraint]);
        let far = keys(&["\tA = 1\n", &CONSTRAINT.replace("0.00008665223", "0.00008686223")]);
        // Any other object: refused exactly as before.
        assert!(constraint_rounding(&none, &game, &near).unwrap_err().contains("by more than rounding to 11 digits"));
        assert_eq!(constraint_rounding_for(&none, &game, &near, false), Err(constraint_rounding(&none, &game, &near).unwrap_err()));
        // Cloth from a proxy mesh: accepted and listed, with the largest difference.
        let r = constraint_rounding_for(&none, &game, &near, true).unwrap();
        assert_eq!((r.count, r.beyond.clone()), (0, vec!["0.00008665223 / 0.00008680223".to_owned()]));
        assert!((r.beyond_largest - 1.5e-7).abs() < 1e-12, "{}", r.beyond_largest);
        // Past 2e-7 (2.1e-7): refused for it too.
        assert!(constraint_rounding_for(&none, &game, &far, true).unwrap_err().contains("allowed for cloth made from a proxy mesh"));
        // Anything but those numbers: refused for both.
        for wrong in [
            CONSTRAINT.replace("3760074545", "3760074546"),
            CONSTRAINT.replace("CTiltTwistConstraint", "CAimConstraint"),
            CONSTRAINT.replace("m_flWeight = 0.5", "m_flWeight = 0.5 0"),
            CONSTRAINT.replace("0.00008665223", "8.680223e-5"),
        ] {
            for proxy in [false, true] {
                assert!(constraint_rounding_for(&none, &game, &keys(&["\tA = 1\n", &wrong]), proxy).is_err(), "{wrong} {proxy}");
            }
        }
        for proxy in [false, true] {
            assert!(constraint_rounding_for(&none, &game, &keys(&["\tA = 2\n", &near_constraint]), proxy).is_err(), "another entry, {proxy}");
        }
        // The game's own text is written all the same.
        let only = BTreeSet::from([WrittenKind::BoneConstraints]);
        let field = |t: &str| obj(vec![("m_modelInfo", obj(vec![("m_keyValueText", s(t))]))]);
        let mut ours = field(&near);
        assert!(copy_field_bound(WrittenKind::BoneConstraints, &only, &BTreeSet::new(), false, &field(&game), &mut ours.clone()).is_err());
        copy_field_bound(WrittenKind::BoneConstraints, &only, &BTreeSet::new(), true, &field(&game), &mut ours).unwrap();
        assert_eq!(flatten(&ours), flatten(&field(&game)));
        assert_eq!(CLOTH_PROXY_CONSTRAINT_ROUNDING, 2e-7);
    }

    #[test]
    fn game_data_is_written_only_when_the_same_entries_come_in_another_order() {
        let game = obj(vec![("m_modelInfo", obj(vec![("m_keyValueText", s("<!-- kv3 -->\n{\n\tB = 2\n\tA = 1\n}\n"))]))]);
        let mut ours = obj(vec![("m_modelInfo", obj(vec![("m_keyValueText", s("<!-- kv3 -->\n{\n\tA = 1\n\tB = 2\n}\n"))]))]);
        copy_field(WrittenKind::KeyOrder, &BTreeSet::new(), &game, &mut ours).unwrap();
        assert_eq!(flatten(&ours), flatten(&game));
        let mut other = obj(vec![("m_modelInfo", obj(vec![("m_keyValueText", s("<!-- kv3 -->\n{\n\tA = 1\n\tB = 3\n}\n"))]))]);
        assert!(copy_field(WrittenKind::KeyOrder, &BTreeSet::new(), &game, &mut other).is_err());
    }
}
