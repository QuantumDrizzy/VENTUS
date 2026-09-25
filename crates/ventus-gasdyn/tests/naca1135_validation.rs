//! M2 driven by the validation harness against `cases/`.
//!
//! DUPLICATION NOTE. The evaluator below mirrors `evaluate_gasdyn` in
//! `crates/xtask/src/main.rs`. Two copies is tolerable and deliberate: this one
//! runs under `cargo test`, that one produces the validation report, and neither
//! should depend on the other (the physics crates only dev-depend on
//! `ventus-validate`). The `both_evaluators_see_the_same_cases` test below pins
//! the case count so the two cannot silently diverge in coverage.
//!
//! Three copies would be a pattern rather than a coincidence. When M3 lands,
//! the evaluators move into a shared place instead of being written a third time.

use std::collections::BTreeMap;
use std::path::Path;
use ventus_gasdyn as gasdyn;
use ventus_validate::case::{self, Case, ExpectValue};
use ventus_validate::check::{check, Outcome, Summary};

fn computed(c: &Case) -> BTreeMap<String, ExpectValue> {
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

fn cases() -> Vec<Case> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("cases");
    case::load_dir(&dir).unwrap_or_else(|e| panic!("case file refused:\n  {e}"))
}

#[test]
fn m2_matches_the_naca1135_tables() {
    let cases = cases();
    let mut summary = Summary::default();
    let mut report = String::new();

    for c in &cases {
        let outcome = check(c, &computed(c));
        summary.record(&outcome);
        if let Outcome::Fail {
            mismatches,
            missing,
        } = &outcome
        {
            report.push_str(&format!("\n{}\n  source: {}\n", c.name, c.source));
            for m in mismatches {
                report.push_str(&format!(
                    "  {}: expected {}, got {} (rel {:.3e})\n",
                    m.key, m.expected, m.actual, m.rel_err
                ));
            }
            for k in missing {
                report.push_str(&format!("  {k}: never computed\n"));
            }
        }
    }

    assert!(
        !summary.breaks_build(),
        "{} of {} M2 cases failed:{report}",
        summary.fail,
        summary.total()
    );

    // The three documented limits of a calorically perfect model: the stagnation
    // state and the post-shock state at the design point, plus the freestream
    // being colder than the cp correlation is fitted for.
    assert_eq!(
        summary.known_limit, 3,
        "expected exactly the three documented gamma limits, got {}",
        summary.known_limit
    );
    assert_eq!(
        summary.stale_known_limit, 0,
        "a known-limit annotation now passes and should be removed"
    );
    assert!(
        summary.pass >= 20,
        "expected the full NACA table, got {}",
        summary.pass
    );
}

/// The cases are only worth having if they would notice M2 being wrong.
///
/// The perturbation is derived from each case's own tolerance rather than being
/// a fixed percentage. A fixed 0.1 % was tried first and this test caught it:
/// the burner-gamma case carries `rel_tol = 1e-2` because published cp values
/// genuinely span that much, so a 0.1 % error is inside its tolerance and the
/// case rightly still passed. A perturbation that does not exceed the tolerance
/// it is probing tests nothing.
#[test]
fn the_cases_would_catch_a_broken_m2() {
    for c in &cases() {
        let factor = 1.0 + 10.0 * c.tol.rel.unwrap_or(1e-3).max(1e-4);
        let mut wrong = computed(c);
        for v in wrong.values_mut() {
            if let ExpectValue::Float(x) = v {
                *x = if *x == 0.0 { 1.0 } else { *x * factor };
            }
        }
        let outcome = check(c, &wrong);
        assert!(
            !matches!(outcome, Outcome::Pass | Outcome::StaleKnownLimit),
            "case `{}` survived a {:.3} % perturbation: {}",
            c.name,
            (factor - 1.0) * 100.0,
            outcome.label()
        );
    }
}

/// Guards the duplication noted at the top of this file: if a case file is added
/// to one evaluator's view and not the other, the counts diverge and this fails.
#[test]
fn both_evaluators_see_the_same_cases() {
    let cases = cases();
    assert_eq!(
        cases.len(),
        30,
        "case count changed; update the xtask evaluator and this number together"
    );
    // Every case must be answerable in at least one key, or the evaluator is
    // missing a branch rather than the module being wrong.
    for c in &cases {
        let got = computed(c);
        let answerable = c.expect.keys().any(|k| got.contains_key(k));
        assert!(
            answerable || c.status == ventus_validate::Status::KnownLimit,
            "case `{}` names keys the evaluator never produces: {:?}",
            c.name,
            c.expect.keys().collect::<Vec<_>>()
        );
    }
}
