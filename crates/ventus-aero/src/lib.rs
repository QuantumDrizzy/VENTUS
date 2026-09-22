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
pub mod drag;
pub mod geometry;

pub use boundary_layer::{
    adiabatic_wall_temperature_k, film_state, recovery_factor, reference_temperature_k,
    skin_friction_coefficient, stanton_number, BoundaryLayerError, EdgeState, FilmState, Regime,
    PRANDTL_AIR,
};
pub use drag::{breakdown, kuchemann_bound, DragBreakdown, DragError};
pub use geometry::{
    derive, ventus1, ventus1_m4_candidate, Geometry, GeometrySpec, FINENESS_RATIO,
    M4_CANDIDATE_FINENESS_RATIO,
};

// DONE: compressible flat-plate boundary layer by the reference-temperature
// method (boundary_layer.rs). It needs only the local edge state and a running
// length, so it is buildable while the geometry-dependent half is not.
//
// DONE: geometry derived from the design point (geometry.rs), and supersonic
// drag with Sears-Haack wave drag, linearised drag due to lift and
// reference-temperature friction (drag.rs). The geometry gap declared in
// ADR-000 and design-point.md 5.2 is closed. The M 4 host-body candidate is
// a second [`geometry::GeometrySpec`], not a silent edit of [`ventus1`].
//
// TODO(M6): area ruling beyond the Sears-Haack ideal, and integrating skin
// friction along the body rather than taking one station.
