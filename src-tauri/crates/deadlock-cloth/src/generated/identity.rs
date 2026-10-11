//! Recover original cloth bone names without binding fresh proxy points to
//! identically named bones from the replaced hero solver. Numerical simulation
//! data and all index relationships remain the native compiler's authority.
use super::*;
use deadlock_skeleton::{aliases, check_transfer};

pub const POLICY: &str = "generated-original-cloth-identities-v1";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ControlRename {
    pub index: usize,
    pub before: String,
    pub after: String,
    pub before_hash: u32,
    pub after_hash: u32,
    pub derived_proxy: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Proof {
    pub policy: String,
    pub native_sha256: String,
    pub canonical_sha256: String,
    pub original_aliases: BTreeMap<String, String>,
    pub control_renames: Vec<ControlRename>,
    pub changed_lookup_fields: usize,
    pub original_bones_checked: usize,
    pub original_binds_and_scales_preserved: bool,
    pub solver_values_unchanged_except_control_names_and_hashes: bool,
    pub geometry_and_animation_samples_byte_preserved: bool,
}

// Case-insensitive Source 2 string token. Before editing a control lookup,
// require every native name/hash pair to agree with this representation.
fn token(name: &str) -> Result<u32> {
    if !name.is_ascii() {
        return Err(invalid("Unsupported non-ASCII native control hash spelling."));
    }
    let bytes = name.to_ascii_lowercase().into_bytes();
    let m = 0x5bd1e995_u32;
    let mut h = 0x31415926_u32 ^ bytes.len() as u32;
    let mut chunks = bytes.chunks_exact(4);
    for chunk in &mut chunks {
        let mut k = u32::from_le_bytes(chunk.try_into().unwrap()).wrapping_mul(m);
        k ^= k >> 24;
        k = k.wrapping_mul(m);
        h = h.wrapping_mul(m) ^ k;
    }
    let tail = chunks.remainder();
    for (i, b) in tail.iter().enumerate() {
        h ^= (*b as u32) << (8 * i);
    }
    if !tail.is_empty() {
        h = h.wrapping_mul(m);
    }
    h ^= h >> 13;
    h = h.wrapping_mul(m);
    Ok(h ^ (h >> 15))
}

fn set_array(value: &mut Value, key: &str, index: usize, kind: Kind) -> Result<()> {
    let Kind::Array(rows) = &mut field_mut(value, key)?.kind else {
        return Err(invalid(format!("Invalid generated identity array: {key}")));
    };
    rows.get_mut(index)
        .ok_or_else(|| invalid("Incomplete generated identity array"))?
        .kind = kind;
    Ok(())
}

fn control_names(
    physics: &mut Value,
    actual: &Skeleton,
    canonical: &Skeleton,
    source: &SourceBinding,
    aliases: &BTreeMap<String, String>,
) -> Result<Vec<ControlRename>> {
    let fe = field(physics, "m_pFeModel")?;
    let names = strings(field(fe, "m_CtrlName")?)?;
    let hashes = field(fe, "m_CtrlHash")?.array()?;
    let static_ = usize_field(fe, "m_nStaticNodes")?;
    let binding = cloth::control_bindings(&names, &actual.names, fe)?;
    let derived: BTreeSet<_> = binding.derived_controls.iter().map(|c| &c.control).collect();
    let restored: BTreeSet<_> = aliases.values().collect();
    let affected = names.iter().any(|n| aliases.contains_key(n) || restored.contains(n));
    if !affected {
        return Ok(vec![]);
    }
    if hashes.len() != names.len() {
        return Err(invalid("Incomplete generated control hashes."));
    }
    for (name, value) in names.iter().zip(hashes) {
        if value.integer()? != token(name)? as i128 {
            return Err(invalid(format!("Native control hash disagrees with its name: {name}")));
        }
    }
    let mut used_names = BTreeSet::new();
    let mut used_hashes = BTreeSet::new();
    for name in names.iter().chain(&actual.names).chain(&canonical.names).chain(&source.skeleton.names) {
        // Exported custom spellings are ASCII even when the source is not.
        let name = if name.is_ascii() { name.clone() } else { spelling(name) };
        used_names.insert(name.to_ascii_lowercase());
        used_hashes.insert(token(&name)?);
    }
    let mut changes = vec![];
    for (index, name) in names.iter().enumerate() {
        let is_proxy = restored.contains(name);
        let after = if is_proxy {
            if !derived.contains(name) || actual.names.contains(name) {
                return Err(invalid(format!("Generated control collides with original bone authority: {name}")));
            }
            let mut serial = 0usize;
            loop {
                let candidate = format!("dmb_generated_point_{index}_{serial}");
                if !used_names.contains(&candidate) && !used_hashes.contains(&token(&candidate)?) {
                    break candidate;
                }
                serial += 1;
            }
        } else if let Some(original) = aliases.get(name) {
            if index >= static_ || !actual.names.contains(name) {
                return Err(invalid(format!("Original cloth bone acquired a generated dynamic driver: {name}")));
            }
            original.clone()
        } else {
            continue;
        };
        used_names.insert(after.to_ascii_lowercase());
        used_hashes.insert(token(&after)?);
        changes.push(ControlRename {
            index, before: name.clone(), after: after.clone(),
            before_hash: token(name)?, after_hash: token(&after)?, derived_proxy: is_proxy,
        });
    }
    let before = fe.clone();
    let changed = field_mut(physics, "m_pFeModel")?;
    for row in &changes {
        set_array(changed, "m_CtrlName", row.index, Kind::String(row.after.clone()))?;
        set_array(changed, "m_CtrlHash", row.index, Kind::U64(row.after_hash as u64))?;
    }
    let new_names = strings(field(changed, "m_CtrlName")?)?;
    let new_hashes = field(changed, "m_CtrlHash")?.array()?;
    if new_names.iter().collect::<BTreeSet<_>>().len() != new_names.len()
        || new_hashes.iter().map(|h| h.integer()).collect::<deadlock_resource::Result<BTreeSet<_>>>()?.len() != new_hashes.len()
    {
        return Err(invalid("Generated control rename produced a lookup collision."));
    }
    // Replacing both lookup arrays with their original values must reproduce
    // every other typed native solver value, including all fit/offset indices.
    let mut restored_value = changed.clone();
    for key in ["m_CtrlName", "m_CtrlHash"] {
        *field_mut(&mut restored_value, key)? = field(&before, key)?.clone();
    }
    if !restored_value.canonical_eq(&before) {
        return Err(invalid("Generated identity repair changed simulation values."));
    }
    let after_binding = cloth::control_bindings(&new_names, &canonical.names, changed)?;
    for row in &changes {
        if row.derived_proxy && !after_binding.derived_controls.iter().any(|c| c.control == row.after) {
            return Err(invalid("Renamed proxy control lost its derived binding."));
        }
    }
    let renames: BTreeMap<_, _> = changes.iter().map(|r| (&r.before, &r.after)).collect();
    let rename = |name: &mut String| {
        if let Some(replacement) = renames.get(name) {
            *name = (*replacement).clone();
        }
    };
    let mut expected_binding = binding;
    expected_binding.skeleton_controls.iter_mut().for_each(rename);
    expected_binding.skeleton_controls.sort();
    for control in &mut expected_binding.derived_controls {
        rename(&mut control.control);
        control.anchors.iter_mut().for_each(rename);
        control.anchors.sort();
        for relation in &mut control.offset_relations {
            rename(&mut relation.control);
            rename(&mut relation.parent);
        }
    }
    if expected_binding != after_binding {
        return Err(invalid("Generated name repair changed control binding relationships."));
    }
    let proxy_names = changes.iter().filter(|r| r.derived_proxy)
        .map(|r| (r.before.clone(), r.after.clone())).collect();
    // No other physics field may contain a hidden name reference to a moved
    // proxy. The audited lookup was already changed above; this is scan-only.
    let mut other_fields = physics.clone();
    field_mut(field_mut(&mut other_fields, "m_pFeModel")?, "m_CtrlName")?.kind = Kind::Array(vec![]);
    aliases::rename_lookup_fields("PHYS", &mut other_fields, &proxy_names)?;
    Ok(changes)
}

pub(super) fn prepare(
    original: &[u8], native: &[u8], source: &SourceBinding,
) -> Result<(Vec<u8>, Proof)> {
    if source.original_sha256 != hash(original) {
        return Err(invalid("Generated identity source belongs to another original model."));
    }
    source.skeleton.validate(true)?;
    let decoded = Decoded::read(native)?;
    let actual = skeleton(&decoded.data.root)?;
    let layout = container::layout(original)?;
    let original_data = decode_unique(original, &layout.blocks, "DATA")?;
    let reference = skeleton(&original_data.root)?;
    let present: BTreeSet<_> = actual.names.iter().collect();
    if reference.names.iter().all(|n| present.contains(n)) {
        return Ok((native.to_vec(), Proof::default()));
    }
    let physics = decode_unique(original, &layout.blocks, "PHYS")?;
    let original_controls = strings(field(field(&physics.root, "m_pFeModel")?, "m_CtrlName")?)?;
    let mut spellings = BTreeMap::<String, usize>::new();
    for name in &source.skeleton.names {
        *spellings.entry(spelling(name)).or_default() += 1;
    }
    let mut aliases = BTreeMap::new();
    for (name, flags) in reference.names.iter().zip(&reference.flags) {
        if present.contains(name) {
            continue;
        }
        let exported = spelling(name);
        if !name.starts_with('$') || *flags & cloth::MOTION_MASK as u32 != cloth::MOTION_MASK as u32
            || !original_controls.contains(name) || !present.contains(&exported)
        {
            return Err(invalid(format!("Extended skeleton missing original identities: {name}. No verified compiler alias is present.")));
        }
        if spellings.get(&exported) != Some(&1) || source.skeleton.names.contains(&exported)
            || reference.names.contains(&exported) || aliases.insert(exported, name.clone()).is_some()
        {
            return Err(invalid(format!("Ambiguous original cloth compiler spelling: {name}")));
        }
    }
    let mut canonical = actual.clone();
    for name in &mut canonical.names {
        if let Some(original) = aliases.get(name) {
            *name = original.clone();
        }
    }
    let projected = extensions::project_skeleton(&canonical, &reference.names)?;
    check_transfer(&projected, &reference)?;
    transforms::audit_scale_preservation(&projected, &reference, false)?;
    let mut proof = Proof {
        policy: POLICY.into(), native_sha256: hash(native), original_aliases: aliases.clone(),
        original_bones_checked: reference.names.len(), original_binds_and_scales_preserved: true,
        ..Proof::default()
    };
    let mut replacements = BTreeMap::new();
    for (ordinal, block) in decoded.layout.blocks.iter().enumerate() {
        if ["MVTX", "MIDX", "RERL"].contains(&block.name.as_str()) {
            continue;
        }
        if !["DATA", "MDAT", "AGRP", "ANIM", "ASEQ", "PHYS", "CTRL", "RED2", "DSTF"].contains(&block.name.as_str()) {
            return Err(invalid(format!("Unsupported block during original identity repair: {}", block.name)));
        }
        let mut doc = binary::decode(container::payload(native, block)?)?;
        let before = doc.root.clone();
        if block.name == "PHYS" {
            proof.control_renames = control_names(&mut doc.root, &actual, &canonical, source, &aliases)?;
        }
        proof.changed_lookup_fields += aliases::rename_lookup_fields(&block.name, &mut doc.root, &aliases)?;
        if !before.canonical_eq(&doc.root) {
            let encoded = binary::encode(&doc.root, doc.format_guid)?;
            if !binary::decode(&encoded)?.root.canonical_eq(&doc.root) {
                return Err(invalid("Original identity repair failed typed serialization round-trip."));
            }
            replacements.insert(ordinal, encoded);
        }
    }
    let output = container::rebuild_ordinals(native, &replacements)?;
    unchanged_blocks(native, &output, &["DATA", "MDAT", "AGRP", "PHYS"])?;
    proof.canonical_sha256 = hash(&output);
    proof.solver_values_unchanged_except_control_names_and_hashes = true;
    proof.geometry_and_animation_samples_byte_preserved = true;
    Ok((output, proof))
}
