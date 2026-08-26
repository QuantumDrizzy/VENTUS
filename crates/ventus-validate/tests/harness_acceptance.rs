//! Acceptance for the harness itself (ADR-000 step 0).
//!
//! The harness is the only module that can legitimately be validated against
//! itself, and only because the tests are NEGATIVE: they assert that it
//! rejects and fails on things it must reject and fail on.
//!
//! Criterion 4 is the one that makes the other three mean anything. A harness
//! that returned `Fail` unconditionally, and refused every file, would satisfy
//! criteria 1 and 2 perfectly. The positive control is what rules that out.

use std::collections::BTreeMap;
use std::path::Path;
use ventus_validate::case::{self, LoadError, Status};
use ventus_validate::check::{check, Outcome};
use ventus_validate::{computed, ExpectValue};

fn origin() -> &'static Path {
    Path::new("<test>")
}

fn one(text: &str) -> Result<case::Case, LoadError> {
    case::load_str(text, origin()).map(|mut v| {
        assert_eq!(v.len(), 1, "fixture must contain exactly one case");
        v.remove(0)
    })
}

const GOOD: &str = r#"
[[case]]
name    = "us76_h20000m"
source  = "NASA-TM-X-74335 (U.S. Standard Atmosphere 1976), Table I, h = 20 km"
inputs  = { altitude_m = 20000.0 }
expect  = { temperature_k = 216.650, pressure_pa = 5474.89 }
rel_tol = 1e-6
"#;

// ---------------------------------------------------------------------------
// Criterion 4 — positive control. Without this the rest proves nothing.
// ---------------------------------------------------------------------------

#[test]
fn c4_a_true_case_passes() {
    let c = one(GOOD).expect("valid case must load");
    assert!(!c.source.is_empty());
    let out = check(
        &c,
        &computed! { "temperature_k" => 216.650_f64, "pressure_pa" => 5474.89_f64 },
    );
    assert!(matches!(out, Outcome::Pass), "got {}", out.label());
    assert!(!out.breaks_build());
}

#[test]
fn c4_a_value_inside_tolerance_passes() {
    let c = one(GOOD).unwrap();
    // 216.650 * (1 + 5e-7): inside rel_tol = 1e-6.
    let out = check(
        &c,
        &computed! { "temperature_k" => 216.650_108_f64, "pressure_pa" => 5474.89_f64 },
    );
    assert!(matches!(out, Outcome::Pass), "got {}", out.label());
}

// ---------------------------------------------------------------------------
// Criterion 1 — a deliberately false case must report failure.
// ---------------------------------------------------------------------------

#[test]
fn c1_a_false_case_fails_and_breaks_the_build() {
    let c = one(GOOD).unwrap();
    let out = check(
        &c,
        &computed! { "temperature_k" => 300.0_f64, "pressure_pa" => 5474.89_f64 },
    );
    let Outcome::Fail {
        mismatches,
        missing,
    } = &out
    else {
        panic!("expected Fail, got {}", out.label());
    };
    assert!(missing.is_empty());
    assert_eq!(mismatches.len(), 1);
    assert_eq!(mismatches[0].key, "temperature_k");
    assert!((mismatches[0].rel_err - 0.384_722).abs() < 1e-4);
    assert!(out.breaks_build());
}

#[test]
fn c1_a_value_just_outside_tolerance_fails() {
    let c = one(GOOD).unwrap();
    // 216.650 * (1 + 2e-6): outside rel_tol = 1e-6. The boundary is where a
    // sloppy comparison hides.
    let out = check(
        &c,
        &computed! { "temperature_k" => 216.650_433_f64, "pressure_pa" => 5474.89_f64 },
    );
    assert!(out.breaks_build(), "got {}", out.label());
}

#[test]
fn c1_a_missing_computed_key_fails() {
    let c = one(GOOD).unwrap();
    let out = check(&c, &computed! { "temperature_k" => 216.650_f64 });
    let Outcome::Fail { missing, .. } = &out else {
        panic!("expected Fail, got {}", out.label());
    };
    assert_eq!(missing, &["pressure_pa"]);
}

#[test]
fn c1_nan_never_passes_any_tolerance() {
    let text = r#"
[[case]]
name    = "nan_guard"
source  = "harness acceptance, ADR-000 step 0"
expect  = { x = 0.0 }
rel_tol = 1e9
abs_tol = 1e9
ulp_tol = 9223372036854775807
"#;
    let c = one(text).unwrap();
    // Tolerances wide enough to admit anything finite. NaN must still fail.
    let out = check(&c, &computed! { "x" => f64::NAN });
    assert!(out.breaks_build(), "NaN slipped through: {}", out.label());
    // Sanity: the same absurd tolerances DO admit a finite value, so the test
    // above is not passing for the wrong reason.
    let out = check(&c, &computed! { "x" => 1.0e8_f64 });
    assert!(matches!(out, Outcome::Pass), "got {}", out.label());
}

#[test]
fn c1_a_type_mismatch_is_never_tolerance_forgiven() {
    let text = r#"
[[case]]
name    = "type_guard"
source  = "harness acceptance, ADR-000 step 0"
expect  = { flag = true }
rel_tol = 1e9
"#;
    let c = one(text).unwrap();
    let out = check(&c, &computed! { "flag" => 1.0_f64 });
    assert!(out.breaks_build(), "float matched a bool: {}", out.label());
    let out = check(&c, &computed! { "flag" => false });
    assert!(out.breaks_build(), "false matched true");
    let out = check(&c, &computed! { "flag" => true });
    assert!(matches!(out, Outcome::Pass));
}

// ---------------------------------------------------------------------------
// Criterion 2 — a case with no `source` must be refused at load.
// ---------------------------------------------------------------------------

#[test]
fn c2_missing_source_is_refused() {
    let text = r#"
[[case]]
name    = "no_source"
expect  = { temperature_k = 216.65 }
rel_tol = 1e-6
"#;
    let err = one(text).expect_err("a case with no source must be refused");
    assert!(
        matches!(&err, LoadError::MissingSource { case, .. } if case == "no_source"),
        "wrong error: {err:?}"
    );
    // The message must say why, not just that.
    let msg = err.to_string();
    assert!(
        msg.contains("no yardstick, no module"),
        "unhelpful message: {msg}"
    );
}

#[test]
fn c2_blank_source_is_refused() {
    for blank in ["\"\"", "\"   \"", "\"\\n\\t \""] {
        let text = format!(
            "[[case]]\nname = \"blank\"\nsource = {blank}\n\
             expect = {{ x = 1.0 }}\nrel_tol = 1e-6\n"
        );
        let err = one(&text).expect_err("blank source must be refused");
        assert!(
            matches!(err, LoadError::EmptySource { .. }),
            "blank {blank} gave {err:?}"
        );
    }
}

#[test]
fn c2_a_typo_in_source_does_not_slip_through_as_valid() {
    // `deny_unknown_fields` turns a near-miss field name into a parse error
    // rather than a silently source-less case.
    let text = r#"
[[case]]
name    = "typo"
sources = "NACA 1135"
expect  = { x = 1.0 }
rel_tol = 1e-6
"#;
    let err = one(text).expect_err("unknown field must be refused");
    assert!(
        matches!(err, LoadError::Parse { .. }),
        "wrong error: {err:?}"
    );
}

#[test]
fn c2_known_limit_without_reason_is_refused() {
    let text = r#"
[[case]]
name    = "silent_limit"
source  = "harness acceptance, ADR-000 step 0"
status  = "known_limit"
expect  = { x = 1.0 }
rel_tol = 1e-6
"#;
    let err = one(text).expect_err("a limit with no reason must be refused");
    assert!(
        matches!(err, LoadError::MissingReason { .. }),
        "wrong error: {err:?}"
    );
    assert!(err.to_string().contains("hidden limit"));
}

#[test]
fn c2_case_with_no_tolerance_is_refused() {
    let text = r#"
[[case]]
name    = "no_tolerance"
source  = "harness acceptance, ADR-000 step 0"
expect  = { x = 1.0 }
"#;
    let err = one(text).expect_err("a case with no tolerance asserts nothing");
    assert!(
        matches!(err, LoadError::NoTolerance { .. }),
        "wrong error: {err:?}"
    );
}

#[test]
fn c2_empty_expect_is_refused() {
    let text = r#"
[[case]]
name    = "asserts_nothing"
source  = "harness acceptance, ADR-000 step 0"
expect  = {}
rel_tol = 1e-6
"#;
    let err = one(text).expect_err("an empty expect asserts nothing");
    assert!(
        matches!(err, LoadError::EmptyExpect { .. }),
        "wrong error: {err:?}"
    );
}

#[test]
fn c2_unsupported_expect_type_is_refused() {
    let text = r#"
[[case]]
name    = "stringly_typed"
source  = "harness acceptance, ADR-000 step 0"
expect  = { label = "hot" }
rel_tol = 1e-6
"#;
    let err = one(text).expect_err("string expectations are not supported");
    assert!(
        matches!(&err, LoadError::UnsupportedExpect { key, kind, .. }
                 if key == "label" && *kind == "string"),
        "wrong error: {err:?}"
    );
}

#[test]
fn c2_negative_or_nonfinite_tolerance_is_refused() {
    for bad in ["-1e-6", "nan", "inf"] {
        let text = format!(
            "[[case]]\nname = \"bad_tol\"\nsource = \"acceptance\"\n\
             expect = {{ x = 1.0 }}\nrel_tol = {bad}\n"
        );
        let err = one(&text).expect_err("bad tolerance must be refused");
        assert!(
            matches!(err, LoadError::BadTolerance { .. }),
            "{bad} gave {err:?}"
        );
    }
}

#[test]
fn c2_missing_name_is_refused_and_still_names_the_file() {
    let text = r#"
[[case]]
source  = "harness acceptance, ADR-000 step 0"
expect  = { x = 1.0 }
rel_tol = 1e-6
"#;
    let err = one(text).expect_err("a nameless case must be refused");
    assert!(
        matches!(err, LoadError::MissingName { index: 0, .. }),
        "wrong error: {err:?}"
    );
    assert_eq!(err.file(), origin());
}

// ---------------------------------------------------------------------------
// known_limit semantics
// ---------------------------------------------------------------------------

#[test]
fn a_known_limit_that_fails_is_visible_but_does_not_break_the_build() {
    let text = r#"
[[case]]
name    = "perfect_gas_stagnation"
source  = "docs/design-point.md section 3.1"
status  = "known_limit"
reason  = "Calorically perfect gamma = 1.4 gives 617.8 K against the thermally perfect 612.0 K."
expect  = { stagnation_temperature_k = 612.0 }
rel_tol = 1e-3
"#;
    let c = one(text).unwrap();
    assert_eq!(c.status, Status::KnownLimit);
    let out = check(&c, &computed! { "stagnation_temperature_k" => 617.8_f64 });
    let Outcome::KnownLimit { mismatches, .. } = &out else {
        panic!("expected KnownLimit, got {}", out.label());
    };
    assert!(
        (mismatches[0].rel_err - 9.477e-3).abs() < 1e-5,
        "{}",
        mismatches[0].rel_err
    );
    assert!(
        !out.breaks_build(),
        "a known limit must not break the build"
    );
}

#[test]
fn a_known_limit_that_passes_is_reported_as_stale() {
    let text = r#"
[[case]]
name    = "limit_that_went_away"
source  = "docs/design-point.md section 3.1"
status  = "known_limit"
reason  = "Was failing before the gamma(T) model landed."
expect  = { stagnation_temperature_k = 612.0 }
rel_tol = 1e-3
"#;
    let c = one(text).unwrap();
    let out = check(&c, &computed! { "stagnation_temperature_k" => 612.0_f64 });
    assert!(
        matches!(out, Outcome::StaleKnownLimit),
        "a limit that no longer reproduces must be flagged, got {}",
        out.label()
    );
    assert!(!out.breaks_build());
}

// ---------------------------------------------------------------------------
// The corpus: every case file checked into this repository must load.
// ---------------------------------------------------------------------------

#[test]
fn every_case_file_in_the_repo_satisfies_the_schema() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
    let crates = repo.join("crates");
    let mut files = 0;
    let mut cases = 0;

    for entry in std::fs::read_dir(&crates).expect("crates/ must exist") {
        let dir = entry.unwrap().path().join("cases");
        if !dir.is_dir() {
            continue;
        }
        for f in std::fs::read_dir(&dir).unwrap() {
            let path = f.unwrap().path();
            if path.extension().is_none_or(|e| e != "toml") {
                continue;
            }
            files += 1;
            let loaded = case::load_file(&path)
                .unwrap_or_else(|e| panic!("checked-in case file rejected:\n  {e}"));
            assert!(!loaded.is_empty(), "{} contains no cases", path.display());
            for c in &loaded {
                assert!(!c.source.trim().is_empty());
                if c.status != Status::Normal {
                    assert!(c.reason.as_ref().is_some_and(|r| !r.trim().is_empty()));
                }
            }
            cases += loaded.len();
        }
    }

    assert!(
        files >= 2,
        "expected at least the M1 and M2 case files, found {files}"
    );
    assert!(
        cases >= 5,
        "expected at least 5 checked-in cases, found {cases}"
    );
}

#[test]
fn load_dir_is_deterministic_and_tolerates_a_missing_directory() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
    let atmos = repo.join("crates").join("ventus-atmos").join("cases");
    let a = case::load_dir(&atmos).unwrap();
    let b = case::load_dir(&atmos).unwrap();
    let names_a: Vec<&str> = a.iter().map(|c| c.name.as_str()).collect();
    let names_b: Vec<&str> = b.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names_a, names_b, "case ordering must be stable across runs");
    assert!(!names_a.is_empty());

    let missing = repo
        .join("crates")
        .join("ventus-fsw")
        .join("cases_does_not_exist");
    assert!(case::load_dir(&missing).unwrap().is_empty());
}

// ---------------------------------------------------------------------------
// Report
// ---------------------------------------------------------------------------

#[test]
fn report_header_carries_run_identity() {
    use ventus_validate::report::{markdown, Entry, Provenance};

    let c = one(GOOD).unwrap();
    let out = check(&c, &BTreeMap::<String, ExpectValue>::new());
    let prov = Provenance {
        run_id: "20260827T120000Z-abc123def456".into(),
        git_hash: "abc123def456".into(),
        git_dirty: true,
        timestamp_utc: "2026-08-27T12:00:00Z".into(),
        rustc: "rustc 1.91.1".into(),
    };
    let md = markdown(
        &prov,
        &[Entry {
            case: &c,
            outcome: &out,
        }],
    );

    // ADR-000 D4 r2: a report without run identity is not reproducible.
    assert!(md.contains("20260827T120000Z-abc123def456"));
    assert!(md.contains("abc123def456"));
    assert!(md.contains("DIRTY"), "a dirty tree must be stated loudly");
    assert!(md.contains("Verdict: **FAIL**"));
    // The citation must survive into the report; that is the whole point.
    assert!(md.contains("NASA-TM-X-74335"));
}
