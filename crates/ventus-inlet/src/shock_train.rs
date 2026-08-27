//! External-compression shock train: N oblique ramps followed by a terminal
//! normal shock.
//!
//! # Why this module is the crux
//!
//! A supersonic engine has to deliver subsonic air to its combustor. Getting
//! there through a single normal shock at M 3.5 throws away 79 % of the total
//! pressure — and total pressure is the currency thrust is bought with. Turning
//! the flow through a series of weak oblique shocks first, and only then
//! crossing a normal shock at a much lower Mach number, recovers most of it.
//!
//! At the VENTUS-1 design point:
//!
//! ```text
//!   single normal shock        p02/p01 = 0.2129
//!   MIL-E-5008B reference      p02/p01 = 0.7416     a factor of 3.48
//! ```
//!
//! That factor is the reason a Mach 3.5 aircraft is shaped the way it is.
//!
//! # What is modelled, and what is not
//!
//! Modelled: the inviscid shock system and the total-pressure it recovers.
//!
//! NOT modelled: boundary-layer bleed, shock/boundary-layer interaction,
//! subsonic diffuser losses, cowl-lip spillage drag, or the moving spike that a
//! real Mach 3 inlet needs to hold its shock system across the flight envelope.
//! A real inlet loses several points of recovery to those. This model is
//! therefore an UPPER BOUND on what its shock system can achieve, and the gap
//! to the MIL-E-5008B empirical curve is the honest measure of what it omits.

use ventus_gasdyn::{normal_shock, oblique_shock, GasDynError, ShockBranch};

/// The largest number of external compression ramps this model carries.
/// Practical Mach 3 inlets use two to four; beyond that the returns are small
/// and the geometry becomes unbuildable.
pub const MAX_RAMPS: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InletError {
    /// More ramps requested than [`MAX_RAMPS`].
    TooManyRamps,
    /// At least one ramp is required.
    NoRamps,
    /// The flow went subsonic before the terminal shock, so the requested ramp
    /// angles are not a physically realisable external-compression system.
    FlowWentSubsonic,
    /// A ramp turns the flow more than an attached shock can manage at the local
    /// Mach number; physically the shock detaches and the inlet does not work.
    Detached,
    Gas(GasDynError),
}

impl From<GasDynError> for InletError {
    fn from(e: GasDynError) -> Self {
        match e {
            GasDynError::DeflectionExceedsMaximum => InletError::Detached,
            GasDynError::SubsonicUpstream => InletError::FlowWentSubsonic,
            other => InletError::Gas(other),
        }
    }
}

/// One station in the shock train.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Station {
    /// Mach number entering this shock.
    pub mach_in: f64,
    /// Mach number leaving it.
    pub mach_out: f64,
    /// Flow deflection across this ramp [rad]. Zero for the terminal shock.
    pub deflection_rad: f64,
    /// Shock wave angle [rad].
    pub wave_angle_rad: f64,
    /// Mach number of the component NORMAL to this shock. This is the number
    /// that sets the loss, and the one Oswatitsch's criterion equalises.
    pub normal_mach: f64,
    /// Total-pressure ratio across this shock alone.
    pub recovery: f64,
}

/// A complete external-compression system.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShockTrain {
    pub mach_freestream: f64,
    pub ramp_count: usize,
    pub stations: [Station; MAX_RAMPS + 1],
    /// Product of the oblique-shock recoveries.
    pub external_recovery: f64,
    /// Recovery across the terminal normal shock.
    pub terminal_recovery: f64,
    /// The number that matters: the whole system's total-pressure ratio.
    pub total_recovery: f64,
    /// Mach number entering the terminal normal shock.
    pub mach_before_terminal: f64,
    /// Total flow turning through the ramps [rad]. This is cowl angle, and it is
    /// what makes an inlet physically large.
    pub total_turning_rad: f64,
}

impl ShockTrain {
    /// The oblique stations only.
    pub fn ramps(&self) -> &[Station] {
        &self.stations[..self.ramp_count]
    }

    /// The terminal normal shock.
    #[must_use]
    pub fn terminal(&self) -> &Station {
        &self.stations[self.ramp_count]
    }
}

/// Solve a shock train for given ramp deflection angles.
///
/// Each ramp turns the flow through its own angle; the local Mach falls at each
/// one, and the terminal normal shock is applied to whatever is left.
///
/// # Errors
/// [`InletError`] if a ramp detaches, the flow goes subsonic early, or the ramp
/// count is out of range.
pub fn shock_train(
    mach_freestream: f64,
    ramp_angles_rad: &[f64],
    gamma: f64,
) -> Result<ShockTrain, InletError> {
    if ramp_angles_rad.is_empty() {
        return Err(InletError::NoRamps);
    }
    if ramp_angles_rad.len() > MAX_RAMPS {
        return Err(InletError::TooManyRamps);
    }

    let mut stations = [Station::default(); MAX_RAMPS + 1];
    let mut mach = mach_freestream;
    let mut external_recovery = 1.0;
    let mut total_turning = 0.0;

    for (i, &theta) in ramp_angles_rad.iter().enumerate() {
        if mach <= 1.0 {
            return Err(InletError::FlowWentSubsonic);
        }
        // Weak branch: a compression ramp that produced a strong shock would
        // choke the flow at the first ramp instead of at the throat.
        let s = oblique_shock(mach, theta, gamma, ShockBranch::Weak)?;
        stations[i] = Station {
            mach_in: mach,
            mach_out: s.mach_downstream,
            deflection_rad: theta,
            wave_angle_rad: s.wave_angle_rad,
            normal_mach: s.normal_mach_upstream,
            recovery: s.stagnation_pressure_ratio,
        };
        external_recovery *= s.stagnation_pressure_ratio;
        total_turning += theta;
        mach = s.mach_downstream;
    }

    if mach <= 1.0 {
        return Err(InletError::FlowWentSubsonic);
    }
    let terminal = normal_shock(mach, gamma)?;
    stations[ramp_angles_rad.len()] = Station {
        mach_in: mach,
        mach_out: terminal.mach_downstream,
        deflection_rad: 0.0,
        wave_angle_rad: core::f64::consts::FRAC_PI_2,
        normal_mach: mach,
        recovery: terminal.stagnation_pressure_ratio,
    };

    Ok(ShockTrain {
        mach_freestream,
        ramp_count: ramp_angles_rad.len(),
        stations,
        external_recovery,
        terminal_recovery: terminal.stagnation_pressure_ratio,
        total_recovery: external_recovery * terminal.stagnation_pressure_ratio,
        mach_before_terminal: mach,
        total_turning_rad: total_turning,
    })
}

/// Maximise total-pressure recovery over the ramp angles, numerically.
///
/// Coordinate descent with a golden-section line search on each angle, swept a
/// fixed number of times. Deterministic and bounded — no convergence criterion
/// that could fail to be met, and no dependence on iteration order beyond what
/// is written here.
///
/// **This routine imposes no theory.** It does not know about Oswatitsch's
/// equal-strength criterion; it only knows how to evaluate [`shock_train`] and
/// climb. That is deliberate: the test module then checks whether the optimum it
/// finds happens to satisfy the criterion, which turns a textbook result into an
/// independent verification of this module rather than an assumption baked into
/// it.
///
/// # Errors
/// [`InletError`] if no feasible ramp system exists at this Mach number.
pub fn optimise_ramps(
    mach_freestream: f64,
    ramp_count: usize,
    gamma: f64,
) -> Result<([f64; MAX_RAMPS], ShockTrain), InletError> {
    if ramp_count == 0 {
        return Err(InletError::NoRamps);
    }
    if ramp_count > MAX_RAMPS {
        return Err(InletError::TooManyRamps);
    }

    let evaluate = |angles: &[f64; MAX_RAMPS]| -> f64 {
        shock_train(mach_freestream, &angles[..ramp_count], gamma)
            .map_or(f64::NEG_INFINITY, |t| t.total_recovery)
    };

    // GRID REFINEMENT, not golden section.
    //
    // [CORRECTED] This was a golden-section line search, and it failed silently.
    // Ramp angles that detach the shock or drive the flow subsonic evaluate to
    // negative infinity, so the recovery surface has large infeasible plateaus.
    // Golden section assumes unimodality: when both of its probe points land on
    // the plateau, `f(a) > f(b)` is false for two infinities, the bracket always
    // walks the same way, and it converges to the infeasible boundary. At M 2.5
    // with three ramps it returned the starting point unchanged - 12 deg on every
    // ramp - and reported it as an optimum.
    //
    // Nothing in the recovery number looked wrong. What caught it was the
    // Oswatitsch check: the "optimum" had normal Mach numbers 1.391, 1.327 and
    // 1.314 instead of the equal values the criterion demands, and the true
    // optimum recovers 0.9225 against the 0.9131 that was being reported.
    //
    // Grid refinement makes no unimodality assumption and simply ignores
    // infeasible points. Fixed counts, so it stays deterministic and bounded.
    const COARSE: usize = 120;
    const REFINEMENTS: usize = 4;
    // 45 degrees. No single external ramp turns further than this in any
    // realisable inlet, and the search never needs to look there.
    const MAX_RAMP_ANGLE_RAD: f64 = core::f64::consts::FRAC_PI_4;

    // Multi-start as well: the surface is not convex, and a single start can sit
    // in a local basin even with a robust line search.
    let mut best_angles = [0.0_f64; MAX_RAMPS];
    let mut best_value = f64::NEG_INFINITY;

    for start_deg in [3.0_f64, 6.0, 9.0, 12.0, 15.0] {
        let mut angles = [0.0_f64; MAX_RAMPS];
        for a in angles.iter_mut().take(ramp_count) {
            *a = start_deg.to_radians();
        }
        if evaluate(&angles) == f64::NEG_INFINITY {
            continue;
        }

        for _ in 0..12 {
            for i in 0..ramp_count {
                let (mut lo, mut hi) = (0.0_f64, MAX_RAMP_ANGLE_RAD);
                for _ in 0..REFINEMENTS {
                    let mut local_best = angles[i];
                    let mut local_value = evaluate(&angles);
                    for k in 0..=COARSE {
                        let mut trial = angles;
                        trial[i] = lo + (hi - lo) * (k as f64) / (COARSE as f64);
                        let v = evaluate(&trial);
                        if v > local_value {
                            local_value = v;
                            local_best = trial[i];
                        }
                    }
                    angles[i] = local_best;
                    let half = (hi - lo) / (COARSE as f64);
                    lo = (local_best - half).max(0.0);
                    hi = (local_best + half).min(MAX_RAMP_ANGLE_RAD);
                }
            }
        }

        let value = evaluate(&angles);
        if value > best_value {
            best_value = value;
            best_angles = angles;
        }
    }

    if best_value == f64::NEG_INFINITY {
        return Err(InletError::Detached);
    }

    let train = shock_train(mach_freestream, &best_angles[..ramp_count], gamma)?;
    Ok((best_angles, train))
}

/// Empirical total-pressure recovery for a practical supersonic inlet,
/// `1 - 0.075 (M - 1)^1.35`, from MIL-E-5008B.
///
/// This is the reference a real inlet is held to. It is an envelope fitted to
/// flight and tunnel data, so it INCLUDES the viscous and bleed losses this
/// module does not model. Comparing the two is how the model's omissions get
/// measured rather than forgotten.
///
/// Returns 1.0 at and below M = 1, where there is no supersonic loss to take.
#[must_use]
pub fn mil_e_5008b_recovery(mach: f64) -> f64 {
    if mach <= 1.0 {
        return 1.0;
    }
    1.0 - 0.075 * libm::pow(mach - 1.0, 1.35)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ventus_units::float::{abs, rel_err};

    const GAMMA: f64 = 1.4;
    const DESIGN_MACH: f64 = 3.5;

    /// A one-ramp train with a zero-angle ramp IS a normal shock. Two different
    /// paths through the code that must agree.
    #[test]
    fn a_zero_angle_ramp_reduces_to_a_single_normal_shock() {
        let train = shock_train(DESIGN_MACH, &[0.0], GAMMA).unwrap();
        let bare = normal_shock(DESIGN_MACH, GAMMA).unwrap();
        assert!(rel_err(train.total_recovery, bare.stagnation_pressure_ratio) < 1e-12);
        assert!(rel_err(train.external_recovery, 1.0) < 1e-12);
    }

    /// Recovery is the product of the individual shocks, by construction. If
    /// this ever drifts, the bookkeeping is wrong somewhere.
    #[test]
    fn total_recovery_is_the_product_of_the_stations() {
        let angles = [
            8.0_f64.to_radians(),
            8.0_f64.to_radians(),
            6.0_f64.to_radians(),
        ];
        let t = shock_train(DESIGN_MACH, &angles, GAMMA).unwrap();
        let product: f64 = t.stations[..=t.ramp_count]
            .iter()
            .map(|s| s.recovery)
            .product();
        assert!(rel_err(t.total_recovery, product) < 1e-14);
        // Every shock destroys total pressure; none creates it.
        for s in &t.stations[..=t.ramp_count] {
            assert!(s.recovery <= 1.0 && s.recovery > 0.0);
            assert!(
                s.normal_mach >= 1.0 - 1e-12,
                "normal Mach {} < 1",
                s.normal_mach
            );
        }
        assert!(
            t.terminal().mach_out < 1.0,
            "the terminal shock must go subsonic"
        );
    }

    /// THE RESULT M3 EXISTS FOR. More shocks recover more, monotonically, and the
    /// first one buys the most.
    #[test]
    fn more_shocks_recover_more_total_pressure() {
        let single = normal_shock(DESIGN_MACH, GAMMA)
            .unwrap()
            .stagnation_pressure_ratio;
        let mut previous = single;
        for n in 1..=4 {
            let (_, t) = optimise_ramps(DESIGN_MACH, n, GAMMA).unwrap();
            assert!(
                t.total_recovery > previous,
                "n={n}: {} did not beat {previous}",
                t.total_recovery
            );
            previous = t.total_recovery;
        }
        // The whole argument, in one number.
        assert!(
            previous / single > 3.0,
            "four ramps recovered {previous} against {single} for a normal shock"
        );
    }

    /// OSWATITSCH, VERIFIED RATHER THAN ASSUMED.
    ///
    /// The classical result is that a shock train recovers the most total
    /// pressure when its oblique shocks are of EQUAL STRENGTH — equal Mach
    /// numbers normal to each wave. `optimise_ramps` knows nothing about that;
    /// it only climbs the recovery surface. So if the optimum it finds turns out
    /// to have equal normal Mach numbers, that is an independent confirmation
    /// that both the optimiser and the underlying shock relations are right.
    ///
    /// This is the strongest check in the module, and it needs no table.
    #[test]
    fn the_numerical_optimum_satisfies_oswatitsch_equal_strength() {
        for mach in [2.5, 3.0, 3.5, 4.0] {
            for n in 2..=4 {
                let (angles, t) = optimise_ramps(mach, n, GAMMA).unwrap();
                let normals: &[Station] = t.ramps();
                let first = normals[0].normal_mach;
                for (i, s) in normals.iter().enumerate() {
                    assert!(
                        rel_err(s.normal_mach, first) < 5e-3,
                        "M={mach} n={n}: ramp {i} has normal Mach {} against {first}; \
                         the optimum should be equal-strength",
                        s.normal_mach
                    );
                }
                // And the ramp angles are NOT equal, which is the point: equal
                // strength requires progressively larger turns as the flow slows.
                if n >= 3 {
                    assert!(
                        angles[n - 1] > angles[0] * 1.05,
                        "M={mach} n={n}: equal-strength shocks need increasing ramp \
                         angles, got {:?}",
                        &angles[..n]
                    );
                }
            }
        }
    }

    /// The model is an inviscid upper bound, so it must BEAT the empirical curve
    /// for a real inlet. If it ever came out below, the model would be losing
    /// pressure it has no mechanism to lose.
    #[test]
    fn the_inviscid_model_bounds_the_empirical_curve_from_above() {
        for mach in [2.0, 2.5, 3.0, 3.5, 4.0] {
            let empirical = mil_e_5008b_recovery(mach);
            let (_, t) = optimise_ramps(mach, 4, GAMMA).unwrap();
            assert!(
                t.total_recovery > empirical,
                "M={mach}: inviscid {} is below the empirical {empirical}, which is \
                 impossible for a model with no viscous losses",
                t.total_recovery
            );
        }
    }

    /// HOW MUCH INLET THE DESIGN POINT ACTUALLY DEMANDS, and why the answer is
    /// not a simple external ramp.
    ///
    /// [CORRECTED] This test first asserted that three ramps would clear the
    /// MIL-E-5008B target. They do not: three give 0.7311 against a target of
    /// 0.7416. It takes FOUR oblique ramps plus the terminal shock - a five-shock
    /// system - to beat the empirical curve inviscidly. The assertion was a guess
    /// written before the module could answer, and the module disagreed.
    ///
    /// The real finding is about geometry rather than pressure. Four
    /// equal-strength ramps at M 3.5 need about 48 degrees of total flow turning.
    /// A purely external compression surface turning the flow 48 degrees is
    /// enormous, and every degree of it is cowl frontal area and wave drag.
    ///
    /// **That is why real Mach 3 inlets use MIXED compression**: some shocks are
    /// folded inside the duct, where they cost duct length instead of frontal
    /// area. It is also why they need a translating spike - an internal shock
    /// system must be held in place as the flight condition changes, and losing
    /// it is an unstart. This model covers the external part only, so the
    /// turning angle it reports is the honest price of doing it all outside.
    #[test]
    fn the_design_point_needs_four_ramps_and_a_lot_of_turning() {
        let target = mil_e_5008b_recovery(DESIGN_MACH);
        assert!(
            rel_err(target, 0.7416) < 1e-3,
            "MIL-E-5008B at M3.5 = {target}"
        );

        let three = optimise_ramps(DESIGN_MACH, 3, GAMMA).unwrap().1;
        let (_, four) = optimise_ramps(DESIGN_MACH, 4, GAMMA).unwrap();

        assert!(
            three.total_recovery < target,
            "three ramps gave {}, which now clears the empirical target - the model \
             got better and this finding needs restating",
            three.total_recovery
        );
        assert!(
            four.total_recovery > target,
            "four ramps gave {}, below the empirical target",
            four.total_recovery
        );

        // The price, in geometry.
        let turning = four.total_turning_rad.to_degrees();
        assert!(
            (45.0..52.0).contains(&turning),
            "four-ramp turning is {turning:.1} deg; the recorded value is about 48"
        );
    }

    /// Turning the flow costs geometry. The total turning at the optimum is the
    /// cowl angle, and it is what makes a Mach 3 inlet physically enormous.
    #[test]
    fn optimal_turning_grows_with_mach_number() {
        let mut previous = 0.0;
        for mach in [2.0, 2.5, 3.0, 3.5, 4.0] {
            let (_, t) = optimise_ramps(mach, 3, GAMMA).unwrap();
            let deg = t.total_turning_rad.to_degrees();
            assert!(deg > previous, "M={mach}: turning {deg} did not grow");
            assert!(
                (5.0..60.0).contains(&deg),
                "M={mach}: {deg} deg is not plausible"
            );
            previous = deg;
        }
    }

    #[test]
    fn infeasible_geometry_is_refused_rather_than_extrapolated() {
        // A single 40 degree ramp detaches at M 3.5.
        assert_eq!(
            shock_train(DESIGN_MACH, &[40.0_f64.to_radians()], GAMMA),
            Err(InletError::Detached)
        );
        assert_eq!(
            shock_train(DESIGN_MACH, &[], GAMMA),
            Err(InletError::NoRamps)
        );
        assert_eq!(
            shock_train(DESIGN_MACH, &[0.01; MAX_RAMPS + 1], GAMMA),
            Err(InletError::TooManyRamps)
        );
        // A subsonic freestream has no shock system at all.
        assert_eq!(
            shock_train(0.8, &[0.1], GAMMA),
            Err(InletError::FlowWentSubsonic)
        );
    }

    #[test]
    fn the_empirical_curve_behaves() {
        assert!(abs(mil_e_5008b_recovery(1.0) - 1.0) < 1e-15);
        assert!(abs(mil_e_5008b_recovery(0.5) - 1.0) < 1e-15);
        let mut previous = 1.0;
        let mut m = 1.0;
        while m <= 5.0 {
            let r = mil_e_5008b_recovery(m);
            assert!(r <= previous, "M={m}: empirical recovery rose");
            previous = r;
            m += 0.01;
        }
    }
}
