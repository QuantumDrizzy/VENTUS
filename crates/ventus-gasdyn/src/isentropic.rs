//! Isentropic relations. NACA 1135 eqs. (43)-(46), (80).
//!
//! All parameterised in gamma (ADR-000 D10).
//!
//! [KNOWN_LIMIT] These assume a calorically perfect gas. At the VENTUS-1 design
//! point the stagnation state is ~753 K, where cp has risen about 8 % above its
//! cold value and gamma has fallen to ~1.358. Using gamma = 1.4 there
//! over-predicts T0 by 2.0 %. See `cases/gamma_validity.toml`.

use crate::{check_gamma, check_mach, GasDynError};

/// T0/T = 1 + (gamma-1)/2 * M^2. NACA 1135 eq. (43).
///
/// # Errors
/// [`GasDynError`] for NaN, negative Mach, or gamma <= 1.
pub fn stagnation_temperature_ratio(mach: f64, gamma: f64) -> Result<f64, GasDynError> {
    check_gamma(gamma)?;
    check_mach(mach)?;
    Ok(1.0 + 0.5 * (gamma - 1.0) * mach * mach)
}

/// p0/p = (1 + (gamma-1)/2 M^2)^(gamma/(gamma-1)). NACA 1135 eq. (44).
///
/// # Errors
/// [`GasDynError`] for NaN, negative Mach, or gamma <= 1.
pub fn stagnation_pressure_ratio(mach: f64, gamma: f64) -> Result<f64, GasDynError> {
    let t = stagnation_temperature_ratio(mach, gamma)?;
    Ok(libm::pow(t, gamma / (gamma - 1.0)))
}

/// rho0/rho = (1 + (gamma-1)/2 M^2)^(1/(gamma-1)). NACA 1135 eq. (45).
///
/// # Errors
/// [`GasDynError`] for NaN, negative Mach, or gamma <= 1.
pub fn stagnation_density_ratio(mach: f64, gamma: f64) -> Result<f64, GasDynError> {
    let t = stagnation_temperature_ratio(mach, gamma)?;
    Ok(libm::pow(t, 1.0 / (gamma - 1.0)))
}

/// A/A*, the isentropic area ratio. NACA 1135 eq. (80).
///
/// Singular at M = 0, where the area required to decelerate the flow to rest is
/// infinite; refused rather than returned as an infinity.
///
/// # Errors
/// [`GasDynError`] for NaN, negative Mach, gamma <= 1, or M = 0.
pub fn area_ratio(mach: f64, gamma: f64) -> Result<f64, GasDynError> {
    check_gamma(gamma)?;
    check_mach(mach)?;
    if mach == 0.0 {
        return Err(GasDynError::SubsonicUpstream);
    }
    let t = 1.0 + 0.5 * (gamma - 1.0) * mach * mach;
    let exponent = 0.5 * (gamma + 1.0) / (gamma - 1.0);
    Ok(libm::pow(2.0 / (gamma + 1.0) * t, exponent) / mach)
}

/// The characteristic Mach number M* = V/a*, where a* is the speed of sound at
/// sonic conditions. NACA 1135 eq. (47).
///
/// This exists because of the Prandtl relation: across a normal shock,
/// M1* . M2* = 1 exactly. That identity is the strongest available check on the
/// shock relations, because it depends on no tabulated value.
///
/// # Errors
/// [`GasDynError`] for NaN, negative Mach, or gamma <= 1.
pub fn characteristic_mach(mach: f64, gamma: f64) -> Result<f64, GasDynError> {
    check_gamma(gamma)?;
    check_mach(mach)?;
    // Written as (g+1) / ((g-1) + 2/M^2) rather than (g+1)M^2 / (2 + (g-1)M^2).
    // The two are identical algebraically, but M^2 overflows to infinity above
    // M ~ 1.3e154 and the direct form then evaluates inf/inf = NaN. A NaN here
    // would propagate silently into the Prandtl-relation check, which is the
    // strongest validation this module has.
    let inv_m2 = 1.0 / (mach * mach);
    Ok(libm::sqrt((gamma + 1.0) / ((gamma - 1.0) + 2.0 * inv_m2)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ventus_units::float::{abs, rel_err};

    const GAMMAS: [f64; 4] = [1.4, 1.304, 1.2, 5.0 / 3.0];

    /// Identity, no table needed: p0/p must equal (T0/T)^(g/(g-1)) exactly, and
    /// rho0/rho must equal (T0/T)^(1/(g-1)). If any of the three drifts apart,
    /// one of them is wrong.
    #[test]
    fn the_three_stagnation_ratios_are_mutually_consistent() {
        for g in GAMMAS {
            for m in [0.0, 0.3, 0.8, 1.0, 1.5, 2.0, 3.5, 8.0] {
                let t = stagnation_temperature_ratio(m, g).unwrap();
                let p = stagnation_pressure_ratio(m, g).unwrap();
                let r = stagnation_density_ratio(m, g).unwrap();
                assert!(
                    rel_err(p, libm::pow(t, g / (g - 1.0))) < 1e-14,
                    "g={g} m={m}"
                );
                assert!(
                    rel_err(r, libm::pow(t, 1.0 / (g - 1.0))) < 1e-14,
                    "g={g} m={m}"
                );
                // The gas law closes the triangle: p0/p = (rho0/rho)(T0/T).
                assert!(
                    rel_err(p, r * t) < 1e-13,
                    "g={g} m={m}: p0/p != rho0/rho * T0/T"
                );
            }
        }
    }

    #[test]
    fn everything_is_unity_at_zero_mach_and_the_throat_is_sonic() {
        for g in GAMMAS {
            assert_eq!(stagnation_temperature_ratio(0.0, g).unwrap(), 1.0);
            assert_eq!(stagnation_pressure_ratio(0.0, g).unwrap(), 1.0);
            assert_eq!(stagnation_density_ratio(0.0, g).unwrap(), 1.0);
            // A/A* = 1 exactly at M = 1, for every gamma. This is the definition
            // of A*, so any deviation is an algebra error.
            assert!(rel_err(area_ratio(1.0, g).unwrap(), 1.0) < 1e-14, "g={g}");
            assert!(
                rel_err(characteristic_mach(1.0, g).unwrap(), 1.0) < 1e-14,
                "g={g}"
            );
        }
    }

    /// A/A* has a minimum at M = 1 and rises on both sides. A sign error in the
    /// exponent would invert this and nothing else would notice.
    #[test]
    fn area_ratio_is_minimum_at_the_throat() {
        for g in GAMMAS {
            let at_throat = area_ratio(1.0, g).unwrap();
            for m in [0.05, 0.2, 0.5, 0.9, 0.99, 1.01, 1.5, 3.5, 10.0] {
                assert!(
                    area_ratio(m, g).unwrap() > at_throat,
                    "g={g} m={m}: A/A* below the throat value"
                );
            }
            assert_eq!(area_ratio(0.0, g), Err(GasDynError::SubsonicUpstream));
        }
    }

    /// M* saturates at sqrt((g+1)/(g-1)) as M -> infinity. For gamma = 1.4 that
    /// is sqrt(6) = 2.449490, a number that appears again in the Prandtl-Meyer
    /// limit — the two are the same constant seen from different directions.
    #[test]
    fn characteristic_mach_saturates_at_the_expected_limit() {
        for g in GAMMAS {
            let limit = libm::sqrt((g + 1.0) / (g - 1.0));
            let far = characteristic_mach(1e8, g).unwrap();
            assert!(rel_err(far, limit) < 1e-12, "g={g}: {far} vs {limit}");
            assert!(characteristic_mach(1e300, g).unwrap() <= limit * (1.0 + 1e-12));
        }
        assert!(abs(libm::sqrt(2.4 / 0.4) - 2.449_489_742_783_178) < 1e-12);
    }

    /// gamma is a real parameter, not decoration: different gases must give
    /// materially different answers at the same Mach number.
    #[test]
    fn gamma_actually_changes_the_answer() {
        let m = 3.5;
        let air = stagnation_temperature_ratio(m, 1.4).unwrap();
        let burner = stagnation_temperature_ratio(m, 1.304).unwrap();
        let monatomic = stagnation_temperature_ratio(m, 5.0 / 3.0).unwrap();
        assert!(air > burner, "lower gamma must give a lower T0/T");
        assert!(monatomic > air);
        // Not a rounding-level difference: T0/T is 3.45 for air, 3.11 for the
        // burner gas, 4.08 monatomic.
        assert!(rel_err(air, burner) > 0.09, "{air} vs {burner}");
    }
}
