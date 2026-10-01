//! Animation primitives for Kova Native.
//!
//! * [`Easing`] — CSS compatible and custom easing curves.
//! * [`Lerp`] — interpolation for numbers, points, colors, fills, corners...
//! * [`Spring`] — analytically solved springs (frame-rate independent).
//! * [`Transition`] — how a property moves to a new value (tween or spring).
//! * [`Animated`] — a value that transitions whenever its target changes,
//!   preserving position and velocity when interrupted.
//! * [`Animation`] — repeating / ping-pong time based animations.
//!
//! This crate is renderer-agnostic: time is always passed in explicitly,
//! which makes everything deterministic and testable.

mod animated;
mod easing;
mod lerp;
mod spring;
mod transition;

pub use animated::{Animated, Animation, Repeat};
pub use easing::Easing;
pub use lerp::Lerp;
pub use spring::Spring;
pub use transition::{Timing, Transition};
