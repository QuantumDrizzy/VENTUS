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
    /// 5400 km. **[TO CITE]**
    ///
    /// The SR-71's own numbers: mid-cruise mass about 55 t against an empty mass
    /// near 30 t, so a mass ratio near 1.8.
    #[test]
    fn the_chain_reproduces_the_sr71_unrefuelled_range() {
        // SR-71 class: M 3.2, L/D about 6, turboramjet Isp about 1900 s at
        // cruise. All [TO CITE].
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
// Empty mass. [CLOSES the [TO COMPUTE] carried by M11 and ADR-002.]
// ---------------------------------------------------------------------------

/// Empty mass as a fraction of start-of-cruise mass, from the SR-71.
///
/// `30 600 / 55 000`, the same two numbers `sr71_unrefuelled_range` already
/// uses. **[TO CITE]**, and it is the anchor the whole empty-mass derivation
/// rests on, so it is the most load-bearing uncited number in this crate.
///
/// **[TO VERIFY] — an ambiguity in this module's own inputs.** The doc comment
/// on `the_chain_reproduces_the_sr71_unrefuelled_range` calls 55 t the
/// *mid-cruise* mass, while the case passes it as `initial_mass_kg`, which is
/// the mass at the *start* of cruise. Those are different quantities and the
/// gap between them is the cruise fuel, which is not small. Reading it as
/// start-of-cruise, as done here, gives the LARGER denominator and therefore the
/// SMALLER empty fraction - the direction that flatters the derived empty mass.
/// Resolving it needs a primary source for the SR-71 mass schedule.
///
/// # Why not a statistical correlation
///
/// Raymer's jet-fighter empty-weight fraction is the textbook route and it is
/// wrong here by a quarter: see [`raymer_jet_fighter_empty_fraction`]. A real
/// aircraft in the actual regime beats a correlation fitted outside it, which is
/// the same argument M11 makes about DAPCA IV.
pub const SR71_EMPTY_FRACTION_OF_CRUISE_MASS: f64 = 30_600.0 / 55_000.0;

/// What a cruise leg leaves over once the airframe and its load are paid for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CruiseClosure {
    /// Mass at the end of cruise: empty plus payload and reserve.
    pub final_mass_kg: f64,
    /// Fuel burnt over the leg, as a fraction of start-of-cruise mass.
    pub fuel_fraction: f64,
    /// Range that fuel actually buys.
    pub range_m: f64,
}

/// Empty mass from a declared cruise mass and an anchored empty fraction.
///
/// M6b declares the 28 t cruise mass as the one number in the geometry that is
/// chosen rather than derived. This turns that choice into an empty mass without
/// adding a second free parameter: the fraction comes from an aircraft that
/// exists.
///
/// # Errors
/// [`MassError`] for non-physical input or a fraction outside `(0, 1)`.
pub fn empty_mass_from_cruise_anchor(
    cruise_mass_kg: f64,
    empty_fraction: f64,
) -> Result<f64, MassError> {
    if cruise_mass_kg.is_nan() || empty_fraction.is_nan() {
        return Err(MassError::NotANumber);
    }
    if cruise_mass_kg <= 0.0 {
        return Err(MassError::NonPhysicalMass);
    }
    if empty_fraction <= 0.0 || empty_fraction >= 1.0 {
        return Err(MassError::NonPhysicalMass);
    }
    Ok(cruise_mass_kg * empty_fraction)
}

/// Close the cruise leg: what range is left once the airframe, the payload and
/// the reserve have been paid for out of the cruise mass.
///
/// This is the direction a designer actually works in. `breguet_range_m` asks
/// "given a mass ratio, how far?"; this asks "given an aircraft and what it must
/// carry, how far *can* it go?" - and unlike the former it can answer that the
/// answer is *nowhere*, which is the interesting case.
///
/// # Errors
/// [`MassError::MissionDoesNotClose`] when the empty mass plus its load already
/// meets or exceeds the cruise mass, so there is no fuel aboard. Other
/// [`MassError`]s for non-physical input.
pub fn close_cruise(
    cruise_mass_kg: f64,
    empty_mass_kg: f64,
    payload_and_reserve_kg: f64,
    velocity_m_s: f64,
    lift_to_drag: f64,
    specific_impulse_s: f64,
) -> Result<CruiseClosure, MassError> {
    for v in [cruise_mass_kg, empty_mass_kg, payload_and_reserve_kg] {
        if v.is_nan() {
            return Err(MassError::NotANumber);
        }
    }
    if cruise_mass_kg <= 0.0 || empty_mass_kg <= 0.0 || payload_and_reserve_kg < 0.0 {
        return Err(MassError::NonPhysicalMass);
    }

    let final_mass_kg = empty_mass_kg + payload_and_reserve_kg;
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

/// Raymer's empty-weight-fraction correlation for a jet fighter,
/// `We/W0 = 2.34 W0^-0.13` with `W0` in pounds. **[TO VERIFY]**
///
/// # Present only to show that it does not apply
///
/// At the SR-71's gross mass it returns 0.488 against an actual 0.392: it
/// **over-predicts the empty fraction by 24 %**. The correlation is fitted on
/// fighters, and a fighter is a different kind of object from a vehicle that is
/// more than half fuel and made of titanium.
///
/// That direction matters. Over-predicting empty mass under-predicts fuel, which
/// under-predicts range - so using this correlation would have made VENTUS-1
/// look *worse* than the anchor does, not better. It is still not used, because
/// a number being conservative is not the same as it being right, and the error
/// is large enough to swamp the thing being computed.
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

/// **VENTUS-1 empty mass, 15 578 kg.** Derived, not chosen: M6b's cruise mass
/// times the SR-71 empty fraction.
///
/// This closes the `[TO COMPUTE]` that M11 and ADR-002 were carrying. It is a
/// derivation resting on one uncited ratio, which is a weaker footing than most
/// numbers in this project - and stronger than the alternative, which was no
/// number at all and a cost model that could only emit a family of answers.
pub const VENTUS1_EMPTY_MASS_KG: f64 = VENTUS1_CRUISE_MASS_KG * SR71_EMPTY_FRACTION_OF_CRUISE_MASS;

#[cfg(test)]
mod empty_mass_tests {
    use super::*;

    const V: f64 = 1_046.946_565; // M1 at M 3.5 / 26 km
    const LD: f64 = 5.5; // M6b
    const ISP: f64 = 1450.0; // M4

    /// The derived number, and the arithmetic behind it in one place.
    #[test]
    fn the_empty_mass_is_the_cruise_mass_times_the_anchor_fraction() {
        let derived = empty_mass_from_cruise_anchor(
            VENTUS1_CRUISE_MASS_KG,
            SR71_EMPTY_FRACTION_OF_CRUISE_MASS,
        )
        .unwrap();
        assert_eq!(derived, VENTUS1_EMPTY_MASS_KG);
        assert!(
            (derived - 15_578.18).abs() < 0.01,
            "empty mass moved to {derived}"
        );
    }

    /// THE RESULT THAT CAME OUT OF CLOSING THIS, AND IT IS A NEGATIVE ONE.
    ///
    /// A 5000 km cruise requirement does not close at the design point. The fuel
    /// it needs leaves 15 384 kg at the end of cruise against an empty airframe
    /// of 15 578 kg — the aircraft arrives 194 kg lighter than it can possibly
    /// be, with nothing aboard at all.
    ///
    /// The module refuses rather than returning a negative payload, because a
    /// negative payload propagates as a number and looks like one.
    #[test]
    fn the_five_thousand_kilometre_requirement_does_not_close() {
        // The fuel fraction 5000 km demands, from Breguet inverted.
        let f = required_fuel_fraction(5.0e6, V, LD, ISP).unwrap();
        let end_of_cruise = VENTUS1_CRUISE_MASS_KG * (1.0 - f);
        assert!(
            end_of_cruise < VENTUS1_EMPTY_MASS_KG,
            "the requirement closes after all: {end_of_cruise} against {VENTUS1_EMPTY_MASS_KG}"
        );

        // With zero payload it still does not close, so it is not a payload
        // problem — the range itself is out of reach.
        assert_eq!(
            close_cruise(
                VENTUS1_CRUISE_MASS_KG * (1.0 - f),
                VENTUS1_EMPTY_MASS_KG,
                0.0,
                V,
                LD,
                ISP
            ),
            Err(MassError::MissionDoesNotClose)
        );
    }

    /// What the aircraft can actually do, which is the number the design point
    /// should carry instead of 5000 km.
    #[test]
    fn the_achievable_range_with_a_tonne_aboard() {
        let c = close_cruise(
            VENTUS1_CRUISE_MASS_KG,
            VENTUS1_EMPTY_MASS_KG,
            1000.0,
            V,
            LD,
            ISP,
        )
        .unwrap();
        std::println!(
            "VENTUS-1: empty {:.0} kg, 1 t aboard -> fuel fraction {:.4}, range {:.0} km",
            VENTUS1_EMPTY_MASS_KG,
            c.fuel_fraction,
            c.range_m / 1000.0
        );
        assert!(
            (c.range_m / 1000.0 - 4376.0).abs() < 1.0,
            "range moved to {:.1} km",
            c.range_m / 1000.0
        );
        assert!(c.fuel_fraction > 0.40 && c.fuel_fraction < 0.41);
    }

    /// Range falls as the load rises, and the closure stays consistent with
    /// Breguet run directly. Two routes to the same number.
    #[test]
    fn the_closure_agrees_with_breguet_run_forwards() {
        let mut previous = f64::INFINITY;
        for load in [500.0, 1000.0, 1500.0, 2000.0] {
            let c = close_cruise(
                VENTUS1_CRUISE_MASS_KG,
                VENTUS1_EMPTY_MASS_KG,
                load,
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
                VENTUS1_EMPTY_MASS_KG + load,
            )
            .unwrap();
            assert!((c.range_m - direct).abs() < 1e-9 * direct);
            assert!(c.range_m < previous, "range rose with load");
            previous = c.range_m;
        }
    }

    /// The textbook correlation, and why it is not used.
    #[test]
    fn the_jet_fighter_correlation_overpredicts_the_sr71_by_a_quarter() {
        let predicted = raymer_jet_fighter_empty_fraction(78_000.0);
        let actual = 30_600.0 / 78_000.0; // [TO CITE]
        let over = predicted / actual - 1.0;
        std::println!(
            "Raymer jet-fighter We/W0 = {predicted:.4} against an actual {actual:.4}: {:+.1} %",
            100.0 * over
        );
        assert!(
            over > 0.20,
            "the correlation used to over-predict by a quarter; now {over:+.3}"
        );
    }

    #[test]
    fn a_mission_that_cannot_close_is_refused_rather_than_returned() {
        // Empty airframe heavier than the cruise mass.
        assert_eq!(
            close_cruise(10_000.0, 12_000.0, 0.0, V, LD, ISP),
            Err(MassError::MissionDoesNotClose)
        );
        // Exactly equal: no fuel, so still no mission.
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
                empty_mass_from_cruise_anchor(28_000.0, bad),
                Err(MassError::NonPhysicalMass),
                "fraction {bad} was accepted"
            );
        }
    }
}
