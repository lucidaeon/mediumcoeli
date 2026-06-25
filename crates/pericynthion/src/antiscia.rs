//! Antiscia and contra-antiscia — pure reflection functions.
//!
//! Both reflections are involutions on the ecliptic (applying the same
//! reflection twice returns the original longitude). They compose to a simple
//! point opposition (180° shift).
//!
//! * **Antiscion** reflects across the Cancer/Capricorn (solstice) axis:
//!   the axis of equal solar declination. Two planets in antiscion occupy
//!   mirror positions around 0°Cancer/0°Capricorn and share the same
//!   declination.
//!
//! * **Contra-antiscion** reflects across the Aries/Libra (equinox) axis:
//!   two planets in contra-antiscion share equal but opposite declinations.

/// Returns the antiscion of an ecliptic longitude.
///
/// Reflects across the Cancer/Capricorn (solstice) axis, i.e.
/// `(180° − λ) mod 360°`. Two planets in antiscion share the same solar
/// declination on opposite sides of 0°Cancer/0°Capricorn.
///
/// # Arguments
/// * `lon_deg` — ecliptic longitude in degrees \[0, 360).
///
/// # Returns
/// The reflected longitude in degrees \[0, 360).
#[must_use]
pub fn antiscion(lon_deg: f64) -> f64 {
    (180.0 - lon_deg).rem_euclid(360.0)
}

/// Returns the contra-antiscion of an ecliptic longitude.
///
/// Reflects across the Aries/Libra (equinox) axis, i.e.
/// `(360° − λ) mod 360°` (equivalently `(−λ) mod 360°`). Two planets in
/// contra-antiscion share equal but opposite solar declinations.
///
/// # Arguments
/// * `lon_deg` — ecliptic longitude in degrees \[0, 360).
///
/// # Returns
/// The reflected longitude in degrees \[0, 360).
#[must_use]
pub fn contra_antiscion(lon_deg: f64) -> f64 {
    (360.0 - lon_deg).rem_euclid(360.0)
}

#[cfg(test)]
mod tests {
    use approx::assert_abs_diff_eq;

    use super::{antiscion, contra_antiscion};

    #[test]
    fn antiscion_reflects_solstice_axis() {
        assert_abs_diff_eq!(antiscion(0.0), 180.0, epsilon = 1e-12);
        assert_abs_diff_eq!(antiscion(90.0), 90.0, epsilon = 1e-12);
        assert_abs_diff_eq!(antiscion(23.0), 157.0, epsilon = 1e-12);
        assert_abs_diff_eq!(antiscion(180.0), 0.0, epsilon = 1e-12);
    }

    #[test]
    fn contra_antiscion_reflects_equinox_axis() {
        assert_abs_diff_eq!(contra_antiscion(0.0), 0.0, epsilon = 1e-12);
        assert_abs_diff_eq!(contra_antiscion(90.0), 270.0, epsilon = 1e-12);
        assert_abs_diff_eq!(contra_antiscion(23.0), 337.0, epsilon = 1e-12);
        assert_abs_diff_eq!(contra_antiscion(180.0), 180.0, epsilon = 1e-12);
    }

    #[test]
    fn double_reflection_is_identity() {
        for lambda in [0.0_f64, 23.5, 90.0, 137.0, 200.0, 359.9] {
            assert_abs_diff_eq!(antiscion(antiscion(lambda)), lambda, epsilon = 1e-9);
            assert_abs_diff_eq!(
                contra_antiscion(contra_antiscion(lambda)),
                lambda,
                epsilon = 1e-9
            );
        }
    }

    #[test]
    fn antiscion_then_contra_is_180_shift() {
        for lambda in [0.0_f64, 23.5, 90.0, 137.0, 200.0, 359.9] {
            assert_abs_diff_eq!(
                contra_antiscion(antiscion(lambda)),
                (lambda + 180.0).rem_euclid(360.0),
                epsilon = 1e-9
            );
        }
    }
}
