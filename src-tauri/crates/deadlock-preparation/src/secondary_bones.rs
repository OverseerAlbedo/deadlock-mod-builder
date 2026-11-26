//! Existing physics bindings authorize reviewed ordinary bone branches only.
//! No new runtime flags, simulation or motion are generated here.
use crate::{algorithms::Bone, invalid, Result};
use deadlock_project::PreparationOptions;
use deadlock_skeleton::{extensions, transforms::{self, PreparedBone}, Skeleton};
use serde::{Deserialize, Serialize};
use sha2::Digest;
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
/// Breast names in pinyin, spelled by whole name parts in a row (XiongBu.L,
/// xiong_bu_l, Xiongbu_R; never inside another word). 乳房 (rufang) is a breast
/// on its own; 胸 and 胸部 (xiong, xiongbu) also name the chest, so like 胸
/// they count only with a side, which must follow them directly (xiong_L_0,
/// XiongBu_R; not XiongMao_L) or come directly before them when they end the
/// name (L_XiongBu, R_Xiong_Bu_01; not L_XiongMao, L_Xiong_Spine). Keep in
/// sync with secondary_bones.py and src/preparation/secondary-bones.ts.
pub const PINYIN_BREAST_HINTS: [&str; 1] = ["rufang"];
pub const PINYIN_SIDED_BREAST_HINTS: [&str; 2] = ["xiong","xiongbu"];
const SIDE_WORDS: [&str; 4] = ["l","r","left","right"];
/// A breast at a branch: a named breast bone may carry more bones than the
/// three of `body_pair_options` (a rig whose breast has a base bone and a ring
/// of surface bones), up to this many, all within `NAMED_BREAST_REACH` torso
/// lengths of it and none named as another part (a ribbon or cape on the
/// breast makes it a carrier, not a breast).
pub const NAMED_BREAST_BONES: usize = 24;
pub const NAMED_BREAST_REACH: f64 = 0.4;
/// Butt bones move only when butt motion is turned on (off by default). ASCII
/// words match whole name parts (Butt_L, glute_r; not Button); 尻 also counts
/// on its own, but never inside 尻尾 (tail). Keep in sync with
/// secondary_bones.py and src/preparation/secondary-bones.ts.
pub const BUTT_HINTS: [&str; 16] = ["butt","buttock","buttocks","glute","glutes","gluteus","booty","bum","ass","pigu","tunbu",
    "臀","屁股","お尻","おしり","ケツ"];
/// The pinyin butt words also match as whole name parts in a row, so joined
/// syllables count (PiGu_L, Tun_Bu_R; not PiGuan). Only these two: a run of
/// the English words would find "bum" in XiongBu_M. Keep in sync with
/// secondary_bones.py and src/preparation/secondary-bones.ts.
pub const PINYIN_BUTT_HINTS: [&str; 2] = ["pigu","tunbu"];
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
    !named_butt(name) && (BREAST_HINTS.iter().any(|h|n.contains(h)) || (n.contains('胸')&&(n.contains('左')||n.contains('右')||tokens(name).iter().any(|t|SIDE_WORDS.contains(&t.as_str()))))
        || pinyin_breast(name))
}
/// `PINYIN_BREAST_HINTS` anywhere as whole name parts in a row, or
/// `PINYIN_SIDED_BREAST_HINTS` followed directly by a side (or 左/右 in the name
/// with the hint at its end), or ending the name directly after a side.
fn pinyin_breast(name:&str)->bool {
    let words=tokens(name);let sided=name.contains('左')||name.contains('右');
    (0..words.len()).any(|i|(i+1..=words.len()).any(|j|{
        let run=words[i..j].concat();
        PINYIN_BREAST_HINTS.contains(&run.as_str()) || (PINYIN_SIDED_BREAST_HINTS.contains(&run.as_str())
            && (words.get(j).map_or(sided,|next|SIDE_WORDS.contains(&next.as_str())||next.starts_with('左')||next.starts_with('右'))
                || (j==words.len()&&i>0&&SIDE_WORDS.contains(&words[i-1].as_str()))))
    }))
}
pub fn named_butt(name:&str)->bool {
    let words=tokens(name);let n=name.to_lowercase();
    BUTT_HINTS.iter().any(|h|if h.is_ascii(){words.iter().any(|w|w==h)}else{n.contains(h)}) || (n.contains('尻')&&!n.contains("尻尾"))
        || (0..words.len()).any(|i|(i+1..=words.len()).any(|j|PINYIN_BUTT_HINTS.contains(&words[i..j].concat().as_str())))
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
/// A breast word anywhere in the name, side or not: `BREAST_HINTS`, 胸, or a
/// pinyin breast word as whole name parts in a row (Xiongbu_Mid.L, 胸先.L).
fn breast_word(name:&str)->bool {
    let n=name.to_lowercase();let words=tokens(name);
    BREAST_HINTS.iter().any(|h|n.contains(h)) || n.contains('胸')
        || (0..words.len()).any(|i|(i+1..=words.len()).any(|j|{let run=words[i..j].concat();PINYIN_BREAST_HINTS.contains(&run.as_str())||PINYIN_SIDED_BREAST_HINTS.contains(&run.as_str())}))
}
/// Named breast bones that carry a larger branch (a base bone and a ring of
/// surface bones): weighted, non-role children of a torso bone named as
/// breasts whose whole branch has at most `NAMED_BREAST_BONES` bones, all
/// within `NAMED_BREAST_REACH` torso lengths, none a role, and every bone
/// below the root named with a breast word (`breast_word`) and not as another
/// part. A breast bone that carries anything else (a cape or a streamer, also
/// one named in a way no hint knows) stays as it was. Unnamed pairs keep the
/// three-bone limit.
fn branched_breasts<'a>(source:&'a [Bone],names:&BTreeMap<&str,&'a Bone>,children:&BTreeMap<&str,Vec<&str>>,weighted:&BTreeSet<&str>,roles:&BTreeMap<String,String>,torso:f64)->BTreeSet<&'a str> {
    let role_names:BTreeSet<&str>=roles.values().map(String::as_str).collect();
    let parents=torso_chain(names,roles);
    let distance=|a:&[f64;3],b:&[f64;3]|(0..3).map(|k|(a[k]-b[k]).powi(2)).sum::<f64>().sqrt();
    let compact=|bone:&Bone|{
        let mut branch=vec![bone.name.as_str()];let mut at=0;
        while at<branch.len() {if branch.len()>NAMED_BREAST_BONES {return false;}branch.extend(children.get(branch[at]).into_iter().flatten().copied());at+=1;}
        branch.len()<=NAMED_BREAST_BONES && branch.iter().all(|n|!role_names.contains(n)&&names.get(n).is_some_and(|b|distance(&b.position,&bone.position)<=NAMED_BREAST_REACH*torso))
            && branch[1..].iter().all(|n|breast_word(n)&&!named_secondary(n)&&!rigid_named(n)&&!named_butt(n))
    };
    source.iter().filter(|b|b.parent.as_deref().is_some_and(|p|parents.contains(&p))&&!role_names.contains(b.name.as_str())
        &&weighted.contains(b.name.as_str())&&named_breast(&b.name)&&compact(b)).map(|b|b.name.as_str()).collect()
}
/// Breast bones: weighted, short, non-role children of a torso bone that
/// are named as breasts (or carry a compact branch, `branched_breasts`), or form a mirrored pair in front of the chest in the
/// upper torso (never on the pelvis, never a butt).
fn breast_roots<'a>(source:&'a [Bone],names:&BTreeMap<&str,&'a Bone>,children:&BTreeMap<&str,Vec<&str>>,weighted:&BTreeSet<&str>,roles:&BTreeMap<String,String>)->BTreeSet<&'a str> {
    let Some((options,torso,pelvis,high))=body_pair_options(source,names,children,weighted,roles) else {return BTreeSet::new();};
    let low=roles.get("spine").and_then(|n|names.get(n.as_str())).map_or(pelvis,|b|b.position[2]);
    let floor=low.max(pelvis+0.35*(high-pelvis));
    let mut output:BTreeSet<&str>=options.iter().filter(|b|named_breast(&b.name)).map(|b|b.name.as_str()).collect();
    output.extend(branched_breasts(source,names,children,weighted,roles,torso));
    // An unnamed pair whose own name says it is another moving part (chest
    // ribbons over the breasts, hair, a cape) is never a breast.
    let above:Vec<&Bone>=options.iter().copied().filter(|b|roles.get("pelvis").is_none_or(|p|b.parent.as_deref()!=Some(p.as_str()))&&!named_butt(&b.name)&&!named_secondary(&b.name)).collect();
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
    let mut request=serde_json::json!({"chest":chest,"pelvis":pelvis,"head":head,"high":high,"torso":chain,"forward":facing(&names,&children,source_roles)});
    let carried=chest_carried_bones(source,&names,&children,source_roles);
    if !carried.is_empty() {request["carried"]=serde_json::json!(carried);}
    Some(request)
}
/// Bones of the rig's own that hang on the chest and carry its skin there,
/// without being recognised as breasts (a breast with a base and surface bones
/// named in another language): the branches of a left/right pair of non-role
/// children of a torso bone above the pelvis (same parent, mirrored sideways),
/// with no role bone in them (never a shoulder or an arm), none named as another
/// part (ribbons, capes, accessories, butts) and no rigid-body physics, each
/// branch as compact as a branched breast (`NAMED_BREAST_BONES`,
/// `NAMED_BREAST_REACH`; a sash or cape reaches farther). breast_bones.py counts their skin as chest surface and
/// can move it to the bones it adds. A single bone (a bow in the middle) never
/// counts. Sorted; empty for most rigs, and then the request is as before.
fn chest_carried_bones(source:&[Bone],names:&BTreeMap<&str,&Bone>,children:&BTreeMap<&str,Vec<&str>>,roles:&BTreeMap<String,String>)->Vec<String> {
    let role_names:BTreeSet<&str>=roles.values().map(String::as_str).collect();
    let (Some(pelvis),Some(head))=(roles.get("pelvis").and_then(|n|names.get(n.as_str())),roles.get("head").and_then(|n|names.get(n.as_str()))) else {return vec![];};
    let torso=(0..3).map(|k|(pelvis.position[k]-head.position[k]).powi(2)).sum::<f64>().sqrt();
    if torso<=1e-6 {return vec![];}
    let parents:Vec<&str>=torso_chain(names,roles).into_iter().filter(|n|*n!=pelvis.name).collect();
    let branch=|root:&str|->Vec<String>{
        let mut out=vec![root.to_owned()];let mut at=0;
        while at<out.len()&&out.len()<=512 {let next:Vec<String>=children.get(out[at].as_str()).into_iter().flatten().map(|c|c.to_string()).collect();out.extend(next);at+=1;}
        out
    };
    let bodies=|b:&Bone|b.details.get("physics").is_some_and(|p|["dynamic_bodies","kinematic_bodies"].iter().any(|k|p[k].as_array().is_some_and(|a|!a.is_empty())));
    let distance=|a:&[f64;3],b:&[f64;3]|(0..3).map(|k|(a[k]-b[k]).powi(2)).sum::<f64>().sqrt();
    let roots:Vec<&Bone>=source.iter().filter(|b|b.parent.as_deref().is_some_and(|p|parents.contains(&p))&&!role_names.contains(b.name.as_str())&&{
        let all=branch(&b.name);
        all.len()<=NAMED_BREAST_BONES && all.iter().all(|n|!role_names.contains(n.as_str())&&!named_secondary(n)&&!rigid_named(n)&&!named_butt(n)
            &&names.get(n.as_str()).is_some_and(|x|!bodies(x)&&distance(&x.position,&b.position)<=NAMED_BREAST_REACH*torso))
    }).collect();
    let paired=mirrored_pairs(&roots,names,children,roles,torso,|_,_|true);
    let mut out:Vec<String>=paired.into_iter().flat_map(|r|branch(r)).collect();
    out.sort();out.dedup();
    out
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
/// The review issue kind of `stale_groups_message` (the page offers its one-click fix).
pub const STALE_GROUPS_KIND: &str = "stale_physics_groups";
/// Saved moving-part roots that are no longer detected groups. The same text is in
/// secondary_bones.py (`tests/page_names.rs` checks it).
pub fn stale_groups_message() -> String {
    use crate::page_names::{LOAD_AND_CHECK, MOVING_PARTS_TAB, USE_DETECTED_GROUPS};
    format!("Some saved moving parts are no longer found in your model. On the {MOVING_PARTS_TAB} tab, press {USE_DETECTED_GROUPS} (or {LOAD_AND_CHECK} if you changed the model file).")
}
/// The most moving-part bones one preparation keeps, all kept groups together.
/// The same number is `MAX_KEPT_BONES` in secondary_bones.py and in
/// src/preparation/secondary-bones.ts (`tests/kept_bone_limit.rs` compares the
/// three). The per-chain walk guards (512 steps) are a separate bound.
pub const MAX_KEPT_BONES: usize = 1024;
/// How many of the largest kept groups `too_many_bones_message` names.
pub const LARGEST_NAMED: usize = 5;
/// One group in `too_many_bones_message`: its root bone and its bone count.
pub const KEPT_GROUP_SIZE: &str = "{root} ({count})";
/// Two kept groups that share bones. `{first}` and `{second}` are their root
/// bones. The same text is SHARED_KEPT_BONES in secondary_bones.py.
pub fn shared_bones_template() -> String {
    use crate::page_names::MOVING_PARTS_TAB;
    format!("The moving parts {{first}} and {{second}} share bones, so they cannot both be kept. On the {MOVING_PARTS_TAB} tab, keep only one of them.")
}
/// More kept bones than `MAX_KEPT_BONES`. The same text is TOO_MANY_KEPT_BONES
/// in secondary_bones.py.
pub fn too_many_bones_template() -> String {
    use crate::page_names::MOVING_PARTS_TAB;
    format!("The kept moving parts have {{total}} bones, more than the limit of {{limit}}. The largest, with their bone counts: {{largest}}. On the {MOVING_PARTS_TAB} tab, leave some of them out until the total is {{limit}} or less; a part that is left out follows the body and stays still.")
}
/// Fills `{key}` placeholders in one pass (as Python's str.format does), so a
/// bone name that holds braces is never filled again.
fn fill(template: &str, values: &[(&str, String)]) -> String {
    let mut out = String::new();
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('}').map(|end| (&after[..end], end)).and_then(|(key, end)| values.iter().find(|(k, _)| *k == key).map(|(_, v)| (v, end))) {
            Some((value, end)) => { out.push_str(value); rest = &after[end + 1..]; }
            None => { out.push('{'); rest = after; }
        }
    }
    out.push_str(rest);
    out
}
pub fn shared_bones_message(first: &str, second: &str) -> String {
    fill(&shared_bones_template(), &[("first", first.into()), ("second", second.into())])
}
/// `groups`: the kept groups; the largest `LARGEST_NAMED` are named, by bone
/// count and then by root name, so Rust and Python give the same line.
pub fn too_many_bones_message(total: usize, groups: &[PhysicsGroup]) -> String {
    let mut largest: Vec<&PhysicsGroup> = groups.iter().collect();
    largest.sort_by(|a, b| b.source_bones.len().cmp(&a.source_bones.len()).then_with(|| a.root.cmp(&b.root)));
    let largest = largest.iter().take(LARGEST_NAMED)
        .map(|g| fill(KEPT_GROUP_SIZE, &[("root", g.root.clone()), ("count", g.source_bones.len().to_string())]))
        .collect::<Vec<_>>().join(", ");
    fill(&too_many_bones_template(), &[("total", total.to_string()), ("limit", MAX_KEPT_BONES.to_string()), ("largest", largest)])
}
/// The check on the kept groups as a whole: groups that share bones (the first
/// pair in the groups' order), then the total against `MAX_KEPT_BONES`. The
/// same check is kept_bones_problem in secondary_bones.py.
pub fn kept_bones_problem(groups: &[PhysicsGroup]) -> Option<String> {
    for (at, first) in groups.iter().enumerate() {
        let bones: BTreeSet<&String> = first.source_bones.iter().collect();
        if let Some(second) = groups[at + 1..].iter().find(|g| g.source_bones.iter().any(|b| bones.contains(b))) {
            return Some(shared_bones_message(&first.root, &second.root));
        }
    }
    let total: usize = groups.iter().map(|g| g.source_bones.len()).sum();
    (total > MAX_KEPT_BONES).then(|| too_many_bones_message(total, groups))
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
    if roots.iter().any(|r| !groups.iter().any(|g| &g.root == r)) { return Err(invalid(stale_groups_message())); }
    let mut groups: Vec<_> = groups.into_iter().filter(|g| roots.contains(&g.root)).collect();
    // Each fitted gun carries its own charms (a copied gun's charms stay on the original gun).
    for hand in gun.map(crate::weapon::Plan::hands).unwrap_or_default().into_iter().filter(|h| h.action == "replace" && !h.copy) {
        let id = hand.objects.join("+");
        for group in groups.iter_mut().filter(|g| on_gun(&g.root, source, &source_roles, &id)) {
            if hand.anchor.is_empty() { return Err(invalid("The fitted gun has no weapon bone for its charms. Inspect the source again.")); }
            group.target_anchor = hand.anchor.clone();
        }
    }
    if let Some(problem) = kept_bones_problem(&groups) { return Err(invalid(problem)); }
    let mut exported = BTreeMap::new();
    let names = exported_names(&groups.iter().flat_map(|g| g.source_bones.iter().cloned()).collect::<Vec<_>>());
    for group in &groups {
        if !target.iter().any(|b| b.name == group.target_anchor) { return Err(invalid("Choose a valid target attachment for source physics bones.")); }
        for name in &group.source_bones {
            let output = names[name].clone();
            if output.len() > 63 || output.chars().any(char::is_control) || target.iter().any(|b| b.name == output) { return Err(invalid(format!("A preserved physics bone has a conflicting or unsupported name: {name}"))); }
            exported.insert(name.clone(), output);
        }
    }
    let compiled = target.iter().map(|b| &b.name).chain(exported.values());
    if let Some((first, second)) = deadlock_skeleton::aliases::compiled_name_collision(compiled) {
        return Err(invalid(deadlock_skeleton::aliases::compiled_name_error(&first, &second)));
    }
    Ok(Some(PreservationPlan { policy: POLICY.into(), groups, exported_bones: exported, new_physics: false, in_game_motion_tested: false }))
}
/// The prepared names of kept moving-part bones: PREFIX and the source name.
/// The model compiler writes every byte other than ASCII letters, digits and _
/// as _, so names that differ only there would become one bone (skirt
/// panels such as 侧裙_0_0, 前裙_0_0 and 后裙_0_0, two Chinese characters each).
/// Those take `distinct_name`; every other name is unchanged.
/// Keep in sync with secondary_bones.py exported_names.
pub fn exported_names(names: &[String]) -> BTreeMap<String, String> {
    let spelled = |n: &str| deadlock_skeleton::aliases::compiler_spelling(&format!("{PREFIX}{n}"));
    let mut counts = BTreeMap::<String, usize>::new();
    for name in names { *counts.entry(spelled(name)).or_default() += 1; }
    names.iter().map(|n| (n.clone(), if counts[&spelled(n)] > 1 { distinct_name(n) } else { format!("{PREFIX}{n}") })).collect()
}
/// A kept bone whose plain name the compiler would merge with another's keeps
/// its name and gets the first six hex digits of its name's SHA-256 (UTF-8).
pub fn distinct_name(name: &str) -> String {
    let digest = sha2::Sha256::digest(name.as_bytes());
    format!("{PREFIX}{name}_{:02x}{:02x}{:02x}", digest[0], digest[1], digest[2])
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
