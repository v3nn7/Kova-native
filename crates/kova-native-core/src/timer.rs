//! UI-thread timers and the frame clock.
//!
//! Timers run on the UI thread, between frames, inside a reactive
//! [`batch`](crate::batch). They never run while a signal is being written or
//! a handler is executing, so callbacks may freely read and write signals.
//!
//! A timer created while an [`Owner`](crate::Owner) is current is cancelled
//! when that owner is disposed. A timer created in a region therefore stops
//! when the region is rebuilt or unmounted.
//!
//! Deadlines are measured against the *frame clock*: the time the element
//! tree is currently processing ([`frame_time`]). Native windows use the
//! system clock; headless runs and recordings can advance time
//! deterministically, and timers follow.
//!
//! ```
//! use kova_native_core::{signal, timer, DurationExt, Instant};
//! let fired = signal(false);
//! let start = Instant::now();
//! timer::set_frame_time(start);
//! timer::set_timeout(300.ms(), move || fired.set(true));
//! assert_eq!(timer::run_due(start + 100.ms()), 0);
//! assert_eq!(timer::run_due(start + 300.ms()), 1);
//! assert!(fired.get());
//! ```

use crate::reactive::{Owner, batch, on_cleanup};
use crate::time::{Duration, Instant};
use std::cell::{Cell, RefCell};
use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap};

thread_local! {
    static FRAME_TIME: Cell<Option<Instant>> = const { Cell::new(None) };
    static TIMERS: RefCell<Timers> = RefCell::new(Timers::default());
}

/// Identifies a pending timer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TimerId(u64);

impl TimerId {
    /// Cancels the timer. Cancelling a fired or cancelled timer does nothing.
    pub fn cancel(self) {
        clear_timer(self);
    }
}

enum Callback {
    Once(Box<dyn FnOnce()>),
    Repeat(Box<dyn FnMut()>, Duration),
}

#[derive(Default)]
struct Timers {
    next_id: u64,
    queue: BinaryHeap<Reverse<(Instant, u64)>>,
    callbacks: BTreeMap<u64, (Instant, Callback)>,
    /// The interval currently executing and whether it cancelled itself.
    running: Option<(u64, bool)>,
}

/// The time currently being processed by the UI (falls back to the system clock).
pub fn frame_time() -> Instant {
    FRAME_TIME.with(Cell::get).unwrap_or_else(Instant::now)
}

/// Sets the frame clock. Called by the element tree before handling input
/// or producing a frame; headless drivers use it to simulate time.
pub fn set_frame_time(now: Instant) {
    FRAME_TIME.with(|t| t.set(Some(now)));
}

fn schedule(delay: Duration, callback: Callback) -> TimerId {
    let id = schedule_detached(delay, callback);
    if Owner::current().is_some() {
        on_cleanup(move || clear_timer(id));
    }
    id
}

fn schedule_detached(delay: Duration, callback: Callback) -> TimerId {
    let deadline = frame_time() + delay;
    TIMERS.with(|t| {
        let mut t = t.borrow_mut();
        t.next_id += 1;
        let id = t.next_id;
        t.queue.push(Reverse((deadline, id)));
        t.callbacks.insert(id, (deadline, callback));
        TimerId(id)
    })
}

/// Like [`set_timeout`], but never cancelled by owner disposal. The
/// callback must tolerate state that was disposed in the meantime (check
/// [`Signal::is_alive`](crate::Signal::is_alive)).
pub fn set_timeout_unowned(delay: Duration, f: impl FnOnce() + 'static) -> TimerId {
    schedule_detached(delay, Callback::Once(Box::new(f)))
}

/// Runs `f` once after `delay`.
pub fn set_timeout(delay: Duration, f: impl FnOnce() + 'static) -> TimerId {
    schedule(delay, Callback::Once(Box::new(f)))
}

/// Runs `f` every `period` until cancelled (or until its owner is disposed).
/// Missed periods are not replayed: one call per due check at most.
pub fn set_interval(period: Duration, f: impl FnMut() + 'static) -> TimerId {
    let period = period.max(Duration::from_millis(1));
    schedule(period, Callback::Repeat(Box::new(f), period))
}

/// Cancels a pending timer.
pub fn clear_timer(id: TimerId) {
    let _ = TIMERS.try_with(|t| {
        if let Ok(mut t) = t.try_borrow_mut() {
            t.callbacks.remove(&id.0);
            if let Some((running, cancelled)) = &mut t.running
                && *running == id.0
            {
                *cancelled = true;
            }
        }
    });
}

/// Earliest pending deadline, if any.
pub fn next_deadline() -> Option<Instant> {
    TIMERS.with(|t| {
        let mut t = t.borrow_mut();
        // Drop cancelled entries from the front of the queue.
        while let Some(Reverse((deadline, id))) = t.queue.peek().copied() {
            match t.callbacks.get(&id) {
                Some((d, _)) if *d == deadline => return Some(deadline),
                _ => {
                    t.queue.pop();
                }
            }
        }
        None
    })
}

/// Number of pending timers.
pub fn pending_count() -> usize {
    TIMERS.with(|t| t.borrow().callbacks.len())
}

/// Runs every timer due at `now`. Returns how many callbacks ran.
pub fn run_due(now: Instant) -> usize {
    let mut ran = 0;
    // Bounded so a zero-delay timer that re-arms itself cannot spin forever.
    for _ in 0..64 {
        let due = TIMERS.with(|t| {
            let mut t = t.borrow_mut();
            let mut due = Vec::new();
            while let Some(Reverse((deadline, id))) = t.queue.peek().copied() {
                if deadline > now {
                    break;
                }
                t.queue.pop();
                if t.callbacks.get(&id).is_some_and(|(d, _)| *d == deadline) {
                    let (_, callback) = t.callbacks.remove(&id).expect("checked");
                    due.push((id, callback));
                }
            }
            due
        });
        if due.is_empty() {
            break;
        }
        batch(|| {
            for (id, callback) in due {
                ran += 1;
                match callback {
                    Callback::Once(f) => f(),
                    Callback::Repeat(mut f, period) => {
                        TIMERS.with(|t| t.borrow_mut().running = Some((id, false)));
                        f();
                        TIMERS.with(|t| {
                            let mut t = t.borrow_mut();
                            // `f` may have cancelled itself through its id.
                            if let Some((_, true)) = t.running.take() {
                                return;
                            }
                            let deadline = now + period;
                            t.queue.push(Reverse((deadline, id)));
                            t.callbacks
                                .insert(id, (deadline, Callback::Repeat(f, period)));
                        });
                    }
                }
            }
        });
    }
    ran
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DurationExt, Owner, signal};

    #[test]
    fn timeouts_run_in_deadline_order() {
        let start = Instant::now();
        set_frame_time(start);
        let log = std::rc::Rc::new(RefCell::new(Vec::new()));
        for (delay, name) in [(30, "c"), (10, "a"), (20, "b")] {
            let log = log.clone();
            set_timeout(delay.ms(), move || log.borrow_mut().push(name));
        }
        assert_eq!(next_deadline(), Some(start + 10.ms()));
        assert_eq!(run_due(start + 25.ms()), 2);
        assert_eq!(*log.borrow(), ["a", "b"]);
        assert_eq!(run_due(start + 30.ms()), 1);
        assert_eq!(next_deadline(), None);
    }

    #[test]
    fn interval_repeats_until_cancelled() {
        let start = Instant::now();
        set_frame_time(start);
        let count = std::rc::Rc::new(Cell::new(0));
        let c = count.clone();
        let id = set_interval(100.ms(), move || c.set(c.get() + 1));
        run_due(start + 100.ms());
        run_due(start + 200.ms());
        assert_eq!(count.get(), 2);
        id.cancel();
        run_due(start + 1000.ms());
        assert_eq!(count.get(), 2);
        assert_eq!(next_deadline(), None);
    }

    #[test]
    fn interval_can_cancel_itself() {
        let start = Instant::now();
        set_frame_time(start);
        let count = std::rc::Rc::new(Cell::new(0));
        let id = std::rc::Rc::new(Cell::new(None::<TimerId>));
        let (c, slot) = (count.clone(), id.clone());
        id.set(Some(set_interval(10.ms(), move || {
            c.set(c.get() + 1);
            if c.get() == 3 {
                slot.get().unwrap().cancel();
            }
        })));
        for i in 1..10 {
            run_due(start + (i * 10).ms());
        }
        assert_eq!(count.get(), 3);
        assert_eq!(pending_count(), 0);
    }

    #[test]
    fn owner_disposal_cancels_timers() {
        let start = Instant::now();
        set_frame_time(start);
        let owner = Owner::new_root();
        let flag = owner.with(|| {
            let flag = signal(false);
            set_timeout(50.ms(), move || flag.set(true));
            flag
        });
        assert_eq!(pending_count(), 1);
        owner.dispose();
        assert_eq!(pending_count(), 0);
        assert_eq!(run_due(start + 100.ms()), 0);
        assert!(!flag.is_alive());
    }
}
