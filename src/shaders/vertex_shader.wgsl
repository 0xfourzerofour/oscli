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

    // Check if this is a playhead vertex (marked with x < -5.0)
    let is_playhead = input.position.x < -5.0;

    var x: f32;
    if (is_playhead) {
        // Playhead stays in center of screen - offset from -10.0 to 0.0
        x = input.position.x + 10.0;
    } else {
        // Waveform scrolls and zooms
        x = (input.position.x - uniforms.scroll_offset) * uniforms.zoom * 2.0 - 1.0;
    }

    let y = input.position.y;
    out.position = vec4<f32>(x, y, 0.0, 1.0);

    // Color based on whether this is playhead or waveform
    let amplitude = abs(y);

    if (is_playhead) {
        // Playhead (line and triangle) - bright red
        out.color = vec4<f32>(1.0, 0.0, 0.0, 1.0);
    } else {
        // Vibrant gradient with brighter transients (high amplitude)
        // Low amplitude: cyan/blue, High amplitude: bright yellow/orange (transients)
        let intensity = pow(amplitude * 2.0, 1.5); // Exponential for more pop
        let r = 0.2 + intensity * 1.8;
        let g = 0.6 + intensity * 0.8;
        let b = 1.0 - intensity * 0.7;
        out.color = vec4<f32>(r, g, b, 1.0);
    }
    return out;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return input.color;
}
