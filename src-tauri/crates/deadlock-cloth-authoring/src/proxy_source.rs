//! Add native ModelDoc cloth authoring without replacing a character of source.
use crate::{hash, invalid, Result};
use deadlock_model::modeldoc;
use deadlock_project::asset_paths::relative_asset;
use deadlock_resource::text::{Document, Edit, Node};
use deadlock_skeleton::{transforms, Skeleton};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeSet, path::Path};
pub const POLICY: &str = "direct-cloth-proxy-source-v1";
pub const UNITS: &str = "Source 2 model inches";
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ColliderProposal {
    pub bone: String,
    pub tip_bone: String,
    pub endpoints_local: [[f64; 3]; 2],
    pub endpoints_world: [[f64; 3]; 2],
    pub radius_local: f64,
    pub radius_world: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Analysis {
    pub units: String,
    pub millimeters_per_unit: f64,
    pub leg_colliders: Vec<ColliderProposal>,
    #[serde(default)]
    pub collision_disabled: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CapsuleReceipt {
    pub name: String,
    pub bone: String,
    pub endpoints_local: [[f64; 3]; 2],
    pub radius_local: f64,
    pub endpoints_world: [[f64; 3]; 2],
    pub radius_world: f64,
    pub accumulated_bind_scale: f64,
    pub coordinates_baked: bool,
    pub fit_approved: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Segment {
    pub source: [usize; 2],
    pub authored: [usize; 2],
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PreservedSpans {
    pub offset_units: String,
    pub segments: Vec<Segment>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Receipt {
    pub policy: String,
    pub source_format: u32,
    pub source_header_changed: bool,
    pub source_sha256: String,
    pub authored_sha256: String,
    pub proxy_asset: String,
    pub proxy_node: String,
    pub binding: String,
    pub original_source_text_preserved: bool,
    pub preserved_source_spans: PreservedSpans,
    pub cloth_containers: Vec<String>,
    pub native_format_migration_pending: bool,
    pub capsules: Vec<CapsuleReceipt>,
    pub units: String,
    pub millimeters_per_unit: f64,
    pub native_compiled: bool,
    pub runtime_ready: bool,
    pub fit_approved: bool,
    pub original_solver_preservation_verified: bool,
    pub pending_checks: Vec<String>,
}
#[derive(Clone, Debug)]
enum Field {
    Scalar(Value),
    Object(Fields),
    Children(Vec<Fields>),
}
type Fields = Vec<(String, Field)>;
fn field(key: &str, value: impl Into<Value>) -> (String, Field) {
    (key.into(), Field::Scalar(value.into()))
}
fn string<'a>(node: &'a Node, key: &str) -> Option<&'a str> {
    node.get(key).and_then(Node::scalar)
}
pub fn supported_source_format(source: &str) -> Result<u32> {
    let version = modeldoc::source_format(source)?;
    if ![28, 40, 41].contains(&version) {
        return Err(invalid(
            "Direct cloth authoring supports ModelDoc28/40/41; the header will not be relabelled.",
        ));
    }
    Ok(version)
}
fn proxy(path: &str, name: &str, version: u32) -> Fields {
    let mut fields = vec![
        field("_class", "ClothProxyMeshFile"),
        field("name", name),
        field("filename", path),
        field("import_scale", 1.0),
        field("back_solve_joints", false),
        field("back_solve_joints_drive_meshes", false),
        field("flex_cloth_borders", false),
        field("add_bones_to_render_mesh", true),
        field("back_solve_influence_threshold", 0.05),
        field("cloth_friction_bias", 0.0),
        field("cloth_friction_scale", 1.0),
        field("lock_friction_0", false),
        field("lock_friction_1", false),
        field("envelope_inches", 1000.0),
        field("cloth_goal_strength_bias", 0.0),
        field("cloth_goal_strength_scale", 1.0),
        field("lock_goal_strength_0", false),
        field("lock_goal_strength_1", false),
    ];
    for key in [
        "drag",
        "mass",
        "gravity",
        "collision_radius",
        "ground_collision",
        "ground_friction",
        "use_rods",
        "make_rods",
        "anchor_free_rotate",
        "volumetric",
        "suspenders",
        "bend_stiffness",
        "stray_radius_inv",
        "stray_radius",
        "stray_radius_stretchiness",
    ] {
        fields.push(field(&format!("cloth_{key}_scale"), 1.0));
    }
    fields.push((
        "import_filter".into(),
        Field::Object(vec![
            field("exclude_by_default", false),
            field("exception_list", json!([])),
        ]),
    ));
    if version == 28 {
        fields.retain(|(key, _)| {
            ![
                "lock_friction_0",
                "lock_friction_1",
                "lock_goal_strength_0",
                "lock_goal_strength_1",
            ]
            .contains(&key.as_str())
        });
    }
    fields
}
fn number(value: f64, positive: bool) -> Result<f64> {
    let packed = value as f32;
    if !value.is_finite() || !packed.is_finite() || (positive && (value <= 0. || packed <= 0.)) {
        return Err(invalid(
            "Cloth collider value is outside finite native float32 range.",
        ));
    }
    Ok(value)
}
fn capsules(
    analysis: &Analysis,
    skeleton: &Skeleton,
    prefix: &str,
    version: u32,
) -> Result<(Vec<Fields>, Vec<CapsuleReceipt>)> {
    if analysis.units != UNITS
        || analysis.millimeters_per_unit != 25.4
        || analysis.leg_colliders.is_empty()
    {
        return Err(invalid(
            "Cloth authoring requires measured bone-bound colliders in Source 2 model inches.",
        ));
    }
    let state = transforms::bind_state(skeleton)?;
    let indices = skeleton.indices();
    let mut nodes = vec![];
    let mut receipts = vec![];
    for (slot, item) in analysis.leg_colliders.iter().enumerate() {
        let &index = indices
            .get(item.bone.as_str())
            .ok_or_else(|| invalid("Cloth collider parent is absent from the source skeleton."))?;
        if !indices.contains_key(item.tip_bone.as_str()) {
            return Err(invalid(
                "Cloth collider tip is absent from the source skeleton.",
            ));
        }
        for &value in item
            .endpoints_local
            .iter()
            .flatten()
            .chain(item.endpoints_world.iter().flatten())
        {
            number(value, false)?;
        }
        let radius = number(item.radius_local, true)?;
        number(item.radius_world, true)?;
        let length = item.endpoints_local[0]
            .iter()
            .zip(item.endpoints_local[1])
            .map(|(a, b)| (b - a).powi(2))
            .sum::<f64>()
            .sqrt();
        if length <= 1e-6
            || item.endpoints_local[0].map(|v| v as f32)
                == item.endpoints_local[1].map(|v| v as f32)
        {
            return Err(invalid("Cloth capsule endpoints must be distinct."));
        }
        let expected = item.endpoints_local.map(|point| {
            std::array::from_fn(|row| {
                state.world[index][row][3]
                    + (0..3)
                        .map(|k| state.world[index][row][k] * point[k])
                        .sum::<f64>()
            })
        });
        let expected_radius = radius * state.accumulated[index];
        if !expected_radius.is_finite()
            || expected.iter().flatten().any(|v| !v.is_finite())
            || expected
                .iter()
                .flatten()
                .zip(item.endpoints_world.iter().flatten())
                .any(|(a, b)| (a - b).abs() > 1e-6 + 1e-7 * b.abs())
            || (expected_radius - item.radius_world).abs()
                > 1e-6_f64.max(1e-7 * expected_radius.abs().max(item.radius_world.abs()))
        {
            return Err(invalid(
                "Cloth collider local/world bind data is stale or inconsistent.",
            ));
        }
        let name = format!("{prefix}_capsule_{}", slot + 1);
        let mut fields = vec![
            field("_class", "ClothShapeCapsule"),
            field("name", name.clone()),
            field("parent_bone", item.bone.clone()),
        ];
        for i in 0..4 {
            fields.push(field(&format!("cloth_collision_layer{i}"), true));
        }
        fields.extend([
            field("cloth_collision_priority", 0),
            field("vertex_map", ""),
            field("inverted_collision", false),
            field("planarize", false),
            field("bounciness", 0.0),
            field("radius0", radius),
            field("radius1", radius),
            field("point0", json!(item.endpoints_local[0])),
            field("point1", json!(item.endpoints_local[1])),
        ]);
        if version == 28 {
            fields.retain(|(key, _)| {
                ![
                    "vertex_map",
                    "inverted_collision",
                    "planarize",
                    "bounciness",
                ]
                .contains(&key.as_str())
            });
            fields.push(field("cloth_stickiness", 0.0));
        }
        nodes.push(fields);
        receipts.push(CapsuleReceipt {
            name,
            bone: item.bone.clone(),
            endpoints_local: item.endpoints_local,
            radius_local: radius,
            endpoints_world: expected,
            radius_world: expected_radius,
            accumulated_bind_scale: state.accumulated[index],
            coordinates_baked: false,
            fit_approved: false,
        });
    }
    Ok((nodes, receipts))
}
fn literal(value: &Value) -> String {
    match value {
        Value::Array(v) => format!("[{}]", v.iter().map(literal).collect::<Vec<_>>().join(", ")),
        _ => serde_json::to_string(value).expect("finite validated scalar"),
    }
}
fn object(fields: &Fields, indent: &str, newline: &str) -> String {
    let mut lines = vec![format!("{indent}{{")];
    for (key, value) in fields {
        match value {
            Field::Object(v) => {
                lines.push(format!("{indent}\t{key} ="));
                lines.push(object(v, &format!("{indent}\t"), newline));
            }
            Field::Children(v) => {
                lines.push(format!("{indent}\tchildren ="));
                lines.push(format!("{indent}\t["));
                for child in v {
                    lines.push(format!(
                        "{},",
                        object(child, &format!("{indent}\t\t"), newline)
                    ));
                }
                lines.push(format!("{indent}\t]"));
            }
            Field::Scalar(v) => lines.push(format!("{indent}\t{key} = {}", literal(v))),
        }
    }
    lines.push(format!("{indent}}}"));
    lines.join(newline)
}
fn comma_outside_comments(mut tail: &str) -> bool {
    while !tail.is_empty() {
        if tail.starts_with("//") {
            tail = &tail[tail.find('\n').unwrap_or(tail.len())..];
        } else if tail.starts_with("/*") {
            let Some(stop) = tail[2..].find("*/") else {
                return false;
            };
            tail = &tail[stop + 4..];
        } else if tail.starts_with("<!--") {
            let Some(stop) = tail[4..].find("-->") else {
                return false;
            };
            tail = &tail[stop + 7..];
        } else {
            let c = tail.chars().next().unwrap();
            if c == ',' {
                return true;
            }
            tail = &tail[c.len_utf8()..];
        }
    }
    false
}
fn append(
    doc: &Document,
    owner: &Node,
    nodes: &[Fields],
    indent: &str,
    newline: &str,
) -> Result<Edit> {
    let body = nodes
        .iter()
        .map(|node| object(node, indent, newline))
        .collect::<Vec<_>>()
        .join(&format!(",{newline}"))
        + ",";
    let Some(children) = owner.get("children") else {
        return Ok(Edit {
            start: owner.end - 1,
            end: owner.end - 1,
            replacement: format!(
                "{newline}{}children = [{newline}{body}{newline}{}]{newline}",
                &indent[..indent.len() - 1],
                &indent[..indent.len() - 1]
            ),
        });
    };
    let values = children
        .array()
        .ok_or_else(|| invalid("Cloth authoring requires an object children array."))?;
    if values.iter().any(|n| n.object().is_none()) {
        return Err(invalid(
            "Cloth authoring requires an object children array.",
        ));
    }
    let sep = if values
        .last()
        .is_some_and(|v| !comma_outside_comments(&doc.source[v.end..children.end - 1]))
    {
        ","
    } else {
        ""
    };
    Ok(Edit {
        start: children.end - 1,
        end: children.end - 1,
        replacement: format!(
            "{sep}{newline}{body}{newline}{}",
            &indent[..indent.len() - 1]
        ),
    })
}
pub fn author_direct_proxy(
    source: &str,
    proxy_asset: &str,
    analysis: &Analysis,
    skeleton: &Skeleton,
    name_prefix: &str,
    allow_no_colliders: bool,
) -> Result<(String, Receipt)> {
    let version = supported_source_format(source)?;
    let path = relative_asset(proxy_asset)?;
    if Path::new(&path)
        .extension()
        .and_then(|s| s.to_str())
        .is_none_or(|s| !s.eq_ignore_ascii_case("dmx"))
    {
        return Err(invalid("Cloth proxy must reference a DMX source."));
    }
    if name_prefix.is_empty()
        || name_prefix.len() > 64
        || !name_prefix
            .bytes()
            .enumerate()
            .all(|(i, c)| c == b'_' || c.is_ascii_alphabetic() || (i > 0 && c.is_ascii_digit()))
    {
        return Err(invalid(
            "Cloth authoring name prefix must be a simple identifier.",
        ));
    }
    let (capsule_nodes, capsule_report) =
        if allow_no_colliders && analysis.collision_disabled && analysis.leg_colliders.is_empty() {
            (vec![], vec![])
        } else {
            capsules(analysis, skeleton, name_prefix, version)?
        };
    let proxy_node = format!("{name_prefix}_proxy");
    let mut nodes = vec![proxy(&path, &proxy_node, version)];
    nodes.extend(capsule_nodes.clone());
    let doc = Document::parse(source)?;
    let root = doc
        .root
        .get("rootNode")
        .filter(|n| string(n, "_class") == Some("RootNode"))
        .ok_or_else(|| invalid("Cloth authoring needs a ModelDoc RootNode."))?;
    let children = root
        .get("children")
        .and_then(Node::array)
        .ok_or_else(|| invalid("Cloth authoring needs a RootNode children array."))?;
    if children.iter().any(|n| n.object().is_none()) {
        return Err(invalid(
            "Cloth authoring needs a RootNode object children array.",
        ));
    }
    let owner = |class: &str| -> Result<Option<&Node>> {
        let existing = doc.objects(Some(class));
        if existing.len() > 1
            || existing
                .first()
                .is_some_and(|node| !children.contains(node))
        {
            return Err(invalid(format!(
                "Cloth authoring needs at most one direct RootNode {class}."
            )));
        }
        Ok(existing.first().copied())
    };
    let (proxy_owner, shape_owner, softbody_owner) = if version == 28 {
        if !doc.objects(Some("Softbody")).is_empty() {
            return Err(invalid(
                "ModelDoc28 cannot mix modern Softbody with legacy cloth containers.",
            ));
        }
        (owner("ClothProxyMeshList")?, owner("ClothShapeList")?, None)
    } else {
        if ["ClothProxyMeshList", "ClothShapeList"]
            .iter()
            .any(|s| !doc.objects(Some(s)).is_empty())
        {
            return Err(invalid(
                "Legacy cloth containers require native ModelDoc migration.",
            ));
        }
        (None, None, owner("Softbody")?)
    };
    let names: BTreeSet<_> = doc
        .objects(None)
        .iter()
        .filter_map(|n| string(n, "name"))
        .collect();
    if nodes.iter().any(|v| {
        v.iter().any(|(key, value)| {
            key == "name"
                && matches!(value,Field::Scalar(Value::String(s)) if names.contains(s.as_str()))
        })
    }) {
        return Err(invalid(
            "Generated cloth node names already exist; use a fresh source.",
        ));
    }
    for node in doc.objects(Some("ClothProxyMeshFile")) {
        if string(node, "filename").is_some_and(|s| {
            deadlock_core::text::casefold(&s.replace('\\', "/"))
                == deadlock_core::text::casefold(&path)
        }) {
            return Err(invalid("This cloth proxy source is already authored."));
        }
    }
    let newline = if source.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let mut edits = vec![];
    if version == 28 {
        let mut lists = vec![];
        for (class, owner, additions) in [
            ("ClothProxyMeshList", proxy_owner, &nodes[..1]),
            ("ClothShapeList", shape_owner, capsule_nodes.as_slice()),
        ] {
            if let Some(owner) = owner {
                if !additions.is_empty() {
                    edits.push(append(&doc, owner, additions, "\t\t\t\t\t", newline)?)
                }
            } else {
                lists.push(vec![
                    field("_class", class),
                    ("children".into(), Field::Children(additions.to_vec())),
                ])
            }
        }
        if !lists.is_empty() {
            edits.push(append(&doc, root, &lists, "\t\t\t", newline)?)
        }
    } else if let Some(owner) = softbody_owner {
        edits.push(append(&doc, owner, &nodes, "\t\t\t\t\t", newline)?)
    } else {
        let mut fields = vec![
            field("_class", "Softbody"),
            ("children".into(), Field::Children(nodes)),
            field("stiffness_on_ragdoll", 0.0),
            field("motion_smooth_cdt", 0.0),
        ];
        for key in [
            "cloth_sleep_enabled",
            "cloth_immovable_hint",
            "cloth_per_bone_scale_enabled",
            "cloth_enable_empty_model",
            "cloth_keychain_motion",
        ] {
            fields.push(field(key, false))
        }
        edits.push(append(&doc, root, &[fields], "\t\t\t", newline)?)
    }
    let output = doc.apply(&edits)?;
    edits.sort_by_key(|e| e.start);
    let mut segments = vec![];
    let (mut src, mut dst) = (0, 0);
    for edit in edits {
        if edit.start != edit.end {
            return Err(invalid(
                "Cloth authoring unexpectedly replaced source text.",
            ));
        }
        let end = dst + edit.start - src;
        if source[src..edit.start] != output[dst..end] {
            return Err(invalid("Existing cloth source text changed."));
        }
        segments.push(Segment {
            source: [
                source[..src].chars().count(),
                source[..edit.start].chars().count(),
            ],
            authored: [output[..dst].chars().count(), output[..end].chars().count()],
        });
        src = edit.start;
        dst = end + edit.replacement.len();
    }
    if source[src..] != output[dst..] {
        return Err(invalid("Existing cloth source tail changed."));
    }
    segments.push(Segment {
        source: [source[..src].chars().count(), source.chars().count()],
        authored: [output[..dst].chars().count(), output.chars().count()],
    });
    let receipt = Receipt {
        policy: POLICY.into(),
        source_format: version,
        source_header_changed: false,
        source_sha256: hash(source.as_bytes()),
        authored_sha256: hash(output.as_bytes()),
        proxy_asset: path,
        proxy_node,
        binding: "direct_proxy_generated_render_bones".into(),
        original_source_text_preserved: true,
        preserved_source_spans: PreservedSpans {
            offset_units: "unicode_codepoints".into(),
            segments,
        },
        cloth_containers: if version == 28 {
            vec!["ClothProxyMeshList".into(), "ClothShapeList".into()]
        } else {
            vec!["Softbody".into()]
        },
        native_format_migration_pending: version == 28,
        capsules: capsule_report,
        units: UNITS.into(),
        millimeters_per_unit: 25.4,
        native_compiled: false,
        runtime_ready: false,
        fit_approved: false,
        original_solver_preservation_verified: false,
        pending_checks: vec![
            "Render cloth mask and proxy correspondence.".into(),
            "Native capsule placement, inherited scale and collision fit.".into(),
            "Proxy and visible garment collision during animation.".into(),
            "Existing cloth collision-layer interactions and original hair preservation.".into(),
            "Native VPK and in-game validation.".into(),
        ],
    };
    Ok((output, receipt))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JointReceipt {
    pub policy: String,
    pub source_version: u32,
    pub proxy_name: String,
    pub proxy_asset: String,
    pub previous_switches: std::collections::BTreeMap<String, String>,
    pub switches: std::collections::BTreeMap<String, String>,
    pub render_source_weights_modified: bool,
    pub bind_transforms_modified: bool,
    pub generated_render_bones_requested: bool,
    pub native_compiled: bool,
    pub native_joint_fit_verified: bool,
    pub original_solver_preservation_verified: bool,
    pub runtime_ready: bool,
    pub build_authorized: bool,
}
pub fn configure_joint_driven_proxy(
    source: &str,
    proxy_name: &str,
) -> Result<(String, JointReceipt)> {
    if modeldoc::source_format(source)? != 28 {
        return Err(invalid(
            "Joint-driven diagnostics require the captured ModelDoc28 schema.",
        ));
    }
    configure_secondary_proxy(source, proxy_name)
}

pub fn configure_secondary_proxy(source: &str, proxy_name: &str) -> Result<(String, JointReceipt)> {
    supported_source_format(source)?;
    if proxy_name.is_empty() {
        return Err(invalid(
            "Select one named proxy for joint-driven diagnostics.",
        ));
    }
    let doc = Document::parse(source)?;
    let nodes = doc.objects(Some("ClothProxyMeshFile"));
    if nodes.len() != 1 || string(nodes[0], "name") != Some(proxy_name) {
        return Err(invalid(
            "Joint-driven diagnostic requires one unambiguous named proxy.",
        ));
    }
    let node = nodes[0];
    let path = relative_asset(string(node, "filename").unwrap_or_default())?;
    if Path::new(&path)
        .extension()
        .and_then(|s| s.to_str())
        .is_none_or(|s| !s.eq_ignore_ascii_case("dmx"))
    {
        return Err(invalid("Joint-driven diagnostic requires a DMX proxy."));
    }
    let mut previous = std::collections::BTreeMap::new();
    let mut switches = std::collections::BTreeMap::new();
    let mut edits = vec![];
    for (key, value) in [
        ("back_solve_joints", "true"),
        ("back_solve_joints_drive_meshes", "true"),
        ("add_bones_to_render_mesh", "false"),
    ] {
        let old = node
            .get(key)
            .ok_or_else(|| invalid(format!("Missing explicit native boolean: {key}")))?;
        if !["true", "false"].contains(&old.scalar().unwrap_or_default())
            || !["true", "false"].contains(&&source[old.start..old.end])
        {
            return Err(invalid(format!(
                "Joint-driven diagnostic requires an explicit native boolean: {key}"
            )));
        }
        previous.insert(key.into(), old.scalar().unwrap().into());
        switches.insert(key.into(), value.into());
        edits.push(Edit {
            start: old.start,
            end: old.end,
            replacement: value.into(),
        });
    }
    Ok((
        doc.apply(&edits)?,
        JointReceipt {
            policy: "existing-joint-cloth-fit-diagnostic-v1".into(),
            source_version: modeldoc::source_format(source)?,
            proxy_name: proxy_name.into(),
            proxy_asset: path,
            previous_switches: previous,
            switches,
            render_source_weights_modified: false,
            bind_transforms_modified: false,
            generated_render_bones_requested: false,
            native_compiled: false,
            native_joint_fit_verified: false,
            original_solver_preservation_verified: false,
            runtime_ready: false,
            build_authorized: false,
        },
    ))
}
