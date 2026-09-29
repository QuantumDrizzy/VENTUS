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
/// The cycle runs at phi = 0.4615 at the design point. The literature band is
/// phi 0.3 to 0.5, so the design point sits inside it and above the middle.
/// No flame holder is declared, so the operative bound is the strict end,
/// and 0.4615 is below that.
#[test]
fn the_design_point_equivalence_ratio_sits_high_in_the_blowout_band() {
    let p = evaluate(3.50);
    let phi = point_equivalence_ratio(&p).unwrap();

    assert!(
        (phi - DESIGN_POINT_EQUIVALENCE_RATIO).abs() < 1e-3,
        "the design-point equivalence ratio moved to {phi:.4}"
    );
    // Inside the literature band, and above its midpoint.
    assert!(phi > LEAN_BLOWOUT_PHI_MIN && phi < LEAN_BLOWOUT_PHI_MAX);
    assert!(phi > 0.5 * (LEAN_BLOWOUT_PHI_MIN + LEAN_BLOWOUT_PHI_MAX));
    assert_eq!(
        literature_blowout_band(phi),
        Some(LiteratureBlowoutBand::InsideBand)
    );

    std::println!(
        "design point runs at phi = {phi:.4}; any blowout limit above that and it never gets there"
    );
}

/// THE OPERATIVE FLY/NO-FLY STATEMENT, WITHOUT INVENTING A HOLDER.
///
/// The permissive end is not available: [`FLAME_HOLDER_DECLARED`] is locked
/// false at compile time in `lib.rs`. The operative bound is therefore 0.50,
/// now cited from Useller Fig. 8 as a V-gutter afterburner floor. Design phi
/// is below it, so under the only bound that applies to this aircraft the
/// snapshot does not hold a flame. A ramjet-no-holder chart has still not
/// been read.
#[test]
fn under_the_no_holder_bound_the_design_point_does_not_hold_a_flame() {
    // Holder lock is compile-time in lib.rs (`const _: () = assert!(!…)`).
    // This test checks the physics that lock exists to protect.
    assert_eq!(OPERATIVE_LEAN_BLOWOUT_PHI, LEAN_BLOWOUT_PHI_STRICT);

    let phi = point_equivalence_ratio(&evaluate(3.50)).unwrap();
    assert_eq!(
        operative_lean_blowout_verdict(phi),
        Some(LeanBlowoutVerdict::BelowOperativeBound)
    );
    // The proposed M 4 row is further lean, so the same bound is harsher there.
    let phi_m4 = point_equivalence_ratio(&evaluate(PROPOSED_M4_CRUISE_MACH)).unwrap();
    assert!(phi_m4 < phi);
    assert_eq!(
        operative_lean_blowout_verdict(phi_m4),
        Some(LeanBlowoutVerdict::BelowOperativeBound)
    );

    assert_eq!(operative_lean_blowout_verdict(0.0), None);
    assert_eq!(operative_lean_blowout_verdict(f64::NAN), None);
    assert_eq!(
        operative_lean_blowout_verdict(0.51),
        Some(LeanBlowoutVerdict::AboveOperativeBound)
    );
}

/// THE DIGIT 0.50 IS READ FROM A PRIMARY FIGURE, AND IT IS THE WRONG CLASS.
///
/// Useller Fig. 8: φ = 0.50 (6-ft) and 0.63 (3-ft) WITH a V-gutter at 1600 psf.
/// King Fig. 16: f/a = 0.035 at the cold, high-P end → φ = 0.518 on King's
/// 0.0676. Both are holder-equipped afterburners. VENTUS has no holder, so
/// the operative bound stays the Useller six-foot floor, and the ramjet-no-
/// holder flag stays false.
#[test]
fn the_0_50_digit_is_useller_fig_8_and_is_not_a_ramjet_no_holder_close() {
    let phi = point_equivalence_ratio(&evaluate(3.50)).unwrap();
    assert!(
        phi < USELLER_FIG_8_PHI_SIX_FOOT,
        "design phi {phi:.4} is no longer below the Useller six-foot floor"
    );
    assert!(
        phi < KING_FIG16_PHI_COLD_HIGH_P,
        "design phi {phi:.4} is no longer below King cold/high-P"
    );
    assert!(
        phi > KING_FIG16_PHI_HOT_HIGH_P,
        "design phi {phi:.4} is no longer above King hot/high-P; that holder reading is the fake save"
    );
}

/// Burner-entry total pressure is above the King afterburner band and still
/// afterburner-scale, not a turbojet main-burner.
#[test]
fn burner_entry_pressure_is_above_king_and_below_turbojet_combustor_scale() {
    let p02 = evaluate(3.50).burner_entry_total_pressure_pa.unwrap();
    assert_eq!(
        burner_pressure_scale(p02),
        Some(BurnerPressureScale::AboveKingBandAfterburnerScale)
    );
    assert_eq!(king_afterburner_pressure_band_contains(p02), Some(false));
    assert!(p02 > KING_AFTERBURNER_PRESSURE_MAX_PA);
    assert!(p02 < 3.0e5);
    assert_eq!(
        burner_pressure_scale(USELLER_FIG_8_PRESSURE_PA),
        Some(BurnerPressureScale::InsideKingBand)
    );
    assert_eq!(
        burner_pressure_scale(KING_AFTERBURNER_PRESSURE_MIN_PA),
        Some(BurnerPressureScale::InsideKingBand)
    );
    assert_eq!(
        burner_pressure_scale(1.0e6),
        Some(BurnerPressureScale::TurbojetCombustorScale)
    );
    assert_eq!(
        burner_pressure_scale(20_000.0),
        Some(BurnerPressureScale::BelowKingBand)
    );
    assert_eq!(burner_pressure_scale(0.0), None);
    assert_eq!(burner_pressure_scale(f64::NAN), None);
}

/// [CORRECTED] Burner-entry pressure is ram total pressure, not 1.6 kPa.
///
/// Freestream at 26 km is 2.15 kPa. Omitting ram and writing p∞ · π_d gives
/// ~1.60 kPa, which would make combustion look impossible. The cycle does
/// not omit ram: burner-entry *total* pressure is ~100 kPa, sea-level-ish,
/// which is what makes a flame physically possible at all. Lefebvre loading
/// still needs a volume and a holder to evaluate; this test only pins the
/// station the correlation would be fed.
#[test]
fn burner_entry_total_pressure_is_ram_not_ambient() {
    let p = evaluate(3.50);
    let p02 = p.burner_entry_total_pressure_pa.unwrap();
    let atmos = ventus_atmos::at_geopotential(p.altitude_m.unwrap()).unwrap();

    // Ambient is kilopascals, not hundred kilopascals.
    assert!(
        atmos.pressure_pa < 3_000.0,
        "freestream at 26 km moved to {} Pa",
        atmos.pressure_pa
    );
    // Burner entry is two orders of magnitude above ambient: ram did the work.
    assert!(
        p02 > 50.0 * atmos.pressure_pa,
        "burner-entry total {p02:.0} Pa is not ram-compressed against {} Pa ambient",
        atmos.pressure_pa
    );
    assert!(
        (1.0e5..1.6e5).contains(&p02),
        "burner-entry total moved to {p02:.0} Pa"
    );
    // The 1.6 kPa trap: p∞ · π_d, ram omitted.
    if let Some(recovery) = p.inlet_recovery {
        let omitted_ram = atmos.pressure_pa * recovery;
        assert!(
            (1_400.0..1_800.0).contains(&omitted_ram),
            "the omitted-ram figure moved to {omitted_ram:.0} Pa; the 1.6 kPa trap needs restating"
        );
        assert!(p02 > 50.0 * omitted_ram);
    }

    std::println!(
        "burner entry p0 = {p02:.0} Pa against p∞ = {:.0} Pa (ram ×{:.0})",
        atmos.pressure_pa,
        p02 / atmos.pressure_pa
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

/// THE PROPOSED M 4 ROW IS NOT A CLOSE OF THE SNAPSHOT AIRCRAFT.
///
/// Programme intent is cruise ≥ Mach 4 (`docs/design-point-m4.md`). The
/// validated snapshot remains M 3.50. On the body M6b actually derived,
/// required capture already exceeds the cross-section at M 4.00 — that bind
/// is at [`CAPTURE_AREA_CLOSES_AT_MACH`], before the proposed cruise. A larger
/// Sears-Haack could still exist (M 4.00 < [`NO_BODY_CLOSES_ABOVE_MACH`]); that
/// is a different aeroplane, and it is [`m4_candidate_geometry`].
#[test]
fn current_geometry_does_not_close_capture_at_the_proposed_m4() {
    let h = altitude_for_constant_q_m(PROPOSED_M4_CRUISE_MACH, DESIGN_DYNAMIC_PRESSURE_PA).unwrap();
    let atmos = ventus_atmos::at_geopotential(h).unwrap();
    let q = 0.7 * atmos.pressure_pa * PROPOSED_M4_CRUISE_MACH * PROPOSED_M4_CRUISE_MACH;
    assert!(
        (q - DESIGN_DYNAMIC_PRESSURE_PA).abs() < 1.0,
        "the proposed row left the constant-q schedule: q = {q:.1} Pa"
    );
    assert!(
        h > 27_000.0 && h < 28_500.0,
        "constant-q altitude at M 4 moved to {h:.0} m"
    );

    let ratio = capture_area_ratio(PROPOSED_M4_CRUISE_MACH).unwrap();
    assert!(
        ratio > 1.0,
        "snapshot geometry closed at the proposed M 4: ratio {ratio:.3}. \
         If a new body landed as the snapshot, delete this assertion in the same change as the body, \
         do not retune the expect toward 1.0"
    );
    // A (different) body size still exists; that is the M 3.85 / M 4.54 split.
    assert!(self_consistent_capture_area_m2(PROPOSED_M4_CRUISE_MACH).is_some());

    std::println!(
        "proposed M {PROPOSED_M4_CRUISE_MACH:.2} at {h:.0} m; snapshot capture/body = {ratio:.3} (does not close)"
    );
}

/// A DECLARED SNAPSHOT COWL UNLOCKS ADDITIVE DRAG, AND IT IS ZERO AT DESIGN.
///
/// The highlight is the required capture at M 3.50, so the snapshot is
/// shock-on-lip. At M 4.00 that same highlight cannot swallow the streamtube:
/// capture exceeds the cowl, which is a refusal, not a fake Cd.
#[test]
fn snapshot_design_cowl_has_zero_additive_drag_and_refuses_when_capture_exceeds() {
    let at_design = additive_drag_on_snapshot_cowl(3.50).unwrap().unwrap();
    assert!(
        ventus_units::float::abs(at_design.force_n) < 1.0,
        "snapshot shock-on-lip additive drag moved to {} N",
        at_design.force_n
    );
    assert!(at_design.within_stated_range);
    assert_eq!(
        additive_drag_on_snapshot_cowl(PROPOSED_M4_CRUISE_MACH)
            .unwrap()
            .unwrap_err(),
        ventus_inlet::CaptureError::CaptureExceedsCowl
    );
}

/// THE M 4 CANDIDATE'S OWN COWL IS SHOCK-ON-LIP AT M 4.00, SO ADDITIVE DRAG
/// IS ZERO THERE. THAT IS A NUMBER, NOT A CLOSE.
#[test]
fn m4_candidate_design_cowl_has_zero_additive_drag_at_the_proposed_row() {
    let d = additive_drag_on_m4_candidate_cowl(PROPOSED_M4_CRUISE_MACH)
        .unwrap()
        .unwrap();
    assert!(
        ventus_units::float::abs(d.force_n) < 1.0,
        "M 4 candidate shock-on-lip additive drag moved to {} N",
        d.force_n
    );
    assert!(d.within_stated_range);
    let lip = m4_candidate_design_cowl_lip().unwrap();
    let body = m4_candidate_geometry();
    assert!(lip.highlight_area_m2 < body.max_cross_section_m2);
}

/// THE M 4 CANDIDATE HOSTS THE INLET AT THE PROPOSED ROW. THE SNAPSHOT DOES NOT.
///
/// A fatter Sears-Haack (fineness 10, same length) pays more wave drag and
/// still has `A_c / A_body < 1` at M 4.00, because the extra station more
/// than covers the extra drag. That is the geometry path, not a close:
/// operative φ is still below 0.50, and this is not the M 3.50 yardstick.
/// Additive drag on the candidate's own cowl is a separate assertion.
#[test]
fn m4_candidate_geometry_hosts_capture_at_the_proposed_m4() {
    let snapshot = snapshot_geometry();
    let candidate = m4_candidate_geometry();
    assert!(candidate.max_cross_section_m2 > snapshot.max_cross_section_m2);
    assert!((candidate.length_m - snapshot.length_m).abs() < 1e-12);

    let snapshot_ratio = capture_area_ratio(PROPOSED_M4_CRUISE_MACH).unwrap();
    let candidate_ratio = m4_candidate_capture_area_ratio(PROPOSED_M4_CRUISE_MACH).unwrap();
    assert!(
        snapshot_ratio > 1.0,
        "snapshot ratio at M 4 moved to {snapshot_ratio:.3}; the candidate test is not a replacement of that bind"
    );
    assert!(
        candidate_ratio < 1.0,
        "M 4 candidate still cannot host the inlet: ratio {candidate_ratio:.3}"
    );
    let a_c = required_capture_area_for(PROPOSED_M4_CRUISE_MACH, &candidate).unwrap();
    assert!(ventus_inlet::body_can_host_capture(a_c, candidate.max_cross_section_m2).unwrap());

    // Same chain, same Mach: the only change is the station. If this ever
    // passed because the cycle invented thrust, the snapshot ratio would
    // have fallen too.
    assert!(candidate_ratio < snapshot_ratio);

    // Pinned so a later "close" cannot hide inside "< 1". Fineness 10 on this
    // length gives ~0.864; if it drifts to 0.99 the candidate is hanging by
    // a rounding, and if it goes above 1 the geometry path has closed.
    assert!(
        (0.85..0.88).contains(&candidate_ratio),
        "M 4 candidate capture/body moved to {candidate_ratio:.4}"
    );

    std::println!(
        "M 4 candidate A_body = {:.4} m2 (snapshot {:.4}); A_c = {a_c:.4} m2; capture/body = {candidate_ratio:.4} (hosts)",
        candidate.max_cross_section_m2,
        snapshot.max_cross_section_m2
    );
}


// ---------------------------------------------------------------------------
// ADR-006: cooled-liner candidate combustor. The snapshot is untouched; these
// pin what the candidate buys and what it costs.
// ---------------------------------------------------------------------------

fn candidate_phi(mach: f64) -> f64 {
    point_equivalence_ratio(&evaluate_with_combustor(mach, &COOLED_LINER_CANDIDATE)).unwrap()
}

#[test]
fn the_two_gas_models_agree_on_the_snapshot_cycle() {
    // Same 1700 K burner, cubic vs JANAF: the equivalence ratio moves 0.45 %.
    let cubic = point_equivalence_ratio(&evaluate(3.50)).unwrap();
    let janaf = point_equivalence_ratio(&evaluate_with_combustor(
        3.50,
        &Combustor { exit_limit_k: BURNER_EXIT_LIMIT_K, gas: ventus_propulsion::ramjet::GasModel::Janaf },
    ))
    .unwrap();
    assert!(((janaf - cubic) / cubic).abs() < 0.006, "cubic {cubic} janaf {janaf}");
}

#[test]
fn cooled_liner_candidate_holds_a_flame_at_m350() {
    let phi = candidate_phi(3.50);
    assert!((phi - 0.6698).abs() < 5e-4, "phi {phi}");
    assert_eq!(operative_lean_blowout_verdict(phi), Some(LeanBlowoutVerdict::AboveOperativeBound));
    // Above both Useller Fig. 8 points, the 6-foot 0.50 and the 3-foot 0.63.
    assert!(phi > USELLER_FIG_8_PHI_SHORT_CHAMBER);
    // The snapshot still does not, and must not be changed by this.
    let snap = point_equivalence_ratio(&evaluate(3.50)).unwrap();
    assert_eq!(operative_lean_blowout_verdict(snap), Some(LeanBlowoutVerdict::BelowOperativeBound));
}

#[test]
fn cooled_liner_candidate_closes_flame_and_capture_at_m400_on_the_snapshot_body() {
    let phi = candidate_phi(PROPOSED_M4_CRUISE_MACH);
    assert!((phi - 0.5856).abs() < 5e-4, "phi {phi}");
    assert_eq!(operative_lean_blowout_verdict(phi), Some(LeanBlowoutVerdict::AboveOperativeBound));
    let g = snapshot_geometry();
    let ratio = required_capture_area_with_combustor(PROPOSED_M4_CRUISE_MACH, &g, &COOLED_LINER_CANDIDATE)
        .unwrap()
        / g.max_cross_section_m2;
    // 1.146 at 1700 K; the hotter burner needs less air for the same drag.
    assert!((ratio - 0.793).abs() < 2e-3, "capture/body {ratio}");
    assert!(ratio < 1.0);
}

#[test]
fn cooled_liner_candidate_at_m425_is_inside_both_bounds_with_little_room() {
    let phi = candidate_phi(4.25);
    assert!((phi - 0.538).abs() < 1e-3, "phi {phi}");
    assert_eq!(operative_lean_blowout_verdict(phi), Some(LeanBlowoutVerdict::AboveOperativeBound));
    let g = snapshot_geometry();
    let ratio = required_capture_area_with_combustor(4.25, &g, &COOLED_LINER_CANDIDATE).unwrap()
        / g.max_cross_section_m2;
    assert!((ratio - 0.96).abs() < 5e-3, "capture/body {ratio}");
}

#[test]
fn cooled_liner_candidate_pays_in_specific_impulse() {
    // Breguet range is linear in Isp at fixed V, L/D and mass fractions, so this
    // ratio IS the range cost at M 3.50: about 6.5 %.
    let snap = evaluate(3.50).ramjet_specific_impulse_s.unwrap();
    let cand = evaluate_with_combustor(3.50, &COOLED_LINER_CANDIDATE)
        .ramjet_specific_impulse_s
        .unwrap();
    let ratio = cand / snap;
    assert!((ratio - 0.9355).abs() < 2e-3, "Isp ratio {ratio} ({cand} / {snap})");
}


/// Thrust over drag along the constant-q climb, with the inlet sized for the
/// cruise Mach: `T / D = A_c,design / A_c,required(M)`.
///
/// Open means T/D >= 1 at every Mach from M 1.6 to cruise, so a vehicle that
/// reaches ramjet speed accelerates to cruise on its own. Scope, stated: the
/// four-ramp inlet is re-optimised at every Mach (variable geometry), additive
/// spillage drag is not in the balance, and below M 1.6 is not asked -- a
/// ramjet needs a booster to get there.
fn min_thrust_over_drag_on_climb(cruise: f64, c: &Combustor) -> (f64, f64) {
    let g = snapshot_geometry();
    let a_design = required_capture_area_with_combustor(cruise, &g, c).unwrap();
    let mut worst = (f64::INFINITY, 0.0);
    // Integer steps so float accumulation cannot drop the cruise point, which is
    // then asked explicitly.
    let steps = ((cruise - 1.6) / 0.2) as usize;
    let machs = (0..=steps).map(|i| 1.6 + 0.2 * i as f64).filter(|m| *m < cruise).chain([cruise]);
    for m in machs {
        let a = required_capture_area_with_combustor(m, &g, c).unwrap();
        let ratio = a_design / a;
        if ratio < worst.0 {
            worst = (ratio, m);
        }
    }
    worst
}

#[test]
fn candidate_climb_corridor_is_open_to_m400() {
    let (ratio, at) = min_thrust_over_drag_on_climb(PROPOSED_M4_CRUISE_MACH, &COOLED_LINER_CANDIDATE);
    // The pinch is the cruise point itself, where the inlet was sized.
    assert!(ratio >= 1.0 - 1e-9, "T/D {ratio} at M {at}");
    assert!((at - PROPOSED_M4_CRUISE_MACH).abs() < 0.11, "pinch moved below cruise: M {at}");
}

#[test]
fn candidate_climb_corridor_is_open_to_m350() {
    let (ratio, at) = min_thrust_over_drag_on_climb(3.50, &COOLED_LINER_CANDIDATE);
    assert!(ratio >= 1.0 - 1e-9, "T/D {ratio} at M {at}");
    assert!((at - 3.50).abs() < 0.11, "pinch moved below cruise: M {at}");
}

/// `design-point-m4.md` must-have 5: thermally perfect T0 at the proposed row,
/// by the method of `design-point.md` 3.1 (h0 = h + V^2/2), from JANAF.
#[test]
fn proposed_m4_row_thermally_perfect_stagnation_temperature() {
    let m = PROPOSED_M4_CRUISE_MACH;
    let alt = altitude_for_constant_q_m(m, DESIGN_DYNAMIC_PRESSURE_PA).unwrap();
    let a = ventus_atmos::at_geopotential(alt).unwrap();
    let v = m * a.speed_of_sound_m_s;
    let t0_perfect = a.temperature_k * ventus_gasdyn::stagnation_temperature_ratio(m, 1.4).unwrap();
    let t0 = ventus_gasdyn::stagnation_temperature_thermally_perfect_k(a.temperature_k, v).unwrap();
    std::println!(
        "M 4.00 row: h={alt:.1} m T={:.2} K V={v:.2} m/s T0(gamma 1.4)={t0_perfect:.2} K T0(JANAF)={t0:.2} K delta={:.2} K",
        a.temperature_k,
        t0 - t0_perfect
    );
    assert!((t0_perfect - 942.5).abs() < 0.05, "gamma-1.4 T0 {t0_perfect} vs the doc's 942.5 K");
    assert!((t0 - MEASURED_M4_T0_K).abs() < 0.05, "T0 {t0}");
    // The correction lowers T0, and by more than at M 3.50 (-15.3 K).
    assert!(t0 < t0_perfect - 15.3);
}

/// Measured from JANAF: 911.29 K, 31.2 K below the gamma = 1.4 answer. The cycle
/// takes T02 from the gamma = 1.4 ratio, so it over-states burner entry
/// temperature here and under-states the heating room: the candidate's phi at
/// M 4.00 is conservative on this account.
const MEASURED_M4_T0_K: f64 = 911.29;


/// Capture/body on the snapshot body when a fraction `x` of the airflow bypasses
/// the flame to cool the liner and rejoins it before the nozzle (ADR-006).
///
/// The core still burns to the candidate limit, so the flame-zone phi is the
/// candidate's own; the nozzle sees the mixed stream, whose temperature comes from
/// an enthalpy balance on JANAF air: `h_mix = (1 - x) h(T4) + x h(T02)`. Cooling-air
/// pressure loss is not charged. No cited cooling fraction exists here, so this
/// pins the BOUNDARY, not a design value.
fn capture_ratio_with_cooling_air(mach: f64, x: f64) -> f64 {
    use ventus_gasdyn::enthalpy_air_janaf_j_kg as h;
    let t02 = evaluate_with_combustor(mach, &COOLED_LINER_CANDIDATE)
        .burner_entry_total_temperature_k
        .unwrap();
    let target = (1.0 - x) * h(COOLED_LINER_CANDIDATE_EXIT_K).unwrap() + x * h(t02).unwrap();
    let (mut lo, mut hi) = (t02, COOLED_LINER_CANDIDATE_EXIT_K);
    for _ in 0..100 {
        let mid = 0.5 * (lo + hi);
        if h(mid).unwrap() < target {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let mixed = Combustor { exit_limit_k: 0.5 * (lo + hi), gas: ventus_propulsion::ramjet::GasModel::Janaf };
    let g = snapshot_geometry();
    required_capture_area_with_combustor(mach, &g, &mixed).unwrap() / g.max_cross_section_m2
}

#[test]
fn m350_closes_with_thirty_percent_cooling_air() {
    let r = capture_ratio_with_cooling_air(3.50, 0.30);
    assert!((r - 0.735).abs() < 2e-3 && r < 1.0, "capture/body {r}");
}

#[test]
fn m400_on_the_snapshot_body_tolerates_between_twenty_and_twenty_five_percent_cooling_air() {
    let at_20 = capture_ratio_with_cooling_air(PROPOSED_M4_CRUISE_MACH, 0.20);
    let at_25 = capture_ratio_with_cooling_air(PROPOSED_M4_CRUISE_MACH, 0.25);
    assert!((at_20 - 0.956).abs() < 2e-3 && at_20 < 1.0, "20 %: {at_20}");
    assert!((at_25 - 1.009).abs() < 2e-3 && at_25 > 1.0, "25 %: {at_25}");
}

/// Sustained M 4.00 on the constant-q row: snapshot body (which hosts the capture once the
/// ADR-006 candidate burns), candidate combustor. The drag is M6b's breakdown at the row, the same
/// call that sizes the inlet, so thrust equals this drag at cruise by construction.
///
/// L/D falls from 5.122 at M 3.50 to 4.578: lift gets dearer with `sqrt(M^2 - 1)` at the same
/// C_L. It stays under Kuchemann's 7.00 and above M6b's 3.5 bug floor. It is **below** the 5.0-6.0
/// target M 3.50 was held to, and below that band scaled by the Kuchemann ratio (4.71-5.65);
/// `design-point-m4.md` said the target would have to be restated, and this is the number it has
/// to be restated against, not a pass.
#[test]
fn m400_cruise_lift_to_drag_on_the_row() {
    let g = snapshot_geometry();
    let c35 = cruise_at_row(3.50, &g, &COOLED_LINER_CANDIDATE).unwrap();
    let c40 = cruise_at_row(PROPOSED_M4_CRUISE_MACH, &g, &COOLED_LINER_CANDIDATE).unwrap();
    assert!((c35.lift_to_drag() - 5.1220).abs() < 5e-4, "M 3.50 L/D {}", c35.lift_to_drag());
    assert!((c40.lift_to_drag() - 4.5783).abs() < 5e-4, "M 4.00 L/D {}", c40.lift_to_drag());
    assert!(c40.lift_to_drag() < ventus_aero::drag::kuchemann_bound(PROPOSED_M4_CRUISE_MACH));
    assert!(c40.lift_to_drag() > 3.5, "below M6b's bug floor");
    assert!(c40.drag.lift_induced > c40.drag.wave + c40.drag.friction, "drag due to lift still dominates");
    assert!((c40.altitude_m - 27_747.0).abs() < 1.0 && (c40.velocity_m_s - 1201.20).abs() < 0.01);
}

/// Two Isp figures live in this repository and they disagree. `ventus-mass` carries 1450 s,
/// commented "M4, ramjet at the design point" but in fact the midpoint of a recited 900-2000 s
/// band (`c04da1d`, case `specific_impulse_in_the_published_ramjet_band`, a known limit), and the published M 3.50 range (4265-5714 km snapshot,
/// 3990-5345 km candidate) was computed with it and with L/D 5.5, the M6b target. The chain's own
/// cycle gives 1966.8 s (snapshot) and 1839.9 s (candidate) at M 3.50, meaningful by
/// `ramjet_isp_is_meaningful`, and M6b's breakdown gives L/D 5.122. Which Isp is right is not
/// decided here: 1450 has no source beyond a comment, and the cycle is an efficiency-factor model
/// whose two factors are themselves [TO CITE]. **[KNOWN_LIMIT]**, pinned so a reconciliation has to
/// change this test and say so.
#[test]
fn the_two_isp_figures_in_the_chain_disagree() {
    let model = evaluate(3.50).ramjet_specific_impulse_s.unwrap();
    assert!((model - 1966.8).abs() < 0.5, "snapshot cycle Isp {model}");
    assert!(model / 1450.0 > 1.3, "the disagreement closed; reconcile the range and this test");
}

/// Range at M 4.00, both ways, with the two reserves M 3.50 was published with (500 kg, and the
/// SR-71's own 11.3 % scaled). Zero-fuel mass and cruise mass are M7's, unchanged.
///
/// * **Published basis**: the M 3.50 constants carried to M 4.00 by the chain's own ratios --
///   L/D x (4.5783 / 5.1220), Isp x (candidate M 4.00 / snapshot M 3.50) -- at the M 4.00 speed.
///   Comparable with the published 3990-5345 km.
/// * **Model basis**: the chain's L/D and Isp at the row, M 3.50 and M 4.00 alike.
///
/// On either basis M 4.00 flies about 4 % further than M 3.50 on the candidate: +14.7 % speed and
/// +1.6 % Isp against -10.6 % L/D. Still reserve-limited; no reserve policy chosen.
#[test]
fn m400_range_both_bases() {
    use ventus_mass::{close_cruise, sr71_reserve_fraction_of_cruise_mass, VENTUS1_CRUISE_MASS_KG, VENTUS1_ZERO_FUEL_MASS_KG};
    let g = snapshot_geometry();
    let c35 = cruise_at_row(3.50, &g, &COOLED_LINER_CANDIDATE).unwrap();
    let c40 = cruise_at_row(PROPOSED_M4_CRUISE_MACH, &g, &COOLED_LINER_CANDIDATE).unwrap();
    let snap_isp35 = evaluate(3.50).ramjet_specific_impulse_s.unwrap();
    let reserves = [500.0, sr71_reserve_fraction_of_cruise_mass() * VENTUS1_CRUISE_MASS_KG];
    let km = |v: f64, ld: f64, isp: f64, r: f64| {
        close_cruise(VENTUS1_CRUISE_MASS_KG, VENTUS1_ZERO_FUEL_MASS_KG, r, v, ld, isp).unwrap().range_m / 1000.0
    };
    let published: [f64; 2] = reserves
        .map(|r| km(c40.velocity_m_s, 5.5 * c40.lift_to_drag() / c35.lift_to_drag(), 1450.0 * c40.specific_impulse_s / snap_isp35, r));
    let model35: [f64; 2] = reserves.map(|r| km(c35.velocity_m_s, c35.lift_to_drag(), c35.specific_impulse_s, r));
    let model40: [f64; 2] = reserves.map(|r| km(c40.velocity_m_s, c40.lift_to_drag(), c40.specific_impulse_s, r));
    std::println!("M 4.00 range, published basis: {:.0} / {:.0} km (reserve 500 kg / 11.3 %)", published[0], published[1]);
    std::println!("model basis: M 3.50 {:.0} / {:.0} km, M 4.00 {:.0} / {:.0} km", model35[0], model35[1], model40[0], model40[1]);
    // The basis is the published one only if it reproduces the published M 3.50 candidate band.
    let back: [f64; 2] = reserves.map(|r| km(c35.velocity_m_s, 5.5, 1450.0 * c35.specific_impulse_s / snap_isp35, r));
    assert!((back[0] - 5345.0).abs() < 5.0 && (back[1] - 3990.0).abs() < 5.0, "published basis at M 3.50: {back:?}");
    for (got, want) in published.iter().zip([PUBLISHED_BASIS_M4_KM.0, PUBLISHED_BASIS_M4_KM.1]) {
        assert!((got - want).abs() < 2.0, "published basis {got} vs {want}");
    }
    for (got, want) in model40.iter().zip([MODEL_BASIS_M4_KM.0, MODEL_BASIS_M4_KM.1]) {
        assert!((got - want).abs() < 2.0, "model basis {got} vs {want}");
    }
    for i in 0..2 {
        let gain = model40[i] / model35[i];
        assert!((1.03..1.06).contains(&gain), "M 4 / M 3.5 range {gain}");
    }
}

/// Pinned after the first run of `m400_range_both_bases` (km, reserve 500 kg then 11.3 %).
const PUBLISHED_BASIS_M4_KM: (f64, f64) = (5569.1, 4157.0);
const MODEL_BASIS_M4_KM: (f64, f64) = (7035.0, 5251.5);
