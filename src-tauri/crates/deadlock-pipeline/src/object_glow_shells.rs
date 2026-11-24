//! The mesh files of an object with body-group shells
//! (`deadlock_components::object_glow_shells`, `Switches::body_group_shells`): in
//! each level of detail the user's mesh in place of the game's object, and in place
//! of the game's shell a shell generated from the user's mesh the way the game's is
//! made from its object: pushed out by the game shell's own distance, faces and
//! normals turned as the game's are, the game's vertex colour and its texture
//! projection, drawn with the game's own shell material (by reference). Every mesh
//! node keeps its name, so the body group's choices show what they showed.
use crate::ability_objects::{body_dmx, dmx_points, shell_meshes, shell_style, CarriedFiles, MeshNode, Original, ShellStyle};
use crate::invalid;
use deadlock_components::object_glow_shells::GlowShells;
use deadlock_core::{safety, Result};
use deadlock_skeleton::export::PreparedMesh;
use serde::{Deserialize, Serialize};

/// How the game's shell is made from its object, read from their decompiled meshes.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GlowStyle {
    /// Distance (the median, over the shell's corners, of the way to the object's
    /// nearest corner, inches) and how faces and normals are turned.
    pub shell: ShellStyle,
    /// The mean of the game shell's vertex colours (RGBA), when it has any.
    pub color: Option<[f64; 4]>,
    /// The game shell's first texture coordinates as a projection over its own
    /// bounds: `t = c[0] + c[1]·x + c[2]·y + c[3]·z`, with x, y, z each from 0 to 1
    /// across the bounds; u and v as the game's file stores them. `None` when the
    /// game's shell has no texture coordinates.
    pub projection: Option<[[f64; 4]; 2]>,
}

/// Every corner of a DMX's vertex data: position, and the colour and first texture
/// coordinates when it has them (corner order of each stream's own index list).
struct Corners {
    points: Vec<[f64; 3]>,
    colors: Vec<[f64; 4]>,
    uvs: Vec<[f64; 2]>,
}
fn floats(dmx: &deadlock_dmx::Dmx, value: Option<&deadlock_dmx::Value>) -> Vec<f64> {
    value.and_then(|v| dmx.value_bytes(v)).map(|b| b.chunks_exact(4).map(|x| f64::from(f32::from_le_bytes(x.try_into().unwrap()))).collect()).unwrap_or_default()
}
fn ints(dmx: &deadlock_dmx::Dmx, value: Option<&deadlock_dmx::Value>) -> Vec<i32> {
    value.and_then(|v| dmx.integer_values(v).ok()).unwrap_or_default()
}
fn corners(dmx: &deadlock_dmx::Dmx) -> Result<Corners> {
    let mut out = Corners { points: vec![], colors: vec![], uvs: vec![] };
    for state in dmx.elements.iter().filter(|e| e.kind == "DmeVertexData" && e.get("position$0").is_some()) {
        let stream = |name: &str, width: usize| -> Result<Vec<Vec<f64>>> {
            let values = floats(dmx, state.get(name));
            ints(dmx, state.get(&format!("{name}Indices")))
                .into_iter()
                .map(|i| usize::try_from(i).ok().and_then(|i| values.get(i * width..i * width + width)).map(<[f64]>::to_vec).ok_or_else(|| invalid(format!("A game mesh's {name} points outside its values."))))
                .collect()
        };
        let points = stream("position$0", 3)?;
        let colors = stream("color$0", 4)?;
        let uvs = stream("texcoord$0", 2)?;
        // The colour and texture streams of the game's shell index every corner.
        if !colors.is_empty() && colors.len() != points.len() || !uvs.is_empty() && uvs.len() != points.len() {
            return Err(invalid("The game's shell holds colours or texture coordinates for some of its corners only."));
        }
        out.points.extend(points.iter().map(|p| [p[0], p[1], p[2]]));
        out.colors.extend(colors.iter().map(|c| [c[0], c[1], c[2], c[3]]));
        out.uvs.extend(uvs.iter().map(|t| [t[0], t[1]]));
    }
    Ok(out)
}
/// The corners' bounds (low, size).
fn bounds(points: &[[f64; 3]]) -> ([f64; 3], [f64; 3]) {
    let mut lo = [f64::MAX; 3];
    let mut hi = [f64::MIN; 3];
    for p in points {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    (lo, [0, 1, 2].map(|k| hi[k] - lo[k]))
}
fn unit(p: [f64; 3], (lo, size): ([f64; 3], [f64; 3])) -> [f64; 3] {
    [0, 1, 2].map(|k| if size[k] > 1e-12 { (p[k] - lo[k]) / size[k] } else { 0. })
}
/// Least squares of `t ≈ c·(1, x, y, z)`; `None` when the system is singular.
fn fit(rows: &[([f64; 3], f64)]) -> Option<[f64; 4]> {
    let mut a = [[0.; 5]; 4];
    for (p, t) in rows {
        let x = [1., p[0], p[1], p[2]];
        for i in 0..4 {
            for j in 0..4 {
                a[i][j] += x[i] * x[j];
            }
            a[i][4] += x[i] * t;
        }
    }
    for col in 0..4 {
        let pivot = (col..4).max_by(|i, j| a[*i][col].abs().total_cmp(&a[*j][col].abs()))?;
        if a[pivot][col].abs() < 1e-9 {
            return None;
        }
        a.swap(col, pivot);
        for row in 0..4 {
            if row != col {
                let f = a[row][col] / a[col][col];
                for k in col..5 {
                    a[row][k] -= f * a[col][k];
                }
            }
        }
    }
    Some([0, 1, 2, 3].map(|i| a[i][4] / a[i][i]))
}
/// How far from a projection the game's texture coordinates may be (in texture
/// units) for the generated shell to take the same projection.
pub const PROJECTION_TOLERANCE: f64 = 1e-3;
/// At most this many of the shell's corners are measured for its distance (every k-th).
const DISTANCE_SAMPLES: usize = 4096;

/// The style of the game's shell, from the decompiled DMX of a level's object and of its shell.
pub fn glow_style(body: &deadlock_dmx::Dmx, shell: &deadlock_dmx::Dmx) -> Result<GlowStyle> {
    let (object, own) = (dmx_points(body)?, corners(shell)?);
    if object.is_empty() || own.points.is_empty() {
        return Err(invalid("The game's shell or its object has no corners."));
    }
    let step = own.points.len().div_ceil(DISTANCE_SAMPLES).max(1);
    let mut gaps: Vec<f64> = own
        .points
        .iter()
        .step_by(step)
        .map(|p| object.iter().map(|q| (0..3).map(|k| (p[k] - q[k]).powi(2)).sum::<f64>()).fold(f64::MAX, f64::min).sqrt())
        .collect();
    gaps.sort_by(f64::total_cmp);
    let distance = gaps[gaps.len() / 2];
    if !(distance.is_finite() && distance > 0.) {
        return Err(invalid("The game's shell stands out from its object by no distance."));
    }
    let color = (!own.colors.is_empty()).then(|| {
        let n = own.colors.len() as f64;
        [0, 1, 2, 3].map(|k| own.colors.iter().map(|c| c[k]).sum::<f64>() / n)
    });
    let projection = if own.uvs.is_empty() {
        None
    } else {
        let frame = bounds(&own.points);
        let mut both = [[0.; 4]; 2];
        for (t, slot) in both.iter_mut().enumerate() {
            let rows: Vec<([f64; 3], f64)> = own.points.iter().zip(&own.uvs).map(|(p, uv)| (unit(*p, frame), uv[t])).collect();
            let c = fit(&rows).ok_or_else(|| invalid("The game's shell's texture coordinates cannot be read as a projection."))?;
            let worst = rows.iter().map(|(p, v)| (c[0] + c[1] * p[0] + c[2] * p[1] + c[3] * p[2] - v).abs()).fold(0., f64::max);
            if worst > PROJECTION_TOLERANCE {
                return Err(invalid(format!("The game's shell's texture coordinates are not a projection over its bounds (off by up to {worst:.4}); the shell cannot be made from your object.")));
            }
            *slot = c;
        }
        Some(both)
    };
    Ok(GlowStyle { shell: shell_style(body, shell, distance)?, color, projection })
}

/// The generated shells: `shell_meshes` (pushed out, turned, drawn with `material`),
/// then the game's vertex colour on every corner and the game's projection over the
/// generated shells' own bounds in place of the user's texture coordinates (v written
/// for this DMX writer, which flips v: 1 − the game's v).
pub fn glow_meshes(meshes: &[PreparedMesh], style: &GlowStyle, material: &str) -> Vec<PreparedMesh> {
    let mut shells = shell_meshes(meshes, &style.shell, material);
    let all: Vec<[f64; 3]> = shells.iter().flat_map(|m| m.positions.iter().copied()).collect();
    let frame = bounds(&all);
    for m in &mut shells {
        if let Some(color) = style.color {
            m.colors = vec![vec![color; m.normals.len()]];
        }
        if let Some([u, v]) = style.projection {
            let mut uvs = vec![[0.; 2]; m.normals.len()];
            for face in &m.faces {
                for (p, c) in face.vertices.iter().zip(&face.corners) {
                    if let (Some(slot), Some(position)) = (uvs.get_mut(*c), m.positions.get(*p)) {
                        let n = unit(*position, frame);
                        let at = |c: [f64; 4]| c[0] + c[1] * n[0] + c[2] * n[1] + c[3] * n[2];
                        *slot = [at(u), 1. - at(v)];
                    }
                }
            }
            m.uvs = vec![uvs];
            // The user's tangents follow the user's texture coordinates: the compiler makes them again.
            m.tangents.clear();
        }
    }
    shells
}

/// The mesh files of a model with body-group shells (`GlowShells`): `nodes` and
/// `groups` are the decompiled vmdl's mesh nodes and levels of detail, which must name
/// the compiled model's meshes in its order (`mesh_names`). `out` comes back with each
/// node's new file, the shell material and the style.
#[allow(clippy::too_many_arguments)]
pub fn files(mut out: CarriedFiles, shells: &GlowShells, mesh_names: &[String], meshes: &[PreparedMesh], body: &[u8], bone: Option<&deadlock_skeleton::Skeleton>, original: &Original, nodes: &[MeshNode], groups: &[Vec<String>], root: &str, leaf: &str) -> Result<CarriedFiles> {
    let names: Vec<&String> = nodes.iter().map(|n| &n.name).collect();
    if names != mesh_names.iter().collect::<Vec<_>>() || groups.len() != shells.levels.len() {
        return Err(invalid(format!("The game's ability object names its meshes {mesh_names:?} in {} levels, its decompiled model {names:?} in {}; its shells cannot be told apart: {}", shells.levels.len(), groups.len(), original.target)));
    }
    let load = |index: usize| -> Result<deadlock_dmx::Dmx> {
        deadlock_dmx::Dmx::load(&safety::inside(&original.folder, &nodes[index].filename)?).map_err(|e| invalid(format!("The game's mesh {} could not be read: {e}", nodes[index].name)))
    };
    let (first_body, first_shell) = shells.levels.iter().find_map(|(b, s)| s.map(|s| (*b, s))).ok_or_else(|| invalid("The game's object has no shell."))?;
    let style = glow_style(&load(first_body)?, &load(first_shell)?)?;
    let shell_dmx = body_dmx(&glow_meshes(meshes, &style, &shells.material), bone, original.axis)?;
    for (level, ((object, shell), group)) in shells.levels.iter().zip(groups).enumerate() {
        let wanted: Vec<&String> = std::iter::once(*object).chain(*shell).map(|i| &nodes[i].name).collect();
        let mut listed: Vec<&String> = group.iter().collect();
        listed.sort();
        let mut sorted = wanted.clone();
        sorted.sort();
        if listed != sorted {
            return Err(invalid(format!("Level of detail {level} lists {group:?}, the game's model {wanted:?}: {}", original.target)));
        }
        let path = format!("{root}/{leaf}_body{level}.dmx");
        out.files.push((path.clone(), body.to_vec()));
        out.nodes.insert(nodes[*object].name.clone(), Some(path));
        if let Some(shell) = shell {
            let path = format!("{root}/{leaf}_shell{level}.dmx");
            out.files.push((path.clone(), shell_dmx.clone()));
            out.nodes.insert(nodes[*shell].name.clone(), Some(path));
        }
    }
    if let Some(left) = nodes.iter().find(|n| !out.nodes.contains_key(&n.name)) {
        return Err(invalid(format!("The game's mesh {} is in no level of detail: {}", left.name, original.target)));
    }
    out.shells = Some(deadlock_components::ability_objects::Shells::Generated);
    out.shell_material = Some(shells.material.clone());
    out.style = Some(style.shell);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadlock_skeleton::export::PreparedFace;

    #[test]
    fn a_projection_is_read_back_from_its_own_coordinates() {
        // u follows y, v falls with z, across the bounds: what the game's shell holds.
        let points: Vec<[f64; 3]> = (0..60).map(|i| [f64::from(i % 3) * 2., f64::from(i % 5) * 3. - 4., f64::from(i % 7)]).collect();
        let frame = bounds(&points);
        let rows: Vec<([f64; 3], f64)> = points.iter().map(|p| (unit(*p, frame), (p[1] + 4.) / 12.)).collect();
        let c = fit(&rows).unwrap();
        assert!((c[0]).abs() < 1e-9 && (c[1]).abs() < 1e-9 && (c[2] - 1.).abs() < 1e-9 && (c[3]).abs() < 1e-9, "{c:?}");
        let rows: Vec<([f64; 3], f64)> = points.iter().map(|p| (unit(*p, frame), 1. - p[2] / 6.)).collect();
        let c = fit(&rows).unwrap();
        assert!((c[0] - 1.).abs() < 1e-9 && (c[3] + 1.).abs() < 1e-9, "{c:?}");
    }

    #[test]
    fn generated_shells_take_the_game_s_colour_and_projection() {
        let quad = PreparedMesh {
            name: "part".into(),
            positions: vec![[0., 0., 0.], [0., 2., 0.], [0., 2., 4.], [0., 0., 4.]],
            normals: vec![[1., 0., 0.]; 4],
            uvs: vec![vec![[0.5, 0.5]; 4]],
            colors: vec![],
            tangents: vec![[0., 1., 0., 1.]; 4],
            weights: vec![],
            faces: vec![PreparedFace { vertices: vec![0, 1, 2, 3], corners: vec![0, 1, 2, 3], material: "own.vmat".into() }],
            extra: Default::default(),
        };
        let style = GlowStyle { shell: ShellStyle { distance: 0.25, turned: false, normals_turned: false }, color: Some([0.3, 0.2, 0.1, 1.]), projection: Some([[0., 0., 1., 0.], [1., 0., 0., -1.]]) };
        let shell = &glow_meshes(std::slice::from_ref(&quad), &style, "game/glow.vmat")[0];
        assert!(shell.faces.iter().all(|f| f.material == "game/glow.vmat"));
        assert!(shell.positions.iter().all(|p| (p[0] - 0.25).abs() < 1e-12), "pushed out along the normals");
        assert_eq!(shell.colors, vec![vec![[0.3, 0.2, 0.1, 1.]; 4]]);
        // u = y across the bounds; the game's v = 1 − z across the bounds, written flipped: z.
        assert_eq!(shell.uvs, vec![vec![[0., 0.], [1., 0.], [1., 1.], [0., 1.]]]);
        assert!(shell.tangents.is_empty());
        // Without the game's colour and projection the user's own coordinates stay.
        let plain = &glow_meshes(std::slice::from_ref(&quad), &GlowStyle { color: None, projection: None, ..style }, "game/glow.vmat")[0];
        assert!(plain.colors.is_empty() && plain.uvs == quad.uvs);
    }
}
