# VENTUS — benchmarks

**Revision:** r1 (2026-09-17)
**Command:** `cargo run --release -p xtask -- bench`

---

## 0. The gate, which is the point

ADR-000 D3 and `native/ventus_cfd/euler2d.cu` already carry the rule this
document extends to the Rust side:

> **A benchmark from unvalidated code is worse than no benchmark**, because it is
> a number that looks like evidence and is not.

`xtask bench` therefore refuses to time anything in three situations:

| Refusal | Why it is a refusal and not a warning |
|---|---|
| **Debug build** | Roughly 20× slower on this workload. Not a slow measurement — a different one. |
| **Case corpus does not pass** | The number would describe code this project does not certify. |
| **Corpus below the declared floor** | A gate that checks nothing cannot open. |

A **dirty tree** is not a refusal: the report labels itself `NOT REPRODUCIBLE`
in its own header, exactly as the validation report does.

The per-run artifacts (`out/<run_id>/bench.md`, `bench.csv`) are **gitignored**.
What is committed is this document and the harness that produces them.

---

## 1. Method, stated so the numbers can be read

Wall clock, `std::time::Instant`, single thread, release profile. Three warm-up
batches discarded, then seven timed batches; the reported figure is the **median
of the batch means**, with the **best** batch also given because it is least
contaminated by scheduling.

`std::hint::black_box` on every result. Without it the optimiser is entitled to
delete a pure call whose value is discarded, and the benchmark would report the
cost of an empty loop.

**No `criterion`.** This workspace has four dependencies in total and a benchmark
harness is not worth a fifth. The consequence is stated rather than hidden: these
numbers are good to a **few per cent, not to one**, and nothing below leans on a
difference smaller than that.

### What is not claimed

Every figure is this workspace measured against **itself**, on one machine, in
release. There is no comparison against any other implementation, and no
statement of the form *"N times faster than X"* appears anywhere in this project,
because nothing here has been measured against an X.

---

## 2. Results

Measured at commit `a9c0b30`, rustc 1.91.1, release.

| Module | Subject | Median | Best | Iters/batch |
|---|---|---|---|---|
| M2 | `stagnation_temperature_ratio` | **1 ns** | 1 ns | 1 000 000 |
| M1 | `at_geopotential` (design altitude) | **100 ns** | 99 ns | 100 000 |
| M4 | `ideal_ramjet` (design point) | **228 ns** | 216 ns | 20 000 |
| M5 | `radiation_equilibrium_wall` (10 m) | **10.24 µs** | 9.92 µs | 2 000 |
| M3 | `optimise_ramps`, 2 ramps | **71.4 ms** | 69.3 ms | 20 |
| M3 | `optimise_ramps`, 4 ramps *(design inlet)* | **220.8 ms** | 208.1 ms | 10 |
| M12 | `evaluate` (whole chain, one Mach) | **213.3 ms** | 201.4 ms | 10 |

---

## 3. The result this exists to report

**The whole M12 chain costs the same as the inlet optimiser alone.**

```
optimise_ramps, 4 ramps   220.8 ms
the whole M12 chain       213.3 ms
everything else, summed    ~0.011 ms
```

The two top figures are the same number within measurement spread. Atmosphere,
compressible flow, the ramjet cycle and the radiation balance together account
for roughly **0.005 %** of the chain. One function is the entire cost.

That is not a curiosity — it **changed the design of M12**. The regime sweep was
originally written as five scans, one per module refusal, at 0.01 Mach. That is
3 500 evaluations at a fifth of a second each: **over nine hours**. It was
restructured into a single pass, and the ramp count fixed at the design inlet's
four rather than swept over one to four, purely because of this measurement.

Both decisions are documented where they were made, in `ventus-envelope`, and
neither would have been taken without timing the code first.

### The scaling in ramp count

```
2 ramps   71.4 ms
4 ramps  220.8 ms      ×3.09 for double the ramps
```

Super-linear, which is expected for an optimiser over a ramp-angle vector whose
feasible region has infeasible plateaus — the same structure that produced the
golden-section bug M3 already records. **[TO QUANTIFY]** whether the exponent is
the search itself or the shock-train evaluation inside it; this measurement does
not separate them.

---

## 4. The number that is a budget, not a curiosity

**`at_geopotential` costs 100 ns.**

M1 is the one module that runs inside the flight software, `no_std` and without
allocation, and M10 shares it bit-for-bit with the digital twin. At a 100 Hz
control loop that is 10⁻⁵ of the budget per call — which is the answer, and the
reason the figure is worth having rather than assuming.

It is also the only row here whose value would matter if it changed by 3×.
Everything else is either negligible (M2, M4, M5) or already known to dominate
(M3).

---

## 5. Open

- **[TO QUANTIFY]** the super-linear ramp scaling: search iterations versus
  shock-train evaluations. A counter inside `optimise_ramps` would settle it.
- **[TO COMPUTE]** M9's CUDA timings. The solver already gates its own benchmarks
  on levels A and B, and both are `BLOCKED` by the CUDA 13 / VS 18 toolchain
  mismatch recorded in `native/ventus_cfd/README.md`. The CPU path is validated
  at level D but is not timed here, because a CPU-only number for a module whose
  whole argument is the GPU would be the wrong measurement presented as the
  right one.
- **No cross-machine comparison.** Every figure is one machine. Reproducing on a
  second would say something about portability that this document currently
  cannot.
