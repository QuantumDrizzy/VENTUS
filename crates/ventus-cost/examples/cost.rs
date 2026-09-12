//! M11 at the two points that matter: the SR-71 anchor and VENTUS-1.
//!
//!   cargo run -p ventus-cost --example cost
//!
//! Velocities come from M1 rather than being recited, so this is the same
//! atmosphere the rest of the project uses.

use ventus_cost::dapca::{
    Envelope, Rates, MATERIAL_FACTOR_ALUMINIUM, MATERIAL_FACTOR_TITANIUM_HIGH,
    MATERIAL_FACTOR_TITANIUM_LOW,
};
use ventus_cost::{estimate, Inputs, Validity};

fn velocity(mach: f64, altitude_m: f64) -> f64 {
    mach * ventus_atmos::at_geopotential(altitude_m)
        .unwrap()
        .speed_of_sound_m_s
}

fn show(label: &str, inputs: &Inputs) {
    let rates = Rates::raymer_1986();
    let env = Envelope::conventional_metal();
    let e = estimate(inputs, &rates, &env).unwrap();
    println!("{label}");
    println!(
        "  empty {:.0} kg   V {:.1} m/s   Q {:.0}   material x{:.2}",
        inputs.empty_mass_kg,
        inputs.max_velocity_m_s,
        inputs.production_quantity,
        inputs.material_factor
    );
    println!(
        "  hours  eng {:.3e}  tool {:.3e}  mfg {:.3e}  QC {:.3e}",
        e.hours.engineering, e.hours.tooling, e.hours.manufacturing, e.hours.quality
    );
    println!(
        "  cost   labour {:.3e}  dev {:.3e}  test {:.3e}  matl {:.3e}",
        e.labour_usd, e.development_usd, e.flight_test_usd, e.materials_usd
    );
    println!(
        "  AIRFRAME-ONLY TOTAL {:.4e} (1986 USD)   per aircraft {:.4e}",
        e.airframe_total_usd, e.per_aircraft_usd
    );
    match e.validity {
        Validity::WithinFit => println!("  validity: inside the fit"),
        Validity::Extrapolated(x) => println!(
            "  validity: EXTRAPOLATED - velocity {:.2}x past the fit, weight {:.2}x",
            x.velocity_ratio, x.empty_weight_ratio
        ),
    }
    println!();
}

fn main() {
    let v_sr71 = velocity(3.2, 24_000.0);
    let v_ventus = velocity(3.5, 26_000.0);
    println!(
        "V(SR-71, M3.2/24km) = {v_sr71:.2} m/s      V(VENTUS-1, M3.5/26km) = {v_ventus:.2} m/s"
    );
    println!("velocity ratio = {:.6}\n", v_ventus / v_sr71);

    // SR-71 empty mass 30 600 kg, 32 built. Both [TO CITE].
    let sr71 = Inputs {
        empty_mass_kg: 30_600.0,
        max_velocity_m_s: v_sr71,
        production_quantity: 32.0,
        flight_test_aircraft: 2.0,
        material_factor: MATERIAL_FACTOR_TITANIUM_LOW,
    };
    show("SR-71 anchor, titanium x1.7", &sr71);

    show(
        "SR-71 airframe priced as aluminium (counterfactual)",
        &Inputs {
            material_factor: MATERIAL_FACTOR_ALUMINIUM,
            ..sr71
        },
    );
    show(
        "SR-71 airframe at the VENTUS design Mach (counterfactual)",
        &Inputs {
            max_velocity_m_s: v_ventus,
            ..sr71
        },
    );
    show(
        "SR-71 airframe, titanium x2.2",
        &Inputs {
            material_factor: MATERIAL_FACTOR_TITANIUM_HIGH,
            ..sr71
        },
    );

    // VENTUS-1 has no declared empty mass, so this is a SWEEP, not a result.
    println!("VENTUS-1: empty mass is [TO COMPUTE], so this is parametric.");
    println!("  Cruise mass is 28 t (M6b). Empty mass is not derived anywhere.\n");
    for empty in [12_000.0, 14_000.0, 16_000.0] {
        show(
            &std::format!("VENTUS-1, empty {empty:.0} kg, Q = 6, titanium x1.7"),
            &Inputs {
                empty_mass_kg: empty,
                max_velocity_m_s: v_ventus,
                production_quantity: 6.0,
                flight_test_aircraft: 2.0,
                material_factor: MATERIAL_FACTOR_TITANIUM_LOW,
            },
        );
    }
}
