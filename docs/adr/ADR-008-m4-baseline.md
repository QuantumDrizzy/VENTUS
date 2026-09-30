# ADR-008 -- VENTUS-1's Mach 4.00 baseline, and what "closed" means for it

**Status:** accepted · **Date:** 2026-09-29 · **Author:** A. Rodríguez (QuantumDrizzy)
**Amends:** the milestone table in the README. **Does not** change a pinned M 3.50 number.

## Context

The owner's ladder is M 3.50 real, then M 4.00 real; Mach 5 is VENTUS-2 on the ADR-003 track.
"Real" in this repository has always meant *closed in code and cases, with sources* -- never a
flown aircraft. Three decisions stood between the M 4.00 column and a close, and all three were
recorded as the owner's:

1. whether to re-baseline onto the ADR-006 cooled-liner candidate combustor;
2. which specific impulse the range is claimed on, after `the_two_isp_figures_in_the_chain_disagree`
   found the published 1450 s and the chain's own 1967 s (M 3.50, snapshot) side by side;
3. the reserve policy.

On 2026-09-29 the owner asked for the close to be finished so the claim is true as stated. This
ADR takes the **conservative** reading of each, and says so.

## Decision

1. **The M 4.00 baseline is the ADR-006 candidate combustor on the snapshot body.** It is the only
   configuration in the repository that holds a flame at M 4.00 (φ 0.586) and hosts the capture
   (0.793). The snapshot is kept, unchanged, as the M 3.50 yardstick: every pinned M 3.50 number,
   test and case stays where it is. This is a re-baseline of the M 4.00 claim, not a rewrite of
   history.
2. **Range is claimed on the published basis: ~4157-5569 km.** That is the basis the M 3.50 range
   was published on (Isp 1450 s, L/D 5.5), carried to M 4.00 by the chain's own ratios, and it is
   the lower of the two. The chain's own Isp gives ~5251-7035 km; that is recorded as upside that
   has not earned a claim.

   **Provenance, found while closing this ADR, and it is weaker than the text above implied.**
   1450 s is not an M4 output. It entered in `c04da1d` as the *midpoint* of a hydrocarbon-ramjet
   band recited from memory, "roughly 900-2000 s [TO CITE]", in the case
   `specific_impulse_in_the_published_ramjet_band`, which the corpus itself calls "the weakest
   anchor in the project" and carries as a known limit. The M7 range tests then reused that
   midpoint with the comment "M4, ramjet at the design point". So **neither Isp is sourced**: 1450
   is a recited midpoint, 1967 is the cycle with two efficiency factors that are `[TO CITE]`. The
   range is claimed on the lower one because it is lower, not because it is better founded, and a
   cited engine deck or primary text is what settles it.
3. **No reserve policy is chosen.** A reserve is a mission requirement, not physics. The range is
   published as the band between the two reserves M 3.50 used (500 kg; the SR-71's own 11.3 %).

## What can be said, exactly

> **VENTUS-1 closes sustained Mach 4.00 in code and cases**, on its ADR-006 candidate combustor, at
> 27.7 km on the design-q row: the flame holds (φ 0.586), the inlet fits the body (0.793), the
> constant-q climb is open from M 1.6 (T/D >= 1), the four-ramp inlet clears MIL-E-5008B, the nose
> and leading edges survive (Inconel 718, ~876 K, ~50 K margin), T₀ is thermally perfect (JANAF),
> cruise L/D is 4.58 (under Küchemann's 7.00) and the cruise range is ~4160-5570 km, reserve-limited.

What it may **not** say: that it flies, that it is verified against a lean-blowout chart, or that its
range is 7000 km.

## Still open, listed so the sentence above is never read without them

- The ramjet-no-holder lean-blowout chart is not read; `lean_blowout_verified` stays unemitted.
- The liner cooling-air fraction is bounded (<= ~24 % at M 4.00), not cited.
- The Isp is unsourced both ways (1450 s is a recited band midpoint, 1967 s the cycle with
  uncited efficiencies); pinned by `the_two_isp_figures_in_the_chain_disagree`, not settled.
- The L/D target for M 4.00 is not restated; 4.58 sits below the M 3.50 band of 5.0-6.0.
- Below M 1.6 a booster is required and not modelled; crew, canopy and life-support mass and
  heating are `[TO DETERMINE]`; the M 4 corpus has its US76 rows (added with this ADR) and lacks
  its skin and M12 rows -- the M 3.50 skin case cited an `analysis/` implementation that was not in
  the repository (written in amendment 1, which also adds the skin rows; M12 rows still missing).

## Consequences

The README's milestone table carries the M 4.00 column as closed on this basis. Any of the open
items that later fails reopens the column, and this ADR is what gets amended -- not the table.

## Amendment 1 -- the flat panels at M 4.00, stated before they are computed (2026-10-01)

The sentence above says the nose and leading edges survive at M 4.00. It says nothing about the flat
radiating panels, and M 3.50's material conclusion -- "radiation is what keeps the flat panels in
conventional titanium" (ventus-thermal) -- was never re-run at M 4.00. The M 3.50 skin case also cites
an independent implementation in `analysis/` that is not in the repository. This amendment writes
that implementation (`analysis/skin_crosscheck.py`, sharing no code with the crates: its own US76,
recovery, Eckert reference temperature, Prandtl-Schlichting friction, Chilton-Colburn analogy and
radiation balance; the cp fit and Sutherland constants are the model's stated inputs) and adds the
M 4.00 skin rows against it.

Hand estimate at the design-q row (27 747 m geopotential, M 4.00, eps 0.85, sink 0 K, turbulent):
T_edge ~224 K, T_aw ~865 K, h ~30 W/m^2/K at 10 m.

* **P-A1 (the cross-check exists).** The new implementation reproduces the three M 3.50 figures the
  corpus pins (548.3 K at 10 m and 543.4 K at the SR-71 condition, 709.3 K adiabatic) to 1e-3.
* **P-A2 (the panels at M 4.00).** The 10 m panel settles **within +-20 K of 623 K**, the Ti-6Al-4V
  limit the corpus uses; the 1 m panel is **above** it. Radiation relief is **~240 K** at 10 m.
  If so, M 4.00's forward flat panels are no longer conventional titanium, and the milestone
  sentence must name what they are.

### Amendment 1 -- results (2026-10-01; the predictions above are unedited)

`analysis/skin_crosscheck.py` exists now and shares no code with the crates. Two M 4.00 cases pin
the module to it at 1e-4; changing Eckert's 0.22 to 0.25 in `ventus-aero` fails six cases, the two
new ones among them. Validation: 125 pass, 0 fail, 11 known limit.

| | predicted | computed | verdict |
|---|---|---|---|
| **P-A1** | reproduces 548.3 K, 543.4 K, 709.3 K to 1e-3 | **548.305 K**, **543.378 K**, **709.290 K** | PASS |
| **P-A2**, 1 m | above 623 K | **638.8 K** (T_aw 865.0 K) | PASS |
| **P-A2**, 10 m | within +-20 K of 623 K | **597.1 K**, 26 K under | **FAIL** |
| **P-A2**, relief at 10 m | ~240 K | **267.9 K** | off by 12 % |

What it says:

* **The first 2.41 m behind every leading edge is above the Ti-6Al-4V limit at M 4.00**; aft of that
  the panel radiates down to titanium-capable temperatures (597 K at 10 m, 577 K at 30 m). At M 3.50
  the whole flat panel was Ti-6Al-4V; at M 4.00 it is not.
* **By the module's own candidate list the forward panels become Ti-6242S** (limit 813 K, density
  4540 against 4430 kg/m^3, +2.5 %), with ~174 K of margin at 1 m. Both limits are `[TO CITE]`, as the
  list already says.
* The milestone sentence therefore gains a clause: *the flat panels are Ti-6Al-4V aft of ~2.4 m from
  each leading edge and Ti-6242S forward of it*. The M 4.00 column stays closed -- the material exists
  on the corpus's list, at almost no mass -- but the airframe is no longer a single alloy.
* Scope, unchanged from M 3.50: a flat plate, no chines, nacelles or shock impingement, local
  radiative equilibrium with no conduction, sink at 0 K (the optimistic bound for a lower surface).

## Amended by ADR-009 (2026-10-01)

The owner re-baselined onto the candidate (M 4.00 is the baseline) and had the Isp settled against
NACA RM E51H02: 1770 s, range ~4970-6660 km. Decision 2 above (the published basis) is history from
here; the claim is ADR-009's.
