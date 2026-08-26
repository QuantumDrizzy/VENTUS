//! Physical constants. Strict SI (ADR-000 D7). Every constant carries a source.
//!
//! Rule: a constant with no source line does not belong in this file.

/// Standard acceleration of free fall [m/s^2].
/// Source: NASA-TM-X-74335 (U.S. Standard Atmosphere 1976), eq. (8).
pub const G0_M_S2: f64 = 9.806_65;

/// Effective Earth radius used by US76 for the geopotential <-> geometric
/// altitude conversion [m]. This is NOT the mean Earth radius; it is the value
/// that makes the US76 geopotential definition exact.
/// Source: NASA-TM-X-74335, section 1.2.2.
pub const EARTH_EFFECTIVE_RADIUS_M: f64 = 6_356_766.0;

/// Universal gas constant as used by US76 [J/(kmol*K)].
/// Source: NASA-TM-X-74335, Table 2.
pub const R_UNIVERSAL_J_KMOL_K: f64 = 8_314.32;

/// Mean molar mass of dry air at sea level [kg/kmol].
/// Source: NASA-TM-X-74335, Table 2.
pub const M_AIR_KG_KMOL: f64 = 28.964_4;

/// Specific gas constant of dry air [J/(kg*K)], DERIVED from the US76 pair
/// above rather than quoted, so that M1 is self-consistent with the US76 table.
///
/// [KNOWN_LIMIT] This evaluates to 287.05307, while the value commonly quoted
/// in the literature (and used in `docs/design-point.md` r2) is 287.0528. The
/// relative difference is 1.0e-6 — exactly M1's acceptance tolerance. Using the
/// quoted value instead of the derived one would put M1 on the edge of its own
/// tolerance for no reason. M1 uses this constant; the design point is annotated.
pub const R_AIR_J_KG_K: f64 = R_UNIVERSAL_J_KMOL_K / M_AIR_KG_KMOL;

/// Ratio of specific heats for calorically perfect air [-].
///
/// ADR-000 D10: this is a DEFAULT for callers to pass in, never a value for
/// `ventus-gasdyn` to assume. Every gas-dynamic relation takes gamma as an
/// explicit parameter. See `crates/ventus-gasdyn/cases/gamma_validity.toml`
/// for where this stops being valid (burner and nozzle: it is a failure there,
/// not a limit).
pub const GAMMA_AIR_CALORICALLY_PERFECT: f64 = 1.4;

/// Sutherland's law reference coefficient for air [kg/(m*s*K^0.5)].
/// mu = SUTHERLAND_C1 * T^1.5 / (T + SUTHERLAND_S)
/// Source: NASA-TM-X-74335, eq. (51).
pub const SUTHERLAND_C1: f64 = 1.458e-6;

/// Sutherland's law effective temperature for air [K].
/// Source: NASA-TM-X-74335, eq. (51).
pub const SUTHERLAND_S_K: f64 = 110.4;

/// Stefan-Boltzmann constant [W/(m^2*K^4)], needed by M5 radiative equilibrium.
/// Source: CODATA 2018 (exact, from the 2019 SI redefinition).
pub const STEFAN_BOLTZMANN_W_M2_K4: f64 = 5.670_374_419e-8;

/// Prandtl number of air, cold-flow approximation [-]. Used for the turbulent
/// recovery factor r = Pr^(1/3) ~ 0.89 in M5.
/// [KNOWN_LIMIT] Pr varies with temperature; M5 must justify or replace this.
pub const PRANDTL_AIR_COLD: f64 = 0.71;

#[cfg(test)]
mod tests {
    use super::*;

    /// Yardstick: the derived value must land where the US76 pair puts it, and
    /// the gap to the commonly quoted 287.0528 must be exactly at M1 tolerance.
    /// If either drifts, something upstream changed and M1 needs re-checking.
    #[test]
    fn air_gas_constant_is_derived_not_quoted() {
        assert!(
            (R_AIR_J_KG_K - 287.053_07).abs() < 1e-5,
            "got {R_AIR_J_KG_K}"
        );
        let quoted = 287.052_8_f64;
        let rel = (R_AIR_J_KG_K - quoted).abs() / quoted;
        assert!(
            rel < 2e-6 && rel > 5e-7,
            "gap to quoted value moved: {rel:e}"
        );
    }
}
