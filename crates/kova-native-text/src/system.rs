//! Font database, fallback and glyph rasterization.

use cosmic_text::{FontSystem, SwashCache, SwashContent};

/// Identifies a rasterized glyph: font, glyph id, pixel size, subpixel offset
/// and weight. Used as the glyph atlas key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GlyphKey(pub(crate) cosmic_text::CacheKey);

/// A rasterized glyph bitmap.
#[derive(Clone, Debug)]
pub struct RasterizedGlyph {
    /// Offset of the bitmap's left edge from the glyph origin (device px).
    pub left: i32,
    /// Offset of the bitmap's top edge above the baseline (device px).
    pub top: i32,
    pub width: u32,
    pub height: u32,
    /// `true`: premultiplied RGBA (emoji). `false`: 8-bit coverage mask.
    pub is_color: bool,
    pub data: Vec<u8>,
}

/// Owns fonts and performs shaping-related lookups and rasterization.
///
/// One `TextSystem` is shared by all windows of an application.
pub struct TextSystem {
    pub(crate) fonts: FontSystem,
    swash: SwashCache,
    ui_family: String,
}

impl Default for TextSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl TextSystem {
    /// Loads system fonts and picks the platform UI font as default family.
    pub fn new() -> Self {
        let mut fonts = FontSystem::new();
        let candidates: &[&str] = if cfg!(target_os = "windows") {
            &["Segoe UI Variable Text", "Segoe UI", "Arial"]
        } else if cfg!(target_os = "macos") {
            &[
                "SF Pro Text",
                ".AppleSystemUIFont",
                "Helvetica Neue",
                "Helvetica",
            ]
        } else {
            &[
                "Inter",
                "Cantarell",
                "Ubuntu",
                "Noto Sans",
                "DejaVu Sans",
                "Liberation Sans",
            ]
        };
        let ui_family = candidates
            .iter()
            .find(|name| Self::db_has_family(&fonts, name))
            .map(|s| s.to_string())
            .unwrap_or_else(|| "sans-serif".to_string());
        // Variable "Segoe UI Variable" works, but its static sibling has better
        // weight coverage across Windows versions.
        let ui_family =
            if ui_family == "Segoe UI Variable Text" && Self::db_has_family(&fonts, "Segoe UI") {
                "Segoe UI".to_string()
            } else {
                ui_family
            };
        if ui_family != "sans-serif" {
            fonts.db_mut().set_sans_serif_family(ui_family.clone());
        }
        log::debug!(
            "kova-native-text: {} font faces, UI family '{ui_family}'",
            fonts.db().len()
        );
        TextSystem {
            fonts,
            swash: SwashCache::new(),
            ui_family,
        }
    }

    fn db_has_family(fonts: &FontSystem, name: &str) -> bool {
        fonts.db().faces().any(|f| {
            f.families
                .iter()
                .any(|(family, _)| family.eq_ignore_ascii_case(name))
        })
    }

    /// The family used when a style does not specify one.
    pub fn ui_family(&self) -> &str {
        &self.ui_family
    }

    /// Overrides the default UI family.
    pub fn set_ui_family(&mut self, family: impl Into<String>) {
        self.ui_family = family.into();
        self.fonts
            .db_mut()
            .set_sans_serif_family(self.ui_family.clone());
    }

    pub fn has_family(&self, name: &str) -> bool {
        Self::db_has_family(&self.fonts, name)
    }

    /// Registers a font from memory (TTF/OTF/TTC).
    pub fn load_font(&mut self, data: Vec<u8>) {
        self.fonts.db_mut().load_font_data(data);
    }

    pub fn face_count(&self) -> usize {
        self.fonts.db().len()
    }

    /// Rasterizes a glyph. Returns `None` for empty glyphs (e.g. spaces).
    pub fn rasterize(&mut self, key: GlyphKey) -> Option<RasterizedGlyph> {
        let image = self.swash.get_image_uncached(&mut self.fonts, key.0)?;
        if image.placement.width == 0 || image.placement.height == 0 {
            return None;
        }
        let (is_color, data) = match image.content {
            SwashContent::Mask => (false, image.data),
            SwashContent::SubpixelMask => {
                // Collapse RGB coverage to grayscale coverage.
                let data = image
                    .data
                    .chunks_exact(4)
                    .map(|px| ((px[0] as u16 + px[1] as u16 + px[2] as u16) / 3) as u8)
                    .collect();
                (false, data)
            }
            SwashContent::Color => {
                let mut data = image.data;
                for px in data.chunks_exact_mut(4) {
                    let a = px[3] as u16;
                    px[0] = ((px[0] as u16 * a + 127) / 255) as u8;
                    px[1] = ((px[1] as u16 * a + 127) / 255) as u8;
                    px[2] = ((px[2] as u16 * a + 127) / 255) as u8;
                }
                (true, data)
            }
        };
        Some(RasterizedGlyph {
            left: image.placement.left,
            top: image.placement.top,
            width: image.placement.width,
            height: image.placement.height,
            is_color,
            data,
        })
    }
}
