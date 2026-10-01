use super::*;
use crate::elements::{column, div, dynamic, text};
use kova_native_core::{Color, Instant, Owner, Size, signal};
use kova_native_render::{Atlas, Scene};
use kova_native_text::{TextStyle, TextSystem};
use std::cell::Cell;
use std::rc::Rc;

fn frame(tree: &mut ElementTree, ts: &mut TextSystem) -> FrameOutput {
    tree.frame(&mut FrameContext {
        text: ts,
        scene: &mut Scene::new(),
        atlas: &mut Atlas::new(),
        now: Instant::now(),
    })
}

#[test]
fn region_keeps_builder_dependencies_after_style_resolution() {
    let owner = Owner::new_root();
    let value = owner.with(|| signal(0));
    let runs = Rc::new(Cell::new(0));
    let count = runs.clone();
    let mut tree = ElementTree::new(
        move || {
            let count = count.clone();
            dynamic(move || {
                count.set(count.get() + 1);
                text(format!("{}", value.get()))
            })
            .into_any()
        },
        TextStyle::default(),
        Color::BLACK,
    );
    let mut ts = TextSystem::new();
    frame(&mut tree, &mut ts);
    assert_eq!(runs.get(), 1);
    for i in 1..=3 {
        value.set(i);
        assert!(tree.needs_frame(), "region must remain subscribed");
        frame(&mut tree, &mut ts);
        assert_eq!(runs.get(), i + 1);
    }
    drop(tree);
    owner.dispose();
}

#[test]
fn paint_only_signal_does_not_relayout_or_rebuild() {
    let owner = Owner::new_root();
    let value = owner.with(|| signal(false));
    let mut tree = ElementTree::new(
        move || {
            column()
                .child(div().size(20.0).bind(move |s| {
                    s.bg(if value.get() {
                        Color::WHITE
                    } else {
                        Color::BLACK
                    })
                }))
                .into_any()
        },
        TextStyle::default(),
        Color::BLACK,
    );
    let mut ts = TextSystem::new();
    frame(&mut tree, &mut ts);
    assert!(!tree.needs_frame());
    value.set(true);
    let output = frame(&mut tree, &mut ts);
    assert_eq!(output.stats.styles_resolved, 1);
    assert_eq!(output.stats.regions_rebuilt, 0);
    assert!(!output.stats.layout_ran);
    assert!(!tree.needs_frame());
    drop(tree);
    owner.dispose();
}

#[test]
fn dropping_tree_disposes_ui_owned_signals() {
    let saved = Rc::new(Cell::new(None));
    let target = saved.clone();
    let mut tree = ElementTree::new(
        move || {
            let value = signal(42);
            target.set(Some(value));
            div().into_any()
        },
        TextStyle::default(),
        Color::BLACK,
    );
    tree.set_viewport(Size::new(320.0, 240.0), 1.25);
    let value = saved.get().unwrap();
    assert!(value.is_alive());
    drop(tree);
    assert!(
        !value.is_alive(),
        "closing a window must dispose its reactive scope"
    );
}

#[test]
fn region_style_binding_does_not_rebuild_its_children() {
    let owner = Owner::new_root();
    let (content, color) = owner.with(|| (signal(0), signal(false)));
    let runs = Rc::new(Cell::new(0));
    let counter = runs.clone();
    let mut tree = ElementTree::new(
        move || {
            let counter = counter.clone();
            dynamic(move || {
                counter.set(counter.get() + 1);
                text(format!("{}", content.get()))
            })
            .bind(move |s| {
                s.bg(if color.get() {
                    Color::WHITE
                } else {
                    Color::BLACK
                })
            })
            .into_any()
        },
        TextStyle::default(),
        Color::BLACK,
    );
    let mut ts = TextSystem::new();
    frame(&mut tree, &mut ts);
    color.set(true);
    let output = frame(&mut tree, &mut ts);
    assert_eq!(runs.get(), 1);
    assert_eq!(output.stats.regions_rebuilt, 0);
    content.set(1);
    let output = frame(&mut tree, &mut ts);
    assert_eq!(runs.get(), 2);
    assert_eq!(output.stats.regions_rebuilt, 1);
    drop(tree);
    owner.dispose();
}

#[test]
fn tiny_scroll_container_can_paint_overlay_thumb() {
    use kova_native_input::{InputEvent, Keymap, Modifiers, MouseMoveEvent};
    let mut tree = ElementTree::new(
        || {
            column()
                .child(
                    column()
                        .id("tiny")
                        .w(100.0)
                        .h(8.0)
                        .overflow_y_scroll()
                        .child(div().w(100.0).h(80.0)),
                )
                .into_any()
        },
        TextStyle::default(),
        Color::BLACK,
    );
    let mut ts = TextSystem::new();
    frame(&mut tree, &mut ts);
    let id = tree.node_by_id(&"tiny".into()).unwrap();
    let position = tree.bounds(id).unwrap().center();
    tree.dispatch(
        &InputEvent::MouseMove(MouseMoveEvent {
            position,
            pressed_button: None,
            modifiers: Modifiers::NONE,
        }),
        &mut DispatchContext {
            text: &mut ts,
            clipboard: &mut MemoryClipboard::default(),
            keymap: &mut Keymap::new(),
            now: Instant::now(),
        },
    );
    frame(&mut tree, &mut ts);
}

struct InputProbe {
    base: ElementBase,
    keydowns: Rc<Cell<u32>>,
}

impl Element for InputProbe {
    fn base(&self) -> &ElementBase {
        &self.base
    }
    fn base_mut(&mut self) -> &mut ElementBase {
        &mut self.base
    }
    fn handle_event(&mut self, _cx: &mut EventCx, event: &kova_native_input::InputEvent) {
        if matches!(event, kova_native_input::InputEvent::KeyDown(_)) {
            self.keydowns.set(self.keydowns.get() + 1);
        }
    }
}
crate::impl_element_builder!(InputProbe);

#[test]
fn capture_prevent_default_suppresses_builtin_target_behavior() {
    use kova_native_input::{
        InputEvent, KeyDownEvent, Keymap, Keystroke, Modifiers, MouseButton, MouseDownEvent,
    };
    let keydowns = Rc::new(Cell::new(0));
    let counter = keydowns.clone();
    let mut tree = ElementTree::new(
        move || {
            column()
                .capture_key_down(|_, cx| cx.prevent_default())
                .child(
                    InputProbe {
                        base: ElementBase::new(),
                        keydowns: counter.clone(),
                    }
                    .id("probe")
                    .focusable()
                    .size(50.0),
                )
                .into_any()
        },
        TextStyle::default(),
        Color::BLACK,
    );
    let mut ts = TextSystem::new();
    frame(&mut tree, &mut ts);
    let id = tree.node_by_id(&"probe".into()).unwrap();
    let position = tree.bounds(id).unwrap().center();
    let mut clipboard = MemoryClipboard::default();
    let mut keymap = Keymap::new();
    let mut env = DispatchContext {
        text: &mut ts,
        clipboard: &mut clipboard,
        keymap: &mut keymap,
        now: Instant::now(),
    };
    tree.dispatch(
        &InputEvent::MouseDown(MouseDownEvent {
            button: MouseButton::Left,
            position,
            modifiers: Modifiers::NONE,
            click_count: 1,
        }),
        &mut env,
    );
    assert_eq!(tree.focused(), Some(id));
    tree.dispatch(
        &InputEvent::KeyDown(KeyDownEvent {
            keystroke: Keystroke::parse("x").unwrap(),
            is_repeat: false,
        }),
        &mut env,
    );
    assert_eq!(keydowns.get(), 0);
}

#[test]
fn transformed_slider_uses_local_coordinates_for_press_and_drag() {
    use kova_native_core::{Point, Transform2D};
    use kova_native_input::{
        InputEvent, Keymap, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent,
    };
    let owner = Owner::new_root();
    let value = owner.with(|| signal(0.0));
    let mut tree = ElementTree::new(
        move || {
            column()
                .padding(140.0)
                .child(
                    div()
                        .id("parent")
                        .w(200.0)
                        .h(160.0)
                        .scale(1.5)
                        .rotate(30.0)
                        .translate(20.0, 30.0)
                        .child(
                            crate::widgets::slider(value)
                                .id("slider")
                                .w(120.0)
                                .rotate(90.0),
                        ),
                )
                .into_any()
        },
        TextStyle::default(),
        Color::BLACK,
    );
    let mut ts = TextSystem::new();
    frame(&mut tree, &mut ts);
    let parent = tree.node_by_id(&"parent".into()).unwrap();
    let slider = tree.node_by_id(&"slider".into()).unwrap();
    let parent_bounds = tree.bounds(parent).unwrap();
    let bounds = tree.bounds(slider).unwrap();
    // Construct expected window points from the specified transforms, rather
    // than reading the event tree's cached transform or hitbox.
    let parent_transform = Transform2D::scale(1.5, 1.5)
        .then(&Transform2D::rotate(30.0_f32.to_radians()))
        .around(parent_bounds.center())
        .then(&Transform2D::translate(20.0, 30.0));
    let transform = Transform2D::rotate(90.0_f32.to_radians())
        .around(bounds.center())
        .then(&parent_transform);
    let point_at = |fraction: f32| {
        transform
            .apply(bounds.origin + Point::new(bounds.width() * fraction, bounds.height() / 2.0))
    };
    let position = point_at(0.25);
    assert_eq!(tree.hit_test(position), Some(slider));
    let mut clipboard = MemoryClipboard::default();
    let mut keymap = Keymap::new();
    let mut env = DispatchContext {
        text: &mut ts,
        clipboard: &mut clipboard,
        keymap: &mut keymap,
        now: Instant::now(),
    };
    tree.dispatch(
        &InputEvent::MouseDown(MouseDownEvent {
            button: MouseButton::Left,
            position,
            modifiers: Modifiers::NONE,
            click_count: 1,
        }),
        &mut env,
    );
    assert!(
        (value.get() - 0.25).abs() < 0.001,
        "transformed press: {}",
        value.get()
    );
    tree.dispatch(
        &InputEvent::MouseMove(MouseMoveEvent {
            position: point_at(0.75),
            pressed_button: Some(MouseButton::Left),
            modifiers: Modifiers::NONE,
        }),
        &mut env,
    );
    assert!(
        (value.get() - 0.75).abs() < 0.001,
        "transformed drag: {}",
        value.get()
    );
    drop(tree);
    owner.dispose();
}

#[test]
fn singular_transform_has_no_pointer_hitbox() {
    let mut tree = ElementTree::new(
        || {
            column()
                .child(div().id("collapsed").size(50.0).scale(0.0).focusable())
                .into_any()
        },
        TextStyle::default(),
        Color::BLACK,
    );
    frame(&mut tree, &mut TextSystem::new());
    let id = tree.node_by_id(&"collapsed".into()).unwrap();
    let position = tree.bounds(id).unwrap().center();
    assert_ne!(
        tree.hit_test(position),
        Some(id),
        "a collapsed element cannot intercept input"
    );
}
