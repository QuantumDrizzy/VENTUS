//! M5 — Thermal. Everything is thermal.
//!
//! Yardsticks: SR-71 nose ~ 588 K at M 3.2. Aluminium dies at ~ 150 C.
//!
//! Design point (design-point.md 3), thermally perfect gas:
//!   T0   = 612.0 K (338.9 C)   [not 617.8 K, the calorically perfect value]
//!   T_aw = 569.0 K (295.9 C)   turbulent, r = Pr^(1/3) ~ 0.89
//! The calorically imperfect correction is only -5.7 K, so the material
//! conclusion is ROBUST, not marginal. That robustness is itself the finding.
//!
//! Module deliverables:
//!   - radiative-equilibrium skin temperature (q_conv = eps*sigma*T^4)
//!   - surviving material set -> titanium airframe, not superalloy
//!   - thermal growth: Ti-6Al-4V, dL/L = 1.9e-3 = 1.9 mm per metre of airframe.
//!     Quantitative form of "the SR-71 leaked fuel on the ground".
//!
//! Depends on M6 (ventus-aero) for boundary-layer state -> film coefficient.
#![no_std]
#![forbid(unsafe_code)]

// TODO(M5): recovery temperature, radiative equilibrium, material limits,
// thermal expansion.
