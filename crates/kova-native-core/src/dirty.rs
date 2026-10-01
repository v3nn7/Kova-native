//! Dirty tracking flags used by the element tree to do minimal work per frame.

use std::ops::{BitAnd, BitOr, BitOrAssign, Not};

/// What part of the frame pipeline has to be redone for a node.
#[derive(Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct Dirty(u8);

impl Dirty {
    pub const NONE: Dirty = Dirty(0);
    /// The effective style must be re-resolved (state change or binding update).
    pub const STYLE: Dirty = Dirty(1 << 0);
    /// The node's layout inputs (style or intrinsic size) changed.
    pub const LAYOUT: Dirty = Dirty(1 << 1);
    /// Only the visual output changed.
    pub const PAINT: Dirty = Dirty(1 << 2);
    /// Reactive bindings of this node must be re-evaluated.
    pub const BINDINGS: Dirty = Dirty(1 << 3);
    /// A reactive region must rebuild its children.
    pub const REBUILD: Dirty = Dirty(1 << 4);

    pub const fn bits(self) -> u8 {
        self.0
    }

    pub const fn contains(self, other: Dirty) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn intersects(self, other: Dirty) -> bool {
        self.0 & other.0 != 0
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn insert(&mut self, other: Dirty) {
        self.0 |= other.0;
    }

    pub fn remove(&mut self, other: Dirty) {
        self.0 &= !other.0;
    }

    /// Returns the current flags and clears them.
    pub fn take(&mut self) -> Dirty {
        std::mem::take(self)
    }
}

impl BitOr for Dirty {
    type Output = Dirty;
    fn bitor(self, rhs: Dirty) -> Dirty {
        Dirty(self.0 | rhs.0)
    }
}

impl BitOrAssign for Dirty {
    fn bitor_assign(&mut self, rhs: Dirty) {
        self.0 |= rhs.0;
    }
}

impl BitAnd for Dirty {
    type Output = Dirty;
    fn bitand(self, rhs: Dirty) -> Dirty {
        Dirty(self.0 & rhs.0)
    }
}

impl Not for Dirty {
    type Output = Dirty;
    fn not(self) -> Dirty {
        Dirty(!self.0)
    }
}

impl std::fmt::Debug for Dirty {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let names = [
            (Dirty::STYLE, "STYLE"),
            (Dirty::LAYOUT, "LAYOUT"),
            (Dirty::PAINT, "PAINT"),
            (Dirty::BINDINGS, "BINDINGS"),
            (Dirty::REBUILD, "REBUILD"),
        ];
        let set: Vec<_> = names
            .iter()
            .filter(|(d, _)| self.contains(*d))
            .map(|(_, n)| *n)
            .collect();
        write!(f, "Dirty({})", set.join("|"))
    }
}
