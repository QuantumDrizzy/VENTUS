//! M5 — Thermal. Everything is thermal.
//!
//! Yardsticks: SR-71 nose ~ 588 K at M 3.2 (where T0 = 664 K, ratio 0.886).
//! Aluminium dies at ~ 150 C.
//!
//! Design point (design-point.md 3, r4: M 3.50 / 26 km), thermally perfect gas:
//!   T0   = 752.8 K (479.7 C)   [not 768.1 K, the calorically perfect value]
//!   T_aw = 694.5 K (421.4 C)   turbulent, r = Pr^(1/3) ~ 0.89
//!
//! THIS MODULE NOW DECIDES THE MATERIAL RATHER THAN CONFIRMING IT. At the old
//! M 3.0 design point T_aw was 296 C against a Ti-6Al-4V sustained limit of
//! 350-400 C — a 55-105 K margin the gas model moved by only 4 K, so M5 would
//! have confirmed the obvious. At M 3.5, T_aw sits 20-70 K ABOVE that limit, so
//! whether the airframe can be Ti-6Al-4V depends entirely on where radiative
//! equilibrium lands. That balance is this module.
//!
//! Module deliverables:
//!   - radiative-equilibrium skin temperature (q_conv = eps*sigma*T^4)
//!   - stagnation-point / leading-edge heating after Fay-Riddell, at a
//!     declared `[TO DETERMINE]` radius -- not a radius chosen to save Ti
//!   - the surviving material set. Flat radiating panels: Ti-6Al-4V. Nose and
//!     leading edges: Ti-6242S / beta-Ti / Inconel; Ti-6Al-4V does not survive
//!     the declared structural-minimum radius.
//!   - thermal growth: Ti alloys, dL/L = 2.9 to 3.5e-3 = 2.9 to 3.5 mm per metre
//!     of airframe, up from 1.9 mm/m at M 3.0. Quantitative form of the SR-71
//!     leaking fuel on the ground.
//!
//! Depends on M6 (ventus-aero) for boundary-layer state -> film coefficient.
#![no_std]
#![forbid(unsafe_code)]

#[cfg(test)]
extern crate std;

pub mod fay_riddell;
pub mod radiative_equilibrium;

pub use fay_riddell::{
    heat_flux, stagnation_edge, stagnation_radiation_equilibrium, ventus1_leading_edge,
    ventus1_nose, BodyKind, FayRiddellHeatFlux, Freestream, StagnationBalance, StagnationEdge,
    CYLINDER_COEFFICIENT, SPHERE_COEFFICIENT, VENTUS_LEADING_EDGE_RADIUS_M, VENTUS_NOSE_RADIUS_M,
};
pub use radiative_equilibrium::{
    lightest_survivor, radiation_equilibrium_wall, survivors, thermal_growth_per_metre, Material,
    RadiationBalance, ThermalError, CANDIDATES, TITANIUM_EXPANSION_PER_K,
};

// DONE: recovery temperature, radiative equilibrium, material selection and
// thermal growth (radiative_equilibrium.rs).
//
// DONE: stagnation-point and leading-edge heating after Fay-Riddell
// (fay_riddell.rs). Internal conduction and the transient during acceleration
// remain open.
