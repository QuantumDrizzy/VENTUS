//! VENTUS-1 geometry, derived rather than assumed.
//!
//! `docs/design-point.md` §5.2 carried "no geometry" as a declared gap from the
//! first ADR, with the note that it becomes blocking the day M6 starts. This is
//! that day, and this module closes it.
//!
//! # The derivation, and its one free choice
//!
//! Almost everything follows from the design point:
//!
//! ```text
//!   q                18.463 kPa      from M1 at the cruise condition
//!   C_L              0.154           back-calculated from the SR-71 at ITS
//!                                    own cruise condition, design-point §5.2
//!   W/S = q C_L      2.84 kPa        = 290 kg/m^2
//! ```
//!
//! That fixes a RATIO. One absolute scale has to be chosen, and it is the only
//! number in this module that is not derived:
//!
//! **Declared choice: cruise mass 28 000 kg.** A demonstrator at roughly half
//! the SR-71's mid-cruise mass. Everything below follows from it.
//!
//! Aspect ratio 1.7 is taken from the SR-71 **[TO CITE]**, because a slender
//! supersonic delta has very little freedom there — span costs wave drag and
//! buys induced-drag relief that supersonic flow largely refuses to give.
//!
//! [KNOWN_LIMIT] Fuselage volume is estimated from a fineness ratio rather than
//! laid out, so the Sears-Haack wave drag that depends on it is a scale
//! estimate, not a shape result. A real area-ruled distribution would come from
//! a layout this project does not have.

use ventus_units::float::abs;

/// The one declared choice. Everything else in this module is derived from it
/// and from the design point.
pub const CRUISE_MASS_KG: f64 = 28_000.0;

/// Cruise lift coefficient, from the SR-71 back-calculation in
/// `docs/design-point.md` §5.2. **[TO CITE]** on the SR-71 mass and wing area.
pub const CRUISE_LIFT_COEFFICIENT: f64 = 0.154;

/// Wing aspect ratio, SR-71 class. **[TO CITE]**
pub const ASPECT_RATIO: f64 = 1.7;

/// Fuselage fineness ratio, length over maximum diameter. Slender bodies at
/// M 3.5 sit near 12; the SR-71 is comparable. **[TO CITE]**
pub const FINENESS_RATIO: f64 = 12.0;

/// Ratio of wetted area to reference wing area. A blended delta runs near 3.
/// **[TO CITE]** — it enters the friction drag linearly, so it matters.
pub const WETTED_AREA_RATIO: f64 = 3.0;

/// The derived vehicle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Geometry {
    pub cruise_mass_kg: f64,
    /// Reference wing area [m^2].
    pub wing_area_m2: f64,
    pub span_m: f64,
    pub length_m: f64,
    /// Maximum fuselage cross-sectional area [m^2], for Sears-Haack.
    pub max_cross_section_m2: f64,
    /// Total wetted area [m^2].
    pub wetted_area_m2: f64,
    /// Equivalent Sears-Haack body volume [m^3].
    pub volume_m3: f64,
    pub wing_loading_pa: f64,
}

/// Derive the vehicle from the cruise dynamic pressure.
///
/// # Panics
/// Never; every input is a compile-time constant and the arithmetic is total.
#[must_use]
pub fn ventus1(dynamic_pressure_pa: f64) -> Geometry {
    let wing_loading = dynamic_pressure_pa * CRUISE_LIFT_COEFFICIENT;
    let weight_n = CRUISE_MASS_KG * ventus_units::constants::G0_M_S2;
    let wing_area = weight_n / wing_loading;
    let span = libm::sqrt(ASPECT_RATIO * wing_area);

    // Length from the SR-71 scaled by the square root of the area ratio: a
    // linear dimension scales as the square root of an area. SR-71: 32.7 m at
    // 167.2 m^2. [TO CITE]
    let length = 32.7 * libm::sqrt(wing_area / 167.2);

    let max_diameter = length / FINENESS_RATIO;
    let max_cross_section = core::f64::consts::PI * max_diameter * max_diameter / 4.0;
    // Sears-Haack: V = (3 pi / 16) A_max L.
    let volume = 3.0 * core::f64::consts::PI / 16.0 * max_cross_section * length;

    Geometry {
        cruise_mass_kg: CRUISE_MASS_KG,
        wing_area_m2: wing_area,
        span_m: span,
        length_m: length,
        max_cross_section_m2: max_cross_section,
        wetted_area_m2: WETTED_AREA_RATIO * wing_area,
        volume_m3: volume,
        wing_loading_pa: wing_loading,
    }
}

impl Geometry {
    /// Lift the vehicle must produce in level flight [N].
    #[must_use]
    pub fn required_lift_n(&self) -> f64 {
        self.cruise_mass_kg * ventus_units::constants::G0_M_S2
    }

    /// Check that the geometry closes: `L = q S C_L` must equal the weight.
    /// The identity the whole derivation rests on.
    #[must_use]
    pub fn closure_residual(&self, dynamic_pressure_pa: f64) -> f64 {
        let lift = dynamic_pressure_pa * self.wing_area_m2 * CRUISE_LIFT_COEFFICIENT;
        abs(lift - self.required_lift_n()) / self.required_lift_n()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ventus_units::float::rel_err;

    const Q: f64 = 18_463.0;

    /// THE IDENTITY THE DERIVATION RESTS ON. If lift does not equal weight at
    /// the design point, the vehicle does not fly and every number downstream is
    /// about a different aircraft.
    #[test]
    fn the_geometry_closes_on_level_flight() {
        let g = ventus1(Q);
        assert!(
            g.closure_residual(Q) < 1e-12,
            "lift and weight disagree by {:e}",
            g.closure_residual(Q)
        );
    }

    /// The wing loading has to be the figure `docs/design-point.md` §5.2
    /// computed independently from q and C_L, or the document and the code have
    /// drifted apart.
    #[test]
    fn the_wing_loading_matches_the_design_point() {
        let g = ventus1(Q);
        assert!(
            rel_err(g.wing_loading_pa, 2843.3) < 1e-3,
            "{}",
            g.wing_loading_pa
        );
        let kg_per_m2 = g.wing_loading_pa / ventus_units::constants::G0_M_S2;
        assert!(
            (280.0..300.0).contains(&kg_per_m2),
            "{kg_per_m2:.0} kg/m2 against the 282 the design point quotes at C_L 0.15"
        );
    }

    /// The vehicle must come out the size of a real aircraft. A derivation that
    /// produced a 3 m or a 300 m airframe would be arithmetically fine and
    /// physically absurd.
    #[test]
    fn the_derived_vehicle_is_a_plausible_aircraft() {
        let g = ventus1(Q);
        assert!(
            (80.0..120.0).contains(&g.wing_area_m2),
            "S = {:.1} m2",
            g.wing_area_m2
        );
        assert!((10.0..16.0).contains(&g.span_m), "b = {:.1} m", g.span_m);
        assert!(
            (20.0..30.0).contains(&g.length_m),
            "L = {:.1} m",
            g.length_m
        );
        assert!(
            (1.5..3.0).contains(&(g.length_m / FINENESS_RATIO)),
            "fuselage diameter {:.2} m",
            g.length_m / FINENESS_RATIO
        );
        // Smaller than the SR-71 in every dimension, as a half-mass demonstrator
        // should be.
        assert!(g.wing_area_m2 < 167.2);
        assert!(g.length_m < 32.7);
    }

    /// Sears-Haack volume must follow its own definition, and the vehicle has to
    /// have room inside it for the fuel M7 will need.
    #[test]
    fn the_volume_follows_sears_haack_and_is_large_enough_to_be_useful() {
        let g = ventus1(Q);
        let from_definition =
            3.0 * core::f64::consts::PI / 16.0 * g.max_cross_section_m2 * g.length_m;
        assert!(rel_err(g.volume_m3, from_definition) < 1e-15);
        // JP-7 is about 800 kg/m3, so this has to hold a serious fuel fraction.
        assert!(
            g.volume_m3 * 800.0 > 0.4 * g.cruise_mass_kg,
            "only {:.0} m3, holding {:.0} kg of fuel",
            g.volume_m3,
            g.volume_m3 * 800.0
        );
    }

    /// Scaling: a heavier aircraft at the same q and C_L needs proportionally
    /// more wing, and its linear dimensions grow as the square root.
    #[test]
    fn the_derivation_scales_the_way_areas_and_lengths_do() {
        let a = ventus1(Q);
        let b = ventus1(Q / 4.0);
        // Quarter the dynamic pressure, four times the wing.
        assert!(rel_err(b.wing_area_m2, 4.0 * a.wing_area_m2) < 1e-12);
        // And twice the linear dimensions.
        assert!(rel_err(b.span_m, 2.0 * a.span_m) < 1e-12);
        assert!(rel_err(b.length_m, 2.0 * a.length_m) < 1e-12);
    }
}
