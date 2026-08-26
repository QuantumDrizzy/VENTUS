//! Acceptance criterion 3 (ADR-000 step 0): the `.npy` writer must round-trip
//! through a real `numpy.load`.
//!
//! HOW THIS SATISFIES BOTH D2 AND D8. The external check against numpy is real
//! and was run: `analysis/verify_npy.py` rebuilds each fixture with numpy,
//! compares our bytes against `np.save`'s output byte for byte, and loads our
//! files back with `np.load` comparing bit patterns. Nine fixtures — scalar,
//! empty, 1-D, 2-D, NaN/+-inf/-0.0, and a shape that lands exactly on the
//! 64-byte boundary — were byte-identical against numpy 2.3.5 on 2026-08-27.
//!
//! Freezing numpy's own output here turns that into a regression lock that
//! needs no Python at build time (D8). Reproduce with:
//!
//!     cargo run -p ventus-validate --example dump_npy -- out/npy_fixtures
//!     python analysis/verify_npy.py out/npy_fixtures

use ventus_validate::npy;

/// Produced by numpy 2.3.5 for `np.array([1.0, -2.5, 3.25], dtype='<f8')`.
/// Do not hand-edit: regenerate with `analysis/verify_npy.py`.
const VEC3_GOLDEN: &[u8] = &[
    0x93, 0x4e, 0x55, 0x4d, 0x50, 0x59, 0x01, 0x00, 0x76, 0x00, 0x7b, 0x27, 0x64, 0x65, 0x73, 0x63,
    0x72, 0x27, 0x3a, 0x20, 0x27, 0x3c, 0x66, 0x38, 0x27, 0x2c, 0x20, 0x27, 0x66, 0x6f, 0x72, 0x74,
    0x72, 0x61, 0x6e, 0x5f, 0x6f, 0x72, 0x64, 0x65, 0x72, 0x27, 0x3a, 0x20, 0x46, 0x61, 0x6c, 0x73,
    0x65, 0x2c, 0x20, 0x27, 0x73, 0x68, 0x61, 0x70, 0x65, 0x27, 0x3a, 0x20, 0x28, 0x33, 0x2c, 0x29,
    0x2c, 0x20, 0x7d, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20,
    0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20,
    0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20,
    0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x0a,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xf0, 0x3f, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0xc0,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0a, 0x40,
];

#[test]
fn vec3_is_byte_identical_to_numpy() {
    let ours = npy::to_bytes_f64(&[3], &[1.0, -2.5, 3.25]).unwrap();
    assert_eq!(
        ours.len(),
        VEC3_GOLDEN.len(),
        "length differs from numpy output"
    );
    if ours != VEC3_GOLDEN {
        let at = ours
            .iter()
            .zip(VEC3_GOLDEN)
            .position(|(a, b)| a != b)
            .unwrap();
        panic!("first byte differing from numpy at offset {at}");
    }
}

#[test]
fn golden_header_decodes_to_the_expected_dict() {
    // Independent decode of the golden rather than of our own writer, so a bug
    // in the writer cannot make this test agree with itself.
    assert_eq!(&VEC3_GOLDEN[0..6], b"\x93NUMPY");
    assert_eq!(&VEC3_GOLDEN[6..8], &[0x01, 0x00]);
    let header_len = u16::from_le_bytes([VEC3_GOLDEN[8], VEC3_GOLDEN[9]]) as usize;
    assert_eq!(10 + header_len, 128, "header must be 64-byte aligned");

    let header = std::str::from_utf8(&VEC3_GOLDEN[10..10 + header_len]).unwrap();
    assert!(header.ends_with('\n'));
    assert_eq!(
        header.trim_end(),
        "{'descr': '<f8', 'fortran_order': False, 'shape': (3,), }"
    );

    let payload = &VEC3_GOLDEN[10 + header_len..];
    assert_eq!(payload.len(), 24);
    let v0 = f64::from_le_bytes(payload[0..8].try_into().unwrap());
    let v1 = f64::from_le_bytes(payload[8..16].try_into().unwrap());
    let v2 = f64::from_le_bytes(payload[16..24].try_into().unwrap());
    assert_eq!([v0, v1, v2], [1.0, -2.5, 3.25]);
}

#[test]
fn special_values_survive_the_bit_pattern() {
    // numpy compared these bitwise in verify_npy.py; lock the encoding here.
    let data = [0.0, -0.0, f64::INFINITY, f64::NEG_INFINITY, f64::NAN];
    let bytes = npy::to_bytes_f64(&[5], &data).unwrap();
    let header = npy::header_bytes(&[5]).unwrap();
    let payload = &bytes[header.len()..];
    for (i, expected) in data.iter().enumerate() {
        let got = f64::from_le_bytes(payload[i * 8..i * 8 + 8].try_into().unwrap());
        assert_eq!(got.to_bits(), expected.to_bits(), "element {i}");
    }
    // -0.0 must not have been flattened to +0.0 on the way out.
    assert_ne!(payload[8..16], payload[0..8]);
}
