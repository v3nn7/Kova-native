//! Text styling.

use kova_core::{Color, SharedString};

/// Font weight on the CSS 100–900 scale.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FontWeight(pub u16);

#[allow(non_upper_case_globals)]
impl FontWeight {
    pub const Thin: FontWeight = FontWeight(100);
    pub const ExtraLight: FontWeight = FontWeight(200);
    pub const Light: FontWeight = FontWeight(300);
    pub const Normal: FontWeight = FontWeight(400);
    pub const Medium: FontWeight = FontWeight(500);
    pub const Semibold: FontWeight = FontWeight(600);
    pub const Bold: FontWeight = FontWeight(700);
    pub const ExtraBold: FontWeight = FontWeight(800);
    pub const Black: FontWeight = FontWeight(900);
}

impl Default for FontWeight {
    fn default() -> Self {
        FontWeight::Normal
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum FontStyle {
    #[default]
    Normal,
    Italic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
    Justify,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum TextWrap {
    /// Break between words, falling back to glyphs for very long words.
    #[default]
    Word,
    /// Break anywhere.
    Glyph,
    /// Single line per paragraph.
    None,
}

/// Line height, relative to the font size or absolute.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LineHeight {
    Relative(f32),
    Px(f32),
}

impl LineHeight {
    pub fn resolve(&self, font_size: f32) -> f32 {
        match *self {
            LineHeight::Relative(r) => (font_size * r).max(1.0),
            LineHeight::Px(px) => px.max(1.0),
        }
    }
}

impl Default for LineHeight {
    fn default() -> Self {
        LineHeight::Relative(1.4)
    }
}

/// The resolved style of a run of text.
#[derive(Clone, Debug, PartialEq)]
pub struct TextStyle {
    /// Font family; `None` uses the platform UI font.
    pub family: Option<SharedString>,
    pub size: f32,
    pub weight: FontWeight,
    pub style: FontStyle,
    pub line_height: LineHeight,
    pub color: Color,
    /// Extra spacing between glyphs in logical pixels.
    pub letter_spacing: f32,
    pub align: TextAlign,
    pub wrap: TextWrap,
    pub underline: bool,
    pub strikethrough: bool,
}

impl Default for TextStyle {
    fn default() -> Self {
        TextStyle {
            family: None,
            size: 14.0,
            weight: FontWeight::Normal,
            style: FontStyle::Normal,
            line_height: LineHeight::default(),
            color: Color::WHITE,
            letter_spacing: 0.0,
            align: TextAlign::Left,
            wrap: TextWrap::Word,
            underline: false,
            strikethrough: false,
        }
    }
}

impl TextStyle {
    /// Whether two styles produce the same glyph layout (ignores paint-only
    /// properties like color and decorations).
    pub fn same_layout(&self, other: &TextStyle) -> bool {
        self.family == other.family
            && self.size == other.size
            && self.weight == other.weight
            && self.style == other.style
            && self.line_height == other.line_height
            && self.letter_spacing == other.letter_spacing
            && self.align == other.align
            && self.wrap == other.wrap
    }
}
