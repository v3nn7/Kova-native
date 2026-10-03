//! The element model.
//!
//! Builders like `column()`, `text(..)` or `button(..)` produce values that
//! implement [`Element`]. When mounted into a window, each element becomes a
//! retained node of the element tree; its children are mounted as child
//! nodes. From then on Kova Native never rebuilds the node: reactive bindings update
//! it in place, and only reactive regions (`dynamic`, views) replace their
//! own subtree.

use crate::context::{EventCx, MeasureCx, PaintCx};
use crate::style::{Style, Styled};
use kova_native_animation::Animation;
use kova_native_core::{ElementId, Instant, Point, Size};
use kova_native_input::{
    Action, DispatchPhase, InputEvent, KeyDownEvent, KeyUpEvent, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, PenEvent, ScrollWheelEvent,
};
use kova_native_layout::MeasureInput;
use smallvec::SmallVec;
use std::any::TypeId;
use std::rc::Rc;

pub(crate) type StyleFn = Rc<dyn Fn(Style) -> Style>;
pub(crate) type ClickFn = Rc<dyn Fn(&mut EventCx)>;
pub(crate) type EventFn<E> = Rc<dyn Fn(&E, &mut EventCx)>;
pub(crate) type BoolFn = Rc<dyn Fn(bool, &mut EventCx)>;
pub(crate) type ActionFn = Rc<dyn Fn(&dyn Action, &mut EventCx)>;
pub(crate) type AnimationFn = Rc<dyn Fn(Style, f32) -> Style>;

/// Pointer drag information.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DragEvent {
    pub button: MouseButton,
    /// Where the drag started (window coordinates).
    pub start: Point,
    /// Current pointer position (window coordinates).
    pub position: Point,
    /// Movement since the previous drag event.
    pub delta: Point,
}

impl DragEvent {
    /// Total movement since the drag started.
    pub fn total(&self) -> Point {
        self.position - self.start
    }
}

#[derive(Default, Clone)]
pub(crate) struct Handlers {
    pub click: Vec<ClickFn>,
    pub double_click: Vec<ClickFn>,
    pub mouse_down: Vec<(DispatchPhase, EventFn<MouseDownEvent>)>,
    pub mouse_up: Vec<(DispatchPhase, EventFn<MouseUpEvent>)>,
    pub mouse_move: Vec<(DispatchPhase, EventFn<MouseMoveEvent>)>,
    pub scroll: Vec<(DispatchPhase, EventFn<ScrollWheelEvent>)>,
    pub pen: Vec<(DispatchPhase, EventFn<PenEvent>)>,
    pub key_down: Vec<(DispatchPhase, EventFn<KeyDownEvent>)>,
    pub key_up: Vec<(DispatchPhase, EventFn<KeyUpEvent>)>,
    pub hover: Vec<BoolFn>,
    pub focus: Vec<BoolFn>,
    pub drag_start: Vec<EventFn<DragEvent>>,
    pub drag: Vec<EventFn<DragEvent>>,
    pub drag_end: Vec<EventFn<DragEvent>>,
    /// Pointer presses anywhere outside the element's logical subtree.
    pub click_outside: Vec<ClickFn>,
    pub actions: Vec<(TypeId, ActionFn)>,
}

impl Handlers {
    pub fn has_pointer(&self) -> bool {
        !self.click.is_empty()
            || !self.double_click.is_empty()
            || !self.mouse_down.is_empty()
            || !self.mouse_up.is_empty()
            || !self.mouse_move.is_empty()
            || !self.scroll.is_empty()
            || !self.pen.is_empty()
            || !self.hover.is_empty()
            || self.has_drag()
    }

    pub fn has_drag(&self) -> bool {
        !self.drag_start.is_empty() || !self.drag.is_empty() || !self.drag_end.is_empty()
    }
}

/// State and configuration common to every element.
#[derive(Default)]
pub struct ElementBase {
    pub(crate) id: Option<ElementId>,
    pub(crate) style: Style,
    pub(crate) hover_styles: Vec<StyleFn>,
    pub(crate) active_styles: Vec<StyleFn>,
    pub(crate) focus_styles: Vec<StyleFn>,
    pub(crate) focus_visible_styles: Vec<StyleFn>,
    pub(crate) disabled_styles: Vec<StyleFn>,
    /// Reactive style closures, re-run when the signals they read change.
    pub(crate) bound_styles: Vec<StyleFn>,
    pub(crate) animations: Vec<(Animation, AnimationFn)>,
    pub(crate) handlers: Handlers,
    pub(crate) focusable: bool,
    pub(crate) tab_index: i32,
    pub(crate) autofocus: bool,
    pub(crate) trap_focus: bool,
    pub(crate) block_pointer: bool,
    pub(crate) key_context: Option<&'static str>,
    pub(crate) children: Vec<AnyElement>,
}

impl ElementBase {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn style(&self) -> &Style {
        &self.style
    }

    pub fn id(&self) -> Option<&ElementId> {
        self.id.as_ref()
    }

    pub(crate) fn has_state_styles(&self) -> bool {
        !self.hover_styles.is_empty() || !self.active_styles.is_empty()
    }

    pub(crate) fn is_reactive(&self) -> bool {
        !self.bound_styles.is_empty()
    }

    /// Whether the node must take part in hit testing.
    pub(crate) fn wants_hitbox(&self) -> bool {
        self.handlers.has_pointer()
            || self.block_pointer
            || self.has_state_styles()
            || self.focusable
            || self.style.cursor.is_some()
            || self.style.layout.scrolls()
    }
}

/// A node in the element tree.
///
/// Most elements only need [`Element::base`]/[`Element::base_mut`]: the tree
/// handles style resolution, layout, background/border/shadow painting,
/// transitions, hit testing and event handlers generically. Leaves with
/// intrinsic content (text, images) implement measuring and painting.
pub trait Element: 'static {
    fn base(&self) -> &ElementBase;
    fn base_mut(&mut self) -> &mut ElementBase;

    /// A short name for debugging.
    fn name(&self) -> &'static str {
        "element"
    }

    /// Children to mount. Called once, when the element is mounted.
    fn take_children(&mut self) -> Vec<AnyElement> {
        std::mem::take(&mut self.base_mut().children)
    }

    /// Whether this element is a leaf sized by [`Element::measure`].
    fn is_measured(&self) -> bool {
        false
    }

    /// Intrinsic content size for the given constraints (logical px).
    fn measure(&mut self, _cx: &mut MeasureCx, _input: MeasureInput) -> Size {
        Size::ZERO
    }

    /// Re-evaluates element specific reactive content (e.g. dynamic text).
    /// Runs inside the node's tracking scope. Returns `true` if the
    /// intrinsic size may have changed.
    fn update_bindings(&mut self) -> bool {
        false
    }

    /// Whether [`Element::update_bindings`] has anything to track.
    fn has_bindings(&self) -> bool {
        false
    }

    /// The resolved inherited text style changed. Returns `true` if the
    /// intrinsic size may have changed.
    fn text_style_changed(
        &mut self,
        _cx: &mut MeasureCx,
        _style: &kova_native_text::TextStyle,
    ) -> bool {
        false
    }

    /// Whether signals read during [`Element::paint`] should schedule a
    /// repaint when they change (custom drawing).
    fn tracks_paint(&self) -> bool {
        false
    }

    /// Paints element specific content above the background and below children.
    fn paint(&mut self, _cx: &mut PaintCx) {}

    /// Paints above children.
    fn paint_overlay(&mut self, _cx: &mut PaintCx) {}

    /// Built-in behavior (text editing...) run when an event reaches this
    /// node, before user handlers in the bubble phase.
    fn handle_event(&mut self, _cx: &mut EventCx, _event: &InputEvent) {}

    /// Whether the element consumes text input (enables IME while focused).
    fn accepts_text_input(&self) -> bool {
        false
    }

    /// Caret rectangle relative to the element origin, for IME placement.
    fn ime_cursor_area(&self) -> Option<kova_native_core::Bounds> {
        None
    }

    /// Reactive regions return a builder producing their children; it is
    /// re-run (and the children replaced) when its dependencies change.
    fn region(&mut self) -> Option<&mut dyn FnMut() -> Vec<AnyElement>> {
        None
    }

    /// Called every frame while the element reports it is animating.
    fn is_animating(&self, _now: Instant) -> bool {
        false
    }

    /// Keyed lists return their child source; the tree reconciles their
    /// children by key. See [`crate::elements::keyed`].
    fn keyed(&mut self) -> Option<&mut dyn crate::elements::KeyedSource> {
        None
    }

    /// The text this element displays, if it is textual. Used by headless
    /// drivers and tests to find elements by what the user sees.
    fn text_content(&self) -> Option<&str> {
        None
    }

    /// Portals return how their children are placed in the overlay layer;
    /// the children are mounted there instead of under this element.
    /// See [`crate::elements::portal`].
    fn portal(&self) -> Option<crate::elements::PortalSpec> {
        None
    }
}

/// A type-erased element.
pub struct AnyElement(pub(crate) Box<dyn Element>);

impl AnyElement {
    pub fn new(element: impl Element) -> Self {
        AnyElement(Box::new(element))
    }

    pub fn name(&self) -> &'static str {
        self.0.name()
    }
}

/// Conversion into an element. Implemented for all elements, strings
/// (become text), `Option`s and [`AnyElement`].
pub trait IntoElement {
    fn into_any(self) -> AnyElement;
}

impl IntoElement for AnyElement {
    fn into_any(self) -> AnyElement {
        self
    }
}

impl IntoElement for &'static str {
    fn into_any(self) -> AnyElement {
        crate::elements::text(self).into_any()
    }
}

impl IntoElement for String {
    fn into_any(self) -> AnyElement {
        crate::elements::text(self).into_any()
    }
}

impl IntoElement for kova_native_core::SharedString {
    fn into_any(self) -> AnyElement {
        crate::elements::text(self).into_any()
    }
}

impl<E: IntoElement> IntoElement for Option<E> {
    fn into_any(self) -> AnyElement {
        match self {
            Some(e) => e.into_any(),
            None => crate::elements::empty().into_any(),
        }
    }
}

/// Builder methods for identity, state styles, reactivity and event handlers.
pub trait Interactive: Sized {
    fn base_mut(&mut self) -> &mut ElementBase;

    /// Gives the element a stable identity (preserves scroll/focus state
    /// across rebuilds and allows addressing it, e.g. `cx.focus_id(..)`).
    fn id(mut self, id: impl Into<ElementId>) -> Self {
        self.base_mut().id = Some(id.into());
        self
    }

    fn child(mut self, child: impl IntoElement) -> Self {
        self.base_mut().children.push(child.into_any());
        self
    }

    fn children<I: IntoElement>(mut self, children: impl IntoIterator<Item = I>) -> Self {
        self.base_mut()
            .children
            .extend(children.into_iter().map(IntoElement::into_any));
        self
    }

    // ---- state styles ---------------------------------------------------------

    /// Style applied while the pointer is over the element.
    fn hover(mut self, f: impl Fn(Style) -> Style + 'static) -> Self {
        self.base_mut().hover_styles.push(Rc::new(f));
        self
    }

    /// Style applied while the element is pressed.
    fn active(mut self, f: impl Fn(Style) -> Style + 'static) -> Self {
        self.base_mut().active_styles.push(Rc::new(f));
        self
    }

    /// Style applied while the element has keyboard focus.
    fn focus(mut self, f: impl Fn(Style) -> Style + 'static) -> Self {
        self.base_mut().focus_styles.push(Rc::new(f));
        self
    }

    /// Style applied while the element has keyboard focus *and* the user is
    /// navigating with the keyboard (like CSS `:focus-visible`). Use it for
    /// focus rings on buttons and other controls, so clicking them with the
    /// mouse does not leave a ring behind. Text fields usually want
    /// [`Interactive::focus`] instead.
    fn focus_visible(mut self, f: impl Fn(Style) -> Style + 'static) -> Self {
        self.base_mut().focus_visible_styles.push(Rc::new(f));
        self
    }

    /// Style applied while the element is disabled.
    fn disabled_style(mut self, f: impl Fn(Style) -> Style + 'static) -> Self {
        self.base_mut().disabled_styles.push(Rc::new(f));
        self
    }

    /// Reactive style: the closure re-runs whenever a signal it reads
    /// changes, updating only this element.
    ///
    /// ```ignore
    /// div().bind(move |s| if selected.get() { s.bg(accent) } else { s })
    /// ```
    fn bind(mut self, f: impl Fn(Style) -> Style + 'static) -> Self {
        self.base_mut().bound_styles.push(Rc::new(f));
        self
    }

    /// Runs a time based animation; `f` receives the base style and eased
    /// progress (0..1) every frame.
    fn animation(
        mut self,
        animation: Animation,
        f: impl Fn(Style, f32) -> Style + 'static,
    ) -> Self {
        self.base_mut().animations.push((animation, Rc::new(f)));
        self
    }

    // ---- focus ------------------------------------------------------------------

    /// Makes the element focusable by click and Tab.
    fn focusable(mut self) -> Self {
        self.base_mut().focusable = true;
        self
    }

    /// Makes the element focusable and orders it in Tab navigation: lower
    /// indices first, ties in tree order. A negative index (like HTML's
    /// `tabindex="-1"`) keeps the element focusable by click and
    /// programmatically, but skips it during Tab navigation.
    fn tab_index(mut self, index: i32) -> Self {
        let base = self.base_mut();
        base.focusable = true;
        base.tab_index = index;
        self
    }

    /// Focuses the element when it is mounted (it is made focusable).
    fn autofocus(mut self) -> Self {
        let base = self.base_mut();
        base.focusable = true;
        base.autofocus = true;
        self
    }

    /// Catches pointer events even without handlers, so presses on this
    /// element's background never reach elements painted below it (panels
    /// over a clickable backdrop, cards over a canvas).
    fn block_pointer(mut self) -> Self {
        self.base_mut().block_pointer = true;
        self
    }

    /// Confines Tab navigation to this element's subtree while it is mounted
    /// (dialogs, drawers). When the element is removed while nothing is
    /// focused, focus returns to the element that was focused when it mounted.
    fn trap_focus(mut self) -> Self {
        self.base_mut().trap_focus = true;
        self
    }

    /// Activates context specific key bindings while focus is inside.
    fn key_context(mut self, context: &'static str) -> Self {
        self.base_mut().key_context = Some(context);
        self
    }

    // ---- handlers -------------------------------------------------------------

    /// Primary-button click (press and release on this element).
    fn on_click(mut self, f: impl Fn(&mut EventCx) + 'static) -> Self {
        self.base_mut().handlers.click.push(Rc::new(f));
        self
    }

    fn on_double_click(mut self, f: impl Fn(&mut EventCx) + 'static) -> Self {
        self.base_mut().handlers.double_click.push(Rc::new(f));
        self
    }

    fn on_mouse_down(mut self, f: impl Fn(&MouseDownEvent, &mut EventCx) + 'static) -> Self {
        self.base_mut()
            .handlers
            .mouse_down
            .push((DispatchPhase::Bubble, Rc::new(f)));
        self
    }

    fn capture_mouse_down(mut self, f: impl Fn(&MouseDownEvent, &mut EventCx) + 'static) -> Self {
        self.base_mut()
            .handlers
            .mouse_down
            .push((DispatchPhase::Capture, Rc::new(f)));
        self
    }

    fn on_mouse_up(mut self, f: impl Fn(&MouseUpEvent, &mut EventCx) + 'static) -> Self {
        self.base_mut()
            .handlers
            .mouse_up
            .push((DispatchPhase::Bubble, Rc::new(f)));
        self
    }

    fn on_mouse_move(mut self, f: impl Fn(&MouseMoveEvent, &mut EventCx) + 'static) -> Self {
        self.base_mut()
            .handlers
            .mouse_move
            .push((DispatchPhase::Bubble, Rc::new(f)));
        self
    }

    fn on_scroll_wheel(mut self, f: impl Fn(&ScrollWheelEvent, &mut EventCx) + 'static) -> Self {
        self.base_mut()
            .handlers
            .scroll
            .push((DispatchPhase::Bubble, Rc::new(f)));
        self
    }

    /// Handles pressure-sensitive pointer input bubbling from the target.
    fn on_pen(mut self, f: impl Fn(&PenEvent, &mut EventCx) + 'static) -> Self {
        self.base_mut()
            .handlers
            .pen
            .push((DispatchPhase::Bubble, Rc::new(f)));
        self
    }

    /// Handles pressure-sensitive pointer input during capture.
    fn capture_pen(mut self, f: impl Fn(&PenEvent, &mut EventCx) + 'static) -> Self {
        self.base_mut()
            .handlers
            .pen
            .push((DispatchPhase::Capture, Rc::new(f)));
        self
    }

    fn on_key_down(mut self, f: impl Fn(&KeyDownEvent, &mut EventCx) + 'static) -> Self {
        self.base_mut()
            .handlers
            .key_down
            .push((DispatchPhase::Bubble, Rc::new(f)));
        self
    }

    fn capture_key_down(mut self, f: impl Fn(&KeyDownEvent, &mut EventCx) + 'static) -> Self {
        self.base_mut()
            .handlers
            .key_down
            .push((DispatchPhase::Capture, Rc::new(f)));
        self
    }

    fn on_key_up(mut self, f: impl Fn(&KeyUpEvent, &mut EventCx) + 'static) -> Self {
        self.base_mut()
            .handlers
            .key_up
            .push((DispatchPhase::Bubble, Rc::new(f)));
        self
    }

    fn on_hover(mut self, f: impl Fn(bool, &mut EventCx) + 'static) -> Self {
        self.base_mut().handlers.hover.push(Rc::new(f));
        self
    }

    fn on_focus_change(mut self, f: impl Fn(bool, &mut EventCx) + 'static) -> Self {
        self.base_mut().handlers.focus.push(Rc::new(f));
        self
    }

    fn on_drag_start(mut self, f: impl Fn(&DragEvent, &mut EventCx) + 'static) -> Self {
        self.base_mut().handlers.drag_start.push(Rc::new(f));
        self
    }

    fn on_drag(mut self, f: impl Fn(&DragEvent, &mut EventCx) + 'static) -> Self {
        self.base_mut().handlers.drag.push(Rc::new(f));
        self
    }

    fn on_drag_end(mut self, f: impl Fn(&DragEvent, &mut EventCx) + 'static) -> Self {
        self.base_mut().handlers.drag_end.push(Rc::new(f));
        self
    }

    /// Called when a mouse button is pressed outside this element's logical
    /// subtree. Overlay content mounted through a [`portal`](crate::elements::portal)
    /// counts as inside the element that declares the portal, so this closes
    /// menus and popovers without reacting to clicks inside them.
    fn on_click_outside(mut self, f: impl Fn(&mut EventCx) + 'static) -> Self {
        self.base_mut().handlers.click_outside.push(Rc::new(f));
        self
    }

    /// Handles an action dispatched by a key binding (or programmatically)
    /// while focus is inside this element.
    fn on_action<A: Action + Clone>(mut self, f: impl Fn(&A, &mut EventCx) + 'static) -> Self {
        let handler: ActionFn = Rc::new(move |action, cx| {
            if let Some(a) = action.as_any().downcast_ref::<A>() {
                f(a, cx);
            }
        });
        self.base_mut()
            .handlers
            .actions
            .push((TypeId::of::<A>(), handler));
        self
    }
}

impl<T: Interactive> Styled for T {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.base_mut().style
    }
}

/// Implements [`Interactive`] and [`IntoElement`] for an element type whose
/// [`Element::base_mut`] is available.
#[macro_export]
macro_rules! impl_element_builder {
    ($ty:ty) => {
        impl $crate::Interactive for $ty {
            fn base_mut(&mut self) -> &mut $crate::ElementBase {
                $crate::Element::base_mut(self)
            }
        }

        impl $crate::IntoElement for $ty {
            fn into_any(self) -> $crate::AnyElement {
                $crate::AnyElement::new(self)
            }
        }
    };
}

/// Small helper so element code can collect children into a SmallVec.
pub(crate) type NodeList = SmallVec<[crate::tree::NodeId; 8]>;
