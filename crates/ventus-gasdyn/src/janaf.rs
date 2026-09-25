//! Heat capacity of air above 1800 K, from primary tables (ADR-006).
//!
//! The cubic in [`crate::gas`] is fitted from 273 K to 1800 K and refuses above
//! it. A burner hotter than 1800 K therefore could not be asked anything. This
//! module answers from the NIST-JANAF rows of the four species that make up
//! 99.997 % of dry air, mixed by the US76 mole fractions, interpolated linearly
//! between 100 K nodes.
//!
//! # What it is not
//! - **Not the burner gas.** The burner holds combustion products, not air; the
//!   same approximation [`crate::gas`] already makes, carried and not fixed.
//! - **Not dissociated.** JANAF rows are single species; the mixture is frozen.
//!   Stops at 3000 K for that reason.
//! - **Not a replacement for the snapshot.** The M 3.50 snapshot keeps the cubic
//!   so its pinned numbers do not move. This module is used where the cubic
//!   refuses, and its disagreement with the cubic is measured, not assumed.

use crate::GasDynError;
use ventus_units::constants::R_UNIVERSAL_J_MOL_K;

/// Mole fractions of dry air at sea level. Source: NASA-TM-X-74335 (US76),
/// Table 3. **[TO VERIFY]** against a primary copy: checked instead against the
/// molar mass the same document gives in Table 2, which the full composition
/// reproduces to 8.7e-7 (see `composition_reproduces_us76_molar_mass`).
pub const X_N2: f64 = 0.780_84;
/// See [`X_N2`].
pub const X_O2: f64 = 0.209_476;
/// See [`X_N2`].
pub const X_AR: f64 = 0.009_34;
/// See [`X_N2`].
pub const X_CO2: f64 = 0.000_314;
/// The four species carried, 0.99997. The remaining 3e-5 (Ne, He, Kr, Xe, CH4,
/// H2) is dropped and the four are renormalised to 1.
const X_SUM: f64 = X_N2 + X_O2 + X_AR + X_CO2;

/// Temperature nodes of the embedded NIST-JANAF rows [K]: 300 K to 3000 K every 100 K.
pub const JANAF_T_MIN_K: f64 = 300.0;
/// Upper node. The tables continue to 6000 K; 3000 K is where this module stops,
/// because dissociation of the mixture is not modelled and becomes material above it.
pub const JANAF_T_MAX_K: f64 = 3000.0;
const JANAF_STEP_K: f64 = 100.0;
/// Cp of N2 [J/(mol*K)], one value per node. Source: NIST-JANAF Thermochemical Tables,
/// 4th ed. (Chase 1998), table N-023 (N2, ref), column Cp, copied digit for digit.
const CP_N2: [f64; 28] = [29.125, 29.249, 29.580, 30.110, 30.754, 31.433, 32.090, 32.697, 33.241, 33.723, 34.147, 34.518, 34.843, 35.128, 35.378, 35.600, 35.796, 35.971, 36.126, 36.268, 36.395, 36.511, 36.616, 36.713, 36.801, 36.883, 36.959, 37.030];
/// Cp of O2 [J/(mol*K)], one value per node. Source: NIST-JANAF Thermochemical Tables,
/// 4th ed. (Chase 1998), table O-029 (O2, ref), column Cp, copied digit for digit.
const CP_O2: [f64; 28] = [29.385, 30.106, 31.091, 32.090, 32.981, 33.733, 34.355, 34.870, 35.300, 35.667, 35.988, 36.277, 36.544, 36.796, 37.040, 37.277, 37.510, 37.741, 37.969, 38.195, 38.419, 38.639, 38.856, 39.068, 39.276, 39.478, 39.674, 39.864];
/// Cp of AR [J/(mol*K)], one value per node. Source: NIST-JANAF Thermochemical Tables,
/// 4th ed. (Chase 1998), table Ar-001 (Ar, ref), column Cp, copied digit for digit.
const CP_AR: [f64; 28] = [20.786, 20.786, 20.786, 20.786, 20.786, 20.786, 20.786, 20.786, 20.786, 20.786, 20.786, 20.786, 20.786, 20.786, 20.786, 20.786, 20.786, 20.786, 20.786, 20.786, 20.786, 20.786, 20.786, 20.786, 20.786, 20.786, 20.786, 20.786];
/// Cp of CO2 [J/(mol*K)], one value per node. Source: NIST-JANAF Thermochemical Tables,
/// 4th ed. (Chase 1998), table C-095 (CO2, g), column Cp, copied digit for digit.
const CP_CO2: [f64; 28] = [37.221, 41.325, 44.627, 47.321, 49.564, 51.434, 52.999, 54.308, 55.409, 56.342, 57.137, 57.802, 58.379, 58.886, 59.317, 59.701, 60.049, 60.350, 60.622, 60.865, 61.086, 61.287, 61.471, 61.647, 61.802, 61.952, 62.095, 62.229];

/// Molar heat capacity of dry air [J/(mol*K)] from the NIST-JANAF rows.
///
/// # Errors
/// [`GasDynError::NotANumber`] for NaN; [`GasDynError::OutsideCorrelationRange`]
/// outside [`JANAF_T_MIN_K`]..=[`JANAF_T_MAX_K`].
pub fn molar_heat_capacity_air_janaf_j_mol_k(temperature_k: f64) -> Result<f64, GasDynError> {
    if temperature_k.is_nan() {
        return Err(GasDynError::NotANumber);
    }
    if !(JANAF_T_MIN_K..=JANAF_T_MAX_K).contains(&temperature_k) {
        return Err(GasDynError::OutsideCorrelationRange);
    }
    let pos = (temperature_k - JANAF_T_MIN_K) / JANAF_STEP_K;
    let last = CP_N2.len() - 1;
    let i = (pos as usize).min(last - 1);
    let w = pos - i as f64;
    let at = |k: usize| {
        (X_N2 * CP_N2[k] + X_O2 * CP_O2[k] + X_AR * CP_AR[k] + X_CO2 * CP_CO2[k]) / X_SUM
    };
    Ok(at(i) * (1.0 - w) + at(i + 1) * w)
}

/// Specific heat of dry air [J/(kg*K)], per unit mass on the US76 molar mass.
///
/// # Errors
/// See [`molar_heat_capacity_air_janaf_j_mol_k`].
pub fn specific_heat_air_janaf_j_kg_k(temperature_k: f64) -> Result<f64, GasDynError> {
    Ok(molar_heat_capacity_air_janaf_j_mol_k(temperature_k)?
        / ventus_units::constants::M_AIR_KG_MOL)
}

/// Ratio of specific heats from the JANAF mixture, on a molar basis.
///
/// # Errors
/// See [`molar_heat_capacity_air_janaf_j_mol_k`].
pub fn gamma_air_janaf(temperature_k: f64) -> Result<f64, GasDynError> {
    let cp = molar_heat_capacity_air_janaf_j_mol_k(temperature_k)?;
    let gamma = cp / (cp - R_UNIVERSAL_J_MOL_K);
    crate::check_gamma(gamma)?;
    Ok(gamma)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gas::{molar_heat_capacity_air_j_mol_k, CP_VALID_MAX_K};

    /// The composition is recited, not read; the molar mass beside it in US76 is
    /// already cited in this workspace. If the recited fractions were wrong,
    /// this is where it would show.
    #[test]
    fn composition_reproduces_us76_molar_mass() {
        // Full US76 Table 3 composition, including the trace species this module drops.
        let m = X_N2 * 28.0134
            + X_O2 * 31.9988
            + X_AR * 39.948
            + X_CO2 * 44.009_95
            + 1.818e-5 * 20.183
            + 5.24e-6 * 4.0026
            + 1.14e-6 * 83.80
            + 8.7e-8 * 131.30
            + 2.0e-6 * 16.043_03
            + 5.0e-7 * 2.015_94;
        let rel = (m - ventus_units::constants::M_AIR_KG_KMOL) / ventus_units::constants::M_AIR_KG_KMOL;
        assert!(rel.abs() < 2e-6, "composition gives {m}, rel {rel}");
    }

    /// At a node the interpolation must return the table mixture exactly.
    #[test]
    fn nodes_are_the_table() {
        let expect_2000 = (X_N2 * 35.971 + X_O2 * 37.741 + X_AR * 20.786 + X_CO2 * 60.350) / X_SUM;
        let got = molar_heat_capacity_air_janaf_j_mol_k(2000.0).unwrap();
        assert!((got - expect_2000).abs() < 1e-12, "{got} vs {expect_2000}");
    }

    #[test]
    fn range_is_refused_not_extrapolated() {
        assert_eq!(molar_heat_capacity_air_janaf_j_mol_k(299.9), Err(GasDynError::OutsideCorrelationRange));
        assert_eq!(molar_heat_capacity_air_janaf_j_mol_k(3000.1), Err(GasDynError::OutsideCorrelationRange));
        assert_eq!(molar_heat_capacity_air_janaf_j_mol_k(f64::NAN), Err(GasDynError::NotANumber));
        assert!(molar_heat_capacity_air_janaf_j_mol_k(JANAF_T_MAX_K).is_ok());
    }

    #[test]
    fn heat_capacity_rises_and_gamma_falls_with_temperature() {
        let mut t = JANAF_T_MIN_K;
        let mut last_cp = 0.0;
        while t <= JANAF_T_MAX_K {
            let cp = molar_heat_capacity_air_janaf_j_mol_k(t).unwrap();
            assert!(cp > last_cp, "cp not rising at {t}");
            last_cp = cp;
            t += 50.0;
        }
        assert!(gamma_air_janaf(2000.0).unwrap() < gamma_air_janaf(1000.0).unwrap());
    }

    /// The two models overlap from 300 K to 1800 K. Their disagreement is
    /// MEASURED here and asserted at the measured size, so a change in either
    /// shows up. It is the [TO VERIFY] in `gas.rs` answered against a primary
    /// table: the cubic sits within this band of JANAF over its whole range.
    #[test]
    fn cubic_disagrees_with_janaf_by_the_measured_amount() {
        let mut worst = 0.0_f64;
        let mut at = 0.0;
        let mut t = JANAF_T_MIN_K;
        while t <= CP_VALID_MAX_K {
            let janaf = molar_heat_capacity_air_janaf_j_mol_k(t).unwrap();
            let cubic = molar_heat_capacity_air_j_mol_k(t).unwrap();
            let rel = ((cubic - janaf) / janaf).abs();
            if rel > worst {
                worst = rel;
                at = t;
            }
            t += 10.0;
        }
        extern crate std;
        std::println!("cubic vs JANAF, 300-1800 K: worst {:.4} % at {at} K", worst * 100.0);
        assert!(worst < MEASURED_CUBIC_VS_JANAF_WORST, "worst {worst} at {at} K");
    }

    /// Set from the measurement above, rounded up; not a tolerance chosen in advance.
    const MEASURED_CUBIC_VS_JANAF_WORST: f64 = 0.0080; // measured 0.7684 % at 500 K
}
