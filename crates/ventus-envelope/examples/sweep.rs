//! M12: the Mach sweep, and where each module stops answering.
//!
//!   cargo run --release -p ventus-envelope --example sweep
//!
//! Release, not debug: M3's ramp optimiser costs a quarter of a second per point
//! and dominates everything else in this workspace.

use ventus_envelope::{
    envelope, evaluate, Envelope, DESIGN_DYNAMIC_PRESSURE_PA, LEAN_BLOWOUT_CROSSING_MACH,
    PEAK_SPECIFIC_THRUST_MACH, PEAK_SPECIFIC_THRUST_N_S_KG,
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

    println!("\n[KNOWN_LIMIT] The M4 ceiling above is not the real end of the ramjet.");
    println!("  ideal_ramjet has no flame stability model and will run at a fuel-air");
    println!("  ratio of 0.0006. Kerosene-air lean blowout sits near f/a = 0.027");
    println!("  [TO CITE], which this sweep crosses at M {LEAN_BLOWOUT_CROSSING_MACH:.2} -");
    println!("  0.41 Mach above the design point, not 2.2 above it.");
}
