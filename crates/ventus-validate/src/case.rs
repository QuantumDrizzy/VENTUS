//! Case files: the contract between a physics module and its external yardstick.
//!
//! ADR-000 D4. Loading is deliberately split in two:
//!   1. permissive deserialisation into [`RawCase`] (every field optional),
//!   2. an explicit validation pass into [`Case`].
//!
//! The split exists so that "a case with no source is refused" is a line of
//! code someone can read and point at, rather than an emergent property of
//! serde's missing-field handling. It also lets the error name the offending
//! case and file.

use serde::Deserialize;
use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

/// How a failing case is treated.
///
/// `KnownLimit` means: **this case is expected to fail**, and the failure is
/// accepted and explained in `reason`. It is therefore news when one passes —
/// see [`crate::check::Outcome::StaleKnownLimit`]. It does not mean "this file
/// documents a limitation"; documentation of a limitation that the module is
/// nonetheless expected to reproduce correctly belongs on a `Normal` case with
/// a `reason`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// A failure breaks the build.
    #[default]
    Normal,
    /// A failure is visible in the report but does not break the build.
    KnownLimit,
}

impl Status {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Normal => "normal",
            Status::KnownLimit => "known_limit",
        }
    }
}

/// A value a case can assert. Deliberately narrow: scalars and booleans only.
/// Structured expectations belong in `inputs`, or in a purpose-built case type.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExpectValue {
    Float(f64),
    Bool(bool),
}

impl ExpectValue {
    #[must_use]
    pub fn kind(self) -> &'static str {
        match self {
            ExpectValue::Float(_) => "float",
            ExpectValue::Bool(_) => "bool",
        }
    }
}

impl fmt::Display for ExpectValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExpectValue::Float(v) => write!(f, "{v}"),
            ExpectValue::Bool(v) => write!(f, "{v}"),
        }
    }
}

/// Tolerances. At least one must be supplied: a case with no tolerance asserts
/// nothing and would silently pass forever.
///
/// A float matches if it satisfies **any** supplied tolerance. `abs` exists to
/// rescue comparisons against zero, where relative error is undefined; `ulp` is
/// for closed-form relations where the only acceptable answer is the exact one.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Tolerance {
    pub rel: Option<f64>,
    pub abs: Option<f64>,
    pub ulp: Option<u64>,
}

impl Tolerance {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rel.is_none() && self.abs.is_none() && self.ulp.is_none()
    }
}

/// A validated case.
#[derive(Debug, Clone)]
pub struct Case {
    pub name: String,
    /// Mandatory citation. No yardstick, no module (ADR-000 D4).
    pub source: String,
    pub status: Status,
    pub reason: Option<String>,
    pub inputs: toml::Table,
    pub expect: BTreeMap<String, ExpectValue>,
    pub tol: Tolerance,
    pub file: PathBuf,
}

#[derive(Debug, Deserialize)]
struct RawCaseFile {
    #[serde(default)]
    case: Vec<RawCase>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCase {
    name: Option<String>,
    source: Option<String>,
    #[serde(default)]
    status: Status,
    reason: Option<String>,
    #[serde(default)]
    inputs: toml::Table,
    #[serde(default)]
    expect: toml::Table,
    rel_tol: Option<f64>,
    abs_tol: Option<f64>,
    ulp_tol: Option<u64>,
}

/// Why a case file was refused. Every variant names the file, and every variant
/// that can names the case, so a rejection is actionable without opening a debugger.
#[derive(Debug)]
pub enum LoadError {
    Io {
        file: PathBuf,
        source: std::io::Error,
    },
    Parse {
        file: PathBuf,
        source: Box<toml::de::Error>,
    },
    MissingName {
        file: PathBuf,
        index: usize,
    },
    MissingSource {
        file: PathBuf,
        case: String,
    },
    EmptySource {
        file: PathBuf,
        case: String,
    },
    MissingReason {
        file: PathBuf,
        case: String,
        status: Status,
    },
    EmptyExpect {
        file: PathBuf,
        case: String,
    },
    UnsupportedExpect {
        file: PathBuf,
        case: String,
        key: String,
        kind: &'static str,
    },
    NoTolerance {
        file: PathBuf,
        case: String,
    },
    BadTolerance {
        file: PathBuf,
        case: String,
        detail: String,
    },
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::Io { file, source } => {
                write!(f, "{}: {source}", file.display())
            }
            LoadError::Parse { file, source } => {
                write!(f, "{}: malformed TOML: {source}", file.display())
            }
            LoadError::MissingName { file, index } => {
                write!(f, "{}: case #{index} has no `name`", file.display())
            }
            LoadError::MissingSource { file, case } => write!(
                f,
                "{}: case `{case}` has no `source`. Every case must cite an \
                 external yardstick (ADR-000 D4): no yardstick, no module.",
                file.display()
            ),
            LoadError::EmptySource { file, case } => write!(
                f,
                "{}: case `{case}` has an empty `source`. A blank citation is \
                 not a citation (ADR-000 D4).",
                file.display()
            ),
            LoadError::MissingReason { file, case, status } => write!(
                f,
                "{}: case `{case}` has status `{}` but no `reason`. A limit \
                 without a stated reason is a hidden limit (ADR-000 D4).",
                file.display(),
                status.as_str()
            ),
            LoadError::EmptyExpect { file, case } => write!(
                f,
                "{}: case `{case}` has an empty `expect`; it asserts nothing.",
                file.display()
            ),
            LoadError::UnsupportedExpect {
                file,
                case,
                key,
                kind,
            } => write!(
                f,
                "{}: case `{case}`, key `{key}`: expected a float or bool, got {kind}.",
                file.display()
            ),
            LoadError::NoTolerance { file, case } => write!(
                f,
                "{}: case `{case}` supplies no tolerance (`rel_tol`, `abs_tol` \
                 or `ulp_tol`); it would pass unconditionally.",
                file.display()
            ),
            LoadError::BadTolerance { file, case, detail } => {
                write!(f, "{}: case `{case}`: {detail}", file.display())
            }
        }
    }
}

impl std::error::Error for LoadError {}

impl LoadError {
    /// The file the rejection came from.
    #[must_use]
    pub fn file(&self) -> &Path {
        match self {
            LoadError::Io { file, .. }
            | LoadError::Parse { file, .. }
            | LoadError::MissingName { file, .. }
            | LoadError::MissingSource { file, .. }
            | LoadError::EmptySource { file, .. }
            | LoadError::MissingReason { file, .. }
            | LoadError::EmptyExpect { file, .. }
            | LoadError::UnsupportedExpect { file, .. }
            | LoadError::NoTolerance { file, .. }
            | LoadError::BadTolerance { file, .. } => file,
        }
    }
}

/// Parse and validate a case file from a string. `origin` is used only for
/// error messages, so this is testable without touching the filesystem.
pub fn load_str(text: &str, origin: &Path) -> Result<Vec<Case>, LoadError> {
    let raw: RawCaseFile = toml::from_str(text).map_err(|e| LoadError::Parse {
        file: origin.to_path_buf(),
        source: Box::new(e),
    })?;

    let mut out = Vec::with_capacity(raw.case.len());
    for (index, rc) in raw.case.into_iter().enumerate() {
        out.push(validate(rc, index, origin)?);
    }
    Ok(out)
}

/// Read, parse and validate a case file.
pub fn load_file(path: &Path) -> Result<Vec<Case>, LoadError> {
    let text = std::fs::read_to_string(path).map_err(|e| LoadError::Io {
        file: path.to_path_buf(),
        source: e,
    })?;
    load_str(&text, path)
}

/// Load every `*.toml` in a directory, sorted by file name for a stable report
/// ordering. A missing directory yields an empty list, not an error: a module
/// that has not written its cases yet is caught by the "zero cases" check in
/// the runner, with a better message than a filesystem error.
pub fn load_dir(dir: &Path) -> Result<Vec<Case>, LoadError> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| LoadError::Io {
            file: dir.to_path_buf(),
            source: e,
        })?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "toml"))
        .collect();
    files.sort();

    let mut out = Vec::new();
    for f in files {
        out.extend(load_file(&f)?);
    }
    Ok(out)
}

fn validate(rc: RawCase, index: usize, origin: &Path) -> Result<Case, LoadError> {
    let file = origin.to_path_buf();

    let name = match rc.name {
        Some(n) if !n.trim().is_empty() => n,
        _ => return Err(LoadError::MissingName { file, index }),
    };

    // ADR-000 D4: this is the rule the whole harness exists to enforce.
    let source = match rc.source {
        None => return Err(LoadError::MissingSource { file, case: name }),
        Some(s) if s.trim().is_empty() => return Err(LoadError::EmptySource { file, case: name }),
        Some(s) => s,
    };

    let reason = match (rc.status, rc.reason) {
        (Status::Normal, r) => r,
        (status, None) => {
            return Err(LoadError::MissingReason {
                file,
                case: name,
                status,
            })
        }
        (status, Some(r)) if r.trim().is_empty() => {
            return Err(LoadError::MissingReason {
                file,
                case: name,
                status,
            })
        }
        (_, Some(r)) => Some(r),
    };

    if rc.expect.is_empty() {
        return Err(LoadError::EmptyExpect { file, case: name });
    }

    let mut expect = BTreeMap::new();
    for (key, value) in rc.expect {
        let ev = match value {
            toml::Value::Float(v) => ExpectValue::Float(v),
            // TOML distinguishes `0` from `0.0`; accept both so a case file is
            // not rejected for writing an exact integer.
            toml::Value::Integer(v) => ExpectValue::Float(v as f64),
            toml::Value::Boolean(v) => ExpectValue::Bool(v),
            other => {
                return Err(LoadError::UnsupportedExpect {
                    file,
                    case: name,
                    key,
                    kind: other.type_str(),
                })
            }
        };
        expect.insert(key, ev);
    }

    let tol = Tolerance {
        rel: rc.rel_tol,
        abs: rc.abs_tol,
        ulp: rc.ulp_tol,
    };
    if tol.is_empty() {
        return Err(LoadError::NoTolerance { file, case: name });
    }
    for (label, value) in [("rel_tol", tol.rel), ("abs_tol", tol.abs)] {
        if let Some(v) = value {
            if !v.is_finite() || v < 0.0 {
                return Err(LoadError::BadTolerance {
                    file,
                    case: name,
                    detail: format!("`{label}` must be finite and non-negative, got {v}"),
                });
            }
        }
    }

    Ok(Case {
        name,
        source,
        status: rc.status,
        reason,
        inputs: rc.inputs,
        expect,
        tol,
        file,
    })
}
