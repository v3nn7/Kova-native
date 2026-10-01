//! Kova: a GPU-accelerated, Rust-first native GUI framework.
//!
//! The facade connects the existing platform, retained element tree and GPU
//! renderer. Use [`prelude`] for element builders and [`Application`] to run
//! them in a native window.

mod application;

pub use application::{Application, RunReport, WindowOptions};
pub use kova_animation as animation;
pub use kova_assets as assets;
pub use kova_core as core;
pub use kova_input as input;
pub use kova_layout as layout;
pub use kova_platform as platform;
pub use kova_render as render;
pub use kova_text as text;
pub use kova_widgets as widgets;

/// Common application and element-building vocabulary.
pub mod prelude {
    pub use crate::{Application, RunReport, WindowOptions};
    pub use kova_animation::{Animated, Animation, Easing, Lerp, Repeat, Spring, Transition};
    pub use kova_assets::{ImageData, SvgData};
    pub use kova_core::*;
    pub use kova_input::{Action, Key, KeyBinding, Keystroke, Modifiers, NamedKey, actions};
    pub use kova_layout::{Length, Track, auto, pct, px, relative};
    pub use kova_text::{FontStyle, FontWeight, LineHeight, TextAlign, TextStyle, TextWrap};
    pub use kova_widgets::elements::*;
    pub use kova_widgets::widgets::*;
    pub use kova_widgets::{
        BoxShadow, Interactive, IntoElement, Style, Styled, Theme, set_theme, theme,
    };
}
