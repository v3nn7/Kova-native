//! Shaped, wrapped, measurable text.

use crate::style::{FontStyle, TextAlign, TextStyle, TextWrap};
use crate::system::{GlyphKey, TextSystem};
use cosmic_text::{Attrs, Buffer, Family, Metrics, Shaping};
use kova_core::{Bounds, Color, Point, Size};
use measure_cache_impl::SmallCache;

/// A glyph positioned in device pixels, ready to be rasterized and drawn.
#[derive(Clone, Copy, Debug)]
pub struct GlyphInstance {
    pub key: GlyphKey,
    /// Pen position (glyph origin on the baseline) in whole device pixels.
    /// The subpixel remainder is encoded in `key`.
    pub x: i32,
    pub y: i32,
    pub color: Option<Color>,
}

/// Metrics of one visual line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineMetrics {
    /// Paragraph index (lines separated by `\n`).
    pub paragraph: usize,
    pub top: f32,
    pub baseline: f32,
    pub height: f32,
    /// Leftmost glyph edge.
    pub x: f32,
    pub width: f32,
}

/// A block of text shaped with a single style.
///
/// `TextLayout` is retained by text elements: shaping results are cached per
/// paragraph by cosmic-text, and measurements are cached per wrap width, so
/// re-layout after a parent resize only re-wraps lines.
pub struct TextLayout {
    buffer: Buffer,
    text: String,
    style: Option<TextStyle>,
    width: Option<f32>,
    measure_cache: SmallCache,
}

mod measure_cache_impl {
    use kova_core::Size;

    /// Tiny LRU-ish cache of `(wrap width -> size)` measurements.
    #[derive(Default)]
    pub struct SmallCache {
        entries: Vec<(Option<u32>, Size)>,
    }

    impl SmallCache {
        pub fn get(&self, width: Option<f32>) -> Option<Size> {
            let key = width.map(f32::to_bits);
            self.entries
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, s)| *s)
        }

        pub fn insert(&mut self, width: Option<f32>, size: Size) {
            if self.entries.len() >= 8 {
                self.entries.remove(0);
            }
            self.entries.push((width.map(f32::to_bits), size));
        }

        pub fn clear(&mut self) {
            self.entries.clear();
        }
    }
}

impl TextLayout {
    pub fn new() -> Self {
        TextLayout {
            buffer: Buffer::new_empty(Metrics::new(14.0, 20.0)),
            text: String::new(),
            style: None,
            width: None,
            measure_cache: SmallCache::default(),
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn style(&self) -> Option<&TextStyle> {
        self.style.as_ref()
    }

    /// Updates text and style. Returns `true` if the layout (size) may have
    /// changed; paint-only style changes return `false`.
    pub fn set(&mut self, ts: &mut TextSystem, text: &str, style: &TextStyle) -> bool {
        let layout_changed =
            self.text != text || self.style.as_ref().is_none_or(|s| !s.same_layout(style));
        if !layout_changed {
            self.style = Some(style.clone());
            return false;
        }
        self.text.clear();
        self.text.push_str(text);
        self.style = Some(style.clone());
        self.measure_cache.clear();

        let line_height = style.line_height.resolve(style.size);
        self.buffer
            .set_metrics(Metrics::new(style.size, line_height));
        self.buffer.set_wrap(match style.wrap {
            TextWrap::Word => cosmic_text::Wrap::WordOrGlyph,
            TextWrap::Glyph => cosmic_text::Wrap::Glyph,
            TextWrap::None => cosmic_text::Wrap::None,
        });
        let family = style
            .family
            .as_deref()
            .unwrap_or(ts.ui_family())
            .to_string();
        let attrs = Attrs::new()
            .family(Family::Name(&family))
            .weight(cosmic_text::Weight(style.weight.0))
            .style(match style.style {
                FontStyle::Normal => cosmic_text::Style::Normal,
                FontStyle::Italic => cosmic_text::Style::Italic,
            })
            .letter_spacing(if style.size > 0.0 {
                style.letter_spacing / style.size
            } else {
                0.0
            });
        let align = match style.align {
            TextAlign::Left => None,
            TextAlign::Center => Some(cosmic_text::Align::Center),
            TextAlign::Right => Some(cosmic_text::Align::Right),
            TextAlign::Justify => Some(cosmic_text::Align::Justified),
        };
        self.buffer.set_text(text, &attrs, Shaping::Advanced, align);
        self.width = None;
        self.buffer.set_size(None, None);
        true
    }

    fn line_height(&self) -> f32 {
        self.buffer.metrics().line_height
    }

    /// Measures the text wrapped to `max_width` (`None` = no wrapping).
    pub fn measure(&mut self, ts: &mut TextSystem, max_width: Option<f32>) -> Size {
        if let Some(size) = self.measure_cache.get(max_width) {
            return size;
        }
        self.apply_width(ts, max_width);
        let size = self.current_size();
        self.measure_cache.insert(max_width, size);
        size
    }

    /// Lays the text out for painting at the final width.
    pub fn layout(&mut self, ts: &mut TextSystem, width: Option<f32>) -> Size {
        // A hair of slack guards against float noise making the final width
        // a tiny bit smaller than the measured one, which would re-wrap.
        self.apply_width(ts, width.map(|w| w + 0.05));
        self.current_size()
    }

    fn apply_width(&mut self, ts: &mut TextSystem, width: Option<f32>) {
        let width = width.map(|w| w.max(0.0));
        if self.width != width {
            self.width = width;
            self.buffer.set_size(width, None);
        }
        self.buffer.shape_until_scroll(&mut ts.fonts, false);
    }

    fn current_size(&self) -> Size {
        let mut width: f32 = 0.0;
        let mut height: f32 = 0.0;
        for run in self.buffer.layout_runs() {
            width = width.max(run.line_w);
            height = height.max(run.line_top + run.line_height);
        }
        if height == 0.0 {
            height = self.line_height();
        }
        // Round up so layout never truncates the last glyph's antialiasing.
        Size::new((width * 64.0).ceil() / 64.0, height)
    }

    /// Size at the current layout width.
    pub fn size(&self) -> Size {
        self.current_size()
    }

    /// Visual lines at the current layout width.
    pub fn lines(&self) -> Vec<LineMetrics> {
        self.buffer
            .layout_runs()
            .map(|run| {
                let x = run.glyphs.first().map_or(0.0, |g| g.x);
                let right = run.glyphs.last().map_or(0.0, |g| g.x + g.w);
                LineMetrics {
                    paragraph: run.line_i,
                    top: run.line_top,
                    baseline: run.line_y,
                    height: run.line_height,
                    x,
                    width: (right - x).max(0.0),
                }
            })
            .collect()
    }

    /// Visits every glyph positioned for an `origin` given in logical pixels,
    /// rendered at `scale` device pixels per logical pixel.
    pub fn for_each_glyph(&self, origin: Point, scale: f32, mut f: impl FnMut(GlyphInstance)) {
        let ox = origin.x * scale;
        let oy = origin.y * scale;
        for run in self.buffer.layout_runs() {
            let baseline = oy + run.line_y * scale;
            for glyph in run.glyphs {
                let physical = glyph.physical((ox, baseline), scale);
                f(GlyphInstance {
                    key: GlyphKey(physical.cache_key),
                    x: physical.x,
                    y: physical.y,
                    color: glyph
                        .color_opt
                        .map(|c| Color::from_rgba8(c.r(), c.g(), c.b(), c.a())),
                });
            }
        }
    }

    fn paragraph_offsets(&self) -> Vec<usize> {
        let mut offsets = Vec::with_capacity(self.buffer.lines.len());
        let mut offset = 0;
        for line in &self.buffer.lines {
            offsets.push(offset);
            offset += line.text().len() + line.ending().as_str().len();
        }
        offsets
    }

    fn to_offset(&self, cursor: cosmic_text::Cursor) -> usize {
        let offsets = self.paragraph_offsets();
        offsets
            .get(cursor.line)
            .map_or(self.text.len(), |o| o + cursor.index)
    }

    fn to_cursor(&self, offset: usize) -> cosmic_text::Cursor {
        let offsets = self.paragraph_offsets();
        let mut line = 0;
        for (i, start) in offsets.iter().enumerate() {
            if *start <= offset {
                line = i;
            }
        }
        let len = self.buffer.lines.get(line).map_or(0, |l| l.text().len());
        cosmic_text::Cursor::new(
            line,
            (offset - offsets.get(line).copied().unwrap_or(0)).min(len),
        )
    }

    /// Byte offset of the character boundary closest to `point` (logical px,
    /// relative to the text origin).
    pub fn hit_test(&self, point: Point) -> usize {
        match self.buffer.hit(point.x, point.y) {
            Some(cursor) => self.to_offset(cursor),
            None => {
                if point.y < 0.0 {
                    0
                } else {
                    self.text.len()
                }
            }
        }
    }

    /// Caret rectangle for byte `offset` (logical px, relative to the origin).
    pub fn caret_bounds(&self, offset: usize) -> Bounds {
        let cursor = self.to_cursor(offset);
        let line_height = self.line_height();
        for run in self.buffer.layout_runs() {
            if run.line_i != cursor.line {
                continue;
            }
            let first = run.glyphs.first();
            let last = run.glyphs.last();
            let in_run = match (first, last) {
                (Some(f), Some(l)) => cursor.index >= f.start && cursor.index <= l.end,
                _ => true,
            };
            if !in_run {
                continue;
            }
            let x = run
                .glyphs
                .iter()
                .find(|g| cursor.index >= g.start && cursor.index < g.end)
                .map(|g| {
                    // Interpolate inside ligatures / multi-byte clusters.
                    let span = (g.end - g.start).max(1) as f32;
                    g.x + g.w * ((cursor.index - g.start) as f32 / span)
                })
                .unwrap_or_else(|| last.map_or(0.0, |g| g.x + g.w));
            return Bounds::new(Point::new(x, run.line_top), Size::new(1.0, run.line_height));
        }
        Bounds::new(
            Point::new(0.0, cursor.line as f32 * line_height),
            Size::new(1.0, line_height),
        )
    }

    /// Highlight rectangles covering the byte range (logical px).
    pub fn selection_bounds(&self, start: usize, end: usize) -> Vec<Bounds> {
        let (start, end) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };
        let a = self.to_cursor(start);
        let b = self.to_cursor(end);
        let mut rects = Vec::new();
        for run in self.buffer.layout_runs() {
            for (x, w) in run.highlight(a, b) {
                rects.push(Bounds::new(
                    Point::new(x, run.line_top),
                    Size::new(w.max(1.0), run.line_height),
                ));
            }
        }
        rects
    }
}

impl Default for TextLayout {
    fn default() -> Self {
        Self::new()
    }
}
