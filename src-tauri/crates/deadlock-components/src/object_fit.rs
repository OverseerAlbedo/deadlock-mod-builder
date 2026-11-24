//! How the user's object is placed in the game object's frame: one affine map
//! `p' = A·p + t` from the prepared file's coordinates to the game model's.
//! The build applies it when it writes the object's mesh (so changing the fit
//! never needs Blender again) and the fit view shows it.
//!
//! 1. Units and axes (both sizes): the prepared file's coordinates are Blender
//!    units; times `unit_metres` (the file's metres per unit) they are metres,
//!    and the game counts inches, so `c = unit_metres / 0.0254`. The axes are
//!    kept as they are: Blender and the game are both Z up and right-handed,
//!    and the file's +X stays the game's +X. (The character preparation has no
//!    fixed unit of its own: it scales a body to the hero's height. "keep" is
//!    this plain metre-to-inch conversion.)
//! 2. `keep`: nothing more; the file's origin is the game model's origin.
//!    `match`: the user's longest bounds axis is turned onto the game object's
//!    longest bounds axis by the cyclic axis permutation that maps one onto the
//!    other (a proper rotation: nothing is mirrored; on equal extents the lower
//!    axis, X before Y before Z, counts as the longest), the object is scaled
//!    uniformly so the two longest extents are equal, and moved so the two
//!    bounds centres coincide. The particle's own size multiplier needs no
//!    modelling: the replacement simply takes the game object's size.
//! 3. Quarter turns about the pivot (the game bounds centre for `match`, the
//!    origin for `keep`): `roll` about X, then `tip` about Y, then `turn` about
//!    Z (each 0–3 quarter turns, counter-clockwise looking down the axis), then
//!    `scale`, uniformly about the same pivot.
//!
//! 4. `on_base` (absent unless set): the map is then moved along the game's up
//!    axis (Z) so the user's lowest point meets the game object's lowest point
//!    (for `match` instead of the centres meeting along Z).
//!
//! `A` is a rotation times a positive uniform scale, so normals take the rotation
//! alone and stay unit length.
use crate::{Result, ability_objects::Bounds, fail};
use deadlock_project::ability_objects::{FitSize, ObjectFit};
use serde::{Deserialize, Serialize};

/// Inches per metre as the game counts them (1 / 0.0254).
pub const INCH_METRES: f64 = 0.0254;
/// The smallest extent (inches, after the conversion) a fitted object may have along its long axis.
const MIN_EXTENT: f64 = 1e-6;

/// `p' = linear·p + offset`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Affine {
    pub linear: [[f64; 3]; 3],
    pub offset: [f64; 3],
}
type M3 = [[f64; 3]; 3];
fn mul(a: &M3, b: &M3) -> M3 {
    let mut out = [[0.; 3]; 3];
    for (i, row) in out.iter_mut().enumerate() {
        for (j, cell) in row.iter_mut().enumerate() {
            *cell = (0..3).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    out
}
fn apply(a: &M3, p: [f64; 3]) -> [f64; 3] {
    [0, 1, 2].map(|i| a[i][0] * p[0] + a[i][1] * p[1] + a[i][2] * p[2])
}
fn scaled(a: &M3, s: f64) -> M3 {
    a.map(|row| row.map(|v| v * s))
}
const IDENTITY: M3 = [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]];
/// One quarter turn about axis `axis` (0 X, 1 Y, 2 Z), exact integers.
fn quarter(axis: usize) -> M3 {
    match axis {
        0 => [[1., 0., 0.], [0., 0., -1.], [0., 1., 0.]],
        1 => [[0., 0., 1.], [0., 1., 0.], [-1., 0., 0.]],
        _ => [[0., -1., 0.], [1., 0., 0.], [0., 0., 1.]],
    }
}
fn power(m: &M3, times: u8) -> M3 {
    (0..times).fold(IDENTITY, |acc, _| mul(m, &acc))
}
/// The turns as one rotation: roll first, then tip, then turn.
pub fn turns(fit: &ObjectFit) -> M3 {
    let t = fit.turns;
    mul(&power(&quarter(2), t.turn), &mul(&power(&quarter(1), t.tip), &power(&quarter(0), t.roll)))
}
/// The index of the longest extent (the lower index on a tie).
pub fn long_axis(size: [f64; 3]) -> usize {
    let mut best = 0;
    for i in 1..3 {
        if size[i] > size[best] {
            best = i;
        }
    }
    best
}
/// The cyclic axis permutation that carries axis `from` onto axis `to`.
pub fn permutation(from: usize, to: usize) -> M3 {
    let k = (from + 3 - to) % 3;
    let mut out = [[0.; 3]; 3];
    for (i, row) in out.iter_mut().enumerate() {
        row[(i + k) % 3] = 1.;
    }
    out
}
fn centre(b: &Bounds) -> [f64; 3] {
    [0, 1, 2].map(|i| (b.min[i] + b.max[i]) / 2.)
}
/// The fit's map from the prepared file (its bounds `user`, in file units, and
/// `unit_metres`) into the game object's frame (its bounds `game`, inches).
pub fn transform(fit: &ObjectFit, unit_metres: f64, user: &Bounds, game: &Bounds) -> Result<Affine> {
    if !unit_metres.is_finite() || unit_metres <= 0. {
        return Err(fail("The prepared object has no valid unit scale. Prepare it again."));
    }
    let c = unit_metres / INCH_METRES;
    let rotation = turns(fit);
    let mut affine = match fit.size {
        FitSize::Keep => Affine { linear: scaled(&rotation, fit.scale * c), offset: [0.; 3] },
        FitSize::Match => {
            let (from, to) = (long_axis(user.size()), long_axis(game.size()));
            let (user_extent, game_extent) = (user.size()[from] * c, game.size()[to]);
            if !(user_extent > MIN_EXTENT) {
                return Err(fail("The prepared object has no size to fit (all its points are in one place)."));
            }
            if !(game_extent > MIN_EXTENT) {
                return Err(fail("The game object has no size to fit to."));
            }
            let s0 = game_extent / user_extent;
            let linear = scaled(&mul(&rotation, &permutation(from, to)), fit.scale * s0 * c);
            let pivot = centre(game);
            let moved = apply(&linear, centre(user));
            Affine { linear, offset: [0, 1, 2].map(|i| pivot[i] - moved[i]) }
        }
    };
    if fit.on_base {
        affine.stand_on(user, game);
    }
    Ok(affine)
}
/// The game's up axis in the object's frame (model space is Z up).
pub const UP: usize = 2;
impl Affine {
    /// Moves the map along `UP` so the user's lowest point meets the game
    /// object's (the turns and axis permutations keep bounds boxes exact).
    fn stand_on(&mut self, user: &Bounds, game: &Bounds) {
        let corners = (0..8).map(|k| [0, 1, 2].map(|i| if k & (1 << i) == 0 { user.min[i] } else { user.max[i] }));
        if let Some(low) = self.bounds(corners).map(|b| b.min[UP]) {
            self.offset[UP] += game.min[UP] - low;
        }
    }
    pub fn point(&self, p: [f64; 3]) -> [f64; 3] {
        let q = apply(&self.linear, p);
        [0, 1, 2].map(|i| q[i] + self.offset[i])
    }
    /// A normal through the rotation part (the linear map over its uniform scale).
    pub fn normal(&self, n: [f64; 3]) -> [f64; 3] {
        let q = apply(&self.linear, n);
        let length = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2]).sqrt();
        if length > 0. { q.map(|v| v / length) } else { q }
    }
    /// The 4×4 matrix, row-major, for the fit view (`p' = M·[p, 1]`).
    pub fn matrix(&self) -> [[f64; 4]; 4] {
        let a = &self.linear;
        [
            [a[0][0], a[0][1], a[0][2], self.offset[0]],
            [a[1][0], a[1][1], a[1][2], self.offset[1]],
            [a[2][0], a[2][1], a[2][2], self.offset[2]],
            [0., 0., 0., 1.],
        ]
    }
    /// The bounds of `points` after the map.
    pub fn bounds(&self, points: impl IntoIterator<Item = [f64; 3]>) -> Option<Bounds> {
        let mut out: Option<Bounds> = None;
        for p in points {
            let q = self.point(p);
            let b = out.get_or_insert(Bounds { min: q, max: q });
            for i in 0..3 {
                b.min[i] = b.min[i].min(q[i]);
                b.max[i] = b.max[i].max(q[i]);
            }
        }
        out
    }
}
/// The bounds of points (`None` without any).
pub fn bounds_of(points: impl IntoIterator<Item = [f64; 3]>) -> Option<Bounds> {
    Affine { linear: IDENTITY, offset: [0.; 3] }.bounds(points)
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadlock_project::ability_objects::Turns;

    fn close(a: [f64; 3], b: [f64; 3]) {
        for i in 0..3 {
            assert!((a[i] - b[i]).abs() < 1e-9, "{a:?} != {b:?}");
        }
    }
    /// The game object of the tests: a thrown blade about 20.4 long along +X.
    const GAME: Bounds = Bounds { min: [-1.75, -0.65, -2.1], max: [18.65, 0.65, 2.1] };

    #[test]
    fn quarter_turns_are_exact_proper_rotations() {
        assert_eq!(apply(&quarter(0), [0., 1., 0.]), [0., 0., 1.], "roll: +Y to +Z");
        assert_eq!(apply(&quarter(1), [0., 0., 1.]), [1., 0., 0.], "tip: +Z to +X");
        assert_eq!(apply(&quarter(2), [1., 0., 0.]), [0., 1., 0.], "turn: +X to +Y");
        assert_eq!(power(&quarter(2), 4), IDENTITY);
        // Roll first, then tip, then turn: +Y → (roll) +Z → (tip) +X → (turn) +Y.
        let fit = ObjectFit { turns: Turns { roll: 1, tip: 1, turn: 1 }, ..Default::default() };
        assert_eq!(apply(&turns(&fit), [0., 1., 0.]), [0., 1., 0.]);
        assert_eq!(apply(&turns(&fit), [1., 0., 0.]), [0., 0., -1.]);
        for (from, to) in [(0, 0), (1, 0), (2, 0), (0, 1), (2, 1), (0, 2)] {
            let p = permutation(from, to);
            let mut unit = [0.; 3];
            unit[from] = 1.;
            let mut want = [0.; 3];
            want[to] = 1.;
            assert_eq!(apply(&p, unit), want, "{from} → {to}");
            let det = p[0][0] * (p[1][1] * p[2][2] - p[1][2] * p[2][1]) - p[0][1] * (p[1][0] * p[2][2] - p[1][2] * p[2][0]) + p[0][2] * (p[1][0] * p[2][1] - p[1][1] * p[2][0]);
            assert_eq!(det, 1., "never mirrored");
        }
    }

    #[test]
    fn match_takes_the_game_objects_long_axis_size_and_centre() {
        // A 2 m bar along Y in a metre file: turned onto X, 20.4 long, centred on the blade.
        let user = Bounds { min: [-0.1, -1.0, -0.05], max: [0.1, 1.0, 0.05] };
        let fit = transform(&ObjectFit::default(), 1.0, &user, &GAME).unwrap();
        let s = 20.4 / (2.0 / INCH_METRES) / INCH_METRES; // inches per file unit after matching
        assert!((s - 10.2).abs() < 1e-12, "{s}");
        // The permutation Y → X: (x, y, z) → (y, z, x).
        close(fit.point([0., 1., 0.]), [18.65, 0., 0.]);
        close(fit.point([0., -1., 0.]), [-1.75, 0., 0.]);
        close(fit.point([0.1, 0., 0.]), [8.45, 0., 1.02]);
        close(fit.point([0., 0., 0.05]), [8.45, 0.51, 0.]);
        let b = fit.bounds([[-0.1, -1., -0.05], [0.1, 1., 0.05]]).unwrap();
        close(b.min, [-1.75, -0.51, -1.02]);
        close(b.max, [18.65, 0.51, 1.02]);
        close(fit.normal([0., 0., 1.]), [0., 1., 0.]);
        // The file's unit does not change a match (a centimetre file of the same shape).
        let cm = transform(&ObjectFit::default(), 0.01, &Bounds { min: user.min.map(|v| v * 100.), max: user.max.map(|v| v * 100.) }, &GAME).unwrap();
        close(cm.point([0., 100., 0.]), [18.65, 0., 0.]);
        // Two quarter turns about Z point it the other way; scale 0.5 halves it about the centre.
        let turned = transform(&ObjectFit { scale: 0.5, turns: Turns { turn: 2, ..Default::default() }, ..Default::default() }, 1.0, &user, &GAME).unwrap();
        close(turned.point([0., 1., 0.]), [3.35, 0., 0.]);
        close(turned.point([0., -1., 0.]), [13.55, 0., 0.]);
    }

    #[test]
    fn keep_converts_metres_to_inches_and_keeps_the_origin() {
        let user = Bounds { min: [0., 0., 0.], max: [0.5, 0.1, 0.1] };
        let fit = transform(&ObjectFit { size: FitSize::Keep, ..Default::default() }, 1.0, &user, &GAME).unwrap();
        close(fit.point([0., 0., 0.]), [0., 0., 0.]);
        close(fit.point([0.0254, 0., 0.]), [1., 0., 0.]);
        close(fit.point([0.5, 0.1, 0.]), [0.5 / 0.0254, 0.1 / 0.0254, 0.]);
        // An inch file (unit 0.0254 m): one unit, one inch; turned a quarter about X and doubled.
        let inch = transform(&ObjectFit { size: FitSize::Keep, scale: 2., turns: Turns { roll: 1, ..Default::default() }, ..Default::default() }, 0.0254, &user, &GAME).unwrap();
        close(inch.point([3., 1., 0.]), [6., 0., 2.]);
        assert_eq!(inch.matrix()[3], [0., 0., 0., 1.]);
    }

    #[test]
    fn standing_on_the_base_matches_the_lowest_points_instead_of_the_centres() {
        // A 2 m bar along Y matched to the blade, then stood on its base: only Z moves.
        let user = Bounds { min: [-0.1, -1.0, -0.05], max: [0.1, 1.0, 0.05] };
        let centred = transform(&ObjectFit::default(), 1.0, &user, &GAME).unwrap();
        let standing = transform(&ObjectFit { on_base: true, ..Default::default() }, 1.0, &user, &GAME).unwrap();
        assert_eq!(standing.linear, centred.linear);
        let b = standing.bounds([user.min, user.max]).unwrap();
        assert!((b.min[UP] - GAME.min[UP]).abs() < 1e-9, "{b:?}");
        close([standing.offset[0], standing.offset[1], 0.], [centred.offset[0], centred.offset[1], 0.]);
        // Turned up on its end, the lowest point still meets the game's.
        let tipped = transform(&ObjectFit { on_base: true, turns: Turns { tip: 1, ..Default::default() }, ..Default::default() }, 1.0, &user, &GAME).unwrap();
        let b = tipped.bounds([user.min, user.max, [user.min[0], user.max[1], user.min[2]], [user.max[0], user.min[1], user.max[2]]]).unwrap();
        assert!((b.min[UP] - GAME.min[UP]).abs() < 1e-9);
        // Keep: the file's own size, its lowest point on the game object's base.
        let kept = transform(&ObjectFit { size: FitSize::Keep, on_base: true, ..Default::default() }, 1.0, &user, &GAME).unwrap();
        close(kept.point([0., 0., -0.05]), [0., 0., GAME.min[UP]]);
        // Unset: exactly the map it always was.
        assert_eq!(transform(&ObjectFit { on_base: false, ..Default::default() }, 1.0, &user, &GAME).unwrap(), centred);
    }

    #[test]
    fn a_flat_or_unscaled_object_cannot_be_matched() {
        let point = Bounds { min: [1.; 3], max: [1.; 3] };
        assert!(transform(&ObjectFit::default(), 1.0, &point, &GAME).is_err());
        assert!(transform(&ObjectFit::default(), 0., &GAME, &GAME).is_err());
        assert!(transform(&ObjectFit::default(), 1.0, &GAME, &point).is_err());
        // Keep needs no size.
        assert!(transform(&ObjectFit { size: FitSize::Keep, ..Default::default() }, 1.0, &point, &GAME).is_ok());
        assert_eq!(long_axis([1., 1., 1.]), 0);
        assert_eq!(long_axis([1., 2., 2.]), 1);
    }
}
