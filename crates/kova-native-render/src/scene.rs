//! The scene: a flat, ordered list of GPU-ready primitives for one frame.
//!
//! All coordinates are in *device pixels*. The element tree converts logical
//! coordinates (applying the window scale factor and pixel snapping) while
//! painting.

use crate::atlas::{AtlasKind, AtlasTile};
use crate::shaders::ShaderId;
use bytemuck::{Pod, Zeroable};
use kova_native_core::{Bounds, Color, Corners, Edges, Fill, Point, Transform2D};
use std::ops::Range;

pub(crate) const KIND_QUAD: u32 = 0;
pub(crate) const KIND_SHADOW: u32 = 1;
pub(crate) const KIND_INSET_SHADOW: u32 = 2;
pub(crate) const KIND_MONO_SPRITE: u32 = 3;
pub(crate) const KIND_POLY_SPRITE: u32 = 4;
pub(crate) const KIND_BACKDROP: u32 = 5;
pub(crate) const KIND_LAYER: u32 = 6;
pub(crate) const KIND_CUSTOM: u32 = 7;

const FLAG_GRADIENT: u32 = 1;
const FLAG_GRAYSCALE: u32 = 2;
const MASK_GRADIENT: u32 = 1;
const MASK_MONO: u32 = 2;
const MASK_COLOR: u32 = 4;

/// One GPU instance. Layout must match `InstanceIn` in `shader.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(crate) struct Instance {
    pub bounds: [f32; 4],
    pub radii: [f32; 4],
    pub border_widths: [f32; 4],
    pub uv: [f32; 4],
    pub clip_bounds: [f32; 4],
    pub clip_radii: [f32; 4],
    pub transform: [f32; 4],
    pub translate_params: [f32; 4],
    pub params: [f32; 4],
    pub color0: u32,
    pub color1: u32,
    pub border_color: u32,
    pub kind: u32,
}

/// Clip region (device pixels), optionally with rounded corners.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContentMask {
    pub bounds: Bounds,
    pub radii: Corners<f32>,
}

impl ContentMask {
    pub const NONE: ContentMask = ContentMask {
        bounds: Bounds::INFINITE,
        radii: Corners::ZERO,
    };

    pub fn new(bounds: Bounds) -> Self {
        ContentMask {
            bounds,
            radii: Corners::ZERO,
        }
    }

    pub fn rounded(bounds: Bounds, radii: Corners<f32>) -> Self {
        ContentMask {
            bounds,
            radii: radii.clamp_to(bounds.size),
        }
    }

    /// Intersection of two masks. Only one set of corner radii can be
    /// represented, so the radii of the smaller (inner) mask are kept.
    pub fn intersect(&self, other: &ContentMask) -> ContentMask {
        let bounds = self.bounds.intersect(&other.bounds);
        let self_area = self.bounds.size.width * self.bounds.size.height;
        let other_area = other.bounds.size.width * other.bounds.size.height;
        let radii = if other_area <= self_area {
            other.radii
        } else {
            self.radii
        };
        ContentMask { bounds, radii }
    }
}

/// Transform, clip and group opacity applied to a primitive.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DrawState {
    pub transform: Transform2D,
    pub clip: ContentMask,
    pub opacity: f32,
}

impl Default for DrawState {
    fn default() -> Self {
        DrawState {
            transform: Transform2D::IDENTITY,
            clip: ContentMask::NONE,
            opacity: 1.0,
        }
    }
}

/// A rounded rectangle with fill and border.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quad {
    pub bounds: Bounds,
    pub radii: Corners<f32>,
    pub fill: Fill,
    pub border_widths: Edges<f32>,
    pub border_color: Color,
}

impl Quad {
    pub fn filled(bounds: Bounds, fill: impl Into<Fill>) -> Self {
        Quad {
            bounds,
            radii: Corners::ZERO,
            fill: fill.into(),
            border_widths: Edges::ZERO,
            border_color: Color::TRANSPARENT,
        }
    }

    pub fn radii(mut self, radii: impl Into<Corners<f32>>) -> Self {
        self.radii = radii.into();
        self
    }

    pub fn border(mut self, widths: impl Into<Edges<f32>>, color: Color) -> Self {
        self.border_widths = widths.into();
        self.border_color = color;
        self
    }
}

/// A box shadow (CSS semantics: `blur` is the blur radius, `spread` grows
/// the shadow shape).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shadow {
    /// Bounds of the element casting (or containing, for inset) the shadow.
    pub bounds: Bounds,
    pub radii: Corners<f32>,
    pub color: Color,
    pub offset: Point,
    pub blur: f32,
    pub spread: f32,
    pub inset: bool,
}

/// Effects that split the scene into multiple render passes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Effect {
    /// Copy what has been drawn so far and blur it with this radius (device
    /// px); following backdrop primitives sample the result.
    CaptureBackdrop { blur: f32 },
}

/// How a layer opened with [`Scene::push_layer`] is composited onto what
/// lies below it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LayerMask {
    /// No mask: the layer is composited with group opacity only.
    None,
    /// Alpha ramps from `from` to `to` along `angle` (degrees, CSS
    /// convention: 0 = upwards, 90 = to the right) between the fractional
    /// positions `start` and `end` of the gradient line.
    LinearGradient {
        angle: f32,
        from: f32,
        to: f32,
        start: f32,
        end: f32,
    },
    /// An atlas tile stretched over the layer bounds. Mono tiles (glyphs,
    /// rasterized SVG masks) use their coverage, color tiles their alpha.
    Tile(AtlasTile),
}

/// One step of rendering a scene, in order.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Op {
    /// Draws instances into the current target with the built-in pipeline
    /// (`shader == None`) or a registered custom shader.
    Draw {
        instances: Range<u32>,
        shader: Option<ShaderId>,
    },
    /// Blurs the current target into the backdrop texture.
    Effect(Effect),
    /// Starts drawing into a new transparent layer.
    PushLayer,
    /// Ends the current layer and composites it onto its parent with the
    /// instance at `composite` (if it is visible).
    PopLayer { composite: Option<u32> },
}

/// Statistics about a built scene.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SceneStats {
    pub primitives: usize,
    pub culled: usize,
    pub draw_calls: usize,
}

/// An ordered list of primitives in painter's order.
#[derive(Default)]
pub struct Scene {
    pub(crate) instances: Vec<Instance>,
    ops: Vec<Op>,
    /// First instance of the run not yet recorded in `ops`.
    run_start: u32,
    /// Pipeline of the open run.
    run_shader: Option<ShaderId>,
    layer_depth: u32,
    culled: usize,
}

fn pack(c: Color) -> u32 {
    u32::from_le_bytes(c.to_rgba8())
}

impl Scene {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.instances.clear();
        self.ops.clear();
        self.run_start = 0;
        self.run_shader = None;
        self.layer_depth = 0;
        self.culled = 0;
    }

    /// Number of layers currently open (see [`Scene::push_layer`]).
    pub fn layer_depth(&self) -> u32 {
        self.layer_depth
    }

    pub fn len(&self) -> usize {
        self.instances.len()
    }

    pub fn is_empty(&self) -> bool {
        self.instances.is_empty()
    }

    pub fn stats(&self) -> SceneStats {
        let draw_calls = self
            .ops()
            .iter()
            .filter(|op| matches!(op, Op::Draw { .. } | Op::PopLayer { composite: Some(_) }))
            .count();
        SceneStats {
            primitives: self.instances.len(),
            culled: self.culled,
            draw_calls,
        }
    }

    /// The rendering steps, including the still open run of instances.
    pub(crate) fn ops(&self) -> Vec<Op> {
        let mut ops = self.ops.clone();
        let end = self.instances.len() as u32;
        if end > self.run_start {
            ops.push(Op::Draw {
                instances: self.run_start..end,
                shader: self.run_shader,
            });
        }
        ops
    }

    fn close_run(&mut self) {
        let end = self.instances.len() as u32;
        if end > self.run_start {
            self.ops.push(Op::Draw {
                instances: self.run_start..end,
                shader: self.run_shader,
            });
        }
        self.run_start = end;
    }

    /// Makes the next instance part of a run drawn with `shader`.
    fn begin(&mut self, shader: Option<ShaderId>) {
        if shader != self.run_shader {
            self.close_run();
            self.run_shader = shader;
        }
    }

    fn push(&mut self, inst: Instance) {
        self.begin(None);
        self.instances.push(inst);
    }

    /// Returns `false` (and counts the primitive as culled) if `local` bounds
    /// transformed by `state` fall entirely outside the clip.
    fn visible(&mut self, local: &Bounds, state: &DrawState) -> bool {
        if state.opacity <= 0.0 || state.clip.bounds.is_empty() {
            self.culled += 1;
            return false;
        }
        let world = state.transform.apply_bounds(local);
        if !world.intersects(&state.clip.bounds) {
            self.culled += 1;
            return false;
        }
        true
    }

    fn base(state: &DrawState, kind: u32) -> Instance {
        let t = &state.transform;
        let c = &state.clip;
        Instance {
            bounds: [0.0; 4],
            radii: [0.0; 4],
            border_widths: [0.0; 4],
            uv: [0.0; 4],
            clip_bounds: [
                c.bounds.origin.x,
                c.bounds.origin.y,
                c.bounds.size.width,
                c.bounds.size.height,
            ],
            clip_radii: [
                c.radii.top_left,
                c.radii.top_right,
                c.radii.bottom_right,
                c.radii.bottom_left,
            ],
            transform: [t.a, t.b, t.c, t.d],
            translate_params: [t.tx, t.ty, 0.0, 0.0],
            params: [0.0; 4],
            color0: 0,
            color1: 0,
            border_color: 0,
            kind,
        }
    }

    pub fn push_quad(&mut self, quad: &Quad, state: &DrawState) {
        if quad.bounds.is_empty() || !self.visible(&quad.bounds, state) {
            return;
        }
        let border_visible = !quad.border_widths.is_zero() && !quad.border_color.is_transparent();
        if quad.fill.is_transparent() && !border_visible {
            return;
        }
        let radii = quad.radii.clamp_to(quad.bounds.size);
        let mut inst = Self::base(state, KIND_QUAD);
        inst.bounds = rect(&quad.bounds);
        inst.radii = corners(&radii);
        if border_visible {
            let b = quad.border_widths;
            inst.border_widths = [b.top, b.right, b.bottom, b.left];
            inst.border_color = pack(quad.border_color.fade(state.opacity));
        }
        match quad.fill {
            Fill::Solid(c) => inst.color0 = pack(c.fade(state.opacity)),
            Fill::LinearGradient(g) => {
                inst.kind |= FLAG_GRADIENT << 8;
                inst.color0 = pack(g.from.color.fade(state.opacity));
                inst.color1 = pack(g.to.color.fade(state.opacity));
                inst.translate_params[2] = g.angle.to_radians();
                inst.translate_params[3] = g.from.position;
                inst.params[0] = g.to.position;
            }
        }
        self.push(inst);
    }

    pub fn push_shadow(&mut self, shadow: &Shadow, state: &DrawState) {
        if shadow.color.is_transparent() || shadow.bounds.is_empty() {
            return;
        }
        let sigma = (shadow.blur * 0.5).max(0.0);
        if shadow.inset {
            if !self.visible(&shadow.bounds, state) {
                return;
            }
            let radii = shadow.radii.clamp_to(shadow.bounds.size);
            let hole = shadow
                .bounds
                .translate(shadow.offset)
                .inflate(-shadow.spread);
            let hole_radii = radii.map(|r| (r - shadow.spread).max(0.0));
            let mut inst = Self::base(state, KIND_INSET_SHADOW);
            inst.bounds = rect(&shadow.bounds);
            inst.radii = corners(&radii);
            inst.uv = rect(&hole);
            inst.border_widths = corners(&hole_radii);
            inst.translate_params[2] = sigma;
            inst.color0 = pack(shadow.color.fade(state.opacity));
            self.push(inst);
        } else {
            let shape = shadow
                .bounds
                .translate(shadow.offset)
                .inflate(shadow.spread);
            if shape.is_empty() {
                return;
            }
            let radii = shadow
                .radii
                .clamp_to(shadow.bounds.size)
                .map(|r| (r + shadow.spread).max(0.0));
            if !self.visible(&shape.inflate(sigma * 3.0), state) {
                return;
            }
            let mut inst = Self::base(state, KIND_SHADOW);
            inst.bounds = rect(&shape);
            inst.radii = corners(&radii.clamp_to(shape.size));
            inst.translate_params[2] = sigma;
            inst.color0 = pack(shadow.color.fade(state.opacity));
            self.push(inst);
        }
    }

    /// Draws a coverage-mask sprite (glyph, tintable icon) in `color`.
    pub fn push_mono_sprite(
        &mut self,
        bounds: Bounds,
        tile: &AtlasTile,
        color: Color,
        state: &DrawState,
    ) {
        if color.is_transparent() || !self.visible(&bounds, state) {
            return;
        }
        let mut inst = Self::base(state, KIND_MONO_SPRITE | (tile.layer << 16));
        inst.bounds = rect(&bounds);
        inst.uv = tile.uv();
        inst.color0 = pack(color.fade(state.opacity));
        self.push(inst);
    }

    /// Draws a full color sprite (image, emoji) with optional rounded corners.
    pub fn push_poly_sprite(
        &mut self,
        bounds: Bounds,
        tile: &AtlasTile,
        radii: Corners<f32>,
        grayscale: bool,
        state: &DrawState,
    ) {
        if !self.visible(&bounds, state) {
            return;
        }
        let flags = if grayscale { FLAG_GRAYSCALE } else { 0 };
        let mut inst = Self::base(state, KIND_POLY_SPRITE | (flags << 8) | (tile.layer << 16));
        inst.bounds = rect(&bounds);
        inst.radii = corners(&radii.clamp_to(bounds.size));
        inst.uv = tile.uv();
        inst.color0 = pack(Color::WHITE.fade(state.opacity));
        self.push(inst);
    }

    /// Frosted glass: blurs everything drawn so far behind `bounds` and draws
    /// it clipped to the rounded rect, tinted with `tint`.
    pub fn push_backdrop_blur(
        &mut self,
        bounds: Bounds,
        radii: Corners<f32>,
        blur: f32,
        tint: Color,
        state: &DrawState,
    ) {
        if !self.visible(&bounds, state) {
            return;
        }
        let effect = Effect::CaptureBackdrop {
            blur: blur.max(0.0),
        };
        self.close_run();
        // Reuse the previous capture when nothing was drawn in between.
        if self.ops.last() != Some(&Op::Effect(effect)) {
            self.ops.push(Op::Effect(effect));
        }
        let mut inst = Self::base(state, KIND_BACKDROP);
        inst.bounds = rect(&bounds);
        inst.radii = corners(&radii.clamp_to(bounds.size));
        inst.color0 = pack(tint);
        inst.params[0] = state.opacity;
        self.push(inst);
    }

    /// Starts a layer: everything pushed until the matching
    /// [`Scene::pop_layer`] is drawn into a transparent offscreen target,
    /// then composited through a mask. Layers nest.
    pub fn push_layer(&mut self) {
        self.close_run();
        self.ops.push(Op::PushLayer);
        self.layer_depth += 1;
    }

    /// Ends the innermost layer and composites it inside `bounds` (device px,
    /// transformed and clipped by `state`) with `mask` and `state.opacity`.
    /// Content of the layer outside `bounds` is discarded, like CSS masks.
    pub fn pop_layer(&mut self, bounds: Bounds, mask: LayerMask, state: &DrawState) {
        if self.layer_depth == 0 {
            debug_assert!(false, "pop_layer without push_layer");
            return;
        }
        self.layer_depth -= 1;
        self.close_run();
        if bounds.is_empty() || !self.visible(&bounds, state) {
            self.ops.push(Op::PopLayer { composite: None });
            return;
        }
        let (flags, layer) = match mask {
            LayerMask::None => (0, 0),
            LayerMask::LinearGradient { .. } => (MASK_GRADIENT, 0),
            LayerMask::Tile(tile) => match tile.kind {
                AtlasKind::Mono => (MASK_MONO, tile.layer),
                AtlasKind::Color => (MASK_COLOR, tile.layer),
            },
        };
        let mut inst = Self::base(state, KIND_LAYER | (flags << 8) | (layer << 16));
        inst.bounds = rect(&bounds);
        let opacity = state.opacity.clamp(0.0, 1.0);
        inst.color0 = pack(Color::WHITE.with_alpha(opacity));
        match mask {
            LayerMask::None => {}
            LayerMask::LinearGradient {
                angle,
                from,
                to,
                start,
                end,
            } => {
                inst.color0 = pack(Color::WHITE.with_alpha(from.clamp(0.0, 1.0) * opacity));
                inst.color1 = pack(Color::WHITE.with_alpha(to.clamp(0.0, 1.0) * opacity));
                inst.translate_params[2] = angle.to_radians();
                inst.translate_params[3] = start;
                inst.params[0] = end;
            }
            LayerMask::Tile(tile) => inst.uv = tile.uv(),
        }
        let index = self.instances.len() as u32;
        self.instances.push(inst);
        self.run_start = self.instances.len() as u32;
        self.ops.push(Op::PopLayer {
            composite: Some(index),
        });
    }

    /// Draws `bounds` (rounded by `radii`) with a registered custom shader.
    /// `params` reach the shader as `input.params0`/`params1`, `time` as
    /// `input.time`, `color` as `input.color`; `scale` is the device scale
    /// (device px per logical px).
    #[allow(clippy::too_many_arguments)]
    pub fn push_custom(
        &mut self,
        shader: ShaderId,
        bounds: Bounds,
        radii: Corners<f32>,
        params: [f32; 8],
        time: f32,
        color: Color,
        scale: f32,
        state: &DrawState,
    ) {
        if bounds.is_empty() || !self.visible(&bounds, state) {
            return;
        }
        let mut inst = Self::base(state, KIND_CUSTOM);
        inst.bounds = rect(&bounds);
        inst.radii = corners(&radii.clamp_to(bounds.size));
        inst.uv = [params[0], params[1], params[2], params[3]];
        inst.border_widths = [params[4], params[5], params[6], params[7]];
        inst.translate_params[2] = time;
        inst.translate_params[3] = scale;
        inst.color0 = pack(Color::WHITE.with_alpha(state.opacity.clamp(0.0, 1.0)));
        inst.color1 = pack(color);
        self.begin(Some(shader));
        self.instances.push(inst);
    }
}

fn rect(b: &Bounds) -> [f32; 4] {
    [b.origin.x, b.origin.y, b.size.width, b.size.height]
}

fn corners(c: &Corners<f32>) -> [f32; 4] {
    [c.top_left, c.top_right, c.bottom_right, c.bottom_left]
}

#[cfg(test)]
mod tests {
    use super::*;
    use kova_native_core::{bounds, rgb};

    #[test]
    fn culls_offscreen_and_clipped() {
        let mut scene = Scene::new();
        let state = DrawState {
            clip: ContentMask::new(bounds(0.0, 0.0, 100.0, 100.0)),
            ..Default::default()
        };
        scene.push_quad(
            &Quad::filled(bounds(10.0, 10.0, 20.0, 20.0), rgb(0xff0000)),
            &state,
        );
        scene.push_quad(
            &Quad::filled(bounds(200.0, 10.0, 20.0, 20.0), rgb(0xff0000)),
            &state,
        );
        scene.push_quad(
            &Quad::filled(bounds(10.0, 10.0, 20.0, 20.0), Color::TRANSPARENT),
            &state,
        );
        assert_eq!(scene.len(), 1);
        assert_eq!(scene.stats().culled, 1);
        assert_eq!(scene.stats().draw_calls, 1);
    }

    #[test]
    fn mixed_primitives_share_one_segment() {
        let mut scene = Scene::new();
        let state = DrawState::default();
        let tile = AtlasTile {
            kind: crate::AtlasKind::Mono,
            layer: 0,
            x: 1,
            y: 1,
            width: 8,
            height: 8,
            origin: (0, 0),
        };
        for i in 0..1000 {
            let b = bounds(i as f32, 0.0, 10.0, 10.0);
            scene.push_quad(&Quad::filled(b, rgb(0x223344)).radii(4.0), &state);
            scene.push_mono_sprite(b, &tile, Color::WHITE, &state);
        }
        assert_eq!(scene.len(), 2000);
        assert_eq!(
            scene.stats().draw_calls,
            1,
            "thousands of primitives, one draw call"
        );
    }

    #[test]
    fn backdrop_splits_runs() {
        let mut scene = Scene::new();
        let state = DrawState::default();
        scene.push_quad(
            &Quad::filled(bounds(0.0, 0.0, 10.0, 10.0), rgb(0x112233)),
            &state,
        );
        scene.push_backdrop_blur(
            bounds(0.0, 0.0, 10.0, 10.0),
            Corners::all(4.0),
            12.0,
            Color::TRANSPARENT,
            &state,
        );
        scene.push_quad(
            &Quad::filled(bounds(0.0, 0.0, 5.0, 5.0), rgb(0x112233)),
            &state,
        );
        let ops = scene.ops();
        assert_eq!(
            ops,
            vec![
                Op::Draw {
                    instances: 0..1,
                    shader: None
                },
                Op::Effect(Effect::CaptureBackdrop { blur: 12.0 }),
                Op::Draw {
                    instances: 1..3,
                    shader: None
                },
            ]
        );
        assert_eq!(scene.stats().draw_calls, 2);
    }

    #[test]
    fn layers_and_custom_shaders_split_runs() {
        let mut scene = Scene::new();
        let state = DrawState::default();
        let quad = Quad::filled(bounds(0.0, 0.0, 10.0, 10.0), rgb(0x112233));
        let shader = ShaderId::from_raw(0);
        scene.push_quad(&quad, &state);
        scene.push_layer();
        scene.push_quad(&quad, &state);
        scene.push_quad(&quad, &state);
        scene.pop_layer(
            bounds(0.0, 0.0, 10.0, 10.0),
            LayerMask::LinearGradient {
                angle: 180.0,
                from: 1.0,
                to: 0.0,
                start: 0.0,
                end: 1.0,
            },
            &state,
        );
        scene.push_custom(
            shader,
            bounds(0.0, 0.0, 4.0, 4.0),
            Corners::ZERO,
            [0.0; 8],
            0.0,
            Color::WHITE,
            1.0,
            &state,
        );
        scene.push_custom(
            shader,
            bounds(4.0, 0.0, 4.0, 4.0),
            Corners::ZERO,
            [0.0; 8],
            0.0,
            Color::WHITE,
            1.0,
            &state,
        );
        scene.push_quad(&quad, &state);
        assert_eq!(scene.layer_depth(), 0);
        assert_eq!(
            scene.ops(),
            vec![
                Op::Draw {
                    instances: 0..1,
                    shader: None
                },
                Op::PushLayer,
                Op::Draw {
                    instances: 1..3,
                    shader: None
                },
                Op::PopLayer { composite: Some(3) },
                Op::Draw {
                    instances: 4..6,
                    shader: Some(shader)
                },
                Op::Draw {
                    instances: 6..7,
                    shader: None
                },
            ]
        );
    }

    #[test]
    fn instance_layout_is_stable() {
        assert_eq!(std::mem::size_of::<Instance>(), 160);
    }
}
