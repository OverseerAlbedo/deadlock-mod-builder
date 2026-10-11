//! Remove the selected render bones from the cloth control name/hash binding.
//! R2 removed calculations but left all fourteen controls resolvable to bones.
//! Keep solver indices, constraints and every other control unchanged.
use crate::{animation::names, native::*};
use deadlock_skeleton::Skeleton;
use std::collections::{BTreeMap, BTreeSet};

/// Source 2 lowercase StringToken / cloth-control hash. Validate against EVERY
/// captured control before using it; a compiler hash change must stop the test.
pub fn control_hash(name: &str) -> u32 {
    const M: u32 = 0x5bd1e995;
    let lower = name.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut h = 0x31415926u32 ^ bytes.len() as u32;
    let mut chunks = bytes.chunks_exact(4);
    for chunk in &mut chunks {
        let mut k = u32::from_le_bytes(chunk.try_into().unwrap()).wrapping_mul(M);
        k ^= k >> 24;
        k = k.wrapping_mul(M);
        h = h.wrapping_mul(M) ^ k;
    }
    let tail = chunks.remainder();
    for (j, &byte) in tail.iter().enumerate() {
        h ^= (byte as u32) << (8 * j);
    }
    if !tail.is_empty() {
        h = h.wrapping_mul(M);
    }
    h ^= h >> 13;
    h = h.wrapping_mul(M);
    h ^ (h >> 15)
}
pub fn targets() -> BTreeSet<String> {
    names()
        .into_iter()
        .flat_map(|n| [format!("{n}___"), n])
        .collect()
}
fn controls(fe: &deadlock_resource::Value) -> Result<(Vec<String>, Vec<u32>)> {
    let names = arr(fe, "m_CtrlName")?
        .iter()
        .map(|v| v.string().map(str::to_owned))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let hashes = arr(fe, "m_CtrlHash")?
        .iter()
        .map(|v| Ok(u32::try_from(v.integer()?)?))
        .collect::<Result<Vec<_>>>()?;
    require(
        names.len() == hashes.len() && !names.is_empty(),
        "Cloth control name/hash counts disagree",
    )?;
    require(
        names
            .iter()
            .zip(&hashes)
            .all(|(n, &h)| control_hash(n) == h),
        "Cloth control hash algorithm differs from the captured native model; refusing to guess",
    )?;
    Ok((names, hashes))
}
fn disconnected_name(index: usize) -> String {
    format!("$dmb_playback_r3_unbound_{index}")
}
pub fn unbind(raw: &[u8], skeleton: &Skeleton) -> Result<(Vec<u8>, serde_json::Value)> {
    let before = data(raw, "PHYS")?;
    let fe = field(&before.root, "m_pFeModel")?;
    let (mut ids, mut hashes) = controls(fe)?;
    let targets = targets();
    let bone_hashes: BTreeSet<_> = skeleton.names.iter().map(|n| control_hash(n)).collect();
    let all_hashes: BTreeSet<_> = hashes.iter().copied().collect();
    let mut seen = BTreeSet::new();
    let mut rows = Vec::new();
    for (j, name) in ids.iter_mut().enumerate() {
        if !targets.contains(name) {
            continue;
        }
        require(
            seen.insert(name.clone()),
            "Duplicate selected cloth control",
        )?;
        let replacement = disconnected_name(j);
        let hash = control_hash(&replacement);
        require(
            !bone_hashes.contains(&hash) && !all_hashes.contains(&hash),
            "Disconnected cloth control collides with an existing name/hash",
        )?;
        rows.push(serde_json::json!({"index":j,"bone":name,"old_hash":hashes[j],"unbound_control":replacement,"new_hash":hash}));
        *name = replacement;
        hashes[j] = hash;
    }
    require(
        seen == targets,
        "Expected all fourteen left-streamer cloth bindings",
    )?;
    require(
        hashes.iter().collect::<BTreeSet<_>>().len() == hashes.len(),
        "Cloth control hash collision",
    )?;
    let mut changed = before.clone();
    let after = changed
        .root
        .get_mut("m_pFeModel")
        .ok_or("No cloth solver")?;
    set(
        after,
        "m_CtrlName",
        list(ids.iter().map(|n| s(n)).collect()),
    )?;
    set(
        after,
        "m_CtrlHash",
        list(hashes.iter().map(|&h| i(h as i64)).collect()),
    )?;
    unchanged_except(fe, after, &["m_CtrlName", "m_CtrlHash"])?;
    unchanged_except(&before.root, &changed.root, &["m_pFeModel"])?;
    let out = rebuild(raw, BTreeMap::from([("PHYS".into(), encode(&changed)?)]))?;
    verify_detached(&out)?;
    Ok((
        out,
        serde_json::json!({"native_hashes_verified":ids.len(),"disconnected_controls":rows,"selected_name_or_hash_bindings_remaining":0,"other_control_names_and_hashes_unchanged":true}),
    ))
}
pub fn verify_detached(raw: &[u8]) -> Result<()> {
    let model = data(raw, "DATA")?;
    let skeleton = Skeleton::from_value(field(&model.root, "m_modelSkeleton")?)?;
    let physics = data(raw, "PHYS")?;
    let (ids, hashes) = controls(field(&physics.root, "m_pFeModel")?)?;
    let targets = targets();
    let target_hashes: BTreeSet<_> = targets.iter().map(|n| control_hash(n)).collect();
    require(
        ids.iter().all(|n| !targets.contains(n))
            && hashes.iter().all(|h| !target_hashes.contains(h)),
        "Left-streamer physics still binds by control name or hash",
    )?;
    let bone_hashes: BTreeSet<_> = skeleton.names.iter().map(|n| control_hash(n)).collect();
    let mut disconnected = 0;
    for (j, name) in ids.iter().enumerate() {
        if name.starts_with("$dmb_playback_r3_unbound_") {
            require(
                *name == disconnected_name(j) && !bone_hashes.contains(&hashes[j]),
                "Disconnected cloth slot still maps to a render bone",
            )?;
            disconnected += 1;
        }
    }
    require(
        disconnected == 14,
        "Missing disconnected left-streamer control slots",
    )?;
    let selected: Vec<_> = skeleton
        .names
        .iter()
        .enumerate()
        .filter(|(_, n)| targets.contains(*n))
        .collect();
    require(
        selected.len() == 14
            && selected
                .iter()
                .all(|(j, _)| skeleton.flags[*j] & 0x400008 == 0),
        "Left-streamer cloth/procedural flags are still enabled",
    )?;
    Ok(())
}
