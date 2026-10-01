//! Reactive regions and views.

use crate::context::EventCx;
use crate::element::{AnyElement, Element, ElementBase, IntoElement};
use kova_native_core::Signal;
use std::cell::RefCell;
use std::rc::Rc;

/// A container whose children are produced by a builder closure. The
/// builder re-runs — replacing only this region's children — whenever a
/// signal it read changes.
pub struct Region {
    base: ElementBase,
    builder: Box<dyn FnMut() -> Vec<AnyElement>>,
}

impl Region {
    pub fn new(builder: impl FnMut() -> Vec<AnyElement> + 'static) -> Self {
        Region {
            base: ElementBase::new(),
            builder: Box::new(builder),
        }
    }
}

impl Element for Region {
    fn base(&self) -> &ElementBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut ElementBase {
        &mut self.base
    }

    fn name(&self) -> &'static str {
        "region"
    }

    fn region(&mut self) -> Option<&mut dyn FnMut() -> Vec<AnyElement>> {
        Some(&mut *self.builder)
    }
}

crate::impl_element_builder!(Region);

/// Structural reactivity: rebuilds its content when signals read inside
/// `build` change.
///
/// ```ignore
/// dynamic(move || if logged_in.get() { dashboard().into_any() } else { login().into_any() })
/// ```
pub fn dynamic<E: IntoElement>(mut build: impl FnMut() -> E + 'static) -> Region {
    Region::new(move || vec![build().into_any()])
}

/// Renders a list reactively: `items` is re-read (and the list rebuilt)
/// when the signals it reads change.
pub fn list<T, E: IntoElement>(
    items: impl Fn() -> Vec<T> + 'static,
    mut render: impl FnMut(usize, T) -> E + 'static,
) -> Region {
    Region::new(move || {
        items()
            .into_iter()
            .enumerate()
            .map(|(i, item)| render(i, item).into_any())
            .collect()
    })
    .with_column()
}

trait WithColumn {
    fn with_column(self) -> Self;
}

impl WithColumn for Region {
    fn with_column(self) -> Self {
        use crate::style::Styled;
        self.flex_col()
    }
}

/// A stateful component in the style of retained-mode toolkits: `render`
/// re-runs when the view calls [`ViewContext::notify`] (directly or via a
/// [`ViewContext::listener`]) or when a signal it read changes.
pub trait View: 'static {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement
    where
        Self: Sized;
}

/// Handle given to [`View::render`].
pub struct ViewContext<V> {
    state: Rc<RefCell<V>>,
    version: Signal<u64>,
}

impl<V: 'static> ViewContext<V> {
    /// Marks the view as changed; it re-renders on the next frame.
    pub fn notify(&self) {
        self.version.update(|v| *v = v.wrapping_add(1));
    }

    /// Wraps a handler that mutates the view state and re-renders it.
    pub fn listener(
        &self,
        f: impl Fn(&mut V, &mut EventCx) + 'static,
    ) -> impl Fn(&mut EventCx) + 'static {
        let state = self.state.clone();
        let version = self.version;
        move |cx| {
            f(&mut state.borrow_mut(), cx);
            version.update(|v| *v = v.wrapping_add(1));
        }
    }

    /// A handle for mutating the view from elsewhere (timers, other views).
    pub fn handle(&self) -> ViewHandle<V> {
        ViewHandle {
            state: self.state.clone(),
            version: self.version,
        }
    }
}

/// Shared access to a view's state.
pub struct ViewHandle<V> {
    state: Rc<RefCell<V>>,
    version: Signal<u64>,
}

impl<V> Clone for ViewHandle<V> {
    fn clone(&self) -> Self {
        ViewHandle {
            state: self.state.clone(),
            version: self.version,
        }
    }
}

impl<V: 'static> ViewHandle<V> {
    /// Mutates the state and schedules a re-render.
    pub fn update<R>(&self, f: impl FnOnce(&mut V) -> R) -> R {
        let r = f(&mut self.state.borrow_mut());
        self.version.update(|v| *v = v.wrapping_add(1));
        r
    }

    pub fn read<R>(&self, f: impl FnOnce(&V) -> R) -> R {
        f(&self.state.borrow())
    }
}

/// Mounts a [`View`].
pub fn view<V: View>(state: V) -> Region {
    let state = Rc::new(RefCell::new(state));
    let version = kova_native_core::signal(0u64);
    Region::new(move || {
        version.with(|_| ());
        let mut cx = ViewContext {
            state: state.clone(),
            version,
        };
        let element = state.borrow_mut().render(&mut cx).into_any();
        vec![element]
    })
}
