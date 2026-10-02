//! Identifiers for elements and resources.

use crate::SharedString;
use std::hash::{Hash, Hasher};

/// A user supplied, stable identity for an element.
///
/// Ids are used to preserve element state (scroll offsets, text editing
/// state) across rebuilds of a reactive region, and to address elements
/// programmatically (e.g. to focus them).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ElementId {
    Name(SharedString),
    Integer(u64),
    /// A name scoped by an index, handy for list items: `("row", 3)`.
    NamedInteger(SharedString, u64),
}

impl ElementId {
    /// A 64-bit hash of the id (stable within a process run).
    pub fn hash64(&self) -> u64 {
        let mut h = rustc_hash::FxHasher::default();
        self.hash(&mut h);
        h.finish()
    }
}

impl From<&'static str> for ElementId {
    fn from(s: &'static str) -> Self {
        ElementId::Name(s.into())
    }
}

impl From<String> for ElementId {
    fn from(s: String) -> Self {
        ElementId::Name(s.into())
    }
}

impl From<SharedString> for ElementId {
    fn from(s: SharedString) -> Self {
        ElementId::Name(s)
    }
}

macro_rules! integer_ids {
    ($($t:ty),*) => {$(
        impl From<$t> for ElementId {
            fn from(v: $t) -> Self {
                ElementId::Integer(v as u64)
            }
        }

        impl From<(&'static str, $t)> for ElementId {
            fn from((s, i): (&'static str, $t)) -> Self {
                ElementId::NamedInteger(s.into(), i as u64)
            }
        }

        impl From<(SharedString, $t)> for ElementId {
            fn from((s, i): (SharedString, $t)) -> Self {
                ElementId::NamedInteger(s, i as u64)
            }
        }
    )*};
}

// Negative integers wrap; ids only need to be distinct and stable.
integer_ids!(u8, u16, u32, u64, usize, i32, i64);

/// Defines a process-unique id newtype backed by an atomic counter.
#[macro_export]
macro_rules! unique_id {
    ($(#[$meta:meta])* $vis:vis struct $name:ident;) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        $vis struct $name(pub u64);

        impl $name {
            /// Allocates a new, never before used id.
            pub fn next() -> Self {
                static COUNTER: ::std::sync::atomic::AtomicU64 = ::std::sync::atomic::AtomicU64::new(1);
                Self(COUNTER.fetch_add(1, ::std::sync::atomic::Ordering::Relaxed))
            }
        }
    };
}

unique_id! {
    /// Identifies a decoded image or rasterizable vector asset.
    pub struct ImageId;
}
