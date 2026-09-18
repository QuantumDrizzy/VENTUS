//! M10 — flight software on the Nucleo-F411RE (STM32F411RE, Cortex-M4F).
//!
//! The main loop is exactly three behaviours, per ADR-003: read one byte,
//! attempt to close a frame, answer a closed frame. All of the logic lives in
//! `ventus_fsw::hil`, the same module the host's gate runs, so there is no
//! second implementation to drift. The air-data replies come from
//! `ventus_fsw::air_data` — the same atmosphere the digital twin uses — which
//! is the whole point of the exercise: US76 computed on a 100 MHz Cortex-M4F
//! that must agree with the host BIT FOR BIT, software f64 and all (the M4F
//! has no double-precision FPU; the arithmetic runs in software, which is
//! slower and still deterministic — the two properties that matter here).

#![no_std]
#![no_main]

use cortex_m_rt::entry;
// `panic-halt` is a dependency, but a dependency nothing names is not linked,
// and the binary then has no `#[panic_handler]`. This import is the link.
use panic_halt as _;
use stm32f4xx_hal::{
    pac,
    prelude::*,
    serial::{config::Config, Serial},
};

use ventus_fsw::hil;

/// The Nucleo's USART2 on PA2/PA3 is bridged to the ST-Link's virtual COM port
/// (solder bridges SB13/SB14 closed at the factory): one USB cable carries both
/// the SWD flashing link and this protocol.
const BAUDRATE: u32 = 115_200;

/// The first sync byte. The frame must start here; anything else before a
/// frame is dropped one byte at a time.
const SYNC0: u8 = 0x7E;

#[entry]
fn main() -> ! {
    let dp = pac::Peripherals::take().unwrap();
    let rcc = dp.RCC.constrain();
    let clocks = rcc.cfgr.sysclk(100.MHz()).freeze();
    let gpioa = dp.GPIOA.split();

    let serial = Serial::new(
        dp.USART2,
        (
            gpioa.pa2.into_alternate::<7>(),
            gpioa.pa3.into_alternate::<7>(),
        ),
        Config::default().baudrate(BAUDRATE.bps()),
        &clocks,
    )
    // The only failure is an unrepresentable baud rate, and the baud rate is a
    // constant in this file: if it ever fails the firmware is wrong, not the
    // environment, and halting is the honest response on a board with no way
    // to report it.
    .unwrap();
    let (mut tx, mut rx) = serial.split();

    let env = hil::DeviceEnv {
        platform: hil::PLATFORM_NUCLEO_F411,
        build_id: *include_bytes!(concat!(env!("OUT_DIR"), "/build_id.bin")),
    };

    let mut frame = [0u8; hil::MAX_FRAME];
    let mut reply = [0u8; hil::MAX_FRAME];
    let mut n = 0usize;

    loop {
        // Blocking read: the host initiates every exchange, so the device has
        // nothing better to do than wait.
        let byte = match rx.read() {
            Ok(b) => b,
            Err(_) => continue,
        };

        if n == 0 && byte != SYNC0 {
            continue; // drop bytes before the frame starts
        }
        frame[n] = byte;
        n += 1;

        if n < hil::FRAME_OVERHEAD {
            continue;
        }

        match hil::decode(&frame[..n]) {
            Err(hil::FrameError::Truncated) if n < hil::MAX_FRAME => continue,
            Err(_) => {
                // Bad length, or the buffer filled without closing a frame:
                // drop silently and wait for the host's retransmission.
                n = 0;
            }
            Ok(parsed) => {
                if let Ok(len) = hil::handle_frame(&env, &frame[..parsed.frame_len], &mut reply) {
                    for &b in &reply[..len] {
                        let _ = tx.write(b);
                    }
                }
                n = 0;
            }
        }
    }
}
