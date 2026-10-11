//! The same conservative name/hierarchy/landmark review as the Blender bridge.
//! Recognition never certifies artistic fit, compiler support, or animation quality.
use crate::{Result, invalid};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
pub const ROLES: [&str; 17] = [
    "pelvis",
    "spine",
    "chest",
    "neck",
    "head",
    "upperarm_L",
    "forearm_L",
    "hand_L",
    "thigh_L",
    "shin_L",
    "foot_L",
    "upperarm_R",
    "forearm_R",
    "hand_R",
    "thigh_R",
    "shin_R",
    "foot_R",
];
const CHAINS: [&[&str]; 5] = [
    &["pelvis", "spine", "chest", "neck", "head"],
    &["upperarm_L", "forearm_L", "hand_L"],
    &["thigh_L", "shin_L", "foot_L"],
    &["upperarm_R", "forearm_R", "hand_R"],
    &["thigh_R", "shin_R", "foot_R"],
];
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Bone {
    pub name: String,
    pub parent: Option<String>,
    pub position: [f64; 3],
    #[serde(flatten)]
    pub details: BTreeMap<String, serde_json::Value>,
}
impl From<&deadlock_skeleton::transforms::PreparedBone> for Bone {
    fn from(b: &deadlock_skeleton::transforms::PreparedBone) -> Self {
        Self {
            name: b.name.clone(),
            parent: b.parent.clone(),
            position: b.position,
            details: BTreeMap::new(),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Identification {
    pub roles: BTreeMap<String, String>,
    pub missing: Vec<String>,
    pub errors: Vec<String>,
    pub status: String,
    pub evidence: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Suggestions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<Identification>,
    pub target: Identification,
    pub mapping: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assistance: Option<crate::automap::Assistance>,
}
fn aliases(base: &str) -> &'static [&'static str] {
    match base {
        "pelvis" => &["hips", "pelvis", "hip"],
        "spine" => &["spine", "spine1", "spine01", "abdomen", "abdomenlower", "waist"],
        "chest" => &[
            "chest",
            "upperchest",
            "spine2",
            "spine3",
            "spine02",
            "spine03",
            "chestupper",
        ],
        "neck" => &["neck", "neck1", "neck01", "necklower"],
        "head" => &["head"],
        "upperarm" => &["upperarm", "armupper", "arm", "shldrbend"],
        "forearm" => &["forearm", "lowerarm", "armlower", "forearmbend"],
        "hand" => &["hand", "wrist"],
        "thigh" => &["thigh", "upleg", "upperleg", "legupper", "hip", "thighbend"],
        "shin" => &["calf", "leg", "lowerleg", "leglower", "knee", "shin"],
        "foot" => &["foot", "ankle"],
        _ => &[],
    }
}
pub fn name_role(name: &str) -> Option<&'static str> {
    let name = name.rsplit(':').next().unwrap().to_lowercase();
    static PREFIX: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    static VALVE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    static DEFORM: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let name = VALVE.get_or_init(||regex::Regex::new(r"^valvebiped[_. -]*").unwrap()).replace(&name, "");
    let name = DEFORM.get_or_init(||regex::Regex::new(r"^def[_. -]+").unwrap()).replace(&name, "");
    let vrm = if let Some(n) = name.strip_prefix("j_bip_c_") { n.to_string() }
        else if let Some(n) = name.strip_prefix("j_bip_l_") { format!("l_{n}") }
        else if let Some(n) = name.strip_prefix("j_bip_r_") { format!("r_{n}") }
        else { name.to_string() };
    let prefix = PREFIX.get_or_init(|| regex::Regex::new(r"^(bip\d+|mixamorig\d*|cc_base)[_ .-]*").unwrap());
    let stripped = prefix.replace(&vrm, "");
    let compact: String = stripped
        .chars()
        .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        .collect();
    for role in ROLES {
        let (base, side) = role.split_once('_').unwrap_or((role, ""));
        for alias in aliases(base) {
            let variants = if side.is_empty() {
                vec![alias.to_string()]
            } else {
                let side = side.to_lowercase();
                let word = if side == "l" { "left" } else { "right" };
                vec![
                    format!("{alias}{side}"),
                    format!("{side}{alias}"),
                    format!("{word}{alias}"),
                    format!("{alias}{word}"),
                ]
            };
            if variants.contains(&compact) {
                return Some(role);
            }
        }
    }
    None
}
fn ancestor(child: &str, parent: &str, names: &BTreeMap<&str, &Bone>) -> Result<bool> {
    let mut cursor = Some(child);
    let mut seen = BTreeSet::new();
    while let Some(name) = cursor {
        if !seen.insert(name) {
            return Err(invalid("Cyclic bone hierarchy."));
        }
        if name == parent {
            return Ok(true);
        }
        cursor = names
            .get(name)
            .ok_or_else(|| invalid("Bone references a missing parent."))?
            .parent
            .as_deref();
    }
    Ok(false)
}
fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.into_iter().zip(b).map(|(a, b)| a * b).sum()
}
fn norm(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}
pub fn identify(bones: &[Bone], overrides: &BTreeMap<String, String>) -> Result<Identification> {
    let names: BTreeMap<_, _> = bones.iter().map(|b| (b.name.as_str(), b)).collect();
    if names.len() != bones.len() {
        return Err(invalid("Duplicate bone names."));
    }
    if bones
        .iter()
        .any(|b| b.parent.as_deref().is_some_and(|p| !names.contains_key(p)))
    {
        return Err(invalid("Bone references a missing parent."));
    }
    let mut roles = BTreeMap::new();
    let mut ambiguous = BTreeSet::new();
    for bone in bones {
        if let Some(role) = name_role(&bone.name) {
            if roles.contains_key(role) {
                ambiguous.insert(role);
            } else {
                roles.insert(role.to_string(), bone.name.clone());
            }
        }
    }
    for role in ambiguous {
        roles.remove(role);
    }
    for (role, name) in overrides {
        if !ROLES.contains(&role.as_str()) || !names.contains_key(name.as_str()) {
            return Err(invalid("Invalid landmark/bone override."));
        }
        roles.insert(role.clone(), name.clone());
    }
    let missing: Vec<_> = ROLES
        .into_iter()
        .filter(|r| !roles.contains_key(*r))
        .map(str::to_string)
        .collect();
    let mut errors = Vec::new();
    if !missing.is_empty() {
        errors.push(format!("Assign humanoid roles: {}", missing.join(", ")));
    }
    for name in names.keys() {
        ancestor(name, "", &names)?;
    }
    for chain in CHAINS {
        for pair in chain.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            if let (Some(parent), Some(child)) = (roles.get(a), roles.get(b)) {
                if !ancestor(child, parent, &names)? {
                    errors.push(format!("Hierarchy mismatch: {b} is not below {a}."));
                }
            }
        }
    }
    if missing.is_empty() {
        let pts: BTreeMap<_, _> = roles
            .iter()
            .map(|(role, name)| (role.as_str(), names[name.as_str()].position))
            .collect();
        if pts.values().flatten().any(|v| !v.is_finite()) {
            return Err(invalid("Invalid bone positions."));
        }
        let height = pts["head"][2] - pts["foot_L"][2].min(pts["foot_R"][2]);
        if height <= 0. {
            errors.push("Orient the humanoid upright (Z up).".into());
        } else {
            if pts["head"][2] <= pts["pelvis"][2]
                || pts["thigh_L"][2].min(pts["thigh_R"][2])
                    <= pts["foot_L"][2].max(pts["foot_R"][2])
            {
                errors.push("Head/pelvis/leg positions are not upright humanoid landmarks.".into());
            }
            let mut lateral = sub(pts["thigh_L"], pts["thigh_R"]);
            lateral[2] = 0.;
            let width = norm(lateral);
            if width < height * 0.025 {
                errors.push("Review left/right spatial separation: thigh".into());
            } else {
                lateral = lateral.map(|v| v / width);
            }
            for base in ["upperarm", "hand", "thigh", "foot"] {
                let (l, r) = (
                    pts[format!("{base}_L").as_str()],
                    pts[format!("{base}_R").as_str()],
                );
                let lp = dot(sub(l, pts["pelvis"]), lateral);
                let rp = dot(sub(r, pts["pelvis"]), lateral);
                if lp <= 0. || rp >= 0. || norm(sub(l, r)) < height * 0.025 {
                    errors.push(format!("Review left/right spatial separation: {base}"));
                }
            }
            for chain in CHAINS {
                for pair in chain.windows(2) {
                    let (a, b) = (pair[0], pair[1]);
                    let distance = norm(sub(pts[a], pts[b])) / height;
                    if !(0.005 < distance && distance < 0.6) {
                        errors.push(format!("Implausible segment: {a} → {b}"));
                    }
                }
            }
        }
    }
    Ok(Identification{roles,missing,status:if errors.is_empty(){"Supported Humanoid"}else{"Needs Review"}.into(),errors,evidence:"Names + ancestor chains + left/right and vertical landmark tests; human confirmation required.".into()})
}
pub fn suggest_mapping(
    source: &[Bone],
    target: &[Bone],
    source_roles: &BTreeMap<String, String>,
    target_roles: &BTreeMap<String, String>,
) -> Result<Suggestions> {
    let a = identify(source, source_roles)?;
    let b = identify(target, target_roles)?;
    let mut mapping = BTreeMap::new();
    for (role, name) in &a.roles {
        if let Some(target) = b.roles.get(role) {
            mapping.insert(name.clone(), target.clone());
        }
    }
    let target_names: BTreeSet<_> = target.iter().map(|b| &b.name).collect();
    for bone in source {
        if target_names.contains(&bone.name) {
            mapping
                .entry(bone.name.clone())
                .or_insert_with(|| bone.name.clone());
        }
    }
    Ok(Suggestions {
        source: Some(a),
        target: b,
        mapping,
        assistance: None,
    })
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReviewIssue {
    pub tab: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub roles: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bones: Vec<String>,
}
pub fn review_issues(
    target: &[Bone],
    source: &[Bone],
    weighted: &[String],
    suggested_mode: &str,
    mode: &str,
    options: &deadlock_project::PreparationOptions,
) -> Vec<ReviewIssue> {
    let mut result = Vec::new();
    let mut add = |tab: &str,
                   message: String,
                   scope: Option<&str>,
                   roles: Vec<String>,
                   bones: Vec<String>| {
        result.push(ReviewIssue {
            tab: tab.into(),
            message,
            scope: scope.map(str::to_string),
            roles,
            bones,
        })
    };
    if mode != suggested_mode {
        add(
            "roles",
            "Switch to the suggested input mode before preparing.".into(),
            None,
            vec![],
            vec![],
        );
        return result;
    }
    let get_map = |key: &str| {
        options
            .values
            .get(key)
            .and_then(|v| serde_json::from_value::<BTreeMap<String, String>>(v.clone()).ok())
            .unwrap_or_default()
    };
    let target_roles = get_map("target_roles");
    let source_roles = get_map("source_roles");
    let mut scopes = vec![("target", target, &target_roles)];
    if mode == "rigged_humanoid" {
        scopes.push(("source", source, &source_roles));
    }
    for (scope, bones, roles) in scopes {
        let missing: Vec<_> = ROLES
            .iter()
            .filter(|r| roles.get(**r).is_none_or(String::is_empty))
            .map(|r| r.to_string())
            .collect();
        if !missing.is_empty() {
            add(
                "roles",
                format!("Choose {scope} bones for: {}.", missing.join(", ")),
                Some(scope),
                missing,
                vec![],
            );
            continue;
        }
        let errors = match identify(bones, roles) {
            Ok(value) => value.errors,
            Err(error) => vec![error.to_string()],
        };
        for error in errors {
            add(
                "roles",
                format!(
                    "{} roles: {error}",
                    if scope == "source" {
                        "Source"
                    } else {
                        "Target"
                    }
                ),
                Some(scope),
                vec![],
                vec![],
            );
        }
    }
    if mode == "rigged_humanoid" {
        let mapping = get_map("mapping");
        let preservation = match crate::secondary_bones::selection(source, weighted, target, options) {
            Ok(plan) => plan,
            Err(error) => { add("physics bones", error.to_string(), None, vec![], vec![]); None }
        };
        let body_options = crate::secondary_bones::body_options(options, preservation.as_ref());
        for error in crate::following::errors(source, &body_options) {
            add("mapping", error, None, vec![], vec![]);
        }
        let targets: BTreeSet<_> = target.iter().map(|b| &b.name).collect();
        let missing: Vec<_> = weighted
            .iter()
            .filter(|n| {
                if preservation.as_ref().is_some_and(|p| p.exported_bones.contains_key(*n)) { return false; }
                mapping
                    .get(*n)
                    .is_none_or(|target| !targets.contains(target))
            })
            .cloned()
            .collect();
        if !missing.is_empty() {
            add(
                "mapping",
                format!(
                    "Choose targets for weighted source bones: {}.",
                    missing.join(", ")
                ),
                None,
                vec![],
                missing,
            );
        }
        for (name, primary) in crate::leg_chains::equivalents(source, &source_roles) {
            if weighted.contains(&name) && mapping.get(&name) != mapping.get(&primary) {
                add("mapping", format!("{name} is a parallel leg joint of {primary}; re-detect body mapping so both skin chains use the same leg joint."), None, vec![], vec![name]);
            }
        }
        for (role, value) in options
            .values
            .get("source_roles")
            .and_then(serde_json::Value::as_object)
            .into_iter()
            .flatten()
        {
            let Some(source) = value.as_str() else {
                continue;
            };
            if let Some(target) = target_roles.get(role) {
                if !target.is_empty() && mapping.get(source) != Some(target) {
                    add(
                        "mapping",
                        format!("{source} must map to {target} to match the reviewed {role} role."),
                        None,
                        vec![],
                        vec![source.into()],
                    );
                }
            }
        }
    } else {
        if options.values.get("preserve_physics_bones").is_some_and(|v| v != &serde_json::Value::Bool(false)) {
            add("physics bones", "Keeping existing physics bones requires an already rigged source.".into(), None, vec![], vec![]);
        }
        let landmarks = options.values.get("landmarks");
        let bad: Vec<_> = ROLES
            .iter()
            .filter(|role| {
                landmarks
                    .and_then(|v| v.get(**role))
                    .and_then(|v| v.as_array())
                    .is_none_or(|point| {
                        point.len() != 3
                            || point.iter().any(|v| {
                                v.as_f64()
                                    .or_else(|| v.as_str().and_then(|v| v.parse().ok()))
                                    .is_none_or(|n| !n.is_finite())
                            })
                    })
            })
            .map(|r| r.to_string())
            .collect();
        if !bad.is_empty() {
            add(
                "landmarks",
                format!("Enter three finite coordinates for: {}.", bad.join(", ")),
                None,
                bad,
                vec![],
            );
        }
    }
    result
}
