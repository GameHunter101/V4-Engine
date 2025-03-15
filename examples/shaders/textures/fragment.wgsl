@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var sample: sampler;


struct VertexOutput {
    @builtin(position) pos: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
}

@fragment
fn main(input: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(tex, sample, input.tex_coords);
}
