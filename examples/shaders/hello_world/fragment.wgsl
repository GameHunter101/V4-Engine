var<immediate> scale: f32;

@fragment
fn main(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    // let temp = textureLoad(hdri, vec2i(0, 0), 0);
    return vec4f(scale);
}
