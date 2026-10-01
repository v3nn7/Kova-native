//! Values that animate towards a target.

use crate::{Lerp, Transition};
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
struct Active<T> {
    from: T,
    start: Instant,
    transition: Transition,
    /// Normalized initial velocity carried over from an interrupted animation.
    v0: f32,
}

/// A value that transitions smoothly whenever its target changes.
///
/// Retargeting mid-flight starts the new transition from the *current*
/// visual value (and, for springs, keeps the current velocity), so
/// interrupted hover animations never jump.
#[derive(Clone, Debug)]
pub struct Animated<T: Lerp> {
    target: T,
    active: Option<Active<T>>,
}

impl<T: Lerp + PartialEq> Animated<T> {
    pub fn new(value: T) -> Self {
        Animated {
            target: value,
            active: None,
        }
    }

    pub fn target(&self) -> &T {
        &self.target
    }

    /// Sets a new target. Returns `true` if an animation was started.
    pub fn set(&mut self, target: T, transition: Transition, now: Instant) -> bool {
        if target == self.target {
            return false;
        }
        if transition.is_instant() {
            self.target = target;
            self.active = None;
            return false;
        }
        let current = self.get(now);
        // Carry velocity over (in value units) into the new normalized space.
        let v0 = match &self.active {
            Some(a) => {
                let elapsed = now.saturating_duration_since(a.start);
                let v_norm = a.transition.velocity(elapsed, a.v0);
                let old_dist = a.from.distance(&self.target);
                let new_dist = current.distance(&target);
                if new_dist > 1e-6 {
                    v_norm * old_dist / new_dist
                } else {
                    0.0
                }
            }
            None => 0.0,
        };
        self.active = Some(Active {
            from: current,
            start: now,
            transition,
            v0,
        });
        self.target = target;
        true
    }

    /// Jumps to `value` immediately, cancelling any animation.
    pub fn jump(&mut self, value: T) {
        self.target = value;
        self.active = None;
    }

    /// The value at time `now`.
    pub fn get(&self, now: Instant) -> T {
        match &self.active {
            None => self.target.clone(),
            Some(a) => {
                let elapsed = now.saturating_duration_since(a.start);
                if a.transition.is_finished(elapsed, a.v0) {
                    self.target.clone()
                } else {
                    a.from
                        .lerp(&self.target, a.transition.progress(elapsed, a.v0))
                }
            }
        }
    }

    /// Whether the value is still changing at `now`.
    pub fn is_animating(&self, now: Instant) -> bool {
        self.active.as_ref().is_some_and(|a| {
            !a.transition
                .is_finished(now.saturating_duration_since(a.start), a.v0)
        })
    }

    /// Drops finished animation state. Returns `true` if still animating.
    pub fn tick(&mut self, now: Instant) -> bool {
        if self.is_animating(now) {
            true
        } else {
            self.active = None;
            false
        }
    }
}

/// A repeating, time based animation (for loaders, pulses, shimmer, ...).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Animation {
    pub duration: Duration,
    pub easing: crate::Easing,
    pub repeat: Repeat,
    /// Reverse direction on every other iteration (ping-pong).
    pub alternate: bool,
    pub delay: Duration,
}

/// How many times an [`Animation`] runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Repeat {
    Once,
    Times(u32),
    Forever,
}

impl Animation {
    pub fn new(duration: Duration) -> Self {
        Animation {
            duration,
            easing: crate::Easing::Linear,
            repeat: Repeat::Once,
            alternate: false,
            delay: Duration::ZERO,
        }
    }

    pub fn repeat(mut self) -> Self {
        self.repeat = Repeat::Forever;
        self
    }

    pub fn times(mut self, n: u32) -> Self {
        self.repeat = Repeat::Times(n);
        self
    }

    pub fn alternate(mut self) -> Self {
        self.alternate = true;
        self
    }

    pub fn easing(mut self, easing: crate::Easing) -> Self {
        self.easing = easing;
        self
    }

    pub fn delay(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }

    /// Eased progress at `elapsed` and whether the animation has finished.
    pub fn sample(&self, elapsed: Duration) -> (f32, bool) {
        let Some(t) = elapsed.checked_sub(self.delay) else {
            return (self.easing.apply(0.0), false);
        };
        if self.duration.is_zero() {
            return (self.easing.apply(1.0), true);
        }
        let cycles = t.as_secs_f64() / self.duration.as_secs_f64();
        let total = match self.repeat {
            Repeat::Once => Some(1.0),
            Repeat::Times(n) => Some(n.max(1) as f64),
            Repeat::Forever => None,
        };
        if let Some(total) = total
            && cycles >= total
        {
            let last_reversed = self.alternate && (total as u64).is_multiple_of(2);
            return (
                self.easing.apply(if last_reversed { 0.0 } else { 1.0 }),
                true,
            );
        }
        let iteration = cycles.floor() as u64;
        let mut local = (cycles - cycles.floor()) as f32;
        if self.alternate && iteration % 2 == 1 {
            local = 1.0 - local;
        }
        (self.easing.apply(local), false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Spring;

    #[test]
    fn tween_reaches_target() {
        let t0 = Instant::now();
        let mut a = Animated::new(0.0f32);
        assert!(a.set(10.0, Transition::new(Duration::from_millis(100)), t0));
        assert_eq!(a.get(t0), 0.0);
        let mid = a.get(t0 + Duration::from_millis(50));
        assert!(mid > 0.0 && mid < 10.0);
        assert_eq!(a.get(t0 + Duration::from_millis(100)), 10.0);
        assert!(!a.is_animating(t0 + Duration::from_millis(120)));
    }

    #[test]
    fn retarget_starts_from_current_value() {
        let t0 = Instant::now();
        let mut a = Animated::new(0.0f32);
        a.set(100.0, Transition::new(Duration::from_millis(100)), t0);
        let t1 = t0 + Duration::from_millis(50);
        let at_interrupt = a.get(t1);
        a.set(0.0, Transition::new(Duration::from_millis(100)), t1);
        assert!(
            (a.get(t1) - at_interrupt).abs() < 1e-4,
            "no jump on retarget"
        );
    }

    #[test]
    fn spring_transition_settles() {
        let t0 = Instant::now();
        let mut a = Animated::new(1.0f32);
        a.set(1.05, Transition::spring(Spring::snappy()), t0);
        assert!(a.is_animating(t0 + Duration::from_millis(10)));
        let end = t0 + Duration::from_secs(3);
        assert!(!a.is_animating(end));
        assert_eq!(a.get(end), 1.05);
    }

    #[test]
    fn instant_transition_jumps() {
        let t0 = Instant::now();
        let mut a = Animated::new(0.0f32);
        assert!(!a.set(5.0, Transition::instant(), t0));
        assert_eq!(a.get(t0), 5.0);
    }

    #[test]
    fn repeating_animation() {
        let anim = Animation::new(Duration::from_millis(100)).repeat();
        let (p, done) = anim.sample(Duration::from_millis(250));
        assert!(!done);
        assert!((p - 0.5).abs() < 1e-3);
        let pp = Animation::new(Duration::from_millis(100))
            .repeat()
            .alternate();
        let (p, _) = pp.sample(Duration::from_millis(125));
        assert!((p - 0.75).abs() < 1e-3);
        let once = Animation::new(Duration::from_millis(100));
        assert_eq!(once.sample(Duration::from_millis(500)), (1.0, true));
    }
}
