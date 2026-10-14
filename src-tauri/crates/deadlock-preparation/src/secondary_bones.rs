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
/// Rigid head and body accessories. Hair and ear names do not make these move
/// (hairpin, hair ornament, 髪飾り, ear ornament); other motion names such as
/// ribbon or tassel still win. Keep in sync with secondary_bones.py and
/// src/preparation/secondary-bones.ts.
pub const RIGID_HINTS: [&str; 38] = ["halo","hat","cap","pin","pins","clip","clips","crown","tiara","circlet","horn","horns","helmet","helm","glasses","goggles","goggle","spectacles","visor","mask","earring","earrings","piercing","hairpin","hairclip","barrette","hairband","headband","ornament","accessory","acc",
    "飾","冠","帽","角","眼鏡","メガネ","輪"];
/// Breast bones move only when breast motion is turned on.
pub const BREAST_HINTS: [&str; 7] = ["breast","bust","boob","pectoral","oppai","おっぱい","乳"];
/// Butt bones move only when butt motion is turned on (off by default). ASCII
/// words match whole name parts (Butt_L, glute_r; not Button); 尻 also counts
/// on its own, but never inside 尻尾 (tail). Keep in sync with
/// secondary_bones.py and src/preparation/secondary-bones.ts.
pub const BUTT_HINTS: [&str; 16] = ["butt","buttock","buttocks","glute","glutes","gluteus","booty","bum","ass","pigu","tunbu",
    "臀","屁股","お尻","おしり","ケツ"];
/// Charms and other small hanging pieces, matched without separators (Key_Chain
/// is keychain). They move anywhere and override the rigid accessory words (a
/// dangle earring moves). Keep in sync with secondary_bones.py and
/// src/preparation/secondary-bones.ts.
pub const CHARM_HINTS: [&str; 18] = ["charm","keychain","keyring","pendant","lanyard","dangle",
    "チャーム","キーホルダー","ペンダント","根付","挂件","掛件","吊坠","吊墜","挂坠","掛墜","吊饰","吊飾"];
/// Straps count as charms only when they hang from a gun bone.
pub const GUN_STRAP_HINTS: [&str; 2] = ["strap","ストラップ"];
/// Moving gun parts (slide, magazine, trigger ...) are never charms.
pub const GUN_PART_HINTS: [&str; 49] = ["slide","mag","magazine","clip","bullet","bullets","shell","shells","ammo","round","rounds","cartridge",
    "casing","trigger","hammer","bolt","barrel","muzzle","cylinder","chamber","breech","sight","sights","scope","stock","grip","handle","safety",
    "lever","pump","drum","receiver","rail","laser","silencer","suppressor","bayonet","blade","spring","guard","hopper",
    "弾","弹","マガジン","スライド","トリガー","引き金","銃口","枪口"];
fn tokens(name:&str)->Vec<String> {
    // Split on separators and camel case: HatBrim_L -> hat, brim, l.
    let mut out=vec![];let mut word=String::new();let mut previous_lower=false;
    for c in name.chars() {
        if !c.is_alphanumeric() {if !word.is_empty(){out.push(std::mem::take(&mut word).to_lowercase());}previous_lower=false;continue;}
        if c.is_uppercase()&&previous_lower&&!word.is_empty(){out.push(std::mem::take(&mut word).to_lowercase());}
        previous_lower=c.is_lowercase()||c.is_ascii_digit();word.push(c);
    }
    if !word.is_empty(){out.push(word.to_lowercase());}
    out.into_iter().map(|t|t.trim_end_matches(|c:char|c.is_ascii_digit()).to_owned()).filter(|t|!t.is_empty()).collect()
}
pub fn rigid_named(name:&str)->bool {
    let words=tokens(name);let n=name.to_lowercase();
    let rigid=RIGID_HINTS.iter().any(|h|if h.is_ascii(){words.iter().any(|w|w==h)||(h.len()>=6&&n.contains(h))}else{n.contains(h)});
    // Moving names other than hair and ears keep their motion (a ribbon on a hat).
    let moving=HINTS.iter().filter(|h|!["hair","髪","发","髮","耳"].contains(h)).any(|h|n.contains(h))||charm_named(name);
    rigid&&!moving
}
pub fn charm_named(name:&str)->bool {
    let n:String=name.to_lowercase().chars().filter(|c|c.is_alphanumeric()).collect();
    CHARM_HINTS.iter().any(|h|n.contains(h))
}
fn gun_part_named(name:&str)->bool {
    let words=tokens(name);let n=name.to_lowercase();
    GUN_PART_HINTS.iter().any(|h|if h.is_ascii(){words.iter().any(|w|w==h)}else{n.contains(h)})
}
/// Bones of a gun candidate carry the candidate ids under `weapon` (see
/// `weapon::mark_gun_bones`).
fn gun_marks(bone:&Bone)->Vec<&str> {
    bone.details.get("weapon").and_then(|v|v.as_array()).map(|a|a.iter().filter_map(|v|v.as_str()).collect()).unwrap_or_default()
}
/// Charms on a gun: a chain hanging from a bone of a gun candidate. Two to four
/// bones in one line whose tip hangs below their top by at least 40% of their
/// length, without gun-part names (slide, magazine, trigger); a strap hanging from
/// the gun qualifies in any shape.
fn gun_charm(bone:&Bone,names:&BTreeMap<&str,&Bone>,children:&BTreeMap<&str,Vec<&str>>,source_roles:&BTreeMap<String,String>)->bool {
    let roles:BTreeSet<&str>=source_roles.values().map(String::as_str).collect();
    let Some(parent)=bone.parent.as_deref().and_then(|p|names.get(p)) else {return false;};
    if roles.contains(bone.name.as_str())||roles.contains(parent.name.as_str())||gun_marks(parent).is_empty() {return false;}
    let lower=bone.name.to_lowercase();
    if GUN_STRAP_HINTS.iter().any(|h|lower.contains(h)) {return true;}
    let distance=|a:&[f64;3],b:&[f64;3]|(0..3).map(|k|(a[k]-b[k]).powi(2)).sum::<f64>().sqrt();
    let mut chain=vec![bone];let mut cursor=bone.name.as_str();
    while let Some(next)=children.get(cursor) {
        if next.len()!=1||chain.len()>4 {return false;}
        cursor=next[0];let Some(child)=names.get(cursor) else {return false;};chain.push(child);
    }
    let path:f64=chain.windows(2).map(|w|distance(&w[0].position,&w[1].position)).sum();
    (2..=4).contains(&chain.len())&&path>1e-6&&!chain.iter().any(|b|roles.contains(b.name.as_str())||gun_part_named(&b.name))
        &&chain[0].position[2]-chain[chain.len()-1].position[2]>=0.4*path
}
/// Whether the moving part at `root` hangs from the gun candidate `gun`: a bone
/// between the root and its body anchor belongs to that gun.
pub fn on_gun(root:&str,source:&[Bone],source_roles:&BTreeMap<String,String>,gun:&str)->bool {
    let names:BTreeMap<&str,&Bone>=source.iter().map(|b|(b.name.as_str(),b)).collect();
    let mut cursor=names.get(root).and_then(|b|b.parent.as_deref()).and_then(|p|names.get(p));let mut steps=0;
    while let Some(bone)=cursor {
        if source_roles.values().any(|r|r==&bone.name)||steps>=512 {return false;}
        if gun_marks(bone).contains(&gun) {return true;}
        cursor=bone.parent.as_deref().and_then(|p|names.get(p));steps+=1;
    }
    false
}
pub fn named_breast(name:&str)->bool {
    let n=name.to_lowercase();
    !named_butt(name) && (BREAST_HINTS.iter().any(|h|n.contains(h)) || (n.contains('胸')&&(n.contains('左')||n.contains('右')||tokens(name).iter().any(|t|["l","r","left","right"].contains(&t.as_str())))))
}
pub fn named_butt(name:&str)->bool {
    let words=tokens(name);let n=name.to_lowercase();
    BUTT_HINTS.iter().any(|h|if h.is_ascii(){words.iter().any(|w|w==h)}else{n.contains(h)}) || (n.contains('尻')&&!n.contains("尻尾"))
}
fn named_secondary(name:&str)->bool {
    let n=name.to_lowercase();HINTS.iter().any(|s|n.contains(s)) || n.split(|c:char|!c.is_alphanumeric()).any(|s|["ear","ears"].contains(&s)) || charm_named(name)
}
/// A closed loop (halo, crown band, hat brim): the single chain that contains the
/// bone, from below its body anchor, ends close to where it starts.
fn ring_chain(bone:&Bone,names:&BTreeMap<&str,&Bone>,children:&BTreeMap<&str,Vec<&str>>,roles:&BTreeMap<String,String>)->bool {
    let distance=|a:&[f64;3],b:&[f64;3]|(0..3).map(|k|(a[k]-b[k]).powi(2)).sum::<f64>().sqrt();
    let mut bone=bone;let mut steps=0;
    while let Some(parent)=bone.parent.as_deref().and_then(|p|names.get(p)) {
        if roles.values().any(|r|r==&parent.name)||steps>=512 {break;}
        bone=parent;steps+=1;
    }
    let mut chain=vec![bone];let mut cursor=bone.name.as_str();
    while let Some(next)=children.get(cursor) {
        if next.len()!=1||chain.len()>=512 {return false;}
        cursor=next[0];let Some(child)=names.get(cursor) else {return false;};chain.push(child);
    }
    let path:f64=chain.windows(2).map(|w|distance(&w[0].position,&w[1].position)).sum();
    chain.len()>=4 && path>1e-6 && distance(&chain[0].position,&chain[chain.len()-1].position)<0.3*path
}
/// Facing direction in the ground plane, from the feet to the toes (default -Y).
/// Only each foot's lowest child counts: the toes are on the ground, while boot
/// straps, heels and pendants hanging from the foot can point anywhere.
fn facing(names:&BTreeMap<&str,&Bone>,children:&BTreeMap<&str,Vec<&str>>,roles:&BTreeMap<String,String>)->[f64;2] {
    let mut f=[0.,0.];
    for side in ["foot_L","foot_R"] {
        let Some(foot)=roles.get(side).and_then(|n|names.get(n.as_str())) else {continue};
        let toe=children.get(foot.name.as_str()).into_iter().flatten().filter_map(|c|names.get(c))
            .fold(None::<&&Bone>,|low,c|match low {Some(l) if l.position[2]<=c.position[2]=>Some(l),_=>Some(c)});
        if let Some(child)=toe {
            f[0]+=child.position[0]-foot.position[0];f[1]+=child.position[1]-foot.position[1];
        }
    }
    let n=(f[0]*f[0]+f[1]*f[1]).sqrt();
    if n<1e-6 {[0.,-1.]} else {[f[0]/n,f[1]/n]}
}
/// Torso bones between the pelvis and the neck (or head), where breast bones hang.
fn torso_chain<'a>(names:&BTreeMap<&str,&'a Bone>,roles:&BTreeMap<String,String>)->Vec<&'a str> {
    let mut chain=vec![];
    let mut cursor=roles.get("neck").or_else(||roles.get("head")).and_then(|n|names.get(n.as_str())).and_then(|b|b.parent.as_deref()).and_then(|p|names.get(p));
    while let Some(bone)=cursor {
        if chain.len()>=64 {break;}
        chain.push(bone.name.as_str());
        if roles.get("pelvis").is_some_and(|p|p==&bone.name) {break;}
        cursor=bone.parent.as_deref().and_then(|p|names.get(p));
    }
    chain
}
/// Short, weighted, non-role children of torso bones: where breast and butt
/// bones hang. Returns the options, the torso length and the pelvis and
/// shoulder (neck or head) heights.
fn body_pair_options<'a>(source:&'a [Bone],names:&BTreeMap<&str,&'a Bone>,children:&BTreeMap<&str,Vec<&str>>,weighted:&BTreeSet<&str>,roles:&BTreeMap<String,String>)->Option<(Vec<&'a Bone>,f64,f64,f64)> {
    let role_names:BTreeSet<&str>=roles.values().map(String::as_str).collect();
    let (pelvis,head)=(roles.get("pelvis").and_then(|n|names.get(n.as_str()))?,roles.get("head").and_then(|n|names.get(n.as_str()))?);
    let distance=|a:&[f64;3],b:&[f64;3]|(0..3).map(|k|(a[k]-b[k]).powi(2)).sum::<f64>().sqrt();
    let torso=distance(&pelvis.position,&head.position);
    if torso<=1e-6 {return None;}
    let parents=torso_chain(names,roles);
    let high=roles.get("neck").and_then(|n|names.get(n.as_str())).unwrap_or(head).position[2];
    let short=|bone:&Bone|{
        let mut branch=vec![bone.name.as_str()];let mut at=0;
        while at<branch.len() {if branch.len()>3 {return false;}branch.extend(children.get(branch[at]).into_iter().flatten().copied());at+=1;}
        branch.len()<=3 && branch.iter().all(|n|!role_names.contains(n)&&names.get(n).is_some_and(|b|distance(&b.position,&bone.position)<=0.17*torso))
    };
    let options=source.iter().filter(|b|b.parent.as_deref().is_some_and(|p|parents.contains(&p))&&!role_names.contains(b.name.as_str())
        &&weighted.contains(b.name.as_str())&&short(b)).collect();
    Some((options,torso,pelvis.position[2],high))
}
/// Mirrored pairs among `options`: same parent, same height and depth,
/// opposite sides. `keep(forward offset, height)` filters by placement.
fn mirrored_pairs<'a>(options:&[&'a Bone],names:&BTreeMap<&str,&'a Bone>,children:&BTreeMap<&str,Vec<&str>>,roles:&BTreeMap<String,String>,torso:f64,keep:impl Fn(f64,f64)->bool)->BTreeSet<&'a str> {
    let forward=facing(names,children,roles);let lateral=[-forward[1],forward[0]];
    let place=|b:&Bone|{let p=names[b.parent.as_deref().unwrap()];let d=[b.position[0]-p.position[0],b.position[1]-p.position[1]];
        (d[0]*forward[0]+d[1]*forward[1],d[0]*lateral[0]+d[1]*lateral[1],b.position[2])};
    let mut output=BTreeSet::new();
    for (i,a) in options.iter().enumerate() {
        let partners=options.iter().enumerate().filter(|(j,b)|*j!=i&&b.parent==a.parent&&{
            let ((fa,la,za),(fb,lb,zb))=(place(a),place(b));
            keep(fa,za)&&keep(fb,zb)&&(fa-fb).abs()<=0.03*torso&&(za-zb).abs()<=0.03*torso&&la*lb<0.
                &&(la+lb).abs()<=0.04*torso&&(la-lb).abs()>=0.06*torso&&(la-lb).abs()<=0.5*torso
        }).count();
        if partners==1 && !rigid_named(&a.name) {output.insert(a.name.as_str());}
    }
    output
}
/// Butt bones: short weighted children of a torso bone named as a butt, or a
/// mirrored pair low on the hips that is not in front of the body. They move
/// only with butt motion on, with their own gentle bounce.
fn butt_roots<'a>(source:&'a [Bone],names:&BTreeMap<&str,&'a Bone>,children:&BTreeMap<&str,Vec<&str>>,weighted:&BTreeSet<&str>,roles:&BTreeMap<String,String>)->BTreeSet<&'a str> {
    let Some((options,torso,pelvis,high))=body_pair_options(source,names,children,weighted,roles) else {return BTreeSet::new();};
    let breasts=breast_roots(source,names,children,weighted,roles);
    let mut output:BTreeSet<&str>=options.iter().filter(|b|named_butt(&b.name)).map(|b|b.name.as_str()).collect();
    let hips=pelvis+0.25*(high-pelvis);
    output.extend(mirrored_pairs(&options,names,children,roles,torso,|forward,z|forward<=0.02*torso&&z<=hips&&z>=pelvis-0.25*torso));
    output.retain(|n|!breasts.contains(n)&&!named_breast(n));
    output
}
/// Breast bones: weighted, short, non-role children of a torso bone that
/// are named as breasts, or form a mirrored pair in front of the chest in the
/// upper torso (never on the pelvis, never a butt).
fn breast_roots<'a>(source:&'a [Bone],names:&BTreeMap<&str,&'a Bone>,children:&BTreeMap<&str,Vec<&str>>,weighted:&BTreeSet<&str>,roles:&BTreeMap<String,String>)->BTreeSet<&'a str> {
    let Some((options,torso,pelvis,high))=body_pair_options(source,names,children,weighted,roles) else {return BTreeSet::new();};
    let low=roles.get("spine").and_then(|n|names.get(n.as_str())).map_or(pelvis,|b|b.position[2]);
    let floor=low.max(pelvis+0.35*(high-pelvis));
    let mut output:BTreeSet<&str>=options.iter().filter(|b|named_breast(&b.name)).map(|b|b.name.as_str()).collect();
    let above:Vec<&Bone>=options.iter().copied().filter(|b|roles.get("pelvis").is_none_or(|p|b.parent.as_deref()!=Some(p.as_str()))&&!named_butt(&b.name)).collect();
    output.extend(mirrored_pairs(&above,names,children,roles,torso,|forward,z|forward>=0.02*torso&&z>floor&&z<high));
    output
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
/// The optional breast motion choice of model preparation.
pub fn breast_motion(options: &PreparationOptions) -> Result<Option<bool>> {
    options.values.get("breast_motion").map(|v| v.as_bool().ok_or_else(|| invalid("Breast motion must be on or off."))).transpose()
}
/// The butt motion choice of model preparation. New models get `false` (off by
/// default, set at the first inspection); unset keeps projects from before butt
/// motion existed unchanged (their saved groups stay valid).
pub fn butt_motion(options: &PreparationOptions) -> Result<Option<bool>> {
    options.values.get("butt_motion").map(|v| v.as_bool().ok_or_else(|| invalid("Butt motion must be on or off."))).transpose()
}
/// Detected butt bones (named, or a mirrored pair low on the hips).
pub fn butt_bones(source: &[Bone], weighted: &[String], source_roles: &BTreeMap<String,String>) -> Vec<String> {
    let names: BTreeMap<_,_> = source.iter().map(|b| (b.name.as_str(),b)).collect();
    let mut children=BTreeMap::<&str,Vec<&str>>::new();
    for bone in source{if let Some(parent)=bone.parent.as_deref(){children.entry(parent).or_default().push(&bone.name);}}
    let weighted:BTreeSet<&str>=weighted.iter().map(String::as_str).collect();
    butt_roots(source,&names,&children,&weighted,source_roles).into_iter().map(str::to_owned).collect()
}
/// Detected breast bones (named or a mirrored pair in front of the chest).
/// What Blender needs to add breast bones where the chest has breasts, for a
/// rig without them (see resources/blender/breast_bones.py).
pub fn breast_request(source: &[Bone], source_roles: &BTreeMap<String,String>) -> Option<serde_json::Value> {
    let names: BTreeMap<_,_> = source.iter().map(|b| (b.name.as_str(),b)).collect();
    let mut children=BTreeMap::<&str,Vec<&str>>::new();
    for bone in source{if let Some(parent)=bone.parent.as_deref(){children.entry(parent).or_default().push(&bone.name);}}
    let chest=source_roles.get("chest").or_else(||source_roles.get("spine"))?;
    let (pelvis,head)=(source_roles.get("pelvis")?,source_roles.get("head")?);
    let high=source_roles.get("neck").unwrap_or(head);
    let mut chain=vec![high.clone()];
    chain.extend(torso_chain(&names,source_roles).into_iter().map(str::to_owned));
    if chain.last()!=Some(pelvis) || !chain.contains(chest) {return None;}
    Some(serde_json::json!({"chest":chest,"pelvis":pelvis,"head":head,"high":high,"torso":chain,"forward":facing(&names,&children,source_roles)}))
}
/// Breast-named bones that carry skin weight themselves or below them, even when
/// other parts hang from them. A rig with these is never given added breast bones.
pub fn breast_named_bones(source: &[Bone], weighted: &[String]) -> Vec<String> {
    let weighted: BTreeSet<&str> = weighted.iter().map(String::as_str).collect();
    let mut children = BTreeMap::<&str, Vec<&str>>::new();
    for bone in source { if let Some(parent) = bone.parent.as_deref() { children.entry(parent).or_default().push(&bone.name); } }
    source.iter().filter(|b| named_breast(&b.name) && {
        let mut pending = vec![b.name.as_str()];
        let mut seen = BTreeSet::new();
        let mut found = false;
        while let Some(n) = pending.pop() {
            if !seen.insert(n) || seen.len() > 512 { break; }
            if weighted.contains(n) { found = true; break; }
            pending.extend(children.get(n).into_iter().flatten().copied());
        }
        found
    }).map(|b| b.name.clone()).collect()
}
pub fn breast_bones(source: &[Bone], weighted: &[String], source_roles: &BTreeMap<String,String>) -> Vec<String> {
    let names: BTreeMap<_,_> = source.iter().map(|b| (b.name.as_str(),b)).collect();
    let mut children=BTreeMap::<&str,Vec<&str>>::new();
    for bone in source{if let Some(parent)=bone.parent.as_deref(){children.entry(parent).or_default().push(&bone.name);}}
    let weighted:BTreeSet<&str>=weighted.iter().map(String::as_str).collect();
    breast_roots(source,&names,&children,&weighted,source_roles).into_iter().map(str::to_owned).collect()
}
fn roles(options: &PreparationOptions, key: &str) -> Result<BTreeMap<String,String>> {
    serde_json::from_value(options.values.get(key).cloned().unwrap_or_default())
        .map_err(|_| invalid("Review the source and target body roles before keeping physics bones."))
}
pub fn candidates(source: &[Bone], weighted: &[String], source_roles: &BTreeMap<String,String>, target_roles: &BTreeMap<String,String>) -> Result<Vec<PhysicsGroup>> {
    candidates_with(source, weighted, source_roles, target_roles, None)
}
/// `breasts`: Some(true) adds breast bones as moving parts, Some(false) leaves
/// them on the body even when the source gives them physics, None keeps the
/// earlier behavior (only source physics).
pub fn candidates_with(source: &[Bone], weighted: &[String], source_roles: &BTreeMap<String,String>, target_roles: &BTreeMap<String,String>, breasts: Option<bool>) -> Result<Vec<PhysicsGroup>> {
    candidates_motion(source, weighted, source_roles, target_roles, breasts, None)
}
/// `butts`: Some(true) adds butt bones as moving parts, Some(false) leaves them
/// on the hips even when the source gives them physics, None keeps the earlier
/// behavior (butt bones move only when the source gives them physics or names).
pub fn candidates_motion(source: &[Bone], weighted: &[String], source_roles: &BTreeMap<String,String>, target_roles: &BTreeMap<String,String>, breasts: Option<bool>, butts: Option<bool>) -> Result<Vec<PhysicsGroup>> {
    let names: BTreeMap<_,_> = source.iter().map(|b| (b.name.as_str(),b)).collect();
    if names.len() != source.len() { return Err(invalid("Duplicate source bone identities.")); }
    let mut children=BTreeMap::<&str,Vec<&str>>::new();
    for bone in source{if let Some(parent)=bone.parent.as_deref(){children.entry(parent).or_default().push(&bone.name);}}
    let mut physical: BTreeSet<_> = source.iter().filter(|b| b.details.get("physics").is_some_and(|p| p["policy"] == BINDING_POLICY && p["dynamic_bodies"].as_array().is_some_and(|v| !v.is_empty() && v.iter().all(|n| n.as_str().is_some_and(|n| !n.is_empty()))))).map(|b| b.name.as_str()).collect();
    let weighted_set:BTreeSet<&str>=weighted.iter().map(String::as_str).collect();
    let breast_set=breast_roots(source,&names,&children,&weighted_set,source_roles);
    let butt_set=butt_roots(source,&names,&children,&weighted_set,source_roles);
    if breasts==Some(false) {physical.retain(|n|!breast_set.contains(n));}
    if butts==Some(false) {physical.retain(|n|!butt_set.contains(n));}
    let verified=physical.clone();
    // FBX/GLB rigs often have usable secondary chains without rigid bodies.
    // Require a semantic hint (or a long hanging chain) PLUS multiple weighted, spatially separated
    // joints. Existing physical rigs always retain their original boundaries.
    let automatic=source.iter().filter(|b|(named_secondary(&b.name)||hanging_chain(b,&names,&children,&weighted_set,source_roles)||gun_charm(b,&names,&children,source_roles))
        && !rigid_named(&b.name) && !ring_chain(b,&names,&children,source_roles) && !breast_set.contains(b.name.as_str()) && !(butts==Some(false) && butt_set.contains(b.name.as_str())));
    for bone in automatic {
        let mut branch=BTreeSet::new();let mut pending=vec![bone.name.as_str()];
        while let Some(n)=pending.pop(){if !branch.insert(n)||branch.len()>512{return Err(invalid("Secondary chain is cyclic or exceeds 512 bones."));}pending.extend(children.get(n).into_iter().flatten().copied());}
        if branch.iter().any(|n|verified.contains(n)||source_roles.values().any(|r|r==n)
            || names[n].details.get("physics").is_some_and(|p|["dynamic_bodies","kinematic_bodies"].iter().any(|k|p[k].as_array().is_some_and(|a|!a.is_empty())))){continue;}
        let used=weighted.iter().filter(|n|branch.contains(n.as_str())).map(|n|names[n.as_str()]).collect::<Vec<_>>();
        if used.len()>=2 && used.iter().skip(1).any(|b|(0..3).map(|k|(b.position[k]-used[0].position[k]).powi(2)).sum::<f64>()>1e-10){physical.insert(bone.name.as_str());}
    }
    // A breast (or butt) is usually one weighted bone; it moves as a whole.
    let unbound=|root:&str|!names[root].details.get("physics").is_some_and(|p|["dynamic_bodies","kinematic_bodies"].iter().any(|k|p[k].as_array().is_some_and(|a|!a.is_empty())));
    if breasts==Some(true) {
        for root in &breast_set { if unbound(root) {physical.insert(root);} }
    }
    if butts==Some(true) {
        for root in &butt_set { if unbound(root) {physical.insert(root);} }
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
    selection_for(source, weighted, target, options, None)
}
/// `gun`: the fitted or removed gun of this conversion. Moving parts hanging from
/// a replaced gun hang from its main weapon bone (`Plan::anchor`); those on a gun
/// that is left out are left out with it. Keep in sync with secondary_bones.py.
pub fn selection_for(source: &[Bone], weighted: &[String], target: &[Bone], options: &PreparationOptions, gun: Option<&crate::weapon::Plan>) -> Result<Option<PreservationPlan>> {
    let enabled = match options.values.get("preserve_physics_bones") {
        None => false,
        Some(v) => v.as_bool().ok_or_else(|| invalid("Keep existing physics bones must be on or off."))?,
    };
    if !enabled { return Ok(None); }
    let roots: Vec<String> = serde_json::from_value(options.values.get("physics_roots").cloned().unwrap_or_default()).map_err(|_| invalid("Choose the detected physics groups to keep."))?;
    if roots.is_empty() || roots.iter().collect::<BTreeSet<_>>().len() != roots.len() { return Err(invalid("Choose the detected physics groups to keep.")); }
    let source_roles = roles(options,"source_roles")?;
    // Guns left out (a left-out hand or an unused gun-file gun) take their moving parts with them.
    let removed = gun.map(crate::weapon::Plan::removed_ids).unwrap_or_default();
    let roots: Vec<String> = roots.into_iter().filter(|r| !removed.iter().any(|id| on_gun(r, source, &source_roles, id))).collect();
    // Butt groups saved with butt motion on stay on the hips once it is off.
    let butts = butt_motion(options)?;
    let butt_set = if butts == Some(false) { butt_bones(source, weighted, &source_roles) } else { vec![] };
    let roots: Vec<String> = roots.into_iter().filter(|r| !butt_set.contains(r)).collect();
    if roots.is_empty() { return Ok(None); }
    let groups = candidates_motion(source, weighted, &source_roles, &roles(options,"target_roles")?, breast_motion(options)?, butts)?;
    if roots.iter().any(|r| !groups.iter().any(|g| &g.root == r)) { return Err(invalid("Saved physics groups are stale or unsupported. Inspect the original source again.")); }
    let mut groups: Vec<_> = groups.into_iter().filter(|g| roots.contains(&g.root)).collect();
    // Each fitted gun carries its own charms (a copied gun's charms stay on the original gun).
    for hand in gun.map(crate::weapon::Plan::hands).unwrap_or_default().into_iter().filter(|h| h.action == "replace" && !h.copy) {
        let id = hand.objects.join("+");
        for group in groups.iter_mut().filter(|g| on_gun(&g.root, source, &source_roles, &id)) {
            if hand.anchor.is_empty() { return Err(invalid("The fitted gun has no weapon bone for its charms. Inspect the source again.")); }
            group.target_anchor = hand.anchor.clone();
        }
    }
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
