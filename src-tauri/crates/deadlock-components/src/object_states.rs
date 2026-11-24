//! Body-group states bound to different bones (`Switches::body_group_states`).
//!
//! Some ability objects hold one mesh per choice of a single body group, each
//! choice a state of the same object (whole and broken, flying and planted),
//! with each state's mesh bound to other bones than the next state's
//! (`ModelFacts::mixed_mesh_bindings`). Every state draws the user's model, as
//! body groups do; each state's copy is bound as the game binds that state's
//! mesh: to no bone where the game's is unskinned, else only to bones the game's
//! state mesh is bound to (the build writes each state's mesh with that state's
//! bones as its skeleton and weights its points only to them; the compiler then
//! lists for each mesh the bones it binds, which the read-back checks). Decided
//! from the masks, the body groups and the bindings alone, never from a name.
use crate::ability_objects::{CarriedCheck, ModelFacts, body_group_choice};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// The line of a states object with several bones under "How your model moves"
/// (`Moving::per_state`): the build binds each state by that state's own bones, so the
/// page offers no bone to pick.
pub const PER_STATE_LINE: &str = "This object shows one of several states, and each state follows the game's own bones for that state: the app picks what your model follows in each one.";

/// How the build bound one state (the receipt's `CarriedEntry::state_bindings`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StateBinding {
    /// The game's mesh of that state (its name) and the choice that shows it.
    pub mesh: String,
    pub choice: String,
    /// The bones the state's mesh was written with; empty for a state the game does not skin.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bones: Vec<String>,
    /// Several bones: what each part of the user's file follows in this state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binding: Option<crate::object_follow::Binding>,
}

/// One state: the game's mesh that one choice shows.
#[derive(Debug, Clone, PartialEq)]
pub struct State {
    /// The embedded mesh (mesh order of the compiled model).
    pub mesh: usize,
    /// Its name (`CTRL embedded_meshes[].m_Name`, the vmdl's `RenderMeshFile` name).
    pub name: String,
    /// The choice that shows it (`m_meshGroups` entry index) and the choice's name.
    pub choice: usize,
    pub choice_name: String,
    /// Whether the default choice shows it (the object as it normally shows).
    pub default: bool,
    /// The bones the game binds it to (`MDAT m_skeleton.m_bones`), in the game's order;
    /// empty for a mesh the game does not skin.
    pub bones: Vec<String>,
}
/// The states of one body group.
#[derive(Debug, Clone, PartialEq)]
pub struct States {
    pub group: String,
    /// In mesh order.
    pub states: Vec<State>,
}

/// Whether some mesh is bound to a bone but not to that bone's parent (`bones` and
/// `parents`: the model skeleton, `m_modelSkeleton.m_boneName` / `m_nParent`;
/// `bindings`: each mesh's bones). Such a mesh's bones are not a skeleton of their own.
pub fn binding_gaps(bones: &[String], parents: &[i64], bindings: &[Vec<String>]) -> bool {
    bindings.iter().any(|bound| {
        bound.iter().any(|name| match bones.iter().position(|b| b == name) {
            None => true,
            Some(i) => usize::try_from(parents.get(i).copied().unwrap_or(-1)).ok().and_then(|p| bones.get(p)).is_some_and(|parent| !bound.contains(parent)),
        })
    })
}

/// The ONE rule: every embedded mesh is shown by exactly one choice of one and
/// the same body group, every choice of that group shows exactly one mesh, the
/// default choice shows one of them, there are no levels of detail, the meshes
/// are bound to different bones, and each mesh's bones hold their parents.
/// `None` for every other model (a choice drawing two meshes included).
pub fn states(facts: &ModelFacts) -> Option<States> {
    let default = facts.default_mesh_group_mask?;
    let count = facts.meshes.len();
    if !facts.mixed_mesh_bindings || facts.mesh_binding_gaps || count < 2 || facts.mesh_groups.is_empty() || facts.mesh_groups.len() > 64 || !facts.lod_meshes.is_empty() {
        return None;
    }
    if facts.mesh_group_masks.len() != count || facts.mesh_names.len() != count || facts.mesh_bindings.len() != count || facts.mesh_names.iter().any(String::is_empty) {
        return None;
    }
    let groups: Vec<(String, String)> = facts.mesh_groups.iter().map(|g| body_group_choice(g)).collect();
    let group = &groups[0].0;
    if groups.iter().any(|(g, _)| g != group) {
        return None;
    }
    let mut shown = BTreeSet::new();
    let mut states = vec![];
    for (mesh, mask) in facts.mesh_group_masks.iter().enumerate() {
        // Exactly one choice, one the groups know of.
        if mask.count_ones() != 1 || mask >> groups.len() != 0 {
            return None;
        }
        let choice = mask.trailing_zeros() as usize;
        if !shown.insert(choice) {
            return None;
        }
        states.push(State { mesh, name: facts.mesh_names[mesh].clone(), choice, choice_name: groups[choice].1.clone(), default: default == *mask, bones: facts.mesh_bindings[mesh].clone() });
    }
    // Every choice shows exactly one mesh, and the default is one of them.
    if shown.len() != groups.len() || !states.iter().any(|s| s.default) {
        return None;
    }
    Some(States { group: group.clone(), states })
}

/// The read-back of a states object (`ability_objects::carried_check`): each state's
/// compiled mesh in the game's place under the game's name, unskinned where the game's
/// is, else bound to one or more of the bones the game binds that state's mesh to and to
/// no other. `want` /
/// `got`: the game's and the compiled meshes' bones, in mesh order. Nothing for any
/// other object.
pub fn check(theirs: &ModelFacts, ours: &ModelFacts, want: &[Vec<String>], got: &[Vec<String>], check: &mut CarriedCheck) {
    let Some(states) = states(theirs) else { return };
    let (mut rows, mut same) = (0, 0);
    for state in &states.states {
        rows += 2;
        // Unskinned where the game's is; else bound to some of the bones the game binds that state
        // to and to no other (which of them depends on what the user's parts follow; the game's list
        // itself cannot be written back, the compiled points index the compiler's own list).
        let bound = got.get(state.mesh).is_some_and(|got| if state.bones.is_empty() { got.is_empty() } else { !got.is_empty() && got.iter().all(|b| state.bones.contains(b)) });
        if want.get(state.mesh) == Some(&state.bones) && bound {
            same += 1;
        } else {
            check.differs.push(format!("MDAT[{}] (state {}) bound to {:?}, the game's to {:?}", state.mesh, state.choice_name, got.get(state.mesh), want.get(state.mesh)));
        }
        if ours.mesh_names.get(state.mesh) == Some(&state.name) {
            same += 1;
        } else {
            check.differs.push(format!("mesh {} is {:?}, the game's state {}", state.mesh, ours.mesh_names.get(state.mesh), state.name));
        }
    }
    check.compared.insert("MDAT states".into(), (rows, same));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn object(groups: &[&str], masks: &[u64], bindings: &[&[&str]]) -> ModelFacts {
        ModelFacts {
            mesh_groups: groups.iter().map(|g| (*g).to_owned()).collect(),
            mesh_group_masks: masks.to_vec(),
            default_mesh_group_mask: Some(1),
            mesh_names: (0..masks.len()).map(|i| format!("mesh_{i}")).collect(),
            mesh_bindings: bindings.iter().map(|b| b.iter().map(|n| (*n).to_owned()).collect()).collect(),
            mixed_mesh_bindings: true,
            meshes: masks.iter().map(|_| crate::ability_objects::MeshFacts { lod_mask: 255, ..Default::default() }).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn one_mesh_per_choice_of_one_group_is_a_set_of_states() {
        let two = object(&["state_@0_#&whole", "state_@1_#&broken"], &[1, 2], &[&["root", "body"], &[]]);
        let s = states(&two).expect("states");
        assert_eq!(s.group, "state");
        assert_eq!(s.states.iter().map(|s| (s.mesh, s.choice, s.choice_name.as_str(), s.default)).collect::<Vec<_>>(), vec![(0, 0, "whole", true), (1, 1, "broken", false)]);
        assert_eq!(s.states[1].bones, Vec::<String>::new());
        // Meshes in another order than the choices: each keeps its own choice.
        let four = object(&["d_@0_#&a", "d_@1_#&b", "d_@2_#&c", "d_@3_#&e"], &[1, 8, 2, 4], &[&["base"], &["base", "x"], &["base", "x", "y"], &["base"]]);
        assert_eq!(states(&four).expect("states").states.iter().map(|s| s.choice).collect::<Vec<_>>(), vec![0, 3, 1, 2]);
    }

    #[test]
    fn parts_two_meshes_of_one_choice_other_groups_and_one_binding_are_not_states() {
        // The same bones for every mesh: plain body groups.
        assert!(states(&ModelFacts { mixed_mesh_bindings: false, ..object(&["s_@0_#&a", "s_@1_#&b"], &[1, 2], &[&["root"], &["root"]]) }).is_none());
        // A choice drawing two meshes.
        assert!(states(&object(&["g_@0_#&a", "g_@1_#&b"], &[1, 2, 2], &[&["a"], &["b"], &["c"]])).is_none());
        // A mesh shown by two choices, or by none.
        assert!(states(&object(&["g_@0_#&a", "g_@1_#&b"], &[1, 3], &[&["a"], &["b"]])).is_none());
        assert!(states(&object(&["g_@0_#&a", "g_@1_#&b"], &[1, 0], &[&["a"], &["b"]])).is_none());
        // Choices of two groups (a part shown with the object, `kept_parts`).
        assert!(states(&object(&["body_@0_#&a", "markers_@0_#&none", "markers_@1_#&ring"], &[1, 4], &[&["a"], &["b"]])).is_none());
        // A choice that shows nothing.
        assert!(states(&object(&["g_@0_#&a", "g_@1_#&b", "g_@2_#&c"], &[1, 2], &[&["a"], &["b"]])).is_none());
        // A default that shows none of them, or two.
        assert!(states(&ModelFacts { default_mesh_group_mask: Some(3), ..object(&["g_@0_#&a", "g_@1_#&b"], &[1, 2], &[&["a"], &["b"]]) }).is_none());
        // Levels of detail.
        assert!(states(&ModelFacts { lod_meshes: vec![1, 1], ..object(&["g_@0_#&a", "g_@1_#&b"], &[1, 2], &[&["a"], &["b"]]) }).is_none());
        // A mesh bound to a bone without its parent.
        assert!(states(&ModelFacts { mesh_binding_gaps: true, ..object(&["g_@0_#&a", "g_@1_#&b"], &[1, 2], &[&["a"], &["b"]]) }).is_none());
        // A mesh without a name (the older form) cannot be told apart in the decompile.
        let mut unnamed = object(&["g_@0", "g_@1"], &[1, 2], &[&["a"], &["b"]]);
        unnamed.mesh_names[1].clear();
        assert!(states(&unnamed).is_none());
    }

    #[test]
    fn the_switch_alone_opens_a_states_object_word_for_word_closed_without_it() {
        use crate::ability_objects::{REASON_BODY_GROUP_BINDINGS, SWITCHES, Switches, offered_with};
        // One bone, drawn by effects, one material, two states: the second one unskinned.
        let facts = ModelFacts {
            bones: vec!["root".into()],
            mesh_skeleton_bones: Some(1),
            embedded_meshes: 2,
            materials: vec!["models/a/a.vmat".into()],
            mesh_names: vec!["whole".into(), "broken".into()],
            ..object(&["s_@0_#&whole", "s_@1_#&broken"], &[1, 2], &[&["root"], &[]])
        };
        assert_eq!(offered_with(&facts, false, &Switches { body_group_states: false, ..SWITCHES }), Err(REASON_BODY_GROUP_BINDINGS));
        assert_eq!(offered_with(&facts, false, &Switches { body_group_states: true, ..SWITCHES }), Ok(()));
        // A choice drawing two meshes stays closed under this switch, word for word: only the
        // switch of further pieces of a state (`object_parts::state_parts`) opens it.
        let two = ModelFacts { mesh_names: vec!["a".into(), "b".into(), "c".into()], ..object(&["s_@0", "s_@1"], &[1, 2, 2], &[&["root"], &[], &[]]) };
        let two = ModelFacts { bones: facts.bones.clone(), mesh_skeleton_bones: Some(1), materials: facts.materials.clone(), ..two };
        assert_eq!(offered_with(&two, false, &Switches { body_group_states: true, state_parts: false, ..SWITCHES }), Err(REASON_BODY_GROUP_BINDINGS));
        assert_eq!(offered_with(&two, false, &Switches { body_group_states: true, ..SWITCHES }), Ok(()));
    }

    #[test]
    fn the_kind_of_a_states_object_with_bones_says_the_app_picks_each_state_s_bones() {
        use crate::ability_objects::object_kind;
        let states = ModelFacts {
            bones: vec!["root".into(), "body".into()],
            mesh_bones: vec!["body".into()],
            materials: vec!["models/a/a.vmat".into()],
            ..object(&["s_@0_#&whole", "s_@1_#&broken"], &[1, 2], &[&["root", "body"], &[]])
        };
        let moving = object_kind(&states, false).and_then(|k| k.moving).expect("moving parts");
        assert_eq!(moving.per_state.as_deref(), Some(PER_STATE_LINE));
        // Plain body groups with the same bones: no such line, and the key is absent.
        let plain = ModelFacts { mixed_mesh_bindings: false, mesh_bindings: vec![], ..states.clone() };
        let moving = object_kind(&plain, false).and_then(|k| k.moving).expect("moving parts");
        assert_eq!(moving.per_state, None);
        assert!(!serde_json::to_string(&moving).unwrap().contains("per_state"));
    }

    #[test]
    fn a_binding_without_its_parent_bone_is_a_gap() {
        let bones: Vec<String> = ["base", "mine", "sensor"].iter().map(|s| (*s).to_owned()).collect();
        let parents = [-1, 0, 1];
        let b = |names: &[&str]| names.iter().map(|s| (*s).to_owned()).collect::<Vec<String>>();
        assert!(!binding_gaps(&bones, &parents, &[b(&["base"]), b(&["base", "mine"]), b(&["base", "mine", "sensor"]), b(&[])]));
        assert!(binding_gaps(&bones, &parents, &[b(&["base"]), b(&["mine"])]));
        assert!(binding_gaps(&bones, &parents, &[b(&["base", "sensor"])]));
        assert!(binding_gaps(&bones, &parents, &[b(&["other"])]));
    }

    #[test]
    fn the_read_back_wants_each_state_bound_as_the_games() {
        let game = ModelFacts { mesh_names: vec!["whole".into(), "broken".into()], ..object(&["s_@0_#&whole", "s_@1_#&broken"], &[1, 2], &[&["root", "body"], &[]]) };
        let want = vec![vec!["root".to_owned(), "body".to_owned()], vec![]];
        let mut ok = CarriedCheck::default();
        check(&game, &game, &want, &want, &mut ok);
        assert!(ok.differs.is_empty(), "{ok:?}");
        assert_eq!(ok.compared["MDAT states"], (4, 4));
        // Fewer of the state's bones (the user's parts follow only one): accepted.
        let mut fewer = CarriedCheck::default();
        check(&game, &game, &want, &[vec!["body".to_owned()], vec![]], &mut fewer);
        assert!(fewer.differs.is_empty(), "{fewer:?}");
        // Every state bound alike (one body file for every choice): refused.
        let mut alike = CarriedCheck::default();
        check(&game, &game, &want, &[want[0].clone(), want[0].clone()], &mut alike);
        assert_eq!(alike.differs.len(), 1, "{alike:?}");
        // A bone the state does not follow, or none where the game's is bound: refused.
        let four = ModelFacts { mesh_names: vec!["a".into(), "b".into()], ..object(&["d_@0_#&a", "d_@1_#&b"], &[1, 2], &[&["base"], &["base", "x"]]) };
        let want4 = vec![vec!["base".to_owned()], vec!["base".to_owned(), "x".to_owned()]];
        let mut other = CarriedCheck::default();
        check(&four, &four, &want4, &[vec!["x".to_owned()], vec![]], &mut other);
        assert_eq!(other.differs.len(), 2, "{other:?}");
        // Another mesh in the state's place: refused.
        let mut renamed = CarriedCheck::default();
        check(&game, &ModelFacts { mesh_names: vec!["broken".into(), "whole".into()], ..game.clone() }, &want, &want, &mut renamed);
        assert_eq!(renamed.differs.len(), 2, "{renamed:?}");
        // Any other object: nothing checked.
        let mut none = CarriedCheck::default();
        check(&ModelFacts::default(), &ModelFacts::default(), &want, &[], &mut none);
        assert!(none.differs.is_empty() && none.compared.is_empty());
    }
}
