# ADR-005 -- Flight-software safety modes and critical discretes

**Status:** accepted (first cut) · **Date:** 2026-09-22 · **Author:** A. Rodríguez (QuantumDrizzy)
**Revision:** r2 (HIL wire bind; ADR-004 landed)
**Amends:** none. M10's pitch loop (`ventus_fsw::step`) is unchanged in
behaviour. This ADR adds a layer beside it.
**Numbering:** ADR-003 is dual-mode/scram. ADR-004 is the Nucleo HIL wire
protocol (`ventus_fsw::hil`). **This stays ADR-005.** Do not invent a
second ADR-004. r2 binds the kernel to that wire rather than forking a
mode table.

## Context

M10 today is control-loop hygiene: `no_std`, no alloc, forbid unsafe, a
fixed-step pitch PI with q-scheduling, `Health::{Nominal, AirDataInvalid,
SensorFault}`, hold-last-command on bad input, bit-determinism via `libm`,
and bit-for-bit shared atmosphere with the twin. That is necessary. It is
not a safety kernel, not emergency modes, and not a policy for cockpit /
ground discretes.

Thermal is one bottleneck. The other, named here, is that a critical
discrete — abort, emergency, effector arm, BIT reset — can fail *silently*:
a sample arrives, nothing in the types requires it to change the mode, and
the pitch loop keeps commanding surfaces. A research twin that binds those
lines to HIL (ADR-004) cannot discover that class of bug in the aero stack,
because the aero stack does not own the buttons.

This is the first honest software cut of that layer. Not everything is
flying. Aero and propulsion stay where they are.

## Decision

Add a **safety / mode / critical-discrete kernel** inside `ventus-fsw`,
beside `step`, that produces a **gate** on whether the pitch loop may write
a new surface command.

```
discretes + heartbeat + loop Health
            │
            ▼
      SafetyKernel.tick  →  ControlGate { mode, authority }
            │
            ├── SurfaceAuthority::Command  →  step(...)   (Nominal only)
            └── SurfaceAuthority::HoldLast →  hold last command and integral
```

`step` remains the pitch PI. It does not grow abort logic. `gated_step`
is the composition: classify inputs, tick the kernel, then either call
`step` or freeze the actuator-facing state. Illegal composition is a type
(`SurfaceAuthority`), not a comment in `step`.

### Modes

Explicit enum, total transition function [`FlightMode::apply`]:

| Mode | Meaning | Surfaces |
|---|---|---|
| `Startup` | Power-on. No BIT yet. | Hold last (zero at rest) |
| `Bit` | Software power-on test in progress. Not a hardware BIT. | Hold last |
| `Ready` | BIT passed. Effectors not armed. | Hold last |
| `Nominal` | Armed. Pitch loop may command. | **Command** |
| `Degraded` | Watchdog miss band, or loop `Health` not Nominal, while not aborting. | Hold last |
| `Emergency` | Abort or emergency discrete confirmed this cut. Detection state. | Hold last |
| `Safe` | Latched end-state after Emergency, BIT fail, or watchdog trip-safe. | Hold last |

`Emergency` lasts the detection tick; the next tick enters `Safe`. The
durable state is `Safe`. Reset is `ResetBit` from `Safe` only, and only
when the *current samples* of abort, emergency and master-arm are
deasserted. Software abort/emergency latches clear on that reset, then
the machine re-enters `Bit`. There is no path `Safe → Nominal`.

Illegal events **stay**. They do not panic, wrap, or skip ahead. Abort and
emergency are accepted from every mode; `Safe` stays `Safe` (already there).

### Critical discretes

Four. Aerospace-honest for a research twin, not a HUD:

| Discrete | Kind | Channels | Debounce | On disagreement | Fail-safe |
|---|---|---|---|---|---|
| `Abort` | latched in software until ResetBIT | dual-agree | 2 consecutive resolved asserts | **assert** (do not fly a split abort) | deasserted |
| `Emergency` | same latch | dual-agree | 2 consecutive | **assert** | deasserted |
| `MasterArm` | latched; drops after 2 consecutive deasserts | dual-agree | 2 consecutive | **deassert** (refuse-to-arm) | deasserted |
| `ResetBit` | momentary (confirmed pulse, once) | **single** `[TO DETERMINE]` | 2 consecutive on channel A; B ignored | n/a | deasserted |

`MasterArm` is **effector arm**: it is the gate that allows the pitch loop
to command surfaces. It is not a weapons bus. This repository is not
weapons work (README). The name is kept because that is what the panel
label is on the class of aircraft this twin is studying; the type comment
is the claim, not the identifier.

Dual-channel is a **software hook**. It is not redundant voting hardware.
HIL `CMD_SAFETY_TICK` / `CMD_GATED_STEP` carry `DualSample { a, b }` as
the discrete byte (`DiscreteFrame::to_wire`). GPIO lines are not bound
yet; the host injects the bits. The hook is testable without pretending
a second computer exists.

`ResetBit` is explicitly single-channel `[TO DETERMINE]`. Declaring it
dual today would be a redundancy claim this cut has not earned. Channel B
is carried on the frame and ignored, so a later bind does not change the
frame type.

Abort/emergency disagreement asserts. That is the exception to "fail-safe
= deasserted", and it is the one that makes the silent-drop test possible:
a single channel held for the confirm window must leave `Nominal`. Arm
disagreement deasserts, so a split arm cannot enable surfaces.

### Watchdog

A **software frame counter**, not an STM32 IWDG, not a windowed watchdog
peripheral, not a claim that a flight computer is being kicked.

- Heartbeat missing `WATCHDOG_DEGRADED_MISSES` (3) consecutive control
  frames → `Degraded`.
- Missing `WATCHDOG_SAFE_MISSES` (10) → `Safe`.
- A present heartbeat clears the miss counter. **Mode** `Safe` does not
  clear: recovery from `Safe` is ResetBIT, not a restored tick.

Thresholds are `[TO DETERMINE]` pending a real frame budget on hardware.
They are 30 ms and 100 ms only *if* the loop is actually at
`CONTROL_RATE_HZ`. That rate is already a declared, not measured, number
in M10.

### BIT

Software walk: `BIT_PASS_FRAMES` consecutive frames in `Bit` with
`bit_clear`, heartbeat present, and `Health::Nominal`. Timeout
(`BIT_TIMEOUT_FRAMES`) or watchdog trip-safe → `Safe`. This is not
coverage of the airframe, the sensors, or the Nucleo. It is a mode you
cannot skip.

## Why beside `step`, not inside it

`step` is total on *sensor* garbage and holds last command. Mixing abort
latches into that function would couple two different fail-safes: "do not
believe this air-data sample" and "the vehicle is no longer allowed to
fly this loop." They must be able to disagree in tests. Degraded with
good sensors still holds; Nominal with `Health::SensorFault` is rejected
by the kernel *before* `step` runs, so a NaN cannot produce a command
that is then thrown away as an afterthought.

Rejected: a `bool armed` on `LoopState`. A boolean does not have illegal
transitions. An enum with `apply` does.

Rejected: replacing `Health` with flight modes. `Health` is about this
frame's air data. `FlightMode` is about the vehicle. A degraded vehicle
can still have Nominal air data; a Nominal vehicle can have a SensorFault
and must then leave Nominal.

Rejected: a GUI, a fake cockpit, ejector-seat physics, DO-178C evidence,
or three-channel voting. Those are non-claims, listed below.

## Non-claims

- **Not flight certification.** No DAL, no trace matrix, no DO-178C
  evidence package. Tests here catch silent drops and illegal skips in
  *this* state machine. They are not a cert argument.
- **Not a GUI.** No pixels, no HUD, no "buttons" beyond typed samples.
- **Not ejector-seat physics.** Abort is discrete logic. The seat, if any,
  is not in this repository.
- **Not redundant voting hardware.** Dual-channel is a pair of `bool`s and
  a disagreement policy. Binding those bools to GPIO is still later; HIL
  currently injects them on the wire.
- **HIL owns the discrete-byte layout**, and that layout is this packing:
  `DiscreteFrame::{to_wire,from_wire}` is byte 0 of `CMD_SAFETY_TICK` and
  byte 32 of `CMD_GATED_STEP` (ADR-004 D6). Mode/authority/latch tags on
  the reply are `FlightMode::to_wire`, `SurfaceAuthority::to_wire`, and
  the latch flags. Do not fork a second packing.
- **Not propulsion cutoff, fuel isolation, or envelope refusals.** The
  gate is surface-command authority for the pitch loop only.

## Consequences

- `ventus-fsw` grows `safety` and `gated_step`. The DAG does not change:
  still `units`, `atmos`, `dynamics`. No alloc, no `unsafe`, hot path is a
  handful of integer compares per frame.
- Validation route stays `Identities`. There is no external number to cite
  for a research-twin mode table. The table is the yardstick, asserted in
  the crate tests, including a case that fails if an abort assert is
  dropped on the floor.
- HIL (ADR-004 r2) calls `SafetyKernel::tick` / `gated_step` from
  `ventus_fsw::hil::handle`. There is no second mode enum. Confirmed abort
  on the wire leaves Nominal and freezes surfaces; host loopback tests
  pin that without a board.

## Open -- next rungs

- **Bound:** kernel inputs and the gate travel on HIL commands
  `CMD_SAFETY_TICK` (0x04), `CMD_SAFETY_STATE` (0x05), `CMD_GATED_STEP`
  (0x06), protocol version 2. Layout in ADR-004 D6. GPIO is not part of
  that bind.
- **[TO BIND]** Dual-channel GPIO (or equivalent) for Abort / Emergency /
  MasterArm; decide whether `ResetBit` earns a second channel or stays
  `[TO DETERMINE]`.
- **[TO DETERMINE]** Watchdog and BIT frame counts on a real control
  computer, and whether a hardware IWDG sits *under* this software
  counter (it should) or is instead treated as a substitute (it should
  not).
- **[TO COMPUTE]** Surface a Degraded/Safe gate into M12 envelope
  refusals so a sweep cannot report a flyable point the kernel would
  freeze.
- **[TO COMPUTE]** Engine cutoff / fuel-isolation discretes as additional
  latched lines with the same debounce/disagree machinery — not in this
  cut, not faked as MasterArm.
- Recover hysteresis (N good heartbeats before leaving Degraded) —
  currently one restored tick. Chatter under a flickering heartbeat is
  accepted as a known thinness, not as a design.

## Non-goals (this cut)

Hardware BIT, windowed watchdog peripheral, three-channel majority,
crew-alert logic, a cockpit, certification artefacts, weapons, guidance,
or re-writing M10's PI. GPIO discrete sampling is a later bind, not a
non-goal of the kernel -- the kernel already consumes DualSample.
