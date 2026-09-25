//! M2 — Compressible flow relations for a calorically perfect gas.
//!
//! Yardstick: NACA Report 1135. See `cases/naca1135.toml`.
//!
//! ADR-000 D10: EVERY relation takes gamma as an explicit parameter. Nothing
//! here hard-codes 1.4. Retrofitting that later would touch every signature, and
//! M4 needs gamma ~ 1.30 in the burner where 1.4 is a failure, not a limit.
//!
//! # How this module is validated
//!
//! The printed tables are recited from memory and have already proved unreliable
//! in the last digit elsewhere in this project. So the primary checks are
//! **analytic identities that depend on no table at all**:
//!
//!   - the Prandtl relation M1* . M2* = 1 across a normal shock,
//!   - mass, momentum and energy conservation across the computed jump,
//!   - p0/p = (T0/T)^(gamma/(gamma-1)) exactly,
//!   - entropy across a shock is non-negative, and zero only at M = 1,
//!   - every ratio tends to 1 as M -> 1,
//!   - the density ratio tends to (gamma+1)/(gamma-1) as M -> infinity,
//!   - an oblique shock at beta = 90 deg reduces to the normal shock,
//!   - an oblique shock at beta = mach angle has zero deflection and zero loss,
//!   - the closed-form theta-beta-M inverse agrees with a bisection solve of the
//!     forward relation,
//!   - Prandtl-Meyer round-trips, and nu(inf) = (pi/2)(sqrt((g+1)/(g-1)) - 1).
//!
//! The table values are a second, independent check carried at the precision the
//! table actually has.
//!
//! `no_std`, no allocation. Transcendentals come from `libm` (ADR-001), so
//! results are bit-identical on every platform.
#![no_std]
#![forbid(unsafe_code)]

#[cfg(test)]
extern crate std;

pub mod gas;
pub mod janaf;
pub mod isentropic;
pub mod prandtl_meyer;
pub mod shock;

pub use gas::{gamma_air, molar_heat_capacity_air_j_mol_k, specific_heat_air_j_kg_k};
pub use janaf::{
    gamma_air_janaf, molar_heat_capacity_air_janaf_j_mol_k, specific_heat_air_janaf_j_kg_k,
};
pub use isentropic::{
    area_ratio, stagnation_density_ratio, stagnation_pressure_ratio, stagnation_temperature_ratio,
};
pub use prandtl_meyer::{mach_from_prandtl_meyer_rad, nu_max_rad, prandtl_meyer_rad};
pub use shock::{
    mach_angle_rad, max_deflection_rad, normal_shock, oblique_shock, NormalShock, ObliqueShock,
    ShockBranch,
};

/// Why a relation refused to answer.
///
/// These are refusals, not clamps. A supersonic relation asked about subsonic
/// flow must say so: returning a plausible number would corrupt everything
/// downstream while looking like it worked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GasDynError {
    /// A relation valid only for M >= 1 was given subsonic flow.
    SubsonicUpstream,
    /// Mach number was negative.
    NegativeMach,
    /// `gamma` must be finite and strictly greater than 1.
    InvalidGamma,
    /// An input was NaN.
    NotANumber,
    /// The requested flow deflection exceeds the maximum this Mach number can
    /// turn through with an attached shock. Physically the shock detaches; this
    /// module does not model detached shocks.
    DeflectionExceedsMaximum,
    /// Requested Prandtl-Meyer angle exceeds `nu_max` for this gamma, so no
    /// finite Mach number produces it.
    BeyondPrandtlMeyerLimit,
    /// An iterative solve did not converge within its fixed iteration budget.
    /// Reported rather than returning the last iterate.
    DidNotConverge,
    /// A temperature-dependent property was asked for outside the range its
    /// correlation was fitted over. Extrapolating a curve fit past its data is
    /// how a plausible-looking wrong number enters a model.
    OutsideCorrelationRange,
}

/// `gamma` must be finite and > 1, or the relations are not merely inaccurate,
/// they are undefined: exponents like `gamma/(gamma-1)` blow up.
#[inline]
pub(crate) fn check_gamma(gamma: f64) -> Result<(), GasDynError> {
    if gamma.is_nan() {
        return Err(GasDynError::NotANumber);
    }
    if !gamma.is_finite() || gamma <= 1.0 {
        return Err(GasDynError::InvalidGamma);
    }
    Ok(())
}

/// Mach number must be finite and non-negative.
#[inline]
pub(crate) fn check_mach(mach: f64) -> Result<(), GasDynError> {
    if mach.is_nan() {
        return Err(GasDynError::NotANumber);
    }
    if mach < 0.0 {
        return Err(GasDynError::NegativeMach);
    }
    Ok(())
}

/// Mach number must additionally be supersonic.
#[inline]
pub(crate) fn check_supersonic(mach: f64) -> Result<(), GasDynError> {
    check_mach(mach)?;
    if mach < 1.0 {
        return Err(GasDynError::SubsonicUpstream);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gamma_is_validated_rather_than_assumed() {
        assert_eq!(check_gamma(1.0), Err(GasDynError::InvalidGamma));
        assert_eq!(check_gamma(0.5), Err(GasDynError::InvalidGamma));
        assert_eq!(check_gamma(-1.4), Err(GasDynError::InvalidGamma));
        assert_eq!(check_gamma(f64::INFINITY), Err(GasDynError::InvalidGamma));
        assert_eq!(check_gamma(f64::NAN), Err(GasDynError::NotANumber));
        assert_eq!(check_gamma(1.4), Ok(()));
        assert_eq!(check_gamma(1.304), Ok(())); // M4 burner
        assert_eq!(check_gamma(5.0 / 3.0), Ok(())); // monatomic
    }

    #[test]
    fn subsonic_input_to_a_supersonic_relation_is_refused() {
        assert_eq!(check_supersonic(0.9), Err(GasDynError::SubsonicUpstream));
        assert_eq!(check_supersonic(-0.1), Err(GasDynError::NegativeMach));
        assert_eq!(check_supersonic(f64::NAN), Err(GasDynError::NotANumber));
        assert_eq!(check_supersonic(1.0), Ok(()));
    }
}
