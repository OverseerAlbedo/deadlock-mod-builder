//! Lossless, reference-compatible model source assembly. Unknown authored nodes
//! survive untouched; unsupported mesh references fail closed.
pub mod ag2;
pub mod equipment;
pub mod modeldoc;
pub mod source_skeleton;
use deadlock_project::asset_paths::relative_asset;
use deadlock_project::{LodStrategy, MeshMapping};
use deadlock_resource::text::{field_edit, Document, Edit, Node, NodeKind};
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
pub type Mappings = BTreeMap<String, MeshMapping>;
pub const KEEP: &str = "@keep";
pub const DISABLE: &str = "@disable";
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Text(#[from] deadlock_resource::Error),
    #[error(transparent)]
    Project(#[from] deadlock_project::Error),
}
pub type Result<T> = std::result::Result<T, Error>;
fn invalid(message: impl Into<String>) -> Error {
    Error::Invalid(message.into())
}
fn get<'a>(node: &'a Node, key: &str) -> Option<&'a str> {
    node.get(key).and_then(Node::scalar)
}
fn regex(pattern: &str) -> Regex {
    Regex::new(pattern).expect("static regex")
}
pub fn canonical(name: &str) -> String {
    let pattern = regex(r"(?i)\.vmdl(?:_c)?\.");
    let name = pattern.split(name).last().unwrap_or(name);
    deadlock_core::text::casefold(name)
        .chars()
        .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        .collect()
}
pub fn lod_base(name: &str) -> String {
    regex(r"(?i)[_. -]?lod[_. -]?\d+$")
        .replace(name, "")
        .into_owned()
}
pub fn expanded_mappings(mappings: &Mappings) -> Result<BTreeMap<String, String>> {
    let mut project = deadlock_project::Project::default();
    project.meshes = mappings.clone();
    Ok(project.expanded_mappings()?)
}
pub fn selected_meshes(mappings: &Mappings) -> Result<BTreeSet<String>> {
    let mut result = BTreeSet::new();
    for mapping in mappings.values() {
        result.extend(mapping.targets()?.into_iter().map(str::to_owned));
    }
    Ok(result)
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MeshInfo {
    pub name: String,
    pub filename: Option<String>,
    pub lower_lod: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AnimationNode {
    pub class: String,
    pub filename: Option<String>,
    pub name: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Analysis {
    pub meshes: Vec<MeshInfo>,
    pub ag2: Vec<AnimationNode>,
    pub lod_groups: usize,
    pub attachments: usize,
    pub classes: Vec<String>,
}
pub fn analyze(text: &str) -> Result<Analysis> {
    let doc = Document::parse(text)?;
    let mut names = BTreeSet::new();
    let mut meshes = Vec::new();
    for node in doc.objects(Some("RenderMeshFile")) {
        let name = get(node, "name").unwrap_or_default();
        if name.is_empty() || !names.insert(name) {
            return Err(invalid(
                "VMDL must have uniquely named RenderMeshFile nodes.",
            ));
        }
        meshes.push(MeshInfo {
            name: name.into(),
            filename: get(node, "filename").map(str::to_owned),
            lower_lod: lod_base(name) != name,
        });
    }
    if meshes.is_empty() {
        return Err(invalid(
            "VMDL must have uniquely named RenderMeshFile nodes.",
        ));
    }
    let mut ag2 = Vec::new();
    let mut classes = BTreeSet::new();
    for node in doc.objects(None) {
        if let Some(class) = get(node, "_class").filter(|s| !s.is_empty()) {
            classes.insert(class.into());
            if ["AnimGraph2", "DefaultAnimGraph2", "NmSkeletonReference"].contains(&class) {
                ag2.push(AnimationNode {
                    class: class.into(),
                    filename: get(node, "filename").map(str::to_owned),
                    name: get(node, "name").unwrap_or_default().into(),
                });
            }
        }
    }
    Ok(Analysis {
        meshes,
        ag2,
        lod_groups: doc.objects(Some("LODGroup")).len(),
        attachments: doc.objects(Some("Attachment")).len(),
        classes: classes.into_iter().collect(),
    })
}
pub fn suggest_mappings(
    model: &Analysis,
    replacement_names: &[String],
    existing: &Mappings,
    strategy: LodStrategy,
    auto: bool,
) -> Result<(Mappings, Vec<String>)> {
    let names: BTreeSet<_> = model.meshes.iter().map(|m| m.name.as_str()).collect();
    let mut mappings = Mappings::new();
    for (key, value) in existing {
        if names.contains(key.as_str())
            && value
                .targets()?
                .iter()
                .all(|v| replacement_names.iter().any(|r| r == v))
        {
            mappings.insert(key.clone(), value.clone());
        }
    }
    let mut nodes = model.meshes.iter().collect::<Vec<_>>();
    nodes.sort_by_key(|n| n.lower_lod);
    for node in nodes {
        let name = &node.name;
        if node.lower_lod && strategy == LodStrategy::Reuse {
            let bases: Vec<_> = names
                .iter()
                .filter(|n| {
                    canonical(n) == canonical(&lod_base(name)) && mappings.contains_key(**n)
                })
                .collect();
            if bases.len() == 1 {
                let value = mappings[*bases[0]].clone();
                mappings.insert(name.clone(), value);
                continue;
            }
        }
        if mappings.contains_key(name) {
            continue;
        }
        if node.lower_lod && strategy == LodStrategy::Preserve {
            mappings.insert(name.clone(), MeshMapping::One(KEEP.into()));
            continue;
        }
        if node.lower_lod && matches!(strategy, LodStrategy::Disable | LodStrategy::Single) {
            mappings.insert(name.clone(), MeshMapping::One(DISABLE.into()));
            continue;
        }
        if !auto {
            continue;
        }
        let mut candidates: Vec<_> = replacement_names
            .iter()
            .filter(|r| canonical(r) == canonical(name))
            .collect();
        if candidates.is_empty() && strategy == LodStrategy::Reuse && node.lower_lod {
            candidates = replacement_names
                .iter()
                .filter(|r| canonical(r) == canonical(&lod_base(name)))
                .collect();
        }
        if candidates.len() == 1 {
            mappings.insert(name.clone(), MeshMapping::One(candidates[0].clone()));
        }
    }
    let unresolved = model
        .meshes
        .iter()
        .filter(|n| !mappings.contains_key(&n.name))
        .map(|n| n.name.clone())
        .collect();
    Ok((mappings, unresolved))
}
pub fn isolated_filename(hero_root: &str, source_name: &str) -> Result<String> {
    let slug: String = source_name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .take(55)
        .collect();
    let tag = format!("{:x}", Sha256::digest(source_name.as_bytes()));
    Ok(relative_asset(&format!(
        "{hero_root}/dmx/builder_{}_{}.dmx",
        if slug.is_empty() { "mesh" } else { &slug },
        &tag[..8]
    ))?)
}
fn raw_field(node: &Node, key: &str, raw: String) -> Edit {
    if let Some(old) = node.get(key) {
        Edit {
            start: old.start,
            end: old.end,
            replacement: raw,
        }
    } else {
        Edit {
            start: node.end - 1,
            end: node.end - 1,
            replacement: format!("\n {key} = {raw}\n"),
        }
    }
}
// ModelDoc is UTF-8. JSON's ASCII \uXXXX escapes do not identify the
// literal DMX mesh in Valve's import filters; preserve the original characters.
fn kv_string(value: &str) -> String {
    serde_json::to_string(value).expect("string serialization")
}
fn json_array(values: &[String]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(|s| kv_string(s))
            .collect::<Vec<_>>()
            .join(", ")
    )
}
fn selection_filter(piece: &str) -> String {
    format!(
        "{{ exclude_by_default = true exception_list = [ {} ] }}",
        kv_string(piece)
    )
}
fn clone_node(
    text: &str,
    node: &Node,
    changes: &[(&str, &str)],
    piece: Option<&str>,
) -> Result<String> {
    let fragment = Document::parse(&text[node.start..node.end])?;
    let mut edits = changes
        .iter()
        .map(|(k, v)| field_edit(&fragment.root, k, v))
        .collect::<deadlock_resource::Result<Vec<_>>>()?;
    if let Some(piece) = piece {
        edits.push(raw_field(
            &fragment.root,
            "import_filter",
            selection_filter(piece),
        ));
    }
    Ok(fragment.apply(&edits)?)
}
fn after_comma(text: &str, end: usize) -> usize {
    let mut at = end;
    for c in text[end..].chars() {
        if c.is_whitespace() {
            at += c.len_utf8();
        } else {
            return if c == ',' { at + 1 } else { end };
        }
    }
    end
}
fn array_items(node: &Node, result: &mut BTreeSet<usize>) {
    match &node.kind {
        NodeKind::Array(values) => {
            for n in values {
                result.insert(n.start);
                array_items(n, result);
            }
        }
        NodeKind::Object(values) => {
            for (_, n) in values {
                array_items(n, result);
            }
        }
        _ => (),
    }
}
pub fn patch(
    text: &str,
    mappings: &Mappings,
    source_paths: &BTreeMap<String, String>,
) -> Result<String> {
    let doc = Document::parse(text)?;
    let mut edits = Vec::new();
    let mut disabled = BTreeSet::new();
    let nodes = doc.objects(Some("RenderMeshFile"));
    expanded_mappings(mappings)?;
    let mut splits = BTreeMap::new();
    for (name, value) in mappings {
        if value.targets()?.len() > 1 {
            splits.insert(
                name.clone(),
                (1..=value.targets()?.len())
                    .map(|i| format!("{name}__part{i:02}"))
                    .collect::<Vec<_>>(),
            );
        }
    }
    let mut array_members = BTreeSet::new();
    array_items(&doc.root, &mut array_members);
    for node in &nodes {
        let name = get(node, "name").ok_or_else(|| invalid("RenderMeshFile has no name."))?;
        let target = mappings.get(name).ok_or_else(|| {
            invalid(format!(
                "Choose a replacement, Keep Original or Disable for mesh: {name}"
            ))
        })?;
        if target == &MeshMapping::One(KEEP.into()) {
            continue;
        }
        if target == &MeshMapping::One(DISABLE.into()) {
            disabled.insert(name.to_owned());
            edits.push(Edit {
                start: node.start,
                end: after_comma(text, node.end),
                replacement: String::new(),
            });
        } else {
            let pieces = target.targets()?;
            for piece in &pieces {
                if !source_paths.contains_key(*piece) {
                    return Err(invalid(format!(
                        "Replacement DMX does not contain mapped mesh: {piece}"
                    )));
                }
            }
            if let Some(outputs) = splits.get(name) {
                if !array_members.contains(&node.start) {
                    return Err(invalid(format!(
                        "Cannot expand non-array RenderMeshFile: {name}"
                    )));
                }
                let mut copies = Vec::new();
                for (out, piece) in outputs.iter().zip(pieces) {
                    let path = relative_asset(&source_paths[piece])?;
                    copies.push(clone_node(
                        text,
                        node,
                        &[("name", out), ("filename", &path)],
                        Some(piece),
                    )?);
                }
                edits.push(Edit {
                    start: node.start,
                    end: node.end,
                    replacement: copies.join(",\n"),
                });
            } else {
                edits.push(field_edit(
                    node,
                    "filename",
                    &relative_asset(&source_paths[pieces[0]])?,
                )?);
                edits.push(raw_field(
                    node,
                    "import_filter",
                    selection_filter(pieces[0]),
                ));
            }
        }
    }
    for node in doc.objects(None) {
        if nodes
            .iter()
            .any(|n| n.start <= node.start && node.start < n.end)
        {
            continue;
        }
        if let Some(outputs) = get(node, "mesh_name").and_then(|name| splits.get(name)) {
            if !array_members.contains(&node.start) {
                return Err(invalid(format!(
                    "Cannot expand a scalar mesh reference to {}; use a supported mesh-reference array.",
                    get(node, "mesh_name").unwrap()
                )));
            }
            let copies = outputs
                .iter()
                .map(|name| clone_node(text, node, &[("mesh_name", name)], None))
                .collect::<Result<Vec<_>>>()?;
            edits.push(Edit {
                start: node.start,
                end: node.end,
                replacement: copies.join(",\n"),
            });
            continue;
        }
        for (key, value) in node.object().unwrap() {
            if key == "name" && matches!(get(node, "_class"), Some("BodyGroup" | "BodyGroupChoice"))
            {
                continue;
            }
            if get(node, "_class") == Some("BodyGroupChoice")
                && key == "meshes"
                && value.array().is_some_and(|v| {
                    v.iter()
                        .any(|x| x.scalar().is_some_and(|n| disabled.contains(n)))
                })
            {
                let values = value.array().unwrap();
                if values.iter().any(|x| x.scalar().is_none()) {
                    return Err(invalid(
                        "Unsupported BodyGroupChoice meshes layout; keep the mesh or use a custom source.",
                    ));
                }
                let mut names = Vec::new();
                for x in values {
                    let name = x.scalar().unwrap();
                    if !disabled.contains(name) {
                        names.extend(
                            splits
                                .get(name)
                                .cloned()
                                .unwrap_or_else(|| vec![name.into()]),
                        );
                    }
                }
                edits.push(Edit {
                    start: value.start,
                    end: value.end,
                    replacement: json_array(&names),
                });
                continue;
            }
            if let Some(v) = value.scalar() {
                if splits.contains_key(v) {
                    return Err(invalid(format!(
                        "Cannot safely expand unknown {key} reference to {v}."
                    )));
                }
            }
            if let Some(values) = value.array() {
                if values
                    .iter()
                    .any(|x| x.scalar().is_some_and(|n| splits.contains_key(n)))
                {
                    if !["meshes", "mesh_names", "mesh_references"].contains(&key.as_str())
                        || values.iter().any(|x| x.scalar().is_none())
                    {
                        return Err(invalid(format!(
                            "Cannot safely expand mesh references in {key}."
                        )));
                    }
                    let mut names = Vec::new();
                    for x in values {
                        let name = x.scalar().unwrap();
                        names.extend(
                            splits
                                .get(name)
                                .cloned()
                                .unwrap_or_else(|| vec![name.into()]),
                        );
                    }
                    edits.push(Edit {
                        start: value.start,
                        end: value.end,
                        replacement: json_array(&names),
                    });
                }
            }
        }
        if get(node, "mesh_name").is_some_and(|name| disabled.contains(name)) {
            edits.push(Edit {
                start: node.start,
                end: after_comma(text, node.end),
                replacement: String::new(),
            });
        }
    }
    let result = doc.apply(&edits)?;
    if !disabled.is_empty() {
        let parsed = Document::parse(&result)?;
        for node in parsed.objects(None) {
            for (key, value) in node.object().unwrap() {
                if let Some(name) = value.scalar() {
                    if disabled.contains(name) && key != "name" && key != "_class" {
                        return Err(invalid(format!(
                            "Disabling {name} leaves an unsupported {key} reference; keep it or use a custom source."
                        )));
                    }
                }
                if value.array().is_some_and(|v| {
                    v.iter()
                        .any(|x| x.scalar().is_some_and(|n| disabled.contains(n)))
                }) {
                    return Err(invalid(format!(
                        "Disabled mesh remains referenced in {key}; keep it or use a custom source."
                    )));
                }
            }
        }
    }
    Ok(result)
}
pub fn source_references(text: &str) -> Result<Vec<String>> {
    let doc = Document::parse(text)?;
    let mut refs = BTreeSet::new();
    let suffix = regex(r"\.(dmx|vmdl|vmat|vnmgraph|vnmskel|vnmclip|vphys|vanim|vagrp|animgraph)$");
    for node in doc.objects(None) {
        for (_, value) in node.object().unwrap() {
            let value = match &value.kind {
                NodeKind::Typed { name, value } if name == "resource" => value.as_ref(),
                _ => value,
            };
            if let Some(v) = value.scalar() {
                if v.contains('/') && suffix.is_match(v) {
                    refs.insert(relative_asset(v)?);
                }
            }
        }
    }
    Ok(refs.into_iter().collect())
}
#[derive(Debug, Clone)]
pub struct BoneRef<'a> {
    pub node: &'a Node,
    pub parent: Option<String>,
}
pub fn bone_hierarchy(doc: &Document) -> Result<BTreeMap<String, BoneRef<'_>>> {
    fn visit<'a>(
        node: &'a Node,
        parent: Option<&str>,
        found: &mut BTreeMap<String, BoneRef<'a>>,
    ) -> Result<()> {
        match &node.kind {
            NodeKind::Object(values) => {
                let mut parent = parent;
                if get(node, "_class") == Some("Bone") {
                    let name = get(node, "name").filter(|s| !s.is_empty()).ok_or_else(|| {
                        invalid("Skeleton contains duplicate or unnamed Bone nodes.")
                    })?;
                    if found
                        .insert(
                            name.into(),
                            BoneRef {
                                node,
                                parent: parent.map(str::to_owned),
                            },
                        )
                        .is_some()
                    {
                        return Err(invalid(
                            "Skeleton contains duplicate or unnamed Bone nodes.",
                        ));
                    }
                    parent = Some(name);
                }
                for (_, n) in values {
                    visit(n, parent, found)?;
                }
            }
            NodeKind::Array(values) => {
                for n in values {
                    visit(n, parent, found)?;
                }
            }
            _ => (),
        }
        Ok(())
    }
    let mut result = BTreeMap::new();
    for skeleton in doc.objects(Some("Skeleton")) {
        visit(skeleton, None, &mut result)?;
    }
    Ok(result)
}
/// Validate explicit mesh filters, required-bone retention and no-cull policy,
/// then expose an owned hierarchy for independent DMX skeleton gates.
pub fn template_bones(text: &str) -> Result<BTreeMap<String, Option<String>>> {
    let document = Document::parse(text)?;
    template_filters(&document)?;
    Ok(bone_hierarchy(&document)?
        .into_iter()
        .map(|(name, bone)| (name, bone.parent))
        .collect())
}
fn template_filters(doc: &Document) -> Result<BTreeMap<String, &Node>> {
    let mut filters = BTreeMap::new();
    for node in doc.objects(Some("RenderMeshFile")) {
        let filter = node.get("import_filter");
        let names = filter
            .and_then(|f| f.get("exception_list"))
            .and_then(Node::array);
        if filter.and_then(|f| get(f, "exclude_by_default")) != Some("true")
            || names.is_none_or(|n| n.len() != 1)
        {
            return Err(invalid(
                "Compile template must explicitly include exactly one source mesh per filtered node.",
            ));
        }
        let target = names.unwrap()[0]
            .scalar()
            .ok_or_else(|| invalid("Compile template has ambiguous source mesh filters."))?;
        if filters.insert(target.into(), node).is_some() {
            return Err(invalid(
                "Compile template has ambiguous source mesh filters.",
            ));
        }
    }
    if filters.is_empty() {
        return Err(invalid(
            "Compile template contains no filtered RenderMeshFile nodes.",
        ));
    }
    let bones = bone_hierarchy(doc)?;
    if bones.is_empty()
        || bones
            .values()
            .any(|b| get(b.node, "do_not_discard") != Some("true"))
    {
        return Err(invalid(
            "Compile template must explicitly retain all required bones.",
        ));
    }
    let markup = doc.objects(Some("BoneMarkupList"));
    if markup.len() != 1 || get(markup[0], "bone_cull_type") != Some("None") {
        return Err(invalid(
            "Compile template must declare BoneMarkupList bone_cull_type None.",
        ));
    }
    Ok(filters)
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SourcePaths {
    One(String),
    PerMesh(BTreeMap<String, String>),
}
pub fn merge_template(
    text: &str,
    template: &str,
    mappings: &Mappings,
    source_path: &SourcePaths,
    strategy: LodStrategy,
    template_targets: Option<&BTreeMap<String, String>>,
    original_report: Option<&source_skeleton::SourceAuditReport>,
) -> Result<String> {
    let tpl = Document::parse(template)?;
    let filters = template_filters(&tpl)?;
    let required = bone_hierarchy(&tpl)?;
    let original = Document::parse(text)?;
    let bones =
        source_skeleton::canonical_source_hierarchy(bone_hierarchy(&original)?, original_report)?;
    for (name, item) in &required {
        let source=bones.get(name).ok_or_else(||invalid(format!("Compile template bone/hierarchy does not match original hero: {name} (bone is missing from the source skeleton)")))?;
        if item.parent != source.parent {
            return Err(invalid(format!(
                "Compile template bone/hierarchy does not match original hero: {name} (template parent {:?}, source parent {:?})",
                item.parent, source.parent
            )));
        }
    }
    let required_source_names: BTreeSet<_> = required
        .keys()
        .filter_map(|n| get(bones[n].node, "name"))
        .collect();
    let paths = match source_path {
        SourcePaths::PerMesh(v) => v.clone(),
        SourcePaths::One(v) => selected_meshes(mappings)?
            .into_iter()
            .map(|k| (k, v.clone()))
            .collect(),
    };
    let mut result = patch(text, mappings, &paths)?;
    if strategy == LodStrategy::Single {
        let d = Document::parse(&result)?;
        let lists = d.objects(Some("LODGroupList"));
        if lists.len() != 1 {
            return Err(invalid(
                "Single LOD strategy requires one original LODGroupList.",
            ));
        }
        let groups = lists[0]
            .get("children")
            .ok_or_else(|| invalid("Cannot determine the original highest-detail LOD group."))?;
        let rows = groups
            .array()
            .filter(|r| !r.is_empty())
            .ok_or_else(|| invalid("Cannot determine the original highest-detail LOD group."))?;
        let first = &rows[0];
        if get(first, "_class") != Some("LODGroup") {
            return Err(invalid(
                "Cannot determine the original highest-detail LOD group.",
            ));
        }
        if get(first, "switch_threshold").and_then(|v| v.parse::<f64>().ok()) != Some(0.) {
            return Err(invalid(
                "Single LOD requires an original first group with zero switch threshold.",
            ));
        }
        let retained = rows
            .iter()
            .filter(|n| n.start == first.start || get(n, "_class") != Some("LODGroup"))
            .map(|n| &result[n.start..n.end])
            .collect::<Vec<_>>();
        result = d.apply(&[Edit {
            start: groups.start,
            end: groups.end,
            replacement: format!("[\n{}\n]", retained.join(",\n")),
        }])?;
    }
    let d = Document::parse(&result)?;
    let mut edits = Vec::new();
    let expanded = expanded_mappings(mappings)?;
    let mut origins = BTreeMap::new();
    for (name, value) in mappings {
        for output in
            expanded_mappings(&BTreeMap::from([(name.clone(), value.clone())]))?.into_keys()
        {
            origins.insert(output, name.clone());
        }
    }
    for node in d.objects(Some("RenderMeshFile")) {
        let name = get(node, "name").unwrap_or_default();
        let target = expanded
            .get(name)
            .ok_or_else(|| invalid(format!("Missing mapped mesh: {name}")))?;
        if target == KEEP || target == DISABLE {
            continue;
        }
        let mapped = template_targets
            .and_then(|m| m.get(target))
            .unwrap_or(target);
        let template_node = if let Some(node) = filters.get(mapped) {
            *node
        } else {
            let parent = lod_base(&origins[name]);
            let candidates = filters
                .values()
                .filter(|n| canonical(get(n, "name").unwrap_or_default()) == canonical(&parent))
                .collect::<Vec<_>>();
            if candidates.len() != 1 {
                return Err(invalid(format!(
                    "Compile template has no unambiguous parent settings for mapped mesh: {target}"
                )));
            }
            *candidates[0]
        };
        edits.push(raw_field(node, "import_filter", selection_filter(target)));
        if let Some(value) = template_node.get("import_scale") {
            edits.push(raw_field(
                node,
                "import_scale",
                template[value.start..value.end].into(),
            ));
        }
    }
    for (name, item) in bone_hierarchy(&d)? {
        if required_source_names.contains(name.as_str())
            && get(item.node, "do_not_discard") != Some("true")
        {
            edits.push(raw_field(item.node, "do_not_discard", "true".into()));
        }
    }
    let markup = d.objects(Some("BoneMarkupList"));
    if markup.len() != 1 {
        return Err(invalid(
            "Original hero requires a single BoneMarkupList for safe template merging.",
        ));
    }
    if get(markup[0], "bone_cull_type") != Some("None") {
        edits.push(field_edit(markup[0], "bone_cull_type", "None")?);
    }
    Ok(d.apply(&edits)?)
}
