//! Stagnation-point heating after Fay and Riddell, and the radiating wall it
//! implies at a declared nose or leading-edge radius.
//!
//! The flat-plate balance in [`crate::radiative_equilibrium`] is the right
//! model for the panels. It is the wrong model at the nose and the leading
//! edges, where the boundary layer starts from a stagnation point behind a
//! bow shock. That is this module.
//!
//! # Correlation
//!
//! Engineering form of Fay and Riddell (1958) for undissociated air:
//!
//! ```text
//! q = C * Pr^{-0.6} * (rho_e mu_e)^{0.4} * (rho_w mu_w)^{0.1}
//!       * sqrt(du_e/dx) * (h_e - h_w)
//! ```
//!
//! `C = 0.763` on a sphere (axisymmetric nose), `C = 0.570` on a cylinder
//! (unswept leading edge). The velocity gradient is the modified-Newtonian
//! stagnation value
//!
//! ```text
//! du_e/dx = (1/R_n) * sqrt(2 (p_e - p_inf) / rho_e)
//! ```
//!
//! with `(p_e, rho_e, T_e)` the post-normal-shock stagnation state from M2.
//! Recovery at a stagnation point is perfect: `T_aw = T0`, `h_e = cp T0`.
//!
//! The Lewis-number / dissociation-enthalpy term in the 1958 paper is dropped.
//! At the VENTUS-1 design point T0 is 768 K calorically perfect (753 K
//! thermally perfect). O2 dissociation is a ~2000 K phenomenon. Including a
//! recombination term here would be theatre.
//!
//! # The radius is an input, not a save
//!
//! Heat flux scales as `1/sqrt(R_n)`. A larger radius cools the stagnation
//! point. The defaults below are structural minima of SR-71-class order,
//! marked `[TO DETERMINE]`, and they are **not** the radii that would keep
//! Ti-6Al-4V alive. Inventing a blunt nose to save an alloy is refused; the
//! radius that would do it is reported as a diagnostic, not adopted.

use crate::radiative_equilibrium::ThermalError;
use ventus_atmos::State as AtmosState;
use ventus_gasdyn::{
    normal_shock, stagnation_pressure_ratio, stagnation_temperature_ratio, GasDynError,
};
use ventus_units::constants::{
    GAMMA_AIR_CALORICALLY_PERFECT, R_AIR_J_KG_K, STEFAN_BOLTZMANN_W_M2_K4,
};

fn map_gasdyn(err: GasDynError) -> ThermalError {
    match err {
        GasDynError::SubsonicUpstream => ThermalError::Subsonic,
        GasDynError::NotANumber => ThermalError::NotANumber,
        GasDynError::NegativeMach
        | GasDynError::InvalidGamma
        | GasDynError::DeflectionExceedsMaximum
        | GasDynError::BeyondPrandtlMeyerLimit
        | GasDynError::DidNotConverge
        | GasDynError::OutsideCorrelationRange => ThermalError::InvalidFreestream,
    }
}

/// Fay-Riddell prefactor for an axisymmetric (spherical) stagnation point.
///
/// Source: Fay, J. A. and Riddell, F. R., "Theory of Stagnation Point Heat
/// Transfer in Dissociated Air", Journal of the Aeronautical Sciences,
/// Vol. 25, No. 2, 1958 -- the equilibrium-air coefficient as carried in the
/// engineering form (Anderson; Bertin). **[TO CITE]** the page in Anderson
/// for the 0.763 spelling; Fay and Riddell printed 0.76.
pub const SPHERE_COEFFICIENT: f64 = 0.763;

/// Fay-Riddell prefactor for a two-dimensional (cylindrical) stagnation
/// point -- an unswept leading edge.
///
/// Source: the 2-D analog of the same correlation (Reshotko / Cohen lineage).
/// **[TO CITE]** before a layout uses this number to size a spar cap.
pub const CYLINDER_COEFFICIENT: f64 = 0.570;

/// Declared VENTUS-1 nose radius used for the stagnation-point computation [m].
///
/// **[TO DETERMINE]** against a real nose drawing. 0.025 m (one inch) is a
/// structural-minimum, SR-71-class order of magnitude, not a cited airframe
/// dimension. It is not a radius chosen so that any alloy survives: heat flux
/// falls as `1/sqrt(R)`, and a blunt nose that kept Ti-6Al-4V alive would be a
/// different aircraft. The already-declared cowl-lip structural minimum in
/// `ventus-inlet` is `r/R = 0.02` on a ~0.9 m highlight -- about 18 mm --
/// the same order.
pub const VENTUS_NOSE_RADIUS_M: f64 = 0.025;

/// Declared unswept leading-edge radius [m].
///
/// **[TO DETERMINE]**. Sharper than the nose: a wing leading edge is a
/// structural minimum, not a blunt body. Sweep is not applied -- the geometry
/// does not declare a sweep angle, and a `cos^n(Lambda)` factor would be a
/// silent save.
pub const VENTUS_LEADING_EDGE_RADIUS_M: f64 = 0.010;

const _: () = assert!(
    VENTUS_NOSE_RADIUS_M > 0.0,
    "a zero nose radius is a spike, not a stagnation point"
);
const _: () = assert!(
    VENTUS_LEADING_EDGE_RADIUS_M > 0.0,
    "a zero leading-edge radius is a knife, not a stagnation point"
);
const _: () = assert!(
    VENTUS_LEADING_EDGE_RADIUS_M <= VENTUS_NOSE_RADIUS_M,
    "the declared leading edge is no longer sharper than the nose; the conservative LE default has been inverted"
);

/// Which stagnation-point body the correlation is evaluated on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyKind {
    /// Axisymmetric nose.
    Sphere,
    /// Unswept two-dimensional leading edge.
    Cylinder,
}

impl BodyKind {
    /// Fay-Riddell prefactor for this body.
    #[must_use]
    pub const fn coefficient(self) -> f64 {
        match self {
            BodyKind::Sphere => SPHERE_COEFFICIENT,
            BodyKind::Cylinder => CYLINDER_COEFFICIENT,
        }
    }
}

/// Freestream state Fay-Riddell needs. Density is carried explicitly because
/// the velocity gradient uses it, and reconstructing it from `p/(R T)` inside
/// the correlation would hide a disagreement with M1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Freestream {
    pub temperature_k: f64,
    pub pressure_pa: f64,
    pub density_kg_m3: f64,
    pub velocity_m_s: f64,
    pub mach: f64,
    pub gamma: f64,
}

impl Freestream {
    /// Build from an M1 atmosphere row and a Mach number.
    ///
    /// Velocity is `M * a` from the atmosphere's own speed of sound, so a
    /// caller cannot hand this module a Mach number that disagrees with the
    /// speed it flies at.
    ///
    /// # Errors
    /// [`ThermalError::InvalidFreestream`] for a non-physical row;
    /// [`ThermalError::Subsonic`] at Mach <= 1; [`ThermalError::NotANumber`]
    /// for NaN.
    pub fn from_atmos(atmos: &AtmosState, mach: f64, gamma: f64) -> Result<Self, ThermalError> {
        if atmos.temperature_k.is_nan()
            || atmos.pressure_pa.is_nan()
            || atmos.density_kg_m3.is_nan()
            || atmos.speed_of_sound_m_s.is_nan()
            || mach.is_nan()
            || gamma.is_nan()
        {
            return Err(ThermalError::NotANumber);
        }
        if mach <= 1.0 {
            return Err(ThermalError::Subsonic);
        }
        if atmos.temperature_k <= 0.0
            || atmos.pressure_pa <= 0.0
            || atmos.density_kg_m3 <= 0.0
            || atmos.speed_of_sound_m_s <= 0.0
            || gamma <= 1.0
        {
            return Err(ThermalError::InvalidFreestream);
        }
        Ok(Self {
            temperature_k: atmos.temperature_k,
            pressure_pa: atmos.pressure_pa,
            density_kg_m3: atmos.density_kg_m3,
            velocity_m_s: mach * atmos.speed_of_sound_m_s,
            mach,
            gamma,
        })
    }

    /// # Errors
    /// See [`Freestream::from_atmos`].
    pub fn validate(&self) -> Result<(), ThermalError> {
        if self.temperature_k.is_nan()
            || self.pressure_pa.is_nan()
            || self.density_kg_m3.is_nan()
            || self.velocity_m_s.is_nan()
            || self.mach.is_nan()
            || self.gamma.is_nan()
        {
            return Err(ThermalError::NotANumber);
        }
        if self.mach <= 1.0 {
            return Err(ThermalError::Subsonic);
        }
        if self.temperature_k <= 0.0
            || self.pressure_pa <= 0.0
            || self.density_kg_m3 <= 0.0
            || self.velocity_m_s <= 0.0
            || self.gamma <= 1.0
        {
            return Err(ThermalError::InvalidFreestream);
        }
        Ok(())
    }
}

/// Post-shock stagnation state at the boundary-layer edge.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StagnationEdge {
    /// Stagnation temperature [K]. Equal to calorically-perfect T0.
    pub temperature_k: f64,
    /// Pitot (Rayleigh) pressure [Pa].
    pub pressure_pa: f64,
    pub density_kg_m3: f64,
    /// Modified-Newtonian velocity gradient times the radius: `R * du_e/dx` [m/s].
    pub velocity_gradient_times_radius_m_s: f64,
}

/// Evaluate the post-shock stagnation edge from the freestream.
///
/// T_e is the calorically perfect stagnation temperature (energy). p_e is the
/// Rayleigh pitot pressure: isentropic p0 times the normal-shock total-pressure
/// ratio from M2. The two cannot drift apart because both come from M2.
///
/// # Errors
/// [`ThermalError`] if the freestream is refused or M2 refuses the shock.
pub fn stagnation_edge(freestream: &Freestream) -> Result<StagnationEdge, ThermalError> {
    freestream.validate()?;
    let shock = normal_shock(freestream.mach, freestream.gamma).map_err(map_gasdyn)?;
    let t0_ratio =
        stagnation_temperature_ratio(freestream.mach, freestream.gamma).map_err(map_gasdyn)?;
    let p0_ratio =
        stagnation_pressure_ratio(freestream.mach, freestream.gamma).map_err(map_gasdyn)?;

    let temperature_k = freestream.temperature_k * t0_ratio;
    // Rayleigh pitot: p_e = p_inf * (p01/p1) * (p02/p01).
    let pressure_pa = freestream.pressure_pa * p0_ratio * shock.stagnation_pressure_ratio;
    let density_kg_m3 = pressure_pa / (R_AIR_J_KG_K * temperature_k);
    let dp = pressure_pa - freestream.pressure_pa;
    if dp <= 0.0 || density_kg_m3 <= 0.0 {
        return Err(ThermalError::InvalidFreestream);
    }
    Ok(StagnationEdge {
        temperature_k,
        pressure_pa,
        density_kg_m3,
        velocity_gradient_times_radius_m_s: libm::sqrt(2.0 * dp / density_kg_m3),
    })
}

/// Heat flux and the intermediate state that produced it, at a prescribed wall
/// temperature. The radiation balance iterates this.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FayRiddellHeatFlux {
    pub flux_w_m2: f64,
    pub stagnation_temperature_k: f64,
    pub edge_pressure_pa: f64,
    pub edge_density_kg_m3: f64,
    pub velocity_gradient_per_s: f64,
    /// `q / (h_e - h_w)` [kg/(m^2 s)]. Useful as a film coefficient analog;
    /// zero when the wall sits at T0.
    pub mass_rate_kg_m2_s: f64,
}

/// Stagnation-point heat flux at a prescribed wall temperature.
///
/// Calorically perfect enthalpies: `h = cp T` with `cp = gamma R / (gamma - 1)`,
/// the same gamma the shock was evaluated at. The flat-plate module uses the
/// same gas; mixing thermally perfect T0 into this correlation would move the
/// driving enthalpy by the 15 K of design-point.md section 3.1 and would have
/// to be named as such. It is not mixed in here.
///
/// # Errors
/// [`ThermalError`] for a refused freestream, a non-positive radius, a
/// non-positive wall temperature, or a non-positive Prandtl number.
pub fn heat_flux(
    freestream: &Freestream,
    radius_m: f64,
    wall_temperature_k: f64,
    prandtl: f64,
    body: BodyKind,
) -> Result<FayRiddellHeatFlux, ThermalError> {
    let edge = stagnation_edge(freestream)?;
    check_radius(radius_m)?;
    if prandtl.is_nan() || wall_temperature_k.is_nan() {
        return Err(ThermalError::NotANumber);
    }
    if prandtl <= 0.0 || !prandtl.is_finite() {
        return Err(ThermalError::InvalidPrandtl);
    }
    if wall_temperature_k <= 0.0 || !wall_temperature_k.is_finite() {
        return Err(ThermalError::InvalidFreestream);
    }

    let velocity_gradient_per_s = edge.velocity_gradient_times_radius_m_s / radius_m;
    let cp = freestream.gamma * R_AIR_J_KG_K / (freestream.gamma - 1.0);
    let dh = cp * (edge.temperature_k - wall_temperature_k);

    // A wall at T0 exchanges no heat, regardless of the transfer coefficient.
    // Handled explicitly so a vanishing driving potential cannot be confused
    // with a vanishing geometry.
    if dh == 0.0 {
        return Ok(FayRiddellHeatFlux {
            flux_w_m2: 0.0,
            stagnation_temperature_k: edge.temperature_k,
            edge_pressure_pa: edge.pressure_pa,
            edge_density_kg_m3: edge.density_kg_m3,
            velocity_gradient_per_s,
            mass_rate_kg_m2_s: 0.0,
        });
    }

    let mu_e = ventus_atmos::dynamic_viscosity_pa_s(edge.temperature_k);
    let mu_w = ventus_atmos::dynamic_viscosity_pa_s(wall_temperature_k);
    let rho_w = edge.pressure_pa / (R_AIR_J_KG_K * wall_temperature_k);
    let rho_mu_e = edge.density_kg_m3 * mu_e;
    let rho_mu_w = rho_w * mu_w;
    if rho_mu_e <= 0.0 || rho_mu_w <= 0.0 {
        return Err(ThermalError::InvalidFreestream);
    }

    let mass_rate_kg_m2_s = body.coefficient()
        * libm::pow(prandtl, -0.6)
        * libm::pow(rho_mu_e, 0.4)
        * libm::pow(rho_mu_w, 0.1)
        * libm::sqrt(velocity_gradient_per_s);

    Ok(FayRiddellHeatFlux {
        flux_w_m2: mass_rate_kg_m2_s * dh,
        stagnation_temperature_k: edge.temperature_k,
        edge_pressure_pa: edge.pressure_pa,
        edge_density_kg_m3: edge.density_kg_m3,
        velocity_gradient_per_s,
        mass_rate_kg_m2_s,
    })
}

/// A converged radiation balance at a stagnation point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StagnationBalance {
    pub wall_temperature_k: f64,
    pub convective_flux_w_m2: f64,
    pub radiative_flux_w_m2: f64,
    pub stagnation_temperature_k: f64,
    pub edge_pressure_pa: f64,
    pub edge_density_kg_m3: f64,
    pub velocity_gradient_per_s: f64,
    pub radius_m: f64,
    pub body: BodyKind,
}

impl StagnationBalance {
    /// How much radiation is worth, in kelvin: T0 minus the radiating wall.
    /// At a stagnation point recovery is perfect, so this is the gap below T0
    /// rather than below a flat-plate T_aw.
    #[must_use]
    pub fn radiation_relief_k(&self) -> f64 {
        self.stagnation_temperature_k - self.wall_temperature_k
    }
}

/// Solve the radiation balance at a stagnation point.
///
/// Same equation as the flat plate, `q_conv(T_w) = eps sigma (T_w^4 - T_sink^4)`,
/// with `q_conv` from [`heat_flux`] instead of a running-length film
/// coefficient. Bisection on `[T_lo, T0]` with a fixed iteration budget.
///
/// # Errors
/// [`ThermalError`] for refused inputs, including an emissivity outside [0, 1]
/// and a negative sink temperature.
pub fn stagnation_radiation_equilibrium(
    freestream: &Freestream,
    radius_m: f64,
    emissivity: f64,
    sink_temperature_k: f64,
    prandtl: f64,
    body: BodyKind,
) -> Result<StagnationBalance, ThermalError> {
    if emissivity.is_nan() || !(0.0..=1.0).contains(&emissivity) {
        return Err(ThermalError::InvalidEmissivity);
    }
    if sink_temperature_k.is_nan() || sink_temperature_k < 0.0 || !sink_temperature_k.is_finite() {
        return Err(ThermalError::InvalidSinkTemperature);
    }
    check_radius(radius_m)?;
    let edge = stagnation_edge(freestream)?;
    let t0 = edge.temperature_k;

    if emissivity == 0.0 {
        let flux = heat_flux(freestream, radius_m, t0, prandtl, body)?;
        return Ok(StagnationBalance {
            wall_temperature_k: t0,
            convective_flux_w_m2: 0.0,
            radiative_flux_w_m2: 0.0,
            stagnation_temperature_k: t0,
            edge_pressure_pa: flux.edge_pressure_pa,
            edge_density_kg_m3: flux.edge_density_kg_m3,
            velocity_gradient_per_s: flux.velocity_gradient_per_s,
            radius_m,
            body,
        });
    }

    let sink4 = sink_temperature_k * sink_temperature_k * sink_temperature_k * sink_temperature_k;
    let net = |t_w: f64| -> Result<(f64, FayRiddellHeatFlux, f64), ThermalError> {
        let flux = heat_flux(freestream, radius_m, t_w, prandtl, body)?;
        let radiative = emissivity * STEFAN_BOLTZMANN_W_M2_K4 * (t_w * t_w * t_w * t_w - sink4);
        Ok((flux.flux_w_m2 - radiative, flux, radiative))
    };

    // Lower bracket: above zero so density and T^4 are defined, and not below
    // the sink (a wall cannot radiatively equilibrate colder than what it
    // radiates to). Convection drives toward T0, so the root sits in this
    // interval.
    let mut lo = if sink_temperature_k > 1.0 {
        sink_temperature_k
    } else {
        1.0
    };
    if lo >= t0 {
        lo = 0.5 * t0;
    }
    let mut hi = t0;
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        if net(mid)?.0 > 0.0 {
            lo = mid;
        } else {
            hi = mid;
        }
        if hi - lo < 1e-12 * hi {
            break;
        }
    }

    let t_wall = 0.5 * (lo + hi);
    let (_, flux, radiative) = net(t_wall)?;
    Ok(StagnationBalance {
        wall_temperature_k: t_wall,
        convective_flux_w_m2: flux.flux_w_m2,
        radiative_flux_w_m2: radiative,
        stagnation_temperature_k: t0,
        edge_pressure_pa: flux.edge_pressure_pa,
        edge_density_kg_m3: flux.edge_density_kg_m3,
        velocity_gradient_per_s: flux.velocity_gradient_per_s,
        radius_m,
        body,
    })
}

/// VENTUS-1 design-point nose: sphere at [`VENTUS_NOSE_RADIUS_M`].
///
/// # Errors
/// See [`stagnation_radiation_equilibrium`].
pub fn ventus1_nose(
    freestream: &Freestream,
    emissivity: f64,
    sink_temperature_k: f64,
    prandtl: f64,
) -> Result<StagnationBalance, ThermalError> {
    stagnation_radiation_equilibrium(
        freestream,
        VENTUS_NOSE_RADIUS_M,
        emissivity,
        sink_temperature_k,
        prandtl,
        BodyKind::Sphere,
    )
}

/// VENTUS-1 design-point leading edge: unswept cylinder at
/// [`VENTUS_LEADING_EDGE_RADIUS_M`].
///
/// # Errors
/// See [`stagnation_radiation_equilibrium`].
pub fn ventus1_leading_edge(
    freestream: &Freestream,
    emissivity: f64,
    sink_temperature_k: f64,
    prandtl: f64,
) -> Result<StagnationBalance, ThermalError> {
    stagnation_radiation_equilibrium(
        freestream,
        VENTUS_LEADING_EDGE_RADIUS_M,
        emissivity,
        sink_temperature_k,
        prandtl,
        BodyKind::Cylinder,
    )
}

fn check_radius(radius_m: f64) -> Result<(), ThermalError> {
    if radius_m.is_nan() {
        return Err(ThermalError::NotANumber);
    }
    if radius_m <= 0.0 || !radius_m.is_finite() {
        return Err(ThermalError::InvalidRadius);
    }
    Ok(())
}

/// Default gamma used when a caller has not picked one: calorically perfect
/// air, matching the rest of M5.
pub const DEFAULT_GAMMA: f64 = GAMMA_AIR_CALORICALLY_PERFECT;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::radiative_equilibrium::{lightest_survivor, CANDIDATES};
    use ventus_aero::boundary_layer::PRANDTL_AIR;
    use ventus_units::float::rel_err;

    fn design_freestream() -> Freestream {
        let a = ventus_atmos::at_geopotential(26_000.0).unwrap();
        Freestream::from_atmos(&a, 3.5, DEFAULT_GAMMA).unwrap()
    }

    fn nose_at(altitude_m: f64, mach: f64, radius_m: f64, body: BodyKind) -> StagnationBalance {
        let a = ventus_atmos::at_geopotential(altitude_m).unwrap();
        let fs = Freestream::from_atmos(&a, mach, DEFAULT_GAMMA).unwrap();
        stagnation_radiation_equilibrium(&fs, radius_m, 0.85, 0.0, PRANDTL_AIR, body).unwrap()
    }

    /// Rayleigh pitot identity: p_e is M2's isentropic p0 times the normal-shock
    /// total-pressure ratio, not a free constant.
    #[test]
    fn the_stagnation_edge_is_the_rayleigh_pitot_state() {
        let fs = design_freestream();
        let edge = stagnation_edge(&fs).unwrap();
        let shock = normal_shock(fs.mach, fs.gamma).unwrap();
        let p0_ratio = stagnation_pressure_ratio(fs.mach, fs.gamma).unwrap();
        let t0_ratio = stagnation_temperature_ratio(fs.mach, fs.gamma).unwrap();

        assert!(rel_err(edge.temperature_k, fs.temperature_k * t0_ratio) < 1e-15);
        assert!(
            rel_err(
                edge.pressure_pa,
                fs.pressure_pa * p0_ratio * shock.stagnation_pressure_ratio
            ) < 1e-15
        );
        assert!(
            rel_err(
                edge.density_kg_m3,
                edge.pressure_pa / (R_AIR_J_KG_K * edge.temperature_k)
            ) < 1e-15
        );
        // Design-point T0 calorically perfect is 768.1 K in design-point.md 3.1.
        assert!(
            rel_err(edge.temperature_k, 768.1) < 5e-4,
            "T0 = {:.2} K, expected 768.1 K calorically perfect",
            edge.temperature_k
        );
    }

    #[test]
    fn a_wall_at_t0_exchanges_no_heat() {
        let fs = design_freestream();
        let edge = stagnation_edge(&fs).unwrap();
        let q = heat_flux(
            &fs,
            VENTUS_NOSE_RADIUS_M,
            edge.temperature_k,
            PRANDTL_AIR,
            BodyKind::Sphere,
        )
        .unwrap();
        assert_eq!(q.flux_w_m2, 0.0);
        assert_eq!(q.mass_rate_kg_m2_s, 0.0);
    }

    /// Cold-wall heat flux must scale as 1/sqrt(R). This is the geometry
    /// dependence the whole argument about "not blunting to save Ti" rests on,
    /// so it is asserted rather than left as a comment.
    #[test]
    fn cold_wall_heat_flux_scales_as_one_over_sqrt_radius() {
        let fs = design_freestream();
        let t_w = 300.0;
        let q1 = heat_flux(&fs, 0.025, t_w, PRANDTL_AIR, BodyKind::Sphere)
            .unwrap()
            .flux_w_m2;
        let q2 = heat_flux(&fs, 0.100, t_w, PRANDTL_AIR, BodyKind::Sphere)
            .unwrap()
            .flux_w_m2;
        assert!(q1 > 0.0 && q2 > 0.0);
        // (rho_w mu_w)^{0.1} is identical at the same T_w, so the only R
        // dependence is sqrt(du_e/dx) ~ 1/sqrt(R). Factor exactly 2.
        assert!(
            rel_err(q1 / q2, 2.0) < 1e-12,
            "q(0.025)/q(0.100) = {}, expected 2",
            q1 / q2
        );
    }

    /// A sphere runs hotter than a cylinder at the same radius: 3-D stagnation
    /// has the higher transfer coefficient. An inverted ordering would mean the
    /// prefactors were swapped, and the LE would be reported as the easy end.
    #[test]
    fn a_sphere_runs_hotter_than_a_cylinder_at_the_same_radius() {
        let sphere = nose_at(26_000.0, 3.5, 0.025, BodyKind::Sphere);
        let cylinder = nose_at(26_000.0, 3.5, 0.025, BodyKind::Cylinder);
        assert!(
            sphere.wall_temperature_k > cylinder.wall_temperature_k,
            "sphere {:.1} K, cylinder {:.1} K",
            sphere.wall_temperature_k,
            cylinder.wall_temperature_k
        );
        assert!(
            rel_err(
                sphere.velocity_gradient_per_s,
                cylinder.velocity_gradient_per_s
            ) < 1e-15
        );
    }

    #[test]
    fn a_larger_radius_cools_the_wall() {
        let mut previous = f64::INFINITY;
        for r in [0.005, 0.010, 0.025, 0.050, 0.100, 0.250] {
            let t = nose_at(26_000.0, 3.5, r, BodyKind::Sphere).wall_temperature_k;
            assert!(t < previous, "R={r}: wall did not cool with blunting");
            previous = t;
        }
    }

    #[test]
    fn the_converged_stagnation_balance_closes() {
        for (h, m, r, body) in [
            (26_000.0, 3.5, VENTUS_NOSE_RADIUS_M, BodyKind::Sphere),
            (
                26_000.0,
                3.5,
                VENTUS_LEADING_EDGE_RADIUS_M,
                BodyKind::Cylinder,
            ),
            (24_000.0, 3.2, VENTUS_NOSE_RADIUS_M, BodyKind::Sphere),
        ] {
            let b = nose_at(h, m, r, body);
            assert!(
                rel_err(b.convective_flux_w_m2, b.radiative_flux_w_m2) < 1e-9,
                "h={h} M={m} R={r}: {} in, {} out",
                b.convective_flux_w_m2,
                b.radiative_flux_w_m2
            );
            assert!(b.convective_flux_w_m2 > 0.0);
            assert!(b.wall_temperature_k < b.stagnation_temperature_k);
            assert!(b.wall_temperature_k > 200.0);
        }
    }

    #[test]
    fn a_non_radiating_stagnation_point_sits_at_t0() {
        let fs = design_freestream();
        let b = stagnation_radiation_equilibrium(
            &fs,
            VENTUS_NOSE_RADIUS_M,
            0.0,
            0.0,
            PRANDTL_AIR,
            BodyKind::Sphere,
        )
        .unwrap();
        let edge = stagnation_edge(&fs).unwrap();
        assert!(rel_err(b.wall_temperature_k, edge.temperature_k) < 1e-12);
        assert_eq!(b.convective_flux_w_m2, 0.0);
    }

    /// THE RESULT. At the declared structural-minimum nose, Ti-6Al-4V is dead
    /// and Inconel is alive. That is the expectation design-point.md carried
    /// as prose; this test is the computation that replaces the prose.
    #[test]
    fn ti6al4v_dies_at_the_declared_nose_and_inconel_does_not() {
        let fs = design_freestream();
        let nose = ventus1_nose(&fs, 0.85, 0.0, PRANDTL_AIR).unwrap();
        let le = ventus1_leading_edge(&fs, 0.85, 0.0, PRANDTL_AIR).unwrap();

        let ti = CANDIDATES.iter().find(|m| m.name == "Ti-6Al-4V").unwrap();
        let ti_ht = CANDIDATES.iter().find(|m| m.name == "Ti-6242S").unwrap();
        let inconel = CANDIDATES.iter().find(|m| m.name == "Inconel 718").unwrap();

        // Pin the computed envelope so a silent change in the correlation is
        // news. These bands are this model's output at the declared radii,
        // not a published yardstick -- the SR-71 residual is a separate test.
        for (label, b) in [("nose", nose), ("LE", le)] {
            assert!(
                (720.0..740.0).contains(&b.wall_temperature_k),
                "{label}: T_wall = {:.1} K is outside the envelope this model produced at the declared radii",
                b.wall_temperature_k
            );
            assert!(
                b.wall_temperature_k > ti.sustained_limit_k,
                "{label}: Ti-6Al-4V survived {:.1} K",
                b.wall_temperature_k
            );
            assert!(
                b.wall_temperature_k < ti_ht.sustained_limit_k,
                "{label}: Ti-6242S died at {:.1} K",
                b.wall_temperature_k
            );
            assert!(
                b.wall_temperature_k < inconel.sustained_limit_k,
                "{label}: Inconel died at {:.1} K",
                b.wall_temperature_k
            );
            assert_eq!(
                lightest_survivor(b.wall_temperature_k).unwrap().name,
                "Ti-6242S",
                "{label}"
            );
        }

        // The numbers docs/design-point.md section 3.2 quotes. rel 1e-3 is a
        // regression pin, not a published precision.
        assert!(
            rel_err(nose.wall_temperature_k, 729.9) < 1e-3,
            "nose T_wall = {:.2} K",
            nose.wall_temperature_k
        );
        assert!(
            rel_err(le.wall_temperature_k, 734.9) < 1e-3,
            "LE T_wall = {:.2} K",
            le.wall_temperature_k
        );
        assert!(
            rel_err(nose.convective_flux_w_m2, 13_683.0) < 2e-3,
            "nose q = {} W/m2",
            nose.convective_flux_w_m2
        );
        assert!(
            rel_err(le.convective_flux_w_m2, 14_059.0) < 2e-3,
            "LE q = {} W/m2",
            le.convective_flux_w_m2
        );

        // 17-7PH dies at the nose. The lightest survivor is therefore a
        // high-temperature titanium, not a stainless.
        let ph = CANDIDATES
            .iter()
            .find(|m| m.name.starts_with("17-7PH"))
            .unwrap();
        assert!(nose.wall_temperature_k > ph.sustained_limit_k);
        assert!(le.wall_temperature_k > ph.sustained_limit_k);
    }

    /// Blunting far enough to put the wall on the Ti-6Al-4V limit is a different
    /// aircraft: the required radius is a substantial fraction of the fuselage
    /// itself. The test names that radius as a refusal, not as a design option.
    #[test]
    fn the_radius_that_saves_ti6al4v_is_not_a_nose() {
        let fs = design_freestream();
        let ti = CANDIDATES.iter().find(|m| m.name == "Ti-6Al-4V").unwrap();
        // Walk radius up until the wall falls through 623 K. If it never does
        // inside a metre, that is also an answer: even a hemispherical fuselage
        // would not save the alloy.
        let mut r = VENTUS_NOSE_RADIUS_M;
        let mut saved_at = None;
        while r <= 1.5 {
            let t =
                stagnation_radiation_equilibrium(&fs, r, 0.85, 0.0, PRANDTL_AIR, BodyKind::Sphere)
                    .unwrap()
                    .wall_temperature_k;
            if t <= ti.sustained_limit_k {
                saved_at = Some(r);
                break;
            }
            r *= 1.15;
        }
        match saved_at {
            Some(r_save) => {
                assert!(
                    r_save > 0.20,
                    "Ti-6Al-4V came alive at R = {r_save:.3} m, which is still a plausible nose; \
                     if that is real it needs a drawing, not a correlation default"
                );
            }
            None => {
                // Even a 1.5 m radius -- larger than the fuselage radius -- did
                // not save Ti. That is the stronger form of the same refusal.
            }
        }
        // And the declared default is nowhere near that save.
        let declared = ventus1_nose(&fs, 0.85, 0.0, PRANDTL_AIR)
            .unwrap()
            .wall_temperature_k;
        assert!(declared > ti.sustained_limit_k + 40.0);
    }

    /// THE ANCHOR. SR-71 published nose ~ 315 C = 588 K at M 3.2. This model at
    /// the VENTUS declared radius does not reproduce that number, and it is
    /// not allowed to: we do not have a cited SR-71 nose radius, and enlarging
    /// ours until the thermometer agrees would be the save this module exists
    /// to refuse.
    ///
    /// The test asserts the residual is real and bounded, not that it is zero.
    #[test]
    fn the_sr71_nose_residual_is_stated_not_tuned_away() {
        let published_k = 588.15; // 315 C
        let b = nose_at(24_000.0, 3.2, VENTUS_NOSE_RADIUS_M, BodyKind::Sphere);
        let residual = b.wall_temperature_k - published_k;

        // The wall must sit between freestream and T0, and radiation must have
        // done some work -- the published 588 K is itself below T0 ~ 672 K.
        assert!(b.wall_temperature_k < b.stagnation_temperature_k);
        assert!(b.radiation_relief_k() > 10.0);

        // A residual of tens of kelvin is the honest outcome of applying a
        // VENTUS radius to an SR-71 flight condition. A residual of 150 K
        // would mean the correlation had come apart; a residual under 5 K
        // would mean the radius had been fitted, which is the failure mode.
        assert!(
            residual.abs() > 5.0,
            "SR-71 nose matched to {:.1} K at the VENTUS radius; that is a fit, not a prediction",
            b.wall_temperature_k
        );
        assert!(
            residual.abs() < 150.0,
            "SR-71 nose residual is {:.0} K (model {:.1} K vs published {:.1} K); \
             the correlation or the gas state has come apart",
            residual,
            b.wall_temperature_k,
            published_k
        );
    }

    #[test]
    fn stagnation_inputs_are_refused() {
        let fs = design_freestream();
        assert_eq!(
            heat_flux(&fs, 0.0, 300.0, PRANDTL_AIR, BodyKind::Sphere),
            Err(ThermalError::InvalidRadius)
        );
        assert_eq!(
            heat_flux(&fs, -0.01, 300.0, PRANDTL_AIR, BodyKind::Sphere),
            Err(ThermalError::InvalidRadius)
        );
        assert_eq!(
            heat_flux(&fs, 0.025, 300.0, 0.0, BodyKind::Sphere),
            Err(ThermalError::InvalidPrandtl)
        );
        assert_eq!(
            stagnation_radiation_equilibrium(&fs, 0.025, 1.1, 0.0, PRANDTL_AIR, BodyKind::Sphere),
            Err(ThermalError::InvalidEmissivity)
        );
        assert_eq!(
            stagnation_radiation_equilibrium(&fs, 0.025, 0.85, -1.0, PRANDTL_AIR, BodyKind::Sphere),
            Err(ThermalError::InvalidSinkTemperature)
        );

        let a = ventus_atmos::at_geopotential(26_000.0).unwrap();
        assert_eq!(
            Freestream::from_atmos(&a, 0.8, DEFAULT_GAMMA),
            Err(ThermalError::Subsonic)
        );
        assert_eq!(
            Freestream::from_atmos(&a, 1.0, DEFAULT_GAMMA),
            Err(ThermalError::Subsonic)
        );
        assert_eq!(
            Freestream::from_atmos(&a, f64::NAN, DEFAULT_GAMMA),
            Err(ThermalError::NotANumber)
        );
    }

    /// Dissociation is out of scope at this design point, stated as a bound
    /// rather than as a hope: T0 must remain well below the onset of O2
    /// dissociation or the dropped Lewis term is no longer an approximation.
    #[test]
    fn t0_is_far_below_oxygen_dissociation() {
        let edge = stagnation_edge(&design_freestream()).unwrap();
        assert!(
            edge.temperature_k < 1200.0,
            "T0 = {:.0} K has entered the regime Fay-Riddell was derived for; \
             the dropped dissociation term needs revisiting",
            edge.temperature_k
        );
    }
}
