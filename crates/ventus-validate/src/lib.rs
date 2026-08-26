//! ventus-validate — the validation harness. Built BEFORE M1 (ADR-000 step 0).
//!
//! Every module contributes `cases/*.toml` carrying a reference value, a
//! tolerance, and a cited `source`. The harness REFUSES to load a case with no
//! `source`: no yardstick, no module — enforced by execution, not prose.
//!
//! Dependencies: `ventus-units` only, so every module can dev-depend on this
//! without creating a cycle (ADR-000 D5).
//!
//! Acceptance for the harness itself lives in `tests/`. It is the only module
//! that can legitimately be validated against itself, and it is validated by
//! **negative** cases plus a positive control:
//!   1. a deliberately false case must report failure,
//!   2. a case with no `source` must be refused at load,
//!   3. the `.npy` writer must match numpy byte-for-byte,
//!   4. a true case must pass — without this, a harness that always failed
//!      would satisfy 1-3.
//!
//! Usage from a physics module:
//!
//! ```no_run
//! use std::collections::BTreeMap;
//! use ventus_validate::{case, check, ExpectValue};
//!
//! let cases = case::load_dir(std::path::Path::new("cases")).unwrap();
//! for c in &cases {
//!     let mut computed = BTreeMap::new();
//!     computed.insert("temperature_k".to_string(), ExpectValue::Float(216.65));
//!     let outcome = check::check(c, &computed);
//!     assert!(!outcome.breaks_build(), "{}: {}", c.name, outcome.label());
//! }
//! ```
#![forbid(unsafe_code)]

pub mod case;
pub mod check;
pub mod npy;
pub mod report;

pub use case::{Case, ExpectValue, LoadError, Status, Tolerance};
pub use check::{check, Mismatch, Outcome, Summary};
pub use report::{Entry, Provenance};

/// Convenience for building a `computed` map in module tests.
#[macro_export]
macro_rules! computed {
    ($($key:expr => $value:expr),* $(,)?) => {{
        let mut m = ::std::collections::BTreeMap::new();
        $( m.insert(::std::string::String::from($key), $crate::ExpectValue::from($value)); )*
        m
    }};
}

impl From<f64> for ExpectValue {
    fn from(v: f64) -> Self {
        ExpectValue::Float(v)
    }
}

impl From<bool> for ExpectValue {
    fn from(v: bool) -> Self {
        ExpectValue::Bool(v)
    }
}
