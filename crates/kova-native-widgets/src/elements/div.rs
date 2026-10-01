//! Containers.

use crate::element::{AnyElement, Element, ElementBase};
use crate::style::Styled;
use kova_native_layout::GridPlacement;

/// The general purpose container: a box with style, children and handlers.
#[derive(Default)]
pub struct Div {
    base: ElementBase,
    stack: bool,
}

impl Element for Div {
    fn base(&self) -> &ElementBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut ElementBase {
        &mut self.base
    }

    fn name(&self) -> &'static str {
        if self.stack { "stack" } else { "div" }
    }

    fn take_children(&mut self) -> Vec<AnyElement> {
        let mut children = std::mem::take(&mut self.base.children);
        if self.stack {
            // All children share the single grid cell and overlap.
            for child in &mut children {
                let grid = child.0.base_mut().style.layout.grid_mut();
                grid.column = GridPlacement::Line { start: 1, span: 1 };
                grid.row = GridPlacement::Line { start: 1, span: 1 };
            }
        }
        children
    }
}

crate::impl_element_builder!(Div);

/// An unstyled container (flex row by default, like CSS `display: flex`).
pub fn div() -> Div {
    Div::default()
}

/// Horizontal flex container with vertically centered children.
pub fn row() -> Div {
    div().flex_row().items_center()
}

/// Vertical flex container.
pub fn column() -> Div {
    div().flex_col()
}

/// Children are layered on top of each other (later children on top).
/// Align individual children with `self_*`/`justify_self` styles.
pub fn stack() -> Div {
    let mut d = div().grid();
    d.stack = true;
    d.with_style(|s| {
        let grid = s.layout.grid_mut();
        grid.template_columns = vec![kova_native_layout::Track::Fr(1.0)];
        grid.template_rows = vec![kova_native_layout::Track::Fr(1.0)];
    })
}

/// Flexible empty space that grows to fill the main axis.
pub fn spacer() -> Div {
    div().flex_grow()
}

/// An element that renders nothing.
pub fn empty() -> Div {
    div()
}
