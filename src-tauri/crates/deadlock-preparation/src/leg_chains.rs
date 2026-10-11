//! Recognize whole deform chains, including parallel skin-bearing leg rigs.
//! A name alone never authorizes merging an unrelated helper into a limb.
use crate::algorithms::{self, Bone};
use crate::following::ancestor;
use std::collections::BTreeMap;

pub const POLICY: &str = "parallel-leg-skin-chains-v1";
const PARTS: [&str; 3] = ["thigh", "shin", "foot"];

pub fn role(name: &str) -> Option<String> {
    let name = name.rsplit(':').next().unwrap_or(name);
    let name = name.strip_prefix("DEF-").unwrap_or(name);
    if let Some(role) = algorithms::name_role(name) {
        if PARTS.iter().any(|p| role.starts_with(&format!("{p}_"))) {
            return Some(role.into());
        }
    }
    let compact: String = name.chars().filter(|c| c.is_alphanumeric())
        .map(|c| c.to_ascii_lowercase()).collect();
    for side in ["L", "R"] {
        for (part, base) in PARTS.into_iter().zip(["足", "ひざ", "足首"]) {
            for suffix in ["", "d"] {
                if compact == format!("{base}{suffix}{}", side.to_lowercase())
                    || compact == format!("{}{base}{suffix}", if side == "L" { "左" } else { "右" }) {
                    return Some(format!("{part}_{side}"));
                }
            }
        }
    }
    None
}
fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.iter().zip(b).map(|(a, b)| (a-b).powi(2)).sum::<f64>().sqrt()
}
fn chains<'a>(bones: &'a [Bone], side: &str, pelvis: &str) -> Vec<[&'a Bone; 3]> {
    let candidates: Vec<Vec<_>> = PARTS.iter().map(|part| bones.iter().filter(|b| {
        b.details.get("deform").and_then(|v| v.as_bool()) != Some(false)
            && role(&b.name).as_deref() == Some(format!("{part}_{side}").as_str())
            && b.position.iter().all(|v| v.is_finite())
            && ancestor(bones, pelvis, &b.name)
    }).collect()).collect();
    if candidates.iter().any(|v| v.len() > 16) { return vec![]; }
    let mut result = vec![];
    for a in &candidates[0] { for b in &candidates[1] { for c in &candidates[2] {
        if ancestor(bones, &a.name, &b.name) && ancestor(bones, &b.name, &c.name)
            && distance(a.position, b.position) > 1e-7 && distance(b.position, c.position) > 1e-7 {
            result.push([*a, *b, *c]);
        }
    } } }
    result
}

/// Resolve all three joints together; per-joint weight selection can mix rigs.
pub fn detect(bones: &[Bone], weighted: &[String], roles: &mut BTreeMap<String, String>) {
    let Some(pelvis) = roles.get("pelvis").cloned() else { return; };
    for side in ["L", "R"] {
        let candidates = chains(bones, side, &pelvis);
        let score = |c: &[&Bone; 3]| {
            let used = c.iter().filter(|b| weighted.contains(&b.name)).count();
            let deform_chain = usize::from(used > 0 && c.iter().all(|b| {
                let n = b.name.to_ascii_lowercase();
                n.ends_with("d.l") || n.ends_with("d.r") || n.ends_with('d')
            }));
            (used, deform_chain)
        };
        let Some(best) = candidates.iter().map(score).max() else { continue; };
        let winners: Vec<_> = candidates.iter().filter(|c| score(c) == best).collect();
        for part in PARTS { roles.remove(&format!("{part}_{side}")); }
        if winners.len() == 1 {
            for (part, bone) in PARTS.into_iter().zip(winners[0]) {
                roles.insert(format!("{part}_{side}"), bone.name.clone());
            }
        }
    }
}

/// Return alternative joint -> reviewed joint only when all three landmarks
/// and their actual ancestry agree. No model names, nearest-bone guesses or IK.
pub fn equivalents(bones: &[Bone], roles: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    let mut result = BTreeMap::new();
    let Some(pelvis) = roles.get("pelvis") else { return result; };
    for side in ["L", "R"] {
        let selected: Vec<_> = PARTS.iter().filter_map(|p| roles.get(&format!("{p}_{side}")))
            .filter_map(|n| bones.iter().find(|b| b.name == *n)).collect();
        if selected.len() != 3 || !ancestor(bones, &selected[0].name, &selected[1].name)
            || !ancestor(bones, &selected[1].name, &selected[2].name) { continue; }
        let length = distance(selected[0].position, selected[1].position)
            + distance(selected[1].position, selected[2].position);
        if !length.is_finite() || length < 1e-7 { continue; }
        for chain in chains(bones, side, pelvis) {
            if chain.iter().zip(&selected).all(|(a,b)| distance(a.position,b.position) <= length * 0.005) {
                for (a,b) in chain.iter().zip(&selected) {
                    if a.name != b.name { result.insert(a.name.clone(), b.name.clone()); }
                }
            }
        }
    }
    result
}
