//! Responsive layout: the window size as reactive state, and breakpoints.
//!
//! [`viewport_size`] and [`breakpoint`] are reactive. Read them in a `bind`
//! closure to restyle a node, or in a [`dynamic`](crate::elements::dynamic)
//! region (or [`responsive`]) to switch structure. [`breakpoint`] only
//! notifies when the window crosses a breakpoint, not on every resize.
//!
//! ```ignore
//! column()
//!     .bind(|s| if breakpoint() >= Breakpoint::Md { s.px(32.0) } else { s.px(12.0) })
//!     .child(responsive(|bp| if bp < Breakpoint::Md { compact_nav() } else { sidebar() }))
//! ```
//!
//! The values describe the most recently resized element tree on this
//! thread (one per native window in practice).

use crate::element::IntoElement;
use crate::elements::{Region, dynamic};
use kova_native_core::{Memo, Owner, Signal, Size};
use std::cell::OnceCell;

/// Width classes, ordered from narrowest to widest.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Breakpoint {
    /// Below 640 px: phones in portrait, very narrow windows.
    Xs,
    /// 640 px and wider.
    Sm,
    /// 768 px and wider.
    Md,
    /// 1024 px and wider.
    Lg,
    /// 1280 px and wider.
    Xl,
}

impl Breakpoint {
    /// Minimum logical width of each breakpoint.
    pub const fn min_width(self) -> f32 {
        match self {
            Breakpoint::Xs => 0.0,
            Breakpoint::Sm => 640.0,
            Breakpoint::Md => 768.0,
            Breakpoint::Lg => 1024.0,
            Breakpoint::Xl => 1280.0,
        }
    }

    /// The breakpoint containing `width` (logical px).
    pub fn from_width(width: f32) -> Breakpoint {
        [
            Breakpoint::Xl,
            Breakpoint::Lg,
            Breakpoint::Md,
            Breakpoint::Sm,
        ]
        .into_iter()
        .find(|b| width >= b.min_width())
        .unwrap_or(Breakpoint::Xs)
    }
}

struct Viewport {
    size: Signal<Size>,
    breakpoint: Memo<Breakpoint>,
}

thread_local! {
    static VIEWPORT: OnceCell<Viewport> = const { OnceCell::new() };
}

fn with_viewport<R>(f: impl FnOnce(&Viewport) -> R) -> R {
    VIEWPORT.with(|cell| {
        f(cell.get_or_init(|| {
            // A dedicated root: no UI rebuild may dispose it.
            Owner::new_root().with(|| {
                let size = kova_native_core::signal(Size::new(800.0, 600.0));
                let breakpoint =
                    kova_native_core::memo(move || Breakpoint::from_width(size.get().width));
                Viewport { size, breakpoint }
            })
        }))
    })
}

/// The window's logical size. Reactive.
pub fn viewport_size() -> Size {
    with_viewport(|v| v.size.get())
}

/// The window's current [`Breakpoint`]. Reactive; changes only when a
/// breakpoint boundary is crossed.
pub fn breakpoint() -> Breakpoint {
    with_viewport(|v| v.breakpoint.get())
}

pub(crate) fn set_viewport_size(size: Size) {
    with_viewport(|v| {
        v.size.set_if_changed(size);
    });
}

/// A region rebuilt with the current breakpoint whenever it changes.
pub fn responsive<E: IntoElement>(build: impl Fn(Breakpoint) -> E + 'static) -> Region {
    dynamic(move || build(breakpoint()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn breakpoints_follow_widths() {
        assert_eq!(Breakpoint::from_width(320.0), Breakpoint::Xs);
        assert_eq!(Breakpoint::from_width(640.0), Breakpoint::Sm);
        assert_eq!(Breakpoint::from_width(1023.9), Breakpoint::Md);
        assert_eq!(Breakpoint::from_width(1024.0), Breakpoint::Lg);
        assert_eq!(Breakpoint::from_width(1920.0), Breakpoint::Xl);
        assert!(Breakpoint::Sm < Breakpoint::Lg);
    }
}
