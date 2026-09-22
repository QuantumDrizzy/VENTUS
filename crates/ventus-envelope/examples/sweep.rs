//! M12: the Mach sweep, and where each module stops answering.
//!
//!   cargo run --release -p ventus-envelope --example sweep
//!
//! Release, not debug: M3's ramp optimiser costs a quarter of a second per point
//! and dominates everything else in this workspace.

use ventus_envelope::{
    capture_area_ratio, envelope, evaluate, lean_blowout_mach, Envelope,
    CAPTURE_AREA_CLOSES_AT_MACH, DESIGN_DYNAMIC_PRESSURE_PA, DESIGN_POINT_EQUIVALENCE_RATIO,
    LEAN_BLOWOUT_PHI_MAX, LEAN_BLOWOUT_PHI_MIN, NO_BODY_CLOSES_ABOVE_MACH,
    OPERATIVE_LEAN_BLOWOUT_PHI, PEAK_SPECIFIC_THRUST_MACH, PEAK_SPECIFIC_THRUST_N_S_KG,
    WAVE_DRAG_FRACTION_AT_DESIGN_POINT,
};

fn opt(v: Option<f64>, width: usize, prec: usize) -> String {
    match v {
        Some(x) => format!("{x:>width$.prec$}"),
        None => format!("{:>width$}", "-"),
    }
}

fn main() {
    println!(
        "M12 regime sweep, constant q = {:.0} Pa (the rule that picked 26 km)\n",
        DESIGN_DYNAMIC_PRESSURE_PA
    );
    println!(
        "{:>5} {:>8} {:>8} {:>9} {:>7} {:>11} {:>7} {:>26}  refused",
        "Mach", "h [km]", "T0 [K]", "gamma(T0)", "pi_d", "Fs [Ns/kg]", "Tw [K]", "material"
    );
    let rule = "-".repeat(118);
    println!("{rule}");

    // Specific thrust, not specific impulse. Isp diverges as the engine dies -
    // 4020 s at M 5.50 against 1900 s at the design point - because the fuel-air
    // ratio falls to zero faster than the thrust does. Printing it here would
    // read like a discovery and would be an engine producing nothing, very
    // efficiently.
    let mut ruled = false;
    let mut m = 2.0_f64;
    while m <= 6.5001 {
        let p = evaluate(m);

        // Once anything has refused, the remaining columns are diagnostics: they
        // say which module is still working, not what the aircraft does.
        if !ruled && p.refusal_count() > 0 {
            let mark = "=".repeat(40);
            println!(
                "{mark}  below this line the chain is incomplete: numbers are diagnostic, not claims"
            );
            ruled = true;
        }

        let refused: Vec<&str> = p.refusals.iter().flatten().map(|r| r.module()).collect();
        println!(
            "{:>5.2} {} {} {} {} {} {} {:>26}  {}",
            p.mach,
            opt(p.altitude_m.map(|a| a / 1000.0), 8, 2),
            opt(p.stagnation_temperature_k, 8, 1),
            opt(p.gamma_at_stagnation, 9, 4),
            opt(p.inlet_recovery, 7, 4),
            opt(p.ramjet_specific_thrust_n_s_kg, 11, 1),
            opt(p.wall_temperature_k, 7, 1),
            p.lightest_material.unwrap_or("-"),
            refused.join(", ")
        );
        m += 0.25;
    }

    // One pass, reused for every question below.
    let e: Envelope = envelope(2.0, 9.0, 0.05);

    println!("\nFirst refusal by module, single pass at 0.05 Mach:");
    for r in Envelope::ORDER {
        match e.first_refusal(r) {
            Some(m) => println!("  M {m:.2}  {:<14} {}", r.module(), r.reason()),
            None => println!("  never   {:<14} answers everywhere up to M 9", r.module()),
        }
    }

    if let Some(m) = e.last_fully_answered_mach {
        println!("\nHIGHEST MACH AT WHICH EVERY MODULE STILL ANSWERS: M {m:.2}");
        println!("The design point is M 3.50. That gap is the margin between what");
        println!("has been modelled and what has been claimed.");
    }

    println!("\nThe engine wants a slower aircraft than it is in:");
    println!(
        "  specific thrust peaks at M {PEAK_SPECIFIC_THRUST_MACH:.2}, {PEAK_SPECIFIC_THRUST_N_S_KG:.1} N.s/kg"
    );
    for m in [3.5, 4.5, 5.0] {
        if let Some(fs) = evaluate(m).ramjet_specific_thrust_n_s_kg {
            println!(
                "  M {m:.2} is at {:.1} % of that",
                100.0 * fs / PEAK_SPECIFIC_THRUST_N_S_KG
            );
        }
    }

    // The corridor, which is the thing an engineer looks at first.
    let strict = lean_blowout_mach(LEAN_BLOWOUT_PHI_MAX, 2.0, 5.7, 0.01);
    let permissive = lean_blowout_mach(LEAN_BLOWOUT_PHI_MIN, 2.0, 5.7, 0.01);
    println!();
    println!("THE CORRIDOR THE DESIGN POINT ACTUALLY SITS IN:");
    println!("  M {PEAK_SPECIFIC_THRUST_MACH:.2}   specific thrust peaks");
    println!("  M 3.50   design point, at 83.7 % of peak thrust");
    if let (Some(lo), Some(hi)) = (strict, permissive) {
        println!("  M {lo:.2} to M {hi:.2}   lean blowout literature band, phi {LEAN_BLOWOUT_PHI_MAX:.2} to {LEAN_BLOWOUT_PHI_MIN:.2}");
        println!("           strict end [TO VERIFY] Mattingly Fig. 10-70 class; permissive end needs a holder");
    }
    println!("  M {CAPTURE_AREA_CLOSES_AT_MACH:.2}   required capture area equals the whole body cross-section");
    println!(
        "  M {NO_BODY_CLOSES_ABOVE_MACH:.2}   no body size closes at all (inside the region above, not past it)"
    );
    println!("  M 5.65   every module still answers (four-ramp inlet)");
    println!("  M 5.70   M4 burner ceiling - never the operative limit");

    println!();
    println!("THE TWO THAT ACTUALLY DECIDE IT:");
    println!("  No flame holder is declared. The operative blowout bound is therefore");
    println!("  the strict end phi = {OPERATIVE_LEAN_BLOWOUT_PHI:.2} [TO VERIFY], not the permissive 0.30.");
    println!("  The cycle runs at phi = {DESIGN_POINT_EQUIVALENCE_RATIO:.4} at the design point, INSIDE the");
    println!("  literature band and BELOW the operative bound. Under that bound M 3.50");
    println!("  does not hold a flame. Inventing a holder that saves it is refused.");
    println!("  Decision: cited phi_LBO ≷ 0.4615.");
    if let Some(r) = capture_area_ratio(3.50) {
        println!();
        println!(
            "  And the inlet already needs {:.1} % of the entire body cross-section",
            100.0 * r
        );
        println!("  at the design point, crossing 100 % at M {CAPTURE_AREA_CLOSES_AT_MACH:.2}. Past there the");
        println!("  configuration M6b assumed is self-inconsistent: the Sears-Haack body");
        println!("  that sets the wave drag cannot host an inlet larger than itself.");
        println!();
        println!("  Wave drag goes as A^2, so that is a FIXED POINT, not a formula. It");
        println!(
            "  converges only because wave drag is {:.1} % of the total and",
            100.0 * WAVE_DRAG_FRACTION_AT_DESIGN_POINT
        );
        println!("  lift-induced dominates; had wave dominated, no stable body size");
        println!("  would exist. Solved, the roots vanish entirely above M {NO_BODY_CLOSES_ABOVE_MACH:.2}");
        println!("  - which is NOT a second usable limit. Past M {CAPTURE_AREA_CLOSES_AT_MACH:.2} the inlet already");
        println!("  exceeds the body carrying it, so M {NO_BODY_CLOSES_ABOVE_MACH:.2} sits inside an excluded");
        println!("  region. It says how the failure happens - the solution stops existing");
        println!("  rather than degrading - not how far the aircraft gets.");
    }

    println!();
    println!("[KNOWN_LIMIT] The M 5.70 ceiling is NOT the real end of the ramjet.");
    println!("  ideal_ramjet has no flame stability model and will run at f/a = 0.0006.");
    println!("  M 5.65 is the ceiling of the FOUR-RAMP design inlet, not of the concept.");
    println!("  Four of these five frontiers are about the MODEL. Only the capture area");
    println!("  is a statement about the aircraft.");
}
