//! Ability objects: the game models a hero's abilities draw (a thrown dagger, a
//! projectile shape), which a project may replace with a shape of the user's.
//!
//! What is listed, per hero, comes only from the game's own data:
//! * the models that the records the hero's bound abilities reach name
//!   (`effect_catalog::Definitions`, plain reach through links from each bound
//!   ability), and
//! * the models the particle trees of those records draw with a
//!   `C_OP_RenderModels` renderer (children followed through each particle
//!   file's resource references).
//!
//! The hero's own models (the roster's models and their extras) are never
//! listed, nor are generic shapes that `SHARED_SHAPE_HEROES` or more live
//! heroes use (spheres, streaks, bullets). "Live" is the effect trees' notion:
//! a hero definition that is not `m_bDisabled` and whose model is in the roster.
//!
//! What is offered for replacement is one tested function, `offered`: a model
//! with no bones, no physics block, no hitboxes, no animation (blocks, groups,
//! sequences or graph), no attachments, exactly one material, that the game
//! draws only through particles (no live definition names it as a model of its
//! own). Another kind of object is opened by changing that function, once that
//! kind is known to work in the game. Every listed model that is not offered
//! carries one plain reason sentence.
//!
//! The build's allowed targets are `allowed`, the catalog's offered rows: one
//! function for both (a test compares them).
use crate::{CancellationToken, Result, effect_catalog::{self, Definitions, Names}, fail, heroes, sha, sound_tags};
use deadlock_core::text::casefold;
use deadlock_project::ability_objects::ObjectFit;
use deadlock_resource::{Value, container};
use deadlock_vpk::{Hashes, Vpk};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::Path;
use std::sync::{Arc, Mutex};

/// The offered profile's version (receipts record it).
pub const PROFILE: &str = "ability-object-profile-v1";
/// A model this many live heroes or more use is a generic shape: never listed.
pub const SHARED_SHAPE_HEROES: usize = 4;
/// Where the build writes an object's own files (materials, textures, the mesh source).
/// Kept short: the compiler names textures after their material (about 40
/// characters more) and its paths end at 259 characters.
pub const OBJECT_FOLDER: &str = "models/dmb_objects";
/// The most records one ability may reach, and the most files one particle tree may hold.
const MAX_REACH: usize = 4096;
const MAX_TREE: usize = 512;

/// Why a listed model is not offered (one sentence each).
pub const REASON_SKELETON: &str = "It has a skeleton; objects with bones cannot be replaced yet.";
pub const REASON_COLLISION: &str = "It has collision (a physics shape); objects with collision cannot be replaced yet.";
pub const REASON_HITBOXES: &str = "It has hitboxes; objects that can be hit cannot be replaced yet.";
pub const REASON_ANIMATED: &str = "It is animated; animated objects cannot be replaced yet.";
pub const REASON_ATTACHMENTS: &str = "It has attachment points that effects or code follow; such objects cannot be replaced yet.";
pub const REASON_MATERIALS: &str = "It has several materials; only objects with one material can be replaced so far.";
pub const REASON_NO_MATERIAL: &str = "It has no material.";
pub const REASON_PLACED: &str = "The game also places it as an object of its own, not only through an effect; such objects cannot be replaced yet.";
pub const REASON_MISSING: &str = "The game archive does not hold this model.";
pub const REASON_UNREADABLE: &str = "The game's file of this model could not be read.";
pub const REASON_NOT_DRAWN: &str = "The game does not draw this model: it only blocks players, shots or sight, and what is seen there is an effect. A replacement would not show.";

/// Whether the game never draws `model` (casefolded `.vmdl`): no effect draws it
/// (`drawn` false), at least one definition names it, and every definition that
/// names it names it only as a blocker (`effect_catalog::Record::blocker_models`:
/// a blocker key held by a modifier, or an invisible vision blocker). What is seen
/// where such a model stands is an effect, so a replacement would not show. The
/// one rule; a row it holds is not offered (`REASON_NOT_DRAWN`) and so is not in
/// the build's allowed targets either.
pub fn not_drawn(defs: &Definitions, model: &str, drawn: bool) -> bool {
    let mut naming = defs.records.values().filter(|r| r.models.contains(model)).peekable();
    !drawn && naming.peek().is_some() && naming.all(|r| r.blocker_models.contains(model))
}

/// The ONE exception to "never close an object by its name". The build's own read-back
/// refuses this object: a resting-turn sign on a bone the sign rule does not predict, and
/// bone positions the compiler changes by up to 1.9e-4 in. No fact read from its file
/// predicts that, so it is closed by name: a deliberate, single exception. Try the entry
/// again after a game update (`ignore_known_for_test` lets a test build it anyway) and
/// remove it when the object builds. The list is not a place for further objects: one that
/// does not build is closed by a fact read from its file.
pub const KNOWN_NOT_TO_REBUILD: [&str; 1] = ["models/heroes_wip/ratking/ratking_armor.vmdl"];
/// Why an object of `KNOWN_NOT_TO_REBUILD` is not offered.
pub const REASON_KNOWN_NOT_TO_REBUILD: &str = "A rebuild of this object does not come out as the game has it, for a cause not yet found; it cannot be replaced yet.";
#[cfg(feature = "object-test-switches")]
static IGNORE_KNOWN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Lets a test build an object of `KNOWN_NOT_TO_REBUILD` to see whether it still
/// refuses (feature `object-test-switches` only, never in the app).
#[cfg(feature = "object-test-switches")]
pub fn ignore_known_for_test(ignore: bool) {
    IGNORE_KNOWN.store(ignore, std::sync::atomic::Ordering::SeqCst);
}
/// Whether `model` (casefolded `.vmdl`) is held closed by `KNOWN_NOT_TO_REBUILD`.
pub fn known_not_to_rebuild(model: &str) -> bool {
    #[cfg(feature = "object-test-switches")]
    if IGNORE_KNOWN.load(std::sync::atomic::Ordering::SeqCst) {
        return false;
    }
    KNOWN_NOT_TO_REBUILD.contains(&model_key(model).as_str())
}

/// Build and command messages (matched by `troubleshooting.rs`).
pub const NOT_OFFERED: &str = "This ability object cannot be replaced (the game changed it, or it is not one of this hero's objects)";
pub const OBJECT_CHANGED: &str = "The game's ability object changed since this build";
pub const COMPILED_DIFFERS: &str = "The compiled ability object differs from the game's in more than its mesh";
pub const NO_BUILDER: &str = "Ability objects are compiled only by Build & install.";
pub const EMPTY_ROW: &str = "Choose a file for each enabled ability object, or remove its empty row.";
pub const NEEDS_CURRENT_SDK: &str = "Ability objects are compiled only with Reduced CSDK 13 or later. Choose it as the Community SDK in Settings and keep the model compiler on Auto";

/// The folder of one object's own files in the game (`OBJECT_FOLDER/<stem>_<hash8>`).
pub fn object_root(target: &str) -> String {
    let key = model_key(target);
    let stem: String = key.rsplit('/').next().unwrap_or_default().trim_end_matches(".vmdl").chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' }).take(24).collect();
    format!("{OBJECT_FOLDER}/{stem}_{}", &sha(key.as_bytes())[..8])
}
/// Checks the enabled rows a build would compile: a file chosen, an offered
/// target (`allowed`), and a current preparation (`deadlock_preparation::ability_object::load`).
/// Returns each row's prepared stamp and packet.
pub fn check_rows(
    archive: &Vpk,
    hero_model: &str,
    rows: &[deadlock_project::ability_objects::AbilityObjectRow],
    project_root: &Path,
    cancel: &CancellationToken,
) -> Result<Vec<(deadlock_preparation::ability_object::Prepared, deadlock_preparation::ability_object::Packet)>> {
    if rows.is_empty() {
        return Ok(vec![]);
    }
    let listed = catalog(archive, hero_model, cancel)?;
    let allowed = allowed_in(&listed);
    let mut out = vec![];
    for row in rows {
        if row.source.trim().is_empty() {
            return Err(fail(EMPTY_ROW));
        }
        let target = deadlock_project::ability_objects::target_path(&row.target).map_err(fail)?;
        if !allowed.contains(&casefold(&target)) {
            // The catalog's own sentence when it lists the object.
            let reason = listed.rows.iter().find(|r| casefold(&r.target) == casefold(&target)).and_then(|r| r.reason.clone());
            return Err(fail(match reason {
                Some(reason) => format!("{NOT_OFFERED}: {target} ({reason})"),
                None => format!("{NOT_OFFERED}: {target}"),
            }));
        }
        let folder = deadlock_preparation::ability_object::folder(project_root, &target);
        out.push(deadlock_preparation::ability_object::load(&folder, Path::new(&row.source), &object_root(&target), cancel).map_err(fail)?);
    }
    Ok(out)
}

// ------------------------------------------------------------------ model facts

/// What a compiled model holds, as far as replacing its mesh is concerned.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ModelFacts {
    pub blocks: Vec<String>,
    pub bones: Vec<String>,
    /// Bones of the meshes' own skeleton (`MDAT m_skeleton`), when stated.
    pub mesh_skeleton_bones: Option<usize>,
    pub embedded_meshes: usize,
    pub mesh_groups: Vec<String>,
    pub physics_block: bool,
    pub physics_refs: usize,
    pub hitboxes: usize,
    pub animation_blocks: Vec<String>,
    pub anim_graph: Option<String>,
    pub sequence_groups: usize,
    pub anim_groups: usize,
    /// `name@bone` of every attachment.
    pub attachments: Vec<String>,
    /// Material references (`.vmat`), casefolded, sorted.
    pub materials: Vec<String>,
    /// Every other resource reference, casefolded, sorted (meshes inside the model are not references).
    pub other_refs: Vec<String>,
    /// Bounds of the scene objects (model space), when stated.
    pub bounds: Option<Bounds>,
    /// Shapes of the physics block by type (`sphere`, `capsule`, `hull`,
    /// `hull of 8 points` (a box is stored so; the decompiled model names it
    /// as a hull file all the same), `mesh`, `compound`) plus `file` for each
    /// physics file the model names.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub physics_shapes: BTreeMap<String, usize>,
    /// Meshes per level of detail (`m_refLODGroupMasks` against
    /// `m_lodGroupSwitchDistances`); empty when the model states none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lod_meshes: Vec<usize>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lod_switch_distances: Vec<f64>,
    /// Each embedded mesh's level-of-detail mask and materials, in mesh order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub meshes: Vec<MeshFacts>,
    /// The bones the embedded meshes are bound to (their own skeletons), in model skeleton order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mesh_bones: Vec<String>,
    /// The physics block holds cloth (`m_pFeModel`): the compiler needs the
    /// game's cloth proxy mesh, which a replacement does not have.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub cloth: bool,
    /// The model's key values name break pieces (`break_list` or
    /// `break_command_list`), which the decompiled model does not keep.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub break_pieces: bool,
    /// A body group choice stored without its choice name (`name_@0` and no
    /// `_#&…`): the compiler always writes the name.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub unnamed_body_choices: bool,
    /// The embedded meshes are not all bound to the same bones.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub mixed_mesh_bindings: bool,
    /// The sequences are arranged in folders (`ASEQ m_keyValues.faceposer_folders`):
    /// the decompiled model lists folders first, so a rebuild reorders them.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub animation_folders: bool,
    /// The animation decode skeleton (`AGRP m_decodeKey.m_boneArray`) is not in
    /// the model skeleton's depth-first order, the order the compiler writes.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub decode_reordered: bool,
    /// The physics block binds its parts to bones (`PHYS m_boneNames` not empty).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub physics_on_bones: bool,
    /// The physics block is stored in a form the project's reader refuses (its
    /// shapes, bones and cloth cannot be read).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub physics_unreadable: bool,
    /// The physics block is stored in the KV3 form whose LZ4 frame table size is
    /// stated as 0 (`binary::unstated_frame_table`), which the reader reads only since
    /// it learned that form; on an object with bones it stays closed as unreadable
    /// collision unless `Switches::unstated_frame_collision` is open.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub physics_unstated_frames: bool,
    /// The material values the model's animation drives (`DATA
    /// m_AnimatedMaterialAttributes[].m_AttributeName`), in file order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub animated_material_values: Vec<String>,
    /// A rest rotation of the model skeleton (`DATA m_boneRotParent`, x y z w)
    /// that the compiler writes with the other sign: the sign its matrix gives
    /// (`w` when the trace is 0 or more, else the component of the largest
    /// diagonal) is negative, or the trace is within `REST_SIGN_TIE` of 0 and
    /// `w` is negative.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub rest_sign_flips: bool,
    /// The bones whose resting turn comes back with the other sign through its Euler angles
    /// (`object_parts::round_trip_flips`), in skeleton order: what the compiler writes with the
    /// other sign for such a bone when the vmdl does not mark it to be kept and no mesh binds it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rest_round_trip_flips: Vec<String>,
    /// The key values list the cameras in the reverse of their attachments' order
    /// (`object_parts::cameras_reversed`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub cameras_reversed: bool,
    /// The key values hold `AttachmentCameraData` after other game data, or
    /// `TaggedSoundSettings_t` before other game data: the compiler writes the
    /// first first and the second last.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub key_values_reordered: bool,
    /// A rest scale of the model skeleton (`DATA m_boneScaleParent`) that the
    /// read-back does not count as 1 (`same_value`): the compiler writes 1.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub rest_scale: bool,
    /// The model holds morph targets (an `MRPH` block), which a replacement mesh
    /// does not have.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub morphs: bool,
    /// The key values hold bone constraints (`BoneConstraintList`), whose numbers a
    /// rebuild writes back with other rounding (the decompile keeps fewer digits).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub bone_constraints: bool,
    /// Each embedded mesh's name (`CTRL embedded_meshes[].m_Name`), in mesh order;
    /// read only for a model with body groups.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mesh_names: Vec<String>,
    /// Which body-group choices draw each embedded mesh (`DATA m_refMeshGroupMasks`,
    /// one bit per entry of `mesh_groups`), in mesh order; read only for a model with body groups.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mesh_group_masks: Vec<u64>,
    /// The choices the model shows unless something picks others (`DATA m_nDefaultMeshGroupMask`);
    /// read only for a model with body groups.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_mesh_group_mask: Option<u64>,
    /// The bones each embedded mesh is bound to (`MDAT m_skeleton.m_bones`), in mesh
    /// order; read only for a model whose meshes are bound to different bones.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mesh_bindings: Vec<Vec<String>>,
    /// Some mesh is bound to a bone but not to that bone's parent (`object_states::binding_gaps`);
    /// read only for a model whose meshes are bound to different bones.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub mesh_binding_gaps: bool,
    /// The names of the model's material groups, its looks (`DATA m_materialGroups[].m_name`),
    /// in order; the first is the look it has unless something picks another.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub material_groups: Vec<String>,
    /// The model's animation decode keys name no bone and only morph channels
    /// (`object_morphs::morph_channels_only`); read only for a model with a morph block.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub morph_channels_only: bool,
    /// A model with cloth whose skeleton holds a bone name with a byte other than A–Z, a–z,
    /// 0–9 and `_` (the cloth's own bones made from a cloth proxy mesh, `$cloth_…`): the
    /// compiler writes such a byte as `_`, so the rebuilt skeleton cannot hold the names the
    /// game's cloth binds by. Read only for a model with cloth.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub cloth_bones_renamed: bool,
    /// A model with a morph block whose morph data nothing but its render mesh and its
    /// animation reads (`object_morphs::morphs_apart`: no cloth morph layer, every hit area
    /// on a bone). Read only for a model with a morph block.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub morphs_apart: bool,
    /// The model skeleton is not in the order the compiler writes (`depth_first`: each bone
    /// followed by its children's branches, siblings in the game's order), so a rebuild lists
    /// the bones in another order. Read only for a model whose physics names bones.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub skeleton_reordered: bool,
}
/// The order the compiler writes a skeleton in (bone indices of the game's list): depth first
/// from each root, each bone followed by its children's branches, siblings in the game's order.
/// `None` when a parent index is out of range or the parents form a loop.
pub fn depth_first(parents: &[i128]) -> Option<Vec<usize>> {
    let n = parents.len();
    let mut children: Vec<Vec<usize>> = vec![vec![]; n];
    let mut roots = vec![];
    for (i, p) in parents.iter().enumerate() {
        match usize::try_from(*p) {
            Ok(p) if p < n && p != i => children[p].push(i),
            Ok(_) => return None,
            Err(_) => roots.push(i),
        }
    }
    let mut out = Vec::with_capacity(n);
    let mut stack: Vec<usize> = roots.into_iter().rev().collect();
    while let Some(b) = stack.pop() {
        if out.len() >= n {
            return None;
        }
        out.push(b);
        stack.extend(children[b].iter().rev());
    }
    (out.len() == n).then_some(out)
}
/// Whether the compiler writes a bone name otherwise: it writes every byte other than A–Z, a–z,
/// 0–9 and `_` as `_` (`ModelFacts::cloth_bones_renamed`).
pub fn renamed_by_compiler(bone: &str) -> bool {
    !bone.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
}
/// How near 0 a rest rotation's trace may be before both signs are possible.
pub const REST_SIGN_TIE: f64 = 1e-4;
/// Whether the compiler writes the rest rotation `q` (x y z w) as `-q` (`ModelFacts::rest_sign_flips`).
pub fn rest_sign_flips(q: [f64; 4]) -> bool {
    let [x, y, z, w] = q;
    let (m00, m11, m22) = (1. - 2. * (y * y + z * z), 1. - 2. * (x * x + z * z), 1. - 2. * (x * x + y * y));
    let trace = m00 + m11 + m22;
    let diagonal = if m00 >= m11 && m00 >= m22 { x } else if m11 >= m22 { y } else { z };
    let sign = if trace >= 0. { w } else { diagonal };
    // At a trace of 0 the compiler takes w (a model with w > 0 there builds; one with w < 0 does not).
    sign < 0. || trace.abs() < REST_SIGN_TIE && w < 0.
}
/// One embedded mesh of a compiled model.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MeshFacts {
    /// The levels of detail it is drawn in (bit per level; 0 when not stated).
    pub lod_mask: u64,
    /// Its draw calls' materials, casefolded, without `_c`.
    pub materials: Vec<String>,
    /// Its scene objects' bounds (model space), when stated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bounds: Option<Bounds>,
}
impl ModelFacts {
    /// One bone at most in the model skeleton and in the mesh skeleton, and one in either.
    pub fn one_bone(&self) -> bool {
        let mesh = self.mesh_skeleton_bones.unwrap_or(0);
        self.bones.len() <= 1 && mesh <= 1 && (self.bones.len() == 1 || mesh == 1)
    }
    /// Two bones or more (in the model skeleton or the mesh skeleton).
    pub fn several_bones(&self) -> bool {
        self.bones.len() > 1 || self.mesh_skeleton_bones.is_some_and(|n| n > 1)
    }
    pub fn has_physics(&self) -> bool {
        self.physics_block || self.physics_refs > 0
    }
    /// Physics made only of `SIMPLE_SHAPES`, inside the model (no physics file).
    pub fn simple_physics(&self) -> bool {
        self.physics_refs == 0 && !self.physics_shapes.is_empty() && self.physics_shapes.keys().all(|k| SIMPLE_SHAPES.contains(&k.as_str()))
    }
    pub fn animated(&self) -> bool {
        !self.animation_blocks.is_empty() || self.anim_graph.is_some() || self.sequence_groups > 0 || self.anim_groups > 0
    }
    /// Animated without an animation graph (a legacy animation block).
    pub fn legacy_animation(&self) -> bool {
        self.animated() && self.anim_graph.is_none()
    }
    /// No bone in the model skeleton nor in the mesh skeleton.
    pub fn boneless(&self) -> bool {
        self.bones.is_empty() && self.mesh_skeleton_bones.unwrap_or(0) == 0
    }
    /// Animated on exactly one bone, without an animation graph and without morph targets
    /// (`Switches::one_bone_animated`).
    pub fn one_bone_animation(&self) -> bool {
        self.one_bone() && self.legacy_animation() && !self.morphs
    }
    /// Animated without a bone, without an animation graph and without morph targets, the
    /// animation driving only material values (`Switches::boneless_animated`).
    pub fn material_animation(&self) -> bool {
        self.boneless() && self.legacy_animation() && !self.morphs && !self.animated_material_values.is_empty()
    }
    /// Whether the build points the model's other looks (material groups) at the user's
    /// materials, every look keeping the game's material it changes to: several bones, or an
    /// animation driving only material values. Every other build keeps the vmdl's looks as the
    /// game wrote them, which then name none of the user's materials.
    pub fn remaps_material_groups(&self) -> bool {
        self.several_bones() || self.material_animation()
    }
    /// Exactly one block each of `ANIM`, `ASEQ` and `AGRP` inside the model (what
    /// keeping the game's animation needs).
    pub fn embedded_animation(&self) -> bool {
        crate::keep_blocks::ANIMATION.iter().all(|k| self.blocks.iter().filter(|b| b == k).count() == 1)
    }
    /// Exactly one physics block inside the model and no physics file it only
    /// names (what keeping the game's collision needs).
    pub fn embedded_physics(&self) -> bool {
        self.physics_refs == 0 && self.blocks.iter().filter(|b| *b == crate::keep_blocks::PHYSICS).count() == 1
    }
    /// Whether some level of detail draws more than one mesh.
    pub fn extra_meshes(&self) -> bool {
        self.lod_meshes.iter().any(|n| *n > 1)
    }
    /// The meshes of each level of detail, by mesh index, in mesh order.
    pub fn levels(&self) -> Vec<Vec<usize>> {
        (0..self.lod_meshes.len().min(64)).map(|level| self.meshes.iter().enumerate().filter(|(_, m)| m.lod_mask & (1u64 << level) != 0).map(|(i, _)| i).collect()).collect()
    }
    /// Outline shells: every level of detail draws exactly two meshes, a body
    /// and a shell; every shell draws with one and the same material that no
    /// body uses. Returns that material. Any other level with several meshes is
    /// not an outline and is never carried.
    pub fn shell_material(&self) -> Option<String> {
        let levels = self.levels();
        if levels.is_empty() || levels.iter().any(|l| l.len() != 2) || self.meshes.len() != levels.len() * 2 {
            return None;
        }
        let body: BTreeSet<&String> = levels.iter().flat_map(|l| &self.meshes[l[0]].materials).collect();
        let shell: BTreeSet<&String> = levels.iter().flat_map(|l| &self.meshes[l[1]].materials).collect();
        match shell.into_iter().collect::<Vec<_>>().as_slice() {
            [one] if !body.contains(one) && levels.iter().all(|l| self.meshes[l[1]].materials.len() == 1) => Some((*one).clone()),
            _ => None,
        }
    }
    /// The bounds of the body meshes (every mesh but the outline shells and the
    /// parts kept as the game's own, `kept_parts`).
    pub fn body_bounds(&self) -> Option<Bounds> {
        let mut shells: BTreeSet<usize> = if self.shell_material().is_some() { self.levels().iter().map(|l| l[1]).collect() } else { BTreeSet::new() };
        if let Some(parts) = parts_kept_as_games(self) {
            shells.extend(parts.kept.iter().map(|p| p.mesh));
        }
        shells.extend(crate::object_glow_shells::glow_shells(self).map(|g| g.shells()).unwrap_or_default());
        // Body-group states (`object_states`): the object as it normally shows, the default state.
        if let Some(states) = crate::object_states::states(self) {
            shells.extend(states.states.iter().filter(|s| !s.default).map(|s| s.mesh));
        }
        let mut out: Option<Bounds> = None;
        for b in self.meshes.iter().enumerate().filter(|(i, _)| !shells.contains(i)).filter_map(|(_, m)| m.bounds) {
            let o = out.get_or_insert(b);
            for k in 0..3 {
                o.min[k] = o.min[k].min(b.min[k]);
                o.max[k] = o.max[k].max(b.max[k]);
            }
        }
        out.or(self.bounds)
    }
    /// How far the game's outline shell stands out from its body: the median
    /// of the six bounds differences of the first level (inches).
    pub fn shell_distance(&self) -> Option<f64> {
        self.shell_material()?;
        let level = self.levels().into_iter().next()?;
        let (body, shell) = (self.meshes[level[0]].bounds?, self.meshes[level[1]].bounds?);
        let mut gaps: Vec<f64> = (0..3).flat_map(|k| [body.min[k] - shell.min[k], shell.max[k] - body.max[k]]).collect();
        gaps.sort_by(f64::total_cmp);
        let d = (gaps[2] + gaps[3]) / 2.;
        (d.is_finite() && d > 0.).then_some(d)
    }
    /// The materials the body draws with (all but the outline shells' material and
    /// the materials only the parts kept as the game's own draw with).
    pub fn body_materials(&self) -> Vec<String> {
        let shell = self.shell_material();
        let kept: BTreeSet<&String> = parts_kept_as_games(self).map(|p| p.kept_materials(self)).unwrap_or_default();
        let glow = crate::object_glow_shells::glow_shells(self).map(|g| g.material);
        self.materials.iter().filter(|m| Some(*m) != shell.as_ref() && !kept.contains(m) && Some(*m) != glow.as_ref()).cloned().collect()
    }
}

/// The embedded meshes of a model whose body groups' parts are bound to different
/// bones (`ModelFacts::mixed_mesh_bindings`), split into the one mesh per level of
/// detail the user's model replaces and the parts that stay the game's own.
#[derive(Debug, Clone, PartialEq)]
pub struct KeptParts {
    /// The replaced meshes (by mesh index): those the default choices show.
    pub replaced: Vec<usize>,
    /// The parts kept as the game's own.
    pub kept: Vec<KeptPart>,
    /// Parts shown together with the object (`object_parts::together`): every material the kept
    /// parts draw with stays the game's own, also one the replaced mesh shares (`drawn_materials`).
    pub together: bool,
}
/// One embedded mesh kept as the game's own.
#[derive(Debug, Clone, PartialEq)]
pub struct KeptPart {
    pub mesh: usize,
    /// Its name (`CTRL embedded_meshes[].m_Name`, the vmdl's `RenderMeshFile` name).
    pub name: String,
    /// The body group whose choices show it, and those choices.
    pub group: String,
    pub choices: Vec<String>,
}
impl KeptParts {
    /// The materials only the kept parts draw with.
    pub fn kept_materials<'a>(&self, facts: &'a ModelFacts) -> BTreeSet<&'a String> {
        let replaced: BTreeSet<&String> = self.replaced.iter().filter_map(|i| facts.meshes.get(*i)).flat_map(|m| &m.materials).collect();
        self.kept.iter().filter_map(|p| facts.meshes.get(p.mesh)).flat_map(|m| &m.materials).filter(|m| !replaced.contains(m)).collect()
    }
    /// The materials the build keeps by reference for the kept parts: `kept_materials`, and for
    /// parts shown together with the object every material the kept parts draw with (one the
    /// replaced mesh shares stays the game's on the kept part, and in every look of it).
    pub fn drawn_materials<'a>(&self, facts: &'a ModelFacts) -> BTreeSet<&'a String> {
        if !self.together {
            return self.kept_materials(facts);
        }
        self.kept.iter().filter_map(|p| facts.meshes.get(p.mesh)).flat_map(|m| &m.materials).collect()
    }
    /// The kept meshes' names, in mesh order (receipts record them).
    pub fn names(&self) -> Vec<String> {
        self.kept.iter().map(|p| p.name.clone()).collect()
    }
}
/// A body-group entry's group and choice (`group_@n_#&choice`; the older form
/// `group_@n` has no choice name).
pub fn body_group_choice(entry: &str) -> (String, String) {
    let (group, rest) = entry.split_once("_@").unwrap_or((entry, ""));
    let choice = rest.split_once("_#&").map_or("", |(_, c)| c);
    (group.to_owned(), choice.to_owned())
}
/// The ONE rule for which embedded meshes the user's model replaces when the
/// body groups' parts are bound to different bones: a mesh a default choice shows
/// (its mask shares a bit with `m_nDefaultMeshGroupMask`) is the object as it
/// normally shows and is replaced; every other mesh stays the game's own. Only
/// when exactly one mesh per level of detail is replaced, and every kept mesh is
/// shown only by choices of body groups that no replaced mesh belongs to (a part
/// that shows together with the object, such as range markers; another choice of
/// the replaced mesh's own group is another state of the object, which this does
/// not open). Decided from the masks and the body groups, never from a name;
/// `None` for every other model.
pub fn kept_parts(facts: &ModelFacts) -> Option<KeptParts> {
    let default = facts.default_mesh_group_mask?;
    if !facts.mixed_mesh_bindings || facts.mesh_groups.is_empty() || facts.mesh_groups.len() > 64 || default == 0 {
        return None;
    }
    let count = facts.meshes.len();
    if count < 2 || facts.mesh_group_masks.len() != count || facts.mesh_names.len() != count || facts.mesh_names.iter().any(String::is_empty) {
        return None;
    }
    let groups: Vec<(String, String)> = facts.mesh_groups.iter().map(|g| body_group_choice(g)).collect();
    let bits = |mask: u64| (0..groups.len()).filter(move |b| mask & (1u64 << b) != 0);
    // A mask naming no body-group entry draws nothing the groups know of.
    if facts.mesh_group_masks.iter().any(|m| *m == 0 || *m >> groups.len() != 0) {
        return None;
    }
    let replaced: Vec<usize> = (0..count).filter(|i| facts.mesh_group_masks[*i] & default != 0).collect();
    let levels = facts.levels();
    let one_per_level = if levels.is_empty() { replaced.len() == 1 } else { levels.iter().all(|l| l.iter().filter(|i| replaced.contains(i)).count() == 1) && levels.iter().flatten().collect::<BTreeSet<_>>().len() == count };
    if !one_per_level {
        return None;
    }
    let replaced_groups: BTreeSet<&String> = replaced.iter().flat_map(|i| bits(facts.mesh_group_masks[*i])).map(|b| &groups[b].0).collect();
    let mut kept = vec![];
    for mesh in (0..count).filter(|i| !replaced.contains(i)) {
        let shown: Vec<usize> = bits(facts.mesh_group_masks[mesh]).collect();
        let group = &groups[shown[0]].0;
        if shown.iter().any(|b| replaced_groups.contains(&groups[*b].0) || &groups[*b].0 != group) {
            return None;
        }
        kept.push(KeptPart { mesh, name: facts.mesh_names[mesh].clone(), group: group.clone(), choices: shown.iter().map(|b| groups[*b].1.clone()).collect() });
    }
    (!kept.is_empty()).then_some(KeptParts { replaced, kept, together: false })
}
/// The parts kept as the game's own under either rule: `kept_parts` (parts another body group
/// shows) or `object_parts::together` (parts shown together with the object, behind
/// `Switches::parts_together`) or `object_parts::state_parts` (further pieces of a state, behind
/// `Switches::state_parts`). Every caller but the offered check uses this one.
pub fn parts_kept_as_games(facts: &ModelFacts) -> Option<KeptParts> {
    kept_parts(facts).or_else(|| crate::object_parts::together(facts)).or_else(|| crate::object_parts::state_parts(facts))
}
/// The page's line for a part kept as the game's own ("Its range markers, kept as the game's own").
pub fn kept_part_line(part: &KeptPart) -> String {
    // The group names the part; a group named `default` says nothing, so then its choice.
    let name = if casefold(&part.group) == "default" { part.choices.first().cloned().unwrap_or_default() } else { part.group.clone() };
    let words = sound_tags::readable(&sound_tags::words(&name));
    format!("Its {}, kept as the game's own", casefold(&words))
}
/// The physics shape types a simple collision may hold (compiled again from
/// the decompiled model). A box is not among them: the game stores it as a
/// hull, and hull or mesh collision is only kept as the game's own physics
/// block (`kept_kinds`), never compiled again.
pub const SIMPLE_SHAPES: [&str; 2] = ["sphere", "capsule"];
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Bounds {
    pub min: [f64; 3],
    pub max: [f64; 3],
}
impl Bounds {
    pub fn size(&self) -> [f64; 3] {
        [self.max[0] - self.min[0], self.max[1] - self.min[1], self.max[2] - self.min[2]]
    }
}
fn numbers(value: &Value) -> Vec<f64> {
    value.array().map(|a| a.iter().filter_map(|n| n.number().ok()).collect()).unwrap_or_default()
}
fn count(value: Option<&Value>) -> usize {
    value.and_then(|v| v.array().ok()).map_or(0, <[Value]>::len)
}
/// The facts of a compiled model (`.vmdl_c` bytes).
pub fn facts(raw: &[u8]) -> Result<ModelFacts> {
    let mut out = ModelFacts::default();
    let layout = container::layout(raw).map_err(|e| fail(format!("Not a complete compiled model: {e}")))?;
    out.blocks = layout.blocks.iter().map(|b| b.name.clone()).collect();
    let mut materials = BTreeSet::new();
    let mut other = BTreeSet::new();
    for reference in deadlock_vpk::resource_references(raw).map_err(fail)? {
        let r = casefold(&reference);
        if r.ends_with(".vmat") || r.ends_with(".vmat_c") {
            materials.insert(r.trim_end_matches("_c").to_owned());
        } else {
            other.insert(r);
        }
    }
    out.materials = materials.into_iter().collect();
    out.other_refs = other.into_iter().collect();
    let data = container::read_blocks(raw, "DATA").map_err(|e| fail(format!("The model's DATA could not be read: {e}")))?;
    let root = &data.first().ok_or_else(|| fail("The model has no DATA block."))?.root;
    if let Some(skeleton) = root.get("m_modelSkeleton") {
        out.bones = skeleton.get("m_boneName").and_then(|v| v.array().ok()).map(|a| a.iter().filter_map(|n| n.string().ok().map(str::to_owned)).collect()).unwrap_or_default();
    }
    out.physics_refs = count(root.get("m_refPhysicsData"));
    out.anim_groups = count(root.get("m_refAnimGroups"));
    out.sequence_groups = count(root.get("m_refSequenceGroups"));
    out.anim_graph = root
        .get("m_animGraph2Refs")
        .and_then(|v| v.array().ok())
        .map(|a| a.iter().filter_map(|g| g.get("m_hGraph").and_then(|h| h.string().ok()).map(str::to_owned)).collect::<Vec<_>>().join("+"))
        .filter(|s| !s.is_empty());
    out.animated_material_values = root
        .get("m_AnimatedMaterialAttributes")
        .and_then(|v| v.array().ok())
        .map(|a| a.iter().filter_map(|m| m.get("m_AttributeName").and_then(|n| n.string().ok()).map(str::to_owned)).collect())
        .unwrap_or_default();
    out.mesh_groups = root.get("m_meshGroups").and_then(|v| v.array().ok()).map(|a| a.iter().filter_map(|g| g.string().ok().map(str::to_owned)).collect()).unwrap_or_default();
    out.material_groups = root.get("m_materialGroups").and_then(|v| v.array().ok()).map(|a| a.iter().map(|g| g.get("m_name").and_then(|n| n.string().ok()).unwrap_or_default().to_owned()).collect()).unwrap_or_default();
    if let Ok(ctrl) = container::read_blocks(raw, "CTRL") {
        if let Some(c) = ctrl.first() {
            out.embedded_meshes = count(c.root.get("embedded_meshes"));
        }
    }
    if out.embedded_meshes == 0 {
        out.embedded_meshes = count(root.get("m_refMeshes"));
    }
    out.lod_switch_distances = root.get("m_lodGroupSwitchDistances").map(numbers).unwrap_or_default();
    let masks: Vec<u64> = root
        .get("m_refLODGroupMasks")
        .and_then(|v| v.array().ok())
        .map(|a| a.iter().map(|m| m.integer().ok().map_or(0, |n| n as u64)).collect())
        .unwrap_or_default();
    out.lod_meshes = (0..out.lod_switch_distances.len().min(64)).map(|level| masks.iter().filter(|m| *m & (1u64 << level) != 0).count()).collect();
    out.physics_shapes = physics_shapes(raw);
    if out.physics_refs > 0 {
        *out.physics_shapes.entry("file".into()).or_default() += out.physics_refs;
    }
    let mut lo = [f64::MAX; 3];
    let mut hi = [f64::MIN; 3];
    if let Ok(mdats) = container::read_blocks(raw, "MDAT") {
        for (i, m) in mdats.iter().enumerate() {
            let mut materials = BTreeSet::new();
            for o in m.root.get("m_sceneObjects").and_then(|v| v.array().ok()).unwrap_or(&[]) {
                for call in o.get("m_drawCalls").and_then(|v| v.array().ok()).unwrap_or(&[]) {
                    if let Some(material) = call.get("m_material").and_then(|v| v.string().ok()) {
                        materials.insert(model_key(material));
                    }
                }
            }
            let mut own: Option<Bounds> = None;
            for o in m.root.get("m_sceneObjects").and_then(|v| v.array().ok()).unwrap_or(&[]) {
                let (mn, mx) = (o.get("m_vMinBounds").map(numbers).unwrap_or_default(), o.get("m_vMaxBounds").map(numbers).unwrap_or_default());
                if mn.len() == 3 && mx.len() == 3 {
                    let b = own.get_or_insert(Bounds { min: [mn[0], mn[1], mn[2]], max: [mx[0], mx[1], mx[2]] });
                    for k in 0..3 {
                        b.min[k] = b.min[k].min(mn[k]);
                        b.max[k] = b.max[k].max(mx[k]);
                    }
                }
            }
            out.meshes.push(MeshFacts { lod_mask: masks.get(i).copied().unwrap_or(0), materials: materials.into_iter().collect(), bounds: own });
            if i == 0 {
                for entry in m.root.get("m_attachments").and_then(|v| v.array().ok()).unwrap_or(&[]) {
                    let v = entry.get("value").unwrap_or(entry);
                    let name = v.get("m_name").and_then(|n| n.string().ok()).unwrap_or("?");
                    let bone = v.get("m_influenceNames").and_then(|n| n.array().ok()).and_then(|a| a.first()).and_then(|n| n.string().ok()).unwrap_or("?");
                    out.attachments.push(format!("{name}@{bone}"));
                }
                out.hitboxes = m
                    .root
                    .get("m_hitboxsets")
                    .and_then(|v| v.array().ok())
                    .map(|sets| sets.iter().map(|s| count(s.get("value").unwrap_or(s).get("m_HitBoxes"))).sum())
                    .unwrap_or(0);
                out.mesh_skeleton_bones = m.root.get("m_skeleton").and_then(|s| s.get("m_bones")).and_then(|b| b.array().ok()).map(<[Value]>::len);
            }
            for o in m.root.get("m_sceneObjects").and_then(|v| v.array().ok()).unwrap_or(&[]) {
                let (mn, mx) = (o.get("m_vMinBounds").map(numbers).unwrap_or_default(), o.get("m_vMaxBounds").map(numbers).unwrap_or_default());
                if mn.len() == 3 && mx.len() == 3 {
                    for k in 0..3 {
                        lo[k] = lo[k].min(mn[k]);
                        hi[k] = hi[k].max(mx[k]);
                    }
                }
            }
        }
    }
    if lo[0] <= hi[0] {
        out.bounds = Some(Bounds { min: lo, max: hi });
    }
    let mut bound = BTreeSet::new();
    for m in container::read_blocks(raw, "MDAT").unwrap_or_default() {
        for b in m.root.get("m_skeleton").and_then(|s| s.get("m_bones")).and_then(|b| b.array().ok()).unwrap_or(&[]) {
            if let Some(n) = b.get("m_boneName").and_then(|n| n.string().ok()) {
                bound.insert(n.to_owned());
            }
        }
    }
    out.mesh_bones = out.bones.iter().filter(|b| bound.contains(*b)).cloned().collect();
    out.physics_block = out.blocks.iter().any(|b| b == "PHYS");
    out.physics_on_bones = container::read_blocks(raw, "PHYS").unwrap_or_default().iter().any(|b| count(b.root.get("m_boneNames")) > 0);
    out.physics_unreadable = out.physics_block && container::read_blocks(raw, "PHYS").is_err();
    out.physics_unstated_frames = layout.blocks.iter().filter(|b| b.name == "PHYS").any(|b| container::payload(raw, b).is_ok_and(deadlock_resource::binary::unstated_frame_table));
    out.rest_sign_flips = root
        .get("m_modelSkeleton")
        .and_then(|s| s.get("m_boneRotParent"))
        .and_then(|v| v.array().ok())
        .unwrap_or(&[])
        .iter()
        .map(numbers)
        .any(|q| q.len() == 4 && rest_sign_flips([q[0], q[1], q[2], q[3]]));
    out.rest_scale = root
        .get("m_modelSkeleton")
        .and_then(|s| s.get("m_boneScaleParent"))
        .and_then(|v| v.array().ok())
        .unwrap_or(&[])
        .iter()
        .any(|s| s.number().is_ok_and(|s| !same_value(&s.to_string(), "1")));
    out.morphs = out.blocks.iter().any(|b| b == "MRPH");
    out.morph_channels_only = out.morphs && crate::object_morphs::morph_channels_only(raw);
    out.morphs_apart = out.morphs && crate::object_morphs::morphs_apart(raw, &out.bones);
    if out.physics_on_bones {
        let parents: Vec<i128> = root.get("m_modelSkeleton").and_then(|s| s.get("m_nParent")).and_then(|v| v.array().ok()).map(|a| a.iter().map(|p| p.integer().unwrap_or(i128::MIN)).collect()).unwrap_or_default();
        out.skeleton_reordered = parents.len() != out.bones.len() || depth_first(&parents).is_none_or(|order| order.iter().enumerate().any(|(i, b)| i != *b));
    }
    out.cloth =container::read_blocks(raw, "PHYS").unwrap_or_default().iter().any(|b| b.root.get("m_pFeModel").is_some_and(|f| !matches!(f.kind, deadlock_resource::Kind::Null)));
    out.cloth_bones_renamed = out.cloth && out.bones.iter().any(|b| renamed_by_compiler(b));
    out.animation_blocks = out.blocks.iter().filter(|b| ["ANIM", "ASEQ", "AGRP"].contains(&b.as_str())).cloned().collect();
    let keys = root.get("m_modelInfo").and_then(|i| i.get("m_keyValueText")).and_then(|t| t.string().ok()).unwrap_or("");
    out.break_pieces = ["break_list", "break_command_list"].iter().any(|k| keys.lines().any(|l| l.trim_start().starts_with(&format!("{k} ")) || l.trim() == *k));
    // The top-level keys of the key values (one tab in, `name = …`).
    let top: Vec<&str> = keys.lines().filter(|l| l.starts_with('\t') && !l.starts_with("\t\t") && l.contains(" = ")).filter_map(|l| l.trim().split(' ').next()).collect();
    // The compiler writes `AttachmentCameraData` first and `TaggedSoundSettings_t` last.
    out.key_values_reordered = top.iter().position(|k| *k == "AttachmentCameraData").is_some_and(|i| i > 0) || top.iter().position(|k| *k == "TaggedSoundSettings_t").is_some_and(|i| i + 1 < top.len());
    out.unnamed_body_choices = out.mesh_groups.iter().any(|g| !g.contains("_#&"));
    out.bone_constraints = top.contains(&"BoneConstraintList");
    let bindings: Vec<Vec<String>> = container::read_blocks(raw, "MDAT")
        .unwrap_or_default()
        .iter()
        .map(|m| m.root.get("m_skeleton").and_then(|s| s.get("m_bones")).and_then(|b| b.array().ok()).map(|a| a.iter().filter_map(|b| b.get("m_boneName").and_then(|n| n.string().ok()).map(str::to_owned)).collect()).unwrap_or_default())
        .collect();
    out.mixed_mesh_bindings = bindings.windows(2).any(|w| w[0] != w[1]);
    if !out.mesh_groups.is_empty() {
        out.mesh_group_masks = root
            .get("m_refMeshGroupMasks")
            .and_then(|v| v.array().ok())
            .map(|a| a.iter().map(|m| m.integer().ok().map_or(0, |n| n as u64)).collect())
            .unwrap_or_default();
        out.default_mesh_group_mask = root.get("m_nDefaultMeshGroupMask").and_then(|m| m.integer().ok()).map(|n| n as u64);
        out.mesh_names = container::read_blocks(raw, "CTRL")
            .ok()
            .and_then(|c| c.into_iter().next())
            .and_then(|c| c.root.get("embedded_meshes").and_then(|v| v.array().ok()).map(|a| a.iter().map(|m| m.get("m_Name").and_then(|n| n.string().ok()).unwrap_or_default().to_owned()).collect()))
            .unwrap_or_default();
    }
    if out.mixed_mesh_bindings {
        out.mesh_bindings = bindings;
    }
    out.animation_folders = container::read_blocks(raw, "ASEQ")
        .unwrap_or_default()
        .iter()
        .any(|s| s.root.get("m_keyValues").and_then(|k| k.get("faceposer_folders")).is_some_and(|f| f.object().is_ok_and(|rows| !rows.is_empty())));
    // The compiler writes the decode skeleton in the model skeleton's depth-first
    // order (children in index order).
    let parents: Vec<i64> = root
        .get("m_modelSkeleton")
        .and_then(|s| s.get("m_nParent"))
        .and_then(|v| v.array().ok())
        .map(|a| a.iter().map(|n| n.integer().ok().map_or(-1, |p| p as i64)).collect())
        .unwrap_or_default();
    out.mesh_binding_gaps = out.mixed_mesh_bindings && crate::object_states::binding_gaps(&out.bones, &parents, &out.mesh_bindings);
    let mut depth_first = vec![];
    let mut stack: Vec<usize> = (0..parents.len().min(out.bones.len())).filter(|i| parents[*i] < 0).rev().collect();
    while let Some(i) = stack.pop() {
        depth_first.push(out.bones[i].clone());
        stack.extend((0..parents.len().min(out.bones.len())).filter(|c| parents[*c] == i as i64).rev());
    }
    for group in container::read_blocks(raw, "AGRP").unwrap_or_default() {
        let decode: Vec<String> = group
            .root
            .get("m_decodeKey")
            .and_then(|k| k.get("m_boneArray"))
            .and_then(|a| a.array().ok())
            .map(|a| a.iter().filter_map(|b| b.get("m_name").and_then(|n| n.string().ok()).map(str::to_owned)).collect())
            .unwrap_or_default();
        let ordered: Vec<&String> = depth_first.iter().filter(|n| decode.contains(n)).collect();
        if decode.iter().collect::<Vec<_>>() != ordered {
            out.decode_reordered = true;
        }
    }
    // Parts kept as the game's own in levels of detail (`object_parts::in_levels`) only, so every
    // other model's facts keep their bytes: the resting turns that come back with the other sign
    // through their Euler angles, and cameras listed in the reverse of their attachments' order.
    if crate::object_parts::in_levels(&out).is_some() {
        let rots: Vec<Vec<f64>> = root.get("m_modelSkeleton").and_then(|s| s.get("m_boneRotParent")).and_then(|v| v.array().ok()).unwrap_or(&[]).iter().map(numbers).collect();
        out.rest_round_trip_flips = out.bones.iter().zip(&rots).filter(|(_, q)| q.len() == 4 && crate::object_parts::round_trip_flips([q[0], q[1], q[2], q[3]])).map(|(b, _)| b.clone()).collect();
        out.cameras_reversed = crate::object_parts::cameras_reversed(keys, &out.attachments);
    }
    Ok(out)
}
/// The shapes of a compiled model's physics blocks by type (see `ModelFacts::physics_shapes`).
pub fn physics_shapes(raw: &[u8]) -> BTreeMap<String, usize> {
    let mut out = BTreeMap::new();
    for block in container::read_blocks(raw, "PHYS").unwrap_or_default() {
        for part in block.root.get("m_parts").and_then(|v| v.array().ok()).unwrap_or(&[]) {
            let Some(shape) = part.get("m_rnShape") else { continue };
            for (key, name) in [("m_spheres", "sphere"), ("m_capsules", "capsule"), ("m_meshes", "mesh"), ("m_compounds", "compound")] {
                let n = count(shape.get(key));
                if n > 0 {
                    *out.entry(name.to_owned()).or_default() += n;
                }
            }
            for hull in shape.get("m_hulls").and_then(|v| v.array().ok()).unwrap_or(&[]) {
                // Points: a blob of float triples (or an array of them).
                let points = hull.get("m_Hull").and_then(|h| h.get("m_VertexPositions")).map_or(0, |p| match &p.kind {
                    deadlock_resource::Kind::Blob(b) => b.len() / 12,
                    _ => count(Some(p)),
                });
                *out.entry(if points == 8 { "hull of 8 points" } else { "hull" }.to_owned()).or_default() += 1;
            }
        }
    }
    out
}

// ------------------------------------------------------------------ the offered profile

/// What the engine may carry beyond the first proven kind (no bones, no
/// physics, no hitboxes, no attachments, one material, drawn only by effects).
/// ONE place: the catalog, the allowed list and the build use `switches()`.
/// A kind is switched on only once a file of that kind made by the engine is
/// known to work in the game (`SWITCHES` holds the ones that are).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Switches {
    /// An object with exactly one bone that the game places itself (a
    /// definition names it): the user's mesh is bound wholly to the bone.
    pub one_bone_placed: bool,
    /// An object with exactly one bone that only effects draw.
    pub one_bone_drawn: bool,
    /// A physics block made only of spheres and capsules inside the model
    /// (`SIMPLE_SHAPES`; a box is stored as a hull).
    pub simple_collision: bool,
    /// Attachment points (the game's values are taken after compiling).
    pub attachments: bool,
    /// Hit areas (hitboxes), kept and compared.
    pub hitboxes: bool,
    /// Objects the game places itself (a definition names the model).
    pub placed: bool,
    /// Several materials: the user's object brings its own.
    pub several_materials: bool,
    /// Extra meshes per level of detail (outline shells): `None` = not carried.
    pub outline_shells: Option<Shells>,
    /// Body groups (parts an effect switches between): every group and choice
    /// kept by name and order, every choice drawing the user's mesh.
    pub body_groups: bool,
    /// Several bones, animated or not: the user's model follows the game's
    /// bones in one piece or by parts (each part wholly bound to one bone).
    pub several_bones: bool,
    /// Several bones with the user's own weights on an armature named as the
    /// game object's bones: a way of a row, checked when it is built, on top
    /// of `several_bones`.
    pub same_skeleton: bool,
    /// The game's own animation blocks (`ANIM`, `ASEQ`, `AGRP`) kept byte for
    /// byte in the rebuilt model (`keep_blocks`), for an object whose animation
    /// is not played through an animation graph and whose animation data lists
    /// its bones in the rebuilt skeleton's own order.
    pub kept_animation: bool,
    /// The game's own physics block (`PHYS`) kept byte for byte, for hull or
    /// mesh collision inside the model that is bound to no bone.
    pub kept_collision: bool,
    /// On top of a kept animation: animation data that lists its bones in
    /// another order than the rebuilt skeleton (the game binds them by name).
    pub kept_animation_by_name: bool,
    /// The game's own animation blocks kept for an object played through an
    /// animation graph whose animation data a rebuild cannot reproduce
    /// (`decode_reordered`, `animation_folders`).
    pub kept_graph_animation: bool,
    /// On top of a kept collision: physics bound to the object's bones.
    pub kept_collision_on_bones: bool,
    /// Body groups whose parts are bound to different bones (`mixed_mesh_bindings`),
    /// where exactly one mesh per level of detail is the object as it normally shows
    /// and every other mesh is a part another body group shows (`kept_parts`): the
    /// user's model replaces the one, the others stay the game's own, compiled
    /// again from the decompile and bound as the game binds them.
    #[serde(default)]
    pub parts_kept: bool,
    /// The game's own resting turns written back after compiling, where the
    /// compiler writes a bone's rest rotation with the other sign (`rest_sign_flips`).
    #[serde(default)]
    pub written_rest_signs: bool,
    /// The game's own resting sizes written back after compiling (`rest_scale`).
    #[serde(default)]
    pub written_rest_scales: bool,
    /// The game's own order of its game data written back after compiling
    /// (`key_values_reordered`).
    #[serde(default)]
    pub written_key_order: bool,
    /// The game's own body-group names written back after compiling, where they
    /// are stored in the older form without a choice name (`unnamed_body_choices`).
    #[serde(default)]
    pub written_body_group_names: bool,
    /// The game's own break pieces (`break_pieces`) in the rebuilt model.
    #[serde(default)]
    pub written_break_pieces: bool,
    /// An object with bones whose physics block is stored in the form whose frame
    /// table size is stated as 0 (`physics_unstated_frames`), judged by its other
    /// facts now that the block can be read (closed as unreadable collision otherwise).
    #[serde(default)]
    pub unstated_frame_collision: bool,
    /// An object with exactly one bone whose animation is not played through an
    /// animation graph: the user's model follows the bone in one piece and the
    /// game's own animation blocks are kept (`kept_animation`).
    #[serde(default)]
    pub one_bone_animated: bool,
    /// An object without bones and without morph targets whose animation drives
    /// only material values: the game's own animation blocks are kept
    /// (`kept_animation`) and the user's model does not use those values.
    #[serde(default)]
    pub boneless_animated: bool,
    /// A model no game data names and no effect draws, beside a listed object of the
    /// hero in the same folder, whose file name is that object's followed by `_` and
    /// more (`file_name_companions`): listed under the same ability, as an object the
    /// game places itself. A new way onto the list, not a kind of object.
    #[serde(default)]
    pub listed_by_file_name: bool,
    /// A shell beside the object's mesh in each level of detail that only another
    /// choice of the object's own body group shows (`object_glow_shells::glow_shells`):
    /// generated from the user's mesh in the game's own shell material, by reference.
    #[serde(default)]
    pub body_group_shells: bool,
    /// Body groups whose choices are states of one object, one mesh per choice, the
    /// meshes bound to different bones (`object_states::states`): every state draws the
    /// user's model, bound as the game binds that state's mesh.
    #[serde(default)]
    pub body_group_states: bool,
    /// A morph block (the game changes the object's own shape while it plays) left out
    /// of the rebuild (`object_morphs::left_out`): the user's model keeps its own shape,
    /// and an animation that changes only the shape (`object_morphs::shape_animation`) is
    /// kept as the game's own blocks (`kept_animation`).
    #[serde(default)]
    pub without_morphs: bool,
    /// The game's own bone constraints (`bone_constraints`: `BoneConstraintList` in the
    /// key values) written back after compiling, where the rebuild computes their numbers again
    /// from the decompile's six-decimal angles and offsets (`write_back::WrittenKind::BoneConstraints`).
    #[serde(default)]
    pub written_bone_constraints: bool,
    /// Several meshes a choice shows together, bound to different bones (`object_parts::together`):
    /// the user's model replaces the first mesh the default choice shows (no levels of detail),
    /// every other mesh stays the game's own, compiled again from the decompile and bound as
    /// the game binds it.
    #[serde(default)]
    pub parts_together: bool,
    /// Body-group states drawn with several meshes bound to different bones (`object_parts::state_parts`):
    /// in each state the user's model replaces the first mesh, every other mesh of that state stays
    /// the game's own.
    #[serde(default)]
    pub state_parts: bool,
    /// Cloth (`m_pFeModel` in the physics block: parts the game simulates) kept as the game's
    /// own physics block, byte for byte (`KeptKind::Cloth`), on a rebuilt skeleton that is the
    /// game's bone by bone (`keep_blocks::Allowed::cloth`): the parts of the user's model that
    /// follow the bones the cloth moves swing with the game's cloth.
    #[serde(default)]
    pub kept_cloth: bool,
    /// A morph block left out of the rebuild of an object with bones that is played through an
    /// animation graph or has collision or hit areas (`object_morphs::left_out_beside_bones`):
    /// the game's animation and physics blocks kept as they are, the user's model keeps its own shape.
    #[serde(default)]
    pub without_morphs_beside_bones: bool,
    /// The game's own physics bound to bones kept on a rebuilt skeleton the compiler lists in
    /// another order (`ModelFacts::skeleton_reordered`, `KeptKind::CollisionReordered`): the same
    /// bones by name and parent, and every skeleton position the physics block can name the same
    /// bone in both (`keep_blocks::Allowed::physics_reordered`).
    #[serde(default)]
    pub kept_collision_reordered: bool,
    /// Cloth made from a cloth proxy mesh (`ModelFacts::cloth_bones_renamed`, `KeptKind::ClothProxy`):
    /// the cloth's bones, which the compiler names `_cloth_…`, take the game's `$cloth_…` names again
    /// through the hero pipeline's audited names-only repair, and the kept cloth binds to them.
    #[serde(default)]
    pub kept_cloth_proxy: bool,
    /// Parts of another body group kept as the game's own in a model with levels of detail
    /// (`object_parts::in_levels`): in every level the user's model replaces the mesh the default
    /// choices show and the other part stays the game's own, bound to exactly the bones the
    /// game's file binds it to (`object_parts::kept_bones`), the bones in the game's order, the
    /// attachments in a prefab where the game lists its cameras reversed (`ModelFacts::cameras_reversed`),
    /// and unbound resting turns written back (`write_back::WrittenKind::UnboundRestSigns`).
    #[serde(default)]
    pub parts_kept_in_levels: bool,
}
/// What the build writes in place of a level's extra meshes (outline shells).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Shells {
    /// A shell made from the user's mesh, pushed out along its normals by the
    /// game shell's own distance, faces turned as the game's are, drawn with
    /// the game's own outline material (by reference).
    Generated,
    /// No shell at all.
    Dropped,
}
impl Switches {
    /// The first proven kind alone.
    pub const OFF: Switches = Switches {
        one_bone_placed: false,
        one_bone_drawn: false,
        simple_collision: false,
        attachments: false,
        hitboxes: false,
        placed: false,
        several_materials: false,
        outline_shells: None,
        body_groups: false,
        several_bones: false,
        same_skeleton: false,
        kept_animation: false,
        kept_collision: false,
        kept_animation_by_name: false,
        kept_graph_animation: false,
        kept_collision_on_bones: false,
        parts_kept: false,
        written_rest_signs: false,
        written_rest_scales: false,
        written_key_order: false,
        written_body_group_names: false,
        written_break_pieces: false,
        unstated_frame_collision: false,
        one_bone_animated: false,
        boneless_animated: false,
        listed_by_file_name: false,
        body_group_shells: false,
        body_group_states: false,
        without_morphs: false,
        written_bone_constraints: false,
        parts_together: false,
        state_parts: false,
        kept_cloth: false,
        without_morphs_beside_bones: false,
        kept_collision_reordered: false,
        kept_cloth_proxy: false,
        parts_kept_in_levels: false,
    };
    /// Every kind the engine can carry by compiling alone (tests and development
    /// builds only), without the kinds that keep the game's own blocks (`ALL_KEPT`
    /// adds them). Kinds not yet enabled in the app stay closed here (`EVERY` opens them).
    pub const ALL: Switches = Switches {
        one_bone_placed: true,
        one_bone_drawn: true,
        simple_collision: true,
        attachments: true,
        hitboxes: true,
        placed: true,
        several_materials: true,
        outline_shells: Some(Shells::Generated),
        body_groups: true,
        several_bones: true,
        same_skeleton: true,
        kept_animation: false,
        kept_collision: false,
        kept_animation_by_name: false,
        kept_graph_animation: false,
        kept_collision_on_bones: false,
        parts_kept: false,
        written_rest_signs: false,
        written_rest_scales: false,
        written_key_order: false,
        written_body_group_names: false,
        written_break_pieces: false,
        unstated_frame_collision: false,
        one_bone_animated: false,
        boneless_animated: false,
        listed_by_file_name: false,
        body_group_shells: false,
        body_group_states: false,
        without_morphs: false,
        written_bone_constraints: false,
        parts_together: false,
        state_parts: false,
        kept_cloth: false,
        without_morphs_beside_bones: false,
        kept_collision_reordered: false,
        kept_cloth_proxy: false,
        parts_kept_in_levels: false,
    };
    /// `ALL` with every kind that keeps the game's own blocks too: every kind
    /// enabled in the app (tests and development builds only). It opens nothing `SWITCHES` does not, but `outline_shells` is
    /// the only switch with a choice, so a development build may still differ
    /// there. Kinds not yet enabled stay closed here (`EVERY` opens them).
    pub const ALL_KEPT: Switches = Switches {
        kept_animation: true,
        kept_collision: true,
        kept_animation_by_name: true,
        kept_graph_animation: true,
        kept_collision_on_bones: true,
        parts_kept: true,
        written_rest_signs: true,
        written_rest_scales: true,
        written_key_order: true,
        written_body_group_names: true,
        written_break_pieces: true,
        unstated_frame_collision: true,
        one_bone_animated: true,
        boneless_animated: true,
        listed_by_file_name: true,
        body_group_shells: true,
        body_group_states: true,
        without_morphs: true,
        written_bone_constraints: true,
        parts_together: true,
        state_parts: true,
        kept_cloth: true,
        without_morphs_beside_bones: true,
        kept_collision_reordered: true,
        parts_kept_in_levels: true,
        kept_cloth_proxy: true,
        ..Switches::ALL
    };
    /// `ALL_KEPT` with the kinds not yet switched on in the app opened as well:
    /// every kind the engine can carry (development builds and counts only).
    pub const EVERY: Switches = Switches::ALL_KEPT;
    /// Whether a kind that keeps the game's own blocks is open.
    pub fn keeps_any(&self) -> bool {
        self.kept_animation || self.kept_collision || self.kept_animation_by_name || self.kept_graph_animation || self.kept_collision_on_bones || self.kept_cloth || self.kept_collision_reordered
    }
}
/// The switches of the app: the kinds enabled for users (a placed object with
/// one bone, capsule collision,
/// attachment points and an outline drawn in the game's own outline material;
/// an effect-drawn object with collision, hit areas, attachment points and two
/// materials; an effect-drawn object with two materials; an effect-drawn object
/// with one bone; an effect-drawn object with body groups; an animated object
/// followed by parts, in one piece and with the user's own skeleton; an object
/// keeping the game's own animation blocks; an object keeping the game's own
/// hull collision; kept animation whose data binds bones by name; kept
/// animation on a graph-played object; kept collision bound to bones, on a
/// rebuilt skeleton in the game's own bone order; parts of another body group kept
/// as the game's own; the game's resting turns, resting sizes, game data order, older
/// body-group names and break pieces written back; an object animated on one bone without
/// an animation graph; an object without bones whose animation drives only
/// material values; collision stored in the form whose frame table size is
/// stated as 0, on an object with bones; a model found by its file name beside a
/// listed one, the standing wall; a shell only another choice of the object's own
/// body group shows, made from the user's mesh in the game's own glow material; body-group
/// choices that are states of one object, each state bound as the game binds it; the game's
/// own bone constraints written back; parts a choice shows together with the object kept as
/// the game's own; further pieces of a state kept as the game's own; cloth kept as the
/// game's own physics block on a skeleton that is the game's bone by bone; a morph block left
/// out beside bones, with the game's collision kept on a skeleton a rebuild lists in another
/// order; parts of another body group kept as the game's own in every level of detail; cloth made
/// from a cloth proxy mesh, its bones given the game's names back).
/// Every object these switches
/// offer was built through the real build path (offered must equal buildable).
/// The one object `written_rest_signs` would offer that does not build is held closed by
/// name (`KNOWN_NOT_TO_REBUILD`, the single exception).
/// Still off: leaving the outline out (`Shells::Dropped`, development builds only).
/// Change one only together with `the_app_s_switches_are_the_enabled_kinds`.
pub const SWITCHES: Switches = Switches {
    one_bone_placed: true,
    one_bone_drawn: true,
    simple_collision: true,
    attachments: true,
    hitboxes: true,
    placed: true,
    several_materials: true,
    outline_shells: Some(Shells::Generated),
    body_groups: true,
    several_bones: true,
    same_skeleton: true,
    kept_animation: true,
    kept_collision: true,
    kept_animation_by_name: true,
    kept_graph_animation: true,
    kept_collision_on_bones: true,
    parts_kept: true,
    written_rest_signs: true,
    written_rest_scales: true,
    written_key_order: true,
    written_body_group_names: true,
    written_break_pieces: true,
    unstated_frame_collision: true,
    one_bone_animated: true,
    boneless_animated: true,
    listed_by_file_name: true,
    body_group_shells: true,
    body_group_states: true,
    without_morphs: true,
    written_bone_constraints: true,
    parts_together: true,
    state_parts: true,
    kept_cloth: true,
    without_morphs_beside_bones: true,
    kept_collision_reordered: true,
    // Built with its own bone-constraint bound (write_back::CLOTH_PROXY_CONSTRAINT_ROUNDING).
    kept_cloth_proxy: true,
    parts_kept_in_levels: true,
};

#[cfg(feature = "object-test-switches")]
static TEST_SWITCHES: Mutex<Option<Switches>> = Mutex::new(None);
/// Opens kinds for tests only (feature `object-test-switches`,
/// enabled only by the app crate's dev-dependencies, so never in the app).
#[cfg(feature = "object-test-switches")]
pub fn open_for_test(switches: Option<Switches>) {
    *TEST_SWITCHES.lock().unwrap_or_else(|e| e.into_inner()) = switches;
}
/// The switches in force: `SWITCHES` (or a test's own, see `open_for_test`).
pub fn switches() -> Switches {
    #[cfg(feature = "object-test-switches")]
    if let Some(test) = *TEST_SWITCHES.lock().unwrap_or_else(|e| e.into_inner()) {
        return test;
    }
    SWITCHES
}

/// What an object holds beyond the first proven kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Feature {
    OneBone,
    /// Two bones or more (`Switches::several_bones`).
    Bones,
    SimpleCollision,
    /// Hulls, meshes, compounds or physics files (inside the model: kept as the
    /// game's own physics block, `Switches::kept_collision`; a physics file: never).
    OtherCollision,
    Hitboxes,
    /// On an object with several bones only (`Switches::several_bones`; without
    /// an animation graph: `Switches::kept_animation`).
    Animated,
    Attachments,
    SeveralMaterials,
    NoMaterial,
    Placed,
    /// Every level of detail: a body and an outline shell (`ModelFacts::shell_material`).
    OutlineShells,
    /// Several meshes in a level of detail that are not an outline (never carried, but for a part
    /// another body group shows, kept as the game's own in every level: `object_parts::in_levels`).
    ExtraMeshes,
    /// Body groups (`Switches::body_groups`).
    BodyGroups,
    /// Every level of detail: the object and a shell only another choice of its body
    /// group shows (`object_glow_shells::glow_shells`, `Switches::body_group_shells`).
    BodyGroupShells,
}
impl Feature {
    pub fn name(self) -> &'static str {
        match self {
            Feature::OneBone => "one bone",
            Feature::Bones => "several bones",
            Feature::SimpleCollision => "simple collision",
            Feature::OtherCollision => "hull or mesh collision",
            Feature::Hitboxes => "hit areas",
            Feature::Animated => "animation",
            Feature::Attachments => "attachment points",
            Feature::SeveralMaterials => "several materials",
            Feature::NoMaterial => "no material",
            Feature::Placed => "placed by the game",
            Feature::OutlineShells => "outline shells",
            Feature::ExtraMeshes => "other extra meshes per level of detail",
            Feature::BodyGroups => "body groups",
            Feature::BodyGroupShells => "shells another body-group choice shows",
        }
    }
}
/// The features of an object. Materials are counted without the outline
/// shells' own (`body_materials`).
pub fn features(facts: &ModelFacts, placed: bool) -> BTreeSet<Feature> {
    let mut out = BTreeSet::new();
    if facts.one_bone() {
        out.insert(Feature::OneBone);
    } else if !facts.bones.is_empty() || facts.mesh_skeleton_bones.is_some_and(|n| n > 0) {
        out.insert(Feature::Bones);
    }
    // A physics block that holds only cloth (no shape) is no collision (as in `kept_kinds`).
    if facts.has_physics() && !(facts.cloth && facts.physics_shapes.is_empty()) {
        out.insert(if facts.simple_physics() { Feature::SimpleCollision } else { Feature::OtherCollision });
    }
    if facts.hitboxes > 0 {
        out.insert(Feature::Hitboxes);
    }
    if facts.animated() {
        out.insert(Feature::Animated);
    }
    if !facts.attachments.is_empty() {
        out.insert(Feature::Attachments);
    }
    match facts.body_materials().len() {
        0 => {
            out.insert(Feature::NoMaterial);
        }
        1 => {}
        _ => {
            out.insert(Feature::SeveralMaterials);
        }
    }
    if placed {
        out.insert(Feature::Placed);
    }
    if facts.extra_meshes() {
        out.insert(if facts.shell_material().is_some() { Feature::OutlineShells } else if crate::object_glow_shells::glow_shells(facts).is_some() { Feature::BodyGroupShells } else { Feature::ExtraMeshes });
    }
    if !facts.mesh_groups.is_empty() {
        out.insert(Feature::BodyGroups);
    }
    out
}
/// Why a listed model is not offered, for the kinds only switches open.
pub const REASON_OTHER_COLLISION: &str = "Its collision is made of hull or mesh shapes; only spheres and capsules can be kept so far.";
pub const REASON_SHELLS: &str = "It has an outline drawn around it; objects with an outline cannot be replaced yet.";
pub const REASON_BODY_GROUPS: &str = "It has body groups (parts an effect switches between); such objects cannot be replaced yet.";
pub const REASON_LEGACY_ANIMATION: &str = "Its animation is not played through an animation graph; such objects cannot be replaced yet.";
pub const REASON_BONES: &str = "It has several bones; objects with more than one bone cannot be replaced yet.";
pub const REASON_BREAK_PIECES: &str = "It breaks into pieces when destroyed; such objects cannot be replaced yet.";
pub const REASON_BODY_GROUP_NAMES: &str = "Its body groups are stored in an older form that a rebuild cannot keep; such objects cannot be replaced yet.";
pub const REASON_BODY_GROUP_BINDINGS: &str = "Its body groups' parts are bound to different bones; such objects cannot be replaced yet.";
pub const REASON_ANIMATION_FOLDERS: &str = "Its animations are arranged in folders, which a rebuild cannot keep in the game's order; such objects cannot be replaced yet.";
pub const REASON_BONE_ORDER: &str = "Its animation data lists its bones in an order a rebuild cannot keep; such objects cannot be replaced yet.";
pub const REASON_CLOTH: &str = "It has cloth (parts the game simulates); objects with cloth cannot be replaced yet.";
/// Reached only once `kept_cloth` is open.
pub const REASON_CLOTH_BONE_NAMES: &str = "Its cloth moves bones whose names a rebuild writes otherwise, so the game's cloth could not find them; such objects cannot be replaced yet.";
/// The kinds that keep the game's own blocks, for the ones still closed (reached only
/// once `kept_animation`, `kept_graph_animation` or `kept_collision` is open).
pub const REASON_ANIMATION_BY_NAME: &str = "Its animation data lists its bones in another order than the object's skeleton; such objects cannot be replaced yet.";
pub const REASON_COLLISION_ON_BONES: &str = "Its collision is made of hull or mesh shapes that move with its bones; such objects cannot be replaced yet.";
pub const REASON_UNREADABLE_COLLISION: &str = "Its collision is stored in a form the app cannot read, so what it is bound to cannot be checked; such objects cannot be replaced yet.";
pub const REASON_REST_SIGN: &str = "Its skeleton stores a bone's resting turn in a form a rebuild writes otherwise; such objects cannot be replaced yet.";
pub const REASON_REST_SCALE: &str = "Its skeleton stores a bone's resting size in a form a rebuild writes otherwise; such objects cannot be replaced yet.";
pub const REASON_MORPHS: &str = "The game changes this object's own shape while it plays, which a replacement cannot carry yet.";
pub const REASON_BONE_CONSTRAINTS: &str = "Its bone constraints are stored with a precision a rebuild does not keep; such objects cannot be replaced yet.";
/// Reached only once every other check passes.
pub const REASON_COLLISION_REORDERED: &str = "Its collision moves with its bones, and a rebuild lists those bones in another order than the game's; such objects cannot be replaced yet.";
pub const REASON_GAME_DATA_ORDER: &str = "Its game data is stored in an order a rebuild cannot keep; such objects cannot be replaced yet.";

/// A kind that keeps the game's own compiled blocks in the rebuilt model (`keep_blocks`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeptKind {
    /// The animation blocks of an object not played through an animation graph.
    Animation,
    /// The animation blocks of an object played through an animation graph.
    GraphAnimation,
    /// On top of either: the animation data lists the bones in another order.
    AnimationByName,
    /// The physics block (hull or mesh collision).
    Collision,
    /// On top of it: the physics is bound to bones.
    CollisionOnBones,
    /// The physics block, for the cloth it holds (`ModelFacts::cloth`), whatever its collision.
    Cloth,
    /// On top of the kept physics bound to bones: the compiler lists the skeleton in another order
    /// than the game's (`ModelFacts::skeleton_reordered`).
    CollisionReordered,
    /// On top of the kept cloth: it is made from a cloth proxy mesh whose bones the compiler names
    /// otherwise (`ModelFacts::cloth_bones_renamed`); their names are given back after compiling.
    ClothProxy,
}
impl KeptKind {
    /// Whether `switches` open this kind.
    pub fn open(self, switches: &Switches) -> bool {
        match self {
            KeptKind::Animation => switches.kept_animation,
            KeptKind::GraphAnimation => switches.kept_graph_animation,
            KeptKind::AnimationByName => switches.kept_animation_by_name,
            KeptKind::Collision => switches.kept_collision,
            KeptKind::CollisionOnBones => switches.kept_collision_on_bones,
            KeptKind::Cloth => switches.kept_cloth,
            KeptKind::CollisionReordered => switches.kept_collision_reordered,
            KeptKind::ClothProxy => switches.kept_cloth_proxy,
        }
    }
}
/// What a rebuild of this object keeps of the game's own blocks, from the game's
/// model alone: the ONE rule for the offered kind, the build and the install check.
/// Empty for every object a rebuild reproduces (they keep today's build).
pub fn kept_kinds(facts: &ModelFacts) -> BTreeSet<KeptKind> {
    let mut out = BTreeSet::new();
    if facts.legacy_animation() {
        out.insert(KeptKind::Animation);
    } else if facts.animated() && (facts.decode_reordered || facts.animation_folders) {
        out.insert(KeptKind::GraphAnimation);
    }
    // With parts kept as the game's own (`kept_parts`) the compiler lists first the bones the
    // user's model and the kept parts use, so the rebuilt skeleton's order cannot be told from
    // the game's: the kept animation binds by name (`KeptKind::AnimationByName`).
    if !out.is_empty() && (facts.decode_reordered || parts_kept_as_games(facts).is_some()) {
        out.insert(KeptKind::AnimationByName);
    }
    // A physics block that holds only cloth (no shape) is not collision.
    let cloth_only = facts.cloth && facts.physics_shapes.is_empty();
    if facts.has_physics() && !facts.simple_physics() && !cloth_only {
        out.insert(KeptKind::Collision);
        if facts.physics_on_bones {
            out.insert(KeptKind::CollisionOnBones);
        }
    }
    // Cloth: the physics block kept for it, whatever its shapes (spheres and capsules too); its
    // shapes bound to bones as the kept collision on bones is (the same skeleton precondition).
    if facts.cloth {
        out.insert(KeptKind::Cloth);
        if facts.physics_on_bones {
            out.insert(KeptKind::CollisionOnBones);
        }
    }
    // Kept physics bound to bones on a skeleton the compiler writes in another order.
    if out.contains(&KeptKind::CollisionOnBones) && facts.skeleton_reordered {
        out.insert(KeptKind::CollisionReordered);
    }
    if facts.cloth && facts.cloth_bones_renamed {
        out.insert(KeptKind::ClothProxy);
    }
    out
}
/// The block names `kinds` keep (`keep_blocks::ANIMATION` and/or `PHYS`).
pub fn kept_block_names(kinds: &BTreeSet<KeptKind>) -> Vec<&'static str> {
    let mut out = vec![];
    if kinds.contains(&KeptKind::Animation) || kinds.contains(&KeptKind::GraphAnimation) {
        out.extend(crate::keep_blocks::ANIMATION);
    }
    if kinds.contains(&KeptKind::Collision) || kinds.contains(&KeptKind::Cloth) {
        out.push(crate::keep_blocks::PHYSICS);
    }
    out
}
/// What the kept blocks may hold under `kinds` (`keep_blocks::problems`).
pub fn kept_allowed(kinds: &BTreeSet<KeptKind>) -> crate::keep_blocks::Allowed {
    crate::keep_blocks::Allowed { decode_by_name: kinds.contains(&KeptKind::AnimationByName), physics_bones: kinds.contains(&KeptKind::CollisionOnBones), cloth: kinds.contains(&KeptKind::Cloth), physics_reordered: kinds.contains(&KeptKind::CollisionReordered), cloth_proxy: kinds.contains(&KeptKind::ClothProxy) }
}
pub const REASON_EXTRA_MESHES: &str = "It draws several meshes in one level of detail; such objects cannot be replaced yet.";

/// The fields of the model's `DATA` a rebuild of this object writes back from the
/// game's own file (`write_back`), from the game's model alone: the ONE rule for
/// the offered kind, the build and the install check. Empty for every object a
/// rebuild reproduces.
pub fn written_kinds(facts: &ModelFacts) -> BTreeSet<crate::write_back::WrittenKind> {
    use crate::write_back::WrittenKind;
    let mut out = BTreeSet::new();
    if !facts.bones.is_empty() && facts.rest_sign_flips {
        out.insert(WrittenKind::RestSigns);
    }
    if !facts.bones.is_empty() && facts.rest_scale {
        out.insert(WrittenKind::RestScales);
    }
    if facts.key_values_reordered {
        out.insert(WrittenKind::KeyOrder);
    }
    if !facts.mesh_groups.is_empty() && facts.unnamed_body_choices {
        out.insert(WrittenKind::BodyGroupNames);
    }
    if facts.break_pieces {
        out.insert(WrittenKind::BreakPieces);
    }
    if facts.bone_constraints {
        out.insert(WrittenKind::BoneConstraints);
    }
    // Parts kept as the game's own in levels of detail: the build marks only the kept parts' bones
    // to be kept, and the compiler writes an unmarked, unbound bone's resting turn through its Euler
    // angles (`object_parts::round_trip_flips`).
    if let Some(parts) = crate::object_parts::in_levels(facts) {
        let marked = crate::object_parts::kept_bones(facts, &parts);
        if facts.rest_round_trip_flips.iter().any(|b| !marked.contains(b)) {
            out.insert(WrittenKind::UnboundRestSigns);
        }
    }
    out
}
/// Whether `switches` open a written kind.
pub fn written_open(kind: crate::write_back::WrittenKind, switches: &Switches) -> bool {
    use crate::write_back::WrittenKind;
    match kind {
        WrittenKind::RestSigns => switches.written_rest_signs,
        WrittenKind::RestScales => switches.written_rest_scales,
        WrittenKind::KeyOrder => switches.written_key_order,
        WrittenKind::BodyGroupNames => switches.written_body_group_names,
        WrittenKind::BreakPieces => switches.written_break_pieces,
        WrittenKind::BoneConstraints => switches.written_bone_constraints,
        WrittenKind::UnboundRestSigns => switches.parts_kept_in_levels,
    }
}

/// The offered profile: `Ok` when an object of this kind may be replaced, else
/// the one reason sentence. `placed`: a live definition names the model as an
/// object of its own (the game spawns or places it, not only an effect).
pub fn offered(facts: &ModelFacts, placed: bool) -> std::result::Result<(), &'static str> {
    offered_with(facts, placed, &switches())
}
/// The first proven profile (policy `PROFILE`), unchanged: what is offered
/// with every switch off, reason for reason.
pub fn first_profile(facts: &ModelFacts, placed: bool) -> std::result::Result<(), &'static str> {
    if !facts.bones.is_empty() || facts.mesh_skeleton_bones.is_some_and(|n| n > 0) {
        return Err(REASON_SKELETON);
    }
    if facts.physics_block || facts.physics_refs > 0 {
        return Err(REASON_COLLISION);
    }
    if facts.hitboxes > 0 {
        return Err(REASON_HITBOXES);
    }
    if !facts.animation_blocks.is_empty() || facts.anim_graph.is_some() || facts.sequence_groups > 0 || facts.anim_groups > 0 {
        return Err(REASON_ANIMATED);
    }
    if !facts.attachments.is_empty() {
        return Err(REASON_ATTACHMENTS);
    }
    match facts.materials.len() {
        0 => return Err(REASON_NO_MATERIAL),
        1 => {}
        _ => return Err(REASON_MATERIALS),
    }
    if placed {
        return Err(REASON_PLACED);
    }
    // A correction of the first profile: body groups were never built in this kind
    // (today's build refuses them at read-back), so they are closed here too. Last,
    // so every other object keeps its reason.
    if !facts.mesh_groups.is_empty() {
        return Err(REASON_BODY_GROUPS);
    }
    Ok(())
}
/// The offered profile under `switches`: an object the first profile offers is
/// offered; with every switch off nothing else is (`first_profile`'s reason).
/// Otherwise the same order of checks, each switch letting only its own
/// feature through. Never offered under any switch: animation on an object
/// without several bones, collision in a physics file the model only names,
/// cloth, extra meshes that are not an outline, and what a rebuild is known
/// not to keep (the facts checked last).
pub fn offered_with(facts: &ModelFacts, placed: bool, switches: &Switches) -> std::result::Result<(), &'static str> {
    let first = first_profile(facts, placed);
    if first.is_ok() || *switches == Switches::OFF {
        return first;
    }
    if !facts.bones.is_empty() || facts.mesh_skeleton_bones.is_some_and(|n| n > 0) {
        // One bone: on an object the game places, or on one only effects draw, each its own switch.
        let one = if placed { switches.one_bone_placed } else { switches.one_bone_drawn };
        if !facts.one_bone() {
            // Several bones: their own switch (animated or not).
            if !switches.several_bones {
                return Err(if switches.one_bone_placed || switches.one_bone_drawn { REASON_BONES } else { REASON_SKELETON });
            }
        } else if !one {
            return Err(REASON_SKELETON);
        }
    }
    if facts.has_physics() {
        if !switches.simple_collision {
            return Err(REASON_COLLISION);
        }
        // Hull or mesh collision: only as the game's own physics block, kept unchanged
        // (inside the model; a physics file the model only names is never kept).
        if !facts.simple_physics() && !(switches.kept_collision && facts.embedded_physics()) {
            return Err(REASON_OTHER_COLLISION);
        }
        // Cloth only as the game's own physics block, kept unchanged, behind its own switch.
        if facts.cloth && !(switches.kept_cloth && facts.embedded_physics()) {
            return Err(REASON_CLOTH);
        }
        // The kept cloth binds bones by name: a bone whose name the compiler writes otherwise
        // cannot be in the rebuilt skeleton as the game's cloth names it.
        if facts.cloth && facts.cloth_bones_renamed && !switches.kept_cloth_proxy {
            return Err(REASON_CLOTH_BONE_NAMES);
        }
        if !facts.simple_physics() && facts.physics_on_bones && !switches.kept_collision_on_bones {
            return Err(REASON_COLLISION_ON_BONES);
        }
        // A physics block the reader refuses is kept only on an object without bones (nothing
        // for it to be bound to, and no cloth). A block in the form the reader reads only since
        // it learned it stays closed so on an object with bones unless its own switch is open.
        let unread = facts.physics_unreadable || facts.physics_unstated_frames && !switches.unstated_frame_collision;
        if unread && (!facts.bones.is_empty() || facts.mesh_skeleton_bones.is_some_and(|n| n > 0)) {
            return Err(REASON_UNREADABLE_COLLISION);
        }
    }
    if facts.hitboxes > 0 && !switches.hitboxes {
        return Err(REASON_HITBOXES);
    }
    // Animation on an object with several bones the switch opened; on one bone, or on none
    // (driving only material values), each behind its own switch, never with morph targets.
    let animation_open = switches.several_bones && facts.several_bones()
        || switches.one_bone_animated && facts.one_bone_animation()
        || switches.boneless_animated && facts.material_animation()
        // An animation that changes only the shape a morph block stores, which the rebuild leaves out.
        || switches.without_morphs && crate::object_morphs::shape_animation(facts);
    if facts.animated() && !animation_open {
        // An object whose own shape the game changes (a morph block) says so, not "animated".
        return Err(if facts.morphs { REASON_MORPHS } else { REASON_ANIMATED });
    }
    if facts.legacy_animation() {
        // Not played through an animation graph: the compiler's rebuild of such a block is not
        // the game's, so only the game's own animation blocks, kept unchanged.
        if !(switches.kept_animation && facts.embedded_animation()) {
            return Err(REASON_LEGACY_ANIMATION);
        }
        if (facts.decode_reordered || parts_kept_as_games(facts).is_some()) && !switches.kept_animation_by_name {
            return Err(REASON_ANIMATION_BY_NAME);
        }
    } else if facts.animated() && (facts.decode_reordered || facts.animation_folders) {
        // What a rebuild from the decompile is known not to keep in animation data, unless the
        // game's own animation blocks are kept.
        if !(switches.kept_graph_animation && facts.embedded_animation()) {
            return Err(if facts.decode_reordered { REASON_BONE_ORDER } else { REASON_ANIMATION_FOLDERS });
        }
        if (facts.decode_reordered || parts_kept_as_games(facts).is_some()) && !switches.kept_animation_by_name {
            return Err(REASON_ANIMATION_BY_NAME);
        }
    }
    if !facts.attachments.is_empty() && !switches.attachments {
        return Err(REASON_ATTACHMENTS);
    }
    // With shells carried, the shells' own outline material is not one of the object's.
    let materials = if switches.outline_shells.is_some() { facts.body_materials().len() } else { facts.materials.len() };
    match materials {
        0 => return Err(REASON_NO_MATERIAL),
        1 => {}
        _ if switches.several_materials => {}
        _ => return Err(REASON_MATERIALS),
    }
    if placed && !switches.placed {
        return Err(REASON_PLACED);
    }
    // Body groups (effects choose a body by number): their own switch.
    if !facts.mesh_groups.is_empty() && !switches.body_groups {
        return Err(REASON_BODY_GROUPS);
    }
    if !facts.mesh_groups.is_empty() && facts.unnamed_body_choices && !switches.written_body_group_names {
        return Err(REASON_BODY_GROUP_NAMES);
    }
    // Parts bound to different bones: only one mesh replaced and the other parts kept as the
    // game's own (`kept_parts`), behind their own switch.
    if !facts.mesh_groups.is_empty() && facts.mixed_mesh_bindings && !(switches.parts_kept && kept_parts(facts).is_some()) && !(switches.body_group_states && crate::object_states::states(facts).is_some()) && !(switches.parts_together && crate::object_parts::together(facts).is_some()) && !(switches.state_parts && crate::object_parts::state_parts(facts).is_some()) {
        return Err(REASON_BODY_GROUP_BINDINGS);
    }
    // A second mesh in every level that is a part of another body group, kept as the game's
    // own (`object_parts::in_levels`), behind its own switch.
    if facts.extra_meshes() && !(switches.parts_kept_in_levels && switches.parts_kept && crate::object_parts::in_levels(facts).is_some()) {
        if facts.shell_material().is_none() && !crate::object_glow_shells::opens(facts, switches) {
            return Err(REASON_EXTRA_MESHES);
        }
        if switches.outline_shells.is_none() {
            return Err(REASON_SHELLS);
        }
    }
    // What a rebuild is known not to keep in the rest of the model.
    // Each written back from the game's own file behind its own switch (`written_kinds`).
    if !facts.bones.is_empty() && facts.rest_sign_flips && !switches.written_rest_signs {
        return Err(REASON_REST_SIGN);
    }
    if !facts.bones.is_empty() && facts.rest_scale && !switches.written_rest_scales {
        return Err(REASON_REST_SCALE);
    }
    // A morph block left out of the rebuild (the user's model keeps its shape), behind its own switch.
    // On an object with bones played through an animation graph, or with collision or hit areas,
    // behind its own switch (`object_morphs::left_out_beside_bones`).
    if facts.morphs && !(switches.without_morphs && crate::object_morphs::left_out(facts)) && !(switches.without_morphs_beside_bones && crate::object_morphs::left_out_beside_bones(facts)) {
        return Err(REASON_MORPHS);
    }
    if facts.key_values_reordered && !switches.written_key_order {
        return Err(REASON_GAME_DATA_ORDER);
    }
    // A rebuild writes their numbers with other rounding: the game's own written back, behind
    // its own switch (`write_back::WrittenKind::BoneConstraints`).
    if facts.bone_constraints && !switches.written_bone_constraints {
        return Err(REASON_BONE_CONSTRAINTS);
    }
    // Last, so every other object keeps its reason.
    if facts.break_pieces && !switches.written_break_pieces {
        return Err(REASON_BREAK_PIECES);
    }
    // Kept physics bound to bones on a skeleton the compiler lists in another order, behind its own
    // switch (after every other check, so no other object's reason changes).
    if kept_kinds(facts).contains(&KeptKind::CollisionReordered) && !switches.kept_collision_reordered {
        return Err(REASON_COLLISION_REORDERED);
    }
    Ok(())
}
fn sorted(names: &[String]) -> Vec<&String> {
    let mut out: Vec<&String> = names.iter().collect();
    out.sort();
    out
}
/// What a replacement must keep: every fact but the mesh, its materials and its
/// bounds. Returns the names of the facts that differ (empty when the same).
pub fn kept_differences(original: &ModelFacts, compiled: &ModelFacts) -> Vec<&'static str> {
    let mut out = vec![];
    for (name, same) in [
        // By name: the compiler may store several bones in another order (the read-back notes it).
        ("bones", original.bones == compiled.bones || original.bones.len() > 1 && sorted(&original.bones) == sorted(&compiled.bones)),
        // Several bones: the meshes may use any of the object's bones (each binding is checked by name).
        ("mesh skeleton bones", original.mesh_skeleton_bones.unwrap_or(0) == compiled.mesh_skeleton_bones.unwrap_or(0) || original.several_bones() && compiled.mesh_skeleton_bones.unwrap_or(0) <= original.bones.len()),
        ("attachments", original.attachments == compiled.attachments),
        ("hitboxes", original.hitboxes == compiled.hitboxes),
        ("physics", original.physics_block == compiled.physics_block && original.physics_refs == compiled.physics_refs),
        ("animation blocks", original.animation_blocks == compiled.animation_blocks),
        ("animation graph", original.anim_graph == compiled.anim_graph),
        ("sequence groups", original.sequence_groups == compiled.sequence_groups),
        ("animation groups", original.anim_groups == compiled.anim_groups),
        ("mesh groups", original.mesh_groups == compiled.mesh_groups),
        ("non-material references", original.other_refs == compiled.other_refs),
    ] {
        if !same {
            out.push(name);
        }
    }
    out
}

// ------------------------------------------------------------------ particles

/// A `C_OP_RenderModels` renderer drawing a model.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Drawing {
    /// The particle file holding the renderer (compiled, casefolded).
    pub particle: String,
    /// `m_hOverrideMaterial`: the material the renderer draws the model with instead of its own.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub override_material: Option<String>,
}
/// One particle file: the particle files it names and the models it draws.
#[derive(Debug, Clone, Default)]
pub struct ParticleInfo {
    pub children: Vec<String>,
    /// (model, override material), model casefolded `.vmdl` (no `_c`).
    pub drawn: Vec<(String, Option<String>)>,
    /// (model, the looks its renderer picks) for each renderer that sets `m_nSkin` (`rendered_looks`).
    pub looks: Vec<(String, Looks)>,
}
/// The looks (material groups, by index) a model renderer's `m_nSkin` picks: from `lo`
/// to `hi`, both included; `None` when the game sets it while playing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Looks {
    pub picks: Option<(i64, i64)>,
}
/// What one `m_nSkin` holds: a plain number, a `PF_TYPE_LITERAL` parameter, or a
/// `PF_TYPE_RANDOM_UNIFORM` one (a number from min up to max, rounded down); any other
/// parameter is set while playing. `None` when the key is absent.
pub fn skin_looks(value: Option<&Value>) -> Option<Looks> {
    let value = value?;
    if let Ok(n) = value.integer() {
        let n = n as i64;
        return Some(Looks { picks: Some((n, n)) });
    }
    let kind = value.get("m_nType").and_then(|t| t.string().ok()).unwrap_or_default();
    let number = |k: &str| value.get(k).and_then(|v| v.number().ok());
    let picks = match kind {
        "PF_TYPE_LITERAL" => number("m_flLiteralValue").map(|v| (v.floor() as i64, v.floor() as i64)),
        "PF_TYPE_RANDOM_UNIFORM" => {
            let (lo, hi) = (number("m_flRandomMin").unwrap_or(0.), number("m_flRandomMax").unwrap_or(0.));
            // A number below `hi`, rounded down (at least `lo`).
            Some((lo.floor() as i64, ((hi.ceil() as i64) - 1).max(lo.floor() as i64)))
        }
        _ => None,
    };
    Some(Looks { picks })
}
/// The looks each `C_OP_RenderModels` renderer of a decoded particle system picks for each model
/// it draws (only renderers that set `m_nSkin`).
pub fn rendered_looks(root: &Value) -> Vec<(String, Looks)> {
    let mut out = vec![];
    for renderer in root.get("m_Renderers").and_then(|v| v.array().ok()).unwrap_or(&[]) {
        if renderer.get("_class").and_then(|c| c.string().ok()) != Some("C_OP_RenderModels") {
            continue;
        }
        let Some(looks) = skin_looks(renderer.get("m_nSkin")) else { continue };
        for row in renderer.get("m_ModelList").and_then(|v| v.array().ok()).unwrap_or(&[]) {
            if let Some(model) = row.get("m_model").and_then(|m| m.string().ok()).filter(|m| casefold(m).trim_end_matches("_c").ends_with(".vmdl")) {
                out.push((model_key(model), looks));
            }
        }
    }
    out
}
fn model_key(path: &str) -> String {
    casefold(path).trim_end_matches("_c").to_owned()
}
/// The models a decoded particle system draws with `C_OP_RenderModels`.
pub fn rendered_models(root: &Value) -> Vec<(String, Option<String>)> {
    let mut out = vec![];
    for renderer in root.get("m_Renderers").and_then(|v| v.array().ok()).unwrap_or(&[]) {
        if renderer.get("_class").and_then(|c| c.string().ok()) != Some("C_OP_RenderModels") {
            continue;
        }
        let material = renderer.get("m_hOverrideMaterial").and_then(|m| m.string().ok()).filter(|m| !m.is_empty()).map(|m| casefold(m).trim_end_matches("_c").to_owned());
        for row in renderer.get("m_ModelList").and_then(|v| v.array().ok()).unwrap_or(&[]) {
            if let Some(model) = row.get("m_model").and_then(|m| m.string().ok()).filter(|m| casefold(m).trim_end_matches("_c").ends_with(".vmdl")) {
                out.push((model_key(model), material.clone()));
            }
        }
    }
    out
}
/// A compiled particle file's children and drawn models. Only a file whose
/// references name a model is decoded.
pub fn particle_info(raw: &[u8]) -> ParticleInfo {
    let refs = deadlock_vpk::resource_references(raw).unwrap_or_default();
    let mut out = ParticleInfo::default();
    let mut models = false;
    for r in refs {
        let r = casefold(&r);
        if r.ends_with(".vpcf") || r.ends_with(".vpcf_c") {
            out.children.push(format!("{}_c", r.trim_end_matches("_c")));
        } else if r.ends_with(".vmdl") || r.ends_with(".vmdl_c") {
            models = true;
        }
    }
    if models {
        if let Ok(doc) = crate::effect_edit::document(raw) {
            out.drawn = rendered_models(&doc.root);
            out.looks = rendered_looks(&doc.root);
        }
    }
    out
}

// ------------------------------------------------------------------ the game

/// What the catalog reads from the game (an archive, or test data).
pub trait Game {
    fn particle(&self, path: &str) -> Option<Arc<ParticleInfo>>;
    /// `None` when the archive has no such model; `Some(Err)` when it cannot be read.
    fn model(&self, model: &str) -> Option<std::result::Result<Arc<ModelFacts>, String>>;
    /// The models (`.vmdl`, casefolded, without `_c`) directly in `folder` whose file name
    /// starts with `prefix` (both casefolded); none unless the game lists its files.
    fn models_named(&self, _folder: &str, _prefix: &str) -> Vec<String> {
        vec![]
    }
}
/// Everything read once per game archive: the definitions, names and the
/// particle and model files read so far.
pub struct GameData {
    pub key: String,
    pub defs: Definitions,
    pub names: Names,
    particles: Mutex<BTreeMap<String, Option<Arc<ParticleInfo>>>>,
    models: Mutex<BTreeMap<String, Option<std::result::Result<Arc<ModelFacts>, String>>>>,
    users: Mutex<Option<Arc<Users>>>,
}
impl GameData {
    pub fn new(key: String, defs: Definitions, names: Names) -> Self {
        Self { key, defs, names, particles: Mutex::default(), models: Mutex::default(), users: Mutex::default() }
    }
}
/// The game data of an archive, read once and kept until the archive changes.
pub fn game_data(archive: &Vpk, cancel: &CancellationToken) -> Result<Arc<GameData>> {
    static KEPT: Mutex<Option<Arc<GameData>>> = Mutex::new(None);
    let key = effect_catalog::archive_key(archive);
    if let Some(data) = KEPT.lock().unwrap_or_else(|e| e.into_inner()).as_ref().filter(|d| d.key == key) {
        return Ok(data.clone());
    }
    let defs = Definitions::read(archive, cancel)?.ok_or_else(|| fail("The game has no hero or ability definitions (scripts/heroes.vdata_c, scripts/abilities.vdata_c)."))?;
    let data = Arc::new(GameData::new(key, defs, Names::beside(&archive.path)));
    *KEPT.lock().unwrap_or_else(|e| e.into_inner()) = Some(data.clone());
    Ok(data)
}
/// `GameData` reading from its archive.
pub struct ArchiveGame<'a> {
    pub data: &'a GameData,
    pub archive: &'a Vpk,
}
impl Game for ArchiveGame<'_> {
    fn particle(&self, path: &str) -> Option<Arc<ParticleInfo>> {
        let key = casefold(path);
        if let Some(hit) = self.data.particles.lock().unwrap_or_else(|e| e.into_inner()).get(&key) {
            return hit.clone();
        }
        let info = self.archive.entries.contains_key(&key).then(|| crate::read(self.archive, &key).ok().map(|raw| Arc::new(particle_info(&raw)))).flatten();
        self.data.particles.lock().unwrap_or_else(|e| e.into_inner()).insert(key, info.clone());
        info
    }
    fn model(&self, model: &str) -> Option<std::result::Result<Arc<ModelFacts>, String>> {
        let key = model_key(model);
        if let Some(hit) = self.data.models.lock().unwrap_or_else(|e| e.into_inner()).get(&key) {
            return hit.clone();
        }
        let compiled = format!("{key}_c");
        let found = self.archive.entries.contains_key(&compiled).then(|| match crate::read(self.archive, &compiled).and_then(|raw| facts(&raw)) {
            Ok(f) => Ok(Arc::new(f)),
            Err(e) => Err(e.to_string()),
        });
        self.data.models.lock().unwrap_or_else(|e| e.into_inner()).insert(key, found.clone());
        found
    }
    fn models_named(&self, folder: &str, prefix: &str) -> Vec<String> {
        let start = format!("{folder}/{prefix}");
        self.archive
            .entries
            .range(start.clone()..)
            .map(|(k, _)| k)
            .take_while(|k| k.starts_with(&start))
            .filter(|k| k.ends_with(".vmdl_c") && !k[folder.len() + 1..].contains('/'))
            .map(|k| k.trim_end_matches("_c").to_owned())
            .collect()
    }
}

/// The page's line for an object found by its file name (`file_name_companions`); `{}` is the
/// readable name of the listed object it sits beside.
pub const FOUND_BY_FILE_NAME: &str = "Found by its file name, next to “{}”. No game data names it: the game places it itself, and the data does not say when it shows.";
/// The name of an object found by its file name: never one of its own, always that of the
/// listed object it sits beside (`{}`, the same readable name `FOUND_BY_FILE_NAME` quotes).
pub const NAMED_AS_PLACED: &str = "{}, as placed";
/// `ObjectRow::drawn` of an object found by its file name.
pub const DRAWN_FILE_NAME: &str = "file_name";
/// The models found by their file name (`Switches::listed_by_file_name`), from the archive's
/// file list and the listed models alone: for each listed model, the models directly in its
/// folder whose file name is the listed one's followed by `_` and more, that are not listed
/// themselves, that no game data knows (`known`: a definition names it, or an effect draws
/// it), and that hold collision of their own (a physics block or file: what the game's code
/// places in the world to stand, block or be hit; a model without it that nothing names or
/// draws may be a copy of another's geometry, or unused, and is never listed). Returns each
/// such model with the listed model it extends (the longest name when several do). The one
/// rule; never decided from a particular name.
pub fn file_name_companions(game: &dyn Game, listed: &BTreeSet<String>, known: &dyn Fn(&str) -> bool) -> BTreeMap<String, String> {
    let mut out: BTreeMap<String, String> = BTreeMap::new();
    for model in listed {
        let Some((folder, file)) = model.rsplit_once('/') else { continue };
        let stem = file.trim_end_matches(".vmdl");
        if stem.is_empty() || stem == file {
            continue;
        }
        for found in game.models_named(folder, &format!("{stem}_")) {
            if listed.contains(&found) || known(&found) || !matches!(game.model(&found), Some(Ok(f)) if f.has_physics()) {
                continue;
            }
            let e = out.entry(found).or_insert_with(|| model.clone());
            if model.len() > e.len() {
                *e = model.clone();
            }
        }
    }
    out
}

fn reach(defs: &Definitions, starts: impl IntoIterator<Item = String>) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    let mut queue: VecDeque<String> = starts.into_iter().collect();
    while let Some(name) = queue.pop_front() {
        if seen.len() >= MAX_REACH || !seen.insert(name.clone()) {
            continue;
        }
        if let Some(record) = defs.records.get(&name) {
            queue.extend(record.links.iter().cloned());
        }
    }
    seen
}
/// The models a particle tree draws: (model, drawing).
fn tree_drawings(game: &dyn Game, top: &str) -> Vec<(String, Drawing)> {
    let mut seen = BTreeSet::new();
    let mut queue = VecDeque::from([casefold(top)]);
    let mut out = vec![];
    while let Some(path) = queue.pop_front() {
        if seen.len() >= MAX_TREE || !seen.insert(path.clone()) {
            continue;
        }
        let Some(info) = game.particle(&path) else { continue };
        for (model, material) in &info.drawn {
            out.push((model.clone(), Drawing { particle: path.clone(), override_material: material.clone() }));
        }
        queue.extend(info.children.iter().cloned());
    }
    out
}
/// How a record set reaches models: def-named models and particle drawings.
#[derive(Default)]
struct Reached {
    /// model → records naming it.
    named: BTreeMap<String, BTreeSet<String>>,
    /// model → drawings.
    drawn: BTreeMap<String, BTreeSet<Drawing>>,
}
fn reached(game: &dyn Game, defs: &Definitions, records: &BTreeSet<String>) -> Reached {
    let mut out = Reached::default();
    for name in records {
        let Some(record) = defs.records.get(name) else { continue };
        for model in &record.models {
            out.named.entry(model.clone()).or_default().insert(name.clone());
        }
        for particle in &record.particles {
            for (model, drawing) in tree_drawings(game, particle) {
                out.drawn.entry(model).or_default().insert(drawing);
            }
        }
    }
    out
}
/// Live heroes (display names) per model, and the live drawings and naming records.
#[derive(Default)]
pub struct Users {
    pub heroes: BTreeMap<String, BTreeSet<String>>,
    pub drawings: BTreeMap<String, BTreeSet<Drawing>>,
    /// model → live records naming it as a model of their own.
    pub placed: BTreeMap<String, BTreeSet<String>>,
}
fn in_roster(roster: &[heroes::Hero], model: &str) -> bool {
    roster.iter().any(|h| model_key(&h.path) == model || h.extras.iter().any(|e| model_key(&e.path) == model))
}
fn hero_name(roster: &[heroes::Hero], model: &str, key: &str) -> String {
    roster
        .iter()
        .find(|h| model_key(&h.path) == model || h.extras.iter().any(|e| model_key(&e.path) == model))
        .map(|h| h.name.clone())
        .unwrap_or_else(|| effect_catalog::readable_internal(key, &[]))
}
/// Who uses each model among the live heroes (roster-wide).
pub fn users(game: &dyn Game, defs: &Definitions, roster: &[heroes::Hero], cancel: &CancellationToken) -> Result<Users> {
    let mut out = Users::default();
    for hero in defs.heroes.iter().filter(|h| !h.disabled && in_roster(roster, &h.model)) {
        cancel.check()?;
        let name = hero_name(roster, &hero.model, &hero.key);
        let records = reach(defs, std::iter::once(hero.key.clone()).chain(hero.bound.iter().map(|(_, a)| a.clone())));
        let found = reached(game, defs, &records);
        for (model, by) in found.named {
            out.heroes.entry(model.clone()).or_default().insert(name.clone());
            out.placed.entry(model).or_default().extend(by);
        }
        for (model, drawings) in found.drawn {
            out.heroes.entry(model.clone()).or_default().insert(name.clone());
            out.drawings.entry(model).or_default().extend(drawings);
        }
    }
    // Records of other definition files (NPC units, modifiers, …) that place a model, unless a test.
    for (name, record) in &defs.records {
        if record.file != "abilities" && record.file != "heroes" && !sound_tags::words(name).iter().any(|w| w == "test") {
            for model in &record.models {
                out.placed.entry(model.clone()).or_default().insert(name.clone());
            }
        }
    }
    Ok(out)
}
fn kept_users(data: &GameData, game: &dyn Game, roster: &[heroes::Hero], cancel: &CancellationToken) -> Result<Arc<Users>> {
    if let Some(kept) = data.users.lock().unwrap_or_else(|e| e.into_inner()).clone() {
        return Ok(kept);
    }
    let made = Arc::new(users(game, &data.defs, roster, cancel)?);
    *data.users.lock().unwrap_or_else(|e| e.into_inner()) = Some(made.clone());
    Ok(made)
}

// ------------------------------------------------------------------ the catalog

/// The structure the page shows (from `ModelFacts`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Structure {
    pub bones: usize,
    pub meshes: usize,
    pub physics: bool,
    pub hitboxes: usize,
    pub animated: bool,
    pub attachments: usize,
    pub materials: Vec<String>,
}
/// One listed ability object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObjectRow {
    /// The game model (`.vmdl`, casefolded), the saved row's `target`.
    pub target: String,
    /// A readable name made from the file name.
    pub name: String,
    /// The file stem.
    pub file: String,
    /// The ability group in the Effects page's order (`effect_catalog::slot_order`).
    pub group: u32,
    /// The bound slot (`ESlot_Signature_1`, …).
    pub slot: String,
    /// The bound ability record and its readable name.
    pub ability: String,
    pub ability_name: String,
    /// `particle`: drawn only by effects; `definition`: an ability names it as a
    /// model of its own; `definition_and_particle`; `file_name` (`DRAWN_FILE_NAME`): no
    /// game data names it, found by its file name beside a listed object.
    pub drawn: String,
    /// For an object found by its file name: the page's one line saying so
    /// (`FOUND_BY_FILE_NAME`); absent for every other row, so they keep their bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub found_by: Option<String>,
    /// The hero's own effects that draw it.
    pub drawn_by: Vec<Drawing>,
    /// The first material any live drawing renderer draws it with instead of its own.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub material_override: Option<String>,
    /// Every live drawing renderer overrides the material (the user's material never shows).
    pub material_always_overridden: bool,
    /// Effects draw it in one of the game's own looks of it (`game_look`); absent otherwise,
    /// so every other row keeps its bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_look: Option<GameLook>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub structure: Option<Structure>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bounds: Option<Bounds>,
    /// Other live heroes that use the same file.
    pub also_used_by: Vec<String>,
    pub offered: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// What kind of object it is, for an offered object a switch opened
    /// (absent for the first proven kind, so such rows keep their bytes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<ObjectKind>,
}
/// An object that effects draw in one of the game's own looks of it (the page shows `line` as it is).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GameLook {
    /// Every live drawing does (or overrides the material): the user's colours never show.
    pub always: bool,
    pub line: String,
}
pub const GAME_LOOK_ALWAYS: &str = "The game's effects draw this object in one of the game's own looks of it, with the game's materials for that look. Your object's shape shows in game; its colors and images do not.";
pub const GAME_LOOK_SOMETIMES: &str = "Some of the game's effects draw this object in one of the game's own looks of it, with the game's materials for that look. There your object's shape shows, but not its colors and images.";
/// What the effects' choice of look does to a replaced object: the ONE rule. A renderer's
/// `m_nSkin` picks a look (material group) of the model by index; only where the build
/// points the model's other looks at the user's materials (`remaps_material_groups`) does a
/// look other than the first draw the user's shape with the game's materials of that look.
/// A pick past the model's looks, or of the first, shows the user's own materials. `drawn`:
/// each drawing with its override material and the looks its renderer picks (`None`: no
/// `m_nSkin`); `live` marks the live ones. `None` when no drawing can show a game look.
pub fn game_look(facts: &ModelFacts, drawn: &[(bool, bool, Option<Looks>)]) -> Option<GameLook> {
    let groups = facts.material_groups.len() as i64;
    if !facts.remaps_material_groups() || groups < 2 {
        return None;
    }
    // The build leaves every look's change of a material the kept parts draw with as the game's
    // (`KeptParts::drawn_materials`): when every material the model's meshes draw with is one of
    // them, no look reaches the user's materials, and every look shows the user's own colors.
    if let Some(parts) = parts_kept_as_games(facts) {
        let kept = parts.drawn_materials(facts);
        if facts.meshes.iter().flat_map(|m| &m.materials).all(|m| kept.contains(m)) {
            return None;
        }
    }
    // Always / maybe in a look of the game's (index 1 to groups - 1); a look the game sets while playing may be.
    let always = |l: &Option<Looks>| l.is_some_and(|l| l.picks.is_some_and(|(lo, hi)| lo >= 1 && hi < groups));
    let maybe = |l: &Option<Looks>| l.is_some_and(|l| l.picks.is_none_or(|(lo, hi)| hi >= 1 && lo < groups));
    if !drawn.iter().any(|(_, _, l)| maybe(l)) {
        return None;
    }
    let live: Vec<_> = drawn.iter().filter(|(live, _, _)| *live).collect();
    let every = !live.is_empty() && live.iter().all(|(_, overridden, l)| *overridden || always(l)) && live.iter().any(|(_, _, l)| always(l));
    Some(GameLook { always: every, line: if every { GAME_LOOK_ALWAYS } else { GAME_LOOK_SOMETIMES }.to_owned() })
}
/// The kind of an offered object beyond the first proven kind (the page shows it as it is).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObjectKind {
    pub features: Vec<Feature>,
    /// One line naming the kind ("An object the game places, with collision and one bone").
    pub label: String,
    /// What stays the game's for this kind, one item each.
    pub kept: Vec<String>,
    /// Has collision: the fit view draws the game's collision shapes.
    pub collision: bool,
    /// The fit may stand the object on the game object's base (collision or placed).
    pub stands_on: bool,
    /// An object with several bones: its moving parts (absent otherwise, so other kinds keep their bytes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub moving: Option<Moving>,
    /// What the build keeps of the game's own compiled data unchanged
    /// (`GAME_BLOCKS_ANIMATION`, `GAME_BLOCKS_COLLISION`); absent for every
    /// other kind, so such rows keep their bytes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub game_blocks: Vec<String>,
}
/// Items of `ObjectKind::game_blocks`.
pub const GAME_BLOCKS_ANIMATION: &str = "animation";
pub const GAME_BLOCKS_COLLISION: &str = "collision";
/// The moving parts of an object with several bones.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Moving {
    /// The bones that carry the game object's mesh, in skeleton order.
    pub bones: Vec<String>,
    /// Every bone of the object.
    pub skeleton: usize,
    pub animated: bool,
    /// Body-group states (`object_states`): the build binds each state by that state's own
    /// bones, so no bone is picked by hand; the line the page shows instead
    /// (`object_states::PER_STATE_LINE`). Absent for every other object, so such rows keep their bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub per_state: Option<String>,
}
/// Items of `ObjectKind::kept` (the page shows them as they are).
pub const KEPT_COLLISION: &str = "Its collision: what players bump into, stand on and shoot";
pub const KEPT_PLACED: &str = "When it appears and how long it stays";
pub const KEPT_ATTACHMENTS: &str = "The points the game attaches effects to";
pub const KEPT_HITBOXES: &str = "Its hit areas";
pub const KEPT_BONE: &str = "Its bone: your object moves as the game's does";
pub const KEPT_ANIMATIONS: &str = "Its animations, and when the game plays them";
pub const KEPT_BONES: &str = "Its bones: each part of yours moves and turns with the bone it follows";
pub const KEPT_BODY_GROUPS: &str = "Its body groups: whichever part an effect picks shows your object";
/// For an object with parts kept as the game's own (`kept_parts`).
pub const KEPT_BODY_GROUP_PARTS: &str = "Its body groups: your object shows where the game's object normally shows";
/// The game's own break pieces (`written_kinds`). The other written-back fields only
/// describe how the file stores something (resting turns and sizes, the order of the
/// game data, the older body-group names): the receipt records them, the page does not.
pub const KEPT_BREAK_PIECES: &str = "The pieces it breaks into when destroyed: the game's own pieces, not pieces of your object";
pub const KEPT_GAME_COLLISION: &str = "Its collision, kept unchanged from the game's own object: what players bump into, stand on and shoot";
pub const KEPT_GAME_ANIMATIONS: &str = "Its animations, kept unchanged from the game's own object, and when the game plays them";
/// For an object whose cloth is kept as the game's own (`KeptKind::Cloth`).
pub const KEPT_GAME_CLOTH: &str = "Its cloth, kept unchanged from the game's own object: only the parts of your object that follow the bones the cloth moves swing like cloth; the rest of it does not";
/// For an object without bones whose animation drives only material values
/// (`ModelFacts::material_animation`): what the replacement loses, when the value
/// is the game material's vertex jitter (`…Jitter…`), and for any other value.
pub const KEPT_MATERIAL_ANIMATION_JITTER: &str = "Its animation, kept unchanged from the game's own object. While it appears, the game makes the surface of its own object shake. Your object does not shake: it keeps its shape";
pub const KEPT_MATERIAL_ANIMATION: &str = "Its animation, kept unchanged from the game's own object. It only changes a setting of the game's own material, so your object does not show that change";
/// The kept line for an object whose animation drives only material values.
pub fn material_animation_line(facts: &ModelFacts) -> &'static str {
    if facts.animated_material_values.iter().all(|v| casefold(v).contains("jitter")) { KEPT_MATERIAL_ANIMATION_JITTER } else { KEPT_MATERIAL_ANIMATION }
}
/// The kind of an offered object (`None` for the first proven kind).
pub fn object_kind(facts: &ModelFacts, placed: bool) -> Option<ObjectKind> {
    let found = features(facts, placed);
    if found.is_empty() {
        return None;
    }
    let has = |f: Feature| found.contains(&f);
    // The game's own blocks the build keeps (absent from the row for every other kind).
    let keeps = kept_kinds(facts);
    let game_animation = keeps.contains(&KeptKind::Animation) || keeps.contains(&KeptKind::GraphAnimation);
    let game_collision = keeps.contains(&KeptKind::Collision);
    let collision = has(Feature::SimpleCollision) || game_collision;
    let mut with = vec![];
    if collision {
        with.push("collision");
    }
    if has(Feature::OneBone) {
        with.push("one bone");
    }
    if has(Feature::Attachments) {
        with.push("attachment points");
    }
    if has(Feature::Hitboxes) {
        with.push("hit areas");
    }
    if has(Feature::SeveralMaterials) {
        with.push("several materials");
    }
    if has(Feature::OutlineShells) {
        with.push("an outline");
    }
    if has(Feature::BodyGroups) {
        with.push("body groups");
    }
    // Animated on one bone or on none (several bones name their own label).
    if has(Feature::Animated) && !has(Feature::Bones) {
        with.push("an animation");
    }
    let subject = if has(Feature::Placed) { "An object the game places" } else { "An object an effect draws" };
    let per_state = crate::object_states::states(facts).map(|_| crate::object_states::PER_STATE_LINE.to_owned());
    let moving = has(Feature::Bones).then(|| Moving { bones: facts.mesh_bones.clone(), skeleton: facts.bones.len(), animated: has(Feature::Animated), per_state });
    let label = match (&moving, with.as_slice()) {
        (Some(m), _) => format!("{} · {} moving parts", if m.animated { "Animated object" } else { "Object with bones" }, m.bones.len().max(1)),
        (None, with) => match with {
        [] => subject.to_owned(),
        [one] => format!("{subject}, with {one}"),
        [rest @ .., last] => format!("{subject}, with {} and {last}", rest.join(", ")),
        },
    };
    let mut kept = vec![];
    if collision {
        kept.push(if game_collision { KEPT_GAME_COLLISION } else { KEPT_COLLISION }.to_owned());
    }
    // The game's own cloth (absent for every other kind, so such rows keep their bytes).
    if keeps.contains(&KeptKind::Cloth) {
        kept.push(KEPT_GAME_CLOTH.to_owned());
    }
    if has(Feature::Placed) {
        kept.push(KEPT_PLACED.to_owned());
    }
    if has(Feature::Attachments) {
        kept.push(KEPT_ATTACHMENTS.to_owned());
    }
    if has(Feature::Hitboxes) {
        kept.push(KEPT_HITBOXES.to_owned());
    }
    if has(Feature::OneBone) {
        kept.push(KEPT_BONE.to_owned());
    }
    // Parts that stay the game's own (absent for every other kind, so such rows keep their bytes).
    let parts = parts_kept_as_games(facts);
    if has(Feature::BodyGroups) {
        // Every choice shows a replaced mesh (states each drawn with the user's model, its further
        // pieces kept, `object_parts::state_parts`): whichever the effect picks shows the user's object.
        let every_choice = |p: &KeptParts| (0..facts.mesh_groups.len()).all(|b| p.replaced.iter().any(|r| facts.mesh_group_masks.get(*r).is_some_and(|m| 1u64.checked_shl(b as u32).is_some_and(|bit| m & bit != 0))));
        kept.push(if parts.as_ref().is_some_and(|p| !every_choice(p)) { KEPT_BODY_GROUP_PARTS } else { KEPT_BODY_GROUPS }.to_owned());
    }
    if let Some(parts) = &parts {
        if parts.together {
            // Parts shown together with the object: each named once (its levels of detail alike).
            for line in parts.kept.iter().map(crate::object_parts::kept_line) {
                if !kept.contains(&line) {
                    kept.push(line);
                }
            }
        } else if crate::object_parts::in_levels(facts).is_some() {
            // Parts kept in every level of detail: each named once.
            for line in parts.kept.iter().map(kept_part_line) {
                if !kept.contains(&line) {
                    kept.push(line);
                }
            }
        } else {
            kept.extend(parts.kept.iter().map(kept_part_line));
        }
    }
    kept.extend(crate::object_glow_shells::kept_line(facts));
    // The game's own break pieces (absent for every other kind). The other written-back
    // fields are file detail a user cannot see: only the receipt records them.
    if written_kinds(facts).contains(&crate::write_back::WrittenKind::BreakPieces) {
        kept.push(KEPT_BREAK_PIECES.to_owned());
    }
    if moving.is_some() {
        if has(Feature::Animated) {
            kept.insert(0, if game_animation { KEPT_GAME_ANIMATIONS } else { KEPT_ANIMATIONS }.to_owned());
        }
        kept.push(KEPT_BONES.to_owned());
    } else if has(Feature::Animated) {
        // One bone: the animation moves the bone your object follows. No bone: it only
        // changes the game's own material, which your object does not have.
        // A shape-only animation (`object_morphs`) is the game's own animation like any other.
        kept.insert(0, if facts.boneless() && !facts.morphs { material_animation_line(facts) } else { KEPT_GAME_ANIMATIONS }.to_owned());
    }
    // The shape the game changes and the user's model does not (absent for every other kind).
    if facts.morphs && crate::object_morphs::left_out_any(facts) {
        kept.insert(1.min(kept.len()), crate::object_morphs::KEPT_SHAPE.to_owned());
    }
    let stands_on = collision || has(Feature::Placed);
    let mut game_blocks = vec![];
    if game_animation {
        game_blocks.push(GAME_BLOCKS_ANIMATION.to_owned());
    }
    if game_collision {
        game_blocks.push(GAME_BLOCKS_COLLISION.to_owned());
    }
    Some(ObjectKind { features: found.iter().copied().collect(), label, kept, collision, stands_on, moving, game_blocks })
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Counts {
    pub listed: usize,
    pub offered: usize,
    /// Generic shapes left out (`SHARED_SHAPE_HEROES` or more live heroes).
    pub shared_shapes: usize,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObjectCatalog {
    pub hero_path: String,
    pub profile: String,
    pub rows: Vec<ObjectRow>,
    pub counts: Counts,
    /// The listed targets that are scenery models (`is_scenery`), sorted; absent
    /// when there are none. Kept beside the rows, never in them, so rows keep their bytes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scenery: Vec<String>,
}
/// The folders the game keeps its ability, effect, weapon, item, interface and
/// gameplay models in (every `models/heroes…/` folder counts as well).
pub const GAMEPLAY_MODEL_FOLDERS: [&str; 7] = ["models/abilities/", "models/particle/", "models/weapons/", "models/w_models/", "models/upgrades/", "models/ui/", "models/props_gameplay/"];
/// Whether a listed model is part of the game's scenery: its path is outside
/// `GAMEPLAY_MODEL_FOLDERS` and every `models/heroes…/` folder. Such a file may
/// also stand in a map, and "Also used by" (the definitions) cannot see that, so
/// the page says so. The one rule; the page only shows it.
pub fn is_scenery(target: &str) -> bool {
    let path = casefold(target).replace('\\', "/");
    let heroes = path.strip_prefix("models/").and_then(|rest| rest.split_once('/')).is_some_and(|(folder, _)| folder.starts_with("heroes"));
    !heroes && !GAMEPLAY_MODEL_FOLDERS.iter().any(|f| path.starts_with(f))
}
/// The hero's definitions, as `effect_catalog::Usage` finds them: those loading
/// the model, those named after the hero when several (or none) do.
fn own_definitions<'a>(defs: &'a Definitions, model: &str) -> Result<Vec<&'a effect_catalog::HeroDef>> {
    let key = model_key(model);
    let tokens = heroes::tokens(model)?;
    let exact: Vec<_> = defs.heroes.iter().filter(|h| h.model == key).collect();
    let named: Vec<_> = exact.iter().copied().filter(|h| tokens.contains(h.key.strip_prefix("hero_").unwrap_or(&h.key))).collect();
    Ok(if !named.is_empty() {
        named
    } else if !exact.is_empty() {
        exact
    } else {
        defs.heroes.iter().filter(|h| tokens.contains(h.key.strip_prefix("hero_").unwrap_or(&h.key))).collect()
    })
}
/// A readable name from a model's file stem (hero words, a leading `fx` and trailing
/// `model`/`fx` left out).
pub fn readable_name(stem: &str, hero_words: &[Vec<String>]) -> String {
    let mut words = sound_tags::strip_leading(sound_tags::words(stem), hero_words, &["models", "abilities", "ability"]);
    while words.len() > 1 && words.last().is_some_and(|w| ["model", "fx", "mdl", "models"].contains(&w.as_str())) {
        words.pop();
    }
    while words.len() > 1 && words.first().is_some_and(|w| w == "fx") {
        words.remove(0);
    }
    if words.is_empty() {
        words = sound_tags::words(stem);
    }
    sound_tags::readable(&words)
}
/// The catalog of one hero from game data (`game`, its definitions and names).
pub fn hero_catalog(game: &dyn Game, defs: &Definitions, names: &Names, users: &Users, hero_model: &str, roster: &[heroes::Hero], cancel: &CancellationToken) -> Result<ObjectCatalog> {
    hero_catalog_with(game, defs, names, users, hero_model, roster, &switches(), cancel)
}
/// `hero_catalog` under `switches` (the development counts compare them).
#[allow(clippy::too_many_arguments)]
pub fn hero_catalog_with(game: &dyn Game, defs: &Definitions, names: &Names, users: &Users, hero_model: &str, roster: &[heroes::Hero], switches: &Switches, cancel: &CancellationToken) -> Result<ObjectCatalog> {
    let hero_models: BTreeSet<String> = roster.iter().flat_map(|h| std::iter::once(model_key(&h.path)).chain(h.extras.iter().map(|e| model_key(&e.path)))).collect();
    let hero_words = sound_tags::token_words(&heroes::tokens(hero_model)?);
    let mut slots: Vec<(String, String)> = own_definitions(defs, hero_model)?.iter().flat_map(|h| h.bound.iter().cloned()).collect();
    slots.sort_by_key(|(slot, root)| (effect_catalog::slot_order(slot), slot.clone(), root.clone()));
    // model → (first slot, root), the hero's own drawings, whether an own record names it.
    let mut found: BTreeMap<String, ((String, String), BTreeSet<Drawing>, bool)> = BTreeMap::new();
    for (slot, root) in &slots {
        cancel.check()?;
        let records = reach(defs, [root.clone()]);
        let hit = reached(game, defs, &records);
        for model in hit.named.keys() {
            found.entry(model.clone()).or_insert_with(|| ((slot.clone(), root.clone()), BTreeSet::new(), false)).2 = true;
        }
        for (model, drawings) in hit.drawn {
            found.entry(model).or_insert_with(|| ((slot.clone(), root.clone()), BTreeSet::new(), false)).1.extend(drawings);
        }
    }
    // Models found by their file name beside a listed one (absent unless the switch is open,
    // so every catalog keeps its bytes then), listed under the same ability.
    let mut by_file_name: BTreeMap<String, String> = BTreeMap::new();
    if switches.listed_by_file_name {
        let listed: BTreeSet<String> = found.keys().filter(|m| !hero_models.contains(*m) && users.heroes.get(*m).map_or(0, BTreeSet::len) < SHARED_SHAPE_HEROES).cloned().collect();
        let known = |m: &str| hero_models.contains(m) || users.heroes.contains_key(m) || users.drawings.contains_key(m) || users.placed.contains_key(m) || defs.records.values().any(|r| r.models.contains(m));
        by_file_name = file_name_companions(game, &listed, &known);
        for (companion, owner) in &by_file_name {
            let at = found[owner].0.clone();
            found.entry(companion.clone()).or_insert_with(|| (at, BTreeSet::new(), false));
        }
    }
    let mut counts = Counts::default();
    let mut rows = vec![];
    let own_name = heroes::display_name(hero_model);
    for (model, ((slot, root), drawn_by, named)) in found {
        if hero_models.contains(&model) {
            continue;
        }
        let heroes_using = users.heroes.get(&model).cloned().unwrap_or_default();
        if heroes_using.len() >= SHARED_SHAPE_HEROES {
            counts.shared_shapes += 1;
            continue;
        }
        // A model found by its file name: no data says what places it, so the game's own code
        // does; it is placed as the objects definitions name are (what it keeps stays the game's).
        let companion = by_file_name.get(&model);
        let placed = users.placed.get(&model).is_some_and(|r| !r.is_empty()) || companion.is_some();
        let stem = model.rsplit('/').next().unwrap_or_default().trim_end_matches(".vmdl").to_owned();
        // Such a row reads as the listed object it sits beside, as placed (`NAMED_AS_PLACED`).
        let owner_name = companion.map(|owner| readable_name(owner.rsplit('/').next().unwrap_or_default().trim_end_matches(".vmdl"), &hero_words));
        let found_by = owner_name.as_ref().map(|owner| FOUND_BY_FILE_NAME.replace("{}", owner));
        let name = owner_name.map_or_else(|| readable_name(&stem, &hero_words), |owner| NAMED_AS_PLACED.replace("{}", &owner));
        let live = users.drawings.get(&model).cloned().unwrap_or_default();
        let (structure, bounds, offer, kind) = match game.model(&model) {
            None => (None, None, Err(REASON_MISSING), None),
            Some(Err(_)) => (None, None, Err(REASON_UNREADABLE), None),
            Some(Ok(f)) => {
                let offer = offered_with(&f, placed, switches);
                // Only objects a switch opened: the first kind's rows keep their bytes.
                let opened = offer.is_ok() && first_profile(&f, placed).is_err();
                let offer = if opened && not_drawn(defs, &model, !drawn_by.is_empty() || !live.is_empty()) { Err(REASON_NOT_DRAWN) } else { offer };
                let offer = if offer.is_ok() && known_not_to_rebuild(&model) { Err(REASON_KNOWN_NOT_TO_REBUILD) } else { offer };
                (
                    Some(Structure {
                        bones: f.bones.len(),
                        meshes: f.embedded_meshes,
                        physics: f.physics_block || f.physics_refs > 0,
                        hitboxes: f.hitboxes,
                        animated: !f.animation_blocks.is_empty() || f.anim_graph.is_some() || f.sequence_groups > 0 || f.anim_groups > 0,
                        attachments: f.attachments.len(),
                        materials: f.materials.clone(),
                    }),
                    f.bounds,
                    offer,
                    // Only objects a switch opened carry a kind; the first kind's rows keep their bytes.
                    offer.ok().filter(|()| first_profile(&f, placed).is_err()).and_then(|()| object_kind(&f, placed)),
                )
            }
        };
        let ability_name = names.get(&root).map(str::to_owned).unwrap_or_else(|| effect_catalog::readable_internal(&root, &hero_words));
        // The looks the drawing renderers pick (only where an effect can show one of the game's).
        let always_overridden = !live.is_empty() && live.iter().all(|d| d.override_material.is_some());
        // Offered rows only: the line describes what a build of the object draws.
        let look = match game.model(&model) {
            Some(Ok(f)) if !always_overridden && offer.is_ok() => {
                let looks_of = |d: &Drawing| {
                    let found: Vec<Looks> = game.particle(&d.particle).map(|i| i.looks.iter().filter(|(m, _)| *m == model).map(|(_, l)| *l).collect()).unwrap_or_default();
                    found.into_iter().reduce(|a, b| Looks { picks: a.picks.zip(b.picks).map(|((l1, h1), (l2, h2))| (l1.min(l2), h1.max(h2))) })
                };
                let drawn: Vec<(bool, bool, Option<Looks>)> = live.iter().map(|d| (true, d)).chain(drawn_by.iter().filter(|d| !live.contains(*d)).map(|d| (false, d))).map(|(l, d)| (l, d.override_material.is_some(), looks_of(d))).collect();
                game_look(&f, &drawn)
            }
            _ => None,
        };
        rows.push(ObjectRow {
            game_look: look,
            name,
            file: stem,
            group: effect_catalog::slot_order(&slot),
            slot,
            ability: root,
            ability_name,
            drawn: match (named, !drawn_by.is_empty()) {
                _ if found_by.is_some() => DRAWN_FILE_NAME,
                (true, true) => "definition_and_particle",
                (true, false) => "definition",
                _ => "particle",
            }
            .into(),
            found_by,
            material_override: live.iter().chain(&drawn_by).find_map(|d| d.override_material.clone()),
            material_always_overridden: always_overridden,
            drawn_by: drawn_by.into_iter().collect(),
            structure,
            bounds,
            also_used_by: heroes_using.into_iter().filter(|h| Some(h) != own_name.as_ref()).collect(),
            offered: offer.is_ok(),
            reason: offer.err().map(str::to_owned),
            kind,
            target: model,
        });
    }
    rows.sort_by(|a, b| (a.group, &a.ability, &a.name, &a.target).cmp(&(b.group, &b.ability, &b.name, &b.target)));
    counts.listed = rows.len();
    counts.offered = rows.iter().filter(|r| r.offered).count();
    let scenery: BTreeSet<String> = rows.iter().filter(|r| is_scenery(&r.target)).map(|r| r.target.clone()).collect();
    Ok(ObjectCatalog { hero_path: hero_model.to_owned(), profile: PROFILE.into(), rows, counts, scenery: scenery.into_iter().collect() })
}
static CATALOG_READS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
/// How many catalogs this process has made (tests: a project without ability
/// objects never reads the catalog).
pub fn catalog_reads() -> usize {
    CATALOG_READS.load(std::sync::atomic::Ordering::SeqCst)
}
/// The catalog of `hero_model` from an archive (game data kept per archive).
pub fn catalog(archive: &Vpk, hero_model: &str, cancel: &CancellationToken) -> Result<ObjectCatalog> {
    catalog_with(archive, hero_model, &switches(), cancel)
}
/// `catalog` under `switches` (development counts only; the app uses `catalog`).
pub fn catalog_with(archive: &Vpk, hero_model: &str, switches: &Switches, cancel: &CancellationToken) -> Result<ObjectCatalog> {
    CATALOG_READS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let data = game_data(archive, cancel)?;
    let game = ArchiveGame { data: &data, archive };
    let roster = heroes::roster()?;
    let users = kept_users(&data, &game, &roster, cancel)?;
    hero_catalog_with(&game, &data.defs, &data.names, &users, hero_model, &roster, switches, cancel)
}
/// The targets a build may replace for `hero_model`: the catalog's offered
/// rows (casefolded `.vmdl` paths). The one rule for the page and the build.
pub fn allowed_in(catalog: &ObjectCatalog) -> BTreeSet<String> {
    catalog.rows.iter().filter(|r| r.offered).map(|r| casefold(&r.target)).collect()
}
pub fn allowed(archive: &Vpk, hero_model: &str, cancel: &CancellationToken) -> Result<BTreeSet<String>> {
    Ok(allowed_in(&catalog(archive, hero_model, cancel)?))
}

// ------------------------------------------------------------------ carrying kinds a switch opened

/// One collision shape of the game's object in model space (inches).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "shape", rename_all = "snake_case")]
pub enum CollisionShape {
    Sphere { center: [f64; 3], radius: f64 },
    Capsule { start: [f64; 3], end: [f64; 3], radius: f64 },
    /// A hull or mesh shape, as its bounds (kept from the game's own physics block).
    Box { min: [f64; 3], max: [f64; 3] },
}
fn point(value: Option<&Value>) -> Option<[f64; 3]> {
    let n = value.map(numbers).unwrap_or_default();
    (n.len() == 3).then(|| [n[0], n[1], n[2]])
}
/// The spheres and capsules of a compiled model's physics, placed by their
/// part's bind pose (`m_bindPose`, a 3×4 row-major bone-to-model matrix).
pub fn collision_shapes(raw: &[u8]) -> Vec<CollisionShape> {
    let mut out = vec![];
    for block in container::read_blocks(raw, "PHYS").unwrap_or_default() {
        let poses: Vec<Vec<f64>> = block.root.get("m_bindPose").and_then(|v| v.array().ok()).map(|a| a.iter().map(numbers).collect()).unwrap_or_default();
        for (i, part) in block.root.get("m_parts").and_then(|v| v.array().ok()).unwrap_or(&[]).iter().enumerate() {
            // Part i sits on bone i (`m_boneNames[i]`), placed by `m_bindPose[i]`.
            let pose = poses.get(i).filter(|p| p.len() == 12);
            let place = |p: [f64; 3]| match pose {
                Some(m) => [0, 1, 2].map(|r| m[r * 4] * p[0] + m[r * 4 + 1] * p[1] + m[r * 4 + 2] * p[2] + m[r * 4 + 3]),
                None => p,
            };
            let Some(shape) = part.get("m_rnShape") else { continue };
            for s in shape.get("m_spheres").and_then(|v| v.array().ok()).unwrap_or(&[]) {
                let sphere = s.get("m_Sphere");
                if let (Some(c), Some(r)) = (point(sphere.and_then(|v| v.get("m_vCenter"))), sphere.and_then(|v| v.get("m_flRadius")).and_then(|r| r.number().ok())) {
                    out.push(CollisionShape::Sphere { center: place(c), radius: r });
                }
            }
            for c in shape.get("m_capsules").and_then(|v| v.array().ok()).unwrap_or(&[]) {
                let capsule = c.get("m_Capsule");
                let ends = capsule.and_then(|v| v.get("m_vCenter")).and_then(|v| v.array().ok()).filter(|a| a.len() == 2);
                if let (Some(ends), Some(r)) = (ends, capsule.and_then(|v| v.get("m_flRadius")).and_then(|r| r.number().ok())) {
                    if let (Some(a), Some(b)) = (point(Some(&ends[0])), point(Some(&ends[1]))) {
                        out.push(CollisionShape::Capsule { start: place(a), end: place(b), radius: r });
                    }
                }
            }
            // Hulls (`m_Hull.m_Bounds`) and meshes (`m_Mesh.m_vMin` / `m_vMax`) as the bounds of
            // their placed corners.
            let hulls = shape.get("m_hulls").and_then(|v| v.array().ok()).unwrap_or(&[]).iter().map(|h| {
                let b = h.get("m_Hull").and_then(|x| x.get("m_Bounds"));
                (point(b.and_then(|x| x.get("m_vMinBounds"))), point(b.and_then(|x| x.get("m_vMaxBounds"))))
            });
            let meshes = shape.get("m_meshes").and_then(|v| v.array().ok()).unwrap_or(&[]).iter().map(|m| {
                let mesh = m.get("m_Mesh");
                (point(mesh.and_then(|x| x.get("m_vMin"))), point(mesh.and_then(|x| x.get("m_vMax"))))
            });
            for (lo, hi) in hulls.chain(meshes) {
                let (Some(lo), Some(hi)) = (lo, hi) else { continue };
                let mut min = [f64::MAX; 3];
                let mut max = [f64::MIN; 3];
                for corner in 0..8 {
                    let p = place([0, 1, 2].map(|k| if corner >> k & 1 == 0 { lo[k] } else { hi[k] }));
                    for k in 0..3 {
                        min[k] = min[k].min(p[k]);
                        max[k] = max[k].max(p[k]);
                    }
                }
                out.push(CollisionShape::Box { min, max });
            }
        }
    }
    out
}
impl CollisionShape {
    /// The distance from `p` to the shape's surface: negative inside.
    pub fn distance(&self, p: [f64; 3]) -> f64 {
        let length = |v: [f64; 3]| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        match self {
            CollisionShape::Sphere { center, radius } => length([0, 1, 2].map(|k| p[k] - center[k])) - radius,
            CollisionShape::Capsule { start, end, radius } => {
                let d = [0, 1, 2].map(|k| end[k] - start[k]);
                let w = [0, 1, 2].map(|k| p[k] - start[k]);
                let dd = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
                let t = if dd > 0. { ((w[0] * d[0] + w[1] * d[1] + w[2] * d[2]) / dd).clamp(0., 1.) } else { 0. };
                length([0, 1, 2].map(|k| p[k] - (start[k] + t * d[k]))) - radius
            }
            CollisionShape::Box { min, max } => {
                // Outside: the distance to the box; inside: minus the distance to the nearest face.
                let out = [0, 1, 2].map(|k| (min[k] - p[k]).max(p[k] - max[k]));
                let outside = length(out.map(|d| d.max(0.)));
                outside + out[0].max(out[1]).max(out[2]).min(0.)
            }
        }
    }
}
/// How far the furthest of `points` lies outside every shape (0 when all are inside one).
pub fn sticks_out(points: impl IntoIterator<Item = [f64; 3]>, shapes: &[CollisionShape]) -> Option<f64> {
    if shapes.is_empty() {
        return None;
    }
    let mut most: Option<f64> = None;
    for p in points {
        let outside = shapes.iter().map(|s| s.distance(p)).fold(f64::INFINITY, f64::min).max(0.);
        most = Some(most.map_or(outside, |m| m.max(outside)));
    }
    most
}
/// How much further (inches) than the game's own object the user's may stick
/// out of the game's collision and still count as fitting it.
pub const FITS_COLLISION: f64 = 2.;
/// The fit view's one line about the collision (the page shows it as it is).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CollisionFit {
    /// How far the user's object sticks out of the game's collision at most (inches).
    pub inches: f64,
    /// How far the game's own object does (its first level's body; many game
    /// objects are drawn larger than their collision).
    pub game_inches: f64,
    /// Not more than `FITS_COLLISION` beyond the game's own.
    pub fits: bool,
    pub line: String,
}
/// The line for an object that sticks out by `inches` at most where the game's own does by `game`.
pub fn collision_fit(inches: f64, game: f64) -> CollisionFit {
    let shown = |v: f64| (v * 10.).round() / 10.;
    let (mine, theirs) = (shown(inches), shown(game));
    let fits = inches <= game + FITS_COLLISION;
    let line = if fits {
        format!("Your object fits the game's collision: it sticks out by up to {mine} in, the game's own object by {theirs} in.")
    } else {
        format!("Your object sticks out of the game's collision by up to {mine} in (the game's own object by {theirs} in): players would be blocked by, or walk through, something that does not match what they see.")
    };
    CollisionFit { inches, game_inches: game, fits, line }
}

/// One KV3 block flattened to `path = value` rows.
pub fn flatten(value: &Value) -> BTreeMap<String, String> {
    fn walk(v: &Value, path: &str, out: &mut BTreeMap<String, String>) {
        use deadlock_resource::Kind;
        match &v.kind {
            Kind::Object(rows) => {
                if rows.is_empty() {
                    out.insert(path.to_owned(), "{}".into());
                }
                for (k, x) in rows {
                    walk(x, &format!("{path}.{k}"), out);
                }
            }
            Kind::Array(items) => {
                if items.is_empty() {
                    out.insert(path.to_owned(), "[]".into());
                }
                for (i, x) in items.iter().enumerate() {
                    walk(x, &format!("{path}[{i}]"), out);
                }
            }
            Kind::String(s) => {
                out.insert(path.to_owned(), format!("{s:?}"));
            }
            Kind::Bool(b) => {
                out.insert(path.to_owned(), b.to_string());
            }
            Kind::I64(n) => {
                out.insert(path.to_owned(), n.to_string());
            }
            Kind::U64(n) => {
                out.insert(path.to_owned(), n.to_string());
            }
            Kind::F32(n) => {
                out.insert(path.to_owned(), format!("{n}"));
            }
            Kind::F64(n) => {
                out.insert(path.to_owned(), format!("{n}"));
            }
            Kind::Null => {
                out.insert(path.to_owned(), "null".into());
            }
            Kind::Blob(b) => {
                out.insert(path.to_owned(), format!("<blob {} bytes {}>", b.len(), &sha(b)[..16]));
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(value, "", &mut out);
    out
}
/// Two flattened values the same: equal text, or numbers within 1e-4 (or 1e-6 relative).
pub fn same_value(a: &str, b: &str) -> bool {
    a == b
        || match (a.parse::<f64>(), b.parse::<f64>()) {
            (Ok(x), Ok(y)) => (x - y).abs() <= 1e-4_f64.max(1e-6 * x.abs().max(y.abs())),
            _ => false,
        }
}
/// Rows that describe the render mesh, its materials or the compiler's own
/// bookkeeping, not the object's structure (the mesh is the user's).
pub fn mesh_row(block: &str, path: &str) -> bool {
    if block == "CTRL" {
        return true;
    }
    let p = path.to_ascii_lowercase();
    // The physics and the hit areas are never the render mesh (their bounds and bone hashes
    // would otherwise match the words below).
    if block == "PHYS" || p.contains("m_hitboxsets") {
        return false;
    }
    [
        "m_sceneobjects",
        "m_drawcalls",
        "m_vminbounds",
        "m_vmaxbounds",
        "m_meshes[",
        "m_morphset",
        "m_vertexbuffers",
        "m_indexbuffers",
        "m_meshbounds",
        "m_vbonepositionbounds",
        "m_toolsbuffers",
        "m_materials",
        "m_refmeshes",
        "m_refmeshgroupmasks",
        "m_reflodgroupmasks",
        "m_lodgroupswitchdistances",
        "m_meshgroups",
        "m_nlodgroupmask",
        "m_affinescale",
        "m_scenebounds",
        "m_bones",
        "m_vminbound",
        "m_vmaxbound",
        "m_vbonepositions",
        "m_bonesboundingsphere",
        "m_materialgroups",
        "m_ndefaultmeshgroupmask",
        "m_remappingtable",
    ]
    .iter()
    .any(|k| p.contains(k))
}
/// The model skeletons of two DATA blocks compared bone by bone, by name: the
/// parent by name and the rest (position, rotation, scale); flags and the
/// bounding sphere (made from the mesh) are noted; the order is noted.
fn skeleton_by_name(theirs: Option<&BTreeMap<String, String>>, ours: Option<&BTreeMap<String, String>>, check: &mut CarriedCheck) {
    let bones = |doc: Option<&BTreeMap<String, String>>| -> Vec<BTreeMap<String, String>> {
        let Some(doc) = doc else { return vec![] };
        let names: Vec<String> = (0..).map_while(|i| doc.get(&format!(".m_modelSkeleton.m_boneName[{i}]")).cloned()).collect();
        names
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let mut out = BTreeMap::new();
                for (key, value) in doc.iter().filter(|(k, _)| k.starts_with(".m_modelSkeleton.")) {
                    let rest = &key[".m_modelSkeleton.".len()..];
                    let Some((field, tail)) = rest.split_once('[') else { continue };
                    let Some((index, inner)) = tail.split_once(']') else { continue };
                    if index.parse::<usize>().ok() != Some(i) {
                        continue;
                    }
                    let value = if field == "m_nParent" { value.parse::<usize>().ok().and_then(|p| names.get(p)).cloned().unwrap_or_else(|| "(none)".into()) } else { value.clone() };
                    out.insert(format!("{field}{inner}"), value);
                }
                out.insert("name".into(), name.clone());
                out
            })
            .collect()
    };
    let (a, b) = (bones(theirs), bones(ours));
    let order = |v: &[BTreeMap<String, String>]| v.iter().map(|b| b["name"].clone()).collect::<Vec<_>>();
    if order(&a) != order(&b) {
        check.noted.push(format!("bone order: the game's {:?} / ours {:?} (compared by name)", order(&a), order(&b)));
    }
    let (mut rows, mut same) = (0, 0);
    for bone in &a {
        let name = &bone["name"];
        let Some(other) = b.iter().find(|x| &x["name"] == name) else {
            check.differs.push(format!("bone {name}: missing in ours"));
            continue;
        };
        for key in bone.keys().chain(other.keys()).collect::<BTreeSet<_>>() {
            let (x, y) = (bone.get(key), other.get(key));
            if key.starts_with("m_nFlag") || key.starts_with("m_boneSphere") {
                if x != y {
                    check.noted.push(format!("bone {name} {key}: {} / {}", x.map_or("(absent)", String::as_str), y.map_or("(absent)", String::as_str)));
                }
                continue;
            }
            rows += 1;
            match (x, y) {
                (Some(x), Some(y)) if same_value(x, y) => same += 1,
                _ => check.differs.push(format!("bone {name} {key}: {} / {}", x.map_or("(absent)", String::as_str), y.map_or("(absent)", String::as_str))),
            }
        }
    }
    if b.len() != a.len() {
        check.differs.push(format!("bones: {} / {}", a.len(), b.len()));
    }
    check.compared.insert("DATA skeleton by name".into(), (rows, same));
}
/// What a carried object's read-back compared.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CarriedCheck {
    /// Rows compared per block (`DATA`, `PHYS`, `MDAT[i]`…) and how many were equal.
    pub compared: BTreeMap<String, (usize, usize)>,
    /// Every row that differs (refused), as `block path: game / compiled`.
    pub differs: Vec<String>,
    /// Rows left out of the comparison on purpose (hitbox CRCs), noted only.
    pub noted: Vec<String>,
}
/// Flattened values that mean "nothing": a key absent on one side and one of
/// these on the other counts as the same (older compiled files leave them out).
const EMPTY_DEFAULTS: [&str; 6] = ["[]", "{}", "false", "0", "\"\"", "null"];
fn block_docs(raw: &[u8], name: &str) -> Vec<BTreeMap<String, String>> {
    container::read_blocks(raw, name).unwrap_or_default().iter().map(|b| flatten(&b.root)).collect()
}
/// How far (inches) a bone position of an animated object's animation decode
/// skeleton may lie from the game's: the compiler rebuilds those positions from
/// the decompiled animation files and rounds them by up to 6.5e-4 in on the
/// objects seen; 1e-3 in (0.025 mm) keeps a margin and stays far below anything
/// visible. Positions only; rotations and every other row stay exact.
pub const DECODE_POSITION_TOLERANCE: f64 = 1e-3;
/// A position row of the animation decode skeleton (`.m_decodeKey.m_boneArray[i].m_pos[k]`).
fn decode_position(key: &str) -> bool {
    key.strip_prefix(".m_decodeKey.m_boneArray[")
        .and_then(|rest| rest.split_once("].m_pos["))
        .is_some_and(|(bone, axis)| bone.parse::<usize>().is_ok() && axis.strip_suffix(']').is_some_and(|k| k.parse::<usize>().is_ok_and(|k| k < 3)))
}
fn within(a: &str, b: &str, tolerance: f64) -> bool {
    matches!((a.parse::<f64>(), b.parse::<f64>()), (Ok(x), Ok(y)) if x.is_finite() && y.is_finite() && (x - y).abs() <= tolerance)
}
fn compare_rows(label: &str, block: &str, theirs: &BTreeMap<String, String>, ours: &BTreeMap<String, String>, check: &mut CarriedCheck) {
    let (mut rows, mut same) = (0, 0);
    let keys: BTreeSet<&String> = theirs.keys().chain(ours.keys()).collect();
    for key in keys {
        if mesh_row(block, key) {
            continue;
        }
        let (a, b) = (theirs.get(key), ours.get(key));
        if key.to_ascii_lowercase().ends_with(".m_ncrc") || key.to_ascii_lowercase().ends_with(".m_crc") {
            if a != b {
                check.noted.push(format!("{label}{key}: {} / {}", a.map_or("(absent)", String::as_str), b.map_or("(absent)", String::as_str)));
            }
            continue;
        }
        rows += 1;
        match (a, b) {
            (Some(a), Some(b)) if same_value(a, b) => same += 1,
            // The compiler's rounding of the bone positions of the animation decode skeleton
            // (`AGRP m_decodeKey.m_boneArray[i].m_pos`): accepted within `DECODE_POSITION_TOLERANCE`
            // and noted, nowhere else.
            (Some(x), Some(y)) if block == "AGRP" && decode_position(key) && within(x, y, DECODE_POSITION_TOLERANCE) => {
                same += 1;
                check.noted.push(format!("{label}{key}: {a} / {b} (compiler rounding, within {DECODE_POSITION_TOLERANCE} in)", a = x, b = y));
            }
            // A key the game's (older) file leaves out and today's compiler writes as its empty
            // default; never the other way round, and never in the physics.
            (None, Some(v)) if block != "PHYS" && EMPTY_DEFAULTS.contains(&v.as_str()) => {
                same += 1;
                check.noted.push(format!("{label}{key}: {} / {} (an empty default)", a.map_or("(absent)", String::as_str), b.map_or("(absent)", String::as_str)));
            }
            _ => check.differs.push(format!("{label}{key}: {} / {}", a.map_or("(absent)", String::as_str), b.map_or("(absent)", String::as_str))),
        }
    }
    check.compared.insert(label.to_owned(), (rows, same));
}
/// The read-back of an object a switch opened: every row of DATA, PHYS and
/// each mesh block that is not the render mesh must equal the game's (the
/// skeleton by name, parent and rest; physics value by value; attachments,
/// hit areas, game data, level-of-detail switch distances), and every
/// compiled mesh must be bound to the same bones as the game's. `shells`:
/// the build's variant; with `Dropped`, compiled mesh `k` is compared with the
/// game's body mesh of level `k`.
pub fn carried_check(original: &[u8], compiled: &[u8], shells: Option<Shells>) -> Result<CarriedCheck> {
    let mut check = CarriedCheck::default();
    let (theirs, ours) = (facts(original)?, facts(compiled)?);
    let several = theirs.several_bones();
    // Several bones (animation, graph): the animation group and sequence blocks too, value by value.
    let blocks: &[&str] = if several { &["DATA", "PHYS", "AGRP", "ASEQ"] } else { &["DATA", "PHYS"] };
    for block in blocks {
        let (mut a, mut b) = (block_docs(original, block), block_docs(compiled, block));
        if a.len() != b.len() {
            check.differs.push(format!("{block}: {} blocks / {}", a.len(), b.len()));
        }
        if several && *block == "DATA" {
            // The skeleton by bone name (the compiler may store the bones in another order).
            skeleton_by_name(a.first(), b.first(), &mut check);
            for doc in a.iter_mut().chain(b.iter_mut()) {
                doc.retain(|k, _| !k.starts_with(".m_modelSkeleton."));
            }
        }
        for (i, (x, y)) in a.iter().zip(&b).enumerate() {
            compare_rows(&format!("{block}[{i}]"), block, x, y, &mut check);
        }
    }
    if several {
        // The animation block the compiler builds again from the decompiled animation files:
        // counted and noted, never packaged from the game (the game plays its own clips).
        let (a, b) = (block_docs(original, "ANIM"), block_docs(compiled, "ANIM"));
        let (x, y) = (a.first().cloned().unwrap_or_default(), b.first().cloned().unwrap_or_default());
        let differ = x.keys().chain(y.keys()).collect::<BTreeSet<_>>().into_iter().filter(|k| !matches!((x.get(*k), y.get(*k)), (Some(p), Some(q)) if same_value(p, q))).count();
        if !a.is_empty() || !b.is_empty() {
            check.noted.push(format!("ANIM: {} blocks / {}; {differ} of {} rows differ (rebuilt from the decompiled animation files)", a.len(), b.len(), x.len().max(y.len())));
        }
    }
    let (a, b) = (block_docs(original, "MDAT"), block_docs(compiled, "MDAT"));
    // (game mesh, compiled mesh): level by level; without levels of detail the
    // one replacing mesh against every mesh of the game's (each carries the
    // attachments and hit areas).
    let pairs: Vec<(usize, usize)> = if theirs.lod_meshes.is_empty() && !theirs.mesh_groups.is_empty() {
        // Body groups: every game mesh keeps its place (each draws the user's mesh).
        (0..a.len()).map(|i| (i, i)).collect()
    } else if theirs.lod_meshes.is_empty() {
        (0..a.len()).map(|i| (i, 0)).collect()
    } else {
        match shells {
            Some(Shells::Dropped) if theirs.shell_material().is_some() => theirs.levels().iter().enumerate().map(|(k, l)| (l[0], k)).collect(),
            _ => (0..a.len()).map(|i| (i, i)).collect(),
        }
    };
    let expected = pairs.iter().map(|p| p.1 + 1).max().unwrap_or(0);
    if expected != b.len() {
        check.differs.push(format!("MDAT: {expected} meshes expected / {}", b.len()));
    }
    for (i, k) in &pairs {
        if let (Some(x), Some(y)) = (a.get(*i), b.get(*k)) {
            if several {
                // How many bones weigh each point is the user's mesh (its own weights or parts): noted.
                let (mut x, mut y) = (x.clone(), y.clone());
                let key = ".m_skeleton.m_nBoneWeightCount";
                let (p, q) = (x.remove(key), y.remove(key));
                if p != q {
                    check.noted.push(format!("MDAT[{i}→{k}]{key}: {} / {} (your mesh)", p.as_deref().unwrap_or("(absent)"), q.as_deref().unwrap_or("(absent)")));
                }
                compare_rows(&format!("MDAT[{i}→{k}]"), "MDAT", &x, &y, &mut check);
            } else {
                compare_rows(&format!("MDAT[{i}→{k}]"), "MDAT", x, y, &mut check);
            }
        }
    }
    // Bound to the same bones (mesh skeletons, which `mesh_row` leaves out above).
    let bones = |raw: &[u8]| -> Vec<Vec<String>> {
        container::read_blocks(raw, "MDAT")
            .unwrap_or_default()
            .iter()
            .map(|m| m.root.get("m_skeleton").and_then(|s| s.get("m_bones")).and_then(|b| b.array().ok()).map(|a| a.iter().filter_map(|b| b.get("m_boneName").and_then(|n| n.string().ok()).map(str::to_owned)).collect()).unwrap_or_default())
            .collect()
    };
    let (want, got) = (bones(original), bones(compiled));
    // Parts kept as the game's own (`kept_parts`): bound exactly as the game binds them,
    // in the same place, with the same name.
    if let Some(parts) = parts_kept_as_games(&theirs) {
        for part in &parts.kept {
            if want.get(part.mesh) != got.get(part.mesh) {
                check.differs.push(format!("MDAT[{}] (kept part {}) bound to {:?}, the game's to {:?}", part.mesh, part.name, got.get(part.mesh), want.get(part.mesh)));
            }
            if ours.mesh_names.get(part.mesh) != Some(&part.name) {
                check.differs.push(format!("mesh {} is {:?}, the game's kept part {}", part.mesh, ours.mesh_names.get(part.mesh), part.name));
            }
        }
    }
    // Body-group states (`object_states`): each state unskinned where the game's is, else bound only to its own bones.
    crate::object_states::check(&theirs, &ours, &want, &got, &mut check);
    for (i, k) in &pairs {
        if several {
            // Bound only to bones the object has.
            if let Some(lacking) = got.get(*k).into_iter().flatten().find(|b| !theirs.bones.contains(b)) {
                check.differs.push(format!("MDAT[{k}] is bound to {lacking}, a bone the game's object does not have"));
            }
        } else if want.get(*i) != got.get(*k) {
            check.differs.push(format!("MDAT[{k}] bound to {:?}, the game's to {:?}", got.get(*k), want.get(*i)));
        }
    }
    // Levels of detail: the same switch distances, one body per level (and its shell when kept).
    if theirs.lod_switch_distances.len() != ours.lod_switch_distances.len() || theirs.lod_switch_distances.iter().zip(&ours.lod_switch_distances).any(|(x, y)| (x - y).abs() > 1e-4) {
        check.differs.push(format!("level-of-detail switch distances: {:?} / {:?}", theirs.lod_switch_distances, ours.lod_switch_distances));
    }
    let levels = match shells {
        Some(Shells::Dropped) if theirs.shell_material().is_some() => theirs.lod_meshes.iter().map(|_| 1).collect(),
        _ => theirs.lod_meshes.clone(),
    };
    if ours.lod_meshes != levels {
        check.differs.push(format!("meshes per level of detail: {levels:?} / {:?}", ours.lod_meshes));
    }
    // Body groups: the groups and their choices (`m_meshGroups`), which meshes each
    // choice draws (`m_refMeshGroupMasks`) and the default choice, as the game's
    // (rows `mesh_row` leaves out above).
    if !theirs.mesh_groups.is_empty() {
        let groups = |raw: &[u8]| -> BTreeMap<String, String> {
            block_docs(raw, "DATA").into_iter().next().unwrap_or_default().into_iter().filter(|(k, _)| ["m_meshgroups", "m_refmeshgroupmasks", "m_ndefaultmeshgroupmask"].iter().any(|g| k.to_ascii_lowercase().starts_with(&format!(".{g}")))).collect()
        };
        let (want, got) = (groups(original), groups(compiled));
        let rows = want.len().max(got.len());
        let same = want.iter().filter(|(k, v)| got.get(*k).is_some_and(|g| same_value(v, g))).count();
        if want.is_empty() || want.len() != got.len() || same != want.len() {
            check.differs.push(format!("body groups: {want:?} / {got:?}"));
        }
        check.compared.insert("DATA body groups".into(), (rows, same));
    }
    // A morph block left out (`object_morphs`): the game's morph textures, and only they, may be
    // missing from the rebuilt references; empty (nothing allowed) for every other object.
    let theirs = match crate::object_morphs::compared(original, compiled, theirs.clone(), &ours) {
        Ok((compared, left)) => {
            if !left.is_empty() {
                check.noted.push(format!("the morph block is left out: the game's {} morph block rows and its morph textures {left:?} are not in the rebuilt model", crate::object_morphs::morph_rows(original)));
            }
            compared
        }
        Err(e) => {
            check.differs.push(format!("morph block: {e}"));
            theirs
        }
    };
    for name in kept_differences(&theirs, &ours) {
        check.differs.push(format!("{name} differ"));
    }
    Ok(check)
}

// ------------------------------------------------------------------ receipts

/// One built object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObjectEntry {
    /// The replaced game file (`….vmdl_c`).
    pub target: String,
    pub original_sha256: String,
    /// The user's file and its digest when it was prepared.
    pub source: String,
    pub source_sha256: String,
    /// The prepared packet the mesh came from.
    pub packet_sha256: String,
    pub fit: ObjectFit,
    /// Every packaged output (the target and the object's own files) with its digest.
    pub files: Hashes,
    /// The materials the compiled model names (the object's own).
    pub materials: Vec<String>,
    /// Compiled model bounds (model space).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bounds: Option<Bounds>,
    /// For an object a switch opened: what was carried and checked (absent
    /// for the first proven kind, so its receipts keep their bytes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub carried: Option<CarriedEntry>,
    /// The row's follow setting (several bones), absent unless the row has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub follow: Option<deadlock_project::ability_objects::Follow>,
}
/// What the build carried from the game's object, and its read-back.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CarriedEntry {
    pub features: Vec<Feature>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shells: Option<Shells>,
    /// The game's own outline material the shells reference (variant `Generated`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell_material: Option<String>,
    /// Game files staged for the compiler (never packaged).
    pub staged: usize,
    /// Attachments taken from the game's model after compiling.
    pub attachments_taken: usize,
    pub check: CarriedCheck,
    /// Several bones: what each part of the user's file was bound to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binding: Option<crate::object_follow::Binding>,
    /// Body-group states (`object_states`): how each state was bound; absent for every
    /// other kind, so such receipts keep their bytes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub state_bindings: Vec<crate::object_states::StateBinding>,
    /// The game's own blocks put into the rebuilt model unchanged
    /// (`keep_blocks`), with their digests; absent for every other kind.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub kept_blocks: Vec<crate::keep_blocks::KeptBlock>,
    /// The meshes kept as the game's own (`kept_parts`, their names); absent for every other kind.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub kept_parts: Vec<String>,
    /// The fields of `DATA` written back from the game's own file (`write_back`),
    /// with digests of their values; absent for every other kind.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub written: Vec<crate::write_back::Written>,
}
/// Build and install messages of the kinds that keep the game's own blocks
/// (matched by `troubleshooting.rs`).
pub const KEPT_BLOCKS_REFUSED: &str = "The game's own animation or collision cannot be kept in the rebuilt ability object";
pub const KEPT_BLOCKS_CHANGED: &str = "The ability object's kept animation or collision is no longer the game's own";
/// Build and install messages of the fields written back from the game's own file (`write_back`).
pub const WRITTEN_REFUSED: &str = "The game's own model data cannot be written into the rebuilt ability object";
pub const WRITTEN_CHANGED: &str = "The ability object's model data written from the game's own file is no longer the game's";
/// Build and install message for the parts kept as the game's own (`kept_parts`).
pub const KEPT_PARTS_CHANGED: &str = "The ability object's parts kept as the game's own no longer match the game's object";
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ObjectReceipt {
    pub schema: u32,
    pub profile: String,
    pub hero_path: String,
    pub entries: Vec<ObjectEntry>,
}
impl ObjectReceipt {
    pub fn empty(hero_path: &str) -> Self {
        Self { schema: 1, profile: PROFILE.into(), hero_path: hero_path.into(), entries: vec![] }
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    /// Files packaged besides the targets (the objects' own files).
    pub fn own_files(&self) -> Vec<String> {
        self.entries.iter().flat_map(|e| e.files.keys().filter(|p| casefold(p) != casefold(&e.target)).cloned()).collect()
    }
    /// Whether the receipt was built from these rows (targets, sources and fits).
    pub fn same_rows(&self, rows: &[deadlock_project::ability_objects::AbilityObjectRow]) -> bool {
        let near = |a: f64, b: f64| (a - b).abs() <= 1e-9;
        rows.len() == self.entries.len()
            && rows.iter().all(|row| {
                let Ok(target) = row.compiled_target() else { return false };
                self.entries.iter().any(|e| {
                    casefold(&e.target) == casefold(&target)
                        && same_source(&e.source, &row.source)
                        && e.fit.size == row.fit.size
                        && e.fit.turns == row.fit.turns
                        && e.fit.on_base == row.fit.on_base
                        && e.follow == row.follow
                        && near(e.fit.scale, row.fit.scale)
                })
            })
    }
}
/// Two spellings of one source file (separators and case, as a saved project
/// may write them differently from the one that was built).
pub fn same_source(a: &str, b: &str) -> bool {
    let plain = |p: &str| casefold(&p.replace('\\', "/")).trim_start_matches("//?/").to_owned();
    plain(a) == plain(b)
}
/// Whether `path` (casefolded) is one of the objects' own files.
pub fn own_file(path: &str) -> bool {
    casefold(path).starts_with(&format!("{OBJECT_FOLDER}/"))
}
/// The compiled targets in `game` still decode to the original's structure, and
/// the original is the current game file.
pub fn verify_compiled(archive: &Vpk, receipt: &ObjectReceipt, game: &Path, cancel: &CancellationToken) -> Result<()> {
    for entry in &receipt.entries {
        cancel.check()?;
        let original = crate::read(archive, &entry.target)?;
        if sha(&original) != entry.original_sha256 {
            return Err(fail(format!("{OBJECT_CHANGED}: {}", entry.target)));
        }
        let compiled = crate::read_file(&deadlock_core::safety::inside(game, &entry.target)?)?;
        // A morph block left out (`object_morphs`): exactly its morph textures may be missing.
        let ours = facts(&compiled)?;
        let (theirs, _) = crate::object_morphs::compared(&original, &compiled, facts(&original)?, &ours).map_err(|e| fail(format!("{COMPILED_DIFFERS} (morph block: {e}): {}", entry.target)))?;
        let differs = kept_differences(&theirs, &ours);
        if !differs.is_empty() {
            return Err(fail(format!("{COMPILED_DIFFERS} ({}): {}", differs.join(", "), entry.target)));
        }
        if let Some(carried) = &entry.carried {
            // The kept blocks first, by their bytes: the game's own and the ones the build kept.
            let mut kept = crate::keep_blocks::same_as_kept(&original, &compiled, &carried.kept_blocks);
            let kinds = kept_kinds(&facts(&original)?);
            let wanted = kept_block_names(&kinds);
            if carried.kept_blocks.iter().map(|k| k.name.as_str()).collect::<Vec<_>>() != wanted {
                kept.push(format!("the build kept {:?}, the game's object needs {wanted:?}", carried.kept_blocks.iter().map(|k| k.name.as_str()).collect::<Vec<_>>()));
            }
            // The game's cloth kept: still valid on the installed file (the skeleton bone by bone, the cloth flags).
            if kinds.contains(&KeptKind::Cloth) {
                kept.extend(crate::keep_blocks::cloth_problems(&original, &compiled));
            }
            if !kept.is_empty() {
                return Err(fail(format!("{KEPT_BLOCKS_CHANGED} ({}): {}", kept.join("; "), entry.target)));
            }
            // The parts kept as the game's own: the ones the game's object has now.
            let parts = parts_kept_as_games(&facts(&original)?).map(|p| p.names()).unwrap_or_default();
            if carried.kept_parts != parts {
                return Err(fail(format!("{KEPT_PARTS_CHANGED} (the build kept {:?}, the game's object has {parts:?}): {}", carried.kept_parts, entry.target)));
            }
            // The fields written back: the values the build wrote, still the game's own, and the
            // ones the game's object needs now.
            let mut written = crate::write_back::same_as_written(&original, &compiled, &carried.written);
            let wanted: Vec<_> = written_kinds(&facts(&original)?).into_iter().collect();
            if carried.written.iter().map(|w| w.kind).collect::<Vec<_>>() != wanted {
                written.push(format!("the build wrote {:?}, the game's object needs {wanted:?}", carried.written.iter().map(|w| w.kind).collect::<Vec<_>>()));
            }
            if !written.is_empty() {
                return Err(fail(format!("{WRITTEN_CHANGED} ({}): {}", written.join("; "), entry.target)));
            }
            let check = carried_check(&original, &compiled, carried.shells)?;
            if !check.differs.is_empty() {
                return Err(fail(format!("{COMPILED_DIFFERS} ({}): {}", check.differs.iter().take(6).cloned().collect::<Vec<_>>().join("; "), entry.target)));
            }
            let shells = crate::object_glow_shells::check_compiled(&original, &compiled)?;
            if !shells.is_empty() {
                return Err(fail(format!("{COMPILED_DIFFERS} ({}): {}", shells.join("; "), entry.target)));
            }
        }
        for file in entry.files.keys() {
            if casefold(file) != casefold(&entry.target) && !own_file(file) {
                return Err(fail(format!("An ability object's file is outside its folder: {file}")));
            }
        }
    }
    Ok(())
}
/// The ways the receipt's rows follow the game's bones are still open: the
/// same-skeleton way has its own switch, checked per row at build and again
/// here at install (the other kinds are rechecked through the allowed list).
pub fn verify_ways(receipt: &ObjectReceipt, switches: &Switches) -> Result<()> {
    use deadlock_project::ability_objects::FollowWay;
    for entry in &receipt.entries {
        let way = entry.follow.as_ref().map(|f| f.way).or_else(|| entry.carried.as_ref().and_then(|c| c.binding.as_ref()).map(|b| b.way));
        if way == Some(FollowWay::SameSkeleton) && !switches.same_skeleton {
            return Err(fail(format!("{}: {}", crate::object_follow::SAME_SKELETON_CLOSED, entry.target)));
        }
    }
    Ok(())
}
/// The receipt's objects are still offered for the hero (read only when there are some).
pub fn verify_entries(archive: &Vpk, receipt: &ObjectReceipt, cancel: &CancellationToken) -> Result<()> {
    if receipt.entries.is_empty() {
        return Ok(());
    }
    verify_ways(receipt, &switches())?;
    let allowed = allowed(archive, &receipt.hero_path, cancel)?;
    for entry in &receipt.entries {
        if !allowed.contains(&model_key(&entry.target)) {
            return Err(fail(format!("{NOT_OFFERED}: {}", entry.target)));
        }
    }
    Ok(())
}
/// Compiles a project's ability objects into the components' game folder and
/// returns their receipt. The pipeline implements it (it owns the mesh, model
/// and material writers and the compiler); `ComponentService::compile` without
/// one refuses rows.
pub trait Builder {
    fn build(&self, project: &deadlock_project::Project, rows: &[deadlock_project::ability_objects::AbilityObjectRow], archive: &Vpk, game: &Path) -> Result<ObjectReceipt>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadlock_resource::Kind;

    fn s(t: &str) -> Value {
        Value::new(Kind::String(t.into()))
    }
    fn obj(rows: Vec<(&str, Value)>) -> Value {
        Value::new(Kind::Object(rows.into_iter().map(|(k, v)| (k.to_owned(), v)).collect()))
    }
    fn arr(items: Vec<Value>) -> Value {
        Value::new(Kind::Array(items))
    }
    fn plain() -> ModelFacts {
        ModelFacts { embedded_meshes: 1, materials: vec!["models/a/a.vmat".into()], ..Default::default() }
    }

    #[test]
    fn a_readable_name_leaves_out_the_file_name_habits() {
        let hero = vec![vec!["kelvin".to_string()]];
        assert_eq!(readable_name("kelvin_fx_bomb_model", &hero), "Bomb");
        assert_eq!(readable_name("kelvin_ice_projectile_model", &hero), "Ice projectile");
        assert_eq!(readable_name("shield_fx_model", &hero), "Shield");
        // A name that is nothing but such a word keeps it.
        assert_eq!(readable_name("kelvin_fx", &hero), "Fx");
    }

    #[test]
    fn the_offered_profile_names_one_reason_per_kind() {
        assert_eq!(offered(&plain(), false), Ok(()));
        let cases: Vec<(ModelFacts, &str)> = vec![
            (ModelFacts { bones: vec!["root".into()], ..plain() }, REASON_SKELETON),
            (ModelFacts { mesh_skeleton_bones: Some(2), ..plain() }, REASON_SKELETON),
            (ModelFacts { physics_block: true, ..plain() }, REASON_COLLISION),
            (ModelFacts { physics_refs: 1, ..plain() }, REASON_COLLISION),
            (ModelFacts { hitboxes: 3, ..plain() }, REASON_HITBOXES),
            (ModelFacts { animation_blocks: vec!["ASEQ".into()], ..plain() }, REASON_ANIMATED),
            (ModelFacts { anim_graph: Some("a.vnmgraph".into()), ..plain() }, REASON_ANIMATED),
            (ModelFacts { sequence_groups: 1, ..plain() }, REASON_ANIMATED),
            (ModelFacts { attachments: vec!["tip@root".into()], ..plain() }, REASON_ATTACHMENTS),
            (ModelFacts { materials: vec![], ..plain() }, REASON_NO_MATERIAL),
            (ModelFacts { materials: vec!["a.vmat".into(), "b.vmat".into()], ..plain() }, REASON_MATERIALS),
            // The correction: body groups were never built in the first kind, so they are closed there too.
            (ModelFacts { mesh_groups: vec!["icicles_@0_#&a".into(), "icicles_@1_#&b".into()], ..plain() }, REASON_BODY_GROUPS),
        ];
        // The first proven profile, word for word, with that one correction.
        let first = |f: &ModelFacts, placed: bool| offered_with(f, placed, &Switches::OFF);
        for (facts, reason) in &cases {
            assert_eq!(first(facts, false), Err(*reason), "{facts:?}");
            assert_eq!(first_profile(facts, false), Err(*reason));
        }
        assert_eq!(first(&plain(), true), Err(REASON_PLACED));
        // A mesh skeleton of zero bones (the particle-drawn dagger's) is no skeleton.
        assert_eq!(offered(&ModelFacts { mesh_skeleton_bones: Some(0), ..plain() }, false), Ok(()));
        // With the app's switches: what stays closed, and its sentence.
        let now: Vec<std::result::Result<(), &str>> = cases.iter().map(|(f, _)| offered(f, false)).collect();
        assert_eq!(
            now,
            [
                Ok(()), // one bone, drawn by effects only
                Ok(()), // several bones
                Err(REASON_OTHER_COLLISION), // a physics block with no sphere or capsule
                Err(REASON_OTHER_COLLISION), // a physics file
                Ok(()),
                Err(REASON_ANIMATED),
                Err(REASON_ANIMATED),
                Err(REASON_ANIMATED),
                Ok(()),
                Err(REASON_NO_MATERIAL),
                Ok(()),
                Ok(()), // body groups with named choices
            ]
        );
        // A body group on an object that fails earlier keeps its earlier reason.
        assert_eq!(first_profile(&ModelFacts { mesh_groups: vec!["a".into()], hitboxes: 1, ..plain() }, false), Err(REASON_HITBOXES));
        assert_eq!(offered(&plain(), true), Ok(()));
    }

    #[test]
    fn the_app_s_switches_are_the_enabled_kinds() {
        // Each "on" is a kind enabled in the app, and every object it offers was built
        // through the real build path.
        assert_eq!(
            SWITCHES,
            Switches {
                one_bone_placed: true,
                one_bone_drawn: true,
                simple_collision: true,
                attachments: true,
                hitboxes: true,
                placed: true,
                several_materials: true,
                outline_shells: Some(Shells::Generated),
                body_groups: true,
                several_bones: true,
                same_skeleton: true,
                kept_animation: true,
                kept_collision: true,
                kept_animation_by_name: true,
                kept_graph_animation: true,
                kept_collision_on_bones: true,
                parts_kept: true,
                // The one object it would offer that does not build is closed by name.
                written_rest_signs: true,
                written_rest_scales: true,
                written_key_order: true,
                written_body_group_names: true,
                written_break_pieces: true,
                // The spectre hand and the hat.
                unstated_frame_collision: true,
                // The bounce pad; the wall while it travels.
                one_bone_animated: true,
                boneless_animated: true,
                // The standing wall.
                listed_by_file_name: true,
                // The briefcase with its glow shell.
                body_group_shells: true,
                // The gravestone in its states.
                body_group_states: true,
                // The gravestone's crumbling, the ice path, the winged icon.
                without_morphs: true,
                // The hand with its bone constraints, the door with its frame and portal
                // surface, the ground impact with the pieces of its second state.
                written_bone_constraints: true,
                parts_together: true,
                state_parts: true,
                // Rem's helper and the Rat King's nibbler with the game's cloth.
                kept_cloth: true,
                // The cat form: its morph block left out beside its bones, its collision
                // kept on a skeleton a rebuild lists in another order.
                without_morphs_beside_bones: true,
                kept_collision_reordered: true,
                // Sinclair's assistant, its cloth from a proxy mesh, with its own
                // bone-constraint bound allowed for this kind only.
                kept_cloth_proxy: true,
                // The pigeon with its shawl kept in every level of detail.
                parts_kept_in_levels: true,
            }
        );
        assert_eq!(switches(), SWITCHES);
        // Every kind switched on in the app is open; only the outline choice can differ.
        assert_eq!(SWITCHES, Switches::ALL_KEPT);
        // `EVERY` opens the kinds not switched on yet as well: none is left.
        assert_eq!(Switches::EVERY, SWITCHES);
        assert!(SWITCHES.kept_cloth_proxy);
        assert_eq!(
            Switches::OFF,
            Switches {
                one_bone_placed: false,
                one_bone_drawn: false,
                simple_collision: false,
                attachments: false,
                hitboxes: false,
                placed: false,
                several_materials: false,
                outline_shells: None,
                body_groups: false,
                several_bones: false,
                same_skeleton: false,
                kept_animation: false,
                kept_collision: false,
                kept_animation_by_name: false,
                kept_graph_animation: false,
                kept_collision_on_bones: false,
                parts_kept: false,
                written_rest_signs: false,
                written_rest_scales: false,
                written_key_order: false,
                written_body_group_names: false,
                written_break_pieces: false,
                unstated_frame_collision: false,
                one_bone_animated: false,
                boneless_animated: false,
                listed_by_file_name: false,
                body_group_shells: false,
                body_group_states: false,
                without_morphs: false,
                written_bone_constraints: false,
                parts_together: false,
                state_parts: false,
                kept_cloth: false,
                without_morphs_beside_bones: false,
                kept_collision_reordered: false,
                kept_cloth_proxy: false,
                parts_kept_in_levels: false,
            }
        );
    }

    #[test]
    fn one_bone_is_offered_on_a_placed_object_and_not_on_one_effects_draw() {
        // The keg (placed by the game, one bone) and the arrow (one bone, only effects draw it): both offered.
        assert_eq!(offered(&keg(), true), Ok(()));
        let arrow = ModelFacts { bones: vec!["arrow".into()], mesh_skeleton_bones: Some(1), ..plain() };
        assert_eq!(offered(&arrow, false), Ok(()));
        assert_eq!(offered(&arrow, true), Ok(()));
        assert_eq!(object_kind(&arrow, false).map(|k| k.label), Some("An object an effect draws, with one bone".to_owned()));
        assert_eq!(offered_with(&arrow, false, &Switches { one_bone_drawn: false, ..SWITCHES }), Err(REASON_SKELETON));
        // An effect-drawn one-bone object with something still closed keeps that reason.
        assert_eq!(offered(&ModelFacts { mesh_groups: vec!["a_@0_#&x".into()], ..arrow.clone() }, false), Ok(()), "body groups are open");
        assert_eq!(offered_with(&ModelFacts { mesh_groups: vec!["a_@0".into()], unnamed_body_choices: true, ..arrow.clone() }, false, &Switches { written_body_group_names: false, ..SWITCHES }), Err(REASON_BODY_GROUP_NAMES));
        assert_eq!(offered(&ModelFacts { mesh_groups: vec!["a_@0".into()], unnamed_body_choices: true, ..arrow.clone() }, false), Ok(()), "older body-group names are written back");
        assert_eq!(offered(&ModelFacts { physics_block: true, physics_shapes: BTreeMap::from([("hull".into(), 2)]), ..arrow.clone() }, false), Err(REASON_OTHER_COLLISION));
        assert_eq!(offered(&ModelFacts { anim_graph: Some("a.vnmgraph".into()), ..arrow.clone() }, false), Err(REASON_ANIMATED));
        assert_eq!(offered(&ModelFacts { materials: vec!["a.vmat".into(), "b.vmat".into()], ..arrow.clone() }, false), Ok(()), "several materials are open");
        assert_eq!(offered_with(&keg(), true, &Switches { one_bone_placed: false, ..SWITCHES }), Err(REASON_SKELETON));
        let two = ModelFacts { bones: vec!["a".into(), "b".into()], ..plain() };
        assert_eq!(offered(&two, true), Ok(()), "several bones are open");
        assert_eq!(offered_with(&two, false, &Switches { several_bones: false, ..SWITCHES }), Err(REASON_BONES));
    }

    /// The keg-like object: one bone, a capsule, two attachments, placed, three
    /// levels of a body and an outline shell.
    fn keg() -> ModelFacts {
        let mesh = |level: u32, material: &str| MeshFacts {
            lod_mask: 1 << level,
            materials: vec![material.into()],
            // The body ±16, its outline 0.7 further out.
            bounds: Some(if material.ends_with("glow.vmat") { Bounds { min: [-16.7; 3], max: [16.7; 3] } } else { Bounds { min: [-16.; 3], max: [16.; 3] } }),
        };
        ModelFacts {
            bones: vec!["joint1".into()],
            mesh_skeleton_bones: Some(1),
            embedded_meshes: 6,
            physics_block: true,
            physics_shapes: BTreeMap::from([("capsule".into(), 1)]),
            attachments: vec!["attach_fx@joint1".into(), "statusbar@joint1".into()],
            materials: vec!["models/a/body.vmat".into(), "models/a/glow.vmat".into()],
            lod_meshes: vec![2, 2, 2],
            lod_switch_distances: vec![0., 10., 20.],
            meshes: (0..3).flat_map(|l| [mesh(l, "models/a/body.vmat"), mesh(l, "models/a/glow.vmat")]).collect(),
            ..plain()
        }
    }
    /// Facts for every combination of the features the switches concern, and the ones they never open.
    fn grid() -> Vec<(ModelFacts, bool)> {
        let mut out = vec![];
        for bones in 0..3 {
            for physics in 0..4 {
                for bits in 0..64u32 {
                    let mut f = plain();
                    match bones {
                        1 => {
                            f.bones = vec!["b".into()];
                            f.mesh_skeleton_bones = Some(1);
                        }
                        2 => f.bones = vec!["a".into(), "b".into()],
                        _ => {}
                    }
                    match physics {
                        1 => {
                            f.physics_block = true;
                            f.physics_shapes = BTreeMap::from([("capsule".into(), 1)]);
                        }
                        2 => {
                            f.physics_block = true;
                            f.physics_shapes = BTreeMap::from([("hull".into(), 3)]);
                        }
                        3 => {
                            f.physics_refs = 1;
                            f.physics_shapes = BTreeMap::from([("file".into(), 1)]);
                        }
                        _ => {}
                    }
                    if bits & 1 != 0 {
                        f.hitboxes = 2;
                    }
                    if bits & 2 != 0 {
                        f.attachments = vec!["a@b".into()];
                    }
                    if bits & 4 != 0 {
                        f.materials.push("models/a/b.vmat".into());
                    }
                    if bits & 8 != 0 {
                        let k = keg();
                        f.materials.push("models/a/glow.vmat".into());
                        (f.lod_meshes, f.lod_switch_distances, f.meshes) = (k.lod_meshes, k.lod_switch_distances, k.meshes);
                        f.meshes.iter_mut().filter(|m| m.materials[0] == "models/a/body.vmat").for_each(|m| m.materials = vec![f.materials[0].clone()]);
                    }
                    if bits & 16 != 0 {
                        f.anim_graph = Some("a.vnmgraph".into());
                    }
                    out.push((f, bits & 32 != 0));
                }
            }
        }
        out.push((ModelFacts { materials: vec![], ..plain() }, false));
        out
    }

    /// An animated object with body groups whose two meshes are bound differently:
    /// `masks` and `groups` as the game's file holds them, default `default`.
    fn two_parts(groups: &[&str], masks: [u64; 2], default: u64) -> ModelFacts {
        let mesh = |m: &str, x: f64| MeshFacts { lod_mask: 0, materials: vec![m.into()], bounds: Some(Bounds { min: [-x; 3], max: [x; 3] }) };
        ModelFacts {
            bones: vec!["root".into(), "body".into()],
            mesh_skeleton_bones: Some(2),
            embedded_meshes: 2,
            anim_graph: Some("a.vnmgraph".into()),
            materials: vec!["models/a/body.vmat".into(), "models/a/marker.vmat".into()],
            mesh_groups: groups.iter().map(|g| (*g).to_owned()).collect(),
            meshes: vec![mesh("models/a/body.vmat", 30.), mesh("models/a/marker.vmat", 300.)],
            mixed_mesh_bindings: true,
            mesh_names: vec!["body".into(), "markers".into()],
            mesh_group_masks: masks.to_vec(),
            default_mesh_group_mask: Some(default),
            mesh_bindings: vec![vec!["root".into(), "body".into()], vec![]],
            ..plain()
        }
    }

    #[test]
    fn parts_kept_opens_a_part_of_another_body_group_and_never_another_state() {
        // A part another body group shows (range markers beside the object): kept.
        let turret = two_parts(&["default_@0_#&turret", "Range_Markers_@0_#&default_turret", "Range_Markers_@1_#&rangemarkers"], [1, 4], 3);
        let parts = kept_parts(&turret).expect("one replaced, one kept");
        assert_eq!(parts.replaced, vec![0]);
        assert_eq!(parts.names(), vec!["markers".to_owned()]);
        assert_eq!(kept_part_line(&parts.kept[0]), "Its range markers, kept as the game's own");
        assert_eq!(turret.body_bounds(), Some(Bounds { min: [-30.; 3], max: [30.; 3] }), "the kept part is not what the fit matches");
        assert_eq!(turret.body_materials(), vec!["models/a/body.vmat".to_owned()]);
        assert_eq!(offered(&turret, true), Ok(()), "open in the app");
        assert_eq!(offered_with(&turret, true, &Switches { parts_kept: false, ..SWITCHES }), Err(REASON_BODY_GROUP_BINDINGS), "closed by its own switch, word for word");
        let kind = object_kind(&turret, true).unwrap();
        assert!(kind.kept.contains(&"Its range markers, kept as the game's own".to_owned()) && kind.kept.contains(&KEPT_BODY_GROUP_PARTS.to_owned()), "{kind:?}");
        // Another choice of the replaced mesh's own group is another state of the object: closed, word for word.
        let states = two_parts(&["STATE_@0_#&default", "STATE_@1_#&critical"], [1, 2], 1);
        assert_eq!(kept_parts(&states), None);
        assert_eq!(offered_with(&states, true, &Switches { body_group_states: false, ..Switches::EVERY }), Err(REASON_BODY_GROUP_BINDINGS));
        // Both meshes shown by default (two replaced): closed.
        assert_eq!(kept_parts(&two_parts(&["default_@0_#&a", "other_@0_#&b"], [1, 2], 3)), None);
        // Without mixed bindings nothing is kept, whatever the masks say.
        assert_eq!(kept_parts(&ModelFacts { mixed_mesh_bindings: false, ..turret.clone() }), None);
    }

    #[test]
    fn each_written_kind_opens_only_its_own_fact() {
        use crate::write_back::WrittenKind;
        // The app's switches with every written kind closed again.
        let closed = Switches { written_rest_signs: false, written_rest_scales: false, written_key_order: false, written_body_group_names: false, written_break_pieces: false, ..SWITCHES };
        let two = ModelFacts { bones: vec!["a".into(), "b".into()], ..plain() };
        let cases = [
            (ModelFacts { rest_sign_flips: true, ..two.clone() }, REASON_REST_SIGN, WrittenKind::RestSigns, Switches { written_rest_signs: true, ..closed }),
            (ModelFacts { rest_scale: true, ..two.clone() }, REASON_REST_SCALE, WrittenKind::RestScales, Switches { written_rest_scales: true, ..closed }),
            (ModelFacts { key_values_reordered: true, ..two.clone() }, REASON_GAME_DATA_ORDER, WrittenKind::KeyOrder, Switches { written_key_order: true, ..closed }),
            (ModelFacts { mesh_groups: vec!["default_@0".into()], unnamed_body_choices: true, ..two.clone() }, REASON_BODY_GROUP_NAMES, WrittenKind::BodyGroupNames, Switches { written_body_group_names: true, ..closed }),
        ];
        for (facts, reason, kind, open) in &cases {
            assert_eq!(offered_with(facts, false, &closed), Err(*reason), "closed by its own switch, word for word");
            assert_eq!(offered_with(facts, false, open), Ok(()));
            assert_eq!(written_kinds(facts), BTreeSet::from([*kind]));
            assert!(written_open(*kind, open) && !written_open(*kind, &closed));
            // In the app every written kind is open.
            assert_eq!(offered(facts, false), Ok(()));
            // Every other written switch leaves it closed.
            for (_, _, other, switches) in &cases {
                if other != kind {
                    assert_eq!(offered_with(facts, false, switches), Err(*reason));
                }
            }
        }
        let breaks = ModelFacts { bones: vec!["a".into(), "b".into()], break_pieces: true, ..plain() };
        assert_eq!(offered_with(&breaks, false, &closed), Err(REASON_BREAK_PIECES));
        assert_eq!(offered_with(&breaks, false, &Switches { written_break_pieces: true, ..closed }), Ok(()));
        assert_eq!(offered(&breaks, false), Ok(()));
        // Nothing to write for an object without the facts.
        assert!(written_kinds(&two).is_empty());
    }

    #[test]
    fn bone_constraints_open_only_by_their_own_switch_which_the_app_has_on() {
        use crate::write_back::WrittenKind;
        let held = ModelFacts { bones: vec!["a".into(), "b".into()], bone_constraints: true, ..plain() };
        assert_eq!(written_kinds(&held), BTreeSet::from([WrittenKind::BoneConstraints]));
        // Its own switch closed: closed, word for word.
        let closed = Switches { written_bone_constraints: false, ..SWITCHES };
        assert_eq!(offered_with(&held, false, &closed), Err(REASON_BONE_CONSTRAINTS));
        assert!(!written_open(WrittenKind::BoneConstraints, &closed));
        // Its own switch opens it, and only it does; the app has it on.
        let open = SWITCHES;
        assert!(open.written_bone_constraints);
        assert_eq!(offered(&held, false), Ok(()));
        assert_eq!(offered_with(&held, false, &open), Ok(()));
        assert_eq!(offered_with(&held, false, &Switches::EVERY), Ok(()));
        assert!(written_open(WrittenKind::BoneConstraints, &open));
        for other in [WrittenKind::RestSigns, WrittenKind::RestScales, WrittenKind::KeyOrder, WrittenKind::BodyGroupNames, WrittenKind::BreakPieces] {
            assert_eq!(written_open(other, &open), written_open(other, &closed));
        }
        // An object without them gets no such write, under any switch.
        assert!(!written_kinds(&ModelFacts { bones: vec!["a".into(), "b".into()], ..plain() }).contains(&WrittenKind::BoneConstraints));
        // The page lists no file detail for it (the receipt records the write).
        assert_eq!(object_kind(&held, false).map(|k| k.kept), object_kind(&ModelFacts { bone_constraints: false, ..held.clone() }, false).map(|k| k.kept));
    }

    #[test]
    fn the_kept_list_names_break_pieces_but_no_written_file_detail() {
        // Resting turns and sizes, game data order and older body-group names are written back
        // but describe only how the file stores something: the receipt records them, the page does not.
        let detail = ModelFacts {
            bones: vec!["a".into(), "b".into()],
            rest_sign_flips: true,
            rest_scale: true,
            key_values_reordered: true,
            mesh_groups: vec!["default_@0".into()],
            unnamed_body_choices: true,
            ..plain()
        };
        assert_eq!(written_kinds(&detail).len(), 4);
        let kind = object_kind(&detail, false).unwrap();
        assert_eq!(kind.kept, vec![KEPT_BODY_GROUPS.to_owned(), KEPT_BONES.to_owned()]);
        let breaks = object_kind(&ModelFacts { break_pieces: true, ..detail }, false).unwrap();
        assert_eq!(breaks.kept, vec![KEPT_BODY_GROUPS.to_owned(), KEPT_BREAK_PIECES.to_owned(), KEPT_BONES.to_owned()]);
    }

    #[test]
    fn with_every_switch_off_the_first_profile_is_unchanged() {
        for (facts, placed) in grid() {
            assert_eq!(offered_with(&facts, placed, &Switches::OFF), first_profile(&facts, placed), "{facts:?} placed {placed}");
            // With the app's switches every object of the first kind is offered as before.
            if first_profile(&facts, placed).is_ok() {
                assert_eq!(offered(&facts, placed), Ok(()));
            }
        }
        // An object the first profile offers stays offered with every switch, and carries no kind.
        for switches in [Switches::OFF, SWITCHES, Switches::ALL, Switches { outline_shells: Some(Shells::Dropped), ..Switches::ALL }] {
            assert_eq!(offered_with(&plain(), false, &switches), Ok(()));
        }
        assert_eq!(features(&plain(), false), BTreeSet::new());
        assert_eq!(object_kind(&plain(), false), None);
    }

    #[test]
    fn each_switch_opens_only_its_own_feature() {
        let one = |f: &ModelFacts, placed: bool, s: Switches| offered_with(f, placed, &s);
        let off = Switches::OFF;
        let bone = ModelFacts { bones: vec!["b".into()], mesh_skeleton_bones: Some(1), ..plain() };
        assert_eq!(one(&bone, false, off), Err(REASON_SKELETON));
        assert_eq!(one(&bone, false, Switches { one_bone_drawn: true, ..off }), Ok(()));
        assert_eq!(one(&bone, false, Switches { one_bone_placed: true, ..off }), Err(REASON_SKELETON), "drawn by effects: its own switch");
        assert_eq!(one(&bone, true, Switches { one_bone_placed: true, placed: true, ..off }), Ok(()));
        assert_eq!(one(&bone, true, Switches { one_bone_drawn: true, placed: true, ..off }), Err(REASON_SKELETON), "placed: its own switch");
        let two = ModelFacts { bones: vec!["a".into(), "b".into()], ..plain() };
        assert_eq!(one(&two, false, Switches { several_bones: false, ..Switches::ALL }), Err(REASON_BONES), "several bones: their own switch");
        assert_eq!(one(&two, false, Switches { several_bones: true, ..off }), Ok(()));
        let moving = ModelFacts { anim_graph: Some("a.vnmgraph".into()), ..two.clone() };
        assert_eq!(one(&moving, true, Switches { several_bones: true, placed: true, ..off }), Ok(()), "animated with several bones");
        let legacy = ModelFacts { animation_blocks: vec!["ANIM".into()], ..two.clone() };
        assert_eq!(one(&legacy, true, Switches { several_bones: true, placed: true, ..off }), Err(REASON_LEGACY_ANIMATION), "a legacy animation block alone stays closed");
        assert_eq!(one(&moving, true, Switches { placed: true, ..Switches::ALL }), Ok(()));
        assert_eq!(one(&moving, true, Switches { several_bones: false, ..Switches::ALL }), Err(REASON_BONES));
        assert_eq!(one(&ModelFacts { physics_block: true, physics_shapes: BTreeMap::from([("hull".into(), 3)]), ..moving.clone() }, true, Switches::ALL), Err(REASON_OTHER_COLLISION), "hulls stay closed");
        let capsule = ModelFacts { physics_block: true, physics_shapes: BTreeMap::from([("capsule".into(), 1)]), ..plain() };
        assert_eq!(one(&capsule, false, Switches { one_bone_drawn: true, ..off }), Err(REASON_COLLISION));
        assert_eq!(one(&capsule, false, Switches { simple_collision: true, ..off }), Ok(()));
        let hull = ModelFacts { physics_block: true, physics_shapes: BTreeMap::from([("hull of 8 points".into(), 1)]), ..plain() };
        assert_eq!(one(&hull, false, Switches::ALL), Err(REASON_OTHER_COLLISION), "a box is a hull: never carried");
        let hit = ModelFacts { hitboxes: 1, ..plain() };
        assert_eq!(one(&hit, false, Switches { placed: true, ..off }), Err(REASON_HITBOXES));
        assert_eq!(one(&hit, false, Switches { hitboxes: true, ..off }), Ok(()));
        let points = ModelFacts { attachments: vec!["a@b".into()], ..plain() };
        assert_eq!(one(&points, false, Switches { hitboxes: true, ..off }), Err(REASON_ATTACHMENTS));
        assert_eq!(one(&points, false, Switches { attachments: true, ..off }), Ok(()));
        let colours = ModelFacts { materials: vec!["a.vmat".into(), "b.vmat".into()], ..plain() };
        assert_eq!(one(&colours, false, Switches { attachments: true, ..off }), Err(REASON_MATERIALS));
        assert_eq!(one(&colours, false, Switches { several_materials: true, ..off }), Ok(()));
        assert_eq!(one(&plain(), true, Switches { several_materials: true, ..off }), Err(REASON_PLACED));
        assert_eq!(one(&plain(), true, Switches { placed: true, ..off }), Ok(()));
        let groups = ModelFacts { mesh_groups: vec!["default_@0_#&a".into(), "default_@1_#&b".into()], materials: vec!["a.vmat".into(), "b.vmat".into()], ..plain() };
        assert_eq!(one(&groups, false, Switches { body_groups: false, ..Switches::ALL }), Err(REASON_BODY_GROUPS), "body groups: their own switch");
        assert_eq!(one(&groups, false, Switches::ALL), Ok(()));
        let grouped = ModelFacts { mesh_groups: groups.mesh_groups.clone(), ..plain() };
        assert_eq!(one(&grouped, false, off), Err(REASON_BODY_GROUPS), "closed in the first kind too");
        assert_eq!(one(&grouped, false, SWITCHES), Ok(()));
        assert_eq!(one(&grouped, false, Switches { body_groups: true, ..off }), Ok(()));
        // What a rebuild cannot keep closes body groups, animation data and break pieces under every switch.
        assert_eq!(one(&ModelFacts { unnamed_body_choices: true, ..grouped.clone() }, false, Switches::ALL), Err(REASON_BODY_GROUP_NAMES));
        assert_eq!(one(&ModelFacts { mixed_mesh_bindings: true, ..grouped.clone() }, false, Switches::ALL), Err(REASON_BODY_GROUP_BINDINGS));
        assert_eq!(one(&ModelFacts { mixed_mesh_bindings: true, ..plain() }, false, Switches::ALL), Ok(()), "bindings matter only with body groups");
        assert_eq!(one(&ModelFacts { decode_reordered: true, ..moving.clone() }, true, Switches::ALL), Err(REASON_BONE_ORDER));
        assert_eq!(one(&ModelFacts { animation_folders: true, ..moving.clone() }, true, Switches::ALL), Err(REASON_ANIMATION_FOLDERS));
        assert_eq!(one(&ModelFacts { decode_reordered: true, animation_folders: true, ..two.clone() }, true, Switches::ALL), Ok(()), "animation facts matter only when animated");
        assert_eq!(one(&ModelFacts { break_pieces: true, ..capsule.clone() }, false, Switches::ALL), Err(REASON_BREAK_PIECES));
        assert_eq!(one(&ModelFacts { break_pieces: true, hitboxes: 1, ..capsule.clone() }, false, off), Err(REASON_COLLISION), "an earlier reason comes first");
        let animated = ModelFacts { anim_graph: Some("a".into()), ..plain() };
        assert_eq!(one(&animated, false, Switches::ALL), Err(REASON_ANIMATED), "animation without several bones is never offered");
        // The keg needs every switch it has: one bone, collision, attachments, placed and shells.
        let keg = keg();
        let all = Switches { hitboxes: false, several_materials: false, ..Switches::ALL };
        assert_eq!(one(&keg, true, all), Ok(()));
        assert_eq!(one(&keg, true, Switches { outline_shells: Some(Shells::Dropped), ..all }), Ok(()));
        for without in [Switches { one_bone_placed: false, ..all }, Switches { simple_collision: false, ..all }, Switches { attachments: false, ..all }, Switches { placed: false, ..all }, Switches { outline_shells: None, ..all }] {
            assert!(one(&keg, true, without).is_err(), "{without:?}");
        }
        assert_eq!(one(&keg, true, Switches { outline_shells: None, ..all }), Err(REASON_MATERIALS), "without shells the outline's material counts");
        assert_eq!(one(&keg, true, Switches { outline_shells: None, several_materials: true, ..all }), Err(REASON_SHELLS));
    }

    #[test]
    fn outline_shells_are_one_material_on_the_second_mesh_of_every_level() {
        let keg = keg();
        assert_eq!(keg.shell_material().as_deref(), Some("models/a/glow.vmat"));
        assert_eq!(keg.body_materials(), ["models/a/body.vmat"]);
        assert_eq!(features(&keg, true), BTreeSet::from([Feature::OneBone, Feature::SimpleCollision, Feature::Attachments, Feature::Placed, Feature::OutlineShells]));
        // One level of three meshes, or a level without a second mesh: not an outline.
        let mut odd = keg.clone();
        odd.lod_meshes = vec![2, 2, 1];
        odd.meshes.pop();
        odd.embedded_meshes = 5;
        assert_eq!(odd.shell_material(), None);
        assert!(features(&odd, true).contains(&Feature::ExtraMeshes));
        assert_eq!(offered_with(&odd, true, &Switches::ALL), Err(REASON_EXTRA_MESHES));
        // A second mesh drawn with the body's material is not an outline either.
        let mut same = keg.clone();
        same.meshes[1].materials = vec!["models/a/body.vmat".into()];
        assert_eq!(same.shell_material(), None);
        // One mesh per level is no extra mesh.
        let levels = ModelFacts { lod_meshes: vec![1, 1], meshes: vec![MeshFacts { lod_mask: 1, ..Default::default() }, MeshFacts { lod_mask: 2, ..Default::default() }], ..plain() };
        assert!(!levels.extra_meshes() && features(&levels, false).is_empty());
        // The shell's distance from the body, and the body's own bounds.
        assert!((keg.shell_distance().unwrap() - 0.7).abs() < 1e-9);
        assert_eq!(keg.body_bounds(), Some(Bounds { min: [-16.; 3], max: [16.; 3] }));
        assert_eq!(odd.shell_distance(), None);
    }

    #[test]
    fn a_switched_kind_names_what_stays_the_games() {
        let kind = object_kind(&keg(), true).unwrap();
        assert_eq!(kind.label, "An object the game places, with collision, one bone, attachment points and an outline");
        assert_eq!(kind.kept, [KEPT_COLLISION, KEPT_PLACED, KEPT_ATTACHMENTS, KEPT_BONE]);
        assert!(kind.collision && kind.stands_on);
        let bone = object_kind(&ModelFacts { bones: vec!["b".into()], ..plain() }, false).unwrap();
        assert_eq!((bone.label.as_str(), bone.collision, bone.stands_on), ("An object an effect draws, with one bone", false, false));
        let colours = object_kind(&ModelFacts { materials: vec!["a.vmat".into(), "b.vmat".into()], ..plain() }, false).unwrap();
        assert!(colours.kept.is_empty() && colours.label.ends_with("with several materials"));
    }

    #[test]
    fn collision_shapes_measure_how_far_an_object_sticks_out() {
        let capsule = CollisionShape::Capsule { start: [0., -1., 0.], end: [0., 1., 0.], radius: 16. };
        assert!((capsule.distance([0., 0., 0.]) + 16.).abs() < 1e-12, "inside: negative");
        assert!((capsule.distance([0., 0.5, 20.]) - 4.).abs() < 1e-12);
        assert!((capsule.distance([0., 11., 0.]) + 6.).abs() < 1e-12, "past the ends: from the end point");
        let ball = CollisionShape::Sphere { center: [30., 0., 0.], radius: 2. };
        assert_eq!(sticks_out([[0., 0., 10.], [31., 0., 0.]], &[capsule.clone(), ball.clone()]), Some(0.), "inside one shape is inside");
        assert!((sticks_out([[0., 0., 10.], [0., 0., 19.]], &[capsule.clone(), ball]).unwrap() - 3.).abs() < 1e-12);
        assert_eq!(sticks_out([[0.; 3]], &[]), None, "no collision: nothing to say");
        assert_eq!(serde_json::to_value(&capsule).unwrap()["shape"], "capsule");
        // The line compares with how far the game's own object sticks out.
        let calm = collision_fit(16.04, 15.0);
        assert!(calm.fits && calm.line.starts_with("Your object fits the game's collision") && calm.line.contains("16 in") && calm.line.contains("by 15 in"), "{}", calm.line);
        let warn = collision_fit(24.8, 15.0);
        assert!(!warn.fits && warn.line.contains("by up to 24.8 in") && warn.line.contains("does not match what they see"));
        assert!(collision_fit(FITS_COLLISION, 0.).fits && !collision_fit(FITS_COLLISION + 0.01, 0.).fits);
    }

    #[test]
    fn the_read_back_compares_every_row_but_the_mesh_and_notes_hitbox_crcs() {
        let rows = |pairs: &[(&str, &str)]| pairs.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect::<BTreeMap<_, _>>();
        let game = rows(&[(".m_modelSkeleton.m_boneName[0]", "\"joint1\""), (".m_lodGroupSwitchDistances[1]", "10"), (".m_parts[0].m_flMass", "500"), (".m_hitboxsets[0].value.m_HitBoxes[0].m_nCRC", "1"), (".m_refMeshes[0]", "\"a\"")]);
        let ours = rows(&[(".m_modelSkeleton.m_boneName[0]", "\"joint1\""), (".m_lodGroupSwitchDistances[1]", "10"), (".m_parts[0].m_flMass", "500.00000001"), (".m_hitboxsets[0].value.m_HitBoxes[0].m_nCRC", "2"), (".m_animGraph2Refs", "[]")]);
        let mut check = CarriedCheck::default();
        compare_rows("X", "DATA", &game, &ours, &mut check);
        assert!(check.differs.is_empty(), "{check:?}");
        assert_eq!(check.compared["X"], (3, 3), "mesh rows and CRCs are not compared; an empty default a newer compiler writes is the same");
        assert_eq!(check.noted.len(), 2);
        let mut odd = CarriedCheck::default();
        compare_rows("Z", "DATA", &game, &rows(&[(".m_modelSkeleton.m_boneName[0]", "\"joint1\""), (".m_parts[0].m_flMass", "500"), (".m_animGraph2Refs[0]", "\"a.vnmgraph\"")]), &mut odd);
        assert_eq!(odd.differs.len(), 1, "an added value is not an empty default: {:?}", odd.differs);
        // An empty value the game has and ours lacks, or an empty default in the physics, differs.
        let mut lost = CarriedCheck::default();
        compare_rows("L", "DATA", &rows(&[(".m_animGraph2Refs", "[]")]), &rows(&[]), &mut lost);
        compare_rows("P", "PHYS", &rows(&[]), &rows(&[(".m_joints", "[]")]), &mut lost);
        assert_eq!(lost.differs.len(), 2, "{:?}", lost.differs);
        let lost = rows(&[(".m_modelSkeleton.m_boneName[0]", "\"joint2\"")]);
        compare_rows("Y", "DATA", &game, &lost, &mut check);
        assert_eq!(check.differs.len(), 2, "{:?}", check.differs);
        assert!(mesh_row("MDAT", ".m_skeleton.m_bones[0].m_bbox.m_vecSize[0]") && !mesh_row("PHYS", ".m_parts[0].m_rnShape.m_capsules[0].m_Capsule.m_flRadius"));
        // Hit-area bounds and the physics' bone hashes are compared.
        assert!(!mesh_row("MDAT", ".m_hitboxsets[0].value.m_HitBoxes[0].m_vMinBounds[0]"));
        assert!(!mesh_row("PHYS", ".m_bonesHash[0]"));
        assert!(mesh_row("MDAT", ".m_sceneObjects[0].m_vMinBounds[0]"));
    }

    #[test]
    fn kept_differences_ignore_the_mesh_materials_and_bounds() {
        let original = ModelFacts { blocks: vec!["MDAT".into(), "MBUF".into()], other_refs: vec!["particles/a.vsnap".into()], bounds: Some(Bounds { min: [0.; 3], max: [1.; 3] }), ..plain() };
        let compiled = ModelFacts { blocks: vec!["MVTX".into(), "MIDX".into()], materials: vec!["models/dmb_ability_objects/x/m.vmat".into()], bounds: None, ..original.clone() };
        assert!(kept_differences(&original, &compiled).is_empty());
        let lost = ModelFacts { other_refs: vec![], attachments: vec!["a@b".into()], ..compiled };
        assert_eq!(kept_differences(&original, &lost), ["attachments", "non-material references"]);
    }

    #[test]
    fn renderers_name_their_models_and_override_materials() {
        let root = obj(vec![(
            "m_Renderers",
            arr(vec![
                obj(vec![("_class", s("C_OP_RenderSprites")), ("m_hTexture", s("materials/a.vtex"))]),
                obj(vec![
                    ("_class", s("C_OP_RenderModels")),
                    ("m_ModelList", arr(vec![obj(vec![("m_model", s("models/abilities/Sample_Dagger_Model.vmdl"))])])),
                    ("m_hOverrideMaterial", s("materials/dev/gray_metal.vmat")),
                ]),
                obj(vec![("_class", s("C_OP_RenderModels")), ("m_ModelList", arr(vec![obj(vec![("m_model", s("models/particle/orb.vmdl"))])])), ("m_hOverrideMaterial", s(""))]),
            ]),
        )]);
        assert_eq!(
            rendered_models(&root),
            [("models/abilities/sample_dagger_model.vmdl".to_owned(), Some("materials/dev/gray_metal.vmat".to_owned())), ("models/particle/orb.vmdl".to_owned(), None)]
        );
    }

    /// Test data: particles and models by path.
    #[derive(Default)]
    struct Fake {
        particles: BTreeMap<String, ParticleInfo>,
        models: BTreeMap<String, ModelFacts>,
    }
    impl Game for Fake {
        fn particle(&self, path: &str) -> Option<Arc<ParticleInfo>> {
            self.particles.get(&casefold(path)).cloned().map(Arc::new)
        }
        fn model(&self, model: &str) -> Option<std::result::Result<Arc<ModelFacts>, String>> {
            self.models.get(&model_key(model)).cloned().map(|f| Ok(Arc::new(f)))
        }
        fn models_named(&self, folder: &str, prefix: &str) -> Vec<String> {
            let start = format!("{folder}/{prefix}");
            self.models.keys().filter(|k| k.starts_with(&start) && !k[folder.len() + 1..].contains('/')).cloned().collect()
        }
    }

    #[test]
    fn a_model_beside_a_listed_one_is_listed_by_its_file_name_only_by_its_switch() {
        let roster = heroes::roster().unwrap();
        let haze = roster.iter().find(|h| h.name == "Haze").unwrap().path.clone();
        let heroes_doc = obj(vec![("hero_haze", hero_def(&haze, &[("ESlot_Signature_3", "ability_wall")]))]);
        let abilities = obj(vec![
            ("ability_wall", obj(vec![("m_eAbilityType", s("EAbilityType_Signature")), ("m_Tracer", s("particles/abilities/sample/wall.vpcf"))])),
            // Another record names this one: the data knows it, so it is not found by its name.
            ("ability_other", obj(vec![("m_eAbilityType", s("EAbilityType_Signature")), ("m_Prop", s("models/abilities/sample_wall_named.vmdl"))])),
        ]);
        let defs = Definitions::from_documents(&[("heroes".into(), heroes_doc), ("abilities".into(), abilities)]);
        let mut game = Fake::default();
        game.particles.insert("particles/abilities/sample/wall.vpcf_c".into(), ParticleInfo { looks: vec![], children: vec![], drawn: vec![("models/abilities/sample_wall.vmdl".into(), None)] });
        game.models.insert("models/abilities/sample_wall.vmdl".into(), plain());
        // The standing shape: collision of its own, named by nothing, drawn by nothing.
        game.models.insert("models/abilities/sample_wall_physics.vmdl".into(), hull());
        // A copy of the geometry without collision, a named one, one in a subfolder, one whose name does not extend.
        game.models.insert("models/abilities/sample_wall_geo.vmdl".into(), plain());
        game.models.insert("models/abilities/sample_wall_named.vmdl".into(), hull());
        game.models.insert("models/abilities/sample_wall_sub/sample_wall_x.vmdl".into(), hull());
        game.models.insert("models/abilities/sample_wallpaper.vmdl".into(), hull());
        let cancel = CancellationToken::new();
        let users = users(&game, &defs, &roster, &cancel).unwrap();
        // Off: the catalog keeps its bytes. On in the app.
        let off = hero_catalog_with(&game, &defs, &Names::default(), &users, &haze, &roster, &Switches { listed_by_file_name: false, ..SWITCHES }, &cancel).unwrap();
        assert_eq!(hero_catalog(&game, &defs, &Names::default(), &users, &haze, &roster, &cancel).unwrap().rows.len(), 2);
        assert_eq!(off.rows.iter().map(|r| r.target.as_str()).collect::<Vec<_>>(), ["models/abilities/sample_wall.vmdl"]);
        assert!(serde_json::to_value(&off.rows[0]).unwrap().get("found_by").is_none());
        let on = hero_catalog_with(&game, &defs, &Names::default(), &users, &haze, &roster, &Switches { listed_by_file_name: true, ..SWITCHES }, &cancel).unwrap();
        assert_eq!(on.rows.iter().map(|r| r.target.as_str()).collect::<Vec<_>>(), ["models/abilities/sample_wall.vmdl", "models/abilities/sample_wall_physics.vmdl"]);
        assert_eq!(on.rows[0], off.rows[0], "the listed object's row is unchanged");
        let found = &on.rows[1];
        assert_eq!((found.drawn.as_str(), found.ability.as_str(), found.offered), (DRAWN_FILE_NAME, "ability_wall", true));
        assert_eq!(found.found_by.as_deref(), Some(FOUND_BY_FILE_NAME.replace("{}", "Sample wall").as_str()));
        // Named as the object it sits beside, never from its own file name.
        assert_eq!(found.name, "Sample wall, as placed");
        assert_eq!(on.rows[0].name, "Sample wall");
        assert!(found.drawn_by.is_empty() && found.material_override.is_none() && !found.material_always_overridden);
        // The game's code places it: what it keeps stays the game's, as for other placed objects.
        let kind = found.kind.as_ref().unwrap();
        assert!(kind.features.contains(&Feature::Placed));
        assert!(kind.kept.contains(&KEPT_PLACED.to_owned()));
        assert!(allowed_in(&on).contains("models/abilities/sample_wall_physics.vmdl"));
        assert!(!allowed_in(&off).contains("models/abilities/sample_wall_physics.vmdl"));
    }

    #[test]
    fn an_effects_choice_of_look_is_read_and_said_only_where_the_build_keeps_the_games_looks() {
        let int = |n: i64| Value::new(Kind::I64(n));
        let num = |n: f64| Value::new(Kind::F64(n));
        assert_eq!(skin_looks(None), None);
        assert_eq!(skin_looks(Some(&int(2))), Some(Looks { picks: Some((2, 2)) }));
        assert_eq!(skin_looks(Some(&obj(vec![("m_nType", s("PF_TYPE_LITERAL")), ("m_flLiteralValue", num(7.))]))), Some(Looks { picks: Some((7, 7)) }));
        assert_eq!(skin_looks(Some(&obj(vec![("m_nType", s("PF_TYPE_RANDOM_UNIFORM")), ("m_flRandomMax", num(4.))]))), Some(Looks { picks: Some((0, 3)) }));
        assert_eq!(skin_looks(Some(&obj(vec![("m_nType", s("PF_TYPE_RANDOM_UNIFORM")), ("m_flRandomMax", num(1.))]))), Some(Looks { picks: Some((0, 0)) }));
        assert_eq!(skin_looks(Some(&obj(vec![("m_nType", s("PF_TYPE_PARTICLE_FIELD"))]))), Some(Looks { picks: None }), "set while playing");
        let looks = |lo, hi| Some(Looks { picks: Some((lo, hi)) });
        // A model whose looks the build points at the user's materials (no bones, its animation only drives a material value).
        let wall = ModelFacts { animation_blocks: vec!["ANIM".into()], animated_material_values: vec!["JitterAmountAnim".into()], material_groups: vec!["default".into(), "plain".into(), "good".into(), "bad".into()], ..plain() };
        assert!(wall.remaps_material_groups());
        assert_eq!(game_look(&wall, &[(true, false, looks(2, 2)), (true, false, looks(3, 3))]).map(|l| (l.always, l.line)), Some((true, GAME_LOOK_ALWAYS.to_owned())));
        assert_eq!(game_look(&wall, &[(true, false, looks(2, 2)), (true, false, None)]).map(|l| l.always), Some(false), "one effect shows the user's materials");
        assert_eq!(game_look(&wall, &[(true, false, looks(0, 3))]).map(|l| l.line), Some(GAME_LOOK_SOMETIMES.to_owned()));
        assert_eq!(game_look(&wall, &[(true, false, looks(0, 0)), (false, false, None)]), None, "the first look is the user's");
        assert_eq!(game_look(&wall, &[(true, false, looks(7, 7))]), None, "a pick past the model's looks");
        // The same picks on a model the build keeps as the game wrote it, or with one look: nothing to say.
        assert_eq!(game_look(&ModelFacts { animated_material_values: vec![], ..wall.clone() }, &[(true, false, looks(2, 2))]), None);
        assert_eq!(game_look(&ModelFacts { material_groups: vec!["default".into()], ..wall }, &[(true, false, looks(2, 2))]), None);
    }

    /// Three meshes bound to different bones in one body group: `masks` and each mesh's
    /// materials as the game's file holds them, the first choice the default.
    fn three_parts(groups: &[&str], masks: [u64; 3], materials: [&[&str]; 3], names: [&str; 3]) -> ModelFacts {
        let mesh = |m: &[&str]| MeshFacts { lod_mask: 0, materials: m.iter().map(|m| (*m).to_owned()).collect(), bounds: Some(Bounds { min: [-10.; 3], max: [10.; 3] }) };
        ModelFacts {
            bones: vec!["root".into(), "a".into(), "b".into()],
            mesh_skeleton_bones: Some(3),
            embedded_meshes: 3,
            anim_graph: Some("a.vnmgraph".into()),
            materials: materials.iter().flat_map(|m| m.iter().map(|m| (*m).to_owned())).collect::<BTreeSet<_>>().into_iter().collect(),
            mesh_groups: groups.iter().map(|g| (*g).to_owned()).collect(),
            meshes: materials.iter().map(|m| mesh(m)).collect(),
            mixed_mesh_bindings: true,
            mesh_names: names.iter().map(|n| (*n).to_owned()).collect(),
            mesh_group_masks: masks.to_vec(),
            default_mesh_group_mask: Some(1),
            mesh_bindings: vec![vec!["a".into(), "b".into()], vec!["root".into()], vec!["root".into()]],
            material_groups: vec!["default".into(), "preview".into()],
            ..plain()
        }
    }

    #[test]
    fn parts_shown_together_or_further_pieces_of_a_state_say_only_what_is_true() {
        let looks = Some(Looks { picks: Some((1, 1)) });
        // Leaves shown only while open, a frame and a plane in both choices; the leaves share the
        // frame's material, which the build keeps as the game's in every look.
        let door = three_parts(&["default_@0_#&open", "default_@1_#&closed"], [1, 3, 3], [&["m/leaf.vmat"], &["m/leaf.vmat", "m/brick.vmat"], &["m/plane.vmat"]], ["leaves", "frame", "plane"]);
        assert!(crate::object_parts::together(&door).is_some() && door.remaps_material_groups());
        assert_eq!(game_look(&door, &[(true, false, looks)]), None, "every look shows the user's own colors");
        // Leaves with a material of their own: a look reaches the user's materials, as before.
        let own = three_parts(&["default_@0_#&open", "default_@1_#&closed"], [1, 3, 3], [&["m/own.vmat"], &["m/leaf.vmat", "m/brick.vmat"], &["m/plane.vmat"]], ["leaves", "frame", "plane"]);
        assert_eq!(game_look(&own, &[(true, false, looks)]).map(|l| l.always), Some(true));
        // The door shows the user's object only where the game's object normally shows.
        let kept = object_kind(&door, true).unwrap().kept;
        assert!(kept.contains(&KEPT_BODY_GROUP_PARTS.to_owned()) && !kept.contains(&KEPT_BODY_GROUPS.to_owned()), "{kept:?}");
        // Two states, the second drawn with two meshes: each state shows the user's object.
        let ground = three_parts(&["group0_@0", "group0_@1"], [1, 2, 2], [&["m/ground.vmat"], &["m/ground.vmat"], &["m/ground.vmat"]], ["", "", ""]);
        let parts = crate::object_parts::state_parts(&ground).expect("pieces of a state");
        assert_eq!(parts.replaced, vec![0, 1]);
        let kept = object_kind(&ground, false).unwrap().kept;
        assert!(kept.contains(&KEPT_BODY_GROUPS.to_owned()) && !kept.contains(&KEPT_BODY_GROUP_PARTS.to_owned()), "{kept:?}");
        assert!(kept.contains(&crate::object_parts::KEPT_PIECES.to_owned()), "{kept:?}");
    }

    #[test]
    fn the_list_of_objects_known_not_to_rebuild_holds_exactly_one_object() {
        // The list is a single deliberate exception: adding an entry means changing this test.
        assert_eq!(KNOWN_NOT_TO_REBUILD, ["models/heroes_wip/ratking/ratking_armor.vmdl"]);
        assert!(known_not_to_rebuild("Models/Heroes_WIP/RatKing/ratking_armor.vmdl_c"));
        assert!(!known_not_to_rebuild("models/heroes_wip/ratking/ratking_grenade.vmdl"));
    }

    #[test]
    fn the_catalog_closes_an_object_known_not_to_rebuild_only_where_it_would_be_offered() {
        let roster = heroes::roster().unwrap();
        let rat = roster.iter().find(|h| h.name == "The Rat King").unwrap().path.clone();
        let armor = KNOWN_NOT_TO_REBUILD[0];
        let other = "models/heroes_wip/ratking/sample_shield.vmdl";
        let heroes_doc = obj(vec![("hero_ratking", hero_def(&rat, &[("ESlot_Signature_1", "ability_armor")]))]);
        let abilities = obj(vec![("ability_armor", obj(vec![("m_eAbilityType", s("EAbilityType_Signature")), ("m_Tracer", s("particles/abilities/sample/armor.vpcf"))]))]);
        let defs = Definitions::from_documents(&[("heroes".into(), heroes_doc), ("abilities".into(), abilities)]);
        let mut game = Fake::default();
        game.particles.insert("particles/abilities/sample/armor.vpcf_c".into(), ParticleInfo { looks: vec![], children: vec![], drawn: vec![(armor.into(), None), (other.into(), None)] });
        // Both with a resting turn the compiler writes with the other sign: offered only with the sign kind.
        let signed = ModelFacts { bones: vec!["a".into(), "b".into()], mesh_skeleton_bones: Some(2), rest_sign_flips: true, ..plain() };
        game.models.insert(armor.into(), signed.clone());
        game.models.insert(other.into(), signed);
        let cancel = CancellationToken::new();
        let users = users(&game, &defs, &roster, &cancel).unwrap();
        let row = |c: &ObjectCatalog, t: &str| c.rows.iter().find(|r| r.target == t).unwrap().clone();
        let on = hero_catalog(&game, &defs, &Names::default(), &users, &rat, &roster, &cancel).unwrap();
        let (a, o) = (row(&on, armor), row(&on, other));
        assert_eq!((a.offered, a.reason.as_deref()), (false, Some(REASON_KNOWN_NOT_TO_REBUILD)));
        assert!(a.kind.is_none() && !allowed_in(&on).contains(armor));
        assert!(o.offered && o.kind.is_some(), "nothing else is closed by the list");
        // With the sign kind off the armor keeps the sign reason as before.
        let off = hero_catalog_with(&game, &defs, &Names::default(), &users, &rat, &roster, &Switches { written_rest_signs: false, ..SWITCHES }, &cancel).unwrap();
        assert_eq!(row(&off, armor).reason.as_deref(), Some(REASON_REST_SIGN));
        assert_eq!(row(&off, other).reason.as_deref(), Some(REASON_REST_SIGN));
        // Every field but the offer, the reason and the kind is the same as for any other row.
        let mut same = a.clone();
        same.offered = true;
        same.reason = None;
        same.kind = o.kind.clone();
        assert_eq!(ObjectRow { target: o.target.clone(), name: o.name.clone(), file: o.file.clone(), ..same }, o);
    }
    fn hero_def(model: &str, bound: &[(&str, &str)]) -> Value {
        obj(vec![("m_strModelName", s(model)), ("m_bDisabled", Value::new(Kind::Bool(false))), ("m_mapBoundAbilities", obj(bound.iter().map(|(k, v)| (*k, s(v))).collect()))])
    }

    #[test]
    fn a_heros_catalog_lists_drawn_and_named_models_and_allows_only_offered_ones() {
        let roster = heroes::roster().unwrap();
        let path = |name: &str| roster.iter().find(|h| h.name == name).unwrap().path.clone();
        let (haze, geist, shiv, vindicta) = (path("Haze"), path("Lady Geist"), path("Shiv"), path("Vindicta"));
        let heroes_doc = obj(vec![
            ("hero_haze", hero_def(&haze, &[("ESlot_Signature_1", "ability_dagger"), ("ESlot_Signature_2", "ability_turret"), ("ESlot_Weapon_Primary", "weapon_haze")])),
            ("hero_ghost", hero_def(&geist, &[("ESlot_Signature_1", "ability_orb")])),
            ("hero_shiv", hero_def(&shiv, &[("ESlot_Signature_1", "ability_orb")])),
            ("hero_vindicta", hero_def(&vindicta, &[("ESlot_Signature_1", "ability_orb2")])),
        ]);
        let abilities = obj(vec![
            ("ability_dagger", obj(vec![("m_eAbilityType", s("EAbilityType_Signature")), ("m_Tracer", s("particles/abilities/sample/dagger_proj.vpcf"))])),
            ("ability_turret", obj(vec![("m_eAbilityType", s("EAbilityType_Signature")), ("m_TurretModel", s("models/abilities/sample_turret.vmdl"))])),
            ("weapon_haze", obj(vec![("m_eAbilityType", s("EAbilityType_Weapon")), ("m_Tracer", s("particles/weapon/orb.vpcf"))])),
            ("ability_orb", obj(vec![("m_eAbilityType", s("EAbilityType_Signature")), ("m_Tracer", s("particles/weapon/orb.vpcf"))])),
            ("ability_orb2", obj(vec![("m_eAbilityType", s("EAbilityType_Signature")), ("m_Tracer", s("particles/weapon/orb.vpcf"))])),
        ]);
        let defs = Definitions::from_documents(&[("heroes".into(), heroes_doc), ("abilities".into(), abilities)]);
        let mut game = Fake::default();
        game.particles.insert("particles/abilities/sample/dagger_proj.vpcf_c".into(), ParticleInfo { looks: vec![], children: vec!["particles/abilities/sample/dagger_model.vpcf_c".into()], drawn: vec![] });
        game.particles.insert("particles/abilities/sample/dagger_model.vpcf_c".into(), ParticleInfo { looks: vec![], children: vec![], drawn: vec![("models/abilities/sample_dagger.vmdl".into(), Some("materials/dev/gray_metal.vmat".into())), ("models/abilities/sample_horn.vmdl".into(), None)] });
        game.particles.insert("particles/weapon/orb.vpcf_c".into(), ParticleInfo { looks: vec![], children: vec![], drawn: vec![("models/particle/sample_orb.vmdl".into(), None)] });
        game.models.insert("models/abilities/sample_dagger.vmdl".into(), plain());
        game.models.insert("models/abilities/sample_horn.vmdl".into(), ModelFacts { bones: vec!["a".into(), "b".into()], anim_graph: Some("a.vnmgraph".into()), decode_reordered: true, ..plain() });
        game.models.insert("models/abilities/sample_turret.vmdl".into(), plain());
        game.models.insert("models/particle/sample_orb.vmdl".into(), plain());
        let cancel = CancellationToken::new();
        let users = users(&game, &defs, &roster, &cancel).unwrap();
        let catalog = hero_catalog(&game, &defs, &Names::default(), &users, &haze, &roster, &cancel).unwrap();
        let rows: Vec<(&str, bool, Option<&str>, u32)> = catalog.rows.iter().map(|r| (r.target.as_str(), r.offered, r.reason.as_deref(), r.group)).collect();
        assert_eq!(
            rows,
            [
                ("models/abilities/sample_dagger.vmdl", true, None, 101),
                ("models/abilities/sample_horn.vmdl", false, Some(REASON_BONE_ORDER), 101),
                ("models/abilities/sample_turret.vmdl", true, None, 102),
            ],
            "the orb four live heroes draw is a generic shape: never listed"
        );
        assert_eq!(catalog.counts, Counts { listed: 3, offered: 2, shared_shapes: 1 });
        // The first kind's row carries no kind; the placed turret, a switch opened, does.
        assert!(catalog.rows[0].kind.is_none());
        assert_eq!(catalog.rows[2].kind.as_ref().map(|k| k.label.as_str()), Some("An object the game places"));
        // With every switch off, the turret is closed as before.
        let first = hero_catalog_with(&game, &defs, &Names::default(), &users, &haze, &roster, &Switches::OFF, &cancel).unwrap();
        assert_eq!(first.rows[2].reason.as_deref(), Some(REASON_PLACED));
        let dagger = &catalog.rows[0];
        assert_eq!((dagger.name.as_str(), dagger.file.as_str(), dagger.drawn.as_str()), ("Sample dagger", "sample_dagger", "particle"));
        assert_eq!(dagger.material_override.as_deref(), Some("materials/dev/gray_metal.vmat"));
        assert!(dagger.material_always_overridden);
        assert!(dagger.also_used_by.is_empty());
        assert_eq!(catalog.rows[2].drawn, "definition");
        // The build's allowed targets are exactly the offered rows.
        let offered: BTreeSet<String> = catalog.rows.iter().filter(|r| r.offered).map(|r| r.target.clone()).collect();
        assert_eq!(allowed_in(&catalog), offered);
        // Another hero's catalog lists none of Haze's objects.
        let other = hero_catalog(&game, &defs, &Names::default(), &users, &vindicta, &roster, &cancel).unwrap();
        assert!(other.rows.is_empty() && other.counts.shared_shapes == 1, "{other:?}");
    }

    #[test]
    fn a_model_named_only_as_a_blocker_is_not_drawn() {
        let dome = "models/abilities/sample_dome.vmdl";
        let wall = "models/particle/blend/sample_wall.vmdl";
        let grave = "models/particle/blend/sample_grave.vmdl";
        let both = "models/abilities/sample_both.vmdl";
        let abilities = obj(vec![
            // A dome a modifier only blocks with, named by two abilities.
            ("ability_dome", obj(vec![("_class", s("ability_dome")), ("m_Modifier", obj(vec![("_class", s("modifier_sample_dome")), ("m_BlockerModel", s(dome))]))])),
            ("ability_smoke", obj(vec![("_class", s("ability_smoke")), ("m_Modifier", obj(vec![("_class", s("modifier_sample_smoke")), ("m_BlockerModel", s(&format!("{dome}_c")))]))])),
            // An invisible vision blocker, whatever holds it.
            ("ability_grove", obj(vec![("_class", s("ability_grove")), ("m_Grove", obj(vec![("m_VisionBlockerModel", s(wall)), ("m_bVisionBlockerIsInvisible", Value::new(Kind::Bool(true)))]))])),
            // A blocker key on the ability itself: what the player sees.
            ("ability_grave", obj(vec![("_class", s("ability_grave")), ("m_BlockerModel", s(grave))])),
            // Named as a blocker by a modifier and as a model of its own elsewhere.
            ("ability_both", obj(vec![("_class", s("ability_both")), ("m_Modifier", obj(vec![("_class", s("modifier_both")), ("m_SolidBlockerModel", s(both))])), ("m_Totem", s(both))])),
        ]);
        let defs = Definitions::from_documents(&[("abilities".into(), abilities)]);
        assert!(not_drawn(&defs, dome, false));
        assert!(!not_drawn(&defs, dome, true), "an effect draws it: seen");
        assert!(not_drawn(&defs, wall, false));
        assert!(!not_drawn(&defs, grave, false));
        assert!(!not_drawn(&defs, both, false));
        assert!(!not_drawn(&defs, "models/abilities/unnamed.vmdl", false), "named by no definition");
        // A visible vision blocker held by an ability is seen too.
        let visible = obj(vec![("ability_grove", obj(vec![("m_Grove", obj(vec![("m_VisionBlockerModel", s(wall)), ("m_bVisionBlockerIsInvisible", Value::new(Kind::Bool(false)))]))]))]);
        assert!(!not_drawn(&Definitions::from_documents(&[("abilities".into(), visible)]), wall, false));
    }

    #[test]
    fn the_catalog_closes_blocker_models_and_marks_scenery_beside_the_rows() {
        let roster = heroes::roster().unwrap();
        let haze = roster.iter().find(|h| h.name == "Haze").unwrap().path.clone();
        let heroes_doc = obj(vec![("hero_haze", hero_def(&haze, &[("ESlot_Signature_1", "ability_dome"), ("ESlot_Signature_2", "ability_rock")]))]);
        let abilities = obj(vec![
            ("ability_dome", obj(vec![("m_eAbilityType", s("EAbilityType_Signature")), ("m_Modifier", obj(vec![("_class", s("modifier_sample_dome")), ("m_BlockerModel", s("models/abilities/sample_dome.vmdl"))])), ("m_Turret", s("models/abilities/sample_turret.vmdl"))])),
            ("ability_rock", obj(vec![("m_eAbilityType", s("EAbilityType_Signature")), ("m_Tracer", s("particles/abilities/sample/rock.vpcf"))])),
        ]);
        let defs = Definitions::from_documents(&[("heroes".into(), heroes_doc), ("abilities".into(), abilities)]);
        let mut game = Fake::default();
        game.particles.insert("particles/abilities/sample/rock.vpcf_c".into(), ParticleInfo { looks: vec![], children: vec![], drawn: vec![("models/props/sample_rock.vmdl".into(), None)] });
        for m in ["models/abilities/sample_dome.vmdl", "models/abilities/sample_turret.vmdl", "models/props/sample_rock.vmdl"] {
            game.models.insert(m.into(), plain());
        }
        let cancel = CancellationToken::new();
        let users = users(&game, &defs, &roster, &cancel).unwrap();
        let catalog = hero_catalog(&game, &defs, &Names::default(), &users, &haze, &roster, &cancel).unwrap();
        let rows: Vec<(&str, bool, Option<&str>)> = catalog.rows.iter().map(|r| (r.target.as_str(), r.offered, r.reason.as_deref())).collect();
        assert_eq!(
            rows,
            [
                ("models/abilities/sample_dome.vmdl", false, Some(REASON_NOT_DRAWN)),
                ("models/abilities/sample_turret.vmdl", true, None),
                ("models/props/sample_rock.vmdl", true, None),
            ]
        );
        assert!(catalog.rows[0].kind.is_none(), "a closed row carries no kind");
        assert!(!allowed_in(&catalog).contains("models/abilities/sample_dome.vmdl"));
        assert_eq!(allowed_in(&catalog).len(), 2);
        // The rock is of the first kind (drawn only by an effect): no kind, and only the answer marks it.
        assert!(catalog.rows[2].kind.is_none());
        assert_eq!(catalog.scenery, ["models/props/sample_rock.vmdl"]);
        assert!(serde_json::to_value(&catalog.rows[2]).unwrap().get("scenery").is_none());
        // Every switch off: the dome is closed by the first profile as before (placed), not by the rule.
        let first = hero_catalog_with(&game, &defs, &Names::default(), &users, &haze, &roster, &Switches::OFF, &cancel).unwrap();
        assert_eq!(first.rows[0].reason.as_deref(), Some(REASON_PLACED));
    }

    #[test]
    fn scenery_is_a_model_outside_the_games_gameplay_folders() {
        for path in ["models/park/park_rocks/park_rock_01.vmdl", "models/props/ibeam_a_148_wood.vmdl", "models/props_cemetery/cemetery_tombstone_b.vmdl", "Models/Props_Industrial/Gear_01.vmdl", "models/heroes.vmdl"] {
            assert!(is_scenery(path), "{path}");
        }
        for path in [
            "models/abilities/ice_dome.vmdl",
            "models/heroes_wip/ratking/ratking_grenade.vmdl",
            "models/heroes_staging/haze/dagger.vmdl",
            "models/heroes/sample/sample.vmdl",
            "models/particle/blend/priest_snaptrap.vmdl",
            "models/weapons/hornet/hornet_drone.vmdl",
            "models/w_models/sample.vmdl",
            "models/upgrades/sample.vmdl",
            "models/ui/sample.vmdl",
            "models/props_gameplay/sample.vmdl",
        ] {
            assert!(!is_scenery(path), "{path}");
        }
        // On the answer, beside the rows (absent when there is none), never inside a row.
        let mut catalog = ObjectCatalog { hero_path: "h".into(), profile: PROFILE.into(), rows: vec![], counts: Counts::default(), scenery: vec![] };
        assert!(serde_json::to_value(&catalog).unwrap().get("scenery").is_none());
        catalog.scenery = vec!["models/props/ibeam_a_148_wood.vmdl".into()];
        assert_eq!(serde_json::to_value(&catalog).unwrap()["scenery"], serde_json::json!(["models/props/ibeam_a_148_wood.vmdl"]));
        let old: ObjectCatalog = serde_json::from_str(r#"{"hero_path":"h","profile":"p","rows":[],"counts":{"listed":0,"offered":0,"shared_shapes":0}}"#).unwrap();
        assert!(old.scenery.is_empty(), "a saved catalog without the key still reads");
    }

    #[test]
    fn receipts_compare_rows_by_target_source_and_fit() {
        use deadlock_project::ability_objects::{AbilityObjectRow, FitSize};
        let row = AbilityObjectRow { target: "models/abilities/sample_dagger.vmdl".into(), enabled: true, source: "C:/files/shape.fbx".into(), fit: ObjectFit { scale: 1.25, ..Default::default() }, follow: None };
        let entry = ObjectEntry {
            target: "models/abilities/sample_dagger.vmdl_c".into(),
            original_sha256: "a".into(),
            source: row.source.clone(),
            source_sha256: "b".into(),
            packet_sha256: "c".into(),
            fit: ObjectFit { scale: 1.25 + 1e-12, ..Default::default() },
            files: Hashes::from([("models/abilities/sample_dagger.vmdl_c".into(), "d".into()), (format!("{OBJECT_FOLDER}/sample_dagger_0000/m.vmat_c"), "e".into())]),
            materials: vec![],
            bounds: None,
            carried: None,
            follow: None,
        };
        assert!(serde_json::to_value(&entry).unwrap().get("carried").is_none(), "the first kind's entries keep their keys");
        let receipt = ObjectReceipt { entries: vec![entry], ..ObjectReceipt::empty("models/heroes_staging/haze/haze.vmdl") };
        assert!(receipt.same_rows(std::slice::from_ref(&row)));
        // A project saved again may spell the same file with other separators.
        assert!(receipt.same_rows(&[AbilityObjectRow { source: "c:\\files\\shape.fbx".into(), ..row.clone() }]));
        assert!(!receipt.same_rows(&[AbilityObjectRow { source: "C:/files/other.fbx".into(), ..row.clone() }]));
        assert_eq!(receipt.own_files(), [format!("{OBJECT_FOLDER}/sample_dagger_0000/m.vmat_c")]);
        assert!(!receipt.same_rows(&[AbilityObjectRow { fit: ObjectFit { size: FitSize::Keep, ..row.fit }, ..row.clone() }]));
        assert!(!receipt.same_rows(&[AbilityObjectRow { fit: ObjectFit { on_base: true, ..row.fit }, ..row.clone() }]), "standing on the base is part of the fit");
        assert!(!receipt.same_rows(&[]));
        let follow = deadlock_project::ability_objects::Follow { way: deadlock_project::ability_objects::FollowWay::OnePiece, bone: None, parts: BTreeMap::new() };
        assert!(!receipt.same_rows(&[AbilityObjectRow { follow: Some(follow), ..row.clone() }]), "how it follows is part of the row");
        assert!(ObjectReceipt::empty("x").is_empty());
    }

    #[test]
    fn install_rechecks_the_same_skeleton_way_against_its_switch() {
        use deadlock_project::ability_objects::{Follow, FollowWay};
        let entry = |follow: Option<Follow>| ObjectEntry {
            target: "models/abilities/sample_canister.vmdl_c".into(),
            original_sha256: "a".into(),
            source: "C:/files/shape.blend".into(),
            source_sha256: "b".into(),
            packet_sha256: "c".into(),
            fit: ObjectFit::default(),
            files: Hashes::new(),
            materials: vec![],
            bounds: None,
            carried: None,
            follow,
        };
        let way = |way| Some(Follow { way, bone: None, parts: BTreeMap::new() });
        let receipt = |follow| ObjectReceipt { entries: vec![entry(follow)], ..ObjectReceipt::empty("models/heroes_wip/sample/sample.vmdl") };
        let closed = verify_ways(&receipt(way(FollowWay::SameSkeleton)), &Switches::OFF).unwrap_err().to_string();
        assert!(closed.contains(&format!("{}: models/abilities/sample_canister.vmdl_c", crate::object_follow::SAME_SKELETON_CLOSED)), "{closed}");
        assert!(verify_ways(&receipt(way(FollowWay::SameSkeleton)), &Switches { same_skeleton: true, ..Switches::OFF }).is_ok());
        // The app's own switches open the way.
        assert!(verify_ways(&receipt(way(FollowWay::SameSkeleton)), &switches()).is_ok());
        for other in [None, way(FollowWay::OnePiece), way(FollowWay::ByParts)] {
            assert!(verify_ways(&receipt(other), &Switches::OFF).is_ok());
        }
        // A receipt whose binding says so is refused the same way.
        let check = CarriedCheck::default();
        let binding = crate::object_follow::Binding { way: FollowWay::SameSkeleton, parts: BTreeMap::new(), suggested: BTreeMap::new() };
        let carried = CarriedEntry { features: vec![], shells: None, shell_material: None, staged: 0, attachments_taken: 0, check, binding: Some(binding), state_bindings: vec![], kept_blocks: vec![], kept_parts: vec![], written: vec![] };
        let bound = ObjectReceipt { entries: vec![ObjectEntry { carried: Some(carried), ..entry(None) }], ..receipt(None) };
        assert!(verify_ways(&bound, &Switches::OFF).is_err());
    }

    #[test]
    fn state_bindings_are_recorded_only_for_a_states_object() {
        let binding = crate::object_follow::Binding { way: deadlock_project::ability_objects::FollowWay::ByParts, parts: BTreeMap::from([("lower".to_owned(), "body".to_owned())]), suggested: BTreeMap::new() };
        let entry = |state_bindings| CarriedEntry { features: vec![], shells: None, shell_material: None, staged: 1, attachments_taken: 0, check: CarriedCheck::default(), binding: Some(binding.clone()), state_bindings, kept_blocks: vec![], kept_parts: vec![], written: vec![] };
        // Every other kind: the receipt's bytes as before the field existed.
        let plain = serde_json::to_string(&entry(vec![])).unwrap();
        assert!(!plain.contains("state_bindings"), "{plain}");
        let old: CarriedEntry = serde_json::from_str(&plain).unwrap();
        assert_eq!(old, entry(vec![]));
        // A states object: each state, the unskinned one without bones or binding.
        let states = vec![
            crate::object_states::StateBinding { mesh: "whole".into(), choice: "whole".into(), bones: vec!["root".into(), "body".into()], binding: Some(binding.clone()) },
            crate::object_states::StateBinding { mesh: "broken".into(), choice: "broken".into(), bones: vec![], binding: None },
        ];
        let json = serde_json::to_value(entry(states.clone())).unwrap();
        assert_eq!(json["state_bindings"][1], serde_json::json!({ "mesh": "broken", "choice": "broken" }));
        assert_eq!(serde_json::from_value::<CarriedEntry>(json).unwrap().state_bindings, states);
    }

    #[test]
    fn only_decode_skeleton_positions_take_the_rounding_tolerance() {
        let rows = |pairs: &[(&str, &str)]| -> BTreeMap<String, String> { pairs.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect() };
        let pos = ".m_decodeKey.m_boneArray[10].m_pos[0]";
        let game = rows(&[(pos, "19.73383331298828"), (".m_decodeKey.m_boneArray[10].m_quat[0]", "0.5")]);
        // 6.5e-4 in apart: accepted and noted.
        let mut check = CarriedCheck::default();
        compare_rows("AGRP[0]", "AGRP", &game, &rows(&[(pos, "19.734481811523438"), (".m_decodeKey.m_boneArray[10].m_quat[0]", "0.5")]), &mut check);
        assert!(check.differs.is_empty(), "{:?}", check.differs);
        assert!(check.noted.iter().any(|n| n.contains("compiler rounding")));
        // Beyond the tolerance, a rotation, or the same row in another block: exact.
        for (block, ours) in [
            ("AGRP", rows(&[(pos, "19.7358"), (".m_decodeKey.m_boneArray[10].m_quat[0]", "0.5")])),
            ("AGRP", rows(&[(pos, "19.73383331298828"), (".m_decodeKey.m_boneArray[10].m_quat[0]", "0.5005")])),
            ("DATA", rows(&[(pos, "19.734481811523438"), (".m_decodeKey.m_boneArray[10].m_quat[0]", "0.5")])),
        ] {
            let mut check = CarriedCheck::default();
            compare_rows("X", block, &game, &ours, &mut check);
            assert_eq!(check.differs.len(), 1, "{block} {:?}", check.differs);
        }
        assert!(!decode_position(".m_decodeKey.m_boneArray[1].m_pos[3]"));
        assert!(!decode_position(".m_decodeKey.m_boneArray[1].m_posX[0]"));
        assert!(!decode_position(".m_other.m_decodeKey.m_boneArray[1].m_pos[0]"));
    }

    #[test]
    fn cloth_closes_an_object_on_the_switched_path() {
        let turret = ModelFacts {
            bones: vec!["root".into(), "arm".into()],
            mesh_skeleton_bones: Some(2),
            anim_graph: Some("a.vnmgraph".into()),
            animation_blocks: vec!["AGRP".into()],
            physics_block: true,
            physics_shapes: BTreeMap::from([("capsule".into(), 2)]),
            materials: vec!["a.vmat".into()],
            cloth: true,
            ..ModelFacts::default()
        };
        assert_eq!(offered_with(&turret, false, &Switches::ALL), Err(REASON_CLOTH));
        assert_eq!(offered_with(&ModelFacts { cloth: false, ..turret.clone() }, false, &Switches::ALL), Ok(()));
        assert!(serde_json::to_value(ModelFacts::default()).unwrap().get("cloth").is_none(), "absent unless true");
    }

    /// An object whose physics block holds only cloth (no shape), as Rem's helper.
    fn cloth_only() -> ModelFacts {
        ModelFacts {
            bones: vec!["root".into(), "tassel_0".into(), "tassel_end".into()],
            mesh_skeleton_bones: Some(3),
            blocks: vec!["MDAT".into(), "ANIM".into(), "ASEQ".into(), "AGRP".into(), "PHYS".into(), "DATA".into()],
            anim_graph: Some("a.vnmgraph".into()),
            animation_blocks: vec!["ANIM".into(), "ASEQ".into(), "AGRP".into()],
            physics_block: true,
            cloth: true,
            ..plain()
        }
    }

    #[test]
    fn kept_cloth_opens_cloth_only_behind_its_own_switch() {
        let helper = cloth_only();
        // Closed without its own switch, word for word; open under the app's switches.
        let without = Switches { kept_cloth: false, ..SWITCHES };
        assert_eq!(offered_with(&helper, false, &without), Err(REASON_CLOTH));
        assert_eq!(offered_with(&helper, false, &Switches { kept_cloth: false, ..Switches::ALL_KEPT }), Err(REASON_CLOTH));
        assert_eq!(offered(&helper, false), Ok(()));
        assert_eq!(offered_with(&helper, false, &Switches::EVERY), Ok(()));
        // Only cloth inside the model is kept: a physics file the model only names never is.
        let file = ModelFacts { physics_refs: 1, ..helper.clone() };
        assert_eq!(offered_with(&file, false, &Switches::EVERY), Err(REASON_OTHER_COLLISION));
        let two = ModelFacts { blocks: [helper.blocks.clone(), vec!["PHYS".into()]].concat(), ..helper.clone() };
        assert_eq!(offered_with(&two, false, &Switches::EVERY), Err(REASON_OTHER_COLLISION));
        // A cloth whose bones the compiler names otherwise stays closed, with its own reason.
        // The fact: a bone name with a byte the compiler writes as `_` (the cloth proxy's own bones).
        assert!(renamed_by_compiler("$cloth_m0p1") && renamed_by_compiler("a.b") && renamed_by_compiler("a-b"));
        assert!(!renamed_by_compiler("tassel_0") && !renamed_by_compiler("Breast_L_0"));
        assert!(serde_json::to_value(&helper).unwrap().get("cloth_bones_renamed").is_none(), "absent unless true");
        let renamed = ModelFacts { cloth_bones_renamed: true, ..helper.clone() };
        assert_eq!(offered_with(&renamed, false, &Switches { kept_cloth_proxy: false, ..Switches::EVERY }), Err(REASON_CLOTH_BONE_NAMES));
        assert_eq!(offered_with(&renamed, false, &Switches { kept_cloth_proxy: false, ..SWITCHES }), Err(REASON_CLOTH_BONE_NAMES));
        // Opened by the cloth-proxy kind (its own switch), which the app has on.
        assert_eq!(offered(&renamed, false), Ok(()));
        assert_eq!(offered_with(&renamed, false, &without), Err(REASON_CLOTH));
        // Without cloth the switch changes nothing.
        let none = ModelFacts { cloth: false, physics_shapes: BTreeMap::from([("hull".into(), 1)]), ..helper.clone() };
        assert_eq!(offered_with(&none, false, &Switches::EVERY), offered(&none, false));
        assert_eq!(kept_kinds(&none), kept_kinds(&ModelFacts { cloth: false, ..none.clone() }));
    }

    #[test]
    fn the_physics_block_is_kept_for_its_cloth() {
        use crate::keep_blocks::Allowed;
        // Cloth only: kept for the cloth, and it is no collision.
        let helper = cloth_only();
        assert_eq!(kept_kinds(&helper), BTreeSet::from([KeptKind::Cloth]));
        assert_eq!(kept_block_names(&kept_kinds(&helper)), ["PHYS"]);
        assert_eq!(kept_allowed(&kept_kinds(&helper)), Allowed { decode_by_name: false, physics_bones: false, cloth: true, physics_reordered: false, cloth_proxy: false });
        // Cloth beside spheres and capsules (which a rebuild would make again): the whole block is kept.
        let capsules = ModelFacts { physics_shapes: BTreeMap::from([("capsule".into(), 2)]), ..helper.clone() };
        assert!(capsules.simple_physics());
        assert_eq!(kept_block_names(&kept_kinds(&capsules)), ["PHYS"]);
        // Cloth beside a hull: collision and cloth; on bones the precondition of collision on bones too.
        let hull = ModelFacts { physics_shapes: BTreeMap::from([("hull".into(), 1)]), physics_on_bones: true, ..helper.clone() };
        assert_eq!(kept_kinds(&hull), BTreeSet::from([KeptKind::Collision, KeptKind::CollisionOnBones, KeptKind::Cloth]));
        assert_eq!(kept_allowed(&kept_kinds(&hull)), Allowed { decode_by_name: false, physics_bones: true, cloth: true, physics_reordered: false, cloth_proxy: false });
        // Every kept kind is open under `EVERY` and the app's switches, and `Cloth` only with its own switch.
        assert!(kept_kinds(&hull).iter().all(|k| k.open(&Switches::EVERY) && k.open(&SWITCHES)));
        assert!(!KeptKind::Cloth.open(&Switches { kept_cloth: false, ..SWITCHES }));
        // The page's lines: the cloth, and no collision line for a block that holds only cloth.
        let kind = object_kind(&helper, false).unwrap();
        assert!(kind.kept.contains(&KEPT_GAME_CLOTH.to_owned()));
        assert!(!kind.collision && !kind.kept.iter().any(|k| k == KEPT_GAME_COLLISION || k == KEPT_COLLISION));
        assert!(!features(&helper, false).iter().any(|f| matches!(f, Feature::SimpleCollision | Feature::OtherCollision)));
        assert!(object_kind(&hull, false).unwrap().kept.contains(&KEPT_GAME_COLLISION.to_owned()));
    }

    /// Facts of the objects the kept kinds concern: a legacy-animated object with
    /// several bones (its three animation blocks inside the model), and hull collision.
    fn legacy() -> ModelFacts {
        ModelFacts { bones: vec!["base".into(), "jaw".into()], mesh_skeleton_bones: Some(2), blocks: vec!["MDAT".into(), "ANIM".into(), "ASEQ".into(), "AGRP".into(), "DATA".into()], animation_blocks: vec!["ANIM".into(), "ASEQ".into(), "AGRP".into()], ..plain() }
    }
    fn hull() -> ModelFacts {
        ModelFacts { blocks: vec!["MDAT".into(), "PHYS".into(), "DATA".into()], physics_block: true, physics_shapes: BTreeMap::from([("hull of 8 points".into(), 1)]), ..plain() }
    }

    #[test]
    fn a_physics_block_in_the_unstated_frame_form_stays_closed_on_bones_unless_its_switch_is_open() {
        let open = SWITCHES;
        let closed = Switches { unstated_frame_collision: false, ..SWITCHES };
        // On an object with a bone: closed as unreadable collision, word for word, with the switch off (on in the app).
        let boned = ModelFacts { bones: vec!["root".into()], mesh_skeleton_bones: Some(1), physics_unstated_frames: true, ..hull() };
        assert_eq!(offered_with(&boned, false, &closed), Err(REASON_UNREADABLE_COLLISION));
        assert_eq!(offered(&boned, false), Ok(()));
        assert_eq!(offered_with(&boned, false, &open), Ok(()));
        // Without bones the block was kept unread before and stays offered (its switch changes nothing).
        let boneless = ModelFacts { physics_unstated_frames: true, ..hull() };
        assert_eq!(offered(&boneless, false), Ok(()));
        assert_eq!(object_kind(&boneless, false), object_kind(&hull(), false));
        // A block the reader still refuses stays closed on bones whatever the switch says.
        assert_eq!(offered_with(&ModelFacts { physics_unreadable: true, ..boned.clone() }, false, &open), Err(REASON_UNREADABLE_COLLISION));
    }

    #[test]
    fn animation_on_one_bone_or_on_none_opens_only_by_its_own_switch_and_never_with_morph_targets() {
        let anim = |f: ModelFacts| ModelFacts { blocks: vec!["MDAT".into(), "ANIM".into(), "ASEQ".into(), "AGRP".into(), "DATA".into()], animation_blocks: vec!["ANIM".into(), "ASEQ".into(), "AGRP".into()], ..f };
        let one = anim(ModelFacts { bones: vec!["root".into()], mesh_skeleton_bones: Some(1), ..plain() });
        let none = anim(ModelFacts { animated_material_values: vec!["JitterAmountAnim".into()], ..plain() });
        let app = Switches { one_bone_animated: false, boneless_animated: false, ..SWITCHES };
        let b = Switches { one_bone_animated: true, ..app };
        let c = Switches { boneless_animated: true, ..app };
        // Both are on in the app; with both off, closed word for word; each opens only its own.
        for f in [&one, &none] {
            assert_eq!(offered(f, false), Ok(()));
            assert_eq!(offered_with(f, false, &app), Err(REASON_ANIMATED));
        }
        assert_eq!(offered_with(&one, false, &b), Ok(()));
        assert_eq!(offered_with(&one, false, &c), Err(REASON_ANIMATED));
        assert_eq!(offered_with(&none, false, &c), Ok(()));
        assert_eq!(offered_with(&none, false, &b), Err(REASON_ANIMATED));
        // A morph block keeps both closed, with the reason that the object's own shape changes.
        for (f, s) in [(&one, b), (&none, c)] {
            assert_eq!(offered_with(&ModelFacts { morphs: true, blocks: [f.blocks.clone(), vec!["MRPH".into()]].concat(), ..f.clone() }, false, &s), Err(REASON_MORPHS));
        }
        // An animation through a graph, or with no material value it drives, is neither kind.
        assert_eq!(offered_with(&ModelFacts { anim_graph: Some("a.vnmgraph".into()), ..one.clone() }, false, &b), Err(REASON_ANIMATED));
        assert_eq!(offered_with(&ModelFacts { animated_material_values: vec![], ..none.clone() }, false, &c), Err(REASON_ANIMATED));
        // Both keep the game's own animation blocks; the kind says so in plain words.
        assert_eq!(kept_kinds(&one), BTreeSet::from([KeptKind::Animation]));
        assert_eq!(kept_kinds(&none), BTreeSet::from([KeptKind::Animation]));
        let kind = object_kind(&one, false).unwrap();
        assert_eq!(kind.label, "An object an effect draws, with one bone and an animation");
        assert_eq!(kind.kept, vec![KEPT_GAME_ANIMATIONS.to_owned(), KEPT_BONE.to_owned()]);
        let kind = object_kind(&none, false).unwrap();
        assert_eq!(kind.label, "An object an effect draws, with an animation");
        assert_eq!(kind.kept, vec![KEPT_MATERIAL_ANIMATION_JITTER.to_owned()]);
        assert_eq!(material_animation_line(&ModelFacts { animated_material_values: vec!["GlowAnim".into()], ..none }), KEPT_MATERIAL_ANIMATION);
    }

    #[test]
    fn each_kept_kind_opens_only_its_own_objects() {
        // The app's switches without any kept kind, then each on its own.
        let app = Switches { kept_animation: false, kept_collision: false, kept_animation_by_name: false, kept_graph_animation: false, kept_collision_on_bones: false, ..SWITCHES };
        let a = Switches { kept_animation: true, ..app };
        let b = Switches { kept_collision: true, ..app };
        assert_eq!(SWITCHES, Switches { kept_animation: true, kept_collision: true, kept_animation_by_name: true, kept_graph_animation: true, kept_collision_on_bones: true, ..app });
        let one = |f: &ModelFacts, s: Switches| offered_with(f, true, &s);
        // With every kept kind off: today's reasons, word for word.
        assert_eq!(one(&legacy(), app), Err(REASON_LEGACY_ANIMATION));
        assert_eq!(one(&hull(), app), Err(REASON_OTHER_COLLISION));
        // A: a legacy animation with its blocks inside the model and the bones in order.
        assert_eq!(one(&legacy(), a), Ok(()));
        assert_eq!(one(&legacy(), b), Err(REASON_LEGACY_ANIMATION), "collision does not open animation");
        assert_eq!(one(&ModelFacts { animation_folders: true, ..legacy() }, a), Ok(()), "the kept sequence block carries its folders");
        let missing = ModelFacts { blocks: vec!["MDAT".into(), "ANIM".into(), "DATA".into()], animation_blocks: vec!["ANIM".into()], ..legacy() };
        assert_eq!(one(&missing, Switches::ALL_KEPT), Err(REASON_LEGACY_ANIMATION), "only animation held inside the model can be kept");
        assert_eq!(one(&ModelFacts { blocks: vec![], animation_blocks: vec![], sequence_groups: 1, ..legacy() }, Switches::ALL_KEPT), Err(REASON_LEGACY_ANIMATION));
        // C on top of A: another bone order.
        let reordered = ModelFacts { decode_reordered: true, ..legacy() };
        assert_eq!(one(&reordered, app), Err(REASON_LEGACY_ANIMATION));
        assert_eq!(one(&reordered, a), Err(REASON_ANIMATION_BY_NAME));
        assert_eq!(one(&reordered, Switches { kept_animation_by_name: true, ..app }), Err(REASON_LEGACY_ANIMATION), "two kinds: both");
        assert_eq!(one(&reordered, Switches { kept_animation_by_name: true, ..a }), Ok(()));
        // Animated on one bone: only by its own switch (on in the app), never by a kept kind alone.
        let one_bone = ModelFacts { bones: vec!["base".into()], mesh_skeleton_bones: Some(1), ..legacy() };
        assert_eq!(one(&one_bone, Switches { one_bone_animated: false, ..Switches::ALL_KEPT }), Err(REASON_ANIMATED));
        assert_eq!(one(&one_bone, Switches::ALL_KEPT), Ok(()));
        // D: a graph-played object the rebuild cannot reproduce; reordered also needs C.
        let graph = ModelFacts { anim_graph: Some("a.vnmgraph".into()), ..legacy() };
        assert_eq!(one(&graph, app), Ok(()), "a graph object a rebuild reproduces keeps today's build");
        assert!(kept_kinds(&graph).is_empty());
        let folders = ModelFacts { animation_folders: true, ..graph.clone() };
        assert_eq!(one(&folders, Switches { kept_graph_animation: false, ..Switches::ALL_KEPT }), Err(REASON_ANIMATION_FOLDERS));
        assert_eq!(one(&folders, Switches { kept_graph_animation: true, ..app }), Ok(()));
        let order = ModelFacts { decode_reordered: true, ..graph.clone() };
        assert_eq!(one(&order, Switches { kept_animation: true, kept_animation_by_name: true, ..app }), Err(REASON_BONE_ORDER), "legacy kinds do not open graph objects");
        assert_eq!(one(&order, Switches { kept_graph_animation: true, ..app }), Err(REASON_ANIMATION_BY_NAME));
        assert_eq!(one(&order, Switches { kept_graph_animation: true, kept_animation_by_name: true, ..app }), Ok(()));
        // B: hull or mesh collision inside the model, bound to no bone.
        assert_eq!(one(&hull(), b), Ok(()));
        assert_eq!(one(&hull(), a), Err(REASON_OTHER_COLLISION), "animation does not open collision");
        assert_eq!(one(&ModelFacts { physics_shapes: BTreeMap::from([("mesh".into(), 1), ("capsule".into(), 1)]), ..hull() }, b), Ok(()));
        let file = ModelFacts { physics_refs: 1, physics_shapes: BTreeMap::from([("file".into(), 1)]), ..hull() };
        assert_eq!(one(&file, Switches::ALL_KEPT), Err(REASON_OTHER_COLLISION), "a physics file is never kept");
        let twice = ModelFacts { blocks: vec!["PHYS".into(), "PHYS".into(), "DATA".into()], ..hull() };
        assert_eq!(one(&twice, Switches::ALL_KEPT), Err(REASON_OTHER_COLLISION));
        assert_eq!(one(&ModelFacts { cloth: true, ..hull() }, Switches { kept_cloth: false, ..Switches::ALL_KEPT }), Err(REASON_CLOTH), "cloth stays closed without its own switch");
        assert_eq!(one(&ModelFacts { break_pieces: true, ..hull() }, Switches { written_break_pieces: false, ..Switches::ALL_KEPT }), Err(REASON_BREAK_PIECES), "break pieces stay closed by their own switch");
        // E on top of B: physics on bones.
        let on_bones = ModelFacts { physics_on_bones: true, bones: vec!["base".into(), "jaw".into()], mesh_skeleton_bones: Some(2), ..hull() };
        assert_eq!(one(&on_bones, app), Err(REASON_OTHER_COLLISION));
        assert_eq!(one(&on_bones, b), Err(REASON_COLLISION_ON_BONES));
        assert_eq!(one(&on_bones, Switches { kept_collision_on_bones: true, ..app }), Err(REASON_OTHER_COLLISION), "two kinds: both");
        assert_eq!(one(&on_bones, Switches { kept_collision_on_bones: true, ..b }), Ok(()));
        // Capsules stay compiled as today, even on bones.
        let capsules = ModelFacts { physics_shapes: BTreeMap::from([("capsule".into(), 1)]), physics_on_bones: true, ..hull() };
        assert_eq!(one(&capsules, app), Ok(()));
        assert!(kept_kinds(&capsules).is_empty());
        // Both: an animated object with hull collision needs A and B.
        let both = ModelFacts { blocks: vec!["MDAT".into(), "ANIM".into(), "ASEQ".into(), "AGRP".into(), "PHYS".into(), "DATA".into()], physics_block: true, physics_shapes: BTreeMap::from([("hull".into(), 1)]), ..legacy() };
        assert_eq!(one(&both, a), Err(REASON_OTHER_COLLISION));
        assert_eq!(one(&both, b), Err(REASON_LEGACY_ANIMATION));
        assert_eq!(one(&both, Switches { kept_animation: true, ..b }), Ok(()));
        assert_eq!(kept_block_names(&kept_kinds(&both)), ["ANIM", "ASEQ", "AGRP", "PHYS"]);
        // The rule the build uses: an object offered under some switches has every kept kind it needs open.
        for (facts, placed) in grid().into_iter().chain([legacy(), hull(), reordered.clone(), folders.clone(), order.clone(), on_bones.clone(), both.clone()].into_iter().map(|f| (f, true))) {
            for s in [app, a, b, Switches::ALL_KEPT, Switches { kept_animation: true, kept_collision: true, ..app }] {
                if offered_with(&facts, placed, &s).is_ok() {
                    assert!(kept_kinds(&facts).iter().all(|k| k.open(&s)), "{facts:?} under {s:?}");
                }
            }
        }
    }

    #[test]
    fn a_kept_kind_shows_what_stays_the_game_s_own() {
        let kind = object_kind(&hull(), true).unwrap();
        assert!(kind.collision && kind.stands_on);
        assert_eq!(kind.label, "An object the game places, with collision");
        assert_eq!(kind.kept, [KEPT_GAME_COLLISION, KEPT_PLACED]);
        assert_eq!(kind.game_blocks, [GAME_BLOCKS_COLLISION]);
        let kind = object_kind(&legacy(), false).unwrap();
        assert_eq!(kind.kept, [KEPT_GAME_ANIMATIONS, KEPT_BONES]);
        assert_eq!(kind.game_blocks, [GAME_BLOCKS_ANIMATION]);
        // Every other kind keeps its bytes: no `game_blocks` key.
        let keg = serde_json::to_value(object_kind(&keg(), true).unwrap()).unwrap();
        assert!(keg.get("game_blocks").is_none());
        assert_eq!(keg["kept"][0], KEPT_COLLISION);
        assert!(serde_json::to_value(ModelFacts::default()).unwrap().get("physics_on_bones").is_none(), "absent unless true");
    }

    #[test]
    fn the_rest_sign_and_game_data_order_close_an_object_on_the_switched_path() {
        // The sign the matrix gives: w with a trace of 0 or more, else the largest diagonal's component.
        assert!(!rest_sign_flips([0., 0., 0., 1.]));
        assert!(rest_sign_flips([0., 0., 0., -1.]));
        assert!(!rest_sign_flips([0.484, 0.875, 0., -0.0001]), "trace -1: y decides, and it is positive");
        assert!(rest_sign_flips([-0.484, -0.875, 0., 0.0001]));
        // A trace at 0 (a third of a turn about the diagonal): the compiler takes w.
        assert!(rest_sign_flips([0.5, 0.5, 0.5, -0.5]));
        assert!(!rest_sign_flips([-0.5, -0.5, -0.5, 0.5]));
        assert!(!rest_sign_flips([0.5, 0.5, 0.5, 0.5]));
        let two = ModelFacts { bones: vec!["a".into(), "b".into()], ..plain() };
        assert_eq!(offered_with(&ModelFacts { rest_sign_flips: true, ..two.clone() }, false, &Switches { written_rest_signs: false, ..SWITCHES }), Err(REASON_REST_SIGN));
        assert_eq!(offered_with(&ModelFacts { key_values_reordered: true, ..two.clone() }, false, &Switches { written_key_order: false, ..SWITCHES }), Err(REASON_GAME_DATA_ORDER));
        assert_eq!(offered(&ModelFacts { rest_sign_flips: true, ..plain() }, false), Ok(()), "no bones: no rest rotation is compared");
        // The first profile is unchanged by them.
        assert_eq!(offered_with(&ModelFacts { key_values_reordered: true, ..plain() }, false, &Switches::OFF), Ok(()));
        // A physics block the reader refuses is kept only without bones.
        let unread = ModelFacts { physics_unreadable: true, ..hull() };
        let b = Switches { kept_collision: true, ..SWITCHES };
        assert_eq!(offered_with(&unread, true, &b), Ok(()));
        assert_eq!(offered_with(&ModelFacts { bones: vec!["a".into()], mesh_skeleton_bones: Some(1), ..unread.clone() }, true, &b), Err(REASON_UNREADABLE_COLLISION));
        assert_eq!(offered_with(&unread, true, &Switches { kept_collision: false, ..SWITCHES }), Err(REASON_OTHER_COLLISION));
        assert_eq!(offered_with(&ModelFacts { rest_scale: true, ..two.clone() }, false, &Switches { written_rest_scales: false, ..SWITCHES }), Err(REASON_REST_SCALE));
        assert_eq!(offered(&ModelFacts { morphs: true, ..two.clone() }, false), Err(REASON_MORPHS));
        assert_eq!(offered_with(&ModelFacts { morphs: true, ..plain() }, false, &Switches::OFF), Ok(()), "the first profile is unchanged");
        for key in ["physics_unreadable", "rest_sign_flips", "key_values_reordered", "rest_scale", "morphs"] {
            assert!(serde_json::to_value(ModelFacts::default()).unwrap().get(key).is_none(), "{key} absent unless true");
        }
    }

    #[test]
    fn a_hull_is_drawn_as_its_bounds_and_measured_as_a_box() {
        let shape = CollisionShape::Box { min: [-1., -2., 0.], max: [1., 2., 4.] };
        assert_eq!(shape.distance([0., 0., 2.]), -1.);
        assert_eq!(shape.distance([3., 0., 2.]), 2.);
        assert!((shape.distance([2., 3., 2.]) - 2f64.sqrt()).abs() < 1e-12);
        assert_eq!(sticks_out([[0., 0., 1.], [0., 0., 5.]], &[shape.clone()]), Some(1.));
        assert_eq!(serde_json::to_value(&shape).unwrap()["shape"], "box");
    }

    #[test]
    fn the_compiler_s_order_is_depth_first_with_siblings_in_the_game_s_order() {
        // root(0) → a(1) → a_end(3); root → b(2) → b_end(4): written root, a, a_end, b, b_end.
        assert_eq!(depth_first(&[-1, 0, 0, 1, 2]), Some(vec![0, 1, 3, 2, 4]));
        assert_eq!(depth_first(&[-1, 0, 1]), Some(vec![0, 1, 2]));
        assert_eq!(depth_first(&[-1, 5]), None, "a parent out of range");
        assert_eq!(depth_first(&[1, 0]), None, "a loop");
    }

    #[test]
    fn kept_physics_on_bones_of_a_reordered_skeleton_needs_its_own_switch() {
        let hull = ModelFacts {
            bones: vec!["root".into(), "body".into()],
            mesh_skeleton_bones: Some(2),
            mesh_bones: vec!["root".into(), "body".into()],
            blocks: ["MDAT", "PHYS", "CTRL", "DATA"].map(String::from).to_vec(),
            physics_block: true,
            physics_on_bones: true,
            physics_shapes: [("hull of 8 points".to_owned(), 1)].into(),
            materials: vec!["materials/sample/a.vmat".into()],
            ..ModelFacts::default()
        };
        assert!(!kept_kinds(&hull).contains(&KeptKind::CollisionReordered));
        assert_eq!(offered_with(&hull, true, &SWITCHES), Ok(()));
        let reordered = ModelFacts { skeleton_reordered: true, ..hull.clone() };
        assert!(kept_kinds(&reordered).contains(&KeptKind::CollisionReordered));
        assert!(kept_allowed(&kept_kinds(&reordered)).physics_reordered && !kept_allowed(&kept_kinds(&hull)).physics_reordered);
        assert_eq!(offered_with(&reordered, true, &Switches { kept_collision_reordered: false, ..SWITCHES }), Err(REASON_COLLISION_REORDERED));
        assert_eq!(offered_with(&reordered, true, &SWITCHES), Ok(()), "the app's switches");
        assert_eq!(offered_with(&reordered, true, &Switches { kept_collision_reordered: true, ..SWITCHES }), Ok(()));
        // Simple collision is compiled again, never kept: the order does not matter.
        let capsule = ModelFacts { physics_shapes: [("capsule".to_owned(), 1)].into(), ..reordered };
        assert!(!kept_kinds(&capsule).contains(&KeptKind::CollisionReordered));
    }

    #[test]
    fn cloth_from_a_proxy_mesh_is_opened_only_by_its_own_switch() {
        let turret = ModelFacts {
            bones: vec!["root".into(), "body".into(), "$cloth_m0p0".into()],
            mesh_skeleton_bones: Some(2),
            mesh_bones: vec!["root".into(), "body".into()],
            blocks: ["MDAT", "PHYS", "CTRL", "DATA"].map(String::from).to_vec(),
            physics_block: true,
            cloth: true,
            cloth_bones_renamed: true,
            materials: vec!["materials/sample/a.vmat".into()],
            ..ModelFacts::default()
        };
        assert!(kept_kinds(&turret).contains(&KeptKind::ClothProxy));
        assert!(kept_allowed(&kept_kinds(&turret)).cloth_proxy);
        let open = Switches { kept_cloth_proxy: false, ..Switches::EVERY };
        assert_eq!(offered_with(&turret, true, &open), Err(REASON_CLOTH_BONE_NAMES));
        assert_eq!(offered_with(&turret, true, &Switches { kept_cloth_proxy: true, ..open }), Ok(()));
        assert_eq!(offered_with(&turret, true, &Switches { kept_cloth_proxy: false, ..SWITCHES }), Err(REASON_CLOTH_BONE_NAMES));
        assert_eq!(offered_with(&turret, true, &SWITCHES), Ok(()), "the app's switches");
        // Cloth on plainly named bones is not this kind.
        let plain = ModelFacts { cloth_bones_renamed: false, bones: vec!["root".into(), "body".into(), "tail".into()], ..turret };
        assert!(!kept_kinds(&plain).contains(&KeptKind::ClothProxy));
    }
}