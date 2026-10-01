//! Ready-made widgets built from the primitive elements.

use crate::element::{AnyElement, Element, ElementBase, Interactive, IntoElement, StyleFn};
use crate::elements::{Div, Svg, TextContent, column, div, icon, row, text};
use crate::style::{BoxShadow, Style, Styled};
use crate::theme::{Theme, theme};
use kova_native_animation::{Animation, Easing, Spring, Transition};
use kova_native_core::{Color, DurationExt, Signal};
use kova_native_layout::relative;
use kova_native_text::FontWeight;
use std::rc::Rc;

const CHECK_ICON: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M5 12.5l4.5 4.5L19 7.5" fill="none" stroke="#000" stroke-width="3.2" stroke-linecap="round" stroke-linejoin="round"/></svg>"##;

/// Visual style of a [`Button`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ButtonVariant {
    #[default]
    Primary,
    Secondary,
    Outline,
    Ghost,
    Danger,
}

/// A clickable button. Focusable, activates with Enter/Space, with hover,
/// press and focus feedback animated by default.
pub struct Button {
    div: Div,
}

fn focus_ring(t: &Theme) -> BoxShadow {
    BoxShadow::new(0.0, 0.0, t.accent.with_alpha(0.55)).spread(3.0)
}

fn variant_styles(t: &Theme, variant: ButtonVariant) -> (Style, StyleFn, StyleFn) {
    let (bg, fg, border, hover, active) = match variant {
        ButtonVariant::Primary => (
            t.accent,
            t.accent_text,
            Color::TRANSPARENT,
            t.accent_hover,
            t.accent_active,
        ),
        ButtonVariant::Secondary => (
            t.surface_raised,
            t.text,
            t.border,
            t.border,
            t.surface_sunken,
        ),
        ButtonVariant::Outline => (
            Color::TRANSPARENT,
            t.text,
            t.border_strong,
            t.surface_raised,
            t.surface_sunken,
        ),
        ButtonVariant::Ghost => (
            Color::TRANSPARENT,
            t.text_muted,
            Color::TRANSPARENT,
            t.surface_raised,
            t.surface_sunken,
        ),
        ButtonVariant::Danger => (
            t.danger,
            Color::WHITE,
            Color::TRANSPARENT,
            t.danger.lighten(0.06),
            t.danger.darken(0.06),
        ),
    };
    let text_hover = if variant == ButtonVariant::Ghost {
        t.text
    } else {
        fg
    };
    let mut base = Style::default();
    base.background = Some(bg.into());
    base.text.color = Some(fg);
    base.border_color = border;
    let hover: StyleFn = Rc::new(move |s: Style| s.bg(hover).text_color(text_hover));
    let active: StyleFn = Rc::new(move |s: Style| s.bg(active).scale(0.97));
    (base, hover, active)
}

/// Creates a primary button with a text label.
pub fn button(label: impl Into<TextContent>) -> Button {
    let t = theme();
    let (base, hover, active) = variant_styles(&t, ButtonVariant::Primary);
    let ring = focus_ring(&t);
    let mut div = row()
        .justify_center()
        .gap(8.0)
        .px(16.0)
        .py(9.0)
        .rounded(t.radius)
        .border(1.0)
        .font_weight(FontWeight::Medium)
        .text_size(t.font_size)
        .cursor_pointer()
        .focusable()
        .focus(move |s| s.shadow(ring))
        .disabled_style(|s| s.opacity(0.45))
        .transition(Transition::new(160.ms()).easing(Easing::EaseOutCubic))
        .child(text(label).whitespace_nowrap());
    {
        let s = div.style_mut();
        s.background = base.background;
        s.text.color = base.text.color;
        s.border_color = base.border_color;
    }
    let b = Interactive::base_mut(&mut div);
    b.hover_styles.insert(0, hover);
    b.active_styles.insert(0, active);
    Button { div }
}

impl Button {
    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        let t = theme();
        let (base, hover, active) = variant_styles(&t, variant);
        {
            let s = self.div.style_mut();
            s.background = base.background;
            s.text.color = base.text.color;
            s.border_color = base.border_color;
        }
        let b = Interactive::base_mut(&mut self.div);
        b.hover_styles[0] = hover;
        b.active_styles[0] = active;
        self
    }

    pub fn primary(self) -> Self {
        self.variant(ButtonVariant::Primary)
    }

    pub fn secondary(self) -> Self {
        self.variant(ButtonVariant::Secondary)
    }

    pub fn outline(self) -> Self {
        self.variant(ButtonVariant::Outline)
    }

    pub fn ghost(self) -> Self {
        self.variant(ButtonVariant::Ghost)
    }

    pub fn danger(self) -> Self {
        self.variant(ButtonVariant::Danger)
    }

    /// Compact size.
    pub fn small(self) -> Self {
        let r = theme().radius_small;
        self.px(10.0).py(5.0).text_size(12.5).rounded(r)
    }

    /// Larger size.
    pub fn large(self) -> Self {
        self.px(22.0).py(12.0).text_size(15.0)
    }

    /// Adds a leading icon (tinted with the label color).
    pub fn icon(mut self, svg: Svg) -> Self {
        let children = &mut Interactive::base_mut(&mut self.div).children;
        children.insert(0, svg.size(16.0).into_any());
        self
    }
}

impl Element for Button {
    fn base(&self) -> &ElementBase {
        self.div.base()
    }

    fn base_mut(&mut self) -> &mut ElementBase {
        Element::base_mut(&mut self.div)
    }

    fn name(&self) -> &'static str {
        "button"
    }

    fn take_children(&mut self) -> Vec<AnyElement> {
        self.div.take_children()
    }
}

crate::impl_element_builder!(Button);

/// A raised panel.
pub fn card() -> Div {
    let t = theme();
    column()
        .bg(t.surface)
        .border(1.0)
        .border_color(t.border)
        .rounded(t.radius_large)
        .padding(20.0)
        .gap(12.0)
        .shadow(BoxShadow::new(1.0, 2.0, Color::BLACK.with_alpha(0.12)))
        .shadow(BoxShadow::new(
            8.0,
            24.0,
            Color::BLACK.with_alpha(if t.dark { 0.28 } else { 0.06 }),
        ))
}

/// A small pill shaped label.
pub fn badge(label: impl Into<TextContent>) -> Div {
    let t = theme();
    row()
        .px(8.0)
        .py(2.0)
        .rounded_full()
        .bg(t.accent_soft)
        .text_color(t.accent_hover)
        .text_size(11.5)
        .font_weight(FontWeight::Semibold)
        .child(text(label).whitespace_nowrap())
}

/// A thin horizontal separator.
pub fn divider() -> Div {
    div().h(1.0).w_full().flex_shrink_0().bg(theme().border)
}

/// A muted secondary label.
pub fn label(content: impl Into<TextContent>) -> crate::elements::Text {
    text(content).color(theme().text_muted)
}

/// A large title.
pub fn heading(content: impl Into<TextContent>) -> crate::elements::Text {
    text(content)
        .size(22.0)
        .weight(FontWeight::Semibold)
        .letter_spacing(-0.3)
}

/// An on/off toggle bound to a signal, with a spring animated knob.
pub fn switch(value: Signal<bool>) -> Div {
    let t = theme();
    let (on, off) = (t.accent, t.border_strong);
    let ring = focus_ring(&t);
    div()
        .w(40.0)
        .h(22.0)
        .flex_shrink_0()
        .rounded_full()
        .padding(3.0)
        .cursor_pointer()
        .focusable()
        .focus(move |s| s.shadow(ring))
        .bind(move |s| s.bg(if value.get() { on } else { off }))
        .transition(Transition::new(180.ms()))
        .on_click(move |_| value.toggle())
        .child(
            div()
                .size(16.0)
                .rounded_full()
                .bg(Color::WHITE)
                .shadow(BoxShadow::new(1.0, 3.0, Color::BLACK.with_alpha(0.3)))
                .bind(move |s| if value.get() { s.translate_x(18.0) } else { s })
                .transition(Spring::snappy()),
        )
}

/// A checkbox bound to a signal.
pub fn checkbox(value: Signal<bool>) -> Div {
    let t = theme();
    let (accent, border, sunken) = (t.accent, t.border_strong, t.surface_sunken);
    let ring = focus_ring(&t);
    div()
        .size(18.0)
        .flex_shrink_0()
        .rounded(5.0)
        .border(1.5)
        .center()
        .cursor_pointer()
        .focusable()
        .focus(move |s| s.shadow(ring))
        .bind(move |s| {
            if value.get() {
                s.bg(accent).border_color(accent)
            } else {
                s.bg(sunken).border_color(border)
            }
        })
        .transition(Transition::new(140.ms()))
        .on_click(move |_| value.toggle())
        .child(
            icon(CHECK_ICON)
                .size(13.0)
                .color(Color::WHITE)
                .bind(move |s| {
                    if value.get() {
                        s.opacity(1.0).scale(1.0)
                    } else {
                        s.opacity(0.0).scale(0.5)
                    }
                })
                .transition(Spring::snappy()),
        )
}

/// A horizontal slider bound to a `0.0..=1.0` signal.
pub fn slider(value: Signal<f32>) -> Div {
    let t = theme();
    let (accent, track) = (t.accent, t.border_strong);
    let ring = focus_ring(&t);
    let set_from = move |position: kova_native_core::Point, cx: &crate::context::EventCx| {
        let b = cx.bounds();
        if b.width() > 0.0 {
            value.set((cx.to_local(position).x / b.width()).clamp(0.0, 1.0));
        }
    };
    div()
        .relative()
        .h(20.0)
        .min_w(80.0)
        .items_center()
        .cursor_pointer()
        .focusable()
        .on_mouse_down(move |e, cx| set_from(e.position, cx))
        .on_drag(move |e, cx| set_from(e.position, cx))
        .on_key_down(move |e, cx| {
            use kova_native_input::{Key, NamedKey};
            let step = match e.keystroke.key {
                Key::Named(NamedKey::ArrowLeft) | Key::Named(NamedKey::ArrowDown) => -0.05,
                Key::Named(NamedKey::ArrowRight) | Key::Named(NamedKey::ArrowUp) => 0.05,
                _ => return,
            };
            value.update(|v| *v = (*v + step).clamp(0.0, 1.0));
            cx.stop_propagation();
        })
        .child(
            div().w_full().h(4.0).rounded_full().bg(track).child(
                div()
                    .h_full()
                    .rounded_full()
                    .bg(accent)
                    .bind(move |s| s.w(relative(value.get()))),
            ),
        )
        .child(
            div()
                .absolute()
                .size(16.0)
                .rounded_full()
                .bg(Color::WHITE)
                .shadow(BoxShadow::new(1.0, 4.0, Color::BLACK.with_alpha(0.35)))
                .bind(move |s| s.left(relative(value.get())).translate_x(-8.0))
                .hover(move |s| {
                    s.scale(1.15)
                        .shadow(BoxShadow::new(0.0, 0.0, accent.with_alpha(0.35)).spread(5.0))
                })
                .focus(move |s| s.shadow(ring))
                .transition(Transition::new(120.ms())),
        )
}

/// A determinate progress bar; `value` returns `0.0..=1.0` and may read signals.
pub fn progress_bar(value: impl Fn() -> f32 + 'static) -> Div {
    let t = theme();
    let accent = t.accent;
    let value = Rc::new(value);
    div()
        .w_full()
        .h(6.0)
        .rounded_full()
        .bg(t.border)
        .overflow_hidden()
        .child(
            div()
                .h_full()
                .rounded_full()
                .bg(kova_native_core::linear_gradient(
                    90.0,
                    accent,
                    t.accent_hover.lighten(0.08),
                ))
                .bind(move |s| s.w(relative(value().clamp(0.0, 1.0))))
                .transition(Transition::new(450.ms()).easing(Easing::EaseOutCubic)),
        )
}

/// Three pulsing dots.
pub fn spinner() -> Div {
    let t = theme();
    let color = t.accent;
    row().gap(5.0).children((0..3).map(move |i| {
        div().size(7.0).rounded_full().bg(color).animation(
            Animation::new(520.ms())
                .repeat()
                .alternate()
                .easing(Easing::EaseInOutCubic)
                .delay((i * 160).ms()),
            |s, p| s.opacity(0.25 + 0.75 * p).translate_y(-4.0 * p),
        )
    }))
}

/// A two-option segmented control bound to an index signal.
pub fn segmented(options: &[&'static str], selected: Signal<usize>) -> Div {
    let t = theme();
    let (surface, text_c, muted) = (t.surface_raised, t.text, t.text_muted);
    row()
        .padding(3.0)
        .gap(2.0)
        .rounded(t.radius)
        .bg(t.surface_sunken)
        .border(1.0)
        .border_color(t.border)
        .children(options.iter().enumerate().map(move |(i, option)| {
            row()
                .px(12.0)
                .py(5.0)
                .rounded(7.0)
                .text_size(13.0)
                .font_weight(FontWeight::Medium)
                .cursor_pointer()
                .bind(move |s| {
                    if selected.get() == i {
                        s.bg(surface).text_color(text_c).shadow(BoxShadow::new(
                            1.0,
                            3.0,
                            Color::BLACK.with_alpha(0.25),
                        ))
                    } else {
                        s.bg(Color::TRANSPARENT).text_color(muted)
                    }
                })
                .hover(move |s| s.text_color(text_c))
                .transition(Transition::new(160.ms()))
                .on_click(move |_| selected.set(i))
                .child(text(*option))
        }))
}
