//! M2 — Compressible flow relations.
//!
//! Yardstick: NACA 1135. Normal shock at M = 3 (closed form, <= 1e-9 relative):
//!   p2/p1 = 10.3333, T2/T1 = 2.6790, M2 = 0.475191, p02/p01 = 0.328344
//!
//! ADR-000 D10: EVERY relation takes gamma as an explicit parameter. Never
//! hard-code 1.4. Retrofitting this later touches every signature.
//!
//! Non-trivial parts, where real bugs live and the 1135 tables catch them:
//!   - theta-beta-M: cubic root, two physical branches (weak / strong).
//!   - inverse Prandtl-Meyer: Newton iteration, needs a robust initial guess.
//!
//! Open: `cases/gamma_validity.toml` (status = known_limit) must quantify the
//! error of gamma = 1.4 at the post-normal-shock state T2 = 591 K, and record
//! that gamma = 1.4 is a FAILURE (not a limit) in the M4 burner and nozzle.
#![no_std]
#![forbid(unsafe_code)]

// TODO(M2): isentropic, normal shock, oblique shock (theta-beta-M),
// Prandtl-Meyer + inverse. All parameterised in gamma.
