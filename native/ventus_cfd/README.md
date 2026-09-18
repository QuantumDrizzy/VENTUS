# M9 - 2D Euler solver

`euler2d.cu` is one source file that builds two ways. Without `__CUDACC__` the
`VENTUS_HD` macro expands to nothing and it is plain C++17; with nvcc the same
functions are `__host__ __device__` and the kernel calls them. That is ADR-000
D3: level-A bit-exactness is a property of the source, not of two
implementations that happen to agree today.

## Status

| Level | What it checks | State |
|---|---|---|
| A | CUDA kernel vs CPU reference, bit-exact | **PASS** - 0 of 80 000 cells differ |
| B | Reduction deterministic, within 4 ULP | **PASS** - 0.0 ULP |
| D | Shock angle vs exact theta-beta-M | **PASS** - 0.006 deg against a 0.5 limit |

```
level A  bit-exact elementwise : PASS (0 of 80000 cells differ)
level B  reduction within 4 ULP: PASS (0.0 ULP)
level D  shock angle           : PASS
           exact  beta =  26.235 deg  (theta-beta-M, weak branch)
           solver beta =  26.242 deg
           error       =   0.006 deg  (limit 0.5)
```

## The former nvcc blocker, resolved

CUDA 13.0 supports MSVC 2019 through 2022, and the machine's interactive
toolchain is Visual Studio 18 (VS 2026), which nvcc rejects:

```
host_config.h(164): fatal error C1189: #error: -- unsupported Microsoft Visual
Studio version! Only the versions between 2019 and 2022 (inclusive) are
supported!
```

Overriding with `-allow-unsupported-compiler` got past the check and then
crashed the front end (`cudafe++` died with 0xC0000005). The supported remedy
was already documented here: **Visual Studio Build Tools 2022 (17.14, MSVC
14.44) are now installed alongside VS 18**, and `build_validate.bat` pins both
the environment (`vcvars64` from BuildTools 2022) and the host compiler
(`-ccbin` to its `cl.exe`). The `-allow-unsupported-compiler` override is gone —
with a supported host compiler, NVIDIA's warning about compilation failure or
incorrect runtime execution no longer applies. Levels A and B above were run on
that build.

## Building

```bat
native\build_cpu.bat        REM plain C++17; validates level D
native\build_validate.bat   REM nvcc + MSVC 2022; validates A, B and D
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
