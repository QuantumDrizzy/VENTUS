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
//! Scope: shock train, capture area, spillage, unstart margin. Capture and
//! Kantrowitz starting are identities plus one cited 1-D limit (`capture.rs`).
//! A declared cowl lip unlocks additive/spillage *force* (pitot-equivalent
//! closure). The translating-spike schedule and time-accurate unstart remain
//! typed refusals: this aircraft has no spike geometry to schedule.
#![no_std]
#![forbid(unsafe_code)]

#[cfg(test)]
extern crate std;

pub mod capture;
pub mod shock_train;

pub use capture::{
    additive_drag_from_lip, additive_drag_n, additive_drag_without_lip, body_can_host_capture,
    capture_to_body_ratio, isentropic_contraction_ratio, kantrowitz_contraction_ratio,
    mass_flow_ratio, self_starts, spike_position_m, spillage, streamtube_area_m2, unstart_margin,
    AdditiveDrag, CaptureError, CowlLip, CowlLipGeometry, FreestreamStation, Spillage, StartError,
    KANTROWITZ_CONTRACTION_INFINITE_MACH_GAMMA_14, SHARP_LIP_RADIUS_RATIO_LIMIT,
    VENTUS_COWL_LIP_RADIUS_RATIO,
};
pub use shock_train::{
    mil_e_5008b_recovery, optimise_ramps, shock_train, InletError, ShockTrain, Station, MAX_RAMPS,
};

// DONE: oblique shock train, optimal ramp angles, total-pressure recovery
// (shock_train.rs).
//
// DONE: capture-vs-body identity, mass-flow ratio / spilled area, cowl-lip
// *geometry*, additive-drag force from a declared lip (pitot-equivalent
// closure of the Seddon & Goldsmith definition), Kantrowitz self-start
// contraction (capture.rs).
//
// REFUSED (typed, not a TODO that can be read as a plan): translating-spike
// schedule and time-accurate unstart. Closing those needs internal contraction
// and a throat this aircraft has not declared. Lip suction is recorded as
// geometry (`r/R`) and not credited as a force. φ_LBO is not this crate.
