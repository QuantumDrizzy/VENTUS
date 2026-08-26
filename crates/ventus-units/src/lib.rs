//! ventus-units — SI constants and unit conventions.
//!
//! ADR-000 D7: everything internal is strict SI. Every identifier carrying a
//! physical quantity carries a suffix (`_pa`, `_k`, `_m_s`, `_m`, `_rad`).
//! Unit conversions live in the presentation layer only.
//!
//! Dependencies: none. Root of the DAG (ADR-000 D5).
#![no_std]
#![forbid(unsafe_code)]

// TODO(step-0): physical constants, ULP helpers, presentation-layer conversions.
