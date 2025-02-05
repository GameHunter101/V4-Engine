struct VertexOutput {
    @builtin(position) pos: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
}

@group(0) @binding(0)
var input_tex: texture_2d<f32>;

@group(0) @binding(1)
var input_sampler: sampler;

@fragment
fn main(input: VertexOutput) -> @location(0) vec4<f32> {
    let input_color = textureSample(input_tex, input_sampler, input.tex_coords).xyz;
    let overlay_alpha = 0.3;
    let overlay_color = vec3f(0.0, 1.0, 0.3);
    let overlay = overlay_alpha * overlay_color + (1.0 - overlay_alpha) * input_color;
    return vec4f(overlay, 1.0);
}
