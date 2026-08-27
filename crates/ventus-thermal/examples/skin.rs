//! Radiation-equilibrium skin temperature along a station sweep, for the
//! numbers quoted in docs/design-point.md section 3.2.
//!
//!   cargo run -p ventus-thermal --example skin

use ventus_aero::boundary_layer::{EdgeState, Regime, PRANDTL_AIR};
use ventus_thermal::{lightest_survivor, radiation_equilibrium_wall};

fn main() {
    for (label, h, m) in [
        ("VENTUS-1  M3.50 / 26 km", 26_000.0, 3.5),
        ("SR-71     M3.20 / 24 km", 24_000.0, 3.2),
    ] {
        let a = ventus_atmos::at_geopotential(h).unwrap();
        let edge = EdgeState {
            temperature_k: a.temperature_k,
            pressure_pa: a.pressure_pa,
            velocity_m_s: m * a.speed_of_sound_m_s,
            mach: m,
            gamma: 1.4,
        };
        println!("{label}   T_e = {:.2} K", a.temperature_k);
        for x in [1.0, 2.0, 5.0, 10.0, 20.0, 30.0] {
            let b = radiation_equilibrium_wall(&edge, x, 0.85, 0.0, PRANDTL_AIR, Regime::Turbulent)
                .unwrap();
            println!(
                "  x={x:5.1} m  T_aw={:6.1} K  h={:6.2} W/m2K  T_wall={:6.1} K ({:5.1} C)  q={:7.0} W/m2  lightest={}",
                b.film.adiabatic_wall_temperature_k,
                b.film.heat_transfer_coefficient_w_m2_k,
                b.wall_temperature_k,
                b.wall_temperature_k - 273.15,
                b.convective_flux_w_m2,
                lightest_survivor(b.wall_temperature_k).map_or("none", |mm| mm.name),
            );
        }
        println!();
    }
}
