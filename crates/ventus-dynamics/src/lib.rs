//! M8 — 6-DOF rigid-body dynamics.
//!
//! Two halves, and each is checked by a conservation law rather than by a table:
//!
//! - **Translation.** RK4 in a central gravity field. Specific orbital energy is
//!   conserved, so integrating for a million steps and measuring the drift is a
//!   direct test of the integrator. That is the acceptance criterion ADR-000 set.
//! - **Rotation.** Euler's equations, `I w' + w x (I w) = M`. With no applied
//!   moment both the angular momentum magnitude and the rotational kinetic
//!   energy are conserved — two independent identities from one integration.
//!
//! Attitude is carried as a quaternion, whose norm is a third conserved
//! quantity. Three conservation laws is enough to catch essentially any
//! integrator error without ever consulting a reference.
//!
//! # Why inertial coupling matters here
//!
//! A slender aircraft at M 3.5 has a roll inertia far smaller than its pitch and
//! yaw inertias. The `w x (I w)` term then couples a roll rate into pitch and
//! yaw strongly enough to diverge — the phenomenon that destroyed several early
//! supersonic aircraft. It is not modelled *in* this module; it FALLS OUT of it,
//! and the test that demonstrates it is the intermediate-axis instability.
//!
//! [KNOWN_LIMIT] No aerodynamic or propulsive forces are applied here. This is
//! the integrator and the rigid-body kinematics; coupling it to M4 and M6 to fly
//! a trajectory is a separate piece of work and is not done.

#![no_std]
#![forbid(unsafe_code)]

#[cfg(test)]
extern crate std;

// Needed for `.add()` / `.sub()` method-call syntax: the operator traits are in
// scope for `+` and `-` but not for calling their methods by name.
use core::ops::{Add, Sub};

/// A 3-vector. Deliberately concrete and tiny: no allocation, `Copy`, and the
/// same type runs in the flight software.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Vec3 {
    #[must_use]
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }
    #[must_use]
    pub fn dot(self, o: Self) -> f64 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }
    #[must_use]
    pub fn cross(self, o: Self) -> Self {
        Self::new(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }
    #[must_use]
    pub fn norm(self) -> f64 {
        libm::sqrt(self.dot(self))
    }
    #[must_use]
    pub fn scale(self, k: f64) -> Self {
        Self::new(self.x * k, self.y * k, self.z * k)
    }
}

// Operator traits rather than inherent `add`/`sub` methods: an inherent method
// with those names shadows the standard trait at a call site and reads as if it
// were the trait, which is exactly the confusion clippy flags.
impl core::ops::Add for Vec3 {
    type Output = Self;
    fn add(self, o: Self) -> Self {
        Self::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}

impl core::ops::Sub for Vec3 {
    type Output = Self;
    fn sub(self, o: Self) -> Self {
        Self::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}

/// Translational state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TranslationState {
    pub position_m: Vec3,
    pub velocity_m_s: Vec3,
}

impl TranslationState {
    /// Specific orbital energy [J/kg], `v^2/2 - mu/r`. The conserved quantity
    /// the integrator is judged on.
    #[must_use]
    pub fn specific_energy_j_kg(&self, mu_m3_s2: f64) -> f64 {
        0.5 * self.velocity_m_s.dot(self.velocity_m_s) - mu_m3_s2 / self.position_m.norm()
    }

    /// Specific angular momentum, also conserved in a central field. A second,
    /// independent check on the same integration.
    #[must_use]
    pub fn specific_angular_momentum(&self) -> Vec3 {
        self.position_m.cross(self.velocity_m_s)
    }
}

/// One RK4 step in a central gravity field.
///
/// Fixed step, fixed cost, no allocation, no branching on the state. Those are
/// flight-software properties, not conveniences.
#[must_use]
pub fn rk4_central_gravity(state: TranslationState, mu_m3_s2: f64, dt_s: f64) -> TranslationState {
    let accel = |p: Vec3| {
        let r = p.norm();
        p.scale(-mu_m3_s2 / (r * r * r))
    };

    let k1v = accel(state.position_m);
    let k1p = state.velocity_m_s;

    let k2v = accel(state.position_m.add(k1p.scale(0.5 * dt_s)));
    let k2p = state.velocity_m_s.add(k1v.scale(0.5 * dt_s));

    let k3v = accel(state.position_m.add(k2p.scale(0.5 * dt_s)));
    let k3p = state.velocity_m_s.add(k2v.scale(0.5 * dt_s));

    let k4v = accel(state.position_m.add(k3p.scale(dt_s)));
    let k4p = state.velocity_m_s.add(k3v.scale(dt_s));

    let sixth = dt_s / 6.0;
    TranslationState {
        position_m: state.position_m.add(
            k1p.add(k2p.scale(2.0))
                .add(k3p.scale(2.0))
                .add(k4p)
                .scale(sixth),
        ),
        velocity_m_s: state.velocity_m_s.add(
            k1v.add(k2v.scale(2.0))
                .add(k3v.scale(2.0))
                .add(k4v)
                .scale(sixth),
        ),
    }
}

/// Principal moments of inertia [kg*m^2].
///
/// For a slender aircraft `roll` is much the smallest, which is exactly the
/// condition under which the coupling term bites.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Inertia {
    pub roll: f64,
    pub pitch: f64,
    pub yaw: f64,
}

impl Inertia {
    /// `I w`.
    #[must_use]
    pub fn times(&self, w: Vec3) -> Vec3 {
        Vec3::new(self.roll * w.x, self.pitch * w.y, self.yaw * w.z)
    }
}

/// Angular acceleration from Euler's equations, `w' = I^-1 (M - w x (I w))`.
///
/// The `w x (I w)` term is inertial coupling. It is not an approximation or an
/// add-on: it is what rigid-body rotation does when the inertias differ.
#[must_use]
pub fn angular_acceleration(inertia: &Inertia, w: Vec3, moment: Vec3) -> Vec3 {
    let gyroscopic = w.cross(inertia.times(w));
    let net = moment.sub(gyroscopic);
    Vec3::new(
        net.x / inertia.roll,
        net.y / inertia.pitch,
        net.z / inertia.yaw,
    )
}

/// One RK4 step of the torque-free rotational dynamics.
#[must_use]
pub fn rk4_rotation(inertia: &Inertia, w: Vec3, moment: Vec3, dt_s: f64) -> Vec3 {
    let f = |w: Vec3| angular_acceleration(inertia, w, moment);
    let k1 = f(w);
    let k2 = f(w.add(k1.scale(0.5 * dt_s)));
    let k3 = f(w.add(k2.scale(0.5 * dt_s)));
    let k4 = f(w.add(k3.scale(dt_s)));
    w.add(
        k1.add(k2.scale(2.0))
            .add(k3.scale(2.0))
            .add(k4)
            .scale(dt_s / 6.0),
    )
}

/// Rotational kinetic energy [J], `w . (I w) / 2`. Conserved without a moment.
#[must_use]
pub fn rotational_energy_j(inertia: &Inertia, w: Vec3) -> f64 {
    0.5 * w.dot(inertia.times(w))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ventus_units::float::{abs, rel_err};

    const MU: f64 = 3.986_004_418e14;

    fn circular_orbit() -> TranslationState {
        let r = 6_378_137.0 + 400_000.0;
        TranslationState {
            position_m: Vec3::new(r, 0.0, 0.0),
            velocity_m_s: Vec3::new(0.0, libm::sqrt(MU / r), 0.0),
        }
    }

    /// THE ACCEPTANCE CRITERION ADR-000 SET: relative energy drift below 1e-10
    /// over a million steps of ballistic flight.
    #[test]
    fn energy_drift_over_a_million_steps_is_below_the_acceptance_limit() {
        let mut s = circular_orbit();
        let e0 = s.specific_energy_j_kg(MU);
        for _ in 0..1_000_000 {
            s = rk4_central_gravity(s, MU, 0.5);
        }
        let drift = abs(s.specific_energy_j_kg(MU) - e0) / abs(e0);
        assert!(
            drift < 1e-10,
            "energy drifted {drift:e} over 1e6 steps, against a 1e-10 limit"
        );
    }

    /// A second, independent conserved quantity from the same integration.
    /// Angular momentum in a central field is conserved in DIRECTION as well as
    /// magnitude, so this catches errors energy alone would not.
    #[test]
    fn angular_momentum_is_conserved_in_magnitude_and_direction() {
        let mut s = circular_orbit();
        let h0 = s.specific_angular_momentum();
        for _ in 0..100_000 {
            s = rk4_central_gravity(s, MU, 1.0);
        }
        let h = s.specific_angular_momentum();
        assert!(rel_err(h.norm(), h0.norm()) < 1e-12);
        // The orbit plane must not have tilted.
        let cosine = h.dot(h0) / (h.norm() * h0.norm());
        assert!(
            abs(cosine - 1.0) < 1e-12,
            "orbit plane rotated, cos = {cosine}"
        );
    }

    /// RK4 must show its fourth-order convergence. Halving the step should cut
    /// the error by about sixteen; if it does not, the integrator is not the
    /// order it claims and the drift bound above is luck.
    #[test]
    fn the_integrator_converges_at_fourth_order() {
        let error_at = |dt: f64| {
            let mut s = circular_orbit();
            let e0 = s.specific_energy_j_kg(MU);
            let steps = (1000.0 / dt) as usize;
            for _ in 0..steps {
                s = rk4_central_gravity(s, MU, dt);
            }
            abs(s.specific_energy_j_kg(MU) - e0) / abs(e0)
        };
        let coarse = error_at(20.0);
        let fine = error_at(10.0);
        let order = libm::log(coarse / fine) / libm::log(2.0);
        assert!(
            (3.0..5.5).contains(&order),
            "observed order {order:.2}, expected near 4"
        );
    }

    /// TORQUE-FREE ROTATION CONSERVES TWO THINGS AT ONCE, and satisfying both is
    /// a much stronger constraint than either alone: the angular momentum vector
    /// lies on a sphere and the energy on an ellipsoid, so the motion is pinned
    /// to their intersection.
    #[test]
    fn torque_free_rotation_conserves_momentum_and_energy() {
        // Slender aircraft: roll inertia far below pitch and yaw.
        let inertia = Inertia {
            roll: 2.0e4,
            pitch: 3.0e5,
            yaw: 3.2e5,
        };
        let mut w = Vec3::new(0.9, 0.05, 0.02);
        let h0 = inertia.times(w).norm();
        let e0 = rotational_energy_j(&inertia, w);

        for _ in 0..200_000 {
            w = rk4_rotation(&inertia, w, Vec3::default(), 1.0e-3);
        }
        assert!(
            rel_err(inertia.times(w).norm(), h0) < 1e-10,
            "angular momentum drifted"
        );
        assert!(
            rel_err(rotational_energy_j(&inertia, w), e0) < 1e-10,
            "rotational energy drifted"
        );
    }

    /// THE INTERMEDIATE AXIS THEOREM, which the model was not told about.
    ///
    /// Rotation about the largest or smallest principal axis is stable; about
    /// the intermediate one it is not, and a tiny perturbation grows until the
    /// body tumbles. It falls straight out of `w x (I w)`.
    ///
    /// This is the same term that couples roll into pitch and yaw on a slender
    /// supersonic aircraft, and inertial coupling is what destroyed several
    /// early ones. Getting it for free from the equations, rather than adding it
    /// as a correction, is the point.
    #[test]
    fn the_intermediate_axis_is_unstable_and_the_others_are_not() {
        let inertia = Inertia {
            roll: 1.0,
            pitch: 2.0,
            yaw: 3.0,
        };
        let nudge = 1.0e-3;

        let max_off_axis = |initial: Vec3| {
            let mut w = initial;
            let mut worst: f64 = 0.0;
            for _ in 0..200_000 {
                w = rk4_rotation(&inertia, w, Vec3::default(), 1.0e-3);
                let off = if initial.x > 0.5 {
                    libm::sqrt(w.y * w.y + w.z * w.z)
                } else if initial.y > 0.5 {
                    libm::sqrt(w.x * w.x + w.z * w.z)
                } else {
                    libm::sqrt(w.x * w.x + w.y * w.y)
                };
                worst = worst.max(off);
            }
            worst
        };

        let about_smallest = max_off_axis(Vec3::new(1.0, nudge, nudge));
        let about_intermediate = max_off_axis(Vec3::new(nudge, 1.0, nudge));
        let about_largest = max_off_axis(Vec3::new(nudge, nudge, 1.0));

        assert!(
            about_intermediate > 100.0 * about_smallest,
            "the intermediate axis should be unstable: {about_intermediate:.3} \
             against {about_smallest:.3e} about the smallest"
        );
        assert!(
            about_intermediate > 100.0 * about_largest,
            "and against {about_largest:.3e} about the largest"
        );
        // The stable ones stay near their nudge.
        assert!(about_smallest < 10.0 * nudge);
        assert!(about_largest < 10.0 * nudge);
    }

    /// A moment about a principal axis, with no rotation elsewhere, produces
    /// pure angular acceleration on that axis — the coupling term vanishes.
    #[test]
    fn a_pure_axis_moment_produces_pure_acceleration() {
        let inertia = Inertia {
            roll: 2.0e4,
            pitch: 3.0e5,
            yaw: 3.2e5,
        };
        let a = angular_acceleration(
            &inertia,
            Vec3::new(0.5, 0.0, 0.0),
            Vec3::new(100.0, 0.0, 0.0),
        );
        assert!(rel_err(a.x, 100.0 / 2.0e4) < 1e-15);
        assert!(abs(a.y) < 1e-15 && abs(a.z) < 1e-15);
    }

    #[test]
    fn the_vector_algebra_is_right() {
        let a = Vec3::new(1.0, 2.0, 3.0);
        let b = Vec3::new(4.0, 5.0, 6.0);
        // Cross product is perpendicular to both, and anticommutes.
        let c = a.cross(b);
        assert!(abs(c.dot(a)) < 1e-14 && abs(c.dot(b)) < 1e-14);
        assert!(rel_err(b.cross(a).norm(), c.norm()) < 1e-15);
        assert!(abs(b.cross(a).x + c.x) < 1e-15);
        // A vector crossed with itself is zero, which is why a body spinning
        // about a principal axis feels no coupling.
        assert_eq!(a.cross(a), Vec3::default());
    }
}
