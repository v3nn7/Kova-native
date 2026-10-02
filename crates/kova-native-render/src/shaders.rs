//! Registration of custom WGSL fragment shaders.
//!
//! A custom shader is a WGSL function evaluated for every pixel of an
//! element-sized quad:
//!
//! ```wgsl
//! fn shade(input: ShaderInput) -> vec4<f32> {
//!     let wave = 0.5 + 0.5 * sin(input.uv.x * 12.0 + input.time * 3.0);
//!     return vec4<f32>(input.params0.rgb * wave, 1.0);
//! }
//! ```
//!
//! `ShaderInput` provides:
//!
//! | Field | Meaning |
//! | --- | --- |
//! | `position: vec2<f32>` | pixel position relative to the element, logical px |
//! | `size: vec2<f32>` | element size, logical px |
//! | `uv: vec2<f32>` | `position / size`, 0..1 |
//! | `time: f32` | seconds, as supplied by the element (animated shaders) |
//! | `scale: f32` | device pixels per logical pixel |
//! | `params0`, `params1: vec4<f32>` | eight user parameters |
//! | `color: vec4<f32>` | the element's inherited text color (straight alpha) |
//!
//! `shade` returns a straight-alpha sRGB color. Kova Native applies the
//! element's rounded corners, clipping, group opacity and premultiplication,
//! and batches consecutive primitives of the same shader into one draw call.
//!
//! The source is validated (parsed and type-checked with naga) when it is
//! registered, so mistakes surface as a [`KovaError`] with the compiler
//! message instead of a GPU error at draw time. The built-in shader's
//! declarations are in scope; avoid redefining names such as `rounded_rect_sdf`,
//! `premultiply` or `gradient_color` (you may call them).

use kova_native_core::{KovaError, KovaResult};
use std::sync::{Arc, Mutex};

/// Identifies a registered custom shader.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ShaderId(u32);

impl ShaderId {
    /// The raw registry index (stable for the process lifetime).
    pub fn index(self) -> u32 {
        self.0
    }

    #[cfg(test)]
    pub(crate) fn from_raw(index: u32) -> Self {
        ShaderId(index)
    }
}

struct Entry {
    label: String,
    source: Arc<str>,
}

static REGISTRY: Mutex<Vec<Entry>> = Mutex::new(Vec::new());

/// Code appended after the user's `shade` function.
const WRAPPER: &str = r#"
struct ShaderInput {
    position: vec2<f32>,
    size: vec2<f32>,
    uv: vec2<f32>,
    time: f32,
    scale: f32,
    params0: vec4<f32>,
    params1: vec4<f32>,
    color: vec4<f32>,
};

@fragment
fn fs_custom(in: VsOut) -> @location(0) vec4<f32> {
    let clip = clip_coverage(in.position.xy, in.clip_bounds, in.clip_radii);
    if clip <= 0.0 {
        discard;
    }
    let aa_scale = in.params.w;
    let shape = saturate(0.5 - rounded_rect_sdf(in.local_pos, in.bounds.xy, in.bounds.zw, in.radii) * aa_scale);
    let scale = max(in.params.y, 1e-4);
    let local = in.local_pos - in.bounds.xy;
    var input: ShaderInput;
    input.position = local / scale;
    input.size = in.bounds.zw / scale;
    input.uv = local / max(in.bounds.zw, vec2<f32>(1e-4));
    input.time = in.params.x;
    input.scale = scale;
    input.params0 = in.uv_rect;
    input.params1 = in.border_widths;
    input.color = in.color1;
    let c = saturate(shade(input));
    return finish(premultiply(c) * shape * clip * in.color0.a);
}
"#;

/// Builds the complete module for a user `shade` function.
fn assemble(user: &str) -> String {
    format!(
        "{}\n// ---- user shader ----\n{}\n// ---- Kova Native wrapper ----\n{}",
        crate::renderer::SHADER,
        user,
        WRAPPER
    )
}

fn validate(label: &str, source: &str) -> KovaResult<()> {
    use wgpu::naga;
    let module = naga::front::wgsl::parse_str(source).map_err(|e| {
        KovaError::Other(format!(
            "custom shader '{label}': {}",
            e.emit_to_string(source)
        ))
    })?;
    let has_shade = module
        .functions
        .iter()
        .any(|(_, f)| f.name.as_deref() == Some("shade"));
    if !has_shade {
        return Err(KovaError::Other(format!(
            "custom shader '{label}' must define `fn shade(input: ShaderInput) -> vec4<f32>`"
        )));
    }
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::empty(),
    )
    .validate(&module)
    .map_err(|e| {
        KovaError::Other(format!(
            "custom shader '{label}': {}",
            e.emit_to_string(source)
        ))
    })?;
    Ok(())
}

/// Validates and registers a custom shader. `wgsl` must define
/// `fn shade(input: ShaderInput) -> vec4<f32>` (see the module docs).
/// Registering identical source again returns the existing id.
pub fn register_shader(label: &str, wgsl: &str) -> KovaResult<ShaderId> {
    let source = assemble(wgsl);
    {
        let registry = REGISTRY.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(i) = registry.iter().position(|e| *e.source == *source) {
            return Ok(ShaderId(i as u32));
        }
    }
    validate(label, &source)?;
    let mut registry = REGISTRY.lock().unwrap_or_else(|e| e.into_inner());
    registry.push(Entry {
        label: label.to_string(),
        source: source.into(),
    });
    Ok(ShaderId(registry.len() as u32 - 1))
}

/// Label and full module source of a registered shader.
pub(crate) fn shader_module_source(id: ShaderId) -> Option<(String, Arc<str>)> {
    let registry = REGISTRY.lock().unwrap_or_else(|e| e.into_inner());
    registry
        .get(id.0 as usize)
        .map(|e| (e.label.clone(), e.source.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const STRIPES: &str = r#"
fn shade(input: ShaderInput) -> vec4<f32> {
    let band = step(0.5, fract(input.position.x / 8.0));
    return mix(input.params0, input.params1, band);
}
"#;

    #[test]
    fn registers_valid_shaders_once() {
        let a = register_shader("stripes", STRIPES).expect("valid shader");
        let b = register_shader("stripes again", STRIPES).expect("valid shader");
        assert_eq!(a, b);
        assert!(shader_module_source(a).unwrap().1.contains("fs_custom"));
    }

    #[test]
    fn reports_compile_errors_and_missing_entry() {
        let err = register_shader(
            "broken",
            "fn shade(input: ShaderInput) -> vec4<f32> { return 1.0; }",
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("broken"), "{err}");
        let err = register_shader("empty", "fn other() {}")
            .unwrap_err()
            .to_string();
        assert!(err.contains("shade"), "{err}");
    }
}
