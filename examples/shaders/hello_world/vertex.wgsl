struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) tex_coords: vec2<f32>,
    @location(2) normal: vec3<f32>,
    @location(3) tangent: vec3<f32>,
    @location(4) bitangent: vec3<f32>,
}

struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) tex_coords: vec2<f32>,
    @location(2) normal: vec3<f32>,
    @location(3) world_tangent: vec3<f32>,
    @location(4) world_bitangent: vec3<f32>,
    @location(5) world_normal: vec3<f32>
}

struct TransformData {
    @location(5) mat_0: vec4<f32>,
    @location(6) mat_1: vec4<f32>,
    @location(7) mat_2: vec4<f32>,
    @location(8) mat_3: vec4<f32>,
}

struct Camera {
    mat: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    pos: vec4<f32>,
}

@group(0) @binding(0)
var<uniform> camera: Camera;

@vertex
fn main(input: VertexInput, transform: TransformData) -> VertexOutput {
    let mat = mat4x4<f32>(
        transform.mat_0,
        transform.mat_1,
        transform.mat_2,
        transform.mat_3,
    );

    var out: VertexOutput;
    let world_pos = mat * vec4f(input.position, 1.0);
    out.clip_pos = camera.mat * world_pos;
    out.world_pos = world_pos.xyz;
    out.tex_coords = input.tex_coords;
    out.normal = input.normal;
    out.world_tangent = normalize((mat * vec4f(input.tangent, 1.0)).xyz);
    out.world_bitangent = normalize((mat * vec4f(input.bitangent, 1.0)).xyz);
    out.world_normal = normalize((mat * vec4f(input.normal , 1.0)).xyz);

    return out;
}
