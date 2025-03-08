@group(0) @binding(0) var<storage, read> input: array<f32, 8>;
@group(1) @binding(0) var<storage, read_write> output: array<f32, 8>;

@compute
@workgroup_size(1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    output[id.x] = input[id.x] *2.0;
}
