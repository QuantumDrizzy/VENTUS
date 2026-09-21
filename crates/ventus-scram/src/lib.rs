//! Dual-mode ram/scram *track* for the Mach 5 stretch — a stub, not a cycle.
//!
//! This crate exists because `ventus_propulsion::ideal_ramjet` is a
//! **subsonic-combustion ramjet**. Extending it past its validity to print a
//! Mach 5 specific impulse would be the same class of error as extrapolating
//! `gamma_air` below 273 K: a plausible-looking number about the wrong physics.
//! ADR-003 is the decision; this module is the refusal that makes the decision
//! executable.
//!
//! # What this is
//!
//! A programme-track placeholder with typed refusals. The public surface names
//! the stations a dual-mode / scramjet cycle will need — inlet isolator,
//! combustor with a supersonic core, nozzle — and refuses to emit specific
//! thrust or Isp until those stations exist as models.
//!
//! The caller names a [`Regime`]. This crate does **not** infer ram / dual-mode
//! / scram from flight Mach. A transition Mach for this aircraft has not been
//! computed, and inventing one here would look like a result.
//!
//! # What this is not
//!
//! - **Not a claim that VENTUS-1 reaches Mach 5.** On the current configuration,
//!   required capture area reaches the body cross-section near M 3.85
//!   (`ventus-envelope`). That is an airframe statement. Nothing in this crate
//!   moves it.
//! - **Not a re-baseline.** The validated design-point snapshot remains Mach
//!   3.50 at 26 km geopotential. Asking this crate at Mach 5 does not change
//!   that snapshot, and a solved cycle is not returned at Mach 5 or anywhere
//!   else.
//! - **Not X-43 or X-51.** Those vehicles are *regime* anchors for the stretch:
//!   they show that air-breathing heat addition with a supersonic core has been
//!   flown. Their numbers are not this aircraft's numbers, and they are not
//!   transcribed here.
//! - **Not `ideal_ramjet` at higher Mach.** M4's refusal near M 5.65–5.70 is a
//!   burner-limit of a subsonic-combustion ramjet, not aircraft capability and
//!   not a scramjet ceiling. Requests for [`Regime::RamSubsonicBurner`] are
//!   sent back to M4 via [`CycleError::UseIdealRamjet`].
//!
//! # Validity
//!
//! Cycle evaluation is out of scope at every Mach until the stations exist.
//! [`solve_cycle`] returns `Err` for every well-formed request. The programme
//! names Mach 4 cruise and Mach 5 stretch as *intent* ([`PROGRAMME_CRUISE_MACH`],
//! [`PROGRAMME_STRETCH_MACH`]); those constants are refusal landmarks, not a
//! validity envelope and not a computed capability.
//!
//! # Non-goals (this crate, this revision)
//!
//! Manufacturable airframe, classified dual-mode data, a flyable Mach 5 claim.
#![no_std]
#![forbid(unsafe_code)]

#[cfg(test)]
extern crate std;

/// Programme cruise floor named in ADR-003 (≥ Mach 4).
///
/// **Not a validity bound and not a computed capability.** The validated
/// design point remains Mach 3.50 at 26 km. This constant exists so a caller
/// can name the stretch without inventing a second design point, and so tests
/// can pin that asking for cycle numbers here still refuses.
pub const PROGRAMME_CRUISE_MACH: f64 = 4.0;

/// Programme stretch/target named in ADR-003 (Mach 5).
///
/// Same caveat as [`PROGRAMME_CRUISE_MACH`]. The stub "reaching" this Mach
/// would mean returning `Ok` from [`solve_cycle`], which it does not.
pub const PROGRAMME_STRETCH_MACH: f64 = 5.0;

/// Combustor-core regime a caller is asking about.
///
/// These are labels, not a transition schedule. This crate will not pick one
/// from a Mach number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Regime {
    /// Subsonic-combustion ramjet. Owned by `ventus-propulsion::ideal_ramjet`.
    /// This crate does not re-solve it.
    RamSubsonicBurner,
    /// Dual-mode: the combustor can run with a subsonic or a supersonic core.
    /// Not modelled.
    DualModeTransition,
    /// Scramjet: heat addition with a supersonic core throughout. Not modelled.
    Scram,
}

/// Flowpath stations a dual-mode / scramjet cycle will need.
///
/// None of these is modelled. They are named so the gap is a type, not a
/// comment that can be read as a plan and forgotten.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Station {
    /// Forebody / inlet compression plus isolator. Not M3's shock train: a
    /// scram inlet keeps the core supersonic.
    InletIsolator,
    /// Heat addition with a supersonic core, or a dual-mode combustor that can
    /// run either way.
    CombustorSupersonicCore,
    /// Expansion. M4's station-9 isentropic expansion is the wrong engine.
    Nozzle,
}

impl Station {
    /// Whether this station has a model that can return state.
    ///
    /// Today: never. Pinned at compile time in this module so a future
    /// implementation cannot start returning numbers from [`solve_cycle`]
    /// while leaving the stations marked unimplemented by accident — it would
    /// have to change these `const` assertions in the same change.
    #[must_use]
    pub const fn is_modelled(self) -> bool {
        match self {
            Station::InletIsolator | Station::CombustorSupersonicCore | Station::Nozzle => false,
        }
    }

    /// Stations the dual-mode and scram paths both still need.
    ///
    /// Listed as the same three on purpose: this project has not earned a
    /// finer split.
    #[must_use]
    pub const fn flowpath() -> [Station; 3] {
        [
            Station::InletIsolator,
            Station::CombustorSupersonicCore,
            Station::Nozzle,
        ]
    }
}

// Honesty lock: every named station is unimplemented. When one is modelled,
// the matching assertion is deleted in the same change as `is_modelled`.
const _: () = assert!(!Station::InletIsolator.is_modelled());
const _: () = assert!(!Station::CombustorSupersonicCore.is_modelled());
const _: () = assert!(!Station::Nozzle.is_modelled());
const _: () = assert!(!this_crate_models(Regime::RamSubsonicBurner));
const _: () = assert!(!this_crate_models(Regime::DualModeTransition));
const _: () = assert!(!this_crate_models(Regime::Scram));

/// Why a dual-mode / scram request was refused.
///
/// These are refusals, not clamps. Returning a plausible Isp at Mach 5 would
/// corrupt every downstream claim while looking like progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CycleError {
    /// An input was NaN.
    NotANumber,
    /// Mach number was negative, or otherwise not a physical flight Mach.
    NonPhysicalMach,
    /// The request is a subsonic-combustion ramjet cycle. Call
    /// `ventus_propulsion::ideal_ramjet`; this crate does not own that engine.
    UseIdealRamjet,
    /// Dual-mode / scram stations do not exist as models yet. There is no
    /// Isp, no specific thrust, and no station state to return.
    StationsNotModelled,
    /// A named station was asked for a state it does not have.
    StationNotModelled(Station),
}

/// Inputs for a cycle request.
///
/// Altitude is accepted so the signature will not have to change the day
/// stations exist. It is not used: the crate refuses before it would need an
/// atmosphere.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CycleRequest {
    pub mach_freestream: f64,
    pub geopotential_altitude_m: f64,
    pub regime: Regime,
}

/// A solved dual-mode / scram cycle.
///
/// There is no public constructor, and there are no thrust or Isp fields.
/// The only function that can produce a `Cycle` is [`solve_cycle`], and it
/// does not, until the stations exist. This type is the API skeleton, not a
/// zeroed result.
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)]
pub struct Cycle {
    mach_freestream: f64,
    regime: Regime,
}

impl Cycle {
    #[allow(dead_code)]
    #[must_use]
    pub fn mach_freestream(&self) -> f64 {
        self.mach_freestream
    }

    #[allow(dead_code)]
    #[must_use]
    pub fn regime(&self) -> Regime {
        self.regime
    }
}

/// Station gas state. Same standing as [`Cycle`]: named, unconstructable
/// from outside, never returned.
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)]
pub struct StationState {
    station: Station,
}

impl StationState {
    #[allow(dead_code)]
    #[must_use]
    pub fn station(&self) -> Station {
        self.station
    }
}

/// Whether *this crate* will emit a cycle solution for `regime`.
///
/// [`Regime::RamSubsonicBurner`] is modelled in `ventus-propulsion`, not here.
/// Dual-mode and scram wait on every station in [`Station::flowpath`].
#[must_use]
pub const fn this_crate_models(regime: Regime) -> bool {
    match regime {
        Regime::RamSubsonicBurner => false,
        Regime::DualModeTransition | Regime::Scram => {
            Station::InletIsolator.is_modelled()
                && Station::CombustorSupersonicCore.is_modelled()
                && Station::Nozzle.is_modelled()
        }
    }
}

/// Refusal that belongs to a well-formed request for `regime`.
#[must_use]
pub const fn refuse_for(regime: Regime) -> CycleError {
    match regime {
        Regime::RamSubsonicBurner => CycleError::UseIdealRamjet,
        Regime::DualModeTransition | Regime::Scram => CycleError::StationsNotModelled,
    }
}

fn check_physical(request: CycleRequest) -> Result<(), CycleError> {
    if !request.mach_freestream.is_finite() || !request.geopotential_altitude_m.is_finite() {
        return Err(CycleError::NotANumber);
    }
    if request.mach_freestream < 0.0 {
        return Err(CycleError::NonPhysicalMach);
    }
    Ok(())
}

/// Solve a dual-mode / scram cycle.
///
/// Always a refusal today. The return type is `Result` so that adding stations
/// later is a fill-in rather than a signature change, and so that a caller
/// cannot obtain a number without handling the error.
///
/// # Errors
///
/// [`CycleError::NotANumber`] or [`CycleError::NonPhysicalMach`] if the
/// request is not a physical flight condition; [`CycleError::UseIdealRamjet`]
/// if the named regime is M4's engine; [`CycleError::StationsNotModelled`]
/// if the named regime is dual-mode or scram.
pub fn solve_cycle(request: CycleRequest) -> Result<Cycle, CycleError> {
    check_physical(request)?;
    Err(refuse_for(request.regime))
}

/// Gas state at a named station.
///
/// # Errors
///
/// [`CycleError::StationNotModelled`] for every station that exists as a
/// variant, after the same physical checks as [`solve_cycle`].
pub fn station_state(station: Station, mach_freestream: f64) -> Result<StationState, CycleError> {
    if !mach_freestream.is_finite() {
        return Err(CycleError::NotANumber);
    }
    if mach_freestream < 0.0 {
        return Err(CycleError::NonPhysicalMach);
    }
    Err(CycleError::StationNotModelled(station))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(mach: f64, regime: Regime) -> CycleRequest {
        CycleRequest {
            mach_freestream: mach,
            geopotential_altitude_m: 26_000.0,
            regime,
        }
    }

    /// THE CONTRACT OF THE STUB. Asking for a scram cycle at the programme
    /// stretch returns a refusal, never a number that could be quoted as Isp
    /// or thrust. If this starts returning `Ok`, the stub has become a fake
    /// engine and ADR-003 has been violated.
    #[test]
    fn scram_at_the_programme_stretch_refuses_without_a_number() {
        let result = solve_cycle(request(PROGRAMME_STRETCH_MACH, Regime::Scram));
        assert_eq!(result, Err(CycleError::StationsNotModelled));
        assert!(result.is_err(), "a Cycle would be a Mach 5 claim");
    }

    /// Dual-mode at the Mach 4 cruise intent is the same refusal. Mach 4 is
    /// still an inlet/body geometry problem on the current configuration;
    /// this crate does not close it by inventing a dual-mode cycle.
    #[test]
    fn dual_mode_at_the_cruise_intent_refuses() {
        let result = solve_cycle(request(PROGRAMME_CRUISE_MACH, Regime::DualModeTransition));
        assert_eq!(result, Err(CycleError::StationsNotModelled));
    }

    /// The design point is not this crate's to re-solve. A ramjet request is
    /// sent back to M4 rather than quietly becoming a second `ideal_ramjet`.
    #[test]
    fn ram_request_at_the_design_point_points_at_m4() {
        let result = solve_cycle(request(3.5, Regime::RamSubsonicBurner));
        assert_eq!(result, Err(CycleError::UseIdealRamjet));
    }

    /// Validity is empty until stations exist: well-formed requests refuse at
    /// the design point, at the stretch, and past M4's ramjet burner-limit
    /// refusal. There is no Mach at which this crate starts answering.
    #[test]
    fn cycle_evaluation_is_outside_validity_at_every_mach() {
        let machs = [0.0, 3.5, 3.85, 4.0, 5.0, 5.7, 10.0];
        for mach in machs {
            assert!(
                !this_crate_models(Regime::Scram),
                "scram became modelled without stations"
            );
            let scram = solve_cycle(request(mach, Regime::Scram));
            assert_eq!(scram, Err(CycleError::StationsNotModelled), "M {mach}");
            let dual = solve_cycle(request(mach, Regime::DualModeTransition));
            assert_eq!(dual, Err(CycleError::StationsNotModelled), "M {mach}");
            let ram = solve_cycle(request(mach, Regime::RamSubsonicBurner));
            assert_eq!(ram, Err(CycleError::UseIdealRamjet), "M {mach}");
        }
    }

    #[test]
    fn non_physical_requests_are_refused_as_such() {
        assert_eq!(
            solve_cycle(request(f64::NAN, Regime::Scram)),
            Err(CycleError::NotANumber)
        );
        assert_eq!(
            solve_cycle(CycleRequest {
                mach_freestream: 5.0,
                geopotential_altitude_m: f64::NAN,
                regime: Regime::Scram,
            }),
            Err(CycleError::NotANumber)
        );
        assert_eq!(
            solve_cycle(request(-1.0, Regime::Scram)),
            Err(CycleError::NonPhysicalMach)
        );
    }

    #[test]
    fn every_named_station_refuses_state() {
        for station in Station::flowpath() {
            assert!(!station.is_modelled());
            assert_eq!(
                station_state(station, PROGRAMME_STRETCH_MACH),
                Err(CycleError::StationNotModelled(station))
            );
        }
    }

    #[test]
    fn this_crate_models_nothing_yet() {
        for regime in [
            Regime::RamSubsonicBurner,
            Regime::DualModeTransition,
            Regime::Scram,
        ] {
            assert!(!this_crate_models(regime));
        }
    }
}
