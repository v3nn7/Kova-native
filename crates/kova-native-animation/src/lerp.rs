//! Interpolation of animatable values.

use kova_native_core::{Color, Corners, Edges, Fill, Point, Size};

/// A value that can be interpolated.
pub trait Lerp: Clone {
    /// Interpolates from `self` to `to`. `t` may lie outside `[0, 1]` for
    /// overshooting curves and springs.
    fn lerp(&self, to: &Self, t: f32) -> Self;

    /// A scalar "distance" between two values, used to carry velocity over
    /// when a spring animation is retargeted mid-flight.
    fn distance(&self, _to: &Self) -> f32 {
        1.0
    }
}

impl Lerp for f32 {
    fn lerp(&self, to: &f32, t: f32) -> f32 {
        self + (to - self) * t
    }

    fn distance(&self, to: &f32) -> f32 {
        (to - self).abs()
    }
}

impl Lerp for Point {
    fn lerp(&self, to: &Point, t: f32) -> Point {
        Point::new(self.x.lerp(&to.x, t), self.y.lerp(&to.y, t))
    }

    fn distance(&self, to: &Point) -> f32 {
        Point::distance(*self, *to)
    }
}

impl Lerp for Size {
    fn lerp(&self, to: &Size, t: f32) -> Size {
        Size::new(
            self.width.lerp(&to.width, t),
            self.height.lerp(&to.height, t),
        )
    }

    fn distance(&self, to: &Size) -> f32 {
        Point::new(to.width - self.width, to.height - self.height).length()
    }
}

impl Lerp for Color {
    fn lerp(&self, to: &Color, t: f32) -> Color {
        // Colors cannot meaningfully overshoot; clamp so springs stay valid.
        self.mix(*to, t.clamp(0.0, 1.0))
    }

    fn distance(&self, to: &Color) -> f32 {
        let d = [to.r - self.r, to.g - self.g, to.b - self.b, to.a - self.a];
        d.iter().map(|v| v * v).sum::<f32>().sqrt()
    }
}

impl Lerp for Fill {
    fn lerp(&self, to: &Fill, t: f32) -> Fill {
        self.mix(to, t.clamp(0.0, 1.0))
    }
}

impl Lerp for Corners<f32> {
    fn lerp(&self, to: &Self, t: f32) -> Self {
        Corners {
            top_left: self.top_left.lerp(&to.top_left, t).max(0.0),
            top_right: self.top_right.lerp(&to.top_right, t).max(0.0),
            bottom_right: self.bottom_right.lerp(&to.bottom_right, t).max(0.0),
            bottom_left: self.bottom_left.lerp(&to.bottom_left, t).max(0.0),
        }
    }
}

impl Lerp for Edges<f32> {
    fn lerp(&self, to: &Self, t: f32) -> Self {
        Edges {
            top: self.top.lerp(&to.top, t),
            right: self.right.lerp(&to.right, t),
            bottom: self.bottom.lerp(&to.bottom, t),
            left: self.left.lerp(&to.left, t),
        }
    }
}

impl<T: Lerp> Lerp for Option<T> {
    fn lerp(&self, to: &Self, t: f32) -> Self {
        match (self, to) {
            (Some(a), Some(b)) => Some(a.lerp(b, t)),
            _ => {
                if t < 0.5 {
                    self.clone()
                } else {
                    to.clone()
                }
            }
        }
    }
}
