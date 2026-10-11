//! Allowlisted names-only repair of compiler-sanitized bone lookups.
use crate::{
    array, check_transfer, field, integer, invalid, string, IdentityAudit, Result, Skeleton,
};
use crate::{bindings, extensions, transforms};
use deadlock_resource::{binary, cloth, container, Kind, Value};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Part {
    Key(String),
    Index(usize),
}
type Path = Vec<Part>;
fn key(s: &str) -> Part {
    Part::Key(s.into())
}
fn hash(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}
fn map_error(e: impl std::fmt::Display) -> crate::Error {
    invalid(e.to_string())
}
fn decode(raw: &[u8], block: &deadlock_vpk::ResourceBlock) -> Result<binary::Document> {
    binary::decode(container::payload(raw, block).map_err(map_error)?).map_err(map_error)
}
fn at_mut<'a>(value: &'a mut Value, path: &[Part]) -> Result<&'a mut Value> {
    if path.is_empty() {
        return Ok(value);
    }
    let next = match &path[0] {
        Part::Key(key) => value
            .get_mut(key)
            .ok_or_else(|| invalid("Missing allowlisted name field"))?,
        Part::Index(index) => match &mut value.kind {
            Kind::Array(rows) => rows
                .get_mut(*index)
                .ok_or_else(|| invalid("Out-of-range allowlisted name index"))?,
            _ => return Err(invalid("Expected name array")),
        },
    };
    at_mut(next, &path[1..])
}
fn allowed_paths(name: &str, value: &Value) -> Result<BTreeSet<Path>> {
    let mut paths = BTreeSet::new();
    match name {
        "DATA" => {
            for i in 0..array(field(field(value, "m_modelSkeleton")?, "m_boneName")?)?.len() {
                paths.insert(vec![
                    key("m_modelSkeleton"),
                    key("m_boneName"),
                    Part::Index(i),
                ]);
            }
        }
        "MDAT" => {
            for i in 0..array(field(field(value, "m_skeleton")?, "m_bones")?)?.len() {
                for name in ["m_boneName", "m_parentName"] {
                    paths.insert(vec![
                        key("m_skeleton"),
                        key("m_bones"),
                        Part::Index(i),
                        key(name),
                    ]);
                }
            }
        }
        "AGRP" => {
            let decode = field(value, "m_decodeKey")?;
            for i in 0..array(field(decode, "m_boneArray")?)?.len() {
                paths.insert(vec![
                    key("m_decodeKey"),
                    key("m_boneArray"),
                    Part::Index(i),
                    key("m_name"),
                ]);
            }
            for (i, channel) in array(field(decode, "m_dataChannelArray")?)?
                .iter()
                .enumerate()
            {
                if string(field(channel, "m_szChannelClass")?)? == "BoneChannel" {
                    for j in 0..array(field(channel, "m_szElementNameArray")?)?.len() {
                        paths.insert(vec![
                            key("m_decodeKey"),
                            key("m_dataChannelArray"),
                            Part::Index(i),
                            key("m_szElementNameArray"),
                            Part::Index(j),
                        ]);
                    }
                }
            }
        }
        _ => (),
    }
    Ok(paths)
}
fn permitted_hash(block: &str, path: &[Part], name: &str, node: &Value) -> bool {
    if block != "MDAT" {
        return false;
    }
    let hitbox_set = path.len() == 3
        && path[0] == key("m_hitboxsets")
        && matches!(path[1], Part::Index(_))
        && path[2] == key("value")
        && name == "m_nNameHash"
        && node
            .get("m_name")
            .is_some_and(|v| matches!(v.kind, Kind::String(_)));
    let hitbox_bone = path.len() == 5
        && path[0] == key("m_hitboxsets")
        && matches!(path[1], Part::Index(_))
        && path[2] == key("value")
        && path[3] == key("m_HitBoxes")
        && matches!(path[4], Part::Index(_))
        && name == "m_nBoneNameHash"
        && node
            .get("m_sBoneName")
            .is_some_and(|v| matches!(v.kind, Kind::String(_)));
    hitbox_set || hitbox_bone
}
fn blob_contains_alias(data: &[u8], aliases: &BTreeMap<String, String>) -> bool {
    let patterns: Vec<_> = aliases.keys().map(|s| s.as_bytes()).collect();
    let Some(first) = patterns.first() else { return false };
    let common = patterns.iter().fold(first.len(), |length, pattern| {
        first[..length].iter().zip(*pattern).take_while(|(a, b)| a == b).count()
    });
    if common == 0 {
        return patterns.iter().any(|p| p.is_empty() || data.windows(p.len()).any(|w| w == *p));
    }
    // Cloth palettes can contain hundreds of aliases sharing one prefix. Scan
    // a large geometry/sample blob once, then compare only matching prefixes.
    data.windows(common).enumerate().any(|(i, prefix)| {
        prefix == &first[..common] && patterns.iter().any(|p| data[i..].starts_with(p))
    })
}
fn edits(
    block: &str,
    value: &Value,
    aliases: &BTreeMap<String, String>,
) -> Result<Vec<(Path, Value, Value)>> {
    let allowed = allowed_paths(block, value)?;
    let mut edits = vec![];
    fn walk(
        block: &str,
        node: &Value,
        path: &mut Path,
        aliases: &BTreeMap<String, String>,
        allowed: &BTreeSet<Path>,
        edits: &mut Vec<(Path, Value, Value)>,
    ) -> Result<()> {
        match &node.kind {
            Kind::Object(rows) => {
                for (name, child) in rows {
                    if aliases.keys().any(|a| name.contains(a)) {
                        return Err(invalid("Export bone alias occurs in unsupported KV3 key"));
                    }
                    if ["DATA", "MDAT", "AGRP", "ASEQ", "ANIM"].contains(&block)
                        && name.to_lowercase().contains("hash")
                        && !permitted_hash(block, path, name, node)
                    {
                        return Err(invalid(format!(
                            "Unsupported hashed runtime binding field: {name}"
                        )));
                    }
                    path.push(key(name));
                    walk(block, child, path, aliases, allowed, edits)?;
                    path.pop();
                }
            }
            Kind::Array(rows) => {
                for (i, child) in rows.iter().enumerate() {
                    path.push(Part::Index(i));
                    walk(block, child, path, aliases, allowed, edits)?;
                    path.pop();
                }
            }
            Kind::String(text) => {
                if let Some(replacement) = aliases.get(text).filter(|_| allowed.contains(path)) {
                    let mut changed = node.clone();
                    changed.kind = Kind::String(replacement.clone());
                    edits.push((path.clone(), node.clone(), changed));
                } else if aliases.keys().any(|a| text.contains(a)) {
                    return Err(invalid(format!(
                        "Export bone alias occurs in unsupported runtime field in {block}"
                    )));
                }
            }
            Kind::Blob(data) => {
                if blob_contains_alias(data, aliases) {
                    return Err(invalid(format!(
                        "Export bone alias occurs in unsupported runtime blob: {block}"
                    )));
                }
            }
            _ => (),
        }
        Ok(())
    }
    walk(block, value, &mut vec![], aliases, &allowed, &mut edits)?;
    Ok(edits)
}
/// Rename only the audited model/mesh/animation lookup fields. Callers must
/// establish identity authority first and validate the resulting bindings.
/// Any alias in another string, key, blob or unsupported hashed binding fails.
pub fn rename_lookup_fields(
    block: &str,
    value: &mut Value,
    aliases: &BTreeMap<String, String>,
) -> Result<usize> {
    if aliases.is_empty() {
        return Ok(0);
    }
    let changes = edits(block, value, aliases)?;
    for (path, _, replacement) in &changes {
        *at_mut(value, path)? = replacement.clone();
    }
    Ok(changes.len())
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alias {
    pub exported: String,
    pub original: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangedBlock {
    pub ordinal: usize,
    pub name: String,
    pub changed_field_count: usize,
    pub before_sha256: String,
    pub after_sha256: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AliasRepair {
    pub status: String,
    pub original_sha256: String,
    pub input_sha256: String,
    pub output_sha256: String,
    pub aliases: Vec<Alias>,
    pub alias_count: usize,
    pub changed_field_count: usize,
    pub changed_blocks: Vec<ChangedBlock>,
    pub skeleton_identity: IdentityAudit,
    pub scale_preservation: transforms::ScaleAudit,
    pub mesh_binding_audit: bindings::MeshAudit,
    pub names_only: bool,
    pub numeric_data_unchanged: bool,
    pub geometry_payloads_unchanged: bool,
    pub encoded_animation_samples_unchanged: bool,
    pub anim_aseq_payloads_unchanged: bool,
    pub animation_bindings_checked: bool,
    pub resource_references_unchanged: bool,
    pub in_game_motion_tested: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_binding: Option<extensions::SourceBindingAudit>,
}
fn source_solver(physics: &Value, skeleton: &Skeleton) -> Result<(usize, Vec<String>)> {
    let solver = field(physics, "m_pFeModel")?;
    if matches!(solver.kind, Kind::Null) {
        return Ok((0, vec![]));
    }
    let count = integer(field(solver, "m_nNodeCount")?)?;
    let static_ = integer(field(solver, "m_nStaticNodes")?)?;
    let ropes = integer(field(solver, "m_nRopeCount")?)?;
    let names = array(field(solver, "m_CtrlName")?)?
        .iter()
        .map(|v| string(v).map(str::to_string))
        .collect::<Result<Vec<_>>>()?;
    if count < 0
        || static_ < 0
        || static_ > count
        || ropes < 0
        || names.iter().any(|n| n.is_empty())
        || names.iter().collect::<BTreeSet<_>>().len() != names.len()
        || (count > 0 && names.is_empty())
    {
        return Err(invalid("Invalid original cloth metadata"));
    }
    if let Some(hashes) = solver.get("m_CtrlHash") {
        let hashes = array(hashes)?;
        if hashes.len() != names.len()
            || hashes
                .iter()
                .any(|v| integer(v).map_or(true, |n| !(0..=u32::MAX as i64).contains(&n)))
        {
            return Err(invalid("Invalid original control hashes"));
        }
    }
    cloth::control_bindings(&names, &skeleton.names, solver).map_err(map_error)?;
    Ok((count as usize, names))
}

pub fn repair_aliases(
    original: &[u8],
    compiled: &[u8],
    allow_extensions: bool,
) -> Result<(Vec<u8>, Option<AliasRepair>)> {
    repair_aliases_with_source(original, compiled, allow_extensions, None)
}

/// Valve's compiler replaces each unsupported UTF-8 byte with an underscore.
/// This spelling alone never grants authority: the complete source palette must
/// be collision-free, and canonical parents/locals/scales are checked below.
fn compiler_spelling(name: &str) -> String {
    name.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b == b'_' {
                b as char
            } else {
                '_'
            }
        })
        .collect()
}

pub fn repair_aliases_with_source(
    original: &[u8],
    compiled: &[u8],
    allow_extensions: bool,
    source_binding: Option<&extensions::SourceBinding>,
) -> Result<(Vec<u8>, Option<AliasRepair>)> {
    if let Some(source) = source_binding {
        if !allow_extensions || source.original_sha256 != hash(original) {
            return Err(invalid(
                "Custom bone alias authority belongs to another original model",
            ));
        }
        source.skeleton.validate(true)?;
    }
    let source_blocks = container::layout(original).map_err(map_error)?;
    let blocks = container::layout(compiled).map_err(map_error)?;
    let source = decode(
        original,
        container::unique(&source_blocks.blocks, "DATA").map_err(map_error)?,
    )?
    .root;
    let data_block = container::unique(&blocks.blocks, "DATA").map_err(map_error)?;
    let data_ordinal = blocks
        .blocks
        .iter()
        .position(|b| std::ptr::eq(b, data_block))
        .unwrap();
    let data = decode(compiled, data_block)?;
    let actual = Skeleton::from_value(field(&data.root, "m_modelSkeleton")?)?;
    let reference = Skeleton::from_value(field(&source, "m_modelSkeleton")?)?;
    let present: BTreeSet<_> = actual.names.iter().map(String::as_str).collect();
    let originals: BTreeSet<_> = reference.names.iter().map(String::as_str).collect();
    if present == originals {
        return Ok((compiled.to_vec(), None));
    }
    let physics = if source_blocks.blocks.iter().any(|b| b.name == "PHYS") {
        decode(
            original,
            container::unique(&source_blocks.blocks, "PHYS").map_err(map_error)?,
        )?
        .root
    } else {
        Value::new(Kind::Object(vec![]))
    };
    let (count, controls) = if physics.get("m_pFeModel").is_some() {
        source_solver(&physics, &reference)?
    } else {
        (0, vec![])
    };
    if count == 0 && source_binding.is_none() {
        return Ok((compiled.to_vec(), None));
    }
    check_transfer(&reference, &reference)?;
    let mut aliases = BTreeMap::new();
    for (name, &flags) in reference.names.iter().zip(&reference.flags) {
        if let Some(suffix) = name.strip_prefix('$') {
            let exported = format!("_{suffix}");
            if count > 0
                && flags & 0x400008 == 0x400008
                && !present.contains(name.as_str())
                && present.contains(exported.as_str())
                && !originals.contains(exported.as_str())
            {
                aliases.insert(exported, name.clone());
            }
        }
    }
    if let Some(source) = source_binding {
        let source_names: BTreeSet<_> = source.skeleton.names.iter().map(String::as_str).collect();
        let mut spellings: BTreeMap<String, Vec<&str>> = BTreeMap::new();
        for name in &source.skeleton.names {
            spellings
                .entry(compiler_spelling(name))
                .or_default()
                .push(name);
        }
        for name in &source.extra_names {
            if originals.contains(name.as_str()) || !source_names.contains(name.as_str()) {
                return Err(invalid("Invalid custom bone alias source palette"));
            }
            let exported = compiler_spelling(name);
            if exported == *name
                || present.contains(name.as_str())
                || !present.contains(exported.as_str())
            {
                continue;
            }
            if spellings[&exported].len() != 1
                || source_names.contains(exported.as_str())
                || originals.contains(exported.as_str())
                || aliases.contains_key(&exported)
            {
                return Err(invalid(format!(
                    "Ambiguous compiler spelling for custom bone: {name}"
                )));
            }
            aliases.insert(exported, name.clone());
        }
    }
    if aliases.is_empty() {
        return Ok((compiled.to_vec(), None));
    }
    let mut canonical = actual.clone();
    canonical.names = canonical
        .names
        .iter()
        .map(|n| aliases.get(n).unwrap_or(n).clone())
        .collect();
    let canonical_set: BTreeSet<_> = canonical.names.iter().map(String::as_str).collect();
    let extended = allow_extensions
        && originals.is_subset(&canonical_set)
        && originals.len() < canonical_set.len();
    if canonical_set.len() != canonical.names.len() || (canonical_set != originals && !extended) {
        return Err(invalid(
            "Export aliases do not form an unambiguous original skeleton identity",
        ));
    }
    let compared = if extended {
        extensions::extension_identity(&canonical, &reference)?;
        let collision = canonical_set.difference(&originals).any(|name| {
            controls.iter().any(|n| n == name)
                || physics
                    .get("m_boneNames")
                    .and_then(|v| v.array().ok())
                    .is_some_and(|rows| rows.iter().any(|v| v.string().ok() == Some(*name)))
        });
        if collision {
            return Err(invalid(
                "Custom bones collide with original physics controls",
            ));
        }
        extensions::project_skeleton(&canonical, &reference.names)?
    } else {
        canonical.clone()
    };
    let identity = check_transfer(&compared, &reference)?;
    let scale = transforms::audit_scale_preservation(&compared, &reference, false)?;
    // Names may be repaired only after proving the same source identities,
    // ancestry and binds; never infer correspondence from array order.
    let source_proof = source_binding
        .map(|source| extensions::source_binding(&canonical, &reference, source, &hash(original)))
        .transpose()?;
    let mut decoded = BTreeMap::new();
    for (i, block) in blocks.blocks.iter().enumerate() {
        if ["MVTX", "MIDX", "RERL"].contains(&block.name.as_str()) {
            continue;
        }
        if ![
            "DATA", "MDAT", "AGRP", "ANIM", "ASEQ", "PHYS", "CTRL", "RED2", "DSTF",
        ]
        .contains(&block.name.as_str())
        {
            return Err(invalid(format!(
                "Unsupported runtime block during alias restoration: {}",
                block.name
            )));
        }
        decoded.insert(
            i,
            if i == data_ordinal {
                data.clone()
            } else {
                decode(compiled, block)?
            },
        );
    }
    let ctrl_ordinal = blocks
        .blocks
        .iter()
        .position(|b| b.name == "CTRL")
        .ok_or_else(|| invalid("Missing CTRL"))?;
    container::unique(&blocks.blocks, "CTRL").map_err(map_error)?;
    let meshes: Vec<_> = blocks
        .blocks
        .iter()
        .enumerate()
        .filter(|(_, b)| b.name == "MDAT")
        .map(|(i, _)| i)
        .collect();
    let groups: Vec<_> = blocks
        .blocks
        .iter()
        .enumerate()
        .filter(|(_, b)| b.name == "AGRP")
        .map(|(i, _)| i)
        .collect();
    let before_meshes: Vec<_> = meshes.iter().map(|i| decoded[i].root.clone()).collect();
    let before_groups: Vec<_> = groups.iter().map(|i| decoded[i].root.clone()).collect();
    let custom: BTreeSet<String> = source_proof
        .as_ref()
        .map(|p| p.source_bones_checked.iter().cloned().collect())
        .unwrap_or_default();
    let before_custom: BTreeSet<String> = actual
        .names
        .iter()
        .filter(|n| custom.contains(aliases.get(*n).unwrap_or(*n)))
        .cloned()
        .collect();
    bindings::audit_bindings_with_custom_bones(
        &decoded[&data_ordinal].root,
        &decoded[&ctrl_ordinal].root,
        &before_meshes,
        &before_groups,
        &blocks.blocks,
        &before_custom,
    )?;
    let mut replacements = BTreeMap::new();
    let mut changes = vec![];
    for (&ordinal, document) in &mut decoded {
        let name = &blocks.blocks[ordinal].name;
        let changes_here = edits(name, &document.root, &aliases)?;
        if changes_here.is_empty() {
            continue;
        }
        let before = document.root.clone();
        for (path, _, new) in &changes_here {
            *at_mut(&mut document.root, path)? = new.clone();
        }
        let patched = binary::encode(&document.root, document.format_guid).map_err(map_error)?;
        let mut checked = binary::decode(&patched).map_err(map_error)?.root;
        for (path, old, _) in &changes_here {
            *at_mut(&mut checked, path)? = old.clone();
        }
        if !checked.canonical_eq(&before) {
            return Err(invalid(
                "Alias repair changed data beyond approved name fields",
            ));
        }
        changes.push(ChangedBlock {
            ordinal,
            name: name.clone(),
            changed_field_count: changes_here.len(),
            before_sha256: hash(
                container::payload(compiled, &blocks.blocks[ordinal]).map_err(map_error)?,
            ),
            after_sha256: hash(&patched),
        });
        replacements.insert(ordinal, patched);
    }
    let mdats: Vec<_> = meshes.iter().map(|i| decoded[i].root.clone()).collect();
    let agrps: Vec<_> = groups.iter().map(|i| decoded[i].root.clone()).collect();
    let checked = bindings::audit_bindings_with_custom_bones(
        &decoded[&data_ordinal].root,
        &decoded[&ctrl_ordinal].root,
        &mdats,
        &agrps,
        &blocks.blocks,
        &custom,
    )?;
    let mesh_audit = bindings::audit_meshes(
        &decoded[&data_ordinal].root,
        &decoded[&ctrl_ordinal].root,
        &mdats,
        &blocks.blocks,
        if extended { &canonical } else { &reference },
    )?;
    let _ = checked;
    let output = container::rebuild_ordinals(compiled, &replacements).map_err(map_error)?;
    if deadlock_vpk::resource_references(&output).map_err(map_error)?
        != deadlock_vpk::resource_references(compiled).map_err(map_error)?
    {
        return Err(invalid("Alias restoration changed resource references"));
    }
    let output_sha256 = hash(&output);
    Ok((
        output,
        Some(AliasRepair {
            status: "restored_export_aliases".into(),
            original_sha256: hash(original),
            input_sha256: hash(compiled),
            output_sha256,
            alias_count: aliases.len(),
            aliases: aliases
                .into_iter()
                .map(|(exported, original)| Alias { exported, original })
                .collect(),
            changed_field_count: changes.iter().map(|c| c.changed_field_count).sum(),
            changed_blocks: changes,
            skeleton_identity: identity,
            scale_preservation: scale,
            mesh_binding_audit: mesh_audit,
            names_only: true,
            numeric_data_unchanged: true,
            geometry_payloads_unchanged: true,
            encoded_animation_samples_unchanged: true,
            anim_aseq_payloads_unchanged: true,
            animation_bindings_checked: true,
            resource_references_unchanged: true,
            in_game_motion_tested: false,
            source_binding: source_proof,
        }),
    ))
}
