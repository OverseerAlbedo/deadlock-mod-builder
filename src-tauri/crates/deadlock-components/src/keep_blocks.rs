//! The game's own compiled animation and physics blocks, kept byte for byte in
//! a rebuilt ability object (the kinds `ability_objects::Switches` names
//! `kept_animation`, `kept_collision` and their extensions).
//!
//! A rebuilt object is compiled from the decompiled game object with only its
//! render meshes changed. The compiler then writes the animation blocks again
//! from the decompiled animation files (`ANIM`, `ASEQ`, `AGRP`), and the
//! physics block again from the decompiled shapes (`PHYS`). Neither is the
//! game's exactly: quantised animation keys, bone bounds, decode positions and
//! hull data are made again. Here the rebuilt file's blocks of those kinds are
//! replaced by the game's payloads, unchanged; every other block keeps its
//! bytes and the block table is written again (offsets, sizes, alignment).
//!
//! Why that is consistent:
//! - the blocks are found through `CTRL` (`embedded_animation.*_data_block`,
//!   `embedded_physics.phys_data_block`), which names the rebuilt file's own
//!   ordinals: the kept payload goes into the slot `CTRL` already names, so no
//!   ordinal changes (the ordinals may differ from the game's file, which can
//!   hold more mesh blocks);
//! - the animation names its bones in its decode key (`AGRP
//!   m_decodeKey.m_boneArray`, names and parent positions). The bones must be
//!   the same by name and parent; unless binding by name is allowed
//!   (`Allowed::decode_by_name`), the decode key must also list them in the
//!   rebuilt skeleton's own order, position by position;
//! - the physics names its own bones (`m_boneNames`); unless physics on bones
//!   is allowed (`Allowed::physics_bones`) it must name none; when it is allowed
//!   and the physics names bones (or cannot be read on a model with bones), the
//!   rebuilt skeleton must list the game's bones at the same positions. What
//!   `m_boneParents` indexes is not settled, so as a guard each such position
//!   must name the same bone in the game's and the rebuilt skeleton;
//! - the kept blocks name no resource the rebuilt reference table lacks.
use crate::ability_objects::flatten;
use deadlock_resource::{Annotation, Kind, Value, binary, container};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// The animation blocks, in the order they are reported.
pub const ANIMATION: [&str; 3] = ["ANIM", "ASEQ", "AGRP"];
pub const PHYSICS: &str = "PHYS";
/// The CTRL row naming each kept kind's block.
const CTRL_ROWS: [(&str, &str); 4] = [
    ("ANIM", ".embedded_animation.anim_data_block"),
    ("ASEQ", ".embedded_animation.seqgroup_data_block"),
    ("AGRP", ".embedded_animation.group_data_block"),
    ("PHYS", ".embedded_physics.phys_data_block"),
];

/// One kept block: its name, size and SHA-256 (receipts record them).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeptBlock {
    pub name: String,
    pub size: usize,
    pub sha256: String,
}

/// What a kept block may hold beyond the proven kinds (each its own switch).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Allowed {
    /// The decode key may list the bones in another order than the rebuilt skeleton.
    pub decode_by_name: bool,
    /// The physics may name bones.
    pub physics_bones: bool,
    /// The physics may hold cloth (`m_pFeModel`), on a rebuilt skeleton that is the game's
    /// bone by bone (name, position, parent) with the same cloth flags on every bone.
    pub cloth: bool,
    /// With `physics_bones`: the rebuilt skeleton may list the bones in another order than the
    /// game's (`physics_reorder_problems`).
    pub physics_reordered: bool,
    /// With `cloth`: the cloth is made from a proxy mesh, whose bones the animation decode key does
    /// not list (`decode_key_without_proxy`).
    pub cloth_proxy: bool,
}
/// The bone flags the compiler sets on a bone the cloth moves (procedural and cloth-driven),
/// as `deadlock_resource::cloth::MOTION_MASK`.
pub const CLOTH_FLAGS: i128 = deadlock_resource::cloth::MOTION_MASK;
/// The cloth's control names (`m_pFeModel.m_CtrlName`) of a physics block; `None` without cloth.
pub fn cloth_controls(physics: &Value) -> Option<Vec<String>> {
    let fe = physics.get("m_pFeModel").filter(|f| !matches!(f.kind, Kind::Null))?;
    Some(strings(fe.get("m_CtrlName")))
}
/// Flags of the DATA model skeleton, in bone order.
fn bone_flags(raw: &[u8]) -> Vec<i128> {
    let data = first(raw, "DATA");
    ints(data.as_ref().and_then(|d| d.get("m_modelSkeleton")).and_then(|s| s.get("m_nFlag")))
}
/// Why the game's cloth (its physics block's `m_pFeModel`) is not valid on the rebuilt file
/// `ours` (empty when it is, and when the game's physics holds no cloth): the build and the
/// install check. The cloth binds itself to the skeleton by name: its controls are names
/// (`m_CtrlName`, with their hashes `m_CtrlHash`), and its own index fields count controls and
/// nodes, never model bones (`m_SkelParents` gives each control's parent as a control index:
/// on Rem's helper `[-1, -1, -1, 0, 1, 2]` for `tassel_0, ear_0_L, ear_0_R, tassel_end, …`,
/// where `tassel_0` is the model skeleton's bone 11). So the rebuilt skeleton must hold the
/// game's bones by name, each with the same parent by name, in any order; every control that is
/// a bone of the game's skeleton must be one of the rebuilt one, and every control that is not
/// (a node the cloth only simulates) must not be; and every bone must carry the same cloth flags
/// (`CLOTH_FLAGS`), so that the game drives the same bones from the kept cloth.
pub fn cloth_problems(game: &[u8], ours: &[u8]) -> Vec<String> {
    let Some(controls) = first(game, PHYSICS).as_ref().and_then(cloth_controls) else { return vec![] };
    let mut out = vec![];
    let ((names, parents), (game_names, game_parents)) = (skeleton(ours), skeleton(game));
    let sorted = |v: &[String]| {
        let mut v = v.to_vec();
        v.sort();
        v
    };
    if sorted(&names) != sorted(&game_names) {
        out.push(format!("the physics holds cloth, and the rebuilt skeleton has {} bones where the game's has {} (not the same names)", names.len(), game_names.len()));
    }
    let (game_flags, flags) = (bone_flags(game), bone_flags(ours));
    if game_flags.len() != game_names.len() || flags.len() != names.len() {
        out.push("the physics holds cloth, and the bone flags of a skeleton cannot be read".to_owned());
    }
    for (i, name) in game_names.iter().enumerate() {
        let Some(at) = names.iter().position(|n| n == name) else { continue };
        if parents.get(at) != game_parents.get(i) {
            out.push(format!("the physics holds cloth, and bone {name} hangs from {:?} in the rebuilt skeleton, from {:?} in the game's", parents.get(at), game_parents.get(i)));
        }
        if let (Some(g), Some(o)) = (game_flags.get(i), flags.get(at)) {
            if g & CLOTH_FLAGS != o & CLOTH_FLAGS {
                out.push(format!("the physics holds cloth, and bone {name} has cloth flags {:#x} in the rebuilt skeleton, {:#x} in the game's", o & CLOTH_FLAGS, g & CLOTH_FLAGS));
            }
        }
    }
    for control in &controls {
        match (game_names.contains(control), names.contains(control)) {
            (true, false) => out.push(format!("cloth control {control} is not a bone of the rebuilt skeleton")),
            (false, true) => out.push(format!("cloth control {control} is a bone of the rebuilt skeleton, not of the game's")),
            _ => {}
        }
    }
    out
}

/// Why the game's physics bound to bones is not valid on the rebuilt file `ours` whose skeleton
/// lists the bones in another order (empty when it is): the rebuilt skeleton holds the game's
/// bones by name, each with the same parent by name; and every skeleton position the physics
/// block can name holds the same bone in both skeletons: the position of each bone the physics
/// names (`m_boneNames`), each index of that list read as a skeleton position, and each
/// `m_boneParents` value. Whether the block indexes the skeleton by position or binds by name,
/// it then reaches the same bones. A physics block that cannot be read is never allowed so.
pub fn physics_reorder_problems(game: &[u8], ours: &[u8]) -> Vec<String> {
    let Some(phys) = first(game, PHYSICS) else { return vec!["the game's physics block cannot be read".to_owned()] };
    let mut out = vec![];
    let ((names, parents), (game_names, game_parents)) = (skeleton(ours), skeleton(game));
    let sorted = |v: &[String]| {
        let mut v = v.to_vec();
        v.sort();
        v
    };
    if sorted(&names) != sorted(&game_names) {
        out.push(format!("the rebuilt skeleton has {} bones where the game's has {} (not the same names)", names.len(), game_names.len()));
    }
    for (i, name) in game_names.iter().enumerate() {
        if let Some(at) = names.iter().position(|n| n == name) {
            if parents.get(at) != game_parents.get(i) {
                out.push(format!("bone {name} hangs from {:?} in the rebuilt skeleton, from {:?} in the game's", parents.get(at), game_parents.get(i)));
            }
        }
    }
    let bones = strings(phys.get("m_boneNames"));
    let mut reached: Vec<usize> = (0..bones.len()).collect();
    reached.extend(ints(phys.get("m_boneParents")).into_iter().filter_map(|p| usize::try_from(p).ok()));
    for bone in &bones {
        match (game_names.iter().position(|n| n == bone), names.iter().position(|n| n == bone)) {
            (Some(g), Some(o)) if g == o => reached.push(g),
            (g, o) => out.push(format!("physics bone {bone} is at position {g:?} in the game's skeleton and {o:?} in the rebuilt one")),
        }
    }
    reached.sort_unstable();
    reached.dedup();
    for p in reached {
        if game_names.get(p) != names.get(p) {
            out.push(format!("skeleton position {p}, which the physics can name, is {:?} in the game's skeleton and {:?} in the rebuilt one", game_names.get(p), names.get(p)));
        }
    }
    out
}

fn sha(b: &[u8]) -> String {
    Sha256::digest(b).iter().map(|x| format!("{x:02x}")).collect()
}
fn ordinals(raw: &[u8], name: &str) -> Vec<usize> {
    container::layout(raw).map(|l| l.blocks.iter().enumerate().filter(|(_, b)| b.name == name).map(|(i, _)| i).collect()).unwrap_or_default()
}
fn payload(raw: &[u8], ordinal: usize) -> Option<&[u8]> {
    let layout = container::layout(raw).ok()?;
    let block = layout.blocks.get(ordinal)?;
    raw.get(block.offset..block.offset + block.size)
}
fn first(raw: &[u8], name: &str) -> Option<Value> {
    container::read_blocks(raw, name).ok()?.into_iter().next().map(|d| d.root)
}
fn strings(v: Option<&Value>) -> Vec<String> {
    v.and_then(|x| x.array().ok()).unwrap_or(&[]).iter().map(|s| s.string().unwrap_or("?").to_owned()).collect()
}
fn ints(v: Option<&Value>) -> Vec<i128> {
    v.and_then(|x| x.array().ok()).unwrap_or(&[]).iter().map(|s| s.integer().unwrap_or(i128::MIN)).collect()
}
/// Names and parent names of the DATA model skeleton.
fn skeleton(raw: &[u8]) -> (Vec<String>, Vec<String>) {
    let data = first(raw, "DATA");
    let skel = data.as_ref().and_then(|d| d.get("m_modelSkeleton"));
    let names = strings(skel.and_then(|s| s.get("m_boneName")));
    let parents = ints(skel.and_then(|s| s.get("m_nParent")));
    let pnames = (0..names.len())
        .map(|i| match parents.get(i) {
            Some(p) if *p >= 0 => names.get(*p as usize).cloned().unwrap_or_default(),
            _ => String::new(),
        })
        .collect();
    (names, pnames)
}
/// Names and parent names of an AGRP decode key.
fn decode_key(raw: &[u8]) -> (Vec<String>, Vec<String>) {
    let Some(agrp) = first(raw, "AGRP") else { return (vec![], vec![]) };
    let bones = agrp.get("m_decodeKey").and_then(|k| k.get("m_boneArray")).and_then(|a| a.array().ok()).unwrap_or(&[]).to_vec();
    let names: Vec<String> = bones.iter().map(|b| b.get("m_name").and_then(|n| n.string().ok()).unwrap_or("?").to_owned()).collect();
    let pnames = bones
        .iter()
        .map(|b| match b.get("m_parent").and_then(|p| p.integer().ok()) {
            Some(p) if p >= 0 => names.get(p as usize).cloned().unwrap_or_default(),
            _ => String::new(),
        })
        .collect();
    (names, pnames)
}
fn resource_strings(v: &Value, out: &mut Vec<String>) {
    if matches!(v.annotation, Annotation::Resource | Annotation::ResourceName) {
        if let Kind::String(s) = &v.kind {
            if !s.is_empty() {
                out.push(s.clone());
            }
        }
    }
    match &v.kind {
        Kind::Object(rows) => rows.iter().for_each(|(_, x)| resource_strings(x, out)),
        Kind::Array(items) => items.iter().for_each(|x| resource_strings(x, out)),
        _ => {}
    }
}

/// The kinds of the ones this module keeps that a compiled file holds, in the
/// order `ANIM`, `ASEQ`, `AGRP`, `PHYS`.
pub fn kinds(raw: &[u8]) -> Vec<&'static str> {
    ANIMATION.into_iter().chain([PHYSICS]).filter(|k| !ordinals(raw, k).is_empty()).collect()
}

/// Why the game's blocks of `kinds` cannot go into the rebuilt file `ours`
/// (empty when they can).
pub fn problems(game: &[u8], ours: &[u8], kinds: &[&str], allowed: Allowed) -> Vec<String> {
    let mut out = vec![];
    if container::layout(game).is_err() {
        return vec!["the game's file is not a complete compiled resource".into()];
    }
    let Ok(layout) = container::layout(ours) else { return vec!["the rebuilt file is not a complete compiled resource".into()] };
    let ctrl = first(ours, "CTRL").map(|c| flatten(&c)).unwrap_or_default();
    for kind in kinds {
        if !ANIMATION.contains(kind) && *kind != PHYSICS {
            out.push(format!("{kind} is not a block that can be kept"));
            continue;
        }
        let (g, o) = (ordinals(game, kind).len(), ordinals(ours, kind).len());
        if g != 1 || o != 1 {
            out.push(format!("{kind}: {g} blocks in the game's file and {o} in the rebuilt one (one each expected)"));
            continue;
        }
        let row = CTRL_ROWS.iter().find(|(k, _)| k == kind).map(|(_, r)| *r).unwrap_or_default();
        let at = ctrl.get(row).and_then(|v| v.parse::<usize>().ok());
        if at.and_then(|i| layout.blocks.get(i)).map(|b| b.name.as_str()) != Some(*kind) {
            out.push(format!("the rebuilt file's control block does not name its {kind} block"));
        }
    }
    let (names, parents) = skeleton(ours);
    let (game_names, _) = skeleton(game);
    if kinds.contains(&"AGRP") {
        let (key, key_parents) = decode_key(game);
        for (n, p) in key.iter().zip(&key_parents) {
            match names.iter().position(|x| x == n) {
                None => out.push(format!("animation decode key: {n} is not a bone of the rebuilt skeleton")),
                Some(i) if &parents[i] != p => out.push(format!("animation decode key: {n} hangs from {p:?}, in the rebuilt skeleton from {:?}", parents[i])),
                _ => {}
            }
        }
        // A cloth made from a proxy mesh: the decode key lists every bone but the cloth's own, as the
        // game's decode key does (`proxy_left_out`).
        let proxy_left_out = allowed.cloth_proxy && {
            let (game_key, _) = decode_key(game);
            let missing = |all: &[String]| -> std::collections::BTreeSet<String> { all.iter().filter(|n| !game_key.contains(n)).cloned().collect() };
            let flags = bone_flags(game);
            let theirs = missing(&game_names);
            !theirs.is_empty() && theirs == missing(&names) && game_names.iter().zip(&flags).all(|(n, f)| !theirs.contains(n) || f & CLOTH_FLAGS == CLOTH_FLAGS && n.starts_with('$'))
        };
        if key.len() != names.len() && !proxy_left_out {
            out.push(format!("animation decode key: {} bones, the rebuilt skeleton {}", key.len(), names.len()));
        } else if !allowed.decode_by_name && key != names {
            out.push(format!("animation decode key: the bones are listed as {key:?}, the rebuilt skeleton as {names:?}"));
        }
    }
    if kinds.contains(&PHYSICS) {
        let phys = first(game, PHYSICS);
        let (bones, bone_parents) = (strings(phys.as_ref().and_then(|p| p.get("m_boneNames"))), ints(phys.as_ref().and_then(|p| p.get("m_boneParents"))));
        if !allowed.physics_bones && !bones.is_empty() {
            out.push(format!("the physics is bound to bones {bones:?}"));
        }
        // Physics bound to bones is kept only on a rebuilt skeleton that is the game's,
        // bone by bone, by name and position (the only case its test covers). A block the
        // reader refuses on a model with bones may be bound to any of them: the same.
        let unread = phys.is_none() && !(names.is_empty() && game_names.is_empty());
        if allowed.physics_bones && (!bones.is_empty() || unread) && names != game_names && !(allowed.physics_reordered && !unread && physics_reorder_problems(game, ours).is_empty()) {
            out.push(format!("the physics is bound to bones, and the rebuilt skeleton lists {names:?} where the game's lists {game_names:?}"));
        }
        // Cloth (`m_pFeModel`): refused unless allowed. When allowed, the rebuilt skeleton must be
        // the game's bone by bone: the same names at the same positions with the same parents
        // (whether a cloth field indexes the model skeleton by position is not settled, so the whole
        // list is required), and every bone the same cloth flags (`CLOTH_FLAGS`), so that the game
        // drives the same bones from the kept cloth. Control names that are no bone of the game's
        // skeleton (nodes the cloth only simulates) are no bone of the rebuilt one either, as the
        // skeletons are equal.
        if let Some(controls) = phys.as_ref().and_then(cloth_controls) {
            if !allowed.cloth {
                out.push(format!("the physics holds cloth on {} controls", controls.len()));
            } else {
                out.extend(cloth_problems(game, ours));
            }
        }
        for (i, bone) in bones.iter().enumerate() {
            if !names.contains(bone) {
                out.push(format!("physics bone {bone} is not a bone of the rebuilt skeleton"));
            }
            if let Some(p) = bone_parents.get(i).filter(|p| **p >= 0).map(|p| *p as usize) {
                if game_names.get(p) != names.get(p) {
                    out.push(format!("physics bone {bone}: parent position {p} is {:?} in the game's skeleton and {:?} in the rebuilt one", game_names.get(p), names.get(p)));
                }
            }
        }
    }
    let references = |raw: &[u8]| -> Vec<String> {
        ordinals(raw, "RERL")
            .first()
            .and_then(|i| payload(raw, *i))
            .and_then(|p| crate::texture_recolor::references(p).ok())
            .unwrap_or_default()
            .into_iter()
            .map(|(_, n)| n.to_ascii_lowercase())
            .collect()
    };
    let rerl = references(ours);
    for kind in kinds {
        for i in ordinals(game, kind) {
            match payload(game, i).map(binary::decode) {
                Some(Ok(doc)) => {
                    let mut named = vec![];
                    resource_strings(&doc.root, &mut named);
                    for r in named.into_iter().filter(|r| !rerl.contains(&r.to_ascii_lowercase())) {
                        out.push(format!("{kind} names {r}, which the rebuilt reference table lacks"));
                    }
                }
                Some(Err(_)) => {
                    // A block the reader refuses is only kept, never read: whatever it names is in
                    // the game file's own reference table, so every reference there other than the
                    // materials the build replaces must be in the rebuilt one.
                    for r in references(game).into_iter().filter(|r| !r.trim_end_matches("_c").ends_with(".vmat") && !rerl.contains(r)) {
                        out.push(format!("{kind} cannot be read, and the game's reference {r} is missing from the rebuilt reference table"));
                    }
                    if *kind == PHYSICS && !allowed.physics_bones && !(names.is_empty() && game_names.is_empty()) {
                        out.push("the game's physics block cannot be read to see which bones it is bound to".to_owned());
                    }
                }
                None => out.push(format!("the game's {kind} block could not be read")),
            }
        }
    }
    out
}

/// The rebuilt file with the game's blocks of `kinds` in place of its own, and
/// what was kept; an error naming every problem when they cannot be kept.
pub fn keep(game: &[u8], ours: &[u8], kinds: &[&str], allowed: Allowed) -> std::result::Result<(Vec<u8>, Vec<KeptBlock>), String> {
    let problems = problems(game, ours, kinds, allowed);
    if !problems.is_empty() {
        return Err(problems.join("; "));
    }
    let mut replacements = BTreeMap::new();
    let mut kept = vec![];
    for kind in kinds {
        let bytes = payload(game, ordinals(game, kind)[0]).ok_or("the game's block could not be read")?.to_vec();
        kept.push(KeptBlock { name: (*kind).to_owned(), size: bytes.len(), sha256: sha(&bytes) });
        replacements.insert(ordinals(ours, kind)[0], bytes);
    }
    let out = container::rebuild_ordinals(ours, &replacements).map_err(|e| e.to_string())?;
    let left = same_bytes(game, &out, kinds);
    if !left.is_empty() {
        return Err(left.join("; "));
    }
    Ok((out, kept))
}

/// The kept kinds whose payload in `ours` is not the game's byte for byte
/// (the read-back and the install check). Empty when every one is.
pub fn same_bytes(game: &[u8], ours: &[u8], kinds: &[&str]) -> Vec<String> {
    kinds
        .iter()
        .filter_map(|kind| {
            let (g, o) = (ordinals(game, kind), ordinals(ours, kind));
            let same = g.len() == 1 && o.len() == 1 && payload(game, g[0]).is_some() && payload(game, g[0]) == payload(ours, o[0]);
            (!same).then(|| format!("{kind} is not the game's block byte for byte"))
        })
        .collect()
}
/// `same_bytes`, and each kept block's digest as the receipt recorded it.
pub fn same_as_kept(game: &[u8], ours: &[u8], kept: &[KeptBlock]) -> Vec<String> {
    let kinds: Vec<&str> = kept.iter().map(|k| k.name.as_str()).collect();
    let mut out = same_bytes(game, ours, &kinds);
    for block in kept {
        let now = ordinals(ours, &block.name).first().and_then(|i| payload(ours, *i)).map(sha);
        if now.as_deref() != Some(block.sha256.as_str()) && !out.iter().any(|m| m.starts_with(&format!("{} ", block.name))) {
            out.push(format!("{} is not the block the build kept", block.name));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const GUID: [u8; 16] = [0x7c, 0x16, 0x12, 0x74, 0xe9, 0x06, 0x98, 0x46, 0xaf, 0xf2, 0xe6, 0x3e, 0xb5, 0x90, 0x37, 0xe7];

    fn v(kind: Kind) -> Value {
        Value::new(kind)
    }
    fn s(t: &str) -> Value {
        v(Kind::String(t.into()))
    }
    fn i(n: i64) -> Value {
        v(Kind::I64(n))
    }
    fn obj(rows: Vec<(&str, Value)>) -> Value {
        v(Kind::Object(rows.into_iter().map(|(k, x)| (k.to_owned(), x)).collect()))
    }
    fn arr(items: Vec<Value>) -> Value {
        v(Kind::Array(items))
    }
    fn kv3(root: &Value) -> Vec<u8> {
        binary::encode(root, GUID).unwrap()
    }
    /// A compiled resource holding `blocks` in order (16-byte aligned payloads).
    fn container_of<N: AsRef<str>>(blocks: &[(N, Vec<u8>)]) -> Vec<u8> {
        let mut out = vec![0u8; 16 + 12 * blocks.len()];
        out[4..6].copy_from_slice(&12u16.to_le_bytes());
        out[8..12].copy_from_slice(&8u32.to_le_bytes());
        out[12..16].copy_from_slice(&(blocks.len() as u32).to_le_bytes());
        for (n, (name, bytes)) in blocks.iter().enumerate() {
            out.resize((out.len() + 15) & !15, 0);
            let start = out.len();
            out.extend_from_slice(bytes);
            let entry = 16 + 12 * n;
            out[entry..entry + 4].copy_from_slice(name.as_ref().as_bytes());
            out[entry + 4..entry + 8].copy_from_slice(&((start - entry - 4) as u32).to_le_bytes());
            out[entry + 8..entry + 12].copy_from_slice(&(bytes.len() as u32).to_le_bytes());
        }
        let size = out.len() as u32;
        out[..4].copy_from_slice(&size.to_le_bytes());
        out
    }
    /// A DATA block with a model skeleton (names and parent positions).
    fn data(bones: &[(&str, i64)]) -> Vec<u8> {
        kv3(&obj(vec![(
            "m_modelSkeleton",
            obj(vec![("m_boneName", arr(bones.iter().map(|(n, _)| s(n)).collect())), ("m_nParent", arr(bones.iter().map(|(_, p)| i(*p)).collect()))]),
        )]))
    }
    fn agrp(bones: &[(&str, i64)], tag: i64) -> Vec<u8> {
        kv3(&obj(vec![
            ("m_decodeKey", obj(vec![("m_boneArray", arr(bones.iter().map(|(n, p)| obj(vec![("m_name", s(n)), ("m_parent", i(*p))])).collect()))])),
            ("m_tag", i(tag)),
        ]))
    }
    fn plain(tag: i64) -> Vec<u8> {
        kv3(&obj(vec![("m_tag", i(tag))]))
    }
    fn phys(bones: &[&str], tag: i64) -> Vec<u8> {
        kv3(&obj(vec![("m_boneNames", arr(bones.iter().map(|b| s(b)).collect())), ("m_boneParents", arr(bones.iter().map(|_| i(0)).collect())), ("m_tag", i(tag))]))
    }
    fn ctrl(at: &[(&str, i64)]) -> Vec<u8> {
        let mut animation = vec![];
        let mut physics = vec![];
        for (row, n) in at {
            match *row {
                "PHYS" => physics.push(("phys_data_block", i(*n))),
                "ANIM" => animation.push(("anim_data_block", i(*n))),
                "ASEQ" => animation.push(("seqgroup_data_block", i(*n))),
                "AGRP" => animation.push(("group_data_block", i(*n))),
                _ => {}
            }
        }
        kv3(&obj(vec![("embedded_animation", obj(animation)), ("embedded_physics", obj(physics))]))
    }
    fn rerl(names: &[&str]) -> Vec<u8> {
        crate::texture_recolor::reference_table(&names.iter().enumerate().map(|(n, x)| (n as u64 + 1, (*x).to_owned())).collect::<Vec<_>>())
    }
    const BONES: [(&str, i64); 3] = [("base", -1), ("arm", 0), ("tip", 1)];
    /// A model with MVTX, ANIM, ASEQ, AGRP, PHYS, CTRL, RERL and DATA; `tag`
    /// tells the game's payloads from the rebuilt ones.
    fn model(tag: i64, key: &[(&str, i64)], physics: &[&str]) -> Vec<u8> {
        container_of(&[
            ("MVTX", vec![tag as u8; 37]),
            ("ANIM", plain(tag)),
            ("ASEQ", plain(tag + 1)),
            ("AGRP", agrp(key, tag)),
            ("PHYS", phys(physics, tag)),
            ("CTRL", ctrl(&[("ANIM", 1), ("ASEQ", 2), ("AGRP", 3), ("PHYS", 4)])),
            ("RERL", rerl(&["materials/a.vmat"])),
            ("DATA", data(&BONES)),
        ])
    }
    fn blocks(raw: &[u8]) -> Vec<(String, Vec<u8>)> {
        container::layout(raw).unwrap().blocks.iter().map(|b| (b.name.clone(), raw[b.offset..b.offset + b.size].to_vec())).collect()
    }

    #[test]
    fn the_kinds_are_the_kept_blocks_a_file_holds() {
        assert_eq!(kinds(&model(1, &BONES, &[])), ["ANIM", "ASEQ", "AGRP", "PHYS"]);
        let only = container_of(&[("MVTX", vec![1; 8]), ("PHYS", phys(&[], 1)), ("DATA", data(&BONES))]);
        assert_eq!(kinds(&only), ["PHYS"]);
        assert!(kinds(&container_of(&[("DATA", data(&BONES))])).is_empty());
        assert!(kinds(b"not a resource").is_empty());
    }

    #[test]
    fn kept_blocks_are_the_game_payloads_and_nothing_else_moves() {
        let (game, ours) = (model(10, &BONES, &[]), model(50, &BONES, &[]));
        let all = ["ANIM", "ASEQ", "AGRP", "PHYS"];
        assert_eq!(same_bytes(&game, &ours, &all).len(), 4, "before: none is the game's");
        assert!(problems(&game, &ours, &all, Allowed::default()).is_empty());
        let (out, kept) = keep(&game, &ours, &all, Allowed::default()).unwrap();
        assert!(same_bytes(&game, &out, &all).is_empty());
        assert!(same_as_kept(&game, &out, &kept).is_empty());
        assert_eq!(kept.iter().map(|k| k.name.as_str()).collect::<Vec<_>>(), all);
        let (g, o, b) = (blocks(&game), blocks(&ours), blocks(&out));
        assert_eq!(b.iter().map(|x| x.0.as_str()).collect::<Vec<_>>(), ["MVTX", "ANIM", "ASEQ", "AGRP", "PHYS", "CTRL", "RERL", "DATA"]);
        for (n, (name, bytes)) in b.iter().enumerate() {
            if all.contains(&name.as_str()) {
                assert_eq!(bytes, &g[n].1, "{name} is the game's");
                assert_eq!(kept.iter().find(|k| &k.name == name).unwrap().sha256, sha(bytes));
            } else {
                assert_eq!(bytes, &o[n].1, "{name} keeps its bytes");
            }
        }
        // A valid block table that the project's reader reads.
        let layout = container::layout(&out).unwrap();
        assert_eq!(layout.declared_size, out.len());
        for block in &layout.blocks {
            assert_eq!(block.offset % 16, 0, "{} is aligned", block.name);
            if block.name != "MVTX" && block.name != "RERL" {
                binary::decode(&out[block.offset..block.offset + block.size]).unwrap();
            }
        }
        assert_eq!(skeleton(&out).0, ["base", "arm", "tip"]);
        assert_eq!(crate::texture_recolor::references(payload(&out, 6).unwrap()).unwrap()[0].1, "materials/a.vmat");
        // Only some kinds: the others stay the rebuilt file's.
        let (out, _) = keep(&game, &ours, &["PHYS"], Allowed::default()).unwrap();
        assert_eq!(blocks(&out)[4].1, g[4].1);
        assert_eq!(blocks(&out)[3].1, o[3].1);
        assert_eq!(same_bytes(&game, &out, &["AGRP"]).len(), 1);
    }

    #[test]
    fn a_tampered_kept_block_is_found_by_the_install_check() {
        let (game, ours) = (model(10, &BONES, &[]), model(50, &BONES, &[]));
        let (out, kept) = keep(&game, &ours, &["ANIM", "ASEQ", "AGRP"], Allowed::default()).unwrap();
        let tampered = container::rebuild_ordinals(&out, &BTreeMap::from([(2, plain(99))])).unwrap();
        assert_eq!(same_bytes(&game, &tampered, &["ANIM", "ASEQ", "AGRP"]), ["ASEQ is not the game's block byte for byte"]);
        assert_eq!(same_as_kept(&game, &tampered, &kept), ["ASEQ is not the game's block byte for byte"]);
        // The game's file changed since: the receipt's digest no longer matches either.
        let newer = container::rebuild_ordinals(&game, &BTreeMap::from([(1, plain(77))])).unwrap();
        let found = same_as_kept(&newer, &out, &kept);
        assert_eq!(found, ["ANIM is not the game's block byte for byte"]);
        // A block gone from the built file.
        let gone = container_of(&blocks(&out).into_iter().filter(|(n, _)| n != "AGRP").collect::<Vec<_>>());
        assert_eq!(same_as_kept(&game, &gone, &kept), ["AGRP is not the game's block byte for byte"]);
    }

    #[test]
    fn every_precondition_refuses_on_its_own() {
        let game = model(10, &BONES, &[]);
        let ok = model(50, &BONES, &[]);
        let refused = |game: &[u8], ours: &[u8], kinds: &[&str], allowed: Allowed, part: &str| {
            let found = problems(game, ours, kinds, allowed);
            assert!(found.iter().any(|p| p.contains(part)), "{part}: {found:?}");
            assert!(keep(game, ours, kinds, allowed).unwrap_err().contains(part));
        };
        // Not a resource.
        refused(&game, b"nothing", &["PHYS"], Allowed::default(), "not a complete compiled resource");
        refused(b"nothing", &ok, &["PHYS"], Allowed::default(), "not a complete compiled resource");
        // A block that is not one of the kept kinds.
        refused(&game, &ok, &["MVTX"], Allowed::default(), "not a block that can be kept");
        // One block of each kept kind in each file.
        let mut two = blocks(&ok);
        two[0] = ("PHYS".into(), phys(&[], 3));
        refused(&game, &container_of(&two), &["PHYS"], Allowed::default(), "PHYS: 1 blocks in the game's file and 2 in the rebuilt one");
        let none = container_of(&[("MVTX", vec![1; 8]), ("CTRL", ctrl(&[])), ("DATA", data(&BONES))]);
        refused(&game, &none, &["ANIM"], Allowed::default(), "ANIM: 1 blocks in the game's file and 0 in the rebuilt one");
        // The rebuilt control block names another block.
        let mut moved = blocks(&ok);
        moved[5].1 = ctrl(&[("ANIM", 1), ("ASEQ", 2), ("AGRP", 0), ("PHYS", 4)]);
        let moved = container_of(&moved);
        refused(&game, &moved, &["AGRP"], Allowed::default(), "control block does not name its AGRP block");
        assert!(problems(&game, &moved, &["ANIM", "ASEQ", "PHYS"], Allowed::default()).is_empty(), "the other rows are fine");
        // The decode key: a bone the rebuilt skeleton lacks, another parent, another count.
        refused(&model(10, &[("base", -1), ("arm", 0), ("hand", 1)], &[]), &ok, &["AGRP"], Allowed::default(), "hand is not a bone of the rebuilt skeleton");
        refused(&model(10, &[("base", -1), ("arm", 0), ("tip", 0)], &[]), &ok, &["AGRP"], Allowed::default(), "tip hangs from \"base\"");
        refused(&model(10, &[("base", -1), ("arm", 0)], &[]), &ok, &["AGRP"], Allowed::default(), "2 bones, the rebuilt skeleton 3");
        // Another order: refused unless binding by name is allowed.
        let reordered = model(10, &[("base", -1), ("tip", 2), ("arm", 0)], &[]);
        refused(&reordered, &ok, &["AGRP"], Allowed::default(), "the bones are listed as");
        assert!(problems(&reordered, &ok, &["AGRP"], Allowed { decode_by_name: true, ..Allowed::default() }).is_empty());
        // Physics on bones: refused unless allowed; a bone the rebuilt skeleton lacks always.
        let on_bones = model(10, &BONES, &["arm"]);
        refused(&on_bones, &ok, &["PHYS"], Allowed::default(), "the physics is bound to bones");
        assert!(problems(&on_bones, &ok, &["PHYS"], Allowed { physics_bones: true, ..Allowed::default() }).is_empty());
        refused(&model(10, &BONES, &["wing"]), &ok, &["PHYS"], Allowed { physics_bones: true, ..Allowed::default() }, "physics bone wing is not a bone of the rebuilt skeleton");
        // The guard on what a physics parent position names.
        // The game's skeleton names another bone at that position.
        let mut other = blocks(&on_bones);
        other[7].1 = data(&[("root", -1), ("arm", 0), ("tip", 1)]);
        refused(&container_of(&other), &ok, &["PHYS"], Allowed { physics_bones: true, ..Allowed::default() }, "parent position 0");
        // Physics on bones: the rebuilt skeleton must be the game's, bone by bone, by name
        // and position, even where the physics' own bones and parents agree.
        let on_bones = |physics: &[&str], skeleton: &[(&str, i64)]| {
            let mut b = blocks(&model(10, &BONES, physics));
            b[7].1 = data(skeleton);
            container_of(&b)
        };
        let swapped = [("base", -1), ("tip", 0), ("arm", 0)];
        let longer = [("base", -1), ("arm", 0), ("tip", 1), ("extra", 0)];
        let allowed = Allowed { physics_bones: true, ..Allowed::default() };
        for skeleton in [&swapped[..], &longer[..]] {
            refused(&on_bones(&["base"], skeleton), &ok, &["PHYS"], allowed, "the rebuilt skeleton lists");
        }
        assert!(problems(&on_bones(&["base"], &BONES), &ok, &["PHYS"], allowed).is_empty(), "the same skeleton: kept");
        // Without bones in the physics, or without physics on bones allowed, nothing changes.
        assert!(problems(&on_bones(&[], &swapped), &ok, &["PHYS"], allowed).is_empty());
        assert!(problems(&on_bones(&[], &swapped), &ok, &["PHYS"], Allowed::default()).is_empty());
        assert!(!problems(&on_bones(&["base"], &swapped), &ok, &["PHYS"], Allowed::default()).iter().any(|p| p.contains("the rebuilt skeleton lists")));
        // A resource a kept block names that the rebuilt reference table lacks.
        let resource = |name: &str| Value { annotation: Annotation::Resource, kind: Kind::String(name.into()) };
        let mut with = blocks(&game);
        with[1].1 = kv3(&obj(vec![("m_tag", i(1)), ("m_hModel", resource("models/other.vmdl"))]));
        refused(&container_of(&with), &ok, &["ANIM"], Allowed::default(), "ANIM names models/other.vmdl");
        // A name the table has is fine (case aside).
        let mut with = blocks(&game);
        with[1].1 = kv3(&obj(vec![("m_tag", i(1)), ("m_hMaterial", resource("MATERIALS/A.VMAT"))]));
        assert!(problems(&container_of(&with), &ok, &["ANIM"], Allowed::default()).is_empty());
        // A game block the reader refuses is only kept: what it names must be in the game's
        // reference table, so that table (less the replaced materials) must be in the rebuilt one.
        let mut broken = blocks(&game);
        broken[2].1 = vec![1, 2, 3];
        assert!(problems(&container_of(&broken), &ok, &["ASEQ"], Allowed::default()).is_empty(), "only a material: kept");
        let (out, _) = keep(&container_of(&broken), &ok, &["ASEQ"], Allowed::default()).unwrap();
        assert_eq!(blocks(&out)[2].1, vec![1, 2, 3]);
        broken[6].1 = rerl(&["materials/a.vmat", "models/other.vnmgraph"]);
        refused(&container_of(&broken), &ok, &["ASEQ"], Allowed::default(), "ASEQ cannot be read, and the game's reference models/other.vnmgraph is missing");
        // An unreadable physics block on a model with bones: what it is bound to cannot be checked.
        let mut physics = blocks(&game);
        physics[4].1 = vec![4, 5, 6];
        refused(&container_of(&physics), &ok, &["PHYS"], Allowed::default(), "cannot be read to see which bones");
        assert!(problems(&container_of(&physics), &ok, &["PHYS"], Allowed { physics_bones: true, ..Allowed::default() }).is_empty());
        let mut unread_other = physics.clone();
        unread_other[7].1 = data(&[("base", -1), ("tip", 0), ("arm", 0)]);
        refused(&container_of(&unread_other), &ok, &["PHYS"], Allowed { physics_bones: true, ..Allowed::default() }, "the rebuilt skeleton lists");
        let bare = |raw: &[u8]| container_of(&blocks(raw).into_iter().map(|(n, b)| if n == "DATA" { (n, data(&[])) } else { (n, b) }).collect::<Vec<_>>());
        assert!(problems(&bare(&container_of(&physics)), &bare(&ok), &["PHYS"], Allowed::default()).is_empty(), "no bones: nothing to be bound to");
    }

    /// A DATA block whose model skeleton has flags too.
    fn data_flags(bones: &[(&str, i64, i64)]) -> Vec<u8> {
        kv3(&obj(vec![(
            "m_modelSkeleton",
            obj(vec![
                ("m_boneName", arr(bones.iter().map(|(n, _, _)| s(n)).collect())),
                ("m_nParent", arr(bones.iter().map(|(_, p, _)| i(*p)).collect())),
                ("m_nFlag", arr(bones.iter().map(|(_, _, f)| i(*f)).collect())),
            ]),
        )]))
    }
    /// A physics block holding only cloth on `controls`.
    fn cloth(controls: &[&str], tag: i64) -> Vec<u8> {
        kv3(&obj(vec![("m_boneNames", arr(vec![])), ("m_pFeModel", obj(vec![("m_CtrlName", arr(controls.iter().map(|c| s(c)).collect())), ("m_nNodeCount", i(controls.len() as i64))])), ("m_tag", i(tag))]))
    }
    const CLOTHED: i64 = 0x8 | 0x400000;
    /// A model with cloth: `skeleton` (name, parent, flags) and the cloth's `controls`.
    fn clothed(tag: i64, skeleton: &[(&str, i64, i64)], controls: &[&str]) -> Vec<u8> {
        let mut b = blocks(&model(tag, &BONES, &[]));
        b[4].1 = cloth(controls, tag);
        b[7].1 = data_flags(skeleton);
        container_of(&b)
    }

    #[test]
    fn the_game_s_cloth_is_kept_only_on_its_own_bones_by_name() {
        let skeleton = [("base", -1, 0), ("arm", 0, 0), ("tip", 1, CLOTHED)];
        let game = clothed(10, &skeleton, &["arm", "tip", "$node_0"]);
        let all = Allowed { cloth: true, ..Allowed::default() };
        // Refused unless allowed, on its own.
        let found = problems(&game, &clothed(50, &skeleton, &["arm", "tip"]), &["PHYS"], Allowed::default());
        assert_eq!(found, ["the physics holds cloth on 3 controls"]);
        // The same bones, parents and cloth flags: kept, byte for byte.
        let ours = clothed(50, &skeleton, &["arm", "tip"]);
        assert!(problems(&game, &ours, &["PHYS"], all).is_empty());
        let (out, kept) = keep(&game, &ours, &["PHYS"], all).unwrap();
        assert!(same_as_kept(&game, &out, &kept).is_empty() && cloth_problems(&game, &out).is_empty());
        // In another order (the compiler may store the bones otherwise): bound by name, kept.
        let reordered = clothed(50, &[("base", -1, 0), ("tip", 2, CLOTHED), ("arm", 0, 0)], &[]);
        assert!(problems(&game, &reordered, &["PHYS"], all).is_empty());
        // Every precondition on its own.
        let refused = |ours: &[u8], part: &str| {
            let found = problems(&game, ours, &["PHYS"], all);
            assert!(found.iter().any(|p| p.contains(part)), "{part}: {found:?}");
            assert!(cloth_problems(&game, ours).iter().any(|p| p.contains(part)), "the install check too: {part}");
        };
        refused(&clothed(50, &[("base", -1, 0), ("arm", 0, 0)], &[]), "cloth control tip is not a bone of the rebuilt skeleton");
        refused(&clothed(50, &[("base", -1, 0), ("arm", 0, 0), ("tip", 0, CLOTHED)], &[]), "bone tip hangs from Some(\"base\") in the rebuilt skeleton, from Some(\"arm\")");
        refused(&clothed(50, &[("base", -1, 0), ("arm", 0, 0), ("tip", 1, 0)], &[]), "bone tip has cloth flags 0x0 in the rebuilt skeleton, 0x400008 in the game's");
        refused(&clothed(50, &[("base", -1, 0), ("arm", 0, 0x8), ("tip", 1, CLOTHED)], &[]), "bone arm has cloth flags 0x8");
        refused(&clothed(50, &[("base", -1, 0), ("arm", 0, 0), ("tip", 1, CLOTHED), ("$node_0", 2, 0)], &[]), "cloth control $node_0 is a bone of the rebuilt skeleton, not of the game's");
        refused(&clothed(50, &[("base", -1, 0), ("arm", 0, 0), ("tip", 1, CLOTHED), ("extra", 0, 0)], &[]), "not the same names");
        // Flags other than the cloth's are not the cloth's business.
        assert!(problems(&game, &clothed(50, &[("base", -1, 0x40), ("arm", 0, 0x100), ("tip", 1, CLOTHED | 0x40)], &[]), &["PHYS"], all).is_empty());
        // Without cloth in the game's block nothing of this applies.
        assert!(cloth_problems(&model(10, &BONES, &[]), &ours).is_empty());
    }

    #[test]
    fn physics_on_bones_is_kept_on_a_reordered_skeleton_only_where_every_position_it_reaches_agrees() {
        let refused = |game: &[u8], ours: &[u8], kinds: &[&str], allowed: Allowed, part: &str| {
            let found = problems(game, ours, kinds, allowed);
            assert!(found.iter().any(|p| p.contains(part)), "{part}: {found:?}");
        };
        // The game lists a leaf last; the compiler writes each bone followed by its branch.
        let game_bones = [("root", -1), ("body", 0), ("tail", 1), ("ear", 1), ("tail_end", 2)];
        let rebuilt = [("root", -1), ("body", 0), ("tail", 1), ("tail_end", 2), ("ear", 1)];
        let file = |bones: &[(&str, i64)], physics: &[&str], tag: i64| container_of(&[("PHYS", phys(physics, tag)), ("CTRL", ctrl(&[("PHYS", 0)])), ("DATA", data(bones))]);
        let (game, ours) = (file(&game_bones, &["root"], 1), file(&rebuilt, &["root"], 2));
        let on_bones = Allowed { physics_bones: true, ..Allowed::default() };
        refused(&game, &ours, &["PHYS"], on_bones, "the rebuilt skeleton lists");
        let reordered = Allowed { physics_reordered: true, ..on_bones };
        assert!(physics_reorder_problems(&game, &ours).is_empty());
        assert!(problems(&game, &ours, &["PHYS"], reordered).is_empty());
        // A physics bone whose position moves: refused.
        let (game, ours) = (file(&game_bones, &["ear"], 1), file(&rebuilt, &["ear"], 2));
        assert!(physics_reorder_problems(&game, &ours).iter().any(|p| p.contains("physics bone ear is at position")));
        refused(&game, &ours, &["PHYS"], reordered, "the rebuilt skeleton lists");
        // A bone list index that reads as a moved position (the second physics bone, at index 3): refused.
        let (game, ours) = (file(&game_bones, &["root", "body", "tail", "tail_end"], 1), file(&rebuilt, &["root", "body", "tail", "tail_end"], 2));
        assert!(physics_reorder_problems(&game, &ours).iter().any(|p| p.contains("tail_end")));
        // Another parent, or other bones: refused.
        let moved = [("root", -1), ("body", 0), ("tail", 1), ("tail_end", 1), ("ear", 1)];
        assert!(physics_reorder_problems(&file(&game_bones, &["root"], 1), &file(&moved, &["root"], 2)).iter().any(|p| p.contains("hangs from")));
        let other = [("root", -1), ("body", 0), ("tail", 1), ("tail_end", 2), ("horn", 1)];
        assert!(physics_reorder_problems(&file(&game_bones, &["root"], 1), &file(&other, &["root"], 2)).iter().any(|p| p.contains("not the same names")));
        // Without the reorder allowance nothing changes for the existing kind.
        assert!(problems(&file(&game_bones, &["root"], 1), &file(&game_bones, &["root"], 2), &["PHYS"], on_bones).is_empty());
    }

    #[test]
    fn a_proxy_cloth_s_bones_may_be_missing_from_the_decode_key_exactly_as_in_the_game_s() {
        // The game's skeleton: three bones and two cloth bones made from a proxy mesh, which the
        // decode key leaves out; the bone flags say which bones the cloth moves.
        let with_flags = |bones: &[(&str, i64, i64)]| {
            kv3(&obj(vec![(
                "m_modelSkeleton",
                obj(vec![
                    ("m_boneName", arr(bones.iter().map(|(n, _, _)| s(n)).collect())),
                    ("m_nParent", arr(bones.iter().map(|(_, p, _)| i(*p)).collect())),
                    ("m_nFlag", arr(bones.iter().map(|(_, _, f)| i(*f)).collect())),
                ]),
            )]))
        };
        let cloth = CLOTH_FLAGS as i64;
        let skeleton = [("base", -1, 0), ("arm", 0, 0), ("tip", 1, 0), ("$cloth_m0p0", 1, cloth), ("$cloth_m0p1", 3, cloth)];
        let key = [("base", -1), ("arm", 0), ("tip", 1)];
        let file = |bones: &[(&str, i64, i64)], tag: i64| container_of(&[("AGRP", agrp(&key, tag)), ("CTRL", ctrl(&[("AGRP", 0)])), ("DATA", with_flags(bones))]);
        let (game, ours) = (file(&skeleton, 1), file(&skeleton, 2));
        let by_name = Allowed { decode_by_name: true, ..Allowed::default() };
        assert!(problems(&game, &ours, &["AGRP"], by_name).iter().any(|p| p.contains("3 bones, the rebuilt skeleton 5")));
        let proxy = Allowed { cloth_proxy: true, ..by_name };
        assert!(problems(&game, &ours, &["AGRP"], proxy).is_empty());
        // A bone missing from the key that the cloth does not move, or another set missing: refused.
        let unflagged = [("base", -1, 0), ("arm", 0, 0), ("tip", 1, 0), ("$cloth_m0p0", 1, 0), ("$cloth_m0p1", 3, cloth)];
        assert!(!problems(&file(&unflagged, 1), &file(&unflagged, 2), &["AGRP"], proxy).is_empty());
        let more = [("base", -1, 0), ("arm", 0, 0), ("tip", 1, 0), ("$cloth_m0p0", 1, cloth), ("$cloth_m0p1", 3, cloth), ("extra", 0, 0)];
        assert!(!problems(&game, &file(&more, 2), &["AGRP"], proxy).is_empty());
    }
}