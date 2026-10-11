use crate::math::{cross, dot, sub};
use deadlock_core::{CancellationToken, CoreError, Result};
fn fail(message: &str) -> CoreError {
    CoreError::Integrity(message.into())
}
fn fan(corners: &[u32]) -> Vec<[u32; 3]> {
    (1..corners.len() - 1)
        .map(|i| [corners[0], corners[i], corners[i + 1]])
        .collect()
}
pub fn polygon(
    corners: &[u32],
    positions: &[[f32; 3]],
    position_indices: &[usize],
    cancel: &CancellationToken,
) -> Result<Vec<[u32; 3]>> {
    if !(3..=1024).contains(&corners.len()) {
        return Err(fail("Preview requires DMX polygons with 3–1024 corners"));
    }
    if corners.len() == 3 {
        return Ok(vec![[corners[0], corners[1], corners[2]]]);
    }
    let mut p: Vec<[f64; 3]> = corners
        .iter()
        .map(|&c| positions[position_indices[c as usize]].map(f64::from))
        .collect();
    let origin = p[0];
    for point in &mut p {
        *point = sub(*point, origin);
    }
    let mut normal = [0.; 3];
    let mut maxcross = 0f64;
    for i in 0..p.len() {
        let turn = cross(p[i], p[(i + 1) % p.len()]);
        for k in 0..3 {
            normal[k] += turn[k];
            maxcross = maxcross.max(turn[k].abs());
        }
    }
    if p.len() == 4 {
        let n2 = dot(normal, normal);
        let epsilon = (n2 * 1e-12).max(1e-30);
        let convex = (0..4).all(|i| {
            dot(
                cross(
                    sub(p[(i + 1) % 4], p[i]),
                    sub(p[(i + 2) % 4], p[(i + 1) % 4]),
                ),
                normal,
            ) >= -epsilon
        });
        if convex && n2.sqrt() > 1e-20 {
            return Ok(fan(corners));
        }
    }
    let dropped = (0..3)
        .max_by(|&a, &b| normal[a].abs().total_cmp(&normal[b].abs()))
        .unwrap();
    let dims: Vec<_> = (0..3).filter(|&i| i != dropped).collect();
    let xy: Vec<[f64; 2]> = p.iter().map(|v| [v[dims[0]], v[dims[1]]]).collect();
    let cross2 = |a: [f64; 2], b: [f64; 2]| a[0] * b[1] - a[1] * b[0];
    let minus = |a: [f64; 2], b: [f64; 2]| [a[0] - b[0], a[1] - b[1]];
    let area: f64 = (0..xy.len())
        .map(|i| cross2(xy[i], xy[(i + 1) % xy.len()]))
        .sum();
    let scale = xy
        .iter()
        .flatten()
        .fold(0f64, |v, x| v.max(x.abs()))
        .powi(2)
        .max(1e-30);
    let epsilon = scale * 1e-12;
    if area.abs() < epsilon {
        if maxcross <= epsilon {
            return Ok(fan(corners));
        }
        return Err(fail(
            "Preview cannot triangulate a self-intersecting polygon",
        ));
    }
    let sign = if area > 0. { 1. } else { -1. };
    let mut pending: Vec<usize> = (0..corners.len()).collect();
    let mut result = Vec::new();
    for i in (0..pending.len()).rev() {
        if pending.len() <= 3 {
            break;
        }
        let a = pending[(i + pending.len() - 1) % pending.len()];
        let b = pending[i];
        if p[a] == p[b] {
            let c = pending[(i + 1) % pending.len()];
            result.push([corners[a], corners[b], corners[c]]);
            pending.remove(i);
        }
    }
    while pending.len() > 3 {
        cancel.check()?;
        let mut found = false;
        for k in 0..pending.len() {
            let a = pending[(k + pending.len() - 1) % pending.len()];
            let b = pending[k];
            let c = pending[(k + 1) % pending.len()];
            if sign * cross2(minus(xy[b], xy[a]), minus(xy[c], xy[b])) <= epsilon {
                continue;
            }
            let enclosed = pending.iter().any(|&point| {
                point != a
                    && point != b
                    && point != c
                    && [(a, b), (b, c), (c, a)].iter().all(|&(start, end)| {
                        sign * cross2(minus(xy[end], xy[start]), minus(xy[point], xy[start]))
                            >= -epsilon
                    })
            });
            if !enclosed {
                result.push([corners[a], corners[b], corners[c]]);
                pending.remove(k);
                found = true;
                break;
            }
        }
        if !found {
            return Err(fail(
                "Preview cannot triangulate this DMX polygon; check for self-intersections",
            ));
        }
    }
    result.push([
        corners[pending[0]],
        corners[pending[1]],
        corners[pending[2]],
    ]);
    Ok(result)
}
pub fn faces(
    values: &[i32],
    positions: &[[f32; 3]],
    position_indices: &[usize],
    cancel: &CancellationToken,
    warnings: &mut Vec<String>,
) -> Result<Vec<[u32; 3]>> {
    if values.is_empty()
        || values.last() != Some(&-1)
        || values
            .iter()
            .any(|&i| i < -1 || i >= position_indices.len() as i32)
    {
        return Err(fail("DMX preview face indices are invalid or unterminated"));
    }
    let mut output = Vec::new();
    let mut corners = Vec::new();
    let mut ambiguous = 0;
    for (index, &value) in values.iter().enumerate() {
        if index % 4096 == 0 {
            cancel.check()?;
        }
        if value >= 0 {
            corners.push(value as u32);
            if corners.len() > 1024 {
                return Err(fail("Preview polygon exceeds 1024 corners"));
            }
        } else {
            if corners.len() < 3 {
                return Err(fail("Preview polygon contains fewer than three corners"));
            }
            let needed = corners.len() - 2;
            if output.len() + needed > crate::MAX_TRIANGLES {
                return Err(fail("Preview exceeds four million triangles"));
            }
            match polygon(&corners, positions, position_indices, cancel) {
                Ok(triangles) => output.extend(triangles),
                Err(CoreError::Cancelled) => return Err(CoreError::Cancelled),
                Err(_) => {
                    ambiguous += 1;
                    output.extend(fan(&corners));
                }
            }
            corners.clear();
        }
    }
    if ambiguous > 0 {
        warnings.push(format!("{ambiguous} ambiguous/non-simple DMX polygons use a preview-only corner fan. Their appearance may differ from Blender or the game; source geometry is unchanged."));
    }
    Ok(output)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn concave_area_preserved() {
        let p = [
            [0., 0., 0.],
            [2., 0., 0.],
            [2., 2., 0.],
            [1., 1., 0.],
            [0., 2., 0.],
        ];
        let t = polygon(
            &[0, 1, 2, 3, 4],
            &p,
            &[0, 1, 2, 3, 4],
            &CancellationToken::default(),
        )
        .unwrap();
        let area: f64 = t
            .iter()
            .map(|x| {
                let [a, b, c] = x.map(|i| p[i as usize].map(f64::from));
                dot(cross(sub(b, a), sub(c, a)), cross(sub(b, a), sub(c, a))).sqrt() / 2.
            })
            .sum();
        assert_eq!(t.len(), 3);
        assert_eq!(area, 3.);
    }
    #[test]
    fn crossing_warns_only_preview() {
        let p = [[0., 0., 0.], [1., 1., 0.], [0., 1., 0.], [1., 0., 0.]];
        let mut warnings = Vec::new();
        let out = faces(
            &[0, 1, 2, 3, -1],
            &p,
            &[0, 1, 2, 3],
            &CancellationToken::default(),
            &mut warnings,
        )
        .unwrap();
        assert_eq!(out.len(), 2);
        assert!(warnings[0].contains("preview-only"));
    }
    #[test]
    fn malformed_faces_rejected() {
        for values in [vec![0, 1, 2], vec![0, 1, -1], vec![0, 1, 99, -1]] {
            assert!(faces(
                &values,
                &[[0.; 3]; 3],
                &[0, 1, 2],
                &CancellationToken::default(),
                &mut vec![]
            )
            .is_err());
        }
    }
}
