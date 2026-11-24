//! Shells that another choice of an object's own body group shows beside its mesh
//! (`Switches::body_group_shells`).
//!
//! Some objects draw, in each level of detail, their mesh and beside it a slightly
//! larger shell in a material of its own, and the shell is shown only by a choice of
//! the object's own body group that the default does not pick (an effect picks it).
//! The shell is to such an object what an outline shell is to an object the game
//! places: the build makes it from the user's mesh in the game's own shell material
//! (by reference), bound and masked as the game's shell is. Decided from the compiled
//! model's masks, levels of detail, materials and bounds only, never from a name.
use crate::ability_objects::{body_group_choice, flatten, ModelFacts, Switches};
use deadlock_resource::container;
use std::collections::BTreeSet;

/// The shells of a model (`glow_shells`).
#[derive(Debug, Clone, PartialEq)]
pub struct GlowShells {
    /// Per level of detail: the mesh the user's model replaces and, where the level
    /// has one, the shell beside it (mesh indices).
    pub levels: Vec<(usize, Option<usize>)>,
    /// The game's own material every shell draws with (casefolded, without `_c`).
    pub material: String,
    /// The body group the mesh and its shells belong to, and the choices that show the shells.
    pub group: String,
    pub choices: Vec<String>,
}
impl GlowShells {
    /// The shells' mesh indices.
    pub fn shells(&self) -> BTreeSet<usize> {
        self.levels.iter().filter_map(|(_, s)| *s).collect()
    }
}

/// The ONE rule for a body-group shell. A model holds them when all of these hold:
/// - it has body groups and a default choice, and its meshes are all bound to the same bones;
/// - every level of detail draws one mesh that a default choice shows (the object) and at
///   most one other mesh, and every mesh is in a level;
/// - each other mesh (the shell) is shown by no default choice, only by choices that also
///   show its level's object (a strict part of the object's mask), all of the object's own
///   body group: it never shows without the object, so it is not another state of it;
/// - every shell draws with one and the same material, which no object mesh draws with;
/// - each shell's bounds hold its object's bounds and stand out from them.
/// At least one level has a shell, and the model is not an outline (`shell_material`).
/// `None` for every other model.
pub fn glow_shells(facts: &ModelFacts) -> Option<GlowShells> {
    let default = facts.default_mesh_group_mask.filter(|d| *d != 0)?;
    let count = facts.meshes.len();
    if facts.shell_material().is_some() || facts.mixed_mesh_bindings || facts.mesh_groups.is_empty() || facts.mesh_groups.len() > 64 || count < 2 || facts.mesh_group_masks.len() != count {
        return None;
    }
    let groups: Vec<(String, String)> = facts.mesh_groups.iter().map(|g| body_group_choice(g)).collect();
    let masks = &facts.mesh_group_masks;
    if masks.iter().any(|m| *m == 0 || *m >> groups.len() != 0) {
        return None;
    }
    let levels = facts.levels();
    if levels.is_empty() || levels.iter().flatten().collect::<BTreeSet<_>>().len() != count {
        return None;
    }
    let bits = |mask: u64| (0..groups.len()).filter(move |b| mask & (1u64 << b) != 0);
    let mut out = vec![];
    for level in &levels {
        let (bodies, others): (Vec<usize>, Vec<usize>) = level.iter().partition(|i| masks[**i] & default != 0);
        let ([body], rest) = (bodies.as_slice(), others.as_slice()) else { return None };
        let shell = match rest {
            [] => None,
            [shell] => Some(*shell),
            _ => return None,
        };
        if let Some(shell) = shell {
            let (b, s) = (masks[*body], masks[shell]);
            // Shown only together with the object: a strict part of its mask.
            if s & !b != 0 || s == b {
                return None;
            }
        }
        out.push((*body, shell));
    }
    let shells: Vec<usize> = out.iter().filter_map(|(_, s)| *s).collect();
    if shells.is_empty() {
        return None;
    }
    // One body group: every choice that shows an object mesh or a shell.
    let all = out.iter().flat_map(|(b, s)| std::iter::once(*b).chain(*s)).fold(0u64, |m, i| m | masks[i]);
    let named: Vec<&String> = bits(all).map(|b| &groups[b].0).collect::<BTreeSet<_>>().into_iter().collect();
    let [group] = named.as_slice() else { return None };
    // One material, the shells' alone.
    let material: Vec<&String> = shells.iter().flat_map(|s| &facts.meshes[*s].materials).collect::<BTreeSet<_>>().into_iter().collect();
    let [material] = material.as_slice() else { return None };
    if shells.iter().any(|s| facts.meshes[*s].materials.len() != 1) || out.iter().any(|(b, _)| facts.meshes[*b].materials.contains(material)) {
        return None;
    }
    // Around the object: its bounds inside the shell's, standing out.
    for (body, shell) in &out {
        let Some(shell) = shell else { continue };
        let (Some(b), Some(s)) = (facts.meshes[*body].bounds, facts.meshes[*shell].bounds) else { return None };
        if (0..3).any(|k| s.min[k] > b.min[k] || s.max[k] < b.max[k]) || (0..3).all(|k| s.min[k] == b.min[k] && s.max[k] == b.max[k]) {
            return None;
        }
    }
    let shown: u64 = shells.iter().fold(0, |m, s| m | masks[*s]);
    Some(GlowShells { levels: out, material: (*material).clone(), group: (*group).clone(), choices: bits(shown).map(|b| groups[b].1.clone()).collect() })
}

/// Whether `switches` open this object's extra meshes as body-group shells.
pub fn opens(facts: &ModelFacts, switches: &Switches) -> bool {
    switches.body_group_shells && glow_shells(facts).is_some()
}

/// The kept-list line for the shells ("Its glow: a shell made from your object, in the game's own material").
pub fn kept_line(facts: &ModelFacts) -> Option<String> {
    let shells = glow_shells(facts)?;
    let name = shells.choices.first().filter(|c| !c.is_empty()).cloned().unwrap_or_else(|| "shell".into());
    let words = crate::sound_tags::readable(&crate::sound_tags::words(&name));
    Some(format!("Its {}: a shell made from your object, in the game's own material, shown when the game shows it", deadlock_core::text::casefold(&words)))
}

/// The semantic names of each embedded mesh's vertex buffer fields (`CTRL
/// embedded_meshes[i].m_vertexBuffers[].m_inputLayoutFields[].m_pSemanticName`), in mesh order.
fn vertex_fields(raw: &[u8]) -> Vec<BTreeSet<String>> {
    let Some(ctrl) = container::read_blocks(raw, "CTRL").ok().and_then(|c| c.into_iter().next()) else { return vec![] };
    let rows = flatten(&ctrl.root);
    let mut out: Vec<BTreeSet<String>> = vec![];
    for (key, value) in &rows {
        let Some(rest) = key.strip_prefix(".embedded_meshes[") else { continue };
        let Some((index, tail)) = rest.split_once(']') else { continue };
        if !(tail.starts_with(".m_vertexBuffers[") && tail.ends_with(".m_pSemanticName")) {
            continue;
        }
        let Ok(index) = index.parse::<usize>() else { continue };
        if out.len() <= index {
            out.resize(index + 1, BTreeSet::new());
        }
        out[index].insert(value.trim_matches('"').to_owned());
    }
    out
}

/// What a rebuilt model with shells must hold beyond the read-back (`carried_check`
/// leaves every render row out): the same number of meshes; each shell in the game
/// shell's place drawing with exactly the game's shell material, with a vertex colour
/// field when the game's shell has one; no object mesh drawing with that material.
/// Empty when the game's model has no shells (`glow_shells`) or when all holds.
pub fn check_compiled(original: &[u8], compiled: &[u8]) -> crate::Result<Vec<String>> {
    let theirs = crate::ability_objects::facts(original)?;
    let Some(shells) = glow_shells(&theirs) else { return Ok(vec![]) };
    let ours = crate::ability_objects::facts(compiled)?;
    let mut out = vec![];
    if ours.meshes.len() != theirs.meshes.len() {
        out.push(format!("shells: {} meshes in the game's object / {}", theirs.meshes.len(), ours.meshes.len()));
        return Ok(out);
    }
    let (want, got) = (vertex_fields(original), vertex_fields(compiled));
    for (body, shell) in &shells.levels {
        if ours.meshes[*body].materials.contains(&shells.material) {
            out.push(format!("mesh {body} (your object) draws with the shell material {}", shells.material));
        }
        let Some(shell) = shell else { continue };
        if ours.meshes[*shell].materials != [shells.material.clone()] {
            out.push(format!("mesh {shell} (shell) draws with {:?}, the game's shell with {}", ours.meshes[*shell].materials, shells.material));
        }
        if want.get(*shell).is_some_and(|f| f.contains("COLOR")) && !got.get(*shell).is_some_and(|f| f.contains("COLOR")) {
            out.push(format!("mesh {shell} (shell) has no vertex colours, the game's shell has"));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ability_objects::{Bounds, MeshFacts};

    fn mesh(lod: u64, material: &str, half: f64) -> MeshFacts {
        MeshFacts { lod_mask: lod, materials: vec![material.into()], bounds: Some(Bounds { min: [-half; 3], max: [half; 3] }) }
    }
    /// An object as the game's briefcase is made: per level its mesh (both choices) and,
    /// in three of four levels, a larger shell only the second choice shows.
    fn cased() -> ModelFacts {
        ModelFacts {
            mesh_groups: vec!["default_@0_#&base".into(), "default_@1_#&glow".into()],
            mesh_group_masks: vec![3, 2, 3, 2, 3, 2, 3],
            default_mesh_group_mask: Some(1),
            lod_meshes: vec![2, 2, 2, 1],
            lod_switch_distances: vec![0., 12., 30., 50.],
            meshes: vec![mesh(1, "m/case.vmat", 10.), mesh(1, "m/glow.vmat", 10.2), mesh(2, "m/case.vmat", 10.), mesh(2, "m/glow.vmat", 10.2), mesh(4, "m/case.vmat", 10.), mesh(4, "m/glow.vmat", 10.2), mesh(8, "m/case.vmat", 10.)],
            materials: vec!["m/case.vmat".into(), "m/glow.vmat".into()],
            ..ModelFacts::default()
        }
    }

    #[test]
    fn a_shell_only_another_choice_of_the_object_s_group_shows_is_found() {
        let shells = glow_shells(&cased()).unwrap();
        assert_eq!(shells.levels, vec![(0, Some(1)), (2, Some(3)), (4, Some(5)), (6, None)]);
        assert_eq!(shells.material, "m/glow.vmat");
        assert_eq!((shells.group.as_str(), shells.choices.clone()), ("default", vec!["glow".to_owned()]));
        assert_eq!(shells.shells(), BTreeSet::from([1, 3, 5]));
        assert_eq!(kept_line(&cased()).unwrap(), "Its glow: a shell made from your object, in the game's own material, shown when the game shows it");
        // Opened only by its switch.
        assert!(!opens(&cased(), &Switches::OFF) && opens(&cased(), &Switches { body_group_shells: true, ..Switches::OFF }));
        // The object's bounds and materials without the shells.
        assert_eq!(cased().body_bounds(), Some(Bounds { min: [-10.; 3], max: [10.; 3] }));
        assert_eq!(cased().body_materials(), vec!["m/case.vmat".to_owned()]);
    }

    #[test]
    fn states_parts_outlines_and_other_meshes_are_not_shells() {
        // Two states of one group (each choice its own mesh): the second never shows with the first.
        let states = ModelFacts { mesh_group_masks: vec![1, 2, 1, 2, 1, 2, 1], ..cased() };
        assert_eq!(glow_shells(&states), None);
        // Both meshes shown by the default choice (parts that show together).
        assert_eq!(glow_shells(&ModelFacts { mesh_group_masks: vec![3, 3, 3, 3, 3, 3, 3], ..cased() }), None);
        // A shell of another body group.
        let other_group = ModelFacts { mesh_groups: vec!["default_@0_#&base".into(), "extra_@0_#&glow".into()], ..cased() };
        assert_eq!(glow_shells(&other_group), None);
        // Bound to other bones than the object.
        assert_eq!(glow_shells(&ModelFacts { mixed_mesh_bindings: true, ..cased() }), None);
        // A shell drawing with the object's own material, or with two materials.
        let mut same = cased();
        same.meshes[3].materials = vec!["m/case.vmat".into()];
        assert_eq!(glow_shells(&same), None);
        let mut two = cased();
        two.meshes[1].materials.push("m/other.vmat".into());
        assert_eq!(glow_shells(&two), None);
        // A shell inside the object, or of exactly its size.
        let mut inside = cased();
        inside.meshes[1].bounds = Some(Bounds { min: [-9.; 3], max: [9.; 3] });
        assert_eq!(glow_shells(&inside), None);
        let mut equal = cased();
        equal.meshes[1].bounds = Some(Bounds { min: [-10.; 3], max: [10.; 3] });
        assert_eq!(glow_shells(&equal), None);
        // Three meshes in a level.
        let mut three = cased();
        three.meshes.push(mesh(1, "m/glow.vmat", 10.4));
        three.mesh_group_masks.push(2);
        three.lod_meshes = vec![3, 2, 2, 1];
        assert_eq!(glow_shells(&three), None);
        // No body groups, or no level of detail.
        assert_eq!(glow_shells(&ModelFacts { mesh_groups: vec![], ..cased() }), None);
        assert_eq!(glow_shells(&ModelFacts { lod_meshes: vec![], lod_switch_distances: vec![], ..cased() }), None);
        // An outline (every level two meshes) is the outline rule's, not this one.
        let mut outline = cased();
        outline.meshes.truncate(6);
        outline.mesh_group_masks.truncate(6);
        outline.lod_meshes = vec![2, 2, 2];
        outline.lod_switch_distances = vec![0., 12., 30.];
        assert!(outline.shell_material().is_some());
        assert_eq!(glow_shells(&outline), None);
    }
}
