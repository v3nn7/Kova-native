//! Colors and fills.
//!
//! [`Color`] stores non-premultiplied sRGB components in `0.0..=1.0`, which is
//! what designers and CSS work with. The renderer blends in sRGB space so that
//! Kova Native output matches design tools pixel-for-pixel.

/// An sRGB color with straight (non-premultiplied) alpha.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

/// Creates an opaque color from a `0xRRGGBB` hex value.
pub const fn rgb(hex: u32) -> Color {
    Color::from_rgba8(
        ((hex >> 16) & 0xff) as u8,
        ((hex >> 8) & 0xff) as u8,
        (hex & 0xff) as u8,
        255,
    )
}

/// Creates a color from a `0xRRGGBBAA` hex value.
pub const fn rgba(hex: u32) -> Color {
    Color::from_rgba8(
        ((hex >> 24) & 0xff) as u8,
        ((hex >> 16) & 0xff) as u8,
        ((hex >> 8) & 0xff) as u8,
        (hex & 0xff) as u8,
    )
}

/// Creates a color from hue (degrees), saturation, lightness (0..1) and alpha.
pub fn hsla(h: f32, s: f32, l: f32, a: f32) -> Color {
    Color::from_hsla(h, s, l, a)
}

impl Color {
    pub const TRANSPARENT: Color = Color {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.0,
    };
    pub const BLACK: Color = Color {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };
    pub const WHITE: Color = Color {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    };

    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub const fn from_rgba8(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
            a: a as f32 / 255.0,
        }
    }

    /// Parses `#rgb`, `#rgba`, `#rrggbb` or `#rrggbbaa` (leading `#` optional).
    pub fn from_hex(s: &str) -> Option<Color> {
        let s = s.strip_prefix('#').unwrap_or(s);
        let digit = |c: u8| -> Option<u8> {
            match c {
                b'0'..=b'9' => Some(c - b'0'),
                b'a'..=b'f' => Some(c - b'a' + 10),
                b'A'..=b'F' => Some(c - b'A' + 10),
                _ => None,
            }
        };
        let bytes = s.as_bytes();
        let short = |i: usize| digit(bytes[i]).map(|d| d * 17);
        let long = |i: usize| Some(digit(bytes[i])? * 16 + digit(bytes[i + 1])?);
        match bytes.len() {
            3 => Some(Color::from_rgba8(short(0)?, short(1)?, short(2)?, 255)),
            4 => Some(Color::from_rgba8(
                short(0)?,
                short(1)?,
                short(2)?,
                short(3)?,
            )),
            6 => Some(Color::from_rgba8(long(0)?, long(2)?, long(4)?, 255)),
            8 => Some(Color::from_rgba8(long(0)?, long(2)?, long(4)?, long(6)?)),
            _ => None,
        }
    }

    pub fn from_hsla(h: f32, s: f32, l: f32, a: f32) -> Self {
        let h = h.rem_euclid(360.0) / 360.0;
        let s = s.clamp(0.0, 1.0);
        let l = l.clamp(0.0, 1.0);
        if s == 0.0 {
            return Color::new(l, l, l, a);
        }
        let q = if l < 0.5 {
            l * (1.0 + s)
        } else {
            l + s - l * s
        };
        let p = 2.0 * l - q;
        let hue = |mut t: f32| {
            if t < 0.0 {
                t += 1.0;
            }
            if t > 1.0 {
                t -= 1.0;
            }
            if t < 1.0 / 6.0 {
                p + (q - p) * 6.0 * t
            } else if t < 0.5 {
                q
            } else if t < 2.0 / 3.0 {
                p + (q - p) * (2.0 / 3.0 - t) * 6.0
            } else {
                p
            }
        };
        Color::new(hue(h + 1.0 / 3.0), hue(h), hue(h - 1.0 / 3.0), a)
    }

    /// Returns `(hue_degrees, saturation, lightness, alpha)`.
    pub fn to_hsla(&self) -> (f32, f32, f32, f32) {
        let max = self.r.max(self.g).max(self.b);
        let min = self.r.min(self.g).min(self.b);
        let l = (max + min) * 0.5;
        if (max - min).abs() < f32::EPSILON {
            return (0.0, 0.0, l, self.a);
        }
        let d = max - min;
        let s = if l > 0.5 {
            d / (2.0 - max - min)
        } else {
            d / (max + min)
        };
        let h = if max == self.r {
            (self.g - self.b) / d + if self.g < self.b { 6.0 } else { 0.0 }
        } else if max == self.g {
            (self.b - self.r) / d + 2.0
        } else {
            (self.r - self.g) / d + 4.0
        };
        (h * 60.0, s, l, self.a)
    }

    pub fn with_alpha(self, a: f32) -> Self {
        Self { a, ..self }
    }

    /// Multiplies the alpha channel by `factor`.
    pub fn fade(self, factor: f32) -> Self {
        Self {
            a: self.a * factor,
            ..self
        }
    }

    /// Increases HSL lightness by `amount` (0..1).
    pub fn lighten(self, amount: f32) -> Self {
        let (h, s, l, a) = self.to_hsla();
        Color::from_hsla(h, s, (l + amount).min(1.0), a)
    }

    /// Decreases HSL lightness by `amount` (0..1).
    pub fn darken(self, amount: f32) -> Self {
        let (h, s, l, a) = self.to_hsla();
        Color::from_hsla(h, s, (l - amount).max(0.0), a)
    }

    pub fn is_transparent(&self) -> bool {
        self.a <= 0.0
    }

    /// Interpolates in premultiplied sRGB space (what CSS transitions do).
    pub fn mix(self, other: Color, t: f32) -> Color {
        let a = self.a + (other.a - self.a) * t;
        if a <= 1e-6 {
            return Color::new(
                self.r + (other.r - self.r) * t,
                self.g + (other.g - self.g) * t,
                self.b + (other.b - self.b) * t,
                0.0,
            );
        }
        let pm = |c0: f32, c1: f32| (c0 * self.a + (c1 * other.a - c0 * self.a) * t) / a;
        Color::new(
            pm(self.r, other.r),
            pm(self.g, other.g),
            pm(self.b, other.b),
            a,
        )
    }

    /// Packs into `[r, g, b, a]` bytes.
    pub fn to_rgba8(&self) -> [u8; 4] {
        let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        [q(self.r), q(self.g), q(self.b), q(self.a)]
    }

    /// Relative luminance per WCAG (useful for picking readable text colors).
    pub fn luminance(&self) -> f32 {
        let lin = |c: f32| {
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * lin(self.r) + 0.7152 * lin(self.g) + 0.0722 * lin(self.b)
    }
}

/// A gradient color stop.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorStop {
    pub color: Color,
    /// Position along the gradient line in `0.0..=1.0`.
    pub position: f32,
}

/// A two-stop linear gradient (evaluated on the GPU in Oklab space).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LinearGradient {
    /// CSS angle in degrees: `0` points up, `90` points right.
    pub angle: f32,
    pub from: ColorStop,
    pub to: ColorStop,
}

/// Creates a linear gradient at `angle` degrees from `from` to `to`.
pub fn linear_gradient(angle: f32, from: impl Into<Color>, to: impl Into<Color>) -> LinearGradient {
    LinearGradient {
        angle,
        from: ColorStop {
            color: from.into(),
            position: 0.0,
        },
        to: ColorStop {
            color: to.into(),
            position: 1.0,
        },
    }
}

impl LinearGradient {
    pub fn stops(mut self, from: f32, to: f32) -> Self {
        self.from.position = from;
        self.to.position = to;
        self
    }
}

/// How the interior of a shape is painted.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fill {
    Solid(Color),
    LinearGradient(LinearGradient),
}

impl Default for Fill {
    fn default() -> Self {
        Fill::Solid(Color::TRANSPARENT)
    }
}

impl Fill {
    pub fn is_transparent(&self) -> bool {
        match self {
            Fill::Solid(c) => c.is_transparent(),
            Fill::LinearGradient(g) => g.from.color.is_transparent() && g.to.color.is_transparent(),
        }
    }

    pub fn fade(self, factor: f32) -> Fill {
        match self {
            Fill::Solid(c) => Fill::Solid(c.fade(factor)),
            Fill::LinearGradient(mut g) => {
                g.from.color = g.from.color.fade(factor);
                g.to.color = g.to.color.fade(factor);
                Fill::LinearGradient(g)
            }
        }
    }

    fn as_gradient(&self, angle: f32) -> LinearGradient {
        match *self {
            Fill::Solid(c) => LinearGradient {
                angle,
                from: ColorStop {
                    color: c,
                    position: 0.0,
                },
                to: ColorStop {
                    color: c,
                    position: 1.0,
                },
            },
            Fill::LinearGradient(g) => g,
        }
    }

    /// Interpolates between two fills, promoting solids to gradients if needed.
    pub fn mix(&self, other: &Fill, t: f32) -> Fill {
        match (self, other) {
            (Fill::Solid(a), Fill::Solid(b)) => Fill::Solid(a.mix(*b, t)),
            _ => {
                let angle = match (self, other) {
                    (Fill::LinearGradient(g), _) | (_, Fill::LinearGradient(g)) => g.angle,
                    _ => 180.0,
                };
                let a = self.as_gradient(angle);
                let b = other.as_gradient(angle);
                let lerp = |x: f32, y: f32| x + (y - x) * t;
                Fill::LinearGradient(LinearGradient {
                    angle: lerp(a.angle, b.angle),
                    from: ColorStop {
                        color: a.from.color.mix(b.from.color, t),
                        position: lerp(a.from.position, b.from.position),
                    },
                    to: ColorStop {
                        color: a.to.color.mix(b.to.color, t),
                        position: lerp(a.to.position, b.to.position),
                    },
                })
            }
        }
    }
}

impl From<Color> for Fill {
    fn from(c: Color) -> Self {
        Fill::Solid(c)
    }
}

impl From<LinearGradient> for Fill {
    fn from(g: LinearGradient) -> Self {
        Fill::LinearGradient(g)
    }
}

impl From<u32> for Color {
    /// Interprets the integer as `0xRRGGBB`.
    fn from(hex: u32) -> Self {
        rgb(hex)
    }
}

impl From<u32> for Fill {
    fn from(hex: u32) -> Self {
        Fill::Solid(rgb(hex))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_parsing() {
        assert_eq!(Color::from_hex("#fff"), Some(Color::WHITE));
        assert_eq!(Color::from_hex("000000"), Some(Color::BLACK));
        assert_eq!(
            Color::from_hex("#ff000080").unwrap().to_rgba8(),
            [255, 0, 0, 128]
        );
        assert_eq!(Color::from_hex("#zz0000"), None);
        assert_eq!(rgb(0x336699).to_rgba8(), [0x33, 0x66, 0x99, 255]);
        assert_eq!(rgba(0x33669980).to_rgba8(), [0x33, 0x66, 0x99, 0x80]);
    }

    #[test]
    fn hsl_roundtrip() {
        let c = rgb(0x3b82f6);
        let (h, s, l, a) = c.to_hsla();
        let back = Color::from_hsla(h, s, l, a);
        let x = c.to_rgba8();
        let y = back.to_rgba8();
        for i in 0..4 {
            assert!((x[i] as i32 - y[i] as i32).abs() <= 1);
        }
    }

    #[test]
    fn premultiplied_mix() {
        // Mixing towards transparent must not darken the color.
        let red = rgb(0xff0000);
        let m = red.mix(Color::TRANSPARENT.with_alpha(0.0), 0.5);
        assert!((m.r - 1.0).abs() < 1e-5);
        assert!((m.a - 0.5).abs() < 1e-5);
    }
}
