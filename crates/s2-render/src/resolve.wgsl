// Indexed frame to colour through the palette.

@group(0) @binding(0) var world: texture_2d<u32>;
@group(0) @binding(1) var palette: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) vertex: u32) -> @builtin(position) vec4<f32> {
    // One triangle over the whole target: vertices 0, 1 and 2 land at (-1, -1), (3, -1) and (-1, 3).
    let uv = vec2<f32>(f32((vertex << 1u) & 2u), f32(vertex & 2u));
    return vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
}

@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let index = textureLoad(world, vec2<i32>(position.xy), 0).r;
    return textureLoad(palette, vec2<i32>(i32(index), 0), 0);
}
