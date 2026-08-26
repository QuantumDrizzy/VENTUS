//! Comparing a module's computed values against a case's expectations.

use crate::case::{Case, ExpectValue, Status, Tolerance};
use std::collections::BTreeMap;
use ventus_units::float;

/// One expectation that did not hold.
#[derive(Debug, Clone)]
pub struct Mismatch {
    pub key: String,
    pub expected: ExpectValue,
    pub actual: ExpectValue,
    /// Relative error, for float mismatches. `f64::NAN` for bool mismatches.
    pub rel_err: f64,
    /// ULP distance, when both values are floats and neither is NaN.
    pub ulp: Option<u64>,
}

/// The result of checking one case.
#[derive(Debug, Clone)]
pub enum Outcome {
    Pass,
    /// Breaks the build.
    Fail {
        mismatches: Vec<Mismatch>,
        missing: Vec<String>,
    },
    /// Expected to fail, and it did. Visible in the report, does not break the
    /// build (ADR-000 D4).
    KnownLimit {
        mismatches: Vec<Mismatch>,
        missing: Vec<String>,
    },
    /// A case marked `known_limit` that now passes. The limit no longer
    /// reproduces, so the annotation is stale and should be removed. Reported
    /// loudly; does not break the build, because the code got better, not worse.
    StaleKnownLimit,
}

impl Outcome {
    /// Only a `Fail` stops the build. This is the single place that decision is
    /// made, so it cannot drift between the runner and the report.
    #[must_use]
    pub fn breaks_build(&self) -> bool {
        matches!(self, Outcome::Fail { .. })
    }

    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Outcome::Pass => "PASS",
            Outcome::Fail { .. } => "FAIL",
            Outcome::KnownLimit { .. } => "KNOWN_LIMIT",
            Outcome::StaleKnownLimit => "STALE_KNOWN_LIMIT",
        }
    }

    /// Worst relative error seen, for report ordering. `0.0` when there is none.
    #[must_use]
    pub fn worst_rel_err(&self) -> f64 {
        match self {
            Outcome::Pass | Outcome::StaleKnownLimit => 0.0,
            Outcome::Fail { mismatches, .. } | Outcome::KnownLimit { mismatches, .. } => {
                mismatches.iter().map(|m| m.rel_err).fold(0.0, f64::max)
            }
        }
    }
}

/// Does `actual` satisfy `tol` against `expected`?
///
/// Passes if **any** supplied tolerance is met. A NaN never passes: `rel_err`
/// returns infinity and `ulp_diff` returns `None` for NaN inputs.
fn float_ok(actual: f64, expected: f64, tol: &Tolerance) -> (bool, f64, Option<u64>) {
    let rel = float::rel_err(actual, expected);
    let ulp = float::ulp_diff(actual, expected);
    let abs = float::abs(actual - expected);

    let mut ok = false;
    if let Some(t) = tol.rel {
        ok |= rel <= t;
    }
    if let Some(t) = tol.abs {
        // NaN must not slip through an absolute check either.
        ok |= abs <= t && !actual.is_nan() && !expected.is_nan();
    }
    if let Some(t) = tol.ulp {
        ok |= ulp.is_some_and(|u| u <= t);
    }
    (ok, rel, ulp)
}

/// Check one case against a module's computed values.
///
/// Keys present in `computed` but absent from `case.expect` are ignored: a
/// module may legitimately compute more than any single case asserts. Keys
/// present in `case.expect` but absent from `computed` are a failure — the case
/// asserts something the module never produced.
#[must_use]
pub fn check(case: &Case, computed: &BTreeMap<String, ExpectValue>) -> Outcome {
    let mut mismatches = Vec::new();
    let mut missing = Vec::new();

    for (key, expected) in &case.expect {
        let Some(actual) = computed.get(key) else {
            missing.push(key.clone());
            continue;
        };

        match (*expected, *actual) {
            (ExpectValue::Float(e), ExpectValue::Float(a)) => {
                let (ok, rel, ulp) = float_ok(a, e, &case.tol);
                if !ok {
                    mismatches.push(Mismatch {
                        key: key.clone(),
                        expected: *expected,
                        actual: *actual,
                        rel_err: rel,
                        ulp,
                    });
                }
            }
            (ExpectValue::Bool(e), ExpectValue::Bool(a)) => {
                if e != a {
                    mismatches.push(Mismatch {
                        key: key.clone(),
                        expected: *expected,
                        actual: *actual,
                        rel_err: f64::NAN,
                        ulp: None,
                    });
                }
            }
            // A type mismatch is always a failure, never tolerance-forgiven.
            _ => mismatches.push(Mismatch {
                key: key.clone(),
                expected: *expected,
                actual: *actual,
                rel_err: f64::NAN,
                ulp: None,
            }),
        }
    }

    let clean = mismatches.is_empty() && missing.is_empty();
    match (case.status, clean) {
        (_, true) if case.status == Status::KnownLimit => Outcome::StaleKnownLimit,
        (_, true) => Outcome::Pass,
        (Status::Normal, false) => Outcome::Fail {
            mismatches,
            missing,
        },
        (Status::KnownLimit, false) => Outcome::KnownLimit {
            mismatches,
            missing,
        },
    }
}

/// Tally over a run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Summary {
    pub pass: usize,
    pub fail: usize,
    pub known_limit: usize,
    pub stale_known_limit: usize,
}

impl Summary {
    #[must_use]
    pub fn total(&self) -> usize {
        self.pass + self.fail + self.known_limit + self.stale_known_limit
    }

    #[must_use]
    pub fn breaks_build(&self) -> bool {
        self.fail > 0
    }

    pub fn record(&mut self, outcome: &Outcome) {
        match outcome {
            Outcome::Pass => self.pass += 1,
            Outcome::Fail { .. } => self.fail += 1,
            Outcome::KnownLimit { .. } => self.known_limit += 1,
            Outcome::StaleKnownLimit => self.stale_known_limit += 1,
        }
    }
}

impl FromIterator<Outcome> for Summary {
    fn from_iter<I: IntoIterator<Item = Outcome>>(iter: I) -> Self {
        let mut s = Summary::default();
        for o in iter {
            s.record(&o);
        }
        s
    }
}
