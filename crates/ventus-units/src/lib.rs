//! ventus-units — SI constants, float comparison primitives, unit conventions.
//!
//! ADR-000 D7: everything internal is strict SI. Every identifier carrying a
//! physical quantity carries a suffix (`_pa`, `_k`, `_m_s`, `_m`, `_rad`).
//! Conversions live in [`convert`] and are for the presentation layer only.
//!
//! Dependencies: none. Root of the DAG (ADR-000 D5).
//!
//! `no_std` is unconditional so that `cargo build` keeps proving it. `std` is
//! linked only for the test harness.
#![no_std]
#![forbid(unsafe_code)]

#[cfg(test)]
extern crate std;

pub mod constants;
pub mod convert;
pub mod float;

pub use float::{abs, rel_err, ulp_diff};
