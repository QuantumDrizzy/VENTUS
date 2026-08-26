# ADR-001 — `libm` for transcendental functions, everywhere, not just `no_std`

**Status:** accepted · **Date:** 2026-08-27 · **Amends:** ADR-000 D1, D7, D9
**Trigger:** the `[KNOWN_LIMIT]` recorded in `ventus-units/src/float.rs` at the
close of step 0: M1 needs `powf` and `exp` for the barometric formula, which
cannot be hand-rolled sensibly.

## Context

`f64::powf`, `f64::exp` and `f64::sqrt` are inherent methods provided by `std`,
not by `core`. ADR-000 D1 requires one atmosphere implementation shared by the
digital twin and the flight software, and the flight software is `no_std`. So
the choice was framed as: pull in `libm`, or split the core into a `std` build
and a `no_std` build.

## Decision

Use the `libm` crate **unconditionally**, in `std` and `no_std` builds alike.
`ventus-atmos` and every later module call `libm::pow`, `libm::exp`,
`libm::sqrt` rather than the inherent `f64` methods.

## Why unconditionally, and not only under `no_std`

This is the part that changed the decision. The obvious design is
`#[cfg(feature = "std")] use std math, #[cfg(not)] use libm`. That is worse,
and the reason is ADR-000 D9.

`std`'s `powf` and `exp` forward to the **platform** math library — MSVC's CRT
on Windows, glibc on Linux, Apple's libm on macOS. None of these is
bit-identical to the others, and glibc's results have changed between versions.
They are all correctly rounded to within an ULP or so, which is fine for
engineering and **not** fine for a project that promises a run manifest and
reproducible numbers.

Under the `cfg` design, the twin (`std`, platform libm) and the flight software
(`no_std`, `libm` crate) would compute slightly different atmospheres from the
same source file — defeating the entire point of D1 — and two developers on
different operating systems would get validation reports that differ in the last
digits, with no way to tell that from a real regression.

`libm` is pure Rust with no platform dependency, and Rust does not perform
floating-point contraction by default, so its results are bit-identical
everywhere. Choosing it for the whole project turns a `no_std` workaround into
a **reproducibility guarantee**.

This is the same argument as ADR-000 D3: determinism is worth explicit cost.

## Cost, stated

- One dependency (`libm`, no transitive dependencies, maintained by the Rust
  embedded working group).
- `libm` is generally somewhat slower than a tuned platform libm. It has not
  been measured here and is not on any hot path yet: the atmosphere is a handful
  of evaluations per control cycle, not a kernel. **If a profile ever shows it
  matters, that is a measurement to record, not a reason to revisit this
  silently** — and the M10 latency budget is where it would show up.
- Accuracy against the platform libm is quantified in
  `crates/ventus-atmos/tests/libm_agreement.rs`, which compares both across the
  full US76 altitude range.

## Measurement (2026-08-27, MSVC CRT, Windows 10, rustc 1.91.1)

| Function | Worst disagreement over the US76 domain |
|---|---|
| `sqrt` | **0 ULP** — IEEE-754 specifies it exactly, so anything else would be a bug |
| `pow` | **1 ULP** (worst at base 1.009266, exponent −5.256) |
| `exp` | **1 ULP** |
| **Resulting pressure** | **3.5 × 10⁻¹⁶ relative**, worst at 54 920 m |

M1's acceptance tolerance is 1e-6. The choice of math library moves the
atmosphere by 3.5e-16 — **ten orders of magnitude below** the tolerance it must
meet. ADR-001 therefore buys cross-platform reproducibility for no measurable
accuracy cost on this platform. The test asserts these bounds, so a future
toolchain that breaks them fails loudly instead of quietly shifting every number
downstream.

## Consequences

- ADR-000 D7 gains a rule: **no physics module may call an inherent `f64`
  transcendental method.** `abs`, comparisons and arithmetic stay as they are;
  `powf`, `exp`, `ln`, `sqrt`, and the trigonometric functions come from `libm`.
- `ventus-units::float` keeps its hand-rolled `abs` and ULP helpers: those are
  exact bit operations, not approximations, and need no library.
- The `[KNOWN_LIMIT]` in `float.rs` is discharged by this ADR.

## Rejected alternatives

**Platform `std` math with a `no_std` fallback.** Rejected above: two
atmospheres, non-reproducible reports across machines.

**Hand-rolled `pow` via `exp`/`ln` series.** Rejected: it is a research project
of its own to get to sub-ULP accuracy, and getting it wrong would corrupt every
number downstream while looking like it worked.

**Fixed-point or rational approximation of the barometric formula.** Rejected as
premature. The exponent `g0*M0/(R*·Lb)` is a fixed constant per layer, so a
per-layer approximation is possible and could be revisited if M10's latency
budget demands it. Not now, and not without a measurement.
