# ADR-003 — Dual-mode ram/scram track for the Mach 5 stretch

**Status:** accepted (research track) · **Date:** 2026-09-21 · **Author:** A. Rodríguez (QuantumDrizzy)
**Revision:** r1
**Amends:** ADR-000 D5 (new crate on the DAG), D12 (the crate declares a validation route)

## Context

VENTUS-1's validated design point is Mach 3.50 at 26 km geopotential. That number
is a *yardstick*, not a ceiling the programme is pretending not to want. The
programme goal is cruise at or above Mach 4, with a stretch/target of Mach 5 —
Lockheed-class air-breathing, SR-72-style as *intent*, not as a copied vehicle.

Those two facts have already been mistaken for each other once in the project's
own notes, and they must not be again:

| What exists | What it is | What it is not |
|---|---|---|
| Design point M 3.50 @ 26 km | the snapshot every module is held to | a quiet Mach 5 aircraft waiting for a rename |
| `ideal_ramjet` refusal ~M 5.65–5.70 | a subsonic-combustion ramjet whose ram total temperature has reached the 1700 K burner limit | aircraft capability, a scramjet ceiling, or a reason to keep extending M4 |
| Capture = body ~M 3.85 | the current Sears-Haack / inlet configuration becoming self-inconsistent (M12) | a number this propulsion track can move by emitting Isp |
| Public ramjet anchors (SR-71, D-21) | why M 3.5 was chosen: both remain checks | a basis for a Mach 5 cycle |
| X-43 / X-51 class | *regime* anchors: air-breathing heat addition with a supersonic core has been flown | copy-paste numbers, trajectories, or engine decks for VENTUS-1 |

Above roughly M 3.35 there is no public vehicle to check a *ramjet* against until
the scramjet demonstrators, and those demonstrators are a different regime.
`docs/design-point.md` §0 already says this. The gap is therefore not "M4 needs
another 1.5 Mach of the same physics". The gap is that subsonic-combustion ramjet
and dual-mode / scramjet are different engines, and this repository only has the
first.

Extending `ideal_ramjet` past its validity to print a Mach 5 specific impulse
would be the same class of error as extrapolating `gamma_air` below 273 K: a
plausible-looking number about the wrong physics, erring in whichever direction
the formula happens to lean. M12 was built specifically so that temptation has a
named refusal instead of a headline.

## Decision

Add a **separate propulsion path**, crate `ventus-scram`, for the dual-mode
ram/scram research track. Do not extend `ventus_propulsion::ideal_ramjet` past
the subsonic-combustion ramjet it actually is.

M4 stays the ramjet. The new crate owns the stations a dual-mode / scramjet
cycle will need — inlet isolator, combustor with a supersonic core, nozzle —
and, until those stations exist as models, **refuses to emit cycle numbers**.

The validated design point is not moved. Capture-area work that Mach 4 still
needs (inlet larger than the body that carries it, wing-mounted nacelles or a
different volume distribution) is named as a geometry problem and is **out of
this ADR's code scope**. It lives in M3/M6b/M12. A scram stub that pretended to
close that balance by inventing thrust would be worse than no stub.

**Rejected: grow `ideal_ramjet` with a Mach switch.** A `if mach > 5.0 { scram }`
inside M4 would smuggle a regime change through the module whose entire published
reason for existing is that ram compression plus a *subsonic* burner is the
right engine at M 3.5. The burner-limit refusal at ~M 5.70 is a property of that
cycle, not a door into another one.

**Rejected: return zeros, or a "preliminary" Isp band, at Mach 5.** A zero is a
number. A recited hydrocarbon-scramjet band would be the M4 Isp `known_limit`
all over again, except this time there is not even a ramjet identity underneath.
The harness already knows how to report a missing key. Use that.

**Rejected: re-baseline the aircraft to Mach 5 in the same change.** The design
point, the US76 row, the inlet recovery, the radiation-equilibrium skin, and the
M12 frontiers all refer to M 3.50 @ 26 km. Silently editing those into Mach 5
would make every existing yardstick a lie.

## What is established, and what is speculation

**Established, in this repository:**

- M 3.50 @ 26 km is the design-point snapshot (`docs/design-point.md`).
- `ideal_ramjet` is a subsonic-combustion ramjet with γ(T) in the burner and
  nozzle. It refuses when ram total temperature reaches the burner limit.
- On the current configuration, required capture area reaches the body
  cross-section near M 3.85. Past that the Sears-Haack body M6b assumed cannot
  host the inlet. That bind is about the airframe, not about M4's burner
  ceiling.
- Lean blowout is a band that already contains the design point; it is not
  modelled in M4, and it is not modelled here either.

**Established, as public *regime* facts, not as VENTUS numbers:**

- Dual-mode ram/scram and scramjet cycles exist as a class of air-breathing
  engine in which the combustor core can remain supersonic.
- Vehicles in the X-43 / X-51 class have flown in that regime. They are the
  reason the stretch is not science fiction. They are not a source of Isp,
  thrust, or geometry for this aircraft, and this ADR does not transcribe any.

**Speculation, and labelled as such:**

- That a VENTUS configuration can cruise at Mach 4, let alone Mach 5. Capture
  geometry is unsolved at Mach 4 on the body that exists.
- Any transition Mach between ram, dual-mode, and scram for *this* vehicle.
  `ventus-scram` does not classify regime from flight Mach. The caller names
  the regime they are asking about; the crate says whether it can answer.
- Any station efficiency, isolator pressure ratio, combustion efficiency, or
  nozzle performance. Those constants would all be `[TO CITE]` on day one of a
  real model, and citing them from memory is how this project produces numbers
  it then has to retract.

## Consequences

- **`ventus-scram` ships as a stub.** Its public API is types, a request, and
  refusals. `solve_cycle` does not return `Ok` until the named stations exist
  as models. There is no specific thrust and no specific impulse field to quote.
- **The envelope must not be asked for scram answers and start inventing them.**
  M12 continues to report *ramjet* and *model* refusals (M4 thermally choked,
  M2 out of range, capture area). Dual-mode / scram questions go to
  `ventus-scram`, which currently refuses. Wiring M12 to call the stub is a
  later, explicit change — not a silent substitution of scram Isp into the
  ramjet columns. Until that wiring exists, a Mach 5 row in M12 is still a
  statement about `ideal_ramjet`, and must be read as one.
- **No fake Isp or thrust at Mach 5.** A case that asks this crate for
  `specific_impulse_s` is expected to fail with a missing key (`known_limit`),
  the same mechanism M11 uses for an uncited programme cost and M2 uses when
  `gamma_air` will not extrapolate.
- **Mach 4 still needs inlet/body geometry work.** Named so it cannot be
  forgotten: closing cruise ≥ M 4 on this configuration is not a combustor-core
  Mach number, it is capture area versus the body that has to carry the inlet.
  Out of this ADR's code scope.
- **ADR-000 D5** gains `ventus-scram -> (none)` for as long as the crate has no
  physics. When stations are modelled they will need `gasdyn` and almost
  certainly `atmos` and a different inlet than M3's subsonic-exit shock train;
  those edges are added then, not now, because declaring them today would be
  describing a crate that does not exist.
- **Validation route is `Cases`.** The yardstick is the decision itself (this
  ADR) and the refusals in code. There is no external engine deck. That is the
  same honesty M12 already uses: "THE SOURCE IS THE CODE". A future real cycle
  must replace those cases with cited numbers, at which point a `known_limit`
  that starts passing goes `STALE` and the annotation is deleted.

## Non-goals

- A manufacturable airframe, a wind-tunnel article, or a flight article.
- Classified Lockheed (or anyone else's) dual-mode / SR-72 data. Public regime
  anchors only. If a number cannot be cited from a public source, it is not
  entered.
- Claiming Mach 5 is flyable, that the stub "reaches M 5", or that M4's model
  refusal at ~M 5.65–5.70 is an aircraft envelope.
- Replacing M4, re-basing the design point, or weakening any existing M12
  frontier.

## Open

- `[TO COMPUTE]` inlet isolator: a compression system that keeps a supersonic
  core and can hold a shock train without the M3 assumption of a subsonic
  combustor entry.
- `[TO COMPUTE]` combustor with a supersonic core, including the dual-mode
  transition as a modelled regime rather than an enum variant.
- `[TO COMPUTE]` nozzle for that flowpath. M4's station-9 expansion is the
  wrong engine.
- `[TO CITE]` any efficiency, recovery, or heating-value figure the cycle will
  need. None are entered now.
- Geometry for Mach 4 cruise (capture versus body) — tracked by M12, not by
  this crate. Sketched as a proposed row, not a close, in
  `docs/design-point-m4.md`. The validated snapshot remains M 3.50 @ 26 km.
