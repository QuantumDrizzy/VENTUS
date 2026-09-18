//! HIL — the wire protocol between the flight software and the digital twin.
//!
//! The hardware-in-the-loop gate runs this crate's air-data path on a real
//! microcontroller (Nucleo-F411RE, ADR-003) and compares it BIT FOR BIT against
//! the host. The protocol lives here, in the same `no_std` crate the firmware
//! links, because a protocol written twice is two protocols: the host and the
//! device would drift exactly the way ADR-000 D1 says the atmosphere would if
//! it were written twice.
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
//! [`handle_frame`] is total: any byte sequence in, one frame out, nothing
//! panics, nothing allocates. A corrupt frame is reported to the caller, not
//! answered — the host's timeout retransmits. The device main loop therefore
//! has exactly three behaviours: read, handle, write.

/// Wire protocol version. Bumped on any breaking change to the framing or the
/// command set; a device that answers a different version fails the gate.
pub const PROTOCOL_VERSION: u8 = 1;

/// Maximum payload a frame can carry [bytes].
pub const MAX_PAYLOAD: usize = 56;
/// Sync + len + cmd + crc.
pub const FRAME_OVERHEAD: usize = 5;
/// Maximum wire length of one frame [bytes].
pub const MAX_FRAME: usize = FRAME_OVERHEAD + MAX_PAYLOAD;

pub const CMD_PING: u8 = 0x01;
pub const CMD_HELLO: u8 = 0x02;
pub const CMD_AIR_DATA: u8 = 0x03;
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

const SYNC: [u8; 2] = [0x7E, 0xC5];

/// What the device says about itself. The firmware fills this from its build
/// script; the host-side tests use a fixed identity.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DeviceEnv {
    pub platform: u8,
    pub build_id: [u8; 8],
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
            crc = if crc & 0x80 != 0 { (crc << 1) ^ 0x07 } else { crc << 1 };
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
/// [`FrameError`] — see the variant docs. A [`FrameError::NoSync`] means the
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
    Ok(Parsed { frame_len, cmd, payload })
}

fn le_f64(bytes: &[u8]) -> f64 {
    let mut bits = [0u8; 8];
    bits.copy_from_slice(&bytes[..8]);
    f64::from_bits(u64::from_le_bytes(bits))
}

fn put_f64(value: f64, out: &mut [u8], at: usize) {
    out[at..at + 8].copy_from_slice(&value.to_bits().to_le_bytes());
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

/// Handle one decoded request. PURE: bytes and [`DeviceEnv`] in, bytes out.
/// No I/O, no allocation, no panic — which is why the host can run the exact
/// dispatch the device will run, and test it without hardware.
///
/// For [`Reply::Error`], the one-byte error payload is left at `out[0]`, so the
/// caller can encode the reply without knowing the code path that produced it.
#[must_use]
pub fn handle(env: &DeviceEnv, cmd: u8, payload: &[u8], out: &mut [u8; MAX_PAYLOAD]) -> Reply {
    match cmd {
        CMD_PING => {
            let n = payload.len();
            out[..n].copy_from_slice(payload);
            Reply::Ok { cmd: CMD_PING, len: n }
        }
        CMD_HELLO => {
            out[0] = PROTOCOL_VERSION;
            out[1] = env.platform;
            out[2..10].copy_from_slice(&env.build_id);
            Reply::Ok { cmd: CMD_HELLO, len: 10 }
        }
        CMD_AIR_DATA => {
            if payload.len() != 16 {
                out[0] = ERR_MALFORMED;
                return Reply::Error { cmd: cmd | ERROR_FLAG, code: ERR_MALFORMED };
            }
            let altitude_m = le_f64(&payload[..8]);
            let true_airspeed_m_s = le_f64(&payload[8..16]);
            match super::air_data(altitude_m, true_airspeed_m_s) {
                Ok(air) => {
                    put_f64(air.altitude_m, out, 0);
                    put_f64(air.mach, out, 8);
                    put_f64(air.dynamic_pressure_pa, out, 16);
                    put_f64(air.static_temperature_k, out, 24);
                    Reply::Ok { cmd: CMD_AIR_DATA, len: 32 }
                }
                Err(e) => {
                    let code = match e {
                        ventus_atmos::AtmosError::BelowDatum => ERR_BELOW_DATUM,
                        ventus_atmos::AtmosError::AboveModelTop => ERR_ABOVE_MODEL_TOP,
                        ventus_atmos::AtmosError::NotANumber => ERR_NOT_A_NUMBER,
                    };
                    out[0] = code;
                    Reply::Error { cmd: cmd | ERROR_FLAG, code }
                }
            }
        }
        _ => {
            out[0] = ERR_UNKNOWN_COMMAND;
            Reply::Error { cmd: cmd | ERROR_FLAG, code: ERR_UNKNOWN_COMMAND }
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
    env: &DeviceEnv,
    frame: &[u8],
    out: &mut [u8; MAX_FRAME],
) -> Result<usize, FrameError> {
    let parsed = decode(frame)?;
    let mut payload = [0u8; MAX_PAYLOAD];
    match handle(env, parsed.cmd, parsed.payload, &mut payload) {
        Reply::Ok { cmd, len } => encode(cmd, &payload[..len], out),
        Reply::Error { cmd, .. } => encode(cmd, &payload[..1], out),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_ENV: DeviceEnv = DeviceEnv { platform: PLATFORM_HOST_TEST, build_id: *b"deadbeef" };

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
        let mut wire = [0u8; MAX_FRAME];
        let n = encode(CMD_PING, b"ping", &mut wire).unwrap();
        wire[n - 1] ^= 0x55; // flip the crc
        assert_eq!(decode(&wire[..n]), Err(FrameError::BadCrc));
        // And handle_frame refuses it entirely - the device sends nothing.
        let mut out = [0u8; MAX_FRAME];
        assert_eq!(handle_frame(&TEST_ENV, &wire[..n], &mut out), Err(FrameError::BadCrc));
    }

    #[test]
    fn hello_reports_the_protocol_version_and_platform() {
        let mut wire = [0u8; MAX_FRAME];
        let n = encode(CMD_HELLO, &[], &mut wire).unwrap();
        let mut out = [0u8; MAX_FRAME];
        let m = handle_frame(&TEST_ENV, &wire[..n], &mut out).unwrap();
        let reply = decode(&out[..m]).unwrap();
        assert_eq!(reply.cmd, CMD_HELLO);
        assert_eq!(reply.payload[0], PROTOCOL_VERSION);
        assert_eq!(reply.payload[1], PLATFORM_HOST_TEST);
        assert_eq!(&reply.payload[2..10], b"deadbeef");
    }

    /// The device's air-data dispatch IS the crate's air-data path: the same
    /// function the twin's gate compares against. Bit-for-bit by construction;
    /// this test pins it so a change to one side of the wire cannot pass alone.
    #[test]
    fn the_air_data_dispatch_is_bit_for_bit_the_air_data_path() {
        for altitude in [0.0, 11_000.0, 24_090.96, 26_000.0, 50_000.0, 84_852.0] {
            let mut wire = [0u8; MAX_FRAME];
            let mut req = [0u8; 16];
            put_f64(altitude, &mut req, 0);
            put_f64(1046.95, &mut req, 8);
            let n = encode(CMD_AIR_DATA, &req, &mut wire).unwrap();

            let mut out = [0u8; MAX_FRAME];
            let m = handle_frame(&TEST_ENV, &wire[..n], &mut out).unwrap();
            let reply = decode(&out[..m]).unwrap();
            assert_eq!(reply.cmd, CMD_AIR_DATA);

            let expected = super::super::air_data(altitude, 1046.95).unwrap();
            assert_eq!(le_f64(&reply.payload[0..8]).to_bits(), expected.altitude_m.to_bits());
            assert_eq!(le_f64(&reply.payload[8..16]).to_bits(), expected.mach.to_bits());
            assert_eq!(le_f64(&reply.payload[16..24]).to_bits(), expected.dynamic_pressure_pa.to_bits());
            assert_eq!(le_f64(&reply.payload[24..32]).to_bits(), expected.static_temperature_k.to_bits());
        }
    }

    /// TOTALITY ON THE WIRE. Bad altitude values are answered with the error
    /// code the atmosphere model declared, never a panic, never a NaN on the
    /// wire pretending to be data.
    #[test]
    fn out_of_domain_air_data_is_an_error_frame_with_the_declared_code() {
        for (altitude, code) in [
            (-100.0, ERR_BELOW_DATUM),
            (200_000.0, ERR_ABOVE_MODEL_TOP),
            (f64::NAN, ERR_NOT_A_NUMBER),
        ] {
            let mut wire = [0u8; MAX_FRAME];
            let mut req = [0u8; 16];
            put_f64(altitude, &mut req, 0);
            put_f64(1046.95, &mut req, 8);
            let n = encode(CMD_AIR_DATA, &req, &mut wire).unwrap();

            let mut out = [0u8; MAX_FRAME];
            let m = handle_frame(&TEST_ENV, &wire[..n], &mut out).unwrap();
            let reply = decode(&out[..m]).unwrap();
            assert_eq!(reply.cmd, CMD_AIR_DATA | ERROR_FLAG);
            assert_eq!(reply.payload, &[code]);
        }
    }

    #[test]
    fn malformed_and_unknown_requests_get_error_frames() {
        let mut wire = [0u8; MAX_FRAME];
        let mut out = [0u8; MAX_FRAME];

        let n = encode(CMD_AIR_DATA, &[0u8; 15], &mut wire).unwrap();
        let m = handle_frame(&TEST_ENV, &wire[..n], &mut out).unwrap();
        assert_eq!(decode(&out[..m]).unwrap().payload, &[ERR_MALFORMED]);

        let n = encode(0x7F, &[], &mut wire).unwrap();
        let m = handle_frame(&TEST_ENV, &wire[..n], &mut out).unwrap();
        assert_eq!(decode(&out[..m]).unwrap().cmd, 0x7F | ERROR_FLAG);
        assert_eq!(decode(&out[..m]).unwrap().payload, &[ERR_UNKNOWN_COMMAND]);
    }

    /// The dispatch must survive arbitrary garbage without panicking: feed it
    /// every single-byte command and a spread of payloads.
    #[test]
    fn the_dispatch_is_total_over_arbitrary_input() {
        let mut wire = [0u8; MAX_FRAME];
        let mut out = [0u8; MAX_FRAME];
        let garbage = [0xA5u8; MAX_PAYLOAD];
        for cmd in 0x00..=0xFF {
            for len in [0usize, 1, 15, 16, 17, MAX_PAYLOAD] {
                let n = encode(cmd, &garbage[..len], &mut wire).unwrap();
                // Ok or Error frame out - either is a defined response.
                let _ = handle_frame(&TEST_ENV, &wire[..n], &mut out);
            }
        }
    }
}
