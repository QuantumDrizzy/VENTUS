//! Capture area, spillage, Kantrowitz starting, and the refusals that remain.
//!
//! # What this module is allowed to compute
//!
//! Identities and one classical starting limit, all from quantities this crate
//! already has or is given:
//!
//! - mass-flow ratio `A₀/A_c` from two areas
//! - whether a required capture fits inside a body cross-section
//! - additive (pre-entry) drag from the streamtube control-volume **definition**,
//!   when the caller supplies the cowl-lip state
//! - the Kantrowitz–Donaldson self-start contraction, composed from M2's
//!   normal-shock and `A/A*` relations
//!
//! # What it still refuses
//!
//! A translating-spike **schedule** and a time-accurate **unstart** (shock
//! expulsion) need internal contraction, throat area and a back-pressure history
//! this aircraft has not declared. Those stay typed refusals, the same contract
//! as `ventus-scram`: a plausible number here would look like an inlet and
//! would be a comment with extra arithmetic.
//!
//! Spillage **force** without a cowl-lip pressure is the same class of gap.
//! The spilled *area* is an identity; the newton is not.

use ventus_gasdyn::{area_ratio, normal_shock, GasDynError};

/// Kantrowitz contraction `A_e/A_t` as Mach → ∞ for γ = 1.4.
///
/// Flock & Gülhan, AIAA J. 57(6), 2019, eq. (4), after Kantrowitz & Donaldson
/// (NACA ACR L5D20 / WR L-713, 1945): the infinite-Mach limit of the
/// self-start contraction equals **1.666** at γ = 1.4. Three cited digits;
/// the function is `A/A*` at the infinite-Mach post-shock Mach
/// `sqrt((γ−1)/(2γ))`, so a drift in M2 cannot hide behind this constant.
pub const KANTROWITZ_CONTRACTION_INFINITE_MACH_GAMMA_14: f64 = 1.666;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureError {
    /// An input was NaN.
    NotANumber,
    /// An area or speed was zero or negative.
    NonPhysical,
    /// Additive drag was asked for without a cowl-lip state. The spilled
    /// *area* is still available from [`spillage`]; the force is not.
    CowlLipNotModelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartError {
    NotANumber,
    /// Mach number was negative, or a contraction ratio was not positive.
    NonPhysical,
    /// Kantrowitz is a supersonic starting limit. Below M = 1 there is no
    /// shock to swallow.
    Subsonic,
    /// The translating-spike schedule that holds an internal shock system
    /// across the envelope is not modelled. This aircraft has no spike
    /// geometry to schedule.
    SpikeScheduleNotModelled,
    /// Time-accurate unstart (shock expulsion under back-pressure or
    /// over-contraction) is not modelled. [`kantrowitz_contraction_ratio`]
    /// and [`self_starts`] are the 1-D starting *criterion*; they are not
    /// a margin in newtons or milliseconds.
    UnstartDynamicsNotModelled,
    Gas(GasDynError),
}

impl From<GasDynError> for StartError {
    fn from(e: GasDynError) -> Self {
        match e {
            GasDynError::NotANumber => StartError::NotANumber,
            GasDynError::NegativeMach | GasDynError::InvalidGamma => StartError::NonPhysical,
            GasDynError::SubsonicUpstream => StartError::Subsonic,
            other => StartError::Gas(other),
        }
    }
}

/// Streamtube capture area from continuity: `A₀ = ṁ / (ρ∞ V∞)`.
///
/// This is the area the engine must present to the freestream to swallow
/// `mass_flow_kg_s`. It is not a cowl highlight and it is not a body
/// cross-section; comparing it to those is [`mass_flow_ratio`] and
/// [`capture_to_body_ratio`].
///
/// # Errors
/// [`CaptureError::NonPhysical`] if density or speed is not positive.
pub fn streamtube_area_m2(
    mass_flow_kg_s: f64,
    density_kg_m3: f64,
    velocity_m_s: f64,
) -> Result<f64, CaptureError> {
    check_finite(mass_flow_kg_s)?;
    check_positive(density_kg_m3)?;
    check_positive(velocity_m_s)?;
    if mass_flow_kg_s < 0.0 {
        return Err(CaptureError::NonPhysical);
    }
    Ok(mass_flow_kg_s / (density_kg_m3 * velocity_m_s))
}

/// Mass-flow ratio `A₀ / A_c` (captured streamtube over cowl highlight).
///
/// At shock-on-lip design operation this is 1. Below 1 the difference is
/// spilled area. Above 1 the cowl cannot swallow the required streamtube:
/// the same self-inconsistency [`capture_to_body_ratio`] reports when the
/// "cowl" is the whole body.
///
/// # Errors
/// [`CaptureError::NonPhysical`] if either area is not positive.
pub fn mass_flow_ratio(streamtube_m2: f64, cowl_m2: f64) -> Result<f64, CaptureError> {
    check_positive(streamtube_m2)?;
    check_positive(cowl_m2)?;
    Ok(streamtube_m2 / cowl_m2)
}

/// Required capture over a body (or cowl) cross-section.
///
/// A ratio above 1 does **not** prove an aircraft is impossible. It proves
/// the configuration that assumed this body cannot also host this inlet.
/// Same claim M12 makes with `capture_area_ratio`; this is the inlet-side
/// identity, with the areas passed in so M3 does not grow a dependency on
/// M6b.
pub fn capture_to_body_ratio(capture_m2: f64, body_m2: f64) -> Result<f64, CaptureError> {
    mass_flow_ratio(capture_m2, body_m2)
}

/// Whether the body can host the capture: ratio ≤ 1.
pub fn body_can_host_capture(capture_m2: f64, body_m2: f64) -> Result<bool, CaptureError> {
    Ok(capture_to_body_ratio(capture_m2, body_m2)? <= 1.0)
}

/// Spillage from two areas. The force coefficient is not invented here.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spillage {
    /// `A₀ / A_c`.
    pub mass_flow_ratio: f64,
    /// `(A_c − A₀)+`. Zero at shock-on-lip; the whole cowl when `A₀ = 0`.
    pub spilled_area_m2: f64,
    /// `A₀ > A_c`: the highlight cannot swallow the streamtube.
    pub capture_exceeds_cowl: bool,
}

/// Spilled area and mass-flow ratio from a streamtube and a cowl highlight.
///
/// This is the hook. Additive *drag* still needs a lip state; see
/// [`additive_drag_n`].
pub fn spillage(streamtube_m2: f64, cowl_m2: f64) -> Result<Spillage, CaptureError> {
    let mu = mass_flow_ratio(streamtube_m2, cowl_m2)?;
    let spilled = if cowl_m2 > streamtube_m2 {
        cowl_m2 - streamtube_m2
    } else {
        0.0
    };
    Ok(Spillage {
        mass_flow_ratio: mu,
        spilled_area_m2: spilled,
        capture_exceeds_cowl: mu > 1.0,
    })
}

/// Cowl-lip state needed to close the additive-drag control volume.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CowlLip {
    pub area_m2: f64,
    pub static_pressure_pa: f64,
    /// Axial velocity into the cowl [m/s].
    pub axial_velocity_m_s: f64,
}

/// Freestream side of the same control volume.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FreestreamStation {
    pub streamtube_area_m2: f64,
    pub static_pressure_pa: f64,
    pub density_kg_m3: f64,
    pub velocity_m_s: f64,
}

/// Pre-entry (additive) drag from the streamtube control-volume definition.
///
/// Seddon & Goldsmith, *Intake Aerodynamics*: the additive drag is the axial
/// force on the captured streamtube between freestream and the cowl lip,
///
/// ```text
///   D_add = (p₀ A₀ + ṁ V₀) − (p_c A_c + ṁ u_c)
/// ```
///
/// with `ṁ = ρ₀ V₀ A₀`. This is a definition, not a correlation. Lip suction
/// that would lower `p` on the pre-entry surface is already inside `p_c` and
/// `u_c` if the caller measured them; this function does not add a fudge.
///
/// At shock-on-lip with a uniform streamtube (`A₀ = A_c`, `p_c = p₀`,
/// `u_c = V₀`) this is **identically zero**. That is the test, and it is why
/// a started external-compression inlet at design has no additive drag in
/// this accounting.
///
/// # Errors
/// [`CaptureError::CowlLipNotModelled`] is not used here — the lip is an
/// argument. [`CaptureError::NonPhysical`] if an area, density or speed is
/// not positive.
pub fn additive_drag_n(freestream: FreestreamStation, lip: CowlLip) -> Result<f64, CaptureError> {
    check_positive(freestream.streamtube_area_m2)?;
    check_positive(freestream.density_kg_m3)?;
    check_positive(freestream.velocity_m_s)?;
    check_positive(lip.area_m2)?;
    check_finite(freestream.static_pressure_pa)?;
    check_finite(lip.static_pressure_pa)?;
    check_finite(lip.axial_velocity_m_s)?;
    if freestream.static_pressure_pa < 0.0 || lip.static_pressure_pa < 0.0 {
        return Err(CaptureError::NonPhysical);
    }
    let mass_flow =
        freestream.density_kg_m3 * freestream.velocity_m_s * freestream.streamtube_area_m2;
    let incoming = freestream.static_pressure_pa * freestream.streamtube_area_m2
        + mass_flow * freestream.velocity_m_s;
    let at_lip = lip.static_pressure_pa * lip.area_m2 + mass_flow * lip.axial_velocity_m_s;
    Ok(incoming - at_lip)
}

/// Additive drag without a cowl-lip state. Always a refusal: the spilled
/// area is [`spillage`]; inventing a `C_D` from mass-flow ratio alone at
/// M 3.5 would be an incompressible fudge in the wrong regime.
pub fn additive_drag_without_lip(streamtube_m2: f64, cowl_m2: f64) -> Result<f64, CaptureError> {
    // Physical checks first, so a NaN cannot hide inside the gap.
    let _ = mass_flow_ratio(streamtube_m2, cowl_m2)?;
    Err(CaptureError::CowlLipNotModelled)
}

/// Kantrowitz–Donaldson self-start contraction `A_e / A_t`.
///
/// Kantrowitz & Donaldson, NACA ACR L5D20 (WR L-713), 1945: put a normal
/// shock at the entrance of an internal contraction and ask for the
/// isentropic area ratio that just chokes the throat. Larger contraction
/// than this and the shock will not swallow — the inlet does not self-start.
///
/// Composed, not transcribed: `A/A*` at the post-shock Mach from M2. The
/// identity `A_e/A_t = (A/A*)_{M_e} · (p₀₂/p₀₁)_NS` is asserted in the
/// tests rather than assumed.
///
/// # Errors
/// [`StartError::Subsonic`] below M = 1; [`StartError::NonPhysical`] for
/// a bad gamma.
pub fn kantrowitz_contraction_ratio(mach_entrance: f64, gamma: f64) -> Result<f64, StartError> {
    let shock = normal_shock(mach_entrance, gamma)?;
    Ok(area_ratio(shock.mach_downstream, gamma)?)
}

/// Isentropic contraction `A_e / A*` at the entrance Mach — what an
/// efficient internal diffuser *wants*. Always larger than Kantrowitz above
/// M = 1, which is why a mixed-compression inlet that tries to swallow that
/// contraction needs variable geometry to start.
pub fn isentropic_contraction_ratio(mach_entrance: f64, gamma: f64) -> Result<f64, StartError> {
    Ok(area_ratio(mach_entrance, gamma)?)
}

/// Whether a declared internal contraction self-starts at this Mach:
/// `A_e/A_t ≤` [`kantrowitz_contraction_ratio`].
///
/// This is the 1-D Kantrowitz criterion, not an unstart *margin*. A
/// contraction this aircraft has not declared cannot be tested; the
/// caller has to bring one. See [`unstart_margin`] for the refusal that
/// belongs to that gap.
pub fn self_starts(
    mach_entrance: f64,
    gamma: f64,
    contraction_ae_over_at: f64,
) -> Result<bool, StartError> {
    if contraction_ae_over_at.is_nan() {
        return Err(StartError::NotANumber);
    }
    if contraction_ae_over_at <= 0.0 || !contraction_ae_over_at.is_finite() {
        return Err(StartError::NonPhysical);
    }
    let limit = kantrowitz_contraction_ratio(mach_entrance, gamma)?;
    Ok(contraction_ae_over_at <= limit)
}

/// Translating-spike position that holds the shock system. Not modelled:
/// there is no spike geometry and no schedule in this workspace.
pub fn spike_position_m(mach_freestream: f64) -> Result<f64, StartError> {
    check_start_mach(mach_freestream)?;
    Err(StartError::SpikeScheduleNotModelled)
}

/// Unstart margin (back-pressure or contraction to shock expulsion).
///
/// Not modelled. [`self_starts`] answers the 1-D starting question when a
/// contraction is supplied. The *margin* needs an operating throat, a
/// diffuser, and a combustor back-pressure, none of which exist here.
pub fn unstart_margin(mach_freestream: f64) -> Result<f64, StartError> {
    check_start_mach(mach_freestream)?;
    Err(StartError::UnstartDynamicsNotModelled)
}

fn check_finite(x: f64) -> Result<(), CaptureError> {
    if x.is_nan() {
        return Err(CaptureError::NotANumber);
    }
    if !x.is_finite() {
        return Err(CaptureError::NonPhysical);
    }
    Ok(())
}

fn check_positive(x: f64) -> Result<(), CaptureError> {
    check_finite(x)?;
    if x <= 0.0 {
        return Err(CaptureError::NonPhysical);
    }
    Ok(())
}

fn check_start_mach(mach: f64) -> Result<(), StartError> {
    if mach.is_nan() {
        return Err(StartError::NotANumber);
    }
    if !mach.is_finite() || mach < 0.0 {
        return Err(StartError::NonPhysical);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ventus_gasdyn::{area_ratio, normal_shock};
    use ventus_units::float::rel_err;

    const GAMMA: f64 = 1.4;
    const DESIGN_MACH: f64 = 3.5;

    /// Continuity inverted: the streamtube that swallows ṁ at ρV is ṁ/(ρV),
    /// and putting it back through `mass_flow_ratio` against itself is 1.
    #[test]
    fn streamtube_area_is_continuity() {
        let a = streamtube_area_m2(10.0, 0.5, 200.0).unwrap();
        assert!(rel_err(a, 0.1) < 1e-15);
        assert!(rel_err(mass_flow_ratio(a, a).unwrap(), 1.0) < 1e-15);
        assert_eq!(
            streamtube_area_m2(1.0, 0.0, 1.0),
            Err(CaptureError::NonPhysical)
        );
        assert_eq!(
            streamtube_area_m2(f64::NAN, 1.0, 1.0),
            Err(CaptureError::NotANumber)
        );
    }

    /// THE CONFIGURATION STATEMENT. A capture larger than the body is the
    /// self-inconsistency M12 reports at M 3.85, as an identity on two areas.
    #[test]
    fn a_capture_larger_than_the_body_does_not_fit() {
        let body = 3.369;
        assert!(body_can_host_capture(0.745 * body, body).unwrap());
        assert!(!body_can_host_capture(1.146 * body, body).unwrap());
        let ratio = capture_to_body_ratio(1.146 * body, body).unwrap();
        assert!(rel_err(ratio, 1.146) < 1e-12);
    }

    /// Spillage is the leftover area, and only when the cowl is larger.
    #[test]
    fn spillage_is_the_leftover_cowl_area() {
        let s = spillage(2.0, 3.0).unwrap();
        assert!(rel_err(s.mass_flow_ratio, 2.0 / 3.0) < 1e-15);
        assert!(rel_err(s.spilled_area_m2, 1.0) < 1e-15);
        assert!(!s.capture_exceeds_cowl);

        let tight = spillage(3.0, 3.0).unwrap();
        assert_eq!(tight.spilled_area_m2, 0.0);
        assert!(rel_err(tight.mass_flow_ratio, 1.0) < 1e-15);

        let over = spillage(4.0, 3.0).unwrap();
        assert_eq!(over.spilled_area_m2, 0.0);
        assert!(over.capture_exceeds_cowl);
    }

    /// THE ADDITIVE-DRAG IDENTITY. Shock-on-lip, uniform streamtube: the
    /// control volume closes with zero force. If this ever drifted, the
    /// bookkeeping would be charging ram drag that the definition does not
    /// contain.
    #[test]
    fn additive_drag_vanishes_at_shock_on_lip() {
        let fs = FreestreamStation {
            streamtube_area_m2: 2.5,
            static_pressure_pa: 2153.0,
            density_kg_m3: 0.0337,
            velocity_m_s: 1047.0,
        };
        let lip = CowlLip {
            area_m2: 2.5,
            static_pressure_pa: 2153.0,
            axial_velocity_m_s: 1047.0,
        };
        let d = additive_drag_n(fs, lip).unwrap();
        let q_a =
            0.5 * fs.density_kg_m3 * fs.velocity_m_s * fs.velocity_m_s * fs.streamtube_area_m2;
        assert!(
            ventus_units::float::abs(d) / q_a < 1e-12,
            "shock-on-lip additive drag is {d} N, not zero"
        );
    }

    /// Without a lip state the force is refused. The spilled area is not.
    #[test]
    fn additive_drag_without_a_lip_is_refused() {
        assert_eq!(
            additive_drag_without_lip(2.0, 3.0),
            Err(CaptureError::CowlLipNotModelled)
        );
        assert_eq!(
            additive_drag_without_lip(f64::NAN, 3.0),
            Err(CaptureError::NotANumber)
        );
        assert!(spillage(2.0, 3.0).is_ok());
    }

    /// Kantrowitz at M = 1 is no contraction: the shock has vanishing
    /// strength and the throat is the entrance.
    #[test]
    fn kantrowitz_is_unity_at_sonic_entrance() {
        assert!(rel_err(kantrowitz_contraction_ratio(1.0, GAMMA).unwrap(), 1.0) < 1e-14);
        assert_eq!(
            kantrowitz_contraction_ratio(0.9, GAMMA),
            Err(StartError::Subsonic)
        );
    }

    /// COMPOSED FROM M2, AND THE TWO ROUTES AGREE.
    ///
    /// `A_e/A_t = (A/A*)_{M₂}` after a normal shock, and also
    /// `(A/A*)_{M_e} · (p₀₂/p₀₁)`. If those drifted apart, one of the
    /// gasdyn relations would be wrong, or this composition would.
    #[test]
    fn kantrowitz_agrees_with_the_shock_times_area_ratio_identity() {
        for mach in [1.0, 1.5, 2.0, 2.5, 3.0, 3.5, 4.0, 5.0] {
            let shock = normal_shock(mach, GAMMA).unwrap();
            let from_post = area_ratio(shock.mach_downstream, GAMMA).unwrap();
            let from_pre = area_ratio(mach, GAMMA).unwrap() * shock.stagnation_pressure_ratio;
            let k = kantrowitz_contraction_ratio(mach, GAMMA).unwrap();
            assert!(
                rel_err(k, from_post) < 1e-14,
                "M={mach}: Kantrowitz {k} against post-shock A/A* {from_post}"
            );
            assert!(
                rel_err(k, from_pre) < 1e-12,
                "M={mach}: Kantrowitz {k} against (A/A*)·π_d {from_pre}"
            );
        }
    }

    /// The infinite-Mach limit is 1.666 at γ = 1.4, and the function has
    /// already arrived there by M = 100. A mixed-compression inlet cannot
    /// buy unlimited starting contraction by flying faster.
    #[test]
    fn kantrowitz_saturates_at_the_cited_infinite_mach_limit() {
        let m2_inf = libm::sqrt((GAMMA - 1.0) / (2.0 * GAMMA));
        let from_m2 = area_ratio(m2_inf, GAMMA).unwrap();
        assert!(
            rel_err(from_m2, KANTROWITZ_CONTRACTION_INFINITE_MACH_GAMMA_14) < 1e-3,
            "A/A* at infinite-Mach M2 is {from_m2}, not the cited 1.666"
        );
        let far = kantrowitz_contraction_ratio(1.0e6, GAMMA).unwrap();
        assert!(
            rel_err(far, from_m2) < 1e-4,
            "infinite-Mach Kantrowitz moved to {far} against {from_m2}"
        );
        // And it is a genuine ceiling: M 3.5 sits well below it, M 8 closer.
        let at_design = kantrowitz_contraction_ratio(DESIGN_MACH, GAMMA).unwrap();
        let at_eight = kantrowitz_contraction_ratio(8.0, GAMMA).unwrap();
        assert!(at_design < at_eight && at_eight < far);
        assert!(
            (1.44..1.46).contains(&at_design),
            "Kantrowitz at M 3.5 moved to {at_design:.4}; recorded ~1.45"
        );
    }

    /// THE FINDING THIS MODULE EXISTS TO STATE.
    ///
    /// At M 3.5 an efficient internal diffuser wants `A_e/A* ≈ 6.79`.
    /// Kantrowitz will only start `A_e/A_t ≈ 1.45`. The gap is why a
    /// mixed-compression inlet needs a translating spike, and it is a
    /// property of the gas, not of a geometry we have not drawn.
    #[test]
    fn isentropic_contraction_outruns_kantrowitz_which_is_why_the_spike_exists() {
        let want = isentropic_contraction_ratio(DESIGN_MACH, GAMMA).unwrap();
        let can_start = kantrowitz_contraction_ratio(DESIGN_MACH, GAMMA).unwrap();
        assert!(
            want > 2.0 * can_start,
            "the starting gap closed: isentropic {want:.2} against Kantrowitz {can_start:.2}"
        );
        assert!(
            (6.7..6.9).contains(&want),
            "isentropic A/A* at M 3.5 moved to {want:.3}"
        );

        // A contraction the isentropic diffuser wants cannot self-start.
        assert!(!self_starts(DESIGN_MACH, GAMMA, want).unwrap());
        // A contraction at the Kantrowitz number can.
        assert!(self_starts(DESIGN_MACH, GAMMA, can_start).unwrap());
        // And one slightly over cannot.
        assert!(!self_starts(DESIGN_MACH, GAMMA, can_start * 1.01).unwrap());
    }

    /// The spike schedule and the unstart margin refuse rather than
    /// returning a number that looks like geometry. Degenerate Mach is
    /// refused as such, first, so a NaN cannot hide inside the gap.
    #[test]
    fn spike_and_unstart_are_refused_rather_than_approximated() {
        assert_eq!(
            spike_position_m(DESIGN_MACH),
            Err(StartError::SpikeScheduleNotModelled)
        );
        assert_eq!(
            unstart_margin(DESIGN_MACH),
            Err(StartError::UnstartDynamicsNotModelled)
        );
        assert_eq!(
            spike_position_m(4.0),
            Err(StartError::SpikeScheduleNotModelled)
        );
        assert_eq!(
            unstart_margin(0.0),
            Err(StartError::UnstartDynamicsNotModelled)
        );
        assert_eq!(spike_position_m(f64::NAN), Err(StartError::NotANumber));
        assert_eq!(unstart_margin(-1.0), Err(StartError::NonPhysical));
        assert_eq!(
            self_starts(DESIGN_MACH, GAMMA, f64::NAN),
            Err(StartError::NotANumber)
        );
        assert_eq!(
            self_starts(DESIGN_MACH, GAMMA, 0.0),
            Err(StartError::NonPhysical)
        );
    }
}
