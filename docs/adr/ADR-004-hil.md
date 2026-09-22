# ADR-004: Hardware-in-the-loop for M10

Date: 2026-09-18
Status: accepted
Revision: r2 (2026-09-22) -- bind ADR-005 SafetyKernel onto this wire

## Context

M10's yardstick is bit-for-bit agreement with M1 -- "the twin and the flight
software share one atmosphere implementation". On the host that is an
assertion between two calls to the same compiled code. The claim ADR-000 D1
actually makes is stronger: the SAME SOURCE, compiled for a different
machine, still computes the same numbers. Only hardware settles that, and a
Nucleo-F411RE (STM32F411RE, Cortex-M4F) is the cheapest hardware that makes
the claim non-trivial: a different instruction set, a different memory map,
and no double-precision FPU -- every f64 runs in software.

ADR-005 added a safety kernel beside the pitch loop. That kernel is useless
on the wire if HIL invents a second mode enum. This revision binds it.

## D1 -- The protocol lives in the no_std crate, not beside it

`ventus_fsw::hil` holds the framing, the CRC, the command set and the
dispatch. The host gate and the firmware both call it. A protocol written
twice is two protocols, and they will diverge -- the same argument ADR-000 D1
makes about the atmosphere, applied to the wire.

The air-data dispatch is PURE (bytes in, bytes out). Safety commands
additionally mutate a [`HilSession`]: a `SafetyKernel` plus the pitch
`LoopState` GATED_STEP writes. That session is still no I/O, no allocation,
no panic -- which is why the host runs the exact code path the device runs,
and its totality is testable without hardware.

## D2 -- Yardstick: bit-exact, no tolerance

The HIL air-data sweep compares `.to_bits()`, not a relative error. This is
the one comparison in the project where bit-exactness is unconditionally
achievable: the work is elementwise arithmetic and two `pow` calls, no
reductions (the honest bound of ADR-000 D3 still stands -- this gate does not
test one). If the ARM port returns one bit different, the gate fails; there
is nothing to tune, which is exactly why it must not be tunable.

Safety replies compare mode, authority, latches, and -- for GATED_STEP --
the held surface command bits against a host-side `gated_step` on the same
inputs.

## D3 -- Transport: USART2 through the ST-Link VCP

A binary frame protocol over the Nucleo's virtual COM port. Rejected:
a USB device stack (large, unsafe-adjacent surface that adds nothing to the
property being validated), and an ASCII protocol (longer frames, no CRC
discipline, invites "human readable" drift between the sides). The frame is
`[7E C5][len][cmd][payload][crc8-SMBus]`, little-endian on the wire. Corrupt
frames are dropped, not answered: the host retransmits, and replying to
garbage doubles the ways to confuse it.

## D4 -- The firmware builds outside the workspace

`firmware/nucleo-f411` is excluded from the workspace: it targets
`thumbv7em-none-eabihf`, which the workspace's host profiles and lints do not
govern, and forcing one toolchain to serve both would loosen both. It depends
on `ventus-fsw` by path, so the physics, the protocol, and the kernel are
still one code.

## D5 -- Build identity travels on the wire

The firmware bakes the first 8 bytes of `git rev-parse HEAD` in at build
time; HELLO reports them; the gate compares against its own HEAD and FAILS
on mismatch with the reflash command in the remedy. The tree's dirty state is
not distinguishable here -- the commit hash alone is carried, and a dirty tree
still makes the run non-reproducible by the same standard as the reports.

## D6 -- Safety kernel on the same frame family

Do not invent a second protocol. New commands are the same `[7E C5]` frames.
`PROTOCOL_VERSION` is **2** because the command set grew (a v1 device fails
HELLO rather than answering `ERR_UNKNOWN_COMMAND` to an abort).

| cmd | name | request | reply |
|---|---|---|---|
| `0x01` | PING | echo payload | same payload |
| `0x02` | HELLO | empty | `[ver=2][platform][build_id; 8]` |
| `0x03` | AIR_DATA | two le f64: altitude_m, tas_m_s | four le f64: alt, mach, q, T |
| `0x04` | SAFETY_TICK | 3 bytes (below) | 6-byte gate snapshot |
| `0x05` | SAFETY_STATE | empty | 6-byte gate snapshot, **no tick** |
| `0x06` | GATED_STEP | 34 bytes (below) | 31-byte loop + gate |

### SAFETY_TICK request (3 bytes)

```
[0] discretes  -- DiscreteFrame::to_wire / from_wire, eight bits:
                 0 abort A, 1 abort B,
                 2 emergency A, 3 emergency B,
                 4 master_arm A, 5 master_arm B,
                 6 reset_bit A, 7 reset_bit B
[1] flags      -- bit 0 heartbeat present, bit 1 bit_clear; other bits ignored
[2] health     -- Health::to_wire: 0 Nominal, 1 AirDataInvalid, 2 SensorFault
```

Unknown health is `ERR_MALFORMED` and does **not** tick. Wrong length is the
same. This packing is the HIL layout; `DiscreteFrame::from_wire` is no longer
a private test helper.

### SAFETY_TICK / SAFETY_STATE reply (6 bytes)

```
[0] mode              -- FlightMode::to_wire (0..6)
[1] authority         -- 0 HoldLast, 1 Command
[2] latches           -- bit 0 abort, bit 1 emergency, bit 2 master_arm
[3] watchdog          -- 0 Ok, 1 Degraded, 2 TripSafe
[4] consecutive_misses
[5] reserved 0
```

SAFETY_STATE is a snapshot. It does not count as a control frame, so a poll
cannot trip the software watchdog.

### GATED_STEP request (34 bytes) / reply (31 bytes)

Request: four le f64 (altitude, tas, commanded pitch, measured pitch), then
the discrete byte and the flags byte (heartbeat, bit_clear). Loop health is
**derived** by `gated_step` / `input_health`, not injected -- the composition
the vehicle actually runs.

Reply: last_command_rad, integral, steps (le u64), health, then the same
6-byte gate snapshot as SAFETY_TICK (31 bytes total).

Confirmed abort on this command must leave Nominal and freeze
`last_command_rad` bit-for-bit. That is the same load-bearing property as
`abort_holds_the_command_that_was_flying` in the crate tests, now on the
wire. Host loopback proves it without a board.

`MasterArm` remains effector arm, not a weapons bus.

## Consequences

- `cargo xtask hil <COM>` is a gate: PASS is bit-exact over the whole
  deterministic air-data sweep (layer bases, design points, LCG
  pseudo-random points, out-of-domain error codes) **and** a safety walk
  (Startup -> Ready -> Nominal -> confirmed abort -> Emergency/Safe with
  HoldLast, plus a GATED_STEP freeze) or the command fails.
- Without a COM port the gate refuses with the remedy. CI proves the new
  commands with host-side loopback in `ventus-fsw` tests, not by claiming a
  board-run PASS.
- The gate's `available: validate | bench | hil` line in xtask is the
  declaration of what a run can prove.
- Adding commands to the protocol means adding them to `ventus_fsw::hil`
  first, with their host-side test; the firmware cannot grow a private
  command. Firmware `main` only dispatches `handle_frame` on a `HilSession`.

## What this does NOT claim

Passing the gate does not claim flight-readiness, real-time behaviour, or
that a Nucleo is flight hardware. The air-data sweep claims exactly one
thing: the same atmosphere, bit for bit, on a second instruction set with no
hardware f64.

The safety walk claims the kernel on the device is the kernel the host
tests: abort is not silent, surfaces freeze when the gate says HoldLast.
It does **not** claim:

- GPIO lines are wired (they are not; bits arrive in the payload)
- an STM32 IWDG is kicking
- certification, a cockpit, or weapons

## Open

- **[TO BIND]** Dual-channel GPIO (or equivalent) for Abort / Emergency /
  MasterArm on the Nucleo; ResetBit is still single-channel `[TO DETERMINE]`.
- **[TO VERIFY]** Board-run PASS of `cargo xtask hil <COM>` on a flashed
  Nucleo-F411RE. Host loopback is not that.
