//! Fine-grained reactive state.
//!
//! Kova Native's reactivity is built from four primitives:
//!
//! * [`Signal<T>`] — a piece of mutable state. Reading it inside a tracking
//!   scope subscribes that scope; writing it notifies subscribers.
//! * [`effect`] — a side effect that re-runs whenever the signals it read
//!   change.
//! * [`Memo<T>`] — a derived value that only notifies its dependents when the
//!   computed value actually changes.
//! * [`Observer`] — a low level subscriber used by the element tree: instead
//!   of re-running code immediately it calls a `notify` callback, which lets
//!   the UI mark exactly one node dirty and schedule a frame.
//!
//! Every signal/effect is created under an [`Owner`]. Disposing an owner
//! disposes everything created under it, so rebuilding a part of the UI
//! never leaks state.
//!
//! The runtime is single threaded (one per thread, typically the UI thread);
//! handles are `!Send`.

use slotmap::{SlotMap, new_key_type};
use smallvec::SmallVec;
use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::fmt;
use std::marker::PhantomData;
use std::rc::Rc;

new_key_type! {
    struct SignalKey;
    struct SubKey;
    struct OwnerKey;
}

type AnyValue = Rc<RefCell<Box<dyn Any>>>;
type EffectFn = Rc<RefCell<dyn FnMut()>>;

struct SignalSlot {
    value: AnyValue,
    subscribers: SmallVec<[SubKey; 4]>,
}

enum SubKind {
    Effect(EffectFn),
    External(Rc<dyn Fn()>),
}

struct SubSlot {
    kind: SubKind,
    sources: SmallVec<[SignalKey; 4]>,
    dirty: bool,
    /// Scope owning everything created during the last run (effects only).
    run_owner: Option<OwnerKey>,
    /// The owner that was current when the subscriber was created.
    parent_owner: Option<OwnerKey>,
}

#[derive(Default)]
struct OwnerSlot {
    parent: Option<OwnerKey>,
    children: Vec<OwnerKey>,
    signals: Vec<SignalKey>,
    subscribers: Vec<SubKey>,
    cleanups: Vec<Box<dyn FnOnce()>>,
}

#[derive(Default)]
struct Inner {
    signals: SlotMap<SignalKey, SignalSlot>,
    subs: SlotMap<SubKey, SubSlot>,
    owners: SlotMap<OwnerKey, OwnerSlot>,
    observer: Option<SubKey>,
    owner: Option<OwnerKey>,
    pending: VecDeque<SubKey>,
}

#[derive(Default)]
struct Runtime {
    inner: RefCell<Inner>,
    batch_depth: Cell<u32>,
    flushing: Cell<bool>,
}

thread_local! {
    static RUNTIME: Runtime = Runtime::default();
}

fn with_runtime<R>(f: impl FnOnce(&Runtime) -> R) -> R {
    RUNTIME.with(f)
}

impl Runtime {
    fn create_signal(&self, value: Box<dyn Any>) -> SignalKey {
        let mut inner = self.inner.borrow_mut();
        let key = inner.signals.insert(SignalSlot {
            value: Rc::new(RefCell::new(value)),
            subscribers: SmallVec::new(),
        });
        if let Some(owner) = inner.owner
            && let Some(o) = inner.owners.get_mut(owner)
        {
            o.signals.push(key);
        }
        key
    }

    /// Registers a read of `key` by the current observer and returns the value cell.
    fn read(&self, key: SignalKey, track: bool) -> Option<AnyValue> {
        let mut inner = self.inner.borrow_mut();
        let value = inner.signals.get(key)?.value.clone();
        if track && let Some(obs) = inner.observer {
            let Inner { signals, subs, .. } = &mut *inner;
            if let Some(sub) = subs.get_mut(obs)
                && !sub.sources.contains(&key)
            {
                sub.sources.push(key);
                if let Some(sig) = signals.get_mut(key) {
                    sig.subscribers.push(obs);
                }
            }
        }
        Some(value)
    }

    /// Marks every subscriber of `key` dirty; external ones are notified
    /// immediately, effects are queued and flushed (unless batching).
    fn notify(&self, key: SignalKey) {
        let mut externals: SmallVec<[Rc<dyn Fn()>; 4]> = SmallVec::new();
        {
            let mut inner = self.inner.borrow_mut();
            let Some(sig) = inner.signals.get(key) else {
                return;
            };
            let subscribers = sig.subscribers.clone();
            for sub_key in subscribers {
                let Some(sub) = inner.subs.get_mut(sub_key) else {
                    continue;
                };
                if sub.dirty {
                    continue;
                }
                sub.dirty = true;
                match &sub.kind {
                    SubKind::Effect(_) => inner.pending.push_back(sub_key),
                    SubKind::External(notify) => externals.push(notify.clone()),
                }
            }
        }
        for notify in externals {
            notify();
        }
        if self.batch_depth.get() == 0 {
            self.flush();
        }
    }

    fn flush(&self) {
        if self.flushing.replace(true) {
            return;
        }
        let mut iterations = 0usize;
        loop {
            let next = {
                let mut inner = self.inner.borrow_mut();
                inner.pending.pop_front()
            };
            let Some(sub) = next else { break };
            iterations += 1;
            if iterations > 100_000 {
                log::error!("kova-native reactive: effect flush did not converge (cyclic effect?)");
                self.inner.borrow_mut().pending.clear();
                break;
            }
            self.run_effect(sub);
        }
        self.flushing.set(false);
    }

    fn clear_sources(inner: &mut Inner, sub_key: SubKey) {
        let Inner { signals, subs, .. } = inner;
        if let Some(sub) = subs.get_mut(sub_key) {
            for source in sub.sources.drain(..) {
                if let Some(sig) = signals.get_mut(source) {
                    sig.subscribers.retain(|s| *s != sub_key);
                }
            }
        }
    }

    fn run_effect(&self, sub_key: SubKey) {
        let (f, prev_observer, prev_owner, old_run_owner) = {
            let mut inner = self.inner.borrow_mut();
            let Some(sub) = inner.subs.get_mut(sub_key) else {
                return;
            };
            let SubKind::Effect(f) = &sub.kind else {
                return;
            };
            let f = f.clone();
            sub.dirty = false;
            let old_run_owner = sub.run_owner.take();
            let parent_owner = sub.parent_owner;
            Self::clear_sources(&mut inner, sub_key);
            (
                f,
                inner.observer,
                inner.owner,
                (old_run_owner, parent_owner),
            )
        };
        let (old_run_owner, parent_owner) = old_run_owner;
        if let Some(old) = old_run_owner {
            self.dispose_owner(old);
        }
        let run_owner = {
            let mut inner = self.inner.borrow_mut();
            let parent = parent_owner.filter(|p| inner.owners.contains_key(*p));
            let key = inner.owners.insert(OwnerSlot {
                parent,
                ..Default::default()
            });
            if let Some(p) = parent
                && let Some(p) = inner.owners.get_mut(p)
            {
                p.children.push(key);
            }
            if let Some(sub) = inner.subs.get_mut(sub_key) {
                sub.run_owner = Some(key);
            }
            inner.observer = Some(sub_key);
            inner.owner = Some(key);
            key
        };
        let _ = run_owner;
        {
            // A panic inside an effect would leave the runtime in a bad state;
            // restore observer/owner via a guard.
            struct Restore<'a>(&'a Runtime, Option<SubKey>, Option<OwnerKey>);
            impl Drop for Restore<'_> {
                fn drop(&mut self) {
                    if let Ok(mut inner) = self.0.inner.try_borrow_mut() {
                        inner.observer = self.1;
                        inner.owner = self.2;
                    }
                }
            }
            let _restore = Restore(self, prev_observer, prev_owner);
            (f.borrow_mut())();
        }
    }

    fn dispose_sub(&self, sub_key: SubKey) {
        let run_owner = {
            let mut inner = self.inner.borrow_mut();
            Self::clear_sources(&mut inner, sub_key);
            inner.pending.retain(|s| *s != sub_key);
            inner.subs.remove(sub_key).and_then(|s| s.run_owner)
        };
        if let Some(o) = run_owner {
            self.dispose_owner(o);
        }
    }

    fn dispose_owner(&self, key: OwnerKey) {
        let slot = {
            let mut inner = self.inner.borrow_mut();
            let Some(slot) = inner.owners.remove(key) else {
                return;
            };
            if let Some(parent) = slot.parent
                && let Some(p) = inner.owners.get_mut(parent)
            {
                p.children.retain(|c| *c != key);
            }
            slot
        };
        for child in slot.children {
            self.dispose_owner(child);
        }
        for sub in slot.subscribers {
            self.dispose_sub(sub);
        }
        {
            let mut inner = self.inner.borrow_mut();
            for sig in slot.signals {
                inner.signals.remove(sig);
            }
        }
        for cleanup in slot.cleanups.into_iter().rev() {
            cleanup();
        }
    }
}

/// A reactive, mutable value.
///
/// `Signal` is a small `Copy` handle; the value lives in the thread's
/// reactive runtime and is dropped when the owning scope is disposed.
pub struct Signal<T: 'static> {
    key: SignalKey,
    _marker: UiThreadOnly<T>,
}

/// Covariant in `T`, but neither `Send` nor `Sync`: signals live in the
/// UI thread's runtime.
type UiThreadOnly<T> = PhantomData<(fn() -> T, *const ())>;

impl<T> Clone for Signal<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Signal<T> {}

impl<T> PartialEq for Signal<T> {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}

impl<T> Eq for Signal<T> {}

impl<T> fmt::Debug for Signal<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Signal<{}>({:?})", std::any::type_name::<T>(), self.key)
    }
}

/// Creates a new [`Signal`] owned by the current reactive scope.
pub fn signal<T: 'static>(value: T) -> Signal<T> {
    let key = with_runtime(|rt| rt.create_signal(Box::new(value)));
    Signal {
        key,
        _marker: PhantomData,
    }
}

impl<T: 'static> Signal<T> {
    pub fn new(value: T) -> Self {
        signal(value)
    }

    fn cell(&self, track: bool) -> AnyValue {
        with_runtime(|rt| rt.read(self.key, track)).unwrap_or_else(|| {
            panic!(
                "Signal<{}> accessed after its owner was disposed",
                std::any::type_name::<T>()
            )
        })
    }

    /// Borrows the value, subscribing the current tracking scope.
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        let cell = self.cell(true);
        let value = cell.borrow();
        f(value.downcast_ref::<T>().expect("signal type mismatch"))
    }

    /// Borrows the value without subscribing.
    pub fn with_untracked<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        let cell = self.cell(false);
        let value = cell.borrow();
        f(value.downcast_ref::<T>().expect("signal type mismatch"))
    }

    /// Returns `true` if the signal's owner has not been disposed.
    pub fn is_alive(&self) -> bool {
        with_runtime(|rt| rt.inner.borrow().signals.contains_key(self.key))
    }

    /// Replaces the value and notifies subscribers.
    pub fn set(&self, value: T) {
        self.update(|v| *v = value);
    }

    /// Mutates the value in place and notifies subscribers.
    pub fn update<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        let r = self.update_untracked(f);
        with_runtime(|rt| rt.notify(self.key));
        r
    }

    /// Mutates the value without notifying anyone.
    pub fn update_untracked<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        let cell = self.cell(false);
        let mut value = cell.borrow_mut();
        f(value.downcast_mut::<T>().expect("signal type mismatch"))
    }

    /// Notifies subscribers without changing the value.
    pub fn notify(&self) {
        with_runtime(|rt| rt.notify(self.key));
    }
}

impl<T: Clone + 'static> Signal<T> {
    /// Returns a clone of the value, subscribing the current tracking scope.
    pub fn get(&self) -> T {
        self.with(T::clone)
    }

    /// Returns a clone of the value without subscribing.
    pub fn get_untracked(&self) -> T {
        self.with_untracked(T::clone)
    }
}

impl<T: PartialEq + 'static> Signal<T> {
    /// Sets the value only if it differs, avoiding needless invalidation.
    pub fn set_if_changed(&self, value: T) -> bool {
        let changed = self.with_untracked(|v| *v != value);
        if changed {
            self.set(value);
        }
        changed
    }
}

impl Signal<bool> {
    /// Flips a boolean signal.
    pub fn toggle(&self) {
        self.update(|v| *v = !*v);
    }
}

/// A derived value recomputed when its dependencies change. Dependents are
/// only notified when the new value differs from the old one.
pub struct Memo<T: 'static> {
    signal: Signal<T>,
}

impl<T> Clone for Memo<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Memo<T> {}

/// Creates a [`Memo`] from a computation.
pub fn memo<T: PartialEq + 'static>(mut compute: impl FnMut() -> T + 'static) -> Memo<T> {
    let key = with_runtime(|rt| rt.create_signal(Box::new(())));
    let signal: Signal<T> = Signal {
        key,
        _marker: PhantomData,
    };
    let mut initialized = false;
    effect(move || {
        let value = compute();
        if !initialized {
            initialized = true;
            let cell = signal.cell(false);
            *cell.borrow_mut() = Box::new(value);
        } else {
            signal.set_if_changed(value);
        }
    });
    Memo { signal }
}

impl<T: 'static> Memo<T> {
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        self.signal.with(f)
    }

    pub fn with_untracked<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        self.signal.with_untracked(f)
    }
}

impl<T: Clone + 'static> Memo<T> {
    pub fn get(&self) -> T {
        self.signal.get()
    }

    pub fn get_untracked(&self) -> T {
        self.signal.get_untracked()
    }
}

/// Runs `f` now and again whenever any signal it read changes.
///
/// The effect lives as long as the current owner.
pub fn effect(f: impl FnMut() + 'static) {
    with_runtime(|rt| {
        let key = {
            let mut inner = rt.inner.borrow_mut();
            let parent_owner = inner.owner;
            let key = inner.subs.insert(SubSlot {
                kind: SubKind::Effect(Rc::new(RefCell::new(f))),
                sources: SmallVec::new(),
                dirty: false,
                run_owner: None,
                parent_owner,
            });
            if let Some(owner) = inner.owner
                && let Some(o) = inner.owners.get_mut(owner)
            {
                o.subscribers.push(key);
            }
            key
        };
        rt.run_effect(key);
    });
}

/// Runs `f` without tracking any signal reads.
pub fn untrack<R>(f: impl FnOnce() -> R) -> R {
    let prev = with_runtime(|rt| rt.inner.borrow_mut().observer.take());
    struct Restore(Option<SubKey>);
    impl Drop for Restore {
        fn drop(&mut self) {
            let prev = self.0;
            let _ = RUNTIME.try_with(|rt| {
                if let Ok(mut inner) = rt.inner.try_borrow_mut() {
                    inner.observer = prev;
                }
            });
        }
    }
    let _restore = Restore(prev);
    f()
}

/// Groups several signal writes: effects run once, after `f` returns.
pub fn batch<R>(f: impl FnOnce() -> R) -> R {
    with_runtime(|rt| rt.batch_depth.set(rt.batch_depth.get() + 1));
    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            let _ = RUNTIME.try_with(|rt| {
                let depth = rt.batch_depth.get() - 1;
                rt.batch_depth.set(depth);
                if depth == 0 && !std::thread::panicking() {
                    rt.flush();
                }
            });
        }
    }
    let _guard = Guard;
    f()
}

/// Registers a callback to run when the current owner is disposed.
pub fn on_cleanup(f: impl FnOnce() + 'static) {
    with_runtime(|rt| {
        let mut inner = rt.inner.borrow_mut();
        if let Some(owner) = inner.owner
            && let Some(o) = inner.owners.get_mut(owner)
        {
            o.cleanups.push(Box::new(f));
        }
    });
}

/// A reactive ownership scope.
///
/// Signals, memos, effects and child owners created while an owner is
/// current (see [`Owner::with`]) are disposed together with it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Owner {
    key: OwnerKey,
}

impl Owner {
    /// Creates a new owner as a child of the current one (or a root owner).
    pub fn new() -> Owner {
        with_runtime(|rt| {
            let mut inner = rt.inner.borrow_mut();
            let parent = inner.owner;
            let key = inner.owners.insert(OwnerSlot {
                parent,
                ..Default::default()
            });
            if let Some(p) = parent
                && let Some(p) = inner.owners.get_mut(p)
            {
                p.children.push(key);
            }
            Owner { key }
        })
    }

    /// Creates a root owner with no parent.
    pub fn new_root() -> Owner {
        with_runtime(|rt| {
            let key = rt.inner.borrow_mut().owners.insert(OwnerSlot::default());
            Owner { key }
        })
    }

    /// The currently active owner, if any.
    pub fn current() -> Option<Owner> {
        with_runtime(|rt| rt.inner.borrow().owner.map(|key| Owner { key }))
    }

    /// Runs `f` with this owner as the current owner.
    pub fn with<R>(&self, f: impl FnOnce() -> R) -> R {
        let prev = with_runtime(|rt| rt.inner.borrow_mut().owner.replace(self.key));
        struct Restore(Option<OwnerKey>);
        impl Drop for Restore {
            fn drop(&mut self) {
                let prev = self.0;
                let _ = RUNTIME.try_with(|rt| {
                    if let Ok(mut inner) = rt.inner.try_borrow_mut() {
                        inner.owner = prev;
                    }
                });
            }
        }
        let _restore = Restore(prev);
        f()
    }

    pub fn is_alive(&self) -> bool {
        with_runtime(|rt| rt.inner.borrow().owners.contains_key(self.key))
    }

    /// Disposes the owner and everything it owns.
    pub fn dispose(self) {
        let _ = RUNTIME.try_with(|rt| rt.dispose_owner(self.key));
    }
}

impl Default for Owner {
    fn default() -> Self {
        Owner::new()
    }
}

/// A low level reactive subscriber.
///
/// Signals read inside [`Observer::track`] become its dependencies. When one
/// of them changes the observer is marked dirty and its `notify` callback is
/// invoked (once, until it tracks again). The element tree uses observers to
/// map a signal change to precisely the nodes that depend on it.
pub struct Observer {
    key: SubKey,
}

impl Observer {
    pub fn new(notify: impl Fn() + 'static) -> Observer {
        let key = with_runtime(|rt| {
            rt.inner.borrow_mut().subs.insert(SubSlot {
                kind: SubKind::External(Rc::new(notify)),
                sources: SmallVec::new(),
                dirty: false,
                run_owner: None,
                parent_owner: None,
            })
        });
        Observer { key }
    }

    /// Runs `f`, replacing this observer's dependencies with the signals `f` reads.
    pub fn track<R>(&self, f: impl FnOnce() -> R) -> R {
        let prev = with_runtime(|rt| {
            let mut inner = rt.inner.borrow_mut();
            Runtime::clear_sources(&mut inner, self.key);
            if let Some(sub) = inner.subs.get_mut(self.key) {
                sub.dirty = false;
            }
            inner.observer.replace(self.key)
        });
        struct Restore(Option<SubKey>);
        impl Drop for Restore {
            fn drop(&mut self) {
                let prev = self.0;
                let _ = RUNTIME.try_with(|rt| {
                    if let Ok(mut inner) = rt.inner.try_borrow_mut() {
                        inner.observer = prev;
                    }
                });
            }
        }
        let _restore = Restore(prev);
        f()
    }

    /// Whether a dependency changed since the last [`Observer::track`].
    pub fn is_dirty(&self) -> bool {
        with_runtime(|rt| {
            rt.inner
                .borrow()
                .subs
                .get(self.key)
                .is_some_and(|s| s.dirty)
        })
    }

    /// Number of signals this observer currently depends on.
    pub fn dependency_count(&self) -> usize {
        with_runtime(|rt| {
            rt.inner
                .borrow()
                .subs
                .get(self.key)
                .map_or(0, |s| s.sources.len())
        })
    }
}

impl Drop for Observer {
    fn drop(&mut self) {
        let key = self.key;
        let _ = RUNTIME.try_with(|rt| {
            if rt.inner.try_borrow_mut().is_ok() {
                rt.dispose_sub(key);
            }
        });
    }
}

impl fmt::Debug for Observer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Observer({:?})", self.key)
    }
}

/// Number of live signals in this thread's runtime (useful for leak tests).
pub fn live_signal_count() -> usize {
    with_runtime(|rt| rt.inner.borrow().signals.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signal_get_set() {
        let s = signal(1);
        assert_eq!(s.get(), 1);
        s.set(5);
        assert_eq!(s.get(), 5);
        s.update(|v| *v += 1);
        assert_eq!(s.get_untracked(), 6);
    }

    #[test]
    fn effect_reruns_on_change() {
        let owner = Owner::new_root();
        let log = Rc::new(RefCell::new(Vec::new()));
        let s = owner.with(|| signal(0));
        {
            let log = log.clone();
            owner.with(|| effect(move || log.borrow_mut().push(s.get())));
        }
        s.set(1);
        s.set(2);
        assert_eq!(*log.borrow(), vec![0, 1, 2]);
        owner.dispose();
        assert!(!s.is_alive());
    }

    #[test]
    fn batch_coalesces_effects() {
        let owner = Owner::new_root();
        let runs = Rc::new(Cell::new(0));
        let (a, b) = owner.with(|| (signal(1), signal(2)));
        {
            let runs = runs.clone();
            owner.with(|| {
                effect(move || {
                    let _ = a.get() + b.get();
                    runs.set(runs.get() + 1);
                })
            });
        }
        batch(|| {
            a.set(10);
            b.set(20);
        });
        assert_eq!(runs.get(), 2);
        owner.dispose();
    }

    #[test]
    fn memo_only_notifies_on_change() {
        let owner = Owner::new_root();
        let runs = Rc::new(Cell::new(0));
        let n = owner.with(|| signal(3));
        let parity = owner.with(|| memo(move || n.get() % 2));
        {
            let runs = runs.clone();
            owner.with(|| {
                effect(move || {
                    let _ = parity.get();
                    runs.set(runs.get() + 1);
                })
            });
        }
        n.set(5); // parity unchanged
        assert_eq!(runs.get(), 1);
        n.set(6);
        assert_eq!(runs.get(), 2);
        assert_eq!(parity.get(), 0);
        owner.dispose();
    }

    #[test]
    fn observer_notifies_once_until_retracked() {
        let s = signal(0);
        let hits = Rc::new(Cell::new(0));
        let obs = {
            let hits = hits.clone();
            Observer::new(move || hits.set(hits.get() + 1))
        };
        obs.track(|| s.get());
        s.set(1);
        s.set(2);
        assert_eq!(hits.get(), 1);
        assert!(obs.is_dirty());
        obs.track(|| s.get());
        s.set(3);
        assert_eq!(hits.get(), 2);
        drop(obs);
        s.set(4);
        assert_eq!(hits.get(), 2);
    }

    #[test]
    fn dynamic_dependencies() {
        let owner = Owner::new_root();
        let (flag, a, b) = owner.with(|| (signal(true), signal(1), signal(2)));
        let obs = Observer::new(|| {});
        let value = obs.track(|| if flag.get() { a.get() } else { b.get() });
        assert_eq!(value, 1);
        assert_eq!(obs.dependency_count(), 2);
        b.set(5);
        assert!(!obs.is_dirty(), "b is not a dependency yet");
        flag.set(false);
        assert!(obs.is_dirty());
        owner.dispose();
    }

    #[test]
    fn effect_scope_disposes_children() {
        let owner = Owner::new_root();
        let trigger = owner.with(|| signal(0));
        let before = live_signal_count();
        owner.with(|| {
            effect(move || {
                let _ = trigger.get();
                let _inner = signal(String::from("scratch"));
            })
        });
        for i in 0..10 {
            trigger.set(i);
        }
        // Only the latest run's inner signal is alive.
        assert_eq!(live_signal_count(), before + 1);
        owner.dispose();
    }

    #[test]
    fn cleanup_runs_on_dispose() {
        let owner = Owner::new_root();
        let cleaned = Rc::new(Cell::new(false));
        {
            let cleaned = cleaned.clone();
            owner.with(|| on_cleanup(move || cleaned.set(true)));
        }
        owner.dispose();
        assert!(cleaned.get());
    }
}
