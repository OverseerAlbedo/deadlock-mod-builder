//! Parts shown together with the object (`Switches::parts_together`).
//!
//! Some ability objects draw several meshes at once in one body group, each bound to
//! other bones (`ModelFacts::mixed_mesh_bindings`): a door's leaves with its frame and a
//! portal plane. One user model cannot be all of them.
//! As the game's own object is kept wherever the user's model does not stand in, the
//! user's model replaces ONE mesh, the first in mesh order that the default choice shows,
//! and every other mesh stays the game's own (compiled again from
//! the decompile and bound as the game binds it, as `ability_objects::kept_parts` does
//! for parts of another body group). Only where every other mesh of that level is shown
//! by every choice that shows the replaced one (so it is shown together with it, never
//! instead of it: another state of the object is `object_states`' kind), and only without
//! levels of detail (Wraith's effect model, a copy of the hero with its arms as a second
//! part in three levels, stays closed: in the one build tried, with the bones released and
//! the user's model on one bone, its collision's bind poses and joints and one resting turn
//! came out otherwise; the cause is not established). Decided from the masks and the bindings alone, never from a name; `None` for every
//! model `kept_parts` or `object_states::states` already covers.
use crate::ability_objects::{KeptPart, KeptParts, ModelFacts, body_group_choice, kept_parts};

/// The ONE rule (see the module header).
pub fn together(facts: &ModelFacts) -> Option<KeptParts> {
    let default = facts.default_mesh_group_mask?;
    // No levels of detail: the build keeps parts only among the body groups' meshes (a model with
    // levels, one of them a figure copied from a hero, was not opened; see the module header).
    if !facts.mixed_mesh_bindings || facts.mesh_groups.is_empty() || facts.mesh_groups.len() > 64 || default == 0 || !facts.lod_meshes.is_empty() {
        return None;
    }
    if kept_parts(facts).is_some() || crate::object_states::states(facts).is_some() {
        return None;
    }
    let count = facts.meshes.len();
    if count < 2 || facts.mesh_group_masks.len() != count || facts.mesh_names.len() != count || facts.mesh_names.iter().any(String::is_empty) {
        return None;
    }
    let groups: Vec<(String, String)> = facts.mesh_groups.iter().map(|g| body_group_choice(g)).collect();
    if facts.mesh_group_masks.iter().any(|m| *m == 0 || *m >> groups.len() != 0) {
        return None;
    }
    // Without levels of detail every mesh is one level.
    let levels: Vec<Vec<usize>> = vec![(0..count).collect()];
    let mut replaced = vec![];
    let mut kept = vec![];
    for level in &levels {
        let first = *level.iter().find(|i| facts.mesh_group_masks[**i] & default != 0)?;
        let mask = facts.mesh_group_masks[first];
        for mesh in level.iter().copied().filter(|i| *i != first) {
            let other = facts.mesh_group_masks[mesh];
            // Shown by every choice that shows the replaced mesh.
            if other & mask != mask {
                return None;
            }
            let shown: Vec<usize> = (0..groups.len()).filter(|b| other & (1u64 << b) != 0).collect();
            kept.push(KeptPart { mesh, name: facts.mesh_names[mesh].clone(), group: groups[shown[0]].0.clone(), choices: shown.iter().map(|b| groups[*b].1.clone()).collect() });
        }
        replaced.push(first);
    }
    if kept.is_empty() {
        return None;
    }
    Some(KeptParts { replaced, kept, together: true })
}

/// The ONE rule of `Switches::parts_kept_in_levels`: `kept_parts` (a part another body group
/// shows, kept as the game's own) in a model with levels of detail, where every level draws
/// the one replaced mesh and, beside it, only kept parts (so the level's second mesh is never
/// an outline or a glow shell). Decided from the masks, the levels and the body groups alone;
/// `None` for every model without levels of detail (`kept_parts` alone covers those) and for
/// every level with a mesh that is neither replaced nor kept.
pub fn in_levels(facts: &ModelFacts) -> Option<KeptParts> {
    if facts.lod_meshes.is_empty() || !facts.extra_meshes() || facts.shell_material().is_some() || crate::object_glow_shells::glow_shells(facts).is_some() {
        return None;
    }
    let parts = kept_parts(facts)?;
    let kept: std::collections::BTreeSet<usize> = parts.kept.iter().map(|p| p.mesh).collect();
    for level in facts.levels() {
        let replaced = level.iter().filter(|m| parts.replaced.contains(m)).count();
        if replaced != 1 || level.iter().any(|m| !parts.replaced.contains(m) && !kept.contains(m)) {
            return None;
        }
    }
    Some(parts)
}
/// The bones the game's file binds the kept parts to (`ModelFacts::mesh_bindings`, every kept
/// part's list together), in the skeleton's order: the bones a build of `in_levels` marks to be
/// kept (`do_not_discard`), so the compiler binds each kept part to exactly the game's bones
/// (with every bone marked it binds them to every bone, with none only to the weighted ones).
/// Empty unless every kept part has a binding.
pub fn kept_bones(facts: &ModelFacts, parts: &KeptParts) -> Vec<String> {
    let mut wanted = std::collections::BTreeSet::new();
    for part in &parts.kept {
        match facts.mesh_bindings.get(part.mesh) {
            Some(bones) if !bones.is_empty() => wanted.extend(bones.iter().cloned()),
            _ => return vec![],
        }
    }
    facts.bones.iter().filter(|b| wanted.contains(*b)).cloned().collect()
}
/// The attachment names of the key values' `AttachmentCameraData` (`DATA m_modelInfo.m_keyValueText`),
/// in the order the game's compiled file lists them. The compiler writes each attachment's
/// camera preview into that list in the order of the attachments in the source; the game's
/// files list them in an order of their own (in 22 of the 73 compiled models with cameras the
/// list is not in attachment order, 8 of them the reverse), which the decompile does not keep.
pub fn camera_order(keys: &str) -> Vec<String> {
    let Some(start) = keys.find("AttachmentCameraData") else { return vec![] };
    let rest = &keys[start..];
    let end = rest.find(']').unwrap_or(rest.len());
    rest[..end].split("attachment_name = \"").skip(1).filter_map(|s| s.split('"').next()).map(str::to_owned).collect()
}

/// Whether the game's key values list the cameras (`camera_order`) in the reverse of the order of
/// their attachments in the model (`ModelFacts::attachments`, `name@bone`). The compiler lists the
/// cameras of attachments written in the vmdl itself in the attachments' own order, and in the
/// reverse order when the attachments come from a prefab file (as the game's source of such a
/// model did: its `RED2` names an attachments `.vmdl_prefab`); the build of `in_levels` then
/// writes the attachment list into a prefab of its own (`deadlock_pipeline::object_levels`).
pub fn cameras_reversed(keys: &str, attachments: &[String]) -> bool {
    let cameras = camera_order(keys);
    let names: Vec<&str> = attachments.iter().map(|a| a.split('@').next().unwrap_or(a)).filter(|a| cameras.iter().any(|c| c == a)).collect();
    cameras.len() >= 2 && names.len() == cameras.len() && names.iter().rev().zip(&cameras).all(|(a, c)| a == c)
}
/// Source's Euler angles (pitch, yaw, roll in degrees, pitch within ±90°) of a rotation `q`
/// (x, y, z, w), as a decompile writes a bone's `angles`.
pub fn quaternion_angles(q: [f64; 4]) -> [f64; 3] {
    let [x, y, z, w] = q;
    // Columns of the matrix: forward (x axis), left (y axis), up (z axis).
    let forward = [1. - 2. * (y * y + z * z), 2. * (x * y + z * w), 2. * (x * z - y * w)];
    let left = [2. * (x * y - z * w), 1. - 2. * (x * x + z * z), 2. * (y * z + x * w)];
    let up_z = 1. - 2. * (x * x + y * y);
    let xy = forward[0].hypot(forward[1]);
    let deg = 180. / std::f64::consts::PI;
    if xy > 0.001 {
        [(-forward[2]).atan2(xy) * deg, forward[1].atan2(forward[0]) * deg, left[2].atan2(up_z) * deg]
    } else {
        [(-forward[2]).atan2(xy) * deg, (-left[0]).atan2(left[1]) * deg, 0.]
    }
}
/// Source's quaternion (x, y, z, w) of Euler angles (pitch, yaw, roll in degrees).
pub fn angle_quaternion(angles: [f64; 3]) -> [f64; 4] {
    let half = std::f64::consts::PI / 360.;
    let (sp, cp) = (angles[0] * half).sin_cos();
    let (sy, cy) = (angles[1] * half).sin_cos();
    let (sr, cr) = (angles[2] * half).sin_cos();
    [sr * cp * cy - cr * sp * sy, cr * sp * cy + sr * cp * sy, cr * cp * sy - sr * sp * cy, cr * cp * cy + sr * sp * sy]
}
/// Whether a resting turn `q` comes back with the other sign through its Euler angles
/// (`quaternion_angles`, then `angle_quaternion`). For a bone the vmdl does not mark to be kept
/// and no mesh binds, the compiler writes the resting turn as the quaternion of its angles: on
/// the pigeon object with its marks released, and with only the kept part's bones marked, the
/// bones this predicts were exactly the bones that came out with the other sign (eight of the
/// 79 unbound ones, then six of the 52), and with every bone marked none did.
pub fn round_trip_flips(q: [f64; 4]) -> bool {
    let back = angle_quaternion(quaternion_angles(q));
    back.iter().zip(q).map(|(a, b)| a * b).sum::<f64>() < 0.
}

/// The page's line for further pieces of a state kept as the game's own when the game names none.
pub const KEPT_PIECES: &str = "Its other pieces in the same look, kept as the game's own";
/// The name the decompile gives a mesh the game stores without a name (the older form):
/// `unnamed_<mesh index + 1>`, as its vmdl's `RenderMeshFile` and the compiled model then name it.
pub fn decompiled_name(mesh: usize) -> String {
    format!("unnamed_{}", mesh + 1)
}

/// The ONE rule of `Switches::state_parts`: one body group whose choices are states of the
/// object (every mesh shown by exactly one choice, every choice showing at least one mesh,
/// the default choice one of them), some state drawn with several meshes bound to different
/// bones, no levels of detail. In each state the user's model replaces the first mesh in
/// mesh order and every other mesh of that state stays the game's own. Meshes may be stored
/// without names (the older form): the decompile names them by their place
/// (`decompiled_name`). `None` for every model another part rule or `object_states` covers.
pub fn state_parts(facts: &ModelFacts) -> Option<KeptParts> {
    let default = facts.default_mesh_group_mask?;
    if !facts.mixed_mesh_bindings || facts.mesh_groups.is_empty() || facts.mesh_groups.len() > 64 || !facts.lod_meshes.is_empty() || default.count_ones() != 1 {
        return None;
    }
    if kept_parts(facts).is_some() || crate::object_states::states(facts).is_some() || together(facts).is_some() {
        return None;
    }
    let count = facts.meshes.len();
    if count < 2 || facts.mesh_group_masks.len() != count || facts.mesh_names.len() != count {
        return None;
    }
    // Every mesh named, or none (the older form); never some.
    let unnamed = facts.mesh_names.iter().all(String::is_empty);
    if !unnamed && facts.mesh_names.iter().any(String::is_empty) {
        return None;
    }
    let groups: Vec<(String, String)> = facts.mesh_groups.iter().map(|g| body_group_choice(g)).collect();
    if groups.iter().any(|(g, _)| *g != groups[0].0) {
        return None;
    }
    let mut by_choice: Vec<Vec<usize>> = vec![vec![]; groups.len()];
    for (mesh, mask) in facts.mesh_group_masks.iter().enumerate() {
        if mask.count_ones() != 1 || mask >> groups.len() != 0 {
            return None;
        }
        by_choice[mask.trailing_zeros() as usize].push(mesh);
    }
    if by_choice.iter().any(Vec::is_empty) || by_choice.iter().all(|m| m.len() == 1) {
        return None;
    }
    let name = |mesh: usize| if unnamed { decompiled_name(mesh) } else { facts.mesh_names[mesh].clone() };
    let (mut replaced, mut kept) = (vec![], vec![]);
    for (choice, meshes) in by_choice.iter().enumerate() {
        replaced.push(meshes[0]);
        for mesh in &meshes[1..] {
            kept.push(KeptPart { mesh: *mesh, name: name(*mesh), group: groups[choice].0.clone(), choices: vec![groups[choice].1.clone()] });
        }
    }
    replaced.sort_unstable();
    Some(KeptParts { replaced, kept, together: true })
}

/// The page's line for a part kept as the game's own under this rule, from the mesh's own
/// name ("Its doorman frame, kept as the game's own"); a level-of-detail suffix
/// (`_lod1`) is left out, so a part shows once.
pub fn kept_line(part: &KeptPart) -> String {
    // A mesh the game stores without a name: its decompiled name says nothing.
    if part.name == decompiled_name(part.mesh) {
        return KEPT_PIECES.to_owned();
    }
    let name = part.name.as_str();
    let name = match name.rsplit_once("_lod") {
        Some((base, n)) if !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) => base,
        _ => name,
    };
    let words = crate::sound_tags::readable(&crate::sound_tags::words(name));
    format!("Its {}, kept as the game's own", deadlock_core::text::casefold(&words))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ability_objects::MeshFacts;

    fn object(groups: &[&str], masks: &[u64], lod: &[u64], bindings: &[&[&str]]) -> ModelFacts {
        ModelFacts {
            mesh_groups: groups.iter().map(|g| (*g).to_owned()).collect(),
            mesh_group_masks: masks.to_vec(),
            default_mesh_group_mask: Some(1),
            mesh_names: (0..masks.len()).map(|i| format!("mesh_{i}")).collect(),
            mesh_bindings: bindings.iter().map(|b| b.iter().map(|n| (*n).to_owned()).collect()).collect(),
            mixed_mesh_bindings: true,
            lod_meshes: if lod.iter().all(|m| *m == 255) { vec![] } else { (0..3).map(|l| lod.iter().filter(|m| **m & (1 << l) != 0).count()).collect() },
            lod_switch_distances: if lod.iter().all(|m| *m == 255) { vec![] } else { vec![0., 14., 35.] },
            meshes: lod.iter().map(|m| MeshFacts { lod_mask: *m, ..Default::default() }).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn a_door_keeps_its_frame_and_portal_and_replaces_its_leaves() {
        // Leaves only while the door is on, frame and portal in both choices.
        let door = object(&["default_@0_#&door_on", "default_@1_#&door_off"], &[1, 3, 3], &[255, 255, 255], &[&["hinge_r", "hinge_l"], &["root"], &["root"]]);
        let parts = together(&door).expect("parts shown together");
        assert_eq!(parts.replaced, vec![0]);
        assert_eq!(parts.kept.iter().map(|p| p.mesh).collect::<Vec<_>>(), vec![1, 2]);
        assert!(parts.together);
    }

    #[test]
    fn levels_of_detail_are_not_opened() {
        // Body and arms in three levels, every mesh shown by every choice.
        let figure = object(&["b_@0_#&a", "b_@1_#&b", "b_@2_#&c"], &[7; 6], &[1, 1, 2, 2, 4, 4], &[&["pelvis"], &["arm"], &["pelvis"], &["arm"], &["pelvis"], &["arm"]]);
        assert!(together(&figure).is_none());
        // The same without levels: the body replaced, the arms kept.
        let flat = object(&["b_@0_#&a", "b_@1_#&b"], &[3, 3], &[255, 255], &[&["pelvis"], &["arm"]]);
        assert_eq!(together(&flat).map(|p| (p.replaced, p.kept.iter().map(|k| k.mesh).collect::<Vec<_>>())), Some((vec![0], vec![1])));
    }

    #[test]
    fn states_other_groups_and_unnamed_meshes_are_not_parts_shown_together() {
        // A second mesh only another choice shows: another state, not a part shown with it.
        assert!(together(&object(&["g_@0_#&a", "g_@1_#&b"], &[1, 2, 2], &[255; 3], &[&["a"], &["b"], &["c"]])).is_none());
        // One mesh per choice: `object_states`.
        assert!(together(&object(&["g_@0_#&a", "g_@1_#&b"], &[1, 2], &[255; 2], &[&["a"], &[]])).is_none());
        // A part of another group: `kept_parts`.
        assert!(together(&object(&["body_@0_#&a", "markers_@0_#&none", "markers_@1_#&ring"], &[1, 4], &[255; 2], &[&["a"], &["b"]])).is_none());
        // The same bones for every mesh: plain body groups.
        assert!(together(&ModelFacts { mixed_mesh_bindings: false, ..object(&["g_@0_#&a", "g_@1_#&b"], &[1, 3, 3], &[255; 3], &[&["a"], &["a"], &["a"]]) }).is_none());
        // Meshes without names cannot be told apart in the decompile.
        let mut unnamed = object(&["g_@0", "g_@1"], &[1, 3], &[255; 2], &[&["a"], &["b"]]);
        unnamed.mesh_names[1].clear();
        assert!(together(&unnamed).is_none());
        // No mesh the default choice shows in a level.
        assert!(together(&ModelFacts { default_mesh_group_mask: Some(4), ..object(&["g_@0_#&a", "g_@1_#&b", "g_@2_#&c"], &[1, 3], &[255; 2], &[&["a"], &["b"]]) }).is_none());
    }

    #[test]
    fn a_state_drawn_with_two_meshes_keeps_its_second_one() {
        // Two states of one group, the second drawn with two meshes, all stored without names.
        let mut ground = object(&["group0_@0", "group0_@1"], &[1, 2, 2], &[255; 3], &[&["p0", "p3"], &["p1", "p4"], &["p2", "p5"]]);
        ground.mesh_names = vec![String::new(); 3];
        let parts = state_parts(&ground).expect("pieces of a state");
        assert_eq!(parts.replaced, vec![0, 1]);
        assert_eq!(parts.kept.iter().map(|p| (p.mesh, p.name.as_str())).collect::<Vec<_>>(), vec![(2, "unnamed_3")]);
        assert_eq!(kept_line(&parts.kept[0]), KEPT_PIECES);
        assert!(together(&ground).is_none());
        // One mesh per state is `object_states`' kind; a mesh in two states is not a state.
        let mut one = object(&["g_@0_#&a", "g_@1_#&b"], &[1, 2], &[255; 2], &[&["a"], &["b"]]);
        assert!(state_parts(&one).is_none());
        one.mesh_group_masks = vec![1, 3];
        assert!(state_parts(&one).is_none());
        // Some names but not all, levels of detail, or two groups: none.
        let mut some = ground.clone();
        some.mesh_names[0] = "a".into();
        assert!(state_parts(&some).is_none());
        assert!(state_parts(&ModelFacts { lod_meshes: vec![2, 1], ..ground.clone() }).is_none());
        assert!(state_parts(&ModelFacts { mesh_groups: vec!["a_@0".into(), "b_@0".into()], ..ground.clone() }).is_none());
        // With its own switch closed it stays closed, word for word; its own switch (on in the app)
        // opens the bindings gate, and the switch of parts shown together does not.
        use crate::ability_objects::{REASON_BODY_GROUP_BINDINGS, SWITCHES, Switches, offered_with};
        let facts = ModelFacts { bones: vec!["p0".into(), "p1".into(), "p2".into(), "p3".into(), "p4".into(), "p5".into()], materials: vec!["m.vmat".into()], unnamed_body_choices: true, ..ground };
        let closed = Switches { state_parts: false, ..SWITCHES };
        assert_eq!(offered_with(&facts, false, &closed), Err(REASON_BODY_GROUP_BINDINGS));
        assert_eq!(offered_with(&facts, false, &SWITCHES), Ok(()));
        assert_eq!(offered_with(&facts, false, &Switches { state_parts: true, ..closed }), Ok(()));
        assert_eq!(offered_with(&facts, false, &Switches { parts_together: true, ..closed }), Err(REASON_BODY_GROUP_BINDINGS));
    }
    /// A body in three levels of detail and, beside it in each, a part another body group shows.
    fn shawled() -> ModelFacts {
        let mut facts = object(&["default_@0_#&on", "shawl_@0_#&off", "shawl_@1_#&on"], &[1, 4, 1, 4, 1, 4], &[1, 1, 2, 2, 4, 4], &[&["root", "body"], &["root", "neck", "body"], &["root", "body"], &["root", "neck", "body"], &["root"], &["root", "neck"]]);
        facts.default_mesh_group_mask = Some(3);
        facts.bones = vec!["root".into(), "body".into(), "neck".into(), "tail".into()];
        facts
    }

    #[test]
    fn a_part_of_another_group_is_kept_in_every_level() {
        let facts = shawled();
        assert!(together(&facts).is_none() && state_parts(&facts).is_none(), "neither earlier rule takes it");
        let parts = in_levels(&facts).expect("kept in every level");
        assert_eq!(parts.replaced, vec![0, 2, 4]);
        assert_eq!(parts.kept.iter().map(|p| p.mesh).collect::<Vec<_>>(), vec![1, 3, 5]);
        // The bones the game binds the kept parts to, in skeleton order.
        assert_eq!(kept_bones(&facts, &parts), ["root", "body", "neck"]);
        // Without levels of detail, `kept_parts` alone covers it.
        let flat = object(&["default_@0_#&on", "shawl_@0_#&off", "shawl_@1_#&on"], &[1, 4], &[255, 255], &[&["root"], &["neck"]]);
        assert!(in_levels(&ModelFacts { default_mesh_group_mask: Some(3), ..flat }).is_none());
        // A level whose second mesh the default choice shows too (two replaced in one level): none.
        let mut three = shawled();
        three.mesh_group_masks[1] = 5;
        assert!(in_levels(&three).is_none());
        // A kept part with no binding stated: no bones to mark.
        let mut unbound = shawled();
        unbound.mesh_bindings[3].clear();
        assert!(kept_bones(&unbound, &in_levels(&unbound).unwrap()).is_empty());
    }

    #[test]
    fn levels_with_kept_parts_open_only_behind_their_own_switch() {
        use crate::ability_objects::{REASON_EXTRA_MESHES, SWITCHES, Switches, offered_with};
        let facts = ModelFacts { materials: vec!["m.vmat".into()], mesh_skeleton_bones: Some(4), ..shawled() };
        assert_eq!(offered_with(&facts, false, &Switches { parts_kept_in_levels: false, ..SWITCHES }), Err(REASON_EXTRA_MESHES), "closed without its own switch, word for word");
        assert_eq!(offered_with(&facts, false, &SWITCHES), Ok(()), "open in the app");
        assert_eq!(offered_with(&facts, false, &Switches { parts_kept_in_levels: true, ..SWITCHES }), Ok(()));
        assert_eq!(offered_with(&facts, false, &Switches { parts_kept_in_levels: true, parts_kept: false, ..SWITCHES }).is_ok(), false);
        // Two meshes the default choice shows in one level stay closed with the switch open.
        let mut three = facts.clone();
        three.mesh_group_masks[1] = 5;
        assert!(offered_with(&three, false, &Switches { parts_kept_in_levels: true, ..SWITCHES }).is_err());
    }

    #[test]
    fn the_cameras_are_reversed_only_against_their_attachments_order() {
        let keys = "<!-- kv3 -->\n{\n\tAttachmentCameraData = \n\t[\n\t\t{\n\t\t\tattachment_name = \"c\"\n\t\t},\n\t\t{\n\t\t\tattachment_name = \"b\"\n\t\t},\n\t\t{\n\t\t\tattachment_name = \"a\"\n\t\t},\n\t]\n\tOther = \n\t{\n\t\tattachment_name = \"z\"\n\t}\n}\n";
        assert_eq!(camera_order(keys), ["c", "b", "a"]);
        let at = |names: &[&str]| names.iter().map(|n| format!("{n}@root")).collect::<Vec<_>>();
        assert!(cameras_reversed(keys, &at(&["a", "hit", "b", "c"])));
        assert!(!cameras_reversed(keys, &at(&["c", "b", "a"])));
        assert!(!cameras_reversed(keys, &at(&["b", "a", "c"])));
        assert!(!cameras_reversed(keys, &at(&["a", "b"])), "a camera without its attachment");
        let one = "{\n\tAttachmentCameraData = \n\t[\n\t\t{\n\t\t\tattachment_name = \"a\"\n\t\t},\n\t]\n}\n";
        assert!(!cameras_reversed(one, &at(&["a"])), "one camera has no order");
        assert!(camera_order("{\n}\n").is_empty());
    }

    #[test]
    fn a_resting_turn_comes_back_through_its_angles_with_the_sign_of_the_angles() {
        // Every turn written as Euler angles comes back as itself; its negation comes back with the other sign.
        for angles in [[0., 0., 0.], [30., 40., 50.], [-89., 170., -179.9], [0.000_005, -1.294, -179.999_9], [45., -135., 179.0], [10., 0.5, 90.]] {
            let q = angle_quaternion(angles);
            let back = quaternion_angles(q);
            let again = angle_quaternion(back);
            assert!(q.iter().zip(again).all(|(a, b)| (a - b).abs() < 1e-9), "{angles:?}: {q:?} / {again:?}");
            assert!(!round_trip_flips(q), "{angles:?}");
            assert!(round_trip_flips(q.map(|c| -c)), "{angles:?}");
        }
        // Near the pole (forward almost straight up): the roll is folded into the yaw.
        let pole = angle_quaternion([-89.99, 20., 0.]);
        assert!(!round_trip_flips(pole) && round_trip_flips(pole.map(|c| -c)));
    }

    #[test]
    fn the_kept_line_names_the_part_once_whatever_its_level() {
        let part = |name: &str| KeptPart { mesh: 1, name: name.into(), group: "default".into(), choices: vec![] };
        assert_eq!(kept_line(&part("doorman_frame")), kept_line(&part("doorman_frame")));
        assert_eq!(kept_line(&part("arms")), kept_line(&part("arms_lod2")));
        assert!(kept_line(&part("arms")).starts_with("Its arms"), "{}", kept_line(&part("arms")));
    }
}
