//! Translation of winit events into Kova Native input events.

use kova_native_core::Point;
use kova_native_input::{CursorStyle, Key, Modifiers, MouseButton, NamedKey};
use winit::keyboard::{Key as WKey, ModifiersState, NamedKey as WNamed};

pub fn modifiers(state: ModifiersState) -> Modifiers {
    Modifiers {
        shift: state.shift_key(),
        control: state.control_key(),
        alt: state.alt_key(),
        platform: state.super_key(),
    }
}

pub fn mouse_button(button: winit::event::MouseButton) -> MouseButton {
    match button {
        winit::event::MouseButton::Left => MouseButton::Left,
        winit::event::MouseButton::Right => MouseButton::Right,
        winit::event::MouseButton::Middle => MouseButton::Middle,
        winit::event::MouseButton::Back => MouseButton::Back,
        winit::event::MouseButton::Forward => MouseButton::Forward,
        winit::event::MouseButton::Other(n) => MouseButton::Other(n),
    }
}

pub fn key(key: &WKey) -> Key {
    match key {
        WKey::Character(s) => Key::character(s),
        WKey::Named(named) => match named_key(*named) {
            Some(n) => Key::Named(n),
            None => Key::Unidentified,
        },
        _ => Key::Unidentified,
    }
}

fn named_key(key: WNamed) -> Option<NamedKey> {
    Some(match key {
        WNamed::Enter => NamedKey::Enter,
        WNamed::Tab => NamedKey::Tab,
        WNamed::Space => NamedKey::Space,
        WNamed::Backspace => NamedKey::Backspace,
        WNamed::Delete => NamedKey::Delete,
        WNamed::Escape => NamedKey::Escape,
        WNamed::ArrowLeft => NamedKey::ArrowLeft,
        WNamed::ArrowRight => NamedKey::ArrowRight,
        WNamed::ArrowUp => NamedKey::ArrowUp,
        WNamed::ArrowDown => NamedKey::ArrowDown,
        WNamed::Home => NamedKey::Home,
        WNamed::End => NamedKey::End,
        WNamed::PageUp => NamedKey::PageUp,
        WNamed::PageDown => NamedKey::PageDown,
        WNamed::Insert => NamedKey::Insert,
        WNamed::Shift => NamedKey::Shift,
        WNamed::Control => NamedKey::Control,
        WNamed::Alt => NamedKey::Alt,
        WNamed::Super | WNamed::Meta => NamedKey::Super,
        WNamed::CapsLock => NamedKey::CapsLock,
        WNamed::ContextMenu => NamedKey::ContextMenu,
        WNamed::F1 => NamedKey::F(1),
        WNamed::F2 => NamedKey::F(2),
        WNamed::F3 => NamedKey::F(3),
        WNamed::F4 => NamedKey::F(4),
        WNamed::F5 => NamedKey::F(5),
        WNamed::F6 => NamedKey::F(6),
        WNamed::F7 => NamedKey::F(7),
        WNamed::F8 => NamedKey::F(8),
        WNamed::F9 => NamedKey::F(9),
        WNamed::F10 => NamedKey::F(10),
        WNamed::F11 => NamedKey::F(11),
        WNamed::F12 => NamedKey::F(12),
        _ => return None,
    })
}

pub fn cursor(style: CursorStyle) -> winit::window::CursorIcon {
    use winit::window::CursorIcon as C;
    match style {
        CursorStyle::Arrow => C::Default,
        CursorStyle::PointingHand => C::Pointer,
        CursorStyle::IBeam => C::Text,
        CursorStyle::Crosshair => C::Crosshair,
        CursorStyle::ClosedHand => C::Grabbing,
        CursorStyle::OpenHand => C::Grab,
        CursorStyle::NotAllowed => C::NotAllowed,
        CursorStyle::ResizeLeftRight => C::EwResize,
        CursorStyle::ResizeUpDown => C::NsResize,
        CursorStyle::ResizeNwse => C::NwseResize,
        CursorStyle::ResizeNesw => C::NeswResize,
        CursorStyle::Move => C::Move,
        CursorStyle::Wait => C::Wait,
        CursorStyle::Progress => C::Progress,
    }
}

pub fn logical(position: winit::dpi::PhysicalPosition<f64>, scale: f64) -> Point {
    Point::new((position.x / scale) as f32, (position.y / scale) as f32)
}
