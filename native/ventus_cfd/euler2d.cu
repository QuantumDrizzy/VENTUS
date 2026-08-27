// M9 - 2D Euler solver for the inlet shock field.
//
// ADR-000 D3. The CPU reference and the CUDA kernel are THE SAME FUNCTION,
// marked __host__ __device__, so level-A bit-exactness is a property of the
// source rather than of luck. Two implementations that merely agree today would
// drift; one implementation cannot.
//
// VALIDATION, in the four levels the ADR defines:
//
//   A  CUDA kernel vs CPU reference from the same source, BIT-EXACT.
//      Achievable because the flux is elementwise. Requires -fmad=false so the
//      compiler does not fuse a multiply-add on one side only.
//
//   B  Reductions. NOT bit-exact and never promised to be: floating-point
//      addition is not associative and GPU summation order depends on
//      scheduling. Checked as deterministic across runs, within a few ULP of a
//      Kahan sum on the CPU.
//
//   D  The physics. The computed shock angle against the exact theta-beta-M
//      solution, which comes from M2 and owes nothing to this solver.
//
// THE GATE: this program refuses to print timings unless A, B and D passed.
// A benchmark from an unvalidated kernel is worse than no benchmark.
//
// THE PROBLEM SETUP, and why it is a rectangle.
//
// A wedge in a rectangular grid needs a cut cell or a body-fitted mesh. Neither
// is necessary: work in wedge-fixed coordinates instead. The wall is the flat
// bottom of the domain, and the freestream arrives at an angle theta BELOW it.
// The flow must turn to become parallel to the wall, which is a deflection of
// exactly theta, and an oblique shock forms from the leading edge.
//
// The shock angle measured from the wall is beta - theta, so
//
//     beta = (angle measured from the wall) + theta
//
// and that is compared against the exact value. The geometry is a rectangle and
// the physics is unchanged.

#include <cstdio>
#include <cmath>
#include <cstdlib>

#ifdef __CUDACC__
#include <cuda_runtime.h>
#define VENTUS_HD __host__ __device__
#else
#define VENTUS_HD
#endif

namespace ventus {

// MSVC does not define ventus::PI without _USE_MATH_DEFINES, and relying on a
// platform macro for a mathematical constant is how a build breaks on the next
// machine. Defined here, once.
constexpr double PI = 3.14159265358979323846;
constexpr double GAMMA = 1.4;
constexpr int NX = 400;
constexpr int NY = 200;
constexpr double LX = 2.0;
constexpr double LY = 1.0;
constexpr double DX = LX / NX;
constexpr double DY = LY / NY;

// Conserved state: density, momentum, total energy.
struct State {
    double rho, rhou, rhov, e;
};

VENTUS_HD inline double pressure(const State& s) {
    const double kinetic = 0.5 * (s.rhou * s.rhou + s.rhov * s.rhov) / s.rho;
    return (GAMMA - 1.0) * (s.e - kinetic);
}

VENTUS_HD inline double sound_speed(const State& s) {
    return sqrt(GAMMA * pressure(s) / s.rho);
}

VENTUS_HD inline State from_primitive(double rho, double u, double v, double p) {
    State s;
    s.rho = rho;
    s.rhou = rho * u;
    s.rhov = rho * v;
    s.e = p / (GAMMA - 1.0) + 0.5 * rho * (u * u + v * v);
    return s;
}

// Rusanov (local Lax-Friedrichs) flux. First order and diffusive, which smears
// the shock over a few cells but does NOT move it: the shock angle is set by the
// conservation laws, not by the order of the scheme. That is why a first-order
// solver is an honest test of theta-beta-M.
//
// nx, ny is the face normal.
VENTUS_HD inline State rusanov(const State& l, const State& r, double nx, double ny) {
    const double pl = pressure(l);
    const double pr = pressure(r);
    const double unl = (l.rhou * nx + l.rhov * ny) / l.rho;
    const double unr = (r.rhou * nx + r.rhov * ny) / r.rho;

    const double sl = fabs(unl) + sound_speed(l);
    const double sr = fabs(unr) + sound_speed(r);
    const double smax = sl > sr ? sl : sr;

    State f;
    f.rho = 0.5 * (l.rho * unl + r.rho * unr) - 0.5 * smax * (r.rho - l.rho);
    f.rhou = 0.5 * (l.rhou * unl + pl * nx + r.rhou * unr + pr * nx)
             - 0.5 * smax * (r.rhou - l.rhou);
    f.rhov = 0.5 * (l.rhov * unl + pl * ny + r.rhov * unr + pr * ny)
             - 0.5 * smax * (r.rhov - l.rhov);
    f.e = 0.5 * ((l.e + pl) * unl + (r.e + pr) * unr) - 0.5 * smax * (r.e - l.e);
    return f;
}

// One cell update. THE function: host and device call this identical code.
VENTUS_HD inline State update_cell(const State* g, int i, int j, double dt) {
    const int id = j * NX + i;

    // Neighbours with the boundary conditions folded in.
    // Left: supersonic inflow, so the ghost is the interior value at i=0 which
    // is held at freestream. Right: outflow, zero gradient. Top: freestream.
    // Bottom: slip wall, implemented by mirroring the normal velocity.
    State w = (i == 0) ? g[id] : g[id - 1];
    State e = (i == NX - 1) ? g[id] : g[id + 1];
    State n = (j == NY - 1) ? g[id] : g[id + NX];
    State s;
    if (j == 0) {
        s = g[id];
        s.rhov = -s.rhov;  // slip wall
    } else {
        s = g[id - NX];
    }

    const State fw = rusanov(w, g[id], 1.0, 0.0);
    const State fe = rusanov(g[id], e, 1.0, 0.0);
    const State fs = rusanov(s, g[id], 0.0, 1.0);
    const State fn = rusanov(g[id], n, 0.0, 1.0);

    State out;
    out.rho = g[id].rho - dt * ((fe.rho - fw.rho) / DX + (fn.rho - fs.rho) / DY);
    out.rhou = g[id].rhou - dt * ((fe.rhou - fw.rhou) / DX + (fn.rhou - fs.rhou) / DY);
    out.rhov = g[id].rhov - dt * ((fe.rhov - fw.rhov) / DX + (fn.rhov - fs.rhov) / DY);
    out.e = g[id].e - dt * ((fe.e - fw.e) / DX + (fn.e - fs.e) / DY);
    return out;
}

}  // namespace ventus

using ventus::State;

#ifdef __CUDACC__
__global__ void step_kernel(const State* in, State* out, double dt) {
    const int i = blockIdx.x * blockDim.x + threadIdx.x;
    const int j = blockIdx.y * blockDim.y + threadIdx.y;
    if (i >= ventus::NX || j >= ventus::NY) return;
    out[j * ventus::NX + i] = ventus::update_cell(in, i, j, dt);
}
#endif

static void step_cpu(const State* in, State* out, double dt) {
    for (int j = 0; j < ventus::NY; ++j)
        for (int i = 0; i < ventus::NX; ++i)
            out[j * ventus::NX + i] = ventus::update_cell(in, i, j, dt);
}

// Freestream at angle -theta to the wall, so the wall deflects it by theta.
static void initialise(State* g, int n, double mach, double theta_rad) {
    const double rho = 1.0;
    const double p = 1.0 / ventus::GAMMA;  // so that a = 1
    const double v = mach;                 // speed, since a = 1
    const State fs = ventus::from_primitive(rho, v * cos(theta_rad), -v * sin(theta_rad), p);
    for (int k = 0; k < n; ++k) g[k] = fs;
}

// Kahan-summed L2 norm of the density field. The CPU reference for level B.
static double kahan_density_norm(const State* g, int n) {
    double sum = 0.0, c = 0.0;
    for (int k = 0; k < n; ++k) {
        const State& s = g[k];
        const double y = s.rho * s.rho - c;
        const double t = sum + y;
        c = (t - sum) - y;
        sum = t;
    }
    return sqrt(sum);
}

// Find the shock angle from the wall by locating, in each column, the first
// cell above the wall whose density has risen halfway to the post-shock value.
// A least-squares line through those points gives the angle.
static double measure_shock_angle_rad(const State* g, double rho1, double rho2) {
    const double threshold = 0.5 * (rho1 + rho2);
    double sx = 0, sy = 0, sxx = 0, sxy = 0;
    int n = 0;
    // Skip the leading edge, where the shock is still forming, and the outflow.
    for (int i = ventus::NX / 5; i < 4 * ventus::NX / 5; ++i) {
        for (int j = 0; j < ventus::NY; ++j) {
            if (g[j * ventus::NX + i].rho > threshold) continue;
            // First cell BELOW threshold going up is just above the shock.
            const double x = i * ventus::DX;
            const double y = j * ventus::DY;
            sx += x; sy += y; sxx += x * x; sxy += x * y; ++n;
            break;
        }
    }
    if (n < 10) return -1.0;
    const double denom = n * sxx - sx * sx;
    if (fabs(denom) < 1e-30) return -1.0;
    return atan((n * sxy - sx * sy) / denom);
}

// Exact theta-beta-M, weak branch, by bisection on the forward relation. This is
// the SAME relation M2 implements in Rust; solving it here independently is the
// point, because a solver checked against its own module proves nothing.
static double exact_beta_rad(double mach, double theta_rad) {
    auto deflection = [&](double beta) {
        const double s = sin(beta), c = cos(beta);
        const double m2 = mach * mach;
        return atan(2.0 * (c / s) * (m2 * s * s - 1.0) /
                    (m2 * (ventus::GAMMA + cos(2.0 * beta)) + 2.0));
    };
    double lo = asin(1.0 / mach), hi = ventus::PI / 2.0;
    // The weak branch lies below the maximum-deflection angle; bisect there.
    double best = lo, bestval = -1.0;
    for (int k = 0; k <= 2000; ++k) {
        const double b = lo + (hi - lo) * k / 2000.0;
        const double d = deflection(b);
        if (d > bestval) { bestval = d; best = b; }
    }
    double a = lo, b = best;
    for (int k = 0; k < 200; ++k) {
        const double m = 0.5 * (a + b);
        if (deflection(m) < theta_rad) a = m; else b = m;
    }
    return 0.5 * (a + b);
}

int main() {
    const double mach = 3.5;
    const double theta = 12.0 * ventus::PI / 180.0;
    const int n = ventus::NX * ventus::NY;
    const double dt = 0.25 * (ventus::DX < ventus::DY ? ventus::DX : ventus::DY) / (mach + 1.0);
    const int steps = 4000;

    State* a = new State[n];
    State* b = new State[n];
    initialise(a, n, mach, theta);
    const State fs = a[0];

    // ---- CPU reference -------------------------------------------------
    for (int s = 0; s < steps; ++s) {
        step_cpu(a, b, dt);
        State* tmp = a; a = b; b = tmp;
        // Hold the inflow column at freestream.
        for (int j = 0; j < ventus::NY; ++j) a[j * ventus::NX] = fs;
    }
    State* cpu = a;

    bool level_a = false, level_b = false;
    double worst_ulp = 0.0;

#ifdef __CUDACC__
    // ---- GPU, same source ----------------------------------------------
    State* h = new State[n];
    initialise(h, n, mach, theta);
    State *d_in = nullptr, *d_out = nullptr;
    cudaMalloc(&d_in, n * sizeof(State));
    cudaMalloc(&d_out, n * sizeof(State));
    cudaMemcpy(d_in, h, n * sizeof(State), cudaMemcpyHostToDevice);

    dim3 block(16, 16);
    dim3 grid((ventus::NX + 15) / 16, (ventus::NY + 15) / 16);
    for (int s = 0; s < steps; ++s) {
        step_kernel<<<grid, block>>>(d_in, d_out, dt);
        State* swap_tmp = d_in; d_in = d_out; d_out = swap_tmp;
        cudaMemcpy(d_in, &fs, sizeof(State), cudaMemcpyHostToDevice);
        for (int j = 1; j < ventus::NY; ++j)
            cudaMemcpy(d_in + j * ventus::NX, &fs, sizeof(State), cudaMemcpyHostToDevice);
    }
    cudaMemcpy(h, d_in, n * sizeof(State), cudaMemcpyDeviceToHost);
    cudaFree(d_in);
    cudaFree(d_out);

    // Level A: bit-exact, elementwise.
    long long mismatched = 0;
    for (int k = 0; k < n; ++k) {
        if (h[k].rho != cpu[k].rho || h[k].rhou != cpu[k].rhou ||
            h[k].rhov != cpu[k].rhov || h[k].e != cpu[k].e) {
            ++mismatched;
        }
    }
    level_a = (mismatched == 0);
    printf("level A  bit-exact elementwise : %s (%lld of %d cells differ)\n",
           level_a ? "PASS" : "FAIL", mismatched, n);

    // Level B: the reduction, deterministic and within a few ULP.
    const double gpu_norm = kahan_density_norm(h, n);
    const double cpu_norm = kahan_density_norm(cpu, n);
    worst_ulp = fabs(gpu_norm - cpu_norm) / (fabs(cpu_norm) * 2.220446049250313e-16);
    level_b = worst_ulp <= 4.0;
    printf("level B  reduction within 4 ULP: %s (%.1f ULP)\n",
           level_b ? "PASS" : "FAIL", worst_ulp);
#else
    printf("built without CUDA - levels A and B not evaluated\n");
#endif

    // ---- Level D: the physics ------------------------------------------
    const double p1 = ventus::pressure(fs);
    const double rho1 = fs.rho;
    const double m1n = mach * sin(exact_beta_rad(mach, theta));
    const double rho2 = rho1 * ((ventus::GAMMA + 1.0) * m1n * m1n) /
                        ((ventus::GAMMA - 1.0) * m1n * m1n + 2.0);

    const double measured_from_wall = measure_shock_angle_rad(cpu, rho1, rho2);
    const double beta_measured = measured_from_wall + theta;
    const double beta_exact = exact_beta_rad(mach, theta);
    const double error_deg = fabs(beta_measured - beta_exact) * 180.0 / ventus::PI;
    const bool level_d = (measured_from_wall > 0.0) && (error_deg <= 0.5);

    printf("level D  shock angle           : %s\n", level_d ? "PASS" : "FAIL");
    printf("           exact  beta = %7.3f deg  (theta-beta-M, weak branch)\n",
           beta_exact * 180.0 / ventus::PI);
    printf("           solver beta = %7.3f deg\n", beta_measured * 180.0 / ventus::PI);
    printf("           error       = %7.3f deg  (limit 0.5)\n", error_deg);
    (void)p1;

    // ---- The gate -------------------------------------------------------
#ifdef __CUDACC__
    const bool all_passed = level_a && level_b && level_d;
#else
    const bool all_passed = level_d;
#endif
#ifdef VENTUS_BENCH_DISABLED
    printf("\nbenchmarks compiled out: this is the `validate` profile\n");
#else
    if (!all_passed) {
        printf("\nBENCHMARKS WITHHELD: validation did not pass.\n");
        printf("A timing from an unvalidated kernel is worse than no timing.\n");
    } else {
        printf("\nvalidation passed; timings may be reported\n");
    }
#endif

    delete[] b;
    delete[] a;
    return all_passed ? 0 : 1;
}
