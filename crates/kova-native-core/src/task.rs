//! Async tasks on the UI thread.
//!
//! [`spawn_local`] runs a `!Send` future on the UI thread. Futures may read
//! and write signals directly; the window repaints whatever those writes
//! invalidate. Blocking or CPU-heavy work belongs on a worker thread through
//! [`spawn_blocking`], whose result is awaited on the UI thread:
//!
//! ```no_run
//! use kova_native_core::{signal, task, DurationExt};
//! let status = signal(String::from("loading"));
//! task::spawn_local(async move {
//!     let bytes = task::spawn_blocking(|| std::fs::read("data.json")).await;
//!     task::sleep(200.ms()).await;
//!     status.set(match bytes {
//!         Ok(b) => format!("{} bytes", b.len()),
//!         Err(e) => e.to_string(),
//!     });
//! });
//! ```
//!
//! A task spawned while an [`Owner`](crate::Owner) is current is dropped
//! (cancelled at its next await point) when that owner is disposed.
//!
//! The window integration polls woken tasks once per event-loop turn through
//! [`run_ready`] and installs a cross-thread wake hook with [`set_wake_hook`].

use crate::reactive::{Owner, batch, on_cleanup};
use crate::time::Duration;
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Wake, Waker};

type LocalFuture = Pin<Box<dyn Future<Output = ()>>>;
type WakeHook = Arc<dyn Fn() + Send + Sync>;

/// Woken task ids of one UI thread, shared with wakers on any thread.
#[derive(Default)]
struct ReadyQueue {
    ids: Mutex<Vec<u64>>,
    hook: Mutex<Option<WakeHook>>,
}

thread_local! {
    static QUEUE: Arc<ReadyQueue> = Arc::new(ReadyQueue::default());
    static TASKS: RefCell<BTreeMap<u64, LocalFuture>> = const { RefCell::new(BTreeMap::new()) };
    static NEXT_ID: Cell<u64> = const { Cell::new(0) };
}

fn ready_queue() -> Arc<ReadyQueue> {
    QUEUE.with(Arc::clone)
}

struct TaskWaker {
    id: u64,
    queue: Arc<ReadyQueue>,
}

impl Wake for TaskWaker {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        let queue = &self.queue;
        queue.ids.lock().expect("ready queue").push(self.id);
        let hook = queue.hook.lock().expect("wake hook").clone();
        if let Some(hook) = hook {
            hook();
        }
    }
}

/// Handle to a spawned task.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TaskHandle(u64);

impl TaskHandle {
    /// Drops the task's future. Has no effect on a finished task.
    pub fn cancel(self) {
        let _ = TASKS.try_with(|t| {
            if let Ok(mut t) = t.try_borrow_mut() {
                t.remove(&self.0);
            }
        });
    }

    /// Whether the task is still pending.
    pub fn is_pending(self) -> bool {
        TASKS.with(|t| t.borrow().contains_key(&self.0))
    }
}

/// Installs the function used to wake this thread's UI event loop when a task
/// becomes ready (called from any thread). The native application does this.
pub fn set_wake_hook(hook: impl Fn() + Send + Sync + 'static) {
    *ready_queue().hook.lock().expect("wake hook") = Some(Arc::new(hook));
}

/// Spawns a future on the UI thread. It is first polled by the next
/// [`run_ready`], not synchronously.
pub fn spawn_local(future: impl Future<Output = ()> + 'static) -> TaskHandle {
    let id = NEXT_ID.with(|n| {
        n.set(n.get() + 1);
        n.get()
    });
    TASKS.with(|t| t.borrow_mut().insert(id, Box::pin(future)));
    if Owner::current().is_some() {
        on_cleanup(move || TaskHandle(id).cancel());
    }
    Arc::new(TaskWaker {
        id,
        queue: ready_queue(),
    })
    .wake();
    TaskHandle(id)
}

/// Whether any task has been woken and waits to be polled.
pub fn has_ready() -> bool {
    !ready_queue().ids.lock().expect("ready queue").is_empty()
}

/// Number of live (unfinished) tasks on this thread.
pub fn pending_count() -> usize {
    TASKS.with(|t| t.borrow().len())
}

/// Polls every woken task. Returns how many polls ran.
pub fn run_ready() -> usize {
    let queue = ready_queue();
    let mut polls = 0;
    for _ in 0..16 {
        let mut ids = std::mem::take(&mut *queue.ids.lock().expect("ready queue"));
        if ids.is_empty() {
            break;
        }
        ids.sort_unstable();
        ids.dedup();
        batch(|| {
            for id in ids {
                // Take the future out so it may spawn or cancel other tasks.
                let Some(mut future) = TASKS.with(|t| t.borrow_mut().remove(&id)) else {
                    continue;
                };
                let waker = Waker::from(Arc::new(TaskWaker {
                    id,
                    queue: queue.clone(),
                }));
                let mut cx = Context::from_waker(&waker);
                polls += 1;
                if future.as_mut().poll(&mut cx).is_pending() {
                    TASKS.with(|t| t.borrow_mut().insert(id, future));
                }
            }
        });
    }
    polls
}

struct Shared<T> {
    value: Option<std::thread::Result<T>>,
    waker: Option<Waker>,
}

/// The result of [`spawn_blocking`].
pub struct Blocking<T> {
    shared: Arc<Mutex<Shared<T>>>,
}

impl<T> Future for Blocking<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T> {
        let mut shared = self.shared.lock().expect("blocking result");
        match shared.value.take() {
            Some(Ok(value)) => Poll::Ready(value),
            Some(Err(panic)) => {
                drop(shared);
                std::panic::resume_unwind(panic)
            }
            None => {
                shared.waker = Some(cx.waker().clone());
                Poll::Pending
            }
        }
    }
}

/// Runs `f` on a new worker thread; await the returned future for its result.
/// A panic in `f` resumes in the awaiting task.
pub fn spawn_blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Blocking<T> {
    let shared = Arc::new(Mutex::new(Shared {
        value: None,
        waker: None,
    }));
    let worker = shared.clone();
    std::thread::Builder::new()
        .name("kova-blocking".into())
        .spawn(move || {
            let value = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
            let waker = {
                let mut s = worker.lock().expect("blocking result");
                s.value = Some(value);
                s.waker.take()
            };
            if let Some(waker) = waker {
                waker.wake();
            }
        })
        .expect("spawn blocking worker thread");
    Blocking { shared }
}

/// Completes after `duration`, measured on the UI frame clock
/// (see [`crate::timer`]).
pub fn sleep(duration: Duration) -> Sleep {
    Sleep {
        duration,
        state: None,
    }
}

/// Future returned by [`sleep`].
pub struct Sleep {
    duration: Duration,
    state: Option<Rc<SleepState>>,
}

struct SleepState {
    done: Cell<bool>,
    waker: RefCell<Option<Waker>>,
    timer: Cell<Option<crate::timer::TimerId>>,
}

impl Future for Sleep {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if let Some(state) = &self.state {
            if state.done.get() {
                return Poll::Ready(());
            }
            *state.waker.borrow_mut() = Some(cx.waker().clone());
            return Poll::Pending;
        }
        let state = Rc::new(SleepState {
            done: Cell::new(false),
            waker: RefCell::new(Some(cx.waker().clone())),
            timer: Cell::new(None),
        });
        let fire = state.clone();
        // Not tied to the current owner: the sleeping future owns the timer.
        let id = crate::timer::set_timeout_unowned(self.duration, move || {
            fire.done.set(true);
            if let Some(waker) = fire.waker.borrow_mut().take() {
                waker.wake();
            }
        });
        state.timer.set(Some(id));
        self.state = Some(state);
        Poll::Pending
    }
}

impl Drop for Sleep {
    fn drop(&mut self) {
        if let Some(state) = &self.state
            && let Some(id) = state.timer.get()
        {
            crate::timer::clear_timer(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DurationExt, Instant, signal, timer};

    #[test]
    fn local_task_runs_and_sleeps_on_the_frame_clock() {
        let start = Instant::now();
        timer::set_frame_time(start);
        let value = signal(0);
        let handle = spawn_local(async move {
            value.set(1);
            sleep(100.ms()).await;
            value.set(2);
        });
        assert!(has_ready());
        run_ready();
        assert_eq!(value.get(), 1);
        assert!(handle.is_pending());
        timer::run_due(start + 50.ms());
        run_ready();
        assert_eq!(value.get(), 1);
        timer::run_due(start + 100.ms());
        run_ready();
        assert_eq!(value.get(), 2);
        assert!(!handle.is_pending());
    }

    #[test]
    fn blocking_work_wakes_the_task() {
        let value = signal(0);
        spawn_local(async move {
            let n = spawn_blocking(|| 21 * 2).await;
            value.set(n);
        });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while value.get() != 42 {
            assert!(
                std::time::Instant::now() < deadline,
                "worker never woke the task"
            );
            run_ready();
            std::thread::yield_now();
        }
    }

    #[test]
    fn disposing_the_owner_cancels_the_task() {
        let owner = Owner::new_root();
        let handle = owner.with(|| spawn_local(std::future::pending()));
        run_ready();
        assert!(handle.is_pending());
        owner.dispose();
        assert!(!handle.is_pending());
    }
}
