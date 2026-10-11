//! ModelDoc revision negotiation. Structural migration belongs to Valve's loader;
//! source headers are never relabeled to pretend an unsupported conversion worked.
use super::{get, invalid, regex, Result};
use deadlock_resource::text::Document;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
pub const ENCODING: &str = "e21c7f3c-8a33-41c5-9977-a76d3a32aa0d";
pub const PROBE_FORMAT: &str = "dmb_modeldoc_probe";
const FORMATS: &[(u32, &str)] = &[
    (28, "fb63b6ca-f435-4aa0-a2c7-c66ddc651dca"),
    (32, "c5dcef98-b629-46ab-88e3-a17c005c935e"),
    (40, "da0ab1f8-9722-4910-94b8-10b6a08c0934"),
    (41, "12fc9d44-453a-4ae4-b4d9-7e2ac0bbd4e0"),
];
pub fn header(version: u32) -> Result<String> {
    let format = FORMATS
        .iter()
        .find(|(v, _)| *v == version)
        .map(|(_, s)| *s)
        .ok_or_else(|| invalid(format!("Unsupported ModelDoc{version} source format.")))?;
    Ok(format!(
        "<!-- kv3 encoding:text:version{{{ENCODING}}} format:modeldoc{version}:version{{{format}}} -->"
    ))
}
pub fn source_format(text: &str) -> Result<u32> {
    let pattern = regex(
        r"^\s*<!--\s*kv3\s+encoding:text:version\{([^}]+)\}\s+format:modeldoc(\d+):version\{([^}]+)\}\s*-->",
    );
    let captured = pattern.captures(text).ok_or_else(|| {
        invalid("Model compiler requires a canonical text ModelDoc source header.")
    })?;
    if captured[1].to_lowercase() != ENCODING {
        return Err(invalid(
            "Model compiler requires a canonical text ModelDoc source header.",
        ));
    }
    let version = captured[2]
        .parse::<u32>()
        .map_err(|_| invalid("Unsupported ModelDoc source version."))?;
    if !FORMATS
        .iter()
        .any(|(v, g)| *v == version && *g == captured[3].to_lowercase())
    {
        return Err(invalid(format!(
            "Unsupported ModelDoc{version} source format/GUID; refusing an unreviewed conversion."
        )));
    }
    Ok(version)
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MigrationPlan {
    pub source_format: u32,
    pub temporary_file_format: u32,
    pub compiler_target: Option<u32>,
    pub conversion: String,
    pub header_rewritten: bool,
    pub structure_rewritten_for_format: bool,
    pub classes: BTreeMap<String, usize>,
    pub native_migration_verified: bool,
}
pub fn migration_plan(text: &str, target: Option<u32>) -> Result<MigrationPlan> {
    let version = source_format(text)?;
    let doc = Document::parse(text)?;
    let root = doc
        .root
        .get("rootNode")
        .filter(|n| n.object().is_some() && get(n, "_class") == Some("RootNode"))
        .ok_or_else(|| invalid("ModelDoc source has no RootNode."))?;
    if root.get("children").and_then(|n| n.array()).is_none() {
        return Err(invalid("ModelDoc RootNode must have a children array."));
    }
    if let Some(target) = target {
        if ![40, 41].contains(&target) {
            return Err(invalid(format!(
                "Unsupported ModelDoc{target} compiler target; no conversion was attempted."
            )));
        }
        if version > target {
            return Err(invalid(format!(
                "Unsupported ModelDoc{version} → ModelDoc{target} downgrade. Use the original/decompiled source or a compiler that supports the source revision. The builder will not relabel or discard newer features."
            )));
        }
    }
    let mut classes = BTreeMap::new();
    for node in doc.objects(None) {
        if let Some(class) = get(node, "_class").filter(|s| !s.is_empty()) {
            *classes.entry(class.to_owned()).or_default() += 1;
        }
    }
    Ok(MigrationPlan {
        source_format: version,
        temporary_file_format: version,
        compiler_target: target,
        conversion: match target {
            None => "pending_compiler_detection",
            Some(t) if t == version => "none",
            _ => "valve_native_loader",
        }
        .into(),
        header_rewritten: false,
        structure_rewritten_for_format: false,
        classes,
        native_migration_verified: false,
    })
}
fn mismatch(output: &str) -> Option<(String, u32)> {
    let pattern = regex(
        r#"(?i)No valid format conversion from\s+['"]([^'"]+)['"]\s+to\s+['"]modeldoc(\d+)['"]"#,
    );
    let capture = pattern.captures(output)?;
    Some((capture[1].into(), capture[2].parse().ok()?))
}
fn commas(value: &str) -> String {
    let reverse = value.chars().rev().collect::<Vec<_>>();
    let mut result = String::new();
    for (i, c) in reverse.iter().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(*c);
    }
    result.chars().rev().collect()
}
pub fn diagnostic(output: &str) -> Option<String> {
    if regex("(?i)Mesh is too large and uses GPU Animated Vertex Cache").is_match(output) {
        let mesh = regex(r"RenderMesh:\s*([^\r\n:]+)")
            .captures(output)
            .map(|m| m[1].trim().to_owned());
        let count = regex(r"(?i)Vertex Count:\s*(\d+)")
            .captures(output)
            .map(|m| m[1].to_owned());
        let mut detail = mesh
            .map(|m| {
                format!(
                    " Render node '{}'.",
                    m.replace('\\', "\\\\").replace('\'', "\\'")
                )
            })
            .unwrap_or_default();
        if let Some(count) = count {
            detail.push_str(&format!(
                " Valve reports {} compiled vertices.",
                commas(&count)
            ));
        }
        return Some(format!(
            "Animated vertex-cache limit exceeded (524,288).{detail} Enable Automatically split safely (default 400,000 conservative vertices), or export separate skinned meshes and use Select multiple in mesh mapping. Aim below about 450,000 vertices per manual piece, allowing for attribute seams. Keep skinning enabled; never use DiscardSkeleton for an animated hero body."
        ));
    }
    if output.contains("ParticleFloatType_t")
        && regex(r"(?i)Mismatch\s*-\s*Member Count").is_match(output)
    {
        return Some("Compiler startup schema mismatch (ParticleFloatType_t). This happens before VMDL loading; changing ModelDoc headers or the DMX cannot fix it. Select a compatible binary family. This diagnostic does not imply mixed DLL directories or PATH contamination.".into());
    }
    let missing_pattern = regex(
        r"(?i)(not (?:been )?registered|unregistered|unknown\s+(?:node\s+)?class|class.*(?:not found|does not exist))",
    );
    let mut missing = BTreeSet::new();
    for line in output.lines() {
        if missing_pattern.is_match(line) {
            for name in [
                "NmSkeletonList",
                "NmSkeletonReference",
                "AnimGraph2List",
                "DefaultAnimGraph2",
                "AnimGraph2",
            ] {
                let pattern = regex(&format!(r"(?:^|[^A-Za-z0-9_]){name}(?:$|[^A-Za-z0-9_])"));
                if pattern.is_match(line) {
                    missing.insert(name);
                }
            }
        }
    }
    if !missing.is_empty() {
        return Some(format!(
            "Native ModelDoc class registration failed: {}. A loaded animationsystem.dll or modeldoc_utils.dll does not establish these node factories. Use Setup → Scan ModelDoc classes to inspect the actual binary family, then select a compatible model toolset. Required skeleton/AG2 nodes are retained; this is not a DMX or material error.",
            missing.into_iter().collect::<Vec<_>>().join(", ")
        ));
    }
    if let Some((from, to)) = mismatch(output) {
        return Some(format!(
            "Compiler/source format mismatch: {from} cannot be converted by this ModelDoc{to} loader."
        ));
    }
    if regex(r"(?i)(?:unknown|unrecognized|unregistered|unsupported|could not (?:find|create)|failed to (?:find|create))[^\n]*(?:node (?:type|class)|modeldoc.*class)|(?:unknown|unrecognized)\s+(?:class|attribute|property)").is_match(output){return Some("Compiler rejected a ModelDoc node or property. Required source features must not be silently discarded.".into());}
    None
}
pub fn probe_source() -> String {
    format!(
        "<!-- kv3 encoding:text:version{{{ENCODING}}} format:{PROBE_FORMAT}:version{{ae831238-90b7-4f7b-81ed-0d12e87b5c65}} -->\n{{ rootNode = {{ _class = \"RootNode\" children = [] }} }}\n"
    )
}
/// Examine the fresh probe's complete output. Exit zero alone is never proof.
/// The process owner must persist the complete receipt and delete owned probe
/// files on either success/failure. This pure function performs no process I/O.
pub fn interpret_probe(output: &str) -> Result<u32> {
    if let Some(problem) = diagnostic(output) {
        if problem.contains("startup schema mismatch") {
            return Err(invalid(problem));
        }
    }
    let (from,target)=mismatch(output).ok_or_else(||invalid("Compiler format probe was inconclusive. See compiler-format.json and Copy Log. In Setup, select 40 or 41 only when confirmed by this compiler's own loader diagnostic. No model or input files were converted."))?;
    if from != PROBE_FORMAT {
        return Err(invalid(
            "Compiler format probe was inconclusive. No model or input files were converted.",
        ));
    }
    if ![40, 41].contains(&target) {
        return Err(invalid(format!(
            "Unsupported ModelDoc{target} compiler target. No automatic conversion is implemented."
        )));
    }
    Ok(target)
}
