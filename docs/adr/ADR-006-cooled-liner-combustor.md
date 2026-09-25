# ADR-006 -- A cooled-liner candidate combustor, and air properties above 1800 K

**Status:** accepted (candidate, not a re-baseline) · **Date:** 2026-09-25 · **Author:** A. Rodríguez (QuantumDrizzy)
**Amends:** none. The M 3.50 snapshot, its 1700 K burner cap, the cubic gas model and every
pinned number are unchanged. This ADR adds a second combustor beside the snapshot's, the way
`GeometrySpec::M4_CANDIDATE` added a second body.

## Context

The snapshot does not hold a flame. The cycle burns at φ = 0.4615 at M 3.50, below the
operative lean-blowout bound of 0.50 (Useller Fig. 8), and φ falls with Mach: 0.3795 at the
proposed M 4.00 row. M12 records it, and `design-point-m4.md` §6 lists it as must-have 3.

**Why the cycle is lean.** φ is not chosen. `ideal_ramjet` runs at the burner cap, and the
fuel it can burn is set by the heating room `T4 − T02`. At M 3.50 ram compression already
delivers ~753 K, and the cap is 1700 K, so the room buys `f/a = 0.0314`. Faster flight means
hotter entry air and less room, which is why φ falls with Mach.

**The cap has no source.** `BURNER_EXIT_LIMIT_K = 1700.0` carries no citation in the code, the
design-point record or any ADR. 1700 K is the order of a *turbine-inlet* limit, and a ramjet
has no turbine. Its burner exit is bounded by liner and nozzle cooling and by chemistry.
Ramjet combustors of the period were run far richer: NACA RM E55G28 (Cervenka & Friedman,
*Ram-Jet Performance*) plots combustion efficiency over equivalence ratios of 0.7 to 1.3
(Figs. 145, 147) and states that "over-all fuel-air ratios near stoichiometric are employed".

**The gas model could not answer above 1800 K.** The cubic cp correlation is fitted from
273 K to 1800 K and refuses above it, correctly. A hotter burner therefore returned nothing.

## Decision

1. **Air properties from primary tables, 300–3000 K.** `ventus_gasdyn::janaf` mixes the
   NIST-JANAF Cp rows (4th ed., Chase 1998) of N2 (N-023), O2 (O-029), Ar (Ar-001) and CO2
   (C-095), copied digit for digit at 100 K nodes. They are mixed by the US76 mole fractions
   and interpolated linearly. It stops at 3000 K because the mixture is frozen and
   dissociation is not modelled.
   - The composition is recited `[TO VERIFY]`. It is checked against the US76 molar mass
     already cited in `ventus-units`, which it reproduces to 8.7e-7.
   - Over 300–1800 K the cubic and JANAF disagree by at most **0.768 % at 500 K**. That is
     measured and pinned, and it answers the cubic's own `[TO VERIFY]` against a primary
     table. At 1700 K both models give the same cycle to 0.45 % in φ.

2. **A candidate combustor, not a new snapshot.** `Combustor { exit_limit_k, gas }`. The
   snapshot is `SNAPSHOT_COMBUSTOR = {1700 K, Cubic}`; `evaluate(mach)` is unchanged and
   delegates to `evaluate_with_combustor`. The candidate is
   `COOLED_LINER_CANDIDATE = {2100 K, Janaf}`.

3. **The 2100 K is declared, below a cited demonstration.** NASA TM-78874 (Wear, Trout, Smith &
   Jones, 1978), a semitranspiration-cooled Lamilloy liner: "Tests conducted at combustor
   exit temperatures in excess of 2200 K have not indicated any cooling or durability
   problems." The candidate sits 100 K below that, enforced by `const` assertions.
   - **Class mismatch, stated as for Useller:** that liner is a gas-turbine combustor tested
     at up to 8 atm. VENTUS burner entry is ~1.2 atm total. Lower pressure lowers the
     convective load, which is the favourable direction, but the cooling air or fuel the liner
     needs is **not budgeted** by this model.

## What it buys (pinned in `ventus-envelope` tests)

Sweep of the candidate gas model, snapshot body unless noted:

| burner exit | M 3.50 φ | M 4.00 φ | M 4.00 capture/body | M 4.25 φ | M 4.25 capture/body | Isp at M 3.50 |
|---|---:|---:|---:|---:|---:|---:|
| 1700 K (snapshot, cubic) | 0.4615 | 0.3795 | 1.146 | 0.333 | 1.446 | 1967 s |
| 1800 K | 0.511 | 0.428 | 1.030 | 0.381 | 1.282 | 1936 s |
| 2000 K | 0.617 | 0.533 | 0.858 | 0.485 | 1.046 | 1870 s |
| **2100 K (candidate)** | **0.670** | **0.586** | **0.793** | **0.538** | **0.960** | **1840 s** |
| 2200 K (demonstrated) | 0.724 | 0.639 | 0.739 | 0.591 | 0.888 | 1812 s |

- **M 3.50:** φ = 0.670, above the operative 0.50 and above Useller's 3-foot 0.63.
- **M 4.00:** φ = 0.586, 17 % above the operative bound. **The snapshot body now hosts the inlet**
  (0.793), with no candidate body needed. The M 3.85 capture bind moves out because a hotter
  burner needs less air for the same drag.
- **M 4.25:** inside both bounds, with little room (φ 0.538, capture 0.96).

## What it costs

- **Specific impulse −6.5 % at M 3.50** (1840 s vs 1967 s). Breguet range is linear in Isp at
  fixed V, L/D and mass fractions, so the M 3.50 range moves from 4265–5714 km to about
  **3990–5345 km**. It is still reserve-limited.
- **Cooling that is not in the cycle, but is bounded.** The liner and the nozzle both see
  2100 K, and the cooling flow's cited size is `[TO DETERMINE]`. What is pinned is how much
  the close can pay for. If a fraction `x` of the air bypasses the flame and rejoins before
  the nozzle, mixed by enthalpy on JANAF air, the flame-zone φ is unchanged (the core still
  burns to 2100 K) while the nozzle runs cooler:

  | cooling air x | M 3.50 capture/body | M 4.00 capture/body (snapshot body) |
  |---:|---:|---:|
  | 0 % | 0.549 | 0.793 |
  | 20 % | 0.659 | **0.956** |
  | 25 % | 0.695 | **1.009** — does not host |
  | 30 % | **0.735** | 1.070 |

  M 3.50 closes with 30 % cooling air, and M 4.00 on the snapshot body up to about 24 %.
  Cooling-air pressure loss is not charged. Pinned as
  `m350_closes_with_thirty_percent_cooling_air` and
  `m400_on_the_snapshot_body_tolerates_between_twenty_and_twenty_five_percent_cooling_air`.
- **Burner gas is still air.** The same approximation the snapshot makes, carried, not fixed.

## What it does not change

- `lean_blowout_verified` stays unemitted, and `PHI_LBO_RAMJET_NO_HOLDER_CITED` stays false.
  The candidate clears every **cited afterburner** floor, but a ramjet-no-holder chart has
  still not been read. `FLAME_HOLDER_DECLARED` stays false.
- The snapshot is not re-baselined. That needs the §6 must-haves closed at the new row and a
  new corpus, in the same change that retires this candidate status.
- Mach 5 is not reached by this. A subsonic-combustion ramjet at 2100 K still loses its heating
  room as ram temperature climbs (M 4.50 φ 0.487, capture 1.169). The M 5 stretch stays the
  dual-mode track (ADR-003).
