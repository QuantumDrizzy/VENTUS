#!/usr/bin/env python3
"""VENTUS-1 baseline (M 4.00): skin-material zones on the planform, and the four-ramp inlet section.

Companion to ventus_3view.py. Shares no code with the crates (ADR-000 D8). Everything drawn is either
re-derived here and checked against the numbers the Rust code / the corpus pin, or an input marked as one.

  Figure 1, skin zones     wall temperature from skin_crosscheck.py (independent flat-plate radiative
                           equilibrium, M 4.00 design-q row), mapped onto the 3-view planform by
                           streamwise distance behind the nearest leading edge. The 623 K crossover
                           (Ti-6Al-4V limit) is solved here and checked against ADR-008 amendment 1 (2.41 m).
                           Nose 867.3 K / leading edge 876.2 K are Fay-Riddell values pinned from
                           ventus-thermal (inputs, not re-derived).

  Figure 2, inlet section  four weak oblique shocks from the ramp angles ventus-inlet `optimise_ramps`
                           returns at M 4.00 (inputs). This script re-solves theta-beta-M and the shock
                           recovery itself and refuses to draw unless wave angles, Mach numbers and the
                           total-pressure recovery reproduce the Rust values. Ramps are laid out
                           shock-on-lip (all four shocks meet the cowl lip), scaled so the capture height
                           equals the per-inlet height of ventus_3view.py.

[KNOWN_LIMIT] Zones: flat plate, local radiative equilibrium, no conduction, sink 0 K (ADR-008 amendment 1);
the plate model is not valid inside the nose / LE radius, which is why those are the Fay-Riddell numbers.
Inlet: external compression only. The internal contraction, throat, bleed, boundary layer and unstart
are not modelled (ventus-inlet refuses them) and are not drawn. The streamwise rule is applied to the
fuselage forward of the wing with the nose tip as origin; that extension is this script's, not the corpus's.

    python analysis/ventus_zones_inlet.py           # writes out/ventus_zones.png, out/ventus_inlet.png
    python analysis/ventus_zones_inlet.py --check
"""

import math
import os
import sys

from scipy.optimize import brentq

import skin_crosscheck as skin
import ventus_3view as g3

H_GEO_M4 = 27747.25646305947        # M 4.00 design-q row, geopotential m (ADR-008)
MACH = 4.0
GAMMA = 1.4
T_LIMIT_TI64 = 623.0                 # Ti-6Al-4V, corpus limit [TO CITE]
T_LIMIT_TI6242 = 813.0               # Ti-6242S
T_NOSE_K, T_LE_K = 867.3, 876.2      # Fay-Riddell, ventus-thermal (inputs)
CROSSOVER_ADR_M = 2.41               # ADR-008 amendment 1

# ventus-inlet optimise_ramps(M 4.00, 4 ramps), printed by the Rust code (inputs, 2 dp)
RAMP_DEG = [9.67, 11.56, 14.02, 16.91]
RUST_WAVE_DEG = [21.94, 26.85, 34.07, 46.80]
RUST_MACH_AFTER = [3.309, 2.668, 2.050, 1.411]
RUST_RECOVERY = 0.7195


# ---------------------------------------------------------------- oblique shocks (own implementation)
def theta_of_beta(m, beta):
    num = 2.0 / math.tan(beta) * (m * m * math.sin(beta) ** 2 - 1.0)
    den = m * m * (GAMMA + math.cos(2 * beta)) + 2.0
    return math.atan(num / den)


def weak_beta(m, theta):
    mu = math.asin(1.0 / m)
    betas = [mu + (math.pi / 2 - mu) * i / 20000 for i in range(20001)]
    b_max = max(betas, key=lambda b: theta_of_beta(m, b))
    if theta > theta_of_beta(m, b_max):
        raise ValueError("detached")
    return brentq(lambda b: theta_of_beta(m, b) - theta, mu + 1e-12, b_max)


def normal_shock(m1):
    g = GAMMA
    p0 = ((g + 1) * m1 * m1 / ((g - 1) * m1 * m1 + 2)) ** (g / (g - 1)) * \
         ((g + 1) / (2 * g * m1 * m1 - (g - 1))) ** (1 / (g - 1))
    m2 = math.sqrt(((g - 1) * m1 * m1 + 2) / (2 * g * m1 * m1 - (g - 1)))
    return m2, p0


def shock_train(m, ramps_deg):
    out, rec, m_cur = [], 1.0, m
    for d in ramps_deg:
        th = math.radians(d)
        b = weak_beta(m_cur, th)
        m2n, p0 = normal_shock(m_cur * math.sin(b))
        m_next = m2n / math.sin(b - th)
        out.append({"beta": b, "m_in": m_cur, "m_out": m_next, "theta": th})
        rec *= p0
        m_cur = m_next
    m_t, p0_t = normal_shock(m_cur)
    return out, rec * p0_t, m_cur


def check_inlet():
    ok = True
    st, rec, m_term = shock_train(MACH, RAMP_DEG)
    for i, s in enumerate(st):
        for name, got, want, tol in (("wave angle", math.degrees(s["beta"]), RUST_WAVE_DEG[i], 0.02),
                                     ("Mach after", s["m_out"], RUST_MACH_AFTER[i], 5e-3)):
            good = abs(got - want) < tol
            ok &= good
            print(f"  {'ok ' if good else 'FAIL'} ramp {i + 1} {name:10s} {got:8.3f}  Rust {want}")
    good = abs(rec - RUST_RECOVERY) < 5e-4
    ok &= good
    print(f"  {'ok ' if good else 'FAIL'} total recovery  {rec:8.4f}  Rust {RUST_RECOVERY}")
    # Oswatitsch, as the Rust test states it: the OBLIQUE shocks of the optimum are equal strength. The
    # terminal normal shock is weaker at finite strength (reported, not asserted).
    mn = [s["m_in"] * math.sin(s["beta"]) for s in st]
    spread = max(mn) - min(mn)
    good = spread < 0.02
    ok &= good
    print(f"  {'ok ' if good else 'FAIL'} equal normal Mach, oblique shocks {[round(x, 3) for x in mn]}; "
          f"terminal normal shock M {m_term:.3f}")
    return ok, st, rec, m_term


# ---------------------------------------------------------------- skin zones
def wall_k(x):
    return skin.skin(H_GEO_M4, MACH, x)["wall_temperature_k"]


def check_zones():
    ok = True
    x_cross = brentq(lambda x: wall_k(x) - T_LIMIT_TI64, 1.0, 10.0, xtol=1e-9)
    good = abs(x_cross - CROSSOVER_ADR_M) < 0.01
    ok &= good
    print(f"  {'ok ' if good else 'FAIL'} Ti-6Al-4V crossover {x_cross:.3f} m  ADR-008 A1 {CROSSOVER_ADR_M}")
    for x, want in ((1.0, 638.8), (10.0, 597.1)):
        good = abs(wall_k(x) - want) < 0.1
        ok &= good
        print(f"  {'ok ' if good else 'FAIL'} T_wall({x:>4} m) {wall_k(x):.2f} K  corpus {want}")
    print(f"       T_wall(2.41 m) {wall_k(2.41):.1f} K; Ti-6242S limit {T_LIMIT_TI6242:.0f} K; "
          f"margin at 1 m {T_LIMIT_TI6242 - wall_k(1.0):.0f} K")
    return ok, x_cross


# ---------------------------------------------------------------- shock-on-lip ramp layout
def ramp_layout(st, capture_height_m):
    betas = [s["beta"] for s in st]
    thetas = [s["theta"] for s in st]
    phi = [0.0]
    for t in thetas:
        phi.append(phi[-1] + t)
    # unit design: first shock from the ramp tip C0 = (0,0) fixes the lip P
    p = (math.cos(betas[0]), math.sin(betas[0]))
    corners = [(0.0, 0.0)]
    for i in range(1, len(st)):
        cx, cy = corners[-1]
        ux, uy = math.cos(phi[i]), math.sin(phi[i])           # ramp i surface direction
        dx, dy = math.cos(phi[i] + betas[i]), math.sin(phi[i] + betas[i])   # shock i+1 direction
        # corner = C + s u such that (P - corner) x d = 0
        a = (p[0] - cx) * dy - (p[1] - cy) * dx
        b = ux * dy - uy * dx
        s = a / b
        corners.append((cx + s * ux, cy + s * uy))
    k = capture_height_m / p[1]
    corners = [(x * k, y * k) for x, y in corners]
    p = (p[0] * k, p[1] * k)
    cx, cy = corners[-1]
    u = (p[0] - cx) / math.cos(phi[-1])
    end = (p[0], cy + u * math.sin(phi[-1]))
    return corners, p, end, phi


def draw_inlet(st, path, inlet_h):
    import matplotlib

    matplotlib.use("Agg")
    import matplotlib.pyplot as plt

    corners, p, end, phi = ramp_layout(st, inlet_h)
    fig, ax = plt.subplots(figsize=(11, 5.5))
    pts = corners + [end]
    ax.plot([c[0] for c in pts], [c[1] for c in pts], color="#222", lw=2.5, label="compression surface")
    ax.fill_between([c[0] for c in pts], [c[1] for c in pts], -0.3, color="#6f7f90", alpha=.5)
    for i, c in enumerate(corners):
        ax.plot([c[0], p[0]], [c[1], p[1]], color="#d9480f", lw=1.2)
        beta = math.degrees(st[i]["beta"])
        ax.text((c[0] + p[0]) / 2 - 0.05, (c[1] + p[1]) / 2 + 0.03,
                f"shock {i + 1}: M {st[i]['m_in']:.2f}→{st[i]['m_out']:.2f}", fontsize=8, color="#d9480f", ha="right")
    ax.plot([p[0], p[0] + 0.6], [p[1], p[1]], color="#222", lw=3)
    ax.text(p[0] + 0.62, p[1], "cowl lip\n(shock-on-lip)", va="center", fontsize=8)
    ax.annotate("", xy=(corners[0][0] - 0.1, p[1]), xytext=(corners[0][0] - 0.1, 0), arrowprops=dict(arrowstyle="<->"))
    ax.text(-0.15, p[1] / 2, f"capture\n{p[1]:.2f} m", ha="right", va="center", fontsize=8)
    ax.annotate("", xy=(end[0] + .15, end[1]), xytext=(end[0] + .15, 0), arrowprops=dict(arrowstyle="<->"))
    ax.text(end[0] + .25, end[1] / 2, f"ramp rise\nat lip: {end[1]:.2f} m", va="center", fontsize=8)
    for i, c in enumerate(corners):
        ax.text(c[0], c[1] - 0.12, f"{math.degrees(st[i]['theta']):.2f}°", fontsize=8, ha="left")
    ax.arrow(-1.0, 0.25, 0.8, 0, head_width=0.05, color="k")
    ax.text(-1.0, 0.33, "M 4.00", fontsize=9)
    total_deg = math.degrees(phi[-1])
    ax.set_aspect("equal"); ax.grid(alpha=.25); ax.set_xlim(-1.3, p[0] + 2.0); ax.set_ylim(-0.3, max(p[1], end[1]) + 0.4)
    ax.set_title(f"VENTUS-1 four-ramp external compression, M 4.00: total turning {total_deg:.1f}°, "
                 f"recovery {RUST_RECOVERY} (ventus-inlet)")
    fig.text(0.5, 0.01,
             "Re-solved here: wave angles, Mach, recovery (checked against Rust). Layout is shock-on-lip, scaled to the per-inlet capture height. "
             "Not modelled/drawn: internal contraction, throat, bleed, unstart.", ha="center", fontsize=7.5)
    fig.tight_layout(rect=(0, 0.03, 1, 1))
    fig.savefig(path, dpi=150)
    print(f"wrote {path}")
    return end, p, corners


def draw_zones(g, x_cross, path):
    import matplotlib

    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
    import numpy as np
    from matplotlib.colors import ListedColormap
    from matplotlib.patches import Polygon

    length, b, r_max, c_root = g["length_m"], g["span_m"], g["d_max_m"] / 2, g["root_chord_m"]
    x_te = length / 2
    x_apex = x_te - c_root
    slope = c_root / (b / 2)

    xs_tab = np.geomspace(0.1, 25.0, 60)
    t_tab = np.array([wall_k(x) for x in xs_tab])

    nx, ny = 700, 360
    X, Y = np.meshgrid(np.linspace(-length / 2, x_te, nx), np.linspace(-b / 2, b / 2, ny))
    body_r = np.array([g3.sears_haack_radius(x, length, r_max) for x in X[0]])
    on_body = np.abs(Y) <= body_r[None, :]
    le_x = x_apex + np.abs(Y) * slope
    on_wing = (X >= le_x) & (X <= x_te)
    dist = np.where(on_body, X + length / 2, X - le_x)
    dist = np.where(on_body & on_wing, np.minimum(dist, X - le_x + 1e9), dist)
    dist = np.clip(dist, 0.1, 25.0)
    T = np.interp(dist, xs_tab, t_tab)
    mask = on_body | on_wing
    # material zones: 0 Ti-6Al-4V, 1 Ti-6242S (T above the Ti-6Al-4V limit)
    Z = np.where(T > T_LIMIT_TI64, 1, 0).astype(float)
    Z = np.where(mask, Z, np.nan)

    fig, ax = plt.subplots(figsize=(11, 6.4))
    ax.imshow(Z, extent=(-length / 2, x_te, -b / 2, b / 2), origin="lower",
              cmap=ListedColormap(["#9bb5c9", "#e0a458"]), vmin=0, vmax=1, aspect="equal", interpolation="nearest")
    xs = np.linspace(-length / 2, x_te, 400)
    r = [g3.sears_haack_radius(x, length, r_max) for x in xs]
    ax.plot(xs, r, color="#222", lw=0.8); ax.plot(xs, [-v for v in r], color="#222", lw=0.8)
    ax.add_patch(Polygon([(x_apex, 0), (x_te, b / 2), (x_te, -b / 2)], closed=True, fill=False, ec="#222", lw=0.8))
    for sgn in (1, -1):    # leading edges, Inconel 718
        ax.plot([x_apex, x_te], [0, sgn * b / 2], color="#7a1f1f", lw=3)
    ax.plot([-length / 2], [0], marker="o", ms=9, color="#7a1f1f")
    ax.annotate(f"Inconel 718 nose\nFay-Riddell {T_NOSE_K:.0f} K ({T_NOSE_K - 273.15:.0f} °C)", xy=(-length / 2, 0),
                xytext=(-length / 2 + 0.4, -3.6), fontsize=8, arrowprops=dict(arrowstyle="-"))
    ax.annotate(f"Inconel 718 leading edges\nFay-Riddell {T_LE_K:.0f} K ({T_LE_K - 273.15:.0f} °C)", xy=(x_apex + 3.5, 3.5 / slope * 0 + 3.5 / slope * 1.0 * 0 + (3.5) / slope),
                xytext=(x_apex - 5.5, 5.4), fontsize=8, arrowprops=dict(arrowstyle="-"))
    from matplotlib.patches import Patch
    ax.legend(handles=[
        Patch(color="#e0a458", label=f"Ti-6242S: ≤ {x_cross:.2f} m behind a leading edge (T_wall {wall_k(1.0):.0f} K at 1 m)"),
        Patch(color="#9bb5c9", label=f"Ti-6Al-4V: aft of it ({wall_k(10.0):.0f} K at 10 m)"),
        Patch(color="#7a1f1f", label="Inconel 718: nose and leading edges")], loc="lower left", fontsize=8)
    ax.text(-length / 2 + 2.0, 1.35, f"fuselage: same rule from the nose\n(Ti-6242S ≤ {x_cross:.2f} m) — script extension", fontsize=7.5)
    ax.set_xlabel("m"); ax.set_ylabel("m")
    ax.set_title("VENTUS-1 baseline skin materials, M 4.00 @ 27.7 km (radiative equilibrium, ε 0.85)")
    fig.text(0.5, 0.01,
             "Wall T: skin_crosscheck.py (independent of the crates). Zone limits Ti-6Al-4V 623 K / Ti-6242S 813 K are [TO CITE] in the corpus.\n"
             "Flat plate, no conduction, sink 0 K; nose/LE use Fay-Riddell at declared R_n 25 mm / R_LE 10 mm [TO DETERMINE].",
             ha="center", fontsize=7.2)
    fig.tight_layout(rect=(0, 0.05, 1, 1))
    fig.savefig(path, dpi=150)
    print(f"wrote {path}")


def main():
    g = g3.derive()
    inlet_h = (g3.CAPTURE_OVER_BODY * g["a_max_m2"] / 2) / g3.INLET_WIDTH_M
    print("inlet (M 4.00, four ramps), re-solved:")
    ok1, st, rec, m_term = check_inlet()
    print("skin zones (M 4.00 design-q row):")
    ok2, x_cross = check_zones()
    if not (ok1 and ok2):
        print("REFUSED: a check failed; not drawing.")
        return 1
    if "--check" not in sys.argv:
        out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "out")
        os.makedirs(out, exist_ok=True)
        end, p, corners = draw_inlet(st, os.path.join(out, "ventus_inlet.png"), inlet_h)
        print(f"  layout: capture height {p[1]:.3f} m, compression length to lip {p[0]:.3f} m, "
              f"ramp surface rise at lip {end[1]:.3f} m, ramp corners {[(round(x, 2), round(y, 2)) for x, y in corners]}")
        draw_zones(g, x_cross, os.path.join(out, "ventus_zones.png"))
    return 0


if __name__ == "__main__":
    sys.exit(main())
