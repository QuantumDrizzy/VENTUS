//! Temperature-dependent properties of air.
//!
//! ADR-000 D10 requires every relation in this crate to take gamma as an
//! explicit parameter. Something has to supply that gamma when the caller does
//! not want the cold-air value, and this is it.
//!
//! WHAT THIS MODULE DOES NOT DO. It supplies gamma(T); it does not make the
//! flow relations calorically imperfect. `normal_shock` and friends still
//! assume constant cp across the process, so passing them gamma(T) evaluated at
//! one station is an improvement over 1.4 but is not the same as solving the
//! jump with real enthalpy. The difference is quantified in
//! `cases/gamma_validity.toml`: at the design point the real solve gives a
//! post-shock temperature 1.6 % below the constant-cp answer.

use crate::{check_gamma, GasDynError};
use ventus_units::constants::{M_AIR_KG_MOL, R_UNIVERSAL_J_MOL_K};

/// Lower bound of the cp correlation [K].
pub const CP_VALID_MIN_K: f64 = 273.0;
/// Upper bound of the cp correlation [K].
pub const CP_VALID_MAX_K: f64 = 1800.0;

/// Molar heat capacity of air at constant pressure [J/(mol*K)].
///
/// Cubic correlation `a + b T + c T^2 + d T^3`, the standard air fit quoted in
/// Cengel Table A-2b. **[TO VERIFY]** against NIST-JANAF or the NASA Glenn
/// thermodynamic coefficients before this is relied on for M4 thrust numbers.
///
/// [KNOWN_LIMIT] TWO RECITED SOURCES DISAGREE HERE, and neither has been checked
/// against a primary copy. This correlation and the tabulated cp values in the
/// same reference differ by up to **0.68 %** (worst at 400 K), measured by
/// `cp_matches_the_tabulated_values`. The claim that the fit is good to 0.4 % is
/// itself recited and is contradicted by that measurement, so it is not repeated
/// as fact. Propagated into gamma the disagreement is about 0.27 %, which is
/// inside the 1 % tolerance the burner case carries but is not negligible for
/// M4 thrust accounting. Resolve before M4 closes.
///
/// # Errors
/// [`GasDynError::InvalidGamma`] is not used here; the temperature range is
/// enforced with [`GasDynError::DeflectionExceedsMaximum`]-style refusal via
/// [`GasDynError::NotANumber`] for NaN and an explicit range error otherwise.
/// See [`GasDynError::OutsideCorrelationRange`].
pub fn molar_heat_capacity_air_j_mol_k(temperature_k: f64) -> Result<f64, GasDynError> {
    if temperature_k.is_nan() {
        return Err(GasDynError::NotANumber);
    }
    if !(CP_VALID_MIN_K..=CP_VALID_MAX_K).contains(&temperature_k) {
        return Err(GasDynError::OutsideCorrelationRange);
    }
    let t = temperature_k;
    Ok(28.11 + 0.196_7e-2 * t + 0.480_2e-5 * t * t - 1.966e-9 * t * t * t)
}

/// Specific heat of air at constant pressure [J/(kg*K)].
///
/// # Errors
/// See [`molar_heat_capacity_air_j_mol_k`].
pub fn specific_heat_air_j_kg_k(temperature_k: f64) -> Result<f64, GasDynError> {
    Ok(molar_heat_capacity_air_j_mol_k(temperature_k)? / M_AIR_KG_MOL)
}

/// Ratio of specific heats of air at a given temperature [-].
///
/// Computed on a MOLAR basis, `gamma = cp_bar / (cp_bar - R_bar)`, so the molar
/// mass cancels out entirely. That matters: the correlation was fitted with a
/// molar mass of 28.97 kg/kmol while US76 defines 28.9644, and forming gamma
/// per unit mass would quietly import that 2e-4 inconsistency.
///
/// # Errors
/// [`GasDynError::OutsideCorrelationRange`] outside 273-1800 K.
pub fn gamma_air(temperature_k: f64) -> Result<f64, GasDynError> {
    let cp = molar_heat_capacity_air_j_mol_k(temperature_k)?;
    let gamma = cp / (cp - R_UNIVERSAL_J_MOL_K);
    check_gamma(gamma)?;
    Ok(gamma)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ventus_units::float::rel_err;

    /// The correlation against the tabulated cp values in the same reference.
    /// Both are recited, [TO VERIFY]. The bound below is the disagreement that
    /// was MEASURED, not the one the source claims: writing 4e-3 here because a
    /// remembered blurb said 0.4 % would have made this test a statement of
    /// belief rather than of fact.
    #[test]
    fn cp_matches_the_tabulated_values() {
        // (T [K], cp [kJ/kg/K] from the air property table)
        for (t, cp_table) in [
            (300.0, 1.005),
            (400.0, 1.013),
            (600.0, 1.051),
            (800.0, 1.099),
            (1000.0, 1.142),
            (1500.0, 1.216),
        ] {
            let cp = specific_heat_air_j_kg_k(t).unwrap() / 1000.0;
            let err = rel_err(cp, cp_table);
            assert!(
                err < 7e-3,
                "T={t}: correlation {cp:.5}, table {cp_table}, {err:.2e} apart. \
                 The measured worst case is 6.8e-3 at 400 K; exceeding 7e-3 means \
                 something changed and the [KNOWN_LIMIT] needs revisiting."
            );
        }
    }

    /// gamma must fall monotonically with temperature, from 1.400 cold to about
    /// 1.30 at burner temperature. A model that did not would invert the whole
    /// argument of ADR-000 D10.
    #[test]
    fn gamma_falls_monotonically_from_cold_air_to_burner_temperature() {
        let mut previous = f64::INFINITY;
        let mut t = CP_VALID_MIN_K;
        while t <= CP_VALID_MAX_K {
            let g = gamma_air(t).unwrap();
            assert!(g < previous, "gamma rose at T={t}");
            assert!((1.25..=1.41).contains(&g), "T={t}: gamma {g} is not air");
            previous = g;
            t += 1.0;
        }
    }

    /// The two anchors ADR-000 D10 is built on.
    #[test]
    fn gamma_hits_the_two_values_the_adr_depends_on() {
        // Cold air: the value every relation defaults to.
        assert!(rel_err(gamma_air(300.0).unwrap(), 1.400) < 1e-3);
        // Burner: where gamma = 1.4 stops being a limit and becomes a failure.
        let burner = gamma_air(1700.0).unwrap();
        assert!(rel_err(burner, 1.304) < 1e-2, "gamma(1700 K) = {burner}");
        assert!(burner < 1.32, "the burner case asserts gamma below 1.32");
    }

    /// THE DESIGN POINT FREESTREAM IS BELOW THIS CORRELATION'S RANGE.
    ///
    /// T_inf = 222.65 K at 26 km, and the fit is stated valid from 273 K. This
    /// is refused rather than extrapolated: outside its range the cubic returns
    /// gamma = 1.4067, which is 0.5 % above the true value and would be wrong in
    /// the direction that flatters the calorically perfect assumption.
    #[test]
    fn the_correlation_refuses_the_design_point_freestream() {
        assert_eq!(
            gamma_air(222.65),
            Err(GasDynError::OutsideCorrelationRange),
            "the freestream is colder than the correlation is fitted for"
        );
        assert_eq!(gamma_air(272.9), Err(GasDynError::OutsideCorrelationRange));
        assert_eq!(gamma_air(1800.1), Err(GasDynError::OutsideCorrelationRange));
        assert_eq!(gamma_air(f64::NAN), Err(GasDynError::NotANumber));
        assert!(gamma_air(CP_VALID_MIN_K).is_ok());
        assert!(gamma_air(CP_VALID_MAX_K).is_ok());
    }
}
