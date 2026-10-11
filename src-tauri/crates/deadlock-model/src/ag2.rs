//! AG2 references retain authoritative DATA identifiers and exact source nodes.
use crate::{get, invalid, kv_string, modeldoc, Result};
use deadlock_project::{asset_paths::relative_asset, AnimationPolicy};
use deadlock_resource::{
    text::{Document, Edit, Node, NodeKind},
    Kind, Value,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
pub const CLASSES: [&str; 3] = ["NmSkeletonReference", "DefaultAnimGraph2", "AnimGraph2"];
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct Reference {
    pub class: String,
    pub name: String,
    pub filename: String,
}
pub fn path(value: &str) -> Result<String> {
    Ok(relative_asset(value.strip_suffix("_c").unwrap_or(value))?)
}
fn node_path(node: Option<&Node>) -> Result<String> {
    let node = node.ok_or_else(|| invalid("AG2 reference has no resource filename."))?;
    match &node.kind {
        NodeKind::Scalar { value, .. } => path(value),
        NodeKind::Typed { name, value } if name == "resource" => node_path(Some(value)),
        _ => Err(invalid("AG2 reference has no resource filename.")),
    }
}
pub fn source_nodes(text: &str) -> Result<Vec<Reference>> {
    let document = Document::parse(text)?;
    let mut nodes = Vec::new();
    for node in document.objects(None) {
        let Some(class) = get(node, "_class").filter(|c| CLASSES.contains(c)) else {
            continue;
        };
        let filename = node_path(node.get("filename"))?;
        let suffix = if class == "NmSkeletonReference" {
            ".vnmskel"
        } else {
            ".vnmgraph"
        };
        if !filename.ends_with(suffix) {
            return Err(invalid(format!("Invalid {class} filename: {filename}")));
        }
        nodes.push(Reference {
            class: class.into(),
            name: get(node, "name").unwrap_or_default().into(),
            filename,
        });
    }
    Ok(nodes)
}
fn defaults(nodes: &[Reference]) -> Result<()> {
    if nodes
        .iter()
        .filter(|n| n.class == "DefaultAnimGraph2")
        .count()
        > 1
    {
        return Err(invalid(
            "Multiple default AG2 graphs require a reviewed source definition.",
        ));
    }
    Ok(())
}
pub fn from_data_dump(output: &str) -> Result<Vec<Reference>> {
    let start = output
        .find("<!-- kv3")
        .or_else(|| crate::regex(r"(?m)^\s*\{").find(output).map(|m| m.start()))
        .ok_or_else(|| invalid("Source 2 Viewer did not return a text KV3 DATA block."))?;
    let end = output
        .rfind('}')
        .filter(|end| *end >= start)
        .ok_or_else(|| invalid("Source 2 Viewer did not return a text KV3 DATA block."))?;
    let document = Document::parse(&output[start..=end])?;
    let mut result = Vec::new();
    if let Some(skeletons) = document.root.get("m_vecNmSkeletonRefs") {
        for item in skeletons
            .array()
            .ok_or_else(|| invalid("Unsupported m_vecNmSkeletonRefs layout."))?
        {
            result.push(Reference {
                class: "NmSkeletonReference".into(),
                name: String::new(),
                filename: node_path(Some(item))?,
            });
        }
    }
    if let Some(graphs) = document.root.get("m_animGraph2Refs") {
        for item in graphs
            .array()
            .ok_or_else(|| invalid("Unsupported m_animGraph2Refs layout."))?
        {
            if item.object().is_none() {
                return Err(invalid("Unsupported m_animGraph2Refs item."));
            }
            let identifier = match item.get("m_sIdentifier") {
                None => "",
                Some(Node {
                    kind: NodeKind::Scalar { value, .. },
                    ..
                }) => value,
                _ => return Err(invalid("Invalid AG2 graph identifier.")),
            };
            let default = ["", "default"].contains(&identifier);
            result.push(Reference {
                class: if default {
                    "DefaultAnimGraph2"
                } else {
                    "AnimGraph2"
                }
                .into(),
                name: if default {
                    String::new()
                } else {
                    identifier.into()
                },
                filename: node_path(item.get("m_hGraph"))?,
            });
        }
    }
    defaults(&result)?;
    Ok(result)
}
/// Native binary DATA boundary, avoiding text/rounded-number conversion entirely.
pub fn from_data(data: &Value) -> Result<Vec<Reference>> {
    let mut result = Vec::new();
    if let Some(skeletons) = data.get("m_vecNmSkeletonRefs") {
        for item in skeletons.array()? {
            result.push(Reference {
                class: "NmSkeletonReference".into(),
                name: String::new(),
                filename: path(item.string()?)?,
            });
        }
    }
    if let Some(graphs) = data.get("m_animGraph2Refs") {
        for item in graphs.array()? {
            if !matches!(&item.kind, Kind::Object(_)) {
                return Err(invalid("Unsupported m_animGraph2Refs item."));
            }
            let identifier = item
                .get("m_sIdentifier")
                .map(|v| v.string())
                .transpose()?
                .unwrap_or("");
            let default = ["", "default"].contains(&identifier);
            let filename = path(
                item.get("m_hGraph")
                    .ok_or_else(|| invalid("AG2 graph filename missing."))?
                    .string()?,
            )?;
            result.push(Reference {
                class: if default {
                    "DefaultAnimGraph2"
                } else {
                    "AnimGraph2"
                }
                .into(),
                name: if default {
                    String::new()
                } else {
                    identifier.into()
                },
                filename,
            });
        }
    }
    defaults(&result)?;
    Ok(result)
}
fn folded(value: &str) -> String {
    deadlock_core::text::casefold(value)
}
pub fn reconcile(
    text: &str,
    original_refs: Option<&[String]>,
    recovered: &[Reference],
) -> Result<(String, Vec<Reference>)> {
    let nodes = source_nodes(text)?;
    let mut required = BTreeSet::new();
    for reference in original_refs.unwrap_or_default() {
        let p = path(reference)?;
        if p.ends_with(".vnmskel") || p.ends_with(".vnmgraph") {
            required.insert(folded(&p));
        }
    }
    if original_refs.is_some() {
        let stale: Vec<_> = nodes
            .iter()
            .filter(|n| !required.contains(&folded(&n.filename)))
            .map(|n| n.filename.as_str())
            .collect();
        if !stale.is_empty() {
            return Err(invalid(format!(
                "Source AG2 references differ from the original compiled model. Regenerate the original source:\n{}",
                stale.join("\n")
            )));
        }
    }
    let covered: BTreeSet<_> = nodes.iter().map(|n| folded(&n.filename)).collect();
    let missing: BTreeSet<_> = required.difference(&covered).cloned().collect();
    let mut additions = Vec::new();
    for reference in recovered {
        if missing.contains(&folded(&path(&reference.filename)?)) {
            if !CLASSES.contains(&reference.class.as_str()) {
                return Err(invalid("Unsupported recovered AG2 node class."));
            }
            additions.push(reference);
        }
    }
    let additions_paths: BTreeSet<_> = additions
        .iter()
        .map(|n| path(&n.filename).map(|p| folded(&p)))
        .collect::<Result<_>>()?;
    if !missing.is_subset(&additions_paths) {
        return Err(invalid(
            "Original AG2 nodes are missing from the decompiled source. Read authoritative DATA to recover their identifiers.",
        ));
    }
    if additions.is_empty() {
        return Ok((text.into(), nodes));
    }
    let document = Document::parse(text)?;
    let children = document
        .root
        .get("rootNode")
        .and_then(|n| n.get("children"))
        .filter(|n| n.array().is_some())
        .ok_or_else(|| invalid("Cannot locate the ModelDoc root children for AG2 recovery."))?;
    let mut edits = Vec::new();
    let mut new_lists = Vec::new();
    for list_class in ["NmSkeletonList", "AnimGraph2List"] {
        let members: Vec<_> = additions
            .iter()
            .filter(|n| (n.class == "NmSkeletonReference") == (list_class == "NmSkeletonList"))
            .collect();
        if members.is_empty() {
            continue;
        }
        let rendered = members
            .into_iter()
            .map(|n| {
                Ok(format!(
                    "{{ _class = {}{} filename = {} }}",
                    kv_string(&n.class),
                    if n.name.is_empty() {
                        String::new()
                    } else {
                        format!(" name = {}", kv_string(&n.name))
                    },
                    kv_string(&path(&n.filename)?)
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        let lists: Vec<_> = children
            .array()
            .unwrap()
            .iter()
            .filter(|n| get(n, "_class") == Some(list_class))
            .collect();
        if lists.len() > 1 {
            return Err(invalid(format!(
                "Multiple {list_class} nodes require a reviewed source."
            )));
        }
        if let Some(list) = lists.first() {
            let array = list
                .get("children")
                .filter(|n| n.array().is_some())
                .ok_or_else(|| invalid("AG2 list has no children array."))?;
            let body = text[array.start + 1..array.end - 1].trim_end();
            let prefix = if !array.array().unwrap().is_empty() && !body.ends_with(',') {
                ","
            } else {
                ""
            };
            edits.push(Edit {
                start: array.end - 1,
                end: array.end - 1,
                replacement: format!("{prefix}\n{},\n", rendered.join(",\n")),
            });
        } else {
            new_lists.push(format!(
                "{{ _class = {} children = [\n{}\n] }}",
                kv_string(list_class),
                rendered.join(",\n")
            ));
        }
    }
    if !new_lists.is_empty() {
        let body = text[children.start + 1..children.end - 1].trim_end();
        let prefix = if !children.array().unwrap().is_empty() && !body.ends_with(',') {
            ","
        } else {
            ""
        };
        edits.push(Edit {
            start: children.end - 1,
            end: children.end - 1,
            replacement: format!("{prefix}\n{},\n", new_lists.join(",\n")),
        });
    }
    let changed = document.apply(&edits)?;
    let nodes = source_nodes(&changed)?;
    Ok((changed, nodes))
}
pub fn compiler_source(
    text: &str,
    policy: AnimationPolicy,
    target: Option<u32>,
) -> Result<(String, Vec<String>)> {
    modeldoc::migration_plan(text, target)?;
    let document = Document::parse(text)?;
    let mut edits = Vec::new();
    let mut changes = Vec::new();
    if policy == AnimationPolicy::DisableLegacy {
        for node in document.objects(None) {
            let Some(class) = get(node, "_class")
                .filter(|c| ["AnimationList", "EmptyAnimGraph", "AnimGraph"].contains(c))
            else {
                continue;
            };
            if get(node, "disabled") == Some("true") {
                continue;
            }
            let edit = if let Some(old) = node.get("disabled") {
                Edit {
                    start: old.start,
                    end: old.end,
                    replacement: "true".into(),
                }
            } else {
                Edit {
                    start: node.end - 1,
                    end: node.end - 1,
                    replacement: "\n disabled = true\n".into(),
                }
            };
            edits.push(edit);
            changes.push(format!("Temporarily disabled {class}"));
        }
    }
    Ok((document.apply(&edits)?, changes))
}
pub fn preview_source(text: &str) -> Result<String> {
    let document = Document::parse(text)?;
    let candidates: Vec<_> = document
        .objects(None)
        .into_iter()
        .filter(|n| {
            get(n, "_class").is_some_and(|c| {
                CLASSES.contains(&c) || ["NmSkeletonList", "AnimGraph2List"].contains(&c)
            })
        })
        .collect();
    let mut edits = Vec::new();
    for node in &candidates {
        if candidates
            .iter()
            .any(|p| p.start < node.start && p.end > node.end)
        {
            continue;
        }
        let mut end = node.end;
        if let Some(after) = crate::regex(r"^\s*,").find(&text[end..]) {
            end += after.end();
        }
        edits.push(Edit {
            start: node.start,
            end,
            replacement: String::new(),
        });
    }
    Ok(document.apply(&edits)?)
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Verification {
    pub references_verified: bool,
    pub identifiers_verified: bool,
    pub expected_nodes: usize,
}
pub fn verify(
    expected: &[Reference],
    compiled_refs: &[String],
    actual: Option<&[Reference]>,
) -> Result<Verification> {
    let refs: BTreeSet<_> = compiled_refs
        .iter()
        .map(|r| path(r).map(|p| folded(&p)))
        .collect::<Result<_>>()?;
    let missing: Vec<_> = expected
        .iter()
        .filter(|n| !refs.contains(&folded(&n.filename)))
        .map(|n| n.filename.clone())
        .collect();
    if !missing.is_empty() {
        return Err(invalid(format!(
            "Compiled model lost required AG2/skeleton references:\n{}",
            missing.join("\n")
        )));
    }
    if let Some(actual) = actual {
        let observed: BTreeSet<_> = actual
            .iter()
            .map(|n| {
                (
                    n.class.clone(),
                    if n.class == "AnimGraph2" {
                        n.name.clone()
                    } else {
                        String::new()
                    },
                    folded(n.filename.strip_suffix("_c").unwrap_or(&n.filename)),
                )
            })
            .collect();
        for n in expected {
            if !observed.contains(&(
                n.class.clone(),
                if n.class == "AnimGraph2" {
                    n.name.clone()
                } else {
                    String::new()
                },
                folded(n.filename.strip_suffix("_c").unwrap_or(&n.filename)),
            )) {
                return Err(invalid(format!(
                    "Compiled DATA lost or changed AG2 graph identifiers/skeleton bindings: {} {} {}",
                    n.class, n.name, n.filename
                )));
            }
        }
    }
    Ok(Verification {
        references_verified: true,
        identifiers_verified: actual.is_some(),
        expected_nodes: expected.len(),
    })
}
