struct VertexInput {
    @location(0) position: vec3<f32>
}

@vertex
fn main(input: VertexInput) -> @builtin(position) vec4<f32> {
    return vec4f(input.position, 1.0);
}
