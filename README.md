# Kova Native

A native, GPU accelerated GUI framework for Rust. Kova Native combines retained elements,
fine grained reactive state, incremental Taffy layout, cosmic-text shaping and an
instanced wgpu renderer. No HTML, CSS runtime, JavaScript or WebView is involved.

## Crates.io packages

The package and Rust API use the new name throughout:

```toml
[dependencies]
kova-native = "0.1.1"
```

Import the framework with `use kova_native::prelude::*`. Subsystem packages use
the `kova-native-*` prefix and Rust imports such as `kova_native_core` and
`kova_native_render`.
Read the [API documentation and quick start](https://docs.rs/kova-native/latest/kova_native/).
Kova Native requires Rust 1.89 or newer. See [the publishing guide](https://github.com/v3nn7/Kova-native/blob/main/docs/PUBLISHING.md)
for package verification and release commands.

```powershell
cargo run -p showcase
```

The showcase includes working buttons, checkbox, switch, slider, progress bar,
theme switching, scrolling, SVG icons, gradients, shadows, glass backdrops,
spring transitions, repeating animations and Unicode text. Use Tab and Enter to
navigate and activate controls. The Motion and Typography pages use the same
retained element tree as the overview.

```powershell
cargo run -p showcase -- --page components
cargo run -p showcase -- --page motion
cargo run -p showcase -- --page typography --light
```

## A small application

```rust,no_run
use kova_native::prelude::*;

fn main() -> KovaResult<()> {
    let owner = Owner::new_root();
    let count = owner.with(|| signal(0));
    let result = Application::new()
        .title("My Kova Native app")
        .size(640.0, 420.0)
        .run(move || {
            column().center().gap(16.0)
                .child(text(move || format!("Count: {}", count.get())))
                .child(button("Increment").on_click(move |_| count.update(|n| *n += 1)))
        });
    owner.dispose();
    result.map(|_| ())
}
```

`run` mounts a reactive root builder. Reads inside a `text` or `bind` closure
update an individual node. Reads in a `dynamic` builder replace only that
region's children. Reads directly in the application builder can rebuild the
whole root (for example when changing themes). Own persistent application state
outside that builder; region-owned state is disposed when its region is rebuilt.

`WindowOptions` exposes the existing native window attributes and renderer
surface settings, including minimum size, vsync and maximum frame latency.
`Application::key_bindings` uses the existing action/keymap system; handle actions
with `.on_action` on an element on the focus path. `kova_native::prelude` contains common
builders; `kova_native::{core, input, layout, text, animation, assets, widgets, platform,
render}` re-export their respective crates.

## Validation and examples

```powershell
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
./scripts/smoke-examples.ps1
```

The five existing example packages are `hello_window`, `buttons`, `layout`,
`animation` and `showcase`. Every example accepts `--smoke`, runs a real native
window for a bounded time and requires at least one presented frame. Static
examples sleep after invalidation settles; animated examples keep requesting
frames. Presentation uses the configured display vsync; this is not an FPS
benchmark.

The smoke script also exercises the showcase's animated Motion and light
Typography pages.

```powershell
cargo run -p showcase -- --smoke
cargo run -p showcase -- --capture target/showcase.png 1.25
cargo run -p showcase -- --capture target/motion.png 1.25 --page motion
```

Capture uses the same showcase builder, ElementTree, text system and renderer,
but renders offscreen. The optional argument is the device scale (0.5–3.0).
Native window presentation and offscreen capture are separate verification paths.
Renderer pixel tests skip when no GPU adapter is available; the explicit capture
and native smoke commands fail when initialization fails.

The renderer respects `WGPU_BACKEND` (for example `$env:WGPU_BACKEND='dx12'`
or `'vulkan'` in PowerShell). Leave it unset for automatic native backend
selection. The native smoke script saves stdout/stderr in `target/native-smoke`.

See [the repository audit](https://github.com/v3nn7/Kova-native/blob/main/docs/AUDIT.md) for subsystem coverage and remaining
work. The public API is experimental; editable text, accessibility, arbitrary
masks and custom shader registration are not implemented yet.

## License

Kova Native is licensed under the [Mozilla Public License 2.0](LICENSE).

This Source Code Form is subject to the terms of the Mozilla Public License,
v. 2.0. If a copy of the MPL was not distributed with this file, You can obtain
one at https://mozilla.org/MPL/2.0/.
