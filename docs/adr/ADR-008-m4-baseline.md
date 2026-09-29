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
  heating are `[TO DETERMINE]`; the M 4 corpus lacks its US76, skin and M12 rows.

## Consequences

The README's milestone table carries the M 4.00 column as closed on this basis. Any of the open
items that later fails reopens the column, and this ADR is what gets amended -- not the table.
