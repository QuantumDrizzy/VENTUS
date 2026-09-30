# ADR-009 — Mach 4.00 is the baseline, and the Isp has a source

**Status:** Accepted (2026-10-01)
**Deciders:** Antonio (QuantumDrizzy)
**Amends:** ADR-008 (its decision 2, the range basis; its open Isp item). **Supersedes** nothing.

## Context

ADR-008 closed sustained Mach 4.00 in code and cases, on the ADR-006 candidate combustor, and left
two things to the owner: whether to re-baseline the snapshot onto the candidate, and which Isp is
right. Neither Isp in the repository had a source: 1450 s was the midpoint of a band recited from
memory (`c04da1d`), and 1966.8 s is the chain's own cycle at the M 3.50 snapshot, with efficiency
factors taken from a turbojet text. ADR-008 amendment 1 then found that the forward flat panels at
M 4.00 are above the Ti-6Al-4V limit.

The owner decided (2026-10-01): **re-baseline**, and **settle the Isp against a primary source**.

## Source read

Evans, P. J., Jr. *Analytical investigation of ram-jet-engine performance in flight Mach number range
from 3 to 7.* NACA RM E51H02, 1951 (NTRS 19930086727), read directly. Hydrocarbon fuel (H/C 0.168),
combustion efficiency 100 %, two diffusers. Read off its charts at flight Mach 4:

| | value | where |
|---|---|---|
| high-efficiency diffuser, total-pressure ratio | **0.45** | Fig. 2 |
| engine efficiency / combustion efficiency, maximum | **0.455** (+-0.005, chart grid) | Fig. 11 |
| same, at maximum thrust | 0.375 | Fig. 11 |

## Decision

1. **Mach 4.00 is the baseline.** The official configuration is the M 4.00 design-q row (27 747 m)
   with the ADR-006 cooled-liner candidate combustor. The M 3.50 snapshot stays in the repository,
   unchanged, as the yardstick every pinned M 3.50 number was computed on — a baseline moves, history
   does not.
2. **The Isp is the cycle brought to NACA RM E51H02.** The chain's cycle, run at Evans's own
   conditions (M 4, recovery 0.45, candidate combustor), gives 0.481 per unit of combustion
   efficiency against Evans's 0.455: **the cycle is 5.3 % optimistic** (calibration 0.94695), as an
   internal-thrust, ideal-nozzle model against a propulsive efficiency should be. The cycle is not
   refuted; it is brought down by that factor. At the row with the vehicle's own inlet the cycle
   gives 1869.3 s, so **the M 4.00 Isp is 1770 s** (1750-1790 s over the chart-reading uncertainty).
   `naca_e51h02_calibration` and `calibrated_m4_specific_impulse_s` compute it; the test
   `the_isp_is_settled_against_naca_rm_e51h02` replaces the one that pinned the disagreement.
3. **The range is claimed on the chain's L/D and the settled Isp: ~4970-6660 km**, reserve-limited
   (11.3 % and 500 kg). The published basis (1450 s, L/D 5.5 carried) and the raw model basis
   (1869 s) stay in the test as history, not as claims.
4. **The flat panels are two alloys** (ADR-008 amendment 1): Ti-6Al-4V aft of ~2.4 m from each
   leading edge, Ti-6242S forward of it.
5. **No L/D target is claimed for M 4.00.** 4.58 is what the geometry gives, under Küchemann's 7.00;
   the M 3.50 band of 5.0-6.0 is not restated, because a target set after the result is not a target.

## What can be said, exactly

> **VENTUS-1's baseline is sustained Mach 4.00**, closed in code and cases on its cooled-liner
> combustor at 27.7 km on the design-q row: the flame holds (φ 0.586, above the cited 0.50 V-gutter
> floor), the inlet fits the body (0.793), the constant-q climb is open from M 1.6, the four-ramp
> inlet clears MIL-E-5008B, the nose and leading edges survive in Inconel 718 (~876 K), the forward
> flat panels in Ti-6242S and the rest in Ti-6Al-4V, cruise L/D is 4.58, the ramjet Isp is 1770 s
> against NACA RM E51H02, and the cruise range is ~4970-6660 km, reserve-limited.

What it may **not** say: that it flies; that its flame stability is verified for a ramjet without a
declared flame holder; that the booster phase below M 1.6, or the crew, canopy and life support,
are designed.

## Still open, and why they do not reopen the baseline

- **Lean blowout, ramjet without a holder.** φ 0.586 clears Useller's cited V-gutter afterburner floor
  (0.50); closing it for a no-holder ramjet needs either a chart that does not exist in the corpus or
  a declared flame holder. Declaring one is a design decision left to the owner, because the corpus
  explicitly refuses a holder invented to save a case. At M 4.00 nothing needs saving.
- Liner cooling fraction bounded (<= ~24 %), not cited. Booster below M 1.6 not modelled. Crew,
  canopy, life support `[TO DETERMINE]`. M12 rows for M 4.00 not in the corpus.
- Evans's figure is a 1951 analysis with 100 % combustion and an idealised diffuser; it is a primary
  source for the *shape* of the efficiency, not a flight measurement. A flight engine deck would
  supersede it.

These bound what the sentence claims; none of them is a claim the sentence makes.
