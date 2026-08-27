# M9 - 2D Euler solver

`euler2d.cu` is one source file that builds two ways. Without `__CUDACC__` the
`VENTUS_HD` macro expands to nothing and it is plain C++17; with nvcc the same
functions are `__host__ __device__` and the kernel calls them. That is ADR-000
D3: level-A bit-exactness is a property of the source, not of two
implementations that happen to agree today.

## Status

| Level | What it checks | State |
|---|---|---|
| A | CUDA kernel vs CPU reference, bit-exact | **BLOCKED** - see below |
| B | Reduction deterministic, within 4 ULP | **BLOCKED** - same cause |
| D | Shock angle vs exact theta-beta-M | **PASS, 0.006 deg against a 0.5 limit** |

```
level D  shock angle           : PASS
           exact  beta =  26.235 deg  (theta-beta-M, weak branch)
           solver beta =  26.242 deg
           error       =   0.006 deg  (limit 0.5)
```

The physics is validated. The GPU port is not, and the reason is environmental
rather than a defect in the solver.

## [KNOWN_LIMIT] nvcc cannot build this on this machine

CUDA 13.0 supports MSVC 2019 through 2022. The installed toolchain is Visual
Studio 18, which nvcc rejects:

```
host_config.h(164): fatal error C1189: #error: -- unsupported Microsoft Visual
Studio version! Only the versions between 2019 and 2022 (inclusive) are
supported!
```

Overriding with `-allow-unsupported-compiler`, which NVIDIA documents as "may
cause compilation failure or incorrect run time execution", gets past the check
and then crashes the front end:

```
nvcc error : 'cudafe++' died with status 0xC0000005 (ACCESS_VIOLATION)
```

Removing all STL from the translation unit did not help, so it is the VS 18
headers rather than the source.

**This is recorded rather than worked around.** Two options resolve it, and both
are decisions rather than fixes:

1. Install the MSVC 2022 build tools alongside VS 18 and point nvcc at them with
   `-ccbin`. The supported path.
2. Wait for a CUDA release that supports VS 18.

Nothing here is tuned to hide the gap: the program still evaluates levels A and
B when built with CUDA, still gates its benchmarks on them, and reports plainly
that they were not evaluated when built without.

## Building

```bat
native\build_cpu.bat        REM works today; validates level D
native\build_validate.bat   REM nvcc; currently fails as above
```

## Why the domain is a rectangle

A wedge in a rectangular grid needs a cut cell or a body-fitted mesh. Neither is
necessary: work in wedge-fixed coordinates. The wall is the flat bottom of the
domain and the freestream arrives at angle `theta` below it, so the flow must
turn through exactly `theta` to become parallel to the wall and an oblique shock
forms from the leading edge. The angle measured from the wall is `beta - theta`.

The scheme is first-order Rusanov, which smears the shock over a few cells but
does **not** move it - the shock angle is set by the conservation laws, not by
the order of the scheme. That is why a first-order solver is an honest test of
theta-beta-M, and why 0.006 degrees is achievable with a diffusive method.
