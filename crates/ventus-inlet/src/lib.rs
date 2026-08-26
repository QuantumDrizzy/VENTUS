//! M3 — Mixed-compression inlet. THE Mach 3.5 problem.
//!
//! Yardstick at the r4 design point (M 3.50): total-pressure recovery >= 0.74.
//! Empirical reference MIL-E-5008B: 1 - 0.075*(M-1)^1.35 = 0.7416 at M = 3.5,
//! against 0.21295 for a single normal shock.
//!
//! THAT FACTOR OF 3.48 IN p0 IS THE REASON THE INLET EXISTS — and the
//! re-baseline made it larger, not smaller. At M 3.0 the same comparison gave
//! 0.809 against 0.328, a factor of 2.46. Going faster degrades the achievable
//! recovery (0.809 -> 0.742) while degrading the normal-shock alternative far
//! more (0.328 -> 0.213), so the inlet carries more of the propulsive burden at
//! M 3.5 than it did at M 3.0.
//!
//! NOTE: the SR-71 thrust split (inlet ~54 %, nozzle ~29 %, engine ~17 %) is a
//! SYSTEM-LEVEL accounting of net thrust at M 3.2. M3 alone cannot reach it;
//! M3 + M4 together can, and the comparison is an extrapolation of 0.3 Mach,
//! which must be stated whenever the number is reported.
//!
//! Scope: shock train, capture area, spillage, unstart margin.
#![no_std]
#![forbid(unsafe_code)]

// TODO(M3): oblique shock train, capture/spill, recovery, unstart criterion.
