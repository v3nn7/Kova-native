//! Layout for Kova Native.
//!
//! [`LayoutStyle`] is Kova Native's own vocabulary for flexbox, grid, block,
//! absolute positioning, sizing constraints, spacing and overflow. The
//! [`LayoutEngine`] solves it incrementally with taffy: nodes cache their
//! results and only dirty subtrees are recomputed.

mod engine;
mod style;

pub use engine::{AvailableSpace, LayoutEngine, LayoutId, MeasureInput, NodeLayout};
pub use style::{
    Align, Axes, Display, FlexDirection, FlexWrap, GridPlacement, GridStyle, Justify, LayoutStyle,
    Length, Overflow, Position, Track, auto, pct, px, relative,
};

#[cfg(test)]
mod tests;
