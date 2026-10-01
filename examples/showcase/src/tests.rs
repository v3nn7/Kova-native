use super::*;
use kova::input::{
    InputEvent, KeyDownEvent, MouseButton, MouseDownEvent, MouseUpEvent, ScrollDelta,
    ScrollWheelEvent,
};
use kova::render::{Atlas, Scene};
use kova::text::TextSystem;
use kova::widgets::{DispatchContext, ElementTree, FrameContext, FrameOutput, MemoryClipboard};

struct Harness {
    tree: ElementTree,
    ts: TextSystem,
    atlas: Atlas,
    scene: Scene,
    clipboard: MemoryClipboard,
    keymap: kova::input::Keymap,
    now: Instant,
}

impl Harness {
    fn new(state: Showcase) -> Self {
        let mut tree = ElementTree::new(
            move || showcase(state).into_any(),
            TextStyle::default(),
            theme().background,
        );
        tree.set_viewport(Size::new(1120.0, 820.0), 1.25);
        Self {
            tree,
            ts: TextSystem::new(),
            atlas: Atlas::new(),
            scene: Scene::new(),
            clipboard: MemoryClipboard::default(),
            keymap: kova::input::Keymap::new(),
            now: Instant::now(),
        }
    }

    fn frame(&mut self) -> FrameOutput {
        self.now += 2.0.secs();
        self.tree.frame(&mut FrameContext {
            text: &mut self.ts,
            scene: &mut self.scene,
            atlas: &mut self.atlas,
            now: self.now,
        })
    }

    fn dispatch(&mut self, event: InputEvent) {
        self.tree.dispatch(
            &event,
            &mut DispatchContext {
                text: &mut self.ts,
                clipboard: &mut self.clipboard,
                keymap: &mut self.keymap,
                now: self.now,
            },
        );
    }

    fn bounds(&self, id: impl Into<ElementId>) -> Bounds {
        self.tree
            .bounds(self.tree.node_by_id(&id.into()).expect("mounted id"))
            .unwrap()
    }

    fn click(&mut self, id: impl Into<ElementId>) {
        let p = self.bounds(id).center();
        assert!(
            self.tree.hit_test(p).is_some(),
            "visible interactive element"
        );
        self.dispatch(InputEvent::MouseDown(MouseDownEvent {
            button: MouseButton::Left,
            position: p,
            modifiers: Modifiers::NONE,
            click_count: 1,
        }));
        self.dispatch(InputEvent::MouseUp(MouseUpEvent {
            button: MouseButton::Left,
            position: p,
            modifiers: Modifiers::NONE,
            click_count: 1,
        }));
        self.frame();
    }

    fn key(&mut self, source: &str) {
        self.dispatch(InputEvent::KeyDown(KeyDownEvent {
            keystroke: Keystroke::parse(source).unwrap(),
            is_repeat: false,
        }));
        self.frame();
    }
}

#[test]
fn showcase_reactivity_focus_scroll_theme_and_widgets_integrate() {
    set_theme(Theme::dark());
    let owner = Owner::new_root();
    let state = owner.with(Showcase::new);
    let mut h = Harness::new(state);
    let initial = h.frame();
    assert!(initial.stats.layout_ran);
    assert!(h.tree.node_by_id(&"increment".into()).is_some());
    assert!(!h.scene.is_empty());
    assert!(
        h.atlas.tile_count() > 20,
        "real glyphs and SVG masks enter the atlas"
    );
    assert!(
        !h.tree.needs_frame(),
        "overview is idle without a timer loop"
    );
    let initial_nodes = h.tree.node_count();

    h.click("increment");
    assert_eq!(state.clicks.get(), 1);
    assert_eq!(h.tree.node_count(), initial_nodes);
    let focused = h.tree.focused().unwrap();
    assert_eq!(h.tree.element_name(focused), Some("button"));
    h.key("enter");
    assert_eq!(state.clicks.get(), 2);
    h.click("disabled");
    assert_eq!(state.clicks.get(), 2);
    h.click("enable");
    assert!(!state.enabled.get());
    h.key("space");
    assert!(state.enabled.get());
    h.click("check");
    assert!(!state.checked.get());
    h.click("intensity");
    assert!((state.amount.get() - 0.5).abs() < 0.02);
    h.key("right");
    assert!((state.amount.get() - 0.55).abs() < 0.02);

    let before = h.bounds("actions-card").top();
    let p = h.bounds("content").center();
    h.dispatch(InputEvent::ScrollWheel(ScrollWheelEvent {
        position: p,
        delta: ScrollDelta::Pixels(point(0.0, -90.0)),
        modifiers: Modifiers::NONE,
    }));
    h.frame();
    assert!(
        h.bounds("actions-card").top() < before,
        "wheel changes retained content positions"
    );

    h.click("theme");
    assert!(!theme().dark);
    assert_eq!(
        state.clicks.get(),
        2,
        "external app state survives root theme rebuild"
    );
    let theme_button = h.tree.node_by_id(&"theme".into()).unwrap();
    assert_eq!(
        h.tree.focused(),
        Some(theme_button),
        "stable id restores focus in the same frame"
    );
    h.click(("nav", 1usize));
    assert_eq!(state.section.get(), 1);
    assert!(h.tree.node_by_id(&"spring-box".into()).is_some());
    h.click("retarget");
    assert!(state.moved.get());
    assert!(h.frame().animating);
    let tilted = h.bounds("tilted-slider");
    let position = Transform2D::scale(1.1, 1.1)
        .then(&Transform2D::rotate((-12.0_f32).to_radians()))
        .around(tilted.center())
        .apply(tilted.origin + point(tilted.width() * 0.25, tilted.height() / 2.0));
    h.dispatch(InputEvent::MouseDown(MouseDownEvent {
        button: MouseButton::Left,
        position,
        modifiers: Modifiers::NONE,
        click_count: 1,
    }));
    h.dispatch(InputEvent::MouseUp(MouseUpEvent {
        button: MouseButton::Left,
        position,
        modifiers: Modifiers::NONE,
        click_count: 1,
    }));
    assert!((state.amount.get() - 0.25).abs() < 0.001);
    h.frame();
    h.click(("nav", 2usize));
    assert!(h.tree.node_by_id(&"spring-box".into()).is_none());
    assert!(
        !h.frame().animating,
        "removing the animated region stops its animations"
    );
    drop(h);
    owner.dispose();
    assert!(!state.clicks.is_alive());
    set_theme(Theme::dark());
}
