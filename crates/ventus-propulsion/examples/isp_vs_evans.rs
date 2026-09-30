//! `cargo run -p ventus-propulsion --example isp_vs_evans` -- the cycle's Isp and overall efficiency
//! at the M 4.00 row, against NACA RM E51H02 (Evans 1951), which reports the overall efficiency of a
//! hydrocarbon ramjet as a function of flight Mach number.

use ventus_propulsion::ramjet::{ideal_ramjet_with_gas, GasModel};
use ventus_propulsion::KEROSENE_LHV_J_KG;

fn main() {
    let h = 27_747.256_463_059_47;
    let a = ventus_atmos::at_geopotential(h).unwrap();
    let m = 4.0;
    let v0 = m * a.speed_of_sound_m_s;
    let mil = 1.0 - 0.075 * (m - 1.0_f64).powf(1.35);
    println!("row: T {:.3} K, p {:.1} Pa, V0 {:.2} m/s; MIL-E-5008B recovery {mil:.4}", a.temperature_k, a.pressure_pa, v0);
    for (label, rec) in [("MIL-E-5008B", mil), ("Evans high-efficiency (Fig. 2)", 0.45)] {
        for t4 in [1700.0, 2100.0] {
            let gas = if t4 > 1800.0 { GasModel::Janaf } else { GasModel::Cubic };
            let c = ideal_ramjet_with_gas(m, a.temperature_k, a.pressure_pa, v0, rec, t4, 1.4, gas).unwrap();
            let eta0 = c.specific_impulse_s * 9.806_65 * v0 / KEROSENE_LHV_J_KG;
            println!(
                "  {label:<32} T4 {t4:>6.0} K: Isp {:.1} s, f {:.5}, overall efficiency {eta0:.4}",
                c.specific_impulse_s, c.fuel_air_ratio
            );
        }
    }
    // Evans' overall efficiency at M 4 (Fig. 11, high-efficiency engine, eta_c = 1), read off the chart.
    for (label, eta) in [("Evans max efficiency", 0.455), ("Evans at max thrust", 0.375)] {
        println!("  {label}: eta0 {eta} -> Isp {:.0} s at this V0 and LHV", eta * KEROSENE_LHV_J_KG / (9.806_65 * v0));
    }
}
