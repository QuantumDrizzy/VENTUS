//! Stagnation-point radiation-equilibrium wall at a declared nose / LE radius,
//! for the numbers quoted in docs/design-point.md section 3.2.
//!
//!   cargo run -p ventus-thermal --example stagnation

use ventus_aero::boundary_layer::PRANDTL_AIR;
use ventus_thermal::{
    lightest_survivor, stagnation_radiation_equilibrium, ventus1_leading_edge, ventus1_nose,
    BodyKind, Freestream, VENTUS_LEADING_EDGE_RADIUS_M, VENTUS_NOSE_RADIUS_M,
};

fn main() {
    for (label, h, m) in [
        ("VENTUS-1  M3.50 / 26 km", 26_000.0, 3.5),
        ("SR-71     M3.20 / 24 km", 24_000.0, 3.2),
    ] {
        let a = ventus_atmos::at_geopotential(h).unwrap();
        let fs = Freestream::from_atmos(&a, m, 1.4).unwrap();
        println!(
            "{label}   T_inf = {:.2} K  p_inf = {:.2} Pa  V = {:.2} m/s",
            a.temperature_k, a.pressure_pa, fs.velocity_m_s
        );

        let nose = ventus1_nose(&fs, 0.85, 0.0, PRANDTL_AIR).unwrap();
        let le = ventus1_leading_edge(&fs, 0.85, 0.0, PRANDTL_AIR).unwrap();
        for (name, b) in [("nose  sphere", nose), ("LE    cylinder", le)] {
            println!(
                "  {name}  R={:.3} m  T0={:.1} K  du/dx={:.0} /s  T_wall={:.1} K ({:.1} C)  q={:.0} W/m2  relief={:.1} K  lightest={}",
                b.radius_m,
                b.stagnation_temperature_k,
                b.velocity_gradient_per_s,
                b.wall_temperature_k,
                b.wall_temperature_k - 273.15,
                b.convective_flux_w_m2,
                b.radiation_relief_k(),
                lightest_survivor(b.wall_temperature_k).map_or("none", |mm| mm.name),
            );
        }

        println!("  radius sweep (sphere, eps=0.85, sink=0 K):");
        for r in [0.005, 0.010, 0.025, 0.050, 0.100, 0.250, 0.500, 1.000] {
            let b =
                stagnation_radiation_equilibrium(&fs, r, 0.85, 0.0, PRANDTL_AIR, BodyKind::Sphere)
                    .unwrap();
            println!(
                "    R={r:6.3} m  T_wall={:6.1} K ({:5.1} C)  q={:7.0} W/m2  lightest={}",
                b.wall_temperature_k,
                b.wall_temperature_k - 273.15,
                b.convective_flux_w_m2,
                lightest_survivor(b.wall_temperature_k).map_or("none", |mm| mm.name),
            );
        }
        println!();
    }

    println!(
        "declared defaults: R_nose = {VENTUS_NOSE_RADIUS_M} m  R_LE = {VENTUS_LEADING_EDGE_RADIUS_M} m  [TO DETERMINE]"
    );
}
