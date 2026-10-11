//! Explicit many-to-one adaptation. A following bone uses its ancestor's rest
//! transform, so its offset is preserved instead of snapping to a target pivot.
use crate::algorithms::Bone;
use deadlock_project::PreparationOptions;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const POLICY: &str = "ancestor-offset-preservation-v2";
pub(crate) fn facial_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    ["face", "eye", "brow", "lip", "mouth", "jaw", "teeth", "tooth", "tongue",
        "cheek", "nose", "顔", "瞳", "目", "眉", "唇", "歯", "瞼", "舌", "頬", "鼻", "顎"]
        .iter().any(|s| lower.contains(s))
        || lower.starts_with("頭") || lower.starts_with("耳")
}
fn dynamic(bone: &Bone) -> bool {
    bone.details.get("physics").is_some_and(|p| p["policy"] == crate::secondary_bones::BINDING_POLICY
        && p["dynamic_bodies"].as_array().is_some_and(|v| !v.is_empty()))
}
fn kinematic(bone: &Bone) -> bool {
    bone.details.get("physics").is_some_and(|p| p["policy"] == crate::secondary_bones::BINDING_POLICY
        && p["kinematic_bodies"].as_array().is_some_and(|v| !v.is_empty()))
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Proposal {
    pub anchor: String,
    pub target: String,
    pub group: String,
    pub automatic: bool,
    pub reason: String,
}
pub fn map(options: &PreparationOptions, key: &str) -> BTreeMap<String, String> {
    options
        .values
        .get(key)
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default()
}
pub fn ancestor(bones: &[Bone], anchor: &str, name: &str) -> bool {
    let by_name: BTreeMap<_, _> = bones.iter().map(|b| (b.name.as_str(), b)).collect();
    let mut next = Some(name);
    let mut seen = BTreeSet::new();
    while let Some(n) = next {
        if !seen.insert(n) {
            return false;
        }
        if n == anchor {
            return true;
        }
        next = by_name.get(n).and_then(|b| b.parent.as_deref());
    }
    false
}
/// True when no reviewed body role lies above `name` in the effective hierarchy.
pub fn role_orphan(bones: &[Bone], roles: &BTreeMap<String, String>, name: &str) -> bool {
    let by_name: BTreeMap<_, _> = bones.iter().map(|b| (b.name.as_str(), b)).collect();
    let mut next = by_name.get(name).and_then(|b| b.parent.as_deref());
    let mut seen = BTreeSet::from([name]);
    while let Some(n) = next {
        if !seen.insert(n) {
            return true;
        }
        if roles.values().any(|r| r == n) {
            return false;
        }
        next = by_name.get(n).and_then(|b| b.parent.as_deref());
    }
    true
}
/// A following anchor is an ancestor, or - for a bone with no body joint above
/// it at all (root-level props, separate attachment roots) - a body role joint.
pub fn valid_anchor(bones: &[Bone], roles: &BTreeMap<String, String>, anchor: &str, name: &str) -> bool {
    ancestor(bones, anchor, name)
        || (roles.values().any(|r| r == anchor) && !roles.values().any(|r| r == name) && role_orphan(bones, roles, name))
}
pub fn proposals(
    bones: &[Bone],
    weighted: &[String],
    roles: &BTreeMap<String, String>,
    mapping: &BTreeMap<String, String>,
    anchors: &BTreeMap<String, String>,
) -> BTreeMap<String, Proposal> {
    let by_name: BTreeMap<_, _> = bones.iter().map(|b| (b.name.as_str(), b)).collect();
    let Some(head) = roles.get("head").and_then(|n| by_name.get(n.as_str())) else {
        return BTreeMap::new();
    };
    let Some(foot) = roles.get("foot_L").and_then(|n| by_name.get(n.as_str())) else {
        return BTreeMap::new();
    };
    let height = distance(head.position, foot.position);
    let mut output = BTreeMap::new();
    let equivalents = crate::leg_chains::equivalents(bones, roles);
    for name in weighted {
        if roles.values().any(|n| n == name) || equivalents.contains_key(name) {
            continue;
        }
        let Some(bone) = by_name.get(name.as_str()) else {
            continue;
        };
        let mut next = bone.parent.as_deref();
        let mut seen = BTreeSet::from([name.as_str()]);
        let mut chain = vec![name.to_lowercase()];
        let mut has_dynamic = dynamic(bone);
        let mut has_kinematic = kinematic(bone);
        let mut proposed = false;
        while let Some(n) = next {
            if !seen.insert(n) {
                break;
            }
            let Some(parent) = by_name.get(n) else {
                break;
            };
            if let Some(target) = mapping.get(n).filter(|_| !anchors.contains_key(n)) {
                let role = roles
                    .iter()
                    .find(|(_, v)| v.as_str() == n)
                    .map(|(r, _)| r.as_str());
                let secondary = chain.iter().any(|s| {
                    [
                        "hair",
                        "髪",
                        "braid",
                        "skirt",
                        "cloth",
                        "スカート",
                        "裙",
                        "face",
                        "eye",
                        "瞳",
                        "目",
                        "眉",
                        "lip",
                        "mouth",
                        "jaw",
                    ]
                    .iter()
                    .any(|t| s.contains(t))
                });
                let lower = name.to_lowercase();
                let helper = lower.contains("twist")
                    || lower.contains('捩')
                    || lower.contains("_ty_")
                    || lower.contains("_tz_");
                let torso = ["下半身", "上半身"].contains(&name.as_str());
                let near = height.is_finite()
                    && height > 1e-6
                    && distance(bone.position, parent.position) < height * 0.22;
                let skin_near = bone
                    .details
                    .get("skin")
                    .and_then(|v| v.get("centroid"))
                    .and_then(|v| serde_json::from_value::<[f64; 3]>(v.clone()).ok())
                    .is_none_or(|p| distance(p, parent.position) < height * 0.4);
                let limb = role.is_some_and(|r| {
                    ["upperarm_", "forearm_", "hand_", "thigh_", "shin_", "foot_"]
                        .iter()
                        .any(|p| r.starts_with(p))
                });
                let side_agrees = role.is_none_or(|r| {
                    let l =
                        lower.contains("_l_") || lower.ends_with(".l") || lower.starts_with('左');
                    let right =
                        lower.contains("_r_") || lower.ends_with(".r") || lower.starts_with('右');
                    !(l && r.ends_with("_R") || right && r.ends_with("_L"))
                });
                let body_helper = !secondary
                    && side_agrees
                    && near
                    && skin_near
                    && ((helper && limb) || (torso && role == Some("pelvis")));
                // Static facial deformation and verified kinematic attachment
                // roots have no independent motion to preserve. Use the same
                // ancestor transform for every vertex, keeping rest offsets.
                // Dynamic branches, distant skin and wrong-side limbs remain
                // review-only; selected physics branches are preserved separately.
                let named_motion = chain.iter().any(|s| ["hair", "髪", "braid", "skirt", "cloth", "tail", "cape", "kimono", "リボン", "裙"].iter().any(|t| s.contains(t)));
                let face = role == Some("head") && !named_motion && chain.iter().any(|s| facial_name(s));
                let shape_helper = role == Some("chest") && ["breast", "胸"].iter().any(|s| lower.contains(s));
                let toe = role.is_some_and(|r| r.starts_with("foot_"))
                    && (lower.contains("足先ex") || lower.contains("toe") || lower.contains("つま先"));
                let static_follow = !has_dynamic && near && skin_near && side_agrees
                    && (face || shape_helper || toe || has_kinematic);
                let automatic = (body_helper && !has_dynamic) || static_follow;
                let group = if static_follow && face {
                    format!("Static face → {target}")
                } else if automatic {
                    "Body helpers".to_string()
                } else {
                    let category = if chain
                        .iter()
                        .any(|s| ["hair", "髪", "braid"].iter().any(|t| s.contains(t)))
                    {
                        "Hair"
                    } else if chain.iter().any(|s| {
                        ["skirt", "cloth", "スカート", "裙"]
                            .iter()
                            .any(|t| s.contains(t))
                    }) {
                        "Clothing"
                    } else if chain.iter().any(|s| {
                        facial_name(s)
                    }) {
                        "Face"
                    } else {
                        "Secondary bones"
                    };
                    format!("{category} → {target}")
                };
                output.insert(name.clone(), Proposal {
                    anchor: n.into(), target: target.clone(), group, automatic,
                    reason: format!("Follows {n} → {target}, retaining its offset during rest alignment. {}Independent source motion is not transferred.",
                        if static_follow { "Static rest-shape attachment; ancestry, skin extent and binding checks agree. Review the loss of independent facial or corrective animation. " }
                        else if body_helper { "Recognized body helper; ancestry, side and distance checks agree. " } else { "Review this simplification. " }),
                });
                proposed = true;
                break;
            }
            chain.push(n.to_lowercase());
            has_dynamic |= dynamic(parent);
            has_kinematic |= kinematic(parent);
            next = parent.parent.as_deref();
        }
        if proposed || !role_orphan(bones, roles, name) {
            continue;
        }
        // No body joint above this bone (for example a prop or accessory bone
        // parented to the armature root). Attach it to the nearest mapped body
        // joint, measured from its skin centre, keeping its rest offset.
        let centre = bone
            .details
            .get("skin")
            .and_then(|v| v.get("centroid"))
            .and_then(|v| serde_json::from_value::<[f64; 3]>(v.clone()).ok())
            .unwrap_or(bone.position);
        let nearest = roles
            .iter()
            .filter_map(|(role, joint)| {
                let target = mapping.get(joint)?;
                let position = by_name.get(joint.as_str())?.position;
                Some((distance(centre, position), role, joint, target))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((gap, role, joint, target)) = nearest {
            let close = height.is_finite() && height > 1e-6 && gap < height * 0.35;
            output.insert(name.clone(), Proposal {
                anchor: joint.clone(),
                target: target.clone(),
                group: format!("Unattached bones → {target}"),
                automatic: close && !has_dynamic,
                reason: format!(
                    "No body joint is above this bone. It follows the nearest body joint ({role}: {joint} → {target}) with its rest offset kept. Independent source motion is not transferred."
                ),
            });
        }
    }
    output
}
fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.iter()
        .zip(b)
        .map(|(a, b)| (a - b).powi(2))
        .sum::<f64>()
        .sqrt()
}

pub fn errors(source: &[Bone], options: &PreparationOptions) -> Vec<String> {
    let anchors = map(options, "mapping_anchors");
    if options
        .values
        .get("mapping_anchors")
        .is_some_and(|v| serde_json::from_value::<BTreeMap<String, String>>(v.clone()).is_err())
    {
        return vec!["Invalid body-following mapping data.".into()];
    }
    let mapping = map(options, "mapping");
    let roles = map(options, "source_roles");
    let mut errors = vec![];
    for (name, anchor) in &anchors {
        if name == anchor
            || roles.values().any(|n| n == name)
            || anchors.contains_key(anchor)
            || !source.iter().any(|b| b.name == *name)
            || !source.iter().any(|b| b.name == *anchor)
            || !valid_anchor(source, &roles, anchor, name)
            || mapping.get(anchor).is_none_or(String::is_empty)
            || mapping.get(name) != mapping.get(anchor)
        {
            errors.push(format!("Body-following mapping for {name} is stale or invalid. Run Auto-map bones again or choose a direct target."));
        }
    }
    if !anchors.is_empty()
        && options
            .values
            .get("follow_motion_reviewed")
            .and_then(|v| v.as_bool())
            != Some(true)
    {
        errors.push(format!("Review the {} body-following mappings: their original independent twist, corrective, face or physics motion will not be reproduced.", anchors.len()));
    }
    errors
}
