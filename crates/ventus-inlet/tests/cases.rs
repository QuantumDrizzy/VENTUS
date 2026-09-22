//! Crate-level lock: the capture / starting cases load, and Normal refusals
//! match the public API. The missing-key known_limit (`lip_suction_force_n`)
//! is asserted by the harness.

use std::path::Path;

use ventus_inlet::{
    additive_drag_from_lip, additive_drag_without_lip, body_can_host_capture,
    capture_to_body_ratio, isentropic_contraction_ratio, kantrowitz_contraction_ratio,
    spike_position_m, spillage, unstart_margin, CaptureError, CowlLipGeometry, FreestreamStation,
    StartError,
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
    let mut saw_declared_lip = false;
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
            let has_lip =
                c.inputs.contains_key("lip_radius_ratio") || c.inputs.contains_key("lip_radius_m");
            if has_lip {
                saw_declared_lip = true;
                let ratio = c
                    .inputs
                    .get("lip_radius_ratio")
                    .and_then(|v| v.as_float())
                    .unwrap();
                let geom = CowlLipGeometry::from_highlight_and_radius_ratio(ac, ratio).unwrap();
                if c.inputs.contains_key("static_pressure_pa") {
                    let fs = FreestreamStation {
                        streamtube_area_m2: a0,
                        static_pressure_pa: f(c, "static_pressure_pa"),
                        density_kg_m3: f(c, "density_kg_m3"),
                        velocity_m_s: f(c, "velocity_m_s"),
                    };
                    let gamma = c
                        .inputs
                        .get("gamma")
                        .and_then(|v| v.as_float())
                        .unwrap_or(1.4);
                    match additive_drag_from_lip(fs, geom, gamma) {
                        Ok(d) => {
                            if let Some(ExpectValue::Float(expected)) =
                                c.expect.get("additive_drag_n")
                            {
                                if expected.abs() < 1e-6 {
                                    assert!(d.force_n.abs() < 1e-6, "{}", c.name);
                                } else {
                                    assert!(
                                        (d.force_n / expected - 1.0).abs() < 1e-3,
                                        "case `{}`: D_add {} against {expected}",
                                        c.name,
                                        d.force_n
                                    );
                                }
                            }
                        }
                        Err(CaptureError::CaptureExceedsCowl) => {
                            assert_eq!(
                                c.expect.get("refused_capture_exceeds_cowl"),
                                Some(&ExpectValue::Bool(true)),
                                "{}",
                                c.name
                            );
                        }
                        Err(e) => panic!("case `{}`: unexpected {e:?}", c.name),
                    }
                }
            } else if matches!(c.status, Status::Normal)
                && c.expect.get("refused_cowl_lip_not_modelled") == Some(&ExpectValue::Bool(true))
            {
                assert_eq!(
                    additive_drag_without_lip(a0, ac),
                    Err(CaptureError::CowlLipNotModelled)
                );
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
    assert!(
        saw_declared_lip,
        "expected at least one declared-lip additive-drag case"
    );
}

fn f(c: &case::Case, key: &str) -> f64 {
    c.inputs
        .get(key)
        .and_then(|v| v.as_float())
        .unwrap_or_else(|| panic!("case `{}` input `{key}` must be a float", c.name))
}
