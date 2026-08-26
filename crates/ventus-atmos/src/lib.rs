//! M1 — U.S. Standard Atmosphere 1976.
//!
//! Yardstick: the published table, layer by layer. Everything else in VENTUS
//! depends on this. If M1 is wrong, every later number is noise.
//!
//! Source: NASA-TM-X-74335 (U.S. Standard Atmosphere, 1976). Equations are
//! referenced by their number in that document.
//!
//! Scope: geopotential altitudes from 0 to 84 852 m — the region where US76
//! treats air as a homogeneous mixture of constant mean molar mass. Above that
//! the standard switches to a molecular-diffusion formulation with varying
//! composition; this implementation returns an error there rather than
//! extrapolating a model that no longer applies.
//!
//! `no_std`, no allocation: this exact code runs inside the flight software
//! (ADR-000 D1). Transcendental functions come from `libm`, not from `std`, so
//! results are bit-identical on every platform (ADR-001).
#![no_std]
#![forbid(unsafe_code)]

#[cfg(test)]
extern crate std;

pub mod layers;

use layers::{Layer, LAYERS, TOP_GEOPOTENTIAL_ALTITUDE_M};
use ventus_units::constants::{
    EARTH_EFFECTIVE_RADIUS_M, G0_M_S2, GAMMA_AIR_CALORICALLY_PERFECT, R_AIR_J_KG_K, SUTHERLAND_C1,
    SUTHERLAND_S_K,
};

/// Mean molar mass of dry air at sea level [kg/mol]. US76 Table 2 gives
/// 28.9644 kg/kmol; expressed here per mole to pair with [`R_UNIVERSAL_J_PER_MOL_K`].
pub const M_AIR_KG_PER_MOL: f64 = 0.028_964_4;

/// Universal gas constant as used by US76 [J/(mol*K)]. Table 2 gives
/// 8.314 32 J/(kmol*K) x 1e3; this is the per-mole form.
pub const R_UNIVERSAL_J_PER_MOL_K: f64 = 8.314_32;

/// The barometric exponent group `g0 * M0 / R*` [K/m], US76 eq. (33)/(33a).
/// Evaluates to 0.034 163 194 736 310 36.
pub const G0_M_OVER_R: f64 = G0_M_S2 * M_AIR_KG_PER_MOL / R_UNIVERSAL_J_PER_MOL_K;

/// Why an altitude was refused. The model has a defined domain; silently
/// clamping or extrapolating would put a plausible-looking wrong number into
/// everything downstream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AtmosError {
    /// Below the US76 datum (geopotential 0 m).
    BelowDatum,
    /// Above the 84 852 m geopotential model top.
    AboveModelTop,
    /// Input was NaN.
    NotANumber,
}

/// Atmospheric state at one altitude. Plain `f64` fields in strict SI
/// (ADR-000 D7); cheap to build, `Copy`, no allocation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct State {
    pub geopotential_altitude_m: f64,
    pub geometric_altitude_m: f64,
    pub temperature_k: f64,
    pub pressure_pa: f64,
    pub density_kg_m3: f64,
    pub speed_of_sound_m_s: f64,
    pub dynamic_viscosity_pa_s: f64,
    /// Temperature gradient of the layer containing this altitude [K/m].
    /// Discontinuous at layer boundaries by construction — that is the point.
    pub lapse_rate_k_per_m: f64,
}

impl State {
    /// Kinematic viscosity [m^2/s]. US76 eq. (52).
    #[must_use]
    pub fn kinematic_viscosity_m2_s(&self) -> f64 {
        self.dynamic_viscosity_pa_s / self.density_kg_m3
    }
}

/// Geometric to geopotential altitude. US76 eq. (19).
///
/// `EARTH_EFFECTIVE_RADIUS_M` is not the mean Earth radius; it is the value that
/// makes the US76 geopotential definition exact.
#[must_use]
pub fn geopotential_from_geometric_m(geometric_altitude_m: f64) -> f64 {
    EARTH_EFFECTIVE_RADIUS_M * geometric_altitude_m
        / (EARTH_EFFECTIVE_RADIUS_M + geometric_altitude_m)
}

/// Geopotential to geometric altitude. US76 eq. (18).
#[must_use]
pub fn geometric_from_geopotential_m(geopotential_altitude_m: f64) -> f64 {
    EARTH_EFFECTIVE_RADIUS_M * geopotential_altitude_m
        / (EARTH_EFFECTIVE_RADIUS_M - geopotential_altitude_m)
}

/// Molecular-scale temperature in a layer. US76 eq. (23).
#[must_use]
fn temperature_in_layer_k(layer: &Layer, geopotential_altitude_m: f64) -> f64 {
    layer.base_temperature_k
        + layer.lapse_rate_k_per_m * (geopotential_altitude_m - layer.base_geopotential_altitude_m)
}

/// Pressure in a layer. US76 eq. (33) for a gradient layer, eq. (33a) for an
/// isothermal one.
#[must_use]
fn pressure_in_layer_pa(layer: &Layer, geopotential_altitude_m: f64, temperature_k: f64) -> f64 {
    if layer.lapse_rate_k_per_m == 0.0 {
        let dz = geopotential_altitude_m - layer.base_geopotential_altitude_m;
        layer.base_pressure_pa * libm::exp(-G0_M_OVER_R * dz / layer.base_temperature_k)
    } else {
        layer.base_pressure_pa
            * libm::pow(
                layer.base_temperature_k / temperature_k,
                G0_M_OVER_R / layer.lapse_rate_k_per_m,
            )
    }
}

/// Speed of sound [m/s]. US76 eq. (50).
#[must_use]
pub fn speed_of_sound_m_s(temperature_k: f64) -> f64 {
    libm::sqrt(GAMMA_AIR_CALORICALLY_PERFECT * R_AIR_J_KG_K * temperature_k)
}

/// Dynamic viscosity by Sutherland's law [Pa*s]. US76 eq. (51).
#[must_use]
pub fn dynamic_viscosity_pa_s(temperature_k: f64) -> f64 {
    SUTHERLAND_C1 * temperature_k * libm::sqrt(temperature_k) / (temperature_k + SUTHERLAND_S_K)
}

/// Full atmospheric state at a geopotential altitude.
///
/// # Errors
/// [`AtmosError`] if the altitude is outside the model domain or is NaN.
pub fn at_geopotential(geopotential_altitude_m: f64) -> Result<State, AtmosError> {
    if geopotential_altitude_m.is_nan() {
        return Err(AtmosError::NotANumber);
    }
    if geopotential_altitude_m < 0.0 {
        return Err(AtmosError::BelowDatum);
    }
    if geopotential_altitude_m > TOP_GEOPOTENTIAL_ALTITUDE_M {
        return Err(AtmosError::AboveModelTop);
    }

    let layer = &LAYERS[layers::layer_index(geopotential_altitude_m)];
    let temperature_k = temperature_in_layer_k(layer, geopotential_altitude_m);
    let pressure_pa = pressure_in_layer_pa(layer, geopotential_altitude_m, temperature_k);

    Ok(State {
        geopotential_altitude_m,
        geometric_altitude_m: geometric_from_geopotential_m(geopotential_altitude_m),
        temperature_k,
        pressure_pa,
        // US76 eq. (42). Equivalent to p * M0 / (R* T) with R_AIR = R*/M0.
        density_kg_m3: pressure_pa / (R_AIR_J_KG_K * temperature_k),
        speed_of_sound_m_s: speed_of_sound_m_s(temperature_k),
        dynamic_viscosity_pa_s: dynamic_viscosity_pa_s(temperature_k),
        lapse_rate_k_per_m: layer.lapse_rate_k_per_m,
    })
}

/// Full atmospheric state at a geometric altitude.
///
/// # Errors
/// [`AtmosError`] if the corresponding geopotential altitude is outside the
/// model domain or the input is NaN.
pub fn at_geometric(geometric_altitude_m: f64) -> Result<State, AtmosError> {
    if geometric_altitude_m.is_nan() {
        return Err(AtmosError::NotANumber);
    }
    at_geopotential(geopotential_from_geometric_m(geometric_altitude_m))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ventus_units::float::{abs, rel_err};

    #[test]
    fn barometric_exponent_matches_us76() {
        // US76 quotes g0*M0/R* = 34.163 195 K/km.
        assert!(rel_err(G0_M_OVER_R * 1000.0, 34.163_195) < 1e-7);
    }

    #[test]
    fn sea_level_matches_the_defining_values() {
        let s = at_geopotential(0.0).unwrap();
        assert_eq!(s.temperature_k, 288.15);
        assert_eq!(s.pressure_pa, 101_325.0);
        // US76 sea-level values, all published in the standard itself.
        assert!(
            rel_err(s.density_kg_m3, 1.225) < 1e-5,
            "{}",
            s.density_kg_m3
        );
        assert!(
            rel_err(s.speed_of_sound_m_s, 340.294) < 1e-5,
            "{}",
            s.speed_of_sound_m_s
        );
        assert!(
            rel_err(s.dynamic_viscosity_pa_s, 1.7894e-5) < 1e-4,
            "{}",
            s.dynamic_viscosity_pa_s
        );
        assert_eq!(s.geometric_altitude_m, 0.0);
    }

    /// The `<` versus `<=` bug, pinned at the 20 km boundary: temperature must
    /// be continuous, the lapse rate must not be.
    #[test]
    fn gradient_is_discontinuous_where_temperature_is_continuous() {
        let below = at_geopotential(19_999.0).unwrap();
        let at = at_geopotential(20_000.0).unwrap();
        let above = at_geopotential(20_001.0).unwrap();

        assert!(abs(below.temperature_k - 216.65) < 1e-9);
        assert!(abs(at.temperature_k - 216.65) < 1e-9);
        assert!(abs(above.temperature_k - 216.651) < 1e-9);

        assert_eq!(below.lapse_rate_k_per_m, 0.0);
        assert_eq!(
            at.lapse_rate_k_per_m, 1.0e-3,
            "a boundary starts the upper layer"
        );
        assert_eq!(above.lapse_rate_k_per_m, 1.0e-3);

        // Pressure must be continuous too, and the one-sided slopes must differ.
        // Over a 1 m gap the pressure changes by ~1.6e-4 relative (dp/dh = -rho*g),
        // so this bounds the gradient; strict continuity is checked at 1e-6 m in
        // temperature_and_pressure_are_continuous_at_all_boundaries.
        assert!(rel_err(at.pressure_pa, below.pressure_pa) < 2e-4);
    }

    /// Every boundary, not just the famous one.
    #[test]
    fn temperature_and_pressure_are_continuous_at_all_boundaries() {
        for l in LAYERS.iter().skip(1) {
            let h = l.base_geopotential_altitude_m;
            let below = at_geopotential(h - 1e-6).unwrap();
            let at = at_geopotential(h).unwrap();
            assert!(
                rel_err(at.temperature_k, below.temperature_k) < 1e-9,
                "T jumps at {h} m: {} vs {}",
                below.temperature_k,
                at.temperature_k
            );
            assert!(
                rel_err(at.pressure_pa, below.pressure_pa) < 1e-9,
                "p jumps at {h} m: {} vs {}",
                below.pressure_pa,
                at.pressure_pa
            );
            assert!(rel_err(at.pressure_pa, l.base_pressure_pa) < 1e-15);
        }
    }

    #[test]
    fn pressure_and_density_decrease_monotonically() {
        let mut prev = at_geopotential(0.0).unwrap();
        let mut h = 10.0;
        while h <= TOP_GEOPOTENTIAL_ALTITUDE_M {
            let s = at_geopotential(h).unwrap();
            assert!(s.pressure_pa < prev.pressure_pa, "pressure rose at {h} m");
            assert!(
                s.density_kg_m3 < prev.density_kg_m3,
                "density rose at {h} m"
            );
            prev = s;
            h += 10.0;
        }
    }

    #[test]
    fn the_domain_is_enforced_rather_than_extrapolated() {
        assert_eq!(at_geopotential(-1.0), Err(AtmosError::BelowDatum));
        assert_eq!(
            at_geopotential(TOP_GEOPOTENTIAL_ALTITUDE_M + 1.0),
            Err(AtmosError::AboveModelTop)
        );
        assert_eq!(at_geopotential(f64::NAN), Err(AtmosError::NotANumber));
        assert_eq!(at_geometric(f64::NAN), Err(AtmosError::NotANumber));
        assert!(at_geopotential(TOP_GEOPOTENTIAL_ALTITUDE_M).is_ok());
        assert!(at_geopotential(0.0).is_ok());
    }

    /// Confusing geopotential with geometric altitude is the other classic M1
    /// bug. At the design point it is worth 91 m and about 1 % in density.
    #[test]
    fn geopotential_and_geometric_round_trip_and_differ_measurably() {
        for z in [0.0, 1000.0, 11_000.0, 24_090.955_5, 50_000.0, 80_000.0] {
            let h = geopotential_from_geometric_m(z);
            assert!(
                rel_err(geometric_from_geopotential_m(h), z) < 1e-12,
                "z = {z}"
            );
        }

        let h = 24_000.0;
        let z = geometric_from_geopotential_m(h);
        assert!(
            abs(z - 24_090.955_5) < 1e-3,
            "geometric altitude at 24 km geopotential: {z}"
        );

        let by_h = at_geopotential(h).unwrap();
        let by_z = at_geometric(h).unwrap(); // deliberately the WRONG call
        let density_error = rel_err(by_z.density_kg_m3, by_h.density_kg_m3);
        assert!(
            (0.008..0.015).contains(&density_error),
            "mixing up the two altitudes should cost about 1 % in density, got {density_error:e}"
        );
    }
}
