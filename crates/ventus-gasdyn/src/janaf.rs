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


/// H - H(298.15 K) at 200 K [kJ/mol], below the uniform grid: freestream at 20-30 km is
/// ~220-230 K. 250 K is not used because the CO2 table has no 250 K row.
const H_LOW_T_K: f64 = 200.0;
/// H - H(298.15 K) of N2 [kJ/mol]: at 200 K, then at every uniform node from 300 K.
/// Source: NIST-JANAF (Chase 1998), same table as `CP_N2`, column H-H(Tr), digit for digit.
const H200_N2: f64 = -2.857;
const H_N2: [f64; 28] = [0.054, 2.971, 5.911, 8.894, 11.937, 15.046, 18.223, 21.463, 24.760, 28.109, 31.503, 34.936, 38.405, 41.904, 45.429, 48.978, 52.548, 56.137, 59.742, 63.361, 66.995, 70.640, 74.296, 77.963, 81.639, 85.323, 89.015, 92.715];
/// H - H(298.15 K) of O2 [kJ/mol]: at 200 K, then at every uniform node from 300 K.
/// Source: NIST-JANAF (Chase 1998), same table as `CP_O2`, column H-H(Tr), digit for digit.
const H200_O2: f64 = -2.868;
const H_O2: [f64; 28] = [0.054, 3.025, 6.084, 9.244, 12.499, 15.835, 19.241, 22.703, 26.212, 29.761, 33.344, 36.957, 40.599, 44.266, 47.958, 51.673, 55.413, 59.175, 62.961, 66.769, 70.600, 74.453, 78.328, 82.224, 86.141, 90.079, 94.036, 98.013];
/// H - H(298.15 K) of AR [kJ/mol]: at 200 K, then at every uniform node from 300 K.
/// Source: NIST-JANAF (Chase 1998), same table as `CP_AR`, column H-H(Tr), digit for digit.
const H200_AR: f64 = -2.040;
const H_AR: [f64; 28] = [0.038, 2.117, 4.196, 6.274, 8.353, 10.431, 12.510, 14.589, 16.667, 18.746, 20.824, 22.903, 24.982, 27.060, 29.139, 31.217, 33.296, 35.375, 37.453, 39.532, 41.610, 43.689, 45.768, 47.846, 49.925, 52.004, 54.082, 56.161];
/// H - H(298.15 K) of CO2 [kJ/mol]: at 200 K, then at every uniform node from 300 K.
/// Source: NIST-JANAF (Chase 1998), same table as `CP_CO2`, column H-H(Tr), digit for digit.
const H200_CO2: f64 = -3.414;
const H_CO2: [f64; 28] = [0.069, 4.003, 8.305, 12.907, 17.754, 22.806, 28.030, 33.397, 38.884, 44.473, 50.148, 55.896, 61.705, 67.569, 73.480, 79.431, 85.419, 91.439, 97.488, 103.562, 109.660, 115.779, 121.917, 128.073, 134.246, 140.433, 146.636, 152.852];

/// Lowest temperature [`enthalpy_air_janaf_j_kg`] answers at [K].
pub const JANAF_H_MIN_K: f64 = H_LOW_T_K;

fn mix(n2: f64, o2: f64, ar: f64, co2: f64) -> f64 {
    (X_N2 * n2 + X_O2 * o2 + X_AR * ar + X_CO2 * co2) / X_SUM
}

/// Sensible enthalpy of dry air relative to 298.15 K [J/kg].
///
/// Exact at every tabulated node. Between uniform nodes it integrates the same
/// linear cp as [`molar_heat_capacity_air_janaf_j_mol_k`] and adds the small linear
/// correction that lands the next node on its tabulated H, so h and cp come from
/// one table rather than two. Between 200 K and 300 K, where cp is flat to 1 %,
/// it is linear in the two tabulated H values.
///
/// # Errors
/// [`GasDynError::NotANumber`] for NaN; [`GasDynError::OutsideCorrelationRange`]
/// outside [`JANAF_H_MIN_K`]..=[`JANAF_T_MAX_K`].
pub fn enthalpy_air_janaf_j_kg(temperature_k: f64) -> Result<f64, GasDynError> {
    if temperature_k.is_nan() {
        return Err(GasDynError::NotANumber);
    }
    if !(JANAF_H_MIN_K..=JANAF_T_MAX_K).contains(&temperature_k) {
        return Err(GasDynError::OutsideCorrelationRange);
    }
    let h_node = |k: usize| mix(H_N2[k], H_O2[k], H_AR[k], H_CO2[k]) * 1000.0; // J/mol
    let cp_node = |k: usize| mix(CP_N2[k], CP_O2[k], CP_AR[k], CP_CO2[k]);
    let h_mol = if temperature_k < JANAF_T_MIN_K {
        let h200 = mix(H200_N2, H200_O2, H200_AR, H200_CO2) * 1000.0;
        let w = (temperature_k - H_LOW_T_K) / (JANAF_T_MIN_K - H_LOW_T_K);
        h200 + w * (h_node(0) - h200)
    } else {
        let pos = (temperature_k - JANAF_T_MIN_K) / JANAF_STEP_K;
        let i = (pos as usize).min(CP_N2.len() - 2);
        let dt = temperature_k - (JANAF_T_MIN_K + i as f64 * JANAF_STEP_K);
        let (c0, c1) = (cp_node(i), cp_node(i + 1));
        let integral = |x: f64| c0 * x + (c1 - c0) * x * x / (2.0 * JANAF_STEP_K);
        let residual = h_node(i + 1) - (h_node(i) + integral(JANAF_STEP_K));
        h_node(i) + integral(dt) + residual * dt / JANAF_STEP_K
    };
    Ok(h_mol / ventus_units::constants::M_AIR_KG_MOL)
}

/// Stagnation temperature with real enthalpy, `h0 = h + V^2/2` [K].
///
/// The method `docs/design-point.md` 3.1 uses, answered from JANAF instead of
/// recited tables. Solved by bisection on the monotonic h(T).
///
/// # Errors
/// [`GasDynError::OutsideCorrelationRange`] if the static or stagnation
/// temperature leaves the tables; [`GasDynError::NotANumber`] for NaN input.
pub fn stagnation_temperature_thermally_perfect_k(
    static_temperature_k: f64,
    velocity_m_s: f64,
) -> Result<f64, GasDynError> {
    if velocity_m_s.is_nan() {
        return Err(GasDynError::NotANumber);
    }
    let h0 = enthalpy_air_janaf_j_kg(static_temperature_k)? + 0.5 * velocity_m_s * velocity_m_s;
    if h0 > enthalpy_air_janaf_j_kg(JANAF_T_MAX_K)? {
        return Err(GasDynError::OutsideCorrelationRange);
    }
    let (mut lo, mut hi) = (static_temperature_k, JANAF_T_MAX_K);
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        if enthalpy_air_janaf_j_kg(mid)? < h0 {
            lo = mid;
        } else {
            hi = mid;
        }
        if hi - lo < 1e-9 {
            break;
        }
    }
    Ok(0.5 * (lo + hi))
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


    /// H and cp are two columns of one table. Integrating the linear cp across each
    /// 100 K interval must land within a small fraction of the tabulated H rise, or
    /// one column was mis-copied.
    #[test]
    fn cp_integrates_to_the_tabulated_enthalpy_rise() {
        let mut worst = 0.0_f64;
        for i in 0..CP_N2.len() - 1 {
            let c0 = mix(CP_N2[i], CP_O2[i], CP_AR[i], CP_CO2[i]);
            let c1 = mix(CP_N2[i + 1], CP_O2[i + 1], CP_AR[i + 1], CP_CO2[i + 1]);
            let integral = 0.5 * (c0 + c1) * JANAF_STEP_K;
            let dh = (mix(H_N2[i + 1], H_O2[i + 1], H_AR[i + 1], H_CO2[i + 1])
                - mix(H_N2[i], H_O2[i], H_AR[i], H_CO2[i]))
                * 1000.0;
            worst = worst.max(((integral - dh) / dh).abs());
        }
        assert!(worst < 2e-3, "worst interval mismatch {worst}");
    }

    #[test]
    fn enthalpy_is_the_table_at_nodes_and_zero_at_reference() {
        let h2000 = enthalpy_air_janaf_j_kg(2000.0).unwrap() * ventus_units::constants::M_AIR_KG_MOL;
        let expect = mix(56.137, 59.175, 35.375, 91.439) * 1000.0;
        assert!((h2000 - expect).abs() < 1e-6, "{h2000} vs {expect}");
        // 298.15 K is the reference; the 200-300 K leg is linear, so it is close, not exact.
        assert!(enthalpy_air_janaf_j_kg(298.15).unwrap().abs() < 200.0);
    }

    /// `docs/design-point.md` 3.1 recites T0 = 752.8 K at the design point from
    /// Cengel-lineage tables, marked [TO VERIFY]. This answers it from JANAF.
    #[test]
    fn design_point_stagnation_temperature_against_janaf() {
        let t0 = stagnation_temperature_thermally_perfect_k(222.65, 1046.95).unwrap();
        extern crate std;
        std::println!("M 3.50 design point, JANAF thermally perfect T0 = {t0:.2} K (recited 752.8 K)");
        assert!((t0 - MEASURED_DESIGN_POINT_T0_K).abs() < 0.05, "T0 {t0}");
    }

    /// Measured from JANAF. The recited 752.8 K of `design-point.md` 3.1 agrees to
    /// 0.12 K, which verifies that [TO VERIFY] against a primary table.
    const MEASURED_DESIGN_POINT_T0_K: f64 = 752.92;

    /// Set from the measurement above, rounded up; not a tolerance chosen in advance.
    const MEASURED_CUBIC_VS_JANAF_WORST: f64 = 0.0080; // measured 0.7684 % at 500 K
}
