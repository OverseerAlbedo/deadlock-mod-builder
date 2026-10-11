//! Existing physics bindings authorize reviewed ordinary bone branches only.
//! No new runtime flags, simulation or motion are generated here.
use crate::{algorithms::Bone, invalid, Result};
use deadlock_project::PreparationOptions;
use deadlock_skeleton::{extensions, transforms::{self, PreparedBone}, Skeleton};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub const PREFIX: &str = "dmb_secondary_";
pub const POLICY: &str = "preserve-source-physics-bones-v1";
pub const BINDING_POLICY: &str = "verified-rigid-body-bone-binding-v1";
/// Keep in sync with secondary_bones.py and src/preparation/secondary-bones.ts.
pub const HINTS: [&str; 28] = ["hair","ponytail","braid","ribbon","streamer","skirt","cloth","cape","tail","robe","sash","scarf","shawl","mantle","cloak","tassel","veil",
    "髪","发","髮","尻尾","しっぽ","耳","リボン","裙","マント","スカート","スカーフ"];
fn named_secondary(name:&str)->bool {
    let n=name.to_lowercase();HINTS.iter().any(|s|n.contains(s)) || n.split(|c:char|!c.is_alphanumeric()).any(|s|["ear","ears"].contains(&s))
}
/// Unnamed hanging parts (sashes, streamers, chains named in another language):
/// a non-role chain of at least three weighted bones attached directly to the
/// pelvis, spine, chest, neck or head that spans a quarter of the torso.
/// Limb chains (fingers, toes) never qualify because they hang off hands and feet.
fn hanging_chain(bone:&Bone,names:&BTreeMap<&str,&Bone>,children:&BTreeMap<&str,Vec<&str>>,weighted:&BTreeSet<&str>,source_roles:&BTreeMap<String,String>)->bool {
    let roles:BTreeSet<&str>=source_roles.values().map(String::as_str).collect();
    if roles.contains(bone.name.as_str()) {return false;}
    let Some(parent)=bone.parent.as_deref() else {return false;};
    if !["pelvis","spine","chest","neck","head"].iter().any(|r|source_roles.get(*r).is_some_and(|n|n==parent)) {return false;}
    let (Some(pelvis),Some(head))=(source_roles.get("pelvis").and_then(|n|names.get(n.as_str())),source_roles.get("head").and_then(|n|names.get(n.as_str()))) else {return false;};
    let distance=|a:&[f64;3],b:&[f64;3]|(0..3).map(|k|(a[k]-b[k]).powi(2)).sum::<f64>().sqrt();
    let torso=distance(&pelvis.position,&head.position);
    if torso<=1e-6 {return false;}
    let mut chain=vec![bone];
    let mut cursor=bone.name.as_str();
    while let Some(next)=children.get(cursor) {
        if next.len()!=1 || chain.len()>=512 {return false;}
        cursor=next[0];
        let Some(child)=names.get(cursor) else {return false;};
        chain.push(child);
    }
    chain.len()>=3 && chain.iter().filter(|b|weighted.contains(b.name.as_str())).count()>=3
        && !chain.iter().any(|b|roles.contains(b.name.as_str()))
        && distance(&chain[0].position,&chain[chain.len()-1].position)>=0.25*torso
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PhysicsGroup {
    pub root: String,
    pub source_anchor: String,
    pub target_anchor: String,
    pub source_bones: Vec<String>,
    pub weighted_bones: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PreservationPlan {
    pub policy: String,
    pub groups: Vec<PhysicsGroup>,
    pub exported_bones: BTreeMap<String, String>,
    pub new_physics: bool,
    pub in_game_motion_tested: bool,
}
fn roles(options: &PreparationOptions, key: &str) -> Result<BTreeMap<String,String>> {
    serde_json::from_value(options.values.get(key).cloned().unwrap_or_default())
        .map_err(|_| invalid("Review the source and target body roles before keeping physics bones."))
}
pub fn candidates(source: &[Bone], weighted: &[String], source_roles: &BTreeMap<String,String>, target_roles: &BTreeMap<String,String>) -> Result<Vec<PhysicsGroup>> {
    let names: BTreeMap<_,_> = source.iter().map(|b| (b.name.as_str(),b)).collect();
    if names.len() != source.len() { return Err(invalid("Duplicate source bone identities.")); }
    let mut children=BTreeMap::<&str,Vec<&str>>::new();
    for bone in source{if let Some(parent)=bone.parent.as_deref(){children.entry(parent).or_default().push(&bone.name);}}
    let mut physical: BTreeSet<_> = source.iter().filter(|b| b.details.get("physics").is_some_and(|p| p["policy"] == BINDING_POLICY && p["dynamic_bodies"].as_array().is_some_and(|v| !v.is_empty() && v.iter().all(|n| n.as_str().is_some_and(|n| !n.is_empty()))))).map(|b| b.name.as_str()).collect();
    let verified=physical.clone();
    // FBX/GLB rigs often have usable secondary chains without rigid bodies.
    // Require a semantic hint (or a long hanging chain) PLUS multiple weighted, spatially separated
    // joints. Existing physical rigs always retain their original boundaries.
    let weighted_set:BTreeSet<&str>=weighted.iter().map(String::as_str).collect();
    for bone in source.iter().filter(|b|named_secondary(&b.name)||hanging_chain(b,&names,&children,&weighted_set,source_roles)) {
        let mut branch=BTreeSet::new();let mut pending=vec![bone.name.as_str()];
        while let Some(n)=pending.pop(){if !branch.insert(n)||branch.len()>512{return Err(invalid("Secondary chain is cyclic or exceeds 512 bones."));}pending.extend(children.get(n).into_iter().flatten().copied());}
        if branch.iter().any(|n|verified.contains(n)||source_roles.values().any(|r|r==n)
            || names[n].details.get("physics").is_some_and(|p|["dynamic_bodies","kinematic_bodies"].iter().any(|k|p[k].as_array().is_some_and(|a|!a.is_empty())))){continue;}
        let used=weighted.iter().filter(|n|branch.contains(n.as_str())).map(|n|names[n.as_str()]).collect::<Vec<_>>();
        if used.len()>=2 && used.iter().skip(1).any(|b|(0..3).map(|k|(b.position[k]-used[0].position[k]).powi(2)).sum::<f64>()>1e-10){physical.insert(bone.name.as_str());}
    }
    let mut groups = vec![];
    for &root in &physical {
        let mut cursor = names[root].parent.as_deref();
        let mut seen = BTreeSet::from([root]);
        let mut dynamic_parent = false;
        let mut anchor = None;
        while let Some(name) = cursor {
            if !seen.insert(name) || !names.contains_key(name) { return Err(invalid("Invalid source physics bone ancestry.")); }
            dynamic_parent |= physical.contains(name);
            if anchor.is_none() && source_roles.values().any(|n| n == name) { anchor = Some(name); }
            cursor = names[name].parent.as_deref();
        }
        let Some(anchor) = anchor else { continue; };
        if dynamic_parent { continue; }
        let mut branch = vec![];
        let mut queue = VecDeque::from([root]);
        while let Some(name) = queue.pop_front() {
            if branch.iter().any(|n| n == name) || branch.len() >= 512 { return Err(invalid("Source physics branch is cyclic or exceeds 512 bones.")); }
            branch.push(name.to_string());
            for (&child,b) in &names { if b.parent.as_deref() == Some(name) { queue.push_back(child); } }
        }
        if branch.iter().any(|n| source_roles.values().any(|r| r == n)) { continue; }
        let used: Vec<_> = weighted.iter().filter(|n| branch.contains(n)).cloned().collect::<BTreeSet<_>>().into_iter().collect();
        if used.is_empty() { continue; }
        let Some((role,_)) = source_roles.iter().find(|(_,n)| n.as_str() == anchor) else { continue; };
        let Some(destination) = target_roles.get(role).filter(|n| !n.is_empty()) else { continue; };
        groups.push(PhysicsGroup { root: root.into(), source_anchor: anchor.into(), target_anchor: destination.clone(), source_bones: branch, weighted_bones: used });
    }
    Ok(groups)
}
pub fn selection(source: &[Bone], weighted: &[String], target: &[Bone], options: &PreparationOptions) -> Result<Option<PreservationPlan>> {
    let enabled = match options.values.get("preserve_physics_bones") {
        None => false,
        Some(v) => v.as_bool().ok_or_else(|| invalid("Keep existing physics bones must be on or off."))?,
    };
    if !enabled { return Ok(None); }
    let roots: Vec<String> = serde_json::from_value(options.values.get("physics_roots").cloned().unwrap_or_default()).map_err(|_| invalid("Choose the detected physics groups to keep."))?;
    if roots.is_empty() || roots.iter().collect::<BTreeSet<_>>().len() != roots.len() { return Err(invalid("Choose the detected physics groups to keep.")); }
    let groups = candidates(source, weighted, &roles(options,"source_roles")?, &roles(options,"target_roles")?)?;
    if roots.iter().any(|r| !groups.iter().any(|g| &g.root == r)) { return Err(invalid("Saved physics groups are stale or unsupported. Inspect the original source again.")); }
    let groups: Vec<_> = groups.into_iter().filter(|g| roots.contains(&g.root)).collect();
    let mut exported = BTreeMap::new();
    for group in &groups {
        if !target.iter().any(|b| b.name == group.target_anchor) { return Err(invalid("Choose a valid target attachment for source physics bones.")); }
        for name in &group.source_bones {
            let output = format!("{PREFIX}{name}");
            if output.len() > 63 || output.chars().any(char::is_control) || target.iter().any(|b| b.name == output) { return Err(invalid(format!("A preserved physics bone has a conflicting or unsupported name: {name}"))); }
            if exported.insert(name.clone(),output).is_some() || exported.len() > 512 { return Err(invalid("Selected physics branches overlap or exceed 512 bones.")); }
        }
    }
    Ok(Some(PreservationPlan { policy: POLICY.into(), groups, exported_bones: exported, new_physics: false, in_game_motion_tested: false }))
}
/// Previous direct/following choices stay saved for an off/on toggle.
pub fn body_options(options: &PreparationOptions, plan: Option<&PreservationPlan>) -> PreparationOptions {
    let mut result = options.clone();
    if let Some(plan) = plan {
        for key in ["mapping", "mapping_anchors"] {
            if let Some(values) = result.values.get_mut(key).and_then(|v| v.as_object_mut()) {
                for name in plan.exported_bones.keys() { values.remove(name); }
            }
        }
    }
    result
}
pub fn export_skeleton(original: &Skeleton, original_target: &[PreparedBone], output: &[PreparedBone], source: &[Bone], plan: Option<&PreservationPlan>) -> Result<Skeleton> {
    let count = original_target.len();
    let added = plan.map_or(0, |p| p.exported_bones.len());
    if output.len() != count + added || serde_json::to_value(&output[..count])? != serde_json::to_value(original_target)? { return Err(invalid("Preparation changed original hero bones or returned unexpected physics bones.")); }
    let mut skeleton = original.clone();
    if let Some(plan) = plan {
        let byname: BTreeMap<_,_> = source.iter().map(|b| (&b.name,b)).collect();
        let expected = plan.groups.iter().flat_map(|g| g.source_bones.iter().map(move |n| (g,n)));
        for (bone,(group,name)) in output[count..].iter().zip(expected) {
            let parent = if name == &group.root { &group.target_anchor } else {
                let parent = byname.get(name).and_then(|b| b.parent.as_ref()).ok_or_else(|| invalid("Missing preserved source parent."))?;
                plan.exported_bones.get(parent).ok_or_else(|| invalid("Physics bone parent escaped its reviewed branch."))?
            };
            if bone.name != plan.exported_bones[name] || bone.parent.as_ref() != Some(parent) { return Err(invalid("Preserved physics hierarchy differs from the reviewed source branches.")); }
            skeleton.parents.push(skeleton.names.iter().position(|n| n == parent).ok_or_else(|| invalid("Missing prepared physics bone attachment."))? as i32);
            skeleton.names.push(bone.name.clone());
            skeleton.positions.push(bone.local_position);
            skeleton.rotations.push(bone.local_rotation);
            skeleton.scales.push(bone.local_scale);
            skeleton.flags.push(0x40);
        }
        extensions::extension_identity(&skeleton,original).map_err(|e| invalid(e.to_string()))?;
    }
    let (canonical,_) = transforms::prepare_target(&skeleton).map_err(|e| invalid(e.to_string()))?;
    for (a,b) in canonical.iter().zip(output) {
        if a.matrix.iter().flatten().zip(b.matrix.iter().flatten()).chain(a.inverse_bind_matrix.iter().flatten().zip(b.inverse_bind_matrix.iter().flatten())).any(|(x,y)| !y.is_finite() || (x-y).abs() > 1e-8 * x.abs().max(1.)) { return Err(invalid("Preserved bind matrices do not match their authored local transforms.")); }
    }
    Ok(skeleton)
}
