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
    let dist = 2.0 / 600.0;
    let top = textureSample(input_tex, input_sampler, input.tex_coords + vec2f(0.0, dist)).xyz;
    let bot = textureSample(input_tex, input_sampler, input.tex_coords - vec2f(0.0, dist)).xyz;
    let right = textureSample(input_tex, input_sampler, input.tex_coords + vec2f(dist, 0.0)).xyz;
    let left = textureSample(input_tex, input_sampler, input.tex_coords - vec2f(dist, 0.0)).xyz;
    return vec4f((top + bot + left + right) / 4.0, 1.0);
}
