//! M12 acceptance. Every assertion here is about a module REFUSING, or about
//! this module reproducing something another module already established.

use super::*;

/// THE SELF-CONSISTENCY CHECK THAT MAKES THE REST BELIEVABLE.
///
/// The constant-q rule is how this project chose 26 km for the M 3.5 design
/// point. Running that rule forwards must therefore land back on 26 km, and it
/// does to under a decimetre. If this drifts, the sweep is not comparable to the
/// design point and nothing else in the module means anything.
#[test]
fn the_constant_q_rule_regenerates_the_design_altitude() {
    let h = altitude_for_constant_q_m(3.5, DESIGN_DYNAMIC_PRESSURE_PA).unwrap();
    assert!(
        (h - 26_000.0).abs() < 0.5,
        "constant q put the design point at {h:.2} m, not 26 000 m"
    );
}

/// And the stagnation temperature there must be M2's published figure.
#[test]
fn the_design_point_row_matches_the_design_point_document() {
    let p = evaluate(3.5);
    let t0 = p.stagnation_temperature_k.unwrap();
    assert!(
        (t0 - 768.1).abs() < 0.1,
        "T0 came out {t0:.1} K against the documented 768.1 K"
    );
    assert_eq!(p.lightest_material, Some("Ti-6Al-4V"));
    assert_eq!(p.refusal_count(), 0, "the design point must be answerable");
}

/// THE TRAP THIS MODULE ALMOST SHIPPED.
///
/// Specific impulse DIVERGES as the ramjet dies, because the fuel-air ratio
/// falls to zero faster than the thrust does. The first version of the sweep
/// printed Isp as a headline column, where 4020 s at M 5.50 reads like a
/// discovery and is an engine producing almost nothing very efficiently.
///
/// This pins the divergence so nobody "fixes" the reports back.
#[test]
fn specific_impulse_diverges_as_the_engine_dies_and_is_flagged() {
    let design = evaluate(3.5);
    let dying = evaluate(5.5);

    let isp_design = design.ramjet_specific_impulse_s.unwrap();
    let isp_dying = dying.ramjet_specific_impulse_s.unwrap();
    assert!(
        isp_dying > 2.0 * isp_design,
        "the divergence is gone: {isp_dying:.0} s against {isp_design:.0} s"
    );

    // Thrust, meanwhile, collapses. That is the honest signal.
    let fs_design = design.ramjet_specific_thrust_n_s_kg.unwrap();
    let fs_dying = dying.ramjet_specific_thrust_n_s_kg.unwrap();
    assert!(fs_dying < 0.25 * fs_design);

    // And the flag separates them.
    assert!(design.ramjet_isp_is_meaningful());
    assert!(!dying.ramjet_isp_is_meaningful());
}

/// Specific thrust peaks well BELOW the design point, which is M4's own
/// recorded finding restated across a sweep: a ramjet has a design Mach number
/// because the burner limit caps exit velocity while flight speed keeps rising.
#[test]
fn the_peak_specific_thrust_is_where_it_is() {
    let peak = evaluate(PEAK_SPECIFIC_THRUST_MACH)
        .ramjet_specific_thrust_n_s_kg
        .unwrap();
    assert!(
        (peak - PEAK_SPECIFIC_THRUST_N_S_KG).abs() < 1.0,
        "peak specific thrust moved to {peak:.1}"
    );
    // Monotone decline from the peak to the refusal: no second hump.
    let mut previous = peak;
    let mut m = PEAK_SPECIFIC_THRUST_MACH + 0.25;
    while m <= 5.5 {
        let fs = evaluate(m).ramjet_specific_thrust_n_s_kg.unwrap();
        assert!(fs < previous, "specific thrust rose again at M {m:.2}");
        previous = fs;
        m += 0.25;
    }

    // At the design point the engine is already off its best.
    let at_design = evaluate(3.5).ramjet_specific_thrust_n_s_kg.unwrap();
    assert!((0.80..0.87).contains(&(at_design / peak)));
}

/// The two refusals that actually bind, and their order.
#[test]
fn the_engine_and_the_gas_model_bind_before_anything_else() {
    let e = envelope(2.0, 7.0, 0.05);

    let ramjet = e.first_refusal(Refusal::RamjetThermallyChoked).unwrap();
    let gas = e.first_refusal(Refusal::GasModelOutOfRange).unwrap();
    assert!(
        (5.6..5.8).contains(&ramjet),
        "the ramjet ceiling moved to M {ramjet:.2}"
    );
    assert!(
        (5.8..6.0).contains(&gas),
        "the gas model moved to M {gas:.2}"
    );
    assert!(
        ramjet < gas,
        "the engine should give out before the gas model"
    );

    // THE COUNTERINTUITIVE ONE. The material never binds on this trajectory,
    // because holding q means climbing and the density collapse takes the
    // convective flux with it. "Mach 5 melts everything" is a statement about
    // constant ALTITUDE, not constant dynamic pressure.
    assert_eq!(e.first_refusal(Refusal::NoMaterialSurvives), None);
    assert_eq!(e.first_refusal(Refusal::AtmosphereModelTop), None);
    assert_eq!(e.first_refusal(Refusal::InletShocksDetached), None);
}

/// The headline number, and the one the whole module exists to produce.
#[test]
fn the_chain_answers_up_to_a_declared_mach_and_no_further() {
    let e = envelope(2.0, 7.0, 0.05);
    let last = e.last_fully_answered_mach.unwrap();
    assert!(
        (5.6..5.7).contains(&last),
        "the fully-answered limit moved to M {last:.2}"
    );
    assert!(
        last > 3.5,
        "the design point must be inside the answerable range"
    );
}

/// Past the first refusal the remaining numbers are diagnostics, not results.
/// This asserts the module still PRODUCES them - suppressing them would hide
/// which module is still working - while the reports label them.
#[test]
fn numbers_past_the_first_refusal_still_compute_but_are_not_claims() {
    let p = evaluate(6.5);
    assert!(p.refused(Refusal::GasModelOutOfRange));
    assert!(p.refused(Refusal::RamjetThermallyChoked));
    // Thermal still answers, and that answer is NOT a claim about M 6.5 flight:
    // the gas model has already said it cannot describe this air.
    assert!(p.wall_temperature_k.is_some());
    assert!(p.refusal_count() >= 2);
}

/// M1 refuses when holding q would need an altitude above the model top. Forced
/// with an absurd dynamic pressure rather than an absurd Mach, because the Mach
/// needed is far outside anything else here.
#[test]
fn the_atmosphere_refuses_above_the_model_top() {
    assert_eq!(altitude_for_constant_q_m(3.5, 1.0e-3), None);
    // And below sea level, the other end.
    assert_eq!(altitude_for_constant_q_m(0.1, 1.0e9), None);
    // Non-physical inputs.
    assert_eq!(altitude_for_constant_q_m(0.0, 18_463.0), None);
    assert_eq!(altitude_for_constant_q_m(3.5, -1.0), None);
    assert_eq!(altitude_for_constant_q_m(f64::NAN, 18_463.0), None);
}

/// The lean-limit gap M12 surfaced in M4, pinned so the number in the
/// documentation stays true.
#[test]
fn the_unmodelled_lean_limit_would_bite_far_earlier_than_the_burner_ceiling() {
    let p = evaluate(LEAN_BLOWOUT_CROSSING_MACH);
    let fs = p.ramjet_specific_thrust_n_s_kg.unwrap();
    let isp = p.ramjet_specific_impulse_s.unwrap();
    let fuel_air = fs / (isp * ventus_units::constants::G0_M_S2);
    assert!(
        (fuel_air - 0.027).abs() < 0.001,
        "the f/a = 0.027 crossing moved off M {LEAN_BLOWOUT_CROSSING_MACH}: f/a = {fuel_air:.5}"
    );
}

/// A sweep with nonsense bounds does nothing rather than looping or panicking.
#[test]
fn a_degenerate_sweep_is_refused() {
    let mut seen = 0;
    sweep(5.0, 2.0, 0.1, |_| seen += 1);
    assert_eq!(seen, 0);
    sweep(2.0, 5.0, 0.0, |_| seen += 1);
    assert_eq!(seen, 0);
    sweep(2.0, 5.0, -1.0, |_| seen += 1);
    assert_eq!(seen, 0);

    let e = envelope(5.0, 2.0, 0.1);
    assert_eq!(e.last_fully_answered_mach, None);
}
