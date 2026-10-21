//! Prepared moving-part bones clear of the compiler's near-pole Euler
//! conversion. The model compiler stores each bone's local rotation through
//! Euler angles (MatrixAngles). When a bone's local x axis lies close to its
//! parent's z axis (`xy`, the length of that axis in the parent's x-y plane, is
//! small) the conversion loses about 4e-5°/xy: SiViolet's character on Nurse
//! Harrow (October 20, 2026) had three chest cloth roots at xy 0.00116 that
//! compiled 0.031–0.041° off, three times the strict transfer budget
//! (quaternion component 1e-4); the same bones on Violet, at xy 0.006, came out
//! 0.004–0.008° off and passed. A float32 round trip does not reproduce the
//! compiler's value, so it cannot be qualified after compiling.
//!
//! On the pole itself the conversion takes its singular branch, where the
//! bone's twist moves into the yaw and the rotation is kept exactly. So bones
//! below [`SNAP_BELOW`] are turned onto the pole here, at preparation: the
//! smallest rotation that swings the x axis onto the parent's z axis (at most
//! asin(SNAP_BELOW), 0.32°), which keeps the bone's twist. Harrow's three bones
//! then compiled within 5e-7.
//!
//! The bone's children turn with it: their locals stay exactly as prepared,
//! so the branch moves rigidly about the bone's joint, at most xy times its
//! length (0.01 in on Harrow's 8.7 in tassel). Keeping the children's world
//! transforms instead (their locals taking the inverse turn) made the
//! compiler's joint fits of the streamer chain under the tassel root disagree
//! with their rest orientation by 0.01° (1.7e-4, over the 1e-4 check). The mesh
//! is unchanged either way: a prepared bone's bind and rest pose are the same.
//! Only added bones move; the hero's own bones never change.
use crate::{invalid, Result, Skeleton};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const POLICY: &str = "near-pole-euler-snap-v1";
/// Bones closer to the pole than this are snapped onto it (about 0.008° after
/// compiling at this distance, under the budget; Violet's bones at 0.006 pass
/// unchanged, measured October 20, 2026).
pub const SNAP_BELOW: f64 = 0.0055;
/// Already on the pole within float noise: left as it is.
const ON_POLE: f64 = 1e-9;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Tilt {
    pub bone: String,
    pub xy_before: f64,
    pub degrees: f64,
}

type Q = [f64; 4];
fn mul(a: Q, b: Q) -> Q {
    let [ax, ay, az, aw] = a;
    let [bx, by, bz, bw] = b;
    [
        aw * bx + ax * bw + ay * bz - az * by,
        aw * by - ax * bz + ay * bw + az * bx,
        aw * bz + ax * by - ay * bx + az * bw,
        aw * bw - ax * bx - ay * by - az * bz,
    ]
}
fn conj([x, y, z, w]: Q) -> Q {
    [-x, -y, -z, w]
}
fn normalized(q: Q) -> Q {
    let n = q.iter().map(|v| v * v).sum::<f64>().sqrt();
    q.map(|v| v / n)
}
fn rotate(q: Q, v: [f64; 3]) -> [f64; 3] {
    let [x, y, z, _] = mul(mul(q, [v[0], v[1], v[2], 0.]), conj(q));
    [x, y, z]
}
/// The bone's local x axis in its parent's frame.
fn x_axis(q: Q) -> [f64; 3] {
    rotate(normalized(q), [1., 0., 0.])
}
/// MatrixAngles' `xy` of a local rotation (x, y, z, w).
pub fn xy(q: Q) -> f64 {
    let x = x_axis(q);
    x[0].hypot(x[1])
}

/// Snap every bone not in `fixed` whose local x axis is near its parent's z
/// axis onto it; its branch turns with it. Bones must come parent first.
pub fn clear(skeleton: &mut Skeleton, fixed: &BTreeSet<String>) -> Result<Vec<Tilt>> {
    if skeleton.parents.iter().enumerate().any(|(i, &p)| p >= i as i32) {
        return Err(invalid("Near-pole snapping needs a parent-first skeleton"));
    }
    let mut tilts = vec![];
    for i in 0..skeleton.names.len() {
        if fixed.contains(&skeleton.names[i]) {
            continue;
        }
        let q = normalized(skeleton.rotations[i]);
        let before = xy(q);
        if !(ON_POLE < before && before < SNAP_BELOW) {
            continue;
        }
        // The branch below must not hold a hero bone (it moves with this one).
        let mut branch = BTreeSet::from([i]);
        for c in (i + 1)..skeleton.names.len() {
            if branch.contains(&(skeleton.parents[c] as usize)) {
                if fixed.contains(&skeleton.names[c]) {
                    return Err(invalid("Near-pole snapping would move a hero bone"));
                }
                branch.insert(c);
            }
        }
        // Swing the x axis onto the pole: about the axis x × z, by asin(xy).
        let x = x_axis(q);
        let pole = [0., 0., x[2].signum()];
        let axis = [x[1] * pole[2] - x[2] * pole[1], x[2] * pole[0] - x[0] * pole[2], 0.];
        let sin = axis[0].hypot(axis[1]);
        let angle = sin.atan2(x[2].abs());
        let half = (angle * 0.5).sin() / sin;
        let tilt = [axis[0] * half, axis[1] * half, 0., (angle * 0.5).cos()];
        skeleton.rotations[i] = normalized(mul(tilt, q));
        tilts.push(Tilt { bone: skeleton.names[i].clone(), xy_before: before, degrees: angle.to_degrees() });
    }
    Ok(tilts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transforms::bind_state;
    /// A quaternion turning x onto the given direction, then twisted about it.
    fn pointing(d: [f64; 3], twist: f64) -> Q {
        let n = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        let d = d.map(|v| v / n);
        let axis = [0., -d[2], d[1]];
        let s = axis.iter().map(|v| v * v).sum::<f64>().sqrt();
        let angle = s.atan2(d[0]);
        let k = (angle * 0.5).sin() / s;
        let swing = [axis[0] * k, axis[1] * k, axis[2] * k, (angle * 0.5).cos()];
        mul(swing, [(twist * 0.5).sin(), 0., 0., (twist * 0.5).cos()])
    }
    fn skeleton() -> Skeleton {
        // A hero bone, a cloth root hanging 0.066° off its parent's z axis (Nurse
        // Harrow's chest cloth, xy 0.00116) with a twist, its child, a bone already
        // on the pole and one clear of it (Violet's distance).
        Skeleton {
            names: ["spine_3", "cloth_root", "cloth_tip", "on_pole", "clear_bone"].map(String::from).to_vec(),
            parents: vec![-1, 0, 1, 0, 0],
            positions: vec![[0., 0., 80.], [1., 0.5, 1.], [3., 0., 0.], [0., 1., 0.], [0., -1., 0.]],
            rotations: vec![pointing([0.2, 0.1, 1.], 0.3), pointing([0.00116, 0., -1.], 0.7), pointing([1., 0.3, 0.1], 0.), pointing([0., 0., 1.], 0.4), pointing([0.006, 0., 1.], 0.2)],
            scales: vec![1.; 5],
            flags: vec![0; 5],
        }
    }
    fn distance(a: &crate::transforms::Matrix4, b: &crate::transforms::Matrix4) -> f64 {
        (0..3).map(|r| (a[r][3] - b[r][3]).powi(2)).sum::<f64>().sqrt()
    }
    #[test]
    fn a_near_pole_bone_is_snapped_onto_the_pole_and_its_branch_turns_with_it() {
        let mut s = skeleton();
        let before = bind_state(&s).unwrap();
        let tilts = clear(&mut s, &BTreeSet::from(["spine_3".to_string()])).unwrap();
        assert_eq!(tilts.len(), 1, "{tilts:?}");
        assert_eq!(tilts[0].bone, "cloth_root");
        assert!((tilts[0].xy_before - 0.00116).abs() < 1e-9);
        assert!((tilts[0].degrees - tilts[0].xy_before.asin().to_degrees()).abs() < 1e-9, "{}", tilts[0].degrees);
        assert!(xy(s.rotations[1]) < 1e-12);
        // Only the turned bone's local changes; its child's local stays as prepared.
        let original = skeleton();
        for i in [0, 2, 3, 4] {
            assert_eq!((s.positions[i], s.rotations[i]), (original.positions[i], original.rotations[i]), "{}", s.names[i]);
        }
        let after = bind_state(&s).unwrap();
        // The joint stays put; the child moves by at most the turn times its distance.
        assert!(distance(&before.world[1], &after.world[1]) < 1e-12);
        let reach = distance(&before.world[1], &before.world[2]);
        let moved = distance(&before.world[2], &after.world[2]);
        assert!(moved > 0. && moved <= reach * tilts[0].degrees.to_radians() + 1e-12, "{moved} of {reach}");
        // Bones outside the branch keep their world frames.
        for i in [0, 3, 4] {
            assert_eq!(before.world[i], after.world[i], "{}", s.names[i]);
        }
        // The bone keeps its twist: the turn only swings its x axis.
        let swing = mul(s.rotations[1], conj(original.rotations[1]));
        let x = x_axis(original.rotations[1]);
        assert!((swing[0] * x[0] + swing[1] * x[1] + swing[2] * x[2]).abs() < 1e-12);
    }
    #[test]
    fn bones_on_the_pole_clear_of_it_or_of_the_hero_stay_as_they_are() {
        let mut s = skeleton();
        s.rotations[1] = pointing([0.1, 0., -1.], 0.7);
        let expected = s.clone();
        assert!(clear(&mut s, &BTreeSet::from(["spine_3".to_string()])).unwrap().is_empty());
        assert_eq!(s, expected);
        // A hero bone near the pole is never moved, nor one with a hero bone below it.
        let mut s = skeleton();
        s.names[1] = "hero_bone".into();
        let names = BTreeSet::from(["spine_3".to_string(), "hero_bone".to_string()]);
        let expected = s.clone();
        assert!(clear(&mut s, &names).unwrap().is_empty());
        assert_eq!(s, expected);
        let mut s = skeleton();
        assert!(clear(&mut s, &BTreeSet::from(["spine_3".to_string(), "cloth_tip".to_string()])).is_err());
    }
}
