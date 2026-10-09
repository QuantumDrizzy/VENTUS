#!/usr/bin/env python3
"""VENTUS-1 baseline three-view, drawn from the geometry the repo derives.

Shares no code with the crates (ADR-000 D8). It re-derives ventus_aero::geometry::derive for the
baseline body (GeometrySpec::SNAPSHOT, which ADR-008/009 keep as the M 4.00 body) and then
refuses to draw unless the derivation reproduces the values the Rust code prints:

    S 96.57 m2 | b 12.81 m | L 24.85 m | A_max 3.3685 m2 | V_SH 49.31 m3 | capture/body 0.7934

Provenance of every drawn dimension:

    DERIVED   span, wing area, length, max diameter, Sears-Haack profile and volume,
              pure-delta root chord c = 2S/b and leading-edge sweep, total inlet capture area
    DECLARED  (this script's layout choices, NOT in the repo; ADR-007 / design-point record no layout)
              wing trailing edge flush with the aft end of the body; nacelles on the wing underside at
              y = +-2.2 m; two rectangular 2-D inlets splitting the capture area; no fin, no gear, no canopy
    NOT DRAWN what the repo does not define

The capture area 0.7934 * A_max comes from ventus-envelope `required_capture_area_with_combustor`
(M 4.00, COOLED_LINER_CANDIDATE, snapshot body); it is an input here, not re-derived.

    python analysis/ventus_3view.py            # writes out/ventus_3view.png, after the checks pass
    python analysis/ventus_3view.py --check    # checks only
"""

import math
import os
import sys

# --- repo constants (ventus-aero geometry.rs, ventus-envelope) -------------------------------------
G0 = 9.80665
Q_PA = 18_463.0
CRUISE_MASS_KG = 28_000.0
CL = 0.154
ASPECT_RATIO = 1.7
FINENESS = 12.0
SR71_LENGTH_M = 32.7
SR71_WING_AREA_M2 = 167.2
CAPTURE_OVER_BODY = 0.7934321777874744   # ventus-envelope, M 4.00, cooled-liner candidate

# --- declared layout (script choices; see module docstring) ----------------------------------------
NACELLE_Y_M = 2.2
INLET_WIDTH_M = 1.2
WING_THICKNESS_M = 0.25
LIP_X_M = 2.0

# Values the Rust code printed for the baseline body; the script must reproduce them.
PINNED = {
    "wing_area_m2": (96.57, 5e-4),
    "span_m": (12.81, 5e-4),
    "length_m": (24.85, 5e-4),
    "a_max_m2": (3.3685, 5e-4),
    "volume_m3": (49.31, 5e-4),
}


def derive():
    weight_n = CRUISE_MASS_KG * G0
    s = weight_n / (Q_PA * CL)
    b = math.sqrt(ASPECT_RATIO * s)
    length = SR71_LENGTH_M * math.sqrt(s / SR71_WING_AREA_M2)
    d = length / FINENESS
    a_max = math.pi * d * d / 4.0
    return {
        "wing_area_m2": s,
        "span_m": b,
        "length_m": length,
        "d_max_m": d,
        "a_max_m2": a_max,
        "volume_m3": 3.0 * math.pi / 16.0 * a_max * length,
        "root_chord_m": 2.0 * s / b,
    }


def sears_haack_radius(x, length, r_max):
    """Radius at station x in [-L/2, L/2]: A(x) = A_max (1 - (2x/L)^2)^(3/2)  =>  r = R (...)^(3/4)."""
    t = 1.0 - (2.0 * x / length) ** 2
    return r_max * max(t, 0.0) ** 0.75


def integrated_volume(length, r_max, n=200_000):
    h = length / n
    v = 0.0
    for i in range(n):
        x = -length / 2 + (i + 0.5) * h
        v += math.pi * sears_haack_radius(x, length, r_max) ** 2 * h
    return v


def check(g):
    ok = True
    for k, (want, tol) in PINNED.items():
        got = g[k]
        rel = abs(got - want) / want
        status = "ok " if rel < tol else "FAIL"
        ok &= rel < tol
        print(f"  {status} {k:14s} {got:10.4f}  pinned {want:<8}  rel {rel:.1e}")

    # The drawn profile must carry the volume the repo uses, not just look like a spindle.
    v_num = integrated_volume(g["length_m"], g["d_max_m"] / 2)
    rel = abs(v_num - g["volume_m3"]) / g["volume_m3"]
    ok &= rel < 1e-6
    print(f"  {'ok ' if rel < 1e-6 else 'FAIL'} profile volume {v_num:10.4f}  Sears-Haack {g['volume_m3']:.4f}  rel {rel:.1e}")

    # A pure delta of span b and root chord 2S/b has area exactly S.
    area = 0.5 * g["root_chord_m"] * g["span_m"]
    rel = abs(area - g["wing_area_m2"]) / g["wing_area_m2"]
    ok &= rel < 1e-12
    print(f"  {'ok ' if rel < 1e-12 else 'FAIL'} delta planform {area:10.4f}  S {g['wing_area_m2']:.4f}  rel {rel:.1e}")

    # The root chord has to fit inside the body length.
    fits = g["root_chord_m"] < g["length_m"]
    ok &= fits
    print(f"  {'ok ' if fits else 'FAIL'} root chord {g['root_chord_m']:.2f} m inside length {g['length_m']:.2f} m")

    cap = CAPTURE_OVER_BODY * g["a_max_m2"]
    print(f"       capture area {cap:.3f} m2 total, {cap / 2:.3f} m2 per inlet "
          f"({CAPTURE_OVER_BODY:.4f} of A_max, ventus-envelope)")
    return ok


def draw(g, path):
    import matplotlib

    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
    from matplotlib.patches import Polygon, Rectangle

    length, b, r_max = g["length_m"], g["span_m"], g["d_max_m"] / 2
    c_root = g["root_chord_m"]
    x_te = length / 2                      # DECLARED: wing TE flush with aft end of body
    x_apex = x_te - c_root
    sweep_le_deg = math.degrees(math.atan2(c_root, b / 2))
    cap_each = CAPTURE_OVER_BODY * g["a_max_m2"] / 2
    inlet_h = cap_each / INLET_WIDTH_M

    xs = [-length / 2 + i * length / 400 for i in range(401)]
    prof = [sears_haack_radius(x, length, r_max) for x in xs]

    fig, ax = plt.subplots(3, 1, figsize=(11, 13), gridspec_kw={"height_ratios": [1.0, 2.0, 1.2]})
    body, wing, nac, ink = "#cfcfcf", "#9aa7b5", "#6f7f90", "#222"

    # Side
    a = ax[0]
    a.fill_between(xs, [-r for r in prof], prof, color=body, ec=ink, lw=0.8)
    a.add_patch(Rectangle((x_apex, -WING_THICKNESS_M / 2), c_root, WING_THICKNESS_M, color=wing, ec=ink, lw=0.8))
    a.add_patch(Rectangle((LIP_X_M, -WING_THICKNESS_M / 2 - inlet_h), x_te - LIP_X_M, inlet_h, color=nac, ec=ink, lw=0.8))
    a.set_title("Side"); a.set_aspect("equal"); a.set_xlim(-length / 2 - 1, length / 2 + 1); a.grid(alpha=.25)

    # Top
    a = ax[1]
    a.fill_between(xs, [-r for r in prof], prof, color=body, ec=ink, lw=0.8, zorder=3)
    a.add_patch(Polygon([(x_apex, 0), (x_te, b / 2), (x_te, -b / 2)], closed=True, color=wing, ec=ink, lw=0.8, zorder=1, alpha=.85))
    for sgn in (1, -1):
        y0 = sgn * NACELLE_Y_M - INLET_WIDTH_M / 2
        a.add_patch(Rectangle((LIP_X_M, y0), x_te - LIP_X_M, INLET_WIDTH_M, fill=False, ec=nac, lw=1.4, ls="--", zorder=4))
    a.annotate("", xy=(x_te + .1, -b / 2 - .6), xytext=(-length / 2, -b / 2 - .6), arrowprops=dict(arrowstyle="<->"))
    a.text(0, -b / 2 - 1.3, f"L = {length:.2f} m", ha="center")
    a.annotate("", xy=(x_te + .8, b / 2), xytext=(x_te + .8, -b / 2), arrowprops=dict(arrowstyle="<->"))
    a.text(x_te + 1.1, 0, f"b = {b:.2f} m", rotation=90, va="center")
    a.set_title(f"Top: pure delta, S = {g['wing_area_m2']:.1f} m², root chord {c_root:.1f} m, LE sweep {sweep_le_deg:.0f}°")
    a.set_aspect("equal"); a.set_xlim(-length / 2 - 1, length / 2 + 2.5); a.set_ylim(-b / 2 - 2, b / 2 + 1); a.grid(alpha=.25)

    # Front
    a = ax[2]
    import numpy as np

    th = np.linspace(0, 2 * math.pi, 200)
    a.fill(r_max * np.cos(th), r_max * np.sin(th), color=body, ec=ink, lw=0.8, zorder=3)
    a.add_patch(Rectangle((-b / 2, -WING_THICKNESS_M / 2), b, WING_THICKNESS_M, color=wing, ec=ink, lw=0.8, zorder=1))
    for sgn in (1, -1):
        a.add_patch(Rectangle((sgn * NACELLE_Y_M - INLET_WIDTH_M / 2, -WING_THICKNESS_M / 2 - inlet_h),
                              INLET_WIDTH_M, inlet_h, color=nac, ec=ink, lw=0.8, zorder=2))
    a.set_title(f"Front: body Ø {2 * r_max:.2f} m; 2 inlets, {cap_each:.2f} m² each ({2 * cap_each:.2f} m² total = {CAPTURE_OVER_BODY:.3f} A_max)")
    a.set_aspect("equal"); a.grid(alpha=.25)

    fig.suptitle("VENTUS-1 baseline body (M 4.00) — derived dimensions; layout is declared, not designed", y=0.995)
    fig.text(0.5, 0.005,
             "Derived: S, b, L, Ø, Sears-Haack profile, delta root chord. Declared by this script: wing TE flush with body aft end, "
             "nacelles at y=±2.2 m below the wing, 2-D rectangular inlets.\nNot drawn (undefined in repo): fin, canopy, gear, nozzle shape, ramp geometry.",
             ha="center", fontsize=8)
    fig.tight_layout(rect=(0, 0.03, 1, 0.98))
    os.makedirs(os.path.dirname(path), exist_ok=True)
    fig.savefig(path, dpi=150)
    print(f"wrote {path}")


def main():
    g = derive()
    print("VENTUS-1 baseline body, re-derived:")
    if not check(g):
        print("REFUSED: the derivation no longer reproduces the Rust values; not drawing.")
        return 1
    if "--check" not in sys.argv:
        root = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
        draw(g, os.path.join(root, "out", "ventus_3view.png"))
    return 0


if __name__ == "__main__":
    sys.exit(main())
