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
/// `ideal_ramjet` happily runs at a fuel-air ratio of 0.0006, an equivalence
/// ratio near 0.01. No combustor sustains that.
///
/// # Reported as a band, because the limit is a band
///
/// [CORRECTED] This first reported a single crossing, M 3.91, from a single
/// equivalence ratio of 0.4. Lean blowout in a ramjet combustor is not a point:
/// it moves with flame holder geometry, pressure and inlet preheat, over roughly
/// phi = 0.3 to 0.5 **[TO CITE]**. A point estimate invites an argument about
/// the point; a band does not, and the band here says something the point hid.
///
/// Swept against a stoichiometric f/a of [`STOICHIOMETRIC_FUEL_AIR_RATIO`]:
///
/// ```text
///   phi 0.30  ->  M 4.42
///   phi 0.35  ->  M 4.16
///   phi 0.40  ->  M 3.89
///   phi 0.45  ->  M 3.58
///   phi 0.50  ->  M 3.23   <- BELOW the M 3.50 design point
/// ```
///
/// **The design point sits inside the uncertainty band.** At the permissive end
/// the engine has 0.9 Mach of margin; at the strict end it has already blown out
/// before reaching the condition the aircraft is designed for. Which of those is
/// true is not knowable from anything in this repository.
///
/// So the M4 refusal this module reports at M 5.70 is a **ceiling far above the
/// real limit**, and closing this needs a cited blowout correlation, not a
/// better sweep.
pub const FLAME_STABILITY_NOT_MODELLED: &str =
    "ideal_ramjet has no lean blowout limit; the M4 refusal is a ceiling, not the real end";

/// Stoichiometric fuel-air ratio for kerosene in air. **[TO CITE]**
pub const STOICHIOMETRIC_FUEL_AIR_RATIO: f64 = 0.0680;

/// Permissive end of the lean blowout band: easiest to hold a flame.
pub const LEAN_BLOWOUT_PHI_MIN: f64 = 0.30;
/// Strict end of the lean blowout band.
pub const LEAN_BLOWOUT_PHI_MAX: f64 = 0.50;

/// Mach at which the cycle's fuel-air ratio falls below the blowout limit for a
/// given equivalence ratio, scanning upward from `from_mach`.
///
/// Returns `None` if the cycle never gets that lean before it refuses outright.
#[must_use]
pub fn lean_blowout_mach(
    equivalence_ratio: f64,
    from_mach: f64,
    to_mach: f64,
    resolution: f64,
) -> Option<f64> {
    if equivalence_ratio.is_nan() || equivalence_ratio <= 0.0 {
        return None;
    }
    if resolution.is_nan() || resolution <= 0.0 || to_mach <= from_mach {
        return None;
    }
    let limit = equivalence_ratio * STOICHIOMETRIC_FUEL_AIR_RATIO;
    let steps = ((to_mach - from_mach) / resolution) as usize;
    for i in 0..=steps {
        let m = from_mach + resolution * i as f64;
        let p = evaluate(m);
        let (Some(isp), Some(fs)) = (p.ramjet_specific_impulse_s, p.ramjet_specific_thrust_n_s_kg)
        else {
            continue;
        };
        let fuel_air = fs / (isp * ventus_units::constants::G0_M_S2);
        if fuel_air < limit {
            return Some(m);
        }
    }
    None
}

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

/// The blowout band is a band, and the permissive end must be the lower
/// equivalence ratio. If these ever invert, every statement about the band
/// reads backwards.
const _: () = assert!(
    LEAN_BLOWOUT_PHI_MIN > 0.0 && LEAN_BLOWOUT_PHI_MIN < LEAN_BLOWOUT_PHI_MAX,
    "the lean blowout band is inverted or non-physical"
);

/// A fraction, not a multiple.
const _: () = assert!(USEFUL_THRUST_FRACTION > 0.0 && USEFUL_THRUST_FRACTION < 1.0);

/// The thrust-balance frontiers order the way the physics requires: a body that
/// no longer FITS the inlet comes before one where no body closes at all, and
/// both sit well under the M 5.70 burner ceiling M4 reports.
///
/// [CORRECTED] This is the third time in this workspace that a relation between
/// constants was written as a runtime `assert!`, which clippy correctly calls
/// `assert!(true)` because constant folding removes it. Same fix as the M7 mass
/// bounds and the M12 thrust constants: as `const _: ()` it breaks the build.
const _: () = assert!(
    CAPTURE_AREA_CLOSES_AT_MACH < NO_BODY_CLOSES_ABOVE_MACH && NO_BODY_CLOSES_ABOVE_MACH < 5.0,
    "the thrust-balance frontiers are out of order"
);

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
    ///
    /// # Why this never fires on a constant-q sweep, as a mechanism
    ///
    /// Holding dynamic pressure means climbing, so density collapses, and the
    /// radiating wall barely moves. The reason it barely moves is the FOURTH
    /// ROOT: the balance is `eps sigma T_w^4 = h (T_aw - T_w)`, so whatever the
    /// right-hand side does is crushed by `^(1/4)` before it reaches `T_w`.
    ///
    /// **[CORRECTED] There is no single exponent here, and an earlier version of
    /// this comment gave one.** It said `T_w ~ rho^-0.21`, which is the local
    /// slope of ONE interval picked out of a set that varies by a factor of
    /// sixteen:
    ///
    /// ```text
    ///   M 4.0 -> 5.0    -0.256
    ///   M 5.0 -> 5.7    -0.212
    ///   M 5.7 -> 7.0    -0.183
    ///   M 7.0 -> 9.0    -0.144
    ///   M 9.0 -> 12.0   -0.016
    /// ```
    ///
    /// Quoting one of those as "the" exponent is the same error as quoting the
    /// textbook asymptotic value, which is what this comment had just finished
    /// correcting. A quantity that moves by 16x across the range is not an
    /// exponent, it is a trend, and the trend is what gets documented.
    ///
    /// **Two mechanisms, neither of which needs a number:**
    ///
    /// 1. The fourth root, above. It is why any of this looks flat.
    /// 2. **Eckert reference-temperature evaluation.** `film_state` takes
    ///    properties at the film temperature rather than the freestream, so as
    ///    the wall heats, the reference density falls and the viscosity rises,
    ///    and both push `h` down. Textbook freestream scaling gives
    ///    `h ~ rho^0.8 V^0.8`, hence `h ~ rho^0.4` at constant q; measured
    ///    against this model `h ~ rho^0.62`, falling faster with altitude
    ///    exactly as that mechanism predicts.
    ///
    /// The two do not compose to a clean closed form, and the composition is
    /// worth writing down because it is where a reader will try to check the
    /// arithmetic: `T_w^4 ~ h T_aw ~ rho^0.62 rho^-1 = rho^-0.38` gives
    /// `T_w ~ rho^-0.095`. Measured end to end from M 5.7 to M 12 the answer is
    /// -0.105, close to that; measured locally at the low end it is -0.26,
    /// nowhere near it, because there `T_w / T_aw` is about 0.4 and the
    /// `T_w << T_aw` assumption the composition rests on does not hold.
    ///
    /// Above about M 9 the trend breaks entirely - `T_w` goes from 835 K to
    /// 843 K between M 9 and M 12 - because the sweep has climbed into the US76
    /// stratopause warming (the 32-47 km layer rises at 2.8 K/km). That is a
    /// real feature of the atmosphere model rather than a solver artefact; the
    /// bisection bracket runs to `T_aw`, which is 7740 K there, so nothing is
    /// clamping. It is also far outside the range where the chain answers at
    /// all, so it is a curiosity and not a result.
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
    ///
    /// Not how fast the aircraft goes: how fast the MODEL goes before something
    /// in it declines to speak.
    ///
    /// **This is the ceiling of the FOUR-RAMP design inlet, not of the
    /// concept.** M3 is asked at [`DESIGN_RAMP_COUNT`] because that is the inlet
    /// this aircraft has; a different ramp schedule moves this number. It is a
    /// property of one configuration, and quoting it as a property of ramjet
    /// flight would be the overreach this module exists to avoid.
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

// ---------------------------------------------------------------------------
// The fourth frontier, and the only one that is about the VEHICLE.
// ---------------------------------------------------------------------------

/// Equivalence ratio the cycle actually runs at the design point: 0.4615.
///
/// **This is the number that decides whether VENTUS-1 flies at all**, and it is
/// one evaluation rather than a scan. Any lean blowout limit ABOVE this
/// equivalence ratio means the engine has already gone out before reaching
/// M 3.50.
///
/// The blowout band this project carries is phi 0.3 to 0.5 **[TO CITE]**, so
/// 0.4615 sits inside it and well above the middle. The probability mass is NOT
/// evenly split: phi 0.46 to 0.50 is an ordinary range for a ramjet combustor
/// without a dedicated flame holder, and every value in it puts the design point
/// out of reach.
///
/// That moves this `[TO CITE]` ahead of the L/D and Isp citations in the queue.
/// Those change a number by some per cent. This one decides between "the design
/// point has margin" and "the design point does not fly".
pub const DESIGN_POINT_EQUIVALENCE_RATIO: f64 = 0.4615;

/// Mach at which the required capture area equals the vehicle's own body
/// cross-section: M 3.847.
///
/// See [`capture_area_ratio`]. At the M 3.50 design point the ratio is 0.745.
pub const CAPTURE_AREA_CLOSES_AT_MACH: f64 = 3.847;

/// Air mass flow the engine must swallow for thrust to equal drag, divided by
/// the mass flux available per unit area: **the capture area the aircraft would
/// need at this Mach**, in square metres.
///
/// # Why this is computed rather than assumed
///
/// M3 has no capture area - `ventus-inlet` carries an explicit
/// `TODO(M3): capture area and spillage drag` - so asking "is there excess
/// thrust?" cannot be answered without inventing one. Inverting the question
/// needs nothing invented: thrust equals `Fs * rho * V * A_c`, drag comes out of
/// M6b in newtons, so the area that balances them falls out.
///
/// Returns `None` wherever M4 or M6b declines.
#[must_use]
pub fn required_capture_area_m2(mach: f64) -> Option<f64> {
    use ventus_aero::boundary_layer::EdgeState;

    let p = evaluate(mach);
    let (altitude_m, specific_thrust, velocity_m_s, wall_temperature_k) = (
        p.altitude_m?,
        p.ramjet_specific_thrust_n_s_kg?,
        p.velocity_m_s?,
        p.wall_temperature_k?,
    );
    let atmos = ventus_atmos::at_geopotential(altitude_m).ok()?;
    let geometry = ventus_aero::geometry::ventus1(DESIGN_DYNAMIC_PRESSURE_PA);
    let edge = EdgeState {
        temperature_k: atmos.temperature_k,
        pressure_pa: atmos.pressure_pa,
        velocity_m_s,
        mach,
        gamma: 1.4,
    };
    let drag = ventus_aero::drag::breakdown(
        &geometry,
        &edge,
        DESIGN_DYNAMIC_PRESSURE_PA,
        wall_temperature_k,
    )
    .ok()?;
    let drag_n = drag.total * DESIGN_DYNAMIC_PRESSURE_PA * geometry.wing_area_m2;
    let mass_flux_kg_m2_s = atmos.density_kg_m3 * velocity_m_s;
    Some(drag_n / (specific_thrust * mass_flux_kg_m2_s))
}

/// [`required_capture_area_m2`] over the vehicle's own maximum body
/// cross-section.
///
/// # What a ratio above 1 means, stated carefully
///
/// It does **not** prove the aircraft is impossible. It proves the
/// configuration M6b assumed is **self-inconsistent**: the Sears-Haack body that
/// sets the wave drag cannot also host an inlet larger than itself. Closing the
/// thrust balance past that point requires wing-mounted nacelles, which changes
/// the frontal area, the wave drag and therefore the drag number this ratio was
/// computed from.
///
/// That is a weaker claim than impossibility and a much harder one to argue
/// with, and it is the honest one.
///
/// # And this is the only frontier here that is about the aircraft
///
/// The other four are statements about the MODEL: where a correlation leaves its
/// fit, where a solver has nothing left to say. This one says something about
/// the vehicle - and it binds at M 3.847, below the lean blowout band's middle
/// and nearly two Mach below the M 5.70 ceiling M4 reports.
#[must_use]
pub fn capture_area_ratio(mach: f64) -> Option<f64> {
    let required = required_capture_area_m2(mach)?;
    let geometry = ventus_aero::geometry::ventus1(DESIGN_DYNAMIC_PRESSURE_PA);
    Some(required / geometry.max_cross_section_m2)
}

/// Above this Mach **no body size closes the thrust balance**: M 4.536.
///
/// See [`self_consistent_capture_area_m2`]. This is a harder frontier than
/// [`CAPTURE_AREA_CLOSES_AT_MACH`], and the two answer different questions.
pub const NO_BODY_CLOSES_ABOVE_MACH: f64 = 4.536;

/// Fraction of total drag that is wave drag at the design point: 0.089.
///
/// The number that decides whether the capture-area fixed point below converges.
/// Lift-induced is 0.661 and friction 0.249; M6b's headline finding is that at
/// M 3.5 drag due to lift dominates, and this is that finding load-bearing
/// somewhere else.
pub const WAVE_DRAG_FRACTION_AT_DESIGN_POINT: f64 = 0.089;

/// Capture area solved **self-consistently** against the wave drag it causes.
///
/// # Why [`required_capture_area_m2`] is not the whole answer
///
/// Sears-Haack wave drag goes as the SQUARE of maximum cross-section
/// (`drag.rs`: `4.5 pi q (A/L)^2`). If the inlet has to fit inside the body,
/// then growing the capture area grows the body, which grows the drag, which
/// grows the capture area needed. That is a fixed point, not an explicit
/// formula, and [`required_capture_area_m2`] evaluates only its first iterate
/// against the declared geometry.
///
/// Substituting `A_body = A` gives a quadratic:
///
/// ```text
///   k A^2  -  (F_s rho V) A  +  D_others  =  0,     k = 4.5 pi q / L^2
/// ```
///
/// with `D_others` the friction and lift-induced terms, neither of which depends
/// on cross-section. The physical branch is the SMALLER root: the larger one is
/// a body so big it is paying for itself in wave drag.
///
/// # Does the iteration converge? Measured, yes, and not narrowly
///
/// `dA_req/dA = 2 (D_wave/D_total) (A_req/A)`. The coefficient `2 (A_req/A)` is
/// 1.49 at the design point, so if wave drag dominated, the iteration would
/// diverge and no stable body size would exist. It does not dominate:
/// [`WAVE_DRAG_FRACTION_AT_DESIGN_POINT`] is 0.089, giving 0.133. Comfortably
/// convergent, and it stays under 0.3 out to M 4.5.
///
/// # What the correction is worth
///
/// At the crossing, **nothing** - and for a structural reason rather than luck.
/// Where `A = A_body` the two formulations evaluate the same drag, so they must
/// agree exactly there, and [`CAPTURE_AREA_CLOSES_AT_MACH`] is unchanged at
/// M 3.847 under the self-consistent solve.
///
/// Away from it they diverge in opposite directions. Below, the fixed-drag
/// answer is CONSERVATIVE (the declared body is larger than needed and carries
/// wave drag for area it is not using): 2.511 against 2.401 m2 at M 3.50. Above,
/// it is OPTIMISTIC: 5.655 against 7.080 m2 at M 4.40, 25 % low.
///
/// # The frontier this one can see and the other cannot
///
/// When the discriminant goes negative the roots stop existing, and that is not
/// gradual degradation - it is **no body size closing the balance at all**. It
/// happens at M 4.536.
///
/// Returns `None` there, and wherever M4 or M6b declines.
#[must_use]
pub fn self_consistent_capture_area_m2(mach: f64) -> Option<f64> {
    use ventus_aero::boundary_layer::EdgeState;

    let p = evaluate(mach);
    let (altitude_m, specific_thrust, velocity_m_s, wall_temperature_k) = (
        p.altitude_m?,
        p.ramjet_specific_thrust_n_s_kg?,
        p.velocity_m_s?,
        p.wall_temperature_k?,
    );
    let atmos = ventus_atmos::at_geopotential(altitude_m).ok()?;
    let geometry = ventus_aero::geometry::ventus1(DESIGN_DYNAMIC_PRESSURE_PA);
    let edge = EdgeState {
        temperature_k: atmos.temperature_k,
        pressure_pa: atmos.pressure_pa,
        velocity_m_s,
        mach,
        gamma: 1.4,
    };
    let drag = ventus_aero::drag::breakdown(
        &geometry,
        &edge,
        DESIGN_DYNAMIC_PRESSURE_PA,
        wall_temperature_k,
    )
    .ok()?;

    let scale = DESIGN_DYNAMIC_PRESSURE_PA * geometry.wing_area_m2;
    let d_others = (drag.friction + drag.lift_induced) * scale;
    let k = 4.5 * core::f64::consts::PI * DESIGN_DYNAMIC_PRESSURE_PA
        / (geometry.length_m * geometry.length_m);
    let thrust_per_area = specific_thrust * atmos.density_kg_m3 * velocity_m_s;

    let discriminant = thrust_per_area * thrust_per_area - 4.0 * k * d_others;
    if discriminant < 0.0 {
        return None;
    }
    Some((thrust_per_area - libm::sqrt(discriminant)) / (2.0 * k))
}
