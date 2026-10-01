//! Contexts passed to event handlers, painters and measurers.

use crate::tree::NodeId;
use kova_native_assets::{ImageData, SvgData};
use kova_native_core::{
    Bounds, Color, Corners, Edges, ElementId, Fill, Instant, Point, Size, Transform2D,
};
use kova_native_input::{Action, DispatchPhase};
use kova_native_render::{
    Atlas, AtlasImage, AtlasKey, AtlasKind, ContentMask, DrawState, Quad, Scene, Shadow,
};
use kova_native_text::{TextLayout, TextStyle, TextSystem};
use std::borrow::Cow;

/// Access to the system clipboard from event handlers.
pub trait Clipboard {
    fn read_text(&mut self) -> Option<String>;
    fn write_text(&mut self, text: &str);
}

/// An in-memory clipboard (tests, headless use).
#[derive(Default)]
pub struct MemoryClipboard(pub Option<String>);

impl Clipboard for MemoryClipboard {
    fn read_text(&mut self) -> Option<String> {
        self.0.clone()
    }

    fn write_text(&mut self, text: &str) {
        self.0 = Some(text.to_string());
    }
}

/// Requests from UI code to the window / application.
#[derive(Clone, Debug, PartialEq)]
pub enum WindowCommand {
    SetTitle(String),
    Minimize,
    ToggleMaximize,
    Close,
    /// Exit the application.
    Quit,
}

pub(crate) enum Command {
    Focus(NodeId),
    FocusId(ElementId),
    Blur,
    FocusNext,
    FocusPrev,
    Repaint(NodeId),
    Relayout(NodeId),
    DispatchAction(Box<dyn Action>),
    Window(WindowCommand),
}

/// Context for event handlers.
pub struct EventCx<'a> {
    pub(crate) node: NodeId,
    pub(crate) bounds: Bounds,
    pub(crate) transform: Transform2D,
    pub(crate) phase: DispatchPhase,
    pub(crate) focused: bool,
    pub(crate) propagation_stopped: bool,
    pub(crate) default_prevented: bool,
    pub(crate) commands: &'a mut Vec<Command>,
    pub(crate) clipboard: &'a mut dyn Clipboard,
    pub(crate) text: &'a mut TextSystem,
    pub(crate) now: Instant,
}

impl EventCx<'_> {
    /// Stops the event from reaching further nodes.
    pub fn stop_propagation(&mut self) {
        self.propagation_stopped = true;
    }

    /// Suppresses Kova Native's default behavior for this event (focus on click,
    /// Tab navigation, text input...).
    pub fn prevent_default(&mut self) {
        self.default_prevented = true;
    }

    /// The current element's layout bounds in logical window coordinates,
    /// before visual transforms. Use `to_local` to map pointer positions.
    pub fn bounds(&self) -> Bounds {
        self.bounds
    }

    /// Converts a window position to element-local coordinates, undoing the
    /// visual transforms of this element and its ancestors.
    pub fn to_local(&self, position: Point) -> Point {
        self.transform
            .inverse()
            .map_or(position, |inv| inv.apply(position))
            - self.bounds.origin
    }

    pub fn phase(&self) -> DispatchPhase {
        self.phase
    }

    /// Whether the current element has keyboard focus.
    pub fn is_focused(&self) -> bool {
        self.focused
    }

    /// Focuses the current element.
    pub fn focus_self(&mut self) {
        self.commands.push(Command::Focus(self.node));
    }

    /// Focuses the element with the given id.
    pub fn focus_id(&mut self, id: impl Into<ElementId>) {
        self.commands.push(Command::FocusId(id.into()));
    }

    pub fn blur(&mut self) {
        self.commands.push(Command::Blur);
    }

    pub fn focus_next(&mut self) {
        self.commands.push(Command::FocusNext);
    }

    pub fn focus_prev(&mut self) {
        self.commands.push(Command::FocusPrev);
    }

    /// Dispatches an action along the focus path after this event.
    pub fn dispatch_action(&mut self, action: impl Action) {
        self.commands
            .push(Command::DispatchAction(Box::new(action)));
    }

    /// Requests a repaint of the current element (for state kept outside
    /// signals, e.g. inside custom elements).
    pub fn repaint(&mut self) {
        self.commands.push(Command::Repaint(self.node));
    }

    /// Requests re-measuring/re-layout of the current element.
    pub fn relayout(&mut self) {
        self.commands.push(Command::Relayout(self.node));
    }

    pub fn read_clipboard(&mut self) -> Option<String> {
        self.clipboard.read_text()
    }

    pub fn write_clipboard(&mut self, text: &str) {
        self.clipboard.write_text(text);
    }

    pub fn text_system(&mut self) -> &mut TextSystem {
        self.text
    }

    pub fn now(&self) -> Instant {
        self.now
    }

    pub fn set_window_title(&mut self, title: impl Into<String>) {
        self.commands
            .push(Command::Window(WindowCommand::SetTitle(title.into())));
    }

    pub fn minimize_window(&mut self) {
        self.commands.push(Command::Window(WindowCommand::Minimize));
    }

    pub fn toggle_maximize_window(&mut self) {
        self.commands
            .push(Command::Window(WindowCommand::ToggleMaximize));
    }

    pub fn close_window(&mut self) {
        self.commands.push(Command::Window(WindowCommand::Close));
    }

    /// Exits the application.
    pub fn quit(&mut self) {
        self.commands.push(Command::Window(WindowCommand::Quit));
    }
}

/// Context for measuring leaf content.
pub struct MeasureCx<'a> {
    pub text: &'a mut TextSystem,
    pub scale: f32,
}

/// Context for painting. All public methods take *logical* coordinates in
/// window space; conversion to device pixels, pixel snapping, transforms,
/// clipping and group opacity are applied automatically.
pub struct PaintCx<'a> {
    pub(crate) scene: &'a mut Scene,
    pub(crate) text: &'a mut TextSystem,
    pub(crate) atlas: &'a mut Atlas,
    pub(crate) scale: f32,
    /// Accumulated transform in logical coordinates.
    pub(crate) transform: Transform2D,
    /// Clip in device pixels.
    pub(crate) clip: ContentMask,
    pub(crate) opacity: f32,
    pub(crate) bounds: Bounds,
    pub(crate) content_bounds: Bounds,
    pub(crate) text_style: TextStyle,
    pub(crate) text_color: Color,
    pub(crate) now: Instant,
    pub(crate) animating: &'a mut bool,
    pub(crate) focused: bool,
    pub(crate) hovered: bool,
}

impl PaintCx<'_> {
    /// The element's border box in window coordinates.
    pub fn bounds(&self) -> Bounds {
        self.bounds
    }

    /// The element's content box (inside border and padding).
    pub fn content_bounds(&self) -> Bounds {
        self.content_bounds
    }

    pub fn scale_factor(&self) -> f32 {
        self.scale
    }

    pub fn now(&self) -> Instant {
        self.now
    }

    /// The inherited text style.
    pub fn text_style(&self) -> &TextStyle {
        &self.text_style
    }

    /// The inherited (possibly animating) text color.
    pub fn text_color(&self) -> Color {
        self.text_color
    }

    pub fn is_focused(&self) -> bool {
        self.focused
    }

    pub fn is_hovered(&self) -> bool {
        self.hovered
    }

    pub fn text_system(&mut self) -> &mut TextSystem {
        self.text
    }

    /// Keeps frames coming (call every frame while animating).
    pub fn request_animation_frame(&mut self) {
        *self.animating = true;
    }

    pub(crate) fn draw_state(&self) -> DrawState {
        let t = self.transform;
        DrawState {
            transform: Transform2D {
                tx: t.tx * self.scale,
                ty: t.ty * self.scale,
                ..t
            },
            clip: self.clip,
            opacity: self.opacity,
        }
    }

    /// Converts logical bounds to device pixels, snapping edges to the pixel grid.
    pub fn snap(&self, b: Bounds) -> Bounds {
        let s = self.scale;
        let x0 = (b.left() * s).round();
        let y0 = (b.top() * s).round();
        let x1 = (b.right() * s).round();
        let y1 = (b.bottom() * s).round();
        Bounds::new(Point::new(x0, y0), Size::new(x1 - x0, y1 - y0))
    }

    fn snap_width(&self, w: f32) -> f32 {
        if w <= 0.0 {
            0.0
        } else {
            (w * self.scale).round().max(1.0)
        }
    }

    /// Paints a rounded rectangle with optional border.
    pub fn paint_quad(
        &mut self,
        bounds: Bounds,
        radii: Corners<f32>,
        fill: impl Into<Fill>,
        border_widths: Edges<f32>,
        border_color: Color,
    ) {
        let quad = Quad {
            bounds: self.snap(bounds),
            radii: radii.scale(self.scale),
            fill: fill.into(),
            border_widths: border_widths.map(|w| self.snap_width(w)),
            border_color,
        };
        let state = self.draw_state();
        self.scene.push_quad(&quad, &state);
    }

    /// Fills a rectangle.
    pub fn fill_rect(&mut self, bounds: Bounds, fill: impl Into<Fill>) {
        self.paint_quad(bounds, Corners::ZERO, fill, Edges::ZERO, Color::TRANSPARENT);
    }

    /// Fills a rounded rectangle.
    pub fn fill_rounded(&mut self, bounds: Bounds, radius: f32, fill: impl Into<Fill>) {
        self.paint_quad(
            bounds,
            Corners::all(radius),
            fill,
            Edges::ZERO,
            Color::TRANSPARENT,
        );
    }

    pub fn paint_shadow(
        &mut self,
        bounds: Bounds,
        radii: Corners<f32>,
        shadow: &crate::style::BoxShadow,
    ) {
        let s = self.scale;
        let shadow = Shadow {
            bounds: self.snap(bounds),
            radii: radii.scale(s),
            color: shadow.color,
            offset: shadow.offset.scale(s),
            blur: shadow.blur * s,
            spread: shadow.spread * s,
            inset: shadow.inset,
        };
        let state = self.draw_state();
        self.scene.push_shadow(&shadow, &state);
    }

    /// Paints laid out text with its top-left corner at `origin`.
    pub fn paint_text(&mut self, layout: &TextLayout, origin: Point, color: Color) {
        let state = self.draw_state();
        let scene = &mut *self.scene;
        let atlas = &mut *self.atlas;
        let text = &mut *self.text;
        let scale = self.scale;
        // Snap the text origin to whole device pixels for crisp baselines.
        let origin = Point::new(
            (origin.x * scale).round() / scale,
            (origin.y * scale).round() / scale,
        );
        layout.for_each_glyph(origin, scale, |glyph| {
            let tile = atlas.get_or_insert_with(AtlasKey::Glyph(glyph.key), || {
                text.rasterize(glyph.key).map(|r| AtlasImage {
                    kind: if r.is_color {
                        AtlasKind::Color
                    } else {
                        AtlasKind::Mono
                    },
                    width: r.width,
                    height: r.height,
                    data: Cow::Owned(r.data),
                    origin: (r.left, -r.top),
                })
            });
            let Some(tile) = tile else { return };
            let b = Bounds::new(
                Point::new(
                    (glyph.x + tile.origin.0) as f32,
                    (glyph.y + tile.origin.1) as f32,
                ),
                Size::new(tile.width as f32, tile.height as f32),
            );
            match tile.kind {
                AtlasKind::Mono => {
                    scene.push_mono_sprite(b, &tile, glyph.color.unwrap_or(color), &state)
                }
                AtlasKind::Color => scene.push_poly_sprite(b, &tile, Corners::ZERO, false, &state),
            }
        });
    }

    /// Paints a raster image stretched to `bounds`.
    pub fn paint_image(
        &mut self,
        image: &ImageData,
        bounds: Bounds,
        radii: Corners<f32>,
        grayscale: bool,
    ) {
        let tile = self
            .atlas
            .get_or_insert_with(AtlasKey::Image(image.id()), || {
                Some(AtlasImage {
                    kind: AtlasKind::Color,
                    width: image.width(),
                    height: image.height(),
                    data: Cow::Borrowed(image.pixels()),
                    origin: (0, 0),
                })
            });
        if let Some(tile) = tile {
            let b = self.snap(bounds);
            let state = self.draw_state();
            self.scene
                .push_poly_sprite(b, &tile, radii.scale(self.scale), grayscale, &state);
        }
    }

    /// Paints an SVG rasterized at the exact device size of `bounds`. With
    /// `tint`, the SVG is used as a coverage mask filled with that color.
    pub fn paint_svg(&mut self, svg: &SvgData, bounds: Bounds, tint: Option<Color>) {
        let b = self.snap(bounds);
        let (w, h) = (b.size.width.max(1.0) as u32, b.size.height.max(1.0) as u32);
        let mono = tint.is_some();
        let tile = self.atlas.get_or_insert_with(
            AtlasKey::Vector {
                id: svg.id(),
                width: w,
                height: h,
                mono,
            },
            || {
                let data = if mono {
                    svg.rasterize_mask(w, h)?
                } else {
                    svg.rasterize(w, h)?
                };
                Some(AtlasImage {
                    kind: if mono {
                        AtlasKind::Mono
                    } else {
                        AtlasKind::Color
                    },
                    width: w,
                    height: h,
                    data: Cow::Owned(data),
                    origin: (0, 0),
                })
            },
        );
        if let Some(tile) = tile {
            let state = self.draw_state();
            match tint {
                Some(color) => self.scene.push_mono_sprite(b, &tile, color, &state),
                None => self
                    .scene
                    .push_poly_sprite(b, &tile, Corners::ZERO, false, &state),
            }
        }
    }

    /// Frosted glass behind `bounds`.
    pub fn paint_backdrop_blur(
        &mut self,
        bounds: Bounds,
        radii: Corners<f32>,
        blur: f32,
        tint: Color,
    ) {
        let b = self.snap(bounds);
        let state = self.draw_state();
        self.scene
            .push_backdrop_blur(b, radii.scale(self.scale), blur * self.scale, tint, &state);
    }

    /// Runs `f` with an additional (rounded) clip in logical coordinates.
    /// Only axis aligned clips are exact under rotation.
    pub fn with_clip<R>(
        &mut self,
        bounds: Bounds,
        radii: Corners<f32>,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let device = self.transform_bounds_to_device(bounds);
        let mask = ContentMask::rounded(
            device,
            radii.scale(self.scale * self.transform.approx_scale()),
        );
        let previous = self.clip;
        self.clip = previous.intersect(&mask);
        let r = f(self);
        self.clip = previous;
        r
    }

    pub(crate) fn transform_bounds_to_device(&self, bounds: Bounds) -> Bounds {
        let world = self.transform.apply_bounds(&bounds);
        let s = self.scale;
        let x0 = (world.left() * s).round();
        let y0 = (world.top() * s).round();
        let x1 = (world.right() * s).round();
        let y1 = (world.bottom() * s).round();
        Bounds::new(Point::new(x0, y0), Size::new(x1 - x0, y1 - y0))
    }
}
