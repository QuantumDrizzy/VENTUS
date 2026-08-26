# VENTUS-1 — Design point

**Revision:** r2 (2026-08-27) · supersedes r1
**Change vs r1:** §3 rebuilt — calorically imperfect stagnation state computed by
enthalpy conservation instead of constant-γ. §5 added (declared gaps). §6 L/D
guard made two-sided.

**Condition:** M = 3.00, h = 24 000 m, U.S. Standard Atmosphere 1976.
**Baseline gas model:** calorically perfect air, γ = 1.4, R = 287.0528 J·kg⁻¹·K⁻¹.
Validity bounds and the correction are in §3 and in ADR-000 D10.

---

## 1. Environment

| Quantity | Value | Provenance |
|---|---|---|
| Geopotential altitude | 24 000 m | design choice |
| Geometric altitude | 24 090.3 m | h_geom = h_geop·r/(r − h_geop), r = 6 356 766 m |
| T∞ | **220.65 K** (−52.50 °C) | US76 layer 20–32 km: T = 216.65 + 1.0·(h − 20 km) |
| p∞ | **2 930.4 Pa** | barometric integration from p(20 km) = 5 474.89 Pa, exponent g₀M/(R·L) = 34.1632 |
| ρ∞ | **0.046266 kg·m⁻³** | p/(R·T) |
| a∞ | **297.78 m·s⁻¹** | sqrt(γRT) |
| μ∞ | **1.4436 × 10⁻⁵ Pa·s** | Sutherland, μ = 1.458e-6·T^1.5/(T + 110.4) |

> The geopotential/geometric distinction is **not** cosmetic here: 90.3 m of
> altitude at this level is ≈ 1 % in density. M1 implements both explicitly.

## 2. Flight condition

| Quantity | Value | Provenance |
|---|---|---|
| V∞ | **893.34 m·s⁻¹** = 3 216 km·h⁻¹ | M·a |
| q∞ | **18.46 kPa** | 0.7·p·M² |
| p₀ (isentropic, γ = 1.4) | **107.6 kPa** | p·(1 + 0.2M²)^3.5, factor 36.73 |
| Re/L | **2.86 × 10⁶ m⁻¹** | ρV/μ — fully turbulent over the whole vehicle |
| Specific kinetic energy V²/2 | **399.03 kJ·kg⁻¹** | drives §3 |

## 3. Thermal state — the result that selects the material

### 3.1 Three gas models, and why the middle one is wrong

At T₀ ≈ 600 K the vibrational modes of O₂ and N₂ begin to populate: cp is no
longer constant, and γ = 1.4 no longer holds. The naive correction is to
re-evaluate γ at the stagnation temperature and reuse the isentropic formula.
**That is a methodological error.** `T₀/T = 1 + (γ−1)/2·M²` is algebraically
identical to ΔT = V²/(2·cp) with **constant** cp. Feeding it γ = 1.376 applies
cp = 1.0505 kJ·kg⁻¹·K⁻¹ across the entire 220 → 600 K rise, where the true cp
starts at 1.003. It charges the hot cp over the cold leg.

The correct condition is energy conservation with real enthalpy:

```
h₀ = h∞ + V²/2
h(220.65 K) = 220.62 kJ/kg          (air ideal-gas tables; h(220 K) = 219.97)
h₀           = 220.62 + 399.03 = 619.65 kJ/kg
h(610 K) = 617.53 , h(620 K) = 628.07   ->  T₀ = 612.0 K
```

Cross-check by interval-mean cp: 399.03/(612.0 − 220.65) = **1.0195 kJ·kg⁻¹·K⁻¹**.
Simpson over the cp table (1.003 @ 220 K, 1.015 @ 416 K, 1.053 @ 612 K) gives
1.0193. Closes to 0.02 %.

| Model | cp [kJ·kg⁻¹·K⁻¹] | T₀ | T_aw (r = Pr^⅓ ≈ 0.89) |
|---|---|---|---|
| Calorically perfect, γ = 1.4 | 1.0047 (const) | 617.8 K / 344.7 °C | 573.4 K / 300.3 °C |
| γ(T₀) = 1.376 in constant-γ formula — **wrong** | 1.0505 (const) | 594.0 K / 320.9 °C | 552.9 K / 279.8 °C |
| **Thermally perfect, h₀ = h + V²/2** — **correct** | 1.0195 (mean) | **612.0 K / 338.9 °C** | **569.0 K / 295.9 °C** |

**Real correction: −5.7 K on T₀ (−0.9 %), −4.4 K on T_aw.**
The r1 baseline was very nearly right; the naive fix would have been wrong by
about 4×. Both rows are kept: the comparison is the teaching.

Source for cp(T): standard air ideal-gas property tables (Cengel A-2b / A-17
lineage). **[TO VERIFY]** — to be pinned to a primary source (NIST-JANAF or
NASA Glenn thermodynamic coefficients) in `ventus-gasdyn/cases/cp_air.toml`
before M2 closes.

### 3.2 Skin temperature

| Quantity | Value | Note |
|---|---|---|
| T₀ (thermally perfect) | 612.0 K / 338.9 °C | §3.1 |
| T_aw, turbulent | 569.0 K / 295.9 °C | **upper bound with no radiation** |
| Skin at radiative equilibrium, flat panels | ~470–530 K **[TO COMPUTE, M5]** | balance q_conv = εσT⁴, ε ≈ 0.85 |
| Skin at nose / leading edges | ~535–555 K **[TO COMPUTE, M5]** | anchor: SR-71 nose ≈ 588 K at M 3.2 |

### 3.3 Material survival at ~296 °C recovery temperature

| Material | Sustained limit | ρ [kg·m⁻³] | Verdict |
|---|---|---|---|
| Al 2024-T3 / 7075 | ~120 °C | 2 780 | **dead** |
| Al 2618 (RR58 — Concorde skin) | ~127 °C | 2 760 | **dead** — Concorde was already at its limit at M 2.04 |
| **Ti-6Al-4V** | ~350–400 °C (creep-limited) | 4 430 | **alive**, with real structural margin |
| Ti β (B-120VCA — SR-71) | ~500–550 °C | 4 850 | alive, oversized for M 3.0 |
| 17-7PH stainless | ~430 °C | 7 800 | alive but **+76 % density** |
| Inconel 718 / René 41 | > 650 °C | 8 190 | leading edges and engine hot section only |

> **M5 result [TO PROVE, not asserted]:** M 3.0 at 24 km is a **titanium airframe**
> point, not a superalloy point. Aluminium is excluded by a factor > 2 in
> temperature. The margin to Ti-6Al-4V's creep limit is ~55–105 K, and the
> calorically imperfect correction moves it by only 4 K — so **the conclusion is
> robust, not marginal.** That robustness is itself the finding.

**Thermal growth (first order, to be redone in M5).** Ti-6Al-4V, α ≈ 8.6 × 10⁻⁶ K⁻¹,
ΔT ground (288 K) → cruise skin (~510 K) ≈ 222 K:

```
ΔL/L = 1.9 × 10⁻³  =  1.9 mm per metre of airframe
```

Reported per unit length because vehicle length is not yet defined (§5.2). This
is the quantitative form of "the SR-71 leaked fuel on the ground", and it is a
module deliverable, not an anecdote.

### 3.4 [TO PROVE] — "M 3.0 is the cheap point of the Mach 3 regime"

The thermal leg of this claim is shown: going to M 3.2 raises T₀ by ~55 K
(612 → ~667 K, thermally perfect) and starts consuming the Ti-6Al-4V creep
budget, which is why the SR-71 went to β-titanium.

**The other legs are not shown.** Inlet total-pressure recovery and ramjet-mode
Isp both vary with M and not necessarily in the same direction — higher M
improves ram compression and cycle efficiency even as it worsens the thermal and
recovery problem. The claim is marked `[TO PROVE]` pending M3 + M4, and must not
be stated as a result until then.

## 4. Acceptance anchors per module

| Module | Yardstick | Numeric criterion |
|---|---|---|
| M1 | US76, published table | ≤ 1e-6 relative, layer by layer, including gradient discontinuities |
| M2 | NACA 1135. Normal shock M = 3: p₂/p₁ = 10.3333, T₂/T₁ = 2.6790, M₂ = 0.475191, p₀₂/p₀₁ = 0.328344 | ≤ 1e-9 relative (closed form) |
| M2 | γ validity | `[KNOWN_LIMIT]` case; quantify error at T₂ = 591 K post-shock |
| M3 | Total-pressure recovery ≥ 0.80 at M 3 (MIL-E-5008B empirical: 1 − 0.075(M−1)^1.35 = 0.809) vs 0.328 for a normal shock | the ×2.46 in p₀ **is** the reason the inlet exists |
| M3+M4 | SR-71 cruise thrust split: inlet ≈ 54 %, ejector nozzle ≈ 29 %, engine ≈ 17 % | ±10 percentage points |
| M4 | Specific-work collapse: T₄max/T₀ = 1700/612 = 2.78 at M 3 vs 5.9 at sea level static | reproduce why a pure turbojet dies |
| M4 | γ(T) in burner and nozzle (ADR-000 D10) | γ = 1.4 is a **fail**, not a limit, in this module |
| M6 | Supercruise L/D: Concorde ≈ 7.5 @ M 2.04, SR-71 ≈ 6 @ M 3.2 | target **5.5–6.5**. **Two-sided guard: > 8 is a bug; < 4 is either a bug or a bad configuration — the harness must distinguish them** (a bug fails; a bad configuration is a finding) |
| M7 | SR-71 empty mass / MTOW / unrefuelled range **[TO CITE]** | pinned to a primary source in `ventus-mass/cases/` |
| M8 | Energy conservation of the integrator, ballistic, no atmosphere | drift ≤ 1e-10 relative over 10⁶ steps |
| M9 | Oblique shock angle vs exact θ-β-M from M2 | ≤ 0.5° |
| M9 | CPU vs GPU | ADR-000 D3, levels A–D |

## 5. Declared gaps

These are deliberate deferrals, not oversights. Each names the point at which it
becomes blocking.

### 5.1 No acceleration corridor — blocking at M4 engine sizing

Everything above is a cruise snapshot. On a Mach 3 aircraft **the engine is not
sized by cruise, it is sized by the transonic pinch**: wave drag peaks around
M 1.0–1.2 where thrust margin is worst. The SR-71 ran in afterburner from
takeoff until well past M 2. A design point without the corridor describes an
aircraft that is already flying and says nothing about how it got there.

Representative schedule **[PLACEHOLDER — the schedule itself is an M6+M4 deliverable]**:

| M | h [m] | T∞ [K] | p∞ [Pa] | q [kPa] | T₀ [K] (γ = 1.4) | T₀ [K] (thermally perfect) |
|---|---|---|---|---|---|---|
| 0.90 | 11 000 | 216.65 | 22 632 | **12.83** | 251.7 | 251.7 (Δ < 0.1 K) |
| 1.20 | 12 000 | 216.65 | 19 330 | **19.49** | 279.0 | 279.0 (Δ < 0.1 K) |
| 2.00 | 18 000 | 216.65 | 7 505 | **21.01** | 390.0 | 389.9 (Δ = 0.1 K) |
| 3.00 | 24 000 | 220.65 | 2 930 | **18.46** | 617.8 | **612.0 (Δ = 5.7 K)** |

Two readings, both useful:

- **q peaks in the corridor, not at cruise** (21.0 kPa at M 2.0 vs 18.5 at M 3.0).
  Structural loads are not set by the design point either.
- **The calorically imperfect correction is negligible below M ~2.5** and only
  matters at the design point. This bounds where D10 has to be taken seriously.

Location of min(T − D) — **[TO DETERMINE, M6 + M4]**. Expected near M 1.1 ± 0.1.

### 5.2 No geometry — blocking at the start of M6

There is no length, no wing area, no mass class, no configuration. M6 (area rule,
Sears-Haack), M7 (mass fractions) and M8 (inertia tensor) are **unbuildable**
without them. **Intent: derive geometry from the physics, not assume it.** This is
a deliberate deferral, and it stops being acceptable the day M6 starts.

What q already gives — the first number connecting the design point to an aircraft:

```
W/S = q · C_L
  C_L = 0.10  ->  W/S = 1.846 kPa =  188 kg/m²
  C_L = 0.15  ->  W/S = 2.769 kPa =  282 kg/m²
```

**Which C_L?** Back-calculating the SR-71 at its own cruise condition
(M 3.2, 24 km → q = 21.0 kPa; mid-cruise mass ≈ 55 t, S = 167.2 m² **[TO CITE]**)
gives W/S = 3.23 kPa and **C_L ≈ 0.154**. So C_L = 0.10 describes a lighter,
larger-winged aircraft than the SR-71, not a comparable one. The **188 kg/m²
figure is not the SR-71-like answer** — 282 kg/m² is closer. Carry both until M6
picks one, and pick it for a stated reason.

## 6. Reproduction

Every number in §1–§3 is regenerated by `cargo xtask report` once M1 and M2 exist,
and every one carries a case file in `crates/*/cases/`. Until then this document
is hand-computed and each entry is marked with its provenance. Entries marked
**[TO COMPUTE]**, **[TO VERIFY]**, **[TO CITE]**, **[TO DETERMINE]** or
**[TO PROVE]** are open and must not be quoted as results.
