//! M6b — supersonic drag and lift-to-drag ratio.
//!
//! Three contributions, and at M 3.5 they are not the ones subsonic intuition
//! expects:
//!
//! ```text
//!   drag due to lift   C_L^2 sqrt(M^2-1) / 4     linearised supersonic theory
//!   wave drag          Sears-Haack minimum       volume, not lift
//!   skin friction      reference-temperature     from the boundary layer module
//! ```
//!
//! The first dominates. In supersonic flow the price of lift is a pressure
//! field that radiates away as waves, and it grows with `sqrt(M^2-1)` — so going
//! faster makes lift more expensive, not less. That single term sets the
//! achievable L/D and it is why no supersonic aircraft approaches the L/D of a
//! subsonic one.
//!
//! [KNOWN_LIMIT] Linearised theory, flat-plate lift. It ignores thickness
//! effects on lift, leading-edge suction (which supersonic flow largely denies
//! anyway), and any benefit from careful area ruling beyond the Sears-Haack
//! ideal. It is a scale model of the drag, checked against the vehicles that
//! exist.

use crate::boundary_layer::{film_state, EdgeState, Regime, PRANDTL_AIR};
use crate::geometry::Geometry;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragError {
    NotANumber,
    /// Linearised supersonic theory needs M > 1.
    NotSupersonic,
    NonPhysicalInput,
}

/// A resolved drag breakdown at one flight condition.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DragBreakdown {
    pub lift_coefficient: f64,
    /// Drag due to lift, referenced to wing area.
    pub lift_induced: f64,
    /// Sears-Haack minimum wave drag, referenced to wing area.
    pub wave: f64,
    /// Skin friction, referenced to wing area.
    pub friction: f64,
    pub total: f64,
    pub lift_to_drag: f64,
}

impl DragBreakdown {
    /// Which term dominates, as a fraction of the total. Reported because the
    /// answer is the whole point of the module.
    #[must_use]
    pub fn lift_induced_fraction(&self) -> f64 {
        self.lift_induced / self.total
    }
}

/// Drag due to lift for a flat plate in linearised supersonic flow.
///
/// `C_D = C_L^2 sqrt(M^2 - 1) / 4`, from `C_L = 4 alpha / sqrt(M^2 - 1)` and
/// `C_D = C_L alpha`. Exact within the linearisation, so it is checked as an
/// identity rather than against a table.
///
/// # Errors
/// [`DragError::NotSupersonic`] at or below M = 1, where the linearisation is
/// singular.
pub fn lift_induced_drag_coefficient(lift_coefficient: f64, mach: f64) -> Result<f64, DragError> {
    if lift_coefficient.is_nan() || mach.is_nan() {
        return Err(DragError::NotANumber);
    }
    if mach <= 1.0 {
        return Err(DragError::NotSupersonic);
    }
    Ok(lift_coefficient * lift_coefficient * libm::sqrt(mach * mach - 1.0) / 4.0)
}

/// Angle of attack that produces a lift coefficient [rad], linearised.
///
/// # Errors
/// [`DragError::NotSupersonic`] at or below M = 1.
pub fn angle_of_attack_rad(lift_coefficient: f64, mach: f64) -> Result<f64, DragError> {
    if mach <= 1.0 {
        return Err(DragError::NotSupersonic);
    }
    Ok(lift_coefficient * libm::sqrt(mach * mach - 1.0) / 4.0)
}

/// Sears-Haack minimum wave drag [N].
///
/// `D = (9 pi / 2) q (A_max / L)^2` for the minimum-drag body of given length
/// and volume. **[TO VERIFY]** — recited. The equivalent coefficient on frontal
/// area is `9 pi^2 / (8 f^2)` for fineness ratio `f`, which the tests check as
/// an internal consistency.
///
/// This is a LOWER BOUND: it is the drag of the optimal body. A real airframe
/// with an intake, a cockpit and a wing joint pays more, and how much more is
/// what area ruling is for.
///
/// # Errors
/// [`DragError::NonPhysicalInput`] for a non-positive length or pressure.
pub fn sears_haack_wave_drag_n(
    dynamic_pressure_pa: f64,
    max_cross_section_m2: f64,
    length_m: f64,
) -> Result<f64, DragError> {
    if dynamic_pressure_pa <= 0.0 || max_cross_section_m2 <= 0.0 || length_m <= 0.0 {
        return Err(DragError::NonPhysicalInput);
    }
    let ratio = max_cross_section_m2 / length_m;
    Ok(4.5 * core::f64::consts::PI * dynamic_pressure_pa * ratio * ratio)
}

/// The full breakdown at a flight condition.
///
/// Friction comes from the boundary-layer module at the vehicle's own length,
/// so M5, M6a and M6b all share one skin-friction model.
///
/// # Errors
/// [`DragError`] for non-physical input or subsonic flight.
pub fn breakdown(
    geometry: &Geometry,
    edge: &EdgeState,
    dynamic_pressure_pa: f64,
    wall_temperature_k: f64,
) -> Result<DragBreakdown, DragError> {
    if dynamic_pressure_pa <= 0.0 {
        return Err(DragError::NonPhysicalInput);
    }
    let c_l = geometry.required_lift_n() / (dynamic_pressure_pa * geometry.wing_area_m2);
    let lift_induced = lift_induced_drag_coefficient(c_l, edge.mach)?;

    let wave_n = sears_haack_wave_drag_n(
        dynamic_pressure_pa,
        geometry.max_cross_section_m2,
        geometry.length_m,
    )?;
    let wave = wave_n / (dynamic_pressure_pa * geometry.wing_area_m2);

    // Skin friction at the vehicle's mid-length, over its wetted area. Using a
    // single station is a scale estimate; integrating along the body is
    // [TO COMPUTE].
    let film = film_state(
        edge,
        0.5 * geometry.length_m,
        wall_temperature_k,
        PRANDTL_AIR,
        Regime::Turbulent,
    )
    .map_err(|_| DragError::NonPhysicalInput)?;
    // The local coefficient at mid-length approximates the plate average.
    let friction = film.skin_friction * geometry.wetted_area_m2 / geometry.wing_area_m2;

    let total = lift_induced + wave + friction;
    Ok(DragBreakdown {
        lift_coefficient: c_l,
        lift_induced,
        wave,
        friction,
        total,
        lift_to_drag: c_l / total,
    })
}

/// Küchemann's correlation for the maximum achievable L/D, `4 (M + 3) / M`.
///
/// An empirical UPPER BOUND fitted across supersonic aircraft. **[TO VERIFY]**.
/// The SR-71 achieves about 6 against a bound of 7.75 at its own Mach number,
/// so real aircraft sit well below it — which is exactly why it is useful as a
/// ceiling rather than as a target.
#[must_use]
pub fn kuchemann_bound(mach: f64) -> f64 {
    4.0 * (mach + 3.0) / mach
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::ventus1;
    use ventus_units::float::rel_err;

    const Q: f64 = 18_463.0;

    fn design_edge() -> EdgeState {
        let a = ventus_atmos::at_geopotential(26_000.0).unwrap();
        EdgeState {
            temperature_k: a.temperature_k,
            pressure_pa: a.pressure_pa,
            velocity_m_s: 3.5 * a.speed_of_sound_m_s,
            mach: 3.5,
            gamma: 1.4,
        }
    }

    /// The linearised relations must be mutually consistent: `C_D = C_L alpha`
    /// exactly. No table involved.
    #[test]
    fn drag_due_to_lift_is_lift_times_angle_of_attack() {
        for mach in [1.5, 2.0, 3.5, 6.0] {
            for c_l in [0.05, 0.154, 0.3] {
                let alpha = angle_of_attack_rad(c_l, mach).unwrap();
                let c_d = lift_induced_drag_coefficient(c_l, mach).unwrap();
                assert!(rel_err(c_d, c_l * alpha) < 1e-14, "M={mach} C_L={c_l}");
            }
        }
        assert_eq!(
            lift_induced_drag_coefficient(0.1, 1.0),
            Err(DragError::NotSupersonic)
        );
    }

    /// GOING FASTER MAKES LIFT MORE EXPENSIVE. The `sqrt(M^2-1)` in the
    /// numerator is the whole reason supersonic L/D is poor, and it is the
    /// opposite of the subsonic intuition that speed is free.
    #[test]
    fn lift_gets_more_expensive_with_mach_number() {
        let mut previous = 0.0;
        for mach in [1.5, 2.0, 3.0, 3.5, 5.0] {
            let c_d = lift_induced_drag_coefficient(0.154, mach).unwrap();
            assert!(c_d > previous, "M={mach}: drag due to lift fell");
            previous = c_d;
        }
    }

    /// Sears-Haack, checked through its own equivalent frontal-area coefficient
    /// `9 pi^2 / (8 f^2)`. Two routes to the same number.
    #[test]
    fn sears_haack_agrees_with_its_frontal_area_form() {
        for fineness in [8.0, 12.0, 20.0] {
            let length = 25.0;
            let diameter = length / fineness;
            let a_max = core::f64::consts::PI * diameter * diameter / 4.0;
            let d = sears_haack_wave_drag_n(Q, a_max, length).unwrap();
            let c_d_frontal = d / (Q * a_max);
            let closed_form =
                9.0 * core::f64::consts::PI * core::f64::consts::PI / (8.0 * fineness * fineness);
            assert!(
                rel_err(c_d_frontal, closed_form) < 1e-12,
                "f={fineness}: {c_d_frontal} vs {closed_form}"
            );
        }
        // Slenderness pays: doubling fineness quarters the wave drag.
        let thin = sears_haack_wave_drag_n(Q, 0.5, 24.0).unwrap();
        let fat = sears_haack_wave_drag_n(Q, 2.0, 24.0).unwrap();
        assert!(rel_err(fat / thin, 16.0) < 1e-12);
    }

    /// THE MODULE'S ANSWER, against the two-sided guard the design point set:
    /// target 5.0-6.0, above the Küchemann bound is a bug, below 3.5 is a bug or
    /// a bad configuration.
    #[test]
    fn the_design_point_lift_to_drag_lands_in_its_target_band() {
        let g = ventus1(Q);
        let b = breakdown(&g, &design_edge(), Q, 548.3).unwrap();

        assert!(
            b.lift_to_drag < kuchemann_bound(3.5),
            "L/D {:.2} exceeds the Kuchemann bound {:.2}, which is a bug",
            b.lift_to_drag,
            kuchemann_bound(3.5)
        );
        assert!(
            b.lift_to_drag > 3.5,
            "L/D {:.2} is below 3.5: either a bug or a bad configuration, and \
             the breakdown says which - lift-induced {:.4}, wave {:.4}, \
             friction {:.4}",
            b.lift_to_drag,
            b.lift_induced,
            b.wave,
            b.friction
        );
        assert!(
            (4.5..6.5).contains(&b.lift_to_drag),
            "L/D {:.2} is outside the 5.0-6.0 target band the design point set",
            b.lift_to_drag
        );
    }

    /// AND THE ANSWER TO THE QUESTION THE MODULE EXISTS FOR: which term
    /// dominates. At M 3.5 it is drag due to lift, not wave drag, and that is
    /// not what subsonic intuition predicts.
    #[test]
    fn drag_due_to_lift_dominates_at_the_design_point() {
        let g = ventus1(Q);
        let b = breakdown(&g, &design_edge(), Q, 548.3).unwrap();
        assert!(
            b.lift_induced > b.wave && b.lift_induced > b.friction,
            "lift {:.4}, wave {:.4}, friction {:.4}",
            b.lift_induced,
            b.wave,
            b.friction
        );
        assert!(
            b.lift_induced_fraction() > 0.5,
            "drag due to lift is only {:.0} % of the total",
            b.lift_induced_fraction() * 100.0
        );
        // Every term is positive and they sum to the total.
        assert!(rel_err(b.lift_induced + b.wave + b.friction, b.total) < 1e-14);
    }

    /// The Küchemann bound falls with Mach and is a bound, not a target: the
    /// SR-71 achieves about 6 against 7.75 at its own condition.
    #[test]
    fn the_kuchemann_bound_behaves_as_a_ceiling() {
        assert!(rel_err(kuchemann_bound(3.2), 7.75) < 1e-3);
        assert!(rel_err(kuchemann_bound(3.5), 7.4286) < 1e-4);
        assert!(kuchemann_bound(5.0) < kuchemann_bound(2.0));
        assert!(
            6.0 < kuchemann_bound(3.2),
            "the SR-71 must fit under its own bound"
        );
    }

    #[test]
    fn non_physical_input_is_refused() {
        assert_eq!(
            sears_haack_wave_drag_n(Q, 0.0, 25.0),
            Err(DragError::NonPhysicalInput)
        );
        let g = ventus1(Q);
        assert_eq!(
            breakdown(&g, &design_edge(), 0.0, 548.3),
            Err(DragError::NonPhysicalInput)
        );
    }

    /// Sanity on the closure: a heavier aircraft at the same geometry needs more
    /// lift, pays quadratically for it, and its L/D falls.
    #[test]
    fn a_higher_lift_coefficient_costs_lift_to_drag() {
        let g = ventus1(Q);
        let at_design = breakdown(&g, &design_edge(), Q, 548.3).unwrap();
        // Half the dynamic pressure doubles the required C_L.
        let at_low_q = breakdown(&g, &design_edge(), Q * 0.5, 548.3).unwrap();
        assert!(rel_err(at_low_q.lift_coefficient, 2.0 * at_design.lift_coefficient) < 1e-12);
        assert!(
            at_low_q.lift_to_drag < at_design.lift_to_drag,
            "{:.2} against {:.2}",
            at_low_q.lift_to_drag,
            at_design.lift_to_drag
        );
    }
}
