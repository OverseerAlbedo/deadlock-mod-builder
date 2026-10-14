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
        return Some("Compiler startup schema mismatch (ParticleFloatType_t): this compiler cannot build hero models. It usually means the Community SDK's compiler was used as the model compiler. Open Settings → Set up model compiler…, verify or download the toolset, click Use this toolset in settings, and build again. Changing the model or the DMX cannot fix this.".into());
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

/// The hero's own cloth as decompiled into a ModelDoc28 source: `Softbody`,
/// `ClothProxyMeshList` and `ClothShapeList` nodes directly under the RootNode.
/// Source 2 Viewer writes them since the October 2026 game files (Haze's
/// backpack hoses, Yamato's skirt proxy mesh); earlier decompiles had none.
/// Builds restore the game's compiled cloth after compiling, so the source copy
/// is left out: compiling it as well would give the hero its cloth twice (with a
/// different solver than the one restored), and the legacy cloth containers
/// secondary motion adds to a ModelDoc28 source cannot sit next to a Softbody.
/// Returns the source without them and how many were removed.
pub fn without_decompiled_cloth(text: &str) -> Result<(String, usize)> {
    if source_format(text).ok() != Some(28) {
        return Ok((text.to_owned(), 0));
    }
    let document = Document::parse(text)?;
    let Some(children) = document
        .root
        .get("rootNode")
        .filter(|n| get(n, "_class") == Some("RootNode"))
        .and_then(|n| n.get("children"))
        .and_then(|n| n.array())
    else {
        return Ok((text.to_owned(), 0));
    };
    let mut edits = vec![];
    for node in children.iter().filter(|n| matches!(get(n, "_class"), Some("Softbody" | "ClothProxyMeshList" | "ClothShapeList"))) {
        let mut end = node.end;
        if let Some(after) = regex(r"^\s*,").find(&text[end..]) {
            end += after.end();
        }
        edits.push(deadlock_resource::text::Edit { start: node.start, end, replacement: String::new() });
    }
    let removed = edits.len();
    Ok((document.apply(&edits)?, removed))
}

#[cfg(test)]
mod cloth_tests {
    use super::*;
    fn source(children: &str) -> String {
        format!("{}\n{{\n\trootNode = \n\t{{\n\t\t_class = \"RootNode\"\n\t\tchildren = \n\t\t[\n{children}\n\t\t]\n\t}}\n}}\n", header(28).unwrap())
    }
    #[test]
    fn the_heros_decompiled_cloth_is_left_out_and_nothing_else_changes() {
        let meshes = "\t\t\t{\n\t\t\t\t_class = \"RenderMeshList\"\n\t\t\t\tchildren = [ ]\n\t\t\t},";
        let cloth = "\t\t\t{\n\t\t\t\t_class = \"Softbody\"\n\t\t\t\tchildren = \n\t\t\t\t[\n\t\t\t\t\t{ _class = \"ClothNode\" name = \"hose_bottom_L_backpack\" },\n\t\t\t\t]\n\t\t\t},";
        let proxy = "\t\t\t{\n\t\t\t\t_class = \"ClothProxyMeshList\"\n\t\t\t\tchildren = [ { _class = \"ClothProxyMeshFile\" filename = \"yamato_cloth_proxy.dmx\" } ]\n\t\t\t},";
        let skeleton = "\t\t\t{\n\t\t\t\t_class = \"Skeleton\"\n\t\t\t\tchildren = [ ]\n\t\t\t},";
        let with = source(&format!("{meshes}\n{cloth}\n{proxy}\n{skeleton}"));
        let (stripped, removed) = without_decompiled_cloth(&with).unwrap();
        assert_eq!(removed, 2);
        assert!(!stripped.contains("Softbody") && !stripped.contains("hose_bottom_L_backpack") && !stripped.contains("cloth_proxy"));
        let doc = Document::parse(&stripped).unwrap();
        assert_eq!(doc.objects(Some("RenderMeshList")).len(), 1);
        assert_eq!(doc.objects(Some("Skeleton")).len(), 1);
        // A source without cloth stays byte for byte.
        let without = source(&format!("{meshes}\n{skeleton}"));
        assert_eq!(without_decompiled_cloth(&without).unwrap(), (without.clone(), 0));
    }
}

/// Source angles (pitch, yaw, roll in degrees) of a parent-relative rotation
/// quaternion (x, y, z, w), as Source 2 Viewer writes a Bone's `angles`.
pub fn angles_of(q: [f64; 4]) -> [f64; 3] {
    let [x, y, z, w] = q;
    let (m00, m10, m20) = (1. - 2. * (y * y + z * z), 2. * (x * y + w * z), 2. * (x * z - w * y));
    let (m01, m11) = (2. * (x * y - w * z), 1. - 2. * (x * x + z * z));
    let (m21, m22) = (2. * (y * z + w * x), 1. - 2. * (x * x + y * y));
    let flat = (m00 * m00 + m10 * m10).sqrt();
    let (pitch, yaw, roll) = if flat > 0.001 {
        ((-m20).atan2(flat), m10.atan2(m00), m21.atan2(m22))
    } else {
        ((-m20).atan2(flat), (-m01).atan2(m11), 0.)
    };
    [pitch, yaw, roll].map(f64::to_degrees)
}

/// A bone transform number the way Source 2 Viewer writes one (single
/// precision, shortest form, no negative zero).
fn number(v: f64) -> String {
    let v = v as f32;
    format!("{}", if v == 0. { 0. } else { v })
}

/// The leading tabs of the line that `at` is on.
fn indent_at(text: &str, at: usize) -> String {
    text[..at].rsplit('\n').next().unwrap_or_default().chars().take_while(|c| *c == '\t').collect()
}

/// The hero's cloth bones that the game's compiler makes (`$cloth_…`, cloth
/// flag `0x400008` in the compiled skeleton), added to a source Skeleton that
/// does not list them. Decompiles until October 2026 listed them as ordinary
/// bones named `_cloth_…` (`$` is not a bone-name character), which the build
/// compiles as plain bones and renames back when the game's compiled cloth is
/// restored; newer decompiles leave them to the cloth definitions instead, which
/// the build takes out (`without_decompiled_cloth`). The bones are written the
/// old way: parent-relative origin and angles, `do_not_discard`, at the end of
/// their parent (the Skeleton for roots), in compiled order. Returns how many.
pub fn with_compiled_cloth_bones(text: &str, compiled: &deadlock_skeleton::Skeleton) -> Result<(String, usize)> {
    let document = Document::parse(text)?;
    let skeletons = document.objects(Some("Skeleton"));
    let [skeleton] = skeletons.as_slice() else { return Ok((text.to_owned(), 0)) };
    let bone_nodes: BTreeMap<String, &deadlock_resource::text::Node> =
        document.objects(Some("Bone")).iter().filter_map(|n| get(n, "name").map(|name| (name.to_owned(), *n))).collect();
    let alias = |name: &str| name.strip_prefix('$').map(|s| format!("_{s}"));
    let wanted: Vec<usize> = (0..compiled.names.len())
        .filter(|&i| compiled.flags[i] & 0x400008 == 0x400008)
        .filter(|&i| alias(&compiled.names[i]).is_some_and(|a| !bone_nodes.contains_key(&a) && !bone_nodes.contains_key(&compiled.names[i])))
        .collect();
    if wanted.is_empty() {
        return Ok((text.to_owned(), 0));
    }
    let added: BTreeSet<usize> = wanted.iter().copied().collect();
    fn render(i: usize, compiled: &deadlock_skeleton::Skeleton, added: &BTreeSet<usize>, indent: &str) -> String {
        let f = number;
        let p = compiled.positions[i];
        let a = angles_of(compiled.rotations[i]);
        let children: Vec<String> = (0..compiled.names.len())
            .filter(|&c| added.contains(&c) && usize::try_from(compiled.parents[c]).ok() == Some(i))
            .map(|c| render(c, compiled, added, &format!("{indent}\t\t")))
            .collect();
        let mut out = format!(
            "{indent}{{\n{indent}\t_class = \"Bone\"\n{indent}\tname = \"_{}\"\n{indent}\torigin = [ {}, {}, {} ]\n{indent}\tangles = [ {}, {}, {} ]\n",
            &compiled.names[i][1..], f(p[0]), f(p[1]), f(p[2]), f(a[0]), f(a[1]), f(a[2])
        );
        if (compiled.scales[i] - 1.).abs() > 1e-4 {
            out += &format!("{indent}\tscale = {}\n", f(compiled.scales[i]));
        }
        out += &format!("{indent}\tdo_not_discard = true\n");
        if !children.is_empty() {
            out += &format!("{indent}\tchildren = \n{indent}\t[\n{}\n{indent}\t]\n", children.join(",\n"));
        }
        out + &format!("{indent}}}")
    }
    // Each added bone goes under its parent: another added bone (rendered with
    // it), a bone of the source, or the Skeleton for roots.
    let mut groups: BTreeMap<usize, (bool, String, Vec<String>)> = BTreeMap::new();
    for &i in &wanted {
        let parent = usize::try_from(compiled.parents[i]).ok();
        if parent.is_some_and(|p| added.contains(&p)) {
            continue;
        }
        let holder = parent
            .and_then(|p| bone_nodes.get(&compiled.names[p]).or_else(|| alias(&compiled.names[p]).and_then(|a| bone_nodes.get(&a))))
            .copied()
            .unwrap_or(skeleton);
        let array = holder.get("children").filter(|c| c.array().is_some());
        let (point, has_array, indent) = match array {
            Some(children) => (children.end - 1, true, indent_at(text, children.start) + "\t"),
            None => (holder.end - 1, false, indent_at(text, holder.start) + "\t\t"),
        };
        groups.entry(point).or_insert_with(|| (has_array, indent.clone(), vec![])).2.push(render(i, compiled, &added, &indent));
    }
    let edits: Vec<deadlock_resource::text::Edit> = groups
        .into_iter()
        .map(|(point, (has_array, indent, nodes))| {
            let outer = &indent[..indent.len() - 1];
            let replacement = if has_array {
                let before = text[..point].trim_end();
                let comma = if before.ends_with('[') || before.ends_with(',') { "" } else { "," };
                format!("{comma}\n{}\n{outer}", nodes.join(",\n"))
            } else {
                format!("\n{outer}children = \n{outer}[\n{}\n{outer}]\n{}", nodes.join(",\n"), &outer[..outer.len() - 1])
            };
            deadlock_resource::text::Edit { start: point, end: point, replacement }
        })
        .collect();
    Ok((document.apply(&edits)?, wanted.len()))
}

/// Source's angles back to the quaternion (the compiler's AngleQuaternion: yaw
/// about Z, pitch about Y, roll about X); the inverse of `angles_of`.
pub fn quaternion_of(angles: [f64; 3]) -> [f64; 4] {
    let [(sp, cp), (sy, cy), (sr, cr)] = angles.map(|a| (a.to_radians() * 0.5).sin_cos());
    [
        sr * cp * cy - cr * sp * sy,
        cr * sp * cy + sr * cp * sy,
        cr * cp * sy - sr * sp * cy,
        cr * cp * cy + sr * sp * sy,
    ]
}

/// Bones of a source Skeleton whose origin or angles drift from the game's
/// compiled skeleton get the compiled values. The compiler takes a Skeleton
/// node's transforms over the DMX's, and decompiles from October 2026 write
/// them around 0.002 inches off the compiled model: the compiled replacement
/// then misses the exact bind pose that restoring the game's cloth requires
/// ("Bind transform outside original-physics transfer budget"). Older
/// decompiles match the compiled values and are left alone. Returns how many
/// bones were changed.
pub fn with_compiled_bone_transforms(text: &str, compiled: &deadlock_skeleton::Skeleton) -> Result<(String, usize)> {
    use deadlock_resource::text::{Edit, Node};
    let document = Document::parse(text)?;
    let index: BTreeMap<&str, usize> = compiled.names.iter().enumerate().map(|(i, n)| (n.as_str(), i)).collect();
    fn triple(node: &Node) -> Option<[f64; 3]> {
        let [a, b, c] = node.array()? else { return None };
        let v = |n: &Node| n.scalar()?.trim().parse::<f64>().ok();
        Some([v(a)?, v(b)?, v(c)?])
    }
    let f = number;
    let mut edits = vec![];
    let mut aligned = 0;
    for node in document.objects(Some("Bone")) {
        let Some(name) = get(node, "name") else { continue };
        let found = index.get(name).or_else(|| index.get(format!("${}", name.strip_prefix('_')?).as_str()));
        let Some(&i) = found else { continue };
        let (Some(origin), Some(angles)) = (node.get("origin"), node.get("angles")) else { continue };
        let (Some(o), Some(a)) = (triple(origin), triple(angles)) else { continue };
        let (p, q, s) = (compiled.positions[i], compiled.rotations[i], quaternion_of(a));
        let drift_p = (0..3).map(|k| (o[k] - p[k]).abs()).fold(0., f64::max);
        let drift_q = [1., -1.]
            .into_iter()
            .map(|sign| (0..4).map(|k| (s[k] * sign - q[k]).abs()).fold(0., f64::max))
            .fold(f64::MAX, f64::min);
        if drift_p <= 1e-4 && drift_q <= 1e-5 {
            continue;
        }
        let c = angles_of(q);
        edits.push(Edit { start: origin.start, end: origin.end, replacement: format!("[ {}, {}, {} ]", f(p[0]), f(p[1]), f(p[2])) });
        edits.push(Edit { start: angles.start, end: angles.end, replacement: format!("[ {}, {}, {} ]", f(c[0]), f(c[1]), f(c[2])) });
        aligned += 1;
    }
    if aligned == 0 {
        return Ok((text.to_owned(), 0));
    }
    Ok((document.apply(&edits)?, aligned))
}

#[cfg(test)]
mod cloth_bone_tests {
    use super::*;
    #[test]
    fn angles_and_quaternions_round_trip() {
        for q in [[0.6335, 0.2324, -0.1667, 0.7189], [-0.6908, 0.0323, -0.4398, 0.5732], [0., 0.7071, 0., 0.7071], [0.1, 0.2, 0.3, 0.9]] {
            let n = q.iter().map(|x| x * x).sum::<f64>().sqrt();
            let q = q.map(|x| x / n);
            let back = quaternion_of(angles_of(q));
            let sign = if (back[3] - q[3]).abs() <= (back[3] + q[3]).abs() { 1. } else { -1. };
            for k in 0..4 {
                assert!((back[k] * sign - q[k]).abs() < 1e-9, "{q:?} -> {back:?}");
            }
        }
    }
    #[test]
    fn drifted_bone_transforms_take_the_compiled_values() {
        let text = format!(
            "{}\n{{\n\trootNode = \n\t{{\n\t\t_class = \"RootNode\"\n\t\tchildren = \n\t\t[\n\t\t\t{{\n\t\t\t\t_class = \"Skeleton\"\n\t\t\t\tchildren = \n\t\t\t\t[\n\t\t\t\t\t{{\n\t\t\t\t\t\t_class = \"Bone\"\n\t\t\t\t\t\tname = \"pelvis\"\n\t\t\t\t\t\torigin = [ 0, 0.000013, 40 ]\n\t\t\t\t\t\tangles = [ 0, 30.000001, 0 ]\n\t\t\t\t\t\tchildren = \n\t\t\t\t\t\t[\n\t\t\t\t\t\t\t{{\n\t\t\t\t\t\t\t\t_class = \"Bone\"\n\t\t\t\t\t\t\t\tname = \"fur_Lft_4\"\n\t\t\t\t\t\t\t\torigin = [ 5.947852, 0.000069, -0.000219 ]\n\t\t\t\t\t\t\t\tangles = [ 33.051521, 3.739185, 83.88076 ]\n\t\t\t\t\t\t\t}},\n\t\t\t\t\t\t\t{{\n\t\t\t\t\t\t\t\t_class = \"Bone\"\n\t\t\t\t\t\t\t\tname = \"_cloth_tip\"\n\t\t\t\t\t\t\t\torigin = [ 1, 0, 0 ]\n\t\t\t\t\t\t\t\tangles = [ 0, 0, 0 ]\n\t\t\t\t\t\t\t}},\n\t\t\t\t\t\t\t{{\n\t\t\t\t\t\t\t\t_class = \"Bone\"\n\t\t\t\t\t\t\t\tname = \"not_in_the_game\"\n\t\t\t\t\t\t\t\torigin = [ 9, 9, 9 ]\n\t\t\t\t\t\t\t\tangles = [ 9, 9, 9 ]\n\t\t\t\t\t\t\t}},\n\t\t\t\t\t\t]\n\t\t\t\t\t}},\n\t\t\t\t]\n\t\t\t}},\n\t\t]\n\t}}\n}}\n",
            header(28).unwrap()
        );
        let fur = [0.6335045695304871, 0.2323744148015976, -0.1667453795671463, 0.7189367413520813];
        let compiled = deadlock_skeleton::Skeleton {
            names: vec!["pelvis".into(), "fur_Lft_4".into(), "$cloth_tip".into()],
            parents: vec![-1, 0, 0],
            positions: vec![[0., 0., 40.], [5.94635009765625, -1.0013580322265625e-5, 6.198883056640625e-5], [1., 0., 0.5]],
            rotations: vec![quaternion_of([0., 30., 0.]), fur, [0., 0., 0., 1.]],
            scales: vec![1.; 3],
            flags: vec![0x1cc0, 0x401cc8, 0x401c88],
        };
        let (out, aligned) = with_compiled_bone_transforms(&text, &compiled).unwrap();
        // The pelvis is within the budget; the fur bone and the aliased cloth bone drift.
        assert_eq!(aligned, 2, "{out}");
        assert!(out.contains("origin = [ 0, 0.000013, 40 ]"));
        assert!(out.contains("name = \"fur_Lft_4\"\n\t\t\t\t\t\t\t\torigin = [ 5.94635, -0.00001001358, 0.00006198883 ]\n\t\t\t\t\t\t\t\tangles = [ 33.051517, 3.7391846, 83.88077 ]"), "{out}");
        assert!(out.contains("name = \"_cloth_tip\"\n\t\t\t\t\t\t\t\torigin = [ 1, 0, 0.5 ]\n\t\t\t\t\t\t\t\tangles = [ 0, 0, 0 ]"), "{out}");
        assert!(out.contains("origin = [ 9, 9, 9 ]"));
        let doc = Document::parse(&out).unwrap();
        assert_eq!(crate::bone_hierarchy(&doc).unwrap()["fur_Lft_4"].parent.as_deref(), Some("pelvis"));
        // Aligned once, nothing drifts any more.
        assert_eq!(with_compiled_bone_transforms(&out, &compiled).unwrap().1, 0);
    }
    #[test]
    fn angles_match_source_2_viewer_output() {
        // Lady Geist's $cloth_m0p28: Source 2 Viewer wrote angles [-34.828644, -41.831959, -87.02153].
        let a = angles_of([-0.6908, 0.0323, -0.4398, 0.5732]);
        assert!((a[0] + 34.83).abs() < 0.2 && (a[1] + 41.83).abs() < 0.2 && (a[2] + 87.02).abs() < 0.2, "{a:?}");
        assert_eq!(angles_of([0., 0., 0., 1.]), [0., 0., 0.]);
    }
    #[test]
    fn compiled_cloth_bones_are_added_the_way_older_decompiles_listed_them() {
        let text = format!(
            "{}\n{{\n\trootNode = \n\t{{\n\t\t_class = \"RootNode\"\n\t\tchildren = \n\t\t[\n\t\t\t{{\n\t\t\t\t_class = \"Skeleton\"\n\t\t\t\tchildren = \n\t\t\t\t[\n\t\t\t\t\t{{\n\t\t\t\t\t\t_class = \"Bone\"\n\t\t\t\t\t\tname = \"pelvis\"\n\t\t\t\t\t\torigin = [ 0, 0, 40 ]\n\t\t\t\t\t\tangles = [ 0, 0, 0 ]\n\t\t\t\t\t}},\n\t\t\t\t]\n\t\t\t}},\n\t\t]\n\t}}\n}}\n",
            header(28).unwrap()
        );
        let compiled = deadlock_skeleton::Skeleton {
            names: vec!["pelvis".into(), "$cloth_m0p0".into(), "$cloth_m0p1".into(), "$cloth_tip".into(), "$cloth_under".into()],
            parents: vec![-1, -1, -1, 2, 0],
            positions: vec![[0., 0., 40.], [5.5, 1., 50.], [6., 1., 49.], [0., 0., 2.], [1., 0., 0.]],
            rotations: vec![[0., 0., 0., 1.]; 5],
            scales: vec![1.; 5],
            flags: vec![0x1cc0, 0x400c88, 0x400c88, 0x401c88, 0x400c88],
        };
        let (out, added) = with_compiled_cloth_bones(&text, &compiled).unwrap();
        assert_eq!(added, 4);
        let doc = Document::parse(&out).unwrap();
        let bones = crate::bone_hierarchy(&doc).unwrap();
        assert_eq!(bones["_cloth_m0p0"].parent, None);
        assert_eq!(bones["_cloth_tip"].parent.as_deref(), Some("_cloth_m0p1"));
        assert_eq!(bones["_cloth_under"].parent.as_deref(), Some("pelvis"));
        assert!(out.contains("name = \"_cloth_m0p0\"\n\t\t\t\t\t\torigin = [ 5.5, 1, 50 ]"), "{out}");
        assert!(out.contains("do_not_discard = true"));
        // The Skeleton list already ended with a comma: the compiler rejects a second one.
        assert!(!regex(r",\s*,").is_match(&out), "{out}");
        assert!(!regex(r"\}\s*\{").is_match(&out), "{out}");
        // Already listed (either spelling): nothing is added twice.
        assert_eq!(with_compiled_cloth_bones(&out, &compiled).unwrap().1, 0);
        // The bones then pass the compile template check through the usual aliases.
        let report = deadlock_skeleton::source_skeleton::SourceAuditReport {
            compiled_sha256: Some("a".repeat(64)),
            runtime_motion: deadlock_skeleton::source_skeleton::RuntimeMotionSourceAudit {
                status: "audited".into(), resource_sha256: Some("a".repeat(64)), cloth_nodes: 1, controls: vec![], skeleton: Some(compiled.clone()),
            },
        };
        let aliases = deadlock_skeleton::source_skeleton::canonical_aliases(&report).unwrap();
        assert_eq!(aliases.get("_cloth_tip").map(String::as_str), Some("$cloth_tip"));
    }
}
