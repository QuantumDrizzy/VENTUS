//! M6 — Wave drag, area rule, L/D.
//!
//! Yardstick, TWO-SIDED (single-sided guards catch half the errors), at the r4
//! design point M 3.50:
//!   Concorde L/D ~ 7.5 at M 2.04; SR-71 ~ 6 at M 3.2.
//!   Kuchemann correlation 4*(M+3)/M gives 7.43 at M 3.5, an UPPER bound: the
//!   SR-71 achieves ~6 against a bound of 7.75 at its own Mach.
//!   Target 5.0-6.0.
//!   - L/D > 7.4 exceeds the Kuchemann bound -> bug.
//!   - L/D < 3.5 -> bug OR bad configuration. The harness must DISTINGUISH
//!     these: a bug fails, a bad configuration is a finding.
//!
//! BLOCKED ON GEOMETRY (design-point.md 5.2). Area rule and Sears-Haack are
//! unbuildable without length, wing area and configuration. The deferral stops
//! being acceptable the day this module starts.
#![no_std]
#![forbid(unsafe_code)]

#[cfg(test)]
extern crate std;

pub mod boundary_layer;

pub use boundary_layer::{
    adiabatic_wall_temperature_k, film_state, recovery_factor, reference_temperature_k,
    skin_friction_coefficient, stanton_number, BoundaryLayerError, EdgeState, FilmState, Regime,
    PRANDTL_AIR,
};

// DONE: compressible flat-plate boundary layer by the reference-temperature
// method (boundary_layer.rs). It needs only the local edge state and a running
// length, so it is buildable while the geometry-dependent half is not.
//
// TODO(M6): wave drag (linearised supersonic), Sears-Haack, drag polar, L/D.
// All still BLOCKED ON GEOMETRY.
