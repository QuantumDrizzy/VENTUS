//! Validation report + run provenance.
//!
//! ADR-000 D4 r2 / D9: the report and the run manifest share one identity. A
//! report without a git hash is not reproducible, so `run_id`, commit and
//! clean/dirty state go in the header of every report, not in a separate file.
//!
//! Provenance is gathered explicitly by the caller (`xtask`), never implicitly
//! by the checker, so that unit tests do not shell out to `git` on every run.

use crate::case::{Case, Status};
use crate::check::{Outcome, Summary};
use std::fmt::Write as _;
use std::io;
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

/// Identity of one validation run.
#[derive(Debug, Clone)]
pub struct Provenance {
    pub run_id: String,
    pub git_hash: String,
    pub git_dirty: bool,
    pub timestamp_utc: String,
    pub rustc: String,
}

impl Provenance {
    /// Gather provenance from the environment. Every field degrades to a loud
    /// placeholder rather than failing: a report that says `unknown` is honest,
    /// a run that aborts because `git` is missing is not useful.
    #[must_use]
    pub fn detect() -> Self {
        let git_hash = run(&["git", "rev-parse", "--short=12", "HEAD"])
            .unwrap_or_else(|| "unknown".to_string());
        // If `git` cannot answer, assume dirty: claiming a clean tree we did not
        // verify would put a false reproducibility guarantee in the report.
        let git_dirty = run(&["git", "status", "--porcelain"]).is_none_or(|s| !s.trim().is_empty());
        let rustc = run(&["rustc", "-V"]).unwrap_or_else(|| "unknown".to_string());

        let epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let timestamp_utc = iso8601_utc(epoch);

        let run_id = format!(
            "{}-{}{}",
            compact_stamp(epoch),
            git_hash,
            if git_dirty { "-dirty" } else { "" }
        );

        Self {
            run_id,
            git_hash,
            git_dirty,
            timestamp_utc,
            rustc,
        }
    }
}

fn run(argv: &[&str]) -> Option<String> {
    let out = Command::new(argv[0]).args(&argv[1..]).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Civil date from a Unix day number. Howard Hinnant's `civil_from_days`,
/// valid across the whole practical range and exact — no floating point.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn split_epoch(epoch: u64) -> (i64, u32, u32, u64, u64, u64) {
    let days = (epoch / 86_400) as i64;
    let secs_of_day = epoch % 86_400;
    let (y, m, d) = civil_from_days(days);
    (
        y,
        m,
        d,
        secs_of_day / 3600,
        (secs_of_day / 60) % 60,
        secs_of_day % 60,
    )
}

fn iso8601_utc(epoch: u64) -> String {
    let (y, mo, d, h, mi, s) = split_epoch(epoch);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}

fn compact_stamp(epoch: u64) -> String {
    let (y, mo, d, h, mi, s) = split_epoch(epoch);
    format!("{y:04}{mo:02}{d:02}T{h:02}{mi:02}{s:02}Z")
}

/// One row of the report.
pub struct Entry<'a> {
    pub case: &'a Case,
    pub outcome: &'a Outcome,
}

/// Render the Markdown report. Deterministic given the same inputs and
/// provenance, so two runs at the same commit produce identical text.
#[must_use]
pub fn markdown(prov: &Provenance, entries: &[Entry<'_>]) -> String {
    let summary: Summary = entries.iter().fold(Summary::default(), |mut s, e| {
        s.record(e.outcome);
        s
    });

    let mut out = String::new();
    let _ = writeln!(out, "# VENTUS validation report");
    let _ = writeln!(out);
    let _ = writeln!(out, "| | |");
    let _ = writeln!(out, "|---|---|");
    let _ = writeln!(out, "| run_id | `{}` |", prov.run_id);
    let _ = writeln!(out, "| commit | `{}` |", prov.git_hash);
    let _ = writeln!(
        out,
        "| tree | {} |",
        if prov.git_dirty {
            "**DIRTY** — this run is not reproducible"
        } else {
            "clean"
        }
    );
    let _ = writeln!(out, "| timestamp | {} |", prov.timestamp_utc);
    let _ = writeln!(out, "| rustc | {} |", prov.rustc);
    let _ = writeln!(out);

    let _ = writeln!(
        out,
        "**{} cases: {} pass, {} fail, {} known limit, {} stale known limit.**",
        summary.total(),
        summary.pass,
        summary.fail,
        summary.known_limit,
        summary.stale_known_limit
    );
    let _ = writeln!(out);
    if summary.breaks_build() {
        let _ = writeln!(out, "Verdict: **FAIL**");
    } else if summary.stale_known_limit > 0 {
        let _ = writeln!(
            out,
            "Verdict: **PASS**, but {} known-limit annotation(s) no longer \
             reproduce and should be removed.",
            summary.stale_known_limit
        );
    } else {
        let _ = writeln!(out, "Verdict: **PASS**");
    }
    let _ = writeln!(out);

    let _ = writeln!(out, "| case | verdict | worst rel err | source |");
    let _ = writeln!(out, "|---|---|---|---|");
    for e in entries {
        let rel = e.outcome.worst_rel_err();
        let rel_txt = if rel == 0.0 {
            "-".to_string()
        } else {
            format!("{rel:.3e}")
        };
        let _ = writeln!(
            out,
            "| `{}` | {} | {} | {} |",
            e.case.name,
            e.outcome.label(),
            rel_txt,
            e.case.source.replace('|', "\\|").replace('\n', " ")
        );
    }
    let _ = writeln!(out);

    // Detail only for the rows that need it.
    for e in entries {
        match e.outcome {
            Outcome::Pass => {}
            Outcome::StaleKnownLimit => {
                let _ = writeln!(
                    out,
                    "### `{}` — STALE KNOWN LIMIT\n\nThis case is annotated \
                     `known_limit` but now passes. The limit no longer \
                     reproduces; remove the annotation.\n\nStated reason was: {}\n",
                    e.case.name,
                    e.case.reason.as_deref().unwrap_or("(none)").trim()
                );
            }
            Outcome::Fail {
                mismatches,
                missing,
            }
            | Outcome::KnownLimit {
                mismatches,
                missing,
            } => {
                let _ = writeln!(out, "### `{}` — {}", e.case.name, e.outcome.label());
                let _ = writeln!(out);
                let _ = writeln!(out, "- file: `{}`", e.case.file.display());
                let _ = writeln!(out, "- source: {}", e.case.source.trim().replace('\n', " "));
                if e.case.status == Status::KnownLimit {
                    let _ = writeln!(
                        out,
                        "- reason: {}",
                        e.case
                            .reason
                            .as_deref()
                            .unwrap_or("(none)")
                            .trim()
                            .replace('\n', " ")
                    );
                }
                let _ = writeln!(out);
                if !mismatches.is_empty() {
                    let _ = writeln!(out, "| key | expected | actual | rel err | ulp |");
                    let _ = writeln!(out, "|---|---|---|---|---|");
                    for m in mismatches {
                        let _ = writeln!(
                            out,
                            "| `{}` | {} | {} | {:.3e} | {} |",
                            m.key,
                            m.expected,
                            m.actual,
                            m.rel_err,
                            m.ulp.map_or("-".to_string(), |u| u.to_string())
                        );
                    }
                    let _ = writeln!(out);
                }
                if !missing.is_empty() {
                    let _ = writeln!(
                        out,
                        "Expected keys the module never produced: {}\n",
                        missing
                            .iter()
                            .map(|k| format!("`{k}`"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                }
            }
        }
    }

    out
}

/// Render the machine-readable companion to the Markdown report.
#[must_use]
pub fn csv(prov: &Provenance, entries: &[Entry<'_>]) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "run_id,commit,dirty,case,file,verdict,worst_rel_err,source"
    );
    for e in entries {
        let _ = writeln!(
            out,
            "{},{},{},{},{},{},{:e},{}",
            prov.run_id,
            prov.git_hash,
            prov.git_dirty,
            csv_field(&e.case.name),
            csv_field(&e.case.file.display().to_string()),
            e.outcome.label(),
            e.outcome.worst_rel_err(),
            csv_field(&e.case.source)
        );
    }
    out
}

fn csv_field(s: &str) -> String {
    let flat = s.replace(['\n', '\r'], " ");
    if flat.contains([',', '"']) {
        format!("\"{}\"", flat.replace('"', "\"\""))
    } else {
        flat
    }
}

/// Write both artifacts into `dir`.
pub fn write_all(dir: &Path, prov: &Provenance, entries: &[Entry<'_>]) -> io::Result<()> {
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join("report.md"), markdown(prov, entries))?;
    std::fs::write(dir.join("report.csv"), csv(prov, entries))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Yardstick: dates whose Unix epoch value is independently known.
    #[test]
    fn epoch_conversion_matches_known_dates() {
        assert_eq!(iso8601_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso8601_utc(1), "1970-01-01T00:00:01Z");
        // 2000-03-01, the leap-year boundary the algorithm is built around.
        assert_eq!(iso8601_utc(951_868_800), "2000-03-01T00:00:00Z");
        // 2000-02-29 must exist: 2000 is a leap year despite being a century.
        assert_eq!(iso8601_utc(951_782_400), "2000-02-29T00:00:00Z");
        // 1900 was NOT a leap year; 2100 will not be either.
        assert_eq!(iso8601_utc(4_107_542_400), "2100-03-01T00:00:00Z");
        assert_eq!(iso8601_utc(1_234_567_890), "2009-02-13T23:31:30Z");
        assert_eq!(compact_stamp(1_234_567_890), "20090213T233130Z");
    }

    #[test]
    fn every_day_round_trips_for_two_centuries() {
        // Independent check: day numbers must advance the civil date by exactly
        // one day, with no gaps or repeats, across 1970-2170.
        let mut prev = civil_from_days(0);
        for day in 1..73_000_i64 {
            let cur = civil_from_days(day);
            assert_ne!(cur, prev, "repeated date at day {day}");
            let (py, pm, pd) = prev;
            let (cy, cm, cd) = cur;
            let ok = (cy == py && cm == pm && cd == pd + 1)
                || (cy == py && cm == pm + 1 && cd == 1)
                || (cy == py + 1 && pm == 12 && cm == 1 && cd == 1);
            assert!(ok, "day {day}: {prev:?} -> {cur:?}");
            prev = cur;
        }
    }
}
