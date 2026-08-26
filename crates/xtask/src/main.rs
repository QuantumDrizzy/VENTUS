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
const PENDING_MODULES: &[&str] = &[
    "ventus-inlet",
    "ventus-propulsion",
    "ventus-aero",
    "ventus-thermal",
    "ventus-mass",
    "ventus-dynamics",
    "ventus-fsw",
];

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
        if PENDING_MODULES.contains(&name.as_str()) {
            pending += loaded.len();
            continue;
        }
        cases.extend(loaded.into_iter().map(|mut c| {
            c.file = c.file.strip_prefix(&repo).unwrap_or(&c.file).to_path_buf();
            c
        }));
    }

    let outcomes: Vec<Outcome> = cases
        .iter()
        .map(|c| {
            // Dispatch on the crate the case came from, not on guessing from its
            // inputs: a case belongs to the module that owns its yardstick.
            let computed = if c.file.components().any(|p| p.as_os_str() == "ventus-atmos") {
                evaluate_atmos(c)
            } else {
                evaluate_gasdyn(c)
            };
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
        println!(
            "\nNOTE: {} known-limit annotation(s) now pass. The limit no longer \
             reproduces; remove the annotation.",
            summary.stale_known_limit
        );
    }

    if summary.breaks_build() {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
