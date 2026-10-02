//! Text for Kova Native.
//!
//! Text is a first-class part of the framework:
//!
//! * [`TextSystem`] owns the font database (system fonts + loaded fonts),
//!   font fallback (emoji, CJK, symbols) and glyph rasterization with
//!   subpixel positioning at any HiDPI scale.
//! * [`TextLayout`] holds shaped (HarfBuzz-compatible shaping, kerning,
//!   ligatures, BiDi), wrapped and measured text, and answers hit-testing,
//!   caret and selection geometry queries.
//! * [`TextStyle`] describes how text looks.

mod layout;
mod style;
mod system;

pub use layout::{GlyphInstance, LineMetrics, TextLayout};
pub use style::{FontStyle, FontWeight, LineHeight, TextAlign, TextStyle, TextWrap};
pub use system::{GlyphKey, RasterizedGlyph, TextSystem};

#[cfg(test)]
mod tests;

/// Generic family name resolved to the platform monospace font
/// (Cascadia Mono / Consolas on Windows, SF Mono / Menlo on macOS, ...).
pub const MONOSPACE: &str = "monospace";
