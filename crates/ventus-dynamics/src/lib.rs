//! M8 — 6-DOF flight dynamics.
//!
//! Yardstick: energy conservation of the integrator in ballistic flight with no
//! atmosphere. Relative drift <= 1e-10 over 1e6 steps.
//!
//! Scope: integrator, stability and control at Mach 3, inertial coupling.
//!
//! BLOCKED ON GEOMETRY (design-point.md 5.2) for the inertia tensor.
#![no_std]
#![forbid(unsafe_code)]

// TODO(M8): state vector, integrators, aero/propulsive force build-up.
