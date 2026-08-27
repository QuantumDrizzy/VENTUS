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
