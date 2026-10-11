//! Automatic attachment on one original open surface. Never runtime approval.
use crate::{
    hash, invalid,
    physics::{self, OriginalGeometry},
    surface::{self, MeshData},
    Result,
};
use deadlock_core::CancellationToken;
use deadlock_dmx::Dmx;
use deadlock_project::customization::ClothSetup;
use deadlock_skeleton::{
    source_identity,
    source_skeleton::SourceAuditReport,
    transforms::{self, Matrix4},
    Skeleton,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
pub const POLICY: &str = "automatic-open-surface-cloth-v1";
pub type Options = ClothSetup;
pub use crate::surface::{Selection, Selections};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SurfaceReport {
    pub mesh: String,
    pub dynamic_positions: usize,
    pub pinned_positions: usize,
    pub attachment: String,
    pub attachment_band_fraction: f64,
    pub attachment_height_range: [f64; 2],
    pub source_positions: usize,
    pub connected_panels: usize,
}
pub fn validate_options(options: &Options) -> Result<()> {
    let meshes = options.meshes.as_deref().unwrap_or(&[]);
    if meshes.len() > 1
        || meshes
            .iter()
            .any(|s| s.trim().is_empty() || s.contains('\0'))
        || meshes.iter().collect::<BTreeSet<_>>().len() != meshes.len()
    {
        return Err(invalid(
            "Choose one garment mesh at a time for automatic cloth setup.",
        ));
    }
    if !["top", "bottom"].contains(&options.attachment.as_deref().unwrap_or("top")) {
        return Err(invalid(
            "Choose the top or bottom attachment edge for cloth setup.",
        ));
    }
    if !["legs", "none"].contains(&options.collision.as_deref().unwrap_or("legs")) {
        return Err(invalid(
            "Choose leg collision or no collision for cloth setup.",
        ));
    }
    Ok(())
}
fn subtract(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| a[i] - b[i])
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn bits(n: f64) -> u64 {
    if n == 0. {
        0
    } else {
        n.to_bits()
    }
}
pub fn surface_selection(
    source: &Dmx,
    options: &Options,
    cancel: &CancellationToken,
) -> Result<(Selections, Vec<SurfaceReport>, BTreeMap<String, MeshData>)> {
    validate_options(options)?;
    let chosen = options.meshes.as_deref().unwrap_or(&[]);
    if chosen.is_empty() {
        return Err(invalid("Choose the skirt, cape or cloth mesh in Motion first. Run Analyze to list the meshes in your replacement."));
    }
    // Mutable caller metadata does not override serialized source bytes.
    let dmx = Dmx::parse_with_cancel(source.bytes().to_vec(), cancel)?;
    surface::binds(&dmx)?;
    let rows = surface::mesh_data(&dmx)?;
    if chosen.iter().any(|n| !rows.contains_key(n)) {
        return Err(invalid(
            "A chosen cloth mesh is missing. Run Analyze and choose it again.",
        ));
    }
    let mut selection = BTreeMap::new();
    let mut reports = vec![];
    for name in chosen {
        cancel.check()?;
        let row = &rows[name];
        let points = &row.positions;
        if points.iter().flatten().any(|n| !n.is_finite()) {
            return Err(invalid(format!(
                "Cloth mesh has non-finite positions: {name}"
            )));
        }
        let triangles = surface::triangles(&dmx, row, cancel, None)?;
        let drawn = triangles
            .iter()
            .flatten()
            .copied()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        if drawn.len() > 60000 || triangles.len() > 240000 {
            return Err(invalid(format!("Cloth mesh is too dense for automatic setup: {name}. Use a separate, reviewed simulation surface; keep the visible mesh intact.")));
        }
        let mut lookup = BTreeMap::new();
        let mut representatives = vec![];
        let mut remap = vec![usize::MAX; points.len()];
        for (offset, &index) in drawn.iter().enumerate() {
            if offset % 4096 == 0 {
                cancel.check()?;
            }
            let mut influences = BTreeMap::<usize, f64>::new();
            for (&bone, &weight) in row.joints[index].iter().zip(&row.weights[index]) {
                if weight > 0. {
                    *influences.entry(bone).or_default() += weight;
                }
            }
            let key = (
                points[index].map(bits),
                influences
                    .into_iter()
                    .map(|(bone, weight)| (bone, bits(weight)))
                    .collect::<Vec<_>>(),
            );
            let next = representatives.len();
            let representative = *lookup.entry(key).or_insert_with(|| {
                representatives.push(index);
                next
            });
            remap[index] = representative;
        }
        let mut faces = BTreeSet::new();
        let mut edges = BTreeMap::<[usize; 2], usize>::new();
        for (index, triangle) in triangles.iter().enumerate() {
            if index % 4096 == 0 {
                cancel.check()?;
            }
            let mapped = triangle.map(|i| remap[i]);
            let mut sorted = mapped;
            sorted.sort_unstable();
            let normal = cross(
                subtract(points[triangle[1]], points[triangle[0]]),
                subtract(points[triangle[2]], points[triangle[0]]),
            );
            if sorted.windows(2).any(|w| w[0] == w[1]) || dot(normal, normal).sqrt() == 0. {
                return Err(invalid(format!(
                    "Cloth has collapsed surface faces: {name}. Repair them before setup."
                )));
            }
            if !faces.insert(sorted) {
                return Err(invalid(format!(
                    "Cloth has overlapping duplicate faces: {name}. Choose one simulation sheet."
                )));
            }
            for mut edge in [
                [mapped[0], mapped[1]],
                [mapped[1], mapped[2]],
                [mapped[2], mapped[0]],
            ] {
                edge.sort_unstable();
                *edges.entry(edge).or_default() += 1;
            }
        }
        if edges.values().any(|&n| n > 2) {
            return Err(invalid(format!(
                "Cloth has an ambiguous joined surface: {name}. Use a single open sheet."
            )));
        }
        let boundary = edges
            .iter()
            .filter_map(|(edge, &count)| (count == 1).then_some(*edge))
            .collect::<Vec<_>>();
        if boundary.is_empty() {
            return Err(invalid(format!("Cloth is a closed or thick shell: {name}. Automatic setup needs an open simulation sheet with an attachment edge.")));
        }
        let mut adjacency = vec![vec![]; representatives.len()];
        for [a, b] in edges.keys() {
            adjacency[*a].push(*b);
            adjacency[*b].push(*a);
        }
        let mut seen = BTreeSet::new();
        let mut pending = vec![0];
        while let Some(node) = pending.pop() {
            if seen.insert(node) {
                pending.extend(adjacency[node].iter().filter(|n| !seen.contains(n)));
                if seen.len() % 4096 == 0 {
                    cancel.check()?;
                }
            }
        }
        if seen.len() != representatives.len() {
            return Err(invalid(format!("Cloth contains disconnected panels or decorations: {name}. Separate the intended cloth sheet from trim before automatic setup.")));
        }
        let height = representatives
            .iter()
            .map(|&i| points[i][2])
            .collect::<Vec<_>>();
        let minimum = height.iter().copied().fold(f64::INFINITY, f64::min);
        let maximum = height.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let span = maximum - minimum;
        if span <= 1e-6 {
            return Err(invalid(format!("Cloth has no vertical attachment-to-hem span: {name}. This setup supports hanging cloth in a Z-up rigged model.")));
        }
        let attachment = options.attachment.as_deref().unwrap_or("top");
        let pins = height
            .iter()
            .map(|&h| {
                if attachment == "top" {
                    h >= maximum - 0.1 * span
                } else {
                    h <= minimum + 0.1 * span
                }
            })
            .collect::<Vec<_>>();
        let count = pins.iter().filter(|&&p| p).count();
        if !boundary.iter().any(|[a, b]| pins[*a] && pins[*b])
            || count * 2 > pins.len()
            || count < 2
            || count == pins.len()
        {
            return Err(invalid(format!("No clear attachment edge was found for {name}. Choose the other edge or prepare an explicit cloth mask.")));
        }
        let mut pinned = vec![];
        let mut enabled = vec![];
        for &index in &drawn {
            if pins[remap[index]] {
                pinned.push(index);
            } else {
                enabled.push(index);
            }
        }
        let pin_height = height
            .iter()
            .zip(&pins)
            .filter_map(|(&h, &pin)| pin.then_some(h))
            .collect::<Vec<_>>();
        reports.push(SurfaceReport {
            mesh: name.clone(),
            dynamic_positions: enabled.len(),
            pinned_positions: pinned.len(),
            attachment: attachment.into(),
            attachment_band_fraction: 0.1,
            attachment_height_range: [
                pin_height.iter().copied().fold(f64::INFINITY, f64::min),
                pin_height.iter().copied().fold(f64::NEG_INFINITY, f64::max),
            ],
            source_positions: drawn.len(),
            connected_panels: 1,
        });
        selection.insert(name.clone(), Selection { enabled, pinned });
    }
    Ok((selection, reports, rows))
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Collider {
    pub bone: String,
    pub tip_bone: String,
    pub origin: String,
    pub endpoints_local: [[f64; 3]; 2],
    pub radius_local: f64,
    pub endpoints_world: [[f64; 3]; 2],
    pub radius_world: f64,
    pub fit_approved: bool,
}
fn transform(matrix: &Matrix4, point: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| {
        matrix[i][0] * point[0] + matrix[i][1] * point[1] + matrix[i][2] * point[2] + matrix[i][3]
    })
}
pub fn leg_proposals(
    original: &OriginalGeometry,
    skeleton: &Skeleton,
    rows: &BTreeMap<String, MeshData>,
    selected: &Selections,
    palette_names: &[String],
    cancel: &CancellationToken,
) -> Result<Vec<Collider>> {
    let bind = transforms::bind_state(skeleton)?;
    let original_names = original
        .state
        .skeleton
        .names
        .iter()
        .collect::<BTreeSet<_>>();
    let mut roles = BTreeMap::new();
    for role in ["thigh_L", "shin_L", "foot_L", "thigh_R", "shin_R", "foot_R"] {
        let candidates = skeleton
            .names
            .iter()
            .filter(|name| {
                original_names.contains(name)
                    && deadlock_preparation::algorithms::name_role(name) == Some(role)
            })
            .collect::<Vec<_>>();
        if candidates.len() != 1 {
            return Err(invalid(format!("Leg collision needs a clear {role} bone. Choose No collision to create a movement-only preview for this rig.")));
        }
        roles.insert(role, candidates[0]);
    }
    let mut colliders = vec![];
    for side in ["L", "R"] {
        for (start, end) in [("thigh", "shin"), ("shin", "foot")] {
            cancel.check()?;
            let bone = roles[format!("{start}_{side}").as_str()];
            let tip = roles[format!("{end}_{side}").as_str()];
            let index = skeleton.names.iter().position(|n| n == bone).unwrap();
            let tip_index = skeleton.names.iter().position(|n| n == tip).unwrap();
            let mut at = tip_index as i32;
            while at >= 0 && at != index as i32 {
                at = skeleton.parents[at as usize];
            }
            if at != index as i32 {
                return Err(invalid(format!(
                    "Leg collision landmarks have an incompatible hierarchy: {bone}"
                )));
            }
            let a = std::array::from_fn(|i| bind.world[index][i][3]);
            let b = std::array::from_fn(|i| bind.world[tip_index][i][3]);
            let delta = subtract(b, a);
            let length2 = dot(delta, delta);
            if length2 <= 1e-12 {
                return Err(invalid(format!(
                    "Leg collision landmarks occupy the same point: {bone}"
                )));
            }
            let mut proposals = physics::ragdoll_capsules(Some(original), bone);
            let mut origin = "original_ragdoll_capsule";
            if proposals.is_empty() {
                let slot = palette_names
                    .iter()
                    .position(|n| n == bone)
                    .ok_or_else(|| {
                        invalid(format!(
                            "Leg bone is missing from the source palette: {bone}"
                        ))
                    })?;
                let mut samples = vec![];
                for (name, row) in rows {
                    if selected.contains_key(name) {
                        continue;
                    }
                    for (index, &drawn) in row.drawn.iter().enumerate() {
                        if index % 65536 == 0 {
                            cancel.check()?;
                        }
                        if !drawn {
                            continue;
                        }
                        let point = row.positions[index];
                        let t = dot(subtract(point, a), delta) / length2;
                        let weight = row.joints[index]
                            .iter()
                            .zip(&row.weights[index])
                            .filter_map(|(&j, &w)| (j == slot).then_some(w))
                            .sum::<f64>();
                        if weight >= 0.5 && (0.1..=0.9).contains(&t) {
                            samples.push(physics::distance(point, a, b)?);
                        }
                    }
                }
                if samples.is_empty() {
                    return Err(invalid(format!("No reliable body geometry was found to fit collision for {bone}. Include the body mesh or choose No collision for the preview.")));
                }
                samples.sort_by(f64::total_cmp);
                let position = (samples.len() - 1) as f64 * 0.95;
                let low = position.floor() as usize;
                let high = position.ceil() as usize;
                let radius =
                    samples[low] + (samples[high] - samples[low]) * (position - low as f64);
                if !radius.is_finite() || radius <= 1e-6 {
                    return Err(invalid(format!(
                        "Cannot measure a useful leg radius for {bone}"
                    )));
                }
                proposals.push(physics::Capsule {
                    endpoints: [
                        transform(&bind.inverse[index], a),
                        transform(&bind.inverse[index], b),
                    ],
                    radius: radius / bind.accumulated[index],
                });
                origin = "dominant_weight_geometry_p95_proposal";
            }
            for capsule in proposals {
                colliders.push(Collider {
                    bone: bone.clone(),
                    tip_bone: tip.clone(),
                    origin: origin.into(),
                    endpoints_local: capsule.endpoints,
                    radius_local: capsule.radius,
                    endpoints_world: capsule.endpoints.map(|p| transform(&bind.world[index], p)),
                    radius_world: capsule.radius * bind.accumulated[index],
                    fit_approved: false,
                });
            }
        }
    }
    Ok(colliders)
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Summary {
    pub garment_mesh_count: usize,
    pub pinned_position_count: usize,
    pub dynamic_position_count: usize,
    pub leg_collider_count: usize,
    pub overlapping_position_count: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AutomaticReport {
    pub kind: String,
    pub policy: String,
    pub status: String,
    pub source_sha256: String,
    pub original_sha256: String,
    pub original_snapshot_verified: bool,
    pub units: String,
    pub millimeters_per_unit: f64,
    pub options: Options,
    pub surfaces: Vec<SurfaceReport>,
    pub leg_colliders: Vec<Collider>,
    pub collision_disabled: bool,
    pub summary: Summary,
    pub blockers: Vec<String>,
    pub warnings: Vec<String>,
    pub source_modified: bool,
    pub weights_modified: bool,
    pub runtime_ready: bool,
    pub published: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub analysis_sha256: String,
    pub analysis_fingerprint_algorithm: String,
}
pub struct Analysis {
    pub report: AutomaticReport,
    pub selection: Selections,
    pub skeleton: Skeleton,
}
pub fn analyze(
    source: &Dmx,
    options: &Options,
    original: &OriginalGeometry,
    audit: &SourceAuditReport,
    cancel: &CancellationToken,
) -> Result<Analysis> {
    let (selection, surfaces, rows) = surface_selection(source, options, cancel)?;
    let dmx = Dmx::parse_with_cancel(source.bytes().to_vec(), cancel)?;
    let binds = surface::binds(&dmx)?;
    let aliases = source_identity::inspect_identity(&dmx, audit)
        .source_aliases
        .unwrap_or_default();
    let mut skeleton = binds.skeleton;
    skeleton.names = skeleton
        .names
        .iter()
        .map(|n| aliases.get(n).unwrap_or(n).clone())
        .collect();
    skeleton.flags = vec![0; skeleton.names.len()];
    let reference = audit
        .runtime_motion
        .skeleton
        .as_ref()
        .ok_or_else(|| invalid("Select the current game original before setting up cloth."))?;
    let projected = deadlock_skeleton::extensions::project_skeleton(&skeleton, &reference.names)?;
    deadlock_skeleton::skeleton_identity(&projected, reference)?;
    transforms::audit_scale_preservation(&projected, reference, false)?;
    let names = binds
        .palette
        .iter()
        .map(|&i| {
            aliases
                .get(&dmx.elements[i].name)
                .unwrap_or(&dmx.elements[i].name)
                .clone()
        })
        .collect::<Vec<_>>();
    let colliders = if options.collision.as_deref().unwrap_or("legs") == "legs" {
        leg_proposals(original, &skeleton, &rows, &selection, &names, cancel)?
    } else {
        vec![]
    };
    let mut overlaps = 0;
    for (name, selected) in &selection {
        for (offset, &index) in selected.enabled.iter().enumerate() {
            if offset % 4096 == 0 {
                cancel.check()?;
            }
            let point = rows[name].positions[index];
            let mut inside = false;
            for collider in &colliders {
                inside |= physics::distance(
                    point,
                    collider.endpoints_world[0],
                    collider.endpoints_world[1],
                )? < collider.radius_world;
            }
            overlaps += usize::from(inside);
        }
    }
    let original_sha256 = audit
        .compiled_sha256
        .as_ref()
        .filter(|s| {
            s.len() == 64
                && s.bytes()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        })
        .ok_or_else(|| invalid("Cloth setup needs an audited original compiled-model SHA256."))?
        .clone();
    if audit.runtime_motion.resource_sha256.as_deref() != Some(&original_sha256) {
        return Err(invalid(
            "Cloth setup original audit digest differs from the compiled snapshot.",
        ));
    }
    let mut warnings = if colliders.is_empty() {
        vec!["No collision was selected; the cloth can pass through the body.".into()]
    } else {
        vec![
            "Attachment uses existing skin weights; inspect the pinned edge in animation.".into(),
            "Generated collision capsules require fit review in motion.".into(),
        ]
    };
    if overlaps > 0 {
        warnings.push(format!("Proposed leg capsules overlap {overlaps} cloth positions at rest; adjust their fit before testing."));
    }
    let mut report = AutomaticReport {
        kind: "automatic_cloth".into(),
        policy: POLICY.into(),
        status: "analyzed".into(),
        source_sha256: hash(dmx.bytes()),
        original_sha256,
        original_snapshot_verified: true,
        units: "Source 2 model inches".into(),
        millimeters_per_unit: 25.4,
        options: options.clone(),
        surfaces,
        collision_disabled: options.collision.as_deref().unwrap_or("legs") == "none",
        summary: Summary {
            garment_mesh_count: selection.len(),
            pinned_position_count: selection.values().map(|v| v.pinned.len()).sum(),
            dynamic_position_count: selection.values().map(|v| v.enabled.len()).sum(),
            leg_collider_count: colliders.len(),
            overlapping_position_count: overlaps,
        },
        leg_colliders: colliders,
        blockers: vec![],
        warnings,
        source_modified: false,
        weights_modified: false,
        runtime_ready: false,
        published: false,
        analysis_sha256: String::new(),
        analysis_fingerprint_algorithm: "rust-sorted-json-v1-sha256".into(),
    };
    let mut json = serde_json::to_value(&report)?;
    sort_json(&mut json);
    report.analysis_sha256 = hash(&serde_json::to_vec(&json)?);
    cancel.check()?;
    Ok(Analysis {
        report,
        selection,
        skeleton,
    })
}
fn sort_json(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            map.sort_keys();
            for value in map.values_mut() {
                sort_json(value);
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                sort_json(value);
            }
        }
        _ => {}
    }
}
