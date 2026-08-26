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
//!   - the surviving material set. [TO PROVE] the expected answer is the one the
//!     SR-71 arrived at: beta-titanium or Ti-6242S for hot structure, Ti-6Al-4V
//!     only where radiation keeps the skin cool, Inconel at leading edges.
//!   - thermal growth: Ti alloys, dL/L = 2.9 to 3.5e-3 = 2.9 to 3.5 mm per metre
//!     of airframe, up from 1.9 mm/m at M 3.0. Quantitative form of the SR-71
//!     leaking fuel on the ground.
//!
//! Depends on M6 (ventus-aero) for boundary-layer state -> film coefficient.
#![no_std]
#![forbid(unsafe_code)]

// TODO(M5): recovery temperature, radiative equilibrium, material limits,
// thermal expansion.
