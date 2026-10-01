//! Layout style vocabulary (independent of the underlying solver).

use kova_core::Edges;

/// A length that can be absolute, relative to the parent, or automatic.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum Length {
    #[default]
    Auto,
    /// Logical pixels.
    Px(f32),
    /// Fraction of the parent's size (`0.5` = 50%).
    Relative(f32),
}

/// Logical pixels.
pub const fn px(v: f32) -> Length {
    Length::Px(v)
}

/// Percentage of the parent (`pct(50.)` = half).
pub fn pct(v: f32) -> Length {
    Length::Relative(v / 100.0)
}

/// Fraction of the parent (`relative(0.5)` = half).
pub const fn relative(fraction: f32) -> Length {
    Length::Relative(fraction)
}

/// Automatic sizing.
pub const fn auto() -> Length {
    Length::Auto
}

impl From<f32> for Length {
    fn from(v: f32) -> Self {
        Length::Px(v)
    }
}

impl From<i32> for Length {
    fn from(v: i32) -> Self {
        Length::Px(v as f32)
    }
}

impl Length {
    pub fn is_auto(&self) -> bool {
        matches!(self, Length::Auto)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Display {
    #[default]
    Flex,
    Grid,
    Block,
    /// Removed from layout and not painted.
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FlexDirection {
    #[default]
    Row,
    Column,
    RowReverse,
    ColumnReverse,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FlexWrap {
    #[default]
    NoWrap,
    Wrap,
    WrapReverse,
}

/// Cross axis alignment (`align-items`, `align-self`, `justify-items`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Start,
    End,
    Center,
    Stretch,
    Baseline,
}

/// Main axis distribution (`justify-content`, `align-content`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Justify {
    Start,
    End,
    Center,
    Stretch,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Position {
    /// Participates in flow layout; insets offset it visually.
    #[default]
    Relative,
    /// Taken out of flow and positioned against the parent's padding box.
    Absolute,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Overflow {
    #[default]
    Visible,
    /// Content is clipped; no scrolling.
    Hidden,
    /// Content is clipped and can be scrolled.
    Scroll,
}

/// A grid track size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Track {
    Px(f32),
    Relative(f32),
    /// Fraction of the remaining space (`fr`).
    Fr(f32),
    Auto,
    MinContent,
    MaxContent,
    /// `minmax(px, fr)`.
    MinMax(f32, f32),
}

/// Placement of a grid item along one axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum GridPlacement {
    #[default]
    Auto,
    /// Spans `n` tracks from the auto placed position.
    Span(u16),
    /// Starts at 1-based line `start` and spans `span` tracks.
    Line { start: i16, span: u16 },
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct GridStyle {
    pub template_columns: Vec<Track>,
    pub template_rows: Vec<Track>,
    pub auto_rows: Vec<Track>,
    pub column: GridPlacement,
    pub row: GridPlacement,
}

/// Width/height pair for layout properties.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Axes<T> {
    pub width: T,
    pub height: T,
}

impl<T: Copy> Axes<T> {
    pub const fn both(v: T) -> Self {
        Axes {
            width: v,
            height: v,
        }
    }
}

/// Everything that influences layout.
#[derive(Clone, Debug, PartialEq)]
pub struct LayoutStyle {
    pub display: Display,
    pub position: Position,
    pub direction: FlexDirection,
    pub wrap: FlexWrap,
    pub align_items: Option<Align>,
    pub align_self: Option<Align>,
    pub align_content: Option<Justify>,
    pub justify_content: Option<Justify>,
    pub size: Axes<Length>,
    pub min_size: Axes<Length>,
    pub max_size: Axes<Length>,
    pub aspect_ratio: Option<f32>,
    pub padding: Edges<Length>,
    pub margin: Edges<Length>,
    pub border: Edges<f32>,
    pub inset: Edges<Length>,
    /// `width` = column gap, `height` = row gap.
    pub gap: Axes<Length>,
    pub flex_grow: f32,
    pub flex_shrink: f32,
    pub flex_basis: Length,
    pub overflow_x: Overflow,
    pub overflow_y: Overflow,
    pub grid: Option<Box<GridStyle>>,
}

impl Default for LayoutStyle {
    fn default() -> Self {
        LayoutStyle {
            display: Display::Flex,
            position: Position::Relative,
            direction: FlexDirection::Row,
            wrap: FlexWrap::NoWrap,
            align_items: None,
            align_self: None,
            align_content: None,
            justify_content: None,
            size: Axes::both(Length::Auto),
            min_size: Axes::both(Length::Auto),
            max_size: Axes::both(Length::Auto),
            aspect_ratio: None,
            padding: Edges::all(Length::Px(0.0)),
            margin: Edges::all(Length::Px(0.0)),
            border: Edges::all(0.0),
            inset: Edges::all(Length::Auto),
            gap: Axes::both(Length::Px(0.0)),
            flex_grow: 0.0,
            flex_shrink: 1.0,
            flex_basis: Length::Auto,
            overflow_x: Overflow::Visible,
            overflow_y: Overflow::Visible,
            grid: None,
        }
    }
}

impl LayoutStyle {
    /// Whether children are clipped to this node's bounds.
    pub fn clips(&self) -> bool {
        self.overflow_x != Overflow::Visible || self.overflow_y != Overflow::Visible
    }

    pub fn scrolls(&self) -> bool {
        self.overflow_x == Overflow::Scroll || self.overflow_y == Overflow::Scroll
    }

    pub fn grid_mut(&mut self) -> &mut GridStyle {
        self.grid.get_or_insert_with(Default::default)
    }
}
