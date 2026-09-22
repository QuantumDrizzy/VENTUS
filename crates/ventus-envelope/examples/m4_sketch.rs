//! Dump the proposed M 4.00 constant-q row so `docs/design-point-m4.md` can be
//! regenerated rather than remembered.
//!
//!   cargo run --release -p ventus-envelope --example m4_sketch
//!
//! This is NOT a design-point close. It prints the same M12 / M1 / M2 / M3 / M4
//! chain that already exists, at the programme cruise Mach, and it says where
//! that chain already refuses to describe a self-consistent aircraft.

use ventus_aero::boundary_layer::{EdgeState, Regime, PRANDTL_AIR};
use ventus_envelope::{
    altitude_for_constant_q_m, capture_area_ratio, evaluate, required_capture_area_m2,
    self_consistent_capture_area_m2, CAPTURE_AREA_CLOSES_AT_MACH, DESIGN_DYNAMIC_PRESSURE_PA,
    DESIGN_POINT_EQUIVALENCE_RATIO, DESIGN_RAMP_COUNT, LEAN_BLOWOUT_PHI_MAX, LEAN_BLOWOUT_PHI_MIN,
    NO_BODY_CLOSES_ABOVE_MACH, PEAK_SPECIFIC_THRUST_MACH, PEAK_SPECIFIC_THRUST_N_S_KG,
    PROPOSED_M4_CRUISE_MACH, STOICHIOMETRIC_FUEL_AIR_RATIO,
};
use ventus_gasdyn::{normal_shock, stagnation_pressure_ratio, stagnation_temperature_ratio};
use ventus_inlet::shock_train::mil_e_5008b_recovery;
use ventus_thermal::radiation_equilibrium_wall;
use ventus_units::constants::G0_M_S2;

const GAMMA: f64 = 1.4;

fn main() {
    println!("VENTUS M 4 sketch dump — proposed re-baseline, NOT a close\n");
    println!("q rule: hold DESIGN_DYNAMIC_PRESSURE_PA = {DESIGN_DYNAMIC_PRESSURE_PA} Pa");
    println!("  (the same rule that picked 26 km for M 3.50; q = 0.7 p M^2)\n");

    dump_row("validated snapshot", 3.5);
    dump_row("proposed M 4 (this sketch)", PROPOSED_M4_CRUISE_MACH);

    let h4 =
        altitude_for_constant_q_m(PROPOSED_M4_CRUISE_MACH, DESIGN_DYNAMIC_PRESSURE_PA).unwrap();
    let at_26 = ventus_atmos::at_geopotential(26_000.0).unwrap();
    let q_if_held_26km =
        0.7 * at_26.pressure_pa * PROPOSED_M4_CRUISE_MACH * PROPOSED_M4_CRUISE_MACH;
    println!("\nCounterfactual: M 4.00 held at 26 km instead of constant q");
    println!(
        "  q = {:.3} kPa  ({:+.1} % vs design q) — a different structural case;",
        q_if_held_26km / 1000.0,
        100.0 * (q_if_held_26km / DESIGN_DYNAMIC_PRESSURE_PA - 1.0)
    );
    println!("  the project does not pick this. Constant-q altitude is {h4:.2} m.");

    println!("\nGeometry / propulsion blockers already on the books (current config)");
    println!("  peak specific thrust            M {PEAK_SPECIFIC_THRUST_MACH:.2}");
    println!("  design-point phi                {DESIGN_POINT_EQUIVALENCE_RATIO:.4}");
    println!("  lean-blowout literature band (phi 0.50-0.30)  M 3.23 — 4.42");
    println!("           operative bound = strict end 0.50 (no holder declared)  [TO VERIFY]");
    println!("  capture area = body             M {CAPTURE_AREA_CLOSES_AT_MACH:.3}");
    println!("  no body closes (inside that)    M {NO_BODY_CLOSES_ABOVE_MACH:.3}");
    println!("  four-ramp model still answers   M 5.65 — not aircraft capability");
}

fn dump_row(label: &str, mach: f64) {
    let h = altitude_for_constant_q_m(mach, DESIGN_DYNAMIC_PRESSURE_PA).unwrap();
    let atmos = ventus_atmos::at_geopotential(h).unwrap();
    let p = evaluate(mach);
    let t0_ratio = stagnation_temperature_ratio(mach, GAMMA).unwrap();
    let t0 = atmos.temperature_k * t0_ratio;
    let p0_ratio = stagnation_pressure_ratio(mach, GAMMA).unwrap();
    let q = 0.7 * atmos.pressure_pa * mach * mach;
    let mil = mil_e_5008b_recovery(mach);
    let nshock = normal_shock(mach, GAMMA).unwrap();
    let geometry = ventus_aero::geometry::ventus1(DESIGN_DYNAMIC_PRESSURE_PA);

    println!("--- {label}: M {mach:.2} ---");
    println!("  h_geopotential          {h:.2} m");
    println!(
        "  h_geometric             {:.2} m",
        atmos.geometric_altitude_m
    );
    println!("  T_inf                   {:.4} K", atmos.temperature_k);
    println!("  p_inf                   {:.4} Pa", atmos.pressure_pa);
    println!("  rho_inf                 {:.8} kg/m3", atmos.density_kg_m3);
    println!(
        "  a_inf                   {:.4} m/s",
        atmos.speed_of_sound_m_s
    );
    println!(
        "  mu_inf                  {:.8e} Pa.s",
        atmos.dynamic_viscosity_pa_s
    );
    println!(
        "  V_inf                   {:.4} m/s",
        p.velocity_m_s.unwrap()
    );
    println!(
        "  q_inf                   {:.4} Pa  ({:.3} kPa)",
        q,
        q / 1000.0
    );
    println!("  p0/p (isen, g=1.4)      {p0_ratio:.5}");
    println!("  T0 (g=1.4)              {t0:.2} K");
    println!("  T4max/T0 (1700 K / T0)  {:.3}", 1700.0 / t0);
    println!(
        "  Re/L                    {:.6e} 1/m",
        atmos.density_kg_m3 * p.velocity_m_s.unwrap() / atmos.dynamic_viscosity_pa_s
    );
    println!(
        "  V^2/2                   {:.2} kJ/kg",
        0.5 * p.velocity_m_s.unwrap() * p.velocity_m_s.unwrap() / 1000.0
    );
    println!("  MIL-E-5008B pi_d        {mil:.5}");
    println!(
        "  normal-shock p02/p01    {:.5}",
        nshock.stagnation_pressure_ratio
    );
    println!(
        "  gain over normal shock  {:.3}x",
        mil / nshock.stagnation_pressure_ratio
    );
    println!(
        "  {DESIGN_RAMP_COUNT}-ramp inlet recovery  {:.5}",
        p.inlet_recovery.unwrap()
    );
    println!(
        "  Fs                      {:.2} N.s/kg  ({:.1} % of peak {:.1})",
        p.ramjet_specific_thrust_n_s_kg.unwrap(),
        100.0 * p.ramjet_specific_thrust_n_s_kg.unwrap() / PEAK_SPECIFIC_THRUST_N_S_KG,
        PEAK_SPECIFIC_THRUST_N_S_KG
    );
    let fuel_air =
        p.ramjet_specific_thrust_n_s_kg.unwrap() / (p.ramjet_specific_impulse_s.unwrap() * G0_M_S2);
    let phi = fuel_air / STOICHIOMETRIC_FUEL_AIR_RATIO;
    println!("  f/a                     {fuel_air:.6}");
    println!(
        "  phi                     {phi:.4}   (literature band {LEAN_BLOWOUT_PHI_MIN:.2}–{LEAN_BLOWOUT_PHI_MAX:.2}; operative = strict, no holder)"
    );
    let edge = EdgeState {
        temperature_k: atmos.temperature_k,
        pressure_pa: atmos.pressure_pa,
        velocity_m_s: p.velocity_m_s.unwrap(),
        mach,
        gamma: GAMMA,
    };
    for x in [1.0_f64, 10.0] {
        let b = radiation_equilibrium_wall(&edge, x, 0.85, 0.0, PRANDTL_AIR, Regime::Turbulent)
            .unwrap();
        println!(
            "  Tw x={x:.0} m radiating     {:.2} K ({:.1} C)  T_aw={:.1} K",
            b.wall_temperature_k,
            b.wall_temperature_k - 273.15,
            b.film.adiabatic_wall_temperature_k
        );
    }
    println!(
        "  A_c required            {:.4} m2",
        required_capture_area_m2(mach).unwrap()
    );
    println!(
        "  A_body (M6b)            {:.4} m2",
        geometry.max_cross_section_m2
    );
    println!(
        "  A_c / A_body            {:.4}   (closes at 1.0 when M = {CAPTURE_AREA_CLOSES_AT_MACH})",
        capture_area_ratio(mach).unwrap()
    );
    match self_consistent_capture_area_m2(mach) {
        Some(a) => println!("  A_c self-consistent     {a:.4} m2  (a body size still exists)"),
        None => println!("  A_c self-consistent     none  (no body closes)"),
    }
    println!();
}
