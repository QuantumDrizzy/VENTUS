# M9 — 2D Euler solver for the inlet shock field.
# Host CPU reference and device kernel live in the SAME .cu as __host__ __device__
# functions, so level-A bit-exactness is a property of the source, not of luck.
# Verification yardstick: oblique shock angle vs exact theta-beta-M from M2.
