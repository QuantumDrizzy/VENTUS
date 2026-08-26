//! M3 — Mixed-compression inlet. THE Mach 3 problem.
//!
//! Yardstick: total-pressure recovery >= 0.80 at M 3.
//! Empirical reference MIL-E-5008B: 1 - 0.075*(M-1)^1.35 = 0.809 at M = 3,
//! against 0.328 for a single normal shock. That factor of 2.46 in p0 IS the
//! reason the inlet exists.
//!
//! NOTE: the SR-71 thrust split (inlet ~54 %, nozzle ~29 %, engine ~17 %) is a
//! SYSTEM-LEVEL accounting of net thrust. M3 alone cannot reach it; M3 + M4
//! together can. Stated here so a shortfall is not mis-read as an M3 failure.
//!
//! Scope: shock train, capture area, spillage, unstart margin.
#![no_std]
#![forbid(unsafe_code)]

// TODO(M3): oblique shock train, capture/spill, recovery, unstart criterion.
