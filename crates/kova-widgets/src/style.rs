//! Styles and the [`Styled`] builder trait.
//!
//! A [`Style`] combines layout properties (flexbox/grid/spacing/sizing) with
//! visual properties (fill, border, radii, shadows, opacity, transforms) and
//! cascading text properties. State styles (`hover`, `active`, `focus`,
//! `disabled`) are closures `Fn(Style) -> Style` applied on top of the base
//! style, so they use exactly the same builder vocabulary:
//!
//! ```ignore
//! div().bg(theme.surface).hover(|s| s.bg(theme.surface_hover).scale(1.02))
//! ```

use kova_animation::{Lerp, Transition};
use kova_core::{Color, Corners, Edges, Fill, Point, SharedString};
use kova_input::CursorStyle;
use kova_layout::{
    Align, Axes, Display, FlexDirection, FlexWrap, GridPlacement, Justify, LayoutStyle, Length,
    Overflow, Position, Track,
};
use kova_text::{FontStyle, FontWeight, LineHeight, TextAlign, TextStyle, TextWrap};
use smallvec::SmallVec;

/// A CSS-like box shadow.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoxShadow {
    pub color: Color,
    pub offset: Point,
    pub blur: f32,
    pub spread: f32,
    pub inset: bool,
}

impl BoxShadow {
    pub fn new(offset_y: f32, blur: f32, color: Color) -> Self {
        BoxShadow {
            color,
            offset: Point::new(0.0, offset_y),
            blur,
            spread: 0.0,
            inset: false,
        }
    }

    pub fn offset(mut self, x: f32, y: f32) -> Self {
        self.offset = Point::new(x, y);
        self
    }

    pub fn spread(mut self, spread: f32) -> Self {
        self.spread = spread;
        self
    }

    pub fn inset(mut self) -> Self {
        self.inset = true;
        self
    }
}

impl Lerp for BoxShadow {
    fn lerp(&self, to: &Self, t: f32) -> Self {
        BoxShadow {
            color: self.color.lerp(&to.color, t),
            offset: self.offset.lerp(&to.offset, t),
            blur: self.blur.lerp(&to.blur, t).max(0.0),
            spread: self.spread.lerp(&to.spread, t),
            inset: if t < 0.5 { self.inset } else { to.inset },
        }
    }
}

/// Text properties that cascade to descendants. `None` = inherit.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextRefinement {
    pub family: Option<SharedString>,
    pub size: Option<f32>,
    pub weight: Option<FontWeight>,
    pub style: Option<FontStyle>,
    pub line_height: Option<LineHeight>,
    pub color: Option<Color>,
    pub letter_spacing: Option<f32>,
    pub align: Option<TextAlign>,
    pub wrap: Option<TextWrap>,
    pub underline: Option<bool>,
    pub strikethrough: Option<bool>,
}

impl TextRefinement {
    pub fn is_empty(&self) -> bool {
        *self == TextRefinement::default()
    }

    /// Applies the refinement on top of an inherited style.
    pub fn apply(&self, base: &TextStyle) -> TextStyle {
        TextStyle {
            family: self.family.clone().or_else(|| base.family.clone()),
            size: self.size.unwrap_or(base.size),
            weight: self.weight.unwrap_or(base.weight),
            style: self.style.unwrap_or(base.style),
            line_height: self.line_height.unwrap_or(base.line_height),
            color: self.color.unwrap_or(base.color),
            letter_spacing: self.letter_spacing.unwrap_or(base.letter_spacing),
            align: self.align.unwrap_or(base.align),
            wrap: self.wrap.unwrap_or(base.wrap),
            underline: self.underline.unwrap_or(base.underline),
            strikethrough: self.strikethrough.unwrap_or(base.strikethrough),
        }
    }
}

/// The complete style of an element.
#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    pub layout: LayoutStyle,
    pub background: Option<Fill>,
    pub border_color: Color,
    pub corner_radii: Corners<f32>,
    pub shadows: SmallVec<[BoxShadow; 2]>,
    pub opacity: f32,
    pub scale: f32,
    /// Rotation in degrees (clockwise).
    pub rotate: f32,
    pub translate: Point,
    /// Pivot for scale/rotation as a fraction of the element size.
    pub transform_origin: Point,
    pub text: TextRefinement,
    pub cursor: Option<CursorStyle>,
    /// Frosted glass: blur radius applied to whatever is behind the element.
    pub backdrop_blur: Option<f32>,
    pub disabled: bool,
    /// When `false` the element is transparent to the pointer.
    pub pointer_events: bool,
    pub transition: Option<Transition>,
}

impl Default for Style {
    fn default() -> Self {
        Style {
            layout: LayoutStyle::default(),
            background: None,
            border_color: Color::TRANSPARENT,
            corner_radii: Corners::ZERO,
            shadows: SmallVec::new(),
            opacity: 1.0,
            scale: 1.0,
            rotate: 0.0,
            translate: Point::ZERO,
            transform_origin: Point::new(0.5, 0.5),
            text: TextRefinement::default(),
            cursor: None,
            backdrop_blur: None,
            disabled: false,
            pointer_events: true,
            transition: None,
        }
    }
}

/// The animatable subset of a style.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Visual {
    pub background: Fill,
    pub border_color: Color,
    pub corner_radii: Corners<f32>,
    pub shadows: SmallVec<[BoxShadow; 2]>,
    pub opacity: f32,
    pub scale: f32,
    pub rotate: f32,
    pub translate: Point,
    pub text_color: Option<Color>,
}

impl Visual {
    pub fn from_style(style: &Style) -> Visual {
        Visual {
            background: style.background.unwrap_or(Fill::Solid(Color::TRANSPARENT)),
            border_color: style.border_color,
            corner_radii: style.corner_radii,
            shadows: style.shadows.clone(),
            opacity: style.opacity,
            scale: style.scale,
            rotate: style.rotate,
            translate: style.translate,
            text_color: style.text.color,
        }
    }

    pub fn has_transform(&self) -> bool {
        self.scale != 1.0 || self.rotate != 0.0 || self.translate != Point::ZERO
    }
}

impl Lerp for Visual {
    fn lerp(&self, to: &Self, t: f32) -> Self {
        let shadows = if self.shadows.len() == to.shadows.len() {
            self.shadows
                .iter()
                .zip(to.shadows.iter())
                .map(|(a, b)| a.lerp(b, t))
                .collect()
        } else {
            // Fade shadows in/out by pairing with transparent copies.
            let n = self.shadows.len().max(to.shadows.len());
            (0..n)
                .map(|i| {
                    let a = self.shadows.get(i).copied();
                    let b = to.shadows.get(i).copied();
                    match (a, b) {
                        (Some(a), Some(b)) => a.lerp(&b, t),
                        (Some(a), None) => a.lerp(
                            &BoxShadow {
                                color: a.color.with_alpha(0.0),
                                ..a
                            },
                            t,
                        ),
                        (None, Some(b)) => BoxShadow {
                            color: b.color.with_alpha(0.0),
                            ..b
                        }
                        .lerp(&b, t),
                        (None, None) => unreachable!(),
                    }
                })
                .collect()
        };
        Visual {
            background: self.background.lerp(&to.background, t),
            border_color: self.border_color.lerp(&to.border_color, t),
            corner_radii: self.corner_radii.lerp(&to.corner_radii, t),
            shadows,
            opacity: self.opacity.lerp(&to.opacity, t).clamp(0.0, 1.0),
            scale: self.scale.lerp(&to.scale, t),
            rotate: self.rotate.lerp(&to.rotate, t),
            translate: self.translate.lerp(&to.translate, t),
            text_color: self.text_color.lerp(&to.text_color, t),
        }
    }

    fn distance(&self, to: &Self) -> f32 {
        // Dominated by transform changes for spring velocity carry-over.
        (self.scale - to.scale).abs() * 100.0
            + (self.rotate - to.rotate).abs()
            + self.translate.distance(to.translate)
            + (self.opacity - to.opacity).abs() * 100.0
            + 1.0
    }
}

/// Builder methods shared by every element *and* by [`Style`] itself (so
/// state closures like `.hover(|s| s.bg(..))` use the same vocabulary).
pub trait Styled: Sized {
    fn style_mut(&mut self) -> &mut Style;

    /// Applies an arbitrary modification.
    fn with_style(mut self, f: impl FnOnce(&mut Style)) -> Self {
        f(self.style_mut());
        self
    }

    /// Conditionally applies a modification.
    fn when(self, condition: bool, f: impl FnOnce(Self) -> Self) -> Self {
        if condition { f(self) } else { self }
    }

    // ---- display & flex -------------------------------------------------

    fn flex(self) -> Self {
        self.with_style(|s| s.layout.display = Display::Flex)
    }
    fn block(self) -> Self {
        self.with_style(|s| s.layout.display = Display::Block)
    }
    fn hidden(self) -> Self {
        self.with_style(|s| s.layout.display = Display::None)
    }
    fn flex_row(self) -> Self {
        self.with_style(|s| {
            s.layout.display = Display::Flex;
            s.layout.direction = FlexDirection::Row
        })
    }
    fn flex_col(self) -> Self {
        self.with_style(|s| {
            s.layout.display = Display::Flex;
            s.layout.direction = FlexDirection::Column
        })
    }
    fn flex_row_reverse(self) -> Self {
        self.with_style(|s| s.layout.direction = FlexDirection::RowReverse)
    }
    fn flex_col_reverse(self) -> Self {
        self.with_style(|s| s.layout.direction = FlexDirection::ColumnReverse)
    }
    fn flex_wrap(self) -> Self {
        self.with_style(|s| s.layout.wrap = FlexWrap::Wrap)
    }
    fn flex_grow(self) -> Self {
        self.with_style(|s| s.layout.flex_grow = 1.0)
    }
    fn grow(self, factor: f32) -> Self {
        self.with_style(|s| s.layout.flex_grow = factor)
    }
    fn flex_shrink_0(self) -> Self {
        self.with_style(|s| s.layout.flex_shrink = 0.0)
    }
    fn shrink(self, factor: f32) -> Self {
        self.with_style(|s| s.layout.flex_shrink = factor)
    }
    fn basis(self, basis: impl Into<Length>) -> Self {
        let basis = basis.into();
        self.with_style(|s| s.layout.flex_basis = basis)
    }
    /// `flex: 1 1 0` — share remaining space equally with siblings.
    fn flex_1(self) -> Self {
        self.with_style(|s| {
            s.layout.flex_grow = 1.0;
            s.layout.flex_shrink = 1.0;
            s.layout.flex_basis = Length::Px(0.0)
        })
    }

    // ---- alignment ------------------------------------------------------

    fn items_start(self) -> Self {
        self.with_style(|s| s.layout.align_items = Some(Align::Start))
    }
    fn items_center(self) -> Self {
        self.with_style(|s| s.layout.align_items = Some(Align::Center))
    }
    fn items_end(self) -> Self {
        self.with_style(|s| s.layout.align_items = Some(Align::End))
    }
    fn items_stretch(self) -> Self {
        self.with_style(|s| s.layout.align_items = Some(Align::Stretch))
    }
    fn items_baseline(self) -> Self {
        self.with_style(|s| s.layout.align_items = Some(Align::Baseline))
    }
    fn self_start(self) -> Self {
        self.with_style(|s| s.layout.align_self = Some(Align::Start))
    }
    fn self_center(self) -> Self {
        self.with_style(|s| s.layout.align_self = Some(Align::Center))
    }
    fn self_end(self) -> Self {
        self.with_style(|s| s.layout.align_self = Some(Align::End))
    }
    fn self_stretch(self) -> Self {
        self.with_style(|s| s.layout.align_self = Some(Align::Stretch))
    }
    fn justify_start(self) -> Self {
        self.with_style(|s| s.layout.justify_content = Some(Justify::Start))
    }
    fn justify_center(self) -> Self {
        self.with_style(|s| s.layout.justify_content = Some(Justify::Center))
    }
    fn justify_end(self) -> Self {
        self.with_style(|s| s.layout.justify_content = Some(Justify::End))
    }
    fn justify_between(self) -> Self {
        self.with_style(|s| s.layout.justify_content = Some(Justify::SpaceBetween))
    }
    fn justify_around(self) -> Self {
        self.with_style(|s| s.layout.justify_content = Some(Justify::SpaceAround))
    }
    fn justify_evenly(self) -> Self {
        self.with_style(|s| s.layout.justify_content = Some(Justify::SpaceEvenly))
    }
    /// Centers children on both axes.
    fn center(self) -> Self {
        self.items_center().justify_center()
    }

    // ---- spacing ----------------------------------------------------------

    fn gap(self, gap: impl Into<Length>) -> Self {
        let gap = gap.into();
        self.with_style(|s| s.layout.gap = Axes::both(gap))
    }
    fn gap_x(self, gap: impl Into<Length>) -> Self {
        let gap = gap.into();
        self.with_style(|s| s.layout.gap.width = gap)
    }
    fn gap_y(self, gap: impl Into<Length>) -> Self {
        let gap = gap.into();
        self.with_style(|s| s.layout.gap.height = gap)
    }
    /// Padding: `f32` (all), `(vertical, horizontal)` or `(top, right, bottom, left)`.
    fn padding(self, padding: impl Into<Edges<f32>>) -> Self {
        let p = padding.into();
        self.with_style(|s| s.layout.padding = p.map(Length::Px))
    }
    fn px(self, v: f32) -> Self {
        self.with_style(|s| {
            s.layout.padding.left = Length::Px(v);
            s.layout.padding.right = Length::Px(v)
        })
    }
    fn py(self, v: f32) -> Self {
        self.with_style(|s| {
            s.layout.padding.top = Length::Px(v);
            s.layout.padding.bottom = Length::Px(v)
        })
    }
    fn pt(self, v: f32) -> Self {
        self.with_style(|s| s.layout.padding.top = Length::Px(v))
    }
    fn pb(self, v: f32) -> Self {
        self.with_style(|s| s.layout.padding.bottom = Length::Px(v))
    }
    fn pl(self, v: f32) -> Self {
        self.with_style(|s| s.layout.padding.left = Length::Px(v))
    }
    fn pr(self, v: f32) -> Self {
        self.with_style(|s| s.layout.padding.right = Length::Px(v))
    }
    /// Margin: `f32` (all), `(vertical, horizontal)` or `(top, right, bottom, left)`.
    fn margin(self, margin: impl Into<Edges<f32>>) -> Self {
        let m = margin.into();
        self.with_style(|s| s.layout.margin = m.map(Length::Px))
    }
    fn mt(self, v: f32) -> Self {
        self.with_style(|s| s.layout.margin.top = Length::Px(v))
    }
    fn mb(self, v: f32) -> Self {
        self.with_style(|s| s.layout.margin.bottom = Length::Px(v))
    }
    fn ml(self, v: f32) -> Self {
        self.with_style(|s| s.layout.margin.left = Length::Px(v))
    }
    fn mr(self, v: f32) -> Self {
        self.with_style(|s| s.layout.margin.right = Length::Px(v))
    }
    /// `margin-left: auto` — pushes the element to the end of a row.
    fn ml_auto(self) -> Self {
        self.with_style(|s| s.layout.margin.left = Length::Auto)
    }
    fn mx_auto(self) -> Self {
        self.with_style(|s| {
            s.layout.margin.left = Length::Auto;
            s.layout.margin.right = Length::Auto
        })
    }

    // ---- sizing -------------------------------------------------------------

    fn w(self, width: impl Into<Length>) -> Self {
        let w = width.into();
        self.with_style(|s| s.layout.size.width = w)
    }
    fn h(self, height: impl Into<Length>) -> Self {
        let h = height.into();
        self.with_style(|s| s.layout.size.height = h)
    }
    /// Sets width and height.
    fn size(self, size: impl Into<Length>) -> Self {
        let v = size.into();
        self.with_style(|s| s.layout.size = Axes::both(v))
    }
    fn w_full(self) -> Self {
        self.with_style(|s| s.layout.size.width = Length::Relative(1.0))
    }
    fn h_full(self) -> Self {
        self.with_style(|s| s.layout.size.height = Length::Relative(1.0))
    }
    fn size_full(self) -> Self {
        self.w_full().h_full()
    }
    fn min_w(self, v: impl Into<Length>) -> Self {
        let v = v.into();
        self.with_style(|s| s.layout.min_size.width = v)
    }
    fn min_h(self, v: impl Into<Length>) -> Self {
        let v = v.into();
        self.with_style(|s| s.layout.min_size.height = v)
    }
    fn max_w(self, v: impl Into<Length>) -> Self {
        let v = v.into();
        self.with_style(|s| s.layout.max_size.width = v)
    }
    fn max_h(self, v: impl Into<Length>) -> Self {
        let v = v.into();
        self.with_style(|s| s.layout.max_size.height = v)
    }
    fn aspect_ratio(self, ratio: f32) -> Self {
        self.with_style(|s| s.layout.aspect_ratio = Some(ratio))
    }

    // ---- positioning ----------------------------------------------------------

    fn relative(self) -> Self {
        self.with_style(|s| s.layout.position = Position::Relative)
    }
    fn absolute(self) -> Self {
        self.with_style(|s| s.layout.position = Position::Absolute)
    }
    fn top(self, v: impl Into<Length>) -> Self {
        let v = v.into();
        self.with_style(|s| s.layout.inset.top = v)
    }
    fn right(self, v: impl Into<Length>) -> Self {
        let v = v.into();
        self.with_style(|s| s.layout.inset.right = v)
    }
    fn bottom(self, v: impl Into<Length>) -> Self {
        let v = v.into();
        self.with_style(|s| s.layout.inset.bottom = v)
    }
    fn left(self, v: impl Into<Length>) -> Self {
        let v = v.into();
        self.with_style(|s| s.layout.inset.left = v)
    }
    /// Absolute positioning covering the parent (`inset: 0`).
    fn inset_0(self) -> Self {
        self.absolute().top(0.0).right(0.0).bottom(0.0).left(0.0)
    }

    // ---- overflow -------------------------------------------------------------

    fn overflow_hidden(self) -> Self {
        self.with_style(|s| {
            s.layout.overflow_x = Overflow::Hidden;
            s.layout.overflow_y = Overflow::Hidden
        })
    }
    fn overflow_scroll(self) -> Self {
        self.with_style(|s| {
            s.layout.overflow_x = Overflow::Scroll;
            s.layout.overflow_y = Overflow::Scroll
        })
    }
    fn overflow_y_scroll(self) -> Self {
        self.with_style(|s| {
            s.layout.overflow_y = Overflow::Scroll;
            if s.layout.overflow_x == Overflow::Visible {
                s.layout.overflow_x = Overflow::Hidden;
            }
        })
    }
    fn overflow_x_scroll(self) -> Self {
        self.with_style(|s| {
            s.layout.overflow_x = Overflow::Scroll;
            if s.layout.overflow_y == Overflow::Visible {
                s.layout.overflow_y = Overflow::Hidden;
            }
        })
    }

    // ---- grid -------------------------------------------------------------------

    fn grid(self) -> Self {
        self.with_style(|s| s.layout.display = Display::Grid)
    }
    /// `n` equal columns (`repeat(n, 1fr)`).
    fn grid_cols(self, n: usize) -> Self {
        self.with_style(|s| {
            s.layout.display = Display::Grid;
            s.layout.grid_mut().template_columns = vec![Track::Fr(1.0); n];
        })
    }
    fn grid_template_columns(self, tracks: Vec<Track>) -> Self {
        self.with_style(|s| {
            s.layout.display = Display::Grid;
            s.layout.grid_mut().template_columns = tracks;
        })
    }
    fn grid_template_rows(self, tracks: Vec<Track>) -> Self {
        self.with_style(|s| s.layout.grid_mut().template_rows = tracks)
    }
    fn col_span(self, n: u16) -> Self {
        self.with_style(|s| s.layout.grid_mut().column = GridPlacement::Span(n))
    }
    fn row_span(self, n: u16) -> Self {
        self.with_style(|s| s.layout.grid_mut().row = GridPlacement::Span(n))
    }

    // ---- visuals ----------------------------------------------------------------

    /// Background fill: a color, `0xRRGGBB` or a gradient.
    fn bg(self, fill: impl Into<Fill>) -> Self {
        let fill = fill.into();
        self.with_style(|s| s.background = Some(fill))
    }
    /// Corner radius for all corners.
    fn rounded(self, radius: f32) -> Self {
        self.with_style(|s| s.corner_radii = Corners::all(radius))
    }
    /// Independent radii `(top_left, top_right, bottom_right, bottom_left)`.
    fn corners(self, radii: impl Into<Corners<f32>>) -> Self {
        let radii = radii.into();
        self.with_style(|s| s.corner_radii = radii)
    }
    fn rounded_full(self) -> Self {
        self.rounded(9999.0)
    }
    /// Border width on all sides (affects layout like CSS `border`).
    fn border(self, width: f32) -> Self {
        self.with_style(|s| s.layout.border = Edges::all(width))
    }
    fn border_widths(self, widths: impl Into<Edges<f32>>) -> Self {
        let w = widths.into();
        self.with_style(|s| s.layout.border = w)
    }
    fn border_color(self, color: impl Into<Color>) -> Self {
        let c = color.into();
        self.with_style(|s| s.border_color = c)
    }
    fn border_b(self, width: f32) -> Self {
        self.with_style(|s| s.layout.border.bottom = width)
    }
    fn border_t(self, width: f32) -> Self {
        self.with_style(|s| s.layout.border.top = width)
    }
    fn border_l(self, width: f32) -> Self {
        self.with_style(|s| s.layout.border.left = width)
    }
    fn border_r(self, width: f32) -> Self {
        self.with_style(|s| s.layout.border.right = width)
    }
    /// Adds a drop shadow (stackable).
    fn shadow(self, shadow: BoxShadow) -> Self {
        self.with_style(|s| s.shadows.push(shadow))
    }
    /// A soft, layered elevation shadow (`level` 1–5).
    fn elevation(self, level: u8) -> Self {
        let l = level.clamp(0, 5) as f32;
        self.with_style(|s| {
            s.shadows.clear();
            if l > 0.0 {
                s.shadows
                    .push(BoxShadow::new(l, l * 2.0, Color::BLACK.with_alpha(0.18)));
                s.shadows.push(BoxShadow::new(
                    l * 4.0,
                    l * 10.0,
                    Color::BLACK.with_alpha(0.22),
                ));
            }
        })
    }
    fn no_shadow(self) -> Self {
        self.with_style(|s| s.shadows.clear())
    }
    fn opacity(self, opacity: f32) -> Self {
        self.with_style(|s| s.opacity = opacity.clamp(0.0, 1.0))
    }
    /// Uniform scale around the transform origin.
    fn scale(self, scale: f32) -> Self {
        self.with_style(|s| s.scale = scale)
    }
    /// Rotation in degrees.
    fn rotate(self, degrees: f32) -> Self {
        self.with_style(|s| s.rotate = degrees)
    }
    /// Visual offset (does not affect layout).
    fn translate(self, x: f32, y: f32) -> Self {
        self.with_style(|s| s.translate = Point::new(x, y))
    }
    fn translate_y(self, y: f32) -> Self {
        self.with_style(|s| s.translate.y = y)
    }
    fn translate_x(self, x: f32) -> Self {
        self.with_style(|s| s.translate.x = x)
    }
    /// Pivot for scale/rotate as fractions of the size (default center).
    fn transform_origin(self, x: f32, y: f32) -> Self {
        self.with_style(|s| s.transform_origin = Point::new(x, y))
    }
    fn backdrop_blur(self, radius: f32) -> Self {
        self.with_style(|s| s.backdrop_blur = Some(radius))
    }
    fn cursor(self, cursor: CursorStyle) -> Self {
        self.with_style(|s| s.cursor = Some(cursor))
    }
    fn cursor_pointer(self) -> Self {
        self.cursor(CursorStyle::PointingHand)
    }
    fn cursor_text(self) -> Self {
        self.cursor(CursorStyle::IBeam)
    }
    fn disabled(self, disabled: bool) -> Self {
        self.with_style(|s| s.disabled = disabled)
    }
    /// Makes the element transparent to mouse events.
    fn pointer_events_none(self) -> Self {
        self.with_style(|s| s.pointer_events = false)
    }
    /// Animates visual changes (colors, radii, shadows, opacity, transforms)
    /// caused by state or reactive updates. Accepts a `Duration`, a
    /// [`kova_animation::Spring`] or a full [`Transition`].
    fn transition(self, transition: impl Into<Transition>) -> Self {
        let t = transition.into();
        self.with_style(|s| s.transition = Some(t))
    }

    // ---- text (cascades to descendants) ---------------------------------

    fn text_color(self, color: impl Into<Color>) -> Self {
        let c = color.into();
        self.with_style(|s| s.text.color = Some(c))
    }
    fn text_size(self, size: f32) -> Self {
        self.with_style(|s| s.text.size = Some(size))
    }
    fn font_weight(self, weight: FontWeight) -> Self {
        self.with_style(|s| s.text.weight = Some(weight))
    }
    fn font_family(self, family: impl Into<SharedString>) -> Self {
        let f = family.into();
        self.with_style(|s| s.text.family = Some(f))
    }
    fn italic(self) -> Self {
        self.with_style(|s| s.text.style = Some(FontStyle::Italic))
    }
    fn line_height(self, multiplier: f32) -> Self {
        self.with_style(|s| s.text.line_height = Some(LineHeight::Relative(multiplier)))
    }
    fn letter_spacing(self, px: f32) -> Self {
        self.with_style(|s| s.text.letter_spacing = Some(px))
    }
    fn text_center(self) -> Self {
        self.with_style(|s| s.text.align = Some(TextAlign::Center))
    }
    fn text_right(self) -> Self {
        self.with_style(|s| s.text.align = Some(TextAlign::Right))
    }
    fn text_left(self) -> Self {
        self.with_style(|s| s.text.align = Some(TextAlign::Left))
    }
    fn whitespace_nowrap(self) -> Self {
        self.with_style(|s| s.text.wrap = Some(TextWrap::None))
    }
    fn underline(self) -> Self {
        self.with_style(|s| s.text.underline = Some(true))
    }
    fn line_through(self) -> Self {
        self.with_style(|s| s.text.strikethrough = Some(true))
    }
}

impl Styled for Style {
    fn style_mut(&mut self) -> &mut Style {
        self
    }
}
