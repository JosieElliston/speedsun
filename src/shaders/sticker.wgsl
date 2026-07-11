// flat-shaded stickers with edge-distance outlines: a transcription of the
// old CPU `StickerPipeline`. emits premultiplied alpha in gamma space, so
// fixed-function ops on the Rgba8Unorm target blend in gamma space exactly
// like the old Color32 math.

struct Vertex {
    @location(0) pos: vec4f,
    @location(1) face: vec4f,
    @location(2) outline: vec4f,
    // x: outline width in pixels; y, z, w: pixel-space distances to the
    // triangle's edges, huge on non-boundary edges (see `push_fan`).
    @location(3) width_edges: vec4f,
}

struct VsOut {
    @builtin(position) pos: vec4f,
    @location(0) face: vec4f,
    @location(1) outline: vec4f,
    @location(2) width_edges: vec4f,
}

@vertex
fn vs_main(v: Vertex) -> VsOut {
    return VsOut(v.pos, v.face, v.outline, v.width_edges);
}

@fragment
fn fs_main(v: VsOut) -> @location(0) vec4f {
    // pixel coverage of the outline: 1 within `width` of the nearest polygon
    // boundary edge, antialiased over one pixel.
    let width = v.width_edges.x;
    var coverage = 0.0;
    if width > 0.0 {
        let d = min(v.width_edges.y, min(v.width_edges.z, v.width_edges.w));
        coverage = clamp(width + 0.5 - d, 0.0, 1.0);
    }
    // the outline (opacity outline.a, scaled by its coverage) composited over
    // the face (opacity face.a).
    let a_out = v.outline.a * coverage;
    let a = a_out + v.face.a * (1.0 - a_out);
    let rgb = v.outline.rgb * a_out + v.face.rgb * v.face.a * (1.0 - a_out);
    return vec4f(rgb, a);
}
