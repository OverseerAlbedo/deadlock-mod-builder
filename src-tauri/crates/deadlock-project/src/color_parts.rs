//! "This part only": which image pixels one part of a model covers, for a color
//! range with a `part` spot (`color_ranges::ColorRange::part`).
//!
//! A part is one connected piece of the meshes that use a material: triangles
//! joined through shared corners, corners welded by position (rounded to
//! 1/`WELD` of a model unit, so the copies of a corner at a texture seam are one
//! corner). The pieces at the spot are those with a triangle whose texture
//! coordinates cover it (within `SPOT_REACH`, texture coordinates repeat);
//! several pieces that share that image area are all chosen, since they show
//! the same pixels. A range then changes only the pixels whose centre lies
//! within `MASK_REACH` pixels of one of the chosen pieces' triangles, at the
//! size of the image being changed (every mip of a game texture at its own size,
//! wrapped like the texture).
//!
//! The same rule is `src/extras/colorParts.ts` (the Colors preview). Both use
//! only + − × ÷, square roots, floor and comparisons on f64 values widened from
//! the meshes' f32 values, in the same order, so they agree bit for bit; both
//! tests check `deadlock-components/tests/fixtures/color-parts.json`.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};

/// Corners closer than about 1/WELD model units are one corner.
pub const WELD: f64 = 200.;
/// How far (texture coordinates) a spot may lie outside a triangle and still be on it.
pub const SPOT_REACH: f64 = 1. / 1024.;
/// How far (pixels) beyond a triangle's edge a pixel centre still belongs to its area.
pub const MASK_REACH: f64 = 1.;
/// A triangle reaching over more image repeats than this is cut to this many.
pub const MAX_REPEATS: i64 = 4;

/// The build's refusal when a part range's spot is on no part any more.
pub const PART_LOST: &str = "The spot this color range was picked on is no longer on a part of the model (the model changed). Pick the part again, or set the range to Everywhere this color.";
/// The build's refusal when the model's parts cannot be read for a part range.
pub const NO_PARTS: &str = "The parts of the model could not be read for a color range that changes one part only.";

/// The triangles of one mesh that uses a material: positions (x, y, z per
/// corner), texture coordinates (u, v per corner) and corner indices.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PartMesh {
    pub positions: Vec<f32>,
    pub uvs: Vec<f32>,
    pub indices: Vec<u32>,
}

/// Where the build gets the meshes of a material: the game hero's own model
/// (decompiled) or the prepared replacement model.
pub trait PartMeshes {
    /// The meshes that use `material` (a `.vmat` path, compared ignoring case).
    fn part_meshes(&self, material: &str) -> std::result::Result<Vec<PartMesh>, String>;
}

/// One triangle's texture coordinates: u0, v0, u1, v1, u2, v2.
pub type UvTriangle = [f32; 6];

/// The piece of every triangle of `meshes` (one list per mesh; a triangle with
/// a corner index out of range is in no piece, `u32::MAX`). Pieces are numbered
/// by their first triangle in mesh order.
pub fn pieces(meshes: &[PartMesh]) -> Vec<Vec<u32>> {
    let mut welded: HashMap<(i64, i64, i64), usize> = HashMap::new();
    let mut corner_ids: Vec<Vec<usize>> = Vec::with_capacity(meshes.len());
    for mesh in meshes {
        let ids = mesh
            .positions
            .chunks_exact(3)
            .map(|p| {
                let key = |x: f32| (f64::from(x) * WELD + 0.5).floor() as i64;
                let next = welded.len();
                *welded.entry((key(p[0]), key(p[1]), key(p[2]))).or_insert(next)
            })
            .collect();
        corner_ids.push(ids);
    }
    let mut parent: Vec<usize> = (0..welded.len()).collect();
    fn find(parent: &mut [usize], mut a: usize) -> usize {
        while parent[a] != a {
            parent[a] = parent[parent[a]];
            a = parent[a];
        }
        a
    }
    let corners = |mesh: &PartMesh, ids: &[usize], t: usize| -> Option<[usize; 3]> {
        let at = |k: usize| mesh.indices.get(3 * t + k).and_then(|i| ids.get(*i as usize)).copied();
        Some([at(0)?, at(1)?, at(2)?])
    };
    for (mesh, ids) in meshes.iter().zip(&corner_ids) {
        for t in 0..mesh.indices.len() / 3 {
            if let Some([a, b, c]) = corners(mesh, ids, t) {
                let (ra, rb, rc) = (find(&mut parent, a), find(&mut parent, b), find(&mut parent, c));
                parent[rb] = ra;
                let rc = find(&mut parent, rc);
                let ra = find(&mut parent, ra);
                parent[rc] = ra;
            }
        }
    }
    let mut numbers: HashMap<usize, u32> = HashMap::new();
    let mut out = Vec::with_capacity(meshes.len());
    for (mesh, ids) in meshes.iter().zip(&corner_ids) {
        let mut list = Vec::with_capacity(mesh.indices.len() / 3);
        for t in 0..mesh.indices.len() / 3 {
            list.push(match corners(mesh, ids, t) {
                Some([a, _, _]) => {
                    let root = find(&mut parent, a);
                    let next = numbers.len() as u32;
                    *numbers.entry(root).or_insert(next)
                }
                None => u32::MAX,
            });
        }
        out.push(list);
    }
    out
}

fn uv_triangle(mesh: &PartMesh, t: usize) -> Option<UvTriangle> {
    let corner = |k: usize| -> Option<(f32, f32)> {
        let i = *mesh.indices.get(3 * t + k)? as usize;
        Some((*mesh.uvs.get(2 * i)?, *mesh.uvs.get(2 * i + 1)?))
    };
    let (a, b, c) = (corner(0)?, corner(1)?, corner(2)?);
    Some([a.0, a.1, b.0, b.1, c.0, c.1])
}

/// Whether (px, py) lies inside the triangle (ax, ay), (bx, by), (cx, cy) or
/// within `reach` of one of its edges (distance to the nearest point of the
/// edge, the triangle's own units). A triangle without area holds nothing.
pub fn inside(t: [f64; 6], px: f64, py: f64, reach: f64) -> bool {
    let [ax, ay, bx, by, cx, cy] = t;
    let area = (bx - ax) * (cy - ay) - (by - ay) * (cx - ax);
    if area == 0. || !area.is_finite() {
        return false;
    }
    let sign = if area > 0. { 1. } else { -1. };
    let edges = [(ax, ay, bx, by), (bx, by, cx, cy), (cx, cy, ax, ay)];
    if edges.iter().all(|&(x0, y0, x1, y1)| ((x1 - x0) * (py - y0) - (y1 - y0) * (px - x0)) * sign >= 0.) {
        return true;
    }
    if reach <= 0. {
        return false;
    }
    edges.iter().any(|&(x0, y0, x1, y1)| {
        let dx = x1 - x0;
        let dy = y1 - y0;
        let length = dx * dx + dy * dy;
        if length == 0. {
            return false;
        }
        let along = (((px - x0) * dx + (py - y0) * dy) / length).clamp(0., 1.);
        let ex = x0 + along * dx - px;
        let ey = y0 + along * dy - py;
        ex * ex + ey * ey <= reach * reach
    })
}

fn widened(t: UvTriangle) -> [f64; 6] {
    t.map(f64::from)
}

/// The pieces (numbers from `pieces`) with a triangle on the spot (u, v).
pub fn at_spot(meshes: &[PartMesh], pieces: &[Vec<u32>], u: f64, v: f64) -> BTreeSet<u32> {
    let pu = u - u.floor();
    let pv = v - v.floor();
    let mut found = BTreeSet::new();
    for (mesh, numbers) in meshes.iter().zip(pieces) {
        for (t, piece) in numbers.iter().enumerate() {
            if *piece == u32::MAX || found.contains(piece) {
                continue;
            }
            let Some(triangle) = uv_triangle(mesh, t) else { continue };
            let w = widened(triangle);
            let (lo_u, hi_u) = (w[0].min(w[2]).min(w[4]), w[0].max(w[2]).max(w[4]));
            let (lo_v, hi_v) = (w[1].min(w[3]).min(w[5]), w[1].max(w[3]).max(w[5]));
            if !(lo_u.is_finite() && hi_u.is_finite() && lo_v.is_finite() && hi_v.is_finite()) {
                continue;
            }
            let (first_u, last_u) = ((lo_u - SPOT_REACH).floor() as i64, ((hi_u + SPOT_REACH).floor() as i64).min((lo_u - SPOT_REACH).floor() as i64 + MAX_REPEATS));
            let (first_v, last_v) = ((lo_v - SPOT_REACH).floor() as i64, ((hi_v + SPOT_REACH).floor() as i64).min((lo_v - SPOT_REACH).floor() as i64 + MAX_REPEATS));
            'tiles: for ku in first_u..=last_u {
                for kv in first_v..=last_v {
                    if inside(w, pu + ku as f64, pv + kv as f64, SPOT_REACH) {
                        found.insert(*piece);
                        break 'tiles;
                    }
                }
            }
        }
    }
    found
}

/// The texture coordinates of every triangle of the chosen pieces, in mesh order.
pub fn area(meshes: &[PartMesh], pieces: &[Vec<u32>], chosen: &BTreeSet<u32>) -> Vec<UvTriangle> {
    let mut out = vec![];
    for (mesh, numbers) in meshes.iter().zip(pieces) {
        for (t, piece) in numbers.iter().enumerate() {
            if chosen.contains(piece) {
                if let Some(triangle) = uv_triangle(mesh, t) {
                    out.push(triangle);
                }
            }
        }
    }
    out
}

/// The image area of a part at `width` × `height` pixels: 1 for a pixel the
/// range may change, 0 elsewhere (rows top first: v = 0 is the first row).
pub fn mask(triangles: &[UvTriangle], width: usize, height: usize) -> Vec<u8> {
    let mut out = vec![0u8; width * height];
    if width == 0 || height == 0 {
        return out;
    }
    let (w, h) = (width as f64, height as f64);
    for triangle in triangles {
        let t = widened(*triangle);
        let p = [t[0] * w, t[1] * h, t[2] * w, t[3] * h, t[4] * w, t[5] * h];
        if p.iter().any(|v| !v.is_finite()) {
            continue;
        }
        let x0 = (p[0].min(p[2]).min(p[4]) - MASK_REACH).floor() as i64;
        let x1 = ((p[0].max(p[2]).max(p[4]) + MASK_REACH).floor() as i64).min(x0 + MAX_REPEATS * width as i64);
        let y0 = (p[1].min(p[3]).min(p[5]) - MASK_REACH).floor() as i64;
        let y1 = ((p[1].max(p[3]).max(p[5]) + MASK_REACH).floor() as i64).min(y0 + MAX_REPEATS * height as i64);
        for y in y0..=y1 {
            let row = y.rem_euclid(height as i64) as usize * width;
            for x in x0..=x1 {
                if inside(p, x as f64 + 0.5, y as f64 + 0.5, MASK_REACH) {
                    out[row + x.rem_euclid(width as i64) as usize] = 1;
                }
            }
        }
    }
    out
}

/// The image area one part range reaches, kept in a build receipt so the
/// build's checks can make the same mask again: the triangles of the chosen
/// pieces and how many pieces there were.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PartArea {
    /// The range's place among the enabled ranges that change something.
    pub range: usize,
    pub pieces: usize,
    pub triangles: Vec<UvTriangle>,
}

/// The area of every part range of `ranges` on `meshes`; `PART_LOST` when a
/// spot is on no piece.
pub fn areas(ranges: &[crate::color_ranges::ColorRange], meshes: &[PartMesh]) -> std::result::Result<Vec<PartArea>, String> {
    if !ranges.iter().any(|r| r.part.is_some()) {
        return Ok(vec![]);
    }
    let numbers = pieces(meshes);
    let mut out = vec![];
    for (index, range) in ranges.iter().enumerate() {
        let Some(spot) = range.part else { continue };
        let chosen = at_spot(meshes, &numbers, spot.u, spot.v);
        if chosen.is_empty() {
            return Err(format!("Color range {}: {PART_LOST}", index + 1));
        }
        out.push(PartArea { range: index, pieces: chosen.len(), triangles: area(meshes, &numbers, &chosen) });
    }
    Ok(out)
}

/// Every range's mask at one size (`None`: the range reaches everywhere).
pub fn masks(ranges: usize, areas: &[PartArea], width: usize, height: usize) -> Vec<Option<Vec<u8>>> {
    let mut out = vec![None; ranges];
    for part in areas {
        if let Some(slot) = out.get_mut(part.range) {
            *slot = Some(mask(&part.triangles, width, height));
        }
    }
    out
}
