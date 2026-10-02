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

/// Produces the children of a [`Keyed`] list. See [`keyed`].
pub trait KeyedSource: 'static {
    /// Reads the current items (signals read here are tracked) and returns
    /// one key per item, in display order.
    fn keys(&mut self) -> Vec<u64>;
    /// Builds the element for a key returned by the latest [`KeyedSource::keys`]
    /// call. Called once per key while that key stays in the list.
    fn build(&mut self, key: u64) -> AnyElement;
}

/// A list that reconciles its children by key. See [`keyed`].
pub struct Keyed {
    base: ElementBase,
    source: Box<dyn KeyedSource>,
}

impl Element for Keyed {
    fn base(&self) -> &ElementBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut ElementBase {
        &mut self.base
    }

    fn name(&self) -> &'static str {
        "keyed"
    }

    fn keyed(&mut self) -> Option<&mut dyn KeyedSource> {
        Some(&mut *self.source)
    }
}

crate::impl_element_builder!(Keyed);

struct TypedSource<T, K, R> {
    items: Box<dyn Fn() -> Vec<T>>,
    key: Box<dyn Fn(&T) -> K>,
    render: R,
    pending: rustc_hash::FxHashMap<u64, T>,
}

impl<T, K, E, R> KeyedSource for TypedSource<T, K, R>
where
    T: 'static,
    K: std::hash::Hash + 'static,
    E: IntoElement,
    R: FnMut(T) -> E + 'static,
{
    fn keys(&mut self) -> Vec<u64> {
        use std::hash::{BuildHasher, BuildHasherDefault};
        let hasher = BuildHasherDefault::<rustc_hash::FxHasher>::default();
        self.pending.clear();
        let mut keys = Vec::new();
        for item in (self.items)() {
            let key = hasher.hash_one((self.key)(&item));
            // Duplicate keys keep their first item.
            if let std::collections::hash_map::Entry::Vacant(slot) = self.pending.entry(key) {
                slot.insert(item);
                keys.push(key);
            }
        }
        keys
    }

    fn build(&mut self, key: u64) -> AnyElement {
        match self.pending.remove(&key) {
            Some(item) => (self.render)(item).into_any(),
            None => crate::elements::empty().into_any(),
        }
    }
}

/// A reactive list that keeps one retained subtree per key.
///
/// `items` is re-read when the signals it reads change. Items whose key is
/// already present keep their nodes, state, focus, scroll position and
/// running transitions; they are only moved. New keys are rendered with
/// `render`; removed keys are unmounted and their reactive state disposed.
/// `render` runs once per key, so per-item changes should flow through
/// signals inside the item (or change the key).
///
/// ```ignore
/// keyed(move || todos.get(), |t| t.id, |t| row().child(text(t.title)))
/// ```
pub fn keyed<T, K, E>(
    items: impl Fn() -> Vec<T> + 'static,
    key: impl Fn(&T) -> K + 'static,
    render: impl FnMut(T) -> E + 'static,
) -> Keyed
where
    T: 'static,
    K: std::hash::Hash + 'static,
    E: IntoElement,
{
    use crate::style::Styled;
    Keyed {
        base: ElementBase::new(),
        source: Box::new(TypedSource {
            items: Box::new(items),
            key: Box::new(key),
            render,
            pending: Default::default(),
        }),
    }
    .flex_col()
}
