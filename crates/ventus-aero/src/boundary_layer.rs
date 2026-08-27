//! Compressible flat-plate boundary layer by the reference-temperature method.
//!
//! This is the part of M6 that does NOT need vehicle geometry: it depends only
//! on the local edge state and a running length, so it can be built while the
//! area rule and Sears-Haack remain blocked (design-point.md 5.2).
//!
//! M5 needs it. The wall temperature of a radiating skin is set by the balance
//! between convective input and radiative output, and the convective side needs
//! a film coefficient, which needs boundary-layer state. That is why the
//! dependency runs thermal -> aero.
//!
//! # Method
//!
//! Eckert's reference-temperature method: evaluate the incompressible flat-plate
//! correlations at a reference temperature that stands in for the property
//! variation across the layer,
//!
//! ```text
//! T* = T_e + 0.5 (T_w - T_e) + 0.22 (T_aw - T_e)
//! ```
//!
//! then take the Reynolds number, skin friction and Stanton number at `T*`.
//! It is an engineering correlation, not a solution of the boundary-layer
//! equations, and it is used here because it is the standard published method
//! with published anchors — not because it is exact.

use ventus_units::constants::{PRANDTL_AIR_COLD, R_AIR_J_KG_K};

/// Which correlation set applies.
///
/// The transition Reynolds number for a smooth flat plate is conventionally
/// 5e5, but it is sensitive to roughness, pressure gradient and freestream
/// turbulence, so the regime is an explicit input rather than something this
/// module decides for the caller. At the VENTUS-1 design point Re/L is
/// 2.4e6 per metre, so anything past a few centimetres is turbulent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Regime {
    Laminar,
    Turbulent,
}

/// Why a boundary-layer relation refused to answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundaryLayerError {
    NotANumber,
    /// Running length must be strictly positive: the correlations are singular
    /// at the leading edge, where the boundary layer has zero thickness.
    NonPositiveLength,
    /// Reynolds number must be strictly positive and finite.
    InvalidReynolds,
    /// Temperature, pressure or velocity was non-physical.
    InvalidEdgeState,
    /// Emissivity outside [0, 1].
    InvalidEmissivity,
    /// A bracketed solve did not converge in its fixed budget.
    DidNotConverge,
}

/// Flow state at the edge of the boundary layer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgeState {
    pub temperature_k: f64,
    pub pressure_pa: f64,
    pub velocity_m_s: f64,
    pub mach: f64,
    pub gamma: f64,
}

impl EdgeState {
    /// # Errors
    /// [`BoundaryLayerError::InvalidEdgeState`] if any field is non-physical.
    pub fn validate(&self) -> Result<(), BoundaryLayerError> {
        if self.temperature_k.is_nan()
            || self.pressure_pa.is_nan()
            || self.velocity_m_s.is_nan()
            || self.mach.is_nan()
            || self.gamma.is_nan()
        {
            return Err(BoundaryLayerError::NotANumber);
        }
        if self.temperature_k <= 0.0
            || self.pressure_pa <= 0.0
            || self.velocity_m_s <= 0.0
            || self.mach <= 0.0
            || self.gamma <= 1.0
        {
            return Err(BoundaryLayerError::InvalidEdgeState);
        }
        Ok(())
    }
}

/// Temperature recovery factor: `sqrt(Pr)` laminar, `Pr^(1/3)` turbulent.
///
/// For air at Pr = 0.71 these are 0.8426 and 0.8921. The turbulent value is the
/// 0.89 that appears throughout `docs/design-point.md`.
#[must_use]
pub fn recovery_factor(prandtl: f64, regime: Regime) -> f64 {
    match regime {
        Regime::Laminar => libm::sqrt(prandtl),
        Regime::Turbulent => libm::cbrt(prandtl),
    }
}

/// Adiabatic wall (recovery) temperature.
///
/// [KNOWN_LIMIT] Calorically perfect: this uses `T0/T = 1 + (g-1)/2 M^2`. At the
/// VENTUS-1 design point that gives T_aw = 709.3 K, while carrying the thermally
/// perfect T0 = 752.8 K through the same recovery factor gives 695.6 K — 13.7 K
/// lower. Propagated through the radiation balance that is worth about 6 K on
/// the wall temperature, because dT_w/dT_aw = h / (h + 4 eps sigma T_w^3) is
/// only about 0.46 there. Quantified in `ventus-thermal`.
///
/// # Errors
/// [`BoundaryLayerError`] for NaN or a non-physical edge state.
pub fn adiabatic_wall_temperature_k(
    edge: &EdgeState,
    prandtl: f64,
    regime: Regime,
) -> Result<f64, BoundaryLayerError> {
    edge.validate()?;
    let r = recovery_factor(prandtl, regime);
    Ok(edge.temperature_k * (1.0 + r * 0.5 * (edge.gamma - 1.0) * edge.mach * edge.mach))
}

/// Eckert reference temperature.
///
/// The 0.5 and 0.22 weights are the published Eckert coefficients. They are an
/// empirical stand-in for the property variation across the layer, not a
/// derivation.
#[must_use]
pub fn reference_temperature_k(
    edge_temperature_k: f64,
    wall_temperature_k: f64,
    adiabatic_wall_temperature_k: f64,
) -> f64 {
    edge_temperature_k
        + 0.5 * (wall_temperature_k - edge_temperature_k)
        + 0.22 * (adiabatic_wall_temperature_k - edge_temperature_k)
}

/// Local skin-friction coefficient for an incompressible flat plate.
///
/// Laminar: the Blasius result `cf = 0.664 / sqrt(Re_x)`, which is exact.
/// Turbulent: `cf = 0.0592 Re_x^(-1/5)`, the Prandtl-Schlichting power-law fit,
/// nominally good over 5e5 < Re_x < 1e7 and used beyond that here because the
/// design point runs to Re_x ~ 5e7. That extrapolation is a stated limitation,
/// not an oversight.
///
/// [KNOWN_LIMIT] THE CHOICE OF CORRELATION IS A REAL UNCERTAINTY, larger than
/// the precision any single one of them is printed to. At Re_x = 1e6:
///
/// ```text
///   Prandtl-Schlichting  0.0592 Re^(-1/5)          3.7353e-3
///   1/7 power law        0.027  Re^(-1/7)          3.7516e-3
///   Schultz-Grunow       0.370 (log10 Re)^(-2.584) 3.6096e-3
/// ```
///
/// A spread of 3.9 %, so asserting three digits of one of them would be
/// asserting an agreement that does not exist. What that spread is worth
/// downstream is measured rather than assumed: `ventus-thermal` shows that a
/// 2 % change in `cf` moves the radiation-equilibrium wall temperature by about
/// 1.5 K, against a 75 K margin to the Ti-6Al-4V limit. The material conclusion
/// is therefore insensitive to which correlation is used, which is why
/// Prandtl-Schlichting is kept without further argument.
///
/// # Errors
/// [`BoundaryLayerError::InvalidReynolds`] for a non-positive or non-finite
/// Reynolds number.
pub fn skin_friction_coefficient(
    reynolds_x: f64,
    regime: Regime,
) -> Result<f64, BoundaryLayerError> {
    if reynolds_x.is_nan() {
        return Err(BoundaryLayerError::NotANumber);
    }
    if reynolds_x <= 0.0 || !reynolds_x.is_finite() {
        return Err(BoundaryLayerError::InvalidReynolds);
    }
    Ok(match regime {
        Regime::Laminar => 0.664 / libm::sqrt(reynolds_x),
        Regime::Turbulent => 0.0592 * libm::pow(reynolds_x, -0.2),
    })
}

/// Stanton number from skin friction by the Chilton-Colburn analogy,
/// `St = (cf/2) Pr^(-2/3)`.
///
/// At Pr = 1 this collapses to the exact Reynolds analogy `St = cf/2`, which is
/// the identity the tests use as an anchor.
#[must_use]
pub fn stanton_number(skin_friction: f64, prandtl: f64) -> f64 {
    0.5 * skin_friction * libm::pow(prandtl, -2.0 / 3.0)
}

/// Everything the thermal balance needs at one station.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FilmState {
    pub adiabatic_wall_temperature_k: f64,
    pub reference_temperature_k: f64,
    pub reference_density_kg_m3: f64,
    pub reynolds_x: f64,
    pub skin_friction: f64,
    pub stanton: f64,
    /// Convective film coefficient [W/(m^2*K)].
    pub heat_transfer_coefficient_w_m2_k: f64,
}

/// Boundary-layer state at a station, for an assumed wall temperature.
///
/// The wall temperature is an input because the reference temperature depends
/// on it; closing the loop against a radiation balance is `ventus-thermal`.
///
/// # Errors
/// [`BoundaryLayerError`] for a non-physical edge state or running length.
pub fn film_state(
    edge: &EdgeState,
    running_length_m: f64,
    wall_temperature_k: f64,
    prandtl: f64,
    regime: Regime,
) -> Result<FilmState, BoundaryLayerError> {
    edge.validate()?;
    if running_length_m.is_nan() || wall_temperature_k.is_nan() {
        return Err(BoundaryLayerError::NotANumber);
    }
    if running_length_m <= 0.0 {
        return Err(BoundaryLayerError::NonPositiveLength);
    }

    let t_aw = adiabatic_wall_temperature_k(edge, prandtl, regime)?;
    let t_star = reference_temperature_k(edge.temperature_k, wall_temperature_k, t_aw);

    // Properties at the reference temperature. Pressure is constant across a
    // boundary layer, so density follows from the gas law at T*.
    let rho_star = edge.pressure_pa / (R_AIR_J_KG_K * t_star);
    let mu_star = ventus_atmos::dynamic_viscosity_pa_s(t_star);
    let cp_star = ventus_gasdyn::specific_heat_air_j_kg_k(t_star).unwrap_or_else(|_| {
        // The cp correlation is fitted from 273 K. A reference temperature can
        // fall below that only for a nearly-cold wall in cold flow, where the
        // cold-air value is the right answer anyway.
        R_AIR_J_KG_K * 1.4 / 0.4
    });

    let reynolds_x = rho_star * edge.velocity_m_s * running_length_m / mu_star;
    let skin_friction = skin_friction_coefficient(reynolds_x, regime)?;
    let stanton = stanton_number(skin_friction, prandtl);

    Ok(FilmState {
        adiabatic_wall_temperature_k: t_aw,
        reference_temperature_k: t_star,
        reference_density_kg_m3: rho_star,
        reynolds_x,
        skin_friction,
        stanton,
        heat_transfer_coefficient_w_m2_k: stanton * rho_star * edge.velocity_m_s * cp_star,
    })
}

/// Prandtl number used throughout, re-exported so callers do not re-declare it.
pub const PRANDTL_AIR: f64 = PRANDTL_AIR_COLD;

#[cfg(test)]
mod tests {
    use super::*;
    use ventus_units::float::rel_err;

    /// The Blasius constant is exact, not tabulated: `cf sqrt(Re_x) = 0.664` at
    /// every Reynolds number. Any Re-dependence in that product is an algebra
    /// error.
    #[test]
    fn laminar_skin_friction_is_the_blasius_constant() {
        for re in [1e3, 1e4, 1e5, 5e5] {
            let cf = skin_friction_coefficient(re, Regime::Laminar).unwrap();
            assert!(rel_err(cf * libm::sqrt(re), 0.664) < 1e-14, "Re={re}");
        }
    }

    /// At Pr = 1 the Chilton-Colburn analogy must collapse to the exact Reynolds
    /// analogy St = cf/2. No table involved.
    #[test]
    fn reynolds_analogy_is_exact_at_unit_prandtl() {
        for cf in [1e-4, 1e-3, 5e-3, 2e-2] {
            assert!(rel_err(stanton_number(cf, 1.0), 0.5 * cf) < 1e-15);
        }
        // Air transfers heat slightly better than momentum, so St > cf/2.
        let cf = 2.6e-3;
        assert!(stanton_number(cf, PRANDTL_AIR) > 0.5 * cf);
    }

    #[test]
    fn recovery_factors_match_their_definitions() {
        assert!(rel_err(recovery_factor(1.0, Regime::Laminar), 1.0) < 1e-15);
        assert!(rel_err(recovery_factor(1.0, Regime::Turbulent), 1.0) < 1e-15);
        // Air: 0.8426 laminar, 0.8921 turbulent. The turbulent value is the
        // 0.89 quoted throughout docs/design-point.md.
        assert!(rel_err(recovery_factor(PRANDTL_AIR, Regime::Laminar), 0.842_615) < 1e-5);
        assert!(rel_err(recovery_factor(PRANDTL_AIR, Regime::Turbulent), 0.892_112) < 1e-5);
        // Turbulent recovery is always the higher of the two for Pr < 1.
        assert!(
            recovery_factor(PRANDTL_AIR, Regime::Turbulent)
                > recovery_factor(PRANDTL_AIR, Regime::Laminar)
        );
    }

    /// At Pr = 1 recovery is perfect and T_aw must equal the stagnation
    /// temperature exactly. That ties this module to M2 through an identity
    /// rather than through a number.
    #[test]
    fn perfect_recovery_reproduces_the_stagnation_temperature() {
        let edge = EdgeState {
            temperature_k: 222.65,
            pressure_pa: 2153.09,
            velocity_m_s: 1046.95,
            mach: 3.5,
            gamma: 1.4,
        };
        for regime in [Regime::Laminar, Regime::Turbulent] {
            let t_aw = adiabatic_wall_temperature_k(&edge, 1.0, regime).unwrap();
            let t0 = edge.temperature_k
                * ventus_gasdyn::stagnation_temperature_ratio(edge.mach, edge.gamma).unwrap();
            assert!(rel_err(t_aw, t0) < 1e-14, "{regime:?}");
        }
        // With real air the wall recovers less than the full stagnation rise.
        let t_aw = adiabatic_wall_temperature_k(&edge, PRANDTL_AIR, Regime::Turbulent).unwrap();
        assert!(t_aw < 768.2 && t_aw > 700.0, "T_aw = {t_aw}");
    }

    /// The reference temperature must sit between the edge and the recovery
    /// temperature for any wall temperature in that range, and reduce to the
    /// edge temperature when the flow is neither hot nor moving.
    #[test]
    fn reference_temperature_is_bracketed() {
        let (te, taw) = (222.65, 709.3);
        for tw in [222.65, 400.0, 550.0, 709.3] {
            let ts = reference_temperature_k(te, tw, taw);
            assert!(ts >= te && ts <= taw, "T_w={tw}: T*={ts}");
        }
        assert!(rel_err(reference_temperature_k(300.0, 300.0, 300.0), 300.0) < 1e-15);
    }

    /// Turbulent skin friction against the published Prandtl-Schlichting value.
    /// Recited, so the tolerance matches the three figures it is quoted to.
    #[test]
    fn turbulent_skin_friction_matches_the_published_correlation() {
        let cf = skin_friction_coefficient(1e6, Regime::Turbulent).unwrap();
        assert!(rel_err(cf, 3.73e-3) < 2e-3, "cf(1e6) = {cf}");
        // Turbulent friction exceeds laminar at any Reynolds number where both
        // are defined, which is why transition matters.
        for re in [1e6, 1e7, 1e8] {
            assert!(
                skin_friction_coefficient(re, Regime::Turbulent).unwrap()
                    > skin_friction_coefficient(re, Regime::Laminar).unwrap()
            );
        }
    }

    #[test]
    fn non_physical_input_is_refused() {
        assert_eq!(
            skin_friction_coefficient(0.0, Regime::Turbulent),
            Err(BoundaryLayerError::InvalidReynolds)
        );
        assert_eq!(
            skin_friction_coefficient(f64::NAN, Regime::Laminar),
            Err(BoundaryLayerError::NotANumber)
        );
        let edge = EdgeState {
            temperature_k: 222.65,
            pressure_pa: 2153.09,
            velocity_m_s: 1046.95,
            mach: 3.5,
            gamma: 1.4,
        };
        assert_eq!(
            film_state(&edge, 0.0, 550.0, PRANDTL_AIR, Regime::Turbulent),
            Err(BoundaryLayerError::NonPositiveLength)
        );
        let bad = EdgeState {
            temperature_k: -1.0,
            ..edge
        };
        assert_eq!(bad.validate(), Err(BoundaryLayerError::InvalidEdgeState));
    }

    /// The film coefficient must fall with running length: the boundary layer
    /// thickens downstream and insulates the surface. A sign error in the
    /// Reynolds exponent would reverse this.
    #[test]
    fn the_film_coefficient_decays_downstream() {
        let edge = EdgeState {
            temperature_k: 222.65,
            pressure_pa: 2153.09,
            velocity_m_s: 1046.95,
            mach: 3.5,
            gamma: 1.4,
        };
        let mut previous = f64::INFINITY;
        for x in [0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 40.0] {
            let f = film_state(&edge, x, 550.0, PRANDTL_AIR, Regime::Turbulent).unwrap();
            assert!(f.heat_transfer_coefficient_w_m2_k < previous, "x={x}");
            assert!(f.reynolds_x > 1e5, "x={x}: Re={}", f.reynolds_x);
            previous = f.heat_transfer_coefficient_w_m2_k;
        }
    }
}
