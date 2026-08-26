//! Print the US76 state at the altitudes given on the command line, so that any
//! number quoted in `docs/` can be regenerated rather than remembered.
//!
//!   cargo run -p ventus-atmos --example table -- 24000 26000 27000
//!
//! Altitudes are geopotential metres. Values are printed at full `f64`
//! precision: a document should quote fewer digits than this, never more.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let altitudes: Vec<f64> = if args.is_empty() {
        vec![0.0, 11_000.0, 20_000.0, 24_000.0, 26_000.0, 27_000.0]
    } else {
        args.iter()
            .map(|a| a.parse().unwrap_or_else(|_| panic!("not an altitude: {a}")))
            .collect()
    };

    for h in altitudes {
        match ventus_atmos::at_geopotential(h) {
            Ok(s) => {
                println!("h_geopotential = {} m", s.geopotential_altitude_m);
                println!("  geometric   = {}", s.geometric_altitude_m);
                println!("  temperature = {}", s.temperature_k);
                println!("  pressure    = {}", s.pressure_pa);
                println!("  density     = {}", s.density_kg_m3);
                println!("  sound speed = {}", s.speed_of_sound_m_s);
                println!("  viscosity   = {}", s.dynamic_viscosity_pa_s);
                println!("  lapse       = {}", s.lapse_rate_k_per_m);
            }
            Err(e) => println!("h = {h} m: {e:?}"),
        }
    }
}
