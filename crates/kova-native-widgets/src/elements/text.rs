//! Text.

use crate::context::{MeasureCx, PaintCx};
use crate::element::{Element, ElementBase};
use crate::style::Styled;
use kova_native_core::{Bounds, Color, Point, SharedString, Size};
use kova_native_layout::{AvailableSpace, MeasureInput};
use kova_native_text::{FontWeight, TextLayout, TextStyle};
use std::rc::Rc;

/// Text content: static, or computed reactively from signals.
#[derive(Clone)]
pub enum TextContent {
    Static(SharedString),
    Dynamic(Rc<dyn Fn() -> SharedString>),
}

impl From<&'static str> for TextContent {
    fn from(s: &'static str) -> Self {
        TextContent::Static(s.into())
    }
}

impl From<String> for TextContent {
    fn from(s: String) -> Self {
        TextContent::Static(s.into())
    }
}

impl From<&String> for TextContent {
    fn from(s: &String) -> Self {
        TextContent::Static(s.into())
    }
}

impl From<SharedString> for TextContent {
    fn from(s: SharedString) -> Self {
        TextContent::Static(s)
    }
}

/// `text(move || format!("{}", count.get()))` — re-evaluated when the
/// signals it reads change; only this text node is updated.
impl<F, R> From<F> for TextContent
where
    F: Fn() -> R + 'static,
    R: Into<SharedString>,
{
    fn from(f: F) -> Self {
        TextContent::Dynamic(Rc::new(move || f().into()))
    }
}

/// A run of text. Inherits font, size, color... from its ancestors unless
/// overridden.
pub struct Text {
    base: ElementBase,
    content: TextContent,
    current: SharedString,
    layout: TextLayout,
    style: Option<TextStyle>,
    text_dirty: bool,
}

/// Creates a text element from static or reactive content.
pub fn text(content: impl Into<TextContent>) -> Text {
    let content = content.into();
    let current = match &content {
        TextContent::Static(s) => s.clone(),
        TextContent::Dynamic(_) => SharedString::default(),
    };
    Text {
        base: ElementBase::new(),
        content,
        current,
        layout: TextLayout::new(),
        style: None,
        text_dirty: true,
    }
}

impl Text {
    /// Font size in logical pixels.
    pub fn size(self, size: f32) -> Self {
        self.text_size(size)
    }

    pub fn weight(self, weight: FontWeight) -> Self {
        self.font_weight(weight)
    }

    pub fn bold(self) -> Self {
        self.font_weight(FontWeight::Bold)
    }

    pub fn semibold(self) -> Self {
        self.font_weight(FontWeight::Semibold)
    }

    pub fn medium(self) -> Self {
        self.font_weight(FontWeight::Medium)
    }

    pub fn color(self, color: impl Into<Color>) -> Self {
        self.text_color(color)
    }

    pub fn font(self, family: impl Into<SharedString>) -> Self {
        self.font_family(family)
    }

    /// The current text.
    pub fn content(&self) -> &str {
        &self.current
    }

    fn sync(&mut self, cx_text: &mut kova_native_text::TextSystem) {
        if let Some(style) = &self.style
            && self.text_dirty
        {
            self.layout.set(cx_text, &self.current, style);
            self.text_dirty = false;
        }
    }
}

impl Element for Text {
    fn base(&self) -> &ElementBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut ElementBase {
        &mut self.base
    }

    fn name(&self) -> &'static str {
        "text"
    }

    fn is_measured(&self) -> bool {
        true
    }

    fn has_bindings(&self) -> bool {
        matches!(self.content, TextContent::Dynamic(_))
    }

    fn update_bindings(&mut self) -> bool {
        if let TextContent::Dynamic(f) = &self.content {
            let next = f();
            if next != self.current {
                self.current = next;
                self.text_dirty = true;
                return true;
            }
        }
        false
    }

    fn text_style_changed(&mut self, cx: &mut MeasureCx, style: &TextStyle) -> bool {
        self.style = Some(style.clone());
        let changed = self.layout.set(cx.text, &self.current, style);
        self.text_dirty = false;
        changed
    }

    fn measure(&mut self, cx: &mut MeasureCx, input: MeasureInput) -> Size {
        self.sync(cx.text);
        let width = input.known_width.or(match input.available_width {
            AvailableSpace::Definite(w) => Some(w),
            AvailableSpace::MinContent => Some(0.0),
            AvailableSpace::MaxContent => None,
        });
        let size = self.layout.measure(cx.text, width);
        Size::new(
            input.known_width.unwrap_or(size.width),
            input.known_height.unwrap_or(size.height),
        )
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        self.sync(cx.text_system());
        let content = cx.content_bounds();
        self.layout.layout(cx.text_system(), Some(content.width()));
        let color = cx.text_color();
        cx.paint_text(&self.layout, content.origin, color);
        if let Some(style) = &self.style
            && (style.underline || style.strikethrough)
        {
            let thickness = (style.size / 14.0).max(1.0);
            for line in self.layout.lines() {
                let x = content.origin.x + line.x;
                if style.underline {
                    let y = content.origin.y + line.baseline + style.size * 0.12;
                    cx.fill_rect(
                        Bounds::new(Point::new(x, y), Size::new(line.width, thickness)),
                        color,
                    );
                }
                if style.strikethrough {
                    let y = content.origin.y + line.baseline - style.size * 0.3;
                    cx.fill_rect(
                        Bounds::new(Point::new(x, y), Size::new(line.width, thickness)),
                        color,
                    );
                }
            }
        }
    }
}

crate::impl_element_builder!(Text);
