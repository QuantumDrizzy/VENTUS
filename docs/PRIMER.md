# A primer on why a Mach 3.5 aircraft is shaped the way it is

Written for someone who builds systems, not airframes. Every number here is
produced by this repository and checked against a published source; nothing is
illustrative.

The point of this document is that **high-speed flight is one causal chain, not a
collection of disciplines.** You pick a Mach number, and everything else —
altitude, engine architecture, material, structural clearances, even the shape of
the air intake — follows from it. If you can walk the chain, you can hold the
conversation.

---

## 1. One equation starts everything

Air brought to rest against a moving body heats up. Not from friction — from the
kinetic energy of the flow becoming thermal energy:

```
h₀ = h + V²/2
```

Enthalpy plus kinetic energy is conserved. For a constant-cp gas this becomes the
form you will see quoted everywhere:

```
T₀/T = 1 + (γ−1)/2 · M²
```

**At M 3.5, T₀/T = 3.45.** The air at −50 °C outside arrives at the vehicle
surface at nearly 500 °C. That single ratio is the origin of the thermal problem,
the material problem, and the engine problem.

> **The trap, and it is a real one.** At 750 K, γ is no longer 1.4 — it is about
> 1.358, because vibrational modes in O₂ and N₂ have started absorbing energy.
> The obvious fix is to put the hot γ into the formula above. **That is wrong.**
> The formula is `ΔT = V²/(2cp)` with *constant* cp, so feeding it the hot γ
> charges the hot cp across the entire temperature rise, including the cold part
> where cp is still 1.003.
>
> Correct: −15.3 K. Naive fix: −56.7 K. Wrong by a factor of 2.7, and in the
> direction that looks like diligence. `docs/design-point.md` §3.1.

## 2. Altitude is the free variable, and it buys almost everything

Aerodynamic and structural loads scale with **dynamic pressure**, not speed:

```
q = ½ρV²  =  0.7 · p · M²      (for air, γ = 1.4)
```

Density falls exponentially with altitude, so you can go much faster at the same
structural load by going higher. This project used that deliberately:

| | M 3.0 at 24 km | M 3.5 at 26 km |
|---|---|---|
| q | 18.462 kPa | **18.463 kPa** |

Half a Mach number faster for **0.004 %** more dynamic pressure. The structure
does not notice. What *does* change is heating and propulsion, which scale with
Mach and not with q.

**The thing to say:** "altitude is how you buy Mach without buying loads. It does
nothing for the heating."

## 3. Air resists being slowed down, and the currency is total pressure

**Total pressure** `p₀` is the pressure the flow would reach if brought to rest
without loss. It is the measure of how much useful work the stream can still do —
the thing an engine spends to make thrust.

Slowing supersonic air requires a **shock wave**, and a shock destroys total
pressure irreversibly. At M 3.5:

```
one normal shock:   p₀₂/p₀₁ = 0.213      79 % of the useful energy, gone
```

You cannot avoid the shock — a combustor needs subsonic air. But you can choose
how you take it. Several **weak oblique shocks** in sequence, each turning the
flow a little, destroy far less than one strong normal shock:

| shocks | recovery | flow turning needed |
|---|---|---|
| 1 (normal only) | 0.213 | 0° |
| 2 | 0.610 | 37° |
| 3 | 0.731 | 44° |
| 4 | **0.809** | **48°** |
| MIL-E-5008B, real inlet | 0.742 | — |

**This is what an inlet is.** The angled ramps and the moving spike on a
high-speed aircraft are not aerodynamic decoration; they are a machine for
staging the deceleration so it costs less.

> **A result worth knowing.** Reaching the empirical curve for a real inlet at
> M 3.5 takes four oblique ramps and about **48 degrees of total flow turning**.
> Turning the flow 48° on an *external* surface makes an enormous cowl, and every
> degree is frontal area and wave drag. That is why real Mach 3 inlets are
> **mixed compression** — some shocks folded inside the duct, where they cost
> length instead of frontal area — and why they need a translating spike to hold
> that internal shock system as conditions change. Losing it is an **unstart**,
> and it is violent.

There is also a beautiful optimality result here. The best ramp angles are the
ones that make every oblique shock **equal strength** — equal Mach number normal
to each wave. That is Oswatitsch's criterion, and in this repo it was not
assumed: a numerical optimiser that knows nothing about it converges on it.
`crates/ventus-inlet`.

## 4. Why the inlet is most of the engine

Because total pressure is what thrust is bought with, and at M 3.5 the inlet
controls a factor of **3.48** in it (0.742 against 0.213). No component
downstream has that kind of leverage.

On the SR-71 this is usually quoted as a thrust split: at cruise the **inlet
produced about 54 %** of the propulsive force, the ejector nozzle 29 %, and the
turbojet core only **17 %**. The engine was, in a real sense, an accessory.

> **What this repo does and does not claim.** The cycle model shows that specific
> thrust rises monotonically with inlet recovery — the leverage is real and
> quantified. The 54/17/29 split itself is an *axial force accounting* over each
> piece of the flowpath, which needs the pressure distribution on the compression
> surfaces and the cowl. A quasi-1D model does not carry that, so it is declared
> as a gap rather than approximated into existence.

## 5. Why the turbomachine dies

A turbojet's compressor is driven by a turbine, and the work available depends on
the temperature headroom between the compressor face and the turbine's material
limit (~1700 K). Ram compression has already spent that headroom:

```
sea level, stationary:   T₄max / T₀ = 1700 / 288  =  5.90
M 3.5 at 26 km:          T₄max / T₀ = 1700 / 768  =  2.21
```

**This is thermodynamics, not metallurgy.** No compressor alloy fixes it. The air
arrives already hot, so there is little room left to heat it further, so there is
little work to extract, so the compressor is not earning its mass.

Past roughly M 3 the sensible move is to delete the turbomachine and let the
inlet do the compressing. That is a **ramjet**, and it has two defining
properties that fall directly out of the model:

- **It produces no static thrust.** No forward speed means no ram compression
  means no pressure ratio to expand across. A ramjet cannot take off; it needs a
  booster or a turbomachine to get it to speed.
- **Its specific thrust peaks and then collapses.** Thrust is `(1+f)V₉ − V₀`. The
  exit velocity `V₉` is capped by the burner temperature limit, but flight speed
  `V₀` keeps rising. The difference has a maximum — and that maximum *is* the
  engine's design Mach number.

The SR-71's J58 was the hybrid answer: a turbojet at low speed that progressively
bypassed air around the core straight into the afterburner, becoming
ramjet-dominated by M 3.

## 6. Why it gets hot — and why radiation is what saves it

The surface does not reach the full stagnation temperature. A boundary layer
recovers only part of it:

```
T_aw = T + r (T₀ − T)        r = Pr^⅓ ≈ 0.89 for a turbulent layer
```

At the design point that is **709 K (436 °C)**. Aluminium is finished at ~150 °C.
Ti-6Al-4V — ordinary aerospace titanium — is creep-limited near **350 °C**. So
the recovery temperature is *above* the limit of the obvious material.

But `T_aw` is the temperature of a wall that cannot lose heat. A real skin
radiates, and at 26 km it radiates to a very cold sky. Steady state is where
convection in equals radiation out:

```
h (T_aw − T_w) = ε σ T_w⁴
```

**Solving it gives 548 K (275 °C) — 75 K *below* the titanium limit.**

| | |
|---|---|
| T_aw, no radiation | 709 K — above the limit |
| **Ti-6Al-4V limit** | **623 K** |
| T_wall, radiating | 548 K — 75 K of margin |
| **what radiation is worth** | **161 K** |

> **The design constraint this produces is not "use titanium".** It is that
> **the margin belongs to the radiation term, not to the material.** Buried
> structure, an internally insulated bay, or a low-emissivity surface does not
> get those 161 K and needs β-titanium or a superalloy. The recovery temperature
> alone would never have surfaced that.

Validation: the same model at the SR-71's own cruise condition gives 270 °C
against its published 250–300 °C skin band, and falls below the band toward the
tail — which is the model stating its own scope, since a flat plate has no nose,
chines or leading edges.

**And then the structure moves.** Titanium expands ~8.6 × 10⁻⁶ per kelvin. From a
288 K hangar to a 548 K cruise skin, that is 2.2 mm per metre — about **7 cm on a
30 m airframe**. Every panel joint, every fuel line, every control run has to
accommodate that. It is why the SR-71's tanks sealed only once hot, and leaked
fuel on the ground. Not a defect: a consequence.

## 7. The quantities, and what each one actually controls

| Symbol | Name | What it decides |
|---|---|---|
| `M` | Mach number | everything downstream; the design variable |
| `q = 0.7pM²` | dynamic pressure | structural loads, wing area, control authority |
| `T₀` | stagnation temperature | the thermal problem, and the engine's headroom |
| `T_aw` | recovery temperature | the ceiling for skin temperature |
| `T_w` | wall temperature | the actual material choice, after radiation |
| `p₀` | total pressure | the currency thrust is bought with |
| `π_d = p₀₂/p₀₁` | inlet recovery | how much of that currency survives the inlet |
| `γ` | ratio of specific heats | 1.4 cold, ~1.30 in a burner; using the wrong one falsifies thrust |
| `Re/L` | Reynolds number per metre | laminar or turbulent, hence heating and drag |
| `Pr` | Prandtl number | how well the boundary layer moves heat vs momentum |
| `ε` | emissivity | how much heat the skin can throw away |
| `L/D` | lift-to-drag ratio | range, via Breguet |
| `Isp` | specific impulse | fuel burned per unit thrust per second |
| `T₄max` | burner exit limit | the ceiling the whole cycle works under |
| `r = Pr^⅓` | recovery factor | the fraction of `T₀ − T` the wall actually feels |

## 8. The chain, compressed

If you have sixty seconds:

> You pick Mach 3.5. That fixes the stagnation temperature ratio at 3.45, so
> −50 °C air arrives at 500 °C. You pick altitude to hold dynamic pressure, so
> the structure sees the same loads as a slower aircraft — 26 km does that. The
> heating does not go away: recovery temperature is 436 °C, above the limit of
> ordinary titanium, and only radiation to a cold sky brings the skin down to
> 275 °C. So the margin lives in the radiation term, and anything that cannot
> radiate needs a better alloy. Meanwhile the air has to be slowed to subsonic
> for the burner, and one normal shock would destroy 79 % of the total pressure,
> so you stage it through four oblique shocks and recover 74 % instead — which is
> a factor of 3.5, which is why the inlet produces more thrust than the engine
> does. And the ram compression has already eaten the temperature budget the
> compressor needed, so past Mach 3 the turbomachine stops earning its mass and
> you are building a ramjet. Which cannot take off, and whose thrust peaks and
> then falls, because the burner caps exit velocity while flight speed keeps
> rising.

Every number in that paragraph is computed in this repository and checked against
a published source. Where the model runs out — leading-edge heating, the thrust
split, anything needing vehicle geometry — it is marked as open rather than
filled in.

---

## Where to look

| You want | Read |
|---|---|
| The atmosphere | `crates/ventus-atmos` — US Standard Atmosphere 1976 |
| Shocks, expansions, γ | `crates/ventus-gasdyn` — NACA Report 1135 |
| The inlet | `crates/ventus-inlet` — shock trains, Oswatitsch |
| The cycle | `crates/ventus-propulsion` — ideal ramjet |
| Dual-mode / scram track | `crates/ventus-scram` — stub; refuses until stations exist (ADR-003) |
| Boundary layer | `crates/ventus-aero` — reference-temperature method |
| Heating and materials | `crates/ventus-thermal` — radiation equilibrium |
| Every number, with sources | `docs/design-point.md` |
| Proposed M 4 row (not a close) | `docs/design-point-m4.md` |
| Why the code is arranged this way | `docs/adr/` |
| What is still open | search for `[TO ` across the repo |
