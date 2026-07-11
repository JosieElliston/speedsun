// fullscreen composite of an overlay layer onto the scene. the pipeline
// blends the emitted `strength * overlay` with One / OneMinusSrcAlpha:
//     out = strength * over + (1 - strength * over.a) * scene
// the stylized-transparency convex combination where the overlay drew
// (over.a is 1 there for the opaque flash/gizmo layers), identity elsewhere.

@group(0) @binding(0) var overlay_tex: texture_2d<f32>;
// only x is used; a vec4f to satisfy uniform sizing.
@group(0) @binding(1) var<uniform> strength: vec4f;

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> @builtin(position) vec4f {
    // fullscreen triangle
    let x = f32(i32(idx & 1u) * 4 - 1);
    let y = f32(i32(idx >> 1u) * 4 - 1);
    return vec4f(x, y, 0.0, 1.0);
}

@fragment
fn fs_main(@builtin(position) pos: vec4f) -> @location(0) vec4f {
    return strength.x * textureLoad(overlay_tex, vec2i(pos.xy), 0);
}
