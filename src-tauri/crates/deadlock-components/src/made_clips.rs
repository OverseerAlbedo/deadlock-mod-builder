//! Clips made from an animation (`ResourceReplacement.made`): the one place
//! that turns a row into the clip's bytes, used by the build (`motion::stage`),
//! the Motion page's preview and report, and the tests. No
//! Blender: the packet was made on the Motion page
//! (`deadlock_preparation::clip_making`); a missing or stale one stops with
//! `clip_making::NOT_MADE`.
use crate::{fail, motion, read, sha, CancellationToken, Result};
use deadlock_preparation::clip_decode::{Clip, Skeleton};
use deadlock_preparation::clip_making::{self as making, Made, MadeReport, Switches, SWITCHES};
use deadlock_project::customization::ResourceReplacement;
use deadlock_project::made_clip::MadeClip;
use deadlock_vpk::Vpk;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The install check's refusal when a made clip's row differs from the build.
pub const CHANGED_SINCE_BUILD: &str = "A clip made from an animation changed since this build. Rebuild before installing.";

/// Where a build finds packets and the prepared model's fit.
#[derive(Debug, Clone)]
pub struct MadeInputs {
    /// The project's workspace folder.
    pub project_root: PathBuf,
    /// The prepared model's fit, or why there is none (`clip_making::fit_path`).
    pub fit: std::result::Result<PathBuf, String>,
}
impl MadeInputs {
    pub fn for_project(project_root: PathBuf, project: &deadlock_project::Project) -> Self {
        Self { project_root, fit: making::fit_path(project).map_err(|e| e.to_string()) }
    }
}

/// What a receipt keeps about a made clip (absent for every other row).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MadeEntry {
    pub made: MadeClip,
    pub switches: Switches,
    pub packet_sha256: String,
    pub fit_sha256: String,
}

/// One made clip: its bytes (checked with `validate_clip`), the game's
/// clip, the packet's stamp and the report.
pub struct Authored {
    pub bytes: Vec<u8>,
    pub original: Vec<u8>,
    pub stamp: Made,
    pub report: MadeReport,
}

/// The clip's own skeleton from the game archive.
pub fn clip_skeleton(archive: &Vpk, original: &[u8]) -> Result<Skeleton> {
    let clip = Clip::decode(original).map_err(fail)?;
    let id = if clip.skeleton.ends_with("_c") { clip.skeleton.clone() } else { format!("{}_c", clip.skeleton) };
    Skeleton::decode(&read(archive, &id)?).map_err(fail)
}

/// Why the game's clip `target` cannot be made from an animation, or None.
pub fn refusal(archive: &Vpk, target: &str) -> Result<Option<&'static str>> {
    making::refusal(&read(archive, &crate::compiled(target)?)?).map_err(fail)
}

/// The clip `target` made from `row` (which must carry `made`) with `SWITCHES`.
pub fn author_row(archive: &Vpk, target: &str, row: &ResourceReplacement, inputs: &MadeInputs, cancel: &CancellationToken) -> Result<Authored> {
    author_with(archive, target, row, inputs, SWITCHES, cancel)
}

/// `author_row` with other switches (tests and comparisons only).
pub fn author_with(archive: &Vpk, target: &str, row: &ResourceReplacement, inputs: &MadeInputs, switches: Switches, cancel: &CancellationToken) -> Result<Authored> {
    let made = row.made.as_ref().ok_or_else(|| fail("This row is not made from an animation."))?;
    let source = row.source.as_deref().unwrap_or_default();
    if source.trim().is_empty() {
        return Err(fail(format!("{}: {}", making::NOT_MADE, made.action)));
    }
    let fit = inputs.fit.as_ref().map_err(|e| fail(e))?;
    let (stamp, export) = making::load(&inputs.project_root, Path::new(source), fit, made, cancel).map_err(fail)?;
    cancel.check()?;
    let original = read(archive, &crate::compiled(target)?)?;
    let skeleton = clip_skeleton(archive, &original)?;
    let (bytes, report) = making::author(&original, &skeleton, &export, made, switches).map_err(fail)?;
    motion::validate_clip(&original, &bytes, cancel)?;
    Ok(Authored { bytes, original, stamp, report })
}

/// The receipt's record of an authored row.
pub fn entry(made: &MadeClip, authored: &Authored, switches: Switches) -> MadeEntry {
    MadeEntry { made: made.clone(), switches, packet_sha256: authored.stamp.packet_sha256.clone(), fit_sha256: authored.stamp.fit_sha256.clone() }
}

/// Whether the rows' made clips are the ones a receipt's entries were built
/// from (rows matched by target, case-blind). Rows and entries without
/// `made` always agree here.
pub fn same_rows(rows: &[ResourceReplacement], entries: &[motion::AnimationEntry]) -> bool {
    rows.iter().all(|row| {
        let target = deadlock_core::text::casefold(row.target.as_deref().unwrap_or_default());
        let entry = entries.iter().find(|e| deadlock_core::text::casefold(&e.target) == target || deadlock_core::text::casefold(&e.target) == format!("{target}_c"));
        match (&row.made, entry.and_then(|e| e.made.as_ref())) {
            (None, None) => true,
            (Some(made), Some(kept)) => *made == kept.made && kept.switches == SWITCHES,
            _ => false,
        }
    })
}

/// A short digest of the bytes (the preview's cache key).
pub fn digest(bytes: &[u8]) -> String {
    sha(bytes)
}
