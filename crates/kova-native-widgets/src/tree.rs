//! The retained element tree of a window and its frame pipeline.
//!
//! ```text
//!  signal write ──► Observer notify ──► dirty queue ──► request frame
//!                                                         │
//!  frame:  reactive flush ─► style resolve (dirty nodes, depth order)
//!          ─► layout (taffy, only dirty subtrees) ─► paint (scene + hitboxes)
//! ```
//!
//! Nodes are never rebuilt wholesale. A signal change touches exactly the
//! nodes whose bindings read it; reactive regions replace only their own
//! children. Layout is incremental (taffy caches per node) and nothing at all
//! runs while the UI is idle.

use crate::context::{Clipboard, Command, EventCx, MeasureCx, PaintCx, WindowCommand};
use crate::element::{AnyElement, DragEvent, Element, NodeList};
use crate::style::{BoxShadow, Style, Styled, Visual};
use kova_native_animation::{Animated, Easing, Transition};
use kova_native_core::{
    Bounds, Color, Corners, Dirty, ElementId, Instant, Observer, Owner, Point, Size, Transform2D,
};
use kova_native_input::{
    Action, CursorStyle, DispatchPhase, InputEvent, Key, KeyDownEvent, Keymap, KeymapMatch,
    MouseButton, NamedKey, ScrollDelta,
};
use kova_native_layout::{LayoutEngine, LayoutId, LayoutStyle, NodeLayout};
use kova_native_render::{Atlas, ContentMask, Scene};
use kova_native_text::{TextStyle, TextSystem};
use rustc_hash::{FxHashMap, FxHashSet};
use slotmap::{SlotMap, new_key_type};
use smallvec::SmallVec;
use std::cell::RefCell;
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::rc::Rc;
use std::time::Duration;

new_key_type! {
    /// Identifies a node in an [`ElementTree`].
    pub struct NodeId;
}

/// Pixels scrolled per wheel "line".
const SCROLL_LINE: f32 = 56.0;
/// Pointer travel before a press becomes a drag.
const DRAG_THRESHOLD: f32 = 3.0;

pub(crate) struct ScrollState {
    offset: Animated<Point>,
    max: Point,
    last_activity: Option<Instant>,
}

impl ScrollState {
    fn new() -> Self {
        ScrollState {
            offset: Animated::new(Point::ZERO),
            max: Point::ZERO,
            last_activity: None,
        }
    }
}

pub(crate) struct Node {
    pub element: Box<dyn Element>,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    pub depth: u32,
    pub layout_id: LayoutId,
    pub resolved: Style,
    pub resolved_once: bool,
    pub text_style: TextStyle,
    pub disabled: bool,
    pub visual: Animated<Visual>,
    pub hovered: bool,
    pub active: bool,
    pub focused: bool,
    pub observer: Option<Observer>,
    pub region_observer: Option<Observer>,
    pub paint_observer: Option<Observer>,
    pub owner: Option<Owner>,
    pub animation_start: Instant,
    pub scroll: Option<ScrollState>,
    pub bounds: Bounds,
    pub transform: Transform2D,
    pub layout: NodeLayout,
    pub wants_hitbox: bool,
    pub queued: bool,
    pub painted: bool,
    /// Whether the element is a reactive region (transparent for anchoring).
    pub is_region: bool,
    /// Children of a portal, mounted under the overlay layer.
    pub portal_children: Vec<NodeId>,
    /// Positioning of a portal child relative to its anchor.
    pub anchor: Option<AnchorState>,
    /// Direct child of a portal: blocks the pointer like an opaque layer.
    pub overlay_root: bool,
    /// For focus traps: the node focused when the trap was mounted.
    pub restore_focus: Option<NodeId>,
    /// Keyed lists: current key of each child, in order.
    pub keyed_children: Vec<(u64, NodeId)>,
    /// Owner of a keyed list item's reactive state (disposed on removal).
    pub item_owner: Option<Owner>,
}

#[derive(Clone, Debug)]
pub(crate) enum AnchorTarget {
    Node(NodeId),
    Id(ElementId),
    Point(Point),
}

#[derive(Clone, Debug)]
pub(crate) struct AnchorState {
    target: AnchorTarget,
    placement: crate::elements::Placement,
    gap: f32,
    match_width: bool,
}

#[derive(Default)]
pub(crate) struct DirtyQueue {
    nodes: Vec<NodeId>,
    waker: Option<Rc<dyn Fn()>>,
}

struct Hitbox {
    node: NodeId,
    bounds: Bounds,
    inverse: Option<Transform2D>,
    clip: Bounds,
}

struct Pressed {
    button: MouseButton,
    path: Vec<NodeId>,
}

struct DragState {
    node: NodeId,
    button: MouseButton,
    start: Point,
    last: Point,
    started: bool,
}

#[derive(Default)]
struct Retained {
    scroll: Option<Point>,
    focused: bool,
}

/// Resources needed to produce a frame.
pub struct FrameContext<'a> {
    pub text: &'a mut TextSystem,
    pub scene: &'a mut Scene,
    pub atlas: &'a mut Atlas,
    pub now: Instant,
}

/// What the window must do after a frame.
#[derive(Debug, Default)]
pub struct FrameOutput {
    /// Something is animating; schedule another frame.
    pub animating: bool,
    pub cursor: CursorStyle,
    /// Caret area for the IME when a text input is focused.
    pub ime_area: Option<Bounds>,
    /// When the window should draw again even if nothing is invalidated
    /// (timed repaints requested with `PaintCx::request_frame_at`).
    pub next_frame: Option<Instant>,
    pub stats: FrameStats,
}

/// Per frame counters, useful to verify incremental behavior.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameStats {
    pub nodes: usize,
    pub styles_resolved: usize,
    pub regions_rebuilt: usize,
    pub layout_ran: bool,
    pub nodes_painted: usize,
}

/// Resources for dispatching an input event.
pub struct DispatchContext<'a> {
    pub text: &'a mut TextSystem,
    pub clipboard: &'a mut dyn Clipboard,
    pub keymap: &'a mut Keymap,
    pub now: Instant,
}

/// Result of dispatching an input event.
#[derive(Default)]
pub struct DispatchResult {
    pub handled: bool,
    /// Actions no element handled (the app may handle them globally).
    pub unhandled_actions: Vec<Box<dyn Action>>,
}

/// The retained UI of one window.
pub struct ElementTree {
    pub(crate) nodes: SlotMap<NodeId, Node>,
    root: NodeId,
    /// Layer painted above `root`; parent of all portal children.
    overlay: NodeId,
    /// Nodes with `on_click_outside` handlers.
    outside_listeners: FxHashSet<NodeId>,
    /// Whether the last interaction was the keyboard (`focus_visible`).
    keyboard_modality: bool,
    root_owner: Owner,
    layout: LayoutEngine<NodeId>,
    queue: Rc<RefCell<DirtyQueue>>,
    style_queue: BinaryHeap<Reverse<(u32, NodeId)>>,
    animated_nodes: FxHashSet<NodeId>,
    needs_layout: bool,
    needs_paint: bool,
    /// Earliest timed repaint requested while painting the last frame.
    next_wake: Option<Instant>,
    ids: FxHashMap<ElementId, NodeId>,
    retained: FxHashMap<ElementId, Retained>,
    hitboxes: Vec<Hitbox>,
    hovered: Vec<NodeId>,
    pressed: Option<Pressed>,
    drag: Option<DragState>,
    focused: Option<NodeId>,
    mouse: Option<Point>,
    viewport: Size,
    scale: f32,
    root_text_style: TextStyle,
    cursor: CursorStyle,
    commands: Vec<Command>,
    window_commands: Vec<WindowCommand>,
    now: Instant,
    stats: FrameStats,
}

impl ElementTree {
    /// Creates a tree whose root is a reactive region running `build`, with
    /// a fixed root text style and background.
    pub fn new(
        build: impl FnMut() -> AnyElement + 'static,
        root_text_style: TextStyle,
        background: Color,
    ) -> Self {
        Self::create(build, root_text_style, background, false)
    }

    /// Creates a tree whose root background, text color and text size follow
    /// the current [`Theme`](crate::Theme), so text without an explicit color
    /// stays readable when [`set_theme`](crate::set_theme) switches themes.
    pub fn themed(build: impl FnMut() -> AnyElement + 'static) -> Self {
        let t = crate::theme();
        let style = TextStyle {
            size: t.font_size,
            color: t.text,
            ..Default::default()
        };
        Self::create(build, style, t.background, true)
    }

    fn create(
        build: impl FnMut() -> AnyElement + 'static,
        root_text_style: TextStyle,
        background: Color,
        follow_theme: bool,
    ) -> Self {
        let root_owner = Owner::new_root();
        let mut tree = ElementTree {
            nodes: SlotMap::with_key(),
            root: NodeId::default(),
            overlay: NodeId::default(),
            outside_listeners: FxHashSet::default(),
            keyboard_modality: false,
            root_owner,
            layout: LayoutEngine::new(),
            queue: Rc::new(RefCell::new(DirtyQueue::default())),
            style_queue: BinaryHeap::new(),
            animated_nodes: FxHashSet::default(),
            needs_layout: true,
            needs_paint: true,
            next_wake: None,
            ids: FxHashMap::default(),
            retained: FxHashMap::default(),
            hitboxes: Vec::new(),
            hovered: Vec::new(),
            pressed: None,
            drag: None,
            focused: None,
            mouse: None,
            viewport: Size::new(800.0, 600.0),
            scale: 1.0,
            root_text_style,
            cursor: CursorStyle::Arrow,
            commands: Vec::new(),
            window_commands: Vec::new(),
            now: Instant::now(),
            stats: FrameStats::default(),
        };
        // The overlay host exists before the root mounts its first portal.
        let overlay = crate::elements::div().with_style(|s| {
            s.layout.position = kova_native_layout::Position::Relative;
        });
        tree.overlay = tree.mount(AnyElement::new(overlay), None);
        let mut build = build;
        let mut root = crate::elements::Region::new(move || vec![build()]).with_style(|s| {
            // A 1x1 grid stretches the user's root element over the
            // whole window unless it sets an explicit size.
            s.layout.display = kova_native_layout::Display::Grid;
            s.layout.grid_mut().template_columns = vec![kova_native_layout::Track::Fr(1.0)];
            s.layout.grid_mut().template_rows = vec![kova_native_layout::Track::Fr(1.0)];
            s.background = Some(background.into());
        });
        if follow_theme {
            use crate::element::Interactive;
            root = root.bind(|s| {
                let t = crate::theme();
                s.bg(t.background).text_color(t.text).text_size(t.font_size)
            });
        }
        tree.root = tree.mount(AnyElement::new(root), None);
        tree
    }

    /// Sets a callback invoked whenever reactive state invalidates the tree
    /// (typically requests a redraw of the window).
    pub fn set_waker(&mut self, waker: impl Fn() + 'static) {
        self.queue.borrow_mut().waker = Some(Rc::new(waker));
    }

    pub fn root(&self) -> NodeId {
        self.root
    }

    /// The overlay layer: painted above the root, parent of portal children.
    pub fn overlay_root(&self) -> NodeId {
        self.overlay
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Resizes the viewport (logical px) and sets the scale factor.
    pub fn set_viewport(&mut self, size: Size, scale: f32) {
        if self.viewport != size || self.scale != scale {
            self.viewport = size;
            self.scale = scale;
            for top in [self.root, self.overlay] {
                if let Some(node) = self.nodes.get(top) {
                    let mut style = node.resolved.layout.clone();
                    style.size = kova_native_layout::Axes {
                        width: size.width.into(),
                        height: size.height.into(),
                    };
                    self.layout.set_style(node.layout_id, &style);
                }
            }
            crate::responsive::set_viewport_size(size);
            self.needs_layout = true;
            self.needs_paint = true;
        }
    }

    pub fn viewport(&self) -> Size {
        self.viewport
    }

    pub fn scale_factor(&self) -> f32 {
        self.scale
    }

    /// Whether a frame is required (dirty state, pending reactive updates
    /// or running animations).
    pub fn needs_frame(&self) -> bool {
        self.needs_layout
            || self.needs_paint
            || !self.style_queue.is_empty()
            || !self.queue.borrow().nodes.is_empty()
            || !self.animated_nodes.is_empty()
            || !self.commands.is_empty()
            || kova_native_core::task::has_ready()
    }

    /// When the tree must produce a frame even without new invalidation:
    /// the earliest timed repaint or UI timer. Native windows wake then.
    pub fn next_deadline(&self) -> Option<Instant> {
        let timer = kova_native_core::timer::next_deadline();
        match (self.next_wake, timer) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    pub fn take_window_commands(&mut self) -> Vec<WindowCommand> {
        std::mem::take(&mut self.window_commands)
    }

    pub fn focused(&self) -> Option<NodeId> {
        self.focused
    }

    /// Looks up a node by element id.
    pub fn node_by_id(&self, id: &ElementId) -> Option<NodeId> {
        self.ids.get(id).copied()
    }

    /// Absolute bounds of a node as of the last frame.
    pub fn bounds(&self, node: NodeId) -> Option<Bounds> {
        self.nodes.get(node).map(|n| n.bounds)
    }

    pub fn element_name(&self, node: NodeId) -> Option<&'static str> {
        self.nodes.get(node).map(|n| n.element.name())
    }

    pub fn children(&self, node: NodeId) -> &[NodeId] {
        self.nodes.get(node).map_or(&[], |n| &n.children)
    }

    /// Topmost interactive node under `position` as of the last frame.
    pub fn hit_test(&self, position: Point) -> Option<NodeId> {
        for hb in self.hitboxes.iter().rev() {
            if !hb.clip.contains(position) {
                continue;
            }
            let local = hb.inverse.map_or(position, |inv| inv.apply(position));
            if hb.bounds.contains(local) {
                return Some(hb.node);
            }
        }
        None
    }

    /// Prints the tree structure with bounds (debugging aid).
    pub fn debug_dump(&self) -> String {
        let mut out = String::new();
        self.dump_node(self.root, 0, &mut out);
        if !self.nodes[self.overlay].children.is_empty() {
            out.push_str("overlay:\n");
            self.dump_node(self.overlay, 1, &mut out);
        }
        out
    }

    fn dump_node(&self, id: NodeId, indent: usize, out: &mut String) {
        let Some(node) = self.nodes.get(id) else {
            return;
        };
        let b = node.bounds;
        out.push_str(&format!(
            "{}{} [{:.1}, {:.1}, {:.1}x{:.1}]{}\n",
            "  ".repeat(indent),
            node.element.name(),
            b.origin.x,
            b.origin.y,
            b.size.width,
            b.size.height,
            node.element
                .base()
                .id()
                .map(|i| format!(" #{i:?}"))
                .unwrap_or_default()
        ));
        for c in &node.children {
            self.dump_node(*c, indent + 1, out);
        }
    }

    // ---- mounting -----------------------------------------------------------

    fn mount(&mut self, element: AnyElement, parent: Option<NodeId>) -> NodeId {
        self.mount_inner(element, parent, None)
    }

    fn mount_inner(
        &mut self,
        element: AnyElement,
        parent: Option<NodeId>,
        item_owner: Option<Owner>,
    ) -> NodeId {
        let mut element = element.0;
        let children = element.take_children();
        let depth = parent
            .and_then(|p| self.nodes.get(p))
            .map_or(0, |p| p.depth + 1);
        let layout_id = self.layout.create(&LayoutStyle::default(), None);
        let has_animations = !element.base().animations.is_empty();
        let reactive = element.base().is_reactive() || element.has_bindings();
        let keyed = element.keyed().is_some();
        let region = element.region().is_some() || keyed;
        let element_id = element.base().id().cloned();
        let measured = element.is_measured();
        let portal = element.portal();
        let autofocus = element.base().autofocus;
        let trap_focus = element.base().trap_focus;
        let outside = !element.base().handlers.click_outside.is_empty();
        let id = self.nodes.insert(Node {
            element,
            parent,
            children: Vec::new(),
            depth,
            layout_id,
            resolved: Style::default(),
            resolved_once: false,
            text_style: TextStyle::default(),
            disabled: false,
            visual: Animated::new(Visual::from_style(&Style::default())),
            hovered: false,
            active: false,
            focused: false,
            observer: None,
            region_observer: None,
            paint_observer: None,
            owner: None,
            animation_start: self.now,
            scroll: None,
            bounds: Bounds::ZERO,
            transform: Transform2D::IDENTITY,
            layout: NodeLayout::default(),
            wants_hitbox: false,
            queued: false,
            painted: false,
            is_region: region,
            portal_children: Vec::new(),
            anchor: None,
            overlay_root: false,
            keyed_children: Vec::new(),
            item_owner,
            restore_focus: if trap_focus { self.focused } else { None },
        });
        if outside {
            self.outside_listeners.insert(id);
        }
        if measured {
            self.layout.set_context(layout_id, Some(id));
        }
        if reactive {
            self.nodes[id].observer = Some(self.make_observer(id));
        }
        if region {
            self.nodes[id].region_observer = Some(self.make_observer(id));
        }
        if self.nodes[id].element.tracks_paint() {
            self.nodes[id].paint_observer = Some(self.make_observer(id));
        }
        if has_animations {
            self.animated_nodes.insert(id);
        }
        if let Some(eid) = element_id {
            if let Some(retained) = self.retained.remove(&eid) {
                if let Some(offset) = retained.scroll {
                    let mut scroll = ScrollState::new();
                    scroll.offset.jump(offset);
                    self.nodes[id].scroll = Some(scroll);
                }
                if retained.focused {
                    self.commands.push(Command::Focus(id));
                }
            }
            self.ids.insert(eid, id);
        }
        if autofocus {
            self.commands.push(Command::Focus(id));
        }
        self.enqueue_style(id);

        if let Some(spec) = portal {
            let anchor = spec.anchor.map(|anchor| AnchorState {
                target: match anchor {
                    crate::elements::Anchor::Parent => {
                        AnchorTarget::Node(self.visual_ancestor(parent).unwrap_or(self.root))
                    }
                    crate::elements::Anchor::Id(eid) => AnchorTarget::Id(eid),
                    crate::elements::Anchor::Point(p) => AnchorTarget::Point(p),
                },
                placement: spec.placement,
                gap: spec.gap,
                match_width: spec.match_width,
            });
            let mut child_ids = Vec::with_capacity(children.len());
            for child in children {
                let child = self.mount(child, Some(id));
                if let Some(anchor) = &anchor
                    && anchor.match_width
                    && let Some(width) = self.anchor_width(anchor)
                {
                    // Known from the last frame: lay out at the right width at once.
                    self.nodes[child]
                        .element
                        .base_mut()
                        .style
                        .layout
                        .min_size
                        .width = kova_native_layout::Length::Px(width);
                }
                self.nodes[child].anchor = anchor.clone();
                self.nodes[child].overlay_root = true;
                child_ids.push(child);
            }
            self.nodes[id].portal_children = child_ids.clone();
            let mut layer = self.nodes[self.overlay].children.clone();
            layer.extend(child_ids);
            self.set_children(self.overlay, layer);
        } else if region {
            self.rebuild_region(id);
        } else {
            let mut child_ids = Vec::with_capacity(children.len());
            for child in children {
                child_ids.push(self.mount(child, Some(id)));
            }
            self.set_children(id, child_ids);
        }
        id
    }

    /// The nearest node at or above `id` that is not a reactive region.
    fn visual_ancestor(&self, mut id: Option<NodeId>) -> Option<NodeId> {
        while let Some(n) = id {
            let node = self.nodes.get(n)?;
            if !node.is_region {
                return Some(n);
            }
            id = node.parent;
        }
        None
    }

    fn make_observer(&self, id: NodeId) -> Observer {
        let queue = self.queue.clone();
        Observer::new(move || {
            let waker = {
                let mut q = queue.borrow_mut();
                q.nodes.push(id);
                q.waker.clone()
            };
            if let Some(w) = waker {
                w();
            }
        })
    }

    fn set_children(&mut self, id: NodeId, children: Vec<NodeId>) {
        let layout_children: Vec<LayoutId> =
            children.iter().map(|c| self.nodes[*c].layout_id).collect();
        self.layout
            .set_children(self.nodes[id].layout_id, &layout_children);
        self.nodes[id].children = children;
        self.needs_layout = true;
    }

    fn nearest_owner(&self, mut id: Option<NodeId>) -> Owner {
        while let Some(n) = id {
            let node = &self.nodes[n];
            if let Some(owner) = node.owner.or(node.item_owner) {
                return owner;
            }
            id = node.parent;
        }
        self.root_owner
    }

    /// The owner under which a region node creates its own owner.
    fn region_parent_owner(&self, id: NodeId) -> Owner {
        self.nodes[id]
            .item_owner
            .unwrap_or_else(|| self.nearest_owner(self.nodes[id].parent))
    }

    fn rebuild_region(&mut self, id: NodeId) {
        if self.nodes[id].element.keyed().is_some() {
            self.rebuild_keyed(id);
            return;
        }
        self.stats.regions_rebuilt += 1;
        let old_children = std::mem::take(&mut self.nodes[id].children);
        for child in old_children {
            self.unmount(child);
        }
        if let Some(owner) = self.nodes[id].owner.take() {
            owner.dispose();
        }
        let parent_owner = self.region_parent_owner(id);
        let owner = parent_owner.with(Owner::new);
        self.nodes[id].owner = Some(owner);
        let elements = {
            let Node {
                element,
                region_observer,
                ..
            } = &mut self.nodes[id];
            let build = element.region().expect("region element");
            match region_observer {
                Some(obs) => obs.track(|| owner.with(build)),
                None => owner.with(build),
            }
        };
        let mut child_ids = Vec::with_capacity(elements.len());
        for element in elements {
            child_ids.push(self.mount(element, Some(id)));
        }
        self.set_children(id, child_ids);
        self.needs_paint = true;
    }

    /// Reconciles a keyed list: kept keys keep their nodes (moved into the
    /// new order), new keys are mounted, removed keys are unmounted.
    fn rebuild_keyed(&mut self, id: NodeId) {
        self.stats.regions_rebuilt += 1;
        let keys = {
            let Node {
                element,
                region_observer,
                ..
            } = &mut self.nodes[id];
            let source = element.keyed().expect("keyed element");
            match region_observer {
                Some(obs) => obs.track(|| source.keys()),
                None => source.keys(),
            }
        };
        let mut previous: FxHashMap<u64, NodeId> =
            std::mem::take(&mut self.nodes[id].keyed_children)
                .into_iter()
                .collect();
        let list_owner = self.nearest_owner(Some(id));
        let mut keyed_children = Vec::with_capacity(keys.len());
        for key in keys {
            let child = match previous.remove(&key) {
                Some(child) if self.nodes.contains_key(child) => child,
                _ => {
                    let owner = list_owner.with(Owner::new);
                    let element = owner.with(|| {
                        kova_native_core::untrack(|| {
                            self.nodes[id]
                                .element
                                .keyed()
                                .expect("keyed element")
                                .build(key)
                        })
                    });
                    self.mount_inner(element, Some(id), Some(owner))
                }
            };
            keyed_children.push((key, child));
        }
        for (_, stale) in previous {
            self.unmount(stale);
        }
        let children = keyed_children.iter().map(|(_, c)| *c).collect();
        self.nodes[id].keyed_children = keyed_children;
        self.set_children(id, children);
        self.needs_paint = true;
    }

    fn unmount(&mut self, id: NodeId) {
        let (children, portal_children) = match self.nodes.get_mut(id) {
            Some(n) => (
                std::mem::take(&mut n.children),
                std::mem::take(&mut n.portal_children),
            ),
            None => return,
        };
        for child in children {
            self.unmount(child);
        }
        if !portal_children.is_empty() {
            for child in &portal_children {
                self.unmount(*child);
            }
            if let Some(layer) = self.nodes.get(self.overlay) {
                let remaining: Vec<NodeId> = layer
                    .children
                    .iter()
                    .copied()
                    .filter(|c| !portal_children.contains(c))
                    .collect();
                self.set_children(self.overlay, remaining);
                self.needs_paint = true;
            }
        }
        self.outside_listeners.remove(&id);
        let Some(node) = self.nodes.remove(id) else {
            return;
        };
        if let Some(eid) = node.element.base().id().cloned() {
            if self.ids.get(&eid) == Some(&id) {
                self.ids.remove(&eid);
            }
            let retained = Retained {
                scroll: node.scroll.as_ref().map(|s| *s.offset.target()),
                focused: self.focused == Some(id),
            };
            if retained.scroll.is_some() || retained.focused {
                self.retained.insert(eid, retained);
            }
        }
        if self.focused == Some(id) {
            self.focused = None;
        }
        if node.element.base().trap_focus {
            // Traps that would restore focus into this one restore to its
            // own target instead (a dialog opened from a menu item).
            for (_, other) in self.nodes.iter_mut() {
                if other.restore_focus == Some(id) {
                    other.restore_focus = node.restore_focus;
                }
            }
            if self.focused.is_none()
                && let Some(previous) = node.restore_focus
                && self.nodes.contains_key(previous)
            {
                self.commands.push(Command::RestoreFocus(previous));
            }
        }
        self.hovered.retain(|n| *n != id);
        if self.drag.as_ref().is_some_and(|d| d.node == id) {
            self.drag = None;
        }
        self.animated_nodes.remove(&id);
        self.layout.remove(node.layout_id);
        if let Some(owner) = node.owner {
            owner.dispose();
        }
        if let Some(owner) = node.item_owner {
            owner.dispose();
        }
        // Observer and element (with its closures) drop here.
    }

    fn enqueue_style(&mut self, id: NodeId) {
        if let Some(node) = self.nodes.get_mut(id)
            && !node.queued
        {
            node.queued = true;
            self.style_queue.push(Reverse((node.depth, id)));
        }
    }

    fn invalidate(&mut self, id: NodeId, dirty: Dirty) {
        if dirty.intersects(Dirty::STYLE | Dirty::BINDINGS) {
            self.enqueue_style(id);
        }
        if dirty.contains(Dirty::LAYOUT)
            && let Some(node) = self.nodes.get(id)
        {
            self.layout.mark_dirty(node.layout_id);
            self.needs_layout = true;
        }
        self.needs_paint = true;
    }

    // ---- frame ------------------------------------------------------------------

    /// Runs the frame pipeline and paints into `cx.scene` (which is cleared).
    pub fn frame(&mut self, cx: &mut FrameContext) -> FrameOutput {
        self.now = cx.now;
        self.stats = FrameStats::default();
        let mut animating = false;
        kova_native_core::timer::set_frame_time(cx.now);
        kova_native_core::timer::run_due(cx.now);
        kova_native_core::task::run_ready();

        self.apply_commands(cx.text, &mut crate::context::MemoryClipboard::default());
        self.flush_reactive();
        // Region remounts can restore focus by stable id. Apply those requests
        // in this frame rather than waiting for an unrelated input event.
        self.apply_commands(cx.text, &mut crate::context::MemoryClipboard::default());

        // Time based animations re-resolve their node's style every frame.
        let animated: Vec<NodeId> = self.animated_nodes.iter().copied().collect();
        for id in animated {
            self.enqueue_style(id);
        }

        self.resolve_styles(cx.text, &mut animating);

        if self.needs_layout {
            self.compute_layout(cx.text);
            self.stats.layout_ran = true;
        }

        cx.scene.clear();
        self.hitboxes.clear();
        self.next_wake = None;
        let root = self.root;
        let parent = ParentPaint {
            origin: Point::ZERO,
            transform: Transform2D::IDENTITY,
            clip: ContentMask::new(Bounds::new(Point::ZERO, self.viewport.scale(self.scale))),
            opacity: 1.0,
            text_color: self.root_text_style.color,
        };
        let overlay_parent = ParentPaint {
            origin: Point::ZERO,
            transform: Transform2D::IDENTITY,
            clip: parent.clip,
            opacity: 1.0,
            text_color: self.root_text_style.color,
        };
        self.paint_node(root, &parent, cx, &mut animating);
        let overlay = self.overlay;
        self.paint_node(overlay, &overlay_parent, cx, &mut animating);
        self.needs_paint = false;
        self.sync_anchor_widths();

        // Content may have moved under a stationary pointer.
        if self.stats.layout_ran && self.drag.is_none() {
            self.update_hover(cx.text);
        }

        // Focus-triggered style changes etc. may need one more frame.
        let pending = !self.style_queue.is_empty() || !self.queue.borrow().nodes.is_empty();
        animating |= pending || !self.animated_nodes.is_empty();

        self.stats.nodes = self.nodes.len();
        FrameOutput {
            animating,
            cursor: self.cursor,
            ime_area: self.ime_area(),
            next_frame: self.next_deadline(),
            stats: self.stats,
        }
    }

    fn flush_reactive(&mut self) {
        for _ in 0..16 {
            let dirty = std::mem::take(&mut self.queue.borrow_mut().nodes);
            if dirty.is_empty() {
                break;
            }
            let mut seen = FxHashSet::default();
            let mut regions = Vec::new();
            for id in dirty {
                if !seen.insert(id) || !self.nodes.contains_key(id) {
                    continue;
                }
                if self.nodes[id]
                    .region_observer
                    .as_ref()
                    .is_some_and(Observer::is_dirty)
                {
                    regions.push(id);
                }
                if self.nodes[id]
                    .observer
                    .as_ref()
                    .is_some_and(Observer::is_dirty)
                    || self.nodes[id]
                        .paint_observer
                        .as_ref()
                        .is_some_and(Observer::is_dirty)
                {
                    self.invalidate(id, Dirty::BINDINGS | Dirty::STYLE);
                }
            }
            // Outer regions first; inner ones may disappear while rebuilding.
            regions.sort_by_key(|id| self.nodes[*id].depth);
            for id in regions {
                if self.nodes.contains_key(id) {
                    self.rebuild_region(id);
                }
            }
        }
    }

    fn resolve_styles(&mut self, text: &mut TextSystem, animating: &mut bool) {
        while let Some(Reverse((_, id))) = self.style_queue.pop() {
            if !self.nodes.contains_key(id) {
                continue;
            }
            self.nodes[id].queued = false;
            self.resolve_node(id, text, animating);
        }
    }

    fn resolve_node(&mut self, id: NodeId, text: &mut TextSystem, animating: &mut bool) {
        self.stats.styles_resolved += 1;
        let now = self.now;
        let (parent_text, parent_disabled, parent_scrolls) = match self.nodes[id].parent {
            Some(p) => {
                let p = &self.nodes[p];
                (
                    p.text_style.clone(),
                    p.disabled,
                    p.resolved.layout.scrolls(),
                )
            }
            None => (self.root_text_style.clone(), false, false),
        };
        let node = &mut self.nodes[id];
        let base = node.element.base();
        let mut style = base.style.clone();
        let mut content_changed = false;
        if let Some(observer) = &node.observer {
            let bound = base.bound_styles.clone();
            let element = &mut node.element;
            let (s, changed) = observer.track(|| {
                let changed = element.update_bindings();
                let mut s = style;
                for f in &bound {
                    s = f(s);
                }
                (s, changed)
            });
            style = s;
            content_changed = changed;
        }
        let base = node.element.base();
        let disabled = style.disabled || parent_disabled;
        if node.hovered && !disabled {
            for f in &base.hover_styles {
                style = f(style);
            }
        }
        if node.active && !disabled {
            for f in &base.active_styles {
                style = f(style);
            }
        }
        if node.focused {
            for f in &base.focus_styles {
                style = f(style);
            }
            if self.keyboard_modality {
                for f in &base.focus_visible_styles {
                    style = f(style);
                }
            }
        }
        if disabled {
            for f in &base.disabled_styles {
                style = f(style);
            }
        }
        if !base.animations.is_empty() {
            let elapsed = now.saturating_duration_since(node.animation_start);
            let mut all_done = true;
            for (animation, f) in &base.animations {
                let (t, done) = animation.sample(elapsed);
                style = f(style, t);
                all_done &= done;
            }
            if all_done {
                self.animated_nodes.remove(&id);
            } else {
                *animating = true;
            }
        }
        let node = &mut self.nodes[id];
        // Overlay layers block the pointer even without handlers, so clicks
        // on a popup's background never reach the content underneath.
        let wants_hitbox =
            (node.element.base().wants_hitbox() || node.overlay_root) && style.pointer_events;

        // Layout.
        let mut layout_style = style.layout.clone();
        if parent_scrolls {
            layout_style.flex_shrink = 0.0;
        }
        let mut relayout = false;
        if !node.resolved_once || layout_style != node.resolved.layout || node.parent.is_none() {
            if node.parent.is_none() {
                layout_style.size = kova_native_layout::Axes {
                    width: self.viewport.width.into(),
                    height: self.viewport.height.into(),
                };
            }
            if node.resolved_once && style.layout.scrolls() != node.resolved.layout.scrolls() {
                relayout = true; // children's flex-shrink rule changes
            }
            self.layout.set_style(node.layout_id, &layout_style);
            self.needs_layout = true;
        }
        if style.layout.scrolls() && node.scroll.is_none() {
            node.scroll = Some(ScrollState::new());
        }

        // Cascading text style.
        let text_style = style.text.apply(&parent_text);
        let text_changed = !node.resolved_once || text_style != node.text_style;
        let disabled_changed = disabled != node.disabled;
        if text_changed {
            node.text_style = text_style.clone();
            let mut mcx = MeasureCx {
                text,
                scale: self.scale,
            };
            if node.element.text_style_changed(&mut mcx, &text_style) {
                content_changed = true;
            }
        }
        if content_changed {
            self.layout.mark_dirty(node.layout_id);
            self.needs_layout = true;
        }

        // Visual transition.
        let target = Visual::from_style(&style);
        if node.resolved_once {
            let transition = style.transition.unwrap_or_else(Transition::instant);
            if node.visual.set(target, transition, now) {
                *animating = true;
            }
        } else {
            node.visual.jump(target);
        }
        let layout_kept = layout_style.clone();
        node.resolved = Style {
            layout: layout_kept,
            ..style
        };
        node.resolved_once = true;
        node.disabled = disabled;
        node.wants_hitbox = wants_hitbox;
        self.needs_paint = true;

        if text_changed || disabled_changed || relayout {
            let node = &self.nodes[id];
            let children: Vec<NodeId> = node
                .children
                .iter()
                .chain(&node.portal_children)
                .copied()
                .collect();
            for c in children {
                self.enqueue_style(c);
            }
        }
    }

    fn compute_layout(&mut self, text: &mut TextSystem) {
        let root_layout = self.nodes[self.root].layout_id;
        let overlay_layout = self.nodes[self.overlay].layout_id;
        let ElementTree {
            layout,
            nodes,
            viewport,
            scale,
            ..
        } = self;
        let mut mcx = MeasureCx {
            text,
            scale: *scale,
        };
        for top in [root_layout, overlay_layout] {
            layout.compute(top, *viewport, |id, input| match nodes.get_mut(id) {
                Some(node) => node.element.measure(&mut mcx, input),
                None => Size::ZERO,
            });
        }
        self.needs_layout = false;
    }

    fn paint_node(
        &mut self,
        id: NodeId,
        parent: &ParentPaint,
        cx: &mut FrameContext,
        animating: &mut bool,
    ) {
        let now = self.now;
        let scale = self.scale;
        let Some(layout_id) = self.nodes.get(id).map(|n| n.layout_id) else {
            return;
        };
        let layout = self.layout.layout(layout_id);
        let anchored = self.nodes[id]
            .anchor
            .as_ref()
            .map(|anchor| self.anchor_origin(anchor, layout.size));
        let node = &mut self.nodes[id];
        node.painted = false;
        if node.resolved.layout.display == kova_native_layout::Display::None {
            return;
        }
        let origin = match anchored {
            // The anchor is not visible this frame: neither is the overlay.
            Some(None) => return,
            Some(Some(origin)) => origin,
            None => parent.origin + layout.location,
        };
        let bounds = Bounds::new(origin, layout.size);
        node.bounds = bounds;
        node.layout = layout;
        let visual = node.visual.get(now);
        if node.visual.tick(now) {
            *animating = true;
        }
        let opacity = parent.opacity * visual.opacity;
        if opacity <= 0.002 {
            return;
        }

        // Transform (scale/rotate around the origin, then translate).
        let mut transform = parent.transform;
        if visual.has_transform() {
            let style = &node.resolved;
            let pivot = Point::new(
                bounds.origin.x + bounds.size.width * style.transform_origin.x,
                bounds.origin.y + bounds.size.height * style.transform_origin.y,
            );
            let local = Transform2D::scale(visual.scale, visual.scale)
                .then(&Transform2D::rotate(visual.rotate.to_radians()))
                .around(pivot)
                .then(&Transform2D::translate(
                    visual.translate.x,
                    visual.translate.y,
                ));
            transform = local.then(&parent.transform);
        }
        node.transform = transform;
        let text_color = visual.text_color.unwrap_or(parent.text_color);
        let radii = visual.corner_radii;
        let style = &node.resolved;
        let border = style.layout.border;
        let padding = node.layout.padding;
        let content_bounds = bounds.inset(kova_native_core::Edges {
            top: border.top + padding.top,
            right: border.right + padding.right,
            bottom: border.bottom + padding.bottom,
            left: border.left + padding.left,
        });
        let clips = style.layout.clips();
        let backdrop = style.backdrop_blur;
        let mask = style.mask.clone();
        let text_style = node.text_style.clone();
        let mut pcx = PaintCx {
            scene: cx.scene,
            text: cx.text,
            atlas: cx.atlas,
            scale,
            transform,
            clip: parent.clip,
            opacity,
            bounds,
            content_bounds,
            radii,
            text_style,
            text_color,
            now,
            animating,
            wake_at: None,
            focused: node.focused,
            hovered: node.hovered,
        };

        // Cull subtrees that clip and are entirely outside the visible area.
        let device_bounds = pcx.transform_bounds_to_device(bounds);
        let visible = device_bounds
            .inflate(64.0 * scale)
            .intersects(&parent.clip.bounds);
        if clips && !visible {
            return;
        }
        node.painted = true;
        self.stats.nodes_painted += 1;
        if mask.is_some() {
            pcx.push_layer();
        }

        for shadow in visual.shadows.iter().filter(|s| !s.inset) {
            pcx.paint_shadow(bounds, radii, shadow);
        }
        if let Some(blur) = backdrop {
            pcx.paint_backdrop_blur(bounds, radii, blur, Color::TRANSPARENT);
        }
        pcx.paint_quad(
            bounds,
            radii,
            visual.background,
            border,
            visual.border_color,
        );
        for shadow in visual.shadows.iter().filter(|s| s.inset) {
            pcx.paint_shadow(bounds, radii, shadow);
        }
        {
            let Node {
                element,
                paint_observer,
                ..
            } = &mut *node;
            match paint_observer {
                Some(observer) => observer.track(|| element.paint(&mut pcx)),
                None => element.paint(&mut pcx),
            }
        }
        merge_wake(&mut self.next_wake, pcx.wake_at.take());

        let clip_logical = Bounds::new(
            parent.clip.bounds.origin / scale,
            parent.clip.bounds.size.scale(1.0 / scale),
        );
        // A singular transform (e.g. scale(0)) has no hittable area.
        if node.wants_hitbox
            && let Some(inverse) = transform.inverse()
        {
            self.hitboxes.push(Hitbox {
                node: id,
                bounds,
                inverse: Some(inverse),
                clip: clip_logical,
            });
        }

        let child_clip = if clips {
            let inner = pcx.transform_bounds_to_device(bounds);
            let mask = ContentMask::rounded(inner, radii.scale(scale * transform.approx_scale()));
            parent.clip.intersect(&mask)
        } else {
            parent.clip
        };
        let (scroll_offset, scroll_animating) = match &mut node.scroll {
            Some(scroll) => {
                let content = node.layout.content_size;
                let view = Size::new(
                    (bounds.size.width - border.left - border.right).max(0.0),
                    (bounds.size.height - border.top - border.bottom).max(0.0),
                );
                let style = &node.resolved.layout;
                scroll.max = Point::new(
                    if style.overflow_x == kova_native_layout::Overflow::Scroll {
                        (content.width - view.width).max(0.0)
                    } else {
                        0.0
                    },
                    if style.overflow_y == kova_native_layout::Overflow::Scroll {
                        (content.height - view.height).max(0.0)
                    } else {
                        0.0
                    },
                );
                let target = *scroll.offset.target();
                let clamped = Point::new(
                    target.x.clamp(0.0, scroll.max.x),
                    target.y.clamp(0.0, scroll.max.y),
                );
                if clamped != target {
                    scroll.offset.jump(clamped);
                }
                (scroll.offset.get(now), scroll.offset.tick(now))
            }
            None => (Point::ZERO, false),
        };
        if scroll_animating {
            *animating = true;
        }
        let child_parent = ParentPaint {
            origin: bounds.origin - scroll_offset,
            transform,
            clip: child_clip,
            opacity,
            text_color,
        };
        let children = node.children.clone();
        for child in children {
            self.paint_node(child, &child_parent, cx, animating);
        }

        // Overlay: element overlay and scrollbars, above children.
        let Some(node) = self.nodes.get_mut(id) else {
            if mask.is_some() {
                cx.scene.pop_layer(
                    Bounds::ZERO,
                    kova_native_render::LayerMask::None,
                    &Default::default(),
                );
            }
            return;
        };
        let mut pcx = PaintCx {
            scene: cx.scene,
            text: cx.text,
            atlas: cx.atlas,
            scale,
            transform,
            clip: child_clip,
            opacity,
            bounds,
            content_bounds,
            radii,
            text_style: node.text_style.clone(),
            text_color,
            now,
            animating,
            wake_at: None,
            focused: node.focused,
            hovered: node.hovered,
        };
        node.element.paint_overlay(&mut pcx);
        if let Some(scroll) = &node.scroll {
            paint_scrollbars(
                &mut pcx,
                scroll,
                bounds,
                node.layout.content_size,
                scroll_offset,
                node.hovered,
            );
        }
        if let Some(mask) = &mask {
            // The mask covers the border box at the element's (unclipped) transform.
            pcx.clip = parent.clip;
            pcx.pop_layer(bounds, mask);
        }
        merge_wake(&mut self.next_wake, pcx.wake_at.take());
    }

    /// Window position for an anchored overlay of `size`, or `None` when
    /// its anchor element is missing or was not painted.
    fn anchor_origin(&self, anchor: &AnchorState, size: Size) -> Option<Point> {
        let rect = match &anchor.target {
            AnchorTarget::Point(p) => Bounds::new(*p, Size::ZERO),
            AnchorTarget::Node(n) => self.painted_bounds(*n)?,
            AnchorTarget::Id(eid) => self.painted_bounds(*self.ids.get(eid)?)?,
        };
        Some(crate::elements::place(
            rect,
            size,
            anchor.placement,
            anchor.gap,
            self.viewport,
        ))
    }

    fn anchor_width(&self, anchor: &AnchorState) -> Option<f32> {
        let node = match &anchor.target {
            AnchorTarget::Point(_) => return None,
            AnchorTarget::Node(n) => *n,
            AnchorTarget::Id(eid) => *self.ids.get(eid)?,
        };
        self.visual_bounds(node).map(|b| b.width())
    }

    /// Keeps `match_width` overlays as wide as their anchor when it resizes.
    fn sync_anchor_widths(&mut self) {
        let overlays = self.nodes[self.overlay].children.clone();
        for id in overlays {
            let Some(anchor) = self.nodes[id].anchor.clone() else {
                continue;
            };
            if !anchor.match_width {
                continue;
            }
            let Some(width) = self.anchor_width(&anchor) else {
                continue;
            };
            let want = kova_native_layout::Length::Px(width);
            let node = &mut self.nodes[id];
            if node.element.base().style.layout.min_size.width != want {
                node.element.base_mut().style.layout.min_size.width = want;
                self.enqueue_style(id);
            }
        }
    }

    fn scroll_into_view(&mut self, eid: &ElementId) {
        let Some(&target) = self.ids.get(eid) else {
            return;
        };
        let Some(mut want) = self.visual_bounds(target) else {
            return;
        };
        let now = self.now;
        let mut current = self.nodes[target].parent;
        while let Some(id) = current {
            let node = &mut self.nodes[id];
            current = node.parent;
            let Some(scroll) = &mut node.scroll else {
                continue;
            };
            let view = node.bounds;
            let offset = *scroll.offset.target();
            let mut next = offset;
            if want.top() < view.top() {
                next.y -= view.top() - want.top();
            } else if want.bottom() > view.bottom() {
                next.y += (want.bottom() - view.bottom()).min(want.top() - view.top());
            }
            if want.left() < view.left() {
                next.x -= view.left() - want.left();
            } else if want.right() > view.right() {
                next.x += (want.right() - view.right()).min(want.left() - view.left());
            }
            next = Point::new(
                next.x.clamp(0.0, scroll.max.x),
                next.y.clamp(0.0, scroll.max.y),
            );
            if next != offset {
                scroll.offset.set(
                    next,
                    Transition::new(Duration::from_millis(120)).easing(Easing::EaseOutCubic),
                    now,
                );
                scroll.last_activity = Some(now);
                self.needs_paint = true;
                // Outer containers see the target where this one will put it.
                want = want.translate(offset - next);
            }
        }
    }

    fn painted_bounds(&self, id: NodeId) -> Option<Bounds> {
        self.visual_bounds(id)
    }

    /// Window bounds of a node as painted in the last frame (after visual
    /// transforms), or `None` if it was not painted.
    pub fn visual_bounds(&self, id: NodeId) -> Option<Bounds> {
        let node = self.nodes.get(id)?;
        node.painted
            .then(|| node.transform.apply_bounds(&node.bounds))
    }

    /// Nodes painted in the last frame, in paint order (root, then overlay).
    pub fn painted_nodes(&self) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut stack = vec![self.overlay, self.root];
        while let Some(id) = stack.pop() {
            let Some(node) = self.nodes.get(id) else {
                continue;
            };
            if !node.painted {
                continue;
            }
            out.push(id);
            stack.extend(node.children.iter().rev());
        }
        out
    }

    /// The text shown by a text element.
    pub fn text_content(&self, id: NodeId) -> Option<&str> {
        self.nodes.get(id)?.element.text_content()
    }

    /// The element id of a node, if it has one.
    pub fn element_id(&self, id: NodeId) -> Option<&ElementId> {
        self.nodes.get(id)?.element.base().id()
    }

    /// Forces a full repaint on the next frame (e.g. after switching atlases).
    pub fn request_repaint(&mut self) {
        self.needs_paint = true;
    }

    fn ime_area(&self) -> Option<Bounds> {
        let id = self.focused?;
        let node = self.nodes.get(id)?;
        if !node.painted || !node.element.accepts_text_input() {
            return None;
        }
        let area = node.element.ime_cursor_area().unwrap_or(Bounds::new(
            Point::ZERO,
            Size::new(1.0, node.bounds.height()),
        ));
        Some(
            node.transform
                .apply_bounds(&area.translate(node.bounds.origin)),
        )
    }

    // ---- event dispatch ---------------------------------------------------------

    fn path_to(&self, target: NodeId) -> Vec<NodeId> {
        let mut path = Vec::new();
        let mut current = Some(target);
        while let Some(id) = current {
            path.push(id);
            current = self.nodes.get(id).and_then(|n| n.parent);
        }
        path.reverse();
        path
    }

    fn is_interactive(&self, id: NodeId) -> bool {
        self.nodes.get(id).is_some_and(|n| !n.disabled)
    }

    fn event_cx<'a>(
        &self,
        id: NodeId,
        phase: DispatchPhase,
        commands: &'a mut Vec<Command>,
        clipboard: &'a mut dyn Clipboard,
        text: &'a mut TextSystem,
        now: Instant,
    ) -> EventCx<'a> {
        EventCx {
            node: id,
            bounds: self.nodes.get(id).map_or(Bounds::ZERO, |n| n.bounds),
            transform: self
                .nodes
                .get(id)
                .map_or(Transform2D::IDENTITY, |n| n.transform),
            phase,
            focused: self.focused == Some(id),
            propagation_stopped: false,
            default_prevented: false,
            commands,
            clipboard,
            text,
            now,
        }
    }

    /// Dispatches an event through capture (root → target) and bubble
    /// (target → root) phases. Returns `(stopped, default_prevented)`.
    fn dispatch_phases<E>(
        &mut self,
        path: &[NodeId],
        event: &E,
        raw: Option<&InputEvent>,
        handlers: impl Fn(&crate::element::Handlers) -> Vec<(DispatchPhase, crate::element::EventFn<E>)>,
        env: &mut DispatchContext,
    ) -> (bool, bool) {
        let mut commands = std::mem::take(&mut self.commands);
        let mut prevented = false;
        let mut stopped = false;
        let target = path.last().copied();
        'outer: for phase in [DispatchPhase::Capture, DispatchPhase::Bubble] {
            let order: NodeList = match phase {
                DispatchPhase::Capture => path.iter().copied().collect(),
                DispatchPhase::Bubble => path.iter().rev().copied().collect(),
            };
            for id in order {
                if !self.is_interactive(id) {
                    continue;
                }
                if phase == DispatchPhase::Bubble
                    && Some(id) == target
                    && !prevented
                    && let Some(raw) = raw
                {
                    let mut cx =
                        self.event_cx(id, phase, &mut commands, env.clipboard, env.text, env.now);
                    let node = self.nodes.get_mut(id).expect("node");
                    node.element.handle_event(&mut cx, raw);
                    prevented |= cx.default_prevented;
                    if cx.propagation_stopped {
                        stopped = true;
                        break 'outer;
                    }
                }
                let list: Vec<_> = handlers(&self.nodes[id].element.base().handlers)
                    .into_iter()
                    .filter(|(p, _)| *p == phase)
                    .map(|(_, h)| h)
                    .collect();
                for handler in list {
                    let mut cx =
                        self.event_cx(id, phase, &mut commands, env.clipboard, env.text, env.now);
                    handler(event, &mut cx);
                    prevented |= cx.default_prevented;
                    if cx.propagation_stopped {
                        stopped = true;
                        break 'outer;
                    }
                }
            }
        }
        self.commands = commands;
        (stopped, prevented)
    }

    /// Runs simple (non-phased) handlers bubbling from `path`'s end.
    fn bubble_simple(
        &mut self,
        path: &[NodeId],
        handlers: impl Fn(&crate::element::Handlers) -> Vec<crate::element::ClickFn>,
        env: &mut DispatchContext,
    ) -> bool {
        let mut commands = std::mem::take(&mut self.commands);
        let mut handled = false;
        'outer: for &id in path.iter().rev() {
            if !self.is_interactive(id) {
                continue;
            }
            let list = handlers(&self.nodes[id].element.base().handlers);
            for handler in list {
                handled = true;
                let mut cx = self.event_cx(
                    id,
                    DispatchPhase::Bubble,
                    &mut commands,
                    env.clipboard,
                    env.text,
                    env.now,
                );
                handler(&mut cx);
                if cx.propagation_stopped {
                    break 'outer;
                }
            }
        }
        self.commands = commands;
        handled
    }

    /// Runs `on_click_outside` handlers of nodes not on the pressed path.
    fn notify_click_outside(&mut self, path: &[NodeId], env: &mut DispatchContext) {
        if self.outside_listeners.is_empty() {
            return;
        }
        let mut targets: Vec<NodeId> = self
            .outside_listeners
            .iter()
            .copied()
            .filter(|id| !path.contains(id) && self.is_interactive(*id))
            .collect();
        // Innermost first, deterministic among equals.
        targets.sort_by_key(|id| (Reverse(self.nodes[*id].depth), *id));
        let mut commands = std::mem::take(&mut self.commands);
        for id in targets {
            let Some(node) = self.nodes.get(id) else {
                continue;
            };
            let list = node.element.base().handlers.click_outside.clone();
            for handler in list {
                let mut cx = self.event_cx(
                    id,
                    DispatchPhase::Bubble,
                    &mut commands,
                    env.clipboard,
                    env.text,
                    env.now,
                );
                handler(&mut cx);
            }
        }
        self.commands = commands;
    }

    fn call_bool_handlers(
        &mut self,
        id: NodeId,
        value: bool,
        pick: impl Fn(&crate::element::Handlers) -> Vec<crate::element::BoolFn>,
        text: &mut TextSystem,
        clipboard: &mut dyn Clipboard,
        now: Instant,
    ) {
        let Some(node) = self.nodes.get(id) else {
            return;
        };
        let list = pick(&node.element.base().handlers);
        if list.is_empty() {
            return;
        }
        let mut commands = std::mem::take(&mut self.commands);
        for handler in list {
            let mut cx = self.event_cx(
                id,
                DispatchPhase::Bubble,
                &mut commands,
                clipboard,
                text,
                now,
            );
            handler(value, &mut cx);
        }
        self.commands = commands;
    }

    fn call_drag_handlers(
        &mut self,
        id: NodeId,
        event: &DragEvent,
        pick: impl Fn(&crate::element::Handlers) -> Vec<crate::element::EventFn<DragEvent>>,
        env: &mut DispatchContext,
    ) {
        let Some(node) = self.nodes.get(id) else {
            return;
        };
        let list = pick(&node.element.base().handlers);
        let mut commands = std::mem::take(&mut self.commands);
        for handler in list {
            let mut cx = self.event_cx(
                id,
                DispatchPhase::Bubble,
                &mut commands,
                env.clipboard,
                env.text,
                env.now,
            );
            handler(event, &mut cx);
        }
        self.commands = commands;
    }

    fn update_hover(&mut self, text: &mut TextSystem) {
        let target = self.mouse.and_then(|p| self.hit_test(p));
        let new_path = target.map(|t| self.path_to(t)).unwrap_or_default();
        if new_path == self.hovered {
            return;
        }
        let old_path = std::mem::replace(&mut self.hovered, new_path.clone());
        let mut clipboard = crate::context::MemoryClipboard::default();
        let now = self.now;
        for id in old_path.iter().filter(|id| !new_path.contains(id)) {
            if let Some(node) = self.nodes.get_mut(*id) {
                node.hovered = false;
                if node.element.base().has_state_styles() {
                    self.enqueue_style(*id);
                }
                self.call_bool_handlers(*id, false, |h| h.hover.clone(), text, &mut clipboard, now);
            }
        }
        for id in new_path.iter().filter(|id| !old_path.contains(id)) {
            if let Some(node) = self.nodes.get_mut(*id) {
                node.hovered = true;
                if node.element.base().has_state_styles() {
                    self.enqueue_style(*id);
                }
                self.call_bool_handlers(*id, true, |h| h.hover.clone(), text, &mut clipboard, now);
            }
        }
        self.cursor = self
            .hovered
            .iter()
            .rev()
            .find_map(|id| {
                let n = self.nodes.get(*id)?;
                if n.disabled && n.resolved.cursor.is_some() {
                    Some(CursorStyle::NotAllowed)
                } else {
                    n.resolved.cursor
                }
            })
            .unwrap_or(CursorStyle::Arrow);
        self.needs_paint = true;
    }

    fn set_focus(
        &mut self,
        new: Option<NodeId>,
        text: &mut TextSystem,
        clipboard: &mut dyn Clipboard,
    ) {
        let new = new.filter(|id| self.nodes.contains_key(*id));
        if self.focused == new {
            return;
        }
        let now = self.now;
        if let Some(old) = self.focused.take()
            && let Some(node) = self.nodes.get_mut(old)
        {
            node.focused = false;
            if has_focus_styles(node) {
                self.enqueue_style(old);
            }
            self.needs_paint = true;
            self.call_bool_handlers(old, false, |h| h.focus.clone(), text, clipboard, now);
        }
        if let Some(id) = new
            && let Some(node) = self.nodes.get_mut(id)
        {
            node.focused = true;
            self.focused = Some(id);
            if has_focus_styles(node) {
                self.enqueue_style(id);
            }
            self.needs_paint = true;
            self.call_bool_handlers(id, true, |h| h.focus.clone(), text, clipboard, now);
        }
    }

    /// Focusable nodes in tab order.
    /// Focusable nodes in Tab order. While a focus trap is mounted, only
    /// nodes inside the most recently painted trap are included.
    fn focus_order(&self) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut trap = None;
        // Overlay content follows the root, matching paint order.
        let mut stack = vec![self.overlay, self.root];
        let mut seen = 0usize;
        while let Some(id) = stack.pop() {
            let Some(node) = self.nodes.get(id) else {
                continue;
            };
            if node.resolved.layout.display == kova_native_layout::Display::None {
                continue;
            }
            let base = node.element.base();
            if base.trap_focus {
                trap = Some(id);
            }
            if base.focusable && !node.disabled && base.tab_index >= 0 {
                out.push((base.tab_index, seen, id));
            }
            seen += 1;
            stack.extend(node.children.iter().rev());
        }
        out.sort_by_key(|(tab, order, _)| (*tab, *order));
        out.into_iter()
            .map(|(_, _, id)| id)
            .filter(|id| trap.is_none_or(|trap| self.is_inside(*id, trap)))
            .collect()
    }

    /// Whether `id` is `ancestor` or one of its logical descendants.
    fn is_inside(&self, mut id: NodeId, ancestor: NodeId) -> bool {
        loop {
            if id == ancestor {
                return true;
            }
            match self.nodes.get(id).and_then(|n| n.parent) {
                Some(parent) => id = parent,
                None => return false,
            }
        }
    }

    fn move_focus(&mut self, forward: bool, text: &mut TextSystem, clipboard: &mut dyn Clipboard) {
        let order = self.focus_order();
        if order.is_empty() {
            return;
        }
        let index = self
            .focused
            .and_then(|f| order.iter().position(|id| *id == f));
        let next = match (index, forward) {
            (None, true) => 0,
            (None, false) => order.len() - 1,
            (Some(i), true) => (i + 1) % order.len(),
            (Some(i), false) => (i + order.len() - 1) % order.len(),
        };
        self.set_focus(Some(order[next]), text, clipboard);
    }

    fn apply_commands(
        &mut self,
        text: &mut TextSystem,
        clipboard: &mut dyn Clipboard,
    ) -> Vec<Box<dyn Action>> {
        let mut actions = Vec::new();
        for _ in 0..8 {
            let commands = std::mem::take(&mut self.commands);
            if commands.is_empty() {
                break;
            }
            for command in commands {
                match command {
                    Command::Focus(id) => self.set_focus(Some(id), text, clipboard),
                    Command::RestoreFocus(id) => {
                        if self.focused.is_none() {
                            self.set_focus(Some(id), text, clipboard);
                        }
                    }
                    Command::FocusId(eid) => {
                        let id = self.ids.get(&eid).copied();
                        self.set_focus(id, text, clipboard);
                    }
                    Command::Blur => self.set_focus(None, text, clipboard),
                    Command::FocusNext => self.move_focus(true, text, clipboard),
                    Command::FocusPrev => self.move_focus(false, text, clipboard),
                    Command::Repaint(id) => self.invalidate(id, Dirty::PAINT),
                    Command::Relayout(id) => self.invalidate(id, Dirty::LAYOUT),
                    Command::ScrollIntoView(eid) => self.scroll_into_view(&eid),
                    Command::DispatchAction(action) => actions.push(action),
                    Command::Window(cmd) => self.window_commands.push(cmd),
                }
            }
        }
        actions
    }

    fn dispatch_action_on_path(
        &mut self,
        path: &[NodeId],
        action: &dyn Action,
        env: &mut DispatchContext,
    ) -> bool {
        let ty = action.action_type();
        let mut commands = std::mem::take(&mut self.commands);
        let mut handled = false;
        'outer: for &id in path.iter().rev() {
            if !self.is_interactive(id) {
                continue;
            }
            let list: Vec<_> = self.nodes[id]
                .element
                .base()
                .handlers
                .actions
                .iter()
                .filter(|(t, _)| *t == ty)
                .map(|(_, h)| h.clone())
                .collect();
            // The innermost handler for the action type wins.
            if let Some(handler) = list.into_iter().next() {
                let mut cx = self.event_cx(
                    id,
                    DispatchPhase::Bubble,
                    &mut commands,
                    env.clipboard,
                    env.text,
                    env.now,
                );
                handler(action, &mut cx);
                handled = true;
                break 'outer;
            }
        }
        self.commands = commands;
        handled
    }

    /// Switches between pointer and keyboard interaction, restyling the
    /// focused node when its `focus_visible` styles start or stop applying.
    fn set_keyboard_modality(&mut self, keyboard: bool) {
        if self.keyboard_modality == keyboard {
            return;
        }
        self.keyboard_modality = keyboard;
        if let Some(id) = self.focused
            && self
                .nodes
                .get(id)
                .is_some_and(|n| !n.element.base().focus_visible_styles.is_empty())
        {
            self.enqueue_style(id);
        }
    }

    /// Whether focus rings (`focus_visible` styles) are currently shown.
    pub fn keyboard_modality(&self) -> bool {
        self.keyboard_modality
    }

    /// The path keyboard events and actions travel: to the focused node, or
    /// to the application's root element when nothing is focused (so its
    /// `on_action`/`on_key_down` handlers act as window-wide shortcuts).
    fn focus_path(&self) -> Vec<NodeId> {
        let target = self.focused.unwrap_or_else(|| {
            self.nodes[self.root]
                .children
                .first()
                .copied()
                .unwrap_or(self.root)
        });
        self.path_to(target)
    }

    /// Dispatches an action along the current focus path.
    pub fn dispatch_action(&mut self, action: &dyn Action, env: &mut DispatchContext) -> bool {
        let path = self.focus_path();
        self.dispatch_action_on_path(&path, action, env)
    }

    /// Dispatches an input event (window coordinates, logical px).
    pub fn dispatch(&mut self, event: &InputEvent, env: &mut DispatchContext) -> DispatchResult {
        self.now = env.now;
        kova_native_core::timer::set_frame_time(env.now);
        let mut result = DispatchResult::default();
        match event {
            InputEvent::MouseMove(e) => {
                self.mouse = Some(e.position);
                if let Some(drag) = &mut self.drag {
                    let node = drag.node;
                    let started = drag.started || drag.start.distance(e.position) > DRAG_THRESHOLD;
                    let first = !drag.started && started;
                    drag.started = started;
                    if started {
                        let event = DragEvent {
                            button: drag.button,
                            start: drag.start,
                            position: e.position,
                            delta: e.position - drag.last,
                        };
                        drag.last = e.position;
                        if first {
                            self.call_drag_handlers(node, &event, |h| h.drag_start.clone(), env);
                        }
                        self.call_drag_handlers(node, &event, |h| h.drag.clone(), env);
                        result.handled = true;
                    }
                }
                if self.drag.as_ref().is_none_or(|d| !d.started) {
                    self.update_hover(env.text);
                }
                if let Some(target) = self.hit_test(e.position) {
                    let path = self.path_to(target);
                    let (stopped, _) =
                        self.dispatch_phases(&path, e, Some(event), |h| h.mouse_move.clone(), env);
                    result.handled |= stopped;
                }
            }
            InputEvent::MouseExit => {
                self.mouse = None;
                self.update_hover(env.text);
            }
            InputEvent::MouseDown(e) => {
                self.set_keyboard_modality(false);
                self.mouse = Some(e.position);
                self.update_hover(env.text);
                let path = self
                    .hit_test(e.position)
                    .map(|t| self.path_to(t))
                    .unwrap_or_else(|| vec![self.root]);
                self.notify_click_outside(&path, env);
                let (_, prevented) =
                    self.dispatch_phases(&path, e, Some(event), |h| h.mouse_down.clone(), env);
                result.handled = true;
                for id in &path {
                    if let Some(node) = self.nodes.get_mut(*id)
                        && !node.disabled
                    {
                        node.active = true;
                        if !node.element.base().active_styles.is_empty() {
                            self.enqueue_style(*id);
                        }
                    }
                }
                if !prevented && e.button == MouseButton::Left {
                    let focus_target = path
                        .iter()
                        .rev()
                        .find(|id| {
                            self.nodes
                                .get(**id)
                                .is_some_and(|n| n.element.base().focusable && !n.disabled)
                        })
                        .copied();
                    self.set_focus(focus_target, env.text, env.clipboard);
                }
                let drag_node = path
                    .iter()
                    .rev()
                    .find(|id| {
                        self.nodes
                            .get(**id)
                            .is_some_and(|n| n.element.base().handlers.has_drag() && !n.disabled)
                    })
                    .copied();
                self.drag = drag_node.map(|node| DragState {
                    node,
                    button: e.button,
                    start: e.position,
                    last: e.position,
                    started: false,
                });
                self.pressed = Some(Pressed {
                    button: e.button,
                    path,
                });
                self.needs_paint = true;
            }
            InputEvent::MouseUp(e) => {
                self.mouse = Some(e.position);
                let target = self.hit_test(e.position);
                let up_path = target
                    .map(|t| self.path_to(t))
                    .unwrap_or_else(|| vec![self.root]);
                self.dispatch_phases(&up_path, e, Some(event), |h| h.mouse_up.clone(), env);
                let dragged = match self.drag.take() {
                    Some(drag) if drag.started => {
                        let event = DragEvent {
                            button: drag.button,
                            start: drag.start,
                            position: e.position,
                            delta: e.position - drag.last,
                        };
                        self.call_drag_handlers(drag.node, &event, |h| h.drag_end.clone(), env);
                        true
                    }
                    _ => false,
                };
                if let Some(pressed) = self.pressed.take() {
                    for id in &pressed.path {
                        if let Some(node) = self.nodes.get_mut(*id)
                            && node.active
                        {
                            node.active = false;
                            if !node.element.base().active_styles.is_empty() {
                                self.enqueue_style(*id);
                            }
                        }
                    }
                    if pressed.button == e.button && e.button == MouseButton::Left && !dragged {
                        let common: Vec<NodeId> = pressed
                            .path
                            .iter()
                            .zip(up_path.iter())
                            .take_while(|(a, b)| a == b)
                            .map(|(a, _)| *a)
                            .collect();
                        if !common.is_empty() {
                            result.handled |= self.bubble_simple(&common, |h| h.click.clone(), env);
                            if e.click_count == 2 {
                                result.handled |=
                                    self.bubble_simple(&common, |h| h.double_click.clone(), env);
                            }
                        }
                    }
                }
                self.update_hover(env.text);
                self.needs_paint = true;
            }
            InputEvent::ScrollWheel(e) => {
                self.mouse = Some(e.position);
                let path = self
                    .hit_test(e.position)
                    .map(|t| self.path_to(t))
                    .unwrap_or_default();
                let (stopped, prevented) =
                    self.dispatch_phases(&path, e, Some(event), |h| h.scroll.clone(), env);
                if !stopped && !prevented {
                    let mut delta = e.delta.pixels(SCROLL_LINE);
                    if e.modifiers.shift && delta.x == 0.0 {
                        delta = Point::new(delta.y, 0.0);
                    }
                    let smooth = matches!(e.delta, ScrollDelta::Lines(_));
                    for id in path.iter().rev() {
                        if self.scroll_node(*id, delta, smooth) {
                            result.handled = true;
                            break;
                        }
                    }
                }
            }
            InputEvent::Pen(e) => {
                let path = self
                    .hit_test(e.position)
                    .map(|target| self.path_to(target))
                    .unwrap_or_else(|| vec![self.root]);
                let (stopped, _) =
                    self.dispatch_phases(&path, e, Some(event), |h| h.pen.clone(), env);
                result.handled = stopped;
            }
            InputEvent::KeyDown(e) => {
                if !e.keystroke.modifiers.is_command_like()
                    || matches!(e.keystroke.key, Key::Named(NamedKey::Tab))
                {
                    self.set_keyboard_modality(true);
                }
                result = self.dispatch_key_down(e, event, env);
            }
            InputEvent::KeyUp(e) => {
                let path = self.focus_path();
                let (stopped, _) =
                    self.dispatch_phases(&path, e, Some(event), |h| h.key_up.clone(), env);
                result.handled = stopped;
            }
            InputEvent::TextInput(_) | InputEvent::Ime(_) => {
                if let Some(id) = self.focused {
                    self.deliver_builtin(id, event, env);
                    result.handled = true;
                }
            }
            InputEvent::ModifiersChanged(_) => {}
        }
        result
            .unhandled_actions
            .extend(self.apply_commands(env.text, env.clipboard));
        // Actions requested by handlers go through the focus path too.
        let pending = std::mem::take(&mut result.unhandled_actions);
        for action in pending {
            if !self.dispatch_action(action.as_ref(), env) {
                result.unhandled_actions.push(action);
            }
        }
        result
    }

    fn deliver_builtin(
        &mut self,
        id: NodeId,
        event: &InputEvent,
        env: &mut DispatchContext,
    ) -> (bool, bool) {
        let mut commands = std::mem::take(&mut self.commands);
        let mut cx = self.event_cx(
            id,
            DispatchPhase::Bubble,
            &mut commands,
            env.clipboard,
            env.text,
            env.now,
        );
        let flags = match self.nodes.get_mut(id) {
            Some(node) if !node.disabled => {
                node.element.handle_event(&mut cx, event);
                (cx.propagation_stopped, cx.default_prevented)
            }
            _ => (false, false),
        };
        self.commands = commands;
        flags
    }

    fn dispatch_key_down(
        &mut self,
        e: &KeyDownEvent,
        raw: &InputEvent,
        env: &mut DispatchContext,
    ) -> DispatchResult {
        let mut result = DispatchResult::default();
        let path = self.focus_path();
        let contexts: SmallVec<[&'static str; 4]> = path
            .iter()
            .filter_map(|id| {
                self.nodes
                    .get(*id)
                    .and_then(|n| n.element.base().key_context)
            })
            .collect();
        match env.keymap.feed(&e.keystroke, &contexts) {
            KeymapMatch::Pending => {
                result.handled = true;
                return result;
            }
            KeymapMatch::Matched(actions) => {
                let mut handled = false;
                for action in &actions {
                    if self.dispatch_action_on_path(&path, action.as_ref(), env) {
                        handled = true;
                        break;
                    }
                }
                if handled {
                    result.handled = true;
                    return result;
                }
                // Let the app try the most specific action globally.
                if let Some(first) = actions.into_iter().next() {
                    result.unhandled_actions.push(first);
                    result.handled = true;
                    return result;
                }
            }
            KeymapMatch::None => {}
        }
        let (stopped, prevented) =
            self.dispatch_phases(&path, e, Some(raw), |h| h.key_down.clone(), env);
        result.handled = stopped || prevented;
        if stopped || prevented {
            return result;
        }
        let activates = matches!(
            e.keystroke.key,
            Key::Named(NamedKey::Enter) | Key::Named(NamedKey::Space)
        ) && !e.keystroke.modifiers.any()
            && !e.is_repeat;
        if activates
            && let Some(focused) = self.focused
            && self.nodes.get(focused).is_some_and(|n| {
                !n.element.base().handlers.click.is_empty() && !n.element.accepts_text_input()
            })
        {
            // Keyboard activation of focused clickable elements.
            let path = self.path_to(focused);
            self.bubble_simple(&path, |h| h.click.clone(), env);
            result.handled = true;
            return result;
        }
        if e.keystroke.key == Key::Named(NamedKey::Tab) && !e.keystroke.modifiers.is_command_like()
        {
            self.move_focus(!e.keystroke.modifiers.shift, env.text, env.clipboard);
            result.handled = true;
            return result;
        }
        if let (Some(text), Some(focused)) = (&e.keystroke.key_char, self.focused)
            && e.keystroke.modifiers.types_text()
            && self
                .nodes
                .get(focused)
                .is_some_and(|n| n.element.accepts_text_input())
        {
            self.deliver_builtin(focused, &InputEvent::TextInput(text.clone()), env);
            result.handled = true;
        }
        result
    }

    fn scroll_node(&mut self, id: NodeId, delta: Point, smooth: bool) -> bool {
        let now = self.now;
        let Some(node) = self.nodes.get_mut(id) else {
            return false;
        };
        if node.disabled {
            return false;
        }
        let Some(scroll) = &mut node.scroll else {
            return false;
        };
        let current = *scroll.offset.target();
        let target = Point::new(
            (current.x - delta.x).clamp(0.0, scroll.max.x),
            (current.y - delta.y).clamp(0.0, scroll.max.y),
        );
        if target == current {
            return false;
        }
        let transition = if smooth {
            Transition::new(Duration::from_millis(140)).easing(Easing::EaseOutCubic)
        } else {
            Transition::instant()
        };
        scroll.offset.set(target, transition, now);
        scroll.last_activity = Some(now);
        self.needs_paint = true;
        true
    }

    /// Scrolls the scroll container with the given id to an absolute offset.
    pub fn scroll_to(&mut self, id: &ElementId, offset: Point) {
        if let Some(node) = self.ids.get(id).and_then(|n| self.nodes.get_mut(*n))
            && let Some(scroll) = &mut node.scroll
        {
            scroll.offset.set(
                offset,
                Transition::new(Duration::from_millis(250)),
                self.now,
            );
            self.needs_paint = true;
        }
    }

    /// Cancels a pointer gesture when the native window loses focus. Keeps
    /// logical keyboard focus, but never turns an interrupted press into a click.
    pub fn cancel_pointer_input(&mut self, text: &mut TextSystem) {
        self.drag = None;
        if let Some(pressed) = self.pressed.take() {
            for id in pressed.path {
                if let Some(node) = self.nodes.get_mut(id) {
                    node.active = false;
                    self.enqueue_style(id);
                }
            }
        }
        self.mouse = None;
        self.update_hover(text);
        self.needs_paint = true;
    }
}

impl Drop for ElementTree {
    fn drop(&mut self) {
        self.unmount(self.root);
        self.unmount(self.overlay);
        self.root_owner.dispose();
    }
}

struct ParentPaint {
    origin: Point,
    transform: Transform2D,
    clip: ContentMask,
    opacity: f32,
    text_color: Color,
}

fn paint_scrollbars(
    cx: &mut PaintCx,
    scroll: &ScrollState,
    bounds: Bounds,
    content: Size,
    offset: Point,
    hovered: bool,
) {
    let idle = scroll.last_activity.map_or(Duration::from_secs(10), |t| {
        cx.now.saturating_duration_since(t)
    });
    let fade = if hovered || idle < Duration::from_millis(900) {
        1.0
    } else if idle < Duration::from_millis(1300) {
        cx.request_animation_frame();
        1.0 - (idle.as_secs_f32() - 0.9) / 0.4
    } else {
        0.0
    };
    if fade <= 0.0 {
        return;
    }
    if idle < Duration::from_millis(1300) && !hovered {
        cx.request_animation_frame();
    }
    let color = Color::from_rgba8(160, 160, 170, 255).with_alpha(0.55 * fade);
    let thickness = 6.0;
    let margin = 3.0;
    if scroll.max.y > 0.5 && content.height > 0.0 {
        let track = (bounds.height() - margin * 2.0).max(0.0);
        let thumb = (bounds.height() / content.height * track).clamp(24.0_f32.min(track), track);
        let progress = offset.y / scroll.max.y;
        let y = bounds.top() + margin + (track - thumb) * progress;
        let b = Bounds::new(
            Point::new(bounds.right() - thickness - margin, y),
            Size::new(thickness, thumb),
        );
        cx.paint_quad(
            b,
            Corners::all(thickness / 2.0),
            color,
            kova_native_core::Edges::ZERO,
            Color::TRANSPARENT,
        );
    }
    if scroll.max.x > 0.5 && content.width > 0.0 {
        let track = (bounds.width() - margin * 2.0).max(0.0);
        let thumb = (bounds.width() / content.width * track).clamp(24.0_f32.min(track), track);
        let progress = offset.x / scroll.max.x;
        let x = bounds.left() + margin + (track - thumb) * progress;
        let b = Bounds::new(
            Point::new(x, bounds.bottom() - thickness - margin),
            Size::new(thumb, thickness),
        );
        cx.paint_quad(
            b,
            Corners::all(thickness / 2.0),
            color,
            kova_native_core::Edges::ZERO,
            Color::TRANSPARENT,
        );
    }
}

#[allow(dead_code)]
fn _assert_shadow_type(_: BoxShadow) {}

fn has_focus_styles(node: &Node) -> bool {
    let base = node.element.base();
    !base.focus_styles.is_empty() || !base.focus_visible_styles.is_empty()
}

fn merge_wake(slot: &mut Option<Instant>, at: Option<Instant>) {
    if let Some(at) = at {
        *slot = Some(slot.map_or(at, |w| w.min(at)));
    }
}
