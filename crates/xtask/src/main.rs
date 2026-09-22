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

mod bench;
mod hil;

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
        Some("bench") => bench_gated(),
        Some("hil") => hil::run(std::env::args().nth(2).as_deref()),
        Some(other) => {
            eprintln!("xtask: `{other}` is not implemented yet.");
            eprintln!("available: validate | bench | hil");
            eprintln!("planned:   build | check-dag | report  (ADR-000 D6)");
            ExitCode::from(2)
        }
        None => {
            eprintln!("usage: cargo xtask <validate|bench|hil>");
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

/// How each crate in the workspace is validated.
///
/// [CORRECTED] This replaces `PENDING_MODULES`, which listed ventus-mass,
/// ventus-dynamics and ventus-fsw as "modules with cases but no evaluator". All
/// three were finished, none of them had a single case, and the runner printed
/// `pending : 0` every time - a line that said nothing about three modules the
/// README called done. The label was wrong in both directions at once.
///
/// The replacement forces the question to be answered per crate, because the
/// real distinction is not "done or not". It is whether a module HAS AN EXTERNAL
/// NUMBER TO CITE. The case corpus exists to hold claims traceable to a
/// published source; a module whose yardstick is an analytic identity does not
/// belong in it, and forcing it in would weaken the contract that makes `source`
/// mandatory for everything else.
///
/// An unlisted crate is an error, so adding one is a decision rather than an
/// omission.
const ROUTES: &[(&str, Route)] = &[
    ("ventus-atmos", Route::Cases),
    ("ventus-gasdyn", Route::Cases),
    ("ventus-inlet", Route::Cases),
    ("ventus-propulsion", Route::Cases),
    ("ventus-scram", Route::Cases),
    ("ventus-aero", Route::Cases),
    ("ventus-thermal", Route::Cases),
    ("ventus-mass", Route::Cases),
    ("ventus-cost", Route::Cases),
    ("ventus-envelope", Route::Cases),
    (
        "ventus-dynamics",
        Route::Identities(concat!(
            "M8 has no published trajectory to check against. Its yardsticks are ",
            "conservation laws and a convergence order: energy drift below 1e-10 over 1e6 ",
            "steps, angular momentum conserved in direction as well as magnitude, RK4 ",
            "shown to be fourth order, and the intermediate-axis instability appearing ",
            "from the equations rather than being added as a correction. None of those is ",
            "a citation; all of them are exact, and they are asserted in the module's own ",
            "tests.",
        )),
    ),
    (
        "ventus-fsw",
        Route::Identities(concat!(
            "M10's claim is bit-for-bit agreement with M1, which is a cross-check between ",
            "two of this project's own modules, not an external number. Putting it in the ",
            "corpus would mean writing a `source` field that cites ourselves, which is ",
            "exactly the drift the mandatory source exists to stop. It is asserted by ",
            "equality tests in the module.",
        )),
    ),
    (
        "ventus-units",
        Route::Identities(concat!(
            "Conversions and float helpers. The yardstick is the SI definition of each ",
            "unit, exact by construction, so the tests assert exact round trips rather ",
            "than tolerances.",
        )),
    ),
    (
        "ventus-validate",
        Route::Identities(concat!(
            "The harness itself, and the one module that cannot be validated by the ",
            "harness without circularity. Its acceptance suite is negative: it asserts ",
            "that malformed cases are refused and wrong numbers are failed, plus a ",
            "positive control, so a harness that refused everything would not pass.",
        )),
    ),
    ("xtask", Route::NotAModule),
];

/// The validation route a crate takes. Every crate declares one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Route {
    /// Cases in `cases/`, each citing an external source, evaluated by
    /// [`evaluate`].
    Cases,
    /// No external number to cite, so no cases. Validated by identities in the
    /// module's own tests. The reason is mandatory and is printed by
    /// `xtask validate --routes`.
    Identities(&'static str),
    /// Build tooling, not physics.
    NotAModule,
}

/// Greedy word wrap. The route reasons are the load-bearing documentation of
/// why a module is absent from the corpus, so they are printed in full rather
/// than truncated.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.len() + 1 + word.len() > width {
            out.push(core::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        out.push(line);
    }
    out
}

fn route_of(crate_name: &str) -> Option<Route> {
    ROUTES
        .iter()
        .find(|(n, _)| *n == crate_name)
        .map(|(_, r)| *r)
}

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

/// M3. Recovery, capture identities, Kantrowitz, declared-lip additive drag,
/// and the spike/unstart refusals.
/// A case names the question with its inputs; keys it does not name are ignored.
fn evaluate_inlet(c: &Case) -> BTreeMap<String, ExpectValue> {
    let mut m = BTreeMap::new();
    let f = |k: &str| c.inputs.get(k).and_then(toml::Value::as_float);
    let flag = |k: &str| {
        c.inputs
            .get(k)
            .and_then(toml::Value::as_bool)
            .unwrap_or(false)
    };

    if let (Some(capture), Some(body)) = (f("capture_area_m2"), f("body_area_m2")) {
        if let Ok(r) = ventus_inlet::capture_to_body_ratio(capture, body) {
            m.insert("capture_to_body_ratio".into(), ExpectValue::Float(r));
        }
        if let Ok(fits) = ventus_inlet::body_can_host_capture(capture, body) {
            m.insert("body_can_host_capture".into(), ExpectValue::Bool(fits));
        }
    }

    if let (Some(a0), Some(ac)) = (f("streamtube_area_m2"), f("cowl_area_m2")) {
        if let Ok(s) = ventus_inlet::spillage(a0, ac) {
            m.insert(
                "mass_flow_ratio".into(),
                ExpectValue::Float(s.mass_flow_ratio),
            );
            m.insert(
                "spilled_area_m2".into(),
                ExpectValue::Float(s.spilled_area_m2),
            );
            m.insert(
                "capture_exceeds_cowl".into(),
                ExpectValue::Bool(s.capture_exceeds_cowl),
            );
        }
        let lip_ratio = f("lip_radius_ratio").or_else(|| {
            f("lip_radius_m").map(|r| {
                let highlight_radius = (ac / std::f64::consts::PI).sqrt();
                r / highlight_radius
            })
        });
        if let Some(ratio) = lip_ratio {
            m.insert(
                "refused_cowl_lip_not_modelled".into(),
                ExpectValue::Bool(false),
            );
            if let (Some(p), Some(rho), Some(v)) = (
                f("static_pressure_pa"),
                f("density_kg_m3"),
                f("velocity_m_s"),
            ) {
                if let Ok(geom) =
                    ventus_inlet::CowlLipGeometry::from_highlight_and_radius_ratio(ac, ratio)
                {
                    let fs = ventus_inlet::FreestreamStation {
                        streamtube_area_m2: a0,
                        static_pressure_pa: p,
                        density_kg_m3: rho,
                        velocity_m_s: v,
                    };
                    let gamma = f("gamma").unwrap_or(1.4);
                    match ventus_inlet::additive_drag_from_lip(fs, geom, gamma) {
                        Ok(d) => {
                            m.insert("additive_drag_n".into(), ExpectValue::Float(d.force_n));
                            m.insert(
                                "additive_drag_coefficient".into(),
                                ExpectValue::Float(d.coefficient),
                            );
                            m.insert(
                                "lip_radius_ratio".into(),
                                ExpectValue::Float(d.lip_radius_ratio),
                            );
                            m.insert(
                                "additive_drag_within_stated_range".into(),
                                ExpectValue::Bool(d.within_stated_range),
                            );
                        }
                        Err(ventus_inlet::CaptureError::CaptureExceedsCowl) => {
                            m.insert(
                                "refused_capture_exceeds_cowl".into(),
                                ExpectValue::Bool(true),
                            );
                        }
                        Err(ventus_inlet::CaptureError::Subsonic) => {
                            m.insert("refused_subsonic".into(), ExpectValue::Bool(true));
                        }
                        Err(_) => {}
                    }
                }
            }
        } else {
            m.insert(
                "refused_cowl_lip_not_modelled".into(),
                ExpectValue::Bool(matches!(
                    ventus_inlet::additive_drag_without_lip(a0, ac),
                    Err(ventus_inlet::CaptureError::CowlLipNotModelled)
                )),
            );
            // Without a lip, do NOT emit additive_drag_coefficient. The
            // known_limit that asked for a Cd from μ alone is gone; the
            // remaining missing-key known_limit is lip suction.
        }
        // Deliberately do NOT emit lip_suction_force_n: r/R is geometry, not a
        // credited force. The known_limit case that asks for it fails closed.
    }

    if let Some(mach) = f("mach") {
        m.insert(
            "mil_recovery".into(),
            ExpectValue::Float(ventus_inlet::mil_e_5008b_recovery(mach)),
        );
        let gamma = f("gamma").unwrap_or(1.4);
        if let Ok(k) = ventus_inlet::kantrowitz_contraction_ratio(mach, gamma) {
            m.insert("kantrowitz_contraction".into(), ExpectValue::Float(k));
        }
        if let Ok(i) = ventus_inlet::isentropic_contraction_ratio(mach, gamma) {
            m.insert("isentropic_contraction".into(), ExpectValue::Float(i));
        }

        if flag("request_spike_schedule") {
            m.insert(
                "refused_spike_schedule_not_modelled".into(),
                ExpectValue::Bool(matches!(
                    ventus_inlet::spike_position_m(mach),
                    Err(ventus_inlet::StartError::SpikeScheduleNotModelled)
                )),
            );
        }
        if flag("request_unstart_margin") {
            m.insert(
                "refused_unstart_dynamics_not_modelled".into(),
                ExpectValue::Bool(matches!(
                    ventus_inlet::unstart_margin(mach),
                    Err(ventus_inlet::StartError::UnstartDynamicsNotModelled)
                )),
            );
        }

        let Some(n) = c.inputs.get("ramp_count").and_then(toml::Value::as_integer) else {
            return m;
        };

        // A "zero ramp" case is the degenerate single normal shock, used to show
        // that the train reduces to it.
        let train = if flag("zero_ramp") {
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
            (
                "burner_entry_total_pressure_pa",
                cycle.burner_entry_total_pressure_pa,
            ),
            (
                "burner_entry_total_temperature_k",
                cycle.burner_entry_total_temperature_k,
            ),
            ("exit_velocity_m_s", cycle.exit_velocity_m_s),
        ] {
            m.insert(k.to_string(), ExpectValue::Float(v));
        }
    }
    m
}

/// Dual-mode / scram track (ADR-003). Emits refusal flags only. Deliberately
/// does NOT emit `specific_impulse_s` or `specific_thrust_n_s_kg`: there is no
/// cited deck, and a zero would be a number. The case that asks for Isp fails
/// with a missing key, the same mechanism as M11's uncited programme cost.
fn evaluate_scram(c: &Case) -> BTreeMap<String, ExpectValue> {
    let mut m = BTreeMap::new();
    let f = |k: &str| c.inputs.get(k).and_then(toml::Value::as_float);
    let regime = match c.inputs.get("regime").and_then(toml::Value::as_str) {
        Some("ram_subsonic_burner") => ventus_scram::Regime::RamSubsonicBurner,
        Some("dual_mode_transition") => ventus_scram::Regime::DualModeTransition,
        Some("scram") => ventus_scram::Regime::Scram,
        Some(other) => panic!("case `{}`: unknown regime `{other}`", c.name),
        None => panic!("case `{}` needs `regime`", c.name),
    };
    let mach = f("mach").unwrap_or_else(|| panic!("case `{}` needs `mach`", c.name));
    let altitude_m = f("geopotential_altitude_m").unwrap_or(26_000.0);

    match ventus_scram::solve_cycle(ventus_scram::CycleRequest {
        mach_freestream: mach,
        geopotential_altitude_m: altitude_m,
        regime,
    }) {
        Ok(_) => {
            panic!(
                "case `{}`: ventus-scram returned Ok; the stub must not emit a \
                 cycle until stations exist (ADR-003)",
                c.name
            );
        }
        Err(e) => {
            m.insert(
                "cycle_numbers_emitted".to_string(),
                ExpectValue::Bool(false),
            );
            let (stations, ramjet, nan, non_physical, station) = match e {
                ventus_scram::CycleError::StationsNotModelled => (true, false, false, false, false),
                ventus_scram::CycleError::UseIdealRamjet => (false, true, false, false, false),
                ventus_scram::CycleError::NotANumber => (false, false, true, false, false),
                ventus_scram::CycleError::NonPhysicalMach => (false, false, false, true, false),
                ventus_scram::CycleError::StationNotModelled(_) => {
                    (false, false, false, false, true)
                }
            };
            m.insert(
                "refused_stations_not_modelled".to_string(),
                ExpectValue::Bool(stations),
            );
            m.insert(
                "refused_use_ideal_ramjet".to_string(),
                ExpectValue::Bool(ramjet),
            );
            m.insert("refused_not_a_number".to_string(), ExpectValue::Bool(nan));
            m.insert(
                "refused_non_physical_mach".to_string(),
                ExpectValue::Bool(non_physical),
            );
            m.insert(
                "refused_station_not_modelled".to_string(),
                ExpectValue::Bool(station),
            );
        }
    }
    m
}

/// M12. The regime sweep. Every output is a module refusing or a module
/// answering; this evaluator adds nothing of its own.
fn evaluate_envelope(c: &Case) -> BTreeMap<String, ExpectValue> {
    use ventus_envelope::Refusal;
    let f = |k: &str| c.inputs.get(k).and_then(toml::Value::as_float);
    let mut m = BTreeMap::new();

    if let Some(mach) = f("mach") {
        if let Some(q) = f("dynamic_pressure_pa") {
            if let Some(h) = ventus_envelope::altitude_for_constant_q_m(mach, q) {
                m.insert("altitude_m".to_string(), ExpectValue::Float(h));
            }
        }

        let p = ventus_envelope::evaluate(mach);
        for (k, v) in [
            ("stagnation_temperature_k", p.stagnation_temperature_k),
            ("gamma_at_stagnation", p.gamma_at_stagnation),
            ("inlet_recovery", p.inlet_recovery),
            ("ramjet_specific_impulse_s", p.ramjet_specific_impulse_s),
            (
                "ramjet_specific_thrust_n_s_kg",
                p.ramjet_specific_thrust_n_s_kg,
            ),
            ("wall_temperature_k", p.wall_temperature_k),
        ] {
            if let Some(x) = v {
                m.insert(k.to_string(), ExpectValue::Float(x));
            }
        }

        m.insert(
            "refusal_count".to_string(),
            ExpectValue::Float(p.refusal_count() as f64),
        );
        m.insert(
            "ramjet_isp_is_meaningful".to_string(),
            ExpectValue::Bool(p.ramjet_isp_is_meaningful()),
        );
        if let (Some(isp), Some(fs)) =
            (p.ramjet_specific_impulse_s, p.ramjet_specific_thrust_n_s_kg)
        {
            let fuel_air = fs / (isp * ventus_units::constants::G0_M_S2);
            let phi = fuel_air / ventus_envelope::STOICHIOMETRIC_FUEL_AIR_RATIO;
            m.insert("fuel_air_ratio".to_string(), ExpectValue::Float(fuel_air));
            m.insert("equivalence_ratio".to_string(), ExpectValue::Float(phi));
            m.insert(
                "below_operative_lean_blowout_bound".to_string(),
                ExpectValue::Bool(
                    ventus_envelope::operative_lean_blowout_verdict(phi)
                        .is_some_and(ventus_envelope::LeanBlowoutVerdict::below_operative),
                ),
            );
            m.insert(
                "inside_literature_blowout_band".to_string(),
                ExpectValue::Bool(
                    ventus_envelope::literature_blowout_band(phi)
                        .is_some_and(ventus_envelope::LiteratureBlowoutBand::inside),
                ),
            );
        }
        m.insert(
            "flame_holder_declared".to_string(),
            ExpectValue::Bool(ventus_envelope::FLAME_HOLDER_DECLARED),
        );
        m.insert(
            "operative_phi_bound".to_string(),
            ExpectValue::Float(ventus_envelope::OPERATIVE_LEAN_BLOWOUT_PHI),
        );
        if let Some(p02) = p.burner_entry_total_pressure_pa {
            m.insert(
                "burner_entry_total_pressure_pa".to_string(),
                ExpectValue::Float(p02),
            );
        }
        if let Some(t02) = p.burner_entry_total_temperature_k {
            m.insert(
                "burner_entry_total_temperature_k".to_string(),
                ExpectValue::Float(t02),
            );
        }
        // Deliberately do NOT emit `lean_blowout_verified`. The arithmetic is
        // pinned above; a verified fly/no-fly needs a primary φ_LBO. The
        // known_limit case that asks for the key fails closed.
        if let Some(r) = ventus_envelope::capture_area_ratio(mach) {
            m.insert("capture_area_ratio".to_string(), ExpectValue::Float(r));
        }
        if let Some(r) = ventus_envelope::m4_candidate_capture_area_ratio(mach) {
            m.insert(
                "m4_candidate_capture_area_ratio".to_string(),
                ExpectValue::Float(r),
            );
            m.insert(
                "m4_candidate_body_can_host_capture".to_string(),
                ExpectValue::Bool(r <= 1.0),
            );
        }
        let candidate = ventus_envelope::m4_candidate_geometry();
        m.insert(
            "m4_candidate_max_cross_section_m2".to_string(),
            ExpectValue::Float(candidate.max_cross_section_m2),
        );
        m.insert(
            "m4_candidate_fineness_ratio".to_string(),
            ExpectValue::Float(candidate.fineness_ratio()),
        );
        match ventus_envelope::additive_drag_on_snapshot_cowl(mach) {
            Some(Ok(d)) => {
                m.insert(
                    "snapshot_cowl_additive_drag_n".to_string(),
                    ExpectValue::Float(d.force_n),
                );
                m.insert(
                    "snapshot_cowl_additive_drag_coefficient".to_string(),
                    ExpectValue::Float(d.coefficient),
                );
                m.insert(
                    "snapshot_cowl_capture_exceeds".to_string(),
                    ExpectValue::Bool(false),
                );
            }
            Some(Err(ventus_inlet::CaptureError::CaptureExceedsCowl)) => {
                m.insert(
                    "snapshot_cowl_capture_exceeds".to_string(),
                    ExpectValue::Bool(true),
                );
            }
            Some(Err(_)) | None => {}
        }
        match ventus_envelope::additive_drag_on_m4_candidate_cowl(mach) {
            Some(Ok(d)) => {
                m.insert(
                    "m4_candidate_cowl_additive_drag_n".to_string(),
                    ExpectValue::Float(d.force_n),
                );
                m.insert(
                    "m4_candidate_cowl_additive_drag_coefficient".to_string(),
                    ExpectValue::Float(d.coefficient),
                );
            }
            Some(Err(ventus_inlet::CaptureError::CaptureExceedsCowl)) => {
                m.insert(
                    "m4_candidate_cowl_capture_exceeds".to_string(),
                    ExpectValue::Bool(true),
                );
            }
            Some(Err(_)) | None => {}
        }
        if let Some(a) = ventus_envelope::self_consistent_capture_area_m2(mach) {
            m.insert(
                "self_consistent_capture_area_m2".to_string(),
                ExpectValue::Float(a),
            );
        }
        m.insert(
            "any_body_closes_the_thrust_balance".to_string(),
            ExpectValue::Bool(ventus_envelope::self_consistent_capture_area_m2(mach).is_some()),
        );
        for (k, r) in [
            ("refused_atmosphere_model_top", Refusal::AtmosphereModelTop),
            (
                "refused_gas_model_out_of_range",
                Refusal::GasModelOutOfRange,
            ),
            (
                "refused_inlet_shocks_detached",
                Refusal::InletShocksDetached,
            ),
            (
                "refused_ramjet_thermally_choked",
                Refusal::RamjetThermallyChoked,
            ),
            ("refused_no_material_survives", Refusal::NoMaterialSurvives),
        ] {
            m.insert(k.to_string(), ExpectValue::Bool(p.refused(r)));
        }
    }
    m
}

/// M11. DAPCA IV. Note what is NOT emitted: `programme_cost_usd_1986_verified`,
/// because no primary source for one exists. The case that asks for it fails
/// with a missing key, which is the honest answer.
fn evaluate_cost(c: &Case) -> BTreeMap<String, ExpectValue> {
    let f = |k: &str| c.inputs.get(k).and_then(toml::Value::as_float);
    let need = |k: &str| f(k).unwrap_or_else(|| panic!("case `{}` needs input `{k}`", c.name));

    let inputs = ventus_cost::Inputs {
        empty_mass_kg: need("empty_mass_kg"),
        max_velocity_m_s: need("max_velocity_m_s"),
        production_quantity: need("production_quantity"),
        flight_test_aircraft: need("flight_test_aircraft"),
        material_factor: need("material_factor"),
    };
    let rates = ventus_cost::Rates::raymer_1986();
    let envelope = ventus_cost::Envelope::conventional_metal();

    let mut m = BTreeMap::new();
    m.insert(
        "implied_learning_curve".to_string(),
        ExpectValue::Float(ventus_cost::dapca::implied_learning_curve()),
    );

    let Ok(e) = ventus_cost::estimate(&inputs, &rates, &envelope) else {
        return m;
    };

    for (k, v) in [
        ("engineering_hours", e.hours.engineering),
        ("tooling_hours", e.hours.tooling),
        ("manufacturing_hours", e.hours.manufacturing),
        ("quality_hours", e.hours.quality),
        (
            "quality_fraction_of_manufacturing",
            e.hours.quality / e.hours.manufacturing,
        ),
        ("development_usd", e.development_usd),
        ("flight_test_usd", e.flight_test_usd),
        ("materials_usd", e.materials_usd),
        ("labour_usd", e.labour_usd),
        ("airframe_total_usd", e.airframe_total_usd),
        ("per_aircraft_usd", e.per_aircraft_usd),
    ] {
        m.insert(k.to_string(), ExpectValue::Float(v));
    }

    m.insert(
        "is_extrapolated".to_string(),
        ExpectValue::Bool(e.validity.is_extrapolated()),
    );
    if let ventus_cost::Validity::Extrapolated(x) = e.validity {
        m.insert(
            "velocity_ratio".to_string(),
            ExpectValue::Float(x.velocity_ratio),
        );
        m.insert(
            "empty_weight_ratio".to_string(),
            ExpectValue::Float(x.empty_weight_ratio),
        );
    }
    m
}

/// M7. Breguet in both directions: a case gives the masses and asks for the
/// range, or gives the range and asks what fuel fraction it costs.
fn evaluate_mass(c: &Case) -> BTreeMap<String, ExpectValue> {
    let mut m = BTreeMap::new();
    let f = |k: &str| c.inputs.get(k).and_then(toml::Value::as_float);
    let need = |k: &str| f(k).unwrap_or_else(|| panic!("case `{}` needs input `{k}`", c.name));

    let v = need("velocity_m_s");
    let ld = need("lift_to_drag");
    let isp = need("specific_impulse_s");

    if let (Some(m0), Some(m1)) = (f("initial_mass_kg"), f("final_mass_kg")) {
        if let Ok(r) = ventus_mass::breguet_range_m(v, ld, isp, m0, m1) {
            m.insert("range_m".to_string(), ExpectValue::Float(r));
        }
    }
    if let Some(r) = f("range_m") {
        if let Ok(frac) = ventus_mass::required_fuel_fraction(r, v, ld, isp) {
            m.insert("fuel_fraction".to_string(), ExpectValue::Float(frac));
        }
    }
    m
}

/// M7, the empty-mass half. Separate from the Breguet evaluator because these
/// cases take a different input set entirely, and dispatching on which keys
/// happen to be present is how a case ends up silently answered by the wrong
/// model.
fn evaluate_mass_empty(c: &Case) -> BTreeMap<String, ExpectValue> {
    let f = |k: &str| c.inputs.get(k).and_then(toml::Value::as_float);
    let mut m = BTreeMap::new();

    if let (Some(cruise), Some(frac)) = (f("cruise_mass_kg"), f("empty_fraction")) {
        if let Ok(e) = ventus_mass::zero_fuel_mass_from_cruise_anchor(cruise, frac) {
            m.insert("zero_fuel_mass_kg".to_string(), ExpectValue::Float(e));
        }
    }
    if let (Some(a), Some(end)) = (f("anchor_mass_kg"), f("end_of_cruise_mass_kg")) {
        // If the anchor were the geometric mid-cruise mass, the start of cruise
        // would be a^2/end. The case checks that against the max gross mass.
        m.insert(
            "geometric_mid_cruise_implied_start_kg".to_string(),
            ExpectValue::Float(a * a / end),
        );
    }
    if let Some(lb) = f("pounds") {
        m.insert(
            "kilograms".to_string(),
            ExpectValue::Float(lb * 0.453_592_37),
        );
    }
    if let Some(w0) = f("takeoff_mass_kg") {
        m.insert(
            "raymer_empty_fraction".to_string(),
            ExpectValue::Float(ventus_mass::raymer_jet_fighter_empty_fraction(w0)),
        );
    }
    if let (Some(cruise), Some(zfm), Some(reserve)) = (
        f("cruise_mass_kg"),
        f("zero_fuel_mass_kg"),
        f("reserve_fuel_kg"),
    ) {
        let v = f("velocity_m_s").unwrap_or_else(|| panic!("case `{}` needs velocity_m_s", c.name));
        let ld =
            f("lift_to_drag").unwrap_or_else(|| panic!("case `{}` needs lift_to_drag", c.name));
        let isp = f("specific_impulse_s")
            .unwrap_or_else(|| panic!("case `{}` needs specific_impulse_s", c.name));
        if let Ok(cl) = ventus_mass::close_cruise(cruise, zfm, reserve, v, ld, isp) {
            for (k, val) in [
                ("range_m", cl.range_m),
                ("fuel_fraction", cl.fuel_fraction),
                ("final_mass_kg", cl.final_mass_kg),
            ] {
                m.insert(k.to_string(), ExpectValue::Float(val));
            }
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
    } else if owner("ventus-scram") {
        evaluate_scram(c)
    } else if owner("ventus-mass") {
        // The two halves of M7 take disjoint input sets; pick by what the case
        // names, not by what happens to parse.
        if c.inputs.contains_key("empty_fraction")
            || c.inputs.contains_key("takeoff_mass_kg")
            || c.inputs.contains_key("reserve_fuel_kg")
            || c.inputs.contains_key("anchor_mass_kg")
            || c.inputs.contains_key("pounds")
        {
            evaluate_mass_empty(c)
        } else {
            evaluate_mass(c)
        }
    } else if owner("ventus-cost") {
        evaluate_cost(c)
    } else if owner("ventus-envelope") {
        evaluate_envelope(c)
    } else if owner("ventus-gasdyn") {
        evaluate_gasdyn(c)
    } else {
        // [CORRECTED] This used to fall through to `evaluate_gasdyn`. A case from
        // a module nobody had wired was therefore handed to the compressible-flow
        // evaluator, which returned whatever it could compute from inputs it did
        // not recognise - so a wiring omission surfaced as physics failures in
        // the wrong module rather than as the omission it was. The dispatch now
        // refuses, because there is no right answer to give.
        panic!(
            "case `{}` ({}) comes from a module with no evaluator. Add one, and list the crate in ROUTES as Route::Cases.",
            c.name,
            c.file.display()
        )
    }
}

/// Run the denied-lint gate before certifying anything.
///
/// `[workspace.lints.clippy]` makes `assertions_on_constants` an error, but a
/// lint only fires when clippy runs, and clippy was something run by hand. That
/// is how the same mistake reached three commits: it was CAUGHT every time and
/// PREVENTED none of them.
///
/// So the report refuses to certify code that has not passed its own lint gate.
/// This is the same move as `ramjet_isp_is_meaningful` in M12 - a thing a human
/// kept having to notice, turned into a thing a machine notices.
///
/// Returns `false` if clippy failed, having already printed its own diagnostics.
fn lint_gate_passes(repo: &Path) -> bool {
    let out = std::process::Command::new(std::env::var("CARGO").as_deref().unwrap_or("cargo"))
        .args(["clippy", "--workspace", "--all-targets", "--quiet"])
        .current_dir(repo)
        .status();
    match out {
        Ok(status) if status.success() => true,
        Ok(_) => {
            eprintln!("xtask validate: the lint gate failed; see the clippy output above.");
            eprintln!("  A denied lint means the code asserts something it does not test.");
            eprintln!("  Fix it rather than bypassing: the report certifies the tree it ran on.");
            false
        }
        Err(e) => {
            eprintln!("xtask validate: could not run clippy: {e}");
            false
        }
    }
}

/// Count the modelling constants still marked `[TO CITE]` in source.
///
/// The harness gates CASES: a case file without a `source` field is refused, and
/// that is what the README describes. It does not gate CONSTANTS - a `[TO CITE]`
/// in a doc comment compiles and validates fine.
///
/// That gap is real and it is not going to be closed by a lint, because the
/// whole point of the marker is to let an uncited number exist while being
/// visibly uncited. What can be closed is the gap between the gap and the claim:
/// the number is counted here and printed with the verdict, so the README can
/// point at a measurement instead of asserting a figure that rots.
fn open_citations(repo: &Path) -> usize {
    fn walk(dir: &Path, count: &mut usize) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.filter_map(Result::ok) {
            let p = e.path();
            if p.is_dir() {
                if p.file_name().is_some_and(|n| n == "target") {
                    continue;
                }
                walk(&p, count);
            } else if p.extension().is_some_and(|x| x == "rs") {
                if let Ok(text) = std::fs::read_to_string(&p) {
                    *count += text.matches("[TO CITE]").count();
                }
            }
        }
    }
    // [CORRECTED] Walked all of `crates/` on the first attempt, which counted
    // xtask's own doc comment and its own search string: four phantom markers
    // reported as open citations. A counter that counts itself is the same class
    // of error as a test that tests nothing, and it inflated the honest number
    // by 12 %. Physics crates only.
    let mut count = 0;
    let Ok(entries) = std::fs::read_dir(repo.join("crates")) else {
        return 0;
    };
    for e in entries.filter_map(Result::ok) {
        let p = e.path();
        if p.is_dir() && p.file_name().is_some_and(|n| n != "xtask") {
            walk(&p, &mut count);
        }
    }
    count
}

fn validate() -> ExitCode {
    let repo = repo_root();
    let crates_dir = repo.join("crates");

    if !lint_gate_passes(&repo) {
        return ExitCode::FAILURE;
    }

    let mut dirs: Vec<PathBuf> = match std::fs::read_dir(&crates_dir) {
        Ok(rd) => rd
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect(),
        Err(e) => {
            eprintln!("xtask: cannot read {}: {e}", crates_dir.display());
            return ExitCode::FAILURE;
        }
    };
    dirs.sort();

    let mut cases: Vec<Case> = Vec::new();
    let mut identities: Vec<(String, &'static str)> = Vec::new();

    for crate_dir in &dirs {
        let name = crate_dir.file_name().unwrap().to_string_lossy().to_string();

        // Every crate declares how it is validated. An unlisted one stops the
        // run: a new module must make that choice, not inherit a default.
        let route = match route_of(&name) {
            Some(r) => r,
            None => {
                eprintln!("xtask validate: crate `{name}` is not listed in ROUTES.");
                eprintln!("  Declare how it is validated: Route::Cases if it has an");
                eprintln!("  external number to cite, Route::Identities(reason) if its");
                eprintln!("  yardstick is an identity, Route::NotAModule if it is tooling.");
                return ExitCode::FAILURE;
            }
        };

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

        match route {
            Route::Cases if loaded.is_empty() => {
                eprintln!("xtask validate: {name} is declared Route::Cases but has no cases.");
                eprintln!("  Write them, or change its route in ROUTES and say why.");
                return ExitCode::FAILURE;
            }
            Route::Identities(reason) => {
                if !loaded.is_empty() {
                    eprintln!(
                        "xtask validate: {name} is declared Route::Identities but ships cases."
                    );
                    eprintln!("  A module with an external number to cite belongs in the corpus;");
                    eprintln!("  change its route to Route::Cases and wire an evaluator.");
                    return ExitCode::FAILURE;
                }
                identities.push((name, reason));
                continue;
            }
            Route::NotAModule => continue,
            Route::Cases => {}
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
    println!(
        "identity: {} module(s) validated by identity, not by citation",
        identities.len()
    );
    println!(
        "open cite: {} modelling constant(s) still marked [TO CITE] in source",
        open_citations(&repo)
    );
    println!(
        "verdict : {} pass, {} fail, {} known limit, {} stale",
        summary.pass, summary.fail, summary.known_limit, summary.stale_known_limit
    );
    println!(
        "report  : {}",
        out_dir.strip_prefix(&repo).unwrap_or(&out_dir).display()
    );

    if !identities.is_empty() {
        println!();
        println!("Modules with no external number to cite, and why:");
        for (name, reason) in &identities {
            println!("  {name}");
            for line in wrap(reason, 72) {
                println!("    {line}");
            }
        }
    }

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

/// Load and evaluate the whole case corpus, without writing a report.
///
/// Extracted so that `bench` can gate on the same corpus `validate` certifies,
/// running it through the same evaluators rather than a second implementation
/// that could drift. `validate` keeps its own copy of the loading rules because
/// it also has to report routes and pending modules; this is the summary-only
/// path.
///
/// # Errors
/// The exit code to return, when the corpus itself could not be read.
fn evaluate_corpus(repo: &Path) -> Result<Summary, ExitCode> {
    let crates_dir = repo.join("crates");
    let mut dirs: Vec<PathBuf> = match std::fs::read_dir(&crates_dir) {
        Ok(rd) => rd
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect(),
        Err(e) => {
            eprintln!("xtask: cannot read {}: {e}", crates_dir.display());
            return Err(ExitCode::FAILURE);
        }
    };
    dirs.sort();

    let mut cases: Vec<Case> = Vec::new();
    for crate_dir in &dirs {
        let name = crate_dir.file_name().unwrap().to_string_lossy().to_string();
        if route_of(&name) != Some(Route::Cases) {
            continue;
        }
        match case::load_dir(&crate_dir.join("cases")) {
            Ok(loaded) => cases.extend(loaded),
            Err(e) => {
                eprintln!(
                    "xtask: case file refused
  {e}"
                );
                return Err(ExitCode::FAILURE);
            }
        }
    }

    if cases.len() < MINIMUM_CORPUS {
        eprintln!(
            "xtask: {} wired cases, below the floor of {MINIMUM_CORPUS}; the corpus is not being read.",
            cases.len()
        );
        return Err(ExitCode::FAILURE);
    }

    Ok(cases
        .iter()
        .map(|c| check(c, &evaluate(c)))
        .fold(Summary::default(), |mut s, o| {
            s.record(&o);
            s
        }))
}

/// `cargo xtask bench` — the correctness gate, then the timings.
///
/// Refuses in three situations, each of which would make the numbers a lie:
///
/// 1. **Not an optimised build.** Debug is roughly twenty times slower on this
///    workload, so a debug timing is not a slow measurement, it is a different
///    measurement.
/// 2. **The case corpus does not pass.** ADR-000 D3 and `euler2d.cu` already
///    carry this rule: a benchmark from unvalidated code is worse than no
///    benchmark, because it is a number that looks like evidence.
/// 3. **A dirty tree**, which is a warning rather than a refusal - the report
///    says NOT REPRODUCIBLE in its own header, the same as validation does.
///
/// The debug refusal matters most: a debug timing is not a slow measurement, it
/// is a different one.
fn bench_gated() -> ExitCode {
    let repo = repo_root();

    if cfg!(debug_assertions) {
        eprintln!("xtask bench: this is a debug build and the numbers would be meaningless.");
        eprintln!("  Debug is roughly 20x slower on this workload, which is not a slow");
        eprintln!("  measurement but a different one. Re-run with:");
        eprintln!();
        eprintln!("      cargo run --release -p xtask -- bench");
        return ExitCode::FAILURE;
    }

    println!("gate 1/2: lint");
    if !lint_gate_passes(&repo) {
        return ExitCode::FAILURE;
    }

    println!("gate 2/2: the case corpus must pass at this commit");
    let summary = match evaluate_corpus(&repo) {
        Ok(s) => s,
        Err(code) => return code,
    };
    if summary.breaks_build() {
        eprintln!();
        eprintln!(
            "xtask bench: REFUSING TO TIME ANYTHING. {} case(s) failed and {} known-limit",
            summary.fail, summary.stale_known_limit
        );
        eprintln!("  annotation(s) are stale. A benchmark from code that does not pass its");
        eprintln!("  own corpus is a number that looks like evidence and is not (ADR-000 D3).");
        return ExitCode::FAILURE;
    }
    println!(
        "          {} pass, {} fail, {} known limit - gate open",
        summary.pass, summary.fail, summary.known_limit
    );

    println!();
    println!("timing {} subjects...", 7);
    let measurements = bench::run_all();

    let provenance = Provenance::detect();
    let out_dir = repo.join("out").join(&provenance.run_id);
    if let Err(e) = bench::write_report(&out_dir, &provenance, &measurements) {
        eprintln!("xtask bench: cannot write report: {e}");
        return ExitCode::FAILURE;
    }

    println!();
    for m in &measurements {
        println!(
            "  {:<4} {:<42} {:>12.3?}  (best {:.3?})",
            m.module, m.subject, m.median, m.best
        );
    }
    println!();
    println!(
        "commit : {} ({})",
        provenance.git_hash,
        if provenance.git_dirty {
            "DIRTY - not reproducible"
        } else {
            "clean"
        }
    );
    println!(
        "report : {}",
        out_dir.strip_prefix(&repo).unwrap_or(&out_dir).display()
    );
    ExitCode::SUCCESS
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
            assert_eq!(
                route_of(&name),
                Some(Route::Cases),
                "`{name}` ships cases but is not declared Route::Cases in ROUTES"
            );
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
