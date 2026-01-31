struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) pos: vec4<f32>,
}

@vertex
fn main(@location(0) position: vec3<f32>) -> VertexOut {
    var out: VertexOut;
    out.clip = vec4f(position, 1.0);
    out.pos = out.clip;
    return out;
}
