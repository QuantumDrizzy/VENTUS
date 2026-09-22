# VENTUS

**A validated physics-model pipeline, in which every validated claim traces to a
published source, and the modelling constants that do not yet are counted by the
build rather than left to be discovered.**

The forcing problem is a Mach 3.5 cruise demonstrator. That is the *input*, not
the point. The deliverable is the modelling system: twelve modules, each declaring
whether it is held to an external yardstick or to an identity; a harness that
**refuses to load a test case without a `source` field**; and a validation report
carrying the git hash that produced it.

**What that harness does not do, said here rather than left to a `grep`.** It
gates *cases*. It does not gate *constants*: a modelling constant marked
`[TO CITE]` in a doc comment compiles and validates fine, and there are **28** of
them right now. `cargo xtask validate` counts and prints that number with every
verdict, so it cannot go stale in this file.

The most important one is the lean blowout equivalence ratio in M12. It decides
whether the design point has margin or does not fly — see the corridor below.
What resolving it takes is written as a specification rather than as a wish, in
`ventus_envelope::LEAN_BLOWOUT_DECISION_CRITERION`: a cited `φ_LBO` on either
side of 0.4615 closes the question. No flame holder is declared, so the
permissive literature end is not available; the operative bound is the strict
end 0.50, still `[TO VERIFY]`. Under that bound M 3.50 does not hold a flame.

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
| Modelling constants not yet cited are marked `[TO CITE]` and **counted by `xtask validate`** | An uncited number being quietly forgotten rather than quietly carried |
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
| M3 | Inlet shock train, optimal ramps | MIL-E-5008B recovery; Oswatitsch, verified not assumed | **done** |
| M4 | Ideal ramjet cycle, gamma(T) | specific-work collapse, ramjet Isp band | **done** |
| M5 | Radiation-equilibrium skin, material selection | SR-71 skin 250-300 C at M 3.2 | **done** |
| M6a | Compressible boundary layer | Blasius, Reynolds analogy (both exact) | **done** |
| M6b | Geometry, wave drag, L/D | Küchemann bound; L/D in the 5.0-6.0 target | **done** |
| M7 | Mass fractions, empty mass, Breguet range | SR-71 unrefuelled range - the end-to-end check | **done** |
| M8 | 6-DOF rigid-body dynamics | energy drift < 1e-10 over 1e6 steps | **done** |
| M9 | 2-D Euler solver | shock angle 0.006 deg vs exact theta-beta-M | **physics done**, GPU build blocked |
| M10 | Flight software | shares M1 bit-for-bit with the twin | **done** |
| M11 | DAPCA IV acquisition cost | SR-71 programme — as a measure of the extrapolation, not a check | **done**, absolute dollars unanchored |
| M12 | Regime sweep: where the chain stops answering | each module's own declared validity bound | **done** |
| Track | Dual-mode ram/scram (Mach 5 stretch) | X-43/X-51 class as *regime* anchors, not copy-paste numbers | **stub** ([ADR-003](docs/adr/ADR-003-dual-mode-scram.md)) |

```
cargo test --workspace     217 tests
cargo xtask validate       107 cases: 97 pass, 0 fail, 10 known limit, 0 stale
                           28 modelling constants still [TO CITE]
native\build_cpu.bat      M9 level D: shock angle 0.006 deg against exact
cargo xtask bench          gated on the corpus passing at the same commit
```

**New here?** [`docs/PRIMER.md`](docs/PRIMER.md) walks the whole causal chain —
why a Mach number fixes the altitude, which fixes the material, which fixes the
engine architecture — using this repository's own numbers. It is the document to
read if you want to understand high-speed flight rather than this codebase.

Not every module belongs in that corpus, and the reason is declared per crate
rather than left to inference. The corpus holds claims traceable to a **published
external number**; M8's yardsticks are conservation laws and a convergence order,
and M10's is bit-for-bit agreement with M1, which is a cross-check against this
project's own code. Neither is a citation, so forcing them in would mean writing
a `source` field that cites ourselves — the exact drift the mandatory source
exists to stop. `xtask validate` prints those modules and the argument for each,
and refuses a crate that declares no route at all (ADR-000 D12).

The ten known limits are not failures being tolerated. Each is a case that is
**expected to fail**, with a written reason. If one ever starts passing, the
harness reports `STALE_KNOWN_LIMIT` and **fails the build** — the limitation
is gone, so the annotation has become a false claim in the report, and the
remedy is to delete it. The build likewise refuses a run whose case corpus has
shrunk below a declared floor, because a harness that has found nothing to
check must not be able to print a pass.

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

**A physics identity catching a numerical-methods bug.** The inlet optimiser used
a golden-section line search. Ramp angles that detach the shock evaluate to
negative infinity, so the recovery surface has large infeasible plateaus, and
golden section assumes unimodality: with both probes on the plateau the bracket
always walks the same way and converges to the infeasible boundary. At M 2.5 it
returned its own starting point unchanged and reported it as an optimum. Nothing
about the recovery number looked wrong. What caught it was Oswatitsch's
criterion — the optimum should have equal-strength shocks, and this one did not.
The true optimum recovers 0.9225 against the 0.9131 being reported.

**Two guesses the model overruled.** A test asserted three inlet ramps would
clear the MIL-E-5008B target; they give 0.7311 against 0.7416, and it takes four.
Another asserted that ramjet specific thrust rises monotonically with Mach; it
peaks and then collapses, because the burner temperature limit caps exit velocity
while flight speed keeps rising — which is precisely why a ramjet has a design
Mach number. Both assertions were written before the module could answer, and
both were wrong.

Full record in `docs/adr/`, in the case files, and in the commit messages. Every
correction is marked `[CORRECTED]` where it lives.

---

## Design point

M 3.50 at 26 km geopotential, US Standard Atmosphere 1976.
Full derivation and provenance in [`docs/design-point.md`](docs/design-point.md).
A proposed Mach 4 constant-q row — not a close, not a replacement of this
snapshot — is sketched in [`docs/design-point-m4.md`](docs/design-point-m4.md).

| | |
|---|---|
| T∞ / p∞ / ρ∞ | 222.65 K · 2 153.09 Pa · 0.0336882 kg·m⁻³ |
| V∞ | 1 046.95 m·s⁻¹ (3 769 km·h⁻¹) |
| q∞ | 18.463 kPa |
| T₀ | **752.8 K** (thermally perfect; 768.1 K if γ = 1.4) |
| T_aw | **709.3 K / 436.2 °C** (no radiation) |
| **Skin, radiating** | **548.3 K / 275.2 °C** at 10 m — computed, ε = 0.85 |
| Inlet recovery | 0.742, against 0.213 for a normal shock — **a factor of 3.48** |
| Zero-fuel mass | **13 624 kg** — derived from 28 t cruise at the *cited* SR-71 zero-fuel fraction |
| Cruise range | **4 265 — 5 714 km** — reserve-limited; no reserve policy chosen |

**The corridor the design point actually sits in.** M12 runs the whole chain
from M 2 to M 9 at constant dynamic pressure — the rule that picked 26 km — and
records where each module **refuses**, never extrapolating past one.

| | |
|---|---|
| Specific thrust peaks | **M 2.30** |
| Design point | **M 3.50**, at 83.7 % of peak thrust |
| Lean blowout **[TO VERIFY]** | **M 3.23 — 4.42** (phi 0.5 to 0.3 literature). **Operative: phi 0.50** (no holder declared); design phi 0.4615 sits below it |
| Required capture area = whole body cross-section | **M 3.85** |
| No body size closes the balance *(inside the row above, not past it)* | M 4.54 |
| Every module still answers to | **M 5.65** (four-ramp inlet) |
| M4 burner ceiling | M 5.70 — never the operative limit |

**Two of those decide whether the aircraft flies, and neither is the ceiling.**
The cycle runs at an equivalence ratio of **0.4615** at the design point — inside
the literature band and above its midpoint. No flame holder is declared, so the
operative bound is the strict end 0.50 `[TO VERIFY]`, not the permissive 0.30.
Every value of that strict end in the ordinary no-holder range puts M 3.50 out
of reach. Burner-entry *total* pressure is ~122–133 kPa (ram), not the 1.6 kPa
omitted-ram trap. And the inlet already needs **74.5 %** of the entire body
cross-section at the design point; past M 3.85 the configuration M6b assumed is
self-inconsistent, because the Sears-Haack body that sets the wave drag cannot
host an inlet larger than itself. Wave drag goes as the *square* of
cross-section, so that is a fixed point rather than a formula — it converges
only because wave drag is 8.9 % of the total and lift-induced dominates. Solved
as a quadratic, the roots vanish entirely above M 4.54: no body size closes the
balance at all. That is **not a second usable limit** — past M 3.85 the inlet
already exceeds the body carrying it, so M 4.54 sits inside a region M 3.85 has
excluded. It says how the failure happens, not how far the aircraft gets.

M3 now owns the capture-vs-body identity, spilled *area*, the additive-drag
*definition* (force refused without a cowl lip), and Kantrowitz starting
(`A_e/A_t ≈ 1.45` at M 3.5 against isentropic `A/A* ≈ 6.79` — why a spike
exists). The translating-spike *schedule* and unstart *dynamics* remain typed
refusals. Four of those five frontiers are statements about the **model**. Only
the capture area is a statement about the **aircraft**.

The material never binds, and that is a mechanism rather than an assertion: the
balance is `eps sigma T_w^4 = h (T_aw — T_w)`, so the fourth root crushes
everything on the right, and film-temperature (Eckert) evaluation makes `h` fall
faster with altitude than freestream scaling predicts. **There is no single
exponent** — the local slope moves from −0.26 to −0.02 across the sweep, and
quoting one value would be the same error as quoting the textbook one.

**What it costs is the one question the project answers badly, and says so.**
M11 runs DAPCA IV anchored on the SR-71. The anchor is already **1.54x past** the
velocity the model was fitted over; VENTUS-1 is 1.70x past. Two results survive
that: moving M 3.2 — M 3.5 costs **+7.4 %** while aluminium — titanium costs
**+21.4 %**, so the model reaches the physics that actually makes M 3.5 hard only
through a fudge factor a human types in. And a 14 t aircraft built six times
costs **more per airframe** than a 30 t one built thirty-two times — the
production run dominates the aeroplane. The absolute dollars are carried as a
failing `known_limit`: no primary source for an SR-71 programme cost has been
read, so the module refuses to emit a verified figure at all. See
[`docs/adr/ADR-002-cost.md`](docs/adr/ADR-002-cost.md).

**Why M 3.5 and not M 3.75.** Above roughly M 3.35 there is no public vehicle to
check against until the scramjet demonstrators, which are a different regime.
At M 3.76 the SR-71 thrust-split anchor becomes an extrapolation rather than a
check, and two modules would lose their yardstick entirely. M 3.5 sits 0.15 Mach
from the D-21 and 0.3 from the SR-71, so both remain checks. The altitude was
chosen to hold dynamic pressure constant against the earlier M 3.0 baseline, and
holds it to 0.004 % — so the re-baseline hardens the thermal and propulsive
problems without touching the structural loads case.

**Programme track, and what is next.** Cruise ≥ Mach 4 with a Mach 5 stretch is
the *intent*, not a second design point. The validated snapshot remains **M 3.50
at 26 km** until a future re-baseline lands in code. The Mach 4 path is a sketch
only — [`docs/design-point-m4.md`](docs/design-point-m4.md) — and on the current
geometry capture already exceeds the body (M 3.85 bind; ratio 1.15 at M 4.00).
Dual-mode / scram is a separate crate (`ventus-scram`) that currently
**refuses**: there is no Isp and no thrust at Mach 5 to quote, and M4's model
refusal near M 5.65–5.70 is still a subsonic-combustion ramjet burner limit, not
aircraft capability. Decision and non-goals:
[`docs/adr/ADR-003-dual-mode-scram.md`](docs/adr/ADR-003-dual-mode-scram.md).

| | |
|---|---|
| Validated design point | M 3.50 @ 26 km — case-gated snapshot |
| Proposed M 4 row | sketch only: [`docs/design-point-m4.md`](docs/design-point-m4.md) — **does not close** |
| Current configuration bind | capture = body ~ M 3.85 (M12); ratio 1.15 at proposed M 4.00 |
| Lean blowout (operative) | no holder declared → strict end φ 0.50 `[TO VERIFY]`; design φ 0.4615 sits below it |
| Ideal ramjet model refusal | ~ M 5.65–5.70 — **not aircraft capability** |
| Dual-mode / scram cycle | `ventus-scram` stub — stations not modelled |

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
  confined to the single GPU module. Dual-mode / scram is a separate crate
  (`ventus-scram`, [ADR-003](docs/adr/ADR-003-dual-mode-scram.md)), not an
  extension of `ideal_ramjet` past its validity.
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
                 scram (dual-mode track, stub), aero, thermal, mass, cost,
                 envelope, dynamics, fsw, validate, xtask
native/          C++17/CUDA, sm_120 — M9 only
crates/*/cases/  the external yardsticks themselves, as TOML. Each case
                 carries its own source, tolerance and reason; there is no
                 separate reference directory (ADR-000 D11).
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
