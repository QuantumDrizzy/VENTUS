@echo off
REM M9 validate profile. ADR-000 D3: -fmad=false so the compiler cannot fuse a
REM multiply-add on the device and not on the host, which would destroy level-A
REM bit-exactness. Benchmarks are compiled out here.
call "C:\Program Files\Microsoft Visual Studio\18\Community\VC\Auxiliary\Build\vcvars64.bat" >nul
if errorlevel 1 (echo vcvars64 failed & exit /b 1)
REM [KNOWN_LIMIT] CUDA 13.0 supports MSVC 2019-2022 and this machine has
REM VS 18, so the host-compiler version check is overridden. NVIDIA warns this
REM may cause compilation failure or incorrect runtime execution - which is
REM exactly why level A exists: if the unsupported host compiler miscompiled
REM either side, the bit-exact comparison would catch it.
nvcc -O2 -std=c++17 -arch=sm_120 -allow-unsupported-compiler ^
     -fmad=false --prec-div=true --prec-sqrt=true --ftz=false ^
     -DVENTUS_BENCH_DISABLED=1 ^
     -o "%~dp0..\out\euler2d_validate.exe" "%~dp0ventus_cfd\euler2d.cu"
