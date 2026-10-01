//! Physically based springs, solved analytically.
//!
//! Springs are evaluated in *normalized* space: progress moves from `0` to
//! `1`, possibly overshooting. Because the damped harmonic oscillator has a
//! closed-form solution the value at any time is exact and frame-rate
//! independent — there is no integration error at 60 vs 144 Hz.

use std::time::Duration;

/// Spring parameters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spring {
    pub stiffness: f32,
    pub damping: f32,
    pub mass: f32,
}

impl Default for Spring {
    fn default() -> Self {
        Spring::smooth()
    }
}

impl Spring {
    pub const fn new(stiffness: f32, damping: f32, mass: f32) -> Self {
        Spring {
            stiffness,
            damping,
            mass,
        }
    }

    /// Critically damped, no overshoot.
    pub fn smooth() -> Self {
        Spring::with_damping_ratio(300.0, 1.0)
    }

    /// Fast with a hint of overshoot. Great for hover/press feedback.
    pub fn snappy() -> Self {
        Spring::with_damping_ratio(520.0, 0.82)
    }

    /// Visibly bouncy.
    pub fn bouncy() -> Self {
        Spring::with_damping_ratio(260.0, 0.45)
    }

    /// Slow and soft.
    pub fn gentle() -> Self {
        Spring::with_damping_ratio(120.0, 1.0)
    }

    /// Builds a spring (mass 1) from stiffness and damping ratio ζ.
    pub fn with_damping_ratio(stiffness: f32, ratio: f32) -> Self {
        let damping = 2.0 * ratio * stiffness.sqrt();
        Spring {
            stiffness,
            damping,
            mass: 1.0,
        }
    }

    fn omega0(&self) -> f32 {
        (self.stiffness / self.mass).sqrt()
    }

    fn zeta(&self) -> f32 {
        self.damping / (2.0 * (self.stiffness * self.mass).sqrt())
    }

    /// Displacement from the target at time `t` (seconds), given initial
    /// displacement `d0` and velocity `v0`.
    fn displacement(&self, d0: f32, v0: f32, t: f32) -> f32 {
        let w0 = self.omega0();
        let z = self.zeta();
        if z < 1.0 - 1e-4 {
            let wd = w0 * (1.0 - z * z).sqrt();
            let a = d0;
            let b = (v0 + z * w0 * d0) / wd;
            (-z * w0 * t).exp() * (a * (wd * t).cos() + b * (wd * t).sin())
        } else if z > 1.0 + 1e-4 {
            let s = (z * z - 1.0).sqrt();
            let r1 = -w0 * (z - s);
            let r2 = -w0 * (z + s);
            let c2 = (v0 - r1 * d0) / (r2 - r1);
            let c1 = d0 - c2;
            c1 * (r1 * t).exp() + c2 * (r2 * t).exp()
        } else {
            (-w0 * t).exp() * (d0 + (v0 + w0 * d0) * t)
        }
    }

    /// Normalized progress (0 → 1) after `elapsed`, starting at rest at 0 with
    /// normalized initial velocity `v0` (units of "distance per second").
    pub fn progress(&self, elapsed: Duration, v0: f32) -> f32 {
        1.0 + self.displacement(-1.0, v0, elapsed.as_secs_f32())
    }

    /// Normalized velocity at `elapsed`.
    pub fn velocity(&self, elapsed: Duration, v0: f32) -> f32 {
        let t = elapsed.as_secs_f32();
        let h = 1e-3;
        let a = self.displacement(-1.0, v0, (t - h).max(0.0));
        let b = self.displacement(-1.0, v0, t + h);
        (b - a) / (t + h - (t - h).max(0.0))
    }

    /// Whether the spring has come to rest (within a small tolerance).
    pub fn is_settled(&self, elapsed: Duration, v0: f32) -> bool {
        let p = self.progress(elapsed, v0);
        (1.0 - p).abs() < 1e-3 && self.velocity(elapsed, v0).abs() < 1e-2
    }

    /// An upper bound on how long the spring takes to settle.
    pub fn settle_duration(&self, v0: f32) -> Duration {
        let mut t = Duration::from_millis(16);
        while t < Duration::from_secs(10) {
            if self.is_settled(t, v0) {
                return t;
            }
            t += Duration::from_millis(16);
        }
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settles_at_target() {
        for spring in [
            Spring::smooth(),
            Spring::snappy(),
            Spring::bouncy(),
            Spring::gentle(),
        ] {
            let d = spring.settle_duration(0.0);
            assert!(d < Duration::from_secs(5), "{spring:?} took {d:?}");
            assert!((spring.progress(d, 0.0) - 1.0).abs() < 1e-2);
        }
    }

    #[test]
    fn starts_at_zero() {
        let s = Spring::snappy();
        assert!(s.progress(Duration::ZERO, 0.0).abs() < 1e-5);
    }

    #[test]
    fn underdamped_overshoots_critically_damped_does_not() {
        let bouncy = Spring::bouncy();
        let smooth = Spring::smooth();
        let mut max_b: f32 = 0.0;
        let mut max_s: f32 = 0.0;
        for ms in 0..2000 {
            let t = Duration::from_millis(ms);
            max_b = max_b.max(bouncy.progress(t, 0.0));
            max_s = max_s.max(smooth.progress(t, 0.0));
        }
        assert!(max_b > 1.05);
        assert!(max_s <= 1.0 + 1e-3);
    }
}
