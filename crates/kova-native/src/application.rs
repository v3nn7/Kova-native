//! Native application integration. No layout, rendering or event logic lives here.

use kova_native_core::{Color, Duration, Instant, KovaError, KovaResult, Size};
use kova_native_input::{KeyBinding, Keymap};
use kova_native_platform::{
    PlatformContext, PlatformEvent, PlatformHandler, PlatformWindow, PlatformWindowId,
    WindowAttributes,
};
use kova_native_render::{GpuContext, Renderer, Scene, SceneStats, SurfaceOptions, WindowSurface};
use kova_native_text::{TextStyle, TextSystem};
use kova_native_widgets::{
    DispatchContext, ElementTree, FrameContext, FrameStats, IntoElement, WindowCommand,
};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

/// Native window and presentation settings, using the platform/renderer options.
#[derive(Clone, Debug)]
pub struct WindowOptions {
    /// Native title, logical dimensions, minimum size and window attributes.
    pub attributes: WindowAttributes,
    /// Swapchain presentation settings, including vsync and frame latency.
    pub surface: SurfaceOptions,
    /// Color used to clear the window before drawing the scene.
    pub background: Color,
    /// Inherited text defaults for the root element tree.
    pub text_style: TextStyle,
    /// When true (the default) the root background, text color and text
    /// size follow the current theme instead of `background`/`text_style`,
    /// so theme switches restyle text that has no explicit color.
    pub follow_theme: bool,
}

impl Default for WindowOptions {
    fn default() -> Self {
        let t = kova_native_widgets::theme();
        Self {
            attributes: WindowAttributes::default(),
            surface: SurfaceOptions::default(),
            background: t.background,
            text_style: TextStyle {
                size: t.font_size,
                color: t.text,
                ..Default::default()
            },
            follow_theme: true,
        }
    }
}

/// Measurements from frames actually presented to the native surface.
#[derive(Clone, Copy, Debug, Default)]
pub struct RunReport {
    /// Frames successfully presented to the native swapchain.
    pub presented_frames: u64,
    /// Element-tree work performed for the last presented frame.
    pub last_frame: FrameStats,
    /// Primitive, culling and draw-call counts for the last presented scene.
    pub last_scene: SceneStats,
}

/// Runs a retained UI in one native window. The event loop sleeps while idle;
/// animations request redraws paced by the swapchain's presentation mode.
#[derive(Default)]
pub struct Application {
    options: WindowOptions,
    bindings: Vec<KeyBinding>,
    on_file_drop: Option<Box<dyn FnMut(PathBuf)>>,
    on_close_request: Option<Box<dyn FnMut() -> bool>>,
    run_for: Option<Duration>,
}

impl Application {
    /// Creates an application using the current theme and default window options.
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces the native window, presentation and root text settings.
    pub fn window(mut self, options: WindowOptions) -> Self {
        self.options = options;
        self
    }

    /// Sets the native window's title.
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.options.attributes.title = title.into();
        self
    }

    /// Initial inner size in logical pixels.
    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.options.attributes.size = Size::new(width, height);
        self
    }

    /// Appends shortcuts to the action keymap used by the focused element path.
    ///
    /// Register action handlers with [`kova_native_widgets::Interactive::on_action`].
    pub fn key_bindings(mut self, bindings: impl IntoIterator<Item = KeyBinding>) -> Self {
        self.bindings.extend(bindings);
        self
    }

    /// Registers a callback for files dropped onto the application window.
    ///
    /// The operating system reports each dropped file separately, so the
    /// callback runs once per file when multiple files are dropped together.
    pub fn on_file_drop(mut self, callback: impl FnMut(PathBuf) + 'static) -> Self {
        self.on_file_drop = Some(Box::new(callback));
        self
    }

    /// Intercepts window close requests. Return `true` to close or `false` to
    /// keep the window open. With no callback, close requests are accepted.
    pub fn on_close_request(mut self, callback: impl FnMut() -> bool + 'static) -> Self {
        self.on_close_request = Some(Box::new(callback));
        self
    }

    /// Ends the event loop after a bounded duration, even when the UI is idle.
    /// Useful for native startup smoke tests and automated example runs.
    pub fn run_for(mut self, duration: Duration) -> Self {
        self.run_for = Some(duration);
        self
    }

    /// Mounts the root builder and runs until the window closes.
    ///
    /// Signals read directly by `build` rebuild its region. Read frequently
    /// changing state in `text`/`bind` closures to update individual nodes.
    /// State that must survive root rebuilds should be owned outside `build`.
    ///
    /// Call this on the main thread with an active native windowing session.
    /// The returned [`RunReport`] describes frames actually presented. Platform,
    /// GPU initialization and rendering failures are returned as [`KovaError`].
    pub fn run<E: IntoElement>(
        self,
        mut build: impl FnMut() -> E + 'static,
    ) -> KovaResult<RunReport> {
        let _ = env_logger::Builder::from_env(
            env_logger::Env::default().default_filter_or("kova_native=info"),
        )
        .try_init();
        let shared = Rc::new(RefCell::new(Outcome::default()));
        let mut keymap = Keymap::new();
        keymap.extend(self.bindings);
        let handler = Runner {
            options: self.options,
            build: Some(Box::new(move || build().into_any())),
            state: None,
            keymap,
            on_file_drop: self.on_file_drop,
            on_close_request: self.on_close_request,
            run_for: self.run_for,
            deadline: None,
            shared: shared.clone(),
        };
        kova_native_platform::run(handler)?;
        let mut outcome = shared.borrow_mut();
        match outcome.error.take() {
            Some(error) => Err(error),
            None => Ok(outcome.report),
        }
    }
}

#[derive(Default)]
struct Outcome {
    report: RunReport,
    error: Option<KovaError>,
}

struct NativeWindow {
    window: PlatformWindow,
    surface: WindowSurface,
    gpu: GpuContext,
    renderer: Renderer,
    tree: ElementTree,
    scene: Scene,
    text: TextSystem,
    clipboard: SystemClipboard,
    occluded: bool,
    animating: bool,
    redraw_pending: bool,
    retry_at: Option<Instant>,
    shown: bool,
    focused: bool,
}

struct SystemClipboard(kova_native_platform::Clipboard);

impl kova_native_widgets::Clipboard for SystemClipboard {
    fn read_text(&mut self) -> Option<String> {
        self.0.read_text()
    }
    fn write_text(&mut self, text: &str) {
        self.0.write_text(text);
    }
}

struct Runner {
    options: WindowOptions,
    build: Option<Box<dyn FnMut() -> kova_native_widgets::AnyElement>>,
    state: Option<NativeWindow>,
    keymap: Keymap,
    on_file_drop: Option<Box<dyn FnMut(PathBuf)>>,
    on_close_request: Option<Box<dyn FnMut() -> bool>>,
    run_for: Option<Duration>,
    deadline: Option<Instant>,
    shared: Rc<RefCell<Outcome>>,
}

impl Runner {
    fn init_window(&mut self, cx: &mut PlatformContext) -> KovaResult<NativeWindow> {
        log::debug!("kova-native: creating native window");
        let window = cx.create_window(&self.options.attributes)?;
        log::debug!("kova-native: creating GPU instance and surface");
        let instance = GpuContext::create_instance();
        let raw_surface = WindowSurface::create_surface(&instance, window.clone())?;
        log::debug!("kova-native: requesting surface-compatible GPU");
        let gpu = GpuContext::new(instance, Some(&raw_surface))?;
        let (w, h) = window.physical_size();
        let surface = WindowSurface::new(&gpu, raw_surface, w, h, self.options.surface)?;
        let renderer = Renderer::new(&gpu);
        log::debug!("kova-native: mounting element tree");
        let build = self.build.take().expect("builder mounted once");
        let mut tree = if self.options.follow_theme {
            ElementTree::themed(build)
        } else {
            ElementTree::new(
                build,
                self.options.text_style.clone(),
                self.options.background,
            )
        };
        tree.set_viewport(window.logical_size(), window.scale_factor());
        let waker = cx.waker();
        tree.set_waker(move || waker.wake());
        // Async tasks may be woken from worker threads.
        let task_waker = cx.waker();
        kova_native_core::task::set_wake_hook(move || task_waker.wake());
        log::debug!("kova-native: native renderer ready");
        Ok(NativeWindow {
            window,
            surface,
            gpu,
            renderer,
            tree,
            scene: Scene::new(),
            text: TextSystem::new(),
            clipboard: SystemClipboard(kova_native_platform::Clipboard::new()),
            occluded: false,
            animating: false,
            redraw_pending: false,
            retry_at: None,
            shown: false,
            focused: true,
        })
    }

    fn draw(&mut self) {
        let Some(state) = &mut self.state else { return };
        state.redraw_pending = false;
        let size = state.window.physical_size();
        if state.occluded || size.0 == 0 || size.1 == 0 {
            return;
        }
        if !state.shown {
            log::debug!("kova-native: acquiring first surface frame");
        }
        // Acquire before consuming invalidation, so a missed surface frame
        // cannot strand a dirty tree in an otherwise sleeping event loop.
        let Some(frame) = state.surface.acquire(&state.gpu) else {
            state.retry_at = Some(Instant::now() + Duration::from_millis(16));
            return;
        };
        state.retry_at = None;
        if !state.shown {
            log::debug!("kova-native: building first scene");
        }
        let output = state.tree.frame(&mut FrameContext {
            text: &mut state.text,
            scene: &mut state.scene,
            atlas: state.renderer.atlas(),
            now: Instant::now(),
        });
        let view = frame.texture.create_view(&Default::default());
        if !state.shown {
            log::debug!("kova-native: submitting first scene");
        }
        state.renderer.render(
            &state.gpu,
            &state.scene,
            &view,
            state.surface.format(),
            size,
            if self.options.follow_theme {
                kova_native_widgets::theme().background
            } else {
                self.options.background
            },
        );
        state.window.pre_present_notify();
        if !state.shown {
            log::debug!("kova-native: presenting first scene");
        }
        state.gpu.queue.present(frame);
        state.animating = output.animating;
        state.window.set_cursor(output.cursor);
        state
            .window
            .set_ime_allowed(state.focused && output.ime_area.is_some());
        if let Some(area) = output.ime_area {
            state.window.set_ime_cursor_area(area);
        }
        if !state.shown {
            state.window.set_visible(true);
            state.shown = true;
        }
        let mut shared = self.shared.borrow_mut();
        shared.report.presented_frames += 1;
        shared.report.last_frame = output.stats;
        shared.report.last_scene = state.scene.stats();
    }

    fn apply_window_commands(&mut self, cx: &mut PlatformContext) {
        let commands = self
            .state
            .as_mut()
            .map(|s| s.tree.take_window_commands())
            .unwrap_or_default();
        for command in commands {
            if matches!(command, WindowCommand::Close | WindowCommand::Quit) {
                let accepted = self
                    .on_close_request
                    .as_mut()
                    .is_none_or(|callback| callback());
                if accepted {
                    if let Some(state) = &self.state {
                        let id = state.window.id();
                        self.state = None;
                        cx.destroy_window(id);
                        cx.exit();
                    }
                }
                continue;
            }
            let Some(state) = &self.state else { break };
            match command {
                WindowCommand::SetTitle(title) => state.window.set_title(&title),
                WindowCommand::Minimize => state.window.minimize(),
                WindowCommand::ToggleMaximize => {
                    state.window.set_maximized(!state.window.is_maximized())
                }
                WindowCommand::Close | WindowCommand::Quit => unreachable!(),
            }
        }
    }
}

impl PlatformHandler for Runner {
    fn init(&mut self, cx: &mut PlatformContext) {
        match self.init_window(cx) {
            Ok(state) => {
                self.state = Some(state);
                self.deadline = self.run_for.map(|d| Instant::now() + d);
                // Hidden Win32 windows do not receive WM_PAINT redraw requests.
                // Present the initial frame directly before revealing the window.
                self.draw();
                log::debug!("kova-native: event loop initialized");
            }
            Err(error) => {
                self.shared.borrow_mut().error = Some(error);
                cx.exit();
            }
        }
    }

    fn window_event(
        &mut self,
        cx: &mut PlatformContext,
        id: PlatformWindowId,
        event: PlatformEvent,
    ) {
        if let PlatformEvent::CloseRequested = &event
            && !self
                .on_close_request
                .as_mut()
                .is_none_or(|callback| callback())
        {
            return;
        }
        let Some(state) = &mut self.state else { return };
        if state.window.id() != id {
            return;
        }
        match event {
            PlatformEvent::RedrawRequested => self.draw(),
            PlatformEvent::CloseRequested => {
                self.state = None;
                cx.destroy_window(id);
                cx.exit();
            }
            PlatformEvent::Resized { width, height } => {
                state.surface.resize(&state.gpu, width, height);
                state
                    .tree
                    .set_viewport(state.window.logical_size(), state.window.scale_factor());
                state.redraw_pending = false;
            }
            PlatformEvent::ScaleFactorChanged(scale) => {
                let (w, h) = state.window.physical_size();
                state.surface.resize(&state.gpu, w, h);
                state.tree.set_viewport(state.window.logical_size(), scale);
                state.redraw_pending = false;
            }
            PlatformEvent::Occluded(value) => {
                state.occluded = value;
                state.redraw_pending = false;
                if !value {
                    state.window.request_redraw();
                }
            }
            PlatformEvent::Focused(false) => {
                state.focused = false;
                self.keymap.clear_pending();
                state.tree.cancel_pointer_input(&mut state.text);
                state.window.set_ime_allowed(false);
            }
            PlatformEvent::Focused(true) => {
                state.focused = true;
                state.window.request_redraw();
            }
            PlatformEvent::Input(event) => {
                let result = state.tree.dispatch(
                    &event,
                    &mut DispatchContext {
                        text: &mut state.text,
                        clipboard: &mut state.clipboard,
                        keymap: &mut self.keymap,
                        now: Instant::now(),
                    },
                );
                for action in result.unhandled_actions {
                    log::debug!("kova-native: unhandled action {}", action.name());
                }
            }
            PlatformEvent::FileDropped(path) => {
                if let Some(callback) = &mut self.on_file_drop {
                    callback(path);
                }
            }
        }
        self.apply_window_commands(cx);
    }

    fn about_to_wait(&mut self, cx: &mut PlatformContext) {
        let now = Instant::now();
        if let Some(deadline) = self.deadline {
            if now >= deadline {
                log::debug!("kova-native: bounded native run completed");
                cx.exit();
                return;
            }
            cx.wake_at(deadline);
        }
        self.apply_window_commands(cx);
        let Some(state) = &mut self.state else { return };
        let (w, h) = state.window.physical_size();
        if state.occluded || w == 0 || h == 0 || state.redraw_pending {
            return;
        }
        if let Some(retry) = state.retry_at
            && retry > now
        {
            cx.wake_at(retry);
            return;
        }
        // Timers registered by handlers since the last frame count too.
        let timed = match state.tree.next_deadline() {
            Some(at) if at > now => {
                cx.wake_at(at);
                false
            }
            Some(_) => true,
            None => false,
        };
        if state.tree.needs_frame() || state.animating || timed || state.retry_at.is_some() {
            if !state.shown {
                // Retry initial acquisition without relying on hidden-window
                // redraw events (the retry deadline above prevents spinning).
                self.draw();
                return;
            }
            state.window.request_redraw();
            state.redraw_pending = true;
        }
    }
}
