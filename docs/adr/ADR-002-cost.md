# ADR-002 — Cost modelling: DAPCA IV, and what it cannot see

**Status:** accepted · **Date:** 2026-09-12 · **Author:** A. Rodríguez (QuantumDrizzy)
**Revision:** r1

## Context

M1 through M10 each have an external yardstick. The cost question — *what does
this aircraft cost?* — has a published methodology, DAPCA IV (RAND, as presented
in Raymer, *Aircraft Design: A Conceptual Approach*), and that methodology was
fitted on aircraft that do not reach this project's flight regime.

Building M11 therefore forced a choice that none of the other modules did: either
refuse the question, or answer it and be precise about the answer's standing.

## D1 — Build it, and make the extrapolation part of the return type

**Rejected: refuse.** `gamma_air` refuses below 273 K, which is the right call
there because a caller has an alternative (use γ = 1.4 and say so). A cost model
that refuses has no alternative; the question simply goes unanswered, and the
first person to need a number will compute one in a spreadsheet with no record
at all. A model whose blind spot is known and quantified beats that.

**Accepted.** `Estimate` carries a `validity: Validity` field that is not
optional and is not a warning flag. A caller holds the extrapolation record in
the same struct as the number, so quoting the cost without the caveat takes
deliberate effort rather than mere carelessness. `Validity::Extrapolated` carries
the overshoot per variable, so a report can say *how far* out it is, not merely
*that* it is out.

## D2 — The SR-71 is not a validation. It is a measurement of the extrapolation

This is the load-bearing decision, and it inverts what an anchor usually means.

The envelope is read as roughly 1200 kn — the fastest class of aircraft built in
quantity when DAPCA IV was fitted. **[TO VERIFY]**, and deliberately generous: a
tighter reading makes the extrapolation look worse, so erring wide errs against
this module's own conclusion.

| | velocity | past the envelope |
|---|---|---|
| DAPCA IV fit | ~1200 kn | — |
| SR-71, M 3.2 / 24 km (by M1) | 1852.3 kn | **1.54×** |
| VENTUS-1, M 3.5 / 26 km (by M1) | 2035.1 kn | **1.70×** |

The anchor is already half again past the fit before VENTUS adds anything. But
the *gap between them* is only **10 % in velocity**, and that is the whole
argument for computing the VENTUS number at all: it is one short step beyond a
point where the answer can at least be sanity-checked, rather than a leap into
nothing. It is equally the reason the VENTUS number cannot be believed to better
than whatever factor the anchor itself is wrong by.

## D3 — The absolute dollars are carried as a known limit, deliberately

M11 computes **7.486 × 10⁹ (1986 USD)** airframe-only for a 32-aircraft SR-71
programme at titanium ×1.7, or 2.34 × 10⁸ per aircraft. Nothing in this
repository can say whether that is right.

No primary source for the programme cost has been read, and the figures in
circulation come with unstated dollar-years and unstated scopes — airframe only,
or with engines, or with the tankers and infrastructure the fleet needed. So the
module **deliberately does not emit** a `programme_cost_usd_1986_verified` key.
The case that asks for one fails with a missing key, exactly as
`gamma_at_design_point_freestream` does in M2. The harness reports an absent
number rather than a wrong one.

**A second anchor is missing: the dollar-year.** Everything is 1986 USD because
that is what Raymer's rates are. Escalating to present-day money needs a cited
index, and reciting one would put a fabricated multiplier on every figure here.
Not done.

**Consequence, stated so it cannot be missed: M11's ratios are usable and its
absolute dollars are not.** No report from this project should quote the latter.

## D4 — The engine term is absent, not defaulted

DAPCA IV's engine relation takes a turbine inlet temperature. **A ramjet has no
turbine.** The term is not defaulted to zero and not approximated by a turbojet
standing in for it — it is absent, which makes every total in M11 an
**airframe-only lower bound**.

M4 chose a ramjet on propulsive grounds that had nothing to do with cost. The
consequence lands here: the one component this model could have priced against a
published relation is the one component it cannot price at all.

## D5 — DAPCA IV stays in pounds and knots, inside the module

ADR-000 D7 puts conversions in the presentation layer. This is not a violation of
that rule; it falls outside it.

`convert::m_s_to_kn` exists so a *report* can show an SI quantity in knots. The
knots in DAPCA IV are not a presentation of anything: the coefficient 4.86
carries units of hours per `lb^0.777 kn^0.894`. Re-expressing the model in SI
would mean re-deriving every coefficient against the original fit data, which
nobody in this project has, and a module that silently invented a refit would be
worse than one that converts its inputs at the boundary.

So `ventus-cost` converts internally, with its own defining constants, and its
identifiers still carry their unit (`we_lb`, `v_knots`) — which is the part of D7
doing the actual work.

## Consequences — the two results that came out of it

**The model cannot see what makes M 3.5 expensive.** Same airframe, same
programme: moving from M 3.2 to M 3.5 costs **+7.4 %**; moving from aluminium to
titanium costs **+21.4 %**. The velocity exponents are mild because the fit
sample never contained an aircraft where speed *forced* the material — and at
M 3.5 it does. M5 showed the recovery temperature sits above the Ti-6Al-4V
sustained limit and that the margin belongs to the radiation balance. DAPCA IV
reaches that physics only through a fudge factor a human types in, and prices the
Mach step itself at almost nothing.

**The production run dominates the aeroplane.** VENTUS-1 at ~14 t empty over a
run of six comes out *more expensive per aircraft* than the SR-71 at 30.6 t over
a run of thirty-two. Half the aircraft, more than half again the unit cost. At
these quantities the learning curve — `Q^0.641`, a 78 % curve — outweighs
everything about the vehicle itself.

That second result is the one with a design consequence, and it is not a cost
consequence. **It says the cheapest way to make this aircraft affordable is to
need more of them**, which is a requirements decision, not an engineering one.

## Open

- `[TO VERIFY]` every DAPCA coefficient, exponent and wrap rate, against a
  primary copy of Raymer. Marked once in `dapca.rs` rather than per line.
- `[TO VERIFY]` which terms the material factor multiplies. This module applies
  it to manufacturing hours and material cost only.
- `[TO CITE]` the SR-71 programme cost, dollar-year and scope — the unblock for
  D3.
- `[TO CITE]` SR-71 empty mass and production quantity, shared with M7.
- `[TO COMPUTE]` a VENTUS-1 empty mass. M6b declares a 28 t cruise mass and
  nothing derives an empty mass from it, so every VENTUS figure in
  `examples/cost.rs` is a **sweep, not a result**, and is labelled as one.
