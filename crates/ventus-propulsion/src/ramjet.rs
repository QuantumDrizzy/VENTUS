//! Ideal ramjet cycle, station by station.
//!
//! # Why a ramjet, and why the turbomachine is out of its element
//!
//! A turbojet extracts work from the flow to drive a compressor. The work
//! available scales with how much temperature headroom there is between the
//! compressor face and the turbine inlet limit. At the VENTUS-1 design point:
//!
//! ```text
//!   sea level static   T4max/T0 = 1700/288 = 5.90
//!   M 3.5 at 26 km     T4max/T0 = 1700/753 = 2.26
//! ```
//!
//! The ram compression has already used most of the budget before the
//! compressor sees the air. That is not a materials problem — it is a
//! thermodynamic one, and no compressor alloy fixes it. Past roughly M 3 the
//! sensible answer is to delete the turbomachine and let the inlet do the
//! compressing, which is a ramjet.
//!
//! # Stations
//!
//! ```text
//!   0   freestream
//!   2   burner entry      = inlet exit. p02 = p00 * pi_d, T02 = T00
//!   4   burner exit       T04 = turbine/liner limit, p04 = p02 * pi_b
//!   9   nozzle exit       expanded to ambient
//! ```
//!
//! # What is modelled, and what is not
//!
//! Modelled: the cycle, its specific thrust and its specific impulse, with
//! gamma taken at each station's own temperature (ADR-000 D10 makes gamma = 1.4
//! a failure in the burner and nozzle, not a known limit).
//!
//! NOT modelled: the component-by-component THRUST SPLIT. The often-quoted
//! SR-71 figure — inlet 54 %, ejector nozzle 29 %, engine 17 % — is an axial
//! force accounting over each piece of the flowpath, not a cycle result. It
//! needs the pressure distribution on the compression surfaces and the cowl,
//! which this quasi-1D model does not carry. Declared as a gap in
//! `docs/design-point.md`, not approximated into existence here.
//!
//! NOT modelled: flame stability. Burner-entry *total* pressure at the design
//! point is ram-compressed (~122 kPa with MIL recovery), not the ~1.6 kPa of
//! omitted-ram static. That pressure is afterburner-scale (King 36–86 kPa,
//! Useller 77 kPa), not turbojet-main-burner-scale. A Lefebvre-family
//! correlation would still need a combustor volume and a flame holder, which
//! this cycle does not have. The operative fly/no-fly comparison lives in
//! `ventus-envelope`, with the 0.50 digit cited from Useller Fig. 8.

use ventus_gasdyn::{gamma_air, specific_heat_air_j_kg_k, GasDynError};
use ventus_units::constants::G0_M_S2;

/// Lower heating value of a kerosene-class fuel [J/kg]. The two candidate
/// specifications bracket the class: MIL-T-38219 (JP-7) gives net heat of
/// combustion 43.5 MJ/kg minimum, ASTM D1655 (Jet A-1) 42.8 MJ/kg minimum.
/// 43.0 is carried as the kerosene-class value inside that band.
pub const KEROSENE_LHV_J_KG: f64 = 43.0e6;

/// Burner total-pressure ratio. A real combustor loses a few per cent to
/// friction and to heat addition at finite Mach number. Mattingly, Heiser &
/// Pratt, *Aircraft Engine Design*, 2nd ed., §10.7: "Total pressure losses of
/// 2 to 5 percent are typically encountered in current systems", i.e. a ratio
/// of 0.95-0.98 for a turbojet main burner. 0.95 is the conservative end of
/// that band, applied to a ramjet combustor — an extrapolation declared rather
/// than hidden.
pub const BURNER_PRESSURE_RATIO: f64 = 0.95;

/// Burner efficiency: the fraction of the fuel heating value that reaches the
/// gas. Same source, §10.7: design-point main-burner combustion efficiency is
/// "usually greater than 99.5 percent". 0.98 is carried below that figure,
/// deliberately: a ramjet burner at ~1.6 kPa entry pressure is off the
/// conditions the turbojet data covers.
pub const BURNER_EFFICIENCY: f64 = 0.98;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CycleError {
    /// The burner exit temperature is at or below the burner entry temperature,
    /// so there is no heat to add. Physically the engine is off.
    NoHeatAddition,
    /// Ram compression alone has already exceeded the burner limit. Above this
    /// Mach number the cycle cannot run at all without cooling the flow.
    RamTemperatureExceedsBurnerLimit,
    /// Fuel-air ratio came out non-physical.
    InvalidFuelAirRatio,
    Gas(GasDynError),
}

impl From<GasDynError> for CycleError {
    fn from(e: GasDynError) -> Self {
        CycleError::Gas(e)
    }
}

/// A solved cycle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cycle {
    pub mach_freestream: f64,
    /// Total temperature entering the burner [K]. Set entirely by ram
    /// compression: a ramjet has no other compressor.
    pub burner_entry_total_temperature_k: f64,
    /// Total pressure entering the burner [Pa].
    pub burner_entry_total_pressure_pa: f64,
    pub burner_exit_total_temperature_k: f64,
    pub burner_exit_total_pressure_pa: f64,
    /// Nozzle exit velocity [m/s], expanded to ambient.
    pub exit_velocity_m_s: f64,
    /// Net thrust per unit air mass flow [N/(kg/s)] = [m/s].
    pub specific_thrust_n_s_kg: f64,
    /// Fuel mass per unit air mass.
    pub fuel_air_ratio: f64,
    /// Specific impulse [s].
    pub specific_impulse_s: f64,
    /// gamma used in the burner and nozzle, at the burner exit temperature.
    pub burner_gamma: f64,
    /// The headroom ratio that decides whether a turbomachine is worth carrying.
    pub temperature_headroom_ratio: f64,
}

/// Solve the ideal ramjet cycle.
///
/// `inlet_recovery` is the total-pressure ratio the inlet delivers — the number
/// M3 computes. It enters the answer directly, which is the quantitative form of
/// "the inlet is most of the engine".
///
/// # Errors
/// [`CycleError`] if there is no heat to add, or if ram compression has already
/// passed the burner limit.
pub fn ideal_ramjet(
    mach_freestream: f64,
    freestream_static_temperature_k: f64,
    freestream_static_pressure_pa: f64,
    freestream_velocity_m_s: f64,
    inlet_recovery: f64,
    burner_exit_total_temperature_k: f64,
    gamma_freestream: f64,
) -> Result<Cycle, CycleError> {
    // Ram compression. A ramjet has no other compressor, so station 2 totals
    // come straight from the freestream totals times the inlet recovery.
    let t0_ratio = ventus_gasdyn::stagnation_temperature_ratio(mach_freestream, gamma_freestream)?;
    let p0_ratio = ventus_gasdyn::stagnation_pressure_ratio(mach_freestream, gamma_freestream)?;
    let t02 = freestream_static_temperature_k * t0_ratio;
    let p02 = freestream_static_pressure_pa * p0_ratio * inlet_recovery;

    if burner_exit_total_temperature_k <= t02 {
        return Err(CycleError::RamTemperatureExceedsBurnerLimit);
    }

    // ADR-000 D10: gamma at the burner temperature, not 1.4. At 1700 K that is
    // about 1.304, and using 1.4 here would falsify the nozzle expansion and
    // therefore the thrust.
    let burner_gamma = gamma_air(burner_exit_total_temperature_k)?;
    let cp_burner = specific_heat_air_j_kg_k(burner_exit_total_temperature_k)?;
    let cp_entry = specific_heat_air_j_kg_k(t02.max(273.0))?;

    let p04 = p02 * BURNER_PRESSURE_RATIO;

    // Nozzle expanded to ambient static pressure.
    let exponent = (burner_gamma - 1.0) / burner_gamma;
    let pressure_term = 1.0 - libm::pow(freestream_static_pressure_pa / p04, exponent);
    if pressure_term <= 0.0 {
        return Err(CycleError::NoHeatAddition);
    }
    let exit_velocity =
        libm::sqrt(2.0 * cp_burner * burner_exit_total_temperature_k * pressure_term);

    // Fuel-air ratio from an energy balance on the burner.
    let heat_required = cp_burner * burner_exit_total_temperature_k - cp_entry * t02;
    let heat_available =
        BURNER_EFFICIENCY * KEROSENE_LHV_J_KG - cp_burner * burner_exit_total_temperature_k;
    if heat_available <= 0.0 || heat_required <= 0.0 {
        return Err(CycleError::InvalidFuelAirRatio);
    }
    let fuel_air_ratio = heat_required / heat_available;

    // Net specific thrust. Including the fuel mass in the exit stream is the
    // small correction that keeps the momentum balance honest.
    let specific_thrust = (1.0 + fuel_air_ratio) * exit_velocity - freestream_velocity_m_s;

    Ok(Cycle {
        mach_freestream,
        burner_entry_total_temperature_k: t02,
        burner_entry_total_pressure_pa: p02,
        burner_exit_total_temperature_k,
        burner_exit_total_pressure_pa: p04,
        exit_velocity_m_s: exit_velocity,
        specific_thrust_n_s_kg: specific_thrust,
        fuel_air_ratio,
        specific_impulse_s: specific_thrust / (fuel_air_ratio * G0_M_S2),
        burner_gamma,
        temperature_headroom_ratio: burner_exit_total_temperature_k / t02,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ventus_units::float::rel_err;

    const T4_MAX: f64 = 1700.0;

    /// The design point, built from M1 and M3 rather than from constants.
    fn design_point(inlet_recovery: f64) -> Cycle {
        let a = ventus_atmos::at_geopotential(26_000.0).unwrap();
        ideal_ramjet(
            3.5,
            a.temperature_k,
            a.pressure_pa,
            3.5 * a.speed_of_sound_m_s,
            inlet_recovery,
            T4_MAX,
            1.4,
        )
        .unwrap()
    }

    /// THE DEFINING PROPERTY OF A RAMJET, and it falls out rather than being
    /// asserted: with no forward speed there is no ram compression, so there is
    /// no pressure ratio to expand across and no thrust. A ramjet cannot take off.
    #[test]
    fn a_ramjet_produces_no_static_thrust() {
        let a = ventus_atmos::at_geopotential(0.0).unwrap();
        let cycle = ideal_ramjet(0.0, a.temperature_k, a.pressure_pa, 0.0, 1.0, T4_MAX, 1.4);
        // At M = 0 the burner pressure loss alone puts p04 below ambient, so the
        // nozzle cannot expand at all: the model refuses rather than returning a
        // negative thrust dressed as a number.
        assert_eq!(cycle, Err(CycleError::NoHeatAddition));

        // Even once it will run, thrust at low Mach is feeble and has to be
        // reached by something else. Every ramjet needs a booster.
        let low = ideal_ramjet(
            1.0,
            a.temperature_k,
            a.pressure_pa,
            a.speed_of_sound_m_s,
            1.0,
            T4_MAX,
            1.4,
        );
        let mid = ideal_ramjet(
            2.0,
            a.temperature_k,
            a.pressure_pa,
            2.0 * a.speed_of_sound_m_s,
            1.0,
            T4_MAX,
            1.4,
        )
        .unwrap();
        if let Ok(low) = low {
            assert!(mid.specific_thrust_n_s_kg > low.specific_thrust_n_s_kg);
        }
    }

    /// THE FUNDAMENTAL LIMIT OF A RAMJET, and it emerged rather than being
    /// asserted.
    ///
    /// [CORRECTED] This started as a test that specific thrust rises
    /// monotonically with flight Mach. It does not, and the model said so at
    /// M 2.5. The physics: specific thrust is `(1+f) V9 - V0`. The exit velocity
    /// V9 is capped by the burner temperature limit, so once ram compression has
    /// eaten the temperature budget it stops growing — while V0 keeps rising
    /// linearly with Mach. The difference therefore peaks and then collapses.
    ///
    /// That peak is why a ramjet has a design Mach number at all, and why
    /// pushing one past it buys nothing. The assertion was a guess; the model
    /// disagreed and the model was right.
    #[test]
    fn specific_thrust_peaks_and_then_collapses() {
        let a = ventus_atmos::at_geopotential(26_000.0).unwrap();
        let mut best = (0.0_f64, f64::NEG_INFINITY);
        let mut samples = 0;

        let mut m = 1.5;
        while m <= 7.0 {
            if let Ok(c) = ideal_ramjet(
                m,
                a.temperature_k,
                a.pressure_pa,
                m * a.speed_of_sound_m_s,
                0.7416,
                T4_MAX,
                1.4,
            ) {
                samples += 1;
                if c.specific_thrust_n_s_kg > best.1 {
                    best = (m, c.specific_thrust_n_s_kg);
                }
            }
            m += 0.05;
        }

        assert!(samples > 20, "only {samples} feasible points");
        assert!(
            (2.0..5.0).contains(&best.0),
            "specific thrust peaks at M {:.2}, which is not a plausible ramjet              design Mach",
            best.0
        );

        // Thrust must genuinely fall away past the peak, not merely flatten.
        let past = ideal_ramjet(
            best.0 + 1.5,
            a.temperature_k,
            a.pressure_pa,
            (best.0 + 1.5) * a.speed_of_sound_m_s,
            0.7416,
            T4_MAX,
            1.4,
        )
        .unwrap();
        assert!(
            past.specific_thrust_n_s_kg < 0.85 * best.1,
            "1.5 Mach past the peak, specific thrust is still {:.0} of {:.0}",
            past.specific_thrust_n_s_kg,
            best.1
        );
    }

    /// The specific-work collapse that pushes a Mach 3.5 design away from a
    /// turbomachine. This is the number ADR-000 and design-point.md both quote.
    #[test]
    fn the_temperature_headroom_collapses_with_flight_mach() {
        let cycle = design_point(0.7416);

        // [CORRECTED] This asserted 2.26, taken from docs/design-point.md, which
        // forms the ratio against the THERMALLY PERFECT T0 of 752.8 K. The cycle
        // compresses with gamma = 1.4, giving T02 = 768.1 K and a ratio of 2.213.
        // Both are right about different things and they differ by 2 %, which is
        // the same ADR-000 D10 gap that appears everywhere else. The model is
        // asserted against what the model computes; the design point now carries
        // both numbers.
        assert!(
            rel_err(cycle.temperature_headroom_ratio, 2.213) < 5e-3,
            "T4max/T02 = {} with calorically perfect ram compression, expected 2.213",
            cycle.temperature_headroom_ratio
        );
        assert!(
            rel_err(T4_MAX / 752.8, 2.258) < 5e-3,
            "the thermally perfect ratio should be 2.258"
        );

        // Sea level static, where a turbojet has room to work.
        let sl = ventus_atmos::at_geopotential(0.0).unwrap();
        let ratio_static = T4_MAX / sl.temperature_k;
        assert!(rel_err(ratio_static, 5.90) < 2e-2, "{ratio_static}");
        assert!(ratio_static / cycle.temperature_headroom_ratio > 2.5);
    }

    /// ADR-000 D10 in force: the burner runs at gamma ~ 1.30, not 1.4, and using
    /// 1.4 would be a failure rather than a known limit.
    #[test]
    fn the_burner_uses_hot_gamma_not_cold() {
        let cycle = design_point(0.7416);
        assert!(
            (1.29..1.32).contains(&cycle.burner_gamma),
            "burner gamma is {}, which is not a hot-gas value",
            cycle.burner_gamma
        );
    }

    /// THE INLET IS MOST OF THE ENGINE, as a number.
    ///
    /// Specific thrust must rise with inlet recovery, and the rise across the
    /// range M3 spans — a bare normal shock at 0.213 against a real inlet at
    /// 0.742 — has to be large enough to justify the geometry M3 says it costs.
    #[test]
    fn inlet_recovery_buys_thrust() {
        let normal_shock_only = design_point(0.21295);
        let real_inlet = design_point(0.7416);
        let perfect = design_point(1.0);

        assert!(real_inlet.specific_thrust_n_s_kg > normal_shock_only.specific_thrust_n_s_kg);
        assert!(perfect.specific_thrust_n_s_kg > real_inlet.specific_thrust_n_s_kg);

        let gain = real_inlet.specific_thrust_n_s_kg / normal_shock_only.specific_thrust_n_s_kg;
        assert!(
            gain > 1.3,
            "going from a normal shock to a real inlet gained only {gain:.2}x in \
             specific thrust; M3 claims the recovery ratio is 3.48x"
        );

        // Monotone across the whole range, with no accidental turning point.
        let mut previous = f64::NEG_INFINITY;
        for i in 1..=20 {
            let c = design_point(f64::from(i) / 20.0);
            assert!(c.specific_thrust_n_s_kg > previous);
            previous = c.specific_thrust_n_s_kg;
        }
    }

    /// Specific impulse in the band published for hydrocarbon ramjets around
    /// M 3. **[TO CITE]** — this is the weakest anchor in the module, carried at
    /// the width of the published range rather than pretending to a figure.
    #[test]
    fn specific_impulse_lands_in_the_published_ramjet_band() {
        let cycle = design_point(0.7416);
        let isp = cycle.specific_impulse_s;
        assert!(
            (900.0..2000.0).contains(&isp),
            "Isp = {isp:.0} s, outside the 900-2000 s band published for \
             hydrocarbon ramjets near M 3"
        );
        // Fuel-air ratio near stoichiometric-ish for a hot burner.
        assert!(
            (0.01..0.06).contains(&cycle.fuel_air_ratio),
            "f = {}",
            cycle.fuel_air_ratio
        );
    }

    /// The cycle has to refuse where it stops being valid rather than returning
    /// a number that looks like thrust.
    #[test]
    fn the_cycle_refuses_where_ram_temperature_passes_the_burner_limit() {
        let a = ventus_atmos::at_geopotential(26_000.0).unwrap();
        // At M 8 the ram total temperature is already above 1700 K.
        let too_fast = ideal_ramjet(
            8.0,
            a.temperature_k,
            a.pressure_pa,
            8.0 * a.speed_of_sound_m_s,
            0.5,
            T4_MAX,
            1.4,
        );
        assert_eq!(too_fast, Err(CycleError::RamTemperatureExceedsBurnerLimit));
    }
}
