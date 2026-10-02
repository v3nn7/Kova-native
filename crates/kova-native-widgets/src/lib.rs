//! The element layer of Kova Native: elements, styling, reactive bindings, the
//! retained element tree and its frame pipeline, event dispatch, focus and
//! the built-in widgets.

mod context;
mod editor;
mod element;
pub mod elements;
pub mod headless;
pub mod icons;
pub mod responsive;
mod style;
mod text_input;
mod theme;
mod tree;
pub mod widgets;

pub use context::{Clipboard, EventCx, MeasureCx, MemoryClipboard, PaintCx, WindowCommand};
pub use editor::TextEditor;
pub use element::{AnyElement, DragEvent, Element, ElementBase, Interactive, IntoElement};
pub use responsive::{Breakpoint, breakpoint, responsive, viewport_size};
pub use style::{BoxShadow, Mask, Style, Styled, TextRefinement};
pub use theme::{Theme, set_theme, theme};
pub use tree::{
    DispatchContext, DispatchResult, ElementTree, FrameContext, FrameOutput, FrameStats, NodeId,
};

#[cfg(test)]
mod tests;

/// Common element-building vocabulary of this crate (the `kova-native`
/// facade's prelude re-exports it together with core types).
pub mod prelude {
    pub use crate::elements::*;
    pub use crate::responsive::{Breakpoint, breakpoint, responsive, viewport_size};
    pub use crate::widgets::*;
    pub use crate::{
        BoxShadow, Interactive, IntoElement, Mask, Style, Styled, Theme, set_theme, theme,
    };
    pub use kova_native_render::{ShaderId, register_shader};
}

#[cfg(test)]
mod overlay_tests;

#[cfg(test)]
mod widget_tests;
