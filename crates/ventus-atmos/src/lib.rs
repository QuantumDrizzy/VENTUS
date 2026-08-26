//! M1 — U.S. Standard Atmosphere 1976.
//!
//! Yardstick: the published table, layer by layer, <= 1e-6 relative.
//! Everything else in VENTUS depends on this. If M1 is wrong, every later
//! number is noise.
//!
//! Mandatory acceptance detail:
//!   - 7 layers to 86 km, boundaries at 0/11/20/32/47/51/71 km geopotential.
//!   - Behaviour AT the discontinuity: h = 20000 m +/- 1 m must give continuous
//!     T and discontinuous dT/dh. The classic bug lives in `<` vs `<=`.
//!   - Geopotential vs geometric altitude implemented explicitly, not ignored:
//!     at 24 km the confusion is 90.3 m, i.e. ~1 % in density.
//!
//! `no_std`, no alloc: this exact code goes inside the flight software (M10).
#![no_std]
#![forbid(unsafe_code)]

// TODO(M1): layer table, geopotential/geometric conversion, T/p/rho/a/mu.
