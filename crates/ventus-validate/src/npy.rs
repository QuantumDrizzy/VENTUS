//! `.npy` v1.0 writer — the Rust/C++ to Python boundary (ADR-000 D2).
//!
//! The header is where this format is usually got wrong, so the layout is
//! spelled out rather than approximated. Mirrors numpy's `_wrap_header`:
//!
//! ```text
//! magic        6 bytes   \x93NUMPY
//! version      2 bytes   \x01\x00
//! header_len   2 bytes   u16 little-endian = len(dict) + padlen + 1
//! dict         N bytes   {'descr': '<f8', 'fortran_order': False, 'shape': (3,), }
//! padding      P bytes   0x20 spaces
//! newline      1 byte    \n
//! data                   raw little-endian f64, C order
//! ```
//!
//! with `padlen = 64 - ((10 + len(dict) + 1) % 64)`. Note that when that
//! modulus is zero numpy pads a **full extra 64 bytes** rather than none; a
//! writer that "optimises" that away produces a valid file that is not
//! byte-identical to numpy's, which would make the golden test meaningless.
//!
//! Verified against numpy 2.3.5 byte-for-byte by `analysis/verify_npy.py`,
//! and locked in by `tests/npy_golden.rs` so the check needs no Python at
//! build time (ADR-000 D8).

use std::fmt;
use std::io::Write;
use std::path::Path;

const MAGIC: &[u8; 6] = b"\x93NUMPY";
const VERSION: [u8; 2] = [0x01, 0x00];
const ARRAY_ALIGN: usize = 64;
/// magic (6) + version (2) + header_len field (2)
const PREFIX_LEN: usize = 10;

#[derive(Debug)]
pub enum NpyError {
    /// `shape` does not describe `data`.
    ShapeMismatch {
        expected: usize,
        got: usize,
    },
    /// The header does not fit in the v1.0 `u16` length field.
    HeaderTooLong {
        len: usize,
    },
    Io(std::io::Error),
}

impl fmt::Display for NpyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NpyError::ShapeMismatch { expected, got } => write!(
                f,
                "shape implies {expected} elements but {got} were supplied"
            ),
            NpyError::HeaderTooLong { len } => write!(
                f,
                "header is {len} bytes, which does not fit the npy v1.0 u16 \
                 length field; v2.0 would be required"
            ),
            NpyError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for NpyError {}

impl From<std::io::Error> for NpyError {
    fn from(e: std::io::Error) -> Self {
        NpyError::Io(e)
    }
}

/// Python `repr` of a shape tuple: `()`, `(3,)`, `(2, 3)`.
fn shape_repr(shape: &[usize]) -> String {
    match shape {
        [] => "()".to_string(),
        [n] => format!("({n},)"),
        _ => {
            let inner: Vec<String> = shape.iter().map(usize::to_string).collect();
            format!("({})", inner.join(", "))
        }
    }
}

/// The complete header: prefix, dict, padding and terminating newline.
/// Always a multiple of 64 bytes.
pub fn header_bytes(shape: &[usize]) -> Result<Vec<u8>, NpyError> {
    let dict = format!(
        "{{'descr': '<f8', 'fortran_order': False, 'shape': {}, }}",
        shape_repr(shape)
    );

    // numpy: hlen counts the trailing newline; padlen is a full block when the
    // sum already aligns.
    let hlen = dict.len() + 1;
    let padlen = ARRAY_ALIGN - ((PREFIX_LEN + hlen) % ARRAY_ALIGN);
    let header_len = hlen + padlen;

    if header_len > u16::MAX as usize {
        return Err(NpyError::HeaderTooLong { len: header_len });
    }

    let mut out = Vec::with_capacity(PREFIX_LEN + header_len);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&VERSION);
    out.extend_from_slice(&(header_len as u16).to_le_bytes());
    out.extend_from_slice(dict.as_bytes());
    out.resize(out.len() + padlen, b' ');
    out.push(b'\n');

    debug_assert_eq!(
        out.len() % ARRAY_ALIGN,
        0,
        "npy header must be 64-byte aligned"
    );
    Ok(out)
}

/// Serialise an f64 array in C order.
pub fn to_bytes_f64(shape: &[usize], data: &[f64]) -> Result<Vec<u8>, NpyError> {
    let expected: usize = shape.iter().product();
    if expected != data.len() {
        return Err(NpyError::ShapeMismatch {
            expected,
            got: data.len(),
        });
    }

    let mut out = header_bytes(shape)?;
    out.reserve(data.len() * 8);
    for v in data {
        out.extend_from_slice(&v.to_le_bytes());
    }
    Ok(out)
}

/// Write an f64 array to `path` as `.npy`.
pub fn write_f64(path: &Path, shape: &[usize], data: &[f64]) -> Result<(), NpyError> {
    let bytes = to_bytes_f64(shape, data)?;
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let mut f = std::fs::File::create(path)?;
    f.write_all(&bytes)?;
    f.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_is_always_64_byte_aligned() {
        // Includes 1-D shapes whose dict length lands exactly on a boundary,
        // where numpy pads a full extra block.
        for n in 0..5000_usize {
            let h = header_bytes(&[n]).unwrap();
            assert_eq!(h.len() % ARRAY_ALIGN, 0, "n = {n}");
            assert_eq!(*h.last().unwrap(), b'\n');
        }
        for shape in [vec![], vec![1], vec![2, 3], vec![4, 5, 6], vec![1, 1, 1, 1]] {
            let h = header_bytes(&shape).unwrap();
            assert_eq!(h.len() % ARRAY_ALIGN, 0, "shape = {shape:?}");
        }
    }

    #[test]
    fn full_extra_block_when_already_aligned() {
        // A 1-D shape cannot reach this branch: the dict length only varies with
        // the digit count of n, so it takes a handful of values and none of them
        // is congruent to -11 (mod 64). Vary the number of dimensions instead —
        // each extra ", 1" adds 3 bytes, and gcd(3, 64) = 1, so every residue is
        // reachable within 64 steps.
        let mut found = None;
        for k in 2..80_usize {
            let shape = vec![1_usize; k];
            let dict_len = format!(
                "{{'descr': '<f8', 'fortran_order': False, 'shape': {}, }}",
                shape_repr(&shape)
            )
            .len();
            if (PREFIX_LEN + dict_len + 1) % ARRAY_ALIGN == 0 {
                let h = header_bytes(&shape).unwrap();
                assert_eq!(
                    h.len(),
                    PREFIX_LEN + dict_len + 1 + ARRAY_ALIGN,
                    "k = {k}: numpy pads a full extra block when already aligned"
                );
                found = Some(k);
                break;
            }
        }
        assert!(
            found.is_some(),
            "no already-aligned shape found; branch untested"
        );
    }

    #[test]
    fn shape_repr_matches_python() {
        assert_eq!(shape_repr(&[]), "()");
        assert_eq!(shape_repr(&[3]), "(3,)");
        assert_eq!(shape_repr(&[2, 3]), "(2, 3)");
        assert_eq!(shape_repr(&[4, 5, 6]), "(4, 5, 6)");
    }

    #[test]
    fn shape_mismatch_is_refused() {
        let e = to_bytes_f64(&[2, 3], &[1.0, 2.0]).unwrap_err();
        assert!(matches!(
            e,
            NpyError::ShapeMismatch {
                expected: 6,
                got: 2
            }
        ));
    }

    #[test]
    fn data_is_little_endian_c_order() {
        let bytes = to_bytes_f64(&[2, 2], &[1.0, 2.0, 3.0, 4.0]).unwrap();
        let header = header_bytes(&[2, 2]).unwrap();
        let payload = &bytes[header.len()..];
        assert_eq!(payload.len(), 32);
        assert_eq!(&payload[0..8], &1.0_f64.to_le_bytes());
        assert_eq!(&payload[24..32], &4.0_f64.to_le_bytes());
    }
}
