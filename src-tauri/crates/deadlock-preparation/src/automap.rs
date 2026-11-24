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
    let mut notes = vec![];
    // Body names behind a prefix every body bone shares. Only when the plain
    // names prove almost nothing, and only to fill roles still missing.
    if found.len() < 6 {
        if let Some((prefix, named)) = algorithms::prefixed_roles(bones, weighted) {
            let mut added = 0;
            for (role, name) in named {
                if !found.contains_key(&role) && !found.values().any(|n| *n == name) {
                    found.insert(role, name);
                    added += 1;
                }
            }
            if added > 0 {
                let shown = bones
                    .iter()
                    .map(|b| b.name.rsplit(':').next().unwrap_or(&b.name))
                    .find(|n| n.to_lowercase().starts_with(&prefix))
                    .and_then(|n| n.get(..prefix.len()))
                    .unwrap_or(&prefix)
                    .to_string();
                notes.push(format!("Body bone names were read after the prefix \"{shown}\" that the rig's bones share."));
            }
        }
    }
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
    numbered_spine(bones, &mut found);
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
    fill_from_structure(bones, weighted, &mut found, &mut notes);
    crate::leg_chains::detect(bones, weighted, &mut found);
    for (role, name) in overrides {
        if !name.is_empty() {
            found.insert(role.clone(), name.clone());
        }
    }
    let named = algorithms::identify(bones, &found)?;
    if named.errors.is_empty() {
        return Ok(refined(bones, weighted, named, overrides, notes));
    }
    // Names were missing, ambiguous or contradictory. Recognize the body from
    // the skin skeleton's structure instead; explicit user choices still win
    // and the ordinary identify checks decide whether the result is usable.
    // Roles the names did prove guide the structure's choice of limb joints.
    let hints: BTreeMap<String, String> = found.iter().filter(|(_, n)| !n.is_empty()).map(|(r, n)| (r.clone(), n.clone())).collect();
    if let Some(detected) = crate::structure::detect_with_hints(bones, weighted, &hints) {
        let mut merged = detected.roles.clone();
        for (role, name) in overrides {
            if !name.is_empty() {
                merged.insert(role.clone(), name.clone());
            }
        }
        let structural = algorithms::identify(bones, &merged)?;
        if structural.errors.is_empty() {
            let mut notes = notes;
            notes.extend(detected.notes.iter().cloned());
            let found = algorithms::Identification {
                evidence: format!(
                    "Skeleton structure (hierarchy, floor contact, arm span and limb proportions); sides from {}. Human confirmation required.",
                    detected.sides_from,
                ),
                ..structural
            };
            return Ok(refined(bones, weighted, found, overrides, notes));
        }
    }
    Ok(named)
}

fn skin_weight(bone: &Bone) -> f64 {
    let skin = bone.details.get("skin");
    skin.and_then(|s| s.get("weight_sum")).and_then(|v| v.as_f64())
        .or_else(|| skin.and_then(|s| s.get("vertices")).and_then(|v| v.as_f64()))
        .unwrap_or(0.)
}
fn bone<'a>(bones: &'a [Bone], name: &str) -> Option<&'a Bone> {
    bones.iter().find(|b| b.name == name)
}
fn deforming(b: &Bone) -> bool {
    b.details.get("deform").and_then(|v| v.as_bool()) != Some(false)
}
/// Ancestors of `name`, starting with its parent.
fn ancestors(bones: &[Bone], name: &str) -> Vec<String> {
    let mut out = vec![];
    let mut next = bone(bones, name).and_then(|b| b.parent.clone());
    while let Some(n) = next {
        if out.contains(&n) {
            break;
        }
        next = bone(bones, &n).and_then(|b| b.parent.clone());
        out.push(n);
    }
    out
}

/// Numbered spine bones (`Spine`, `Spine1`, `Spine2` ...): the aliases read
/// `Spine` and `Spine1` both as the spine (and `Spine2`, `Spine3` both as the
/// chest), so the name pass drops the role. When the pelvis is known and the
/// spine or the chest is missing, the deforming bones below the pelvis named
/// as spine or chest that lie on one ancestor chain (2 to 8 of them) map
/// lowest to highest: the lowest is the spine, the highest the chest, each
/// only when that role is missing and never both to one bone. The same rule
/// as the Blender rest-rig recovery (`source_compatibility.py plan`), which
/// gives these roles as hints when it recovers a rig. Fills missing roles
/// only; identify still checks the whole body.
fn numbered_spine(bones: &[Bone], found: &mut BTreeMap<String, String>) {
    let Some(pelvis) = found.get("pelvis").cloned() else { return };
    if found.contains_key("spine") && found.contains_key("chest") {
        return;
    }
    let candidates: Vec<&Bone> = bones
        .iter()
        .filter(|b| {
            deforming(b)
                && b.position.iter().all(|v| v.is_finite())
                && matches!(algorithms::name_role(&b.name), Some("spine" | "chest"))
                && is_ancestor(bones, &pelvis, &b.name)
        })
        .collect();
    if !(2..=8).contains(&candidates.len())
        || !candidates.iter().all(|a| candidates.iter().all(|b| is_ancestor(bones, &a.name, &b.name) || is_ancestor(bones, &b.name, &a.name)))
    {
        return;
    }
    let lowest = candidates.iter().find(|b| candidates.iter().all(|c| is_ancestor(bones, &b.name, &c.name)));
    let highest = candidates.iter().find(|b| candidates.iter().all(|c| is_ancestor(bones, &c.name, &b.name)));
    let (Some(lowest), Some(highest)) = (lowest, highest) else { return };
    let spine = found.get("spine").cloned().unwrap_or_else(|| lowest.name.clone());
    let chest = found.get("chest").cloned().unwrap_or_else(|| highest.name.clone());
    if spine == chest || found.iter().any(|(role, name)| role != "spine" && role != "chest" && (*name == spine || *name == chest)) {
        return;
    }
    found.insert("spine".into(), spine);
    found.insert("chest".into(), chest);
}

/// Roles the names left out but the named neighbours prove: the pelvis is the
/// joint that carries both named thighs and the named spine (rigs whose root
/// carries the hips' skin), and a limb's middle joint is the only joint
/// between its named upper and end joints (a misspelled shin or forearm).
/// Fills missing roles only; identify still checks the whole body.
fn fill_from_structure(bones: &[Bone], weighted: &[String], found: &mut BTreeMap<String, String>, notes: &mut Vec<String>) {
    if !found.contains_key("pelvis") {
        if let (Some(l), Some(r), Some(s)) = (found.get("thigh_L"), found.get("thigh_R"), found.get("spine")) {
            let (al, ar, asp) = (ancestors(bones, l), ancestors(bones, r), ancestors(bones, s));
            if let Some(common) = al.iter().find(|a| ar.contains(a) && asp.contains(a)).cloned() {
                if weighted.contains(&common) && bone(bones, &common).is_some_and(deforming) && !found.values().any(|n| *n == common) {
                    notes.push(format!("The pelvis is {common}: no bone is named as hips, and it carries both thighs and the spine."));
                    found.insert("pelvis".into(), common);
                }
            }
        }
    }
    for side in ["L", "R"] {
        for [top, middle, end] in [["thigh", "shin", "foot"], ["upperarm", "forearm", "hand"]] {
            let role = format!("{middle}_{side}");
            if found.contains_key(&role) {
                continue;
            }
            let (Some(t), Some(e)) = (found.get(&format!("{top}_{side}")), found.get(&format!("{end}_{side}"))) else { continue };
            let up = ancestors(bones, e);
            let Some(at) = up.iter().position(|n| n == t) else { continue };
            if at != 1 {
                continue;
            }
            let joint = up[0].clone();
            if bone(bones, &joint).is_some_and(deforming) && !found.values().any(|n| *n == joint) {
                notes.push(format!("{} is {joint}: it is the only joint between the named {top} and {end}.", role.replace('_', " ")));
                found.insert(role, joint);
            }
        }
    }
}

/// Skin-based corrections of a valid guess. Each is kept only when identify
/// still accepts the whole body; roles the user chose are never changed.
fn refined(
    bones: &[Bone],
    weighted: &[String],
    found: Identification,
    overrides: &BTreeMap<String, String>,
    mut notes: Vec<String>,
) -> Identification {
    let chosen = |role: &str| overrides.get(role).is_some_and(|n| !n.is_empty());
    let mut roles = found.roles.clone();
    let mut also = BTreeMap::new();
    let mut extra = vec![];
    // The head is the joint that carries the face. A joint at or above the
    // neck that carries at least HEAD_SKIN of the head's skin weight holds the
    // face (the guessed head then carries little more than a hair cap): it
    // becomes the head, and the guessed head follows the hero's head too.
    // Ordinary rigs measured 0.00-0.18 for neck / head; the rig this rule was
    // made for 1.15, and 0.35 of the head with its children.
    if !chosen("head") && !chosen("neck") {
        if let (Some(neck), Some(head), Some(chest)) = (roles.get("neck").cloned(), roles.get("head").cloned(), roles.get("chest").cloned()) {
            let head_skin = bone(bones, &head).map(skin_weight).unwrap_or(0.);
            let up = ancestors(bones, &head);
            if let Some(at) = up.iter().position(|n| *n == neck) {
                // Everything the head carries with its children (hair, ears,
                // and on rigs with facial bones the jaw, eyes and face).
                let head_tree: f64 = bones
                    .iter()
                    .filter(|b| b.name == head || ancestors(bones, &b.name).contains(&head))
                    .map(skin_weight)
                    .sum();
                if head_skin > 0. {
                    // From the neck upwards, the lowest joint carrying the face.
                    let face = up[..=at].iter().rev().find(|n| {
                        weighted.contains(n)
                            && bone(bones, n).is_some_and(|b| {
                                skin_weight(b) >= HEAD_SKIN * head_skin && skin_weight(b) >= HEAD_TREE_SKIN * head_tree
                            })
                    }).cloned();
                    if let Some(face) = face {
                        let new_neck = if face == neck {
                            ancestors(bones, &neck).first().cloned().filter(|p| {
                                *p != chest && ancestors(bones, p).contains(&chest) && !roles.values().any(|n| n == p)
                            })
                        } else {
                            Some(neck.clone())
                        };
                        if let Some(new_neck) = new_neck {
                            extra.push(format!(
                                "The head is {face}: it carries about as much skin as {head} above it or more, which is where the face is. {head} follows the hero's head as well."
                            ));
                            roles.insert("neck".into(), new_neck);
                            roles.insert("head".into(), face);
                            also.insert(head, "head".to_string());
                        }
                    }
                }
            }
        }
    }
    let accepted = !extra.is_empty() && algorithms::identify(bones, &roles).is_ok_and(|r| r.errors.is_empty());
    let mut result = if accepted {
        notes.extend(extra);
        Identification { roles, also, ..found }
    } else {
        found
    };
    if !notes.is_empty() {
        result.evidence = format!("{} {}", result.evidence, notes.join(" "));
    }
    result.notes = notes;
    result
}
const HEAD_SKIN: f64 = 0.8;
/// The face-carrying joint also holds at least this share of everything the
/// guessed head and its children carry, so a light head bone whose face sits
/// on facial bones below it (jaw, eyes) never hands the head to the neck.
const HEAD_TREE_SKIN: f64 = 0.25;

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
    // Joints the guess found to carry the rest of a role's body part (the hair
    // cap above the face-carrying head joint) follow the same hero bone, unless
    // the project already chose something for them.
    for (name, role) in &sr.also {
        if let Some(dest) = tr.roles.get(role) {
            if !mapping.contains_key(name) {
                mapping.insert(name.clone(), dest.clone());
                automatic.insert(name.clone(), format!("Carries the rest of the {role} above the joint chosen as {role}; follows the same hero bone"));
            }
        }
    }
    assistance.warnings.extend(sr.notes.iter().map(|n| format!("Body guess: {n}")));
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
