//! HIL -- the wire protocol between the flight software and the digital twin.
//!
//! The hardware-in-the-loop gate runs this crate's air-data path on a real
//! microcontroller (Nucleo-F411RE, ADR-004) and compares it BIT FOR BIT against
//! the host. The protocol lives here, in the same `no_std` crate the firmware
//! links, because a protocol written twice is two protocols: the host and the
//! device would drift exactly the way ADR-000 D1 says the atmosphere would if
//! it were written twice.
//!
//! Safety commands (ADR-005) ride the same frames. [`handle`] ticks a
//! [`crate::SafetyKernel`] held in [`HilSession`]; it does not invent a second
//! mode table. Confirmed abort on the wire must leave Nominal and freeze
//! surfaces.
//!
//! # The frame
//!
//! ```text
//! [0x7E 0xC5] [len: u8] [cmd: u8] [payload: len bytes] [crc8: u8]
//! ```
//!
//! `len` is the payload length; the CRC-8 (SMBus, polynomial 0x07) covers cmd
//! plus payload. Multi-byte numbers are little-endian on the wire, so the
//! encoding never depends on the device's endianness.
//!
//! # Totality
//!
//! [`handle_frame`] is total: any byte sequence in, one frame out (or a silent
//! drop on bad CRC), nothing panics, nothing allocates. A corrupt frame is
//! reported to the caller, not answered -- the host's timeout retransmits. The
//! device main loop therefore has exactly three behaviours: read, handle, write.

use crate::safety::{
    ControlGate, DiscreteFrame, FlightMode, Heartbeat, SafetyInputs, SafetyKernel,
    SurfaceAuthority, WatchdogLevel,
};
use crate::{ControlFrame, Controller, Health, LoopState};

/// Wire protocol version. Bumped on any breaking change to the framing or the
/// command set; a device that answers a different version fails the gate.
///
/// Version 2 adds SAFETY_TICK / SAFETY_STATE / GATED_STEP (ADR-004 D6). A v1
/// device fails HELLO rather than answering `ERR_UNKNOWN_COMMAND` to abort.
pub const PROTOCOL_VERSION: u8 = 2;

const _: () = assert!(PROTOCOL_VERSION == 2);

/// Maximum payload a frame can carry [bytes].
pub const MAX_PAYLOAD: usize = 56;
/// Sync + len + cmd + crc.
pub const FRAME_OVERHEAD: usize = 5;
/// Maximum wire length of one frame [bytes].
pub const MAX_FRAME: usize = FRAME_OVERHEAD + MAX_PAYLOAD;

pub const CMD_PING: u8 = 0x01;
pub const CMD_HELLO: u8 = 0x02;
pub const CMD_AIR_DATA: u8 = 0x03;
/// Tick [`SafetyKernel`] with dual-channel discretes + heartbeat + health.
pub const CMD_SAFETY_TICK: u8 = 0x04;
/// Snapshot the gate without ticking (a poll must not trip the watchdog).
pub const CMD_SAFETY_STATE: u8 = 0x05;
/// One [`crate::gated_step`]: sensors + discretes in, loop + gate out.
pub const CMD_GATED_STEP: u8 = 0x06;
/// Error replies carry the request command with this bit set.
pub const ERROR_FLAG: u8 = 0x80;

/// Error reply payload codes.
pub const ERR_MALFORMED: u8 = 1;
pub const ERR_BELOW_DATUM: u8 = 2;
pub const ERR_ABOVE_MODEL_TOP: u8 = 3;
pub const ERR_NOT_A_NUMBER: u8 = 4;
pub const ERR_UNKNOWN_COMMAND: u8 = 5;

/// Platform identifiers reported by HELLO.
pub const PLATFORM_HOST_TEST: u8 = 0x00;
pub const PLATFORM_NUCLEO_F411: u8 = 0x01;

/// SAFETY_TICK request length [bytes].
pub const SAFETY_TICK_LEN: usize = 3;
/// SAFETY_TICK / SAFETY_STATE reply length [bytes].
pub const SAFETY_REPLY_LEN: usize = 6;
/// GATED_STEP request length [bytes].
pub const GATED_STEP_REQ_LEN: usize = 34;
/// GATED_STEP reply length [bytes]: loop (24) + health (1) + safety snapshot (6).
pub const GATED_STEP_REPLY_LEN: usize = 31;

/// Request flags byte: heartbeat present this control frame.
pub const FLAG_HEARTBEAT: u8 = 1 << 0;
/// Request flags byte: software-BIT sample clear.
pub const FLAG_BIT_CLEAR: u8 = 1 << 1;
/// Reply latch flags: abort software-latched.
pub const LATCH_ABORT: u8 = 1 << 0;
/// Reply latch flags: emergency software-latched.
pub const LATCH_EMERGENCY: u8 = 1 << 1;
/// Reply latch flags: master-arm (effector arm) software-latched.
pub const LATCH_MASTER_ARM: u8 = 1 << 2;

const SYNC: [u8; 2] = [0x7E, 0xC5];

/// What the device says about itself. The firmware fills this from its build
/// script; the host-side tests use a fixed identity.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DeviceEnv {
    pub platform: u8,
    pub build_id: [u8; 8],
}

/// Device identity plus the safety / pitch session the wire mutates.
///
/// [`DeviceEnv`] stays the Copy identity HELLO reports. The kernel and the
/// pitch loop live here so SAFETY_TICK / GATED_STEP have somewhere to sit that
/// is still `no_std`, no alloc, and total. Firmware `main` holds one of these
/// and only calls [`handle_frame`].
#[derive(Debug, Clone, Copy)]
pub struct HilSession {
    pub env: DeviceEnv,
    kernel: SafetyKernel,
    controller: Controller,
    loop_state: LoopState,
}

impl HilSession {
    #[must_use]
    pub fn new(env: DeviceEnv) -> Self {
        Self {
            env,
            kernel: SafetyKernel::new(),
            controller: Controller::ventus1_pitch(),
            loop_state: LoopState::default(),
        }
    }

    #[must_use]
    pub const fn kernel(self) -> SafetyKernel {
        self.kernel
    }

    #[must_use]
    pub const fn loop_state(self) -> LoopState {
        self.loop_state
    }

    #[must_use]
    pub const fn gate(self) -> ControlGate {
        self.kernel.gate()
    }
}

/// Decoded SAFETY_TICK / SAFETY_STATE reply. Host tests and the xtask gate
/// compare this rather than repeating the byte layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SafetyWireState {
    pub mode: FlightMode,
    pub authority: SurfaceAuthority,
    pub abort_latched: bool,
    pub emergency_latched: bool,
    pub master_arm_latched: bool,
    pub watchdog: WatchdogLevel,
    pub watchdog_misses: u8,
}

impl SafetyWireState {
    #[must_use]
    pub fn from_kernel(kernel: SafetyKernel) -> Self {
        let gate = kernel.gate();
        Self {
            mode: gate.mode,
            authority: gate.authority,
            abort_latched: gate.abort_latched,
            emergency_latched: gate.emergency_latched,
            master_arm_latched: gate.master_arm_latched,
            watchdog: gate.watchdog,
            watchdog_misses: kernel.watchdog().consecutive_misses(),
        }
    }
}

/// Decoded GATED_STEP reply.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GatedStepWire {
    pub last_command_rad: f64,
    pub integral: f64,
    pub steps: u64,
    pub health: Health,
    pub safety: SafetyWireState,
}

/// CRC-8/SMBus (polynomial 0x07, init 0, no reflection, no final xor).
/// `check("123456789") == 0xF4` is asserted in the tests.
#[must_use]
pub const fn crc8(bytes: &[u8]) -> u8 {
    let mut crc: u8 = 0;
    let mut i = 0;
    while i < bytes.len() {
        crc ^= bytes[i];
        let mut bit = 0;
        while bit < 8 {
            crc = if crc & 0x80 != 0 {
                (crc << 1) ^ 0x07
            } else {
                crc << 1
            };
            bit += 1;
        }
        i += 1;
    }
    crc
}

/// Why a byte sequence did not decode into a frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameError {
    /// The buffer does not begin with the sync pair.
    NoSync,
    /// The header promises more bytes than the buffer holds. Keep reading.
    Truncated,
    /// Payload length is impossible for this protocol.
    BadLength,
    /// The CRC does not match. Drop the frame; the host retransmits.
    BadCrc,
    /// `encode` was asked to carry a payload that cannot fit.
    PayloadTooLong,
}

/// One decoded frame, borrowed from the input buffer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Parsed<'a> {
    /// Total wire length, so the caller can advance past it.
    pub frame_len: usize,
    pub cmd: u8,
    pub payload: &'a [u8],
}

/// Write one frame into `out`; returns its wire length.
///
/// # Errors
/// [`FrameError::PayloadTooLong`] if the payload cannot fit; `out` is then
/// untouched.
pub fn encode(cmd: u8, payload: &[u8], out: &mut [u8; MAX_FRAME]) -> Result<usize, FrameError> {
    if payload.len() > MAX_PAYLOAD {
        return Err(FrameError::PayloadTooLong);
    }
    out[0] = SYNC[0];
    out[1] = SYNC[1];
    out[2] = payload.len() as u8;
    out[3] = cmd;
    out[4..4 + payload.len()].copy_from_slice(payload);
    let crc_at = 4 + payload.len();
    out[crc_at] = crc8(&out[3..crc_at]);
    Ok(crc_at + 1)
}

/// Decode the frame at the front of `buf`.
///
/// # Errors
/// [`FrameError`] -- see the variant docs. A [`FrameError::NoSync`] means the
/// caller should drop one byte and retry; [`FrameError::Truncated`] means it
/// should keep reading.
pub fn decode(buf: &[u8]) -> Result<Parsed<'_>, FrameError> {
    if buf.len() < 2 || buf[0] != SYNC[0] || buf[1] != SYNC[1] {
        return Err(FrameError::NoSync);
    }
    if buf.len() < 4 {
        return Err(FrameError::Truncated);
    }
    let len = buf[2] as usize;
    if len > MAX_PAYLOAD {
        return Err(FrameError::BadLength);
    }
    let frame_len = FRAME_OVERHEAD + len;
    if buf.len() < frame_len {
        return Err(FrameError::Truncated);
    }
    let cmd = buf[3];
    let payload = &buf[4..4 + len];
    if crc8(&buf[3..4 + len]) != buf[4 + len] {
        return Err(FrameError::BadCrc);
    }
    Ok(Parsed {
        frame_len,
        cmd,
        payload,
    })
}

fn le_f64(bytes: &[u8]) -> f64 {
    let mut bits = [0u8; 8];
    bits.copy_from_slice(&bytes[..8]);
    f64::from_bits(u64::from_le_bytes(bits))
}

fn put_f64(value: f64, out: &mut [u8], at: usize) {
    out[at..at + 8].copy_from_slice(&value.to_bits().to_le_bytes());
}

fn le_u64(bytes: &[u8]) -> u64 {
    let mut bits = [0u8; 8];
    bits.copy_from_slice(&bytes[..8]);
    u64::from_le_bytes(bits)
}

fn put_u64(value: u64, out: &mut [u8], at: usize) {
    out[at..at + 8].copy_from_slice(&value.to_le_bytes());
}

/// Pack a SAFETY_TICK request. Shared by host tests and `xtask hil`.
#[must_use]
pub fn pack_safety_tick(
    discretes: DiscreteFrame,
    heartbeat: Heartbeat,
    bit_clear: bool,
    loop_health: Health,
) -> [u8; SAFETY_TICK_LEN] {
    let mut flags = 0u8;
    if heartbeat.present {
        flags |= FLAG_HEARTBEAT;
    }
    if bit_clear {
        flags |= FLAG_BIT_CLEAR;
    }
    [discretes.to_wire(), flags, loop_health.to_wire()]
}

/// Pack a GATED_STEP request. Loop health is derived on the device, not sent.
#[must_use]
pub fn pack_gated_step(frame: ControlFrame) -> [u8; GATED_STEP_REQ_LEN] {
    let mut out = [0u8; GATED_STEP_REQ_LEN];
    put_f64(frame.altitude_m, &mut out, 0);
    put_f64(frame.true_airspeed_m_s, &mut out, 8);
    put_f64(frame.commanded_pitch_rad, &mut out, 16);
    put_f64(frame.measured_pitch_rad, &mut out, 24);
    out[32] = frame.discretes.to_wire();
    let mut flags = 0u8;
    if frame.heartbeat.present {
        flags |= FLAG_HEARTBEAT;
    }
    if frame.bit_clear {
        flags |= FLAG_BIT_CLEAR;
    }
    out[33] = flags;
    out
}

fn decode_safety_tick(payload: &[u8]) -> Option<SafetyInputs> {
    if payload.len() != SAFETY_TICK_LEN {
        return None;
    }
    let loop_health = Health::from_wire(payload[2])?;
    Some(SafetyInputs {
        discretes: DiscreteFrame::from_wire(payload[0] as u16),
        heartbeat: if payload[1] & FLAG_HEARTBEAT != 0 {
            Heartbeat::PRESENT
        } else {
            Heartbeat::MISSING
        },
        bit_clear: payload[1] & FLAG_BIT_CLEAR != 0,
        loop_health,
    })
}

fn decode_gated_step(payload: &[u8]) -> Option<ControlFrame> {
    if payload.len() != GATED_STEP_REQ_LEN {
        return None;
    }
    Some(ControlFrame {
        altitude_m: le_f64(&payload[0..8]),
        true_airspeed_m_s: le_f64(&payload[8..16]),
        commanded_pitch_rad: le_f64(&payload[16..24]),
        measured_pitch_rad: le_f64(&payload[24..32]),
        discretes: DiscreteFrame::from_wire(payload[32] as u16),
        heartbeat: if payload[33] & FLAG_HEARTBEAT != 0 {
            Heartbeat::PRESENT
        } else {
            Heartbeat::MISSING
        },
        bit_clear: payload[33] & FLAG_BIT_CLEAR != 0,
    })
}

fn encode_safety_reply(kernel: SafetyKernel, out: &mut [u8; MAX_PAYLOAD]) {
    let state = SafetyWireState::from_kernel(kernel);
    write_safety_reply(state, out);
}

fn write_safety_reply(state: SafetyWireState, out: &mut [u8]) {
    out[0] = state.mode.to_wire();
    out[1] = state.authority.to_wire();
    let mut latches = 0u8;
    if state.abort_latched {
        latches |= LATCH_ABORT;
    }
    if state.emergency_latched {
        latches |= LATCH_EMERGENCY;
    }
    if state.master_arm_latched {
        latches |= LATCH_MASTER_ARM;
    }
    out[2] = latches;
    out[3] = state.watchdog.to_wire();
    out[4] = state.watchdog_misses;
    out[5] = 0;
}

/// Decode a 6-byte safety reply. Unknown tags -> `None` (never a silent Startup).
#[must_use]
pub fn unpack_safety_reply(payload: &[u8]) -> Option<SafetyWireState> {
    if payload.len() != SAFETY_REPLY_LEN {
        return None;
    }
    Some(SafetyWireState {
        mode: FlightMode::from_wire(payload[0])?,
        authority: SurfaceAuthority::from_wire(payload[1])?,
        abort_latched: payload[2] & LATCH_ABORT != 0,
        emergency_latched: payload[2] & LATCH_EMERGENCY != 0,
        master_arm_latched: payload[2] & LATCH_MASTER_ARM != 0,
        watchdog: WatchdogLevel::from_wire(payload[3])?,
        watchdog_misses: payload[4],
    })
}

/// Decode a 30-byte GATED_STEP reply.
#[must_use]
pub fn unpack_gated_step_reply(payload: &[u8]) -> Option<GatedStepWire> {
    if payload.len() != GATED_STEP_REPLY_LEN {
        return None;
    }
    Some(GatedStepWire {
        last_command_rad: le_f64(&payload[0..8]),
        integral: le_f64(&payload[8..16]),
        steps: le_u64(&payload[16..24]),
        health: Health::from_wire(payload[24])?,
        safety: unpack_safety_reply(&payload[25..31])?,
    })
}

/// The outcome of handling one request. The device loop turns this into a
/// reply frame; the host-side loopback test asserts on it directly.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Reply {
    /// A normal reply with this command and payload length in `out`.
    Ok { cmd: u8, len: usize },
    /// An error reply: the request command with [`ERROR_FLAG`] set, payload
    /// `[code]`.
    Error { cmd: u8, code: u8 },
}

fn malformed(cmd: u8, out: &mut [u8; MAX_PAYLOAD]) -> Reply {
    out[0] = ERR_MALFORMED;
    Reply::Error {
        cmd: cmd | ERROR_FLAG,
        code: ERR_MALFORMED,
    }
}

/// Handle one decoded request.
///
/// Air-data commands (PING, HELLO, AIR_DATA) are still bytes-in-bytes-out
/// against [`DeviceEnv`]. Safety commands additionally mutate [`HilSession`]'s
/// kernel (and, for GATED_STEP, the loop state). No I/O, no allocation, no
/// panic -- which is why the host can run the exact dispatch the device will
/// run, and test it without hardware.
///
/// For [`Reply::Error`], the one-byte error payload is left at `out[0]`, so the
/// caller can encode the reply without knowing the code path that produced it.
#[must_use]
pub fn handle(
    session: &mut HilSession,
    cmd: u8,
    payload: &[u8],
    out: &mut [u8; MAX_PAYLOAD],
) -> Reply {
    match cmd {
        CMD_PING => {
            let n = payload.len();
            out[..n].copy_from_slice(payload);
            Reply::Ok {
                cmd: CMD_PING,
                len: n,
            }
        }
        CMD_HELLO => {
            out[0] = PROTOCOL_VERSION;
            out[1] = session.env.platform;
            out[2..10].copy_from_slice(&session.env.build_id);
            Reply::Ok {
                cmd: CMD_HELLO,
                len: 10,
            }
        }
        CMD_AIR_DATA => {
            if payload.len() != 16 {
                return malformed(cmd, out);
            }
            let altitude_m = le_f64(&payload[..8]);
            let true_airspeed_m_s = le_f64(&payload[8..16]);
            match super::air_data(altitude_m, true_airspeed_m_s) {
                Ok(air) => {
                    put_f64(air.altitude_m, out, 0);
                    put_f64(air.mach, out, 8);
                    put_f64(air.dynamic_pressure_pa, out, 16);
                    put_f64(air.static_temperature_k, out, 24);
                    Reply::Ok {
                        cmd: CMD_AIR_DATA,
                        len: 32,
                    }
                }
                Err(e) => {
                    let code = match e {
                        ventus_atmos::AtmosError::BelowDatum => ERR_BELOW_DATUM,
                        ventus_atmos::AtmosError::AboveModelTop => ERR_ABOVE_MODEL_TOP,
                        ventus_atmos::AtmosError::NotANumber => ERR_NOT_A_NUMBER,
                    };
                    out[0] = code;
                    Reply::Error {
                        cmd: cmd | ERROR_FLAG,
                        code,
                    }
                }
            }
        }
        CMD_SAFETY_TICK => {
            let Some(inputs) = decode_safety_tick(payload) else {
                return malformed(cmd, out);
            };
            let _gate = session.kernel.tick(inputs);
            encode_safety_reply(session.kernel, out);
            Reply::Ok {
                cmd: CMD_SAFETY_TICK,
                len: SAFETY_REPLY_LEN,
            }
        }
        CMD_SAFETY_STATE => {
            if !payload.is_empty() {
                return malformed(cmd, out);
            }
            encode_safety_reply(session.kernel, out);
            Reply::Ok {
                cmd: CMD_SAFETY_STATE,
                len: SAFETY_REPLY_LEN,
            }
        }
        CMD_GATED_STEP => {
            let Some(frame) = decode_gated_step(payload) else {
                return malformed(cmd, out);
            };
            let result = super::gated_step(
                &mut session.kernel,
                &session.controller,
                session.loop_state,
                frame,
            );
            session.loop_state = result.loop_state;
            put_f64(result.loop_state.last_command_rad, out, 0);
            put_f64(result.loop_state.integral, out, 8);
            put_u64(result.loop_state.steps, out, 16);
            out[24] = result.loop_state.health.to_wire();
            write_safety_reply(SafetyWireState::from_kernel(session.kernel), &mut out[25..]);
            Reply::Ok {
                cmd: CMD_GATED_STEP,
                len: GATED_STEP_REPLY_LEN,
            }
        }
        _ => {
            out[0] = ERR_UNKNOWN_COMMAND;
            Reply::Error {
                cmd: cmd | ERROR_FLAG,
                code: ERR_UNKNOWN_COMMAND,
            }
        }
    }
}

/// Full round trip for the device main loop: frame in, reply frame out.
/// Returns the reply's wire length, or the frame error to drop silently.
///
/// # Errors
/// [`FrameError`] from `decode`/`encode`. A bad CRC is NOT answered: the host
/// retransmits, and replying to garbage doubles the ways to confuse it.
pub fn handle_frame(
    session: &mut HilSession,
    frame: &[u8],
    out: &mut [u8; MAX_FRAME],
) -> Result<usize, FrameError> {
    let parsed = decode(frame)?;
    let mut payload = [0u8; MAX_PAYLOAD];
    match handle(session, parsed.cmd, parsed.payload, &mut payload) {
        Reply::Ok { cmd, len } => encode(cmd, &payload[..len], out),
        Reply::Error { cmd, .. } => encode(cmd, &payload[..1], out),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gated_step;
    use crate::safety::{DualSample, BIT_PASS_FRAMES, DISCRETE_CONFIRM_FRAMES};

    const TEST_ENV: DeviceEnv = DeviceEnv {
        platform: PLATFORM_HOST_TEST,
        build_id: *b"deadbeef",
    };

    fn test_session() -> HilSession {
        HilSession::new(TEST_ENV)
    }

    fn round_trip(
        session: &mut HilSession,
        cmd: u8,
        payload: &[u8],
    ) -> (u8, [u8; MAX_PAYLOAD], usize) {
        let mut wire = [0u8; MAX_FRAME];
        let n = encode(cmd, payload, &mut wire).unwrap();
        let mut out = [0u8; MAX_FRAME];
        let m = handle_frame(session, &wire[..n], &mut out).unwrap();
        let reply = decode(&out[..m]).unwrap();
        let mut buf = [0u8; MAX_PAYLOAD];
        buf[..reply.payload.len()].copy_from_slice(reply.payload);
        (reply.cmd, buf, reply.payload.len())
    }

    #[test]
    fn crc8_matches_the_smbus_check_value() {
        assert_eq!(crc8(b"123456789"), 0xF4, "CRC-8/SMBus check value");
    }

    #[test]
    fn frames_round_trip() {
        let mut wire = [0u8; MAX_FRAME];
        let mut req = [0u8; 16];
        req[8..].fill(1);
        let n = encode(CMD_AIR_DATA, &req, &mut wire).unwrap();
        let parsed = decode(&wire[..n]).unwrap();
        assert_eq!(parsed.cmd, CMD_AIR_DATA);
        assert_eq!(parsed.payload.len(), 16);
        assert_eq!(parsed.frame_len, n);
    }

    #[test]
    fn a_corrupted_frame_is_rejected_not_answered() {
        let mut session = test_session();
        let mut wire = [0u8; MAX_FRAME];
        let n = encode(CMD_PING, b"ping", &mut wire).unwrap();
        wire[n - 1] ^= 0x55; // flip the crc
        assert_eq!(decode(&wire[..n]), Err(FrameError::BadCrc));
        // And handle_frame refuses it entirely -- the device sends nothing.
        let mut out = [0u8; MAX_FRAME];
        assert_eq!(
            handle_frame(&mut session, &wire[..n], &mut out),
            Err(FrameError::BadCrc)
        );
    }

    #[test]
    fn hello_reports_the_protocol_version_and_platform() {
        let mut session = test_session();
        let (cmd, payload, len) = round_trip(&mut session, CMD_HELLO, &[]);
        assert_eq!(cmd, CMD_HELLO);
        assert_eq!(len, 10);
        assert_eq!(payload[0], PROTOCOL_VERSION);
        assert_eq!(payload[0], 2, "safety bind bumped the command set");
        assert_eq!(payload[1], PLATFORM_HOST_TEST);
        assert_eq!(&payload[2..10], b"deadbeef");
    }

    /// The device's air-data dispatch IS the crate's air-data path: the same
    /// function the twin's gate compares against. Bit-for-bit by construction;
    /// this test pins it so a change to one side of the wire cannot pass alone.
    #[test]
    fn the_air_data_dispatch_is_bit_for_bit_the_air_data_path() {
        let mut session = test_session();
        for altitude in [0.0, 11_000.0, 24_090.96, 26_000.0, 50_000.0, 84_852.0] {
            let mut req = [0u8; 16];
            put_f64(altitude, &mut req, 0);
            put_f64(1046.95, &mut req, 8);
            let (cmd, payload, len) = round_trip(&mut session, CMD_AIR_DATA, &req);
            assert_eq!(cmd, CMD_AIR_DATA);
            assert_eq!(len, 32);

            let expected = super::super::air_data(altitude, 1046.95).unwrap();
            assert_eq!(
                le_f64(&payload[0..8]).to_bits(),
                expected.altitude_m.to_bits()
            );
            assert_eq!(le_f64(&payload[8..16]).to_bits(), expected.mach.to_bits());
            assert_eq!(
                le_f64(&payload[16..24]).to_bits(),
                expected.dynamic_pressure_pa.to_bits()
            );
            assert_eq!(
                le_f64(&payload[24..32]).to_bits(),
                expected.static_temperature_k.to_bits()
            );
        }
    }

    /// TOTALITY ON THE WIRE. Bad altitude values are answered with the error
    /// code the atmosphere model declared, never a panic, never a NaN on the
    /// wire pretending to be data.
    #[test]
    fn out_of_domain_air_data_is_an_error_frame_with_the_declared_code() {
        let mut session = test_session();
        for (altitude, code) in [
            (-100.0, ERR_BELOW_DATUM),
            (200_000.0, ERR_ABOVE_MODEL_TOP),
            (f64::NAN, ERR_NOT_A_NUMBER),
        ] {
            let mut req = [0u8; 16];
            put_f64(altitude, &mut req, 0);
            put_f64(1046.95, &mut req, 8);
            let (cmd, payload, len) = round_trip(&mut session, CMD_AIR_DATA, &req);
            assert_eq!(cmd, CMD_AIR_DATA | ERROR_FLAG);
            assert_eq!(len, 1);
            assert_eq!(payload[0], code);
        }
    }

    #[test]
    fn malformed_and_unknown_requests_get_error_frames() {
        let mut session = test_session();

        let (cmd, payload, _) = round_trip(&mut session, CMD_AIR_DATA, &[0u8; 15]);
        assert_eq!(cmd, CMD_AIR_DATA | ERROR_FLAG);
        assert_eq!(payload[0], ERR_MALFORMED);

        let (cmd, payload, _) = round_trip(&mut session, 0x7F, &[]);
        assert_eq!(cmd, 0x7F | ERROR_FLAG);
        assert_eq!(payload[0], ERR_UNKNOWN_COMMAND);

        let (cmd, payload, _) = round_trip(&mut session, CMD_SAFETY_TICK, &[0u8; 2]);
        assert_eq!(cmd, CMD_SAFETY_TICK | ERROR_FLAG);
        assert_eq!(payload[0], ERR_MALFORMED);

        let (cmd, payload, _) = round_trip(&mut session, CMD_SAFETY_STATE, &[0x01]);
        assert_eq!(cmd, CMD_SAFETY_STATE | ERROR_FLAG);
        assert_eq!(payload[0], ERR_MALFORMED);

        let (cmd, payload, _) = round_trip(&mut session, CMD_GATED_STEP, &[0u8; 33]);
        assert_eq!(cmd, CMD_GATED_STEP | ERROR_FLAG);
        assert_eq!(payload[0], ERR_MALFORMED);

        // Unknown health must not tick and must not become silent Nominal.
        let mut bad_health = pack_safety_tick(
            DiscreteFrame::fail_safe(),
            Heartbeat::PRESENT,
            true,
            Health::Nominal,
        );
        bad_health[2] = 0xFF;
        let before = session.kernel();
        let (cmd, payload, _) = round_trip(&mut session, CMD_SAFETY_TICK, &bad_health);
        assert_eq!(cmd, CMD_SAFETY_TICK | ERROR_FLAG);
        assert_eq!(payload[0], ERR_MALFORMED);
        assert_eq!(session.kernel().mode(), before.mode());
    }

    /// The dispatch must survive arbitrary garbage without panicking: feed it
    /// every single-byte command and a spread of payloads.
    #[test]
    fn the_dispatch_is_total_over_arbitrary_input() {
        let mut session = test_session();
        let mut wire = [0u8; MAX_FRAME];
        let mut out = [0u8; MAX_FRAME];
        let garbage = [0xA5u8; MAX_PAYLOAD];
        for cmd in 0x00..=0xFF {
            for len in [0usize, 1, 15, 16, 17, 3, 34, MAX_PAYLOAD] {
                let n = encode(cmd, &garbage[..len], &mut wire).unwrap();
                // Ok or Error frame out -- either is a defined response.
                let _ = handle_frame(&mut session, &wire[..n], &mut out);
            }
        }
    }

    fn fail_safe_healthy() -> [u8; SAFETY_TICK_LEN] {
        pack_safety_tick(
            DiscreteFrame::fail_safe(),
            Heartbeat::PRESENT,
            true,
            Health::Nominal,
        )
    }

    fn arm_tick() -> [u8; SAFETY_TICK_LEN] {
        pack_safety_tick(
            DiscreteFrame {
                master_arm: DualSample::both(true),
                ..DiscreteFrame::fail_safe()
            },
            Heartbeat::PRESENT,
            true,
            Health::Nominal,
        )
    }

    fn abort_tick() -> [u8; SAFETY_TICK_LEN] {
        pack_safety_tick(
            DiscreteFrame {
                abort: DualSample::both(true),
                master_arm: DualSample::both(true),
                ..DiscreteFrame::fail_safe()
            },
            Heartbeat::PRESENT,
            true,
            Health::Nominal,
        )
    }

    fn safety_reply(session: &mut HilSession, cmd: u8, payload: &[u8]) -> SafetyWireState {
        let (got_cmd, buf, len) = round_trip(session, cmd, payload);
        assert_eq!(got_cmd, cmd, "safety reply must not be an error frame");
        unpack_safety_reply(&buf[..len]).expect("safety reply layout")
    }

    fn drive_wire_to_nominal(session: &mut HilSession) {
        let mut last = safety_reply(session, CMD_SAFETY_STATE, &[]);
        assert_eq!(last.mode, FlightMode::Startup);
        assert_eq!(last.authority, SurfaceAuthority::HoldLast);

        for _ in 0..(BIT_PASS_FRAMES as u16 + 4) {
            last = safety_reply(session, CMD_SAFETY_TICK, &fail_safe_healthy());
            if last.mode == FlightMode::Ready {
                break;
            }
        }
        assert_eq!(last.mode, FlightMode::Ready);
        assert_eq!(last.authority, SurfaceAuthority::HoldLast);

        for _ in 0..(DISCRETE_CONFIRM_FRAMES as u16 + 2) {
            last = safety_reply(session, CMD_SAFETY_TICK, &arm_tick());
            if last.mode == FlightMode::Nominal {
                break;
            }
        }
        assert_eq!(last.mode, FlightMode::Nominal);
        assert_eq!(last.authority, SurfaceAuthority::Command);
    }

    /// SAFETY_TICK is SafetyKernel::tick on the wire, bit for bit on the gate.
    #[test]
    fn safety_tick_matches_the_kernel_bit_for_bit() {
        let mut session = test_session();
        let mut kernel = SafetyKernel::new();
        let healthy = SafetyInputs::healthy();
        for _ in 0..(BIT_PASS_FRAMES as u16 + 4) {
            let packed = pack_safety_tick(
                healthy.discretes,
                healthy.heartbeat,
                healthy.bit_clear,
                healthy.loop_health,
            );
            let on_wire = safety_reply(&mut session, CMD_SAFETY_TICK, &packed);
            let gate = kernel.tick(healthy);
            let expected = SafetyWireState::from_kernel(kernel);
            assert_eq!(on_wire, expected);
            assert_eq!(on_wire.mode, gate.mode);
            if on_wire.mode == FlightMode::Ready {
                break;
            }
        }
        assert_eq!(session.kernel().mode(), FlightMode::Ready);
    }

    /// THE SILENT-DROP TEST, on the wire. Two confirmed dual-assert abort
    /// frames must leave Nominal. If abort handling is deleted from handle,
    /// this fails while the crate tests could still pass.
    #[test]
    fn abort_confirmed_on_the_wire_leaves_nominal() {
        let mut session = test_session();
        drive_wire_to_nominal(&mut session);
        assert!(session.gate().allows_surface_command());

        let g1 = safety_reply(&mut session, CMD_SAFETY_TICK, &abort_tick());
        assert_eq!(
            g1.mode,
            FlightMode::Nominal,
            "debounce: one abort sample must not yet take"
        );
        assert_eq!(g1.authority, SurfaceAuthority::Command);

        // A poll must not count as a confirm frame.
        let snap = safety_reply(&mut session, CMD_SAFETY_STATE, &[]);
        assert_eq!(snap.mode, FlightMode::Nominal);

        let g2 = safety_reply(&mut session, CMD_SAFETY_TICK, &abort_tick());
        assert_ne!(
            g2.mode,
            FlightMode::Nominal,
            "a confirmed abort must leave Nominal -- if this fails, abort is silent on the wire"
        );
        assert_eq!(g2.mode, FlightMode::Emergency);
        assert!(g2.abort_latched);
        assert_eq!(g2.authority, SurfaceAuthority::HoldLast);
        assert!(!g2.master_arm_latched, "abort clears effector arm");

        let g3 = safety_reply(&mut session, CMD_SAFETY_TICK, &abort_tick());
        assert_eq!(g3.mode, FlightMode::Safe);
        assert_eq!(g3.authority, SurfaceAuthority::HoldLast);
    }

    fn healthy_control(commanded: f64, measured: f64) -> ControlFrame {
        ControlFrame {
            altitude_m: 26_000.0,
            true_airspeed_m_s: 1046.95,
            commanded_pitch_rad: commanded,
            measured_pitch_rad: measured,
            discretes: DiscreteFrame::fail_safe(),
            heartbeat: Heartbeat::PRESENT,
            bit_clear: true,
        }
    }

    fn arm_control(commanded: f64, measured: f64) -> ControlFrame {
        ControlFrame {
            discretes: DiscreteFrame {
                master_arm: DualSample::both(true),
                ..DiscreteFrame::fail_safe()
            },
            ..healthy_control(commanded, measured)
        }
    }

    fn abort_control(commanded: f64, measured: f64) -> ControlFrame {
        ControlFrame {
            discretes: DiscreteFrame {
                abort: DualSample::both(true),
                master_arm: DualSample::both(true),
                ..DiscreteFrame::fail_safe()
            },
            ..healthy_control(commanded, measured)
        }
    }

    fn gated_reply(session: &mut HilSession, frame: ControlFrame) -> GatedStepWire {
        let packed = pack_gated_step(frame);
        let (cmd, buf, len) = round_trip(session, CMD_GATED_STEP, &packed);
        assert_eq!(cmd, CMD_GATED_STEP);
        unpack_gated_step_reply(&buf[..len]).expect("gated-step reply layout")
    }

    /// Confirmed abort on GATED_STEP must freeze last_command_rad bit-for-bit.
    /// Same load-bearing property as abort_holds_the_command_that_was_flying,
    /// now on the cablea frames, without a board.
    #[test]
    fn abort_confirmed_on_the_wire_freezes_surfaces() {
        let mut session = test_session();
        let mut kernel = SafetyKernel::new();
        let controller = Controller::ventus1_pitch();
        let mut state = LoopState::default();

        let mut last = gated_reply(&mut session, healthy_control(0.0, 0.0));
        let g = gated_step(&mut kernel, &controller, state, healthy_control(0.0, 0.0));
        state = g.loop_state;
        assert_eq!(last.safety.mode, g.gate.mode);

        for _ in 0..(BIT_PASS_FRAMES as u16 + 8) {
            if session.kernel().mode() == FlightMode::Ready {
                break;
            }
            last = gated_reply(&mut session, healthy_control(0.0, 0.0));
            let g = gated_step(&mut kernel, &controller, state, healthy_control(0.0, 0.0));
            state = g.loop_state;
            assert_eq!(last.safety.mode, g.gate.mode);
            assert_eq!(
                last.last_command_rad.to_bits(),
                g.loop_state.last_command_rad.to_bits()
            );
        }
        assert_eq!(session.kernel().mode(), FlightMode::Ready);

        for _ in 0..(DISCRETE_CONFIRM_FRAMES as u16 + 2) {
            last = gated_reply(&mut session, arm_control(0.05, 0.0));
            let g = gated_step(&mut kernel, &controller, state, arm_control(0.05, 0.0));
            state = g.loop_state;
            if last.safety.mode == FlightMode::Nominal {
                break;
            }
        }
        assert_eq!(last.safety.mode, FlightMode::Nominal);
        assert_eq!(last.safety.authority, SurfaceAuthority::Command);

        // Establish a real deflection under Nominal.
        for _ in 0..20 {
            last = gated_reply(&mut session, arm_control(0.05, 0.0));
            let g = gated_step(&mut kernel, &controller, state, arm_control(0.05, 0.0));
            state = g.loop_state;
            assert_eq!(last.safety.mode, FlightMode::Nominal);
            assert_eq!(
                last.last_command_rad.to_bits(),
                g.loop_state.last_command_rad.to_bits()
            );
        }
        assert!(
            last.last_command_rad > 0.0,
            "setup must have a real command to hold"
        );

        let g1 = gated_reply(&mut session, abort_control(0.05, 0.0));
        let local = gated_step(&mut kernel, &controller, state, abort_control(0.05, 0.0));
        state = local.loop_state;
        assert_eq!(g1.safety.mode, FlightMode::Nominal, "debounce");
        assert_eq!(
            g1.last_command_rad.to_bits(),
            local.loop_state.last_command_rad.to_bits()
        );
        let held = g1.last_command_rad;
        let held_integral = g1.integral;
        assert!(
            held > 0.0,
            "the last Nominal command must be a real deflection"
        );

        let reverse = abort_control(-0.2, 0.0);
        let g2 = gated_reply(&mut session, reverse);
        let local = gated_step(&mut kernel, &controller, state, reverse);
        state = local.loop_state;
        assert_eq!(g2.safety.mode, FlightMode::Emergency);
        assert_eq!(g2.safety.authority, SurfaceAuthority::HoldLast);
        assert_eq!(
            g2.last_command_rad.to_bits(),
            held.to_bits(),
            "abort must freeze the flying command -- if this fails, abort is silent on the wire"
        );
        assert_eq!(g2.integral.to_bits(), held_integral.to_bits());
        assert_eq!(
            g2.last_command_rad.to_bits(),
            local.loop_state.last_command_rad.to_bits()
        );

        let g3 = gated_reply(&mut session, reverse);
        let local = gated_step(&mut kernel, &controller, state, reverse);
        assert_eq!(g3.safety.mode, FlightMode::Safe);
        assert_eq!(g3.last_command_rad.to_bits(), held.to_bits());
        assert_eq!(
            g3.last_command_rad.to_bits(),
            local.loop_state.last_command_rad.to_bits()
        );
        assert_eq!(g3.safety.authority, SurfaceAuthority::HoldLast);
    }

    #[test]
    fn pack_helpers_are_the_documented_layout() {
        let frame = DiscreteFrame {
            abort: DualSample::split(true, false),
            emergency: DualSample::split(false, true),
            master_arm: DualSample::both(true),
            reset_bit: DualSample::split(true, false),
        };
        let packed = pack_safety_tick(frame, Heartbeat::PRESENT, true, Health::SensorFault);
        assert_eq!(packed[0], frame.to_wire());
        assert_eq!(packed[0], 0b0111_1001); // abortA, emB, armA, armB, resetA
        assert_eq!(packed[1], FLAG_HEARTBEAT | FLAG_BIT_CLEAR);
        assert_eq!(packed[2], Health::SensorFault.to_wire());

        let cf = arm_control(0.05, 0.0);
        let g = pack_gated_step(cf);
        assert_eq!(le_f64(&g[0..8]).to_bits(), cf.altitude_m.to_bits());
        assert_eq!(g[32], cf.discretes.to_wire());
        assert_eq!(g[33], FLAG_HEARTBEAT | FLAG_BIT_CLEAR);
    }
}
