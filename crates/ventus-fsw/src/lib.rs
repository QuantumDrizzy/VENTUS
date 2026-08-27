//! M10 — Flight software.
//!
//! A deterministic fixed-step control loop with no allocation on the hot path.
//!
//! # What this module is actually for
//!
//! ADR-000 D1 chose Rust for the physics core rather than only for the flight
//! software, with one argument: if the digital twin and the flight software have
//! separate atmosphere implementations, they will diverge. That was a claim.
//! This module makes it testable — the air-data path here calls `ventus-atmos`,
//! the same code the twin uses, and a test asserts the two agree BIT FOR BIT.
//!
//! That is the whole payoff of the decision, and it is now a passing assertion
//! rather than an intention.
//!
//! # The properties that make it flight software
//!
//! - **No allocation.** `no_std` without `alloc`, so it is structural: there is
//!   no allocator to call. Not a discipline, an impossibility.
//! - **Bounded work.** Every loop here has a compile-time bound. No iteration
//!   count depends on the data, so worst-case timing equals typical timing.
//! - **Deterministic.** `libm` rather than the platform maths (ADR-001), so the
//!   same inputs give bit-identical outputs on every machine and every run.
//! - **Total on bad input.** Sensor faults produce a defined response rather
//!   than a panic. A control loop that can panic is not a control loop.
//!
//! # Latency budget
//!
//! Declared, not measured on a desktop: at a 100 Hz control rate the loop has
//! 10 ms, and this work is a few hundred floating-point operations plus two
//! `pow` calls in the atmosphere. That is microseconds on any target worth
//! considering. **[TO VERIFY]** on real hardware — a desktop measurement would
//! say nothing about a flight computer, so none is claimed here.

#![no_std]
#![forbid(unsafe_code)]

#[cfg(test)]
extern crate std;

use ventus_atmos::AtmosError;

/// Control loop rate [Hz]. The latency budget is its reciprocal.
pub const CONTROL_RATE_HZ: f64 = 100.0;

/// The time the loop has to finish in [s].
pub const LATENCY_BUDGET_S: f64 = 1.0 / CONTROL_RATE_HZ;

/// What the loop does when it cannot trust its inputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Health {
    /// Air data is valid and the commanded output is being tracked.
    Nominal,
    /// Altitude outside the atmosphere model's domain. The loop holds its last
    /// valid command rather than acting on a number it does not believe.
    AirDataInvalid,
    /// A sensor returned NaN. Same response, different cause, reported
    /// separately because they mean different things to whoever reads the log.
    SensorFault,
}

/// Air data derived from the shared atmosphere model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AirData {
    pub altitude_m: f64,
    pub mach: f64,
    pub dynamic_pressure_pa: f64,
    pub static_temperature_k: f64,
}

/// Compute air data from altitude and true airspeed.
///
/// Calls `ventus-atmos` directly — the same code path as the digital twin, which
/// is the point of ADR-000 D1.
///
/// # Errors
/// [`AtmosError`] if the altitude is outside the model domain.
pub fn air_data(altitude_m: f64, true_airspeed_m_s: f64) -> Result<AirData, AtmosError> {
    let state = ventus_atmos::at_geopotential(altitude_m)?;
    let mach = true_airspeed_m_s / state.speed_of_sound_m_s;
    Ok(AirData {
        altitude_m,
        mach,
        // 0.7 p M^2 for air, the same form the design point uses.
        dynamic_pressure_pa: 0.7 * state.pressure_pa * mach * mach,
        static_temperature_k: state.temperature_k,
    })
}

/// A gain-scheduled proportional-integral controller with explicit limits.
///
/// Scheduling on dynamic pressure rather than on Mach or altitude: control
/// effectiveness scales with `q`, so a fixed gain that is right at cruise is
/// far too high in the dense air low down. Dividing by `q` is the crudest
/// possible scheduling and it is the right first thing to do.
#[derive(Debug, Clone, Copy)]
pub struct Controller {
    pub proportional_gain: f64,
    pub integral_gain: f64,
    /// Command limit, symmetric [rad].
    pub command_limit_rad: f64,
    /// Integrator limit, to stop wind-up while the surface is saturated.
    pub integral_limit: f64,
    /// Reference dynamic pressure the gains were tuned at [Pa].
    pub reference_q_pa: f64,
}

impl Controller {
    /// A controller tuned at the VENTUS-1 cruise condition.
    #[must_use]
    pub const fn ventus1_pitch() -> Self {
        Self {
            proportional_gain: 1.2,
            integral_gain: 0.15,
            command_limit_rad: 0.35,
            integral_limit: 0.5,
            reference_q_pa: 18_463.0,
        }
    }
}

/// Everything the loop carries between steps. `Copy`, fixed size, no heap.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LoopState {
    pub integral: f64,
    pub last_command_rad: f64,
    pub health: Health,
    pub steps: u64,
}

impl Default for LoopState {
    fn default() -> Self {
        Self {
            integral: 0.0,
            last_command_rad: 0.0,
            health: Health::Nominal,
            steps: 0,
        }
    }
}

/// One control step.
///
/// Total: every input path produces a defined output, including NaN and
/// out-of-domain altitude. Nothing here can panic, allocate, or run for an
/// unbounded time.
#[must_use]
pub fn step(
    controller: &Controller,
    state: LoopState,
    altitude_m: f64,
    true_airspeed_m_s: f64,
    commanded_pitch_rad: f64,
    measured_pitch_rad: f64,
) -> LoopState {
    let mut next = state;
    next.steps = state.steps.wrapping_add(1);

    if altitude_m.is_nan()
        || true_airspeed_m_s.is_nan()
        || commanded_pitch_rad.is_nan()
        || measured_pitch_rad.is_nan()
    {
        next.health = Health::SensorFault;
        return next; // hold the last command
    }

    let Ok(air) = air_data(altitude_m, true_airspeed_m_s) else {
        next.health = Health::AirDataInvalid;
        return next; // hold the last command
    };

    // Gain scheduling. The clamp keeps the schedule finite at very low q, where
    // the surfaces do nothing anyway and dividing would blow the gain up.
    let schedule = (controller.reference_q_pa / air.dynamic_pressure_pa.max(1.0)).clamp(0.05, 20.0);

    let error = commanded_pitch_rad - measured_pitch_rad;
    let integral = (state.integral + error * LATENCY_BUDGET_S)
        .clamp(-controller.integral_limit, controller.integral_limit);

    let raw =
        schedule * (controller.proportional_gain * error + controller.integral_gain * integral);
    let command = raw.clamp(-controller.command_limit_rad, controller.command_limit_rad);

    // Anti-windup: if the command saturated, do not let the integrator keep
    // growing. Without this the loop overshoots badly on the way out of a limit.
    next.integral = if command == raw {
        integral
    } else {
        state.integral
    };
    next.last_command_rad = command;
    next.health = Health::Nominal;
    next
}

#[cfg(test)]
mod tests {
    use super::*;
    use ventus_units::float::abs;

    const CRUISE_ALT: f64 = 26_000.0;
    const CRUISE_TAS: f64 = 1046.95;

    /// THE PAYOFF OF ADR-000 D1, as an assertion rather than an intention.
    ///
    /// The flight software and the digital twin must compute the same
    /// atmosphere, BIT FOR BIT, because they are the same code. If this ever
    /// fails, someone has forked the model and the argument for putting the
    /// physics core in Rust has been lost.
    #[test]
    fn the_flight_software_and_the_twin_share_one_atmosphere_bit_for_bit() {
        for altitude in [0.0, 11_000.0, 26_000.0, 50_000.0, 84_000.0] {
            let via_fsw = air_data(altitude, CRUISE_TAS).unwrap();
            let via_twin = ventus_atmos::at_geopotential(altitude).unwrap();
            assert_eq!(
                via_fsw.static_temperature_k.to_bits(),
                via_twin.temperature_k.to_bits(),
                "temperature differs at {altitude} m"
            );
            let mach = CRUISE_TAS / via_twin.speed_of_sound_m_s;
            assert_eq!(via_fsw.mach.to_bits(), mach.to_bits());
            assert_eq!(
                via_fsw.dynamic_pressure_pa.to_bits(),
                (0.7 * via_twin.pressure_pa * mach * mach).to_bits()
            );
        }
    }

    /// The design point must come back out of the flight software's own air-data
    /// path, or the aircraft the software thinks it is flying is not the one the
    /// rest of the project designed.
    #[test]
    fn the_air_data_path_reproduces_the_design_point() {
        let air = air_data(CRUISE_ALT, CRUISE_TAS).unwrap();
        assert!(abs(air.mach - 3.5) < 1e-3, "M = {}", air.mach);
        assert!(
            abs(air.dynamic_pressure_pa - 18_463.0) < 20.0,
            "q = {:.0} Pa",
            air.dynamic_pressure_pa
        );
    }

    /// DETERMINISM. Same inputs, bit-identical outputs, every time. This is what
    /// `libm` was chosen for in ADR-001.
    #[test]
    fn the_loop_is_bit_deterministic() {
        let c = Controller::ventus1_pitch();
        let run = || {
            let mut s = LoopState::default();
            for i in 0..1000 {
                let commanded = 0.02 * libm::sin(f64::from(i) * 0.01);
                s = step(
                    &c,
                    s,
                    CRUISE_ALT,
                    CRUISE_TAS,
                    commanded,
                    s.last_command_rad * 0.1,
                );
            }
            s
        };
        let a = run();
        let b = run();
        assert_eq!(a.last_command_rad.to_bits(), b.last_command_rad.to_bits());
        assert_eq!(a.integral.to_bits(), b.integral.to_bits());
    }

    /// TOTALITY. Every bad input produces a defined response, and the loop holds
    /// its last command rather than acting on a number it does not believe.
    /// A control loop that can panic is not a control loop.
    #[test]
    fn bad_input_holds_the_last_command_rather_than_panicking() {
        let c = Controller::ventus1_pitch();
        let good = step(&c, LoopState::default(), CRUISE_ALT, CRUISE_TAS, 0.05, 0.0);
        assert_eq!(good.health, Health::Nominal);
        assert!(good.last_command_rad > 0.0);

        for (alt, tas, cmd, meas, expected) in [
            (f64::NAN, CRUISE_TAS, 0.05, 0.0, Health::SensorFault),
            (CRUISE_ALT, f64::NAN, 0.05, 0.0, Health::SensorFault),
            (CRUISE_ALT, CRUISE_TAS, f64::NAN, 0.0, Health::SensorFault),
            (200_000.0, CRUISE_TAS, 0.05, 0.0, Health::AirDataInvalid),
            (-100.0, CRUISE_TAS, 0.05, 0.0, Health::AirDataInvalid),
        ] {
            let s = step(&c, good, alt, tas, cmd, meas);
            assert_eq!(s.health, expected, "alt={alt} tas={tas} cmd={cmd}");
            assert_eq!(
                s.last_command_rad.to_bits(),
                good.last_command_rad.to_bits(),
                "the command must be HELD, not zeroed or recomputed"
            );
            assert_eq!(
                s.steps,
                good.steps + 1,
                "the step counter must still advance"
            );
        }
    }

    /// The command limit is a limit. Nothing gets past it, at any error.
    #[test]
    fn the_command_is_always_within_its_limit() {
        let c = Controller::ventus1_pitch();
        let mut s = LoopState::default();
        for i in 0..5000 {
            // A step demand far beyond anything achievable.
            let commanded = if i % 2 == 0 { 10.0 } else { -10.0 };
            s = step(&c, s, CRUISE_ALT, CRUISE_TAS, commanded, 0.0);
            assert!(
                abs(s.last_command_rad) <= c.command_limit_rad + 1e-15,
                "command {} exceeded the limit",
                s.last_command_rad
            );
            assert!(abs(s.integral) <= c.integral_limit + 1e-15);
        }
    }

    /// ANTI-WINDUP. Held against a saturating demand, the integrator must stop
    /// growing. Without this the loop overshoots badly coming out of a limit,
    /// which is the classic way an autopilot loses an aircraft.
    #[test]
    fn the_integrator_does_not_wind_up_while_saturated() {
        let c = Controller::ventus1_pitch();
        let mut s = LoopState::default();
        // Drive it hard into the limit.
        for _ in 0..200 {
            s = step(&c, s, CRUISE_ALT, CRUISE_TAS, 5.0, 0.0);
        }
        let saturated_integral = s.integral;
        for _ in 0..2000 {
            s = step(&c, s, CRUISE_ALT, CRUISE_TAS, 5.0, 0.0);
        }
        assert_eq!(
            s.integral.to_bits(),
            saturated_integral.to_bits(),
            "the integrator kept growing while the command was saturated"
        );
    }

    /// GAIN SCHEDULING. The same error must produce a smaller surface command in
    /// dense air, because control effectiveness scales with dynamic pressure. A
    /// fixed gain right at cruise would be violently over-responsive low down.
    #[test]
    fn the_gains_schedule_down_in_dense_air() {
        let c = Controller::ventus1_pitch();
        let at = |alt: f64, tas: f64| {
            step(&c, LoopState::default(), alt, tas, 0.02, 0.0).last_command_rad
        };
        let low = at(1_000.0, 300.0);
        let cruise = at(CRUISE_ALT, CRUISE_TAS);
        assert!(
            abs(low) < abs(cruise),
            "low altitude command {low:.5} should be smaller than cruise {cruise:.5}"
        );
        assert!(low > 0.0 && cruise > 0.0, "both should push the same way");
    }

    /// The latency budget is a declared number and the rate must match it.
    #[test]
    fn the_latency_budget_matches_the_control_rate() {
        assert!(abs(LATENCY_BUDGET_S * CONTROL_RATE_HZ - 1.0) < 1e-15);
        assert!(abs(LATENCY_BUDGET_S - 0.01) < 1e-15, "10 ms at 100 Hz");
    }
}
