# ADR-000 — Repository architecture

**Status:** accepted · **Date:** 2026-08-26 · **Author:** A. Rodríguez (QuantumDrizzy)
**Revision:** r2 — supersedes r1. Changes: D5 replaced (layer numbers → DAG check),
D10 added (gamma parameterisation), D3 split into `validate` / `release` profiles.

## Context

VENTUS-1 is a validated digital twin plus flight software for a sustained Mach 3
cruise demonstrator. Ten physics fronts, each with an external yardstick.
Hard constraints: Windows 10 + MSVC, CUDA 13.0 sm_120, no cloud, no wind tunnel.
The product is the *design system*, not the airframe.

The dominant risk is not physics — it is **traceability erosion**: in six months
there will be 200 numbers and nobody will know which came from a published table
and which from a 3 AM fudge. The architecture is organised around that.

---

## D1 — Rust owns the physics core, not just the flight software

M1–M8 in Rust (`f64`, `no_std` where possible). C++17/CUDA is confined to M9.
C only if a real ABI requirement appears (none today).

**Rationale.** The flight software (M10) needs atmosphere, air data and dynamics.
A C++ core means two atmosphere implementations, and they will diverge. One
implementation shared by the twin and the FSW is the single decision that
prevents the most bugs.

**Trade-off.** Rust has a thinner numerical library ecosystem. Irrelevant here:
M1–M8 are algebraic relations, 1D root finding and ODE integrators. Nothing that
justifies pulling in Eigen.

**Rejected.** All-C++ with FFI to Rust for the FSW only. Duplicates the core and
puts an FFI boundary on the critical path.

## D2 — Zero FFI. The Rust↔CUDA boundary is versioned files

No `extern "C"`, no `bindgen`, no `cxx`. Rust writes cases and boundary
conditions; the CUDA binary reads them and writes fields back; Rust and Python
consume those.

| Payload | Format | Why |
|---|---|---|
| Cases, configuration | TOML | human-readable, diffable in git |
| Reference tables | CSV, units in the column name | citable, inspectable |
| Fields and arrays | **`.npy` v1.0** | ~60 lines to write from Rust and C++, `numpy.load` reads it with zero extra deps |

**Rationale.** An FFI boundary across MSVC + nvcc on Windows is a permanent
source of build friction (C runtime, `/MT` vs `/MD`, alignment) for zero gain:
M9 is a batch job, not a hot-loop call. Files are also inspectable, diffable,
regression-testable artifacts.

**Accepted cost.** One disk round-trip per CFD run. Milliseconds against minutes
of solver.

**`.npy` risk.** The header is where this fails. Little-endian, C order,
`fortran_order: False`, header padded to a multiple of 64 bytes. The writer must
be **round-trip tested against a real `numpy.load`**, not assumed. Tracked as a
required test in `ventus-validate`, not as a convention.

## D3 — Two-tier cross validation; "bit-exact" is explicitly bounded

Bit-exactness CPU↔GPU is achievable **only for elementwise kernels**. In a
reduction (residual, L2 norm, global CFL) the summation order on the GPU depends
on scheduling, and floating-point addition is not associative. Promising
bit-exactness there is promising something false.

| Level | Compares | Criterion | How |
|---|---|---|---|
| **A** | CUDA kernel vs CPU reference in the **same `.cu`** (`__host__ __device__`) | **bit-exact**, step by step | `validate` profile flags below |
| **B** | Reductions | deterministic across runs, ≤ 4 ULP vs Kahan sum on CPU | fixed-order tree reduction, fixed block size, no atomics |
| **C** | Rust core vs C++ core (atmosphere, shocks) | relative error ≤ 1e-12 | shared golden files |
| **D** | CFD solver vs exact solution | oblique shock angle ≤ 0.5° vs M2 θ-β-M | analytic verification case |

### Build profiles (r2)

`-fmad=false` costs real throughput, and fused multiply-add is a large part of
Blackwell's arithmetic rate. Paying for validation in production is wrong.

- **`validate`**: `-fmad=false --prec-div=true --prec-sqrt=true --ftz=false`
  (CPU reference built `/fp:strict`). Level A bit-exact. **Benchmarks disabled.**
- **`release`**: FMA enabled, contraction allowed. Validated at levels B and D only.

`xtask bench m9` refuses to run unless `xtask validate` passed **at the same
commit hash with a clean tree**. The run manifest records which profile produced
each timing. The gate is code in `main`, not a convention.

## D4 — The validation harness is a first-class module, built before M1

`ventus-validate`. Every module contributes `cases/*.toml` with reference value,
tolerance and a **cited source**. The harness **refuses to load a case with no
`source` field**. No yardstick, no module — enforced by execution, not by prose.

```toml
[[case]]
name    = "us76_h20000m"
source  = "NASA-TM-X-74335 (U.S. Standard Atmosphere 1976), Table I, h = 20 km"
inputs  = { altitude_m = 20000.0 }
expect  = { temperature_k = 216.650, pressure_pa = 5474.89, density_kg_m3 = 0.0880349 }
rel_tol = 1e-6
```

The harness emits a validation report (Markdown + CSV) on every run: case,
expected, obtained, error, source, verdict. That report **is** a project
deliverable, as much as the code.

**`[KNOWN_LIMIT]` corollary.** A case may be marked `status = "known_limit"` with
a mandatory `reason`. It counts as a visible failure in the report but does not
break the build. Limits are shown, not hidden.

**[CORRECTED] — `known_limit` has exactly one meaning, and there is no third
status.** The first draft of `gamma_validity.toml` used a third status,
`fail_if_used`, and used `known_limit` loosely to mean "this file documents a
limitation". Implementing the harness showed both were wrong:

- `fail_if_used` behaved identically to `Normal` — a failure breaks the build —
  so it was a distinction with no mechanism behind it. Removed. Two statuses.
- `known_limit` now means **this case is expected to fail**, and the failure is
  accepted and explained. Consequently a `known_limit` that *passes* is news:
  the harness reports `STALE_KNOWN_LIMIT`, because the limit no longer
  reproduces and the annotation should be deleted. It is therefore **not a TODO
  marker** — deferred work belongs in this ADR, not in a case file that fakes a
  failure. Documenting a limitation the module must nonetheless get right
  belongs on a `Normal` case with a `reason` (which is optional there, mandatory
  on `known_limit`).

A case is also refused at load if it supplies no tolerance, has an empty
`expect`, or carries a non-finite or negative tolerance: each of those asserts
nothing and would pass forever.

**Report/manifest linkage (r2).** A report without a git hash is not
reproducible. `report.md` carries `run_id`, git hash and clean/dirty tree state
in its header — the same identity as the D9 manifest, not a second loose artifact.

## D5 — The dependency graph is a DAG. There are no layer numbers *(r2, replaces r1)*

r1 defined numbered layers with the rule "a crate may not depend on an equal or
higher layer", then placed M3 and M4 both in L3 while stating that M4 calls M3.
That is impossible under its own rule, and it broke in three places:

| pair | real dependency | r1 placement |
|---|---|---|
| M4 propulsion → M3 inlet | the cycle needs inlet pressure recovery | both L3 |
| M5 thermal → M6 aero | the film coefficient needs boundary-layer state | both L4 |
| M8 dynamics → M7 mass | 6-DOF needs inertia tensor and c.g. | both L5 |

Layer numbers are a proxy for what actually matters — acyclicity — and they had
already desynchronised before a single line of code existed. **The rule is now:
the crate dependency graph must be a DAG.**

**[CORRECTED] — `xtask check-dag` does not do cycle detection.** The first draft
of this decision specified `check-dag` as a cycle detector over the workspace
`Cargo.toml` files. That was tested by deliberately adding
`ventus-units -> ventus-atmos` and running `cargo check --workspace`. Cargo
rejects it on its own:

```
error: cyclic package dependency: package `ventus-atmos` depends on itself. Cycle:
package `ventus-atmos`
    ... which satisfies path dependency `ventus-atmos` of package `ventus-units`
    ... which satisfies path dependency `ventus-units` of package `ventus-atmos`
```

Cargo already enforces acyclicity for path dependencies, with a better message
than a hand-rolled check would produce. Writing one would have been dead code
that looked like a safeguard — worse than no check, because it invites trust.

What cargo does **not** enforce is the *intended* edge set. Adding
`ventus-aero -> ventus-thermal` is perfectly acyclic and architecturally wrong
(it inverts M5 → M6). So `xtask check-dag` compares the actual edges against the
declared list below and fails on any edge not declared. Acyclicity is cargo's
job; architectural intent is `xtask`'s.

Actual edges:

```
ventus-units      ->  (none)
ventus-atmos      ->  units
ventus-gasdyn     ->  units
ventus-inlet      ->  units, gasdyn, atmos
ventus-propulsion ->  units, gasdyn, atmos, inlet
ventus-aero       ->  units, gasdyn, atmos
ventus-thermal    ->  units, gasdyn, atmos, aero
ventus-mass       ->  units
ventus-dynamics   ->  units, atmos, aero, mass, propulsion
ventus-fsw        ->  units, atmos, dynamics          (no_std, no alloc in hot path)
ventus-validate   ->  units                            (dev-dependency of every module)
```

`ventus-validate` depends only on `units` by construction, so every module can
use it in tests without creating a cycle.

## D6 — `xtask` is the orchestrator. No Make, no `.bat`

```
cargo xtask build       # rust + native; detects vcvars, fails with the exact fix
cargo xtask check-dag   # cycle detection over the workspace graph
cargo xtask validate    # full harness -> out/<run_id>/report.md
cargo xtask bench m9    # gated on validate passing at the same commit
cargo xtask report      # design point tables + plots (invokes analysis/)
```

No reliable `make` on this machine; `.bat` is neither diffable nor testable; a
Rust binary is already in the toolchain. `xtask` checks `VSCMD_ARG_TGT_ARCH=x64`
and, if absent, aborts telling the user to open the x64 Native Tools Command
Prompt for VS 2022 or call `vcvars64.bat` — instead of failing forty lines later
inside the wrong `link.exe` (Git's coreutils `link` shadows MSVC's).

## D7 — Strict SI, mandatory unit suffix on every identifier

Everything internal in SI. Any identifier carrying a physical quantity carries a
suffix: `pressure_pa`, `temperature_k`, `velocity_m_s`, `altitude_m`, `angle_rad`.
No naked `f64` crosses a module boundary. Conversions (ft, kft, degC, kn, psf)
live only in the presentation layer.

**Rejected.** Full dimensional-analysis newtypes (`uom`). Correct in principle,
but the syntactic tax in dense numerical code is real and pushes `.get::<pascal>()`
everywhere. The suffix convention captures ~90% of the benefit at zero cost. If a
real units bug appears, this decision is revisited and recorded here.

## D8 — Python is quarantined

`analysis/` reads `out/**/*.npy` and `*.csv` and produces PNG/PDF. **Nothing** in
`crates/` or `native/` imports, invokes or depends on Python. If the build breaks
when Python is absent, this decision has been violated.

## D9 — Reproducibility: one manifest per run

Every execution writes `out/<run_id>/manifest.toml`: git hash, clean/dirty tree
state, rustc/nvcc/MSVC versions, compile flags, build profile (`validate` or
`release`), RNG seeds, timestamp, hardware. **A number without a manifest is not
a result.**

## D10 — `ventus-gasdyn` is parameterised in gamma from day one *(r2, new)*

The design point declares calorically perfect air (γ = 1.4) and then computes a
stagnation temperature near 618 K. At that temperature the O₂/N₂ vibrational
modes begin to populate and cp is no longer constant. Every relation in
`ventus-gasdyn` therefore takes γ as an explicit parameter — never hard-coded —
because retrofitting it later touches every signature.

**Magnitude of the effect, computed properly.** The correct condition is energy
conservation with real enthalpy, `h0 = h_inf + V^2/2`. Substituting γ(T₀) into
the constant-γ formula is *not* the same calculation: that formula is equivalent
to ΔT = V²/(2·cp) with **constant** cp, so using γ = 1.376 applies the hot cp
(1.0505) across the entire 220 → 600 K rise, where the true cp starts at 1.003.
It overstates the correction by roughly 4×.

| Model | cp [kJ/kg·K] | T₀ | T_aw (r = 0.89) |
|---|---|---|---|
| Calorically perfect, γ = 1.4 | 1.0047 (const) | 617.8 K | 573.4 K |
| γ(T₀) = 1.376 in the constant-γ formula — **wrong method** | 1.0505 (const) | 594.0 K | 552.9 K |
| **Thermally perfect, h0 = h + V²/2** — **correct** | 1.0195 (interval mean) | **612.0 K** | **569.0 K** |

Real delta: **−5.7 K (−0.9 %)**, not −24 K. Derivation in `docs/design-point.md` §3.
Consequence: the titanium-vs-superalloy conclusion is **robust, not marginal**.
6 K decides nothing between Ti-6Al-4V (~350–400 °C sustained) and a superalloy.

**Where γ = 1.4 does break down and must be flagged:**

| Station | T | cp | γ | γ = 1.4 acceptable? |
|---|---|---|---|---|
| Freestream | 220.65 K | 1.003 | 1.400 | yes |
| Inlet / shock train | 220–450 K | 1.003–1.020 | 1.400–1.393 | yes |
| Post normal shock, M = 3 | ~591 K | ~1.049 | ~1.377 | **[TO QUANTIFY] in M2** |
| Compressor face / stagnation | ~612 K | ~1.053 | ~1.375 | −0.9 % on T₀, documented |
| **Burner** | 1700 K | ~1.21–1.23 | **~1.30–1.31** | **no** — and it is combustion products, not air |
| **Nozzle expansion** | 1700 → ~600 K | varies | varies | **no** — falsifies specific work and thrust |

**Baseline stays calorically perfect for M1–M3.** The decision is to *document
where it breaks*, with the error quantified, not to complicate M2. M4 must use
γ(T) in the burner and nozzle; that is a requirement on M4, recorded here.

Enforced by `crates/ventus-gasdyn/cases/gamma_validity.toml`, `status = "known_limit"`.

---

## Layout

```
VENTUS/
├── docs/adr/ADR-000-architecture.md
├── docs/design-point.md
├── docs/validation/                 # generated reports, versioned
├── crates/
│   ├── ventus-units/  ventus-atmos/  ventus-gasdyn/
│   ├── ventus-inlet/  ventus-propulsion/  ventus-aero/
│   ├── ventus-thermal/  ventus-mass/  ventus-dynamics/
│   ├── ventus-fsw/  ventus-validate/  xtask/
├── native/ventus_cfd/               # M9: 2D Euler, host+device in one .cu
├── data/reference/                  # US76, NACA 1135 as cited CSV
├── analysis/                        # Python, plots only
└── out/                             # gitignored
```

## Build order

**Step 0 — `ventus-units` + `ventus-validate`, before M1.**
The harness is what makes the following nine modules cheap. If M1 is written
first, its tests will be hand-rolled `assert!((t - 216.65).abs() < 1e-3)` with no
cited source, and that debt replicates ten times. Building the harness first
*structurally forces* the yardstick rule. **This is not negotiated down when M1
looks urgent.**

Acceptance for the harness itself — negative cases, the only module that can be
validated against itself:

1. A deliberately false case must report failure.
2. A case with no `source` field must be refused at load.
3. `.npy` writer must round-trip through a real `numpy.load`.

**Step 1 — M1 atmosphere.** Everything depends on it and it has the cleanest
yardstick that exists: a published table, layer by layer.

**Step 2 — M2 compressible flow**, parameterised in γ (D10).

**Then** M3 → M4 together (the 54/17 thrust split is a system-level accounting;
M3 alone cannot reach it — stated now so it is not later mis-read as an M3
failure), M5 and M6, M7, M8, M9, M10.

## Deferred, with explicit unblock points

| Gap | Deferred until | Becomes blocking at |
|---|---|---|
| Acceleration corridor (transonic thrust pinch) | M6 + M4 produce T − D | engine sizing in M4 |
| Vehicle geometry (length, wing area, mass class) | derived from physics | **start of M6** — area rule and Sears-Haack are unbuildable without it |

Both are declared in `docs/design-point.md` §5.
