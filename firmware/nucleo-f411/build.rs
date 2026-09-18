//! Bake the git commit into the firmware, so every HIL reply identifies the
//! exact build that produced it — the same discipline the validation reports
//! carry with their run_id and commit hash.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let commit = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();

    // First 8 bytes of the commit hash. Dirty trees are NOT distinguished here:
    // the HIL gate prints the id and the host compares it against `git rev-parse`
    // itself, which knows the tree state.
    let mut build_id = [0u8; 8];
    if commit.len() >= 16 {
        for (i, chunk) in commit.as_bytes().chunks(2).take(8).enumerate() {
            build_id[i] = u8::from_str_radix(std::str::from_utf8(chunk).unwrap_or("00"), 16)
                .unwrap_or(0);
        }
    } else {
        // No git available at build time: an all-0xFF id means "untraceable".
        // The gate refuses it rather than pretending to be reproducible.
        build_id = [0xFF; 8];
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR is set by cargo"));
    fs::write(out_dir.join("build_id.bin"), build_id).expect("write build_id.bin");
}
