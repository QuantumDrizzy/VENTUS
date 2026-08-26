//! Floating-point comparison primitives shared by the validation harness.
//!
//! Why these are hand-rolled: `f64::abs`, `f64::sqrt` and friends are inherent
//! methods provided by `std`, not by `core`. A `no_std` crate cannot call them.
//! Bit masking is exact, branch-free and portable, so it is used instead of
//! pulling in `libm` for something this small.
//!
//! [KNOWN_LIMIT] M1 will need `powf` for the barometric formula, which cannot
//! be hand-rolled sensibly. The decision (`libm` crate vs. a `std`-only core
//! plus a separate FSW build) is deferred to the start of M1 and must be
//! recorded as an ADR amendment, not decided silently in a commit.

/// Absolute value, by clearing the sign bit. Exact for every input including
/// -0.0 (returns +0.0) and NaN (returns a positive NaN).
#[inline]
#[must_use]
pub fn abs(x: f64) -> f64 {
    f64::from_bits(x.to_bits() & 0x7fff_ffff_ffff_ffff)
}

/// Maps an `f64` onto a `u64` that preserves ordering, so that the unsigned
/// difference between two mapped values is their distance in ULPs.
///
/// DELIBERATE DEVIATION from IEEE-754 `totalOrder`, which ranks `-0.0` strictly
/// below `+0.0` and would therefore report them as 1 ULP apart. In a validation
/// harness that is pure noise: an expected `0.0` against a computed `-0.0` is a
/// match, not a near-miss. Both zeros are collapsed onto the same key.
/// Everything else follows `totalOrder` exactly.
#[inline]
fn monotone_key(x: f64) -> u64 {
    const ZERO: u64 = 0x8000_0000_0000_0000; // where both signed zeros land
    let bits = x.to_bits();
    let magnitude = bits & 0x7fff_ffff_ffff_ffff;
    if bits & 0x8000_0000_0000_0000 != 0 {
        ZERO - magnitude
    } else {
        ZERO + magnitude
    }
}

/// Distance between two floats in units in the last place.
///
/// Returns `None` if either input is NaN — a NaN comparison has no meaningful
/// ULP distance, and silently returning 0 or u64::MAX would hide a bug.
/// Adjacent floats are 1 ULP apart; `+0.0` and `-0.0` are 0 ULPs apart.
#[inline]
#[must_use]
pub fn ulp_diff(a: f64, b: f64) -> Option<u64> {
    if a.is_nan() || b.is_nan() {
        return None;
    }
    Some(monotone_key(a).abs_diff(monotone_key(b)))
}

/// Relative error of `actual` against `expected`.
///
/// When `expected` is exactly zero the relative error is undefined, so this
/// degenerates to the absolute error. Callers comparing against zero should
/// supply an absolute tolerance instead of relying on that fallback.
/// Returns `f64::INFINITY` if either input is NaN, so a NaN never passes a
/// tolerance check by accident.
#[inline]
#[must_use]
pub fn rel_err(actual: f64, expected: f64) -> f64 {
    if actual.is_nan() || expected.is_nan() {
        return f64::INFINITY;
    }
    let diff = abs(actual - expected);
    if expected == 0.0 {
        diff
    } else {
        diff / abs(expected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abs_handles_signed_zero_and_infinity() {
        assert_eq!(abs(-0.0).to_bits(), 0.0_f64.to_bits());
        assert_eq!(abs(-3.5), 3.5);
        assert_eq!(abs(f64::NEG_INFINITY), f64::INFINITY);
        assert!(abs(f64::NAN).is_nan());
    }

    #[test]
    fn ulp_diff_counts_adjacent_floats() {
        let a = 1.0_f64;
        let b = f64::from_bits(a.to_bits() + 1);
        assert_eq!(ulp_diff(a, b), Some(1));
        assert_eq!(ulp_diff(a, a), Some(0));
        // Signed zeros are the same number. This is a deliberate deviation from
        // IEEE-754 totalOrder, which would report 1 ULP here; see monotone_key.
        assert_eq!(ulp_diff(0.0, -0.0), Some(0));
        // Collapsing the zeros must not distort the neighbourhood: with +-0
        // occupying a single slot, the three consecutive floats
        // (-min_subnormal, 0, +min_subnormal) must measure 1, 1 and 2 apart.
        let pos_min_sub = f64::from_bits(1);
        let neg_min_sub = f64::from_bits(0x8000_0000_0000_0001);
        assert_eq!(ulp_diff(-0.0, pos_min_sub), Some(1));
        assert_eq!(ulp_diff(0.0, pos_min_sub), Some(1));
        assert_eq!(ulp_diff(neg_min_sub, 0.0), Some(1));
        assert_eq!(ulp_diff(neg_min_sub, pos_min_sub), Some(2));
        // Ordering is preserved across the whole range.
        assert_eq!(
            ulp_diff(f64::NEG_INFINITY, f64::INFINITY),
            Some(2 * 0x7ff0_0000_0000_0000)
        );
    }

    #[test]
    fn nan_never_passes() {
        assert_eq!(ulp_diff(f64::NAN, 1.0), None);
        assert_eq!(rel_err(f64::NAN, 1.0), f64::INFINITY);
        assert_eq!(rel_err(1.0, f64::NAN), f64::INFINITY);
    }

    #[test]
    fn rel_err_degenerates_to_absolute_at_zero() {
        assert_eq!(rel_err(1e-9, 0.0), 1e-9);
        assert!(rel_err(216.66, 216.65) - 4.6e-5 < 1e-6);
    }
}
