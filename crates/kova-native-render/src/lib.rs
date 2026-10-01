//! GPU renderer for Kova Native.
//!
//! The renderer is built around a single "uber" pipeline: every primitive —
//! rounded rectangles with independent corner radii, borders, linear
//! gradients, drop shadows, inset shadows, glyphs, images, frosted-glass
//! backdrops — is an instance of a unit quad evaluated analytically in the
//! fragment shader. Primitives therefore batch regardless of type, and a
//! typical frame with thousands of elements is **one draw call**. Effects
//! that need the pixels drawn so far (backdrop blur) split the frame into
//! segments.
//!
//! Glyphs and images live in growable texture-array atlases, so atlas growth
//! never breaks batches either.

mod atlas;
mod renderer;
mod scene;

pub use atlas::{ATLAS_SIZE, Atlas, AtlasImage, AtlasKey, AtlasKind, AtlasTile};
pub use renderer::{GpuContext, Renderer, SurfaceOptions, WindowSurface};
pub use scene::{ContentMask, DrawState, Effect, Quad, Scene, SceneStats, Shadow};
pub use wgpu;

#[cfg(test)]
mod tests;
