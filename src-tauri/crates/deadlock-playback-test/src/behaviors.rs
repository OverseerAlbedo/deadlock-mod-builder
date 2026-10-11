//! R4 authored profiles selected for native Haze states; no graph/timing edits.
use crate::{
    animation::{names, normalize, qdistance},
    native::*,
};
use deadlock_resource::{Kind, Value};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub duration_seconds: f64,
    pub looping: bool,
    pub rotations: Vec<Vec<[f64; 4]>>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Behaviors {
    pub schema: u32,
    pub fps: u32,
    pub bone_names: Vec<String>,
    pub profiles: BTreeMap<String, Profile>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Selection {
    pub category: &'static str,
    pub profile: String,
    pub strength: f64,
}
const PROFILES: [&str; 13] = [
    "idle",
    "run_center",
    "run_n",
    "run_ne",
    "run_e",
    "run_se",
    "run_s",
    "run_sw",
    "run_w",
    "run_nw",
    "turn_l",
    "turn_r",
    "stop",
];

pub type RunProfiles = BTreeMap<String, String>;

fn run_profile(x: f64, y: f64) -> Result<String> {
    require(
        x.is_finite() && y.is_finite(),
        "Nonfinite movement blend value",
    )?;
    let length = x.hypot(y);
    if length < 1e-6 {
        return Ok("run_center".into());
    }
    let diagonal = std::f64::consts::FRAC_1_SQRT_2;
    let directions = [
        ("n", 1., 0.),
        ("ne", diagonal, -diagonal),
        ("e", 0., -1.),
        ("se", -diagonal, -diagonal),
        ("s", -1., 0.),
        ("sw", -diagonal, diagonal),
        ("w", 0., 1.),
        ("nw", diagonal, diagonal),
    ];
    let (name, dot) = directions
        .iter()
        .map(|&(name, dx, dy)| (name, (x * dx + y * dy) / length))
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .unwrap();
    require(
        dot > 0.99,
        "Movement blend direction is outside the reviewed eight-direction profile set",
    )?;
    Ok(format!("run_{name}"))
}

/// The native graph's forward/strafe blend coordinates determine when a clip
/// plays. Clip root motion is not authoritative: some clips animate in place,
/// and two reachable crouch clips have root directions unlike their blend slots.
/// Read graph data without rewriting it; reject ambiguous/unsupported mappings.
pub fn collect_graph_run_profiles(root: &Value, profiles: &mut RunProfiles) -> Result<()> {
    let nodes = field(root, "m_nodes")?.array()?;
    let paths = field(root, "m_nodePaths")?.array()?;
    let resources = field(root, "m_resources")?.array()?;
    let index = |v: &Value| -> Result<usize> { Ok(usize::try_from(v.integer()?)?) };
    for node in nodes {
        if field(node, "_class")?.string()? != "CNmBlend2DNode::CDefinition" {
            continue;
        }
        let a = paths
            .get(index(field(node, "m_nInputParameterNodeIdx0")?)?)
            .ok_or("Movement blend input outside graph paths")?
            .string()?;
        let b = paths
            .get(index(field(node, "m_nInputParameterNodeIdx1")?)?)
            .ok_or("Movement blend input outside graph paths")?
            .string()?;
        let forward = match (a, b) {
            ("forward_speed", "strafe_speed") => 0,
            ("strafe_speed", "forward_speed") => 1,
            _ => continue,
        };
        let sources = field(node, "m_sourceNodeIndices")?.array()?;
        let values = field(node, "m_values")?.array()?;
        require(
            !sources.is_empty() && sources.len() == values.len(),
            "Invalid movement blend slots",
        )?;
        for (source, value) in sources.iter().zip(values) {
            let child = nodes
                .get(index(source)?)
                .ok_or("Movement source outside graph nodes")?;
            require(
                field(child, "_class")?.string()? == "CNmClipNode::CDefinition",
                "Unsupported movement blend source; expected native clip",
            )?;
            let path = resources
                .get(index(field(child, "m_nDataSlotIdx")?)?)
                .ok_or("Movement clip slot outside graph resources")?
                .string()?;
            deadlock_vpk::relative_asset(path)?;
            require(
                path.ends_with(".vnmclip"),
                "Movement slot is not an animation clip",
            )?;
            let xy = value.array()?;
            require(xy.len() == 2, "Invalid movement blend coordinate")?;
            let profile = run_profile(number(&xy[forward])?, number(&xy[1 - forward])?)?;
            let path = format!("{path}_c");
            if let Some(prior) = profiles.get(&path) {
                require(
                    prior == &profile,
                    "Conflicting movement directions for the same native clip",
                )?;
            } else {
                profiles.insert(path, profile);
            }
        }
    }
    Ok(())
}
impl Behaviors {
    pub fn validate(&self) -> Result<()> {
        require(
            self.schema == 2 && self.fps == 60 && self.bone_names == names(),
            "Wrong R4 authored behavior library",
        )?;
        require(
            self.profiles.len() == PROFILES.len()
                && PROFILES.iter().all(|n| self.profiles.contains_key(*n)),
            "Incomplete R4 motion profiles",
        )?;
        for (name, profile) in &self.profiles {
            let looping = name == "idle" || name.starts_with("run_");
            require(
                profile.looping == looping
                    && profile.duration_seconds == if looping { 2. } else { 1. },
                "Unexpected authored profile duration/mode",
            )?;
            require(
                profile.rotations.len() == if looping { 121 } else { 61 },
                "Incomplete Blender-evaluated profile",
            )?;
            let mut maximum = 0.0f64;
            for frame in &profile.rotations {
                require(
                    frame.len() == 7,
                    "Authored frame must contain seven streamer bones",
                )?;
                for &q in frame {
                    require(
                        q.iter().all(|x| x.is_finite())
                            && (q.iter().map(|x| x * x).sum::<f64>() - 1.).abs() < 1e-5,
                        "Invalid authored profile quaternion",
                    )?;
                    maximum = maximum.max(qdistance(q, [0., 0., 0., 1.]));
                }
            }
            require(
                maximum > 0.003 && maximum < 0.25,
                "Profile is static or exceeds restrained local-angle limit",
            )?;
            for j in 0..7 {
                let first = profile.rotations[0][j];
                let last = profile.rotations.last().unwrap()[j];
                require(
                    qdistance(first, last) < 0.001,
                    "Profile endpoints are discontinuous",
                )?;
                if !looping {
                    require(
                        qdistance(first, [0., 0., 0., 1.]) < 0.001
                            && qdistance(last, [0., 0., 0., 1.]) < 0.001,
                        "One-shot profile must enter and leave at neutral",
                    )?;
                }
            }
        }
        Ok(())
    }
    pub fn sample(&self, selection: &Selection, t: f64, j: usize) -> [f64; 4] {
        let p = &self.profiles[&selection.profile];
        let frame = t.clamp(0., 1.) * (p.rotations.len() - 1) as f64;
        let lo = frame.floor() as usize;
        let hi = (lo + 1).min(p.rotations.len() - 1);
        let a = p.rotations[lo][j];
        let mut b = p.rotations[hi][j];
        if a.iter().zip(b).map(|(a, b)| a * b).sum::<f64>() < 0. {
            b = b.map(|x| -x);
        }
        let mut q = normalize(std::array::from_fn(|i| {
            a[i] + (b[i] - a[i]) * (frame - lo as f64)
        }));
        if q[3] < 0. {
            q = q.map(|x| -x);
        }
        // Reduce crouch/walk amplitude without changing the authored axes.
        normalize(std::array::from_fn(|i| {
            let identity = if i == 3 { 1. } else { 0. };
            identity + (q[i] - identity) * selection.strength
        }))
    }
    pub fn samples(&self, selection: &Selection, frames: usize) -> Vec<Vec<[f64; 4]>> {
        (0..frames)
            .map(|f| {
                (0..7)
                    .map(|j| self.sample(selection, f as f64 / (frames - 1) as f64, j))
                    .collect()
            })
            .collect()
    }
}

/// Explicit clip families avoid the R1-R3 broad "idle" substring rule, which
/// also swept death/reload/aim helper resources. One-frame poses stay static.
pub fn select(path: &str, root: &Value, run_profiles: &RunProfiles) -> Result<Option<Selection>> {
    let frames = field(root, "m_nNumFrames")?.integer()?;
    let duration = number(field(root, "m_flDuration")?)?;
    if frames < 2 || !(0.1..=60.).contains(&duration) {
        return Ok(None);
    }
    let name = path
        .rsplit('/')
        .next()
        .unwrap_or(path)
        .strip_suffix(".vnmclip_c")
        .unwrap_or("");
    let Kind::Bool(additive) = field(root, "m_bIsAdditive")?.kind else {
        return Err("Invalid clip additive flag".into());
    };
    let chosen = |category, profile: &str, strength| {
        Some(Selection {
            category,
            profile: profile.into(),
            strength,
        })
    };
    if additive {
        return Ok(match name {
            "run_to_stop_stand" => chosen("stop", "stop", 1.),
            "run_to_stop_crouch" => chosen("stop", "stop", 0.65),
            _ => None,
        });
    }
    if [
        "weapon_stand_idle",
        "idle_hideout",
        "idle_hideout_crouch",
        "hideout_stand_idle",
        "hideout_crouch_idle",
        "invis_stand_idle",
        "invis_crouch_idle",
        "item_stand_idle",
        "shopping_idle",
        "ui_shop",
        "ui_shop_idle",
    ]
    .contains(&name)
    {
        return Ok(chosen(
            "idle",
            "idle",
            if name.contains("crouch") { 0.65 } else { 1. },
        ));
    }
    for (prefix, strength) in [
        ("weapon_crouch_run_", 0.6),
        ("item_crouch_run_", 0.6),
        ("out_of_combat_crouch_run_", 0.6),
        ("zoomed_crouch_run_", 0.6),
        ("hideout_crouch_walk_", 0.45),
        ("weapon_run_", 1.),
        ("hideout_run_", 1.),
        ("invis_run_", 1.),
        ("item_run_", 1.),
        ("out_of_combat_run_", 1.),
    ] {
        if let Some(direction) = name.strip_prefix(prefix) {
            if ["center", "n", "ne", "e", "se", "s", "sw", "w", "nw"].contains(&direction) {
                return Ok(run_profiles
                    .get(path)
                    .and_then(|profile| chosen("run", profile, strength)));
            }
        }
    }
    Ok(match name {
        "turn_left" | "turn_hideout_l" => chosen("turn", "turn_l", 1.),
        "turn_right" | "turn_hideout_r" => chosen("turn", "turn_r", 1.),
        "turn_crouch_left" | "turn_hideout_crouch_l" => chosen("turn", "turn_l", 0.65),
        "turn_crouch_right" | "turn_hideout_crouch_r" => chosen("turn", "turn_r", 0.65),
        _ => None,
    })
}
