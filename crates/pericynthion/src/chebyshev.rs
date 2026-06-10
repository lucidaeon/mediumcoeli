//! Chebyshev polynomial evaluation on the canonical interval x ∈ \[−1, 1\].
//!
//! # Why Chebyshev?
//!
//! NASA JPL planetary ephemeris files (DE200 through DE441) store body
//! positions as piecewise Chebyshev polynomials of the first kind. Each
//! 32-day "granule" of a DE441 record holds, for each body, a vector of
//! coefficients (cₖ) that, when summed against the basis functions Tₖ(x),
//! reproduce the body's rectangular coordinates to within roughly a
//! kilometer over the granule's time window.
//!
//! The basis is chosen because it is *minimax optimal*: among all
//! polynomial bases of a given degree, expansions in Tₖ minimize the
//! worst-case error over the interval. That's exactly the property an
//! ephemeris team wants when fitting a smooth orbit to a 32-day window
//! they will never re-evaluate.
//!
//! # The basis, defined by recurrence
//!
//! ```text
//! T₀(x) = 1
//! T₁(x) = x
//! Tₙ(x) = 2x · Tₙ₋₁(x) − Tₙ₋₂(x)   for n ≥ 2
//! ```
//!
//! At x = 1, every Tₙ = 1. At x = −1, Tₙ = (−1)ⁿ. At x = 0, Tₙ alternates
//! 1, 0, −1, 0, 1, … . These identities are how the unit tests below
//! anchor the implementation to a verifiable truth instead of round-tripping
//! the algorithm against itself.
//!
//! # Why Clenshaw and not direct summation?
//!
//! Naively summing cₖ·Tₖ(x) for k = 0 … N−1 requires either materializing
//! all N basis values (memory) or recomputing them from the recurrence
//! (slow, and slightly less numerically stable due to growing rounding
//! errors at high n). The Clenshaw recurrence:
//!
//! ```text
//! b_{N+1} = 0
//! b_N     = 0
//! for k = N−1 down to 1:
//!     b_k = c_k + 2x · b_{k+1} − b_{k+2}
//! result = c_0 + x · b_1 − b_2
//! ```
//!
//! evaluates the same sum in a single backward pass with two scalar
//! registers, no allocation, and tighter numerical conditioning. It is
//! the canonical algorithm for evaluating any orthogonal polynomial
//! series; we use it for both the position series and its derivative.
//!
//! # The derivative series
//!
//! Velocity in DE441 is the time-derivative of position. Differentiating
//! ∑ cₖ·Tₖ(x) term-by-term gives a sum involving Uₙ (Chebyshev polynomials
//! of the second kind) because Tₙ'(x) = n · Uₙ₋₁(x). The simplest robust
//! approach is to **first convert** the Tₖ-coefficient series to its
//! derivative-as-Tₖ-coefficients form using the standard recurrence:
//!
//! ```text
//! c'_{N−1} = 0
//! c'_{N−2} = 2·(N−1)·c_{N−1}
//! for k = N−3 down to 0:
//!     c'_k = c'_{k+2} + 2·(k+1)·c_{k+1}
//! c'_0 /= 2
//! ```
//!
//! and then Clenshaw-evaluate the derivative coefficients in the same
//! T-basis. This keeps a single evaluator and avoids carrying a separate
//! U-basis implementation just for velocities.

/// Evaluate the Chebyshev-T series Σ_{k=0..N} `c_k` · `T_k(x)` at x ∈ \[−1, 1\].
///
/// Uses the Clenshaw backward recurrence. Returns 0.0 for an empty
/// coefficient slice (the empty sum). `x` is not clamped; callers
/// outside DE441 ephemeris use should respect the canonical interval to
/// avoid the rapid growth of Tₙ(|x| > 1).
///
/// # Examples
///
/// ```
/// use pericynthion::chebyshev::evaluate;
///
/// // T₀(x) = 1 everywhere
/// assert!((evaluate(&[1.0], 0.5) - 1.0).abs() < 1e-15);
///
/// // 3·T₀ + 2·T₁ + T₂ at x = 0.5
/// // = 3·1 + 2·0.5 + (2·0.25 − 1) = 3 + 1 − 0.5 = 3.5
/// assert!((evaluate(&[3.0, 2.0, 1.0], 0.5) - 3.5).abs() < 1e-15);
/// ```
#[must_use]
pub fn evaluate(coefficients: &[f64], x: f64) -> f64 {
    let n = coefficients.len();
    if n == 0 {
        return 0.0;
    }
    if n == 1 {
        return coefficients[0];
    }
    let two_x = 2.0 * x;
    let mut b_kp1 = 0.0_f64; // b_{k+1}
    let mut b_kp2 = 0.0_f64; // b_{k+2}
    for k in (1..n).rev() {
        let b_k = coefficients[k] + two_x * b_kp1 - b_kp2;
        b_kp2 = b_kp1;
        b_kp1 = b_k;
    }
    coefficients[0] + x * b_kp1 - b_kp2
}

/// Evaluate the derivative d/dx \[Σ `c_k` · `T_k(x)`\] at x ∈ \[−1, 1\].
///
/// Computes the derivative coefficients in the T basis via the standard
/// recurrence (see module docs), then Clenshaw-evaluates them. Allocates
/// one scratch vector of length `coefficients.len().saturating_sub(1)`.
/// Returns 0.0 for slices of length ≤ 1 (constant series have zero
/// derivative).
///
/// # Examples
///
/// ```
/// use pericynthion::chebyshev::evaluate_derivative;
///
/// // d/dx T₀ = 0
/// assert!(evaluate_derivative(&[1.0], 0.3).abs() < 1e-15);
///
/// // d/dx T₁ = 1
/// assert!((evaluate_derivative(&[0.0, 1.0], 0.3) - 1.0).abs() < 1e-15);
///
/// // d/dx T₂ = 4x;  at x = 0.5  →  2.0
/// assert!((evaluate_derivative(&[0.0, 0.0, 1.0], 0.5) - 2.0).abs() < 1e-15);
/// ```
#[must_use]
pub fn evaluate_derivative(coefficients: &[f64], x: f64) -> f64 {
    let n = coefficients.len();
    if n <= 1 {
        return 0.0;
    }
    // The original series has highest degree N = n − 1. Its derivative
    // is a series of highest degree N − 1, so the derivative-coefficient
    // vector `d` has length m = N = n − 1.
    let m = n - 1;
    let mut d = vec![0.0_f64; m];
    // Top coefficient: d[m−1] = 2·N·c[N]   (and N = m here).
    #[allow(clippy::cast_precision_loss)]
    {
        d[m - 1] = 2.0 * (m as f64) * coefficients[m];
        if m >= 2 {
            // Next: d[m−2] = 2·(N−1)·c[N−1].
            d[m - 2] = 2.0 * ((m - 1) as f64) * coefficients[m - 1];
        }
        // Walk down: d[k] = d[k+2] + 2·(k+1)·c[k+1]   for k = m−3 … 0.
        if m >= 3 {
            for k in (0..=m - 3).rev() {
                d[k] = d[k + 2] + 2.0 * ((k + 1) as f64) * coefficients[k + 1];
            }
        }
    }
    // Halve the d[0] term — the T-basis convention's standard correction.
    d[0] *= 0.5;
    evaluate(&d, x)
}

// =============================================================================
// Unit tests
// =============================================================================
//
// These tests anchor the implementation to mathematical truth in three ways:
//
//   1. Tₙ at canonical x values (x = ±1, 0, ±½) where Tₙ has closed-form
//      values that don't require any library to compute.
//   2. Clenshaw vs direct-sum-of-recurrence: two independent algorithms
//      should produce the same answer to machine precision.
//   3. Derivative identities: dTₙ/dx has known closed forms for small n
//      (T₀' = 0, T₁' = 1, T₂' = 4x, T₃' = 12x² − 3, …) that we hard-code.
//
// If any of these fail, the implementation is wrong, not the test.

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;

    /// Direct evaluation of `T_n(x)` via the textbook three-term recurrence.
    /// Slow (O(n) per call, O(n²) in tests below), but algorithmically
    /// independent of [`evaluate`] — that's the whole point.
    fn t_n(n: usize, x: f64) -> f64 {
        match n {
            0 => 1.0,
            1 => x,
            _ => {
                let mut tnm2 = 1.0_f64;
                let mut tnm1 = x;
                let mut tn = 0.0_f64;
                for _ in 2..=n {
                    tn = 2.0 * x * tnm1 - tnm2;
                    tnm2 = tnm1;
                    tnm1 = tn;
                }
                tn
            }
        }
    }

    #[test]
    fn t_n_known_values_at_x_equals_one() {
        // T_n(1) = 1 for all n.
        for n in 0..20 {
            assert_abs_diff_eq!(t_n(n, 1.0), 1.0, epsilon = 1e-14);
        }
    }

    #[test]
    fn t_n_known_values_at_x_equals_minus_one() {
        // T_n(-1) = (-1)^n.
        for n in 0..20 {
            let expected = if n % 2 == 0 { 1.0 } else { -1.0 };
            assert_abs_diff_eq!(t_n(n, -1.0), expected, epsilon = 1e-14);
        }
    }

    #[test]
    fn t_n_known_values_at_x_equals_zero() {
        // T_n(0): 1, 0, -1, 0, 1, 0, -1, ...
        let expected = [1.0, 0.0, -1.0, 0.0, 1.0, 0.0, -1.0, 0.0, 1.0, 0.0];
        for (n, &e) in expected.iter().enumerate() {
            assert_abs_diff_eq!(t_n(n, 0.0), e, epsilon = 1e-14);
        }
    }

    #[test]
    fn t_n_known_values_at_x_equals_half() {
        // T_n(1/2) closed forms:
        //   T_0 = 1, T_1 = 1/2, T_2 = -1/2, T_3 = -1, T_4 = -1/2,
        //   T_5 = 1/2, T_6 = 1, T_7 = 1/2, ... (period 6)
        let expected = [1.0, 0.5, -0.5, -1.0, -0.5, 0.5, 1.0, 0.5, -0.5, -1.0];
        for (n, &e) in expected.iter().enumerate() {
            assert_abs_diff_eq!(t_n(n, 0.5), e, epsilon = 1e-14);
        }
    }

    #[test]
    fn evaluate_empty_is_zero() {
        assert!((evaluate(&[], 0.5) - 0.0).abs() < 1e-15);
        assert!((evaluate(&[], -1.0) - 0.0).abs() < 1e-15);
    }

    #[test]
    fn evaluate_constant_series() {
        for &c in &[-3.0, -1.0, 0.0, 0.25, 7.5] {
            for &x in &[-1.0, -0.5, 0.0, 0.5, 1.0] {
                assert_abs_diff_eq!(evaluate(&[c], x), c, epsilon = 1e-14);
            }
        }
    }

    #[test]
    fn evaluate_matches_direct_sum_for_single_term_series() {
        // For each n in 0..10, the series with a single 1.0 in slot n
        // must equal T_n(x).
        for n in 0..10 {
            let mut coeffs = vec![0.0_f64; n + 1];
            coeffs[n] = 1.0;
            for &x in &[-1.0, -0.75, -0.5, -0.25, 0.0, 0.25, 0.5, 0.75, 1.0] {
                let got = evaluate(&coeffs, x);
                let expected = t_n(n, x);
                assert_abs_diff_eq!(got, expected, epsilon = 1e-13);
            }
        }
    }

    #[test]
    fn evaluate_matches_direct_sum_for_mixed_series() {
        // Mixed series: c = [3, -2, 1, 0.5, -0.25]
        // Compare Clenshaw vs explicit sum of c_k * T_k(x).
        let coeffs = [3.0_f64, -2.0, 1.0, 0.5, -0.25];
        for &x in &[-1.0, -0.9, -0.5, -0.1, 0.0, 0.1, 0.5, 0.9, 1.0] {
            let clenshaw = evaluate(&coeffs, x);
            let direct: f64 = coeffs.iter().enumerate().map(|(k, &c)| c * t_n(k, x)).sum();
            assert_abs_diff_eq!(clenshaw, direct, epsilon = 1e-13);
        }
    }

    #[test]
    fn derivative_of_constant_is_zero() {
        for &c in &[-2.0, 0.0, 3.0] {
            for &x in &[-1.0, -0.5, 0.0, 0.5, 1.0] {
                assert_abs_diff_eq!(evaluate_derivative(&[c], x), 0.0, epsilon = 1e-14);
            }
        }
    }

    #[test]
    fn derivative_of_t1_is_one() {
        // T_1(x) = x, so d/dx = 1 everywhere.
        for &x in &[-1.0, -0.5, 0.0, 0.5, 1.0] {
            assert_abs_diff_eq!(evaluate_derivative(&[0.0, 1.0], x), 1.0, epsilon = 1e-14);
        }
    }

    #[test]
    fn derivative_of_t2_is_four_x() {
        // T_2(x) = 2x² − 1, so d/dx = 4x.
        for &x in &[-1.0, -0.5, 0.0, 0.5, 1.0] {
            assert_abs_diff_eq!(
                evaluate_derivative(&[0.0, 0.0, 1.0], x),
                4.0 * x,
                epsilon = 1e-14
            );
        }
    }

    #[test]
    fn derivative_of_t3_is_twelve_x_squared_minus_three() {
        // T_3(x) = 4x³ − 3x, so d/dx = 12x² − 3.
        for &x in &[-1.0, -0.5, 0.0, 0.5, 1.0] {
            let expected = 12.0 * x * x - 3.0;
            assert_abs_diff_eq!(
                evaluate_derivative(&[0.0, 0.0, 0.0, 1.0], x),
                expected,
                epsilon = 1e-13
            );
        }
    }

    #[test]
    fn derivative_of_mixed_series_via_numeric_check() {
        // Independent verification by central difference on `evaluate` itself.
        // The central difference is a separate algorithm path (no derivative
        // coefficients), so agreement is meaningful.
        let coeffs = [3.0_f64, -2.0, 1.0, 0.5, -0.25, 0.1];
        let h = 1e-6_f64;
        for &x in &[-0.9, -0.5, -0.1, 0.0, 0.1, 0.5, 0.9] {
            let analytic = evaluate_derivative(&coeffs, x);
            let numeric = (evaluate(&coeffs, x + h) - evaluate(&coeffs, x - h)) / (2.0 * h);
            assert_abs_diff_eq!(analytic, numeric, epsilon = 1e-7);
        }
    }
}
