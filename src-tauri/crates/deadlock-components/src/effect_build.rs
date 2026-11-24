//! Effect edits at build (`Customization.effect_edits`, Extras → Effects): each
//! enabled row's effect tree is read from the game, `TreeEditExt::edit_part`
//! edits each of its parts, and only the parts whose values change are written
//! at their game paths.
//!
//! The allow-list: a row's target must be a top of the hero's effect trees
//! (`effect_trees::tree_files` over the hero's Effects catalog), and only that
//! tree's parts are written. Parts the game also uses elsewhere are edited
//! like any other (the page shows "Also used by"), so no shared choice is
//! asked. Rules, each reported in plain words (`PartOutcome`):
//! * a part left out of the edit (`excluded`) and a part the edit does not
//!   change are not written;
//! * a part outside the hero's own folders (`effect_trees::outside_hero`:
//!   not in the hero's Effects catalog, or in it as a linked `shared` file)
//!   is left out unless the edit lists it in `shared` (`shared_left_out`);
//! * a part with a replacement file of its own (Extras → Effects file rows):
//!   the user's file is used for that part, the edit for the tree's other parts;
//! * a part of two edited trees: when both edits give the same bytes nothing
//!   is decided; otherwise the row that comes first in the project is used.
//!
//! Verification: every written file passes `resources::validate_effect` against
//! the game's file; before installing, the snapshot builds the trees again
//! from the current game and edits each part again, which must give the same
//! bytes (a game update or a changed engine stops with `EFFECT_CHANGED`).
use crate::effect_colors::{LeftOut, TreeEditExt};
use crate::effect_trees::{self, TreeFiles};
use crate::resources::{self, ResourceKind};
use crate::{CancellationToken, Result, fail, read, read_file, sha};
use deadlock_core::{safety, text::casefold};
use deadlock_project::effect_edit::{EffectEditRow, TreeEdit};
use deadlock_vpk::Vpk;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub const EFFECT_EDIT_RECEIPT_SCHEMA: u32 = 1;
/// A saved row whose effect the game no longer has as an effect of this hero.
pub const EFFECT_NOT_IN_GAME: &str = "An edited effect is no longer one of this hero's effects in the game files";
/// The game's files of an edited effect differ from the ones the mod was built from.
pub const EFFECT_CHANGED: &str = "The game's files of an edited effect changed since this mod was built. Build again";

/// What happened to one part of an edited tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// Edited and packaged.
    Written,
    /// The edit changes none of its values: the game's file stays.
    Unchanged,
    /// Left out of the edit: the game's file stays.
    Excluded,
    /// Outside the hero's own folders (shared with other heroes) and not
    /// chosen in the edit's `shared` list: the game's file stays.
    SharedLeftOut,
    /// The user's replacement file is used for it.
    UserFile,
    /// Another edited effect that comes first in the project writes it.
    OtherEffect,
    /// Another edited effect writes the same bytes for it.
    SameAsOtherEffect,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PartOutcome {
    pub path: String,
    pub outcome: Outcome,
    /// For `other_effect` and `same_as_other_effect`: the effect (top path) whose edit is used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub follows: Option<String>,
    /// For the collision outcomes: what happens, in plain words.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditedTree {
    /// The row's target: the tree's top.
    pub target: String,
    pub edit: TreeEdit,
    pub parts: Vec<PartOutcome>,
}
/// One packaged file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditedEntry {
    /// The part's game path.
    pub target: String,
    /// The tree (row target) whose edit wrote it.
    pub tree: String,
    pub original_sha256: String,
    pub output_sha256: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EffectEditReceipt {
    pub schema: u32,
    pub policy: String,
    pub hero_path: String,
    /// The enabled rows in project order, with every part's outcome.
    pub trees: Vec<EditedTree>,
    /// The packaged files, by path.
    pub entries: Vec<EditedEntry>,
}
impl Default for EffectEditReceipt {
    fn default() -> Self {
        Self::empty("")
    }
}
impl EffectEditReceipt {
    pub fn empty(hero: &str) -> Self {
        Self { schema: EFFECT_EDIT_RECEIPT_SCHEMA, policy: crate::effect_edit::EFFECT_EDIT_POLICY.into(), hero_path: hero.into(), trees: vec![], entries: vec![] }
    }
    /// The collision notes, for the build summary.
    pub fn notes(&self) -> Vec<String> {
        self.trees.iter().flat_map(|t| t.parts.iter()).filter(|p| !p.note.is_empty()).map(|p| p.note.clone()).collect()
    }
    /// Whether the receipt was made for exactly these rows (targets and edits, in order).
    pub fn same_rows(&self, rows: &[EffectEditRow]) -> bool {
        self.trees.len() == rows.len() && self.trees.iter().zip(rows).all(|(t, r)| casefold(&t.target) == casefold(&r.target) && t.edit == r.edit)
    }
}
fn stem(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path).trim_end_matches(".vpcf_c")
}

/// The rows' edited files, written under `game`. `rows` are the enabled rows in
/// project order; `user_files` the casefolded targets of the user's own
/// replacement files. Reads nothing when `rows` is empty.
pub fn stage(archive: &Vpk, model: &str, rows: &[EffectEditRow], user_files: &BTreeSet<String>, game: &Path, cancel: &CancellationToken) -> Result<EffectEditReceipt> {
    let mut receipt = EffectEditReceipt::empty(model);
    if rows.is_empty() {
        return Ok(receipt);
    }
    let (trees, raws, outside) = trees(archive, model, cancel)?;
    let outputs = plan(Some(archive), rows, user_files, &outside, &trees, &raws, cancel)?;
    receipt.trees = outputs.0;
    for (path, (tree, bytes, original)) in outputs.1 {
        cancel.check()?;
        safety::atomic_write(&safety::inside(game, &path)?, &bytes)?;
        receipt.entries.push(EditedEntry { target: path, tree, original_sha256: sha(&original), output_sha256: sha(&bytes) });
    }
    Ok(receipt)
}

/// The hero's trees exactly as a build reads them, kept so that rows can be
/// planned again and again (the Effects page's player shows "yours" from
/// `shown`): the files come from the same `plan` a build runs.
pub struct HeroTrees {
    trees: BTreeMap<String, TreeFiles>,
    raws: BTreeMap<String, Vec<u8>>,
    outside: BTreeSet<String>,
}
impl HeroTrees {
    pub fn read(archive: &Vpk, model: &str, cancel: &CancellationToken) -> Result<Self> {
        let catalog = resources::discover(archive, model, ResourceKind::Effects, cancel)?;
        Self::from_catalog(archive, &catalog, cancel)
    }
    /// From the hero's Effects catalog already read (`resources::discover`
    /// with `ResourceKind::Effects` on the same archive, as the app keeps it
    /// after loading the character's assets).
    pub fn from_catalog(archive: &Vpk, catalog: &[resources::CatalogRow], cancel: &CancellationToken) -> Result<Self> {
        let (trees, raws, outside) = trees_of(archive, catalog, cancel)?;
        Ok(Self { trees, raws, outside })
    }
    /// What a build of `row` alone writes: its parts' outcomes and the
    /// written files (game path → bytes). `user_files`: the casefolded
    /// targets of the user's own replacement files, as `stage` takes them.
    pub fn files(&self, archive: &Vpk, row: &EffectEditRow, user_files: &BTreeSet<String>, cancel: &CancellationToken) -> Result<(EditedTree, BTreeMap<String, Vec<u8>>)> {
        let (mut trees, written) = plan(Some(archive), std::slice::from_ref(row), user_files, &self.outside, &self.trees, &self.raws, cancel)?;
        let tree = trees.pop().ok_or_else(|| fail(format!("{EFFECT_NOT_IN_GAME}: {}", row.target)))?;
        Ok((tree, written.into_iter().map(|(path, (_, bytes, _))| (path, bytes)).collect()))
    }
    /// Whether `target` is the top of one of the hero's trees.
    pub fn has(&self, target: &str) -> bool {
        self.trees.contains_key(&casefold(target))
    }
    /// What a build of `rows` packages for the parts of `target`'s tree,
    /// whichever row writes each part (the Effects page's player shows "yours"
    /// from it): `target`'s outcomes and the written files (game path → bytes)
    /// of the same `plan` a build runs over `rows`. Only the rows whose trees
    /// share a part with `target`'s are planned: the other rows cannot change
    /// what is written for its parts. `target` must be one of `rows`. The
    /// parts the user's own replacement files win (`user_files`) are not
    /// among the files: a build stages those from the user's file.
    pub fn shown(&self, archive: &Vpk, rows: &[EffectEditRow], target: &str, user_files: &BTreeSet<String>, cancel: &CancellationToken) -> Result<(EditedTree, BTreeMap<String, Vec<u8>>)> {
        shown_from(Some(archive), rows, target, user_files, &self.outside, &self.trees, &self.raws, cancel)
    }
}

/// The rows a build would stage with `target`'s edit set to `edit` (the
/// page's current edit, saved or not): the saved rows in project order, the
/// enabled ones only when extras are `active`, and `target`'s row in its
/// saved place (taken as enabled) or after the others when it is not saved,
/// where the page saves a new row.
pub fn rows_with(saved: &[EffectEditRow], active: bool, target: &str, edit: &TreeEdit) -> Vec<EffectEditRow> {
    let key = casefold(target);
    let shown = |old: Option<&EffectEditRow>| EffectEditRow { target: old.map_or_else(|| target.to_owned(), |r| r.target.clone()), enabled: true, edit: edit.clone(), main: None };
    let mut out = vec![];
    let mut placed = false;
    for row in saved {
        if casefold(&row.target) == key {
            if !placed {
                out.push(shown(Some(row)));
                placed = true;
            }
        } else if active && row.enabled {
            out.push(row.clone());
        }
    }
    if !placed {
        out.push(shown(None));
    }
    out
}

/// `HeroTrees::shown` over trees and raw files.
#[allow(clippy::too_many_arguments)]
fn shown_from(
    archive: Option<&Vpk>,
    rows: &[EffectEditRow],
    target: &str,
    user_files: &BTreeSet<String>,
    outside: &BTreeSet<String>,
    trees: &BTreeMap<String, TreeFiles>,
    raws: &BTreeMap<String, Vec<u8>>,
    cancel: &CancellationToken,
) -> Result<(EditedTree, BTreeMap<String, Vec<u8>>)> {
    let key = casefold(target);
    let tree = trees.get(&key).ok_or_else(|| fail(format!("{EFFECT_NOT_IN_GAME}: {target}")))?;
    let parts: BTreeSet<String> = tree.parts.iter().map(|p| casefold(p)).collect();
    // Rows whose tree has a part of this one (this one included), in order.
    let related: Vec<EffectEditRow> = rows
        .iter()
        .filter(|r| casefold(&r.target) == key || trees.get(&casefold(&r.target)).is_some_and(|t| t.parts.iter().any(|p| parts.contains(&casefold(p)))))
        .cloned()
        .collect();
    let at = related.iter().position(|r| casefold(&r.target) == key).ok_or_else(|| fail(format!("{EFFECT_NOT_IN_GAME}: {target}")))?;
    let (mut outcomes, written) = plan(archive, &related, user_files, outside, trees, raws, cancel)?;
    let files = written.into_iter().filter(|(path, _)| parts.contains(&casefold(path))).map(|(path, (_, bytes, _))| (path, bytes)).collect();
    Ok((outcomes.swap_remove(at), files))
}

/// The hero's trees by casefolded top, the raw files, and the casefolded parts
/// outside the hero's own folders (`effect_trees::outside_hero`).
#[allow(clippy::type_complexity)]
fn trees(archive: &Vpk, model: &str, cancel: &CancellationToken) -> Result<(BTreeMap<String, TreeFiles>, BTreeMap<String, Vec<u8>>, BTreeSet<String>)> {
    let catalog = resources::discover(archive, model, ResourceKind::Effects, cancel)?;
    trees_of(archive, &catalog, cancel)
}
/// `trees` from a catalog already read.
#[allow(clippy::type_complexity)]
fn trees_of(archive: &Vpk, catalog: &[resources::CatalogRow], cancel: &CancellationToken) -> Result<(BTreeMap<String, TreeFiles>, BTreeMap<String, Vec<u8>>, BTreeSet<String>)> {
    let (trees, raws) = effect_trees::tree_files(archive, catalog, cancel)?;
    let rows: BTreeMap<String, &resources::CatalogRow> = catalog.iter().map(|r| (casefold(&r.target), r)).collect();
    let outside = trees.iter().flat_map(|t| &t.parts).map(|p| casefold(p)).filter(|k| effect_trees::outside_hero(rows.get(k).copied())).collect();
    Ok((trees.into_iter().map(|t| (casefold(&t.top), t)).collect(), raws, outside))
}

/// Every row's outcomes, and the files to write: game path → (tree, bytes, original bytes).
#[allow(clippy::type_complexity)]
fn plan(
    archive: Option<&Vpk>,
    rows: &[EffectEditRow],
    user_files: &BTreeSet<String>,
    outside: &BTreeSet<String>,
    trees: &BTreeMap<String, TreeFiles>,
    raws: &BTreeMap<String, Vec<u8>>,
    cancel: &CancellationToken,
) -> Result<(Vec<EditedTree>, BTreeMap<String, (String, Vec<u8>, Vec<u8>)>)> {
    let mut written: BTreeMap<String, (String, Vec<u8>, Vec<u8>)> = BTreeMap::new();
    let mut by_key: BTreeMap<String, String> = BTreeMap::new();
    let mut out = vec![];
    for row in rows {
        row.edit.check()?;
        let tree = trees.get(&casefold(&row.target)).ok_or_else(|| fail(format!("{EFFECT_NOT_IN_GAME}: {}", row.target)))?;
        let mut parts = vec![];
        for part in &tree.parts {
            cancel.check()?;
            let key = casefold(part);
            // The archive's own spelling of the path.
            let path = archive.and_then(|a| a.entries.get(&key)).map(|e| e.path.clone()).unwrap_or_else(|| part.clone());
            let outcome = |outcome, follows: Option<String>, note: String| PartOutcome { path: path.clone(), outcome, follows, note };
            match row.edit.left_out(part, outside.contains(&key)) {
                Some(LeftOut::User) => {
                    parts.push(outcome(Outcome::Excluded, None, String::new()));
                    continue;
                }
                Some(LeftOut::Shared) => {
                    parts.push(outcome(
                        Outcome::SharedLeftOut,
                        None,
                        format!("{}: shared with other heroes, so it is left out of the edit of {} (choose it in Advanced to change it)", stem(&path), stem(&tree.top)),
                    ));
                    continue;
                }
                None => {}
            }
            if user_files.contains(&key) {
                parts.push(outcome(
                    Outcome::UserFile,
                    None,
                    format!("{}: your own replacement file is used; the edit of {} changes the effect's other parts", stem(&path), stem(&tree.top)),
                ));
                continue;
            }
            let raw = raws.get(&key).ok_or_else(|| fail(format!("{EFFECT_NOT_IN_GAME}: {part}")))?;
            let edited = row.edit.edit_part(raw, &path, false)?;
            if edited.report.changed().is_empty() {
                parts.push(outcome(Outcome::Unchanged, None, String::new()));
                continue;
            }
            match by_key.get(&key).and_then(|p| written.get(p)) {
                Some((first, bytes, _)) if *bytes == edited.bytes => parts.push(outcome(Outcome::SameAsOtherEffect, Some(first.clone()), String::new())),
                Some((first, _, _)) => parts.push(outcome(
                    Outcome::OtherEffect,
                    Some(first.clone()),
                    format!("{}: also part of {}, which comes first in Extras → Effects; that effect's edit is used for it", stem(&path), stem(first)),
                )),
                None => {
                    by_key.insert(key, path.clone());
                    written.insert(path.clone(), (tree.top.clone(), edited.bytes, raw.clone()));
                    parts.push(outcome(Outcome::Written, None, String::new()));
                }
            }
        }
        out.push(EditedTree { target: tree.top.clone(), edit: row.edit.clone(), parts });
    }
    Ok((out, written))
}

/// The written files: each still the receipt's bytes, still made from the
/// game's file it names, and a valid edit of it (`validate_effect`).
pub fn verify_compiled(archive: &Vpk, receipt: &EffectEditReceipt, game: &Path, cancel: &CancellationToken) -> Result<()> {
    for entry in &receipt.entries {
        cancel.check()?;
        let output = read_file(&safety::inside(game, &entry.target)?)?;
        if sha(&output) != entry.output_sha256 {
            return Err(fail(format!("An edited effect file changed after the build: {}", entry.target)));
        }
        let original = read(archive, &entry.target).map_err(|_| fail(format!("{EFFECT_CHANGED}: {}", entry.target)))?;
        if sha(&original) != entry.original_sha256 {
            return Err(fail(format!("{EFFECT_CHANGED}: {}", entry.target)));
        }
        resources::validate_effect(&original, &output)?;
    }
    Ok(())
}

/// The receipt against the current game: the same trees and outcomes, and
/// every packaged file the same bytes when its edit is made again.
pub fn verify_entries(archive: &Vpk, receipt: &EffectEditReceipt, user_files: &BTreeSet<String>, cancel: &CancellationToken) -> Result<()> {
    if receipt.trees.is_empty() && receipt.entries.is_empty() {
        return Ok(());
    }
    let rows: Vec<EffectEditRow> = receipt.trees.iter().map(|t| EffectEditRow { target: t.target.clone(), enabled: true, edit: t.edit.clone(), main: None }).collect();
    let (trees, raws, outside) = trees(archive, &receipt.hero_path, cancel)?;
    let (outcomes, written) = plan(Some(archive), &rows, user_files, &outside, &trees, &raws, cancel).map_err(|e| {
        if e.to_string().contains(EFFECT_NOT_IN_GAME) { e } else { fail(format!("{EFFECT_CHANGED}: {e}")) }
    })?;
    let again: Vec<EditedEntry> = written
        .into_iter()
        .map(|(path, (tree, bytes, original))| EditedEntry { target: path, tree, original_sha256: sha(&original), output_sha256: sha(&bytes) })
        .collect();
    if outcomes != receipt.trees || again != receipt.entries {
        return Err(fail(format!("{EFFECT_CHANGED}.")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadlock_project::effect_edit::ColorReplacement;
    use deadlock_resource::{Annotation, Kind, Value, binary, container};

    fn v(kind: Kind) -> Value {
        Value::new(kind)
    }
    fn s(t: &str) -> Value {
        v(Kind::String(t.into()))
    }
    fn obj(rows: Vec<(&str, Value)>) -> Value {
        v(Kind::Object(rows.into_iter().map(|(k, v)| (k.to_owned(), v)).collect()))
    }
    fn color(c: &[i64]) -> Value {
        v(Kind::Array(c.iter().map(|n| v(Kind::I64(*n))).collect()))
    }
    fn system(constant: &[i64], kids: &[&str]) -> Vec<u8> {
        let root = obj(vec![
            ("_class", s("CParticleSystemDefinition")),
            ("m_ConstantColor", color(constant)),
            ("m_Renderers", v(Kind::Array(vec![obj(vec![("_class", s("C_OP_RenderSprites"))])]))),
            (
                "m_Children",
                v(Kind::Array(
                    kids.iter()
                        .map(|k| {
                            let mut p = s(k);
                            p.annotation = Annotation::Resource;
                            obj(vec![("m_ChildRef", p)])
                        })
                        .collect(),
                )),
            ),
        ]);
        let base = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/particle.bin"));
        let layout = container::layout(base).unwrap();
        let block = container::unique(&layout.blocks, "DATA").unwrap();
        let doc = binary::decode(container::payload(base, block).unwrap()).unwrap();
        container::rebuild(base, &BTreeMap::from([("DATA".to_owned(), binary::encode(&root, doc.format_guid).unwrap())])).unwrap()
    }
    const A: &str = "particles/a.vpcf_c";
    const B: &str = "particles/b.vpcf_c";
    const SHARED: &str = "particles/shared.vpcf_c";
    const GREY: &str = "particles/grey.vpcf_c";

    /// Two trees, A (shared, grey) and B (shared), from raw files only.
    fn world() -> (BTreeMap<String, TreeFiles>, BTreeMap<String, Vec<u8>>) {
        let raws = BTreeMap::from([
            (A.to_owned(), system(&[200, 40, 40, 255], &["particles/shared.vpcf", "particles/grey.vpcf"])),
            (B.to_owned(), system(&[40, 40, 200, 255], &["particles/shared.vpcf"])),
            (SHARED.to_owned(), system(&[40, 200, 40, 255], &[])),
            (GREY.to_owned(), system(&[90, 90, 90, 255], &[])),
        ]);
        let trees = BTreeMap::from([
            (A.to_owned(), TreeFiles { top: A.into(), parts: vec![A.into(), SHARED.into(), GREY.into()] }),
            (B.to_owned(), TreeFiles { top: B.into(), parts: vec![B.into(), SHARED.into()] }),
        ]);
        (trees, raws)
    }
    fn row(target: &str, edit: TreeEdit) -> EffectEditRow {
        EffectEditRow { target: target.into(), enabled: true, edit, main: None }
    }
    #[test]
    fn a_hidden_part_alone_is_written_and_a_changed_part_setting_needs_a_new_build() {
        let (trees, raws) = world();
        let cancel = CancellationToken::new();
        let hidden = |on: bool| TreeEdit {
            parts: std::collections::BTreeMap::from([(GREY.to_uppercase(), deadlock_project::effect_edit::PartSettings { hidden: on, size: Some(2.), ..Default::default() })]),
            ..Default::default()
        };
        let rows = [row(A, hidden(true))];
        let (out, files) = plan(None, &rows, &BTreeSet::new(), &BTreeSet::new(), &trees, &raws, &cancel).unwrap();
        // Only the hidden part is written (Size reaches no 3D model there).
        assert_eq!(outcomes(&out[0]), [(A, Outcome::Unchanged), (SHARED, Outcome::Unchanged), (GREY, Outcome::Written)]);
        let back = crate::effect_edit::document(&files[GREY].1).unwrap().root;
        assert_eq!(back.get("m_Renderers").unwrap().array().unwrap()[0].get("m_bDisableOperator").unwrap().kind, Kind::Bool(true));
        let receipt = EffectEditReceipt { trees: out, ..EffectEditReceipt::empty("") };
        assert!(receipt.same_rows(&rows));
        assert!(!receipt.same_rows(&[row(A, hidden(false))]), "a changed part setting");
        // Left out, it gets none of it.
        let left = TreeEdit { excluded: vec![GREY.into()], ..hidden(true) };
        let (out, files) = plan(None, &[row(A, left)], &BTreeSet::new(), &BTreeSet::new(), &trees, &raws, &cancel).unwrap();
        assert!(files.is_empty());
        assert_eq!(outcomes(&out[0])[2], (GREY, Outcome::Excluded));
    }
    fn outcomes(tree: &EditedTree) -> Vec<(&str, Outcome)> {
        tree.parts.iter().map(|p| (p.path.as_str(), p.outcome)).collect()
    }

    #[test]
    fn collisions_unchanged_and_excluded_parts_follow_their_rules() {
        let archive: Option<&Vpk> = None;
        let (trees, raws) = world();
        let cancel = CancellationToken::new();
        let turn = |hue| TreeEdit { hue, ..Default::default() };
        let none = BTreeSet::new();
        // A first (+90), then B (+180): the shared part follows A; greys do not change under a hue turn.
        let (out, files) = plan(archive, &[row(A, turn(90.)), row(B, turn(180.))], &BTreeSet::new(), &none, &trees, &raws, &cancel).unwrap();
        assert_eq!(outcomes(&out[0]), [(A, Outcome::Written), (SHARED, Outcome::Written), (GREY, Outcome::Unchanged)]);
        assert_eq!(outcomes(&out[1]), [(B, Outcome::Written), (SHARED, Outcome::OtherEffect)]);
        assert_eq!(out[1].parts[1].follows.as_deref(), Some(A));
        assert_eq!(out[1].parts[1].note, "shared: also part of a, which comes first in Extras → Effects; that effect's edit is used for it");
        assert_eq!(files.keys().collect::<Vec<_>>(), [A, B, SHARED]);
        assert_eq!(files[SHARED].0, A, "the shared part is written by A's edit");
        // The same edit on both: nothing to decide, no note.
        let (out, _) = plan(archive, &[row(A, turn(90.)), row(B, turn(90.))], &BTreeSet::new(), &none, &trees, &raws, &cancel).unwrap();
        assert_eq!(out[1].parts[1].outcome, Outcome::SameAsOtherEffect);
        assert!(out[1].parts[1].note.is_empty());
        // A user's own replacement file wins for that part; excluded parts are not written.
        let user = BTreeSet::from([SHARED.to_owned()]);
        let edit = TreeEdit { hue: 90., excluded: vec![A.into()], ..Default::default() };
        let (out, files) = plan(archive, &[row(A, edit)], &user, &none, &trees, &raws, &cancel).unwrap();
        assert_eq!(outcomes(&out[0]), [(A, Outcome::Excluded), (SHARED, Outcome::UserFile), (GREY, Outcome::Unchanged)]);
        assert_eq!(out[0].parts[1].note, "shared: your own replacement file is used; the edit of a changes the effect's other parts");
        assert!(files.is_empty(), "nothing differs from the game");
        // An identity edit writes nothing; a size of 1 adds nothing.
        let (_, files) = plan(archive, &[row(A, TreeEdit { size: 1., ..Default::default() })], &BTreeSet::new(), &none, &trees, &raws, &cancel).unwrap();
        assert!(files.is_empty());
        // A replacement of one color touches only the part that has it.
        let one = TreeEdit { colors: vec![ColorReplacement { from: [40, 200, 40], to: [255, 0, 0], near: false }], ..Default::default() };
        let (_, files) = plan(archive, &[row(B, one)], &BTreeSet::new(), &none, &trees, &raws, &cancel).unwrap();
        assert_eq!(files.keys().collect::<Vec<_>>(), [SHARED]);
        // Only tops are allowed, and the message names the effect.
        let error = plan(archive, &[row(SHARED, turn(90.))], &BTreeSet::new(), &none, &trees, &raws, &cancel).unwrap_err().to_string();
        assert!(error.contains(EFFECT_NOT_IN_GAME) && error.contains(SHARED), "{error}");
        // A part outside the hero's folders is left out unless the edit lists it in `shared`.
        let outside = BTreeSet::from([SHARED.to_owned()]);
        let (left, files) = plan(archive, &[row(A, turn(90.))], &none, &outside, &trees, &raws, &cancel).unwrap();
        assert_eq!(outcomes(&left[0]), [(A, Outcome::Written), (SHARED, Outcome::SharedLeftOut), (GREY, Outcome::Unchanged)]);
        assert_eq!(left[0].parts[1].note, "shared: shared with other heroes, so it is left out of the edit of a (choose it in Advanced to change it)");
        assert_eq!(files.keys().collect::<Vec<_>>(), [A]);
        let chosen = TreeEdit { hue: 90., shared: vec!["PARTICLES/shared.vpcf_c".into()], ..Default::default() };
        let (left, files) = plan(archive, &[row(A, chosen.clone())], &none, &outside, &trees, &raws, &cancel).unwrap();
        assert_eq!(left[0].parts[1].outcome, Outcome::Written);
        assert_eq!(files.keys().collect::<Vec<_>>(), [A, SHARED], "ticking a shared part writes it");
        // `excluded` wins over `shared`.
        let both = TreeEdit { excluded: vec![SHARED.into()], ..chosen };
        let (left, _) = plan(archive, &[row(A, both)], &none, &outside, &trees, &raws, &cancel).unwrap();
        assert_eq!(left[0].parts[1].outcome, Outcome::Excluded);
        // Rows and receipts compare targets and edits in order.
        let receipt = EffectEditReceipt { trees: out, ..EffectEditReceipt::empty("h") };
        assert!(receipt.same_rows(&[row("PARTICLES/A.vpcf_c", TreeEdit { hue: 90., excluded: vec![A.into()], ..Default::default() })]));
        assert!(!receipt.same_rows(&[row(A, turn(91.))]));
        assert_eq!(receipt.notes().len(), 1);
    }

    #[test]
    fn the_shown_files_are_what_a_build_of_every_row_writes_for_the_trees_parts() {
        let archive: Option<&Vpk> = None;
        let (trees, raws) = world();
        let cancel = CancellationToken::new();
        let turn = |hue| TreeEdit { hue, ..Default::default() };
        let none = BTreeSet::new();
        let build = |rows: &[EffectEditRow], user: &BTreeSet<String>| plan(archive, rows, user, &none, &trees, &raws, &cancel).unwrap().1;
        // A first (+90), then B (+180): B's shown shared part is A's edit, as the build writes it.
        let rows = [row(A, turn(90.)), row(B, turn(180.))];
        let written = build(&rows, &none);
        let (shown, files) = shown_from(archive, &rows, B, &none, &none, &trees, &raws, &cancel).unwrap();
        assert_eq!(outcomes(&shown), [(B, Outcome::Written), (SHARED, Outcome::OtherEffect)]);
        assert_eq!(files.keys().collect::<Vec<_>>(), [B, SHARED]);
        assert_eq!(files[SHARED], written[SHARED].1, "the shared part plays A's edit");
        assert_eq!(files[B], written[B].1);
        // B leaves the shared part out; A still writes it, so B's effect plays A's file there.
        let rows = [row(A, turn(90.)), row(B, TreeEdit { hue: 180., excluded: vec![SHARED.into()], ..Default::default() })];
        let (_, files) = shown_from(archive, &rows, B, &none, &none, &trees, &raws, &cancel).unwrap();
        assert_eq!(files[SHARED], build(&rows, &none)[SHARED].1);
        // Every file of A's tree, exactly the build's.
        let (_, files) = shown_from(archive, &rows, A, &none, &none, &trees, &raws, &cancel).unwrap();
        let all = build(&rows, &none);
        let expected: BTreeMap<String, Vec<u8>> = all.into_iter().filter(|(p, _)| p != B).map(|(p, (_, b, _))| (p, b)).collect();
        assert_eq!(files, expected);
        // The user's own file wins for its part: not among the edited files.
        let user = BTreeSet::from([SHARED.to_owned()]);
        let (shown, files) = shown_from(archive, &[row(A, turn(90.))], A, &user, &none, &trees, &raws, &cancel).unwrap();
        assert_eq!(shown.parts[1].outcome, Outcome::UserFile);
        assert_eq!(files.keys().collect::<Vec<_>>(), [A]);
        // A row that cannot be planned and shares nothing does not stop the player.
        let stale = row("particles/gone.vpcf_c", turn(10.));
        let (_, files) = shown_from(archive, &[stale, row(B, turn(180.))], B, &none, &none, &trees, &raws, &cancel).unwrap();
        assert_eq!(files.keys().collect::<Vec<_>>(), [B, SHARED]);
    }

    #[test]
    fn rows_with_puts_the_shown_edit_where_the_page_saves_it() {
        let turn = |hue| TreeEdit { hue, ..Default::default() };
        let paused = EffectEditRow { enabled: false, ..row(GREY, turn(5.)) };
        let saved = [row(A, turn(10.)), paused, row(B, turn(20.))];
        let targets = |rows: &[EffectEditRow]| rows.iter().map(|r| (r.target.clone(), r.edit.hue, r.enabled)).collect::<Vec<_>>();
        // A saved row keeps its place with the current edit; paused rows are left out.
        assert_eq!(targets(&rows_with(&saved, true, "PARTICLES/B.vpcf_c", &turn(30.))), [(A.into(), 10., true), (B.into(), 30., true)]);
        // A paused row is shown in its place.
        assert_eq!(targets(&rows_with(&saved, true, GREY, &turn(40.))), [(A.into(), 10., true), (GREY.into(), 40., true), (B.into(), 20., true)]);
        // A new row comes last; with extras paused only the shown row is planned.
        assert_eq!(targets(&rows_with(&saved, true, SHARED, &turn(50.))).last().cloned(), Some((SHARED.into(), 50., true)));
        assert_eq!(targets(&rows_with(&saved, false, B, &turn(60.))), [(B.into(), 60., true)]);
    }
}
