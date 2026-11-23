// Resolved world to the output: nearest, sharp bilinear or area-weighted.

struct Params {
    scale: vec2<f32>,
    origin: vec2<f32>,
    mode: u32,
    decode_srgb: u32,
    size: vec2<u32>,
}

const NEAREST: u32 = 0u;
const SHARP: u32 = 1u;
const AREA: u32 = 2u;
const AREA_TAPS: i32 = 5;

@group(0) @binding(0) var image: texture_2d<f32>;
@group(0) @binding(1) var<uniform> params: Params;

@vertex
fn vs_main(@builtin(vertex_index) vertex: u32) -> @builtin(position) vec4<f32> {
    // One triangle over the whole target: vertices 0, 1 and 2 land at (-1, -1), (3, -1) and (-1, 3).
    let uv = vec2<f32>(f32((vertex << 1u) & 2u), f32(vertex & 2u));
    return vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
}

// The image's size comes with the parameters. Direct3D 12's software renderer answers
// textureDimensions with zero for a pixel whose left or upper neighbour a scissor rectangle
// leaves out, and the clamp then reads outside the image.
fn load(texel: vec2<i32>) -> vec4<f32> {
    let size = vec2<i32>(params.size);
    return textureLoad(image, clamp(texel, vec2<i32>(0), size - 1), 0);
}

// Reads the four texels round the sample point and mixes them, which gives every GPU the same
// weights.
fn sharp(centre: vec2<f32>) -> vec4<f32> {
    let texel = centre / params.scale;
    let prescale = max(floor(params.scale), vec2<f32>(1.0));
    let region = 0.5 - 0.5 / prescale;
    let from_centre = fract(texel) - 0.5;
    let f = (from_centre - clamp(from_centre, -region, region)) * prescale + 0.5;
    let point = floor(texel) + f - 0.5;
    let first = vec2<i32>(floor(point));
    let weight = fract(point);
    let top = mix(load(first), load(first + vec2<i32>(1, 0)), weight.x);
    let bottom = mix(load(first + vec2<i32>(0, 1)), load(first + vec2<i32>(1, 1)), weight.x);
    return mix(top, bottom, weight.y);
}

fn area(corner: vec2<f32>) -> vec4<f32> {
    let lo = corner / params.scale;
    let hi = (corner + 1.0) / params.scale;
    let first = vec2<i32>(floor(lo));
    var sum = vec4<f32>(0.0);
    var weight = 0.0;
    for (var dy = 0; dy < AREA_TAPS; dy++) {
        for (var dx = 0; dx < AREA_TAPS; dx++) {
            let texel = first + vec2<i32>(dx, dy);
            let cell = vec2<f32>(texel);
            let overlap = max(min(hi, cell + 1.0) - max(lo, cell), vec2<f32>(0.0));
            let w = overlap.x * overlap.y;
            if w > 0.0 {
                sum += load(texel) * w;
                weight += w;
            }
        }
    }
    return sum / max(weight, 1e-6);
}

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let low = c / 12.92;
    let high = pow((c + 0.055) / 1.055, vec3<f32>(2.4));
    return select(high, low, c <= vec3<f32>(0.04045));
}

@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let at = position.xy - params.origin;
    var colour: vec4<f32>;
    switch params.mode {
        case SHARP: {
            colour = sharp(at);
        }
        case AREA: {
            colour = area(floor(at));
        }
        default: {
            colour = load(vec2<i32>(floor(at / params.scale)));
        }
    }
    if params.decode_srgb != 0u {
        colour = vec4<f32>(srgb_to_linear(colour.rgb), colour.a);
    }
    return colour;
}
