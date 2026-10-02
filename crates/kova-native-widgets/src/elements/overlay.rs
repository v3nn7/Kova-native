//! The overlay layer: portals and anchored placement.
//!
//! Every [`ElementTree`](crate::ElementTree) owns an overlay layer painted
//! above the application root. A [`Portal`] mounts its children into that
//! layer instead of in place, so they escape the clipping, scrolling and
//! paint order of their ancestors. Logically the children still belong to the
//! portal: they inherit its text style, are owned by its reactive scope,
//! bubble events through its ancestors and are removed with it.
//!
//! Portal children are absolutely positioned against the window. Use
//! `inset_0()` for a full-window layer (modal backdrops), or anchor them to
//! an element with [`Portal::anchored`] (menus, popovers, tooltips):
//!
//! ```ignore
//! div()
//!     .child(button("Options").on_click(move |_| open.toggle()))
//!     .child(dynamic(move || open.get().then(|| {
//!         portal().anchored().placement(Placement::BOTTOM_START).child(menu())
//!     })))
//! ```

use crate::element::{AnyElement, Element, ElementBase};
use crate::style::Styled;
use kova_native_core::{Bounds, ElementId, Point, Size};
use kova_native_layout::Position;

/// The side of the anchor an overlay is placed on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Top,
    Bottom,
    Left,
    Right,
}

/// Alignment along the anchor's edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Alignment {
    Start,
    Center,
    End,
}

/// Where an anchored overlay appears relative to its anchor. If it does not
/// fit, it flips to the opposite side and is kept inside the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placement {
    pub side: Side,
    pub align: Alignment,
}

impl Placement {
    pub const fn new(side: Side, align: Alignment) -> Self {
        Placement { side, align }
    }

    pub const TOP: Placement = Placement::new(Side::Top, Alignment::Center);
    pub const TOP_START: Placement = Placement::new(Side::Top, Alignment::Start);
    pub const TOP_END: Placement = Placement::new(Side::Top, Alignment::End);
    pub const BOTTOM: Placement = Placement::new(Side::Bottom, Alignment::Center);
    pub const BOTTOM_START: Placement = Placement::new(Side::Bottom, Alignment::Start);
    pub const BOTTOM_END: Placement = Placement::new(Side::Bottom, Alignment::End);
    pub const LEFT: Placement = Placement::new(Side::Left, Alignment::Center);
    pub const LEFT_START: Placement = Placement::new(Side::Left, Alignment::Start);
    pub const RIGHT: Placement = Placement::new(Side::Right, Alignment::Center);
    pub const RIGHT_START: Placement = Placement::new(Side::Right, Alignment::Start);
}

impl Default for Placement {
    fn default() -> Self {
        Placement::BOTTOM_START
    }
}

/// What an anchored portal is positioned against.
#[derive(Clone, Debug, PartialEq)]
pub enum Anchor {
    /// The nearest ancestor of the portal that is not a reactive region
    /// (`dynamic`, `list`, `view`), i.e. the element that visually contains it.
    Parent,
    /// The element with this id.
    Id(ElementId),
    /// A fixed window position (e.g. where a context menu was requested).
    Point(Point),
}

/// How a portal's children are positioned; see [`Element::portal`].
#[derive(Clone, Debug, PartialEq)]
pub struct PortalSpec {
    /// `None` positions children against the window.
    pub anchor: Option<Anchor>,
    pub placement: Placement,
    /// Distance between the anchor and the overlay, in logical px.
    pub gap: f32,
    /// Makes anchored children at least as wide as the anchor (select lists).
    pub match_width: bool,
}

/// Renders its children in the window's overlay layer. See the module docs.
pub struct Portal {
    base: ElementBase,
    spec: PortalSpec,
}

/// Creates a portal. Its children are positioned against the window.
pub fn portal() -> Portal {
    Portal {
        base: ElementBase::new(),
        spec: PortalSpec {
            anchor: None,
            placement: Placement::default(),
            gap: 6.0,
            match_width: false,
        },
    }
    // The portal itself takes no space where it is declared.
    .absolute()
}

impl Portal {
    /// Positions the children next to the element containing the portal.
    pub fn anchored(mut self) -> Self {
        self.spec.anchor = Some(Anchor::Parent);
        self
    }

    /// Positions the children next to the element with `id`.
    pub fn anchor_to(mut self, id: impl Into<ElementId>) -> Self {
        self.spec.anchor = Some(Anchor::Id(id.into()));
        self
    }

    /// Positions the children at a window point (logical px).
    pub fn at(mut self, point: Point) -> Self {
        self.spec.anchor = Some(Anchor::Point(point));
        self
    }

    pub fn placement(mut self, placement: Placement) -> Self {
        self.spec.placement = placement;
        self
    }

    /// Distance from the anchor (default 6 px).
    pub fn gap(mut self, gap: f32) -> Self {
        self.spec.gap = gap;
        self
    }

    /// Makes anchored children at least as wide as their anchor.
    pub fn match_anchor_width(mut self) -> Self {
        self.spec.match_width = true;
        self
    }
}

impl Element for Portal {
    fn base(&self) -> &ElementBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut ElementBase {
        &mut self.base
    }

    fn name(&self) -> &'static str {
        "portal"
    }

    fn take_children(&mut self) -> Vec<AnyElement> {
        let mut children = std::mem::take(&mut self.base.children);
        for child in &mut children {
            child.0.base_mut().style.layout.position = Position::Absolute;
        }
        children
    }

    fn portal(&self) -> Option<PortalSpec> {
        Some(self.spec.clone())
    }
}

crate::impl_element_builder!(Portal);

/// Margin kept between an anchored overlay and the window edges.
pub const VIEWPORT_MARGIN: f32 = 8.0;

/// Computes the top-left corner of an overlay of `size` placed next to
/// `anchor` inside `viewport`. Flips to the opposite side when the preferred
/// side lacks room and the opposite side has more, then clamps the result so
/// the overlay stays within the window where possible.
pub fn place(anchor: Bounds, size: Size, placement: Placement, gap: f32, viewport: Size) -> Point {
    let m = VIEWPORT_MARGIN;
    let align = |start: f32, len: f32, extent: f32| match placement.align {
        Alignment::Start => start,
        Alignment::Center => start + (len - extent) / 2.0,
        Alignment::End => start + len - extent,
    };
    let clamp = |v: f32, extent: f32, limit: f32| {
        let max = (limit - extent - m).max(m);
        v.clamp(m, max)
    };
    match placement.side {
        Side::Top | Side::Bottom => {
            let above = anchor.top() - gap - m;
            let below = viewport.height - anchor.bottom() - gap - m;
            let want_below = placement.side == Side::Bottom;
            let fits = if want_below {
                size.height <= below
            } else {
                size.height <= above
            };
            let below_side = if fits {
                want_below
            } else if want_below {
                below >= above
            } else {
                above < below
            };
            let y = if below_side {
                anchor.bottom() + gap
            } else {
                anchor.top() - gap - size.height
            };
            let x = align(anchor.left(), anchor.width(), size.width);
            Point::new(
                clamp(x, size.width, viewport.width),
                clamp(y, size.height, viewport.height),
            )
        }
        Side::Left | Side::Right => {
            let left_room = anchor.left() - gap - m;
            let right_room = viewport.width - anchor.right() - gap - m;
            let want_right = placement.side == Side::Right;
            let fits = if want_right {
                size.width <= right_room
            } else {
                size.width <= left_room
            };
            let right_side = if fits {
                want_right
            } else if want_right {
                right_room >= left_room
            } else {
                left_room < right_room
            };
            let x = if right_side {
                anchor.right() + gap
            } else {
                anchor.left() - gap - size.width
            };
            let y = align(anchor.top(), anchor.height(), size.height);
            Point::new(
                clamp(x, size.width, viewport.width),
                clamp(y, size.height, viewport.height),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIEW: Size = Size {
        width: 800.0,
        height: 600.0,
    };

    fn anchor(x: f32, y: f32) -> Bounds {
        Bounds::new(Point::new(x, y), Size::new(100.0, 30.0))
    }

    #[test]
    fn bottom_start_sits_below_the_anchor() {
        let p = place(
            anchor(100.0, 100.0),
            Size::new(200.0, 150.0),
            Placement::BOTTOM_START,
            6.0,
            VIEW,
        );
        assert_eq!(p, Point::new(100.0, 136.0));
    }

    #[test]
    fn flips_above_when_there_is_no_room_below() {
        let p = place(
            anchor(100.0, 500.0),
            Size::new(200.0, 150.0),
            Placement::BOTTOM_START,
            6.0,
            VIEW,
        );
        assert_eq!(p.y, 500.0 - 6.0 - 150.0);
    }

    #[test]
    fn end_alignment_and_horizontal_clamping() {
        let p = place(
            anchor(10.0, 100.0),
            Size::new(200.0, 50.0),
            Placement::BOTTOM_END,
            4.0,
            VIEW,
        );
        // Right edges would align at x = -90; clamped to the margin.
        assert_eq!(p.x, VIEWPORT_MARGIN);
        let p = place(
            anchor(750.0, 100.0),
            Size::new(200.0, 50.0),
            Placement::BOTTOM_START,
            4.0,
            VIEW,
        );
        assert_eq!(p.x, 800.0 - 200.0 - VIEWPORT_MARGIN);
    }

    #[test]
    fn side_placement_flips_left() {
        let p = place(
            anchor(700.0, 100.0),
            Size::new(150.0, 40.0),
            Placement::RIGHT,
            8.0,
            VIEW,
        );
        assert_eq!(p.x, 700.0 - 8.0 - 150.0);
        assert_eq!(p.y, 100.0 + (30.0 - 40.0) / 2.0);
    }
}
