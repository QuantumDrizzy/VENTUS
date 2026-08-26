//! Normal and oblique shocks. NACA 1135 eqs. (93)-(96), (138), (150).
//!
//! All parameterised in gamma (ADR-000 D10).
//!
//! [KNOWN_LIMIT] Calorically perfect gas. At the VENTUS-1 design point the
//! post-shock temperature is 738 K under this assumption; solving the same jump
//! with real enthalpy gives 726.3 K, 1.6 % lower, because a calorically
//! imperfect gas absorbs part of the compression into vibrational modes and so
//! heats less and compresses more. Quantified in `cases/gamma_validity.toml`.

use crate::isentropic::characteristic_mach;
use crate::{check_gamma, check_supersonic, GasDynError};

use core::f64::consts::PI;

/// State change across a normal shock. Every field is a downstream/upstream
/// ratio except `mach_downstream`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NormalShock {
    pub mach_upstream: f64,
    pub mach_downstream: f64,
    /// p2/p1
    pub pressure_ratio: f64,
    /// T2/T1
    pub temperature_ratio: f64,
    /// rho2/rho1
    pub density_ratio: f64,
    /// p02/p01 — the loss that makes supersonic inlets hard.
    pub stagnation_pressure_ratio: f64,
}

impl NormalShock {
    /// Entropy rise across the shock, in units of R: `ds/R = -ln(p02/p01)`.
    ///
    /// The second law makes this non-negative, and zero only for a shock of
    /// vanishing strength. It is the cheapest available sanity check on the
    /// stagnation-pressure ratio.
    #[must_use]
    pub fn entropy_rise_over_r(&self) -> f64 {
        -libm::log(self.stagnation_pressure_ratio)
    }
}

/// Normal shock relations.
///
/// # Errors
/// [`GasDynError::SubsonicUpstream`] below M = 1: a normal shock cannot exist
/// there, and the algebra would return a spurious "expansion shock" that
/// violates the second law.
pub fn normal_shock(mach_upstream: f64, gamma: f64) -> Result<NormalShock, GasDynError> {
    check_gamma(gamma)?;
    check_supersonic(mach_upstream)?;

    let m2 = mach_upstream * mach_upstream;
    let gp1 = gamma + 1.0;
    let gm1 = gamma - 1.0;

    // Ratios that are finite as M -> infinity are written divided through by
    // M^2, so they stay correct past the point where M^2 itself overflows
    // (M ~ 1.3e154). The direct forms give inf/inf = NaN there.
    let inv_m2 = 1.0 / m2;

    // NACA 1135 eq. (96)
    let mach_downstream = libm::sqrt((inv_m2 + 0.5 * gm1) / (gamma - 0.5 * gm1 * inv_m2));
    // eq. (93). This one genuinely diverges, so it is left as written.
    let pressure_ratio = (2.0 * gamma * m2 - gm1) / gp1;
    // eq. (94)
    let density_ratio = gp1 / (gm1 + 2.0 * inv_m2);
    // eq. (95). Derived from the other two through the gas law rather than
    // written out again, so the three cannot drift apart.
    let temperature_ratio = pressure_ratio / density_ratio;
    // eq. (99)
    let stagnation_pressure_ratio = libm::pow(density_ratio, gamma / gm1)
        * libm::pow(gp1 / (2.0 * gamma * m2 - gm1), 1.0 / gm1);

    Ok(NormalShock {
        mach_upstream,
        mach_downstream,
        pressure_ratio,
        temperature_ratio,
        density_ratio,
        stagnation_pressure_ratio,
    })
}

/// Mach angle mu = asin(1/M) [rad]. The wave angle of a vanishingly weak shock.
///
/// # Errors
/// [`GasDynError::SubsonicUpstream`] below M = 1, where no Mach wave exists.
pub fn mach_angle_rad(mach: f64) -> Result<f64, GasDynError> {
    check_supersonic(mach)?;
    Ok(libm::asin(1.0 / mach))
}

/// Which root of the theta-beta-M relation to take.
///
/// For any deflection below the maximum there are two attached-shock solutions.
/// The weak one leaves the flow supersonic in almost all cases and is what
/// occurs in practice unless downstream conditions force otherwise; the strong
/// one leaves it subsonic. An inlet designer needs both: the weak solutions
/// build the compression ramp, the strong one is the terminal shock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShockBranch {
    Weak,
    Strong,
}

/// Flow deflection produced by a shock at wave angle `beta`. NACA 1135 eq. (138).
///
/// `tan(theta) = 2 cot(beta) (M^2 sin^2(beta) - 1) / (M^2 (gamma + cos 2beta) + 2)`
///
/// # Errors
/// [`GasDynError`] for NaN, subsonic Mach, or gamma <= 1.
pub fn deflection_from_wave_angle_rad(
    mach: f64,
    wave_angle_rad: f64,
    gamma: f64,
) -> Result<f64, GasDynError> {
    check_gamma(gamma)?;
    check_supersonic(mach)?;
    if wave_angle_rad.is_nan() {
        return Err(GasDynError::NotANumber);
    }
    let m2 = mach * mach;
    let s = libm::sin(wave_angle_rad);
    let c = libm::cos(wave_angle_rad);
    let numerator = 2.0 * (c / s) * (m2 * s * s - 1.0);
    let denominator = m2 * (gamma + libm::cos(2.0 * wave_angle_rad)) + 2.0;
    Ok(libm::atan(numerator / denominator))
}

/// Maximum flow deflection for an attached shock at this Mach number [rad],
/// together with the wave angle at which it occurs.
///
/// Beyond this the shock detaches and bows away from the body. This module does
/// not model detached shocks, so it refuses rather than extrapolating.
///
/// # Errors
/// [`GasDynError`] for NaN, subsonic Mach, or gamma <= 1.
pub fn max_deflection_rad(mach: f64, gamma: f64) -> Result<(f64, f64), GasDynError> {
    check_gamma(gamma)?;
    check_supersonic(mach)?;

    let m2 = mach * mach;
    let m4 = m2 * m2;
    let gp1 = gamma + 1.0;
    let gm1 = gamma - 1.0;

    // Closed form for the wave angle at maximum deflection.
    let inner = gp1 * (gp1 * m4 / 16.0 + gm1 * m2 / 2.0 + 1.0);
    let sin2_beta = (gp1 * m2 / 4.0 - 1.0 + libm::sqrt(inner)) / (gamma * m2);
    let beta = libm::asin(libm::sqrt(sin2_beta.clamp(0.0, 1.0)));
    let theta = deflection_from_wave_angle_rad(mach, beta, gamma)?;
    Ok((theta, beta))
}

/// Wave angle for a given flow deflection [rad], on the requested branch.
///
/// Closed form rather than iteration, so the cost is bounded — this runs inside
/// the inlet model and eventually inside flight software. The closed form is
/// validated in the test module against a bisection solve of
/// [`deflection_from_wave_angle_rad`], with the bracket found by numerical
/// maximisation rather than by the closed form above, so the two checks do not
/// share an assumption.
///
/// # Errors
/// [`GasDynError::DeflectionExceedsMaximum`] if the shock would detach.
pub fn wave_angle_from_deflection_rad(
    mach: f64,
    deflection_rad: f64,
    gamma: f64,
    branch: ShockBranch,
) -> Result<f64, GasDynError> {
    check_gamma(gamma)?;
    check_supersonic(mach)?;
    if deflection_rad.is_nan() {
        return Err(GasDynError::NotANumber);
    }
    if deflection_rad < 0.0 {
        return Err(GasDynError::DeflectionExceedsMaximum);
    }

    // Zero deflection is the degenerate case: a Mach wave, or a normal shock.
    // The general formula divides by tan(theta), so it is handled here.
    if deflection_rad == 0.0 {
        return Ok(match branch {
            ShockBranch::Weak => mach_angle_rad(mach)?,
            ShockBranch::Strong => 0.5 * PI,
        });
    }

    // The detachment boundary is knife-edge, and callers reach it by computing
    // `max_deflection_rad` and passing the result straight back. Rejecting a
    // request one ULP above theta_max would make that round trip fail for a
    // reason that is pure floating-point noise rather than physics - and the
    // inlet model in M3 searches ramp angles right up against this boundary.
    // So a request within a relative 1e-12 of theta_max is accepted and clamped;
    // anything beyond that is a genuinely detached shock and is refused.
    let (theta_max, _) = max_deflection_rad(mach, gamma)?;
    if deflection_rad > theta_max * (1.0 + 1e-12) {
        return Err(GasDynError::DeflectionExceedsMaximum);
    }
    let deflection_rad = deflection_rad.min(theta_max);

    let m2 = mach * mach;
    let m4 = m2 * m2;
    let gp1 = gamma + 1.0;
    let gm1 = gamma - 1.0;
    let tan_theta = libm::tan(deflection_rad);
    let t2 = tan_theta * tan_theta;

    let a = 1.0 + 0.5 * gm1 * m2;
    let lambda_sq = (m2 - 1.0) * (m2 - 1.0) - 3.0 * a * (1.0 + 0.5 * gp1 * m2) * t2;
    // Numerically, lambda_sq can go slightly negative exactly at theta_max.
    let lambda = libm::sqrt(lambda_sq.max(0.0));
    if lambda == 0.0 {
        // At theta_max both roots coincide.
        let (_, beta) = max_deflection_rad(mach, gamma)?;
        return Ok(beta);
    }

    let chi = ((m2 - 1.0) * (m2 - 1.0) * (m2 - 1.0) - 9.0 * a * (a + 0.25 * gp1 * m4) * t2)
        / (lambda * lambda * lambda);
    let delta = match branch {
        ShockBranch::Strong => 0.0,
        ShockBranch::Weak => 1.0,
    };
    let angle = (4.0 * PI * delta + libm::acos(chi.clamp(-1.0, 1.0))) / 3.0;
    let tan_beta = (m2 - 1.0 + 2.0 * lambda * libm::cos(angle)) / (3.0 * a * tan_theta);
    Ok(libm::atan(tan_beta))
}

/// State change across an oblique shock.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ObliqueShock {
    pub mach_upstream: f64,
    pub mach_downstream: f64,
    /// Flow deflection [rad].
    pub deflection_rad: f64,
    /// Shock wave angle [rad].
    pub wave_angle_rad: f64,
    /// Mach number of the component normal to the shock, upstream.
    pub normal_mach_upstream: f64,
    pub pressure_ratio: f64,
    pub temperature_ratio: f64,
    pub density_ratio: f64,
    pub stagnation_pressure_ratio: f64,
    pub branch: ShockBranch,
}

/// Oblique shock for a given flow deflection.
///
/// The thermodynamic jump is that of a normal shock applied to the component of
/// the flow normal to the wave — this is not an approximation but the exact
/// content of NACA 1135 eq. (132). Writing it that way rather than restating the
/// ratios means the oblique and normal relations cannot disagree.
///
/// # Errors
/// [`GasDynError::DeflectionExceedsMaximum`] if the shock would detach.
pub fn oblique_shock(
    mach_upstream: f64,
    deflection_rad: f64,
    gamma: f64,
    branch: ShockBranch,
) -> Result<ObliqueShock, GasDynError> {
    let wave_angle_rad =
        wave_angle_from_deflection_rad(mach_upstream, deflection_rad, gamma, branch)?;
    let normal_mach_upstream = mach_upstream * libm::sin(wave_angle_rad);
    // Rounding can leave the normal component a hair below 1 at zero deflection.
    let normal = normal_shock(normal_mach_upstream.max(1.0), gamma)?;

    let turned = wave_angle_rad - deflection_rad;
    let sin_turned = libm::sin(turned);
    let mach_downstream = if sin_turned == 0.0 {
        return Err(GasDynError::DidNotConverge);
    } else {
        normal.mach_downstream / sin_turned
    };

    Ok(ObliqueShock {
        mach_upstream,
        mach_downstream,
        deflection_rad,
        wave_angle_rad,
        normal_mach_upstream,
        pressure_ratio: normal.pressure_ratio,
        temperature_ratio: normal.temperature_ratio,
        density_ratio: normal.density_ratio,
        stagnation_pressure_ratio: normal.stagnation_pressure_ratio,
        branch,
    })
}

/// Prandtl relation across a normal shock: `M1* . M2* = 1`, exactly, for every
/// gamma. NACA 1135 eq. (97).
///
/// Exposed rather than kept in the tests because it is the single strongest
/// check on the shock relations and depends on no tabulated value.
///
/// # Errors
/// [`GasDynError`] for NaN, subsonic Mach, or gamma <= 1.
pub fn prandtl_relation_residual(mach_upstream: f64, gamma: f64) -> Result<f64, GasDynError> {
    let s = normal_shock(mach_upstream, gamma)?;
    let m1_star = characteristic_mach(s.mach_upstream, gamma)?;
    let m2_star = characteristic_mach(s.mach_downstream, gamma)?;
    Ok(m1_star * m2_star - 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ventus_units::float::{abs, rel_err};

    const GAMMAS: [f64; 4] = [1.4, 1.304, 1.2, 5.0 / 3.0];

    /// The Prandtl relation. No table involved: if the shock algebra is right,
    /// M1* . M2* is 1 to machine precision for every gamma and every Mach.
    #[test]
    fn prandtl_relation_holds_for_every_gamma() {
        for g in GAMMAS {
            for m in [1.0, 1.05, 1.5, 2.0, 3.5, 5.0, 10.0, 50.0] {
                let r = prandtl_relation_residual(m, g).unwrap();
                assert!(abs(r) < 1e-13, "gamma={g} M={m}: M1* M2* - 1 = {r:e}");
            }
        }
    }

    /// Mass, momentum and energy across the computed jump. Working in units
    /// where rho1 = 1, a1 = 1, this closes the shock without reference to how
    /// the ratios were derived.
    #[test]
    fn the_jump_conserves_mass_momentum_and_energy() {
        for g in GAMMAS {
            for m1 in [1.2, 2.0, 3.5, 6.0] {
                let s = normal_shock(m1, g).unwrap();
                let (rho1, t1, p1) = (1.0, 1.0, 1.0 / g);
                let u1 = m1;
                let rho2 = rho1 * s.density_ratio;
                let p2 = p1 * s.pressure_ratio;
                let t2 = t1 * s.temperature_ratio;
                let u2 = s.mach_downstream * libm::sqrt(t2);

                assert!(rel_err(rho2 * u2, rho1 * u1) < 1e-13, "mass, g={g} M={m1}");
                assert!(
                    rel_err(p2 + rho2 * u2 * u2, p1 + rho1 * u1 * u1) < 1e-13,
                    "momentum, g={g} M={m1}"
                );
                let cp = g / (g - 1.0) * (1.0 / g); // R = 1/g in these units
                assert!(
                    rel_err(cp * t2 + 0.5 * u2 * u2, cp * t1 + 0.5 * u1 * u1) < 1e-13,
                    "energy, g={g} M={m1}"
                );
            }
        }
    }

    #[test]
    fn a_shock_of_zero_strength_changes_nothing_and_loses_nothing() {
        for g in GAMMAS {
            let s = normal_shock(1.0, g).unwrap();
            assert!(rel_err(s.mach_downstream, 1.0) < 1e-14, "g={g}");
            assert!(rel_err(s.pressure_ratio, 1.0) < 1e-14);
            assert!(rel_err(s.temperature_ratio, 1.0) < 1e-14);
            assert!(rel_err(s.density_ratio, 1.0) < 1e-14);
            assert!(rel_err(s.stagnation_pressure_ratio, 1.0) < 1e-14);
            assert!(abs(s.entropy_rise_over_r()) < 1e-14);
        }
    }

    /// The second law. A shock destroys stagnation pressure, never creates it.
    #[test]
    fn entropy_never_decreases_across_a_shock() {
        for g in GAMMAS {
            let mut previous = 0.0;
            let mut m = 1.0;
            while m <= 20.0 {
                let s = normal_shock(m, g).unwrap();
                let ds = s.entropy_rise_over_r();
                assert!(ds >= -1e-15, "g={g} M={m}: entropy fell by {ds:e}");
                assert!(
                    ds >= previous - 1e-15,
                    "g={g} M={m}: entropy rise not monotonic"
                );
                assert!(s.stagnation_pressure_ratio <= 1.0 + 1e-15);
                assert!(
                    s.mach_downstream <= 1.0 + 1e-14,
                    "downstream must be subsonic"
                );
                previous = ds;
                m += 0.01;
            }
        }
    }

    /// The strong-shock limit: density ratio saturates at (gamma+1)/(gamma-1).
    /// For air that is 6, which is why hypersonic shock layers are thin.
    #[test]
    fn density_ratio_saturates_at_the_strong_shock_limit() {
        for g in GAMMAS {
            let limit = (g + 1.0) / (g - 1.0);
            let s = normal_shock(1e6, g).unwrap();
            assert!(rel_err(s.density_ratio, limit) < 1e-10, "g={g}");
            assert!(
                s.density_ratio < limit,
                "the limit is approached from below"
            );
        }
        assert!(abs(2.4 / 0.4 - 6.0) < 1e-15);
    }

    #[test]
    fn subsonic_and_invalid_input_is_refused() {
        assert_eq!(normal_shock(0.9, 1.4), Err(GasDynError::SubsonicUpstream));
        assert_eq!(normal_shock(f64::NAN, 1.4), Err(GasDynError::NotANumber));
        assert_eq!(normal_shock(2.0, 1.0), Err(GasDynError::InvalidGamma));
        assert_eq!(mach_angle_rad(0.5), Err(GasDynError::SubsonicUpstream));
    }

    // --- oblique ---------------------------------------------------------

    /// An oblique shock at beta = 90 degrees IS a normal shock. If the two
    /// paths through the code disagree, one of them is wrong.
    #[test]
    fn oblique_at_ninety_degrees_reduces_to_the_normal_shock() {
        for g in GAMMAS {
            for m in [1.5, 2.0, 3.5, 6.0] {
                let normal = normal_shock(m, g).unwrap();
                let theta = deflection_from_wave_angle_rad(m, 0.5 * PI, g).unwrap();
                assert!(
                    abs(theta) < 1e-14,
                    "a normal shock deflects nothing, got {theta}"
                );
                let oblique = oblique_shock(m, 0.0, g, ShockBranch::Strong).unwrap();
                assert!(rel_err(oblique.pressure_ratio, normal.pressure_ratio) < 1e-13);
                assert!(rel_err(oblique.mach_downstream, normal.mach_downstream) < 1e-12);
            }
        }
    }

    /// At the Mach angle the shock is vanishingly weak: no deflection, no loss.
    #[test]
    fn oblique_at_the_mach_angle_is_a_mach_wave() {
        for g in GAMMAS {
            for m in [1.5, 2.0, 3.5, 6.0] {
                let mu = mach_angle_rad(m).unwrap();
                let theta = deflection_from_wave_angle_rad(m, mu, g).unwrap();
                assert!(
                    abs(theta) < 1e-12,
                    "g={g} M={m}: Mach wave deflects {theta}"
                );
                let s = oblique_shock(m, 0.0, g, ShockBranch::Weak).unwrap();
                assert!(rel_err(s.stagnation_pressure_ratio, 1.0) < 1e-12);
                assert!(rel_err(s.mach_downstream, m) < 1e-11);
            }
        }
    }

    /// theta(beta) found numerically, with no closed form involved, must agree
    /// with `max_deflection_rad`. Golden-section search on the forward relation.
    #[test]
    fn max_deflection_closed_form_agrees_with_numerical_maximisation() {
        const INV_PHI: f64 = 0.618_033_988_749_894_9;
        for g in GAMMAS {
            for m in [1.2, 1.5, 2.0, 3.5, 6.0, 12.0] {
                let (mut lo, mut hi) = (mach_angle_rad(m).unwrap(), 0.5 * PI);
                for _ in 0..300 {
                    let a = hi - (hi - lo) * INV_PHI;
                    let b = lo + (hi - lo) * INV_PHI;
                    if deflection_from_wave_angle_rad(m, a, g).unwrap()
                        > deflection_from_wave_angle_rad(m, b, g).unwrap()
                    {
                        hi = b;
                    } else {
                        lo = a;
                    }
                }
                let beta_num = 0.5 * (lo + hi);
                let theta_num = deflection_from_wave_angle_rad(m, beta_num, g).unwrap();
                let (theta_cf, beta_cf) = max_deflection_rad(m, g).unwrap();
                assert!(
                    abs(theta_cf - theta_num) < 1e-9,
                    "g={g} M={m}: theta_max closed form {theta_cf} vs numerical {theta_num}"
                );
                assert!(
                    abs(beta_cf - beta_num) < 1e-6,
                    "g={g} M={m}: beta at theta_max"
                );
            }
        }
    }

    /// The closed-form theta-beta-M inverse against a bisection solve of the
    /// forward relation. The bracket comes from the numerical maximisation
    /// above, not from `max_deflection_rad`, so the two do not share an
    /// assumption.
    #[test]
    fn oblique_inverse_closed_form_agrees_with_bisection() {
        for g in GAMMAS {
            for m in [1.5, 2.0, 3.5, 6.0] {
                let (theta_max, beta_at_max) = max_deflection_rad(m, g).unwrap();
                let mu = mach_angle_rad(m).unwrap();

                for i in 1..20 {
                    let theta = theta_max * f64::from(i) / 20.0;

                    for (branch, mut lo, mut hi) in [
                        (ShockBranch::Weak, mu, beta_at_max),
                        (ShockBranch::Strong, beta_at_max, 0.5 * PI),
                    ] {
                        for _ in 0..200 {
                            let mid = 0.5 * (lo + hi);
                            let f_lo = deflection_from_wave_angle_rad(m, lo, g).unwrap() - theta;
                            let f_mid = deflection_from_wave_angle_rad(m, mid, g).unwrap() - theta;
                            if f_lo * f_mid <= 0.0 {
                                hi = mid;
                            } else {
                                lo = mid;
                            }
                        }
                        let beta_bisect = 0.5 * (lo + hi);
                        let beta_cf = wave_angle_from_deflection_rad(m, theta, g, branch).unwrap();
                        assert!(
                            abs(beta_cf - beta_bisect) < 1e-8,
                            "g={g} M={m} theta={theta} {branch:?}: closed form {beta_cf}, \
                             bisection {beta_bisect}"
                        );
                    }
                }
            }
        }
    }

    /// Round trip: the deflection recovered from the computed wave angle must be
    /// the deflection asked for, on both branches.
    #[test]
    fn deflection_round_trips_through_the_wave_angle() {
        for g in GAMMAS {
            for m in [1.5, 2.0, 3.5, 8.0] {
                let (theta_max, _) = max_deflection_rad(m, g).unwrap();
                for i in 0..=20 {
                    // Parenthesised deliberately: `theta_max * i / 20.0` is
                    // `(theta_max * 20.0) / 20.0` at i = 20, which is NOT
                    // theta_max in floating point. That precedence slip is what
                    // first exposed the knife-edge boundary handled above.
                    let theta = theta_max * (f64::from(i) / 20.0);
                    for branch in [ShockBranch::Weak, ShockBranch::Strong] {
                        let beta = wave_angle_from_deflection_rad(m, theta, g, branch).unwrap();
                        let back = deflection_from_wave_angle_rad(m, beta, g).unwrap();
                        assert!(
                            abs(back - theta) < 1e-9,
                            "g={g} M={m} {branch:?}: asked {theta}, got back {back}"
                        );
                    }
                }
            }
        }
    }

    /// The weak branch leaves the flow supersonic over almost the whole range
    /// and the strong branch leaves it subsonic. The weak solution does go
    /// subsonic in a narrow band just below theta_max, which is real physics,
    /// not a bug — so the assertion is made where it holds.
    #[test]
    fn weak_and_strong_branches_behave_as_advertised() {
        for g in GAMMAS {
            for m in [2.0, 3.5, 6.0] {
                let (theta_max, _) = max_deflection_rad(m, g).unwrap();
                for i in 1..=18 {
                    let theta = theta_max * f64::from(i) / 20.0;
                    let weak = oblique_shock(m, theta, g, ShockBranch::Weak).unwrap();
                    let strong = oblique_shock(m, theta, g, ShockBranch::Strong).unwrap();
                    assert!(
                        weak.wave_angle_rad < strong.wave_angle_rad,
                        "g={g} M={m}: the weak shock must lie at the smaller wave angle"
                    );
                    assert!(
                        weak.stagnation_pressure_ratio > strong.stagnation_pressure_ratio,
                        "g={g} M={m}: the weak shock must lose less total pressure"
                    );
                    assert!(
                        strong.mach_downstream < 1.0,
                        "strong branch must be subsonic"
                    );
                }
                // Comfortably below theta_max the weak solution is supersonic.
                let weak = oblique_shock(m, 0.5 * theta_max, g, ShockBranch::Weak).unwrap();
                assert!(weak.mach_downstream > 1.0, "g={g} M={m}");
            }
        }
    }

    #[test]
    fn a_deflection_beyond_the_maximum_is_refused_rather_than_extrapolated() {
        let (theta_max, _) = max_deflection_rad(3.5, 1.4).unwrap();
        assert_eq!(
            wave_angle_from_deflection_rad(3.5, theta_max * 1.01, 1.4, ShockBranch::Weak),
            Err(GasDynError::DeflectionExceedsMaximum)
        );
        assert_eq!(
            oblique_shock(3.5, theta_max * 1.5, 1.4, ShockBranch::Weak).unwrap_err(),
            GasDynError::DeflectionExceedsMaximum
        );
        assert!(wave_angle_from_deflection_rad(3.5, theta_max, 1.4, ShockBranch::Weak).is_ok());
    }

    /// A ramp of several weak shocks recovers more total pressure than one
    /// normal shock doing the same job. This is the entire argument for a
    /// mixed-compression inlet, and M3 depends on it being true here.
    #[test]
    fn a_shock_train_beats_a_single_normal_shock() {
        let (g, m) = (1.4, 3.5);
        let single = normal_shock(m, g).unwrap().stagnation_pressure_ratio;

        let mut mach = m;
        let mut recovery = 1.0;
        for _ in 0..3 {
            let s = oblique_shock(mach, 8.0_f64.to_radians(), g, ShockBranch::Weak).unwrap();
            recovery *= s.stagnation_pressure_ratio;
            mach = s.mach_downstream;
        }
        recovery *= normal_shock(mach, g).unwrap().stagnation_pressure_ratio;

        assert!(
            recovery > single * 2.0,
            "three 8 deg ramps plus a terminal shock recovered {recovery}, \
             a single normal shock {single}"
        );
    }
}
