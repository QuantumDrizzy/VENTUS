//! The U.S. Standard Atmosphere 1976 defining table.
//!
//! Source: NASA-TM-X-74335 (U.S. Standard Atmosphere, 1976), Table 4 —
//! defined geopotential layer bases, molecular-scale temperature and gradient.
//!
//! US76 is *defined* by these eight numbers plus the barometric equations; the
//! printed tables are tabulated output of that definition, not an independent
//! source. The base pressures below are therefore not an extra input: they are
//! computed by forward integration from (0 m, 101 325 Pa) and are reproduced
//! here as literals only so the hot path does not repeat seven `pow` calls per
//! query. `base_pressures_are_consistent_with_forward_integration` in the test
//! module proves the literals are what the integration produces, and
//! `cases/us76_layers.toml` checks them against the published table.

/// One US76 layer, keyed by geopotential altitude.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Layer {
    /// Geopotential altitude of the layer base [m].
    pub base_geopotential_altitude_m: f64,
    /// Molecular-scale temperature at the layer base [K].
    pub base_temperature_k: f64,
    /// Gradient of molecular-scale temperature with geopotential altitude [K/m].
    /// US76 tabulates this in K/km; it is stored in SI (ADR-000 D7).
    pub lapse_rate_k_per_m: f64,
    /// Pressure at the layer base [Pa]. See the module note: derived, not input.
    pub base_pressure_pa: f64,
}

/// The seven layers of US76 below the model top.
pub const LAYERS: [Layer; 7] = [
    Layer {
        base_geopotential_altitude_m: 0.0,
        base_temperature_k: 288.15,
        lapse_rate_k_per_m: -6.5e-3,
        base_pressure_pa: 101_325.0,
    },
    Layer {
        base_geopotential_altitude_m: 11_000.0,
        base_temperature_k: 216.65,
        lapse_rate_k_per_m: 0.0,
        base_pressure_pa: 22632.063973462926,
    },
    Layer {
        base_geopotential_altitude_m: 20_000.0,
        base_temperature_k: 216.65,
        lapse_rate_k_per_m: 1.0e-3,
        base_pressure_pa: 5474.888669677779,
    },
    Layer {
        base_geopotential_altitude_m: 32_000.0,
        base_temperature_k: 228.65,
        lapse_rate_k_per_m: 2.8e-3,
        base_pressure_pa: 868.018684755229,
    },
    Layer {
        base_geopotential_altitude_m: 47_000.0,
        base_temperature_k: 270.65,
        lapse_rate_k_per_m: 0.0,
        base_pressure_pa: 110.90630555496627,
    },
    Layer {
        base_geopotential_altitude_m: 51_000.0,
        base_temperature_k: 270.65,
        lapse_rate_k_per_m: -2.8e-3,
        base_pressure_pa: 66.9388731186875,
    },
    Layer {
        base_geopotential_altitude_m: 71_000.0,
        base_temperature_k: 214.65,
        lapse_rate_k_per_m: -2.0e-3,
        base_pressure_pa: 3.9564204280407402,
    },
];

/// Geopotential altitude of the model top [m]. US76 continues above this with a
/// different formulation (molecular diffusion, varying mean molar mass); this
/// implementation stops here rather than extrapolating a model that no longer
/// applies.
pub const TOP_GEOPOTENTIAL_ALTITUDE_M: f64 = 84_852.0;

/// Index of the layer containing `h`.
///
/// A boundary altitude belongs to the layer it *starts*, i.e. the comparison is
/// `h >= base`. This is the `<` versus `<=` that the discontinuity cases in
/// `cases/us76_layers.toml` exist to pin down: at exactly 20 000 m the
/// temperature must be continuous (both layers give 216.65 K) while the lapse
/// rate must already be the upper layer's +1.0 K/km, not the lower layer's zero.
#[must_use]
pub fn layer_index(geopotential_altitude_m: f64) -> usize {
    let mut i = 0;
    while i + 1 < LAYERS.len()
        && geopotential_altitude_m >= LAYERS[i + 1].base_geopotential_altitude_m
    {
        i += 1;
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;
    use ventus_units::constants::G0_M_S2;
    use ventus_units::float::abs;

    /// The literals above must be exactly what forward integration from sea
    /// level produces. If someone edits one by hand, this fails.
    #[test]
    fn base_pressures_are_consistent_with_forward_integration() {
        let g0m_over_r = G0_M_S2 * crate::M_AIR_KG_PER_MOL / crate::R_UNIVERSAL_J_PER_MOL_K;
        let mut p = LAYERS[0].base_pressure_pa;

        for i in 0..LAYERS.len() - 1 {
            let l = LAYERS[i];
            let dz = LAYERS[i + 1].base_geopotential_altitude_m - l.base_geopotential_altitude_m;
            p = if l.lapse_rate_k_per_m == 0.0 {
                p * libm::exp(-g0m_over_r * dz / l.base_temperature_k)
            } else {
                let t_top = l.base_temperature_k + l.lapse_rate_k_per_m * dz;
                p * libm::pow(
                    l.base_temperature_k / t_top,
                    g0m_over_r / l.lapse_rate_k_per_m,
                )
            };

            let expected = LAYERS[i + 1].base_pressure_pa;
            let rel = abs(p - expected) / expected;
            assert!(
                rel < 1e-15,
                "layer {} base pressure: integrated {p}, literal {expected}, rel {rel:e}",
                i + 1
            );
        }
    }

    /// The temperature at the top of each layer must equal the base temperature
    /// of the next. US76 is continuous in temperature by construction; a typo in
    /// the table would break that and nothing else would notice.
    #[test]
    fn temperature_is_continuous_across_every_boundary() {
        for i in 0..LAYERS.len() - 1 {
            let l = LAYERS[i];
            let dz = LAYERS[i + 1].base_geopotential_altitude_m - l.base_geopotential_altitude_m;
            let t_top = l.base_temperature_k + l.lapse_rate_k_per_m * dz;
            let t_next = LAYERS[i + 1].base_temperature_k;
            assert!(
                abs(t_top - t_next) < 1e-9,
                "boundary {}: {t_top} K from below, {t_next} K from above",
                i + 1
            );
        }
    }

    #[test]
    fn layer_selection_puts_a_boundary_in_the_layer_it_starts() {
        assert_eq!(layer_index(0.0), 0);
        assert_eq!(layer_index(10_999.999), 0);
        assert_eq!(
            layer_index(11_000.0),
            1,
            "a boundary belongs to the upper layer"
        );
        assert_eq!(layer_index(19_999.0), 1);
        assert_eq!(layer_index(20_000.0), 2);
        assert_eq!(layer_index(20_001.0), 2);
        assert_eq!(layer_index(71_000.0), 6);
        assert_eq!(layer_index(84_852.0), 6);
    }

    /// Layer bases must be strictly increasing, or `layer_index` silently
    /// returns the wrong layer for part of the range.
    #[test]
    fn layer_bases_are_strictly_increasing() {
        for i in 0..LAYERS.len() - 1 {
            assert!(
                LAYERS[i + 1].base_geopotential_altitude_m > LAYERS[i].base_geopotential_altitude_m
            );
        }
        assert!(TOP_GEOPOTENTIAL_ALTITUDE_M > LAYERS[6].base_geopotential_altitude_m);
    }
}
