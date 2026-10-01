# Repository audit — 2026-10-01

The working tree supplied for this continuation is the source of truth. It
already contained extensive uncommitted implementation files. Those systems
were retained, and no alternate renderer, layout engine or event system was
introduced. The repository has one initial commit and no prior roadmap or
milestone log; continuation points below are established from actual code.

## Workspace and API

The workspace contains ten library crates and five executable examples.
Rust 2024 / MSRV 1.89; wgpu 30, winit 0.30, Taffy 0.14, cosmic-text 0.19 and
resvg 0.48 are pinned through Cargo.lock. Each subsystem exposes its own API.
At audit start the `kova` facade contained only a module comment and all five
examples contained `fn main() {}`. `kova-native-widgets/src/tests.rs` was empty.
There were no TODO/FIXME, `todo!` or `unimplemented!` markers in the source.
Absence of markers did not imply that integration was complete.

## Existing subsystem coverage

| Crate / subsystem | Implemented in supplied source | Limits / unfinished work |
| --- | --- | --- |
| kova-native-core | Logical geometry, affine transforms, colors/fills, stable ids, shared strings, time helpers, dirty flags; Signal, Memo, effects, observers, owners, batching and cleanup | Single UI thread; panic recovery of effect flushing needs additional review |
| kova-native-render | GPU/device/surface management; ordered flat Scene built by the retained tree; one instanced pipeline; quads, independent radii, borders, two-stop Oklab gradients, outer/inset shadows, monochrome/color sprites, opacity, affine transforms, clipping and backdrop blur | Scene is repainted on scheduled frames; no retained per-node command cache, arbitrary masks, general foreground blur or custom shader registration |
| Atlas | Glyph/image/vector caches, negative cache, growable texture arrays, uploads and explicit remove | No automatic eviction/budget; assets larger than 2046 px in either dimension cannot fit a tile; texture array growth lacks an application budget |
| kova-native-layout | Incremental Taffy wrapper, row/column/flex/wrapping, block, stack via shared grid cell, constraints, margins/padding/gaps, alignment, absolute positioning, overflow and Grid | Grid already exists; scrolling and overlay scrollbars are implemented in ElementTree; no virtualization or draggable scrollbar thumbs |
| kova-native-text | System fonts/loading, fallback, advanced Unicode shaping/BiDi, glyph rasterization, width measurement cache, wrapping, alignment, hit testing, caret and selection rectangles | Caret geometry interpolates clusters by byte distance and needs grapheme/BiDi/ligature review before an editor; selection geometry is not an editable widget |
| kova-native-input | Mouse/key/IME event vocabulary, modifiers, click counting, actions, key bindings and chords | Chord timeout/replay is absent; platform translation and tree dispatch are separate layers |
| kova-native-platform | Native windows, event loop, logical input conversion, scale changes, IME area/enable hooks, cursor, clipboard, waker and timer deadline | Window creation is synchronous; no accessibility bridge; cross-platform/device behavior needs live validation |
| kova-native-widgets tree | Retained nodes, dirty queues, per-node bindings, reactive regions/views/lists, style inheritance, incremental layout, hit testing, capture/bubble, focus/Tab, hover/press/click/double-click, drag, wheel scrolling, window commands; transform-aware local event coordinates | No keyed reconciliation/virtual lists; hit clips under rotation and nested rounded masks are approximate |
| kova-native-widgets elements | Div/row/column/stack/spacer, Text, image object-fit, SVG/icon, custom Canvas; theme tokens | No actual text-input/editor widget; accepts_text_input and IME are extension hooks only; images use their base corner radii rather than resolved animated radii |
| Built-in widgets | Button variants, card, badge, divider, heading/label, switch, checkbox, transform-aware slider, progress, spinner and segmented control | No menus/tooltips/dialogs |
| kova-native-animation | Easing/cubic bezier/steps, Lerp, analytical springs, tweens, retargeting, delays, repeats and ping-pong | Visual transitions cover fill, border color, radii, shadows, opacity and transforms; layout size/position changes are immediate unless explicitly animated through bindings; spring retarget velocity direction needs further review |

## Verification at audit start

The initial `cargo check --workspace` failed on ambiguous `base_mut` in Button.
A local qualified trait call fixed compilation. The baseline suite then passed
54 unit tests and 2 doctests; all five GPU pixel tests ran successfully on this
host. Three widget documentation snippets are explicitly ignored. All existing
example binaries were launched; their original empty main functions exited
without creating windows.

## Continuation implemented

1. Retained all subsystem implementations and existing public traits. Added
   facade re-exports/prelude and a single-window Application joining the existing
   PlatformHandler, ElementTree and Renderer. Handles input, clipboard, viewport,
   scale, window commands, native IME hooks, occlusion/minimization, first-frame
   reveal, invalidation wakes, animation redraws and bounded runs.
2. Fixed reactive regions losing builder dependencies during style resolution:
   region building and style binding now use separate existing Observer
   instances. Region/style/paint notifications retain their independent meaning.
3. Added tree disposal so closing a native window releases region owners and
   signals. Stable-id focus restoration is applied during the rebuild frame.
   Interrupted native pointer gestures are cancelled on focus loss.
4. Populated the five existing example packages. Expanded showcase with
   interactive controls, retained signal labels, dark/light themes, scrolling,
   effects, motion and typography pages. Added an offscreen GPU PNG capture
   mode using the same builder and an explicit native smoke mode.
5. Added regression and showcase integration tests for reactivity, cleanup,
   incremental invalidation, focus, controls, theme changes, scrolling and
   animation removal. Rendering tests remain in the existing renderer suite.
6. Fixed backdrop captures retaining an old texture binding after the post
   uniform buffer grows, and backdrop compositing ignoring inherited opacity.
   Both defects were reproduced in actual GPU pixel tests before correction.
7. Fixed tiny scrollbars panicking when their available track is shorter than
   the minimum thumb; capture `prevent_default` now suppresses built-in target
   behavior. Pointer coordinates and slider dragging undo the painted affine
   transform, including transformed parents; singular transforms are excluded
   from hit testing. Event dispatch uses the existing small-vector helper.
8. Enabled wgpu's `std` feature so the existing environment-variable backend
   selection actually works. Fixed hidden Win32 startup by presenting the first
   frame directly, with timed retries, before revealing the window.
9. Fixed the platform wrapper overwriting exit requests with `ControlFlow::Wait`
   in `AboutToWait`. On Win32 this could leave an already completed bounded run
   sleeping until an unrelated native message arrived. Exit now selects Poll,
   and the wrapper preserves it when the event loop is exiting. Native backend
   selection retains the existing DX12/Vulkan configuration and env override.

## Verification after continuation

`cargo fmt`, `cargo check --workspace` and `cargo test --workspace` pass without
warnings. The suite now contains 65 passing unit/integration tests and 2 passing
doctests; the same 3 documentation snippets remain explicitly ignored. Seven
GPU pixel tests executed on the host GPU rather than taking the no-adapter skip.
The showcase integration test drives the actual tree at 125% scale through mouse,
keyboard, controls, scrolling, theme/root remounts, focus restoration, transformed
slider input, section changes and animation removal.

The native smoke script creates real windows and requires nonzero presentation,
successful automatic shutdown and exit code 0. After the event-loop exit fix,
all seven cases passed on NVIDIA GeForce GTX 1650 with automatic Vulkan selection:

| Native smoke case | Presented frames | Layout in last frame | Exit code |
| --- | ---: | --- | ---: |
| hello_window | 2 | false | 0 |
| buttons | 2 | false | 0 |
| layout | 2 | false | 0 |
| animation | 58 | false | 0 |
| showcase Overview | 2 | false | 0 |
| showcase Motion | 117 | false | 0 |
| showcase light Typography | 2 | false | 0 |

The five example packages also passed an earlier DX12 smoke run. After the exit
fix, three consecutive idle hello_window runs under DX12 presented 2, 2 and 3
frames and returned exit code 0. The GPU renderer suite was rerun with
`--nocapture`; all 13 renderer tests passed and no GPU test skipped. Logs are in
`target/native-smoke` and `target/gpu-tests.log`.

Counts vary with startup, vsync and interaction; they do not establish refresh
rate or latency guarantees. Native input/device-scale changes and manual
interactions have not been fully exercised by these automated runs.

GPU offscreen showcase capture at 125% produced a 1400 x 1025 PNG: 100 mounted
nodes, 621 submitted primitives, 48 culled and 2 scene draw segments. Captures
use the actual showcase builder, text system, atlas and renderer. This verifies
pixels separately from the native presentation smoke runs.
Motion and light Typography were also captured and visually inspected at the
same scale. Outputs are in `target/showcase-125.png`, `target/showcase-motion.png`
and `target/showcase-typography-light.png`.

## Next work, in priority order

Continue closing correctness gaps in event propagation, native focus/IME
behavior and clipping. Improve text cursor/selection
geometry for graphemes and RTL, then implement editable text through the existing
Element hooks. Add atlas budgeting/eviction and measured performance baselines
before caching large application scenes. Validate native Windows resize/HiDPI
changes, then Linux/macOS and accessibility. Do not claim 120/144 FPS or full IME
editing from a build or offscreen capture.
