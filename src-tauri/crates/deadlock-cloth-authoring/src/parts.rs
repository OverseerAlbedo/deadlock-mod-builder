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
    /// Long hanging strips inside a cloth part, moving like streamers (`strips`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub strips: Vec<Strip>,
}
/// A long hanging strip inside a cloth part: the weighted joints from the part's
/// root to a tip, root first. `branch` is where it leaves the joints it shares
/// with other strips (0 when the part does not fork before it).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Strip {
    pub path: Vec<String>,
    pub branch: usize,
}
/// Torso lengths (head to pelvis) a strip reaches at least: arcaneVin's shoulder
/// capes R_capeC/D (0.67) count, skirt panels of 10–12 in (0.4) do not.
const STRIP_REACH: f64 = 0.6;
/// Long hanging strips inside a cloth part (arcaneVin's cloak panels L/R_capeB and
/// ribbonC, its shoulder capes R_capeC/D, the capes hanging from its breast
/// bones), which move like streamers, as SiViolet's floor-length sleeves do: each
/// path of weighted joints from the part's root to a tip whose stretch below its
/// last fork has at least three weighted joints, that reaches STRIP_REACH torso
/// lengths, and whose own mesh (below the fork) is at least twice as long as
/// wide and flat (thickness at most 0.4 of its width).
/// The root must be weighted: streamer tethers hang from the fixed point next to it.
fn strips(sk: &Skeleton, group: &Group, position: &dyn Fn(&str) -> [f64; 3], shape: &dyn Fn(&[String]) -> [f64; 3], torso: f64) -> Vec<Strip> {
    let lookup = sk.indices();
    let used = group.weighted_bones.iter().map(String::as_str).collect::<BTreeSet<_>>();
    if !used.contains(group.root.as_str()) || torso <= 1e-4 {
        return vec![];
    }
    let mut children = BTreeMap::<&str, Vec<&str>>::new();
    for name in group.bones.iter().map(String::as_str).filter(|n| used.contains(n) && *n != group.root) {
        let mut at = sk.parents[lookup[name]];
        while at >= 0 && !used.contains(sk.names[at as usize].as_str()) {
            at = sk.parents[at as usize];
        }
        if at < 0 {
            return vec![];
        }
        children.entry(sk.names[at as usize].as_str()).or_default().push(name);
    }
    let mut found = vec![];
    let mut pending = vec![vec![group.root.as_str()]];
    while let Some(path) = pending.pop() {
        let last = *path.last().unwrap();
        match children.get(last) {
            Some(next) => pending.extend(next.iter().rev().map(|&c| [path.clone(), vec![c]].concat())),
            None => {
                let branch = (0..path.len() - 1).rev().find(|&i| children.get(path[i]).is_some_and(|c| c.len() > 1)).map_or(0, |i| i + 1);
                let reach: f64 = path.windows(2).map(|w| norm(sub(position(w[1]), position(w[0])))).sum();
                // Its own mesh, alone or with the joint it hangs from: arcaneVin's
                // left cloak panel is a strip only with its fork joint (ribbonC
                // leaves there), the strap below its box only without it.
                let strip_like = |bones: &[&str]| {
                    let r = shape(&bones.iter().map(|n| n.to_string()).collect::<Vec<_>>());
                    r[1] > 1e-5 && r[0] / r[1] >= 2. && r[2] / r[1] <= 0.4
                };
                let strip_like = strip_like(&path[branch..]) || (branch > 0 && strip_like(&path[branch - 1..]));
                if path.len() - branch >= 3 && reach >= STRIP_REACH * torso && strip_like {
                    found.push(Strip { path: path.iter().map(|n| n.to_string()).collect(), branch });
                }
            }
        }
    }
    found
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
/// Weighted joints a small piece hanging off a strand may have (a braid's tie
/// or ornament) for `trunk` to still find one strand.
const SIDE_PIECE_BONES: usize = 2;
/// A strand's main chain when small pieces hang off it: arcaneVin's braid
/// `plait__jnt1` carries `L/R_plaitA` and `L/R_plaitB` (one weighted bone each)
/// along its 29 bones. At each fork the chain continues into the branch with
/// the most weighted joints; every other branch must be a piece of at most
/// SIDE_PIECE_BONES and smaller than the continuation. Empty when the group is
/// not one strand (a cape with long panels). Only for hair and tails: their
/// side pieces still get their own simulated points, a streamer's would not.
fn trunk(sk: &Skeleton, group: &Group) -> Vec<String> {
    let lookup = sk.indices();
    let used = group.weighted_bones.iter().map(String::as_str).collect::<BTreeSet<_>>();
    let mut children = BTreeMap::<&str, Vec<&str>>::new();
    let mut tops = vec![];
    for name in group.bones.iter().map(String::as_str).filter(|n| used.contains(n)) {
        let mut at = sk.parents[lookup[name]];
        while at >= 0 && !used.contains(sk.names[at as usize].as_str()) {
            at = sk.parents[at as usize];
        }
        if at >= 0 {
            children.entry(sk.names[at as usize].as_str()).or_default().push(name);
        } else {
            tops.push(name);
        }
    }
    fn size(at: &str, children: &BTreeMap<&str, Vec<&str>>) -> usize {
        1 + children.get(at).into_iter().flatten().map(|c| size(c, children)).sum::<usize>()
    }
    let [mut at] = tops[..] else { return vec![] };
    let mut order = vec![at.to_string()];
    while let Some(next) = children.get(at) {
        let mut ranked = next.iter().map(|&c| (c, size(c, &children))).collect::<Vec<_>>();
        ranked.sort_by(|a, b| b.1.cmp(&a.1));
        if ranked.len() > 1 && (ranked[1].1 > SIDE_PIECE_BONES || ranked[1].1 >= ranked[0].1) {
            return vec![];
        }
        at = ranked[0].0;
        order.push(at.to_string());
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
    else if has(&["hair", "ponytail", "braid", "plait", "bang", "髪", "发", "髮", "辫", "辮", "三つ編"]) {
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
/// How far (torso lengths, head to pelvis) the weighted bones of a breast- or
/// butt-named group may reach from its root and still be a breast or butt.
/// Small pieces on a breast (YvvoneHaze's, about 0.2) swing as its charms;
/// a cape or garment hanging from it reaches much farther (arcaneVin's MMD
/// 胸上2.R carries a cape to the knees, about 2; TangTang's clothes about 1.3).
const BODY_PART_REACH: f64 = 0.5;
/// The part type of what hangs from a breast or butt bone that carries a
/// longer part: the type its other bones are named for when they agree, Cloth
/// when they name several (a cape and ribbons), none when they are unnamed
/// (the mesh decides then).
fn hanging_hint(group: &Group) -> Option<Kind> {
    let mut kinds: Vec<Kind> = vec![];
    for kind in group.bones.iter().skip(1).filter_map(|n| semantic(n)) {
        if !matches!(kind, Kind::Breast | Kind::Butt) && !kinds.contains(&kind) {
            kinds.push(kind);
        }
    }
    match kinds.as_slice() {
        [] => None,
        [kind] => Some(*kind),
        _ => Some(Kind::Cloth),
    }
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
        part.strips.clear();
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
    let mut bone_points = BTreeMap::<String, Vec<[f64; 3]>>::new();
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
            // The bone with the most weight on the vertex, for each strip's own shape.
            if let Some((&j, _)) = js.iter().zip(ws).filter(|&(&j, _)| palette.get(j).is_some_and(|g| g.is_some())).max_by(|a, b| a.1.total_cmp(b.1)) {
                bone_points.entry(d.elements[bind.palette[j]].name.clone()).or_default().push(p);
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
    let torso=if lookup.contains_key("head")&&lookup.contains_key("pelvis"){norm(sub(position("head"),position("pelvis")))}else{0.};
    let decided=groups.iter().enumerate().map(|(i,g)|{
        let shape=radii(&points[i]);let slender=shape[1]>1e-5 && shape[0]/shape[1]>=3.;let flat=shape[1]>1e-5&&shape[2]/shape[1]<=0.35;
        let joined_panel=shared.iter().any(|(&(a,b),&n)|n>=3&&(a==i||b==i));
        let (hint, descendant_hint)=group_semantic(g);let explicit=overrides.get(&g.source_root).map(|s|Kind::parse(s)).transpose()?;
        // A breast or butt bone that carries a longer part (a cape, a garment)
        // moves as that part: the whole group is one simulation.
        let reach=g.weighted_bones.iter().map(|n|norm(sub(position(n),position(&g.root)))).fold(0.,f64::max);
        let carried=matches!(hint,Some(Kind::Breast|Kind::Butt))&&torso>1e-4&&reach>BODY_PART_REACH*torso;
        let carrier=if hint==Some(Kind::Butt){"butt"}else{"breast"};
        let (hint,descendant_hint)=if carried{(hanging_hint(g),false)}else{(hint,descendant_hint)};
        // Hair and tails with small pieces hanging off them (a braid's ties) are still one strand.
        let strand_hint=explicit.map_or(matches!(hint,Some(Kind::Hair|Kind::Tail))||(hint.is_none()&&g.anchor.eq_ignore_ascii_case("head")),|k|matches!(k,Kind::Hair|Kind::Tail));
        let chain=match chain(sk,g) {strict if strict.is_empty()&&strand_hint=>trunk(sk,g),strict=>strict};
        let length=chain.windows(2).map(|w|norm(sub(position(&w[1]),position(&w[0])))).sum::<f64>();
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
        if carried&&explicit.is_none(){
            reason=format!("A longer part hangs from this {carrier} bone (reaching {reach:.0} in); it moves with that part, not as a {carrier}");
        }
        if kind==Kind::Streamer&&!supported{return Err(invalid(format!("{} cannot use streamer motion: it needs one chain of at least two weighted bones. Choose Cloth for a branching part.",g.source_root)));}
        let strand=matches!(kind,Kind::Hair|Kind::Tail);
        if strand&&!supported {
            review=true;
            reason.push_str(if kind==Kind::Hair {"; shared hair movement and twist protection need one non-collapsed chain of at least two weighted bones"} else {"; twist protection needs one non-collapsed chain of at least two weighted bones"});
        }
        let basis=Basis{explicit:explicit.is_some(),by_join:reason==JOINED_PANEL,own_streamer:supported&&slender&&flat,bones:chain.len(),length};
        // Panels whose mesh joins their neighbors stay one sheet (paimonRem's PF_0_*).
        let strips=if explicit.is_none()&&kind==Kind::Cloth&&reason!=JOINED_PANEL {
            let shape=|bones:&[String]|radii(&bones.iter().flat_map(|b|bone_points.get(b).into_iter().flatten().copied()).collect::<Vec<_>>());
            strips(sk,g,&position,&shape,torso)
        } else {vec![]};
        if !strips.is_empty() {
            reason.push_str(&format!("; {} long hanging strip{} move{} like streamers",strips.len(),if strips.len()==1 {""} else {"s"},if strips.len()==1 {"s"} else {""}));
        }
        Ok((Part{root:g.source_root.clone(),kind,reason,weighted_chain:chain,chain_length_inches:length,shape_radii_inches:shape,roll_locked:kind==Kind::Streamer||(strand&&supported),review_recommended:review,strips},basis))
    }).collect::<Result<Vec<_>>>()?;
    Ok(harmonize_mirrors(decided))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn part(root: &str, kind: Kind) -> Part {
        Part { root: root.into(), kind, reason: String::new(), weighted_chain: vec![], chain_length_inches: 56.2, shape_radii_inches: [0.; 3], roll_locked: kind == Kind::Streamer, review_recommended: false, strips: vec![] }
    }
    fn basis(explicit: bool, by_join: bool, own_streamer: bool, bones: usize, length: f64) -> Basis {
        Basis { explicit, by_join, own_streamer, bones, length }
    }
    fn group(bones: &[&str]) -> Group {
        let names: Vec<String> = bones.iter().map(|n| format!("dmb_secondary_{n}")).collect();
        Group { source_root: bones[0].into(), root: names[0].clone(), anchor: "spine_3".into(), weighted_bones: names.clone(), bones: names }
    }
    /// A skeleton of (name, parent) rows and the group of every bone but the first.
    fn rig(rows: &[(&str, i32)], weighted: &[&str]) -> (Skeleton, Group) {
        let n = rows.len();
        let sk = Skeleton { names: rows.iter().map(|r| r.0.into()).collect(), parents: rows.iter().map(|r| r.1).collect(),
            positions: vec![[0.; 3]; n], rotations: vec![[0., 0., 0., 1.]; n], scales: vec![1.; n], flags: vec![0; n] };
        let bones: Vec<String> = rows[1..].iter().map(|r| r.0.into()).collect();
        (sk, Group { source_root: bones[0].clone(), root: bones[0].clone(), anchor: rows[0].0.into(), bones, weighted_bones: weighted.iter().map(|w| w.to_string()).collect() })
    }
    #[test]
    fn a_strand_with_small_side_pieces_keeps_its_main_chain() {
        // arcaneVin's braid: ties hang off joints 2 and 4; joint 3 is an unweighted helper.
        let (sk, braid) = rig(&[("head", -1), ("plait1", 0), ("plait2", 1), ("tie_L", 2), ("tie_R", 2), ("plait3", 2), ("plait4", 5), ("bead", 6), ("plait5", 6), ("plait6", 8)],
            &["plait1", "plait2", "tie_L", "tie_R", "plait4", "bead", "plait5", "plait6"]);
        assert!(chain(&sk, &braid).is_empty());
        assert_eq!(trunk(&sk, &braid), ["plait1", "plait2", "plait4", "plait5", "plait6"]);
        // A cape splitting into two long panels is not one strand.
        let (sk, cape) = rig(&[("spine", -1), ("cape1", 0), ("left1", 1), ("left2", 2), ("left3", 3), ("right1", 1), ("right2", 5), ("right3", 6)],
            &["cape1", "left1", "left2", "left3", "right1", "right2", "right3"]);
        assert!(trunk(&sk, &cape).is_empty());
        // Nor is a tip that splits in two equal ends.
        let (sk, fork) = rig(&[("head", -1), ("a", 0), ("b", 1), ("c", 1)], &["a", "b", "c"]);
        assert!(trunk(&sk, &fork).is_empty());
        assert!(semantic("plait__jnt1") == Some(Kind::Hair) && semantic("三つ編み_L") == Some(Kind::Hair) && semantic("辫子_01") == Some(Kind::Hair));
    }
    #[test]
    fn long_panels_inside_a_cloth_part_are_strips() {
        // arcaneVin's back cloak: a short center strip, a small flap, two long
        // panels to the knees, and a short tab that is not a strip.
        let rows = [("spine", -1), ("cape1", 0), ("flap", 1), ("cape2", 1), ("left1", 3), ("left2", 4), ("left3", 5), ("right1", 3), ("right2", 7), ("right3", 8), ("tab1", 1), ("tab2", 10), ("tab3", 11)];
        let weighted: Vec<&str> = rows[1..].iter().map(|r| r.0).collect();
        let (sk, cape) = rig(&rows, &weighted);
        let at = |n: &str| -> [f64; 3] {
            match n {
                "spine" | "cape1" => [0., 0., 70.], "flap" => [0., 3., 66.], "cape2" => [0., 0., 64.],
                "left1" => [-1., 4., 54.], "left2" => [-2., 5., 44.], "left3" => [-3., 6., 34.],
                "right1" => [-1., -4., 54.], "right2" => [-2., -5., 44.], "right3" => [-3., -6., 34.],
                "tab1" => [0., 0., 68.], "tab2" => [0., 0., 66.], _ => [0., 0., 64.],
            }
        };
        let strip_shape = |_: &[String]| [10., 4., 1.];
        let strips_found = strips(&sk, &cape, &at, &strip_shape, 27.);
        let tips: Vec<&str> = strips_found.iter().map(|s| s.path.last().unwrap().as_str()).collect();
        assert_eq!(tips, ["left3", "right3"]);
        assert_eq!(strips_found[0].path, ["cape1", "cape2", "left1", "left2", "left3"]);
        assert_eq!(strips_found[0].branch, 2);
        // A tube or a block is not a strip, and an unweighted root has none.
        assert!(strips(&sk, &cape, &at, &|_: &[String]| [10., 6., 5.], 27.).is_empty());
        let (sk, loose) = rig(&rows, &weighted[1..]);
        assert!(strips(&sk, &loose, &at, &strip_shape, 27.).is_empty());
    }
    #[test]
    fn a_breast_bone_carrying_a_longer_part_takes_that_part_s_type() {
        // arcaneVin (MMD): a cape hangs from each 胸上2; the left one also
        // carries ribbons, a phone and a clip.
        assert_eq!(hanging_hint(&group(&["胸上2.R", "R_capeA__jnt1", "R_capeA__jnt5", "胸先.R"])), Some(Kind::Cloth));
        assert_eq!(hanging_hint(&group(&["胸上2.L", "L_capeA__jnt1", "ribbonE_01_jnt", "phone_jnt", "clipA_jnt", "ribbonJ_01_jnt", "胸先.L"])), Some(Kind::Cloth));
        // TangTang: front clothes flaps with spikes at their ends.
        assert_eq!(hanging_hint(&group(&["胸上2.L", "clothes_base_L_a_01_jnt", "spike_base_L_a_01_jnt"])), Some(Kind::Cloth));
        assert_eq!(hanging_hint(&group(&["胸上2.L", "ribbon_L_01", "ribbon_L_02"])), Some(Kind::Streamer));
        // Unnamed pieces leave it to the mesh; the breast's own tip never counts.
        assert_eq!(hanging_hint(&group(&["胸上2.L", "Bone_012", "胸先.L"])), None);
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
