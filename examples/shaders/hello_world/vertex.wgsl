struct VertexInput {
    @location(0) position: vec3<f32>,
}

struct TransformData {
    @location(1) mat_0: vec4<f32>,
    @location(2) mat_1: vec4<f32>,
    @location(3) mat_2: vec4<f32>,
    @location(4) mat_3: vec4<f32>,
}

@vertex
fn main(input: VertexInput, transform: TransformData) -> @builtin(position) vec4<f32> {
    let mat = mat4x4<f32>(
        transform.mat_0,
        transform.mat_1,
        transform.mat_2,
        transform.mat_3,
    );

    return mat * vec4f(input.position, 1.0);
}
