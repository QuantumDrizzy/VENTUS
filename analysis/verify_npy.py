"""Verify the Rust .npy writer against numpy, byte for byte.

ADR-000 D2 says the .npy header is where this format is usually got wrong, and
D8 says Python must never be required to build. Both hold: this script is the
one-time external check, and its result is frozen into a Rust golden test that
needs no Python.

Usage:
    cargo run -p ventus-validate --example dump_npy -- <dir>
    python analysis/verify_npy.py <dir>

Exits non-zero on any mismatch. Prints a Rust golden literal on success.
"""

import io
import sys
import struct
from pathlib import Path

import numpy as np


def rebuild(shape_txt, data_txt):
    shape = tuple(int(s) for s in shape_txt.split(",") if s != "")
    bits = [int(h, 16) for h in data_txt.split(",") if h != ""]
    flat = np.array([struct.unpack("<d", struct.pack("<Q", b))[0] for b in bits],
                    dtype="<f8")
    return flat.reshape(shape)


def numpy_bytes(arr):
    buf = io.BytesIO()
    np.save(buf, arr, allow_pickle=False)
    return buf.getvalue()


def main():
    if len(sys.argv) != 2:
        print(__doc__)
        return 2
    d = Path(sys.argv[1])
    spec = (d / "spec.tsv").read_text().strip().splitlines()

    failures = 0
    checked = 0
    golden = None

    for line in spec:
        name, shape_txt, data_txt = line.split("\t")
        arr = rebuild(shape_txt, data_txt)
        ours = (d / f"{name}.npy").read_bytes()
        theirs = numpy_bytes(arr)

        if ours != theirs:
            failures += 1
            print(f"MISMATCH {name}: shape={arr.shape}")
            print(f"  ours   {len(ours)} bytes: {ours[:80]!r}")
            print(f"  numpy  {len(theirs)} bytes: {theirs[:80]!r}")
            for i, (a, b) in enumerate(zip(ours, theirs)):
                if a != b:
                    print(f"  first differing byte at {i}: ours={a:#04x} numpy={b:#04x}")
                    break
            continue

        # numpy must also be able to read our file back with identical values,
        # NaN included (compared bitwise, since NaN != NaN).
        back = np.load(d / f"{name}.npy")
        assert back.dtype == np.dtype("<f8"), f"{name}: dtype {back.dtype}"
        assert back.shape == arr.shape, f"{name}: shape {back.shape} != {arr.shape}"
        if back.size:
            a_bits = back.reshape(-1).view(np.uint64)
            b_bits = arr.reshape(-1).view(np.uint64)
            assert np.array_equal(a_bits, b_bits), f"{name}: bit-level round-trip failed"

        checked += 1
        print(f"ok  {name:16s} shape={str(arr.shape):12s} {len(ours):7d} bytes")

        if name == "vec3":
            golden = ours

    print(f"\n{checked} fixtures byte-identical to numpy {np.__version__}, "
          f"{failures} mismatches")

    if failures == 0 and golden is not None:
        print("\n// Golden for tests/npy_golden.rs, produced by numpy "
              f"{np.__version__} via analysis/verify_npy.py")
        print("const VEC3_GOLDEN: &[u8] = &[")
        for i in range(0, len(golden), 12):
            row = ", ".join(f"0x{b:02x}" for b in golden[i:i + 12])
            print(f"    {row},")
        print("];")

    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
