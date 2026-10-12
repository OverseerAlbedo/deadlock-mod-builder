//! Per-source joint-frame proxies. Never edits rendered geometry or FE arrays.
use crate::{
    hash, invalid,
    motion::{self, MotionProfile},
    surface::{self, Editable},
    surface_geometry::{norm, sub, Surface},
    Result,
};
use deadlock_core::CancellationToken;
use deadlock_dmx::Dmx;
use deadlock_project::Project;
use deadlock_skeleton::{extensions::SourceBinding, transforms};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const POLICY: &str = "automatic-part-motion-v9";
pub const TANG_SOURCE: &str = "5aea8de41d4e403c4b356527048ac81da240822785f0856ac826d706e3d47484";
pub const TANG_PROXY: &str = "1eebba6483d63387a060fd35331e90868397a835356a3ab8a77c65ac25e9c8b2";
pub const TANG_REFERENCE_PROXY: &str =
    "8ef0c2fc7e3ae155d0fa8709a5d452953ff5ea2c494418cfb93963176316ce07";
const PREFIX: &str = "dmb_secondary_";
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Group {
    pub source_root: String,
    pub root: String,
    pub anchor: String,
    pub bones: Vec<String>,
    pub weighted_bones: Vec<String>,
}
#[derive(Debug, Serialize)]
pub struct HairLengthAdjustment {
    pub root: String,
    pub strand_length_inches: f64,
    pub short_hair_limit_inches: f64,
    pub goal_strength_multiplier: f64,
    pub scale_reference: String,
}
#[derive(Debug, Serialize)]
pub struct HairMotion {
    pub policy: String,
    pub shared_profile_groups: Vec<String>,
    pub custom_profile_groups: Vec<String>,
    pub review_groups: Vec<String>,
    pub length_adjusted_groups: Vec<HairLengthAdjustment>,
    pub jump_protected_groups: Vec<crate::hair_limits::JumpProtection>,
}
#[derive(Debug, Serialize)]
pub struct Setup {
    pub policy: String,
    pub preset: String,
    pub source_sha256: String,
    pub proxy_sha256: String,
    pub groups: Vec<Group>,
    pub profiles: BTreeMap<String, String>,
    pub parts: Vec<crate::parts::Part>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hair_motion: Option<HairMotion>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub long_skirt: Option<crate::long_skirt::Plan>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachments: Option<crate::attachments::Report>,
    pub roll_lock_chains: Vec<deadlock_cloth::generated::roll_lock::Chain>,
    pub reused_accepted_reference: bool,
    pub proxy_positions: usize,
    pub dynamic_positions: usize,
    pub render_source_modified: bool,
    pub body_collisions_authored: bool,
    pub replaces_original_cloth_solver: bool,
    pub native_compiled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preset_fit: Option<crate::tangtang_fit::FitReceipt>,
    #[serde(skip)]
    pub proxy: Vec<u8>,
}
impl Setup {
    pub fn fabric_stability(&self) -> deadlock_cloth::generated::sheet_stability::Plan {
        let mut plan=self.long_skirt.as_ref().map(|p|p.stability.clone()).unwrap_or_default();
        if let Some(attachments)=&self.attachments {plan.attachments=attachments.plan.clone();}
        plan
    }
    pub fn requires_animation_limits(&self) -> bool {
        self.preset == "tangtang_reference" || self.reused_accepted_reference
            || self.roll_lock_chains.iter().any(|c| !c.per_joint_reference)
            || self.hair_motion.as_ref().is_some_and(|h| !h.jump_protected_groups.is_empty())
            || self.long_skirt.is_some()
    }
}
pub fn active(p: &Project) -> bool {
    p.customization.active()
        && p.customization
            .secondary_motion
            .as_ref()
            .is_some_and(|v| v.enabled == Some(true))
}
pub fn profile(name: &str) -> Result<MotionProfile> {
    let (goal_at_anchor, goal_at_hem, drag) = match name {
        "balanced" => (0.65, 0.35, 0.15),
        "stiff" => (0.88, 0.65, 0.20),
        "soft" => (0.58, 0.27, 0.12),
        _ => {
            return Err(invalid(
                "Choose balanced, stiff or soft secondary movement.",
            ))
        }
    };
    Ok(MotionProfile {
        goal_at_anchor,
        goal_at_hem,
        drag,
        antishrink: 1.,
    })
}
pub fn reviewed_groups(d: &Dmx, roots: &[String], source: &SourceBinding) -> Result<Vec<Group>> {
    if roots.is_empty()
        || roots.len() > 128
        || roots.iter().collect::<BTreeSet<_>>().len() != roots.len()
    {
        return Err(invalid("Select 1–128 unique reviewed secondary groups."));
    }
    let bind = surface::binds(d)?;
    let sk = &bind.skeleton;
    let allowed = source.extra_names.iter().collect::<BTreeSet<_>>();
    let lookup = sk.indices();
    let mut children = vec![vec![]; sk.names.len()];
    for (i, &parent) in sk.parents.iter().enumerate() {
        if parent >= 0 {
            children[parent as usize].push(i);
        }
    }
    let mut weighted = BTreeSet::new();
    for row in surface::mesh_data(d)?.values() {
        for (ids, weights) in row.joints.iter().zip(&row.weights) {
            for (&j, &w) in ids.iter().zip(weights) {
                if w > 0. {
                    let e = bind
                        .palette
                        .get(j)
                        .and_then(|i| d.elements.get(*i))
                        .ok_or_else(|| invalid("Secondary skin index is out of range."))?;
                    weighted.insert(e.name.clone());
                }
            }
        }
    }
    let mut occupied = BTreeSet::new();
    let mut groups = vec![];
    for source_root in roots {
        let name = format!("{PREFIX}{source_root}");
        let &root=lookup.get(name.as_str()).ok_or_else(||invalid(format!("The prepared model has no moving-part bones for {source_root}. If you changed moving parts or breast motion, convert again in Model preparation and click Use prepared model; otherwise enable Keep existing physics bones and convert.")))?;
        let parent = sk.parents[root];
        if parent < 0 || allowed.contains(&sk.names[parent as usize]) {
            return Err(invalid(
                "Secondary groups must attach directly to a verified original body anchor.",
            ));
        }
        let mut pending = vec![root];
        let mut branch = vec![];
        while let Some(at) = pending.pop() {
            if !allowed.contains(&sk.names[at]) || !occupied.insert(at) || occupied.len() > 512 {
                return Err(invalid(
                    "Secondary groups overlap, include a body bone or exceed 512 bones.",
                ));
            }
            branch.push(sk.names[at].clone());
            pending.extend(children[at].iter().rev());
        }
        let used = branch
            .iter()
            .filter(|n| weighted.contains(*n))
            .cloned()
            .collect::<Vec<_>>();
        if used.is_empty() {
            return Err(invalid(format!(
                "Secondary group has no weighted mesh: {source_root}"
            )));
        }
        groups.push(Group {
            source_root: source_root.clone(),
            root: name,
            anchor: sk.names[parent as usize].clone(),
            bones: branch,
            weighted_bones: used,
        });
    }
    Ok(groups)
}
pub fn setup(
    p: &Project,
    d: &Dmx,
    source: Option<&SourceBinding>,
    cancel: &CancellationToken,
) -> Result<Option<Setup>> {
    if !active(p) {
        return Ok(None);
    }
    if !p.customization.model_enabled() {
        return Err(invalid("Secondary motion needs a replacement model."));
    }
    if p.customization
        .physics
        .as_ref()
        .is_some_and(|v| v.enabled == Some(true))
    {
        return Err(invalid("Choose one replacement physics mode."));
    }
    let source=source.ok_or_else(||invalid("The selected replacement meshes do not contain a preserved secondary rig. In Mesh mapping, include the prepared meshes with moving parts, then Analyze again. If the converted model has no physics bones, enable Keep existing physics bones and convert it again."))?;
    let sha = hash(d.bytes());
    if sha != source.source_sha256 {
        return Err(invalid("Secondary source changed after analysis."));
    }
    let cfg = p.customization.secondary_motion.as_ref().unwrap();
    let preset = cfg.preset.as_deref().unwrap_or("automatic");
    let tang = matches!(preset, "tangtang_r2" | "tangtang_reference");
    let opts = &p.preparation_options.values;
    if opts.get("preserve_physics_bones").and_then(|v| v.as_bool()) != Some(true) {
        return Err(invalid("Automatic secondary motion uses the physics groups reviewed in model preparation. Enable Keep existing physics bones and convert first."));
    }
    let reviewed: Vec<String> =
        serde_json::from_value(opts.get("physics_roots").cloned().unwrap_or_default())?;
    let roots = if tang {
        &reviewed
    } else {
        cfg.roots.as_ref().unwrap_or(&reviewed)
    };
    if roots.iter().any(|r| !reviewed.contains(r)) {
        return Err(invalid(
            "A motion group was not reviewed in model preparation.",
        ));
    }
    let groups = reviewed_groups(d, roots, source)?;
    let mut profiles = if tang {
        BTreeMap::new()
    } else {
        cfg.profiles.clone().unwrap_or_default()
    };
    profiles.retain(|_,v|v!="automatic");
    if profiles.keys().any(|r| !roots.contains(r)) {
        return Err(invalid(
            "A saved motion profile refers to an unselected group.",
        ));
    }
    let mut part_choices=if tang {BTreeMap::new()}else{cfg.part_kinds.clone().unwrap_or_default()};
    // Part types chosen in model preparation (for example breast motion) apply
    // unless the part type was changed on the Motion page.
    if !tang {
        for (root, kind) in opts.get("physics_kinds").and_then(|v| v.as_object()).into_iter().flatten() {
            if let Some(kind) = kind.as_str().filter(|_| roots.contains(root) && !part_choices.contains_key(root)) {
                part_choices.insert(root.clone(), kind.to_owned());
            }
        }
    }
    let mut parts=crate::parts::analyze(d,&groups,&part_choices,cancel)?;
    // Recognize the complete accepted rig, never a partial name match. Rebind
    // its existing proxy for proportion changes using the existing strict fit.
    #[derive(Deserialize)] struct Reference { groups:Vec<Group> }
    let saved:Reference=serde_json::from_str(include_str!("../resources/tangtang-r2-groups.json"))?;
    let by_root=|g:&[Group]|g.iter().map(|v|(v.source_root.clone(),v.clone())).collect::<BTreeMap<_,_>>();
    let mut reuse=preset=="automatic" && profiles.is_empty() && cfg.part_kinds.as_ref().is_none_or(|v|v.is_empty())
        && p.source_model()=="models/heroes_staging/haze/haze.vmdl" && by_root(&groups)==by_root(&saved.groups);
    let mut automatic_fit=None;
    if reuse && sha!=TANG_SOURCE {
        let reference=Dmx::parse(include_bytes!("../resources/tangtang-hinged-streamers-proxy.dmx").to_vec())?;
        match crate::tangtang_fit::rebind(d,&reference,cancel) {
            Ok(fitted)=>automatic_fit=Some(fitted),
            // A different model may coincidentally use the same rig names.
            // Automatic must then author from that model, not force a preset.
            Err(crate::Error::Invalid(_))=>reuse=false,
            Err(error)=>return Err(error),
        }
    }
    let effective=if reuse {"tangtang_reference"}else{preset};
    let long_skirt = if effective == "automatic" {
        crate::long_skirt::analyze(d, &groups, &mut parts, &part_choices, cancel)?
    } else { None };
    let attachments=if effective=="automatic" {
        crate::attachments::analyze(d,&groups,&parts,&part_choices,&profiles,long_skirt.as_ref(),cancel)?
    } else {None};
    let hair_bind = surface::binds(d)?.skeleton;
    let hair_state = transforms::bind_state(&hair_bind)?;
    let hair_motion = if effective == "automatic" {
        let mut report = HairMotion {
            policy: crate::hair_motion::POLICY.into(),
            shared_profile_groups: vec![], custom_profile_groups: vec![], review_groups: vec![],
            length_adjusted_groups: vec![],
            jump_protected_groups: vec![],
        };
        for (part, group) in parts.iter().zip(&groups).filter(|(p,_)| p.kind == crate::parts::Kind::Hair) {
            if part.weighted_chain.len() < 2 || part.chain_length_inches <= 1e-4 {
                report.review_groups.push(part.root.clone());
            } else if profiles.contains_key(&part.root) {
                report.custom_profile_groups.push(part.root.clone());
            } else {
                report.shared_profile_groups.push(part.root.clone());
                if let Some(adjustment) = crate::hair_motion::length_adjustment(group, part, &hair_bind, &hair_state, None) {
                    report.jump_protected_groups.push(crate::hair_limits::plan(group, part, &hair_bind, &hair_state, &adjustment)?);
                    report.length_adjusted_groups.push(adjustment);
                }
            }
        }
        Some(report)
    } else { None };
    if effective=="tangtang_reference" {
        for part in &mut parts {
            if ["spring_base_L_a_01_jnt","spring_base_R_a_01_jnt"].contains(&part.root.as_str()) {
                part.kind=crate::parts::Kind::Streamer;part.roll_locked=true;
                part.reason="Complete accepted TangTang rig and streamer reference".into();part.review_recommended=false;
            } else { part.roll_locked=false; }
        }
    }
    if effective=="tangtang_r2" {for part in &mut parts{part.roll_locked=false;}}
    let roll_lock_chains=parts.iter().zip(&groups).filter(|(p,_)|p.roll_locked).map(|(p,g)|deadlock_cloth::generated::roll_lock::Chain{anchor:g.anchor.clone(),bones:p.weighted_chain.clone(),per_joint_reference:matches!(p.kind,crate::parts::Kind::Hair|crate::parts::Kind::Tail)}).collect();
    let mut preset_fit = None;
    let (proxy, positions, dynamic) = match effective {
        "tangtang_r2" | "tangtang_reference" => {
            if p.source_model() != "models/heroes_staging/haze/haze.vmdl" {
                return Err(invalid("TangTang presets require the Haze conversion. Use Automatic for another hero."));
            }
            #[derive(Deserialize)]
            struct Saved {
                groups: Vec<Group>,
            }
            let saved: Saved =
                serde_json::from_str(include_str!("../resources/tangtang-r2-groups.json"))?;
            let by_root = |g: &[Group]| {
                g.iter()
                    .map(|v| (v.source_root.clone(), v.clone()))
                    .collect::<BTreeMap<_, _>>()
            };
            if by_root(&groups) != by_root(&saved.groups) || !profiles.is_empty() {
                return Err(invalid("TangTang presets use all 38 saved groups and their saved tuning. Choose Automatic for custom group settings."));
            }
            let (bytes, expected): (&[u8], &str) = if effective == "tangtang_reference" {
                (
                    include_bytes!("../resources/tangtang-hinged-streamers-proxy.dmx"),
                    TANG_REFERENCE_PROXY,
                )
            } else {
                (
                    include_bytes!("../resources/tangtang-r2-proxy.dmx"),
                    TANG_PROXY,
                )
            };
            let mut proxy = bytes.to_vec();
            if hash(&proxy) != expected {
                return Err(invalid("Bundled TangTang proxy changed."));
            }
            if sha != TANG_SOURCE {
                let (fitted, receipt) = if let Some(fitted)=automatic_fit.take(){fitted}else{crate::tangtang_fit::rebind(d, &Dmx::parse(proxy)?, cancel)?};
                proxy = fitted;
                preset_fit = Some(receipt);
            }
            (proxy, 900, 786)
        }
        "automatic" => generate_inner(d, &groups, &profiles, Some(&parts), long_skirt.as_ref(), cancel)?,
        _ => return Err(invalid("Unknown secondary motion preset.")),
    };
    cancel.check()?;
    let checked = Dmx::parse(proxy.clone())?;
    let source_bind = surface::binds(d)?.skeleton;
    let proxy_bind = surface::binds(&checked)?.skeleton;
    if source_bind.names != proxy_bind.names
        || source_bind.parents != proxy_bind.parents
        || source_bind.positions != proxy_bind.positions
        || source_bind.rotations != proxy_bind.rotations
        || source_bind.scales != proxy_bind.scales
    {
        return Err(invalid("Secondary proxy changed the source skeleton."));
    }
    Ok(Some(Setup {
        policy: POLICY.into(),
        preset: preset.into(),
        source_sha256: sha,
        proxy_sha256: hash(&proxy),
        groups,
        profiles,
        parts,
        hair_motion,
        long_skirt,
        attachments,
        roll_lock_chains,
        reused_accepted_reference:reuse,
        proxy_positions: positions,
        dynamic_positions: dynamic,
        render_source_modified: false,
        body_collisions_authored: false,
        replaces_original_cloth_solver: true,
        native_compiled: false,
        preset_fit,
        proxy,
    }))
}
fn frame(
    surface: &mut Surface,
    center: [f64; 3],
    basis: [[f64; 3]; 3],
    radius: f64,
    bone: usize,
    dynamic: bool,
) -> [usize; 3] {
    let at = surface.points.len();
    for angle in [
        0.,
        std::f64::consts::TAU / 3.,
        std::f64::consts::TAU * 2. / 3.,
    ] {
        surface.points.push(std::array::from_fn(|r| {
            center[r] + radius * (angle.cos() * basis[r][0] + angle.sin() * basis[r][2])
        }));
        surface.influences.push(BTreeMap::from([(bone, 1.)]));
        surface.enabled.push(dynamic);
    }
    let ids = [at, at + 1, at + 2];
    surface.faces.push(ids);
    ids
}
fn connect(s: &mut Surface, a: [usize; 3], b: [usize; 3]) {
    let cost = |k: usize| {
        (0..3)
            .map(|i| norm(sub(s.points[a[i]], s.points[b[(i + k) % 3]])).powi(2))
            .sum::<f64>()
    };
    let shift = (0..3).min_by(|&a, &b| cost(a).total_cmp(&cost(b))).unwrap();
    let b = std::array::from_fn::<_, 3, _>(|i| b[(i + shift) % 3]);
    for i in 0..3 {
        let j = (i + 1) % 3;
        s.faces.extend([[a[i], b[i], b[j]], [a[i], b[j], a[j]]]);
    }
}
pub fn generate(
    d: &Dmx,
    groups: &[Group],
    profiles: &BTreeMap<String, String>,
    cancel: &CancellationToken,
) -> Result<(Vec<u8>, usize, usize)> {
    generate_inner(d,groups,profiles,None,None,cancel)
}
fn generate_inner(d:&Dmx,groups:&[Group],profiles:&BTreeMap<String,String>,parts:Option<&[crate::parts::Part]>,long_skirt:Option<&crate::long_skirt::Plan>,cancel:&CancellationToken)->Result<(Vec<u8>,usize,usize)> {
    let bind = surface::binds(d)?;
    let sk = &bind.skeleton;
    let state = transforms::bind_state(sk)?;
    let lookup = sk.indices();
    let slots = bind
        .palette
        .iter()
        .enumerate()
        .map(|(i, &e)| (d.elements[e].name.as_str(), i))
        .collect::<BTreeMap<_, _>>();
    let mut s = Surface {
        points: vec![],
        faces: vec![],
        influences: vec![],
        enabled: vec![],
        initial_proxy_positions: 0,
        fixed_positions_not_on_dynamic_faces: 0,
        components: groups.len(),
        degenerate_triangles_excluded: 0,
        overlay_followers: serde_json::Value::Null,
    };
    let position = |i: usize| std::array::from_fn::<_, 3, _>(|r| state.world[i][r][3]);
    let basis = |i: usize| {
        std::array::from_fn::<_, 3, _>(|r| {
            std::array::from_fn::<_, 3, _>(|c| state.world[i][r][c] / state.accumulated[i])
        })
    };
    let mut ranges = vec![];
    let mut frames = BTreeMap::new();
    let mut part_maps=vec![];
    let mut breast_rings=vec![];
    // Each breast of a left/right pair on one anchor knows where the other is.
    let breasts:Vec<(usize,&str,[f64;3])>=parts.map(|parts|groups.iter().zip(parts).enumerate()
        .filter(|(_,(_,p))|p.kind==crate::parts::Kind::Breast)
        .map(|(i,(g,p))|(i,g.anchor.as_str(),position(lookup[p.weighted_chain.first().unwrap_or(&g.root).as_str()]))).collect()).unwrap_or_default();
    for (ordinal,group) in groups.iter().enumerate() {
        cancel.check()?;
        let root = lookup[group.root.as_str()];
        let b = basis(root);
        let center = position(root);
        let mut lengths = vec![];
        for name in group.bones.iter().skip(1) {
            let i = lookup[name.as_str()];
            let length = norm(sub(position(i), position(sk.parents[i] as usize)));
            if !length.is_finite() || length <= 1e-5 {
                return Err(invalid(format!("Secondary joints share a pivot: {name}")));
            }
            lengths.push(length);
        }
        lengths.sort_by(f64::total_cmp);
        let n = lengths.len();
        let pitch = if n == 0 {
            1.
        } else if n % 2 == 0 {
            (lengths[n / 2 - 1] + lengths[n / 2]) / 2.
        } else {
            lengths[n / 2]
        };
        let radius = (pitch * 0.08).clamp(0.05, 0.30);
        let offset = (radius * 3.).max((pitch * 0.25).min(1.));
        let first = s.points.len();
        if let Some(part)=parts.map(|p|&p[ordinal]) {
            let pair:Vec<_>=breasts.iter().filter(|b|b.1==group.anchor).collect();
            let partner=if pair.len()==2 {pair.iter().find(|b|b.0!=ordinal).map(|b|b.2)} else {None};
            if let Some(ring)=crate::secondary_auto::append(&mut s,group,part,&slots,&lookup,sk,&state,profiles.get(&group.source_root).map(String::as_str),long_skirt.and_then(|plan| plan.group(&group.source_root)),&mut part_maps,partner)? {
                breast_rings.push((group.anchor.clone(),ring));
            }
            continue;
        }
        let fixed = frame(
            &mut s,
            std::array::from_fn(|r| center[r] - b[r][1] * offset),
            b,
            radius,
            slots[group.anchor.as_str()],
            false,
        );
        for name in &group.bones {
            let i = lookup[name.as_str()];
            let ring = frame(
                &mut s,
                position(i),
                basis(i),
                radius,
                slots[name.as_str()],
                true,
            );
            let parent = if name == &group.root {
                fixed
            } else {
                *frames
                    .get(&sk.names[sk.parents[i] as usize])
                    .ok_or_else(|| invalid("Secondary hierarchy is not parent-first."))?
            };
            connect(&mut s, parent, ring);
            frames.insert(name.clone(), ring);
        }
        ranges.push((
            first..s.points.len(),
            profile(
                profiles
                    .get(&group.source_root)
                    .map(String::as_str)
                    .unwrap_or("balanced"),
            )?,
        ));
    }
    crate::secondary_auto::link_breast_pairs(&mut s, &breast_rings);
    if let Some(plan) = long_skirt { crate::long_skirt::couple(&mut s, &slots, plan)?; }
    let positions = s.points.len();
    let dynamic = s.enabled.iter().filter(|v| **v).count();
    if groups.is_empty() || positions > 3840 || dynamic == 0 {
        return Err(invalid(
            "Secondary proxy is empty or exceeds its bounded capacity.",
        ));
    }
    let row = surface::mesh_data(d)?
        .into_values()
        .next()
        .ok_or_else(|| invalid("Secondary source contains no mesh."))?;
    let mut edited = Editable::new(d);
    surface::write_surface(&mut edited, &row, &s)?;
    let raw = surface::prune_proxy(edited, &BTreeSet::from([row.mesh]))?;
    let raw=if parts.is_some(){crate::secondary_auto::write_maps(&raw,&part_maps)?}else{
        motion::author_motion_maps_by_group(&raw, &profile("balanced")?, &ranges, cancel)?.0
    };
    Ok((raw, positions, dynamic))
}
