//! Capture area, spillage, Kantrowitz starting, and the refusals that remain.
//!
//! # What this module is allowed to compute
//!
//! Identities, one classical starting limit, and a pitot-equivalent additive
//! force once a cowl lip is **declared**:
//!
//! - mass-flow ratio `A₀/A_c` from two areas
//! - whether a required capture fits inside a body cross-section
//! - cowl-lip **geometry** (highlight area, lip radius, `r/R`, projected lip
//!   area) — the Seddon & Goldsmith parameters, not a pressure field
//! - additive (pre-entry) drag from the streamtube control-volume definition,
//!   either from a supplied lip *state* or from that geometry via a
//!   pitot-equivalent closure composed from M2
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
//! Spillage **force** without a declared cowl lip is the same class of gap.
//! Lip **suction** (the blunt-lip credit that can cancel additive drag) is
//! not credited: `r/R` is recorded and used as a range flag, not as a force.
//! φ_LBO is not this module.

use ventus_gasdyn::{area_ratio, normal_shock, GasDynError};

/// Kantrowitz contraction `A_e/A_t` as Mach → ∞ for γ = 1.4.
///
/// Flock & Gülhan, AIAA J. 57(6), 2019, eq. (4), after Kantrowitz & Donaldson
/// (NACA ACR L5D20 / WR L-713, 1945): the infinite-Mach limit of the
/// self-start contraction equals **1.666** at γ = 1.4. Three cited digits;
/// the function is `A/A*` at the infinite-Mach post-shock Mach
/// `sqrt((γ−1)/(2γ))`, so a drift in M2 cannot hide behind this constant.
pub const KANTROWITZ_CONTRACTION_INFINITE_MACH_GAMMA_14: f64 = 1.666;

/// Lip radius / highlight radius above which neglecting suction is no longer
/// a "nearly sharp" supersonic-cowl approximation.
///
/// Seddon & Goldsmith treat `r/R` (or `r/D`) as the bluntness parameter that
/// sets lip suction. This crate records the ratio and refuses to invent a
/// `C_s(r/R, M)` credit. Below this bound the no-suction force is the model's
/// stated range; above it the streamtube force is still computed and
/// [`AdditiveDrag::within_stated_range`] is false. **[TO CITE]** the figure
/// in Seddon & Goldsmith, *Intake Aerodynamics*, at which suction becomes
/// first-order at these Mach numbers; 0.05 is a declared nearly-sharp cut,
/// not a digit read from that figure.
pub const SHARP_LIP_RADIUS_RATIO_LIMIT: f64 = 0.05;

/// Design lip-radius ratio for a declared VENTUS cowl.
///
/// A supersonic lip is nearly sharp. 0.02 is a structural/thermal minimum,
/// not a cited optimum. **[TO DETERMINE]** against leading-edge heating (M5)
/// and a real lip drawing. It does not enter the force until a suction
/// correlation is cited.
pub const VENTUS_COWL_LIP_RADIUS_RATIO: f64 = 0.02;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureError {
    /// An input was NaN.
    NotANumber,
    /// An area or speed was zero or negative.
    NonPhysical,
    /// Additive drag was asked for without a cowl-lip geometry. The spilled
    /// *area* is still available from [`spillage`]; the force is not.
    CowlLipNotModelled,
    /// The pitot-equivalent lip closure needs a supersonic normal shock.
    Subsonic,
    /// `A₀ > A_c`: the highlight cannot swallow the streamtube. That is not
    /// spillage, and the pre-entry control volume is not this operating point.
    CaptureExceedsCowl,
    Gas(GasDynError),
}

impl From<GasDynError> for CaptureError {
    fn from(e: GasDynError) -> Self {
        match e {
            GasDynError::NotANumber => CaptureError::NotANumber,
            GasDynError::NegativeMach | GasDynError::InvalidGamma => CaptureError::NonPhysical,
            GasDynError::SubsonicUpstream => CaptureError::Subsonic,
            other => CaptureError::Gas(other),
        }
    }
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

/// Cowl-lip *flow* state needed to close the additive-drag control volume.
///
/// Geometry lives on [`CowlLipGeometry`]. This is `p_c`, `u_c` and `A_c` at
/// the highlight, however they were obtained.
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

impl FreestreamStation {
    /// Freestream Mach from `V / sqrt(γ p / ρ)`.
    ///
    /// # Errors
    /// [`CaptureError::NonPhysical`] if gamma is not greater than 1 or the
    /// speed of sound would not be positive.
    pub fn mach(&self, gamma: f64) -> Result<f64, CaptureError> {
        check_finite(gamma)?;
        if gamma <= 1.0 {
            return Err(CaptureError::NonPhysical);
        }
        check_positive(self.density_kg_m3)?;
        check_positive(self.velocity_m_s)?;
        check_finite(self.static_pressure_pa)?;
        if self.static_pressure_pa < 0.0 {
            return Err(CaptureError::NonPhysical);
        }
        let a2 = gamma * self.static_pressure_pa / self.density_kg_m3;
        if a2 <= 0.0 || !a2.is_finite() {
            return Err(CaptureError::NonPhysical);
        }
        Ok(self.velocity_m_s / libm::sqrt(a2))
    }

    /// Dynamic pressure `½ ρ V²` [Pa].
    #[must_use]
    pub fn dynamic_pressure_pa(&self) -> f64 {
        0.5 * self.density_kg_m3 * self.velocity_m_s * self.velocity_m_s
    }
}

/// Declared cowl-lip **geometry**. Without this, additive-drag force is
/// [`additive_drag_without_lip`].
///
/// Seddon & Goldsmith, *Intake Aerodynamics*: the highlight area `A_c` is
/// the capture-plane reference for additive drag, and the lip radius ratio
/// `r/R` (equivalently `r/D`) is the bluntness that sets lip suction. Both
/// are stored. Only `A_c` enters the streamtube force in this crate; `r/R`
/// is a range flag until a cited suction correlation exists. **[TO CITE]**
/// the chapter/figure that tabulates `C_s(r/R, M)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CowlLipGeometry {
    /// Highlight (capture-plane) area [m²].
    pub highlight_area_m2: f64,
    /// Leading-edge radius of the lip [m]. Zero is a perfectly sharp lip.
    pub lip_radius_m: f64,
}

impl CowlLipGeometry {
    /// Axisymmetric highlight from area and `r/R`.
    ///
    /// `lip_radius_ratio = 0` is a sharp lip and is allowed.
    ///
    /// # Errors
    /// [`CaptureError::NonPhysical`] if the area is not positive or `r/R` is
    /// negative.
    pub fn from_highlight_and_radius_ratio(
        highlight_area_m2: f64,
        lip_radius_ratio: f64,
    ) -> Result<Self, CaptureError> {
        check_positive(highlight_area_m2)?;
        check_finite(lip_radius_ratio)?;
        if lip_radius_ratio < 0.0 {
            return Err(CaptureError::NonPhysical);
        }
        let highlight_radius_m = libm::sqrt(highlight_area_m2 / core::f64::consts::PI);
        Ok(Self {
            highlight_area_m2,
            lip_radius_m: lip_radius_ratio * highlight_radius_m,
        })
    }

    /// VENTUS design lip on a given highlight: [`VENTUS_COWL_LIP_RADIUS_RATIO`].
    pub fn ventus_on_highlight(highlight_area_m2: f64) -> Result<Self, CaptureError> {
        Self::from_highlight_and_radius_ratio(highlight_area_m2, VENTUS_COWL_LIP_RADIUS_RATIO)
    }

    /// Highlight radius [m], `sqrt(A_c / π)`.
    #[must_use]
    pub fn highlight_radius_m(self) -> f64 {
        libm::sqrt(self.highlight_area_m2 / core::f64::consts::PI)
    }

    /// Highlight diameter [m].
    #[must_use]
    pub fn highlight_diameter_m(self) -> f64 {
        2.0 * self.highlight_radius_m()
    }

    /// `r / R`, the Seddon & Goldsmith bluntness parameter.
    #[must_use]
    pub fn lip_radius_ratio(self) -> f64 {
        self.lip_radius_m / self.highlight_radius_m()
    }

    /// Frontal projected area of a circular-arc lip that stands proud by `r`
    /// around the highlight: `π r (2R + r)` [m²].
    ///
    /// This is cowl *wave-drag* area, not additive-drag area. Reported so the
    /// geometry is complete; it does not enter [`additive_drag_from_lip`].
    #[must_use]
    pub fn projected_lip_area_m2(self) -> f64 {
        let r = self.lip_radius_m;
        let highlight_radius_m = self.highlight_radius_m();
        core::f64::consts::PI * r * (2.0 * highlight_radius_m + r)
    }
}

/// Pre-entry (additive) drag from a declared lip, with the range flag.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AdditiveDrag {
    /// Axial force [N], positive aft.
    pub force_n: f64,
    /// `D_add / (q∞ A_c)`.
    pub coefficient: f64,
    pub mass_flow_ratio: f64,
    /// `r/R` of the declared lip.
    pub lip_radius_ratio: f64,
    /// True when the pitot-equivalent, no-suction model is inside its
    /// stated range: supersonic, `μ ≤ 1`, `r/R` at or below
    /// [`SHARP_LIP_RADIUS_RATIO_LIMIT`].
    pub within_stated_range: bool,
}

/// Pre-entry (additive) drag from the streamtube control-volume definition.
///
/// Seddon & Goldsmith, *Intake Aerodynamics*: the additive drag is the axial
/// force on the captured streamtube between freestream and the cowl lip,
/// written as the integral of gauge pressure over the pre-entry surface,
///
/// ```text
///   D_add = (p∞ A₀ + ṁ V∞) − (p_c A_c + ṁ u_c) + p∞ (A_c − A₀)
///         = ṁ (V∞ − u_c) + (p∞ − p_c) A_c
/// ```
///
/// with `ṁ = ρ∞ V∞ A₀`. This is a definition, not a correlation.
///
/// [CORRECTED] An earlier revision omitted `p∞ (A_c − A₀)`. That term is
/// identically zero at shock-on-lip (`A₀ = A_c`), which was the only identity
/// asserted, so the tests stayed green while subcritical force was the
/// ungaged streamtube momentum. The incompressible sharp-lip result
/// `C_D,add = 2 μ (1 − μ)` is the gauge form with `p_c = p∞` and
/// `u_c = μ V∞`; recovering it is the test that the term is now present.
///
/// Lip suction that would act on the *solid* lip is not in this integral.
/// If the caller measured `p_c` and `u_c`, they are used as given.
///
/// At shock-on-lip with a uniform streamtube (`A₀ = A_c`, `p_c = p∞`,
/// `u_c = V∞`) this is **identically zero**. That is still the test, and it
/// is why a started external-compression inlet at design has no additive
/// drag in this accounting.
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
    // Gauge form: ṁ (V∞ − u_c) + (p∞ − p_c) A_c.
    Ok(
        mass_flow * (freestream.velocity_m_s - lip.axial_velocity_m_s)
            + (freestream.static_pressure_pa - lip.static_pressure_pa) * lip.area_m2,
    )
}

/// Additive drag without a cowl-lip geometry. Always a refusal: the spilled
/// area is [`spillage`]; inventing a `C_D` from mass-flow ratio alone at
/// M 3.5 would be an incompressible fudge in the wrong regime.
pub fn additive_drag_without_lip(streamtube_m2: f64, cowl_m2: f64) -> Result<f64, CaptureError> {
    // Physical checks first, so a NaN cannot hide inside the gap.
    let _ = mass_flow_ratio(streamtube_m2, cowl_m2)?;
    Err(CaptureError::CowlLipNotModelled)
}

/// Additive drag from a **declared** cowl lip.
///
/// Pitot-equivalent sketch, composed from M2 rather than transcribed:
/// the pre-entry surface that the spilled streamtube presents to the cowl
/// has axial projection `(A_c − A₀)+`, and that surface sits at the static
/// pressure behind a normal shock at the freestream Mach,
///
/// ```text
///   D_add = (p₂ − p∞) (A_c − A₀)+
/// ```
///
/// with `p₂/p∞` from [`normal_shock`]. At shock-on-lip the spilled area is
/// zero and the force is zero. At μ → 0 the force is the gauge-pressure
/// drag of a pitot disk of area `A_c`.
///
/// This is **not** a four-ramp mixed-compression external field, and it is
/// **not** `C_D = 2 μ (1 − μ)` (that identity is the incompressible
/// sharp-lip result, recovered by [`additive_drag_n`] when `p_c = p∞` and
/// `u_c = μ V∞`, and refused as a standalone correlation at M 3.5). A real
/// VENTUS cowl sits behind those ramps; the detached-shock sketch puts more
/// of the compression on the pre-entry surface than the ramps would.
/// **[TO CITE]** Seddon & Goldsmith, *Intake Aerodynamics*, additive-drag /
/// pre-entry chapter, for the streamtube force; the `p₂` closure is this
/// crate's composition of M2.
///
/// Lip suction is not credited. Two lips that differ only in `r/R` return
/// the same force. `r/R` is a range flag, not a multiply.
///
/// # Errors
/// [`CaptureError::Subsonic`] below M = 1; [`CaptureError::CaptureExceedsCowl`]
/// when `A₀ > A_c`.
pub fn additive_drag_from_lip(
    freestream: FreestreamStation,
    lip: CowlLipGeometry,
    gamma: f64,
) -> Result<AdditiveDrag, CaptureError> {
    let spilled = spillage(freestream.streamtube_area_m2, lip.highlight_area_m2)?;
    if spilled.capture_exceeds_cowl {
        return Err(CaptureError::CaptureExceedsCowl);
    }
    let mach = freestream.mach(gamma)?;
    if mach < 1.0 {
        return Err(CaptureError::Subsonic);
    }
    let shock = normal_shock(mach, gamma)?;
    let p2 = freestream.static_pressure_pa * shock.pressure_ratio;
    if p2 < 0.0 || !p2.is_finite() {
        return Err(CaptureError::NonPhysical);
    }
    let force_n = (p2 - freestream.static_pressure_pa) * spilled.spilled_area_m2;
    let q_a = freestream.dynamic_pressure_pa() * lip.highlight_area_m2;
    let coefficient = if q_a > 0.0 { force_n / q_a } else { 0.0 };
    let lip_radius_ratio = lip.lip_radius_ratio();
    let within_stated_range = mach >= 1.0
        && spilled.mass_flow_ratio <= 1.0
        && lip_radius_ratio <= SHARP_LIP_RADIUS_RATIO_LIMIT + 1e-15;
    Ok(AdditiveDrag {
        force_n,
        coefficient,
        mass_flow_ratio: spilled.mass_flow_ratio,
        lip_radius_ratio,
        within_stated_range,
    })
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

    /// [CORRECTED] The gauge term `p∞(A_c − A₀)` recovers the incompressible
    /// sharp-lip result `C_D,add = 2 μ (1 − μ)` when `p_c = p∞` and
    /// `u_c = μ V∞`. If this drifted, subcritical force would be the ungaged
    /// streamtube again.
    #[test]
    fn gauge_additive_drag_recovers_the_incompressible_sharp_lip_coefficient() {
        let fs = FreestreamStation {
            streamtube_area_m2: 2.0,
            static_pressure_pa: 2153.0,
            density_kg_m3: 0.0337,
            velocity_m_s: 1047.0,
        };
        let mu = 2.0 / 3.0;
        let lip = CowlLip {
            area_m2: 3.0,
            static_pressure_pa: fs.static_pressure_pa,
            axial_velocity_m_s: mu * fs.velocity_m_s,
        };
        let d = additive_drag_n(fs, lip).unwrap();
        let cd = d / (fs.dynamic_pressure_pa() * lip.area_m2);
        let textbook = 2.0 * mu * (1.0 - mu);
        assert!(
            rel_err(cd, textbook) < 1e-12,
            "incompressible C_D,add moved to {cd} against 2μ(1−μ) = {textbook}"
        );
    }

    /// Design-point atmosphere, M 3.50 / 26 km. Numbers from
    /// `docs/design-point.md` §1–2.
    fn design_freestream(streamtube_m2: f64) -> FreestreamStation {
        FreestreamStation {
            streamtube_area_m2: streamtube_m2,
            static_pressure_pa: 2153.09,
            density_kg_m3: 0.0336882,
            velocity_m_s: 1046.95,
        }
    }

    /// A declared lip at shock-on-lip still gives zero additive drag: there
    /// is no spilled area to put p₂ on.
    #[test]
    fn declared_lip_at_shock_on_lip_still_has_zero_additive_drag() {
        let fs = design_freestream(2.511);
        let lip = CowlLipGeometry::ventus_on_highlight(2.511).unwrap();
        assert!(rel_err(lip.lip_radius_ratio(), VENTUS_COWL_LIP_RADIUS_RATIO) < 1e-15);
        let d = additive_drag_from_lip(fs, lip, GAMMA).unwrap();
        assert!(
            ventus_units::float::abs(d.force_n)
                / (fs.dynamic_pressure_pa() * lip.highlight_area_m2)
                < 1e-10,
            "shock-on-lip pitot-equivalent additive drag is {} N, not zero",
            d.force_n
        );
        assert!(d.within_stated_range);
        assert!(rel_err(d.mass_flow_ratio, 1.0) < 1e-15);
    }

    /// THE NUMBER A DECLARED LIP UNLOCKS. Subcritical μ = 2/3 at the snapshot
    /// atmosphere: D_add = (p₂ − p∞) × spilled area, with p₂ from M2. Not
    /// 2μ(1−μ): that is the incompressible fudge this module refused to print
    /// from μ alone.
    #[test]
    fn declared_lip_at_subcritical_spillage_computes_a_force() {
        let fs = design_freestream(2.0);
        let lip = CowlLipGeometry::ventus_on_highlight(3.0).unwrap();
        let d = additive_drag_from_lip(fs, lip, GAMMA).unwrap();
        assert!(d.force_n > 0.0, "subcritical additive drag should be drag");
        let mach = fs.mach(GAMMA).unwrap();
        let shock = normal_shock(mach, GAMMA).unwrap();
        let p2 = fs.static_pressure_pa * shock.pressure_ratio;
        let from_definition = (p2 - fs.static_pressure_pa) * 1.0;
        assert!(
            rel_err(d.force_n, from_definition) < 1e-12,
            "D_add {} N against (p2-p∞)A_spill {from_definition}",
            d.force_n
        );
        let incompressible = 2.0 * d.mass_flow_ratio * (1.0 - d.mass_flow_ratio);
        assert!(
            rel_err(d.coefficient, incompressible) > 0.05,
            "pitot-equivalent C_D,add collapsed onto 2μ(1−μ) = {incompressible}; \
             that is the incompressible fudge at the wrong Mach"
        );
        assert!(d.within_stated_range);
        assert!(rel_err(d.mass_flow_ratio, 2.0 / 3.0) < 1e-15);
        // r/R does not enter the force: suction is not credited.
        let sharp = CowlLipGeometry::from_highlight_and_radius_ratio(3.0, 0.0).unwrap();
        let d_sharp = additive_drag_from_lip(fs, sharp, GAMMA).unwrap();
        assert!(
            rel_err(d.force_n, d_sharp.force_n) < 1e-14,
            "lip radius changed the streamtube force; suction was smuggled in"
        );
        std::println!(
            "subcritical μ = {:.4}: D_add = {:.1} N, C_D,add = {:.4} (2μ(1−μ) = {incompressible:.4})",
            d.mass_flow_ratio,
            d.force_n,
            d.coefficient
        );
    }

    /// Geometry identities: diameter from area, projected lip from r(2R+r).
    #[test]
    fn cowl_lip_geometry_is_the_axisymmetric_highlight() {
        let lip =
            CowlLipGeometry::from_highlight_and_radius_ratio(core::f64::consts::PI, 0.02).unwrap();
        assert!(rel_err(lip.highlight_radius_m(), 1.0) < 1e-15);
        assert!(rel_err(lip.highlight_diameter_m(), 2.0) < 1e-15);
        assert!(rel_err(lip.lip_radius_m, 0.02) < 1e-15);
        let projected = core::f64::consts::PI * 0.02 * (2.0 + 0.02);
        assert!(rel_err(lip.projected_lip_area_m2(), projected) < 1e-15);
        assert_eq!(
            CowlLipGeometry::from_highlight_and_radius_ratio(1.0, -0.01),
            Err(CaptureError::NonPhysical)
        );
    }

    /// Supercritical capture and subsonic freestream refuse rather than
    /// inventing a spillage force for an operating point this model does not
    /// cover.
    #[test]
    fn additive_drag_from_a_lip_refuses_off_its_range() {
        let super_fs = design_freestream(4.0);
        let lip = CowlLipGeometry::ventus_on_highlight(3.0).unwrap();
        assert_eq!(
            additive_drag_from_lip(super_fs, lip, GAMMA),
            Err(CaptureError::CaptureExceedsCowl)
        );
        let sub = FreestreamStation {
            streamtube_area_m2: 2.0,
            static_pressure_pa: 101_325.0,
            density_kg_m3: 1.225,
            velocity_m_s: 100.0,
        };
        assert_eq!(
            additive_drag_from_lip(sub, lip, GAMMA),
            Err(CaptureError::Subsonic)
        );
        let blunt = CowlLipGeometry::from_highlight_and_radius_ratio(3.0, 0.20).unwrap();
        let d = additive_drag_from_lip(design_freestream(2.0), blunt, GAMMA).unwrap();
        assert!(!d.within_stated_range);
        assert!(d.force_n > 0.0);
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
