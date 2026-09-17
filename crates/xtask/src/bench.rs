//! `cargo xtask bench` — timings, gated on correctness.
//!
//! # The gate, which is the whole point
//!
//! ADR-000 D3 and the M9 solver already carry this rule: **a benchmark from
//! unvalidated code is worse than no benchmark**, because it is a number that
//! looks like evidence and is not. `euler2d.cu` refuses to print timings unless
//! levels A, B and D passed in the same binary.
//!
//! This applies the same rule to the Rust side. `bench` runs the lint gate and
//! the full case corpus first, and **refuses to time anything** unless the
//! verdict is PASS at the same commit. The report carries `run_id`, the commit
//! hash and the clean/dirty state, exactly as the validation report does, and a
//! dirty tree is labelled not reproducible.
//!
//! # What is measured, and why these
//!
//! Not everything: the things that are actually slow, and the things that run
//! somewhere the speed matters.
//!
//! - **M3 `optimise_ramps`** — measured at roughly a quarter of a second for the
//!   four-ramp design inlet. It dominates M12 so completely that the regime
//!   sweep had to be restructured around it (one pass instead of five scans).
//!   The single worst hot spot in the workspace.
//! - **M1 `at_geopotential`** — `no_std`, no allocation, and the one function
//!   that runs inside the flight software. Its cost is a budget, not a curiosity.
//! - **M5 `radiation_equilibrium_wall`** — a fixed-iteration bisection, so its
//!   cost should be flat in the input. Worth confirming rather than assuming.
//! - **M4 `ideal_ramjet`** — the cycle, called once per sweep point.
//! - **M12 `evaluate`** — the whole chain at one Mach, to show what the parts
//!   add up to and how much of it is M3.
//!
//! # Method, stated so the numbers can be read
//!
//! Wall clock, `std::time::Instant`, single thread. Each subject is run for a
//! fixed number of warm-up iterations that are discarded, then timed in batches;
//! the reported figure is the **median** of the batch means, with the minimum
//! also given because it is the least contaminated by scheduling.
//!
//! No statistical machinery beyond that, and no `criterion`: this workspace has
//! four dependencies total and a benchmark harness is not worth a fifth. The
//! consequence is that these numbers are good to a few per cent, not to one, and
//! nothing here leans on a difference smaller than that.
//!
//! **Build profile matters and is recorded.** A debug build is roughly twenty
//! times slower on this workload, which is why `bench` refuses to run unless it
//! was itself compiled with optimisations.

use std::path::Path;
use std::time::{Duration, Instant};

/// Iterations discarded before timing starts.
const WARMUP: usize = 3;
/// Batches timed; the reported figure is the median of their means.
const BATCHES: usize = 7;

/// One measured subject.
pub struct Measurement {
    pub module: &'static str,
    pub subject: &'static str,
    /// Median of the batch means.
    pub median: Duration,
    /// Fastest single batch mean, least contaminated by scheduling.
    pub best: Duration,
    /// Iterations per batch, so a reader can see what was averaged.
    pub iterations: usize,
    /// What the number is useful for, in one line.
    pub note: &'static str,
}

impl Measurement {
    fn spread_percent(&self) -> f64 {
        if self.best.as_secs_f64() == 0.0 {
            return 0.0;
        }
        100.0 * (self.median.as_secs_f64() / self.best.as_secs_f64() - 1.0)
    }
}

/// Time `f` with warm-up and batching.
fn measure(
    module: &'static str,
    subject: &'static str,
    iterations: usize,
    note: &'static str,
    mut f: impl FnMut(),
) -> Measurement {
    for _ in 0..WARMUP {
        for _ in 0..iterations {
            f();
        }
    }
    let mut means: Vec<Duration> = Vec::with_capacity(BATCHES);
    for _ in 0..BATCHES {
        let start = Instant::now();
        for _ in 0..iterations {
            f();
        }
        means.push(start.elapsed() / iterations as u32);
    }
    means.sort_unstable();
    Measurement {
        module,
        subject,
        median: means[BATCHES / 2],
        best: means[0],
        iterations,
        note,
    }
}

/// Everything timed, in the order the chain runs.
///
/// `std::hint::black_box` on every result: without it the optimiser is entitled
/// to delete a pure call whose value is discarded, and the benchmark would
/// report the cost of an empty loop.
pub fn run_all() -> Vec<Measurement> {
    use std::hint::black_box;

    let design_altitude = 26_000.0_f64;
    let atmos = ventus_atmos::at_geopotential(design_altitude).unwrap();
    let velocity = 3.5 * atmos.speed_of_sound_m_s;
    let edge = ventus_aero::boundary_layer::EdgeState {
        temperature_k: atmos.temperature_k,
        pressure_pa: atmos.pressure_pa,
        velocity_m_s: velocity,
        mach: 3.5,
        gamma: 1.4,
    };

    vec![
        measure(
            "M1",
            "at_geopotential (design altitude)",
            100_000,
            "runs inside the flight software; this is a budget, not a curiosity",
            || {
                black_box(ventus_atmos::at_geopotential(black_box(design_altitude)).unwrap());
            },
        ),
        measure(
            "M2",
            "stagnation_temperature_ratio",
            1_000_000,
            "a closed form; included as the floor the others are measured against",
            || {
                black_box(
                    ventus_gasdyn::stagnation_temperature_ratio(black_box(3.5), 1.4).unwrap(),
                );
            },
        ),
        measure(
            "M5",
            "radiation_equilibrium_wall (10 m station)",
            2_000,
            "fixed-iteration bisection, so the cost should be flat in the input",
            || {
                black_box(
                    ventus_thermal::radiation_equilibrium_wall(
                        black_box(&edge),
                        10.0,
                        0.85,
                        0.0,
                        ventus_aero::boundary_layer::PRANDTL_AIR,
                        ventus_aero::boundary_layer::Regime::Turbulent,
                    )
                    .unwrap(),
                );
            },
        ),
        measure(
            "M4",
            "ideal_ramjet (design point)",
            20_000,
            "the cycle, called once per sweep point",
            || {
                black_box(
                    ventus_propulsion::ramjet::ideal_ramjet(
                        black_box(3.5),
                        atmos.temperature_k,
                        atmos.pressure_pa,
                        velocity,
                        0.742,
                        1700.0,
                        1.4,
                    )
                    .unwrap(),
                );
            },
        ),
        measure(
            "M3",
            "optimise_ramps, 2 ramps",
            20,
            "shown alongside 4 to make the scaling in ramp count visible",
            || {
                black_box(
                    ventus_inlet::shock_train::optimise_ramps(black_box(3.5), 2, 1.4).unwrap(),
                );
            },
        ),
        measure(
            "M3",
            "optimise_ramps, 4 ramps (design inlet)",
            10,
            "THE HOT SPOT: dominates M12 and forced the sweep to be restructured",
            || {
                black_box(
                    ventus_inlet::shock_train::optimise_ramps(black_box(3.5), 4, 1.4).unwrap(),
                );
            },
        ),
        measure(
            "M12",
            "evaluate (whole chain, one Mach)",
            10,
            "what the parts add up to; compare against the M3 row above",
            || {
                black_box(ventus_envelope::evaluate(black_box(3.5)));
            },
        ),
    ]
}

/// The benchmark report, in the shape of the validation report.
pub fn markdown(provenance: &ventus_validate::report::Provenance, m: &[Measurement]) -> String {
    use std::fmt::Write;
    let mut out = String::new();

    let _ = writeln!(out, "# VENTUS benchmark report\n");
    let _ = writeln!(out, "| | |");
    let _ = writeln!(out, "|---|---|");
    let _ = writeln!(out, "| run_id | `{}` |", provenance.run_id);
    let _ = writeln!(out, "| commit | `{}` |", provenance.git_hash);
    let _ = writeln!(
        out,
        "| tree | {} |",
        if provenance.git_dirty {
            "**DIRTY - not reproducible**"
        } else {
            "clean"
        }
    );
    let _ = writeln!(out, "| rustc | {} |", provenance.rustc);
    let _ = writeln!(out, "| profile | release |");
    let _ = writeln!(out, "| method | median of {BATCHES} batch means, {WARMUP} warm-up batches discarded, single thread |\n");

    let _ = writeln!(
        out,
        "**Gate: these timings exist only because `xtask validate` reported PASS \
         at this same commit.** A benchmark from unvalidated code is a number that \
         looks like evidence and is not (ADR-000 D3).\n"
    );

    let _ = writeln!(
        out,
        "| module | subject | median | best | spread | iters/batch |"
    );
    let _ = writeln!(out, "|---|---|---|---|---|---|");
    for x in m {
        let _ = writeln!(
            out,
            "| {} | `{}` | **{:.3?}** | {:.3?} | +{:.1} % | {} |",
            x.module,
            x.subject,
            x.median,
            x.best,
            x.spread_percent(),
            x.iterations
        );
    }
    let _ = writeln!(out);

    let _ = writeln!(out, "## What each is for\n");
    for x in m {
        let _ = writeln!(out, "- **{}** `{}` — {}", x.module, x.subject, x.note);
    }

    let _ = writeln!(
        out,
        "\n## Reading these honestly\n\n\
         Wall clock, single thread, no `criterion`. Good to a few per cent, not to \
         one; nothing here leans on a smaller difference. The `spread` column is \
         median over best, so a large value means the measurement was contaminated \
         by scheduling rather than that the code is variable.\n\n\
         **Baselines are stated, never implied.** Each figure is this workspace \
         against itself on one machine, in release. It is not a comparison against \
         any other implementation, and no claim of the form \"N times faster than X\" \
         appears anywhere, because nothing here has been measured against an X."
    );
    out
}

/// Write the report next to the validation artifacts.
///
/// # Errors
/// Any filesystem failure writing the report.
pub fn write_report(
    out_dir: &Path,
    provenance: &ventus_validate::report::Provenance,
    m: &[Measurement],
) -> std::io::Result<()> {
    std::fs::create_dir_all(out_dir)?;
    std::fs::write(out_dir.join("bench.md"), markdown(provenance, m))?;

    let mut csv = String::from("module,subject,median_ns,best_ns,iterations\n");
    for x in m {
        csv.push_str(&format!(
            "{},{},{},{},{}\n",
            x.module,
            x.subject,
            x.median.as_nanos(),
            x.best.as_nanos(),
            x.iterations
        ));
    }
    std::fs::write(out_dir.join("bench.csv"), csv)?;
    Ok(())
}
