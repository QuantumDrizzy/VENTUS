//! M6 — Wave drag, area rule, L/D.
//!
//! Yardstick, TWO-SIDED (single-sided guards catch half the errors):
//!   Concorde L/D ~ 7.5 at M 2.04; SR-71 ~ 6 at M 3.2. Target 5.5-6.5.
//!   - L/D > 8 at M 3 -> bug.
//!   - L/D < 4 at M 3 -> bug OR bad configuration. The harness must
//!     DISTINGUISH these: a bug fails, a bad configuration is a finding.
//!
//! BLOCKED ON GEOMETRY (design-point.md 5.2). Area rule and Sears-Haack are
//! unbuildable without length, wing area and configuration. The deferral stops
//! being acceptable the day this module starts.
#![no_std]
#![forbid(unsafe_code)]

// TODO(M6): wave drag (linearised supersonic), Sears-Haack, skin friction via
// reference temperature, drag polar, L/D.
