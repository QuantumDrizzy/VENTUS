//! M10 — Flight software.
//!
//! Deterministic control loop, declared latency budget, NO allocation on the
//! hot path. Shares the M1 atmosphere implementation with the digital twin by
//! construction (ADR-000 D1) — there is exactly one atmosphere in this repo.
#![no_std]
#![forbid(unsafe_code)]

// TODO(M10): control loop, air-data path, latency budget instrumentation.
