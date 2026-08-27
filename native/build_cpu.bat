@echo off
REM CPU-only build of the M9 solver. The same source file: without __CUDACC__
REM the VENTUS_HD macro expands to nothing and it is plain C++17.
REM
REM This exists because nvcc cannot currently build it on this machine - see
REM native/ventus_cfd/README.md. Level D, the physics, does not need the GPU.
call "C:\Program Files\Microsoft Visual Studio\18\Community\VC\Auxiliary\Build\vcvars64.bat" >nul
cl /nologo /O2 /std:c++17 /fp:strict /EHsc /Tp "%~dp0ventus_cfd\euler2d.cu" ^
   /Fe:"%~dp0..\out\euler2d_cpu.exe" /Fo:"%~dp0..\out\\" >nul
