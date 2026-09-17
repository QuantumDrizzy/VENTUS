//! M12 - where the chain stops answering.
//!
//! Every other module answers a question at ONE flight condition. This one asks
//! all of them the same question across a Mach sweep and records, per module,
//! **the Mach at which it refuses**.
//!
//! # The rule this module lives by
//!
//! **It computes no physics of its own and it extrapolates nothing.** Every
//! limit reported here is a module declining to answer, through a refusal that
//! is already implemented and already tested somewhere else in this workspace:
//!
//! - `gamma_air` refuses outside its fitted 273-1800 K range
//! - `ideal_ramjet` refuses once ram total temperature reaches the burner limit
//! - `lightest_survivor` returns `None` when no candidate material survives
//! - `at_geopotential` refuses above the top of the US76 model
//! - `shock_train` returns negative infinity for a detached ramp
//!
//! So the envelope is not an opinion about high-speed flight. It is this
//! project's own modules saying where they stop, collected in one place.
//!
//! # Why that is the interesting output
//!
//! The tempting thing to build is "VENTUS at Mach 5". It would be worthless: at
//! M 5 the ramjet is gone, the gas is dissociating, the titanium is gone, and
//! there is no public vehicle to check any of it against. A number produced
//! there would look exactly like the numbers produced at M 3.5 and mean nothing.
//!
//! What can be produced honestly is the **boundary** - and a boundary computed
//! from refusals is a stronger claim than a value computed from extrapolations,
//! because every point on it is something a module knows about itself.
//!
//! # The altitude schedule
//!
//! Constant dynamic pressure, 18.463 kPa. That is not an arbitrary choice: it is
//! how this project picked 26 km in the first place (`docs/design-point.md` 0),
//! holding the structural loads case fixed so that changing Mach changes the
//! thermal and propulsive problems and nothing else. Extending the same rule
//! across the sweep keeps every point comparable to the design point.

#![no_std]
#![forbid(unsafe_code)]

#[cfg(test)]
extern crate std;

#[cfg(test)]
mod tests;

use ventus_aero::boundary_layer::{EdgeState, Regime, PRANDTL_AIR};

/// The design-point dynamic pressure, held constant across the sweep.
/// `docs/design-point.md` 2.
pub const DESIGN_DYNAMIC_PRESSURE_PA: f64 = 18_463.0;

/// Burner exit total temperature limit, from M4. The number that ends the
/// ramjet.
pub const BURNER_EXIT_LIMIT_K: f64 = 1700.0;

/// Skin emissivity used for the radiation balance, from M5.
pub const SKIN_EMISSIVITY: f64 = 0.85;

/// Running length at which the skin temperature is evaluated, from M5's design
/// point table.
pub const SKIN_STATION_M: f64 = 10.0;

/// Ramp count of the design inlet, from M3: three fall short of the
/// MIL-E-5008B target at M 3.5 and four clear it.
pub const DESIGN_RAMP_COUNT: usize = 4;

/// Peak ideal-ramjet specific thrust over the sweep, at M 2.30.
///
/// Computed by this module and pinned as a constant so
/// [`Point::ramjet_isp_is_meaningful`] does not have to re-run the sweep to
/// answer a question about one point. `the_peak_specific_thrust_is_where_it_is`
/// asserts it still holds.
pub const PEAK_SPECIFIC_THRUST_N_S_KG: f64 = 723.4;

/// Mach at which specific thrust peaks. **The Mach this engine wants**, as
/// opposed to the M 3.50 it is being asked to fly.
pub const PEAK_SPECIFIC_THRUST_MACH: f64 = 2.30;

/// Below this fraction of peak specific thrust, the specific-impulse number
/// stops meaning anything. A declared judgement, not a physical boundary.
pub const USEFUL_THRUST_FRACTION: f64 = 0.25;

/// **[KNOWN_LIMIT] The cycle has no flame stability model.**
///
/// `ideal_ramjet` happily runs at a fuel-air ratio of 0.0006, which is an
/// equivalence ratio near 0.01. No combustor sustains that: kerosene-air lean
/// blowout is somewhere around an equivalence ratio of 0.4, meaning f/a of
/// roughly 0.027 **[TO CITE]**.
///
/// [CORRECTED] This first said the crossing was "around M 4.3", estimated rather
/// than run. Measured, the sweep passes f/a = 0.027 at **M 3.91** - only 0.41
/// Mach above the design point, and 1.8 Mach below the 1700 K ceiling the
/// envelope reports.
///
/// So the M4 refusal here is a **ceiling, not the real limit**, and the real
/// limit is far earlier and close enough to the design point to matter. M12
/// surfaced this; M4 does not model it.
pub const LEAN_BLOWOUT_CROSSING_MACH: f64 = 3.91;

/// See [`LEAN_BLOWOUT_CROSSING_MACH`].
pub const FLAME_STABILITY_NOT_MODELLED: &str =
    "ideal_ramjet has no lean blowout limit; the M4 refusal is a ceiling, not the real end";

// ---------------------------------------------------------------------------
// Relations between the declared constants, as BUILD-TIME assertions.
//
// These compare constants, so a runtime test over them is `assert!(true)` after
// constant folding and checks nothing. As `const _: ()` they break the build,
// which is what a relation that must always hold deserves. Same treatment as the
// mass bounds in M7.
// ---------------------------------------------------------------------------

/// The engine's best point is BELOW the Mach the aircraft is designed for.
/// If this ever inverts, the finding this module reports has gone away.
const _: () = assert!(
    PEAK_SPECIFIC_THRUST_MACH < 3.5,
    "specific thrust no longer peaks below the design point"
);

/// The unmodelled lean limit bites close to the design point and far below the
/// burner ceiling. Both halves matter: close enough to be a design problem,
/// early enough that the reported M4 ceiling is not the real limit.
const _: () = assert!(
    LEAN_BLOWOUT_CROSSING_MACH > 3.5
        && LEAN_BLOWOUT_CROSSING_MACH - 3.5 < 0.5
        && LEAN_BLOWOUT_CROSSING_MACH < 5.0,
    "the lean blowout crossing moved away from the design point"
);

/// A fraction, not a multiple.
const _: () = assert!(USEFUL_THRUST_FRACTION > 0.0 && USEFUL_THRUST_FRACTION < 1.0);

/// Why a module stopped answering.
///
/// Every variant corresponds to a refusal implemented in the module named, not
/// to a judgement made here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// M1: the flight condition needs an altitude above the top of US76.
    AtmosphereModelTop,
    /// M2: `gamma_air` declined to extrapolate its cp correlation.
    GasModelOutOfRange,
    /// M3: no ramp schedule keeps the shocks attached.
    InletShocksDetached,
    /// M4: ram total temperature has reached the burner limit, so the cycle can
    /// add no heat. This is the ramjet ending, not a numerical failure.
    RamjetThermallyChoked,
    /// M5: no candidate material survives the radiating wall temperature.
    NoMaterialSurvives,
}

impl Refusal {
    #[must_use]
    pub fn module(self) -> &'static str {
        match self {
            Refusal::AtmosphereModelTop => "M1 atmos",
            Refusal::GasModelOutOfRange => "M2 gasdyn",
            Refusal::InletShocksDetached => "M3 inlet",
            Refusal::RamjetThermallyChoked => "M4 propulsion",
            Refusal::NoMaterialSurvives => "M5 thermal",
        }
    }

    /// The refusal in the module's own terms, for a report a reader can check.
    #[must_use]
    pub fn reason(self) -> &'static str {
        match self {
            Refusal::AtmosphereModelTop => {
                "holding q needs an altitude above the top of the US76 model"
            }
            Refusal::GasModelOutOfRange => {
                "gamma_air refuses: the stagnation temperature left the 273-1800 K fit"
            }
            Refusal::InletShocksDetached => "no ramp schedule keeps the shocks attached",
            Refusal::RamjetThermallyChoked => {
                "ram total temperature reached the 1700 K burner limit; the cycle can add no heat"
            }
            Refusal::NoMaterialSurvives => {
                "the radiating wall exceeds every candidate material's sustained limit"
            }
        }
    }
}

/// What the chain could answer at one Mach number.
///
/// A field is `None` exactly when the module that produces it refused. Nothing
/// here is filled in by a fallback.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub mach: f64,
    /// Geopotential altitude that holds the design dynamic pressure.
    pub altitude_m: Option<f64>,
    pub static_temperature_k: Option<f64>,
    pub velocity_m_s: Option<f64>,
    /// Stagnation temperature, calorically perfect at gamma = 1.4.
    pub stagnation_temperature_k: Option<f64>,
    /// gamma evaluated at the stagnation temperature. `None` when M2 refuses.
    pub gamma_at_stagnation: Option<f64>,
    /// Inlet total-pressure recovery at [`DESIGN_RAMP_COUNT`] ramps.
    pub inlet_recovery: Option<f64>,
    /// Specific impulse. **Read the caveat before using this.**
    ///
    /// As the ram total temperature approaches the burner limit the cycle can
    /// add less and less heat, so the fuel-air ratio falls to zero faster than
    /// the thrust does. `Isp = F / (m_dot g)` therefore DIVERGES just before the
    /// engine dies: this sweep returns 4020 s at M 5.50 and 12 674 s at M 5.62,
    /// against roughly 1900 s at the design point. Those are arithmetically
    /// correct and physically meaningless - an engine producing almost nothing,
    /// very efficiently.
    ///
    /// [`Point::ramjet_specific_thrust_n_s_kg`] is the honest quantity and the
    /// one the reports print. Use [`Point::ramjet_isp_is_meaningful`] before
    /// quoting this.
    pub ramjet_specific_impulse_s: Option<f64>,
    /// Specific thrust, which collapses monotonically and is what actually ends
    /// the ramjet. Peaks at M 2.30 and is already down to 83.7 % of peak at the
    /// M 3.50 design point.
    pub ramjet_specific_thrust_n_s_kg: Option<f64>,
    /// Radiation-equilibrium wall temperature at [`SKIN_STATION_M`].
    pub wall_temperature_k: Option<f64>,
    /// Lightest material whose sustained limit clears the wall temperature.
    pub lightest_material: Option<&'static str>,
    /// Every module that declined at this Mach.
    pub refusals: [Option<Refusal>; 5],
}

impl Point {
    #[must_use]
    pub fn refused(&self, r: Refusal) -> bool {
        self.refusals.iter().flatten().any(|&x| x == r)
    }

    #[must_use]
    pub fn refusal_count(&self) -> usize {
        self.refusals.iter().flatten().count()
    }

    /// Whether [`Point::ramjet_specific_impulse_s`] means anything here.
    ///
    /// False once specific thrust has fallen below [`USEFUL_THRUST_FRACTION`] of
    /// its peak, because past that the Isp divergence dominates the number.
    ///
    /// **The threshold is a declared engineering judgement, not a physical
    /// boundary**, which is why it is a named constant rather than a magic
    /// number and why this returns a flag rather than silently blanking the
    /// field.
    #[must_use]
    pub fn ramjet_isp_is_meaningful(&self) -> bool {
        self.ramjet_specific_thrust_n_s_kg
            .is_some_and(|fs| fs >= USEFUL_THRUST_FRACTION * PEAK_SPECIFIC_THRUST_N_S_KG)
    }

    fn record(&mut self, r: Refusal) {
        for slot in &mut self.refusals {
            if slot.is_none() {
                *slot = Some(r);
                return;
            }
        }
    }
}

/// Geopotential altitude that holds `q` at this Mach, by bisection on US76.
///
/// `q = 0.7 p M^2` for a perfect gas, so the target is a static pressure. US76
/// pressure is monotonically decreasing in altitude, which is asserted by M1's
/// own tests, so bisection is safe and terminates in a fixed iteration count.
///
/// Returns `None` when the required pressure lies above the top of the model -
/// which is M1 refusing, and is reported as such.
#[must_use]
pub fn altitude_for_constant_q_m(mach: f64, dynamic_pressure_pa: f64) -> Option<f64> {
    if mach.is_nan() || mach <= 0.0 || dynamic_pressure_pa.is_nan() || dynamic_pressure_pa <= 0.0 {
        return None;
    }
    let target_pressure_pa = dynamic_pressure_pa / (0.7 * mach * mach);

    let top = ventus_atmos::layers::TOP_GEOPOTENTIAL_ALTITUDE_M;
    let at_top = ventus_atmos::at_geopotential(top).ok()?;
    if target_pressure_pa < at_top.pressure_pa {
        // Holding q would need to be higher than the model goes.
        return None;
    }
    let sea_level = ventus_atmos::at_geopotential(0.0).ok()?;
    if target_pressure_pa > sea_level.pressure_pa {
        return None;
    }

    let (mut lo, mut hi) = (0.0_f64, top);
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        let p = ventus_atmos::at_geopotential(mid).ok()?.pressure_pa;
        if p > target_pressure_pa {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    Some(0.5 * (lo + hi))
}

/// Ask every module about one Mach number.
///
/// Downstream questions are still asked when an upstream one refused, wherever
/// they can be: the point of the sweep is to see each module's own boundary, not
/// to stop at the first.
#[must_use]
pub fn evaluate(mach: f64) -> Point {
    let mut p = Point {
        mach,
        altitude_m: None,
        static_temperature_k: None,
        velocity_m_s: None,
        stagnation_temperature_k: None,
        gamma_at_stagnation: None,
        inlet_recovery: None,
        ramjet_specific_impulse_s: None,
        ramjet_specific_thrust_n_s_kg: None,
        wall_temperature_k: None,
        lightest_material: None,
        refusals: [None; 5],
    };

    // M1.
    let Some(altitude_m) = altitude_for_constant_q_m(mach, DESIGN_DYNAMIC_PRESSURE_PA) else {
        p.record(Refusal::AtmosphereModelTop);
        return p;
    };
    let Ok(atmos) = ventus_atmos::at_geopotential(altitude_m) else {
        p.record(Refusal::AtmosphereModelTop);
        return p;
    };
    p.altitude_m = Some(altitude_m);
    p.static_temperature_k = Some(atmos.temperature_k);
    let velocity_m_s = mach * atmos.speed_of_sound_m_s;
    p.velocity_m_s = Some(velocity_m_s);

    // M2. The calorically perfect stagnation temperature always exists; whether
    // the gas model is still valid there is the question.
    let gamma = 1.4;
    if let Ok(ratio) = ventus_gasdyn::stagnation_temperature_ratio(mach, gamma) {
        let t0 = atmos.temperature_k * ratio;
        p.stagnation_temperature_k = Some(t0);
        match ventus_gasdyn::gamma_air(t0) {
            Ok(g) => p.gamma_at_stagnation = Some(g),
            Err(_) => p.record(Refusal::GasModelOutOfRange),
        }
    }

    // M3, at the DESIGN ramp count. M3's own cases establish that three ramps
    // fall short of the MIL-E-5008B target at M 3.5 and four clear it, so four
    // is the configuration this aircraft has, and asking what the design inlet
    // does at other Mach numbers is the question the envelope needs.
    //
    // Sweeping ramp counts here was the first implementation. It cost four
    // optimiser runs per point at 20-260 ms each, which made the sweep slower
    // than every other module in this workspace combined, for an answer that
    // described a different inlet at every Mach.
    let best_recovery =
        match ventus_inlet::shock_train::optimise_ramps(mach, DESIGN_RAMP_COUNT, gamma) {
            Ok((_, train)) if train.total_recovery.is_finite() && train.total_recovery > 0.0 => {
                Some(train.total_recovery)
            }
            _ => None,
        };
    match best_recovery {
        Some(r) => p.inlet_recovery = Some(r),
        None => p.record(Refusal::InletShocksDetached),
    }

    // M4. Needs a recovery to run at all, so it inherits M3's refusal.
    if let Some(recovery) = best_recovery {
        match ventus_propulsion::ramjet::ideal_ramjet(
            mach,
            atmos.temperature_k,
            atmos.pressure_pa,
            velocity_m_s,
            recovery,
            BURNER_EXIT_LIMIT_K,
            gamma,
        ) {
            Ok(cycle) => {
                p.ramjet_specific_impulse_s = Some(cycle.specific_impulse_s);
                p.ramjet_specific_thrust_n_s_kg = Some(cycle.specific_thrust_n_s_kg);
            }
            Err(_) => p.record(Refusal::RamjetThermallyChoked),
        }
    }

    // M5.
    let edge = EdgeState {
        temperature_k: atmos.temperature_k,
        pressure_pa: atmos.pressure_pa,
        velocity_m_s,
        mach,
        gamma,
    };
    if let Ok(balance) = ventus_thermal::radiation_equilibrium_wall(
        &edge,
        SKIN_STATION_M,
        SKIN_EMISSIVITY,
        0.0,
        PRANDTL_AIR,
        Regime::Turbulent,
    ) {
        p.wall_temperature_k = Some(balance.wall_temperature_k);
        match ventus_thermal::lightest_survivor(balance.wall_temperature_k) {
            Some(m) => p.lightest_material = Some(m.name),
            None => p.record(Refusal::NoMaterialSurvives),
        }
    }

    p
}

/// One pass over the Mach range, evaluated once per point.
///
/// `f` is called with every point in ascending Mach order. A callback rather
/// than a returned collection because this crate is `no_std` without `alloc`,
/// and because the sweep is expensive enough that materialising it twice would
/// be noticeable: M3's ramp optimiser dominates the cost at roughly a quarter of
/// a second per point, which is why this exists instead of one scan per module.
pub fn sweep(from_mach: f64, to_mach: f64, resolution: f64, mut f: impl FnMut(&Point)) {
    if resolution.is_nan() || resolution <= 0.0 || to_mach <= from_mach {
        return;
    }
    let steps = ((to_mach - from_mach) / resolution) as usize;
    for i in 0..=steps {
        let m = from_mach + resolution * i as f64;
        f(&evaluate(m));
    }
}

/// Where each module first declines, and the highest Mach at which none of them
/// does, from a single pass.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Envelope {
    /// First refusing Mach per module, in the order of [`Envelope::ORDER`].
    pub first_refusal_mach: [Option<f64>; 5],
    /// Highest Mach at which every module still answered.
    pub last_fully_answered_mach: Option<f64>,
}

impl Envelope {
    /// The refusals tracked, in the order [`Envelope::first_refusal_mach`] uses.
    pub const ORDER: [Refusal; 5] = [
        Refusal::AtmosphereModelTop,
        Refusal::GasModelOutOfRange,
        Refusal::InletShocksDetached,
        Refusal::RamjetThermallyChoked,
        Refusal::NoMaterialSurvives,
    ];

    #[must_use]
    pub fn first_refusal(&self, r: Refusal) -> Option<f64> {
        let i = Self::ORDER.iter().position(|&x| x == r)?;
        self.first_refusal_mach[i]
    }
}

/// Compute the envelope over a Mach range.
///
/// Scans upward rather than bisecting: a refusal is not guaranteed monotonic in
/// Mach, and bisection would quietly assume it is.
#[must_use]
pub fn envelope(from_mach: f64, to_mach: f64, resolution: f64) -> Envelope {
    let mut e = Envelope::default();
    let mut still_clean = true;
    sweep(from_mach, to_mach, resolution, |p| {
        for (i, r) in Envelope::ORDER.iter().enumerate() {
            if e.first_refusal_mach[i].is_none() && p.refused(*r) {
                e.first_refusal_mach[i] = Some(p.mach);
            }
        }
        if still_clean {
            if p.refusal_count() == 0 {
                e.last_fully_answered_mach = Some(p.mach);
            } else {
                still_clean = false;
            }
        }
    });
    e
}
