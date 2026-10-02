//! Overlay widgets: dialogs, drawers, popovers, menus, selects, tooltips,
//! toasts and the command palette. All of them render through the
//! window's overlay layer ([`portal`]), so they are never clipped by
//! scroll containers and always paint above the application.

use super::{button, focus_ring};
use crate::element::{AnyElement, Interactive, IntoElement};
use crate::elements::{
    Div, Placement, Region, Side, TextContent, column, div, dynamic, icon, keyed, portal, row, text,
};
use crate::icons;
use crate::style::{BoxShadow, Styled};
use crate::theme::theme;
use kova_native_animation::{Animation, Easing};
use kova_native_core::{
    Color, Duration, DurationExt, Owner, Point, SharedString, Signal, on_cleanup, signal, timer,
};
use kova_native_input::{Key, MouseButton, NamedKey};
use kova_native_text::FontWeight;
use std::cell::Cell;
use std::rc::Rc;

/// The shared look of floating panels (menus, popovers, dialogs).
pub fn overlay_panel() -> Div {
    let t = theme();
    column()
        .bg(t.surface)
        .border(1.0)
        .border_color(t.border_strong)
        .rounded(t.radius)
        .text_color(t.text)
        .shadow(BoxShadow::new(
            12.0,
            32.0,
            Color::BLACK.with_alpha(if t.dark { 0.45 } else { 0.16 }),
        ))
        .shadow(BoxShadow::new(1.0, 3.0, Color::BLACK.with_alpha(0.12)))
        .block_pointer()
}

fn enter_animation() -> Animation {
    Animation::new(150.ms()).easing(Easing::EaseOutCubic)
}

fn key_is(e: &kova_native_input::KeyDownEvent, key: NamedKey) -> bool {
    e.keystroke.key == Key::Named(key) && !e.keystroke.modifiers.any()
}

/// A square, borderless button showing only an icon. Give it an accessible
/// label through a surrounding [`tooltip`].
pub fn icon_button(svg: impl Into<crate::elements::SvgSource>) -> Div {
    icon_button_frame().child(icon(svg).size(16.0))
}

/// The interactive square of [`icon_button`], without content.
pub(crate) fn icon_button_frame() -> Div {
    let t = theme();
    let ring = focus_ring(&t);
    let (raised, sunken, text_c, muted) =
        (t.surface_raised, t.surface_sunken, t.text, t.text_muted);
    div()
        .size(28.0)
        .flex_shrink_0()
        .center()
        .rounded(t.radius_small)
        .text_color(muted)
        .cursor_pointer()
        .focusable()
        .focus_visible(move |s| s.shadow(ring))
        .hover(move |s| s.bg(raised).text_color(text_c))
        .active(move |s| s.bg(sunken))
        .disabled_style(|s| s.opacity(0.4))
        .transition(120.ms())
}

// ---- dialogs -----------------------------------------------------------------

type Build = Rc<dyn Fn() -> AnyElement>;

/// A modal dialog. Renders while `open` is true; Escape and a click on the
/// backdrop close it (unless [`Dialog::dismissible`] is turned off). Focus
/// moves into the dialog, Tab stays inside it, and focus returns to the
/// previously focused element when it closes.
///
/// ```ignore
/// dialog(open)
///     .title("Rename project")
///     .description("Names are visible to everyone in the workspace.")
///     .content(move || text_input(name).autofocus())
///     .footer(move || row().gap(8.0).child(button("Save").on_click(move |_| save())))
/// ```
pub struct Dialog {
    open: Signal<bool>,
    title: Option<SharedString>,
    description: Option<SharedString>,
    content: Option<Build>,
    footer: Option<Build>,
    width: f32,
    dismissible: bool,
}

/// Creates a [`Dialog`] shown while `open` is true.
pub fn dialog(open: Signal<bool>) -> Dialog {
    Dialog {
        open,
        title: None,
        description: None,
        content: None,
        footer: None,
        width: 460.0,
        dismissible: true,
    }
}

impl Dialog {
    pub fn title(mut self, title: impl Into<SharedString>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// The body, rebuilt each time the dialog opens. Scrolls when tall.
    pub fn content<E: IntoElement>(mut self, build: impl Fn() -> E + 'static) -> Self {
        self.content = Some(Rc::new(move || build().into_any()));
        self
    }

    /// Actions shown right-aligned at the bottom.
    pub fn footer<E: IntoElement>(mut self, build: impl Fn() -> E + 'static) -> Self {
        self.footer = Some(Rc::new(move || build().into_any()));
        self
    }

    /// Maximum width in logical px (default 460).
    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    /// Whether Escape, the close button and backdrop clicks close it (default true).
    pub fn dismissible(mut self, dismissible: bool) -> Self {
        self.dismissible = dismissible;
        self
    }

    fn panel(&self) -> Div {
        let t = theme();
        let open = self.open;
        let dismissible = self.dismissible;
        let mut header = row()
            .items_start()
            .gap(12.0)
            .px(20.0)
            .pt(18.0)
            .pb(4.0)
            .child(
                column()
                    .flex_1()
                    .min_w(0.0)
                    .gap(4.0)
                    .when_some(self.title.clone(), |c, title| {
                        c.child(text(title).size(15.5).semibold().letter_spacing(-0.2))
                    })
                    .when_some(self.description.clone(), |c, d| {
                        c.child(text(d).size(13.0).color(t.text_muted).line_height(1.45))
                    }),
            );
        if dismissible {
            header = header.child(
                icon_button(icons::CLOSE)
                    .id("dialog-close")
                    .tab_index(1)
                    .on_click(move |_| open.set(false)),
            );
        }
        let mut panel = overlay_panel()
            .w_full()
            .max_w(self.width)
            .max_h(kova_native_layout::relative(1.0))
            .trap_focus()
            .tab_index(-1)
            .autofocus()
            .on_key_down(move |e, cx| {
                if dismissible && key_is(e, NamedKey::Escape) {
                    open.set(false);
                    cx.stop_propagation();
                }
            })
            .animation(enter_animation(), |s, p| {
                s.opacity(p)
                    .scale(0.97 + 0.03 * p)
                    .translate_y(6.0 * (1.0 - p))
            })
            .child(header);
        if let Some(content) = &self.content {
            panel = panel.child(
                column()
                    .flex_1()
                    .min_h(0.0)
                    .overflow_y_scroll()
                    .px(20.0)
                    .py(14.0)
                    .gap(12.0)
                    .text_size(13.5)
                    .child(content()),
            );
        } else {
            panel = panel.child(div().h(14.0));
        }
        if let Some(footer) = &self.footer {
            panel = panel.child(
                row()
                    .justify_end()
                    .gap(8.0)
                    .px(20.0)
                    .py(12.0)
                    .border_t(1.0)
                    .border_color(t.border)
                    .bg(t.surface_sunken)
                    .corners((0.0, 0.0, t.radius, t.radius))
                    .child(footer()),
            );
        }
        panel
    }
}

/// A full-window layer with a dimmed backdrop and `panel` centered on it.
fn modal_layer(open: Signal<bool>, dismissible: bool, panel: Div) -> crate::elements::Portal {
    portal().child(
        div()
            .inset_0()
            .center()
            .padding(24.0)
            .child(
                div()
                    .inset_0()
                    .bg(Color::BLACK.with_alpha(if theme().dark { 0.6 } else { 0.38 }))
                    .on_click(move |_| {
                        if dismissible {
                            open.set(false)
                        }
                    })
                    .animation(enter_animation(), |s, p| s.opacity(p)),
            )
            .child(panel),
    )
}

impl IntoElement for Dialog {
    fn into_any(self) -> AnyElement {
        let open = self.open;
        let dismissible = self.dismissible;
        dynamic(move || {
            open.get()
                .then(|| modal_layer(open, dismissible, self.panel()))
        })
        .into_any()
    }
}

/// A confirmation dialog for destructive actions. Cancel is focused first;
/// `on_confirm` runs after the dialog closes.
pub fn confirm_dialog(
    open: Signal<bool>,
    title: impl Into<SharedString>,
    message: impl Into<SharedString>,
    confirm_label: impl Into<SharedString>,
    on_confirm: impl Fn() + 'static,
) -> Dialog {
    let on_confirm = Rc::new(on_confirm);
    let label: SharedString = confirm_label.into();
    dialog(open)
        .title(title)
        .description(message)
        .width(420.0)
        .footer(move || {
            let on_confirm = on_confirm.clone();
            row()
                .gap(8.0)
                .child(
                    button("Cancel")
                        .secondary()
                        .id("confirm-cancel")
                        .autofocus()
                        .on_click(move |_| open.set(false)),
                )
                .child(
                    button(label.clone())
                        .danger()
                        .id("confirm-accept")
                        .on_click(move |_| {
                            open.set(false);
                            on_confirm();
                        }),
                )
        })
}

/// A panel that slides in from the left or right edge while `open` is true.
/// Closes on Escape and backdrop clicks; traps focus like a dialog.
pub fn drawer<E: IntoElement>(
    open: Signal<bool>,
    side: Side,
    width: f32,
    content: impl Fn() -> E + 'static,
) -> Region {
    dynamic(move || {
        open.get().then(|| {
            let t = theme();
            let from = if side == Side::Left { -width } else { width };
            let mut panel = column()
                .absolute()
                .top(0.0)
                .bottom(0.0)
                .w(width)
                .max_w(kova_native_layout::relative(1.0))
                .bg(t.surface)
                .border_color(t.border_strong)
                .shadow(BoxShadow::new(0.0, 40.0, Color::BLACK.with_alpha(0.35)))
                .block_pointer()
                .trap_focus()
                .tab_index(-1)
                .autofocus()
                .on_key_down(move |e, cx| {
                    if key_is(e, NamedKey::Escape) {
                        open.set(false);
                        cx.stop_propagation();
                    }
                })
                .animation(
                    Animation::new(200.ms()).easing(Easing::EaseOutCubic),
                    move |s, p| s.translate_x(from * (1.0 - p)),
                )
                .child(content());
            panel = if side == Side::Left {
                panel.left(0.0).border_r(1.0)
            } else {
                panel.right(0.0).border_l(1.0)
            };
            portal().child(
                div()
                    .inset_0()
                    .child(
                        div()
                            .inset_0()
                            .bg(Color::BLACK.with_alpha(0.45))
                            .on_click(move |_| open.set(false))
                            .animation(enter_animation(), |s, p| s.opacity(p)),
                    )
                    .child(panel),
            )
        })
    })
}

// ---- popover -----------------------------------------------------------------

/// Floating content anchored to a trigger. Clicking the trigger toggles it;
/// clicking elsewhere or pressing Escape closes it.
pub struct Popover {
    open: Signal<bool>,
    trigger: AnyElement,
    content: Build,
    placement: Placement,
}

/// Creates a [`Popover`]. `open` can also be driven from elsewhere.
pub fn popover<E: IntoElement>(
    open: Signal<bool>,
    trigger: impl IntoElement,
    content: impl Fn() -> E + 'static,
) -> Popover {
    Popover {
        open,
        trigger: trigger.into_any(),
        content: Rc::new(move || content().into_any()),
        placement: Placement::BOTTOM_START,
    }
}

impl Popover {
    pub fn placement(mut self, placement: Placement) -> Self {
        self.placement = placement;
        self
    }
}

impl IntoElement for Popover {
    fn into_any(self) -> AnyElement {
        let Popover {
            open,
            trigger,
            content,
            placement,
        } = self;
        div()
            .on_click_outside(move |_| open.set(false))
            .on_key_down(move |e, cx| {
                if open.get_untracked() && key_is(e, NamedKey::Escape) {
                    open.set(false);
                    cx.stop_propagation();
                }
            })
            .child(div().on_click(move |_| open.toggle()).child(trigger))
            .child(dynamic(move || {
                open.get().then(|| {
                    portal().anchored().placement(placement).child(
                        overlay_panel()
                            .padding(12.0)
                            .animation(enter_animation(), |s, p| {
                                s.opacity(p).translate_y(-4.0 * (1.0 - p))
                            })
                            .child(content()),
                    )
                })
            }))
            .into_any()
    }
}

// ---- menus -------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum MenuKind {
    Item,
    Separator,
    Heading,
}

/// An entry of a [`dropdown_menu`], [`context_menu`] or [`select`].
#[derive(Clone)]
pub struct MenuItem {
    kind: MenuKind,
    label: SharedString,
    icon: Option<&'static [u8]>,
    shortcut: Option<SharedString>,
    danger: bool,
    disabled: bool,
    checked: Option<bool>,
    action: Option<Rc<dyn Fn()>>,
}

/// A selectable menu entry running `action` (after the menu closes).
pub fn menu_item(label: impl Into<SharedString>, action: impl Fn() + 'static) -> MenuItem {
    MenuItem {
        kind: MenuKind::Item,
        label: label.into(),
        icon: None,
        shortcut: None,
        danger: false,
        disabled: false,
        checked: None,
        action: Some(Rc::new(action)),
    }
}

/// A horizontal rule between groups of entries.
pub fn menu_separator() -> MenuItem {
    MenuItem {
        kind: MenuKind::Separator,
        ..menu_item("", || {})
    }
}

/// A small non-interactive group title.
pub fn menu_heading(label: impl Into<SharedString>) -> MenuItem {
    MenuItem {
        kind: MenuKind::Heading,
        ..menu_item(label, || {})
    }
}

impl MenuItem {
    /// A leading icon, e.g. `icons::TRASH`.
    pub fn icon(mut self, svg: &'static [u8]) -> Self {
        self.icon = Some(svg);
        self
    }

    /// A shortcut hint shown on the right (display only).
    pub fn shortcut(mut self, shortcut: impl Into<SharedString>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }

    /// Renders the entry in the danger color.
    pub fn danger(mut self) -> Self {
        self.danger = true;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Shows a check mark column; `true` draws the mark.
    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = Some(checked);
        self
    }

    fn selectable(&self) -> bool {
        self.kind == MenuKind::Item && !self.disabled
    }
}

/// The keyboard-navigable list used by all menus. `close` removes the menu.
fn menu_panel(items: Vec<MenuItem>, initial: Option<usize>, close: Rc<dyn Fn()>) -> Div {
    let t = theme();
    let items = Rc::new(items);
    let with_icons = items.iter().any(|i| i.icon.is_some());
    let with_checks = items.iter().any(|i| i.checked.is_some());
    let activate = {
        let (items, close) = (items.clone(), close.clone());
        Rc::new(move |index: usize| {
            let Some(item) = items.get(index).filter(|i| i.selectable()) else {
                return;
            };
            close();
            if let Some(action) = &item.action {
                action();
            }
        })
    };
    let step = {
        let items = items.clone();
        move |from: Option<usize>, forward: bool| -> Option<usize> {
            let n = items.len();
            if n == 0 {
                return None;
            }
            let mut i = from.unwrap_or(if forward { n - 1 } else { 0 });
            for _ in 0..n {
                i = if forward {
                    (i + 1) % n
                } else {
                    (i + n - 1) % n
                };
                if items[i].selectable() {
                    return Some(i);
                }
            }
            None
        }
    };
    // Start on `initial`, or the first selectable entry after it.
    let start = initial.and_then(|i| {
        if items.get(i).is_some_and(MenuItem::selectable) {
            Some(i)
        } else {
            step(Some(i), true).filter(|j| *j > i)
        }
    });
    let highlighted = signal(start);
    let key_activate = activate.clone();
    let (raised, muted, subtle, danger, border) = (
        t.surface_raised,
        t.text_muted,
        t.text_subtle,
        t.danger,
        t.border,
    );
    overlay_panel()
        .min_w(184.0)
        .max_h(320.0)
        .overflow_y_scroll()
        .padding(4.0)
        .text_size(13.0)
        .trap_focus()
        .tab_index(-1)
        .autofocus()
        .on_key_down(move |e, cx| {
            let key = &e.keystroke.key;
            let next = match key {
                Key::Named(NamedKey::ArrowDown) => step(highlighted.get_untracked(), true),
                Key::Named(NamedKey::ArrowUp) => step(highlighted.get_untracked(), false),
                Key::Named(NamedKey::Home) => step(None, true),
                Key::Named(NamedKey::End) => step(None, false),
                Key::Named(NamedKey::Enter) | Key::Named(NamedKey::Space) => {
                    if let Some(i) = highlighted.get_untracked() {
                        key_activate(i);
                    }
                    cx.stop_propagation();
                    return;
                }
                Key::Named(NamedKey::Escape) | Key::Named(NamedKey::Tab) => {
                    close();
                    cx.stop_propagation();
                    cx.prevent_default();
                    return;
                }
                _ => return,
            };
            if let Some(i) = next {
                highlighted.set(Some(i));
                cx.scroll_into_view(("menu-item", i));
            }
            cx.stop_propagation();
        })
        .animation(enter_animation(), |s, p| {
            s.opacity(p).translate_y(-4.0 * (1.0 - p))
        })
        .children(items.iter().enumerate().map(move |(i, item)| {
            match item.kind {
                MenuKind::Separator => div()
                    .h(1.0)
                    .my(4.0)
                    .mx(-4.0)
                    .bg(border)
                    .flex_shrink_0()
                    .into_any(),
                MenuKind::Heading => text(item.label.clone())
                    .size(11.0)
                    .semibold()
                    .letter_spacing(0.3)
                    .color(subtle)
                    .px(8.0)
                    .pt(6.0)
                    .pb(3.0)
                    .into_any(),
                MenuKind::Item => {
                    let activate = activate.clone();
                    let color = if item.danger { danger } else { t.text };
                    let mut entry = row()
                        .id(("menu-item", i))
                        .flex_shrink_0()
                        .gap(8.0)
                        .px(8.0)
                        .py(6.0)
                        .rounded(t.radius_small)
                        .text_color(color)
                        .bind(move |s| {
                            if highlighted.get() == Some(i) {
                                s.bg(raised)
                            } else {
                                s
                            }
                        });
                    if item.disabled {
                        entry = entry.opacity(0.42);
                    } else {
                        entry = entry
                            .cursor_pointer()
                            .on_hover(move |hovered, _| {
                                if hovered {
                                    highlighted.set(Some(i));
                                }
                            })
                            .on_click(move |_| activate(i));
                    }
                    if with_checks {
                        entry = entry.child(
                            icon(icons::CHECK)
                                .size(14.0)
                                .flex_shrink_0()
                                .opacity(if item.checked == Some(true) { 1.0 } else { 0.0 }),
                        );
                    }
                    if with_icons {
                        entry = entry.child(match item.icon {
                            Some(svg) => icon(svg)
                                .size(15.0)
                                .color(if item.danger { danger } else { muted })
                                .flex_shrink_0()
                                .into_any(),
                            None => div().size(15.0).flex_shrink_0().into_any(),
                        });
                    }
                    entry = entry.child(text(item.label.clone()).whitespace_nowrap().flex_1());
                    if let Some(shortcut) = &item.shortcut {
                        entry = entry.child(
                            text(shortcut.clone())
                                .size(11.5)
                                .color(subtle)
                                .whitespace_nowrap()
                                .ml(16.0),
                        );
                    }
                    entry.into_any()
                }
            }
        }))
}

/// A menu opened by clicking (or pressing ↓ on) `trigger`.
///
/// ```ignore
/// dropdown_menu(button("Actions").secondary(), move || vec![
///     menu_item("Rename", move || rename.set(true)).icon(icons::EDIT),
///     menu_separator(),
///     menu_item("Delete", move || confirm.set(true)).icon(icons::TRASH).danger(),
/// ])
/// ```
pub struct DropdownMenu {
    trigger: AnyElement,
    items: Rc<dyn Fn() -> Vec<MenuItem>>,
    placement: Placement,
}

/// Creates a [`DropdownMenu`]. `items` runs each time the menu opens.
pub fn dropdown_menu(
    trigger: impl IntoElement,
    items: impl Fn() -> Vec<MenuItem> + 'static,
) -> DropdownMenu {
    DropdownMenu {
        trigger: trigger.into_any(),
        items: Rc::new(items),
        placement: Placement::BOTTOM_START,
    }
}

impl DropdownMenu {
    pub fn placement(mut self, placement: Placement) -> Self {
        self.placement = placement;
        self
    }
}

impl IntoElement for DropdownMenu {
    fn into_any(self) -> AnyElement {
        let DropdownMenu {
            trigger,
            items,
            placement,
        } = self;
        let open = signal(false);
        let close: Rc<dyn Fn()> = Rc::new(move || open.set(false));
        div()
            .on_click_outside(move |_| open.set(false))
            .child(
                div()
                    .on_click(move |_| open.toggle())
                    .on_key_down(move |e, cx| {
                        if key_is(e, NamedKey::ArrowDown) && !open.get_untracked() {
                            open.set(true);
                            cx.stop_propagation();
                        }
                    })
                    .child(trigger),
            )
            .child(dynamic(move || {
                open.get().then(|| {
                    portal()
                        .anchored()
                        .placement(placement)
                        .gap(4.0)
                        .child(menu_panel(items(), Some(0), close.clone()))
                })
            }))
            .into_any()
    }
}

/// Wraps `target` so that a right click opens a menu at the pointer.
/// The returned container can be styled (e.g. `.flex_1()`).
pub fn context_menu(target: impl IntoElement, items: impl Fn() -> Vec<MenuItem> + 'static) -> Div {
    let at = signal(None::<Point>);
    let close: Rc<dyn Fn()> = Rc::new(move || at.set(None));
    div()
        .on_mouse_down(move |e, _| {
            if e.button == MouseButton::Right {
                at.set(Some(e.position));
            }
        })
        .child(target)
        .child(dynamic(move || {
            at.get().map(|point| {
                let dismiss = close.clone();
                portal()
                    .at(point)
                    .gap(2.0)
                    .placement(Placement::BOTTOM_START)
                    .child(
                        menu_panel(items(), None, close.clone())
                            .on_click_outside(move |_| dismiss()),
                    )
            })
        }))
}

// ---- select ------------------------------------------------------------------

/// A dropdown list for choosing one option by index. See [`select`].
pub struct Select {
    options: Rc<Vec<SharedString>>,
    selected: Signal<usize>,
    placeholder: SharedString,
    width: Option<f32>,
    id: Option<kova_native_core::ElementId>,
}

/// Creates a [`Select`] bound to an index into `options`. An out-of-range
/// index shows the placeholder.
pub fn select<S: Into<SharedString>>(
    options: impl IntoIterator<Item = S>,
    selected: Signal<usize>,
) -> Select {
    Select {
        options: Rc::new(options.into_iter().map(Into::into).collect()),
        selected,
        placeholder: "Select…".into(),
        width: None,
        id: None,
    }
}

impl Select {
    pub fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    /// Fixed trigger width in logical px (default: fill / content width).
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// Element id of the trigger (focus it, find it in tests).
    pub fn id(mut self, id: impl Into<kova_native_core::ElementId>) -> Self {
        self.id = Some(id.into());
        self
    }
}

impl IntoElement for Select {
    fn into_any(self) -> AnyElement {
        let Select {
            options,
            selected,
            placeholder,
            width,
            id,
        } = self;
        let t = theme();
        let open = signal(false);
        let ring = BoxShadow::new(0.0, 0.0, t.accent.with_alpha(0.4)).spread(3.0);
        let (accent, hover_border, subtle) = (t.accent, t.border_strong, t.text_subtle);
        let label_options = options.clone();
        let mut trigger = row()
            .justify_between()
            .gap(8.0)
            .px(10.0)
            .py(7.0)
            .bg(t.surface_sunken)
            .border(1.0)
            .border_color(t.border)
            .rounded(t.radius_small + 2.0)
            .text_size(t.font_size)
            .cursor_pointer()
            .focusable()
            .hover(move |s| s.border_color(hover_border))
            .focus(move |s| s.border_color(accent).shadow(ring))
            .bind(move |s| {
                if open.get() {
                    s.border_color(accent)
                } else {
                    s
                }
            })
            .transition(120.ms())
            .on_click(move |_| open.toggle())
            .on_key_down(move |e, cx| {
                let opens = [
                    NamedKey::ArrowDown,
                    NamedKey::ArrowUp,
                    NamedKey::Enter,
                    NamedKey::Space,
                ]
                .into_iter()
                .any(|k| key_is(e, k));
                if opens && !open.get_untracked() {
                    open.set(true);
                    cx.stop_propagation();
                }
            })
            .child(
                text(move || {
                    label_options
                        .get(selected.get())
                        .cloned()
                        .unwrap_or_else(|| placeholder.clone())
                })
                .whitespace_nowrap()
                .bind({
                    let options = options.clone();
                    move |s| {
                        if selected.get() < options.len() {
                            s
                        } else {
                            s.text_color(subtle)
                        }
                    }
                }),
            )
            .child(icon(icons::CHEVRONS_UP_DOWN).size(14.0).color(subtle));
        if let Some(width) = width {
            trigger = trigger.w(width);
        }
        if let Some(id) = id {
            trigger = trigger.id(id);
        }
        let close: Rc<dyn Fn()> = Rc::new(move || open.set(false));
        div()
            .flex_col()
            .on_click_outside(move |_| open.set(false))
            .child(trigger)
            .child(dynamic(move || {
                open.get().then(|| {
                    let current = selected.get_untracked();
                    let items = options
                        .iter()
                        .enumerate()
                        .map(|(i, label)| {
                            menu_item(label.clone(), move || selected.set(i)).checked(i == current)
                        })
                        .collect();
                    portal()
                        .anchored()
                        .gap(4.0)
                        .match_anchor_width()
                        .child(menu_panel(items, Some(current), close.clone()))
                })
            }))
            .into_any()
    }
}

// ---- tooltip -----------------------------------------------------------------

/// Delay before a tooltip appears.
pub const TOOLTIP_DELAY: Duration = Duration::from_millis(450);

/// Shows `tip` above `trigger` after the pointer rests on it.
pub fn tooltip(trigger: impl IntoElement, tip: impl Into<TextContent>) -> Div {
    let t = theme();
    let tip: TextContent = tip.into();
    let visible = signal(false);
    let pending: Rc<Cell<Option<timer::TimerId>>> = Rc::new(Cell::new(None));
    if Owner::current().is_some() {
        let pending = pending.clone();
        on_cleanup(move || {
            if let Some(id) = pending.take() {
                id.cancel();
            }
        });
    }
    let (bg, fg) = if t.dark {
        (
            kova_native_core::rgb(0xe9ebf0),
            kova_native_core::rgb(0x14161c),
        )
    } else {
        (
            kova_native_core::rgb(0x1c1f27),
            kova_native_core::rgb(0xf4f5f8),
        )
    };
    let radius = t.radius_small;
    let hide = {
        let pending = pending.clone();
        move || {
            if let Some(id) = pending.take() {
                id.cancel();
            }
            if visible.is_alive() {
                visible.set(false);
            }
        }
    };
    let hide_on_press = hide.clone();
    div()
        .on_hover(move |hovered, _| {
            if hovered {
                if let Some(id) = pending.take() {
                    id.cancel();
                }
                pending.set(Some(timer::set_timeout_unowned(TOOLTIP_DELAY, move || {
                    if visible.is_alive() {
                        visible.set(true);
                    }
                })));
            } else {
                hide();
            }
        })
        .capture_mouse_down(move |_, _| hide_on_press())
        .child(trigger)
        .child(dynamic(move || {
            let tip = tip.clone();
            visible.get().then(|| {
                portal()
                    .anchored()
                    .placement(Placement::TOP)
                    .gap(6.0)
                    .child(
                        row()
                            .px(8.0)
                            .py(4.0)
                            .rounded(radius)
                            .bg(bg)
                            .text_color(fg)
                            .text_size(11.5)
                            .font_weight(FontWeight::Medium)
                            .pointer_events_none()
                            .shadow(BoxShadow::new(4.0, 14.0, Color::BLACK.with_alpha(0.3)))
                            .animation(Animation::new(120.ms()), |s, p| s.opacity(p))
                            .child(text(tip).whitespace_nowrap()),
                    )
            })
        }))
}

// ---- toasts ------------------------------------------------------------------

/// Identifies a shown toast.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ToastId(u64);

#[derive(Clone)]
struct ToastEntry {
    id: u64,
    kind: super::AlertKind,
    title: SharedString,
    description: Option<SharedString>,
    action: Option<(SharedString, Rc<dyn Fn()>)>,
}

struct ToastStore {
    entries: Signal<Vec<ToastEntry>>,
    next: Cell<u64>,
}

thread_local! {
    static TOASTS: std::cell::OnceCell<ToastStore> = const { std::cell::OnceCell::new() };
}

fn with_toasts<R>(f: impl FnOnce(&ToastStore) -> R) -> R {
    TOASTS.with(|cell| {
        f(cell.get_or_init(|| ToastStore {
            // A dedicated root so no UI rebuild disposes the queue.
            entries: Owner::new_root().with(|| signal(Vec::new())),
            next: Cell::new(0),
        }))
    })
}

/// Most toasts kept on screen; older ones are dismissed first.
pub const MAX_TOASTS: usize = 4;

/// A notification shown by [`toaster`]. Build with [`toast`], then call
/// [`Toast::show`].
pub struct Toast {
    entry: ToastEntry,
    duration: Option<Duration>,
}

/// Starts a toast with a title. Toasts auto-dismiss after 4 seconds.
///
/// ```ignore
/// toast("Project created").success().description("kova-demo is ready").show();
/// ```
pub fn toast(title: impl Into<SharedString>) -> Toast {
    Toast {
        entry: ToastEntry {
            id: 0,
            kind: super::AlertKind::Info,
            title: title.into(),
            description: None,
            action: None,
        },
        duration: Some(Duration::from_secs(4)),
    }
}

impl Toast {
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.entry.description = Some(description.into());
        self
    }

    pub fn kind(mut self, kind: super::AlertKind) -> Self {
        self.entry.kind = kind;
        self
    }

    pub fn success(self) -> Self {
        self.kind(super::AlertKind::Success)
    }

    pub fn warning(self) -> Self {
        self.kind(super::AlertKind::Warning)
    }

    pub fn danger(self) -> Self {
        self.kind(super::AlertKind::Danger)
    }

    /// How long the toast stays (default 4 s).
    pub fn duration(mut self, duration: Duration) -> Self {
        self.duration = Some(duration);
        self
    }

    /// Keeps the toast until dismissed.
    pub fn persistent(mut self) -> Self {
        self.duration = None;
        self
    }

    /// Adds an action button; clicking it runs `f` and dismisses the toast.
    pub fn action(mut self, label: impl Into<SharedString>, f: impl Fn() + 'static) -> Self {
        self.entry.action = Some((label.into(), Rc::new(f)));
        self
    }

    /// Shows the toast in every mounted [`toaster`].
    pub fn show(self) -> ToastId {
        let Toast {
            mut entry,
            duration,
        } = self;
        let id = with_toasts(|store| {
            let id = store.next.get() + 1;
            store.next.set(id);
            entry.id = id;
            store.entries.update(|list| {
                list.push(entry);
                let excess = list.len().saturating_sub(MAX_TOASTS);
                list.drain(..excess);
            });
            id
        });
        if let Some(duration) = duration {
            timer::set_timeout_unowned(duration, move || dismiss_toast(ToastId(id)));
        }
        ToastId(id)
    }
}

/// Removes a toast (no effect if it is already gone).
pub fn dismiss_toast(id: ToastId) {
    with_toasts(|store| {
        if store
            .entries
            .with_untracked(|l| l.iter().any(|e| e.id == id.0))
        {
            store.entries.update(|l| l.retain(|e| e.id != id.0));
        }
    });
}

/// Number of toasts currently queued.
pub fn toast_count() -> usize {
    with_toasts(|store| store.entries.with_untracked(Vec::len))
}

/// Renders toasts in the bottom-right corner. Mount one per window, anywhere.
pub fn toaster() -> AnyElement {
    let entries = with_toasts(|store| store.entries);
    portal()
        .child(
            column()
                .right(16.0)
                .bottom(16.0)
                .w(340.0)
                .gap(8.0)
                .pointer_events_none()
                .child(keyed(move || entries.get(), |e| e.id, render_toast).gap(8.0)),
        )
        .into_any()
}

fn render_toast(entry: ToastEntry) -> Div {
    let t = theme();
    let color = match entry.kind {
        super::AlertKind::Info => t.accent,
        super::AlertKind::Success => t.success,
        super::AlertKind::Warning => t.warning,
        super::AlertKind::Danger => t.danger,
    };
    let glyph = match entry.kind {
        super::AlertKind::Info => icons::INFO,
        super::AlertKind::Success => icons::CHECK_CIRCLE,
        super::AlertKind::Warning => icons::ALERT_TRIANGLE,
        super::AlertKind::Danger => icons::ALERT_CIRCLE,
    };
    let id = ToastId(entry.id);
    let mut body = column()
        .flex_1()
        .min_w(0.0)
        .gap(2.0)
        .child(text(entry.title.clone()).size(13.0).semibold());
    if let Some(description) = &entry.description {
        body = body.child(text(description.clone()).size(12.5).color(t.text_muted));
    }
    if let Some((label, action)) = entry.action.clone() {
        body = body.child(
            text(label)
                .size(12.5)
                .semibold()
                .color(color)
                .mt(4.0)
                .cursor_pointer()
                .on_click(move |_| {
                    action();
                    dismiss_toast(id);
                }),
        );
    }
    overlay_panel()
        .id(("toast", entry.id))
        .flex_row()
        .items_start()
        .gap(10.0)
        .padding(12.0)
        .animation(
            Animation::new(220.ms()).easing(Easing::EaseOutCubic),
            |s, p| s.opacity(p).translate_y(10.0 * (1.0 - p)),
        )
        .child(icon(glyph).size(17.0).color(color).flex_shrink_0().mt(1.0))
        .child(body)
        .child(
            icon_button(icons::CLOSE)
                .size(22.0)
                .on_click(move |_| dismiss_toast(id)),
        )
}

// ---- command palette ---------------------------------------------------------

/// An entry of the [`command_palette`].
#[derive(Clone)]
pub struct PaletteCommand {
    title: SharedString,
    group: Option<SharedString>,
    shortcut: Option<SharedString>,
    icon: Option<&'static [u8]>,
    keywords: SharedString,
    action: Rc<dyn Fn()>,
}

/// A command running `action` (after the palette closes).
pub fn command(title: impl Into<SharedString>, action: impl Fn() + 'static) -> PaletteCommand {
    PaletteCommand {
        title: title.into(),
        group: None,
        shortcut: None,
        icon: None,
        keywords: SharedString::default(),
        action: Rc::new(action),
    }
}

impl PaletteCommand {
    /// A category shown next to the title ("Navigation", "Theme").
    pub fn group(mut self, group: impl Into<SharedString>) -> Self {
        self.group = Some(group.into());
        self
    }

    pub fn shortcut(mut self, shortcut: impl Into<SharedString>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }

    pub fn icon(mut self, svg: &'static [u8]) -> Self {
        self.icon = Some(svg);
        self
    }

    /// Extra words that match this command but are not displayed.
    pub fn keywords(mut self, keywords: impl Into<SharedString>) -> Self {
        self.keywords = keywords.into();
        self
    }
}

/// Scores how well `query` matches `candidate` as a case-insensitive
/// subsequence; `None` if it does not match. Higher is better: consecutive
/// characters, word starts and early matches score more.
pub fn fuzzy_score(query: &str, candidate: &str) -> Option<i32> {
    let query: Vec<char> = query
        .chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect();
    if query.is_empty() {
        return Some(0);
    }
    let chars: Vec<char> = candidate.chars().collect();
    let mut score = 0;
    let mut qi = 0;
    let mut previous: Option<usize> = None;
    for (i, c) in chars.iter().enumerate() {
        if qi == query.len() {
            break;
        }
        if c.to_lowercase().eq(std::iter::once(query[qi])) {
            let word_start = i == 0 || !chars[i - 1].is_alphanumeric();
            score += 1;
            if word_start {
                score += 8;
            }
            if previous == Some(i.wrapping_sub(1)) {
                score += 5;
            }
            if i < 4 {
                score += 4 - i as i32;
            }
            previous = Some(i);
            qi += 1;
        }
    }
    (qi == query.len()).then_some(score - chars.len() as i32 / 8)
}

fn rank(commands: &[PaletteCommand], query: &str) -> Vec<usize> {
    let mut scored: Vec<(i32, usize)> = commands
        .iter()
        .enumerate()
        .filter_map(|(i, c)| {
            let title = fuzzy_score(query, &c.title);
            let extra = fuzzy_score(
                query,
                &format!("{} {}", c.group.as_deref().unwrap_or(""), c.keywords),
            )
            .map(|s| s - 6);
            title.max(extra).map(|s| (s, i))
        })
        .collect();
    scored.sort_by_key(|(s, i)| (std::cmp::Reverse(*s), *i));
    scored.into_iter().map(|(_, i)| i).collect()
}

/// A searchable list of commands in a modal (bind it to e.g. Ctrl+K).
/// Type to filter, ↑/↓ to move, Enter to run, Escape to close.
pub fn command_palette(
    open: Signal<bool>,
    commands: impl Fn() -> Vec<PaletteCommand> + 'static,
) -> Region {
    let commands = Rc::new(commands);
    dynamic(move || {
        open.get().then(|| {
            let t = theme();
            let all = Rc::new(commands());
            let query = signal(String::new());
            let highlighted = signal(0usize);
            let ranked = {
                let all = all.clone();
                kova_native_core::memo(move || query.with(|q| rank(&all, q)))
            };
            // A new query starts at the best match.
            kova_native_core::effect(move || {
                query.with(|_| ());
                highlighted.set(0);
            });
            let run = {
                let all = all.clone();
                Rc::new(move |position: usize| {
                    let Some(&index) = ranked.get_untracked().get(position) else {
                        return;
                    };
                    open.set(false);
                    (all[index].action)();
                })
            };
            let key_run = run.clone();
            let (raised, muted, subtle, accent_soft) =
                (t.surface_raised, t.text_muted, t.text_subtle, t.accent_soft);
            let item_radius = t.radius_small;
            let panel = overlay_panel()
                .w_full()
                .max_w(580.0)
                .trap_focus()
                .capture_key_down(move |e, cx| {
                    let count = ranked.with_untracked(Vec::len);
                    let current = highlighted.get_untracked();
                    let next = match &e.keystroke.key {
                        Key::Named(NamedKey::ArrowDown) => {
                            (current + 1).min(count.saturating_sub(1))
                        }
                        Key::Named(NamedKey::ArrowUp) => current.saturating_sub(1),
                        Key::Named(NamedKey::Enter) => {
                            key_run(current);
                            cx.prevent_default();
                            cx.stop_propagation();
                            return;
                        }
                        Key::Named(NamedKey::Escape) => {
                            open.set(false);
                            cx.stop_propagation();
                            return;
                        }
                        _ => return,
                    };
                    highlighted.set(next);
                    cx.scroll_into_view(("palette-item", next));
                    cx.prevent_default();
                    cx.stop_propagation();
                })
                .animation(enter_animation(), |s, p| {
                    s.opacity(p).scale(0.98 + 0.02 * p)
                })
                .child(
                    row()
                        .gap(10.0)
                        .px(14.0)
                        .py(4.0)
                        .border_b(1.0)
                        .border_color(t.border)
                        .child(icon(icons::SEARCH).size(16.0).color(subtle))
                        .child(
                            super::text_input(query)
                                .id("palette-query")
                                .placeholder("Type a command or search…")
                                .autofocus()
                                .flex_1()
                                .bg(Color::TRANSPARENT)
                                .border(0.0)
                                .px(0.0)
                                .py(10.0)
                                .no_shadow()
                                .focus(|s| s.no_shadow()),
                        ),
                )
                .child(
                    dynamic(move || {
                        let order = ranked.get();
                        if order.is_empty() {
                            return text("No matching commands")
                                .size(13.0)
                                .color(muted)
                                .px(14.0)
                                .py(18.0)
                                .into_any();
                        }
                        let all = all.clone();
                        column()
                            .padding(6.0)
                            .children(order.into_iter().take(60).enumerate().map(|(pos, index)| {
                                let c = &all[index];
                                let run = run.clone();
                                let mut entry = row()
                                    .id(("palette-item", pos))
                                    .gap(10.0)
                                    .px(10.0)
                                    .py(7.0)
                                    .rounded(item_radius)
                                    .text_size(13.0)
                                    .cursor_pointer()
                                    .bind(move |s| {
                                        if highlighted.get() == pos {
                                            s.bg(accent_soft)
                                        } else {
                                            s
                                        }
                                    })
                                    .hover(move |s| s.bg(raised))
                                    .on_click(move |_| run(pos));
                                entry = entry.child(match c.icon {
                                    Some(svg) => icon(svg).size(15.0).color(muted).into_any(),
                                    None => div().size(15.0).into_any(),
                                });
                                entry =
                                    entry.child(text(c.title.clone()).flex_1().whitespace_nowrap());
                                if let Some(group) = &c.group {
                                    entry =
                                        entry.child(text(group.clone()).size(11.5).color(subtle));
                                }
                                if let Some(shortcut) = &c.shortcut {
                                    entry = entry.child(super::kbd(shortcut.clone()));
                                }
                                entry
                            }))
                            .into_any()
                    })
                    .flex_col()
                    .max_h(340.0)
                    .overflow_y_scroll(),
                )
                .child(
                    row()
                        .gap(14.0)
                        .px(14.0)
                        .py(8.0)
                        .border_t(1.0)
                        .border_color(t.border)
                        .text_size(11.5)
                        .text_color(subtle)
                        .child(
                            row()
                                .gap(5.0)
                                .child(super::kbd("↑↓"))
                                .child(text("navigate")),
                        )
                        .child(row().gap(5.0).child(super::kbd("↵")).child(text("run")))
                        .child(row().gap(5.0).child(super::kbd("esc")).child(text("close"))),
                );
            portal().child(
                div()
                    .inset_0()
                    .flex_col()
                    .items_center()
                    .pt(72.0)
                    .px(16.0)
                    .child(
                        div()
                            .inset_0()
                            .bg(Color::BLACK.with_alpha(if t.dark { 0.55 } else { 0.3 }))
                            .on_click(move |_| open.set(false))
                            .animation(enter_animation(), |s, p| s.opacity(p)),
                    )
                    .child(panel),
            )
        })
    })
}

#[cfg(test)]
mod tests {
    use super::fuzzy_score;

    #[test]
    fn fuzzy_prefers_word_starts_and_rejects_non_subsequences() {
        assert!(fuzzy_score("tt", "Toggle theme") > fuzzy_score("tt", "Settings"));
        assert!(
            fuzzy_score("new", "New project").unwrap() > fuzzy_score("new", "Renew token").unwrap()
        );
        assert_eq!(fuzzy_score("xyz", "Open settings"), None);
        assert_eq!(fuzzy_score("", "anything"), Some(0));
        assert!(fuzzy_score("OPS", "open project settings").is_some());
    }
}
