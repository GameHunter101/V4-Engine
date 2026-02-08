@group(1) @binding(0) var diffuse: texture_2d<f32>;
@group(1) @binding(1) var normal: texture_2d<f32>;
@group(1) @binding(2) var sample: sampler;

struct Scale {
    val: f32,
}
var<immediate> scale: Scale;

struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) tex_coords: vec2<f32>,
    @location(2) normal: vec3<f32>,
    @location(3) world_tangent: vec3<f32>,
    @location(4) world_bitangent: vec3<f32>,
    @location(5) world_normal: vec3<f32>,
}

struct Camera {
    mat: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    pos: vec4<f32>,
}

@group(0) @binding(0)
var<uniform> camera: Camera;

@fragment
fn main(in: VertexOutput) -> @location(0) vec4<f32> {
    let light_pos = vec3f(3.0);
    let tangent_matrix = mat3x3(
        in.world_tangent,
        in.world_bitangent,
        in.world_normal,
    );
    let tangent_light_pos = tangent_matrix * light_pos;
    let tangent_pos = tangent_matrix * in.world_pos;
    let tangent_view_pos = tangent_matrix * camera.pos.xyz;

    let color = textureSample(diffuse, sample, in.tex_coords).xyz;
    let normal = textureSample(normal, sample, in.tex_coords).xyz;

    let tangent_normal = normal * 2.0 - 1.0;
    let light_dir = normalize(tangent_light_pos - tangent_pos);
    let view_dir = normalize(tangent_view_pos - tangent_pos);
    let half_dir = normalize(light_dir + view_dir);

    let diffuse_strength = max(dot(tangent_normal, light_dir), 0.0);

    let specular_strength = pow(max(dot(tangent_normal, half_dir), 0.0), 32.0);

    let res = (color + specular_strength) * color;

    return vec4f(res, scale.val);
}
