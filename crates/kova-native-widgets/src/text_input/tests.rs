//! Text fields driven through the real element tree.

use crate::elements::{column, div};
use crate::widgets::text_input;
use crate::{
    AnyElement, DispatchContext, ElementTree, FrameContext, FrameOutput, Interactive, IntoElement,
    MemoryClipboard, Styled,
};
use kova_native_core::{Color, Instant, Owner, Point, Size, signal};
use kova_native_input::{
    ImeEvent, InputEvent, Key, KeyDownEvent, Keymap, Keystroke, Modifiers, MouseButton,
    MouseDownEvent, MouseUpEvent, NamedKey,
};
use kova_native_render::{Atlas, Scene};
use kova_native_text::{TextStyle, TextSystem};
use std::cell::RefCell;
use std::rc::Rc;

struct Harness {
    tree: ElementTree,
    ts: TextSystem,
    clipboard: MemoryClipboard,
    keymap: Keymap,
}

impl Harness {
    fn new(build: impl FnMut() -> AnyElement + 'static) -> Harness {
        let mut h = Harness {
            tree: ElementTree::new(build, TextStyle::default(), Color::BLACK),
            ts: TextSystem::new(),
            clipboard: MemoryClipboard::default(),
            keymap: Keymap::new(),
        };
        h.tree.set_viewport(Size::new(400.0, 200.0), 1.0);
        h.frame();
        h
    }

    fn frame(&mut self) -> FrameOutput {
        self.tree.frame(&mut FrameContext {
            text: &mut self.ts,
            scene: &mut Scene::new(),
            atlas: &mut Atlas::new(),
            now: Instant::now(),
        })
    }

    fn send(&mut self, event: InputEvent) {
        self.tree.dispatch(
            &event,
            &mut DispatchContext {
                text: &mut self.ts,
                clipboard: &mut self.clipboard,
                keymap: &mut self.keymap,
                now: Instant::now(),
            },
        );
    }

    fn click(&mut self, id: &'static str, fraction: f32, clicks: u32) {
        let node = self.tree.node_by_id(&id.into()).unwrap();
        let b = self.tree.bounds(node).unwrap();
        let position = Point::new(b.origin.x + b.width() * fraction, b.center().y);
        self.send(InputEvent::MouseDown(MouseDownEvent {
            button: MouseButton::Left,
            position,
            modifiers: Modifiers::NONE,
            click_count: clicks,
        }));
        self.send(InputEvent::MouseUp(MouseUpEvent {
            button: MouseButton::Left,
            position,
            modifiers: Modifiers::NONE,
            click_count: clicks,
        }));
    }

    fn key(&mut self, stroke: &str) {
        let keystroke = Keystroke::parse(stroke).unwrap();
        self.send(InputEvent::KeyDown(KeyDownEvent {
            keystroke,
            is_repeat: false,
        }));
    }

    fn press(&mut self, modifiers: Modifiers, key: Key, text: &str) {
        self.send(InputEvent::KeyDown(KeyDownEvent {
            keystroke: Keystroke {
                modifiers,
                key,
                key_char: Some(text.to_string()),
            },
            is_repeat: false,
        }));
    }

    fn type_text(&mut self, text: &str) {
        for c in text.chars() {
            let s = c.to_string();
            let key = if c == ' ' {
                Key::Named(NamedKey::Space)
            } else {
                Key::character(&s)
            };
            let modifiers = Modifiers {
                shift: c.is_uppercase(),
                ..Default::default()
            };
            self.press(modifiers, key, &s);
        }
    }
}

#[test]
fn typing_editing_and_clipboard_update_the_signal() {
    let owner = Owner::new_root();
    let value = owner.with(|| signal(String::new()));
    let mut h = Harness::new(move || {
        column()
            .child(text_input(value).id("field").w(300.0))
            .child(div().id("next").size(10.0).focusable())
            .into_any()
    });
    h.click("field", 0.5, 1);
    let field = h.tree.node_by_id(&"field".into()).unwrap();
    assert_eq!(h.tree.focused(), Some(field));
    h.type_text("Hello world");
    assert_eq!(value.get(), "Hello world");
    h.key("ctrl-backspace");
    assert_eq!(value.get(), "Hello ");
    h.key("left");
    h.key("shift-home");
    h.key("ctrl-c");
    assert_eq!(h.clipboard.0.as_deref(), Some("Hello"));
    h.key("end");
    h.key("ctrl-v");
    assert_eq!(value.get(), "Hello Hello");
    h.key("ctrl-z");
    assert_eq!(value.get(), "Hello ");
    h.key("ctrl-a");
    h.key("ctrl-x");
    assert_eq!(value.get(), "");
    assert_eq!(h.clipboard.0.as_deref(), Some("Hello "));
    // Space is typed rather than activating the field, and Tab still moves
    // focus out of it.
    h.type_text("a b");
    assert_eq!(value.get(), "a b");
    h.key("tab");
    assert_eq!(h.tree.focused(), h.tree.node_by_id(&"next".into()));
    drop(h);
    owner.dispose();
}

#[test]
fn altgr_and_ime_insert_text() {
    let owner = Owner::new_root();
    let value = owner.with(|| signal(String::from("x")));
    let mut h = Harness::new(move || text_input(value).id("field").into_any());
    h.click("field", 0.9, 1);
    let altgr = Modifiers {
        control: true,
        alt: true,
        ..Default::default()
    };
    h.press(altgr, Key::character("a"), "\u{105}");
    if !cfg!(target_os = "macos") {
        assert_eq!(
            value.get(),
            "x\u{105}",
            "AltGr types instead of selecting all"
        );
    }
    let before = value.get();
    h.send(InputEvent::Ime(ImeEvent::Preedit {
        text: "\u{306b}\u{307b}".into(),
        cursor: Some((6, 6)),
    }));
    assert_eq!(value.get(), before, "composition is not committed text");
    h.frame();
    h.send(InputEvent::Ime(ImeEvent::Commit("\u{65e5}\u{672c}".into())));
    assert_eq!(value.get(), format!("{before}\u{65e5}\u{672c}"));
    drop(h);
    owner.dispose();
}

#[test]
fn external_updates_submit_and_word_selection() {
    let owner = Owner::new_root();
    let value = owner.with(|| signal(String::new()));
    let submitted = Rc::new(RefCell::new(String::new()));
    let sink = submitted.clone();
    let mut h = Harness::new(move || {
        let sink = sink.clone();
        text_input(value)
            .id("field")
            .w(300.0)
            .on_submit(move |text, _| *sink.borrow_mut() = text.to_string())
            .into_any()
    });
    value.set("alpha beta".into());
    h.frame();
    h.click("field", 0.02, 1);
    h.click("field", 0.02, 2);
    h.key("ctrl-c");
    assert_eq!(h.clipboard.0.as_deref(), Some("alpha"));
    h.key("enter");
    assert_eq!(*submitted.borrow(), "alpha beta");
    // A focused field schedules a timed repaint for the caret blink
    // instead of continuous animation frames.
    let output = h.frame();
    assert!(output.next_frame.is_some());
    assert!(output.ime_area.is_some());
    drop(h);
    owner.dispose();
}

#[test]
fn password_field_masks_and_refuses_copy() {
    let owner = Owner::new_root();
    let value = owner.with(|| signal(String::new()));
    let mut h = Harness::new(move || text_input(value).password().id("field").into_any());
    h.click("field", 0.5, 1);
    h.type_text("secret");
    h.frame();
    h.key("ctrl-a");
    h.key("ctrl-c");
    assert_eq!(h.clipboard.0, None);
    assert_eq!(value.get(), "secret");
    drop(h);
    owner.dispose();
}
