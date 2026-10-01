//! Easing curves.

/// Maps linear progress `t ∈ [0, 1]` to eased progress.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum Easing {
    Linear,
    /// CSS `ease`.
    Ease,
    /// CSS `ease-in`.
    EaseIn,
    /// CSS `ease-out`.
    EaseOut,
    /// CSS `ease-in-out`.
    EaseInOut,
    /// Cubic ease-out: fast start, gentle stop. Kova's default for UI transitions.
    #[default]
    EaseOutCubic,
    EaseInOutCubic,
    EaseOutQuart,
    EaseOutExpo,
    /// Overshoots slightly before settling (`c1 = 1.70158`).
    EaseOutBack,
    /// A CSS-style `cubic-bezier(x1, y1, x2, y2)`.
    CubicBezier(f32, f32, f32, f32),
    /// Discrete jumps (CSS `steps(n, end)`).
    Steps(u32),
}

impl Easing {
    /// Applies the curve. Input is clamped to `[0, 1]`; output may overshoot
    /// for curves like [`Easing::EaseOutBack`].
    pub fn apply(&self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match *self {
            Easing::Linear => t,
            Easing::Ease => cubic_bezier(0.25, 0.1, 0.25, 1.0, t),
            Easing::EaseIn => cubic_bezier(0.42, 0.0, 1.0, 1.0, t),
            Easing::EaseOut => cubic_bezier(0.0, 0.0, 0.58, 1.0, t),
            Easing::EaseInOut => cubic_bezier(0.42, 0.0, 0.58, 1.0, t),
            Easing::EaseOutCubic => 1.0 - (1.0 - t).powi(3),
            Easing::EaseInOutCubic => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
                }
            }
            Easing::EaseOutQuart => 1.0 - (1.0 - t).powi(4),
            Easing::EaseOutExpo => {
                if t >= 1.0 {
                    1.0
                } else {
                    1.0 - 2f32.powf(-10.0 * t)
                }
            }
            Easing::EaseOutBack => {
                const C1: f32 = 1.70158;
                const C3: f32 = C1 + 1.0;
                1.0 + C3 * (t - 1.0).powi(3) + C1 * (t - 1.0).powi(2)
            }
            Easing::CubicBezier(x1, y1, x2, y2) => cubic_bezier(x1, y1, x2, y2, t),
            Easing::Steps(n) => {
                let n = n.max(1) as f32;
                (t * n).floor().min(n) / n
            }
        }
    }
}

/// Evaluates a CSS cubic-bezier timing function at `x`.
fn cubic_bezier(x1: f32, y1: f32, x2: f32, y2: f32, x: f32) -> f32 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    // Polynomial coefficients for B(t) = ((a t + b) t + c) t.
    let cx = 3.0 * x1;
    let bx = 3.0 * (x2 - x1) - cx;
    let ax = 1.0 - cx - bx;
    let cy = 3.0 * y1;
    let by = 3.0 * (y2 - y1) - cy;
    let ay = 1.0 - cy - by;

    let sample_x = |t: f32| ((ax * t + bx) * t + cx) * t;
    let sample_dx = |t: f32| (3.0 * ax * t + 2.0 * bx) * t + cx;
    let sample_y = |t: f32| ((ay * t + by) * t + cy) * t;

    // Newton-Raphson, falling back to bisection.
    let mut t = x;
    for _ in 0..8 {
        let err = sample_x(t) - x;
        if err.abs() < 1e-6 {
            return sample_y(t);
        }
        let d = sample_dx(t);
        if d.abs() < 1e-6 {
            break;
        }
        t -= err / d;
    }
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    t = x;
    for _ in 0..32 {
        let v = sample_x(t);
        if (v - x).abs() < 1e-6 {
            break;
        }
        if v < x {
            lo = t;
        } else {
            hi = t;
        }
        t = (lo + hi) * 0.5;
    }
    sample_y(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoints() {
        let all = [
            Easing::Linear,
            Easing::Ease,
            Easing::EaseIn,
            Easing::EaseOut,
            Easing::EaseInOut,
            Easing::EaseOutCubic,
            Easing::EaseInOutCubic,
            Easing::EaseOutQuart,
            Easing::EaseOutExpo,
            Easing::EaseOutBack,
            Easing::CubicBezier(0.2, 0.8, 0.2, 1.0),
            Easing::Steps(4),
        ];
        for e in all {
            assert!(e.apply(0.0).abs() < 1e-4, "{e:?} at 0");
            assert!((e.apply(1.0) - 1.0).abs() < 1e-4, "{e:?} at 1");
        }
    }

    #[test]
    fn css_ease_matches_reference() {
        // Reference values from browsers' cubic-bezier(0.25, 0.1, 0.25, 1).
        assert!((Easing::Ease.apply(0.5) - 0.8024).abs() < 2e-3);
        assert!((Easing::EaseInOut.apply(0.5) - 0.5).abs() < 1e-3);
    }

    #[test]
    fn monotonic_ease_out() {
        let mut prev = 0.0;
        for i in 0..=100 {
            let v = Easing::EaseOut.apply(i as f32 / 100.0);
            assert!(v + 1e-6 >= prev);
            prev = v;
        }
    }
}
