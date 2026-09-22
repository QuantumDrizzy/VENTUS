# M10 firmware -- Nucleo-F411RE

The flight software (`crates/ventus-fsw`) running on a real STM32F411RE
(Cortex-M4F, 100 MHz), answering the host's gate over the ST-Link virtual COM
port. ADR-004 records the decisions; this file records how to run it.

## What it does

One loop, three behaviours: read a byte, close a frame, answer a frame. Every
reply comes from `ventus_fsw::hil` -- the same module the host runs -- and the
air-data replies come from `ventus_fsw::air_data`, the same atmosphere the
digital twin uses. Safety commands tick the same `SafetyKernel` the host tests
(ADR-005); this binary does not grow a private mode table. The gate
(`cargo xtask hil COM7`) sweeps the atmosphere's layer boundaries and the
design points, then a safety walk, and compares every reply BIT FOR BIT
against the host's own computation.

The M4F has no double-precision FPU: every f64 runs in software. That is the
point of the exercise -- software arithmetic is slower than hardware and still
deterministic, and determinism is the property the gate checks.

GPIO abort / arm lines are **not** bound. The host injects dual-sample bits
over the wire. Do not read a board-run PASS as a claim that the pins work.

## Build and flash

```bash
rustup target add thumbv7em-none-eabihf
cargo install probe-rs-tools        # once
cargo build --release               # in this directory
cargo flash --chip STM32F411RETx --release
```

The Nucleo's onboard ST-Link does both jobs over one USB cable: SWD flashing,
and the virtual COM port bridged to USART2 (PA2/PA3, solder bridges SB13/SB14
closed at the factory).

## Run the gate

```bash
cargo xtask hil COM7        # Windows; /dev/ttyACM0 on Linux
```

The gate fails with the exact remedy when something is missing -- no board, a
stale build id, a protocol mismatch. Its verdict for the sweep is bit-exact or
fail: there is no tolerance to tune, because the yardstick is that the ARM
port and the host compute the same number. Without a COM port the command
refuses rather than degrading; host-side loopback of the same commands lives
in `cargo test -p ventus-fsw`.

## Deliberately not here

- No RTOS, no interrupts, no DMA. A polling loop with bounded work is enough
  for a gate whose worst-case timing is uninteresting, and every line of it
  is honest about that.
- No USB device stack. The VCP bridge gives a serial wire protocol without
  the surface area of a USB stack; the gate validates physics, not
  enumeration.
- No allocator. The firmware is `no_std` without `alloc`, like the crate it
  runs.
- No GPIO discrete sampling. Dual-channel abort/arm arrive as payload bits
  until a later bind reads the pins.
