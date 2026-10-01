// Fullscreen passes: blit, downsample and separable gaussian blur.

struct PostParams {
    // 1 / source texture size.
    texel: vec2<f32>,
    // Blur direction in texels ((1,0) or (0,1)); unused for blit/downsample.
    direction: vec2<f32>,
    sigma: f32,
    radius: f32,
    linear_output: f32,
    _pad: f32,
};

@group(0) @binding(0) var<uniform> params: PostParams;
@group(0) @binding(1) var source: texture_2d<f32>;
@group(0) @binding(2) var source_sampler: sampler;

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_fullscreen(@builtin(vertex_index) vid: u32) -> VsOut {
    // One oversized triangle covering the viewport.
    let x = f32((vid << 1u) & 2u);
    let y = f32(vid & 2u);
    var out: VsOut;
    out.position = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    out.uv = vec2<f32>(x, y);
    return out;
}

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

@fragment
fn fs_blit(in: VsOut) -> @location(0) vec4<f32> {
    let c = textureSampleLevel(source, source_sampler, in.uv, 0.0);
    if params.linear_output > 0.5 && c.a > 0.0 {
        return vec4<f32>(srgb_to_linear(c.rgb / c.a) * c.a, c.a);
    }
    return c;
}

@fragment
fn fs_downsample(in: VsOut) -> @location(0) vec4<f32> {
    // 4 bilinear taps = a 4x4 box filter, avoiding shimmering when the
    // backdrop moves.
    let o = params.texel * 0.5;
    var c = textureSampleLevel(source, source_sampler, in.uv + vec2<f32>(-o.x, -o.y), 0.0);
    c += textureSampleLevel(source, source_sampler, in.uv + vec2<f32>(o.x, -o.y), 0.0);
    c += textureSampleLevel(source, source_sampler, in.uv + vec2<f32>(-o.x, o.y), 0.0);
    c += textureSampleLevel(source, source_sampler, in.uv + vec2<f32>(o.x, o.y), 0.0);
    return c * 0.25;
}

@fragment
fn fs_blur(in: VsOut) -> @location(0) vec4<f32> {
    let sigma = max(params.sigma, 0.5);
    let radius = i32(params.radius);
    var sum = textureSampleLevel(source, source_sampler, in.uv, 0.0);
    var weight_sum = 1.0;
    // Pairs of taps sampled between texels exploit bilinear filtering to
    // evaluate two gaussian weights with one fetch.
    for (var i = 1; i <= radius; i += 2) {
        let x0 = f32(i);
        let x1 = f32(i + 1);
        let w0 = exp(-(x0 * x0) / (2.0 * sigma * sigma));
        let w1 = exp(-(x1 * x1) / (2.0 * sigma * sigma));
        let w = w0 + w1;
        let offset = (x0 * w0 + x1 * w1) / w;
        let delta = params.direction * params.texel * offset;
        sum += textureSampleLevel(source, source_sampler, in.uv + delta, 0.0) * w;
        sum += textureSampleLevel(source, source_sampler, in.uv - delta, 0.0) * w;
        weight_sum += 2.0 * w;
    }
    return sum / weight_sum;
}
