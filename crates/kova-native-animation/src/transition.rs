//! Transitions: how a value moves from its old to its new target.

use crate::{Easing, Spring};
use std::time::Duration;

/// The timing model of a transition.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Timing {
    /// Fixed duration with an easing curve.
    Tween { duration: Duration, easing: Easing },
    /// Physically based spring; duration emerges from the parameters.
    Spring(Spring),
}

/// Describes how animatable properties transition between values.
///
/// `Duration` converts into a transition with Kova Native's default easing, so
/// `.transition(180.ms())` just works.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transition {
    pub timing: Timing,
    pub delay: Duration,
}

impl Default for Transition {
    fn default() -> Self {
        Transition::new(Duration::from_millis(150))
    }
}

impl Transition {
    pub fn new(duration: Duration) -> Self {
        Transition {
            timing: Timing::Tween {
                duration,
                easing: Easing::default(),
            },
            delay: Duration::ZERO,
        }
    }

    /// No animation: changes apply immediately.
    pub fn instant() -> Self {
        Transition::new(Duration::ZERO)
    }

    pub fn spring(spring: Spring) -> Self {
        Transition {
            timing: Timing::Spring(spring),
            delay: Duration::ZERO,
        }
    }

    pub fn easing(mut self, easing: Easing) -> Self {
        if let Timing::Tween { easing: e, .. } = &mut self.timing {
            *e = easing;
        }
        self
    }

    pub fn delay(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }

    pub fn is_instant(&self) -> bool {
        matches!(self.timing, Timing::Tween { duration, .. } if duration.is_zero())
            && self.delay.is_zero()
    }

    /// Eased progress at `elapsed` since the transition started, with
    /// normalized initial velocity `v0` (only meaningful for springs).
    pub fn progress(&self, elapsed: Duration, v0: f32) -> f32 {
        let Some(t) = elapsed.checked_sub(self.delay) else {
            return 0.0;
        };
        match self.timing {
            Timing::Tween { duration, easing } => {
                if duration.is_zero() {
                    1.0
                } else {
                    easing.apply(t.as_secs_f32() / duration.as_secs_f32())
                }
            }
            Timing::Spring(spring) => spring.progress(t, v0),
        }
    }

    /// Normalized velocity at `elapsed`.
    pub fn velocity(&self, elapsed: Duration, v0: f32) -> f32 {
        let Some(t) = elapsed.checked_sub(self.delay) else {
            return 0.0;
        };
        match self.timing {
            Timing::Tween { duration, easing } => {
                if duration.is_zero() {
                    return 0.0;
                }
                let d = duration.as_secs_f32();
                let x = t.as_secs_f32() / d;
                if x >= 1.0 {
                    return 0.0;
                }
                let h = 1e-3;
                (easing.apply(x + h) - easing.apply((x - h).max(0.0)))
                    / ((x + h) - (x - h).max(0.0))
                    / d
            }
            Timing::Spring(spring) => spring.velocity(t, v0),
        }
    }

    pub fn is_finished(&self, elapsed: Duration, v0: f32) -> bool {
        let Some(t) = elapsed.checked_sub(self.delay) else {
            return false;
        };
        match self.timing {
            Timing::Tween { duration, .. } => t >= duration,
            Timing::Spring(spring) => spring.is_settled(t, v0),
        }
    }
}

impl From<Duration> for Transition {
    fn from(d: Duration) -> Self {
        Transition::new(d)
    }
}

impl From<Spring> for Transition {
    fn from(s: Spring) -> Self {
        Transition::spring(s)
    }
}
