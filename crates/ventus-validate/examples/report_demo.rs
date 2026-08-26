//! End-to-end smoke run of the harness against the case files checked into
//! this repository, with real provenance from `git` and `rustc`.
//!
//! There is no physics yet, so no module can answer any case: every case is
//! expected to report FAIL with missing keys. That is the point — it exercises
//! the load path, the checker, the D9 provenance linkage and both report
//! writers on real data, and it will keep working unchanged once M1 supplies
//! actual computed values.
//!
//!   cargo run -p ventus-validate --example report_demo

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use ventus_validate::case;
use ventus_validate::check::{check, Summary};
use ventus_validate::report::{self, Entry, Provenance};
use ventus_validate::ExpectValue;

fn main() {
    // parent().parent() rather than join("..") so the paths that end up in the
    // report have no `..` components. A report is a versioned artifact; its
    // paths must be stable and diff-friendly, not an accident of the caller.
    let repo: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crates/<name> is two levels below the workspace root")
        .to_path_buf();
    let crates = repo.join("crates");

    let mut dirs: Vec<PathBuf> = std::fs::read_dir(&crates)
        .expect("crates/")
        .filter_map(Result::ok)
        .map(|e| e.path().join("cases"))
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();

    let mut cases = Vec::new();
    for d in &dirs {
        match case::load_dir(d) {
            Ok(mut c) => {
                // Repo-relative paths, so two machines produce identical reports.
                for case in &mut c {
                    case.file = relative(&case.file, &repo);
                }
                cases.append(&mut c);
            }
            // A rejected case file must stop the run loudly, not be skipped.
            Err(e) => {
                eprintln!("case file refused:\n  {e}");
                std::process::exit(1);
            }
        }
    }

    // No physics module exists yet, so nothing is computed.
    let empty: BTreeMap<String, ExpectValue> = BTreeMap::new();
    let outcomes: Vec<_> = cases.iter().map(|c| check(c, &empty)).collect();
    let entries: Vec<Entry<'_>> = cases
        .iter()
        .zip(&outcomes)
        .map(|(case, outcome)| Entry { case, outcome })
        .collect();

    let prov = Provenance::detect();
    let dir = repo.join("out").join(&prov.run_id);
    report::write_all(&dir, &prov, &entries).expect("write report");

    let summary: Summary = outcomes.iter().fold(Summary::default(), |mut s, o| {
        s.record(o);
        s
    });

    println!("run_id : {}", prov.run_id);
    println!(
        "commit : {} ({})",
        prov.git_hash,
        if prov.git_dirty { "dirty" } else { "clean" }
    );
    println!("rustc  : {}", prov.rustc);
    println!(
        "cases  : {} from {} directories",
        summary.total(),
        dirs.len()
    );
    println!(
        "verdict: {} pass, {} fail, {} known limit, {} stale",
        summary.pass, summary.fail, summary.known_limit, summary.stale_known_limit
    );
    println!("report : {}", relative(&dir, &repo).display());
}

fn relative(p: &Path, base: &Path) -> PathBuf {
    p.strip_prefix(base).unwrap_or(p).to_path_buf()
}
