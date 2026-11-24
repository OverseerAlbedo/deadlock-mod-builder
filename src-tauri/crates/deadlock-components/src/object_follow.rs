//! How the user's model follows a game object with several bones (a row's
//! `follow`): which bone each of the file's separate objects is bound to, or
//! the file's own weights. Pure and deterministic; the build, the fit view and
//! the page's suggestion all use `binding`.
//!
//! * In one piece: every point is bound wholly to one bone (the row's `bone`,
//!   else `default_bone`: the bone that carries most of the game's own mesh).
//! * By parts: each separate object of the file is bound wholly to one bone
//!   (the row's choice, else `suggest`: after the fit, each point of the part
//!   votes for the bone carrying the game's nearest point; most votes wins).
//! * Same skeleton: the file's own weights, limited to the game object's bones;
//!   refused when the file has no such armature, when a weight names a bone the
//!   object lacks, or when a point follows no bone.
//!
//! Only bones that carry the game's mesh may be followed in the first two ways.
use deadlock_project::ability_objects::{Follow, FollowWay};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const NO_CARRIERS: &str = "The game's object has no bone that carries its mesh; it cannot be followed.";
pub const SAME_SKELETON_NONE: &str = "Your file has no armature, so it cannot use the game object's skeleton. Choose In one piece or By parts, or give your file an armature named as the object's bones.";
pub const SAME_SKELETON_NAMES: &str = "Your file's armature has no bone named as the game object's bones";
pub const SAME_SKELETON_OTHER_BONE: &str = "Your file weights a part to a bone the game's object does not have";
pub const SAME_SKELETON_UNWEIGHTED: &str = "Part of your file follows no bone; weight every point to one of the game object's bones";
pub const NOT_A_CARRIER: &str = "Your model can follow only a bone that carries part of the game object's mesh";
pub const SAME_SKELETON_CLOSED: &str = "Using your file's own skeleton is not offered yet; choose In one piece or By parts";

/// A bone carrying part of the game object's mesh: how many of its points it
/// carries most, and their centre (model space).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Carrier {
    pub name: String,
    pub points: usize,
    pub centre: [f64; 3],
    /// Its share of the game's points (0–1).
    pub share: f64,
}
/// The carriers from the game's points and their strongest bone, in `order` (the skeleton's order).
pub fn carriers<'a>(points: impl IntoIterator<Item = ([f64; 3], &'a str)>, order: &[String]) -> Vec<Carrier> {
    let mut sums: BTreeMap<&str, ([f64; 3], usize)> = BTreeMap::new();
    let mut total = 0usize;
    for (p, bone) in points {
        let s = sums.entry(bone).or_insert(([0.; 3], 0));
        for k in 0..3 {
            s.0[k] += p[k];
        }
        s.1 += 1;
        total += 1;
    }
    order
        .iter()
        .filter_map(|name| {
            let (sum, n) = sums.get(name.as_str())?;
            Some(Carrier { name: name.clone(), points: *n, centre: sum.map(|v| v / *n as f64), share: *n as f64 / total.max(1) as f64 })
        })
        .collect()
}
/// The bone carrying most of the game's points (the first in skeleton order on a tie).
pub fn default_bone(carriers: &[Carrier]) -> Option<&str> {
    carriers.iter().fold(None::<&Carrier>, |best, c| if best.is_none_or(|b| c.points > b.points) { Some(c) } else { best }).map(|c| c.name.as_str())
}
/// A triangle of the game's mesh: each corner with the bone that weighs it most.
pub type Triangle = [([f64; 3], String); 3];
/// Samples per longest side of the game's mesh on its triangles, and the most
/// samples in all (the spacing widens until they fit).
pub const SURFACE_STEPS: f64 = 200.;
pub const MAX_SURFACE: usize = 400_000;

/// The game's surface for the vote: its points, and points spread over its
/// triangles (about `SURFACE_STEPS` per longest side of the mesh), each taking
/// the bone of its triangle's nearest corner (the first corner on a tie). A
/// long polygon has points only at its ends; without the spread a part beside
/// its middle would vote for whatever lies beyond those ends.
///
/// The spacing widens until the count fits `MAX_SURFACE`. The count gives each
/// triangle at least its three corners, so a mesh of more than a third of
/// `MAX_SURFACE` triangles never fits; widening stops once no triangle is split
/// any more (every side within one spacing), where the spread adds nothing and
/// the surface is the points alone. Every mesh whose count fits gets exactly the
/// spacing it got before that stop existed.
pub fn surface(points: &[([f64; 3], String)], triangles: &[Triangle]) -> Vec<([f64; 3], String)> {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for (p, _) in points.iter().chain(triangles.iter().flatten()) {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let longest = (0..3).map(|k| hi[k] - lo[k]).fold(0., f64::max);
    let mut out = points.to_vec();
    if !(longest > 0.) {
        return out;
    }
    let edge = |a: [f64; 3], b: [f64; 3]| (0..3).map(|k| (a[k] - b[k]).powi(2)).sum::<f64>().sqrt();
    let steps = |t: &Triangle, spacing: f64| (edge(t[0].0, t[1].0).max(edge(t[1].0, t[2].0)).max(edge(t[2].0, t[0].0)) / spacing).ceil().clamp(1., 4096.) as usize;
    let mut spacing = longest / SURFACE_STEPS;
    loop {
        let count: usize = triangles.iter().map(|t| steps(t, spacing)).map(|n| (n + 1) * (n + 2) / 2).sum();
        if count <= MAX_SURFACE || triangles.iter().all(|t| steps(t, spacing) == 1) {
            break;
        }
        spacing *= (count as f64 / MAX_SURFACE as f64).sqrt() * 1.01;
    }
    for t in triangles {
        let n = steps(t, spacing);
        for i in 0..=n {
            for j in 0..=n - i {
                let k = n - i - j;
                if i == n || j == n || k == n {
                    continue;
                }
                let w = [i, j, k].map(|v| v as f64 / n as f64);
                let p = [0, 1, 2].map(|d| w[0] * t[0].0[d] + w[1] * t[1].0[d] + w[2] * t[2].0[d]);
                let corner = (0..3).fold(0, |best, c| if [i, j, k][c] > [i, j, k][best] { c } else { best });
                out.push((p, t[corner].1.clone()));
            }
        }
    }
    out
}

/// Most points of one part that vote (a fixed stride through its points, so a
/// large file stays quick and the answer stays the same every time).
pub const MAX_VOTERS: usize = 4096;

/// The game's points in a uniform grid, for the nearest one to a point.
struct Grid<'a> {
    points: &'a [[f64; 3]],
    min: [f64; 3],
    cell: f64,
    dims: [i64; 3],
    cells: std::collections::HashMap<[i64; 3], Vec<usize>>,
}
impl<'a> Grid<'a> {
    fn new(points: &'a [[f64; 3]]) -> Self {
        let mut min = [f64::INFINITY; 3];
        let mut max = [f64::NEG_INFINITY; 3];
        for p in points {
            for k in 0..3 {
                min[k] = min[k].min(p[k]);
                max[k] = max[k].max(p[k]);
            }
        }
        let extent: Vec<f64> = (0..3).map(|k| (max[k] - min[k]).max(0.)).collect();
        let longest = extent.iter().copied().fold(0., f64::max);
        // About two points a cell on a filled volume; never finer than 1/128 of the longest side.
        let volume = extent.iter().map(|e| e.max(longest / 128.).max(1e-9)).product::<f64>();
        let cell = (2. * volume / points.len().max(1) as f64).cbrt().max(longest / 128.).max(1e-6);
        let dims = [0, 1, 2].map(|k| (extent[k] / cell).floor() as i64 + 1);
        let mut cells: std::collections::HashMap<[i64; 3], Vec<usize>> = std::collections::HashMap::new();
        let mut grid = Grid { points, min, cell, dims, cells: std::collections::HashMap::new() };
        for (i, p) in points.iter().enumerate() {
            cells.entry(grid.at(*p)).or_default().push(i);
        }
        grid.cells = cells;
        grid
    }
    fn at(&self, p: [f64; 3]) -> [i64; 3] {
        [0, 1, 2].map(|k| ((p[k] - self.min[k]) / self.cell).floor() as i64)
    }
    /// The index of the game point nearest `q` (the lowest index on a tie).
    fn nearest(&self, q: [f64; 3]) -> Option<usize> {
        if self.points.is_empty() {
            return None;
        }
        let c = self.at(q);
        let far = |i: usize| (0..3).map(|k| (self.points[i][k] - q[k]).powi(2)).sum::<f64>();
        // Rings of cells around q's cell, out to the far side of the grid.
        let reach = (0..3).map(|k| c[k].abs().max((self.dims[k] - 1 - c[k]).abs())).max().unwrap_or(0);
        let mut best: Option<(f64, usize)> = None;
        for r in 0..=reach {
            let span = |k: usize| (-r).max(-c[k])..=r.min(self.dims[k] - 1 - c[k]);
            for dx in span(0) {
                for dy in span(1) {
                    for dz in span(2) {
                        if dx.abs().max(dy.abs()).max(dz.abs()) != r {
                            continue;
                        }
                        for &i in self.cells.get(&[c[0] + dx, c[1] + dy, c[2] + dz]).into_iter().flatten() {
                            let d = far(i);
                            if best.is_none_or(|(bd, bi)| d < bd || (d == bd && i < bi)) {
                                best = Some((d, i));
                            }
                        }
                    }
                }
            }
            // Every point of a further ring is at least r cells away.
            if best.is_some_and(|(d, _)| d <= (r as f64 * self.cell).powi(2)) {
                break;
            }
        }
        best.map(|(_, i)| i)
    }
}

/// For each query, the index of the nearest of `points` (the lowest index on a
/// tie); `None` when `points` is empty.
pub fn nearest(points: &[[f64; 3]], queries: &[[f64; 3]]) -> Vec<Option<usize>> {
    let grid = Grid::new(points);
    queries.iter().map(|q| grid.nearest(*q)).collect()
}

/// The suggested bone of each part, by a vote of its points: each point
/// (after the fit; at most `MAX_VOTERS` of them, by a fixed stride) votes for
/// the bone that carries the game's point nearest to it; the part takes the
/// carrier with the most votes, the first in skeleton order on a tie. A part
/// without points takes `default_bone`. `game`: every point of the game's mesh
/// with the bone that weighs it most.
pub fn suggest(parts: &[(String, &[[f64; 3]])], game: &[([f64; 3], String)], carriers: &[Carrier]) -> BTreeMap<String, String> {
    let positions: Vec<[f64; 3]> = game.iter().map(|(p, _)| *p).collect();
    let grid = Grid::new(&positions);
    let first = default_bone(carriers);
    parts
        .iter()
        .filter_map(|(name, points)| {
            let mut votes = vec![0usize; carriers.len()];
            let stride = points.len().div_ceil(MAX_VOTERS).max(1);
            for p in points.iter().step_by(stride) {
                let Some(i) = grid.nearest(*p) else { continue };
                if let Some(k) = carriers.iter().position(|c| c.name == game[i].1) {
                    votes[k] += 1;
                }
            }
            let best = (0..carriers.len()).filter(|&k| votes[k] > 0).fold(None::<usize>, |best, k| if best.is_none_or(|b| votes[k] > votes[b]) { Some(k) } else { best });
            let bone = best.map(|k| carriers[k].name.as_str()).or(first)?;
            Some((name.clone(), bone.to_owned()))
        })
        .collect()
}
/// The way a row without a setting follows: by parts when the file holds more than one separate object.
pub fn default_way(parts: usize) -> FollowWay {
    if parts > 1 { FollowWay::ByParts } else { FollowWay::OnePiece }
}
/// One part of the user's file: its name, points after the fit, and (same
/// skeleton) its points' weights by bone name.
#[derive(Debug, Clone)]
pub struct Part<'a> {
    pub name: &'a str,
    pub points: &'a [[f64; 3]],
    pub weights: &'a [Vec<(String, f64)>],
}
/// What each part is bound to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Binding {
    pub way: FollowWay,
    /// Part name → bone (one piece and by parts); empty for the same skeleton.
    pub parts: BTreeMap<String, String>,
    /// The engine's suggestion for each part (by parts) or the default bone (one piece).
    pub suggested: BTreeMap<String, String>,
}
/// The binding of `parts` under `follow` (absent: `default_way`). `game`: the
/// game mesh's points with their strongest bone (`suggest`); `skeleton`: every
/// bone of the object; `armature`: the file's armature bone names.
pub fn binding(follow: Option<&Follow>, parts: &[Part], game: &[([f64; 3], String)], carriers: &[Carrier], skeleton: &[String], armature: &[String], same_skeleton_open: bool) -> Result<Binding, String> {
    let way = follow.map_or_else(|| default_way(parts.len()), |f| f.way);
    let first = default_bone(carriers).ok_or_else(|| NO_CARRIERS.to_owned())?.to_owned();
    let carries = |bone: &str| carriers.iter().any(|c| c.name == bone);
    let names = || carriers.iter().map(|c| c.name.as_str()).collect::<Vec<_>>().join(", ");
    match way {
        FollowWay::OnePiece => {
            let bone = follow.and_then(|f| f.bone.clone()).unwrap_or_else(|| first.clone());
            if !carries(&bone) {
                return Err(format!("{NOT_A_CARRIER} ({}): {bone}", names()));
            }
            let suggested = parts.iter().map(|p| (p.name.to_owned(), first.clone())).collect();
            Ok(Binding { way, parts: parts.iter().map(|p| (p.name.to_owned(), bone.clone())).collect(), suggested })
        }
        FollowWay::ByParts => {
            let voters: Vec<(String, &[[f64; 3]])> = parts.iter().map(|p| (p.name.to_owned(), p.points)).collect();
            let suggested = suggest(&voters, game, carriers);
            let mut out = BTreeMap::new();
            for p in parts {
                let bone = follow.and_then(|f| f.parts.get(p.name).cloned()).unwrap_or_else(|| suggested[p.name].clone());
                if !carries(&bone) {
                    return Err(format!("{NOT_A_CARRIER} ({}): {} follows {bone}", names(), p.name));
                }
                out.insert(p.name.to_owned(), bone);
            }
            Ok(Binding { way, parts: out, suggested })
        }
        FollowWay::SameSkeleton => {
            if !same_skeleton_open {
                return Err(SAME_SKELETON_CLOSED.into());
            }
            if armature.is_empty() {
                return Err(SAME_SKELETON_NONE.into());
            }
            if !armature.iter().any(|b| skeleton.contains(b)) {
                return Err(format!("{SAME_SKELETON_NAMES} (for example {}).", skeleton.first().map_or("", String::as_str)));
            }
            for p in parts {
                for point in p.weights {
                    if let Some((bone, _)) = point.iter().find(|(b, w)| *w > 0. && !skeleton.contains(b)) {
                        return Err(format!("{SAME_SKELETON_OTHER_BONE}: {} follows {bone}.", p.name));
                    }
                }
                if p.weights.is_empty() || p.weights.iter().any(|point| !point.iter().any(|(_, w)| *w > 0.)) {
                    return Err(format!("{SAME_SKELETON_UNWEIGHTED}: {}.", p.name));
                }
            }
            Ok(Binding { way, parts: BTreeMap::new(), suggested: BTreeMap::new() })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn order() -> Vec<String> {
        ["main", "full_top", "full_bottom", "unused"].map(String::from).to_vec()
    }
    fn game() -> Vec<Carrier> {
        let pts = [([0., 0., 10.], "main"), ([0., 0., 20.], "main"), ([0., 0., 30.], "main"), ([0., 0., 40.], "full_top"), ([0., 0., 5.], "full_bottom")];
        carriers(pts, &order())
    }

    #[test]
    fn carriers_come_in_skeleton_order_with_their_share_and_centre() {
        let c = game();
        assert_eq!(c.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["main", "full_top", "full_bottom"]);
        assert_eq!((c[0].points, c[0].centre, c[0].share), (3, [0., 0., 20.], 0.6));
        assert_eq!(default_bone(&c), Some("main"));
        assert_eq!(default_bone(&[]), None);
    }

    /// A canister-like game mesh: an outer shell (radius 10, height 0–40) on
    /// `main`, liquid inside it: an upper pool on `full_top` (radius 3, height
    /// 30–40) and a lower one on `full_bottom` (radius 3, height 5–15).
    fn canister() -> Vec<([f64; 3], String)> {
        let ring = |r: f64, z0: f64, z1: f64, bone: &'static str| {
            (0..=10).flat_map(move |h| (0..24).map(move |a| {
                let t = f64::from(a) * std::f64::consts::TAU / 24.;
                ([r * t.cos(), r * t.sin(), z0 + (z1 - z0) * f64::from(h) / 10.], bone.to_owned())
            }))
        };
        ring(10., 0., 40., "main").chain(ring(3., 30., 40., "full_top")).chain(ring(3., 5., 15., "full_bottom")).collect()
    }
    fn ring_points(r: f64, z: f64, n: usize) -> Vec<[f64; 3]> {
        (0..n).map(|a| {
            let t = a as f64 * std::f64::consts::TAU / n as f64 + 0.1;
            [r * t.cos(), r * t.sin(), z]
        }).collect()
    }

    #[test]
    fn parts_are_suggested_by_a_vote_of_their_nearest_points_and_the_users_choice_wins() {
        let game = canister();
        let c = carriers(game.iter().map(|(p, b)| (*p, b.as_str())), &order());
        let column: Vec<[f64; 3]> = (0..=8).flat_map(|h| ring_points(9.5, f64::from(h) * 5., 16)).collect();
        // A collar just outside the shell at the upper pool's height: its centre
        // (0, 0, 33) is nearest the upper pool's centre, but every one of its
        // points is nearest the shell.
        let collar = ring_points(11., 33., 32);
        let bar: Vec<[f64; 3]> = (0..5).flat_map(|h| ring_points(1., 6. + 2. * f64::from(h), 8)).collect();
        let cap: Vec<[f64; 3]> = (0..4).flat_map(|h| ring_points(2., 36. + f64::from(h), 8)).collect();
        let parts = [
            Part { name: "column", points: &column, weights: &[] },
            Part { name: "collar", points: &collar, weights: &[] },
            Part { name: "bar", points: &bar, weights: &[] },
            Part { name: "cap", points: &cap, weights: &[] },
        ];
        let b = binding(None, &parts, &game, &c, &order(), &[], false).unwrap();
        assert_eq!(b.way, FollowWay::ByParts, "several parts: by parts by default");
        assert_eq!(b.parts, BTreeMap::from([("bar".into(), "full_bottom".into()), ("cap".into(), "full_top".into()), ("collar".into(), "main".into()), ("column".into(), "main".into())]));
        assert_eq!(b.suggested, b.parts);
        // The same answer every time.
        assert_eq!(binding(None, &parts, &game, &c, &order(), &[], false).unwrap(), b);
        let chosen = Follow { way: FollowWay::ByParts, bone: None, parts: BTreeMap::from([("bar".into(), "main".into())]) };
        assert_eq!(binding(Some(&chosen), &parts, &game, &c, &order(), &[], false).unwrap().parts["bar"], "main");
        let wrong = Follow { parts: BTreeMap::from([("bar".into(), "unused".into())]), ..chosen.clone() };
        assert!(binding(Some(&wrong), &parts, &game, &c, &order(), &[], false).unwrap_err().starts_with(NOT_A_CARRIER));
        // One part: in one piece, on the bone carrying most.
        let one = binding(None, &parts[..1], &game, &c, &order(), &[], false).unwrap();
        assert_eq!((one.way, one.parts["column"].as_str()), (FollowWay::OnePiece, "main"));
        let top = Follow { way: FollowWay::OnePiece, bone: Some("full_top".into()), parts: BTreeMap::new() };
        assert!(binding(Some(&top), &parts, &game, &c, &order(), &[], false).unwrap().parts.values().all(|b| b == "full_top"));
    }

    #[test]
    fn ties_go_to_skeleton_order_and_far_points_still_vote() {
        let game: Vec<([f64; 3], String)> = vec![([0., 0., 0.], "full_top".into()), ([10., 0., 0.], "main".into()), ([0., 0., 0.], "main".into())];
        let c = carriers(game.iter().map(|(p, b)| (*p, b.as_str())), &order());
        // One vote each: the first carrier in skeleton order (main) wins.
        let split = [[9., 0., 0.], [1., 0., 0.]];
        assert_eq!(suggest(&[("x".into(), &split[..])], &game, &c)["x"], "main");
        // Two game points at the same place: the first one in the mesh decides (full_top).
        let near = [[0.5, 0., 0.]];
        assert_eq!(suggest(&[("x".into(), &near[..])], &game, &c)["x"], "full_top");
        // A part far outside the game's mesh still finds its nearest point.
        let far = [[500., 300., -200.]];
        assert_eq!(suggest(&[("x".into(), &far[..])], &game, &c)["x"], "main");
        // A part without points takes the bone carrying most.
        assert_eq!(suggest(&[("x".into(), &[][..])], &game, &c)["x"], "main");
    }

    #[test]
    fn a_long_polygon_votes_along_its_middle() {
        // A tall wall on main (one quad, points only at its ends, height 0–40)
        // and a small pool on full_bottom at height 20, 8 in inside the wall.
        let at = |p: [f64; 3], b: &str| (p, b.to_owned());
        let wall = [at([10., -5., 0.], "main"), at([10., 5., 0.], "main"), at([10., 5., 40.], "main"), at([10., -5., 40.], "main")];
        let pool = at([2., 0., 20.], "full_bottom");
        let points: Vec<([f64; 3], String)> = wall.iter().cloned().chain([pool.clone()]).collect();
        let triangles: Vec<Triangle> = vec![[wall[0].clone(), wall[1].clone(), wall[2].clone()], [wall[0].clone(), wall[2].clone(), wall[3].clone()]];
        let c = carriers(points.iter().map(|(p, b)| (*p, b.as_str())), &order());
        // A collar point just outside the wall's middle.
        let collar = [[12., 0., 20.]];
        assert_eq!(suggest(&[("collar".into(), &collar[..])], &points, &c)["collar"], "full_bottom", "the wall's ends are far: points alone pick the pool");
        let game = surface(&points, &triangles);
        assert_eq!(suggest(&[("collar".into(), &collar[..])], &game, &c)["collar"], "main");
        // The points come first, unchanged; every sample lies on the wall and is the wall's.
        assert_eq!(game[..points.len()], points[..]);
        assert!(game[points.len()..].iter().all(|(p, b)| (p[0] - 10.).abs() < 1e-9 && b == "main"));
        // The mesh is 40 high: steps of 0.2 in; each triangle's longest side (the
        // diagonal, 41.23) takes 207 of them, every corner left out.
        assert_eq!(game.len() - points.len(), 2 * ((208 * 209) / 2 - 3));
        assert_eq!(surface(&points, &triangles), game);
    }

    #[test]
    fn a_mesh_with_more_triangles_than_the_spread_can_count_is_its_points_alone() {
        // Every triangle counts at least its three corners, so 134,000 triangles
        // (402,000 > MAX_SURFACE) never fit, however wide the spacing: the
        // widening stops once no triangle is split, and nothing is spread.
        let at = |p: [f64; 3], b: &str| (p, b.to_owned());
        let corners = [at([0., 0., 0.], "main"), at([40., 0., 0.], "main"), at([0., 0., 40.], "full_top")];
        let many: Vec<Triangle> = vec![corners.clone(); MAX_SURFACE / 3 + 667];
        assert!(3 * many.len() > MAX_SURFACE);
        assert_eq!(surface(&corners, &many), corners.to_vec());
        // A large mesh of small triangles stops at once: its points alone.
        let strip: Vec<Triangle> = (0..140_000).map(|i| {
            let x = f64::from(i);
            [at([x, 0., 0.], "main"), at([x + 1., 0., 0.], "main"), at([x, 0., 1.], "main")]
        }).collect();
        let points: Vec<([f64; 3], String)> = strip.iter().map(|t| t[0].clone()).collect();
        assert_eq!(surface(&points, &strip), points);
        // One triangle fewer than the limit's third still fits at one step each: unchanged.
        let fits: Vec<Triangle> = vec![corners.clone(); MAX_SURFACE / 3];
        assert_eq!(surface(&corners, &fits), corners.to_vec());
    }

    #[test]
    fn a_large_part_votes_by_a_fixed_stride() {
        let game = canister();
        let c = carriers(game.iter().map(|(p, b)| (*p, b.as_str())), &order());
        // 20,000 points: 3/4 of them in the lower pool, the rest on the shell.
        let points: Vec<[f64; 3]> = (0..20_000).map(|i| if i % 4 == 3 { [9.9, 0., 20.] } else { [0.5, 0.5, 10.] }).collect();
        let first = suggest(&[("x".into(), &points[..])], &game, &c);
        assert_eq!(first["x"], "full_bottom");
        assert_eq!(suggest(&[("x".into(), &points[..])], &game, &c), first);
    }

    #[test]
    fn the_same_skeleton_way_takes_the_files_weights_or_says_why_not() {
        let c = game();
        let same = Follow { way: FollowWay::SameSkeleton, bone: None, parts: BTreeMap::new() };
        let good = vec![vec![("main".to_owned(), 1.)], vec![("full_bottom".to_owned(), 0.5), ("main".to_owned(), 0.5)]];
        let part = |w| [Part { name: "body", points: &[], weights: w }];
        assert_eq!(binding(Some(&same), &part(&good), &[], &c, &order(), &order(), false).unwrap_err(), SAME_SKELETON_CLOSED);
        assert!(binding(Some(&same), &part(&good), &[], &c, &order(), &order(), true).unwrap().parts.is_empty());
        assert_eq!(binding(Some(&same), &part(&good), &[], &c, &order(), &[], true).unwrap_err(), SAME_SKELETON_NONE);
        assert!(binding(Some(&same), &part(&good), &[], &c, &order(), &["Bone".into()], true).unwrap_err().contains("for example main"));
        let other = vec![vec![("tail".to_owned(), 1.)]];
        assert_eq!(binding(Some(&same), &part(&other), &[], &c, &order(), &order(), true).unwrap_err(), format!("{SAME_SKELETON_OTHER_BONE}: body follows tail."));
        let loose = vec![vec![("main".to_owned(), 1.)], vec![]];
        assert!(binding(Some(&same), &part(&loose), &[], &c, &order(), &order(), true).unwrap_err().starts_with(SAME_SKELETON_UNWEIGHTED));
    }
}
