//! Driving an element tree without a window: tests, scripted demos and
//! offscreen captures.
//!
//! [`Headless`] owns an [`ElementTree`] plus the resources a frame needs and
//! a simulated clock. Input helpers address elements by [`ElementId`] or by
//! their visible text, then dispatch the same events a native window would.
//! Time only moves when [`Headless::advance`] (or a helper that waits) is
//! called, so animations, timers and async sleeps are deterministic.
//!
//! ```
//! use kova_native_widgets::headless::Headless;
//! use kova_native_widgets::prelude::*;
//! use kova_native_core::{Owner, Size, signal};
//!
//! let owner = Owner::new_root();
//! let count = owner.with(|| signal(0));
//! let mut ui = Headless::new(Size::new(400.0, 300.0), move || {
//!     column()
//!         .child(text(move || format!("Count: {}", count.get())))
//!         .child(button("Increment").id("inc").on_click(move |_| count.update(|n| *n += 1)))
//! });
//! ui.click_id("inc");
//! ui.click_text("Increment");
//! assert_eq!(count.get(), 2);
//! assert!(ui.find_text("Count: 2").is_some());
//! drop(ui);
//! owner.dispose();
//! ```
//!
//! With a GPU adapter, [`Headless::with_gpu`] renders frames to RGBA pixels
//! through the real renderer ([`Headless::capture`]).

use crate::context::MemoryClipboard;
use crate::element::IntoElement;
use crate::tree::{
    DispatchContext, DispatchResult, ElementTree, FrameContext, FrameOutput, NodeId,
};
use kova_native_core::{Bounds, Duration, ElementId, Instant, KovaError, KovaResult, Point, Size};
use kova_native_input::{
    InputEvent, KeyDownEvent, KeyUpEvent, Keymap, Keystroke, Modifiers, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, ScrollDelta, ScrollWheelEvent,
};
use kova_native_render::{Atlas, GpuContext, Renderer, Scene};
use kova_native_text::TextSystem;

/// Simulated frame interval used while advancing time.
pub const FRAME: Duration = Duration::from_micros(16_667);

enum Backend {
    Cpu(Atlas),
    Gpu {
        gpu: Box<GpuContext>,
        renderer: Box<Renderer>,
    },
}

/// A windowless element tree with a simulated clock. See the module docs.
pub struct Headless {
    tree: ElementTree,
    text: TextSystem,
    scene: Scene,
    backend: Backend,
    clipboard: MemoryClipboard,
    keymap: Keymap,
    now: Instant,
    mouse: Point,
    size: Size,
    scale: f32,
    last: FrameOutput,
}

impl Headless {
    /// Mounts `build` at `size` (logical px, scale 1) in a tree that follows
    /// the current theme ([`ElementTree::themed`]), and produces the first frame.
    pub fn new<E: IntoElement>(size: Size, mut build: impl FnMut() -> E + 'static) -> Self {
        let tree = ElementTree::themed(move || build().into_any());
        Self::from_tree(tree, size)
    }

    /// Wraps an existing tree.
    pub fn from_tree(mut tree: ElementTree, size: Size) -> Self {
        tree.set_viewport(size, 1.0);
        let mut ui = Headless {
            tree,
            text: TextSystem::new(),
            scene: Scene::new(),
            backend: Backend::Cpu(Atlas::new()),
            clipboard: MemoryClipboard::default(),
            keymap: Keymap::new(),
            now: Instant::now(),
            mouse: Point::new(-1.0, -1.0),
            size,
            scale: 1.0,
            last: FrameOutput::default(),
        };
        ui.settle();
        ui
    }

    /// Switches to GPU rendering so [`Headless::capture`] works. Fails when
    /// no adapter is available.
    pub fn with_gpu(mut self) -> KovaResult<Self> {
        let gpu = GpuContext::new_headless()?;
        let renderer = Renderer::new(&gpu);
        self.backend = Backend::Gpu {
            gpu: Box::new(gpu),
            renderer: Box::new(renderer),
        };
        // The new atlas has none of the cached glyphs: repaint everything.
        self.tree.request_repaint();
        self.frame();
        Ok(self)
    }

    /// Changes the logical size and device scale.
    pub fn resize(&mut self, size: Size, scale: f32) {
        self.size = size;
        self.scale = scale;
        self.tree.set_viewport(size, scale);
        self.settle();
    }

    pub fn size(&self) -> Size {
        self.size
    }

    pub fn tree(&self) -> &ElementTree {
        &self.tree
    }

    pub fn tree_mut(&mut self) -> &mut ElementTree {
        &mut self.tree
    }

    pub fn scene(&self) -> &Scene {
        &self.scene
    }

    /// The simulated time.
    pub fn now(&self) -> Instant {
        self.now
    }

    /// Output of the most recent frame.
    pub fn last_frame(&self) -> &FrameOutput {
        &self.last
    }

    /// Registers key bindings (as `Application::key_bindings` does).
    pub fn key_bindings(
        &mut self,
        bindings: impl IntoIterator<Item = kova_native_input::KeyBinding>,
    ) {
        self.keymap.extend(bindings);
    }

    /// Text most recently written to the clipboard.
    pub fn clipboard(&self) -> Option<&str> {
        self.clipboard.0.as_deref()
    }

    pub fn set_clipboard(&mut self, text: impl Into<String>) {
        self.clipboard.0 = Some(text.into());
    }

    /// Produces one frame at the current simulated time.
    pub fn frame(&mut self) -> &FrameOutput {
        let atlas = match &mut self.backend {
            Backend::Cpu(atlas) => atlas,
            Backend::Gpu { renderer, .. } => renderer.atlas(),
        };
        self.last = self.tree.frame(&mut FrameContext {
            text: &mut self.text,
            scene: &mut self.scene,
            atlas,
            now: self.now,
        });
        &self.last
    }

    /// Renders frames (without advancing time) until the tree is idle, then
    /// advances through any running animation. Bounded to 10 simulated seconds.
    pub fn settle(&mut self) {
        for _ in 0..8 {
            self.frame();
            if !self.tree.needs_frame() {
                break;
            }
        }
        let limit = self.now + Duration::from_secs(10);
        while (self.last.animating || self.tree.needs_frame()) && self.now < limit {
            self.now += FRAME;
            self.frame();
        }
    }

    /// Advances the simulated clock by `duration`, producing a frame every
    /// [`FRAME`]. Timers, async sleeps and animations progress accordingly.
    pub fn advance(&mut self, duration: Duration) {
        let end = self.now + duration;
        while self.now < end {
            self.now = (self.now + FRAME).min(end);
            self.frame();
        }
    }

    /// Advances time in frame steps until `f` returns true, for at most `limit`.
    pub fn wait_until(&mut self, limit: Duration, mut f: impl FnMut(&mut Self) -> bool) -> bool {
        let end = self.now + limit;
        loop {
            if f(self) {
                return true;
            }
            if self.now >= end {
                return false;
            }
            self.now += FRAME;
            self.frame();
        }
    }

    // ---- lookup -----------------------------------------------------------------

    /// The node with element id `id`.
    pub fn node(&self, id: impl Into<ElementId>) -> Option<NodeId> {
        self.tree.node_by_id(&id.into())
    }

    /// Window bounds (after transforms) of the element with `id`.
    pub fn bounds_of(&self, id: impl Into<ElementId>) -> Option<Bounds> {
        let node = self.node(id)?;
        self.tree.visual_bounds(node)
    }

    /// The first painted text element whose content equals `content`
    /// (searching the root, then the overlay layer, in paint order).
    pub fn find_text(&self, content: &str) -> Option<NodeId> {
        self.find_text_where(|t| t == content)
    }

    /// The first painted text element whose content contains `needle`.
    pub fn find_text_containing(&self, needle: &str) -> Option<NodeId> {
        self.find_text_where(|t| t.contains(needle))
    }

    fn find_text_where(&self, f: impl Fn(&str) -> bool) -> Option<NodeId> {
        self.tree
            .painted_nodes()
            .into_iter()
            .find(|n| self.tree.text_content(*n).is_some_and(&f))
    }

    /// All painted text, in paint order (handy in assertions and debugging).
    pub fn visible_text(&self) -> Vec<String> {
        self.tree
            .painted_nodes()
            .into_iter()
            .filter_map(|n| self.tree.text_content(n).map(str::to_string))
            .filter(|t| !t.is_empty())
            .collect()
    }

    /// The currently focused element's id, if it has one.
    pub fn focused_id(&self) -> Option<ElementId> {
        let node = self.tree.focused()?;
        self.tree.element_id(node).cloned()
    }

    // ---- pointer ----------------------------------------------------------------

    fn dispatch(&mut self, event: InputEvent) -> DispatchResult {
        let result = self.tree.dispatch(
            &event,
            &mut DispatchContext {
                text: &mut self.text,
                clipboard: &mut self.clipboard,
                keymap: &mut self.keymap,
                now: self.now,
            },
        );
        self.settle();
        result
    }

    /// Dispatches an arbitrary input event, then settles.
    pub fn send(&mut self, event: InputEvent) -> DispatchResult {
        self.dispatch(event)
    }

    pub fn mouse_move(&mut self, position: Point) {
        self.mouse = position;
        self.dispatch(InputEvent::MouseMove(MouseMoveEvent {
            position,
            pressed_button: None,
            modifiers: Modifiers::default(),
        }));
    }

    pub fn mouse_down(&mut self, position: Point, button: MouseButton) {
        self.mouse = position;
        self.dispatch(InputEvent::MouseDown(MouseDownEvent {
            button,
            position,
            modifiers: Modifiers::default(),
            click_count: 1,
        }));
    }

    pub fn mouse_up(&mut self, position: Point, button: MouseButton) {
        self.mouse = position;
        self.dispatch(InputEvent::MouseUp(MouseUpEvent {
            button,
            position,
            modifiers: Modifiers::default(),
            click_count: 1,
        }));
    }

    /// Moves to `position` and clicks the left button there.
    pub fn click_at(&mut self, position: Point) {
        self.mouse_move(position);
        self.mouse_down(position, MouseButton::Left);
        self.mouse_up(position, MouseButton::Left);
    }

    /// Moves to `position` and clicks the right button there.
    pub fn right_click_at(&mut self, position: Point) {
        self.mouse_move(position);
        self.mouse_down(position, MouseButton::Right);
        self.mouse_up(position, MouseButton::Right);
    }

    /// Clicks the center of the element with `id`. Panics if it is not painted.
    pub fn click_id(&mut self, id: impl Into<ElementId>) {
        let id = id.into();
        let b = self
            .bounds_of(id.clone())
            .unwrap_or_else(|| panic!("no painted element with id {id:?}"));
        self.click_at(b.center());
    }

    /// Clicks the center of the text element showing `content`.
    /// Panics if no painted text matches.
    pub fn click_text(&mut self, content: &str) {
        let point = self.text_center(content);
        self.click_at(point);
    }

    /// Moves the pointer over the center of the element with `id`.
    pub fn hover_id(&mut self, id: impl Into<ElementId>) {
        let id = id.into();
        let b = self
            .bounds_of(id.clone())
            .unwrap_or_else(|| panic!("no painted element with id {id:?}"));
        self.mouse_move(b.center());
    }

    /// Moves the pointer over the text element showing `content`.
    pub fn hover_text(&mut self, content: &str) {
        let point = self.text_center(content);
        self.mouse_move(point);
    }

    fn text_center(&self, content: &str) -> Point {
        let node = self.find_text(content).unwrap_or_else(|| {
            panic!(
                "no painted text {content:?}; visible: {:?}",
                self.visible_text()
            )
        });
        self.tree
            .visual_bounds(node)
            .expect("painted text has bounds")
            .center()
    }

    /// Scrolls the wheel by `lines` (positive scrolls content up) at `position`.
    pub fn scroll_at(&mut self, position: Point, lines: f32) {
        self.mouse = position;
        self.dispatch(InputEvent::ScrollWheel(ScrollWheelEvent {
            position,
            delta: ScrollDelta::Lines(Point::new(0.0, lines)),
            modifiers: Modifiers::default(),
        }));
    }

    /// Current pointer position.
    pub fn mouse(&self) -> Point {
        self.mouse
    }

    // ---- keyboard ---------------------------------------------------------------

    /// Presses and releases a keystroke such as `"enter"`, `"tab"`,
    /// `"shift-tab"`, `"ctrl-k"` or `"escape"`. Panics on an invalid string.
    pub fn press(&mut self, keystroke: &str) {
        let keystroke = Keystroke::parse(keystroke).unwrap_or_else(|e| panic!("{e}"));
        self.dispatch(InputEvent::KeyDown(KeyDownEvent {
            keystroke: keystroke.clone(),
            is_repeat: false,
        }));
        self.dispatch(InputEvent::KeyUp(KeyUpEvent { keystroke }));
    }

    /// Types `text` into the focused element.
    pub fn type_text(&mut self, text: &str) {
        self.dispatch(InputEvent::TextInput(text.to_string()));
    }

    /// Types `text` one character at a time, advancing `per_char` between
    /// characters (for recordings).
    pub fn type_slowly(&mut self, text: &str, per_char: Duration) {
        for c in text.chars() {
            self.type_text(&c.to_string());
            self.advance(per_char);
        }
    }

    // ---- capture ----------------------------------------------------------------

    /// Physical pixel size of captures.
    pub fn pixel_size(&self) -> (u32, u32) {
        (
            (self.size.width * self.scale).round() as u32,
            (self.size.height * self.scale).round() as u32,
        )
    }

    /// Renders the last frame to tightly packed RGBA8 pixels.
    /// Requires [`Headless::with_gpu`].
    pub fn capture(&mut self) -> KovaResult<Vec<u8>> {
        let (w, h) = self.pixel_size();
        let background = crate::theme().background;
        match &mut self.backend {
            Backend::Gpu { gpu, renderer } => {
                Ok(renderer.render_to_rgba(gpu, &self.scene, w, h, background))
            }
            Backend::Cpu(_) => Err(KovaError::Other(
                "Headless::capture requires Headless::with_gpu()".into(),
            )),
        }
    }
}
