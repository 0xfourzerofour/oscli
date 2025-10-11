@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

struct Uniforms {
    zoom: f32,
    scroll_offset: f32,
    playhead_pos: f32,
    _padding: f32,
}

struct VertexInput {
    @location(0) position: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
}

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let x = (input.position.x - uniforms.scroll_offset) * uniforms.zoom * 2.0 - 1.0;
    let y = input.position.y;
    out.position = vec4<f32>(x, y, 0.0, 1.0);

    // Color waveform green, playhead red
    if (abs(input.position.x - uniforms.playhead_pos) < 0.001) {
        out.color = vec4<f32>(1.0, 0.0, 0.0, 1.0); // Red for playhead
    } else {
        out.color = vec4<f32>(0.0, 1.0, 0.0, 1.0); // Green for waveform
    }
    return out;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return input.color;
}
