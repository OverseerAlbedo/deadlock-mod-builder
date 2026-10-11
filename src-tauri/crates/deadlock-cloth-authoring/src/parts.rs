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
    // Breasts and rigid accessories (hairpin, hair ornament, halo, hat) come
    // first: a hair or ear word in their name must not give them hair motion.
    if deadlock_preparation::secondary_bones::named_breast(name) {
        Some(Kind::Breast)
    } else if deadlock_preparation::secondary_bones::rigid_named(name) {
        Some(Kind::Rigid)
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
    } else if has(&["streamer", "ribbon", "リボン", "飘带", "飄帶"]) {
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
    groups.iter().enumerate().map(|(i,g)|{
        let chain=chain(sk,g);let length=chain.windows(2).map(|w|norm(sub(position(&w[1]),position(&w[0])))).sum::<f64>();
        let shape=radii(&points[i]);let slender=shape[1]>1e-5 && shape[0]/shape[1]>=3.;let flat=shape[1]>1e-5&&shape[2]/shape[1]<=0.35;
        let joined_panel=shared.iter().any(|(&(a,b),&n)|n>=3&&(a==i||b==i));
        let (hint, descendant_hint)=group_semantic(g);let explicit=overrides.get(&g.source_root).map(|s|Kind::parse(s)).transpose()?;
        let supported=chain.len()>=2&&length>1e-4;
        // A chain that ends where it starts is a band or ring, never a strand.
        let looped=chain.len()>=4&&length>1e-4&&norm(sub(position(&chain[chain.len()-1]),position(&chain[0])))<0.3*length;
        let (kind,mut reason,mut review):(Kind,String,bool)=if let Some(kind)=explicit{(kind,"Selected part type".into(),false)}
        else if looped&&matches!(hint,None|Some(Kind::Hair)){(Kind::Rigid,"Closed ring (halo, crown or hat band); follows its bone without motion".into(),true)}
        else if let Some(kind)=hint{
            if kind==Kind::Streamer && !(supported&&slender&&flat){(Kind::Cloth,"Ribbon name, but geometry is not a thin, elongated chain; using cloth support".into(),true)}
            else if descendant_hint{(kind,"Consistent hair/tail names within the preserved physics group".into(),false)}
            else{(kind,"Part name checked against its weighted mesh and attachment hierarchy".into(),false)}
        }else if joined_panel{(Kind::Cloth,"Weighted mesh joins adjacent moving groups into one cloth panel".into(),false)}
        else if supported&&slender&&flat{(Kind::Streamer,"Thin, elongated weighted mesh on one connected joint chain".into(),false)}
        else{(Kind::Accessory,"No unambiguous part identity; conservative accessory support".into(),true)};
        if kind==Kind::Streamer&&!supported{return Err(invalid(format!("{} cannot use streamer motion: it needs one chain of at least two weighted bones. Choose Cloth for a branching part.",g.source_root)));}
        let strand=matches!(kind,Kind::Hair|Kind::Tail);
        if strand&&!supported {
            review=true;
            reason.push_str(if kind==Kind::Hair {"; shared hair movement and twist protection need one non-collapsed chain of at least two weighted bones"} else {"; twist protection needs one non-collapsed chain of at least two weighted bones"});
        }
        Ok(Part{root:g.source_root.clone(),kind,reason,weighted_chain:chain,chain_length_inches:length,shape_radii_inches:shape,roll_locked:kind==Kind::Streamer||(strand&&supported),review_recommended:review})
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
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
        assert!(breast[0] < 1. && breast[3] == 0. && breast[4] > 0. && breast[4] <= 0.8);
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
