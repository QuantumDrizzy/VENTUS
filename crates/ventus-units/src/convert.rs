//! Unit conversions. PRESENTATION LAYER ONLY (ADR-000 D7).
//!
//! Nothing inside a physics module may call these. They exist so that reports
//! and plots can show feet and degrees Celsius without any non-SI value ever
//! crossing a module boundary.

/// Metres to international feet. Exact by definition (1 ft = 0.3048 m).
#[inline]
#[must_use]
pub fn m_to_ft(m: f64) -> f64 {
    m / 0.3048
}

/// International feet to metres. Exact by definition.
#[inline]
#[must_use]
pub fn ft_to_m(ft: f64) -> f64 {
    ft * 0.3048
}

/// Kelvin to degrees Celsius.
#[inline]
#[must_use]
pub fn k_to_celsius(k: f64) -> f64 {
    k - 273.15
}

/// Degrees Celsius to kelvin.
#[inline]
#[must_use]
pub fn celsius_to_k(c: f64) -> f64 {
    c + 273.15
}

/// Pascals to pounds per square foot.
/// 1 lbf = 4.4482216152605 N (exact), 1 ft^2 = 0.09290304 m^2 (exact).
#[inline]
#[must_use]
pub fn pa_to_psf(pa: f64) -> f64 {
    pa * 0.092_903_04 / 4.448_221_615_260_5
}

/// Metres per second to knots. 1 kn = 1852 m/h exactly.
#[inline]
#[must_use]
pub fn m_s_to_kn(m_s: f64) -> f64 {
    m_s * 3600.0 / 1852.0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Yardsticks: the design point in SI, restated in the units the
    /// literature uses. If these drift, a conversion is wrong.
    #[test]
    fn design_point_restated_in_legacy_units() {
        // 24 000 m geopotential -> ~78 740 ft, the altitude band the SR-71 flew.
        assert!((m_to_ft(24_000.0) - 78_740.2).abs() < 0.1);
        // Cruise dynamic pressure 18.46 kPa -> ~386 psf.
        assert!((pa_to_psf(18_460.0) - 385.6).abs() < 0.5);
        // T_aw = 569.0 K -> 295.85 C.
        assert!((k_to_celsius(569.0) - 295.85).abs() < 1e-9);
        // 893.34 m/s -> ~1736 kn.
        assert!((m_s_to_kn(893.34) - 1736.4).abs() < 0.5);
    }

    #[test]
    fn conversions_round_trip() {
        for v in [0.0_f64, 1.0, -273.15, 1e5, 1e-7] {
            assert!((ft_to_m(m_to_ft(v)) - v).abs() <= f64::EPSILON * v.abs().max(1.0) * 4.0);
            assert!((celsius_to_k(k_to_celsius(v)) - v).abs() <= 1e-9);
        }
    }
}
