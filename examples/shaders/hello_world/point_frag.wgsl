@fragment
fn main(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    return vec4f(0.0, 1.0, 0.0, 1.0);
}

