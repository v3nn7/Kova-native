//! Cheaply clonable immutable strings.

use std::borrow::Borrow;
use std::fmt;
use std::ops::Deref;
use std::rc::Rc;

/// An immutable string that is either `'static` or reference counted.
///
/// Cloning never allocates, which matters because text content flows through
/// element builders, bindings and caches.
#[derive(Clone)]
pub enum SharedString {
    Static(&'static str),
    Owned(Rc<str>),
}

impl SharedString {
    pub const fn new_static(s: &'static str) -> Self {
        SharedString::Static(s)
    }

    pub fn as_str(&self) -> &str {
        match self {
            SharedString::Static(s) => s,
            SharedString::Owned(s) => s,
        }
    }
}

impl Default for SharedString {
    fn default() -> Self {
        SharedString::Static("")
    }
}

impl Deref for SharedString {
    type Target = str;
    fn deref(&self) -> &str {
        self.as_str()
    }
}

impl AsRef<str> for SharedString {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Borrow<str> for SharedString {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl PartialEq for SharedString {
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}

impl Eq for SharedString {}

impl PartialEq<str> for SharedString {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for SharedString {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl std::hash::Hash for SharedString {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.as_str().hash(state)
    }
}

impl fmt::Debug for SharedString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_str(), f)
    }
}

impl fmt::Display for SharedString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self.as_str(), f)
    }
}

impl From<&'static str> for SharedString {
    fn from(s: &'static str) -> Self {
        SharedString::Static(s)
    }
}

impl From<String> for SharedString {
    fn from(s: String) -> Self {
        SharedString::Owned(s.into())
    }
}

impl From<&String> for SharedString {
    fn from(s: &String) -> Self {
        SharedString::Owned(s.as_str().into())
    }
}

impl From<Rc<str>> for SharedString {
    fn from(s: Rc<str>) -> Self {
        SharedString::Owned(s)
    }
}

impl From<char> for SharedString {
    fn from(c: char) -> Self {
        SharedString::Owned(c.to_string().into())
    }
}
