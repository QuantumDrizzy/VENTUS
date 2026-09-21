# ADR-004: Hardware-in-the-loop for M10

Date: 2026-09-18
Status: accepted

## Context

M10's yardstick is bit-for-bit agreement with M1 — "the twin and the flight
software share one atmosphere implementation". On the host that is an
assertion between two calls to the same compiled code. The claim ADR-000 D1
actually makes is stronger: the SAME SOURCE, compiled for a different
machine, still computes the same numbers. Only hardware settles that, and a
Nucleo-F411RE (STM32F411RE, Cortex-M4F) is the cheapest hardware that makes
the claim non-trivial: a different instruction set, a different memory map,
and no double-precision FPU — every f64 runs in software.

## D1 — The protocol lives in the no_std crate, not beside it

`ventus_fsw::hil` holds the framing, the CRC, the command set and the
dispatch. The host gate and the firmware both call it. A protocol written
twice is two protocols, and they will diverge — the same argument ADR-000 D1
makes about the atmosphere, applied to the wire. The dispatch is PURE
(bytes in, bytes out), so the host runs the exact code path the device runs,
and its totality is testable without hardware.

## D2 — Yardstick: bit-exact, no tolerance

The HIL sweep compares `.to_bits()`, not a relative error. This is the one
comparison in the project where bit-exactness is unconditionally achievable:
the work is elementwise arithmetic and two `pow` calls, no reductions (the
honest bound of ADR-000 D3 still stands — this gate does not test one). If
the ARM port returns one bit different, the gate fails; there is nothing to
tune, which is exactly why it must not be tunable.

## D3 — Transport: USART2 through the ST-Link VCP

A binary frame protocol over the Nucleo's virtual COM port. Rejected:
a USB device stack (large, unsafe-adjacent surface that adds nothing to the
property being validated), and an ASCII protocol (longer frames, no CRC
discipline, invites "human readable" drift between the sides). The frame is
`[7E C5][len][cmd][payload][crc8-SMBus]`, little-endian on the wire. Corrupt
frames are dropped, not answered: the host retransmits, and replying to
garbage doubles the ways to confuse it.

## D4 — The firmware builds outside the workspace

`firmware/nucleo-f411` is excluded from the workspace: it targets
`thumbv7em-none-eabihf`, which the workspace's host profiles and lints do not
govern, and forcing one toolchain to serve both would loosen both. It depends
on `ventus-fsw` by path, so the physics and the protocol are still one code.

## D5 — Build identity travels on the wire

The firmware bakes the first 8 bytes of `git rev-parse HEAD` in at build
time; HELLO reports them; the gate compares against its own HEAD and FAILS
on mismatch with the reflash command in the remedy. The tree's dirty state is
not distinguishable here — the commit hash alone is carried, and a dirty tree
still makes the run non-reproducible by the same standard as the reports.

## Consequences

- `cargo xtask hil <COM>` is a gate: PASS is bit-exact over the whole
  deterministic sweep (layer bases, design points, LCG pseudo-random points,
  out-of-domain error codes) or the command fails.
- The gate's `available: validate | bench | hil` line in xtask is the
  declaration of what a run can prove.
- Adding commands to the protocol means adding them to `ventus_fsw::hil`
  first, with their host-side test; the firmware cannot grow a private
  command.

## What this does NOT claim

Passing the gate does not claim flight-readiness, real-time behaviour, or
that a Nucleo is flight hardware. It claims exactly one thing: the same
atmosphere, bit for bit, on a second instruction set with no hardware f64.
That is one rung of a ladder, and it is the rung that makes every later rung
(HIL with the dynamics loop, control surfaces, flight) share its foundation.
