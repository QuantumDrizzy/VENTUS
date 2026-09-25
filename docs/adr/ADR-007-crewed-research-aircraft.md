# ADR-007 -- VENTUS-1 is a crewed research aircraft

**Status:** accepted · **Date:** 2026-09-25 · **Author:** A. Rodríguez (QuantumDrizzy)
**Amends:** none in code. It fixes a question the flight software (M10, ADR-004, ADR-005) and
the mass budget (M7) had been leaving open, and it states what follows from the answer.

## Context

The owner asked the question directly: piloted, or autonomous? It decides the control
architecture, so it has to be answered before the flight software grows further.

The anchors split along the question. The SR-71, which is crewed, is the M 3.2 anchor that
M5, M7 and M11 close against. The D-21, uncrewed, is used only as a ramjet and titanium
regime check. The X-43 and X-51, which ADR-003 names as regime anchors for the Mach 5 track,
are uncrewed scramjet demonstrators. So the *cruise-aircraft* anchors are crewed and the
*propulsion-regime* anchors are not.

## Decision

**VENTUS-1 is a crewed research aircraft with fly-by-wire and envelope protection.** The
pilot commands; the safety kernel (ADR-005) bounds what those commands can do; the pitch
loop (M10) executes them.

It is **not autonomous**, for two reasons that hold independently:

1. **Scope.** An autonomous vehicle at this speed and range needs guidance and navigation,
   which "What this is not" excludes by name ("No armament, no targeting, no guidance, no
   countermeasures"). A crewed research aircraft needs none of it.
2. **Anchors.** The numbers M7 closes against, zero-fuel fraction and range, come from the
   SR-71, and they include a crew. An uncrewed design would have to re-derive them without an
   anchor. The uncrewed X-43 and X-51 anchor a propulsion regime, not an aircraft's mass.

It is **not a combat aircraft.** "Caza" (fighter) was the working word. It does not
describe this vehicle, and the repository does not use it.

## What follows, and where each item lives

| Consequence | Module | State |
|---|---|---|
| Envelope protection: q, Mach and temperature limits the pilot cannot command through | M10 / ADR-005 safety kernel | **open**, next FSW work |
| Pilot-in-the-loop timing: the loop runs faster than a human, so a stick input is a *demand*, not a surface position | M10 | the existing PI already takes a demand; to be stated in its API |
| Cockpit, pressure suit and life support mass | M7 | `[TO DETERMINE]`; inside the SR-71-anchored zero-fuel fraction today, not budgeted separately |
| Cockpit thermal: canopy and cabin at M 3.5–4 recovery temperature | M5 | `[TO DETERMINE]`; M5 evaluates a skin panel and the nose / leading edges, not a canopy |
| Crew escape envelope | — | out of scope for a model; recorded so it is not assumed |

## Consequences

- The FSW backlog gains one item ahead of the others: **envelope protection** in the safety
  kernel. The kernel already makes abort and emergency discretes change the mode (ADR-005).
  Protection is its continuous counterpart: a limit the command cannot pass.
- No number moves. The mass and thermal items are recorded as `[TO DETERMINE]` rather than
  estimated, because an estimated crew mass would carry a range change this repository could
  not defend.
