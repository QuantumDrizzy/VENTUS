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
//! the SR-71's start-of-cruise mass. Everything below follows from it, and since
//! M7 it also fixes the empty mass, so this is the scale of the whole aircraft.
//!
//! [CORRECTED] This said *mid-cruise* mass. **No number changes** - the 55 t it
//! refers to was always used as the start-of-cruise value, which is the
//! like-for-like comparison because `CRUISE_MASS_KG` is itself a start-of-cruise
//! mass. Only the label was wrong, and it was wrong in three places at once.
//! Corrected here because M7 now divides by that quantity to derive an empty
//! mass, where the two readings differ by 29 %. See
//! `ventus_mass::SR71_EMPTY_FRACTION_OF_CRUISE_MASS`.
//!
//! Aspect ratio 1.7 is taken from the SR-71, because a slender
//! supersonic delta has very little freedom there — span costs wave drag and
//! buys induced-drag relief that supersonic flow largely refuses to give.
//! Source: SR-71 span 55.6 ft (16.94 m) over wing area 1 800 ft² (167.2 m²)
//! gives AR = 1.72 (NASA SR-71 fact sheet; SR-71A Flight Manual); 1.7 is
//! carried rounded.
//!
//! [KNOWN_LIMIT] Fuselage volume is estimated from a fineness ratio rather than
//! laid out, so the Sears-Haack wave drag that depends on it is a scale
//! estimate, not a shape result. A real area-ruled distribution would come from
//! a layout this project does not have.
//!
//! Two named specs share the cruise mass and the wing, and differ only in
//! fineness: [`GeometrySpec::SNAPSHOT`] is the M 3.50 yardstick;
//! [`GeometrySpec::M4_CANDIDATE`] is a fatter station that can host the M 4
//! capture. Neither is a closed airframe. The snapshot constructor [`ventus1`]
//! must keep pointing at the first.

use ventus_units::float::abs;

/// The one declared choice. Everything else in this module is derived from it
/// and from the design point.
pub const CRUISE_MASS_KG: f64 = 28_000.0;

/// Cruise lift coefficient, from the SR-71 back-calculation in
/// `docs/design-point.md` §5.2. The cited inputs it needs — SR-71 wing area
/// 1 800 ft² (167.2 m²) and cruise mass — are now cited on
/// [`ASPECT_RATIO`] and in `ventus_mass`, which closes this derivation.
///
/// That back-calculation uses the SR-71 at its START-of-cruise mass, matched
/// against [`CRUISE_MASS_KG`] which is also a start-of-cruise mass. Taking the
/// SR-71 at 77 t instead, which the discarded mid-cruise reading implied, would
/// give C_L = 0.215 rather than 0.154.
pub const CRUISE_LIFT_COEFFICIENT: f64 = 0.154;

/// Wing aspect ratio, SR-71 class. Derived from the cited SR-71 geometry:
/// span 55.6 ft = 16.94 m, wing area 1 800 ft² = 167.2 m², so AR = b²/S = 1.72;
/// 1.7 carried rounded (NASA SR-71 fact sheet; SR-71A Flight Manual).
pub const ASPECT_RATIO: f64 = 1.7;

/// Fuselage fineness ratio, length over maximum diameter. Slender bodies at
/// M 3.5 sit near 12. The SR-71 length is cited � 107 ft 5 in = 32.74 m (NASA
/// fact sheet) � but its maximum body diameter is not read from a primary
/// source yet. **[TO VERIFY]**
///
/// This is the **snapshot** body [`ventus1`] uses. The M 4 host-body candidate
/// is a different slenderness, [`M4_CANDIDATE_FINENESS_RATIO`], and does not
/// replace this number.
pub const FINENESS_RATIO: f64 = 12.0;

/// Fineness of the M 4 host-body candidate: same length as [`ventus1`], fatter
/// fuselage.
///
/// **[TO DETERMINE]** — not a cited airframe. Fineness 12 is the snapshot.
/// This is the round slender-body value below 12 that puts Sears-Haack `A_max`
/// on the snapshot length above the M 4 self-consistent capture M12 already
/// computes, so `A_c / A_body` can fall below 1 at the proposed row. It is a
/// volume-distribution change, not a heavier aircraft of the same shape
/// (scale is invariant on this ratio). It does not close blowout, spillage
/// force, unstart, or the thermal nose.
pub const M4_CANDIDATE_FINENESS_RATIO: f64 = 10.0;

/// Ratio of wetted area to reference wing area. A blended delta runs near 3.
/// **[TO CITE]** — it enters the friction drag linearly, so it matters.
pub const WETTED_AREA_RATIO: f64 = 3.0;

/// The snapshot body is more slender than the M 4 candidate. If these ever
/// invert, the candidate is no longer the fatter host the capture-area work
/// named, and the M 4 ratio assertion should be deleted in the same change.
const _: () = assert!(
    M4_CANDIDATE_FINENESS_RATIO > 0.0 && M4_CANDIDATE_FINENESS_RATIO < FINENESS_RATIO,
    "the M 4 candidate is no longer fatter than the snapshot body"
);

/// Named geometry inputs. Wing area and length still follow from cruise mass
/// and the design point; fineness then sets the Sears-Haack station.
///
/// [`GeometrySpec::SNAPSHOT`] is the M 3.50 yardstick. [`GeometrySpec::M4_CANDIDATE`]
/// is a proposed host body for the M 4.00 row, not a replacement of that
/// yardstick and not a closed aircraft.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GeometrySpec {
    pub cruise_mass_kg: f64,
    pub cruise_lift_coefficient: f64,
    pub aspect_ratio: f64,
    pub fineness_ratio: f64,
    pub wetted_area_ratio: f64,
}

impl GeometrySpec {
    /// M6b snapshot: 28 t, C_L 0.154, AR 1.7, fineness 12.
    pub const SNAPSHOT: Self = Self {
        cruise_mass_kg: CRUISE_MASS_KG,
        cruise_lift_coefficient: CRUISE_LIFT_COEFFICIENT,
        aspect_ratio: ASPECT_RATIO,
        fineness_ratio: FINENESS_RATIO,
        wetted_area_ratio: WETTED_AREA_RATIO,
    };

    /// M 4 host-body candidate: same mass and wing, fineness 10.
    pub const M4_CANDIDATE: Self = Self {
        cruise_mass_kg: CRUISE_MASS_KG,
        cruise_lift_coefficient: CRUISE_LIFT_COEFFICIENT,
        aspect_ratio: ASPECT_RATIO,
        fineness_ratio: M4_CANDIDATE_FINENESS_RATIO,
        wetted_area_ratio: WETTED_AREA_RATIO,
    };
}

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

/// Derive a vehicle from a [`GeometrySpec`] and the cruise dynamic pressure.
///
/// Length is still the SR-71 scaled by the square root of the wing-area ratio
/// (a linear dimension scales as the square root of an area). Fineness then
/// sets the maximum diameter. Two specs that share mass, C_L and aspect ratio
/// therefore share wing, span and length, and differ only in the Sears-Haack
/// station — which is the capture-hosting lever, and the only one this
/// module is allowed to pull without inventing a new aircraft family.
///
/// # Panics
/// Never; the arithmetic is total. Non-physical specs produce non-physical
/// geometry rather than a hidden default.
#[must_use]
pub fn derive(spec: GeometrySpec, dynamic_pressure_pa: f64) -> Geometry {
    let wing_loading = dynamic_pressure_pa * spec.cruise_lift_coefficient;
    let weight_n = spec.cruise_mass_kg * ventus_units::constants::G0_M_S2;
    let wing_area = weight_n / wing_loading;
    let span = libm::sqrt(spec.aspect_ratio * wing_area);

    // Length from the SR-71 scaled by the square root of the area ratio: a
    // linear dimension scales as the square root of an area. SR-71: 32.74 m
    // (107 ft 5 in) at 167.2 m² — NASA SR-71 fact sheet; SR-71A Flight Manual.
    let length = 32.7 * libm::sqrt(wing_area / 167.2);

    let max_diameter = length / spec.fineness_ratio;
    let max_cross_section = core::f64::consts::PI * max_diameter * max_diameter / 4.0;
    // Sears-Haack: V = (3 pi / 16) A_max L.
    let volume = 3.0 * core::f64::consts::PI / 16.0 * max_cross_section * length;

    Geometry {
        cruise_mass_kg: spec.cruise_mass_kg,
        wing_area_m2: wing_area,
        span_m: span,
        length_m: length,
        max_cross_section_m2: max_cross_section,
        wetted_area_m2: spec.wetted_area_ratio * wing_area,
        volume_m3: volume,
        wing_loading_pa: wing_loading,
    }
}

/// Snapshot geometry: M 3.50 yardstick. [`GeometrySpec::SNAPSHOT`].
///
/// # Panics
/// Never; every input is a compile-time constant and the arithmetic is total.
#[must_use]
pub fn ventus1(dynamic_pressure_pa: f64) -> Geometry {
    derive(GeometrySpec::SNAPSHOT, dynamic_pressure_pa)
}

/// M 4 host-body candidate. Same cruise mass, wing and length as [`ventus1`];
/// fatter Sears-Haack station ([`M4_CANDIDATE_FINENESS_RATIO`]).
///
/// This is **not** a re-baseline of VENTUS-1. M12 still reports capture/body
/// on [`ventus1`]. The candidate exists so that ratio can be asked of a
/// different volume distribution without silently replacing the snapshot.
#[must_use]
pub fn ventus1_m4_candidate(dynamic_pressure_pa: f64) -> Geometry {
    derive(GeometrySpec::M4_CANDIDATE, dynamic_pressure_pa)
}

impl Geometry {
    /// Lift the vehicle must produce in level flight [N].
    #[must_use]
    pub fn required_lift_n(&self) -> f64 {
        self.cruise_mass_kg * ventus_units::constants::G0_M_S2
    }

    /// Length over maximum diameter implied by the Sears-Haack station.
    #[must_use]
    pub fn fineness_ratio(&self) -> f64 {
        let diameter = libm::sqrt(4.0 * self.max_cross_section_m2 / core::f64::consts::PI);
        self.length_m / diameter
    }

    /// Check that the geometry closes: `L = q S C_L` must equal the weight.
    /// The identity the whole derivation rests on.
    ///
    /// Uses the snapshot C_L. Both named specs share that value; a future spec
    /// with a different C_L would need its own residual.
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

    /// THE M 4 CANDIDATE IS A DIFFERENT STATION, NOT A DIFFERENT AIRCRAFT FAMILY.
    ///
    /// Same mass, wing, span and length as the snapshot. Only the Sears-Haack
    /// cross-section grows, as 1/f², because that is the lever that can host
    /// an inlet the snapshot body cannot. A uniformly larger aeroplane of the
    /// same shape would leave capture/body unchanged.
    #[test]
    fn the_m4_candidate_shares_the_wing_and_fattens_the_station() {
        let snapshot = ventus1(Q);
        let candidate = ventus1_m4_candidate(Q);

        assert!(rel_err(candidate.cruise_mass_kg, snapshot.cruise_mass_kg) < 1e-15);
        assert!(rel_err(candidate.wing_area_m2, snapshot.wing_area_m2) < 1e-15);
        assert!(rel_err(candidate.span_m, snapshot.span_m) < 1e-15);
        assert!(rel_err(candidate.length_m, snapshot.length_m) < 1e-15);
        assert!(candidate.closure_residual(Q) < 1e-12);

        let expected_area = snapshot.max_cross_section_m2
            * (FINENESS_RATIO / M4_CANDIDATE_FINENESS_RATIO)
            * (FINENESS_RATIO / M4_CANDIDATE_FINENESS_RATIO);
        assert!(
            rel_err(candidate.max_cross_section_m2, expected_area) < 1e-12,
            "A_max = {:.4} m2 against 1/f^2 scaling {:.4}",
            candidate.max_cross_section_m2,
            expected_area
        );
        assert!(candidate.max_cross_section_m2 > snapshot.max_cross_section_m2);
        assert!(rel_err(candidate.fineness_ratio(), M4_CANDIDATE_FINENESS_RATIO) < 1e-12);
        assert!(rel_err(snapshot.fineness_ratio(), FINENESS_RATIO) < 1e-12);

        // The snapshot constructor is still the snapshot: a later edit that
        // quietly pointed ventus1 at the candidate would make every M 3.50
        // capture ratio a lie.
        assert!(rel_err(snapshot.fineness_ratio(), M4_CANDIDATE_FINENESS_RATIO) > 0.05);
    }
}
