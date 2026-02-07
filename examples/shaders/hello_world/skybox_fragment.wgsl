@group(1) @binding(0) var env_map: texture_cube<f32>;
@group(1) @binding(1) var env_sampler: sampler;

struct Camera {
    view_proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    pos: vec4<f32>,
}
@group(0) @binding(0) var<uniform> camera: Camera;

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) pos: vec4<f32>,
}

@fragment
fn main(in: VertexOut) -> @location(0) vec4<f32> {
    let t = camera.inv_view_proj * in.pos;
    return textureSample(env_map, env_sampler, normalize(t.xyz / t.w) * vec3f(1.0, 1.0, -1.0));
}
