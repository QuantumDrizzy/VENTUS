//! xtask — the single entry point. No Make, no .bat (ADR-000 D6).
//!
//!   cargo xtask validate    run every module against its cases -> out/<run_id>/
//!   cargo xtask build       rust + native; detects vcvars, fails with the fix
//!   cargo xtask check-dag   the declared edge set (D5); cargo enforces acyclicity
//!   cargo xtask bench m9    GATED on validate passing at the same commit (D3)
//!   cargo xtask report      design point tables + plots (invokes analysis/)
//!
//! vcvars note for `build`: if VSCMD_ARG_TGT_ARCH != x64, abort with the exact
//! remedy. The Git coreutils `link` shadows the MSVC linker, so a missing
//! vcvars fails forty lines later inside the wrong link.exe instead of here.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use ventus_aero::boundary_layer::{self as bl, EdgeState, Regime, PRANDTL_AIR};
use ventus_gasdyn as gasdyn;
use ventus_validate::case::{self, Case, ExpectValue};
use ventus_validate::check::{check, Outcome, Summary};
use ventus_validate::report::{self, Entry, Provenance};

fn main() -> ExitCode {
    let cmd = std::env::args().nth(1);
    match cmd.as_deref() {
        Some("validate") => validate(),
        Some(other) => {
            eprintln!("xtask: `{other}` is not implemented yet.");
            eprintln!("available: validate");
            eprintln!("planned:   build | check-dag | bench | report  (ADR-000 D6)");
            ExitCode::from(2)
        }
        None => {
            eprintln!("usage: cargo xtask <validate>");
            ExitCode::from(2)
        }
    }
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crates/xtask is two levels below the workspace root")
        .to_path_buf()
}

/// Which modules can answer cases today.
///
/// A module with cases but no evaluator would report every case as FAIL with
/// missing keys, which is indistinguishable from a broken module. So an unwired
/// module is listed here explicitly and its cases are reported as PENDING rather
/// than silently red. Each entry moves from `pending` to `wired` when its module
/// lands.
const PENDING_MODULES: &[&str] = &["ventus-mass", "ventus-dynamics", "ventus-fsw"];

/// The floor under the wired corpus.
///
/// `case::load_dir` returns an empty list for a missing directory rather than an
/// error, and its doc comment claimed the runner caught that. It did not: the
/// verdict is computed from failures, and zero cases produce zero failures, so a
/// corpus that had vanished reported `0 pass, 0 fail` and exited 0. A harness
/// that passes loudest when it is checking nothing is worse than no harness.
///
/// It found this the honest way. The repository was moved on disk; `repo_root`
/// is built from `env!("CARGO_MANIFEST_DIR")`, which is baked in at compile time,
/// and cargo did not rebuild because no source had changed. The stale binary
/// looked for cases at the old absolute path. That failure surfaced only because
/// reading `crates/` itself errored — had one module's `cases/` gone missing
/// instead, this would have reported PASS over a silently smaller corpus.
///
/// This is a FLOOR, not the count. It exists to catch a corpus that disappeared,
/// not to track every case added, so it is deliberately not `== 64`: pinning the
/// exact number would turn every new case into a two-line edit and the constant
/// would be updated reflexively, which is how a guard stops guarding.
const MINIMUM_CORPUS: usize = 50;

/// M1. Every field of the state is exposed; the harness ignores what a case
/// does not name.
fn evaluate_atmos(c: &Case) -> BTreeMap<String, ExpectValue> {
    let state = if let Some(h) = c.inputs.get("geopotential_altitude_m") {
        ventus_atmos::at_geopotential(
            h.as_float()
                .expect("geopotential_altitude_m must be a float"),
        )
    } else if let Some(z) = c.inputs.get("geometric_altitude_m") {
        ventus_atmos::at_geometric(z.as_float().expect("geometric_altitude_m must be a float"))
    } else {
        panic!("case `{}` names no altitude input", c.name);
    }
    .unwrap_or_else(|e| panic!("case `{}`: {e:?}", c.name));

    let mut m = BTreeMap::new();
    for (k, v) in [
        ("geopotential_altitude_m", state.geopotential_altitude_m),
        ("geometric_altitude_m", state.geometric_altitude_m),
        ("temperature_k", state.temperature_k),
        ("pressure_pa", state.pressure_pa),
        ("density_kg_m3", state.density_kg_m3),
        ("speed_of_sound_m_s", state.speed_of_sound_m_s),
        ("dynamic_viscosity_pa_s", state.dynamic_viscosity_pa_s),
        ("kinematic_viscosity_m2_s", state.kinematic_viscosity_m2_s()),
        ("lapse_rate_k_per_m", state.lapse_rate_k_per_m),
    ] {
        m.insert(k.to_string(), ExpectValue::Float(v));
    }
    m
}

/// M2. Emits whatever the case inputs make computable; the harness ignores keys
/// a case does not name, and a key a relation refuses to produce is simply
/// absent, which the harness reports as "never computed" rather than as a wrong
/// number. That is how `gamma_at_design_point_freestream` shows up as a known
/// limit instead of silently extrapolating a curve fit.
fn evaluate_gasdyn(c: &Case) -> BTreeMap<String, ExpectValue> {
    let mut m = BTreeMap::new();
    let mut put = |k: &str, v: f64| {
        m.insert(k.to_string(), ExpectValue::Float(v));
    };
    let f = |k: &str| c.inputs.get(k).and_then(toml::Value::as_float);

    let gamma = f("gamma").unwrap_or(1.4);

    if let Some(t) = f("temperature_k") {
        if let Ok(g) = gasdyn::gamma_air(t) {
            put("gamma", g);
        }
        if let Ok(cp) = gasdyn::specific_heat_air_j_kg_k(t) {
            put("specific_heat_j_kg_k", cp);
        }
    }

    let Some(mach) = f("mach") else { return m };

    if let Ok(r) = gasdyn::stagnation_temperature_ratio(mach, gamma) {
        put("stagnation_temperature_ratio", r);
        if let Some(t1) = f("static_temperature_k") {
            put("stagnation_temperature_k", t1 * r);
        }
    }
    if let Ok(r) = gasdyn::stagnation_pressure_ratio(mach, gamma) {
        put("stagnation_pressure_ratio_isentropic", r);
    }
    if let Ok(r) = gasdyn::area_ratio(mach, gamma) {
        put("area_ratio", r);
    }
    if let Ok(s) = gasdyn::normal_shock(mach, gamma) {
        put("pressure_ratio", s.pressure_ratio);
        put("temperature_ratio", s.temperature_ratio);
        put("density_ratio", s.density_ratio);
        put("mach_downstream", s.mach_downstream);
        put("stagnation_pressure_ratio", s.stagnation_pressure_ratio);
        if let Some(t1) = f("upstream_temperature_k") {
            put("downstream_temperature_k", t1 * s.temperature_ratio);
        }
    }
    if let Ok(nu) = gasdyn::prandtl_meyer_rad(mach, gamma) {
        put("prandtl_meyer_deg", nu.to_degrees());
    }
    if let Ok((theta, beta)) = gasdyn::max_deflection_rad(mach, gamma) {
        put("max_deflection_deg", theta.to_degrees());
        put("max_deflection_wave_angle_deg", beta.to_degrees());
    }
    m
}

/// M6, boundary-layer half.
fn evaluate_aero(c: &Case) -> BTreeMap<String, ExpectValue> {
    let mut m = BTreeMap::new();
    let f = |k: &str| c.inputs.get(k).and_then(toml::Value::as_float);
    let regime = match c.inputs.get("regime").and_then(toml::Value::as_str) {
        Some("laminar") => Regime::Laminar,
        _ => Regime::Turbulent,
    };

    if let Some(re) = f("reynolds_x") {
        if let Ok(cf) = bl::skin_friction_coefficient(re, regime) {
            m.insert("skin_friction".into(), ExpectValue::Float(cf));
            m.insert(
                "stanton".into(),
                ExpectValue::Float(bl::stanton_number(cf, f("prandtl").unwrap_or(PRANDTL_AIR))),
            );
        }
    }
    if let Some(pr) = f("prandtl") {
        m.insert(
            "recovery_factor".into(),
            ExpectValue::Float(bl::recovery_factor(pr, regime)),
        );
    }
    m
}

/// M5. Builds the edge state from M1 rather than taking it as case input, so a
/// change in the atmosphere propagates into the thermal cases automatically.
fn evaluate_thermal(c: &Case) -> BTreeMap<String, ExpectValue> {
    let mut m = BTreeMap::new();
    let f = |k: &str| c.inputs.get(k).and_then(toml::Value::as_float);

    let (Some(h), Some(mach), Some(x)) = (
        f("geopotential_altitude_m"),
        f("mach"),
        f("running_length_m"),
    ) else {
        return m;
    };
    let Ok(a) = ventus_atmos::at_geopotential(h) else {
        return m;
    };
    let edge = EdgeState {
        temperature_k: a.temperature_k,
        pressure_pa: a.pressure_pa,
        velocity_m_s: mach * a.speed_of_sound_m_s,
        mach,
        gamma: f("gamma").unwrap_or(1.4),
    };

    if let Ok(b) = ventus_thermal::radiation_equilibrium_wall(
        &edge,
        x,
        f("emissivity").unwrap_or(0.85),
        f("sink_temperature_k").unwrap_or(0.0),
        PRANDTL_AIR,
        Regime::Turbulent,
    ) {
        for (k, v) in [
            ("wall_temperature_k", b.wall_temperature_k),
            (
                "adiabatic_wall_temperature_k",
                b.film.adiabatic_wall_temperature_k,
            ),
            ("radiation_relief_k", b.radiation_relief_k()),
            (
                "heat_transfer_coefficient_w_m2_k",
                b.film.heat_transfer_coefficient_w_m2_k,
            ),
            ("convective_flux_w_m2", b.convective_flux_w_m2),
        ] {
            m.insert(k.to_string(), ExpectValue::Float(v));
        }
    }
    m
}

/// M3.
fn evaluate_inlet(c: &Case) -> BTreeMap<String, ExpectValue> {
    let mut m = BTreeMap::new();
    let f = |k: &str| c.inputs.get(k).and_then(toml::Value::as_float);
    let Some(mach) = f("mach") else { return m };

    m.insert(
        "mil_recovery".into(),
        ExpectValue::Float(ventus_inlet::mil_e_5008b_recovery(mach)),
    );

    let Some(n) = c.inputs.get("ramp_count").and_then(toml::Value::as_integer) else {
        return m;
    };
    let gamma = f("gamma").unwrap_or(1.4);

    // A "zero ramp" case is the degenerate single normal shock, used to show
    // that the train reduces to it.
    let train = if c
        .inputs
        .get("zero_ramp")
        .and_then(toml::Value::as_bool)
        .unwrap_or(false)
    {
        ventus_inlet::shock_train(mach, &[0.0], gamma).ok()
    } else {
        ventus_inlet::optimise_ramps(mach, n as usize, gamma)
            .ok()
            .map(|(_, t)| t)
    };

    if let Some(t) = train {
        m.insert(
            "total_recovery".into(),
            ExpectValue::Float(t.total_recovery),
        );
        m.insert(
            "total_turning_deg".into(),
            ExpectValue::Float(t.total_turning_rad.to_degrees()),
        );
        m.insert(
            "mach_before_terminal".into(),
            ExpectValue::Float(t.mach_before_terminal),
        );
    }
    m
}

/// M4. Builds the freestream from M1 so the cycle cannot drift from the
/// atmosphere the rest of the project uses.
fn evaluate_propulsion(c: &Case) -> BTreeMap<String, ExpectValue> {
    let mut m = BTreeMap::new();
    let f = |k: &str| c.inputs.get(k).and_then(toml::Value::as_float);
    let (Some(h), Some(mach)) = (f("geopotential_altitude_m"), f("mach")) else {
        return m;
    };
    let Ok(a) = ventus_atmos::at_geopotential(h) else {
        return m;
    };

    if let Ok(cycle) = ventus_propulsion::ideal_ramjet(
        mach,
        a.temperature_k,
        a.pressure_pa,
        mach * a.speed_of_sound_m_s,
        f("inlet_recovery").unwrap_or(0.7416),
        f("burner_exit_temperature_k").unwrap_or(1700.0),
        f("gamma").unwrap_or(1.4),
    ) {
        for (k, v) in [
            (
                "temperature_headroom_ratio",
                cycle.temperature_headroom_ratio,
            ),
            ("burner_gamma", cycle.burner_gamma),
            ("specific_impulse_s", cycle.specific_impulse_s),
            ("specific_thrust_n_s_kg", cycle.specific_thrust_n_s_kg),
            ("fuel_air_ratio", cycle.fuel_air_ratio),
            ("exit_velocity_m_s", cycle.exit_velocity_m_s),
        ] {
            m.insert(k.to_string(), ExpectValue::Float(v));
        }
    }
    m
}

fn evaluate(c: &Case) -> BTreeMap<String, ExpectValue> {
    // Dispatch on the crate the case came from, not on guessing from its inputs:
    // a case belongs to the module that owns its yardstick.
    let owner = |name: &str| c.file.components().any(|p| p.as_os_str() == name);
    if owner("ventus-atmos") {
        evaluate_atmos(c)
    } else if owner("ventus-aero") {
        evaluate_aero(c)
    } else if owner("ventus-thermal") {
        evaluate_thermal(c)
    } else if owner("ventus-inlet") {
        evaluate_inlet(c)
    } else if owner("ventus-propulsion") {
        evaluate_propulsion(c)
    } else {
        evaluate_gasdyn(c)
    }
}

fn validate() -> ExitCode {
    let repo = repo_root();
    let crates_dir = repo.join("crates");

    let mut dirs: Vec<PathBuf> = match std::fs::read_dir(&crates_dir) {
        Ok(rd) => rd
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.join("cases").is_dir())
            .collect(),
        Err(e) => {
            eprintln!("xtask: cannot read {}: {e}", crates_dir.display());
            return ExitCode::FAILURE;
        }
    };
    dirs.sort();

    let mut cases: Vec<Case> = Vec::new();
    let mut pending = 0_usize;

    for crate_dir in &dirs {
        let name = crate_dir.file_name().unwrap().to_string_lossy().to_string();
        let loaded = match case::load_dir(&crate_dir.join("cases")) {
            Ok(c) => c,
            Err(e) => {
                // ADR-000 D4: a refused case file stops the run. It is not a
                // failing case, it is a malformed contract.
                eprintln!("xtask validate: case file refused\n  {e}");
                return ExitCode::FAILURE;
            }
        };
        // An empty `cases/` is the other way the corpus shrinks quietly: the
        // directory survives, so `MINIMUM_CORPUS` below is the only thing left
        // to notice, and it only notices once enough of them have gone.
        if loaded.is_empty() && crate_dir.join("cases").is_dir() {
            eprintln!("xtask validate: {name} has a cases/ directory with no cases in it.");
            eprintln!("  Delete the directory or write the cases; an empty one is not a pass.");
            return ExitCode::FAILURE;
        }
        if PENDING_MODULES.contains(&name.as_str()) {
            pending += loaded.len();
            continue;
        }
        cases.extend(loaded.into_iter().map(|mut c| {
            c.file = c.file.strip_prefix(&repo).unwrap_or(&c.file).to_path_buf();
            c
        }));
    }

    if cases.len() < MINIMUM_CORPUS {
        eprintln!(
            "xtask validate: {} wired cases, below the floor of {MINIMUM_CORPUS}.",
            cases.len()
        );
        eprintln!("  The corpus is not being read, so a verdict would be meaningless.");
        eprintln!(
            "  Check that crates/*/cases/ resolve under {},",
            repo.display()
        );
        eprintln!("  and that this is not a stale build carrying a baked-in path");
        eprintln!("  from somewhere the repository no longer lives.");
        return ExitCode::FAILURE;
    }

    let outcomes: Vec<Outcome> = cases
        .iter()
        .map(|c| {
            let computed = evaluate(c);
            check(c, &computed)
        })
        .collect();

    let entries: Vec<Entry<'_>> = cases
        .iter()
        .zip(&outcomes)
        .map(|(case, outcome)| Entry { case, outcome })
        .collect();

    let prov = Provenance::detect();
    let out_dir = repo.join("out").join(&prov.run_id);
    if let Err(e) = report::write_all(&out_dir, &prov, &entries) {
        eprintln!("xtask: cannot write report: {e}");
        return ExitCode::FAILURE;
    }

    let summary: Summary = outcomes.iter().fold(Summary::default(), |mut s, o| {
        s.record(o);
        s
    });

    println!("run_id  : {}", prov.run_id);
    println!(
        "commit  : {} ({})",
        prov.git_hash,
        if prov.git_dirty {
            "DIRTY - not reproducible"
        } else {
            "clean"
        }
    );
    println!("wired   : {} cases evaluated", summary.total());
    println!("pending : {pending} cases in modules not yet implemented");
    println!(
        "verdict : {} pass, {} fail, {} known limit, {} stale",
        summary.pass, summary.fail, summary.known_limit, summary.stale_known_limit
    );
    println!(
        "report  : {}",
        out_dir.strip_prefix(&repo).unwrap_or(&out_dir).display()
    );

    if summary.stale_known_limit > 0 {
        println!();
        println!(
            "FAIL: {} known-limit annotation(s) now pass. The limitation no longer",
            summary.stale_known_limit
        );
        println!("  reproduces, so the annotation is a false claim in the report.");
        println!("  Delete it and re-run. This is the build refusing to ship a");
        println!("  statement it has itself just disproved.");
    }

    if summary.breaks_build() {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs every checked-in case through the canonical evaluator under
    /// `cargo test`, so the validation suite is not something that only happens
    /// when someone remembers to type `cargo xtask validate`.
    #[test]
    fn every_case_in_the_repository_passes() {
        let repo = repo_root();
        let mut summary = Summary::default();
        let mut failures = String::new();
        let mut seen = 0_usize;

        let mut dirs: Vec<PathBuf> = std::fs::read_dir(repo.join("crates"))
            .expect("crates/")
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.join("cases").is_dir())
            .collect();
        dirs.sort();

        for dir in &dirs {
            let name = dir.file_name().unwrap().to_string_lossy().to_string();
            let cases = case::load_dir(&dir.join("cases")).unwrap_or_else(|e| {
                panic!(
                    "case file refused:
  {e}"
                )
            });
            if PENDING_MODULES.contains(&name.as_str()) {
                continue;
            }
            for c in &cases {
                seen += 1;
                let outcome = check(c, &evaluate(c));
                summary.record(&outcome);
                if let Outcome::Fail {
                    mismatches,
                    missing,
                } = &outcome
                {
                    failures.push_str(&format!(
                        "
{} [{}]
",
                        c.name,
                        c.file.display()
                    ));
                    for m in mismatches {
                        failures.push_str(&format!(
                            "  {}: expected {}, got {} (rel {:.3e})
",
                            m.key, m.expected, m.actual, m.rel_err
                        ));
                    }
                    for k in missing {
                        failures.push_str(&format!(
                            "  {k}: never computed
"
                        ));
                    }
                }
            }
        }

        assert!(
            seen >= MINIMUM_CORPUS,
            "expected the full case corpus, saw {seen}"
        );
        assert!(
            !summary.breaks_build(),
            "{} of {} cases failed:{failures}",
            summary.fail,
            summary.total()
        );
        assert_eq!(
            summary.stale_known_limit, 0,
            "a known-limit annotation now passes and should be removed"
        );
    }
}
