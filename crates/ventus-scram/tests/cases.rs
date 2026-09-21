//! Crate-level lock: the TOML cases load, and Normal cases match the public
//! refusal API. The missing-key known_limit (`specific_impulse_s`) is asserted
//! by the harness, which is the only place that knows a key was never emitted.

use std::path::Path;

use ventus_scram::{solve_cycle, CycleError, CycleRequest, Regime};
use ventus_validate::case::{self, ExpectValue, Status};

fn regime_of(c: &case::Case) -> Regime {
    match c.inputs.get("regime").and_then(|v| v.as_str()) {
        Some("ram_subsonic_burner") => Regime::RamSubsonicBurner,
        Some("dual_mode_transition") => Regime::DualModeTransition,
        Some("scram") => Regime::Scram,
        other => panic!("case `{}`: unknown regime {other:?}", c.name),
    }
}

#[test]
fn refusal_cases_match_the_public_api() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("cases");
    let cases = case::load_dir(&dir).unwrap_or_else(|e| panic!("case file refused:\n  {e}"));
    assert!(
        !cases.is_empty(),
        "ventus-scram is Route::Cases; an empty cases/ is not a pass"
    );

    for c in &cases {
        let mach = c
            .inputs
            .get("mach")
            .and_then(|v| v.as_float())
            .unwrap_or_else(|| panic!("case `{}` needs mach", c.name));
        let altitude_m = c
            .inputs
            .get("geopotential_altitude_m")
            .and_then(|v| v.as_float())
            .unwrap_or(26_000.0);
        let result = solve_cycle(CycleRequest {
            mach_freestream: mach,
            geopotential_altitude_m: altitude_m,
            regime: regime_of(c),
        });

        match c.status {
            Status::Normal => {
                assert!(result.is_err(), "case `{}` must refuse", c.name);
                if c.expect.get("refused_stations_not_modelled") == Some(&ExpectValue::Bool(true)) {
                    assert_eq!(result, Err(CycleError::StationsNotModelled));
                }
                if c.expect.get("refused_use_ideal_ramjet") == Some(&ExpectValue::Bool(true)) {
                    assert_eq!(result, Err(CycleError::UseIdealRamjet));
                }
                if c.expect.get("cycle_numbers_emitted") == Some(&ExpectValue::Bool(false)) {
                    assert!(result.is_err());
                }
            }
            Status::KnownLimit => {
                // No cycle number exists to compare. The stub's job is still to
                // refuse; the harness reports the missing Isp key.
                assert_eq!(result, Err(CycleError::StationsNotModelled));
            }
        }
    }
}
