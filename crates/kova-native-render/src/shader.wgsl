// Kova Native uber-shader.
//
// Every UI primitive is an instance of a unit quad. The vertex shader places
// it (with optional affine transform) and the fragment shader evaluates the
// primitive analytically: signed distance fields for rounded rectangles and
// borders, a closed-form gaussian for shadows, atlas lookups for glyphs and
// images. Because all kinds share one pipeline, consecutive primitives batch
// into a single draw call regardless of their type.
//
// Colors arrive as straight-alpha sRGB and are blended premultiplied in sRGB
// space, matching browsers and design tools.

const KIND_QUAD: u32 = 0u;
const KIND_SHADOW: u32 = 1u;
const KIND_INSET_SHADOW: u32 = 2u;
const KIND_MONO_SPRITE: u32 = 3u;
const KIND_POLY_SPRITE: u32 = 4u;
const KIND_BACKDROP: u32 = 5u;

const FLAG_GRADIENT: u32 = 1u;
const FLAG_GRAYSCALE: u32 = 2u;

struct Globals {
    viewport: vec2<f32>,
    // 1.0 when the target is an sRGB format and we must output linear values.
    linear_output: f32,
    _pad: f32,
};

@group(0) @binding(0) var<uniform> globals: Globals;
@group(0) @binding(1) var mono_atlas: texture_2d_array<f32>;
@group(0) @binding(2) var color_atlas: texture_2d_array<f32>;
@group(0) @binding(3) var atlas_sampler: sampler;
@group(0) @binding(4) var backdrop_texture: texture_2d<f32>;

struct InstanceIn {
    @location(0) bounds: vec4<f32>,
    @location(1) radii: vec4<f32>,
    @location(2) border_widths: vec4<f32>,
    @location(3) uv: vec4<f32>,
    @location(4) clip_bounds: vec4<f32>,
    @location(5) clip_radii: vec4<f32>,
    @location(6) transform: vec4<f32>,
    @location(7) translate_params: vec4<f32>,
    @location(8) params: vec4<f32>,
    @location(9) color0: vec4<f32>,
    @location(10) color1: vec4<f32>,
    @location(11) border_color: vec4<f32>,
    @location(12) kind: u32,
};

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) local_pos: vec2<f32>,
    @location(1) tex_coord: vec2<f32>,
    @location(2) @interpolate(flat) bounds: vec4<f32>,
    @location(3) @interpolate(flat) radii: vec4<f32>,
    @location(4) @interpolate(flat) border_widths: vec4<f32>,
    @location(5) @interpolate(flat) uv_rect: vec4<f32>,
    @location(6) @interpolate(flat) clip_bounds: vec4<f32>,
    @location(7) @interpolate(flat) clip_radii: vec4<f32>,
    // x, y, z: kind specific parameters, w: screen pixels per local pixel.
    @location(8) @interpolate(flat) params: vec4<f32>,
    @location(9) @interpolate(flat) color0: vec4<f32>,
    @location(10) @interpolate(flat) color1: vec4<f32>,
    @location(11) @interpolate(flat) border_color: vec4<f32>,
    @location(12) @interpolate(flat) kind: u32,
};

@vertex
fn vs_main(@builtin(vertex_index) vid: u32, inst: InstanceIn) -> VsOut {
    let corner = vec2<f32>(f32(vid & 1u), f32(vid >> 1u));
    let kind = inst.kind & 0xffu;

    let a = inst.transform.x;
    let b = inst.transform.y;
    let c = inst.transform.z;
    let d = inst.transform.w;
    let aa_scale = max(sqrt(abs(a * d - b * c)), 1e-4);

    // Expand geometry so antialiased edges and blur tails are not cut off.
    var pad = 1.0 / aa_scale;
    if kind == KIND_SHADOW {
        pad = 3.0 * inst.translate_params.z + 1.0 / aa_scale;
    } else if kind == KIND_MONO_SPRITE || kind == KIND_POLY_SPRITE {
        pad = 0.0;
    }
    let origin = inst.bounds.xy - vec2<f32>(pad);
    let size = inst.bounds.zw + vec2<f32>(2.0 * pad);
    let local = origin + corner * size;

    let world = vec2<f32>(
        a * local.x + b * local.y + inst.translate_params.x,
        c * local.x + d * local.y + inst.translate_params.y,
    );
    let ndc = vec2<f32>(world.x / globals.viewport.x * 2.0 - 1.0, 1.0 - world.y / globals.viewport.y * 2.0);

    var out: VsOut;
    out.position = vec4<f32>(ndc, 0.0, 1.0);
    out.local_pos = local;
    out.tex_coord = mix(inst.uv.xy, inst.uv.zw, corner);
    out.bounds = inst.bounds;
    out.radii = inst.radii;
    out.border_widths = inst.border_widths;
    out.uv_rect = inst.uv;
    out.clip_bounds = inst.clip_bounds;
    out.clip_radii = inst.clip_radii;
    out.params = vec4<f32>(inst.translate_params.z, inst.translate_params.w, inst.params.x, aa_scale);
    out.color0 = inst.color0;
    out.color1 = inst.color1;
    out.border_color = inst.border_color;
    out.kind = inst.kind;
    return out;
}

// --- Signed distance helpers ---------------------------------------------

fn pick_radius(p: vec2<f32>, radii: vec4<f32>) -> f32 {
    // radii = (top_left, top_right, bottom_right, bottom_left)
    let top = select(radii.y, radii.x, p.x < 0.0);
    let bottom = select(radii.z, radii.w, p.x < 0.0);
    return select(bottom, top, p.y < 0.0);
}

fn rounded_rect_sdf(p: vec2<f32>, origin: vec2<f32>, size: vec2<f32>, radii: vec4<f32>) -> f32 {
    let half_size = size * 0.5;
    let q = p - (origin + half_size);
    let r = pick_radius(q, radii);
    let dd = abs(q) - half_size + vec2<f32>(r);
    return length(max(dd, vec2<f32>(0.0))) + min(max(dd.x, dd.y), 0.0) - r;
}

fn clip_coverage(frag: vec2<f32>, clip_bounds: vec4<f32>, clip_radii: vec4<f32>) -> f32 {
    let dist = rounded_rect_sdf(frag, clip_bounds.xy, clip_bounds.zw, clip_radii);
    return saturate(0.5 - dist);
}

// --- Color helpers -----------------------------------------------------------

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

fn linear_to_srgb(c: vec3<f32>) -> vec3<f32> {
    let lo = c * 12.92;
    let hi = 1.055 * pow(max(c, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.4)) - vec3<f32>(0.055);
    return select(hi, lo, c <= vec3<f32>(0.0031308));
}

fn linear_srgb_to_oklab(c: vec3<f32>) -> vec3<f32> {
    let l = 0.4122214708 * c.r + 0.5363325363 * c.g + 0.0514459929 * c.b;
    let m = 0.2119034982 * c.r + 0.6806995451 * c.g + 0.1073969566 * c.b;
    let s = 0.0883024619 * c.r + 0.2817188376 * c.g + 0.6299787005 * c.b;
    let l_ = pow(max(l, 0.0), 1.0 / 3.0);
    let m_ = pow(max(m, 0.0), 1.0 / 3.0);
    let s_ = pow(max(s, 0.0), 1.0 / 3.0);
    return vec3<f32>(
        0.2104542553 * l_ + 0.7936177850 * m_ - 0.0040720468 * s_,
        1.9779984951 * l_ - 2.4285922050 * m_ + 0.4505937099 * s_,
        0.0259040371 * l_ + 0.7827717662 * m_ - 0.8086757660 * s_,
    );
}

fn oklab_to_linear_srgb(c: vec3<f32>) -> vec3<f32> {
    let l_ = c.x + 0.3963377774 * c.y + 0.2158037573 * c.z;
    let m_ = c.x - 0.1055613458 * c.y - 0.0638541728 * c.z;
    let s_ = c.x - 0.0894841775 * c.y - 1.2914855480 * c.z;
    let l = l_ * l_ * l_;
    let m = m_ * m_ * m_;
    let s = s_ * s_ * s_;
    return vec3<f32>(
        4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
        -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
        -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s,
    );
}

fn premultiply(c: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(c.rgb * c.a, c.a);
}

/// Evaluates a two-stop linear gradient, interpolating in Oklab for smooth,
/// perceptually even transitions without muddy midpoints.
fn gradient_color(p: vec2<f32>, bounds: vec4<f32>, angle: f32, stop0: f32, stop1: f32, c0: vec4<f32>, c1: vec4<f32>) -> vec4<f32> {
    let center = bounds.xy + bounds.zw * 0.5;
    let dir = vec2<f32>(sin(angle), -cos(angle));
    let line_len = abs(bounds.z * dir.x) + abs(bounds.w * dir.y);
    var t = dot(p - center, dir) / max(line_len, 1e-4) + 0.5;
    t = saturate((t - stop0) / max(stop1 - stop0, 1e-4));
    let lab0 = linear_srgb_to_oklab(srgb_to_linear(c0.rgb));
    let lab1 = linear_srgb_to_oklab(srgb_to_linear(c1.rgb));
    let rgb = linear_to_srgb(saturate(oklab_to_linear_srgb(mix(lab0, lab1, t))));
    return vec4<f32>(rgb, mix(c0.a, c1.a, t));
}

// --- Shadows (closed form gaussian blur of a rounded rect) -------------------
// Technique by Evan Wallace: https://madebyevan.com/shaders/fast-rounded-rectangle-shadows/

fn gaussian(x: f32, sigma: f32) -> f32 {
    return exp(-(x * x) / (2.0 * sigma * sigma)) / (2.5066282746 * sigma);
}

fn erf_vec2(x: vec2<f32>) -> vec2<f32> {
    let s = sign(x);
    let a = abs(x);
    var r = 1.0 + (0.278393 + (0.230389 + (0.000972 + 0.078108 * a) * a) * a) * a;
    r = r * r;
    return s - s / (r * r);
}

fn blur_along_x(x: f32, y: f32, sigma: f32, corner: f32, half_size: vec2<f32>) -> f32 {
    let delta = min(half_size.y - corner - abs(y), 0.0);
    let curved = half_size.x - corner + sqrt(max(0.0, corner * corner - delta * delta));
    let integral = 0.5 + 0.5 * erf_vec2((vec2<f32>(x) + vec2<f32>(-curved, curved)) * (0.70710678 / sigma));
    return integral.y - integral.x;
}

fn shadow_coverage(p: vec2<f32>, bounds: vec4<f32>, radii: vec4<f32>, sigma: f32, aa_scale: f32) -> f32 {
    if sigma < 0.05 {
        return saturate(0.5 - rounded_rect_sdf(p, bounds.xy, bounds.zw, radii) * aa_scale);
    }
    let half_size = bounds.zw * 0.5;
    let center = bounds.xy + half_size;
    let q = p - center;
    let corner = pick_radius(q, radii);
    let low = q.y - half_size.y;
    let high = q.y + half_size.y;
    let start = clamp(-3.0 * sigma, low, high);
    let end = clamp(3.0 * sigma, low, high);
    let step = (end - start) / 4.0;
    var y = start + step * 0.5;
    var alpha = 0.0;
    for (var i = 0; i < 4; i += 1) {
        alpha += blur_along_x(q.x, q.y - y, sigma, corner, half_size) * gaussian(y, sigma) * step;
        y += step;
    }
    return alpha;
}

// --- Text contrast / gamma correction ---------------------------------------
// Coverage masks are blended in gamma space; boosting contrast for light-on-
// dark text and applying DirectWrite style alpha correction keeps strokes
// crisp and even (values from the DirectWrite gamma 1.8 table).

fn color_brightness(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.30, 0.59, 0.11));
}

fn enhance_contrast(alpha: f32, k: f32) -> f32 {
    return alpha * (k + 1.0) / (alpha * k + 1.0);
}

fn text_coverage(sample: f32, color: vec3<f32>) -> f32 {
    let brightness = color_brightness(color);
    let k = 1.0 * saturate(4.0 * (0.75 - brightness));
    let contrasted = enhance_contrast(sample, k);
    let g = vec4<f32>(0.1469, -0.8911, 1.4644, -0.3234) / 4.0;
    let correction = (g.x * brightness + g.y) * contrasted + (g.z * brightness + g.w);
    return saturate(contrasted + contrasted * (1.0 - contrasted) * correction);
}

// --- Fragment ------------------------------------------------------------------

fn finish(c: vec4<f32>) -> vec4<f32> {
    if globals.linear_output > 0.5 && c.a > 0.0 {
        let straight = c.rgb / c.a;
        return vec4<f32>(srgb_to_linear(straight) * c.a, c.a);
    }
    return c;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let kind = in.kind & 0xffu;
    let flags = (in.kind >> 8u) & 0xffu;
    let layer = i32(in.kind >> 16u);
    let aa_scale = in.params.w;

    let clip = clip_coverage(in.position.xy, in.clip_bounds, in.clip_radii);
    if clip <= 0.0 {
        discard;
    }

    var color = vec4<f32>(0.0);

    if kind == KIND_QUAD {
        let outer = saturate(0.5 - rounded_rect_sdf(in.local_pos, in.bounds.xy, in.bounds.zw, in.radii) * aa_scale);
        var fill = in.color0;
        if (flags & FLAG_GRADIENT) != 0u {
            fill = gradient_color(in.local_pos, in.bounds, in.params.x, in.params.y, in.params.z, in.color0, in.color1);
        }
        var c = premultiply(fill);
        let bw = in.border_widths; // top, right, bottom, left
        if bw.x + bw.y + bw.z + bw.w > 0.0 {
            let inner_origin = in.bounds.xy + vec2<f32>(bw.w, bw.x);
            let inner_size = max(in.bounds.zw - vec2<f32>(bw.w + bw.y, bw.x + bw.z), vec2<f32>(0.0));
            let inner_radii = max(
                in.radii - vec4<f32>(max(bw.w, bw.x), max(bw.x, bw.y), max(bw.y, bw.z), max(bw.z, bw.w)),
                vec4<f32>(0.0),
            );
            let inner = saturate(0.5 - rounded_rect_sdf(in.local_pos, inner_origin, inner_size, inner_radii) * aa_scale);
            c = c * inner + premultiply(in.border_color) * (1.0 - inner);
        }
        color = c * outer;
    } else if kind == KIND_SHADOW {
        let alpha = shadow_coverage(in.local_pos, in.bounds, in.radii, in.params.x, aa_scale);
        color = premultiply(in.color0) * alpha;
    } else if kind == KIND_INSET_SHADOW {
        let shape = saturate(0.5 - rounded_rect_sdf(in.local_pos, in.bounds.xy, in.bounds.zw, in.radii) * aa_scale);
        // uv_rect holds the "hole" rectangle, border_widths its radii.
        let hole = shadow_coverage(in.local_pos, in.uv_rect, in.border_widths, in.params.x, aa_scale);
        color = premultiply(in.color0) * shape * (1.0 - hole);
    } else if kind == KIND_MONO_SPRITE {
        let sample = textureSampleLevel(mono_atlas, atlas_sampler, in.tex_coord, layer, 0.0).r;
        let coverage = text_coverage(sample, in.color0.rgb);
        color = premultiply(in.color0) * coverage;
    } else if kind == KIND_POLY_SPRITE {
        var texel = textureSampleLevel(color_atlas, atlas_sampler, in.tex_coord, layer, 0.0);
        if (flags & FLAG_GRAYSCALE) != 0u {
            let gray = color_brightness(texel.rgb);
            texel = vec4<f32>(vec3<f32>(gray), texel.a);
        }
        let shape = saturate(0.5 - rounded_rect_sdf(in.local_pos, in.bounds.xy, in.bounds.zw, in.radii) * aa_scale);
        color = texel * in.color0.a * shape;
    } else if kind == KIND_BACKDROP {
        // Blurred backdrop: uv is in screen space of the backdrop texture.
        let uv = in.position.xy / globals.viewport;
        let texel = textureSampleLevel(backdrop_texture, atlas_sampler, uv, 0.0);
        let shape = saturate(0.5 - rounded_rect_sdf(in.local_pos, in.bounds.xy, in.bounds.zw, in.radii) * aa_scale);
        let tint = premultiply(in.color0);
        color = (texel * (1.0 - tint.a) + tint) * shape * in.params.z;
    }

    return finish(color * clip);
}
