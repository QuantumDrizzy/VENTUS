# VENTUS native — M9 only (ADR-000 D1). Everything else is Rust.
# Build via `cargo xtask build`, never directly, so the vcvars check runs first.

cmake_minimum_required(VERSION 3.24)
project(ventus_native LANGUAGES CXX CUDA)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CUDA_STANDARD 17)
set(CMAKE_CUDA_ARCHITECTURES 120)   # RTX 5060 Ti, Blackwell sm_120

# --- ADR-000 D3: two build profiles ------------------------------------------
#
# VENTUS_PROFILE=validate : bit-exact CPU/GPU comparison (level A).
#   -fmad=false disables fused multiply-add so host and device produce
#   identical results. This costs real throughput on Blackwell, which is
#   exactly why it must not leak into release.
#   Benchmarks are compiled out (VENTUS_BENCH_DISABLED).
#
# VENTUS_PROFILE=release  : FMA and contraction enabled. Validated at levels
#   B (deterministic reductions, <= 4 ULP) and D (oblique shock angle) only.
#
set(VENTUS_PROFILE "validate" CACHE STRING "validate | release")
set_property(CACHE VENTUS_PROFILE PROPERTY STRINGS validate release)

if(VENTUS_PROFILE STREQUAL "validate")
    set(VENTUS_CUDA_FLAGS -fmad=false --prec-div=true --prec-sqrt=true --ftz=false)
    set(VENTUS_CXX_FLAGS  /fp:strict)
    add_compile_definitions(VENTUS_BENCH_DISABLED=1)
elseif(VENTUS_PROFILE STREQUAL "release")
    set(VENTUS_CUDA_FLAGS -use_fast_math=false)   # FMA on, but no fast-math
    set(VENTUS_CXX_FLAGS  /fp:precise)
else()
    message(FATAL_ERROR "VENTUS_PROFILE must be 'validate' or 'release', got '${VENTUS_PROFILE}'")
endif()

message(STATUS "VENTUS profile: ${VENTUS_PROFILE}")

# add_subdirectory(ventus_cfd)   # M9 — enabled when the module starts
