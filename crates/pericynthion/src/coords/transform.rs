//! Generic 3-vector rotation primitives and the equatorial→ecliptic
//! conversion.
//!
//! All rotations follow the right-hand-rule convention: a positive
//! angle around the X axis rotates the Y axis toward the Z axis. This
//! matches the IAU/JPL convention used throughout the rest of the
//! pipeline.

/// 3×3 rotation matrix, row-major.
pub type Matrix3 = [[f64; 3]; 3];

/// 3-vector.
pub type Vector3 = [f64; 3];

/// Identity matrix.
#[must_use]
pub const fn identity() -> Matrix3 {
    [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
}

/// Rotation matrix around the X axis by `angle_rad` (right-hand rule).
#[must_use]
pub fn rotate_x(angle_rad: f64) -> Matrix3 {
    let c = angle_rad.cos();
    let s = angle_rad.sin();
    [[1.0, 0.0, 0.0], [0.0, c, s], [0.0, -s, c]]
}

/// Rotation matrix around the Y axis by `angle_rad` (right-hand rule).
#[must_use]
pub fn rotate_y(angle_rad: f64) -> Matrix3 {
    let c = angle_rad.cos();
    let s = angle_rad.sin();
    [[c, 0.0, -s], [0.0, 1.0, 0.0], [s, 0.0, c]]
}

/// Rotation matrix around the Z axis by `angle_rad` (right-hand rule).
#[must_use]
pub fn rotate_z(angle_rad: f64) -> Matrix3 {
    let c = angle_rad.cos();
    let s = angle_rad.sin();
    [[c, s, 0.0], [-s, c, 0.0], [0.0, 0.0, 1.0]]
}

/// Matrix × vector product.
#[must_use]
pub fn apply(m: &Matrix3, v: &Vector3) -> Vector3 {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

/// Matrix × matrix product (returns `a · b`).
#[must_use]
pub fn multiply(a: &Matrix3, b: &Matrix3) -> Matrix3 {
    let mut out = [[0.0_f64; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            out[i][j] = a[i][0] * b[0][j] + a[i][1] * b[1][j] + a[i][2] * b[2][j];
        }
    }
    out
}

/// Convert an equatorial 3-vector to ecliptic coordinates by rotating
/// around the X axis by `+obliquity_rad`.
///
/// Equatorial Z is Earth's rotation axis; ecliptic Z is normal to the
/// orbital plane. The rotation that takes equatorial → ecliptic is
/// `Rx(+ε)`. (Some references use `Rx(−ε)` depending on whether the
/// rotation is "of the axes" or "of the vector"; this implementation
/// uses the standard astronomical convention.)
#[must_use]
pub fn equatorial_to_ecliptic(v: &Vector3, obliquity_rad: f64) -> Vector3 {
    apply(&rotate_x(obliquity_rad), v)
}

/// Magnitude (Euclidean norm) of a 3-vector.
#[must_use]
pub fn magnitude(v: &Vector3) -> f64 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

/// Ecliptic longitude (radians, in `[0, 2π)`) of an ecliptic-frame
/// 3-vector, via `atan2(y, x)`.
#[must_use]
pub fn longitude_rad(v: &Vector3) -> f64 {
    let lon = v[1].atan2(v[0]);
    if lon < 0.0 {
        lon + 2.0 * std::f64::consts::PI
    } else {
        lon
    }
}

/// Ecliptic latitude (radians) of an ecliptic-frame 3-vector.
#[must_use]
pub fn latitude_rad(v: &Vector3) -> f64 {
    let r = magnitude(v);
    if r == 0.0 { 0.0 } else { (v[2] / r).asin() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;
    use std::f64::consts::PI;

    fn vec_close(a: &Vector3, b: &Vector3, eps: f64) {
        for i in 0..3 {
            assert!((a[i] - b[i]).abs() < eps, "axis {i}: {a:?} vs {b:?}");
        }
    }

    #[test]
    fn identity_matrix_preserves_vectors() {
        let id = identity();
        let v = [1.0, 2.0, 3.0];
        vec_close(&apply(&id, &v), &v, 1e-15);
    }

    #[test]
    fn rotate_x_by_zero_is_identity() {
        let m = rotate_x(0.0);
        assert_abs_diff_eq!(m[0][0], 1.0, epsilon = 1e-15);
        assert_abs_diff_eq!(m[1][1], 1.0, epsilon = 1e-15);
        assert_abs_diff_eq!(m[2][2], 1.0, epsilon = 1e-15);
    }

    #[test]
    fn rotate_x_by_90_takes_y_to_z() {
        let m = rotate_x(PI / 2.0);
        let v = [0.0, 1.0, 0.0];
        vec_close(&apply(&m, &v), &[0.0, 0.0, -1.0], 1e-15);
    }

    #[test]
    fn rotate_z_by_90_takes_x_to_y() {
        let m = rotate_z(PI / 2.0);
        let v = [1.0, 0.0, 0.0];
        vec_close(&apply(&m, &v), &[0.0, -1.0, 0.0], 1e-15);
    }

    #[test]
    fn matrix_multiply_is_associative_with_apply() {
        let a = rotate_z(0.3);
        let b = rotate_x(0.5);
        let v = [1.0, 2.0, 3.0];
        let direct = apply(&a, &apply(&b, &v));
        let combined = apply(&multiply(&a, &b), &v);
        vec_close(&direct, &combined, 1e-14);
    }

    #[test]
    fn equatorial_to_ecliptic_x_axis_is_invariant() {
        let v = [1.0, 0.0, 0.0];
        let ec = equatorial_to_ecliptic(&v, 0.409_092_8);
        vec_close(&ec, &v, 1e-15);
    }

    #[test]
    fn longitude_of_pure_x_vector_is_zero() {
        assert_abs_diff_eq!(longitude_rad(&[1.0, 0.0, 0.0]), 0.0, epsilon = 1e-15);
    }

    #[test]
    fn longitude_of_pure_y_vector_is_pi_over_2() {
        assert_abs_diff_eq!(longitude_rad(&[0.0, 1.0, 0.0]), PI / 2.0, epsilon = 1e-15);
    }

    #[test]
    fn longitude_of_minus_y_vector_is_three_pi_over_2() {
        // -y is at longitude 270° = 3π/2 (we normalize to [0, 2π)).
        assert_abs_diff_eq!(
            longitude_rad(&[0.0, -1.0, 0.0]),
            3.0 * PI / 2.0,
            epsilon = 1e-15
        );
    }

    #[test]
    fn latitude_of_equatorial_vector_is_zero() {
        assert_abs_diff_eq!(latitude_rad(&[1.0, 0.0, 0.0]), 0.0, epsilon = 1e-15);
    }

    #[test]
    fn latitude_of_pole_vector_is_pi_over_2() {
        assert_abs_diff_eq!(latitude_rad(&[0.0, 0.0, 1.0]), PI / 2.0, epsilon = 1e-15);
    }
}
