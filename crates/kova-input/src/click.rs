//! Multi-click (double / triple click) detection.

use crate::MouseButton;
use kova_core::Point;
use std::time::{Duration, Instant};

/// Counts consecutive clicks that are close in time and space.
#[derive(Clone, Debug)]
pub struct ClickTracker {
    last: Option<(MouseButton, Point, Instant)>,
    count: u32,
    /// Maximum delay between clicks of a multi-click.
    pub interval: Duration,
    /// Maximum pointer travel between clicks of a multi-click.
    pub slop: f32,
}

impl Default for ClickTracker {
    fn default() -> Self {
        ClickTracker {
            last: None,
            count: 0,
            interval: Duration::from_millis(500),
            slop: 4.0,
        }
    }
}

impl ClickTracker {
    /// Registers a button press and returns its click count (1, 2, 3, ...).
    pub fn press(&mut self, button: MouseButton, position: Point, now: Instant) -> u32 {
        let continues = self.last.is_some_and(|(b, p, t)| {
            b == button
                && now.saturating_duration_since(t) <= self.interval
                && p.distance(position) <= self.slop
        });
        self.count = if continues { self.count + 1 } else { 1 };
        self.last = Some((button, position, now));
        self.count
    }

    pub fn count(&self) -> u32 {
        self.count
    }

    pub fn reset(&mut self) {
        self.last = None;
        self.count = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kova_core::point;

    #[test]
    fn double_and_triple_click() {
        let mut t = ClickTracker::default();
        let t0 = Instant::now();
        assert_eq!(t.press(MouseButton::Left, point(10.0, 10.0), t0), 1);
        assert_eq!(
            t.press(
                MouseButton::Left,
                point(11.0, 10.0),
                t0 + Duration::from_millis(200)
            ),
            2
        );
        assert_eq!(
            t.press(
                MouseButton::Left,
                point(11.0, 11.0),
                t0 + Duration::from_millis(400)
            ),
            3
        );
        // Too slow.
        assert_eq!(
            t.press(
                MouseButton::Left,
                point(11.0, 11.0),
                t0 + Duration::from_millis(1400)
            ),
            1
        );
        // Too far.
        assert_eq!(
            t.press(
                MouseButton::Left,
                point(50.0, 11.0),
                t0 + Duration::from_millis(1500)
            ),
            1
        );
        // Different button.
        assert_eq!(
            t.press(
                MouseButton::Right,
                point(50.0, 11.0),
                t0 + Duration::from_millis(1600)
            ),
            1
        );
    }
}
