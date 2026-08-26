//! xtask — the single entry point. No Make, no .bat (ADR-000 D6).
//!
//!   cargo xtask build       rust + native; detects vcvars, fails with the fix
//!   cargo xtask check-dag   real cycle detection over the workspace graph (D5)
//!   cargo xtask validate    full harness -> out/<run_id>/report.md
//!   cargo xtask bench m9    GATED: refuses unless validate passed at the same
//!                           commit hash with a clean tree (D3)
//!   cargo xtask report      design point tables + plots (invokes analysis/)
//!
//! vcvars note: if VSCMD_ARG_TGT_ARCH != x64, abort with the exact remedy.
//! The Git coreutils `link` shadows the MSVC linker, so a missing vcvars fails
//! forty lines later inside the wrong link.exe instead of here.

fn main() {
    eprintln!("xtask: not implemented yet (skeleton per ADR-000).");
    eprintln!("commands: build | check-dag | validate | bench | report");
    std::process::exit(2);
}
