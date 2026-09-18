//! M7 — Mass breakdown and range.
//!
//! Unblocked by M6's geometry derivation. The Breguet range equation ties every
//! other module together:
//!
//! ```text
//!   R = V · (L/D) · Isp · ln(W_initial / W_final)
//!        |     |       |          |
//!        |     |       |          +--  M7, the fuel fraction
//!        |     |       +-------------  M4, the engine
//!        |     +---------------------  M6, the airframe
//!        +---------------------------  M1 and the design point
//! ```
//!
//! Every term comes from a different module, and the product is checked against
//! one number: the SR-71's unrefuelled range. That makes it the closest thing
//! this project has to an end-to-end test.
//!
//! [KNOWN_LIMIT] Cruise only. Breguet assumes steady level flight at constant
//! L/D and Isp, so it says nothing about climb, acceleration or reserves. A real
//! mission spends a serious fraction of its fuel getting to the design point —
//! `docs/design-point.md` §5.1 declares that corridor as an open gap, and this
//! module inherits it.

#![no_std]
#![forbid(unsafe_code)]

#[cfg(test)]
extern crate std;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MassError {
    NotANumber,
    /// Masses must be positive and the final mass below the initial one.
    NonPhysicalMass,
    /// A performance term was not positive.
    NonPhysicalPerformance,
    /// The masses do not add up: the aircraft is already heavier at the end of
    /// cruise than the empty airframe plus what it is carrying. Returned rather
    /// than handing back a negative payload, which would look like a number.
    MissionDoesNotClose,
}

/// A mass breakdown at one point in the mission.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MassBreakdown {
    pub empty_mass_kg: f64,
    pub fuel_mass_kg: f64,
    pub payload_mass_kg: f64,
}

impl MassBreakdown {
    #[must_use]
    pub fn takeoff_mass_kg(&self) -> f64 {
        self.empty_mass_kg + self.fuel_mass_kg + self.payload_mass_kg
    }

    /// Mass with the fuel burnt off.
    #[must_use]
    pub fn final_mass_kg(&self) -> f64 {
        self.empty_mass_kg + self.payload_mass_kg
    }

    /// Fuel as a fraction of takeoff mass. **The single most important number in
    /// aircraft design.** Range goes as `ln(1/(1-f))`, which ACCELERATES as the
    /// tanks fill — the familiar "diminishing returns" statement is true of the
    /// mass ratio, not of the fuel fraction, and the two point opposite ways.
    /// See `range_diminishes_in_mass_ratio_but_accelerates_in_fuel_fraction`.
    #[must_use]
    pub fn fuel_fraction(&self) -> f64 {
        self.fuel_mass_kg / self.takeoff_mass_kg()
    }

    /// Empty mass as a fraction of takeoff mass.
    #[must_use]
    pub fn empty_fraction(&self) -> f64 {
        self.empty_mass_kg / self.takeoff_mass_kg()
    }
}

/// Breguet cruise range [m].
///
/// `R = V (L/D) Isp ln(Wi/Wf)`, with `Isp` in seconds. The `g0` that converts
/// specific impulse cancels against the one in the weight, which is why seconds
/// and metres per second multiply straight to metres.
///
/// # Errors
/// [`MassError`] for non-physical masses or performance.
pub fn breguet_range_m(
    velocity_m_s: f64,
    lift_to_drag: f64,
    specific_impulse_s: f64,
    initial_mass_kg: f64,
    final_mass_kg: f64,
) -> Result<f64, MassError> {
    for v in [velocity_m_s, lift_to_drag, specific_impulse_s] {
        if v.is_nan() {
            return Err(MassError::NotANumber);
        }
        if v <= 0.0 {
            return Err(MassError::NonPhysicalPerformance);
        }
    }
    if initial_mass_kg.is_nan() || final_mass_kg.is_nan() {
        return Err(MassError::NotANumber);
    }
    if final_mass_kg <= 0.0 || initial_mass_kg < final_mass_kg {
        return Err(MassError::NonPhysicalMass);
    }
    Ok(velocity_m_s
        * lift_to_drag
        * specific_impulse_s
        * libm::log(initial_mass_kg / final_mass_kg))
}

/// The fuel fraction a range requires — Breguet inverted.
///
/// The question a designer actually asks. Returns the fraction of takeoff mass
/// that must be fuel; a value at or above 1 means the range is impossible with
/// this airframe and engine no matter how the structure is built.
///
/// # Errors
/// [`MassError`] for non-physical inputs.
pub fn required_fuel_fraction(
    range_m: f64,
    velocity_m_s: f64,
    lift_to_drag: f64,
    specific_impulse_s: f64,
) -> Result<f64, MassError> {
    for v in [velocity_m_s, lift_to_drag, specific_impulse_s, range_m] {
        if v.is_nan() {
            return Err(MassError::NotANumber);
        }
        if v <= 0.0 {
            return Err(MassError::NonPhysicalPerformance);
        }
    }
    let ratio = libm::exp(range_m / (velocity_m_s * lift_to_drag * specific_impulse_s));
    Ok(1.0 - 1.0 / ratio)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ventus_units::float::rel_err;

    /// Design point, from the modules that produced each term.
    const CRUISE_VELOCITY_M_S: f64 = 1046.95; // M1 at M 3.5 / 26 km
    const LIFT_TO_DRAG: f64 = 5.5; // M6
    const SPECIFIC_IMPULSE_S: f64 = 1450.0; // M4, ramjet at the design point

    /// THE END-TO-END CHECK. Every term comes from a different module, and the
    /// product is compared against the SR-71's unrefuelled range of about
    /// 5400 km. Two published sources disagree on the unit: the NMUSAF fact
    /// sheet says "more than 2 900 statute miles" (= 4 700 km), while the
    /// figure widely repeated as 2 900 nmi (= 5 400 km) reads nautical. The
    /// nautical reading is carried here; the assert band below spans both
    /// readings, so the check does not silently take sides.
    ///
    /// The SR-71's own numbers: **start-of-cruise** mass about 55 t falling to
    /// about 33 t, against an empty mass near 30 t. Mass ratio 1.67.
    ///
    /// [CORRECTED] This comment used to call 55 t the MID-cruise mass while the
    /// call below passed it as `initial_mass_kg`, which is the start of cruise.
    /// Those are different quantities and the gap between them is the cruise
    /// fuel, which is 22 t. The error was invisible here - the range assertion is
    /// wide enough to survive either - and became load-bearing the moment
    /// `SR71_EMPTY_FRACTION_OF_CRUISE_MASS` started dividing by this number:
    /// the two readings differ by 29 % in VENTUS-1's derived empty mass.
    ///
    /// **The model settles it; no citation was needed.** See
    /// `the_mid_cruise_reading_of_the_anchor_is_refuted_two_ways`.
    #[test]
    fn the_chain_reproduces_the_sr71_unrefuelled_range() {
        // SR-71 class: M 3.2, L/D about 6, turboramjet Isp about 1900 s at
        // cruise. Both **[TO CITE]** — no primary source read for either yet.
        let range_km =
            breguet_range_m(3.2 * 297.8, 6.0, 1900.0, 55_000.0, 33_000.0).unwrap() / 1000.0;
        assert!(
            (4000.0..8000.0).contains(&range_km),
            "SR-71 class range came out {range_km:.0} km against a published 5400"
        );
    }

    /// VENTUS-1 at its own design point, with the 28 t cruise mass M6 declared.
    #[test]
    fn the_ventus1_design_point_gives_a_useful_range() {
        let cruise_mass = 28_000.0;
        for fuel_fraction in [0.35, 0.45, 0.55] {
            let final_mass = cruise_mass * (1.0 - fuel_fraction);
            let range_km = breguet_range_m(
                CRUISE_VELOCITY_M_S,
                LIFT_TO_DRAG,
                SPECIFIC_IMPULSE_S,
                cruise_mass,
                final_mass,
            )
            .unwrap()
                / 1000.0;
            assert!(
                range_km > 1000.0,
                "fuel fraction {fuel_fraction}: only {range_km:.0} km"
            );
            assert!(
                range_km < 30_000.0,
                "{range_km:.0} km is more than the planet"
            );
        }
    }

    /// THE TWO FRAMINGS OF THE SAME EQUATION, WHICH GIVE OPPOSITE ANSWERS.
    ///
    /// [CORRECTED] This test asserted that the marginal kilogram of fuel is
    /// worth less as the tanks fill. It is not, and the model said so.
    ///
    /// Range goes as `ln(mass ratio)`, so per unit of MASS RATIO the returns
    /// diminish — that is the familiar statement. But a designer does not buy
    /// mass ratio, they buy FUEL FRACTION, and `ln(1/(1-f))` accelerates as `f`
    /// approaches 1 because the mass ratio runs away. Going from 10 % to 20 %
    /// fuel buys 0.118 of the logarithm; going from 80 % to 90 % buys 0.693,
    /// nearly six times as much.
    ///
    /// Both statements are true about the same equation and they point opposite
    /// ways. The one that matters operationally is the second, and it is why
    /// fuel fraction is the number aircraft designers fight over.
    ///
    /// **What actually stops it** is not in this model: the structure to carry
    /// the fuel grows with the fuel, so the empty mass is not independent of the
    /// fuel fraction. Breguet holds `Wf` fixed and therefore cannot see that.
    /// [KNOWN_LIMIT], and the reason a real design closes by iteration.
    #[test]
    fn range_diminishes_in_mass_ratio_but_accelerates_in_fuel_fraction() {
        let range = |fuel_fraction: f64| {
            breguet_range_m(
                CRUISE_VELOCITY_M_S,
                LIFT_TO_DRAG,
                SPECIFIC_IMPULSE_S,
                1.0,
                1.0 - fuel_fraction,
            )
            .unwrap()
        };

        // Half the mass in fuel gets ln(2); three quarters gets ln(4), exactly
        // twice. Tripling the fuel doubles the range.
        assert!(rel_err(range(0.75) / range(0.5), 2.0) < 1e-12);

        // In FUEL FRACTION the marginal return accelerates.
        let early = range(0.2) - range(0.1);
        let late = range(0.9) - range(0.8);
        assert!(
            late > 5.0 * early,
            "late gain {:.0} km against early {:.0} km; the logarithm should run              away near a fuel fraction of 1",
            late / 1000.0,
            early / 1000.0
        );

        // In MASS RATIO it diminishes, which is the other framing.
        let by_ratio = |r: f64| {
            breguet_range_m(
                CRUISE_VELOCITY_M_S,
                LIFT_TO_DRAG,
                SPECIFIC_IMPULSE_S,
                r,
                1.0,
            )
            .unwrap()
        };
        assert!(by_ratio(5.0) - by_ratio(4.0) < by_ratio(2.0) - by_ratio(1.0));
    }

    /// The inverse must round-trip, and it must saturate rather than return a
    /// fuel fraction above one.
    #[test]
    fn the_inverse_round_trips_and_saturates_at_the_impossible() {
        for range_km in [1000.0, 5000.0, 12_000.0] {
            let f = required_fuel_fraction(
                range_km * 1000.0,
                CRUISE_VELOCITY_M_S,
                LIFT_TO_DRAG,
                SPECIFIC_IMPULSE_S,
            )
            .unwrap();
            assert!((0.0..1.0).contains(&f), "{range_km} km needs fraction {f}");
            let back = breguet_range_m(
                CRUISE_VELOCITY_M_S,
                LIFT_TO_DRAG,
                SPECIFIC_IMPULSE_S,
                1.0,
                1.0 - f,
            )
            .unwrap()
                / 1000.0;
            assert!(rel_err(back, range_km) < 1e-9);
        }

        // [CORRECTED] An absurd range was asserted to give a fraction strictly
        // below 1. It gives exactly 1.0: the exponential overflows and
        // `1 - 1/inf` is 1. That is the right answer and the right way to say it
        // — "all of the mass, and it still is not enough" — but the test had to
        // stop demanding it be less.
        let absurd =
            required_fuel_fraction(1.0e9, CRUISE_VELOCITY_M_S, LIFT_TO_DRAG, SPECIFIC_IMPULSE_S)
                .unwrap();
        assert_eq!(absurd, 1.0, "an impossible range must saturate at 1.0");
    }

    /// [CORRECTED, TWICE] Isp enters linearly and fuel fraction
    /// logarithmically, and I twice guessed wrong about which wins.
    ///
    /// First guess: a 10 % better engine always beats 10 % more fuel. False —
    /// at a fuel fraction of 0.45 the engine gains 499 km and the fuel 796 km.
    ///
    /// Second guess: there is a crossover, with the engine winning when the
    /// tanks are nearly empty. Also false. Working it out instead of guessing:
    ///
    /// ```text
    ///   engine, +10 % Isp      0.1 · ln(1/(1-f))
    ///   fuel,   +10 % fraction  ln((1-f)/(1-1.1f))
    /// ```
    ///
    /// As `f -> 0` both tend to `0.1 f`, so they are EQUAL in the limit, and the
    /// fuel term grows faster everywhere above it. **A proportional increase in
    /// fuel fraction always beats the same proportional increase in specific
    /// impulse.** There is no crossover.
    ///
    /// Which is a statement about the equation, not about engineering: fuel
    /// fraction is bought with structure and volume, Isp is bought once in the
    /// engine and then carried for free. Breguet cannot see that cost, which is
    /// the [KNOWN_LIMIT] noted above.
    #[test]
    fn a_proportional_fuel_increase_always_beats_the_same_isp_increase() {
        let range = |isp: f64, fuel_fraction: f64| {
            breguet_range_m(
                CRUISE_VELOCITY_M_S,
                LIFT_TO_DRAG,
                isp,
                1.0,
                1.0 - fuel_fraction,
            )
            .unwrap()
        };

        // Isp is exactly linear. That much is an identity.
        assert!(rel_err(range(1595.0, 0.45) / range(1450.0, 0.45), 1.1) < 1e-12);

        let engine_gain = |f: f64| range(1595.0, f) - range(1450.0, f);
        let fuel_gain = |f: f64| range(1450.0, f * 1.1) - range(1450.0, f);

        // Fuel wins everywhere, by a margin that widens as the tanks fill.
        let mut previous_ratio = 0.0;
        for f in [0.05, 0.1, 0.3, 0.45, 0.6, 0.75] {
            let ratio = fuel_gain(f) / engine_gain(f);
            assert!(
                ratio > 1.0,
                "at a fuel fraction of {f} the engine gained more, ratio {ratio:.4}"
            );
            assert!(ratio > previous_ratio, "the margin should widen with f");
            previous_ratio = ratio;
        }

        // And they converge as the tanks empty, which is the limit that says
        // there is no crossover to look for.
        assert!(
            fuel_gain(1e-4) / engine_gain(1e-4) < 1.001,
            "the two should be equal in the limit of an empty aircraft"
        );
    }

    #[test]
    fn the_mass_breakdown_is_consistent() {
        let m = MassBreakdown {
            empty_mass_kg: 12_000.0,
            fuel_mass_kg: 15_000.0,
            payload_mass_kg: 1_000.0,
        };
        assert!(rel_err(m.takeoff_mass_kg(), 28_000.0) < 1e-12);
        assert!(rel_err(m.final_mass_kg(), 13_000.0) < 1e-12);
        assert!(
            rel_err(
                m.fuel_fraction() + m.empty_fraction(),
                1.0 - 1000.0 / 28_000.0
            ) < 1e-12
        );
        // SR-71 class fuel fraction, near 0.55. [TO CITE]
        assert!((0.5..0.6).contains(&m.fuel_fraction()));
    }

    #[test]
    fn non_physical_input_is_refused() {
        assert_eq!(
            breguet_range_m(1000.0, 5.0, 1400.0, 100.0, 200.0),
            Err(MassError::NonPhysicalMass)
        );
        assert_eq!(
            breguet_range_m(1000.0, 5.0, 1400.0, 100.0, 0.0),
            Err(MassError::NonPhysicalMass)
        );
        assert_eq!(
            breguet_range_m(1000.0, -1.0, 1400.0, 100.0, 50.0),
            Err(MassError::NonPhysicalPerformance)
        );
        assert_eq!(
            breguet_range_m(f64::NAN, 5.0, 1400.0, 100.0, 50.0),
            Err(MassError::NotANumber)
        );
    }
}
// ---------------------------------------------------------------------------
// Zero-fuel mass. [CLOSES the [TO COMPUTE] carried by M11 and ADR-002.]
// ---------------------------------------------------------------------------

/// SR-71 **zero fuel weight**, 59 000 lb.
///
/// SOURCE, PRIMARY: NASA, "The SR-71 Test Bed Aircraft: A Facility for
/// High-Speed Flight Research", Table 1: "Basic aircraft zero fuel weight
/// 59,000 lb". Cross-checked against the declassified SR-71 Flight Manual,
/// page 1-4: "Zero fuel weight varies from 56,500 to more than 60,000 pounds."
/// 59 000 lb sits inside that band, so two independent documents agree.
///
/// # [CORRECTED] This replaces a recited 30 600 kg that no source supports
///
/// The project carried 30 600 kg (67 461 lb) as the SR-71 "empty mass". Nothing
/// in either primary document supports it; it is 13 % above the flight manual's
/// upper bound. It had been recited, never read.
///
/// # Zero fuel weight, not empty weight, and the distinction is load-bearing
///
/// ZFW is the aircraft with everything aboard except fuel: airframe, crew, oil,
/// sensors. It is what the sources actually state, and it is measurable, whereas
/// "empty weight" is ambiguous between manufacturer's and operating empty. The
/// flight manual's 3 500 lb spread is the sensor fit varying.
///
/// The module is therefore expressed in ZFW rather than empty mass, and
/// [`close_cruise`] takes RESERVE FUEL rather than "payload and reserve":
/// payload is already inside this number. Conflating the two double-counted the
/// payload, which is the error this renaming removes.
pub const SR71_ZERO_FUEL_MASS_KG: f64 = 59_000.0 * 0.453_592_37;

/// Lower bound of the flight manual's zero-fuel band, 56 500 lb.
pub const SR71_ZERO_FUEL_MASS_MIN_KG: f64 = 56_500.0 * 0.453_592_37;
/// Upper bound of the flight manual's zero-fuel band, 60 000 lb.
pub const SR71_ZERO_FUEL_MASS_MAX_KG: f64 = 60_000.0 * 0.453_592_37;

/// SR-71 loaded gross mass, upper end of the flight manual's stated range:
/// 140 000 lb.
///
/// SOURCE, PRIMARY: SR-71 Flight Manual page 1-4, "The loaded gross weight of
/// the aircraft varies from approximately 135,000 to over 140,000 pounds." The
/// NASA test bed took off at 143 000 lb (Table 1 of the report above), which is
/// consistent with "over 140,000".
///
/// # Two sources disagree, and the disagreement is carried rather than resolved
///
/// Secondary and tertiary references widely quote a maximum takeoff weight of
/// 170 000 to 172 000 lb, roughly 23 % above the flight manual's range. The gap
/// is most likely operational versus structural limit, but no primary document
/// stating a structural maximum has been read, so this project does not pick.
///
/// It does not have to. The only place gross mass is used is an INEQUALITY, in
/// `the_mid_cruise_reading_of_the_anchor_is_refuted_two_ways`, and it holds
/// under both readings by 44 % and 17 % respectively. Where a conclusion needs
/// an upper bound, [`SR71_MAX_PLAUSIBLE_GROSS_MASS_KG`] takes the LARGER figure,
/// which is the one that makes the conclusion hardest to reach.
pub const SR71_LOADED_GROSS_MASS_KG: f64 = 140_000.0 * 0.453_592_37;

/// The most generous gross mass anywhere in the literature, 172 000 lb.
/// **[TO CITE]**, tertiary, and used deliberately.
///
/// Arguments needing an upper bound on gross mass use this rather than the
/// flight manual figure, because erring generous errs against the conclusion
/// being drawn. A refutation that survives 172 000 lb survives the truth.
pub const SR71_MAX_PLAUSIBLE_GROSS_MASS_KG: f64 = 172_000.0 * 0.453_592_37;

/// SR-71 mass at the start of cruise. **[TO CITE]: the one mass here that no
/// primary source has been found for.**
///
/// Used by `sr71_unrefuelled_range` and as the denominator of the zero-fuel
/// fraction. It is BOUNDED by the cited figures, not established by them: it
/// must sit below the loaded gross of 61 235 to 63 503 kg and above the
/// end-of-cruise mass, and 55 000 kg does. That is consistency, not provenance.
pub const SR71_START_OF_CRUISE_MASS_KG: f64 = 55_000.0;

/// SR-71 mass at the end of cruise. **[TO CITE]**, same status as above.
///
/// Consistency check it passes: 33 000 kg less the cited zero-fuel mass leaves
/// 6 238 kg of reserve, 11.3 % of the start-of-cruise mass. Large, but an
/// aircraft descending and landing from M 3.2 at 24 km is not gliding home.
pub const SR71_END_OF_CRUISE_MASS_KG: f64 = 33_000.0;

/// Zero-fuel mass as a fraction of start-of-cruise mass, from the SR-71.
///
/// # What is cited and what is not
///
/// The NUMERATOR is primary. The DENOMINATOR is **[TO CITE]**: the flight manual
/// gives loaded gross and zero fuel, not a cruise mass schedule. So this
/// fraction is half-cited. That is better than it was, it is not closed, and the
/// difference is written down rather than rounded off.
///
/// [CORRECTED] Was `30_600.0 / 55_000.0` = 0.5564, over a mass no source
/// supports. The cited value is 0.4866, **12.5 % lower**, and it propagates into
/// VENTUS-1's derived mass and from there into M11's cost estimate.
///
/// # Do not substitute the familiar number
///
/// Tertiary sources quote an SR-71 empty weight near 60 000 lb against a max
/// takeoff of 170 000 lb, a fraction near 0.35. That is zero-fuel over GROSS
/// mass. This is zero-fuel over START-OF-CRUISE mass, because M6b declares
/// VENTUS-1's 28 t as a cruise mass and a fraction must be taken against the
/// same reference as the mass it multiplies. Substituting 0.35 would look like a
/// correction and would shrink the derived mass by 28 %, in the flattering
/// direction. Guarded by
/// `the_fraction_must_be_taken_against_cruise_mass_not_gross_mass`.
pub const SR71_ZERO_FUEL_FRACTION_OF_CRUISE_MASS: f64 =
    SR71_ZERO_FUEL_MASS_KG / SR71_START_OF_CRUISE_MASS_KG;

// ---------------------------------------------------------------------------
// The mass relations, as BUILD-TIME assertions.
//
// These are relations between constants, so a runtime test over them is
// `assert!(true)` after constant folding and checks nothing. As `const _: ()`
// they break the build instead, which is what a relation that must always hold
// deserves.
// ---------------------------------------------------------------------------

/// The cited anchor sits inside the flight manual's stated band.
const _: () = assert!(
    SR71_ZERO_FUEL_MASS_KG > SR71_ZERO_FUEL_MASS_MIN_KG
        && SR71_ZERO_FUEL_MASS_KG < SR71_ZERO_FUEL_MASS_MAX_KG,
    "the cited zero-fuel mass left the flight manual band"
);

/// The recited 30 600 kg this replaced sits OUTSIDE that band, which is why it
/// went. Pinned so the old value cannot drift back in unnoticed.
const _: () = assert!(
    30_600.0 > SR71_ZERO_FUEL_MASS_MAX_KG,
    "the superseded 30 600 kg is no longer outside the cited band; recheck why it was replaced"
);

/// Cruise starts after climb and acceleration, so below loaded gross, and ends
/// above the zero-fuel mass because the margin between them is reserve fuel.
/// This is the only thing propping up the uncited start-of-cruise mass.
const _: () = assert!(
    SR71_START_OF_CRUISE_MASS_KG < SR71_LOADED_GROSS_MASS_KG
        && SR71_START_OF_CRUISE_MASS_KG > SR71_END_OF_CRUISE_MASS_KG
        && SR71_END_OF_CRUISE_MASS_KG > SR71_ZERO_FUEL_MASS_KG,
    "the uncited cruise masses no longer sit inside the cited bounds"
);

/// The generous gross mass must actually be the generous one, or every argument
/// that leans on it for conservatism is inverted.
const _: () = assert!(
    SR71_MAX_PLAUSIBLE_GROSS_MASS_KG > SR71_LOADED_GROSS_MASS_KG,
    "the tertiary gross mass is no longer the larger figure"
);

/// What a cruise leg leaves over once the airframe and its load are paid for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CruiseClosure {
    /// Mass at the end of cruise: zero-fuel mass plus the reserve still aboard.
    pub final_mass_kg: f64,
    /// Fuel burnt over the leg, as a fraction of start-of-cruise mass.
    pub fuel_fraction: f64,
    /// Range that fuel actually buys.
    pub range_m: f64,
}

/// Zero-fuel mass from a declared cruise mass and an anchored fraction.
///
/// M6b declares the 28 t cruise mass as the one number in the geometry that is
/// chosen rather than derived. This turns that choice into a zero-fuel mass
/// without adding a second free parameter.
///
/// # Errors
/// [`MassError`] for non-physical input or a fraction outside `(0, 1)`.
pub fn zero_fuel_mass_from_cruise_anchor(
    cruise_mass_kg: f64,
    zero_fuel_fraction: f64,
) -> Result<f64, MassError> {
    if cruise_mass_kg.is_nan() || zero_fuel_fraction.is_nan() {
        return Err(MassError::NotANumber);
    }
    if cruise_mass_kg <= 0.0 {
        return Err(MassError::NonPhysicalMass);
    }
    if zero_fuel_fraction <= 0.0 || zero_fuel_fraction >= 1.0 {
        return Err(MassError::NonPhysicalMass);
    }
    Ok(cruise_mass_kg * zero_fuel_fraction)
}

/// Close the cruise leg: what range is left once the airframe, its load and the
/// reserve have been paid for out of the cruise mass.
///
/// The direction a designer actually works in. `breguet_range_m` asks "given a
/// mass ratio, how far?"; this asks "given an aircraft and what it must carry,
/// how far CAN it go?", and unlike the former it can answer that the answer is
/// nowhere, which is the interesting case.
///
/// `reserve_fuel_kg` is fuel only. Payload is already inside the zero-fuel mass.
///
/// # Errors
/// [`MassError::MissionDoesNotClose`] when the zero-fuel mass plus its reserve
/// already meets or exceeds the cruise mass, so there is no burnable fuel
/// aboard. Other [`MassError`]s for non-physical input.
pub fn close_cruise(
    cruise_mass_kg: f64,
    zero_fuel_mass_kg: f64,
    reserve_fuel_kg: f64,
    velocity_m_s: f64,
    lift_to_drag: f64,
    specific_impulse_s: f64,
) -> Result<CruiseClosure, MassError> {
    for v in [cruise_mass_kg, zero_fuel_mass_kg, reserve_fuel_kg] {
        if v.is_nan() {
            return Err(MassError::NotANumber);
        }
    }
    if cruise_mass_kg <= 0.0 || zero_fuel_mass_kg <= 0.0 || reserve_fuel_kg < 0.0 {
        return Err(MassError::NonPhysicalMass);
    }

    let final_mass_kg = zero_fuel_mass_kg + reserve_fuel_kg;
    if final_mass_kg >= cruise_mass_kg {
        return Err(MassError::MissionDoesNotClose);
    }

    let range_m = breguet_range_m(
        velocity_m_s,
        lift_to_drag,
        specific_impulse_s,
        cruise_mass_kg,
        final_mass_kg,
    )?;

    Ok(CruiseClosure {
        final_mass_kg,
        fuel_fraction: 1.0 - final_mass_kg / cruise_mass_kg,
        range_m,
    })
}

/// The SR-71's own reserve fuel at the end of cruise, as a fraction of its
/// start-of-cruise mass: 11.3 %.
///
/// Derived from the cited zero-fuel mass and the **[TO CITE]** end-of-cruise
/// mass, so it inherits the weaker of the two. It exists because the alternative
/// to anchoring VENTUS-1's reserve is choosing it, and a chosen reserve is what
/// carried an earlier result that did not survive the citation. See
/// `the_five_thousand_kilometre_question_turns_on_the_reserve`.
#[must_use]
pub fn sr71_reserve_fraction_of_cruise_mass() -> f64 {
    (SR71_END_OF_CRUISE_MASS_KG - SR71_ZERO_FUEL_MASS_KG) / SR71_START_OF_CRUISE_MASS_KG
}

/// Raymer's empty-weight-fraction correlation for a jet fighter,
/// `We/W0 = 2.34 W0^-0.13` with `W0` in pounds. **[TO VERIFY]**
///
/// # Present only to show that it does not apply
///
/// The correlation is fitted on fighters, and a fighter is a different kind of
/// object from a vehicle that is more than half fuel and made of titanium. At
/// the SR-71's cited loaded gross mass it over-predicts the structural fraction
/// by a fifth.
///
/// The direction matters. Over-predicting the structure under-predicts fuel,
/// which under-predicts range, so this correlation would have made VENTUS-1 look
/// WORSE than the anchor does, not better. It is still not used: a number being
/// conservative is not the same as it being right.
#[must_use]
pub fn raymer_jet_fighter_empty_fraction(takeoff_mass_kg: f64) -> f64 {
    const POUNDS_PER_KILOGRAM: f64 = 1.0 / 0.453_592_37;
    2.34 * libm::pow(takeoff_mass_kg * POUNDS_PER_KILOGRAM, -0.13)
}

// ---------------------------------------------------------------------------
// VENTUS-1, derived.
// ---------------------------------------------------------------------------

/// Start-of-cruise mass, from M6b. The one chosen number in the chain.
pub const VENTUS1_CRUISE_MASS_KG: f64 = 28_000.0;

/// **VENTUS-1 zero-fuel mass, 13 624 kg.** Derived, not chosen: M6b's cruise
/// mass times the SR-71 zero-fuel fraction. Closes the `[TO COMPUTE]` M11 and
/// ADR-002 were carrying.
///
/// [CORRECTED] Was 15 578 kg, from a recited SR-71 mass no source supports. The
/// cited anchor puts it 12.5 % lower, which propagates into M11's cost estimate
/// and overturns a result this project had already published. See
/// `the_five_thousand_kilometre_question_turns_on_the_reserve`.
pub const VENTUS1_ZERO_FUEL_MASS_KG: f64 =
    VENTUS1_CRUISE_MASS_KG * SR71_ZERO_FUEL_FRACTION_OF_CRUISE_MASS;
#[cfg(test)]
mod zero_fuel_mass_tests {
    use super::*;

    const V: f64 = 1_046.946_565; // M1 at M 3.5 / 26 km
    const LD: f64 = 5.5; // M6b
    const ISP: f64 = 1450.0; // M4

    /// The derived number, and the arithmetic behind it in one place.
    #[test]
    fn the_zero_fuel_mass_is_the_cruise_mass_times_the_anchor_fraction() {
        let derived = zero_fuel_mass_from_cruise_anchor(
            VENTUS1_CRUISE_MASS_KG,
            SR71_ZERO_FUEL_FRACTION_OF_CRUISE_MASS,
        )
        .unwrap();
        assert_eq!(derived, VENTUS1_ZERO_FUEL_MASS_KG);
        assert!(
            (derived - 13_624.27).abs() < 0.01,
            "zero-fuel mass moved to {derived}"
        );
    }

    /// The mass relations are checked at COMPILE TIME, above, not here.
    ///
    /// [CORRECTED] They were runtime `assert!`s over constants, which clippy
    /// correctly flagged as `assert!(true)`: the compiler folds them away and
    /// they test nothing. This repository already catalogues one test that
    /// tested nothing; this would have been the second. Promoted to `const _: ()
    /// = assert!(..)`, which is strictly stronger - it breaks the BUILD, not a
    /// test run, and it cannot be skipped by a filtered `cargo test`.
    ///
    /// What is left here is the one quantity that is actually computed.
    #[test]
    fn the_implied_reserve_fraction_is_plausible() {
        let reserve = sr71_reserve_fraction_of_cruise_mass();
        assert!(
            (0.10..0.13).contains(&reserve),
            "implied reserve fraction {reserve:.4} left the plausible band"
        );
    }

    /// [CORRECTED] THE RESULT THAT DID NOT SURVIVE THE CITATION.
    ///
    /// This project previously reported, and committed, that a 5000 km cruise
    /// requirement "does not close" at the design point. That rested on a
    /// recited SR-71 mass of 30 600 kg which no source supports, and on a term
    /// that conflated payload with reserve. With the cited zero-fuel mass the
    /// aircraft is 12.5 % lighter and the answer turns entirely on the reserve,
    /// which is a CHOICE nobody in this project has made:
    ///
    ///   reserve 500 kg                   -> 5714 km, the requirement closes
    ///   reserve at the anchor's fraction -> 4265 km, it does not
    ///
    /// So the honest statement is not "5000 km fails". It is "5000 km is
    /// reserve-limited, and this project has not chosen a reserve policy". The
    /// earlier claim was an artefact of an uncited number.
    #[test]
    fn the_five_thousand_kilometre_question_turns_on_the_reserve() {
        let generous = close_cruise(
            VENTUS1_CRUISE_MASS_KG,
            VENTUS1_ZERO_FUEL_MASS_KG,
            500.0,
            V,
            LD,
            ISP,
        )
        .unwrap();
        assert!(
            generous.range_m > 5.0e6,
            "with a 500 kg reserve the requirement should close, got {:.0} km",
            generous.range_m / 1000.0
        );

        let anchored = close_cruise(
            VENTUS1_CRUISE_MASS_KG,
            VENTUS1_ZERO_FUEL_MASS_KG,
            sr71_reserve_fraction_of_cruise_mass() * VENTUS1_CRUISE_MASS_KG,
            V,
            LD,
            ISP,
        )
        .unwrap();
        assert!(
            anchored.range_m < 5.0e6,
            "at the anchor reserve fraction it should not close, got {:.0} km",
            anchored.range_m / 1000.0
        );

        std::println!(
            "VENTUS-1 zero-fuel {:.0} kg: 500 kg reserve -> {:.0} km; anchored reserve -> {:.0} km",
            VENTUS1_ZERO_FUEL_MASS_KG,
            generous.range_m / 1000.0,
            anchored.range_m / 1000.0
        );
    }

    /// Scaling every mass by the same anchor reproduces the SR-71 mass ratio
    /// exactly, and therefore its range. Worth pinning because it explains why
    /// the corrected zero-fuel mass did NOT move the anchored answer: the
    /// correction cancels when the reserve is anchored too.
    #[test]
    fn anchoring_the_reserve_as_well_reproduces_the_anchor_mass_ratio() {
        let scaled = close_cruise(
            VENTUS1_CRUISE_MASS_KG,
            VENTUS1_ZERO_FUEL_MASS_KG,
            sr71_reserve_fraction_of_cruise_mass() * VENTUS1_CRUISE_MASS_KG,
            V,
            LD,
            ISP,
        )
        .unwrap();
        let ours = VENTUS1_CRUISE_MASS_KG / scaled.final_mass_kg;
        let theirs = SR71_START_OF_CRUISE_MASS_KG / SR71_END_OF_CRUISE_MASS_KG;
        assert!(
            (ours - theirs).abs() < 1e-12,
            "mass ratio {ours} against the anchor {theirs}"
        );
    }

    /// Range falls as the reserve rises, and the closure agrees with Breguet run
    /// directly. Two routes to the same number.
    #[test]
    fn the_closure_agrees_with_breguet_run_forwards() {
        let mut previous = f64::INFINITY;
        for reserve in [500.0, 1000.0, 2000.0, 3000.0] {
            let c = close_cruise(
                VENTUS1_CRUISE_MASS_KG,
                VENTUS1_ZERO_FUEL_MASS_KG,
                reserve,
                V,
                LD,
                ISP,
            )
            .unwrap();
            let direct = breguet_range_m(
                V,
                LD,
                ISP,
                VENTUS1_CRUISE_MASS_KG,
                VENTUS1_ZERO_FUEL_MASS_KG + reserve,
            )
            .unwrap();
            assert!((c.range_m - direct).abs() < 1e-9 * direct);
            assert!(c.range_m < previous, "range rose with reserve");
            previous = c.range_m;
        }
    }

    /// The textbook correlation, and why it is not used. Stated against the
    /// CITED gross mass now rather than a recited one.
    #[test]
    fn the_jet_fighter_correlation_overpredicts_the_structure() {
        let predicted = raymer_jet_fighter_empty_fraction(SR71_LOADED_GROSS_MASS_KG);
        let actual = SR71_ZERO_FUEL_MASS_KG / SR71_LOADED_GROSS_MASS_KG;
        let over = predicted / actual - 1.0;
        std::println!(
            "Raymer We/W0 = {predicted:.4} against a cited zero-fuel fraction {actual:.4}: {:+.1} %",
            100.0 * over
        );
        assert!(
            over > 0.15,
            "the correlation should over-predict the structure; got {over:+.3}"
        );
    }

    #[test]
    fn a_mission_that_cannot_close_is_refused_rather_than_returned() {
        assert_eq!(
            close_cruise(10_000.0, 12_000.0, 0.0, V, LD, ISP),
            Err(MassError::MissionDoesNotClose)
        );
        // Exactly equal: no burnable fuel, so still no mission.
        assert_eq!(
            close_cruise(16_000.0, 15_000.0, 1000.0, V, LD, ISP),
            Err(MassError::MissionDoesNotClose)
        );
        // One kilogram of fuel is a mission, if a short one.
        assert!(close_cruise(16_001.0, 15_000.0, 1000.0, V, LD, ISP).is_ok());
    }

    #[test]
    fn the_anchor_fraction_is_refused_outside_the_unit_interval() {
        for bad in [0.0, 1.0, -0.5, 1.5] {
            assert_eq!(
                zero_fuel_mass_from_cruise_anchor(28_000.0, bad),
                Err(MassError::NonPhysicalMass),
                "fraction {bad} was accepted"
            );
        }
    }
}

#[cfg(test)]
mod anchor_reading_tests {
    use super::*;

    const V: f64 = 952.899_424_000_000_1; // M1, M 3.2 at 24 km
    const LD: f64 = 6.0; // [TO CITE]
    const ISP: f64 = 1900.0; // [TO CITE]

    /// RESOLVING AN AMBIGUITY IN THIS MODULE'S OWN INPUTS, WITHOUT A CITATION.
    ///
    /// The doc comment on the range check called 55 t the mid-cruise mass; the
    /// call passed it as the start of cruise. Nothing distinguished them, and
    /// once the zero-fuel derivation began dividing by that number the
    /// difference stopped being cosmetic.
    ///
    /// Both mid-cruise readings die, for unrelated reasons, which is what makes
    /// the conclusion safe rather than merely convenient.
    #[test]
    fn the_mid_cruise_reading_of_the_anchor_is_refuted_two_ways() {
        // Reading B1: mid-cruise as the geometric mean, which is the one Breguet
        // makes natural because range goes as the log of the mass ratio.
        let implied_start = SR71_START_OF_CRUISE_MASS_KG * SR71_START_OF_CRUISE_MASS_KG
            / SR71_END_OF_CRUISE_MASS_KG;

        // Checked against the MOST GENEROUS gross mass in the literature, not
        // the cited one. If the refutation survives 172 000 lb it survives the
        // truth, whichever of the two disagreeing figures that turns out to be.
        assert!(
            implied_start > SR71_MAX_PLAUSIBLE_GROSS_MASS_KG,
            "the geometric reading implies {implied_start:.0} kg at cruise start, which must exceed even the most generous gross mass of {SR71_MAX_PLAUSIBLE_GROSS_MASS_KG:.0} kg"
        );
        // And against the cited flight-manual figure it fails far harder.
        assert!(implied_start > 1.4 * SR71_LOADED_GROSS_MASS_KG);

        // Reading B2: mid-cruise as the arithmetic mean. Not impossible, so it
        // has to be killed on performance instead.
        let implied_start_b2 = 2.0 * SR71_START_OF_CRUISE_MASS_KG - SR71_END_OF_CRUISE_MASS_KG;
        let range_b2 =
            breguet_range_m(V, LD, ISP, implied_start_b2, SR71_END_OF_CRUISE_MASS_KG).unwrap();
        assert!(
            range_b2 > 9.0e6,
            "B2 range came out {:.0} km",
            range_b2 / 1000.0
        );

        // Reading A: 55 t is the start of cruise. The only one that reproduces
        // the published range.
        let range_a = breguet_range_m(
            V,
            LD,
            ISP,
            SR71_START_OF_CRUISE_MASS_KG,
            SR71_END_OF_CRUISE_MASS_KG,
        )
        .unwrap();
        // 2 900 nmi = 5 400 km, the nautical reading (Wikipedia infobox);
        // the NMUSAF fact sheet reads "more than 2 900 statute miles" = 4 700 km.
        // The two sources disagree on the unit; see the end-to-end test above.
        let published = 5.4e6;
        assert!(
            (range_a / published - 1.0).abs() < 0.05,
            "reading A should land within 5 % of the published range, got {:.0} km",
            range_a / 1000.0
        );
        assert!(
            range_b2 / published > 1.5,
            "reading B2 should be far outside it"
        );

        std::println!(
            "A start-of-cruise: {:.0} km ({:+.1} %). B2 mid-cruise: {:.0} km ({:+.1} %). B1 mid-cruise: {:.0} kg at cruise start, above even a {:.0} kg gross mass.",
            range_a / 1000.0,
            100.0 * (range_a / published - 1.0),
            range_b2 / 1000.0,
            100.0 * (range_b2 / published - 1.0),
            implied_start,
            SR71_MAX_PLAUSIBLE_GROSS_MASS_KG
        );
    }

    /// THE OTHER HALF, AND THE ONE A WELL-MEANING READER IS LIKELIER TO BREAK.
    ///
    /// Even with the masses settled, the fraction must be taken against the same
    /// reference as the mass it is applied to. M6b declares VENTUS-1's 28 t as
    /// CRUISE mass, so the anchor must be zero-fuel-over-cruise-mass and not the
    /// familiar zero-fuel-over-gross-mass that tertiary sources quote.
    ///
    /// Substituting the familiar number would look like a correction and would
    /// shrink the derived mass, in the flattering direction.
    #[test]
    fn the_fraction_must_be_taken_against_cruise_mass_not_gross_mass() {
        let over_cruise = SR71_ZERO_FUEL_MASS_KG / SR71_START_OF_CRUISE_MASS_KG;
        let over_gross = SR71_ZERO_FUEL_MASS_KG / SR71_MAX_PLAUSIBLE_GROSS_MASS_KG;

        assert_eq!(over_cruise, SR71_ZERO_FUEL_FRACTION_OF_CRUISE_MASS);
        assert!(
            over_gross < 0.8 * over_cruise,
            "the two references should be far apart, or this test guards nothing"
        );

        let right = zero_fuel_mass_from_cruise_anchor(VENTUS1_CRUISE_MASS_KG, over_cruise).unwrap();
        let wrong = zero_fuel_mass_from_cruise_anchor(VENTUS1_CRUISE_MASS_KG, over_gross).unwrap();
        assert!(
            wrong < right,
            "the gross-mass fraction must be the flattering one, or the warning is backwards"
        );
        assert!(
            (right / wrong - 1.0) > 0.25,
            "the two readings should differ by about 28 %, got {:.1} %",
            100.0 * (right / wrong - 1.0)
        );
    }
}
