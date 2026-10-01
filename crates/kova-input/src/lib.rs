//! Input model for Kova.
//!
//! Platform backends translate native events into [`InputEvent`]s; the UI
//! layer dispatches them through the element tree in a capture phase
//! (root → target) followed by a bubble phase (target → root).

mod click;
mod event;
mod keymap;
mod keys;

pub use click::ClickTracker;
pub use event::{
    CursorStyle, DispatchPhase, ImeEvent, InputEvent, KeyDownEvent, KeyUpEvent, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, ScrollDelta, ScrollWheelEvent,
};
pub use keymap::{Action, KeyBinding, Keymap, KeymapMatch};
pub use keys::{Key, Keystroke, KeystrokeParseError, Modifiers, NamedKey};
