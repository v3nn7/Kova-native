//! Single-line editable text.

use crate::context::{EventCx, MeasureCx, PaintCx};
use crate::editor::TextEditor;
use crate::element::{Element, ElementBase, Interactive};
use crate::style::{BoxShadow, Styled};
use crate::theme::theme;
use kova_native_animation::{Easing, Transition};
use kova_native_core::{Bounds, Color, DurationExt, Instant, Point, SharedString, Signal, Size};
use kova_native_input::{ImeEvent, InputEvent, Key, KeyDownEvent, MouseButton, NamedKey};
use kova_native_layout::MeasureInput;
use kova_native_text::{TextAlign, TextLayout, TextStyle, TextSystem, TextWrap};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;
use unicode_segmentation::UnicodeSegmentation;

const BULLET: &str = "\u{2022}";
const BLINK: Duration = Duration::from_millis(530);
const DEFAULT_WIDTH: f32 = 220.0;

type SubmitFn = Rc<dyn Fn(&str, &mut EventCx)>;

/// How a pointer drag extends the selection.
#[derive(Clone, Debug)]
enum DragUnit {
    Grapheme,
    /// Word selection started on this range by a double click.
    Word(std::ops::Range<usize>),
    All,
}

#[derive(Clone, Copy)]
struct Palette {
    caret: Color,
    selection: Color,
    placeholder: Color,
}

/// Editing, layout and scroll state shared between the element and its
/// drag handler.
struct InputState {
    editor: TextEditor,
    layout: TextLayout,
    placeholder_layout: TextLayout,
    placeholder: SharedString,
    style: Option<TextStyle>,
    masked: bool,
    /// IME composition shown at the caret: text and selected range within it.
    preedit: Option<(String, Option<(usize, usize)>)>,
    palette: Palette,
    /// Horizontal scroll of the text inside the content box.
    scroll_x: f32,
    /// Text origin relative to the element's border box (last paint).
    text_origin: Point,
    content_width: f32,
    blink_epoch: Instant,
    was_focused: bool,
    drag: DragUnit,
}

impl InputState {
    /// Text shown in the field: masked when secure, with the IME
    /// composition spliced in at the caret.
    fn display(&self) -> String {
        let text = self.editor.text();
        let cursor = self.editor.cursor();
        let preedit = self.preedit.as_ref().map_or("", |(p, _)| p.as_str());
        if self.masked {
            BULLET.repeat(text.graphemes(true).count() + preedit.graphemes(true).count())
        } else if preedit.is_empty() {
            text.to_string()
        } else {
            format!("{}{}{}", &text[..cursor], preedit, &text[cursor..])
        }
    }

    fn preedit_len(&self) -> usize {
        match &self.preedit {
            Some((p, _)) if self.masked => p.graphemes(true).count() * BULLET.len(),
            Some((p, _)) => p.len(),
            None => 0,
        }
    }

    /// Display offset of a text offset. The composition sits at the caret,
    /// so later offsets shift by its length.
    fn text_to_display(&self, offset: usize) -> usize {
        let text = self.editor.text();
        let base = if self.masked {
            text[..offset].graphemes(true).count() * BULLET.len()
        } else {
            offset
        };
        if offset > self.editor.cursor() {
            base + self.preedit_len()
        } else {
            base
        }
    }

    /// Text offset for a display offset.
    fn display_to_text(&self, display: usize) -> usize {
        let caret = self.text_to_display(self.editor.cursor());
        let plen = self.preedit_len();
        let display = if display <= caret {
            display
        } else if display < caret + plen {
            return self.editor.cursor();
        } else {
            display - plen
        };
        if self.masked {
            let n = display / BULLET.len();
            self.editor
                .text()
                .grapheme_indices(true)
                .nth(n)
                .map_or(self.editor.text().len(), |(i, _)| i)
        } else {
            self.editor.snap(display)
        }
    }

    /// Where the caret is drawn, honoring the IME's own caret.
    fn caret_display(&self) -> usize {
        let caret = self.text_to_display(self.editor.cursor());
        match &self.preedit {
            Some((p, cursor)) if !self.masked => caret + cursor.map_or(p.len(), |(_, end)| end),
            Some(_) => caret + self.preedit_len(),
            None => caret,
        }
    }

    fn sync_layout(&mut self, ts: &mut TextSystem) {
        let Some(style) = &self.style else { return };
        let display = self.display();
        self.layout.set(ts, &display, style);
        self.layout.layout(ts, None);
        if self.placeholder_layout.text() != self.placeholder.as_ref()
            || self.placeholder_layout.style() != Some(style)
        {
            self.placeholder_layout.set(ts, &self.placeholder, style);
            self.placeholder_layout.layout(ts, None);
        }
    }

    fn line_height(&self) -> f32 {
        self.style
            .as_ref()
            .map_or(20.0, |s| s.line_height.resolve(s.size))
    }

    /// Text offset under an element-local point.
    fn offset_at(&self, local: Point) -> usize {
        let x = local.x - self.text_origin.x + self.scroll_x;
        let y = self.line_height() / 2.0;
        self.display_to_text(self.layout.hit_test(Point::new(x.max(0.0), y)))
    }

    fn caret_x(&self) -> f32 {
        self.layout.caret_bounds(self.caret_display()).origin.x
    }

    /// Scrolls so the caret stays inside the visible width.
    fn reveal_caret(&mut self) {
        let width = self.content_width.max(1.0);
        let text_width = self.layout.size().width + 1.0;
        let caret = self.caret_x();
        if caret - self.scroll_x > width - 1.0 {
            self.scroll_x = caret - width + 1.0;
        }
        if caret < self.scroll_x {
            self.scroll_x = caret;
        }
        self.scroll_x = self.scroll_x.clamp(0.0, (text_width - width).max(0.0));
    }

    fn reset_blink(&mut self, now: Instant) {
        self.blink_epoch = now;
    }

    fn extend_drag(&mut self, offset: usize) {
        match self.drag.clone() {
            DragUnit::Grapheme => self.editor.move_to(offset, true),
            DragUnit::Word(origin) => {
                let word = self.editor.word_at(offset);
                if offset < origin.start {
                    self.editor.select(origin.end, word.start);
                } else {
                    self.editor.select(origin.start, word.end.max(origin.end));
                }
            }
            DragUnit::All => {}
        }
    }
}

/// A single-line text field bound to a `Signal<String>`.
///
/// Supports caret and selection by mouse (click, double-click word, triple
/// click all, drag) and keyboard (arrows, word jumps, Home/End, Shift
/// selection), clipboard, undo/redo, IME composition and masking for
/// passwords. Editing writes the signal; setting the signal elsewhere updates
/// the field.
///
/// ```ignore
/// let name = signal(String::new());
/// text_input(name).placeholder("Your name").on_submit(|text, _| println!("{text}"))
/// ```
pub struct TextInput {
    base: ElementBase,
    value: Signal<String>,
    state: Rc<RefCell<InputState>>,
    on_submit: Option<SubmitFn>,
}

/// Creates a text field editing `value`.
pub fn text_input(value: Signal<String>) -> TextInput {
    let t = theme();
    let state = Rc::new(RefCell::new(InputState {
        editor: TextEditor::new(value.get_untracked()),
        layout: TextLayout::new(),
        placeholder_layout: TextLayout::new(),
        placeholder: SharedString::default(),
        style: None,
        masked: false,
        preedit: None,
        palette: Palette {
            caret: t.accent,
            selection: t.accent.with_alpha(if t.dark { 0.38 } else { 0.26 }),
            placeholder: t.text_subtle,
        },
        scroll_x: 0.0,
        text_origin: Point::ZERO,
        content_width: 0.0,
        blink_epoch: Instant::now(),
        was_focused: false,
        drag: DragUnit::Grapheme,
    }));
    let drag_state = state.clone();
    let ring = BoxShadow::new(0.0, 0.0, t.accent.with_alpha(0.4)).spread(3.0);
    let (accent, hover_border) = (t.accent, t.border_strong);
    TextInput {
        base: ElementBase::new(),
        value,
        state,
        on_submit: None,
    }
    .px(12.0)
    .py(8.0)
    .bg(t.surface_sunken)
    .border(1.0)
    .border_color(t.border)
    .rounded(t.radius_small + 2.0)
    .text_size(t.font_size)
    .text_color(t.text)
    .cursor_text()
    .focusable()
    .hover(move |s| s.border_color(hover_border))
    .focus(move |s| s.border_color(accent).shadow(ring))
    .disabled_style(|s| s.opacity(0.5))
    .transition(Transition::new(140.ms()).easing(Easing::EaseOutCubic))
    .on_drag(move |e, cx| {
        let mut st = drag_state.borrow_mut();
        let offset = st.offset_at(cx.to_local(e.position));
        st.extend_drag(offset);
        st.reset_blink(cx.now());
        cx.repaint();
    })
}

impl TextInput {
    /// Hint shown while the field is empty.
    pub fn placeholder(self, text: impl Into<SharedString>) -> Self {
        self.state.borrow_mut().placeholder = text.into();
        self
    }

    /// Masks the text with bullets and disables copying it.
    pub fn password(self) -> Self {
        self.state.borrow_mut().masked = true;
        self
    }

    /// Limits the length in characters.
    pub fn max_chars(self, max: usize) -> Self {
        self.state.borrow_mut().editor.set_max_chars(Some(max));
        self
    }

    /// Called with the current text when Enter is pressed.
    pub fn on_submit(mut self, f: impl Fn(&str, &mut EventCx) + 'static) -> Self {
        self.on_submit = Some(Rc::new(f));
        self
    }

    /// Writes the edited text back to the bound signal.
    fn publish(&self) {
        let text = self.state.borrow().editor.text().to_string();
        if self.value.with_untracked(|v| *v != text) {
            self.value.set(text);
        }
    }

    /// Handles editing keys. Returns `(handled, text_changed)`.
    fn key_down(&mut self, e: &KeyDownEvent, cx: &mut EventCx) -> (bool, bool) {
        let m = e.keystroke.modifiers;
        let shift = m.shift;
        let word = if cfg!(target_os = "macos") {
            m.alt
        } else {
            m.control
        };
        // AltGr reports Ctrl+Alt; it types characters, not shortcuts.
        let shortcut = m.secondary() && !m.alt;
        let mut st = self.state.borrow_mut();
        let masked = st.masked;
        let ed = &mut st.editor;
        let changed = match &e.keystroke.key {
            Key::Named(NamedKey::ArrowLeft) => {
                ed.move_left(shift, word);
                false
            }
            Key::Named(NamedKey::ArrowRight) => {
                ed.move_right(shift, word);
                false
            }
            Key::Named(NamedKey::Home) => {
                ed.move_home(shift);
                false
            }
            Key::Named(NamedKey::End) => {
                ed.move_end(shift);
                false
            }
            Key::Named(NamedKey::Backspace) => ed.backspace(word),
            Key::Named(NamedKey::Delete) => ed.delete(word),
            Key::Named(NamedKey::Enter) => {
                let Some(submit) = self.on_submit.clone() else {
                    return (false, false);
                };
                let text = ed.text().to_string();
                drop(st);
                submit(&text, cx);
                return (true, false);
            }
            Key::Character(c) if shortcut => match c.as_str() {
                "a" => {
                    ed.select_all();
                    false
                }
                "c" | "x" if masked => return (true, false),
                "c" => {
                    if ed.has_selection() {
                        cx.write_clipboard(ed.selected_text());
                    }
                    false
                }
                "x" => {
                    if ed.has_selection() {
                        cx.write_clipboard(ed.selected_text());
                    }
                    ed.delete_selection()
                }
                "v" => match cx.read_clipboard() {
                    Some(text) => ed.insert(&text),
                    None => false,
                },
                "z" if shift => ed.redo(),
                "z" => ed.undo(),
                "y" if !cfg!(target_os = "macos") => ed.redo(),
                _ => return (false, false),
            },
            _ => return (false, false),
        };
        (true, changed)
    }
}

impl Element for TextInput {
    fn base(&self) -> &ElementBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut ElementBase {
        &mut self.base
    }

    fn name(&self) -> &'static str {
        "text_input"
    }

    fn is_measured(&self) -> bool {
        true
    }

    fn has_bindings(&self) -> bool {
        true
    }

    fn update_bindings(&mut self) -> bool {
        let state = &self.state;
        self.value.with(|v| {
            let mut st = state.borrow_mut();
            if st.editor.text() != v {
                st.editor.set_text(v);
            }
        });
        // The intrinsic size does not depend on the content.
        false
    }

    fn text_style_changed(&mut self, _cx: &mut MeasureCx, style: &TextStyle) -> bool {
        let mut style = style.clone();
        style.wrap = TextWrap::None;
        style.align = TextAlign::Left;
        let mut st = self.state.borrow_mut();
        let changed = st.style.as_ref().is_none_or(|s| !s.same_layout(&style));
        st.style = Some(style);
        changed
    }

    fn measure(&mut self, _cx: &mut MeasureCx, input: MeasureInput) -> Size {
        let height = self.state.borrow().line_height();
        Size::new(
            input.known_width.unwrap_or(DEFAULT_WIDTH),
            input.known_height.unwrap_or(height),
        )
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let now = cx.now();
        let focused = cx.is_focused();
        let bounds = cx.bounds();
        let content = cx.content_bounds();
        let color = cx.text_color();
        let mut st = self.state.borrow_mut();
        if focused && !st.was_focused {
            st.reset_blink(now);
        }
        if !focused {
            st.preedit = None;
        }
        st.was_focused = focused;
        st.sync_layout(cx.text_system());
        let line_height = st.line_height();
        let text_top = content.origin.y + ((content.height() - line_height) / 2.0).max(0.0);
        st.text_origin = Point::new(content.origin.x, text_top) - bounds.origin;
        st.content_width = content.width();
        st.reveal_caret();
        let origin = Point::new(content.origin.x - st.scroll_x, text_top);
        let palette = st.palette;

        // Leave room for the caret at the right edge.
        let clip = Bounds::new(
            Point::new(content.origin.x - 1.0, bounds.origin.y),
            Size::new(content.width() + 2.0, bounds.height()),
        );
        cx.with_clip(clip, kova_native_core::Corners::ZERO, |cx| {
            let ed = &st.editor;
            if focused && ed.has_selection() {
                let range = ed.selection();
                let (a, b) = (
                    st.text_to_display(range.start),
                    st.text_to_display(range.end),
                );
                for rect in st.layout.selection_bounds(a, b) {
                    cx.fill_rect(rect.translate(origin), palette.selection);
                }
            }
            if ed.text().is_empty() && st.preedit.is_none() {
                cx.paint_text(&st.placeholder_layout, origin, palette.placeholder);
            } else {
                cx.paint_text(&st.layout, origin, color);
            }
            if st.preedit.is_some() {
                let start = st.text_to_display(ed.cursor());
                for rect in st.layout.selection_bounds(start, start + st.preedit_len()) {
                    let underline = Bounds::new(
                        Point::new(rect.origin.x, rect.bottom() - 2.0),
                        Size::new(rect.width(), 1.0),
                    );
                    cx.fill_rect(underline.translate(origin), color);
                }
            }
            if focused {
                let elapsed = now.saturating_duration_since(st.blink_epoch);
                let phase = elapsed.as_millis() / BLINK.as_millis();
                if phase.is_multiple_of(2) {
                    let caret = st.layout.caret_bounds(st.caret_display());
                    let rect = Bounds::new(
                        Point::new(caret.origin.x, caret.origin.y + 1.0),
                        Size::new(1.5, (caret.height() - 2.0).max(1.0)),
                    );
                    cx.fill_rect(rect.translate(origin), palette.caret);
                }
                let next = st.blink_epoch + BLINK * (phase as u32 + 1);
                cx.request_frame_at(next);
            }
        });
    }

    fn handle_event(&mut self, cx: &mut EventCx, event: &InputEvent) {
        let now = cx.now();
        let changed = match event {
            InputEvent::MouseDown(e) if e.button == MouseButton::Left => {
                let mut st = self.state.borrow_mut();
                let offset = st.offset_at(cx.to_local(e.position));
                match e.click_count {
                    1 => {
                        st.editor.move_to(offset, e.modifiers.shift);
                        st.drag = DragUnit::Grapheme;
                    }
                    2 => {
                        let word = st.editor.word_at(offset);
                        st.editor.select(word.start, word.end);
                        st.drag = DragUnit::Word(word);
                    }
                    _ => {
                        st.editor.select_all();
                        st.drag = DragUnit::All;
                    }
                }
                false
            }
            InputEvent::KeyDown(e) => {
                let (handled, changed) = self.key_down(e, cx);
                if !handled {
                    return;
                }
                cx.prevent_default();
                cx.stop_propagation();
                changed
            }
            InputEvent::TextInput(text) => self.state.borrow_mut().editor.insert(text),
            InputEvent::Ime(ime) => {
                let mut st = self.state.borrow_mut();
                match ime {
                    ImeEvent::Preedit { text, cursor } if !text.is_empty() => {
                        // Composition replaces the selection.
                        let changed = st.editor.delete_selection();
                        st.preedit = Some((text.clone(), *cursor));
                        changed
                    }
                    ImeEvent::Commit(text) => {
                        st.preedit = None;
                        st.editor.insert(text)
                    }
                    _ => {
                        st.preedit = None;
                        false
                    }
                }
            }
            _ => return,
        };
        self.state.borrow_mut().reset_blink(now);
        cx.repaint();
        if changed {
            self.publish();
        }
    }

    fn accepts_text_input(&self) -> bool {
        true
    }

    fn ime_cursor_area(&self) -> Option<Bounds> {
        let st = self.state.borrow();
        let caret = st.layout.caret_bounds(st.caret_display());
        Some(Bounds::new(
            st.text_origin + Point::new(caret.origin.x - st.scroll_x, caret.origin.y),
            Size::new(1.0, caret.height()),
        ))
    }
}

crate::impl_element_builder!(TextInput);

#[cfg(test)]
mod tests;
