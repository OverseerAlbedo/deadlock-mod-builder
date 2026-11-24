//! Building an ability object whose body-group choices are states of one object,
//! one mesh per choice, the meshes bound to different bones
//! (`deadlock_components::object_states`, `Switches::body_group_states`).
//!
//! Every state draws the user's model (one fit for the object, to the default
//! state's bounds), written as its own mesh file in the place of the game's
//! state mesh and bound as the game binds that state's mesh:
//! * a state the game does not skin: an unskinned mesh (it follows no bone);
//! * a bound state: a mesh whose skeleton is exactly that state's bones, each
//!   part of the user's file following one of them by the row's way (`object_follow::binding`)
//!   with the carriers and the vote taken from that state's own game mesh only;
//!   on an object with one bone, wholly that bone.
//!
//! The read-back then requires each state bound exactly as the game's
//! (`object_states::check` inside `carried_check`).
use crate::ability_objects::{fit_bounds, fitted_meshes, game_skeleton, mesh_nodes, part_name, dmx_dominant_joints, dmx_triangles, CarriedFiles, Original};
use crate::invalid;
use deadlock_components::ability_objects::{self as objects, ModelFacts};
use deadlock_components::{object_follow, object_states};
use deadlock_resource::text::{Document, Edit};
use deadlock_core::{safety, Result};
use deadlock_preparation::ability_object::Packet;
use deadlock_project::ability_objects::{Follow, FollowWay, ObjectFit};
use deadlock_skeleton::export::{export_bytes_with_axis, export_rigid_bytes, PreparedMesh, PreparedReport};
use deadlock_skeleton::Skeleton;
use std::collections::BTreeMap;

/// The user's file weights a part (own skeleton) to a bone of the object that one of
/// its states is not bound to: each state follows only its own bones.
pub const STATE_BONES: &str = "Your file weights a part to a bone that one of the game object's states does not follow; with this object each state follows only its own bones";

/// The game's skeleton cut down to `bones` (each holding its parent, `object_states`'
/// rule), in the game's order.
pub fn state_skeleton(skeleton: &Skeleton, bones: &[String]) -> Result<Skeleton> {
    let keep: Vec<usize> = skeleton.names.iter().enumerate().filter(|(_, n)| bones.contains(n)).map(|(i, _)| i).collect();
    if keep.len() != bones.len() {
        return Err(invalid("A state of the game's object is bound to a bone its skeleton does not have."));
    }
    let mut parents = vec![];
    for i in &keep {
        let parent = skeleton.parents[*i];
        parents.push(if parent < 0 {
            -1
        } else {
            let at = keep.iter().position(|k| *k as i32 == parent).ok_or_else(|| invalid("A state of the game's object is bound to a bone without its parent."))?;
            at as i32
        });
    }
    Ok(Skeleton {
        names: keep.iter().map(|i| skeleton.names[*i].clone()).collect(),
        parents,
        positions: keep.iter().map(|i| skeleton.positions[*i]).collect(),
        rotations: keep.iter().map(|i| skeleton.rotations[*i]).collect(),
        scales: keep.iter().map(|i| skeleton.scales[*i]).collect(),
        flags: if skeleton.flags.len() == skeleton.names.len() { keep.iter().map(|i| skeleton.flags[*i]).collect() } else { vec![] },
    })
}

/// The vmdl of a states object with no bone marked `do_not_discard` (each made `false`).
/// With every bone marked (as the decompile writes them) the compiler lists every bone
/// for every mesh; without the marks it lists the bones the mesh's points use (seen on
/// one object with its root bone as well), as the game's file lists fewer bones for
/// some states. The bones stayed in the model on every object built. The text unchanged
/// for every other object and while the switch is closed (the hook in
/// `ObjectBuild::build_carried`).
pub fn vmdl(facts: &ModelFacts, text: String) -> Result<String> {
    if !objects::switches().body_group_states || object_states::states(facts).is_none() {
        return Ok(text);
    }
    released_bones(text)
}
/// `text` with every bone's `do_not_discard = true` made `false`; nothing else changes.
pub fn released_bones(text: String) -> Result<String> {
    let document = Document::parse(&text).map_err(|e| invalid(e.to_string()))?;
    let edits: Vec<Edit> = document
        .objects(Some("Bone"))
        .iter()
        .filter_map(|bone| bone.get("do_not_discard"))
        .filter(|value| value.scalar() == Some("true"))
        .map(|value| Edit { start: value.start, end: value.end, replacement: "false".into() })
        .collect();
    if edits.is_empty() {
        return Ok(text);
    }
    document.apply(&edits).map_err(|e| invalid(e.to_string()))
}

/// One state of a states object as the build binds it (`bound_states`).
pub struct BoundState {
    pub state: object_states::State,
    /// The state's own decompiled mesh file in the game's object.
    pub file: String,
    /// The skeleton the state's mesh is written with: exactly that state's bones
    /// (`state_skeleton`); `None` for a state the game does not skin.
    pub skeleton: Option<Skeleton>,
    /// The user's fitted meshes as the build writes them for this state (weighted to
    /// `skeleton`'s bones; unweighted for an unskinned state).
    pub meshes: Vec<PreparedMesh>,
    /// Several bones: the bones carrying this state's game mesh, and what each part follows.
    pub carriers: Vec<object_follow::Carrier>,
    pub binding: Option<object_follow::Binding>,
}

/// The states object's mesh nodes, checked against the compiled model's mesh names.
fn state_nodes(original: &Original) -> Result<Vec<crate::ability_objects::MeshNode>> {
    let text = crate::sources::read_text(&original.vmdl_path()?)?;
    let nodes = mesh_nodes(&text)?;
    if nodes.iter().map(|n| &n.name).collect::<Vec<_>>() != original.facts.mesh_names.iter().collect::<Vec<_>>() {
        return Err(invalid(format!("The game's ability object names its meshes {:?}, its decompiled model {:?}; its states cannot be told apart: {}", original.facts.mesh_names, nodes.iter().map(|n| &n.name).collect::<Vec<_>>(), original.target)));
    }
    Ok(nodes)
}

/// The decompiled mesh file of a states object's default state (the object as it
/// normally shows), or `None` for every other object and while the switch is closed.
pub fn default_state_file(original: &Original) -> Result<Option<String>> {
    if !objects::switches().body_group_states {
        return Ok(None);
    }
    let Some(states) = object_states::states(&original.facts) else { return Ok(None) };
    let nodes = state_nodes(original)?;
    Ok(states.states.iter().find(|s| s.default).map(|s| nodes[s.mesh].filename.clone()))
}

/// Every state of a states object bound as the build binds it, in mesh order (the
/// build's order, so a refusal is the build's own), from the user's `fitted` meshes;
/// `None` for every other object and while the switch is closed. The ONE path for the
/// build (`carried_files`) and the fit view (which draws the default state).
pub fn bound_states(packet: &Packet, original: &Original, raw: &[u8], follow: Option<&Follow>, fitted: &[PreparedMesh]) -> Result<Option<Vec<BoundState>>> {
    if !objects::switches().body_group_states {
        return Ok(None);
    }
    let Some(states) = object_states::states(&original.facts) else { return Ok(None) };
    let nodes = state_nodes(original)?;
    let skeleton = if original.facts.bones.is_empty() { None } else { Some(game_skeleton(raw)?) };
    let several = original.facts.several_bones();
    let armature: Vec<String> = packet.armature.iter().map(|b| b.name.clone()).collect();
    let mut out = vec![];
    for state in &states.states {
        let file = nodes[state.mesh].filename.clone();
        if state.bones.is_empty() {
            // The game's state mesh is not skinned: neither is the user's.
            out.push(BoundState { state: state.clone(), file, skeleton: None, meshes: fitted.to_vec(), carriers: vec![], binding: None });
            continue;
        }
        let skeleton = skeleton.as_ref().ok_or_else(|| invalid("A state of the game's object is bound to bones, but the object has no skeleton."))?;
        let own = state_skeleton(skeleton, &state.bones)?;
        let (meshes, carriers, binding) = if several {
            let (weights, carriers, binding) = state_weights(packet, follow, original, &file, &own, &armature, fitted)?;
            (weights, carriers, Some(binding))
        } else {
            // One bone: wholly that bone.
            (fitted.iter().map(|m| PreparedMesh { weights: vec![vec![(own.names[0].clone(), 1.)]; m.positions.len()], ..m.clone() }).collect(), vec![], None)
        };
        out.push(BoundState { state: state.clone(), file, skeleton: Some(own), meshes, carriers, binding });
    }
    Ok(Some(out))
}

/// The mesh files of a states row, or `None` for every other object (and while the
/// switch is closed): the hook at the start of `ability_objects::carried_files`.
pub fn carried_files(packet: &Packet, fit: &ObjectFit, original: &Original, raw: &[u8], follow: Option<&Follow>) -> Result<Option<CarriedFiles>> {
    if !objects::switches().body_group_states || object_states::states(&original.facts).is_none() {
        return Ok(None);
    }
    let (meshes, _) = fitted_meshes(packet, fit, &fit_bounds(original)?, &packet.meshes)?;
    let Some(bound) = bound_states(packet, original, raw, follow, &meshes)? else { return Ok(None) };
    let nodes = state_nodes(original)?;
    let root = objects::object_root(&original.target);
    let leaf = root.rsplit('/').next().unwrap_or("object").to_owned();
    let mut out = CarriedFiles { files: vec![], nodes: BTreeMap::new(), single: None, shell_material: None, shells: None, style: None, binding: None, states: vec![] };
    for state in &bound {
        let node = &nodes[state.state.mesh];
        let bytes = match &state.skeleton {
            None => export_rigid_bytes(&state.meshes, original.axis).map_err(|e| invalid(e.to_string()))?,
            Some(own) => {
                let report = PreparedReport { meshes: state.meshes.clone(), extra: BTreeMap::new() };
                export_bytes_with_axis(&report, own, original.axis).map_err(|e| invalid(e.to_string()))?.0
            }
        };
        let path = format!("{root}/{leaf}_state{}.dmx", state.state.mesh);
        out.files.push((path.clone(), bytes));
        out.nodes.insert(node.name.clone(), Some(path));
        out.states.push(object_states::StateBinding {
            mesh: state.state.name.clone(),
            choice: state.state.choice_name.clone(),
            bones: state.skeleton.as_ref().map(|s| s.names.clone()).unwrap_or_default(),
            binding: state.binding.clone(),
        });
    }
    // `binding` stays the default state's (else the first bound state's); `states` records every state's.
    let bindings: Vec<(bool, &object_follow::Binding)> = bound.iter().filter_map(|s| s.binding.as_ref().map(|b| (s.state.default, b))).collect();
    out.binding = bindings.iter().find(|(default, _)| *default).or(bindings.first()).map(|(_, b)| (*b).clone());
    Ok(Some(out))
}

/// One bound state of an object with several bones: the user's fitted meshes weighted by
/// the row's way, the carriers and the vote taken from the state's own game mesh `file`,
/// every weight on one of the state's bones (`own`).
fn state_weights(packet: &Packet, follow: Option<&Follow>, original: &Original, file: &str, own: &Skeleton, armature: &[String], fitted: &[PreparedMesh]) -> Result<(Vec<PreparedMesh>, Vec<object_follow::Carrier>, object_follow::Binding)> {
    let dmx = deadlock_dmx::Dmx::load(&safety::inside(&original.folder, file)?).map_err(|e| invalid(format!("The game's ability object mesh could not be read ({file}): {e}")))?;
    let points = dmx_dominant_joints(&dmx)?;
    let triangles = dmx_triangles(&dmx);
    let carriers = object_follow::carriers(points.iter().map(|(p, b)| (*p, b.as_str())), &own.names);
    let game = object_follow::surface(&points, &triangles);
    let parts: Vec<object_follow::Part> = packet.meshes.iter().zip(fitted).map(|(m, f)| object_follow::Part { name: part_name(m), points: &f.positions, weights: &m.weights }).collect();
    let same_skeleton = follow.is_some_and(|f| f.way == FollowWay::SameSkeleton);
    if same_skeleton {
        // The file's own weights: only on the bones this state follows.
        for p in &parts {
            if let Some((bone, _)) = p.weights.iter().flatten().find(|(b, w)| *w > 0. && original.facts.bones.contains(b) && !own.names.contains(b)) {
                return Err(invalid(format!("{STATE_BONES}: {} follows {bone}.", p.name)));
            }
        }
    }
    let binding = object_follow::binding(follow, &parts, &game, &carriers, &own.names, armature, objects::switches().same_skeleton).map_err(invalid)?;
    let meshes = packet
        .meshes
        .iter()
        .zip(fitted)
        .map(|(m, f)| {
            let weights = match binding.parts.get(part_name(m)) {
                Some(bone) => vec![vec![(bone.clone(), 1.)]; f.positions.len()],
                None => m.weights.iter().map(|w| {
                    let kept: Vec<(String, f64)> = w.iter().filter(|(_, x)| *x > 0.).cloned().collect();
                    let sum: f64 = kept.iter().map(|(_, x)| x).sum();
                    kept.into_iter().map(|(b, x)| (b, x / sum)).collect()
                }).collect(),
            };
            PreparedMesh { weights, ..f.clone() }
        })
        .collect();
    Ok((meshes, carriers, binding))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_state_skeleton_keeps_the_state_s_bones_in_the_game_s_order_with_their_parents() {
        let skeleton = Skeleton {
            names: ["base", "mine", "a", "b"].iter().map(|s| (*s).to_owned()).collect(),
            parents: vec![-1, 0, 1, 1],
            positions: vec![[0.; 3], [1., 0., 0.], [2., 0., 0.], [3., 0., 0.]],
            rotations: vec![[0., 0., 0., 1.]; 4],
            scales: vec![1.; 4],
            flags: vec![],
        };
        let own = state_skeleton(&skeleton, &["base".into(), "mine".into(), "b".into()]).unwrap();
        assert_eq!(own.names, vec!["base", "mine", "b"]);
        assert_eq!(own.parents, vec![-1, 0, 1]);
        assert_eq!(own.positions[2], [3., 0., 0.]);
        assert!(state_skeleton(&skeleton, &["mine".into()]).is_err(), "a bone without its parent");
        assert!(state_skeleton(&skeleton, &["other".into()]).is_err());
    }

    #[test]
    fn released_bones_change_only_the_do_not_discard_values() {
        let text = "{\n\trootNode = \n\t{\n\t\t_class = \"RootNode\"\n\t\tchildren = \n\t\t[\n\t\t\t{\n\t\t\t\t_class = \"Skeleton\"\n\t\t\t\tchildren = \n\t\t\t\t[\n\t\t\t\t\t{\n\t\t\t\t\t\t_class = \"Bone\"\n\t\t\t\t\t\tname = \"base\"\n\t\t\t\t\t\tdo_not_discard = true\n\t\t\t\t\t\tchildren = \n\t\t\t\t\t\t[\n\t\t\t\t\t\t\t{\n\t\t\t\t\t\t\t\t_class = \"Bone\"\n\t\t\t\t\t\t\t\tname = \"tip\"\n\t\t\t\t\t\t\t\tdo_not_discard = true\n\t\t\t\t\t\t\t},\n\t\t\t\t\t\t]\n\t\t\t\t\t},\n\t\t\t\t]\n\t\t\t},\n\t\t]\n\t}\n}\n";
        let out = released_bones(text.to_owned()).unwrap();
        assert_eq!(out, text.replace("do_not_discard = true", "do_not_discard = false"));
        let none = "{\n\trootNode = \n\t{\n\t\t_class = \"RootNode\"\n\t}\n}\n";
        assert_eq!(released_bones(none.to_owned()).unwrap(), none);
    }
}
