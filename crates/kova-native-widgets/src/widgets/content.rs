//! Content widgets: code, copy buttons, empty states, search fields and
//! status labels.

use super::overlays::{icon_button, icon_button_frame};
use super::{AlertKind, TextInput, text_input};
use crate::element::{Interactive, IntoElement};
use crate::elements::{Div, column, div, dynamic, icon, row, stack, text};
use crate::icons;
use crate::style::Styled;
use crate::theme::theme;
use kova_native_core::{Duration, Owner, SharedString, Signal, on_cleanup, signal, timer};
use kova_native_text::{FontWeight, MONOSPACE};
use std::cell::Cell;
use std::rc::Rc;

/// How long copy buttons show their confirmation.
pub const COPIED_FEEDBACK: Duration = Duration::from_millis(1500);

/// An icon button that writes `content()` to the clipboard and briefly
/// shows a check mark.
pub fn copy_button(content: impl Fn() -> String + 'static) -> Div {
    let t = theme();
    let copied = signal(false);
    let pending: Rc<Cell<Option<timer::TimerId>>> = Rc::new(Cell::new(None));
    if Owner::current().is_some() {
        let pending = pending.clone();
        on_cleanup(move || {
            if let Some(id) = pending.take() {
                id.cancel();
            }
        });
    }
    let success = t.success;
    icon_button_frame()
        .on_click(move |cx| {
            cx.write_clipboard(&content());
            copied.set(true);
            if let Some(id) = pending.take() {
                id.cancel();
            }
            pending.set(Some(timer::set_timeout_unowned(
                COPIED_FEEDBACK,
                move || {
                    if copied.is_alive() {
                        copied.set(false);
                    }
                },
            )));
        })
        .child(dynamic(move || {
            if copied.get() {
                icon(icons::CHECK).size(16.0).color(success)
            } else {
                icon(icons::COPY).size(16.0)
            }
        }))
}

/// A block of monospace code with a header (language label and copy
/// button). Long lines scroll horizontally.
pub struct CodeBlock {
    code: SharedString,
    language: Option<SharedString>,
    line_numbers: bool,
    max_height: Option<f32>,
}

/// Creates a [`CodeBlock`].
pub fn code_block(code: impl Into<SharedString>) -> CodeBlock {
    CodeBlock {
        code: code.into(),
        language: None,
        line_numbers: false,
        max_height: None,
    }
}

impl CodeBlock {
    /// Label shown in the header ("rust", "bash"...).
    pub fn language(mut self, language: impl Into<SharedString>) -> Self {
        self.language = Some(language.into());
        self
    }

    pub fn line_numbers(mut self) -> Self {
        self.line_numbers = true;
        self
    }

    /// Scrolls vertically beyond this height.
    pub fn max_height(mut self, px: f32) -> Self {
        self.max_height = Some(px);
        self
    }
}

impl IntoElement for CodeBlock {
    fn into_any(self) -> crate::AnyElement {
        let t = theme();
        let code = self.code.trim_end_matches('\n').to_string();
        let copy_text = code.clone();
        let mut body = row()
            .items_start()
            .gap(14.0)
            .px(14.0)
            .py(12.0)
            .font_family(MONOSPACE)
            .text_size(12.5)
            .line_height(1.55)
            .overflow_x_scroll();
        if self.line_numbers {
            let count = code.lines().count().max(1);
            let numbers: Vec<String> = (1..=count).map(|n| n.to_string()).collect();
            body = body.child(
                text(numbers.join("\n"))
                    .color(t.text_subtle)
                    .text_right()
                    .whitespace_nowrap()
                    .flex_shrink_0(),
            );
        }
        body = body.child(text(code).whitespace_nowrap().flex_shrink_0());
        let body = match self.max_height {
            Some(h) => body.max_h(h).overflow_y_scroll(),
            None => body,
        };
        column()
            .w_full()
            .bg(t.surface_sunken)
            .border(1.0)
            .border_color(t.border)
            .rounded(t.radius)
            .overflow_hidden()
            .child(
                row()
                    .justify_between()
                    .pl(14.0)
                    .pr(6.0)
                    .py(4.0)
                    .border_b(1.0)
                    .border_color(t.border)
                    .child(
                        text(self.language.unwrap_or_else(|| "code".into()))
                            .size(11.5)
                            .weight(FontWeight::Medium)
                            .color(t.text_subtle),
                    )
                    .child(copy_button(move || copy_text.clone())),
            )
            .child(body)
            .into_any()
    }
}

/// Inline code: monospace text on a subtle background.
pub fn code(content: impl Into<SharedString>) -> Div {
    let t = theme();
    row()
        .px(5.0)
        .py(1.0)
        .rounded(4.0)
        .bg(t.surface_raised)
        .border(1.0)
        .border_color(t.border)
        .font_family(MONOSPACE)
        .text_size(12.0)
        .child(text(content.into()).whitespace_nowrap())
}

/// A centered placeholder for empty lists, missing results or errors.
/// Append actions with `.child(button(..))`.
pub fn empty_state(
    svg: &'static [u8],
    title: impl Into<SharedString>,
    description: impl Into<SharedString>,
) -> Div {
    let t = theme();
    column()
        .w_full()
        .items_center()
        .gap(6.0)
        .px(24.0)
        .py(36.0)
        .child(
            div()
                .size(40.0)
                .center()
                .rounded(t.radius)
                .bg(t.surface_raised)
                .border(1.0)
                .border_color(t.border)
                .mb(6.0)
                .child(icon(svg).size(20.0).color(t.text_muted)),
        )
        .child(text(title.into()).size(14.0).semibold())
        .child(
            text(description.into())
                .size(13.0)
                .color(t.text_muted)
                .text_center()
                .max_w(360.0),
        )
}

/// A text field with a search icon and a clear button.
pub fn search_input(value: Signal<String>) -> SearchInput {
    SearchInput {
        input: text_input(value).pl(32.0).pr(30.0).w_full(),
        value,
    }
}

/// See [`search_input`]; configure the inner field with [`SearchInput::input`].
pub struct SearchInput {
    input: TextInput,
    value: Signal<String>,
}

impl SearchInput {
    pub fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.input = self.input.placeholder(placeholder);
        self
    }

    /// Adjusts the inner [`TextInput`] (id, autofocus, submit handler...).
    pub fn input(mut self, f: impl FnOnce(TextInput) -> TextInput) -> Self {
        self.input = f(self.input);
        self
    }
}

impl IntoElement for SearchInput {
    fn into_any(self) -> crate::AnyElement {
        let t = theme();
        let value = self.value;
        stack()
            .w_full()
            .child(self.input)
            .child(
                icon(icons::SEARCH)
                    .size(15.0)
                    .color(t.text_subtle)
                    .self_center()
                    .justify_self_start()
                    .ml(10.0)
                    .pointer_events_none(),
            )
            .child(
                dynamic(move || {
                    (!value.with(String::is_empty)).then(|| {
                        icon_button(icons::CLOSE)
                            .size(22.0)
                            .mr(5.0)
                            .on_click(move |_| value.set(String::new()))
                    })
                })
                .self_center()
                .justify_self_end(),
            )
            .into_any()
    }
}

/// A compact colored label: a dot and text, for states such as
/// "Active", "Revoked" or "Pending".
pub fn status_badge(kind: AlertKind, label: impl Into<SharedString>) -> Div {
    let t = theme();
    let color = match kind {
        AlertKind::Info => t.accent,
        AlertKind::Success => t.success,
        AlertKind::Warning => t.warning,
        AlertKind::Danger => t.danger,
    };
    row()
        .gap(6.0)
        .px(8.0)
        .py(2.0)
        .rounded(t.radius_small)
        .bg(color.with_alpha(if t.dark { 0.12 } else { 0.10 }))
        .border(1.0)
        .border_color(color.with_alpha(0.28))
        .text_size(11.5)
        .font_weight(FontWeight::Medium)
        .text_color(if t.dark {
            color.lighten(0.1)
        } else {
            color.darken(0.12)
        })
        .child(div().size(6.0).rounded_full().bg(color))
        .child(text(label.into()).whitespace_nowrap())
}

/// A labelled metric: small caption, large value, optional detail line.
pub fn stat(label: impl Into<SharedString>, value: impl Into<crate::elements::TextContent>) -> Div {
    let t = theme();
    column()
        .gap(4.0)
        .child(
            text(label.into())
                .size(11.5)
                .weight(FontWeight::Medium)
                .color(t.text_muted),
        )
        .child(text(value).size(22.0).semibold().letter_spacing(-0.4))
}
