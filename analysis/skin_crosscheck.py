#!/usr/bin/env python3
"""Independent cross-check of ventus-thermal's radiation-equilibrium flat-plate skin temperature.

Shares no code with the crates. Written from the model as the crates document it:

    edge state      US Standard Atmosphere 1976 at a geopotential altitude, M * a, gamma 1.4
    recovery        T_aw = T_e + Pr^(1/3) (T_0 - T_e), turbulent, Pr = 0.71
    reference temp  Eckert: T* = T_e + 0.5 (T_w - T_e) + 0.22 (T_aw - T_e)
    friction        Prandtl-Schlichting, c_f = 0.0592 Re*_x^(-1/5), properties at T*
    heat transfer   Chilton-Colburn, St = (c_f / 2) Pr^(-2/3); h = St rho* u cp(T*)
    balance         h (T_aw - T_w) = eps sigma (T_w^4 - T_sink^4)

The model's stated inputs are taken as given, not re-derived: the Cengel Table A-2b cubic for cp
(itself [TO VERIFY] in ventus-gasdyn), Sutherland's constants, the US76 constants. The solver is a
different one (Brent's method from scipy, not a fixed-count bisection).

    python analysis/skin_crosscheck.py

Like everything in analysis/, nothing in crates/ imports or runs this (ADR-000 D8); the cases cite it.
"""

import math

from scipy.optimize import brentq

G0 = 9.80665
R_STAR = 8314.32          # J/(kmol K), US76
M0 = 28.9644              # kg/kmol, US76
R_AIR = R_STAR / M0
SIGMA = 5.670374419e-8
PR = 0.71
GAMMA = 1.4
SUTH_C1, SUTH_S = 1.458e-6, 110.4

# US76 geopotential layers up to 32 km: base altitude [m], base temperature [K], lapse [K/m].
LAYERS = [(0.0, 288.15, -0.0065), (11000.0, 216.65, 0.0), (20000.0, 216.65, 0.001), (32000.0, 228.65, 0.0028)]


def us76(h):
    """(T, p) at geopotential altitude h [m], h < 47 km, integrated layer by layer from 101325 Pa."""
    p = 101325.0
    for i, (hb, tb, lapse) in enumerate(LAYERS):
        top = LAYERS[i + 1][0] if i + 1 < len(LAYERS) else 47000.0
        dh = min(h, top) - hb
        if lapse == 0.0:
            p_end = p * math.exp(-G0 * M0 * dh / (R_STAR * tb))
        else:
            p_end = p * ((tb + lapse * dh) / tb) ** (-G0 * M0 / (R_STAR * lapse))
        if h <= top:
            return tb + lapse * dh, p_end
        p = p_end
    raise ValueError("above 47 km")


def cp_air(t):
    """Cengel Table A-2b cubic, J/(mol K) -> J/(kg K); the model's own input."""
    return (28.11 + 0.1967e-2 * t + 0.4802e-5 * t * t - 1.966e-9 * t ** 3) / (M0 / 1000.0)


def mu_air(t):
    return SUTH_C1 * t ** 1.5 / (t + SUTH_S)


def skin(h_geo, mach, x, eps=0.85, t_sink=0.0):
    t_e, p = us76(h_geo)
    u = mach * math.sqrt(GAMMA * R_AIR * t_e)
    t0 = t_e * (1 + (GAMMA - 1) / 2 * mach * mach)
    t_aw = t_e + PR ** (1 / 3) * (t0 - t_e)

    def film(t_w):
        t_star = t_e + 0.5 * (t_w - t_e) + 0.22 * (t_aw - t_e)
        rho = p / (R_AIR * t_star)
        re = rho * u * x / mu_air(t_star)
        cf = 0.0592 * re ** -0.2
        st = cf / 2 * PR ** (-2 / 3)
        cp = cp_air(t_star) if 273.0 <= t_star <= 1800.0 else R_AIR * GAMMA / (GAMMA - 1)
        return st * rho * u * cp

    def net(t_w):
        return film(t_w) * (t_aw - t_w) - eps * SIGMA * (t_w ** 4 - t_sink ** 4)

    t_w = brentq(net, t_e, t_aw, xtol=1e-12, rtol=1e-15)
    return {"wall_temperature_k": t_w, "adiabatic_wall_temperature_k": t_aw,
            "radiation_relief_k": t_aw - t_w, "heat_transfer_coefficient_w_m2_k": film(t_w)}


if __name__ == "__main__":
    print("M 3.50 pins (the corpus's existing cases):")
    print("  SR-71 M 3.2 / 24 km, 1 m:", skin(24000.0, 3.2, 1.0))
    print("  design point M 3.5 / 26 km, 10 m:", skin(26000.0, 3.5, 10.0))
    print("M 4.00 design-q row, 27 747.256 m:")
    for x in (1.0, 5.0, 10.0, 20.0, 30.0):
        r = skin(27747.25646305947, 4.0, x)
        print(f"  x {x:>4} m: " + ", ".join(f"{k} {v!r}" for k, v in r.items()))
