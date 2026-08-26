//! ventus-validate — the validation harness. Built BEFORE M1 (ADR-000 step 0).
//!
//! Every module contributes `cases/*.toml` carrying a reference value, a
//! tolerance, and a cited `source`. The harness REFUSES to load a case with no
//! `source` field: no yardstick, no module — enforced by execution, not prose.
//!
//! Emits a Markdown + CSV report whose header carries `run_id`, git hash and
//! clean/dirty tree state, so the report and the D9 manifest are one identity
//! rather than two loose artifacts (ADR-000 D4 r2).
//!
//! A case may be marked `status = "known_limit"` with a mandatory `reason`.
//! It shows as a visible failure in the report but does not break the build.
//!
//! Dependencies: ventus-units only, so every module can dev-depend on this
//! without creating a cycle.
//!
//! Acceptance for the harness itself (negative cases — the only module that can
//! legitimately be validated against itself):
//!   1. deliberately false case  -> must report failure
//!   2. case with no `source`    -> must be refused at load
//!   3. `.npy` writer            -> must round-trip through a real numpy.load
//!      (little-endian, C order, fortran_order: False, header padded to 64 B)
#![forbid(unsafe_code)]

// TODO(step-0): Case/Expect/Verdict types, TOML loader with source enforcement,
// report writer, npy writer + round-trip test.
