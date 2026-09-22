//! Crate-level lock: the capture / starting cases load, and Normal refusals
//! match the public API. The missing-key known_limit (`additive_drag_coefficient`)
//! is asserted by the harness.

use std::path::Path;

use ventus_inlet::{
    additive_drag_without_lip, body_can_host_capture, capture_to_body_ratio,
    isentropic_contraction_ratio, kantrowitz_contraction_ratio, spike_position_m, spillage,
    unstart_margin, CaptureError, StartError,
};
use ventus_validate::case::{self, ExpectValue, Status};

#[test]
fn capture_cases_match_the_public_api() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("cases");
    let cases = case::load_dir(&dir).unwrap_or_else(|e| panic!("case file refused:\n  {e}"));
    assert!(
        !cases.is_empty(),
        "ventus-inlet is Route::Cases; an empty cases/ is not a pass"
    );

    let mut saw_capture = false;
    for c in &cases {
        if c.inputs.contains_key("capture_area_m2") {
            saw_capture = true;
            let capture = f(c, "capture_area_m2");
            let body = f(c, "body_area_m2");
            let ratio = capture_to_body_ratio(capture, body).unwrap();
            if let Some(ExpectValue::Float(expected)) = c.expect.get("capture_to_body_ratio") {
                assert!(
                    (ratio / expected - 1.0).abs() < 1e-3,
                    "case `{}`: ratio {ratio} against {expected}",
                    c.name
                );
            }
            if let Some(ExpectValue::Bool(expected)) = c.expect.get("body_can_host_capture") {
                assert_eq!(body_can_host_capture(capture, body).unwrap(), *expected);
            }
        }

        if c.inputs.contains_key("request_spike_schedule") {
            let mach = f(c, "mach");
            assert_eq!(
                spike_position_m(mach),
                Err(StartError::SpikeScheduleNotModelled),
                "case `{}`",
                c.name
            );
        }
        if c.inputs.contains_key("request_unstart_margin") {
            let mach = f(c, "mach");
            assert_eq!(
                unstart_margin(mach),
                Err(StartError::UnstartDynamicsNotModelled),
                "case `{}`",
                c.name
            );
        }

        if c.inputs.contains_key("streamtube_area_m2") && c.inputs.contains_key("cowl_area_m2") {
            let a0 = f(c, "streamtube_area_m2");
            let ac = f(c, "cowl_area_m2");
            let s = spillage(a0, ac).unwrap();
            if let Some(ExpectValue::Float(expected)) = c.expect.get("spilled_area_m2") {
                assert!((s.spilled_area_m2 - expected).abs() < 1e-12, "{}", c.name);
            }
            match c.status {
                Status::Normal
                    if c.expect.get("refused_cowl_lip_not_modelled")
                        == Some(&ExpectValue::Bool(true)) =>
                {
                    assert_eq!(
                        additive_drag_without_lip(a0, ac),
                        Err(CaptureError::CowlLipNotModelled)
                    );
                }
                Status::KnownLimit => {
                    assert_eq!(
                        additive_drag_without_lip(a0, ac),
                        Err(CaptureError::CowlLipNotModelled)
                    );
                }
                Status::Normal => {}
            }
        }

        if c.expect.contains_key("kantrowitz_contraction") {
            let mach = f(c, "mach");
            let gamma = c
                .inputs
                .get("gamma")
                .and_then(|v| v.as_float())
                .unwrap_or(1.4);
            let k = kantrowitz_contraction_ratio(mach, gamma).unwrap();
            if let Some(ExpectValue::Float(expected)) = c.expect.get("kantrowitz_contraction") {
                assert!(
                    (k / expected - 1.0).abs() < 5e-3,
                    "case `{}`: Kantrowitz {k} against {expected}",
                    c.name
                );
            }
            if let Some(ExpectValue::Float(expected)) = c.expect.get("isentropic_contraction") {
                let i = isentropic_contraction_ratio(mach, gamma).unwrap();
                assert!(
                    (i / expected - 1.0).abs() < 5e-3,
                    "case `{}`: isentropic {i} against {expected}",
                    c.name
                );
            }
        }
    }
    assert!(saw_capture, "expected at least one capture-vs-body case");
}

fn f(c: &case::Case, key: &str) -> f64 {
    c.inputs
        .get(key)
        .and_then(|v| v.as_float())
        .unwrap_or_else(|| panic!("case `{}` input `{key}` must be a float", c.name))
}
