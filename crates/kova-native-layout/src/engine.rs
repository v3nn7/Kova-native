//! Incremental layout engine backed by taffy.

use crate::style::*;
use kova_native_core::{Point, Size};
use taffy::prelude::{TaffyAuto, TaffyMaxContent, TaffyMinContent};
use taffy::style_helpers;

/// Handle to a node in the [`LayoutEngine`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LayoutId(taffy::NodeId);

/// Space available to a node along one axis while measuring.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AvailableSpace {
    Definite(f32),
    MinContent,
    MaxContent,
}

impl AvailableSpace {
    /// The definite size, or `None` for intrinsic sizing queries.
    pub fn definite(self) -> Option<f32> {
        match self {
            AvailableSpace::Definite(v) => Some(v),
            _ => None,
        }
    }
}

/// Inputs passed to leaf measure functions (e.g. text).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeasureInput {
    /// Sizes already fixed by the parent/style, if any.
    pub known_width: Option<f32>,
    pub known_height: Option<f32>,
    pub available_width: AvailableSpace,
    pub available_height: AvailableSpace,
}

/// Computed layout of a node, relative to its parent's border box.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct NodeLayout {
    pub location: Point,
    pub size: Size,
    /// Extent of the scrollable content, measured from the padding box origin.
    pub content_size: Size,
    pub padding: kova_native_core::Edges<f32>,
    pub border: kova_native_core::Edges<f32>,
}

/// An incremental layout tree.
///
/// Nodes keep their computed layout between frames; changing a style or
/// marking a leaf dirty only recomputes the affected ancestors. Leaves with a
/// measure context `K` are sized through the measure callback passed to
/// [`LayoutEngine::compute`].
pub struct LayoutEngine<K: Copy + 'static> {
    tree: taffy::TaffyTree<K>,
}

impl<K: Copy + 'static> Default for LayoutEngine<K> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Copy + 'static> LayoutEngine<K> {
    pub fn new() -> Self {
        let mut tree = taffy::TaffyTree::new();
        // Kova Native snaps to device pixels at paint time; logical-pixel rounding
        // would misalign content at fractional scale factors.
        tree.disable_rounding();
        LayoutEngine { tree }
    }

    /// Creates a node. Leaves that need measuring get a `context`.
    pub fn create(&mut self, style: &LayoutStyle, context: Option<K>) -> LayoutId {
        let style = to_taffy(style);
        let id = match context {
            Some(ctx) => self.tree.new_leaf_with_context(style, ctx),
            None => self.tree.new_leaf(style),
        }
        .expect("taffy node creation cannot fail");
        LayoutId(id)
    }

    /// Sets or clears the measure context of a node (making it a measured leaf).
    pub fn set_context(&mut self, id: LayoutId, context: Option<K>) {
        let _ = self.tree.set_node_context(id.0, context);
    }

    pub fn set_style(&mut self, id: LayoutId, style: &LayoutStyle) {
        let _ = self.tree.set_style(id.0, to_taffy(style));
    }

    pub fn set_children(&mut self, id: LayoutId, children: &[LayoutId]) {
        let children: Vec<taffy::NodeId> = children.iter().map(|c| c.0).collect();
        let _ = self.tree.set_children(id.0, &children);
    }

    /// Marks a leaf as needing re-measurement (e.g. its text changed).
    pub fn mark_dirty(&mut self, id: LayoutId) {
        let _ = self.tree.mark_dirty(id.0);
    }

    pub fn is_dirty(&self, id: LayoutId) -> bool {
        self.tree.dirty(id.0).unwrap_or(false)
    }

    pub fn remove(&mut self, id: LayoutId) {
        let _ = self.tree.remove(id.0);
    }

    pub fn node_count(&self) -> usize {
        self.tree.total_node_count()
    }

    /// Computes layout for the subtree rooted at `root` within `available`.
    pub fn compute(
        &mut self,
        root: LayoutId,
        available: Size,
        mut measure: impl FnMut(K, MeasureInput) -> Size,
    ) {
        let available = taffy::Size {
            width: taffy::AvailableSpace::Definite(available.width),
            height: taffy::AvailableSpace::Definite(available.height),
        };
        let _ = self.tree.compute_layout_with_measure(
            root.0,
            available,
            |inputs, _node, ctx, style| {
                taffy::compute_leaf_layout(
                    inputs,
                    style,
                    |_, _| 0.0,
                    |known, avail| {
                        let Some(ctx) = ctx else {
                            return taffy::Size::ZERO;
                        };
                        let convert = |a: taffy::AvailableSpace| match a {
                            taffy::AvailableSpace::Definite(v) => AvailableSpace::Definite(v),
                            taffy::AvailableSpace::MinContent => AvailableSpace::MinContent,
                            taffy::AvailableSpace::MaxContent => AvailableSpace::MaxContent,
                        };
                        let size = measure(
                            *ctx,
                            MeasureInput {
                                known_width: known.width,
                                known_height: known.height,
                                available_width: convert(avail.width),
                                available_height: convert(avail.height),
                            },
                        );
                        taffy::Size {
                            width: size.width,
                            height: size.height,
                        }
                    },
                )
            },
        );
    }

    /// The computed layout of a node.
    pub fn layout(&self, id: LayoutId) -> NodeLayout {
        let Ok(l) = self.tree.layout(id.0) else {
            return NodeLayout::default();
        };
        let edges = |r: taffy::Rect<f32>| kova_native_core::Edges {
            top: r.top,
            right: r.right,
            bottom: r.bottom,
            left: r.left,
        };
        NodeLayout {
            location: Point::new(l.location.x, l.location.y),
            size: Size::new(l.size.width, l.size.height),
            content_size: Size::new(
                l.scrollable_overflow_rect.right.max(0.0),
                l.scrollable_overflow_rect.bottom.max(0.0),
            ),
            padding: edges(l.padding),
            border: edges(l.border),
        }
    }
}

fn dim(l: Length) -> taffy::Dimension {
    match l {
        Length::Auto => taffy::Dimension::auto(),
        Length::Px(v) => taffy::Dimension::length(v),
        Length::Relative(f) => taffy::Dimension::percent(f),
    }
}

fn lpa(l: Length) -> taffy::LengthPercentageAuto {
    match l {
        Length::Auto => taffy::LengthPercentageAuto::auto(),
        Length::Px(v) => taffy::LengthPercentageAuto::length(v),
        Length::Relative(f) => taffy::LengthPercentageAuto::percent(f),
    }
}

fn lp(l: Length) -> taffy::LengthPercentage {
    match l {
        Length::Auto => taffy::LengthPercentage::length(0.0),
        Length::Px(v) => taffy::LengthPercentage::length(v),
        Length::Relative(f) => taffy::LengthPercentage::percent(f),
    }
}

fn align(a: Align) -> taffy::AlignItems {
    match a {
        Align::Start => taffy::AlignItems::FLEX_START,
        Align::End => taffy::AlignItems::FLEX_END,
        Align::Center => taffy::AlignItems::CENTER,
        Align::Stretch => taffy::AlignItems::STRETCH,
        Align::Baseline => taffy::AlignItems::BASELINE,
    }
}

fn justify(j: Justify) -> taffy::JustifyContent {
    match j {
        Justify::Start => taffy::JustifyContent::FLEX_START,
        Justify::End => taffy::JustifyContent::FLEX_END,
        Justify::Center => taffy::JustifyContent::CENTER,
        Justify::Stretch => taffy::JustifyContent::STRETCH,
        Justify::SpaceBetween => taffy::JustifyContent::SPACE_BETWEEN,
        Justify::SpaceAround => taffy::JustifyContent::SPACE_AROUND,
        Justify::SpaceEvenly => taffy::JustifyContent::SPACE_EVENLY,
    }
}

fn overflow(o: Overflow) -> taffy::Overflow {
    match o {
        Overflow::Visible => taffy::Overflow::Visible,
        Overflow::Hidden => taffy::Overflow::Clip,
        Overflow::Scroll => taffy::Overflow::Scroll,
    }
}

fn track(t: Track) -> taffy::TrackSizingFunction {
    match t {
        Track::Px(v) => style_helpers::length(v),
        Track::Relative(f) => style_helpers::percent(f),
        Track::Fr(f) => style_helpers::fr(f),
        Track::Auto => taffy::TrackSizingFunction::AUTO,
        Track::MinContent => taffy::TrackSizingFunction::MIN_CONTENT,
        Track::MaxContent => taffy::TrackSizingFunction::MAX_CONTENT,
        Track::MinMax(min, max_fr) => style_helpers::minmax(
            taffy::MinTrackSizingFunction::length(min),
            taffy::MaxTrackSizingFunction::fr(max_fr),
        ),
    }
}

fn placement(p: GridPlacement) -> taffy::Line<taffy::GridPlacement> {
    match p {
        GridPlacement::Auto => taffy::Line {
            start: taffy::GridPlacement::Auto,
            end: taffy::GridPlacement::Auto,
        },
        GridPlacement::Span(n) => taffy::Line {
            start: style_helpers::span(n),
            end: taffy::GridPlacement::Auto,
        },
        GridPlacement::Line { start, span } => taffy::Line {
            start: style_helpers::line(start),
            end: style_helpers::span(span.max(1)),
        },
    }
}

/// Converts a Kova Native layout style to a taffy style.
pub(crate) fn to_taffy(s: &LayoutStyle) -> taffy::Style {
    let mut t = taffy::Style {
        display: match s.display {
            Display::Flex => taffy::Display::Flex,
            Display::Grid => taffy::Display::Grid,
            Display::Block => taffy::Display::Block,
            Display::None => taffy::Display::None,
        },
        position: match s.position {
            Position::Relative => taffy::Position::Relative,
            Position::Absolute => taffy::Position::Absolute,
        },
        flex_direction: match s.direction {
            FlexDirection::Row => taffy::FlexDirection::Row,
            FlexDirection::Column => taffy::FlexDirection::Column,
            FlexDirection::RowReverse => taffy::FlexDirection::RowReverse,
            FlexDirection::ColumnReverse => taffy::FlexDirection::ColumnReverse,
        },
        flex_wrap: match s.wrap {
            FlexWrap::NoWrap => taffy::FlexWrap::NoWrap,
            FlexWrap::Wrap => taffy::FlexWrap::Wrap,
            FlexWrap::WrapReverse => taffy::FlexWrap::WrapReverse,
        },
        align_items: s.align_items.map(align),
        align_self: s.align_self.map(align),
        align_content: s.align_content.map(justify),
        justify_content: s.justify_content.map(justify),
        size: taffy::Size {
            width: dim(s.size.width),
            height: dim(s.size.height),
        },
        min_size: taffy::Size {
            width: lpa(s.min_size.width),
            height: lpa(s.min_size.height),
        },
        max_size: taffy::Size {
            width: lpa(s.max_size.width),
            height: lpa(s.max_size.height),
        },
        aspect_ratio: s.aspect_ratio,
        padding: taffy::Rect {
            top: lp(s.padding.top),
            right: lp(s.padding.right),
            bottom: lp(s.padding.bottom),
            left: lp(s.padding.left),
        },
        margin: taffy::Rect {
            top: lpa(s.margin.top),
            right: lpa(s.margin.right),
            bottom: lpa(s.margin.bottom),
            left: lpa(s.margin.left),
        },
        border: taffy::Rect {
            top: taffy::LengthPercentage::length(s.border.top),
            right: taffy::LengthPercentage::length(s.border.right),
            bottom: taffy::LengthPercentage::length(s.border.bottom),
            left: taffy::LengthPercentage::length(s.border.left),
        },
        inset: taffy::Rect {
            top: lpa(s.inset.top),
            right: lpa(s.inset.right),
            bottom: lpa(s.inset.bottom),
            left: lpa(s.inset.left),
        },
        gap: taffy::Size {
            width: lp(s.gap.width),
            height: lp(s.gap.height),
        },
        flex_grow: s.flex_grow,
        flex_shrink: s.flex_shrink,
        flex_basis: dim(s.flex_basis),
        overflow: taffy::Point {
            x: overflow(s.overflow_x),
            y: overflow(s.overflow_y),
        },
        // Kova Native draws overlay scrollbars that do not take layout space.
        scrollbar_width: 0.0,
        ..Default::default()
    };
    if let Some(grid) = &s.grid {
        t.grid_template_columns = grid
            .template_columns
            .iter()
            .map(|tr| taffy::GridTemplateComponent::Single(track(*tr)))
            .collect();
        t.grid_template_rows = grid
            .template_rows
            .iter()
            .map(|tr| taffy::GridTemplateComponent::Single(track(*tr)))
            .collect();
        t.grid_auto_rows = grid.auto_rows.iter().map(|tr| track(*tr)).collect();
        t.grid_column = placement(grid.column);
        t.grid_row = placement(grid.row);
    }
    t
}
