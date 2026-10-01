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

const CHEVRON_ICON: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M9 6l6 6-6 6" fill="none" stroke="#000" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"/></svg>"##;
const STAR_ICON: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M12 2.8l2.8 5.9 6.4.8-4.7 4.4 1.2 6.4L12 17.2l-5.7 3.1 1.2-6.4-4.7-4.4 6.4-.8z" fill="#000"/></svg>"##;
const INFO_ICON: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><circle cx="12" cy="12" r="9" fill="none" stroke="#000" stroke-width="2.2"/><path d="M12 11v6M12 7.2v.1" stroke="#000" stroke-width="2.4" stroke-linecap="round"/></svg>"##;
const WARNING_ICON: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M12 3.5L2.8 19.5h18.4z" fill="none" stroke="#000" stroke-width="2.2" stroke-linejoin="round"/><path d="M12 10v4.5M12 17.2v.1" stroke="#000" stroke-width="2.4" stroke-linecap="round"/></svg>"##;
const DANGER_ICON: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><circle cx="12" cy="12" r="9" fill="none" stroke="#000" stroke-width="2.2"/><path d="M9 9l6 6M15 9l-6 6" stroke="#000" stroke-width="2.4" stroke-linecap="round"/></svg>"##;

/// A vertical group of mutually exclusive options bound to an index signal.
/// Arrow keys move the selection while an option is focused.
pub fn radio_group(options: &[&'static str], selected: Signal<usize>) -> Div {
    let t = theme();
    let (accent, border, sunken, muted, text_c) = (
        t.accent,
        t.border_strong,
        t.surface_sunken,
        t.text_muted,
        t.text,
    );
    let ring = focus_ring(&t);
    let count = options.len();
    column()
        .gap(10.0)
        .children(options.iter().enumerate().map(move |(i, option)| {
            row()
                .gap(10.0)
                .cursor_pointer()
                .on_click(move |_| selected.set(i))
                .child(
                    div()
                        .size(18.0)
                        .flex_shrink_0()
                        .rounded_full()
                        .border(1.5)
                        .center()
                        .focusable()
                        .focus(move |s| s.shadow(ring))
                        .on_key_down(move |e, cx| {
                            use kova_native_input::{Key, NamedKey};
                            let next = match e.keystroke.key {
                                Key::Named(NamedKey::ArrowUp) | Key::Named(NamedKey::ArrowLeft) => {
                                    (selected.get() + count - 1) % count
                                }
                                Key::Named(NamedKey::ArrowDown)
                                | Key::Named(NamedKey::ArrowRight) => (selected.get() + 1) % count,
                                _ => return,
                            };
                            selected.set(next);
                            cx.stop_propagation();
                        })
                        .bind(move |s| {
                            if selected.get() == i {
                                s.bg(sunken).border_color(accent)
                            } else {
                                s.bg(sunken).border_color(border)
                            }
                        })
                        .transition(Transition::new(140.ms()))
                        .child(
                            div()
                                .size(8.0)
                                .rounded_full()
                                .bg(accent)
                                .bind(move |s| {
                                    if selected.get() == i {
                                        s.opacity(1.0).scale(1.0)
                                    } else {
                                        s.opacity(0.0).scale(0.3)
                                    }
                                })
                                .transition(Spring::snappy()),
                        ),
                )
                .child(
                    text(*option)
                        .bind(move |s| {
                            s.text_color(if selected.get() == i { text_c } else { muted })
                        })
                        .transition(Transition::new(140.ms())),
                )
        }))
}

/// A row of tabs with an underline that follows the selected tab.
pub fn tabs(options: &[&'static str], selected: Signal<usize>) -> Div {
    let t = theme();
    let (accent, text_c, muted) = (t.accent, t.text, t.text_muted);
    let ring = focus_ring(&t);
    row()
        .gap(4.0)
        .border_b(1.0)
        .border_color(t.border)
        .children(options.iter().enumerate().map(move |(i, option)| {
            column()
                .items_center()
                .gap(8.0)
                .pt(6.0)
                .px(12.0)
                .cursor_pointer()
                .focusable()
                .rounded(t.radius_small)
                .focus(move |s| s.shadow(ring))
                .on_click(move |_| selected.set(i))
                .bind(move |s| s.text_color(if selected.get() == i { text_c } else { muted }))
                .hover(move |s| s.text_color(text_c))
                .transition(Transition::new(160.ms()))
                .child(
                    text(*option)
                        .text_size(13.5)
                        .font_weight(FontWeight::Medium),
                )
                .child(
                    div()
                        .h(2.0)
                        .w_full()
                        .rounded_full()
                        .bg(accent)
                        .bind(move |s| {
                            if selected.get() == i {
                                s.opacity(1.0).scale(1.0)
                            } else {
                                s.opacity(0.0).scale(0.4)
                            }
                        })
                        .transition(Spring::snappy()),
                )
        }))
}

/// A toggleable pill, e.g. for filters or tags.
pub fn chip(label: impl Into<TextContent>, selected: Signal<bool>) -> Div {
    let t = theme();
    let (accent, soft, border, text_c, muted, raised) = (
        t.accent,
        t.accent_soft,
        t.border,
        t.text,
        t.text_muted,
        t.surface_raised,
    );
    let ring = focus_ring(&t);
    row()
        .px(12.0)
        .py(5.0)
        .rounded_full()
        .border(1.0)
        .text_size(12.5)
        .font_weight(FontWeight::Medium)
        .cursor_pointer()
        .focusable()
        .focus(move |s| s.shadow(ring))
        .bind(move |s| {
            if selected.get() {
                s.bg(soft).border_color(accent).text_color(text_c)
            } else {
                s.bg(raised).border_color(border).text_color(muted)
            }
        })
        .hover(move |s| s.text_color(text_c))
        .active(|s| s.scale(0.96))
        .transition(Transition::new(150.ms()))
        .on_click(move |_| selected.toggle())
        .child(
            icon(CHECK_ICON)
                .size(12.0)
                .bind(move |s| {
                    if selected.get() {
                        s.opacity(1.0).scale(1.0).mr(6.0)
                    } else {
                        s.opacity(0.0).scale(0.4).w(0.0)
                    }
                })
                .transition(Spring::snappy()),
        )
        .child(text(label).whitespace_nowrap())
}

/// A round avatar showing initials over a gradient picked from the text.
pub fn avatar(initials: &str) -> Div {
    const PALETTE: [(u32, u32); 6] = [
        (0x8b5cf6, 0x6366f1),
        (0x06b6d4, 0x3b82f6),
        (0x10b981, 0x0d9488),
        (0xf59e0b, 0xef4444),
        (0xec4899, 0x8b5cf6),
        (0x64748b, 0x334155),
    ];
    let hash = initials
        .bytes()
        .fold(0usize, |h, b| h.wrapping_mul(31).wrapping_add(b as usize));
    let (a, b) = PALETTE[hash % PALETTE.len()];
    let label: String = initials.chars().take(2).collect::<String>().to_uppercase();
    div()
        .size(36.0)
        .flex_shrink_0()
        .center()
        .rounded_full()
        .bg(kova_native_core::linear_gradient(
            135.0,
            kova_native_core::rgb(a),
            kova_native_core::rgb(b),
        ))
        .border(2.0)
        .border_color(theme().surface)
        .text_color(Color::WHITE)
        .text_size(12.0)
        .font_weight(FontWeight::Semibold)
        .child(text(label).whitespace_nowrap())
}

/// Severity of an [`alert`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AlertKind {
    #[default]
    Info,
    Success,
    Warning,
    Danger,
}

/// A highlighted message box with an icon, a title and a description.
pub fn alert(
    kind: AlertKind,
    title: impl Into<TextContent>,
    message: impl Into<TextContent>,
) -> Div {
    let t = theme();
    let (color, glyph) = match kind {
        AlertKind::Info => (t.accent, INFO_ICON),
        AlertKind::Success => (t.success, CHECK_ICON),
        AlertKind::Warning => (t.warning, WARNING_ICON),
        AlertKind::Danger => (t.danger, DANGER_ICON),
    };
    row()
        .items_start()
        .gap(12.0)
        .padding(14.0)
        .rounded(t.radius)
        .bg(color.with_alpha(if t.dark { 0.10 } else { 0.08 }))
        .border(1.0)
        .border_color(color.with_alpha(0.35))
        .child(icon(glyph).size(18.0).color(color).flex_shrink_0())
        .child(
            column()
                .gap(3.0)
                .flex_1()
                .min_w(0.0)
                .child(text(title).semibold().color(if t.dark {
                    color.lighten(0.08)
                } else {
                    color.darken(0.08)
                }))
                .child(text(message).size(12.5).color(t.text_muted)),
        )
}

/// A numeric field with − and + buttons, clamped to `min..=max`.
pub fn stepper(value: Signal<i32>, min: i32, max: i32) -> Div {
    let t = theme();
    let ring = focus_ring(&t);
    let (raised, text_c, muted, radius) = (t.surface_raised, t.text, t.text_muted, t.radius_small);
    let step_button = move |glyph: &'static str, delta: i32| {
        div()
            .size(30.0)
            .center()
            .rounded(radius)
            .text_size(16.0)
            .font_weight(FontWeight::Medium)
            .cursor_pointer()
            .text_color(muted)
            .hover(move |s| s.bg(raised).text_color(text_c))
            .active(|s| s.scale(0.92))
            .bind(move |s| {
                let v = value.get();
                s.opacity(if (delta < 0 && v <= min) || (delta > 0 && v >= max) {
                    0.35
                } else {
                    1.0
                })
            })
            .transition(Transition::new(120.ms()))
            .on_click(move |_| value.update(|v| *v = (*v + delta).clamp(min, max)))
            .child(text(glyph))
    };
    row()
        .gap(2.0)
        .padding(3.0)
        .rounded(t.radius)
        .bg(t.surface_sunken)
        .border(1.0)
        .border_color(t.border)
        .focusable()
        .focus(move |s| s.shadow(ring))
        .on_key_down(move |e, cx| {
            use kova_native_input::{Key, NamedKey};
            let delta = match e.keystroke.key {
                Key::Named(NamedKey::ArrowDown) | Key::Named(NamedKey::ArrowLeft) => -1,
                Key::Named(NamedKey::ArrowUp) | Key::Named(NamedKey::ArrowRight) => 1,
                _ => return,
            };
            value.update(|v| *v = (*v + delta).clamp(min, max));
            cx.stop_propagation();
        })
        .child(step_button("−", -1))
        .child(
            text(move || value.get().to_string())
                .min_w(36.0)
                .text_center()
                .medium(),
        )
        .child(step_button("+", 1))
}

/// A collapsible section. `content` is built only while `open` is true.
pub fn accordion<E: IntoElement>(
    title: impl Into<TextContent>,
    open: Signal<bool>,
    content: impl Fn() -> E + 'static,
) -> Div {
    let t = theme();
    let (muted, raised) = (t.text_muted, t.surface_raised);
    let ring = focus_ring(&t);
    column()
        .w_full()
        .rounded(t.radius)
        .border(1.0)
        .border_color(t.border)
        .overflow_hidden()
        .child(
            row()
                .w_full()
                .justify_between()
                .px(14.0)
                .py(11.0)
                .cursor_pointer()
                .focusable()
                .focus(move |s| s.shadow(ring))
                .hover(move |s| s.bg(raised))
                .transition(Transition::new(120.ms()))
                .on_click(move |_| open.toggle())
                .child(text(title).medium())
                .child(
                    icon(CHEVRON_ICON)
                        .size(16.0)
                        .color(muted)
                        .bind(move |s| if open.get() { s.rotate(90.0) } else { s })
                        .transition(Spring::snappy()),
                ),
        )
        .child(
            dynamic_section(move || {
                open.get().then(|| {
                    column()
                        .w_full()
                        .px(14.0)
                        .pb(14.0)
                        .pt(2.0)
                        .text_color(muted)
                        .text_size(13.0)
                        .child(content())
                })
            })
            .w_full(),
        )
}

fn dynamic_section(build: impl Fn() -> Option<Div> + 'static) -> crate::elements::Region {
    crate::elements::Region::new(move || build().into_iter().map(IntoElement::into_any).collect())
}

/// Wraps `trigger` so that hovering it shows a small label above it.
pub fn tooltip(trigger: impl IntoElement, tip: impl Into<TextContent>) -> Div {
    let t = theme();
    let visible = kova_native_core::signal(false);
    let (bg, fg) = if t.dark {
        (rgb_hex(0xf4f5f8), rgb_hex(0x14161c))
    } else {
        (rgb_hex(0x1c1f27), rgb_hex(0xf4f5f8))
    };
    div()
        .relative()
        .on_hover(move |hovered, _| visible.set(hovered))
        .child(trigger)
        .child(
            row()
                .absolute()
                .bottom(relative(1.0))
                .left(0.0)
                .px(9.0)
                .py(5.0)
                .rounded(t.radius_small)
                .bg(bg)
                .text_color(fg)
                .text_size(11.5)
                .font_weight(FontWeight::Medium)
                .pointer_events_none()
                .shadow(BoxShadow::new(4.0, 14.0, Color::BLACK.with_alpha(0.3)))
                .bind(move |s| {
                    if visible.get() {
                        s.opacity(1.0).translate_y(-8.0)
                    } else {
                        s.opacity(0.0).translate_y(-4.0)
                    }
                })
                .transition(Transition::new(140.ms()).easing(Easing::EaseOutCubic))
                .child(text(tip).whitespace_nowrap()),
        )
}

fn rgb_hex(hex: u32) -> Color {
    kova_native_core::rgb(hex)
}

/// A star rating bound to a `0..=max` signal. Clicking the current value clears it.
pub fn rating(value: Signal<u8>, max: u8) -> Div {
    let t = theme();
    let (on, off) = (t.warning, t.border_strong);
    row().gap(3.0).children((1..=max).map(move |i| {
        div()
            .cursor_pointer()
            .bind(move |s| s.text_color(if value.get() >= i { on } else { off }))
            .hover(|s| s.scale(1.18))
            .transition(Spring::snappy())
            .on_click(move |_| value.update(|v| *v = if *v == i { 0 } else { i }))
            .child(icon(STAR_ICON).size(20.0))
    }))
}

/// A keyboard key cap, e.g. `kbd("Ctrl")`.
pub fn kbd(label: impl Into<TextContent>) -> Div {
    let t = theme();
    row()
        .px(7.0)
        .py(2.0)
        .rounded(5.0)
        .bg(t.surface_raised)
        .border(1.0)
        .border_color(t.border_strong)
        .border_b(2.0)
        .text_color(t.text_muted)
        .text_size(11.5)
        .font_weight(FontWeight::Medium)
        .child(text(label).whitespace_nowrap())
}

/// A pulsing placeholder block shown while content loads.
pub fn skeleton() -> Div {
    let t = theme();
    div()
        .h(12.0)
        .w_full()
        .rounded(t.radius_small)
        .bg(t.surface_raised.mix(t.border, 0.6))
        .animation(
            Animation::new(900.ms())
                .repeat()
                .alternate()
                .easing(Easing::EaseInOutCubic),
            |s, p| s.opacity(1.0 - 0.55 * p),
        )
}
