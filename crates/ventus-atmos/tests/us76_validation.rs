//! M1 driven by the validation harness against `cases/us76_layers.toml`.
//!
//! Every case is a citation to NASA-TM-X-74335. The harness refuses to load one
//! without a source, so this test cannot silently drift into checking M1
//! against itself.

use std::collections::BTreeMap;
use std::path::Path;
use ventus_atmos as atmos;
use ventus_validate::case::{self, Case, ExpectValue};
use ventus_validate::check::{check, Outcome, Summary};

/// Compute the full state for a case and expose every field. The harness
/// ignores keys a case does not name, so one mapping serves every case and no
/// per-case special handling is needed.
fn computed(case: &Case) -> BTreeMap<String, ExpectValue> {
    let state = if let Some(h) = case.inputs.get("geopotential_altitude_m") {
        atmos::at_geopotential(
            h.as_float()
                .expect("geopotential_altitude_m must be a float"),
        )
    } else if let Some(z) = case.inputs.get("geometric_altitude_m") {
        atmos::at_geometric(z.as_float().expect("geometric_altitude_m must be a float"))
    } else {
        panic!("case `{}` names no altitude input", case.name);
    }
    .unwrap_or_else(|e| panic!("case `{}`: {e:?}", case.name));

    let mut m = BTreeMap::new();
    m.insert(
        "geopotential_altitude_m".into(),
        ExpectValue::Float(state.geopotential_altitude_m),
    );
    m.insert(
        "geometric_altitude_m".into(),
        ExpectValue::Float(state.geometric_altitude_m),
    );
    m.insert(
        "temperature_k".into(),
        ExpectValue::Float(state.temperature_k),
    );
    m.insert("pressure_pa".into(), ExpectValue::Float(state.pressure_pa));
    m.insert(
        "density_kg_m3".into(),
        ExpectValue::Float(state.density_kg_m3),
    );
    m.insert(
        "speed_of_sound_m_s".into(),
        ExpectValue::Float(state.speed_of_sound_m_s),
    );
    m.insert(
        "dynamic_viscosity_pa_s".into(),
        ExpectValue::Float(state.dynamic_viscosity_pa_s),
    );
    m.insert(
        "kinematic_viscosity_m2_s".into(),
        ExpectValue::Float(state.kinematic_viscosity_m2_s()),
    );
    m.insert(
        "lapse_rate_k_per_m".into(),
        ExpectValue::Float(state.lapse_rate_k_per_m),
    );
    m
}

fn cases() -> Vec<Case> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("cases");
    case::load_dir(&dir).unwrap_or_else(|e| panic!("case file refused:\n  {e}"))
}

#[test]
fn m1_matches_the_published_us76_table() {
    let cases = cases();
    assert!(
        cases.len() >= 15,
        "expected the full layer table, found {}",
        cases.len()
    );

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
            report.push_str(&format!(
                "\n{} [{}]\n  source: {}\n",
                c.name,
                c.file.display(),
                c.source
            ));
            for m in mismatches {
                report.push_str(&format!(
                    "  {}: expected {}, got {} (rel {:.3e}, {} ulp)\n",
                    m.key,
                    m.expected,
                    m.actual,
                    m.rel_err,
                    m.ulp.map_or("-".to_string(), |u| u.to_string())
                ));
            }
            for k in missing {
                report.push_str(&format!("  {k}: never computed\n"));
            }
        }
    }

    assert!(
        !summary.breaks_build(),
        "{} of {} US76 cases failed:{report}",
        summary.fail,
        summary.total()
    );

    // Two density values do not reproduce in their last printed digit and are
    // annotated `known_limit` with `[TO VERIFY]` rather than having their
    // tolerances widened to fit the implementation. See cases/us76_density.toml.
    // If a primary copy of the table later resolves them, the harness reports
    // STALE_KNOWN_LIMIT and the annotations come out — which this asserts.
    assert_eq!(
        summary.stale_known_limit, 0,
        "a known-limit annotation now passes and should be removed"
    );
    assert_eq!(
        summary.known_limit, 2,
        "expected exactly the two documented [TO VERIFY] density cases, got {}",
        summary.known_limit
    );
    assert_eq!(summary.pass + summary.known_limit, cases.len());
}

/// The harness is only meaningful if it would notice M1 being wrong. Perturb
/// the computed values and confirm every case turns red.
#[test]
fn the_cases_would_catch_a_broken_m1() {
    for c in &cases() {
        let mut wrong = computed(c);
        for v in wrong.values_mut() {
            if let ExpectValue::Float(f) = v {
                // 0.1 % — far larger than any tolerance in the file, far smaller
                // than an obviously absurd value.
                *f = if *f == 0.0 { 1.0 } else { *f * 1.001 };
            }
        }
        let outcome = check(c, &wrong);
        // A `known_limit` case reports KnownLimit rather than Fail, so
        // `breaks_build` is the wrong question here. What must never happen is
        // that a perturbed M1 looks correct: neither Pass nor StaleKnownLimit.
        assert!(
            !matches!(outcome, Outcome::Pass | Outcome::StaleKnownLimit),
            "case `{}` survived a 0.1 % perturbation: {}",
            c.name,
            outcome.label()
        );
    }
}
