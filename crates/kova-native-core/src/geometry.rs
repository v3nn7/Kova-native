//! 2D geometry primitives used throughout Kova Native.
//!
//! All UI coordinates are expressed in *logical pixels* (`f32`) unless a type
//! or function explicitly says otherwise. Device pixels are obtained by
//! multiplying with the window scale factor.

use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

/// A point (or vector) in 2D space.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

/// Shorthand constructor for [`Point`].
#[inline]
pub const fn point(x: f32, y: f32) -> Point {
    Point { x, y }
}

impl Point {
    pub const ZERO: Point = Point { x: 0.0, y: 0.0 };

    #[inline]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    #[inline]
    pub fn length(self) -> f32 {
        (self.x * self.x + self.y * self.y).sqrt()
    }

    #[inline]
    pub fn distance(self, other: Point) -> f32 {
        (self - other).length()
    }

    #[inline]
    pub fn scale(self, factor: f32) -> Self {
        Self::new(self.x * factor, self.y * factor)
    }

    #[inline]
    pub fn round(self) -> Self {
        Self::new(self.x.round(), self.y.round())
    }

    #[inline]
    pub fn min(self, other: Point) -> Self {
        Self::new(self.x.min(other.x), self.y.min(other.y))
    }

    #[inline]
    pub fn max(self, other: Point) -> Self {
        Self::new(self.x.max(other.x), self.y.max(other.y))
    }
}

impl Add for Point {
    type Output = Point;
    #[inline]
    fn add(self, rhs: Point) -> Point {
        Point::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl AddAssign for Point {
    #[inline]
    fn add_assign(&mut self, rhs: Point) {
        self.x += rhs.x;
        self.y += rhs.y;
    }
}

impl Sub for Point {
    type Output = Point;
    #[inline]
    fn sub(self, rhs: Point) -> Point {
        Point::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl SubAssign for Point {
    #[inline]
    fn sub_assign(&mut self, rhs: Point) {
        self.x -= rhs.x;
        self.y -= rhs.y;
    }
}

impl Neg for Point {
    type Output = Point;
    #[inline]
    fn neg(self) -> Point {
        Point::new(-self.x, -self.y)
    }
}

impl Mul<f32> for Point {
    type Output = Point;
    #[inline]
    fn mul(self, rhs: f32) -> Point {
        self.scale(rhs)
    }
}

impl Div<f32> for Point {
    type Output = Point;
    #[inline]
    fn div(self, rhs: f32) -> Point {
        Point::new(self.x / rhs, self.y / rhs)
    }
}

impl From<(f32, f32)> for Point {
    fn from((x, y): (f32, f32)) -> Self {
        Self::new(x, y)
    }
}

/// A 2D size.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

/// Shorthand constructor for [`Size`].
#[inline]
pub const fn size(width: f32, height: f32) -> Size {
    Size { width, height }
}

impl Size {
    pub const ZERO: Size = Size {
        width: 0.0,
        height: 0.0,
    };

    #[inline]
    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }

    #[inline]
    pub fn scale(self, factor: f32) -> Self {
        Self::new(self.width * factor, self.height * factor)
    }

    #[inline]
    pub fn is_empty(self) -> bool {
        self.width <= 0.0 || self.height <= 0.0
    }

    #[inline]
    pub fn max(self, other: Size) -> Self {
        Self::new(self.width.max(other.width), self.height.max(other.height))
    }

    #[inline]
    pub fn min(self, other: Size) -> Self {
        Self::new(self.width.min(other.width), self.height.min(other.height))
    }

    #[inline]
    pub fn to_point(self) -> Point {
        Point::new(self.width, self.height)
    }
}

impl From<(f32, f32)> for Size {
    fn from((w, h): (f32, f32)) -> Self {
        Self::new(w, h)
    }
}

impl Mul<f32> for Size {
    type Output = Size;
    fn mul(self, rhs: f32) -> Size {
        self.scale(rhs)
    }
}

/// An axis-aligned rectangle described by its origin (top-left) and size.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Bounds {
    pub origin: Point,
    pub size: Size,
}

/// Shorthand constructor for [`Bounds`].
#[inline]
pub const fn bounds(x: f32, y: f32, width: f32, height: f32) -> Bounds {
    Bounds {
        origin: Point { x, y },
        size: Size { width, height },
    }
}

impl Bounds {
    pub const ZERO: Bounds = Bounds {
        origin: Point::ZERO,
        size: Size::ZERO,
    };

    /// A rectangle so large it effectively means "no bounds".
    pub const INFINITE: Bounds = Bounds {
        origin: Point {
            x: -1.0e7,
            y: -1.0e7,
        },
        size: Size {
            width: 2.0e7,
            height: 2.0e7,
        },
    };

    #[inline]
    pub const fn new(origin: Point, size: Size) -> Self {
        Self { origin, size }
    }

    pub fn from_corners(a: Point, b: Point) -> Self {
        let min = a.min(b);
        let max = a.max(b);
        Self::new(min, Size::new(max.x - min.x, max.y - min.y))
    }

    #[inline]
    pub fn left(&self) -> f32 {
        self.origin.x
    }
    #[inline]
    pub fn top(&self) -> f32 {
        self.origin.y
    }
    #[inline]
    pub fn right(&self) -> f32 {
        self.origin.x + self.size.width
    }
    #[inline]
    pub fn bottom(&self) -> f32 {
        self.origin.y + self.size.height
    }
    #[inline]
    pub fn width(&self) -> f32 {
        self.size.width
    }
    #[inline]
    pub fn height(&self) -> f32 {
        self.size.height
    }

    #[inline]
    pub fn center(&self) -> Point {
        Point::new(
            self.origin.x + self.size.width * 0.5,
            self.origin.y + self.size.height * 0.5,
        )
    }

    #[inline]
    pub fn bottom_right(&self) -> Point {
        Point::new(self.right(), self.bottom())
    }

    #[inline]
    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.left() && p.x < self.right() && p.y >= self.top() && p.y < self.bottom()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.size.is_empty()
    }

    pub fn intersects(&self, other: &Bounds) -> bool {
        self.left() < other.right()
            && other.left() < self.right()
            && self.top() < other.bottom()
            && other.top() < self.bottom()
    }

    /// The overlapping region of two rectangles (empty if disjoint).
    pub fn intersect(&self, other: &Bounds) -> Bounds {
        let l = self.left().max(other.left());
        let t = self.top().max(other.top());
        let r = self.right().min(other.right());
        let b = self.bottom().min(other.bottom());
        Bounds::new(
            Point::new(l, t),
            Size::new((r - l).max(0.0), (b - t).max(0.0)),
        )
    }

    /// The smallest rectangle containing both rectangles.
    pub fn union(&self, other: &Bounds) -> Bounds {
        let l = self.left().min(other.left());
        let t = self.top().min(other.top());
        let r = self.right().max(other.right());
        let b = self.bottom().max(other.bottom());
        Bounds::new(Point::new(l, t), Size::new(r - l, b - t))
    }

    pub fn translate(&self, by: Point) -> Bounds {
        Bounds::new(self.origin + by, self.size)
    }

    /// Grows the rectangle by `amount` on every side.
    pub fn inflate(&self, amount: f32) -> Bounds {
        Bounds::new(
            Point::new(self.origin.x - amount, self.origin.y - amount),
            Size::new(
                (self.size.width + amount * 2.0).max(0.0),
                (self.size.height + amount * 2.0).max(0.0),
            ),
        )
    }

    /// Shrinks the rectangle by the given edge insets.
    pub fn inset(&self, edges: Edges<f32>) -> Bounds {
        Bounds::new(
            Point::new(self.origin.x + edges.left, self.origin.y + edges.top),
            Size::new(
                (self.size.width - edges.left - edges.right).max(0.0),
                (self.size.height - edges.top - edges.bottom).max(0.0),
            ),
        )
    }

    pub fn scale(&self, factor: f32) -> Bounds {
        Bounds::new(self.origin.scale(factor), self.size.scale(factor))
    }
}

/// Values for the four edges of a box (CSS order: top, right, bottom, left).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Edges<T> {
    pub top: T,
    pub right: T,
    pub bottom: T,
    pub left: T,
}

impl<T: Copy> Edges<T> {
    pub const fn all(v: T) -> Self {
        Self {
            top: v,
            right: v,
            bottom: v,
            left: v,
        }
    }

    pub const fn symmetric(vertical: T, horizontal: T) -> Self {
        Self {
            top: vertical,
            right: horizontal,
            bottom: vertical,
            left: horizontal,
        }
    }

    pub fn map<U>(&self, f: impl Fn(T) -> U) -> Edges<U> {
        Edges {
            top: f(self.top),
            right: f(self.right),
            bottom: f(self.bottom),
            left: f(self.left),
        }
    }
}

impl Edges<f32> {
    pub const ZERO: Edges<f32> = Edges {
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
        left: 0.0,
    };

    pub fn horizontal(&self) -> f32 {
        self.left + self.right
    }

    pub fn vertical(&self) -> f32 {
        self.top + self.bottom
    }

    pub fn is_zero(&self) -> bool {
        self.top == 0.0 && self.right == 0.0 && self.bottom == 0.0 && self.left == 0.0
    }

    pub fn max_value(&self) -> f32 {
        self.top.max(self.right).max(self.bottom).max(self.left)
    }
}

impl From<f32> for Edges<f32> {
    fn from(v: f32) -> Self {
        Edges::all(v)
    }
}

/// `(vertical, horizontal)` like CSS `padding: 10px 18px`.
impl From<(f32, f32)> for Edges<f32> {
    fn from((v, h): (f32, f32)) -> Self {
        Edges::symmetric(v, h)
    }
}

/// `(top, right, bottom, left)` like CSS.
impl From<(f32, f32, f32, f32)> for Edges<f32> {
    fn from((top, right, bottom, left): (f32, f32, f32, f32)) -> Self {
        Edges {
            top,
            right,
            bottom,
            left,
        }
    }
}

/// Values for the four corners of a box.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Corners<T> {
    pub top_left: T,
    pub top_right: T,
    pub bottom_right: T,
    pub bottom_left: T,
}

impl<T: Copy> Corners<T> {
    pub const fn all(v: T) -> Self {
        Self {
            top_left: v,
            top_right: v,
            bottom_right: v,
            bottom_left: v,
        }
    }

    pub fn map<U>(&self, f: impl Fn(T) -> U) -> Corners<U> {
        Corners {
            top_left: f(self.top_left),
            top_right: f(self.top_right),
            bottom_right: f(self.bottom_right),
            bottom_left: f(self.bottom_left),
        }
    }
}

impl Corners<f32> {
    pub const ZERO: Corners<f32> = Corners::all(0.0);

    pub fn is_zero(&self) -> bool {
        self.top_left == 0.0
            && self.top_right == 0.0
            && self.bottom_right == 0.0
            && self.bottom_left == 0.0
    }

    pub fn max_value(&self) -> f32 {
        self.top_left
            .max(self.top_right)
            .max(self.bottom_right)
            .max(self.bottom_left)
    }

    /// Clamps radii so that adjacent corners never overlap for a box of `size`
    /// (the CSS "radius scaling" rule).
    pub fn clamp_to(&self, size: Size) -> Corners<f32> {
        let mut factor: f32 = 1.0;
        let pairs = [
            (self.top_left + self.top_right, size.width),
            (self.bottom_left + self.bottom_right, size.width),
            (self.top_left + self.bottom_left, size.height),
            (self.top_right + self.bottom_right, size.height),
        ];
        for (sum, extent) in pairs {
            if sum > extent && sum > 0.0 {
                factor = factor.min(extent / sum);
            }
        }
        self.map(|r| (r * factor).max(0.0))
    }

    pub fn scale(&self, factor: f32) -> Corners<f32> {
        self.map(|r| r * factor)
    }
}

impl From<f32> for Corners<f32> {
    fn from(v: f32) -> Self {
        Corners::all(v)
    }
}

/// `(top_left, top_right, bottom_right, bottom_left)`.
impl From<(f32, f32, f32, f32)> for Corners<f32> {
    fn from((tl, tr, br, bl): (f32, f32, f32, f32)) -> Self {
        Corners {
            top_left: tl,
            top_right: tr,
            bottom_right: br,
            bottom_left: bl,
        }
    }
}

/// A 2D affine transform stored as a 2x3 matrix:
///
/// ```text
/// | a  b  tx |
/// | c  d  ty |
/// ```
///
/// mapping `(x, y)` to `(a*x + b*y + tx, c*x + d*y + ty)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform2D {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub tx: f32,
    pub ty: f32,
}

impl Default for Transform2D {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Transform2D {
    pub const IDENTITY: Transform2D = Transform2D {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        tx: 0.0,
        ty: 0.0,
    };

    pub fn translate(x: f32, y: f32) -> Self {
        Self {
            tx: x,
            ty: y,
            ..Self::IDENTITY
        }
    }

    pub fn scale(sx: f32, sy: f32) -> Self {
        Self {
            a: sx,
            d: sy,
            ..Self::IDENTITY
        }
    }

    /// Rotation by `radians` (clockwise in screen space, since y points down).
    pub fn rotate(radians: f32) -> Self {
        let (s, c) = radians.sin_cos();
        Self {
            a: c,
            b: -s,
            c: s,
            d: c,
            tx: 0.0,
            ty: 0.0,
        }
    }

    pub fn is_identity(&self) -> bool {
        *self == Self::IDENTITY
    }

    /// Returns `self ∘ other`: applies `other` first, then `self`.
    pub fn then(&self, after: &Transform2D) -> Transform2D {
        // after * self
        let m = after;
        Transform2D {
            a: m.a * self.a + m.b * self.c,
            b: m.a * self.b + m.b * self.d,
            c: m.c * self.a + m.d * self.c,
            d: m.c * self.b + m.d * self.d,
            tx: m.a * self.tx + m.b * self.ty + m.tx,
            ty: m.c * self.tx + m.d * self.ty + m.ty,
        }
    }

    /// A transform that applies `self` around the pivot point `origin`.
    pub fn around(&self, origin: Point) -> Transform2D {
        Transform2D::translate(-origin.x, -origin.y)
            .then(self)
            .then(&Transform2D::translate(origin.x, origin.y))
    }

    #[inline]
    pub fn apply(&self, p: Point) -> Point {
        Point::new(
            self.a * p.x + self.b * p.y + self.tx,
            self.c * p.x + self.d * p.y + self.ty,
        )
    }

    pub fn determinant(&self) -> f32 {
        self.a * self.d - self.b * self.c
    }

    pub fn inverse(&self) -> Option<Transform2D> {
        let det = self.determinant();
        if det.abs() < 1e-12 {
            return None;
        }
        let inv = 1.0 / det;
        let a = self.d * inv;
        let b = -self.b * inv;
        let c = -self.c * inv;
        let d = self.a * inv;
        Some(Transform2D {
            a,
            b,
            c,
            d,
            tx: -(a * self.tx + b * self.ty),
            ty: -(c * self.tx + d * self.ty),
        })
    }

    /// Axis-aligned bounding box of a transformed rectangle.
    pub fn apply_bounds(&self, r: &Bounds) -> Bounds {
        if self.b == 0.0 && self.c == 0.0 {
            let p0 = self.apply(r.origin);
            let p1 = self.apply(r.bottom_right());
            return Bounds::from_corners(p0, p1);
        }
        let pts = [
            self.apply(r.origin),
            self.apply(Point::new(r.right(), r.top())),
            self.apply(r.bottom_right()),
            self.apply(Point::new(r.left(), r.bottom())),
        ];
        let mut min = pts[0];
        let mut max = pts[0];
        for p in &pts[1..] {
            min = min.min(*p);
            max = max.max(*p);
        }
        Bounds::from_corners(min, max)
    }

    /// Uniform scale factor approximation (sqrt of |det|).
    pub fn approx_scale(&self) -> f32 {
        self.determinant().abs().sqrt()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: Point, b: Point) -> bool {
        (a.x - b.x).abs() < 1e-4 && (a.y - b.y).abs() < 1e-4
    }

    #[test]
    fn bounds_ops() {
        let a = bounds(0.0, 0.0, 10.0, 10.0);
        let b = bounds(5.0, 5.0, 10.0, 10.0);
        assert!(a.intersects(&b));
        assert_eq!(a.intersect(&b), bounds(5.0, 5.0, 5.0, 5.0));
        assert_eq!(a.union(&b), bounds(0.0, 0.0, 15.0, 15.0));
        assert!(a.contains(point(9.9, 0.0)));
        assert!(!a.contains(point(10.0, 0.0)));
        assert!(a.intersect(&bounds(20.0, 20.0, 1.0, 1.0)).is_empty());
    }

    #[test]
    fn transform_compose_and_invert() {
        let t = Transform2D::scale(2.0, 2.0).around(point(10.0, 10.0));
        assert!(approx(t.apply(point(10.0, 10.0)), point(10.0, 10.0)));
        assert!(approx(t.apply(point(11.0, 10.0)), point(12.0, 10.0)));
        let inv = t.inverse().unwrap();
        assert!(approx(inv.apply(t.apply(point(3.0, 7.0))), point(3.0, 7.0)));

        let r = Transform2D::rotate(std::f32::consts::FRAC_PI_2);
        assert!(approx(r.apply(point(1.0, 0.0)), point(0.0, 1.0)));
        let tr = Transform2D::translate(5.0, 0.0).then(&r);
        assert!(approx(tr.apply(point(0.0, 0.0)), point(0.0, 5.0)));
    }

    #[test]
    fn corner_clamping() {
        let c = Corners::all(50.0).clamp_to(size(40.0, 100.0));
        assert!((c.top_left - 20.0).abs() < 1e-5);
    }
}
