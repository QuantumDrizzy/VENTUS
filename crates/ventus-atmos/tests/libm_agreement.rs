//! ADR-001 asked for the `libm` versus platform-`std` disagreement to be
//! quantified rather than assumed. This is that measurement.
//!
//! `std`'s `powf`/`exp`/`sqrt` forward to the platform math library (MSVC CRT
//! here, glibc on Linux, Apple libm on macOS). `libm` is pure Rust and
//! identical everywhere. The point of this test is NOT that they agree — it is
//! to record by how much they differ, so that if a validation report ever moves
//! in its last digits, this number says whether the math library could explain
//! it.
//!
//! The measured agreement is printed by `cargo test -- --nocapture`.

use ventus_units::float::ulp_diff;

/// The atmosphere is evaluated over its whole domain, at the exponents US76
/// actually uses.
fn us76_pow_arguments() -> Vec<(f64, f64)> {
    // (base, exponent) pairs: Tb/T for each gradient layer, exponent g0*M0/(R*L).
    let g0m_over_r = ventus_atmos::G0_M_OVER_R;
    let mut args = Vec::new();
    for (tb, lapse, span) in [
        (288.15_f64, -6.5e-3_f64, 11_000.0_f64),
        (216.65, 1.0e-3, 12_000.0),
        (228.65, 2.8e-3, 15_000.0),
        (270.65, -2.8e-3, 20_000.0),
        (214.65, -2.0e-3, 13_852.0),
    ] {
        for i in 0..=1000 {
            let dz = span * f64::from(i) / 1000.0;
            let t = tb + lapse * dz;
            args.push((tb / t, g0m_over_r / lapse));
        }
    }
    args
}

#[test]
fn quantify_libm_versus_platform_std() {
    let mut worst_pow = 0_u64;
    let mut worst_pow_at = (0.0, 0.0);
    for (base, exp) in us76_pow_arguments() {
        let a = libm::pow(base, exp);
        let b = base.powf(exp);
        let d = ulp_diff(a, b).expect("no NaN in the US76 domain");
        if d > worst_pow {
            worst_pow = d;
            worst_pow_at = (base, exp);
        }
    }

    let mut worst_exp = 0_u64;
    let mut worst_sqrt = 0_u64;
    for i in 0..=20_000 {
        let x = -3.0 + 6.0 * f64::from(i) / 20_000.0;
        worst_exp = worst_exp.max(ulp_diff(libm::exp(x), x.exp()).unwrap());

        let t = 180.0 + (300.0 - 180.0) * f64::from(i) / 20_000.0;
        worst_sqrt = worst_sqrt.max(ulp_diff(libm::sqrt(t), t.sqrt()).unwrap());
    }

    println!("libm vs platform std over the US76 domain:");
    println!(
        "  pow  worst {worst_pow} ULP  at base {:.6}, exp {:.3}",
        worst_pow_at.0, worst_pow_at.1
    );
    println!("  exp  worst {worst_exp} ULP");
    println!("  sqrt worst {worst_sqrt} ULP");

    // sqrt is exactly specified by IEEE-754, so any difference at all would mean
    // one of the two is not doing what the standard requires.
    assert_eq!(
        worst_sqrt, 0,
        "sqrt is IEEE-754 exact; a difference is a bug"
    );

    // pow and exp are not required to be correctly rounded. A handful of ULP is
    // expected and harmless; a large disagreement would mean one of the two is
    // broken and the atmosphere cannot be trusted on this platform.
    assert!(
        worst_pow <= 4,
        "pow disagrees by {worst_pow} ULP, which is too much to ignore"
    );
    assert!(
        worst_exp <= 4,
        "exp disagrees by {worst_exp} ULP, which is too much to ignore"
    );
}

/// The consequence that actually matters: how far apart are the two atmospheres?
#[test]
fn the_two_math_libraries_give_the_same_atmosphere_to_well_inside_tolerance() {
    let mut worst_rel = 0.0_f64;
    let mut worst_at = 0.0_f64;

    for i in 0..=8485 {
        let h = f64::from(i) * 10.0;
        let s = ventus_atmos::at_geopotential(h).unwrap();

        // Recompute the pressure with the platform math library instead.
        let layer = &ventus_atmos::layers::LAYERS[ventus_atmos::layers::layer_index(h)];
        let dz = h - layer.base_geopotential_altitude_m;
        let std_pressure = if layer.lapse_rate_k_per_m == 0.0 {
            layer.base_pressure_pa
                * (-ventus_atmos::G0_M_OVER_R * dz / layer.base_temperature_k).exp()
        } else {
            let t = layer.base_temperature_k + layer.lapse_rate_k_per_m * dz;
            layer.base_pressure_pa
                * (layer.base_temperature_k / t)
                    .powf(ventus_atmos::G0_M_OVER_R / layer.lapse_rate_k_per_m)
        };

        let rel = ventus_units::float::rel_err(s.pressure_pa, std_pressure);
        if rel > worst_rel {
            worst_rel = rel;
            worst_at = h;
        }
    }

    println!("worst pressure disagreement between libm and platform std: {worst_rel:.3e} at {worst_at} m");

    // M1's acceptance tolerance is 1e-6. The choice of math library must be
    // orders of magnitude below that, or ADR-001 has a real cost to declare.
    assert!(
        worst_rel < 1e-12,
        "math library choice moves the atmosphere by {worst_rel:e}, which is not negligible \
         against M1's 1e-6 acceptance tolerance"
    );
}
