//! Radiation-equilibrium wall temperature, and what survives it.
//!
//! A skin in high-speed flow reaches steady state when the heat convected into
//! it equals the heat it radiates away:
//!
//! ```text
//! h (T_aw - T_w) = eps sigma (T_w^4 - T_sink^4)
//! ```
//!
//! This is the calculation that decides the airframe material, and at the
//! VENTUS-1 design point it decides it in a way the recovery temperature alone
//! does not: T_aw is 436 C, above the sustained limit of Ti-6Al-4V, while the
//! radiating wall settles near 275 C, comfortably below it. **Radiation is what
//! keeps the flat panels in conventional titanium.** The margin belongs to the
//! radiation term, not to the material.
//!
//! # The sink temperature is an assumption, and it matters
//!
//! An upward-facing surface at 26 km radiates to a sky that is effectively very
//! cold; a downward-facing one sees a warm Earth. Taking the sink as 0 K is the
//! optimistic bound and is the default here because it is the standard textbook
//! treatment, but it is an input, not a constant, and a caller modelling a lower
//! surface should say so. The sensitivity is weak — T_sink enters as the fourth
//! power against a much hotter wall — but weak is not zero.

use ventus_aero::boundary_layer::{
    adiabatic_wall_temperature_k, film_state, BoundaryLayerError, EdgeState, FilmState, Regime,
};
use ventus_units::constants::STEFAN_BOLTZMANN_W_M2_K4;

/// Why a thermal balance refused to answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThermalError {
    /// Emissivity must lie in [0, 1].
    InvalidEmissivity,
    /// Sink temperature must be finite and non-negative.
    InvalidSinkTemperature,
    /// The boundary-layer model refused first.
    BoundaryLayer(BoundaryLayerError),
}

impl From<BoundaryLayerError> for ThermalError {
    fn from(e: BoundaryLayerError) -> Self {
        ThermalError::BoundaryLayer(e)
    }
}

/// A converged radiation balance at one station.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RadiationBalance {
    pub wall_temperature_k: f64,
    /// Heat convected into the wall at equilibrium [W/m^2].
    pub convective_flux_w_m2: f64,
    /// Heat radiated away at equilibrium [W/m^2]. Equal to the convective flux
    /// at the solution; both are reported so a caller can see the balance close
    /// rather than take it on trust.
    pub radiative_flux_w_m2: f64,
    /// Boundary-layer state evaluated at the converged wall temperature.
    pub film: FilmState,
}

impl RadiationBalance {
    /// How much radiation is worth, in kelvin: the gap between the adiabatic
    /// wall and the radiating one. This is the number that decides the material.
    #[must_use]
    pub fn radiation_relief_k(&self) -> f64 {
        self.film.adiabatic_wall_temperature_k - self.wall_temperature_k
    }
}

/// Solve the radiation balance for the wall temperature.
///
/// Bisection on the net flux over `[T_edge, T_aw]`. The bracket is guaranteed:
/// at the edge temperature convection dominates, at the recovery temperature
/// convection vanishes and radiation does not. The net flux is monotonically
/// decreasing in `T_w`, so bisection is safe and its iteration count is fixed —
/// no chance of an unbounded solve inside a model that eventually runs in
/// flight software.
///
/// # Errors
/// [`ThermalError`] for an emissivity outside [0, 1], a negative sink
/// temperature, or a boundary-layer refusal.
pub fn radiation_equilibrium_wall(
    edge: &EdgeState,
    running_length_m: f64,
    emissivity: f64,
    sink_temperature_k: f64,
    prandtl: f64,
    regime: Regime,
) -> Result<RadiationBalance, ThermalError> {
    if emissivity.is_nan() || !(0.0..=1.0).contains(&emissivity) {
        return Err(ThermalError::InvalidEmissivity);
    }
    if sink_temperature_k.is_nan() || sink_temperature_k < 0.0 || !sink_temperature_k.is_finite() {
        return Err(ThermalError::InvalidSinkTemperature);
    }

    let t_aw = adiabatic_wall_temperature_k(edge, prandtl, regime)?;

    // A non-radiating wall is adiabatic and sits at the recovery temperature by
    // definition. Handled explicitly: the bisection below would converge to the
    // same place, but only because its bracket happens to end there, which is
    // luck rather than logic.
    if emissivity == 0.0 {
        let film = film_state(edge, running_length_m, t_aw, prandtl, regime)?;
        return Ok(RadiationBalance {
            wall_temperature_k: t_aw,
            convective_flux_w_m2: 0.0,
            radiative_flux_w_m2: 0.0,
            film,
        });
    }

    let sink4 = sink_temperature_k * sink_temperature_k * sink_temperature_k * sink_temperature_k;
    let net = |t_w: f64| -> Result<(f64, f64, f64, FilmState), ThermalError> {
        let film = film_state(edge, running_length_m, t_w, prandtl, regime)?;
        let convective = film.heat_transfer_coefficient_w_m2_k * (t_aw - t_w);
        let radiative = emissivity * STEFAN_BOLTZMANN_W_M2_K4 * (t_w * t_w * t_w * t_w - sink4);
        Ok((convective - radiative, convective, radiative, film))
    };

    let (mut lo, mut hi) = (edge.temperature_k, t_aw);
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
    let (_, convective, radiative, film) = net(t_wall)?;
    Ok(RadiationBalance {
        wall_temperature_k: t_wall,
        convective_flux_w_m2: convective,
        radiative_flux_w_m2: radiative,
        film,
    })
}

/// A candidate airframe material.
///
/// `sustained_limit_k` is the temperature above which the alloy cannot be used
/// for long-duration structure — creep-limited for the titanium alloys rather
/// than strength-limited. These are engineering rules of thumb collected from
/// the literature, **[TO CITE]** individually before M7 sizes anything with them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Material {
    pub name: &'static str,
    pub sustained_limit_k: f64,
    pub density_kg_m3: f64,
}

/// The candidate set, ordered by density.
///
/// Aluminium is included precisely because it fails: a material list that only
/// contains materials that work cannot show that the design point excludes
/// anything.
pub const CANDIDATES: [Material; 7] = [
    Material {
        name: "Al 2618 (RR58, Concorde)",
        sustained_limit_k: 400.0,
        density_kg_m3: 2760.0,
    },
    Material {
        name: "Al 2024-T3",
        sustained_limit_k: 393.0,
        density_kg_m3: 2780.0,
    },
    Material {
        name: "Ti-6Al-4V",
        sustained_limit_k: 623.0,
        density_kg_m3: 4430.0,
    },
    Material {
        name: "Ti-6242S",
        sustained_limit_k: 813.0,
        density_kg_m3: 4540.0,
    },
    Material {
        name: "Ti beta B-120VCA (SR-71)",
        sustained_limit_k: 810.0,
        density_kg_m3: 4850.0,
    },
    Material {
        name: "17-7PH stainless",
        sustained_limit_k: 703.0,
        density_kg_m3: 7800.0,
    },
    Material {
        name: "Inconel 718",
        sustained_limit_k: 923.0,
        density_kg_m3: 8190.0,
    },
];

/// Every candidate that survives a sustained wall temperature.
pub fn survivors(wall_temperature_k: f64) -> impl Iterator<Item = &'static Material> {
    CANDIDATES
        .iter()
        .filter(move |m| m.sustained_limit_k >= wall_temperature_k)
}

/// The lightest candidate that survives, which is the one a mass-driven design
/// would pick. Returns `None` if nothing on the list survives — which is a real
/// answer, not an error: it means the design point needs a material this list
/// does not contain.
#[must_use]
pub fn lightest_survivor(wall_temperature_k: f64) -> Option<&'static Material> {
    survivors(wall_temperature_k).min_by(|a, b| {
        a.density_kg_m3
            .partial_cmp(&b.density_kg_m3)
            .unwrap_or(core::cmp::Ordering::Equal)
    })
}

/// Fractional thermal growth between two temperatures, `dL/L`.
///
/// Reported per unit length because the vehicle length is not yet defined
/// (design-point.md 5.2). Multiply by the airframe length to get the absolute
/// growth that a real structure has to accommodate.
#[must_use]
pub fn thermal_growth_per_metre(
    expansion_coefficient_per_k: f64,
    ground_temperature_k: f64,
    hot_temperature_k: f64,
) -> f64 {
    expansion_coefficient_per_k * (hot_temperature_k - ground_temperature_k)
}

/// Linear expansion coefficient of the titanium alloys, averaged over the range
/// of interest [1/K]. 8.6e-6 is the Ti-6Al-4V value over 20-100 °C (ASM
/// Handbook, Vol. 2, *Properties and Selection: Nonferrous Alloys*, Ti-6Al-4V
/// data); it rises with temperature across 300-800 K, by roughly 10 % at the
/// top of the range.
pub const TITANIUM_EXPANSION_PER_K: f64 = 8.6e-6;

#[cfg(test)]
mod tests {
    use super::*;
    use ventus_aero::boundary_layer::PRANDTL_AIR;
    use ventus_units::float::rel_err;

    fn edge_at(altitude_m: f64, mach: f64) -> EdgeState {
        let a = ventus_atmos::at_geopotential(altitude_m).unwrap();
        EdgeState {
            temperature_k: a.temperature_k,
            pressure_pa: a.pressure_pa,
            velocity_m_s: mach * a.speed_of_sound_m_s,
            mach,
            gamma: 1.4,
        }
    }

    fn wall(altitude_m: f64, mach: f64, x: f64) -> RadiationBalance {
        radiation_equilibrium_wall(
            &edge_at(altitude_m, mach),
            x,
            0.85,
            0.0,
            PRANDTL_AIR,
            Regime::Turbulent,
        )
        .unwrap()
    }

    /// THE ANCHOR, with its residual stated rather than tuned away.
    ///
    /// SR-71 skin at cruise is reported at 250-300 C over most of the airframe,
    /// with the nose around 315 C. Source: the surface-temperature diagram of
    /// the SR-71A Flight Manual (declassified) — most of the airframe
    /// 480-570 °F (250-300 °C), nose/windscreen ~600 °F (315 °C); the NASA
    /// SR-71 test bed report (NTRS 20000064011) carries a 600 °F structural
    /// limit on the upper fuselage.
    ///
    /// This flat-plate balance at the SR-71 cruise condition predicts:
    ///
    /// ```text
    ///   x =  1 m   270.2 C
    ///   x =  5 m   253.6 C
    ///   x = 10 m   246.0 C
    ///   x = 20 m   238.2 C
    /// ```
    ///
    /// The model OVERLAPS the published band on its lower half and runs below it
    /// aft of about 8 m. That residual is not a tuning knob, it is the model
    /// telling the truth about its own scope: a flat plate has no nose, no
    /// chines, no engine nacelles and no leading edges, and those are precisely
    /// the regions that make up the upper end of the published band and the
    /// 315 C nose. What the flat plate covers, it gets to within about 15 K at
    /// the low end.
    ///
    /// The test asserts the computed envelope and the overlap, not equality.
    /// Asserting the published band directly would have required widening it
    /// until it passed, which is the failure mode this project exists to avoid.
    #[test]
    fn the_sr71_flat_panel_agrees_with_its_published_band_where_the_model_applies() {
        let published = 250.0..=300.0;

        for x in [1.0, 5.0, 10.0, 20.0] {
            let c = wall(24_000.0, 3.2, x).wall_temperature_k - 273.15;
            assert!(
                (230.0..=285.0).contains(&c),
                "x={x} m: {c:.1} C is outside the envelope this model produced when \
                 it was validated; something has changed"
            );
            // Where the flat plate is a fair representation - forward and mid
            // body - it must land INSIDE the published band, not merely near it.
            if x <= 5.0 {
                assert!(
                    published.contains(&c),
                    "x={x} m: {c:.1} C is outside the published 250-300 C band on the \
                     forward body, where a flat plate is a fair model"
                );
            }
        }

        // And the aft skin must run cooler, which is the physical reason the
        // model drops below the band rather than an unexplained disagreement.
        let fwd = wall(24_000.0, 3.2, 1.0).wall_temperature_k;
        let aft = wall(24_000.0, 3.2, 20.0).wall_temperature_k;
        assert!(fwd - aft > 25.0, "aft cooling is only {:.0} K", fwd - aft);
    }

    /// The result the whole re-baseline turned on.
    #[test]
    fn radiation_is_what_keeps_the_design_point_in_conventional_titanium() {
        let b = wall(26_000.0, 3.5, 10.0);
        let t_aw = b.film.adiabatic_wall_temperature_k;
        let t_wall = b.wall_temperature_k;

        // Without radiation the skin would sit at the recovery temperature,
        // which is above the Ti-6Al-4V sustained limit.
        let ti = CANDIDATES.iter().find(|m| m.name == "Ti-6Al-4V").unwrap();
        assert!(t_aw > ti.sustained_limit_k, "T_aw = {t_aw} K");
        assert!(lightest_survivor(t_aw).is_some_and(|m| m.name != "Ti-6Al-4V"));

        // With radiation it does not, and by a wide margin.
        assert!(t_wall < ti.sustained_limit_k, "T_wall = {t_wall} K");
        assert!(
            b.radiation_relief_k() > 140.0,
            "radiation is worth only {:.0} K",
            b.radiation_relief_k()
        );
        assert_eq!(lightest_survivor(t_wall).unwrap().name, "Ti-6Al-4V");

        // Aluminium is still dead by a wide margin, at either temperature.
        for m in CANDIDATES.iter().filter(|m| m.name.starts_with("Al ")) {
            assert!(
                m.sustained_limit_k < t_wall,
                "{} survived {t_wall} K",
                m.name
            );
        }
    }

    /// The balance must actually close at the reported solution.
    #[test]
    fn the_converged_balance_closes() {
        for (h, m, x) in [
            (26_000.0, 3.5, 10.0),
            (24_000.0, 3.2, 5.0),
            (26_000.0, 2.0, 1.0),
        ] {
            let b = wall(h, m, x);
            assert!(
                rel_err(b.convective_flux_w_m2, b.radiative_flux_w_m2) < 1e-9,
                "h={h} M={m} x={x}: {} in, {} out",
                b.convective_flux_w_m2,
                b.radiative_flux_w_m2
            );
            assert!(b.convective_flux_w_m2 > 0.0);
        }
    }

    /// The wall must sit strictly between the edge and recovery temperatures for
    /// any real emissivity, and reach the recovery temperature only when it
    /// cannot radiate at all.
    #[test]
    fn the_wall_is_bracketed_and_falls_with_emissivity() {
        let edge = edge_at(26_000.0, 3.5);
        let t_aw = adiabatic_wall_temperature_k(&edge, PRANDTL_AIR, Regime::Turbulent).unwrap();

        let adiabatic =
            radiation_equilibrium_wall(&edge, 10.0, 0.0, 0.0, PRANDTL_AIR, Regime::Turbulent)
                .unwrap();
        assert!(rel_err(adiabatic.wall_temperature_k, t_aw) < 1e-12);

        let mut previous = t_aw;
        for eps in [0.05, 0.2, 0.5, 0.85, 1.0] {
            let b =
                radiation_equilibrium_wall(&edge, 10.0, eps, 0.0, PRANDTL_AIR, Regime::Turbulent)
                    .unwrap();
            assert!(b.wall_temperature_k > edge.temperature_k);
            assert!(b.wall_temperature_k < t_aw);
            assert!(
                b.wall_temperature_k < previous,
                "eps={eps}: a blacker surface must run cooler"
            );
            previous = b.wall_temperature_k;
        }
    }

    /// A warmer sink can only make the wall hotter, and the sensitivity is weak
    /// because the sink enters as the fourth power against a much hotter wall.
    /// Weak is not zero, which is why it is an input rather than a constant.
    #[test]
    fn a_warmer_sink_raises_the_wall_but_only_slightly() {
        let edge = edge_at(26_000.0, 3.5);
        let cold =
            radiation_equilibrium_wall(&edge, 10.0, 0.85, 0.0, PRANDTL_AIR, Regime::Turbulent)
                .unwrap()
                .wall_temperature_k;
        let earth =
            radiation_equilibrium_wall(&edge, 10.0, 0.85, 288.0, PRANDTL_AIR, Regime::Turbulent)
                .unwrap()
                .wall_temperature_k;
        assert!(earth > cold);
        assert!(
            earth - cold < 15.0,
            "sink sensitivity is {} K",
            earth - cold
        );
    }

    /// A hotter wall runs further downstream, because the film coefficient
    /// decays and convection loses ground to radiation more slowly than it
    /// gains it. Confirms the coupling runs the right way round.
    #[test]
    fn the_wall_cools_downstream() {
        let mut previous = f64::INFINITY;
        for x in [0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 40.0] {
            let t = wall(26_000.0, 3.5, x).wall_temperature_k;
            assert!(t < previous, "x={x}: wall did not cool downstream");
            previous = t;
        }
    }

    #[test]
    fn material_selection_answers_rather_than_guesses() {
        // Nothing on the list survives a rocket nozzle throat.
        assert!(lightest_survivor(2000.0).is_none());
        assert_eq!(survivors(2000.0).count(), 0);
        // Everything survives room temperature, and the lightest is aluminium.
        assert_eq!(survivors(300.0).count(), CANDIDATES.len());
        assert_eq!(
            lightest_survivor(300.0).unwrap().name,
            "Al 2618 (RR58, Concorde)"
        );
        // At a leading-edge temperature only the superalloy is left.
        assert_eq!(lightest_survivor(900.0).unwrap().name, "Inconel 718");
    }

    #[test]
    fn invalid_inputs_are_refused() {
        let edge = edge_at(26_000.0, 3.5);
        for eps in [-0.1, 1.1, f64::NAN] {
            assert_eq!(
                radiation_equilibrium_wall(&edge, 10.0, eps, 0.0, PRANDTL_AIR, Regime::Turbulent),
                Err(ThermalError::InvalidEmissivity)
            );
        }
        assert_eq!(
            radiation_equilibrium_wall(&edge, 10.0, 0.85, -1.0, PRANDTL_AIR, Regime::Turbulent),
            Err(ThermalError::InvalidSinkTemperature)
        );
        assert!(matches!(
            radiation_equilibrium_wall(&edge, -1.0, 0.85, 0.0, PRANDTL_AIR, Regime::Turbulent),
            Err(ThermalError::BoundaryLayer(_))
        ));
    }

    /// HOW MUCH THE CORRELATION CHOICE IS WORTH, measured rather than asserted.
    ///
    /// `ventus-aero` documents that published turbulent skin-friction
    /// correlations disagree by 3.9 % at Re_x = 1e6, and claims that this is
    /// worth about 1.5 K on the wall temperature. That claim has to be backed by
    /// a number or it is just a comforting sentence.
    ///
    /// Since `cf` scales as `x^(-1/5)`, moving the station by a factor of
    /// `0.98^-5 = 1.1041` changes `cf` by exactly -2 % and nothing else. The
    /// resulting shift in wall temperature is the sensitivity, using only the
    /// public API and no injected fudge.
    #[test]
    fn the_skin_friction_correlation_is_worth_about_one_kelvin() {
        let x = 10.0;
        let x_2pc_lower_cf = x * libm::pow(0.98, -5.0);

        let a = wall(26_000.0, 3.5, x);
        let b = wall(26_000.0, 3.5, x_2pc_lower_cf);

        let cf_change = (b.film.skin_friction - a.film.skin_friction) / a.film.skin_friction;
        assert!(
            (cf_change + 0.02).abs() < 1e-3,
            "the station shift should change cf by -2 %, it changed it by {:.4}",
            cf_change
        );

        let shift = a.wall_temperature_k - b.wall_temperature_k;
        assert!(
            (0.5..3.0).contains(&shift),
            "a 2 % change in cf moved the wall by {shift:.2} K; the documented \
             claim in ventus-aero is about 1.5 K and needs revisiting"
        );

        // And the thing that matters: it is small against the margin to the
        // Ti-6Al-4V limit, so the material conclusion does not depend on which
        // published correlation is used.
        let ti = CANDIDATES.iter().find(|m| m.name == "Ti-6Al-4V").unwrap();
        let margin = ti.sustained_limit_k - a.wall_temperature_k;
        assert!(
            margin > 20.0 * shift,
            "margin {margin:.0} K is only {:.0}x the correlation sensitivity",
            margin / shift
        );
    }

    /// The quantitative form of the SR-71 leaking fuel on the ground.
    #[test]
    fn thermal_growth_is_millimetres_per_metre() {
        let t_wall = wall(26_000.0, 3.5, 10.0).wall_temperature_k;
        let growth = thermal_growth_per_metre(TITANIUM_EXPANSION_PER_K, 288.15, t_wall);
        assert!(
            (2.0e-3..3.0e-3).contains(&growth),
            "dL/L = {growth:e}, expected a few mm per metre"
        );
        // A 30 m airframe grows a couple of centimetres.
        assert!((0.06..0.09).contains(&(growth * 30.0)));
    }
}
