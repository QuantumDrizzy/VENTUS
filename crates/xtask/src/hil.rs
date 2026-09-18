//! The hardware-in-the-loop gate (ADR-003).
//!
//! Opens the Nucleo-F411RE's virtual COM port and confronts the device's
//! air-data path against the host's — BIT FOR BIT — over a deterministic
//! altitude sweep. The expected values come from `ventus_fsw::air_data`, the
//! same function the device runs compiled for ARM; any difference means the
//! port, the hardware arithmetic, or the wire has lied.
//!
//! Without a port argument this fails with the exact remedy rather than
//! degrading, like every other xtask check.

use std::process::ExitCode;
use std::time::Duration;

use ventus_fsw::hil;

const BAUDRATE: u32 = 115_200;
const TIMEOUT_S: u64 = 2;
const RETRIES: usize = 2;

/// The true airspeed every sweep point uses — the design-point cruise value,
/// so the point that matters most is in the sweep by construction.
const TAS_M_S: f64 = 1046.95;

pub fn run(port: Option<&str>) -> ExitCode {
    let Some(port_name) = port else {
        eprintln!("hil: no port given.");
        eprintln!("remedy: cargo xtask hil COM7  (Windows) or /dev/ttyACM0 (Linux)");
        eprintln!("        the Nucleo's ST-Link VCP appears as \"USB Serial Device\" in");
        eprintln!("        Device Manager once the board is plugged in by USB.");
        return ExitCode::from(2);
    };

    let Ok(mut port) = serialport::new(port_name, BAUDRATE)
        .timeout(Duration::from_secs(TIMEOUT_S))
        .open()
    else {
        eprintln!("hil: could not open {port_name} at {BAUDRATE} baud.");
        eprintln!("remedy: plug the Nucleo in by USB, close any other program holding the");
        eprintln!("        port, and check Device Manager for the ST-Link COM number.");
        return ExitCode::from(2);
    };

    let mut failures = 0usize;

    // --- HELLO: the device must identify itself as THIS commit's build. -----
    let Ok((cmd, payload)) = exchange(&mut *port, hil::CMD_HELLO, &[]) else {
        eprintln!("hil  hello  : NO REPLY");
        eprintln!("remedy: flash the firmware first - see firmware/nucleo-f411/README.md.");
        return ExitCode::FAILURE;
    };
    if cmd != hil::CMD_HELLO || payload.len() != 10 {
        eprintln!("hil  hello  : MALFORMED (cmd {cmd:#04x}, {} bytes)", payload.len());
        return ExitCode::FAILURE;
    }
    let device_proto = payload[0];
    let device_platform = payload[1];
    let mut device_build = [0u8; 8];
    device_build.copy_from_slice(&payload[2..10]);

    let head = head_build_id();
    let build_ok = device_build == head;
    if !build_ok {
        failures += 1;
    }
    println!(
        "hil  hello  : protocol {}, platform {}, build {}{}",
        device_proto,
        platform_name(device_platform),
        fmt_build(device_build),
        if build_ok { "" } else { " != HEAD" },
    );
    if device_proto != hil::PROTOCOL_VERSION {
        eprintln!("remedy: the device answers protocol {device_proto}, this host speaks {} - rebuild both sides from the same commit.", hil::PROTOCOL_VERSION);
        return ExitCode::FAILURE;
    }
    if !build_ok {
        eprintln!(
            "remedy: device build {} != HEAD {} - reflash: cargo flash (in firmware/nucleo-f411/).",
            fmt_build(device_build),
            fmt_build(head)
        );
    }

    // --- The sweep: deterministic, so a failure is reproducible. ------------
    let mut points = Vec::new();
    // US76 layer bases and boundaries - where the model changes formula.
    for &h in &[
        0.0, 11_000.0, 20_000.0, 32_000.0, 47_000.0, 51_000.0, 71_000.0, 84_852.0,
    ] {
        points.push(h);
    }
    // The design points this project actually cares about.
    points.push(24_090.96); // SR-71 cruise geopotential, the [CORRECTED] figure
    points.push(26_000.0); // VENTUS-1 cruise
    // Deterministic pseudo-random points across the whole domain. An LCG, not
    // rand: no dependency, and the sweep is identical on every machine.
    let mut x: u64 = 0x5645_4E54_5553_3131; // "VENTUS11"
    for _ in 0..12 {
        x = x.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        let frac = (x >> 11) as f64 / (1u64 << 53) as f64;
        points.push(frac * 84_852.0);
    }

    let mut exact = 0usize;
    let mut mismatches = Vec::new();
    for &altitude in &points {
        match exchange(&mut *port, hil::CMD_AIR_DATA, &f64s(altitude, TAS_M_S)) {
            Ok((cmd, payload)) => {
                let expected = match ventus_fsw::air_data(altitude, TAS_M_S) {
                    Ok(a) => a,
                    Err(_) => {
                        mismatches.push(format!("h={altitude}: host refused its own sweep point"));
                        failures += 1;
                        continue;
                    }
                };
                if cmd != hil::CMD_AIR_DATA || payload.len() != 32 {
                    mismatches.push(format!("h={altitude}: reply not air-data (cmd {cmd:#04x}, {} bytes)", payload.len()));
                    failures += 1;
                    continue;
                }
                let got = [
                    f64_at(&payload, 0),
                    f64_at(&payload, 8),
                    f64_at(&payload, 16),
                    f64_at(&payload, 24),
                ];
                let want = [
                    expected.altitude_m,
                    expected.mach,
                    expected.dynamic_pressure_pa,
                    expected.static_temperature_k,
                ];
                if got.iter().zip(want.iter()).all(|(g, w)| g.to_bits() == w.to_bits()) {
                    exact += 1;
                } else {
                    let field = ["altitude", "mach", "q", "temperature"]
                        .iter()
                        .zip(got.iter().zip(want.iter()))
                        .find(|(_, (g, w))| g.to_bits() != w.to_bits());
                    mismatches.push(format!(
                        "h={altitude}: {:?} device {:?} host {:?}",
                        field.map(|(n, _)| *n),
                        field.map(|(_, (g, _))| g),
                        field.map(|(_, (_, w))| w),
                    ));
                    failures += 1;
                }
            }
            Err(e) => {
                mismatches.push(format!("h={altitude}: {e}"));
                failures += 1;
            }
        }
    }
    println!("hil  air-data : {exact}/{} points bit-exact", points.len());
    for m in &mismatches {
        eprintln!("             {m}");
    }

    // --- Out of domain: the device must answer the DECLARED error codes. ----
    let error_cases = [
        (-100.0, hil::ERR_BELOW_DATUM),
        (200_000.0, hil::ERR_ABOVE_MODEL_TOP),
        (f64::NAN, hil::ERR_NOT_A_NUMBER),
    ];
    let mut error_ok = 0usize;
    for (altitude, want_code) in &error_cases {
        match exchange(&mut *port, hil::CMD_AIR_DATA, &f64s(*altitude, TAS_M_S)) {
            Ok((cmd, payload))
                if cmd == hil::CMD_AIR_DATA | hil::ERROR_FLAG
                    && payload == &[*want_code] =>
            {
                error_ok += 1;
            }
            Ok((cmd, payload)) => {
                eprintln!("             h={altitude}: error reply cmd {cmd:#04x} payload {payload:?}, wanted code {want_code}");
                failures += 1;
            }
            Err(e) => {
                eprintln!("             h={altitude}: {e}");
                failures += 1;
            }
        }
    }
    println!("hil  errors  : {error_ok}/{} out-of-domain codes as declared", error_cases.len());

    if failures == 0 {
        println!("hil  verdict : PASS - the ARM port computes the twin's atmosphere bit for bit");
        ExitCode::SUCCESS
    } else {
        println!("hil  verdict : FAIL ({failures} failure(s))");
        ExitCode::FAILURE
    }
}

/// One request with retransmission. A corrupt or missing reply is retried
/// RETRIES times; a reply is returned as (cmd, payload).
fn exchange(
    port: &mut dyn serialport::SerialPort,
    cmd: u8,
    payload: &[u8],
) -> Result<(u8, Vec<u8>), String> {
    let mut wire = [0u8; hil::MAX_FRAME];
    let n = hil::encode(cmd, payload, &mut wire).map_err(|e| format!("encode: {e:?}"))?;

    for _attempt in 0..RETRIES {
        port.write_all(&wire[..n]).map_err(|e| format!("write: {e}"))?;
        port.flush().map_err(|e| format!("flush: {e}"))?;

        let mut buf = [0u8; hil::MAX_FRAME];
        let mut got = 0usize;
        loop {
            match read_byte(&mut *port) {
                Ok(b) => {
                    buf[got] = b;
                    got += 1;
                }
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => break,
                Err(e) => return Err(format!("read: {e}")),
            }
            if got >= hil::FRAME_OVERHEAD {
                match hil::decode(&buf[..got]) {
                    Ok(parsed) => return Ok((parsed.cmd, parsed.payload.to_vec())),
                    Err(hil::FrameError::Truncated) if got < hil::MAX_FRAME => continue,
                    Err(_) => break, // corrupt: retransmit
                }
            }
        }
    }
    Err(format!("no complete reply after {RETRIES} attempts (timeout {TIMEOUT_S} s each)"))
}

/// One byte from the port. `io::Read::bytes` is `where Self: Sized`, so a
/// trait object needs the raw `read` call instead.
fn read_byte(port: &mut dyn serialport::SerialPort) -> std::io::Result<u8> {
    let mut b = [0u8; 1];
    loop {
        match port.read(&mut b) {
            Ok(0) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "port closed",
                ))
            }
            Ok(_) => return Ok(b[0]),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
}

fn f64s(a: f64, b: f64) -> Vec<u8> {
    let mut out = [0u8; 16];
    out[..8].copy_from_slice(&a.to_bits().to_le_bytes());
    out[8..].copy_from_slice(&b.to_bits().to_le_bytes());
    out.to_vec()
}

fn f64_at(bytes: &[u8], at: usize) -> f64 {
    let mut bits = [0u8; 8];
    bits.copy_from_slice(&bytes[at..at + 8]);
    f64::from_bits(u64::from_le_bytes(bits))
}

/// First 8 bytes of `git rev-parse HEAD`, hex-parsed the same way the
/// firmware's build script does it.
fn head_build_id() -> [u8; 8] {
    let out = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success());
    let hex = out
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    parse_build_id(&hex)
}

fn parse_build_id(hex: &str) -> [u8; 8] {
    let mut id = [0u8; 8];
    if hex.len() >= 16 {
        for (i, byte) in id.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap_or(0);
        }
    } else {
        id = [0xFF; 8];
    }
    id
}

fn fmt_build(id: [u8; 8]) -> String {
    id.iter().map(|b| format!("{b:02x}")).collect()
}

fn platform_name(p: u8) -> &'static str {
    match p {
        hil::PLATFORM_NUCLEO_F411 => "nucleo-f411",
        hil::PLATFORM_HOST_TEST => "host-test",
        _ => "unknown",
    }
}
