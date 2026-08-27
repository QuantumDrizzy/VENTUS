# VENTUS

**A validated physics-model pipeline, in which every number traces to a published
source and the build refuses numbers that do not.**

The forcing problem is a Mach 3.5 cruise demonstrator. That is the *input*, not
the point. The deliverable is the modelling system: ten physics modules, each
pinned to an external yardstick, a harness that mechanically rejects any claim
without a citation, and a validation report carrying the git hash that produced
it.

---

## What this is

A **digital twin plus flight software**, built bottom-up in Rust with CUDA for
the one kernel that earns a GPU, on bare metal, with no cloud dependency.

The interesting engineering is not the aircraft. It is the answer to a question
that most simulation code answers badly:

> When your model prints `T = 752.8 K`, how do you know it is not 768 K, or 594 K,
> or nonsense that happens to look plausible?

VENTUS answers it structurally rather than by care:

| Mechanism | What it prevents |
|---|---|
| Every test case carries a `source` field; the loader **refuses to load a case without one** | Numbers that drift into the codebase with no provenance |
| Tolerances derived from the **printed precision of the source**, per value | Asserting digits the yardstick does not contain |
| `known_limit` cases that are **expected to fail**, with a mandatory reason | Limitations quietly widened out of existence |
| Analytic identities checked before any tabulated value | Validating a model against a table someone half-remembers |
| Every report carries `run_id`, commit hash and clean/dirty tree state | Results that cannot be reproduced |
| `libm` everywhere instead of the platform math library | Two machines silently computing different physics |

## What this is not

- **Not an aircraft build.** No hardware, no wind tunnel, no claim that any of
  this is manufacturable. Where the model runs out, the model says so.
- **Not weapons work.** Propulsion, aerodynamics, thermal, structure, control.
  No armament, no targeting, no guidance, no countermeasures.
- **Not a simulation demo.** There is no renderer and no web front end. The
  output is a validation report and a set of numbers with citations attached.
- **Not benchmark theatre.** The GPU module refuses to print timings unless its
  correctness gate passed at the same commit.

---

## Status

| Module | Subject | Yardstick | State |
|---|---|---|---|
| Step 0 | Validation harness | negative cases + numpy byte-equality | **done** |
| M1 | U.S. Standard Atmosphere 1976 | the published table, layer by layer | **done** |
| M2 | Compressible flow, parameterised in γ | NACA Report 1135 + analytic identities | **done** |
| M3 | Mixed-compression inlet | MIL-E-5008B recovery, SR-71 thrust split | next |
| M4 | Turboramjet cycle | SR-71 cruise thrust accounting | next |
| M5 | Radiation-equilibrium skin, material selection | SR-71 skin 250-300 C at M 3.2 | **done** |
| M6a | Compressible boundary layer | Blasius, Reynolds analogy (both exact) | **done** |
| M6b | Wave drag, area rule, L/D | Concorde, SR-71, Küchemann bound | blocked on geometry |
| M7 | Mass fractions, range | SR-71 mass breakdown | blocked on geometry |
| M8 | 6-DOF flight dynamics | integrator energy conservation | pending |
| M9 | CUDA 2-D Euler solver | exact oblique-shock angles from M2 | pending |
| M10 | Flight software | declared latency budget | pending |

```
cargo test --workspace     107 tests
cargo xtask validate       57 cases: 52 pass, 0 fail, 5 known limit, 0 stale
```

The five known limits are not failures being tolerated. Each is a case that is
**expected to fail**, with a written reason, and the harness reports a
`STALE_KNOWN_LIMIT` if one ever starts passing — because that means the
limitation is gone and the annotation should be deleted.

---

## Why you should believe the numbers

Because the method keeps catching things, including in work that had already
been reviewed line by line. A representative sample, all recorded in the repo:

**A wrong number that survived review.** `docs/design-point.md` stated the
geometric altitude at 24 km geopotential as 24 090.3 m and the offset as 90.3 m.
Both were wrong — 24 090.96 m and 91.0 m. The figure had been checked by hand and
signed off as verified. It was caught the first time M1 *computed* it rather than
quoting it.

**Fabricated precision, twice.** A case file asserted normal-shock ratios to
sixteen digits at `rel_tol = 1e-12`, hand-derived. Checking against the closed
form showed the downstream-Mach digits were invented past the eleventh, and the
hand-computed total-pressure ratio was 0.32744 against a true 0.328344. Replaced
with the published values at the precision the published table actually has.

**A constant sitting exactly on a tolerance.** US76 defines `R = R*/M₀ =
287.05307`; the value commonly quoted in the literature is `287.0528`. The gap is
1.0 × 10⁻⁶ — exactly M1's acceptance tolerance. Two published density values do
not reproduce; for the one at 20 km, using the quoted constant instead of the
derived one reproduces the published figure *exactly*. Both are carried as
failing `known_limit` cases with `[TO VERIFY]`, because the honest answer is that
it is unresolved without a primary copy of the table.

**A correction that was wrong by 4×.** The calorically imperfect stagnation
temperature was first "fixed" by substituting γ(T₀) into the constant-γ formula.
That formula is `ΔT = V²/(2cp)` with *constant* cp, so it charges the hot cp over
the cold leg. The real correction, from `h₀ = h + V²/2`, is −15.3 K; the naive fix
gave −56.7 K. The wrong value is kept on the record so the mistake is not made
twice.

**A model used outside its range, erring in the flattering direction.** The cp
correlation is fitted from 273 K. The design-point freestream is 222.65 K.
Extrapolated, it returns γ = 1.4067 — 0.5 % *above* the true value, which would
have made the calorically perfect assumption look sounder than it is, exactly
where the project leans on it. The function refuses instead.

**Two remembered sources contradicting each other.** A cp correlation and the cp
table in the same reference disagree by up to 0.68 %, against a remembered claim
of 0.4 % maximum error. The test asserts the *measured* 0.68 %, not the
remembered 0.4 %, and says why in the failure message.

**Silent NaN.** `characteristic_mach` overflowed above M ≈ 1.3 × 10¹⁵⁴ because M²
overflows, giving `inf/inf`. A NaN there would have propagated into the Prandtl
relation `M₁* · M₂* = 1` — the single strongest check in the module — and made it
pass vacuously. Rewritten divided through by M².

**A test that tested nothing.** The perturbation check broke each computed value
by a fixed 0.1 % and asserted every case turned red. One case legitimately
carries a 1 % tolerance, so it correctly survived. The perturbation is now
derived from each case's own tolerance.

Full record in `docs/adr/`, in the case files, and in the commit messages. Every
correction is marked `[CORRECTED]` where it lives.

---

## Design point

M 3.50 at 26 km geopotential, US Standard Atmosphere 1976.
Full derivation and provenance in [`docs/design-point.md`](docs/design-point.md).

| | |
|---|---|
| T∞ / p∞ / ρ∞ | 222.65 K · 2 153.09 Pa · 0.0336882 kg·m⁻³ |
| V∞ | 1 046.95 m·s⁻¹ (3 769 km·h⁻¹) |
| q∞ | 18.463 kPa |
| T₀ | **752.8 K** (thermally perfect; 768.1 K if γ = 1.4) |
| T_aw | **709.3 K / 436.2 °C** (no radiation) |
| **Skin, radiating** | **548.3 K / 275.2 °C** at 10 m — computed, ε = 0.85 |
| Inlet recovery | 0.742, against 0.213 for a normal shock — **a factor of 3.48** |

**Why M 3.5 and not M 3.75.** Above roughly M 3.35 there is no public vehicle to
check against until the scramjet demonstrators, which are a different regime.
At M 3.76 the SR-71 thrust-split anchor becomes an extrapolation rather than a
check, and two modules would lose their yardstick entirely. M 3.5 sits 0.15 Mach
from the D-21 and 0.3 from the SR-71, so both remain checks. The altitude was
chosen to hold dynamic pressure constant against the earlier M 3.0 baseline, and
holds it to 0.004 % — so the re-baseline hardens the thermal and propulsive
problems without touching the structural loads case.

**What that cost, and how it resolved.** At M 3.0 the material answer was
comfortable. At M 3.5 the recovery temperature sits *above* the Ti-6Al-4V
sustained limit, so the answer came to depend entirely on the radiation balance.
M5 computed it: the radiating wall settles 75 K *below* the limit, and radiation
is worth 161 K. **The margin belongs to the radiation term, not to the
material** — which means buried or low-emissivity structure does not get it and
needs β-titanium. That is a design constraint the recovery temperature alone
would never have surfaced.

The same model at the SR-71 cruise condition gives 270 °C at 1 m against its
published 250–300 °C skin band, and falls below the band aft of ~8 m — which is
the model stating its own scope rather than being tuned to agree.

---

## Architecture

Decisions and their trade-offs live in [`docs/adr/`](docs/adr/). The load-bearing ones:

- **Rust owns the physics core**, not just the flight software, so the digital
  twin and the FSW share exactly one atmosphere implementation. C++/CUDA is
  confined to the single GPU module.
- **Zero FFI.** The Rust↔CUDA boundary is versioned files — TOML for cases, CSV
  for tables, `.npy` for fields. Verified byte-identical to `numpy.save` across
  nine fixtures, then frozen as a golden so the build needs no Python.
- **`libm` unconditionally**, not just under `no_std`. `std` forwards to the
  platform math library, which differs between operating systems and between
  versions; a `cfg`-switched design would have the twin and the flight software
  computing different atmospheres from the same source file. Measured cost:
  1 ULP on `pow` and `exp`, 0 on `sqrt`, 3.5 × 10⁻¹⁶ on the resulting pressure.
- **Bit-exactness is bounded honestly.** CPU↔GPU bit-equality is achievable for
  elementwise kernels and *not* for reductions, because floating-point addition
  is not associative and GPU summation order depends on scheduling. Four
  validation levels are defined rather than one promise that cannot be kept.
- **`-fmad=false` does not ship.** It costs real throughput, so it lives in a
  `validate` profile that has benchmarks compiled out, while `release` keeps FMA
  and is validated at the levels that survive it.

## Layout

```
crates/          Rust workspace: units, atmos, gasdyn, inlet, propulsion,
                 aero, thermal, mass, dynamics, fsw, validate, xtask
native/          C++17/CUDA, sm_120 — M9 only
data/reference/  external yardsticks as cited CSV
analysis/        Python. Plots and one-off cross-checks. Quarantined:
                 nothing in crates/ or native/ may depend on it.
docs/adr/        architecture decisions, including the corrected ones
out/             run artifacts, gitignored
```

## Running it

```bash
cargo test --workspace
```

```bash
cargo xtask validate
```

Writes `out/<run_id>/report.md` and `report.csv`. The header carries the commit
hash and whether the tree was dirty; a dirty tree is labelled *not reproducible*
in the report itself.

Building `native/` needs the MSVC environment loaded — the x64 Native Tools
prompt, or `vcvars64.bat` first. `xtask` checks for it and fails with the exact
remedy rather than dying inside the wrong linker forty lines later.

---

## Conventions

- Strict SI internally; every identifier carrying a quantity carries its unit
  (`pressure_pa`, `temperature_k`, `velocity_m_s`). Conversions exist only in the
  presentation layer.
- Code and comments in English.
- Open items are tagged in place and never quoted as results:
  `[TO VERIFY]` · `[TO CITE]` · `[TO COMPUTE]` · `[TO PROVE]` · `[TO QUANTIFY]` ·
  `[KNOWN_LIMIT]` · `[CORRECTED]`
