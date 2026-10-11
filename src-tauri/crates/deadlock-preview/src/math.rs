use deadlock_core::{CoreError, Result};
pub type Matrix = [[f64; 4]; 4];
pub fn identity() -> Matrix {
    let mut out = [[0.; 4]; 4];
    for (i, row) in out.iter_mut().enumerate() {
        row[i] = 1.;
    }
    out
}
pub fn mul(a: &Matrix, b: &Matrix) -> Matrix {
    let mut out = [[0.; 4]; 4];
    for r in 0..4 {
        for c in 0..4 {
            out[r][c] = (0..4).map(|k| a[r][k] * b[k][c]).sum();
        }
    }
    out
}
pub fn inverse(matrix: &Matrix) -> Result<Matrix> {
    let mut a = *matrix;
    let mut out = identity();
    for col in 0..4 {
        let pivot = (col..4)
            .max_by(|&x, &y| a[x][col].abs().total_cmp(&a[y][col].abs()))
            .unwrap();
        if a[pivot][col].abs() < f64::MIN_POSITIVE {
            return Err(CoreError::Integrity(
                "Preview bind transform is singular".into(),
            ));
        }
        a.swap(col, pivot);
        out.swap(col, pivot);
        let divisor = a[col][col];
        for k in 0..4 {
            a[col][k] /= divisor;
            out[col][k] /= divisor;
        }
        for row in 0..4 {
            if row == col {
                continue;
            }
            let factor = a[row][col];
            for k in 0..4 {
                a[row][k] -= factor * a[col][k];
                out[row][k] -= factor * out[col][k];
            }
        }
    }
    if out.iter().flatten().any(|x| !x.is_finite()) {
        return Err(CoreError::Integrity(
            "Preview inverse transform overflowed".into(),
        ));
    }
    Ok(out)
}
pub fn transform(matrix: &Matrix, p: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|r| matrix[r][3] + (0..3).map(|k| matrix[r][k] * p[k]).sum::<f64>())
}
pub fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| a[i] - b[i])
}
pub fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
pub fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| a[i] * b[i]).sum()
}
pub fn normalize(v: [f64; 3]) -> [f64; 3] {
    let length = dot(v, v).sqrt().max(1e-20);
    v.map(|x| x / length)
}
pub fn trs(position: [f64; 3], rotation: [f64; 4], scale: f64) -> Result<Matrix> {
    if !scale.is_finite()
        || scale <= 0.
        || position
            .iter()
            .chain(rotation.iter())
            .any(|x| !x.is_finite())
    {
        return Err(CoreError::Integrity(
            "Preview requires finite transforms and positive local scalar scale".into(),
        ));
    }
    let length = rotation.iter().map(|x| x * x).sum::<f64>().sqrt();
    if length < 1e-12 {
        return Err(CoreError::Integrity(
            "Preview rotation quaternion has zero length".into(),
        ));
    }
    let [x, y, z, w] = rotation.map(|v| v / length);
    let mut m = identity();
    m[0][0] = 1. - 2. * (y * y + z * z);
    m[0][1] = 2. * (x * y - z * w);
    m[0][2] = 2. * (x * z + y * w);
    m[1][0] = 2. * (x * y + z * w);
    m[1][1] = 1. - 2. * (x * x + z * z);
    m[1][2] = 2. * (y * z - x * w);
    m[2][0] = 2. * (x * z - y * w);
    m[2][1] = 2. * (y * z + x * w);
    m[2][2] = 1. - 2. * (x * x + y * y);
    for row in 0..3 {
        for col in 0..3 {
            m[row][col] *= scale;
        }
        m[row][3] = position[row];
    }
    Ok(m)
}
