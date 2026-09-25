//! Envelope protection: limits the pilot's pitch demand cannot pass (ADR-007).
//!
//! The safety kernel (ADR-005) decides **whether** the surfaces may take a new
//! command: modes, guarded discretes, watchdog. Nothing decided **what** a crewed
//! pilot may ask for. This is that layer, first cut, pitch axis only, because the
//! pitch loop is the only loop M10 has.
//!
//! # The protection law
//!
//! Over a limit, a nose-up **floor** is imposed on the pitch demand. Climbing is
//! how a ramjet aircraft at constant thrust sheds both dynamic pressure and Mach:
//! it trades speed for altitude. The pilot may still ask for more nose-up than the
//! floor; the floor only removes the nose-down half of the stick.
//!
//! ```text
//!   floor = gain * max(q/q_max - 1, M/M_max - 1)        clamped to the command limit
//!   protected = max(pilot_demand, floor)                only while a limit is exceeded
//! ```
//!
//! Inside the envelope the demand passes **bit for bit** unchanged; a protection
//! layer that nudges normal flying is a second pilot, not a limiter.
//!
//! # What it is not
//!
//! - Not a thrust or throttle limiter: M10 has no propulsion loop.
//! - Not a load-factor (g) limiter: M10 has no normal-acceleration input.
//! - Not certified, and not tuned against a vehicle model. The gain is declared.
//!
//! # Totality
//!
//! Bad air data (NaN, altitude outside the atmosphere) turns protection **off**
//! and says so in [`Protection::Unavailable`]. The existing gate already holds the
//! last command on such a frame; protection must not invent a demand from data
//! the loop itself has refused.

use crate::{air_data, AirData};

/// Dynamic-pressure limit [Pa]. **[DECLARED]** from the representative climb
/// schedule in `docs/design-point.md`, where q peaks at 21.01 kPa at M 2.0
/// ("q still peaks in the corridor, not at cruise"). That schedule is itself a
/// placeholder, so this is the loads case the repository has, not a structural
/// limit it has derived. `[TO DETERMINE]` against a loads analysis.
pub const Q_LIMIT_PA: f64 = 21_010.0;

/// Mach limit **[DECLARED]**: the proposed M 4.00 row, the fastest row at which
/// nose and leading-edge stagnation heating have been computed (Inconel 718,
/// ~50 K margin, `design-point-m4.md` must-have 4). Past it no module has said
/// the structure survives, so the stick must not take the aircraft there.
pub const MACH_LIMIT: f64 = 4.0;

/// Pitch floor per unit fractional exceedance [rad]. **[DECLARED]**, not tuned:
/// 10 % over a limit asks for 0.1 rad (5.7 deg) nose-up minimum.
pub const PROTECTION_GAIN_RAD: f64 = 1.0;

/// Symmetric pitch-demand limit the floor is clamped to [rad]. Matches the
/// cruise controller's command limit so protection cannot out-ask the surfaces.
pub const DEMAND_LIMIT_RAD: f64 = 0.35;

/// What protection did this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Protection {
    /// Inside both limits: the demand passed unchanged.
    Inside,
    /// Over a limit: a nose-up floor was imposed. `floor_rad` is the floor;
    /// `demand_rad` is what reached the loop, never below the floor.
    Limiting {
        over_q: bool,
        over_mach: bool,
        floor_rad: f64,
        demand_rad: f64,
    },
    /// Air data was not believable; protection is off for this frame and the
    /// demand passed unchanged for the gate to hold.
    Unavailable,
}

impl Protection {
    /// The pitch demand that reaches the control loop.
    #[must_use]
    pub fn demand_rad(self, pilot_demand_rad: f64) -> f64 {
        match self {
            Protection::Limiting { demand_rad, .. } => demand_rad,
            Protection::Inside | Protection::Unavailable => pilot_demand_rad,
        }
    }
}

/// Apply envelope protection to one pilot pitch demand, from air data.
#[must_use]
pub fn protect(air: &AirData, pilot_demand_rad: f64) -> Protection {
    if air.mach.is_nan() || air.dynamic_pressure_pa.is_nan() || pilot_demand_rad.is_nan() {
        return Protection::Unavailable;
    }
    let q_excess = air.dynamic_pressure_pa / Q_LIMIT_PA - 1.0;
    let mach_excess = air.mach / MACH_LIMIT - 1.0;
    let over_q = q_excess > 0.0;
    let over_mach = mach_excess > 0.0;
    if !over_q && !over_mach {
        return Protection::Inside;
    }
    let excess = if q_excess > mach_excess { q_excess } else { mach_excess };
    let floor_rad = (PROTECTION_GAIN_RAD * excess).clamp(0.0, DEMAND_LIMIT_RAD);
    let demand_rad = if pilot_demand_rad > floor_rad { pilot_demand_rad } else { floor_rad };
    Protection::Limiting { over_q, over_mach, floor_rad, demand_rad }
}

/// [`protect`] from the loop's raw inputs: altitude and true airspeed.
///
/// Air data comes from the same `ventus-atmos` path the loop and the twin use
/// (ADR-000 D1). An altitude outside the atmosphere is [`Protection::Unavailable`].
#[must_use]
pub fn protect_raw(altitude_m: f64, true_airspeed_m_s: f64, pilot_demand_rad: f64) -> Protection {
    if altitude_m.is_nan() || true_airspeed_m_s.is_nan() {
        return Protection::Unavailable;
    }
    match air_data(altitude_m, true_airspeed_m_s) {
        Ok(air) => protect(&air, pilot_demand_rad),
        Err(_) => Protection::Unavailable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn air(mach: f64, q: f64) -> AirData {
        AirData { altitude_m: 26_000.0, mach, dynamic_pressure_pa: q, static_temperature_k: 222.65 }
    }

    /// Inside the envelope the pilot's demand reaches the loop bit for bit.
    #[test]
    fn inside_the_envelope_protection_is_transparent() {
        for demand in [-0.35, -0.1, 0.0, 0.05, 0.35] {
            let p = protect(&air(3.5, 18_463.0), demand);
            assert_eq!(p, Protection::Inside);
            assert_eq!(p.demand_rad(demand).to_bits(), demand.to_bits());
        }
    }

    /// The design cruise and the M 4.00 row are inside; the limit itself is inside.
    #[test]
    fn the_design_rows_are_inside() {
        assert_eq!(protect(&air(3.5, 18_463.0), 0.0), Protection::Inside);
        assert_eq!(protect(&air(MACH_LIMIT, Q_LIMIT_PA), 0.0), Protection::Inside);
    }

    /// Over q, a nose-down stick is overridden to the floor; the floor grows with
    /// the exceedance and never passes the demand limit.
    #[test]
    fn over_q_the_pilot_cannot_push_the_nose_down() {
        let Protection::Limiting { over_q, over_mach, floor_rad, demand_rad } =
            protect(&air(2.0, Q_LIMIT_PA * 1.10), -0.2)
        else {
            panic!("expected limiting");
        };
        assert!(over_q && !over_mach);
        assert!((floor_rad - 0.10).abs() < 1e-12, "floor {floor_rad}");
        assert_eq!(demand_rad, floor_rad);

        let mut last = 0.0;
        for pct in [1.0, 5.0, 20.0, 60.0, 300.0] {
            let p = protect(&air(2.0, Q_LIMIT_PA * (1.0 + pct / 100.0)), -1.0);
            let Protection::Limiting { floor_rad, .. } = p else { panic!("expected limiting") };
            assert!(floor_rad >= last, "floor not monotonic at +{pct} %");
            assert!(floor_rad <= DEMAND_LIMIT_RAD);
            last = floor_rad;
        }
        assert_eq!(last, DEMAND_LIMIT_RAD);
    }

    /// More nose-up than the floor is still the pilot's to ask for.
    #[test]
    fn over_a_limit_more_nose_up_than_the_floor_passes() {
        let p = protect(&air(4.2, 18_000.0), 0.3);
        let Protection::Limiting { over_mach, floor_rad, demand_rad, .. } = p else {
            panic!("expected limiting")
        };
        assert!(over_mach);
        assert!(floor_rad < 0.3);
        assert_eq!(demand_rad, 0.3);
    }

    /// Whichever limit is exceeded more sets the floor.
    #[test]
    fn the_larger_exceedance_sets_the_floor() {
        let Protection::Limiting { floor_rad, over_q, over_mach, .. } =
            protect(&air(MACH_LIMIT * 1.02, Q_LIMIT_PA * 1.05), 0.0)
        else {
            panic!("expected limiting")
        };
        assert!(over_q && over_mach);
        assert!((floor_rad - 0.05).abs() < 1e-12, "floor {floor_rad}");
    }

    /// Air data the loop would refuse turns protection off rather than inventing
    /// a demand from it.
    #[test]
    fn bad_air_data_is_unavailable_not_guessed() {
        assert_eq!(protect(&air(f64::NAN, 18_000.0), 0.0), Protection::Unavailable);
        assert_eq!(protect(&air(3.5, f64::NAN), 0.0), Protection::Unavailable);
        assert_eq!(protect(&air(3.5, 18_000.0), f64::NAN), Protection::Unavailable);
        assert_eq!(protect_raw(f64::NAN, 1000.0, 0.0), Protection::Unavailable);
        assert_eq!(protect_raw(500_000.0, 1000.0, 0.0), Protection::Unavailable);
    }

    /// From raw inputs, the design cruise (26 km, 1046.95 m/s) is inside; the same
    /// airspeed 6 km lower is over q.
    #[test]
    fn raw_inputs_use_the_shared_atmosphere() {
        assert_eq!(protect_raw(26_000.0, 1046.95, 0.0), Protection::Inside);
        match protect_raw(20_000.0, 1046.95, 0.0) {
            Protection::Limiting { over_q, .. } => assert!(over_q),
            other => panic!("expected over-q limiting at 20 km, got {other:?}"),
        }
    }
}
