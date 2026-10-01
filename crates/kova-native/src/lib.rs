//! Kova Native: a GPU-accelerated, Rust-first native GUI framework.
//!
//! The facade connects the existing platform, retained element tree and GPU
//! renderer. Use [`prelude`] for element builders and [`Application`] to run
//! them in a native window.
//!
//! Kova Native draws a retained UI through wgpu, solves layout with Taffy, and
//! shapes Unicode text with cosmic-text. Applications use Rust elements,
//! callbacks and signals. Windows and input are provided by the native platform
//! layer; there is no browser runtime.
//!
//! # Quick start
//!
//! Add the facade package to your application's manifest:
//!
//! ```toml
//! [dependencies]
//! kova-native = "0.2.0"
//! ```
//!
//! The package name uses hyphens; the Rust import is `kova_native`.
//! This example opens a window with a reactive counter:
//!
//! ```no_run
//! use kova_native::prelude::*;
//!
//! fn main() -> KovaResult<()> {
//!     let owner = Owner::new_root();
//!     let count = owner.with(|| signal(0_u32));
//!     let result = Application::new()
//!         .title("Kova Native counter")
//!         .size(640.0, 420.0)
//!         .run(move || {
//!             column().center().gap(16.0)
//!                 .child(text(move || format!("Count: {}", count.get())))
//!                 .child(button("Increment")
//!                     .on_click(move |_| count.update(|value| *value += 1)))
//!         });
//!     owner.dispose();
//!     result.map(|_| ())
//! }
//! ```
//!
//! Run native applications on the main thread. They need a windowing session
//! and a supported GPU backend. Window dimensions and layout lengths are logical
//! pixels; the platform's scale factor controls glyph rasterization and GPU output.
//!
//! # State and invalidation
//!
//! The reactive runtime is owned by the UI thread. [`core::Owner`] scopes signals,
//! effects and cleanup. Keep application state outside the root builder when it
//! must survive a rebuild, as in the counter above.
//!
//! Where a signal is read determines the work it invalidates:
//!
//! | Read location | Result |
//! | --- | --- |
//! | A [`text`][widgets::elements::text] closure | Update that text node |
//! | A [`bind`][widgets::Interactive::bind] style closure | Update the node's style and invalidate affected layout or paint |
//! | A [`dynamic`][widgets::elements::dynamic] builder | Rebuild that region's children |
//! | The builder passed to [`Application::run`] | Rebuild the root region |
//!
//! Reads outside a tracked callback do not subscribe a UI node. Use
//! [`core::batch`] to coalesce changes, and [`core::memo`] for derived state.
//! Region-owned state is disposed when that region is rebuilt or removed.
//!
//! # Elements, layout and events
//!
//! Start with [`widgets::elements::row`], [`widgets::elements::column`],
//! [`widgets::elements::stack`] and [`widgets::elements::div`]. The
//! [`widgets::Styled`] trait adds sizing, spacing, fills, borders and shadows.
//! [`widgets::Interactive`] adds reactive bindings, callbacks, actions and transitions. The
//! [`widgets::widgets`] module provides buttons, checkboxes, switches, sliders
//! and progress bars. [`prelude`] imports this common vocabulary.
//!
//! [`Application::key_bindings`] registers shortcuts in the existing action
//! system. Handle actions on elements with [`widgets::Interactive::on_action`].
//! Keyboard dispatch follows the focus path; pointer dispatch uses hit testing,
//! capture and bubbling through the retained element tree.
//!
//! # Subsystems
//!
//! The facade re-exports the existing library crates. Use these modules when
//! implementing custom elements or working directly with the frame pipeline:
//!
//! | Module | Responsibility |
//! | --- | --- |
//! | [`crate::core`] | Geometry, colors, identifiers, reactive state and dirty flags |
//! | [`crate::widgets`] | Elements, styling, themes, retained tree and event dispatch |
//! | [`crate::layout`] | Flexbox, grid, constraints and incremental layout |
//! | [`crate::text`] | Unicode shaping, font fallback, wrapping and glyph rasterization |
//! | [`crate::render`] | GPU context, scene commands, batching, atlases and effects |
//! | [`crate::assets`] | Image and SVG loading, rasterization and caching |
//! | [`crate::input`] | Events, keystrokes, actions and keymaps |
//! | [`crate::animation`] | Tweens, easing, springs and property transitions |
//! | [`crate::platform`] | Native windows, event loop, clipboard and IME hooks |
//!
//! # Examples and current scope
//!
//! The repository's [showcase](https://github.com/v3nn7/Kova-native/tree/main/examples/showcase)
//! exercises the same element tree, text system and renderer used by applications.
//! It includes native smoke runs and GPU captures. See the
//! [README](https://github.com/v3nn7/Kova-native#readme) for launch commands.
//!
//! The API is experimental. `text_input` provides single-line editing with
//! selection, clipboard, undo and IME composition; multi-line editing,
//! accessibility, arbitrary masks and custom shader registration are still
//! pending. The
//! [repository audit](https://github.com/v3nn7/Kova-native/blob/main/docs/AUDIT.md)
//! records the implemented subsystems and validation boundaries.

mod application;

pub use application::{Application, RunReport, WindowOptions};
pub use kova_native_animation as animation;
pub use kova_native_assets as assets;
pub use kova_native_core as core;
pub use kova_native_input as input;
pub use kova_native_layout as layout;
pub use kova_native_platform as platform;
pub use kova_native_render as render;
pub use kova_native_text as text;
pub use kova_native_widgets as widgets;

/// Common application and element-building vocabulary.
pub mod prelude {
    pub use crate::{Application, RunReport, WindowOptions};
    pub use kova_native_animation::{
        Animated, Animation, Easing, Lerp, Repeat, Spring, Transition,
    };
    pub use kova_native_assets::{ImageData, SvgData};
    pub use kova_native_core::*;
    pub use kova_native_input::{Action, Key, KeyBinding, Keystroke, Modifiers, NamedKey, actions};
    pub use kova_native_layout::{Length, Track, auto, pct, px, relative};
    pub use kova_native_text::{FontStyle, FontWeight, LineHeight, TextAlign, TextStyle, TextWrap};
    pub use kova_native_widgets::elements::*;
    pub use kova_native_widgets::widgets::*;
    pub use kova_native_widgets::{
        BoxShadow, Interactive, IntoElement, Style, Styled, Theme, set_theme, theme,
    };
}
