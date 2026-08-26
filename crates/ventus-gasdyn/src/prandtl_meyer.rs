//! Prandtl-Meyer expansion. NACA 1135 eq. (171).
//!
//! `nu(M) = sqrt((g+1)/(g-1)) atan(sqrt((g-1)(M^2-1)/(g+1))) - atan(sqrt(M^2-1))`
//!
//! Parameterised in gamma (ADR-000 D10).

use crate::{check_gamma, check_supersonic, GasDynError};
use core::f64::consts::PI;

/// Prandtl-Meyer angle [rad]. Zero at M = 1, increasing thereafter.
///
/// # Errors
/// [`GasDynError::SubsonicUpstream`] below M = 1, where the function is undefined.
pub fn prandtl_meyer_rad(mach: f64, gamma: f64) -> Result<f64, GasDynError> {
    check_gamma(gamma)?;
    check_supersonic(mach)?;
    let m2m1 = mach * mach - 1.0;
    if m2m1 <= 0.0 {
        return Ok(0.0);
    }
    let k = libm::sqrt((gamma + 1.0) / (gamma - 1.0));
    Ok(k * libm::atan(libm::sqrt(m2m1) / k) - libm::atan(libm::sqrt(m2m1)))
}

/// The asymptotic limit of `nu` as M -> infinity [rad]:
/// `(pi/2)(sqrt((g+1)/(g-1)) - 1)`.
///
/// For air this is 130.454 degrees — the most a supersonic stream can turn
/// through an expansion before the static pressure reaches vacuum.
///
/// # Errors
/// [`GasDynError`] if gamma is not finite and greater than 1.
pub fn nu_max_rad(gamma: f64) -> Result<f64, GasDynError> {
    check_gamma(gamma)?;
    Ok(0.5 * PI * (libm::sqrt((gamma + 1.0) / (gamma - 1.0)) - 1.0))
}

/// Mach number from a Prandtl-Meyer angle: the inverse of [`prandtl_meyer_rad`].
///
/// `nu` is strictly increasing in M, so this is a bracketed root find. Newton
/// alone is not used: near M = 1 the derivative `dnu/dM` vanishes, and an
/// unguarded Newton step there flies off to infinity. This is bisection with a
/// Newton step accepted only when it stays inside the bracket, which keeps the
/// iteration count bounded and the result deterministic.
///
/// # Errors
/// [`GasDynError::BeyondPrandtlMeyerLimit`] if `nu` is at or beyond [`nu_max_rad`],
/// where no finite Mach number exists.
/// [`GasDynError::DidNotConverge`] if the iteration budget is exhausted, rather
/// than returning the last iterate.
pub fn mach_from_prandtl_meyer_rad(nu: f64, gamma: f64) -> Result<f64, GasDynError> {
    check_gamma(gamma)?;
    if nu.is_nan() {
        return Err(GasDynError::NotANumber);
    }
    if nu < 0.0 {
        return Err(GasDynError::BeyondPrandtlMeyerLimit);
    }
    if nu == 0.0 {
        return Ok(1.0);
    }
    let limit = nu_max_rad(gamma)?;
    if nu >= limit {
        return Err(GasDynError::BeyondPrandtlMeyerLimit);
    }

    let (mut lo, mut hi) = (1.0_f64, 2.0_f64);
    // Grow the upper bracket rather than starting from a huge number: this keeps
    // the bisection working on a tight interval for the common, moderate cases.
    while prandtl_meyer_rad(hi, gamma)? < nu {
        lo = hi;
        hi *= 2.0;
        if hi > 1e12 {
            return Err(GasDynError::DidNotConverge);
        }
    }

    let mut mach = 0.5 * (lo + hi);
    for _ in 0..200 {
        let f = prandtl_meyer_rad(mach, gamma)? - nu;
        if f > 0.0 {
            hi = mach;
        } else {
            lo = mach;
        }
        if hi - lo < 1e-15 * mach.max(1.0) {
            return Ok(0.5 * (lo + hi));
        }

        // dnu/dM = sqrt(M^2 - 1) / (M (1 + (g-1)/2 M^2))
        let m2m1 = mach * mach - 1.0;
        let newton = if m2m1 > 0.0 {
            let d = libm::sqrt(m2m1) / (mach * (1.0 + 0.5 * (gamma - 1.0) * mach * mach));
            if d > 0.0 {
                mach - f / d
            } else {
                f64::NAN
            }
        } else {
            f64::NAN
        };

        mach = if newton.is_finite() && newton > lo && newton < hi {
            newton
        } else {
            0.5 * (lo + hi)
        };
    }
    Err(GasDynError::DidNotConverge)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ventus_units::float::{abs, rel_err};

    const GAMMAS: [f64; 4] = [1.4, 1.304, 1.2, 5.0 / 3.0];

    #[test]
    fn nu_is_zero_at_mach_one_and_increases() {
        for g in GAMMAS {
            assert_eq!(prandtl_meyer_rad(1.0, g).unwrap(), 0.0);
            let mut previous = 0.0;
            let mut m = 1.0;
            while m <= 30.0 {
                let nu = prandtl_meyer_rad(m, g).unwrap();
                assert!(nu >= previous - 1e-15, "g={g} M={m}: nu decreased");
                assert!(
                    nu < nu_max_rad(g).unwrap(),
                    "g={g} M={m}: nu passed its limit"
                );
                previous = nu;
                m += 0.01;
            }
            assert_eq!(
                prandtl_meyer_rad(0.9, g),
                Err(GasDynError::SubsonicUpstream)
            );
        }
    }

    /// The limit for air, 130.454 degrees, is the one number here that is worth
    /// checking against a remembered value — it is quoted everywhere and is
    /// exactly (pi/2)(sqrt(6) - 1).
    #[test]
    fn nu_max_matches_the_closed_form_and_the_quoted_value_for_air() {
        let limit = nu_max_rad(1.4).unwrap();
        assert!(
            rel_err(limit.to_degrees(), 130.454) < 1e-5,
            "{}",
            limit.to_degrees()
        );
        assert!(abs(limit - 0.5 * PI * (libm::sqrt(6.0) - 1.0)) < 1e-15);
        // A softer gas can turn further before reaching vacuum.
        assert!(nu_max_rad(1.304).unwrap() > limit);
        assert!(nu_max_rad(5.0 / 3.0).unwrap() < limit);
    }

    /// nu approaches its limit from below and never reaches it.
    #[test]
    fn nu_approaches_its_limit_asymptotically() {
        for g in GAMMAS {
            let limit = nu_max_rad(g).unwrap();
            let far = prandtl_meyer_rad(1e7, g).unwrap();
            assert!(far < limit);
            assert!(rel_err(far, limit) < 1e-6, "g={g}: {far} vs {limit}");
        }
    }

    /// Round trip. The inverse is the part most likely to be subtly wrong, and
    /// this is the check that would catch it — including near M = 1 where the
    /// derivative vanishes and a bare Newton solve diverges.
    #[test]
    fn the_inverse_round_trips_across_the_whole_range() {
        for g in GAMMAS {
            for m in [
                1.0, 1.000_001, 1.001, 1.01, 1.1, 1.5, 2.0, 3.5, 5.0, 10.0, 100.0, 1000.0,
            ] {
                let nu = prandtl_meyer_rad(m, g).unwrap();
                let back = mach_from_prandtl_meyer_rad(nu, g).unwrap();
                assert!(
                    rel_err(back, m) < 1e-9,
                    "g={g}: M={m} -> nu={nu} -> M={back}"
                );
            }
        }
    }

    /// The other direction of the round trip, sweeping nu rather than M.
    #[test]
    fn the_forward_relation_round_trips_from_the_angle_side() {
        for g in GAMMAS {
            let limit = nu_max_rad(g).unwrap();
            for i in 1..200 {
                let nu = limit * f64::from(i) / 200.0;
                let m = mach_from_prandtl_meyer_rad(nu, g).unwrap();
                let back = prandtl_meyer_rad(m, g).unwrap();
                assert!(
                    abs(back - nu) < 1e-12,
                    "g={g} nu={nu} -> M={m} -> nu={back}"
                );
            }
        }
    }

    #[test]
    fn beyond_the_limit_is_refused_rather_than_saturated() {
        for g in GAMMAS {
            let limit = nu_max_rad(g).unwrap();
            assert_eq!(
                mach_from_prandtl_meyer_rad(limit, g),
                Err(GasDynError::BeyondPrandtlMeyerLimit)
            );
            assert_eq!(
                mach_from_prandtl_meyer_rad(limit * 1.001, g),
                Err(GasDynError::BeyondPrandtlMeyerLimit)
            );
            assert_eq!(
                mach_from_prandtl_meyer_rad(-0.1, g),
                Err(GasDynError::BeyondPrandtlMeyerLimit)
            );
            assert_eq!(
                mach_from_prandtl_meyer_rad(f64::NAN, g),
                Err(GasDynError::NotANumber)
            );
            assert_eq!(mach_from_prandtl_meyer_rad(0.0, g).unwrap(), 1.0);
        }
    }
}
