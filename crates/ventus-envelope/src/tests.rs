//! M12 acceptance. Every assertion here is about a module REFUSING, or about
//! this module reproducing something another module already established.

use super::*;

/// THE PRESSURE-TO-ALTITUDE INVERSION IS A TRUE INVERSE.
///
/// The constant-q rule is how this project chose 26 km, so running it forwards
/// must land back on 26 km. It does, to 8 cm.
///
/// **What that 8 cm proves, and what it does not.** It proves the bisection is a
/// genuine inverse of `at_geopotential` and that the sweep is anchored to the
/// same flight condition as the design point. It proves NOTHING about whether
/// the atmosphere is right: if US76 were mis-implemented, the round trip would
/// still close to 8 cm, because both directions would be wrong identically.
/// M1's own cases against the published table are what check that; this is a
/// regression lock on the inversion, and it is named for what it tests.
#[test]
fn the_pressure_to_altitude_inversion_round_trips_to_the_design_altitude() {
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

/// THE LEAN-LIMIT GAP M12 SURFACED IN M4, AS A BAND.
///
/// [CORRECTED] This pinned a single crossing at M 3.91 from a single equivalence
/// ratio. Blowout is a band, and swept across it the band straddles the design
/// point: at the permissive end the engine has 0.9 Mach of margin, at the strict
/// end it has already blown out before reaching M 3.50.
#[test]
fn the_unmodelled_lean_limit_band_straddles_the_design_point() {
    let permissive = lean_blowout_mach(LEAN_BLOWOUT_PHI_MIN, 2.0, 5.7, 0.01).unwrap();
    let strict = lean_blowout_mach(LEAN_BLOWOUT_PHI_MAX, 2.0, 5.7, 0.01).unwrap();

    assert!(strict < permissive, "the band is inverted");
    assert!(
        (4.3..4.5).contains(&permissive),
        "the permissive end moved to M {permissive:.2}"
    );
    assert!(
        (3.1..3.3).contains(&strict),
        "the strict end moved to M {strict:.2}"
    );

    // THE FINDING. The design point is inside the band, not above or below it.
    assert!(
        strict < 3.5 && permissive > 3.5,
        "the design point no longer sits inside the blowout band: {strict:.2} to {permissive:.2}"
    );

    // And the whole band is far below the burner ceiling the envelope reports,
    // which is the point: that ceiling is not the real limit.
    let ceiling = envelope(2.0, 7.0, 0.05)
        .first_refusal(Refusal::RamjetThermallyChoked)
        .unwrap();
    assert!(permissive < ceiling - 1.0);

    std::println!(
        "lean blowout band M {strict:.2} to M {permissive:.2}; design point M 3.50; reported ceiling M {ceiling:.2}"
    );
}

/// A blowout query with nonsense inputs refuses rather than looping.
#[test]
fn a_degenerate_blowout_query_is_refused() {
    assert_eq!(lean_blowout_mach(0.0, 2.0, 5.0, 0.01), None);
    assert_eq!(lean_blowout_mach(-0.4, 2.0, 5.0, 0.01), None);
    assert_eq!(lean_blowout_mach(f64::NAN, 2.0, 5.0, 0.01), None);
    assert_eq!(lean_blowout_mach(0.4, 5.0, 2.0, 0.01), None);
    assert_eq!(lean_blowout_mach(0.4, 2.0, 5.0, 0.0), None);
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

/// THE NUMBER THAT DECIDES WHETHER THE AIRCRAFT FLIES AT ALL.
///
/// The cycle runs at phi = 0.4615 at the design point. The blowout band carried
/// here is phi 0.3 to 0.5, so the design point sits inside it and above the
/// middle - and phi 0.46 to 0.50 is ordinary for a combustor without a dedicated
/// flame holder. The probability mass is not evenly split.
#[test]
fn the_design_point_equivalence_ratio_sits_high_in_the_blowout_band() {
    let p = evaluate(3.50);
    let fuel_air = p.ramjet_specific_thrust_n_s_kg.unwrap()
        / (p.ramjet_specific_impulse_s.unwrap() * ventus_units::constants::G0_M_S2);
    let phi = fuel_air / STOICHIOMETRIC_FUEL_AIR_RATIO;

    assert!(
        (phi - DESIGN_POINT_EQUIVALENCE_RATIO).abs() < 1e-3,
        "the design-point equivalence ratio moved to {phi:.4}"
    );
    // Inside the band, and above its midpoint.
    assert!(phi > LEAN_BLOWOUT_PHI_MIN && phi < LEAN_BLOWOUT_PHI_MAX);
    assert!(phi > 0.5 * (LEAN_BLOWOUT_PHI_MIN + LEAN_BLOWOUT_PHI_MAX));

    std::println!(
        "design point runs at phi = {phi:.4}; any blowout limit above that and it never gets there"
    );
}

/// THE FOURTH FRONTIER, AND THE ONLY ONE ABOUT THE VEHICLE.
///
/// The other four say where a MODEL stops. This says the required capture area
/// outgrows the body that has to carry it, at M 3.847 - below the middle of the
/// blowout band and nearly two Mach under the ceiling M4 reports.
#[test]
fn the_required_capture_area_outgrows_the_body_below_the_reported_ceiling() {
    let at_design = capture_area_ratio(3.50).unwrap();
    assert!(
        (at_design - 0.745).abs() < 0.01,
        "the design-point capture ratio moved to {at_design:.3}"
    );

    let at_crossing = capture_area_ratio(CAPTURE_AREA_CLOSES_AT_MACH).unwrap();
    assert!(
        (at_crossing - 1.0).abs() < 0.01,
        "the crossing moved: ratio {at_crossing:.3} at M {CAPTURE_AREA_CLOSES_AT_MACH}"
    );

    // Monotone in Mach over the range that matters, so the crossing is a
    // crossing and not one of several.
    let mut previous = 0.0;
    let mut m = 2.0_f64;
    while m <= 4.5 {
        let r = capture_area_ratio(m).unwrap();
        assert!(r > previous, "the capture ratio stopped rising at M {m:.2}");
        previous = r;
        m += 0.25;
    }

    // It binds well below the engine ceiling the envelope reports.
    let ceiling = envelope(2.0, 7.0, 0.05)
        .first_refusal(Refusal::RamjetThermallyChoked)
        .unwrap();
    assert!(CAPTURE_AREA_CLOSES_AT_MACH < ceiling - 1.5);
}

/// The capture-area question refuses wherever its inputs do, rather than
/// producing a number from a chain that has already gone silent.
#[test]
fn the_capture_area_question_refuses_where_the_engine_does() {
    assert!(required_capture_area_m2(6.5).is_none());
    assert!(capture_area_ratio(6.5).is_none());
    assert!(required_capture_area_m2(3.5).is_some());
}

/// [CORRECTED] THE CAPTURE-AREA NUMBER WAS A FIRST ITERATE, NOT A FIXED POINT.
///
/// Sears-Haack wave drag goes as the SQUARE of cross-section, so growing the
/// inlet grows the body grows the drag grows the inlet. `required_capture_area_m2`
/// evaluates that once against the declared geometry;
/// `self_consistent_capture_area_m2` solves it.
#[test]
fn the_self_consistent_capture_area_agrees_at_the_crossing_and_not_elsewhere() {
    let g = ventus_aero::geometry::ventus1(DESIGN_DYNAMIC_PRESSURE_PA);

    // AT the crossing the two must agree exactly, because A = A_body there means
    // both formulations evaluate the same drag. Structural, not luck.
    let fixed = required_capture_area_m2(CAPTURE_AREA_CLOSES_AT_MACH).unwrap();
    let solved = self_consistent_capture_area_m2(CAPTURE_AREA_CLOSES_AT_MACH).unwrap();
    assert!(
        (solved / fixed - 1.0).abs() < 2e-3,
        "the two formulations disagree at the crossing: {solved:.3} against {fixed:.3}"
    );
    assert!((solved / g.max_cross_section_m2 - 1.0).abs() < 2e-3);

    // BELOW it the fixed-drag answer is conservative: the declared body is
    // larger than needed and pays wave drag for area it is not using.
    let (f_lo, s_lo) = (
        required_capture_area_m2(3.50).unwrap(),
        self_consistent_capture_area_m2(3.50).unwrap(),
    );
    assert!(
        s_lo < f_lo,
        "below the crossing the solve should be smaller"
    );

    // ABOVE it the fixed-drag answer is optimistic, by 25 % at M 4.40.
    let (f_hi, s_hi) = (
        required_capture_area_m2(4.40).unwrap(),
        self_consistent_capture_area_m2(4.40).unwrap(),
    );
    assert!(s_hi > f_hi);
    assert!(
        (s_hi / f_hi - 1.25).abs() < 0.05,
        "the optimism at M 4.40 moved to {:.3}x",
        s_hi / f_hi
    );
}

/// The convergence criterion, measured rather than assumed.
///
/// `dA_req/dA = 2 (D_wave/D_total) (A_req/A)`. The `2 (A_req/A)` factor is 1.49
/// at the design point, so a wave-dominated drag budget would diverge and no
/// stable body size would exist. Lift-induced drag dominates instead - M6b's own
/// headline - and the iteration converges comfortably.
#[test]
fn the_capture_area_iteration_converges_because_wave_drag_is_a_small_fraction() {
    use ventus_aero::boundary_layer::EdgeState;

    let g = ventus_aero::geometry::ventus1(DESIGN_DYNAMIC_PRESSURE_PA);
    let p = evaluate(3.50);
    let atmos = ventus_atmos::at_geopotential(p.altitude_m.unwrap()).unwrap();
    let edge = EdgeState {
        temperature_k: atmos.temperature_k,
        pressure_pa: atmos.pressure_pa,
        velocity_m_s: p.velocity_m_s.unwrap(),
        mach: 3.50,
        gamma: 1.4,
    };
    let d = ventus_aero::drag::breakdown(
        &g,
        &edge,
        DESIGN_DYNAMIC_PRESSURE_PA,
        p.wall_temperature_k.unwrap(),
    )
    .unwrap();

    let wave_fraction = d.wave / d.total;
    assert!(
        (wave_fraction - WAVE_DRAG_FRACTION_AT_DESIGN_POINT).abs() < 2e-3,
        "the wave fraction moved to {wave_fraction:.4}"
    );
    // Lift-induced dominates, which is M6b's finding and is what saves this.
    assert!(d.lift_induced > d.wave + d.friction);

    let ratio = capture_area_ratio(3.50).unwrap();
    let derivative = 2.0 * wave_fraction * ratio;
    assert!(
        derivative < 0.3,
        "the fixed point stopped converging: dA_req/dA = {derivative:.3}"
    );
    // And the coefficient it would have been on, had wave drag dominated.
    assert!(2.0 * ratio > 1.4);

    std::println!(
        "wave {:.1} % of drag, dA_req/dA = {derivative:.3} (would be {:.2} if wave dominated)",
        100.0 * wave_fraction,
        2.0 * ratio
    );
}

/// THE FRONTIER THE FIRST FORMULATION COULD NOT SEE.
///
/// When the discriminant goes negative the roots stop existing: not gradual
/// degradation, no body size closing the balance at all.
#[test]
fn above_a_declared_mach_no_body_size_closes_the_thrust_balance() {
    assert!(self_consistent_capture_area_m2(NO_BODY_CLOSES_ABOVE_MACH - 0.02).is_some());
    assert!(self_consistent_capture_area_m2(NO_BODY_CLOSES_ABOVE_MACH + 0.02).is_none());

    // The fixed-drag formulation happily returns a number up there, which is
    // exactly why it needed replacing as the headline.
    assert!(required_capture_area_m2(NO_BODY_CLOSES_ABOVE_MACH + 0.02).is_some());
}
