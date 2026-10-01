//! The scene: a flat, ordered list of GPU-ready primitives for one frame.
//!
//! All coordinates are in *device pixels*. The element tree converts logical
//! coordinates (applying the window scale factor and pixel snapping) while
//! painting.

use crate::atlas::AtlasTile;
use bytemuck::{Pod, Zeroable};
use kova_core::{Bounds, Color, Corners, Edges, Fill, Point, Transform2D};
use std::ops::Range;

pub(crate) const KIND_QUAD: u32 = 0;
pub(crate) const KIND_SHADOW: u32 = 1;
pub(crate) const KIND_INSET_SHADOW: u32 = 2;
pub(crate) const KIND_MONO_SPRITE: u32 = 3;
pub(crate) const KIND_POLY_SPRITE: u32 = 4;
pub(crate) const KIND_BACKDROP: u32 = 5;

const FLAG_GRADIENT: u32 = 1;
const FLAG_GRAYSCALE: u32 = 2;

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

/// A contiguous run of instances drawn with one draw call, followed by an
/// optional effect.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Segment {
    pub instances: Range<u32>,
    pub effect: Option<Effect>,
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
    effects: Vec<(u32, Effect)>,
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
        self.effects.clear();
        self.culled = 0;
    }

    pub fn len(&self) -> usize {
        self.instances.len()
    }

    pub fn is_empty(&self) -> bool {
        self.instances.is_empty()
    }

    pub fn stats(&self) -> SceneStats {
        SceneStats {
            primitives: self.instances.len(),
            culled: self.culled,
            draw_calls: self.segments().len(),
        }
    }

    pub(crate) fn segments(&self) -> Vec<Segment> {
        let mut segments = Vec::with_capacity(self.effects.len() + 1);
        let mut start = 0u32;
        for (at, effect) in &self.effects {
            segments.push(Segment {
                instances: start..*at,
                effect: Some(*effect),
            });
            start = *at;
        }
        segments.push(Segment {
            instances: start..self.instances.len() as u32,
            effect: None,
        });
        segments.retain(|s| !s.instances.is_empty() || s.effect.is_some());
        segments
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
        self.instances.push(inst);
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
            self.instances.push(inst);
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
            self.instances.push(inst);
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
        self.instances.push(inst);
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
        self.instances.push(inst);
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
        let at = self.instances.len() as u32;
        let effect = Effect::CaptureBackdrop {
            blur: blur.max(0.0),
        };
        // Reuse the previous capture when nothing was drawn in between.
        if self.effects.last() != Some(&(at, effect)) {
            self.effects.push((at, effect));
        }
        let mut inst = Self::base(state, KIND_BACKDROP);
        inst.bounds = rect(&bounds);
        inst.radii = corners(&radii.clamp_to(bounds.size));
        inst.color0 = pack(tint);
        inst.params[0] = state.opacity;
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
    use kova_core::{bounds, rgb};

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
    fn backdrop_splits_segments() {
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
        let segments = scene.segments();
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].instances, 0..1);
        assert_eq!(segments[1].instances, 1..3);
    }

    #[test]
    fn instance_layout_is_stable() {
        assert_eq!(std::mem::size_of::<Instance>(), 160);
    }
}
