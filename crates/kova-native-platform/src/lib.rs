//! Windowing and OS integration for Kova Native.
//!
//! This crate is the only place that talks to the windowing system (winit).
//! It owns the event loop, creates windows and translates native events into
//! [`kova_native_input::InputEvent`]s in logical pixels. Everything above it is
//! platform independent, which is what makes Linux and macOS ports a matter
//! of backend work rather than framework changes.

mod convert;

use kova_native_core::{Bounds, KovaError, KovaResult, Point, Size};
use kova_native_input::{
    ClickTracker, CursorStyle, ImeEvent, InputEvent, KeyDownEvent, KeyUpEvent, Keystroke,
    Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ScrollDelta,
    ScrollWheelEvent,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle,
};

/// Opaque identifier of a platform window.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PlatformWindowId(winit::window::WindowId);

/// Options used to create a window.
#[derive(Clone, Debug)]
pub struct WindowAttributes {
    pub title: String,
    /// Inner size in logical pixels.
    pub size: Size,
    pub min_size: Option<Size>,
    pub resizable: bool,
    pub decorations: bool,
    pub maximized: bool,
    /// Created hidden; the app shows it after the first frame is presented
    /// to avoid a white flash.
    pub visible: bool,
}

impl Default for WindowAttributes {
    fn default() -> Self {
        WindowAttributes {
            title: "Kova Native".into(),
            size: Size::new(960.0, 640.0),
            min_size: None,
            resizable: true,
            decorations: true,
            maximized: false,
            visible: false,
        }
    }
}

/// A native window. Cheap to clone; implements the raw window handle traits
/// so the renderer can create a surface for it.
#[derive(Clone)]
pub struct PlatformWindow {
    inner: Arc<winit::window::Window>,
}

impl PlatformWindow {
    pub fn id(&self) -> PlatformWindowId {
        PlatformWindowId(self.inner.id())
    }

    /// Inner size in physical pixels.
    pub fn physical_size(&self) -> (u32, u32) {
        let s = self.inner.inner_size();
        (s.width, s.height)
    }

    pub fn scale_factor(&self) -> f32 {
        self.inner.scale_factor() as f32
    }

    /// Inner size in logical pixels.
    pub fn logical_size(&self) -> Size {
        let (w, h) = self.physical_size();
        let s = self.scale_factor();
        Size::new(w as f32 / s, h as f32 / s)
    }

    pub fn request_redraw(&self) {
        self.inner.request_redraw();
    }

    /// Must be called right before presenting a frame (lets the compositor
    /// schedule the next frame callback optimally on some platforms).
    pub fn pre_present_notify(&self) {
        self.inner.pre_present_notify();
    }

    pub fn set_title(&self, title: &str) {
        self.inner.set_title(title);
    }

    pub fn set_visible(&self, visible: bool) {
        self.inner.set_visible(visible);
    }

    pub fn set_cursor(&self, cursor: CursorStyle) {
        self.inner.set_cursor(convert::cursor(cursor));
    }

    pub fn set_ime_allowed(&self, allowed: bool) {
        self.inner.set_ime_allowed(allowed);
    }

    /// Tells the IME where the caret is (logical pixels) so candidate
    /// windows appear next to it.
    pub fn set_ime_cursor_area(&self, area: Bounds) {
        self.inner.set_ime_cursor_area(
            winit::dpi::LogicalPosition::new(area.origin.x as f64, area.origin.y as f64),
            winit::dpi::LogicalSize::new(area.size.width as f64, area.size.height as f64),
        );
    }

    pub fn focus(&self) {
        self.inner.focus_window();
    }

    pub fn minimize(&self) {
        self.inner.set_minimized(true);
    }

    pub fn set_maximized(&self, maximized: bool) {
        self.inner.set_maximized(maximized);
    }

    pub fn is_maximized(&self) -> bool {
        self.inner.is_maximized()
    }
}

impl HasWindowHandle for PlatformWindow {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        self.inner.window_handle()
    }
}

impl HasDisplayHandle for PlatformWindow {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        self.inner.display_handle()
    }
}

/// Window level events.
#[derive(Clone, Debug, PartialEq)]
pub enum PlatformEvent {
    /// The inner size changed (physical pixels).
    Resized {
        width: u32,
        height: u32,
    },
    ScaleFactorChanged(f32),
    RedrawRequested,
    CloseRequested,
    Focused(bool),
    Occluded(bool),
    Input(InputEvent),
}

/// Callbacks from the event loop. All run on the main thread.
pub trait PlatformHandler {
    /// The event loop is ready; create initial windows here.
    fn init(&mut self, cx: &mut PlatformContext);
    fn window_event(
        &mut self,
        cx: &mut PlatformContext,
        window: PlatformWindowId,
        event: PlatformEvent,
    );
    /// All pending events were processed; the loop is about to sleep.
    fn about_to_wait(&mut self, cx: &mut PlatformContext);
    /// A [`Waker`] was triggered from another thread.
    fn wake(&mut self, _cx: &mut PlatformContext) {}
}

/// Access to the running event loop.
pub struct PlatformContext<'a> {
    event_loop: &'a ActiveEventLoop,
    state: &'a mut LoopState,
}

impl PlatformContext<'_> {
    pub fn create_window(&mut self, attrs: &WindowAttributes) -> KovaResult<PlatformWindow> {
        let mut wa = winit::window::Window::default_attributes()
            .with_title(attrs.title.clone())
            .with_inner_size(winit::dpi::LogicalSize::new(
                attrs.size.width as f64,
                attrs.size.height as f64,
            ))
            .with_resizable(attrs.resizable)
            .with_decorations(attrs.decorations)
            .with_maximized(attrs.maximized)
            .with_visible(attrs.visible);
        if let Some(min) = attrs.min_size {
            wa = wa.with_min_inner_size(winit::dpi::LogicalSize::new(
                min.width as f64,
                min.height as f64,
            ));
        }
        let window = self
            .event_loop
            .create_window(wa)
            .map_err(|e| KovaError::Platform(e.to_string()))?;
        let window = PlatformWindow {
            inner: Arc::new(window),
        };
        self.state.windows.insert(
            window.inner.id(),
            WindowInputState::new(window.scale_factor() as f64),
        );
        Ok(window)
    }

    /// Forgets the input state of a closed window.
    pub fn destroy_window(&mut self, id: PlatformWindowId) {
        self.state.windows.remove(&id.0);
    }

    /// Stops the event loop; `run` returns afterwards.
    pub fn exit(&self) {
        self.event_loop.set_control_flow(ControlFlow::Poll);
        self.event_loop.exit();
    }

    /// Wake up at `deadline` even if no events arrive (timers, delayed animations).
    pub fn wake_at(&mut self, deadline: Instant) {
        self.state.wake_at = Some(self.state.wake_at.map_or(deadline, |d| d.min(deadline)));
    }

    pub fn waker(&self) -> Waker {
        Waker {
            proxy: self.state.proxy.clone(),
        }
    }
}

/// Wakes the event loop from any thread.
#[derive(Clone)]
pub struct Waker {
    proxy: EventLoopProxy<UserEvent>,
}

impl Waker {
    pub fn wake(&self) {
        let _ = self.proxy.send_event(UserEvent::Wake);
    }
}

#[derive(Debug, Clone, Copy)]
pub enum UserEvent {
    Wake,
}

/// Per-window input tracking (cursor position, buttons, modifiers, clicks).
struct WindowInputState {
    scale: f64,
    cursor: Point,
    modifiers: Modifiers,
    pressed: Vec<MouseButton>,
    clicks: ClickTracker,
}

impl WindowInputState {
    fn new(scale: f64) -> Self {
        WindowInputState {
            scale,
            cursor: Point::ZERO,
            modifiers: Modifiers::default(),
            pressed: Vec::new(),
            clicks: ClickTracker::default(),
        }
    }
}

struct LoopState {
    windows: HashMap<winit::window::WindowId, WindowInputState>,
    wake_at: Option<Instant>,
    proxy: EventLoopProxy<UserEvent>,
    initialized: bool,
}

struct Runner<H: PlatformHandler> {
    handler: H,
    state: LoopState,
}

impl<H: PlatformHandler> Runner<H> {
    fn cx<'a>(state: &'a mut LoopState, event_loop: &'a ActiveEventLoop) -> PlatformContext<'a> {
        PlatformContext { event_loop, state }
    }

    fn translate(
        &mut self,
        id: winit::window::WindowId,
        event: &WindowEvent,
    ) -> Option<PlatformEvent> {
        let input = self.state.windows.get_mut(&id)?;
        Some(match event {
            WindowEvent::Resized(size) => PlatformEvent::Resized {
                width: size.width,
                height: size.height,
            },
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                input.scale = *scale_factor;
                PlatformEvent::ScaleFactorChanged(*scale_factor as f32)
            }
            WindowEvent::RedrawRequested => PlatformEvent::RedrawRequested,
            WindowEvent::CloseRequested => PlatformEvent::CloseRequested,
            WindowEvent::Focused(focused) => {
                if !focused {
                    input.pressed.clear();
                }
                PlatformEvent::Focused(*focused)
            }
            WindowEvent::Occluded(occluded) => PlatformEvent::Occluded(*occluded),
            WindowEvent::ModifiersChanged(m) => {
                input.modifiers = convert::modifiers(m.state());
                PlatformEvent::Input(InputEvent::ModifiersChanged(input.modifiers))
            }
            WindowEvent::CursorMoved { position, .. } => {
                input.cursor = convert::logical(*position, input.scale);
                PlatformEvent::Input(InputEvent::MouseMove(MouseMoveEvent {
                    position: input.cursor,
                    pressed_button: input.pressed.first().copied(),
                    modifiers: input.modifiers,
                }))
            }
            WindowEvent::CursorLeft { .. } => PlatformEvent::Input(InputEvent::MouseExit),
            WindowEvent::MouseInput { state, button, .. } => {
                let button = convert::mouse_button(*button);
                match state {
                    ElementState::Pressed => {
                        if !input.pressed.contains(&button) {
                            input.pressed.push(button);
                        }
                        let click_count = input.clicks.press(button, input.cursor, Instant::now());
                        PlatformEvent::Input(InputEvent::MouseDown(MouseDownEvent {
                            button,
                            position: input.cursor,
                            modifiers: input.modifiers,
                            click_count,
                        }))
                    }
                    ElementState::Released => {
                        input.pressed.retain(|b| *b != button);
                        PlatformEvent::Input(InputEvent::MouseUp(MouseUpEvent {
                            button,
                            position: input.cursor,
                            modifiers: input.modifiers,
                            click_count: input.clicks.count().max(1),
                        }))
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let delta = match delta {
                    MouseScrollDelta::LineDelta(x, y) => ScrollDelta::Lines(Point::new(*x, *y)),
                    MouseScrollDelta::PixelDelta(p) => ScrollDelta::Pixels(Point::new(
                        (p.x / input.scale) as f32,
                        (p.y / input.scale) as f32,
                    )),
                };
                PlatformEvent::Input(InputEvent::ScrollWheel(ScrollWheelEvent {
                    position: input.cursor,
                    delta,
                    modifiers: input.modifiers,
                }))
            }
            WindowEvent::KeyboardInput { event, .. } => {
                #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
                let unmodified = {
                    use winit::platform::modifier_supplement::KeyEventExtModifierSupplement;
                    event.key_without_modifiers()
                };
                #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
                let unmodified = event.logical_key.clone();
                let key = match convert::key(&event.logical_key) {
                    kova_native_input::Key::Named(n) => kova_native_input::Key::Named(n),
                    _ => convert::key(&unmodified),
                };
                let keystroke = Keystroke {
                    modifiers: input.modifiers,
                    key,
                    key_char: event
                        .text
                        .as_ref()
                        .map(|t| t.to_string())
                        .filter(|t| !t.chars().any(char::is_control)),
                };
                match event.state {
                    ElementState::Pressed => {
                        PlatformEvent::Input(InputEvent::KeyDown(KeyDownEvent {
                            keystroke,
                            is_repeat: event.repeat,
                        }))
                    }
                    ElementState::Released => {
                        PlatformEvent::Input(InputEvent::KeyUp(KeyUpEvent { keystroke }))
                    }
                }
            }
            WindowEvent::Ime(ime) => PlatformEvent::Input(InputEvent::Ime(match ime {
                winit::event::Ime::Enabled => ImeEvent::Enabled,
                winit::event::Ime::Preedit(text, cursor) => ImeEvent::Preedit {
                    text: text.clone(),
                    cursor: *cursor,
                },
                winit::event::Ime::Commit(text) => ImeEvent::Commit(text.clone()),
                winit::event::Ime::Disabled => ImeEvent::Disabled,
            })),
            _ => return None,
        })
    }
}

impl<H: PlatformHandler> ApplicationHandler<UserEvent> for Runner<H> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.initialized {
            return;
        }
        self.state.initialized = true;
        let mut cx = Self::cx(&mut self.state, event_loop);
        self.handler.init(&mut cx);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        if let Some(event) = self.translate(id, &event) {
            let mut cx = Self::cx(&mut self.state, event_loop);
            self.handler
                .window_event(&mut cx, PlatformWindowId(id), event);
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Wake => {
                let mut cx = Self::cx(&mut self.state, event_loop);
                self.handler.wake(&mut cx);
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.state.wake_at = None;
        {
            let mut cx = Self::cx(&mut self.state, event_loop);
            self.handler.about_to_wait(&mut cx);
        }
        // On Win32, AboutToWait runs before winit enters its OS wait. A
        // handler may request exit here; overwriting Poll with Wait would
        // strand that exit until another unrelated native message arrives.
        event_loop.set_control_flow(if event_loop.exiting() {
            ControlFlow::Poll
        } else {
            match self.state.wake_at {
                Some(deadline) => ControlFlow::WaitUntil(deadline),
                None => ControlFlow::Wait,
            }
        });
    }
}

/// Runs the event loop until [`PlatformContext::exit`] is called.
pub fn run(handler: impl PlatformHandler) -> KovaResult<()> {
    let event_loop = EventLoop::<UserEvent>::with_user_event()
        .build()
        .map_err(|e| KovaError::Platform(format!("cannot create event loop: {e}")))?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let proxy = event_loop.create_proxy();
    let mut runner = Runner {
        handler,
        state: LoopState {
            windows: HashMap::new(),
            wake_at: None,
            proxy,
            initialized: false,
        },
    };
    let result = event_loop
        .run_app(&mut runner)
        .map_err(|e| KovaError::Platform(e.to_string()));
    log::debug!("kova-native-platform: event loop returned");
    result
}

/// System clipboard access.
pub struct Clipboard {
    inner: Option<arboard::Clipboard>,
}

impl Default for Clipboard {
    fn default() -> Self {
        Self::new()
    }
}

impl Clipboard {
    pub fn new() -> Self {
        Clipboard {
            inner: arboard::Clipboard::new().ok(),
        }
    }

    pub fn read_text(&mut self) -> Option<String> {
        self.inner.as_mut()?.get_text().ok()
    }

    pub fn write_text(&mut self, text: &str) {
        if let Some(c) = self.inner.as_mut() {
            let _ = c.set_text(text.to_owned());
        }
    }
}
