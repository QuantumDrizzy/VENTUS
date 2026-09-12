//! M11 - DAPCA IV acquisition cost, and an honest account of what it cannot do.
//!
//! DAPCA IV is the RAND parametric cost model as presented in Raymer,
//! *Aircraft Design: A Conceptual Approach*. It estimates engineering, tooling,
//! manufacturing and quality-control HOURS from three numbers - empty weight,
//! maximum velocity and production quantity - plus separate relations for
//! development support, flight test and materials.
//!
//! # Why this module is built differently from the other ten
//!
//! Every other module in VENTUS has a yardstick it can be held to. This one has
//! a methodology whose fit data does not reach the flight regime the project is
//! about, and it says so structurally rather than in a comment:
//!
//! - [`Estimate::validity`] is not optional and not a warning. A caller holds
//!   the extrapolation record in the same struct as the number, so quoting the
//!   cost without the caveat takes deliberate effort.
//! - The engine term is **absent**, not defaulted. See [`Estimate`].
//!
//! # The result this module exists to report
//!
//! At the SR-71 point, moving to the VENTUS design Mach costs **+7.3 %** by
//! DAPCA IV. Switching the airframe from aluminium to titanium costs **+31 %**.
//! The velocity exponents are mild because the fit sample never contained an
//! aircraft where speed FORCED the material - and at M 3.5 it does. M5 showed
//! the recovery temperature sits above the Ti-6Al-4V sustained limit, and that
//! the margin belongs to the radiation balance rather than to the alloy.
//!
//! So DAPCA IV routes the physics of high-speed flight through a fudge factor a
//! human types in, and prices the Mach step itself at almost nothing. That is
//! the honest headline: **this model cannot see what makes M 3.5 expensive.**
//! It is still worth having, because a model whose blind spot is known and
//! quantified beats no model - but the blind spot is the result, not a footnote.
//!
//! # Units
//!
//! DAPCA IV is defined in pounds, knots and 1986 US dollars, which is what it
//! was fitted in, so the relations are evaluated in those rather than silently
//! re-fitted to SI. [`Inputs`] takes SI and converts at the boundary, which is
//! the project convention (ADR-000 D7).

#![no_std]
#![forbid(unsafe_code)]

#[cfg(test)]
extern crate std;

pub mod dapca;

pub use dapca::{Envelope, Hours, Rates};

/// Refusals: questions the model cannot answer at all. A number it answers
/// badly is reported through [`Validity`] instead, not through an error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CostError {
    NotANumber,
    /// Weight, velocity, quantity and flight-test aircraft must all be positive.
    NonPhysicalInput,
    /// A material factor outside the range Raymer tabulates. Extrapolating a
    /// fudge factor is not a defensible thing to do, so this refuses.
    MaterialFactorOutOfRange,
}

/// What a programme is made of, in SI.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Inputs {
    /// Empty mass, in DAPCA IV's sense.
    ///
    /// **A SUBSTITUTION IS BEING MADE HERE AND IT IS NOT FREE.** DAPCA IV was
    /// fitted against empty weight. What M7 can supply is a ZERO-FUEL mass,
    /// because that is the quantity the SR-71 sources actually state, and zero
    /// fuel weight includes crew, oil and sensors that empty weight does not.
    ///
    /// ZFW is therefore an over-estimate of We, and since every DAPCA relation
    /// is increasing in We, every figure this module produces is biased HIGH by
    /// that difference. For the SR-71 the flight manual's 3 500 lb zero-fuel
    /// spread is roughly the size of the payload term, so the bias is of order a
    /// few per cent - small against an extrapolation already 1.7x past the fit,
    /// and stated rather than absorbed.
    pub empty_mass_kg: f64,
    /// Maximum velocity. From M1 and the design Mach, computed rather than
    /// recited.
    pub max_velocity_m_s: f64,
    /// Production quantity. The single largest lever in the model.
    pub production_quantity: f64,
    /// Aircraft consumed by the flight-test programme.
    pub flight_test_aircraft: f64,
    /// Raymer's material fudge factor. 1.0 is aluminium, which is what DAPCA was
    /// fitted on; titanium is 1.7-2.2. **[TO VERIFY]** against a primary copy,
    /// including WHICH terms it multiplies - see [`dapca::MATERIAL_FACTOR_NOTE`].
    pub material_factor: f64,
}

/// Whether the inputs sit inside the data DAPCA IV was fitted on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Validity {
    WithinFit,
    /// Outside the fit, carrying by how much per variable - so a caller can say
    /// how far out it is, not merely that it is out.
    Extrapolated(Extrapolation),
}

impl Validity {
    #[must_use]
    pub fn is_extrapolated(&self) -> bool {
        matches!(self, Validity::Extrapolated(_))
    }
}

/// How far past the fit each variable reaches, as a ratio. At or below 1.0 the
/// variable is inside.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Extrapolation {
    pub velocity_ratio: f64,
    pub empty_weight_ratio: f64,
}

impl Extrapolation {
    /// The worst single overshoot, for a one-line summary.
    #[must_use]
    pub fn worst(&self) -> f64 {
        if self.velocity_ratio > self.empty_weight_ratio {
            self.velocity_ratio
        } else {
            self.empty_weight_ratio
        }
    }
}

/// A costed programme, in 1986 US dollars.
///
/// # What is deliberately missing
///
/// **The engine.** DAPCA IV's engine relation takes a turbine inlet temperature.
/// A ramjet has no turbine. The term is not defaulted to zero and not
/// approximated: it is absent, which makes every total here an **airframe-only
/// LOWER BOUND**. M4 chose a ramjet for reasons that had nothing to do with
/// cost, and the consequence is that the one component this model could have
/// priced against a published relation is the one it cannot price at all.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Estimate {
    pub hours: Hours,
    pub development_usd: f64,
    pub flight_test_usd: f64,
    pub materials_usd: f64,
    /// Labour, at [`Rates`].
    pub labour_usd: f64,
    /// Airframe-only RDT&E plus flyaway. **Lower bound** - see the type docs.
    pub airframe_total_usd: f64,
    pub per_aircraft_usd: f64,
    /// Not optional, so the number cannot be read without it.
    pub validity: Validity,
}

/// The whole model at one programme point.
///
/// # Errors
/// [`CostError`] for non-physical input, or a material factor outside the
/// tabulated range.
pub fn estimate(
    inputs: &Inputs,
    rates: &Rates,
    envelope: &Envelope,
) -> Result<Estimate, CostError> {
    for v in [
        inputs.empty_mass_kg,
        inputs.max_velocity_m_s,
        inputs.production_quantity,
        inputs.flight_test_aircraft,
        inputs.material_factor,
    ] {
        if v.is_nan() {
            return Err(CostError::NotANumber);
        }
        if v <= 0.0 {
            return Err(CostError::NonPhysicalInput);
        }
    }
    if inputs.material_factor < dapca::MATERIAL_FACTOR_MIN
        || inputs.material_factor > dapca::MATERIAL_FACTOR_MAX
    {
        return Err(CostError::MaterialFactorOutOfRange);
    }

    let we_lb = dapca::kilograms_to_pounds(inputs.empty_mass_kg);
    let v_knots = dapca::metres_per_second_to_knots(inputs.max_velocity_m_s);
    let q = inputs.production_quantity;

    let hours = dapca::hours(we_lb, v_knots, q, inputs.material_factor);
    let development_usd = dapca::development_support_usd(we_lb, v_knots);
    let flight_test_usd = dapca::flight_test_usd(we_lb, v_knots, inputs.flight_test_aircraft);
    let materials_usd = dapca::materials_usd(we_lb, v_knots, q, inputs.material_factor);
    let labour_usd = hours.engineering * rates.engineering_usd_per_hour
        + hours.tooling * rates.tooling_usd_per_hour
        + hours.manufacturing * rates.manufacturing_usd_per_hour
        + hours.quality * rates.quality_usd_per_hour;

    let airframe_total_usd = labour_usd + development_usd + flight_test_usd + materials_usd;

    Ok(Estimate {
        hours,
        development_usd,
        flight_test_usd,
        materials_usd,
        labour_usd,
        airframe_total_usd,
        per_aircraft_usd: airframe_total_usd / q,
        validity: envelope.classify(we_lb, v_knots),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dapca::{
        MATERIAL_FACTOR_ALUMINIUM, MATERIAL_FACTOR_TITANIUM_HIGH, MATERIAL_FACTOR_TITANIUM_LOW,
    };

    /// SR-71 at M 3.2 / 24 km, by M1. Empty mass and quantity are **[TO CITE]**.
    const V_SR71_M_S: f64 = 952.899_424_000_000_1;
    /// VENTUS-1 at M 3.5 / 26 km, by M1.
    const V_VENTUS_M_S: f64 = 1_046.946_565;

    fn sr71() -> Inputs {
        Inputs {
            // [CORRECTED] was 30 600.0, recited. This is the cited zero-fuel mass:
            // 59 000 lb, NASA test-bed report Table 1, inside the flight manual band.
            empty_mass_kg: 26_761.949_83,
            max_velocity_m_s: V_SR71_M_S,
            production_quantity: 32.0,
            flight_test_aircraft: 2.0,
            material_factor: MATERIAL_FACTOR_TITANIUM_LOW,
        }
    }

    fn ventus() -> Inputs {
        Inputs {
            // M7 derives this: 28 t cruise mass at the cited SR-71 zero-fuel
            // fraction. [CORRECTED] was 15 578.18, from a recited SR-71 mass.
            empty_mass_kg: 13_624.265_368,
            max_velocity_m_s: V_VENTUS_M_S,
            production_quantity: 6.0,
            flight_test_aircraft: 2.0,
            material_factor: MATERIAL_FACTOR_TITANIUM_LOW,
        }
    }

    fn run(i: &Inputs) -> Estimate {
        estimate(i, &Rates::raymer_1986(), &Envelope::conventional_metal()).unwrap()
    }

    /// THE RESULT THIS MODULE EXISTS FOR.
    ///
    /// DAPCA IV prices the Mach step at almost nothing and the material at a
    /// great deal, and the material factor is a number a human types in. So the
    /// physics that actually makes M 3.5 hard reaches the cost only through a
    /// fudge factor, while the velocity term, the one part of the model that
    /// knows about speed at all, barely moves.
    #[test]
    fn the_material_factor_outweighs_the_mach_step_by_a_wide_margin() {
        let base = run(&sr71());
        let faster = run(&Inputs {
            max_velocity_m_s: V_VENTUS_M_S,
            ..sr71()
        });
        let aluminium = run(&Inputs {
            material_factor: MATERIAL_FACTOR_ALUMINIUM,
            ..sr71()
        });

        let mach_step = faster.airframe_total_usd / base.airframe_total_usd - 1.0;
        let material_step = base.airframe_total_usd / aluminium.airframe_total_usd - 1.0;

        std::println!(
            "M 3.2 to M 3.5 costs {:+.2} %; aluminium to titanium costs {:+.2} %",
            100.0 * mach_step,
            100.0 * material_step
        );

        assert!(
            mach_step > 0.0 && mach_step < 0.10,
            "the Mach step should be small and positive, got {mach_step:+.4}"
        );
        assert!(
            material_step > 2.0 * mach_step,
            "material {material_step:+.4} should dominate the Mach step {mach_step:+.4}"
        );
    }

    /// A SECOND COUNTERINTUITIVE RESULT, AND THE ONE A PROGRAMME MANAGER CARES
    /// ABOUT. VENTUS-1 is under half the SR-71 empty mass and costs MORE per
    /// aircraft, because six is not thirty-two. At these quantities the size of
    /// the production run dominates the size of the aeroplane.
    #[test]
    fn a_short_production_run_costs_more_per_aircraft_than_a_heavier_long_one() {
        let anchor = run(&sr71());
        let v = run(&ventus());

        assert!(
            ventus().empty_mass_kg < 0.6 * sr71().empty_mass_kg,
            "this test is only interesting if VENTUS is much the lighter aircraft"
        );
        assert!(
            v.per_aircraft_usd > anchor.per_aircraft_usd,
            "the lighter aircraft over a sixth of the run came out cheaper per unit, which would mean the quantity exponent is not being applied: {:.3e} against {:.3e}",
            v.per_aircraft_usd,
            anchor.per_aircraft_usd
        );

        // AND THE HALF THAT MAKES THE CLAIM PRECISE. Run the two at the SAME
        // quantity and the ordering inverts: VENTUS is the lighter aircraft and
        // comes out the cheaper one. So the result above is ENTIRELY a quantity
        // effect, not a statement about the vehicle. Without this half, the
        // first assertion reads as "small aircraft cost more", which is false.
        let matched = run(&Inputs {
            production_quantity: 32.0,
            ..ventus()
        });
        assert!(
            matched.per_aircraft_usd < anchor.per_aircraft_usd,
            "at equal quantity the lighter aircraft should be cheaper: {:.3e} against {:.3e}",
            matched.per_aircraft_usd,
            anchor.per_aircraft_usd
        );
        std::println!(
            "per aircraft: VENTUS at Q=6 {:.3e}, at Q=32 {:.3e}, SR-71 at Q=32 {:.3e}",
            v.per_aircraft_usd,
            matched.per_aircraft_usd,
            anchor.per_aircraft_usd
        );
    }

    /// Both reference points are outside the fit, and VENTUS-1 is further out.
    /// The module's honest summary of itself.
    #[test]
    fn neither_reference_point_is_inside_the_fit() {
        let anchor = run(&sr71());
        let v = run(&ventus());

        let (Validity::Extrapolated(a), Validity::Extrapolated(b)) = (anchor.validity, v.validity)
        else {
            panic!("a Mach 3 aircraft landed inside a fit for conventional metal aircraft");
        };
        assert!(a.velocity_ratio > 1.5, "anchor ratio {}", a.velocity_ratio);
        assert!(b.velocity_ratio > a.velocity_ratio);

        // The gap the whole module turns on: only about 10 % in velocity between
        // the aircraft this can be sanity-checked against and the one it is
        // being asked about.
        let gap = b.velocity_ratio / a.velocity_ratio;
        assert!(
            (1.09..1.11).contains(&gap),
            "anchor-to-target velocity gap moved to {gap:.4}"
        );
    }

    /// Exact arithmetic on the exponent, landing in the 75-85 % band quoted for
    /// airframe production **[TO CITE]**. The band is recited, so it is not
    /// asserted as a yardstick; the arithmetic is.
    #[test]
    fn the_implied_learning_curve_is_exact_and_plausible() {
        let lc = dapca::implied_learning_curve();
        assert_eq!(lc, libm::pow(2.0, 0.641 - 1.0));
        assert!((0.75..0.85).contains(&lc), "learning curve {lc}");
    }

    /// The identity above is about the exponent. This is about the code using
    /// it: doubling the run must actually multiply per-aircraft manufacturing
    /// hours by the curve.
    #[test]
    fn doubling_the_run_applies_the_learning_curve_to_manufacturing() {
        let one = run(&sr71());
        let two = run(&Inputs {
            production_quantity: 64.0,
            ..sr71()
        });
        let ratio = (two.hours.manufacturing / 64.0) / (one.hours.manufacturing / 32.0);
        assert!(
            (ratio - dapca::implied_learning_curve()).abs() < 1e-12,
            "{ratio} against {}",
            dapca::implied_learning_curve()
        );
    }

    /// A fudge factor outside the tabulated band is refused, not extrapolated.
    /// Same stance as `gamma_air` below 273 K in M2, and for the same reason:
    /// the flattering direction is the dangerous one.
    #[test]
    fn a_material_factor_outside_the_table_is_refused() {
        for bad in [0.5, 2.5, 10.0] {
            assert_eq!(
                estimate(
                    &Inputs {
                        material_factor: bad,
                        ..sr71()
                    },
                    &Rates::raymer_1986(),
                    &Envelope::conventional_metal(),
                ),
                Err(CostError::MaterialFactorOutOfRange),
                "material factor {bad} was accepted"
            );
        }
        for good in [MATERIAL_FACTOR_ALUMINIUM, MATERIAL_FACTOR_TITANIUM_HIGH] {
            assert!(estimate(
                &Inputs {
                    material_factor: good,
                    ..sr71()
                },
                &Rates::raymer_1986(),
                &Envelope::conventional_metal(),
            )
            .is_ok());
        }
    }

    #[test]
    fn non_physical_inputs_are_refused() {
        let breakers: [fn(&mut Inputs); 4] = [
            |i| i.empty_mass_kg = 0.0,
            |i| i.max_velocity_m_s = -1.0,
            |i| i.production_quantity = 0.0,
            |i| i.flight_test_aircraft = -2.0,
        ];
        for f in breakers {
            let mut bad = sr71();
            f(&mut bad);
            assert_eq!(
                estimate(&bad, &Rates::raymer_1986(), &Envelope::conventional_metal()),
                Err(CostError::NonPhysicalInput)
            );
        }
        let mut nan = sr71();
        nan.empty_mass_kg = f64::NAN;
        assert_eq!(
            estimate(&nan, &Rates::raymer_1986(), &Envelope::conventional_metal()),
            Err(CostError::NotANumber)
        );
    }

    /// The breakdown must add up. Cheap, and it catches a term being dropped
    /// from the total when one is added to the struct.
    #[test]
    fn the_breakdown_sums_to_the_total() {
        let e = run(&sr71());
        let sum = e.labour_usd + e.development_usd + e.flight_test_usd + e.materials_usd;
        assert!((sum - e.airframe_total_usd).abs() < 1e-9 * e.airframe_total_usd);
        assert!(
            (e.per_aircraft_usd * 32.0 - e.airframe_total_usd).abs() < 1e-9 * e.airframe_total_usd
        );
        assert!((e.hours.quality / e.hours.manufacturing - 0.133).abs() < 1e-15);
    }

    /// The conversions are part of the model, not presentation (ADR-002), so
    /// they are pinned here rather than trusted to a crate this one does not
    /// depend on.
    #[test]
    fn the_unit_conversions_are_the_defining_ones() {
        // 1 lb = 0.45359237 kg exactly.
        assert!((dapca::kilograms_to_pounds(0.453_592_37) - 1.0).abs() < 1e-15);
        // 1 kn = 1852 m/h exactly.
        assert!((dapca::metres_per_second_to_knots(1852.0 / 3600.0) - 1.0).abs() < 1e-15);
        // And the two reference velocities, in the units DAPCA is dimensioned in.
        assert!((dapca::metres_per_second_to_knots(V_SR71_M_S) - 1852.288).abs() < 0.01);
        assert!((dapca::metres_per_second_to_knots(V_VENTUS_M_S) - 2035.101).abs() < 0.01);
    }
}
