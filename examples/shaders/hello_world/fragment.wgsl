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
    let tbn = mat3x3(
        in.world_tangent,
        in.world_bitangent,
        in.world_normal,
    );

    let color = textureSample(diffuse, sample, in.tex_coords).xyz;
    let sampled_normal = textureSample(normal, sample, in.tex_coords).xyz;

    let strength = 0.2;

    let tangent_normal = (sampled_normal * 2.0 - 1.0) * vec3f(strength, strength, 1.0);
    let world_normal = normalize(tbn * tangent_normal);

    let light_dir = normalize(light_pos - in.world_pos);
    let view_dir = normalize(camera.pos.xyz - in.world_pos);
    let half_dir = normalize(light_dir + view_dir);

    let diffuse_strength = max(dot(world_normal, light_dir), 0.0);

    let specular_strength = pow(max(dot(world_normal, half_dir), 0.0), 20.0);

    let res = vec3f(specular_strength) + color * dot(world_normal, normalize(light_pos - in.world_pos));

    return vec4f(world_normal, scale.val);
}
