//! Foundation types for Kova Native.
//!
//! `kova-native-core` has no knowledge of windows, GPUs or layout. It provides the
//! vocabulary shared by every other crate: geometry, colors and fills,
//! identifiers, cheap strings, durations, dirty flags and the fine-grained
//! reactive runtime that drives invalidation, plus UI-thread timers and
//! async tasks.

pub mod color;
pub mod dirty;
pub mod error;
pub mod geometry;
pub mod id;
pub mod reactive;
pub mod shared_string;
pub mod task;
pub mod time;
pub mod timer;

pub use color::{Color, ColorStop, Fill, LinearGradient, hsla, linear_gradient, rgb, rgba};
pub use dirty::Dirty;
pub use error::{KovaError, KovaResult};
pub use geometry::{Bounds, Corners, Edges, Point, Size, Transform2D, bounds, point, size};
pub use id::{ElementId, ImageId};
pub use reactive::{
    Memo, Observer, Owner, Signal, batch, effect, memo, on_cleanup, signal, untrack,
};
pub use shared_string::SharedString;
pub use time::{Duration, DurationExt, Instant};
