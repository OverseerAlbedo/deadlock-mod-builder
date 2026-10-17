//! Per-part secondary motion. Names supply semantic hints; weighted geometry
//! and the hierarchy must independently support the special streamer topology.
use crate::{
    invalid,
    secondary::Group,
    surface,
    surface_geometry::{norm, sub},
    Result,
};
use deadlock_core::CancellationToken;
use deadlock_dmx::Dmx;
use deadlock_skeleton::{transforms, Skeleton};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Hair,
    Streamer,
    Tail,
    Ear,
    Cloth,
    LongSkirt,
    Accessory,
    /// Gentle spring for breasts (optional breast motion).
    Breast,
    /// Small, stiffer bounce for a butt (optional butt motion, off by default).
    Butt,
    /// Light, quick-settling swing for charms, keychains and pendants (gun charms).
    Charm,
    /// Follows its bone without motion: halos, hats, crowns, hairpins.
    Rigid,
}
impl Kind {
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "hair" => Ok(Self::Hair),
            "streamer" => Ok(Self::Streamer),
            "tail" => Ok(Self::Tail),
            "ear" => Ok(Self::Ear),
            "cloth" => Ok(Self::Cloth),
            "long_skirt" => Ok(Self::LongSkirt),
            "accessory" => Ok(Self::Accessory),
            "breast" => Ok(Self::Breast),
            "butt" => Ok(Self::Butt),
            "charm" => Ok(Self::Charm),
            "rigid" => Ok(Self::Rigid),
            _ => Err(invalid("Unknown moving-part type.")),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Part {
    pub root: String,
    pub kind: Kind,
    pub reason: String,
    pub weighted_chain: Vec<String>,
    pub chain_length_inches: f64,
    pub shape_radii_inches: [f64; 3],
    pub roll_locked: bool,
    pub review_recommended: bool,
}
/// Ordered weighted joints, allowing unweighted intermediate helpers but never
/// a second weighted branch or a disconnected selection.
pub fn chain(sk: &Skeleton, group: &Group) -> Vec<String> {
    let lookup = sk.indices();
    let used = group.weighted_bones.iter().collect::<BTreeSet<_>>();
    let mut order = group
        .bones
        .iter()
        .filter(|n| used.contains(n))
        .cloned()
        .collect::<Vec<_>>();
    // Groups are parent-first. Check nearest weighted ancestor, not suffixes.
    for j in 1..order.len() {
        let mut at = sk.parents[lookup[order[j].as_str()]];
        while at >= 0 && !used.contains(&sk.names[at as usize]) {
            at = sk.parents[at as usize];
        }
        if at < 0 || sk.names[at as usize] != order[j - 1] {
            order.clear();
            break;
        }
    }
    order
}
// Rotation-invariant PCA of the actual weighted vertices. Square-root
// eigenvalues describe length/width/thickness without assuming model axes.
fn radii(points: &[[f64; 3]]) -> [f64; 3] {
    if points.len() < 3 {
        return [0.; 3];
    }
    let mean: [f64; 3] =
        std::array::from_fn(|i| points.iter().map(|p| p[i]).sum::<f64>() / points.len() as f64);
    let mut a: [[f64; 3]; 3] = std::array::from_fn(|i| {
        std::array::from_fn(|j| {
            points
                .iter()
                .map(|p| (p[i] - mean[i]) * (p[j] - mean[j]))
                .sum::<f64>()
                / points.len() as f64
        })
    });
    for _ in 0..24 {
        let (i, j) = [(0, 1), (0, 2), (1, 2)]
            .into_iter()
            .max_by(|&(i, j), &(k, l)| a[i][j].abs().total_cmp(&a[k][l].abs()))
            .unwrap();
        if a[i][j].abs() < 1e-12 {
            break;
        }
        let angle = 0.5 * (2. * a[i][j]).atan2(a[j][j] - a[i][i]);
        let (c, s) = (angle.cos(), angle.sin());
        let (ai, aj, off) = (a[i][i], a[j][j], a[i][j]);
        a[i][i] = c * c * ai - 2. * c * s * off + s * s * aj;
        a[j][j] = s * s * ai + 2. * c * s * off + c * c * aj;
        a[i][j] = 0.;
        a[j][i] = 0.;
        for k in 0..3 {
            if k != i && k != j {
                let (x, y) = (a[k][i], a[k][j]);
                a[k][i] = c * x - s * y;
                a[i][k] = a[k][i];
                a[k][j] = s * x + c * y;
                a[j][k] = a[k][j];
            }
        }
    }
    let mut r = std::array::from_fn(|i| a[i][i].max(0.).sqrt());
    r.sort_by(|a, b| b.total_cmp(a));
    r
}
fn semantic(name: &str) -> Option<Kind> {
    let n = name.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| n.contains(w));
    // Butts, breasts and rigid accessories (hairpin, hair ornament, halo, hat)
    // come first: a hair or ear word in their name must not give them hair motion.
    if deadlock_preparation::secondary_bones::named_butt(name) {
        Some(Kind::Butt)
    } else if deadlock_preparation::secondary_bones::named_breast(name) {
        Some(Kind::Breast)
    } else if deadlock_preparation::secondary_bones::rigid_named(name) {
        Some(Kind::Rigid)
    }
    // Charm names win over hair and accessory words (a hair charm, a chest pendant).
    else if deadlock_preparation::secondary_bones::charm_named(name) {
        Some(Kind::Charm)
    }
    // Hair takes precedence over 'tail' (ponytail) and flat mesh evidence.
    else if has(&["hair", "ponytail", "braid", "bang", "髪", "发", "髮"]) {
        Some(Kind::Hair)
    } else if has(&["tail", "尻尾", "しっぽ", "尾"]) {
        Some(Kind::Tail)
    } else if n
        .split(|c: char| !c.is_alphanumeric())
        .any(|s| ["ear", "ears", "mimi"].contains(&s))
        || n.contains("耳")
    {
        Some(Kind::Ear)
    } else if has(&["chest", "胸", "ornament", "board"]) {
        Some(Kind::Accessory)
    } else if has(&["streamer", "ribbon", "リボン", "飘带", "飄帶", "tassel", "タッセル", "流苏", "流蘇"]) {
        Some(Kind::Streamer)
    } else if has(&[
        "cloth",
        "skirt",
        "cape",
        "shawl",
        "garment",
        "dress",
        "strap",
        "スカート",
        "裙",
    ]) {
        Some(Kind::Cloth)
    } else {
        None
    }
}
/// The hero's hip bones, where tails hang.
/// A sleeve name: "sleeve" in any case, or 袖 (sleeve in Japanese and Chinese,
/// as in 袖1.L or 袖子).
/// Bun names besides the word "bun": お団子 / 団子 (dango), 丸子, 发髻 / 髻, odango.
const BUN_WORDS: [&str; 5] = ["団子", "だんご", "丸子", "髻", "odango"];
/// A hair bun: a bun name, or one round piece of hair on one weighted bone
/// (no axis much longer than the others; SiViolet's two buns behind her head).
/// Buns stay still (Rigid) unless the user picks another part type.
fn hair_bun(name: &str, weighted_bones: usize, shape: [f64; 3]) -> bool {
    let lower = name.to_lowercase();
    let named = lower.split(|c: char| !c.is_alphanumeric()).any(|w| w == "bun" || w == "buns")
        || BUN_WORDS.iter().any(|w| lower.contains(w));
    let round = weighted_bones == 1 && shape[0] > 0.3 && shape[1] > 1e-5 && shape[0] / shape[1] <= 1.6;
    named || round
}
fn sleeve_named(name: &str) -> bool {
    name.to_lowercase().contains("sleeve") || name.contains('袖')
}
/// Weighted joints a long hanging strip has at least (a floor-length sleeve has
/// 24 to 30).
const LONG_STRIP_JOINTS: usize = 6;
/// A long hanging strip: one chain of at least [`LONG_STRIP_JOINTS`] weighted
/// joints whose mesh is thin and elongated.
fn long_strip(joints: usize, slender: bool, flat: bool) -> bool {
    joints >= LONG_STRIP_JOINTS && slender && flat
}
/// Whether every group whose mesh joins group `i` (3 or more shared triangles)
/// is a small part next to it: at most a quarter of its joints (2 or fewer
/// anyway) and a quarter of its length, like a cuff or a band. `sizes` are the
/// groups' (weighted joints, chain length).
fn joined_only_to_small_parts(i: usize, shared: &BTreeMap<(usize, usize), usize>, sizes: &[(usize, f64)]) -> bool {
    let (joints, length) = sizes[i];
    shared.iter().filter(|(&(a, b), &n)| n >= 3 && (a == i || b == i)).all(|(&(a, b), _)| {
        let (other_joints, other_length) = sizes[if a == i { b } else { a }];
        other_joints <= (joints / 4).max(2) && other_length <= length * 0.25
    })
}
fn is_hips(anchor: &str) -> bool {
    ["pelvis", "spine_0"].iter().any(|n| anchor.eq_ignore_ascii_case(n))
}
/// A group hanging from a hero weapon bone: a charm on a fitted gun. Same
/// weapon-bone names as deadlock_preparation::weapon::weapon_bones.
fn on_weapon(sk: &Skeleton, anchor: &str) -> bool {
    let mut at = sk.names.iter().position(|n| n == anchor).map_or(-1, |i| i as i32);
    let mut steps = 0;
    while at >= 0 && steps < 512 {
        let n = sk.names[at as usize].to_lowercase();
        if (n.contains("weapon") || n.contains("gun")) && !["target", "aim", "intent", "ik"].iter().any(|w| n.contains(w)) {
            return true;
        }
        at = sk.parents[at as usize];
        steps += 1;
    }
    false
}
/// A generic root (e.g. spring_01) must not hide named hair/tail descendants.
/// Never infer hair just because something is long, flat, or attached to head:
/// those properties are also shared by ribbons, veils and ornaments.
fn group_semantic(group: &Group) -> (Option<Kind>, bool) {
    if let Some(kind) = semantic(&group.source_root) {
        return (Some(kind), false);
    }
    let hints = group
        .bones
        .iter()
        .filter_map(|name| semantic(name))
        .collect::<Vec<_>>();
    if let Some(&kind) = hints.first() {
        if matches!(kind, Kind::Hair | Kind::Tail) && hints.iter().all(|&other| other == kind) {
            return (Some(kind), true);
        }
    }
    (None, false)
}
/// Why a part got its type, for the mirror rule below.
#[derive(Debug, Clone)]
struct Basis {
    /// The user chose the type.
    explicit: bool,
    /// Typed cloth only because its mesh joins another moving group.
    by_join: bool,
    /// Its own chain and mesh are a streamer's (one chain, thin and flat).
    own_streamer: bool,
    bones: usize,
    length: f64,
}
const JOINED_PANEL: &str = "Weighted mesh joins adjacent moving groups into one cloth panel";

/// The name with its side marker replaced (L/R, Left/Right as a word, 左/右),
/// when it has exactly one: L_sleeveD_01_jnt_1 and R_sleeveD_01_jnt_1 match.
fn mirror_key(name: &str) -> Option<String> {
    let chars: Vec<char> = name.chars().collect();
    let (mut out, mut sides, mut i) = (String::new(), 0, 0);
    while i < chars.len() {
        if chars[i] == '左' || chars[i] == '右' {
            out.push('*');
            sides += 1;
            i += 1;
        } else if chars[i].is_ascii_alphanumeric() {
            let start = i;
            while i < chars.len() && chars[i].is_ascii_alphanumeric() {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            if ["l", "r", "left", "right"].contains(&word.to_ascii_lowercase().as_str()) {
                out.push('*');
                sides += 1;
            } else {
                out.push_str(&word);
            }
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    (sides == 1).then_some(out)
}

/// A left/right pair is one part on two sides. When one side became a cloth
/// panel only because its mesh touches a moving group the other side does not
/// have (SiViolet: the left sleeve and a cord on the left forearm), while its
/// twin is a streamer by its own shape and both chains match, the joined side
/// takes the twin's type, provided its own shape is a streamer's too. Sheets
/// joined on both sides (MMD sleeves and skirts) and chosen types never change.
fn harmonize_mirrors(mut decided: Vec<(Part, Basis)>) -> Vec<Part> {
    let keys: Vec<Option<String>> = decided.iter().map(|(p, _)| mirror_key(&p.root)).collect();
    for i in 0..decided.len() {
        let Some(key) = &keys[i] else { continue };
        let Some(j) = (0..decided.len()).find(|&j| j != i && keys[j].as_ref() == Some(key)) else { continue };
        let ((a, sa), (b, sb)) = (&decided[i], &decided[j]);
        let matching = sa.bones == sb.bones && (sa.length - sb.length).abs() <= 0.1 * sa.length.max(sb.length);
        if sa.explicit || sb.explicit || !sa.by_join || sb.by_join || !sa.own_streamer || b.kind != Kind::Streamer || a.kind == b.kind || !matching {
            continue;
        }
        let twin = b.root.clone();
        let part = &mut decided[i].0;
        part.kind = Kind::Streamer;
        part.roll_locked = true;
        part.review_recommended = false;
        part.reason = format!("Same type as its mirror twin {twin}: a thin, elongated chain whose mesh also touches a moving group the other side does not have");
    }
    decided.into_iter().map(|(p, _)| p).collect()
}
pub fn analyze(
    d: &Dmx,
    groups: &[Group],
    overrides: &BTreeMap<String, String>,
    cancel: &CancellationToken,
) -> Result<Vec<Part>> {
    let bind = surface::binds(d)?;
    let sk = &bind.skeleton;
    let lookup = sk.indices();
    let state = transforms::bind_state(sk)?;
    let position = |n: &str| std::array::from_fn(|r| state.world[lookup[n]][r][3]);
    let meshes = surface::mesh_data(d)?;
    let mut points = vec![vec![]; groups.len()];
    let mut shared = BTreeMap::<(usize, usize), usize>::new();
    let owner = groups
        .iter()
        .enumerate()
        .flat_map(|(i, g)| g.weighted_bones.iter().map(move |n| (n.as_str(), i)))
        .collect::<BTreeMap<_, _>>();
    let palette = bind
        .palette
        .iter()
        .map(|&i| owner.get(d.elements[i].name.as_str()).copied())
        .collect::<Vec<_>>();
    for mesh in meshes.values() {
        let mut dominant = vec![None; mesh.positions.len()];
        for (i, (&p, (js, ws))) in mesh
            .positions
            .iter()
            .zip(mesh.joints.iter().zip(&mesh.weights))
            .enumerate()
        {
            if i % 4096 == 0 {
                cancel.check()?;
            }
            let mut sums = BTreeMap::<usize, f64>::new();
            for (&j, &w) in js.iter().zip(ws) {
                if let Some(Some(g)) = palette.get(j) {
                    *sums.entry(*g).or_default() += w;
                }
            }
            for (g, w) in sums {
                if w >= 0.5 {
                    points[g].push(p);
                    dominant[i] = Some(g);
                }
            }
        }
        for face in surface::triangles(d, mesh, cancel, None)? {
            let owners = face
                .into_iter()
                .filter_map(|i| dominant[i])
                .collect::<BTreeSet<_>>();
            for &a in &owners {
                for &b in &owners {
                    if a < b {
                        *shared.entry((a, b)).or_default() += 1;
                    }
                }
            }
        }
    }
    if overrides
        .keys()
        .any(|r| !groups.iter().any(|g| &g.source_root == r))
    {
        return Err(invalid(
            "A saved part type refers to an unselected motion group.",
        ));
    }
    let sizes:Vec<(usize,f64)>=groups.iter().map(|g|{let chain=chain(sk,g);(chain.len(),chain.windows(2).map(|w|norm(sub(position(&w[1]),position(&w[0])))).sum::<f64>())}).collect();
    let decided=groups.iter().enumerate().map(|(i,g)|{
        let chain=chain(sk,g);let length=chain.windows(2).map(|w|norm(sub(position(&w[1]),position(&w[0])))).sum::<f64>();
        let shape=radii(&points[i]);let slender=shape[1]>1e-5 && shape[0]/shape[1]>=3.;let flat=shape[1]>1e-5&&shape[2]/shape[1]<=0.35;
        let joined_panel=shared.iter().any(|(&(a,b),&n)|n>=3&&(a==i||b==i));
        let (hint, descendant_hint)=group_semantic(g);let explicit=overrides.get(&g.source_root).map(|s|Kind::parse(s)).transpose()?;
        let supported=chain.len()>=2&&length>1e-4;
        // A chain that ends where it starts is a band or ring, never a strand.
        let looped=chain.len()>=4&&length>1e-4&&norm(sub(position(&chain[chain.len()-1]),position(&chain[0])))<0.3*length;
        let (kind,reason,review):(Kind,String,bool)=if let Some(kind)=explicit{(kind,"Selected part type".into(),false)}
        else if looped&&matches!(hint,None|Some(Kind::Hair)){(Kind::Rigid,"Closed ring (halo, crown or hat band); follows its bone without motion".into(),true)}
        else if on_weapon(sk,&g.anchor)&&!matches!(hint,Some(Kind::Rigid|Kind::Breast|Kind::Butt)){(Kind::Charm,"Hangs from the fitted gun; light charm swing".into(),false)}
        else if let Some(kind)=hint{
            if kind==Kind::Streamer && !(supported&&slender&&flat){
                // A long, slender but round chain is a cord or a tassel bundle (SiViolet's
                // right arm tassel): it swings like a streamer. Wide or short ones are cloth.
                if supported&&slender{(Kind::Streamer,"Ribbon or tassel name on a long, round cord; streamer motion".into(),false)}
                else{(Kind::Cloth,"Ribbon name, but geometry is not a thin, elongated chain; using cloth support".into(),true)}
            }
            else if descendant_hint{(kind,"Consistent hair/tail names within the preserved physics group".into(),false)}
            else{(kind,"Part name checked against its weighted mesh and attachment hierarchy".into(),false)}
        }
        // Unnamed strands on the head are hair (names in other scripts are
        // often stripped, e.g. F_0_1 ... F_18_1), even when strips join.
        else if g.anchor.eq_ignore_ascii_case("head")&&supported{(Kind::Hair,"Unnamed strand on the head; hair motion".into(),false)}
        // A long hanging sleeve swings like a streamer even where its mesh
        // joins the rest of the sleeve (SiViolet's floor-length 袖1.L/R).
        else if sleeve_named(&g.source_root)&&long_strip(chain.len(),slender,flat){(Kind::Streamer,"Long hanging sleeve; streamer motion".into(),false)}
        else if joined_panel&&long_strip(chain.len(),slender,flat)&&joined_only_to_small_parts(i,&shared,&sizes){(Kind::Streamer,"Long thin strip whose mesh only joins small parts (a cuff or band); streamer motion".into(),false)}
        else if joined_panel{(Kind::Cloth,JOINED_PANEL.into(),false)}
        // A long, round, unnamed strand on the hips is a tail.
        else if is_hips(&g.anchor)&&supported&&chain.len()>=4&&slender&&!flat{(Kind::Tail,"Unnamed long round strand on the hips; tail motion".into(),false)}
        else if supported&&slender&&flat{(Kind::Streamer,"Thin, elongated weighted mesh on one connected joint chain".into(),false)}
        else{(Kind::Accessory,"No unambiguous part identity; conservative accessory support".into(),true)};
        // Hair buns stay still (SiViolet: the right bun flung around).
        let (kind,mut reason,mut review)=if explicit.is_none()&&kind==Kind::Hair&&hair_bun(&g.source_root,g.weighted_bones.len(),shape)
            {(Kind::Rigid,"Hair bun (one round piece of hair, or a bun name); stays still".into(),false)} else {(kind,reason,review)};
        if kind==Kind::Streamer&&!supported{return Err(invalid(format!("{} cannot use streamer motion: it needs one chain of at least two weighted bones. Choose Cloth for a branching part.",g.source_root)));}
        let strand=matches!(kind,Kind::Hair|Kind::Tail);
        if strand&&!supported {
            review=true;
            reason.push_str(if kind==Kind::Hair {"; shared hair movement and twist protection need one non-collapsed chain of at least two weighted bones"} else {"; twist protection needs one non-collapsed chain of at least two weighted bones"});
        }
        let basis=Basis{explicit:explicit.is_some(),by_join:reason==JOINED_PANEL,own_streamer:supported&&slender&&flat,bones:chain.len(),length};
        Ok((Part{root:g.source_root.clone(),kind,reason,weighted_chain:chain,chain_length_inches:length,shape_radii_inches:shape,roll_locked:kind==Kind::Streamer||(strand&&supported),review_recommended:review},basis))
    }).collect::<Result<Vec<_>>>()?;
    Ok(harmonize_mirrors(decided))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn part(root: &str, kind: Kind) -> Part {
        Part { root: root.into(), kind, reason: String::new(), weighted_chain: vec![], chain_length_inches: 56.2, shape_radii_inches: [0.; 3], roll_locked: kind == Kind::Streamer, review_recommended: false }
    }
    fn basis(explicit: bool, by_join: bool, own_streamer: bool, bones: usize, length: f64) -> Basis {
        Basis { explicit, by_join, own_streamer, bones, length }
    }
    #[test]
    fn hair_buns_are_round_one_bone_pieces_or_bun_names() {
        // SiViolet's buns: one weighted bone, about 1.5 x 1.4 x 0.8 in.
        assert!(hair_bun("L_hairD_01_jnt", 1, [1.54, 1.38, 0.83]));
        // A strand on one bone is long, not round; strands on several bones move.
        assert!(!hair_bun("ahoge_01", 1, [2.4, 0.5, 0.2]));
        assert!(!hair_bun("L_hairC_01_jnt", 3, [4.21, 1.23, 0.42]));
        // Names count whatever the shape, but not words that merely contain "bun".
        for name in ["hair_bun_L", "Buns", "お団子.R", "发髻", "Odango_01"] {
            assert!(hair_bun(name, 4, [3., 0.5, 0.2]), "{name}");
        }
        assert!(!hair_bun("bunny_ear_L", 3, [3., 0.5, 0.2]));
    }
    #[test]
    fn long_sleeves_and_strips_joined_only_to_cuffs_are_streamers() {
        for name in ["L_sleeveD_01_jnt_1", "Sleeve_L", "袖1.L", "左袖子"] {
            assert!(sleeve_named(name), "{name}");
        }
        assert!(!sleeve_named("L_ribbonA_01_jnt_1"));
        // A floor-length sleeve (24 joints, thin and flat) is a long strip; a
        // one- or two-joint cuff is not, and neither is a round cord.
        assert!(long_strip(24, true, true));
        assert!(!long_strip(2, true, true) && !long_strip(24, true, false) && !long_strip(24, false, true));
        // Group 0 is the sleeve; 1 and 2 are a cuff and a band; 3 is a panel of
        // the same size (a skirt panel next to another).
        let sizes = [(24, 56.), (1, 0.), (2, 3.2), (24, 50.)];
        let only_cuffs = BTreeMap::from([((0, 1), 12), ((0, 2), 5), ((1, 2), 3)]);
        assert!(joined_only_to_small_parts(0, &only_cuffs, &sizes));
        let with_panel = BTreeMap::from([((0, 1), 12), ((0, 3), 40)]);
        assert!(!joined_only_to_small_parts(0, &with_panel, &sizes));
        // Two or fewer shared triangles are not a join.
        let touching = BTreeMap::from([((0, 3), 2)]);
        assert!(joined_only_to_small_parts(0, &touching, &sizes));
    }
    #[test]
    fn mirror_names_match_by_their_side_marker() {
        assert_eq!(mirror_key("L_sleeveD_01_jnt_1"), mirror_key("R_sleeveD_01_jnt_1"));
        assert_eq!(mirror_key("sleeve.L"), mirror_key("sleeve.R"));
        assert_eq!(mirror_key("Hair_Left_01"), mirror_key("Hair_Right_01"));
        assert_eq!(mirror_key("袖左"), mirror_key("袖右"));
        assert!(mirror_key("M_hairA_01_jnt").is_none() && mirror_key("裙子_0_1").is_none() && mirror_key("L_R_strap").is_none());
        assert_ne!(mirror_key("L_sleeveD_01_jnt_1"), mirror_key("R_sleeveI_01_jnt"));
    }
    #[test]
    fn a_side_joined_only_on_its_own_follows_its_streamer_twin() {
        // SiViolet: the left sleeve touches a cord on the left forearm only.
        let parts = harmonize_mirrors(vec![
            (part("L_sleeveD_01_jnt_1", Kind::Cloth), basis(false, true, true, 30, 56.2)),
            (part("R_sleeveD_01_jnt_1", Kind::Streamer), basis(false, false, true, 30, 56.2)),
            (part("L_sleeveI_01_jnt", Kind::Accessory), basis(false, false, false, 2, 3.2)),
        ]);
        assert_eq!(parts.iter().map(|p| p.kind).collect::<Vec<_>>(), [Kind::Streamer, Kind::Streamer, Kind::Accessory]);
        assert!(parts[0].roll_locked && parts[0].reason.contains("R_sleeveD_01_jnt_1"));
    }
    #[test]
    fn sheets_joined_on_both_sides_chosen_types_and_different_chains_stay() {
        let kinds = |rows: Vec<(Part, Basis)>| harmonize_mirrors(rows).into_iter().map(|p| p.kind).collect::<Vec<_>>();
        // MMD sleeve or skirt sheets: both sides join their neighbors.
        assert_eq!(kinds(vec![(part("袖_L_0", Kind::Cloth), basis(false, true, true, 8, 20.)), (part("袖_R_0", Kind::Cloth), basis(false, true, true, 8, 20.))]), [Kind::Cloth, Kind::Cloth]);
        // The user chose cloth for the joined side.
        assert_eq!(kinds(vec![(part("L_sleeve", Kind::Cloth), basis(true, false, true, 30, 56.)), (part("R_sleeve", Kind::Streamer), basis(false, false, true, 30, 56.))]), [Kind::Cloth, Kind::Streamer]);
        // Its own shape is not a streamer's (a wide panel).
        assert_eq!(kinds(vec![(part("L_sleeve", Kind::Cloth), basis(false, true, false, 30, 56.)), (part("R_sleeve", Kind::Streamer), basis(false, false, true, 30, 56.))]), [Kind::Cloth, Kind::Streamer]);
        // The chains differ (bone count or length).
        assert_eq!(kinds(vec![(part("L_sleeve", Kind::Cloth), basis(false, true, true, 12, 56.)), (part("R_sleeve", Kind::Streamer), basis(false, false, true, 30, 56.))]), [Kind::Cloth, Kind::Streamer]);
        assert_eq!(kinds(vec![(part("L_sleeve", Kind::Cloth), basis(false, true, true, 30, 30.)), (part("R_sleeve", Kind::Streamer), basis(false, false, true, 30, 56.))]), [Kind::Cloth, Kind::Streamer]);
    }
    #[test]
    fn generic_group_uses_consistent_descendants_but_never_overrides_a_known_part() {
        let mut group = Group {
            source_root: "spring_001".into(),
            root: "spring_001".into(),
            anchor: "head".into(),
            bones: vec![
                "spring_001".into(),
                "hair_back_02".into(),
                "hair_back_03".into(),
            ],
            weighted_bones: vec!["hair_back_02".into(), "hair_back_03".into()],
        };
        assert_eq!(group_semantic(&group), (Some(Kind::Hair), true));
        group.bones = vec!["spring_001".into(), "tail_02".into()];
        assert_eq!(group_semantic(&group), (Some(Kind::Tail), true));
        group.bones.push("hair_03".into());
        assert_eq!(group_semantic(&group), (None, false));
        group.source_root = "cloth_panel".into();
        assert_eq!(group_semantic(&group), (Some(Kind::Cloth), false));
        group.source_root = "spring_001".into();
        group.bones = vec!["spring_001".into(), "spring_002".into()];
        assert_eq!(group_semantic(&group), (None, false));
    }
    #[test]
    fn names_keep_hair_tail_ears_and_streamers_separate() {
        for (n, k) in [
            ("ponytail_ribbon_hair", Kind::Hair),
            ("tail_base", Kind::Tail),
            ("ear_L", Kind::Ear),
            ("ribbon_left", Kind::Streamer),
            ("tasselA_01_jnt", Kind::Streamer),
            ("sword_tassel_R", Kind::Streamer),
            ("skirt_front", Kind::Cloth),
        ] {
            assert_eq!(semantic(n), Some(k));
        }
        assert_eq!(semantic("spring_01"), None);
        assert_eq!(semantic("rear_support"), None);
    }
    #[test]
    fn breasts_and_rigid_accessories_never_get_hair_motion() {
        for (n, k) in [
            ("Bone_Breast_L_00", Kind::Breast),
            ("Butt_L_00", Kind::Butt),
            ("臀部_R", Kind::Butt),
            ("尻尾_01", Kind::Tail),
            ("左胸", Kind::Breast),
            ("HairPin_01", Kind::Rigid),
            ("hair_ornament", Kind::Rigid),
            ("髪飾り", Kind::Rigid),
            ("Halo_00", Kind::Rigid),
            ("hat_base_M_a_01_jnt", Kind::Rigid),
            ("Hat_Ribbon_00", Kind::Streamer),
            ("hair_back_01", Kind::Hair),
        ] {
            assert_eq!(semantic(n), Some(k), "{n}");
        }
        let maps = crate::secondary_auto::parameters(Kind::Rigid, 0.5, false, 4., 0.2, None).unwrap();
        assert_eq!((maps[0], maps[3]), (1., 0.));
        let breast = crate::secondary_auto::parameters(Kind::Breast, 0., true, 2., 0.2, None).unwrap();
        assert!(breast[0] < 1. && breast[3] == 0. && breast[4] >= 0.4 && breast[4] <= 1.5);
    }
    #[test]
    fn charms_get_a_light_swing_that_never_sags_into_the_gun() {
        for (n, k) in [
            ("Charm_00", Kind::Charm),
            ("Key_Chain_01", Kind::Charm),
            ("hair_charm", Kind::Charm),
            ("chest_pendant", Kind::Charm),
            ("Earring_Dangle", Kind::Charm),
            ("ペンダント", Kind::Charm),
            ("gun_strap", Kind::Cloth),
        ] {
            assert_eq!(semantic(n), Some(k), "{n}");
        }
        let pivot = crate::secondary_auto::parameters(Kind::Charm, 0., true, 3., 0.2, None).unwrap();
        assert_eq!(pivot[0], 1.);
        let tip = crate::secondary_auto::parameters(Kind::Charm, 1., false, 3., 0.2, None).unwrap();
        // Goal, drag, gravity and travel: settles quickly, no sag, small travel.
        assert!(tip[0] >= 0.3 && tip[0] < 0.5 && tip[1] >= 0.3 && tip[3] == 0. && tip[4] > 0. && tip[4] <= 1.2);
        let soft = crate::secondary_auto::parameters(Kind::Charm, 1., false, 3., 0.2, Some("soft")).unwrap();
        let stiff = crate::secondary_auto::parameters(Kind::Charm, 1., false, 3., 0.2, Some("stiff")).unwrap();
        assert!(soft[4] > tip[4] && tip[4] > stiff[4] && stiff[0] > tip[0] && tip[0] > soft[0]);
        // A long strap is still limited; a tiny keychain still moves a little.
        assert!(crate::secondary_auto::parameters(Kind::Charm, 1., false, 40., 0.2, None).unwrap()[4] <= 1.2);
        assert!(crate::secondary_auto::parameters(Kind::Charm, 1., false, 0.1, 0.2, None).unwrap()[4] >= 0.2);
        assert!(crate::secondary_auto::parameters(Kind::Charm, 1., false, 3., 0.2, Some("wild")).is_err());
    }
    #[test]
    fn groups_on_a_hero_weapon_bone_are_charms() {
        let sk = Skeleton {
            names: vec!["pelvis".into(), "hand_R".into(), "weaponPivot".into(), "weapon".into(), "weapon_aim_target".into()],
            parents: vec![-1, 0, 1, 2, 0],
            positions: vec![[0.; 3]; 5],
            rotations: vec![[0., 0., 0., 1.]; 5],
            scales: vec![1.; 5],
            flags: vec![0; 5],
        };
        assert!(on_weapon(&sk, "weapon") && on_weapon(&sk, "weaponPivot"));
        assert!(!on_weapon(&sk, "hand_R") && !on_weapon(&sk, "weapon_aim_target") && !on_weapon(&sk, "missing"));
    }
    #[test]
    fn shape_is_rotation_invariant() {
        let p = vec![
            [-10., -1., 0.],
            [10., -1., 0.],
            [10., 1., 0.],
            [-10., 1., 0.],
        ];
        let a = radii(&p);
        let q = p
            .iter()
            .map(|p| {
                [
                    (p[0] - p[1]) * 0.5f64.sqrt(),
                    (p[0] + p[1]) * 0.5f64.sqrt(),
                    0.,
                ]
            })
            .collect::<Vec<_>>();
        let b = radii(&q);
        for i in 0..3 {
            assert!((a[i] - b[i]).abs() < 1e-8);
        }
    }
}
