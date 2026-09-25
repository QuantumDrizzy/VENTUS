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
//! - **A safety kernel beside the loop, not inside it** (ADR-005). Modes,
//!   guarded discretes, and a software watchdog produce a gate on whether
//!   [`step`] may write a new surface command. That is the first cut of
//!   emergency logic. It is not certification and not a cockpit.
//! - **One HIL wire protocol** (ADR-004), in this crate, not beside it. The
//!   Nucleo firmware and the host gate both call [`hil::handle`]. Safety
//!   commands tick [`SafetyKernel`] / [`gated_step`]; they do not invent a
//!   second mode table. GPIO discrete sampling is not bound.
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

pub mod envelope;
pub mod hil;
pub mod safety;

use ventus_atmos::AtmosError;

pub use safety::{
    resolve_sample, ChannelPolicy, ControlGate, CriticalDiscrete, DisagreeAction, DiscreteFrame,
    DiscreteKind, DualSample, FlightMode, Heartbeat, ModeEvent, SafetyInputs, SafetyKernel,
    SurfaceAuthority, Watchdog, WatchdogLevel, BIT_PASS_FRAMES, BIT_TIMEOUT_FRAMES,
    DISCRETE_CONFIRM_FRAMES, EMERGENCY_TO_SAFE_FRAMES, WATCHDOG_DEGRADED_MISSES,
    WATCHDOG_SAFE_MISSES,
};

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

impl Health {
    /// Wire tag: 0 Nominal, 1 AirDataInvalid, 2 SensorFault. HIL SAFETY_TICK
    /// request byte 2 and GATED_STEP reply health byte (ADR-004 D6).
    #[must_use]
    pub const fn to_wire(self) -> u8 {
        match self {
            Health::Nominal => 0,
            Health::AirDataInvalid => 1,
            Health::SensorFault => 2,
        }
    }

    /// Inverse of [`Self::to_wire`]. Unknown -> `None`, never a silent Nominal.
    #[must_use]
    pub const fn from_wire(raw: u8) -> Option<Self> {
        match raw {
            0 => Some(Health::Nominal),
            1 => Some(Health::AirDataInvalid),
            2 => Some(Health::SensorFault),
            _ => None,
        }
    }
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

/// Classify this frame's air-data / sensor inputs without commanding.
///
/// Same totality as [`step`]: NaN is [`Health::SensorFault`], altitude outside
/// the atmosphere domain is [`Health::AirDataInvalid`]. The safety kernel
/// reads this so a Degraded vehicle still reports *why* the loop does not
/// believe the sample, without writing a new surface command.
#[must_use]
pub fn input_health(
    altitude_m: f64,
    true_airspeed_m_s: f64,
    commanded_pitch_rad: f64,
    measured_pitch_rad: f64,
) -> Health {
    if altitude_m.is_nan()
        || true_airspeed_m_s.is_nan()
        || commanded_pitch_rad.is_nan()
        || measured_pitch_rad.is_nan()
    {
        return Health::SensorFault;
    }
    match air_data(altitude_m, true_airspeed_m_s) {
        Ok(_) => Health::Nominal,
        Err(_) => Health::AirDataInvalid,
    }
}

/// One control frame presented to [`gated_step`]: sensors plus the safety
/// kernel's inputs. `Copy`, fixed size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ControlFrame {
    pub altitude_m: f64,
    pub true_airspeed_m_s: f64,
    pub commanded_pitch_rad: f64,
    pub measured_pitch_rad: f64,
    pub discretes: DiscreteFrame,
    pub heartbeat: Heartbeat,
    pub bit_clear: bool,
}

/// Result of [`gated_step`]: the loop state the surfaces should see, and the
/// gate that decided whether [`step`] was allowed to write it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GatedStep {
    pub loop_state: LoopState,
    pub gate: ControlGate,
}

/// One control step, gated by [`SafetyKernel`].
///
/// Composition, not a replacement: [`step`] is still the pitch PI. This
/// function classifies inputs, ticks the kernel, and either calls [`step`]
/// (Nominal) or holds the last command and integrator (every other mode).
/// Hold-last on bad air data still holds when the kernel has already said
/// Degraded — two fail-safes, one actuator.
#[must_use]
pub fn gated_step(
    kernel: &mut SafetyKernel,
    controller: &Controller,
    state: LoopState,
    frame: ControlFrame,
) -> GatedStep {
    let health = input_health(
        frame.altitude_m,
        frame.true_airspeed_m_s,
        frame.commanded_pitch_rad,
        frame.measured_pitch_rad,
    );
    let gate = kernel.tick(SafetyInputs {
        discretes: frame.discretes,
        heartbeat: frame.heartbeat,
        bit_clear: frame.bit_clear,
        loop_health: health,
    });

    let loop_state = if gate.allows_surface_command() {
        step(
            controller,
            state,
            frame.altitude_m,
            frame.true_airspeed_m_s,
            frame.commanded_pitch_rad,
            frame.measured_pitch_rad,
        )
    } else {
        let mut next = state;
        next.steps = state.steps.wrapping_add(1);
        next.health = health;
        next
    };

    GatedStep { loop_state, gate }
}

/// Result of [`protected_gated_step`]: the gated step, plus what envelope
/// protection did to the pilot's demand before the loop saw it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProtectedStep {
    pub step: GatedStep,
    pub protection: envelope::Protection,
}

/// [`gated_step`] with envelope protection on the pilot's pitch demand (ADR-007).
///
/// Composition again: protection rewrites `commanded_pitch_rad` only while a
/// limit is exceeded, then [`gated_step`] runs unchanged. Inside the envelope
/// the result is bit-for-bit [`gated_step`]'s, so the HIL wire (ADR-004), which
/// calls [`gated_step`], is not moved by this layer.
#[must_use]
pub fn protected_gated_step(
    kernel: &mut SafetyKernel,
    controller: &Controller,
    state: LoopState,
    frame: ControlFrame,
) -> ProtectedStep {
    let protection =
        envelope::protect_raw(frame.altitude_m, frame.true_airspeed_m_s, frame.commanded_pitch_rad);
    let mut protected_frame = frame;
    protected_frame.commanded_pitch_rad = protection.demand_rad(frame.commanded_pitch_rad);
    ProtectedStep { step: gated_step(kernel, controller, state, protected_frame), protection }
}

/// One control step.
///
/// Total: every input path produces a defined output, including NaN and
/// out-of-domain altitude. Nothing here can panic, allocate, or run for an
/// unbounded time. This function does **not** consult the safety kernel; use
/// [`gated_step`] when modes and discretes are in play.
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

    match input_health(
        altitude_m,
        true_airspeed_m_s,
        commanded_pitch_rad,
        measured_pitch_rad,
    ) {
        health @ (Health::SensorFault | Health::AirDataInvalid) => {
            next.health = health;
            return next; // hold the last command
        }
        Health::Nominal => {}
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

    fn healthy_frame(commanded: f64, measured: f64) -> ControlFrame {
        ControlFrame {
            altitude_m: CRUISE_ALT,
            true_airspeed_m_s: CRUISE_TAS,
            commanded_pitch_rad: commanded,
            measured_pitch_rad: measured,
            discretes: DiscreteFrame::fail_safe(),
            heartbeat: Heartbeat::PRESENT,
            bit_clear: true,
        }
    }

    fn arm_frame(commanded: f64, measured: f64) -> ControlFrame {
        ControlFrame {
            discretes: DiscreteFrame {
                master_arm: DualSample::both(true),
                ..DiscreteFrame::fail_safe()
            },
            ..healthy_frame(commanded, measured)
        }
    }

    fn drive_gated_to_nominal(
        kernel: &mut SafetyKernel,
        controller: &Controller,
        mut state: LoopState,
    ) -> LoopState {
        for _ in 0..16 {
            if kernel.mode() == FlightMode::Ready {
                break;
            }
            let g = gated_step(kernel, controller, state, healthy_frame(0.0, 0.0));
            state = g.loop_state;
        }
        assert_eq!(kernel.mode(), FlightMode::Ready);
        for _ in 0..4 {
            if kernel.mode() == FlightMode::Nominal {
                break;
            }
            let g = gated_step(kernel, controller, state, arm_frame(0.05, 0.0));
            state = g.loop_state;
        }
        assert_eq!(kernel.mode(), FlightMode::Nominal);
        state
    }

    /// Degraded freezes the last command even when the pitch error would have
    /// produced a new one. If this fails, the kernel is reporting Degraded
    /// while `step` is still writing surfaces.
    #[test]
    fn degraded_holds_last_command_while_the_pitch_error_would_move() {
        let c = Controller::ventus1_pitch();
        let mut kernel = SafetyKernel::new();
        let mut state = drive_gated_to_nominal(&mut kernel, &c, LoopState::default());

        // Establish a non-zero command under Nominal.
        for _ in 0..20 {
            let g = gated_step(&mut kernel, &c, state, arm_frame(0.05, 0.0));
            state = g.loop_state;
            assert_eq!(g.gate.mode, FlightMode::Nominal);
        }
        assert!(
            state.last_command_rad > 0.0,
            "setup must have a real command to hold"
        );

        let mute = ControlFrame {
            heartbeat: Heartbeat::MISSING,
            commanded_pitch_rad: -0.2,
            measured_pitch_rad: 0.0,
            discretes: DiscreteFrame {
                master_arm: DualSample::both(true),
                ..DiscreteFrame::fail_safe()
            },
            ..healthy_frame(-0.2, 0.0)
        };
        let mut held = state.last_command_rad;
        let mut held_integral = state.integral;
        for _ in 0..WATCHDOG_DEGRADED_MISSES {
            let g = gated_step(&mut kernel, &c, state, mute);
            if g.gate.mode == FlightMode::Nominal {
                held = g.loop_state.last_command_rad;
                held_integral = g.loop_state.integral;
            }
            state = g.loop_state;
        }
        assert_eq!(kernel.mode(), FlightMode::Degraded);
        assert_eq!(
            state.last_command_rad.to_bits(),
            held.to_bits(),
            "the Degraded entry frame must freeze the last Nominal command"
        );
        // Stay inside the Degraded miss band; the Safe trip is a different test.
        for _ in 0..(WATCHDOG_SAFE_MISSES - WATCHDOG_DEGRADED_MISSES - 1) {
            let g = gated_step(&mut kernel, &c, state, mute);
            state = g.loop_state;
            assert_eq!(g.gate.mode, FlightMode::Degraded);
            assert_eq!(
                state.last_command_rad.to_bits(),
                held.to_bits(),
                "Degraded must HOLD the last command, not reverse it toward the new demand"
            );
            assert_eq!(
                state.integral.to_bits(),
                held_integral.to_bits(),
                "Degraded must freeze the integrator too, or the loop winds while frozen"
            );
        }
        assert_eq!(state.health, Health::Nominal, "sensors were still good");
    }

    /// Confirmed abort freezes the command that was flying. A silent drop of
    /// the abort assert would keep producing PI output toward the demand.
    #[test]
    fn abort_holds_the_command_that_was_flying() {
        let c = Controller::ventus1_pitch();
        let mut kernel = SafetyKernel::new();
        let mut state = drive_gated_to_nominal(&mut kernel, &c, LoopState::default());
        for _ in 0..20 {
            let g = gated_step(&mut kernel, &c, state, arm_frame(0.05, 0.0));
            state = g.loop_state;
        }
        let abort_debounce = ControlFrame {
            discretes: DiscreteFrame {
                abort: DualSample::both(true),
                master_arm: DualSample::both(true),
                ..DiscreteFrame::fail_safe()
            },
            ..arm_frame(0.05, 0.0)
        };
        let g1 = gated_step(&mut kernel, &c, state, abort_debounce);
        assert_eq!(
            g1.gate.mode,
            FlightMode::Nominal,
            "debounce: one abort sample must not yet take"
        );
        let held = g1.loop_state.last_command_rad;
        assert!(
            held > 0.0,
            "the last Nominal command must be a real deflection"
        );
        let abort_reverse = ControlFrame {
            commanded_pitch_rad: -0.2,
            measured_pitch_rad: 0.0,
            discretes: DiscreteFrame {
                abort: DualSample::both(true),
                master_arm: DualSample::both(true),
                ..DiscreteFrame::fail_safe()
            },
            ..healthy_frame(-0.2, 0.0)
        };
        let g2 = gated_step(&mut kernel, &c, g1.loop_state, abort_reverse);
        assert_eq!(g2.gate.mode, FlightMode::Emergency);
        assert_eq!(
            g2.loop_state.last_command_rad.to_bits(),
            held.to_bits(),
            "abort must freeze the flying command — if this fails, abort is being ignored"
        );
        assert!(!g2.gate.allows_surface_command());
        let g3 = gated_step(&mut kernel, &c, g2.loop_state, abort_reverse);
        assert_eq!(g3.gate.mode, FlightMode::Safe);
        assert_eq!(g3.loop_state.last_command_rad.to_bits(), held.to_bits());
    }

    /// ADR-007: inside the envelope, protection changes nothing, bit for bit.
    #[test]
    fn protected_step_is_gated_step_inside_the_envelope() {
        let c = Controller::ventus1_pitch();
        let mut k1 = SafetyKernel::new();
        let mut k2 = SafetyKernel::new();
        let mut s1 = drive_gated_to_nominal(&mut k1, &c, LoopState::default());
        let mut s2 = drive_gated_to_nominal(&mut k2, &c, LoopState::default());
        for i in 0..200 {
            let commanded = 0.02 * libm::sin(f64::from(i) * 0.01);
            let f = arm_frame(commanded, s1.last_command_rad * 0.1);
            let p = protected_gated_step(&mut k1, &c, s1, f);
            let g = gated_step(&mut k2, &c, s2, f);
            assert_eq!(p.protection, envelope::Protection::Inside);
            assert_eq!(p.step.loop_state.last_command_rad.to_bits(), g.loop_state.last_command_rad.to_bits());
            assert_eq!(p.step.loop_state.integral.to_bits(), g.loop_state.integral.to_bits());
            assert_eq!(p.step.gate, g.gate);
            s1 = p.step.loop_state;
            s2 = g.loop_state;
        }
    }

    /// ADR-007: over the q limit, the loop is driven by the nose-up floor, not by
    /// the pilot's nose-down stick.
    #[test]
    fn over_q_the_loop_sees_the_floor_not_the_stick() {
        let c = Controller::ventus1_pitch();
        let mut k1 = SafetyKernel::new();
        let mut k2 = SafetyKernel::new();
        let s1 = drive_gated_to_nominal(&mut k1, &c, LoopState::default());
        let s2 = drive_gated_to_nominal(&mut k2, &c, LoopState::default());
        // Cruise airspeed 6 km low: well over the q limit.
        let mut f = arm_frame(-0.2, 0.0);
        f.altitude_m = 20_000.0;
        let p = protected_gated_step(&mut k1, &c, s1, f);
        let envelope::Protection::Limiting { over_q, floor_rad, demand_rad, .. } = p.protection else {
            panic!("expected limiting, got {:?}", p.protection);
        };
        assert!(over_q);
        assert!(floor_rad > 0.0 && demand_rad == floor_rad);
        let mut floor_frame = f;
        floor_frame.commanded_pitch_rad = floor_rad;
        let g = gated_step(&mut k2, &c, s2, floor_frame);
        assert_eq!(p.step.loop_state.last_command_rad.to_bits(), g.loop_state.last_command_rad.to_bits());
        assert!(p.step.loop_state.last_command_rad > 0.0, "surface should be driving nose-up");
    }

    #[test]
    fn gated_step_is_bit_deterministic_on_the_nominal_path() {
        let c = Controller::ventus1_pitch();
        let run = || {
            let mut kernel = SafetyKernel::new();
            let mut state = drive_gated_to_nominal(&mut kernel, &c, LoopState::default());
            for i in 0..200 {
                let commanded = 0.02 * libm::sin(f64::from(i) * 0.01);
                let g = gated_step(
                    &mut kernel,
                    &c,
                    state,
                    arm_frame(commanded, state.last_command_rad * 0.1),
                );
                state = g.loop_state;
            }
            (state, kernel.gate())
        };
        let (a, ga) = run();
        let (b, gb) = run();
        assert_eq!(a.last_command_rad.to_bits(), b.last_command_rad.to_bits());
        assert_eq!(a.integral.to_bits(), b.integral.to_bits());
        assert_eq!(ga, gb);
    }

    #[test]
    fn input_health_matches_step_on_the_bad_paths() {
        let c = Controller::ventus1_pitch();
        let good = step(&c, LoopState::default(), CRUISE_ALT, CRUISE_TAS, 0.05, 0.0);
        for (alt, tas, cmd, meas, expected) in [
            (f64::NAN, CRUISE_TAS, 0.05, 0.0, Health::SensorFault),
            (CRUISE_ALT, f64::NAN, 0.05, 0.0, Health::SensorFault),
            (CRUISE_ALT, CRUISE_TAS, f64::NAN, 0.0, Health::SensorFault),
            (200_000.0, CRUISE_TAS, 0.05, 0.0, Health::AirDataInvalid),
            (-100.0, CRUISE_TAS, 0.05, 0.0, Health::AirDataInvalid),
        ] {
            assert_eq!(input_health(alt, tas, cmd, meas), expected);
            let s = step(&c, good, alt, tas, cmd, meas);
            assert_eq!(s.health, expected);
        }
        assert_eq!(
            input_health(CRUISE_ALT, CRUISE_TAS, 0.05, 0.0),
            Health::Nominal
        );
    }

    #[test]
    fn health_wire_tags_are_total_over_u8() {
        let mut known = 0u8;
        for raw in 0u8..=255 {
            match Health::from_wire(raw) {
                Some(h) => {
                    assert_eq!(h.to_wire(), raw);
                    known += 1;
                }
                None => assert!(raw > 2, "gap in health tags at {raw}"),
            }
        }
        assert_eq!(known, 3);
    }
}
