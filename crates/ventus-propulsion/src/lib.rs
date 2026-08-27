//! M4 — Turboramjet cycle and system-level thrust accounting.
//!
//! Yardsticks at the r4 design point (M 3.50 / 26 km):
//!   - Specific-work collapse: T4max/T0 = 1700/752.8 = 2.26, against 5.9 at sea
//!     level static. At M 3.0 it was 2.78. This is WHY a turbomachine is out of
//!     its element here, and it is a temperature-ratio argument, not a
//!     compressor-material one.
//!   - SR-71 cruise thrust split, jointly with M3: inlet ~54 %, nozzle ~29 %,
//!     engine ~17 %, +/- 10 percentage points. Measured at M 3.2, so applying it
//!     at M 3.5 is a 0.3 Mach extrapolation — say so when reporting.
//!
//! ADR-000 D10 HARD REQUIREMENT: gamma = 1.4 is a FAILURE in this module, not a
//! known limit. In the burner at 1700 K, gamma ~ 1.30-1.31, and the working
//! fluid is combustion products, not air. Using 1.4 in the burner and nozzle
//! falsifies specific work and therefore thrust.
//!
//! Sizing note: the engine is NOT sized by cruise. See design-point.md 5.1 —
//! the transonic thrust pinch near M 1.1 governs.
#![no_std]
#![forbid(unsafe_code)]

#[cfg(test)]
extern crate std;

pub mod ramjet;

pub use ramjet::{ideal_ramjet, Cycle, CycleError, BURNER_PRESSURE_RATIO, KEROSENE_LHV_J_KG};

// DONE: ideal ramjet cycle with gamma(T) in the burner and nozzle (ramjet.rs).
//
// TODO(M4): the component THRUST SPLIT (inlet 54 % / nozzle 29 % / engine 17 %
// on the SR-71) is an axial force accounting over the flowpath, not a cycle
// result. It needs the pressure distribution on the compression surfaces and
// the cowl, which a quasi-1D model does not carry. Declared as a gap rather
// than approximated. Also: J58-style bleed/bypass, and the turbo-to-ram
// transition schedule.
