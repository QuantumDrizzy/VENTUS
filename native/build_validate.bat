@echo off
REM M9 validate profile. ADR-000 D3: -fmad=false so the compiler cannot fuse a
REM multiply-add on the device and not on the host, which would destroy level-A
REM bit-exactness. Benchmarks are compiled out here.
REM
REM nvcc 13.0 rejects the VS 18 headers, so the host compiler is pinned to the
REM MSVC 2022 Build Tools installed alongside it, via vcvars AND -ccbin. With a
REM supported host compiler the -allow-unsupported-compiler override is gone:
REM NVIDIA's warning about compilation failure or incorrect runtime execution
REM no longer applies.
call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" >nul
if errorlevel 1 (echo vcvars64 failed & exit /b 1)
nvcc -O2 -std=c++17 -arch=sm_120 ^
     -ccbin "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Tools\MSVC\14.44.35207\bin\Hostx64\x64\cl.exe" ^
     -fmad=false --prec-div=true --prec-sqrt=true --ftz=false ^
     -DVENTUS_BENCH_DISABLED=1 ^
     -o "%~dp0..\out\euler2d_validate.exe" "%~dp0ventus_cfd\euler2d.cu"
