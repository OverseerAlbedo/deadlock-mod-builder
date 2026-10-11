//! Suggestions are editable proposals. The existing native review/conversion
//! gates remain authoritative; unknown secondary bones are never collapsed.
use crate::{
    Result,
    algorithms::{self, Bone, Identification, Suggestions},
    invalid,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const POLICY: &str = "humanoid-mapping-assistance-v6";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MappingEntry {
    pub source: String,
    pub target: Option<String>,
    pub group: String,
    pub label: String,
    pub reason: String,
    pub automatic: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub follow: Option<crate::following::Proposal>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Assistance {
    pub policy: String,
    pub entries: Vec<MappingEntry>,
    pub labels: BTreeMap<String, String>,
    pub warnings: Vec<String>,
    #[serde(default)]
    pub anchors: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub arm_deformation: BTreeMap<String, crate::arm_helpers::Match>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
}

fn compact(name: &str) -> String {
    name.rsplit(':')
        .next()
        .unwrap_or(name)
        .chars()
        .filter_map(|c| {
            if ('０'..='９').contains(&c) {
                char::from_u32(c as u32 - '０' as u32 + '0' as u32)
            } else if c.is_alphanumeric() {
                Some(c.to_ascii_lowercase())
            } else {
                None
            }
        })
        .collect()
}
fn is_ancestor(bones: &[Bone], parent: &str, child: &str) -> bool {
    let names: BTreeMap<_, _> = bones.iter().map(|b| (b.name.as_str(), b)).collect();
    let mut next = Some(child);
    let mut seen = BTreeSet::new();
    while let Some(n) = next {
        if !seen.insert(n) {
            return false;
        }
        if n == parent {
            return true;
        }
        next = names.get(n).and_then(|b| b.parent.as_deref());
    }
    false
}
fn unique(bones: &[Bone], names: &[String]) -> Option<String> {
    let choices: Vec<_> = bones
        .iter()
        .filter(|b| {
            b.details.get("deform").and_then(|v| v.as_bool()) != Some(false)
                && names.iter().any(|n| compact(&b.name) == compact(n))
        })
        .collect();
    (choices.len() == 1).then(|| choices[0].name.clone())
}
fn side_names(base: &str, side: &str) -> Vec<String> {
    vec![
        format!("{base}.{side}"),
        format!("{}{base}", if side == "L" { "左" } else { "右" }),
    ]
}

/// Full-name aliases only: helpers, IK targets and twist bones cannot match a
/// primary limb by a substring. Weighted D chains win over MMD IK-driven chains.
pub fn roles(
    bones: &[Bone],
    weighted: &[String],
    overrides: &BTreeMap<String, String>,
) -> Result<Identification> {
    let mut found = algorithms::identify(bones, &BTreeMap::new())?.roles;
    // Blender's rest-rig recovery supplies recomputed anatomical hints. They
    // fill missing roles only; duplicate hints, ordinary detections and explicit
    // user choices are never silently overridden. identify below still checks
    // the full hierarchy, spacing, sides and upright geometry.
    for role in algorithms::ROLES {
        if found.contains_key(role) { continue; }
        let candidates: Vec<_> = bones.iter().filter(|b|
            b.details.get("humanoid_role_hint").and_then(|v| v.as_str()) == Some(role)
                && b.details.get("deform").and_then(|v| v.as_bool()) != Some(false)
        ).collect();
        if candidates.len() == 1 { found.insert(role.into(), candidates[0].name.clone()); }
    }
    for (role, alternatives) in [
        ("pelvis", vec!["腰"]),
        ("spine", vec!["上半身1", "上半身"]),
        ("chest", vec!["上半身2"]),
        ("neck", vec!["首", "neck_0"]),
        ("head", vec!["頭"]),
    ] {
        if !found.contains_key(role) {
            for name in alternatives {
                if let Some(name) = unique(bones, &[name.into()]) {
                    found.insert(role.into(), name);
                    break;
                }
            }
        }
    }
    for side in ["L", "R"] {
        for (role, name) in [
            ("upperarm", "腕"),
            ("forearm", "ひじ"),
            ("hand", "手首"),
            ("thigh", "足"),
            ("shin", "ひざ"),
            ("foot", "足首"),
        ] {
            let role = format!("{role}_{side}");
            if found.contains_key(&role) {
                continue;
            }
            let ordinary = unique(bones, &side_names(name, side));
            let deform = unique(bones, &side_names(&format!("{name}D"), side));
            let choice = if ["足", "ひざ", "足首"].contains(&name) {
                match (&ordinary, &deform) {
                    (_, Some(d)) if weighted.contains(d) => deform,
                    (Some(o), _) if weighted.contains(o) => ordinary,
                    (None, _) => deform,
                    (_, None) => ordinary,
                    _ => None, // two unweighted chains: no evidence which is the skin rig
                }
            } else {
                ordinary
            };
            if let Some(name) = choice {
                found.insert(role, name);
            }
        }
    }
    // Valve/Mixamo rigs can contain several named chest/spine candidates.
    // Use the highest member of one actual ancestor chain below the neck.
    if !found.contains_key("chest") {
        if let (Some(neck), Some(spine)) = (found.get("neck"), found.get("spine")) {
            let candidates: Vec<_> = bones
                .iter()
                .filter(|b| {
                    (algorithms::name_role(&b.name) == Some("chest")
                        || compact(&b.name) == "上半身2")
                        && b.details.get("deform").and_then(|v| v.as_bool()) != Some(false)
                        && is_ancestor(bones, &b.name, neck)
                        && is_ancestor(bones, spine, &b.name)
                })
                .collect();
            if let Some(last) = candidates.iter().find(|b| {
                candidates
                    .iter()
                    .all(|a| is_ancestor(bones, &a.name, &b.name))
            }) {
                found.insert("chest".into(), last.name.clone());
            }
        }
    }
    crate::leg_chains::detect(bones, weighted, &mut found);
    for (role, name) in overrides {
        if !name.is_empty() {
            found.insert(role.clone(), name.clone());
        }
    }
    let named = algorithms::identify(bones, &found)?;
    if named.errors.is_empty() {
        return Ok(named);
    }
    // Names were missing, ambiguous or contradictory. Recognize the body from
    // the skin skeleton's structure instead; explicit user choices still win
    // and the ordinary identify checks decide whether the result is usable.
    if let Some(detected) = crate::structure::detect(bones, weighted) {
        let mut merged = detected.roles.clone();
        for (role, name) in overrides {
            if !name.is_empty() {
                merged.insert(role.clone(), name.clone());
            }
        }
        let structural = algorithms::identify(bones, &merged)?;
        if structural.errors.is_empty() {
            return Ok(algorithms::Identification {
                evidence: format!(
                    "Skeleton structure (hierarchy, floor contact, arm span and limb proportions); sides from {}. {}Human confirmation required.",
                    detected.sides_from,
                    detected.notes.iter().map(|n| format!("{n} ")).collect::<String>()
                ),
                ..structural
            });
        }
    }
    Ok(named)
}

fn primary_ancestor<'a>(
    bones: &[Bone],
    roles: &'a BTreeMap<String, String>,
    name: &str,
) -> Option<&'a str> {
    let names: BTreeMap<_, _> = bones.iter().map(|b| (b.name.as_str(), b)).collect();
    let mut next = Some(name);
    let mut seen = BTreeSet::new();
    while let Some(name) = next {
        if !seen.insert(name) {
            return None;
        }
        if let Some((role, _)) = roles.iter().find(|(_, value)| value.as_str() == name) {
            return Some(role);
        }
        next = names.get(name).and_then(|b| b.parent.as_deref());
    }
    None
}
fn finger(name: &str) -> Option<(&'static str, char, u32)> {
    // Autodesk's Biped template: Finger0/01/02 = thumb, Finger1/11/12 =
    // index, through Finger4/41/42 = pinky. Full-name matching is essential:
    // generic numbered helpers are not fingers. Equal counts and ancestry
    // are still required below before any mapping is accepted.
    static BIPED: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let pattern=BIPED.get_or_init(||regex::Regex::new(r"(?i)^(?:valvebiped[_. -]*)?bip\d+[_. -]*([lr])[_. -]*finger([0-4])([12]?)$").unwrap());
    if let Some(c)=pattern.captures(name.rsplit(':').next().unwrap_or(name)) {
        let kind=["thumb","index","middle","ring","pinky"][c[2].parse::<usize>().ok()?];
        return Some((kind,c[1].chars().next()?.to_ascii_uppercase(),c[3].parse().unwrap_or(0)));
    }
    static MIXAMO: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let pattern=MIXAMO.get_or_init(||regex::Regex::new(r"(?i)^(?:mixamorig[_. -]*)?(left|right)hand(thumb|index|middle|ring|pinky)([1-4])$").unwrap());
    if let Some(c)=pattern.captures(name.rsplit(':').next().unwrap_or(name)) {
        let kind=match c[2].to_ascii_lowercase().as_str(){"thumb"=>"thumb","index"=>"index","middle"=>"middle","ring"=>"ring",_=>"pinky"};
        return Some((kind,if c[1].eq_ignore_ascii_case("left"){'L'}else{'R'},c[3].parse().ok()?));
    }
    let name = compact(name);
    let (side, base) = if let Some(v) = name.strip_prefix('左') {
        ('L', v)
    } else if let Some(v) = name.strip_prefix('右') {
        ('R', v)
    } else if let Some(v) = name.strip_suffix('l') {
        ('L', v)
    } else if let Some(v) = name.strip_suffix('r') {
        ('R', v)
    } else {
        return None;
    };
    for (kind, aliases) in [
        ("thumb", &["親指", "fingerthumb", "thumb"][..]),
        ("index", &["人指", "人差指", "fingerindex", "index"]),
        ("middle", &["中指", "fingermiddle", "middle"]),
        ("ring", &["薬指", "fingerring", "ring"]),
        ("pinky", &["小指", "fingerpinky", "pinky", "little"]),
    ] {
        for prefix in aliases {
            if let Some(n) = base
                .strip_prefix(prefix)
                .and_then(|v| v.parse::<u32>().ok())
            {
                return Some((kind, side, n));
            }
        }
    }
    None
}
fn group(name: &str, source: &[Bone], roles: &BTreeMap<String, String>) -> String {
    if let Some((_, side, _)) = finger(name) {
        return format!("{} hand", if side == 'L' { "Left" } else { "Right" });
    }
    let n = name.to_lowercase();
    if n.contains("捩") || n.contains("twist") || n.contains("_ty_") || n.contains("_tz_") {
        return "Extra deformation bones".into();
    }
    if ["hair", "髪", "braid"].iter().any(|s| n.contains(s)) {
        return "Hair".into();
    }
    if ["skirt", "cloth", "kimono", "スカート", "裙"]
        .iter()
        .any(|s| n.contains(s))
    {
        return "Clothing".into();
    }
    if crate::following::facial_name(name) {
        return "Face".into();
    }
    if roles.values().any(|s| s == name)
        || ["下半身", "上半身", "上半身1", "上半身2"].contains(&name)
    {
        return "Body".into();
    }
    match primary_ancestor(source, roles, name) {
        Some("hand_L") => "Left hand / extras",
        Some("hand_R") => "Right hand / extras",
        Some("head") => "Head / secondary bones",
        Some("neck") => "Neck / secondary bones",
        Some("pelvis") => "Pelvis / secondary bones",
        Some("spine") => "Spine / secondary bones",
        Some("chest") => "Chest / secondary bones",
        Some("upperarm_L") => "Left upper arm / secondary bones",
        Some("upperarm_R") => "Right upper arm / secondary bones",
        Some("forearm_L") => "Left forearm / secondary bones",
        Some("forearm_R") => "Right forearm / secondary bones",
        Some("thigh_L") => "Left thigh / secondary bones",
        Some("thigh_R") => "Right thigh / secondary bones",
        Some("shin_L") => "Left shin / secondary bones",
        Some("shin_R") => "Right shin / secondary bones",
        Some("foot_L") => "Left foot / secondary bones",
        Some("foot_R") => "Right foot / secondary bones",
        _ => "Other / secondary bones",
    }
    .into()
}

pub fn suggest(
    source: &[Bone],
    weighted: &[String],
    target: &[Bone],
    options: &deadlock_project::PreparationOptions,
) -> Result<Suggestions> {
    let map = |key: &str| -> Result<BTreeMap<String, String>> {
        options
            .values
            .get(key)
            .map(|v| {
                serde_json::from_value(v.clone()).map_err(|_| invalid("Invalid mapping choices."))
            })
            .transpose()
            .map(Option::unwrap_or_default)
    };
    let sr = if source.is_empty() {
        None
    } else {
        Some(roles(source, weighted, &map("source_roles")?)?)
    };
    let tr = roles(target, &[], &map("target_roles")?)?;
    let mut mapping = map("mapping")?;
    let targets: BTreeSet<_> = target.iter().map(|b| b.name.as_str()).collect();
    mapping.retain(|n, t| source.iter().any(|b| b.name == *n) && targets.contains(t.as_str()));
    let mut assistance = Assistance {
        policy: POLICY.into(),
        ..Default::default()
    };
    let Some(sr) = sr else {
        return Ok(Suggestions {
            source: None,
            target: tr,
            mapping,
            assistance: Some(assistance),
        });
    };
    assistance
        .warnings
        .extend(sr.errors.iter().map(|e| format!("Source: {e}")));
    assistance
        .warnings
        .extend(tr.errors.iter().map(|e| format!("Target: {e}")));
    let mut automatic = BTreeMap::<String, String>::new();
    for (role, name) in &sr.roles {
        assistance
            .labels
            .insert(name.clone(), role.replace('_', " "));
        if let Some(dest) = tr.roles.get(role) {
            mapping.insert(name.clone(), dest.clone()); // keep roles and skin mapping consistent, even for unweighted controls
            automatic.insert(name.clone(), "Corresponding humanoid role".into());
        }
    }
    // Identical names are accepted only within corresponding anatomical branches.
    for bone in source {
        if !mapping.contains_key(&bone.name) && targets.contains(bone.name.as_str()) {
            let a = primary_ancestor(source, &sr.roles, &bone.name);
            if a.is_some() && a == primary_ancestor(target, &tr.roles, &bone.name) {
                mapping.insert(bone.name.clone(), bone.name.clone());
                automatic.insert(
                    bone.name.clone(),
                    "Same name and corresponding anatomical branch".into(),
                );
            }
        }
    }
    // Match finger chains only when both rigs have the same number of joints,
    // ordered digits form an ancestor chain, and each chain is below its hand.
    for side in ['L', 'R'] {
        for kind in ["thumb", "index", "middle", "ring", "pinky"] {
            let collect = |bones: &[Bone]| {
                let mut v: Vec<_> = bones
                    .iter()
                    .filter_map(|b| {
                        finger(&b.name)
                            .filter(|(k, s, _)| *k == kind && *s == side)
                            .map(|(_, _, i)| (i, b.name.clone()))
                    })
                    .collect();
                v.sort();
                v
            };
            let a = collect(source);
            let b = collect(target);
            let valid = |bones: &[Bone], list: &[(u32, String)], hand: Option<&String>| {
                hand.is_some_and(|h| list.iter().all(|(_, n)| is_ancestor(bones, h, n)))
                    && list
                        .windows(2)
                        .all(|w| w[0].0 != w[1].0 && is_ancestor(bones, &w[0].1, &w[1].1))
            };
            if !(2..=4).contains(&a.len())
                || a.len() != b.len()
                || !valid(source, &a, sr.roles.get(&format!("hand_{side}")))
                || !valid(target, &b, tr.roles.get(&format!("hand_{side}")))
            {
                continue;
            }
            for ((_, n), (_, t)) in a.iter().zip(&b) {
                assistance.labels.insert(
                    n.clone(),
                    format!("{} {kind}", if side == 'L' { "Left" } else { "Right" }),
                );
                if !mapping.contains_key(n) {
                    mapping.insert(n.clone(), t.clone());
                    automatic.insert(
                        n.clone(),
                        "Matching finger chain, joint order and hand ancestry".into(),
                    );
                }
            }
        }
    }
    // Both chains can carry real skin weights. Map proven parallel joints to
    // the same limb, including stale ancestor-follow choices from old projects.
    let equivalents = crate::leg_chains::equivalents(source, &sr.roles);
    for (name, primary) in &equivalents {
        if let Some(dest) = mapping.get(primary).cloned() {
            mapping.insert(name.clone(), dest);
            automatic.insert(name.clone(), "Equivalent leg joint: complete chain, side, ancestry and rest positions agree".into());
        }
    }
    let mut anchors = map("mapping_anchors")?;
    if sr.errors.is_empty() && tr.errors.is_empty() {
        for (name, matched) in crate::arm_helpers::detect(source,target,&sr.roles,&tr.roles) {
            if !mapping.contains_key(&name) || anchors.contains_key(&name) || mapping.get(&name)==Some(&matched.target) {
                mapping.insert(name.clone(),matched.target.clone());
                anchors.remove(&name);
                automatic.insert(name.clone(),format!("Existing hero {} joint; limb ancestry and relative position agree. Rest geometry keeps its anatomical alignment.",matched.kind));
                assistance.arm_deformation.insert(name,matched);
            }
        }
    }
    anchors.retain(|name, anchor| {
        !equivalents.contains_key(name) && mapping.contains_key(name)
            && source.iter().any(|b| b.name == *anchor)
            && crate::following::valid_anchor(source, &sr.roles, anchor, name)
            && !sr.roles.values().any(|n| n == name)
    });
    // Never let invalid reviewed roles authorize secondary-bone adaptation.
    let follows = if sr.errors.is_empty() && tr.errors.is_empty() {
        crate::following::proposals(source, weighted, &sr.roles, &mapping, &anchors)
    } else {
        BTreeMap::new()
    };
    // Fresh inspections ask for a complete mapping: every remaining weighted
    // bone follows its proposed body joint with its rest offset kept. The
    // review still lists these and the user can change any of them.
    let complete = options.values.get("complete_mapping").and_then(|v| v.as_bool()) == Some(true);
    for (name, proposal) in &follows {
        if ((proposal.automatic || complete) && !mapping.contains_key(name)) || anchors.contains_key(name) {
            mapping.insert(name.clone(), proposal.target.clone());
            anchors.insert(name.clone(), proposal.anchor.clone());
            automatic.insert(name.clone(), proposal.reason.clone());
        }
    }
    anchors.retain(|n, a| mapping.get(n) == mapping.get(a));
    assistance.anchors = anchors;
    for name in weighted {
        let g = group(name, source, &sr.roles);
        let (target, reason, auto) = if let Some(t) = mapping.get(name) {
            (
                Some(t.clone()),
                automatic
                    .get(name)
                    .cloned()
                    .unwrap_or_else(|| "Existing project choice; retained for review".into()),
                true,
            )
        } else if name == "下半身"
            && sr
                .roles
                .get("pelvis")
                .is_some_and(|p| is_ancestor(source, p, name))
        {
            (
                tr.roles.get("pelvis").cloned(),
                "Lower-body joint below the pelvis control; review the proposed bind position"
                    .into(),
                false,
            )
        } else if name.starts_with("上半身") {
            (
                tr.roles.get("spine").cloned(),
                "Additional torso joint; merging it changes its bind pivot and needs review".into(),
                false,
            )
        } else {
            (None, "No proven equivalent. Choose a target explicitly; extra motion is not automatically transferred.".into(), false)
        };
        assistance.entries.push(MappingEntry {
            source: name.clone(),
            target,
            group: g,
            label: assistance
                .labels
                .get(name)
                .cloned()
                .unwrap_or_else(|| name.clone()),
            reason,
            automatic: auto,
            follow: follows.get(name).cloned(),
        });
    }
    Ok(Suggestions {
        source: Some(sr),
        target: tr,
        mapping,
        assistance: Some(assistance),
    })
}
