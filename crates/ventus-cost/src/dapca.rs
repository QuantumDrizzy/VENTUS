//! The DAPCA IV relations themselves, in the units they were fitted in.
//!
//! **[TO VERIFY] — every coefficient and exponent below is recited, not read
//! from a primary copy of Raymer.** They are marked once here rather than on
//! each line. Until a primary copy is checked, this module's absolute numbers
//! are provisional; its RATIOS are not, because a common error in the
//! coefficients cancels in a ratio, which is why the validated cases lean on
//! ratios and the programme totals are carried as a known limit.

use crate::{Extrapolation, Validity};

/// Labour rates, 1986 US dollars per hour. **[TO VERIFY]**
///
/// These are the wrap rates Raymer tabulates alongside the hour relations. They
/// carry a dollar-year, which is why nothing in this module is quoted in
/// present-day money: escalating 1986 dollars needs a cited index, and reciting
/// one from memory would put a fabricated multiplier on every number here.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rates {
    pub engineering_usd_per_hour: f64,
    pub tooling_usd_per_hour: f64,
    pub manufacturing_usd_per_hour: f64,
    pub quality_usd_per_hour: f64,
}

impl Rates {
    /// Raymer's 1986 rates. **[TO VERIFY]**
    #[must_use]
    pub const fn raymer_1986() -> Self {
        Self {
            engineering_usd_per_hour: 59.10,
            tooling_usd_per_hour: 60.70,
            manufacturing_usd_per_hour: 50.10,
            quality_usd_per_hour: 55.40,
        }
    }
}

/// Where the material factor is applied.
///
/// Raymer tabulates a fudge factor for non-aluminium construction but the
/// presentation does not make unambiguous which terms it multiplies. This module
/// applies it to **manufacturing hours and material cost** - the two terms that
/// scale with how hard the metal is to cut and how much it costs per pound - and
/// NOT to engineering, tooling, development support or flight test.
///
/// That is a modelling choice, not a citation, and it is the largest single
/// source of uncertainty in this module after the dollar-year. Applying it to
/// tooling as well would raise a titanium programme by a further several per
/// cent. **[TO VERIFY]**
pub const MATERIAL_FACTOR_NOTE: &str =
    "applied to manufacturing hours and material cost only; [TO VERIFY]";

/// Aluminium, the construction DAPCA IV was fitted on.
pub const MATERIAL_FACTOR_ALUMINIUM: f64 = 1.0;
/// Titanium, lower end of Raymer's band. **[TO VERIFY]**
pub const MATERIAL_FACTOR_TITANIUM_LOW: f64 = 1.7;
/// Titanium, upper end of Raymer's band. **[TO VERIFY]**
pub const MATERIAL_FACTOR_TITANIUM_HIGH: f64 = 2.2;

/// Below aluminium there is nothing to model: the factor is a penalty, never a
/// discount.
pub const MATERIAL_FACTOR_MIN: f64 = 1.0;
/// Past the top of the tabulated band the factor is being invented.
pub const MATERIAL_FACTOR_MAX: f64 = MATERIAL_FACTOR_TITANIUM_HIGH;

/// The exponent on production quantity in the manufacturing-hours relation.
///
/// Named because it is not a fitting detail - it IS the learning curve, and
/// [`implied_learning_curve`] is the one property of this model that can be
/// checked against published practice without a dollar figure.
pub const MANUFACTURING_QUANTITY_EXPONENT: f64 = 0.641;

/// The exponent on velocity in the development-support relation: the steepest
/// velocity dependence anywhere in DAPCA IV, and still only 1.3.
pub const DEVELOPMENT_VELOCITY_EXPONENT: f64 = 1.3;

/// Hours by category.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hours {
    pub engineering: f64,
    pub tooling: f64,
    pub manufacturing: f64,
    pub quality: f64,
}

impl Hours {
    #[must_use]
    pub fn total(&self) -> f64 {
        self.engineering + self.tooling + self.manufacturing + self.quality
    }
}

/// Quality control as a fraction of manufacturing hours, for a non-cargo
/// aircraft. Definitional inside DAPCA IV, so it is checked at zero ulp.
pub const QUALITY_FRACTION_OF_MANUFACTURING: f64 = 0.133;

/// Hours for a programme. `we` in pounds, `v` in knots, `q` aircraft.
#[must_use]
pub fn hours(we: f64, v: f64, q: f64, material_factor: f64) -> Hours {
    let engineering = 4.86 * pw(we, 0.777) * pw(v, 0.894) * pw(q, 0.163);
    let tooling = 5.99 * pw(we, 0.777) * pw(v, 0.696) * pw(q, 0.263);
    let manufacturing = 7.37
        * pw(we, 0.82)
        * pw(v, 0.484)
        * pw(q, MANUFACTURING_QUANTITY_EXPONENT)
        * material_factor;
    Hours {
        engineering,
        tooling,
        manufacturing,
        quality: QUALITY_FRACTION_OF_MANUFACTURING * manufacturing,
    }
}

/// Development support cost, 1986 USD.
#[must_use]
pub fn development_support_usd(we: f64, v: f64) -> f64 {
    45.42 * pw(we, 0.630) * pw(v, DEVELOPMENT_VELOCITY_EXPONENT)
}

/// Flight test cost, 1986 USD. `fta` is the number of aircraft the test
/// programme consumes.
#[must_use]
pub fn flight_test_usd(we: f64, v: f64, fta: f64) -> f64 {
    1243.03 * pw(we, 0.325) * pw(v, 0.822) * pw(fta, 1.21)
}

/// Manufacturing materials cost, 1986 USD.
#[must_use]
pub fn materials_usd(we: f64, v: f64, q: f64, material_factor: f64) -> f64 {
    11.0 * pw(we, 0.921) * pw(v, 0.621) * pw(q, 0.799) * material_factor
}

/// The learning curve DAPCA IV implies, as the per-aircraft cost ratio on
/// doubling the production run.
///
/// Total manufacturing hours go as `Q^0.641`, so the per-aircraft average goes
/// as `Q^(0.641-1)`, and doubling multiplies it by `2^-0.359`. This is exact
/// arithmetic from the exponent, not a fit, which is what makes it checkable
/// without any dollar anchor at all.
#[must_use]
pub fn implied_learning_curve() -> f64 {
    libm::pow(2.0, MANUFACTURING_QUANTITY_EXPONENT - 1.0)
}

/// The range of production quantities and speeds DAPCA IV was fitted over.
///
/// **[TO VERIFY], and the most consequential [TO VERIFY] in this module.** The
/// bounds below are a conservative reading of "conventional metal aircraft" and
/// are deliberately generous: a tighter envelope would make the extrapolation
/// look worse, so erring wide errs against this module's own conclusion.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Envelope {
    pub max_velocity_knots: f64,
    pub max_empty_weight_lb: f64,
}

impl Envelope {
    /// A generous reading of the fit sample. **[TO VERIFY]**
    ///
    /// 1200 knots is roughly M 2 at altitude - the fastest class of aircraft
    /// built in quantity when DAPCA IV was fitted. Both the SR-71 and VENTUS-1
    /// are past it, which is the point: **the SR-71 is not a validation of this
    /// model, it is a measurement of how far outside the fit the answer already
    /// is before VENTUS adds anything.**
    #[must_use]
    pub const fn conventional_metal() -> Self {
        Self {
            max_velocity_knots: 1200.0,
            max_empty_weight_lb: 150_000.0,
        }
    }

    #[must_use]
    pub fn classify(&self, we_lb: f64, v_knots: f64) -> Validity {
        let velocity_ratio = v_knots / self.max_velocity_knots;
        let empty_weight_ratio = we_lb / self.max_empty_weight_lb;
        if velocity_ratio <= 1.0 && empty_weight_ratio <= 1.0 {
            Validity::WithinFit
        } else {
            Validity::Extrapolated(Extrapolation {
                velocity_ratio,
                empty_weight_ratio,
            })
        }
    }
}

/// `libm::pow`, per ADR-001: the platform math library differs between machines
/// and these exponents are all irrational-ish fractions where that shows.
fn pw(x: f64, e: f64) -> f64 {
    libm::pow(x, e)
}

// ---------------------------------------------------------------------------
// Units: why this module converts inside itself instead of using
// `ventus_units::convert`.
// ---------------------------------------------------------------------------

/// ADR-000 D7 puts conversions in the presentation layer, and this module does
/// not break that rule - it falls outside it.
///
/// `convert::m_s_to_kn` exists so a REPORT can show an SI quantity in knots. The
/// knots here are not a presentation of anything: the coefficient 4.86 carries
/// units of hours per `lb^0.777 kn^0.894`, so pounds and knots are part of the
/// model's own definition. Re-expressing DAPCA IV in SI would mean re-deriving
/// every coefficient against the original fit data, which nobody in this project
/// has. Converting the INPUTS at the boundary and leaving the model in its own
/// units is the only option that does not silently invent a refit.
///
/// The identifiers still carry their unit, which is the part of D7 that is doing
/// the work.
pub const UNITS_NOTE: &str = "DAPCA IV is dimensioned in lb and kn; see ADR-002";

/// 1 lb = 0.453 592 37 kg exactly, by definition of the international pound.
const KILOGRAMS_PER_POUND: f64 = 0.453_592_37;

/// 1 kn = 1852 m/h exactly.
const METRES_PER_SECOND_PER_KNOT: f64 = 1852.0 / 3600.0;

/// Empty mass to the pounds DAPCA IV is dimensioned in.
#[inline]
#[must_use]
pub fn kilograms_to_pounds(kg: f64) -> f64 {
    kg / KILOGRAMS_PER_POUND
}

/// Velocity to the knots DAPCA IV is dimensioned in.
#[inline]
#[must_use]
pub fn metres_per_second_to_knots(m_s: f64) -> f64 {
    m_s / METRES_PER_SECOND_PER_KNOT
}
